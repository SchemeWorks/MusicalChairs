# Phase D — Core User Flow Fixes — Completion Report

**Status:** ✅ All 5 tasks complete
**TypeScript errors:** 44 (all pre-existing, zero new errors introduced)
**Files modified:** 5

---

## Task #22 — Convert status bar to stacked layout (A-8)

**Files:** `frontend/src/components/GameStatusBar.tsx`, `frontend/src/index.css`, `frontend/src/App.tsx`

**Problem:** v2 plan specified labels stacked above values (vertical casino-scoreboard style). Layout was inline (label and value side-by-side).

**What was done:**

1. Changed `.mc-status-bar-stat` from `flex-direction: row` to `flex-direction: column` with `gap: 1px` and `justify-content: center`
2. Increased bar height from `36px` to `44px` to accommodate stacked layout
3. Added `line-height: 1` to both label and value to keep vertical alignment tight
4. Reduced label `letter-spacing` from `0.08em` to `0.05em` (better readability stacked)
5. Added `mc-status-bar-mobile-hide` class to Positions stat — mobile now shows only Balance, P/L, PP (3 stats, not dense)
6. Mobile labels now visible (removed `display: none` on mobile labels), reduced to `font-size: 8px`
7. Updated `App.tsx` main content `padding-top` from `36px` to `44px` to match new bar height

---

## Task #23 — Remove disabled states from CTA button (A-3)

**File:** `frontend/src/components/GamePlans.tsx`

**Problem:** CTA button had extensive `disabled` attribute and cycled through error text labels. v2 plan called for always-enabled button.

**What was done:**

1. Removed multi-condition `disabled` attribute — button only disabled during `createGameMutation.isPending`
2. Button text is always `<Dices /> START GAME` (never "Rate Limited", "Fix Input Error", etc.)
3. Button always has `mc-btn-primary` styling, never grayed out — `pulse` class added when amount is valid
4. Added `clickError` and `shakeError` state with `triggerClickError()` helper
5. `handleCreateGame` now validates step-by-step and shows specific inline error below button:
   - "Choose a mode first"
   - "Select a plan first"
   - "Enter an amount"
   - "Amount below minimum (0.1 ICP)"
   - "Insufficient balance"
   - "Max for simple mode: X ICP"
   - "Rate limited — wait before opening another position"
   - "Fix input error first"
6. Error display: `<p className="text-xs mc-text-danger mt-2 text-center mc-shake">` below button
7. `clickError` clears automatically when user changes mode, plan, or amount

---

## Task #24 — Add shake on error to ProfileSetup input (A-9)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** GamePlans and WalletDropdown both have shake-on-error but ProfileSetup didn't.

**What was done:**

1. Added `shakeInput` state with `triggerShake()` helper (same pattern as GamePlans)
2. On submit with empty name or name exceeding max length → `triggerShake()` fires
3. Applied `mc-shake` class to the input element conditionally
4. 400ms timeout clears the shake state

---

## Task #25 — Character count / validation feedback while typing (B-4)

**File:** `frontend/src/components/ProfileSetup.tsx`

**Problem:** No feedback about name length or validity as the user types.

**What was done:**

1. Added `MAX_NAME_LENGTH = 20` constant
2. Character counter appears below input once user starts typing: `"3/20 characters"`
3. Counter turns `mc-text-danger` (red) when over limit
4. "Too long" label appears on the right when over limit
5. "Players will see you as:" preview only shows when name is valid (non-empty and within limit)
6. HTML `maxLength` set to `MAX_NAME_LENGTH + 5` — soft limit lets user see the red counter before being hard-stopped
7. Validation in `handleSubmit` enforces the limit with shake feedback

---

## Task #26 — Casino registration desk atmospheric visual (B-23)

**Files:** `frontend/src/components/ProfileSetup.tsx`, `frontend/src/index.css`

**Problem:** ProfileSetup should feel like "walking up to a casino registration desk" but was just a plain card with an input.

**What was done:**

1. Added `mc-registration-glow` CSS class — radial purple gradient glow behind the card (`inset: -40px`, `rgba(168, 85, 247, 0.08)`)
2. Applied `mc-registration-glow` to the setup card alongside existing `mc-card-elevated`
3. Added decorative stacked CreditCard icons above the card — two overlapping cards at 40% opacity with rotation transforms (`-rotate-12` and `rotate-6`), gold and purple colors
4. Imported `CreditCard` from lucide-react

---

## Files Summary

| File | Tasks |
|------|-------|
| `frontend/src/index.css` | #22 (stacked bar CSS, mobile-hide), #26 (registration glow) |
| `frontend/src/components/GameStatusBar.tsx` | #22 (mobile-hide class on Positions) |
| `frontend/src/App.tsx` | #22 (padding-top 36→44px) |
| `frontend/src/components/GamePlans.tsx` | #23 (always-enabled CTA, inline errors) |
| `frontend/src/components/ProfileSetup.tsx` | #24 (shake), #25 (char count), #26 (glow + icons) |

## Verification

- `npx tsc --noEmit`: 44 errors (all pre-existing in useActor, useLedger, useQueries, useWallet — zero in modified files)
- Dev server: Running on port 5175, no build errors

## Spec Coverage Audit

| Task | Plan Requirement | Status |
|------|-----------------|--------|
| #22 stacked layout | ✅ Vertical label-above-value | Done |
| #22 mobile density | ✅ 2-3 key stats on mobile (Balance, P/L, PP) | Done — Positions hidden, Pot already desktop-only |
| #23 remove disabled | ✅ No disabled attribute | Done — only disabled during isPending |
| #23 always START GAME | ✅ Button text always "START GAME" | Done |
| #23 inline error | ✅ Error below button with shake | Done |
| #23 error messages | ✅ Specific validation messages | Done — 8 distinct messages |
| #24 shake on error | ✅ mc-shake on invalid submit | Done |
| #25 char count | ✅ X/20 counter while typing | Done |
| #25 red on over limit | ✅ mc-text-danger when too long | Done |
| #25 hide on empty | ✅ No "0/20" on empty field | Done |
| #26 radial glow | ✅ mc-registration-glow CSS | Done |
| #26 decorative icon | ✅ Stacked CreditCard icons | Done |
| #26 mc-card-elevated | ✅ Already had it | Confirmed |
