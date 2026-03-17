# Musical Chairs — UX Implementation Task List

*Generated: February 2026*
*Based on: UX Evaluation Report + Owner Input*
*Status: AWAITING APPROVAL — Do not begin until owner signs off*

---

## Decision Log (Owner Answers)

- **Navigation:** Merge 7 tabs to 5, keep personality names (Profit Center, Pick Your Plan, House Ledger, Shenanigans, The Pyramid)
- **Status bar:** Show everything — pot, P/L, round, active players, PP balance
- **Splash stats:** Public queries exist (`getPlatformStats`, `getActiveGameCount`, `getAvailableBalance`, `getRecentShenanigans`) — can be called without auth
- **Trollbox:** Separate Motoko canister, ship clean first, PP powers later
- **Backend language:** Stay Motoko, do NOT convert to Rust
- **Docs:** Both splash teaser + in-app full docs
- **Onboarding:** Guided tooltip tour after profile setup
- **Share targets:** Twitter/X, Telegram, WhatsApp, Facebook, QR Code
- **Admin:** Lock to 3 principals, rename to "Charles" everywhere, give Charles a visual identity
- **Admin principals:** `zs6vm-4yyag-sbw7x-6ipms-h4tmz-ox4pu-mcq3b-thtt4-de25x-wmsh4-rqe`, `stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe`, `zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae`

---

## Phase 1: Critical Fixes & Foundation

### 1.1 Fix Duplicate Icon Bug in More Sheet
**File:** `Dashboard.tsx` lines 184-185
**Task:** Remove duplicate `{item.icon}` render in the more sheet. Keep only the `<span className="text-lg">{item.icon}</span>` version.
**Effort:** 5 min

### 1.2 Lock Admin Access ("Charles")
**Files:** `App.tsx`, `ShenanigansAdminPanel.tsx`, any admin references
**Tasks:**
- Create a `CHARLES_PRINCIPALS` constant array with the 3 principal IDs
- Gate the admin button visibility: only show if `identity.getPrincipal()` matches a Charles principal
- Rename all "Admin" references to "Charles" (button label, panel header, back button text)
- Create a Charles visual identity: a small monochrome silhouette icon (top hat + monocle aesthetic — the shadowy casino owner). Use lucide-react `Crown` or `User` with custom styling as placeholder, or create a simple SVG.
- System messages in trollbox (future): "Charles says..." for admin announcements
**Effort:** 1-2 hours

### 1.3 Wallet-Type-Aware Dropdown
**Files:** `WalletDropdown.tsx`, `App.tsx`
**Tasks:**
- Detect `walletType` from authentication context (II vs Plug vs OISY)
- II users: show full dropdown (deposit, withdraw, balance, profile edit)
- Plug/OISY users: show simplified view (game balance only, profile edit). Hide deposit/withdraw buttons.
- Rename "Internal Balance" → "Game Balance", "External Balance" → remove or relabel contextually
- Mobile: convert dropdown to bottom sheet (create `WalletBottomSheet.tsx` or add responsive variant)
**Effort:** 2-3 hours

---

## Phase 2: Navigation Restructure

### 2.1 Merge 7 Tabs to 5
**File:** `Dashboard.tsx`
**Tasks:**
- Remove `rewards` tab — merge PP balance display into Profit Center as a sub-card
- Remove `hallOfFame` tab — move leaderboards into Shenanigans as a sub-tab (toggle: "Cast" / "Leaderboards")
- Final 5 tabs: `profitCenter`, `setup`, `houseLedger`, `shenanigans`, `referrals`
- Labels: "Profit Center", "Pick Your Plan", "House Ledger", "Shenanigans", "The Pyramid"
- Delete the More sheet entirely — all 5 tabs fit in mobile bottom bar
- Update desktop rail: remove group dividers, show all 5 with always-visible labels
**Effort:** 3-4 hours

### 2.2 Desktop Rail: Always Show Labels
**File:** `index.css` (.mc-rail styles)
**Tasks:**
- Set rail width to 200px (fixed, no collapse)
- Remove hover-expand behavior
- Always show `.mc-rail-label`
- Adjust main content margin-left accordingly
**Effort:** 30 min

