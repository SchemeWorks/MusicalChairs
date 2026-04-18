import { useICPBalance, useGetUserGames, useGetPonziPoints, useGetGameStats } from '../hooks/useQueries';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { formatICP } from '../lib/formatICP';
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
  const portfolio = useLivePortfolio(games);

  const activeGames = games?.length || 0;
  const ponziPoints = ponziData?.totalPoints || 0;
  const potBalance = gameStats?.potBalance || 0;
  const netPL = portfolio.totalEarnings - portfolio.totalDeposits;
  const isUp = netPL >= 0;

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
            {isUp ? '+' : ''}{formatICP(netPL)}
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

      {/* Pot — desktop only */}
      <div className="mc-status-bar-stat mc-status-bar-desktop">
        <span className="mc-status-bar-label">Pot</span>
        <span className="mc-status-bar-value mc-text-gold">{formatICP(potBalance)} ICP</span>
      </div>
    </div>
  );
}
