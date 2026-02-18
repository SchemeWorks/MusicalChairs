import { useState, useEffect, useRef } from 'react';
import { useCreateGame, useGetInternalWalletBalance, useGetMaxDepositLimit, useCheckDepositRateLimit, calculateSimpleROI, calculateCompoundingROI, getDailyRate, getPlanDays, calculatePonziPoints } from '../hooks/useQueries';
import { useCountUp } from '../hooks/useCountUp';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Sprout, Flame, Rocket, Gem, BarChart3, AlertTriangle, Dices, Wallet, TrendingUp, ChevronRight } from 'lucide-react';

/* ================================================================
   Charles quotes — per-phase, random on mount
   ================================================================ */

const charlesModeQuotes = [
  "Let me ask you something. Do you want to drive a Honda... or a Lamborghini?",
  "Everybody starts somewhere. Some people start on a yacht.",
  "Two doors. One leads to a paycheck. The other leads to a lifestyle.",
  "I'm going to show you two paths. One of them is going to change your life.",
];

const charlesPlanQuotes = [
  "Now we're talking. Do you want the standard package, or the executive suite?",
  "I don't show this tier to just anyone. You've earned a look at both options.",
  "This is where it gets interesting. Two packages — both exclusive. One is just a little more exclusive.",
  "You want to retire early, or retire really early?",
];

const charlesAmountQuotes = [
  "Here's the real question: how much do you want to make?",
  "Winners don't think about what they're spending. They think about what they're earning.",
  "This is the part where you decide if you're serious or just window shopping.",
  "The only limit is your imagination. Well, that and your wallet balance.",
];

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

type Phase = 1 | 2 | 3;

interface GamePlansProps {
  onNavigateToProfitCenter?: () => void;
}

