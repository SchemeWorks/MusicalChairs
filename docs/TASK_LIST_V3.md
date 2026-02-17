# UX Task List v3

*Reordered from UX Implementation Plan v3. Every one of the 63 tracked items has its own numbered heading (#1 through #63), grouped into 12 lettered phases (A through L).*

*Source of truth: `docs/UX_SELF_EVALUATION_V2.md`*

---

## Guiding Principles (unchanged from v2)

1. **"If I were designing this from scratch, what would I do?"**
2. **Show less, do more, feel better.**
3. **Structure before copy.**
4. **The self-evaluation is the standard.** — Every identified gap must be closed or explicitly deferred with a reason.

---

## How This Plan Is Organized

The v2 self-evaluation identified 63 issues across three categories:

- **Section A (items 1-11):** Things the v2 plan specified but were not implemented correctly
- **Section B (items 1-46):** Things the original report, v2 plan, or Design Philosophy called for that were dropped at various stages
- **Section C (items 1-6):** Bugs and regressions introduced by the v2 work

This task list groups them into 12 lettered implementation phases (A through L) by area, not by origin, reordered for execution priority. Each of the 63 items has its own `###` heading numbered sequentially from #1 to #63.

---

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

---

## Phase B — v2 Spec Compliance

*Resolves: B-33, B-34, B-35, B-36, B-37, B-38, B-39*

These items are specific sub-items from the v2 plan itself that were neither implemented to spec nor tracked for future work. Found by cross-referencing every v2 plan sub-item against the actual codebase.

### #8 — Status bar P/L glow effect (B-33)

**File:** `frontend/src/components/GameStatusBar.tsx`

**Problem:** The v2 plan specified `mc-glow-green` on the P/L stat when positive. `GameTracking.tsx` correctly uses `mc-glow-green` on its P/L hero number (line 306), but `GameStatusBar.tsx` only uses `mc-text-green` with no glow. The same data point (net P/L) is styled differently in the two places it appears.

**Fix:** Change line 31 of GameStatusBar.tsx from:
```
className={`mc-status-bar-value ${isUp ? 'mc-text-green' : 'mc-text-danger'}`}
```
to:
```
className={`mc-status-bar-value ${isUp ? 'mc-text-green mc-glow-green' : 'mc-text-danger'}`}
```

**Effort:** 2 min

### #9 — Header tab base font size spec compliance (B-34)

**File:** `frontend/src/index.css`

**Problem:** The v2 plan specified `13px` for header tab font size (Phase 2, line 107). The implementation uses `11px` (line 503 of index.css). The v3 Phase 1.6 proposes shrinking further to `10px` at narrow breakpoints — moving in the opposite direction.

**Fix:** This requires a design decision: the v2 spec said 13px, the implementation chose 11px (likely because 13px caused overflow at narrower widths), and the v3 plan proposes 10px. The honest resolution is:
1. Try `12px` as a compromise between spec (13px) and current (11px)
2. At `@media (max-width: 1024px)`, drop to `11px`
3. At `@media (max-width: 900px)`, drop to `10px` or switch to icon-only
4. Verify at 769px, 900px, 1024px, 1200px breakpoints

This supersedes the approach in v3 Phase 1.6. Do both items together.

**Effort:** 30 min (including visual testing)

### #10 — Podium avatar/initial circles (B-35)

**File:** `frontend/src/components/HallOfFame.tsx`

**Problem:** The v2 plan specified "Avatar/initial circle" on each podium block. The implementation uses medal icons (Crown, Medal) inside colored circles. The code comment on line 42 says "Avatar + name" but the content is a medal icon, not the player's initial.

**Fix:** Replace the medal icon in the circle with the player's first letter initial. Move the medal/crown to a smaller badge overlapping the circle:
```tsx
<div className={`w-10 h-10 rounded-full ${m.bg} border ${m.border} flex items-center justify-center mb-1.5 relative`} style={{ boxShadow: m.glow }}>
  <span className={`font-display text-sm ${m.text}`}>{entry.name.charAt(0).toUpperCase()}</span>
  <div className="absolute -top-1 -right-1 w-4 h-4 flex items-center justify-center">
    {m.icon}
  </div>
</div>
```

This shows user identity (initial) as primary and rank identity (medal) as a badge — matching the v2 plan's intent while keeping the rank medals.

**Effort:** 15 min

### #11 — QR code download button (B-36)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** The v2 plan specified a downloadable QR ("wrap in a canvas and provide a 'Download QR' button"). The v3 Phase 7.1 adds QR display but not download.

**Fix:** Use `qrcode.react`'s `QRCodeCanvas` (not `QRCodeSVG`) and add a download handler:
```tsx
import { QRCodeCanvas } from 'qrcode.react';

const qrRef = useRef<HTMLCanvasElement>(null);

const downloadQR = () => {
  const canvas = qrRef.current;
  if (!canvas) return;
  const url = canvas.toDataURL('image/png');
  const link = document.createElement('a');
  link.href = url;
  link.download = 'musical-chairs-referral-qr.png';
  link.click();
};

<QRCodeCanvas ref={qrRef} value={referralLink} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
<button onClick={downloadQR} className="mc-btn-secondary text-xs mt-2 flex items-center gap-1.5 mx-auto">
  <Download className="h-3.5 w-3.5" /> Download QR
</button>
```

This replaces the `QRCodeSVG` in v3 Phase 7.1 with `QRCodeCanvas` to enable download.

**Effort:** 15 min

### #12 — "Last Payout" stat on splash ribbon (B-37)

**File:** `frontend/src/App.tsx`

**Problem:** The v2 plan specified "Last Payout: 2.4 ICP" as one of three splash ribbon stats. Both the current implementation and v3 Phase 8.1 use "Live on ICP" instead. "Last Payout" is more compelling social proof — it tells visitors someone recently got paid.

**Fix (depends on data availability):**
1. **If public stats include last payout data:** Show `Last Payout: {formatICP(lastPayout)} ICP` as the third ribbon stat
2. **If no public endpoint for this:** Show it when live data becomes available (after v3 Phase 8.1 adds public stats). Add "Last Payout" to the proposed `getPublicStats()` return type
3. **Static fallback:** If truly blocked, note it as a backend dependency and keep "Live on ICP"

The key point: when the public stats endpoint is eventually added (Phase 8.1), include `lastPayout` in the return value. Don't settle for "Live on ICP" if the data can be made available.

**Effort:** 10 min (frontend), depends on backend for data

### #13 — Celebration timer correction (B-38)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** The v2 plan said "After 3 seconds, auto-navigate." The implementation uses `4000`. The v3 Phase 1.3 fixes the empty callback and adds a proceed button but doesn't correct the timer.

**Fix:** When implementing v3 Phase 1.3, also change `4000` to `3000` on line 33:
```
}, 3000);
```

Trivial fix that should be bundled with Phase 1.3's other celebration fixes.

**Effort:** 1 min

### #14 — House Ledger accordion — first section expanded by default (B-39)

**File:** `frontend/src/components/HouseDashboard.tsx`

**Problem:** The v2 plan said "Default: all collapsed (the first one optionally expanded)." The implementation starts with `openSection = null` (all closed). The common progressive disclosure pattern is first-section-open so users see content immediately rather than a wall of closed headers.

**Fix:** Change the initial state:
```tsx
const [openSection, setOpenSection] = useState<string | null>(sections[0]?.title || null);
```

Or hardcode to the first section title if the array is static:
```tsx
const [openSection, setOpenSection] = useState<string | null>('What Is This?');
```

This way new users see the first section's content and understand the accordion pattern. Returning users who've read it can collapse it.

**Effort:** 5 min

---

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

---

## Phase D — Core User Flow Fixes

*Resolves: A-8, A-3, A-9, B-4, B-23*

Status bar layout, always-enabled CTA, and ProfileSetup polish. These are the highest-impact remaining UX fixes in the core user flow — the screens every user touches on every session.

### #22 — Convert status bar to stacked layout (A-8)

**File:** `frontend/src/components/GameStatusBar.tsx`, `frontend/src/index.css`

**Problem:** The v2 plan specified labels stacked above values (vertical casino-scoreboard style). The actual layout is inline (label and value side-by-side).

**Fix:** Restructure each stat cell from horizontal to vertical:
```
// Before (inline):
<span className="mc-status-bar-label">P/L</span>
<span className="mc-status-bar-value">+1.2 ICP</span>

// After (stacked):
<div className="mc-status-bar-stat">
  <span className="mc-status-bar-label">P/L</span>
  <span className="mc-status-bar-value">+1.2 ICP</span>
</div>
```

CSS changes:
```css
.mc-status-bar-stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}
.mc-status-bar-label {
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.mc-status-bar-value {
  font-size: 12px;
  font-weight: 700;
}
```

Mobile: reduce to 2-3 key stats (Balance, P/L, PP). Hide Positions and Pot. The bar should never feel dense on a phone.

**Effort:** 30 min

### #23 — Remove disabled states from CTA button (A-3)

**File:** `frontend/src/components/GamePlans.tsx`

**Problem:** The CTA button has a `disabled={!amount || !selectedPlan || !selectedMode || !isAmountValid || !canDeposit || ...}` attribute and cycles through error text labels. The v2 plan explicitly called for an always-enabled button.

**Fix:**
1. Remove the `disabled` attribute entirely from the CTA button
2. Button text is always "START GAME" with the Dices icon
3. On click, if the state is invalid, show the specific error as an inline message below the button with shake animation:
```
{clickError && (
  <p className={`text-xs mc-text-danger mt-2 text-center ${shakeError ? 'mc-shake' : ''}`}>
    {clickError}
  </p>
)}
```
4. Error messages: "Choose a mode first", "Enter an amount", "Amount below minimum (0.1 ICP)", "Rate limited — wait X seconds", etc.
5. The button always looks actionable — `mc-btn-primary` styling, never grayed out

**Effort:** 45 min

### #24 — Add shake on error to ProfileSetup input (A-9)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** GamePlans and WalletDropdown both have shake-on-error but ProfileSetup doesn't.

**Fix:** Add `shakeInput` state. On submit with invalid name (empty or too long), set `shakeInput = true` with a 400ms timeout to clear. Apply `mc-shake` class to the input wrapper.

**Effort:** 10 min

### #25 — Character count / validation feedback while typing (B-4)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** No feedback about name length or validity as the user types.

**Fix:** Below the input, show:
```
<div className="flex justify-between mt-1.5 text-xs">
  <span className={name.length > 20 ? 'mc-text-danger' : 'mc-text-muted'}>
    {name.length > 0 ? `${name.length}/20 characters` : ''}
  </span>
  {name.length > 20 && (
    <span className="mc-text-danger">Too long</span>
  )}
</div>
```

Adjust the max length to match whatever the backend enforces. Show the counter only once the user starts typing (don't show "0/20" on an empty field).

**Effort:** 15 min

### #26 — Casino registration desk atmospheric visual (B-23)

**File:** `frontend/src/components/ProfileSetup.tsx`, `frontend/src/index.css`

**Problem:** The original report wanted ProfileSetup to feel like "walking up to a casino registration desk" with atmospheric illustration or animated elements. Currently it's just a card with an input.

**Fix:** Add atmospheric touches without requiring external assets:
1. A subtle CSS gradient or radial glow behind the setup card — `mc-registration-glow`:
```css
.mc-registration-glow {
  position: relative;
}
.mc-registration-glow::before {
  content: '';
  position: absolute;
  inset: -40px;
  background: radial-gradient(ellipse at center, rgba(168, 85, 247, 0.08) 0%, transparent 70%);
  pointer-events: none;
  z-index: -1;
}
```
2. Add a decorative icon above the card — a stylized chip stack or card deck using lucide-react icons (`Layers` or `CreditCard` stacked with CSS transforms):
```
<div className="flex justify-center mb-6 opacity-40">
  <div className="relative">
    <CreditCard className="h-12 w-12 mc-text-gold absolute -rotate-12 -translate-x-2" />
    <CreditCard className="h-12 w-12 mc-text-purple rotate-6 translate-x-2" />
  </div>
</div>
```
3. The card itself gets `mc-card-elevated` treatment if it doesn't already have it

This creates a visual "registration desk" atmosphere without needing custom illustrations.

**Effort:** 30 min

---

## Phase E — Animation Infrastructure & House Ledger

*Resolves: A-10, A-11, A-4, A-5, B-19, B-20*

CountUp re-animation and progress bar consistency provide animation infrastructure used by later phases. House Ledger polish (hero promotion, redistribution drama, tab counts, timeline) completes the remaining component work on a single screen.

### #27 — CountUp re-animates on tab switch (A-10)

**File:** `frontend/src/hooks/useCountUp.ts`, `frontend/src/components/GameTracking.tsx`

**Problem:** `useCountUp` skips animation for changes < 1%. Switching tabs and back doesn't re-trigger because values haven't changed.

**Fix:** Add a `key` or `resetToken` parameter to the hook:
```ts
export function useCountUp(target: number, duration = 1000, resetToken?: number): number
```

In `GameTracking`, pass a token that changes when the tab becomes visible. Two approaches:

**Option A (simpler):** Accept a `visible` boolean prop from Dashboard. When `visible` transitions from false to true, increment a counter and pass it as `resetToken`. The hook detects the token change and re-runs the animation even if the target hasn't changed.

**Option B:** Use `IntersectionObserver` on the Running Tally card. When it enters the viewport, trigger re-animation. This also fixes the problem for scroll-based visibility.

Go with Option A (tab-driven) first. The hook change:
```ts
// When resetToken changes, force re-animation
useEffect(() => {
  if (resetToken !== undefined) {
    // Reset animation from 0 -> target
    setValue(0);
    // ... start animation
  }
}, [resetToken]);
```

**Effort:** 45 min

### #28 — Replace custom progress bars with shadcn Progress (A-11)

**File:** `frontend/src/components/GameTracking.tsx`

**Problem:** The plan specified shadcn `<Progress>` but a custom div bar was built. HouseDashboard uses shadcn Progress — inconsistency.

**Fix:** Replace the custom div bar in the PositionCard with:
```tsx
import { Progress } from '@/components/ui/progress';

<Progress
  value={progressPercent}
  className="h-1.5"
  indicatorClassName={planType === 'simple' ? 'bg-green-500' : 'bg-purple-500'}
/>
```

Check the shadcn Progress component's API for `indicatorClassName` or equivalent prop. If it doesn't support indicator color overrides, wrap it with a CSS class:
```css
.mc-progress-green [data-indicator] { background: var(--mc-neon-green); }
.mc-progress-purple [data-indicator] { background: var(--mc-purple); }
```

**Effort:** 20 min

### #29 — AddHouseMoney hero promotion (A-4)

**File:** `frontend/src/components/HouseDashboard.tsx`

**Problem:** AddHouseMoney is visually equivalent to any other card. The plan called for hero treatment above the grid.

**Fix:** Move the `<AddHouseMoney>` render above the backers grid and wrap in a `mc-card-elevated` container:
```
<div className="mc-card-elevated p-6 mb-6">
  <h3 className="font-display text-lg mc-text-gold mb-3">Back the House</h3>
  <p className="text-sm mc-text-dim mb-4">
    Become a dealer. Earn your 12% entitlement. (Returns not guaranteed — this is still a Ponzi.)
  </p>
  <AddHouseMoney />
</div>
```

**Effort:** 20 min

### #30 — Redistribution Event dramatic treatment (A-5)

**File:** `frontend/src/components/HouseDashboard.tsx`, `frontend/src/index.css`

**Problem:** The Redistribution Event callout exists but is static. The plan called for a pulsing flame and hover glow.

**Fix:** Add CSS:
```css
.mc-redistribution-pulse .lucide-flame {
  animation: mc-flame-pulse 2s ease-in-out infinite;
}
@keyframes mc-flame-pulse {
  0%, 100% { opacity: 0.8; transform: scale(1); }
  50% { opacity: 1; transform: scale(1.1); }
}
.mc-redistribution-pulse:hover {
  box-shadow: 0 0 20px rgba(255, 68, 68, 0.2);
  transition: box-shadow 0.3s ease;
}
```

Add `mc-redistribution-pulse` class to the Redistribution Event callout container.

**Effort:** 15 min

### #31 — Tab labels with preview counts (B-19)

**File:** `frontend/src/components/HouseDashboard.tsx`

**Problem:** The tab toggle says "Backers" / "Ledger" with no context about what's inside.

**Fix:** Show counts in the tab labels:
```
Backers ({backerPositions?.length || 0})  |  Ledger ({ledgerRecords?.length || 0})
```

Use `mc-text-muted` for the count to keep it subordinate:
```
<span>Backers</span>
<span className="mc-text-muted ml-1">({backerPositions?.length || 0})</span>
```

**Effort:** 10 min

### #32 — Ledger as transaction timeline (B-20)

**File:** `frontend/src/components/HouseDashboard.tsx`

**Problem:** The ledger is a flat list of records. The original report wanted a proper timeline with icons and visual hierarchy.

**Fix:** Replace the flat list with a timeline layout:
```
{ledgerRecords.map((record, i) => (
  <div key={i} className="flex gap-3 py-3 border-b border-white/5 last:border-0">
    {/* Timeline dot + line */}
    <div className="flex flex-col items-center">
      <div className={`w-2.5 h-2.5 rounded-full mt-1 ${
        record.amount > 0 ? 'bg-green-500' : 'bg-red-500'
      }`} />
      {i < ledgerRecords.length - 1 && (
        <div className="w-px flex-1 bg-white/10 mt-1" />
      )}
    </div>
    {/* Content */}
    <div className="flex-1 min-w-0">
      <div className="flex items-center gap-2">
        {record.amount > 0 ? (
          <ArrowDownLeft className="h-3.5 w-3.5 mc-text-green flex-shrink-0" />
        ) : (
          <ArrowUpRight className="h-3.5 w-3.5 mc-text-danger flex-shrink-0" />
        )}
        <span className="text-sm font-bold truncate">{record.description}</span>
      </div>
      <div className="flex justify-between mt-1 text-xs mc-text-muted">
        <span>{formatTimestamp(record.timestamp)}</span>
        <span className={record.amount > 0 ? 'mc-text-green' : 'mc-text-danger'}>
          {record.amount > 0 ? '+' : ''}{formatICP(record.amount)} ICP
        </span>
      </div>
    </div>
  </div>
))}
```

Import `ArrowDownLeft`, `ArrowUpRight` from lucide-react.

**Effort:** 45 min

---

## Phase F — Wallet & Referral Enrichment

*Resolves: A-2, A-1, B-16, B-22, B-8, B-10*

The wallet money flow diagram and referral section enrichments (QR code, stats context, PP bridge CTA, activity feed, network visualization) are the 'money movement' features — they help users understand where value flows through the system.

### #33 — Money flow diagram in wallet (A-2)

**File:** `frontend/src/components/WalletDropdown.tsx`

**Problem:** No visual explaining how money flows through the system. New users don't understand the difference between wallet balance, game balance, and position.

**Fix:** Add a compact flow diagram inside the wallet dropdown/sheet, below the balance section:
```
<div className="flex items-center justify-center gap-1 text-xs mc-text-muted py-2 px-3 border-t border-white/5">
  <span className={currentStep === 'wallet' ? 'mc-text-primary font-bold' : ''}>Wallet</span>
  <span>-></span>
  <span className={currentStep === 'balance' ? 'mc-text-primary font-bold' : ''}>Game Balance</span>
  <span>-></span>
  <span className={currentStep === 'position' ? 'mc-text-green font-bold' : ''}>Position</span>
  <span>-></span>
  <span className={currentStep === 'earnings' ? 'mc-text-gold font-bold' : ''}>Earnings</span>
</div>
```

Determine `currentStep` from context: if user has no game balance, highlight "Wallet". If they have balance but no positions, highlight "Game Balance". If they have positions, highlight "Position" or "Earnings".

**Effort:** 30 min

### #34 — QR code for referral link (A-1)

**File:** `frontend/src/components/ReferralSection.tsx`, `package.json`

**Problem:** QR code was planned but never implemented.

**Fix:**
1. Install: `npm install qrcode.react`
2. Below the share buttons, add:
```tsx
import { QRCodeSVG } from 'qrcode.react';

<div className="flex flex-col items-center mt-4 p-4 mc-card">
  <QRCodeSVG
    value={referralLink}
    size={160}
    bgColor="transparent"
    fgColor="#ffffff"
    level="M"
  />
  <p className="text-xs mc-text-muted mt-2">Scan to join your pyramid</p>
</div>
```

**Effort:** 20 min

### #35 — Referral stats context (B-16)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** Stats show raw numbers ("Direct Referrals: 0") with no context about what they mean or what to aim for.

**Fix:** Add contextual framing to each stat:
```
// Instead of just the number:
<div className="text-center">
  <div className="text-2xl font-bold">{directReferrals}</div>
  <div className="text-xs mc-text-muted">Direct Referrals</div>
  <div className="text-xs mc-text-dim mt-1">
    {directReferrals === 0 ? 'Share your link to get started' :
     directReferrals < 5 ? `${5 - directReferrals} more for Networker badge` :
     directReferrals < 10 ? `${10 - directReferrals} more for Pyramid Architect` :
     'Top recruiter energy'}
  </div>
</div>
```

For Level 2 and Level 3, show contextual text like "Your referrals' referrals" and "Three levels deep."

For Referral PP, show what it could buy: "Enough for {Math.floor(referralPP / 500)} shenanigan casts" or similar.

**Effort:** 30 min

### #36 — "Spend your PP" bridge CTA (B-22)

**File:** `frontend/src/components/ReferralSection.tsx` (or wherever the PP balance is shown in Profit Center)

**Problem:** No cross-linking between earning PP and spending PP on shenanigans.

**Fix:** In the ReferralSection (after the stats grid), if user has PP >= 100 (cheapest shenanigan), add:
```
<button
  onClick={() => onTabChange?.('shenanigans')}
  className="mc-btn-secondary flex items-center gap-2 mx-auto mt-4 text-xs"
>
  <Dice5 className="h-4 w-4 mc-text-purple" />
  Spend your PP on Shenanigans ->
</button>
```

This requires `onTabChange` to be passed as a prop from Dashboard. Also add a similar CTA in the PP section of GameTracking (Profit Center).

**Effort:** 20 min

### #37 — Referral activity feed (B-8)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** No feed showing recent referral activity (who signed up, who deposited, PP earned).

**Reality check:** This likely requires backend support — a query that returns recent referral events for the user. If the backend doesn't provide this data, this item is **blocked**.

**If backend supports it:** Add a small feed below the stats grid:
```
<div className="mc-card p-4 mt-4">
  <h4 className="mc-label mb-3">Recent Activity</h4>
  <div className="space-y-2 max-h-48 overflow-y-auto">
    {referralActivity.map((event, i) => (
      <div key={i} className="flex justify-between text-xs">
        <span className="mc-text-dim">{event.description}</span>
        <span className="mc-text-green">+{event.ppEarned} PP</span>
      </div>
    ))}
  </div>
</div>
```

**If backend doesn't support it:** Note as blocked, add a placeholder text: "Referral activity feed coming soon" with a `mc-text-muted` styling.

**Effort:** 1-2 hours (depends on backend)

### #38 — Network visualization (B-10)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** No visual tree/graph of the user's referral network. The original report wanted this.

**Reality check:** This is a large feature. A proper interactive tree visualization requires either a library (d3, react-flow) or significant custom SVG work. The data is also limited — we only have 3 levels of referral data.

**Minimum viable version:** A simplified 3-level tree using pure CSS/HTML:
```
         [You]
        /  |  \
    [L1a] [L1b] [L1c]
     / \
  [L2a] [L2b]
```

Each node: small circle with first letter of name, tooltip with full name + PP earned. Lines drawn with CSS borders or SVG paths. Limit to showing first 5 referrals per level (with "+N more" overflow).

**Explicit deferral option:** If this is too much scope, explicitly note it as deferred in the report with a reason. Don't silently drop it again.

**Effort:** 3-4 hours (MVP), 8+ hours (polished with d3)

---

## Phase G — Splash Page Enhancements

*Resolves: A-6, B-1, B-2, B-3, B-14*

### #39 — Live data on splash ribbon (A-6)

**File:** `frontend/src/App.tsx`, possibly `frontend/src/hooks/useQueries.ts`

**Problem:** Backend requires auth for `useGetGameStats`. The ribbon shows static copy instead of live numbers.

**Fix options (in order of preference):**
1. **Add a public (no-auth) query endpoint** to the backend: `getPublicStats()` returning pot size and player count. This is the correct fix but requires backend work.
2. **Call the existing public queries** mentioned in the v1 task list: `getPlatformStats()`, `getActiveGameCount()`, `getAvailableBalance()`. Check if these actually exist and work without auth.
3. **If no public endpoint exists:** Keep the static copy but explicitly note this as a backend dependency in the report. Don't fake numbers.

If live data becomes available:
```
const { data: publicStats } = useQuery({
  queryKey: ['publicStats'],
  queryFn: () => backendActor.getPublicStats(),
  enabled: !isAuthenticated, // only on splash
  refetchInterval: 30000,
});
```

Update the ribbon to show: `Pot: {formatICP(publicStats.pot)} ICP | {publicStats.activePlayers} Players | Live on ICP`

**Effort:** 1-2 hours (frontend), unknown for backend

### #40 — Typewriter effect on tagline (B-1)

**File:** `frontend/src/App.tsx`, `frontend/src/index.css`

**Problem:** The "It's a Ponzi!" tagline appears instantly. The original report wanted it to type out letter by letter.

**Fix:** CSS-only typewriter:
```css
.mc-typewriter {
  overflow: hidden;
  white-space: nowrap;
  border-right: 2px solid var(--mc-gold);
  width: 0;
  animation: mc-typewriter 1.2s steps(14) 0.8s forwards, mc-blink-caret 0.6s step-end 3;
}
@keyframes mc-typewriter {
  to { width: 100%; }
}
@keyframes mc-blink-caret {
  50% { border-color: transparent; }
}
```

Apply to the tagline. The delay (0.8s) allows the logo to appear first. `steps(14)` matches the character count of "It's a Ponzi!". After animation completes, set `border-right: transparent` (animation-fill-mode handles this).

The caret blinks 3 times then stops. Clean, lightweight, no JS.

**Effort:** 20 min

### #41 — Animated background on splash (B-2)

**File:** `frontend/src/index.css` (or new lightweight component)

**Problem:** The splash page is static. The original report wanted particles or gradient shifts.

**Fix (CSS-only, no library):** A slow-moving gradient background:
```css
.mc-splash-bg {
  position: fixed;
  inset: 0;
  z-index: -1;
  background: radial-gradient(ellipse at 20% 50%, rgba(168, 85, 247, 0.06) 0%, transparent 50%),
              radial-gradient(ellipse at 80% 50%, rgba(57, 255, 20, 0.04) 0%, transparent 50%);
  animation: mc-bg-drift 20s ease-in-out infinite alternate;
}
@keyframes mc-bg-drift {
  0% { background-position: 0% 50%; }
  100% { background-position: 100% 50%; }
}
```

Add `<div className="mc-splash-bg" />` inside the splash section. Respects `prefers-reduced-motion` (Phase 13.2).

Subtle, performant, no dependencies.

**Effort:** 15 min

### #42 — Docs teaser / "How It Works" section (B-3)

**File:** `frontend/src/App.tsx`

**Problem:** No expandable section on the splash explaining the game mechanics. The entire app assumes you know what a Ponzi scheme game is.

**Fix:** Below the info cards and before the stats ribbon, add an expandable "How It Works" section:
```
const [showHowItWorks, setShowHowItWorks] = useState(false);

<div className="mt-6">
  <button
    onClick={() => setShowHowItWorks(!showHowItWorks)}
    className="flex items-center gap-2 mx-auto text-xs mc-text-dim hover:mc-text-primary transition-colors"
  >
    <ChevronDown className={`h-4 w-4 transition-transform ${showHowItWorks ? 'rotate-180' : ''}`} />
    How does it work?
  </button>
  {showHowItWorks && (
    <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-3 text-left mc-stagger">
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-green mb-2">Deposit ICP</h4>
        <p className="text-xs mc-text-dim">Choose a plan. Simple earns 11%/day for 21 days. Compounding earns more but locks your money.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-gold mb-2">Earn Daily</h4>
        <p className="text-xs mc-text-dim">Your position earns interest from the pot. Withdraw anytime — earlier exits pay a higher toll.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-purple mb-2">Cast Shenanigans</h4>
        <p className="text-xs mc-text-dim">Earn Ponzi Points. Spend them on cosmetic chaos — rename other players, skim their earnings, boost your referrals.</p>
      </div>
      <div className="mc-card p-4">
        <h4 className="font-display text-sm mc-text-danger mb-2">The Catch</h4>
        <p className="text-xs mc-text-dim">When the pot empties, the game resets. If you're still in — total loss. That's the Ponzi part.</p>
      </div>
    </div>
  )}
</div>
```

**Effort:** 30 min

### #43 — Scroll-triggered animations (B-14)

**File:** `frontend/src/App.tsx`, `frontend/src/index.css`

**Problem:** Page-load animations fire before below-the-fold elements are visible. On mobile, cards animate into empty air.

**Fix:** Create a small `useScrollAnimate` hook or use IntersectionObserver directly:
```ts
function useScrollAnimate(ref: RefObject<HTMLElement>, className = 'mc-scroll-visible') {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          el.classList.add(className);
          observer.unobserve(el);
        }
      },
      { threshold: 0.1 }
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
}
```

CSS:
```css
.mc-scroll-animate {
  opacity: 0;
  transform: translateY(20px);
  transition: opacity 0.6s ease-out, transform 0.6s ease-out;
}
.mc-scroll-animate.mc-scroll-visible {
  opacity: 1;
  transform: translateY(0);
}
```

Apply `mc-scroll-animate` to the info cards, stats ribbon, and "How It Works" section. The hero (logo, tagline) keeps the page-load animation since it's always above the fold.

**Effort:** 45 min

---

## Phase H — Secondary Tab Enrichment

*Resolves: B-17, B-21, B-15*

Shenanigans 'Popular Now' indicator and Ponzi Points enrichment (rate table, activity feed, spending suggestions). Both are secondary tabs that currently feel thin — these tasks give them substance.

### #44 — "Popular Now" / Trending indicator (B-17)

**File:** `frontend/src/components/Shenanigans.tsx`

**Problem:** No indication of which shenanigans other players are casting most frequently.

**Fix:** The live feed already shows recent casts. Count occurrences in the recent feed to determine the most-cast shenanigan, then add a "Popular" badge to that card:
```
// Compute from recent feed:
const castCounts = recentFeed.reduce((acc, event) => {
  acc[event.shenaniganType] = (acc[event.shenaniganType] || 0) + 1;
  return acc;
}, {} as Record<string, number>);
const mostPopular = Object.entries(castCounts).sort((a, b) => b[1] - a[1])[0]?.[0];

// In the card render:
{shenaniganId === mostPopular && (
  <span className="absolute -top-2 -right-2 text-xs bg-orange-500/20 text-orange-400 px-2 py-0.5 rounded-full font-bold">
    Popular
  </span>
)}
```

If there's no feed data or too few entries, don't show the badge. Requires the card container to have `position: relative`.

**Effort:** 30 min

### #45 — PP earn rates as visual comparison table (B-21)

**File:** `frontend/src/components/GameTracking.tsx` (PP section within Profit Center)

**Problem:** Earn rates are presented as prose text. Should be a visual comparison.

**Fix:** Replace the prose with a comparison table:
```
<div className="grid grid-cols-3 gap-2 text-center text-xs mt-3">
  <div className="mc-card p-3">
    <div className="mc-text-green font-bold text-sm">1,000</div>
    <div className="mc-text-muted">PP per ICP</div>
    <div className="mc-text-dim mt-1">Simple 21-day</div>
  </div>
  <div className="mc-card p-3">
    <div className="mc-text-purple font-bold text-sm">2,000</div>
    <div className="mc-text-muted">PP per ICP</div>
    <div className="mc-text-dim mt-1">Compound 15-day</div>
  </div>
  <div className="mc-card p-3">
    <div className="mc-text-gold font-bold text-sm">3,000</div>
    <div className="mc-text-muted">PP per ICP</div>
    <div className="mc-text-dim mt-1">Compound 30-day</div>
  </div>
</div>
```

Visual, scannable, makes the tradeoff clear at a glance.

**Effort:** 20 min

### #46 — PP activity feed and spending suggestions (B-15)

**File:** `frontend/src/components/GameTracking.tsx` (PP section)

**Problem:** PP content is thin — just a number. No activity, no suggestions, no context.

**Fix:** Add two subsections to the PP area:

**Spending suggestions** (can compute client-side from PP balance):
```
{ponziPoints >= 100 && (
  <div className="mt-3">
    <p className="mc-label mb-2">You can afford:</p>
    <div className="flex flex-wrap gap-2">
      {affordableShenanigans.slice(0, 3).map(s => (
        <span key={s.name} className="text-xs mc-card px-2 py-1">
          {s.name} ({s.cost} PP)
        </span>
      ))}
    </div>
  </div>
)}
```

Where `affordableShenanigans` is computed from the shenanigan config (sorted by cost ascending, filtered by `cost <= ponziPoints`).

**Activity feed** — if the backend provides PP transaction history, show it. If not, show a simplified breakdown of PP sources:
```
<div className="mt-3 text-xs space-y-1">
  <div className="flex justify-between">
    <span className="mc-text-muted">From deposits</span>
    <span className="mc-text-green">+{depositPP} PP</span>
  </div>
  <div className="flex justify-between">
    <span className="mc-text-muted">From referrals</span>
    <span className="mc-text-cyan">+{referralPP} PP</span>
  </div>
  <div className="flex justify-between">
    <span className="mc-text-muted">Spent on shenanigans</span>
    <span className="mc-text-danger">-{burnedPP} PP</span>
  </div>
</div>
```

This data is already available from `useGetPonziPoints()`.

**Effort:** 45 min

---

## Phase I — Cross-Cutting Polish

*Resolves: B-26, B-27, B-28, B-29, B-30, B-31, B-32*

These items were found by cross-referencing the original report and v1 task list against everything already tracked. They slipped through every previous audit.

### #47 — Splash card narrative pacing / visual differentiation (B-26)

**File:** `frontend/src/App.tsx`, `frontend/src/index.css`

**Problem:** The three splash info cards are equally-sized in a uniform grid. The report said "The Pitch is the hook, the Catch is the friction, the Twist is the payoff — they need dramatic pacing, not a uniform grid."

**Fix:** Give each card a different visual weight:
- **Card 1 (Pitch/green):** Slightly larger, more padding, the "hook" — `p-6` instead of `p-5`, maybe a subtle pulsing border glow to draw the eye first
- **Card 2 (Catch/danger):** Standard size but with a distinctive treatment — a strikethrough or caution tape motif, `border-dashed` or a subtle diagonal stripe pattern
- **Card 3 (Twist/gold):** The payoff — `mc-card-elevated` with a gold glow, feels premium and final

On mobile (single-column), the stagger timing should increase so each card lands with deliberate pacing: 0ms, 400ms, 800ms instead of the tighter desktop stagger.

**Effort:** 30 min

### #48 — Facebook share button (B-27)

**File:** `frontend/src/components/ReferralSection.tsx`

**Problem:** The v1 task list (owner-approved Decision Log) listed "Twitter/X, Telegram, WhatsApp, Facebook, QR Code" as share targets. Facebook was silently dropped.

**Fix:** Add a Facebook share button alongside the existing three:
```tsx
<a
  href={`https://www.facebook.com/sharer/sharer.php?u=${encodeURIComponent(referralLink)}`}
  target="_blank"
  rel="noopener noreferrer"
  className="mc-btn-secondary px-4 py-2 rounded-lg text-xs flex items-center gap-1.5"
>
  <Globe className="h-3.5 w-3.5" /> {/* or Facebook-specific icon */}
  Facebook
</a>
```

Note: Facebook's sharer only accepts a URL (no custom text). The referral link itself is the share content.

**Effort:** 10 min

### #49 — Shenanigans live feed as desktop right-side panel (B-28)

**File:** `frontend/src/components/Shenanigans.tsx`, `frontend/src/index.css`

**Problem:** The live feed is below the cards on both desktop and mobile. The report said it should be a right-side panel on desktop.

**Fix:** On desktop (>=1024px), restructure the Shenanigans layout to a 2-column grid:
```css
@media (min-width: 1024px) {
  .mc-shenanigans-layout {
    display: grid;
    grid-template-columns: 1fr 300px;
    gap: 24px;
  }
}
```

Left column: filter tabs + shenanigan cards grid + guardrails.
Right column: stats grid + live feed (sticky, `position: sticky; top: 160px; max-height: calc(100vh - 200px); overflow-y: auto`).

On mobile: unchanged (everything stacks vertically, feed at bottom).

This makes the feed always visible while scrolling through cards on desktop.

**Effort:** 1 hour

### #50 — Time-based leaderboard filters (B-29)

**File:** `frontend/src/components/HallOfFame.tsx`

**Problem:** No "This Round" / "All Time" toggle. The data always shows all-time stats.

**Backend dependency:** Requires a round-scoped leaderboard query (filtering by current round's start date). If the backend doesn't track round boundaries in a queryable way, this is **blocked**.

**If backend supports it:** Add a toggle above the leaderboard:
```tsx
const [timeFilter, setTimeFilter] = useState<'round' | 'allTime'>('allTime');

<div className="flex gap-2 mb-4 justify-center">
  <button
    onClick={() => setTimeFilter('round')}
    className={`mc-btn-pill text-xs ${timeFilter === 'round' ? 'mc-btn-primary' : ''}`}
  >
    This Round
  </button>
  <button
    onClick={() => setTimeFilter('allTime')}
    className={`mc-btn-pill text-xs ${timeFilter === 'allTime' ? 'mc-btn-primary' : ''}`}
  >
    All Time
  </button>
</div>
```

Pass `timeFilter` to the query hook. Frontend filtering won't work — the data must come from the backend.

**If blocked:** Add the UI toggle but disable "This Round" with a tooltip: "Coming after next round reset." This shows the feature is planned without faking data.

**Effort:** 30 min (frontend only), blocked on backend

### #51 — Mobile bottom sheets for all dialogs (B-30)

**Files:** `frontend/src/components/GameTracking.tsx`, `frontend/src/components/Shenanigans.tsx`

**Problem:** WalletDropdown uses a mobile bottom sheet. The withdrawal dialog, reinvest dialog, and shenanigan confirmation dialog still use centered shadcn Dialog modals on mobile.

**Fix:** Create a reusable `MobileSheet` wrapper component that detects mobile and renders as bottom sheet instead of Dialog:
```tsx
function MobileSheet({ open, onOpenChange, children }: { open: boolean; onOpenChange: (open: boolean) => void; children: React.ReactNode }) {
  const isMobile = useMobileDetect(); // reuse from WalletDropdown
  const sheetRef = useRef<HTMLDivElement>(null);

  // Drag-to-dismiss — reuse the touch handler logic from Phase 1.2
  const handleTouchStart = (e: React.TouchEvent) => { /* track startY */ };
  const handleTouchMove = (e: React.TouchEvent) => { /* translate sheet */ };
  const handleTouchEnd = () => { /* if dragged > 30% of height, dismiss */ };

  if (isMobile) {
    return (
      <>
        {open && <div className="mc-sheet-backdrop" onClick={() => onOpenChange(false)} />}
        {open && (
          <div ref={sheetRef} className="mc-bottom-sheet">
            <div className="mc-drag-handle"
              onTouchStart={handleTouchStart}
              onTouchMove={handleTouchMove}
              onTouchEnd={handleTouchEnd}
            />
            {children}
          </div>
        )}
      </>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="mc-dialog">
        {children}
      </DialogContent>
    </Dialog>
  );
}
```

Replace `Dialog` with `MobileSheet` in:
- `GameTracking.tsx` — withdrawal and reinvest dialogs
- `Shenanigans.tsx` — cast confirmation dialog

Reuse the existing `mc-bottom-sheet` and `mc-sheet-backdrop` CSS from WalletDropdown.

**Effort:** 1-2 hours

### #52 — Charles personality throughout the app (B-31)

**Files:** `frontend/src/components/HouseDashboard.tsx`, `frontend/src/components/GameTracking.tsx`, various

**Problem:** Charles only appears in the admin panel and GameTracking empty state. The v1 task list Phase 13.4 wanted Charles throughout the app.

**Fix:** Add Charles personality to these specific locations:
- **HouseDashboard info cards:** "Charles takes a 3% maintenance fee on every deposit" (in the fee explanation section)
- **HouseDashboard redistribution callout:** "When the pot runs dry, Charles resets the table. No exceptions." (replace or augment existing copy)
- **GameTracking exit toll info:** "Charles collects a 7% exit toll if you leave within 3 days. His table, his rules." (in the info card)
- **Error states (global):** When an API call fails, show "Even Charles couldn't fix this one. Try again?" alongside the retry button
- **Loading states:** Where relevant, "Charles is counting the money..." or "Charles is shuffling the deck..." instead of generic spinners

These are copy changes, not structural changes. The pattern is: anywhere there's an explanation of game mechanics or a wait state, inject Charles's voice.

**Effort:** 45 min

### #53 — Gold notification badge on The Pyramid tab (B-32)

**File:** `frontend/src/App.tsx`

**Problem:** Red (Profit Center) and purple (Shenanigans) badge dots exist. Gold (The Pyramid) for unviewed referral activity was specified but never implemented.

**Fix:** Add to the badge computation in App.tsx:
```tsx
const hasNewReferrals = referralStats?.directReferrals > 0 &&
  referralStats.directReferrals > (parseInt(localStorage.getItem('mc_last_seen_referrals') || '0'));

const badges = {
  profitCenter: hasWithdrawableEarnings ? 'red' : null,
  shenanigans: canCastShenanigan ? 'purple' : null,
  mlm: hasNewReferrals ? 'gold' : null,
};
```

When the user visits The Pyramid tab, update localStorage:
```tsx
useEffect(() => {
  if (activeTab === 'mlm' && referralStats) {
    localStorage.setItem('mc_last_seen_referrals', String(referralStats.directReferrals));
  }
}, [activeTab, referralStats]);
```

Add `.mc-badge-gold` CSS:
```css
.mc-badge-gold {
  background: var(--mc-gold);
  box-shadow: 0 0 6px rgba(255, 215, 0, 0.5);
}
```

**Effort:** 20 min

---

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

---

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

---

## Phase L — Final Cleanup

*Resolves: B-6, B-13, B-18*

### #61 — Check for duplicate refresh buttons (B-6)

**File:** `frontend/src/components/GameTracking.tsx`

**Problem:** v1 said "Duplicate refresh buttons may still exist." Nobody ever checked.

**Fix:** Search GameTracking.tsx for all refresh/reload buttons. If there are two, remove the duplicate. Keep only one refresh button at the top-right of the section.

This is a 5-minute check-and-fix.

**Effort:** 5-10 min

### #62 — Trollbox — explicit deferral (B-13)

**Status:** EXPLICITLY DEFERRED

The trollbox is a major feature requiring a new Motoko canister, websocket or polling infrastructure, rate limiting, moderation tools, and a full frontend component. It was the last phase in the v1 task list for good reason.

**Not doing it in v3 because:** It's estimated at 11-16 hours of work, requires backend changes, and is a feature addition rather than a UX fix. The v3 plan focuses on closing gaps in the existing UI, not adding new features.

**What to do instead:** Add a placeholder "Trollbox — Coming Soon" teaser somewhere visible (footer or header) to acknowledge it's planned. This is better than silently dropping it for a third time.

### #63 — Information density audit (B-18)

**Problem:** Some tabs feel packed, others feel sparse.

**Fix:** After all other phases are complete, do a visual review of every tab and assess:
- **Dense pages** (Profit Center, House Ledger): are they now using progressive disclosure, collapsible sections, and visual hierarchy to manage density?
- **Sparse pages** (MLM, ProfileSetup): have the enrichments from other phases (activity feed, milestones, atmospheric visuals) filled in the empty space?

If any page still feels empty after all enrichments, add contextual CTAs or "did you know" info cards to pad it. If any page still feels dense, add more collapsible sections.

This is an assessment pass, not a coding phase. Time it after everything else is done.

**Effort:** 30-60 min

---

## Execution Order

Prioritized by: bugs first, then spec compliance, then design system, then user-facing impact, then polish.

1. **Phase A: Bug Fixes & Regressions** (#1-#7) — Fix the 6 regressions + 1 plan miss before adding anything new
2. **Phase B: v2 Spec Compliance** (#8-#14) — Align implementation with v2 plan specs
3. **Phase C: Design System & Doc Updates** (#15-#21) — Fix design system violations and doc updates
4. **Phase D: Core User Flow Fixes** (#22-#26) — Status bar layout, always-enabled CTA, ProfileSetup polish
5. **Phase E: Animation Infrastructure & House Ledger** (#27-#32) — CountUp, progress bars, hero promotion, timeline
6. **Phase F: Wallet & Referral Enrichment** (#33-#38) — Money flow diagram, QR, stats context, activity feed, network viz
7. **Phase G: Splash Page Enhancements** (#39-#43) — Live data, typewriter, animated BG, docs teaser, scroll-trigger
8. **Phase H: Secondary Tab Enrichment** (#44-#46) — Popular indicator, PP rate table, spending suggestions
9. **Phase I: Cross-Cutting Polish** (#47-#53) — Splash card pacing, Facebook share, feed panel, leaderboard filters, bottom sheets, Charles, gold badge
10. **Phase J: Animations, Accessibility & Audits** (#54-#58) — ROI animation, prefers-reduced-motion, typography audit, mobile audit, pull-to-refresh
11. **Phase K: Onboarding & Documentation** (#59-#60) — Tour, docs page (largest new features)
12. **Phase L: Final Cleanup** (#61-#63) — Duplicate buttons, trollbox deferral, density audit

---

## Completeness Tracking

| v2 Eval Item | Phase | Status |
|---|---|---|
| **A: Plan Misses (11)** | | |
| A-1: QR code | #34 (F) | |
| A-2: Money flow indicator | #33 (F) | |
| A-3: Always-enabled CTA | #23 (D) | |
| A-4: AddHouseMoney hero | #29 (E) | |
| A-5: Redistribution dramatic | #30 (E) | |
| A-6: Live splash data | #39 (G) | |
| A-7: Celebration proceed button | #4 (A) | |
| A-8: Status bar stacked layout | #22 (D) | |
| A-9: ProfileSetup shake | #24 (D) | |
| A-10: countUp re-animation | #27 (E) | |
| A-11: shadcn Progress bars | #28 (E) | |
| **B: Dropped Items (46)** | | |
| B-1: Typewriter tagline | #40 (G) | |
| B-2: Animated background | #41 (G) | |
| B-3: Docs teaser on splash | #42 (G) | |
| B-4: Char count in ProfileSetup | #25 (D) | |
| B-5: Onboarding tour | #59 (K) | |
| B-6: Duplicate refresh buttons | #61 (L) | |
| B-7: Animated ROI calculator | #54 (J) | |
| B-8: Referral activity feed | #37 (F) | |
| B-9: Typography audit | #56 (J) | |
| B-10: Network visualization | #38 (F) | |
| B-11: Mobile audit | #57 (J) | |
| B-12: Docs page | #60 (K) | |
| B-13: Trollbox | #62 (L) | DEFERRED |
| B-14: Scroll-triggered animations | #43 (G) | |
| B-15: PP enrichment | #46 (H) | |
| B-16: Referral stats context | #35 (F) | |
| B-17: Popular Now indicator | #44 (H) | |
| B-18: Info density audit | #63 (L) | |
| B-19: Ledger tab preview counts | #31 (E) | |
| B-20: Ledger as timeline | #32 (E) | |
| B-21: PP rate comparison table | #45 (H) | |
| B-22: Spend PP bridge CTA | #36 (F) | |
| B-23: Casino registration atmosphere | #26 (D) | |
| B-24: Pull-to-refresh | #58 (J) | |
| B-25: prefers-reduced-motion | #55 (J) | |
| B-26: Splash card narrative pacing | #47 (I) | |
| B-27: Facebook share button | #48 (I) | |
| B-28: Live feed desktop panel | #49 (I) | |
| B-29: Time-based leaderboard filters | #50 (I) | BLOCKED (backend) |
| B-30: Bottom sheets for all dialogs | #51 (I) | |
| B-31: Charles personality throughout | #52 (I) | |
| B-32: Gold badge on Pyramid tab | #53 (I) | |
| B-33: Status bar P/L glow | #8 (B) | |
| B-34: Header tab font size spec | #9 (B) | |
| B-35: Podium avatar/initials | #10 (B) | |
| B-36: QR code download button | #11 (B) | |
| B-37: Last Payout splash stat | #12 (B) | Depends on backend |
| B-38: Celebration timer 3s | #13 (B) | |
| B-39: Accordion first section open | #14 (B) | |
| B-40: Header tab 2px gap violates 8px min | #15 (C) | |
| B-41: Perpetual tagline bob animation | #16 (C) | |
| B-42: Color token inconsistency | #17 (C) | |
| B-43: Header content density (design) | #18 (C) | |
| B-44: Design Philosophy doc outdated | #19 (C) | |
| B-45: MobileSheet missing drag-to-dismiss | #20 (C) | Enhancement |
| B-46: countUp resetToken as built-in pattern | #21 (C) | Enhancement |
| **C: Bugs (6)** | | |
| C-1: Filter empty state | #1 (A) | |
| C-2: Drag handle decorative | #2 (A) | |
| C-3: Celebration no refetch | #3 (A) | |
| C-4: MAX on zero balance | #5 (A) | |
| C-5: Podium with 2 entries | #6 (A) | |
| C-6: Header tabs overflow | #7 (A) | |

**63 items tracked. 60 addressed. 1 explicitly deferred (Trollbox). 2 blocked on backend (time-based leaderboard filters, Last Payout stat).**

---

## Files Changed Summary

**New files:**
- `frontend/src/components/OnboardingTour.tsx`
- `frontend/src/components/GameDocs.tsx`
- `frontend/src/hooks/useScrollAnimate.ts` (or inline in App.tsx)
- `frontend/src/hooks/usePullToRefresh.ts` (if custom)

**Modified files:**
- `frontend/src/App.tsx` (splash enhancements, docs button, scroll-animate, animated BG)
- `frontend/src/components/Dashboard.tsx` (onboarding tour mount, pull-to-refresh)
- `frontend/src/components/GameStatusBar.tsx` (stacked layout)
- `frontend/src/components/GameTracking.tsx` (PP enrichment, progress bars, countUp reset, refresh buttons)
- `frontend/src/components/GamePlans.tsx` (always-enabled CTA, MAX disable, ROI animation)
- `frontend/src/components/ProfileSetup.tsx` (shake, char count, atmosphere, celebration fix)
- `frontend/src/components/WalletDropdown.tsx` (drag handle, money flow)
- `frontend/src/components/Shenanigans.tsx` (empty state, Popular Now)
- `frontend/src/components/HouseDashboard.tsx` (hero promotion, redistribution pulse, tab counts, timeline)
- `frontend/src/components/ReferralSection.tsx` (QR, stats context, bridge CTA, activity feed, network viz)
- `frontend/src/components/HallOfFame.tsx` (2-entry podium fix)
- `frontend/src/hooks/useCountUp.ts` (resetToken, prefers-reduced-motion)
- `frontend/src/index.css` (overflow fix, stacked bar, flame pulse, typewriter, animated BG, scroll-animate, reduced-motion, ROI pop, registration glow, timeline styles)
- `package.json` (add `qrcode.react`)

---

## Estimated Total Effort

| Phase | Hours |
|---|---|
| A: Bug Fixes & Regressions (#1-#7) | 2-3 |
| B: v2 Spec Compliance (#8-#14) | 1-1.5 |
| C: Design System & Doc Updates (#15-#21) | 2-3 |
| D: Core User Flow Fixes (#22-#26) | 2-2.5 |
| E: Animation Infrastructure & House Ledger (#27-#32) | 2.5 |
| F: Wallet & Referral Enrichment (#33-#38) | 5.5-8.5 |
| G: Splash Page Enhancements (#39-#43) | 2-3 |
| H: Secondary Tab Enrichment (#44-#46) | 1.5 |
| I: Cross-Cutting Polish (#47-#53) | 4-5 |
| J: Animations, Accessibility & Audits (#54-#58) | 5-7 |
| K: Onboarding & Documentation (#59-#60) | 6-9 |
| L: Final Cleanup (#61-#63) | 1 |
| **Total** | **~35-50 hours** |

---

*End of v3 task list. 63 items tracked. 60 addressed. 1 explicitly deferred (Trollbox). 2 blocked on backend (time-based leaderboard filters, Last Payout stat). No silent drops.*
