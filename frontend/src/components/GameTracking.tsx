import { useState, useEffect, useRef } from 'react';
import { useGetUserGames, useWithdrawGameEarnings, useSettleCompoundingGame, isCompoundingPlanUnlocked, getTimeRemaining, useGetPonziPoints, useGetShenaniganConfigs, useGetMintConfig, useGetUserSolGames, useWithdrawSolGameEarnings } from '../hooks/useQueries';
import { useLivePortfolio } from '../hooks/useLiveEarnings';
import { useWallet } from '../hooks/useWallet';
import { formatSolFloat, formatSOL, parseSOL } from '../solana/lamports';
import { useNativeCloseRecord, useNativeTicket, useNativeTxStatus } from '../hooks/useNativeSolana';
import {
  useNativeAvailability,
  useNativeConfig,
  useNativePortfolio,
  useNowSeconds,
  usePanelNativeAction,
  usePanelNativeTask,
  useRequestAccountLength,
  type NativePortfolio,
  type NativePositionRow,
} from '../solana/native/panelsHooks';
import {
  PLAN_LABEL,
  defaultMinimumReceive,
  describeNativeError,
  planOfPosition,
  positionAction,
  type PositionAction,
} from '../solana/native/panelsQuote';
import { describePrepStage, payoutPrereqs, runPrepared, type PrepTx } from '../solana/native/panelsPrereqs';
import {
  closingPayoutPlan,
  nextProcessingStep,
  requestLooksFulfilled,
  runPayout,
  processingOutcome,
  runProcessingStep,
  type ProcessingStep,
} from '../solana/native/panelsClosing';
import { DRAW_STATE, type NativePool, type NativeTxResult } from '../solana/native/types';
import type { PayoutRequest } from '../solana/native/client';
import { NativeTxStatusCard } from './SolInvestPanel';
import { GameRecord, GamePlan, SolGameRecord } from '../backend';
import { triggerConfetti } from './ConfettiCanvas';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';
import {
  EXIT_TOLL_EARLY,
  EXIT_TOLL_MID,
  EXIT_TOLL_LATE,
  EXIT_TOLL_EARLY_DAYS,
  EXIT_TOLL_MID_DAYS,
  JACKPOT_FEE_RATE_15D,
  JACKPOT_FEE_RATE_30D,
  pctPrecise,
} from '../lib/gameConstants';
import { Lock, ArrowDownCircle, Rocket, TrendingUp, TrendingDown, Dice5, ChevronDown } from 'lucide-react';
import type { TabType } from '../App';
import MobileSheet from './MobileSheet';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';

/* ================================================================
   Constants
   ================================================================ */

function getPlanName(plan: any): string {
  if ('simple21Day' in plan) return '21-Day Simple';
  if ('compounding15Day' in plan) return '15-Day Compounding';
  if ('compounding30Day' in plan) return '30-Day Compounding';
  return 'Unknown Plan';
}

function getPlanAccent(plan: any): string {
  if ('simple21Day' in plan) return 'mc-plan-simple';
  if ('compounding15Day' in plan) return 'mc-plan-compound';
  if ('compounding30Day' in plan) return 'mc-plan-compound';
  return '';
}

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

/** Format ICP for display — full 8 decimal precision */
const formatICPDisplay = (value: number): string => {
  if (isNaN(value) || !isFinite(value)) return '0.00000000';
  return value.toFixed(8);
};

const formatDate = (ts: bigint) =>
  new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1_000_000));

const daysActive = (ts: bigint) => Math.floor((Date.now() - Number(ts) / 1_000_000) / 86_400_000);

function getExitTollInfo(game: GameRecord) {
  const startTime = Number(game.startTime) / 1_000_000;
  const elapsed = Date.now() - startTime;
  const days = elapsed / 86_400_000;
  if (game.isCompounding) {
    const fee = 'compounding15Day' in game.plan ? JACKPOT_FEE_RATE_15D * 100 : JACKPOT_FEE_RATE_30D * 100;
    return { currentFee: fee, nextFee: null, timeToNext: null };
  }
  if (days < EXIT_TOLL_EARLY_DAYS) return { currentFee: EXIT_TOLL_EARLY * 100, nextFee: EXIT_TOLL_MID * 100, timeToNext: (EXIT_TOLL_EARLY_DAYS * 86_400_000) - elapsed };
  if (days < EXIT_TOLL_MID_DAYS) return { currentFee: EXIT_TOLL_MID * 100, nextFee: EXIT_TOLL_LATE * 100, timeToNext: (EXIT_TOLL_MID_DAYS * 86_400_000) - elapsed };
  return { currentFee: EXIT_TOLL_LATE * 100, nextFee: null, timeToNext: null };
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
  // Compounding fees get purple regardless of magnitude
  if (fee === JACKPOT_FEE_RATE_15D * 100 || fee === JACKPOT_FEE_RATE_30D * 100) {
    return 'bg-[var(--mc-purple)]/20 mc-text-purple';
  }
  if (fee <= EXIT_TOLL_LATE * 100) return 'bg-[var(--mc-neon-green)]/20 mc-text-green';
  if (fee <= EXIT_TOLL_MID * 100) return 'bg-[var(--mc-gold)]/20 mc-text-gold';
  return 'bg-[var(--mc-danger)]/20 mc-text-danger';
}

