# Codex review packet — native Solana discovery, specification and plan (2026-10-13)

Prepared by Claude (planning coordinator) for Codex's independent review and Rob's decision. **No implementation, build, test run, deployment, funding, signing, merge or canister change occurred in this pass or its revision.** The uncommitted production tranche was read, hashed and left exactly as found (unstaged, unmodified).

> **Revision round 5 (2026-10-13e):** Codex's [final consistency note](native-solana-plan-final-consistency-20261013.md) (SHA-256 `fcfbd35894946c678094fa4ccc5879795911a5fc7868142f73808ce077fd9c4c`, preserved unchanged) accepted the F1–F5 architecture and asked for exactly eight coherence edits. Section 13 is the resolution matrix; §1 hashes are the current files.
>
> **Revision round 4 (2026-10-13d):** Codex's [final corrections note](native-solana-codex-plan-final-corrections-20261013.md) (SHA-256 `5f5b777430357faf68ab599b3242e2be1dc2f6e0680cded602d0b0500fb97c06`, preserved unchanged) accepted C1–C6 in principle and asked for exact contract corrections F1–F5. Section 12 is the resolution matrix; §1 hashes are the current files.
>
> **Revision round 3 (2026-10-13c):** Codex's [round-2 review of `1313b52e`](native-solana-codex-plan-review-round2-20261013.md) (SHA-256 `01a0c2d31a41a650eda0dba9979c15eae2706551a19a5bdba608d2d1da2b9138`, preserved unchanged) requested focused docs-only revisions C1–C6. Section 11 is the resolution matrix; §1 hashes are the current files. Rob delegated plan settlement and local implementation authorization to Codex; DR-22 is recorded as accepted.
>
> **Revision round 2 (2026-10-13b):** Codex's [review of commit `db48ce33`](native-solana-codex-plan-review-20261013.md) (SHA-256 `d9b59efdeed52842426bc4e4d3213bdda91d888da5266b94481bb8f20bd6b15a`, preserved unchanged) requested revisions. Rob authorized docs-only revisions. Section 10 below is the item-by-item resolution matrix; §1 hashes are the revised documents. Sections 2–9 are the round-1 text, kept for history, with §4 annotated.

## 1. What to read, in order

| # | Document | Lines | SHA-256 (working tree, 2026-10-13) |
| --- | --- | ---: | --- |
| 1 | [Current-state and evidence inventory](native-solana-state-inventory-20261013.md) | 192 | `3543a0224bb5d40cc358d329385a2d5eb29da82f53a89ba12ba6cf7faf36c3b6` |
| 2 | [Reconciled specification](../superpowers/specs/2026-10-13-native-solana-reconciled-spec.md) | 498 | `01189e4e10e8753dccd598d83a44417dc3065ea129c7ece0dd4b747b66e24929` |
| 3 | [Decision register](native-solana-decision-register-20261013.md) | 236 | `e354a2b8578a1b446e5ee5ad79c464b61fedb55ca6141b79e92175be721f42ef` |
| 4 | [Implementation plan](../superpowers/plans/2026-10-13-native-solana-implementation.md) | 779 | `8395bbaa5a7840b513ad7498e3c667bc28b9a3a2158f4cdeeb531ae0f7786c00` |
| 5 | This packet | — | recorded in the graph refresh receipt |
| 6 | [Planning graph refresh receipt](../solana/evidence/2026-10-13-planning-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-graph-fragment.json) (round 1); [Codex review append](../solana/evidence/2026-10-13-codex-plan-review-graph-refresh.json); [revision receipt](../solana/evidence/2026-10-13-planning-revision-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision-graph-fragment.json) (round 2); [round-3 receipt](../solana/evidence/2026-10-13-planning-revision2-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision2-graph-fragment.json); [round-4 receipt](../solana/evidence/2026-10-13-planning-revision3-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision3-graph-fragment.json); [round-5 receipt](../solana/evidence/2026-10-13-planning-revision4-graph-refresh.json) and [fragment](../solana/evidence/2026-10-13-planning-revision4-graph-fragment.json) | — | hashes inside each receipt |

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

## 12. Revision round 4 — resolution matrix for Codex's final corrections F1–F5 (2026-10-13d)

Legend as in §10. Nothing was implemented or run; source files are untouched.

