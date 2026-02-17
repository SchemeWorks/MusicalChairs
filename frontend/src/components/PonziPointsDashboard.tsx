import React from 'react';
import { useGetPonziPoints } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';

export default function PonziPointsDashboard() {
  const { data: ponziData, isLoading, error } = useGetPonziPoints();

  if (isLoading) return <LoadingSpinner />;

  if (error) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        Unable to load Ponzi Points data. Please try again later.
      </div>
    );
  }

  const totalPoints = ponziData?.totalPoints || 0;
  const pointsEarned = ponziData?.pointsEarned || 0;
  const pointsBurned = ponziData?.pointsBurned || 0;
  const referralPoints = ponziData?.referralPoints || 0;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Main balance */}
        <div className="text-center mb-6">
          <div className="mc-label mb-2">Your Ponzi Points Balance</div>
          <div className="text-2xl sm:text-4xl mc-text-purple mc-glow-purple font-display">
            {totalPoints.toLocaleString()} PP
          </div>
          <p className="text-xs mc-text-muted mt-2 font-accent italic">Worthless tokens for YOLOing into a Ponzi</p>
        </div>

        {/* Breakdown */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="mc-card mc-accent-green p-4 text-center">
            <div className="mc-label mb-1">Earned from Deposits</div>
            <div className="text-xl font-bold mc-text-green">{pointsEarned.toLocaleString()}</div>
          </div>
          <div className="mc-card mc-accent-pink p-4 text-center">
            <div className="mc-label mb-1">Burned on Shenanigans</div>
            <div className="text-xl font-bold mc-text-pink">{pointsBurned.toLocaleString()}</div>
          </div>
          <div className="mc-card mc-accent-cyan p-4 text-center">
            <div className="mc-label mb-1">From Referrals</div>
            <div className="text-xl font-bold mc-text-cyan">{referralPoints.toLocaleString()}</div>
          </div>
        </div>
      </div>

      {/* How to earn */}
      <div className="mc-card p-5">
        <h3 className="font-display text-base mc-text-primary mb-3">How to Earn</h3>
        <div className="text-sm mc-text-dim space-y-2 leading-relaxed">
          <p><span className="mc-text-green font-bold">Deposits:</span> 1,000 PP per ICP (Simple), 2,000 PP (15-day), 3,000 PP (30-day)</p>
          <p><span className="mc-text-cyan font-bold">Referrals:</span> Earn PP when your downline deposits</p>
          <p><span className="mc-text-gold font-bold">House Money:</span> 4,000 PP per ICP deposited as house money</p>
          <p><span className="mc-text-pink font-bold">Spend:</span> Burn PP on Shenanigans for chaos and glory</p>
        </div>
      </div>
    </div>
  );
}
