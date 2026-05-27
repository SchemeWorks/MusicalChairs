import React from 'react';
import { Medal, Trophy, Target, Gem, Shield, Heart } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetTopPonziPointsBurners, useGetRoundBurnedLeaderboard, useGetPonziPoints, useGetKarmaReceived, useGetCurrentRoundId } from '../hooks/useQueries';
import { useWallet } from '../hooks/useWallet';
import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';
import GoldenName from './GoldenName';
import LoadingSpinner from './LoadingSpinner';
import Podium from './hall-of-fame/Podium';
import type { HallOfFameEntry } from './hall-of-fame/PodiumCard';
import { CharlesIcon, isCharles } from '../lib/charles';

function LeaderboardRow({
  entry,
  isUser,
}: {
  entry: HallOfFameEntry;
  isUser: boolean;
}) {
  const principal = React.useMemo(() => Principal.fromText(entry.principal), [entry.principal]);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);

  const getRankStyle = (rank: number) => {
    switch (rank) {
      case 1: return { card: 'mc-rank-gold', icon: <Medal className="h-5 w-5 mc-text-gold" />, label: 'Gold' };
      case 2: return { card: 'mc-rank-silver', icon: <Medal className="h-5 w-5 text-gray-300" />, label: 'Silver' };
      case 3: return { card: 'mc-rank-bronze', icon: <Medal className="h-5 w-5 text-amber-600" />, label: 'Bronze' };
      default: return { card: 'mc-rank-default', icon: <Medal className="h-5 w-5 mc-text-purple" />, label: `#${rank}` };
    }
  };

  const style = getRankStyle(entry.rank);
  const isTop3 = entry.rank <= 3;
  const displayName = name || '…';
  // When not golden, the row name still respects the "(you)" cyan highlight.
  // When golden, <GoldenName> takes over the color/decoration regardless of
  // whether it's the current user.
  const fallbackClass = isUser ? 'mc-text-cyan' : 'mc-text-primary';

  return (
    <div
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
            isTop3 ? 'bg-white/10' : 'bg-[var(--mc-purple)]/10'
          } mc-text-dim`}>{style.label}</span>
          {isGolden ? (
            <GoldenName name={displayName} isGolden={true} className="font-bold text-sm ml-2" />
          ) : (
            <span className={`font-bold text-sm ml-2 ${fallbackClass}`}>
              {displayName}{isUser ? ' (you)' : ''}
            </span>
          )}
        </div>
      </div>
      <div className="text-right">
        <div className="text-lg font-bold mc-text-purple">{(entry.ponziPointsBurned || 0).toLocaleString()}</div>
        <div className="text-xs mc-text-muted">Burned</div>
      </div>
    </div>
  );
}

export default function HallOfFame() {
  const [filter, setFilter] = React.useState<'all-time' | 'this-round'>('all-time');
  const { data: allTimeBurnersData, isLoading: burnersLoading, error: burnersError } = useGetTopPonziPointsBurners();
  // Pass an explicit roundId from ponzi_math rather than letting shenanigans
  // fall back to its 30s-stale cached value — keeps the leaderboard pinned to
  // the live round even right after a reset.
  const { data: currentRoundId } = useGetCurrentRoundId();
  const { data: roundRaw } = useGetRoundBurnedLeaderboard(currentRoundId ?? undefined, 50);
  const { data: ponziData } = useGetPonziPoints();
  const { principal } = useWallet();
  const { data: karmaUnits } = useGetKarmaReceived(principal ?? undefined);
  const karmaPp = karmaUnits ? Number(karmaUnits / 100_000_000n) : 0;

  // Build this-round entries in the same shape as all-time entries
  const roundBurnersData = React.useMemo(() => {
    if (!roundRaw) return [];
    return roundRaw
      .filter(([p]) => !isCharles(p.toString()))
      .map(([p, unitsBig], index) => ({
        rank: index + 1,
        ponziPointsBurned: Number(unitsBig / 100_000_000n),
        principal: p.toString(),
      }));
  }, [roundRaw]);

  const burnersData = filter === 'this-round' ? roundBurnersData : allTimeBurnersData;

  if (burnersLoading) return <LoadingSpinner />;

  if (burnersError) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        Unable to load Hall of Fame data. Please try again later.
      </div>
    );
  }

  const hasData = allTimeBurnersData && allTimeBurnersData.length > 0;

  const userPrincipal = principal || '';
  const userBurnerRank = burnersData?.findIndex(e => e.principal === userPrincipal);
  const userPoints = ponziData?.totalPoints || 0;

  if (!hasData) {
    return (
      <div className="mc-card-elevated text-center py-10">
        <Trophy className="h-12 w-12 mc-text-gold mb-4 mx-auto" />
        <p className="font-display text-lg mc-text-primary mb-2">The Leaderboard Is Empty</p>
        <p className="text-sm mc-text-dim mb-4">Start burning Ponzi Points on Shenanigans to claim your spot.</p>
        <p className="font-accent text-xs mc-text-dim italic">
          Every empire starts with a first transaction.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Time filter toggle */}
      <div className="flex gap-2 justify-center">
        <button
          onClick={() => setFilter('this-round')}
          className={`px-4 py-2 rounded-full text-xs font-bold border transition-all ${
            filter === 'this-round'
              ? 'bg-[var(--mc-purple)]/25 mc-text-primary border-[var(--mc-purple)]/30'
              : 'mc-text-muted bg-white/5 border-white/10 hover:bg-white/10'
          }`}
        >
          This Round
        </button>
        <button
          onClick={() => setFilter('all-time')}
          className={`px-4 py-2 rounded-full text-xs font-bold border transition-all ${
            filter === 'all-time'
              ? 'bg-[var(--mc-purple)]/25 mc-text-primary border-[var(--mc-purple)]/30'
              : 'mc-text-muted bg-white/5 border-white/10 hover:bg-white/10'
          }`}
        >
          All Time
        </button>
      </div>

      {/* Your Rank banner — "House Status" variant for admins (Charles never
          appears on the public leaderboard, so showing them a rank wouldn't
          make sense). Layout mirrors the standard banner exactly to avoid
          page-shift between admin and non-admin views. */}
      {principal && isCharles(principal) ? (
        <div className="mc-card p-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <CharlesIcon className="h-5 w-5 mc-text-gold" />
            <div>
              <span className="text-xs mc-label">House Status</span>
              <div className="font-bold mc-text-primary text-sm">Not ranked</div>
              <div className="text-xs mc-text-muted italic">The house never plays its own table.</div>
            </div>
          </div>
          <div className="text-right">
            <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
            <div className="text-xs mc-text-muted">PP</div>
          </div>
        </div>
      ) : (
        <div className="mc-card p-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Target className="h-5 w-5 mc-text-cyan" />
            <div>
              <span className="text-xs mc-label">Your Rank (Diamond Tier)</span>
              <div className="font-bold mc-text-primary text-sm">
                {userBurnerRank !== undefined && userBurnerRank >= 0 ? (
                  <span className={userBurnerRank < 3 ? 'mc-text-gold mc-glow-gold' : ''}>
                    #{userBurnerRank + 1} of {burnersData?.length || 0} burners
                  </span>
                ) : (
                  <span className="mc-text-muted">Unranked — burn PP to climb</span>
                )}
              </div>
            </div>
          </div>
          <div className="text-right">
            <div className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()}</div>
            <div className="text-xs mc-text-muted">PP</div>
          </div>
        </div>
      )}

      {/* Karma received — prestige stat from trollbox karma reactions */}
      <div className="mc-card p-4 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Heart className="h-5 w-5 mc-text-pink" />
          <div>
            <span className="text-xs mc-label">Karma Received</span>
            <div className="text-xs mc-text-muted">PP tipped to you via 🔥 / 🚀 / 💀 reactions</div>
          </div>
        </div>
        <div className="text-right">
          <div className="text-lg font-bold mc-text-cyan">{karmaPp.toLocaleString()}</div>
          <div className="text-xs mc-text-muted">PP</div>
        </div>
      </div>

      <div className="mc-card-elevated">
        <div className="flex items-center gap-2 mb-1">
          <Gem className="h-4 w-4 mc-text-cyan" />
          <h3 className="font-display text-base mc-text-primary">Diamond Tier</h3>
        </div>
        <p className="text-xs mc-text-muted mb-4">Most Points Spent on Shenanigans</p>
        {burnersData && burnersData.length >= 1 && (
          <Podium entries={burnersData} />
        )}
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
          <span>All effects are limited to Ponzi Points and cosmetics only &mdash; never touching ICP, AUM, backer selection, payout math, or round structure.</span>
        </div>
      </div>
    </div>
  );
}