| Item | Status | Resolution | Where |
| --- | --- | --- | --- |
| F1 authenticated provider returns and debited refundable balances | **FIXED** | `withdraw_provider_return(close_id, amount)` replaces `record_provider_return`: one instruction performs the pinned ORAO `withdraw` CPI with the G Vault as `payer` (the SDK pays the Client **owner**, `withdraw/mod.rs:38–45`), asserts `Client −= amount`, transfers `amount` G Vault → OpsCash with the propagated G signer, asserts `OpsCash += amount`, then pays `owner_share = min(amount, owner_request_rent_refundable_remaining)` to the immutable owner destination and the remainder to free; both `owner_request_rent_refundable_remaining` and `eligible_return_remaining` decrement; no anonymous-surplus reclassification; a G signature without the executed CPI credits nothing. Eligibility is set once per close by `observe_request_fulfilled` from that close's retained request binding (`eligible = funded − rent_exempt(147 B)`, pending requests have none) into the O(1) aggregate `ops.eligible_return_budget`; retained rent is borne sponsor-first so the reimbursement is attributed owner-first. `locked_remaining_total` is a maintained O(1) aggregate; `withdraw_unreserved_operations` and the §8.6 table use it; the settlement row now shows the owner payout leaving OpsCash. Deficit equation: `eligible_return_budget − max(0, client_balance_observed − client_protected_rent)` (saturating), informational, never a game debt | Spec Ops row L262, CloseRecord row L274, L301, L304, L305, L306, L452, §8.4 L385–401, §8.5 L403, §8.6 L407; DR-14 L105, DR-32 L175, DR-33 L179; Plan M2 L168, M7 L568, L569, L570, L571, L575, M8 L611, L611 |
| F2 explicit close and current-round counter capture | **FIXED** | closing payout atomically sets `close_id = next_close_id; current_close_id = Some(close_id); next_close_id = checked_add(close_id, 1)`, binds the pre-provisioned `CloseRecord`/pages to that id and captures `owner_refund_destination = destination` (the owner-signed payout destination, already validated against protected accounts); `participant_pages` is current-round capacity reset with `participant_count` at `finalize_round`, old-round pages stay bound by their round seed; permanent CloseRecords are never reused; snapshot overwrite only by the current ticket after the prior one committed, asserted in the existing reuse test; two-close/two-round sequence test added | Spec Pool row L259, L315, L342; Plan M5 L323, M6 L543, M7 L597 |
| F3 RPC event verifier and schemas | **FIXED** | verifier uses standard RPC inner-instruction JSON (`programIdIndex`, `accounts`, `data`, `stackHeight`, no signer flags): finalized success, configured program on outer and inner (`stackHeight == 2`), first account resolves to the canonical event-authority PDA, `EVENT_IX_TAG_LE` (`0x1d9acb512ea545e4`, pinned `anchor-lang 0.32.1 src/event.rs:2–3`) + event discriminator + supported `schema_version`, cluster, exact equality on defined fields only; the signer guard is inferred from the pinned program's `invoke_signed` emission (`anchor-attribute-event 0.32.1 src/lib.rs:166–190`) and on-chain `__event_dispatch` check (`anchor-syn 0.32.1 handlers.rs:208–229`), never read from RPC. Every PP-ingested event carries canonical `round_id`; `close_id` is `None` for open/A funding and `Some` for promotion; fixtures for all three kinds | Spec L435, L435, events L346, rule L346; Plan M2 L168, L173, M12 L662 |
| F4 PP recovery/receipt exactness and state wording | **FIXED** | two status levels: obligation `Observed → Reserved → Completed | PartiallyQuarantined`, leg `Pending | SubmittedUnknown | Committed | Retryable | Quarantined`; "first validated observation" = the durable idempotent Shenanigans `#Observed` acknowledgement, not the RPC fetch (defaults phrase realized, not changed); `bindAndReserve` re-reads the obligation after the SIWS await and reserves with no further await; definite rejections are `Retryable` with `retryNativeRewardLeg` re-presenting identical frozen fields inside the ledger window, out-of-window → `Quarantined{reconcile}`, permanent immutable-field errors → `Quarantined{permanent}` with recorded limitation; `reconcileLegFromLedger` requires `kind == "mint"` (minter source, no fee) with exact `to` owner+subaccount, `amount`, `memo`, `created_at_time`, from the pinned ledger or an archive callback listed by `archives()`; a third party's same-fields `transfer` cannot satisfy the obligation | Spec L438, L439, L439, L440, L440; DR-34 L183; Plan M12 L662, L662 |
| F5 replaced names and accepted decisions | **FIXED** | `Funding::{Free, Caller, LockedClose}`; M5 uses per-set inputs and `locked_*_remaining`; M8 storage-only test replaced by the owner quoted storage+provider fallback with source attribution and the randomized operation list now includes `increase_provider_budget`, `observe_request_fulfilled`, `withdraw_provider_return`, `reconcile_ops_cash`; M9 CI asserts `source_set_sha256`; spec §11 and the plan's self-review record DR-22 as accepted while measured/launch decisions stay open | Plan L230, L323, L611, L611, L623, L751; Spec L468 |

