# Hall of Fame Podium Redesign — Design

**Date:** 2026-05-24
**Status:** Spec — ready for plan
**Context:** The current Hall of Fame "Diamond Tier" podium is a bar-chart visualization that uses about 30% of the wide card's horizontal real-estate. Each podium slot is `minWidth: 90px` with `max-w-[80px]` on the name and `truncate` applied, so names like "Cat of Wisdom" render as "Cat of Wi…" even at the most prestigious surface in the app. Whitelisted players — who spent 420 PP for the privilege of glowing — get their gold-treatment name clipped before it can read. The user wants the podium to fill the card width, the names to be large and never truncate, and the Whitelisted treatment to actually pop here. Status pills, hierarchy, and tinting follow the brainstorm choices below.

## Goals

- Podium fills the wide Hall of Fame card horizontally instead of leaving 70% empty.
- Top-3 player names render large enough to read (`text-xl`+) and never truncate.
- Whitelisted players' VIP treatment supersedes rank tinting on their card and reads as the loudest visual in the entire Hall of Fame.
- #1 has a subtle rank-based amplification (not a height difference, but border/glow/scale) so the row of cards stays aligned and clean.

## Non-goals

- No changes to the burner-ranking math, the data fetched, or which players appear.
- No changes to the `LeaderboardRow` component used for #4+. Only the top-3 podium block changes.
- No change to the Whitelisted spell mechanics or `useIsGolden`.
- No tier-naming overhaul ("Series A" / "Series Gold" stays cut, per the prior spec).

## 1. New `PodiumCard` component

Replace the existing `PodiumSlot` (bar-chart slot with avatar circle + pedestal block) with a new card-based `PodiumCard` component.

