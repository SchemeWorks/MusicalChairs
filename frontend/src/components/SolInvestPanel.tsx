import { useState, useEffect, useMemo, useRef, type ReactNode } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, BarChart3, TrendingUp, Dices, Loader2 } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyPendingSolIntents,
  useGetUserSolGames,
  usePokeMyDeposit,
  useSolBalance,
  calculateSimpleROI,
  calculateCompoundingROI,
  getDailyRate,
  getPlanDays,
} from '../hooks/useQueries';
import { useNativeTxStatus } from '../hooks/useNativeSolana';
import { useNativeActiveConfig, useNativeAvailability, useNativeRefresh, usePanelNativeTask } from '../solana/native/panelsHooks';
import { describePrepStage, openPrereqs, runPrepared } from '../solana/native/panelsPrereqs';
import { LOCAL_CLUSTER_MODE } from '../solana/native/localModeFlag';
import { describeUnavailable, railOf } from '../solana/native/panelsRail';
import { projectSimpleDailyLamports } from '../solana/native/economics';
import { describeNativeError, planKindFromInvestId, projectOpen } from '../solana/native/panelsQuote';
import { checkTxStatus, describeTxStatus, furtherStatus, resendSameTx, shortSignature, type StatusActionResult, type TxStatusView } from '../solana/native/panelsStatus';
import { describeOrigin } from '../solana/native/panelsCluster';
import type { NativeConfigVersion, NativeTxResult, PlanKind, StatusUpdate } from '../solana/native/types';
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
  | { kind: 'awaitingWallet'; lamports: bigint; depositAddress?: string; baselineGames?: number }
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

// ---------------------------------------------------------------------------
// Legacy rail: ponzi_math_sol deposit intents (Chain Fusion). Unchanged; used only
// while the native program is not available on the connected cluster.
// ---------------------------------------------------------------------------
function LegacySolInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { actor } = usePonziMathSolActor();
  const { solanaPubkey } = useWallet();
  const prepareMut = usePrepareSolDeposit();
  const pokeMut = usePokeMyDeposit();
  const { data: pendingIntents } = useGetMyPendingSolIntents();
  const { data: solGames } = useGetUserSolGames();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [flow, setFlow] = useState<Flow>({ kind: 'input' });
  // Monotonic id for the active one-click op. Bumped on each start AND on cancel,
  // so a slow/hung sendSolDeposit that finally settles after the user bailed can't
  // stomp the UI (its captured gen no longer matches opGenRef.current).
  const opGenRef = useRef(0);

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
    const gen = ++opGenRef.current;
    setFlow({ kind: 'awaitingWallet', lamports });
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ plan: investPlanToSolGamePlan(planId), expectedAmountLamports: lamports });
    } catch {
      if (opGenRef.current === gen) setFlow({ kind: 'input' }); // prepareMut.isError surfaces the message in the input view
      return;
    }
    if (opGenRef.current !== gen) return; // cancelled while preparing
    const baselineGames = solGames?.length ?? 0;
    // Stash the prepared address in the spinner state so Cancel can drop to the
    // manual view (and the credit detector still has a baseline) if the wallet hangs.
    setFlow({ kind: 'awaitingWallet', lamports, depositAddress: prepared.depositAddress, baselineGames });
    try {
      await sendSolDeposit({ toAddress: prepared.depositAddress, lamports, expectedPubkey: solanaPubkey });
      if (opGenRef.current !== gen) return; // user bailed; ignore the late success
      setFlow({ kind: 'opening', lamports, baselineGames }); // the opening effect pokes for a fast credit
    } catch (e) {
      if (opGenRef.current !== gen) return; // user bailed; ignore the late failure
      setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, baselineGames, note: `${friendlyWalletError(e)} Send manually below — your position will still open.` });
    }
  };

  // Escape hatch from the "Confirm in your wallet…" spinner (e.g. Phantom popup
  // hung or dismissed without resolving). We can't truly abort the wallet promise,
  // so we bump opGenRef (its late settle is ignored) and drop to the manual view
  // when an address was prepared — the deposit detector keeps polling, so a transfer
  // the user DID approve still opens. Copy avoids implying a (double-)send is needed.
  const handleCancelWallet = () => {
    opGenRef.current += 1;
    if (flow.kind === 'awaitingWallet' && flow.depositAddress) {
      setFlow({
        kind: 'manual',
        depositAddress: flow.depositAddress,
        lamports: flow.lamports,
        baselineGames: flow.baselineGames ?? (solGames?.length ?? 0),
        note: "Stopped waiting on your wallet. If you already approved the transfer in Phantom, your position opens automatically — don't send again. Otherwise, send manually below or start over.",
      });
    } else {
      setFlow({ kind: 'input' });
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

  // Mainnet-beta as of the M3 cutover — no devnet warning.
  const Devnet = null;

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
          <button onClick={handleCancelWallet} className="mc-btn-secondary px-4 py-1.5 rounded-full text-xs mt-1">Cancel</button>
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
            <div className="mc-status-red p-2 text-xs text-center">{flow.note}</div>
          )}
          <div className="text-center">
            <div className="mc-label">Send exactly</div>
            <div className="text-2xl font-bold mc-text-gold">{formatSOL(flow.lamports)} SOL</div>
            <div className="text-xs mc-text-dim mt-1">SOL from your wallet — the position opens automatically within ~a minute.</div>
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