### 2.3 Notification Badges
**Files:** `Dashboard.tsx`, new `NotificationBadge.tsx` component
**Tasks:**
- Red dot on Profit Center when any position has withdrawable earnings
- Purple dot on Shenanigans when PP balance exceeds cheapest shenanigan cost (100 PP)
- Gold dot on The Pyramid when there's unviewed referral activity (stretch goal)
- Badge component: 8px circle, absolute positioned top-right of nav icon
**Effort:** 2-3 hours

---

## Phase 3: Persistent Game Status Bar

### 3.1 Create Status Bar Component
**File:** New `GameStatusBar.tsx`
**Tasks:**
- Fixed sub-header bar below the main header (h-10, full width, mc-felt-raised background)
- Data points (left to right): Pot Size | Your P/L | Round # | Active Players | PP Balance
- Use public queries: `getAvailableBalance()`, `getPlatformStats()`, `getActiveGameCount()`
- P/L calculated client-side from user games data (total earnings - total deposits)
- PP balance from `getPonziPoints()`
- Color code P/L: green if positive, red if negative
- Auto-refresh every 30 seconds via react-query refetchInterval
- Mobile: show only Pot + P/L + PP (3 items). Full bar accessible via horizontal scroll or tap-to-expand.
**Effort:** 3-4 hours

### 3.2 Adjust Layout for Status Bar
**Files:** `App.tsx`, `Dashboard.tsx`
**Tasks:**
- Add `pt-26 md:pt-30` to main content (header 16/20 + status bar 10)
- Status bar only visible when authenticated and past profile setup
**Effort:** 30 min

---

## Phase 4: Splash Page Overhaul

### 4.1 Live Stats on Splash
**File:** `App.tsx` (splash section)
**Tasks:**
- Add a stats ribbon above the login CTA showing: pot size, active players, recent payouts
- Call `getPlatformStats()`, `getActiveGameCount()` without auth (these are `public query` methods — confirmed from backend)
- Style as a compact horizontal bar with mc-card background, neon number styling
- Animate numbers with countUp effect on page load
**Effort:** 2-3 hours

### 4.2 Reorder Splash Layout
**File:** `App.tsx`
**Tasks:**
- Move info cards (Pitch, Catch, Twist) ABOVE the login CTA
- Add staggered entrance animation to cards (already have mc-stagger)
- Move login CTA to after the cards with a prominent "Join the Game" label
- Add typewriter effect to tagline "It's a Ponzi!" (CSS animation or lightweight library)
**Effort:** 1-2 hours

### 4.3 Docs Teaser on Splash
**File:** `App.tsx`
**Tasks:**
- Below the info cards, add a "How It Works" expandable section or scrollable cards
- Content: brief summaries of game plans, exit tolls, shenanigans, PP system
- "Read the full docs" link that scrolls to the docs section (or opens in-app docs after login)
- Keep it concise — 3-4 cards max, matching the existing mc-card style
**Effort:** 2-3 hours

### 4.4 Animated Background
**File:** `index.css` or new `ParticleBackground.tsx`
**Tasks:**
- Subtle floating particles or slow gradient animation behind splash hero
- Must be performant (CSS-only or lightweight canvas, NOT a heavy library)
- Respect `prefers-reduced-motion`
**Effort:** 1-2 hours

---

## Phase 5: Profit Center Enhancements

### 5.1 Net P/L Hero Card
**File:** `GameTracking.tsx`
**Tasks:**
- Replace the 2-column tally (deposits, earnings) with a 3-column layout: Deposits | Net P/L | Earnings
- Net P/L = Earnings - Deposits (including withdrawn amounts)
- P/L card is the center, largest, with green/red color and +/- prefix
- Remove duplicate refresh button (keep only one, top-right)
**Effort:** 1-2 hours

### 5.2 PP Balance Inline
**File:** `GameTracking.tsx` or new sub-component
**Tasks:**
- Add a compact PP balance card below the tally section (since Rewards page is merged in)
- Show: total PP, breakdown (deposit/referral/burned), "Spend on Shenanigans →" CTA link
- Replaces the standalone PonziPointsDashboard component
**Effort:** 1-2 hours

### 5.3 Position Card Progress Bars
**File:** `GameTracking.tsx`
**Tasks:**
- Add thin progress bar to each position card showing time elapsed / plan duration
- Simple 21-day: 21 day total, show `daysActive / 21`
- Compounding 15-day: show `daysActive / 15`
- Compounding 30-day: show `daysActive / 30`
- Use mc-accent-green for simple, mc-accent-purple for compounding
- Show current exit toll tier as a small badge: "7% toll", "5% toll", "3% toll"
**Effort:** 2-3 hours

