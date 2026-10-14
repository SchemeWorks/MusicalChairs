# Native Solana decision register — 2026-10-13 planning pass

Date: 2026-10-13. Author: Claude (planning coordinator), taking over from Codex. Status: **recommendations for Codex review and Rob's decision; nothing here authorizes implementation, deployment, funding or signing.**

Companions: [current-state inventory](native-solana-state-inventory-20261013.md), [reconciled specification](../superpowers/specs/2026-10-13-native-solana-reconciled-spec.md), [implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md), [Codex review packet](native-solana-codex-review-packet-20261013.md). Accepted policy is recorded in [launch decisions](../solana/launch-decisions.md) and [defaults](native-solana-defaults.md); this register does not reopen D1–D9.

## How to read this register

Each entry is classified as exactly one of:

- **ROUTINE** — implementation detail the implementer may decide; recorded so Codex can object before code exists.
- **ROB** — user-visible policy or product behavior that Rob must confirm; a recommended default is given.
- **MEASURE** — resolved by a measurement that the plan schedules; the recommended default is the fallback if measurement is inconclusive.
- **LAUNCH** — a concrete identity, amount or endpoint supplied at launch preparation, not now.
- **DEFERRED** — optional scope explicitly outside the first release; listed so no one treats it as a gate.

An entry marked **DEVIATION** proposes a change to an accepted invariant and says so in its first line. There is one such entry (DR-14) and it is a clarification of wording, not a change of behavior.

Solana notes are attached where a Solana-specific mechanism drove the recommendation. Rob's background is ICP (canisters, stable memory, management-canister threshold signing) plus Solidity; the notes compare to that.

---

## A. Account and storage contract

### DR-01 Reuse the uncommitted draft account seeds and names — ROUTINE

**Recommendation:** adopt the seed set in the uncommitted `docs/solana/production-accounts.md` / `state.rs` draft as the base contract: `[pool]`, `[admission]`, `[game-vault]`, `[ops]`, `[ops-cash]`, `[ticket]`, `[user, owner]`, `[user-round, round_le_u64, owner]`, `[position, owner, nonce_le_u64]`, `[backer, series_u8, owner]`, `[registry, page_le_u64]`, `[snapshot, page_le_u64]`. Add the accounts the draft omits (DR-02, DR-03, DR-04, DR-06). Rationale: the seeds match the accepted spec's candidate map and the ownership/width contract; renaming would cost review time without changing behavior. The draft's correctness is **not** assumed; every handler is re-derived in the plan with failing tests first.

**Alternatives:** per-round registry seeds (rejected: registry is global across rounds by accepted design); owner-first seed order for Position (immaterial).

Solana note: a PDA (program-derived address) is an address with no private key, derived from seeds plus the program ID. It plays the role an ICP canister's internal `HashMap` key plays, except each PDA is a separate rent-paying account that must be passed into every transaction that reads or writes it. That is why the seed list is a public contract and why "which accounts does this instruction touch" is part of every instruction's specification below.

### DR-02 Per-close retained `CloseRecord` instead of close fields inside `Pool` — ROUTINE

**Recommendation:** keep the current-round scalars in `Pool`, but create one retained `CloseRecord` PDA `[close, close_id_le_u64]` per accepted close. It pins `close_started_at`, `promotion_deadline`, the sealed candidate digests/lengths/set tag, the provider request binding, the terminal promotion result and the finalize marker. The draft stores closing time/deadline in `Pool` only, which cannot satisfy the accepted rule that terminal close/request bindings are retained so an old callback cannot attach to a later round.

**Provisioning:** `CloseRecord` for `pool.next_close_id` must already exist (sponsor-paid, permissionless `provision_close_record`) before any payout that *could* close is accepted; the payout handler writes into it only on insolvency. This mirrors the draft's pre-provisioned snapshot pages and keeps the closing payout a single transaction.

**Alternatives:** create `CloseRecord` inside the closing payout with `init` (rejected: Anchor `init` requires an unconditional create, and the payout only sometimes closes); reuse one close account (rejected: loses retained binding history). Budgeted in the ICP-budget doc as "retained close receipt 512 bytes".

### DR-03 Candidate and participant pages store owner pubkeys only — ROUTINE

