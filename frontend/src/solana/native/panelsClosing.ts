// Round-closing and allocation-run orchestration for the native panels (IMPL-46c).
//
// Web3-free: only type imports from the client, so the decisions here are unit
// tested with a fake client (panelsClosing.test.ts) and the module can sit in
// the main chunk.
//
// Two things need pushing by somebody after a payout, and both are permissionless
// (the program lets any wallet send the step, the sender only pays the fee):
//
//   1. An ALLOCATION RUN: a payout that charges Carried Interest / a Jackpot Fee
//      starts one when Backers exist. Six typed steps (Discover, Senior, Seal,
//      Allocate, Prune, Commit) credit the Backers; payouts are refused
//      (AllocationPending) until it is idle again.
//   2. A ROUND CLOSE: a payout whose owed amount is strictly greater than the pot
//      (owed > pot) closes the round. Then, in order:
//        provision the Candidate pages (both sets) -> build_candidates until sealed
//        -> request_round_draw (randomized selection of one unprofitable
//        depositor to convert into a Series B Backer) -> wait for the randomness
//        -> capture_round_draw -> apply_promotion
//        (or expire_promotion once the 600 s deadline passes with no capture)
//        -> finalize_round -> the next round is open.
//      The allocation run of the closing payout must commit first (cash barrier).
//
// `nextProcessingStep` reads the on-chain state and names the ONE next step;
// `runProcessingStep` sends it. The UI exposes a single "resume processing" button
// that does exactly that per click.

import type { NativeClient, PayoutRequest } from './client';
import {
  DRAW_STATE,
  TICKET_PHASE,
  type FundingKind,
  type NativeCloseRecord,
  type NativeConfigVersion,
  type NativePool,
  type NativeTxResult,
  type StatusUpdate,
} from './types';
import { TICKET_PHASE_ORDER } from './panelsQuote';

export type ProcessingStepKind =
  | 'advance-allocation'
  | 'provision-page'
  | 'build-candidates'
  | 'request-selection'
  | 'await-randomness'
  | 'capture-randomness'
  | 'apply-promotion'
  | 'expire-promotion'
  | 'finalize-round'
  | 'blocked'
  | 'done';

export interface ProcessingStep {
  kind: ProcessingStepKind;
  /** Short button/step label (no casino words: "randomized selection", "allocation run"). */
  label: string;
  /** One sentence for the banner under the label. */
  detail: string;
  /** The step can be sent right now. */
  runnable: boolean;
  closeId: bigint | null;
  /** provision-page: which candidate page. */
  page?: { setTag: number; index: bigint };
}

/** Rows per build_candidates batch (the coordinator's fixture round has a handful of participants). */
export const CANDIDATE_BATCH_ROWS = 8;
/** The program's promotion deadline after the close starts (config.deadline_seconds), for copy only. */
export const PROMOTION_DEADLINE_COPY = '10 minutes';

const done: ProcessingStep = {
  kind: 'done',
  label: 'Nothing to process',
  detail: 'No allocation run or round close is waiting.',
  runnable: false,
  closeId: null,
};

const blocked = (detail: string, closeId: bigint | null): ProcessingStep => ({
  kind: 'blocked',
  label: 'Waiting',
  detail,
  runnable: false,
  closeId,
});

/** Whether the randomized-selection request looks fulfilled, from the request account's size; null when unknowable. */
export function requestLooksFulfilled(
  accountLength: number | null | undefined,
  config: Pick<NativeConfigVersion, 'pendingRequestBytes' | 'fulfilledRequestBytes'> | null | undefined,
): boolean | null {
  if (accountLength === null || accountLength === undefined || !config) return null;
  // The provider grows the request account when it fulfils it; a config that does
  // not distinguish the two sizes gives no signal.
  if (config.fulfilledRequestBytes <= config.pendingRequestBytes) return null;
  return accountLength >= config.fulfilledRequestBytes;
}

