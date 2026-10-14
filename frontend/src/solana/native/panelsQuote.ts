// Pure display math + copy for the native Solana panels (IMPL-46c). No web3.js,
// no React: every number here mirrors the on-chain handler it previews, and
// panelsQuote.test.ts pins it to the golden vectors in solana/fixtures/.
//
// What is previewed (solana/programs/musical_chairs/src/positions.rs):
//   * open_position      Front-End Load = floor(gross x load_bps / 10,000) to Management;
//                        the rest is the pot credit. Interest runs on the GROSS deposit.
//   * withdraw_simple    owed = accrued_gross' - paid_gross, where accrual brings the
//                        position up to `now` (11 %/day, capped at maturity, remainder carried);
//                        paid_gross = min(owed, pot); Carried Interest = floor(paid x tier bps / 10,000)
//                        with the tier picked by seconds since open; net = paid - toll.
//   * settle_compounding at maturity only: owed = floor(principal x factor_q18 / 10^18),
//                        Jackpot Fee from config.jackpot_bps.
//   * close_simple_position  a matured Simple position with nothing left to withdraw.
//   * claim_backer       pays `funded` plus any `pending` whose allocation run is at or
//                        below pool.finalized_ticket.

import type { NativeBacker, NativeConfigVersion, NativePool, NativePosition, PlanKind } from './types';
import { POSITION_KIND_COMPOUND_15, POSITION_KIND_COMPOUND_30, POSITION_KIND_SIMPLE } from './types';

export const BPS = 10_000n;
export const SECONDS_PER_DAY = 86_400n;
/** 11 % per day expressed against the per-second accrual: rate_numerator x elapsed / 8,640,000. */
export const SIMPLE_DENOMINATOR = 8_640_000n;
export const SIMPLE_RATE_NUMERATOR = 11n;
export const Q18 = 10n ** 18n;

/**
 * Slippage allowed on the `minimum_receive` guard of a payout: 100 bps (1 %).
 *
 * Between the quote on screen and the transaction landing, only two things move
 * the net amount down: another payout shrinking the pot (the payout is capped at
 * the pot) and a second payout from the SAME position (a double click), which
 * leaves only seconds of fresh accrual. Accrual since the quote and a falling
 * Carried Interest tier only move it up. One percent absorbs ordinary pot
 * movement while still making a stale or duplicated payout fail in simulation
 * (MinimumReceive) instead of paying out almost nothing.
 */
export const DEFAULT_PAYOUT_SLIPPAGE_BPS = 100n;

export type PositionPlan = PlanKind;

export const PLAN_LABEL: Record<PositionPlan, string> = {
  Simple: '21-Day Simple',
  Compound15: '15-Day Compounding',
  Compound30: '30-Day Compounding',
};

export function planOfPosition(kind: number): PositionPlan | null {
  if (kind === POSITION_KIND_SIMPLE) return 'Simple';
  if (kind === POSITION_KIND_COMPOUND_15) return 'Compound15';
  if (kind === POSITION_KIND_COMPOUND_30) return 'Compound30';
  return null;
}

/** Invest-tab plan ids (getDailyRate / getPlanDays) -> the program's PlanKindArg names. */
export function planKindFromInvestId(planId: string): PlanKind {
  switch (planId) {
    case '15-day-compounding':
      return 'Compound15';
    case '30-day-compounding':
      return 'Compound30';
    case '21-day-simple':
    default:
      return 'Simple';
  }
}

const bps = (amount: bigint, rate: number | bigint): bigint => (amount * BigInt(rate)) / BPS;

// ---------------------------------------------------------------------------
// Opening a position
// ---------------------------------------------------------------------------

export interface OpenProjection {
  gross: bigint;
  /** Front-End Load, floored; routed to Management and never touches the pot. */
  frontEndLoad: bigint;
  /** Pot credit: gross - load. */
  potCredit: bigint;
  /** Simple only: gross x 11 % per day (interest runs on the GROSS deposit). */
  dailyLamports: bigint | null;
  /** Gross earnings if the plan runs its full term (before Carried Interest / Jackpot Fee). */
  maturityEarnings: bigint | null;
  /** Fee charged on a payout at maturity, in bps (Simple: the last Carried Interest tier). */
  maturityFeeBps: number | null;
}

