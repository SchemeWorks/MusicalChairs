# Solana Port — Premortem Transcript

**Date:** 2026-05-23
**Premortemed:** [2026-05-23-solana-port-design.md](2026-05-23-solana-port-design.md)
**Framing:** It's November 2026. The Solana port shipped six months ago. It failed. We're looking back to explain why.

---

## Context

The plan being premortemed:
- Port Musical Chairs' `ponzi_math` canister (Motoko, IC) to a Solana Anchor program using native SOL
- Build an IC indexer canister with HTTPS outcalls that mirrors Solana program events for historical queries
- Adapt the existing React frontend with a chain-aware `PonziService` interface; users connect either Internet Identity (IC game) or Phantom (Solana game)
- Two games run independently — separate pools, separate users, no bridge
- Shenanigans / Ponzi Points / NFTs stay II-bound
- Solo/small-team operator

**Success criteria:** Solana flavor ships, attracts Phantom users, runs 6+ months reliably, no exploit, no major downtime, no math parity scandal, doesn't burn out the maintainer.

**V1 simplifications baked into the spec:**
- Native SOL only (no stablecoin)
- Slot-hash RNG for Series B promotion (deferred Switchboard VRF to V2)
- Dropped oldest-Series-A 35% bonus (deferred to V2)
- Pull-model reward distribution (MasterChef pattern)
- Off-chain crank for round reset
- Helius as primary Solana RPC provider
- Anchor framework

---

## Failure modes identified (9)

1. Math parity drift — IC vs Solana payouts diverge subtly
2. Audience mismatch — Phantom degens vs satirical-VC theme
3. MEV bots + Series B slot-hash exploitation
4. Indexer infrastructure failure (cycle drain + RPC SPOF)
5. Frontend adapter abstraction leaks
6. Phantom UX friction kills retention
7. Audit unavailable or refused
8. Maintainer burnout from two stacks
9. Cold-start liquidity failure

---

## Deep analyses

### 1. Math parity drift

**THE FAILURE STORY**

Launch week: Solana flavor goes live. Same UI labels — "Compounding 15-Day @ 12% daily", "12%/7.5%/3% Carried Interest tiers", same Front-End Load. Charles tweets "same game, pick your wallet." Whales test both sides with matched 1.0 SOL / 1.0 ICP deposits on identical plans within minutes of each other.

A degen named SOLBRO on day 8 settles a 5 SOL compounding15Day position on Solana and posts the result next to an II user's matched compounding15Day on IC. The Solana side paid him exactly `amount × ROI_COMP15_Q64` at maturity. The IC side paid him `amount × (1.12^15.04 - 1)` rounded to 8 decimals, because Motoko's `Float.pow` runs against fractional `daysElapsed` and the user happened to settle 56 minutes after the 15-day mark. The IC payout was 0.6% larger. SOLBRO calls it a scam in the Telegram. A backer on the Solana side claiming carried-interest yield via the pull-model `reward_index_q64` notices their accrual is off the IC version by another 0.3% because the IC code splits 35/25/40 across oldest/other/all backers while V1 Solana flattens to a single weight-proportional share — a spec-acknowledged difference that nobody internalized.

