# Musical Chairs native Solana discovery

User brief: retain ICP frontend hosting and current GUI; move financial custody, accounting, and execution to native Solana. Discovery and draft planning only; no product code or live actions.

Source baseline: `main`, `f96b33f` (verify full HEAD during synthesis). Existing untracked `.claude/skills/`, `.codex/`, and `AGENTS.md` are preserved.

| ID | State | Owner | Task | Depends on | Evidence / blocker |
| --- | --- | --- | --- | --- | --- |
| SOL-01 | Done | `/root/economy_inventory` | Current economics and financial inventory | — | Coordinator checked rates, senior-first allocation, gross accrual, fees and reset references; handoff in native-solana-discovery.md |
| SOL-02 | Done | `/root/native_solana_gap` | Current SOL custody and native Solana gaps | — | Coordinator checked Motoko authority, threshold signing, obsolete native draft and boundedness gaps |
| SOL-03 | Done | `/root/frontend_preservation` | GUI preservation and wallet/data changes | — | Coordinator checked asset hosting, actor hooks, SIWS/PP and CSP submission boundary; corrected native fallback recommendation |
| SOL-04 | Done | coordinator; `/root/draft_design_review` | Integrate research, review risks, draft spec and roadmap | SOL-01, SOL-02, SOL-03 | Accepted as discovery drafts with corrections applied; Rob confirmed fresh launch and ICP PP/Shenanigans; no execution/launch approval implied |
| SOL-05 | Done | `/root/economic_contract` | Recommend native economic contract | SOL-01 | Coordinator checked source/UI drift and integer vectors; selected floor Q18 rather than worker's nearest rounding; explicit precision/fee changes |
| SOL-06 | Done | `/root/backer_design` | Preserve senior-first capped Backer allocation with bounded work | SOL-01, SOL-02 | Complete batches recommended; coordinator replaced unbounded final merge with scalar watermark/lazy credit, and lifetime scan with dense active membership |
| SOL-07 | Done | `/root/round_design` | Preserve round reset, candidate completeness and failure recovery | SOL-01, SOL-02 | Frozen lists and one authenticated client-scoped request; initial indefinite-wait recommendation superseded by SOL-10 under user's availability requirement; no rerolls |
| SOL-08 | Done | coordinator; `/root/economic_design_review` | Integrate recommendations into spec and roadmap | SOL-05, SOL-06, SOL-07 | Independent review accepted direction/vectors; width/conservation and request-initiation findings corrected and follow-up confirmed; design only |
| SOL-09 | Done | `/root/processing_recovery` | Bound processing delays without dropping financial allocations | SOL-06 | Accepted design direction: narrow barrier, captured work-unit budget independent of chunk grouping, sponsor/default-zero optional contribution and self-funded fallback; runtime costs unproved |
| SOL-10 | Done | `/root/oracle_outage_policy` | Terminal randomness outage policy under no-indefinite-round requirement | SOL-07 | Recommended 600-second one-shot deadline, permissionless skip, no reroll; timely capture protected and pending cash allocations preserved |
| SOL-11 | Done | `/root/solana_skill_comparison`; coordinator | Assess Foundation/Mindrally skill additions | — | Foundation local copy outdated; Mindrally body already installed locally; update Foundation rather than duplicates; nothing installed |
| SOL-12 | Done | coordinator; `/root/availability_review` | Update/review pause and outage spec/plan | SOL-09, SOL-10 | Independent review accepted document coherence after permissions, finalize-before-open, defect limits and canonical terminal naming corrections; implementation/runtime readiness unclaimed |
| SOL-13 | Done | coordinator | Refresh existing Foundation skill | SOL-11 | User authorized refresh; v2.6.0 / 491a19a installed at existing target; 33-file parity, symlink and rollback verified |
| SOL-14 | Done | `/root/default_economics` | Recommend D3–D5 and financial-feature defaults | SOL-08 | Coordinator accepted with source-equivalent operator eligibility/public A on flag difference; worker follow-up confirmed |
| SOL-15 | Done | `/root/default_controls` | Recommend D8 control/governance defaults | SOL-13 | Coordinator selected 2-of-3/48h G, immediate 2-of-3 P; follow-up checked zero-key sentinel, timelock bypass/trust and prospective snapshots |
| SOL-16 | Done | `/root/default_operations` | Recommend D6/D7/D9 work funding, keeper and launch-measurement defaults | SOL-12, SOL-13 | Accepted after zero reward/storage-provider reserve separation and automatic B sponsor rent; no unmeasured budget/cap |
| SOL-17 | Done | `/root/default_integration` | Recommend wallet/toolchain/ICP PP and legacy-access defaults | SOL-13 | Callback 0.4.0 published manifest checked; bridge fabrication trust/PP loss basis/source-round/outbox refinements accepted |
| SOL-18 | Done | coordinator; `/root/defaults_review` | Integrate/review all remaining recommended defaults | SOL-14–SOL-17 | Independent coherence review accepted after reserve/payer clarification; runtime proofs and actual launch inputs remain explicit; no implementation/launch approval implied |
| SOL-19 | Done | `/root/icp_keeper_research` | ICP keeper placement and least-authority design | SOL-18 | Coordinator checked official signing/cycles/subnet/timers; fully ICP viable, fee-only keys; hybrid recommended on pilot counts |
| SOL-20 | Done | `/root/oracle_cost_research` | Callback provider fees/account lifecycle and setup costs | SOL-18 | Live configured Callback fee 0.0003 SOL, no-callback rent/retention, Squads creation fee 0; coordinator decoded saved public receipt |
| SOL-21 | Done | `/root/footprint_cost_research` | Resource/transaction count scenarios for operating budget | SOL-18 | Corrected old rent coefficient to live quote, excluded owner costs, counted request creation, conservative two candidate lists; no runtime measurement claim |
| SOL-22 | Done | coordinator; `/root/icp_budget_review` | Reconcile ICP hosting and SOL budget; update spec/plan | SOL-19–SOL-21 | Independent arithmetic/coherence accepted after all seven documentation clarifications; runtime sizes/counts/latency remain unmeasured; no implementation approval implied |
| SOL-23 | Done | coordinator; `/root/footprint_cost_research` | Semantic graph fragment: native spec/plan/economics/recovery | SOL-22 | Native Luna extraction accepted after coordinator line-anchor corrections and reuse of existing frontend file nodes |
| SOL-24 | Done | coordinator; `/root/oracle_cost_research` | Semantic graph fragment: defaults/budget/discovery/skills/workflow | SOL-22 | Native Luna extraction accepted after label/ID/status corrections; 10 documents merged with scoped hashes/cache, old relationships preserved |
| SOL-25 | Done | `/root/default_controls`; coordinator | Review unresolved pre-build choices and maintenance workflow | SOL-22 | Independent checks accepted source pins/topology/status/workflow; root-status metadata clarification corrected; no new planning gate or implementation approval |

