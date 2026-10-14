# Codex review packet — native Solana discovery, specification and plan (2026-10-13)

Prepared by Claude (planning coordinator) for Codex's independent review and Rob's decision. **No implementation, build, test run, deployment, funding, signing, merge or canister change occurred in this pass or its revision.** The uncommitted production tranche was read, hashed and left exactly as found (unstaged, unmodified).

> **Revision round 3 (2026-10-13c):** Codex's [round-2 review of `1313b52e`](native-solana-codex-plan-review-round2-20261013.md) (SHA-256 `01a0c2d31a41a650eda0dba9979c15eae2706551a19a5bdba608d2d1da2b9138`, preserved unchanged) requested focused docs-only revisions C1–C6. Section 11 is the resolution matrix; §1 hashes are the current files. Rob delegated plan settlement and local implementation authorization to Codex; DR-22 is recorded as accepted.
>
> **Revision round 2 (2026-10-13b):** Codex's [review of commit `db48ce33`](native-solana-codex-plan-review-20261013.md) (SHA-256 `d9b59efdeed52842426bc4e4d3213bdda91d888da5266b94481bb8f20bd6b15a`, preserved unchanged) requested revisions. Rob authorized docs-only revisions. Section 10 below is the item-by-item resolution matrix; §1 hashes are the revised documents. Sections 2–9 are the round-1 text, kept for history, with §4 annotated.

## 1. What to read, in order

| # | Document | Lines | SHA-256 (working tree, 2026-10-13) |
| --- | --- | ---: | --- |
| 1 | [Current-state and evidence inventory](native-solana-state-inventory-20261013.md) | 191 | `1fbca542727d53699d4e287605c341725a7d28d725dbaf90c3824d358ae31c03` |
| 2 | [Reconciled specification](../superpowers/specs/2026-10-13-native-solana-reconciled-spec.md) | 484 | `13f33a373d5cd6ccbb2c1e8822b74f3c3dcea4dd4b6137c736f7797b70052c4f` |
| 3 | [Decision register](native-solana-decision-register-20261013.md) | 236 | `9ebc8244995d729f011d2e740b189584bc24e162eff9327803392812a2f4c521` |
| 4 | [Implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md) | 741 | `f80ce46a43fb036e0a69927c8ac182384f3bd0548e7a4e6187aa526ca82eab4d` |
| 5 | This packet | — | recorded in the graph refresh receipt |
| 6 | [Planning graph refresh receipt](../solana/evidence/2026-10-13-planning-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-graph-fragment.json) (round 1); [Codex review append](../solana/evidence/2026-10-13-codex-plan-review-graph-refresh.json); [revision receipt](../solana/evidence/2026-10-13-planning-revision-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision-graph-fragment.json) (round 2); [round-3 receipt](../solana/evidence/2026-10-13-planning-revision2-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision2-graph-fragment.json) | — | hashes inside each receipt |

Banners were added at the top of `docs/superpowers/specs/2026-10-12-native-solana-design.md` and `docs/superpowers/plans/2026-10-12-native-solana.md` pointing to their successors; no other line in those files changed. All other files in the worktree are untouched by this pass. Verify with:

```bash
cd /Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs
git status --porcelain
shasum -a 256 docs/coordination/native-solana-state-inventory-20261013.md docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md docs/coordination/native-solana-decision-register-20261013.md docs/superpowers/plans/2026-10-13-native-solana-implementation.md
```

The expected `git status` shows Codex's prior dirty paths (`docs/coordination/kanban.md`, `solana/Cargo.toml`, `docs/solana/production-accounts.md`, `solana/programs/`, `solana/tests/`) plus the five new planning documents, the two banner edits and the two evidence JSON files. Hashes of every pre-existing dirty file are in inventory §5 and must still match.

## 2. Reconciliation summary (what changed in meaning, not just wording)

1. Status wording is replaced by a four-rung ladder: accepted policy / host-modeled (72 + 5 tests recorded) / SBF-probed (88 cases recorded) / draft (10 handlers `NotImplemented`). No count was re-run.
2. Nineteen document/source disagreements are resolved in spec §2 (R1–R19). None changes an accepted invariant. Two correct the handoff text itself: `createBackdatedGame` is not in the SOL canister (R13); candidate rows are 32 bytes (R14).
3. Draft deferrals are confirmed as tranche boundaries: owner contribution in the ABI from day one (DR-09), full close/provider/reopen (M7), reserve/refund/cleanup (M8), config versioning (DR-04), retained `CloseRecord` (DR-02).
4. One wording deviation is declared (DR-14: locked-reserve settlement at terminal promotion, rent components at safe closure); behavior and conservation unchanged.

