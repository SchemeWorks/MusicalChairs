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
