# UX Implementation Plan v2

*Written after an honest self-evaluation found the first pass was 35% complete on substance, 80% on copy. This plan covers the remaining 65%.*

*Every section ends with a verification loop. You don't move on until the loop passes.*

---

## Guiding Principles

1. **"If I were designing this from scratch, what would I do?"** — Don't preserve old decisions out of inertia.
2. **Show less, do more, feel better.** — Fewer words, more interactivity, better pacing.
3. **Structure before copy.** — Layout, data display, and interaction patterns come first. Words come last.
4. **The self-evaluation is the standard.** — Every grade below B must be raised. Every "What was NOT done" must be done.

---

## Phase 1: Persistent Game Status Bar

**Priority: #1. This was the single most impactful missing feature identified in the entire original report. It was graded F in the self-evaluation.**

**Self-evaluation said:** *"The status bar was the single most impactful missing feature identified in the entire report. It was Phase 3 in the task list — meant to be done third. It wasn't done at all."*

### What It Is
A thin, always-visible bar below the header that shows the player's key numbers at a glance. On every page, on every tab, at all times. This is the "casino floor dashboard" — the thing you glance at between hands.

### Design

**Layout:** Full-width bar, fixed below header (below `mc-header`, above main content). Height: ~36px on desktop, ~32px on mobile. Background: `rgba(8, 6, 14, 0.85)` with `backdrop-filter: blur(12px)`. Border-bottom: `1px solid var(--mc-border)`.

**Desktop (≥769px) — single row, left-to-right:**
```
[Game Balance: 4.523 ICP]  [Earnings: +1.204 ICP ▲]  [Positions: 3 active]  [PP: 12,400]  [Pot: 847.2 ICP]
```

**Mobile (<769px) — compact, two key numbers:**
```
[4.523 ICP]  [+1.204 ▲]  [3 pos]  [12.4k PP]
```

**Styling details:**
- Each stat is a `<span>` with `mc-label` above and value below, separated by thin `border-right: 1px solid var(--mc-border)` dividers
- Game Balance: `mc-text-primary` (white)
- Earnings: `mc-text-green` with `mc-glow-green` if positive, `mc-text-danger` if negative
- Active Positions: `mc-text-cyan`
- PP: `mc-text-purple`
- Pot Size: `mc-text-gold`
- The earnings number should use `useLivePortfolio` for real-time updates (already exists in `useLiveEarnings.ts`)
- Pot size comes from `useGetGameStats` (the same hook HouseDashboard uses)

### Implementation

**New component:** `frontend/src/components/GameStatusBar.tsx`

```
Props: none (reads from hooks directly)
Hooks needed:
  - useGetInternalWalletBalance() → walletBalance
  - useLivePortfolio(games) → totalEarnings, totalDeposits
  - useGetUserGames() → game count
  - useGetPonziPoints() → totalPoints
  - useGetGameStats() → pot size
```

**Mount location:** `App.tsx`, inside the authenticated branch, immediately after the `<header>` closing tag and before `<main>`. Only render when `isAuthenticated && !showProfileSetup`.

**CSS additions to `index.css`:**
- `.mc-status-bar` — the bar container (fixed positioning, below header)
- `.mc-status-bar-stat` — each stat cell
- Adjust `.mc-content-offset` and `<main>` padding-top to account for the extra bar height (~36px)
- On mobile, the status bar sits below the header but above content. `<main>` padding-top becomes `pt-[calc(4rem+36px)] md:pt-[calc(5rem+36px)]` or equivalent.

### Files Modified
- `frontend/src/components/GameStatusBar.tsx` (new)
- `frontend/src/App.tsx` (mount the bar)
- `frontend/src/index.css` (status bar styles, adjust content offset)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 12 ("Cross-Cutting Issues") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Does the status bar show all 5 stats? Is it visible on every authenticated page? Does it update live? Does it work on mobile? Does the content below it not overlap?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 2: Navigation Paradigm — Sidebar to Header Tabs