export function ticketPhaseLabel(phase: number | null | undefined): string | null {
  if (phase === null || phase === undefined || phase < 0 || phase >= TICKET_PHASE_ORDER.length) return null;
  return TICKET_PHASE_ORDER[phase];
}

/**
 * The one step that moves processing forward, from the pool, the close record
 * in progress and the clock.
 *
 * `randomnessReady`: true = the provider's answer is visible, false = still
 * pending, null = unknown (try the capture and let the program decide).
 */
export function nextProcessingStep(args: {
  pool: NativePool | null | undefined;
  close: NativeCloseRecord | null | undefined;
  ticketPhase: number | null | undefined;
  /** Unix seconds. */
  now: bigint;
  randomnessReady: boolean | null;
}): ProcessingStep {
  const { pool, close, now } = args;
  if (!pool) return done;
  const closeId = pool.currentCloseId;

  if (pool.activeTicket !== 0n) {
    const phase = ticketPhaseLabel(args.ticketPhase);
    const step = phase === null || args.ticketPhase === null || args.ticketPhase === undefined ? null : args.ticketPhase + 1;
    return {
      kind: 'advance-allocation',
      label: 'Resume processing',
      detail: phase
        ? `Allocation run in progress: step ${step} of ${TICKET_PHASE_ORDER.length} (${phase}). One transaction per click.`
        : 'Allocation run in progress. One transaction per click.',
      runnable: args.ticketPhase !== TICKET_PHASE.Idle,
      closeId,
    };
  }

  if (closeId === null) return done;
  if (!close) return blocked('Loading the round close record…', closeId);
  if (close.finalized) return done;

  if (close.promotion.kind !== 'Pending') {
    return close.reserveSettled
      ? {
          kind: 'finalize-round',
          label: 'Finalize round',
          detail: 'The randomized selection is settled. Finalizing carries the seed into AUM and opens the next round.',
          runnable: true,
          closeId,
        }
      : blocked("Settling the round's processing-cost reserve…", closeId);
  }

  if (!close.cashCommitted) {
    return blocked('Waiting for the closing allocation run to commit its cash.', closeId);
  }

  const pastDeadline = now > close.promotionDeadline;

  if (close.drawState === DRAW_STATE.Captured) {
    return {
      kind: 'apply-promotion',
      label: 'Apply Emergency Equity Conversion',
      detail: 'The randomness is captured. Applying it converts one unprofitable depositor into a Series B Backer.',
      runnable: true,
      closeId,
    };
  }

  if (pastDeadline) {
    return {
      kind: 'expire-promotion',
      label: 'Expire the selection',
      detail: `The ${PROMOTION_DEADLINE_COPY} promotion window passed without captured randomness. Expiring skips the conversion so the round can finalize.`,
      runnable: true,
      closeId,
    };
  }

  if (!close.sealed) {
    for (const setTag of [0, 1]) {
      const provisioned = setTag === 0 ? close.provisionedPreferredPages : close.provisionedFallbackPages;
      if (provisioned < close.requiredPagesPerSet) {
        return {
          kind: 'provision-page',
          label: 'Resume processing',
          detail: `Provisioning candidate page ${provisioned + 1n} of ${close.requiredPagesPerSet} for the ${setTag === 0 ? 'preferred' : 'fallback'} set.`,
          runnable: true,
          closeId,
          page: { setTag, index: provisioned },
        };
      }
    }
    const left = close.participantCount - close.candidateCursor;
    return {
      kind: 'build-candidates',
      label: 'Resume processing',
      detail: `Building the candidate sets: ${close.candidateCursor} of ${close.participantCount} participants visited (${left} left).`,
      runnable: true,
      closeId,
    };
  }

  if (close.drawState === DRAW_STATE.NotRequested) {
    return {
      kind: 'request-selection',
      label: 'Request randomized selection',
      detail: 'The candidates are sealed. Requesting randomness from the provider.',
      runnable: true,
      closeId,
    };
  }

  if (close.drawState === DRAW_STATE.Requested) {
    if (args.randomnessReady === false) {
      return {
        kind: 'await-randomness',
        label: 'Awaiting randomness',
        detail: "Awaiting randomness: the provider hasn't fulfilled the request yet. This usually takes a few seconds.",
        runnable: false,
        closeId,
      };
    }
    return {
      kind: 'capture-randomness',
      label: 'Capture randomness',
      detail: 'The provider has answered. Capturing the randomness.',
      runnable: true,
      closeId,
    };
  }

  // IgnoredLate arrives only after the deadline, which is handled above; anything else is unexpected.
  return blocked(`Unexpected randomness state ${close.drawState}.`, closeId);
}

