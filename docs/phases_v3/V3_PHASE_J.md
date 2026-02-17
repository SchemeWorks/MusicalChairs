## Phase J — Animations, Accessibility & Audits

*Resolves: B-7, B-25, B-9, B-11, B-24*

Animated ROI calculator, prefers-reduced-motion support, typography audit, mobile responsive audit, and pull-to-refresh. These are all 'pass over the whole app' tasks that benefit from being done together — they touch every component and are best done after all per-component work is complete.

### #54 — Animated ROI calculator (B-7)

**File:** `frontend/src/components/GamePlans.tsx`

**Problem:** The ROI calculator updates instantly with no visual feedback. The original report wanted animated countUp and color shifts.

**Fix:**
1. Apply `useCountUp` to the projected return number in the ROI card. **Important:** Use the `resetToken` pattern from Phase 5.1 (#27) so the animation replays when inputs change, not just on first load. Pass a token derived from the selected plan/mode/amount so the countUp resets when any input changes.
2. Add color transitions based on ROI percentage:
```
const roiColor = roiPercent < 50 ? 'mc-text-green' :
                 roiPercent < 200 ? 'mc-text-purple' :
                 'mc-text-gold mc-glow-gold';
```
3. Add a brief scale animation when the ROI value changes significantly:
```css
.mc-roi-pop {
  animation: mc-roi-pop 0.3s ease-out;
}
@keyframes mc-roi-pop {
  0% { transform: scale(1); }
  50% { transform: scale(1.05); }
  100% { transform: scale(1); }
}
```

The Phase 5.1 connection: this item consumes the `resetToken` pattern that #27 (A-10) introduces into `useCountUp`.

**Effort:** 30 min

### #55 — `prefers-reduced-motion` support (B-25)

**File:** `frontend/src/index.css`, `frontend/src/hooks/useCountUp.ts`

**Problem:** None of the v2 animations respect `prefers-reduced-motion`. This is an accessibility requirement.

**Fix:** Add a global media query in `index.css`:
```css
@media (prefers-reduced-motion: reduce) {
  .mc-hero-entrance,
  .mc-stagger > *,
  .mc-scroll-animate,
  .mc-shake,
  .mc-badge-pulse,
  .mc-typewriter,
  .mc-splash-bg,
  .mc-redistribution-pulse .lucide-flame,
  .mc-roi-pop {
    animation: none !important;
    transition: none !important;
  }
  .mc-scroll-animate {
    opacity: 1 !important;
    transform: none !important;
  }
  .mc-typewriter {
    width: 100% !important;
    border-right: none !important;
  }
}
```

In `useCountUp.ts`, check the preference:
```ts
const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
if (prefersReducedMotion) {
  setValue(target);
  return;
}
```

This ensures all animations are disabled for users who have requested reduced motion.

**Effort:** 30 min

### #56 — Typography consistency audit (B-9)

**Files:** All components

**Problem:** The original report specified a strict typography hierarchy: Bungee for headers, Fredoka One for taglines, Space Mono for everything else. This was never audited.

**Fix:** Grep through all components and verify:
- Every section header (`h2`, `h3`, major labels) uses `font-display` (Bungee)
- Taglines and snarky subtitles use `font-accent` (Fredoka One)
- All body text, buttons, inputs, labels use `font-body` (Space Mono)

Create a checklist and fix each violation. Common issues to look for:
- `font-bold` on a header that should be `font-display`
- Regular text using Bungee where it shouldn't
- Inconsistent `font-display` vs `text-xl font-bold` in different components

**Effort:** 1-2 hours

### #57 — Dedicated mobile responsive audit (B-11)

**Scope:** All components, tested at 375px (iPhone SE), 390px (iPhone 14), 768px (iPad)

**Problem:** Mobile was never holistically evaluated. Individual pieces work (bottom tabs, bottom sheet) but the overall experience was never reviewed.

**Checklist:**
- [ ] Status bar: at 375px, is it readable? Are 4 stats too many?
- [ ] Splash page: do cards stack properly? Is text readable?
- [ ] ProfileSetup: does the card fit? Is the input usable?
- [ ] Profit Center: do position cards stack? Is the P/L hero readable?
- [ ] GamePlans: does the two-column ROI layout stack to one column?
- [ ] Shenanigans: do filter tabs wrap? Do 3-column cards go to 1-column?
- [ ] HouseDashboard: does the accordion work? Is the ledger timeline readable?
- [ ] WalletDropdown: does the bottom sheet fit? Is content scrollable?
- [ ] Overall: excessive horizontal scrolling? Truncated text? Touch targets too small (<44px)?

Fix each issue found. Common mobile fixes:
- `text-sm` -> `text-xs` on mobile
- `grid-cols-3` -> `grid-cols-1` below 640px
- `gap-6` -> `gap-3` on mobile
- Min touch target: `min-h-[44px] min-w-[44px]`

**Effort:** 2-3 hours

### #58 — Pull-to-refresh on mobile (B-24)

**Problem:** No pull-to-refresh anywhere. Standard mobile game pattern.

**Fix:** Create a `usePullToRefresh` hook:
```ts
function usePullToRefresh(onRefresh: () => Promise<void>) {
  // Track touch start/move/end on the main content area
  // When at scroll top and user pulls down > 60px:
  //   1. Show a spinner indicator at top
  //   2. Call onRefresh()
  //   3. Hide spinner when promise resolves
}
```

Apply to the Dashboard's main content area. `onRefresh` should call `queryClient.invalidateQueries()` to refetch all data.

**Alternative (simpler):** Use a small library like `react-pull-to-refresh` if available. Or skip the custom gesture and add a visible "Refresh" button at the top of the content area on mobile (less elegant but functional).

**Effort:** 2-3 hours (custom), 30 min (library or button fallback)

