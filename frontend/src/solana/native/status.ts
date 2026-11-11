// Confirmation ladder + unknown-outcome handling for one submitted transaction.
//
// Web3-free (type-only imports): it drives any object with a
// `getSignatureStatuses` (and optionally `getBlockHeight`), so it stays out of
// the main bundle's web3.js dependency and is trivially testable with injected
// responses. There is NO websocket: the asset-canister CSP only allows https
// connect-src, and web3.js `confirmTransaction` waits on `signatureSubscribe`
// over wss (see sendSolDeposit.ts). Polling `getSignatureStatuses` over https is
// the whole mechanism.
//
// States (types.ts NativeTxStatus):
//   submitted -> confirmed -> finalized      landed
//   submitted -> failed                      landed with a program error (state rolled back)
//   unknown                                  could not tell (RPC died / not seen in the window)
//   expired                                  blockhash height passed and the signature never appeared
//
// `unknown` is not failure and not success. The signature stays valid; the
// caller polls the SAME signature again. A new transaction (and for
// open_position, a reused nonce) is only safe once the status is `failed` or
// `expired` (`safeToRebuild`).

import { programErrorName } from './codecs';
import type { NativeTxStatus, StatusUpdate } from './types';

export interface SignatureStatusValue {
  slot: number;
  confirmations: number | null;
  err: unknown | null;
  confirmationStatus?: 'processed' | 'confirmed' | 'finalized';
}

/** The slice of web3.js `Connection` the tracker needs. */
export interface StatusRpc {
  getSignatureStatuses(
    signatures: string[],
    config?: { searchTransactionHistory?: boolean },
  ): Promise<{ value: Array<SignatureStatusValue | null> }>;
  getBlockHeight?(commitment?: 'confirmed' | 'finalized'): Promise<number>;
}

export interface StatusTrackerOptions {
  rpc: StatusRpc;
  signature: string;
  /** `submitted` when the node accepted the bytes; `unknown` when the send itself failed ambiguously. */
  initial: Extract<NativeTxStatus, 'submitted' | 'unknown'>;
  /** The error behind an `unknown` start (e.g. the RPC timeout). */
  initialError?: unknown;
  /** Polls per stream/settle window (default 40). */
  maxPolls?: number;
  /** Delay between polls in ms (default 1500). */
  pollIntervalMs?: number;
  /** Injected for tests; defaults to a setTimeout sleep. */
  sleep?: (ms: number) => Promise<void>;
  /** The transaction's blockhash validity bound; enables `expired` detection. */
  lastValidBlockHeight?: number;
}

const DEFAULT_MAX_POLLS = 40;
const DEFAULT_POLL_INTERVAL_MS = 1_500;
const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

const RANK: Record<NativeTxStatus, number> = {
  unknown: 0,
  submitted: 1,
  confirmed: 2,
  finalized: 3,
  failed: 3,
  expired: 3,
};

export const isTerminalStatus = (status: NativeTxStatus): boolean =>
  status === 'finalized' || status === 'failed' || status === 'expired';

/** Pull the program error code out of a cluster transaction error or an RPC error message. */
export function describeTransactionError(err: unknown): { code: number; name: string } | null {
  let code: number | null = null;
  const direct = (err as { InstructionError?: unknown } | null)?.InstructionError;
  if (Array.isArray(direct) && typeof direct[1] === 'object' && direct[1] !== null && 'Custom' in direct[1]) {
    code = Number((direct[1] as { Custom: number }).Custom);
  }
  if (code === null) {
    let text = '';
    try {
      text =
        typeof err === 'string'
          ? err
          : JSON.stringify(err, (_k, v: unknown) => (typeof v === 'bigint' ? v.toString() : v)) ?? '';
    } catch {
      text = '';
    }
    if (err instanceof Error) text += ` ${err.message}`;
    const logs = (err as { logs?: unknown } | null)?.logs;
    if (Array.isArray(logs)) text += ` ${logs.join(' ')}`;
    const custom = /"Custom"\s*:\s*(\d+)/.exec(text);
    const hex = /custom program error: 0x([0-9a-fA-F]+)/.exec(text);
    const anchor = /Error Number: (\d+)/.exec(text);
    if (custom) code = Number(custom[1]);
    else if (hex) code = parseInt(hex[1], 16);
    else if (anchor) code = Number(anchor[1]);
  }
  if (code === null || !Number.isFinite(code)) return null;
  return { code, name: programErrorName(code) ?? 'Unknown' };
}

