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

