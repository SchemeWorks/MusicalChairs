# Musical Chairs Native Solana Implementation Roadmap

> **For agentic workers:** When approved and frozen, use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox syntax. This draft is not an execution instruction.

**Goal:** Move Musical Chairs' SOL custody and financial rules into a native Solana program while preserving the ICP-hosted GUI.

**Architecture:** Anchor/Rust program is the native SOL game's financial authority; existing React screens use a native transaction/account adapter. ICP retains asset hosting, PP, Shenanigans and social services, with finalized native events feeding ICP rewards/history. The new pool starts fresh; legacy positions retain separate withdrawal/recovery access.

**Tech stack:** Rust/Anchor, checked integer/fixed-point math, generated IDL/client, React/TypeScript/Vite, existing Solana wallet adapters, LiteSVM or an equivalent chosen program test harness, RPC/validator integration and optional Motoko mirror. Exact compatible versions are to be pinned during the toolchain task, not invented here.

**Spec:** [2026-10-12-native-solana-design.md](../specs/2026-10-12-native-solana-design.md).

**Economic/account contract recommendation:** [native-solana-economics.md](../../coordination/native-solana-economics.md). The second cheaper-agent pass recommends concrete formulas, complete allocation tickets and a sealed round/draw lifecycle. This plan implements that design only after its intentional precision/fee/availability differences are accepted.

**Status:** Revised draft sequencing, candidate interfaces and proposed acceptance checks. Rob confirmed D1 fresh launch with separate legacy withdrawals and D2 SOL game first with PP/Shenanigans on ICP. D4–D7 now have concrete recommendations; acceptance, measured execution costs and provider lifecycle proof remain open. Governance D8 and operational sizing D9 remain decisions. No product implementation has been kicked off.

## Global constraints

- Keep ICP frontend hosting and the recognizable GUI; no visual overhaul.
- Financial authorization comes from Solana program state and wallet signatures.
- Preserve all three plan types, Backer tiers, seniority, capped allocations and EEC unless Rob accepts explicit differences.
- Use Front-End Load, Carried Interest, Jackpot Fee and Emergency Equity Conversion. Do not rename existing `coverCharge` / `exitToll` internals as unrelated cleanup.
- Never add `@rollup/rollup-darwin-*` directly to package.json.
- Keep PP/Shenanigans on ICP and the existing PP desk on its separately identified ICP/Chain Fusion rail. Native PP/desk replacement is deferred.
- Start a fresh native pool; do not import balances/positions or move legacy reserves. Preserve separately labeled legacy withdrawals and recovery.
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

`1 accept economic contract/oracle → 2 integer/allocation model + 3 round/provider contract → 4 bounded native core → 5 preserved GUI → 6 ICP PP/social/history → 7 verification → 8 authorized launch`

Tasks 2 and 3 can be investigated independently after task 1, with one coordinator owning the shared account contract. Frontend view-model contracts may be drafted early, but production mutations wait for the frozen IDL. Native PP is deferred; this roadmap includes ICP reward/history integration and separate legacy access, not token migration.

### Task 1 — Freeze the economic and launch contract

**Owner:** coordinator with economy worker and Rob for product decisions.
**Files:** create `docs/solana/economics.md`, `docs/solana/parity-matrix.md`, `docs/solana/launch-decisions.md`, `solana/fixtures/economics.json`. These paths are proposed future artifacts, not files that exist now.
**Inputs:** confirmed D1/D2 and open D3–D9; current source at the pinned HEAD; source/deployed capability evidence for separate legacy access.
**Outputs:** accepted rate/fee/rounding/eligibility contract; vector corpus with inputs, timestamps, expected amounts and accepted legacy differences; a fixed scope for SOL, PP and legacy rails.