// ---------------------------------------------------------------------------
// Pre-sign simulation failures
// ---------------------------------------------------------------------------

export type SimulationFailureKind =
  /** PageNotProvisioned: the batch ran into a page the keeper did not supply (shrinkable). */
  | 'page'
  /** Compute budget / instruction trace exhausted (shrinkable). */
  | 'budget'
  /** The assembled transaction does not fit the packet (shrinkable). */
  | 'size'
  /** DuplicateWritableAccount: two rows of the batch share a writable account (shrinkable). */
  | 'batch'
  | 'insufficient-funds'
  | 'missing-account'
  /** Any other program error: deterministic, not fixed by fewer rows. */
  | 'program'
  | 'unknown';

export interface SimulationFailure {
  kind: SimulationFailureKind;
  program: { code: number; name: string } | null;
  /** One readable line: the program error name and code, or the cluster's error value. */
  reason: string;
}

/** Failures a smaller batch can cure. Every other kind is deterministic for the same state. */
export const SHRINKABLE_FAILURES: readonly SimulationFailureKind[] = ['page', 'budget', 'size', 'batch'];

const ANCHOR_MISSING_ACCOUNT_CODES = new Set([3005, 3012]); // AccountNotEnoughKeys, AccountNotInitialized
const BUDGET_INSTRUCTION_ERRORS = new Set([
  'ComputationalBudgetExceeded',
  'MaxInstructionTraceLengthExceeded',
  'CallDepth',
  'ProgramFailedToComplete',
]);
const MISSING_ACCOUNT_ERRORS = new Set(['AccountNotFound', 'ProgramAccountNotFound', 'MissingAccount', 'UninitializedAccount', 'NotEnoughAccountKeys']);
const FUNDS_ERRORS = new Set(['InsufficientFundsForFee', 'InsufficientFundsForRent', 'InsufficientFunds']);

/** Classify the `err` + logs of a failed `simulateTransaction`. Never throws. */
export function classifySimulationFailure(err: unknown, logs?: readonly string[] | null): SimulationFailure {
  const logText = (logs ?? []).join('\n');
  const program = describeTransactionError({ err, logs: logs ?? [] });
  const named = program && program.name !== 'Unknown' ? program : null;

  // The cluster's error value: a bare string, {Variant: ...}, or {InstructionError: [index, detail]}.
  const top = typeof err === 'string' ? err : err && typeof err === 'object' ? Object.keys(err)[0] : undefined;
  const detail = Array.isArray((err as { InstructionError?: unknown } | null)?.InstructionError)
    ? (err as { InstructionError: unknown[] }).InstructionError[1]
    : undefined;
  const detailName = typeof detail === 'string' ? detail : undefined;

  let kind: SimulationFailureKind = 'unknown';
  if (named?.name === 'PageNotProvisioned') kind = 'page';
  else if (named?.name === 'DuplicateWritableAccount') kind = 'batch';
  else if (named) kind = 'program';

  if (kind === 'unknown' || kind === 'program') {
    if ((detailName && BUDGET_INSTRUCTION_ERRORS.has(detailName)) || /exceeded CUs meter|computational budget exceeded|exceeded maximum number of instructions/i.test(logText)) {
      kind = 'budget';
    } else if (kind === 'unknown') {
      if ((top && FUNDS_ERRORS.has(top)) || (detailName && FUNDS_ERRORS.has(detailName)) || /insufficient lamports|insufficient funds/i.test(logText)) {
        kind = 'insufficient-funds';
      } else if (
        (top && MISSING_ACCOUNT_ERRORS.has(top)) ||
        (detailName && MISSING_ACCOUNT_ERRORS.has(detailName)) ||
        (program && ANCHOR_MISSING_ACCOUNT_CODES.has(program.code))
      ) {
        kind = 'missing-account';
      }
    }
  }

  let reason: string;
  if (named) reason = `${named.name} (${named.code})`;
  else if (program && ANCHOR_MISSING_ACCOUNT_CODES.has(program.code)) reason = `account not initialized or missing (${program.code})`;
  else {
    let rendered: string;
    try {
      rendered = typeof err === 'string' ? err : JSON.stringify(err, (_k, v: unknown) => (typeof v === 'bigint' ? v.toString() : v)) ?? String(err);
    } catch {
      rendered = String(err);
    }
    reason = rendered;
  }
  return { kind, program: named, reason };
}