// ---------------------------------------------------------------------------
// Native rail: open_position on the Musical Chairs program (IMPL-46c)
// ---------------------------------------------------------------------------

/** Display-only reserve for network fees and the one-time rent of the Position, User and UserRound accounts the wallet pays. */
const NATIVE_FEE_AND_RENT_RESERVE_LAMPORTS = 10_000_000n;

/**
 * Shown on EVERY legacy-only fallback (invest, Series A, positions, backers, ledger): the native program
 * is not available on the connected cluster, so what follows is the legacy ponzi_math_sol canister rail.
 * Native and legacy are never mixed under one unlabeled view (DR-22).
 */
export function LegacyRailBanner({ what }: { what: string }) {
  return (
    <div className="mc-status-gold p-3 text-xs space-y-0.5" data-testid="legacy-rail-banner" data-rail="legacy">
      <div className="font-bold">Legacy (Chain Fusion)</div>
      <p>The native program isn't available on this cluster, so {what} use the legacy ponzi_math_sol canister rail.</p>
    </div>
  );
}

/** While native availability is being verified no rail is offered, in particular no legacy action as if native had failed. */
export function NativeRailVerifying({ what }: { what: string }) {
  return (
    <div className="mc-card p-6 text-center space-y-2" data-testid="native-rail-verifying" data-rail="verifying">
      <Loader2 className="h-6 w-6 mc-text-gold mx-auto animate-spin" />
      <p className="text-sm mc-text-primary font-bold">Verifying the native program on this cluster…</p>
      <p className="text-xs mc-text-dim">{what} will appear once it is verified.</p>
    </div>
  );
}

const TONE_CLASS: Record<TxStatusView['tone'], string> = {
  pending: 'mc-status-blue',
  success: 'mc-status-green',
  warning: 'mc-status-gold',
  danger: 'mc-status-red',
};

/**
 * Live confirmation ladder for one native transaction: submitted -> confirmed ->
 * finalized, or unknown / failed / expired. The signature (and open_position's
 * nonce) stay on screen; `unknown` offers "Check status", which re-polls the SAME
 * signature and never builds a new transaction. Shared by the invest, positions
 * and backer panels.
 */
