# Musical Chairs Native Solana Implementation Roadmap

> **For agentic workers:** When approved and frozen, use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox syntax. This draft is not an execution instruction.

**Goal:** Move Musical Chairs' SOL custody and financial rules into a native Solana program while preserving the ICP-hosted GUI.

**Architecture:** Anchor/Rust program is the financial authority; existing React screens use a native transaction/account adapter. ICP retains asset hosting and, subject to PP scope, social services and an optional finalized-event mirror.

**Tech stack:** Rust/Anchor, checked integer/fixed-point math, generated IDL/client, React/TypeScript/Vite, existing Solana wallet adapters, LiteSVM or an equivalent chosen program test harness, RPC/validator integration and optional Motoko mirror. Exact compatible versions are to be pinned during the toolchain task, not invented here.

**Spec:** [2026-10-12-native-solana-design.md](../specs/2026-10-12-native-solana-design.md).

**Status:** Draft sequencing, interfaces and proposed acceptance checks. D1–D9 in the spec remain unapproved. Later tasks are conditional on their recorded decisions. A final task-level implementation plan should replace this roadmap once the economic and account contracts are frozen. No work has been kicked off.

## Global constraints

- Keep ICP frontend hosting and the recognizable GUI; no visual overhaul.
- Financial authorization comes from Solana program state and wallet signatures.
- Preserve all three plan types, Backer tiers, seniority, capped allocations and EEC unless Rob accepts explicit differences.
- Use Front-End Load, Carried Interest, Jackpot Fee and Emergency Equity Conversion. Do not rename existing `coverCharge` / `exitToll` internals as unrelated cleanup.
- Never add `@rollup/rollup-darwin-*` directly to package.json.
- Do not silently drop PP/Shenanigans/desk capabilities; declare PP scope and any explicitly deferred native feature.
- No deployment, legacy shutdown, migration transfer, mainnet funding or signing is authorized by this plan.
- Implementation occurs on an isolated `codex/` branch; preserve the current untracked agent/config files.

## Review focus

| Failure class | Expected result | Owning task |
| --- | --- | --- |
| Wrong wallet/cluster or stale signature submission | No wrong-owner position or duplicate payment; visible recoverable status | 4, 5 |
| Cap crossing and senior rotation in a toll | Exact chosen allocation oracle; overshoot remains tracked | 2, 4 |
| Pool empty or smaller than owed; min-receive causes revert | Explicit partial-payout/reset behavior; no stuck round or invented funds | 2, 4 |
| Randomness/keeper/history service outage | No biased selection; reserved claims remain usable; visible lag | 3, 6 |
| Old/new event or PP import replay | No duplicate SOL liability, PP reward or migrated claim | 6, 7 |

## Workstreams and dependencies

`1 decisions/oracle → 2 bounded economic design + 3 round design → 4 core program → 5 frontend → 6 PP/social/history → 7 verification → 8 authorized launch`

Tasks 2 and 3 can be investigated independently after task 1, with one coordinator owning the shared account contract. Frontend view-model contracts may be drafted early, but production mutations wait for the frozen IDL. A native PP workstream, if chosen, gets its own dependent spec/plan rather than being hidden inside the SOL program task.

### Task 1 — Freeze the economic and launch contract

**Owner:** coordinator with economy worker and Rob for product decisions.
**Files:** create `docs/solana/economics.md`, `docs/solana/parity-matrix.md`, `docs/solana/launch-decisions.md`, `solana/fixtures/economics.json`. These paths are proposed future artifacts, not files that exist now.
**Inputs:** spec D1–D9; current source at the pinned HEAD; D1 legacy-state audit only if migration is selected.
**Outputs:** accepted rate/fee/rounding/eligibility contract; vector corpus with inputs, timestamps, expected amounts and accepted legacy differences; a fixed scope for SOL, PP and legacy rails.

- [ ] Record fresh launch vs import and core SOL vs native PP decisions. Resolve SOL-only asset scope without assuming the old design still binds Rob.
- [ ] Record source 35/45/20 vs display 35/25/40, gross accrual base, no separate principal refund, fee payer, dust and precision choices.
- [ ] Capture vectors at 0, 1, 7, 14, 15, 21 and 30 days and one second either side of boundaries; minimum deposits; mixed and absent Backer tiers; partial/zero pot.
- [ ] Include senior-before-A-weight recalculation, top-up/merge, caps, unclaimed credit and missing-tier fold cases. Compare to current Motoko formulas and identify deliberate divergences.
- [ ] Freeze operator-account promotion eligibility, no-Backer preference/fallback, loser aggregation and payout-order behavior; classify alternate payout destinations and each manual recovery/proceeds/refund route as preserve/change/legacy-only.
- [ ] Review and commit the readable contract before implementation of production financial instructions.

