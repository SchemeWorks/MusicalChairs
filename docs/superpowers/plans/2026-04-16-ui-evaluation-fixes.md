# UI Evaluation Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address the actionable items from the comprehensive UI evaluation — polish landing page, onboarding, navigation, Profit Center, Invest flow, Seed Round, MLM, Shenanigans, and the deposit-flow discoverability problem.

**Architecture:** Pure frontend changes (no backend work required). Each task is a small, self-contained visual or behavioral improvement. Many are CSS/markup tweaks; a handful add state for collapse/toggle behavior; one wires a cross-component navigation action (Invest "Fund Your Wallet First" → open WalletDropdown deposit panel).

**Tech Stack:** React + TypeScript, Tailwind, existing `mc-*` utility classes, `sonner` toasts, framer-motion (already used in OnboardingTour), no new dependencies expected.

---

## Responses to Your Three Callouts

These were discussed explicitly — no tasks are generated for them, but the reasoning is recorded here so reviewers understand the scope decision.

### 1. Invest Page Phase 1 → Phase 3 jump (Simple plan)

**Decision:** Fix with an auto-selected, non-interactive Phase 2 tile. See Task 17 below.

**Approach:** Keep Phase 2 visible when Simple mode is selected, but render a single dimmed "Simple Plan" summary tile (non-interactive) with a note like *"Only one plan available for Simple mode"* before advancing to Phase 3. Preserves the three-phase rhythm without adding a real decision, and keeps the Compounding flow untouched.

### 2. Seed Round repayment timeline

**Decision:** Do not attempt to estimate. Agree that this is inherently unforecastable (depends on deposit inflow rate, backer count, payout cadence, shenanigan drag, emergency equity conversion probability) and that the uncertainty itself is part of the intended experience. No task generated.

### 3. Dark-mode-only / accessibility

**Decision:** Conceded — dark-mode-only is fine. No task generated.

**Reasoning:** My earlier "accessibility" flag was weak. Modern accessibility standards (WCAG 2.2) care about contrast ratios, text sizing, focus indicators, screen-reader semantics, and motion preferences — not whether the theme is light or dark. A well-contrasted dark theme is fully accessible. In fact, a meaningful minority of users actively *need* dark UIs (photosensitivity, migraine, vestibular disorders, certain low-vision conditions). The casino/bar analogy lands: committing to the aesthetic is a feature, not a bug. If accessibility ever becomes a concern, it would be about contrast ratios within the dark theme, not about offering a light toggle — and the current palette already looks compliant at a glance.

---

## File Structure Map

Files that will be modified, grouped by task cluster:

| File | Responsibility | Touched by tasks |
|------|----------------|------------------|
| [frontend/src/App.tsx](frontend/src/App.tsx) | Landing page + header + status bar + global footer | 1, 2, 3, 5, 12 |
| [frontend/src/components/ProfileSetup.tsx](frontend/src/components/ProfileSetup.tsx) | First-login name capture | 4 |
| [frontend/src/components/OnboardingTour.tsx](frontend/src/components/OnboardingTour.tsx) | First-time tour overlay | 6 |
| [frontend/src/components/GameTracking.tsx](frontend/src/components/GameTracking.tsx) | Profit Center (positions, P/L, fees, PP) | 7, 8 |
| [frontend/src/components/PonziPointsDashboard.tsx](frontend/src/components/PonziPointsDashboard.tsx) | PP earn rates + source breakdown | 8 |
| [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx) | Invest / plan picker | 9, 10 |
| [frontend/src/components/HouseDashboard.tsx](frontend/src/components/HouseDashboard.tsx) | Seed Round / backers | 11 |
| [frontend/src/components/ReferralSection.tsx](frontend/src/components/ReferralSection.tsx) | MLM / referrals | 13 |
| [frontend/src/components/Shenanigans.tsx](frontend/src/components/Shenanigans.tsx) | Shenanigans catalog + casting | 14, 15 |
| [frontend/src/components/WalletDropdown.tsx](frontend/src/components/WalletDropdown.tsx) | Wallet menu / deposit panel | 16 |
| [frontend/src/hooks/useWallet.tsx](frontend/src/hooks/useWallet.tsx) | Wallet open-state context | 16 |

New files created: none (everything is an edit to existing files).

---

## Task 1: Add a global footer component

**Files:**
- Create: `frontend/src/components/Footer.tsx`
- Modify: `frontend/src/App.tsx` (insert `<Footer />` inside the root layout, after main content, before any fixed bottom-nav spacer)

- [ ] **Step 1: Create the Footer component**