/**
 * What to tell the user once processing has gone idle after `lastKind` landed (browser finding F3). Only
 * `finalize-round` opens a new round; the "Open a Position in the new round" call to action belongs
 * there and nowhere else. A plain allocation-run commit just means payouts work again.
 */
export function processingOutcome(lastKind: string): { message: string; opensNewRound: boolean } {
  if (lastKind === 'finalize-round') {
    return { message: 'The round is finalized. The next round is open.', opensNewRound: true };
  }
  if (lastKind === 'advance-allocation') {
    return { message: 'The allocation run finished. Payouts are available again.', opensNewRound: false };
  }
  return { message: 'Processing is up to date.', opensNewRound: false };
}

/** Send the step `nextProcessingStep` named. One transaction. */
/** The NativeClient methods a processing step can call (advancePhase widened so a test double need not add `phase`/`rows`). */
export type ProcessingClient = Pick<NativeClient, 'provisionPage' | 'buildCandidates' | 'requestDraw' | 'captureDraw' | 'applyPromotion' | 'expirePromotion' | 'finalizeRound'> & {
  advancePhase(p: { rows?: number; skipNewlyCapped?: boolean }): Promise<NativeTxResult>;
};

export async function runProcessingStep(client: ProcessingClient, step: ProcessingStep): Promise<NativeTxResult> {
  const closeId = step.closeId;
  switch (step.kind) {
    case 'advance-allocation':
      return client.advancePhase({});
    case 'provision-page':
      if (closeId === null || !step.page) throw new Error('provision-page needs a close id and a page');
      return client.provisionPage({
        page: { kind: 'Candidate', closeId, setTag: step.page.setTag },
        index: step.page.index,
        // The reserve locked by the closing payout covers exactly these pages.
        funding: 'LockedClose',
      });
    case 'build-candidates':
      if (closeId === null) throw new Error('build-candidates needs a close id');
      return client.buildCandidates({ closeId, rows: CANDIDATE_BATCH_ROWS });
    case 'request-selection':
      if (closeId === null) throw new Error('request-selection needs a close id');
      return client.requestDraw({ closeId });
    case 'capture-randomness':
      if (closeId === null) throw new Error('capture-randomness needs a close id');
      return client.captureDraw({ closeId });
    case 'apply-promotion':
      if (closeId === null) throw new Error('apply-promotion needs a close id');
      return client.applyPromotion({ closeId });
    case 'expire-promotion':
      if (closeId === null) throw new Error('expire-promotion needs a close id');
      return client.expirePromotion({ closeId });
    case 'finalize-round':
      if (closeId === null) throw new Error('finalize-round needs a close id');
      return client.finalizeRound({ closeId });
    default:
      throw new Error(`Nothing to send for step ${step.kind}`);
  }
}

// ---------------------------------------------------------------------------
// A payout that closes the round
// ---------------------------------------------------------------------------

/** Seconds of Simple accrual treated as "about to tip over" when deciding to prepare the close record early. */
const CLOSE_MARGIN_SECONDS = 120n;

/**
 * Whether the payout being quoted closes the round (owed > pot, strict) and
 * whether the close record must be prepared first. The record is also prepared
 * when the quote sits within two minutes of accrual of the pot: accrual keeps
 * running while the wallet prompt is open, and a payout that closes the round
 * without its record fails PageNotProvisioned.
 */
