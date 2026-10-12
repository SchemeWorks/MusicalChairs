# Musical Chairs native Solana discovery

User brief: retain ICP frontend hosting and current GUI; move financial custody, accounting, and execution to native Solana. Discovery and draft planning only; no product code or live actions.

Source baseline: `main`, `f96b33f` (verify full HEAD during synthesis). Existing untracked `.claude/skills/`, `.codex/`, and `AGENTS.md` are preserved.

| ID | State | Owner | Task | Depends on | Evidence / blocker |
| --- | --- | --- | --- | --- | --- |
| SOL-01 | Done | `/root/economy_inventory` | Current economics and financial inventory | — | Coordinator checked rates, senior-first allocation, gross accrual, fees and reset references; handoff in native-solana-discovery.md |
| SOL-02 | Done | `/root/native_solana_gap` | Current SOL custody and native Solana gaps | — | Coordinator checked Motoko authority, threshold signing, obsolete native draft and boundedness gaps |
| SOL-03 | Done | `/root/frontend_preservation` | GUI preservation and wallet/data changes | — | Coordinator checked asset hosting, actor hooks, SIWS/PP and CSP submission boundary; corrected native fallback recommendation |
| SOL-04 | Done | coordinator; `/root/draft_design_review` | Integrate research, review risks, draft spec and roadmap | SOL-01, SOL-02, SOL-03 | Accepted as discovery drafts with corrections applied; Rob confirmed fresh launch and ICP PP/Shenanigans; no execution/launch approval implied |

## Shared dispatch contract

- Workers inspect the same checkout read-only; only the coordinator writes documentation.
- Requested/actual native worker model: `gpt-5.6-luna`; narrow source investigations fit the lower-cost model. Authenticated Sonnet 5 availability confirmed via the Sonnet verdict tool; reserve it for substantive independent review.
- Harness: native collaboration workers; Sonnet verdict tool for review. Worktree: `/Users/robertripley/coding/musicalchairs`; branch `main`.
- Acceptance: exact file/line evidence, distinction between source and deployed behavior, assumptions and blockers, migration scope, and proposed durable persona lessons where justified.
- Integration gate: coordinator source checks and independent design review. No merge, deployment, signing, fund movement, or implementation is authorized by this board.
- Planning artifacts will mark unconfirmed decisions explicitly; they are drafts for discussion.

## Draft artifacts

- [Discovery and evidence](native-solana-discovery.md)
- [Draft specification](../superpowers/specs/2026-10-12-native-solana-design.md)
- [Draft implementation roadmap](../superpowers/plans/2026-10-12-native-solana.md)
- Handoffs accepted as research results, not production financial verification.
- Verification: source cross-checks and documentation structure checks passed. Product tests/builds skipped because no product files changed.
- No branches merged, programs deployed or funds moved.
- Documentation branch: `codex/native-solana-discovery-20261012`; source baseline remains `f96b33ff95020458be754bfa0d7471b72a7403af`.
- Review requested model: Sonnet 5. Actual source/document review: did not run; automatic approval review rejected private repository disclosure to an external Claude destination without provider-specific authorization. Availability check succeeded without reading repository contents.
- Safe fallback: native `gpt-5.6-luna`, requested and dispatch-confirmed, completed bounded independent review of coordinator-authored drafts. No external-provider retry or workaround.
- Review corrections applied: source/branch provenance, alternate payout and recovery parity, randomness outage behavior, rent payer/refund/liveness, replay-safe SIWS PP reward binding. Backer algorithms and remaining D3–D9 decisions remain explicitly unproved/unapproved.
- User scope confirmed 2026-10-12: fresh native pool with legacy positions withdrawable separately; SOL financial game first; PP/Shenanigans stay on ICP. No imports/native PP project. Existing PP desk stays a separate ICP/Chain Fusion rail; native desk replacement deferred.