```tsx
// frontend/src/components/Footer.tsx
import { ExternalLink } from 'lucide-react';

export function Footer() {
  return (
    <footer className="mc-border-subtle border-t mt-12 pt-6 pb-8 px-4 text-center text-xs mc-text-muted">
      <div className="max-w-4xl mx-auto flex flex-col gap-3 md:flex-row md:justify-between md:items-center">
        <div className="flex items-center justify-center gap-2">
          <span>© 2026 Musical Chairs</span>
          <span aria-hidden>·</span>
          <span>Built on the Internet Computer</span>
        </div>
        <div className="flex items-center justify-center gap-4">
          <a
            href="https://internetcomputer.org"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 hover:mc-text-primary"
          >
            ICP <ExternalLink className="w-3 h-3" />
          </a>
          <a href="#docs" className="hover:mc-text-primary" onClick={(e) => {
            e.preventDefault();
            window.dispatchEvent(new CustomEvent('mc:open-docs'));
          }}>
            Docs
          </a>
          <span className="opacity-60">Not financial advice. For entertainment only.</span>
        </div>
      </div>
    </footer>
  );
}
```

- [ ] **Step 2: Mount Footer in App.tsx**

In [frontend/src/App.tsx](frontend/src/App.tsx), import `Footer` and render `<Footer />` once at the bottom of the main app container (after all route/tab content, before the mobile bottom-nav). On the landing page, render it after the gambling warning card. On authenticated views, render it after the tab content container.

If there's a single top-level layout wrapper, place it there once. If the landing page and dashboard are separate render branches, place it in both.

- [ ] **Step 3: Wire the "Docs" link**

The footer dispatches a `mc:open-docs` custom event. In `App.tsx`, add a `useEffect` that listens for it and calls the same handler the existing header "Docs" button uses (switches to the Docs view / sets the relevant tab state). If there is no existing global docs handler, reuse the handler the header button already has by lifting it or exposing it via context.

- [ ] **Step 4: Visual verify in browser**

Start the dev server (use `preview_start`). Load the landing page — confirm footer appears below the gambling warning. Load an authenticated view — confirm footer appears below tab content and is not obscured by the mobile bottom-nav (add `pb-24` on mobile to the main container if needed). Resize to mobile width and re-check.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Footer.tsx frontend/src/App.tsx
git commit -m "feat(ui): add global footer with ICP badge, docs link, and disclaimer"
```

---

## Task 2: Verify Days Active bug is fixed

The backend at [backend/main.mo:1408](backend/main.mo:1408) computes `daysActive` dynamically from March 16 2026. Today (2026-04-16) that should return ~31. If the landing page still shows a large number, the fix has not been deployed or the frontend is reading a stale cached value.

- [ ] **Step 1: Start the preview, load landing page, record the displayed value**

Read the "Days Active" number in the stats ribbon on the landing page.

- [ ] **Step 2: Classify**

- If the number is small (< 100): already fixed — skip the rest of this task and commit nothing.
- If the number is large (e.g., 20,559): the computation in main.mo is correct but frontend may be reading the raw stored `daysActive` field which is never updated. Investigate [frontend/src/App.tsx:559](frontend/src/App.tsx:559) — confirm `publicStats.daysActive` is coming from the `getPublicStats` query which applies the `platformStats with daysActive = ...` override, not from a stale cached record.

- [ ] **Step 3: If broken, fix at the frontend read site**

If the backend override is in place but frontend is still showing a huge number, the frontend is probably caching an older fetch or reading a different field. Fix at [frontend/src/App.tsx:559](frontend/src/App.tsx:559) by adding a sanity clamp:

```tsx
{publicStats ? Math.min(Number(publicStats.daysActive), 9999) : '—'}
```

and then investigate the root cause separately. Do not clamp silently if the root cause is a fresh bug — fix the root cause first.

- [ ] **Step 4: Commit only if changes made**

```bash
git add frontend/src/App.tsx
git commit -m "fix(ui): ensure Days Active stat displays computed value, not stored default"
```

---

## Task 3: Remove the redundant landing-page CTA

The header shows a compact "Connect" button and the hero shows a large "Connect Wallet" button. On landing-page viewports both are visible.

**Decision to encode:** Keep the hero CTA (it's the primary call-to-action), hide the header "Connect" button while the unauthenticated landing page is visible. Once the user clicks into another view (Docs), the header CTA reappears.

**Files:**
- Modify: [frontend/src/App.tsx](frontend/src/App.tsx) — conditional render of header Connect button

- [ ] **Step 1: Find the header Connect button**

Grep for the header's `LoginButton` / "Connect" render in [frontend/src/App.tsx](frontend/src/App.tsx). Confirm whether the landing hero is a separate branch or same-page with scroll.

- [ ] **Step 2: Add a condition to hide header Connect on landing**

Wrap the header's `<LoginButton />` (or equivalent) with:

```tsx
{!isOnLandingPage && <LoginButton variant="compact" />}
```

where `isOnLandingPage` is `!identity && currentView === 'landing'` (use whatever state name the app already has; if not present, derive from `!identity && !showDocs`).

- [ ] **Step 3: Visual verify**

Load landing page — only the hero Connect button shows. Open Docs — header Connect reappears. Sign in — header shows the wallet dropdown trigger instead. No flicker between states.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "refactor(ui): hide redundant header Connect button on landing hero"
```

