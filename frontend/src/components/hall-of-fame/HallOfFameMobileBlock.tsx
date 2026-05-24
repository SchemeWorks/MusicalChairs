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

const DEFAULT_VISIBLE = 5;
const EXPANDED_VISIBLE = 7;

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
