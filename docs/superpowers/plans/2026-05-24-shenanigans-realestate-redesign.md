# Shenanigans Tab Real-Estate Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize the Shenanigans tab so Hall of Fame is ambient (sticky right rail on desktop, top-of-page block on mobile), redundant user-data cards collapse into one dense "You" line inside HoF, and three static info blocks shrink/merge — taking the page from 11 blocks to ~6 without touching the spell-casting flow or any backend code.

**Architecture:** Two new self-contained Hall-of-Fame components (`<HallOfFameRail />` for desktop sidebar, `<HallOfFameMobileBlock />` for mobile top-of-page) share three building blocks (`<CompactPodium />`, `<PinnedYouLine />`, plus the existing `LeaderboardRow`). The current `<HallOfFame />` component stops being rendered. Both new HoF variants are always mounted in the page tree; Tailwind `lg:` visibility classes (matching the existing 1024px sidebar breakpoint) ensure exactly one is visible at a time. The Shenanigans page layout is swapped to drop redundant blocks, add an (i) guardrails popover to the filter row, and merge two disclaimers into one compact footer.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, lucide-react icons, React Query (existing data hooks), `minidenticon` for avatars, `@dfinity/principal`. No new dependencies.

---

## File Structure

**New files:**
- `frontend/src/components/hall-of-fame/CompactPodium.tsx` — shared compact 3-column podium (top 3) with identicon, name, PP burned, golden treatment.
- `frontend/src/components/hall-of-fame/PinnedYouLine.tsx` — shared dense "You" stats line: rank/House Status, PP burned, casts, outcomes, karma. Hides empty fields.
- `frontend/src/components/hall-of-fame/HallOfFameRail.tsx` — desktop sticky-rail variant. Composes header + CompactPodium + LeaderboardRow list (#4–#10 by default, "see all" expands) + PinnedYouLine at bottom.
- `frontend/src/components/hall-of-fame/HallOfFameMobileBlock.tsx` — mobile top-of-page variant. Composes header + CompactPodium + PinnedYouLine + condensed LeaderboardRow list (#4–#8 by default, "see top 10" expands to #4–#10).
- `frontend/src/components/Shenanigans/LiveFeedPanel.tsx` — extracted from current inline `Shenanigans.tsx` definition; takes `defaultCollapsed` prop for mobile.
- `frontend/src/components/Shenanigans/GuardrailsTooltip.tsx` — small (i) icon button + click-controlled popover containing the three guardrail bullets. Hand-rolled (no Radix dependency added — Radix Tooltip is bundled but `TooltipProvider` is not mounted app-wide).

**Modified files:**
- `frontend/src/components/Shenanigans.tsx` — replaces the page layout per the spec; cuts the top PP balance card, the guardrails info card, the Track Record sidebar card, and both disclaimers; swaps `<HallOfFame />` for `<HallOfFameRail />` + `<HallOfFameMobileBlock />`; adds `<GuardrailsTooltip />` to the filter row; replaces inline Live Feed with `<LiveFeedPanel />`; merges disclaimers into one inline compact footer line.

**Untouched but dead-coded after this PR (cleanup is follow-up):**
- `frontend/src/components/HallOfFame.tsx` — no longer imported by Shenanigans. Stays in the repo for now; deletion deferred.
- `frontend/src/components/hall-of-fame/Podium.tsx` and `PodiumCard.tsx` — wide podium no longer rendered by Shenanigans. Stay in the repo; deletion deferred.

**Untouched and live:**
- `frontend/src/hooks/useQueries.ts` — `useGetShenaniganStats`, `useGetPonziPoints`, `useGetTopPonziPointsBurners`, `useGetKarmaReceived`, `useGetRecentShenanigans`, `useGetActiveSpellEffects` all consumed unchanged.
- `frontend/src/lib/charles.tsx` — `isCharles`, `CharlesIcon` consumed unchanged in `<PinnedYouLine />`.
- `frontend/src/components/trollbox/useDisplayName.ts` — `useDisplayName`, `useIsGolden` consumed unchanged.
- `frontend/src/components/GoldenName.tsx` — consumed unchanged.
- `frontend/src/index.css` — `.mc-shenanigans-layout` and `.mc-shenanigans-sidebar` rules unchanged. The existing sidebar is already `position: sticky` with `overflow-y: auto`; the HoF rail inherits this behavior by living inside it. No new CSS classes needed.

**Verification:**
- The project has no automated test suite. Verification is done via the dev preview (preview_start, preview_snapshot, preview_resize, preview_click), checking visual output at desktop (≥1024px) and mobile (<1024px) widths.

---

## Task 1: `<CompactPodium />` — shared top-3 podium

**Files:**
- Create: `frontend/src/components/hall-of-fame/CompactPodium.tsx`

- [ ] **Step 1: Write the component**

```tsx
import React from 'react';
import { Medal } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { minidenticon } from 'minidenticons';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import type { HallOfFameEntry } from './PodiumCard';

interface CompactPodiumProps {
  entries: HallOfFameEntry[];
  identiconSize?: 'sm' | 'md'; // sm = sidebar (h-10), md = mobile block (h-12)
}

function rankChipColor(rank: 1 | 2 | 3): string {
  if (rank === 1) return 'mc-text-gold';
  if (rank === 2) return 'text-gray-300';
  return 'text-amber-500';
}

function tintClasses(rank: 1 | 2 | 3, isGolden: boolean): string {
  if (isGolden) return 'border border-[var(--mc-gold)] bg-[var(--mc-gold)]/15';
  if (rank === 1) return 'border border-[var(--mc-gold)]/40 bg-[var(--mc-gold)]/[0.08]';
  if (rank === 2) return 'border border-gray-400/40 bg-gray-400/5';
  return 'border border-amber-600/40 bg-amber-600/5';
}

function CompactPodiumCell({ entry, rank, identiconSize }: { entry: HallOfFameEntry; rank: 1 | 2 | 3; identiconSize: 'sm' | 'md' }) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  const displayName = name || '…';
  const ppBurned = (entry.ponziPointsBurned || 0).toLocaleString();
  const chipColor = rankChipColor(rank);
  const tint = tintClasses(rank, isGolden);
  const identiconUri = React.useMemo(() => {
    const svg = minidenticon(entry.principal, 60, 50);
    return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
  }, [entry.principal]);
  const idSize = identiconSize === 'md' ? 'h-12 w-12' : 'h-10 w-10';
  return (
    <div
      role="group"
      aria-label={`Rank #${rank}: ${displayName}, ${ppBurned} PP burned${isGolden ? ', Whitelisted' : ''}`}
      className={`relative rounded-lg p-2 flex flex-col items-center text-center ${tint}`}
      title={displayName}
    >
      <div className={`absolute top-1 right-1 flex items-center gap-0.5 text-[10px] font-bold ${chipColor}`}>
        <Medal className={`h-3 w-3 ${chipColor}`} />
        <span>#{rank}</span>
      </div>
      {isGolden ? (
        <div
          className="rounded-full p-[2px] bg-[var(--mc-gold)]/40 mb-1.5 mt-3"
          style={{ boxShadow: '0 0 8px rgba(255, 215, 0, 0.5)' }}
        >
          <img src={identiconUri} alt="" className={`${idSize} rounded-full bg-zinc-800`} />
        </div>
      ) : (
        <img src={identiconUri} alt="" className={`${idSize} rounded-full bg-zinc-800 mb-1.5 mt-3`} />
      )}
      <div className="text-xs font-semibold truncate max-w-full leading-tight">
        <GoldenName name={displayName} isGolden={isGolden} className="truncate" />
      </div>
      <div className="text-sm font-bold mc-text-purple mt-0.5">{ppBurned}</div>
    </div>
  );
}

