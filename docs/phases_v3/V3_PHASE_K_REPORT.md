# Phase K — Onboarding & Documentation — REPORT

*Resolves: B-5, B-12*

**TS errors:** 44 total (all pre-existing) · 0 new

---

## #59 — Guided tooltip onboarding tour (B-5)

**Problem:** New users see 5 tabs with no guidance. No "here's how to play" flow.

**Solution:**
1. Created `OnboardingTour.tsx` component with 5-step tour:
   - Step 1: "Profit Center" — Track your positions and see your P/L.
   - Step 2: ""Invest"" — Choose a plan and deposit ICP to start earning.
   - Step 3: "Seed Round" — Back the house as a dealer. Earn 12%*.
   - Step 4: "MLM" — Recruit friends. Three-level pyramid. Charles approves.
   - Step 5: "Shenanigans" — Spend Ponzi Points on cosmetic chaos.
2. Each step navigates to the corresponding tab via `onTabChange`
3. Tour triggers on first visit — checks `localStorage.getItem('mc_tour_completed')`
4. Shows after a 600ms delay to let the dashboard render first
5. UI: floating tooltip with dark overlay backdrop, progress bar, step counter, "Next"/"Done" button, "Skip Tour" link
6. On complete/skip: sets `localStorage.setItem('mc_tour_completed', 'true')` and returns to Profit Center tab
7. Position: centered horizontally, bottom-positioned on mobile (above bottom tabs), top-positioned on desktop
8. Integrated into `Dashboard.tsx` — renders inside the dashboard component, receives `handleTabChange` and `isMobile` props

**Files:** `OnboardingTour.tsx` (new), `Dashboard.tsx`

---

## #60 — In-app docs page (B-12)

**Problem:** No documentation exists in the app. Users must understand Ponzi mechanics, lock-up periods, exit tolls, shenanigan odds, and PP earn rates with no explanation.

**Solution:**
1. Created `GameDocs.tsx` component — full-page overlay with collapsible accordion sections
2. **9 sections** covering all game mechanics:
   - **How It Works** — deposit → earn → withdraw → reset cycle overview
   - **Game Plans** — Simple 21-Day (11%/day), Compounding 15-Day (12%/day), Compounding 30-Day (9%/day) with styled cards per plan
   - **Exit Tolls** — Day 0-3: 7%, Day 3-10: 5%, Day 10+: 3%, Compounding: 13% Jackpot Fee (presented as a table)
   - **Dealers & Seed Round** — 12% entitlement, 3% maintenance fee, pro-rata distribution
   - **Shenanigans** — All 11 types organized by category (Offense/Defense/Chaos) with PP costs and success rates
   - **Ponzi Points** — Earn rates per plan, spending, referral earning
   - **The Pyramid (MLM)** — L1: 10%, L2: 5%, L3: 3% (presented as a table)
   - **Redistribution Events** — Triggers, consequences, signals
   - **Wallet System** — Internet Identity vs Plug/OISY differences
3. "How It Works" section opens by default; others collapsed
4. Charles quote at the top: "Knowledge is power. Power is money. Money is what you're about to lose."
5. Responsible gambling disclaimer at the bottom
6. **Access:** `HelpCircle` icon button in the header right controls area (between Charles admin button and Wallet button). 8×8 rounded-full button.
7. Opens as a fixed full-screen overlay (`z-50`) with sticky header and close button
8. State managed in `App.tsx` via `showDocs` / `setShowDocs`

**Files:** `GameDocs.tsx` (new), `App.tsx`

---

## Files Modified

| File | Tasks |
|------|-------|
| `frontend/src/components/OnboardingTour.tsx` | #59 (new) |
| `frontend/src/components/Dashboard.tsx` | #59 |
| `frontend/src/components/GameDocs.tsx` | #60 (new) |
| `frontend/src/App.tsx` | #60 |

## Spec Coverage Audit

| # | Title | Spec'd | Implemented | Notes |
|---|-------|--------|-------------|-------|
| 59 | Guided tooltip onboarding tour | 5-step localStorage-gated tour, tooltip positioned near tabs, Next/Skip | ✅ All | Centered floating tooltip with backdrop overlay, progress bar, tab navigation |
| 60 | In-app docs page | Collapsible accordion, ? icon access, 9 sections of game mechanics | ✅ All | 9 sections (plan spec listed 9), HelpCircle icon in header, full-page overlay |
