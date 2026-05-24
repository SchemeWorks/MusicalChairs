# Whitelisted VIP Treatment v1 — Design

**Date:** 2026-05-24
**Status:** Spec — ready for plan
**Context:** The Whitelisted shenanigan ("golden name" spell, internal ID `goldenName`, caster cost 420 PP on success / 42 PP on failure / 69 PP on backfire, lasts 72h common or 7d rare roll) currently flips the player's name to `mc-text-gold` with a faint 20px text-shadow. The treatment is visually weak — at `text-xs` sizes the glow barely registers, and on the rank-1 podium slot the gold name sits inside an already-gold medal cell, so the spell reads as no change at all. This spec covers a v1 redesign to make the spell feel like a real VC-bro flex worth paying for, plus a cast-moment fanfare.

## Goals

- Whitelisted name reads as unmistakably premium at every size, on every surface where a player's name appears.
- Casting Whitelisted feels like a moment, not a notification.

## Non-goals

- No changes to the spell's mechanics, cost, duration, or rare-roll odds.
- No changes to backend or canister code. This is a frontend-only cosmetic change.
- No audio.
- No rename of `goldenName` / `useIsGolden` / `useGoldenNameQuery` internals.

## 1. Shared `<GoldenName>` component

New file: `frontend/src/components/GoldenName.tsx`.

Pure presentation component. Takes `name: string` + `isGolden: boolean` (plus optional `className` for size/weight overrides from the parent). Renders the name, conditionally with VIP treatment.

When `isGolden` is true:
- **◆ prefix glyph** — a small gold diamond character before the name with a hair of right margin. Reads as VIP without colliding semantically with the purple `Gem` icon used for Diamond Tier (different color, different rendering — text glyph vs. lucide SVG).
- **Animated gradient sweep** — `background-image: linear-gradient(90deg, #FFD700 0%, #FFF4B0 50%, #FFD700 100%)` + `background-clip: text` + `color: transparent` + `background-size: 200% 100%` + 3s linear infinite animation moving `background-position` from `0% 0%` to `-200% 0%`. Holographic-credit-card sweep.
- **Pulsing glow** — `text-shadow` breathing between `0 0 16px rgba(255,215,0,0.4)` and `0 0 28px rgba(255,215,0,0.7)`, 2s ease-in-out infinite alternate.

When `isGolden` is false, renders the name with `mc-text-primary` (or whatever class the parent passes).

The component does NOT call `useIsGolden` itself — callers pass the boolean in. This keeps it cheap to use in lists where the parent has already resolved the query.

A small companion hook helper `useGoldenName(principal): { name: string; isGolden: boolean }` co-located in the same file wraps `useDisplayName` + `useIsGolden` for callers that want one call instead of two.

## 2. Apply `<GoldenName>` to every name surface

Currently `useDisplayName` is called by 7 components. All should render names through `<GoldenName>` so the treatment is uniform wherever a player's name appears.

| File | Notes |
|------|-------|
| `frontend/src/components/HallOfFame.tsx` | Both `PodiumSlot` and `LeaderboardRow`. Already use `useIsGolden`. Replace inline `mc-text-gold mc-glow-gold` classes with `<GoldenName>`. |
| `frontend/src/components/trollbox/rows/UserMessageRow.tsx` | Add `useIsGolden(item.author)`. Sender name uses `<GoldenName>`. |
| `frontend/src/components/trollbox/rows/SpellRow.tsx` | Both actor and target names use `<GoldenName>` (each with its own `useIsGolden`). |
| `frontend/src/components/trollbox/rows/RoundResultRow.tsx` | Round-winner name uses `<GoldenName>`. |
| `frontend/src/components/trollbox/rows/RankUpRow.tsx` | Promoted player name uses `<GoldenName>`. |
| `frontend/src/components/trollbox/rows/SignupRow.tsx` | New signup name uses `<GoldenName>`. |
| `frontend/src/components/trollbox/BlockedUsersMenu.tsx` | Out of scope — admin list, no flex needed. Keep as plain text. |

In `UserMessageRow` and `PodiumSlot`, the avatar circle's initial letter also flips to gold (`mc-text-gold` instead of the default zinc), and the avatar circle background gets a subtle gold tint + border when golden.

## 3. Chat bubble VIP treatment (UserMessageRow only)

When `useIsGolden(item.author)` is true in `UserMessageRow`:
- Row container adds: left border `2px solid var(--mc-gold)`, background `linear-gradient(90deg, rgba(255,215,0,0.08), transparent 60%)`.
- Avatar circle: `border border-[var(--mc-gold)]/60` and `bg-[var(--mc-gold)]/15` instead of `bg-zinc-700`.

The mention-highlight amber left border (`border-amber-400` when current user is `@`-mentioned) stays orthogonal — if both apply (golden author who mentions you), gold wins (gold border, not amber). Reads correctly because mentions of the current user are already visually distinct via the amber `@` text in the body.

## 4. Podium pedestal upgrade (HallOfFame)

In `PodiumSlot`, when `isGolden` is true:
- Pedestal `<div>` (the `${h} w-full mt-2 rounded-t-lg ...` block) swaps:
  - `bg` → `bg-[var(--mc-gold)]/15`
  - `border` → `border-[var(--mc-gold)]/50`
- Pedestal gains an animated gold shimmer: a pseudo-element with the same 3s gradient-sweep animation as the name, applied as `background-image` on a `before` overlay with `opacity-30`.
- Rank number text (`#1/#2/#3`) flips to `mc-text-gold`.

Standard (non-golden) rank styling is unchanged.

## 5. Cast-moment fanfare ("Series Gold" card)

