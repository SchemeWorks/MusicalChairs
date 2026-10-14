// NativeClient: the one object the UI and keepers use to talk to the native
// Musical Chairs program.
//
//   const client = createNativeClient({ connection, wallet, sessionPubkey });
//   const { signature, nonce, status$ } = await client.openPosition({ plan: 'Simple', gross });
//
// Guarantees (plan M11):
//  * Wallet binding: before ANY network I/O every state-changing method checks
//    that the connected wallet's pubkey equals the SIWS session pubkey
//    (WalletMismatchError otherwise). Nothing is built, signed or sent.
//  * Signed first, sent second: the signature is known from the signed bytes
//    before broadcast. If the send dies ambiguously (RPC timeout) the result
//    keeps `{ signature, nonce }` with status `unknown`; `retry()` re-polls the
//    SAME signature (and re-broadcasts the IDENTICAL bytes) and never rebuilds
//    or re-signs. open_position's `client_nonce` is the idempotency key: a
//    second open with the same nonce would be `Replay` on chain, so a new
//    transaction is only safe once `safeToRebuild` (failed / blockhash expired).
//  * A preflight rejection (simulation failed) is NOT unknown: nothing was
//    broadcast, it throws NativeSendRejectedError with the decoded program error.
//  * Status comes from polling getSignatureStatuses over https (no wss).
//
// Imports @solana/web3.js statically: load this module with dynamic import().

import { Connection, PublicKey } from '@solana/web3.js';
import type { Commitment, Transaction, TransactionInstruction } from '@solana/web3.js';
import bs58 from 'bs58';
import { SOLANA_RPC_ENDPOINT } from '../rpc';
import {
  PROGRAM_ID,
  closeRecordAddress,
  createReaders,
  pageOf,
  type NativeReader,
} from './accounts';
import { PAGE_CAPACITY } from './codecs';
import { mapOutput, protocolSeed } from './sampling';
import { StatusTracker, describeTransactionError, type StatusRpc } from './status';
import {
  INLINE_THRESHOLD,
  NO_APPLY_KEYS,
  PHASE_INSTRUCTION,
  PHASE_NAME,
  allocateRowsIx,
  applyKeysFor,
  applyPromotionIx,
  applySeniorIx,
  assembleTransaction,
  batchRows,
  buildCandidatesIx,
  candidateBatchRows,
  captureRoundDrawIx,
  claimBackerIx,
  closeSimplePositionIx,
  commitTicketIx,
  computeUnitLimitFor,
  discoverRowsIx,
  expirePromotionIx,
  finalizeRoundIx,
  fundSeriesAIx,
  increaseProviderBudgetIx,
  inlineAllocationAccounts,
  liveRemainingOf,
  openPositionIx,
  payoutIx,
  planPrune,
  provisionCloseRecordIx,
  provisionPageIx,
  pruneRowsIx,
  requestRoundDrawIx,
  sealWeightsIx,
  selectedSet,
  type ApplyKeys,
  type PayoutKind,
} from './transactions';
import {
  DRAW_STATE,
  type Address,
  type BackerSeries,
  type FundingKind,
  type NativeBacker,
  type NativeCloseRecord,
  type NativeConfigVersion,
  type NativePool,
  type NativePosition,
  type NativeTicket,
  type NativeTxResult,
  type PageKindArg,
  type PlanKind,
  type StatusUpdate,
  type TicketPhaseName,
} from './types';
import { assertWalletMatchesSession, WalletMismatchError, type NativeWallet } from './wallet';

export { WalletMismatchError };

export type NativeConnection = Pick<
  Connection,
  | 'getAccountInfo'
  | 'getMultipleAccountsInfo'
  | 'getProgramAccounts'
  | 'getLatestBlockhash'
  | 'sendRawTransaction'
  | 'getSignatureStatuses'
  | 'getBlockHeight'
>;

