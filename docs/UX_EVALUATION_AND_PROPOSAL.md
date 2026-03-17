# Musical Chairs — UX Evaluation Report

*Prepared: February 2026*
*Perspective: Obsessive web/mobile game UX designer*

---

## Executive Summary

Musical Chairs has strong bones — a confident dark theme, a coherent visual language, self-aware humor, and a functional tab-based SPA. But it suffers from the same thing most dev-built UIs do: **it was assembled component-by-component rather than designed experience-by-experience.** The result is a product where every screen *works* but few screens *feel* great. Information architecture is flat, critical user journeys have unnecessary friction, and the visual density is inconsistent — some pages are breathing, others are drowning.

If I were designing this from scratch, I'd start from three principles: **show less, do more, feel better.**

---

## 1. First Contact: Splash / Landing Page

### What's There
A big gradient logo, tagline, login button, three info cards, and "Built on Internet Computer" branding.

### What's Wrong

**The page is vertically static.** There's no motion, no storytelling, no building tension. For a game that's literally about reckless gambling, the landing page feels like a government form. The three info cards (Pitch, Catch, Twist) are laid out as equals — but they shouldn't be. The Pitch is the hook, the Catch is the friction, the Twist is the payoff. They need **dramatic pacing**, not a uniform grid.

**The CTA is stranded.** The login button sits in a void between the logo and the info cards (`mb-16`). That's 64px of dead air between your tagline and your only call to action. On mobile, users have to scroll past it to even understand what the game is.

**No social proof, no urgency.** There are zero live stats visible (pot size, active players, latest payout). In game UX, the first thing a new player should see is *other people winning*. It creates FOMO. Right now the landing page is saying "trust us" when it should be saying "look at this."

### If I Built It From Scratch

- Hero would scroll-animate: logo fades in with scale, tagline types itself out, then the three cards stagger in below — building the narrative: seduction, warning, hook.
- Live pot stats and recent payouts would appear above the fold, BEFORE the login button. Create urgency.
- The login CTA would be **after** the info cards, not before. Let the pitch land, *then* ask for commitment.
- A subtle animated background (particles, slow gradient shifts, or a floating chair icon) to give the page life.

---

## 2. Onboarding: Profile Setup

### What's There
A centered card with a name input and "JOIN THE GAME" button.

### What's Wrong

**It's a dead end.** There's no context. The user just authenticated with a wallet and now they're on a blank screen with "Pick a name." No explanation of what the name is for, no preview of what's coming, no excitement.

**No character.** This is literally the moment someone joins a gambling game and the UI treats it like a JIRA account creation form. Where's the drama? Where's the tension?

### If I Built It From Scratch

- The name input would be styled as a "casino registration desk" — some atmospheric illustration or animated element that makes this feel like walking up to a table.
- Below the input, show a real-time preview: "Players will see you as: **[typed name]**"
- After submit, don't just redirect — show a momentary "welcome aboard" celebration screen with confetti and a brief tour prompt.
- Add name validation feedback *while typing* (character limit, profanity check indication).

---

## 3. Navigation: Desktop Rail + Mobile Tabs

### What's There
7 nav items across three groups (core/extras/fun). Desktop gets a collapsible left rail. Mobile gets 4 bottom tabs + a "More" overflow sheet.

### What's Wrong

**The rail collapses too aggressively.** At 64px collapsed, the icons are floating without labels. The expand-on-hover behavior is clever but creates a "mystery meat" navigation — users have to hover to discover what things are. First-time users won't know Dice5 = Shenanigans.

**The mobile "More" sheet is a UX graveyard.** Rewards, MLM, and Hall of Fame are buried behind a generic "More" button. These are engagement drivers — especially Referrals/MLM for growth. Hiding them ensures low discovery rates. Also, there's a **duplicate icon rendering bug** in the More sheet (lines 184-185 of `Dashboard.tsx` render `{item.icon}` twice).

**7 tabs is too many.** Mobile game UX research consistently shows 4-5 is the max before cognitive overload. The current tabs conflate *actions* (Pick Your Plan) with *views* (Profit Center) with *features* (Shenanigans). They need conceptual regrouping.

**No visual distinction between "where I am" and "what I should do."** The active state is just a border glow. In game UX, your primary loop tab should have persistent badge indicators (unread, earnings available, etc.).

### If I Built It From Scratch