New file: `frontend/src/components/WhitelistedFanfare.tsx`.

A full-viewport fixed-position overlay. Triggered from `Shenanigans.tsx` when a Whitelisted cast lands cleanly (currently the `case 10` arm at line ~636 that renders the small green text line).

**Behavior:**
- Mounts when the cast resolves successfully, unmounts after 5s OR on any click/keypress.
- Fades in over 200ms, fades out over 400ms.
- Backdrop: `bg-black/70` with a subtle gold radial gradient bloom from center.
- Card (centered, ~440px wide): dark `var(--mc-bg-card)` background, `border-2 border-[var(--mc-gold)]`, generous padding, rounded corners, drop-shadow gold glow.
  - Title: **"WHITELISTED"** — display font, ~48px, `mc-text-gold`, pulsing glow (reuse `.mc-name-vip` keyframes).
  - Subtitle: **"72 HOURS"** — display font, ~20px, gold, letter-spacing wide.
  - Kicker: *"You're on the list now."* — italic, ~14px, `mc-text-muted`.
- Confetti burst behind the card via `canvas-confetti`:
  - 150 particles, origin `{ x: 0.5, y: 0.5 }`, spread 90°, gravity 1, ticks 200.
  - Colors: `['#FFD700', '#FFF4B0', '#E8C547', '#FFFFFF']`.

**Duration source:**
The fanfare always shows "72 HOURS" for v1. A rare-roll variant (e.g. an extended duration with a louder fanfare) is out of scope here — current rare-roll wiring is unverified and not worth gating v1 on. Easy follow-up if/when the rare roll is confirmed live.

## 6. Naming

Use "Whitelisted" everywhere — fanfare card, chat ledger, docs, Shenanigans grid. No alternate tier labels. (Earlier drafts proposed "Series A" / "Series Gold" framing; cut to avoid confusion with the backer-tier Seed Round and because the rare-roll wiring isn't verified.)

## 7. CSS additions (`frontend/src/index.css`)

```css
@keyframes mc-gold-sweep {
  0%   { background-position: 0% 0%; }
  100% { background-position: -200% 0%; }
}

@keyframes mc-gold-pulse {
  0%   { text-shadow: 0 0 16px rgba(255, 215, 0, 0.4); }
  100% { text-shadow: 0 0 28px rgba(255, 215, 0, 0.7); }
}

.mc-name-vip {
  background-image: linear-gradient(90deg, #FFD700 0%, #FFF4B0 50%, #FFD700 100%);
  background-size: 200% 100%;
  background-clip: text;
  -webkit-background-clip: text;
  color: transparent;
  animation:
    mc-gold-sweep 3s linear infinite,
    mc-gold-pulse 2s ease-in-out infinite alternate;
}

.mc-pedestal-vip {
  position: relative;
  overflow: hidden;
}
.mc-pedestal-vip::before {
  content: '';
  position: absolute;
  inset: 0;
  background-image: linear-gradient(90deg, transparent 0%, rgba(255, 215, 0, 0.25) 50%, transparent 100%);
  background-size: 200% 100%;
  animation: mc-gold-sweep 3s linear infinite;
  pointer-events: none;
  opacity: 0.6;
}
```

The existing `.mc-glow-gold` class stays for non-name uses; the new `.mc-name-vip` is the canonical class for VIP names.

## 8. Dependencies

Add to `frontend/package.json`:
- `canvas-confetti` (runtime, ~4KB minified, MIT)
- `@types/canvas-confetti` (dev)

## 9. Out of scope for v1

Explicitly deferred — easy follow-ups if the v1 treatment still doesn't feel sufficient:
- Sparkle particle emitter on the running name (option C from brainstorm)
- "VERIFIED INSIDER" chip next to chat timestamps (option F)
- Header VIP chip with live countdown (option G)

## 10. Accessibility / motion

The gradient sweep + pulse animations should respect `prefers-reduced-motion`. Add a `@media (prefers-reduced-motion: reduce)` block that disables both keyframes and falls back to a static `color: var(--mc-gold)` + the existing 20px shadow. The ◆ prefix and gold color remain.

## 11. Files touched

**New:**
- `frontend/src/components/GoldenName.tsx`
- `frontend/src/components/WhitelistedFanfare.tsx`

**Modified:**
- `frontend/src/index.css` (CSS additions in §7)
- `frontend/src/components/HallOfFame.tsx` (use `<GoldenName>`, pedestal upgrade)
- `frontend/src/components/trollbox/rows/UserMessageRow.tsx` (use `<GoldenName>`, bubble treatment)
- `frontend/src/components/trollbox/rows/SpellRow.tsx`
- `frontend/src/components/trollbox/rows/RoundResultRow.tsx`
- `frontend/src/components/trollbox/rows/RankUpRow.tsx`
- `frontend/src/components/trollbox/rows/SignupRow.tsx`
- `frontend/src/components/Shenanigans.tsx` (fire fanfare on Whitelisted cast)
- `frontend/package.json` (add `canvas-confetti` + types)

## 12. Testing

This is a cosmetic frontend change with no math or canister implications, so the testing bar is "does it look and feel right in the running app." Plan should include:
- Verifying every name surface flips correctly (cast Whitelisted on a test principal, check podium / leaderboard / chat / spell-cast rows / signup row / round-result row).
- Verifying bubble treatment on UserMessageRow.
- Verifying podium pedestal shimmer.
- Verifying fanfare fires once per successful Whitelisted cast.
- Verifying `prefers-reduced-motion` falls back gracefully.
- Verifying `canvas-confetti` doesn't bloat the bundle materially (`npm run build` size diff).
