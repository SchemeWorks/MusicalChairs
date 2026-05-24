# Admins Exempt from Hall of Fame — Design

**Date:** 2026-05-24
**Status:** Spec — ready for plan
**Context:** Charles (the admin / "the house") is currently free to outburn every player on the Hall of Fame leaderboard. The user wants to remove themselves from the public ranking — both as a fairness move (others should get to top it) and as a brand move (the house never plays its own table). This is a frontend-only display filter; backend state is unchanged. The admin set is the existing `CHARLES_PRINCIPALS` list in `frontend/src/lib/charles.tsx` (4 principals).

## Goals

- Charles principals do not appear on the Hall of Fame leaderboard (podium or list).
- Non-admin player rankings re-rank contiguously (1..N) with admins removed — no skipped numbers.
- When Charles loads the Hall of Fame, the "Your Rank" banner becomes a "House Status" banner that explicitly says they're not ranked.

## Non-goals

- No backend changes. The `getTopPpBurners` shenanigans query still returns admins; the frontend filters on read.
- No changes to PP economics, round mechanics, or how PP burning is recorded.
- No configurable admin-exemption setting. `CHARLES_PRINCIPALS` is the single source of truth.
- No effect on other surfaces (trollbox, spell-cast log, Live Feed) — Charles still appears there.

## 1. Filter at the hook layer

In `useGetTopPonziPointsBurners` (`frontend/src/hooks/useQueries.ts`), drop entries whose principal matches `isCharles(...)`, then re-index ranks so the remaining list is contiguous starting at 1.

Current shape:
```ts
return burners.map(([principal, unitsBig], index) => ({
  rank: index + 1,
  ponziPointsBurned: Number(unitsBig / 100_000_000n),
  principal: principal.toString(),
}));
```

New shape:
```ts
return burners
  .filter(([principal]) => !isCharles(principal.toString()))
  .map(([principal, unitsBig], index) => ({
    rank: index + 1,
    ponziPointsBurned: Number(unitsBig / 100_000_000n),
    principal: principal.toString(),
  }));
```

`isCharles` is already exported from `frontend/src/lib/charles.tsx` — import alongside the existing imports in `useQueries.ts`.

## 2. "House Status" banner variant in HallOfFame

The current "Your Rank" banner in `HallOfFame.tsx`:

```tsx
<div className="mc-card p-4 flex items-center justify-between">
  <div className="flex items-center gap-3">
    <Target className="h-5 w-5 mc-text-cyan" />
    <div>
      <span className="text-xs mc-label">Your Rank (Diamond Tier)</span>
      <div className="font-bold mc-text-primary text-sm">
        {userBurnerRank !== undefined && userBurnerRank >= 0 ? (
          <span className={userBurnerRank < 3 ? 'mc-text-gold mc-glow-gold' : ''}>
            #{userBurnerRank + 1} of {burnersData?.length || 0} burners
          </span>
        ) : (
          <span className="mc-text-muted">Unranked — burn PP to climb</span>
        )}
      </div>
    </div>
  </div>
  <div className="text-right">
    <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
    <div className="text-xs mc-text-muted">PP</div>
  </div>
</div>
```

When `principal && isCharles(principal)`, render this variant instead:

```tsx
<div className="mc-card p-4 flex items-center justify-between">
  <div className="flex items-center gap-3">
    <CharlesIcon className="h-5 w-5 mc-text-gold" />
    <div>
      <span className="text-xs mc-label">House Status</span>
      <div className="font-bold mc-text-primary text-sm">Not ranked</div>
      <div className="text-xs mc-text-muted italic">The house never plays its own table.</div>
    </div>
  </div>
  <div className="text-right">
    <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
    <div className="text-xs mc-text-muted">PP</div>
  </div>
</div>
```

Right-side PP balance preserved — it's the caller's holding, not their rank. `CharlesIcon` is already exported from `frontend/src/lib/charles.tsx`.

## 3. Empty-list placeholder count is already correct

The existing placeholder line ("Only N burners so far…") reads from the filtered `burnersData.length`. Since the filter happens at the hook layer, the count is already post-admin-removal — nothing to change in the placeholder logic.

## 4. Edge cases

- **Caller not logged in:** `principal` is null. Guard with `principal ? isCharles(principal) : false` at the banner branch. The `isCharles` function takes a string, so passing null would compile-fail anyway — the guard is for runtime clarity.
- **List goes empty after filter:** If the only burner is Charles, the post-filter list has zero entries. The existing `!hasData` branch in `HallOfFame` already handles the empty-leaderboard case; nothing to add.
- **Admin's own PP balance:** Still displayed on the right of the House Status banner. They're the only ones who see it (it's their wallet). Unrelated to public ranking.

## 5. Files touched

**Modified:**
- `frontend/src/hooks/useQueries.ts` — add `isCharles` import, filter in `useGetTopPonziPointsBurners`
- `frontend/src/components/HallOfFame.tsx` — add `CharlesIcon` import, admin variant of the "Your Rank" banner

**Untouched:**
- Backend (no deploy needed)
- `frontend/src/lib/charles.tsx` (already has everything we need)
- Any other ranking / list surface (none currently exist)

## 6. Testing

Visual + manual:
- Load Hall of Fame as Charles → House Status banner shows; user doesn't appear on podium or list.
- Load Hall of Fame as a non-admin → standard "Your Rank" banner shows; ranking is contiguous (no gaps where admins would've been).
- Burner counts (`Only N burners so far…` placeholder) reflect post-filter count.
- Trollbox, spell-cast log, Live Feed: Charles's name still appears (filter is leaderboard-only).
