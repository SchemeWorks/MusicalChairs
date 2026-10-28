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
- **When to update (ongoing duty).** Rob asked on 2026-10-27, in Alden's Lookout chat as relayed by Gideon, that boards on the live site stay current whenever a task moves.
  - While working, update the kanban promptly whenever a task moves: a card is added, or its status, progress or evidence, blockers or next step change. That is a material checkpoint.
  - In the same commit, update the projection, BOTH summaries (technical `summary` and `plainSummary`), the affected glossary entries, and the priorities, including each priority's explicit `cardIds`. Both summaries bind to the exact board source revision.
  - Then publish promptly (see Publishing below).
  - This applies while working. It is not a claim of autonomous updates while idle.
  - Unchanged content stays a no-op.
  - It needs no new timer, worker, build, test, graph refresh or live Solana/ICP action.
  - Include new material cards honestly, and keep state wording consistent with the kanban.
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

  If a real publication hold or an auto-review denial prevents a live update:
  - Preserve it as reported.
  - Record the exact visibility blocker in the kanban and the publication log.
  - Tell Gideon and Rob.
  - Never retry or bypass it through another identity, tool or executor.

  The old denial of a root publisher attempt (`.lock`) stays binding for root. Silas's normal publisher run with this manifest is the authorized path, not a substitute executor.
- **Status summaries and glossary.** `docs/coordination/lookout-presentation.json` has the shape `{summary, plainSummary, glossary:[{term, definition, cardId?}]}`. The current reference is the lookout-kanban `docs/PUBLISHING.md`, section "Summary, definitions, and priorities". Update it alongside every material board checkpoint, in the same commit as the projection.
  - **Technical `summary`.** Keep it brief (about 35–60 words, under 600 codepoints) and current. State the local acceptance and the held public approval plainly, with no hashes, internal review ids or undefined abbreviations.
  - **`plainSummary`** (Rob's request, 2026-10-27; Lookout shows it by default, with a toggle to the technical view).
    - Write 2–3 short sentences in ordinary language for a general reader, under 600 Unicode codepoints.
    - Cover overall progress, what is done and works, what blocks it, and what happens next.
    - No unexplained acronyms, hashes or internal labels.
    - Report only verified facts. A finished software task does not mean deployment, signing, money movement or readiness.
    - Whenever the meaningful status changes, update both summaries together.
    - If `plainSummary` were omitted, the publisher would leave the old plain summary in place, so never drop it to skip an update.
  - Glossary terms are literal, case-sensitive words that appear on the cards. Scope any term whose meaning differs by card with its exact `cardId`, including review labels, flow or version labels, and numeric counts such as `12/12`. Remove entries whose term no longer appears.
  - Never guess a meaning or reuse another project's shorthand. Apply the same sanitization as the projection. No new timer.
  - Setup's `presentationPath` cutover is complete: the manifest points at this file. Each publisher run publishes the cards first and then binds the technical summary, the glossary and the plain summary (published separately) to their exact source revision. Metadata-only changes still publish, and identical repeats are no-ops. The publisher's dry run reports the plain summary separately; the readback must show it unchanged too.
- **Priorities (ordered sub-projects).** `docs/coordination/lookout-priorities.json` has the shape `{items:[{id,title,status,blockedReason,nextAction,cardIds}]}`; the canonical copy of the ranking is the "Native board priorities" section of the kanban. Gideon (supervisor) chooses the ranking; Silas maintains the file and the board.
  - **Format.** Status is one of `ready`, `in_progress`, `blocked`, `paused`, `done`. `blocked` and `paused` need the reason and what clears it; `ready` and `in_progress` need the next action. Keep the rank when a status changes.
  - **Card links.** Lookout shows each linked card's priority number, with the sub-project name on hover or focus. This is Alden's card badge, live since 2026-10-27.
    - Each priority's `cardIds` lists the projected cards whose canonical board scope belongs to that sub-project alone, in projection order, completed cards included.
    - Choose them from the scope, never by id prefix or keyword.
    - A card whose scope spans several sub-projects stays unlinked, and no card sits under two ranks.
    - Keep the links current at every checkpoint.
    - The rule and the current mapping are in the kanban "Native board priorities" section and `docs/coordination/lookout-priority-links-20261027.md`.
  - **Choosing work.** At every checkpoint, worker completion, blocker-clear event and existing supervision wake, pick the highest-ranked authorized `ready` or `in_progress` item. When it blocks, record the block and what clears it, then move straight to the next ready item. Return to the higher item at its next safe checkpoint.
  - **Limits.** Never interrupt irreversible work or duplicate an owner, and keep useful approved independent workers running. If nothing is available, record the exact blockers and rely on the existing monitoring. Never manufacture optional gates or authority. No new timer.
  - **Publishing.** Alden's initial `priorityPath` cutover is complete: confirmed live 2026-10-26, and the manifest points at this file with the same scoped identity. From now on, every material checkpoint publishes the priorities together with the cards, summary and glossary, through the same manifest and identity: dry-run, then `--publish` only if something changed, then a no-op readback. Same sanitization.
- **Boundaries.**
  - Never read or export keyring secrets.
  - Board-sync approval grants nothing beyond board updates. Block A on devnet, including the section 5a fresh program id, is already explicitly approved by Rob: never ask for Block A approval again. The B/C rows (ICP), real PP minting, unknown-outcome retries, mainnet and the PR #125 merge each remain separately held.
  - Add no new timer: the existing 30-minute supervision check covers mirror freshness.
  - A publication failure is a visibility issue only. It never justifies bypassing an authority boundary.
  - Board-metadata authority grants nothing for Solana keys, faucets, signing or deploys, or for any ICP upgrade, mint, activation or merge.
