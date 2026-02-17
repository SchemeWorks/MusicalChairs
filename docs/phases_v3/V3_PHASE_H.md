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

