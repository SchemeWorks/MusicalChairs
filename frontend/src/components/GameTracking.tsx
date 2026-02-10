import React, { useState, useEffect } from 'react';
import { useGetUserGames, useWithdrawGameEarnings, calculateCurrentEarnings, isCompoundingPlanUnlocked, getTimeRemaining, calculateExitTollFee } from '../hooks/useQueries';
import { GameRecord, GamePlan } from '../backend';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';

const planNames: { [key in GamePlan]: string } = {
  [GamePlan.simple21Day]: '21-Day Simple Plan',
  [GamePlan.compounding15Day]: '15-Day Compounding Plan',
  [GamePlan.compounding30Day]: '30-Day Compounding Plan'
};

const planEmojis: { [key in GamePlan]: string } = {
  [GamePlan.simple21Day]: '🌱',
  [GamePlan.compounding15Day]: '🚀',
  [GamePlan.compounding30Day]: '💎'
};

const planGradients: { [key in GamePlan]: string } = {
  [GamePlan.simple21Day]: 'from-green-400 to-green-600',
  [GamePlan.compounding15Day]: 'from-green-400 to-blue-500',
  [GamePlan.compounding30Day]: 'from-purple-400 to-pink-500'
};

interface GameTrackingProps {
  onNavigateToGameSetup?: () => void;
}

