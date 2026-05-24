# Hall of Fame Podium Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the cramped bar-chart podium in Hall of Fame with three large cards filling the wide card horizontally, so player names (especially Whitelisted ones) render at `text-xl` with no truncation.

**Architecture:** Lift the inline `PodiumSlot` + `Podium` definitions out of `HallOfFame.tsx` into a new `frontend/src/components/hall-of-fame/` folder. Replace the bar-chart slot with a card-based component that layers two orthogonal visual signals (rank tinting + golden-status supersession). Add an empty-state placeholder under the podium for when the list of #4+ is empty.

**Tech Stack:** React 18, TypeScript, Tailwind, existing `<GoldenName>`, existing `useDisplayName`/`useIsGolden`, existing minidenticon-based `<Identicon>` pattern.

**Spec:** [docs/superpowers/specs/2026-05-24-hall-of-fame-podium-redesign.md](../specs/2026-05-24-hall-of-fame-podium-redesign.md)

**Testing posture:** No test runner in repo. Verification is `npm run build` (tsc + vite) plus visual check in `npm run dev`. Each task ends with a build check.

**Working directory:** All paths relative to repo root `/Users/robertripley/coding/musicalchairs`.

---

## File Map

**New files:**
- `frontend/src/components/hall-of-fame/PodiumCard.tsx` — the per-rank card (avatar + name + PP + status pill + rank-and-golden styling)
- `frontend/src/components/hall-of-fame/Podium.tsx` — the 3-cell grid wrapping the cards in desktop order `[#2, #1, #3]` (stacks on mobile)

**Modified:**
- `frontend/src/components/HallOfFame.tsx` — remove inline `PodiumSlot` + `Podium`, import from new folder, add empty-list placeholder below the podium

