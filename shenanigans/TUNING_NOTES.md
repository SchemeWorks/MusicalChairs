# Shenanigans — Tuning Notes & Idea Backlog

Living scratch doc for game-balance work on the spell roster. Two purposes:

1. **Per-spell tuning log** — when Charles edits config in admin, record the new numbers, my reaction, suggested counter-edit, and reasoning.
2. **Spell-extension idea backlog** — punch list of mechanic ideas we've floated but not built. Pull from here when we want to deepen a spell.

Status legend: 🟢 implemented · 🟡 partially implemented (config-only) · 🔴 idea only · ⏸ deferred

---

## Roster snapshot (as of 2026-05-21)

| ID | Name | Cost | S/F/B | Cooldown | Castmit | Notes |
|----|------|------|-------|----------|---------|-------|
| 0 | MEV Attack | 10 | 60/25/15 | 2h | 0 | 2–8% target, cap 250 |
| 1 | Contagion | 20 | 40/40/20 | 12h | 1 | 1–3% everyone, cap 60 |
| 2 | Cease & Desist | 10 | 90/5/5 | 24h | 0 | rename 7d |
| 3 | Trailing Commission | 15 | 70/20/10 | 24h | 0 | 5% siphon 7d, cap 1000 |
| 4 | Crossline Poach | 15 | 30/60/10 | 8h | 1 | poach L3 downline |
| 5 | Poison Pill | 5 | 100/0/0 | 6h | 2 | shield charge |
| 6 | Yield Boost | 10 | 100/0/0 | 24h | 1 | +5–15% PP rest of round |
| 7 | Bridge Exploit | 15 | 20/50/30 | 8h | 0 | 25–50% target, cap 800 |
| 8 | Wealth Tax | 20 | 50/30/20 | 12h | 0 | 20% top 3 whales, cap 300 |
| 9 | Override Bonus | 10 | 100/0/0 | 24h | 1 | 1.3x downline rest of round |
| 10 | Whitelisted | 5 | 100/0/0 | 24h | 1 | gold name 24h |

Reminder: `castLimit` is **not enforced anywhere in the backend** — cooldown is the only real recast gate. Treat the column as advisory until/unless we wire enforcement.

---

## #10 — Whitelisted

### Current state
- 5 PP, 100% success, 24h cooldown, 24h gold-name buff.
- Cannot backfire (and backfire handler is a no-op stub even if you give it positive odds — caster pays cost, no effect lands).
- Config `effectValues = [24.0, 168.0]` is dead — code always uses `oneDayNs`.

### Idea backlog
- 🔴 **Blacklisted** (backfire variant) — red/black name + "RUG SUSPECT" or "SEC INVESTIGATION" chip on the leaderboard for 24h. Cheapest to implement: parallel `blacklistedUntil` map, leaderboard color-keys off both. On-brand because it riffs on actual sketchy crypto-VC outcomes.
- 🔴 **Reverse Halo** (backfire variant) — your random target gets the gold name for 24h instead of you. Requires switching cast to targeted (currently self-cast). Pure schadenfreude: you paid PP to gild your rival.
- 🔴 **Down Round** (backfire variant) — lose 25–50% of your PP. Flavor: "your cap table just got crammed down." Symmetric with Bridge Exploit's success effect, which is nice.
- 🔴 **Disclosure Penalty** (backfire variant) — every other player sees your real principal/holdings in a "leaked S-1" overlay for 24h. Reginald-tier privacy violation. Mechanically: a `disclosedUntil` map that the leaderboard/profile UI honors.
- 🔴 **24h vs 7d roll** (success variant) — the dead `effectValues = [24, 168]` becomes live: weighted roll between two buff durations. E.g., 80% chance 24h, 20% chance 7d. The 7d outcome is a "Series A lead investor" moment.
- 🔴 **Real cast limit per round** — wire up `castLimit` enforcement so Whitelisted truly is one-shot per round, not "recastable every 24h forever for 5 PP/day."

### Open balance questions
- At 5 PP / 24h cooldown with no backfire risk, this is the cheapest persistent cosmetic in the game. Is it underpriced for "stay gold continuously for 5 PP/day"? Or is that the point — cosmetic clout should be cheap and visible so it's used?
- If we add Blacklisted as a backfire, the spell stops being pure-upside. That changes the strategic identity from "cosmetic flex" to "cosmetic gamble." Probably good — every other 100%-success spell is a buff to yourself or downline; Whitelisted being the one cosmetic with risk creates an interesting outlier.

---

## Per-spell tuning log

(Charles edits config in admin → screenshots stats → I react here.)

### #10 — Whitelisted — edit on 2026-05-21

**New stats:**
- Cost: 420 / 42 / 69 PP (success / fail / backfire) — was flat 5 PP
- Odds: 35 / 50 / 15 — was 100 / 0 / 0
- Cooldown: 24h (unchanged), Duration: 24h, castLimit: 1, effectValues: [24, 168]

**Identity shift:** "cheap always-on flex" → "high-roller gamble." Good move — fits the VC/MLM voice better, makes gold name actually coveted. Meme numbers (420/42/69) are pure flavor and land.

**Math:**
- E[cost per cast] = 0.35×420 + 0.50×42 + 0.15×69 = **178 PP/cast**
- E[casts per gild] = 1/0.35 ≈ **2.86**
- E[PP per gilded day] ≈ **510 PP**
- ~50× more expensive per effect than Trailing Commission. Whale-only luxury at this price (intentional, probably fine).

**Concerns:**
1. **Backfire ($69) > failure ($42) but mechanically identical.** The `#goldenName` backfire handler is a no-op. So 15% of casts you pay a 64% premium for a label change ("BACKFIRED" vs "FAIL"). Fix by either (a) wiring a real backfire effect or (b) dropping backfire cost ≤ failure cost.
2. **Cooldown only fires on success.** With 65% non-success rate and uncapped retries, the 24h lockout does less work than it looks like — the loop is "spam until success, then wait 24h, then spam again." If gold name should feel earned, consider lockout on any outcome (or finally wiring `castLimit`).
3. **Floor problem.** A new player with <2k PP can't realistically chase this. That's probably fine for a luxury cosmetic, but worth naming.

**Suggested edit (recommended): keep 420/42/69, build the Blacklisted backfire effect.**
- Parallel `blacklistedUntil` map, leaderboard renders red name + "RUG SUSPECT" chip for 24h on backfire.
- Now 69 PP buys real pain 15% of the time. Spell becomes a true status-axis gamble: clout, nothing, or anti-clout.
- Fills a roster gap — no other spell has reputational downside on the cosmetic axis.

**Alternative edit (lighter):** swap to **420/69/42** (backfire cheaper than fail). Removes the "pay more for label" trap but loses some of the 69-meme energy.

**Idea backlog adds (from this conversation):**
- 🔴 **Asymmetric outcome costs as a roster-wide pattern.** Whitelisted is the first spell with non-flat S/F/B costs. Worth exploring elsewhere — e.g., MEV Attack could charge much more on success (you only pay big when you actually drain) to feel more transactional.
- 🔴 **Meme-number flavor tradition.** Cosmetic/chaos spells get joke numbers as a convention. Whitelisted = 420/42/69. Cease & Desist could get 1337 or 8008 somewhere. Builds in-game vocabulary.
- 🔴 **Name-tier hierarchy.** If gold name is now expensive/rare, sub-tier it: bronze name (cheap, common), gold name (current Whitelisted, ~500 PP/day), platinum name (absurd threshold — e.g., 3 consecutive successful casts or 5000+ PP burn). Each tier unlocks a more obnoxious leaderboard treatment.
- 🔴 **"Due diligence fees" pattern.** A spell where failure costs *more* than success ("we charge you whether the deal closes or not"). On-brand satire of VC behavior. Could be a fresh chaos spell rather than a Whitelisted variant.
- 🔴 **Honest backfire mode (no extra mechanic).** If we don't build Blacklisted etc., explicitly accept that backfire = "expensive failure flavor." UI should make this clear so the 15% feels like a tax on style, not a stealth penalty.

---

### #9 — Override Bonus — edit on 2026-05-21

**New stats:**
- Cost: 30 / 20 / 100 PP (success / fail / backfire) — was flat 10 PP
- Odds: 25 / 65 / 10 — was 100 / 0 / 0
- Cooldown: 24h, Duration: 0 (rest-of-round effect), castLimit: 1, effectValues: [1.3]

**Identity shift:** "guaranteed downline buff" → "MLM kingpin gamble." Thematically perfect — VC/MLM downline payouts *should* feel like gambling. Big downline + lucky roll = compounding payday. Small fish gambling = burning capital.