**Self-evaluation grade: C. "Tab count fixed, but the navigation paradigm itself was never reconsidered."**

### What Changes
The desktop left rail (`.mc-rail`, 200px fixed sidebar) is replaced with horizontal header tabs integrated into the existing `mc-header`. Mobile bottom tabs stay as-is.

### Why
5 tabs don't justify a 200px sidebar that eats horizontal space on every page. A compact horizontal tab bar in the header gives back that space and keeps the player's eyes on the content, not the nav.

### Design

**Desktop (≥769px):** Inside the header, between the logo and the right controls (Wallet/Logout), add a horizontal tab row. Each tab is an inline button with icon + label. The active tab gets an underline indicator (2px bottom border, colored by tab type).

```
[Logo: MC / It's a Ponzi!]   [Profit Center] ["Invest"] [Seed Round] [MLM] [Shenanigans]   [Wallet] [Logout]
```

Tab styling:
- Font: `font-body` (Space Mono), 13px, bold, uppercase
- Inactive: `mc-text-muted`, no border
- Hover: `mc-text-dim`, subtle background `rgba(255,255,255,0.04)`
- Active: `mc-text-primary`, `border-bottom: 2px solid var(--mc-purple)` (or `--mc-neon-green` for Shenanigans)
- Icon: 16px, same color as text, `gap-1.5` between icon and label
- Padding: `px-3 py-2`, `gap-1` between tabs

**Mobile (<769px):** No change. Bottom tabs remain.

### Implementation

**Dashboard.tsx** is the component that currently owns the navigation state (`activeTab`). The challenge: the header lives in `App.tsx` and the nav lives in `Dashboard.tsx`. Two approaches:

**Approach A (recommended):** Lift `activeTab` state from `Dashboard.tsx` to `App.tsx`. Pass `activeTab` and `setActiveTab` down to `Dashboard` as props. Render the header tabs in `App.tsx` (only when `showDashboard && !isMobile`).

**Approach B:** Keep state in Dashboard, use a React context or callback. More complex, less recommended.

Go with Approach A.

**Changes to `App.tsx`:**
- Add `activeTab` / `setActiveTab` state
- In the header (desktop only), after the logo and before the right controls, render the tab buttons
- Pass `activeTab` and `onTabChange` to `<Dashboard>`

**Changes to `Dashboard.tsx`:**
- Accept `activeTab` and `onTabChange` as props instead of owning the state
- Remove the `mc-rail` section entirely
- Remove the `mc-content-offset` class from the content wrapper
- Keep the mobile bottom tabs (they still live here)
- Remove the `isMobile` check for the rail; only use it for the bottom tabs

**CSS changes:**
- `.mc-rail`, `.mc-rail-item`, `.mc-rail-icon`, `.mc-rail-label`, `.mc-content-offset` — delete all of these
- Add `.mc-header-tabs` — horizontal flex container for header tabs
- Add `.mc-header-tab` — individual tab button style
- Add `.mc-header-tab.active` — active state with bottom border

### Files Modified
- `frontend/src/App.tsx` (lift state, render header tabs on desktop)
- `frontend/src/components/Dashboard.tsx` (accept props, remove rail)
- `frontend/src/index.css` (remove rail CSS, add header tab CSS)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 3 ("Navigation") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is the desktop sidebar gone? Are tabs in the header? Does active state work? Does mobile bottom nav still work? Does content fill the full width now? Is there no wasted horizontal space?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 3: Profit Center — P/L Hero Card + Position Improvements

**Self-evaluation grade: D. "The most-visited page still doesn't answer 'am I up or down?' at a glance."**

### 3.1 Net P/L Hero Card

The "Running Tally" section in `GameTracking.tsx` (lines 251-263) currently shows:
- Total Deposits (white)
- Accumulated Earnings (green)

It does NOT show the single most important number: **net profit/loss**. Add it.