export function NativeTxStatusCard({
  tx,
  label,
  onTryAgain,
  actions,
  compact = false,
  status,
}: {
  tx: NativeTxResult;
  /** Capitalised noun phrase: "Your deposit", "Your withdrawal". */
  label: string;
  onTryAgain?: () => void;
  /** Extra buttons rendered under the status (receives the current view). */
  actions?: (view: TxStatusView) => ReactNode;
  compact?: boolean;
  /** The caller already follows this transaction (useNativeTxStatus): pass its latest value to avoid a second poller. */
  status?: StatusUpdate | null;
}) {
  const own = useNativeTxStatus(status === undefined ? tx : null);
  const latest = status === undefined ? own : status;
  const refresh = useNativeRefresh();
  const [override, setOverride] = useState<StatusUpdate | null>(null);
  const [busy, setBusy] = useState<'check' | 'resend' | null>(null);
  const [confirmResend, setConfirmResend] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    setOverride(null);
    setNote(null);
    setActionError(null);
    setConfirmResend(false);
  }, [tx]);

  const update = furtherStatus(latest ?? { signature: tx.signature, status: tx.status }, override);
  const view = describeTxStatus(update, { label });
  const origin = describeOrigin(tx.origin);

  const apply = (result: StatusActionResult) => {
    if (result.ok) {
      setOverride(result.update);
      setNote(result.note);
      setActionError(null);
      if (result.update.status !== 'unknown' && result.update.status !== 'submitted') refresh();
    } else {
      setNote(null);
      setActionError(result.error);
    }
  };
  // "Check status" is READ-ONLY (reconcile on the ORIGIN cluster). Resending the same bytes is a separate,
  // confirmed action, only offered for an unknown outcome.
  const checkAgain = async () => {
    setBusy('check');
    setConfirmResend(false);
    try {
      apply(await checkTxStatus(tx));
    } finally {
      setBusy(null);
    }
  };
  const resend = async () => {
    setBusy('resend');
    setConfirmResend(false);
    try {
      apply(await resendSameTx(tx));
    } finally {
      setBusy(null);
    }
  };
  const copySignature = async () => {
    try {
      await navigator.clipboard?.writeText(tx.signature);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard blocked: the signature is still on screen
    }
  };

  return (
    <div className={`${TONE_CLASS[view.tone]} p-3 text-left space-y-2 ${compact ? 'text-xs' : 'text-sm'}`} data-testid="native-tx-status" data-status={view.status}>
      <div className="flex items-center gap-2 font-bold">
        {view.tone === 'pending' && <Loader2 className="h-4 w-4 animate-spin" />}
        <span>{view.headline}</span>
      </div>
      <p className="text-xs opacity-90">{view.detail}</p>
      <div className="flex items-center gap-2 text-[11px] font-mono opacity-80">
        <span title={tx.signature}>Signature {shortSignature(tx.signature)}</span>
        <button type="button" onClick={copySignature} className="mc-btn-secondary px-1.5 py-0.5 rounded" aria-label="Copy signature">
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
        </button>
      </div>
      {tx.nonce !== null && <div className="text-[11px] font-mono opacity-80">Nonce {tx.nonce.toString()}</div>}
      <div className="text-[11px] opacity-80" data-testid="tx-origin" title={tx.origin?.rpcEndpoint ? `RPC host: ${origin.host ?? 'unknown'}` : undefined}>
        Sent on {origin.label}
      </div>
      {note && <p className="text-xs opacity-90" data-testid="tx-status-note">{note}</p>}
      {actionError && <div className="mc-status-red p-2 text-xs" data-testid="tx-status-error">{actionError}</div>}
      {(view.canPollAgain || (view.canRebuild && onTryAgain)) && (
        <div className="flex gap-2 flex-wrap pt-1 items-center">
          {view.canPollAgain && (
            <button type="button" onClick={checkAgain} disabled={busy !== null} className="mc-btn-secondary px-3 py-1 rounded-full text-xs">
              {busy === 'check' ? 'Checking…' : 'Check status'}
            </button>
          )}
          {view.canPollAgain && !confirmResend && (
            <button type="button" onClick={() => setConfirmResend(true)} disabled={busy !== null} className="underline text-xs opacity-80 hover:opacity-100">
              Resend the same transaction
            </button>
          )}
          {view.canPollAgain && confirmResend && (
            <div className="w-full mc-card p-2 text-xs space-y-1" data-testid="tx-resend-confirm">
              <p>
                This re-sends the exact same signed bytes to {origin.label}. Same signature, so the cluster cannot apply it twice. It
                is never rebuilt, re-signed or given a new nonce. Check status first if you have not.
              </p>
              <div className="flex gap-2">
                <button type="button" onClick={resend} disabled={busy !== null} className="mc-btn-primary px-3 py-1 rounded-full text-xs">
                  {busy === 'resend' ? 'Resending…' : 'Resend'}
                </button>
                <button type="button" onClick={() => setConfirmResend(false)} className="mc-btn-secondary px-3 py-1 rounded-full text-xs">Cancel</button>
              </div>
            </div>
          )}
          {view.canRebuild && onTryAgain && (
            <button type="button" onClick={onTryAgain} className="mc-btn-primary px-3 py-1 rounded-full text-xs">
              Try again
            </button>
          )}
        </div>
      )}
      {actions?.(view)}
    </div>
  );
}