- Reduce to 5 tabs max. Merge "Rewards" into the Profit Center as a sub-section. Merge "Hall of Fame" into Shenanigans as a sub-tab. This leaves: **Play** (positions), **Plans**, **House**, **Tricks**, **Network** (MLM).
- Desktop rail: always show labels (give it 200px), don't collapse. Screen real estate is cheap on desktop; discoverability isn't.
- Mobile: use all 5 slots with icons + labels. No More sheet needed.
- Add notification badges: red dot on Profit Center when earnings are withdrawable, purple dot on Shenanigans when you have enough PP to cast.

---

## 4. Profit Center (Positions)

### What's There
A "Running Tally" with two big numbers (deposits, earnings), then a list of position cards with withdraw buttons, plus an info card about exit tolls.

### What's Wrong

**The Running Tally tells you *what* but not *so what*.** Total Deposits and Accumulated Earnings are raw numbers. What the user actually wants to know: **Am I up or down? By how much? What's my overall ROI?** A simple net P/L number with a color indicator (green = up, red = down) would immediately answer the question every player asks first.

**Two Refresh buttons.** The tally section has one, and the positions section has another. This is confusing — are they refreshing different data? It feels like an error.

**Position cards are dense but flat.** Every card has the same visual weight. There should be visual urgency: a compounding position about to unlock should look different from a simple position that's 2 days old. Progress toward maturity is invisible — there's no timeline or progress bar.

**The exit toll info card is buried at the bottom.** This is critical decision-making information. When a user is about to withdraw, they need to see the toll *on the position card itself*, not in a separate explainer. The withdrawal dialog does show the toll — good — but the countdown to the next tier reduction should be visible at a glance on the card.

**Empty state is good.** The bouncing Dices icon + "Pick Your Plan" CTA is well done. One of the better empty states in the app.

### If I Built It From Scratch

- **Top of page: Net P/L card** — one number, huge, green or red. "+2.4 ICP (34% ROI)". That's the dopamine hit.
- Each position card gets a **thin progress bar** showing time elapsed / total plan duration.
- Current exit toll tier shown as a small badge on each card: "7% toll" in gold, with a subtle countdown.
- One Refresh button, top-right of the whole section, not duplicated.
- Sort positions: withdrawable first, then by urgency (closest to unlock).

---

## 5. Pick Your Plan (Game Setup)

### What's There
A 3-step wizard: Mode (Simple/Compounding) -> Plan Length (compounding only) -> Amount + ROI calculator + CTA.

### What's Wrong

**The wizard is good conceptually but the step numbering is dynamic and confusing.** Simple mode skips Step 2 entirely, so the amount entry becomes "Step 2." Compounding has all three steps. This inconsistency in labeling creates cognitive dissonance — "wait, did I miss something?"

**The ROI calculator is passive.** It updates via useEffect when inputs change, but there's no visual fanfare. In game UX, ROI projections should be dramatic — animated number countups, color shifts as the return gets bigger, maybe a playful "danger zone" indicator when the compound ROI gets absurd.

**The amount input has weak affordances.** The placeholder says "Min: 0.1, Max: X ICP" but these should be explicit buttons: "Min" and "Max" quick-fill buttons flanking the input. Every crypto dApp has these. Their absence feels like a gap.

**The CTA button has too many disabled states.** 8 different conditions can disable it, each with a different label (Fund Wallet First, Rate Limited, Choose Mode First, Fix Input Error, etc.). This is informative but overwhelming. A simpler pattern: always show "START GAME" but show the blocker as a red message *below* the button.

**The gambling warning is good.** Side-by-side with the CTA, in red. Prominent, honest. One of the best things in the app.

### If I Built It From Scratch

- Remove step numbers entirely. Use visual states instead: the selected mode card lights up, the plan card lights up, and the amount section appears with a smooth reveal animation. The "flow" is visual, not numerical.
- Add Min/Max amount buttons.
- ROI calculator animates: numbers roll up, the color shifts from neutral to green to "holy shit" purple as the projected return increases.
- Single CTA: "START GAME" — always enabled-looking, but if clicked with invalid state, the specific issue pulses/shakes into view.

---

## 6. House Ledger

### What's There
Two sub-tabs (Dealers/Ledger), dealer info cards, AddHouseMoney form, dealer position list, and house ledger records.

### What's Wrong

**This is the most informationally dense page and the hardest to parse.** The dealer positions page tries to do four things simultaneously: explain dealers, let you become one, show existing dealers, and display aggregate stats. Each deserves its own visual zone.

**The tab control is disconnected from the content.** Two rounded buttons floating in center — they work but they don't indicate what's inside each tab. A preview or summary stat above each tab label would help: "Dealers (3 active)" / "Ledger (47 records)".