**Design:** Replace the 2-column grid with a 3-column layout or a hero number + supporting stats:

```
┌──────────────────────────────────────────────┐
│           YOUR RUNNING TALLY                 │
│                                              │
│      Net P/L: +0.847 ICP  ▲                 │  ← Hero number, large
│      (mc-text-green, 2xl, mc-glow-green)     │
│                                              │
│  [Deposited: 3.200 ICP]  [Earned: 4.047 ICP]│  ← Supporting stats, smaller
└──────────────────────────────────────────────┘
```

**Net P/L = totalEarnings - totalDeposits.** This is already computable from `useLivePortfolio`. If positive, green with glow. If negative, red (`mc-text-danger`). Show an up arrow (▲) or down arrow (▼) next to the number.

### 3.2 Progress Bars on Position Cards

In `PositionCard` (GameTracking.tsx lines 72-146), add a thin progress bar showing how far through the plan duration the position is.

**Simple plans:** 21 days total. Progress = `daysActive / 21`.
**Compounding plans:** 15 or 30 days. Progress = `daysActive / planDays`. If locked, show the bar in purple. If unlocked, show it in green.

Use the existing `<Progress>` component from `@/components/ui/progress`. Place it between the numbers row and the withdraw button. Height: `h-1.5` (thin). Color: override the shadcn default with `mc-text-green` or `mc-text-purple` depending on plan type.

### 3.3 Exit Toll Badge on Each Card

The exit toll percentage is already shown (line 119-124), but it's just a number. Make it a visual badge:
- `7%` → red-ish badge (`bg-red-500/20 text-red-400`)
- `5%` → gold badge (`bg-yellow-500/20 text-yellow-400`)
- `3%` → green badge (`bg-green-500/20 text-green-400`)
- `13%` (compounding) → purple badge (`bg-purple-500/20 text-purple-400`)

This makes the toll tier instantly scannable without reading.

### 3.4 Sort Positions by Urgency

Currently positions render in array order (lines 273-281). Sort them by urgency:
1. Compounding positions near unlock (< 3 days remaining) → first
2. Simple positions with high toll (7%) → next
3. Everything else → by start date (oldest first)

This ensures the positions that need attention are at the top.

### Files Modified
- `frontend/src/components/GameTracking.tsx` (hero card, progress bars, toll badges, sorting)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 4 ("Profit Center") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is the net P/L number visible as a hero? Does it update live? Are progress bars on every position card? Are toll tiers color-coded? Are positions sorted by urgency? Are there any duplicate refresh buttons?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 4: Pick Your Plan — Remove Step Numbers + UX Fixes

**Self-evaluation grade: D. "The step numbers being left in is particularly inexcusable since the report specifically flagged them."**

### 4.1 Remove Step Numbers

In `GamePlans.tsx`:
- Line 113: `Step 1 — Choose Your Poison` → `Choose Your Poison`
- Line 160: `Step 2 — Select Lockup Period` → `Select Lockup Period`
- Line 194-195: `Step 2` / `Step 3` → just `Enter Amount & Open Position`

The step numbers imply a rigid linear flow. The actual flow is: pick mode → (optionally pick lockup) → enter amount. It's not a 3-step wizard; it's a card-based selector. Remove the step framing entirely. Keep the `mc-label` class and the descriptive text.

### 4.2 Min/Max Amount Buttons

Next to the amount input (line 216-223), add two small buttons:

- **MIN** — sets amount to `minDeposit` (0.1 ICP)
- **MAX** — sets amount to `Math.min(walletBalance, maxDeposit)` for simple, or `walletBalance` for compounding

These go in a button group to the right of the input, similar to the existing MAX button in WalletDropdown (line 259).

### 4.3 Single Always-Enabled CTA

Currently (lines 298-309), the CTA button has multiple disabled states and changes text based on error conditions. Simplify:

