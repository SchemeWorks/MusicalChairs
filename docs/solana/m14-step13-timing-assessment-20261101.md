# Step 13 timing assessment — 2026-11-01

**Status: ACCEPTED by Gideon (2026-11-01) at `03e3625` (receipt `e51c8fb`), with his four review clarifications applied: the inflow race, "no time minimum", the scope of the conclusion, and the step 37 risk.**
- **What he checked:** the report, the arithmetic script and output, the source logic, the exact threshold and boundary, the margin, the alternative's lower bound, the downstream amounts and F6 margin, the four manifest hashes, the one finalized snapshot, and the unchanged journal and receipt hashes.
- **What the acceptance covers:** the investigation only. The existing schedule and the immutable runner `13b25fb3…` are kept. No operator revision, new send or extra candidate withdrawal is authorized.
- **Follow-up (2026-11-01, after acceptance):** Rob then directed the margin's removal. The resulting candidate, in Review, is [the 2026-11-01 amendment](m14-flow-plan-amendment-20261101.md). This assessment's text is otherwise unchanged.
- **What stays open:** step 37's missing-refusal-evidence risk stays OPEN on M14-A7-EXPIRE, for a bounded decision before its window.

Card M14-A7-CLOSE1. Rob's request, relayed by Gideon: "Have Silas check whether we can bring step 13 forward too. Separate the waits enforced by the game from delays we chose in the test plan." Nothing here changes the program, the runner, the plan, the frozen packet or the accepted amendment. No transaction was sent.

## Answer

- **The game allows the round 1 close from 2026-11-06T00:10:57Z** (Thursday 2026-11-05, 16:10:57 PST) on the cluster clock. The accepted plan runs step 13 at **02:23:35Z** (18:23:35 PST). The **7,958 s (2 h 12 min 38 s)** between them is chosen test margin, not a program rule.
- **Removing that margin changes no amount and no outcome.** The closing payout is capped at the pot, and the toll stays at 7.5 % anywhere from the threshold to day 14, so steps 13–39 produce identical figures. Every later date would move earlier by the same amount.
- **The accepted runner cannot do it as built.** It refuses each step before genesis + its plan offset. Every later offset derives from `CLOSE1_AT`, so moving step 13 means a runner revision, a new build and Gideon's acceptance.
- **For the unchanged accepted 39-step sequence, the days before the threshold are unavoidable.** No planned action can bring the threshold earlier. The exact invariant and the alternatives examined are in section 4. This is not a claim about every possible redesign of the test.
- **Recommendation: keep the accepted schedule.** This is a cost/benefit preference, not a program restriction and not a new approval gate. The most a revision could gain is 2 h 13 min, and only through a new operator binary in the money-moving close.
  - At the accepted instant, owed exceeds the pot by 10,131,944. A stranger's minimum 0.01 SOL deposit adds 9,600,000 to the pot, so one such deposit would still leave the close insolvent.
  - A 300 s buffer would leave only 382,175, which that one deposit would erase.
  - No buffer guarantees closure (section 1). Amounts are identical either way.
  - If Rob wants the earlier sitting anyway, section 2 lists what a revision needs.

## 1. Earliest strict-insolvency close

**Rule (source).**
- Simple accrual is `floor(principal × 11 × elapsed / 8,640,000)`, with the remainder carried across claims, so split claims add up to the one-shot floor (`crates/economics/src/earnings.rs:8`, `:140–175`).
- Owed is `accrued_gross − paid_gross` (`programs/musical_chairs/src/positions.rs:852`).
- A payout closes the round only when owed is strictly greater than the pot (`positions.rs:144`; `crates/economics/src/fees.rs:82`). It pays `min(owed, pot)`, and the toll is `floor(paid × bps / 10,000)`.
- The Carried Interest tier is 1,200 bps before day 7, 750 bps before day 14, then 300 bps (`fees.rs:39`; `positions.rs:880`).
- The Simple position stops accruing at 21 days (`earnings.rs:6`, `:150`): 2026-11-17T15:37:47Z for A1, so it does not affect this.