## 3. Findings Codex should check first

| # | Finding | Where | Why it matters |
| --- | --- | --- | --- |
| F1 | Squads `Multisig` discriminator constant ends `0xc8`; correct last byte is `0xec` | `controls.rs:438`, fixture `:711` | Real governance accounts would be rejected; the draft's four unit tests are circular |
| F2 | `config_authority` parsed as `Option<Pubkey>`; Squads v4 stores a plain `Pubkey` (default = autonomous) | `controls.rs:446–447` | Parse desynchronizes for exactly the accepted autonomous profile |
| F3 | Harness does not compile (`Signature.map`) | `financial-runtime/src/runtime.rs:293` | No financial test has ever executed; RED baseline must be captured first (plan M0) |
| F4 | Allocation draft prune: same-page swap-delete errors; alias branch never deactivates pruned Backer | `/private/tmp/mc-allocation-handlers-20261013.rs:464–530` | Would stall tickets or leave phantom active rows; regression tests specified in M6 |
| F5 | `mc-production-accounts-contract.md` reverses senior/A (45/35) | `:69–78` | Stale proposal; production source is correct at 35/45/20 |
| F6 | Platform-tools pin conflict v1.48 (provenance) vs v1.52 (cached binary) | inventory §4 | Must be resolved before any build claim (M0 step 2) |
| F7 | Rent reduction SIMD-0437 in progress; v1 4,096-byte transactions live on mainnet | inventory §8 | Budget inputs are dated scenarios (DR-27); GUI stays v0, keeper format is a later measured decision (DR-28) |
| F8 | ICP docs guide prints an invalid SOL RPC principal; repo binding `tghme-zyaaa-aaaar-qarca-cai` is the valid one | inventory §8 | Verify live Candid before the bridge binds (M12) |

## 4. Sharp questions for Codex (round 1; Codex answered all eight — adoption recorded in §10.D)

1. Spec §7.3 collapses the draft's seven allocation entrypoints into one `advance_ticket(rows)` dispatching by phase. Do you see a reason to keep separate instructions (e.g., simpler account validation per phase or smaller instruction data)? The plan tolerates either; the ABI must be frozen in M2.
2. DR-02 moves close state into a retained `CloseRecord` provisioned ahead of time. Is there a cheaper way to satisfy "old callbacks cannot attach to a later round" that you prefer?
3. DR-11 picks `emit_cpi!` for events because the ICP bridge decodes transactions via RPC and logs are truncatable. Any objection to the extra CU and the self-CPI account in every financial instruction?
4. DR-03 stores candidate rows as owner pubkeys and re-reads `UserRound` at apply. Agree that `UserRound` is immutable once the round is sealed, so this is safe?
5. R17: accepted text says global funded claims are u128; the draft used u64. Keep u128 as specified, or do you want to argue u64 on the grounds that the identity bounds it by vault lamports?
6. M9 may show that no inline path fits at N = 16 with production accounts. Are you content that this yields a "not a first-release path" record rather than any economics change?
7. The plan runs the frontend browser verification against a locally installed `solana-test-validator` (Agave 2.3.13) in an owned temp dir. Acceptable dev tooling under the no-global-install rule, or do you prefer LiteSVM-only until devnet?
8. DR-24 proposes a new `native_sol_bridge` canister rather than extending `shenanigans`. Do you agree that isolating the RPC trust boundary justifies a new canister, given the extra controller/cycles surface?

## 5. Risks carried into implementation

- **Packet fit:** production rows carry more accounts than the probe kernel; eight rows may not fit 1,232 bytes. One-row fallback is mandatory and measured in M6/M9.
- **Provider liveness:** local lifecycle used one synthetic fulfill authority; devnet latency and mainnet quorum are unproved until M14.
- **Governance parse:** F1/F2 show the draft cannot be trusted on Squads layout; M3 derives fixtures from the pinned Squads source commit.
- **Heap/CU:** lazy errors are applied in the tranche; M9 measures worst-case rows.
- **Rent volatility:** every lamport figure in budgets is a dated scenario.
- **Legacy observer defects** (owner-only `solBackerSeen`, fresh `created_at_time` per mint) are recorded but out of scope; the bridge does not inherit them.