- Button is ALWAYS enabled (never `disabled`)
- If clicked with invalid state, show an inline error message below the button (shake animation optional)
- Button text: always `START GAME` (with the Dices icon)
- Remove the conditional text for rate limit, input error, etc. — show those as status messages above or below the button, not as button text

This makes the page feel actionable at all times instead of having a grayed-out dead button.

### Files Modified
- `frontend/src/components/GamePlans.tsx` (remove step numbers, add min/max, simplify CTA)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 5 ("Pick Your Plan") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Are ALL step numbers removed (search for "Step" in GamePlans.tsx — zero matches)? Are Min and Max buttons present and functional? Is the CTA always enabled? Does clicking with invalid state show an inline error instead of a disabled button?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 5: Referral / MLM — Share Buttons + Functional Improvements

**Self-evaluation grade: D+. "Better words, zero functional improvements. The entire point of a referral page is making it easy to share."**

### 5.1 Share Buttons

Below the referral link copy button (ReferralSection.tsx line 91-94), add a row of share buttons:

- **Twitter/X** — opens `https://twitter.com/intent/tweet?text={encodedMessage}&url={encodedLink}`
- **Telegram** — opens `https://t.me/share/url?url={encodedLink}&text={encodedMessage}`
- **WhatsApp** — opens `https://wa.me/?text={encodedMessage} {encodedLink}`
- **Copy Link** — already exists, keep it

Pre-written share message (Charles voice):
```
"I found a Ponzi scheme that's honest about being a Ponzi scheme. Up to 12% daily. It's called Musical Chairs."
```

Each button: icon (from lucide or inline SVG) + platform name. Styled as `mc-btn-secondary` with `px-4 py-2 rounded-lg text-xs`. Row layout: `flex flex-wrap gap-2`.

### 5.2 QR Code

Below the share buttons, show a QR code of the referral link. Use a lightweight QR library like `qrcode.react` (or generate inline SVG). The QR code should be downloadable (wrap in a canvas and provide a "Download QR" button).

Install dependency: `npm install qrcode.react`

### 5.3 Milestone Badges (stretch)

If the user has referrals, show milestone badges:
- 1 referral: "First Blood"
- 5 referrals: "Networker"
- 10 referrals: "Pyramid Architect"
- 25 referrals: "MLM Legend"

Small badge row above the stats grid. Use `mc-status-green`/`mc-status-gold` styling. Show earned badges lit up, unearned badges grayed out.

### Files Modified
- `frontend/src/components/ReferralSection.tsx` (share buttons, QR code, milestones)
- `package.json` (add `qrcode.react` dependency)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 8 ("MLM / Referral") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Are there share buttons for Twitter, Telegram, and WhatsApp? Do they open the correct URLs with pre-written messages? Is there a QR code? Is the QR downloadable? Are milestone badges shown?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 6: Shenanigans — Filter Tabs + Contextual Cast Buttons

**Self-evaluation grade: B-. "Good visual improvements. But the concrete UX fixes from the report weren't done."**

### 6.1 Filter Tabs

Above the shenanigan cards grid (Shenanigans.tsx line 172), add filter tabs:

```
[All]  [Offense]  [Defense]  [Chaos]
```

Each shenanigan should have an `auraCategory` field (or derive it from the type). Rough categorization:
- **Offense:** moneyTrickster, aoeSkim, mintTaxSiphon, downlineHeist, purseCutter, whaleRebalance
- **Defense:** magicMirror, ppBoosterAura, downlineBoost
- **Chaos:** renameSpell, goldenName

Use pill-style tabs similar to the Backers/Ledger toggle in HouseDashboard (lines 12-31). When a filter is active, only show matching cards. "All" shows everything.

### 6.2 Contextual Cast Buttons

Currently (lines 215-223), every cast button says "Cast" or "Casting...". Change to show the cost:

```
Cast (500 PP)
```

If the user can't afford it, change to:
```
Need 500 PP
```
(disabled, `mc-text-muted`)

This eliminates the need to scan up to the PP cost badge to know if you can afford to cast.

