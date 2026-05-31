import { useState, useEffect, useMemo, useRef } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, BarChart3, TrendingUp, Dices, Loader2 } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyPendingSolIntents,
  useGetUserSolGames,
  usePokeMyDeposit,
  calculateSimpleROI,
  calculateCompoundingROI,
  getDailyRate,
  getPlanDays,
} from '../hooks/useQueries';
import { usePonziMathSolActor } from '../hooks/usePonziMathSolActor';
import { useWallet } from '../hooks/useWallet';
import { useCountUp } from '../hooks/useCountUp';
import { formatSOL, formatSolFloat, parseSOL, LAMPORTS_PER_SOL } from '../solana/lamports';
import { sendSolDeposit } from '../solana/sendSolDeposit';
import { COVER_CHARGE_RATE, MIN_DEPOSIT_SOL, pct } from '../lib/gameConstants';
import { investPlanToSolGamePlan, ppPerSolForPlan } from '../lib/solPlanMapping';

const MIN_LAMPORTS = parseSOL(String(MIN_DEPOSIT_SOL));

type Flow =
  | { kind: 'input' }
  | { kind: 'awaitingWallet'; lamports: bigint }
  | { kind: 'opening'; lamports: bigint; baselineGames: number }
  | { kind: 'manual'; depositAddress: string; lamports: bigint; baselineGames: number; note?: string }
  | { kind: 'opened' };

interface SolInvestPanelProps {
  planId: string;
  onNavigateToProfitCenter?: () => void;
}

function friendlyWalletError(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/reject|denied|cancel/i.test(msg)) return 'Wallet request was cancelled.';
  return msg || 'Could not complete the transfer from your wallet.';
}