## Accepted model/probe implementation pass — 2026-10-12

Rob accepted the defaults and authorized the Rust economic model plus local provider/resource probes, cheaper agents, graph maintenance and contextual Solana explanations. No deployment, live signing/broadcast, wallet/funding, frontend migration or production financial entrypoint is authorized.

Managed worktree: `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs`; branch `codex/native-solana-model-probes`; baseline `38cbf153a5ff46487fdbe80314f93acfa7328e63`. Requested native worker model `gpt-5.6-luna`, selected explicitly per card; a focused `gpt-5.6-terra` round pass followed repeated lifecycle failures; external repository disclosure remains unauthorized. Worktree paths require normal sandbox escalation for writes; approval grants workspace work, not live actions.

| ID | State | Owner | Owned files / scope | Acceptance / evidence |
| --- | --- | --- | --- | --- |
| SOL-26 | Done | coordinator | Acceptance, contract/parity docs, workspace/lock, integration | Accepted defaults, local-only scope, pinned Rust1.92/locks, no native production ABI; source/GUI unchanged |
| SOL-27 | Done | `/root/model_math` (Luna) | earnings/fees/admission, vectors/tests, economics doc | 16 tests; malformed state/time/remainder rollback; fees and checked wide math; compound example typo fixed |
| SOL-28 | Done | `/root/model_backers` (Luna) | Backers, allocation/conservation/transition tests | 25 tests; caps/lifetime/custody, replay/chunks, saturated oversized continuations, closing payout barrier; A primitive crate-local |
| SOL-29 | Done | `/root/model_rounds` (Luna); `/root/round_fixes` (Terra) | rounds source/tests/contract | 23 integration +5 unit tests; strict close, position settlement, cash barrier, expiry,64-byte draw, sampler, canonical shared A admission and durable B history |
| SOL-30 | Done | `/root/provider_resource_probe` (Luna) | standalone provider/resource probe, evidence/doc | 5 tests +locked host SDK/CPI typecheck, exact schema/PDA/unsigned packing snapshot, vendor unchanged; SBF/CU/liveness not established |
| SOL-31 | Done | native Luna independent reviewers; coordinator | read-only model/probe semantic review | Confirmed findings fixed; third fresh whole-pass pair PASS; coordinator72 economic+5provider tests/fmt/clippy/snapshot green; reviewed host artifacts only |
| SOL-32 | Done | coordinator; native Luna extraction/review | scoped Rust AST +semantic graph refresh | Independent PASS:497scopednodes831edges6hyperedges/31pins; all6466oldIDs/12hyperedges and8380unrelatedrawedges retained; exact-ID queries;6954nodes9211relationships total |

