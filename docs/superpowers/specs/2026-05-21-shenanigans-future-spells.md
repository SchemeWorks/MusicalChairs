# Future Shenanigans — Spell Concept Backlog

**Status:** ideas-only. None implemented. Brainstormed 2026-05-21 during the post-wiring tuning pass (see [shenanigans/TUNING_NOTES.md](../../../shenanigans/TUNING_NOTES.md) for that session's context).

**Purpose:** capture concrete new-spell ideas with enough detail to pick up cold later, separated by *what design problem they solve*. Not all of these should ship — they're a menu.

---

## Theme 1 — Whale offensive toolkit (5 ideas)

**Design problem this addresses:** post-tuning, the 11-spell roster has all-anti-whale offense (Contagion, Wealth Tax, MEV Attack, Bridge Exploit) and unreliable defense (Poison Pill gambly). Whales aren't financially hurt by this (the per-cast EV cost is <1% of their stack/day at worst), but they have **nothing fun to spend offensive cycles on.** It's a game-feel boredom problem, not a balance problem. These five ideas give whales something to *do*.

The structural pattern across all of them: gate by minimum caster balance, scale reward with caster balance (or eliminate balance-scaling on backfire), so the spells are uncastable or unprofitable for small fish.

### 1.1 — Tender Offer

**The cleanest fit.** Pay a premium to absorb a smaller player's entire position.

- Pre-cast gate: target must have balance ≤ 50% of caster's. Otherwise the spell rejects.
- **Success:** target's entire PP balance transfers to caster. Target is "acquired" — locked out of casting any spell for 24h (post-acquisition integration period).
- **Failure:** target keeps your premium (cost paid) and you don't get them. They smell blood and you publicly tried.
- **Backfire:** target receives 3× your cost as "poison pill compensation," and you can't cast Tender Offer for 7 days.
- **Cost:** 500 / 100 / 300
- **Odds:** 35 / 50 / 15
- **Description draft:** *"Make a tender offer for a smaller player's entire position. They get taken private. Their cap table integrates into yours."*

Why whale-favorable: cost in absolute PP is whale-only; reward scales with target's balance uncapped; backfire absolute is whale-friendly. Anti-small-fish by sticker price alone.

### 1.2 — Margin Call

**The predator move.** Force-liquidate any player whose balance falls below a threshold.

- Pre-cast gate: caster must have ≥ 5,000 PP balance.
- **Success:** target loses their entire stack (burned, not transferred). Caster receives a 5% "liquidation fee" of target's lost balance, capped at 500 PP.
- **Failure:** target gets a 24h "underwater warning" notification but otherwise unscathed.
- **Backfire:** caster is publicly flagged as a "predatory liquidator" — every other player's spells against caster get +50% success rate for 12h. Reputational, not financial.
- **Cost:** 200 / 50 / 50
- **Odds:** 40 / 50 / 10
- **Description draft:** *"Call the loan. Target liquidates their position. You take a finder's fee for the cleanup."*

Why whale-favorable: hard caster minimum (5k), uncapped destruction, reputational backfire whales can absorb.

### 1.3 — Founder's Allocation

**The passive flex.** A self-buff that *increases* the more PP you already hold. Literal rich-get-richer.

- **Success:** for 24h, all your PP minting rates get a multiplier = `1 + log10(your_balance / 1000)`. So 1k = 1×, 10k = 2×, 100k = 3×, 1M = 4×. Stacks with Yield Boost and Override Bonus.
- No effect on other players.
- **Failure:** nothing.
- **Backfire:** "SEC opens an S-1 amendment investigation." Your minting rate is set to 0× for 24h.
- **Cost:** 1000 / 100 / 500
- **Odds:** 50 / 35 / 15
- **Description draft:** *"Self-grant a founder's bonus to your PP minting. Multiplier scales with your existing position size — the bigger you are, the more the system loves you."*

Why whale-favorable: the only spell in the roster with effect that scales positively with caster balance. Doesn't disrupt small-fish-vs-whale dynamics — pure flex with no interaction.

### 1.4 — Capital Call

**The downline tax.** Force your downline to "contribute" PP to you.

- Pre-cast gate: caster must have ≥ 3 active L1 downline members.
- **Success:** each downline member (L1, L2, L3) surrenders 5% of their balance to caster, capped at 200 PP per member. Shielded members skipped.
- **Failure:** nothing; your downline is unaware.
- **Backfire:** your downline gets a public "capital call breach" notification — each receives 50 PP from YOU as compensation, AND each gets 24h immunity to your future spell casts.
- **Cost:** 100 / 30 / 30
- **Odds:** 50 / 35 / 15
- **Description draft:** *"Issue a capital call to your downline. Everyone in your network chips in 5%. If they refuse, you're paying severance."*

Why whale-favorable: requires a downline (whales typically have one from recruiting); reward scales with downline size and wealth; backfire is relational damage small fish can't trigger.

### 1.5 — Strategic Dilution

**Chaos option.** Reduce everyone else's relative leaderboard position by issuing yourself "new shares."

- **Success:** caster gains 10% of their own current balance as bonus PP (minted from nothing, capped at 1000 PP/cast). Everyone else's leaderboard rank shifts accordingly.
- **Failure:** nothing.
- **Backfire:** "Shareholders sue. SEC charges." Caster loses 20% of their own balance (uncapped). Public "dilution attempt failed" toast goes to all players.
- **Cost:** 0 / 100 / 0 (free to play, expensive to fail — inverse "due diligence fees")
- **Odds:** 25 / 65 / 10
- **Description draft:** *"Issue yourself bonus shares — purely for liquidity purposes. The rest of the cap table just got diluted."*

Why whale-favorable: reward scales with caster balance, backfire is %-of-balance uncapped (terrifying for whales — but they're the only ones who profit from it).

**Caveat:** the "free PP from thin air" mechanic is the most satirically savage but creates the loudest inflation pressure on the economy. Worth shipping eventually for the voice, but not in a tuning pass focused on balance.

### Whale-toolkit ship recommendations

- **Just one:** Tender Offer. Cleanest satirical fit, most coherent counter to small-fish AOE drain, creates strategic loop (small fish drain whales → whales acquire smaller players → balance pressure rebalances).
- **Two:** add Founder's Allocation. Pure passive flex with no anti-small interaction, doesn't escalate hostilities.
- **Three (full whale toolkit):** Tender Offer + Margin Call + Founder's Allocation. Mirrors the small-fish toolkit (single-target spike + mass attack + self-buff).

---

## Theme 2 — Social-attractor / community-event spells (twin pair)

**Design problem this addresses:** the existing roster is built around 1v1 or 1vMany damage flows. There's no spell whose primary value is *generating community focus* — chat reactions, mob mentality, hero/villain narratives. This pair fills that gap.

**Key insight (user's framing):** the PP amounts are decoys. The real reward is collective attention. When everyone's piling on the villain caster, ambient plays happen unnoticed — emergent gameplay layer.

### 2.1 — Stimulus Check (hero)

**The crowd-pleaser.** Caster delivers a windfall to themselves and a smaller payday to every other holder.

- **Success:** caster +100 PP; every other holder +40–50 PP (rolled per-victim).
- **Failure:** nothing.
- **Backfire:** "Bill didn't pass — you ate the lobbying budget." Caster -200 PP.
- **Cost:** 100 / 30 / 50
- **Odds:** 55 / 35 / 10
- **Source of PP:** pure mint (inflationary; accept it for the spectacle).
- **Cooldown:** 24h
- **Description draft:** *"Pull strings at the Fed — everyone gets a check. You get a bigger one for proposing it."*

### 2.2 — Bear Raid (villain)

**The crowd-targeter.** Symmetric inversion of Stimulus Check.

- **Success:** caster +100 PP; every other holder -40–50 PP (rolled per-victim).
- **Failure:** nothing.
- **Backfire:** "You misread the cycle." Caster -100 PP; everyone else +40–50 PP (karmic inversion — villain accidentally helps).
- **Cost:** 100 / 30 / 50
- **Odds:** 55 / 35 / 10
- **Source of PP:** drain from victims; caster keeps 100, excess burns.
- **Cooldown:** 24h
- **Description draft:** *"Coordinated short. You profit on the spread; everyone else takes a haircut."*

### Why the twin design works

- **Identical numbers and odds.** Players grok the symmetry instantly. Twin spells, opposite valence.
- **55% success** = both will land regularly. Spectacle happens often enough to be a real game-loop feature, not a rare easter egg.
- **Inflationary hero / deflationary villain** roughly balance over time. Economy doesn't drift one direction.
- **Bear Raid backfire = accidental Stimulus Check.** The most satisfying narrative possible: would-be villain becomes unwilling hero. Chat explodes.

### Social hooks to layer on top

1. **Public broadcast banner on every successful cast.** *"[Name] just stimulated the economy! You received 47 PP."* / *"[Name] just shorted the market! You lost 43 PP."*
2. **Auto-trollbox post.** *"Bonsai Capital dropped a Stimulus Check 💸 — everyone got paid."* / *"RatPoison Investments just shorted you all 🐻 — bag holders, line up."*
3. **Hero/Villain leaderboards.** "Top Heroes This Week" and "Most Wanted Villains This Week" — lifetime tallies. Players will play for the title.
4. **Bear Raid pile-on debuff.** A successful Bear Raid grants 24h "Most Wanted" status: any spell cast against the Bear Raider within that window gets +20% success rate. Mechanical incentive for the pile-on the chat will naturally form. Heat-sink dynamic — while everyone's targeting the villain, other plays sneak in unnoticed.

### Why these aren't "for the little guys" only

The user framed these as small-player spells but the structure works for *anyone*. Whales casting Stimulus Check become beloved community figures (a different kind of status than the leaderboard). Whales casting Bear Raid become enemies of the people (a different kind of attention than just being the top-3 PP holder). These spells let everyone perform a *role*, regardless of balance. That's their real value.

### Implementation considerations (when ready)

- Stimulus Check source = pure mint requires a new mint path in the canister (currently mints come through the cascade observer). Probably a `programmaticMint` helper that bypasses the cascade — small wiring task.
- Bear Raid source = transfer + burn excess. Reuses existing `chipTransfer` and `burnFrom` primitives. Cleanest path: transfer cap PP to caster, burn the rest.
- Both need the AOE iteration pattern (already used by Contagion at `#aoeSkim`) — wallpaper from that handler.
- New `MostWanted` map for Bear Raid pile-on debuff. Read on every cast attempt against a Most Wanted target, multiply baseSuccess.
- Public broadcast banner is a frontend concern — backend just needs to emit a clear `ShenaniganOutcome` record with caster/effect details.

---

## Cross-references

- Tuning context for the existing 11 spells: [shenanigans/TUNING_NOTES.md](../../../shenanigans/TUNING_NOTES.md)
- Earlier feature backlog (public feed, MLM ranks, bounties, insurance with Reginald, Dokapon curses/events): `project_shenanigan_feature_backlog` memory entry
- Cost-shape typology that emerged during tuning is in TUNING_NOTES.md — new spells should pick a pattern intentionally rather than drift
- Silent-backfire trap concern (any new spell with positive backfire odds needs a real handler, not a stub) — also in TUNING_NOTES.md