**Inputs.**
- From the accepted receipts: A1 opened at 1,793,115,467 with principal 1,000,000,000. Step 10 paid 456,785,185, which is exactly `floor(1e9 × 11 × 358,784 / 8,640,000)`.
- From the post-step-12 status: the pot is 572,414,815 (= 1,029,200,000 − 456,785,185), and genesis is 1,792,942,295.

**Calculation.** Both the pot and A1's owed fell by the same 456,785,185 at step 10. The close condition is therefore `floor(1e9 × 11 × e / 8,640,000) > 1,029,200,000`, whatever F3 paid. The smallest such elapsed time is **e\* = 808,390 s (9.356366 d)**.

| instant (cluster clock) | elapsed | A1 owed | pot | owed − pot | result |
| --- | --- | --- | --- | --- | --- |
| 2026-11-06T00:10:56Z · 2026-11-05 16:10:56 PST | 808,389 | 572,413,773 | 572,414,815 | −1,042 | solvent: would pay in full and **not** close |
| 2026-11-06T00:10:57Z · 2026-11-05 16:10:57 PST | 808,390 | 572,415,046 | 572,414,815 | +231 | strictly insolvent: closes round 1, toll 750 bps |
| plan 2026-11-06T02:23:35Z · 18:23:35 PST | 816,348 | — | 572,414,815 | +10,131,944 | the plan's 7,958 s margin |

**Saved state vs fresh observation.**
- *Saved-state threshold.* The figures above use the accepted post-step-12 state from 2026-10-31. They hold only if nothing has entered round 1 since.
- *Fresh observation.* One read-only `flow status --commitment finalized` was taken with the approved runner `13b25fb3…` (finalized slot 517,546,407, cluster clock 2026-11-01T15:55:25Z).
  - 12/39 done, conservation true, foreign receipts 0.
  - The game vault is unchanged at 641,317,888 lamports, and pot + seed + management + pending + claims + excess is unchanged at 640,621,928.
  - Any deposit or Series A funding would have raised that total, and the only round 1 positions that could pay out are the plan's own. So the pot was still 572,414,815 at that slot, and the threshold stands as of then.
  - The cluster clock matched the local UTC clock to within 1 s.
- *Still conditional.* A stranger can open a position on the public devnet program until the close.
  - A minimum 0.01 SOL deposit would delay the threshold by 7,541 s; a minimum 0.05 SOL Series A funding by 39,273 s.
  - A foreign position would also become an extra draw candidate.
  - **Activity visible before the runner's read and simulation** produces a refusal and nothing is sent. The runner re-derives insolvency from the live pot before signing (`tests/financial-runtime/src/devnet_ops/flow.rs:1175`), and the transaction is simulated before it is sent.
  - **A deposit landing between that check and execution is a real race.** The payout instruction has no "must close" flag. If such a deposit lifted the pot to at least A1's owed, step 13 would execute as a full solvent withdrawal and round 1 would not close.
  - **Buffers.** The insolvency cushion is how far owed exceeds the pot. It is 231 at the threshold, 76,620 at +60 s, 382,175 at +300 s and 10,131,944 at the accepted instant. A larger cushion lowers this exposure but cannot guarantee closure.

## 2. The chosen margin, and what removing it would change

**How big it is.** 7,958 s = plan instant 1,793,931,815 − threshold 1,793,923,857. The plan's own model writes it as 8,090 s (10,300,000 lamports) from the planned open; A1 actually opened 132 s late.

**Downstream outcomes with the close at any instant from e\* to day 14.** Computed in `step13_timing.py`, which first reproduces F3, ticket 1 and the pot from the receipts.