**AddHouseMoney is buried inside a card inside the Dealers tab.** This is a revenue-critical action. It should be a prominent, always-visible CTA, not something you need to discover by scrolling the Dealers tab.

**The Dealer Info card is a wall of text.** Four dense info boxes + a large "Redistribution Event" callout. Players don't read this. The information is important but the delivery is wrong — it should be progressive disclosure. Show a one-liner summary, expand on tap/click.

### If I Built It From Scratch

- Lead with the money opportunity: "Become a Dealer -> Earn 12% guaranteed*" as a hero card with a CTA. Then show existing dealers below.
- Dealer info becomes a collapsible FAQ or a swipeable carousel of cards.
- The Redistribution Event gets its own dramatic treatment — maybe a special "danger" card that expands to reveal details, with a subtle animated flame/pulse effect.
- Ledger tab shows a proper transaction timeline with icons, amounts, and types — not just a flat list.

---

## 7. Rewards (Ponzi Points Dashboard)

### What's There
A big PP balance, breakdown by source (earned/burned/referrals), and a "How to Earn" info section.

### What's Wrong

**This page is too thin.** It's essentially a balance display + a static info card. There's nothing to *do* here. No action, no game loop. It's a dead-end information page.

**The "How to Earn" rates are buried in prose.** "1,000 PP per ICP (Simple), 2,000 PP (15-day), 3,000 PP (30-day)" — this should be a visual comparison table or graphic, not a sentence.

**No connection to spending.** Points are earned here but spent in Shenanigans — there's no bridge. A "Spend your PP" CTA linking to Shenanigans would close the loop.

### If I Built It From Scratch

- Merge this into the main Dashboard sidebar or header as a persistent balance indicator. A full page for showing a number is wasteful.
- If it stays as a page, add: recent PP activity feed, spending suggestions ("You have enough PP for Money Trickster!"), and a visual earn-rate comparison chart.

---

## 8. MLM / Referral Section

### What's There
Referral link + copy button, stats grid (direct/level 2/level 3/referral PP), and a "How the Pyramid Works" explainer.

### What's Wrong

**The referral link is not shareable enough.** Copy-to-clipboard is table stakes. Where are the share buttons? Twitter/X, Telegram, WhatsApp — these are where crypto communities live. A QR code for in-person sharing. A pre-written share message.

**Stats are numbers without context.** "Direct Referrals: 0" tells me nothing about what I should be doing. Show me a progress indicator: "Invite 3 friends to earn X bonus PP" or some kind of tier/milestone system.

**The page is too static.** No network visualization, no downline tree, no "your referrals' activity" feed. It feels like an afterthought.

### If I Built It From Scratch

- Big share CTA at top: pre-filled messages for Twitter, Telegram, WhatsApp, plus QR code.
- Visual referral tree (even simplified): see your 3 levels as a branching graphic.
- Activity feed: "Your referral [name] deposited 2 ICP — you earned 200 PP"
- Milestone badges: "Recruited 5 -> Pyramid Initiate" etc. Gamify the gamification.

---

## 9. Shenanigans

### What's There
PP balance bar, 11 shenanigan cards in a grid, odds bars, cast buttons, current round stats, live feed, guardrails section.

### What's Wrong

**This is actually the best page in the app.** The card design is atmospheric (aura effects, hover states, odds bars), the confirm dialog is appropriately dramatic, and the guardrails section is thoughtful. But:

**11 cards in a grid is overwhelming.** On desktop (3-col), that's 4 rows. On mobile (1-col), that's 11 full-width cards requiring extensive scrolling. No grouping, no categorization, no way to orient. Which shenanigans are offensive vs defensive? Cheap vs expensive?

**The odds bar is ambiguous.** Three colored segments (green/red/purple) are small and unlabeled. The numbers below help but the bar itself communicates nothing to someone who doesn't already understand the system. "Success/Fail/Backfire" labels would help.

**The live feed is undersized.** It's a `max-h-48` (192px) overflow box at the bottom. In a social/competitive game, the activity feed IS the product. It should be prominent, not a footnote. Real-time shenanigan activity from other players creates FOMO and excitement.

**Cast button text is just "Cast" — it should be contextual.** "Cast" doesn't convey what's at stake. "Cast for 500 PP" or "Risk 500 PP" would be more honest and exciting.

### If I Built It From Scratch

- Group shenanigans by tier or category (offense/defense/chaos) with filter tabs.
- The live feed becomes a right-side panel on desktop (or a prominent top section on mobile) — always visible, auto-scrolling, with animations for new entries.
- Cast buttons show the cost: "Cast (500 PP)" — no need to look at the card header.
- Add a "Popular Now" or "Trending" indicator to the most-cast shenanigan.