**Accidental class divider (probably good):** With Yield Boost still at 10 PP / 100% for self-buff, the roster now naturally splits:
- Yield Boost = beginner self-care (cheap, reliable, +5–15% self)
- Override Bonus = recruiter flex (expensive gamble, 1.3x downline)
Newcomers self-buff, MLM kingpins gamble for the big payday. Tier hierarchy without explicit gating. **Worth keeping this asymmetry intentional.**

**Math:**
- E[cost per cast] = 0.25×30 + 0.65×20 + 0.10×100 = **30.5 PP/cast**
- E[casts per success] = 1/0.25 = **4**
- E[PP per successful boost] ≈ **122 PP**
- ROI scales with downline size × round-time-remaining. Timing strategy now matters: cast early = big ROI, cast late = guaranteed loss.

**Concerns:**
1. **Backfire is a no-op.** [main.mo:2383](main.mo:2383) — `#downlineBoost` backfire returns null. 10% of casts you pay 100 PP for the label only. **5× premium over failure for the same mechanic.** Worst pathology of the screenshots so far.
2. **Cooldown only fires on success** (same as Whitelisted) — spam-until-success loop, 24h lockout doesn't gate retries.
3. **0-downline players paying for a no-op success.** Spell still costs 30 PP on "success" if you have no referrals — the 1.3x just multiplies nothing. Worth gating on `referralChain` non-empty so newcomers don't accidentally torch PP.

**Suggested edit (recommended): keep 30/20/100, build Override Reversal as the backfire effect.**
- 1.3x downline-PP-kickup retargets to *your upline* for the rest of the round. Reuses the `cascadeBoosts` map — just write the entry against `referralChain[caster]` instead of `caster`.
- Pure MLM satire: you did the work, your sponsor ate. 100 PP cost is now justified by genuine consequence (could be the biggest mint-period loss in the game for an active recruiter).

**Alternative edit (lighter):** drop backfire cost to **30/20/30** or **30/20/40**. Removes the "pay 5× for vibes" trap.

**Idea backlog adds:**
- 🔴 **Override Reversal as backfire mechanic** (the recommended ship for this spell — see above).
- 🔴 **Compliance Audit:** downline frozen for 24h on backfire — no PP flows from them. "Under SEC review." Different consequence shape than Reversal (denial vs. redirection); could be a different spell entirely.
- 🔴 **Whistleblower:** on backfire, one random downline member ejects from your tree (becomes new root or reparented). Permanent, cross-round. Highest stakes. Best as a *separate* spell, not Override Bonus's backfire — too heavy to be a 10% outcome on a buff spell.
- 🔴 **Round-timer awareness in UI.** If rest-of-round effects now matter strategically, the cast UI should surface "X minutes left in round" so players can make informed timing calls. Currently players have to know this out-of-band.
- 🔴 **Pre-cast gate: require non-empty downline for Override Bonus.** Don't let new players burn PP on a spell that mechanically can't do anything for them. Soft gate (warning toast) or hard gate (button disabled).

---

## Open balance questions (cross-cutting)

- **Are all 100%-success spells getting gamble-ified?** So far Whitelisted and Override Bonus have moved. Remaining reliable buffs: Yield Boost (10 PP / 100%), Poison Pill (5 PP / 100%). Decide before next pass: is the direction "all reliable spells become gambles" or "keep some reliable anchors for newcomers"? Yield Boost as the anchor reads coherent — Poison Pill is *defense* so its 100% feels structurally different.
- **Backfire-as-no-op is now blocking two screenshots.** This is the most-leveraged engineering investment we could make on this whole pass. Worth scheduling a Blacklisted + Override Reversal implementation sprint before tuning more spells with positive backfire odds, otherwise we keep recommending the same fix.
- **Cooldown-only-on-success is a roster-wide design choice.** Helpful for offensive spells (don't lock the attacker out for a missed swing), but it neuters cooldowns on buffs (where you'd want to gate even unsuccessful attempts). Consider per-spell config for "lock-on-any" vs "lock-on-success."

---

## 📡 Final live-state audit (2026-05-21, post-deploy)

Read straight from mainnet (`dfx canister --network ic call j56tm-oaaaa-aaaac-qf34q-cai getShenaniganConfigs --query`) after Charles' deploy of commits 460bbef + e54f383 (V6 migration). The wiring fix from PR #88 (commit 3e061ee) means **all configured values now fire live.**

