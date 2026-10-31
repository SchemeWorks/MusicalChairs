# M14 flow plan amendment — 2026-10-31: run F3 (steps 10–12) now

**Status: CANDIDATE for Gideon's bounded acceptance.** No revised devnet transaction has been sent. Card M14-A7-ACCEL.

## 1. Authority and scope

- **Rob's direct instruction.** Given in Gideon's Codex chat on 2026-10-31 and relayed in Gideon's bounded handoff (sha256 `eb032da57631b460f013c989864a0a4f791d3cdf3a5a2307c13c5d7fb42bd3d7`): "Then revise the test so we can do it now. Have Silas handle it and verify the later checks still work. I don't want to wait days just because of a test plan we chose."
- **What it covers.** A revision of the chosen Block A devnet test schedule and its operator, then, after Gideon accepts, the newly eligible steps once under the existing guards.
- **What it does not cover.** Production economics or maturity rules, the deployed program, governance, ICP, mainnet, financial holds, unknown-outcome retries, funding ceilings or the PR merge.

## 2. What changes, and what does not

| item | before | after |
| --- | --- | --- |
| F3 offset (`F3_AT`) | `O1 + 596,160` (6.9 d after the planned open; genesis + 769,200) | `O1 + 345,600` (4.0 d; genesis + 518,640) |
| steps 11 and 12 | genesis + 769,230 / 769,290 | genesis + 518,670 / 518,730 |
| F3 send window (new runner guard) | none: only "not before genesis + offset" | refused unless the ACTUAL time since A1's on-chain `opened_at` is at least 342,000 s and under 604,200 s; checked before anything is signed, simulated or sent, in a dry run too |
| plan label | `M14-SIM Seq-B` | `M14-SIM Seq-B` unchanged, plus an additive `plan_revision` "2026-10-31 F3 at 4.0 d" in status, dry-run and receipt documents |
| operator | accepted `d3c66966…` (22,600,400 bytes) | candidate `13b25fb3…` (22,997,376 bytes), built once from commit 5b32a02; the accepted runner is kept unchanged beside it |

Unchanged:
- the program, its IDL and ELF (`0c88d4e7…`);
- the frozen packet (`ae8a6a0e…`, byte-identical);
- steps 1–9, their receipts (sha256 `6385fc3d…`) and genesis;
- every amount, wallet funding, cap and budget;
- every other offset, including step 13 and everything after it.

The runner's "too early" check is kept. The F3 window is an additional refusal.

## 3. Discretionary offsets versus enforced times

| step(s) | what decides the time | can it move earlier? |
| --- | --- | --- |
| 10–12 | the plan's choice (the old 6.9 d was chosen for the 12 % tier) | **yes**, down to the F6 limit below; that is this amendment |
| 13 | strict insolvency, from the economics: A1's accrual exceeds the round 1 pot 808,390 s after its open, whatever F3 pays | no. On chain from 2026-11-06T00:10:57Z; the plan step stays at 2026-11-06T02:23:35Z (the original 2.2 h margin, worth 10,300,000 lamports of insolvency) |
| 17, 19 | oracle deadline: request and capture only while `now <= close_started_at + 600` | no |
| 28 | Compound15 maturity: P1's actual open + 1,296,000 s | no |
| 32 | Compound30 maturity: W's actual open (step 21) + 2,592,000 s | no |
| 37–39 | expiry needs `now > close_started_at + 600` | no |

No deposit can bring step 13 earlier: more deposits only grow the pot. Moving any later step would need changed production durations or rates, which this instruction does not authorize.

## 4. The limiting inequality (why 4.0 days, and the window)

F6 requires A1's Series A row to reach its 124 % cap in the Compound15 ticket (step 29).

- **What that ticket can give A1.** Its senior slice is 35 % of half of the Compound15 toll: 35 % × 40,262,092 ≈ 14.09 M lamports.
- **What round 1 credits A1.** In round 1, A1 is the only open row, so it receives the whole Backer half of both tolls:
  - credit = ½ × 12 % × F3 paid + ½ × 7.5 % × (1,029,200,000 − F3 paid)
  - = 38,595,000 + 2.25 % × F3 paid.
- **The condition.** F6 holds only if 62,000,000 − credit ≤ what the Compound15 ticket gives A1.
- **How F3 paid depends on time.** F3 paid is A1's accrual, 1 SOL × 11 % per day × elapsed.

The economics oracle bisection (`f3_earliest_viable_elapsed`) puts the smallest viable F3 elapsed at **325,118 s (3.763 d)**. Below that, the arithmetic reports "F6 fails".

The window:
- **Lower bound, 342,000 s** (4.0 d less a 1-hour late-open allowance). That leaves 16,882 s (4.7 h) of margin over the computed minimum. At the bound, A1 has 13,608,126 left, against a slice of about 14.09 M.
- **Upper bound, 604,200 s (exclusive).** That is 10 minutes before the 12 % tier ends at 7 days. At or after 7 days the toll is 7.5 % and the plan's tier claim would be false.

Every other condition holds across the whole window: F3 solvent, close 1 strictly insolvent, A1 a loser, Compound15 solvent with more than a 10 % margin, Compound30 strictly insolvent, and the winner's combined claim with both parts.

## 5. Old against new expectations (model, lamports)