**Recommendation:** `ParticipantPage` `[participants, round_le_u64, page_le_u64]` rows are 32-byte owner pubkeys appended on first game open in the round. `CandidatePage` `[candidates, close_id_le_u64, set_tag_u8, page_le_u64]` rows are 32-byte owner pubkeys. Loss and A/B history are re-read from the owner's `UserRound` and `User` PDAs during candidate traversal and again at promotion apply (both immutable once the round is sealed). Candidate pages are abandoned/closable after terminal promotion; participant pages after finalization.

**Rationale:** smallest rows, no duplicated P/L state that could drift, and the budget doc's 96-byte-row assumption becomes an over-estimate rather than an under-estimate.

### DR-04 Versioned configuration as separate `ConfigVersion` PDAs — ROUTINE

**Recommendation:** `ConfigVersion` `[config, version_le_u32]` holds plan rates/durations/fee bps/minima/toll split/quota constants/close reserve amount/provider tuple and `effective_round_id`. `Pool.active_config_version` is the only one admissions read. G stages a new version (G-delayed); permissionless `activate_config_version(version)` succeeds only when `pool.round_id >= effective_round_id`, the round is `Active`, no ticket is active and the caller passes the expected current version. `Position` pins `config_version` at open; `CloseRecord` pins the version at close; `Ticket` pins the toll-split constants at capture. This is how "future schedule cannot rewrite pinned liabilities" is enforced mechanically rather than by promise.

### DR-05 Width contract and serialization — ROUTINE

**Recommendation:** u64 for every cash partition and transferable amount; u128 for owed earnings, entitlements, lifetime credited, UserRound gross/net, global funded claims; U256 (ethnum, 32-byte little-endian in accounts) for the sealed A denominator and pro-rata products. Slot/cursor/count fields are u64. Discriminators are Anchor's 8-byte defaults. Any draft field narrower than this table is a defect to fix, not a width decision.

### DR-06 Operations cash as a system-owned PDA paying rent by `invoke_signed` — ROUTINE

**Recommendation:** keep the draft's `[ops-cash]` system-owned, zero-data PDA as the lamport holder for sponsor funds, and the program-owned `[ops]` account as its ledger (free balance, locked partitions, reserved amounts, protected rent). Shared-page creation uses a manual `system_program::create_account` CPI signed with the `ops-cash` seeds; Anchor's `init, payer =` shortcut is not used for sponsor-paid accounts because it requires a client-side `Signer`.

Solana note: on ICP a canister pays its own cycles implicitly. On Solana every byte of state must be pre-funded with lamports by *some* signer in the transaction. A PDA can be that payer only if the program signs for it in a cross-program call; that is the `invoke_signed` path above, and it is the reason the sponsor's SOL must sit in an account the program controls.

---

## B. Payouts, allocation, claims

### DR-07 Singleton reusable `Ticket` workspace — ROUTINE

**Recommendation:** one `[ticket]` account reused across tolls, as the draft does, because the accepted barrier already serializes allocations (no second payout while a ticket is active). `Ticket.ticket_id` increments per use and every snapshot row and pending Backer credit carries the `ticket_id` it belongs to, so a stale snapshot page can never be read as current. Closing tolls use the same workspace.

**Alternative:** one `Ticket` PDA per payout (rejected: rent churn for no isolation benefit while the barrier exists).

### DR-08 Pre-provisioned reusable `SnapshotPage`s, provisioning permissionless and sponsor-paid — ROUTINE (interacts with DR-12)

**Recommendation:** keep the draft path: `provision_snapshot_page(page)` is permissionless and paid from reserved operations cash; a payout verifies `pool.snapshot_pages * 8 >= captured_active_len` before accepting cash. Pages are overwritten only by a ticket whose `ticket_id` is greater than the page's recorded one and only after the prior ticket is terminal. This is the first-release storage path; it does not preclude later per-ticket pages.

### DR-09 Owner work contribution in the ABI from day one, default zero — ROUTINE (accepted policy, draft deferred it)

**Recommendation:** `withdraw_simple`, `settle_compounding` and `close_simple_position` take `max_work_contribution_lamports: u64` now. First release implements sponsor-first consumption and the owner partition exactly as accepted (locked partitions on `Ticket`/`CloseRecord`, immutable owner refund destination, once-only refund). The GUI exposes the field as an advanced input defaulting to 0. Rationale: the accepted defaults already specify this contract; the draft's deferral was a tranche boundary. Adding the argument later would change the instruction ABI after clients exist.

