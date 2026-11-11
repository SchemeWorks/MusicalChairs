# Lookout roadmap queue — 2026-10-28

**Request.** Rob asked Gideon and Silas to "get a bunch of task cards queued up on the Lookout page". Gideon relayed the request as a bounded docs and board task.

**Change.** Planning and visibility only. Nineteen queued cards were added so the remaining roadmap is visible:
- the rest of the approved test-network flow, Block A wrap-up and one optional rehearsal;
- the held Internet Computer delivery rows and the held legacy delivery.

Every card is `Blocked` or `Backlog`. None is `Ready`, and none was started. No hold, approval, deadline or ranking changed. Nothing was built, tested, sent, signed, funded or deployed.

The canonical rows are in `kanban.md` (in the "Local implementation pass" table and the "Native board priorities" table). The sanitized copies are in `lookout-projection.json`.

## Sources read first

- The current board.
- The frozen approval packet `native-solana-m14-approval-packet-20261023.md` (sha256 `ae8a6a0e…392f`, unchanged): §1, §6.1–6.5, §7, §8 and §9.
- The flow plan `docs/solana/m14-flow-plan.md`: §3 steps, §5 arithmetic, §6 findings, §8 time bounds and §9 limits.
- The flow runner section of `docs/solana/devnet-ops.md`.
- The accepted flow receipts.
- The program's expiry classification (`expiry_stage`, `solana/programs/musical_chairs/src/close.rs`).
- The current `lookout-kanban/docs/PUBLISHING.md`.

## New cards

Every native child is owned by Silas as operator, with the immutable main-flow runner `d3c66966…`.

**Dates.**
- Each date is the plan's earliest time, counted from the actual activation: genesis 2026-10-25T15:31:35Z, read from the network, plus the plan offset.
- Each later date depends on its predecessors, and the program's own gates still decide. These are maturity, the 600-second draw deadline and strict insolvency.
- Step 10's window end is A1's actual open plus 7 days, where the 12% tier ends.

| ID | Title (short) | State | Depends on | Priority |
| --- | --- | --- | --- | --- |
| M14-A7-WD1 | First player withdrawal and the backer's fee share (steps 10–11) | Blocked: 2026-11-03T13:11:35Z ≤ cluster clock < 15:37:47Z | steps 1–9 (accepted) | 1 |
| M14-A7-CLAIM1 | First backer claim before any draw (12) | Blocked | M14-A7-WD1 | 1 |
| M14-A7-CLOSE1 | Close round 1, line up candidates (13–16) | Blocked: plan earliest 2026-11-06T02:23:35Z | M14-A7-CLAIM1 | 1 |
| M14-A7-DRAW1 | First live draw: request, real oracle answer, capture (17–19) | Blocked: same sitting, inside 600 s | M14-A7-CLOSE1 | 1 |
| M14-A7-PROMO | Promote the draw winner (20) | Blocked | M14-A7-DRAW1 (timely capture) | 1 |
| M14-A7-REOPEN | Finalize round 1 and open round 2 in one transaction (21) | Blocked | M14-A7-PROMO | 1 |
| M14-A7-R2SETUP | Round 2 deposits and backer funding (22–27) | Blocked | M14-A7-REOPEN | 1 |
| M14-A7-C15 | First compounding payout (28) | Blocked: maturity; plan earliest 2026-11-21T02:28:35Z | M14-A7-R2SETUP | 1 |
| M14-A7-CAP | Backer cap and combined claims (29–31) | Blocked | M14-A7-C15 | 1 |
| M14-A7-CLOSE2 | Close round 2, request a second draw (32–36) | Blocked: maturity; plan earliest 2026-12-06T02:27:35Z | M14-A7-CAP | 1 |
| M14-A7-EXPIRE | Second draw expires, finalize round 2 (37–39) | Blocked: close + 600 s | M14-A7-CLOSE2 | 1 |
| M14-A-ACCEPT | Block A wrap-up: final evidence and Gideon's acceptance | Blocked | M14-A7, M14-F11 | 1 |
| M14-F12-OPT | Optional guardian pause and unpause rehearsal | Backlog (optional, deferred, never a gate) | — | 1 |
| M14-B1B2 | Install the bridge and the keeper, inactive (packet B1 + B2) | Backlog (held) | Rob's separate B1 and B2 approvals; MI-3 | 5 |
| M14-B3 | Live Shenanigans upgrade with the native path off (packet B3) | Backlog (held; highest risk) | Rob's separate B3 approval; Block A green; B1; MI-3; MI-4 | 5 |
| M14-B4 | Test website on a new canister (packet B4) | Backlog (held) | Rob's separate B4 approval; B1; MI-3 | 5 |
| M14-C2 | One supervised keeper window at round 2's close (packet C2): operator stop at the first of 3 journaled watchdog-signed transactions, finalize, 45 min or any incident condition (6 per hour is only the software ceiling); primary worker not running during the window | Backlog (held) | Rob's separate C2 approval; B2; close #2; how it combines with flow steps 33–39 settled with Gideon before arming | 5 |
| M14-C1C3 | Reward path on, then proof with real PP (packet C1 then C3) | Backlog (held) | Rob's separate C1 and C3 approvals; B1; B3; MI-4 | 5 |
| LEG-1-SHIP | Ship the legacy memo fix to the live Shenanigans canister | Backlog (held; priority 3 paused) | LEG-1, LEG-1-REV; Gideon's candidate acceptance; Rob's upgrade approval; MI-4 | 3 |

