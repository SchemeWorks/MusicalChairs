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

