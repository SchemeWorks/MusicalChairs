import { useState, useEffect, useRef } from 'react';
import { useGetUserGames, useWithdrawGameEarnings, useSettleCompoundingGame, isCompoundingPlanUnlocked, getTimeRemaining, useGetPonziPoints, useGetShenaniganConfigs, useGetMintConfig, useGetUserSolGames } from '../hooks/useQueries';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { useWallet } from '../hooks/useWallet';
import { formatSOL } from '../solana/lamports';
import { GameRecord, GamePlan, SolGameRecord } from '../backend';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';
import {
  EXIT_TOLL_EARLY,
  EXIT_TOLL_MID,
  EXIT_TOLL_LATE,
  EXIT_TOLL_EARLY_DAYS,
  EXIT_TOLL_MID_DAYS,
  JACKPOT_FEE_RATE_15D,
  JACKPOT_FEE_RATE_30D,
  pctPrecise,
} from '../lib/gameConstants';
import { Lock, ArrowDownCircle, Rocket, TrendingUp, TrendingDown, Dice5, ChevronDown } from 'lucide-react';
import type { TabType } from '../App';
import MobileSheet from './MobileSheet';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';

/* ================================================================
   Constants
   ================================================================ */

function getPlanName(plan: any): string {
  if ('simple21Day' in plan) return '21-Day Simple';
  if ('compounding15Day' in plan) return '15-Day Compounding';
  if ('compounding30Day' in plan) return '30-Day Compounding';
  return 'Unknown Plan';
}

function getPlanAccent(plan: any): string {
  if ('simple21Day' in plan) return 'mc-plan-simple';
  if ('compounding15Day' in plan) return 'mc-plan-compound';
  if ('compounding30Day' in plan) return 'mc-plan-compound';
  return '';
}

// Rotating quotes attributed to Charles — shown in empty state
const charlesQuotes = [
  "You're either allocating, or you're watching someone else's allocation work.",
  "You can sit on the sidelines. That's a decision too.",
  "I can't guarantee returns. I can guarantee you won't see this entry price again.",
  "I'm allocating my own capital. I wouldn't put you into something I'm not in.",
  "The window doesn't close slowly. It just closes.",
  "Everyone who got in early said it felt too early.",
  "I don't need you in this. I'm offering because I like you.",
  "Risk is what happens when you don't have information. You have information.",
  "You're not early. But you're not late.",
];

/* ================================================================
   Helpers
   ================================================================ */

/** Format ICP for display — full 8 decimal precision */
const formatICPDisplay = (value: number): string => {
  if (isNaN(value) || !isFinite(value)) return '0.00000000';
  return value.toFixed(8);
};

const formatDate = (ts: bigint) =>
  new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1_000_000));

const daysActive = (ts: bigint) => Math.floor((Date.now() - Number(ts) / 1_000_000) / 86_400_000);

function getExitTollInfo(game: GameRecord) {
  const startTime = Number(game.startTime) / 1_000_000;
  const elapsed = Date.now() - startTime;
  const days = elapsed / 86_400_000;
  if (game.isCompounding) {
    const fee = 'compounding15Day' in game.plan ? JACKPOT_FEE_RATE_15D * 100 : JACKPOT_FEE_RATE_30D * 100;
    return { currentFee: fee, nextFee: null, timeToNext: null };
  }
  if (days < EXIT_TOLL_EARLY_DAYS) return { currentFee: EXIT_TOLL_EARLY * 100, nextFee: EXIT_TOLL_MID * 100, timeToNext: (EXIT_TOLL_EARLY_DAYS * 86_400_000) - elapsed };
  if (days < EXIT_TOLL_MID_DAYS) return { currentFee: EXIT_TOLL_MID * 100, nextFee: EXIT_TOLL_LATE * 100, timeToNext: (EXIT_TOLL_MID_DAYS * 86_400_000) - elapsed };
  return { currentFee: EXIT_TOLL_LATE * 100, nextFee: null, timeToNext: null };
}

