## Phase A — Bug Fixes & Regressions

*Resolves: C-1, C-2, C-3, A-7, C-4, C-5, C-6*

These are things the v2 broke. Fix them first. A-7 (missing proceed button) is bundled here because it addresses the same component as C-3.

### #1 — Shenanigans filter empty state (C-1)

**File:** `frontend/src/components/Shenanigans.tsx`

**Problem:** When a category filter (Offense/Defense/Chaos) returns zero matching cards, the user sees blank space.

**Fix:** After the filtered cards grid, add a conditional empty state:
```
{filteredShenanigans.length === 0 && (
  <div className="text-center py-12">
    <Dices className="h-10 w-10 mc-text-muted mb-3 mx-auto" />
    <p className="mc-text-dim text-sm">No shenanigans in this category.</p>
    <button onClick={() => setFilterCategory('all')} className="mc-text-purple text-xs mt-2 hover:underline">
      Show all
    </button>
  </div>
)}
```

**Effort:** 10 min

### #2 — WalletDropdown drag handle — add touch events (C-2)

**File:** `frontend/src/components/WalletDropdown.tsx`

**Problem:** The mobile bottom sheet has a visual drag handle but no touch interaction. Users expect swipe-to-dismiss.

**Fix:** Add touch event handlers to the drag handle area:
- `onTouchStart` — record starting Y position
- `onTouchMove` — track delta Y, apply `transform: translateY(${deltaY}px)` to the sheet (only positive/downward)
- `onTouchEnd` — if delta > 100px, close the sheet; otherwise snap back with transition
- Add `will-change: transform` to `.mc-bottom-sheet` for smooth transforms

State needed: `touchStartY: number | null`, `dragDelta: number`

Apply the same pattern to any future bottom sheet components.

**Effort:** 45 min

### #3 — ProfileSetup celebration — explicit refetch (C-3)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** The celebration `onSuccess` callback never calls `queryClient.invalidateQueries()`. The `setTimeout` callback is empty. If React Query is slow, the user is stuck with no way to proceed.

**Fix (two changes):**
1. In the `onSuccess` handler, add `queryClient.invalidateQueries({ queryKey: ['userProfile'] })` to force a refetch
2. Replace the empty `setTimeout` callback with a fallback that also invalidates the query

**Effort:** 15 min

### #4 — ProfileSetup celebration — proceed button (A-7)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** If React Query is slow after the celebration, the user has no way to proceed manually.

**Fix:** Add a "TAKE ME TO THE TABLE" button below the spinner text:
```
<button
  onClick={() => queryClient.invalidateQueries({ queryKey: ['userProfile'] })}
  className="mc-btn-primary mt-4"
>
  TAKE ME TO THE TABLE
</button>
```

This gives users an active escape hatch and ensures the profile query is explicitly refreshed.

**Effort:** 5 min

### #5 — MAX button disable on zero balance (C-4)

**File:** `frontend/src/components/GamePlans.tsx`

**Problem:** MAX button is clickable even when wallet balance is 0 or below minDeposit. It sets amount to "0", which fails validation.

**Fix:** Add `disabled` prop to the MAX button:
```
disabled={!walletBalance || walletBalance < minDeposit}
```
Also add `opacity-50 cursor-not-allowed` when disabled. Same for MIN button if minDeposit > walletBalance.

**Effort:** 10 min

### #6 — HallOfFame podium with 2 entries (C-5)

**File:** `frontend/src/components/HallOfFame.tsx`

**Problem:** Podium layout assumes 3 entries. With exactly 2, there's an empty gap where 3rd place would be.

**Fix:** Conditional rendering based on entry count:
- 0-1 entries: skip the podium entirely, render as a simple list
- 2 entries: render a 2-column podium (1st center-left, 2nd center-right, no 3rd column)
- 3+ entries: current 3-column podium layout

Alternative (simpler): if `top3.length < 3`, don't use the reorder trick. Just render them in a row with the first entry taller. Add a `min-h` to the podium container so it doesn't collapse awkwardly.

**Effort:** 20 min

### #7 — Header tabs overflow handling (C-6)

**File:** `frontend/src/index.css`

**Problem:** `.mc-header-tabs` has no overflow handling. At 769px-1024px desktop widths, tabs likely overflow or push controls off-screen.

**Fix (multi-pronged):**
1. Add `min-width: 0` to `.mc-header-tabs` (allows flex shrink)
2. Add `flex-shrink: 1` and `overflow: hidden` to the tab container
3. Reduce tab font from `11px` to `10px` at a `@media (max-width: 1024px)` breakpoint
4. Reduce tab padding from `6px 12px` to `4px 8px` at that breakpoint
5. Add `text-overflow: ellipsis` to tab labels as a last resort
6. **Test visually at 769px, 900px, 1024px, 1200px** — this is the phase that absolutely requires visual verification

Alternative: on narrow desktop (769-900px), hide tab labels and show icon-only tabs, similar to mobile. This gives more breathing room without dropping any tabs.

**Effort:** 1 hour (including visual testing)