| ID | Spell | Cost (S/F/B) | Odds (S/F/B) | Cooldown | Notes |
|---|---|---|---|---|---|
| 0 | MEV Attack | 10/15/25 | 60/25/15 | 4h | effectValues [7, 19, 250] — descriptions in sync |
| 1 | Contagion | 5/10/60 | 43/39/18 | 12h | effectValues [2, 5, 60] — descriptions in sync |
| 2 | Cease & Desist | 125/25/10 | 38/55/7 | 96h | clean |
| 3 | Trailing Commission | 15/15/60 | 13/79/8 | **48h** (Charles raised from 24h) | clean |
| 4 | Crossline Poach | 150/40/150 | 8/84/8 | 96h | clean |
| 5 | Poison Pill | 80/40/222 | 64/35/**1** | 72h | 🚨 silent backfire trap LIVE |
| 6 | Yield Boost | 10/15/10 | **16**/84/0 (Charles softened from 12/88/0) | 48h | clean |
| 7 | Bridge Exploit | 30/10/0 | 21/60/19 | 8h | effectValues [25, 50, 1600] — **description still says "max 800 PP"** (stale); **backfireDescription typo "A the token"** |
| 8 | Wealth Tax | 50/20/0 | 30/55/15 | 6h | effectValues [20, 900] — **description still says "max 300 PP/whale"** (stale); satirical backfire description landed |
| 9 | Override Bonus | 30/20/100 | 25/65/**10** | 24h | 🚨 silent backfire trap LIVE; `backfireDescription = null` |
| 10 | Whitelisted | 420/42/69 | 35/50/**15** | 24h | 🚨 silent backfire trap LIVE; `backfireDescription = null` |

### 🚨 Still needs admin attention before "all good"

Charles must fix in admin UI — I can't reach it from CLI:

1. **Drop silent-backfire odds to 0** (Path A from the per-spell entries):
   - Whitelisted: 35 / 50 / **15** → **35 / 65 / 0**
   - Override Bonus: 25 / 65 / **10** → **25 / 75 / 0**
   - Poison Pill: 64 / 35 / **1** → **64 / 36 / 0**
2. **Fix stale description literals** (low-priority, players just learn from play):
   - Bridge Exploit description: "max 800 PP" → "max 1600 PP" (or template with `{2}`)
   - Wealth Tax description: "max 300 PP/whale" → "max 900 PP/whale" (or template with `{1}`)
3. **Fix typo** in Bridge Exploit backfire description: "A the token" → "A token"
4. **Optionally:** add backfire descriptions for Whitelisted and Override Bonus (currently `null`, falls back to hardcoded default). Lower priority since they should be 0% backfire anyway after fix #1.

### 🟢 What Charles did beyond the doc's recommendations

Worth recording — Charles incorporated feedback during the pass:
- **Yield Boost:** softened to 16/84/0 from screenshot's 12/88/0 (per my "88% may feel punitive" note)
- **Trailing Commission:** raised cooldown to 48h from screenshot's 24h (per my "conspicuously unchanged" note)
- **Wealth Tax:** wrote a sharper backfire description than my suggested draft, riffing on the consulting-role angle
- **MEV Attack:** kept descriptions in sync with effectValues edit (good discipline that wasn't in the original spec)

---

## ✅ RESOLVED — admin odds and effectValues now wire to runtime (PR #88, 2026-05-21)

PR #88 (`fix(shenanigans): wire config effectValues end-to-end and tidy cards`) landed on main. The 11 commits behind it wire `successOdds`/`failureOdds`/`backfireOdds` and `effectValues` from config into runtime for every spell that needs it. Verified on main:
- `effectNatOr` helper at [main.mo:1527](main.mo:1527)
- Odds read from config at [main.mo:1713-1714](main.mo:1713)
- Per-spell handlers use `effectNatOr` reads (MEV Attack, Contagion, Trailing Commission, Yield Boost, Bridge Exploit, Wealth Tax, Override Bonus, Whitelisted)
- Cease & Desist duration wired through `config.duration`

**Scope explicitly left as-is** (per session brief):
- Trailing Commission backfire duration (still hardcoded `halfWeekNs`).
- castLimit enforcement (still not wired anywhere).
- RNG primitive (`Time.now() % 100` — fine for fairness, not adversarial-grade).
- Default description literals — they still hardcode "25–50%", "(max 800 PP)", etc. Templating (`{0}`, `{1}`, `{2}`) is supported but unused. Without it, admin edits to effectValues silently desync from the displayed copy.

### 🚨 Pre-deploy blocker that the wiring fix exposed

The wiring fix made admin odds *live*. Two spells now have positive backfire odds with **no-op backfire handlers**:

| Spell | Backfire odds | Backfire handler | What players see |
|---|---|---|---|
| Whitelisted (`#goldenName`) | 15% | no-op stub at [main.mo:~2386](main.mo) | Pay 69 PP, "BACKFIRED" toast, nothing happens |
| Override Bonus (`#downlineBoost`) | 10% | no-op stub at [main.mo:~2383](main.mo) | Pay 100 PP, "BACKFIRED" toast, nothing happens |

**Resolve before deploy.** Two paths:
- **Path A:** admin-edit backfire odds to 0 (Whitelisted → 35/65/0, Override Bonus → 25/75/0). 30 seconds. Loses the punishment teeth.
- **Path B:** build the Blacklisted and Override Reversal mechanics from this doc's idea backlog. Half-day session. Preserves the high-roller-gamble identity.

Recommend Path A now to ship today, Path B as polish later.

---

## 🚨 (HISTORICAL — was the active blocker before PR #88) admin odds and effectValues not wired to runtime

Discovered while answering the RNG question (2026-05-21).

**The bug:** `determineOutcomeWithMod` at [main.mo:1689-1701](main.mo:1689) hardcodes `baseSuccess` and `baseBackfireTail` per `ShenaniganType` variant. It never reads `config.successOdds` / `config.failureOdds` / `config.backfireOdds`. Likewise, every spell handler hardcodes its own caps and percentages in code, ignoring `effectValues`.

**Impact on this tuning pass:**

| Spell | Admin set | Actually rolling | Admin effectValues | Actual cap |
|---|---|---|---|---|
| Whitelisted | 35/50/15 | 100/0/0 | [24, 168] | hardcoded 24h |
| Override Bonus | 25/65/10 | 100/0/0 | [1.3] | hardcoded 1.3x |
| Wealth Tax | 30/55/15 | 50/30/20 (baseline) | [20, 900] | hardcoded 300 PP/whale |

### ⚠️ RETRACTION — the PROMPT ERRATA below was wrong

The "Correction to first-draft analysis" and "📋 PROMPT ERRATA" sections that previously sat here claimed that MEV Attack, Contagion, and Trailing Commission were already wired to `effectValues` on `main`. **They were not.** That claim came from me reading `shenanigans/main.mo` in a working directory that had been silently switched to the spawned session's branch (`fix/shenanigans-config-runtime-wiring`) — I was reading the session's own freshly-committed wiring and mistaking it for pre-existing code.

The original 🚨 BLOCKER section above (the one referenced in the spawn_task brief) is the correct picture of `main` at session-start: **all 11 spells had hardcoded odds and effectValues. None used `effectNatOr`.** `effectNatOr` itself does not exist anywhere on `main` — it was introduced by the session's commit `a7298ab fix(shenanigans): wire effectValues into MEV Attack` and used by the subsequent commits.

The spawned session pushed back on the bad ERRATA with concrete evidence (`git log --all -S "effectNatOr"`, `git show main:shenanigans/main.mo`). The session's read is correct. Keep all 11 of the session's wiring commits in place — including MEV Attack, Contagion, and Trailing Commission.

**Lesson for future sessions:** before claiming "X is already in the codebase," run `git branch --show-current` and verify the file's content against `git show main:path` rather than the working-tree state. The user's auto-memory already has [subagent_worktree_isolation](../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_subagent_worktree_isolation.md) — this is a variant of that failure mode where the *parent* session, not the subagent, was the one looking at the wrong tree.

**What admin edits DO take effect:**
- Costs (success/fail/backfire) — read at [main.mo:1815-1817](main.mo:1815)
- Cooldown
- Name, description, backfireDescription
- Background color

**Two paths to fix:**

1. **Wire config to runtime (recommended).** Replace the hardcoded `baseSuccess`/`baseBackfireTail` switches with `config.successOdds` reads and computed tails. Then wire `effectValues` into each spell handler — bigger lift, since each one hardcodes its own caps/percentages. Probably worth doing handler-by-handler as we tune each spell.
2. **Hide the dead fields in admin.** Disable odds/effectValues editing and document them as cosmetic-only. Fast unblock, but kicks the can.

**Until this is fixed:** all "this spell is now a 35% gamble" tuning is aspirational. The admin UI is silently lying about its own effects. **Block further odds/effectValues tuning until path 1 lands.**

**Also worth fixing in the same sprint:** RNG primitive (`Time.now() % 100`) is fine for fairness but trivially predictable. Not adversarial-grade. Consider upgrading to `aaaaa-aa.raw_rand()` if/when stakes get serious or PvP becomes profitable to predict.

---

### #8 — Wealth Tax — edit on 2026-05-21

**New stats (configured):**
- Cost: 50 / 20 / 0 PP (success / fail / backfire) — was flat 20 PP
- Odds: 30 / 55 / 15 — was 50 / 30 / 20 — **but odds are not wired (see blocker above); actual roll still 50/30/20**
- Cooldown: 6h (was 12h)
- effectValues: [20, 900] — **not wired; cap stays at hardcoded 300 PP/whale**
- backfireDescription: *"The mayor accepted a 'consulting role' with the top 3 firms. Turns out socialists like money too. You pay each of the top 3 whales (caps at ~49% loss)."*

**Identity shift:** "20-PP guaranteed gamble at populism" → "50-PP success / free-on-backfire high-roller redistribution attempt." Plus a satirical backfire that mirrors the success line's political register.

**What actually shipped (post-bug-filter):**
- Costs: real. Caster pays 50 on success, 20 on fail, **0 on backfire**.
- Cooldown 6h: real.
- Backfire description: real. Lands beautifully — best satirical button in the roster.

**The free-backfire is elegant.** Backfire effect (~49% compound drain to whales) is already brutal; charging an additional 20 PP cost on top would double-tap. Setting `costBackfire = 0` means "the punishment IS the drain, not an extra cast tax." Worth keeping as a pattern for any spell where the effect itself is the cost.

**Math (under current bug):**
- Real outcome split is still 50% success / 30% fail / 20% backfire.
- E[cost per cast] = 0.50×50 + 0.30×20 + 0.20×0 = **31 PP/cast**
- E[gain per cast] = 0.50×(up to 900 PP from 3 whales, hardcoded cap 300/whale) − 0.20×(~49% of balance)
- For a small fish (balance < 1500 PP): backfire is catastrophic, success cap doesn't fully fire either.
- For a whale (balance > 1500 PP): backfire is trivial (~49% of large balance, but compound-drained), success caps at 900 PP. Net EV still positive due to cap.
- 6h cooldown means up to 4 casts/day. Aggressive spell, frequent attempts.

**Math (under intended config, if bug fixed):**
- Configured 30/55/15 split makes this strictly less aggressive than the 50/30/20 baseline. Charles is *reducing* the success rate while increasing the success cost. Bigger swings: 50 PP × 30% = 15 PP per success-attempt EV cost, vs the original 20 PP × 50% = 10 PP per success-attempt EV cost. So configured intent = 50% more EV-expensive per redistribution attempt.

**Concerns:**
1. **Bug blocks the intended balance.** Until odds are wired, the only real change is cheaper cooldown + free-backfire cost. The success rate is unchanged.
2. **Cooldown 6h on a redistribution spell with 50% success** = whales can spam this defensively if they end up at top-3 (?). Wait — the spell targets top 3, caster is anyone. So it's anti-whale, not pro-whale. 6h cooldown = whales get drained 4x/day per active caster. With many casters, top 3 holders lose meaningful PP every day. Could be brutal at scale.
3. **The 300 PP per-whale cap stays hardcoded.** Charles intended to raise it to 900. The intent is significant (3x more PP transferred per success) but doesn't fire.

**Suggested edit (after bug fix):** the current configured values are reasonable for the "redistribute but make it riskier per-cast" identity. Once odds/effectValues are wired, ship as-is and observe. If whales complain about constant drain, raise cooldown back to 8-12h.

**Idea backlog adds:**
- 🔴 **Free-backfire-cost as a roster pattern.** Any spell where the backfire effect is itself painful should set `costBackfire = 0` to avoid double-tap. Worth auditing the existing roster — does MEV Attack double-tap? (10 PP cost + you pay 2-8% to target.) Probably yes. Worth reviewing.
- 🔴 **"Tax the whales but they're already lobbying" UI moment.** Now that Wealth Tax has a corruption-flavor backfire, the cast animation could lean in — show a fake "Lobbyist Hired" alert during the ~2sec resolve animation when backfire rolls. Pure flavor.
- 🔴 **Wealth Tax success: rename mayor candidates?** The success says "A socialist mayor takes office." Could randomize among 3-5 archetypes (socialist, populist, anti-corruption crusader, etc.) and the backfire line picks the matching corruption-pattern. Pure flavor variety, low impact.

---

### #7 — Bridge Exploit — edit on 2026-05-21

**New stats (configured):**
- Cost: 30 / 10 / 0 PP (success / fail / backfire) — was flat 15 PP
- Odds: 21 / 60 / 19 — was 20 / 50 / 30 — **odds not wired; runtime still rolls 20/50/30**
- effectValues: [25, 50, 1600] — was [25, 50, 800] — **not wired for `#purseCutter`; runtime cap stays at 800**
- Cooldown 8h, Duration 0 (unchanged), castLimit 0
- Backfire description: *"A the token you're holding was using the bridge."* (typo: "A the" → "A token")

**Identity shift:** "high-variance gamble" → "safer, harder-hitting whale-punisher." Less self-harm (backfire 19% vs 30%), more whiffs (failure 60% vs 50%), but successes hit twice as hard once wiring lands (1600 cap vs 800).

**What actually shipped (post-bug-filter):** new cost structure (30/10/0) + backfire description. Odds and cap doubling are aspirational until the spawned-session fix lands.

**The cap-doubling amplifies the anti-whale gradient.** Bridge Exploit was already structurally anti-whale (caster's own cap protects them from their own size on backfire). Doubling to 1600 makes punching up dramatically more profitable. Small player → whale: cap binds on success (you steal more), but cap binds harder on backfire (you lose your max which is small anyway). Whale → whale: roughly break-even.

**Free-backfire-cost is on pattern.** Same as Wealth Tax — when the effect itself is brutal (burn 25-50% of caster's stack), charging a cast cost on top is double-tap. Pattern is now: any spell where backfire has a real punishment, `costBackfire = 0`.

**Math (under intended config, post-wiring):**
- E[cost per cast] = 0.21×30 + 0.60×10 + 0.19×0 = **12.3 PP/cast**
- E[gain] vs whale (cap binds at 1600): 0.21 × 600 = **126 PP**
- E[loss] from backfire if caster is whale: 0.19 × 600 = **114 PP**
- Net EV at equal sizing: ~break-even (whale vs whale)
- Net EV punching up (you 800 PP vs target 4000 PP): success 600 - backfire 200 ≈ **+100 PP/cast**

**Math under current bug:** costs are real but odds/cap aren't. Effective spell is 20/50/30 with 800 cap — old behavior at new cost structure. The backfire-rate-cut (the safety improvement Charles wants) doesn't fire.

**Concerns:**
1. **Backfire description typo.** "A the token" → "A token". Easy fix.
2. **Description has stale "max 800 PP"** which will lie once cap-wiring lands. Two options: hardcode "1600" in the description (one-shot fix), or use the templater with `{2}` placeholder so the cap auto-pulls from effectValues. Templater syntax is already supported — none of the existing descriptions use it.
3. Otherwise: coherent design. Approve once wiring lands.

**Suggested edit:** fix typo to "A token you're holding was using the bridge." Optionally template the description: "Target loses {0}–{1}% of their PP (max {2} PP)." so it auto-updates as effectValues changes.

**Idea backlog adds:**
- 🔴 **Asymmetric caps for success vs backfire.** Currently success and backfire share `effectValues[2]` for cap. Once wiring lands, could split — e.g., success cap = 1600 but backfire cap = 800, making punching up even more attractive. Would require adding a 4th effectValues slot or a separate `effectValuesBackfire` field.
- 🔴 **"Bag Holder" wall.** A leaderboard slice showing who's been Bridge-Exploit-backfired most. Gallows humor for the people who keep stepping on rakes. Same tone as the "PP burned" leaderboard.
- 🔴 **Description templating as a discipline.** Right now every description hardcodes its numbers, so admin edits to effectValues silently desync the copy. Migrate all 11 descriptions to use `{0}`, `{1}`, etc. placeholders so the templater keeps copy and mechanic in lockstep. Probably a follow-up after the wiring fix.
- 🔴 **Bridge Exploit cooldown reset on backfire?** Right now cooldown only fires on success ([main.mo:1834-ish](main.mo) `Lockout only fires on success`). For a spell where backfire is brutal (25-50% own burn), allowing immediate retry feels punitive on tilt. Consider lockout on any-outcome for high-variance spells specifically.

---

### #6 — Yield Boost — edit on 2026-05-21 (post-wiring)

**New stats (all now firing — post PR #88):**
- Cost: 10 / 15 / 10 PP (success / fail / backfire) — was flat 10 PP. **Failure cost > success cost** is a roster-first.
- Odds: 12 / 88 / 0 — was 100 / 0 / 0. Steepest failure rate in the roster.
- Cooldown: 48h (was 24h) — effectively once-per-round
- effectValues: [5, 15] (unchanged) — boost min/max % for rest-of-round PP minting
- Backfire description: "Cannot backfire." (unchanged, correct — backfire odds = 0)

**Identity shift:** "guaranteed +5–15% self-buff" → "12% gamble at the self-buff, 88% nothing, 48h lockout on hit." Cleanest gamble shape in the roster — pure opportunity cost, no self-immolation possible.

**New satirical pattern: "due diligence fees."** Failure costs MORE than success (15 vs 10 PP). Trying-and-failing carries a retainer — the VC bills you for looking at the deck whether they invest or not. This is the first spell with this cost shape; worth recognizing as a pattern we might want to spread.

**Math:**
- E[cost per cast] = 0.12×10 + 0.88×15 + 0×10 = **14.4 PP/cast**
- E[casts per success] = 1/0.12 ≈ **8.33**
- E[PP per successful boost] ≈ **120 PP**
- Breakeven requires minting **800–2400 PP rest of round** depending on boost roll (5–15%). Casual players lose money in expectation; whales with big positions profit.

**Concerns:**
1. **88% failure rate** is the highest in the roster (Override Bonus 65%, Bridge Exploit 60%, Wealth Tax intended 55%). May feel punitive in playtest. Counterargument: no backfire means the spell never makes things *worse*, only fails to improve them. Different gamble shape than the punishment-bearing spells, which arguably justifies the steeper fail rate.
2. **Cooldown lockout fires on success only.** With 8+ expected attempts before a hit, the 48h "lockout" really triggers post-success. Could feel like "I finally got it… now I can't use it again for two days" — but that's the design: once-per-round scarcity, not multi-cast spam.
3. **Whale-favoring math.** Consistent with the rest of the gamble-ified roster, but worth naming — this is no longer the universal cheap self-care it used to be.

**Suggested edit: ship as-is.** Three reasons:
1. "Due diligence fees" pattern is a satirical win.
2. No backfire = purest gamble in roster, structurally distinct.
3. Math direction (whale-favoring) is consistent with the rest of the tuning pass.

If 88% failure produces complaints in playtest, soft-land to 15/85/0 or 18/82/0 without changing identity.

**Idea backlog adds:**
- 🔴 **"Due diligence fees" pattern as a roster-wide tool.** Yield Boost is the first instance (failure costs > success cost). Could spread to other low-stakes spells — particularly buff-class. Pure VC satire of retainer-billing.
- 🔴 **Discretized boost magnitudes.** Instead of `rollPct(105, 115)` for a smooth random boost, could pick from `[low=105, mid=110, high=115]` with weighted odds — gambler-style "did I hit the jackpot tier" excitement. Would need wiring extension but adds another variance dimension on the success path.
- 🔴 **Cooldown-on-any-outcome flag.** Right now the lockout-on-success design works fine for Yield Boost (because it's once-per-round-ish anyway) but might be wrong for spells where you'd want to gate even unsuccessful attempts. A per-spell config field like `lockoutOnFail: bool` would let admin choose.

---

### #5 — Poison Pill — edit on 2026-05-21 (post-wiring) — 🚨 SILENT BACKFIRE TRAP

**New stats (configured):**
- Cost: 80 / 40 / 222 PP (success / fail / backfire) — was flat 5 PP. **16× cost increase on success.**
- Odds: 64 / 35 / **1** — was 100 / 0 / 0. **1% backfire is a silent-no-op trap** (Poison Pill's `#magicMirror` backfire handler returns `{0, null, 0}`).
- Cooldown: 72h (was 6h) — 12× longer
- castLimit: 2, Duration: 0, effectValues: [] (unchanged)
- Backfire description: "Cannot backfire." (now a lie on 1% of casts)

**🚨 Pre-deploy blocker:** drop backfire odds to 0 (try **65/35/0**) OR build a real backfire mechanic. Recommended: 65/35/0 since "shield backfire" is structurally weird as a concept — what's the punishment, anti-shield? Worth a separate conversation if you want one.

**Identity shift (post-fix):** "guaranteed defense at 5 PP" → "expensive 65% gamble at defense for 80 PP." **This is the biggest meta change of the tuning pass.** Reliable defense is dead.

**Math (assuming 65/35/0 fix):**
- E[cost per cast] = 0.65×80 + 0.35×40 = **66 PP/cast**
- E[casts per shield] = 1/0.65 ≈ 1.54
- E[PP per shield] ≈ **102 PP** — 20× more expensive than before
- 72h cooldown post-success = one shield per ~3 days max

**Strategic implications:**
1. **Shields used to be reliable; now they're not.** Players can't pre-buy defense confidently.
2. **Self-shield only** — no reactive shielding after a hit lands. Must pre-buy.
3. **Shield economics shift:** at 102 PP per shield, only worth it against high-damage offensives (Bridge Exploit 1600 cap, Wealth Tax ~49%). Against MEV Attack (250 max) the shield costs more than the damage it blocks. Players will shield selectively, not reflexively.
4. **Roster meta tilts toward offense.** Aggressors get more bites because shields stochastically don't apply.

**Concerns:** the meta-tilt-toward-offense is a coherent design ("everything is gambling now, including survival") but it's a significantly harsher game. If you wanted Poison Pill to stay as the reliable anchor (which I'd flagged as a possible design choice in earlier notes), 64% is too low — needs 90%+. Decide explicitly: gamble or anchor?

**Idea backlog adds:**
- 🔴 **Anti-shield as a real backfire effect.** If Poison Pill must have backfire, the punishment could be: caster becomes UNSHIELDABLE for 24h (any shield they cast is automatically nullified, and existing shield charges are stripped). Pure "your defense system has been compromised" satire. On-brand crypto-incident framing.
- 🔴 **Tiered shields.** If shield reliability becomes a gamble, sub-tier the success: 30% chance for cheap-shield-1-charge, 30% chance for mid-shield-2-charges, 5% chance for premium-shield-3-charges (instead of always 1 charge). Would require effectValues wiring extension.
- 🔴 **Shield insurance.** A separate spell or modifier that ENSURES a Poison Pill cast succeeds (at premium cost). "Pay for the audit" type satire — pay extra to guarantee defense. Useful if Poison Pill stays a 65% gamble.

---

### #4 — Crossline Poach — edit on 2026-05-21 (post-wiring)

**New stats:**
- Cost: 150 / 40 / 150 PP (success / fail / backfire) — was flat 15 PP. **10× cost on success.**
- Odds: 8 / 84 / 8 — was 30 / 60 / 10
- Cooldown: 96h (was 8h) — 12× longer
- castLimit: 1, Duration: 0, effectValues: [] (unchanged)

**Real backfire** (caster loses deepest downline to target) — no silent-no-op trap here. Good.

**Identity shift:** "frequent downline-grab gamble" → "rare-and-expensive downline-grab gamble."

**Math:**
- E[cost per cast] = 0.08×150 + 0.84×40 + 0.08×150 = **57.6 PP/cast**
- E[casts per success] = 1/0.08 = **12.5**
- E[PP per successful poach] ≈ **720 PP**
- **E[backfires per success] = 12.5 × 0.08 = 1.0** — you statistically lose one of your own downline per poach you land
- 96h cooldown = once per 4 days max

**Structural insight:** success rate == backfire rate (8/8) makes this **statistically zero-sum on downline membership**. For every poach you land, you lose one of yours. Only net-positive outcome: timing (cast when target's deep downline is *higher quality* than yours, so you trade quality even when quantity is even).

**Concerns:**
1. **720 PP per net-zero downline outcome** is expensive — nearly a month of small-player PP earnings for an even trade in headcount.
2. **96h cooldown** on a once-per-month-cost spell means the mechanic might not affect meta at all.
3. **84% failure rate** with 40 PP cost = a wallet drain feature. Lots of "tried, nothing" moments.
4. **Success effect under-scaled for cost.** One member. Compare to spells that drain 1600 PP or set 7-day siphons.

**Suggested edit:** **15/75/10 with current costs**. Makes poach favor the caster (typically wins), keeps backfire as real punishment, preserves "downline tug-of-war" identity. Current 8/8 split makes the spell a coin flip on cost-only — not interesting strategy.

Alternatives:
- **(b)** Drop costs to 100/30/100, keep low success — reduces wallet drain
- **(c)** Drop cooldown to 48h — lets the mechanic show up in play more

**Idea backlog adds:**
- 🔴 **Asymmetric poach quality.** Right now poach grabs one member (favors L3). Could weight the steal toward target's highest-PP downline, OR toward member with longest tenure (most-established). Adds another dimension to "is this poach worth the gamble." Would need effectValues wiring.
- 🔴 **Poach insurance via Magic Mirror/Poison Pill.** Currently downline shields against direct attacks, not against poach. Question: should Poison Pill block Crossline Poach? Right now it does NOT (only blocks per the `consumeShieldIfActive` calls in offensive handlers, and Crossline Poach doesn't check). If shields are now expensive gambles, expanding what they block might give them more value.
- 🔴 **Multi-poach on critical success.** 5% chance of "critical" success that grabs 2 members instead of 1. Adds variance to the success path beyond binary.

---

## Cross-cutting observations (2026-05-21)

**Roster is becoming a patience economy.** Every spell touched this pass has moved toward higher costs, lower success rates, and dramatically longer cooldowns (Poison Pill 6→72h, Yield Boost 24→48h, Crossline Poach 8→96h, Wealth Tax 12→6h is the exception). Players will cast 1–2 spells per week instead of per session.

This is a coherent design direction — *"each cast is a meaningful commitment"* — but it changes the game feel substantially. Two decisions worth making explicit:

1. **Is Poison Pill an exception to gamble-ification, or part of it?** Defense-as-gamble (current direction) tilts the meta toward offense. If unintentional, raise Poison Pill success to 90%+ to keep it as the reliable anchor.

2. **Is the cooldown escalation intentional?** 72h–96h on multiple spells changes playable rhythm. Make sure that's the design, not drift.

**Repeating silent-backfire trap.** Poison Pill is the THIRD spell this pass with 1%+ backfire odds on a no-op handler (Whitelisted 15%, Override Bonus 10%, Poison Pill 1%). The wiring fix unblocked admin odds editing, but **admin can still create silent-backfire bugs by setting positive backfire odds on spells with stub handlers.** Two roster-wide options:
- **Soft fix:** add a warning banner in admin when setting backfire >0 on a spell whose handler is currently a stub.
- **Hard fix:** block save when backfire >0 conflicts with a stub handler, force admin to either set backfire=0 or implement a mechanic first.

---

### #3 — Trailing Commission — edit on 2026-05-21 (post-wiring)

**New stats:**
- Cost: 15 / 15 / 60 PP (success / fail / backfire) — was flat 15. Backfire cost 4×.
- Odds: 13 / 79 / 8 — was 70 / 20 / 10. Steep drop in success.
- Cooldown: 24h (**unchanged** — striking given the cooldown escalation elsewhere this pass)
- Duration 168, effectValues [5, 1000], castLimit 0 (unchanged)

**Real backfire** (target siphons caster's mints 5%/3-day/cap 1000) — no silent-no-op trap.

**Identity shift:** "70% reliable PP-siphon" → "13% rare expensive PP-siphon with 4× backfire premium." Becomes a **scouting-rewarded spell** — target selection determines ROI.

**Math:**
- E[cost per cast] = 0.13×15 + 0.79×15 + 0.08×60 = **18.6 PP/cast**
- E[casts per success] = 1/0.13 ≈ **7.69**
- E[PP per successful siphon] ≈ **143 PP**

**ROI scales heavily with target activity:**
- Active whale minting 20k+ PP/week: siphon cap binds → ~7× ROI per success
- Mid player minting ~3k PP/week: barely positive EV
- Inactive target: wasted cast

**Cost-shape pattern:** "front-load + linger" — 60 PP backfire premium adds immediate sting on top of the slow-drag backfire effect. Distinct from "free backfire" (Wealth Tax/Bridge Exploit) because their effects are immediate burns.

**Concerns:**
1. **13% success is brutally low** for a target-dependent spell. Players will quit before learning to scout well.
2. **24h cooldown unchanged** while other spells went 48-96h. Either intentional (keep this as a regular tool) or hadn't been touched yet.

**Suggested edit:** nudge to **18/74/8** — gives players enough successes to learn the target-selection strategy before quitting. Costs unchanged.

**Idea backlog adds:**
- 🔴 **Target-activity preview.** Since this spell's ROI depends on target's recent minting velocity, the cast UI could surface "Target's mint rate: 850 PP/day (high)" before commit. Helps players scout. Could also show as a leaderboard slice — "most siphonable targets this week."
- 🔴 **Siphon stacking.** What happens if two casters successfully siphon the same target? Current code likely overwrites the first siphon. Could allow stacking (up to 2-3) so multiple players can target one whale in parallel. Adds coordination/competition dynamics.

---

### #2 — Cease & Desist — edit on 2026-05-21 (post-wiring)

**New stats:**
- Cost: 125 / 25 / 10 PP (success / fail / backfire) — was flat 10. **12.5× success cost.** Backfire stays cheap.
- Odds: 38 / 55 / 7 — was 90 / 5 / 5
- Cooldown: 96h (was 24h) — 4× longer
- Duration 168 (7d), effectValues [7], castLimit 0 (unchanged)

**Real backfire** (caster renamed for 7 days) — no silent-no-op trap.

**Identity shift:** "guaranteed cosmetic griefing" → "high-investment griefing ritual."

**Math:**
- E[cost per cast] = 0.38×125 + 0.55×25 + 0.07×10 = **62 PP/cast**
- E[casts per success] = 1/0.38 ≈ **2.63**
- E[PP per successful rename] ≈ **163 PP**

**Cost-vs-value check:** rename is purely social (no PP transfer). Compared to Whitelisted at ~510 PP/gild (24h, self), Cease & Desist gives **7× the duration on someone else for ⅓ the price per cast**. Cheaper per social-impact-unit than Whitelisted. Coherent.

**Cheap backfire (10 PP) is on-pattern.** Effect (caster renamed 7d) is the punishment; no double-tap. Same shape as Wealth Tax/Bridge Exploit's free-backfire-cost.

**Strategic shape:** 96h cooldown means once-per-4-days max. Combined with 38% success and 7-day duration, **target wears the rename for almost the full cooldown window**. Pick your moment carefully, commit serious PP, target wears it almost continuously. Tight design.

**Concerns:**
1. **55% failure** is the gentlest in this tuning pass (vs Yield Boost 88%, Crossline Poach 84%). Probably because 125 PP success cost is already steep enough that low fail isn't needed for the gamble feel. Coherent — different gamble shape than the buff/grief spells.
2. **7% backfire on a 7-day self-rename** is real consequences. ~1 in 14 casts you wear a clown name for a week. Tilt risk.

**Suggested edit:** ship as-is. The numbers all read coherently.

**Idea backlog adds:**
- 🔴 **Rename pool curation.** The hardcoded rename name pool (`pickRenameName()`) currently picks from a fixed list. If Cease & Desist becomes a high-investment ritual, the *quality* of the random name matters more — players will want the rename to land with VC/MLM satire (e.g., "GeneralPartnerOfMyMomsBasement", "DeFi Death Spiraler", "Liquidity Provider Of Tears"). Worth a copy pass on the name pool.
- 🔴 **Display Rename Indicator on Leaderboard.** Visibly mark renamed players with a tag like "RENAMED" or "🦝" next to their forced name so observers immediately know it's not self-chosen. Amplifies the social griefing payoff.
- 🔴 **Rename auction.** Once-cast, target gets 5 minutes via `setPendingRenameName` to choose a name themselves from a pool. Could extend this: caster picks from 3 candidates the target *must* live with for 7 days. Adds a "choose your poison" decision moment for caster, slightly softens the impact for target.

---

## Cost-shape typology (emerging design language)

The tuning pass has surfaced 7 distinct spell cost shapes worth recognizing as design patterns:

| Pattern | Spells | Logic |
|---|---|---|
| **Free backfire** | Wealth Tax, Bridge Exploit | Effect is the punishment; no double-tap |
| **Due diligence fees** | Yield Boost | Fail > success; "you pay the retainer" |
| **Punishment via cost** | Override Bonus, Poison Pill* | Backfire cost dominates (*silently no-ops on Poison Pill until fixed) |
| **High-success-cost flex** | Whitelisted | Pay big to land the cosmetic |
| **Symmetric expensive** | Crossline Poach | Sticker price same both ways |
| **Front-load + linger** | Trailing Commission | Premium cost on backfire to add sting to a slow-drag effect |
| **Cheap backfire (effect is punishment)** | Cease & Desist | Like free-backfire but with a token cost (could be 0) |
| **Graduated escalation** | Contagion | Cheap to play, more to fail, lots to backfire — lottery ticket with sliding-scale loss |

This is *good*. Each spell has a distinct economic flavor, not just different success rates. Future spells can pick a pattern intentionally. Worth documenting somewhere outside this scratch doc once the tuning pass settles.

---

### #1 — Contagion — edit on 2026-05-21 (post-wiring)

**New stats (all live — effectValues wired pre-tuning, then admin tuned):**
- Cost: 5 / 10 / 60 PP (success / fail / backfire) — was flat 20. New cost-shape pattern: **graduated escalation.**
- Odds: 43 / 39 / 18 — was 40 / 40 / 20 (marginal)
- effectValues: [2, 5, 60] — was [1, 3, 60]. **Damage range nearly doubled.**
- Description updated to match: "surrenders 2-5%" (was "1-3%"). Good discipline.
- Cooldown 12h, castLimit 1, duration 0 (unchanged)

**Mechanic facts:**
- Success: `chipTransfer(victim, caster, ...)` — PP flows FROM each victim TO caster (profit, not griefing)
- Backfire: `burnFrom(caster, ...)` — uncapped burn of 2-5% of caster's stack ([main.mo:2287-2302](main.mo:2287))
- Comment in backfire handler: *"no per-cap on backfire because the description doesn't promise one and casterBal is naturally bounded."*

**The strategic insight:** combination of (transfer with per-victim cap) + (uncapped backfire burn scaling with caster balance) creates a **structural anti-whale gradient.**

**Math (cost + backfire burn):**
- Per cast cost (constant): 0.43×5 + 0.39×10 + 0.18×60 = **16.85 PP**
- E[backfire burn] = 0.18 × 3.5% × casterBal = **0.63% of casterBal**
- Total expected cost: 16.85 + 0.0063 × balance
  - Small fish (1k PP): ~23 PP/cast
  - Whale (100k PP): ~647 PP/cast

**Gain per success** (depends on pool):
- 30 mid-balance players (~500 PP avg): ~525 PP/success
- 10 whales (cap-bound at 60) + 20 mid: ~950 PP/success

**Net EV per cast:**
- **Small fish vs whale-heavy pool:** -23 + 0.43×950 = **+385 PP/cast (insanely positive)**
- **Whale vs same pool:** -647 + 0.43×950 = **-238 PP/cast (uncapped backfire bleeds them)**

**Identity:** the primary anti-whale spell. Combined with Wealth Tax (top-3 targeting), Crossline Poach (downline drain), the roster now offers small players a coherent redistributive toolkit while whales structurally cannot cast Contagion profitably.

**Pairs with Poison Pill becoming unreliable** (prior screenshot): whales can't AOE-defend effectively, so they bleed continuously to small-fish Contagion spam. Whale meta is now pure defense, and defense is now a gamble.

**Concerns:**
1. **5 PP success cost + 43% success + 12h cooldown** = small fish will spam Contagion constantly. ~6 successful AOE drains per week per active small caster. Multiply by every small caster active. Whales drained nonstop.
2. **Whales literally can't rebalance via Contagion** — uncapped backfire burn forbids it. They have no offensive AOE.
3. **18% backfire on 5 PP success cost = small fish take essentially free shots.** Backfire-burn-on-tiny-balance is negligible.

**Suggested edit:** **ship as-is, but decide consciously** whether the anti-whale gradient is intentional. If yes, this is the most coherent spell in the roster — perfect satirical alignment with the late-Ponzi-entrants-clawing-back frame. If no, three knobs:
- Cap the backfire burn (add 4th effectValues slot for backfire cap)
- Reduce backfire rate to 5-10% so whales can risk-tolerate occasional casts
- Accept that whales avoid this spell (current state, arguably correct)

My pick: **ship as the cleanest design statement in the tuning pass.** The anti-whale alignment is a powerful satirical anchor for the whole roster.

**Idea backlog adds:**
- 🔴 **Asymmetric effectValues for success vs backfire.** Currently success uses pctMin/pctMax/cap from effectValues[0,1,2], and backfire uses the same pctMin/pctMax but no cap. Could add effectValues[3] = backfire cap as an explicit slot — would let admin cap the whale-vulnerability if desired without rewriting the handler.
- 🔴 **Shield economics revisit.** Contagion is the most-cast hostile per week against any active player. With Poison Pill at 80 PP and 65% success, defending against Contagion alone is costlier than just absorbing it for whales with balance 8000+. Worth checking that this is intended outcome.
- 🔴 **Pool-size-aware UI.** Contagion's value depends on pool size and balance distribution. Cast UI could surface "Active players: 32 · Avg balance: 480 PP · Est. AOE gain on success: 480 PP" to help players reason about timing.
- 🔴 **"Tax season" event.** Pre-announced 24h window when Contagion (or all AOE redistribution) doubles or has +50% success. Creates a social event around drains. Whales prepare defenses, small fish queue casts. Pure stream-able drama.

---

### #0 — MEV Attack — edit on 2026-05-21 (post-wiring)

**New stats (all live):**
- Cost: 10 / 15 / 25 PP (success / fail / backfire) — was flat 10. **Graduated escalation** (Contagion-pattern).
- Odds: 60 / 25 / 15 — unchanged from default
- effectValues: [7, 19, 250] — was [2, 8, 250]. **Damage range nearly 3× the floor, 2.4× the ceiling.** Cap unchanged.
- **Description + backfire description both updated to match** the new range. First edit in the pass where Charles updated copy alongside effectValues without prompting. Good discipline.
- Cooldown: 4h (was 2h)
- castLimit 0, duration 0 (unchanged)

**Real backfire:** caster pays target 7-19% of own PP, cap 250 (`chipTransfer(caster, t, ...)`). Mirror of success.

**Mechanic facts:**
- Success: TRANSFER from target to caster (caster profits)
- Backfire: TRANSFER from caster to target
- Cap (250 PP) binds for targets >1316 PP balance

**Identity:** **The dominant small-fish offensive tool.** Paired with Contagion forms a single-target/AOE small-fish toolkit. Both share the anti-whale gradient (cap binds for whale targets while caster's per-target burn caps too).

**Math (small caster):**
- Per-cast cost = 0.60×10 + 0.25×15 + 0.15×25 = **13.5 PP/cast**
- Plus backfire-transfer scaling with caster balance: ~0.15 × 13% × casterBal (capped at 250)
- Total expected cost (100 PP caster): ~15.5 PP/cast
- Total expected cost (whale caster): ~51 PP/cast

**Gain per success** (depends on target):
- vs whale (cap binds): 250 PP
- vs mid (1000 PP): ~130 PP
- vs small (100 PP): ~13 PP

**Net EV (small caster, picking targets):**
- vs whale: **+134 PP/cast**
- vs mid: +62 PP/cast
- vs small: -7.5 PP/cast (don't)

**Key insight: ~6 successful attacks per day per active small caster.** 60% success + 4h cooldown (success-only lockout) = roughly 1.67 attempts per success, ~6 hits/day. **Multiply by every active small caster = whales get hit dozens to hundreds of times per day.**

**Pairs with Contagion** but is structurally MORE potent:
1. MEV Attack lets caster *target* whales specifically (Contagion AOE distributes damage)
2. Cap protects whales from absolute loss per hit but doesn't reduce frequency
3. 60% vs 43% success — MEV Attack lands way more reliably
4. 4h vs 12h cooldown — MEV Attack fires 3× as often

**Concerns:**
1. **Most spammable offensive in the roster** (60% + 4h). With ~9× ROI vs whales for small fish, this becomes the default play pattern.
2. **Whale defense is unworkable.** Poison Pill at 65% success/72h cooldown can't replenish shields fast enough vs MEV Attack frequency. Whales just bleed.
3. **Mid-tier is the new pain center.** For 1000-1500 PP balance targets, cap doesn't bind, so they lose absolute %-of-stack faster than whales. **Risk: mid-tier player churn** when newly recruited players get hammered before building defenses.

**Suggested edit:** **ship as-is if the anti-whale + new-player-onboarding-friendly thesis holds.** If new players quit after being MEV-attacked into oblivion, soften:
- Cooldown 6-8h (cuts daily attack rate)
- Damage ceiling [7, 15, 250] (caps relative variance)
- Don't touch the 60% success — that's the spell's identity as the reliable offensive tool

Pre-launch this is a hypothesis. Post-launch, watch mid-tier churn.

**Idea backlog adds:**
- 🔴 **MEV Attack rate-limiter per target.** Currently anyone can cast 6 MEV Attacks/day against one target. If 10 small fish all target the same whale, that's 60 hits/day. A per-target rate limit (e.g., max 2 MEV Attacks per target per 24h regardless of caster) would prevent whale-pile-on while keeping the spell strong against varied targets. Forces tactical spread of attacks.
- 🔴 **MEV Attack rebate.** Failed casts give back 50% of the failure cost. Creates a "courtesy refund" framing — *"the dealer gives you back half your wager when the trade doesn't fill."* Pure MEV-bot satire. Small mechanical impact, big flavor win.
- 🔴 **Sandwich-attack timing window.** MEV in real life is about being first/last in a block. Could add a 5-second cast window where the spell is buffed if it lands during a "trading window" (a synchronized server tick announced in chat). Adds skill expression without changing core math. High effort, niche payoff.

---

## Whale meta concern (cross-cutting, 2026-05-21)

With the full tuning pass complete, the roster now has a clear structural imbalance:

**Small fish have:**
- Contagion (AOE drain, anti-whale gradient, 12h cooldown)
- MEV Attack (single-target spike, ~9× ROI vs whales, 4h cooldown)
- Wealth Tax (top-3 redistribution, 6h cooldown — wait need to double-check, was 12h originally)
- Crossline Poach (downline grab, 96h cooldown — too rare to matter)
- Bridge Exploit (high-variance attack, 8h cooldown)

**Whales have:**
- Override Bonus (downline buff — only useful if they have downline; gambly post-tuning)
- Whitelisted (cosmetic flex — pure status, no economic effect)
- Poison Pill (unreliable defense — 65% gambly, 72h cooldown)
- Yield Boost (self-buff — works for whales but with 88% failure)

**Whales have no offensive tool.** Every aggressive spell is either anti-whale (Contagion, Wealth Tax, MEV Attack) or symmetric-and-rare (Crossline Poach, Bridge Exploit, Trailing Commission). No spell rewards "whale attacking anyone."

**Three resolution options:**
1. **Embrace it.** The thesis is "early Ponzi entrants get clawed back." Whales play defense and watch their lead erode. Late entrants extract. This is the most satirically coherent option and matches the brand voice perfectly.
2. **Add a whale-favorable offensive.** New spell (or repurpose Override Bonus) where being big is the *advantage*. E.g., "Pull Rank" — uses your PP balance as a multiplier to overwhelm a smaller target. Restores symmetric play.
3. **Cap the small-fish dominance.** Per-target rate limits, harder backfire scaling, smaller damage caps for small casters. Mechanical fixes to break the optimal play pattern.

**Recommend option 1.** It's the cleanest design statement and the most on-brand for the satirical thesis. Ship the tuning, let whales feel the squeeze, lean into the "early-entrant-rugs-itself" punchline as the core game feel. Post-launch, if whales churn, revisit.

---

## Deferred work

- ✅ **Wire admin odds + effectValues to runtime** — RESOLVED via PR #88 on 2026-05-21. See the ✅ RESOLVED section above for the final scope and the new pre-deploy blocker it exposed.
- ⏸ **Resolve silent backfires for Whitelisted and Override Bonus.** Both now roll backfires under live odds (15% and 10% respectively) but the handlers are no-op stubs. Either drop backfire odds to 0 in admin (Path A, 30 sec) or build Blacklisted + Override Reversal (Path B, half-day session). Block any deploy until this is resolved.
- ⏸ **Implement the "24h or 7d roll" for Whitelisted** (or remove the effectValues hint that suggests it). Duration is now wired to `config.duration` (24h), but `effectValues = [24, 168]` still implies an unimplemented two-value roll mechanic. Either ship the weighted roll (e.g., 80% short, 20% long) or trim effectValues to a single value and update the [DocsPage.tsx:334](../frontend/src/components/DocsPage.tsx) fallback string `"(24h or 7d)"` to match.
- ⏸ **Admin UX cleanup — gray out dead fields + universal templating.** Promoted to a dedicated plan section below (see "## Plan: admin UX cleanup"). Combines what was previously two separate deferred bullets (description templating discipline + per-spell field gating) plus the silent-backfire-trap save-blocker.
- ⏸ **Backfire mechanics: Blacklisted + Override Reversal.** Per the idea backlog entries for Whitelisted (#10) and Override Bonus (#9). These are the Path B resolution to the silent-backfire blocker above.
- ⏸ **Wire `castLimit` enforcement.** Field exists, admin edits work, value validates non-negative, but it's not checked anywhere in the cast pipeline. Cooldown is the de facto recast gate today.
- ⏸ **Promote the Poison Pill charge cap to a tunable.** The magic-mirror handler hardcodes a charge ceiling of `3` ([main.mo:2178](main.mo:2178)): `let newCharges : Nat = if (priorCharges + 1 > 3) { 3 } else { priorCharges + 1 };`. The accompanying comment even spells out the design intent — *"Cap at 3 so castLimit=2 can be raised without runaway shielding"* — i.e., the `3` was meant as a backstop while `castLimit` did the per-round gating. With castLimit unenforced (above), `3` is doing all the work alone, and it's invisible to the admin panel. Should be promoted to a config field (e.g. `effectCap : Nat` on `ShenaniganConfig`, or shield-specific `shieldChargeCap`). Pair this with the castLimit wiring PR — same area of code, same design decision (how many of this spell can a player keep stockpiled?). UX corollary: once visibility lands, the Poison Pill card should refuse to cast (gray out with "Shield full") when charges are at cap, so players don't blindly burn PP for a silent expiry-refresh.
- ⏸ **Free-backfire-cost audit.** New pattern from this tuning pass: when a backfire effect itself is brutal (Wealth Tax compound drain, Bridge Exploit burn), `costBackfire = 0` avoids double-tap. MEV Attack, Contagion still charge full cost on backfire — worth deciding whether to apply the pattern.

---

## Plan: admin UX cleanup — honest fields + universal templating (deferred, 2026-05-21)

Two related fixes to make the admin UI consistent with runtime behavior and the displayed copy self-syncing with numeric config. Captured after Charles tried to shorten Cease & Desist's rename to "2 days" by editing the description text + effectValues[0] but didn't change `config.duration` (the actual lever) — illustrating exactly the desync problem this plan solves.

### Problem statement

Today, admin can edit fields that have no runtime effect, and descriptions hardcode numbers that drift from config. Three concrete examples:

1. **Cease & Desist effectValues is decorative.** Only `config.duration` drives the rename mechanic ([main.mo:2037](main.mo:2037)). Editing effectValues[0] from 7 to 2 changes nothing.
2. **Bridge Exploit description says "max 800 PP" while effectValues[2] = 1600.** Post-wiring the actual cap is 1600. Description lies.
3. **Wealth Tax description says "max 300 PP/whale" while effectValues[1] = 900.** Same story.

Charles can edit copy without realizing it doesn't change the mechanic, OR change the mechanic without remembering to update the copy. Three places to keep in sync (description, backfire description, numeric config) and no enforcement.

### Part 1 — Field-state metadata: gray out / disable / warn

Per-spell-type metadata mapping that drives admin field rendering. For each field, mark as `wired`, `decorative`, or `unwired-stub`:

| Spell | duration | effectValues[0,1,2] | castLimit | backfire-handler |
|---|---|---|---|---|
| MEV Attack | decorative (instant) | wired [pctMin, pctMax, cap] | decorative (not enforced) | implemented |
| Contagion | decorative | wired [pctMin, pctMax, cap] | decorative | implemented |
| Cease & Desist | wired (rename duration) | decorative | decorative | implemented |
| Trailing Commission | wired (success siphon) | wired [pct, cap] | decorative | implemented (backfire duration hardcoded `halfWeekNs` — separate issue) |
| Crossline Poach | decorative | n/a (empty) | decorative | implemented |
| Poison Pill | decorative | n/a | decorative | **unwired-stub** |
| Yield Boost | decorative | wired [boostMin, boostMax] | decorative | **unwired-stub** |
| Bridge Exploit | decorative | wired [pctMin, pctMax, cap] | decorative | implemented |
| Wealth Tax | decorative | wired [pct, cap] | decorative | implemented |
| Override Bonus | decorative | wired [multiplier] | decorative | **unwired-stub** |
| Whitelisted | wired (gild duration) | decorative | decorative | **unwired-stub** |

**Admin UI rendering rules:**
- **Decorative fields:** render at ~40% opacity with hover tooltip *"Not used by this spell."* Save still works but the field's value is documented as having no effect.
- **Unwired-stub backfire handlers:** if `backfireOdds > 0`, show a red warning *"⚠️ Backfire has no mechanic. Players will pay PP and see 'BACKFIRED' with no effect. Set this to 0 or implement the handler first."* **Block save** until backfire = 0 or the handler is implemented and the metadata flips to `implemented`. This kills the silent-backfire trap class permanently.
- **castLimit:** gray out everywhere with tooltip *"Not enforced yet. Cooldown is the real recast gate."* Or hide entirely until enforcement lands.

The metadata mapping lives in [ShenaniganAdminPanel.tsx](../frontend/src/components/ShenanigansAdminPanel.tsx) alongside the field render logic. Comments reference main.mo line numbers so the next time a handler is rewired, the source-of-truth pointer is obvious.

### Part 2 — Universal description templating

The templater already supports `{0}`, `{1}`, `{2}` (effectValues indices), `{dur_h}` (duration hours), and `{dur_d}` (duration / 24, days). It's wired into [DocsPage.tsx](../frontend/src/components/DocsPage.tsx) and the spell cards via `renderTemplate` calls. **No description currently uses any placeholder** — every description hardcodes numbers.

**Migration table** (proposed copy for each of the 11 spells):

| Spell | New description | New backfire |
|---|---|---|
| MEV Attack | `"Sandwich-attacks the target for {0}–{1}% of their Ponzi Points (max {2} PP)."` | `"You pay the target {0}–{1}% of your PP (max {2})."` |
| Contagion | `"Losses get socialized — every player surrenders {0}–{1}% (max {2} PP each)."` | `"You burn {0}–{1}% of your own PP."` |
| Cease & Desist | `"Target is forced to change their display name for {dur_d} days."` | `"You get renamed for {dur_d} days."` |
| Trailing Commission | `"Skims {0}% of target's new PP for {dur_d} days (max {1} PP)."` | `"The target siphons {0}% of YOUR mints for 3 days (cap {1} PP)."` (3 days is the hardcoded `halfWeekNs` — separate fix to templatize) |
| Crossline Poach | unchanged (no numbers) | unchanged (no numbers) |
| Poison Pill | unchanged (no numbers) | `"Cannot backfire."` (replace when backfire mechanic ships) |
| Yield Boost | `"Earn +{0}–{1}% additional PP for the rest of the round."` | `"Cannot backfire."` |
| Bridge Exploit | `"Target loses {0}–{1}% of their PP (max {2} PP)."` | `"A token you're holding was using the bridge."` (typo fix folded in) |
| Wealth Tax | `"A socialist mayor takes office — {0}% from the top 3 PP holders (max {1} PP/whale)."` | existing satirical text — no numbers to template |
| Override Bonus | `"Your downline kicks up {0}x PP for the rest of the round."` | n/a (no backfire mechanic yet) |
| Whitelisted | `"Gold name on the leaderboard for {dur_h} hours — the only clout that matters."` | n/a (no backfire mechanic yet) |

After migration, editing any numeric config in admin auto-updates everywhere descriptions render:
- Admin preview pane (live as you type)
- Spell card on Shenanigans tab (already live-pulls config per PR #81)
- Docs page (already live-pulls per PR #81)
- Cast outcome toasts — **needs verification.** Likely still uses hardcoded text in [Shenanigans.tsx](../frontend/src/components/Shenanigans.tsx) toast component. May need a small fix to pass templated description into the toast.

### Single source of truth (target state)

After both parts ship:
- All numbers live in numeric config (`effectValues`, `duration`, `cooldown`, costs).
- All copy lives in `description` + `backfireDescription` with placeholders.
- Admin edits to numeric config propagate automatically to all rendered copy.
- Dead fields are visually marked.
- Silent-no-op traps become impossible (UI blocks the misconfiguration).

### Acceptance criteria

- [ ] Per-spell field metadata mapping exists and drives admin field opacity
- [ ] Decorative fields render at reduced opacity with explanatory tooltip
- [ ] Save is blocked when `backfireOdds > 0` on an unwired-stub backfire handler
- [ ] All 11 descriptions migrated to placeholders (or explicitly marked no-number-needed)
- [ ] All 11 backfire descriptions same treatment
- [ ] Bridge Exploit "A the token" typo fixed in the templated copy
- [ ] Bridge Exploit description shows live cap (1600 PP, not 800) by templating against effectValues[2]
- [ ] Wealth Tax description shows live cap (900 PP/whale, not 300) by templating against effectValues[1]
- [ ] Cease & Desist description shows live duration via `{dur_d}` (so changing duration → 48h auto-updates "for 2 days")
- [ ] Whitelisted description shows live duration via `{dur_h}` or `{dur_d}`
- [ ] Admin preview pane shows the templated result so edits are visible before save
- [ ] Cast outcome toasts confirmed to use templated descriptions (verify; fix if not)

### Out of scope (separate work)

- Wiring castLimit enforcement (the field becomes a real gate, not just decorative)
- Building backfire mechanics for Whitelisted / Override Bonus / Poison Pill (separate spec — see Blacklisted / Override Reversal in the per-spell idea backlogs above)
- Templating the Trailing Commission backfire duration (currently hardcoded `halfWeekNs`; would need a new effectValues slot or a `backfireDuration` config field)
- Adding a `lockoutOnFail` flag (cooldown-on-any-outcome — separate cross-cutting decision)

### Estimated effort

Half-day total:
- Part 1 (field-state metadata + UI rendering): ~2 hours including the save-block logic
- Part 2 (description migration): ~1 hour mechanical edits across 11 spells × 2 fields each
- Toast verification + fix if needed: ~30 min
- Testing across admin preview + spell card + docs + toasts: ~1 hour

Low-risk because none of this touches the canister — frontend-only changes plus admin config edits. The templater is already in place. The metadata mapping is additive.
