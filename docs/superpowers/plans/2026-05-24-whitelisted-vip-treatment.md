# Whitelisted VIP Treatment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the weak `mc-text-gold + faint shadow` treatment of the Whitelisted (golden-name) spell with an animated gold gradient + pulse + ◆ prefix applied to every player-name surface, plus a chat bubble treatment, a podium pedestal upgrade, and a confetti-driven cast-moment fanfare card.

**Architecture:** Pure frontend (React + Tailwind + Vite). Add a shared `<GoldenName>` presentation component plus matching CSS keyframes, then sweep every existing `useDisplayName` caller to render through it. Add a `<WhitelistedFanfare>` overlay component fired by the existing cast handler in `Shenanigans.tsx`. New runtime dep: `canvas-confetti` (~4KB, MIT).

**Tech Stack:** React 18, TypeScript, Tailwind, Vite, `canvas-confetti`, existing `useDisplayName` / `useIsGolden` hooks in `frontend/src/components/trollbox/useDisplayName.ts`.

**Spec:** [docs/superpowers/specs/2026-05-24-whitelisted-vip-treatment-design.md](../specs/2026-05-24-whitelisted-vip-treatment-design.md)

**Testing posture:** The repo has no test runner (`package.json` has only `dev` / `build` scripts — no vitest/jest). Per spec §12, verification is visual + `npm run build` (typecheck). Each task ends with a build check and a manual visual check in `npm run dev`.

**Working directory:** All paths relative to repo root `/Users/robertripley/coding/musicalchairs`.

---

## File Map

**New files:**
- `frontend/src/components/GoldenName.tsx` — shared presentation component
- `frontend/src/components/WhitelistedFanfare.tsx` — full-viewport cast-moment overlay

**Modified files:**
- `package.json` — add `canvas-confetti` runtime dep + `@types/canvas-confetti` dev dep
- `frontend/src/index.css` — add `@keyframes mc-gold-sweep`, `mc-gold-pulse`, `.mc-name-vip`, `.mc-pedestal-vip` + reduced-motion fallback
- `frontend/src/components/HallOfFame.tsx` — use `<GoldenName>` in `PodiumSlot` + `LeaderboardRow`, add pedestal upgrade
- `frontend/src/components/trollbox/rows/UserMessageRow.tsx` — use `<GoldenName>`, add bubble + avatar VIP treatment
- `frontend/src/components/trollbox/rows/SpellRow.tsx` — use `<GoldenName>` for caster + target
- `frontend/src/components/trollbox/rows/RoundResultRow.tsx` — use `<GoldenName>` for winner
- `frontend/src/components/trollbox/rows/RankUpRow.tsx` — use `<GoldenName>` for promoted player
- `frontend/src/components/trollbox/rows/SignupRow.tsx` — use `<GoldenName>` for new signup
- `frontend/src/components/Shenanigans.tsx` — fire `<WhitelistedFanfare>` on successful goldenName cast

---

## Task 1: Add `canvas-confetti` dependency

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install runtime and types**

Run:
```bash
npm install canvas-confetti
npm install -D @types/canvas-confetti
```

Expected: both packages added to `package.json`, `package-lock.json` updated, `node_modules/canvas-confetti/` exists.

- [ ] **Step 2: Verify TypeScript build still passes**