### DR-10 Combined A+B claim takes both expected Backer accounts, each optional by existence flag — ROUTINE

**Recommendation:** `claim_backer(destination)` requires the owner's `User` and, for each of A and B where `User.has_ever_A/B` is set, the corresponding `Backer` PDA; a missing required account fails. Lazy materialization of finalized pending credit happens inside the claim for each present Backer before transfer. Claim during an active ticket pays only previously finalized credit.

### DR-11 Events via Anchor `emit_cpi!` (self-CPI) for every financial event — ROUTINE (affects ICP bridge decoding)

**Recommendation:** use `emit_cpi!` rather than log-based `emit!` for `PositionOpened`, `SeriesAFunded`, `PayoutPaid`, `TicketCommitted`, `BackerClaimed`, `RoundClosed`, `PromotionApplied`, `PromotionSkipped`, `RoundFinalized`. Event data then lives in an inner-instruction's data, which RPC returns intact under `maxSupportedTransactionVersion`, whereas program logs are truncatable and can be omitted by RPC providers. The ICP ingestion identity tuple (cluster, program, signature, instruction index, inner-instruction index, event schema version) is defined over that representation.

Solana note: Solana has no canister-to-canister callback or certified query. Events are not stored on chain; they are observed from transaction metadata. The self-CPI pattern makes the event part of the transaction's executed instruction list instead of a log line, which is the closest thing Solana has to a durable, decodable event.

---

## C. Resource locking, refunds and cleanup

### DR-12 Mandatory reserve check order and amounts — ROUTINE, amounts LAUNCH

**Recommendation:** before any payout mutates state, the handler checks in this order: (1) no active ticket; (2) snapshot pages provisioned for captured N (DR-08); (3) if the payout would close: `CloseRecord` for `next_close_id` exists, and `ops.free_balance + owner_contribution >= config.close_reserve_lamports`, then moves exactly that amount into `locked_sponsor` / `locked_owner` on the `CloseRecord`; (4) toll > 0 implies ticket work; captured work units recorded. Only then is cash moved. `close_reserve_lamports` (covering provider request fee + pending request rent + one possible automatic B account + candidate-page rent for the captured participant count) is a `ConfigVersion` field supplied at launch from measured sizes.

### DR-13 Beneficial rent attribution with at most two components — ROUTINE

**Recommendation:** every sponsor-created shared account stores `rent_funded_by_ops: u64` and `rent_funded_by_owner: u64` (owner component nonzero only for an opted-in shortfall). `close_shared_account(kind, id)` is permissionless after the account's safe-closure predicate holds (ticket terminal for snapshot pages; promotion terminal for candidate pages; round finalized for participant pages; never for `CloseRecord`, `Backer`, `User`) and returns each component to its funder exactly once (ops free balance; recorded owner refund destination). Account cleanup is never a precondition for cash commit, finalization or reopening.

### DR-14 Refund of unused locked reserve happens at terminal promotion, not at cleanup — ROUTINE (DEVIATION: wording clarification only)

The accepted text says "refund unused funds exactly once after required work/cleanup" and separately that cleanup must not block cash. **Recommendation:** split the two timers explicitly: the *locked reserve* on a `CloseRecord` is settled once when promotion becomes terminal (consumed provider fee and any B-account rent are deducted; the remainder returns to ops free balance / owner destination); *rent components* on shared accounts return at safe closure under DR-13. No lamport is refunded twice because the two pools are disjoint. This is presented as a deviation only because the accepted sentence can be read as a single refund event; behavior and conservation are unchanged.

### DR-15 Unsolicited SOL — ROUTINE (accepted, no change)

Plain transfers to `[game-vault]` raise `unallocated_excess` at the next instruction that reconciles physical lamports; no instruction can spend, credit or sweep it in first release. `fund_operations` is the only sanctioned way to give the project SOL.

---

## D. Round close, candidates, randomness

### DR-16 Participant traversal completeness proof — ROUTINE

