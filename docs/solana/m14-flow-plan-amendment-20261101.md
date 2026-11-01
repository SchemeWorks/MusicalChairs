# M14 flow plan amendment — 2026-11-01: close 1 at the strict-insolvency threshold

**Status: CANDIDATE for Gideon's bounded review.** Nothing has been sent and nothing may be sent under this candidate before he accepts it. Card M14-A7-CLOSE1. His five documentation precision fixes from the bounded review (scoped basis, the step 37 exception, the usable step 37 interval, "can turn", the build provenance) are applied; the tested code 29c8f97 and the candidate `a9dcb1b3…` are unchanged.

## 1. Authority and scope

- **Rob's direct instruction.** Given in Gideon's Codex chat on 2026-11-01 and relayed in Gideon's bounded handoff: "Have Silas remove the extra two-hour margin and shift the dependent steps with it. Keep the game rules unchanged and verify the later checks still work."
- **Basis.** The accepted step 13 timing assessment ([m14-step13-timing-assessment-20261101.md](m14-step13-timing-assessment-20261101.md), accepted at 03e3625) found that the 7,958 s of chosen margin before step 13 can be removed without changing any payout amount or adding a financial step. The later post-maturity 60 s and the permissionless step spacing also remain chosen timings; this amendment keeps them.
- **What it covers.** A revision of the Block A devnet test schedule and its operator:
  - the margin removed;
  - the dependent steps shifted with it;
  - a minimal scheduling and evidence correction for step 37 within its accepted definition;
  - local proof, and one new candidate runner build.
- **What it does not cover.**
  - Any send, including step 13.
  - The program, IDL or ELF, economics, rates, durations, amounts or budgets.
  - Steps 1–12 and their receipts.
  - The frozen packet and the accepted earlier amendment sections.
  - Governance, ICP, mainnet and the PR merge.

## 2. What changes, and what does not

| item | before | after |
| --- | --- | --- |
| step 13 offset (`CLOSE1_AT`) | `O1 + 816,480` (genesis + 989,520 = 2026-11-06T02:23:35Z) | `O1 + 132 + 808,390` (genesis + 981,562 = **2026-11-06T00:10:57Z**, Nov 5 16:10:57 PST) |
| steps 14–39 | derived from `CLOSE1_AT` | the same derivation, so each moves by exactly −7,958 s, EXCEPT step 37, which moves an additional 450 s earlier (−8,408 s in all; next row and section 5) |
| step 37 offset | `C30_AT + 600`, on the deadline second | `C30_AT + 150`, 30 s after the close 2 request, inside the deadline (section 4) |
| step 37 "done" | its `refused_as_expected` receipt, OR chain state once the deadline had passed, OR any later step | ONLY its `refused_as_expected` receipt; a missed window is reported as MISSED and stops the plan (section 4) |
| plan label | `plan_revision` "2026-10-31 F3 at 4.0 d" | "2026-11-01 close 1 at the strict-insolvency threshold"; `plan` stays `M14-SIM Seq-B`, so the 12 receipts resume |
| operator | approved `13b25fb3…` (22,997,376 B) | ONE immutable candidate **`a9dcb1b3…`** (23,020,064 B) from commit 29c8f97, after two `cargo build --bin devnet_ops` invocations: the first found the binary already current (cargo had built it from the same sources during the test runs) and gave `a9dcb1b3…`; the second, forced by touching the sources, recompiled and reproduced `a9dcb1b3…`. Both earlier main runners are kept unchanged beside it |

What sets the new step 13 time:
- A1's owed first exceeds the round 1 pot at elapsed 808,390 s. One second earlier owed − pot is −1,042 lamports, and at 808,390 s it is +231. The threshold does not depend on F3, because F3's payment lowered owed and pot by the same amount.
- On devnet, A1 actually opened at 1,793,115,467, which is 132 s after `genesis + O1`.
- The plan time is therefore the threshold measured from that actual open: no margin and no nominal-open substitute.

