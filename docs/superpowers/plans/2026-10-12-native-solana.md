# Musical Chairs Native Solana Implementation Roadmap

> Update 2026-10-12: Rob accepted the defaults and authorized only the Rust economic model plus local provider/resource probes. [Acceptance and scope](../../solana/launch-decisions.md). Earlier discovery status/evidence paragraphs are historical; no deployment or live financial action is authorized.
> **For agentic workers:** When approved and frozen, use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox syntax. The accepted host-model/local-probe subset is in execution; later production tasks remain a roadmap.

**Goal:** Move Musical Chairs' SOL custody and financial rules into a native Solana program while preserving the ICP-hosted GUI.

**Architecture:** Anchor/Rust program is the native SOL game's financial authority; existing React screens use a native transaction/account adapter. ICP retains asset hosting, PP, Shenanigans and social services, with finalized native events feeding ICP rewards/history. The new pool starts fresh; legacy positions retain separate withdrawal/recovery access.

**Tech stack:** Rust/Anchor; initial provider candidate Anchor 0.32.1 plus ORAO Callback SDK 0.4.0 CPI-only; checked integer math; generated IDL/client; existing React/TypeScript/Vite/wallet-adapter/web3.js v1 and legacy/v0 transactions. Matched LiteSVM/Surfpool and exact compatible CLI/Rust/client versions are pinned/proved at workspace setup.

**Spec:** [2026-10-12-native-solana-design.md](../specs/2026-10-12-native-solana-design.md).

**Economic/account contract recommendation:** [native-solana-economics.md](../../coordination/native-solana-economics.md). The second cheaper-agent pass recommends concrete formulas, complete allocation tickets and a sealed round/draw lifecycle. This plan implements that design only after its intentional precision/fee/availability differences are accepted.

**Current availability contract:** [processing and randomness recovery](../../coordination/native-solana-availability.md). Rob rejects indefinite oracle-stuck rounds; fixed close+600-second promotion expiry supersedes indefinite oracle wait. [Recommended defaults](../../coordination/native-solana-defaults.md) adds zero on-chain keeper rewards, sponsored storage/provider/B rent, 2-of-3/48h governance and explicit bridge trust. [Foundation refresh receipt](../../coordination/solana-skill-assessment.md) is verified; no GUI overhaul.

**Current status:** Rob accepted D1–D9 and authorized the host Rust economic/allocation/round models and local provider/resource probes. These are implemented on `codex/native-solana-model-probes` with focused tests and independent review. They do not implement native custody or production instructions. SBF/runtime compute, account constraints, provider lifecycle and launch identities/capital/endpoints remain later work. The older research paragraphs below describe the evidence available at discovery time.

**Keeper/budget investigation:** [ICP placement and pilot budget](../../coordination/native-solana-icp-budget.md) uses Rob's ~100 active Backers/1,000 payouts/10 closes monthly. Recommend external fee-only primary with ICP monitor/emergency backup on current modeled signing cost; fully ICP is viable after batching/cycle/latency review. Conditional 12 SOL startup and 0.5–1.5 SOL monthly exclude ICP cycles, subscriptions, audit/development and game liquidity. Actual compiled/storage sizes remain unmeasured.

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
- Proposed launch defaults: public Series A on, initial pot/seed zero; no privileged/excluded operator wallets or manual credits. Owner pays personal/A rent; sponsor pays shared storage and automatic B rent. Work contribution max defaults zero; on-chain keeper reward zero.
- Governance default: autonomous Squads v4 2-of-3/48h, zero config-authority sentinel/no spending-limit bypass; separate immediate 2-of-3 admission guardian. Pinned active terms/close/provider tuple survive prospective changes.
- Preserve native PP basis and existing ICP policies through a durable per-leg outbox; ordinary two-RPC observation is trusted infrastructure, not consensus proof. Native payouts never depend on it.
- ICP keeper keys hold capped fee balances only; no user/pool/G/management/provider/operations-withdrawal role or arbitrary-byte signing endpoint. Hosting choice never changes public recovery permissions or the close deadline.

## Review focus

