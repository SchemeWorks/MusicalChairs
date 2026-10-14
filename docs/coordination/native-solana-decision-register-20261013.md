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

**Revision 2026-10-13b:** after [Codex's review](native-solana-codex-plan-review-20261013.md), DR-08, DR-11, DR-12, DR-13, DR-14, DR-17, DR-20, DR-23 and DR-24 were amended and DR-29–DR-32 added. Each amended entry starts with "Revised:". Nothing here reopens D1–D9.

**Revision 2026-10-13d:** after [Codex's final corrections](native-solana-codex-plan-final-corrections-20261013.md), DR-32, DR-33 and DR-34 were amended (authenticated provider withdrawal wrapper with per-close eligible-return accounting, O(1) lock aggregate and deficit equation, two-level PP status and retry/receipt exactness). **Revision 2026-10-13c:** after [Codex's round-2 review](native-solana-codex-plan-review-round2-20261013.md), DR-04, DR-09, DR-12, DR-14, DR-22 and DR-32 were amended and DR-33–DR-35 added. Rob delegated plan settlement and local implementation authorization to Codex; DR-22 is therefore recorded as accepted.

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

**Revised (C6):** `ConfigVersion` `[config, version_le_u32]` holds plan rates/durations/fee bps/minima/toll split/quota constants, the **close cost-schedule inputs** (`provider_request_fee_lamports`, `provider_fee_headroom_bps`, request account sizes; there is no flat close reserve amount, see DR-12), provider tuple and `effective_round_id`. `Pool.active_config_version` is the only one admissions read. G stages a new version (G-delayed); permissionless `activate_config_version(version)` succeeds only when `pool.round_id >= effective_round_id`, the round is `Active`, no ticket is active and the caller passes the expected current version. `Position` pins `config_version` at open; `CloseRecord` pins the version at close; `Ticket` pins the toll-split constants at capture. This is how "future schedule cannot rewrite pinned liabilities" is enforced mechanically rather than by promise.

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

### DR-08 Pre-provisioned reusable `SnapshotPage`s are permanent; provisioning permissionless and sponsor-paid — ROUTINE (interacts with DR-12)

**Revised (R3):** keep the draft path: `provision_page(Snapshot, index)` is permissionless and paid from operations cash; a payout verifies `pool.snapshot_pages * 8 >= registry_len` before accepting cash. Pages are overwritten only by a ticket whose `ticket_id` is greater than the page's recorded one and only after the prior ticket is terminal. **Snapshot and registry pages are never closable in the first release**, so `snapshot_pages` is monotonic and the capacity check is a live invariant; the earlier text that allowed closing a terminal snapshot page is withdrawn because a closed middle page would leave a count that admits a payout Discover cannot serve. Reclaiming this rent needs a future reviewed upgrade with a contiguous-prefix scheme.

### DR-09 Owner work contribution in the ABI from day one, default zero — ROUTINE (accepted policy, draft deferred it)

**Revised (C6):** `withdraw_simple` and `settle_compounding` take `max_work_contribution_lamports: u64` now; `close_simple_position` does **not**, because it moves no cash, creates no ticket and can never close a round. First release implements sponsor-first consumption and the owner partition exactly as accepted (locked partitions on `Ticket`/`CloseRecord`, immutable owner refund destination, once-only refund). The GUI exposes the field as an advanced input defaulting to 0. Rationale: the accepted defaults already specify this contract; the draft's deferral was a tranche boundary. Adding the argument later would change the instruction ABI after clients exist.

### DR-10 Combined A+B claim takes both expected Backer accounts, each optional by existence flag — ROUTINE

**Recommendation:** `claim_backer(destination)` requires the owner's `User` and, for each of A and B where `User.has_ever_A/B` is set, the corresponding `Backer` PDA; a missing required account fails. Lazy materialization of finalized pending credit happens inside the claim for each present Backer before transfer. Claim during an active ticket pays only previously finalized credit.

### DR-11 Events via Anchor `emit_cpi!` (self-CPI) for every financial event — ROUTINE (affects ICP bridge decoding)

**Revised (mechanics):** use `emit_cpi!` rather than log-based `emit!` for `PositionOpened`, `SeriesAFunded`, `PayoutPaid`, `TicketCommitted`, `BackerClaimed`, `RoundClosed`, `PromotionApplied`, `PromotionSkipped`, `RoundFinalized`. This requires the `event-cpi` Cargo feature, `#[event_cpi]` on every emitting `Accounts` struct (adding the `event_authority` PDA and program account to each account list), IDL/client fixtures for each event, a decoding test over an executed self-CPI, and inclusion of the extra accounts and CU in M9 packet measurements. Event data then lives in an inner-instruction's data, which RPC returns intact under `maxSupportedTransactionVersion`, whereas program logs are truncatable and can be omitted by RPC providers. The ICP ingestion identity tuple (cluster, program, signature, instruction index, inner-instruction index, event schema version) is defined over that representation.

Solana note: Solana has no canister-to-canister callback or certified query. Events are not stored on chain; they are observed from transaction metadata. The self-CPI pattern makes the event part of the transaction's executed instruction list instead of a log line, which is the closest thing Solana has to a durable, decodable event.

---

## C. Resource locking, refunds and cleanup

### DR-12 Mandatory reserve check order; the closing reserve is computed from captured work — ROUTINE, schedule values LAUNCH

**Revised (R2):** before any payout mutates state, the handler checks in this order: (1) no active ticket; (2) snapshot pages cover `registry_len` (DR-08); (3) if the payout would close: `CloseRecord` for `next_close_id` exists, and `required_close_reserve` (spec §8.1: for **each** candidate set `max(required_pages_per_set − existing_<set>_pages, 0)` pages in u64, a registry extension page when the tail is full, one automatic B account, pending-request rent, and the provider fee with a headroom margin) is fully lockable sponsor-first then owner up to `max_work_contribution_lamports`; the exact obligation is written to `CloseRecord.required_reserve` with the schedule snapshot and per-set counts; candidate pages are then created from the locked partitions by `provision_page(Candidate{close_id,set}, index, LockedClose)` before `build_candidates` appends (C1); (4) toll > 0 implies ticket work; captured work units recorded. Only then is cash moved. The earlier single `close_reserve_lamports` launch value is withdrawn: a flat allowance either caps population silently or under-reserves. `ConfigVersion` now carries only the schedule inputs (`provider_request_fee_lamports`, `provider_fee_headroom_bps`, request account sizes); account rent comes from `Rent::get()` and program `LEN` constants. If the live provider fee exceeds `approved_provider_fee_cap`, `request_round_draw` fails with `ProviderFeeExceeded` and anyone may `increase_provider_budget(close_id, extra)` before the deadline from explicitly accounted free sponsor funds (DR-33); otherwise expiry proceeds with that diagnostic stage.

### DR-13 Beneficial rent attribution with at most two components — ROUTINE

**Revised (R3):** every sponsor-created shared account stores `rent_funded_by_ops: u64` and `rent_funded_by_owner: u64` (owner component nonzero only for an opted-in shortfall, and only on program-owned storage). `close_shared_account(kind)` is permissionless **only** for `CandidatePage{close_id, set}` after `CloseRecord(close_id).promotion` is terminal and for `ParticipantPage{round}` after that round's `CloseRecord.finalized`; both predicates derive from the page's own seeds, never from a reusable tag or the current ticket. `Snapshot`, `Registry`, `CloseRecord`, `Backer`, `User` and `Position` are never closable in the first release. Each component returns to its funder exactly once. Account cleanup is never a precondition for cash commit, finalization or reopening.

### DR-14 Refund of unused locked reserve happens at terminal promotion, not at cleanup — ROUTINE (DEVIATION: wording clarification only)

The accepted text says "refund unused funds exactly once after required work/cleanup" and separately that cleanup must not block cash. **Revised:** split the two timers explicitly: the *locked reserve* on a `CloseRecord` is settled once when promotion becomes terminal (amounts transferred to the ORAO Client and rent spent on candidate pages, the registry extension and the B account are deducted by category; the remainder returns to ops free balance / owner destination); *rent components* on closable shared accounts return at safe closure under DR-13. Provider reimbursements go to the ORAO Client and are observations only; they become accounted OpsCash money solely through the G-signed `withdraw_provider_return` wrapper (DR-32), which executes the ORAO withdrawal, asserts the balance deltas and pays the recorded owner refundable share first. Settlement refunds only the **remaining** partitions, which decrement at spend time, so spent money can never be refunded. No lamport is refunded twice because the pools are disjoint. This is presented as a deviation only because the accepted sentence can be read as a single refund event; behavior and conservation are unchanged.

### DR-15 Unsolicited SOL — ROUTINE (accepted, no change)

Plain transfers to `[game-vault]` raise `unallocated_excess` at the next instruction that reconciles physical lamports; no instruction can spend, credit or sweep it in first release. `fund_operations` is the only sanctioned way to give the project SOL.

---

## D. Round close, candidates, randomness

### DR-16 Participant traversal completeness proof — ROUTINE

**Recommendation:** `CloseRecord.participant_count` is copied from `Pool.participant_count` at close acceptance; `build_candidates(page, rows)` must visit participant slots in strictly increasing order with an exact cursor, re-deriving each owner's `UserRound` and `User` PDAs from the stored pubkey and verifying them; sealing requires `cursor == participant_count`. Every participant is classified exactly once as preferred, fallback-only, or not-a-loser. The sealed digest is SHA-256 over the ordered candidate pubkeys per set.

### DR-17 ORAO Callback 0.4.0, no client-level callback, direct capture — ROUTINE (accepted candidate)

**Revised (R5):** the pinned SDK distinguishes three roles (`vendor/orao-solana-vrf-cb/src/state/client.rs:14–23`, `instructions/register/mod.rs:85–92`): the Solana account owner is the ORAO program; `Client.owner` is the management authority that can withdraw Client funds or transfer the Client, and registration requires the registered program's upgrade authority to sign as payer, so `Client.owner = G Vault` (accepted G-owned provider client); `Client.state` is the request authority, our `[rng-authority]` PDA, which CPI-signs `request`. The earlier wording "Client owned by RngAuthority" conflated these and is withdrawn. `request_round_draw()` funds the Client from the locked sponsor partition, then CPI-requests with the protocol seed; `capture_round_draw()` reads the fulfilled `RequestAccount` directly and requires `Clock <= deadline`. Registration is a G Squads vault transaction performed at devnet/mainnet stages (launch input), and readback must show the three roles. A request-level callback is not used.

### DR-18 Keep `expire_promotion` separate from `finalize_round`, and allow a composed client transaction — ROUTINE (accepted)

Composition is at the transaction level (three instructions: `expire_promotion`, `finalize_round`, `open_position`) so each instruction keeps one responsibility and the composed path needs no new program logic. `open_position` takes `expected_round_id` and fails if the round differs.

---

## E. Governance and operations

### DR-19 Initialize against `ProgramData.upgrade_authority` — ROUTINE (accepted), with two draft fixes

`initialize` passes the executable `Program` and its `ProgramData` accounts and requires `ProgramData.upgrade_authority == G vault PDA`, and that the G vault signs (Squads vault CPI). No first-caller enrollment.

**Draft fixes required before the control tests mean anything** (inventory §5.2 #1–#2): the Squads `Multisig` discriminator constant in `controls.rs:438` has a wrong last byte (`0xc8`; correct `0xec`), and `config_authority` is parsed as `Option<Pubkey>` where Squads v4 stores a plain `Pubkey` (`Pubkey::default()` = autonomous, confirmed from public source on 2026-10-13). The plan's governance task rebuilds the parser from the pinned Squads source and tests it against a byte-exact fixture derived from that source, not from the parser itself.

Solana note: unlike a canister whose controllers are read from the canister's own settings, a Solana program's code lives in a `ProgramData` account controlled by the BPF upgradeable loader, and only its `upgrade_authority` can replace code. The initialize check ties application governance to that same key so there is exactly one party that can change code *and* config.

### DR-20 Guardian pause instruction touches `[admission]` and the P vault signer only; Admission stores the authoritative guardian key — ROUTINE (accepted)

**Revised (R4):** `Admission.guardian_vault` is the key `set_admission_pause` checks (the draft already stores `Admission.guardian`); `rotate_guardian(new_multisig)` is a G instruction that validates the replacement 2-of-3 / zero-delay profile and writes `Admission.guardian_vault` and the `Pool.guardian_vault` copy atomically. The previous text had rotation writing only `Pool`, which would have left the pause surface authorizing the old key. Tests assert the pause instruction's account list contains no `Pool`, `Ticket`, `CloseRecord`, vault, Backer or provider account, that the old P is rejected and the new P accepted after rotation, and that G can recover a lost P quorum.

### DR-21 Keeper hosting: external fee-only primary + ICP watchdog — MEASURE (default = hybrid)

Default stands until the plan's measurement milestone yields full-path K (transactions per month) and CU/packet data for N≈100. If measured K at the chosen chunk size makes ICP signing cost acceptable to Rob, fully ICP becomes available; the decision is Rob's at that point and is not a gate for earlier milestones.

### DR-26 Stay on Anchor 0.32.1 / SDK 2.3.1 / LiteSVM 0.7.1 / Agave 2.3.13 — ROUTINE

Public check on 2026-10-13: 0.32.1 is published and not yanked; 0.32.2 (2026-09-14) is a patch on the same Solana 2.x line; 1.2.1 is the latest stable on Solana 3.x; 2.0.0-rc.2 is a prerelease; LiteSVM is at 0.18.0 on Agave 4.3 while 0.7.1 remains published. The accepted tuple stays. If a Task 0 probe needs an Anchor patch, 0.32.2 is the only permitted move without a new decision. The Foundation skill's "Kit 8 / transaction v1 by default" guidance is reference, not instruction.

### DR-27 Rent figures are dated scenarios under SIMD-0437 — MEASURE / LAUNCH

Solana rent is being cut in five feature-gated steps (6,960 → 6,333 live 2026-09-03 → 5,080 → 2,575 → 1,322 → 696 lamports/byte; steps 3–5 expected with Agave 4.4, Nov 2026). The budget rebuild (plan Task 11) records the active step at measurement time and recomputes at launch; the close cost schedule values (provider fee, headroom) and operations funding are launch inputs, not constants, and the captured reserve itself is computed per close (DR-12). No rent coefficient is hard-coded in the program; the program reads `Rent::get()`.

Solana note: unlike ICP cycles, which are consumed, Solana rent is a refundable deposit; a rent cut lowers the capital locked in shared pages, not a running cost. It does not change any game rule.

### DR-28 Keeper transaction format — MEASURE (default = legacy/v0)

v1 transactions (4,096 bytes) are live on mainnet since Agave 4.2; the 96-account limit is not yet active. The GUI keeps legacy/v0 (accepted). A keeper may adopt v1 or address-lookup tables only after Task 11 measures the account-lock bound, which v1 does not relax today, and only without changing program economics or cursor proofs. The ICP bridge decoder must handle observed v1 transactions regardless (accepted).

### DR-29 Six typed allocation instructions instead of one dispatcher — ROUTINE (adopts Codex answer 1)

`discover_rows`, `apply_senior`, `seal_weights`, `allocate_rows`, `prune_rows`, `commit_ticket`, each taking `expected_ticket_id` (and `expected_cursor`, `rows` for the row phases) and a typed `Accounts` struct as the draft already shapes them. Stale calls fail with `StaleTicket`/`StaleCursor`; replaying a completed chunk cannot re-credit. A unified dispatcher remains an optional later refactor only with an equivalent proved remaining-accounts schema. Rationale: typed account validation per phase is simpler to audit and test.

### DR-30 Position accounts are permanent; no rent reclamation in first release — ROUTINE (R1)

The Position PDA is the durable nonce tombstone. Closing it would let a freshly signed retry with the same nonce create and fund a second Position after settlement, which the accepted unknown-outcome protection forbids. `close_position_account` is withdrawn. A stale-round Position is terminal by `round_id < pool.round_id` and needs no write. Compact per-owner tombstones with rent reclamation are **DEFERRED**; locked owner rent is one Position's rent-exempt minimum per opening.

### DR-31 Inline allocation threshold chosen per measured path, never by blanket exclusion — MEASURE

M9 measures a complete inline allocation inside the payout at N = 0, 1, 2, 4, 8, 16, 17 with production accounts. The inline path is enabled for every N up to the largest measured N that fits packet, account-lock and CU limits with margin; the zero-toll and zero-slice shortcuts always stay inline; chunked tickets cover larger N. If the production handlers only support chunks when M9 runs, M9 adds the inline step. A failure at N = 16 does not disable smaller feasible N.

### DR-32 Provider cash attribution: sponsor-first with the accepted owner fallback, three cash locations, G-authenticated returns — ROUTINE

**Revised (C2):** the 2026-10-13b text narrowed the owner partition to storage only; that contradicted the accepted defaults ("may cover reserved storage/provider costs only if owner opts in") and is withdrawn. Provider fee and pending-request rent are funded sponsor-first and then from the owner partition up to the owner's quote, with exact category/source attribution in the `CloseRecord.spent` ledger. The fee is consumed to the ORAO Treasury inside `request` (non-refundable from either source). Pending rent sits in the RequestAccount and, on fulfillment, 4,259,520 of 6,173,520 lamports (local default rent, PROBED) is reimbursed to the **Client**, which is G-owned and mutable outside this program, so Client values are observations only. Reimbursed rent becomes accounted OpsCash money only through G-signed `withdraw_provider_return(close_id, amount)`, a wrapper that performs the pinned ORAO `withdraw` CPI (which pays the Client **owner**, `withdraw/mod.rs:38–45`) and the G-Vault→OpsCash transfer in one instruction, asserting the Client and OpsCash balance deltas; eligibility is set once per close by `observe_request_fulfilled` from that close's own retained request binding (`eligible = checked_sum(request_rent_funded_s/o).saturating_sub(actual fulfilled RequestAccount lamports)` (frozen once after the retained request binding/state/owner checks; later Rent quotes cannot raise eligibility, and unsolicited RequestAccount credits can only reduce it)), the retained rent is borne sponsor-first so the owner's share is refunded first, `owner_request_rent_refundable_remaining` and `eligible_return_remaining` decrement on every return, and no G signature without the executed CPI, no anonymous OpsCash surplus and no observed Client gift can create eligibility. Until a real return the owner refund is recorded but blocked; it never becomes free Ops. Expired unfulfilled requests keep their rent provider-side; G withdrawals elsewhere are recorded as `provider_deficit`. None of this blocks cash commit, expiry, finalization or claims. The pinned adapter does not prevent attributable recovery; it only requires the G-signed return path above.

### DR-33 Provider budget increase and OpsCash reconciliation instructions — ROUTINE (C2)

`increase_provider_budget(close_id, extra)` is the only way to cure a live provider-fee rise after a fully funded lock: permissionless, before the deadline, only while `NotRequested`; moves exactly `extra` from free sponsor balance into `locked_sponsor_remaining`, raises `approved_provider_fee_cap` and `required_reserve` by `extra`, never touches the owner quote, seed, deadline, candidates or weights; unused `extra` returns once at settlement. `reconcile_ops_cash()` classifies any physical OpsCash surplus as `unallocated_excess` (never spendable in first release, mirroring the game vault), refreshes `client_balance_observed` and recomputes the informational `provider_deficit = eligible_return_budget − max(0, client_balance_observed − client_protected_rent)` (saturating). `Ops.locked_remaining_total` is a maintained O(1) aggregate (`+=` on lock and budget increase, `−=` on each spend and settlement), never a runtime sum. The earlier `top_up_close_reserve` is withdrawn because its guard was unreachable once the lock was fully funded.

### DR-34 PP ingestion is a two-phase observe/bind protocol with a pinned policy version — ROUTINE (C3)

`observeNativeEvent` pins `policy_version` at first validated observation and stores a pubkey-keyed observation with no effects consumed; `bindAndReserve` resolves the principal via `siws_provider.get_principal`, writes the shared `signupGiftClaimed` marker before any await, computes legs once under the pinned version with the upline chain resolved at binding, consumes effects, freezes legs. Obligation status (`Observed → Reserved → Completed | PartiallyQuarantined`) and leg status (`Pending | SubmittedUnknown | Committed | Retryable | Quarantined`) are two levels. After the SIWS lookup await, `bindAndReserve` re-reads the obligation and reserves atomically with no further await. Definite rejections (`TemporarilyUnavailable`, `BadFee`, `InsufficientFunds`, `GenericError`) are `Retryable` and may be re-presented with identical frozen fields via `retryNativeRewardLeg` inside the verified ledger window after the cause is corrected; outside the window or on `TooOld`/`CreatedInFuture` the leg is `Quarantined{reconcile}` and only `reconcileLegFromLedger` completes it, requiring a `kind == "mint"` record whose `to` (owner and subaccount), `amount`, `memo` and `created_at_time` match exactly, fetched from the pinned ledger or an archive callback listed by `archives()`; permanent immutable-field errors stay `Quarantined{permanent}` with the recovery limitation recorded. The bridge cursor advances only on `#Observed`/`#AlreadyObserved` or over a durable quarantined record. This completes R6; it does not change any reward rate.

### DR-35 Canonical source digest and exact deployed-byte comparison — ROUTINE (C5)

`source_set_sha256` is defined over the sorted relative paths and content hashes of the production program sources, manifests, lockfile, cargo config, the build wrapper and the toolchain file (plan M0); the wrapper fix is scheduled in M0 and used in every build/CI/release receipt. Deployed bytes are verified by decoding the 45-byte loader-v3 `ProgramData` header, comparing exactly `elf_len` bytes to the reviewed ELF and asserting the remaining bytes are zero padding, recorded separately; trailing zeros are never trimmed from the ELF.

---

## F. Frontend and ICP services

### DR-22 Native rail is the default SOL view; legacy Chain Fusion positions appear in a labeled, accessible secondary section — ACCEPTED (Codex, under Rob's delegated authority, 2026-10-13)

**Recommendation:** within the existing SOL panels, show native positions/Backer/claims by default and render the legacy ICP-observed positions in a clearly labeled "Legacy (Chain Fusion) positions" section with their existing withdrawal/claim actions unchanged. Layout, styling and navigation are untouched. Alternative: a toggle between rails (more clicks; hides legacy balances from users who still have them). This is the one GUI arrangement decision that changes what users see first.

### DR-23 Transaction confirmation via HTTPS polling with `confirmed` for progress UI and `finalized` for authoritative state — ROUTINE (accepted)

No WebSocket. Simulation before wallet prompt. Signature retained across unknown outcomes; a client nonce on `open_position` makes retries idempotent because the Position PDA is permanent (DR-30), so an unknown-outcome retry either finds the Position or creates it once. For `fund_series_a`, which is deliberately repeatable, the client keeps the original signature and resolves finality/history before any deliberate second funding; an explicit retry test covers this without changing the accepted repeatable ABI.

### DR-24 ICP ingestion canister is a new `native_sol_bridge` canister, not a modification of `shenanigans` observer loops — ROUTINE

**Revised (R6):** add one new canister that owns the relayer ingress, two-RPC finalized verification, the event outbox and cursor. **Reward policy, effect/siphon reservation and ledger-leg execution are owned by Shenanigans**, which already owns the PP minting account and effect state: `reserveNativeReward(event)` (bridge-only, idempotent on event id, computes and freezes every leg once and consumes effects at reservation, no ledger await) and `executeNativeRewardLeg(event_id, leg_index)` (status written before the single transfer; identical fields on retry; `Duplicate` reconciles; out-of-window → `Quarantined`). The earlier `mintFromBridge(leg)` reusing `mintWithEffects` is withdrawn because that helper applies multipliers/siphons dynamically and marks dedup after its await (`shenanigans/main.mo:2871–2935`). Legacy observers are untouched. Isolation of the RPC trust boundary remains the reason for a separate bridge canister; it does not by itself produce exactly-once minting, the protocol above does.

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
| `provider_request_fee_lamports`, `provider_fee_headroom_bps`, snapshot/registry pages to pre-provision, ops funding amount | Rob from measured sizes and the live `NetworkState` | `ConfigVersion`, `fund_operations` |
| Production RPC endpoints (primary + fallback), CSP `connect-src` origins | Rob/operator | frontend constants, `.ic-assets.json` |
| Bootstrap pot/seed | fixed at zero unless Rob funds externally with a named receipt | none |

---

## H. Deferred optional scope (not prerequisites)

Native PP/SPL and native desk; Position rent reclamation via compact nonce tombstones (DR-30); snapshot/registry page rent reclamation (DR-08); proof-bearing refund of anonymous excess in the game vault or OpsCash; paid on-chain keeper rewards; frontend v1 transaction / Kit migration; Pinocchio or custom allocation accumulators; additional governance signers, custom timelock wrapper or renounced upgrades; ICP `raw_rand` as an alternative provider; II/SIWS account merging; legacy deposit pause or retirement.

---

## I. Explicit non-decisions

The following were considered and are **not** reopened: strict `owed > pot` close; 35/45/20 split with folding/sealed denominator; gross accrual; three-successes-per-rolling-hour quota with no dynamic cap; 600-second immutable deadline with no reroll; zero keeper reward; public A on; operator eligibility; separate legacy rail; PP on ICP. Any plan task that appears to touch one of these is implementing it, not changing it.