export class StatusTracker {
  private current: StatusUpdate;
  private readonly rpc: StatusRpc;
  private readonly maxPolls: number;
  private readonly pollIntervalMs: number;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly lastValidBlockHeight?: number;
  private lastRpcError: unknown;

  constructor(options: StatusTrackerOptions) {
    this.rpc = options.rpc;
    this.maxPolls = options.maxPolls ?? DEFAULT_MAX_POLLS;
    this.pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    this.sleep = options.sleep ?? defaultSleep;
    this.lastValidBlockHeight = options.lastValidBlockHeight;
    this.current = {
      signature: options.signature,
      status: options.initial,
      ...(options.initialError === undefined ? {} : { error: options.initialError }),
    };
  }

  get latest(): StatusUpdate {
    return this.current;
  }

  get safeToRebuild(): boolean {
    return this.current.status === 'failed' || this.current.status === 'expired';
  }

  /** One poll. Never throws: an RPC failure is remembered and the status is left as it was. */
  async poll(): Promise<StatusUpdate> {
    if (isTerminalStatus(this.current.status)) return this.current;
    const signature = this.current.signature;

    // Read the block height BEFORE the status. If the signature is unseen AND the
    // height was already past lastValidBlockHeight, the transaction can no longer
    // land, and no read of the status that follows can be racing a landing.
    let heightPastValidity = false;
    if (this.lastValidBlockHeight !== undefined && this.rpc.getBlockHeight) {
      try {
        const height = await this.rpc.getBlockHeight('finalized');
        heightPastValidity = height > this.lastValidBlockHeight;
      } catch (error) {
        this.lastRpcError = error;
      }
    }

    let value: SignatureStatusValue | null;
    try {
      const response = await this.rpc.getSignatureStatuses([signature], { searchTransactionHistory: true });
      value = response.value[0] ?? null;
    } catch (error) {
      this.lastRpcError = error;
      return this.current;
    }

    if (value === null) {
      if (heightPastValidity) this.current = { signature, status: 'expired' };
      return this.current;
    }
    if (value.err !== null && value.err !== undefined) {
      this.current = { signature, status: 'failed', slot: value.slot, error: value.err };
      return this.current;
    }
    let next: NativeTxStatus;
    if (value.confirmationStatus === 'finalized') next = 'finalized';
    else if (value.confirmationStatus === 'confirmed') next = 'confirmed';
    else if (value.confirmationStatus === undefined && value.confirmations === null) next = 'finalized';
    else next = 'submitted'; // processed: seen, not yet confirmed
    if (RANK[next] > RANK[this.current.status]) {
      this.current = { signature, status: next, slot: value.slot };
    } else if (this.current.slot === undefined && value.slot !== undefined && next === this.current.status) {
      this.current = { ...this.current, slot: value.slot };
    }
    return this.current;
  }

  /** Yields the latest status first, then every change until a terminal status or the end of the poll window. */
  async *stream(): AsyncGenerator<StatusUpdate> {
    let last = this.current;
    yield last;
    if (isTerminalStatus(last.status)) return;
    for (let i = 0; i < this.maxPolls; i += 1) {
      if (i > 0) await this.sleep(this.pollIntervalMs);
      const next = await this.poll();
      if (next.status !== last.status) {
        last = next;
        yield next;
      }
      if (isTerminalStatus(next.status)) return;
    }
    // Window over. A transaction never seen on chain is `unknown`, whatever the node said at send time.
    if (RANK[this.current.status] <= RANK.submitted && this.current.status !== 'unknown') {
      this.current = {
        signature: this.current.signature,
        status: 'unknown',
        ...(this.lastRpcError === undefined ? {} : { error: this.lastRpcError }),
      };
      yield this.current;
    }
  }

  /**
   * Poll until the transaction reaches `until` (default finalized), ends
   * terminally, or the poll window elapses; return where it ended up.
   */
  async settle(until: 'confirmed' | 'final' = 'final'): Promise<StatusUpdate> {
    const stopAt = until === 'confirmed' ? RANK.confirmed : RANK.finalized;
    for await (const update of this.stream()) {
      if (RANK[update.status] >= stopAt) break;
    }
    return this.current;
  }
}