## Earlier planning-pass dispatch contract

Graph/workflow pass (2026-10-12): existing native `gpt-5.6-luna` workers reused for two disjoint semantic fragments and independent pre-build/graph review. Coordinator updated CLAUDE/plan workflow and pre-build status, then preserved the historical root graph and added 162 nodes/270 relationships/6 hyperedges across 10 scoped documents. Current accumulated graph: 6,466 nodes/8,614 relationships. Source SHA256, exact-node navigation, manifest scope and topology checks passed. Root aggregate and native-only HTML views generated locally; no browser-rendered or product-runtime proof claimed. The original whole-graph baseline remains historical. [Workflow](native-solana-knowledge-graph.md), [durable fragment](evidence/2026-10-12-native-solana-graph-fragment.json), [refresh receipt](evidence/2026-10-12-native-solana-graph-refresh.json). No product implementation/build/deploy/sign/send/funding occurred.

- Workers inspect the same checkout read-only; only the coordinator writes documentation.
- Requested native worker model: `gpt-5.6-luna`; bounded research/design cards fit the lower-cost model; actual dispatch metadata is recorded per pass. External provider disclosure authorization remains absent; no Sonnet repository-content calls.
- Harness: native collaboration workers and native independent reviewer. Worktree: `/Users/robertripley/coding/musicalchairs`; active documentation branch `codex/native-solana-discovery-20261012` at pass start `f26022ab610d4df611b994698bf60ec1426502c1`; financial source baseline remains unchanged.
- Acceptance: exact file/line evidence, distinction between source and deployed behavior, assumptions and blockers, migration scope, and proposed durable persona lessons where justified.
- Integration gate: coordinator source checks and independent design review. No merge, deployment, signing, fund movement, or implementation is authorized by this board.
- Planning artifacts will mark unconfirmed decisions explicitly; they are drafts for discussion.

## Draft artifacts

- [Discovery and evidence](native-solana-discovery.md)
- [Draft specification](../superpowers/specs/2026-10-12-native-solana-design.md)
- [Draft implementation roadmap](../superpowers/plans/2026-10-12-native-solana.md)
- [Economic/allocation/round recommendation and vectors](native-solana-economics.md)
- [Processing and randomness recovery](native-solana-availability.md)
- [Solana skills assessment](solana-skill-assessment.md)
- [Recommended remaining defaults](native-solana-defaults.md)
- [ICP keeper placement and operating budget](native-solana-icp-budget.md)
- [Public price/account receipt](evidence/2026-10-12-native-solana-costs.json)

## ICP hosting and operating-budget pass