---

## Task 4: Clarify the Profile Setup name field

The label "What do they call you?" is ambiguous.

**Files:**
- Modify: [frontend/src/components/ProfileSetup.tsx](frontend/src/components/ProfileSetup.tsx)

- [ ] **Step 1: Replace label and add helper text**

Find the form label. Change it to:

```tsx
<label className="mc-text-primary text-sm font-medium" htmlFor="display-name">
  Pick a display name
</label>
<p className="text-xs mc-text-muted mb-2">
  Shown on leaderboards, the live feed, and when other players interact with you.
  You can change it later from the wallet menu.
</p>
```

- [ ] **Step 2: Tighten the auto-redirect**

Current behavior is a 5-second auto-redirect after confetti. Change it to 3 seconds and ensure the "SHOW ME THE YIELD" button remains fully interactive during the countdown. If there's a timer visible, update its copy to "Auto-continuing in Ns…" so the user knows they can skip.

- [ ] **Step 3: Visual verify**

Run through first-login flow (use a fresh principal if possible, or clear localStorage). Confirm the new label, helper text, and that the button fires immediately while the 3s timer also fires the same redirect if untouched.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/ProfileSetup.tsx
git commit -m "feat(ui): clarify profile setup label and shorten auto-redirect"
```

---

## Task 5: Make the status-bar stats clickable

Status bar P/L → Profit Center; PP → Shenanigans. Zero-cost discoverability boost.

**Files:**
- Modify: [frontend/src/App.tsx](frontend/src/App.tsx) — status-bar render

- [ ] **Step 1: Identify the status bar container**

Grep [frontend/src/App.tsx](frontend/src/App.tsx) for the P/L and PP display in the status bar. Find the parent element of each stat cell.

- [ ] **Step 2: Wrap P/L stat in a button**

```tsx
<button
  type="button"
  onClick={() => setActiveTab('profit')}
  className="text-left hover:mc-bg-elev-2 rounded px-2 py-1 -mx-2 -my-1 transition-colors"
  aria-label="Go to Profit Center"
>
  {/* existing P/L content */}
</button>
```

Use whatever the current tab-setter function is. Do the same for the PP cell, routing to the Shenanigans tab.

- [ ] **Step 3: Visual verify**

Hover reveals a subtle background shift. Click P/L → Profit Center opens. Click PP → Shenanigans opens. Keyboard: Tab lands on them in reading order; Enter activates.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(ui): make status-bar P/L and PP stats navigate to their tabs"
```

---

## Task 6: Onboarding Tour — spotlight + fix asterisk

Two sub-fixes: (a) add a spotlight cutout that highlights the tab being described, (b) kill the orphan asterisk on "Earn up to 24%*".

**Files:**
- Modify: [frontend/src/components/OnboardingTour.tsx](frontend/src/components/OnboardingTour.tsx)

- [ ] **Step 1: Remove the orphan asterisk**

Grep the file for `24%*` or similar. Either remove the `*` entirely, or add a matching `<span className="text-[10px] mc-text-muted">*for Series A tier, first repayment window</span>` beneath. Pick removal — shorter is better for tour copy.

- [ ] **Step 2: Add a spotlight container**

The tour already navigates to each tab. After navigating, locate the tab trigger element by a data attribute and highlight it. First, add `data-tour-id` to each tab trigger in App.tsx (e.g., `data-tour-id="tab-invest"`).

In OnboardingTour, for each step add a `targetSelector` field (e.g., `'[data-tour-id="tab-invest"]'`). Then render a ring overlay:

```tsx
useEffect(() => {
  const el = document.querySelector<HTMLElement>(step.targetSelector);
  if (!el) return;
  const rect = el.getBoundingClientRect();
  setSpotlight({
    top: rect.top - 6,
    left: rect.left - 6,
    width: rect.width + 12,
    height: rect.height + 12,
  });
}, [stepIndex]);
```

