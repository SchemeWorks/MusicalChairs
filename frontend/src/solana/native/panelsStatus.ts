// View-model for a native transaction's confirmation ladder (IMPL-46c). Pure and
// web3-free: components render whatever this returns, and panelsStatus.test.ts
// pins the wording and the rebuild/poll rules.
//
// The rules that matter (client.ts / status.ts):
//   * `unknown` is NOT failure. The signature (and for open_position the nonce)
//     stay valid, the same bytes may still land, and the only safe action is to
//     poll the SAME signature again (tx.retry()). Never rebuild while unknown.
//   * A new transaction is only safe once the status is `failed` or `expired`.

import { describeTransactionError } from './status';
import { describeNativeError, programErrorCopy } from './panelsQuote';
import { describeOrigin } from './panelsCluster';
import type { NativeTxResult, NativeTxStatus, StatusUpdate } from './types';

export type StatusTone = 'pending' | 'success' | 'warning' | 'danger';

export interface TxStatusView {
  status: NativeTxStatus;
  tone: StatusTone;
  headline: string;
  detail: string;
  /** The ladder reached the end (finalized, failed or expired). */
  terminal: boolean;
  /** Offer "Check status" (re-polls the same signature). */
  canPollAgain: boolean;
  /** Offer "Try again" (a NEW transaction is safe). */
  canRebuild: boolean;
}

const RANK: Record<NativeTxStatus, number> = { unknown: 0, submitted: 1, confirmed: 2, finalized: 3, failed: 3, expired: 3 };

/** The more advanced of two observations of the same signature (terminal beats pending, unknown loses to anything seen). */
export function furtherStatus(a: StatusUpdate | null | undefined, b: StatusUpdate | null | undefined): StatusUpdate | null {
  if (!a) return b ?? null;
  if (!b) return a;
  return RANK[b.status] >= RANK[a.status] ? b : a;
}

export function shortSignature(signature: string): string {
  return signature.length <= 18 ? signature : `${signature.slice(0, 8)}…${signature.slice(-8)}`;
}

export function describeTxStatus(update: Pick<StatusUpdate, 'status' | 'error'> | null | undefined, ctx: { label: string }): TxStatusView {
  const status: NativeTxStatus = update?.status ?? 'submitted';
  switch (status) {
    case 'submitted':
      return {
        status,
        tone: 'pending',
        headline: 'Submitted',
        detail: `${ctx.label} was sent. Waiting for the cluster to confirm it.`,
        terminal: false,
        canPollAgain: false,
        canRebuild: false,
      };
    case 'confirmed':
      return {
        status,
        tone: 'pending',
        headline: 'Confirmed',
        detail: `${ctx.label} landed. Waiting for it to finalize.`,
        terminal: false,
        canPollAgain: false,
        canRebuild: false,
      };
    case 'finalized':
      return { status, tone: 'success', headline: 'Finalized', detail: `${ctx.label} is final.`, terminal: true, canPollAgain: false, canRebuild: false };
    case 'unknown':
      return {
        status,
        tone: 'warning',
        headline: 'Outcome unknown',
        detail:
          `We could not tell whether ${ctx.label.toLowerCase()} landed. Your signature is saved below. ` +
          "Check its status again; don't send it twice, the same transaction may still land.",
        terminal: false,
        canPollAgain: true,
        canRebuild: false,
      };
    case 'failed': {
      const program = describeTransactionError(update?.error);
      return {
        status,
        tone: 'danger',
        headline: 'Failed',
        detail: program ? programErrorCopy(program.name) : `${ctx.label} was rejected by the cluster. Nothing changed.`,
        terminal: true,
        canPollAgain: false,
        canRebuild: true,
      };
    }
    case 'expired':
      return {
        status,
        tone: 'warning',
        headline: 'Expired',
        detail: `${ctx.label} never reached the cluster before its blockhash expired. Nothing changed, so it is safe to try again.`,
        terminal: true,
        canPollAgain: false,
        canRebuild: true,
      };
  }
}

// ---------------------------------------------------------------------------
// The two actions an unknown outcome offers (shared by every panel's status card)
// ---------------------------------------------------------------------------

export type StatusActionResult =
  | { ok: true; update: StatusUpdate; note: string | null }
  | { ok: false; error: string };

const errorText = (error: unknown): string | null => {
  if (error === null || error === undefined) return null;
  const text = error instanceof Error ? error.message : typeof error === 'string' ? error : (() => { try { return JSON.stringify(error); } catch { return String(error); } })();
  return text && text.length > 160 ? `${text.slice(0, 157)}...` : text;
};

/**
 * "Check status": the READ-ONLY reconcile() on the transaction's ORIGIN cluster. It re-reads the original
 * signature's status and never sends, rebuilds, re-signs or changes a nonce, and keeps working after the
 * app switched its active cluster. A failure is surfaced (never swallowed); a status that stays unknown
 * says so, with the last RPC error and where it looked.
 */
export async function checkTxStatus(tx: Pick<NativeTxResult, 'reconcile' | 'origin'>): Promise<StatusActionResult> {
  const origin = describeOrigin(tx.origin).label;
  try {
    const update = await tx.reconcile();
    if (update.status === 'unknown') {
      const detail = errorText(update.error);
      return {
        ok: true,
        update,
        note: `Still can't tell on ${origin}${detail ? `. Last RPC error: ${detail}` : ''}. Nothing was re-sent.`,
      };
    }
    return { ok: true, update, note: null };
  } catch (error) {
    return { ok: false, error: `Could not read the status on ${origin}: ${describeNativeError(error)}` };
  }
}

/**
 * "Resend the same transaction": retransmit the IDENTICAL signed bytes (same signature, so the cluster
 * cannot apply it twice). Never rebuilds, re-signs or changes the nonce. The adapter refuses it after a
 * cluster switch (NativeClusterSwitchedError); that is shown with the origin cluster instead of being retried.
 */
export async function resendSameTx(tx: Pick<NativeTxResult, 'retry' | 'origin'>): Promise<StatusActionResult> {
  try {
    const update = await tx.retry();
    return {
      ok: true,
      update,
      note: update.status === 'unknown'
        ? `The same signed transaction was sent again but is still not visible on ${describeOrigin(tx.origin).label}.`
        : null,
    };
  } catch (error) {
    return { ok: false, error: describeNativeError(error) };
  }
}
