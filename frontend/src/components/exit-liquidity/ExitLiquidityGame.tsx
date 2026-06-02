// frontend/src/components/exit-liquidity/ExitLiquidityGame.tsx
import { useState } from 'react';
import { toast } from 'sonner';
import { TrendingUp, Lock, Coins, Dice5 } from 'lucide-react';
import { useWallet } from '../../hooks/useWallet';
import {
  useGetExitLiquidityConfig, useGetActiveExitRun, useGetExitRunCount,
  useStartExitRun, useExitRunDecision,
} from '../../hooks/useQueries';
import { bpsToMultiplier, tellForStage, nextStageRewardBps } from '../../lib/exitLiquidity';
import { prettifyCanisterError } from '../../lib/errorMessages';
import { triggerConfetti } from '../ConfettiCanvas';
import type { ExitDecision, ExitRunResult } from '../../declarations/shenanigans/shenanigans.did';
import LoadingSpinner from '../LoadingSpinner';

const TELL_CLASS: Record<string, string> = {
  Calm: 'mc-text-green', Firm: 'mc-text-green', Choppy: 'mc-text-gold',
  Toppy: 'mc-text-gold', Critical: 'mc-text-danger',
};

export default function ExitLiquidityGame() {
  const { principal } = useWallet();
  const { data: config } = useGetExitLiquidityConfig();
  const { data: activeRun, isLoading } = useGetActiveExitRun(principal ?? undefined);
  const { data: runCount } = useGetExitRunCount(principal ?? undefined);
  const start = useStartExitRun();
  const decide = useExitRunDecision();
  const [lastResult, setLastResult] = useState<ExitRunResult | null>(null);

  const stageCount = config ? Number(config.stageCount) : 5;
  const windowSize = config ? Number(config.windowSize) : 25;
  const buyInPp = config ? Number(config.buyInUnits) / 1e8 : 0;

  const onStart = async () => {
    try { setLastResult(null); await start.mutateAsync(); }
    catch (e) { toast.error(prettifyCanisterError(e).message); }
  };

  const onDecide = async (decision: ExitDecision) => {
    try {
      const result = await decide.mutateAsync(decision);
      // Always record the latest result. The render keys off `activeRun`
      // (server-authoritative) and only shows the splash once the run is gone,
      // so an ongoing decision's result stays masked until the run truly ends.
      setLastResult(result);
      // Confetti only on an explicit winning Cash Out — never inferred from
      // `finalStage`, which can't distinguish "advanced to the last stage"
      // from "cleared it" (both report finalStage === stageCount, rotated=false).
      if (!result.rotated && 'exit' in decision && Number(result.runScoreBps) > 20000) {
        triggerConfetti();
      }
    } catch (e) { toast.error(prettifyCanisterError(e).message); }
  };

  if (isLoading) return <LoadingSpinner />;

  // ---- Result splash ----
  if (!activeRun && lastResult) {
    const r = lastResult;
    return (
      <div className="mc-card-elevated p-6 text-center max-w-md mx-auto">
        <h3 className={`font-display text-lg mb-2 ${r.rotated ? 'mc-text-danger' : 'mc-text-green'}`}>
          {r.rotated ? 'The music stopped. You were the exit liquidity.' : 'Clean exit.'}
        </h3>
        <p className="text-3xl font-display mc-text-primary my-3">{bpsToMultiplier(r.runScoreBps)}</p>
        <p className="text-sm mc-text-dim mb-4">
          {r.qualified
            ? `Best window this round: ${bpsToMultiplier(r.bestWindowAvgBps)}`
            : `${Number(runCount ?? 0n)}/${windowSize} runs to qualify for the cap table`}
        </p>
        <button className="mc-btn-primary w-full" onClick={onStart} disabled={start.isPending}>
          Commit Capital — {buyInPp} PP
        </button>
      </div>
    );
  }

  // ---- In-flight ----
  if (activeRun) {
    const stage = Number(activeRun.stage);
    const riding = Number(activeRun.ridingBps);
    const banked = Number(activeRun.bankedBps);
    const tell = tellForStage(stage, stageCount);
    const reward = config ? nextStageRewardBps(riding, Number(config.stageStepBps)) : riding;
    const busy = decide.isPending;
    return (
      <div className="mc-card-elevated p-6 max-w-md mx-auto">
        <div className="flex justify-between text-sm mb-4">
          <span className="mc-text-dim">Stage {stage}/{stageCount}</span>
          <span className={`font-bold ${TELL_CLASS[tell]}`}>{tell}</span>
        </div>
        <div className="grid grid-cols-2 gap-3 mb-4">
          <div className="text-center">
            <div className="flex items-center justify-center gap-1 mc-text-dim text-xs"><Lock className="h-3 w-3" /> Banked (safe)</div>
            <div className="font-display text-2xl mc-text-green">{bpsToMultiplier(banked)}</div>
          </div>
          <div className="text-center">
            <div className="flex items-center justify-center gap-1 mc-text-dim text-xs"><TrendingUp className="h-3 w-3" /> Riding</div>
            <div className="font-display text-2xl mc-text-gold">{bpsToMultiplier(riding)}</div>
          </div>
        </div>
        <p className="text-center text-xs mc-text-dim mb-4">Survive the next rotation → riding grows to ~{bpsToMultiplier(reward)}</p>
        <div className="grid grid-cols-3 gap-2">
          <button className="mc-btn-secondary" disabled={busy} onClick={() => onDecide({ bank: null } as ExitDecision)}>Take Distribution</button>
          <button className="mc-btn-secondary" disabled={busy} onClick={() => onDecide({ ride: null } as ExitDecision)}>Let It Ride</button>
          <button className="mc-btn-primary" disabled={busy} onClick={() => onDecide({ exit: null } as ExitDecision)}>Cash Out</button>
        </div>
      </div>
    );
  }

  // ---- Idle ----
  return (
    <div className="mc-card-elevated p-6 text-center max-w-md mx-auto">
      <Dice5 className="h-10 w-10 mc-text-green mx-auto mb-3" />
      <h3 className="font-display text-lg mc-text-primary mb-1">Exit Liquidity</h3>
      <p className="text-sm mc-text-dim mb-4">Ride the position, take distributions, cash out before the rotation. The only prize is the cap table.</p>
      <p className="text-xs mc-text-dim mb-4 flex items-center justify-center gap-1"><Coins className="h-3 w-3" /> {Number(runCount ?? 0n)}/{windowSize} runs to qualify this round</p>
      <button className="mc-btn-primary w-full" onClick={onStart} disabled={start.isPending || !principal}>
        Commit Capital — {buyInPp} PP
      </button>
    </div>
  );
}
