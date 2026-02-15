import { useState, useEffect } from 'react';
import { useGetUserGames, useWithdrawGameEarnings, isCompoundingPlanUnlocked, getTimeRemaining } from '../hooks/useQueries';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { useCountUp } from '../hooks/useCountUp';
import { GameRecord, GamePlan } from '../backend';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';
import { Lock, ArrowDownCircle, Rocket, TrendingUp, TrendingDown } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

/* ================================================================
   Constants
   ================================================================ */

const planNames: Record<string, string> = {
  [GamePlan.simple21Day]: '21-Day Simple',
  [GamePlan.compounding15Day]: '15-Day Compounding',
  [GamePlan.compounding30Day]: '30-Day Compounding',
};

const planAccents: Record<string, string> = {
  [GamePlan.simple21Day]: 'mc-plan-simple',
  [GamePlan.compounding15Day]: 'mc-plan-compound',
  [GamePlan.compounding30Day]: 'mc-plan-compound',
};

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

const formatDate = (ts: bigint) =>
  new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1_000_000));

const daysActive = (ts: bigint) => Math.floor((Date.now() - Number(ts) / 1_000_000) / 86_400_000);

function getExitTollInfo(game: GameRecord) {
  const startTime = Number(game.startTime) / 1_000_000;
  const elapsed = Date.now() - startTime;
  const days = elapsed / 86_400_000;
  if (game.isCompounding) return { currentFee: 13, nextFee: null, timeToNext: null };
  if (days < 3) return { currentFee: 7, nextFee: 5, timeToNext: (3 * 86_400_000) - elapsed };
  if (days < 10) return { currentFee: 5, nextFee: 3, timeToNext: (10 * 86_400_000) - elapsed };
  return { currentFee: 3, nextFee: null, timeToNext: null };
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
  if (fee <= 3) return 'bg-green-500/20 text-green-400';
  if (fee <= 5) return 'bg-yellow-500/20 text-yellow-400';
  if (fee <= 7) return 'bg-red-500/20 text-red-400';
  return 'bg-purple-500/20 text-purple-400'; // compounding 13%
}