| outcome | depends on the close instant? | value (projection unless it is a receipt) |
| --- | --- | --- |
| closing payout (step 13) | no: pot-capped, and 750 bps for every elapsed time from 604,800 to 1,209,599 s | paid 572,414,815, toll 42,931,111, net 529,483,704 |
| fee tier | no, until 2026-11-10T15:37:46Z | 7.5 % |
| ticket 2 (step 14) | no: A1 is the only row; same rule ticket 1 followed exactly | A1 credited 21,465,555, seed 21,465,556 |
| A1 loser and fallback-only | no | round 1 net 931,454,667, a loser of 68,545,333; Series A history, so fallback only |
| candidates (steps 15–16) | no | preferred {P4, P5}, each a loser of 10,000,000 |
| F6 cap (step 29) | no | A1 has 13,127,336 left against a Compound15 senior slice of 14,091,732: holds, margin 964,396 |
| Compound15 settlement (step 28) | no: fixed factor, round 2 pot unchanged | owed 894,713,151 of pot 1,032,072,669: solvent |
| close 2 (step 32), both candidate sets | no | Compound30 owed 7,360,607,081 against pot 137,359,518: strictly insolvent |
| conservation and wallet equations | no: the identities hold at every instant | — |
| combined claim (step 30) | no | Winner holds A and B as planned |
| 600 s deadline (steps 17, 19, 36–38) | no: measured from each close's own start (`close.rs:1073`, `:478–487`, `:1410`) | — |
| real ORAO latency (step 18) | not measured on devnet yet; a 2 h shift gives no reason to expect a difference | observed at step 18 |
| all later dates | yes: they move earlier by exactly the margin removed | Compound15 maturity +15 d after P1's open, Compound30 +30 d after W's open |

**What a bring-forward would need** (option only; no authority inferred):
- **Move `CLOSE1_AT` itself.** Changing it in `tests/financial-runtime/src/m14_plan.rs:74` moves all later offsets together (`:77–83`).
  - Moving step 13 alone would leave steps 14–17 gated at the old times (`flow.rs:1829`).
  - Step 17 would then miss the 600 s request deadline, and close 1 could only expire.
- **Replace the 1-hour late-open allowance.** The accepted window test checks the close with 3,600 s of it. It would need A1's actual 132 s instead, or a step-13 guard on actual elapsed, like the F3 window.
- **Rebuild and re-prove.** That means a new runner build and pin, a rerun of the plan's pure tests and the whole-flow operator run, an amendment and Gideon's acceptance.
- **Choose a buffer knowingly.** The payout instruction has no "must close" flag. If step 13 executed while solvent, A1 would receive a full withdrawal and the round would not close, which would break F6. That could happen through an execution a second before e\*, or a foreign inflow in the race described in section 1.
  - The runner's pre-check and simulation guard only what is visible before sending.
  - Any buffer lowers the exposure; none is a proved safety boundary. For example, 300 s gives 00:15:57Z (16:15:57 PST), saves 7,658 s and leaves a cushion of 382,175 lamports. That is smaller than one minimum foreign deposit.

## 3. Remaining waits (steps 13–39)

How to read the table:
- **"Minimum"** means the program refuses before a time.
- **"Deadline"** means it refuses after a time.
- **"Provider latency"** is outside the program.
- **"No time minimum"** means only that no additional time minimum applies. It does not mean "unconditional": the program still enforces each step's phase and order conditions, such as an open ticket blocking payouts, pages before the candidate build, a captured draw before promotion, and a terminal promotion before finalize. The runner also runs the steps strictly in plan order.
- Dates assume the accepted schedule and that each predecessor runs on time.