- [x] Record Rob's fresh launch and SOL-game-first decisions, preserving separate legacy withdrawals and ICP PP/Shenanigans. No imports or native PP migration.
- [ ] Resolve SOL-only asset scope without assuming the old design still binds Rob; document the source and availability of separate legacy payout/recovery routes.
- [ ] Accept source 35/45/20 and gross accrual; fix identified UI drift later. Record no principal refund, 1-lamport arithmetic, remainder carry, floor Q18 factors, wallet-paid fees, >10-lamport Backer-open cutoff and the tightened atomic quota as explicit native differences.
- [ ] Capture vectors at 0, 1, 7, 14, 15, 21 and 30 days and one second either side of boundaries; minimum deposits; mixed and absent Backer tiers; partial/zero pot.
- [ ] Include senior-before-A-weight recalculation, top-up/merge, caps, unclaimed credit and missing-tier fold cases. Compare to current Motoko formulas and identify deliberate divergences.
- [ ] Freeze operator-account promotion eligibility, no-Backer preference/fallback, loser aggregation and payout-order behavior; classify alternate payout destinations and each manual recovery/proceeds/refund route as preserve/change/legacy-only.
- [ ] Accept complete allocation tickets and their processing barrier; accept one bound durable draw with no reroll/automatic skip and its oracle-outage availability cost. Record governance/operations limits without inventing a population cap or completion-time promise.
- [ ] Review and commit the readable contract before implementation of production financial instructions.

Acceptance: every current financial feature is mapped to preserve / explicitly change / separate legacy rail; no executable instruction relies on an unresolved economic choice. Fresh launch has no import mechanism or access to legacy reserves. Legacy withdrawal access does not imply verified solvency.

### Task 2 — Prove bounded Backer allocation and integer math

**Owner:** economy/program worker; independent review by security reviewer.
**Files:** proposed `solana/crates/economics/src/{lib.rs,earnings.rs,fees.rs,backers.rs,allocation.rs}`, `solana/crates/economics/tests/{vectors.rs,conservation.rs,backer_transitions.rs,allocation.rs}`, `docs/solana/backer-allocation.md`.
**Inputs:** frozen task 1 corpus.
**Outputs:** exact numeric primitives and a bounded allocation algorithm; finalized Backer/registry schema and error limits for task 4.

- [ ] Write failing vector tests for `entry_split(gross_lamports)`, simple accrual, maturity earnings, tiered fee, insolvency scaling and Backer allocation under the chosen contract.
- [ ] Implement the accepted formulas and width table: u64 cash, u128 entitlements/credited/cumulative P&L/global funded claims, fixed U256 headroom sums/pro-rata division; floor entry fee, Simple remainder, Q18 factors and actual-gross fee. Test safe loss subtraction for net winners and near-limit A/B entitlement growth; prospective B grants must fit before deposits are accepted. Include backwards time, >u64 owed and zero earnings at expiry.
- [ ] Model Discover → Senior → SealWeights → Allocate → Prune → Commit. Initial membership determines tier folds; derive A denominator/weights after senior credit without a third full scan. Floor each slice and route all percentage/division/cap residue to seed.
- [ ] Test no Backers, only A/B, senior caps all A, senior leaves 1–10 lamports, A/B caps, reopens with original seniority, joint A+B claims, and user splitting. Reject generic reward-index substitution without a new equivalence proof.
- [ ] Model scalar finalized-ticket watermark and lazy local credit materialization. Test old funded claims during processing, no pending-credit spend, no global double funding and no lifetime-credit decrease on claim.
- [ ] Prove conservation at each partial phase and complete-cursor terminal commit. Snapshot/read state cannot change through top-up, game payout or term updates during the barrier.
- [ ] Measure maximum accounts/compute/data and liveness at approved population scales; document any product capacity limit before accepting it.
- [ ] Commit tests, numeric primitives and the reviewed account/allocation contract.

Acceptance: exact agreement with the accepted integer oracle; no hidden claim loss or over-cap allocation; no unbounded finalizer; all residue tracked. This is a model/contract proof, not SVM latency proof. If economic parity fails, return a concrete redesign choice before building that rail. Toolchain commands are recorded after task 4 pins compatible versions; this draft does not fabricate test results.

### Task 3 — Prove bounded round closure and verifiable selection

**Owner:** program/integration worker, coordinated with task 2.
**Files:** proposed `solana/crates/economics/src/rounds.rs`, `solana/crates/economics/tests/rounds.rs`, `docs/solana/round-state-machine.md`, `docs/solana/randomness.md`.
**Inputs:** task 1 eligibility/reset contract; task 2 reserve representation.
**Outputs:** explicit round phases, complete candidate representation, provider interface and permissionless progress rules.