export function closingPayoutPlan(args: {
  owed: bigint;
  pot: bigint;
  /** Gross earnings accruing per second right now (Simple: principal x 11 / 8,640,000); 0 for Compound. */
  accrualPerSecondNumerator: bigint;
  closeRecordExists: boolean;
  nextCloseId: bigint;
}): { closesRound: boolean; prepareCloseId: bigint | null } {
  const closesRound = args.owed > args.pot;
  const margin = (args.accrualPerSecondNumerator * CLOSE_MARGIN_SECONDS) / 8_640_000n;
  const likely = args.owed + margin > args.pot;
  return { closesRound, prepareCloseId: likely && !args.closeRecordExists ? args.nextCloseId : null };
}

export type PayoutStage = 'provisioning-close-record' | 'sending-payout';

export interface PayoutOutcome {
  /** The close-record provisioning transaction, when one was sent. */
  provision: NativeTxResult | null;
  /** The payout transaction; null when provisioning has not confirmed yet. */
  payout: NativeTxResult | null;
}

const LANDED = new Set(['confirmed', 'finalized']);

/** Follow a transaction's ladder until it lands, fails, expires or the poll window ends. */
export async function waitUntilLanded(tx: NativeTxResult): Promise<StatusUpdate> {
  let last: StatusUpdate = { signature: tx.signature, status: tx.status };
  for await (const update of tx.status$) {
    last = update;
    if (LANDED.has(update.status) || update.status === 'failed' || update.status === 'expired') break;
  }
  return last;
}

export interface PayoutRunner {
  provisionCloseRecord(p: { closeId: bigint; funding: FundingKind }): Promise<NativeTxResult>;
  withdrawSimple(p: PayoutRequest): Promise<NativeTxResult>;
  settleCompounding(p: PayoutRequest): Promise<NativeTxResult>;
  readCloseRecord(closeId: bigint): Promise<NativeCloseRecord | null>;
}

/**
 * Send a payout, first provisioning the CloseRecord for `prepareCloseId` when it
 * does not exist yet (Funding::Caller: the sender fronts a refundable rent).
 * Returns with `payout: null` when the provisioning transaction has not landed,
 * so the caller can show its signature instead of sending a payout the program
 * would refuse (PageNotProvisioned).
 */
export async function runPayout(
  client: PayoutRunner,
  args: {
    kind: 'withdraw' | 'settle';
    request: PayoutRequest;
    prepareCloseId: bigint | null;
    onStage?: (stage: PayoutStage, tx: NativeTxResult | null) => void;
    wait?: (tx: NativeTxResult) => Promise<StatusUpdate>;
    sleep?: (ms: number) => Promise<void>;
  },
): Promise<PayoutOutcome> {
  const wait = args.wait ?? waitUntilLanded;
  const sleep = args.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  let provision: NativeTxResult | null = null;

  if (args.prepareCloseId !== null && !(await client.readCloseRecord(args.prepareCloseId))) {
    provision = await client.provisionCloseRecord({ closeId: args.prepareCloseId, funding: 'Caller' });
    args.onStage?.('provisioning-close-record', provision);
    const landed = await wait(provision);
    if (landed.status === 'failed' || landed.status === 'expired') {
      throw new Error('Preparing the close record did not land. Nothing was withdrawn; try again.');
    }
    if (!LANDED.has(landed.status)) return { provision, payout: null };
    // The payout reads the record at `confirmed`; give the node a moment to show it.
    let visible = false;
    for (let attempt = 0; attempt < 6 && !visible; attempt += 1) {
      visible = !!(await client.readCloseRecord(args.prepareCloseId));
      if (!visible) await sleep(500);
    }
    if (!visible) return { provision, payout: null };
  }

  args.onStage?.('sending-payout', null);
  const payout = args.kind === 'withdraw' ? await client.withdrawSimple(args.request) : await client.settleCompounding(args.request);
  return { provision, payout };
}
