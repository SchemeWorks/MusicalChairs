# Accepted native Solana plan — local implementation go-ahead

Codex accepts final pushed planning commit `3109063e782d5b37a55db79cbd71d61df6f64f25` on `codex/native-solana-plan-20261013`. Claude agrees in review packet §14.A; root and both independent targeted reviewers PASS. This is the final spec/plan; no additional design or Rob local-stage approval is required.

- `docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md` — SHA256 `01189e4e10e8753dccd598d83a44417dc3065ea129c7ece0dd4b747b66e24929`
- `docs/superpowers/plans/2026-10-13-native-solana-implementation.md` — SHA256 `1a328d367adb8387120a3a09353a56ab4f040350665541f6f809dac0a52882e9`
- `docs/coordination/native-solana-decision-register-20261013.md` — SHA256 `2f873676903fd180c89ade0e9093b34639ce9c4676df725365c03f30f8f40941`
- `docs/coordination/native-solana-codex-review-packet-20261013.md` — SHA256 `6641bc99e9b7e910df3d82ed478acaa348330909000e845fb4e7883fdf279044`

Acceptance evidence: `docs/solana/evidence/2026-10-14-codex-plan-acceptance.json`. Exact source preservation and graph checks pass: 21 draft/board/manifest pins unchanged; graph 8,071 nodes / 11,422 directed links / 36 hyperedges preserves prior raw identities, no duplicates/dangling references. Graph evidence is navigation/design evidence, not financial execution proof.

## Authorized execution

**Start local implementation now, milestones M0–M13 under this plan.** Use the existing Claude planning conversation/coordinator in MusicalChairs and `/Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs`. Claude owns worker dispatch/implementation; Codex owns independent review and integration decisions. Do not create another coordinator/chat/board or competing implementation worker.

1. Re-read repo instructions and accepted plan. Use canonical `docs/coordination/kanban.md`: explicit owned card, model actually invoked, file scope, dependencies, acceptance checks, evidence and handoff before every worker writes. Prefer cheaper Sonnet for substantive implementation and Haiku for bounded mechanical/evidence work. One build executor; no duplicate full CI or shared build contention. Apply local persona/orchestration skills where relevant.
2. M0 first adopts the preserved uncommitted production tranche verbatim with exact inventory hashes on `codex/native-solana-production-core` from `c99a636`, before fixing it. Integrate the final planning commit deliberately, preserving all dirty/untracked files and other worktrees. No reset/clean/force-push/deletion or global installs. Treat existing docs/source bytes as their stated proof stage, not readiness.
3. Begin M0/M1, then follow milestone dependencies through M13. Use meaningful failing assertions, valid fixtures and the exact compiled ELF; host models alone cannot prove program financial paths. Run actual local SBF instructions with ownership/signer/CU/transaction/account checks, focused tests/CI and independent milestone/final review. No fake RED from a CLI observation, compile error or invalid fixture. Fix test-outline typos by deriving valid assertions from accepted spec, not by weakening financial invariants.
4. Preserve accepted economics/D1–D9, fresh launch, separate accessible legacy withdrawals, existing ICP-hosted GUI/look/feel and PP/Shenanigans/desk on ICP. No new cap, cooldown, economic rewrite, arbitrary custody override or position import. Implement the six typed phase instructions, complete reserve/refund/cash invariants, once-only PP protocol and measured small-N inline path. Keep source/tests/CI/merge/deployment/finality/balances separate in reports.
5. Keep the graph current at coherent milestones, preserving historical nodes/raw directed edge identities/hyperedges and honest scoped freshness. Include this acceptance/go-ahead evidence in the first appropriate scoped graph/docs commit; do not fabricate global freshness. Use complete source-set digests and exact artifact hashes. Explain Solana quirks when they affect design or tests, with short ICP/EVM comparisons.
6. After each coherent milestone, return exact commits/changed files, commands/outcomes, proof level, staffing/ownership, scoped graph receipt and remaining blocker. Continue authorized local work without another routine user approval. On genuine tool/approval rejection, preserve the denied action/reason and stop that action; do not reroute via another executor. Escalate concrete architecture/custody/economic departures to Codex before changing scope.

## Not authorized

Stop before M14/M15 public-network actions. No devnet/mainnet deployment, live wallet/key access, live signing/broadcast/funding, oracle/Squads registration, governance activation, production ICP canister/frontend upgrades, operating service activation, legacy shutdown or GitHub PR merge. Local synthetic fixtures and local validator loading are authorized development tests only, never live deployment evidence.

Return a brief acknowledgement with the initial M0/M1 card ownership and preserved adoption evidence, then proceed. Codex will keep supervising; no need to wait for Rob to wake up.
