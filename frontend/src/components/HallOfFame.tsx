import React from 'react';
import { useGetTopPonziPointsHolders, useGetTopPonziPointsBurners } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';

interface HallOfFameEntry {
  rank: number;
  name: string;
  ponziPoints?: number;
  ponziPointsBurned?: number;
  principal: string;
}

export default function HallOfFame() {
  const { data: holdersData, isLoading: holdersLoading, error: holdersError } = useGetTopPonziPointsHolders();
  const { data: burnersData, isLoading: burnersLoading, error: burnersError } = useGetTopPonziPointsBurners();

  if (holdersLoading || burnersLoading) return <LoadingSpinner />;

  if (holdersError || burnersError) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        Unable to load Hall of Fame data. Please try again later.
      </div>
    );
  }

  const hasData = (holdersData && holdersData.length > 0) || (burnersData && burnersData.length > 0);

  const getRankStyle = (rank: number) => {
    switch (rank) {
      case 1: return { card: 'mc-rank-gold', emoji: '🥇', label: 'Gold' };
      case 2: return { card: 'mc-rank-silver', emoji: '🥈', label: 'Silver' };
      case 3: return { card: 'mc-rank-bronze', emoji: '🥉', label: 'Bronze' };
      default: return { card: 'mc-rank-default', emoji: '🏅', label: `#${rank}` };
    }
  };

  const renderEntry = (entry: HallOfFameEntry, isHolders: boolean) => {
    const style = getRankStyle(entry.rank);
    const value = isHolders ? entry.ponziPoints : entry.ponziPointsBurned;
    return (
      <div key={`${isHolders ? 'h' : 'b'}-${entry.rank}`} className={`${style.card} p-3 flex items-center justify-between`}>
        <div className="flex items-center gap-3">
          <span className="text-xl">{style.emoji}</span>
          <div>
            <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
              entry.rank <= 3 ? 'bg-white/10' : 'bg-purple-500/10'
            } mc-text-dim`}>{style.label}</span>
            <span className="font-bold text-sm mc-text-primary ml-2">{entry.name}</span>
          </div>
        </div>
        <div className="text-right">
          <div className="text-lg font-bold mc-text-purple">{value?.toLocaleString() || 0}</div>
          <div className="text-xs mc-text-muted">{isHolders ? 'Points' : 'Burned'}</div>
        </div>
      </div>
    );
  };

  if (!hasData) {
    return (
      <div className="mc-card-elevated text-center py-10">
        <div className="text-5xl mb-4">🏆</div>
        <p className="font-bold mc-text-primary mb-1">No activity yet</p>
        <p className="text-sm mc-text-dim">Start playing to earn Ponzi Points and claim your spot.</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Holders */}
          <div>
            <h3 className="font-display text-base mc-text-primary mb-1">Top Holders</h3>
            <p className="text-xs mc-text-muted mb-4">Most Points Accumulated</p>
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {holdersData && holdersData.length > 0
                ? holdersData.map(e => renderEntry(e, true))
                : <p className="text-sm mc-text-dim text-center py-4">No holders yet</p>}
            </div>
          </div>

          {/* Burners */}
          <div>
            <h3 className="font-display text-base mc-text-primary mb-1">Top Burners</h3>
            <p className="text-xs mc-text-muted mb-4">Most Points Burned on Shenanigans</p>
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {burnersData && burnersData.length > 0
                ? burnersData.map(e => renderEntry(e, false))
                : <p className="text-sm mc-text-dim text-center py-4">No burners yet</p>}
            </div>
          </div>
        </div>
      </div>

      <div className="mc-status-blue p-4 text-center text-sm">
        <span className="font-bold">🎯 Climb the leaderboards!</span>
        <span className="mc-text-dim"> Earn Ponzi Points by depositing and referring friends. Burn them on Shenanigans to climb the burners board.</span>
      </div>
    </div>
  );
}