export default function GamePlans({ onNavigateToProfitCenter }: GamePlansProps) {
  // Existing state
  const [selectedMode, setSelectedMode] = useState<'simple' | 'compounding' | ''>('');
  const [selectedPlan, setSelectedPlan] = useState('');
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState('');
  const [successToast, setSuccessToast] = useState<{ quote: string; amount: number } | null>(null);
  const [shakeInput, setShakeInput] = useState(false);
  const [clickError, setClickError] = useState('');
  const [shakeError, setShakeError] = useState(false);

  // Progressive reveal state
  const [phase, setPhase] = useState<Phase>(1);
  const [phaseTransitioning, setPhaseTransitioning] = useState(false);
  const [charlesQuotes] = useState(() => ({
    mode: charlesModeQuotes[Math.floor(Math.random() * charlesModeQuotes.length)],
    plan: charlesPlanQuotes[Math.floor(Math.random() * charlesPlanQuotes.length)],
    amount: charlesAmountQuotes[Math.floor(Math.random() * charlesAmountQuotes.length)],
  }));

  const phaseRef = useRef<HTMLDivElement>(null);

  const triggerShake = () => {
    setShakeInput(true);
    setTimeout(() => setShakeInput(false), 400);
  };

  const triggerClickError = (msg: string) => {
    setClickError(msg);
    setShakeError(true);
    setTimeout(() => setShakeError(false), 400);
  };

  // Hooks
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
    setClickError('');
  };

  // ROI calculation
  const [roiData, setRoiData] = useState<{
    totalReturn: number; profit: number; roiPercent: number; ponziPoints: number;
  } | null>(null);

  const roiResetToken = useRef(0);
  const prevRoiKey = useRef('');

  useEffect(() => {
    if (depositAmount > 0 && selectedPlan && selectedMode) {
      const days = getPlanDays(selectedPlan);
      const roi = selectedMode === 'simple'
        ? calculateSimpleROI(depositAmount, selectedPlan, days)
        : calculateCompoundingROI(depositAmount, selectedPlan, days);
      setRoiData({ ...roi, ponziPoints: calculatePonziPoints(depositAmount, selectedPlan, selectedMode) });
      const key = `${depositAmount}-${selectedPlan}-${selectedMode}`;
      if (key !== prevRoiKey.current) {
        roiResetToken.current += 1;
        prevRoiKey.current = key;
      }
    } else {
      setRoiData(null);
    }
  }, [depositAmount, selectedPlan, selectedMode]);

  const animatedReturn = useCountUp(roiData?.totalReturn || 0, 800, roiResetToken.current);
  const animatedPP = useCountUp(roiData?.ponziPoints || 0, 800, roiResetToken.current);

  const roiColor = !roiData ? 'mc-text-green' :
    roiData.roiPercent < 50 ? 'mc-text-green' :
    roiData.roiPercent < 200 ? 'mc-text-purple mc-glow-purple' :
    'mc-text-gold mc-glow-gold';

  const dailyEarnings = depositAmount > 0 && selectedPlan && selectedMode
    ? depositAmount * getDailyRate(selectedPlan)
    : 0;

  // Phase transitions
  const advancePhase = (nextPhase: Phase) => {
    if (phaseTransitioning) return;
    setPhaseTransitioning(true);
    setTimeout(() => {
      setPhase(nextPhase);
      setPhaseTransitioning(false);
      setTimeout(() => phaseRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' }), 50);
    }, 200);
  };

  const handleSelectMode = (mode: 'simple' | 'compounding') => {
    setSelectedMode(mode);
    setClickError('');
    if (mode === 'simple') {
      setSelectedPlan('21-day-simple');
      advancePhase(3);
    } else {
      setSelectedPlan('');
      advancePhase(2);
    }
  };

  const handleSelectPlan = (planId: string) => {
    setSelectedPlan(planId);
    setClickError('');
    advancePhase(3);
  };

  const goBackToPhase = (targetPhase: Phase) => {
    if (phaseTransitioning) return;
    setPhaseTransitioning(true);
    setTimeout(() => {
      if (targetPhase <= 1) {
        setSelectedMode(''); setSelectedPlan(''); setAmount(''); setInputError(''); setClickError('');
      } else if (targetPhase <= 2) {
        setSelectedPlan(''); setAmount(''); setInputError(''); setClickError('');
      }
      setPhase(targetPhase);
      setPhaseTransitioning(false);
    }, 200);
  };

  const handleCreateGame = async () => {
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
      setPhase(1);
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

  // Current phase's Charles quote
  const currentCharlesQuote = phase === 1 ? charlesQuotes.mode
    : phase === 2 ? charlesQuotes.plan
    : charlesQuotes.amount;

  return (
    <div className="space-y-4">
      {/* Charles Quote Banner */}
      <div key={phase} className="mc-charles-banner mc-charles-enter">
        <p className="font-accent text-sm md:text-base mc-text-dim italic leading-relaxed">
          &ldquo;{currentCharlesQuote}&rdquo;
        </p>
        <span className="text-xs mc-text-muted font-bold mt-1 block">&mdash; Charles</span>
      </div>

      {/* Summary Strips — collapsed previous selections */}
      {phase > 1 && (
        <div
          onClick={() => goBackToPhase(1)}
          className={`mc-summary-strip mc-strip-enter ${
            selectedMode === 'simple' ? 'mc-summary-strip-green' : 'mc-summary-strip-purple'
          }`}
        >
          {selectedMode === 'simple'
            ? <Sprout className="h-4 w-4 mc-text-green shrink-0" />
            : <Flame className="h-4 w-4 mc-text-purple shrink-0" />}
          <span className="text-sm mc-text-primary font-bold">
            {selectedMode === 'simple' ? 'The Starter Package' : 'The VIP Experience'}
          </span>
          <span className="text-xs mc-text-dim hidden sm:inline">
            {selectedMode === 'simple' ? '21 days · 11%/day' : 'Compounding returns'}
          </span>
          <span className="ml-auto text-xs mc-text-muted hover:mc-text-primary transition-colors">Change</span>
        </div>
      )}

      {phase > 2 && selectedMode === 'compounding' && (
        <div
          onClick={() => goBackToPhase(2)}
          className="mc-summary-strip mc-strip-enter mc-summary-strip-gold"
        >
          {selectedPlan === '15-day-compounding'
            ? <Rocket className="h-4 w-4 mc-text-gold shrink-0" />
            : <Gem className="h-4 w-4 mc-text-gold shrink-0" />}
          <span className="text-sm mc-text-primary font-bold">
            {selectedPlan === '15-day-compounding' ? 'The Executive Package' : "The Chairman's Circle"}
          </span>
          <span className="text-xs mc-text-dim hidden sm:inline">
            {selectedPlan === '15-day-compounding' ? '15 days · 12%/day · 2x PP' : '30 days · 9%/day · 3x PP'}
          </span>
          <span className="ml-auto text-xs mc-text-muted hover:mc-text-primary transition-colors">Change</span>
        </div>
      )}

      {/* Active Phase Content */}
      <div
        ref={phaseRef}
        className={phaseTransitioning ? 'mc-phase-out' : 'mc-phase-in'}
        key={`phase-${phase}`}
      >
        {/* ============ PHASE 1: Mode Selection ============ */}
        {phase === 1 && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* The Starter Package (Simple) */}
            <div
              onClick={() => handleSelectMode('simple')}
              className="mc-card-select p-6 group cursor-pointer"
            >
              <Sprout className="h-10 w-10 mc-text-green mb-4 mx-auto" />
              <h4 className="font-display text-lg mc-text-green text-center mb-2">The Starter Package</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-4">
                Consistent returns. Cash out whenever you want. Smart money moves at its own pace.
              </p>
              <ul className="text-xs mc-text-muted space-y-1.5">
                <li>• 21 days of 11% daily returns</li>
                <li>• Withdraw your earnings anytime</li>
                <li>• Exit Toll: 7% / 5% / 3% based on timing</li>
                <li>• Ponzi Points: 1x multiplier</li>
              </ul>
              <div className="mt-4 flex items-center justify-center gap-1 text-xs mc-text-green opacity-0 group-hover:opacity-100 transition-opacity">
                Select <ChevronRight className="h-3 w-3" />
              </div>
            </div>

            {/* The VIP Experience (Compounding) */}
            <div
              onClick={() => handleSelectMode('compounding')}
              className="mc-card-select p-6 group cursor-pointer"
            >
              <Flame className="h-10 w-10 mc-text-purple mb-4 mx-auto" />
              <h4 className="font-display text-lg mc-text-purple text-center mb-2">The VIP Experience</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-4">
                This is where the real money is. Lock it in and let compounding do the heavy lifting.
              </p>
              <ul className="text-xs mc-text-muted space-y-1.5">
                <li>• Enhanced returns through compounding</li>
                <li>• Choose 15 or 30-day lockup</li>
                <li>• Exit Toll: Flat 13% fee</li>
                <li>• Ponzi Points: 2x–3x multipliers</li>
              </ul>
              <div className="mt-4 flex items-center justify-center gap-1 text-xs mc-text-purple opacity-0 group-hover:opacity-100 transition-opacity">
                Select <ChevronRight className="h-3 w-3" />
              </div>
            </div>
          </div>
        )}

        {/* ============ PHASE 2: Plan Selection (Compounding only) ============ */}
        {phase === 2 && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* The Executive Package (15-day) */}
            <div
              onClick={() => handleSelectPlan('15-day-compounding')}
              className="mc-card-select p-6 group cursor-pointer"
            >
              <Rocket className="h-10 w-10 mc-text-gold mb-4 mx-auto" />
              <h4 className="font-display text-lg mc-text-gold text-center mb-2">The Executive Package</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-4">
                15 days. 12% daily. The fast track for people who know what they want.
              </p>
              <ul className="text-xs mc-text-muted space-y-1.5">
                <li>• 12% compounding daily for 15 days</li>
                <li>• 2x Ponzi Points multiplier</li>
                <li>• Funds locked until maturity</li>
              </ul>
              <div className="mt-4 flex items-center justify-center gap-1 text-xs mc-text-gold opacity-0 group-hover:opacity-100 transition-opacity">
                Select <ChevronRight className="h-3 w-3" />
              </div>
            </div>

            {/* The Chairman's Circle (30-day) */}
            <div
              onClick={() => handleSelectPlan('30-day-compounding')}
              className="mc-card-select p-6 group cursor-pointer"
            >
              <Gem className="h-10 w-10 mc-text-gold mb-4 mx-auto" />
              <h4 className="font-display text-lg mc-text-gold text-center mb-2">The Chairman's Circle</h4>
              <p className="font-accent text-xs mc-text-dim text-center italic mb-4">
                30 days. 9% daily. 3x points. The big picture play for serious investors.
              </p>
              <ul className="text-xs mc-text-muted space-y-1.5">
                <li>• 9% compounding daily for 30 days</li>
                <li>• 3x Ponzi Points multiplier</li>
                <li>• Funds locked until maturity</li>
              </ul>
              <div className="mt-4 flex items-center justify-center gap-1 text-xs mc-text-gold opacity-0 group-hover:opacity-100 transition-opacity">
                Select <ChevronRight className="h-3 w-3" />
              </div>
            </div>
          </div>
        )}

        {/* ============ PHASE 3: Amount + ROI + CTA ============ */}
        {phase === 3 && (
          <div className="space-y-6">
            {/* Empty wallet CTA */}
            {walletBalance < minDeposit && (
              <div className="mc-card mc-accent-gold p-6 text-center">
                <Wallet className="h-10 w-10 mc-text-gold mb-3 mx-auto" />
                <p className="font-display text-base mc-text-primary mb-2">Fund Your Wallet First</p>
                <p className="text-sm mc-text-dim mb-1">You need at least {minDeposit} ICP to open a position.</p>
                <p className="text-xs mc-text-muted">Use the wallet dropdown in the top-right to deposit ICP.</p>
              </div>
            )}

            {walletBalance >= minDeposit && (
              <>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
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
                          <p className="text-sm mc-text-muted">Enter an amount to see projected returns</p>
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                {/* Warning + CTA */}
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
              </>
            )}

            {createGameMutation.isError && (
              <div className="mc-status-red p-3 text-center text-sm">
                {createGameMutation.error?.message || 'Failed to start game'}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Post-deposit success toast */}
      {successToast && (
        <div className="fixed inset-0 z-[9999] flex items-center justify-center p-4">
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
