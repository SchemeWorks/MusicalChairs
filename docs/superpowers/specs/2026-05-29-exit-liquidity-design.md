# Exit Liquidity — PP-Sink Skill Game with Clout Leaderboard

**Date:** 2026-05-29
**Status:** Design only. Precedes architecture + implementation plan. No code written yet.
**Scope:** A new PP sink — a multi-stage, judgment-based risk game. Players burn PP to play; the only reward is leaderboard clout. No PP or ICP payout.
**Canister(s):** Likely `shenanigans` (it already owns PP burning, leaderboards `getTopPpBurners`/`getTopSpellCasters`, and golden-name state). Confirm in the plan — a dedicated sibling canister is the alternative.
**Deploy:** N/A yet. Standing policy: never deploy backend without explicit permission.

## Concept

Exit Liquidity is a game about being exit liquidity. You commit PP, ride a position through a sequence of rotations, and try to take your distributions before the rotation takes you. The only prize is a rank on the cap table. You are paying — in burned PP — for the privilege of finding out whether you can time the top. You cannot. Some of you will rank highly anyway.

The name is the joke and the thesis: don't be the one still holding when the music stops.

## Design rationale (why this shape)

Three constraints drove every decision; recording them so the implementation doesn't quietly violate one:

1. **On-brand by construction, not by paint.** A bolt-on arcade game (Pac-Man, Breakout) would be the one tonally-off thing in the product. Everything in Musical Chairs *is* the satire. So the game's mechanics had to *be* the Ponzi, and chasing its high score had to *be* the investor delusion the brand mocks.
2. **The skill has to be real.** The only prize is clout, and a leaderboard topped by luck generates complaints, not clout. A naive cash-out-before-it-busts game is 100% luck. Skill is therefore load-bearing, not optional.
3. **No dominant strategy.** The moment a player can *compute* the optimal play from numbers on screen, skill collapses into memorization. Skill must come from judgment under ambiguity.

The resolved identity (chosen over Perception-only and Execution): **Judgment** — decisions under ambiguity. The richest skill ceiling, the least bottable, the most on-brand (you are an analyst who thinks he can time the top), and — usefully — latency-tolerant on a canister.

## v1 Scope

- Burn PP to start a run.
- A run is a **multi-stage ride** (sequence of discrete rotations).
- At each stage: **Take Distribution (bank a portion) / Let It Ride / Exit (bank all, end run)**.
- **Banking locks the banked portion safe from the rotation.** A rotation forfeits only the portion still riding.
- A silent **volatility-tell** read signal so each stage decision is a *read*, not a solve.
- Per-Round leaderboard (the cap table), ranked on a skill-reflecting average (below).
- An off-rank "biggest single run" vanity badge.

## v1 Out of scope (parked — YAGNI, stack later once the core proves fun)

- **Regime tags** — per-run learnable "market regime" risk profiles. Cheap to add later; not needed to make Judgment the identity.
- **Hedge lever** — a mid-run toggle trading growth for lower rotation risk. Stacks cleanly in v1.1.
- **Audio.** The music-stops framing is the spiritual core, but autoplay is muted-by-default everywhere and forced sound reads as hostile. Audio, if added, is opt-in flavor that *rewards* players who enable it — never the load-bearing signal. The silent volatility tell carries v1.
- **Real-money / payout variants.** v1 is clout-only. A paid arcade is a separate design with a different regulatory/UX surface.
- **Provably-fair commit-reveal.** See Architecture — clout-only justifies the simpler canister-authoritative model. Commit-reveal is the upgrade path if players ever demand verifiable fairness.

## Core loop

```
Idle
  └─[burn PP buy-in]→ Stage 1
        ├─ Take Distribution (bank fraction, riding stack continues) → next Stage
        ├─ Let It Ride (whole stack continues)                       → next Stage
        └─ Exit (bank everything)                                    → Result
   at each Stage transition the canister rolls the rotation:
        ├─ survives → Stage N+1 (multiplier steps up)
        └─ ROTATION → riding stack forfeited; Result = total banked so far
```