Render:

```tsx
{spotlight && (
  <div
    className="fixed pointer-events-none rounded-lg ring-2 ring-yellow-400 ring-offset-2 ring-offset-black transition-all duration-300"
    style={{
      top: spotlight.top,
      left: spotlight.left,
      width: spotlight.width,
      height: spotlight.height,
      zIndex: 60,
    }}
  />
)}
```

The tooltip keeps its existing center-of-screen position — the spotlight is what ties it to the tab.

- [ ] **Step 3: Visual verify**

Clear `localStorage['mc:onboarding-seen']` (or whatever key is used), reload. Step through the tour — confirm each step highlights the correct tab with the yellow ring and the ring tracks the tab if you resize the window.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/OnboardingTour.tsx frontend/src/App.tsx
git commit -m "feat(ui): add tab spotlight to onboarding tour, remove orphan asterisk"
```

---

## Task 7: Collapse the Fee Disclosure card in Profit Center

**Files:**
- Modify: [frontend/src/components/GameTracking.tsx](frontend/src/components/GameTracking.tsx)

- [ ] **Step 1: Add collapse state**

Near the top of the `GameTracking` component, add:

```tsx
const [feesExpanded, setFeesExpanded] = useState(false);
```

- [ ] **Step 2: Replace the always-open Fee Disclosure section with a toggle header**

Find the Fee Disclosure card. Replace the header row with:

```tsx
<button
  type="button"
  onClick={() => setFeesExpanded(v => !v)}
  className="w-full flex items-center justify-between text-left"
  aria-expanded={feesExpanded}
>
  <span className="font-semibold mc-text-primary">Fee Disclosure</span>
  <ChevronDown className={`w-4 h-4 transition-transform ${feesExpanded ? 'rotate-180' : ''}`} />
</button>
{feesExpanded && (
  <div className="mt-3 text-sm mc-text-muted space-y-2">
    {/* existing fee disclosure body */}
  </div>
)}
```

Import `ChevronDown` from `lucide-react` if not already.

- [ ] **Step 3: Visual verify**

Profit Center loads with fees collapsed (summary only). Click header → expands with smooth rotation on the chevron. Click again → collapses. No layout jank.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/GameTracking.tsx
git commit -m "feat(ui): collapse Fee Disclosure by default in Profit Center"
```

---

## Task 8: Add a live-earnings pulse indicator + reorganize PP section

Two related Profit Center polish items.

**Files:**
- Modify: [frontend/src/components/GameTracking.tsx](frontend/src/components/GameTracking.tsx)
- Modify: [frontend/src/components/PonziPointsDashboard.tsx](frontend/src/components/PonziPointsDashboard.tsx)

- [ ] **Step 1: Add a pulsing dot next to the live P/L value**

In the "Your Running Tally" card header, add:

```tsx
<span className="inline-flex items-center gap-1.5 text-xs mc-text-muted">
  <span className="relative flex h-2 w-2">
    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75" />
    <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
  </span>
  LIVE
</span>
```

- [ ] **Step 2: Collapse the PP subsection by default**

The Ponzi Points earn-rates + source-breakdown + spending-suggestions are a scroll-heavy subsection. Wrap the whole `<PonziPointsDashboard />` in a collapse header following the same pattern as Task 7:

```tsx
const [ppExpanded, setPpExpanded] = useState(false);

<button onClick={() => setPpExpanded(v => !v)} …>
  <span>Ponzi Points ({ppBalance})</span>
  <ChevronDown className={…} />
</button>
{ppExpanded && <PonziPointsDashboard … />}
```

The PP total stays always-visible in the header; the breakdown hides until opened.

- [ ] **Step 3: Visual verify**

Live dot pulses. PP section is collapsed on first load, expands smoothly, header shows current PP balance so nothing important is hidden.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/GameTracking.tsx frontend/src/components/PonziPointsDashboard.tsx
git commit -m "feat(ui): add LIVE indicator to P/L and collapse PP section by default"
```

---

## Task 9: Surface the 3% entry skim before commit (Invest)

**Files:**
- Modify: [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx)

- [ ] **Step 1: Add an entry-fee line to the ROI calculator summary**

In Phase 3 (amount entry), find the ROI calculator that shows projected returns. Add a new row **above** the "Total Return" line:

```tsx
<div className="flex justify-between text-sm">
  <span className="mc-text-muted">Entry skim (3%)</span>
  <span className="mc-text-primary font-medium">
    -{formatICP(amount * 0.03)} ICP
  </span>