function fmtCountdown(ms: number) {
  if (ms <= 0) return '';
  const d = Math.floor(ms / 86_400_000);
  const h = Math.floor((ms % 86_400_000) / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  return `${d}d ${h}h ${m}m`;
}

function getPlanDuration(game: GameRecord): number {
  if ('compounding15Day' in game.plan) return 15;
  if ('compounding30Day' in game.plan) return 30;
  return 21;
}

function getTollBadgeClasses(fee: number): string {
  // Compounding fees get purple regardless of magnitude
  if (fee === JACKPOT_FEE_RATE_15D * 100 || fee === JACKPOT_FEE_RATE_30D * 100) {
    return 'bg-[var(--mc-purple)]/20 mc-text-purple';
  }
  if (fee <= EXIT_TOLL_LATE * 100) return 'bg-[var(--mc-neon-green)]/20 mc-text-green';
  if (fee <= EXIT_TOLL_MID * 100) return 'bg-[var(--mc-gold)]/20 mc-text-gold';
  return 'bg-[var(--mc-danger)]/20 mc-text-danger';
}

function getPositionUrgency(game: GameRecord): number {
  const planDays = getPlanDuration(game);
  const days = daysActive(game.startTime);
  const remaining = planDays - days;
  if (game.isCompounding && remaining > 0 && remaining <= 3) return 0; // near unlock → highest
  const tollInfo = getExitTollInfo(game);
  if (tollInfo.currentFee >= EXIT_TOLL_EARLY * 100) return 1; // high toll (early simple tier)
  return 2 + days; // everything else by age, oldest first
}

/* ================================================================
   Live Position Card
   ================================================================ */

function PositionCard({
  game,
  earnings,
  onWithdraw,
  withdrawPending,
}: {
  game: GameRecord;
  earnings: number;
  onWithdraw: (game: GameRecord) => void;
  withdrawPending: boolean;
}) {
  const name = getPlanName(game.plan);
  const accent = getPlanAccent(game.plan);
  const unlocked = isCompoundingPlanUnlocked(game);
  const canWithdraw = !game.isCompounding || unlocked;
  const hasEarnings = earnings > 0;
  // A matured Simple position with 0 earnings can still be closed (backend marks inactive)
  const isMaturedSimpleClose = !game.isCompounding && daysActive(game.startTime) >= getPlanDuration(game) && !hasEarnings;
  const buttonEnabled = canWithdraw && (hasEarnings || isMaturedSimpleClose);
  const timeRem = getTimeRemaining(game);
  const tollInfo = getExitTollInfo(game);

  return (
    <div className={`mc-card ${accent} p-4 transition-all duration-200 hover:translate-y-[-2px] hover:shadow-lg`}>
      {/* Top row: plan name + badge + days active */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="font-bold mc-text-primary">{name}</span>
          <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
            game.isCompounding ? 'bg-[var(--mc-purple)]/20 mc-text-purple' : 'bg-[var(--mc-neon-green)]/20 mc-text-green'
          }`}>
            {game.isCompounding ? 'Compounding' : 'Simple'}
          </span>
        </div>
        <span className="text-xs mc-text-muted">{daysActive(game.startTime)}d active</span>
      </div>

      {/* Numbers row: deposit | live earnings | carried interest */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-3">
        <div>
          <div className="mc-label">Deposit</div>
          <div className="text-base font-bold mc-text-primary">{formatICP(game.amount)} ICP</div>
          <div className="text-xs mc-text-muted">{formatDate(game.startTime)}</div>
        </div>
        <div className="text-center">
          <div className="mc-label">Earnings</div>
          <div className="text-lg sm:text-xl font-bold mc-text-green mc-glow-green">
            {formatICP(earnings)} ICP
          </div>
          <div className="text-xs mc-text-muted">{game.isCompounding ? 'before carry' : 'live'}</div>
        </div>
        <div className="text-right">
          <div className="mc-label">Carried Interest</div>
          <div className={`inline-block text-sm font-bold px-2 py-0.5 rounded-full ${getTollBadgeClasses(tollInfo.currentFee)}`}>
            {tollInfo.currentFee}%
          </div>
          {tollInfo.nextFee && tollInfo.timeToNext && (
            <div className="text-xs mc-text-cyan mt-0.5">{fmtCountdown(tollInfo.timeToNext)} to {tollInfo.nextFee}%</div>
          )}
        </div>
      </div>

      {/* Progress bar + Withdraw button side by side */}
      <div className="flex items-end gap-3">
        {/* Progress bar — left side, fills remaining space */}
        {(() => {
          const planDays = getPlanDuration(game);
          const days = daysActive(game.startTime);
          const pct = Math.min(100, Math.round((days / planDays) * 100));
          const indicatorColor = game.isCompounding ? 'mc-bg-purple' : 'mc-bg-green';
          return (
            <div className="flex-1">
              <div className="flex justify-between text-xs mc-text-muted mb-1">
                <span>Day {days} / {planDays}</span>
                <span>{pct}%</span>
              </div>
              <Progress value={pct} className="h-1.5 bg-white/5" indicatorClassName={indicatorColor} />
            </div>
          );
        })()}

        {/* Withdraw button — right-aligned, compact */}
        <button
          onClick={() => onWithdraw(game)}
          disabled={withdrawPending || !buttonEnabled}
          className={`px-4 py-2 rounded-lg text-xs font-bold uppercase transition-all whitespace-nowrap ${
            buttonEnabled
              ? 'mc-btn-primary'
              : 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5'
          }`}
          title={!canWithdraw ? 'Locked until maturity' : isMaturedSimpleClose ? 'Close matured position' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
        >
          {!canWithdraw ? (
            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h {timeRem.minutes}m</span>
          ) : isMaturedSimpleClose ? (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Close</span>
          ) : (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Withdraw</span>
          )}
        </button>
      </div>
    </div>
  );
}

/* ================================================================
   Empty State with rotating Charles quotes
   ================================================================ */

function EmptyState({ onNavigate }: { onNavigate?: () => void }) {
  const [quoteIndex] = useState(() => Math.floor(Math.random() * charlesQuotes.length));

  return (
    <div className="text-center py-16">
      <div className="mb-8 flex flex-col items-center justify-center">
        <p className="font-accent text-sm mc-text-dim italic max-w-md mx-auto leading-relaxed">
          {charlesQuotes[quoteIndex]}
        </p>
      </div>
      <button onClick={onNavigate} className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2">
        <Rocket className="h-4 w-4" /> Pick Your Plan
      </button>
    </div>
  );
}

/* ================================================================
   Main Export
   ================================================================ */

interface GameTrackingProps {
  onNavigateToGameSetup?: () => void;
  onTabChange?: (tab: TabType) => void;
  visible?: boolean;
}

export default function GameTracking({ onNavigateToGameSetup, onTabChange, visible = true }: GameTrackingProps) {
  const { data: games, isLoading, error } = useGetUserGames();
  const { walletType } = useWallet();
  const solGamesQuery = useGetUserSolGames();
  const solGames: SolGameRecord[] = walletType === 'siws' ? (solGamesQuery.data ?? []) : [];
  const { data: ponziData } = useGetPonziPoints();
  const { data: shenaniganConfigs } = useGetShenaniganConfigs();
  const { data: mintConfig } = useGetMintConfig();
  const simplePpPerIcp = mintConfig ? Number(mintConfig.simple21DayPpPerIcp) : 0;
  const comp15PpPerIcp = mintConfig ? Number(mintConfig.compounding15DayPpPerIcp) : 0;
  const comp30PpPerIcp = mintConfig ? Number(mintConfig.compounding30DayPpPerIcp) : 0;
  const portfolio = useLivePortfolio(games);
  const withdrawMutation = useWithdrawGameEarnings();
  const settleMutation = useSettleCompoundingGame();
  const netPL = portfolio.totalEarnings - portfolio.totalDeposits;

  // Collapse state — both sections start closed
  const [feesExpanded, setFeesExpanded] = useState(false);
  const [ppExpanded, setPpExpanded] = useState(false);

  // Live values — no countUp animation since portfolio updates every second
  // and countUp fights with the live tick, causing erratic jumps
  const [withdrawDialogOpen, setWithdrawDialogOpen] = useState(false);
  const [reinvestDialogOpen, setReinvestDialogOpen] = useState(false);
  const [selectedGame, setSelectedGame] = useState<GameRecord | null>(null);
  const [withdrawnAmount, setWithdrawnAmount] = useState(0);
  const [countdown, setCountdown] = useState('');

  const handleWithdrawClick = (game: GameRecord) => {
    setSelectedGame(game);
    setWithdrawDialogOpen(true);
  };

  const handleWithdrawConfirm = async () => {
    if (!selectedGame) return;
    try {
      if (selectedGame.isCompounding) {
        const result = await settleMutation.mutateAsync(selectedGame.id);
        setWithdrawnAmount(result.earnings);
      } else {
        const result = await withdrawMutation.mutateAsync(selectedGame.id);
        setWithdrawnAmount(result.earnings);
      }
      setWithdrawDialogOpen(false);
      setSelectedGame(null);
      triggerConfetti();
      setReinvestDialogOpen(true);
    } catch (err) {
      console.error('Withdrawal failed:', err);
    }
  };

  // Live countdown for withdrawal dialog
  useEffect(() => {
    if (!withdrawDialogOpen || !selectedGame) return;
    const update = () => {
      const info = getExitTollInfo(selectedGame);
      if (info.timeToNext) setCountdown(fmtCountdown(info.timeToNext));
    };
    update();
    const iv = setInterval(update, 1000);
    return () => clearInterval(iv);
  }, [withdrawDialogOpen, selectedGame]);

  if (isLoading) return <LoadingSpinner />;

  if (error) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        <p className="font-accent italic mb-1">Even Charles couldn't fix this one.</p>
        Unable to load profit center data. Please try again later.
      </div>
    );
  }

  const hasGames = games && games.length > 0;

  return (
    <>
      <div className="space-y-6">
        {/* Running Tally — hero P/L card */}
        <div className="mc-card-elevated text-center">
          <div className="flex items-center justify-between mb-3">
            <h2 className="font-display text-lg mc-text-primary">Your Running Tally</h2>
            <span className="inline-flex items-center gap-1.5 text-xs mc-text-muted">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75" />
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
              </span>
              LIVE
            </span>
          </div>
          {/* Hero P/L number — animated countUp */}
          <div className="mb-4">
            <div className="mc-label mb-1">Net P/L</div>
            <div className={`text-2xl sm:text-4xl font-bold flex items-center justify-center gap-2 ${
              netPL >= 0 ? 'mc-text-green mc-glow-green' : 'mc-text-danger'
            }`}>
              {netPL >= 0 ? <TrendingUp className="h-6 w-6" /> : <TrendingDown className="h-6 w-6" />}
              {netPL >= 0 ? '+' : ''}{formatICPDisplay(netPL)} ICP
            </div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Deposited</div>
              <div className="text-xl font-bold mc-text-primary">{formatICPDisplay(portfolio.totalDeposits)} ICP</div>
            </div>
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Earned</div>
              <div className="text-xl font-bold mc-text-green mc-glow-green">{formatICPDisplay(portfolio.totalEarnings)} ICP</div>
            </div>
          </div>
        </div>

        {/* Positions */}
        <div className="mc-card-elevated">
          <h2 className="font-display text-lg mc-text-primary mb-4">Your Positions</h2>

          {!hasGames ? (
            <EmptyState onNavigate={onNavigateToGameSetup} />
          ) : (
            <div className="space-y-3">
              {[...portfolio.games].sort((a, b) => getPositionUrgency(a.game) - getPositionUrgency(b.game)).map(({ game, earnings }) => (
                <PositionCard
                  key={game.id.toString()}
                  game={game}
                  earnings={earnings}
                  onWithdraw={handleWithdrawClick}
                  withdrawPending={withdrawMutation.isPending || settleMutation.isPending}
                />
              ))}
            </div>
          )}

          {(withdrawMutation.isError || settleMutation.isError) && (
            <div className="mc-status-red p-3 mt-3 text-center text-sm">
              {withdrawMutation.error?.message || settleMutation.error?.message || 'Withdrawal failed'}
            </div>
          )}
        </div>

        {/* SOL Positions (SIWS-authed users only) */}
        {walletType === 'siws' && solGames.length > 0 && (
          <div className="mc-card-elevated">
            <h2 className="font-display text-lg mc-text-primary mb-4">Your SOL Positions</h2>
            <div className="space-y-3">
              {solGames.map((game) => {
                // SolGameRecord shape: id (Nat), plan (variant), amount (Float SOL),
                // accumulatedEarnings (Float SOL), isCompounding (Bool), startTime (Int ns).
                const deposit = game.amount;
                const earnings = game.accumulatedEarnings;
                // Convert Float SOL → bigint lamports for formatSOL. Float precision loss
                // is acceptable for display since lamports are not user-input here.
                const toLamports = (sol: number) => BigInt(Math.round(sol * 1_000_000_000));
                return (
                  <div key={game.id.toString()} className="mc-card p-3">
                    <div className="flex items-center justify-between mb-2">
                      <span className="font-display text-sm mc-text-primary">
                        {getPlanName(game.plan)}
                      </span>
                      <span className="text-xs mc-text-muted">#{game.id.toString()}</span>
                    </div>
                    <div className="grid grid-cols-2 gap-2 text-xs">
                      <div>
                        <div className="mc-label">Deposited</div>
                        <div className="mc-text-primary font-bold">{formatSOL(toLamports(deposit))} SOL</div>
                      </div>
                      <div>
                        <div className="mc-label">Accrued</div>
                        <div className="mc-text-green font-bold">{formatSOL(toLamports(earnings))} SOL</div>
                      </div>
                    </div>
                    <div className="mt-3 flex justify-end">
                      <Button
                        disabled
                        size="sm"
                        variant="secondary"
                        title="SOL withdrawals coming soon — backend M1.5"
                      >
                        <ArrowDownCircle className="h-3 w-3 mr-1" /> Withdraw
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
            <div className="mt-3 text-[10px] mc-text-muted italic text-center">
              Withdrawals disabled in M2 — re-enabled once ponzi_math_sol.withdrawEarnings accepts a target address parameter (backend M1.5).
            </div>
          </div>
        )}

        {/* Fee disclosure */}
        <div className="mc-house-card">
          <button
            type="button"
            onClick={() => setFeesExpanded(v => !v)}
            className="w-full flex items-center justify-between text-left"
            aria-expanded={feesExpanded}
          >
            <span className="font-semibold mc-text-primary">Fee Disclosure</span>
            <ChevronDown className={`w-4 h-4 transition-transform ${feesExpanded ? 'rotate-180' : ''}`} />
          </button>
          {feesExpanded && (
            <div className="mt-3 text-sm mc-text-muted space-y-2">
              <p className="font-accent italic">A small reinvestment keeps the engine running — and your returns flowing.</p>
              <p>Simple positions: {pctPrecise(EXIT_TOLL_EARLY)} carried interest within {EXIT_TOLL_EARLY_DAYS} days, {pctPrecise(EXIT_TOLL_MID)} within {EXIT_TOLL_MID_DAYS} days, {pctPrecise(EXIT_TOLL_LATE)} after.</p>
              <p>Compounding plans: 9% carry (15-day) or 13% carry (30-day) at maturity.</p>
              <p>Your deposit is deployed into AUM to fund interest obligations. Your position is an entitlement to future interest from AUM.</p>
            </div>
          )}
        </div>

        {/* Ponzi Points section */}
        <div className="mc-card-elevated">
          <button
            type="button"
            onClick={() => setPpExpanded(v => !v)}
            className="w-full flex items-center justify-between text-left"
            aria-expanded={ppExpanded}
          >
            <span className="font-semibold mc-text-primary">
              Ponzi Points ({(ponziData?.totalPoints ?? 0).toLocaleString()})
            </span>
            <ChevronDown className={`w-4 h-4 transition-transform ${ppExpanded ? 'rotate-180' : ''}`} />
          </button>
          {ppExpanded && (
            <div className="mt-3">
              {/* PP balance */}
              <div className="text-center mb-4">
                <div className="mc-label mb-1">Your Balance</div>
                <div className="text-2xl font-bold mc-text-purple mc-glow-purple">
                  {(ponziData?.totalPoints || 0).toLocaleString()} PP
                </div>
              </div>

              {/* Earn rates comparison table */}
              <div className="mb-4">
                <div className="mc-label mb-2">Earn Rates</div>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2 text-center text-xs">
                  <div className="mc-card p-3">
                    <div className="mc-text-green font-bold text-sm">{simplePpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Simple 21-day</div>
                  </div>
                  <div className="mc-card p-3">
                    <div className="mc-text-purple font-bold text-sm">{comp15PpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Compound 15-day</div>
                  </div>
                  <div className="mc-card p-3">
                    <div className="mc-text-gold font-bold text-sm">{comp30PpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Compound 30-day</div>
                  </div>
                </div>
              </div>

              {/* PP custody breakdown */}
              {ponziData && (
                <div className="mb-4">
                  <div className="mc-label mb-2">Custody</div>
                  <div className="text-xs space-y-1.5">
                    <div className="flex justify-between">
                      <span className="mc-text-muted">Side pocket (spendable)</span>
                      <span className="mc-text-green font-bold">{(ponziData.chipPoints || 0).toLocaleString()} PP</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="mc-text-muted">Wallet (external)</span>
                      <span className="mc-text-cyan font-bold">{(ponziData.walletPoints || 0).toLocaleString()} PP</span>
                    </div>
                  </div>
                </div>
              )}

              {/* Spending suggestions — affordability uses costSuccess
                  (the upfront commitment). A worse outcome may charge more
                  but the backend clamps to balance if they're short. */}
              {(() => {
                const pp = ponziData?.totalPoints || 0;
                if (pp < 100 || !shenaniganConfigs) return null;
                const affordable = shenaniganConfigs
                  .filter(c => Number(c.costSuccess) <= pp)
                  .sort((a, b) => Number(a.costSuccess) - Number(b.costSuccess))
                  .slice(0, 3);
                if (affordable.length === 0) return null;
                return (
                  <div>
                    <div className="mc-label mb-2">You can afford</div>
                    <div className="flex flex-wrap gap-2">
                      {affordable.map(s => (
                        <span key={s.name} className="text-xs mc-card px-2 py-1">
                          {s.name} <span className="mc-text-purple">({Number(s.costSuccess).toLocaleString()} PP)</span>
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })()}
            </div>
          )}
        </div>

        {/* Spend PP bridge CTA */}
        {(ponziData?.totalPoints || 0) >= 100 && onTabChange && (
          <div className="text-center">
            <button
              onClick={() => onTabChange('shenanigans')}
              className="mc-btn-secondary inline-flex items-center gap-2 mx-auto text-xs px-4 py-2 rounded-lg"
            >
              <Dice5 className="h-4 w-4 mc-text-purple" />
              Spend your {ponziData?.totalPoints?.toLocaleString()} PP on Shenanigans →
            </button>
          </div>
        )}
      </div>

      {/* Withdrawal Dialog — MobileSheet for bottom sheet on mobile */}
      <MobileSheet open={withdrawDialogOpen} onOpenChange={setWithdrawDialogOpen}>
        <div className="space-y-4">
          <h2 className="font-display text-lg mc-text-primary">Confirm Withdrawal</h2>
          <p className="mc-text-dim text-sm">
            {selectedGame && (() => {
              const info = getExitTollInfo(selectedGame);
              if (info.nextFee && info.timeToNext) {
                return (
                  <span>
                    You'll pay <strong className="mc-text-gold">{info.currentFee}%</strong> carried interest.
                    Wait <strong className="mc-text-cyan">{countdown}</strong> to reduce it to <strong className="mc-text-green">{info.nextFee}%</strong>.
                  </span>
                );
              }
              return <span>You'll pay <strong className="mc-text-gold">{info.currentFee}%</strong> carried interest.</span>;
            })()}
          </p>
          <div className="flex gap-2 justify-end">
            <Button variant="outline" onClick={() => setWithdrawDialogOpen(false)} className="mc-btn-secondary">
              Cancel
            </Button>
            <Button onClick={handleWithdrawConfirm} disabled={withdrawMutation.isPending || settleMutation.isPending} className="mc-btn-primary">
              {(withdrawMutation.isPending || settleMutation.isPending) ? 'Processing...' : 'Confirm'}
            </Button>
          </div>
        </div>
      </MobileSheet>

      {/* Post-withdrawal celebration toast */}
      {reinvestDialogOpen && (
        <div className="fixed inset-0 z-[9999] flex items-center justify-center p-4">
          <div className="mc-toast text-center">
            <div className="font-display text-xl mc-text-primary mb-2">Congratulations!</div>
            <p className="text-sm mc-text-dim mb-1">
              This scheme earned you{' '}
              <span className="mc-toast-accent">{formatICP(withdrawnAmount)} ICP</span>
            </p>
            <p className="text-xs mc-text-muted mb-4">Want to grow it? Reinvest now.</p>
            <div className="flex gap-3 justify-center">
              <button
                onClick={() => setReinvestDialogOpen(false)}
                className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
              >
                Nah
              </button>
              <button
                onClick={() => { setReinvestDialogOpen(false); onNavigateToGameSetup?.(); }}
                className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2"
              >
                <Rocket className="h-4 w-4" /> YOLO Again
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