**Recommendation:** `CloseRecord.participant_count` is copied from `Pool.participant_count` at close acceptance; `build_candidates(page, rows)` must visit participant slots in strictly increasing order with an exact cursor, re-deriving each owner's `UserRound` and `User` PDAs from the stored pubkey and verifying them; sealing requires `cursor == participant_count`. Every participant is classified exactly once as preferred, fallback-only, or not-a-loser. The sealed digest is SHA-256 over the ordered candidate pubkeys per set.

### DR-17 ORAO Callback 0.4.0, no client-level callback, direct capture — ROUTINE (accepted candidate)

**Recommendation:** register one ORAO Client owned by the program's `[rng-authority]` PDA (the "State" signer in the SDK); `request_round_draw(close_id)` CPI-requests with the protocol seed; `capture_round_draw(close_id)` reads the fulfilled `RequestAccount` directly and requires `Clock <= deadline`. The runtime probes executed this lifecycle locally; what remains is devnet service latency and mainnet quorum. A request-level callback is not used.

### DR-18 Keep `expire_promotion` separate from `finalize_round`, and allow a composed client transaction — ROUTINE (accepted)

Composition is at the transaction level (three instructions: `expire_promotion`, `finalize_round`, `open_position`) so each instruction keeps one responsibility and the composed path needs no new program logic. `open_position` takes `expected_round_id` and fails if the round differs.

---

## E. Governance and operations

### DR-19 Initialize against `ProgramData.upgrade_authority` — ROUTINE (accepted), with two draft fixes

`initialize` passes the executable `Program` and its `ProgramData` accounts and requires `ProgramData.upgrade_authority == G vault PDA`, and that the G vault signs (Squads vault CPI). No first-caller enrollment.

**Draft fixes required before the control tests mean anything** (inventory §5.2 #1–#2): the Squads `Multisig` discriminator constant in `controls.rs:438` has a wrong last byte (`0xc8`; correct `0xec`), and `config_authority` is parsed as `Option<Pubkey>` where Squads v4 stores a plain `Pubkey` (`Pubkey::default()` = autonomous, confirmed from public source on 2026-10-13). The plan's governance task rebuilds the parser from the pinned Squads source and tests it against a byte-exact fixture derived from that source, not from the parser itself.

Solana note: unlike a canister whose controllers are read from the canister's own settings, a Solana program's code lives in a `ProgramData` account controlled by the BPF upgradeable loader, and only its `upgrade_authority` can replace code. The initialize check ties application governance to that same key so there is exactly one party that can change code *and* config.

### DR-20 Guardian pause instruction touches `[admission]` and the P vault signer only — ROUTINE (accepted)

Tests assert the instruction's account list contains no `Pool`, `Ticket`, `CloseRecord`, vault, Backer or provider account.

### DR-21 Keeper hosting: external fee-only primary + ICP watchdog — MEASURE (default = hybrid)

Default stands until the plan's measurement milestone yields full-path K (transactions per month) and CU/packet data for N≈100. If measured K at the chosen chunk size makes ICP signing cost acceptable to Rob, fully ICP becomes available; the decision is Rob's at that point and is not a gate for earlier milestones.

### DR-26 Stay on Anchor 0.32.1 / SDK 2.3.1 / LiteSVM 0.7.1 / Agave 2.3.13 — ROUTINE

Public check on 2026-10-13: 0.32.1 is published and not yanked; 0.32.2 (2026-09-14) is a patch on the same Solana 2.x line; 1.2.1 is the latest stable on Solana 3.x; 2.0.0-rc.2 is a prerelease; LiteSVM is at 0.18.0 on Agave 4.3 while 0.7.1 remains published. The accepted tuple stays. If a Task 0 probe needs an Anchor patch, 0.32.2 is the only permitted move without a new decision. The Foundation skill's "Kit 8 / transaction v1 by default" guidance is reference, not instruction.

### DR-27 Rent figures are dated scenarios under SIMD-0437 — MEASURE / LAUNCH

Solana rent is being cut in five feature-gated steps (6,960 → 6,333 live 2026-09-03 → 5,080 → 2,575 → 1,322 → 696 lamports/byte; steps 3–5 expected with Agave 4.4, Nov 2026). The budget rebuild (plan Task 11) records the active step at measurement time and recomputes at launch; `close_reserve_lamports` and operations funding are launch inputs, not constants. No rent coefficient is hard-coded in the program; the program reads `Rent::get()`.

Solana note: unlike ICP cycles, which are consumed, Solana rent is a refundable deposit; a rent cut lowers the capital locked in shared pages, not a running cost. It does not change any game rule.

### DR-28 Keeper transaction format — MEASURE (default = legacy/v0)

v1 transactions (4,096 bytes) are live on mainnet since Agave 4.2; the 96-account limit is not yet active. The GUI keeps legacy/v0 (accepted). A keeper may adopt v1 or address-lookup tables only after Task 11 measures the account-lock bound, which v1 does not relax today, and only without changing program economics or cursor proofs. The ICP bridge decoder must handle observed v1 transactions regardless (accepted).

---

## F. Frontend and ICP services

### DR-22 Native rail is the default SOL view; legacy Chain Fusion positions appear in a labeled secondary section — ROB

**Recommendation:** within the existing SOL panels, show native positions/Backer/claims by default and render the legacy ICP-observed positions in a clearly labeled "Legacy (Chain Fusion) positions" section with their existing withdrawal/claim actions unchanged. Layout, styling and navigation are untouched. Alternative: a toggle between rails (more clicks; hides legacy balances from users who still have them). This is the one GUI arrangement decision that changes what users see first.

### DR-23 Transaction confirmation via HTTPS polling with `confirmed` for progress UI and `finalized` for authoritative state — ROUTINE (accepted)

No WebSocket. Simulation before wallet prompt. Signature retained across unknown outcomes; a client nonce on `open_position` makes retries idempotent.

### DR-24 ICP ingestion canister is a new `native_sol_bridge` canister, not a modification of `shenanigans` observer loops — ROUTINE

**Recommendation:** add one new canister that owns the relayer ingress, two-RPC finalized verification, outbox, cursor and PP-reward legs, and calls the existing PP ledger with the exactly-once contract. `shenanigans` is touched only to expose/whitelist the bridge as an authorized reward source for the SOL plan rates, leaving its legacy observers intact. Rationale: keeps the legacy rail's behavior unchanged and gives the new trust boundary its own upgrade/controller story. Alternative: extend `shenanigans` (rejected: couples a new RPC trust boundary to the PP/spell canister's upgrade cycle).

