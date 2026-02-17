import { useState, useEffect, useRef } from 'react';
import { useCreateGame, useGetInternalWalletBalance, useGetMaxDepositLimit, useCheckDepositRateLimit, calculateSimpleROI, calculateCompoundingROI, getDailyRate, getPlanDays, calculatePonziPoints } from '../hooks/useQueries';
import { useCountUp } from '../hooks/useCountUp';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Sprout, Flame, Rocket, Gem, BarChart3, AlertTriangle, Dices, Wallet, TrendingUp } from 'lucide-react';

/* ================================================================
   Post-deposit Charles quotes
   ================================================================ */

const charlesSuccessQuotes = [
  "Smart money moves fast. You just moved fast.",
  "That's not a deposit. That's a statement of intent.",
  "I've seen a lot of investors. You've got the look.",
  "The best time to get in was yesterday. The second best time is right now. You chose right.",
  "Most people just talk about it. You actually did it.",
  "You made the right move. Most people overthink and miss it.",
  "Now you're positioned. That's the hard part.",
  "Stay close. There's more coming for insiders.",
  "The key now is conviction. Weak hands don't get paid.",
  "This is where most people panic. We don't.",
  "The only mistake now is doubting your entry.",
];

function getRandomCharlesQuote() {
  return charlesSuccessQuotes[Math.floor(Math.random() * charlesSuccessQuotes.length)];
}

/* ================================================================
   Main Component
   ================================================================ */

interface GamePlansProps {
  onNavigateToProfitCenter?: () => void;
}