/**
 * Right-hand column of the native deposit form: the projection for `lamports` on `plan`.
 * Interest runs on the GROSS deposit; the Front-End Load only reduces what reaches AUM.
 */
export function NativeInvestProjection({
  plan,
  planId,
  lamports,
  config,
}: {
  plan: PlanKind;
  planId: string;
  lamports: bigint;
  config: NativeConfigVersion | null;
}) {
  const isCompounding = plan !== 'Simple';
  const ppPerSol = ppPerSolForPlan(planId);
  const loadRate = (config?.frontEndLoadBps ?? 400) / 10_000;
  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const projection = lamports > 0n ? projectOpen({ plan, gross: lamports, config }) : null;
  const maturityFloat = projection?.maturityEarnings != null ? Number(projection.maturityEarnings) / Number(LAMPORTS_PER_SOL) : 0;
  const dailyLamports = lamports > 0n
    ? plan === 'Simple'
      ? projectSimpleDailyLamports(lamports)
      : projection?.dailyLamports ?? 0n
    : 0n;
  const roiPercent = solFloat > 0 && maturityFloat > 0 ? (maturityFloat / solFloat) * 100 : 0;
  const projectedPP = solFloat > 0 ? Math.round(solFloat * ppPerSol) : 0;

  const roiColor = roiPercent < 50 ? 'mc-text-green'
    : roiPercent < 200 ? 'mc-text-purple mc-glow-purple'
    : 'mc-text-gold mc-glow-gold';

  const roiResetToken = useRef(0);
  const prevKey = useRef('');
  useEffect(() => {
    const key = `${lamports.toString()}-${planId}`;
    if (key !== prevKey.current) { roiResetToken.current += 1; prevKey.current = key; }
  }, [lamports, planId]);
  const animatedReturn = useCountUp(maturityFloat, 800, roiResetToken.current);
  const animatedPP = useCountUp(projectedPP, 800, roiResetToken.current);

  return (
    <div>
    {projection && maturityFloat > 0 ? (
      <div>
        <div className="text-center mb-3"><span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span></div>
        <div className="mc-card p-4 space-y-3">
          <div className="flex justify-between items-center">
            <div>
              <div className="mc-label">{isCompounding ? 'Compounded Interest' : 'Interest Payout'}</div>
              <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatSolFloat(animatedReturn)} SOL</div>
              <div className={`text-xs opacity-70 ${roiColor}`}>
                {isCompounding ? `${roiPercent.toFixed(1)}% ROI` : `${(maturityFloat / solFloat).toFixed(2)}x ROI (${roiPercent.toFixed(0)}%)`}
              </div>
            </div>
            <div className="text-right">
              <div className="mc-label">Ponzi Points</div>
              <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{Math.round(animatedPP).toLocaleString()}</div>
              <div className="text-xs mc-text-purple opacity-70">{ppPerSol.toLocaleString()} / SOL · credited once finalized</div>
            </div>
          </div>
          <div className="border-t border-white/10 pt-3 space-y-1.5">
            <div className="flex justify-between text-sm">
              <span className="mc-text-muted">Front-End Load ({pct(loadRate)})</span>
              <span className="mc-text-primary font-medium">-{formatSOL(projection.frontEndLoad)} SOL</span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="mc-text-muted">Deposit into AUM</span>
              <span className="mc-text-primary font-medium">{formatSOL(projection.potCredit)} SOL</span>
            </div>
            <div className="flex items-center justify-between pt-1">
              <div className="flex items-center gap-1.5"><TrendingUp className="h-3.5 w-3.5 mc-text-cyan" /><span className="text-xs mc-text-dim">{isCompounding ? 'Day-one earnings' : 'Daily earnings'}</span></div>
              <span className="text-sm font-bold mc-text-cyan">{formatSOL(dailyLamports)} SOL/day</span>
            </div>
            <p className="text-[10px] mc-text-muted">
              Interest accrues on your gross deposit of {formatSOL(lamports)} SOL.{' '}
              {projection.maturityFeeBps !== null && (isCompounding
                ? `A ${pct(projection.maturityFeeBps / 10_000)} Jackpot Fee applies at maturity.`
                : `Carried Interest applies on withdrawal (${pct(projection.maturityFeeBps / 10_000)} after day 14, higher earlier).`)}
            </p>
          </div>
        </div>
      </div>
    ) : (
      <div className="h-full flex items-center justify-center text-center">
        <div><BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" /><p className="text-sm mc-text-muted">Enter an amount to see projected returns</p></div>
      </div>
    )}
  </div>
  );
}

function NativeInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { solanaPubkey } = useWallet();
  const { pool } = useNativeAvailability();
  const config = useNativeActiveConfig(pool).data ?? null;
  const { data: solBalance } = useSolBalance();
  const [solInput, setSolInput] = useState('');
  const [stage, setStage] = useState<string | null>(null);
  // The first position of a round needs that round's participant page; a fresh round (after
  // finalize_round) has none, so it is provisioned first, in its own transaction, and only then is
  // the deposit built (panelsPrereqs.ts).
  const openMut = usePanelNativeTask(
    async (client, vars: { plan: PlanKind; gross: bigint }, readers) => {
      setStage(null);
      return runPrepared(
        client,
        readers,
        (fresh) => openPrereqs(readers, fresh, client.sessionPubkey as string),
        () => client.openPosition(vars),
        { onStage: (s) => setStage(describePrepStage(s, 'your deposit')) },
      );
    },
    (outcome) => [...outcome.prep.map((p) => p.tx), outcome.result],
  );

  const plan = planKindFromInvestId(planId);
  const minLamports = config?.minGame ?? MIN_LAMPORTS;
  const minSol = Number(minLamports) / Number(LAMPORTS_PER_SOL);

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);
  const belowMin = lamports > 0n && lamports < minLamports;
  const closing = !!pool && pool.currentCloseId !== null;
  const balanceLamports = solBalance === undefined ? null : BigInt(Math.round(solBalance * Number(LAMPORTS_PER_SOL)));
  const insufficient = balanceLamports !== null && lamports > 0n && lamports + NATIVE_FEE_AND_RENT_RESERVE_LAMPORTS > balanceLamports;
  const canDeposit = !!solanaPubkey && !!pool && lamports >= minLamports && !insufficient && !closing && !openMut.isPending;

  const handleDeposit = () => {
    if (!canDeposit) return;
    openMut.mutate({ plan, gross: lamports });
  };
  const handleStartOver = () => { openMut.reset(); setSolInput(''); setStage(null); };

  if (!pool) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting to the native program…</p>
      </div>
    );
  }

  // A transaction was signed and sent: show its ladder. The nonce is kept; "unknown"
  // never rebuilds (Check status re-polls the same signature). Preparatory page
  // transactions (if any were needed) are listed above the deposit with their own status.
  if (openMut.data) {
    const { prep, result: tx } = openMut.data;
    return (
      <div className="space-y-6">
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          {prep.map((p) => (
            <NativeTxStatusCard key={p.tx.signature} tx={p.tx} label={p.label} compact onTryAgain={handleStartOver} />
          ))}
          {tx ? (
            <NativeTxStatusCard
              tx={tx}
              label="Your deposit"
              onTryAgain={handleStartOver}
              actions={(view) => (
                <div className="flex gap-3 justify-center pt-2 flex-wrap">
                  {(view.status === 'confirmed' || view.status === 'finalized') && (
                    <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">Open another</button>
                  )}
                  <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2">
                    <TrendingUp className="h-4 w-4" /> Go to Profit Center
                  </button>
                </div>
              )}
            />
          ) : (
            <div className="mc-status-blue p-3 text-sm space-y-2">
              <p>Your deposit has not been sent yet: the preparation above has not confirmed. Check its status, then deposit again.</p>
              <button onClick={handleStartOver} className="mc-btn-secondary px-4 py-1.5 rounded-full text-xs">Back to the deposit form</button>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div>
          <div className="flex justify-between items-center mb-2">
            <span className="text-xs font-bold mc-text-primary">Amount</span>
          </div>
          <div className="flex gap-2">
            <button onClick={() => setSolInput(String(minSol))} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap">MIN</button>
            <input type="text" inputMode="decimal" value={solInput} onChange={(e) => setSolInput(e.target.value)} placeholder={`Min: ${minSol} SOL`} className="mc-input flex-1 text-center text-lg font-mono" />
          </div>
          {belowMin && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {minSol} SOL</div>)}
          {insufficient && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Your wallet needs about {formatSOL(NATIVE_FEE_AND_RENT_RESERVE_LAMPORTS)} SOL more than the deposit for network fees and one-time account rent.</div>)}
          {closing && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />The round is closing. New positions open once it finalizes (see Profit Center).</div>)}
          {!solanaPubkey && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Reconnect your Solana wallet to deposit.</div>)}

          <div className="mc-status-red p-3 text-center text-sm font-bold mt-3">
            <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
            <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
          </div>

          <button onClick={handleDeposit} disabled={!canDeposit} className={`w-full py-3 mt-3 text-sm font-bold rounded-xl transition-all mc-btn-primary inline-flex items-center justify-center gap-2 ${canDeposit ? 'pulse' : ''}`}>
            {openMut.isPending ? (stage ?? 'Confirm in your wallet…') : <><Dices className="h-5 w-5" /> {LOCAL_CLUSTER_MODE ? 'DEPOSIT FROM LOCAL WALLET' : 'DEPOSIT FROM YOUR WALLET'}</>}
          </button>

          {openMut.isError && (<p className="text-xs mc-text-danger mt-2 text-center">{describeNativeError(openMut.error)}</p>)}

          <p className="text-[10px] mc-text-muted text-center mt-2">
            Your wallet also pays network fees and one-time account rent, separate from the deposit. The first position of a round may need one extra transaction to prepare the round's participant page.
          </p>
        </div>

        <NativeInvestProjection plan={plan} planId={planId} lamports={lamports} config={config} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Entry point: native first (DR-22), legacy only when the program is not on this cluster
// ---------------------------------------------------------------------------

export default function SolInvestPanel(props: SolInvestPanelProps) {
  const availability = useNativeAvailability();
  switch (railOf(availability)) {
    case 'native':
      return <NativeInvestPanel {...props} />;
    case 'loading':
      return <NativeRailVerifying what="Deposits" />;
    default:
      // The native program is not verifiable here (the default mainnet endpoint, no cluster, a mismatch,
      // an uninitialized program): the legacy rail only, clearly labeled, no native UI. Local-cluster
      // mode has no legacy rail (its canister is never reached), so it says what is missing instead.
      if (LOCAL_CLUSTER_MODE) {
        return (
          <div className="mc-card p-6 text-center max-w-md mx-auto space-y-2" data-testid="native-unavailable">
            <div className="font-display text-lg mc-text-primary">The native program isn't available here.</div>
            <p className="text-sm mc-text-dim">{describeUnavailable(availability)}</p>
          </div>
        );
      }
      return (
        <div className="space-y-4">
          <LegacyRailBanner what="deposits" />
          <LegacySolInvestPanel {...props} />
        </div>
      );
  }
}