/**
 * Compact 3-column podium for the Hall of Fame rail and mobile block.
 * Order: [#2, #1, #3] on all viewports (the parent already constrains width).
 * Renders only the cells that exist when there are fewer than 3 burners.
 */
export default function CompactPodium({ entries, identiconSize = 'sm' }: CompactPodiumProps) {
  const top3 = entries.slice(0, 3);
  if (top3.length === 0) return null;
  const ordered: Array<{ entry: HallOfFameEntry; rank: 1 | 2 | 3 }> = [];
  if (top3[1]) ordered.push({ entry: top3[1], rank: 2 });
  if (top3[0]) ordered.push({ entry: top3[0], rank: 1 });
  if (top3[2]) ordered.push({ entry: top3[2], rank: 3 });
  return (
    <div className="grid grid-cols-3 gap-1 mb-3">
      {ordered.map(({ entry, rank }) => (
        <CompactPodiumCell key={`compact-${rank}`} entry={entry} rank={rank} identiconSize={identiconSize} />
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run from `frontend/`:
```bash
npx tsc --noEmit -p tsconfig.json 2>&1 | grep CompactPodium || echo OK
```
Expected: `OK` (no TypeScript errors mentioning this file).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/hall-of-fame/CompactPodium.tsx
git commit -m "feat(hof): add CompactPodium for compact 3-column top-3 display"
```

---

## Task 2: `<PinnedYouLine />` — shared dense "You" stats row

**Files:**
- Create: `frontend/src/components/hall-of-fame/PinnedYouLine.tsx`

- [ ] **Step 1: Write the component**

```tsx
import React from 'react';
import { useGetPonziPoints, useGetShenaniganStats, useGetKarmaReceived } from '../../hooks/useQueries';
import { useWallet } from '../../hooks/useWallet';
import { isCharles } from '../../lib/charles';
import type { HallOfFameEntry } from './PodiumCard';

interface PinnedYouLineProps {
  burners: HallOfFameEntry[];
  variant?: 'rail' | 'mobile'; // controls tint depth + spacing
}

/**
 * Dense single-block "You" stats row used at the bottom of the desktop HoF
 * rail and at the top of the mobile HoF block. Replaces the standalone
 * PP-balance card, Your Rank banner, Karma card, and Your Track Record card.
 *
 * Empty fields (zero / null) hide along with their preceding " · " separator
 * — when all of row 2's fields are absent, row 2 is omitted entirely.
 */
export default function PinnedYouLine({ burners, variant = 'rail' }: PinnedYouLineProps) {
  const { principal } = useWallet();
  const { data: ponziData } = useGetPonziPoints();
  const { data: stats } = useGetShenaniganStats();
  const { data: karmaUnits } = useGetKarmaReceived(principal ?? undefined);

  const userIsCharles = !!principal && isCharles(principal);
  const userPoints = ponziData?.totalPoints ?? 0;
  const myRankIndex = principal ? burners.findIndex(e => e.principal === principal) : -1;
  const myRank = myRankIndex >= 0 ? myRankIndex + 1 : null;
  const myBurned = myRankIndex >= 0 ? (burners[myRankIndex]?.ponziPointsBurned ?? 0) : 0;
  const totalCast = stats?.totalCast ? Number(stats.totalCast) : 0;
  const good = stats?.goodOutcomes ? Number(stats.goodOutcomes) : 0;
  const bad = stats?.badOutcomes ? Number(stats.badOutcomes) : 0;
  const backfires = stats?.backfires ? Number(stats.backfires) : 0;
  const karmaPp = karmaUnits ? Number(karmaUnits / 100_000_000n) : 0;

  // Row 1: rank line. Either "House Status" (Charles) or "rank #N · X PP burned".
  const row1 = userIsCharles ? (
    <span className="font-bold text-sm">
      <span className="mc-text-gold">★ You: HOUSE STATUS</span>
      <span className="mc-text-muted"> · {userPoints.toLocaleString()} PP</span>
    </span>
  ) : (
    <span className="font-bold text-sm">
      <span className="mc-text-primary">★ You: </span>
      {myRank !== null ? (
        <>
          <span className="mc-text-primary">rank #{myRank}</span>
          {myBurned > 0 && <span className="mc-text-muted"> · {myBurned.toLocaleString()} PP burned</span>}
        </>
      ) : (
        <span className="mc-text-muted">unranked · burn PP to climb</span>
      )}
    </span>
  );

  // Row 2: secondary stats. Each field is added only when non-zero.
  // Layout: render the chunks into an array and join with " · " visually
  // (we put the separator in the chunk itself so a missing field doesn't
  // leave an orphan " · ").
  const row2Parts: React.ReactNode[] = [];
  if (totalCast > 0) row2Parts.push(<span key="casts">{totalCast} cast{totalCast === 1 ? '' : 's'}</span>);
  if (good + bad + backfires > 0) row2Parts.push(<span key="outcomes">{good} good / {bad} bad / {backfires} backfire</span>);
  if (karmaPp > 0) row2Parts.push(<span key="karma">✦ {karmaPp.toLocaleString()} karma</span>);
  const showRow2 = row2Parts.length > 0;

  const bgTint = variant === 'mobile' ? 'bg-[var(--mc-purple)]/12' : 'bg-[var(--mc-purple)]/[0.08]';
  const spacing = variant === 'mobile' ? 'mt-2 mb-3' : 'mt-2 pt-2 border-t border-white/10';

  return (
    <div
      role="status"
      aria-label={
        userIsCharles
          ? `You: House Status, ${userPoints.toLocaleString()} PP`
          : myRank !== null
            ? `You: rank ${myRank}, ${myBurned.toLocaleString()} PP burned, ${totalCast} casts, ${karmaPp} karma`
            : `You: unranked, ${userPoints.toLocaleString()} PP`
      }
      className={`rounded-md px-2 py-1.5 ${bgTint} ${spacing}`}
    >
      <div className="leading-tight">{row1}</div>
      {showRow2 && (
        <div className="text-xs mc-text-muted leading-tight mt-0.5">
          {row2Parts.map((part, idx) => (
            <React.Fragment key={idx}>
              {idx > 0 && <span> · </span>}
              {part}
            </React.Fragment>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | grep PinnedYouLine || echo OK
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/hall-of-fame/PinnedYouLine.tsx
git commit -m "feat(hof): add PinnedYouLine for consolidated user stats row"
```

---

## Task 3: `<HallOfFameRail />` — desktop sticky-rail variant

**Files:**
- Create: `frontend/src/components/hall-of-fame/HallOfFameRail.tsx`

- [ ] **Step 1: Write the component**

```tsx
import React from 'react';
import { Trophy } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetTopPonziPointsBurners } from '../../hooks/useQueries';
import { useWallet } from '../../hooks/useWallet';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import { Medal } from 'lucide-react';
import LoadingSpinner from '../LoadingSpinner';
import CompactPodium from './CompactPodium';
import PinnedYouLine from './PinnedYouLine';
import type { HallOfFameEntry } from './PodiumCard';

const DEFAULT_VISIBLE = 7; // ranks #4 through #10

/**
 * Inline row used for ranks #4+ inside the rail / mobile block. Narrower than
 * the standalone LeaderboardRow so it fits cleanly in the 320px sidebar.
 */
function RailRow({ entry, isUser }: { entry: HallOfFameEntry; isUser: boolean }) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  const displayName = name || '…';
  return (
    <div
      className={`flex items-center justify-between gap-2 px-2 py-1 rounded-md text-xs ${
        isUser ? 'bg-[var(--mc-purple)]/15 ring-1 ring-purple-500/30' : ''
      }`}
    >
      <div className="flex items-center gap-1.5 min-w-0">
        <Medal className="h-3 w-3 mc-text-purple flex-shrink-0" />
        <span className="mc-text-muted w-6 flex-shrink-0">#{entry.rank}</span>
        {isGolden ? (
          <GoldenName name={displayName} isGolden={true} className="font-semibold truncate" />
        ) : (
          <span className={`font-semibold truncate ${isUser ? 'mc-text-cyan' : 'mc-text-primary'}`} title={displayName}>
            {displayName}
            {isUser ? ' (you)' : ''}
          </span>
        )}
      </div>
      <span className="font-bold mc-text-purple flex-shrink-0">{(entry.ponziPointsBurned || 0).toLocaleString()}</span>
    </div>
  );
}

export default function HallOfFameRail() {
  const { data: burnersData, isLoading, error } = useGetTopPonziPointsBurners();
  const { principal: userPrincipal } = useWallet();
  const [expanded, setExpanded] = React.useState(false);

  if (isLoading) return <div className="mc-card-elevated"><LoadingSpinner /></div>;
  if (error) {
    return (
      <div className="mc-status-red p-3 text-center text-xs">
        Unable to load Hall of Fame.
      </div>
    );
  }

  const burners = burnersData ?? [];
  const tail = burners.slice(3); // ranks #4+
  const visibleTail = expanded ? tail : tail.slice(0, DEFAULT_VISIBLE);
  const showSeeAll = tail.length > DEFAULT_VISIBLE;

  return (
    <section aria-label="Hall of Fame" className="mc-card-elevated p-3">
      <div className="flex items-center gap-1.5 mb-2">
        <Trophy className="h-4 w-4 mc-text-gold" />
        <h3 className="font-display text-sm mc-text-primary">Hall of Fame</h3>
      </div>

      <CompactPodium entries={burners} identiconSize="sm" />

      {tail.length > 0 ? (
        <div className="space-y-0.5">
          {visibleTail.map(entry => (
            <RailRow key={`rail-${entry.rank}`} entry={entry} isUser={entry.principal === (userPrincipal ?? '')} />
          ))}
          {showSeeAll && (
            <button
              type="button"
              onClick={() => setExpanded(v => !v)}
              aria-expanded={expanded}
              className="mc-text-muted text-xs hover:mc-text-purple w-full text-center py-1 mt-1"
            >
              {expanded ? '↑ show fewer' : `see all ${burners.length} →`}
            </button>
          )}
        </div>
      ) : (
        burners.length > 0 && burners.length <= 3 && (
          <div className="text-center py-2 text-[11px] mc-text-muted italic">
            Only {burners.length} burner{burners.length === 1 ? '' : 's'} so far.
          </div>
        )
      )}

      <PinnedYouLine burners={burners} variant="rail" />
    </section>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | grep HallOfFameRail || echo OK
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/hall-of-fame/HallOfFameRail.tsx
git commit -m "feat(hof): add HallOfFameRail for desktop sticky sidebar"
```

---

## Task 4: `<HallOfFameMobileBlock />` — mobile top-of-page variant

**Files:**
- Create: `frontend/src/components/hall-of-fame/HallOfFameMobileBlock.tsx`

- [ ] **Step 1: Write the component**

```tsx
import React from 'react';
import { Trophy } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetTopPonziPointsBurners } from '../../hooks/useQueries';
import { useWallet } from '../../hooks/useWallet';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import { Medal } from 'lucide-react';
import LoadingSpinner from '../LoadingSpinner';
import CompactPodium from './CompactPodium';
import PinnedYouLine from './PinnedYouLine';
import type { HallOfFameEntry } from './PodiumCard';

const DEFAULT_VISIBLE = 5; // ranks #4 through #8
const EXPANDED_VISIBLE = 7; // ranks #4 through #10

function MobileRow({ entry, isUser }: { entry: HallOfFameEntry; isUser: boolean }) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  const displayName = name || '…';
  return (
    <div
      className={`flex items-center justify-between gap-2 px-2 py-1.5 rounded-md text-xs ${
        isUser ? 'bg-[var(--mc-purple)]/15 ring-1 ring-purple-500/30' : ''
      }`}
    >
      <div className="flex items-center gap-2 min-w-0">
        <Medal className="h-3.5 w-3.5 mc-text-purple flex-shrink-0" />
        <span className="mc-text-muted w-6 flex-shrink-0">#{entry.rank}</span>
        {isGolden ? (
          <GoldenName name={displayName} isGolden={true} className="font-semibold truncate" />
        ) : (
          <span className={`font-semibold truncate ${isUser ? 'mc-text-cyan' : 'mc-text-primary'}`} title={displayName}>
            {displayName}
            {isUser ? ' (you)' : ''}
          </span>
        )}
      </div>
      <span className="font-bold mc-text-purple flex-shrink-0">{(entry.ponziPointsBurned || 0).toLocaleString()}</span>
    </div>
  );
}

export default function HallOfFameMobileBlock() {
  const { data: burnersData, isLoading, error } = useGetTopPonziPointsBurners();
  const { principal: userPrincipal } = useWallet();
  const [expanded, setExpanded] = React.useState(false);

  if (isLoading) return <div className="mc-card-elevated"><LoadingSpinner /></div>;
  if (error) {
    return (
      <div className="mc-status-red p-3 text-center text-xs">
        Unable to load Hall of Fame.
      </div>
    );
  }

  const burners = burnersData ?? [];
  const tail = burners.slice(3);
  const visibleCount = expanded ? EXPANDED_VISIBLE : DEFAULT_VISIBLE;
  const visibleTail = tail.slice(0, visibleCount);
  // Reveal button only matters when there's something beyond the default to show.
  const showExpand = tail.length > DEFAULT_VISIBLE;

  return (
    <section aria-label="Hall of Fame" className="mc-card-elevated p-3">
      <div className="flex items-center gap-1.5 mb-2">
        <Trophy className="h-4 w-4 mc-text-gold" />
        <h3 className="font-display text-sm mc-text-primary">Hall of Fame</h3>
      </div>

      <CompactPodium entries={burners} identiconSize="md" />

      <PinnedYouLine burners={burners} variant="mobile" />

      {tail.length > 0 ? (
        <div className="space-y-0.5">
          {visibleTail.map(entry => (
            <MobileRow key={`mobile-${entry.rank}`} entry={entry} isUser={entry.principal === (userPrincipal ?? '')} />
          ))}
          {showExpand && !expanded && (
            <button
              type="button"
              onClick={() => setExpanded(true)}
              aria-expanded={false}
              className="mc-text-muted text-xs hover:mc-text-purple w-full text-center py-1 mt-1"
            >
              see top 10 ↓
            </button>
          )}
        </div>
      ) : (
        burners.length > 0 && burners.length <= 3 && (
          <div className="text-center py-2 text-[11px] mc-text-muted italic">
            Only {burners.length} burner{burners.length === 1 ? '' : 's'} so far.
          </div>
        )
      )}
    </section>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | grep HallOfFameMobileBlock || echo OK
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/hall-of-fame/HallOfFameMobileBlock.tsx
git commit -m "feat(hof): add HallOfFameMobileBlock for mobile top-of-page"
```

---

## Task 5: `<GuardrailsTooltip />` — (i) button + click-controlled popover

**Files:**
- Create: `frontend/src/components/Shenanigans/GuardrailsTooltip.tsx`

- [ ] **Step 1: Create the directory and the component**

```bash
mkdir -p frontend/src/components/Shenanigans
```

Then write the file:

```tsx
import React from 'react';
import { Info, Shield, Zap, AlertTriangle } from 'lucide-react';

/**
 * Tap/click-controlled popover holding the three Shenanigans guardrails.
 * Replaces the full Guardrails info card from the previous page layout.
 * Hand-rolled (no Radix dependency added — Radix Tooltip is bundled but
 * TooltipProvider is not mounted app-wide).
 */
export default function GuardrailsTooltip() {
  const [open, setOpen] = React.useState(false);
  const wrapRef = React.useRef<HTMLDivElement | null>(null);

  // Close on outside click + Escape
  React.useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        aria-label="Guardrails"
        aria-expanded={open}
        onClick={() => setOpen(v => !v)}
        className="h-7 w-7 rounded-full bg-white/5 hover:bg-white/10 flex items-center justify-center mc-text-muted hover:mc-text-cyan"
        title="Guardrails"
      >
        <Info className="h-4 w-4" />
      </button>
      {open && (
        <div
          role="dialog"
          aria-label="Shenanigans guardrails"
          className="absolute right-0 top-full mt-2 z-50 w-72 rounded-lg border border-white/10 bg-zinc-900 shadow-xl p-3"
          style={{ boxShadow: '0 12px 40px rgba(0,0,0,0.5)' }}
        >
          <h4 className="font-display text-sm mc-text-primary mb-2 flex items-center gap-1.5">
            <Shield className="h-3.5 w-3.5 mc-text-cyan" /> Guardrails
          </h4>
          <div className="space-y-2 text-xs mc-text-dim">
            <div className="flex items-start gap-2">
              <Info className="h-3 w-3 mc-text-cyan mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">PP &amp; Cosmetics Only</strong> — Never affects ICP, pot, backer selection, or payout math.</span>
            </div>
            <div className="flex items-start gap-2">
              <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">Cooldowns</strong> — A successful cast locks that spell for hours. Failures and backfires? Try again immediately.</span>
            </div>
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-3 w-3 mc-text-gold mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">No Refunds</strong> — Every cast burns PP, win or lose.</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | grep GuardrailsTooltip || echo OK
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/Shenanigans/GuardrailsTooltip.tsx
git commit -m "feat(shenanigans): add GuardrailsTooltip (i) popover"
```

---

## Task 6: Extract `<LiveFeedPanel />` from inline definition

**Files:**
- Create: `frontend/src/components/Shenanigans/LiveFeedPanel.tsx`

Note: This task creates the file but does NOT yet swap `Shenanigans.tsx` to use it — the swap happens in Task 7 to keep the page in a working state until the full layout change.

- [ ] **Step 1: Write the component**

```tsx
import React from 'react';
import { Principal } from '@dfinity/principal';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import type { ShenaniganRecord } from '../../backend';

interface LiveFeedRowProps {
  record: ShenaniganRecord;
  spellName: string;
  spellIcon: React.ReactNode;
}

// Variant tags are objects like { success: null }; extract the single key.
const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

function LiveFeedRow({ record, spellName, spellIcon }: LiveFeedRowProps) {
  const casterName = useDisplayName(record.user);
  const isCasterGolden = useIsGolden(record.user);
  const target = record.target[0] ?? null;
  const targetName = useDisplayName(target);
  const isTargetGolden = useIsGolden(target);
  const outcomeKey = variantKey(record.outcome);
  const outcomeColor =
    outcomeKey === 'success' ? 'mc-text-green' :
    outcomeKey === 'fail' ? 'mc-text-danger' :
    'mc-text-purple';
  return (
    <div className="mc-card p-2 text-xs space-y-1">
      <div className="flex items-center justify-between gap-2">
        {isCasterGolden ? (
          <GoldenName name={casterName || 'Anon'} isGolden={true} className="font-bold truncate" />
        ) : (
          <span className="font-bold mc-text-primary truncate">{casterName || 'Anon'}</span>
        )}
        <span className={`font-bold flex-shrink-0 ${outcomeColor}`}>{outcomeKey.toUpperCase()}</span>
      </div>
      <div className="mc-text-dim flex items-center gap-1 min-w-0">
        <span className="flex-shrink-0">{spellIcon}</span>
        <span className="truncate">{spellName}</span>
        {target ? (
          isTargetGolden ? (
            <span className="mc-text-muted truncate"> → <GoldenName name={targetName} isGolden={true} /></span>
          ) : (
            <span className="mc-text-muted truncate"> → {targetName}</span>
          )
        ) : null}
      </div>
    </div>
  );
}

export interface LiveFeedPanelProps {
  records: ShenaniganRecord[];
  resolveSpell: (record: ShenaniganRecord) => { name: string; icon: React.ReactNode };
  defaultCollapsed?: boolean;
}

/**
 * Recent-casts feed. On mobile parents pass defaultCollapsed=true to reduce
 * scroll length; users tap the header to expand. On desktop the panel is
 * always expanded (defaultCollapsed defaults to false).
 */
export default function LiveFeedPanel({ records, resolveSpell, defaultCollapsed = false }: LiveFeedPanelProps) {
  const [open, setOpen] = React.useState(!defaultCollapsed);
  const trimmed = records.slice(0, 20);
  return (
    <div className="mc-card-elevated p-3">
      <button
        type="button"
        onClick={() => setOpen(v => !v)}
        aria-expanded={open}
        className="flex items-center justify-between w-full mb-2"
      >
        <h3 className="font-display text-sm mc-text-primary">Live Feed</h3>
        <span className="text-xs mc-text-muted">{open ? '▴' : 'latest casts ▾'}</span>
      </button>
      {open && (
        <div className="space-y-2 max-h-72 overflow-y-auto">
          {trimmed.length > 0 ? (
            trimmed.map(s => {
              const { name, icon } = resolveSpell(s);
              return <LiveFeedRow key={s.id.toString()} record={s} spellName={name} spellIcon={icon} />;
            })
          ) : (
            <p className="text-center mc-text-muted text-xs py-4">No shenanigans cast yet. Be the first!</p>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | grep LiveFeedPanel || echo OK
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/Shenanigans/LiveFeedPanel.tsx
git commit -m "feat(shenanigans): extract LiveFeedPanel with defaultCollapsed prop"
```

---

## Task 7: Swap `Shenanigans.tsx` layout

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx`

This is the big change that brings the redesign online. Before this task the new components exist but are unused. After this task the page renders the new layout.

- [ ] **Step 1: Update imports at the top of `Shenanigans.tsx`**

Remove the `HallOfFame` import (line 11) and the `Trophy` icon from the lucide-react import (no longer used here — it lives inside the new HoF components).

Replace lines 10–11:
```tsx
import { Info, Shield, Zap, AlertTriangle, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, Dices, Trophy, LayoutGrid, List } from 'lucide-react';
import HallOfFame from './HallOfFame';
```

With:
```tsx
import { Shield, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, Dices, LayoutGrid, List } from 'lucide-react';
import HallOfFameRail from './hall-of-fame/HallOfFameRail';
import HallOfFameMobileBlock from './hall-of-fame/HallOfFameMobileBlock';
import LiveFeedPanel from './Shenanigans/LiveFeedPanel';
import GuardrailsTooltip from './Shenanigans/GuardrailsTooltip';
```

(`Info`, `Zap`, `AlertTriangle` moved into `GuardrailsTooltip`. `Trophy` was only used by the section header for the now-removed wide HoF. `Shield` is still used elsewhere in the file for Poison Pill UI.)

- [ ] **Step 2: Remove the inline `LiveFeedRow` function**

Delete lines 94–136 (the entire `function LiveFeedRow({...}: {...}) { ... }` block). It now lives inside `LiveFeedPanel.tsx`.

- [ ] **Step 3: Replace the page return JSX**

Find the return statement at line 506 (`return (`) and replace the JSX block that starts there with the new layout. The new return — top of the component — is:

```tsx
  return (
    <div className="space-y-6">
      {/* Mobile-only: Hall of Fame at top of page. Hidden on lg+. */}
      <div className="block lg:hidden">
        <HallOfFameMobileBlock />
      </div>

      <ActiveEffectsStrip effects={activeEffects ?? null} />

      {/* Desktop 2-column layout: cards left, sidebar right. */}
      <div className="mc-shenanigans-layout">
        {/* Left column: filter row + cards */}
        <div className="space-y-6">
          {/* Filter tabs + view toggle + guardrails (i) */}
          <div className="flex items-center gap-2">
            <div className="flex flex-1 rounded-lg bg-white/5 p-0.5">
              {([
                { key: 'all' as FilterCategory, label: 'All' },
                { key: 'offense' as FilterCategory, label: 'Offense' },
                { key: 'defense' as FilterCategory, label: 'Defense' },
                { key: 'chaos' as FilterCategory, label: 'Chaos' },
              ]).map(tab => (
                <button
                  key={tab.key}
                  onClick={() => setFilterCategory(tab.key)}
                  className={`flex-1 py-2 text-xs font-bold rounded-md transition-all ${
                    filterCategory === tab.key ? 'bg-[var(--mc-purple)]/25 mc-text-primary border border-[var(--mc-purple)]/30' : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>
            <div className="flex items-center gap-1 ml-auto">
              <button
                type="button"
                onClick={() => setViewMode('cards')}
                className={viewMode === 'cards' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60 hover:opacity-100'}
                aria-label="Card view"
              >
                <LayoutGrid className="w-4 h-4" />
              </button>
              <button
                type="button"
                onClick={() => setViewMode('compact')}
                className={viewMode === 'compact' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60 hover:opacity-100'}
                aria-label="List view"
              >
                <List className="w-4 h-4" />
              </button>
              <GuardrailsTooltip />
            </div>
          </div>

          {/* Shenanigan cards grid / compact list — UNCHANGED block from the
              previous version. Copy lines 561–702 of the old file as-is. */}
          {/* PASTE: the existing `{viewMode === 'cards' ? (...) : (...)}` block */}

          {/* Empty state when filter matches nothing — UNCHANGED, lines 705–713. */}
          {/* PASTE: the existing empty-state block */}
        </div>

        {/* Right column (desktop): HoF rail on top, Live Feed below — sticky via .mc-shenanigans-sidebar */}
        <div className="mc-shenanigans-sidebar space-y-4">
          <HallOfFameRail />
          <LiveFeedPanel
            records={recentShenanigans ?? []}
            resolveSpell={(s) => {
              const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
              return { name: config?.name ?? 'Unknown', icon: config?.icon ?? null };
            }}
          />
        </div>
      </div>

      {/* Mobile-only Live Feed: collapsed by default. Hidden on lg+. */}
      <div className="block lg:hidden">
        <LiveFeedPanel
          records={recentShenanigans ?? []}
          resolveSpell={(s) => {
            const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
            return { name: config?.name ?? 'Unknown', icon: config?.icon ?? null };
          }}
          defaultCollapsed
        />
      </div>

      {/* Compact footer — replaces the previous two stacked disclaimers */}
      <div className="text-center text-xs mc-text-muted mt-2">
        PP &amp; cosmetics only · pure entertainment · no refunds
      </div>

      {/* All modals (WhitelistedFanfare, outcome toast, rename modal, target
          picker, confirm dialog) — UNCHANGED. Copy lines 797–995 of the old
          file as-is. */}
      {/* PASTE: WhitelistedFanfare, outcomeToast modal, renamePrompt modal,
                TargetPicker, confirmOpen modal — all unchanged. */}
    </div>
  );
```

**Concrete edit recipe** (since some sections are pasted from the existing file):

1. Open `frontend/src/components/Shenanigans.tsx`.
2. Replace lines 506–795 (everything from `return (` through the closing `)` of the bottom `<div>` of the page block — i.e. through the closing of the Footer block at line 795) with the JSX shown above, copy-pasting the spell-grid block (`{viewMode === 'cards' ? ... : ...}`), the empty-state block, and the modal blocks from the OLD lines:
   - Spell grid: old lines 561–702 — paste at `{/* PASTE: the existing ... block */}` marker.
   - Empty state: old lines 705–713 — paste at the second marker.
   - Modals: old lines 797–995 — paste at the bottom marker, before the closing `</div>`.
3. The desktop sidebar now contains `<HallOfFameRail />` + `<LiveFeedPanel />` (stacked with `space-y-4`). The `mc-shenanigans-sidebar` CSS class already provides `position: sticky; overflow-y: auto;` for desktop, so internal scroll handles overflow if both panels together exceed viewport height.

Removed completely:
- Top "Your Ponzi Points" balance card (old lines 508–512).
- Old guardrails info card (old lines 715–734).
- Old "Your Track Record" sidebar grid (old lines 740–757).
- Old inline Live Feed (old lines 759–778).
- Old wide Hall of Fame section + heading (old lines 782–789).
- Old `<HallOfFame />` invocation.
- Old footer disclaimer (old lines 791–795).

- [ ] **Step 4: Verify it compiles**

```bash
cd frontend && npx tsc --noEmit -p tsconfig.json 2>&1 | head -40
```
Expected: no errors. If TypeScript flags anything, fix and re-run.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat(shenanigans): swap page layout for ambient Hall of Fame

Removes the top PP balance card (redundant with HoF You line), the
guardrails info card (now an (i) popover), the Track Record sidebar
card (absorbed into PinnedYouLine), the inline Live Feed block (now
extracted), the wide Hall of Fame section, and one of the two disclaimers.
Mounts HallOfFameRail in the existing sticky sidebar and
HallOfFameMobileBlock at the top of the page on mobile."
```

---

## Task 8: Visual verification in dev preview

**Files:** None modified — verification only.

Use the preview tools to confirm the redesign works at desktop and mobile breakpoints. The verification is split into checkpoints; if any fail, diagnose by reading the source files, fix, re-verify.

- [ ] **Step 1: Start the dev server**

Use `preview_start` (no command argument — defaults to project-configured dev server). If the project's dev server isn't auto-detected, run `npm run dev` from `frontend/` via Bash with `run_in_background: true`, then `preview_start` the resulting URL.

Expected: dev server running, preview URL available.

- [ ] **Step 2: Desktop layout check (≥1024px)**

```
preview_resize: width=1440, height=900
preview_eval: window.location.hash = '#/shenanigans'   (or click the Tricks tab via preview_click)
preview_snapshot
```

In the snapshot, verify:
- HoF rail visible in the right column with header "Hall of Fame", compact 3-column podium, leaderboard rows for #4+, "see all N →" button if applicable, and pinned "You" line at the bottom.
- Spell grid visible in the left column.
- Live Feed panel visible BELOW the HoF rail in the sidebar.
- No "Your Ponzi Points" card at the top.
- No "Your Track Record" stats grid in the sidebar.
- No "Hall of Fame" section heading at the bottom of the page.
- (i) icon visible to the right of the card/list view toggle on the filter row.
- Footer is a single muted line.

- [ ] **Step 3: Desktop sticky-rail check**

```
preview_eval: window.scrollTo(0, 600)
preview_snapshot
```

Verify the sidebar (containing HoF + Live Feed) stays visible — its content is still in view at the top of the viewport, even after scrolling the spell grid down 600px.

- [ ] **Step 4: Desktop guardrails tooltip check**

```
preview_click: the (i) Info button on the filter row
preview_snapshot
```

Verify the popover opens with the three guardrails bullets. Then:
```
preview_click: somewhere outside the popover (e.g. the spell grid)
preview_snapshot
```
Verify the popover closes.

- [ ] **Step 5: Mobile layout check (<1024px)**

```
preview_resize: width=390, height=844
preview_snapshot
```

Verify:
- HoF mobile block is at the very top of the page (above Active Effects, filter tabs, spell grid).
- Compact podium visible inside the HoF block.
- "You" line visible inside the HoF block, directly under the podium.
- Top-5 leaderboard rows visible below the "You" line; "see top 10 ↓" button visible only if total burners > 8.
- Active Effects strip (if any), filter tabs (with the (i) button), spell grid follow in order below the HoF block.
- Live Feed collapsed: header shows "Live Feed · latest casts ▾".

- [ ] **Step 6: Mobile Live Feed expand check**

```
preview_click: the "Live Feed · latest casts ▾" header
preview_snapshot
```

Verify the feed expands inline to show recent casts.

- [ ] **Step 7: Mobile "see top 10" expand check (only if total burners > 8)**

```
preview_click: the "see top 10 ↓" button
preview_snapshot
```

Verify the list extends to show ranks #9 and #10 (7 visible total below the podium).

- [ ] **Step 8: Console + network sanity**

```
preview_console_logs
preview_network
```

Verify no new errors or 4xx/5xx failures introduced by the redesign. Pre-existing warnings unrelated to this change can be ignored.

- [ ] **Step 9: Screenshot for the PR**

```
preview_screenshot: full page at desktop width
preview_resize: width=390, height=844
preview_screenshot: full page at mobile width
```

Saves visual proof. No commit step.

---

## Task 9: Deploy frontend and push commits

**Files:** None modified.

The Musical Chairs project deploys frontend assets to the IC asset canister. The user has explicitly authorized this deploy. Backend is NOT touched in this PR.

- [ ] **Step 1: Identify the frontend deploy command**

```bash
ls /Users/robertripley/coding/musicalchairs/icp.yaml /Users/robertripley/coding/musicalchairs/dfx.json 2>/dev/null
grep -E "build|deploy|frontend|assets" /Users/robertripley/coding/musicalchairs/icp.yaml 2>/dev/null | head -10
grep -E "build|deploy|frontend|assets" /Users/robertripley/coding/musicalchairs/dfx.json 2>/dev/null | head -10
ls /Users/robertripley/coding/musicalchairs/scripts/ 2>/dev/null
```

The output reveals whether the project uses `icp deploy <recipe>`, `dfx deploy <canister>`, or a shell script under `scripts/`. Use the same command the user has used recently — check `git log --all --grep="deploy"` if uncertain. **If the deploy command is not obvious from these files, STOP and ask the user before running anything.**

- [ ] **Step 2: Build the frontend**

Whatever the project uses — typically one of:
- `cd frontend && npm run build`
- `icp build <frontend-recipe>`
- A combined deploy script that builds + uploads

Run the build step. Expected: build succeeds with no TypeScript errors. If errors, fix them in source and re-run.

- [ ] **Step 3: Deploy frontend assets only**

Run the project's frontend-only deploy command (most likely something like `icp deploy frontend` or `dfx deploy <asset-canister-name> --network ic` — DO NOT use `dfx deploy --network ic` without a canister name, as that would also redeploy the backend). 

**STOP** before running if there's any ambiguity about what gets deployed.

Expected: asset canister upload completes; the project's standard "deployed" output appears.

- [ ] **Step 4: Push commits to the remote**

```bash
git status --short
git log origin/main..HEAD --oneline
```

Verify the commits to push are exactly the seven new commits from this plan (CompactPodium, PinnedYouLine, HallOfFameRail, HallOfFameMobileBlock, GuardrailsTooltip, LiveFeedPanel, Shenanigans.tsx swap) plus the spec commits from earlier.

```bash
git push origin main
```

Expected: push succeeds.

- [ ] **Step 5: Final state check**

```bash
git status
git log --oneline -10
```

Confirm working tree is clean and the new commits are at HEAD.

---

## Self-Review

Checking the plan against the spec:

**Spec coverage:**
- §1 New page structure → Task 7 (full layout swap).
- §2 HallOfFameRail → Task 3.
- §3 PinnedYouLine → Task 2 (and uses hide-empty-fields per spec amendment).
- §4 HallOfFameMobileBlock → Task 4.
- §5 Cuts and merges (every block accounted for) → Task 7 removes each block listed.
- §6 Guardrails (i) tooltip → Task 5 (component) + Task 7 (wiring).
- §7 LiveFeedPanel with defaultCollapsed → Task 6 (extraction) + Task 7 (wiring with mobile collapsed).
- §8 Files touched → matches File Structure section above.
- §9 Out of scope items → not implemented (per spec).
- §10 Accessibility → aria-labels included in CompactPodium, PinnedYouLine (`role="status"`), HoF rail/mobile block (`section aria-label`), GuardrailsTooltip (`aria-expanded`, `role="dialog"`).
- §11 Testing → Task 8 covers every desktop and mobile check listed in the spec.

**Type/method consistency:** `HallOfFameEntry` imported consistently from `./PodiumCard`. `useWallet().principal` is a string throughout (matches existing usage in HallOfFame.tsx). `useGetShenaniganStats` returns BigInt counts — converted via `Number(...)` in PinnedYouLine. `useGetKarmaReceived` returns `0n` fallback, divided by `100_000_000n`. `useGetTopPonziPointsBurners` returns `HallOfFameEntry[]` with `rank`, `principal`, `ponziPointsBurned`.

**No placeholders:** Every step contains the actual code or the exact diff recipe. Task 7's "PASTE: the existing block" markers point to specific line ranges in the existing file and clearly indicate that the content is unchanged from those lines — not "TBD".

**One judgement-call deferral** in Task 9 step 1: deploy command discovery. The plan tells the engineer to STOP and ask if the command isn't obvious, rather than guessing — this is the correct behavior given the user's standing memory rule that backend deploys require explicit permission per deploy.