- [ ] Write tests for aggregate loser selection across winning/losing and closed/open positions, preference/fallback, no candidates and duplicate traversal.
- [ ] Define unique current-round participants/UserRound P&L and persistent native A/B-existence flags, including closed records. Seal complete program-owned preferred/fallback lists, preserve all-position net-loss aggregation and verify direct random-ordinal lookup.
- [ ] Specify Active → AllocatingClosingToll → CountingCandidates → AwaitingRandomness → PromotionApplied → ResetReady → Active(next). No candidates skip the draw normally; no next-round funding before terminal promotion/seed carry.
- [ ] Evaluate ORAO registered-client interface first, Switchboard as alternative. Pin provider program/SDK/owner and namespace; test native RNG-State-PDA request signer, canonical registered client, upgrade/client/callback controls, fees/rent, fulfilled record retention and recovery. Registration needs separately authorized deployment/operator actions later; documents alone do not prove suitability.
- [ ] Implement the companion's fixed-width domain-separated seed and `NotRequested → Requested → Captured` marker; CPI request creation and marker are atomic. Test duplicate delivery, unauthorized precreation/foreign client, unexpected existing account fail-closed, and rejection of arbitrary seeds/prefulfilled requests; no collision fallback creates a new draw.
- [ ] Test one unique sealed request with repeated delivery/settlement only, no reroll or outcome-sensitive timeout skip. If using a reveal-slot provider, capture the verified value in the reveal transaction before later progress. Rejection sampling must continue from the same output.
- [ ] Prove claims/progress stay callable during pauses and oracle/keeper outage. Record that permanent provider outage can pause the next round indefinitely, and agree recovery/governance policy rather than assuming finite progress.
- [ ] Preserve strict `owed > pot` and equality/refill behavior. Owner-authorized eligible positive-earnings/matured zero-pot claim with minimum zero starts close; no keeper/timer closes an initial empty pool. A minimum failure reverts and exposes an explicit owner retry route.
- [ ] Commit reviewed state transitions, schemas, tests and boundedness measurements.

Acceptance: complete set, authenticated bound result, at most one promotion and exactly one terminal seed carry; bounded per-instruction work and permissionless resumption. Provider outage is explicitly not a finite-liveness guarantee. A provider adapter is not accepted until the request lifecycle/authentication checks pass.

### Task 4 — Implement the native core program

**Owner:** program worker; coordinator owns account/IDL integration.
**Files:** proposed `solana/Anchor.toml`, `solana/Cargo.toml`, `solana/programs/musical_chairs/src/{lib.rs,state.rs,error.rs,events.rs,instructions/*}`, `solana/tests/{authority,positions,backers,rounds,conservation}.*`, CI configuration scoped to this workspace.
**Inputs:** frozen task 1–3 contracts. **Outputs:** reviewed account layouts, IDL, generated client and locally verified financial program.

- [ ] Pin mutually compatible Anchor/Rust/Solana/testing/client versions; create workspace and focused CI with the first failing account-constraint tests.
- [ ] Implement authenticated initialization, canonical vault/rent partition and narrow authority roles; verify no first-caller takeover or production test-credit instructions. Freeze per-account rent payer, minimum balance, recorded refund recipient, close authority and liability-completion conditions; test rent refunds never become pot or management revenue and candidate-page rent budget cannot stall progress.
- [ ] Implement atomic `open_position(plan, gross_lamports, client_nonce)` and `fund_series_a(gross_lamports)` with exact fees, minima and owner/rate-limit rules.
- [ ] Implement `withdraw_simple(position, minimum_receive_lamports, destination)` and `settle_compounding(position, minimum_receive_lamports, destination)` as atomic accepted net payment + toll escrow + Position/P&L change + allocation ticket; reserve full work/rent budget before acceptance. Preserve owner-authorized destinations and simulate failed transfer/minimum rollback.
- [ ] Implement dense active Backer PDA pages, captured-version cursor traversal, immutable snapshots, bounded descending-slot compaction and moved-slot validation. Retain lifetime/first-funding records and permanent existence flags. No historical Backer scan per toll or unbounded commit.
- [ ] Implement constant-size ticket commit and lazy funded credit, plus combined `claim_backer(destination)` using the owner's expected A/B accounts. Claim during allocation cannot spend pending toll; lazy materialization cannot add global funded totals twice.
- [ ] Implement permissionless allocation/closing/list/draw/seed progress, operations-funded keeper incentives and protected claims/progress during pause; preserve versioned terms. No abort shortcut, arbitrary credits, winner selection or automatic draw skip.
- [ ] Test wrong signer, substituted PDA, wrong account owner, duplicate mutable aliases, reinitialization, arithmetic overflow, replay, double claims, failure rollback and unauthorized treasury/config changes.
- [ ] Run chosen local program harness, compare all economics vectors and measure compute/accounts/rent/total transaction cost/latency at 64/256/1,024 active Backers and participants, including growth, tiny-claim operations-budget griefing and adversarial churn. Preserve zero-toll/all-rounded-slices-zero fast paths without adding a hidden minimum/cooldown. Eight rows per chunk is a starting candidate, not a proven limit. Agree a launch performance envelope; return to design if it fails, rather than silently adding a capacity cap. Generate/verify IDL/client.
- [ ] Commit reviewed source, tests and reproducible artifact/build instructions.