| Failure class | Expected result | Owning task |
| --- | --- | --- |
| Wrong wallet/cluster or stale signature submission | No wrong-owner position or duplicate payment; visible recoverable status | 4, 5 |
| Cap crossing and senior rotation in a toll | Exact chosen allocation oracle; overshoot remains tracked | 2, 4 |
| Pool empty or smaller than owed; min-receive causes revert | Explicit partial-payout/reset behavior; no stuck round or invented funds | 2, 4 |
| Randomness/keeper/history service outage | No reroll or caller-selected winner; explicit promotion omission risk; reserved claims remain usable; visible lag | 3, 6 |
| Old/new event or PP import replay | No duplicate SOL liability, PP reward or migrated claim | 6, 7 |

## Workstreams and dependencies

**Knowledge graph throughout implementation:** follow [graph maintenance](../../coordination/native-solana-knowledge-graph.md) after each coherent code/document change. Structural Rust/TypeScript extraction and semantic contract extraction are separate; verify scoped source hashes, preserve existing graph relationships, and check queries for the affected mechanism before each workstream handoff. Record stale Motoko/document coverage honestly. Graph maintenance supports navigation and does not replace financial tests or imply deployment proof. Explain relevant Solana concepts at the point of each design/implementation choice, using Rob's ICP/EVM background.

`1 accept economic contract/oracle → 2 integer/allocation model + 3 round/provider contract → 4 bounded native core → 5 preserved GUI → 6 ICP PP/social/history → 7 verification → 8 authorized launch`

Tasks 2 and 3 can be investigated independently after task 1, with one coordinator owning the shared account contract. Frontend view-model contracts may be drafted early, but production mutations wait for the frozen IDL. Native PP is deferred; this roadmap includes ICP reward/history integration and separate legacy access, not token migration.

### Task 1 — Freeze the economic and launch contract

**Owner:** coordinator with economy worker and Rob for product decisions.
**Files:** create `docs/solana/economics.md`, `docs/solana/parity-matrix.md`, `docs/solana/launch-decisions.md`, `solana/fixtures/economics.json`. These artifacts now exist in the accepted host-model pass.
**Inputs:** accepted D1–D9 defaults register; current source at pinned HEAD. Legacy source mapping here, deployed capability evidence at later acceptance/launch.
**Outputs:** accepted rate/fee/rounding/eligibility contract; vector corpus with inputs, timestamps, expected amounts and accepted legacy differences; a fixed scope for SOL, PP and legacy rails.

- [x] Record Rob's fresh launch and SOL-game-first decisions, preserving separate legacy withdrawals and ICP PP/Shenanigans. No imports or native PP migration.
- [x] Record proposed native SOL-only scope, no operator promotion exclusion, public A on as explicit legacy-flag difference, and zero bootstrap pot/seed; map separate legacy payout/recovery source paths.
- [x] Accept source 35/45/20 and gross accrual; fix identified UI drift later. Record no principal refund, 1-lamport arithmetic, remainder carry, floor Q18 factors, wallet-paid fees, >10-lamport Backer-open cutoff and the tightened atomic quota as explicit native differences.
- [ ] Capture vectors at 0, 1, 7, 14, 15, 21 and 30 days and one second either side of boundaries; minimum deposits; mixed and absent Backer tiers; partial/zero pot.
- [ ] Include senior-before-A-weight recalculation, top-up/merge, caps, unclaimed credit and missing-tier fold cases. Compare to current Motoko formulas and identify deliberate divergences.
- [x] Freeze same-rule operator eligibility, no-Backer preference/fallback and owner-signed alternate destinations; native unsolicited SOL stays unallocated with no arbitrary refund/credit/sweep; manual legacy recovery/desk remains separate.
- [x] Accept narrowed processing permissions and separate storage/provider/B reserve, zero on-chain rewards, two sponsor keepers and default-zero optional contribution. Schedule resource measurements in implementation; no population cap implied.
- [x] Freeze recommended close+600-second promotion deadline, capture <= / expire > boundary, no fresh draws, permissionless no-promotion expiry and explicit withholding risk. User rejects indefinite oracle waits; ten-minute value is an accepted policy default, not a wall-clock completion guarantee.
- [x] Record 2-of-3/48h autonomous governance and immediate admission guardian, future-only versioned schedules/provider tuples, exact Callback dependency candidate, native PP trust/replay contract and launch-only inputs from defaults register.
- [x] Record selected pilot workload and provisional host/cost envelope; distinguish cycle charges, consumed SOL fees, reusable/retained rent and deployment-buffer liquidity. No protocol cap, live funding, server order or measured binary size implied.
- [ ] Review and commit the readable contract before implementation of production financial instructions.

