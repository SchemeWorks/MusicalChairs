# Lookout priority card links — 2026-10-27

**Change.** Metadata only. It links each priority to its board cards so the Lookout board can show a card's priority number and sub-project title on hover, derived only from the existing priorities order and explicit `cardIds`. Ranks, ids, titles, statuses, blocked reasons, next actions, holds, ownership and card meanings are unchanged. The projection and presentation files are unchanged. This grants nothing for any Solana or ICP action and does not move either network-clock deadline.

**Source.** Requested by Gideon (supervisor), relaying Alden (Lookout setup), who reports Rob asked for the hover feature. The mapping is Gideon's recommendation, confirmed by Silas's owner review against each card's canonical board scope.

| File | Before | After |
| --- | --- | --- |
| board and projection source commit | `d31bff2` | (this change) |
| `lookout-projection.json` sha256 | `f37da75ff253b87a2470df9f46524a32d3e0330ee826d03349e5b0596f8bc03e` | unchanged |
| `lookout-priorities.json` sha256 | `37c56a860784e9abc9b11a6e23e134888cec3d1b77b2128a40155f301285b01d` | `83a8109ced5aa03448be62b71fc7bc61d8d3049b57bf6254ca24e46a591c04f3` |

## Mapping

| Rank | Priority id | Cards (projection order) | Count |
| --- | --- | --- | --- |
| 1 | native-solana-devnet-acceptance | M14-PRE-ORAO, M14-PRE-SQUADS, M14-TOOL, M14-SIM, M14-RUN, M14-TOOL-REV, M14-TOOL-FIX, M14-TOOL-RECHECK, M14-TOOL-FIX2, M14-F3-HOLD, M14-A-APPROVAL, M14-5A-REPIN, M14-A1, M14-A2, M14-A3, M14-FUND-TOOL (added); M14-A4, M14-A5, M14-A6, M14-A7, M14-F11, M14-F11-TOOL (kept) | 22 |
| 2 | private-board-visibility | LOOKOUT-PROJ (kept) | 1 |
| 3 | legacy-memo-repair-delivery | LEG-1, LEG-1-REV (kept) | 2 |
| 4 | legacy-failed-mint-recovery | LEG-2 (kept) | 1 |
| 5 | icp-integration-production-activation | none: no current projected card is specific to it | 0 |

Coverage: 26 of the 29 projected cards are linked (22 native plus 4 others). No card is linked under two ranks.

**Owner review of the 16 added cards.** Each canonical scope is Block A devnet preparation or execution only:
- local compatibility checks of the devnet oracle and the real multisig binaries (M14-PRE-ORAO, M14-PRE-SQUADS);
- the devnet operator tool, its flow simulation and its runner (M14-TOOL, M14-SIM, M14-RUN);
- the tool's independent reviews and fixes (M14-TOOL-REV, M14-TOOL-FIX, M14-TOOL-RECHECK, M14-TOOL-FIX2, M14-F3-HOLD);
- Rob's devnet-only Block A approval, which holds B/C outside it (M14-A-APPROVAL);
- the fresh program-id re-pin (M14-5A-REPIN);
- devnet steps A1 to A3;
- the guarded devnet wallet funding (M14-FUND-TOOL).

None is chosen by id prefix.

## Intentionally unlinked (shared scope)

| Card | Why it stays unlinked |
| --- | --- |
| M14-PKT-R | The approval packet also carries the held ICP B/C rows (B1/B2 ceilings, the C2 window), so it spans ranks 1 and 5. |
| M14-GRAPH | Its canonical scope is the graph refresh "for the M14 preparation and legacy-card work", so it spans the native and legacy sub-projects. |
| PR-125 | The branch-wide draft review spans accepted M0–M13 work plus the M14 and live boundaries. |

These cards are not forced into one rank. If a later scope change makes one sub-project-specific, link it then.

## Rule going forward

- At each normal board checkpoint, in the same commit as the card change, keep each priority's `cardIds` equal to the projected cards whose canonical scope belongs to that sub-project alone, in projection order and including completed cards.
- Never guess a link from a keyword or id prefix.
- Keep the kanban "Native board priorities" table's cards column identical to the file.

## Validation (offline; no test, build, network call or graph rebuild)

- The publisher's own `validatePriorities` passed against the current projection: schema, allowlisted fields, statuses, required reasons, every card present, no repeated card within an item, and size limits.
- Compared field by field with `d31bff2`: rank order, ids, titles, statuses, blocked reasons and next actions are identical. Only rank 1's `cardIds` changed, as a superset of the old six.
- No card appears under two ranks, and every list is in projection order.
- The kanban priority table matches the file for all five ranks.
- Privacy: the only new strings are card ids already present in the published projection. There are no new links, keys, addresses, balances, signatures or paths.

## Publication

**Readiness hold: SATISFIED and released (2026-10-27).** The original hold, committed at `50353dd`, read: do not publish until Alden explicitly confirms the hover feature is ready. It is satisfied as follows.

- **Gideon's independent read (relayed).** Gideon independently read Alden's completed card-badge response in the Lookout chat (turn `01a17f5e-a18f-7982-919b-caba43c022f5`). Alden says the feature is live in Lookout. Task cards show the sub-project's priority number beside the agent's name, and hovering over or focusing the number shows the sub-project name. Alden reports that the tests passed and that the deployed frontend matches the build.
- **What Silas did not see.** Silas did not read Alden's chat. That confirmation is relayed by Gideon, not independently observed.
- **What Silas observed locally.** The local lookout-kanban checkout is at commit `4254a0d` ("Show sub-project priority on task cards"). That is source evidence only, not deployment evidence.
- **Scope of the release.** Only this temporary sequencing hold is released. No real publication rejection had been observed for this scope.

**Rule from here.** Publish with the same scoped manifest and identity: dry run, publish only what changed, an immediate no-op readback, then a receipt row in `lookout-publication-log.md`. Nothing is visible until that receipt exists. Any real rejection is preserved as reported, with no retry or bypass.

**Publication receipt.** Published 2026-10-27T22:15Z from `4d880fb` through the same manifest: board backend revision 21, source revision `c7f49347…3be0`; presentation and priorities bound to that revision; the immediate readback reported board, presentation and priorities unchanged. Row in `lookout-publication-log.md`.

**Ongoing duty.** Rob asked on 2026-10-27 that the live board stay current whenever a task moves. The links therefore follow every normal board checkpoint, as set out in CLAUDE.md "Lookout board mirror".

**Priorities file.** Unchanged since `50353dd` (sha256 `83a8109c…`). The readiness commit adds one sentence to the LOOKOUT-PROJ card's evidence, covering the ongoing duty and the explicit links. That is a card change, so the board's source revision changes with it. The mapping, the 26/29 coverage and the three unlinked cards are unchanged.
