# Ponzi Points & MLM Redesign — Design Notes

Working notes. Not user-facing docs. Tracks the full design pile from the in-session brainstorm so we don't lose anything between work blocks.

User-facing docs to update once changes land: `frontend/src/components/DocsPage.tsx`. Match existing tone (formal/business voice, knowing satire, MLM/VC jargon over casino).

---

## v1 — Locked scope

### MLM mechanics
- **Recursive cascade** replaces current direct 3-tier. New player pays 10% of their PP earnings to their direct upline. Upline keeps 50%, passes 50% further up. Each level above bleeds.
- **Deductive cascade** (decided 2026-05-15): the 10% comes OUT of the player's mint. Player earns `base - cascade`; upline receives `cascade` distributed up the chain. Total PP minted per event = base (conserved). Matches the receipt-narrative ("here's your gift, here are the deductions, here's your net").
- **Charles-as-default upline.** Any account with no referrer gets Charles as upline. Catch-all → Charles eventually skims everything.
- **Activity gating.** Cascade walks upline chain; skips any account that hasn't met the activity bar (≥0.1 ICP deposit ever). Skipped accounts don't get paid; cascade routes around them. Closes the dummy-chain loophole.
- **Signup gift, 500 PP**, gated behind first deposit (0.1 ICP min). Receipt shown to new joiner shows where their sponsorship cut cascades upward.
- **Reverse index** on `referralChain` so "who is in Rob's downline?" is a constant-time lookup, not an O(N) scan.
- **`getPayoutTarget(downliner)` abstraction** in cascade logic so the v2 NFT migration is a one-line swap, not a refactor.
- **MLM tab query method** on the shenanigans canister + replace the hardcoded-zero stub in `useGetReferralStats`.
- **Toast + persistent tab badge** when someone signs up via your link. Toast wording: *"You've grown your downline!"* with subtext *"You've added a new member to your organization."*
- **Backfill** referral payouts for any deposits made between signup and `registerReferral` being called. (Or document loudly that registration must precede first deposit.)

### Economy rebalance — lower mint rates instead of raising spell prices

PP supply is inflationary today (sources >> sinks per audit). Fix on the source side. Single knob, cleaner than 5+ price changes.

| Source | Today | Proposed |
|---|---|---|
| Simple 21-day | 1,000 PP/ICP | **200 PP/ICP** |
| Compound 15-day | 2,000 PP/ICP | **400 PP/ICP** |
| Compound 30-day | 3,000 PP/ICP | **600 PP/ICP** |
| Backer | 4,000 PP/ICP | **800 PP/ICP** |
| Signup gift | (none) | **500 PP** (gated behind first deposit) |
| Referral cascade | 8/5/2% direct | 10% recursive (50% passthrough) |

5x cut on all mint sources. Existing whale balances retained — on-brand for a Ponzi where early entrants benefit.

### Shenanigan effects — 5 financial spells (rest deferred)

Implement effect bodies for these only. Cosmetic / state-with-expiry spells stay stubbed for v1.

| Spell | Cost | Effect | Cap (new) |
|---|---|---|---|
| Money Trickster | 120 PP | Steal 2–8% target PP, transfer to caster | **500 PP** (was 250) |
| AOE Skim | 600 PP | Siphon 1–3% from each active player to caster | **100 PP/player** (was 60) |
| Purse Cutter | 900 PP | Target loses 25–50% PP (burned, caster gains 0) | **1,500 PP** (was 800) |
| Whale Rebalance | 800 PP | Take 20% from top 3 PP holders, transferred to caster | **500 PP/whale** (was 300) |
| Magic Mirror | 200 PP | Self-buff: blocks next hostile spell against you | n/a |

### Rubber-banding modifier (Mario Kart catch-up)

For all aggressive spells, applied at odds-resolution time:
- Caster has <50% target's PP → **+25% success rate**
- Caster has >200% target's PP → **−25% success rate**
- Otherwise: no modifier
- Clamp final success rate to [5, 95].

### Skipped for v1
- **Defection fee** — replaced by v2 NFT-burn shenanigan
- **Downline Heist, Downline Boost spells** — restructured under v2 NFT model
- **Mint Tax Siphon, Rename Spell, PP Booster Aura, Golden Name** — cosmetic / state-with-expiry effects, deferred to v1.5

---

## v2 — NFT contracts + Harberger tax + marketplace

After v1 ships and we've watched player behavior.

### Core mechanics
- **NFT per downline edge.** When a new player signs up, a contract NFT is minted to their referrer (or Charles if none). NFT metadata: `{ downliner: Principal, original_sponsor: Principal }`.
- **Payment rule:** whoever currently holds the NFT receives the downliner's tax. Cascade traverses NFT ownership instead of the static referralChain map.
- **Harberger tax.** Each NFT owner self-declares a sale price. Owner pays an ongoing tax (% of declared price) to Charles. Anyone can force-buy at the declared price at any time. Forces honest valuation; concentration becomes self-limiting via tax burden; creates a passive ongoing PP sink to Charles.
- **Marketplace UI** for browsing, listing, and buying contracts.
- **Built-in defection:** player can buy their own contract at the declared Harberger price and burn it → out of the pyramid.