Acceptance: every current financial feature is mapped to preserve / explicitly change / separate legacy rail; no executable instruction relies on an unresolved economic choice. Fresh launch has no import mechanism or access to legacy reserves. Legacy withdrawal access does not imply verified solvency.

### Task 2 — Prove bounded Backer allocation and integer math

Host implementation: `solana/crates/economics` and [allocation contract](../../solana/backer-allocation.md). Host chunk equivalence and cash-conservation tests exist. Cloning maps for host rollback/commit does not prove constant-size SVM commit or compute limits; account layouts and measured inline feasibility remain later work.

**Owner:** economy/program worker; independent review by security reviewer.
**Files:** proposed `solana/crates/economics/src/{lib.rs,earnings.rs,fees.rs,backers.rs,allocation.rs}`, `solana/crates/economics/tests/{vectors.rs,conservation.rs,backer_transitions.rs,allocation.rs}`, `docs/solana/backer-allocation.md`.
**Inputs:** frozen task 1 corpus.
**Outputs:** exact numeric primitives and a bounded allocation algorithm; finalized Backer/registry schema and error limits for task 4.

- [ ] Write failing vector tests for `entry_split(gross_lamports)`, simple accrual, maturity earnings, tiered fee, insolvency scaling and Backer allocation under the chosen contract.
- [ ] Implement the accepted formulas and width table: u64 cash, u128 entitlements/credited/cumulative P&L/global funded claims, fixed U256 headroom sums/pro-rata division; floor entry fee, Simple remainder, Q18 factors and actual-gross fee. Test safe loss subtraction for net winners and near-limit A/B entitlement growth; prospective B grants must fit before deposits are accepted. Include backwards time, >u64 owed and zero earnings at expiry.
- [ ] Model Discover → Senior → SealWeights → Allocate → Prune → Commit, plus a measured complete inline path. Initial membership determines folds; derive post-senior weights without a third full scan. Floor slices and route residue to seed. Inline and any 1/4/8-row grouping must produce identical economics.
- [ ] Test no Backers, only A/B, senior caps all A, senior leaves 1–10 lamports, A/B caps, reopens with original seniority, joint A+B claims, and user splitting. Reject generic reward-index substitution without a new equivalence proof.
- [ ] Model scalar finalized-ticket watermark and lazy local credit materialization. Test old funded claims during processing, no pending-credit spend, no global double funding and no lifetime-credit decrease on claim.
- [ ] Prove conservation at each phase/terminal commit and captured Backer immutability. Normal game deposits may change pot/management/UserRound after the accepted payout but not ticket weights, N, toll or Backer snapshot. Top-ups/other game payouts/affected term changes remain barred; closing admissions remain sealed.
- [ ] Measure maximum accounts/compute/data and liveness at approved population scales; document any product capacity limit before accepting it.
- [ ] Commit tests, numeric primitives and the reviewed account/allocation contract.

Acceptance: exact agreement with the accepted integer oracle; no hidden claim loss or over-cap allocation; no unbounded finalizer; all residue tracked. This is a model/contract proof, not SVM latency proof. If economic parity fails, return a concrete redesign choice before building that rail. Toolchain commands are recorded after task 4 pins compatible versions; this draft does not fabricate test results.

### Task 3 — Prove bounded round closure and verifiable selection

Host implementation: [round state model](../../solana/round-state-machine.md); SDK/transport evidence: [provider/resource probes](../../solana/provider-resource-probes.md). The host model checks logical bindings; actual SDK PDA/schema checks are isolated in the probe. Neither is SBF account-constraint or live oracle evidence.

**Owner:** program/integration worker, coordinated with task 2.
**Files:** proposed `solana/crates/economics/src/rounds.rs`, `solana/crates/economics/tests/rounds.rs`, `docs/solana/round-state-machine.md`, `docs/solana/randomness.md`.
**Inputs:** task 1 eligibility/reset contract; task 2 reserve representation.
**Outputs:** explicit round phases, complete candidate representation, provider interface and permissionless progress rules.

