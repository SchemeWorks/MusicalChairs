import React from 'react';
import { Medal, Trophy, Target, Gem, Shield } from 'lucide-react';
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
      case 1: return { card: 'mc-rank-gold', icon: <Medal className="h-5 w-5 text-yellow-400" />, label: 'Gold' };
      case 2: return { card: 'mc-rank-silver', icon: <Medal className="h-5 w-5 text-gray-300" />, label: 'Silver' };
      case 3: return { card: 'mc-rank-bronze', icon: <Medal className="h-5 w-5 text-amber-600" />, label: 'Bronze' };
      default: return { card: 'mc-rank-default', icon: <Medal className="h-5 w-5 mc-text-purple" />, label: `#${rank}` };
    }
  };

  const renderEntry = (entry: HallOfFameEntry, isHolders: boolean) => {
    const style = getRankStyle(entry.rank);
    const value = isHolders ? entry.ponziPoints : entry.ponziPointsBurned;
    const isTop3 = entry.rank <= 3;
    return (
      <div
        key={`${isHolders ? 'h' : 'b'}-${entry.rank}`}
        className={`${style.card} p-3 flex items-center justify-between transition-all ${
          isTop3 ? 'ring-1 ring-white/10' : ''
        }`}
        style={isTop3 ? {
          boxShadow: entry.rank === 1
            ? '0 0 12px rgba(255, 215, 0, 0.15)'
            : entry.rank === 2
              ? '0 0 10px rgba(192, 192, 192, 0.12)'
              : '0 0 10px rgba(205, 127, 50, 0.12)'
        } : undefined}
      >
        <div className="flex items-center gap-3">
          <span className="flex items-center">{style.icon}</span>
          <div>
            <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
              isTop3 ? 'bg-white/10' : 'bg-purple-500/10'
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
        <Trophy className="h-12 w-12 mc-text-gold mb-4 mx-auto" />
        <p className="font-display text-lg mc-text-primary mb-2">The Leaderboard Is Empty</p>
        <p className="text-sm mc-text-dim mb-4">Start playing to earn Ponzi Points and claim your spot.</p>
        <p className="font-accent text-xs mc-text-dim italic">
          &ldquo;Every empire starts with a first transaction.&rdquo;
          <span className="block mc-text-muted font-bold mt-1 not-italic">&mdash; Charles</span>
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Holders */}
          <div>
            <div className="flex items-center gap-2 mb-1">
              <Trophy className="h-4 w-4 mc-text-gold" />
              <h3 className="font-display text-base mc-text-primary">Top Holders</h3>
            </div>
            <p className="text-xs mc-text-muted mb-4">Most Points Accumulated</p>
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {holdersData && holdersData.length > 0
                ? holdersData.map(e => renderEntry(e, true))
                : <p className="text-sm mc-text-dim text-center py-4">No holders yet</p>}
            </div>
          </div>

          {/* Diamond Tier */}
          <div>
            <div className="flex items-center gap-2 mb-1">
              <Gem className="h-4 w-4 mc-text-cyan" />
              <h3 className="font-display text-base mc-text-primary">Diamond Tier</h3>
            </div>
            <p className="text-xs mc-text-muted mb-4">Most Points Spent on Shenanigans</p>
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {burnersData && burnersData.length > 0
                ? burnersData.map(e => renderEntry(e, false))
                : <p className="text-sm mc-text-dim text-center py-4">No Diamond Tier members yet</p>}
            </div>
          </div>
        </div>
      </div>

      {/* PP disclaimer */}
      <div className="mc-card p-5">
        <h3 className="font-display text-sm mc-text-primary mb-3 flex items-center gap-2">
          <Shield className="h-4 w-4 mc-text-cyan" /> About Ponzi Points & Shenanigans
        </h3>
        <p className="text-xs mc-text-dim leading-relaxed mb-3">
          Ponzi Points are the in-game fun currency. Shenanigans are cosmetic chaos you cast using PP.
          They don't affect the actual game math &mdash; just the madness.
          Burn those Ponzi Points for glory!
        </p>
        <div className="flex items-start gap-2 text-xs mc-text-dim">
          <span className="mc-text-gold mt-0.5">&#9888;</span>
          <span>All effects are limited to Ponzi Points and cosmetics only &mdash; never touching ICP, pot size, backer selection, payout math, or round structure.</span>
        </div>
      </div>

      <div className="mc-status-blue p-4 text-center text-sm">
        <span className="font-bold inline-flex items-center gap-2"><Target className="h-4 w-4" /> Climb the leaderboards!</span>
        <span className="mc-text-dim"> Earn Ponzi Points by depositing and referring friends. Spend them on Shenanigans to reach Diamond Tier.</span>
      </div>
    </div>
  );
}