export default function GamePlans({ onNavigateToProfitCenter }: GamePlansProps) {
  const [selectedMode, setSelectedMode] = useState<'simple' | 'compounding' | ''>('');
  const [selectedPlan, setSelectedPlan] = useState('');
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState('');
  const [successToast, setSuccessToast] = useState<{ quote: string; amount: number } | null>(null);
  const [shakeInput, setShakeInput] = useState(false);
  const [clickError, setClickError] = useState('');
  const [shakeError, setShakeError] = useState(false);

  const triggerShake = () => {
    setShakeInput(true);
    setTimeout(() => setShakeInput(false), 400);
  };

  const triggerClickError = (msg: string) => {
    setClickError(msg);
    setShakeError(true);
    setTimeout(() => setShakeError(false), 400);
  };

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
    setClickError(''); // Clear CTA error when user adjusts input
  };

  const [roiData, setRoiData] = useState<{
    totalReturn: number; profit: number; roiPercent: number; ponziPoints: number;
  } | null>(null);

  // Reset token for countUp — changes when any ROI input changes
  const roiResetToken = useRef(0);
  const prevRoiKey = useRef('');

  useEffect(() => {
    if (depositAmount > 0 && selectedPlan && selectedMode) {
      const days = getPlanDays(selectedPlan);
      const roi = selectedMode === 'simple'
        ? calculateSimpleROI(depositAmount, selectedPlan, days)
        : calculateCompoundingROI(depositAmount, selectedPlan, days);
      setRoiData({ ...roi, ponziPoints: calculatePonziPoints(depositAmount, selectedPlan, selectedMode) });
      // Bump reset token when inputs change
      const key = `${depositAmount}-${selectedPlan}-${selectedMode}`;
      if (key !== prevRoiKey.current) {
        roiResetToken.current += 1;
        prevRoiKey.current = key;
      }
    } else {
      setRoiData(null);
    }
  }, [depositAmount, selectedPlan, selectedMode]);

  // Animated ROI values
  const animatedReturn = useCountUp(roiData?.totalReturn || 0, 800, roiResetToken.current);
  const animatedPP = useCountUp(roiData?.ponziPoints || 0, 800, roiResetToken.current);

  // Color shift based on ROI percentage
  const roiColor = !roiData ? 'mc-text-green' :
    roiData.roiPercent < 50 ? 'mc-text-green' :
    roiData.roiPercent < 200 ? 'mc-text-purple mc-glow-purple' :
    'mc-text-gold mc-glow-gold';

  const handleCreateGame = async () => {
    // Always-enabled CTA — validate on click and show inline error
    if (!selectedMode) { triggerClickError('Choose a mode first'); return; }
    if (!selectedPlan) { triggerClickError('Select a plan first'); return; }
    if (!amount) { triggerClickError('Enter an amount'); return; }
    const dep = parseFloat(amount);
    if (dep < minDeposit) { triggerClickError(`Amount below minimum (${minDeposit} ICP)`); triggerShake(); return; }
    if (dep > walletBalance) { triggerClickError('Insufficient balance'); triggerShake(); return; }
    if (selectedMode === 'simple' && dep > maxDeposit) { triggerClickError(`Max for simple mode: ${formatICP(maxDeposit)} ICP`); triggerShake(); return; }
    if (!canDeposit) { triggerClickError('Rate limited — wait before opening another position'); return; }
    const v = validateICPInput(amount);
    if (!v.isValid) { setInputError(v.error || ''); triggerShake(); return; }
    if (inputError) { triggerClickError('Fix input error first'); triggerShake(); return; }
    setClickError('');
    try {
      await createGameMutation.mutateAsync({ planId: selectedPlan, amount: dep, mode: selectedMode });
      triggerConfetti();
      setSuccessToast({ quote: getRandomCharlesQuote(), amount: dep });
      setAmount(''); setSelectedPlan(''); setSelectedMode(''); setInputError(''); setClickError('');
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

  // Daily earnings for the ROI display
  const dailyEarnings = depositAmount > 0 && selectedPlan && selectedMode
    ? depositAmount * getDailyRate(selectedPlan)
    : 0;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Step 1: Mode Selection */}
        <div className="mb-8">
          <div className="mc-label mb-3">Choose Your Poison</div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mc-stagger">
            {/* Simple */}
            <div
              onClick={() => { setSelectedMode('simple'); setSelectedPlan('21-day-simple'); setClickError(''); }}
              className={`mc-card-select p-5 ${selectedMode === 'simple' ? 'mc-active-green' : ''}`}
            >
              <Sprout className="h-8 w-8 mc-text-green mb-3 mx-auto" />
              <h4 className="font-display text-base mc-text-green text-center mb-1">Simple Mode</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-3">The slow grift. Steady, patient, still a grift.</p>
              <ul className="text-xs mc-text-muted space-y-1">
                <li>• 21 days, 11% daily return (interest only)</li>
                <li>• Principal consumed at deposit</li>
                <li>• Withdraw earnings anytime</li>
                <li>• Exit Toll: 7% / 5% / 3% based on timing</li>
                <li>• Ponzi Points: 1x multiplier</li>
              </ul>
              {selectedMode === 'simple' && (
                <div className="mc-status-green p-2 mt-3 text-center text-xs font-bold">Selected</div>
              )}
            </div>

            {/* Compounding */}
            <div
              onClick={() => { setSelectedMode('compounding'); setSelectedPlan(''); setClickError(''); }}
              className={`mc-card-select p-5 ${selectedMode === 'compounding' ? 'mc-active-purple' : ''}`}
            >
              <Flame className="h-8 w-8 mc-text-purple mb-3 mx-auto" />
              <h4 className="font-display text-base mc-text-purple text-center mb-1">Compounding Mode</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-3">For the true degenerate. Lock it up and pray.</p>
              <ul className="text-xs mc-text-muted space-y-1">
                <li>• Choose 15 or 30-day lockup below</li>
                <li>• Enhanced returns through compounding</li>
                <li>• Funds locked until maturity</li>
                <li>• Exit Toll: Flat 13% fee</li>
                <li>• Ponzi Points: 2x-3x multipliers</li>
              </ul>
              {selectedMode === 'compounding' && (
                <div className="mc-status-green p-2 mt-3 text-center text-xs font-bold">Selected</div>
              )}
            </div>
          </div>
        </div>

        {/* Step 2: Plan Length (compounding only) */}
        {selectedMode === 'compounding' && (
          <div className="mb-8">
            <div className="mc-label mb-2">Select Lockup Period</div>
            <div className="mc-status-gold p-3 text-xs text-center mb-4">
              <AlertTriangle className="h-3 w-3 inline mr-1" /> The longer the plan, the greater the ROI potential — but also the higher the risk the round ends first.
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div
                onClick={() => { setSelectedPlan('15-day-compounding'); setClickError(''); }}
                className={`mc-card-select p-4 text-center ${selectedPlan === '15-day-compounding' ? 'mc-active-gold' : ''}`}
              >
                <Rocket className="h-6 w-6 mc-text-gold mb-2 mx-auto" />
                <h4 className="font-display text-base mc-text-primary mb-1">15-Day Compounding</h4>
                <span className="text-xs mc-text-dim">12% daily — 2x Ponzi Points</span>
                {selectedPlan === '15-day-compounding' && (
                  <div className="mc-status-green p-1 mt-2 text-center text-xs font-bold">Selected</div>
                )}
              </div>
              <div
                onClick={() => { setSelectedPlan('30-day-compounding'); setClickError(''); }}
                className={`mc-card-select p-4 text-center ${selectedPlan === '30-day-compounding' ? 'mc-active-gold' : ''}`}
              >
                <Gem className="h-6 w-6 mc-text-gold mb-2 mx-auto" />
                <h4 className="font-display text-base mc-text-primary mb-1">30-Day Compounding</h4>
                <span className="text-xs mc-text-dim">9% daily — 3x Ponzi Points</span>
                {selectedPlan === '30-day-compounding' && (
                  <div className="mc-status-green p-1 mt-2 text-center text-xs font-bold">Selected</div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Final Step: Amount + CTA */}
        {((selectedMode === 'simple' && selectedPlan) || (selectedMode === 'compounding' && selectedPlan)) && (
          <div>
            <div className="mc-label mb-3">Enter Amount & Open Position</div>

            {/* Empty wallet prominent CTA */}
            {walletBalance < minDeposit && (
              <div className="mc-card mc-accent-gold p-6 mb-6 text-center">
                <Wallet className="h-10 w-10 mc-text-gold mb-3 mx-auto" />
                <p className="font-display text-base mc-text-primary mb-2">Fund Your Wallet First</p>
                <p className="text-sm mc-text-dim mb-1">You need at least {minDeposit} ICP to open a position.</p>
                <p className="text-xs mc-text-muted">Use the wallet dropdown in the top-right to deposit ICP.</p>
              </div>
            )}

            {walletBalance >= minDeposit && (
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
                {/* Amount input */}
                <div>
                  <div className="flex justify-between items-center mb-2">
                    <span className="text-xs font-bold mc-text-primary">Amount</span>
                    <span className="text-xs mc-text-dim">Available: {formatICP(walletBalance)} ICP</span>
                  </div>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setAmount(minDeposit.toString())}
                      disabled={walletBalance < minDeposit}
                      className={`mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap ${
                        walletBalance < minDeposit ? 'opacity-50 cursor-not-allowed' : ''
                      }`}
                    >MIN</button>
                    <input
                      type="number"
                      value={amount}
                      onChange={handleAmountChange}
                      placeholder={selectedMode === 'simple' ? `Min: ${minDeposit}, Max: ${formatICP(maxDeposit)} ICP` : `Min: ${minDeposit} ICP`}
                      min={minDeposit}
                      step="0.00000001"
                      className={`mc-input flex-1 text-center text-lg ${shakeInput ? 'mc-shake' : ''}`}
                    />
                    <button
                      onClick={() => {
                        const max = selectedMode === 'simple' ? Math.min(walletBalance, maxDeposit) : walletBalance;
                        setAmount(max.toString());
                      }}
                      disabled={!walletBalance || walletBalance < minDeposit}
                      className={`mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap ${
                        !walletBalance || walletBalance < minDeposit ? 'opacity-50 cursor-not-allowed' : ''
                      }`}
                    >MAX</button>
                  </div>

                  {depositAmount > 0 && (
                    <div className="mt-2 space-y-1 text-xs">
                      {inputError && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />{inputError}</div>}
                      {!inputError && depositAmount < minDeposit && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {minDeposit} ICP</div>}
                      {!inputError && depositAmount > walletBalance && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Insufficient balance</div>}
                      {!inputError && selectedMode === 'simple' && depositAmount > maxDeposit && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Max for simple mode: {formatICP(maxDeposit)} ICP</div>}
                      {!canDeposit && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Rate limit: 3 positions per hour max</div>}
                    </div>
                  )}

                  <div className="mc-status-blue p-3 mt-3 text-xs space-y-1">
                    <p>3% Entry Skim on every deposit</p>
                    <p>Half of skim + tolls seed the next round, half repay the House</p>
                    {selectedMode === 'simple' && <p>Simple max: 20% of pot or 5 ICP (whichever is higher)</p>}
                  </div>
                </div>

                {/* ROI Calculator */}
                <div>
                  {roiData ? (
                    <div>
                      <div className="text-center mb-3">
                        <span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span>
                      </div>
                      <div className="mc-card p-4 space-y-3">
                        <div className="flex justify-between items-center">
                          <div>
                            <div className="mc-label">{selectedMode === 'simple' ? 'Interest Payout' : 'Compounded Interest'}</div>
                            <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatICP(animatedReturn)} ICP</div>
                            <div className={`text-xs opacity-70 ${roiColor}`}>
                              {selectedMode === 'simple'
                                ? `${(roiData.totalReturn / depositAmount).toFixed(2)}x ROI (${roiData.roiPercent.toFixed(0)}%)`
                                : `${roiData.roiPercent.toFixed(1)}% ROI`}
                            </div>
                          </div>
                          <div className="text-right">
                            <div className="mc-label">Ponzi Points</div>
                            <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{Math.round(animatedPP).toLocaleString()}</div>
                            <div className="text-xs mc-text-purple opacity-70">
                              {selectedMode === 'simple' ? '1x' : selectedPlan === '15-day-compounding' ? '2x' : '3x'} multiplier
                            </div>
                          </div>
                        </div>
                        {/* Daily earnings line */}
                        <div className="border-t border-white/10 pt-3 flex items-center justify-between">
                          <div className="flex items-center gap-1.5">
                            <TrendingUp className="h-3.5 w-3.5 mc-text-cyan" />
                            <span className="text-xs mc-text-dim">Daily earnings</span>
                          </div>
                          <span className="text-sm font-bold mc-text-cyan">{formatICP(dailyEarnings)} ICP/day</span>
                        </div>
                      </div>
                    </div>
                  ) : (
                    <div className="h-full flex items-center justify-center text-center">
                      <div>
                        <BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" />
                        <p className="text-sm mc-text-muted">Enter an amount to see ROI</p>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Warning + CTA — warning first on mobile, CTA below */}
            {walletBalance >= minDeposit && (
              <div className="space-y-3">
                <div className="mc-status-red p-3 text-center text-sm font-bold">
                  <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
                  <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
                </div>
                <button
                  onClick={handleCreateGame}
                  disabled={createGameMutation.isPending}
                  className={`w-full py-4 text-base font-bold rounded-xl transition-all mc-btn-primary ${
                    hasValidAmount ? 'pulse' : ''
                  }`}
                >
                  {createGameMutation.isPending
                    ? 'Starting Game...'
                    : <><Dices className="h-4 w-4" /> START GAME</>}
                </button>
                {clickError && (
                  <p className={`text-xs mc-text-danger mt-2 text-center ${shakeError ? 'mc-shake' : ''}`}>
                    {clickError}
                  </p>
                )}
              </div>
            )}
          </div>
        )}

        {createGameMutation.isError && (
          <div className="mc-status-red p-3 mt-4 text-center text-sm">
            {createGameMutation.error?.message || 'Failed to start game'}
          </div>
        )}
      </div>

      {/* Post-deposit success toast */}
      {successToast && (
        <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]">
          <div className="mc-toast text-center">
            <div className="font-display text-xl mc-text-primary mb-2">You're In.</div>
            <p className="font-accent text-sm mc-text-dim italic mb-3">
              &ldquo;{successToast.quote}&rdquo;
            </p>
            <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>
            <p className="text-sm mc-text-dim mt-3 mb-4">
              <span className="mc-toast-accent">{formatICP(successToast.amount)} ICP</span> is now earning.
            </p>
            <div className="flex gap-3 justify-center">
              <button
                onClick={() => setSuccessToast(null)}
                className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
              >
                Stay Here
              </button>
              <button
                onClick={() => { setSuccessToast(null); onNavigateToProfitCenter?.(); }}
                className="mc-btn-primary px-5 py-2 rounded-full text-sm"
              >
                <><TrendingUp className="h-4 w-4" /> Watch It Climb</>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
