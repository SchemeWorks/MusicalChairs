# Phase I — Cross-Cutting Polish — REPORT

*Resolves: B-26, B-27, B-28, B-29, B-30, B-31, B-32*

**TS errors:** 44 total (all pre-existing) · 0 new

---

## #47 — Splash card narrative pacing / visual differentiation (B-26)

**Problem:** Three splash info cards are equally-sized in a uniform grid — no dramatic pacing between the hook, friction, and payoff.

**Solution:**
1. Changed card container class from `mc-stagger` to `mc-splash-cards` with custom staggered entrance animations
2. Card 1 (The Pitch/green): Increased padding to `p-6`, added `mc-card-hook` class with pulsing green glow via `mc-hook-pulse` keyframe (3s alternate, `rgba(57,255,20)` 0.08→0.15)
3. Card 2 (The Catch/danger): Added `border-dashed` for caution-tape feel
4. Card 3 (The Twist/gold): Upgraded from `mc-card` to `mc-card-elevated` with `mc-card-payoff` class — static gold glow at `rgba(255,215,0,0.1)`
5. Desktop stagger: 0/150/300ms. Mobile stagger: 0/400/800ms (wider gaps for deliberate pacing on single-column layout)
6. `@keyframes mc-card-enter`: opacity 0→1, translateY 20→0, 0.5s ease-out
7. `prefers-reduced-motion` block extended to include `.mc-splash-cards > *` and `.mc-card-hook`

**Files:** `App.tsx`, `index.css`

---

## #48 — Facebook share button (B-27)

**Problem:** Facebook was listed as a share target in the owner-approved Decision Log but was silently dropped during implementation.

**Solution:**
1. Added `Globe` to lucide-react imports
2. Added Facebook share `<a>` tag using `facebook.com/sharer/sharer.php?u=` pattern (Facebook sharer only accepts URL, no custom text)
3. Placed after WhatsApp button, styled identically with `mc-btn-secondary` pill styling
4. Opens in new tab with `target="_blank"` and `rel="noopener noreferrer"`

**Files:** `ReferralSection.tsx`

---

## #49 — Shenanigans live feed as desktop right-side panel (B-28)

**Problem:** Live feed and stats sat below the cards on all viewports. On desktop, the feed should be a persistent right-side panel.

**Solution:**
1. Wrapped main Shenanigans content in `<div className="mc-shenanigans-layout">` — flex column on mobile, `grid 1fr 320px` at ≥1024px
2. Left column: filter tabs, shenanigan cards grid (changed from `lg:grid-cols-3` to `md:grid-cols-2` since the sidebar takes ~320px), empty state, guardrails
3. Right column (`mc-shenanigans-sidebar`): stats grid (changed from `md:grid-cols-4` to `grid-cols-2` to fit narrow sidebar) + live feed
4. Sidebar is sticky on desktop: `position: sticky; top: 160px; max-height: calc(100vh - 200px); overflow-y: auto; align-self: start`
5. PP balance bar stays above the layout; Hall of Fame stays below (outside the 2-column grid)
6. Mobile: unchanged — everything stacks vertically

**Files:** `Shenanigans.tsx`, `index.css`

---

## #50 — Time-based leaderboard filters (B-29)

**Problem:** No "This Round" / "All Time" toggle. Data always shows all-time stats.

**Solution:**
1. **Backend dependency confirmed blocked:** No round-scoped leaderboard query exists (`useGetTopPonziPointsHolders` / `useGetTopPonziPointsBurners` have no time filter parameter)
2. Followed plan's fallback: added UI toggle with "This Round" button **disabled** (`cursor-not-allowed opacity-50`) and tooltip `"Coming after next round reset"`
3. "All Time" button styled as active (`bg-[var(--mc-purple)]/25`)
4. Placed above the "Your Rank" banner in the HallOfFame layout

**Files:** `HallOfFame.tsx`

---

## #51 — Mobile bottom sheets for all dialogs (B-30)

**Problem:** WalletDropdown uses a mobile bottom sheet, but the withdrawal dialog still uses a centered shadcn Dialog modal on mobile.

**Solution:**
1. Created reusable `MobileSheet.tsx` component:
   - Detects mobile via `window.innerWidth < 769` (with resize listener)
   - Mobile: renders `mc-sheet-backdrop` + `mc-bottom-sheet` with drag-to-dismiss touch handling (dismiss if dragged >30% of sheet height)
   - Desktop: renders shadcn `Dialog` / `DialogContent` as before
2. Replaced `Dialog` imports with `MobileSheet` in `GameTracking.tsx`
3. Converted withdrawal dialog from `Dialog/DialogContent/DialogHeader/DialogTitle/DialogDescription/DialogFooter` to `MobileSheet` with plain HTML structure inside
4. **Scoping note:** Shenanigans confirm/outcome overlays already use custom fixed-position divs (not shadcn Dialog), so they don't need MobileSheet conversion. The reinvest flow uses toast-based confirmation, not a dialog. Only the withdrawal dialog required conversion.