Acceptance: every current financial feature is mapped to preserve / explicitly change / separate legacy rail; no executable instruction relies on an unresolved economic choice. A fresh launch needs no balance-import mechanism. A migration decision adds the separate snapshot/funding task below.

### Task 2 — Prove bounded Backer allocation and integer math

**Owner:** economy/program worker; independent review by security reviewer.
**Files:** proposed `solana/crates/economics/src/{lib.rs,earnings.rs,fees.rs,backers.rs}`, `solana/crates/economics/tests/{vectors.rs,conservation.rs,backer_transitions.rs}`, `docs/solana/backer-allocation.md`.
**Inputs:** frozen task 1 corpus.
**Outputs:** exact numeric primitives and a bounded allocation algorithm; finalized Backer/registry schema and error limits for task 4.

- [ ] Write failing vector tests for `entry_split(gross_lamports)`, simple accrual, maturity earnings, tiered fee, insolvency scaling and Backer allocation under the chosen contract.
- [ ] Use checked lamport arithmetic; separate transfers, liabilities, reserved allocations, rent and rounding residue. Compare exact outputs to the approved corpus.
- [ ] Evaluate accumulator/queue vs bounded complete allocation batches; test adversarial cap crossing, senior departure, user splitting, merges and repeated claims.
- [ ] Prove conservation and completeness for every toll. Reject any algorithm that allocates from incomplete caller-selected lists or delays closure in a way that changes denominators unnoticed.
- [ ] Measure maximum accounts/compute/data and liveness at approved population scales; document any product capacity limit before accepting it.
- [ ] Commit tests, numeric primitives and the reviewed account/allocation contract.

Acceptance: oracle agreement or explicitly accepted differences; no hidden claim loss or over-cap allocation; bounded cost and progress. If exact parity cannot be established, return a concrete redesign choice to Rob before writing that rail. Toolchain commands are recorded after task 4 pins compatible versions; this draft does not fabricate test results.

### Task 3 — Prove bounded round closure and verifiable selection

**Owner:** program/integration worker, coordinated with task 2.
**Files:** proposed `solana/crates/economics/src/rounds.rs`, `solana/crates/economics/tests/rounds.rs`, `docs/solana/round-state-machine.md`, `docs/solana/randomness.md`.
**Inputs:** task 1 eligibility/reset contract; task 2 reserve representation.
**Outputs:** explicit round phases, complete candidate representation, provider interface and permissionless progress rules.

- [ ] Write tests for aggregate loser selection across winning/losing and closed/open positions, preference/fallback, no candidates and duplicate traversal.
- [ ] Maintain per-player P/L incrementally and specify a complete bounded registry; current-round validity must invalidate old positions without mutating all history.
- [ ] Freeze eligible membership before randomness request; verify provider fulfillment on-chain and request uniqueness; test adversarial requests/slots/operator choices.
- [ ] Choose retry-with-frozen-candidates or disclosed close-without-promotion on sustained randomness failure, rather than assuming current raw_rand skip behavior is preserved. Define timeout/retry, pending-promotion/new-round ordering, fee budget, keeper incentive and permissionless recovery. No biased timeout fallback.
- [ ] Prove pending round work cannot block previously funded Backer claims and that a zero-pot/min-receive revert cannot leave an unrecoverable closing state.
- [ ] Commit reviewed state transitions, schemas, tests and boundedness measurements.

Acceptance: complete set, verifiable winner selection, at most one promotion and exactly one seed carry per round, bounded progress with keeper/provider outages under the selected failure policy. A particular provider or randomness replacement is not yet accepted.

### Task 4 — Implement the native core program

**Owner:** program worker; coordinator owns account/IDL integration.
**Files:** proposed `solana/Anchor.toml`, `solana/Cargo.toml`, `solana/programs/musical_chairs/src/{lib.rs,state.rs,error.rs,events.rs,instructions/*}`, `solana/tests/{authority,positions,backers,rounds,conservation}.*`, CI configuration scoped to this workspace.
**Inputs:** frozen task 1–3 contracts. **Outputs:** reviewed account layouts, IDL, generated client and locally verified financial program.