---

## 10. Hall of Fame

### What's There
Two side-by-side leaderboards (Top Holders, Top Burners) in a simple list layout.

### What's Wrong

**Leaderboards need drama.** The current implementation is a pair of flat lists with Medal icons. There's no trophy treatment for #1, no podium visualization, no "your rank" indicator, no animations on rank changes.

**The empty state is fine but the populated state is underwhelming.** When there ARE entries, the gold/silver/bronze styling is too subtle — just a slightly different card class. In game UX, the top 3 should feel special: bigger, with glow effects, maybe animated.

**There's no "you" indicator.** The player doesn't know where they rank unless they happen to be in the top N. A "Your Rank: #47" callout would create aspiration.

### If I Built It From Scratch

- Top 3 get a podium visual: #1 in the center (tallest), #2 left, #3 right. Animated crowns or flames.
- Below the podium, the rest of the list with "Your Rank" highlighted wherever you fall.
- Tabs or toggle between Holders/Burners.
- Add time-based filters: "This Round" / "All Time".

---

## 11. Wallet System (Dropdown + Connect Modal)

### What's There
A header "Wallet" button that toggles a positioned dropdown showing balance, deposit/withdraw, profile name edit, and internal/external balances. Wallet connect modal with II/Plug/OISY support.

### What's Wrong

**The wallet dropdown is the same for all wallet types, but it shouldn't be.** Internet Identity creates a unique principal per app, so users *must* use in-game deposit/withdraw to move ICP in and out. Plug and OISY users already have full wallets — the deposit/withdraw buttons are irrelevant for them. The dropdown should be wallet-type-aware: show deposit/withdraw for II users, show a simpler balance-only view for Plug/OISY.

**The internal vs external balance distinction is confusing.** "Internal Balance" and "External Balance" are backend concepts. Players think in terms of "My Chips" (in-game) and "My Wallet" (on-chain). Rename for clarity.

**Deposit and Withdraw are in the wallet dropdown but the actual game deposit happens in Pick Your Plan.** There are TWO places to put money in, conceptually: the wallet (fund your account) and the game (start a position). The UX doesn't make this two-step flow clear enough.

### If I Built It From Scratch

- Rename: "Game Balance" and "Wallet Balance" (or "Chips" and "Wallet").
- On mobile: wallet becomes a bottom sheet, not a dropdown.
- **Wallet-type-aware UI:** II users see deposit/withdraw/balance. Plug/OISY users see balance only (or nothing — just show their connected wallet info).
- The Max button on Pick Your Plan should pull from the user's available game balance, reflecting the pot-relative deposit cap.
- Add a visual flow indicator: "Wallet -> Game Balance -> Position" so II users understand the money path.
- The connect modal is actually solid — clean, good wallet detection, clear installed/not-installed states.

---

## 12. Trollbox

### Vision

The trollbox isn't a chat feature. It's **ambient social proof that the casino is alive.** It's the background noise of a crowded poker room — chips clinking, someone yelling about a bad beat, a stranger offering unsolicited financial advice. The moment a player opens Musical Chairs, they should *hear the room*. If the trollbox is dead, the game feels dead. If it's popping, you stay.

Inspired by the legendary btc-e trollbox: ephemeral, unfiltered, raw, and absolutely part of the product identity.

### Position & Presence

- **Desktop:** persistent right-side panel, ~300px wide, always visible on the Dashboard. Collapses to a floating bubble with unread count badge when the user wants more screen space.
- **Mobile:** floating pill/bubble in the bottom-right corner (above the tab bar), showing the latest message preview. Tap to expand into a bottom sheet overlay.

### Architecture

- A dedicated `TrollBox` canister storing the last ~200 messages in a ring buffer. Clients poll every 3-5 seconds (cheap on ICP). Messages are `{ principal, name, text, timestamp }`.
- Rate limiting: 1 message per 5 seconds per user. Forces people to make their words count.
- Max message length: 140 characters. This isn't Discord. It's a ticker tape of chaos.
- No message history beyond the buffer. When it's gone, it's gone. Ephemeral by design — like the money in the pot.

### Visual Design

- Messages render in a scrolling feed, newest at bottom, auto-scroll with a "jump to latest" pill if you scroll up.
- Username rendered in their rank color (gold/silver/bronze for top holders, purple for everyone else). Flex your leaderboard position in chat.
- **System messages** injected automatically: "[PlayerName] just deposited 5 ICP", "[PlayerName] withdrew 2.3 ICP", "[PlayerName] cast Money Trickster and BACKFIRED!" — these are the lifeblood. They make the room feel alive even when nobody's chatting.
- Input field at the bottom, monospace, with a "Send" button styled as `mc-btn-primary`. Placeholder text rotates through snarky prompts: "say something dumb...", "financial advice goes here...", "wen moon?", "this is fine."
- No emoji reactions, no threads, no DMs. Raw, unfiltered, one-channel chaos.