### 5.4 Position Sorting
**File:** `GameTracking.tsx`
**Tasks:**
- Sort positions: withdrawable (unlocked compounding) first, then by closest to unlock, then newest
**Effort:** 30 min

---

## Phase 6: Pick Your Plan Improvements

### 6.1 Min/Max Amount Buttons
**File:** `GamePlans.tsx`
**Tasks:**
- Add "Min" and "Max" pill buttons flanking the amount input
- Min: sets to 0.1 ICP
- Max: sets to user's available game balance OR the pot-relative deposit cap (`getMaxDepositLimit()`), whichever is lower
- Styled as mc-btn-pill, inline with the input
**Effort:** 1 hour

### 6.2 Remove Step Numbers
**File:** `GamePlans.tsx`
**Tasks:**
- Remove "Step 1", "Step 2", "Step 3" labels
- Use visual flow: mode selection cards → plan selection (if compounding) → amount + calculator
- Smooth reveal animation (mc-slide-up) when each section becomes relevant
**Effort:** 1-2 hours

### 6.3 Animated ROI Calculator
**File:** `GamePlans.tsx`
**Tasks:**
- Animate the projected return number with a countUp effect when input changes
- Color gradient: neutral (white) → green (modest return) → purple (high return) → gold (absurd return)
- Threshold ideas: <50% ROI = green, 50-200% = purple, >200% = gold with glow
**Effort:** 1-2 hours

---

## Phase 7: Shenanigans Enhancements

### 7.1 Merge Hall of Fame Into Shenanigans
**Files:** `Shenanigans.tsx`, `HallOfFame.tsx`
**Tasks:**
- Add a tab toggle at top of Shenanigans: "Cast" | "Leaderboards"
- "Cast" tab shows existing shenanigan cards + live feed
- "Leaderboards" tab shows HallOfFame content (top holders + top burners)
- Smooth transition between tabs
**Effort:** 2-3 hours

### 7.2 Podium Visualization for Top 3
**File:** `HallOfFame.tsx`
**Tasks:**
- Top 3 displayed as a podium: #1 center (tallest), #2 left, #3 right
- Animated glow effects for each rank (gold/silver/bronze)
- "Your Rank: #N" indicator if user is in the list
- Time-based filter toggle: "This Round" / "All Time" (stretch — requires backend support)
**Effort:** 3-4 hours

### 7.3 Shenanigan Card Grouping
**File:** `Shenanigans.tsx`
**Tasks:**
- Add filter tabs: "All" | "Offense" | "Defense" | "Chaos"
- Categorize: Offense (Money Trickster, AOE Skim, Purse Cutter, Whale Rebalance, Downline Heist), Defense (Magic Mirror, PP Booster Aura, Downline Boost), Chaos (Rename Spell, Mint Tax Siphon, Golden Name)
- Default to "All"
**Effort:** 1-2 hours

### 7.4 Contextual Cast Buttons
**File:** `Shenanigans.tsx`
**Tasks:**
- Change "Cast" button text to "Cast (X PP)" showing the cost
- Add Success/Fail/Backfire labels to the odds bar segments
**Effort:** 30 min

### 7.5 Enlarged Live Feed
**File:** `Shenanigans.tsx`
**Tasks:**
- Increase live feed from `max-h-48` to `max-h-80` (320px)
- Add animated entrance for new entries (slide-in from right or fade-in)
- Desktop: consider making feed a right-side panel alongside the cards
**Effort:** 1-2 hours

---

## Phase 8: The Pyramid (Referral) Improvements

### 8.1 Share Buttons
**File:** `ReferralSection.tsx`
**Tasks:**
- Add share buttons: Twitter/X, Telegram, WhatsApp, Facebook, QR Code
- Twitter: `https://twitter.com/intent/tweet?text=...&url=REFERRAL_LINK`
- Telegram: `https://t.me/share/url?url=REFERRAL_LINK&text=...`
- WhatsApp: `https://wa.me/?text=...REFERRAL_LINK`
- Facebook: `https://www.facebook.com/sharer/sharer.php?u=REFERRAL_LINK`
- QR Code: generate client-side using a lightweight QR library (qrcode.react or similar)
- Pre-written share text: "I'm playing Musical Chairs — a transparent Ponzi on ICP. Join with my link and we both earn PP."
- Style as icon buttons in a row, mc-btn-secondary or custom social-branded styles
**Effort:** 2-3 hours