Unchanged:
- the program and its IDL and ELF (`0c88d4e7…`);
- the economics, rates and durations;
- every amount, wallet funding, cap and budget;
- steps 1–12 (ids, offsets, actions, amounts), their receipts (sha256 `2d56a1bb…`, 12 lines) and genesis;
- the frozen packet (`ae8a6a0e…`, byte-identical);
- the accepted earlier amendment.

Step 13 gets no new gate. It keeps:
- the plan-clock "too early" check, which now opens exactly at the threshold for this run;
- the existing insolvency pre-check, which refuses a payout that would not close.

## 3. What the program enforces, and what the plan chooses

The accepted assessment separates these, and this amendment removes only the chosen margin before step 13.

What the program enforces stays exactly as it was:
- strict insolvency before close 1;
- the Compound15 and Compound30 maturities, measured from each position's ACTUAL open;
- the 600 s promotion deadline (request and capture at or before it, expiry strictly after it).

The runner pre-checks each of these from chain state before it signs anything.

If an open runs late, the plan instant can come before the game allows the step. The runner then refuses with a plain reason until the actual instant, and the local rehearsal shows each case: the close, both settlements and the expiry.

The permissionless 15–60 s step spacing is unchanged.

## 4. Step 37: a minimal scheduling and evidence correction

Step 37 is an expected-refusal simulation with zero sends. Its only evidence is a `refused_as_expected` receipt (`DeadlineNotReached`). Gideon's 2026-11-01 review recorded two problems as an open runner risk.

**The window.**
- The old plan put step 37 on the deadline second. On devnet the refusal is possible only while `now <= close 2 start + 600`, and close 2 starts whenever step 32 actually runs.
- The usable window was therefore `[genesis + C30_AT + 600, step 32 + 600]`: as wide as step 32's lateness, so a prompt step 32 left seconds.
- The plan now simulates step 37 at `C30_AT + 150`, 30 s after the request. Its nominal window is `[genesis + C30_AT + 150, actual close 2 start + 600]`, inclusive.
- The usable interval is narrower: it starts at max(the plan earliest, the completion of steps 33–36), because the runner runs steps strictly in order, and operator and RPC time can consume it. No fixed width is guaranteed. Only if step 32 runs on time and steps 33–36 keep their 30 s spacing do about 450 s remain.
- The LiteSVM rehearsal still proves the exact boundary: an expiry is refused at its own instant AND on the deadline second, and accepted one second later.

**The substitution.**
- The runner used to mark step 37 done from chain state once the deadline had passed, and also whenever a later step was done.
- Now only its own receipt completes it.
- If the window is missed, `flow status` lists the step under `missed_refusal_evidence`, `all_ok` is false, and `flow next` refuses step 37 with "MISSED … not waived: stop and report".
- Steps 38 and 39 stay "not the next step". The outcome is never passed or waived.

**What stays the same.** The definition is unchanged: zero sends, the actual refusal required, the same error. No other step moved for this. The risk is reduced, not proven: the devnet window still has to be met at the time (section 9).

## 5. Revised dates (cluster clock; steps after 13 assume each earlier step ran on time)

