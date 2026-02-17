## Phase C — Design System & Doc Updates

*Resolves: B-40, B-41, B-42, B-43, B-44, B-45, B-46*

Found by auditing the UX Design Philosophy doc's rules against the current codebase and by promoting criticisms buried in the v2 self-evaluation prose to tracked items.

### #15 — Header tab spacing violation (B-40)

**File:** `frontend/src/index.css`

**Problem:** The Design Philosophy says "Never less than `--space-sm` (8px) between interactive elements." The `.mc-header-tabs` container uses `gap: 2px` (line 487) between individually clickable tab buttons. 2px is a 75% violation of the minimum.

**Fix:** Increase the gap to at least `8px` (or `var(--space-sm)`). This will consume more horizontal space, so combine with Phase 17.2's font size work and Phase 1.6's overflow handling:
```css
.mc-header-tabs {
  gap: var(--space-sm); /* 8px — Design Philosophy minimum */
}
```

If 8px causes overflow at narrow widths, use `6px` as a pragmatic compromise and note the deviation.

**Effort:** 5 min (but test alongside 17.2)

### #16 — Remove perpetual tagline bob animation (B-41)

**File:** `frontend/src/index.css`

**Problem:** The Design Philosophy explicitly says "Bad: perpetual bounce on decoration." The tagline has `animation: mc-tagline-bob 3s ease-in-out infinite` — a perpetual bounce on static decoration. Phase 8.2 adds a typewriter effect to the same element. The bob and typewriter can't coexist.

**Fix:** Remove the `mc-tagline-bob` animation. When Phase 8.2 adds the typewriter effect, that becomes the tagline's animation (purposeful — conveys the text appearing). After the typewriter completes, the tagline is static. No more perpetual bob.

```css
/* DELETE these lines: */
/* animation: mc-tagline-bob 3s ease-in-out infinite; */
/* @keyframes mc-tagline-bob { ... } */
```