### 8.2 Referral Milestone Badges (Stretch)
**File:** `ReferralSection.tsx`
**Tasks:**
- Show milestone badges: "3 referrals: Pyramid Initiate", "10: Scheme Architect", "25: MLM Mogul"
- Visual: locked/unlocked badge icons with progress indicators
**Effort:** 2-3 hours

---

## Phase 9: House Ledger Cleanup

### 9.1 Collapsible Dealer Info
**File:** `HouseDashboard.tsx`
**Tasks:**
- Convert the dense info cards into collapsible accordion sections
- Show one-liner summaries by default, expand on click for full details
- Redistribution Event gets special "danger" styling with flame icon
**Effort:** 1-2 hours

### 9.2 Prominent Add House Money CTA
**File:** `HouseDashboard.tsx`
**Tasks:**
- Move AddHouseMoney form to a prominent hero position at top of Dealers tab
- Style as a full-width card with "Become a Dealer → Earn 12% guaranteed*" headline
- *(Returns not guaranteed) fine print preserved
**Effort:** 1 hour

---

## Phase 10: Profile & Onboarding

### 10.1 Enhanced Profile Setup
**File:** `ProfileSetup.tsx`
**Tasks:**
- Add live preview: "Players will see you as: **[typed name]**"
- Add character count indicator (max length TBD)
- Post-submit: show celebration screen with confetti + "Welcome to Musical Chairs!" before redirecting to dashboard
**Effort:** 1-2 hours

### 10.2 Guided Tooltip Tour
**File:** New `OnboardingTour.tsx`, modified `Dashboard.tsx`
**Tasks:**
- Create a step-by-step tooltip overlay highlighting each tab
- Steps: (1) Profit Center — "Track your positions and earnings here", (2) Pick Your Plan — "Choose a game plan and deposit ICP", (3) House Ledger — "Become a dealer and fund the house", (4) Shenanigans — "Spend Ponzi Points on chaos", (5) The Pyramid — "Recruit friends for bonus PP"
- Show once per user (localStorage flag `mc_tour_completed`)
- Dismissable at any step with "Skip Tour" button
- Use a lightweight tooltip/popover positioned near each nav item
**Effort:** 3-4 hours

---

## Phase 11: Docs Section

### 11.1 In-App Docs Component
**File:** New `GameDocs.tsx`
**Tasks:**
- Comprehensive documentation page accessible from a "?" icon in the header or footer
- Sections:
  - **How It Works** (overview)
  - **Game Plans** (simple 21-day, compounding 15-day, compounding 30-day)
  - **The Math** (daily rates, compounding formulas, exit toll schedule)
  - **Dealers & House Money** (12% entitlement, fee distribution, redistribution events)
  - **Shenanigans** (all 11, costs, odds, effects, backfire mechanics)
  - **Ponzi Points** (earn rates per plan, burn mechanics, referral PP)
  - **The Pyramid** (3-level referral: L1=10%, L2=5%, L3=3%)
  - **Redistribution Events** (what triggers them, what happens)
  - **Wallet System** (II vs Plug/OISY differences)
  - **Charles** (who runs this thing)
- All math extracted from Motoko backend (documented in separate section below)
- Collapsible sections for readability
- Searchable (stretch goal)
**Effort:** 4-6 hours

### 11.2 Backend Math Documentation
**Source:** `backend/main.mo` (fully read and extracted)
**Content to document:**

**Interest Rates:**
- Simple 21-Day: 11% daily (`dailyRate = 0.11`)
- Compounding 15-Day: 12% daily (`dailyRate = 0.12`)
- Compounding 30-Day: 9% daily (`dailyRate = 0.09`)

**Earnings Formula:**
- Simple: `earnings = amount * dailyRate * (elapsedSeconds / 86400)`
- Compounding: `earnings = amount * ((1 + dailyRate)^daysElapsed - 1)`

**Exit Tolls (Simple mode only):**
- Days 0-3: 7%
- Days 3-10: 5%
- Days 10+: 3%
- Compounding plans: flat 13% Jackpot Fee at withdrawal