function getPositionUrgency(game: GameRecord): number {
  const planDays = getPlanDuration(game);
  const days = daysActive(game.startTime);
  const remaining = planDays - days;
  if (game.isCompounding && remaining > 0 && remaining <= 3) return 0; // near unlock → highest
  const tollInfo = getExitTollInfo(game);
  if (tollInfo.currentFee >= 7) return 1; // high toll
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
  const name = planNames[game.plan] || 'Unknown Plan';
  const accent = planAccents[game.plan] || '';
  const unlocked = isCompoundingPlanUnlocked(game);
  const canWithdraw = !game.isCompounding || unlocked;
  const hasEarnings = earnings > 0;
  const timeRem = getTimeRemaining(game);
  const tollInfo = getExitTollInfo(game);

  return (
    <div className={`mc-card ${accent} p-4 transition-all duration-200 hover:translate-y-[-2px] hover:shadow-lg`}>
      {/* Top row: plan name + badge + days active */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="font-bold mc-text-primary">{name}</span>
          <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
            game.isCompounding ? 'bg-purple-500/20 mc-text-purple' : 'bg-green-500/20 mc-text-green'
          }`}>
            {game.isCompounding ? 'Compounding' : 'Simple'}
          </span>
        </div>
        <span className="text-xs mc-text-muted">{daysActive(game.startTime)}d active</span>
      </div>

      {/* Numbers row: deposit | live earnings | exit toll */}
      <div className="grid grid-cols-3 gap-3 mb-3">
        <div>
          <div className="mc-label">Deposit</div>
          <div className="text-base font-bold mc-text-primary">{formatICP(game.amount)} ICP</div>
          <div className="text-xs mc-text-muted">{formatDate(game.startTime)}</div>
        </div>
        <div className="text-center">
          <div className="mc-label">Earnings</div>
          <div className="text-base font-bold mc-text-green mc-glow-green">{formatICP(earnings)} ICP</div>
          <div className="text-xs mc-text-muted">live</div>
        </div>
        <div className="text-right">
          <div className="mc-label">Exit Toll</div>
          <div className={`inline-block text-sm font-bold px-2 py-0.5 rounded-full ${getTollBadgeClasses(tollInfo.currentFee)}`}>
            {tollInfo.currentFee}%
          </div>
          {tollInfo.nextFee && tollInfo.timeToNext && (
            <div className="text-xs mc-text-cyan mt-0.5">{fmtCountdown(tollInfo.timeToNext)} to {tollInfo.nextFee}%</div>
          )}
        </div>
      </div>

      {/* Progress bar — how far through the plan */}
      {(() => {
        const planDays = getPlanDuration(game);
        const days = daysActive(game.startTime);
        const pct = Math.min(100, Math.round((days / planDays) * 100));
        const barColor = game.isCompounding ? 'bg-purple-500' : 'bg-green-500';
        return (
          <div className="mb-3">
            <div className="flex justify-between text-xs mc-text-muted mb-1">
              <span>Day {days} / {planDays}</span>
              <span>{pct}%</span>
            </div>
            <div className="w-full h-1.5 rounded-full bg-white/5">
              <div className={`h-full rounded-full ${barColor} transition-all`} style={{ width: `${pct}%` }} />
            </div>
          </div>
        );
      })()}

      {/* Withdraw button */}
      <button
        onClick={() => onWithdraw(game)}
        disabled={withdrawPending || !canWithdraw || !hasEarnings}
        className={`w-full py-2 rounded-lg text-xs font-bold uppercase transition-all ${
          canWithdraw && hasEarnings
            ? 'mc-btn-primary'
            : 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5'
        }`}
        title={!canWithdraw ? 'Locked until maturity' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
      >
        {!canWithdraw ? (
          <span className="flex items-center justify-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h {timeRem.minutes}m</span>
        ) : (
          <span className="flex items-center justify-center gap-1"><ArrowDownCircle className="h-3 w-3" />Withdraw</span>
        )}
      </button>
    </div>
  );
}

/* ================================================================
   Empty State with rotating Charles quotes
   ================================================================ */

function EmptyState({ onNavigate }: { onNavigate?: () => void }) {
  const [quoteIndex, setQuoteIndex] = useState(() => Math.floor(Math.random() * charlesQuotes.length));
  const [fade, setFade] = useState(true);

  useEffect(() => {
    const iv = setInterval(() => {
      setFade(false);
      setTimeout(() => {
        setQuoteIndex(prev => (prev + 1) % charlesQuotes.length);
        setFade(true);
      }, 400);
    }, 6000);
    return () => clearInterval(iv);
  }, []);

  return (
    <div className="text-center py-16">
      <div className="mb-8 min-h-[80px] flex flex-col items-center justify-center">
        <p
          className={`font-accent text-sm mc-text-dim italic max-w-md mx-auto leading-relaxed transition-opacity duration-400 ${
            fade ? 'opacity-100' : 'opacity-0'
          }`}
        >
          &ldquo;{charlesQuotes[quoteIndex]}&rdquo;
        </p>
        <span className="text-xs mc-text-muted mt-2 font-bold">&mdash; Charles</span>
      </div>
      <button onClick={onNavigate} className="mc-btn-primary text-sm">
        <Rocket className="h-4 w-4 inline mr-1" /> Pick Your Plan
      </button>
    </div>
  );
}

/* ================================================================
   Main Export
   ================================================================ */

interface GameTrackingProps {
  onNavigateToGameSetup?: () => void;
}

export default function GameTracking({ onNavigateToGameSetup }: GameTrackingProps) {
  const { data: games, isLoading, error } = useGetUserGames();
  const portfolio = useLivePortfolio(games);
  const withdrawMutation = useWithdrawGameEarnings();
  const netPL = portfolio.totalEarnings - portfolio.totalDeposits;
  const animatedNetPL = useCountUp(netPL, 1000);
  const animatedDeposits = useCountUp(portfolio.totalDeposits, 1000);
  const animatedEarnings = useCountUp(portfolio.totalEarnings, 1000);
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
      const result = await withdrawMutation.mutateAsync(selectedGame.id);
      setWithdrawnAmount(result.netEarnings);
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
          <h2 className="font-display text-lg mc-text-primary mb-3">Your Running Tally</h2>
          {/* Hero P/L number — animated countUp */}
          <div className="mb-4">
            <div className="mc-label mb-1">Net P/L</div>
            <div className={`text-4xl font-bold flex items-center justify-center gap-2 ${
              netPL >= 0 ? 'mc-text-green mc-glow-green' : 'mc-text-danger'
            }`}>
              {netPL >= 0 ? <TrendingUp className="h-6 w-6" /> : <TrendingDown className="h-6 w-6" />}
              {netPL >= 0 ? '+' : ''}{formatICP(animatedNetPL)} ICP
            </div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Deposited</div>
              <div className="text-xl font-bold mc-text-primary">{formatICP(animatedDeposits)} ICP</div>
            </div>
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Earned</div>
              <div className="text-xl font-bold mc-text-green mc-glow-green">{formatICP(animatedEarnings)} ICP</div>
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
                  withdrawPending={withdrawMutation.isPending}
                />
              ))}
            </div>
          )}

          {withdrawMutation.isError && (
            <div className="mc-status-red p-3 mt-3 text-center text-sm">
              {withdrawMutation.error?.message || 'Withdrawal failed'}
            </div>
          )}
        </div>

        {/* House info */}
        <div className="mc-house-card">
          <h3 className="font-display text-base mc-text-gold mb-3">The House Always Wins</h3>
          <p className="font-accent text-sm mc-text-muted italic mb-3">But here's how much.</p>
          <div className="text-sm mc-text-dim space-y-2 leading-relaxed">
            <p>Simple positions: 7% exit toll within 3 days, 5% within 10 days, 3% after.</p>
            <p>Compounding plans: flat 13% Jackpot Fee at withdrawal.</p>
            <p>Compounding plans pay out compounded interest at maturity.</p>
          </div>
        </div>
      </div>

      {/* Withdrawal Dialog */}
      <Dialog open={withdrawDialogOpen} onOpenChange={setWithdrawDialogOpen}>
        <DialogContent className="mc-dialog">
          <DialogHeader>
            <DialogTitle className="font-display">Confirm Withdrawal</DialogTitle>
            <DialogDescription className="mc-text-dim text-sm">
              {selectedGame && (() => {
                const info = getExitTollInfo(selectedGame);
                if (info.nextFee && info.timeToNext) {
                  return (
                    <span>
                      You'll pay a <strong className="mc-text-gold">{info.currentFee}%</strong> exit toll.
                      Wait <strong className="mc-text-cyan">{countdown}</strong> to reduce it to <strong className="mc-text-green">{info.nextFee}%</strong>.
                    </span>
                  );
                }
                return <span>You'll pay a <strong className="mc-text-gold">{info.currentFee}%</strong> exit toll.</span>;
              })()}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2">
            <Button variant="outline" onClick={() => setWithdrawDialogOpen(false)} className="mc-btn-secondary">
              Cancel
            </Button>
            <Button onClick={handleWithdrawConfirm} disabled={withdrawMutation.isPending} className="mc-btn-primary">
              {withdrawMutation.isPending ? 'Processing...' : 'Confirm'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Post-withdrawal celebration toast */}
      {reinvestDialogOpen && (
        <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]">
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
                className="mc-btn-primary px-5 py-2 rounded-full text-sm"
              >
                <><Rocket className="h-4 w-4" /> YOLO Again</>
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
