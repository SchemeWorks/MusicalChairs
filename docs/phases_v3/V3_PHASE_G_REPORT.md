# Phase G — Splash Page Enhancements — REPORT

*Resolves: A-6, B-1, B-2, B-3, B-14*

**TS errors:** 44 total (all pre-existing) · 0 new

---

## #39 — Live data on splash ribbon (A-6)

**Problem:** Splash ribbon shows static copy ("Growing daily", "Up to 12% / day") instead of live numbers.

**Finding:** Backend has `getPlatformStats()` as a query method — works without authentication on ICP. The existing `useGetGameStats` hook uses `useActor` which depends on auth, so it can't be called from the splash.

**Solution:**
1. Added `useGetPublicStats()` hook in `useQueries.ts` that creates its own anonymous `HttpAgent` + actor, bypassing the auth-dependent `useActor` hook
2. Queries `getPlatformStats()` with 30s refetch interval and 15s stale time
3. Added necessary imports: `Actor`, `HttpAgent` from `@dfinity/agent`, `idlFactory` and `_SERVICE` from declarations, `BACKEND_CANISTER_ID` and `HOST` constants
4. App.tsx calls `useGetPublicStats()` and displays:
   - **Pot:** `{formatICP(publicStats.potBalance)} ICP` (falls back to "Growing daily" before data loads)
   - **Players:** `{publicStats.activeGames} active` (falls back to "Up to 12% / day")
   - **Status:** remains "Live on ICP" (always static)
5. Added `formatICP` import to App.tsx

**Files:** `useQueries.ts`, `App.tsx`

---

## #40 — Typewriter effect on tagline (B-1)

**Problem:** "It's a Ponzi!" tagline appears instantly on splash.

**Solution:** CSS-only typewriter animation:
- `.mc-typewriter` class: `overflow: hidden`, `white-space: nowrap`, `width: 0`, `display: inline-block`
- `mc-typewriter` keyframe: `width: 0 → 100%` over 1.2s with `steps(14)` matching character count
- `mc-blink-caret` keyframe: gold cursor blinks 3 times via `border-right: 2px solid var(--mc-gold)`
- 0.8s delay allows hero logo to appear first
- `animation-fill-mode: forwards` holds final state
- Wrapped tagline text in `<span className="mc-typewriter">`
- Respects `prefers-reduced-motion` — immediately shows full text with no animation

**Files:** `index.css`, `App.tsx`

---

## #41 — Animated background on splash (B-2)

**Problem:** Splash page is static.

**Solution:** CSS-only slow-moving gradient:
- `.mc-splash-bg` class: `position: fixed`, `inset: 0`, `z-index: -1`
- Two overlapping `radial-gradient` ellipses: purple (6% opacity) at 20% left, green (4% opacity) at 80% left
- `mc-bg-drift` keyframe: `background-position` shifts over 20s, alternating direction
- Added `<div className="mc-splash-bg" />` inside splash section
- Respects `prefers-reduced-motion` — no animation

**Files:** `index.css`, `App.tsx`

---

## #42 — "How It Works" expandable section (B-3)

**Problem:** No explanation of game mechanics on the splash page.

**Solution:** Expandable section between stats ribbon and Charles quote:
- Toggle button: "How does it work?" with rotating ChevronDown icon
- `showHowItWorks` state controls expansion
- 2×2 grid (1-col on mobile) with 4 cards:
  - **Deposit ICP** (green): Plan types and rates
  - **Earn Daily** (gold): Interest + withdrawal tolls
  - **Cast Shenanigans** (purple): PP earning and spending
  - **The Catch** (danger): Pot empty = reset = total loss
- Uses `mc-stagger` animation on expansion
- Added `ChevronDown` to lucide-react imports

**Files:** `App.tsx`

---

## #43 — Scroll-triggered animations (B-14)

**Problem:** Page-load animations fire on below-the-fold elements before they're visible.

**Solution:**
1. Added `useScrollAnimate` hook using `IntersectionObserver` (threshold 0.1):
   - Observes the element ref
   - Adds `mc-scroll-visible` class when element enters viewport
   - Unobserves after first trigger (one-shot animation)
2. CSS classes in `index.css`:
   - `.mc-scroll-animate`: `opacity: 0`, `transform: translateY(20px)`, `transition: 0.6s ease-out`
   - `.mc-scroll-animate.mc-scroll-visible`: `opacity: 1`, `transform: translateY(0)`
3. Applied to 3 splash elements via refs:
   - `cardsRef` → info cards grid
   - `ribbonRef` → stats ribbon
   - `howItWorksRef` → "How It Works" section
4. Hero (logo, tagline) keeps page-load animation since it's always above the fold
5. Respects `prefers-reduced-motion` — no opacity/transform transition

**Files:** `index.css`, `App.tsx`

---

## Files Modified

| File | Tasks |
|------|-------|
| `frontend/src/App.tsx` | #39, #40, #41, #42, #43 |
| `frontend/src/hooks/useQueries.ts` | #39 |
| `frontend/src/index.css` | #40, #41, #43 |

## Spec Coverage Audit

| ID | Item | Status |
|----|------|--------|
| A-6 | Live data on splash ribbon | ✅ Anonymous actor calls getPlatformStats, shows pot + player count |
| B-1 | Typewriter effect on tagline | ✅ CSS-only, 14 steps, gold caret, reduced-motion safe |
| B-2 | Animated background on splash | ✅ CSS-only gradient drift, reduced-motion safe |
| B-3 | How It Works expandable section | ✅ 4-card grid with toggle, covers all game mechanics |
| B-14 | Scroll-triggered animations | ✅ IntersectionObserver hook + CSS, 3 elements animated |