export default function SolInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { actor } = usePonziMathSolActor();
  const { solanaPubkey } = useWallet();
  const prepareMut = usePrepareSolDeposit();
  const pokeMut = usePokeMyDeposit();
  const { data: pendingIntents } = useGetMyPendingSolIntents();
  const { data: solGames } = useGetUserSolGames();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [flow, setFlow] = useState<Flow>({ kind: 'input' });

  const isCompounding = planId === '15-day-compounding' || planId === '30-day-compounding';
  const days = getPlanDays(planId);
  const ppPerSol = ppPerSolForPlan(planId);

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);

  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const belowMin = lamports > 0n && lamports < MIN_LAMPORTS;
  const canDeposit = !!actor && !!solanaPubkey && lamports >= MIN_LAMPORTS && !prepareMut.isPending;

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

  // Count-up animation (matches the ICP panel). Reset token bumps when the
  // amount/plan changes; bumped in an effect (never mutated during render).
  const roiResetToken = useRef(0);
  const prevKey = useRef('');
  useEffect(() => {
    const key = `${lamports.toString()}-${planId}`;
    if (key !== prevKey.current) { roiResetToken.current += 1; prevKey.current = key; }
  }, [lamports, planId]);
  const animatedReturn = useCountUp(roi?.totalReturn || 0, 800, roiResetToken.current);
  const animatedPP = useCountUp(projectedPP, 800, roiResetToken.current);

  // Credit detection: while opening/manual, a new SOL game beyond the baseline
  // captured at entry means the position opened.
  useEffect(() => {
    if ((flow.kind === 'opening' || flow.kind === 'manual') && solGames) {
      if (solGames.length > flow.baselineGames) setFlow({ kind: 'opened' });
    }
  }, [flow, solGames]);

  // While opening or in the manual fallback, poke the canister to scan for the
  // deposit — immediately, then every 6s for ~36s — so the position opens within
  // seconds of the transfer landing rather than waiting for the 60s timer (which
  // still backstops). pokeMyDeposit is self-only, cooldown'd, and open-intent-
  // gated, so an early/no-op poke is cheap and harmless.
  useEffect(() => {
    if (flow.kind !== 'opening' && flow.kind !== 'manual') return;
    const poke = () => { pokeMut.mutateAsync().catch(() => {}); };
    poke();
    let tries = 0;
    const id = setInterval(() => {
      tries += 1;
      poke();
      if (tries >= 6) clearInterval(id);
    }, 6000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flow.kind]);

  const handleCopy = async (text: string) => {
    let ok = false;
    try {
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); ok = true; }
    } catch { ok = false; }
    if (!ok) {
      try {
        const ta = document.createElement('textarea');
        ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
        document.body.appendChild(ta); ta.focus(); ta.select();
        ok = document.execCommand('copy');
        document.body.removeChild(ta);
      } catch { ok = false; }
    }
    if (ok) { setCopied(true); setTimeout(() => setCopied(false), 1500); }
  };

  const handleOneClick = async () => {
    if (!canDeposit || !solanaPubkey) return;
    setFlow({ kind: 'awaitingWallet', lamports });
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ plan: investPlanToSolGamePlan(planId), expectedAmountLamports: lamports });
    } catch {
      setFlow({ kind: 'input' }); // prepareMut.isError surfaces the message in the input view
      return;
    }
    const baselineGames = solGames?.length ?? 0;
    try {
      await sendSolDeposit({ toAddress: prepared.depositAddress, lamports, expectedPubkey: solanaPubkey });
      setFlow({ kind: 'opening', lamports, baselineGames }); // the opening effect pokes for a fast credit

    } catch (e) {
      setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, baselineGames, note: friendlyWalletError(e) });
    }
  };

  const handleManual = async () => {
    if (!canDeposit) return;
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ plan: investPlanToSolGamePlan(planId), expectedAmountLamports: lamports });
    } catch { return; }
    setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, baselineGames: solGames?.length ?? 0 });
  };

  const handleCheckNow = () => { pokeMut.mutateAsync().catch(() => {}); };
  const handleStartOver = () => { setFlow({ kind: 'input' }); prepareMut.reset(); pokeMut.reset(); };

  if (!actor) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting your Solana session…</p>
      </div>
    );
  }

  const Devnet = (
    <div className="mc-status-amber p-3 text-center text-xs font-bold">
      <AlertTriangle className="h-4 w-4 inline mr-1" /> DEVNET — uses devnet SOL only. This position is funded on Solana devnet.
    </div>
  );

  if (flow.kind === 'opened') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <div className="font-display text-xl mc-text-primary">You're In.</div>
          <p className="text-sm mc-text-dim">Your position is open and earning.</p>
          <div className="flex gap-3 justify-center pt-1">
            <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">Open another</button>
            <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2">
              <TrendingUp className="h-4 w-4" /> Go to Profit Center
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (flow.kind === 'awaitingWallet') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
          <p className="text-sm mc-text-primary font-bold">Confirm the transfer in your wallet…</p>
          <p className="text-xs mc-text-dim">Approve the {formatSOL(flow.lamports)} SOL transfer in Phantom.</p>
        </div>
      </div>
    );
  }

  if (flow.kind === 'opening') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
          <p className="text-sm mc-text-primary font-bold">Opening your position…</p>
          <p className="text-xs mc-text-dim">Your {formatSOL(flow.lamports)} SOL transfer is confirmed — the position usually opens within a few seconds (up to a minute).</p>
          <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-secondary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2 mt-2">
            <TrendingUp className="h-4 w-4" /> Go to Profit Center
          </button>
        </div>
      </div>
    );
  }

  if (flow.kind === 'manual') {
    const qrPayload = `solana:${flow.depositAddress}?amount=${formatSOL(flow.lamports)}`;
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-4 space-y-3 max-w-md mx-auto">
          {flow.note && (
            <div className="mc-status-red p-2 text-xs text-center">{flow.note} Send manually below — your position will still open.</div>
          )}
          <div className="text-center">
            <div className="mc-label">Send exactly</div>
            <div className="text-2xl font-bold mc-text-gold">{formatSOL(flow.lamports)} SOL</div>
            <div className="text-xs mc-text-dim mt-1">devnet SOL from your wallet — the position opens automatically within ~a minute.</div>
          </div>
          <div className="mc-label">Deposit address</div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={flow.depositAddress}>{flow.depositAddress}</code>
            <button onClick={() => handleCopy(flow.depositAddress)} className="mc-btn-secondary text-xs">
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>
          <div className="flex justify-center pt-2">
            <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
          </div>
          {pendingIntents && pendingIntents.length > 0 && (
            <div className="text-[10px] mc-text-muted text-center">{pendingIntents.length} pending deposit{pendingIntents.length === 1 ? '' : 's'} awaiting confirmation</div>
          )}
          <div className="flex gap-2 justify-center pt-2 flex-wrap">
            <button onClick={handleCheckNow} disabled={pokeMut.isPending} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">
              {pokeMut.isPending ? 'Checking…' : 'Check now'}
            </button>
            <button onClick={handleStartOver} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">Start over</button>
            <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-primary px-4 py-2 rounded-full text-sm inline-flex items-center gap-2">
              <TrendingUp className="h-4 w-4" /> Profit Center
            </button>
          </div>
        </div>
      </div>
    );
  }

  // input (default)
  return (
    <div className="space-y-6">
      {Devnet}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div>
          <div className="flex justify-between items-center mb-2">
            <span className="text-xs font-bold mc-text-primary">Amount</span>
          </div>
          <div className="flex gap-2">
            <button onClick={() => setSolInput(String(MIN_DEPOSIT_SOL))} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap">MIN</button>
            <input type="text" inputMode="decimal" value={solInput} onChange={(e) => setSolInput(e.target.value)} placeholder={`Min: ${MIN_DEPOSIT_SOL} SOL`} className="mc-input flex-1 text-center text-lg font-mono" />
          </div>
          {belowMin && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {MIN_DEPOSIT_SOL} SOL</div>)}
          {!solanaPubkey && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Reconnect your Solana wallet to deposit.</div>)}

          <div className="mc-status-red p-3 text-center text-sm font-bold mt-3">
            <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
            <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
          </div>

          <button onClick={handleOneClick} disabled={!canDeposit} className={`w-full py-3 mt-3 text-sm font-bold rounded-xl transition-all mc-btn-primary inline-flex items-center justify-center gap-2 ${canDeposit ? 'pulse' : ''}`}>
            {prepareMut.isPending ? 'Starting…' : <><Dices className="h-5 w-5" /> DEPOSIT WITH PHANTOM</>}
          </button>

          {prepareMut.isError && (<p className="text-xs mc-text-danger mt-2 text-center">{(prepareMut.error as Error).message}</p>)}

          <button onClick={handleManual} disabled={!canDeposit} className="w-full text-xs mc-text-muted hover:mc-text-primary transition-colors mt-2 underline">
            or get a deposit address to send manually
          </button>
        </div>

        <div>
          {roi ? (
            <div>
              <div className="text-center mb-3"><span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span></div>
              <div className="mc-card p-4 space-y-3">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="mc-label">{isCompounding ? 'Compounded Interest' : 'Interest Payout'}</div>
                    <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatSolFloat(animatedReturn)} SOL</div>
                    <div className={`text-xs opacity-70 ${roiColor}`}>
                      {isCompounding ? `${roi.roiPercent.toFixed(1)}% ROI` : `${(roi.totalReturn / net).toFixed(2)}x ROI (${roi.roiPercent.toFixed(0)}%)`}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="mc-label">Ponzi Points</div>
                    <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{Math.round(animatedPP).toLocaleString()}</div>
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
                    <div className="flex items-center gap-1.5"><TrendingUp className="h-3.5 w-3.5 mc-text-cyan" /><span className="text-xs mc-text-dim">Daily earnings</span></div>
                    <span className="text-sm font-bold mc-text-cyan">{formatSolFloat(dailyEarnings)} SOL/day</span>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="h-full flex items-center justify-center text-center">
              <div><BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" /><p className="text-sm mc-text-muted">Enter an amount to see projected returns</p></div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