## 6. Proposed acceptance checklist for the plan (before Rob authorizes)

- [ ] Codex confirms spec §2 reconciliations R1–R19 do not alter any accepted invariant.
- [ ] Codex confirms DR-01–DR-28 classifications (ROUTINE / ROB / MEASURE / LAUNCH / DEFERRED) and flags any it believes needs Rob.
- [ ] Rob decides DR-22 (native default view with labeled legacy section).
- [ ] Codex agrees the test matrix covers every §3–§8 invariant or names gaps.
- [ ] Codex agrees M0's RED baseline + toolchain re-pin precede any handler work.
- [ ] Both agree the authorization gates (local / devnet / mainnet) are correctly placed.

## 7. Optional backlog (explicitly not prerequisites)

Native PP/SPL and desk; refund of anonymous excess; paid keeper rewards; frontend v1/Kit migration; Pinocchio/custom accumulators; more governance signers, custom timelock, renounced upgrades; ICP `raw_rand` alternative; account merging; legacy observer fixes (`solBackerSeen` keying, mint timestamp); candid sync of `getBackerLifetimeRepaid`/`getOutstandingBackerDebt` in the checked-in `.did`.

## 8. Exact inventory of changes made by this pass

Created: the five documents in §1 plus `docs/solana/evidence/2026-10-13-planning-graph-fragment.json` and `docs/solana/evidence/2026-10-13-planning-graph-refresh.json`. Modified: two banner lines (top of the 2026-10-12 spec and plan). Graph: `graphify-out/` (gitignored) updated by merge with the fragment; pre-merge copies kept at the session scratchpad and at `/private/tmp/mc-production-graph-base-20261013/`. Not touched: `docs/coordination/kanban.md` (carries Codex's uncommitted board lines), `solana/Cargo.toml`, all `solana/programs/**` and `solana/tests/**`, every `/private/tmp/mc-*` draft, every frontend/Motoko file, memory/persona catalogs.

Git artifact: planning documents and graph evidence only are committed to branch `codex/native-solana-plan-20261013` (created from `c99a636` without changing the worktree's checked-out branch); the branch is pushed for review; no existing branch is force-pushed; nothing is merged.

## 9. Stop

Implementation begins only after Codex's re-review of the revised package and Rob's explicit authorization. The first implementation commit (plan M0 step 1) adopts the tranche verbatim so the inventory hashes remain the review baseline.

## 10. Revision round 2 — resolution matrix (2026-10-13b)

Legend: **FIXED** = the document now specifies the corrected contract and the plan carries the regression tests; **CORRECTED** = an earlier inference of mine was wrong and is withdrawn. Nothing was implemented or run. Line numbers are in the revised files hashed in §1 ("Spec", "Plan", "DR" = register, "Inv" = inventory).

### 10.A Required contract revisions

| Item | Status | Resolution | Where |
| --- | --- | --- | --- |
| R1 nonce identity after Position reclamation | **FIXED** | Position accounts are permanent in the first release; the PDA is the durable nonce tombstone; `close_position_account` withdrawn; a stale-round Position is terminal without any write, so no stale→closed path is needed; compact tombstones with reclamation deferred (DR-30) | Spec L263 (Position row), L311 (§7.2 paragraph), L307 (stale-round precondition), L432; DR-30 L165, DR-23 L185; Plan L242, tests L399, L411, L414, matrix L697 |
| R2 closing reserve from captured work | **FIXED** | `required_close_reserve(participant_count, registry_len, candidate_pages_existing, schedule, rent)` covers both candidate sets, a registry extension page when the tail is full, one automatic B account, pending-request rent and the provider fee with headroom; locked exactly before cash moves, sponsor-first then bounded owner; `ConfigVersion` carries schedule inputs only; live fee rise → `ProviderFeeExceeded` with permissionless `top_up_close_reserve` before the deadline; no flat allowance, no population cap | Spec §8.1–8.5 L348–371 (formula L354, lock L365), CloseRecord row L269, ConfigVersion row L260, `top_up_close_reserve` L298, `request_round_draw` L330; DR-12 L95; Plan M2 L168, L170, M5 L324, L335, M7 L565, L580, M8 L592 |
| R3 reusable-page cleanup capacity invariant | **FIXED** | Snapshot and registry pages are permanent and never closable in first release, so `pool.snapshot_pages` is monotonic and the admission capacity check is live; `close_shared_account` accepts only `ParticipantPage{round}` (that round's `CloseRecord.finalized`) and `CandidatePage{close_id,set}` (`CloseRecord(close_id).promotion` terminal), predicates bound to the page's own seeds, never a reusable tag; rent reclamation of snapshots deferred | Spec L266 (SnapshotPage row), L299, §8.6–8.7 L373–375; DR-08 L73, DR-13 L99; Plan M4 L232, M8 L592 |
| R4 guardian key shared by pause and rotation | **FIXED** | `Admission.guardian_vault` is authoritative; `set_admission_pause` checks it; `rotate_guardian(new_multisig)` (G) validates the P profile and writes `Admission.guardian_vault` and the `Pool` copy atomically; pause surface stays two accounts | Spec L237 (§5 P bullet), L255 (Admission row), L291, L293; DR-20 L139; Plan M3 L209, M14 readback L666 |
| R5 ORAO Client roles | **FIXED** | `AccountInfo.owner = ORAO program`, `Client.owner = G Vault` (registration payer must be the program upgrade authority, `register/mod.rs:85–92`; owner may withdraw/transfer per `client.rs:14–19`), `Client.state = RngAuthority` (request authority, `client.rs:20–23`); "Client owned by RngAuthority" withdrawn; request preconditions and devnet readback check all three | Spec L236 (§5 G bullet), L259 (RngAuthority row), L330; DR-17 L119; Plan M7 L564, M14 L666 |
| R6 PP policy owner and per-leg exactly-once | **FIXED** | Shenanigans owns policy, effect/siphon reservation and leg execution: `reserveNativeReward(event)` (bridge-only, idempotent on event id, computes every leg once, consumes effects at reservation, fixes `created_at_time`/memo, no ledger await) and `executeNativeRewardLeg(event_id, leg_index)` (status written before the single transfer; identical fields on retry; `Duplicate` reconciles; out-of-window → `Quarantined`); bridge owns observation/outbox and advances its cursor only after reservation; `mintFromBridge` reusing `mintWithEffects` withdrawn; legacy path untouched | Spec §9 L386–388; DR-24 L189; Plan M12 L639, L641, boundary tests L643 |

### 10.B Additional ABI/proof corrections

| Item | Status | Resolution | Where |
| --- | --- | --- | --- |
| U256 discovery accumulation | **FIXED** | `Ticket.a_remainder_total` is U256 and accumulated as U256 in `discover_rows`; host `Book` widened in M1; fixture with a sum above u128 | Spec L267, L317; Plan M1 L155, M6 L481 |
| Simple accrual wording | **FIXED** | per-step quotient is an `increment`; `accrued_gross` cumulative; `owed = accrued_gross − gross_paid`; failed/partial claims never drop unpaid earnings | Spec L101; Plan M5 L428 |
| Immutable cluster domain | **FIXED** | `Pool.cluster_domain [u8;32]` written once by `initialize`, no setter; seed-byte vectors for devnet and mainnet proved in M7 | Spec L217, L254; Plan M3 L217, M7 L560 |
| Provider funding / refund / retained rent | **FIXED** | OpsCash→Client funding from `locked_sponsor` only; fee consumed; ORAO reimburses excess pending rent to the Client (runtime receipt amounts 6,173,520 / 4,259,520 / 300,000), tracked as `ops.provider_prepaid`, G-withdrawable, never an owner or close refund; expired requests retain rent provider-side | Spec L330, L257, §8.3–8.5 L367–371; DR-32 L173, DR-14 L103; Plan M7 L561, L562, M8 L592 |
| Prune aliases and stale rejection | **FIXED** | duplicate independent Backer rows rejected; same-page and moved==pruned aliases decoded and written once; six typed instructions carry `expected_ticket_id`/`expected_cursor` with `StaleTicket`/`StaleCursor` | Spec L321, L322, L343; DR-29 L161; Plan M6 L437, L478, L509 |
| Inline default | **FIXED** | threshold chosen per measured production path at N = 0,1,2,4,8,16,17; feasible small N kept; zero-slice shortcuts always inline; failure at 16 never disables smaller N; inline step added if handlers only chunk | DR-31 L169; Plan M9 L602 |

### 10.C Execution-proof and release corrections

| Item | Status | Resolution | Where |
| --- | --- | --- | --- |
| M0 genuine RED | **FIXED** | build and pin the current stub ELF with the wrapper; repair fixture validity (Ticket phase, snapshot count, sequence, Backer-PDA keys, preloaded counts) so the only failures are `NotImplemented`; RED is `cargo test` on `lib.rs:31–59` asserting exact `NotImplemented` and fee-only rollback; the CLI is receipt generation only | Plan M0 L97, L101, L105 |
| Toolchain F6 inference | **CORRECTED** | the v1.52 banner is the builder's default; the wrapper passes `--tools-version v1.48 --skip-tools-install` and exports the installed rustc 1.84.x (`build-sbf.sh:56–57, 62–70`); M0 verifies the installed tree by commit stamps and hashes before any re-download; prior 88-case evidence not superseded by the banner | Inv L56, L188; Plan M0 L79 |
| `MC_SBF_BUILDER` consistency | **FIXED** | treated as the executable everywhere | Plan L79, L97 |
| Anchor `solana_program::hash` re-export | **FIXED** | test uses the pinned `sha2` crate | Plan M3 L193 |
| Event-CPI feature/accounts/IDL | **FIXED** | `event-cpi` feature, `#[event_cpi]` on every emitting context (event authority PDA + program account), IDL and per-event encoding fixtures, decoding test, CU/accounts measured | Spec L338; DR-11 L85; Plan M2 L166, L173 |
| Production SBF CI | **FIXED** | explicit `production-sbf` job: pinned builder/tools by SHA-256 into a job-local cache, wrapper build, artifact/source/lock hash assertions, harness tests against the fresh artifact; must fail visibly, never fall back to host-only | Plan M9 L604 |
| Release evidence | **FIXED** | on-chain `ProgramData` bytes (loader header/padding handled) vs ELF, upgrade authority, source/lock/artifact/IDL hashes, ORAO role readback, ICP module hashes for bridge/keeper/shenanigans vs built Wasm, served frontend hash; uploaded/deployed/activation/finality/balances recorded separately; authorized ICP deployment steps added | Plan M14 L667, L668, M15 L678 |
| 64-counter proof split | **FIXED** | pure sampler test with constructed words or near-2^63 N; terminal transition tested from clearly labeled synthetic state; no production test-only RNG/winner override | Plan M7 L570 |

### 10.D Codex's answers to the eight questions — adoption

1 separate phase instructions → adopted (DR-29 L161). 2 retained `CloseRecord` with explicit pointers → kept. 3 self-CPI events with feature/accounts/IDL and measured overhead → adopted (§10.C). 4 owner-only candidate rows with a same-owner aggregation/immutability test → adopted (Plan L557). 5 u128 global funded claims plus U256 discovery → adopted. 6 no blanket N=16 rule → adopted (DR-31). 7 owned local validator labeled as local tooling → kept. 8 separate bridge canister plus explicit R6 protocol → adopted.

Nonblocking items: Fund A retry test added (Plan L414); explicit differential matrix (Plan L428); drafts remain references, not gates; rent re-quoted from RPC/feature state at measurement and launch (DR-27 unchanged). DR-22 remains Rob's user-visible preference, distinct from these financial corrections, and does not block core planning.

### 10.E Changed files in this revision

Edited: the four documents hashed in §1, this packet. Unchanged: Codex's review artifact and its two graph evidence files (included in the docs-only commit for durable history); the uncommitted production tranche, `/private/tmp` drafts, kanban, `solana/Cargo.toml`, every source and dependency byte (hashes in Inv §5 re-verified). Added: `docs/solana/evidence/2026-10-13-planning-revision-graph-{fragment,refresh}.json`. The successor commit SHA is reported in the hand-back message, not here, because this file is part of that commit.

## 11. Revision round 3 — resolution matrix for Codex's C1–C6 (2026-10-13c)

Legend as in §10. Nothing was implemented or run; source files are untouched.

| Item | Status | Resolution | Where |
| --- | --- | --- | --- |
| C1 executable, per-set captured reserve | **FIXED** | `required_pages_per_set = pages(participant_count)`; `missing_<set> = saturating_sub(required, existing_<set>)`; all counts u64; `provision_page(Candidate{close_id,set}, index, LockedClose)` creates one page per instruction from `locked_sponsor_remaining` then `locked_owner_remaining`, sequential index per set, never touches free Ops, cannot debit twice; excess pages stay attributed to their set; `build_candidates` creates nothing and fails `PageNotProvisioned` until the receiving page exists | Spec §8.1 L355, CloseRecord row L272, `provision_page` L300, `build_candidates` L334, §12 L476; DR-12 L97; Plan M2 L168, vectors L170, M4 L232, M5 L329, M8 L599, matrix L710 |
| C2 one conservation contract | **FIXED** | `locked_*_remaining` decrement at the instant lamports leave OpsCash; ten-field spent ledger by category and source; three cash locations (OpsCash authoritative; Treasury consumed; RequestAccount→Client observed) with `provider_fee_consumed_total`, `request_rent_outstanding`, `provider_returned_total`, `provider_deficit`, `client_balance_observed`; G-signed `record_provider_return` pays the owner's recorded refundable share first; `increase_provider_budget` raises the provider-only cap from new free sponsor funds before the deadline (replaces the unreachable top-up); owner fallback restored to storage **and** provider costs with exact attribution; `Ops.unallocated_excess` and `reconcile_ops_cash`; deficits never block cash; §8.6 transition table | Spec §3.7 L172–175, Ops row L260, L301, L302, L303, `request_round_draw` L335, §8.2–8.6 L374–398, §12 L477; DR-14 L105, DR-32 L175, DR-33 L179; Plan M7 L562, L565, L568, L572, L587, M8 L599 |
| C3 PP protocol for unbound identity and recovery | **FIXED** | two phases: `observeNativeEvent` pins `policy_version` at first validated observation (pubkey-keyed, no effects); `bindAndReserve` resolves via `siws_provider.get_principal`, writes the shared `signupGiftClaimed` marker before any await, computes legs once under the pinned version, consumes effects, freezes legs; `Retryable` added; definite-rejection vs unknown-outcome semantics; `reconcileLegFromLedger` via `pp_ledger.get_transactions`/`archives` with identical-field match, no manual mark-committed; cursor advances only on `#Observed`/`#AlreadyObserved` or over a durable quarantined record | Spec §9 L425–430; DR-34 L183; Plan M12 L646, L648, tests L650 |
| C4 genuine program provenance | **FIXED** | verified = finalized and `meta.err == null`, configured program on outer and inner instruction, canonical `event_authority` PDA signer, event-CPI and event discriminators, supported `schema_version`, ancestry indices, cluster, exact descriptor equality, signer provenance per instruction kind (owner among signers; promotion beneficiary from the event, never keeper); M14 PP path acceptance added | Spec L423; Plan M12 tests L650, M14 L675 |
| C5 exact source/artifact provenance | **FIXED** | `source_set_sha256` over sorted relative paths and content hashes of program sources, manifests, lockfile, cargo config, build wrapper and toolchain file; wrapper fix scheduled in M0 step 5 (harness script, not program source) and used in build/CI/release receipts; `ProgramData` verified by decoding the 45-byte header (pinned `solana-loader-v3-interface 3.0.0 state.rs:47–48`), comparing exactly `elf_len` bytes, asserting the remainder is zero padding separately; never trimming the ELF | DR-35 L187; Plan M0 L66, L99, M14 L674, M15 L686 |
| C6 wording and delegated authorization | **FIXED** | provider-accounts sentence and keeper list state the three ORAO roles; DR-04 holds cost-schedule inputs only; spec R15 restated per the repaired M0 distinction; `advancePhase`/old `advance_ticket(ticket, rows)` described as client orchestration over six typed instructions; `close_simple_position` takes no contribution argument (DR-09 narrowed); plan header and gate now wait for Codex's exact final acceptance under Rob's delegation; DR-22 recorded as accepted (native default, accessible labeled legacy withdrawals) | Spec L275, L440, L69, L457, L314; DR-04 L51, DR-09 L79, DR-22 L195; Plan L3, L19, L633 |

### 11.A Agreement

I agree the plan as revised is executable: every instruction's accounts, signers, payers, preconditions, postconditions and replay rules are specified; every reserve, lock and spend has a conservation transition; every Codex finding maps to a milestone step with named failing tests; and the authorization gates are explicit (local on Codex's go-ahead; devnet, mainnet, production ICP activation and PR merge remain unauthorized). Residual facts that only execution can settle are listed in the inventory §9 and plan M9, not hidden as assumptions.

### 11.B Changed files in this revision

Edited: the four documents in §1 and this packet. Included unchanged for durable history: `native-solana-codex-plan-review-round2-20261013.md`. Added: `docs/solana/evidence/2026-10-13-planning-revision2-graph-{fragment,refresh}.json`. Not touched: the production tranche, `/private/tmp` drafts, kanban, `solana/Cargo.toml`, source and dependency bytes. The successor commit SHA is reported in the hand-back message.
