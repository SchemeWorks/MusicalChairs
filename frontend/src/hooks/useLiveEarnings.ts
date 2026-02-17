import { useState, useEffect, useRef } from 'react';
import { GameRecord } from '../backend';
import { getDailyRate, getTimeRemaining, isCompoundingPlanUnlocked } from './useQueries';

// Convert GamePlan enum to the string format getDailyRate expects
function getGamePlanString(plan: any): string {
  // GamePlan enum values from Candid
  if ('simple21Day' in plan) return '21-day-simple';
  if ('compounding15Day' in plan) return '15-day-compounding';
  if ('compounding30Day' in plan) return '30-day-compounding';
  return '21-day-simple';
}

/**
 * Pure deterministic earnings calculation from a GameRecord.
 * No backend calls — just math + Date.now().
 */
export function computeLiveEarnings(game: GameRecord): number {
  const startMs = Number(game.startTime) / 1_000_000;
  const elapsedDays = (Date.now() - startMs) / 86_400_000;
  if (elapsedDays <= 0) return 0;

  const dailyRate = getDailyRate(getGamePlanString(game.plan));

  if (game.isCompounding) {
    // Compounding: principal × [(1 + rate)^days - 1]
    return game.amount * (Math.pow(1 + dailyRate, elapsedDays) - 1);
  } else {
    // Simple: principal × rate × days
    return game.amount * dailyRate * elapsedDays;
  }
}

/**
 * Hook: live-updating earnings for a single game.
 * Ticks every second so the number visibly climbs.
 */
export function useLiveGameEarnings(game: GameRecord | null): number {
  const [earnings, setEarnings] = useState(() =>
    game ? computeLiveEarnings(game) : 0
  );

  useEffect(() => {
    if (!game) { setEarnings(0); return; }
    // Initial compute
    setEarnings(computeLiveEarnings(game));
    // Tick every second
    const iv = setInterval(() => setEarnings(computeLiveEarnings(game)), 1000);
    return () => clearInterval(iv);
  }, [game?.id?.toString(), game?.amount, game?.startTime?.toString()]);

  return earnings;
}

export interface LivePortfolio {
  totalDeposits: number;
  totalEarnings: number;
  games: Array<{ game: GameRecord; earnings: number }>;
}

/**
 * Hook: live-updating portfolio totals across all games.
 * Ticks every second. Returns per-game earnings + totals.
 */
export function useLivePortfolio(games: GameRecord[] | undefined): LivePortfolio {
  const [portfolio, setPortfolio] = useState<LivePortfolio>({
    totalDeposits: 0,
    totalEarnings: 0,
    games: [],
  });

  // Stable ref so the interval always sees latest games
  const gamesRef = useRef(games);
  gamesRef.current = games;

  useEffect(() => {
    const compute = () => {
      const g = gamesRef.current;
      if (!g || g.length === 0) {
        setPortfolio({ totalDeposits: 0, totalEarnings: 0, games: [] });
        return;
      }
      let totalDeposits = 0;
      let totalEarnings = 0;
      const mapped = g.map(game => {
        const earnings = computeLiveEarnings(game);
        totalDeposits += game.amount;
        totalEarnings += earnings;
        return { game, earnings };
      });
      setPortfolio({ totalDeposits, totalEarnings, games: mapped });
    };

    compute();
    const iv = setInterval(compute, 1000);
    return () => clearInterval(iv);
  }, [games?.length, games?.map(g => g.id.toString()).join(',')]);

  return portfolio;
}