- User asks whether keepers/other services can run on ICP and for approximate SOL operating costs before implementation. Same branch/workspace, baseline `fb8b5e5419381929f5a1f553c02a2f12de027b3c`; financial source unchanged.
- SOL-19–SOL-21 use explicitly selected native `gpt-5.6-luna`, repository/public sources read-only; coordinator owns docs. No product code/install/build/deployment/signing/send or fund movement. Public RPC price/account reads are permitted research, not transaction execution.
- SOL-19 acceptance: official timers/threshold-Ed25519/SOL RPC capability, signing/custody boundaries, failure domains, cycle costs and existing reusable source. SOL-20: exact Callback fees/registration/request rent/retention and governance fees. SOL-21: explicit draft byte budgets and count/fee formulas; declining no evidence is fine, inventing a measured cost is not.
- SOL-22 acceptance: distinguish deployment peak, locked/reclaimable rent, permanent growth, consumed Solana fees/oracle fees, ICP cycles and paid RPC subscriptions; calculate low/moderate/stress cases and propose a replenishable startup amount. No game liquidity, audit spend or guarantees hidden in operating budget.
- User selected pilot N≈100 active Backers, P≈1,000 payouts/month, C≈10 closes/month. M=100 participants/close and b=8 are separate modeling assumptions. Native Luna dispatch confirmed SOL-19–SOL-21. Coordinator validated public account owner/discriminator/schema, compared independent rent quotes and checked current CLI default max length; no signed transaction executed.
- Handoffs accepted as research results, not production financial verification.
- Verification: source cross-checks and documentation structure checks passed. Product tests/builds skipped because no product files changed.
- No branches merged, programs deployed or funds moved.
- Documentation branch: `codex/native-solana-discovery-20261012`; source baseline remains `f96b33ff95020458be754bfa0d7471b72a7403af`.
- Review requested model: Sonnet 5. Actual source/document review: did not run; automatic approval review rejected private repository disclosure to an external Claude destination without provider-specific authorization. Availability check succeeded without reading repository contents.
- Safe fallback: native `gpt-5.6-luna`, requested and dispatch-confirmed, completed bounded independent review of coordinator-authored drafts. No external-provider retry or workaround.
- Review corrections applied: source/branch provenance, alternate payout and recovery parity, randomness outage behavior, rent payer/refund/liveness, replay-safe SIWS PP reward binding. Backer algorithms and remaining D3–D9 decisions remain explicitly unproved/unapproved.
- User scope confirmed 2026-10-12: fresh native pool with legacy positions withdrawable separately; SOL financial game first; PP/Shenanigans stay on ICP. No imports/native PP project. Existing PP desk stays a separate ICP/Chain Fusion rail; native desk replacement deferred.

## Economic design pass contract

- SOL-05–SOL-07 are user-authorized design investigations, read-only to the product and shared documentation. Workers return recommendations; coordinator owns documentation writes.
- Apply source and numeric checks; ephemeral calculation scripts in `/private/tmp` are permitted design aids, not production implementation. No product scaffold/dependencies, builds, deployments, financial actions or live state mutation.
- Each worker identifies exact source parity, intentional integer/fee/infrastructure differences, rejected options, liveness/cost assumptions and unresolved decisions. Independent reviewer assesses the integrated recommendation.
- Target: concrete recommended economic/account/state-machine contracts and a revised implementation sequence, with evidence limits named. No recommendation is labeled approved or SVM-proven.
- Worker/reviewer dispatch: native `gpt-5.6-luna` explicitly selected and dispatch-confirmed for SOL-05–SOL-08. No external Sonnet content calls in this pass.
- Checked: source references, Decimal precision-100 Q18 factors, independent integer fee/boundary/Backer vectors, exact toll conservation examples, documentation links/structure and diff whitespace. These are design checks, not executable program tests.
- Review findings addressed: explicit u64/u128/U256 and phase custody equality; exact RNG seed/request marker/collision rules and registered-client authority. Reviewer follow-up accepted these corrections.
- Remaining acceptance/proof: 1-lamport/remainder/fee/quota changes, allocation barrier and oracle outage policy, operations funding/governance, provider version/source/lifecycle and measured execution costs. No product code/builds/deployments/transactions started.

## Availability pass