Run: `npm run build`
Expected: build succeeds (we haven't imported the lib yet — this just confirms nothing regressed).

- [ ] **Step 3: Commit**

```bash
git add package.json package-lock.json
git commit -m "deps: add canvas-confetti for Whitelisted fanfare"
```

---

## Task 2: Add VIP CSS keyframes and helper classes

**Files:**
- Modify: `frontend/src/index.css` (append near the existing `.mc-glow-gold` rule around line 475)

- [ ] **Step 1: Append the new keyframes and classes**

Append this block to the bottom of `frontend/src/index.css` (or co-located near the other `mc-*` gold rules — anywhere inside the same `@layer utilities` block as `.mc-text-gold`):

```css
@keyframes mc-gold-sweep {
  0%   { background-position: 0% 0%; }
  100% { background-position: -200% 0%; }
}

@keyframes mc-gold-pulse {
  0%   { text-shadow: 0 0 16px rgba(255, 215, 0, 0.4); }
  100% { text-shadow: 0 0 28px rgba(255, 215, 0, 0.7); }
}

@layer utilities {
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
}

@media (prefers-reduced-motion: reduce) {
  .mc-name-vip {
    background-image: none;
    color: var(--mc-gold);
    text-shadow: 0 0 20px var(--mc-glow-gold);
    animation: none;
  }
  .mc-pedestal-vip::before {
    animation: none;
    opacity: 0;
  }
}
```

Note: if `index.css` already has an `@layer utilities { ... }` block, place the two utility classes inside that existing block instead of opening a new one. Keep the `@keyframes` and the `@media` block outside any layer.

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: build succeeds, no Tailwind/PostCSS warnings about the new rules.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/index.css
git commit -m "ui: add VIP gold sweep + pulse + pedestal shimmer keyframes"
```

---

## Task 3: Create `<GoldenName>` component

**Files:**
- Create: `frontend/src/components/GoldenName.tsx`

- [ ] **Step 1: Create the component file**

Write `frontend/src/components/GoldenName.tsx`:

```tsx
import React from 'react';
import { Principal } from '@dfinity/principal';
import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';

interface GoldenNameProps {
  /** Resolved display name (caller decides how to get it). */
  name: string;
  /** Whether this player has an active Whitelisted (goldenName) spell. */
  isGolden: boolean;
  /**
   * Optional className for the wrapping `<span>`. Use to set size/weight
   * (e.g. `text-xs font-bold`). Color and decoration are handled internally:
   * non-golden falls back to `mc-text-primary`; golden uses `.mc-name-vip`.
   */
  className?: string;
}

/**
 * Renders a player display name with the Whitelisted VIP treatment when
 * `isGolden` is true: a ◆ prefix glyph plus the animated gold gradient sweep
 * + pulsing glow (see `.mc-name-vip` in index.css). Falls back to plain
 * `mc-text-primary` when not golden.
 *
 * The component is pure presentation — callers resolve `name` and `isGolden`
 * themselves. Use `<GoldenNameByPrincipal>` for the common case where you
 * already have a principal and want both lookups in one call.
 */
export default function GoldenName({ name, isGolden, className }: GoldenNameProps) {
  if (isGolden) {
    // The ◆ prefix lives in its own span with an explicit gold color, because
    // `.mc-name-vip` uses `color: transparent` + background-clip:text on the
    // name span — inheriting that would render the glyph invisible.
    return (
      <span className={`inline-flex items-baseline gap-1 ${className ?? ''}`.trim()}>
        <span aria-hidden="true" className="mc-text-gold">◆</span>
        <span className="mc-name-vip">{name}</span>
      </span>
    );
  }
  return (
    <span className={`mc-text-primary ${className ?? ''}`.trim()}>{name}</span>
  );
}

/**
 * Convenience wrapper that resolves both display name and golden status from
 * a principal. Use this in lists where each row has a principal and you want
 * the standard VIP treatment.
 */
export function GoldenNameByPrincipal({
  principal,
  className,
}: {
  principal: Principal | null;
  className?: string;
}) {
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  return <GoldenName name={name || '…'} isGolden={isGolden} className={className} />;
}
```

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: build succeeds, no TypeScript errors. (Nothing imports it yet — this just confirms the file compiles.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/GoldenName.tsx
git commit -m "ui: add GoldenName component for Whitelisted VIP name treatment"
```

---

## Task 4: Wire `<GoldenName>` into `HallOfFame` + add pedestal upgrade

**Files:**
- Modify: `frontend/src/components/HallOfFame.tsx`

This task updates both `PodiumSlot` (top-3 podium) and `LeaderboardRow` (rank list below it), and adds the pedestal shimmer for golden occupants.

- [ ] **Step 1: Import `GoldenName` at the top of the file**

In `frontend/src/components/HallOfFame.tsx`, add to the import block near line 6:

```tsx
import GoldenName from './GoldenName';
```

(Keep the existing `import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';` — `PodiumSlot` still calls both hooks directly because it also needs the avatar initial.)

- [ ] **Step 2: Update `PodiumSlot` — name + pedestal**

Replace the body of `PodiumSlot` (lines 19–59) with:

```tsx
function PodiumSlot({
  entry,
  rank,
}: {
  entry: HallOfFameEntry;
  rank: 1 | 2 | 3;
}) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);

  const heights = { 1: 'h-28', 2: 'h-20', 3: 'h-14' };
  const medals = {
    1: { bg: 'bg-[var(--mc-gold)]/20', border: 'border-[var(--mc-gold)]/40', text: 'mc-text-gold', glow: '0 0 16px rgba(255, 215, 0, 0.2)', icon: <Medal className="h-3.5 w-3.5 mc-text-gold" /> },
    2: { bg: 'bg-gray-400/10', border: 'border-gray-400/30', text: 'text-gray-300', glow: '0 0 12px rgba(192, 192, 192, 0.15)', icon: <Medal className="h-3 w-3 text-gray-300" /> },
    3: { bg: 'bg-amber-600/15', border: 'border-amber-600/30', text: 'text-amber-500', glow: '0 0 12px rgba(205, 127, 50, 0.15)', icon: <Medal className="h-3 w-3 text-amber-500" /> },
  };
  const m = medals[rank];
  const h = heights[rank];

  const displayName = name || '…';

  // When golden, the pedestal swaps to a gold-tinted background + border and
  // gets the animated shimmer overlay via `.mc-pedestal-vip`. Rank number
  // also flips gold. Otherwise standard rank-based styling.
  const pedestalBg = isGolden ? 'bg-[var(--mc-gold)]/15' : m.bg;
  const pedestalBorder = isGolden ? 'border-[var(--mc-gold)]/50' : m.border;
  const pedestalShimmer = isGolden ? 'mc-pedestal-vip' : '';
  const rankNumberClass = isGolden ? 'mc-text-gold' : m.text;

  return (
    <div className="flex flex-col items-center" style={{ minWidth: '90px' }}>
      <div
        className={`w-10 h-10 rounded-full ${isGolden ? 'bg-[var(--mc-gold)]/20 border-[var(--mc-gold)]/60' : `${m.bg} border-${m.border}`} border flex items-center justify-center mb-1.5 relative`}
        style={{ boxShadow: m.glow }}
      >
        <span className={`font-display text-sm ${isGolden ? 'mc-text-gold' : m.text}`}>
          {displayName.charAt(0).toUpperCase() || '?'}
        </span>
        <div className="absolute -top-1 -right-1 w-4 h-4 flex items-center justify-center">
          {m.icon}
        </div>
      </div>
      <GoldenName name={displayName} isGolden={isGolden} className="text-xs font-bold truncate max-w-[80px] text-center" />
      <span className="text-xs font-bold mc-text-purple">{(entry.ponziPointsBurned || 0).toLocaleString()}</span>
      <div className={`${h} w-full mt-2 rounded-t-lg ${pedestalBg} border-t border-x ${pedestalBorder} ${pedestalShimmer} flex items-start justify-center pt-2`}>
        <span className={`font-display text-sm ${rankNumberClass}`}>#{rank}</span>
      </div>
    </div>
  );
}
```

Key changes from the original:
- Dropped the inline `nameClass` variable — `<GoldenName>` owns that decision.
- Avatar circle bg/border swap when `isGolden`.
- New `pedestalBg` / `pedestalBorder` / `pedestalShimmer` / `rankNumberClass` derived from `isGolden`.
- Pedestal `<div>` adds `${pedestalShimmer}` className (renders the `::before` animated overlay when golden).

- [ ] **Step 3: Update `LeaderboardRow` — name only**

In `LeaderboardRow` (lines 81–140), replace the name-related block.

Find this section (around lines 103–110):

```tsx
  const displayName = name || '…';
  // Golden-name spell wins over the rank-based color, and over the "(you)" cyan.
  const nameClass = isGolden
    ? 'mc-text-gold mc-glow-gold'
    : isUser
      ? 'mc-text-cyan'
      : 'mc-text-primary';
```

Replace with:

```tsx
  const displayName = name || '…';
  // When not golden, the row name still respects the "(you)" cyan highlight.
  // When golden, <GoldenName> takes over the color/decoration regardless of
  // whether it's the current user.
  const fallbackClass = isUser ? 'mc-text-cyan' : 'mc-text-primary';
```

Then find the JSX that renders the name (around line 130):

```tsx
          <span className={`font-bold text-sm ml-2 ${nameClass}`}>
            {displayName}{isUser ? ' (you)' : ''}
          </span>
```

Replace with:

```tsx
          {isGolden ? (
            <GoldenName name={displayName} isGolden={true} className="font-bold text-sm ml-2" />
          ) : (
            <span className={`font-bold text-sm ml-2 ${fallbackClass}`}>
              {displayName}{isUser ? ' (you)' : ''}
            </span>
          )}
```

(The `(you)` suffix is dropped for golden rows on purpose — the gold treatment is already strong enough to identify your own row, and stacking `(you)` after a glowing gold name looks cluttered. If you disagree after seeing it, add it back inside the `<GoldenName>` `name` prop: `name={`${displayName}${isUser ? ' (you)' : ''}`}`.)

- [ ] **Step 4: Verify build**

Run: `npm run build`
Expected: build succeeds with no TypeScript errors.

- [ ] **Step 5: Visual check**

Run `npm run dev`, open the app, navigate to Shenanigans → Hall of Fame.
- Without a golden spell active: leaderboard looks identical to before.
- With a golden spell active on yourself (cast Whitelisted, or set the cache via React DevTools): your name on the podium AND in the leaderboard row gets the animated gold sweep + glow + ◆ prefix; if you're on the podium, the pedestal also shimmers gold.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/HallOfFame.tsx
git commit -m "ui: render Hall of Fame names through GoldenName, add VIP pedestal"
```

---

## Task 5: Wire `<GoldenName>` into `UserMessageRow` + add chat bubble treatment

**Files:**
- Modify: `frontend/src/components/trollbox/rows/UserMessageRow.tsx`

This is the surface that prompted the redesign — currently chat sender names are gray regardless of spell state.

- [ ] **Step 1: Add imports and `useIsGolden` call**

At the top of `frontend/src/components/trollbox/rows/UserMessageRow.tsx`, change the import line:

```tsx
import { useDisplayName } from '../useDisplayName';
```

to:

```tsx
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';
```

In the component body (after line 19), add:

```tsx
  const isGolden = useIsGolden(item.author);
```

- [ ] **Step 2: Apply bubble treatment to the row container**

Replace this line (line 27):

```tsx
    <div className={`relative flex gap-2 px-3 py-2 ${mentioned ? 'border-l-2 border-amber-400' : ''}`}>
```

with:

```tsx
    <div
      className={`relative flex gap-2 px-3 py-2 ${
        isGolden ? 'border-l-2 border-[var(--mc-gold)]' : mentioned ? 'border-l-2 border-amber-400' : ''
      }`}
      style={isGolden ? { backgroundImage: 'linear-gradient(90deg, rgba(255,215,0,0.08), transparent 60%)' } : undefined}
    >
```

(Gold border wins over the amber mention border when both apply, per spec §3.)

- [ ] **Step 3: Apply avatar VIP treatment**

Replace this line (line 28):

```tsx
      <div className="flex h-6 w-6 items-center justify-center rounded-full bg-zinc-700 text-xs font-medium text-zinc-200 shrink-0">
```

with:

```tsx
      <div
        className={`flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium shrink-0 ${
          isGolden
            ? 'bg-[var(--mc-gold)]/15 border border-[var(--mc-gold)]/60 mc-text-gold'
            : 'bg-zinc-700 text-zinc-200'
        }`}
      >
```

- [ ] **Step 4: Replace the sender name span with `<GoldenName>`**

Replace this line (line 33):

```tsx
          <span className="text-sm font-medium text-zinc-200 truncate">{authorName}</span>
```

with:

```tsx
          {isGolden ? (
            <GoldenName name={authorName} isGolden={true} className="text-sm font-medium truncate" />
          ) : (
            <span className="text-sm font-medium text-zinc-200 truncate">{authorName}</span>
          )}
```

(Conditional render keeps the non-golden path byte-identical to today — no regression risk on the common case.)

- [ ] **Step 5: Verify build**

Run: `npm run build`
Expected: build succeeds.

- [ ] **Step 6: Visual check**

Run `npm run dev`, open the Trollbox.
- Non-golden senders: messages look identical to before.
- Golden sender (cast Whitelisted on yourself, then send a chat message): row gets a gold left border + faint gold gradient fade across the row; avatar circle is gold-tinted with a gold initial; sender name has the animated gold sweep + ◆ prefix.
- Edge case: golden sender who mentions you (`@yourName`) — gold border wins, the amber `@` text in the body still highlights as before.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/trollbox/rows/UserMessageRow.tsx
git commit -m "ui: apply VIP gold treatment to whitelisted chat messages"
```

---

## Task 6: Wire `<GoldenName>` into `SpellRow`

**Files:**
- Modify: `frontend/src/components/trollbox/rows/SpellRow.tsx`

`SpellRow` renders the cast log lines (e.g. `✨ Cap Table Casualty cast Wealth Tax — landed clean.`). Both caster and target names should pick up the VIP treatment.

- [ ] **Step 1: Update imports and add `useIsGolden` calls**

Replace the import line:

```tsx
import { useDisplayName } from '../useDisplayName';
```

with:

```tsx
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';
```

In the component body, after the existing `useDisplayName` calls (lines 28 and 30), add:

```tsx
  const isCasterGolden = useIsGolden(cast.caster);
  const isTargetGolden = useIsGolden(target);
```

- [ ] **Step 2: Replace caster and target spans**

Find the caster span (line 41):

```tsx
      <span className={`${outcomeColor} font-medium`}>✨ {userName}</span>
```

Replace with:

```tsx
      <span className={`${outcomeColor} font-medium`}>
        ✨{' '}
        {isCasterGolden ? (
          <GoldenName name={userName} isGolden={true} className="font-medium" />
        ) : (
          userName
        )}
      </span>
```

Find the target span (line 47):

```tsx
          <span className="text-zinc-200 font-medium">{targetName}</span>
```

Replace with:

```tsx
          {isTargetGolden ? (
            <GoldenName name={targetName} isGolden={true} className="font-medium" />
          ) : (
            <span className="text-zinc-200 font-medium">{targetName}</span>
          )}
```

(For the caster, the outer span's `${outcomeColor}` still tints the `✨` glyph — only the name itself flips to gold when golden, which is the right priority: the spell outcome color is for the *act of casting*, the gold treatment is for the *player's identity*.)

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: build succeeds.

- [ ] **Step 4: Visual check**

Run `npm run dev`, open Trollbox. Find a spell-cast row where the caster or target is golden (or trigger one by casting from a golden account). The name(s) should render with the gold sweep + ◆ prefix; the rest of the row (emoji, "cast", spell name, outcome word) stays as today.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/trollbox/rows/SpellRow.tsx
git commit -m "ui: render SpellRow caster/target names through GoldenName"
```

---

## Task 7: Wire `<GoldenName>` into remaining trollbox rows

**Files:**
- Modify: `frontend/src/components/trollbox/rows/RoundResultRow.tsx`
- Modify: `frontend/src/components/trollbox/rows/RankUpRow.tsx`
- Modify: `frontend/src/components/trollbox/rows/SignupRow.tsx`

Three near-identical changes, batched into one task because each file is ~17 lines and the pattern is the same.

- [ ] **Step 1: Update `RoundResultRow.tsx`**

Replace the whole file with:

```tsx
import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';

export default function RoundResultRow({ item }: { item: ChatItem }) {
  const winner = 'roundResult' in item.kind ? item.kind.roundResult.winner : null;
  const name = useDisplayName(winner);
  const isGolden = useIsGolden(winner);
  if (!('roundResult' in item.kind)) return null;
  const { gameId, winnerPpUnits } = item.kind.roundResult;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  const ppWon = Number(winnerPpUnits) / 100_000_000;
  return (
    <div className="px-3 py-1 text-xs text-amber-300">
      🎰 Round #{gameId.toString()} —{' '}
      {isGolden ? (
        <GoldenName name={name} isGolden={true} className="font-medium" />
      ) : (
        <span className="text-zinc-100 font-medium">{name}</span>
      )}
      {' '}took the chair. Won: {ppWon.toFixed(0)} PP.
    </div>
  );
}
```

- [ ] **Step 2: Update `RankUpRow.tsx`**

Replace the whole file with:

```tsx
import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';

export default function RankUpRow({ item }: { item: ChatItem }) {
  const user = 'rankUp' in item.kind ? item.kind.rankUp.user : null;
  const name = useDisplayName(user);
  const isGolden = useIsGolden(user);
  if (!('rankUp' in item.kind)) return null;
  const { newRank } = item.kind.rankUp;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-emerald-300">
      📈{' '}
      {isGolden ? (
        <GoldenName name={name} isGolden={true} className="font-medium" />
      ) : (
        <span className="text-zinc-100 font-medium">{name}</span>
      )}
      {' '}promoted to <span className="font-semibold">{newRank}</span>.
    </div>
  );
}
```

- [ ] **Step 3: Update `SignupRow.tsx`**

Replace the whole file with:

```tsx
import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';