| step(s) | what the game enforces | type | plan's chosen offset / spacing | depends on | earliest useful (UTC) | removable |
| --- | --- | --- | --- | --- | --- | --- |
| 13 close 1 | owed > pot, i.e. elapsed ≥ 808,390 s | minimum (insolvency) | genesis + 989,520 | step 12 (done) | **2026-11-06 00:10:57** | 7,958 s, by runner revision only |
| 14–16 ticket, pages, candidates | no time minimum (phase conditions: closing ticket phases in order, pages from the locked reserve before the build) | — | +30, +60, +90 s after close | each predecessor's finalized transactions | right after step 13 | spacing only (seconds) |
| 17 request draw | request only while now ≤ close + 600 | deadline | +120 s | step 16 (candidates sealed) | right after step 16 | spacing only |
| 18 fulfilment | no time minimum (zero sends; observation of ORAO) | provider latency | +135 s (a synthetic LiteSVM figure, not a forecast) | step 17 | whenever ORAO fulfils | not controllable |
| 19 capture | capture only while now ≤ close + 600 | deadline | +150 s | step 18 | right after fulfilment | spacing only |
| 20 apply promotion | no time minimum (phase condition: a captured draw) | — | +165 s | step 19 | right after step 19 | spacing only |
| 21 finalize and reopen | no time minimum (phase conditions: a terminal promotion and the committed closing ticket) | — | +180 s | step 20; W's actual open here starts Compound30's 30 days | right after step 20 | spacing only |
| 22–27 round 2 setup | no time minimum (phase condition: round 2 open) | — | 60 s apart | step 21; P1's actual open at 22 starts Compound15's 15 days | minutes after step 21 | spacing only |
| 28 Compound15 settle | now ≥ P1's open + 1,296,000 s | minimum (maturity) | maturity + 60 s | step 27 and maturity | 2026-11-21 02:27:35 (if 22 is on time) | 60 s |
| 29–31 ticket 3 and claims | no time minimum (phase conditions: ticket phases in order; claims after the commit) | — | +30, +90, +150 s | step 28 | right after step 28 | spacing only |
| 32 Compound30 settle (close 2) | now ≥ W's open + 2,592,000 s | minimum (maturity) | maturity + 60 s | step 31 and maturity | 2026-12-06 02:26:35 (if 21 is on time) | 60 s |
| 33–35 | no time minimum (phase conditions as for 14–16) | — | +30, +60, +90 s | each predecessor | right after step 32 | spacing only |
| 36 request draw 2 | now ≤ close 2 + 600 | deadline | +120 s | step 35 | right after step 35 | spacing only |
| 37 refused expiry (simulation only) | expiry is refused while now ≤ close 2 + 600; the evidence exists only then | deadline (an upper limit for the evidence) | runner earliest genesis + C30_AT + 600 | step 36 | between the runner's earliest and the real deadline (see the risk below) | the chosen +600 s |
| 38 expire | now > close 2 + 600 | minimum (strictly after the deadline) | +601 s | step 37 | close 2 + 601 s | none |
| 39 finalize | no time minimum (phase condition: a terminal promotion) | — | +611 s | step 38 | right after step 38 | spacing only |

The last plan instant is 2026-12-06T02:37:46Z. The long remaining waits are the two maturities, 15 and 30 days, which the program enforces and which start at the actual round 2 opens.

**Unresolved test-runner risk: step 37's evidence window.** This does not block step 13 or this assessment, but it needs a bounded decision or fix before close 2 (planned 2026-12-06).
- Step 37's refusal simulation can only be captured between the runner's earliest (genesis + C30_AT + 600) and the real deadline (close 2 + 600).
- That window is exactly as wide as step 32's lateness against its own earliest. Sending step 32 promptly at its earliest could leave a window of seconds.
- Once the deadline passes, the runner counts step 37 as done from chain state (`flow.rs:575–578`), and the flow advances. **That does not satisfy the accepted 39-step requirement for a `refused_as_expected` simulation.**
- If the window is missed, step 37's evidence is missing. It is not waived and must not be counted as passed.

## 4. Can planned actions bring the close earlier?

**No.** The invariant:
- Since step 12, the round 1 pot changes only through deposits, Series A funding and payouts.
- A1's own payouts lower its owed and the pot by the same amount. So A1 closes exactly when `floor(1e9 × 11 × e / 8,640,000) > 1,029,200,000` + (any inflow) − (other positions' payouts).
- The cluster clock is the only thing that moves e.
- None of steps 13–39 can legally run before step 13: steps 14–21 need the close, and steps 22–27 are round 2. Running round 2's deposits early would only enlarge the round 1 pot.