- New user requirement: rounds must have an explicit recovery outcome instead of waiting indefinitely for randomness. The prior fail-closed oracle wait is superseded for design purposes.
- Native `gpt-5.6-luna` workers, repository read-only and public-source browsing; coordinator alone edits docs. No external repository disclosure, product code, deployment, signing, funds or skill installation.
- Evaluate safe processing resumption and explicit cost/scale limits separately from a terminal no-promotion oracle timeout. No dishonest wall-clock guarantee during a chain halt or without transaction submission.
- Skill question is advisory: compare current Foundation/Mindrally contents to installed January Solana guidance; do not overwrite skills without an installation/update request.
- Pass baseline: `9327a53ee6cff11a0f2ec5fad0dcb8e49a5c5203`; source baseline unchanged. SOL-09–SOL-12 dispatch-confirmed native `gpt-5.6-luna`, no external private-content review.
- Coordinator corrected suggested transaction-count budgeting to verified work units, kept caller contribution explicit/default zero, and replaced indefinite oracle wait with public deadline expiry. Same-output mapping is bounded to 64 counters; canonical terminal reason/event representation is explicit.
- Independent follow-up accepted cross-document permissions, atomic finalization ordering, preserved-state defect limits and terminal naming. Documentation links/fences and diff whitespace passed; no product tests run because product code is unchanged.
- Open decisions/proofs: accept timeout/omission and optional cost policy; pin/test provider lifecycle and toolchain compatibility; measure complete/chunked processing costs; governance/launch decisions. Skill refresh is optional groundwork, not an implementation gate.

## Defaults pass — SOL-14–SOL-18

- User authorized Foundation refresh and cheaper-agent recommendations before implementation. Baseline `c9da2faefd6753e89ad04a20c372aa2de5775767`; same documentation branch/workspace and unchanged financial source.
- Native `gpt-5.6-luna` selected for four bounded investigations and independent review. Workers read only; coordinator owns all shared-document edits. No external provider private-source disclosure, product scaffold, dependencies, builds, transactions or deployments.
- SOL-14 scope: D3–D5 and unmapped financial features. Acceptance: concrete defaults with source lines, example/divergence and later validation; avoid changing the game to reduce cost.
- SOL-15 scope: D8 roles, upgrade/config/pause/provider controls. Acceptance: permission/delay matrix, justified defaults and explicit residual upgrade trust; do not invent signers or timelock guarantees.
- SOL-16 scope: D6/D7/D9 operations. Acceptance: sponsor/contribution/reward/rent/refund contract, finite recovery and proposed measurement envelope; economic admission differences disclosed; no fake measured results.
- SOL-17 scope: toolchain/wallet/PP/legacy interfaces. Acceptance: retained GUI/adapters, compatible candidate stack and replay-safe finalized-event trust contract, no native PP/desk scope expansion.
- SOL-18 integration: distinguish confirmed user scope, recommended defaults, implementation proof and launch-only choices. Independent document review before Done; pushed documentation receipt. No request to approve solely local hashes.
- Dispatch-confirmed model SOL-14–SOL-17: native `gpt-5.6-luna`, read-only. Coordinator checked source minima/quota/promotion/operator eligibility, PP amount-versus-entitlement and round namespace, published Callback crate manifest, and primary governance/RPC/provider references. Proposed persona lessons were reviewed as handoffs only, not installed.
- SOL-18 independent native `gpt-5.6-luna` review accepted after P2 fixes: imported/zero game liquidity versus mandatory operations reserve; operations top-up/free-withdrawal/locked partitions/refunds and separate fee/rent/authority roles. Frontend v0 and external newer-format bridge handling clarified. Documentation links/fences/table structure/whitespace checked; product builds/tests not run because product source is unchanged.
- [Recommended defaults](native-solana-defaults.md) resolves D3–D9 for discussion. Remaining work is scoped implementation proof and launch-only identities/capital/endpoints, not an unrelated preparatory project. Foundation refresh receipt is complete; old shared skill tree preserved. No merge, deployment, financial action or product dependency change occurred.

## Local runtime probe pass — 2026-10-13

