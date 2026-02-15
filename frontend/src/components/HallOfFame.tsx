import React from 'react';
import { Medal, Trophy, Target, Gem, Shield, Crown } from 'lucide-react';
import { useGetTopPonziPointsHolders, useGetTopPonziPointsBurners, useGetPonziPoints } from '../hooks/useQueries';
import { useWallet } from '../hooks/useWallet';
import LoadingSpinner from './LoadingSpinner';

interface HallOfFameEntry {
  rank: number;
  name: string;
  ponziPoints?: number;
  ponziPointsBurned?: number;
  principal: string;
}

function Podium({ entries, isHolders }: { entries: HallOfFameEntry[]; isHolders: boolean }) {
  const top3 = entries.slice(0, 3);
  if (top3.length === 0) return null;

  // Reorder for podium: [2nd, 1st, 3rd]
  const podiumOrder = top3.length >= 3
    ? [top3[1], top3[0], top3[2]]
    : top3.length === 2
    ? [top3[1], top3[0]]
    : [top3[0]];

  const heights = { 1: 'h-28', 2: 'h-20', 3: 'h-14' };
  const medals = {
    1: { bg: 'bg-yellow-500/20', border: 'border-yellow-500/40', text: 'text-yellow-400', glow: '0 0 16px rgba(255, 215, 0, 0.2)', icon: <Crown className="h-5 w-5 text-yellow-400" /> },
    2: { bg: 'bg-gray-400/10', border: 'border-gray-400/30', text: 'text-gray-300', glow: '0 0 12px rgba(192, 192, 192, 0.15)', icon: <Medal className="h-4 w-4 text-gray-300" /> },
    3: { bg: 'bg-amber-600/15', border: 'border-amber-600/30', text: 'text-amber-500', glow: '0 0 12px rgba(205, 127, 50, 0.15)', icon: <Medal className="h-4 w-4 text-amber-500" /> },
  };

  return (
    <div className={`flex items-end justify-center gap-2 mb-6 ${podiumOrder.length === 1 ? '' : ''}`}>
      {podiumOrder.map(entry => {
        const rank = entry.rank as 1 | 2 | 3;
        const m = medals[rank];
        const h = heights[rank];
        const value = isHolders ? entry.ponziPoints : entry.ponziPointsBurned;
        return (
          <div key={entry.rank} className="flex flex-col items-center" style={{ minWidth: '90px' }}>
            {/* Avatar + name */}
            <div className={`w-10 h-10 rounded-full ${m.bg} border ${m.border} flex items-center justify-center mb-1.5`} style={{ boxShadow: m.glow }}>
              {m.icon}
            </div>
            <span className="text-xs font-bold mc-text-primary truncate max-w-[80px] text-center">{entry.name}</span>
            <span className="text-xs font-bold mc-text-purple">{(value || 0).toLocaleString()}</span>
            {/* Podium block */}
            <div className={`${h} w-full mt-2 rounded-t-lg ${m.bg} border-t border-x ${m.border} flex items-start justify-center pt-2`}>
              <span className={`font-display text-sm ${m.text}`}>#{rank}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

export default function HallOfFame() {
  const { data: holdersData, isLoading: holdersLoading, error: holdersError } = useGetTopPonziPointsHolders();
  const { data: burnersData, isLoading: burnersLoading, error: burnersError } = useGetTopPonziPointsBurners();
  const { data: ponziData } = useGetPonziPoints();
  const { principal } = useWallet();

  if (holdersLoading || burnersLoading) return <LoadingSpinner />;

  if (holdersError || burnersError) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        Unable to load Hall of Fame data. Please try again later.
      </div>
    );
  }

  const hasData = (holdersData && holdersData.length > 0) || (burnersData && burnersData.length > 0);

  // Find user's rank
  const userPrincipal = principal || '';
  const userHolderRank = holdersData?.findIndex(e => e.principal === userPrincipal);
  const userBurnerRank = burnersData?.findIndex(e => e.principal === userPrincipal);
  const userPoints = ponziData?.totalPoints || 0;

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
    const isUser = entry.principal === userPrincipal;
    return (
      <div
        key={`${isHolders ? 'h' : 'b'}-${entry.rank}`}
        className={`${style.card} p-3 flex items-center justify-between transition-all ${
          isTop3 ? 'ring-1 ring-white/10' : ''
        } ${isUser ? 'ring-2 ring-purple-500/40' : ''}`}
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
            <span className={`font-bold text-sm ml-2 ${isUser ? 'mc-text-cyan' : 'mc-text-primary'}`}>
              {entry.name}{isUser ? ' (you)' : ''}
            </span>
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
      {/* Your Rank banner */}
      <div className="mc-card p-4 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Target className="h-5 w-5 mc-text-cyan" />
          <div>
            <span className="text-xs mc-label">Your Rank</span>
            <div className="font-bold mc-text-primary text-sm">
              {userHolderRank !== undefined && userHolderRank >= 0 ? (
                <span className={userHolderRank < 3 ? 'mc-text-gold mc-glow-gold' : ''}>
                  #{userHolderRank + 1} of {holdersData?.length || 0} players
                </span>
              ) : (
                <span className="mc-text-muted">Unranked — earn PP to climb</span>
              )}
            </div>
          </div>
        </div>
        <div className="text-right">
          <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
          <div className="text-xs mc-text-muted">PP</div>
        </div>
      </div>

      <div className="mc-card-elevated">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Holders */}
          <div>
            <div className="flex items-center gap-2 mb-1">
              <Trophy className="h-4 w-4 mc-text-gold" />
              <h3 className="font-display text-base mc-text-primary">Top Holders</h3>
            </div>
            <p className="text-xs mc-text-muted mb-4">Most Points Accumulated</p>
            {holdersData && holdersData.length >= 3 && (
              <Podium entries={holdersData} isHolders={true} />
            )}
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {holdersData && holdersData.length > 0
                ? holdersData.slice(3).map(e => renderEntry(e, true))
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
            {burnersData && burnersData.length >= 3 && (
              <Podium entries={burnersData} isHolders={false} />
            )}
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {burnersData && burnersData.length > 0
                ? burnersData.slice(3).map(e => renderEntry(e, false))
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
    </div>
  );
}