- [ ] Write tests for aggregate loser selection across winning/losing and closed/open positions, preference/fallback, no candidates and duplicate traversal.
- [ ] Define unique current-round participants/UserRound P&L and persistent native A/B-existence flags, including closed records. Seal complete program-owned preferred/fallback lists, preserve all-position net-loss aggregation and verify direct random-ordinal lookup.
- [ ] Specify Active → AllocatingClosingToll → CountingCandidates → AwaitingRandomness → PromotionApplied/Skipped → ResetReady → Active(next), with immutable close time/deadline +600. Freeze the companion's canonical `PromotionApplied` / `PromotionSkipped(reason)` representation in state and events, including Deadline, NoCandidates and EntropyExhausted reasons. Empty lists skip normally. No new-round funding before cash commit, terminal promotion and full seed carry.
- [ ] Probe published ORAO registered-client Callback 0.4.0 with exact Anchor 0.32.1 family, CPI-only feature and compatible SBF/CLI/harness. Pin deployed provider owner/namespace separately later. Test State-PDA signer, client ownership, fees/rent/retention and direct capture; no client-level callback by default. Switchboard is a redesign option if this adapter fails, never a second active draw. Registration needs separately authorized deployment later.
- [ ] Implement the companion's fixed-width domain-separated seed and `NotRequested → Requested → Captured` marker; CPI request creation and marker are atomic. Test duplicate delivery, unauthorized precreation/foreign client, unexpected existing account fail-closed, and rejection of arbitrary seeds/prefulfilled requests; no collision fallback creates a new draw.
- [ ] Test one sealed request with same-seed retries; deadline never extends. Capture validates execution-time Clock <= deadline; expiration requires Clock > deadline with no timely capture. Timely capture may apply later and cannot be overwritten; late authentic callbacks no-op. No fresh seed/provider/fallback winner; withholding can still deny promotion.
- [ ] Bound same-output rejection sampling to counters 0–63; first valid ordinal is mandatory, all failures terminalize `PromotionSkipped(EntropyExhausted)` without reroll. Test N=1, threshold boundaries, resumed counter order and bounded cost; closing work reserve covers the mapping steps. Disclose rare omission rather than claiming infinite sampling must terminate.
- [ ] Implement public expiry across partial candidate build, request-init/collision failure and provider nonresponse. Abandon candidate-only pages without deleting financial state; seed carries once after cash commit. Test no mandatory oracle wait after deadline without claiming chain/transaction timing guarantees.
- [ ] Separate application Clock deadline from ORAO callback slot expiry; test direct canonical-result capture after callback failure and ignored-late callback safety. No conversion to a guaranteed slot count.
- [ ] Preserve strict `owed > pot` and equality/refill behavior. Owner-authorized eligible positive-earnings/matured zero-pot claim with minimum zero starts close; no keeper/timer closes an initial empty pool. A minimum failure reverts and exposes an explicit owner retry route.
- [ ] Commit reviewed state transitions, schemas, tests and boundedness measurements.

Acceptance: complete set for successful promotion, authenticated timely result or explicit terminal skip; at most one grant/full seed carry. Oracle outage cannot disable deadline expiry/finalization, and neither can admission pause. Finishing still needs live-chain transactions and committed cash allocations; a deterministic cash-processing defect needs preserved-state diagnosis/reviewed repair, not a pretend timeout unlock. Provider lifecycle/authentication must pass before adapter acceptance.

### Task 4 — Implement the native core program

**Owner:** program worker; coordinator owns account/IDL integration.
**Files:** proposed `solana/Anchor.toml`, `solana/Cargo.toml`, `solana/programs/musical_chairs/src/{lib.rs,state.rs,error.rs,events.rs,instructions/*}`, `solana/tests/{authority,positions,backers,rounds,conservation}.*`, CI configuration scoped to this workspace.
**Inputs:** frozen task 1–3 contracts. **Outputs:** reviewed account layouts, IDL, generated client and locally verified financial program.