New file: `frontend/src/components/hall-of-fame/PodiumCard.tsx` (lift the podium pieces out of `HallOfFame.tsx` into a co-located folder so the redesign doesn't bloat the parent file further).

Props:
```ts
interface PodiumCardProps {
  entry: HallOfFameEntry;
  rank: 1 | 2 | 3;
}
```

Internally calls `useDisplayName` + `useIsGolden` for the entry's principal (same as today).

### Anatomy (top to bottom):

1. **Rank chip** at top-right corner of the card — a small absolute-positioned chip showing the medal icon + `#1`/`#2`/`#3` label. Gold / silver / bronze colored per existing `medals` lookup.
2. **Identicon** centered, large — `h-16 w-16` (64px). When golden, wrap in the same gold-ring container used in `UserMessageRow` (`rounded-full p-[2px] bg-[var(--mc-gold)]/40` + gold box-shadow).
3. **Player name** — `text-xl font-bold` (20px), centered, with `break-words` and `max-w-full`. No `truncate`. Long names wrap to a max of two lines via `line-clamp-2`. Uses `<GoldenName>` so golden players get the animated gold + ◆ at this larger size where the effect actually reads.
4. **PP burned** — `text-2xl font-bold mc-text-purple` (24px), centered. Suffix " PP" in smaller `text-xs mc-text-muted` next to it (e.g. `3,580 PP`).
5. **Status pill** — only rendered when the entry is golden. Small chip below the PP figure: `◆ WHITELISTED · ~Xd left` or `◆ WHITELISTED · ~Xh left`. Uses the active-effects expiry to compute remaining time. Background `bg-[var(--mc-gold)]/15`, border `border-[var(--mc-gold)]/40`, text `mc-text-gold`. No pill rendered for non-golden players.

### Card styling (the layered hierarchy):

Two orthogonal signals drive the card's visual weight: **rank** (#1 vs #2/#3) and **golden status** (Whitelisted spell active or not). Golden status supersedes rank tinting for the border/glow but does NOT remove the #1 scale-lift — both signals coexist.

| State | Border | Background | Glow / shadow | Scale |
|-------|--------|------------|---------------|-------|
| Non-golden #1 | `border-2 border-[var(--mc-gold)]/40` | `bg-[var(--mc-gold)]/8` | `0 0 24px rgba(255,215,0,0.18)` | `scale-105` |
| Non-golden #2 | `border-2 border-gray-400/40` | `bg-gray-400/5` | `0 0 16px rgba(192,192,192,0.12)` | `scale-100` |
| Non-golden #3 | `border-2 border-amber-600/40` | `bg-amber-600/5` | `0 0 16px rgba(205,127,50,0.12)` | `scale-100` |
| Golden #1 | `border-4 border-[var(--mc-gold)]` + `mc-pedestal-vip` shimmer overlay | `bg-[var(--mc-gold)]/15` | `0 0 40px rgba(255,215,0,0.45)` | `scale-105` |
| Golden #2 | `border-4 border-[var(--mc-gold)]` + `mc-pedestal-vip` shimmer overlay | `bg-[var(--mc-gold)]/15` | `0 0 32px rgba(255,215,0,0.35)` | `scale-100` |
| Golden #3 | `border-4 border-[var(--mc-gold)]` + `mc-pedestal-vip` shimmer overlay | `bg-[var(--mc-gold)]/15` | `0 0 32px rgba(255,215,0,0.35)` | `scale-100` |

The `mc-pedestal-vip` class already exists from the Whitelisted VIP treatment work and gives the animated gold-sweep `::before` overlay. Reuse it here so the card surface itself shimmers when golden.

The rank chip in the top corner keeps its rank-tinted color regardless of golden status (#1 chip stays gold-text, #2 chip stays silver-text, #3 chip stays amber-text) — so even when a #2 card is gold-bordered, the small chip still labels them as silver, preserving rank legibility.

## 2. New `<Podium>` layout

Replace the existing `<Podium>` flex container with a CSS grid (or flex-row) of exactly three cards, filling the card width.

Order: `[#2, #1, #3]` on desktop (preserves the existing podium left-center-right convention), `[#1, #2, #3]` on narrow viewports (stacked vertically, best-to-worst top-down).

Implementation:
```tsx
<div className="grid grid-cols-3 gap-3 sm:gap-4 mb-6">
  {/* desktop order via CSS or via reordered children */}
</div>
```

For mobile stacking, use Tailwind responsive classes: `grid-cols-1 sm:grid-cols-3`. The desktop podium-order (#2 left, #1 center, #3 right) is achieved either by reordering children at the parent level (current approach) or by `order-*` Tailwind utilities. Prefer the existing pattern — reorder the array — and use mobile `order-*` to flip back to natural rank order when stacked vertically.

The cards expand to fill the grid cells; no fixed widths. Inner padding `p-4` to `p-5`. The `scale-105` on #1 reads cleanly because the grid cell is fixed and the card scales inside it.

## 3. Empty-list placeholder under the podium

The Hall of Fame `mc-card-elevated` block currently shows a `LeaderboardRow` list for entries from index 3 onwards. When there are exactly 3 or fewer burners (the user's current state — 3 total), this list is empty and leaves visible dead space below the podium.

Add a small inline placeholder rendered when `burnersData.length <= 3`:

```tsx
<div className="text-center py-6 text-xs mc-text-muted italic">
  Only 3 burners so far. Anyone with ≥1 PP burned can join the leaderboard.
</div>
```

Wording matches the existing voice (see `mc-text-muted italic` use elsewhere in the file for the same kind of empty-state). Not overbearing; doesn't try to be a CTA.

## 4. `<GoldenName>` truncation behavior

Already handled in the prior Whitelisted spec — `<GoldenName>` forwards `truncate` / `max-w-*` / `whitespace-*` classes to the inner text span. The podium card passes no truncation classes, just `text-xl font-bold text-center` and `line-clamp-2` on the wrapping element. `line-clamp` works on block-level elements, so wrap the `<GoldenName>` in a parent `<div>` with `line-clamp-2` rather than passing line-clamp to GoldenName itself (which renders an inline-flex span when golden).

Concretely:
```tsx
<div className="text-xl font-bold text-center line-clamp-2 leading-tight">
  <GoldenName name={displayName} isGolden={isGolden} />
</div>
```

This gives long names 2 lines of room and the gold gradient still applies on both lines because the gradient is on the inner text-bearing span, not the wrapping line-clamp container.

## 5. Status pill — duration calculation

The chip needs the remaining time on the Whitelisted spell. Source: `getActiveSpellEffects(principal).golden` is currently a bare boolean. The expiry timestamp lives elsewhere in the backend (the goldenName effect record probably has an `expiresAt` field, similar to `ShieldState.expiresAt` already used in `Shenanigans.tsx`'s `formatRemaining` helper).

Two paths:

**Option A (recommended):** Promote `golden` from `boolean` to `[] | [GoldenStatus]` (record with `expiresAt`) in the backend's `ActiveSpellEffects` type, then read expiry in the frontend. This is the right long-term shape and a one-Motoko-line change in the backend's `getActiveSpellEffects` implementation. Mirror the existing `shield: [] | [ShieldState]` pattern. **Requires a backend deploy.**

**Option B (defer):** Show the pill without a duration — just `◆ WHITELISTED`. Faster to ship, no backend change, but the pill loses information. If we want a duration without a backend change we'd have to read the goldenName cast event from chat history and compute expiry from that — ugly and brittle.

Recommendation: ship Option B in v1 of this redesign (no duration in the pill, just the label). File the backend change as a follow-up so the pill can show duration in v2. The user has explicitly flagged backend-deploy safety in their memory notes — not worth coupling this purely-cosmetic frontend redesign to a backend change.

## 6. Files touched

**New:**
- `frontend/src/components/hall-of-fame/PodiumCard.tsx`
- `frontend/src/components/hall-of-fame/Podium.tsx` (lifted from inline definitions in `HallOfFame.tsx`)

**Modified:**
- `frontend/src/components/HallOfFame.tsx` — replace inline `PodiumSlot` + `Podium` with imports from the new folder; add the empty-list placeholder under the podium.

**No changes:**
- `LeaderboardRow` keeps its current behavior for #4+.
- `<GoldenName>` already handles long names correctly via the truncation-class splitter.
- Backend stays untouched in v1 (Option B above).

## 7. Out of scope for v1

- Backend change to expose Whitelisted expiry timestamp on `ActiveSpellEffects.golden` (Option A above) — follow-up.
- Animations on hierarchy changes (a player ranking up from #4 to #3 should ideally have a transition). Punted to follow-up.
- "Days as #1" / "Longest reign" prestige stats on the cards — could be added later once we have the data.
- Hall of Fame as a separate full-screen view — currently lives inside the Shenanigans tab. Not changing that here.

## 8. Accessibility

- Card has `role="group"` and `aria-label="Rank #N: <name>, <PP> PP burned"` for screen readers.
- Status pill text is part of the card's aria-label when present: `aria-label="Rank #N: <name>, <PP> PP burned, Whitelisted"`.
- `scale-105` and `mc-pedestal-vip` shimmer respect `prefers-reduced-motion` via the existing CSS rule (already in place from the Whitelisted spec).

## 9. Testing

No automated tests (repo has none). Verification is visual:
- Cast Whitelisted and check #1 (user's own row): card should ramp glow + thicker border + animated shimmer overlay + ◆ + animated name + WHITELISTED pill.
- Check #2 and #3 cards: silver/bronze tinting baseline, supersede to gold when their player is golden.
- Long name test: ensure a player with a 16+ character name (e.g. via renameSpell to "Cap Table Casualty Wisdom Cat" — 28 chars) wraps to 2 lines and does NOT truncate.
- Empty list state: with only 3 burners total, the area below the podium shows the placeholder line, not a blank gap.
- Mobile viewport: cards stack vertically, top-down rank order.
- Reduced motion: animations halt, gold treatment becomes static.