/**
 * Preview of `open_position`. `config` supplies the pinned terms; without it
 * only the Front-End Load (from `fallbackLoadBps`) is previewed.
 */
export function projectOpen(args: {
  plan: PlanKind;
  gross: bigint;
  config: NativeConfigVersion | null | undefined;
  fallbackLoadBps?: number;
}): OpenProjection {
  const { plan, gross, config } = args;
  if (gross < 0n) throw new RangeError('projectOpen: gross cannot be negative');
  const loadBps = config?.frontEndLoadBps ?? args.fallbackLoadBps ?? 400;
  const frontEndLoad = bps(gross, loadBps);
  const base: OpenProjection = {
    gross,
    frontEndLoad,
    potCredit: gross - frontEndLoad,
    dailyLamports: null,
    maturityEarnings: null,
    maturityFeeBps: null,
  };
  if (!config) return base;
  if (plan === 'Simple') {
    const durationSeconds = BigInt(config.planDurationsS[0] ?? 0);
    const rateNumerator = BigInt(config.planRatesBpsPerDay[0] ?? 0) / 100n;
    return {
      ...base,
      dailyLamports: (gross * rateNumerator * SECONDS_PER_DAY) / SIMPLE_DENOMINATOR,
      maturityEarnings: (gross * rateNumerator * durationSeconds) / SIMPLE_DENOMINATOR,
      maturityFeeBps: config.simpleTollBps[config.simpleTollBps.length - 1] ?? null,
    };
  }
  const index = plan === 'Compound15' ? 0 : 1;
  const factor = config.compoundFactorsQ18[index];
  return {
    ...base,
    maturityEarnings: factor === undefined ? null : (gross * factor) / Q18,
    maturityFeeBps: config.jackpotBps[index] ?? null,
  };
}

/** Series A entitlement for a funded amount: floor(gross x aEntitlementBps / 10,000) (124 %). */
export function seriesAEntitlement(gross: bigint, config: Pick<NativeConfigVersion, 'aEntitlementBps'> | null | undefined): bigint {
  return bps(gross, config?.aEntitlementBps ?? 12_400);
}

// ---------------------------------------------------------------------------
// Quoting an existing position
// ---------------------------------------------------------------------------

export interface PositionQuote {
  plan: PositionPlan;
  /** The Position is terminal: paid out in full, closed, or its round has closed. */
  terminal: boolean;
  matured: boolean;
  elapsedSeconds: bigint;
  secondsToMaturity: bigint;
  /** Gross earnings owed (Simple: accrued to `now`; Compound: the at-maturity amount). */
  owed: bigint;
  /** min(owed, pot): what the payout can actually move. */
  paidGross: bigint;
  /** Carried Interest (Simple) or Jackpot Fee (Compound), in bps. */
  feeBps: number;
  fee: bigint;
  net: bigint;
  /** owed > pot (strict). */
  insolvent: boolean;
  /** Simple only: the next, lower tier and when it starts. */
  nextTier: { bps: number; inSeconds: bigint } | null;
  /** A payout instruction would currently be accepted by the position's own gates (not the pool's). */
  payable: boolean;
  /** close_simple_position would currently be accepted. */
  closable: boolean;
}

const MIN = (a: bigint, b: bigint) => (a < b ? a : b);