- [ ] Pin the exact provider-compatible tuple/lockfiles; create workspace and focused CI with first failing account-constraint tests. Start from Callback manifest candidate, not a generic tutorial; record failed compatibility probes and no untested newest-stack assumptions.
- [ ] Implement authenticated initialization with zero initial pot/seed, canonical vault/rent partitions and G/P/management roles. Test unauthorized first caller, one-voter governance failure, prospective schedule snapshots and no arbitrary repair ABI. Record rent payer/refund: owner User/Position/UserRound/A, sponsor shared/automatic B; preserve persistent lifetime records.
- [ ] Implement public non-refundable `fund_operations(amount)` and sponsor-recipient-only free withdrawal to itself. Capture ticket/close locked sponsor/owner reserve; consume sponsor first, refund unused to free vault/recorded owner once, and preserve at most two beneficial rent-funding components per account until safe closure. Test no donor registry, no locked-reserve withdrawal/config override, no game/PP credit from top-up, and operations physical conservation. Enforce personal rent payer owner; test third-party transaction fee payer cannot gain authority or redirect refunds.
- [ ] Implement admission-only guardian set/clear and G-controlled future changes; test claims/progress/expiry/finalization survive pause, active Position/ticket/close byte-equivalence under staged config changes, recipient-only management withdrawal and unallocated excess isolation. Squads loader/threshold/delay/bypass and provider-client ownership readback belongs to later authorized integration/deployment evidence.
- [ ] Implement atomic `open_position(plan, gross_lamports, client_nonce)` and `fund_series_a(gross_lamports)` with exact fees, minima and owner/rate-limit rules.
- [ ] Implement payout methods with owner minimum/destination and default-zero `max_work_contribution_lamports`: atomic net payment + toll escrow + Position/P&L + ticket, reserving sponsor work/rent before acceptance or explicitly bounded owner contribution. No silent charge or retroactive debit; test one-lamport-underfunded and contribution-limit rejection before mutation.
- [ ] Implement dense active Backer PDA pages, captured-version cursor traversal, immutable snapshots, bounded descending-slot compaction and moved-slot validation. Retain lifetime/first-funding records and permanent existence flags. No historical Backer scan per toll or unbounded commit.
- [ ] Implement constant-size ticket commit and lazy funded credit, plus combined `claim_backer(destination)` using the owner's expected A/B accounts. Claim during allocation cannot spend pending toll; lazy materialization cannot add global funded totals twice.
- [ ] Implement permissionless allocation/list/draw/seed progress with finite captured units, zero on-chain reward, separate reserved storage/provider/automatic-B costs and once-only per-funder refunds. Test no retroactive contribution, no winner signature for B, one-row fallback and no lease. Normal nonclosing deposits stay available; closing/A top-up/subsequent payout/affected terms remain barred. Paid reward schedules are optional deferred work.
- [ ] Implement immutable deadline, permissionless promotion expiration and full seed carry only after cash commit. No arbitrary cash abort/credit or winner choice. Direct closing-round open rejects; an atomic expire/finalize/open composition must check eligibility and expected new round/config before accepting funds.
- [ ] Test wrong signer, substituted PDA, wrong account owner, duplicate mutable aliases, reinitialization, arithmetic overflow, replay, double claims, failure rollback and unauthorized treasury/config changes.
- [ ] Measure inline candidates N=0/1/16/17 and chunk populations 64/256/1,024, compute/accounts/rent/transaction cost/latency, tiny-claim load and churn. Test arbitrary chunk grouping, replacement keeper, one-row fallback, sponsor fee-budget shortage/spike/top-up and unchanged economics. Preserve zero-slice fast paths without hidden minimum/cooldown/cap. Record measured envelope, not a false ten-minute cash completion promise. Generate/verify IDL/client.
- [ ] Add N≈100/M≈100 selected-pilot measurements and larger packet/chunk/inline candidates where feasible; count actual keeper signatures/cleanup and price ICP threshold signing separately. If a keeper uses ALT/newer format, isolate it from unchanged GUI signing and retain account/cursor proofs. Replace assumed eight-row count and byte budgets before choosing routine keeper host.
- [ ] Commit reviewed source, tests and reproducible artifact/build instructions.

Acceptance: all core behavior, including Backers/EEC, passes source, economic and security checks. Deposit/withdraw-only output is an intermediate prototype. No deployment readiness is inferred from passing local tests.