- A run = one paid buy-in = one multi-stage sequence ending in **Exit** (clean) or **Rotation** (caught holding).
- Decisions are **discrete and deliberate** — chosen per stage, not on a millisecond clock. Presentation can animate the multiplier ticking up for drama, but the authoritative decision points are turn-based.
- *Starting value: 3–5 stages per run. Tune in the balance pass. Test: do players reach a genuine "bank vs. ride" tension by stage 2–3, rather than auto-piloting?*

## The banking rule (load-bearing)

This is the single rule that makes "bank a portion" a meaningful decision instead of decoration:

> **Banking locks that portion safe from the rotation. A rotation forfeits only the portion still riding. A run's score = everything banked (including a clean final Exit).**

Consequences:
- A disciplined player who banks steadily rarely scores zero.
- A greedy player who never banks and rides for the moon gets **rotated to 0×** often.
- Because the leaderboard averages runs (below), greed tanks the average. **Skill = the banking schedule, read off a signal you cannot compute.**

No degenerate dominant strategy:
- **Turtle** (bank everything at stage 1): caps low, ≈ the stage-1 multiplier, never rotates → mediocre average.
- **Moon** (never bank, always ride): frequent 0× runs → volatile, low average.
- **Win** = the judged middle, and the judgment is real because the rotation odds are read, not displayed.

## The read signal — volatility tells (silent)

- The run is presented as a live position/chart. As the **hidden** rotation hazard rises, the chart visibly destabilizes — choppier wicks, jitter, a stall before the step-up.
- The player reads the *look* and judges proximity to a rotation. **The actual hazard percentage is never shown.**
- The tell **correlates with** but does **not determine** the rotation. Reading it well improves your odds; it can never make a ride safe. Tension is preserved.

## Scoring & leaderboard

- **Rank = your best 25-consecutive-run window this Round** (mean run-score over the window). *N = 25 is a starting value; test: does a measurably better player land a higher window-average in 8/10 Rounds? If variance still dominates, raise N.*
- **Consecutive window** (not lifetime average, not rolling-last-N): a hot streak defines you, a cold streak is recoverable, and you keep playing to beat your best window — good for the sink and for motivation.
- **Qualifying minimum:** must complete ≥ N runs this Round to appear on the cap table at all (no sniping with 2 lucky runs).
- **Busts count.** A no-bank rotation is a 0× run in the window. A bust is not excluded — excluding busts would reward mooning and collapse the game back to luck.
- **Off-rank vanity badge:** "biggest single run" (largest total banked in one run, ever). Pure flex, does **not** affect rank — this restores the dopamine of a big number without making the board luck-based.

### Illustrative scoring (numbers are illustrative pending the balance pass)

| Style | Behavior | Run scores | ~Window avg |
|-------|----------|-----------|-------------|
| Turtle | bank all at stage 1 every run | ~1.2× flat | ~1.2× |
| Moon | never bank, ride to last stage | mostly 0×, rare 6× | ~1.8× (lumpy) |
| Disciplined | bank ~half/stage, exit on a bad tell | cluster 2–3.5×, rare 0× | ~2.8× |

Disciplined wins, with no degenerate alternative. The curve must be tuned so this ordering holds — that is the balance pass's acceptance test.

### Bounded-score property (keeps the board honest)

Because busts count as 0× and the EV-optimal line still carries real rotation risk, **every player's achievable average converges to the EV of optimal play** — no strategy produces a runaway score. A skilled human approaches the ceiling; a bot reaches it more consistently but cannot exceed it, and every run it grinds burns PP. The sink is the anti-bot tax; the bounded ceiling is the backstop.

## Clout integration (cap table / golden name)

- The Exit Liquidity board is presented as part of **the cap table** (Hall of Fame family), reusing existing prestige rather than standing up an isolated, clout-less leaderboard.
- **Proposed default (flagged for review):** the top of the board is auto-gilded via the existing golden-name path. This interacts with `goldenName` currently being a *paid* spell (ICP cost) — auto-gilding the #1 may undercut or conflict with that. Decide one of: (a) auto-gild #1 with a distinct "Exit Liquidity champion" styling separate from spell-cast gold, (b) award a separate badge and leave gold to the paid spell, (c) something else. **Needs your call.**