Acceptance: all core behavior, including Backers/EEC, passes source, economic and security checks. Deposit/withdraw-only output is an intermediate prototype. No deployment readiness is inferred from passing local tests.

### Task 5 — Connect the existing GUI to native transactions

**Owner:** frontend worker; financial contract comes from task 4.
**Files:** proposed `frontend/src/solana/native/{client.ts,transactions.ts,accounts.ts,status.ts,types.ts}`, focused tests, `frontend/src/hooks/useNativeSolana.ts`; adapt `useWallet.tsx`, SOL sections of `useQueries.ts`, `SolInvestPanel.tsx`, `SolBackerPanel.tsx`, `GameTracking.tsx`, `HouseDashboard.tsx`, `WalletDropdown.tsx`, `.ic-assets.json` only as needed.
**Inputs:** generated IDL/client; canonical account data. **Outputs:** preserved screens with wallet-signed financial actions and direct canonical reads.

- [ ] Define view models using bigint lamports and wallet pubkey; keep ICP principal for social services separately. Include cluster/program ID in cache keys.
- [ ] Write failed/rejected/expired/unknown transaction and wallet-switch tests; preserve submission identity so an RPC timeout cannot trigger a duplicate opening.
- [ ] Build simulation → wallet signing → submission → confirmed/finalized flow; display signature, fee/rent and minimum receive in current panels. Distinguish finalized user payout, pending Backer allocation and closing round; disable busy financial actions while keeping funded claims available. Users need no signature per keeper chunk.
- [ ] Correct SOL gross-base ROI previews and 35/45/20 senior/proportional/per-B text from the accepted contract, retaining existing component layout and styling.
- [ ] Replace canister prepare/poke/derived-address/manual-transfer behavior on the native rail; preserve separately labeled legacy position withdrawal, settlement, Backer claim and recovery actions. Test the same wallet holding both legacy and native positions without cross-credit or mistaken rail selection.
- [ ] Read positions, Backer claims, pot and round validity from Solana; stale mirror state cannot hide a valid payout or authorize an invalid one.
- [ ] Verify HTTPS confirmation polling under ICP CSP, RPC failures, wallet rejection, insufficient SOL for fees/rent, wrong cluster and session expiry.
- [ ] Run `npm test` and `npm run build`; browser-compare major screens to a captured baseline across existing supported viewports and wallets.

Acceptance: same recognizable layout/branding and complete core financial flows; only necessary transaction/status/chain changes. Existing ICP/PP paths remain valid or are visibly scoped by the accepted product decision.

### Task 6 — Restore histories and the shared PP/social experience

**Owner:** ICP/integration worker; depends on PP decision and finalized events.
**Files:** candidate `solana_indexer/main.mo` plus small source modules and tests, `shenanigans/main.mo` observer adapter, `frontend/src/hooks/useNativeHistory.ts`, feed/admin history views, `dfx.json` only if an ICP mirror is accepted.
**Inputs:** task 4 versioned events with transaction/instruction/event identity; SIWS ownership mapping; confirmed ICP PP/Shenanigans scope and separate legacy desk.
**Outputs:** correct histories and ICP PP integration with a disclosed trust boundary, retaining existing social/desk capabilities.