### Task 5 — Connect the existing GUI to native transactions

**Owner:** frontend worker; financial contract comes from task 4.
**Files:** proposed `frontend/src/solana/native/{client.ts,transactions.ts,accounts.ts,status.ts,types.ts}`, focused tests, `frontend/src/hooks/useNativeSolana.ts`; adapt `useWallet.tsx`, SOL sections of `useQueries.ts`, `SolInvestPanel.tsx`, `SolBackerPanel.tsx`, `GameTracking.tsx`, `HouseDashboard.tsx`, `WalletDropdown.tsx`, `.ic-assets.json` only as needed.
**Inputs:** generated IDL/client; canonical account data. **Outputs:** preserved screens with wallet-signed financial actions and direct canonical reads.

- [ ] Define view models using bigint lamports and wallet pubkey; keep ICP principal for social services separately. Include cluster/program ID in cache keys.
- [ ] Keep current wallet-adapter/web3.js boundary and legacy/v0 packet/read capability; verify selected wallet pubkey again at send. Test one-row recovery in that packet limit and explicit native/legacy cache/selector separation. No v1/Kit/UI migration assumed.
- [ ] Write failed/rejected/expired/unknown transaction and wallet-switch tests; preserve submission identity so an RPC timeout cannot trigger a duplicate opening.
- [ ] Preserve panels with signature, fee/rent, minimum receive and optional work contribution maximum. Distinguish payout-finalized/allocation-processing/round-closing; allow normal deposits and funded claims per permission matrix. Display promotion deadline/skipped outcome and exact blocked phase/cursor/diagnostics. No auto-signing of user recovery.
- [ ] Add a small “Resume processing” action/unsigned recovery builder for any wallet, including one-row fallback and completed-cash expiration/finalize/open composition. Expose 60-second no-progress warning and 300-second escalation as operational objectives. No original keeper/admin account or global GUI redesign.
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

**Keeper artifacts:** proposed dedicated `native_sol_keeper/` canister with bounded timer/RPC/transaction-journal modules for ICP monitor/backup; external routine worker under `solana/keepers/` if hybrid selected. These are future paths, not installed services. Reuse reviewed SolSigner/SolRpc/SolTx primitives without importing legacy pool authority; verify current provider Candid/key environment before new bindings and leave legacy endpoints unchanged.

- [ ] Implement authenticated relayer descriptor ingress and ICP two-independent-RPC finalized observation, durable archive/gap/outbox recovery. Validate program/accounts/schema/event beneficiary; disagreement remains pending. Decode observed cluster transaction formats via raw-RPC boundary, including external v1 program calls if active; unsupported version stays a recoverable gap, never cursor-skipped. Document residual fabrication/omission trust and separate native source-round namespace from ICP PP bucket cache.
- [ ] Implement canonical SIWS-bound owner/B-beneficiary rewards alongside legacy observers; gross game/A or B-loss basis, existing rates/cascade/effects/signup dedup, immutable event tuple and policy snapshot. Missing binding retains pending pubkey event; no keeper/operator-selected recipient or rebind/remint.
- [ ] Add native write-ahead per-leg reward outbox with immutable recipient/amount/fee/memo hash/created_at_time and effect/cascade obligations persisted before await. Verify actual PP ledger dedup/history capability; reconcile Duplicate/unknown receipt before retry, quarantine unresolved expired-window result, and never repeat committed legs or silently drop a mint after retry count. Test ledger commit/canister trap, partial cascade/siphon, gaps, policy update, missing binding and cross-rail signup replay. Native integration does not claim to repair all legacy observer behavior.
- [ ] Preserve the existing ICP/Chain Fusion PP desk purchase/proceeds/refund flow, clearly labeled and isolated from the native game pool. Verify legacy desk transactions cannot create native Positions, and native pool funds cannot pay desk refunds or proceeds. Native desk/escrow rewrite remains optional deferred work.
- [ ] Verify history outage leaves direct core SOL reads/transactions available, and display lag without implying financial failure.
- [ ] Implement chosen active/standby keeper journal, bounded schema-derived signing, adaptive idle polling and batched statuses; re-arm ICP timers after upgrade. Test canister/host/RPC outage, duplicate races, blockhash expiry/unknown broadcasts and public wallet takeover. Budget/refill capped fee addresses separately; no live signer setup/auto-running process is authorized by source implementation.
- [ ] Measure ICP cycles per signature/RPC/bridge batch and end-to-end progress time at pilot workload, including idle/standby overhead; test that observed sign/cursor latency supports proposed close policy without extending deadline. Same-subnet keepers are not independent subnet failure domains.