function getPositionUrgency(game: GameRecord): number {
  const planDays = getPlanDuration(game);
  const days = daysActive(game.startTime);
  const remaining = planDays - days;
  if (game.isCompounding && remaining > 0 && remaining <= 3) return 0; // near unlock → highest
  const tollInfo = getExitTollInfo(game);
  if (tollInfo.currentFee >= EXIT_TOLL_EARLY * 100) return 1; // high toll (early simple tier)
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
  denomination = 'ICP',
  withdrawDisabled = false,
  withdrawDisabledTitle,
  settleLabel = 'Withdraw',
}: {
  game: GameRecord;
  earnings: number;
  onWithdraw: (game: GameRecord) => void;
  withdrawPending: boolean;
  denomination?: 'ICP' | 'SOL';
  withdrawDisabled?: boolean;
  withdrawDisabledTitle?: string;
  settleLabel?: string;
}) {
  const name = getPlanName(game.plan);
  const accent = getPlanAccent(game.plan);
  const unlocked = isCompoundingPlanUnlocked(game);
  const canWithdraw = !game.isCompounding || unlocked;
  const hasEarnings = earnings > 0;
  // A matured Simple position with 0 earnings can still be closed (backend marks inactive)
  const isMaturedSimpleClose = !game.isCompounding && daysActive(game.startTime) >= getPlanDuration(game) && !hasEarnings;
  const buttonEnabled = canWithdraw && (hasEarnings || isMaturedSimpleClose) && !withdrawDisabled;
  const timeRem = getTimeRemaining(game);
  const tollInfo = getExitTollInfo(game);
  const fmt = denomination === 'SOL' ? formatSolFloat : formatICP;
  const unit = denomination;

  return (
    <div className={`mc-card ${accent} p-4 transition-all duration-200 hover:translate-y-[-2px] hover:shadow-lg`}>
      {/* Top row: plan name + badge + days active */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="font-bold mc-text-primary">{name}</span>
          <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
            game.isCompounding ? 'bg-[var(--mc-purple)]/20 mc-text-purple' : 'bg-[var(--mc-neon-green)]/20 mc-text-green'
          }`}>
            {game.isCompounding ? 'Compounding' : 'Simple'}
          </span>
        </div>
        <span className="text-xs mc-text-muted">{daysActive(game.startTime)}d active</span>
      </div>

      {/* Numbers row: deposit | live earnings | carried interest */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-3">
        <div>
          <div className="mc-label">Deposit</div>
          <div className="text-base font-bold mc-text-primary">{fmt(game.amount)} {unit}</div>
          <div className="text-xs mc-text-muted">{formatDate(game.startTime)}</div>
        </div>
        <div className="text-center">
          <div className="mc-label">Earnings</div>
          <div className="text-lg sm:text-xl font-bold mc-text-green mc-glow-green">
            {fmt(earnings)} {unit}
          </div>
          <div className="text-xs mc-text-muted">{game.isCompounding ? 'before carry' : 'live'}</div>
        </div>
        <div className="text-right">
          <div className="mc-label">Carried Interest</div>
          <div className={`inline-block text-sm font-bold px-2 py-0.5 rounded-full ${getTollBadgeClasses(tollInfo.currentFee)}`}>
            {tollInfo.currentFee}%
          </div>
          {tollInfo.nextFee && tollInfo.timeToNext && (
            <div className="text-xs mc-text-cyan mt-0.5">{fmtCountdown(tollInfo.timeToNext)} to {tollInfo.nextFee}%</div>
          )}
        </div>
      </div>

      {/* Progress bar + Withdraw button side by side */}
      <div className="flex items-end gap-3">
        {/* Progress bar — left side, fills remaining space */}
        {(() => {
          const planDays = getPlanDuration(game);
          const days = daysActive(game.startTime);
          const pct = Math.min(100, Math.round((days / planDays) * 100));
          const indicatorColor = game.isCompounding ? 'mc-bg-purple' : 'mc-bg-green';
          return (
            <div className="flex-1">
              <div className="flex justify-between text-xs mc-text-muted mb-1">
                <span>Day {days} / {planDays}</span>
                <span>{pct}%</span>
              </div>
              <Progress value={pct} className="h-1.5 bg-white/5" indicatorClassName={indicatorColor} />
            </div>
          );
        })()}

        {/* Withdraw button — right-aligned, compact */}
        <button
          onClick={() => onWithdraw(game)}
          disabled={withdrawPending || !buttonEnabled}
          className={`px-4 py-2 rounded-lg text-xs font-bold uppercase transition-all whitespace-nowrap ${
            buttonEnabled
              ? 'mc-btn-primary'
              : 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5'
          }`}
          title={withdrawDisabled ? withdrawDisabledTitle : !canWithdraw ? 'Locked until maturity' : isMaturedSimpleClose ? 'Close matured position' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
        >
          {!canWithdraw ? (
            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h {timeRem.minutes}m</span>
          ) : isMaturedSimpleClose ? (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Close</span>
          ) : (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />{game.isCompounding ? settleLabel : 'Withdraw'}</span>
          )}
        </button>
      </div>
    </div>
  );
}

/* ================================================================
   Empty State with rotating Charles quotes
   ================================================================ */

function EmptyState({ onNavigate }: { onNavigate?: () => void }) {
  const [quoteIndex] = useState(() => Math.floor(Math.random() * charlesQuotes.length));

  return (
    <div className="text-center py-16">
      <div className="mb-8 flex flex-col items-center justify-center">
        <p className="font-accent text-sm mc-text-dim italic max-w-md mx-auto leading-relaxed">
          {charlesQuotes[quoteIndex]}
        </p>
      </div>
      <button onClick={onNavigate} className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2">
        <Rocket className="h-4 w-4" /> Pick Your Plan
      </button>
    </div>
  );
}

/* ================================================================
   Native positions (Musical Chairs program) — IMPL-46c
   Native is the default SOL rail (DR-22); the legacy ponzi_math_sol list
   below it is untouched. The two are never merged or summed.
   ================================================================ */

const lamportsToFloat = (v: bigint) => Number(v) / 1_000_000_000;

const STEP_NOUN: Record<string, string> = {
  'advance-allocation': 'The allocation step',
  'provision-page': 'The candidate page',
  'build-candidates': 'The candidate batch',
  'request-selection': 'The randomness request',
  'capture-randomness': 'The capture',
  'apply-promotion': 'The conversion',
  'expire-promotion': 'The expiry',
  'finalize-round': 'The finalization',
};

const programErrorNameOf = (error: unknown): string | undefined =>
  (error as { programError?: { name?: string } | null } | null)?.programError?.name;

/**
 * Allocation run / round close. Shows "allocation run in progress" or "round
 * closing" with ONE "resume processing" button: each click sends the single next
 * step (nextProcessingStep reads the Ticket, Pool and CloseRecord) and shows its status.
 */
function NativeProcessingBanner({ pool, onNavigateToGameSetup }: { pool: NativePool; onNavigateToGameSetup?: () => void }) {
  const ticket = useNativeTicket().data ?? null;
  const closeId = pool.currentCloseId;
  const close = useNativeCloseRecord(closeId).data ?? null;
  const config = useNativeConfig(close ? close.configVersion : null).data ?? null;
  const now = useNowSeconds();

  const drawRequested = !!close && close.drawState === DRAW_STATE.Requested;
  const requestLength = useRequestAccountLength(close?.requestAccount, drawRequested).data;
  // "Awaiting randomness": the provider grows the request account when it fulfils it. If that
  // signal is missing or wrong, the button re-enables 20 s after the request was first seen
  // and the program itself decides (an unfulfilled capture is rejected before broadcast).
  const requestedSince = useRef<number | null>(null);
  if (drawRequested) {
    if (requestedSince.current === null) requestedSince.current = Date.now();
  } else {
    requestedSince.current = null;
  }
  const [notReadyUntil, setNotReadyUntil] = useState(0);
  const hint = drawRequested ? requestLooksFulfilled(requestLength, config) : null;
  const inFreshWindow = requestedSince.current !== null && Date.now() - requestedSince.current < 20_000;
  const randomnessReady: boolean | null = !drawRequested
    ? null
    : Date.now() < notReadyUntil
      ? false
      : hint === false
        ? (inFreshWindow ? false : null)
        : hint;

  const step = nextProcessingStep({ pool, close, ticketPhase: ticket?.phase, now, randomnessReady });
  const stepKey = [
    step.kind,
    pool.activeTicket,
    pool.currentCloseId,
    ticket?.phase,
    ticket?.cursor,
    close?.candidateCursor,
    close?.provisionedPreferredPages,
    close?.provisionedFallbackPages,
    close?.drawState,
    close?.promotion.kind,
    close?.finalized,
  ].join(':');

  const run = usePanelNativeAction((client, vars: { step: ProcessingStep }) => runProcessingStep(client, vars.step));
  const [sent, setSent] = useState<{ tx: NativeTxResult; key: string; kind: string } | null>(null);
  const sentStatus = useNativeTxStatus(sent?.tx ?? null);
  const sentStalled = sentStatus?.status === 'failed' || sentStatus?.status === 'expired';
  // One step per state: a second click on the same state would only re-send what is already in flight.
  const alreadySent = !!sent && sent.key === stepKey && !sentStalled;

  const onResume = async () => {
    setNotReadyUntil(0);
    try {
      const tx = await run.mutateAsync({ step });
      setSent({ tx, key: stepKey, kind: step.kind });
    } catch (error) {
      if (programErrorNameOf(error) === 'RequestNotFulfilled') setNotReadyUntil(Date.now() + 5_000);
    }
  };

  const closing = closeId !== null;
  const nothingLeft = step.kind === 'done';
  if (nothingLeft && !sent) return null;

  const deadlineLeft = close && close.promotion.kind === 'Pending' ? close.promotionDeadline - now : null;
  const mmss = (s: bigint) => `${s / 60n}:${(s % 60n).toString().padStart(2, '0')}`;
  const awaitingRandomness = step.kind === 'await-randomness' || (run.isError && programErrorNameOf(run.error) === 'RequestNotFulfilled');

  return (
    <div className="mc-card mc-accent-gold p-4 mb-4 space-y-2" data-testid="native-processing" data-step={step.kind}>
      {!nothingLeft && (
        <>
          <div className="flex items-start justify-between gap-3 flex-wrap">
            <div>
              <div className="font-bold mc-text-gold">
                {closing ? 'Round closing' : 'Allocation run in progress'} — resume processing
              </div>
              <p className="text-xs mc-text-dim mt-0.5">{step.detail}</p>
              {closing && deadlineLeft !== null && (
                <p className="text-xs mc-text-muted mt-0.5">
                  {deadlineLeft > 0n ? `Promotion window: ${mmss(deadlineLeft)} left.` : 'Promotion window closed.'}
                </p>
              )}
            </div>
            <button
              type="button"
              onClick={onResume}
              disabled={!step.runnable || run.isPending || alreadySent}
              className="mc-btn-primary px-4 py-2 rounded-lg text-xs font-bold whitespace-nowrap disabled:opacity-50"
            >
              {run.isPending ? 'Confirm in your wallet…' : alreadySent ? 'Waiting for the cluster…' : step.label}
            </button>
          </div>
          {awaitingRandomness && !run.isPending && (
            <div className="mc-status-blue p-2 text-xs">Awaiting randomness. The provider hasn't answered yet; this usually takes a few seconds.</div>
          )}
        </>
      )}
      {/* Every rejection is visible, whatever step the state has moved on to (the simulation refuses before signing). */}
      {run.isError && programErrorNameOf(run.error) !== 'RequestNotFulfilled' && (
        <div className="mc-status-red p-2 text-xs" data-testid="native-processing-error">{describeNativeError(run.error)}</div>
      )}
      {sent && (
        <NativeTxStatusCard
          tx={sent.tx}
          status={sentStatus}
          label={STEP_NOUN[sent.kind] ?? 'The step'}
          compact
          onTryAgain={() => setSent(null)}
          actions={() => {
            if (!nothingLeft) return null;
            const landed = sentStatus?.status === 'confirmed' || sentStatus?.status === 'finalized';
            const outcome = processingOutcome(sent.kind);
            return (
              <div className="space-y-2 pt-1" data-testid="native-processing-done">
                {landed && <p className="text-xs">{outcome.message}</p>}
                <div className="flex gap-2 flex-wrap">
                  {landed && outcome.opensNewRound && (
                    <button type="button" onClick={() => onNavigateToGameSetup?.()} className="mc-btn-primary px-3 py-1 rounded-full text-xs inline-flex items-center gap-1">
                      <Rocket className="h-3 w-3" /> Open a Position in the new round
                    </button>
                  )}
                  <button type="button" onClick={() => setSent(null)} className="mc-btn-secondary px-3 py-1 rounded-full text-xs">Dismiss</button>
                </div>
              </div>
            );
          }}
        />
      )}
    </div>
  );
}

/** One native position, laid out like the legacy PositionCard. */
export function NativePositionCard({
  row,
  pool,
  busy,
  onAct,
}: {
  row: NativePositionRow;
  pool: NativePool;
  busy: boolean;
  onAct: (row: NativePositionRow, action: PositionAction) => void;
}) {
  const { position, quote } = row;
  const plan = planOfPosition(position.kind) ?? 'Simple';
  const isCompound = plan !== 'Simple';
  const accent = isCompound ? 'mc-plan-compound' : 'mc-plan-simple';
  const action = quote ? positionAction({ position, quote, pool }) : null;
  const durationDays = Number((position.maturityAt - position.openedAt) / 86_400n) || 1;
  const days = quote ? Number(quote.elapsedSeconds / 86_400n) : 0;
  const progress = Math.min(100, Math.round((days / durationDays) * 100));
  const enabled = !!action && action.enabled && !busy;
  const left = quote ? quote.secondsToMaturity : 0n;
  const countdown = `${left / 86_400n}d ${(left % 86_400n) / 3_600n}h ${(left % 3_600n) / 60n}m`;
  const feeName = isCompound ? 'Jackpot Fee' : 'Carried Interest';

  return (
    <div className={`mc-card ${accent} p-4 transition-all duration-200 hover:translate-y-[-2px] hover:shadow-lg`} data-testid="native-position" data-nonce={position.nonce.toString()}>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className="font-bold mc-text-primary">{PLAN_LABEL[plan]}</span>
          <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${isCompound ? 'bg-[var(--mc-purple)]/20 mc-text-purple' : 'bg-[var(--mc-neon-green)]/20 mc-text-green'}`}>
            {isCompound ? 'Compounding' : 'Simple'}
          </span>
          <span className="text-[10px] px-1.5 py-0.5 rounded border border-white/10 bg-white/5 mc-text-cyan uppercase tracking-wider">Native</span>
        </div>
        <span className="text-xs mc-text-muted">{days}d active</span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-3">
        <div>
          <div className="mc-label">Deposit</div>
          <div className="text-base font-bold mc-text-primary">{formatSOL(position.principal)} SOL</div>
          <div className="text-xs mc-text-muted">{formatDate(position.openedAt * 1_000_000_000n)}</div>
        </div>
        <div className="text-center">
          <div className="mc-label">Earnings</div>
          <div className="text-lg sm:text-xl font-bold mc-text-green mc-glow-green">{quote ? formatSOL(quote.owed) : '…'} SOL</div>
          <div className="text-xs mc-text-muted">{isCompound ? (quote?.matured ? 'ready to settle' : 'at maturity, before fee') : 'live, before carry'}</div>
        </div>
        <div className="text-right">
          <div className="mc-label">{feeName}</div>
          {quote && (
            <div className={`inline-block text-sm font-bold px-2 py-0.5 rounded-full ${getTollBadgeClasses(quote.feeBps / 100)}`}>{quote.feeBps / 100}%</div>
          )}
          {quote?.nextTier && (
            <div className="text-xs mc-text-cyan mt-0.5">{fmtCountdown(Number(quote.nextTier.inSeconds) * 1000)} to {quote.nextTier.bps / 100}%</div>
          )}
        </div>
      </div>

      {quote && quote.payable && (
        <div className="text-xs mc-text-dim mb-3" data-testid="native-quote">
          If you {isCompound ? 'settle' : 'withdraw'} now: {formatSOL(quote.paidGross)} SOL gross, {feeName} −{formatSOL(quote.fee)} SOL, you receive{' '}
          <strong className="mc-text-primary">{formatSOL(quote.net)} SOL</strong>.
          {quote.insolvent && <span className="mc-text-danger font-bold"> This payout closes the round.</span>}
        </div>
      )}

      <div className="flex items-end gap-3">
        <div className="flex-1">
          <div className="flex justify-between text-xs mc-text-muted mb-1">
            <span>Day {days} / {durationDays}</span>
            <span>{progress}%</span>
          </div>
          <Progress value={progress} className="h-1.5 bg-white/5" indicatorClassName={isCompound ? 'mc-bg-purple' : 'mc-bg-green'} />
        </div>
        <button
          onClick={() => action && onAct(row, action)}
          disabled={!enabled}
          className={`px-4 py-2 rounded-lg text-xs font-bold uppercase transition-all whitespace-nowrap ${
            enabled ? 'mc-btn-primary' : 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5'
          }`}
          title={action?.reason ?? (action?.kind === 'close' ? 'Close matured position' : isCompound ? 'Settle' : 'Withdraw')}
        >
          {!action || action.kind === 'none' ? (
            'Closed'
          ) : action.kind === 'locked' ? (
            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{countdown}</span>
          ) : action.kind === 'close' ? (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Close</span>
          ) : (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />{action.kind === 'settle' ? 'Settle' : 'Withdraw'}</span>
          )}
        </button>
      </div>
    </div>
  );
}