export interface NativeClientConfig {
  connection: NativeConnection;
  wallet: NativeWallet;
  /** The SIWS session's Solana pubkey (useWallet().solanaPubkey). */
  sessionPubkey: string | null;
  commitment?: Commitment;
  /** Status polling: delay between polls, polls per window, injected sleeper (tests). */
  pollIntervalMs?: number;
  maxPolls?: number;
  sleep?: (ms: number) => Promise<void>;
  /** Pre-broadcast blockhash fetch timeout (never applied to the wallet or the broadcast). */
  blockhashTimeoutMs?: number;
  /** Source of open_position client nonces (default: random u64). */
  randomNonce?: () => bigint;
}

export class NativeSendRejectedError extends Error {
  readonly programError: { code: number; name: string } | null;
  constructor(message: string, readonly cause: unknown) {
    super(message);
    this.name = 'NativeSendRejectedError';
    this.programError = describeTransactionError(cause);
  }
}

/** The program is not initialized, or the account a step needs does not exist (yet). */
export class NativeStateError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'NativeStateError';
  }
}

export interface AdvancePhasePlan {
  phase: TicketPhaseName;
  instructionName: string;
  rows: number;
  instruction: TransactionInstruction;
}

export interface PayoutRequest {
  /** The Position's client nonce. */
  nonce: bigint;
  /** Slippage guard: the program refuses to pay less than this net amount. */
  minimumReceive: bigint;
  /** Advanced: extra lamports the owner will add toward allocation rent (default 0). */
  maxWorkContribution?: bigint;
  /** Net payout destination (default: the owner's wallet). */
  destination?: Address;
  /** Opt in to allocating the toll inline when the registry holds 1..8 Backers (default off). */
  inline?: boolean;
}

export interface NativeClient {
  readonly sessionPubkey: string | null;
  // --- player ---
  openPosition(p: { plan: PlanKind; gross: bigint; nonce?: bigint }): Promise<NativeTxResult>;
  fundSeriesA(p: { gross: bigint }): Promise<NativeTxResult>;
  withdrawSimple(p: PayoutRequest): Promise<NativeTxResult>;
  settleCompounding(p: PayoutRequest): Promise<NativeTxResult>;
  closeSimplePosition(p: { nonce: bigint }): Promise<NativeTxResult>;
  claimBacker(p: { destination?: Address }): Promise<NativeTxResult>;
  // --- allocation ticket (permissionless keeper steps) ---
  discoverRows(p: { rows?: number }): Promise<NativeTxResult>;
  applySenior(): Promise<NativeTxResult>;
  sealWeights(): Promise<NativeTxResult>;
  allocateRows(p: { rows?: number }): Promise<NativeTxResult>;
  pruneRows(p: { rows?: number; skipNewlyCapped?: boolean }): Promise<NativeTxResult>;
  commitTicket(): Promise<NativeTxResult>;
  /** Read Ticket.phase and send exactly one of the six typed instructions. Client-side orchestration, not an on-chain ABI. */
  advancePhase(p: { rows?: number; skipNewlyCapped?: boolean }): Promise<NativeTxResult & { phase: TicketPhaseName; rows: number }>;
  /** The same decision without sending (inspection, tests, keepers that batch). */
  buildAdvancePhase(p: { rows?: number; skipNewlyCapped?: boolean }): Promise<AdvancePhasePlan>;
  // --- provisioning ---
  provisionPage(p: { page: PageKindArg; index: bigint; funding: FundingKind }): Promise<NativeTxResult>;
  provisionCloseRecord(p: { closeId: bigint; funding: FundingKind }): Promise<NativeTxResult>;
  increaseProviderBudget(p: { closeId: bigint; extra: bigint }): Promise<NativeTxResult>;
  // --- closing round ---
  buildCandidates(p: { closeId: bigint; rows?: number }): Promise<NativeTxResult>;
  requestDraw(p: { closeId: bigint }): Promise<NativeTxResult>;
  captureDraw(p: { closeId: bigint }): Promise<NativeTxResult>;
  applyPromotion(p: { closeId: bigint }): Promise<NativeTxResult>;
  expirePromotion(p: { closeId: bigint }): Promise<NativeTxResult>;
  finalizeRound(p: { closeId: bigint }): Promise<NativeTxResult>;
  /**
   * ONE transaction: expire_promotion (when the promotion is still Pending),
   * finalize_round, then open_position into the next round. The open's
   * expected round is the close's round + 1, so it can only succeed after
   * finalize in the same transaction (the host model's `expire_finalize_open`).
   */
  composeExpireFinalizeOpen(p: { closeId: bigint; plan: PlanKind; gross: bigint; nonce?: bigint; includeExpire?: boolean }): Promise<NativeTxResult>;
  // --- reads ---
  readPool(): Promise<NativePool | null>;
  readPosition(owner: Address, nonce: bigint): Promise<NativePosition | null>;
  readBacker(owner: Address, series: BackerSeries): Promise<NativeBacker | null>;
  readCloseRecord(closeId: bigint): Promise<NativeCloseRecord | null>;
  readTicket(): Promise<NativeTicket | null>;
  readConfig(version: number): Promise<NativeConfigVersion | null>;
  listPositions(owner: Address): Promise<NativePosition[]>;
}