### Moderation

- Profanity filter: OFF. This is a trollbox, not a daycare.
- Spam filter: rate limit + character limit handles this mechanically.
- Admin can mute a principal from the admin panel. That's the only moderation tool.
- **Optional Shenanigan integration:** spend PP for trollbox powers. 100 PP to turn your next message gold. 500 PP to pin a message for 60 seconds. Creates a PP sink and makes the trollbox part of the game economy.

### What Makes It btc-e Tier

- Ephemeral messages (no permanent record)
- System event injection (deposits, withdrawals, shenanigan outcomes)
- Rank-colored usernames (social hierarchy visible)
- Tight character limit forcing wit over walls of text
- No moderation pretense — the game calls itself a Ponzi, the chat matches the energy
- Rotating snarky placeholder text
- The name "Trollbox" displayed proudly, no corporate euphemism

---

## 13. Cross-Cutting Issues

### Typography Hierarchy is Inconsistent
Some sections use `font-display` (Bungee) for headers, others use `font-bold` (Space Mono). The tagline uses Fredoka One. This three-font system is expressive but inconsistently applied. Every section header should follow the same pattern.

### Information Density Varies Wildly
Profit Center and House Ledger are dense. Rewards and MLM are sparse. This creates an uneven experience — some tabs feel packed, others feel empty. Each page should have roughly the same amount of content density, padded with contextual actions where light.

### No Persistent Game State Indicator
There's no always-visible indicator of: current round status, pot size, time remaining, or your total P/L. In a game this dynamic, these numbers should be in the header or a persistent sub-header bar.

### Mobile Experience is an Afterthought
The responsive CSS exists but the design was clearly built desktop-first. Key issues:
- Two-column grids that stack to single-column lose their comparative value.
- The rail-to-bottom-tab transition loses the group dividers and the hover-expand discoverability.
- Large padding/spacing values designed for desktop create excessive scrolling on mobile.
- No pull-to-refresh anywhere (standard mobile game pattern).

### Animations Are Decorative, Not Functional
The hover effects, glow states, and aura pulses look great but they don't communicate *information*. Functional animation should: indicate loading progress, confirm actions, show state changes, draw attention to what matters. The confetti is the one good example — it's a reward signal. Everything else is ambient.

### No Onboarding Tour
A brand new user sees 7 tabs with zero guidance. No "here's how to play" flow, no tooltips, no progressive reveal. The three splash page cards are the only introduction, and they're forgotten the moment you log in.

---

## 14. The One-Liner Priorities

If I could change 10 things tomorrow, in order:

1. **Add a persistent game status bar** below the header: pot size, your P/L, round status.
2. **Reduce tabs to 5**, merge Rewards into Dashboard, merge HoF into Shenanigans.
3. **Fix the duplicate icon bug** in the Mobile More sheet.
4. **Add Min/Max buttons** to all amount inputs.
5. **Show net P/L** (not just deposits + earnings separately) as the hero number in Profit Center.
6. **Add share buttons** (Twitter, Telegram, QR) to the referral page.
7. **Add progress bars** to position cards showing time-to-maturity.
8. **Make the live feed prominent** on Shenanigans — it's content, not a footnote.
9. **Add notification badges** to nav tabs (withdrawable earnings, PP thresholds).
10. **Mobile: convert wallet dropdown to bottom sheet**, add pull-to-refresh.

---

## 15. What's Actually Good

Don't lose these:

- **The tone is perfect.** Self-aware, irreverent, honest about being a gambling game. The copy is the best part of the entire product.
- **The color system is cohesive.** Purple/green/gold/pink works. The dark felt background is atmospheric without being oppressive.
- **The confetti celebration** on successful transactions is delightful.
- **Withdrawal confirmation dialog** with live countdown to lower toll — this is genuinely thoughtful UX.
- **The gambling warnings** are prominent and honest. Legally smart, morally right, and they actually enhance the satirical brand.
- **Error/empty states exist.** Most dev-built UIs forget these entirely. Every page has a loading state, an error state, and an empty state. That's solid craftsmanship.
- **The shenanigan card aura effects** are genuinely atmospheric. Best visual design in the app.

---

*End of report. The bones are strong. The flesh needs sculpting.*
