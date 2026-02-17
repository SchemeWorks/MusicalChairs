# Phase H — Secondary Tab Enrichment — REPORT

*Resolves: B-17, B-21, B-15*

**TS errors:** 44 total (all pre-existing) · 0 new

---

## #44 — "Popular Now" / Trending indicator (B-17)

**Problem:** No indication of which shenanigans other players are casting most frequently.

**Solution:**
1. Computed `mostPopularType` from `recentShenanigans` feed by counting `shenaniganType` occurrences
2. Requires minimum 3 entries in the feed before showing a badge (avoids noise from sparse data)
3. Sorts by count descending, takes the top entry
4. Compares `String(trick.type)` against `mostPopularType` for each card
5. Badge: `absolute -top-2 -right-2`, orange pill with 🔥 emoji, `z-10` to sit above the aura pseudo-element
6. Card already has `position: relative` and `overflow: visible` in CSS — no CSS changes needed

**Files:** `Shenanigans.tsx`

---

## #45 — PP earn rates as visual comparison table (B-21)

**Problem:** PP earn rates not presented anywhere — users don't know how deposits translate to PP.

**Solution:**
1. Added new "Ponzi Points" section to GameTracking (Profit Center) after the House info card
2. Section contains PP balance display (large purple glow number) and 3-column earn rates grid
3. Three cards showing rates per plan:
   - **Simple 21-day:** 1,000 PP per ICP (green)
   - **Compound 15-day:** 2,000 PP per ICP (purple)
   - **Compound 30-day:** 3,000 PP per ICP (gold)
4. Visual, scannable — makes the deposit→PP tradeoff clear at a glance
5. Uses `mc-card-elevated` wrapper and `mc-card` for each rate card

**Files:** `GameTracking.tsx`

---

## #46 — PP activity feed and spending suggestions (B-15)

**Problem:** PP content is thin — just a number. No breakdown, no context, no suggestions.

**Solution:**
1. **PP source breakdown** — Shows where PP came from:
   - From deposits: `+{depositPoints} PP` (green)
   - From referrals: `+{referralPoints} PP` (cyan)
   - Spent on shenanigans: `−{computed} PP` (red) — computed as `(depositPoints + referralPoints) - totalPoints`
   - Spent line only shows when amount > 0
2. **Spending suggestions** — Shows when PP ≥ 100:
   - Imports `useGetShenaniganConfigs` to get shenanigan names and costs
   - Filters configs to those affordable at current PP balance
   - Sorts by cost ascending, shows up to 3 cheapest affordable options
   - Each shown as a small `mc-card` pill with name and cost
3. Both subsections sit inside the PP section below the earn rates table

**Files:** `GameTracking.tsx`

---

## Files Modified

| File | Tasks |
|------|-------|
| `frontend/src/components/Shenanigans.tsx` | #44 |
| `frontend/src/components/GameTracking.tsx` | #45, #46 |

## Spec Coverage Audit

| ID | Item | Status |
|----|------|--------|
| B-17 | "Popular Now" / Trending indicator | ✅ Computed from recent feed, orange badge on most-cast card, min 3 entries |
| B-21 | PP earn rates as visual comparison | ✅ 3-column grid in Profit Center PP section, color-coded per plan |
| B-15 | PP activity feed + spending suggestions | ✅ Source breakdown (deposit/referral/spent) + affordable shenanigans list |
