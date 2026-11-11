import { useICPBalance, useGetUserGames, useGetPonziPoints, useGetGameStats, useGetUserSolGames, useSolBalance, useGetSolGameStats } from '../hooks/useQueries';
import { useNativeAvailability, useNativePortfolio } from '../solana/native/panelsHooks';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { useWallet } from '../hooks/useWallet';
import { formatICP } from '../lib/formatICP';
import { formatSolFloat, formatSOL } from '../solana/lamports';
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
  const { data: solBalance } = useSolBalance();
  const { data: solGameStats } = useGetSolGameStats();
  const portfolio = useLivePortfolio(games);
  const solPortfolio = useLivePortfolio(solGames);
  // Native rail figures (the Musical Chairs program) sit NEXT TO the legacy canister ones and are
  // never summed with them: separate AUM (= pot) stats, separate position counts.
  const nativeAvailability = useNativeAvailability();
  const nativePortfolio = useNativePortfolio();
  const nativePool = isSiws && nativeAvailability.state === 'native' ? nativeAvailability.pool : null;

  // SIWS sessions are SOL-denominated; everyone else is ICP. Every per-user and
  // protocol figure follows the active wallet so the bar never contradicts the
  // Profit Center's running tally.
  const ponziPoints = ponziData?.totalPoints || 0;
  const balance = isSiws ? (solBalance ?? 0) : (icpBalance ?? 0);
  const potBalance = isSiws ? (solGameStats?.potBalance ?? 0) : (gameStats?.potBalance || 0);
  const activeGames = isSiws ? solGames.length : (games?.length || 0);
  const nativeNetPL = (Number(nativePortfolio.totalEarned) - Number(nativePortfolio.totalDeposits)) / 1_000_000_000;
  // P/L follows the default rail, matching the Profit Center's running tally.
  const netPL = nativePool
    ? nativeNetPL
    : isSiws
      ? solPortfolio.totalEarnings - solPortfolio.totalDeposits
      : portfolio.totalEarnings - portfolio.totalDeposits;
  const isUp = netPL >= 0;
  const unit = isSiws ? 'SOL' : 'ICP';
  // Format a non-negative amount in the active denomination.
  const fmtAmt = (v: number) => (isSiws ? formatSolFloat(v) : formatICP(v));
  const netPLDisplay = isSiws
    ? `${isUp ? '+' : '-'}${formatSolFloat(Math.abs(netPL))}`
    : `${isUp ? '+' : ''}${formatICP(netPL)}`;

  return (
    <div className="mc-status-bar">
      {/* Balance */}
      <div className="mc-status-bar-stat">
        <span className="mc-status-bar-label">Balance</span>
        <span className="mc-status-bar-value mc-text-primary">{fmtAmt(balance)}</span>
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
        <span className="mc-status-bar-value mc-text-cyan">
          {nativePool ? (
            <>
              {nativePortfolio.active.length}
              <span className="mc-text-muted text-[10px]"> native</span>
              {activeGames > 0 && (
                <>
                  {' · '}{activeGames}
                  <span className="mc-text-muted text-[10px]"> legacy</span>
                </>
              )}
            </>
          ) : (
            activeGames
          )}
        </span>
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

      {/* AUM — compact format on mobile (1 decimal), full precision on desktop.
          With the native rail on, AUM is the native pool's pot and the legacy canister's AUM is its own stat. */}
      {nativePool && (
        <div className="mc-status-bar-stat">
          <span className="mc-status-bar-label">AUM</span>
          <span className="mc-status-bar-value mc-text-gold" data-testid="native-aum">
            <span className="sm:hidden">{nativePool.pot >= 10_000_000_000n ? (Number(nativePool.pot) / 1e9).toFixed(1) : formatSOL(nativePool.pot)}</span>
            <span className="hidden sm:inline">{formatSOL(nativePool.pot)} SOL</span>
          </span>
        </div>
      )}
      <div className={`mc-status-bar-stat ${nativePool ? 'mc-status-bar-mobile-hide' : ''}`}>
        <span className="mc-status-bar-label">{nativePool ? 'Legacy AUM' : 'AUM'}</span>
        <span className="mc-status-bar-value mc-text-gold">
          <span className="sm:hidden">{potBalance >= 10 ? potBalance.toFixed(1) : fmtAmt(potBalance)}</span>
          <span className="hidden sm:inline">{fmtAmt(potBalance)} {unit}</span>
        </span>
      </div>
    </div>
  );
}