User authorized cheaper agents for local compute/account/provider probes and graph maintenance; no deployment. Reuse attached managed worktree on `codex/native-solana-runtime-probes`, baseline `830d57377e71aa4f15ad0c7fa86ff2d6b3ff4e0c`; PR123 remains open/CI green. Native Luna workers explicitly selected; no external repository-content providers. In-process synthetic actors/transactions and read-only public bytecode/account snapshots are test fixtures; no wallet/key-file access, live signing/broadcast, registration/funding, production custody ABI or GUI changes. Root owns manifests/locks/shared contracts/graph/integration.

| ID | State | Owner | Scope | Acceptance / evidence |
| --- | --- | --- | --- | --- |
| SOL-33 | Done | `/root/runtime_toolchain` (Luna) | pinned local build/harness toolchain in owned temporary directory | actual SBF compile + compatible in-process runtime; record exact versions/hashes, limited disk budget; no global install |
| SOL-34 | Done | `/root/runtime_provider_research` (Luna) | actual ORAO artifact/SDK lifecycle research and local fixture provenance | real executable provenance and executable lifecycle versus synthetic or unsupported cases explicitly distinguished |
| SOL-35 | Done | `/root/runtime_contract` (Luna); coordinator | concrete probe-only wire/accounts/test contract | meaningful compute/account checks tied to accepted math and provider constraints; no production cost/readiness claims |
| SOL-36 | Done | `/root/runtime_permissions` (Luna) | compiled Anchor fixture program + negative permission tests | signer/writable/owner/discriminator/PDA, rollback, state-PDA CPI privileges |
| SOL-37 | Done | `/root/runtime_provider_research` (Luna) | local runtime provider requests/results/recovery | registration/request/fees/fulfillment/capture, failures and fixed-deadline recovery with explicit proof limits |
| SOL-38 | Done | `/root/runtime_compute` (Luna) | local CU/packet/account envelopes and snapshots | executed SBF numeric/kernel paths and budgets at representative chunks; no extrapolated whole-game cost |
| SOL-39 | Done | coordinator; independent reviewers | deterministic full gates and semantic review | local runtime receipts reproducible; financial/source/authority/runtime/live evidence separated |
| SOL-40 | Done | coordinator; extraction/review workers | scoped AST/semantic graph refresh | preserve historical raw records/hyperedges, canonical relative IDs, exact source hashes and query checks |

- Executed local evidence: [88 SBF transaction cases](../solana/local-runtime-probes.md), 45 asserted successes/43 asserted failures. Genuine captured ORAO registration/request/Ed25519 fulfillment/direct capture and expire-before-fulfill recovery; synthetic test actors only.
- Corrected eager Anchor-error allocation that exhausted the default SBF heap, double-discriminator fixtures, readonly error expectation, exact account bounds/binding, client funding/rent preservation, actual client ELF loader metadata and a public snapshot slot transcription. Before-fix heap receipt retained.
- Frozen tuple: Anchor0.32.1/SDK2.3.1/LiteSVM0.7.1 (Agave2.3.13), SBF builder3.1.0-beta.0/platform-tools v1.48; separate locked SBF/host graphs. Native Luna authors/reviewers; coordinator integration/source verification.
- Local test, locked all-target Clippy, offline fixture provenance and deterministic two-run JSON checks passed. Runtime-loaded account count unavailable; 32-row kernel packet oversize, full game costs/cluster/service/production custody unproved. Deployment remains unauthorized.

- Two final native Luna source/evidence reviewers returned PASS for the bounded runtime contract after request-binding/deadline and failure-class corrections. Independent graph reviewer returned PASS: 7,149 nodes, 9,594 directed relationships, 21 hyperedges; 44 source pins and all prior base records preserved.
- Graphify AST refresh plus scoped native semantic extraction covers 12 sources. Four inferred links connect encoded SVM tests/probe handlers to their contracts; no fabricated SDK calls. Whole-repository/legacy freshness remains historical. [Graph receipt](../solana/evidence/2026-10-13-runtime-graph-refresh.json).
- No live wallet/signing/broadcast, deployment, governance, funds movement, frontend change or branch merge. Runtime CI execution is not claimed; these are reproducible local gates and independent source/evidence reviews.

## Production account/payout/allocation pass — 2026-10-13

