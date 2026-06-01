import { useICPBalance, useGetUserGames, useGetPonziPoints, useGetGameStats, useGetUserSolGames } from '../hooks/useQueries';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { useWallet } from '../hooks/useWallet';
import { formatICP } from '../lib/formatICP';
import { formatSolFloat } from '../solana/lamports';
import { SolGameRecord } from '../backend';
import { TrendingUp, TrendingDown } from 'lucide-react';

type NavigableTab = 'profitCenter' | 'shenanigans';

interface GameStatusBarProps {
  onNavigate?: (tab: NavigableTab) => void;
}

export default function GameStatusBar({ onNavigate }: GameStatusBarProps) {
  const { data: icpBalance } = useICPBalance();
  const { data: games } = useGetUserGames();
  const { data: ponziData } = useGetPonziPoints();
  const { data: gameStats } = useGetGameStats();
  const { walletType } = useWallet();
  const solGamesQuery = useGetUserSolGames();
  const isSiws = walletType === 'siws';
  const solGames: SolGameRecord[] = isSiws ? (solGamesQuery.data ?? []) : [];
  const portfolio = useLivePortfolio(games);
  const solPortfolio = useLivePortfolio(solGames);

  // SIWS sessions are SOL-denominated; everyone else is ICP. P/L and the
  // position count follow the active wallet so they don't contradict the
  // Profit Center's running tally. (Balance + AUM remain ICP figures.)
  const ponziPoints = ponziData?.totalPoints || 0;
  const potBalance = gameStats?.potBalance || 0;
  const activeGames = isSiws ? solGames.length : (games?.length || 0);
  const netPL = isSiws
    ? solPortfolio.totalEarnings - solPortfolio.totalDeposits
    : portfolio.totalEarnings - portfolio.totalDeposits;
  const isUp = netPL >= 0;
  const netPLDisplay = isSiws
    ? `${isUp ? '+' : '-'}${formatSolFloat(Math.abs(netPL))}`
    : `${isUp ? '+' : ''}${formatICP(netPL)}`;

  return (
    <div className="mc-status-bar">
      {/* Balance */}
      <div className="mc-status-bar-stat">
        <span className="mc-status-bar-label">Balance</span>
        <span className="mc-status-bar-value mc-text-primary">{formatICP(icpBalance ?? 0)}</span>
      </div>

      {/* Net P/L — hero stat */}
      <div className="mc-status-bar-stat">
        <span className="mc-status-bar-label">P/L</span>
        <button
          type="button"
          onClick={() => onNavigate?.('profitCenter')}
          className="text-left hover:mc-bg-elev-2 rounded px-2 py-1 -mx-2 -my-1 transition-colors"
          aria-label="Go to Profit Center"
        >
          <span className={`mc-status-bar-value ${isUp ? 'mc-text-green mc-glow-green' : 'mc-text-danger'}`}>
            {isUp ? <TrendingUp className="h-3 w-3 inline mr-0.5" /> : <TrendingDown className="h-3 w-3 inline mr-0.5" />}
            {netPLDisplay}
          </span>
        </button>
      </div>

      {/* Positions — hidden on mobile to reduce density */}
      <div className="mc-status-bar-stat mc-status-bar-mobile-hide">
        <span className="mc-status-bar-label">Positions</span>
        <span className="mc-status-bar-value mc-text-cyan">{activeGames}</span>
      </div>

      {/* PP */}
      <div className="mc-status-bar-stat">
        <span className="mc-status-bar-label">PP</span>
        <button
          type="button"
          onClick={() => onNavigate?.('shenanigans')}
          className="text-left hover:mc-bg-elev-2 rounded px-2 py-1 -mx-2 -my-1 transition-colors"
          aria-label="Go to Shenanigans"
        >
          <span className="mc-status-bar-value mc-text-purple">{ponziPoints >= 1000 ? `${(ponziPoints / 1000).toFixed(1)}k` : ponziPoints.toLocaleString()}</span>
        </button>
      </div>

      {/* AUM — compact format on mobile (1 decimal), full precision on desktop */}
      <div className="mc-status-bar-stat">
        <span className="mc-status-bar-label">AUM</span>
        <span className="mc-status-bar-value mc-text-gold">
          <span className="sm:hidden">{potBalance >= 10 ? potBalance.toFixed(1) : formatICP(potBalance)}</span>
          <span className="hidden sm:inline">{formatICP(potBalance)} ICP</span>
        </span>
      </div>
    </div>
  );
}