### DR-25 ICP mirror for history/feeds is served from the same bridge canister's durable event store — ROUTINE

No separate indexer canister in first release; the bridge already stores verified finalized events. The frontend reads live financial state from Solana and *only* history/feed enrichment from ICP.

---

## G. Launch inputs (not decisions now)

| Input | Who supplies | Where it lands |
| --- | --- | --- |
| G and P Squads vault addresses and their three independent voters each | Rob | `initialize` arguments; launch manifest |
| Management recipient, sponsor recipient | Rob | `initialize` arguments |
| Program ID (deploy keypair), cluster, genesis `cluster_domain_32` | Operator at deploy | `initialize`; seed derivation |
| ORAO network state, treasury, registered Client address | Operator after registration | `ConfigVersion` provider tuple |
| `close_reserve_lamports`, snapshot pages to pre-provision, ops funding amount | Rob from measured sizes | `ConfigVersion`, `fund_operations` |
| Production RPC endpoints (primary + fallback), CSP `connect-src` origins | Rob/operator | frontend constants, `.ic-assets.json` |
| Bootstrap pot/seed | fixed at zero unless Rob funds externally with a named receipt | none |

---

## H. Deferred optional scope (not prerequisites)

Native PP/SPL and native desk; proof-bearing refund of anonymous excess; paid on-chain keeper rewards; frontend v1 transaction / Kit migration; Pinocchio or custom allocation accumulators; additional governance signers, custom timelock wrapper or renounced upgrades; ICP `raw_rand` as an alternative provider; II/SIWS account merging; legacy deposit pause or retirement.

---

## I. Explicit non-decisions

The following were considered and are **not** reopened: strict `owed > pot` close; 35/45/20 split with folding/sealed denominator; gross accrual; three-successes-per-rolling-hour quota with no dynamic cap; 600-second immutable deadline with no reroll; zero keeper reward; public A on; operator eligibility; separate legacy rail; PP on ICP. Any plan task that appears to touch one of these is implementing it, not changing it.
