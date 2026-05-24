import React from 'react';
import { Trophy, Medal } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetTopPonziPointsBurners } from '../../hooks/useQueries';
import { useWallet } from '../../hooks/useWallet';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import LoadingSpinner from '../LoadingSpinner';
import CompactPodium from './CompactPodium';
import PinnedYouLine from './PinnedYouLine';
import type { HallOfFameEntry } from './PodiumCard';

const DEFAULT_VISIBLE = 7;

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
  const tail = burners.slice(3);
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
