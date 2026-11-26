# Native Solana knowledge graph maintenance

Date: 2026-10-12. Scope: repository navigation, accepted contracts and implemented host models/local probes. Graph output is not runtime or deployment evidence.

## Existing graph and scoped refresh

The existing repository graph is `graphify-out/graph.json`. Its original report is dated 2026-06-28 at commit `18b7811e2334bdbe8a207b221f4e860302891504`, with 6,304 nodes and 8,344 edges. It contains legacy Motoko concepts, frontend code and historical plans. Source coverage outside this refresh is historical, not reverified at the current branch.

This refresh adds the October native-Solana specification, implementation plan, economics, processing/randomness recovery, defaults, operating budget, discovery, skill assessment and this workflow, plus the repository instructions in `CLAUDE.md`. Capture source SHA256 and line anchors. Rob subsequently accepted D1–D9 defaults and authorized host models/local probes in [launch decisions](../solana/launch-decisions.md). Label policy as accepted, implemented host models as host-tested, and future production controls as accepted-but-unimplemented. Costs and packing fixtures remain estimates or explicitly scoped measurements. No native program, runtime transaction, production endpoint or funding result exists merely because it is described in the graph.

The coordinator preserves the root graph and merges scoped semantic fragments. The durable planning fragment/receipt is stored under `docs/coordination/evidence/`, and subsequent model/probe fragments under `docs/solana/evidence/`; generated `graphify-out/` files remain ignored by Git as already configured. Keep the original whole-graph baseline and add scoped freshness metadata instead of replacing it with a current-HEAD claim. Unrelated source changes require their own refresh.

## Implementation handoff workflow

1. Query the existing graph before source navigation; use exact source paths/node identities to distinguish legacy rails from the proposed native program. Read the cited current source before relying on any authority or financial relationship.
2. After a coherent Rust/TypeScript change, run `graphify update .` for supported structural extraction. Verify the output covers the changed symbols/files. Macro-generated Anchor instructions and cross-program boundaries may need explicit semantic links; never fabricate cross-language `calls` edges. Motoko remains limited coverage and needs source checks.
3. After a spec/plan/contract change, use the Graphify skill's semantic extraction for the changed documents. Native lower-cost agents may write separate fragments; only the coordinator writes the shared graph. AST-only update is not sufficient for document semantics. Do not send private repository content to an external extraction provider without the appropriate authorization.
4. Replace obsolete nodes/edges for the refreshed files, then merge new fragments with the repository graph. Preserve other files, original edge direction, confidence labels and old/new hyperedges. Check for duplicate IDs, dangling references, stale decision wording and accidental unrelated graph shrinkage. Use actual source hashes; do not stamp untouched files as refreshed.
5. Save semantic cache and scoped manifest entries for files actually extracted. Regenerate report/community navigation. A graph larger than 5,000 nodes is expensive to render in full: use an aggregate community visualization if needed rather than promising a readable full-node diagram.
6. Check queries for the changed mechanism and its contract/test/frontend relationships. At minimum, for this migration check financial authority/ICP boundaries, Backer allocation, claim-triggered closure/randomness expiry, operations cost assumptions and implementation dependencies. Verify the scoped SHA256 pins still match.
7. Include graph scope, commands/checks, covered hashes, stale coverage and evidence limitations in the workstream handoff. Commit readable contracts and durable extraction/receipt changes with the work they describe. Product tests, economic conservation proofs, provider compatibility and deployment receipts remain distinct.

Keep the graph at repository scope when refreshing a subcomponent. Do not run a subfolder rebuild that overwrites the existing root graph. Do not advance the entire manifest to current contents when only a subset was semantically extracted.

## Solana explanations during the work

Explain a Solana concept when it affects an actual choice: what it means, how it differs from familiar ICP/EVM behavior and why it matters here. Examples are programs versus data accounts, PDA authority without a private key, explicit transaction account lists and compute limits, refundable storage balances, cross-program invocation, later oracle fulfillment and RPC finality. Keep explanations short and tied to the code or UX under discussion; identify hypotheses and measurements separately.

Rounds remain claim-triggered: neither a timer nor a keeper closes a healthy round. Potential follow-up work is pending Backer fee allocation or continuation of a triggered close. Prefer a measured complete inline payout path; use permissionless chunks where necessary. Keepers are continuity infrastructure for those pending transactions. The eight-row/42,170-transaction monthly budget is an unmeasured scenario, not inherent Solana overhead or an accepted capacity cap.