**Deposit Limits:**
- Minimum: 0.1 ICP
- Maximum (simple mode): max(potBalance * 0.2, 5.0 ICP)
- Rate limit: 3 positions per hour per user
- Precision: 8 decimal places max

**Ponzi Points Earn Rates:**
- Simple 21-Day: 1,000 PP per ICP
- Compounding 15-Day: 2,000 PP per ICP
- Compounding 30-Day: 3,000 PP per ICP
- House Money: 4,000 PP per ICP

**Dealer System:**
- Entitlement: deposit * 1.12 (12% bonus on principal)
- Maintenance fee on player deposits: 3%
- If sole dealer: 50% of maintenance fee returned
- Shenanigan dealer cut: 10% of PP cost distributed equally among all dealers

**Fee Distribution (via distributeFees):**
- 50% of total fees earmarked for dealer repayment
- Of that 50%: 35% to oldest upstream dealer, 25% split among other upstream dealers, 40% split among ALL dealers

**Referral Percentages:**
- Level 1 (Direct): 10% of deposit amount as PP
- Level 2: 5%
- Level 3: 3%
- Max 2 deposits counted per referral record

**Shenanigan Costs & Odds:**
| # | Name | Cost (PP) | Success | Fail | Backfire |
|---|------|-----------|---------|------|----------|
| 0 | Money Trickster | 120 | 60% | 25% | 15% |
| 1 | AOE Skim | 600 | 40% | 40% | 20% |
| 2 | Rename Spell | 200 | 90% | 5% | 5% |
| 3 | Mint Tax Siphon | 1,200 | 70% | 20% | 10% |
| 4 | Downline Heist | 500 | 30% | 60% | 10% |
| 5 | Magic Mirror | 200 | 100% | 0% | 0% |
| 6 | PP Booster Aura | 300 | 100% | 0% | 0% |
| 7 | Purse Cutter | 900 | 20% | 50% | 30% |
| 8 | Whale Rebalance | 800 | 50% | 30% | 20% |
| 9 | Downline Boost | 400 | 100% | 0% | 0% |
| 10 | Golden Name | 100 | 100% | 0% | 0% |

**Game Reset (Redistribution Event):**
- Triggered when `earnings > platformStats.potBalance` on withdrawal attempt
- All game records wiped, platform stats reset to zero
- Reset history preserved in `gameResetHistory`

**Wallet System:**
- ICP stored in e8s (1 ICP = 100,000,000 e8s)
- Deposit: ICRC-2 `transfer_from` (user must `icrc2_approve` first)
- Withdrawal: ICRC-1 `transfer` (includes 10,000 e8s transfer fee)
- Test mode: users get 500 ICP (50,000,000,000 e8s) on first access

---

## Phase 12: Trollbox

### 12.1 Trollbox Motoko Canister
**File:** New `trollbox/main.mo`
**Tasks:**
- Ring buffer of 200 messages: `{ principal, name, text, timestamp }`
- `sendMessage(text: Text)` — update call, rate limited to 1 per 5 seconds per principal
- `getMessages()` — query call, returns all messages in buffer (public, no auth required)
- `muteUser(principal: Principal)` — update call, admin-only (Charles principals)
- `getSystemMessages()` — query call for recent game events (or integrate with main backend)
- Max message length: 140 characters
- Add to `dfx.json` as a new canister
**Effort:** 3-4 hours

### 12.2 Trollbox Frontend Component
**File:** New `TrollBox.tsx`
**Tasks:**
- Desktop: 300px right panel, fixed position, full height below header+status bar
- Mobile: floating bubble (bottom-right, above tab bar), tap to expand bottom sheet
- Message feed: scrolling, auto-scroll, "Jump to latest" pill when scrolled up
- Input: monospace, mc-input styled, rotating placeholder text array
- Send button: mc-btn-primary, small
- Username colors: gold for #1 holder, silver for #2, bronze for #3, purple for everyone else
- System messages: italic, mc-text-muted, no username highlight
- Poll canister every 3-5 seconds via react-query
- Collapse/expand toggle button on desktop (persisted to localStorage)
**Effort:** 4-6 hours

