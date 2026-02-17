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