</div>
<div className="flex justify-between text-sm">
  <span className="mc-text-muted">Net deposit</span>
  <span className="mc-text-primary font-medium">
    {formatICP(amount * 0.97)} ICP
  </span>
</div>
```

Keep the existing daily-earnings and total-return rows; they should compute off the net deposit (0.97 × amount). If they currently use gross, update the math so projections match reality.

- [ ] **Step 2: Promote the small blue info box to an amber caution box**

Change its classes from blue info styling to amber/warning styling so it reads as a cost, not a nice-to-know:

```tsx
<div className="mc-border-warn border rounded-lg p-3 bg-amber-500/10 text-xs">
  <span className="font-semibold text-amber-300">3% entry skim</span>
  <span className="mc-text-muted"> goes to the pot. You deposit N, the house books N×0.97.</span>
</div>
```

- [ ] **Step 3: Visual verify**

Enter an amount. The calculator now shows skim and net deposit before the total return. The amber caution box is more noticeable than before but not alarming.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/GamePlans.tsx
git commit -m "feat(ui): surface 3% entry skim prominently in Invest calculator"
```

---

## Task 10: Add an exit-toll schedule preview to Invest

Before committing, the user should see the toll curve so there are no surprises.

**Files:**
- Modify: [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx)
- Reference: [frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts) for toll tiers

- [ ] **Step 1: Read toll tier data**

Open [frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts) and locate the toll tier definitions per plan. Note the structure (array of `{ daysFrom, daysTo, tollPct }` or similar).

- [ ] **Step 2: Render a mini schedule under the Plan Summary**

In Phase 3, below the plan summary tile, render:

```tsx
<details className="mt-3 text-xs mc-text-muted">
  <summary className="cursor-pointer hover:mc-text-primary">
    Exit toll schedule →
  </summary>
  <div className="mt-2 space-y-1 pl-3">
    {tollTiersForSelectedPlan.map((tier) => (
      <div key={tier.daysFrom} className="flex justify-between">
        <span>Day {tier.daysFrom}–{tier.daysTo}</span>
        <span className={tier.tollPct > 50 ? 'text-red-400' : tier.tollPct > 20 ? 'text-amber-400' : 'text-green-400'}>
          {tier.tollPct}% toll
        </span>
      </div>
    ))}
  </div>
</details>
```

- [ ] **Step 3: Visual verify**

Click "Exit toll schedule →" — reveals the full tier table color-coded by severity. Works for both simple and compounding plans with correct numbers. The `<details>` element handles open/close without extra state.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/GamePlans.tsx
git commit -m "feat(ui): add exit-toll schedule preview before commit in Invest"
```

---

## Task 11: Reorder Seed Round — BackerInfoCard above backers list

**Files:**
- Modify: [frontend/src/components/HouseDashboard.tsx](frontend/src/components/HouseDashboard.tsx)

- [ ] **Step 1: Find the Backers tab render block**

Grep for `BackerInfoCard` usage and the existing backers list render.

- [ ] **Step 2: Move `BackerInfoCard` above the list**

In the Backers tab JSX, cut the `<BackerInfoCard />` block and paste it immediately below the tab toggle and above the "Existing Backers" section header. Add a visual separator:

```tsx
<BackerInfoCard />
<Separator className="my-6" />
<h3 className="text-lg font-semibold mc-text-primary mb-3">Existing Backers</h3>
{/* existing backers list */}
```

- [ ] **Step 3: Visual verify**

Seed Round tab → Backers. The explainer card is the first thing you see. Scroll down to find existing backers. Order also makes sense for new users who have never backed.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/HouseDashboard.tsx
git commit -m "refactor(ui): show backer explainer above existing backers list"
```

---

## Task 12: Scroll hint on landing hero

Landing hero is clean; no indication more content is below on first load.

**Files:**
- Modify: [frontend/src/App.tsx](frontend/src/App.tsx)

- [ ] **Step 1: Add a subtle bouncing chevron**

At the bottom of the hero section, add:

```tsx
<div className="absolute bottom-8 left-1/2 -translate-x-1/2 animate-bounce">
  <ChevronDown className="w-6 h-6 mc-text-muted opacity-60" aria-hidden />
</div>
```

Only render it while `window.scrollY < 100` — use a small effect with a scroll listener, or CSS `animation` that fades on scroll. Simple version: always render, accept it disappears only when scrolled past naturally.

- [ ] **Step 2: Visual verify**