### v2-only shenanigans
- **Contract Steal** — spend PP for a chance to forcibly seize a random contract from another holder.
- **Contract Burn** — very expensive, chance-based, destroys one of *your own* obligations.
- **Raffle** — spend PP for a chance at a random Charles-held contract.
- **Rubber-banding on Contract Steal odds:** more contracts you hold → higher odds *you* get hit.
- **Idle decay on Contract Steal odds:** longer you go without deposit/claim/cast → higher odds your contracts get stolen.

---

## Confirmed design pile (build after v1 ships)

All locked in — design done, not yet scheduled.

### Public shenanigan feed

- **Live ticker** on dashboard: *"[Beady] just hit [Rob] for 1,200 PP via Money Trickster."*
- **Backfire theater:** *"[Rob] tried Whale Rebalance — BACKFIRED for 600 PP. The pot laughs."*
- **"Word on the street"** flavor lines: *"[Beady] muttered something about 'getting yours.'"* *"[Rob] is now visibly seething."*
- **Streak callouts:** *"[Beady] has hit 3 spells in a row. She's on a roll."* *"[Rob] has been hit 5 times today. Are you OK, buddy?"*
- **Daily highlights digest** on profile: "Biggest steal of the day," "Most spectacular backfire," "Most-targeted player."

Highest engagement-per-line-of-code. Should ship right after v1.

### MLM rank titles

Auto-assigned based on downline size + total PP earned. Titles should be lifted from real MLM compensation plans (Amway, Herbalife, Mary Kay).

| Rank | Threshold | Perk |
|---|---|---|
| Cold Lead | 0 PP, 0 downline | none |
| Affiliate | First deposit | none |
| Junior Partner | 5 downline OR 5k PP earned | small profile badge |
| Senior Advisor | 25 downline OR 25k PP earned | 5% cooldown discount |
| Regional Director | 100 downline OR 100k PP earned | 10% cooldown discount, profile flair |
| Diamond Director | 500 downline OR 500k PP earned | 15% cooldown discount, gold name |
| Triple-Diamond Founder's Circle | 2,500 downline OR 2.5M PP earned | unique title, leaderboard flag, smug emoji |

### Bounties (crowdsourced harassment)

- **Open bounties feed:** dedicated page listing every active bounty. Sortable by size, age, target.
- **Bounty pile-on:** anyone can ADD to an existing bounty. Shows contributor count.
- **Bounty multipliers over time:** unclaimed bounties grow by % per day. Funded by contributors (top-up tax) or Charles minting.
- **Permanent targets** (whale-tier): pay huge PP fee to make someone public target for 24h — anyone hitting them gets 50% off cast cost.
- **Coalition bounties:** require "must be hit by ≥3 different casters within X hours" for higher payout. Forces organized harassment.

### Insurance — the most satirically rich addition

Should feel like dealing with a real insurance company.

- **Tiered plans:**
  - Bronze (10% reimbursement, 5 PP/day premium) — *"Affordable peace of mind"*
  - Silver (25%, 15 PP/day) — *"For the discerning protectee"*
  - Gold (50%, 50 PP/day) — *"Premium coverage with white-glove service"*
  - Platinum (75%, 200 PP/day) — *"For the true believer"*
- **Claim denials.** ~20% auto-denied with randomized excuses:
  - *"Pre-existing pyramid condition."*
  - *"Incident outside coverage area (your wallet)."*
  - *"You failed to file Form 4-B in triplicate."*
  - *"Act of Charles."*
- **Reginald the Adjuster.** Bot persona sending condescending claim-status DMs. *"Hello [Player], I've reviewed your claim and unfortunately we'll need to escalate this to underwriting. Expected resolution time: 14 business days. Have a blessed day, Reginald."*
- **Premium hikes after claims.** Make a claim → your premium goes up next renewal.
- **Cancellation fees.** Want to drop your plan? 500 PP, please.

Premiums = real ongoing PP sink to Charles.

---

## Spitballed ideas — file for later

Not committed. Re-evaluate when v2 or beyond.

### Dokapon Kingdom-inspired

- **Curses (persistent debuffs).** Multi-day debuffs. *"Cursed by Beady: −5% PP earnings, 7 days remaining."* Stackable. Removable via buyout offer.
- **Random market events.** Game-wide events that screw players over: *"SEC investigation triggered. All deposits frozen for 24h."* *"Audit complete: 5% of everyone's PP reclassified (burned)."*
- **Banter dialogue on every action.** When you cast a spell or get hit, a dialogue popup attributed to the relevant player appears.

### Other
- **Dynamic cast odds based on history:** failed casts increase next cast's odds; successful casts decrease them. Streak-based pity timer.
- **Reciprocal rubber-banding on hostile shenanigans:** if you've been hit recently, your offensive odds go up; if you've been hitting others, your odds go down.
- **Position-holding PP tax:** small ongoing PP cost per open position per day. Passive sink. Only consider if v2's Harberger isn't enough.