/** A sent transaction with the noun phrase its status card uses. */
type SentTx = PrepTx;

/** Confirm sheet for withdraw_simple / settle_compounding, including the payout that closes the round. */
function NativePayoutSheet({
  row,
  pool,
  maxWorkLamports,
  maxWorkInvalid,
  onClose,
  onSent,
}: {
  row: NativePositionRow | null;
  pool: NativePool;
  maxWorkLamports: bigint;
  maxWorkInvalid: boolean;
  onClose: () => void;
  onSent: (items: SentTx[]) => void;
}) {
  const quote = row?.quote ?? null;
  const nextCloseRecord = useNativeCloseRecord(pool.nextCloseId).data ?? null;
  const [stage, setStage] = useState<string | null>(null);
  // Order: shared snapshot pages the toll's allocation run needs (panelsPrereqs), then the CloseRecord when
  // this payout closes the round (runPayout), then the payout itself. Each preparatory step is its own
  // transaction that must land and be visible first.
  const payout = usePanelNativeTask(
    async (client, vars: { kind: 'withdraw' | 'settle'; request: PayoutRequest; prepareCloseId: bigint | null }, readers) => {
      const prepared = await runPrepared(
        client,
        readers,
        (fresh) => payoutPrereqs(fresh),
        () =>
          runPayout(client, {
            ...vars,
            onStage: (s) => setStage(s === 'provisioning-close-record' ? 'Preparing the close record…' : 'Sending the payout…'),
          }),
        { onStage: (s) => s.kind === 'preparing' && setStage(describePrepStage(s, 'the payout')) },
      );
      return { pages: prepared.prep, outcome: prepared.result };
    },
    (result) => [...result.pages.map((p) => p.tx), result.outcome?.provision, result.outcome?.payout],
  );
  useEffect(() => {
    if (!row) {
      setStage(null);
      payout.reset();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [row]);

  if (!row || !quote) return null;
  const { position } = row;
  const isCompound = quote.plan !== 'Simple';
  const feeName = isCompound ? 'Jackpot Fee' : 'Carried Interest';
  const minimumReceive = defaultMinimumReceive(quote.net);
  const plan = closingPayoutPlan({
    owed: quote.owed,
    pot: pool.pot,
    accrualPerSecondNumerator: isCompound ? 0n : position.principal * 11n,
    closeRecordExists: !!nextCloseRecord,
    nextCloseId: pool.nextCloseId,
  });

  const confirm = async () => {
    setStage(null);
    try {
      const outcome = await payout.mutateAsync({
        kind: isCompound ? 'settle' : 'withdraw',
        request: { nonce: position.nonce, minimumReceive, maxWorkContribution: maxWorkLamports },
        prepareCloseId: plan.prepareCloseId,
      });
      const items: SentTx[] = [
        ...outcome.pages,
        ...(outcome.outcome?.provision ? [{ tx: outcome.outcome.provision, label: 'Preparing the close record' }] : []),
        ...(outcome.outcome?.payout ? [{ tx: outcome.outcome.payout, label: isCompound ? 'Your settlement' : 'Your withdrawal' }] : []),
      ];
      // A preparatory step that has not landed means the payout was NOT sent: keep the sheet open and
      // surface the signatures outside so the user can check them, then confirm again.
      onSent(items);
      if (outcome.outcome?.payout) onClose();
    } catch {
      // payout.error is rendered below
    }
  };

  return (
    <MobileSheet open={true} onOpenChange={(open) => { if (!open) onClose(); }}>
      <div className="space-y-4" data-testid="native-payout-sheet">
        <h2 className="font-display text-lg mc-text-primary">{isCompound ? 'Confirm Settlement' : 'Confirm Withdrawal'}</h2>
        <div className="text-sm mc-text-dim space-y-1">
          <div className="flex justify-between"><span>Owed (gross)</span><strong className="mc-text-primary">{formatSOL(quote.owed)} SOL</strong></div>
          {quote.insolvent && (
            <div className="flex justify-between"><span>Paid from AUM (gross)</span><strong className="mc-text-primary">{formatSOL(quote.paidGross)} SOL</strong></div>
          )}
          <div className="flex justify-between">
            <span>{feeName} ({quote.feeBps / 100}%)</span>
            <strong className="mc-text-gold">−{formatSOL(quote.fee)} SOL</strong>
          </div>
          <div className="flex justify-between border-t border-white/10 pt-1"><span>You receive (net)</span><strong className="mc-text-green">{formatSOL(quote.net)} SOL</strong></div>
          <div className="flex justify-between text-xs mc-text-muted">
            <span>Minimum receive (quote less 1% slippage)</span>
            <span>{formatSOL(minimumReceive)} SOL</span>
          </div>
          {quote.nextTier && (
            <p className="text-xs mc-text-cyan pt-1">
              Wait {fmtCountdown(Number(quote.nextTier.inSeconds) * 1000)} to reduce {feeName} to {quote.nextTier.bps / 100}%.
            </p>
          )}
        </div>
        {plan.closesRound && (
          <div className="mc-status-red p-3 text-xs space-y-1">
            <div className="font-bold">This payout closes the round.</div>
            <p>
              Owed ({formatSOL(quote.owed)} SOL) exceeds AUM ({formatSOL(pool.pot)} SOL), so it pays out what AUM holds and starts the Emergency Equity Conversion:
              a randomized selection converts one unprofitable depositor into a Series B Backer. Anyone can then push the remaining steps with "resume processing".
            </p>
          </div>
        )}
        {plan.prepareCloseId !== null && !nextCloseRecord && (
          <div className="mc-status-blue p-2 text-xs">
            First this sends a small transaction to prepare the round's close record (a refundable account deposit paid by your wallet); the payout follows once it confirms.
          </div>
        )}
        {payoutPrereqs(pool).length > 0 && (
          <div className="mc-status-blue p-2 text-xs">
            First this sends a small transaction to prepare the allocation snapshot page (a one-time account deposit paid by your wallet); the payout follows once it confirms.
          </div>
        )}
        {stage && <div className="mc-status-blue p-2 text-xs">{stage}</div>}
        {payout.isError && <div className="mc-status-red p-2 text-xs">{describeNativeError(payout.error)}</div>}
        {maxWorkInvalid && <div className="mc-status-red p-2 text-xs">Fix the Advanced contribution amount first.</div>}
        <div className="flex gap-2 justify-end">
          <Button variant="outline" onClick={onClose} className="mc-btn-secondary">Cancel</Button>
          <Button onClick={confirm} disabled={payout.isPending || maxWorkInvalid} className="mc-btn-primary">
            {payout.isPending ? 'Processing...' : plan.closesRound ? 'Confirm and close the round' : 'Confirm'}
          </Button>
        </div>
      </div>
    </MobileSheet>
  );
}

/** The native half of "Your Positions": banner, cards, Advanced field, status. */
function NativePositionsSection({
  portfolio,
  onNavigateToGameSetup,
}: {
  portfolio: NativePortfolio;
  onNavigateToGameSetup?: () => void;
}) {
  const { pool, active, closed } = portfolio;
  const [maxWork, setMaxWork] = useState('');
  const [sheetRow, setSheetRow] = useState<NativePositionRow | null>(null);
  const [sentTxs, setSentTxs] = useState<SentTx[]>([]);
  const closeMut = usePanelNativeAction((client, vars: { nonce: bigint }) => client.closeSimplePosition(vars));

  const maxWorkParsed = (() => {
    if (!maxWork.trim()) return { lamports: 0n, invalid: false };
    try { return { lamports: parseSOL(maxWork), invalid: false }; } catch { return { lamports: 0n, invalid: true }; }
  })();

  if (!pool) return null;

  const onAct = async (row: NativePositionRow, action: PositionAction) => {
    if (action.kind === 'close') {
      try {
        const tx = await closeMut.mutateAsync({ nonce: row.position.nonce });
        setSentTxs([{ tx, label: 'Closing the position' }]);
      } catch {
        // closeMut.error is rendered below
      }
      return;
    }
    if (action.kind === 'withdraw' || action.kind === 'settle') setSheetRow(row);
  };

  return (
    <div data-testid="native-positions">
      <div className="flex items-center justify-between mb-3">
        <h3 className="font-display text-base mc-text-primary">Native positions</h3>
        <span className="text-[10px] px-1.5 py-0.5 rounded border border-white/10 bg-white/5 mc-text-cyan uppercase tracking-wider">on-chain program</span>
      </div>

      <NativeProcessingBanner pool={pool} onNavigateToGameSetup={onNavigateToGameSetup} />

      {active.length > 0 ? (
        <div className="space-y-3">
          {active.map((row) => (
            <NativePositionCard key={row.position.nonce.toString()} row={row} pool={pool} busy={closeMut.isPending} onAct={onAct} />
          ))}
        </div>
      ) : (
        <p className="text-sm mc-text-muted text-center py-4">No open native positions.</p>
      )}

      <details className="mt-3 text-xs">
        <summary className="cursor-pointer mc-text-muted hover:mc-text-primary">Advanced</summary>
        <div className="mt-2 space-y-1">
          <label className="mc-label" htmlFor="native-max-work">Max work contribution (SOL)</label>
          <input
            id="native-max-work"
            type="text"
            inputMode="decimal"
            value={maxWork}
            onChange={(e) => setMaxWork(e.target.value)}
            placeholder="0"
            className="mc-input w-40 text-xs px-2 py-1 font-mono"
          />
          <p className="mc-text-muted">Owner contribution to the round's processing costs, 0 by default. Only a payout that closes the round can use it.</p>
          {maxWorkParsed.invalid && <p className="mc-text-danger">Enter a SOL amount like 0.05.</p>}
        </div>
      </details>

      {closeMut.isError && <div className="mc-status-red p-2 mt-3 text-xs text-center">{describeNativeError(closeMut.error)}</div>}
      {sentTxs.length > 0 && (
        <div className="mt-3 space-y-2" data-testid="native-sent-txs">
          {sentTxs.map((item, index) => {
            const last = index === sentTxs.length - 1;
            return (
              <NativeTxStatusCard
                key={item.tx.signature}
                tx={item.tx}
                label={item.label}
                compact={!last}
                onTryAgain={() => setSentTxs([])}
                actions={(view) => (last && view.terminal ? (
                  <button type="button" onClick={() => setSentTxs([])} className="mc-btn-secondary px-3 py-1 rounded-full text-xs mt-1">Dismiss</button>
                ) : null)}
              />
            );
          })}
        </div>
      )}

      {closed.length > 0 && (
        <details className="mt-3 text-xs">
          <summary className="cursor-pointer mc-text-muted hover:mc-text-primary">Closed native positions ({closed.length})</summary>
          <div className="mt-2 space-y-1">
            {closed.map(({ position }) => (
              <div key={position.nonce.toString()} className="flex justify-between mc-text-muted">
                <span>{PLAN_LABEL[planOfPosition(position.kind) ?? 'Simple']} · round {position.roundId.toString()}</span>
                <span>{formatSOL(position.principal)} SOL deposited · {formatSOL(position.netPayout)} SOL received</span>
              </div>
            ))}
          </div>
        </details>
      )}

      <NativePayoutSheet
        row={sheetRow}
        pool={pool}
        maxWorkLamports={maxWorkParsed.lamports}
        maxWorkInvalid={maxWorkParsed.invalid}
        onClose={() => setSheetRow(null)}
        onSent={(items) => setSentTxs(items)}
      />
    </div>
  );
}

/* ================================================================
   Main Export
   ================================================================ */

interface GameTrackingProps {
  onNavigateToGameSetup?: () => void;
  onTabChange?: (tab: TabType) => void;
  visible?: boolean;
}

export default function GameTracking({ onNavigateToGameSetup, onTabChange, visible = true }: GameTrackingProps) {
  const { data: games, isLoading, error } = useGetUserGames();
  const { walletType, solanaPubkey } = useWallet();
  const solGamesQuery = useGetUserSolGames();
  const solGames: SolGameRecord[] = walletType === 'siws' ? (solGamesQuery.data ?? []) : [];
  const { data: ponziData } = useGetPonziPoints();
  const { data: shenaniganConfigs } = useGetShenaniganConfigs();
  const { data: mintConfig } = useGetMintConfig();
  const simplePpPerIcp = mintConfig ? Number(mintConfig.simple21DayPpPerIcp) : 0;
  const comp15PpPerIcp = mintConfig ? Number(mintConfig.compounding15DayPpPerIcp) : 0;
  const comp30PpPerIcp = mintConfig ? Number(mintConfig.compounding30DayPpPerIcp) : 0;
  const portfolio = useLivePortfolio(games);
  const solPortfolio = useLivePortfolio(solGames);
  // Native rail (the Musical Chairs program) is the default SOL view (DR-22) whenever its Pool
  // exists on the connected cluster; the legacy canister positions stay in their own section.
  const nativeAvailability = useNativeAvailability();
  const nativePortfolio = useNativePortfolio();
  const withdrawMutation = useWithdrawGameEarnings();
  const settleMutation = useSettleCompoundingGame();
  const solWithdrawMutation = useWithdrawSolGameEarnings();
  // The Profit Center is single-denomination per session: SIWS users transact in
  // SOL, everyone else in ICP. The running tally + positions follow the active
  // wallet. (A SIWS principal never holds ICP positions and vice-versa.)
  const isSiws = walletType === 'siws';
  const showNative = isSiws && nativeAvailability.state === 'native';
  // Hero tally follows the default rail: native when available, else legacy. Never a sum of the two.
  const tallyDeposits = showNative ? lamportsToFloat(nativePortfolio.totalDeposits) : isSiws ? solPortfolio.totalDeposits : portfolio.totalDeposits;
  const tallyEarned = showNative ? lamportsToFloat(nativePortfolio.totalEarned) : isSiws ? solPortfolio.totalEarnings : portfolio.totalEarnings;
  const netPL = tallyEarned - tallyDeposits;
  const tallyUnit = isSiws ? 'SOL' : 'ICP';
  // Format a (non-negative) tally amount in the active denomination. Net P/L
  // prepends its own sign and passes the magnitude, since it can go negative.
  const fmtTallyAmt = (v: number) => (isSiws ? formatSolFloat(v) : formatICPDisplay(v));

  // Collapse state — both sections start closed
  const [feesExpanded, setFeesExpanded] = useState(false);
  const [ppExpanded, setPpExpanded] = useState(false);

  // Live values — no countUp animation since portfolio updates every second
  // and countUp fights with the live tick, causing erratic jumps
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
      if (selectedGame.isCompounding) {
        const result = await settleMutation.mutateAsync(selectedGame.id);
        setWithdrawnAmount(result.earnings);
      } else {
        const result = await withdrawMutation.mutateAsync(selectedGame.id);
        setWithdrawnAmount(result.earnings);
      }
      setWithdrawDialogOpen(false);
      setSelectedGame(null);
      triggerConfetti();
      setReinvestDialogOpen(true);
    } catch (err) {
      console.error('Withdrawal failed:', err);
    }
  };

  // SOL positions pay out directly to the connected Phantom wallet (no toll
  // dialog — the SOL plans have their own fee schedule applied server-side).
  // The mutation refuses to run without a connected SIWS pubkey, so funds can
  // never be sent to the unspendable canister-derived deposit address.
  const handleSolWithdraw = async (game: SolGameRecord) => {
    try {
      await solWithdrawMutation.mutateAsync({ gameId: game.id, isCompounding: game.isCompounding });
      triggerConfetti();
    } catch (err) {
      console.error('SOL withdrawal failed:', err);
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
        <p className="font-accent italic mb-1">Even Charles couldn't fix this one.</p>
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
          <div className="flex items-center justify-between mb-3">
            <h2 className="font-display text-lg mc-text-primary">Your Running Tally</h2>
            <span className="inline-flex items-center gap-1.5 text-xs mc-text-muted">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75" />
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
              </span>
              LIVE
            </span>
          </div>
          {/* Hero P/L number — animated countUp */}
          <div className="mb-4">
            <div className="mc-label mb-1">Net P/L</div>
            <div className={`text-2xl sm:text-4xl font-bold flex items-center justify-center gap-2 ${
              netPL >= 0 ? 'mc-text-green mc-glow-green' : 'mc-text-danger'
            }`}>
              {netPL >= 0 ? <TrendingUp className="h-6 w-6" /> : <TrendingDown className="h-6 w-6" />}
              {netPL >= 0 ? '+' : '-'}{fmtTallyAmt(Math.abs(netPL))} {tallyUnit}
            </div>
          </div>
          {showNative && (
            <p className="text-[10px] mc-text-muted mb-2">Native positions only. Legacy (Chain Fusion) positions are tallied in their own section below.</p>
          )}
          <div className="grid grid-cols-2 gap-4">
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Deposited</div>
              <div className="text-xl font-bold mc-text-primary">{fmtTallyAmt(tallyDeposits)} {tallyUnit}</div>
            </div>
            <div className="mc-card p-4 text-center">
              <div className="mc-label mb-1">Earned</div>
              <div className="text-xl font-bold mc-text-green mc-glow-green">{fmtTallyAmt(tallyEarned)} {tallyUnit}</div>
            </div>
          </div>
        </div>

        {/* Positions — SIWS users hold SOL positions; everyone else holds ICP.
            One unified section so a SOL user never sees an empty ICP card. */}
        <div className="mc-card-elevated">
          <h2 className="font-display text-lg mc-text-primary mb-4">Your Positions</h2>

          {isSiws ? (
            <>
              {showNative && (
                <NativePositionsSection portfolio={nativePortfolio} onNavigateToGameSetup={onNavigateToGameSetup} />
              )}

              {/* Legacy (Chain Fusion) rail: the existing positions and withdrawals, unchanged. */}
              {(solGames.length > 0 || !showNative) && (
                <div className={showNative ? 'mt-6 pt-4 border-t border-white/10' : undefined} data-testid="legacy-positions">
                  {showNative && (
                    <div className="mb-3">
                      <h3 className="font-display text-base mc-text-primary">Legacy (Chain Fusion) positions</h3>
                      <p className="text-xs mc-text-muted">
                        Positions opened through the ponzi_math_sol canister. Withdrawals work exactly as before and are not part of the native tally above.
                        Tally: {formatSolFloat(solPortfolio.totalDeposits)} SOL deposited · {formatSolFloat(solPortfolio.totalEarnings)} SOL earned.
                      </p>
                    </div>
                  )}
                  {solGames.length > 0 ? (
                    <div className="space-y-3">
                      {[...solPortfolio.games]
                        .sort((a, b) => getPositionUrgency(a.game) - getPositionUrgency(b.game))
                        .map(({ game, earnings }) => (
                          <PositionCard
                            key={game.id.toString()}
                            game={game}
                            earnings={earnings}
                            denomination="SOL"
                            settleLabel="Settle"
                            withdrawDisabled={!solanaPubkey}
                            withdrawDisabledTitle="Reconnect your Solana wallet to withdraw"
                            onWithdraw={(g) => handleSolWithdraw(g as SolGameRecord)}
                            withdrawPending={
                              solWithdrawMutation.isPending &&
                              solWithdrawMutation.variables?.gameId === game.id
                            }
                          />
                        ))}
                    </div>
                  ) : (
                    <EmptyState onNavigate={onNavigateToGameSetup} />
                  )}

                  {solWithdrawMutation.isError && (
                    <div className="mt-3 mc-status-red p-2 text-xs text-center">
                      {solWithdrawMutation.error?.message || 'SOL withdrawal failed'}
                    </div>
                  )}
                  {solGames.length > 0 && (
                    <div className="mt-3 text-[10px] mc-text-muted italic text-center">
                      Withdrawals are sent to your connected Phantom wallet.
                    </div>
                  )}
                </div>
              )}

              {showNative && solGames.length === 0 && nativePortfolio.rows.length === 0 && !nativePortfolio.isLoading && (
                <EmptyState onNavigate={onNavigateToGameSetup} />
              )}
            </>
          ) : (
            <>
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
                      withdrawPending={withdrawMutation.isPending || settleMutation.isPending}
                    />
                  ))}
                </div>
              )}

              {(withdrawMutation.isError || settleMutation.isError) && (
                <div className="mc-status-red p-3 mt-3 text-center text-sm">
                  {withdrawMutation.error?.message || settleMutation.error?.message || 'Withdrawal failed'}
                </div>
              )}
            </>
          )}
        </div>

        {/* Fee disclosure */}
        <div className="mc-house-card">
          <button
            type="button"
            onClick={() => setFeesExpanded(v => !v)}
            className="w-full flex items-center justify-between text-left"
            aria-expanded={feesExpanded}
          >
            <span className="font-semibold mc-text-primary">Fee Disclosure</span>
            <ChevronDown className={`w-4 h-4 transition-transform ${feesExpanded ? 'rotate-180' : ''}`} />
          </button>
          {feesExpanded && (
            <div className="mt-3 text-sm mc-text-muted space-y-2">
              <p className="font-accent italic">A small reinvestment keeps the engine running — and your returns flowing.</p>
              <p>Simple positions: {pctPrecise(EXIT_TOLL_EARLY)} carried interest within {EXIT_TOLL_EARLY_DAYS} days, {pctPrecise(EXIT_TOLL_MID)} within {EXIT_TOLL_MID_DAYS} days, {pctPrecise(EXIT_TOLL_LATE)} after.</p>
              <p>Compounding plans: 9% carry (15-day) or 13% carry (30-day) at maturity.</p>
              <p>Your deposit is deployed into AUM to fund interest obligations. Your position is an entitlement to future interest from AUM.</p>
            </div>
          )}
        </div>

        {/* Ponzi Points section */}
        <div className="mc-card-elevated">
          <button
            type="button"
            onClick={() => setPpExpanded(v => !v)}
            className="w-full flex items-center justify-between text-left"
            aria-expanded={ppExpanded}
          >
            <span className="font-semibold mc-text-primary">
              Ponzi Points ({(ponziData?.totalPoints ?? 0).toLocaleString()})
            </span>
            <ChevronDown className={`w-4 h-4 transition-transform ${ppExpanded ? 'rotate-180' : ''}`} />
          </button>
          {ppExpanded && (
            <div className="mt-3">
              {/* PP balance */}
              <div className="text-center mb-4">
                <div className="mc-label mb-1">Your Balance</div>
                <div className="text-2xl font-bold mc-text-purple mc-glow-purple">
                  {(ponziData?.totalPoints || 0).toLocaleString()} PP
                </div>
              </div>

              {/* Earn rates comparison table */}
              <div className="mb-4">
                <div className="mc-label mb-2">Earn Rates</div>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2 text-center text-xs">
                  <div className="mc-card p-3">
                    <div className="mc-text-green font-bold text-sm">{simplePpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Simple 21-day</div>
                  </div>
                  <div className="mc-card p-3">
                    <div className="mc-text-purple font-bold text-sm">{comp15PpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Compound 15-day</div>
                  </div>
                  <div className="mc-card p-3">
                    <div className="mc-text-gold font-bold text-sm">{comp30PpPerIcp.toLocaleString()}</div>
                    <div className="mc-text-muted">PP per ICP</div>
                    <div className="mc-text-dim mt-1">Compound 30-day</div>
                  </div>
                </div>
              </div>

              {/* PP custody breakdown */}
              {ponziData && (
                <div className="mb-4">
                  <div className="mc-label mb-2">Custody</div>
                  <div className="text-xs space-y-1.5">
                    <div className="flex justify-between">
                      <span className="mc-text-muted">Side pocket (spendable)</span>
                      <span className="mc-text-green font-bold">{(ponziData.chipPoints || 0).toLocaleString()} PP</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="mc-text-muted">Wallet (external)</span>
                      <span className="mc-text-cyan font-bold">{(ponziData.walletPoints || 0).toLocaleString()} PP</span>
                    </div>
                  </div>
                </div>
              )}

              {/* Spending suggestions — affordability uses costSuccess
                  (the upfront commitment). A worse outcome may charge more
                  but the backend clamps to balance if they're short. */}
              {(() => {
                const pp = ponziData?.totalPoints || 0;
                if (pp < 100 || !shenaniganConfigs) return null;
                const affordable = shenaniganConfigs
                  .filter(c => Number(c.costSuccess) <= pp)
                  .sort((a, b) => Number(a.costSuccess) - Number(b.costSuccess))
                  .slice(0, 3);
                if (affordable.length === 0) return null;
                return (
                  <div>
                    <div className="mc-label mb-2">You can afford</div>
                    <div className="flex flex-wrap gap-2">
                      {affordable.map(s => (
                        <span key={s.name} className="text-xs mc-card px-2 py-1">
                          {s.name} <span className="mc-text-purple">({Number(s.costSuccess).toLocaleString()} PP)</span>
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })()}
            </div>
          )}
        </div>

        {/* Spend PP bridge CTA */}
        {(ponziData?.totalPoints || 0) >= 100 && onTabChange && (
          <div className="text-center">
            <button
              onClick={() => onTabChange('shenanigans')}
              className="mc-btn-secondary inline-flex items-center gap-2 mx-auto text-xs px-4 py-2 rounded-lg"
            >
              <Dice5 className="h-4 w-4 mc-text-purple" />
              Spend your {ponziData?.totalPoints?.toLocaleString()} PP on Shenanigans →
            </button>
          </div>
        )}
      </div>

      {/* Withdrawal Dialog — MobileSheet for bottom sheet on mobile */}
      <MobileSheet open={withdrawDialogOpen} onOpenChange={setWithdrawDialogOpen}>
        <div className="space-y-4">
          <h2 className="font-display text-lg mc-text-primary">Confirm Withdrawal</h2>
          <p className="mc-text-dim text-sm">
            {selectedGame && (() => {
              const info = getExitTollInfo(selectedGame);
              if (info.nextFee && info.timeToNext) {
                return (
                  <span>
                    You'll pay <strong className="mc-text-gold">{info.currentFee}%</strong> carried interest.
                    Wait <strong className="mc-text-cyan">{countdown}</strong> to reduce it to <strong className="mc-text-green">{info.nextFee}%</strong>.
                  </span>
                );
              }
              return <span>You'll pay <strong className="mc-text-gold">{info.currentFee}%</strong> carried interest.</span>;
            })()}
          </p>
          <div className="flex gap-2 justify-end">
            <Button variant="outline" onClick={() => setWithdrawDialogOpen(false)} className="mc-btn-secondary">
              Cancel
            </Button>
            <Button onClick={handleWithdrawConfirm} disabled={withdrawMutation.isPending || settleMutation.isPending} className="mc-btn-primary">
              {(withdrawMutation.isPending || settleMutation.isPending) ? 'Processing...' : 'Confirm'}
            </Button>
          </div>
        </div>
      </MobileSheet>

      {/* Post-withdrawal celebration toast */}
      {reinvestDialogOpen && (
        <div className="fixed inset-0 z-[9999] flex items-center justify-center p-4">
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
                className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2"
              >
                <Rocket className="h-4 w-4" /> YOLO Again
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
