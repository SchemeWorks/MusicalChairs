import React, { useState, useEffect } from 'react';
import { useCreateGame, useGetInternalWalletBalance, useGetMaxDepositLimit, useCheckDepositRateLimit, calculateSimpleROI, calculateCompoundingROI, getDailyRate, getPlanDays, calculatePonziPoints } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { toast } from 'sonner';

export default function GamePlans() {
  const [selectedMode, setSelectedMode] = useState<'simple' | 'compounding' | ''>('');
  const [selectedPlan, setSelectedPlan] = useState<string>('');
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState<string>('');
  
  const { data: balanceData, isLoading: balanceLoading } = useGetInternalWalletBalance();
  const { data: maxDepositLimit, isLoading: maxDepositLoading } = useGetMaxDepositLimit();
  const { data: canDeposit, isLoading: rateLimitLoading } = useCheckDepositRateLimit();
  const createGameMutation = useCreateGame();

  const walletBalance = balanceData?.internalBalance || 0;
  const maxDeposit = maxDepositLimit || 0;
  const minDeposit = 0.1;
  const depositAmount = parseFloat(amount) || 0;

  // Handle amount input with 8 decimal place validation
  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const input = e.target.value;
    const restrictedInput = restrictToEightDecimals(input);
    const validation = validateICPInput(restrictedInput);
    
    setAmount(restrictedInput);
    setInputError(validation.error || '');
  };

  // Calculate ROI and fees in real-time
  const [roiData, setRoiData] = useState<{
    totalReturn: number;
    profit: number;
    roiPercent: number;
    ponziPoints: number;
  } | null>(null);

  useEffect(() => {
    if (depositAmount > 0 && selectedPlan && selectedMode) {
      const planDays = getPlanDays(selectedPlan);
      const roi = selectedMode === 'simple' 
        ? calculateSimpleROI(depositAmount, selectedPlan, planDays)
        : calculateCompoundingROI(depositAmount, selectedPlan, planDays);
      
      const ponziPoints = calculatePonziPoints(depositAmount, selectedPlan, selectedMode);
      
      setRoiData({
        ...roi,
        ponziPoints
      });
    } else {
      setRoiData(null);
    }
  }, [depositAmount, selectedPlan, selectedMode]);

  const handleCreateGame = async () => {
    if (!selectedPlan || !selectedMode || !amount) return;
    
    const depositAmount = parseFloat(amount);
    if (depositAmount < minDeposit || depositAmount > walletBalance) return;
    
    // Final validation check
    const validation = validateICPInput(amount);
    if (!validation.isValid) {
      setInputError(validation.error || '');
      return;
    }
    
    try {
      await createGameMutation.mutateAsync({
        planId: selectedPlan,
        amount: depositAmount,
        mode: selectedMode
      });
      
      // Show toast notification
      toast.success('🎉 Game started successfully! Welcome aboard, and don\'t stop inviting your friends! 😉');
      
      // Trigger confetti celebration
      triggerConfetti();
      
      setAmount('');
      setSelectedPlan('');
      setSelectedMode('');
      setInputError('');
    } catch (error) {
      console.error('Game creation failed:', error);
    }
  };

  if (balanceLoading || maxDepositLoading || rateLimitLoading) {
    return (
      <div className="space-y-8">
        <div className="game-setup-single-container">
          <div className="text-center mb-8">
            <h2 className="text-2xl font-black text-white text-with-backdrop">
              🎮 Game Setup 🎮
            </h2>
          </div>
          <div className="flex justify-center">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  // Determine if amount is valid based on mode
  const isAmountValid = selectedMode === 'simple'
    ? depositAmount >= minDeposit && depositAmount <= walletBalance && depositAmount <= maxDeposit && !inputError
    : depositAmount >= minDeposit && depositAmount <= walletBalance && !inputError;
  
  const hasValidAmount = amount && isAmountValid;

  return (
    <div className="space-y-8">
      {/* Single Large Frosted Glass Container */}
      <div className="game-setup-single-container">
        {/* Game Setup Header inside the container */}
        <div className="text-center mb-8">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🎮 Game Setup 🎮
          </h2>
        </div>
        
        <div className="space-y-8">
          {/* Step 1: Mode Selection */}
          <div>
            <Label className="text-lg font-bold mb-4 block text-white text-with-backdrop">
              Step 1: Choose Game Mode
            </Label>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              {/* Simple Mode */}
              <div
                onClick={() => {
                  setSelectedMode('simple');
                  setSelectedPlan('21-day-simple');
                }}
                className={`mc-card-selectable p-6 ${
                  selectedMode === 'simple' ? 'selected-green' : ''
                }`}
              >
                <div className="text-center mb-4">
                  <div className="text-4xl mb-2">🌱</div>
                  <h4 className="text-xl font-black text-green-400 mb-2">Simple Mode</h4>
                  <p className="text-sm font-semibold text-green-300/70">21-Day Simple Plan → 11% daily return (interest only)</p>
                </div>
                
                <ul className="text-sm text-white/70 space-y-2">
                  <li className="flex items-start">
                    <span className="text-green-400 mr-2">•</span>
                    Principal consumed at deposit - only interest paid out
                  </li>
                  <li className="flex items-start">
                    <span className="text-green-400 mr-2">•</span>
                    Withdraw earnings at any time
                  </li>
                  <li className="flex items-start">
                    <span className="text-green-400 mr-2">•</span>
                    Exit Toll: 7%/5%/3% based on timing
                  </li>
                  <li className="flex items-start">
                    <span className="text-green-400 mr-2">•</span>
                    Ponzi Points: Base multiplier (1x)
                  </li>
                </ul>
                
                {selectedMode === 'simple' && (
                  <div className="mt-4 mc-card-status-green rounded-lg p-2">
                    <div className="text-green-300 font-bold text-center text-sm">
                      ✅ Simple Mode Selected
                    </div>
                  </div>
                )}
              </div>

              {/* Compounding Mode */}
              <div
                onClick={() => {
                  setSelectedMode('compounding');
                  setSelectedPlan(''); // Reset plan selection for compounding
                }}
                className={`mc-card-selectable p-6 ${
                  selectedMode === 'compounding' ? 'selected-purple' : ''
                }`}
              >
                <div className="text-center mb-4">
                  <div className="text-4xl mb-2">🔥</div>
                  <h4 className="text-xl font-black text-purple-400 mb-2">Compounding Mode</h4>
                  <p className="text-sm font-semibold text-purple-300/70">Choose plan length below</p>
                </div>
                
                <ul className="text-sm text-white/70 space-y-2">
                  <li className="flex items-start">
                    <span className="text-purple-400 mr-2">•</span>
                    Enhanced returns through compounding
                  </li>
                  <li className="flex items-start">
                    <span className="text-purple-400 mr-2">•</span>
                    Funds locked until plan period ends
                  </li>
                  <li className="flex items-start">
                    <span className="text-purple-400 mr-2">•</span>
                    Exit Toll: Flat 13% fee
                  </li>
                  <li className="flex items-start">
                    <span className="text-purple-400 mr-2">•</span>
                    Ponzi Points: Enhanced multipliers (2x-3x)
                  </li>
                </ul>
                
                {selectedMode === 'compounding' && (
                  <div className="mt-4 mc-card-status-green rounded-lg p-2">
                    <div className="text-green-300 font-bold text-center text-sm">
                      ✅ Compounding Mode Selected
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Divider */}
          {selectedMode && <Separator className="bg-white/20" />}

          {/* Step 2: Plan Length Selection - Only show for Compounding mode */}
          {selectedMode === 'compounding' && (
            <div>
              <Label className="text-lg font-bold mb-2 block text-white text-with-backdrop">
                Step 2: Select Lockup Period
              </Label>
              <div className="game-setup-warning-bubble mb-4">
                <div className="text-center text-yellow-900 font-semibold">
                  ⚠️ The longer the plan, the greater the potential ROI, but also the higher the risk that the round will end before your plan completes.
                </div>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {/* 15-Day Compounding Plan */}
                <div
                  onClick={() => setSelectedPlan('15-day-compounding')}
                  className={`mc-card-selectable p-4 ${
                    selectedPlan === '15-day-compounding' ? 'selected-yellow' : ''
                  }`}
                >
                  <div className="w-12 h-12 bg-gradient-to-br from-green-400 to-blue-500 rounded-xl flex items-center justify-center text-2xl mb-3 mx-auto">
                    🚀
                  </div>
                  
                  <div className="text-center">
                    <h4 className="text-lg font-black text-white mb-2">15-Day Compounding</h4>
                    <Badge variant="outline" className="mb-2 text-white/70 border-white/20">
                      12% daily return
                    </Badge>
                    <div className="text-xs text-purple-400 font-bold">
                      Ponzi Points: 2x multiplier
                    </div>
                  </div>
                  
                  {selectedPlan === '15-day-compounding' && (
                    <div className="mt-3 mc-card-status-green rounded-lg p-2">
                      <div className="text-green-300 font-bold text-center text-sm">
                        ✅ Selected
                      </div>
                    </div>
                  )}
                </div>

                {/* 30-Day Compounding Plan */}
                <div
                  onClick={() => setSelectedPlan('30-day-compounding')}
                  className={`mc-card-selectable p-4 ${
                    selectedPlan === '30-day-compounding' ? 'selected-yellow' : ''
                  }`}
                >
                  <div className="w-12 h-12 bg-gradient-to-br from-purple-400 to-pink-500 rounded-xl flex items-center justify-center text-2xl mb-3 mx-auto">
                    💎
                  </div>
                  
                  <div className="text-center">
                    <h4 className="text-lg font-black text-white mb-2">30-Day Compounding</h4>
                    <Badge variant="outline" className="mb-2 text-white/70 border-white/20">
                      9% daily return
                    </Badge>
                    <div className="text-xs text-purple-400 font-bold">
                      Ponzi Points: 3x multiplier
                    </div>
                  </div>
                  
                  {selectedPlan === '30-day-compounding' && (
                    <div className="mt-3 mc-card-status-green rounded-lg p-2">
                      <div className="text-green-300 font-bold text-center text-sm">
                        ✅ Selected
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Divider */}
          {((selectedMode === 'simple' && selectedPlan === '21-day-simple') || 
            (selectedMode === 'compounding' && selectedPlan)) && <Separator className="bg-white/20" />}

          {/* Final Step: Amount Input and Position Opening - Show if mode is selected and plan is determined */}
          {((selectedMode === 'simple' && selectedPlan === '21-day-simple') || 
            (selectedMode === 'compounding' && selectedPlan)) && (
            <div>
              <Label className="text-lg font-bold mb-4 block text-white text-with-backdrop">
                {selectedMode === 'simple' ? 'Step 2: Enter Amount & Open Position' : 'Step 3: Enter Amount & Open Position'}
              </Label>
              
              {/* Side-by-side layout: Deposit field and ROI calculator */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-4">
                {/* Left side: Amount Entry Field */}
                <div>
                  <div className="flex justify-between items-center mb-2">
                    <Label className="text-sm font-bold text-white text-with-backdrop">Select Amount</Label>
                    <span className="text-sm text-white">Available: {formatICP(walletBalance)} ICP</span>
                  </div>
                  <Input
                    type="number"
                    value={amount}
                    onChange={handleAmountChange}
                    placeholder={selectedMode === 'simple' ? `Min: ${minDeposit} ICP, Max: ${formatICP(maxDeposit)} ICP` : `Min: ${minDeposit} ICP`}
                    min={minDeposit}
                    max={selectedMode === 'simple' ? Math.min(walletBalance, maxDeposit) : walletBalance}
                    step="0.00000001"
                    className="text-center font-bold text-lg h-12 bg-white text-black placeholder:text-gray-500"
                  />
                  
                  {/* Validation Error Messages - Directly below the input field */}
                  {depositAmount > 0 && (
                    <div className="mt-2 space-y-1">
                      {inputError && (
                        <div className="text-white text-sm">
                          ⚠️ {inputError}
                        </div>
                      )}
                      {!inputError && depositAmount < minDeposit && (
                        <div className="text-white text-sm">
                          ⚠️ Minimum game deposit is {minDeposit} ICP
                        </div>
                      )}
                      {!inputError && depositAmount > walletBalance && (
                        <div className="text-white text-sm">
                          ⚠️ Insufficient balance. Please fund your Musical Chairs Wallet first.
                        </div>
                      )}
                      {!inputError && selectedMode === 'simple' && depositAmount > maxDeposit && (
                        <div className="text-white text-sm">
                          ⚠️ Maximum game deposit for simple mode is currently {formatICP(maxDeposit)} ICP
                        </div>
                      )}
                      {!canDeposit && (
                        <div className="text-white text-sm">
                          ⚠️ Rate limit: You can only open 3 positions per hour
                        </div>
                      )}
                    </div>
                  )}

                  {/* Deposit Information Box - Below the error messages */}
                  <div className="game-setup-info-bubble mt-2">
                    <div className="text-left space-y-1 text-sm">
                      <div>• Minimum Deposit: 0.1 ICP</div>
                      <div>• Simple mode: Maximum deposit applies (20% of pot or 5 ICP, whichever is higher)</div>
                      <div>• Compounding mode: No maximum deposit limit</div>
                      <div>• <strong>3%</strong> Entry Skim on every deposit</div>
                      <div>• Half the skim and exit tolls seed the next round, half repay the House</div>
                    </div>
                  </div>
                </div>

                {/* Right side: Live ROI Calculator */}
                <div>
                  {roiData ? (
                    <div className="h-full">
                      <div className="text-center mb-4">
                        <h3 className="text-lg font-bold text-white text-with-backdrop">
                          🚀 Expected ROI if plan matures before the round ends.
                        </h3>
                      </div>
                      <div className="mc-card rounded-lg p-4">
                        <div className="flex justify-between items-center">
                          <div className="text-left">
                            <div className="text-sm font-bold text-white/60">
                              {selectedMode === 'simple' ? 'Expected Interest Payout' : 'Expected Compounded Interest Payout'}
                            </div>
                            <div className="text-xl font-black mc-value-green mc-value-glow-green">
                              {formatICP(roiData.totalReturn)} ICP
                            </div>
                            <div className="text-sm font-bold text-green-400/70">
                              {selectedMode === 'simple' 
                                ? `${(roiData.totalReturn / depositAmount).toFixed(2)}x ROI (${roiData.roiPercent.toFixed(0)}%)`
                                : `${roiData.roiPercent.toFixed(1)}% ROI`
                              }
                            </div>
                          </div>
                          <div className="text-right">
                            <div className="text-sm font-bold text-purple-400">Ponzi Points</div>
                            <div className="text-xl font-black mc-value-purple mc-value-glow">
                              {roiData.ponziPoints.toLocaleString()}
                            </div>
                            <div className="text-xs text-purple-400/70">
                              {selectedMode === 'simple' ? '1x' : 
                               selectedPlan === '15-day-compounding' ? '2x' : '3x'} multiplier
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  ) : (
                    <div className="h-full flex items-center justify-center">
                      <div className="text-center text-white/40">
                        <div className="text-4xl mb-2">📊</div>
                        <p className="font-semibold">
                          Enter deposit amount to see ROI
                        </p>
                        <p className="text-sm">Live calculations will appear here</p>
                      </div>
                    </div>
                  )}
                </div>
              </div>
              
              {walletBalance < minDeposit && (
                <div className="game-setup-alert-bubble mb-4">
                  <div className="text-sm">
                    💡 Insufficient balance. Please fund your Musical Chairs Wallet with at least {minDeposit} ICP first.
                  </div>
                </div>
              )}

              {/* Side-by-side layout: Start Game Button and Gambling Warning */}
              <div className="mt-6 grid grid-cols-1 lg:grid-cols-2 gap-6 items-center">
                {/* Left side: Start Game Button */}
                <div>
                  <button
                    onClick={handleCreateGame}
                    disabled={
                      !amount || 
                      !selectedPlan ||
                      !selectedMode ||
                      !isAmountValid ||
                      !canDeposit ||
                      createGameMutation.isPending ||
                      walletBalance < minDeposit ||
                      !!inputError
                    }
                    className={`w-full h-14 text-xl font-black rounded-xl transition-all duration-300 ease-in-out ${
                      !amount || 
                      !selectedPlan ||
                      !selectedMode ||
                      !isAmountValid ||
                      !canDeposit ||
                      createGameMutation.isPending ||
                      walletBalance < minDeposit ||
                      !!inputError
                        ? 'start-game-button-disabled'
                        : hasValidAmount ? 'start-game-button-active-with-glow' : 'start-game-button-active'
                    }`}
                  >
                    {createGameMutation.isPending ? (
                      <div className="flex items-center justify-center">
                        <div className="animate-spin rounded-full h-6 w-6 border-2 border-white border-t-transparent mr-2"></div>
                        Starting Game...
                      </div>
                    ) : walletBalance < minDeposit ? (
                      '💰 Fund Musical Chairs Wallet First'
                    ) : !canDeposit ? (
                      '⏰ Rate Limited - 3 Positions Per Hour Max'
                    ) : !selectedMode ? (
                      '🎮 Choose Game Mode First'
                    ) : selectedMode === 'compounding' && !selectedPlan ? (
                      '📅 Select Plan Length First'
                    ) : inputError ? (
                      '❌ Fix Input Error'
                    ) : (
                      '🎰 START GAME!'
                    )}
                  </button>
                </div>

                {/* Right side: Gambling Warning */}
                <div className="flex justify-center">
                  <div className="game-setup-warning-red-bubble w-full text-center">
                    ⚠️ THIS IS A GAMBLING GAME! ⚠️
                    <br />
                    Only play with money you can afford to lose!
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Status Messages */}
          {createGameMutation.isError && (
            <div className="game-setup-error-bubble">
              <div className="text-center">
                ❌ {createGameMutation.error?.message || 'Failed to start game'}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