---

## Docs to update when v1 ships

`frontend/src/components/DocsPage.tsx`:
- "The Pyramid (MLM)" section: rewrite for recursive model. Update the table showing L1=8% / L2=5% / L3=2%.
- "Ponzi Points (PP)" section: update mint rates (200/400/600/800 instead of 1000/2000/3000/4000). Add signup gift to PP-earning activities.
- "Glossary" entry for Downline: update math description.
- Possibly add a new section: "Charles, your eternal upline" — explaining the default referrer mechanic. Tone match: VC/MLM voice.
- "Shenanigans" section: update payoff caps for the 5 active spells; note that the others are "in development."

Drafts to be reviewed by user before merging.

---

## Implementation plan — Option 2 interleave

Estimated total: ~8–14 hours of focused work, spread across 1–3 sessions of back-and-forth.

### Phase A — Foundation (backend)
1. Add `referrerToDownline` reverse index map in `shenanigans/main.mo`. Stable variable, populated on `registerReferral`.
2. Backfill the reverse index from existing `referralChain` at upgrade time.
3. Add `getPayoutTarget(downliner: Principal): Principal` helper. v1: returns `referralChain.get(downliner) ?? CHARLES_PRINCIPAL`. v2 will swap implementation.
4. Add `CHARLES_PRINCIPAL` constant. **Open question: which principal is Charles?** Need confirmation before locking.
5. Add activity check: `isActive(principal: Principal): Bool`. Returns true if principal has ever deposited ≥ 0.1 ICP. Reads from ponzi_math.

### Phase B — Cascade rewrite
1. Replace `cascadeReferralMint` with a recursive walk: compute 10% of base, walk upline via `getPayoutTarget`, skip inactives, pay each active upline 50% of received cascade, pass remaining 50% up. Cap depth at 10 (safety). Residual to Charles.
2. Replace `setReferralBps` admin call with `setCascadeBps(initial, passthrough)`.
3. Lower mint rates per the table above.

### Phase C — Signup gift
1. Add `signupGiftPp` config constant (default 500).
2. On first deposit event in `processNewGames` (detect via "player has no prior game records"), mint signup gift to player's chip subaccount before applying cascade.
3. Pre-compute signup gift breakdown for the receipt — cascade simulation result, returned as a query.

### Phase D — Query / UI (frontend + backend)
1. Add `getReferralStats(player: Principal): { l1Count; l2Count; l3Count; l1Pp; l2Pp; l3Pp; recentSignups: [Principal] }` query method on shenanigans canister, using reverse index.
2. Replace stubbed `useGetReferralStats` in `frontend/src/hooks/useQueries.ts` with a real `useQuery` calling the new method.
3. Add toast (with confetti) on MLM tab when downline grows since last visit. Persist "last seen" timestamp in localStorage.
4. Add cascade visualization receipt component shown after first deposit (the "where your 500 PP signup gift cascades" panel).

### Phase E — Shenanigan effects (5 spells)
Write the body of the corresponding success branch in `castShenanigan` for each. Reuse existing `chipTransfer`, `burnFrom`, `mintInternal` primitives.

1. **Money Trickster (id=0):** roll 2–8% target chip balance, cap 500 PP, `chipTransfer(target, caster)`.
2. **AOE Skim (id=1):** iterate active players, roll 1–3% per player, cap 100/player, `chipTransfer(player, caster)`.
3. **Purse Cutter (id=7):** roll 25–50% target chip balance, cap 1,500 PP, `burnFrom(target)`.
4. **Whale Rebalance (id=8):** query top 3 chip-balance holders, roll 20% each, cap 500/whale, `chipTransfer(whale, caster)`.
5. **Magic Mirror (id=5):** add `magicMirrorShields: Map<Principal, Nat>` state. Increment on cast. Decrement and short-circuit on hostile cast hitting target with count > 0.

### Phase F — Rubber-banding modifier
1. Add `rubberBandOddsMod(caster, target): Int` helper. Computes caster vs. target PP ratio, returns +25 / 0 / −25.
2. Apply in `castShenanigan` before rolling the d100. Clamp final success rate to [5, 95].

### Phase G — Cap updates + smoke tests
1. Update `effectValues` in `initializeDefaultShenanigans` for the 5 spells.
2. Smoke-test each spell against various target wealth levels. Verify burns happen, cascade flows correctly, rubber-banding kicks in.

### Phase H — Docs update
1. Draft updated DocsPage.tsx sections per the list above.
2. User reviews tone.
3. Merge.

---

## Open questions before starting

1. **Which principal is Charles?** Need a constant for `CHARLES_PRINCIPAL`. Should be a real admin-controlled identity.
2. **Backfill or document-only?** When `registerReferral` is called *after* a deposit, do we backfill the missed cascade payout, or just document loudly?
3. **Activity bar definition:** "≥0.1 ICP deposited ever" is the v1 proposal. Should it tighten over time (e.g., "in last 30 days")?