### 6.3 Odds Bar Labels

Currently (lines 201-211), the odds bar shows percentages but no labels for what the colors mean. Add text labels:

```
[===green===][==red==][=purple=]
 Success 60%  Fail 25%  Backfire 15%
```

Replace the current three bare `<span>` elements with labeled versions:
```
<span className="mc-text-green">✓ {trick.odds.success}%</span>
<span className="mc-text-danger">✗ {trick.odds.fail}%</span>
<span className="mc-text-purple">↩ {trick.odds.backfire}%</span>
```

### 6.4 Live Feed Size

Currently `max-h-48` (line 250). Increase to `max-h-72` and show more entries (from 12 to 20). This makes the feed feel more alive.

### Files Modified
- `frontend/src/components/Shenanigans.tsx` (filter tabs, contextual buttons, odds labels, feed size)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 9 ("Shenanigans") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Do filter tabs exist and work (All/Offense/Defense/Chaos)? Do cast buttons show the PP cost? Do they show "Need X PP" when unaffordable? Do odds bars have Success/Fail/Backfire labels? Is the live feed larger (max-h-72, 20 entries)?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 7: House Ledger — Progressive Disclosure + Structural Fixes

**Self-evaluation grade: C-. "Better words on the same structure."**

### 7.1 BackerInfoCard → Collapsible FAQ

The `BackerInfoCard` component (HouseDashboard.tsx lines 37-125) renders 4 info cards + the Redistribution Event callout all at once. This is a wall of text.

Replace with a collapsible accordion:
- Each section title is a clickable header
- Click to expand/collapse the content
- Default: all collapsed (the first one optionally expanded)
- Use a simple `useState<string | null>` for which section is open

The "Redistribution Event" callout stays always visible (it's the most dramatic element and shouldn't be hidden).

### 7.2 AddHouseMoney Promotion

Currently `AddHouseMoney` is rendered inside a card within the BackerPositions grid (line 244-246). It's visually equivalent to any other card. Promote it:

- Move it above the grid, full-width
- Give it its own `mc-card-elevated` wrapper with a hero treatment
- Larger text: "Back the House" as a `font-display text-xl` heading
- The form itself stays the same, just the visual hierarchy changes

### 7.3 Redistribution Event Dramatic Treatment

The Redistribution Event callout (lines 107-122) is already styled with `mc-accent-danger` but it's static. Add:
- A subtle pulsing animation on the `Flame` icon (CSS keyframe, similar to `mc-aura-pulse`)
- A `mc-status-red` glow effect on hover
- Keep the content as-is; the copy is already dramatic enough

### Files Modified
- `frontend/src/components/HouseDashboard.tsx` (collapsible FAQ, AddHouseMoney promotion, event treatment)
- `frontend/src/index.css` (minor animation additions if needed)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 6 ("House Ledger") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is BackerInfoCard collapsible? Is AddHouseMoney promoted to a hero position? Does the Redistribution Event have visual drama (animation, glow)? Is the wall of text broken up into progressive disclosure?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 8: Wallet — Mobile Bottom Sheet + Money Flow Indicator

**Self-evaluation grade: B. "Solid functional improvements. Mobile UX unchanged."**

### 8.1 Mobile Bottom Sheet

Currently `WalletDropdown` renders as a dropdown positioned below the wallet button (line 159, `mc-dropdown`). On mobile, this is awkward — it's a fixed-position dropdown that can clip off-screen.

On mobile (`window.innerWidth < 769`), render as a bottom sheet instead:
- Full width, slides up from bottom
- `position: fixed; bottom: 0; left: 0; right: 0;`
- Max height: `70vh`
- Rounded top corners: `rounded-t-2xl`
- Backdrop overlay: `bg-black/50` behind it
- Drag handle at top (small gray bar, cosmetic)
- Close by tapping overlay or clicking X

Use the same `isMobile` detection pattern as Dashboard (resize listener). The content stays exactly the same — only the container changes.