export default function GameTracking({ onNavigateToGameSetup }: GameTrackingProps) {
  const { data: games, isLoading, error, refetch } = useGetUserGames();
  const withdrawEarningsMutation = useWithdrawGameEarnings();
  const [withdrawDialogOpen, setWithdrawDialogOpen] = useState(false);
  const [reinvestDialogOpen, setReinvestDialogOpen] = useState(false);
  const [selectedGame, setSelectedGame] = useState<GameRecord | null>(null);
  const [withdrawnAmount, setWithdrawnAmount] = useState<number>(0);
  const [refreshing, setRefreshing] = useState(false);
  const [countdown, setCountdown] = useState<string>('');
  const [animatingTotals, setAnimatingTotals] = useState(false);

  const handleRefresh = async () => {
    setRefreshing(true);
    setAnimatingTotals(true);
    
    await refetch();
    
    // Keep animation running for visual effect
    setTimeout(() => {
      setAnimatingTotals(false);
    }, 2000);
    
    setRefreshing(false);
  };

  const handleWithdrawClick = (game: GameRecord) => {
    setSelectedGame(game);
    setWithdrawDialogOpen(true);
  };

  const handleWithdrawConfirm = async () => {
    if (!selectedGame) return;
    
    try {
      const result = await withdrawEarningsMutation.mutateAsync(selectedGame.id);
      setWithdrawnAmount(result.netEarnings);
      setWithdrawDialogOpen(false);
      setSelectedGame(null);
      
      // Trigger confetti celebration
      triggerConfetti();
      setReinvestDialogOpen(true);
    } catch (error) {
      console.error('Withdrawal failed:', error);
    }
  };

  const handleReinvestClick = () => {
    setReinvestDialogOpen(false);
    if (onNavigateToGameSetup) {
      onNavigateToGameSetup();
    }
  };

  const getExitTollInfo = (game: GameRecord) => {
    const startTime = Number(game.startTime) / 1000000;
    const elapsedTime = Date.now() - startTime;
    const elapsedDays = elapsedTime / (1000 * 60 * 60 * 24);
    
    if (game.isCompounding) {
      return { currentFee: 13, nextFee: null, timeToNext: null };
    }
    
    if (elapsedDays < 3) {
      const timeToNext = (3 * 24 * 60 * 60 * 1000) - elapsedTime;
      return { 
        currentFee: 7, 
        nextFee: 5, 
        timeToNext 
      };
    } else if (elapsedDays < 10) {
      const timeToNext = (10 * 24 * 60 * 60 * 1000) - elapsedTime;
      return { 
        currentFee: 5, 
        nextFee: 3, 
        timeToNext 
      };
    } else {
      return { currentFee: 3, nextFee: null, timeToNext: null };
    }
  };

  const formatCountdown = (timeInMs: number): string => {
    if (timeInMs <= 0) return '';
    
    const days = Math.floor(timeInMs / (1000 * 60 * 60 * 24));
    const hours = Math.floor((timeInMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
    const minutes = Math.floor((timeInMs % (1000 * 60 * 60)) / (1000 * 60));
    const seconds = Math.floor((timeInMs % (1000 * 60)) / 1000);
    
    return `${days} days, ${hours} hours, ${minutes} minutes, ${seconds} seconds`;
  };

  // Update countdown every second when dialog is open
  useEffect(() => {
    let interval: NodeJS.Timeout;
    
    if (withdrawDialogOpen && selectedGame) {
      const updateCountdown = () => {
        const exitTollInfo = getExitTollInfo(selectedGame);
        if (exitTollInfo.timeToNext) {
          setCountdown(formatCountdown(exitTollInfo.timeToNext));
        }
      };
      
      updateCountdown(); // Initial update
      interval = setInterval(updateCountdown, 1000);
    }
    
    return () => {
      if (interval) {
        clearInterval(interval);
      }
    };
  }, [withdrawDialogOpen, selectedGame]);

  if (isLoading) {
    return (
      <div className="space-y-6">
        {/* Single Frosted Glass Container - Loading */}
        <div className="profit-center-single-container">
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="space-y-6">
        {/* Single Frosted Glass Container - Error */}
        <div className="profit-center-single-container">
          <Card className="border-red-500 bg-red-50">
            <CardContent className="pt-4">
              <p className="text-red-800 font-bold text-center">
                ⚠️ Unable to load profit center data. Please try again later.
              </p>
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  if (!games || games.length === 0) {
    return (
      <div className="space-y-6">
        {/* Single Frosted Glass Container - Empty State */}
        <div className="profit-center-single-container">
          {/* Your Running Tally Section */}
          <div className="text-center mb-8">
            <h2 className="text-xl font-bold text-white text-with-backdrop mb-6">
              Your Running Tally
            </h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
              <Card className="portfolio-metric-card !bg-transparent border-0">
                <CardContent className="pt-4 text-center">
                  <div className="text-sm font-bold text-white mb-1">Total Deposits</div>
                  <div className="text-3xl font-black text-white portfolio-icp-clean">0 ICP</div>
                </CardContent>
              </Card>
              <Card className="portfolio-metric-card !bg-transparent border-0">
                <CardContent className="pt-4 text-center">
                  <div className="text-sm font-bold text-white mb-1">Total Accumulated Earnings</div>
                  <div className="text-4xl font-black text-white portfolio-icp-clean">0 ICP</div>
                </CardContent>
              </Card>
            </div>
          </div>

          <Separator className="bg-white/20 mb-8" />

          {/* Your Positions Section */}
          <div className="text-center mb-8">
            <h2 className="text-xl font-bold text-white text-with-backdrop mb-6">
              Your Positions
            </h2>
            <p className="text-sm text-white text-with-backdrop mb-4">
              Earnings accumulate in real time
            </p>
            <div className="py-8">
              <div className="text-6xl mb-4">🎰</div>
              <p className="text-white font-bold text-lg text-with-backdrop">No positions yet!</p>
              <p className="text-white text-sm mt-2 text-with-backdrop">
                Start by making your first deposit in one of our game plans below.
              </p>
            </div>
          </div>

          <Separator className="bg-white/20 mb-8" />

          {/* The House Always Wins Section */}
          <div className="text-center">
            <div className="house-always-wins-card">
              <div className="text-lg font-black text-gray-900 mb-2">🎰 The House Always Wins 🎰</div>
              <div className="text-sm text-gray-700 italic font-semibold mb-4">The House Always Wins — but here's how much.</div>
              <div className="text-sm text-gray-800 space-y-3 font-medium text-left max-w-2xl mx-auto">
                <div>
                  Simple positions will be charged a withdrawal fee of 7% within 3 days of starting the plan, 5% within 10 days, 3% after 10 days.
                </div>
                <div>
                  Successful compounding plans will be charged a 13% Jackpot Fee on their withdrawal.
                </div>
                <div>
                  Compounding plans pay out the compounded interest at maturity.
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  const formatDate = (timestamp: bigint) => {
    const date = new Date(Number(timestamp) / 1000000); // Convert nanoseconds to milliseconds
    return new Intl.DateTimeFormat('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    }).format(date);
  };

  const calculateDaysActive = (startTime: bigint) => {
    const now = Date.now();
    const startDate = Number(startTime) / 1000000; // Convert nanoseconds to milliseconds
    const diffTime = Math.abs(now - startDate);
    const diffDays = Math.floor(diffTime / (1000 * 60 * 60 * 24));
    return diffDays;
  };

  const CountdownTimer = ({ game }: { game: GameRecord }) => {
    const timeRemaining = getTimeRemaining(game);
    
    if (timeRemaining.days === 0 && timeRemaining.hours === 0 && timeRemaining.minutes === 0) {
      return <span className="text-green-600 font-bold">✅ Unlocked</span>;
    }
    
    return (
      <div className="text-orange-600 font-bold text-sm">
        🔒 {timeRemaining.days}d {timeRemaining.hours}h {timeRemaining.minutes}m
      </div>
    );
  };

  return (
    <>
      <div className="space-y-6">
        {/* Single Frosted Glass Container with All Three Sections */}
        <div className="profit-center-single-container">
          {/* Your Running Tally Section */}
          <div className="text-center mb-8">
            <h2 className="text-xl font-bold text-white text-with-backdrop mb-6">
              Your Running Tally
            </h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
              <Card className="portfolio-metric-card !bg-transparent border-0">
                <CardContent className="pt-4 text-center">
                  <div className="text-sm font-bold text-white mb-1">Total Deposits</div>
                  <div className={`text-3xl font-black text-white portfolio-icp-clean ${animatingTotals ? 'animate-slot-roll' : ''}`}>
                    {formatICP(games.reduce((sum, game) => sum + game.amount, 0))} ICP
                  </div>
                </CardContent>
              </Card>
              <Card className="portfolio-metric-card !bg-transparent border-0">
                <CardContent className="pt-4 text-center">
                  <div className="text-sm font-bold text-white mb-1">Total Accumulated Earnings</div>
                  <div className={`text-4xl font-black text-white portfolio-icp-clean ${animatingTotals ? 'animate-jackpot-glow' : ''}`}>
                    {formatICP(games.reduce((sum, game) => sum + calculateCurrentEarnings(game), 0))} ICP
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>

          <Separator className="bg-white/20 mb-8" />

          {/* Your Positions Section */}
          <div className="mb-8">
            <div className="text-center mb-6">
              <h2 className="text-xl font-bold text-white text-with-backdrop">
                Your Positions
              </h2>
              <p className="text-sm text-white text-with-backdrop">
                Earnings accumulate in real time
              </p>
            </div>
            
            {/* Control Bar with Refresh Button Only */}
            <div className="flex justify-start items-center mb-4">
              <Button
                onClick={handleRefresh}
                disabled={refreshing}
                className="refresh-earnings-button-redesigned rounded-full"
              >
                {refreshing ? (
                  <div className="flex items-center">
                    <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                    Refreshing...
                  </div>
                ) : (
                  'Refresh Earnings'
                )}
              </Button>
            </div>

            <div className="space-y-4">
              {games.map((game, index) => {
                // Get accumulated earnings from our tracking system
                const currentEarnings = calculateCurrentEarnings(game);
                const daysActive = calculateDaysActive(game.startTime);
                const planName = planNames[game.plan];
                const planEmoji = planEmojis[game.plan];
                const planGradient = planGradients[game.plan];
                const isUnlocked = isCompoundingPlanUnlocked(game);
                const canWithdraw = !game.isCompounding || isUnlocked;
                const hasEarnings = currentEarnings > 0;

                // Determine plan card styling based on plan type
                const planCardClass = game.isCompounding ? 'plan-card-compounding' : 'plan-card-simple';

                return (
                  <div key={game.id.toString()}>
                    {index > 0 && <Separator className="my-4 bg-gray-300" />}
                    <Card className={`border-2 hover:shadow-lg bg-gradient-to-r from-gray-50 to-white transition-all ${planCardClass}`}>
                      <CardContent className="pt-6">
                        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                          {/* Plan Info */}
                          <div className="flex items-center">
                            <div className={`w-12 h-12 bg-gradient-to-br ${planGradient} rounded-xl flex items-center justify-center text-xl mr-4`}>
                              {planEmoji}
                            </div>
                            <div>
                              <div className="font-black text-gray-900 text-lg">{planName}</div>
                              <div className="flex gap-2 mt-1">
                                <Badge variant={game.isCompounding ? "default" : "outline"}>
                                  {game.isCompounding ? '🔥 Compounding' : '🌱 Simple'}
                                </Badge>
                              </div>
                              <div className="text-xs text-gray-500 mt-1">
                                {daysActive} days active
                              </div>
                            </div>
                          </div>

                          {/* Game Amount */}
                          <div className="text-center lg:text-left">
                            <div className="text-sm font-bold text-gray-600 mb-1">Initial Deposit</div>
                            <div className="text-2xl font-black text-blue-600">
                              {formatICP(game.amount)} ICP
                            </div>
                            <div className="text-xs text-gray-500">
                              {formatDate(game.startTime)}
                            </div>
                          </div>

                          {/* Accumulated Earnings */}
                          <div className="text-center lg:text-left">
                            <div className="text-sm font-bold text-gray-600 mb-1">Accumulated Earnings</div>
                            <div className="text-2xl font-black text-green-600">
                              {formatICP(currentEarnings)} ICP
                            </div>
                            <div className="text-xs text-gray-500">
                              {game.isCompounding ? 'Compound accumulation' : 'Simple accumulation'}
                            </div>
                          </div>
                        </div>

                        <Separator className="my-4" />

                        {/* Status Bar and Actions */}
                        <div className="flex items-center justify-between">
                          <div className="flex items-center space-x-4">
                            <Badge variant={game.isActive ? "default" : "secondary"}>
                              {game.isActive ? '🟢 Active' : '⚪ Inactive'}
                            </Badge>
                            <div className="text-xs text-gray-500">
                              Position #{index + 1}
                            </div>
                            {game.isCompounding && !isUnlocked && (
                              <div className="text-xs">
                                <CountdownTimer game={game} />
                              </div>
                            )}
                          </div>
                          <div className="flex items-center space-x-2">
                            {/* Show withdraw button for all active plans */}
                            {game.isActive && (
                              <Button
                                onClick={() => handleWithdrawClick(game)}
                                disabled={withdrawEarningsMutation.isPending || !canWithdraw || !hasEarnings}
                                size="sm"
                                className={`${
                                  canWithdraw && hasEarnings
                                    ? 'bg-gradient-to-r from-green-500 to-green-600 hover:from-green-600 hover:to-green-700' 
                                    : 'bg-gray-400 cursor-not-allowed'
                                }`}
                                title={
                                  !canWithdraw 
                                    ? 'Locked until plan period completes' 
                                    : !hasEarnings 
                                      ? 'No earnings to withdraw yet'
                                      : 'Withdraw earnings'
                                }
                              >
                                {withdrawEarningsMutation.isPending ? (
                                  <div className="flex items-center">
                                    <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-1"></div>
                                    Withdrawing...
                                  </div>
                                ) : !canWithdraw ? (
                                  '🔒 Locked'
                                ) : !hasEarnings ? (
                                  '💰 No Earnings'
                                ) : (
                                  '💰 Withdraw'
                                )}
                              </Button>
                            )}
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  </div>
                );
              })}

              {withdrawEarningsMutation.isError && (
                <Card className="border-red-500 bg-red-50">
                  <CardContent className="pt-4">
                    <p className="text-red-800 font-bold text-center text-sm">
                      ❌ {withdrawEarningsMutation.error?.message || 'Withdrawal failed'}
                    </p>
                  </CardContent>
                </Card>
              )}

              {withdrawEarningsMutation.isSuccess && (
                <Card className="border-green-500 bg-green-50">
                  <CardContent className="pt-4">
                    <p className="text-green-800 font-bold text-center text-sm">
                      ✅ Earnings withdrawn successfully! Withdrawal fee deducted. Net amount credited to your wallet! 🎉
                    </p>
                  </CardContent>
                </Card>
              )}
            </div>
          </div>

          <Separator className="bg-white/20 mb-8" />

          {/* The House Always Wins Section */}
          <div className="text-center">
            <div className="house-always-wins-card">
              <div className="text-lg font-black text-gray-900 mb-2">🎰 The House Always Wins 🎰</div>
              <div className="text-sm text-gray-700 italic font-semibold mb-4">The House Always Wins — but here's how much.</div>
              <div className="text-sm text-gray-800 space-y-3 font-medium text-left max-w-2xl mx-auto">
                <div>
                  Simple positions will be charged a withdrawal fee of 7% within 3 days of starting the plan, 5% within 10 days, 3% after 10 days.
                </div>
                <div>
                  Successful compounding plans will be charged a 13% Jackpot Fee on their withdrawal.
                </div>
                <div>
                  Compounding plans pay out the compounded interest at maturity.
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Withdrawal Confirmation Dialog - Updated with dynamic countdown */}
      <Dialog open={withdrawDialogOpen} onOpenChange={setWithdrawDialogOpen}>
        <DialogContent className="bg-white border-2 border-gray-300 shadow-lg rounded-lg">
          <DialogHeader>
            <DialogTitle className="text-gray-900">Confirm Withdrawal</DialogTitle>
            <DialogDescription className="text-gray-700">
              {selectedGame && (() => {
                const exitTollInfo = getExitTollInfo(selectedGame);
                if (exitTollInfo.nextFee && exitTollInfo.timeToNext) {
                  return (
                    <>
                      You will pay a <strong>{exitTollInfo.currentFee}%</strong> exit toll on this withdrawal. 
                      If you wait <strong>{countdown}</strong> the exit toll will reduce to <strong>{exitTollInfo.nextFee}%</strong>.
                    </>
                  );
                } else {
                  return `You will pay a ${exitTollInfo.currentFee}% exit toll on this withdrawal.`;
                }
              })()}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setWithdrawDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleWithdrawConfirm} disabled={withdrawEarningsMutation.isPending}>
              {withdrawEarningsMutation.isPending ? 'Processing...' : 'Confirm Withdrawal'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Post-Withdrawal Celebration Toast - Updated to match house money toast style */}
      {reinvestDialogOpen && (
        <div
          className="fixed top-8 left-1/2 transform -translate-x-1/2 z-[9999] transition-all duration-300 ease-out opacity-100 translate-y-0"
          style={{ pointerEvents: 'auto' }}
        >
          {/* Confetti container */}
          <div id="withdrawal-confetti" className="absolute inset-0 pointer-events-none" />
          
          {/* Toast card */}
          <div className="house-money-toast-card relative">
            {/* Content */}
            <div className="text-center space-y-3">
              {/* Title */}
              <div className="text-2xl font-black text-white">
                🎉 Congratulations!
              </div>
              
              {/* Subtitle with gradient accent on numbers */}
              <div className="text-base text-white/90 leading-relaxed">
                This scheme has earned you{' '}
                <span className="house-toast-accent font-black">{formatICP(withdrawnAmount)} ICP</span>!
                Want to grow it even more? Reinvest in a new plan now!
              </div>
            </div>
            
            {/* Buttons */}
            <div className="flex justify-center space-x-4 mt-4">
              <button
                onClick={() => setReinvestDialogOpen(false)}
                className="px-6 py-3 rounded-full bg-gray-600 hover:bg-gray-700 text-white font-bold transition-all"
              >
                Nah
              </button>
              <button
                onClick={handleReinvestClick}
                className="house-toast-button"
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
