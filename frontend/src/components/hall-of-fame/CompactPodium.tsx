import React from 'react';
import { Medal } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { minidenticon } from 'minidenticons';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import type { HallOfFameEntry } from './PodiumCard';

interface CompactPodiumProps {
  entries: HallOfFameEntry[];
  identiconSize?: 'sm' | 'md';
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