**No changes:**
- `LeaderboardRow` (handles #4+, unchanged)
- `<GoldenName>` (already forwards truncation classes; we just don't pass any)
- Backend (Option B from spec — pill shows `◆ WHITELISTED` with no duration; backend change is a follow-up)

---

## Task 1: Scaffold `PodiumCard` component

**Files:**
- Create: `frontend/src/components/hall-of-fame/PodiumCard.tsx`

This is the heart of the redesign. One component handles every state: rank #1/2/3, golden vs non-golden, and the layered styling table from spec §1.

- [ ] **Step 1: Create the directory and file**

Run:
```bash
mkdir -p frontend/src/components/hall-of-fame
```

Then create `frontend/src/components/hall-of-fame/PodiumCard.tsx`:

```tsx
import React from 'react';
import { Medal } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { minidenticon } from 'minidenticons';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';

export interface HallOfFameEntry {
  rank: number;
  ponziPointsBurned?: number;
  principal: string;
}

interface PodiumCardProps {
  entry: HallOfFameEntry;
  rank: 1 | 2 | 3;
}

// Resolved styling for a (rank, isGolden) pair. Golden status supersedes the
// rank tint on border / background / glow but does NOT remove the #1 scale
// lift — both signals coexist. See the spec table for the full matrix.
function resolveCardStyle(rank: 1 | 2 | 3, isGolden: boolean) {
  const isFirst = rank === 1;
  if (isGolden) {
    return {
      borderClass: 'border-4 border-[var(--mc-gold)]',
      bgClass: 'bg-[var(--mc-gold)]/15',
      shimmerClass: 'mc-pedestal-vip',
      boxShadow: isFirst
        ? '0 0 40px rgba(255, 215, 0, 0.45)'
        : '0 0 32px rgba(255, 215, 0, 0.35)',
      scaleClass: isFirst ? 'scale-105' : 'scale-100',
    };
  }
  if (isFirst) {
    return {
      borderClass: 'border-2 border-[var(--mc-gold)]/40',
      bgClass: 'bg-[var(--mc-gold)]/[0.08]',
      shimmerClass: '',
      boxShadow: '0 0 24px rgba(255, 215, 0, 0.18)',
      scaleClass: 'scale-105',
    };
  }
  if (rank === 2) {
    return {
      borderClass: 'border-2 border-gray-400/40',
      bgClass: 'bg-gray-400/5',
      shimmerClass: '',
      boxShadow: '0 0 16px rgba(192, 192, 192, 0.12)',
      scaleClass: 'scale-100',
    };
  }
  // rank 3
  return {
    borderClass: 'border-2 border-amber-600/40',
    bgClass: 'bg-amber-600/5',
    shimmerClass: '',
    boxShadow: '0 0 16px rgba(205, 127, 50, 0.12)',
    scaleClass: 'scale-100',
  };
}

// Rank chip color stays tied to rank regardless of golden status — so a
// gold-bordered #2 card still labels its occupant as silver at the corner.
function rankChipStyle(rank: 1 | 2 | 3) {
  if (rank === 1) return { text: 'mc-text-gold', iconClass: 'mc-text-gold' };
  if (rank === 2) return { text: 'text-gray-300', iconClass: 'text-gray-300' };
  return { text: 'text-amber-500', iconClass: 'text-amber-500' };
}

export default function PodiumCard({ entry, rank }: PodiumCardProps) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  const displayName = name || '…';

  const card = resolveCardStyle(rank, isGolden);
  const chip = rankChipStyle(rank);
  const ppBurned = (entry.ponziPointsBurned || 0).toLocaleString();

  // Per-card identicon. Cached by principal text via useMemo (cheap regen
  // would re-run minidenticon on every parent re-render otherwise).
  const identiconUri = React.useMemo(() => {
    const svg = minidenticon(entry.principal, 60, 50);
    return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
  }, [entry.principal]);

  const ariaLabel = isGolden
    ? `Rank #${rank}: ${displayName}, ${ppBurned} PP burned, Whitelisted`
    : `Rank #${rank}: ${displayName}, ${ppBurned} PP burned`;

  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className={`relative rounded-2xl p-5 transition-transform ${card.borderClass} ${card.bgClass} ${card.shimmerClass} ${card.scaleClass} flex flex-col items-center text-center`}
      style={{ boxShadow: card.boxShadow }}
    >
      {/* Rank chip — top right corner */}
      <div className={`absolute top-2 right-3 flex items-center gap-1 text-xs font-bold ${chip.text}`}>
        <Medal className={`h-3.5 w-3.5 ${chip.iconClass}`} />
        <span>#{rank}</span>
      </div>

      {/* Identicon (gold-ringed when golden) */}
      {isGolden ? (
        <div
          className="rounded-full p-[2px] bg-[var(--mc-gold)]/40 mb-3"
          style={{ boxShadow: '0 0 12px rgba(255, 215, 0, 0.5)' }}
        >
          <img src={identiconUri} alt="" className="h-16 w-16 rounded-full bg-zinc-800" />
        </div>
      ) : (
        <img src={identiconUri} alt="" className="h-16 w-16 rounded-full bg-zinc-800 mb-3" />
      )}

      {/* Player name — text-xl, never truncate, wrap to max 2 lines */}
      <div className="text-xl font-bold leading-tight line-clamp-2 break-words w-full mb-2">
        <GoldenName name={displayName} isGolden={isGolden} />
      </div>

      {/* PP burned */}
      <div className="flex items-baseline gap-1">
        <span className="text-2xl font-bold mc-text-purple">{ppBurned}</span>
        <span className="text-xs mc-text-muted">PP</span>
      </div>

      {/* Status pill — only when golden. v1 has no duration; backend follow-up
          will promote ActiveSpellEffects.golden from boolean to a record so we
          can show "~Xd left". */}
      {isGolden && (
        <div className="mt-3 inline-flex items-center gap-1 px-2 py-1 rounded-full bg-[var(--mc-gold)]/15 border border-[var(--mc-gold)]/40 mc-text-gold text-xs font-bold tracking-wide">
          <span aria-hidden="true">◆</span>
          <span>WHITELISTED</span>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: clean pass. Nothing imports `PodiumCard` yet — this just confirms the file compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/hall-of-fame/PodiumCard.tsx
git commit -m "$(cat <<'EOF'
ui: add PodiumCard component for Hall of Fame redesign

Standalone per-rank card replacing the cramped bar-chart slot. Layered
styling: rank tint on the border/glow by default, supersededed by gold
treatment when the occupant is whitelisted; #1 also gets a scale-105
lift regardless of golden status, so the two signals coexist cleanly.

Renders identicon (gold-ringed if golden), text-xl name through
<GoldenName> (no truncation, 2-line clamp), large PP burned figure,
and a "◆ WHITELISTED" pill below when golden. v1 pill has no
duration — pending backend exposing the goldenName expiry timestamp.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Scaffold `Podium` grid layout

**Files:**
- Create: `frontend/src/components/hall-of-fame/Podium.tsx`

This is the 3-cell grid that arranges the cards. Desktop order `[#2, #1, #3]`, mobile stacks as `[#1, #2, #3]` top-down.

- [ ] **Step 1: Create the file**

Create `frontend/src/components/hall-of-fame/Podium.tsx`:

```tsx
import React from 'react';
import PodiumCard, { HallOfFameEntry } from './PodiumCard';

interface PodiumProps {
  entries: HallOfFameEntry[];
}

/**
 * Top-3 podium for the Hall of Fame Diamond Tier. Three cards filling the
 * parent card's width via CSS grid; on narrow viewports the cards stack
 * vertically in natural rank order (#1, #2, #3 top-down).
 */
export default function Podium({ entries }: PodiumProps) {
  const top3 = entries.slice(0, 3);
  if (top3.length === 0) return null;

  // Desktop horizontal order: [#2, #1, #3] — classic podium left-center-right.
  // We reorder the array for desktop rendering, then use Tailwind `order-*`
  // utilities to flip back to natural rank order when stacked vertically.
  // Renders fewer cards when there are <3 burners.
  const desktopOrder: Array<{ entry: HallOfFameEntry; rank: 1 | 2 | 3; mobileOrderClass: string }> = [];
  if (top3[1]) desktopOrder.push({ entry: top3[1], rank: 2, mobileOrderClass: 'order-2 sm:order-none' });
  if (top3[0]) desktopOrder.push({ entry: top3[0], rank: 1, mobileOrderClass: 'order-1 sm:order-none' });
  if (top3[2]) desktopOrder.push({ entry: top3[2], rank: 3, mobileOrderClass: 'order-3 sm:order-none' });

  return (
    <div
      className={`grid grid-cols-1 sm:grid-cols-${Math.min(3, top3.length)} gap-3 sm:gap-4 mb-6`}
    >
      {desktopOrder.map(({ entry, rank, mobileOrderClass }) => (
        <div key={`podium-${rank}`} className={mobileOrderClass}>
          <PodiumCard entry={entry} rank={rank} />
        </div>
      ))}
    </div>
  );
}
```

Note on the `grid-cols-${...}` template: Tailwind's JIT mode needs class names to appear literally in source code to be detected. The expression `grid-cols-${Math.min(3, top3.length)}` will NOT be picked up by the scanner. Fix by using static branching instead — replace the className expression with:

```tsx
  const gridColsClass = top3.length === 1 ? 'sm:grid-cols-1' : top3.length === 2 ? 'sm:grid-cols-2' : 'sm:grid-cols-3';
```

then use `className={`grid grid-cols-1 ${gridColsClass} gap-3 sm:gap-4 mb-6`}`. Apply this fix in the file before saving.

- [ ] **Step 2: Apply the gridColsClass fix**

Final `Podium.tsx` should be:

```tsx
import React from 'react';
import PodiumCard, { HallOfFameEntry } from './PodiumCard';

interface PodiumProps {
  entries: HallOfFameEntry[];
}

/**
 * Top-3 podium for the Hall of Fame Diamond Tier. Three cards filling the
 * parent card's width via CSS grid; on narrow viewports the cards stack
 * vertically in natural rank order (#1, #2, #3 top-down).
 */
export default function Podium({ entries }: PodiumProps) {
  const top3 = entries.slice(0, 3);
  if (top3.length === 0) return null;

  // Desktop horizontal order: [#2, #1, #3] — classic podium left-center-right.
  // Mobile stacks vertically in natural rank order via `order-*` utilities.
  const desktopOrder: Array<{ entry: HallOfFameEntry; rank: 1 | 2 | 3; mobileOrderClass: string }> = [];
  if (top3[1]) desktopOrder.push({ entry: top3[1], rank: 2, mobileOrderClass: 'order-2 sm:order-none' });
  if (top3[0]) desktopOrder.push({ entry: top3[0], rank: 1, mobileOrderClass: 'order-1 sm:order-none' });
  if (top3[2]) desktopOrder.push({ entry: top3[2], rank: 3, mobileOrderClass: 'order-3 sm:order-none' });

  // Tailwind JIT needs class names literal in source — pick the right
  // grid-cols-N from a static set rather than templating with a number.
  const gridColsClass =
    top3.length === 1 ? 'sm:grid-cols-1'
    : top3.length === 2 ? 'sm:grid-cols-2'
    : 'sm:grid-cols-3';

  return (
    <div className={`grid grid-cols-1 ${gridColsClass} gap-3 sm:gap-4 mb-6`}>
      {desktopOrder.map(({ entry, rank, mobileOrderClass }) => (
        <div key={`podium-${rank}`} className={mobileOrderClass}>
          <PodiumCard entry={entry} rank={rank} />
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: clean pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/hall-of-fame/Podium.tsx
git commit -m "$(cat <<'EOF'
ui: add Podium grid wrapper for Hall of Fame redesign

3-cell CSS grid wrapping PodiumCard components in classic podium order
[#2, #1, #3] on desktop. Stacks vertically on narrow viewports with
natural rank order (#1, #2, #3 top-down) via Tailwind order-* utilities.

Grid-cols-N picked from a static branch so Tailwind JIT can detect the
classes — templating with a numeric expression would silently drop them.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Wire new components into `HallOfFame.tsx` + add empty-list placeholder

**Files:**
- Modify: `frontend/src/components/HallOfFame.tsx`

This task removes the old inline `PodiumSlot` + `Podium` definitions, imports the new components, and adds the empty-state line under the podium for the <=3-burners case.

- [ ] **Step 1: Read the current file to confirm structure**

Read `frontend/src/components/HallOfFame.tsx` end-to-end. Confirm:
- `PodiumSlot` function exists (was the bar-chart slot)
- `Podium` function exists (was the bar-chart wrapper)
- `LeaderboardRow` function exists (handles #4+)
- Default export `HallOfFame` renders the page

- [ ] **Step 2: Update imports at the top**

Replace the existing top-of-file imports block with the version below. Keep ALL existing imports for hooks/utils that the remaining `LeaderboardRow` and `HallOfFame` still use. Specifically:
- Remove `Medal` from the `lucide-react` import IF no other code in this file uses it (LeaderboardRow uses it for the row icon — keep `Medal` if it's still referenced).
- Remove the `import GoldenName from './GoldenName';` line — only LeaderboardRow now needs GoldenName, but it ALREADY imports it. Verify before removing.
- Remove `useDisplayName` and `useIsGolden` from the trollbox import if `PodiumSlot` was the only consumer in this file. Keep them if LeaderboardRow still uses them.
- Add `import Podium from './hall-of-fame/Podium';`
- Add `import type { HallOfFameEntry } from './hall-of-fame/PodiumCard';`

The cleanest approach: search the file for each symbol AFTER removing `PodiumSlot` and `Podium` definitions and only then prune unused imports.

- [ ] **Step 3: Delete the inline `PodiumSlot` function**

Find and remove the entire `PodiumSlot` function. In the current file it starts with:

```tsx
function PodiumSlot({
  entry,
  rank,
}: {
  entry: HallOfFameEntry;
  rank: 1 | 2 | 3;
}) {
```

and ends at the closing `}` of that function (look for the `</div>\n  );\n}` followed by a blank line).

- [ ] **Step 4: Delete the inline `Podium` function**

Find and remove the entire `Podium` function. It starts with:

```tsx
function Podium({ entries }: { entries: HallOfFameEntry[] }) {
```

and ends at its closing `}`.

- [ ] **Step 5: Update the local `HallOfFameEntry` interface**

The file currently defines `HallOfFameEntry` inline. Either:
- Delete the local definition (since `PodiumCard.tsx` exports the canonical type) and import it from `'./hall-of-fame/PodiumCard'`.
- OR keep both — they're structurally identical, TypeScript won't complain.

Prefer the first: single source of truth. Replace:

```tsx
interface HallOfFameEntry {
  rank: number;
  ponziPointsBurned?: number;
  principal: string;
}
```

with:

```tsx
import type { HallOfFameEntry } from './hall-of-fame/PodiumCard';
```

(Move this import to the top of the file with the other imports.)

- [ ] **Step 6: Add the empty-list placeholder**

Find the section that renders the LeaderboardRow list. It looks like:

```tsx
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {burnersData && burnersData.length > 0
            ? burnersData
                .slice(burnersData.length >= 2 ? Math.min(3, burnersData.length) : 0)
                .map(entry => (
                  <LeaderboardRow
                    key={`b-${entry.rank}`}
                    entry={entry}
                    isUser={entry.principal === userPrincipal}
                  />
                ))
            : null}
        </div>
```

Wrap it so the empty case shows the placeholder. Replace with:

```tsx
        {burnersData && burnersData.length > 3 ? (
          <div className="space-y-2 max-h-96 overflow-y-auto">
            {burnersData
              .slice(3)
              .map(entry => (
                <LeaderboardRow
                  key={`b-${entry.rank}`}
                  entry={entry}
                  isUser={entry.principal === userPrincipal}
                />
              ))}
          </div>
        ) : (
          <div className="text-center py-6 text-xs mc-text-muted italic">
            Only {burnersData?.length ?? 0} burners so far. Anyone with ≥1 PP burned can join the leaderboard.
          </div>
        )}
```

Note: the original slice logic `burnersData.length >= 2 ? Math.min(3, burnersData.length) : 0` had odd behavior with <2 burners — fixing here by switching to a clean `length > 3` branch. With <=3 burners we just show the placeholder. With >3 we slice from index 3.

- [ ] **Step 7: Update the Podium render call**

Find the existing podium render (right above the leaderboard list block):

```tsx
        {burnersData && burnersData.length >= 2 && (
          <Podium entries={burnersData} />
        )}
```

Change the gating condition so a single-burner case also shows the new card (it's nicer than no podium at all):

```tsx
        {burnersData && burnersData.length >= 1 && (
          <Podium entries={burnersData} />
        )}
```

- [ ] **Step 8: Run typecheck + build**

Run: `npm run build`
Expected: clean pass. If TypeScript complains about unused imports left over from `PodiumSlot`/`Podium`, prune them now.

- [ ] **Step 9: Visual sanity check (optional)**

If `npm run dev` is convenient, open the running app and confirm Hall of Fame still renders. Don't worry about Whitelisted-on/off scenarios here — full visual sweep is Task 4.

- [ ] **Step 10: Commit**

```bash
git add frontend/src/components/HallOfFame.tsx
git commit -m "$(cat <<'EOF'
ui: replace cramped HoF podium with full-width PodiumCard grid

Drops the bar-chart PodiumSlot + Podium inline functions (~60 lines) in
favor of the new Podium grid + PodiumCard lifted into hall-of-fame/.
Adds an empty-state placeholder under the podium for the common case
where there are <=3 burners total — previously the bottom half of the
card was just dead space.

Also fixes the prior slice condition that special-cased <2 burners
(was: slice(length >= 2 ? min(3, length) : 0)) — now a clean
`length > 3 ? slice(3) : placeholder` branch.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: End-to-end visual sweep + deploy

**Files:** None modified — this is verification + deploy.

- [ ] **Step 1: Full build**

Run: `npm run build`
Expected: clean pass, dist bundle generated.

- [ ] **Step 2: Local visual sweep via `npm run dev`**

Run: `npm run dev` (background process is fine).

Open the app, navigate to Shenanigans → Hall of Fame. Verify:

| Scenario | Expected |
|----------|----------|
| 3 burners, none golden | 3 large cards filling the row, #1 in middle with subtle gold tint + scale-105 lift, #2 silver, #3 bronze. No names truncated. Placeholder line below ("Only 3 burners so far…"). |
| #1 burner is golden | #1 card: thick gold border + bright glow + animated shimmer overlay + animated gold name + "◆ WHITELISTED" pill. Scale-105 lift preserved. |
| #2 or #3 burner is golden | That card's border supersedes to gold + shimmer; rank chip in the corner stays silver/bronze (preserves rank legibility). |
| Long player name (rename to e.g. "Cap Table Casualty Wisdom Cat") | Name wraps to 2 lines, does NOT truncate, gold treatment applies on both lines if golden. |
| Mobile viewport (<640px) | Cards stack vertically in #1, #2, #3 order top-down. |
| Reduced-motion (macOS Accessibility setting) | Animations halt, gold treatment becomes static gold with the existing 20px shadow. Scale lift stays (it's not motion-based). |

- [ ] **Step 3: Bundle size sanity**

The change adds ~150 lines of new code and removes ~60. Bundle should grow by under 5 kB minified. Run `npm run build` and note the dist sizes — compare against the prior commit if you want a precise delta.

- [ ] **Step 4: Commit any fixes from the sweep**

If steps 2 found issues, fix and commit before deploying.

- [ ] **Step 5: Deploy frontend to mainnet**

The user has explicit deploy-safety rules: NEVER deploy the backend without explicit permission. This change is frontend-only.

Run:
```bash
dfx deploy --network ic frontend
```

Expected: upgrades canister `5qu42-fqaaa-aaaac-qecla-cai`. Backend canisters (`backend`, `shenanigans`, `ponzi_math`, `pp_ledger`, etc.) are NOT touched.

If the build inside `dfx deploy` fails, fix locally with `npm run build` first, then retry.

- [ ] **Step 6: Verify live**

Open `https://musicalchairs.fun` (or `https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io/`). Navigate to Shenanigans → Hall of Fame. Repeat the scenario checks from Step 2 against the live deploy.

---

## Self-Review Notes

**Spec coverage check:**

| Spec section | Task |
|--------------|------|
| §1 `PodiumCard` component (rank+golden style matrix, anatomy, status pill) | Task 1 |
| §2 `Podium` grid layout (desktop order, mobile stack) | Task 2 |
| §3 Empty-list placeholder under the podium | Task 3 Step 6 |
| §4 `<GoldenName>` truncation handling (line-clamp wrapper) | Task 1 (name block uses `line-clamp-2 break-words` wrapping `<GoldenName>`) |
| §5 Status pill — Option B (no duration in v1) | Task 1 (pill renders `◆ WHITELISTED` with no time) |
| §6 Files touched | All tasks |
| §7 Out of scope | n/a (correctly omitted) |
| §8 Accessibility (role/aria-label, reduced-motion) | Task 1 (role + aria-label), pre-existing CSS for reduced-motion |
| §9 Testing | Task 4 |

**Coverage gaps:** None.

**Placeholder scan:** Plan contains complete code blocks for every code step, exact commit messages, exact file paths. No "TBD", no "implement later", no narrative-only steps.

**Type consistency check:**
- `HallOfFameEntry` defined and exported from `PodiumCard.tsx` (Task 1), imported from there in `Podium.tsx` (Task 2) and `HallOfFame.tsx` (Task 3 Step 5). One source of truth.
- `rank: 1 | 2 | 3` literal type used consistently across `PodiumCard` and `Podium`.
- No method-name inconsistencies (`resolveCardStyle` and `rankChipStyle` are only used inside Task 1's component, never referenced elsewhere).