export function quotePosition(args: {
  position: NativePosition;
  config: NativeConfigVersion;
  /** pool.pot at the time of the quote. */
  pot: bigint;
  /** Unix seconds. */
  now: bigint;
}): PositionQuote | null {
  const { position, config, pot } = args;
  const plan = planOfPosition(position.kind);
  if (!plan) return null;
  const now = args.now;
  const elapsedSeconds = now > position.openedAt ? now - position.openedAt : 0n;
  const secondsToMaturity = position.maturityAt > now ? position.maturityAt - now : 0n;
  const matured = now >= position.maturityAt;

  let owed = 0n;
  let feeBps = 0;
  let nextTier: PositionQuote['nextTier'] = null;

  if (plan === 'Simple') {
    // SimpleAccrual::accrue on a copy, then owed = accrued_gross' - paid_gross.
    const end = MIN(now, position.maturityAt);
    let accrued = position.accruedGross;
    if (end > position.lastAccruedAt) {
      const elapsed = end - position.lastAccruedAt;
      const numerator = position.principal * SIMPLE_RATE_NUMERATOR * elapsed + position.remainder;
      accrued += numerator / SIMPLE_DENOMINATOR;
    }
    owed = accrued > position.paidGross ? accrued - position.paidGross : 0n;

    // simple_toll: tier by seconds since open against the pinned day bounds.
    const [earlyDays = 7, midDays = 14] = config.simpleTollDayBounds;
    const [earlyBps = 1_200, midBps = 750, lateBps = 300] = config.simpleTollBps;
    const earlyEnd = BigInt(earlyDays) * SECONDS_PER_DAY;
    const midEnd = BigInt(midDays) * SECONDS_PER_DAY;
    if (elapsedSeconds < earlyEnd) {
      feeBps = earlyBps;
      nextTier = { bps: midBps, inSeconds: earlyEnd - elapsedSeconds };
    } else if (elapsedSeconds < midEnd) {
      feeBps = midBps;
      nextTier = { bps: lateBps, inSeconds: midEnd - elapsedSeconds };
    } else {
      feeBps = lateBps;
    }
  } else {
    const index = plan === 'Compound15' ? 0 : 1;
    const factor = config.compoundFactorsQ18[index] ?? 0n;
    owed = (position.principal * factor) / Q18;
    feeBps = config.jackpotBps[index] ?? 0;
  }

  const terminal = position.claimed;
  const paidGross = MIN(owed, pot);
  const fee = bps(paidGross, feeBps);
  return {
    plan,
    terminal,
    matured,
    elapsedSeconds,
    secondsToMaturity,
    owed,
    paidGross,
    feeBps,
    fee,
    net: paidGross - fee,
    insolvent: owed > pot,
    nextTier,
    payable: !terminal && (plan === 'Simple' ? owed > 0n : matured),
    closable: !terminal && plan === 'Simple' && matured && owed === 0n,
  };
}

/** minimum_receive default: the quoted net less DEFAULT_PAYOUT_SLIPPAGE_BPS (floored). */
export function defaultMinimumReceive(quotedNet: bigint, slippageBps: bigint = DEFAULT_PAYOUT_SLIPPAGE_BPS): bigint {
  if (quotedNet <= 0n) return 0n;
  if (slippageBps < 0n || slippageBps > BPS) throw new RangeError('slippage must be between 0 and 10,000 bps');
  return (quotedNet * (BPS - slippageBps)) / BPS;
}

// ---------------------------------------------------------------------------
// What the action button should do
// ---------------------------------------------------------------------------

export type PositionActionKind = 'withdraw' | 'settle' | 'close' | 'locked' | 'none';

export interface PositionAction {
  kind: PositionActionKind;
  enabled: boolean;
  /** Why it is disabled (shown as the button title). */
  reason: string | null;
}

export const ALLOCATION_RUN_REASON = 'An allocation run is in progress. Resume processing first.';

/**
 * Which button a native position card shows. `pool` gates mirror payout_gate():
 * position round == pool round, not claimed, not closing, no active allocation run.
 */
export function positionAction(args: { position: NativePosition; quote: PositionQuote; pool: NativePool }): PositionAction {
  const { position, quote, pool } = args;
  const isCompound = quote.plan !== 'Simple';
  if (position.claimed || quote.terminal) return { kind: 'none', enabled: false, reason: 'This Position is closed.' };
  if (position.roundId !== pool.roundId) return { kind: 'none', enabled: false, reason: 'This Position belongs to a round that has already closed.' };

  if (isCompound && !quote.matured) return { kind: 'locked', enabled: false, reason: 'Locked until maturity' };

  if (quote.closable) return { kind: 'close', enabled: true, reason: null };

  const kind: PositionActionKind = isCompound ? 'settle' : 'withdraw';
  if (!quote.payable) return { kind, enabled: false, reason: 'No earnings yet' };
  if (pool.currentCloseId !== null) return { kind, enabled: false, reason: 'The round is closing. Payouts resume once it finalizes.' };
  if (pool.activeTicket !== 0n) return { kind, enabled: false, reason: ALLOCATION_RUN_REASON };
  return { kind, enabled: true, reason: null };
}