### 8.2 Money Flow Indicator

Add a small visual diagram showing how money flows through the system:

```
Wallet → Game Balance → Position → Earnings → Withdraw
```

Render as a simple horizontal flow with arrows and icons. Place it inside the wallet dropdown/sheet, below the balance section (after line 220, before the tabs).

- Each node: small icon + label, connected by `→` arrows
- Current step highlighted (e.g., if they have a balance, "Game Balance" is highlighted)
- Subtle, small — `text-xs mc-text-muted`

This helps new users understand where their money is at any given time.

### Files Modified
- `frontend/src/components/WalletDropdown.tsx` (bottom sheet on mobile, money flow indicator)
- `frontend/src/index.css` (bottom sheet animation, overlay)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 11 ("Wallet System") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: On mobile, does the wallet open as a bottom sheet (not a dropdown)? Is there a backdrop overlay? Does it close on overlay tap? Is the money flow indicator visible? Does it correctly show the flow from wallet to position to earnings?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 9: Hall of Fame — Podium + Your Rank

**Self-evaluation grade: C. "Correctly merged. Personality added. But the visual drama that makes leaderboards exciting was not implemented."**

### 9.1 Podium Visualization for Top 3

The `HallOfFame.tsx` component currently uses `mc-rank-gold`, `mc-rank-silver`, `mc-rank-bronze` for the top 3, but they're just styled cards in a list. Replace with a visual podium:

```
         🥇
        [1st]
   🥈  ████  🥉
  [2nd] ████ [3rd]
  ████  ████  ████
```

Three columns. The center column (1st place) is taller than the sides. Each podium block shows:
- Avatar/initial circle
- Player name
- PP count
- Rank medal (gold, silver, bronze glow effects)

Use CSS grid with `grid-template-rows` to create the stepped height effect. No external libraries needed.

### 9.2 "Your Rank" Indicator

After the podium, show the current user's rank:

```
Your Rank: #47 of 312 players  |  12,400 PP
```

If the user is in the top 3, highlight this with a gold glow. If not ranked, show "Unranked — start earning PP to climb."

This requires knowing the user's principal to find them in the leaderboard. Use the existing `useWallet` hook for `principal` and match against the leaderboard entries.

### 9.3 Wire HallOfFame into Dashboard

HallOfFame is currently an orphaned component — it exists but isn't rendered in any tab. The self-evaluation says it was "merged into Shenanigans." Verify this is true. If HallOfFame is NOT rendered inside the Shenanigans tab, add it as a section at the bottom of the Shenanigans page.

### Files Modified
- `frontend/src/components/HallOfFame.tsx` (podium, your rank)
- `frontend/src/components/Shenanigans.tsx` (ensure HallOfFame is rendered)
- `frontend/src/index.css` (podium CSS)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 10 ("Hall of Fame") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is there a visual podium for top 3? Does it show names, PP, and rank medals? Is there a "Your Rank" indicator? Is HallOfFame actually rendered somewhere in the app (verify by navigating to Shenanigans in the browser)?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 10: Splash Page — Live Stats Ribbon

**Self-evaluation grade: C+. "The urgency-creating elements (live stats, social proof) that would actually convert visitors are absent."**

### What It Is
A horizontal stats ribbon on the splash page showing live game data. This creates urgency and social proof for visitors who haven't logged in yet.

### Design

Between the three info cards and the Charles quote, add:

```
┌────────────────────────────────────────────────────┐
│  🏦 Pot: 847 ICP  |  👥 12 Players  |  💰 Last Payout: 2.4 ICP  │
└────────────────────────────────────────────────────┘
```

Styling: `mc-card` with `mc-accent-gold` border. Horizontal flex, centered, `gap-6`. Small text (`text-xs`). Numbers in bold accent colors.

### Data Source