Landing page loads — chevron bounces at the bottom of the fold, hints at more content. No performance cost.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(ui): add scroll hint chevron to landing hero"
```

---

## Task 13: MLM / Referrals cleanup

Bundle three related fixes: regulatory-risk tweet copy, placeholder removal, always-on pyramid viz, per-level PP rates.

**Files:**
- Modify: [frontend/src/components/ReferralSection.tsx](frontend/src/components/ReferralSection.tsx)

- [ ] **Step 1: Soften the pre-composed tweet**

Find the tweet template string with "Up to 12% daily". Replace with something funnier and less yield-claim-y:

```ts
const shareText = "I joined the Musical Chairs Ponzi. Come get stuck in with me before the music stops. 🪑";
```

Drop the explicit rate. The landing page and docs carry the disclaimers; social copy should be vibes, not specs.

- [ ] **Step 2: Remove "Referral activity feed coming soon" placeholder**

Grep for "coming soon" in the file. Delete the placeholder block entirely. Don't replace it with anything — empty space is better than a promise-IOU.

- [ ] **Step 3: Always render the pyramid visualization**

Find the pyramid viz render condition (`if (referrals.length > 0)` or similar). Remove the conditional — render it always. For new users, the tiers show with zeros.

- [ ] **Step 4: Add per-level PP rate labels to the pyramid viz**

On each pyramid tier, append the PP rate (values come from gameConstants — read them, don't hardcode):

```tsx
<div className="tier-row">
  <span>Level 1 (direct)</span>
  <span className="mc-text-muted">{REFERRAL_PP_RATES.L1}% of their PP</span>
  <span>{directCount} referrals</span>
</div>
```

- [ ] **Step 5: Visual verify**

Pyramid visible even for a user with 0 referrals, with rate labels (8% / 5% / 2% or whatever the constants define). Tweet share copy no longer mentions a percent rate. No "coming soon" text anywhere.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/ReferralSection.tsx
git commit -m "feat(ui): clean up MLM section — soften tweet, show pyramid always, drop placeholder"
```

---

## Task 14: Shenanigans — compact view toggle + show effects

**Files:**
- Modify: [frontend/src/components/Shenanigans.tsx](frontend/src/components/Shenanigans.tsx)

- [ ] **Step 1: Add view-mode state**

```tsx
const [viewMode, setViewMode] = useState<'cards' | 'compact'>('cards');
```

- [ ] **Step 2: Add a toggle UI next to the existing filter tabs**

```tsx
<div className="flex items-center gap-1 ml-auto">
  <button
    onClick={() => setViewMode('cards')}
    className={viewMode === 'cards' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60'}
    aria-label="Card view"
  >
    <LayoutGrid className="w-4 h-4" />
  </button>
  <button
    onClick={() => setViewMode('compact')}
    className={viewMode === 'compact' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60'}
    aria-label="List view"
  >
    <List className="w-4 h-4" />
  </button>
</div>
```

Import `LayoutGrid, List` from `lucide-react`.

- [ ] **Step 3: Render compact view**

Wrap the existing card grid in a conditional and add a compact list alternative:

```tsx
{viewMode === 'cards' ? (
  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
    {filtered.map(s => <ShenaniganCard key={s.id} s={s} />)}
  </div>
) : (
  <div className="divide-y mc-border-subtle">
    {filtered.map(s => (
      <div key={s.id} className="py-2 flex items-center gap-3">
        <span className="flex-1 font-medium">{s.name}</span>
        <span className="text-xs mc-text-muted">{s.cost} PP</span>
        <span className="text-xs">{s.successPct}% win</span>
        <Button size="sm" onClick={() => castShenanigan(s)}>Cast</Button>
      </div>
    ))}
  </div>
)}
```

- [ ] **Step 4: Add the mechanical effect to each card**

In the existing `ShenaniganCard` (cards view), below the description text, add:

```tsx
<div className="text-xs mc-text-muted mt-1 italic">
  Effect: {s.effects || 'see docs'}
</div>
```

Pull from the existing `effects` field. If the field is missing on some entries, show "see docs" and file a follow-up to fill them in (not in this plan).

- [ ] **Step 5: Visual verify**