// ---------------------------------------------------------------------------
// Backers
// ---------------------------------------------------------------------------

export interface BackerQuote {
  entitlement: bigint;
  /** Credited so far (lifetime); never lowered by claims. */
  credited: bigint;
  remaining: bigint;
  /** funded + pending credit whose allocation run has finalized: what claim_backer pays right now. */
  claimable: bigint;
  /** Credit from an allocation run that has not finalized yet. */
  inProgress: bigint;
  creditedPct: number;
}

export function quoteBacker(backer: NativeBacker, finalizedTicket: bigint): BackerQuote {
  const materialized = backer.pendingTicket !== 0n && backer.pendingTicket <= finalizedTicket;
  const claimable = backer.funded + (materialized ? backer.pending : 0n);
  const inProgress = !materialized && backer.pendingTicket !== 0n ? backer.pending : 0n;
  const remaining = backer.entitlement > backer.lifetime ? backer.entitlement - backer.lifetime : 0n;
  const creditedPct = backer.entitlement > 0n ? Number((backer.lifetime * 10_000n) / backer.entitlement) / 100 : 0;
  return {
    entitlement: backer.entitlement,
    credited: backer.lifetime,
    remaining,
    claimable,
    inProgress,
    creditedPct: Math.min(100, creditedPct),
  };
}

// ---------------------------------------------------------------------------
// Allocation runs (the program's chunked toll distribution)
// ---------------------------------------------------------------------------

export const TICKET_PHASE_ORDER = ['Discover', 'Senior', 'Seal', 'Allocate', 'Prune', 'Commit'] as const;

export interface AllocationRunView {
  active: boolean;
  /** Phase name while active. */
  phase: string | null;
  /** 1-based step out of 6 while active. */
  step: number;
  steps: number;
}

