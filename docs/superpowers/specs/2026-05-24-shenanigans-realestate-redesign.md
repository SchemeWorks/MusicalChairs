# Shenanigans Tab Real-Estate Redesign — Design

**Date:** 2026-05-24
**Status:** Spec — ready for plan
**Context:** The current Shenanigans tab is an 11-block vertical scroll where Hall of Fame sits at positions 6–9, well below the fold. The viewing user's PP balance is shown twice (top of page as a standalone card, then again in the HoF "Your Rank" banner). Three static info blocks (guardrails, About PP, Shenanigans disclaimer) eat real estate without earning re-reads. Track Record and Karma Received are scattered as separate user-data cards instead of being grouped with the user's rank. The user wants HoF promoted to **ambient prominence** (always visible while interacting with spells) and the redundant/static cards consolidated.

The choice between two ambient flavors — banner-on-top vs. sticky-rail-sidebar — was made during brainstorming: **Variant B (sticky sidebar)**. HoF lives in a sticky right rail on desktop and as a single block at the top of the scroll on mobile (no sidebar exists at narrow widths). Podium and leaderboard stay in one block; no splitting.

## Goals

- Hall of Fame is impossible to miss: sticky right sidebar on desktop, top-of-page block on mobile.
- Spell grid remains the visual headline of the main column at all viewport sizes.
- Personal stats (rank, PP burned, track record, karma) consolidate into a single dense "You" line pinned inside HoF — no separate sidebar cards for user data.
- The 11-block page collapses to ~6 blocks by cutting redundancy and merging static text.
- Live Feed stays in the experience as a complementary "what's happening now" companion to HoF's "who's winning overall."

## Non-goals

- No backend changes. No new fields, no new endpoints, no Motoko edits.
- No changes to the spell-casting flow, the spell cards themselves, the cost math, the cooldowns, or any spell mechanics.
- No changes to other tabs (Profit, Invest, Seed, MLM).
- No new HoF data — uses existing `burnersData`, `getActiveSpellEffects`, `getUserMetrics` already wired up.
- No changes to the recently-redesigned wide-form `PodiumCard` (`frontend/src/components/hall-of-fame/PodiumCard.tsx`) — the wide podium component remains in the codebase, but is no longer rendered by the Shenanigans tab. It is unused after this change; deletion is a follow-up if no other surface picks it up.
- No "Hall of Fame as its own top-level tab" — explicitly rejected during brainstorming.

## 1. New page structure (`Shenanigans.tsx`)

The top-level layout in `Shenanigans.tsx` becomes:

```
<div className="mc-shenanigans">
  <SidebarLayout>                          // existing wrapper, keeps current sidebar mechanic
    <Main>                                  // the existing main column
      <ActiveEffectsStrip />                // unchanged, still conditional
      <FilterTabs />                        // adds the (i) guardrails tooltip — see §6
      <SpellGrid />                         // unchanged
      <CompactFooter />                     // new: merged disclaimer line — see §5
    </Main>
    <Sidebar>                               // existing right-rail
      <HallOfFameRail />                    // NEW component — see §2
      <LiveFeedPanel />                     // existing, moves below HoF in stack order
    </Sidebar>
  </SidebarLayout>
</div>
```

On mobile (whichever breakpoint the existing `mc-shenanigans-sidebar` CSS uses to collapse the sidebar — verify in implementation; the new visibility classes in Option A below should match that same breakpoint), the existing layout already collapses the sidebar to nothing. The new structure on mobile becomes a single column in this top-down order:

1. `<HallOfFameMobileBlock />` — NEW compact mobile-only variant — see §4
2. `<ActiveEffectsStrip />` — conditional, unchanged
3. `<FilterTabs />` with the new (i) tooltip
4. `<SpellGrid />` — unchanged
5. `<LiveFeedPanel />` rendered with `defaultCollapsed` — see §7
6. `<CompactFooter />` — same as desktop

We need a way for the page to render the mobile HoF block in mobile flow and the desktop HoF rail in desktop sidebar. Two implementation options:

**Option A (recommended):** Render BOTH `<HallOfFameRail />` (in the sidebar) and `<HallOfFameMobileBlock />` (at the top of the main column). Each is wrapped in a CSS visibility class matching the existing sidebar's breakpoint — e.g. `hidden lg:block` for the rail and `block lg:hidden` for the mobile block if the existing sidebar uses Tailwind's `lg` breakpoint, or matching custom CSS otherwise. The two share no rendering work; they're cheap because they read from the same data hooks. Cleanest separation, no JS responsive logic.

**Option B:** Single `<HallOfFame />` component that internally branches on viewport width. Requires a media-query hook (or `useMediaQuery` from a lib) and re-renders on resize. More state to manage; less clean.

Go with Option A.

## 2. `<HallOfFameRail />` component (desktop sidebar)

New file: `frontend/src/components/hall-of-fame/HallOfFameRail.tsx`

Sticky right-rail component. Renders in the existing `mc-shenanigans-sidebar` container.

### Anatomy (top to bottom inside the card):

1. **Header row** — small label "Hall of Fame" + a trophy icon, like a section title.
2. **Compact podium** — three columns at the top showing top-3 burners. Visual treatment:
   - 3 columns `grid-cols-3 gap-1`
   - Order: `[#2, #1, #3]` (preserves the left-center-right podium convention)
   - Each column shows: rank chip (#1 gold / #2 silver / #3 bronze, small) on top, identicon `h-10 w-10` centered, player display name `text-xs font-semibold` (truncated to one line via `truncate`, full name available via `title=` attribute), PP burned as `text-sm font-bold mc-text-purple`.
   - The #1 column has a thicker gold border and `bg-[var(--mc-gold)]/8`; #2 silver-tinted; #3 amber-tinted. The user's existing tier styling pattern from `PodiumCard.tsx` applies, just at compact scale.
   - Golden players (`useIsGolden`) get the `<GoldenName>` treatment for their name and a gold ring around the identicon — same as today.
3. **Leaderboard list** — ranks #4 through #10 as `LeaderboardRow` components. The existing component is reused as-is; it already renders cleanly at narrow widths. If total burners ≤ 3, render the same empty-state placeholder pattern used by the current Hall of Fame leaderboard (`"Only N burners so far. Anyone with ≥1 PP burned…"`); if total burners is between 4 and 10, render only the ranks that exist.
4. **"See all" link** — `<button>` with class `mc-text-muted text-xs`, label `see all {N} →`. On click, expands the list inline to show all ranks (currently #4–end). Local component state; no routing. Only rendered when total burners > 10.
5. **Pinned "You" line** — see §3.

### Sticky behavior:

The HoF card uses `position: sticky; top: <header offset>;` inside the sidebar container. The Live Feed panel below it sits in normal flow — as the user scrolls, the HoF stays pinned to the top of the viewport while Live Feed scrolls naturally. When the user scrolls past the end of the page's content, the sticky "unsticks" at the bottom of the sidebar container (standard `position: sticky` behavior).

If the HoF card's natural height exceeds the viewport (very tall when "see all" is expanded), the sticky still pins the top, and the user scrolls inside the card. We do NOT add an internal `overflow-y: scroll` — the card grows the page, the page scrolls, sticky still works correctly.

### Charles / admin treatment:

The "House Status" pill for Charles principals (added in commit 635665b) stays. In the compact podium, Charles never appears (he's filtered out of `burnersData` per commit 4963214). In the pinned "You" line, the existing `getIsCharles` check still applies — Charles sees "House Status" instead of a numeric rank, with the same gold-treatment styling already in `HallOfFame.tsx`.

## 3. The pinned "You" line

The pinned "You" line at the bottom of the HoF rail is the single dense personal-stats display for the page. It replaces:

- The current top-of-page "Your Ponzi Points" card (§5: cut #1).
- The current "Karma Received" card inside HoF (§5: cut #7).
- The desktop sidebar "Your Track Record" card (§5: cut Track Record).
- The "Your Rank" banner inside HoF (kept conceptually; relocated to the bottom of the rail).

Layout (two-row, compact):

```
┌─ Pinned at bottom of HoF card ─────────────────────────┐
│ ★ You: rank #12 · 1,240 PP burned                       │  ← row 1: rank line (bold)
│ 47 casts · 12 good / 8 bad / 3 backfire · ✦ 4 karma     │  ← row 2: track + karma (muted small)
└─────────────────────────────────────────────────────────┘
```

- Row 1 uses `text-sm font-bold`.
- Row 2 uses `text-xs mc-text-muted`. Each stat separated by ` · `.
- Container: `border-t border-white/10 pt-2 mt-2` to visually separate from the leaderboard list above.
- Background tint: `bg-[var(--mc-purple)]/8` so the "You" row reads as distinct from the global leaderboard rows.

Charles / admin variant:

```
│ ★ You: HOUSE STATUS · — burned                          │  ← rank replaced by HOUSE STATUS pill
│ 47 casts · 12 good / 8 bad / 3 backfire · ✦ 4 karma     │  ← row 2 unchanged
```

The PP-burned figure for Charles is shown as `—` rather than a number (admin doesn't burn PP).

Data sources (all already wired up):
- Rank: `getMyHallOfFameRank()` or inferred from index in `burnersData` (matches current banner logic).
- PP burned: from the same source the current banner reads.
- Casts, outcomes: from `getUserMetrics(principal)` — already used by the Track Record card today.
- Karma: from `getUserMetrics(principal)` — already used by the Karma Received card today.

If any of those metrics are zero / null, render `0` rather than hiding the field. Density is the goal; gaps would feel buggy.

## 4. `<HallOfFameMobileBlock />` (mobile top-of-page)

New file: `frontend/src/components/hall-of-fame/HallOfFameMobileBlock.tsx`

Compact single-block variant rendered at the top of the mobile scroll. Same data, smaller footprint.

Anatomy:

1. Header label "Hall of Fame" + trophy icon (same as rail).
2. **Compact podium** — identical `grid-cols-3` layout as in the rail; identicons `h-12 w-12` (slightly larger than rail since mobile has full page width to work with).
3. **Pinned "You" line** — identical content to §3, but visually placed ABOVE the leaderboard list (not below), so the user sees their own rank before scrolling.
4. **Leaderboard preview** — shows ranks #4 through #8 (top 5 below the podium) by default.
5. **"See top 10 ↓" button** — expands inline to show ranks #9 through #10 (giving 7 visible total below the podium, plus top 3 = full top 10). After expansion, no further reveal; mobile users who want beyond #10 are not served inline. Rationale (per user input): "tap to show ten, that's fine." Only rendered when total burners > 8 (i.e. when there's actually anything to reveal beyond the default #4–#8).

The "You" line uses the same two-row format from §3. Background tint slightly heavier on mobile (`bg-[var(--mc-purple)]/12`) since it sits inside a single block rather than at the bottom of a rail.

## 5. Cuts and merges (real-estate audit)

Every current block is accounted for:

| Current block | Disposition |
|---|---|
| #1 Your Ponzi Points card (top) | **Cut.** Same data shown in pinned "You" line. |
| #2 Active Effects Strip | **Kept**, unchanged. Same position in main column. |
| #3 Spell filter tabs + view toggle | **Kept**, plus a new (i) tooltip — see §6. |
| #4 Spell cards grid | **Kept**, unchanged. |
| #5 Guardrails Info card | **Collapsed** into the (i) tooltip on the filter row — see §6. |
| Desktop sidebar: Your Track Record | **Cut.** Casts + outcomes absorbed into "You" line row 2. |
| Desktop sidebar: Live Feed | **Kept**, moved to below HoF in the sidebar stack. |
| #6 HoF: Your Rank Banner | **Cut as a separate banner**; rank moves into the pinned "You" line. |
| #7 HoF: Karma Received card | **Cut.** Karma absorbed into "You" line row 2 (`✦ N karma`). |
| #8 HoF: Diamond Tier Podium | **Replaced** by compact podium in rail/mobile block. The wide `PodiumCard` is no longer rendered here. |
| #9 HoF: Leaderboard list (#4+) | **Kept** inside the HoF rail / mobile block. Same `LeaderboardRow` component. |
| #10 About Ponzi Points card | **Merged** with #11 into one compact footer line. |
| #11 Shenanigans Disclaimer footer | **Merged** with #10 into one compact footer line. |

### `<CompactFooter />`

New file: `frontend/src/components/Shenanigans/CompactFooter.tsx` (or inline in `Shenanigans.tsx` if the component is trivial — see §8).

Replaces the two stacked disclaimers with a single line:

```tsx
<div className="text-center text-xs mc-text-muted mt-6 mb-4">
  PP &amp; cosmetics only · pure entertainment · no refunds
</div>
```

The longer copy from "About Ponzi Points" (`"Ponzi Points are in-game fun currency…"`) is dropped entirely. If the user wants to retain a more verbose explanation, the (i) tooltip on the filter row (§6) is the natural home.

## 6. Guardrails (i) tooltip

The current guardrails block (`PP & Cosmetics Only`, `Cooldowns`, `No Refunds`) becomes a small (i) info icon on the right side of the filter-tab row. Hovering (desktop) or tapping (mobile) opens a popover with the three guardrail bullets. Reuses the project's existing tooltip pattern (search for existing `Tooltip` or `Popover` usage in the codebase; if none, use a lightweight `title=` attribute and a custom `<div>` popover on click for mobile).

Implementation note: if no tooltip/popover primitive exists in the codebase already, prefer the simplest workable thing — a controlled `<div>` shown on click/tap that closes on outside-click. Do not pull in a new dependency for this.

## 7. `<LiveFeedPanel />` treatment

Desktop (in sidebar, below HoF rail):
- Default expanded (current behavior).
- Reuses existing `LiveFeedRow` rendering.
- Shows up to 20 recent casts (current cap).

Mobile (below spell grid):
- New prop `defaultCollapsed: boolean` (defaults to `false`).
- On mobile, parent passes `defaultCollapsed={true}`.
- Collapsed state shows only the panel header "Live Feed · latest casts ▾" — a button row.
- Tap expands inline to show the full list.
- State is local to the panel; no persistence across page reloads.

If `<LiveFeedPanel />` doesn't already exist as a named component (currently inline in `Shenanigans.tsx` per the exploration map), extract it to `frontend/src/components/Shenanigans/LiveFeedPanel.tsx` as part of this change.

## 8. Files touched

**New:**
- `frontend/src/components/hall-of-fame/HallOfFameRail.tsx` — desktop sticky rail variant.
- `frontend/src/components/hall-of-fame/HallOfFameMobileBlock.tsx` — mobile top-of-page variant.
- `frontend/src/components/hall-of-fame/PinnedYouLine.tsx` — shared component for the "You" stats line, used by both Rail and MobileBlock.
- `frontend/src/components/hall-of-fame/CompactPodium.tsx` — shared compact 3-column podium, used by both Rail and MobileBlock.

**Modified:**
- `frontend/src/components/Shenanigans.tsx` — replaces the page layout per §1; cuts blocks per §5; adds the (i) tooltip to the filter row per §6; passes `defaultCollapsed` to Live Feed on mobile per §7.
- `frontend/src/components/HallOfFame.tsx` — this file is no longer rendered by Shenanigans tab. The redesign removes the `<HallOfFame />` import from Shenanigans entirely. The file itself can stay in the codebase for now (it has logic worth preserving as a reference for the new compact components), but is dead code after this change. Deletion is a follow-up if no other surface picks it up.

**Possibly extracted (judgment call during implementation):**
- `frontend/src/components/Shenanigans/CompactFooter.tsx` — if trivial, inline in `Shenanigans.tsx`; if it grows past 5 lines, extract.
- `frontend/src/components/Shenanigans/LiveFeedPanel.tsx` — extracted from the current inline definition so it can take `defaultCollapsed`.

**No changes:**
- `LeaderboardRow` — reused as-is for #4+.
- `<GoldenName>` — reused as-is for podium names and "You" line when applicable.
- `<ActiveEffectsStrip />` — unchanged, just rendered in the same position.
- All spell-card components, cost math, cooldown logic, cast button.
- All backend code.

## 9. Out of scope for v1

- Animations on rank changes (player moving from #4 to #3 should ideally have a transition). Punted.
- Pull-to-refresh on mobile for HoF data. Existing query-invalidation cadence stays.
- "Show Charles in a separate House Status sticker above the podium" — Charles is just shown in the pinned You line per existing pattern.
- Deletion of `HallOfFame.tsx` and the wide `PodiumCard.tsx` if they end up fully unused. Spawn a separate cleanup task after this ships.
- "This Round" filtering on the leaderboard (already a known disabled feature — see project memory `shenanigans_known_issues`).
- A separate Hall of Fame page reachable via a top-level tab — explicitly rejected.

## 10. Accessibility

- HoF rail and mobile block both get `<section aria-label="Hall of Fame">`.
- Pinned "You" line gets `<div role="status" aria-label="Your rank: {rank}, {pp} PP burned, {casts} casts, {karma} karma">`.
- The (i) tooltip on the filter row is keyboard-accessible: `<button aria-label="Guardrails" aria-expanded={open}>`. Popover content is `role="dialog"` with focus trap when open via tap; on hover (desktop), no focus trap.
- The "See all" / "See top 10" buttons in the leaderboard are `<button>` elements with `aria-expanded` reflecting state.
- `position: sticky` on the HoF rail does not interfere with screen reader linear order — the element is still in document flow.
- `prefers-reduced-motion` is respected for any animations on the compact podium (the existing gold-shimmer rules already handle this).

## 11. Testing

Repo has no automated test suite. Verification is visual + interactive:

**Desktop (≥1024px viewport):**
- HoF rail is visible in the right column on initial load; spell grid takes the left column.
- Scroll down through the spell grid — HoF rail stays pinned to the top of the viewport.
- Live Feed panel sits below HoF rail in the sidebar; scrolls with the page (not sticky).
- "See all" button in HoF rail expands the leaderboard to show all entries inline.
- Pinned "You" line at the bottom of the HoF rail shows the current user's rank, PP burned, casts, outcomes, karma. For Charles, shows "HOUSE STATUS" pill in place of rank and `—` for PP burned.
- (i) tooltip on the filter row opens on click; shows three guardrail bullets.
- No "Your Ponzi Points" card at top of main column.
- No "Karma Received" card anywhere.
- No "Your Track Record" sidebar card.
- Footer is a single muted line, not two stacked disclaimers.

**Mobile (narrow viewport, below the sidebar-collapse breakpoint):**
- HoF mobile block is the first thing on the page.
- Mobile podium shows top 3 in compact 3-column grid; "You" line directly under the podium.
- "See top 10 ↓" button expands the list to ranks #4–#10.
- Below HoF: Active Effects (if any), filter tabs with (i) tooltip, spell grid, collapsed Live Feed header, footer.
- Live Feed "Latest casts ▾" expands inline on tap.

**Cross-cutting:**
- Cast a Whitelisted spell on yourself; verify the gold treatment renders in both HoF rail and mobile block (gold name, gold border on identicon).
- Verify a player with a 16+ character display name truncates with `title=` attribute on hover (compact podium) — full name should be visible somewhere via the leaderboard row #4+ rendering.
- With only 3 burners total (current state), the leaderboard list section is empty but the podium and "You" line still render. No visible dead space.
- Resize from desktop to mobile width — HoF rail disappears from sidebar, HoF mobile block appears at top. No duplicate render.
- `prefers-reduced-motion: reduce` — gold-shimmer animations halt on the compact podium.
