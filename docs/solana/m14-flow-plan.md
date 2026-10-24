# M14 flow plan (packet RG-7): the devnet acceptance flows with concrete amounts

Status (2026-10-23): **designed and compile-checked, not yet executed.** The plan is code
(`solana/tests/financial-runtime/tests/m14_flow_plan.rs`) and this document is its human form. Every
number marked (model) was derived from the host economics oracles (`musical-chairs-economics`: `fees`,
`earnings`, and the `Book` allocation oracle the program is differential-tested against) and re-derived
in an independent script; the LiteSVM run prints the chain's own values next to them and fails on any
disagreement it can decide (section 10). Replace the (model) values with the recorded ones after the
first run (`MC_M14_FLOW_REPORT=<path>` writes them as JSON).

Nothing here touches a network, a real key or a deployed program: LiteSVM only, the production SBF ELF
and the DEVNET ORAO binary (`MC_ORAO_PROFILE=devnet`), in-test keypairs.

## 1. Result in one table

| item | value |
| --- | --- |
| schedule | the packet's Seq-B (Simple-first), production economics unchanged |
| flows of the plan's M14 list reached | all 15 flow codes (section 4), none unreachable with the default terms |
| total duration | **3,582,371 s = 41.463 days** after the G approvals (packet Seq-B: 40.74; reason in section 6, item 2) |
| first live draw evidence (close 1 request) | **day 11.454** (packet: about 10.7) |
| earliest possible close 1 | day 11.359 (A1's owed first exceeds the pot 808,390 s after its open); the plan closes at day 11.453 for a 10,300,000 lamport margin |
| gross game deposits | 1.94 SOL in 7 positions (packet cap 6.0); largest single deposit 1.0 SOL (packet cap 1.0) |
| Series A | 3 x 0.05 = 0.15 SOL (packet cap 0.5; each below the 0.25 cap) |
| test wallets, devnet-priced need | 2.121 SOL for the 8 wallets (packet bucket 7.0), Sponsor 0.500 SOL into operations, of which about 0.018 SOL becomes permanent pages (bucket 1.0), Keeper about 0.0002 SOL of fees (bucket 0.1) |
| rounds / closes | round 1 closes as close 1 (promotion applied), round 2 closes as close 2 (deadline expiry), round 3 opens |

## 2. Wallets

| wallet | role in the plan |
| --- | --- |
| S (Sponsor) | funds operations (0.5 SOL); the ops account then pays every shared page |
| K (Keeper) | permissionless fee payer: pages, ticket phases, candidates, draw, expiry, finalize |
| A1 | Backer 1 and the dominant round 1 Simple depositor (1.0 SOL); its two payouts generate the tolls that credit, and in round 2 cap, its own Series A row |
| A2 | Backer 2: Series A only, funded at the start of round 2 |
| P4, P5 | round 1 candidates: a 0.01 SOL Simple position each (the program minimum); the draw promotes one of them (the "Winner") |
| Winner | whichever of P4 / P5 the draw selects: funds Series A in round 2 so it holds A and B |
| P1 | round 2 Compound15 (0.2 SOL), settled solvent |
| P2, P3 | round 2 Simple positions (0.1 and 0.02 SOL): extra pot and extra candidates of close 2 |
| W (plain wallet) | submits the composed finalize/reopen transaction and thereby opens the round 2 Compound30 (0.6 SOL); its settlement at day 41.456 closes round 2 |

The packet says "5 players, 2 backers, 1 plain wallet": P1-P5, A1-A2, W. The harness' fixed
`owner`/`attacker` actors are not used; every wallet is a fresh in-test keypair funded in steps of at
most 2 SOL.

## 3. The plan

`day` and `offset` are measured from the instant the G approvals are given (genesis); the first
transaction is at 172,800 s because `initialize` and the ORAO `register` cannot execute earlier (G's delay
applies to those two actions only; they are represented by the harness' genesis fixture and
`provider_setup`, not by plan steps). Amounts are the lamports each step moves from the actor. "Winner"
is resolved at run time from the `PromotionApplied` event.

| # | day | offset (s) | actor | step | amount (SOL) | expected outcome | flow |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 2.000 | 172,800 | S | `fund_operations` | 0.5 | Ops.free_balance += 0.5 SOL; OperationsFunded | setup |
| 2 | 2.000 | 172,830 | K | `provision_page(Registry 0)` |  | registry_pages 1; rent from Ops.free_balance | setup |
| 3 | 2.001 | 172,860 | K | `provision_page(Snapshot 0)` |  | snapshot_pages 1 | setup |
| 4 | 2.001 | 172,890 | K | `provision_page(Participant, round 1, 0)` |  | participant_pages 1 (round 1) | setup |
| 5 | 2.001 | 172,920 | K | `provision_close_record(1)` |  | CloseRecord(1) exists | setup |
| 6 | 2.002 | 172,980 | A1 | `fund_series_a` | 0.05 | SeriesAFunded: pot += 0.05 in full (no load), entitlement 0.062, registry_len 1 | F1 |
| 7 | 2.003 | 173,040 | A1 | `open_position` Simple | 1 | PositionOpened: Front-End Load 4 % = 0.04, pot += 0.96; round 1 pot is 1.0292 with P4 and P5 | F2 |
| 8 | 2.003 | 173,100 | P4 | `open_position` Simple | 0.01 | PositionOpened (candidate-to-be) | F2 |
| 9 | 2.004 | 173,160 | P5 | `open_position` Simple | 0.01 | PositionOpened (candidate-to-be) | F2 |
| 10 | 8.903 | 769,200 | A1 | `withdraw_simple` |  | PayoutPaid, solvent: owed 0.759 of pot 1.0292, toll 12 % = 0.09108, net 0.66792; the toll rides ticket 1 | F3 |
| 11 | 8.903 | 769,230 | K | ticket phases Discover..Commit (keeper) |  | ticket Discover..Commit; TicketCommitted: A1 credited 0.04554 (the Backer half), seed 0.04554 | F3 |
| 12 | 8.904 | 769,290 | A1 | `claim_backer` |  | BackerClaimed a_part 0.04554, b_part 0 (A alone, before any promotion) | F5a |
| 13 | 11.453 | 989,520 | A1 | `withdraw_simple` |  | strict insolvency: owed 0.2805 > pot 0.2702; PayoutPaid closing, gross_paid = pot (partial), toll 7.5 %; RoundClosed, deadline = now + 600 | F4,F8 |
| 14 | 11.453 | 989,550 | K | ticket phases Discover..Commit (keeper) |  | closing ticket 2 commits; CloseRecord.cash_committed; A1 credited 0.0101325 | F8 |
| 15 | 11.453 | 989,580 | K | `provision_page(Candidate)` x2 (locked reserve) |  | 1 preferred + 1 fallback page from the locked reserve | F8 |
| 16 | 11.454 | 989,610 | K | `build_candidates` |  | CandidatesSealed: preferred set {P4, P5}, count 2 (A1 is fallback-only: it holds Series A history) | F8 |
| 17 | 11.454 | 989,640 | K | `request_round_draw` |  | DrawRequested: fee 300,000 lamports + request rent, paid from the locked reserve | F8r |
| 18 | 11.454 | 989,655 | K | ORAO `fulfill` (synthetic authority) |  | ORAO request fulfilled by the synthetic authority (no cluster latency in LiteSVM) | F8c |
| 19 | 11.455 | 989,670 | K | `capture_round_draw` |  | DrawCaptured inside the 600 s deadline | F8c |
| 20 | 11.455 | 989,685 | K | `apply_promotion` |  | PromotionApplied: winner in {P4, P5}, loss 0.01, Series B entitlement 0.0116; B row joins the registry | F9a |
| 21 | 11.455 | 989,700 | W | `finalize_round` + `provision_page` + `open_position` Compound30 (one tx) | 0.6 | ONE tx by the plain wallet: finalize_round + participant page + Compound30 open; round 2, seed 0.0556725 carried | F9b |
| 22 | 11.456 | 989,760 | P1 | `open_position` Compound15 | 0.2 | PositionOpened; maturity +15 d | F2 |
| 23 | 11.456 | 989,820 | P2 | `open_position` Simple | 0.1 | PositionOpened | F2 |
| 24 | 11.457 | 989,880 | P3 | `open_position` Simple | 0.02 | PositionOpened | F2 |
| 25 | 11.458 | 989,940 | A2 | `fund_series_a` | 0.05 | SeriesAFunded; registry_len +1 | F1 |
| 26 | 11.458 | 990,000 | Winner | `fund_series_a` | 0.05 | SeriesAFunded: the promoted wallet now holds A and B | F1 |
| 27 | 11.459 | 990,060 | K | `provision_close_record(2)` |  | CloseRecord(2) exists | setup |
| 28 | 26.456 | 2,285,820 | P1 | `settle_compounding` |  | solvent: owed 0.89471 of pot 1.03887 (16 % spare); Jackpot Fee 9 % = 0.08052, net 0.81419; ticket 3 | F7a |
| 29 | 26.457 | 2,285,850 | K | ticket phases Discover..Commit (keeper) |  | ticket 3 commits; A1's Series A reaches 124 % (remaining 0.0063275 -> 0) and leaves the registry | F6 |
| 30 | 26.457 | 2,285,910 | Winner | `claim_backer` |  | BackerClaimed in ONE instruction: a_part 0.00906, b_part 0.00805 | F5b |
| 31 | 26.458 | 2,285,970 | A1 | `claim_backer` |  | a capped row can still claim its funded credit (0.01646) | F6 |
| 32 | 41.456 | 3,581,760 | W | `settle_compounding` |  | strict insolvency: owed 7.3606 vs pot 0.14416 (51x); gross_paid = pot, toll 13 %; deadline = now + 600 | F7b,F4 |
| 33 | 41.456 | 3,581,790 | K | ticket phases Discover..Commit (keeper) |  | closing ticket 4 commits | F10 |
| 34 | 41.456 | 3,581,820 | K | `provision_page(Candidate)` x2 (locked reserve) |  | 1 + 1 pages | F10 |
| 35 | 41.457 | 3,581,850 | K | `build_candidates` |  | CandidatesSealed: preferred {W, P2, P3}, count 3 | F10 |
| 36 | 41.457 | 3,581,880 | K | `request_round_draw` |  | DrawRequested; the request is never fulfilled | F10 |
| 37 | 41.462 | 3,582,360 | K | `expire_promotion` ON the deadline |  | refused DeadlineNotReached at now == deadline (fee only) | F10 |
| 38 | 41.463 | 3,582,361 | K | `expire_promotion` |  | PromotionSkipped(Deadline, requested-unfulfilled) | F10 |
| 39 | 41.463 | 3,582,371 | K | `finalize_round` |  | RoundFinalized: round 3 | F10 |

Each step is followed by both conservation identities (`game_vault - rent == pot + seed + management +
pending_toll + funded_claims + excess`, and `ops_cash - rent == free + locked + unallocated_excess`)
plus the partition identity `ops.locked_remaining_total == sum of every CloseRecord's remaining
partitions`, and by a per-wallet lamport equation: the actor's balance change equals receipts minus
deposits minus fees minus rent of the accounts it created, and no other wallet moves.

## 4. Where each flow of the plan's M14 list is reached

| code | flow | step(s) | day |
| --- | --- | --- | --- |
| F1 | A funding | 6 (A1), 25 (A2), 26 (Winner) | 2.002 |
| F2 | deposit | 7-9, 22-24 and 21 (W's Compound30) | 2.003 |
| F3 | Simple payout (solvent, 12 % tier) | 10-11 | 8.903 |
| F5a | claim, A part only, before any promotion | 12 | 8.904 |
| F4 | partial payout (pot-capped) | 13 and 32 | 11.453 |
| F8 | closing with candidates | 13-16 | 11.453 |
| F8r | draw request | 17 | 11.454 |
| F8c | draw capture (fulfil, then capture) | 18-19 | 11.454 |
| F9a | apply promotion | 20 | 11.455 |
| F9b | finalize/reopen composed from a plain wallet | 21 | 11.455 |
| F7a | Compound15 settlement (solvent) | 28 | 26.456 |
| F6 | cap completion (A1 reaches 124 %) | 29 and 31 | 26.457 |
| F5b | combined claim, A and B in one instruction | 30 | 26.457 |
| F7b | Compound30 settlement (insolvent) | 32 | 41.456 |
| F10 | expiry path on a second close | 32-39 | 41.456 |

## 5. Solvency arithmetic (model; lamports unless noted)

Round 1 (clock zero is A1's open at offset 173,040):

* Pot after the deposits: A1 Series A 50,000,000 + 96 % x 1,000,000,000 + 2 x 9,600,000 = **1,029,200,000**;
  Front-End Load taken to management 40,800,000.
* F3, A1 withdraws at elapsed 596,160 s (6.9 d, inside the 12 % tier, `< 7 d`): owed 759,000,000 < pot, so
  solvent. Toll 12 % = 91,080,000, net 667,920,000, pot left 270,200,000. Ticket 1: Backer half 45,540,000
  (senior 15,939,000 + A pool 29,601,000, all to A1 because it is the only open row and no B row exists),
  seed 45,540,000. A1 has 16,460,000 of its 62,000,000 entitlement left. F5a claims 45,540,000.
* Strict insolvency threshold: A1's owed minus what F3 paid first exceeds the pot at elapsed **808,390 s =
  9.3564 d** (day 11.359). The plan closes at elapsed 816,480 s (9.45 d, 7.5 % tier): owed 280,500,000
  against pot 270,200,000, **insolvent by 10,300,000**. The payout is pot-capped: gross_paid 270,200,000,
  toll 20,265,000, net 249,935,000, pot 0. Ticket 2: A1 credited 10,132,500 (remaining 6,327,500), seed
  10,132,500.
* A1 ends round 1 with net payouts 917,855,000 against a deposit of 1,000,000,000: a loser of 82,145,000 with
  Series A history, hence a fallback candidate only. P4 and P5 (history-free, loss 10,000,000 each) are the
  preferred set, so the draw selects between them; the winner receives a Series B entitlement of
  floor(10,000,000 x 11,600 / 10,000) = **11,600,000**. Seed carried into round 2: **55,672,500**.

Round 2 (clock zero is W's open at offset 989,700):

* Pot at the Compound15 settlement: 55,672,500 + 96 % x (600,000,000 + 200,000,000 + 100,000,000 +
  20,000,000) + Series A 2 x 50,000,000 = **1,038,872,500**. Owed to P1: floor(200,000,000 x 4.473565759...)
  = **894,713,151**, solvent with 144,159,349 spare (16.1 %). Jackpot Fee 9 % = 80,524,183, net
  814,188,968 (P1 turns 0.2 SOL into 0.814 SOL). Ticket 3: A1's senior slice (35 % of 40,262,092 = 14.1 M)
  exceeds its 6,327,500 remaining, so A1 is credited exactly 6,327,500 and **reaches 124 %** (the overshoot
  goes to seed); A2 and the Winner's A row get 9,058,970 each; the Winner's B row gets 8,052,418.
* Pot at the Compound30 settlement: 144,159,349. Owed to W: floor(600,000,000 x 12.267678469...) =
  **7,360,607,081**, 51 times the pot, so strictly insolvent by construction. Paid: gross 144,159,349, toll
  13 % = 18,740,715, net 125,418,634 (W loses 474,581,366). The pot is 0 and the close starts.
* Total Front-End Load over the run: 4 % x 1.94 SOL = 77,600,000.

Why the amounts are what they are:

| constraint | consequence for the amounts |
| --- | --- |
| close 1 must be strictly insolvent early | the closing position is the largest allowed deposit (1.0 SOL, packet cap) and everything else in the round is a minimum (Series A 0.05, candidates 0.01 x 2), so the pot is only 0.0692 SOL above 96 % of the position |
| F6 at the Compound15 toll | A1 must have at most 14.1 M left when ticket 3 starts, i.e. at least 47.9 M credited in round 1: the early solvent F3 payout (12 % tier) supplies 45.5 M, the closing toll 10.1 M |
| C15 solvent, C30 insolvent | 4.4736 x 0.2 SOL must fit the round 2 pot (needs the 0.6 SOL Compound30 and the 0.12 SOL of Simple positions); 12.2677 x 0.6 SOL is 51 x the remaining pot |
| combined claim | the Winner must hold A and B, so it funds Series A before ticket 3 |
| close 2 candidates | W (loss 474.6 M), P2 and P3 are history-free losers: preferred set of 3; P1 won (net 814 M > 200 M deposit) |

## 6. Findings: what the literal Seq-B does not do, and the smallest change

None of the 15 flows is unreachable under production economics and the default terms. Four things in the
packet's description (section 6.4/6.5) do not hold as written, and one plan line is a corollary:

1. **F4 (partial payout) and F8 (closing) are the same payout.** `payout` caps `paid_gross` at the pot and
   `insolvent = gross_owed > pot` is exactly the close trigger, so every pot-capped payout closes the round.
   The plan realizes F4 twice (steps 13 and 32). A "partial payout that does not close" cannot exist; the
   solvent partial claims of a Simple position (F3) are the only non-closing partial claims.
2. **The 10.73-day close is the lone-depositor bound.** It assumes a pot of 0.96 x the position. Round 1 also
   needs Series A (minimum 0.05 SOL, added to the pot in full) and candidates (minimum 0.01 SOL each, 96 %
   added), so the pot is 1.0292 x the position and the strict insolvency comes at 9.356 days after the open
   (day 11.359), not 8.727. The schedule moves by that 0.63 days, plus 0.09 days of deliberate margin before
   the close and a few minutes of step spacing: 41.46, not 40.74. Smallest change
   that gets back toward the bound: a larger dominant deposit (the pot overhead is fixed, so 2.0 SOL would
   close at day 11.04 and the limit is 8.727 + 2 = 10.73 only for an unbounded deposit); this needs a budget
   decision against the 1.0 SOL per-deposit cap and is not needed for reachability. Scale-down is cheap in
   days: 0.5 SOL closes 0.63 days later, 0.25 SOL about 1.9 days later.
3. **F5b needs a wallet that holds both a Series A and a Series B row, and a promoted candidate never does.**
   The preferred set contains only owners with no A or B history, so the winner holds only B. As literally
   written (P1 dominant, P4 and P5 history-free) "A+B in one instruction after F9" is unreachable. Smallest
   change: the promoted wallet funds Series A (at least 0.05 SOL) in round 2 before the toll it will claim
   from (step 26). The plan additionally makes the dominant depositor a Backer (A1), which keeps the preferred
   set exactly {P4, P5} (two equal losses, so the Winner's budget is the same whichever it is); the literal
   P1-dominant variant also works with step 26, but then the draw has three unequal candidates.
4. **F6 at the Compound15 toll depends on F3.** With the minimum Series A (entitlement 62,000,000) A1 is capped by
   the first big toll only if it has little entitlement left. Without the early solvent payout the round 1
   closing toll alone credits A1 38.6 M (23.4 M left), and the 0.2 SOL Compound15 toll cannot cap it; it would
   need a Compound15 of at least 0.332 SOL and a pot above 1.486 SOL. F3 at 6.9 days (12 % tier, before the
   7-day boundary) is therefore part of the F6 plan, not an independent flow.
5. **F10 differs from the packet's C2 window in one stage code.** The plan builds and requests close 2's draw and
   never fulfils it, so the expiry records stage `requested-unfulfilled` (5). The C2 watchdog window never
   builds candidates or requests, so there the stage is `candidates-incomplete` (1). Both end in
   `PromotionSkipped(Deadline)` and a finalize. To mirror the window exactly, drop steps 34-36; expiry at the
   deadline is refused first (step 37, `now > deadline` is strict) in both.

## 7. Per-wallet SOL budget (devnet-priced) and the packet's buckets

Rent is priced at devnet's 5,080 lamports x (bytes + 128): User 1,076,960, UserRound 1,102,360, Position
1,457,960, Backer 1,397,000 (the LiteSVM run pays the default 6,960-per-byte schedule, about 37 % more; the
test prints both columns). Fees are 5,000 lamports per transaction signature. "Need" is deposits + rent
+ fees; the receipts reduce the net.

| wallet | funding sent | deposits / Series A | devnet rent | fees | need | receipts | net spend |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | 1.100 | 1.000 + 0.050 | 0.005034 (User, Backer, UserRound, Position) | 0.000030 (6 tx) | 1.055064 | 0.979855 (F3 net 0.66792, closing net 0.249935, claims 0.04554 + 0.01646) | 0.075209 |
| A2 | 0.060 | 0.050 | 0.002474 (User, Backer) | 0.000005 | 0.052479 | 0 (9.06 M unclaimed credit) | 0.052479 |
| P1 | 0.210 | 0.200 | 0.003637 | 0.000010 | 0.203647 | 0.814189 | -0.610542 |
| P2 | 0.110 | 0.100 | 0.003637 | 0.000005 | 0.103642 | 0 | 0.103642 |
| P3 | 0.030 | 0.020 | 0.003637 | 0.000005 | 0.023642 | 0 | 0.023642 |
| P4 / P5 (not promoted) | 0.080 | 0.010 | 0.003637 | 0.000005 | 0.013642 | 0 | 0.013642 |
| P4 / P5 (Winner) | 0.080 | 0.010 + 0.050 | 0.005034 | 0.000015 | 0.065049 | 0.017111 (A 9.06 M + B 8.05 M) | 0.047938 |
| W | 0.630 | 0.600 | 0.003637 | 0.000010 | 0.603647 | 0.125419 | 0.478228 |
| **8 test wallets** | | **1.94 + 0.15 = 2.09** | | | **2.121** (with the Winner counted once) | | |
| S | 0.550 | 0.500 operations | 0 | 0.000005 | 0.500005 | 0 (stays in operations) | 0.500005 |
| K | 0.050 | 0 | 0 | about 0.00022 (43 tx, one with an extra signature) | 0.00022 | 0 | 0.00022 |

Fit to the packet's buckets (4b/4c): game deposits 1.94 SOL against 6.0, Series A 0.15 against 0.5, largest
deposit 1.0 against 1.0, largest Series A 0.05 against 0.25, every funding transfer at most 1.1 SOL against the 2 SOL
cap, test wallets 2.121 SOL of need against the 7.0 SOL bucket (4.9 SOL of slack, so the plan could scale up the
position sizes in rounds 2 if a larger visible effect is wanted without touching a cap).
Operations (bucket S, 1.0 SOL): the 0.5 SOL covers the permanent pages (Registry 2,280,920 + Snapshot
3,337,560 + two Participant pages 2 x 2,326,640 + two CloseRecords 2 x 3,987,800 = 18,247,360) and two close
reserves (13.2 M each at devnet rent for at most 8 participants); the 4 candidate pages (about 2.33 M each) are
returned by the worker's cleanup, the unfulfilled close 2 request rent (4,505,960) stays outstanding at the provider.
The test prints the ops books at the end. All figures are the plan's, not a replacement for the packet's caps.

## 8. What is time-bound by what

| step(s) | bound by | detail |
| --- | --- | --- |
| 1-9 (day 2.0) | **governance** | 172,800 s after the second G approval: `initialize` and ORAO `register` execute together; nothing can precede it |
| 10-12 (day 8.9) | market clock, free choice | any time before the 7-day tier boundary and before the strict-insolvency threshold; day 8.9 gives the 12 % tier |
| 13 (day 11.453) | **market clock (insolvency)** | owed has to exceed the pot: not before day 11.359; no oracle involved |
| 14-16 (day 11.453) | immediate | closing ticket, candidate pages, candidate build (permissionless, no oracle) |
| 17-20 (day 11.454-11.455) | **oracle** | the request is only accepted while `now <= close_started_at + 600` and the fulfilled result is captured only inside the same window (a later capture is ignored); `apply_promotion` may come later. The plan spends 165 s in total because LiteSVM has no latency; devnet's real fulfilment time is what this step measures |
| 21 (day 11.455) | immediate | the composed finalize/reopen needs only a terminal promotion and a committed closing ticket |
| 22-27 | immediate | round 2 deposits and the second Series A funding |
| 28-31 (day 26.456) | **maturity** | Compound15 settles 1,296,000 s after its own open (the position pins `maturity_at` at open) |
| 32 (day 41.456) | **maturity** | Compound30 settles 2,592,000 s after W's open (the composed transaction); the insolvency is by construction |
| 33-36 | immediate, inside 600 s | closing ticket, candidates, request |
| 37-39 | **deadline** | `expire_promotion` needs `now > close_started_at + 600`: refused ON the deadline, accepted one second later, finalize 10 s later |

Everything else is permissionless and immediate. The only wall-clock risks on devnet are the 2-day governance
wait, the 30-day Compound30 maturity and the real ORAO fulfilment time against the 600 s deadline.

## 9. Assumptions and limits

* LiteSVM advances the slot by one per clock warp; request, fulfil and capture slots are therefore not cluster
  figures. The recorded "latency" is the plan's 15 s spacing, not an observation. The real ORAO devnet
  fulfilment (four fulfil authorities, 9,000-slot callback deadline) is not exercised; the synthetic authority
  signs `client || protocol_seed` at the step the plan names.
* Squads is the test-only stand-in; G's 172,800 s delay is the plan's first offset, not a transaction. The
  Client is registered with the G vault as owner by the harness' `provider_setup`; `initialize` is the genesis
  fixture with the registry, snapshot and participant pages removed so that the plan provisions them with real
  `provision_page(Free)` transactions (step 2-4). `Pool.cluster_domain` is the devnet domain
  `musical-chairs-devnet-domain-v1!`.
* `ConfigVersion` v1 as built by `ConfigVersion::default_v1()` with `public_series_a_enabled = true` and the
  ORAO tuple of the harness (fee 300,000 = the live devnet fee, headroom 2,000 bps, request sizes 759/147).
  The packet's 2,500 bps headroom would change the reserve by 15,000 lamports per close.
* The closing payout's owner contribution is 0 (the sponsor funds the whole reserve); every wallet pays its own
  fee and receives its own payouts (destination = signer).
* `observe_request_fulfilled`, provider returns and `deliver_owner_refund` are not in the M14 list and are not
  run; the ops books at the end therefore still show the close 1 request rent outstanding.
* Not simulated: the governance reduction of G's time lock (F11), the P pause rehearsal (F12), the keeper worker
  and watchdog themselves, and the ICP legs.

## 10. Running it

```text
cd solana/tests/financial-runtime
MC_ORAO_PROFILE=devnet MC_M14_FLOW_REPORT=/tmp/m14-flow-report.json \
  cargo +1.92.0 test --locked --test m14_flow_plan -- --ignored --nocapture
cargo +1.92.0 test --locked --test m14_flow_plan          # the two pure tests (plan shape, solvency arithmetic)
```

(plus `MC_FINANCIAL_SBF` / `MC_STANDIN_SBF` when the artifacts are not at their default paths.) The run
prints, in this order: every step (offset, day, actor, transactions, fees, slot, amount, flows), the first day
of each flow, the round and close ids, the oracle timeline, the per-step solvency lines with the model-versus-chain
comparison, the wallet ledger (LiteSVM and devnet-priced rent), the program books and the duration.
Hard assertions: both identities and the per-wallet equation after every step; every payout equals the economics
oracle on the chain's own pre-state; the three payouts that the pure arithmetic predicts (F3, close 1, C15)
equal it exactly; the closing payouts are strictly insolvent and the others solvent; the candidate pages hold the
independently derived sets; the winner is P4 or P5 with a 116 % B row; A1 reaches 124 % in ticket 3 and leaves
the registry; the combined claim has both parts; expiry on the deadline is refused fee-only and one second later
succeeds with stage 5; both rounds end finalized. Soft (printed, not asserted): the model's pot and seed against
the chain's.