User authorizes production accounts and payout/allocation source plus local testing/graph maintenance; still no deployment. Reuse attached worktree on `codex/native-solana-accounts-payouts`, baseline `c99a636b9f040c9bf0d18d3698328d9bbd33b7b2`; PR123/124 remain open. Native `gpt-5.6-luna` workers explicitly selected. Root owns interfaces/manifests/locks/one build executor/shared docs and graph. Preserve primary checkout untracked files and all frontend/ICP code. Synthetic local actors and in-process transfers are test fixtures, never live wallet/funds authority. No external repository-content providers.

| ID | State | Owner | Task / scope | Acceptance / dependency |
| --- | --- | --- | --- | --- |
| SOL-41 | Review | `/root/production_account_contract` (Luna) | account/PDA/custody/init contract, temp handoff | fixed layouts, authenticated first init, no population cap; coordinator freezes shared ABI |
| SOL-42 | Review | `/root/production_payout_contract` (Luna) | economic payout/admission mapping, temp handoff | exact host parity and actual transfer partitions, nonce/quota/minimum and close barrier |
| SOL-43 | Review | `/root/production_allocation_contract` (Luna) | bounded allocation/registry/snapshot design, temp handoff | six phases, descending prune, lazy watermark and constant commit |
| SOL-44 | Ready | state worker (pending) | production state/PDA types and account codecs | coordinator schema freeze; serialization widths/layout tests |
| SOL-45 | Ready | controls worker (pending) | authenticated init, operations/management, guardian controls | real program accounts/cash movement, threshold profile checks, conserved reserves |
| SOL-46 | Ready | payout worker (pending) | open/A funding and owner payout instructions | schema/helpers frozen, host vectors, actual SBF transfers/rollback |
| SOL-47 | Ready | allocation worker (pending) | discover/senior/seal/allocate/prune/commit/claims | schema/helpers frozen, chunk and membership parity, real funds custody |
| SOL-48 | Ready | test worker; coordinator | actual program SBF/LiteSVM tests and measurements | failing financial tests before handler implementation; one coordinated build executor |
| SOL-49 | Ready | independent reviewers; coordinator | source/financial/account review and evidence | deterministic local gates then independent high-stakes review; no readiness shortcut |
| SOL-50 | Ready | graph worker; coordinator | AST + semantic refresh and preservation | retained current base7149/9594/21, portable identities/source pins, exact-ID paths |

Scope includes production account source and the admission/payout/allocation tranche. Production provider/candidate/promotion integration, GUI, ICP bridge, actual Squads execution/loader deployment and launch funding remain later roadmap work; no claim that this tranche alone is a launchable whole game. Source-close barriers must preserve all financial state and remain compatible with the accepted fixed-deadline recovery contract. Optional modernization or economic redesign does not become a required gate.

## Local implementation pass — 2026-10-14 (Codex go-ahead; M0–M13 local only)

Authorization: [go-ahead](native-solana-local-implementation-go-ahead-20261014.md) (SHA-256 `69ce19e3…21c0`) and [acceptance evidence](../solana/evidence/2026-10-14-codex-plan-acceptance.json) for planning commit `3109063e`. Branch `codex/native-solana-production-core` from `c99a636`: `80c543b` adopts the tranche verbatim (21 paths, hashes equal inventory §5); `39bc31c` merges the accepted planning lineage by plumbing (no working-tree writes). Coordinator: Claude (Fable 5.1) owns dispatch, shared contracts, the **single build executor** and graph; Codex owns independent review. Workers write only their card's file scope after the card exists here. No deployment, live keys/funds, ORAO/Squads registration, governance, production canister/frontend change, service activation or PR merge.