**Structure.**
- M14-A7 stays the parent and tracks the plan total, and each child owns the execution of its own steps. Each child's grouping follows the plan's own time bounds (flow plan §8) and its flow codes, as Gideon suggested. None was split per command.
- M14-F11 is kept as ONE atomic same-sitting card: execute reduce #3, read back, probe, document, restore #4, then the full readback. It gets no child cards and still has its exact deadline of 2026-10-29T18:01:21Z on the cluster clock.

## Honest limits recorded on the cards

- **Step 18 sends nothing on the test network.** The runner only watches the request account for the real ORAO answer, for up to 600 s. The plan's synthetic fulfilment exists only in the local simulation. Real answer time is still unmeasured.
- **A missed first draw.** It is an allowed, recorded outcome (packet §7 A7). The committed plan's winner steps 20, 26 and 30 then cannot run as written, so the run stops and goes to Gideon, with no reroll and no retry.
- **The second draw is never captured, by design.** The program classifies the expiry from its own draw state, so an oracle answer does not change the path.
- **The step 10 window.** Missing it means stop and report: F6 depends on F3 at the 12% tier (flow plan §6 item 4).
- **Optional rehearsal (packet F12, default D11).** It has no plan line and no tool command, would need a reviewed command first, and is never a gate.
- **The held Internet Computer rows (B1–B4, C1–C3).** Each still needs its own separate approval, and none has been requested. C3 would mint real, irreversible PP, capped at 5,000. C2 is not a dependency of the flow.
- **Gideon's pre-publication correction (2026-10-28).**
  - **C2 now carries the packet's stricter operator stop (§4f, frozen packet lines 347–352).** The window switches off at the FIRST of:
    - 3 journaled watchdog-signed transactions;
    - finalize landed;
    - 45 minutes;
    - an unexpected blocked reason, a failed, expired or unknown entry, a domain mismatch, or the fee address below its floor.
    `maxSignaturesPerHour = 6` is only the software ceiling. The primary worker is not running during the window.
  - **Shared flow acceptance text.** It now covers only sending steps and explicitly excepts the observation-only step 18 and the simulation-only expected-refusal step 37. Both have zero sends; their evidence is the observed real fulfilment and `refused_as_expected`.

## Historical cards qualified, not erased

- **M14-TOOL and M14-RUN:** "has not run on devnet" now reads as true at that milestone, pointing to M14-A7 and M14-F11.
- **M14-A3:** "initialization has not happened yet" now reads as true at that point, done later in M14-A4.
- **M14-5A-REPIN:** "nothing has been deployed" now reads as true at that point, deployed later in M14-A3.
- **M14-PKT-R:** "awaiting Block A" and "the first public step is starting" are now marked historical.
- **Glossary:** "M14" no longer says the trial has not started, and the M14-RUN "12/12" definition was corrected.

Accepted results are unchanged.

## Counts

| | Done | In progress | Blocked | Backlog | Total |
| --- | --- | --- | --- | --- | --- |
| Before | 26 | 2 | 1 | 0 | 29 |
| After | 26 | 2 | 13 | 7 | 48 |

- **Priority links.** Rank 1 has 35 cards (+13) and rank 3 has 3 (+1). Rank 5 went from none to 5. Ranks 2 and 4 are unchanged. The order, ids, titles, statuses, reasons and next actions are unchanged.
- **Coverage.** 45 of 48 cards are linked. M14-PKT-R, M14-GRAPH and PR-125 stay unlinked, as before.

## Validation (offline; no product suites, build, network call or graph rebuild)

- **Publisher parser** (`parseMarkdownBoard`, curated JSON): 48 cards, source revision `0bef3e9880edf6e9eae40e69936c4748324493508761fb2ebfba58ee4c6faf10`.
- **Priorities.** The publisher's `validatePriorities` passed. Every list is in projection order, with no card under two ranks. The kanban priority table equals the file for all five ranks, and every projected card has exactly one canonical row.
- **Glossary.** It has 72 entries. "Carried Interest", "backer", "Block A" and "Shenanigans" now apply to all cards, and 8 terms were added: keeper, oracle, promotion, pot, candidates, Jackpot Fee, canister, snapshot. Every term appears on its card or, if it applies to all cards, on at least one card.
- **Privacy.** No keys, addresses, balances, signatures, payloads, local paths or logs are in the projection. The only link host is the approved draft PR #125.
- **Summaries.**
  - Technical summary: 61 words, 344 codepoints.
  - Plain summary: 3 sentences, 54 words, 311 codepoints, with no acronyms, hashes or labels. It is written to Rob's 2026-10-28 rule.
- **Hashes.**
  - projection `f04457c26ef8b5a3f7036f2a0fdfc56b39b9108944d4a1008920a865802f45c5`
  - priorities `13e56230192ff73ca010b36620f2b4ab034db1b7b3108f0a5f983b3d00a88593`
  - presentation `2567293c981f146f8ff42bef027d5512c1994be7ebd1c25db96e5bb8a0f9c140`

**Publication.** Through the same scoped manifest and identity. The receipt is in `lookout-publication-log.md`.