These stats need to be available to unauthenticated users. Check if `useGetGameStats` works without authentication. If not, the backend may need a public query endpoint. If the backend doesn't support this, mock the data or skip this section and note it as blocked.

If the data IS available:
- Pot size: from `gameStats.potBalance`
- Active players: from `gameStats.activePlayers` or `gameStats.totalPlayers`
- Last payout: from recent house ledger or game records

### Files Modified
- `frontend/src/App.tsx` (add stats ribbon to splash section)
- May need to check/add a public stats hook in `useQueries.ts`

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 1 ("Splash / Landing Page") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is there a live stats ribbon on the splash page? Does it show pot size, player count, and last payout? Does it update with real data (or gracefully degrade if the backend doesn't support unauthenticated queries)?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 11: ProfileSetup — Post-Submit Celebration

**Self-evaluation grade: B-. "Still a form, not an experience. The 'moment of joining a gambling game' still doesn't feel special enough."**

### What Changes

After the user submits their name and the profile saves successfully, show a celebration screen BEFORE redirecting to the dashboard:

1. Trigger `triggerConfetti()` (already imported and available)
2. Show a centered celebration message:
   ```
   Welcome to Musical Chairs, [NAME]!

   "I knew you had it in you."
   — Charles

   [TAKE ME TO THE TABLE] (button → navigates to dashboard)
   ```
3. After 3 seconds, auto-navigate to the dashboard (or let them click)

### Implementation

Add a `showCelebration` state to `ProfileSetup.tsx`. After `saveProfile.mutate` succeeds (use `onSuccess` callback), set `showCelebration = true` and trigger confetti.

When `showCelebration` is true, render the celebration screen instead of the form. After 3 seconds (`setTimeout`), the parent App.tsx will naturally redirect because `userProfile` will no longer be null (React Query will refetch).

### Files Modified
- `frontend/src/components/ProfileSetup.tsx` (celebration screen, confetti trigger)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 2 ("Profile Setup / Onboarding") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: After submitting a name, does confetti trigger? Is there a celebration screen with the user's name and a Charles quote? Does it auto-navigate or provide a button to continue?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 12: Notification Badges on Navigation

**Self-evaluation said:** *"No notification badges (red dot for withdrawable earnings, purple dot for castable shenanigans)."*

### Design

On the navigation tabs (header on desktop, bottom bar on mobile), show small colored dots to indicate actionable items:

- **Profit Center tab:** Red dot if any position has withdrawable earnings > 0
- **Shenanigans tab:** Purple dot if user has enough PP to cast at least one shenanigan

### Implementation

Dot: `w-2 h-2 rounded-full absolute -top-1 -right-1` positioned on the icon or label. Red dot: `bg-red-500`. Purple dot: `bg-purple-500`. Add a subtle pulse animation.

The data hooks are already being called elsewhere in the app. Either:
- Lift the relevant checks to `App.tsx` (or wherever the header tabs live after Phase 2)
- Or use the hooks in Dashboard.tsx and pass badge state to the nav items

### Files Modified
- `frontend/src/App.tsx` or `frontend/src/components/Dashboard.tsx` (badge logic + rendering)
- `frontend/src/index.css` (badge dot styles, pulse animation)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 3 ("Navigation") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Is there a red dot on Profit Center when earnings are withdrawable? Is there a purple dot on Shenanigans when the user can cast? Do the dots appear on both desktop header tabs and mobile bottom tabs? Do the dots pulse?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Phase 13: Functional Animations

**Self-evaluation said:** *"No functional animations (countUp, shake on error)."*

### 13.1 CountUp on Big Numbers

When the Profit Center page loads, the hero P/L number and the tally numbers should count up from 0 to their actual value over ~1 second. This creates a "casino scoreboard" feel.

Implementation: A small `useCountUp(target, duration)` hook that interpolates from 0 to `target` using `requestAnimationFrame`. Use it on:
- Net P/L in the GameTracking hero card
- Total Deposits and Total Earnings in the Running Tally

### 13.2 Shake on Error

When a form submission fails (GamePlans, WalletDropdown, ProfileSetup), the input field should shake briefly. CSS-only:

```css
@keyframes mc-shake {
  0%, 100% { transform: translateX(0); }
  20%, 60% { transform: translateX(-4px); }
  40%, 80% { transform: translateX(4px); }
}
.mc-shake { animation: mc-shake 0.4s ease-in-out; }
```

Apply the `mc-shake` class conditionally when an error occurs.

### Files Modified
- `frontend/src/hooks/useCountUp.ts` (new hook)
- `frontend/src/components/GameTracking.tsx` (countUp on numbers)
- `frontend/src/components/GamePlans.tsx` (shake on error)
- `frontend/src/components/WalletDropdown.tsx` (shake on error)
- `frontend/src/index.css` (shake keyframe)

### Verification Loop
After completing this phase:
1. Re-read this section of the task list.
2. Re-read Section 12 ("Cross-Cutting Issues") of `docs/UX_SELF_EVALUATION.md`.
3. Cross-check: Do big numbers count up on page load? Do inputs shake on error? Are the animations smooth and not janky?
4. If anything is missing, fix it before moving on.
5. Repeat steps 1-4 until the section is TRULY complete.

---

## Execution Order

The phases are ordered by impact. If time is limited, do them in this order:

1. **Phase 1: Status Bar** — F → A (the biggest single win)
2. **Phase 2: Nav to Header** — C → A (reclaims horizontal space, feels modern)
3. **Phase 3: Profit Center** — D → B+ (answers "am I up or down?")
4. **Phase 4: Pick Your Plan** — D → B (removes the step numbers, adds Min/Max)
5. **Phase 5: Referral Share Buttons** — D+ → B+ (makes the MLM page actually useful)
6. **Phase 6: Shenanigans Filters** — B- → A- (filter tabs + contextual buttons)
7. **Phase 7: House Ledger** — C- → B (progressive disclosure)
8. **Phase 8: Wallet Mobile** — B → A- (bottom sheet)
9. **Phase 9: Hall of Fame Podium** — C → B+ (visual drama)
10. **Phase 10: Splash Stats** — C+ → B+ (social proof, may be blocked by backend)
11. **Phase 11: ProfileSetup Celebration** — B- → A (confetti + welcome)
12. **Phase 12: Notification Badges** — C → B+ (actionable indicators)
13. **Phase 13: Functional Animations** — F → B (countUp + shake)

---

## Files Changed Summary

**New files:**
- `frontend/src/components/GameStatusBar.tsx`
- `frontend/src/hooks/useCountUp.ts`

**Modified files:**
- `frontend/src/App.tsx` (status bar mount, header tabs, splash stats)
- `frontend/src/components/Dashboard.tsx` (nav refactor, notification badges)
- `frontend/src/components/GameTracking.tsx` (P/L hero, progress bars, sorting, countUp)
- `frontend/src/components/GamePlans.tsx` (step numbers, min/max, CTA, shake)
- `frontend/src/components/ReferralSection.tsx` (share buttons, QR, milestones)
- `frontend/src/components/Shenanigans.tsx` (filter tabs, contextual buttons, odds labels, feed size, HallOfFame mount)
- `frontend/src/components/HouseDashboard.tsx` (collapsible FAQ, AddHouseMoney promotion, event treatment)
- `frontend/src/components/WalletDropdown.tsx` (mobile bottom sheet, money flow, shake)
- `frontend/src/components/HallOfFame.tsx` (podium, your rank)
- `frontend/src/components/ProfileSetup.tsx` (celebration screen)
- `frontend/src/index.css` (status bar, header tabs, bottom sheet, podium, shake, countUp, notification dot, remove rail CSS)
- `package.json` (add `qrcode.react`)

---

*End of plan. Every phase has a verification loop. Every verification loop requires re-reading this plan AND the self-evaluation. No phase is done until the loop passes.*