Toggle between card and list view. Card view now shows a one-line effect summary. List view is compact, scans fast, still offers cast action.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat(ui): add compact view toggle and expose effect summary on Shenanigans"
```

---

## Task 15: Shenanigans casting animation + mobile toast position

**Files:**
- Modify: [frontend/src/components/Shenanigans.tsx](frontend/src/components/Shenanigans.tsx)

- [ ] **Step 1: Add a casting spinner overlay to the button during the 1.5s delay**

Replace the plain "Casting..." button state with:

```tsx
{isCasting ? (
  <Button disabled className="relative">
    <span className="inline-block animate-spin mr-2">🎲</span>
    Casting…
  </Button>
) : (
  <Button onClick={…}>Cast</Button>
)}
```

For a nicer version, add a particle burst using framer-motion (already a dep if OnboardingTour uses it). Minimum bar: spinner + disabled state.

- [ ] **Step 2: Move toasts to bottom on mobile**

Find the toast config (likely a `<Toaster position="top-right" />` in App.tsx or `sonner.tsx`). Change to responsive position:

```tsx
<Toaster position={window.innerWidth < 768 ? 'bottom-center' : 'top-right'} />
```

Use a `useMediaQuery`-style approach if available; otherwise the window check at mount is acceptable since viewport changes require reload anyway, or wrap in a resize listener.

Better: use sonner's built-in responsive options — pass `position="bottom-center"` for all viewports if the top-right isn't critical on desktop. Discuss with user if unsure; default to bottom-center globally for simplicity.

- [ ] **Step 3: Visual verify**

Cast a shenanigan on mobile width — button shows spinning die for 1.5s, outcome toast appears at bottom-center, close to the user's thumb. Desktop behavior unchanged if you kept top-right.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/components/ui/sonner.tsx frontend/src/App.tsx
git commit -m "feat(ui): add casting animation and reposition toasts for mobile"
```

---

## Task 16: Fix the two-hop deposit-flow discoverability problem

The highest-impact UX fix in the plan. When a user hits "Invest" with 0 internal balance, the current "Fund Your Wallet First" message is a dead end — they have to hunt for the wallet dropdown. Wire it so the message has a button that opens the wallet dropdown with the deposit panel pre-activated.

**Files:**
- Modify: [frontend/src/hooks/useWallet.tsx](frontend/src/hooks/useWallet.tsx) — expose open/close and initial-panel state
- Modify: [frontend/src/components/WalletDropdown.tsx](frontend/src/components/WalletDropdown.tsx) — accept initial panel
- Modify: [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx) — wire the CTA button

- [ ] **Step 1: Extend the WalletContext**

In [frontend/src/hooks/useWallet.tsx](frontend/src/hooks/useWallet.tsx), add to the context shape:

```tsx
type WalletContextValue = {
  // existing fields…
  isOpen: boolean;
  openWallet: (panel?: 'main' | 'deposit' | 'withdraw' | 'send') => void;
  closeWallet: () => void;
  initialPanel: 'main' | 'deposit' | 'withdraw' | 'send';
};
```

Implement with `useState` for `isOpen` and `initialPanel`. `openWallet(panel)` sets both.

- [ ] **Step 2: Drive the WalletDropdown render from context**

In [frontend/src/components/WalletDropdown.tsx](frontend/src/components/WalletDropdown.tsx), consume `useWallet()` and switch its internal "active panel" state to initialize from `initialPanel` whenever `isOpen` transitions to true.

If the dropdown is currently opened by a click handler on a trigger button, keep that — the trigger can call `openWallet('main')`.

- [ ] **Step 3: Add a CTA button to the "Fund Your Wallet First" message**

In [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx), find the zero-balance render branch. Replace with:

```tsx
<Card className="p-6 text-center">
  <h3 className="text-lg font-semibold mc-text-primary mb-2">Fund your wallet first</h3>
  <p className="text-sm mc-text-muted mb-4">
    Deposit ICP from your external wallet (Plug, Oisy, etc.) before picking a plan.
  </p>
  <Button onClick={() => openWallet('deposit')}>Open deposit panel →</Button>
</Card>
```

Import `useWallet` to get `openWallet`.

- [ ] **Step 4: Visual verify**