- [ ] Choose event transport and durable archive/provider recovery; logs alone must not be assumed complete. Track finality, progress cursor, duplicates, gaps, schema version, lag and provider disagreements.
- [ ] Make finalized-event ingestion idempotent and bounded; validate program, instruction/accounts and source data. Name the RPC/verification trust assumptions; do not label ordinary RPC ingestion trustless.
- [ ] Add native finalized-event rewards alongside existing legacy observers; mint once for actual event-owner wallets bound to canonical SIWS principals. Validate signed challenge/domain/expiry, prohibit operator-chosen recipients, freeze replay-safe binding/recovery policy and keep historic event ownership fixed across rebinds. Preserve referral/Backer/signup semantics and prevent old/new event namespace collisions or duplicate signup grants.
- [ ] Preserve the existing ICP/Chain Fusion PP desk purchase/proceeds/refund flow, clearly labeled and isolated from the native game pool. Verify legacy desk transactions cannot create native Positions, and native pool funds cannot pay desk refunds or proceeds. Native desk/escrow rewrite remains optional deferred work.
- [ ] Verify history outage leaves direct core SOL reads/transactions available, and display lag without implying financial failure.

Acceptance: no duplicate PP reward, no fabricated SOL balances, correct social identity mapping, declared desk behavior and recoverable historical gaps. An ICP indexer is optional; preserving existing user-visible history remains a product requirement that can be served by another reviewed solution.

### Task 7 — Devnet acceptance, reconciliation and independent review

**Owner:** coordinator; independent program/security and frontend reviewers.
**Files:** proposed `docs/solana/acceptance.md`, `docs/solana/threat-model.md`, `docs/solana/reconciliation.md`, bounded read-only receipt tooling.
**Inputs:** tested core/client/social artifacts. **Outputs:** reproducible evidence and an explicit release recommendation.

- [ ] Review custody, signer/account checks, upgrade/config powers, economic conservation, candidate completeness and transaction-order exposure.
- [ ] With separate deployment/test-wallet authorization, run devnet deposit, simple payout, both compound settlements, Series A funding/claim, cap completion, partial payout and reset/promotion.
- [ ] Reconcile physical balances, protected rent, pot, seed, funded claims, management and pending escrows after each action. Verify transaction signatures/finality independently of GUI/mirror.
- [ ] Exercise RPC/indexer outages, randomness delay/permanent nonfulfillment, stopped keeper, interrupted compaction and lazy claims, stale browser/session, duplicate send, and measured population envelopes. No test receipt may imply completion under an indefinitely failed randomness provider.
- [ ] Verify compiled program/IDL/client alignment; review ICP hosting build and major GUI parity. Served production bytes are a later release check, not local build evidence.
- [ ] Record accepted findings, exact unresolved issues, source commit/build hashes, tested account sizes/costs and authority configuration; no invented timing or audit-budget estimate.

Acceptance: critical financial and liveness issues resolved, core and selected PP scope covered by receipts/tests, and independent review completed. External audit availability/cost should be obtained rather than assumed; agree review scope before relying on it.

### Legacy access work — preserve separate withdrawals

**Owner:** coordinator and frontend/integration reviewer; verify within tasks 1, 5 and 7 rather than as a balance-migration project.
**Files:** proposed `docs/solana/legacy-access.md`; existing position, claim and recovery views only where needed to distinguish rails.

- [ ] Verify actual deployed withdrawal, maturity settlement, Backer claim and deposit/desk recovery capabilities without mutating live state.
- [ ] Preserve the correct ICP or SIWS identity and payout destination for each legacy rail; new native wallet state cannot overwrite the legacy account selection.
- [ ] Keep legacy histories, pending transfers and recovery accessible; do not present old positions as native PDA accounts or promise guaranteed liquidity.
- [ ] Test a user with simultaneous legacy and native positions, independent balances/rounds, and failed or unavailable legacy payouts without affecting native access.
- [ ] Document any proposed future legacy deposit pause/retirement as separately authorized operations. No balance import, reserve transfer or shutdown is included.

No production canister can be classified withdraw-only without checking its actual capabilities. Preserved withdrawal access is a required part of the selected product scope; verified solvency and actual payout receipts remain separate evidence.

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

PP/legacy scope is confirmed and complete batches are the recommended Backer design. No reliable calendar/cost estimate is available before acceptance of precision/availability tradeoffs, provider authentication/recovery, measured active-population costs and audit arrangement. The first future execution milestone is the accepted oracle and bounded model, not a mainnet date. Numeric design examples have been checked outside any program runtime; no implementation test has yet passed.
