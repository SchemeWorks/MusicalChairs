import React, { useState, useEffect } from 'react';
import { useGetUserGames, useWithdrawGameEarnings, calculateCurrentEarnings, isCompoundingPlanUnlocked, getTimeRemaining } from '../hooks/useQueries';
import { GameRecord, GamePlan } from '../backend';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';
import { RefreshCw, Lock, ArrowDownCircle } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

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

interface GameTrackingProps {
  onNavigateToGameSetup?: () => void;
}

export default function GameTracking({ onNavigateToGameSetup }: GameTrackingProps) {
  const { data: games, isLoading, error, refetch } = useGetUserGames();
  const withdrawMutation = useWithdrawGameEarnings();
  const [withdrawDialogOpen, setWithdrawDialogOpen] = useState(false);
  const [reinvestDialogOpen, setReinvestDialogOpen] = useState(false);
  const [selectedGame, setSelectedGame] = useState<GameRecord | null>(null);
  const [withdrawnAmount, setWithdrawnAmount] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const [flashValues, setFlashValues] = useState(false);
  const [countdown, setCountdown] = useState('');

  const handleRefresh = async () => {
    setRefreshing(true);
    await refetch();
    setFlashValues(true);
    setTimeout(() => { setRefreshing(false); setFlashValues(false); }, 800);
  };

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

  const getExitTollInfo = (game: GameRecord) => {
    const startTime = Number(game.startTime) / 1000000;
    const elapsed = Date.now() - startTime;
    const days = elapsed / (1000 * 60 * 60 * 24);
    if (game.isCompounding) return { currentFee: 13, nextFee: null, timeToNext: null };
    if (days < 3) return { currentFee: 7, nextFee: 5, timeToNext: (3 * 86400000) - elapsed };
    if (days < 10) return { currentFee: 5, nextFee: 3, timeToNext: (10 * 86400000) - elapsed };
    return { currentFee: 3, nextFee: null, timeToNext: null };
  };

  const fmtCountdown = (ms: number) => {
    if (ms <= 0) return '';
    const d = Math.floor(ms / 86400000);
    const h = Math.floor((ms % 86400000) / 3600000);
    const m = Math.floor((ms % 3600000) / 60000);
    return `${d}d ${h}h ${m}m`;
  };

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

  const totalDeposits = games?.reduce((s, g) => s + g.amount, 0) || 0;
  const totalEarnings = games?.reduce((s, g) => s + calculateCurrentEarnings(g), 0) || 0;
  const hasGames = games && games.length > 0;

  const formatDate = (ts: bigint) =>
    new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1000000));

  const daysActive = (ts: bigint) => Math.floor((Date.now() - Number(ts) / 1000000) / 86400000);

  return (
    <>
      <div className="space-y-6">
        {/* Running Tally */}
        <div className="mc-card-elevated">
          <div className="flex items-center justify-between mb-4">
            <h2 className="font-display text-lg mc-text-primary">Your Running Tally</h2>
            <button onClick={handleRefresh} className="mc-btn-pill" disabled={refreshing}>
              <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} /> Refresh
            </button>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="mc-card p-5 text-center">
              <div className="mc-label mb-2">Total Deposits</div>
              <div className={`text-3xl font-bold mc-text-primary ${flashValues ? 'mc-counter-flash' : ''}`}>{formatICP(totalDeposits)} ICP</div>
            </div>
            <div className="mc-card p-5 text-center">
              <div className="mc-label mb-2">Accumulated Earnings</div>
              <div className={`text-3xl font-bold mc-text-green mc-glow-green ${flashValues ? 'mc-counter-flash' : ''}`}>{formatICP(totalEarnings)} ICP</div>
            </div>
          </div>
        </div>

        {/* Positions */}
        <div className="mc-card-elevated">
          <div className="flex items-center justify-between mb-4">
            <h2 className="font-display text-lg mc-text-primary">Your Positions</h2>
            {hasGames && (
              <button onClick={handleRefresh} disabled={refreshing} className="mc-btn-refresh flex items-center gap-2">
                <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
                Refresh
              </button>
            )}
          </div>

          {!hasGames ? (
            <div className="text-center py-12">
              <div className="text-6xl mb-5 animate-bounce" style={{ animationDuration: '2s' }}>🎰</div>
              <p className="font-display text-lg mc-text-primary mb-2">No positions yet</p>
              <p className="text-sm mc-text-dim mb-6 max-w-sm mx-auto">
                Start by making your first deposit in a game plan. The house is waiting.
              </p>
              <button
                onClick={onNavigateToGameSetup}
                className="mc-btn-primary text-sm"
              >
                Pick Your Plan
              </button>
            </div>
          ) : (
            <div className="space-y-3">
              {games!.map((game) => {
                const earnings = calculateCurrentEarnings(game);
                const name = planNames[game.plan] || 'Unknown Plan';
                const accent = planAccents[game.plan] || '';
                const unlocked = isCompoundingPlanUnlocked(game);
                const canWithdraw = !game.isCompounding || unlocked;
                const hasEarnings = earnings > 0;
                const timeRem = getTimeRemaining(game);

                return (
                  <div key={game.id.toString()} className={`mc-card ${accent} p-4 transition-all duration-200 hover:translate-y-[-2px] hover:shadow-lg`}>
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4 items-center">
                      {/* Plan info */}
                      <div>
                        <div className="font-bold mc-text-primary">{name}</div>
                        <div className="flex items-center gap-2 mt-1">
                          <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
                            game.isCompounding
                              ? 'bg-purple-500/20 mc-text-purple'
                              : 'bg-green-500/20 mc-text-green'
                          }`}>
                            {game.isCompounding ? 'Compounding' : 'Simple'}
                          </span>
                          <span className="text-xs mc-text-muted">{daysActive(game.startTime)}d active</span>
                        </div>
                      </div>

                      {/* Deposit */}
                      <div className="md:text-center">
                        <div className="mc-label">Deposit</div>
                        <div className="text-lg font-bold mc-text-primary">{formatICP(game.amount)} ICP</div>
                        <div className="text-xs mc-text-muted">{formatDate(game.startTime)}</div>
                      </div>

                      {/* Earnings + action */}
                      <div className="flex items-center justify-between md:justify-end gap-4">
                        <div className="md:text-right">
                          <div className="mc-label">Earnings</div>
                          <div className="text-lg font-bold mc-text-green mc-glow-green">{formatICP(earnings)} ICP</div>
                        </div>
                        <button
                          onClick={() => handleWithdrawClick(game)}
                          disabled={withdrawMutation.isPending || !canWithdraw || !hasEarnings}
                          className={`text-xs px-3 py-2 rounded-lg font-bold transition-all ${
                            canWithdraw && hasEarnings
                              ? 'mc-btn-primary'
                              : 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5'
                          }`}
                          title={!canWithdraw ? 'Locked until maturity' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
                        >
                          {!canWithdraw ? (
                            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h</span>
                          ) : (
                            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Withdraw</span>
                          )}
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
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
                YOLO Again 🚀
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
