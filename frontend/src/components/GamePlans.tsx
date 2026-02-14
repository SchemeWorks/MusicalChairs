import React, { useState, useEffect } from 'react';
import { useCreateGame, useGetInternalWalletBalance, useGetMaxDepositLimit, useCheckDepositRateLimit, calculateSimpleROI, calculateCompoundingROI, getDailyRate, getPlanDays, calculatePonziPoints } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { toast } from 'sonner';

export default function GamePlans() {
  const [selectedMode, setSelectedMode] = useState<'simple' | 'compounding' | ''>('');
  const [selectedPlan, setSelectedPlan] = useState('');
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState('');

  const { data: balanceData, isLoading: balanceLoading } = useGetInternalWalletBalance();
  const { data: maxDepositLimit, isLoading: maxDepositLoading } = useGetMaxDepositLimit();
  const { data: canDeposit, isLoading: rateLimitLoading } = useCheckDepositRateLimit();
  const createGameMutation = useCreateGame();

  const walletBalance = balanceData?.internalBalance || 0;
  const maxDeposit = maxDepositLimit || 0;
  const minDeposit = 0.1;
  const depositAmount = parseFloat(amount) || 0;

  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const restricted = restrictToEightDecimals(e.target.value);
    const validation = validateICPInput(restricted);
    setAmount(restricted);
    setInputError(validation.error || '');
  };

  const [roiData, setRoiData] = useState<{
    totalReturn: number; profit: number; roiPercent: number; ponziPoints: number;
  } | null>(null);

  useEffect(() => {
    if (depositAmount > 0 && selectedPlan && selectedMode) {
      const days = getPlanDays(selectedPlan);
      const roi = selectedMode === 'simple'
        ? calculateSimpleROI(depositAmount, selectedPlan, days)
        : calculateCompoundingROI(depositAmount, selectedPlan, days);
      setRoiData({ ...roi, ponziPoints: calculatePonziPoints(depositAmount, selectedPlan, selectedMode) });
    } else {
      setRoiData(null);
    }
  }, [depositAmount, selectedPlan, selectedMode]);

  const handleCreateGame = async () => {
    if (!selectedPlan || !selectedMode || !amount) return;
    const dep = parseFloat(amount);
    if (dep < minDeposit || dep > walletBalance) return;
    const v = validateICPInput(amount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try {
      await createGameMutation.mutateAsync({ planId: selectedPlan, amount: dep, mode: selectedMode });
      toast.success('Game started! Welcome aboard, and don\'t stop inviting your friends! 😉');
      triggerConfetti();
      setAmount(''); setSelectedPlan(''); setSelectedMode(''); setInputError('');
    } catch (error) {
      console.error('Game creation failed:', error);
    }
  };

  if (balanceLoading || maxDepositLoading || rateLimitLoading) {
    return <LoadingSpinner />;
  }

  const isAmountValid = selectedMode === 'simple'
    ? depositAmount >= minDeposit && depositAmount <= walletBalance && depositAmount <= maxDeposit && !inputError
    : depositAmount >= minDeposit && depositAmount <= walletBalance && !inputError;
  const hasValidAmount = amount && isAmountValid;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Step 1: Mode Selection */}
        <div className="mb-8">
          <div className="mc-label mb-3">Step 1 — Choose Game Mode</div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mc-stagger">
            {/* Simple */}
            <div
              onClick={() => { setSelectedMode('simple'); setSelectedPlan('21-day-simple'); }}
              className={`mc-card-select p-5 ${selectedMode === 'simple' ? 'active-green' : ''}`}
            >
              <div className="text-3xl mb-3 text-center">🌱</div>
              <h4 className="font-display text-base mc-text-green text-center mb-2">Simple Mode</h4>
              <p className="text-xs mc-text-dim text-center mb-3">21-Day Plan — 11% daily return (interest only)</p>
              <ul className="text-xs mc-text-muted space-y-1">
                <li>• Principal consumed at deposit</li>
                <li>• Withdraw earnings at any time</li>
                <li>• Exit Toll: 7% / 5% / 3% based on timing</li>
                <li>• Ponzi Points: 1x multiplier</li>
              </ul>
              {selectedMode === 'simple' && (
                <div className="mc-status-green p-2 mt-3 text-center text-xs font-bold">✓ Simple Mode Selected</div>
              )}
            </div>

            {/* Compounding */}
            <div
              onClick={() => { setSelectedMode('compounding'); setSelectedPlan(''); }}
              className={`mc-card-select p-5 ${selectedMode === 'compounding' ? 'active-purple' : ''}`}
            >
              <div className="text-3xl mb-3 text-center">🔥</div>
              <h4 className="font-display text-base mc-text-purple text-center mb-2">Compounding Mode</h4>
              <p className="text-xs mc-text-dim text-center mb-3">Choose plan length below</p>
              <ul className="text-xs mc-text-muted space-y-1">
                <li>• Enhanced returns through compounding</li>
                <li>• Funds locked until plan period ends</li>
                <li>• Exit Toll: Flat 13% fee</li>
                <li>• Ponzi Points: 2x–3x multipliers</li>
              </ul>
              {selectedMode === 'compounding' && (
                <div className="mc-status-green p-2 mt-3 text-center text-xs font-bold">✓ Compounding Selected</div>
              )}
            </div>
          </div>
        </div>

        {/* Step 2: Plan Length (compounding only) */}
        {selectedMode === 'compounding' && (
          <div className="mb-8">
            <div className="mc-label mb-2">Step 2 — Select Lockup Period</div>
            <div className="mc-status-gold p-3 text-xs text-center mb-4">
              ⚠️ The longer the plan, the greater the ROI potential — but also the higher the risk the round ends first.
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div
                onClick={() => setSelectedPlan('15-day-compounding')}
                className={`mc-card-select p-4 text-center ${selectedPlan === '15-day-compounding' ? 'active-gold' : ''}`}
              >
                <div className="text-2xl mb-2">🚀</div>
                <h4 className="font-bold mc-text-primary mb-1">15-Day Compounding</h4>
                <span className="text-xs mc-text-dim">12% daily — 2x Ponzi Points</span>
                {selectedPlan === '15-day-compounding' && (
                  <div className="mc-status-green p-1 mt-2 text-center text-xs font-bold">✓ Selected</div>
                )}
              </div>
              <div
                onClick={() => setSelectedPlan('30-day-compounding')}
                className={`mc-card-select p-4 text-center ${selectedPlan === '30-day-compounding' ? 'active-gold' : ''}`}
              >
                <div className="text-2xl mb-2">💎</div>
                <h4 className="font-bold mc-text-primary mb-1">30-Day Compounding</h4>
                <span className="text-xs mc-text-dim">9% daily — 3x Ponzi Points</span>
                {selectedPlan === '30-day-compounding' && (
                  <div className="mc-status-green p-1 mt-2 text-center text-xs font-bold">✓ Selected</div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Final Step: Amount + CTA */}
        {((selectedMode === 'simple' && selectedPlan) || (selectedMode === 'compounding' && selectedPlan)) && (
          <div>
            <div className="mc-label mb-3">
              {selectedMode === 'simple' ? 'Step 2' : 'Step 3'} — Enter Amount & Open Position
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
              {/* Amount input */}
              <div>
                <div className="flex justify-between items-center mb-2">
                  <span className="text-xs font-bold mc-text-primary">Amount</span>
                  <span className="text-xs mc-text-dim">Available: {formatICP(walletBalance)} ICP</span>
                </div>
                <input
                  type="number"
                  value={amount}
                  onChange={handleAmountChange}
                  placeholder={selectedMode === 'simple' ? `Min: ${minDeposit}, Max: ${formatICP(maxDeposit)} ICP` : `Min: ${minDeposit} ICP`}
                  min={minDeposit}
                  step="0.00000001"
                  className="mc-input w-full text-center text-lg"
                />

                {depositAmount > 0 && (
                  <div className="mt-2 space-y-1 text-xs">
                    {inputError && <div className="mc-text-danger">⚠️ {inputError}</div>}
                    {!inputError && depositAmount < minDeposit && <div className="mc-text-danger">⚠️ Minimum deposit is {minDeposit} ICP</div>}
                    {!inputError && depositAmount > walletBalance && <div className="mc-text-danger">⚠️ Insufficient balance</div>}
                    {!inputError && selectedMode === 'simple' && depositAmount > maxDeposit && <div className="mc-text-danger">⚠️ Max for simple mode: {formatICP(maxDeposit)} ICP</div>}
                    {!canDeposit && <div className="mc-text-danger">⚠️ Rate limit: 3 positions per hour max</div>}
                  </div>
                )}

                <div className="mc-status-blue p-3 mt-3 text-xs space-y-1">
                  <p>Min deposit: 0.1 ICP</p>
                  <p>Simple mode: max = 20% of pot or 5 ICP (whichever is higher)</p>
                  <p>3% Entry Skim on every deposit</p>
                  <p>Half of skim + tolls seed the next round, half repay the House</p>
                </div>
              </div>

              {/* ROI Calculator */}
              <div>
                {roiData ? (
                  <div>
                    <div className="text-center mb-3">
                      <span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span>
                    </div>
                    <div className="mc-card p-4">
                      <div className="flex justify-between items-center">
                        <div>
                          <div className="mc-label">{selectedMode === 'simple' ? 'Interest Payout' : 'Compounded Interest'}</div>
                          <div className="text-xl font-bold mc-text-green mc-glow-green">{formatICP(roiData.totalReturn)} ICP</div>
                          <div className="text-xs mc-text-green opacity-70">
                            {selectedMode === 'simple'
                              ? `${(roiData.totalReturn / depositAmount).toFixed(2)}x ROI (${roiData.roiPercent.toFixed(0)}%)`
                              : `${roiData.roiPercent.toFixed(1)}% ROI`}
                          </div>
                        </div>
                        <div className="text-right">
                          <div className="mc-label">Ponzi Points</div>
                          <div className="text-xl font-bold mc-text-purple mc-glow-purple">{roiData.ponziPoints.toLocaleString()}</div>
                          <div className="text-xs mc-text-purple opacity-70">
                            {selectedMode === 'simple' ? '1x' : selectedPlan === '15-day-compounding' ? '2x' : '3x'} multiplier
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="h-full flex items-center justify-center text-center">
                    <div>
                      <div className="text-3xl mb-2 opacity-30">📊</div>
                      <p className="text-sm mc-text-muted">Enter an amount to see ROI</p>
                    </div>
                  </div>
                )}
              </div>
            </div>

            {walletBalance < minDeposit && (
              <div className="mc-status-blue p-3 mb-4 text-xs text-center">
                Insufficient balance. Fund your Musical Chairs Wallet with at least {minDeposit} ICP first.
              </div>
            )}

            {/* CTA + Warning side by side */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 items-center">
              <button
                onClick={handleCreateGame}
                disabled={!amount || !selectedPlan || !selectedMode || !isAmountValid || !canDeposit || createGameMutation.isPending || walletBalance < minDeposit || !!inputError}
                className={`w-full py-4 text-base font-bold rounded-xl transition-all ${
                  hasValidAmount ? 'mc-btn-primary pulse' : 'mc-btn-primary'
                }`}
              >
                {createGameMutation.isPending ? 'Starting Game...'
                  : walletBalance < minDeposit ? 'Fund Wallet First'
                  : !canDeposit ? 'Rate Limited'
                  : !selectedMode ? 'Choose Mode First'
                  : selectedMode === 'compounding' && !selectedPlan ? 'Select Plan First'
                  : inputError ? 'Fix Input Error'
                  : '🎰 START GAME'}
              </button>
              <div className="mc-status-red p-3 text-center text-sm font-bold">
                ⚠️ THIS IS A GAMBLING GAME<br />
                <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
              </div>
            </div>
          </div>
        )}

        {createGameMutation.isError && (
          <div className="mc-status-red p-3 mt-4 text-center text-sm">
            {createGameMutation.error?.message || 'Failed to start game'}
          </div>
        )}
      </div>
    </div>
  );
}
