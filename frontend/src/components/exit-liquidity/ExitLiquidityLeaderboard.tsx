import { Crown } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetExitLiquidityLeaderboard, useGetCurrentRoundId } from '../../hooks/useQueries';
import { GoldenNameByPrincipal } from '../GoldenName';
import { bpsToMultiplier } from '../../lib/exitLiquidity';

export default function ExitLiquidityLeaderboard() {
  const { data: roundId } = useGetCurrentRoundId();
  const { data: rows = [] } = useGetExitLiquidityLeaderboard(
    roundId !== undefined ? Number(roundId) : undefined, 25,
  );

  return (
    <div className="mc-card-elevated p-4 max-w-md mx-auto mt-6">
      <h3 className="font-display mc-text-primary mb-3">The Cap Table</h3>
      {rows.length === 0 ? (
        <p className="text-sm mc-text-dim">No qualified players yet this round. Be the first to survive 25 runs.</p>
      ) : (
        <ol className="space-y-2">
          {rows.map(([p, scoreBps]: [Principal, bigint], i: number) => (
            <li key={p.toText()} className="flex items-center justify-between text-sm">
              <span className="flex items-center gap-2 min-w-0">
                <span className="mc-text-dim w-5 text-right">{i + 1}</span>
                {i === 0 && <Crown className="h-4 w-4 mc-text-gold flex-shrink-0" aria-label="Champion" />}
                <GoldenNameByPrincipal principal={p} className="truncate max-w-[160px]" />
              </span>
              <span className="font-display mc-text-gold">{bpsToMultiplier(scoreBps)}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