| Card | State | Owner (model actually invoked) | File scope | Depends | Acceptance / evidence |
| --- | --- | --- | --- | --- | --- |
| IMPL-00 | Done | coordinator (Fable) | tranche adoption + planning merge | — | `80c543b` blobs == inventory §5 hashes; `39bc31c` planning pins 01189e4e/1a328d36/2f873676/6641bc99 |
| IMPL-01 | Done | Haiku worker (claude-haiku) | `/private/tmp/mc-solana-runtime-tools-20261013/downloads/`, new `/private/tmp/mc-platform-tools-v148-check-20261014/` only (no repo files) | — | installed platform-tools identity vs anza v1.48 release: archive SHA `799e4cfc…` verified after a 422 MB fetch into the owned temp dir; all 2,872 files + 11 symlinks byte-identical (version.md, rustc 1.84.1-dev, cargo, clang-19); verdict IS v1.48; receipt `docs/solana/evidence/2026-10-14-m0-toolchain.json` (M0 step 2) |
| IMPL-02 | Done | coordinator (build executor) | none (read-only `cargo check`) | IMPL-00 | recorded `cargo +1.92.0 check --locked` failure at `runtime.rs:293` (E0599 `Signature` is not an iterator, E0282; exit 101) before any fix; receipt `docs/solana/evidence/2026-10-14-m0-compile-red.json` (M0 step 3) |
| IMPL-03 | Done | Sonnet worker (claude-sonnet) | `solana/tests/financial-runtime/scripts/build-sbf.sh`, new `solana/tests/financial-runtime/scripts/source-set-digest.sh` | IMPL-00 | wrapper emits `source_set_sha256` per plan M0 (sorted relative paths, `path||0x00||sha256||0x0A` over program sources/manifests/lock/cargo config/wrapper/toolchain file); self-check showed editing a copy of `state.rs` changes the digest; no build run by the worker. Evidence: current tree digest `43c94ebf5101fc004897bde6d285496ef555f3f379d266f815044f11cc61cf29` over 10 files; coordinator re-ran `--list` and `bash -n`; the probe wrapper `solana/probes/runtime/scripts/build-sbf.sh` still emits the old single-file field and is out of scope |
| IMPL-04 | In progress | Sonnet worker (claude-sonnet) | `solana/tests/financial-runtime/src/runtime.rs` (compile fix L293 + fixture validity: Ticket IDLE/id 0, `snapshot_pages=1`, `next_sequence=5`, Backer-PDA keys, no preloaded discover counts), `financial.rs` fixture helpers only | IMPL-02 | `cargo +1.92.0 check --locked` passes (run by coordinator); fixtures match genesis shape (M0 steps 4, 6); no handler change |
| IMPL-05 | Done | coordinator (build executor) | `programs/musical_chairs/artifacts/` (ignored), `docs/solana/evidence/2026-10-14-m0-stub-build.json` | IMPL-01, IMPL-03 | stub ELF built with the pinned wrapper at head `5d9afa5`: artifact `d7538b07aa834a28…` (616,064 B), `source_set_sha256 886b8fa6de4339f1…`, lock `15b2592c…`, platform-tools v1.48 verified; receipt written (M0 step 5) |
| IMPL-06 | Ready | coordinator (build executor) | `docs/solana/evidence/2026-10-14-m0-red-baseline.json` | IMPL-04, IMPL-05 | `cargo test -p musical-chairs-financial-runtime`: exactly `open_moves_real_sol`, `claim_moves_real_sol`, `allocation_golden_t10000` fail with the program's `NotImplemented` code and fee-only rollback; other tests pass; CLI receipt saved (M0 step 7) |
| IMPL-07 | Ready | coordinator + Haiku (hashing) | `docs/solana/evidence/2026-10-14-m0-graph-refresh.json`, graph | IMPL-06 | `graphify update .` AST over the adopted tranche + scoped receipt preserving prior identities; handoff with commits, commands, proof level |
| IMPL-08 | Done | Sonnet worker (claude-sonnet) | `solana/crates/economics/src/backers.rs`, `solana/crates/economics/tests/widths.rs`, `solana/fixtures/economics.json`; program `allocation.rs::floor_proportional` call-through | IMPL-00 (host-only crate; no SBF build contention; worker may run the std-only economics workspace tests with its own scratch `CARGO_TARGET_DIR`) | M1: U256 `sealed_denominator`/`pro_rata`/`seal_denominator`/`accumulate_a_remainder` + U256 accumulator and denominator in `Ticket`; `floor_proportional` delegates to the crate; 72 → 77 host tests, clippy `-D warnings` and fmt green under the coordinator's own run; program crate `cargo check --locked --lib` exit 0 (log `impl08-program-check.log`) |
