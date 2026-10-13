# Musical Chairs — repo notes for Claude

## Naming: "Exit Toll" → "Carried Interest"

The user-facing term for the withdrawal fee on Simple positions is **Carried Interest** (a deliberately VC-sleazy euphemism). The compounding-plan version is still called the **Jackpot Fee**.

Internal code identifiers still use the old name `exitToll`:

- Backend: `calculateExitToll` ([backend/main.mo](backend/main.mo))
- Frontend constants: `EXIT_TOLL_EARLY`, `EXIT_TOLL_MID`, `EXIT_TOLL_LATE` ([frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts))
- Frontend helpers: `calculateExitTollFee`, `getExitTollInfo`

These were left unrenamed deliberately to avoid backend churn for a pure naming change. **Do not rename these identifiers** without explicit instruction. Treat them as the internal name for the same concept the UI calls "Carried Interest".

When writing new user-facing strings, use **Carried Interest**. When writing new code that touches these identifiers, you can either keep using `exitToll`/`EXIT_TOLL_*` or introduce parallel `carriedInterest` names — coordinate with the user before doing the latter.

## Naming: "Cover Charge" → "Front-End Load"

The user-facing term for the entry fee skimmed off every deposit is **Front-End Load** (mutual-fund jargon — on-brand with the VC/MLM voice). This is the third name for this concept:

1. **Entry Skim** (original)
2. **Cover Charge** (renamed once)
3. **Front-End Load** (current)

Internal code identifiers still use the second name `coverCharge` / `COVER_CHARGE_RATE`:

- Backend: `coverCharge`, `payManagement` and related ([backend/main.mo](backend/main.mo))
- Frontend: `COVER_CHARGE_RATE` ([frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts)), `coverChargeData`/`coverChargeLoading` (admin wallet UI), and various comments

These were left unrenamed deliberately to avoid backend churn for a pure naming change. **Do not rename these identifiers** without explicit instruction. Treat them as the internal name for the same concept the UI calls "Front-End Load".

When writing new user-facing strings, use **Front-End Load**.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After a coherent Rust/TypeScript implementation change, run `graphify update .` for structural extraction. This AST-only command does not refresh semantic design documents or establish complete Motoko coverage.
- After changing the native-Solana spec, plan or economic/recovery contract, semantically extract the changed documents with the Graphify skill and merge into the existing repository graph. Preserve unrelated nodes, edge direction and hyperedges; record per-file hashes and scoped freshness. Follow [the graph maintenance workflow](docs/coordination/native-solana-knowledge-graph.md).
- Each implementation handoff records graph maintenance performed, covered files, query checks and any stale coverage. A graph relationship is navigation evidence; verify financial/authority claims against current source and tests.

## Native Solana explanations

Rob's background is extensive ICP development with Solidity coursework and DeFi analysis. When a Solana-specific choice affects implementation or behavior, briefly explain the concept, its closest ICP/EVM comparison and its concrete effect on Musical Chairs. Cover concepts when they arise, such as separate code/data accounts, PDAs versus signing keys, transaction account/compute bounds, rent, cross-program calls, asynchronous randomness and finalized observations. Keep this explanation alongside the relevant decision; distinguish proposed behavior from tested facts.