- [ ] Pin mutually compatible Anchor/Rust/Solana/testing/client versions; create workspace and focused CI with the first failing account-constraint tests.
- [ ] Implement authenticated initialization, canonical vault/rent partition and narrow authority roles; verify no first-caller takeover or production test-credit instructions. Freeze per-account rent payer, minimum balance, recorded refund recipient, close authority and liability-completion conditions; test rent refunds never become pot or management revenue and candidate-page rent budget cannot stall progress.
- [ ] Implement atomic `open_position(plan, gross_lamports, client_nonce)` and `fund_series_a(gross_lamports)` with exact fees, minima and owner/rate-limit rules.
- [ ] Implement `withdraw_simple(position, minimum_receive_lamports, destination)`, `settle_compounding(position, minimum_receive_lamports, destination)` and `claim_backer(series, destination)` against the frozen algorithms; preserve owner-authorized alternate destinations and test substitution/replay.
- [ ] Implement bounded closing/selection/finalization and management/config controls; preserve versioned terms on existing positions.
- [ ] Test wrong signer, substituted PDA, wrong account owner, duplicate mutable aliases, reinitialization, arithmetic overflow, replay, double claims, failure rollback and unauthorized treasury/config changes.
- [ ] Run chosen local program harness, compare all economics vectors and measure worst-case compute/account costs; generate and verify IDL/client from the tested program.
- [ ] Commit reviewed source, tests and reproducible artifact/build instructions.

Acceptance: all core behavior, including Backers/EEC, passes source, economic and security checks. Deposit/withdraw-only output is an intermediate prototype. No deployment readiness is inferred from passing local tests.

### Task 5 — Connect the existing GUI to native transactions

**Owner:** frontend worker; financial contract comes from task 4.
**Files:** proposed `frontend/src/solana/native/{client.ts,transactions.ts,accounts.ts,status.ts,types.ts}`, focused tests, `frontend/src/hooks/useNativeSolana.ts`; adapt `useWallet.tsx`, SOL sections of `useQueries.ts`, `SolInvestPanel.tsx`, `SolBackerPanel.tsx`, `GameTracking.tsx`, `HouseDashboard.tsx`, `WalletDropdown.tsx`, `.ic-assets.json` only as needed.
**Inputs:** generated IDL/client; canonical account data. **Outputs:** preserved screens with wallet-signed financial actions and direct canonical reads.

- [ ] Define view models using bigint lamports and wallet pubkey; keep ICP principal for social services separately. Include cluster/program ID in cache keys.
- [ ] Write failed/rejected/expired/unknown transaction and wallet-switch tests; preserve submission identity so an RPC timeout cannot trigger a duplicate opening.
- [ ] Build simulation → wallet signing → submission → confirmed/finalized flow; display signature, fee/rent and minimum receive in current panels.
- [ ] Replace canister prepare/poke/derived-address/manual-transfer behavior on the native rail; preserve legacy recovery separately if launch policy requires it.
- [ ] Read positions, Backer claims, pot and round validity from Solana; stale mirror state cannot hide a valid payout or authorize an invalid one.
- [ ] Verify HTTPS confirmation polling under ICP CSP, RPC failures, wallet rejection, insufficient SOL for fees/rent, wrong cluster and session expiry.
- [ ] Run `npm test` and `npm run build`; browser-compare major screens to a captured baseline across existing supported viewports and wallets.

Acceptance: same recognizable layout/branding and complete core financial flows; only necessary transaction/status/chain changes. Existing ICP/PP paths remain valid or are visibly scoped by the accepted product decision.

### Task 6 — Restore histories and the shared PP/social experience

**Owner:** ICP/integration worker; depends on PP decision and finalized events.
**Files:** candidate `solana_indexer/main.mo` plus small source modules and tests, `shenanigans/main.mo` observer adapter, `frontend/src/hooks/useNativeHistory.ts`, feed/admin history views, `dfx.json` only if an ICP mirror is accepted.
**Inputs:** task 4 versioned events with transaction/instruction/event identity; SIWS ownership mapping; task 1 PP/desk decision.
**Outputs:** correct histories/PP integration with a disclosed trust boundary, or native PP subproject.

- [ ] Choose event transport and durable archive/provider recovery; logs alone must not be assumed complete. Track finality, progress cursor, duplicates, gaps, schema version, lag and provider disagreements.
- [ ] Make finalized-event ingestion idempotent and bounded; validate program, instruction/accounts and source data. Name the RPC/verification trust assumptions; do not label ordinary RPC ingestion trustless.
- [ ] If PP stays on ICP, mint rewards once for actual event-owner wallets bound to canonical SIWS principals. Validate signed challenge/domain/expiry, prohibit operator-chosen recipients, freeze replay-safe binding/recovery policy and keep historic event ownership fixed across rebinds. Preserve referral/Backer/signup semantics and prevent old/new event namespace collisions.
- [ ] Resolve the desk explicitly: accepted cross-chain purchase/refund protocol or a visibly deferred native desk. Test stuck inventory, expired receipts, refund/replay and no mixed proceeds/pot accounting.
- [ ] If PP is native instead, stop this branch of the roadmap and prepare its separate mint/chips/rewards/desk/spells migration spec and plan; integrate only after its acceptance checks pass.
- [ ] Verify history outage leaves direct core SOL reads/transactions available, and display lag without implying financial failure.

