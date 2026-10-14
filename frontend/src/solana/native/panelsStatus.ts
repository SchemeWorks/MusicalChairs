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
import { programErrorCopy } from './panelsQuote';
import type { NativeTxStatus, StatusUpdate } from './types';

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
