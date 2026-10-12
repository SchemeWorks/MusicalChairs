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
| SOL-07 | Done | `/root/round_design` | Preserve round reset, candidate completeness and failure recovery | SOL-01, SOL-02 | Frozen current-round lists; coordinator rejected fresh retries/automatic skip and selected one authenticated client-scoped request; outage tradeoff explicit |
| SOL-08 | Done | coordinator; `/root/economic_design_review` | Integrate recommendations into spec and roadmap | SOL-05, SOL-06, SOL-07 | Independent review accepted direction/vectors; width/conservation and request-initiation findings corrected and follow-up confirmed; design only |

## Shared dispatch contract

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