Acceptance: no duplicate PP reward, no fabricated SOL balances, correct social identity mapping, declared desk behavior and recoverable historical gaps. An ICP indexer is optional; preserving existing user-visible history remains a product requirement that can be served by another reviewed solution.

### Task 7 — Devnet acceptance, reconciliation and independent review

**Owner:** coordinator; independent program/security and frontend reviewers.
**Files:** proposed `docs/solana/acceptance.md`, `docs/solana/threat-model.md`, `docs/solana/reconciliation.md`, bounded read-only receipt tooling.
**Inputs:** tested core/client/social artifacts. **Outputs:** reproducible evidence and an explicit release recommendation.

- [ ] Review custody, signer/account checks, upgrade/config powers, economic conservation, candidate completeness and transaction-order exposure.
- [ ] With separate deployment/test-wallet authorization, run devnet deposit, simple payout, both compound settlements, Series A funding/claim, cap completion, partial payout and reset/promotion.
- [ ] Reconcile physical balances, protected rent, pot, seed, funded claims, management and pending escrows after each action. Verify transaction signatures/finality independently of GUI/mirror.
- [ ] Exercise RPC/indexer outages, randomness delay, stopped keeper, stale browser/session, duplicate send, and full population limits.
- [ ] Verify compiled program/IDL/client alignment; review ICP hosting build and major GUI parity. Served production bytes are a later release check, not local build evidence.
- [ ] Record accepted findings, exact unresolved issues, source commit/build hashes, tested account sizes/costs and authority configuration; no invented timing or audit-budget estimate.

Acceptance: critical financial and liveness issues resolved, core and selected PP scope covered by receipts/tests, and independent review completed. External audit availability/cost should be obtained rather than assumed; agree review scope before relying on it.

### Conditional migration task — only if D1 selects importing existing state

**Owner:** coordinator/operator and separate migration reviewer.
**Files:** proposed `docs/solana/migration.md`, signed snapshot manifest, versioned import/claim tooling and reconciliation fixtures.

- [ ] Read current deployed state and finalized source-asset balances without changing them; inventory all liability/reserve/desk/pending/refund buckets.
- [ ] Establish wallet ownership and II/SIWS mapping, cutoff, valuation if ICP converts to SOL, funded escrow and anti-double-claim rules.
- [ ] Prove snapshot completeness, rounding/term carryover, paused/pending handling and rollback before funds leave the legacy rail.
- [ ] Rehearse imports/claims against a nonproduction snapshot and record zero reconciliation drift.
- [ ] Obtain separate approval for actual pause/import/transfer/retirement actions with pushed readable artifacts and exact hashes.

No production canister can be classified withdraw-only without checking its actual capabilities. A fresh launch instead documents legacy withdrawal and recovery availability without pretending funds were migrated.

### Task 8 — Separately authorized launch

**Owner:** Rob/operator; coordinator prepares evidence.
**Files:** proposed `docs/solana/launch-runbook.md`, artifact and authority manifest, monitoring/rollback plan.

- [ ] Publish the reviewed source, reproducible build, program/cluster and chosen authority manifest; publish the explicit scope and fee/insolvency disclosures.
- [ ] Present concrete deployment, frontend routing, initial liquidity and legacy handling actions for approval; do not infer fund or shutdown authority from a merged PR.
- [ ] After authorization, deploy verified program bytes, verify actual authorities/accounts, and deploy ICP frontend assets from the approved build.
- [ ] With separately authorized funds, reconcile initial pool partitions and verify a bounded live lifecycle with finalized receipts and balances.
- [ ] Monitor reserve mismatch, keeper/draw delay, failed transactions, RPC/mirror lag and frontend served version; keep recovery instructions accessible.

Acceptance: installed bytes and authority state match reviewed artifacts; selected financial lifecycle has actual finalized receipts and reconciled balances. Source, tests, CI, merge, deployment, funding, finality and displayed balances are separate evidence.

## Cost-aware execution recommendation

Use cheaper native workers for bounded inventory, vectors, generated clients and frontend adaptations. External Sonnet program work/review is an option only after provider-specific repository-disclosure authorization and authenticated capability are both confirmed; the rejected review attempt grants neither authorization nor a workaround. Keep the coordinator responsible for shared financial contracts and integration. Do not fan out overlapping account-layout/IDL edits. Increase model capability only for a demonstrated unresolved problem.

No reliable calendar/cost estimate is available before PP/legacy scope, exact Backer algorithm, randomness provider, population limits and audit arrangement are fixed. The first execution milestone is a reviewed oracle and bounded-design proof, not a mainnet date.
