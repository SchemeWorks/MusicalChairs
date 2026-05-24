import React from 'react';
import { useGetPonziPoints, useGetShenaniganStats, useGetKarmaReceived } from '../../hooks/useQueries';
import { useWallet } from '../../hooks/useWallet';
import { isCharles } from '../../lib/charles';
import type { HallOfFameEntry } from './PodiumCard';

interface PinnedYouLineProps {
  burners: HallOfFameEntry[];
  variant?: 'rail' | 'mobile';
}

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