| step | before (F3 at 6.9 d) | after (F3 at 4.0 d) |
| --- | --- | --- |
| 10 F3 | owed 759,000,000 of pot 1,029,200,000; toll 12 % 91,080,000; net 667,920,000 | owed 440,000,000 of the same pot; toll 12 % 52,800,000; net 387,200,000 |
| 11 ticket 1 | A1 credited 45,540,000; seed 45,540,000 | A1 credited 26,400,000; seed 26,400,000 |
| 12 claim | a_part 45,540,000 | a_part 26,400,000 |
| 13 close 1 | owed 280,500,000 > pot 270,200,000 (by 10,300,000); toll 7.5 % 20,265,000; net 249,935,000 | owed 599,500,000 > pot 589,200,000 (by 10,300,000); toll 7.5 % 44,190,000; net 545,010,000 |
| 14 ticket 2 | A1 credited 10,132,500 (remaining 6,327,500) | A1 credited 22,095,000 (remaining 13,505,000) |
| A1 round 1 | net payouts 917,855,000 (loser of 82,145,000) | net payouts 932,210,000 (loser of 67,790,000) |
| 21 seed carried | 55,672,500 | 48,495,000 |
| 28 Compound15 | pot 1,038,872,500; owed 894,713,151; spare 144,159,349 | pot 1,031,695,000; owed 894,713,151; spare 136,981,849 (15.3 %) |
| 29 ticket 3 (F6) | A1 credited its last 6,327,500 → 124 % | A1 credited its last 13,505,000 → 124 % |
| 30 Winner claim | a_part 9,058,970; b_part 8,052,418 | unchanged |
| 31 A1 capped claim | 16,460,000 | 35,600,000 |
| 32 Compound30 | pot 144,159,349 (owed 51×); toll 18,740,715; net 125,418,634 | pot 136,981,849 (owed 54×); toll 17,807,640; net 119,174,209 |

These are the plan's model figures at exactly 4.0 d. The real F3 will run at whatever elapsed time it is sent inside the window. The runner computes each payout from the chain's own pre-state and checks it against the economics oracle, and the arithmetic test proves the later checks for every hour of the window.

For example, the read-only dry run at elapsed 358,278 s previewed: paid 456,142,245, toll 54,737,069, net 401,405,176.

**Budgets.** Deposits, rent, fees and each wallet's need are unchanged. Only receipts move:
- A1 receives 994,210,000 in the model instead of 979,855,000; its net spend falls to 0.060854 SOL.
- W receives 119,174,209 instead of 125,418,634; its net spend rises to 0.484473 SOL, still covered by its 0.630 SOL funding against a need of 0.603647.

There is no new funding source and no reserve release.

## 6. Timing boundaries (cluster clock, UTC)

| boundary | unix | UTC |
| --- | --- | --- |
| A1's on-chain open | 1,793,115,467 | 2026-10-27T15:37:47Z (132 s after the planned open) |
| step 10 plan earliest | 1,793,460,935 | 2026-10-31T15:35:35Z |
| F3 window opens / closes (exclusive) | 1,793,457,467 / 1,793,719,667 | 2026-10-31T14:37:47Z / 2026-11-03T15:27:47Z |
| effective step 10 interval | | **2026-10-31T15:35:35Z to before 2026-11-03T15:27:47Z** |
| steps 11, 12 plan earliest | | 2026-10-31T15:36:05Z, 15:37:05Z (both run straight after step 10) |
| close 1 possible on chain | 1,793,923,857 | 2026-11-06T00:10:57Z |
| step 13 plan earliest | 1,793,931,815 | 2026-11-06T02:23:35Z |
| step 28 (Compound15) | P1's actual open + 1,296,000 s | plan 2026-11-21T02:28:35Z if step 22 is on time |
| step 32 (Compound30) | W's actual open + 2,592,000 s | plan 2026-12-06T02:27:35Z if step 21 is on time |
| steps 17, 19 / 37–39 | close start + 600 s (inclusive for request and capture; expiry strictly after) | |

## 7. Proof

**Local proof** (model and LiteSVM on the production ELF with the devnet ORAO binary; not devnet proof):
- `m14_flow_plan`, the three pure tests, pass. Among them, `f3_window_keeps_every_later_check` checks every hour of the window plus its last second, against the nominal close and closes 132 s and 3,600 s later, and checks that the close threshold does not depend on F3.
- The plan's LiteSVM rehearsal passes, with the model equal to the chain for the seed carried and both round 2 pots. Duration is unchanged: 3,582,371 s.
- `m14_flow_tool`: 15 plain tests pass, including `the_f3_window_closes_ten_minutes_before_the_12_percent_tier_ends`.
- `m14_flow_tool`: both whole-flow runs through `flow next` pass.
  - The nominal run.
  - `the_f3_window_guard_and_a_late_open_still_reach_every_later_check`. A1 opens 3,700 s late. F3 is refused at the plan's instant (elapsed 341,900 s), with nothing sent or simulated. It is accepted at 342,000 s, the worst case for F6. All 39 steps then pass, including A1's cap at ticket 3.

**Devnet evidence so far is read-only.** Status with the candidate shows:
- 9/39 done, 0 foreign receipts, the books balanced;
- step 10 ready, with the F3 window inside;
- the step 10 dry run built only the transaction, with no signature and no simulation;
- the write-ahead and receipts files byte-identical before and after.

Evidence: `docs/solana/evidence/2026-10-31-f3-acceleration/` (pre-state, status and dry run with both runners, rehearsal report, test results, pins).

## 8. After acceptance

With the candidate runner `13b25fb3…`, steps 10, 11 and 12 run, each once:
- fresh `flow status` and the standard pre-send guards first;
- each step dry-run, then executed once;
- conservation, readback and append-only receipts.

Any failed check, refusal, contradiction or unknown outcome means STOP: no retry and no alternate executor. Step 13 and later keep the times in section 6, on the existing monitor.