## Architecture direction (resolved in the plan, sketched here)

- **Canister-authoritative randomness.** The rotation is rolled server-side per stage (`Random.blob()`-style, consistent with the existing spell-roll trust model). The client never sees a future roll. This is sufficient *because the prize is clout, not money* — same trust model players already accept for spells.
- **Turn-based round-trips, not real-time.** Each stage decision is a deliberate call → canister resolves → returns outcome + next state. Latency (100–300 ms) is therefore noise, not signal. The Judgment/multi-stage shape is what buys this; a continuous real-time climb would have been latency-fragile.
- **The client displays only a fuzzed volatility proxy.** The true hazard and roll stay server-side; the proxy is derived to correlate with hazard without revealing the roll.
- **Upgrade path:** if verifiable fairness is ever demanded, move to commit-reveal (commit a hashed seed, reveal post-run). Out of scope for v1.

## The inviolable balance law

**Never display the actual rotation probability.** Every skill mechanic in this design works *only* because the risk is read, not computed. The instant the true hazard is on screen, the optimal play is solvable, becomes a dominant strategy, and skill dies. This constraint outranks UI clarity requests.

## Anti-abuse

- **Bots:** mitigated by the bounded-score property + the PP-per-run tax. Residual: bots are slightly more *consistent* at the very top. Acceptable for clout-only; if it bites, add light input friction (decision must arrive in a human-plausible window; randomize tell timing so it can't be trivially pattern-matched). Not in v1 unless observed.
- **PP-richness dominance:** the average-with-qualifying-minimum scoring (not cumulative) prevents "whoever burns most PP wins." Volume helps you *find* a better window but cannot *buy* a higher average.
- **Sybil / multi-accounting:** standard protocol-level concern; inherits whatever the cap table already does. Note, don't re-solve here.
- **Turtle / moon degeneracy:** prevented by the banking rule + busts-count scoring (see above). Validate in the balance pass.

## Deferred to the balance pass (with `game-balance-economy-tuning` + real economy data)

These need real PP earn-rate numbers, which this doc does not have. Do not invent them in implementation:

- **PP buy-in cost per run** — must bite against actual PP earn rates: a meaningful sink, but cheap enough that reaching the 25-run qualifying minimum is achievable. Too cheap → infinite grind; too dear → empty board.
- **Multiplier step-up curve** and **per-stage rotation-hazard curve** — must produce the Disciplined > Moon > Turtle ordering above.
- **Stage count** (3–5 start) and **N** (25 start).
- **Banking granularity** — discrete fractions (e.g. bank 25/50/100%) vs. a slider. Default discrete for Clarity/Response; confirm.

## Open questions deferred

- **Golden-name interaction** — see Clout integration. Needs your call before implementation.
- **Canister home** — `shenanigans` vs. new sibling. Decide in the plan based on coupling to PP-burn and leaderboard state.
- **Round alignment** — assume the Exit Liquidity season == the protocol Round. Confirm.
- **Sink-fatigue at the top** — a player satisfied with their rank may stop burning. Per-Round reset re-opens the grind each Round; monitor whether that is enough recurring sink or whether v1.1 needs a decay/defense mechanic.

## Playtest scenarios (validate before claiming done)

1. **New player:** can they infer "bank before the rotation" from one run without a tutorial?
2. **Skill test:** do better players post higher window-averages reliably across Rounds? (Core validation of the whole premise.)
3. **Abuse test:** do Turtle and Moon both lose to Disciplined under the tuned curve?
4. **Readability:** can a spectator watching the cap table understand why #1 is #1? (Average is less legible than a high score — the vanity badge and a clear "your best window" display are the mitigations.)
5. **Latency test:** do stage decisions feel deliberate and fair under 100–300 ms round-trips?
