## Phase K — Onboarding & Documentation

*Resolves: B-5, B-12*

### #59 — Guided tooltip onboarding tour (B-5)

**File:** New `frontend/src/components/OnboardingTour.tsx`, `frontend/src/components/Dashboard.tsx`

**Problem:** New users see 5 tabs with no guidance. No "here's how to play" flow.

**Fix:** A lightweight tooltip overlay that walks through each tab:

**Steps:**
1. "This is your Profit Center" -> highlights Profit Center tab
2. "Start here to invest" -> highlights Invest tab
3. "Become a dealer" -> highlights Seed Round tab
4. "Recruit friends" -> highlights MLM tab
5. "Spend your Ponzi Points" -> highlights Shenanigans tab

**Implementation:**
```tsx
const TOUR_STEPS = [
  { tab: 'profitCenter', title: 'Profit Center', desc: 'Track your positions and see your P/L.' },
  { tab: 'invest', title: '"Invest"', desc: 'Choose a plan and deposit ICP to start earning.' },
  { tab: 'seedRound', title: 'Seed Round', desc: 'Back the house as a dealer. Earn 12%*.' },
  { tab: 'mlm', title: 'MLM', desc: 'Recruit friends. Three-level pyramid. Charles approves.' },
  { tab: 'shenanigans', title: 'Shenanigans', desc: 'Spend Ponzi Points on cosmetic chaos.' },
];
```

Show if `!localStorage.getItem('mc_tour_completed')`. Render as a floating tooltip positioned near each tab with a "Next" button and "Skip Tour" link. On complete/skip, set `localStorage.setItem('mc_tour_completed', 'true')`.

Use `position: absolute` positioned relative to each tab button. No external library needed.

**Effort:** 2-3 hours

### #60 — In-app docs page (B-12)

**File:** New `frontend/src/components/GameDocs.tsx`, `frontend/src/App.tsx`

**Problem:** No documentation exists in the app. Users must understand Ponzi mechanics, lock-up periods, exit tolls, shenanigan odds, and PP earn rates with no explanation.

**Fix:** Create a comprehensive docs component accessible from a "?" icon in the header:

**Sections (collapsible accordion):**
1. **How It Works** — overview of deposit -> earn -> withdraw cycle
2. **Game Plans** — Simple 21-day (11%/day), Compounding 15-day (12%/day), Compounding 30-day (9%/day)
3. **Exit Tolls** — Day 0-3: 7%, Day 3-10: 5%, Day 10+: 3%. Compounding: 13% Jackpot Fee
4. **Dealers & Seed Round** — 12% entitlement, 3% maintenance fee, fee distribution
5. **Shenanigans** — all 11 types with costs and odds (reference the table from v1 task list)
6. **Ponzi Points** — earn rates per plan, what you can spend them on
7. **The Pyramid** — L1: 10%, L2: 5%, L3: 3%
8. **Redistribution Events** — what triggers them, what happens
9. **Wallet System** — II vs Plug/OISY differences

**Access:** Add a `?` icon button in the header (between logo and tabs, or in the right controls area). Clicking opens the docs as a full-page overlay or a new tab view.

```tsx
// In App.tsx header:
<button
  onClick={() => setShowDocs(!showDocs)}
  className="mc-btn-secondary w-8 h-8 rounded-full flex items-center justify-center text-xs"
>
  ?
</button>
```

**Effort:** 4-6 hours (content writing + component)