**Alternatives examined that need extra transactions or changed amounts** (described only; no authority inferred):
- **P4 and P5 withdraw their earnings just before the close.** This lowers the pot without lowering A1's owed. The earliest close would then be 2026-11-05T19:46:47Z (11:46:47 PST), at most 15,850 s (4 h 24 min 10 s) before the threshold.
  - Cost: two extra withdrawals and two extra keeper tickets (a payout is refused while a ticket is open), plus new runner steps.
  - P4's loss would fall from 10,000,000 to about 666,898, so the Winner's Series B entitlement would fall from 11,600,000 to about 773,601.
  - That changes ticket 3's B part, the combined claim, the wallet budgets and the accepted evidence coverage.
- **Another deposit in round 1 does not help.** A deposit adds 96 % of itself to the pot at once, and its own 11 % per day takes 8.73 days to draw that back out.
- **Changing rates, durations or the deployed program** is a production change and is out of scope.

**Conclusion.** For the unchanged accepted 39-step sequence, the 4 d 8 h 15 min 32 s from the fresh snapshot to the threshold is mathematically unavoidable. The governing invariant is `floor(1e9 × 11 × e / 8,640,000) > 1,029,200,000`, with no planned inflow or other payout before step 13.

Among the alternatives examined here, the most that could be saved is 6 h 36 min 48 s: the margin, plus the candidate-withdrawal option, which changes amounts and coverage. This is not a claim about every possible redesign of the test. The recommendation to keep the schedule is a cost/benefit preference, not a program restriction or a new approval gate.

## 5. Commands and results

- **Arithmetic.** `python3 -I step13_timing.py`, an integer-only copy of the rules cited above. It asserts F3's paid, toll and net, the step-10 one-second-earlier preview (456,783,912), ticket 1's credit and seed, and the pot before computing anything new. Exit 0. Output in `step13_timing.out.json`.
- **Fresh snapshot.** `devnet_ops flow status --wallet-dir <wallets> --rpc https://api.devnet.solana.com --cluster devnet --commitment finalized`, with the approved runner `13b25fb3…` from its immutable copy (hash rechecked). Exit 0, read-only.
  - It reads only the public `wallets.json`, the program accounts, the rent sysvar, the clock and the slot.
  - The write-ahead and receipts files were byte-identical before and after.
  - No receipts path was passed, so it judged the steps from chain state alone (`receipt_recorded` false). Every step's verdict matches the post-step-12 status.
- **Not run.** No build, test suite, rehearsal, simulation that sends, transaction, monitoring loop or second RPC read.

Evidence: [`evidence/2026-11-01-step13-timing/`](evidence/2026-11-01-step13-timing/) (script, output, finalized status, file hashes, manifest).

## 6. Residual uncertainty

- **Foreign activity before the close.** A stranger's deposit or Series A would delay the threshold, change round 1 amounts and could add a candidate. The snapshot shows none as of slot 517,546,407.
  - Activity before the runner's read and simulation produces a refusal.
  - A deposit racing between that check and execution cannot be excluded. The cushion lowers this exposure but cannot remove it (section 1).
- **Step 37 refusal evidence** is an open test-runner risk that needs a bounded decision or fix before close 2 (section 3). Until then it is unresolved, not passed.
- **Cluster clock drift.** All times are on the cluster clock, which matched UTC within 1 s at the snapshot. The runner uses the cluster clock, so drift moves the wall-clock moment, not correctness.
- **Projections.** The F6 margin (964,396) and the round 2 figures are projections from the receipts and the program's arithmetic. They are not chain facts until those steps run.
- **ORAO latency on devnet** is still unmeasured; step 18 measures it against the 600 s deadline.