Within two weeks the parity test suite (the one listed as the mitigation in the Risks section) ships, and it confirms what users already knew: dozens of small drifts, all in the IC version's favor for matured compounding positions, all in the Solana version's favor for early simple-plan withdrawals (because `roundToEightDecimals` truncates downward on IC, and Solana lamports don't). Charles tries to explain "lamports are more precise, ICP rounds down." Nobody believes him. The framing inverts from "same game, two chains" to "Charles runs two slightly different scams and hopes you don't notice which side he's tilted." Deposits on both sides freeze.

**THE UNDERLYING ASSUMPTION**

That "same game rules" survives translation between Float-with-runtime-rounding and integer-fixed-point-at-maturity-only — when the IC version's payouts are themselves an emergent product of `Float.pow` on fractional `daysElapsed` plus `roundToEightDecimals` at every step, not a clean closed form anyone can reproduce.

**EARLY WARNING SIGNS**

- The parity test suite is "TODO" past Phase 5. It's listed as a mitigation under Risks but isn't a Phase 1 deliverable next to the fixed-point math itself. If parity isn't a gating exit criterion for the LiteSVM tests in Phase 1, the divergence ships.
- `adminGetActivePlansSnapshot` shows different numbers on the two flavors at matched inputs in devnet. The spec already says this snapshot is "computed client-side with full float precision" on Solana — which means the in-flight ROI displayed for a Solana comp15 position will differ from the IC one even before maturity, visible to anyone who opens both dashboards side-by-side during the Phase 4 bug bash.

---

### 2. Audience mismatch (degen vs satire)

**THE FAILURE STORY**

The Phantom launch went live in late June 2026 and got a respectable first-week bump from a co-marketing push. Then the cohorts diverged hard. About 40% of new wallets were the "literalist" segment: degens who skimmed the landing page, deposited 0.5-2 SOL on the implicit promise of "early gains," lost to the Carried Interest curve, and immediately took to Twitter calling it a rug. The "Front-End Load" copy got screenshotted next to the contract address with captions like "they literally TELL you they're skimming and people still deposit." It read as an own, not a joke. By August, three mid-tier Solana alpha accounts had pinned threads warning followers off, and the protocol's Telegram filled with refund demands the team couldn't honor.

The second cohort — the sophisticates — was worse in a quieter way. Within two weeks, a Jito-adjacent quant group published a Dune dashboard reverse-engineering the tier schedule, posted optimal entry/exit timing as a public Jupyter notebook, and ran a bot farm that extracted ~28% of net inflows during the August window. The carefully-tuned 12%/7.5%/3% week schedule became a solved game. Honest players watched their "Series A" returns get front-run by MEV-style coordinated exits.

Meanwhile Pump.fun was minting 10,000 tokens a day and BONK derivatives offered 50x leverage. The satirical-MLM frame that felt clever to ICP natives read as low-energy LARP next to actual on-chain casino energy. Volume bled to <5% of ICP daily by October. The cross-chain unified frontend made the contrast worse — Phantom users saw the IC chat/shenanigans social layer they were locked out of and assumed the "real" product was elsewhere.

**THE UNDERLYING ASSUMPTION**

That a self-aware satirical frame travels across crypto cultures — when in fact the joke requires an in-group that trusts the protocol's intent before reading the copy.

**EARLY WARNING SIGNS**

- Week 1-2: Ratio of "is this a scam?" replies to satirical engagement on the Solana launch tweet exceeds 3:1 (vs. <0.5:1 for IC launch posts).
- Week 2-4: Any public Dune dashboard or Twitter thread reverse-engineering the tier schedule with optimal-exit math gets >50 likes — signals the sophisticate cohort is treating it as an extractable system, not a game.

---

### 3. MEV bots + Series B exploitation

**THE FAILURE STORY**

By July 2026 a Jito searcher named "chair-sniper" had written a 200-line Rust bot that monitored the `Pool` PDA's lamport balance via Geyser. The moment a withdrawal pushed the pot under 0.05 SOL of rent-exemption, the bot raced to land `trigger_reset` in the very next slot — submitting the same instruction at 50ms intervals across 12 different fee-payer wallets. Because the program lets the cranker submit *any* of the 512 recent SlotHashes plus *any* valid underwater player as the promotee, the bot simply iterated all 512 hashes locally, checked which `hash(slot_hash || sockpuppet_pubkey) % candidate_count` index pointed at one of its 30 pre-seeded sockpuppet "losses," and submitted that combination. Series B went to one of chair-sniper's wallets in 41 of the next 47 round resets.

The sockpuppet farm cost ~0.3 SOL to seed (30 wallets × 0.01 SOL min deposit, all left to go underwater on `simple21` plans). Each Series B promotion paid a 1.16× recovery on the loss, harvested over the next round's toll stream — net positive on every reset. By September, on-chain explorers showed three wallet clusters that had won 89% of all Series B promotions ever granted. A Twitter thread titled "Musical Chairs is a bot farm now" hit 400K views. The sandwich vector compounded the bleed: searchers watched the mempool for `withdraw_earnings` calls that would tip the pot into partial-payout territory, then front-ran with their own `withdraw_earnings` at the still-100% ratio, leaving the victim with the scaled-down payout. New deposits dropped 70% week-over-week. By October the only addresses still depositing were the searchers themselves, recycling SOL through the bonus loop. Pot died.

**THE UNDERLYING ASSUMPTION**

"The prize is small so the incentive to attack is low" — but the *prize* doesn't have to be large when the *marginal cost of attacking is near-zero* (sockpuppet seed dust + Jito fees) and the attack is fully automatable across infinite rounds.

**EARLY WARNING SIGNS**

- The same 5-10 wallets cranking `trigger_reset` within the same slot as the triggering withdrawal, every round. (Healthy pattern: random humans cranking minutes later.)
- A cluster of identical underwater `simple21` games, all deposited at the protocol minimum, all created within the same hour, all by addresses that never withdraw — sitting dormant waiting to be "selected."

---

### 4. Indexer infrastructure failure

**THE FAILURE STORY**

October 14, 2026, 2:47 AM ET. A Twitter influencer posts the Solana game as "Phantom-degen heaven." Within 90 minutes, 4,200 wallets pile in. The Anchor program handles it fine — Solana's serializer doesn't care. The indexer does not. At 60s polling, each poll now pulls 200+ new signatures, each requiring a `getTransaction` outcall. On a 13-node subnet that's 13 real HTTP hits per call, and Helius rate-limits us to 429s by 3:15 AM. Outcalls start failing "no consensus" because the throttled replicas get different error bodies than the successful ones. The bookmark stops advancing.

By morning the indexer is 9 hours behind. Frontend history tab shows "no recent events." The Solana subreddit screenshots a withdrawal that "disappeared." Trust evaporates in a day. We scramble to ship a Triton failover — except we never wrote provider-agnostic parsing because Helius response shapes were hardcoded. Three days of triage. Meanwhile cycle burn during the spike hit ~14T/day (~$20/day), 4x projection, and the canister freezes on day 5 because the CycleOps alert threshold was set against the $5/day "active" estimate, not viral-spike reality.

By November, "the Solana version is broken" is the dominant sentiment. Phantom DAU drops 80%. We quietly stop promoting it.

**THE UNDERLYING ASSUMPTION**

We treated the indexer as plumbing — a passive mirror — when it is actually a load-bearing piece of user-visible infrastructure with its own SLA, attack surface, and cost curve that scales nonlinearly with on-chain activity.

**EARLY WARNING SIGNS**

- Indexer lag (`secondsSinceLastPoll` already in `getIndexerHealth`) — surface this in the frontend header from day one. If lag ever exceeds 5 minutes in non-spike conditions, the architecture is wrong.
- Cycle burn rate per 1000 signatures processed — track cycles-per-event as a unit cost. The moment it exits the ~3M/event band (i.e., consensus retries kick in), you have a Helius reliability problem days before it becomes a freeze.

---

### 5. Frontend adapter abstraction leaks

**THE FAILURE STORY**

Phase 4 kicked off in July expecting the 1-2 week "wrap existing actor behind `PonziService`" refactor. Week 1 hit the first leak: `useCreateGame` already branched on `walletType === 'oisy'` for sequential approve+createGame because batched ICRC-112 broke. Solana didn't need approve at all — Phantom signs the program instruction directly — so the deposit flow couldn't share its `try/catch` shape or error string parsing (`error.message?.includes('Minimum deposit is 0.1 ICP')`). The handler split in two. Two days later the *mutation onSuccess invalidation list* — six queryKeys, four refetches — was discovered to be useless for Solana because `getUserGames` reads directly from RPC, not from a query that needed invalidating; Phantom confirmation came back in 600ms and the user saw stale balances for 5 seconds while React Query waited on the 5s `refetchInterval`. The team added a chain-aware `refetch-after-confirm` layer, then realized every mutation needed it.

By week 3 the leaks had spread: `useCheckDepositRateLimit` had no Solana analog (it lives on the program PDA, not a query), the Charles admin chip blew up because `isCharles()` compared Principal text and Phantom users have base58 pubkeys, error toasts showed "Error 6001" instead of "Minimum deposit" because Anchor errors are u32 codes, `PendingQueueCard` assumed II-style 2-second update latency and showed a stuck spinner during Phantom popups, and the BridgeCard onboarding ICP-balance flow had no SOL equivalent. Phase 4 finished in November, four months late. Phase 5 audit slipped, the mainnet launch missed the planned window, and to ship at all the team disabled half the polish (live earnings counter, optimistic toasts, rate-limit pre-check) on the Solana path. The Solana flavor went out feeling like a degraded prototype next to the IC version.

**THE UNDERLYING ASSUMPTION**

That two chains with fundamentally different transaction lifecycles, error vocabularies, and authentication models can share a single async-call-based interface without forcing every UI component that consumes timing, errors, or auth identity to branch internally.

**EARLY WARNING SIGNS**

- Count of `walletType === 'oisy'` / chain-specific branches inside `useQueries.ts` mutations crossing ~3 within the first week of Phase 4 (the existing Oisy branching is already a leading indicator; Solana will compound it).
- Number of `useQuery` hooks whose `onSuccess` invalidation list is empty or different on Solana climbing past ~5. Once invalidation becomes chain-conditional, the "shared component" claim is already false.

---

### 6. Phantom UX friction kills retention

**THE FAILURE STORY**

Launch week looked fine — 1,200 unique wallets, healthy curiosity inflows, the Solana DeFi crowd kicking the tires. Then the curve cratered. D1 retention came in at 38% (vs. 71% on IC). D7 hit 11% (vs. 44%). D30 was 3%. Sessions-per-DAU collapsed from 4.2 on day one to 1.6 by week two — users were doing one popup-burdened action per visit and leaving. The shape wasn't a slow bleed; it was a cliff at day 2-3, exactly when the novelty of "try the weird ponzi game" wore off and the friction tax became the dominant signal.

Discord and the Solana subreddit told the story plainly: "why am I signing 5 times to play a game", "phantom popup hell", "fun concept, brutal UX". Power users who understood the compounding plan and wanted to settle/restake daily quit fastest — they were the ones hitting 6-8 popups per session. The casual deposit-and-forget crowd stuck around longer but never re-engaged, which meant no compounding-plan velocity, which meant the pool stayed at $180K TVL versus IC's $2.1M. Carried-interest revenue never crossed the threshold to make Charles's cut meaningful. By September the Solana flavor was a ghost town with a working program, and by November we quietly stopped marketing it.

Meanwhile the IC version's delegated-auth micro-loop kept humming — same game, same math, dramatically different session length and return frequency.

**THE UNDERLYING ASSUMPTION**

We assumed Solana DeFi users' tolerance for popup-per-action friction (proven in swap/mint contexts with 1-2 txs per session) would transfer to a game designed around dozens of micro-decisions per session — when in fact that friction was the entire reason no one had built a fast-loop game on Solana before.

**EARLY WARNING SIGNS**

- D1 retention <50% within the first launch cohort (IC baseline ~70%) — visible within 48 hours of opening signups, well before TVL trends matter.
- Sessions-per-DAU dropping below 2.0 by day 3 combined with median session length under 90 seconds — users are doing one action and bouncing, the exact pattern of friction-driven abandonment.

---

### 7. Audit unavailable or refused

**THE FAILURE STORY**

We sent the spec to OtterSec and Zellic in early July 2026. OtterSec passed within a week — the lead noted internally that their report would be public on their site and they don't want "ponzi-themed" branded engagements regardless of the satirical framing; their reputation with institutional clients (Solana Foundation grant recipients, exchanges) made it a non-starter. Zellic ghosted after the intro call once the scoping doc made it past their BD person to a partner. Halborn quoted $48K with a queue starting in October — three months past the launch window. Neodyme would've been a fit but had a 5-month backlog from the Jito and Marginfi follow-on work.

We pivoted to mid-tier (Ackee, Offside) in August. Ackee quoted $22K with availability in 6 weeks, but their reviewer flagged the slot-hash RNG and pull-model reward index as "needs second pass" — implying another round and another $10K. The operator, burning cycles on parallel IC work, made the call to ship to devnet with a self-review and a public bug bounty as the "audit substitute." A Q&A thread on Solana subreddit caught the `claim_rewards` math drifting on edge cases (fixed-point rounding accumulating in `reward_index`) two weeks after mainnet. Refunds were issued from the admin escape hatch, which itself became the next narrative problem. Momentum died. The IC version meanwhile shipped V2 NFT cascades and the spec went stale.

**THE UNDERLYING ASSUMPTION**

That auditor selection is a capacity-and-budget problem, not a brand-acceptance problem — that any firm with a slot would take the engagement if the money was there.

**EARLY WARNING SIGNS**

- First outreach email to a top-tier auditor goes unanswered for 10+ business days, or the intro call ends with "let me check internally and get back to you" followed by silence — that's the brand-risk filter killing it quietly, not a scheduling issue.
- The scoping doc requires you to soften "ponzi" to "yield game" before sending — if you're already euphemizing for the auditor, the auditor will euphemize the decline.

---

### 8. Maintainer burnout from two stacks

**THE FAILURE STORY**

**June 2026 (Month 1 post-launch):** The Solana port ships. The first dual-stack bug lands two weeks in — carried interest tier math is off-by-one on the Solana side because the Anchor program's `Clock::get()` timestamp is in seconds, not nanoseconds, and Robert forgot to divide. He fixes it in Rust at midnight, then has to mirror the fix in Motoko anyway because the tier schedule was wrong in shared docs. Two PRs, two test suites, two deploys, two announcements. He stays up until 3am.

**August 2026 (Month 3):** Robert ships a new Shenanigan spell. It's joyful work — Motoko persistent actor migrations feel clean, the IC version goes out in an afternoon. The Solana port requires a borsh schema change, a program upgrade with the upgrade authority, a re-bumping of the indexer canister, and breaks two integration tests. It takes four days. He doesn't ship the spell to Solana. Players on Solana start asking why. He says "soon." A pattern starts: IC features ship same-week, Solana features ship "when I get to it."

**October 2026 (Month 5):** Robert tweets "honestly, the IC version is just more fun." A Solana player notices the carried interest tier was retuned on IC but not Solana — a real economic divergence affecting their withdrawal. They post angrily. Robert spends a weekend backporting the change, hates every minute of it. Two weeks later he posts the wind-down: "Solana version entering maintenance mode. No new features. Withdraw your positions by Dec 31." Trust on both sides erodes. The IC community wonders what else might get sunset.

**THE UNDERLYING ASSUMPTION**

That a solo operator who already complained about Solana's limitations during design would somehow find renewed enthusiasm for maintaining it once shipping reality replaced theoretical novelty.

**EARLY WARNING SIGNS**

- Commit cadence divergence by Month 2: IC commits outpace Solana commits 3:1, with Solana commits clustered on weekends ("forced catch-up" pattern) rather than distributed across the week.
- Bug-report response latency by Month 3: IC issues get a same-day reply, Solana issues sit 48-72 hours before Robert engages — and his Solana replies become noticeably terser, more often closing with "tracking, will fix" rather than an actual fix.

---

### 9. Cold-start liquidity failure

**THE FAILURE STORY**

Launch week (June 2026): 47 unique Phantom wallets connect after a Twitter/X push and a Solana subreddit post. Median deposit is 8 USDC ($1.50 above the $6 front-end-load break-even). Within 72 hours, 31 of those wallets initiate Carried Interest withdrawals at the Week 1 tier (12% toll). The dashboard shows a pool TVL graph that pumps to $4,200 on day 2, sags to $1,900 by day 5 as withdrawals outpace deposits, and never recovers. Daily active wallets settle at 3-6 by week 4, mostly the operator's own test addresses and two crypto-Twitter friends who promised to "check it out."

The Discord (created day -3) gets one substantive question — "where does the yield come from?" — and after the honest answer ("from later depositors, it's a ponzi") the channel goes silent. Withdrawal screenshots posted by skeptics frame the 4% front-end load as "rugged on entry." Nobody self-identifies as a Series A backer because the Solana landing page describes it as "seed the pool with 100+ USDC for a 24% bonus entitlement payable when the pool reaches solvency" — which to a Phantom user reads as "lock up real money in a stranger's empty ponzi for an IOU." Zero Series A commitments by month 2. By November, TVL is ~$5K across 43 wallets, of which the operator controls 11.

**THE UNDERLYING ASSUMPTION**

That Solana degens would self-bootstrap a ponzi out of pure financial curiosity, the way they do memecoins — ignoring that memecoins have a liquid secondary market and exit, while this game requires *new depositors* (not buyers) to generate yield.

**EARLY WARNING SIGNS**

- Week 1: Withdrawal-initiations / new-deposits ratio > 0.5 within 72 hours of launch. (Healthy bootstrap needs <0.2.)
- Week 2: Zero Series A backer commitments despite >100 wallet connects — meaning nobody trusts the pool enough to be the patient capital, so the pool will never grow past speculator churn.

---

## Synthesis

### The Most Likely Failure

**Operator burnout (#8)** — and unusually for a premortem, the early warning signs are already present in this room. During the design discussion the operator stated outright that Motoko "sounds much more elegant and sophisticated," called the Solana workarounds "weird," said it "kills me" to walk away from the oldest-35% rule. Pre-existing disposition + dual-stack maintenance is the canonical recipe for atrophy.

Close second: **cold-start liquidity (#9)**. The IC bootstrap mechanisms (PP, NFT cascade, shenanigans, community pressure) are explicitly dropped on the Solana side, and Solana audiences don't self-bootstrap ponzis the way they self-bootstrap memecoins (the latter has a liquid secondary market and an exit; this game has neither).

### The Most Dangerous Failure

**MEV bots + Series B slot-hash exploitation (#3)**. Three reasons this is worst-case:
1. Silent — no obvious "broken" state, just unfair outcomes accumulating
2. Cross-chain reputation damage — Twitter doesn't distinguish "Solana version exploited" from "Musical Chairs exploited"
3. The fix requires either an audit (blocked by #7) or Switchboard VRF (more work the burned-out operator won't want)

Math parity drift (#1) is comparable in damage — both attack the "fairness" pillar that holds the entire game together. But MEV is harder to walk back once headlines exist.

### The Hidden Assumption

Across all 9 failures, the deepest unexamined assumption is:

> **"This is a port — same product, same audience, different plumbing."**

But it's not a port. It's a **new product launched in a new market under a familiar name**. Different audience (Phantom degens vs ICP natives), different competitive set (Pump.fun vs nothing comparable on IC), different distribution channel (Twitter/Solana subreddit vs IC community), different operational model (dual stack vs single stack), different trust assumptions (anonymous wallet sigs vs II delegations).

The spec treats it as a technical undertaking. The failure modes are almost all **product / market / operator** failures, not technical ones. The Anchor program will probably work fine. What might not work is everything around it.

### The Revised Plan

**1. Validate the audience BEFORE building.** Run a 2-week customer-development sprint *before* writing Rust. Concrete: spin up a fake landing page with Phantom connect, run $500 in paid traffic, measure intent-to-deposit conversion and sentiment. If "is this a scam?" replies exceed 3:1 vs satirical engagement, kill the project. Two weeks beats fourteen weeks of building toward nothing.

**2. Promote the parity test suite from Phase 5 mitigation to Phase 1 gating criterion.** Refuse to merge any economics code without a passing golden-vector test against the Motoko implementation, with the tolerance band publicly documented in advance. If you can't define the tolerance band, you can't ship.

**3. Ship Switchboard VRF from day one for Series B selection.** The "V1 is fine because the prize is small" reasoning died in failure mode #3. Add 1-2 weeks and pay the per-call cost. Cheaper than a 400K-view exploit thread.

**4. Operator commitment test at Phase 3 boundary.** Before starting Phase 4 (frontend) or Phase 5 (audit), the operator does a written self-assessment: "Am I going to want to maintain this for 18+ months?" If the honest answer is anything but yes, kill the project before sinking the frontend and audit costs. (The wind-down in the burnout scenario already implies the answer was no.)

**5. Audit outreach in Phase 0, not Phase 5.** Email 5+ Solana auditors with the spec and the satirical framing visible during the design phase. If brand-risk decline is the pattern, discover it in week 1, not week 11. If decline is universal, the project's positioning needs to change before any code is written.

**6. Indexer multi-RPC failover and viral-spike load test as launch-blockers.** Provider-agnostic parser. Helius + Triton as default redundancy. Simulated 1000-tx-in-5-min stress test before mainnet. CycleOps alert thresholds set against viral-spike worst case, not active baseline.

**7. Phantom UX: collapse popups aggressively.** Use Solana's transaction composition and ICRC-112-equivalent batching (Lighthouse assertion accounts) to do multi-step flows in one popup. Target ≤1 popup per *session*, not per action. Test the friction-collapsed UX against IC retention numbers in pilot before committing.

**8. Cold-start liquidity bootstrap is mandatory, not optional.** Either: (a) operator seeds Pool with X SOL of house liquidity at launch, (b) first-N-depositors get a small bonus, (c) cross-promo with a Solana protocol with overlapping audience, or (d) don't launch until 100+ committed pre-launch wallets exist (whitelist + waitlist). Picking none of these is picking failure mode #9.

### The Pre-Launch Checklist

Five gates that must pass before mainnet:

1. **Audience pilot complete.** $500 in paid traffic to a Phantom-connect landing page. Intent-to-deposit conversion measured. "Is this a scam?" / satirical engagement ratio <2:1.
2. **Parity test suite green.** 1000+ golden test vectors comparing IC vs Solana implementations within explicit ε tolerance. Tolerance band publicly disclosed in advance.
3. **At least 2 reputable auditors quoted in writing.** Not "let me check internally." Written engagement quotes confirming the brand-risk filter wasn't activated.
4. **Indexer viral-spike test passed.** 1000+ tx in 5 min simulation: no consensus failures, lag <60s recovery, cycle burn within 3x baseline, automatic failover from Helius → Triton verified.
5. **Operator self-assessment yes.** Written, dated commitment to 18+ months of maintenance, signed before Phase 5 audit dollars are spent.

If any of these fails, the project shouldn't ship in its current form. That's a feature of the checklist, not a bug.
