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

## Agent names

On the native Solana work, the Claude coordinator is **Silas** and the Codex supervisor and reviewer is **Gideon**. Rob's rule: never reuse an agent first name across projects or chats, and check existing names before assigning one. **Miles** is already the Rumi Base bridge supervisor. A name changes no role, execution or approval scope.

## Native Solana explanations

Rob's background is extensive ICP development with Solidity coursework and DeFi analysis. When a Solana-specific choice affects implementation or behavior, briefly explain the concept, its closest ICP/EVM comparison and its concrete effect on Musical Chairs. Cover concepts when they arise, such as separate code/data accounts, PDAs versus signing keys, transaction account/compute bounds, rent, cross-program calls, asynchronous randomness and finalized observations. Keep this explanation alongside the relevant decision; distinguish proposed behavior from tested facts.

## Lookout board mirror

`docs/coordination/kanban.md` is the sole authority for the native board. `docs/coordination/lookout-projection.json` is a **sanitized, derived** mirror of the current material cards in its "Local implementation pass" section, for Rob's private Lookout board. The mirror's shape is `{cards:[{id,state,owner,scope,evidenceText,evidenceLinks:[{title,url}]}]}`, and the publisher rejects any other field. The Lookout setup chat owns the publisher configuration and the manifest.

- **States.** Use only the publisher's standard column states: `Backlog`, `Ready`, `In progress`, `Review`, `Done`, `Blocked`, `Cancelled`. The board allows at most 12 columns, and each distinct state is a column. Put any qualifier, such as "done locally, not deployed" or "awaiting Gideon", at the start of `evidenceText` as `State detail: …`.
- **When to update.** After every material board checkpoint, update the projection in the same commit as the kanban change. A material checkpoint is a card being added, a card changing state, or a card's evidence changing materially. Include new material cards honestly, and keep state wording consistent with the kanban.
- **Sanitation.**
  - Write fresh summaries; never copy raw board cells.
  - Exclude keys and credentials, wallet or program addresses, balances, transaction payloads or signatures, private logs and local user paths.
  - `evidenceLinks` may contain only approved public HTTPS links, currently draft PR #125. New hosts or new sensitive content need fresh approval.
  - Keep source, test, local, independent-review and approval evidence distinct. Never imply approval, deployment, or public execution that has not happened.
- **Publishing (standing authority).** Setup's manifest cutover is complete: `scripts/musicalchairs.mainnet.json` (`source.kind` `curated-json`) reads this worktree's projection. The first publication was verified at backend revision 2. Rob approved future sanitized board updates without being asked each time ("Keep all connected boards synced without asking me each time"). After every material checkpoint, update the projection and commit it, then run these from the lookout-kanban checkout:
  1. `node scripts/publisher.mjs --config scripts/musicalchairs.mainnet.json --dry-run`
  2. `--publish` only if the dry run shows a change.
  3. `--dry-run` again, which must report no change (the readback).
  4. Append the backend revision and source revision to `docs/coordination/lookout-publication-log.md`.

  Use the same manifest and its scoped identity only. Never inspect or export secrets. Do not touch setup's `.local` seed. Recording a receipt in the log is not a material checkpoint, so do not republish for it. Any NEW tool or permission rejection is binding: never route around it with another executor, identity or tool. New sensitive content or new link hosts still need fresh approval.
- **Plain-English status and glossary.** `docs/coordination/lookout-presentation.json` has the shape `{summary, glossary:[{term, definition, cardId?}]}` and is described under "Plain-English status and clickable explanations" in the lookout-kanban `outputs/PUBLISHING.md`. Update it alongside every material board checkpoint, in the same commit as the projection.
  - Keep the summary brief (about 35–60 words) and current. State the local acceptance and the held public approval plainly, with no hashes, internal review ids or undefined abbreviations.
  - Glossary terms are literal, case-sensitive words that appear on the cards. Scope any term whose meaning differs by card with its exact `cardId`, including review labels, flow or version labels, and numeric counts such as `12/12`. Remove entries whose term no longer appears.
  - Never guess a meaning or reuse another project's shorthand. Apply the same sanitization as the projection. No new timer.
  - Setup's `presentationPath` cutover is complete: the manifest points at this file. Each publisher run publishes the cards first and then binds the summary and glossary to their exact source revision. Metadata-only changes still publish, and identical repeats are no-ops.
- **Priorities (ordered sub-projects).** `docs/coordination/lookout-priorities.json` has the shape `{items:[{id,title,status,blockedReason,nextAction,cardIds}]}`; the canonical copy of the ranking is the "Native board priorities" section of the kanban. Gideon (supervisor) chooses the ranking; Silas maintains the file and the board.
  - **Format.** Status is one of `ready`, `in_progress`, `blocked`, `paused`, `done`. `blocked` and `paused` need the reason and what clears it; `ready` and `in_progress` need the next action. Keep the rank when a status changes.
  - **Choosing work.** At every checkpoint, worker completion, blocker-clear event and existing supervision wake, pick the highest-ranked authorized `ready` or `in_progress` item. When it blocks, record the block and what clears it, then move straight to the next ready item. Return to the higher item at its next safe checkpoint.
  - **Limits.** Never interrupt irreversible work or duplicate an owner, and keep useful approved independent workers running. If nothing is available, record the exact blockers and rely on the existing monitoring. Never manufacture optional gates or authority. No new timer.
  - **Publishing.** The file is published in the same checkpoint flow as the cards, summary and glossary, using the same sanitization. Alden owns the initial `priorityPath` cutover; until Alden confirms readiness, do not publish it or call the priorities API.
- **Boundaries.**
  - Never read or export keyring secrets.
  - Board-sync approval grants nothing beyond board updates. Block A on devnet, including the section 5a fresh program id, is already explicitly approved by Rob: never ask for Block A approval again. The B/C rows (ICP), real PP minting, unknown-outcome retries, mainnet and the PR #125 merge each remain separately held.
  - Add no new timer: the existing 30-minute supervision check covers mirror freshness.
  - A publication failure is a visibility issue only. It never justifies bypassing an authority boundary.
  - Board-metadata authority grants nothing for Solana keys, faucets, signing or deploys, or for any ICP upgrade, mint, activation or merge.