**Files:** `MobileSheet.tsx` (new), `GameTracking.tsx`

---

## #52 — Charles personality throughout the app (B-31)

**Problem:** Charles only appears in the admin panel, GameTracking empty state, and the splash quote. The v1 task list wanted Charles throughout the app.

**Solution:**
5 personality injections:
1. **HouseDashboard Risk & Rewards section:** Added Charles fee quote — `"Charles takes a cut on every deposit. His table, his rules."` (italic, muted, font-accent)
2. **HouseDashboard redistribution callout:** Changed generic explainer to `"When the pot runs dry, Charles resets the table. No exceptions." — Charles`
3. **GameTracking House card subtitle:** Changed from `"But here's how much."` to `"Charles collects a 7% exit toll if you leave within 3 days. His table, his rules."` — informative + personality
4. **GameTracking error state:** Added `"Even Charles couldn't fix this one."` above the error message in mc-text-muted italic
5. **LoadingSpinner (global):** Added random Charles-themed loading messages below the spinner: "Charles is counting the money...", "Charles is shuffling the deck...", "Warming up the Ponzi engine...", "Charles is reviewing your application...", "Checking if the pot is still there..." — selected once on mount via `useState`, displayed as `font-accent italic animate-pulse`

**Files:** `HouseDashboard.tsx`, `GameTracking.tsx`, `LoadingSpinner.tsx`

---

## #53 — Gold notification badge on MLM tab (B-32)

**Problem:** Red (Profit Center) and purple (Shenanigans) badge dots exist. Gold (The Pyramid) for unviewed referral activity was specified but never implemented.

**Solution:**
1. Added `useGetReferralStats` import to App.tsx
2. **Badge signal:** Uses `totalEarnings` from referral stats compared against `localStorage.getItem('mc_last_seen_referral_earnings')`. Shows gold badge when `totalEarnings > lastSeen`. (Note: `level1Count` always returns 0 from backend — only points are tracked, so `totalEarnings` is the correct proxy for activity.)
3. **Clear on visit:** `useEffect` sets `localStorage` value when `activeTab === 'mlm' && referralStats` — clears the badge
4. Expanded badge type from `'red' | 'purple' | null` to `'red' | 'purple' | 'gold' | null` in both `App.tsx` and `Dashboard.tsx`
5. Updated badge rendering ternary in both files: `badge === 'red' ? 'mc-badge-red' : badge === 'gold' ? 'mc-badge-gold' : 'mc-badge-purple'`
6. Added `.mc-badge-gold` CSS: `background: var(--mc-gold); box-shadow: 0 0 6px rgba(255, 215, 0, 0.5)` — inherits pulse animation from `.mc-badge-dot`

**Files:** `App.tsx`, `Dashboard.tsx`, `index.css`

---

## Files Modified

| File | Tasks |
|------|-------|
| `frontend/src/App.tsx` | #47, #53 |
| `frontend/src/index.css` | #47, #49, #53 |
| `frontend/src/components/ReferralSection.tsx` | #48 |
| `frontend/src/components/Shenanigans.tsx` | #49 |
| `frontend/src/components/HallOfFame.tsx` | #50 |
| `frontend/src/components/MobileSheet.tsx` | #51 (new) |
| `frontend/src/components/GameTracking.tsx` | #51, #52 |
| `frontend/src/components/HouseDashboard.tsx` | #52 |
| `frontend/src/components/LoadingSpinner.tsx` | #52 |
| `frontend/src/components/Dashboard.tsx` | #53 |

## Spec Coverage Audit

| # | Title | Spec'd | Implemented | Notes |
|---|-------|--------|-------------|-------|
| 47 | Splash card pacing | Card 1 bigger+glow, Card 2 caution, Card 3 elevated+gold, mobile stagger wider | ✅ All | Pulsing glow on Card 1 + static gold on Card 3 |
| 48 | Facebook share button | `facebook.com/sharer/sharer.php?u=` | ✅ | URL-only (Facebook limitation) |
| 49 | Desktop right-side panel | 2-col grid ≥1024px, feed+stats right, sticky sidebar | ✅ All | Sidebar 320px, sticky top:160px |
| 50 | Time-based leaderboard filters | Toggle UI, "This Round" disabled if blocked | ✅ Blocked path | No backend support; disabled button with tooltip |
| 51 | Mobile bottom sheets | Reusable MobileSheet, convert all dialogs | ✅ Scoped | Only withdrawal dialog uses shadcn Dialog; others use custom overlays |
| 52 | Charles personality | HouseDashboard fees, redistribution, GameTracking toll, error states, loading states | ✅ All 5 | Loading spinner now globally shows Charles lines |
| 53 | Gold MLM badge | Gold dot on MLM tab, localStorage tracking, CSS | ✅ All | Uses totalEarnings as proxy (counts unavailable from backend) |