export function allocationRunView(args: { pool: Pick<NativePool, 'activeTicket'> | null | undefined; ticketPhase: number | null | undefined }): AllocationRunView {
  const steps = TICKET_PHASE_ORDER.length;
  // pool.active_ticket != 0 is exactly the payout gate (AllocationPending); the
  // phase only decorates it while the Ticket account is readable.
  const active = !!args.pool && args.pool.activeTicket !== 0n;
  const phaseIndex = args.ticketPhase ?? -1;
  const phaseKnown = active && phaseIndex >= 0 && phaseIndex < steps;
  return {
    active,
    phase: phaseKnown ? TICKET_PHASE_ORDER[phaseIndex] : null,
    step: phaseKnown ? phaseIndex + 1 : 0,
    steps,
  };
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

const PROGRAM_ERROR_COPY: Record<string, string> = {
  AllocationPending: 'An allocation run is still in progress. Resume processing, then try again.',
  Closing: 'The round is closing. Payouts resume once it finalizes.',
  MinimumReceive: 'The payout would be smaller than your minimum receive because the pot changed. Review the quote and try again.',
  NotMature: 'This Position has not matured yet.',
  AlreadyClaimed: 'This Position has already been paid out or closed.',
  PositionRoundClosed: 'This Position belongs to a round that has already closed.',
  ReserveUnavailable:
    "This payout closes the round and the round's processing costs are not fully funded. Open Advanced and add an owner contribution, then try again.",
  MinimumDeposit: 'That amount is below the minimum.',
  QuotaExceeded: 'Three successful deposits per rolling hour is the limit. Try again later.',
  AdmissionPaused: 'New positions are paused right now.',
  StaleRound: 'The round changed while you were signing. Refresh and try again.',
  StaleConfig: 'The plan terms changed while you were signing. Refresh and try again.',
  Replay: 'That Position already exists. Check your positions before sending again.',
  PublicSeriesADisabled: "Series A isn't open to the public on this cluster yet.",
  NothingToClaim: 'Nothing is claimable yet. Credit from a running allocation run becomes claimable once it finalizes.',
  InsufficientOperations: "Operations funding can't cover this step right now.",
  PageNotProvisioned: 'A required account is not provisioned yet. Try again shortly.',
  InvalidPhase: 'The allocation run moved on. Refresh and resume processing.',
  StaleCursor: 'The allocation run moved on. Refresh and resume processing.',
  StaleTicket: 'The allocation run moved on. Refresh and resume processing.',
  CursorMismatch: 'The allocation run moved on. Refresh and resume processing.',
  ProtectedDestination: 'That payout destination is not allowed.',
};

/** User-facing sentence for a program error name (IDL error names), or a neutral fallback. */
export function programErrorCopy(name: string | undefined | null): string {
  if (!name) return 'The program rejected the transaction.';
  return PROGRAM_ERROR_COPY[name] ?? `The program rejected the transaction (${name}).`;
}

const SIGNED_MISMATCH_COPY: Record<string, string> = {
  'fee-payer': 'Your wallet changed the fee payer of the transaction. Nothing was sent. Reconnect your wallet and retry.',
  'message-mutated': 'Your wallet returned a different transaction than the one it was asked to sign. Nothing was sent. Reconnect your wallet and retry.',
  signature: 'Your wallet did not return a valid signature from your session account. Nothing was sent. Reconnect your wallet and retry.',
};

const CLUSTER_MISMATCH_COPY: Record<string, string> = {
  'genesis-hash': "The connected Solana RPC isn't the cluster this app expects (its genesis hash differs). Nothing was read or signed. Check the RPC endpoint.",
  'cluster-domain': 'The program on the connected cluster belongs to a different deployment. Nothing was signed.',
  'program-id': 'This build is configured for a different program than the one requested. Nothing was signed.',
};

/** Copy for a pre-sign simulation rejection (kind from classifySimulationFailure) or a node preflight rejection. */
function rejectedCopy(e: { programError?: { name?: string } | null; kind?: string; phase?: string }, message: string): string {
  const suffix = e.phase === 'simulation' ? ' Nothing was signed.' : '';
  if (e.kind === 'insufficient-funds' || /insufficient lamports|insufficient funds|no record of a prior credit/i.test(message)) {
    return `Your wallet doesn't have enough SOL for this amount plus network fees and one-time account rent.${suffix}`;
  }
  if (e.kind === 'missing-account') {
    return `A required account is missing or not set up on this cluster yet.${suffix}`;
  }
  if (e.programError?.name) return `${programErrorCopy(e.programError.name)}${suffix}`;
  if (e.kind === 'page' || e.kind === 'budget' || e.kind === 'size' || e.kind === 'batch') {
    return `That batch was too large for one transaction. Try the step again; it retries with a smaller batch.${suffix}`;
  }
  const firstLine = message.split('\n')[0].trim();
  return firstLine.length > 220 ? `${firstLine.slice(0, 217)}...` : firstLine;
}

/**
 * One sentence for any error a native action or read can throw. Matches by error `name` (and the
 * `reason` / `kind` / `phase` fields the adapter's typed errors carry) so this file never imports
 * client.ts and with it web3.js.
 */
export function describeNativeError(error: unknown): string {
  if (error === null || error === undefined) return 'Something went wrong.';
  const e = error as {
    name?: string;
    message?: string;
    reason?: string;
    kind?: string;
    phase?: string;
    programError?: { name?: string } | null;
  };
  const message = typeof e.message === 'string' ? e.message : String(error);
  switch (e.name) {
    case 'NativeClusterMismatchError':
      return CLUSTER_MISMATCH_COPY[e.reason ?? ''] ?? 'The connected cluster is not the one this app expects. Nothing was signed.';
    case 'NativeClusterNotConfiguredError':
      return "The native game isn't available on this cluster yet.";
    case 'NativeClusterError':
      return message || 'The native game could not verify the connected cluster.';
    case 'SignedTransactionMismatchError':
      return SIGNED_MISMATCH_COPY[e.reason ?? ''] ?? message;
    case 'NativeSimulationUnavailableError':
      return 'Could not check the transaction with the Solana RPC before signing (unavailable or slow). Nothing was signed; try again in a moment.';
    case 'NativeSendRejectedError':
      return rejectedCopy(e, message);
    default:
      break;
  }
  if (/reject|denied|cancel|declin/i.test(message)) return 'Wallet request was cancelled.';
  // WalletMismatchError / NativeStateError / RangeError messages are already written for people.
  return message || 'Something went wrong.';
}