export default function SignupRow({ item }: { item: ChatItem }) {
  const newUser = 'signup' in item.kind ? item.kind.signup.newUser : null;
  const name = useDisplayName(newUser);
  const isGolden = useIsGolden(newUser);
  if (!('signup' in item.kind)) return null;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-zinc-400">
      🆕{' '}
      {isGolden ? (
        <GoldenName name={name} isGolden={true} className="font-medium" />
      ) : (
        <span className="text-zinc-200 font-medium">{name}</span>
      )}
      {' '}just signed the dotted line.
    </div>
  );
}
```

- [ ] **Step 4: Verify build**

Run: `npm run build`
Expected: build succeeds.

- [ ] **Step 5: Visual check**

Run `npm run dev`, scroll the Trollbox. Any round-result, rank-up, or signup row referencing a golden player should now show the gold sweep + ◆ prefix on the name.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/trollbox/rows/RoundResultRow.tsx frontend/src/components/trollbox/rows/RankUpRow.tsx frontend/src/components/trollbox/rows/SignupRow.tsx
git commit -m "ui: render remaining trollbox row names through GoldenName"
```

---

## Task 8: Create `<WhitelistedFanfare>` overlay component

**Files:**
- Create: `frontend/src/components/WhitelistedFanfare.tsx`