Acceptance: no duplicate PP reward, no fabricated SOL balances, correct social identity mapping, declared desk behavior and recoverable historical gaps. An ICP indexer is optional; preserving existing user-visible history remains a product requirement that can be served by another reviewed solution.

### Task 7 — Devnet acceptance, reconciliation and independent review

**Owner:** coordinator; independent program/security and frontend reviewers.
**Files:** proposed `docs/solana/acceptance.md`, `docs/solana/threat-model.md`, `docs/solana/reconciliation.md`, bounded read-only receipt tooling.
**Inputs:** tested core/client/social artifacts. **Outputs:** reproducible evidence and an explicit release recommendation.

- [ ] Review custody, signer/account checks, upgrade/config powers, economic conservation, candidate completeness and transaction-order exposure.
- [ ] With separate deployment/test-wallet authorization, run devnet deposit, simple payout, both compound settlements, Series A funding/claim, cap completion, partial payout and reset/promotion.
- [ ] Under that authorization verify G Vault as actual loader/provider-client authority, autonomous zero config-authority sentinel, 2-of-3/48h configuration/vault delays, no spending-limit bypass, guardian-only pause and prospective provider/config snapshots. Test configuration reducing its own delay; do not label the governance quorum unable to upgrade malicious code.
- [ ] Reconcile physical balances, protected rent, pot, seed, funded claims, management and pending escrows after each action. Verify transaction signatures/finality independently of GUI/mirror.
- [ ] Exercise deadline−1/equality/+1, timely execution finalized later, late delivery/old callbacks, candidate partial/init failure/collision, permanent provider nonfulfillment, financial allocation pending at expiry and exactly-once terminal carry. Test user recovery without keepers, sponsor shortage, admission pause, normal deposit races and captured-draw/expire competition.
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
- [ ] Reprice exact compiled bytes/capacity/upload buffer, account sizes/cleanup/retention, oracle/governance configuration and measured fee/signature counts from current public quotes; publish SOL and cycle/subscription funding breakdown before requesting those funding actions. The 12-SOL pilot estimate is not itself a deployment receipt.
- [ ] After authorization, deploy verified program bytes, verify actual authorities/accounts, and deploy ICP frontend assets from the approved build.
- [ ] With separately authorized funds, reconcile initial pool partitions and verify a bounded live lifecycle with finalized receipts and balances.
- [ ] Monitor reserve mismatch, keeper/draw delay, failed transactions, RPC/mirror lag and frontend served version; keep recovery instructions accessible.

Acceptance: installed bytes and authority state match reviewed artifacts; selected financial lifecycle has actual finalized receipts and reconciled balances. Source, tests, CI, merge, deployment, funding, finality and displayed balances are separate evidence.

## Cost-aware execution recommendation

Use cheaper native workers for bounded inventory, vectors, generated clients and frontend adaptations. External Sonnet program work/review is an option only after provider-specific repository-disclosure authorization and authenticated capability are both confirmed; the rejected review attempt grants neither authorization nor a workaround. Keep the coordinator responsible for shared financial contracts and integration. Do not fan out overlapping account-layout/IDL edits. Increase model capability only for a demonstrated unresolved problem.

PP/legacy scope is confirmed and D3–D9 defaults are recorded. Foundation v2.6.0 refresh is verified; Mindrally was already present. Pilot host/SOL/cycle budget is now estimated with explicit assumptions and public quotes. On a later implementation instruction, start accepted oracle/model plus provider/toolchain compatibility, then native core, preserved GUI and PP outbox. Measurements/provider/runtime/ledger proof happen there; actual voters/funding/endpoints and mainnet go remain launch inputs. No product code/build/test/deploy/financial transaction started; no exact deployment quote or mainnet date is claimed.