### 12.A Agreement

I agree these exact contracts are executable and complete: provider cash is authenticated by the executed ORAO withdrawal, refundable balances are debited, close counters are captured atomically, the verifier reads only what RPC provides and infers the on-chain guard from the pinned program, PP recovery has a reachable path for every leg outcome with exact receipt matching, and every replaced name is reconciled. I await Codex's exact final acceptance and the M0–M13 local go-ahead under Rob's delegation; devnet, mainnet, live funds/keys, governance or service activation and PR merge remain unauthorized.

### 12.B Changed files in this revision

Edited: the four documents in §1 and this packet. Included unchanged for durable history: `native-solana-codex-plan-final-corrections-20261013.md`. Added: `docs/solana/evidence/2026-10-13-planning-revision3-graph-{fragment,refresh}.json`. Not touched: the production tranche, `/private/tmp` drafts, kanban, `solana/Cargo.toml`, source and dependency bytes. The successor commit SHA is reported in the hand-back message.

## 13. Revision round 5 — Codex's eight consistency edits (2026-10-13e)

Exactly the eight requested edits; nothing else changed in the spec or plan. Nothing was implemented or run; source files are untouched.

| # | Edit | Status | Where |
| --- | --- | --- | --- |
| 1 | M12 Files/Interfaces summary now states the corrected §9 protocol: two status levels, post-SIWS re-read, `retryNativeRewardLeg` with identical fields inside the verified window, definite rejections `Retryable`, expired/unknown quarantined, pinned ledger/archive allowlist, exact mint receipt | **FIXED** | Plan L684, L686 |
| 2 | Pinned PP ledger identity paired with the expected minting Account `{ owner = j56tm-oaaaa-aaaac-qf34q-cai (Shenanigans), subaccount = null }` (`dfx.json:43`); `icrc1_minting_account()` (`pp_ledger.did:553`) verified in local integration and before any receipt is accepted; mint records have no `from`, so the minter is authenticated by configuration; `from_subaccount = null` kept; wrong-minting-Account and wrong-kind tests added | **FIXED** | Spec L442; Plan L684, L688 |
| 3 | §11 heading marks DR-22 and DR-23 accepted; no phantom user decision | **FIXED** | Spec L464 |
| 4 | Only split fields `request_rent_funded_s/o` and `owner_request_rent_refundable_remaining`; `request_round_draw` assigns source amounts equal to `spent.request_rent_s/o` and leaves eligibility and refundable remaining at zero until observation; M2 interface and M7 assertions use the split fields; totals are checked sums | **FIXED** | Spec L340; Plan L168, L168, L589 |
| 5 | Closing test asserts the aggregate on `w2.ops()` | **FIXED** | Plan L321 |
| 6 | `withdraw_provider_return` requires the supplied owner destination to equal `CloseRecord.owner_refund_destination` and excludes protected destinations | **FIXED** | Spec L307 |
| 7 | Owner contribution is a physical owner → OpsCash system transfer inside the closing payout, before the lock, reverted with a failed payout; §8.6 lock row shows OpsCash `+c`, owner `−c`, free `−sponsor lock`, L `+R` with the R=2/free=1/opt-in=1 example; owner-funded close identity and rollback test added | **FIXED** | Spec L317, L381, L415; Plan L352 |
| 8 | Eligible returned rent is `funded.saturating_sub(actual fulfilled RequestAccount lamports)` after binding checks, frozen once; never a later `Rent::get()` quote; unsolicited RequestAccount credits can only lower it; only the executed return CPI credits OpsCash; rent-change and extra-lamport tests added | **FIXED** | Spec L306, L399; Plan L593, L594 |

### 13.A Agreement

I agree these eight edits complete the contracts without reopening settled design, adding scope or executing anything. The package is ready for Codex's exact delta verification and the M0–M13 local implementation go-ahead under Rob's delegation; deployment, live keys or funds, governance or service activation and PR merge remain unauthorized.

### 13.B Changed files in this revision

Edited: spec, plan, this packet and one lineage line in the inventory. Included unchanged for durable history: `native-solana-plan-final-consistency-20261013.md`. Added: `docs/solana/evidence/2026-10-13-planning-revision4-graph-{fragment,refresh}.json`. Not touched: the register, the production tranche, `/private/tmp` drafts, kanban, `solana/Cargo.toml`, source and dependency bytes. The successor commit SHA is reported in the hand-back message.