- [ ] **Step 1: Write the fanfare component**

Create `frontend/src/components/WhitelistedFanfare.tsx`:

```tsx
import React from 'react';
import confetti from 'canvas-confetti';

interface WhitelistedFanfareProps {
  /** When true, the overlay is mounted and visible. */
  open: boolean;
  /** Called when the overlay dismisses (auto after 5s or user click/key). */
  onClose: () => void;
}

const DISMISS_MS = 5000;

const GOLD_COLORS = ['#FFD700', '#FFF4B0', '#E8C547', '#FFFFFF'];

/**
 * Full-viewport cast-moment overlay for a successful Whitelisted cast.
 * Fires a gold confetti burst from screen center on mount, displays a gold-
 * bordered card with the spell name + 72-hour duration, and auto-dismisses
 * after 5s (or any click / keypress).
 */
export default function WhitelistedFanfare({ open, onClose }: WhitelistedFanfareProps) {
  // Fire confetti exactly once per open transition.
  React.useEffect(() => {
    if (!open) return;
    confetti({
      particleCount: 150,
      spread: 90,
      origin: { x: 0.5, y: 0.5 },
      colors: GOLD_COLORS,
      gravity: 1,
      ticks: 200,
      zIndex: 9999,
    });
  }, [open]);

  // Auto-dismiss timer + click/key listeners.
  React.useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(onClose, DISMISS_MS);
    const handleKey = () => onClose();
    window.addEventListener('keydown', handleKey, { once: true });
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('keydown', handleKey);
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-[9998] flex items-center justify-center cursor-pointer"
      style={{
        background:
          'radial-gradient(circle at center, rgba(255,215,0,0.15) 0%, rgba(0,0,0,0.7) 60%)',
        animation: 'mc-fanfare-fade-in 200ms ease-out',
      }}
      role="dialog"
      aria-label="Whitelisted spell cast successfully"
    >
      <div
        className="mc-card-elevated border-2 border-[var(--mc-gold)] rounded-2xl px-12 py-10 text-center max-w-md mx-4"
        style={{ boxShadow: '0 0 60px rgba(255, 215, 0, 0.4)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mc-name-vip font-display text-5xl mb-3 tracking-wide">
          WHITELISTED
        </div>
        <div className="mc-text-gold font-display text-xl mb-4 tracking-widest">
          72 HOURS
        </div>
        <div className="italic text-sm mc-text-muted">
          You're on the list now.
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add the fade-in keyframe to `index.css`**

Append to `frontend/src/index.css` (anywhere near the other `mc-*` keyframes you added in Task 2):

```css
@keyframes mc-fanfare-fade-in {
  from { opacity: 0; }
  to   { opacity: 1; }
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: build succeeds. (Nothing imports `WhitelistedFanfare` yet — wired up in Task 9.)

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/WhitelistedFanfare.tsx frontend/src/index.css
git commit -m "ui: add WhitelistedFanfare cast-moment overlay"
```

---

## Task 9: Fire the fanfare from `Shenanigans.tsx`

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx`

The existing cast handler `handleConfirmCast` (around line 410) sets `outcomeToast` after a successful cast. For a successful Whitelisted (goldenName) cast we want to skip that toast and show the fanfare instead, following the same pattern the code already uses for `renameSpell` success.

The goldenName shenanigan ID is **10** (per the `SHEN_VARIANT_ORDER` array in `SpellRow.tsx` and the `case 10` arm in `Shenanigans.tsx` around line 636).

- [ ] **Step 1: Add the import**

Near the top of `frontend/src/components/Shenanigans.tsx`, add:

```tsx
import WhitelistedFanfare from './WhitelistedFanfare';
```

- [ ] **Step 2: Add the fanfare state**

Find the `outcomeToast` state declaration (around line 270):

```tsx
  const [outcomeToast, setOutcomeToast] = useState<{
```

Just before it, add:

```tsx
  const [whitelistedFanfareOpen, setWhitelistedFanfareOpen] = useState(false);
```

- [ ] **Step 3: Branch on goldenName-success in `handleConfirmCast`**

In `handleConfirmCast` (around lines 423–452), find this block:

```tsx
      setTimeout(() => {
        const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
        const targetPrincipalText = detail.affectedTarget && detail.affectedTarget.length > 0
          ? detail.affectedTarget[0]?.toText() ?? null
          : null;
        if (isRenameSuccess && targetPrincipalText) {
          // Skip the success toast — the rename modal IS the success
          // affirmation, and otherwise the toast would sit hidden behind
          // the rename modal's backdrop.
          setRenamePrompt({ targetPrincipal: targetPrincipalText });
        } else {
```

Replace it with:

```tsx
      setTimeout(() => {
        const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
        const isWhitelistedSuccess = outcome === 'success' && selectedShenanigan.id === 10 /* goldenName */;
        const targetPrincipalText = detail.affectedTarget && detail.affectedTarget.length > 0
          ? detail.affectedTarget[0]?.toText() ?? null
          : null;
        if (isRenameSuccess && targetPrincipalText) {
          // Skip the success toast — the rename modal IS the success
          // affirmation, and otherwise the toast would sit hidden behind
          // the rename modal's backdrop.
          setRenamePrompt({ targetPrincipal: targetPrincipalText });
        } else if (isWhitelistedSuccess) {
          // Skip the success toast — the fanfare card IS the affirmation,
          // and stacking the small green toast under a confetti overlay
          // looks ridiculous. Failure / backfire still go through the
          // normal toast below.
          setWhitelistedFanfareOpen(true);
        } else {
```

(The closing `} else {` at the bottom of the original block now becomes the third branch — already in place. Don't change the existing toast `setOutcomeToast({ ... })` call inside the final else.)

- [ ] **Step 4: Render the fanfare in the JSX**

Find the existing toast render block near line 776:

```tsx
      {outcomeToast && (
```

Just before it (or just after, the order doesn't matter — they don't co-occur for goldenName-success), add:

```tsx
      <WhitelistedFanfare
        open={whitelistedFanfareOpen}
        onClose={() => setWhitelistedFanfareOpen(false)}
      />
```

- [ ] **Step 5: Verify build**

Run: `npm run build`
Expected: build succeeds, no TypeScript errors.

- [ ] **Step 6: Visual check (the payoff)**

Run `npm run dev`. From an account with enough PP, cast Whitelisted (Shenanigans → find Whitelisted card → Cast).
- **On success:** gold confetti burst from screen center; centered gold-bordered card with "WHITELISTED" big + "72 HOURS" + "You're on the list now."; auto-dismisses after 5s or on any click/keypress. The usual small green outcome toast should NOT appear.
- **On failure or backfire:** existing toast behavior is unchanged (small toast with fail/backfire flavor, no fanfare).
- Verify confetti doesn't bleed past the dismiss (canvas should clean itself up after `ticks: 200`).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat: fire WhitelistedFanfare on successful goldenName cast"
```

---

## Task 10: Final end-to-end visual sweep

**Files:** None modified — this is a verification pass.

- [ ] **Step 1: Full build + bundle size sanity check**

Run: `npm run build`
Note the dist bundle size. Compare against `git stash && npm run build` on `main` if you want a precise delta. `canvas-confetti` is ~4KB minified gzipped, so the JS bundle should grow by single-digit KB.

- [ ] **Step 2: Run the dev server and walk the surfaces**

Run: `npm run dev`. With a golden test account, verify each surface:

| Surface | Expected when golden |
|---------|---------------------|
| Hall of Fame podium (rank 1–3) | Name has gold sweep + ◆; pedestal shimmers gold; rank number gold |
| Hall of Fame leaderboard row | Name has gold sweep + ◆ (no `(you)` suffix) |
| Trollbox `UserMessageRow` | Row has gold left-border + faint gold fade bg; avatar gold; sender name has sweep + ◆ |
| Trollbox `SpellRow` (as caster) | Caster name has sweep + ◆; outcome color on `✨` preserved |
| Trollbox `SpellRow` (as target) | Target name has sweep + ◆ |
| Trollbox `RoundResultRow` (as winner) | Winner name has sweep + ◆ |
| Trollbox `RankUpRow` | Promoted name has sweep + ◆ |
| Trollbox `SignupRow` | New-signup name has sweep + ◆ |
| Cast moment (cast Whitelisted on success) | Full confetti + fanfare card, no green toast |

- [ ] **Step 3: Reduced-motion check**

In the OS / browser, enable "Reduce motion" (macOS: System Settings → Accessibility → Display → Reduce motion). Reload the app and confirm:
- Names render in static gold with the existing 20px shadow (no sweep, no pulse).
- Pedestal shimmer is gone.
- Fanfare confetti still fires (canvas-confetti respects its own settings; we don't gate on `prefers-reduced-motion` because the burst is intentional and one-shot — if this feels wrong on the device, file a follow-up).

- [ ] **Step 4: Non-golden regression check**

With a non-golden account (or after the spell expires), confirm:
- Hall of Fame, leaderboard, and all trollbox rows look identical to pre-change.
- Casting any non-Whitelisted spell still shows the normal small toast.

- [ ] **Step 5: Final commit (if any cleanup needed)**

If steps 1–4 found nothing to fix, no commit needed. Otherwise commit fixes with a clear message and re-run step 2.

---

## Self-Review Notes

**Spec coverage check:**

| Spec section | Task(s) |
|--------------|---------|
| §1 `<GoldenName>` component | Task 3 |
| §2 Apply to every name surface | Tasks 4, 5, 6, 7 (HallOfFame + 4 trollbox rows) |
| §3 Chat bubble VIP treatment | Task 5 |
| §4 Podium pedestal upgrade | Task 4 |
| §5 Cast-moment fanfare | Tasks 8, 9 |
| §6 Naming = "Whitelisted" only | Task 8 (card copy) |
| §7 CSS additions | Tasks 2, 8 |
| §8 Dependencies | Task 1 |
| §9 Out of scope | n/a (correctly omitted) |
| §10 Reduced motion | Task 2 (CSS), Task 10 (verify) |
| §11 Files touched | All tasks |
| §12 Testing posture | Task 10 |

**Coverage gaps:** None.

**Note on the `<GoldenName>` non-golden path:** Tasks 5–7 each gate `<GoldenName>` behind `isGolden ? <GoldenName/> : <span/>` rather than always rendering `<GoldenName>` and letting it choose internally. This is intentional — the non-golden branch needs to preserve per-row-specific color classes (`text-zinc-100`, `text-zinc-200`, `mc-text-cyan` for "you"), which `<GoldenName>`'s `mc-text-primary` fallback doesn't capture. The component still owns the golden path's styling uniformly. If a future refactor consolidates name colors, the gate can collapse.