// ---------------------------------------------------------------------------

const BLOCKHASH_TIMEOUT_MS = 20_000;
const DEFAULT_ROWS = 8;

function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function randomU64(): bigint {
  const words = new Uint32Array(2);
  globalThis.crypto.getRandomValues(words);
  return (BigInt(words[0]) << 32n) | BigInt(words[1]);
}

/** A preflight (simulation) rejection means nothing was broadcast; anything else is ambiguous. */
function classifySendError(error: unknown): 'rejected' | 'unknown' {
  const name = (error as { name?: string } | null)?.name;
  const message = String((error as { message?: string } | null)?.message ?? error);
  if (name === 'SendTransactionError') return 'rejected';
  if (/simulation failed/i.test(message)) return 'rejected';
  return 'unknown';
}

const requirePositive = (value: bigint, label: string): bigint => {
  if (typeof value !== 'bigint' || value <= 0n) throw new RangeError(`${label} must be a positive bigint`);
  return value;
};

export function createNativeClient(config: NativeClientConfig): NativeClient {
  const { connection, wallet, sessionPubkey } = config;
  const commitment: Commitment = config.commitment ?? 'confirmed';
  const readers: NativeReader = createReaders(connection, commitment);
  const nextNonce = config.randomNonce ?? randomU64;

  /** First thing every state-changing method does: no wallet match, no I/O. */
  const owner = (): string => assertWalletMatchesSession(wallet, sessionPubkey);

  async function send(instructions: TransactionInstruction[], opts: { nonce?: bigint | null; computeUnits: number }): Promise<NativeTxResult> {
    const feePayer = new PublicKey(owner());

    // Pre-broadcast: a timeout here is safe (nothing is signed or sent yet).
    const { blockhash, lastValidBlockHeight } = await withTimeout(
      connection.getLatestBlockhash(commitment),
      config.blockhashTimeoutMs ?? BLOCKHASH_TIMEOUT_MS,
      'Could not reach Solana to prepare the transaction (network slow or RPC unavailable). Try again in a moment.',
    );
    const tx: Transaction = assembleTransaction({ feePayer, blockhash, lastValidBlockHeight, instructions, computeUnits: opts.computeUnits });

    // The wallet prompt: never time this out, and never sign twice.
    const signed = await wallet.signTransaction(tx);
    // The session wallet must still be the one that signed (the user can switch accounts inside the extension mid-prompt).
    owner();
    if (!signed.signature || !signed.verifySignatures()) {
      throw new WalletMismatchError('The wallet did not return a valid signature from your session account — reconnect your wallet and retry.');
    }
    const raw = signed.serialize();
    const signature = bs58.encode(signed.signature);

    let initial: 'submitted' | 'unknown' = 'submitted';
    let initialError: unknown;
    try {
      await connection.sendRawTransaction(raw, { skipPreflight: false, preflightCommitment: commitment });
    } catch (error) {
      if (classifySendError(error) === 'rejected') {
        throw new NativeSendRejectedError(`Transaction rejected before broadcast: ${(error as Error).message}`, error);
      }
      // Ambiguous: the RPC may or may not have the transaction. Keep the signature and poll it.
      initial = 'unknown';
      initialError = error;
    }

    const tracker = new StatusTracker({
      rpc: connection as unknown as StatusRpc,
      signature,
      initial,
      initialError,
      sleep: config.sleep,
      pollIntervalMs: config.pollIntervalMs,
      maxPolls: config.maxPolls,
      lastValidBlockHeight,
    });

    const result: NativeTxResult = {
      signature,
      nonce: opts.nonce ?? null,
      get status() {
        return tracker.latest.status;
      },
      status$: { [Symbol.asyncIterator]: () => tracker.stream() },
      async retry(): Promise<StatusUpdate> {
        const first = await tracker.poll();
        if (first.status === 'confirmed' || first.status === 'finalized' || first.status === 'failed' || first.status === 'expired') {
          return first;
        }
        // Not visible yet: re-broadcast the IDENTICAL signed bytes (same signature, so the cluster dedups) and keep polling.
        try {
          await connection.sendRawTransaction(raw, { skipPreflight: true, preflightCommitment: commitment });
        } catch {
          // Still ambiguous; the poll below decides.
        }
        return tracker.settle('confirmed');
      },
      get safeToRebuild() {
        return tracker.safeToRebuild;
      },
    };
    return result;
  }

  const stateError = (what: string) => new NativeStateError(what);

  async function requirePool(): Promise<NativePool> {
    const pool = await readers.readPool();
    if (!pool) throw stateError('The pool account does not exist');
    return pool;
  }
  async function requireTicket(): Promise<NativeTicket> {
    const ticket = await readers.readTicket();
    if (!ticket) throw stateError('The allocation ticket account does not exist');
    return ticket;
  }
  async function requireClose(closeId: bigint): Promise<NativeCloseRecord> {
    const close = await readers.readCloseRecord(closeId);
    if (!close) throw stateError(`Close record ${closeId} does not exist`);
    return close;
  }
  async function requireConfig(version: number): Promise<NativeConfigVersion> {
    const found = await readers.readConfig(version);
    if (!found) throw stateError(`Config version ${version} does not exist`);
    return found;
  }

  const run = async (build: () => Promise<{ instructions: TransactionInstruction[]; names: string[]; nonce?: bigint | null; inline?: boolean }>) => {
    owner();
    const plan = await build();
    return send(plan.instructions, { nonce: plan.nonce ?? null, computeUnits: computeUnitLimitFor(plan.names, plan.inline) });
  };

  async function registryKeyReader(slots: bigint[]): Promise<(slot: bigint) => Address> {
    const pages = await readers.readRegistryPages(slots.map(pageOf));
    return (slot) => {
      const page = pages.get(pageOf(slot));
      if (!page) throw stateError(`Registry page ${pageOf(slot)} is not provisioned`);
      return page.keys[Number(slot % BigInt(PAGE_CAPACITY))];
    };
  }
  async function snapshotRowReader(slots: bigint[]) {
    const pages = await readers.readSnapshotPages(slots.map(pageOf));
    return (slot: bigint) => {
      const page = pages.get(pageOf(slot));
      if (!page) throw stateError(`Snapshot page ${pageOf(slot)} is not provisioned`);
      return page.rows[Number(slot % BigInt(PAGE_CAPACITY))];
    };
  }
  const range = (from: bigint, count: number): bigint[] => Array.from({ length: count }, (_, i) => from + BigInt(i));

  async function payout(kind: PayoutKind, p: PayoutRequest) {
    return run(async () => {
      const ownerKey = owner();
      const [pool, position] = await Promise.all([requirePool(), readers.readPosition(ownerKey, p.nonce)]);
      if (!position) throw stateError(`Position ${p.nonce} does not exist for this wallet`);
      // The next CloseRecord, when already provisioned, so a payout that closes the round can bind it.
      const nextClose = closeRecordAddress(pool.nextCloseId);
      const closeRecord = (await readers.isProvisioned(nextClose)) ? nextClose : null;
      let inlineAccounts = null;
      if (p.inline && pool.registryLen >= 1n && pool.registryLen <= BigInt(INLINE_THRESHOLD)) {
        // Page 0 holds slots 0..7: the Backer PDAs in registry order.
        const keyAt = await registryKeyReader([0n]);
        inlineAccounts = inlineAllocationAccounts(pool.registryLen, range(0n, Number(pool.registryLen)).map(keyAt));
      }
      const instruction = payoutIx({
        kind,
        owner: ownerKey,
        pool,
        nonce: p.nonce,
        positionConfigVersion: position.configVersion,
        destination: p.destination ?? ownerKey,
        closeRecord,
        minimumReceive: p.minimumReceive,
        maxWorkContribution: p.maxWorkContribution ?? 0n,
        inlineAccounts,
      });
      return { instructions: [instruction], names: [kind], inline: inlineAccounts !== null };
    });
  }

  async function buildAdvance(p: { rows?: number; skipNewlyCapped?: boolean }): Promise<AdvancePhasePlan> {
    const ticket = await requireTicket();
    const phase = PHASE_NAME[ticket.phase];
    if (!phase) throw stateError(`Unknown allocation phase ${ticket.phase}`);
    if (phase === 'Idle') throw stateError('No allocation ticket is active (phase Idle)');
    const maxRows = p.rows ?? DEFAULT_ROWS;
    const instructionName = PHASE_INSTRUCTION[phase];
    switch (phase) {
      case 'Discover': {
        const rows = batchRows(ticket, maxRows);
        const keyAt = await registryKeyReader(range(ticket.cursor, rows));
        return { phase, instructionName, rows, instruction: discoverRowsIx({ ticket, rows, registryKeyAt: keyAt }) };
      }
      case 'Senior':
        return { phase, instructionName, rows: 0, instruction: applySeniorIx(ticket) };
      case 'Seal':
        return { phase, instructionName, rows: 0, instruction: sealWeightsIx(ticket) };
      case 'Allocate': {
        const rows = batchRows(ticket, maxRows);
        const rowAt = await snapshotRowReader(range(ticket.cursor, rows));
        return { phase, instructionName, rows, instruction: allocateRowsIx({ ticket, rows, snapshotRowAt: rowAt }) };
      }
      case 'Prune': {
        const pool = await requirePool();
        const limit = Math.min(maxRows, Number(ticket.cursor));
        const slots = range(ticket.cursor - BigInt(limit), limit);
        const rowAt = await snapshotRowReader(slots);
        // The registry slots touched by swap-deletes: the batch's own slots plus the tail of the registry.
        const tail = range(pool.registryLen > BigInt(limit + 1) ? pool.registryLen - BigInt(limit + 1) : 0n, Math.min(limit + 1, Number(pool.registryLen)));
        const keyAt = await registryKeyReader([...slots, ...tail]);
        const backers = new Map<Address, NativeBacker | null>();
        await Promise.all(
          slots.map(async (slot) => {
            const key = rowAt(slot).key;
            if (!backers.has(key)) backers.set(key, await readers.readBackerAt(key));
          }),
        );
        const plan = planPrune({
          ticket,
          registryLen: pool.registryLen,
          maxRows,
          skipNewlyCapped: p.skipNewlyCapped,
          registryKeyAt: keyAt,
          snapshotRowAt: rowAt,
          liveRemaining: (key) => {
            const backer = backers.get(key);
            if (!backer) throw stateError(`Backer account ${key} named by the snapshot does not exist`);
            return liveRemainingOf(backer);
          },
        });
        return { phase, instructionName, rows: plan.rows, instruction: pruneRowsIx(ticket, plan) };
      }
      case 'Commit':
        return { phase, instructionName, rows: 0, instruction: commitTicketIx(ticket) };
      default:
        throw stateError(`Unhandled allocation phase ${String(phase)}`);
    }
  }

  const advanceByPhase = (expected: TicketPhaseName, p: { rows?: number; skipNewlyCapped?: boolean }) =>
    run(async () => {
      const plan = await buildAdvance(p);
      if (plan.phase !== expected) throw stateError(`The ticket is in phase ${plan.phase}, not ${expected}`);
      return { instructions: [plan.instruction], names: [plan.instructionName] };
    });

  const closeStep =
    (names: string[], build: (close: NativeCloseRecord) => Promise<TransactionInstruction[]> | TransactionInstruction[]) =>
    (p: { closeId: bigint }) =>
      run(async () => {
        const close = await requireClose(p.closeId);
        return { instructions: await build(close), names };
      });

  async function openInstruction(pool: NativePool, ownerKey: string, plan: PlanKind, gross: bigint, nonce: bigint) {
    return openPositionIx({ owner: ownerKey, pool, plan, gross, nonce });
  }

  return {
    sessionPubkey,

    // --- player ---
    openPosition: (p) =>
      run(async () => {
        const ownerKey = owner();
        requirePositive(p.gross, 'gross');
        const nonce = p.nonce ?? nextNonce();
        const pool = await requirePool();
        return { instructions: [await openInstruction(pool, ownerKey, p.plan, p.gross, nonce)], names: ['open_position'], nonce };
      }),

    fundSeriesA: (p) =>
      run(async () => {
        const ownerKey = owner();
        requirePositive(p.gross, 'gross');
        const pool = await requirePool();
        return { instructions: [fundSeriesAIx({ owner: ownerKey, pool, gross: p.gross })], names: ['fund_series_a'] };
      }),

    withdrawSimple: (p) => payout('withdraw_simple', p),
    settleCompounding: (p) => payout('settle_compounding', p),

    closeSimplePosition: (p) =>
      run(async () => {
        const ownerKey = owner();
        const position = await readers.readPosition(ownerKey, p.nonce);
        if (!position) throw stateError(`Position ${p.nonce} does not exist for this wallet`);
        return {
          instructions: [closeSimplePositionIx({ owner: ownerKey, nonce: p.nonce, configVersion: position.configVersion })],
          names: ['close_simple_position'],
        };
      }),

    claimBacker: (p) =>
      run(async () => {
        const ownerKey = owner();
        const [a, b] = await Promise.all([readers.readBacker(ownerKey, 0), readers.readBacker(ownerKey, 1)]);
        if (!a && !b) throw stateError('This wallet has no Series A or Series B backer position');
        return {
          instructions: [claimBackerIx({ owner: ownerKey, destination: p.destination ?? ownerKey, hasSeriesA: !!a, hasSeriesB: !!b })],
          names: ['claim_backer'],
        };
      }),

    // --- allocation ticket ---
    discoverRows: (p) => advanceByPhase('Discover', p),
    applySenior: () => advanceByPhase('Senior', {}),
    sealWeights: () => advanceByPhase('Seal', {}),
    allocateRows: (p) => advanceByPhase('Allocate', p),
    pruneRows: (p) => advanceByPhase('Prune', p),
    commitTicket: () => advanceByPhase('Commit', {}),

    async advancePhase(p) {
      owner();
      const plan = await buildAdvance(p);
      const result = await send([plan.instruction], { computeUnits: computeUnitLimitFor([plan.instructionName]) });
      return Object.assign(result, { phase: plan.phase, rows: plan.rows });
    },
    async buildAdvancePhase(p) {
      return buildAdvance(p);
    },

    // --- provisioning ---
    provisionPage: (p) =>
      run(async () => ({
        instructions: [provisionPageIx({ payer: owner(), page: p.page, index: p.index, funding: p.funding })],
        names: ['provision_page'],
      })),
    provisionCloseRecord: (p) =>
      run(async () => ({
        instructions: [provisionCloseRecordIx({ payer: owner(), closeId: p.closeId, funding: p.funding })],
        names: ['provision_close_record'],
      })),
    increaseProviderBudget: (p) =>
      run(async () => {
        requirePositive(p.extra, 'extra');
        return { instructions: [increaseProviderBudgetIx({ closeId: p.closeId, extra: p.extra })], names: ['increase_provider_budget'] };
      }),

    // --- closing round ---
    buildCandidates: (p) =>
      run(async () => {
        const close = await requireClose(p.closeId);
        const rows = candidateBatchRows(close, p.rows ?? DEFAULT_ROWS);
        if (rows === 0) throw stateError('Every participant has already been visited for this close');
        const pages = await readers.readParticipantPages(close.roundId, range(close.candidateCursor, rows).map(pageOf));
        const participantAt = (slot: bigint): Address => {
          const page = pages.get(pageOf(slot));
          if (!page) throw stateError(`Participant page ${pageOf(slot)} of round ${close.roundId} is not provisioned`);
          return page.rows[Number(slot % BigInt(PAGE_CAPACITY))];
        };
        return { instructions: [buildCandidatesIx({ close, rows, participantAt })], names: ['build_candidates'] };
      }),

    requestDraw: closeStep(['request_round_draw'], async (close) => {
      if (!close.sealed) throw stateError('Candidates are not sealed yet');
      const [pool, cfg] = await Promise.all([requirePool(), requireConfig(close.configVersion)]);
      const set = selectedSet(close);
      const seed = await protocolSeed({
        clusterDomain: pool.clusterDomain,
        programId: PROGRAM_ID,
        roundId: close.roundId,
        closeId: close.closeId,
        setTag: set.tag,
        selectedCount: set.count,
        sealedDigest: set.digest,
      });
      return [requestRoundDrawIx({ payer: owner(), close, config: cfg, protocolSeed: seed })];
    }),

    captureDraw: closeStep(['capture_round_draw'], async (close) => [
      captureRoundDrawIx({ close, config: await requireConfig(close.configVersion) }),
    ]),

    applyPromotion: closeStep(['apply_promotion'], async (close) => {
      let keys: ApplyKeys = NO_APPLY_KEYS;
      if (close.drawState === DRAW_STATE.Captured) {
        const set = selectedSet(close);
        const mapped = await mapOutput(close.randomness, close.roundId, set.count);
        if (mapped) {
          const [pool, page] = await Promise.all([requirePool(), readers.readCandidatePage(close.closeId, set.tag, mapped.ordinal / BigInt(PAGE_CAPACITY))]);
          if (!page) throw stateError('The winning candidate page is not provisioned');
          keys = applyKeysFor({ close, pool, ordinal: mapped.ordinal, winner: page.rows[Number(mapped.ordinal % BigInt(PAGE_CAPACITY))] });
        }
      }
      return [applyPromotionIx({ payer: owner(), close, keys })];
    }),

    expirePromotion: closeStep(['expire_promotion'], (close) => [expirePromotionIx(close)]),
    finalizeRound: closeStep(['finalize_round'], (close) => [finalizeRoundIx(close.closeId)]),

    composeExpireFinalizeOpen: (p) =>
      run(async () => {
        const ownerKey = owner();
        requirePositive(p.gross, 'gross');
        const [pool, close] = await Promise.all([requirePool(), requireClose(p.closeId)]);
        if (close.finalized) throw stateError(`Close ${p.closeId} is already finalized — open normally`);
        if (pool.currentCloseId !== p.closeId) throw stateError(`Close ${p.closeId} is not the close in progress`);
        const includeExpire = p.includeExpire ?? close.promotion.kind === 'Pending';
        const nonce = p.nonce ?? nextNonce();
        // After finalize the pool is in round + 1 with an empty participant list; the open is built against that.
        const nextPool: NativePool = { ...pool, roundId: close.roundId + 1n, participantCount: 0n };
        const instructions = [
          ...(includeExpire ? [expirePromotionIx(close)] : []),
          finalizeRoundIx(p.closeId),
          await openInstruction(nextPool, ownerKey, p.plan, p.gross, nonce),
        ];
        const names = [...(includeExpire ? ['expire_promotion'] : []), 'finalize_round', 'open_position'];
        return { instructions, names, nonce };
      }),

    // --- reads ---
    readPool: () => readers.readPool(),
    readPosition: (ownerAddress, nonce) => readers.readPosition(ownerAddress, nonce),
    readBacker: (ownerAddress, series) => readers.readBacker(ownerAddress, series),
    readCloseRecord: (closeId) => readers.readCloseRecord(closeId),
    readTicket: () => readers.readTicket(),
    readConfig: (version) => readers.readConfig(version),
    listPositions: (ownerAddress) => readers.listPositions(ownerAddress),
  };
}

/** A connection to the configured Solana RPC (SOLANA_RPC_ENDPOINT; its host must be in the CSP connect-src). */
export function createDefaultConnection(): NativeConnection {
  return new Connection(SOLANA_RPC_ENDPOINT, 'confirmed');
}