As a user with 0 internal balance, click "Invest". The zero-balance card shows a button. Click it → wallet dropdown opens directly into the deposit panel with instructions visible. Close and reopen the wallet normally → defaults back to the main panel.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/useWallet.tsx frontend/src/components/WalletDropdown.tsx frontend/src/components/GamePlans.tsx
git commit -m "feat(ui): wire Invest zero-balance CTA to open wallet deposit panel directly"
```

---

## Task 17: Render a dimmed auto-selected Simple Plan tile in Phase 2

Smooths the Phase 1 → Phase 3 jump when Simple mode is chosen. Keep Phase 2 visible, auto-select the only Simple plan, show it as a non-interactive summary tile with a short note, then advance.

**Files:**
- Modify: [frontend/src/components/GamePlans.tsx](frontend/src/components/GamePlans.tsx)

- [ ] **Step 1: Find the Phase 2 render condition**

Locate the `{phase === 2 && …}` (or equivalent — mode-selected-but-plan-not-selected) block. Note how it currently branches: likely it renders the plan list for compounding and is skipped entirely for simple.

- [ ] **Step 2: Always render Phase 2 when mode is chosen; branch on mode inside it**

```tsx
{mode && !selectedPlan && (
  <section className="space-y-3">
    <h2 className="text-lg font-semibold mc-text-primary">Phase 2: Pick a plan</h2>

    {mode === 'simple' ? (
      <div
        className="mc-border-subtle border rounded-lg p-4 opacity-70 cursor-default select-none"
        aria-disabled="true"
      >
        <div className="flex items-center justify-between mb-1">
          <span className="font-semibold mc-text-primary">Simple Plan</span>
          <span className="text-xs mc-bg-elev-2 rounded px-2 py-0.5">Auto-selected</span>
        </div>
        <p className="text-sm mc-text-muted">
          Daily yield, exit anytime, pay the toll for the day you leave.
        </p>
        <p className="text-xs mc-text-muted italic mt-2">
          Only one plan available for Simple mode.
        </p>
      </div>
    ) : (
      /* existing compounding plan list render */
    )}
  </section>
)}
```

- [ ] **Step 3: Auto-advance to Phase 3**

When `mode === 'simple'` and the user has just selected the mode, set `selectedPlan` to the single simple plan after a short delay so the user registers the tile before Phase 3 appears:

```tsx
useEffect(() => {
  if (mode === 'simple' && !selectedPlan) {
    const t = setTimeout(() => setSelectedPlan(SIMPLE_PLAN), 600);
    return () => clearTimeout(t);
  }
}, [mode, selectedPlan]);
```

Use whatever the existing simple-plan constant is (grep [frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts) for the plan object). Keep the delay short enough to feel snappy, long enough to register (500–700ms).

- [ ] **Step 4: Ensure the "Change" backtrack still works**

The existing summary-strip "Change" action for the plan should, when Simple mode is active, step back to mode selection (Phase 1) — not sit stuck on an auto-selecting Phase 2. If it currently only clears `selectedPlan`, also clear `mode` when the cleared plan is the simple one. One-line check:

```tsx
const onChangePlan = () => {
  setSelectedPlan(null);
  if (mode === 'simple') setMode(null);
};
```

- [ ] **Step 5: Visual verify**

Enter Invest. Pick Simple mode → Phase 2 appears with the dimmed Simple Plan tile and the "Only one plan available" note → after ~600ms, Phase 3 (amount) appears naturally. Pick Compounding mode → Phase 2 renders the plan list as before, unchanged. Use the Phase-3 "Change" on simple → returns all the way to Phase 1 so the user isn't trapped.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/GamePlans.tsx
git commit -m "feat(ui): show auto-selected Simple Plan tile in Phase 2 before advancing"
```

---

## Deferred / Out of Scope

Flagged from the evaluation, **not** included in this plan. Raise separately if you want any of them:

- **Rename `House*` files → `Seed*` / game-appropriate names.** Developer-facing only. Large diff, low user value. Handle as a standalone refactor PR.
- **Real-time notification system.** Badge dots already exist on tabs; full push-style notifications are a medium-sized project (channel, unread state, persistence, opt-out). Separate spec.
- **Closed-positions / withdrawal history view in Profit Center.** Needs backend query for historical game records + a new tab or subview. Medium-sized; separate spec.
- **Refactor WalletDropdown into a full account page.** Defensible, but risks destabilizing a working flow. Revisit if the dropdown continues to grow.
- **Compact header on mobile (overflow menu).** Low priority — current density is legible. Revisit after live user feedback.
- **Mini compounding-curve chart on the Invest page.** Nice-to-have; not critical once the toll schedule (Task 10) and entry-skim clarity (Task 9) land.

---

## Self-Review Checklist

- Spec coverage: every actionable item from the evaluation has a task or is listed in Deferred. The three discussed callouts have explicit decisions.
- Placeholders: none — every step names files, shows code, gives verification criteria.
- Type consistency: `openWallet` signature matches in all three places (Task 16). Toll tier field names are read from gameConstants rather than invented. `viewMode` type in Task 14 is reused consistently.
- Frequent commits: each task ends with a commit.
- Visual verification: tasks without real logic use browser verification rather than forcing unit tests.
