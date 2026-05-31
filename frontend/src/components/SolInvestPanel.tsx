import { useMemo, useState } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, BarChart3, TrendingUp, Dices } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyPendingSolIntents,
  calculateSimpleROI,
  calculateCompoundingROI,
  getDailyRate,
  getPlanDays,
} from '../hooks/useQueries';
import { usePonziMathSolActor } from '../hooks/usePonziMathSolActor';
import { formatSOL, formatSolFloat, parseSOL, LAMPORTS_PER_SOL } from '../solana/lamports';
import { COVER_CHARGE_RATE, MIN_DEPOSIT_SOL, pct } from '../lib/gameConstants';
import { investPlanToSolGamePlan, ppPerSolForPlan } from '../lib/solPlanMapping';

// Computed once: 0.01 SOL as lamports.
const MIN_LAMPORTS = parseSOL(String(MIN_DEPOSIT_SOL));

interface SolInvestPanelProps {
  planId: string;
  onNavigateToProfitCenter?: () => void;
}

export default function SolInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { actor } = usePonziMathSolActor();
  const prepareMut = usePrepareSolDeposit();
  const { data: pendingIntents } = useGetMyPendingSolIntents();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [intentResult, setIntentResult] = useState<
    { intentId: bigint; depositAddress: string; lamports: bigint } | null
  >(null);

  const isCompounding = planId === '15-day-compounding' || planId === '30-day-compounding';
  const days = getPlanDays(planId);
  const ppPerSol = ppPerSolForPlan(planId);

  const lamports = useMemo(() => {
    try {
      return solInput.trim() ? parseSOL(solInput) : 0n;
    } catch {
      return 0n;
    }
  }, [solInput]);

  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const belowMin = lamports > 0n && lamports < MIN_LAMPORTS;
  const canReserve = !!actor && lamports >= MIN_LAMPORTS && !prepareMut.isPending && !intentResult;

  // ROI mirrors the ICP panel exactly: projected on the NET deposit; PP on gross.
  const net = solFloat * (1 - COVER_CHARGE_RATE);
  const roi = solFloat > 0
    ? (isCompounding ? calculateCompoundingROI(net, planId, days) : calculateSimpleROI(net, planId, days))
    : null;
  const projectedPP = solFloat > 0 ? Math.round(solFloat * ppPerSol) : 0;
  const dailyEarnings = roi ? net * getDailyRate(planId) : 0;

  const roiColor = !roi ? 'mc-text-green'
    : roi.roiPercent < 50 ? 'mc-text-green'
    : roi.roiPercent < 200 ? 'mc-text-purple mc-glow-purple'
    : 'mc-text-gold mc-glow-gold';

  const handleReserve = async () => {
    if (!canReserve) return;
    try {
      const result = await prepareMut.mutateAsync({
        plan: investPlanToSolGamePlan(planId),
        expectedAmountLamports: lamports,
      });
      setIntentResult({ intentId: result.intentId, depositAddress: result.depositAddress, lamports });
    } catch {
      // surfaces via prepareMut.isError below
    }
  };

  const handleStartOver = () => {
    setIntentResult(null);
    prepareMut.reset();
  };

  const handleCopy = async () => {
    if (!intentResult) return;
    await navigator.clipboard.writeText(intentResult.depositAddress);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (!actor) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting your Solana session…</p>
      </div>
    );
  }

  const qrPayload = intentResult
    ? `solana:${intentResult.depositAddress}?amount=${formatSOL(intentResult.lamports)}`
    : null;

  return (
    <div className="space-y-6">
      <div className="mc-status-amber p-3 text-center text-xs font-bold">
        <AlertTriangle className="h-4 w-4 inline mr-1" /> DEVNET — send devnet SOL only. This position is funded on Solana devnet.
      </div>

      {!intentResult ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Amount input + CTA */}
          <div>
            <div className="flex justify-between items-center mb-2">
              <span className="text-xs font-bold mc-text-primary">Amount</span>
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => setSolInput(String(MIN_DEPOSIT_SOL))}
                className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap"
              >MIN</button>
              <input
                type="text"
                inputMode="decimal"
                value={solInput}
                onChange={(e) => setSolInput(e.target.value)}
                placeholder={`Min: ${MIN_DEPOSIT_SOL} SOL`}
                className="mc-input flex-1 text-center text-lg font-mono"
              />
            </div>

            {belowMin && (
              <div className="mt-2 text-xs mc-text-danger">
                <AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {MIN_DEPOSIT_SOL} SOL
              </div>
            )}

            <div className="mc-status-red p-3 text-center text-sm font-bold mt-3">
              <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
              <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
            </div>

            <button
              onClick={handleReserve}
              disabled={!canReserve}
              className={`w-full py-3 mt-3 text-sm font-bold rounded-xl transition-all mc-btn-primary inline-flex items-center justify-center gap-2 ${canReserve ? 'pulse' : ''}`}
            >
              {prepareMut.isPending
                ? 'Reserving…'
                : <><Dices className="h-5 w-5" /> RESERVE DEPOSIT ADDRESS</>}
            </button>

            {prepareMut.isError && (
              <p className="text-xs mc-text-danger mt-2 text-center">
                {(prepareMut.error as Error).message}
              </p>
            )}
          </div>

          {/* ROI calculator — same figures/breakdown as the ICP panel, SOL-denominated */}
          <div>
            {roi ? (
              <div>
                <div className="text-center mb-3">
                  <span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span>
                </div>
                <div className="mc-card p-4 space-y-3">
                  <div className="flex justify-between items-center">
                    <div>
                      <div className="mc-label">{isCompounding ? 'Compounded Interest' : 'Interest Payout'}</div>
                      <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatSolFloat(roi.totalReturn)} SOL</div>
                      <div className={`text-xs opacity-70 ${roiColor}`}>
                        {isCompounding
                          ? `${roi.roiPercent.toFixed(1)}% ROI`
                          : `${(roi.totalReturn / net).toFixed(2)}x ROI (${roi.roiPercent.toFixed(0)}%)`}
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="mc-label">Ponzi Points</div>
                      <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{projectedPP.toLocaleString()}</div>
                      <div className="text-xs mc-text-purple opacity-70">{ppPerSol.toLocaleString()} / SOL</div>
                    </div>
                  </div>
                  <div className="border-t border-white/10 pt-3 space-y-1.5">
                    <div className="flex justify-between text-sm">
                      <span className="mc-text-muted">Front-End Load ({pct(COVER_CHARGE_RATE)})</span>
                      <span className="mc-text-primary font-medium">-{formatSolFloat(solFloat * COVER_CHARGE_RATE)} SOL</span>
                    </div>
                    <div className="flex justify-between text-sm">
                      <span className="mc-text-muted">Net deposit</span>
                      <span className="mc-text-primary font-medium">{formatSolFloat(net)} SOL</span>
                    </div>
                    <div className="flex items-center justify-between pt-1">
                      <div className="flex items-center gap-1.5">
                        <TrendingUp className="h-3.5 w-3.5 mc-text-cyan" />
                        <span className="text-xs mc-text-dim">Daily earnings</span>
                      </div>
                      <span className="text-sm font-bold mc-text-cyan">{formatSolFloat(dailyEarnings)} SOL/day</span>
                    </div>
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
      ) : (
        /* Reserved state — locked amount, address, QR, pending */
        <div className="mc-card p-4 space-y-3 max-w-md mx-auto">
          <div className="text-center">
            <div className="mc-label">Send exactly</div>
            <div className="text-2xl font-bold mc-text-gold">{formatSOL(intentResult.lamports)} SOL</div>
            <div className="text-xs mc-text-dim mt-1">
              devnet SOL from Phantom — your position opens automatically within ~a minute.
            </div>
          </div>

          <div className="mc-label">Deposit address</div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={intentResult.depositAddress}>
              {intentResult.depositAddress}
            </code>
            <button onClick={handleCopy} className="mc-btn-secondary text-xs">
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>

          {qrPayload && (
            <div className="flex justify-center pt-2">
              <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
            </div>
          )}

          {pendingIntents && pendingIntents.length > 0 && (
            <div className="text-[10px] mc-text-muted text-center">
              {pendingIntents.length} pending deposit{pendingIntents.length === 1 ? '' : 's'} awaiting confirmation
            </div>
          )}

          <div className="flex gap-3 justify-center pt-2">
            <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">
              Start over
            </button>
            <button
              onClick={() => onNavigateToProfitCenter?.()}
              className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2"
            >
              <TrendingUp className="h-4 w-4" /> Go to Profit Center
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
