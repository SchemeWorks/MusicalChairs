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