### 12.3 System Event Integration
**Files:** `GameTracking.tsx`, `Shenanigans.tsx`, `GamePlans.tsx`
**Tasks:**
- After successful deposit: call trollbox canister `sendSystemMessage("[Name] deposited X ICP")`
- After withdrawal: `"[Name] withdrew X ICP"`
- After shenanigan cast: `"[Name] cast [Shenanigan] — [Outcome]!"`
- System messages use a dedicated canister method (not the user `sendMessage`)
**Effort:** 2-3 hours

### 12.4 Admin Mute in Charles Panel
**File:** `ShenanigansAdminPanel.tsx` (or new `CharlesPanel.tsx`)
**Tasks:**
- Add "Mute User" section to Charles panel
- Input for principal ID + mute duration
- Calls trollbox canister `muteUser`
**Effort:** 1 hour

### 12.5 Layout Adjustment for Trollbox
**Files:** `Dashboard.tsx`, `index.css`
**Tasks:**
- Desktop: main content area width reduced by 300px when trollbox is expanded
- Add CSS transition for smooth expand/collapse
- Mobile: no layout change (trollbox overlays as bottom sheet)
**Effort:** 1-2 hours

---

## Phase 13: Cross-Cutting Polish

### 13.1 Typography Consistency Audit
**Files:** All components
**Tasks:**
- Ensure ALL section headers use `font-display` (Bungee)
- Ensure ALL subtitles/taglines use `font-accent` (Fredoka One)
- Ensure ALL body/buttons use `font-body` (Space Mono) — verify Tailwind utility classes map correctly
**Effort:** 1-2 hours

### 13.2 Mobile Bottom Sheet for Modals
**Files:** `WalletDropdown.tsx`, dialog components
**Tasks:**
- On mobile (below 768px), convert positioned dropdowns to bottom sheets
- Reuse the existing `.mc-more-sheet` pattern (slide-up from bottom with overlay)
**Effort:** 2-3 hours

### 13.3 Functional Animations
**Files:** Various
**Tasks:**
- Add countUp animation utility for numbers (P/L, pot size, PP balance)
- Add shake animation for invalid form submission (CTA button + error message)
- Add pulse animation on notification badge when count changes
- Respect `prefers-reduced-motion` globally
**Effort:** 2-3 hours

### 13.4 "Charles" Personality Touches
**Files:** Various
**Tasks:**
- Find places to sneak in Charles references:
  - House Ledger info: "Charles takes a 3% maintenance fee on every deposit"
  - Exit toll info: "Charles collects a 7% exit toll if you leave within 3 days"
  - Redistribution Event: "When the pot runs dry, Charles resets the table"
  - Error states: "Even Charles couldn't fix this one"
  - Loading states: "Charles is counting the money..."
  - Trollbox system messages: "Charles is watching."
  - Admin panel header: "Charles's Office"
- Charles's visual: crown or top-hat icon from lucide-react, styled with mc-text-gold
**Effort:** 1-2 hours

---

## Estimated Total Effort

| Phase | Hours |
|-------|-------|
| 1: Critical Fixes | 3-5 |
| 2: Navigation | 5-7 |
| 3: Status Bar | 4-5 |
| 4: Splash Page | 6-10 |
| 5: Profit Center | 5-7 |
| 6: Pick Your Plan | 3-5 |
| 7: Shenanigans | 8-12 |
| 8: The Pyramid | 4-6 |
| 9: House Ledger | 2-3 |
| 10: Onboarding | 4-6 |
| 11: Docs | 4-6 |
| 12: Trollbox | 11-16 |
| 13: Polish | 6-10 |
| **Total** | **~65-98 hours** |

---

## Suggested Execution Order

1. Phase 1 (fixes) — unblocks everything
2. Phase 2 (navigation) — restructures the skeleton
3. Phase 3 (status bar) — framework for persistent data
4. Phase 5 (profit center) — most-visited page, biggest impact
5. Phase 6 (pick your plan) — second most-visited
6. Phase 7 (shenanigans + HoF merge) — most fun page
7. Phase 9 (house ledger) — cleanup
8. Phase 8 (pyramid/referral) — growth feature
9. Phase 4 (splash) — first impression
10. Phase 10 (onboarding) — new user flow
11. Phase 11 (docs) — comprehensive reference
12. Phase 12 (trollbox) — biggest new feature, saved for last
13. Phase 13 (polish) — ongoing throughout

---

*Awaiting owner approval before any work begins.*