| step | before | after (UTC) | after (Pacific) |
| --- | --- | --- | --- |
| 13 `a1_closing_withdraw` | 2026-11-06T02:23:35Z | **2026-11-06T00:10:57Z** | Nov 5 16:10:57 PST |
| 14–16 (ticket, pages, build) | 02:24:05–02:25:05Z | 00:11:27–00:12:27Z | |
| 17 request, 18 fulfil, 19 capture | 02:25:35, 02:25:50, 02:26:05Z | 00:12:57, 00:13:12, 00:13:27Z | |
| 20 apply, 21 finalize/reopen | 02:26:20, 02:26:35Z | 00:13:42, 00:13:57Z | |
| 22–27 (round 2 setup) | 02:27:35–02:32:35Z | 00:14:57–00:19:57Z | |
| 28 Compound15 (P1's actual open + 15 d) | 2026-11-21T02:28:35Z | **2026-11-21T00:15:57Z** | Nov 20 16:15:57 PST |
| 29–31 | 02:29:05–02:31:05Z | 00:16:27–00:18:27Z | |
| 32 Compound30 (W's actual open + 30 d) | 2026-12-06T02:27:35Z | **2026-12-06T00:14:57Z** | Dec 5 16:14:57 PST |
| 33–36 | 02:28:05–02:29:35Z | 00:15:27–00:16:57Z | |
| 37 expected refusal | 02:37:35Z (the deadline second) | **00:17:27Z**, window to close 2 + 600 (inclusive) | |
| 38 expiry, 39 finalize | 02:37:36, 02:37:46Z | 00:24:58, 00:25:08Z | |

The total plan length goes from 41.463 to 41.371 days. Each date is an earliest time, not a promise:
- each settlement waits for its position's actual maturity;
- each draw step waits for the real ORAO oracle within the real deadline.

All offsets and dates are recomputed independently in `close1_shift.py`.

## 6. Amounts: unchanged

The close is pot-capped (paid = pot, 7.5 % tier) at any instant from the threshold to 14 days, so moving it changes no amount. The pure test `close1_has_no_margin_and_every_later_offset_moves_with_it` compares every model output at the new and the previous close instant:
- the close payout;
- the seed;
- both round 2 pots;
- C15 and C30;
- A1's remaining entitlement;
- the winner's claims.

**Projection for the devnet run.** These are model figures from the real step 10, not chain facts yet. They are also the figures the devnet-shaped LiteSVM replay produces:

| quantity | value |
| --- | --- |
| step 13 paid / toll / net | 572,414,815 / 42,931,111 / 529,483,704 (owed − pot +231 at the threshold) |
| ticket 2: A1 credit / seed | 21,465,555 / 21,465,556 |
| seed carried into round 2 | 48,872,669 |
| A1 left after round 1 vs the Compound15 senior slice (F6) | 13,127,336 vs 14,091,732, about 0.96 M of margin |

Spending is unchanged:
- **Deposits, rent and wallet needs:** unchanged.
- **Fees:** on devnet, step 37 still sends nothing.
- **Rehearsal only:** step 37 now sends one extra failing transaction in LiteSVM (+5,000 lamports of keeper fee).
- **Funding:** no new funding and no reserve release.

## 7. Proof

**Local proof.** This is the model plus LiteSVM on the production ELF `0c88d4e7…` with the devnet ORAO binary; it is not devnet proof.

- `m14_flow_plan`, 4 pure tests, pass:
  - the new `close1_has_no_margin_and_every_later_offset_moves_with_it`: steps 1–12 unchanged; steps 13–39 shifted by 7,958 s with step 37 at `C30_AT + 150`; the threshold matches the economics oracle; devnet's F3 gives owed − pot +231 at the threshold and a solvent payout one second before; the same model outcome as at the old instant;
  - `f3_window_keeps_every_later_check`: every hour of the F3 window against the close at an on-time open, at the threshold and at the last second of the 7.5 % tier; one second below the threshold always fails as "strictly insolvent";
  - the plan-shape test;
  - the solvency-arithmetic test.
- **Plan rehearsal.** The LiteSVM run passes with model equal to chain for the seed and both round 2 pots. Step 37 is refused at its own instant and on the deadline second. Total duration is 3,574,413 s.
- **`m14_flow_tool`:** 16 plain tests pass, including `a_fresh_status_reports_no_missed_refusal_evidence`. Four whole-flow runs through `flow next` pass:
  - **Nominal.** All 39 steps. Step 37's refusal is simulated inside the deadline.
  - **Current devnet run.** It replays devnet's actual timings for steps 1–12 (A1 +132 s, P4 and P5 late, F3 at elapsed 358,784 s) and reproduces devnet's recorded facts: pot 572,414,815, seed 27,407,113, A1 remaining 34,592,891.
    - Steps 1–12 are refused as already complete.
    - Step 13 is refused one second early, with nothing simulated or sent, and sent at the threshold, with owed − pot −1,042 then +231.
    - All 39 steps pass, with F6 at ticket 3, model-equal seed and pots, `refused_as_expected` inside the deadline, and expiry at deadline + 1.
  - **Late execution** (A1 opens 3,700 s late):
    - close 1 is refused by the insolvency pre-check at its plan instant and at threshold − 1, then sent at the threshold;
    - steps 14–27 run late but inside the real deadline;
    - both settlements are refused before the positions' actual maturities, read from chain, then sent at them;
    - step 38 is refused on the deadline second and sent one second later;
    - all 39 steps pass.
  - **Missed refusal window.**
    - After the deadline, step 37 is MISSED, listed, and `all_ok` is false.
    - `flow next` refuses it and the steps after it.
    - A third-party expiry that completes step 38 does not make step 37 done.
- **Lint.** `clippy --all-targets -D warnings` is clean.

**Devnet evidence is read-only.** One finalized `flow status` with the candidate:
- read the real 12-line receipts;
- reported 12/39, 0 foreign receipts, all 12 receipts counted;
- conservation true, `all_ok` true;
- next step 13 at 1,793,923,857 (2026-11-06T00:10:57Z), not ready.

The write-ahead and receipts files were byte-identical before and after. There was no send, signature, simulation, polling, funding or governance.

Evidence: [`evidence/2026-11-01-close1-threshold/`](evidence/2026-11-01-close1-threshold/) (receipt, arithmetic script and output, finalized status, rehearsal report and log, whole-flow summary, manifest).

**Source versus local proof.**
- The tests prove the plan data, the runner's gates and the evidence rule against the real program bytes in LiteSVM.
- They do not prove devnet behaviour: real RPC timing, the real ORAO latency against the 600 s deadline, foreign activity, or that step 37's window is met on the day.

## 8. Preservation

- **Earlier runners.** Both main runners verify unchanged after the build: `d3c66966…` (22,600,400 B; steps 1–9) and `13b25fb3…` (22,997,376 B; steps 10–12; still the approved runner until acceptance).
- **Devnet files.** Receipts `2d56a1bb…` (12 lines), write-ahead `e37f76e3…` and SOL ledger `ec6acf8c…` are unchanged. There is no new ledger row, because nothing was sent.
- **Pins.** The frozen packet `ae8a6a0e…`, the ELF `0c88d4e7…`, genesis and `PLAN_NAME` are unchanged.
- **Graph.** No graph maintenance in this bounded pass. AST coverage of the four changed files is stale since the 2026-10-27 refresh, as it was for 5b32a02.

## 9. Remaining real conditions

- **Step 13 is still conditional on the unchanged live pot.** The threshold assumes the round 1 pot is still 572,414,815. The program is public, so a foreign deposit before the run moves the threshold later; the runner's pre-check then refuses until the new threshold.
- **The inflow race stays.** A foreign deposit landing between the runner's pre-check and execution can turn the close into a solvent withdrawal; whether it does depends on the raced amount and the actual execution time, and no buffer guarantees the close. The payout has no must-close flag, and this amendment does not change the program. At the threshold the cushion is 231 lamports, against about 10.1 M with the old margin; neither covers a minimum Series A (0.05 SOL). The margin's removal makes this race the main residual risk at step 13.
- **The draw.** Steps 17–19 must land inside the real 600 s deadline; devnet ORAO latency is still unmeasured.
- **Round 2 timing.** The Compound15 and Compound30 dates hold only if steps 21–22 run on time; otherwise the actual maturities decide.
- **Step 37 on devnet.** The usable interval runs from max(the plan earliest, the completion of steps 33–36) to close 2 start + 600, inclusive. Operator and RPC time can consume it, so no width is guaranteed. A miss is reported and stops; it is never counted as passed.
- **Running step 13 at all** needs Gideon's acceptance of this candidate and then his separate go-ahead.

## 10. After acceptance

- **The approved runner** becomes `a9dcb1b3…`, from its immutable copy only.
- **Step 13** may run no earlier than 2026-11-06T00:10:57Z and only with Gideon's go-ahead, through the standard guards:
  - fresh status and pins;
  - dry run, then executed once;
  - steps 14–20 in the same sitting.
- **On any problem, stop.** Any refusal, failed check, contradiction or unknown outcome means STOP: no retry and no alternate executor.