Keep the `rotate(-3deg)` transform (that's a static style choice, not an animation).

**Effort:** 5 min

### #17 — Color token audit (B-42)

**Files:** All component files in `frontend/src/components/`

**Problem:** Several components use Tailwind color classes (`bg-purple-500`, `bg-green-500`, `text-yellow-400`) instead of the `mc-text-*` / `mc-bg-*` design system tokens. This was noted in the v2 self-eval Phase 3 prose but never promoted to a tracked item. Known violations in `GameTracking.tsx`:
- `bg-purple-500` for compounding position bars
- `bg-green-500` for simple position bars
- `bg-purple-500/20` / `bg-green-500/20` for toll and plan type badges
- `text-yellow-400`, `text-red-400`, `text-green-400` in `getTollBadgeClasses()`

**Fix:** Grep the codebase for Tailwind color classes that have `mc-*` equivalents and replace them:
- `bg-purple-500` → Create `mc-bg-purple` utility or use inline `style={{ backgroundColor: 'var(--mc-purple)' }}`
- `bg-green-500` → `mc-bg-green`
- `text-green-400` → `mc-text-green`
- `text-yellow-400` → `mc-text-gold`
- `text-red-400` → `mc-text-danger`

Add the missing `mc-bg-*` utilities to `index.css` if they don't exist:
```css
.mc-bg-green { background: var(--mc-neon-green); }
.mc-bg-purple { background: var(--mc-purple); }
.mc-bg-gold { background: var(--mc-gold); }
```

Also audit `.mc-card-select` class names (`active-green`, `active-purple`, `active-gold`) which lack the `mc-` prefix. Rename to `mc-active-green`, etc.

**Effort:** 1 hour

### #18 — Header content density — design evaluation (B-43)

**Problem:** The v2 self-eval raised this as a design concern beyond just overflow: "that's a lot of content for one horizontal bar." C-6 and B-34 treat it as a sizing/overflow problem. But should the header have fewer elements?

**Options to evaluate:**
1. **Remove tagline from header on desktop.** The "It's a Ponzi!" tagline is brand flavor but takes ~100px of horizontal space in the header. On the splash page it's prominent. Once logged in, it's redundant — the user already knows what this is.
2. **Charles button visible only for admin principals.** Non-admin users see a button that does nothing useful. Removing it saves ~40px.
3. **Collapse logo to icon at medium widths.** At 769-1024px, show just "MC" or a small logo mark instead of the full "Musical Chairs" text.

**Implementation:** This is a design decision, not a code fix. The v3 plan should note that the header density needs a design review. The most impactful change is #1 (remove tagline when logged in):
```tsx
{!isAuthenticated && <span className="mc-tagline">It's a Ponzi!</span>}
```

**Effort:** 15 min for option 1, design review needed for options 2-3

### #19 — Update Design Philosophy doc (B-44)

**File:** `docs/UX_DESIGN_PHILOSOPHY.md` (main repo)

**Problem:** The Design Philosophy doc — whose stated purpose is "For any agent continuing UX/frontend work on this project" — still prescribes:
- "Left rail navigation (200px, always labeled)" — deleted in v2
- "Trollbox panel: fixed right, ~300px, always visible" — deferred indefinitely
- Layout breakpoints assume sidebar + content + trollbox

**Fix:** Update the Layout Rules section to reflect reality:
- **Desktop:** Header tab navigation (replaces sidebar). Main content area `max-w-4xl` / `max-w-7xl`. Status bar below header.
- **Mobile:** Bottom tab bar (unchanged). Status bar compact mode.
- **Trollbox:** Deferred. Remove from layout rules, add a note in Trollbox Integration Notes that this is planned but not implemented.
- **Breakpoints:** `769px`: header tabs visible, bottom tabs hidden. Below `769px`: bottom tabs, header tabs hidden. Remove the trollbox-related breakpoints.

**Effort:** 20 min

### #20 — MobileSheet drag-to-dismiss (B-45)

**Files:** `frontend/src/components/GameTracking.tsx`, `frontend/src/components/Shenanigans.tsx`

**Problem:** Phase 1.2 (#2) fixes drag-to-dismiss for WalletDropdown specifically. When a reusable `MobileSheet` wrapper is created (#51, B-30), it must include the same touch event logic by default, or every new bottom sheet will ship with a decorative-only drag handle (the exact bug C-2 identified).

**Fix:** Include the drag-to-dismiss touch handlers from Phase 1.2 in the reusable `MobileSheet` component — not just the visual drag handle. The critical point: decorative-only drag handles are the bug, not the feature. The reusable component must bake in the touch event logic (`onTouchStart`, `onTouchMove`, `onTouchEnd` with threshold-based dismiss) so that every consumer gets working drag-to-dismiss for free.

The `MobileSheet` component's drag handle area should have:
```tsx
<div className="mc-drag-handle"
  onTouchStart={handleTouchStart}
  onTouchMove={handleTouchMove}
  onTouchEnd={handleTouchEnd}
/>
```

Where `handleTouchStart` / `handleTouchMove` / `handleTouchEnd` implement the same logic as Phase 1.2: track startY, apply `translateY`, dismiss if dragged > 30% of height.

**Effort:** Included in #51 (B-30) implementation time

### #21 — countUp resetToken as built-in hook pattern (B-46)

**File:** `frontend/src/hooks/useCountUp.ts`

**Problem:** The `resetToken` pattern (introduced in Phase 5.1, item #27 / A-10) is useful beyond just tab-switch re-animation. Currently each consumer must implement it independently.

**Fix:** The `resetToken` pattern should be built into `useCountUp` itself as an optional parameter rather than requiring each consumer to implement it independently. Consider adding a `resetOnChange` dependency array or an IntersectionObserver-based auto-reset so all future usages get re-animation for free.

This means `useCountUp` would accept:
```ts
export function useCountUp(
  target: number,
  duration?: number,
  resetToken?: number,
  options?: { autoResetOnVisible?: boolean }
): number
```

With `autoResetOnVisible`, the hook internally uses IntersectionObserver to detect when the element scrolls into view and triggers re-animation. This eliminates the need for consumers to manage visibility detection themselves.

**Effort:** 30 min (if done as enhancement to #27)

