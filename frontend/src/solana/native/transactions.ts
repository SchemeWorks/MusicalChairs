// Instruction builders for the native Musical Chairs program.
//
// Pure and synchronous: every builder takes already-read on-chain state (Pool,
// Ticket, CloseRecord ...) and returns a web3.js TransactionInstruction. The
// client (client.ts) does the reads and the sending. Account lists and
// writable/signer flags come from the program IDL, so they cannot drift from
// the program; only the DERIVED addresses and the variable-length remaining
// accounts of the keeper steps are decided here. The reference for those
// layouts is the LiteSVM harness (solana/tests/financial-runtime/src/
// world.rs, runtime.rs, close.rs, tests/inline_alloc.rs).
//
// Imports @solana/web3.js at module top level; load through dynamic import()
// (see accounts.ts).

import { ComputeBudgetProgram, PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js';
import type { AccountMeta } from '@solana/web3.js';
import {
  PAGE_CAPACITY,
  encodeInstructionData,
  getInstruction,
  type IdlAccountRef,
} from './codecs';
import {
  PROGRAM_ID,
  admissionAddress,
  backerAddress,
  candidateAddress,
  closeRecordAddress,
  configAddress,
  eventAuthorityAddress,
  gameVaultAddress,
  opsAddress,
  opsCashAddress,
  oraoRequestAddress,
  pageAddress,
  participantAddress,
  poolAddress,
  positionAddress,
  registryAddress,
  rngAuthorityAddress,
  snapshotAddress,
  ticketAddress,
  toPublicKey,
  userAddress,
  userRoundAddress,
  type KeyLike,
} from './accounts';
import {
  SERIES_A,
  SERIES_B,
  TICKET_PHASE,
  type Address,
  type FundingKind,
  type NativeBacker,
  type NativeCloseRecord,
  type NativeConfigVersion,
  type NativePool,
  type NativeTicket,
  type PageKindArg,
  type PlanKind,
  type SnapshotRow,
  type TicketPhaseName,
} from './types';

// ---------------------------------------------------------------------------
// Generic IDL-driven instruction builder (covers all 36 instructions)
// ---------------------------------------------------------------------------

export type AccountInputs = Record<string, KeyLike | null | undefined>;

export interface BuildIxInput {
  args?: Record<string, unknown>;
  /** Accounts by IDL name. Singletons (pool, ops, event_authority ...) default to their PDA. */
  accounts?: AccountInputs;
  /** Variable-length remaining accounts (keeper rows, inline allocation). */
  remaining?: AccountMeta[];
}

const SINGLETONS: Record<string, () => PublicKey> = {
  pool: poolAddress,
  admission: admissionAddress,
  game_vault: gameVaultAddress,
  ops: opsAddress,
  ops_cash: opsCashAddress,
  ticket: ticketAddress,
  rng_authority: rngAuthorityAddress,
  event_authority: eventAuthorityAddress,
  program: () => PROGRAM_ID,
};

/** Anchor's `None` for an optional account: the program id, read-only. */
export const NONE_PLACEHOLDER = (): AccountMeta => ({ pubkey: PROGRAM_ID, isSigner: false, isWritable: false });

function resolveAccount(ixName: string, ref: IdlAccountRef, provided: AccountInputs): AccountMeta {
  if (ref.address) {
    const fixed = new PublicKey(ref.address);
    const given = provided[ref.name];
    if (given && !toPublicKey(given).equals(fixed)) {
      throw new Error(`${ixName}: account "${ref.name}" is fixed to ${ref.address}`);
    }
    return { pubkey: fixed, isSigner: !!ref.signer, isWritable: !!ref.writable };
  }
  const given = provided[ref.name];
  if (given === null) {
    if (!ref.optional) throw new Error(`${ixName}: account "${ref.name}" is required (null given)`);
    return NONE_PLACEHOLDER();
  }
  let key: PublicKey | undefined = given === undefined ? undefined : toPublicKey(given);
  if (key === undefined) {
    const singleton = SINGLETONS[ref.name];
    if (singleton) key = singleton();
  }
  if (key === undefined) {
    if (ref.optional) return NONE_PLACEHOLDER();
    throw new Error(`${ixName}: missing account "${ref.name}"`);
  }
  return { pubkey: key, isSigner: !!ref.signer, isWritable: !!ref.writable };
}

/**
 * Build any of the program's instructions from the IDL: discriminator + Borsh
 * args, accounts in IDL order with IDL signer/writable flags, then `remaining`.
 * Unknown account names are rejected (typo guard).
 */
export function buildIx(name: string, input: BuildIxInput = {}): TransactionInstruction {
  const ix = getInstruction(name);
  const provided = input.accounts ?? {};
  const known = new Set(ix.accounts.map((a) => a.name));
  for (const key of Object.keys(provided)) {
    if (!known.has(key)) throw new Error(`${name}: unknown account "${key}" (IDL accounts: ${[...known].join(', ')})`);
  }
  const keys = ix.accounts.map((ref) => resolveAccount(name, ref, provided));
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [...keys, ...(input.remaining ?? [])],
    // A plain Uint8Array: the browser has no global Buffer, and web3.js only base58-encodes the bytes.
    data: encodeInstructionData(name, input.args ?? {}) as unknown as TransactionInstruction['data'],
  });
}

const meta = (key: KeyLike, writable: boolean): AccountMeta => ({
  pubkey: toPublicKey(key),
  isSigner: false,
  isWritable: writable,
});

// ---------------------------------------------------------------------------
// Compute budget
// ---------------------------------------------------------------------------

/**
 * Compute-unit limits per instruction. Measured maxima (docs/solana/
 * measurements-2026-10.md: discover 40k, allocate 42k, prune 57k, build 66k,
 * apply 72k, a plain payout 49k, an inline payout 83k at N=1 growing about
 * 24k per Backer) with generous headroom. A limit is a cap, not a charge, and
 * no priority fee is set, so headroom costs nothing.
 */
const CU_DEFAULT = 200_000;
const CU_BY_INSTRUCTION: Record<string, number> = {
  withdraw_simple: 400_000,
  settle_compounding: 400_000,
  prune_rows: 300_000,
  build_candidates: 400_000,
  request_round_draw: 400_000,
  apply_promotion: 300_000,
};
export const CU_INLINE_PAYOUT = 900_000;

export function computeUnitLimitFor(instructionNames: string[], inline = false): number {
  if (inline) return CU_INLINE_PAYOUT;
  const total = instructionNames.reduce((sum, n) => sum + (CU_BY_INSTRUCTION[n] ?? CU_DEFAULT), 0);
  return Math.min(1_400_000, Math.max(CU_DEFAULT, total));
}

export function computeBudgetIx(units: number): TransactionInstruction {
  return ComputeBudgetProgram.setComputeUnitLimit({ units });
}

export function assembleTransaction(params: {
  feePayer: PublicKey;
  blockhash: string;
  lastValidBlockHeight: number;
  instructions: TransactionInstruction[];
  computeUnits: number;
}): Transaction {
  const tx = new Transaction({
    feePayer: params.feePayer,
    blockhash: params.blockhash,
    lastValidBlockHeight: params.lastValidBlockHeight,
  });
  tx.add(computeBudgetIx(params.computeUnits), ...params.instructions);
  return tx;
}

// ---------------------------------------------------------------------------
// Player instructions
// ---------------------------------------------------------------------------

const PAGE = BigInt(PAGE_CAPACITY);

/** open_position: plan + gross deposit; pins the round and config it was quoted against. */
export function openPositionIx(p: { owner: KeyLike; pool: NativePool; plan: PlanKind; gross: bigint; nonce: bigint }): TransactionInstruction {
  return buildIx('open_position', {
    args: {
      plan: p.plan,
      gross: p.gross,
      clientNonce: p.nonce,
      expectedRoundId: p.pool.roundId,
      expectedConfig: p.pool.activeConfigVersion,
    },
    accounts: {
      owner: p.owner,
      user: userAddress(p.owner),
      user_round: userRoundAddress(p.pool.roundId, p.owner),
      position: positionAddress(p.owner, p.nonce),
      config: configAddress(p.pool.activeConfigVersion),
      // The page the NEXT participant of the round lands on; read only on the owner's first position of the round.
      participant_page: participantAddress(p.pool.roundId, p.pool.participantCount / PAGE),
    },
  });
}

/** fund_series_a: Series A funding (Backer series 0) appended at the registry tail. */
export function fundSeriesAIx(p: { owner: KeyLike; pool: NativePool; gross: bigint }): TransactionInstruction {
  return buildIx('fund_series_a', {
    args: { gross: p.gross, expectedRoundId: p.pool.roundId },
    accounts: {
      owner: p.owner,
      user: userAddress(p.owner),
      backer: backerAddress(SERIES_A, p.owner),
      registry: registryAddress(p.pool.registryLen / PAGE),
      config: configAddress(p.pool.activeConfigVersion),
    },
  });
}

/** Most Backers the opt-in inline allocation can carry (state.rs INLINE_THRESHOLD). */
export const INLINE_THRESHOLD = 8;

/**
 * Remaining accounts of the opt-in inline allocation inside a NON-closing
 * payout: `[Backer slot 0 .. registry_len-1, RegistryPage 0, SnapshotPage 0]`,
 * all writable, only while 1 <= registry_len <= 8. Returns null when the
 * registry is outside that range (the chunked ticket is then the only path).
 */
export function inlineAllocationAccounts(registryLen: bigint, registryKeys: Address[]): AccountMeta[] | null {
  if (registryLen < 1n || registryLen > BigInt(INLINE_THRESHOLD)) return null;
  if (registryKeys.length < Number(registryLen)) throw new Error('inline allocation needs every registry key of page 0');
  return [
    ...registryKeys.slice(0, Number(registryLen)).map((key) => meta(key, true)),
    meta(registryAddress(0), true),
    meta(snapshotAddress(0), true),
  ];
}

export type PayoutKind = 'withdraw_simple' | 'settle_compounding';

export interface PayoutParams {
  kind: PayoutKind;
  /** The signer; also the owner of `position` and of the current-round UserRound. */
  owner: KeyLike;
  pool: NativePool;
  nonce: bigint;
  /** Position.configVersion (the position's pinned rules). */
  positionConfigVersion: number;
  /** Where the net payout lands; must be a plain owner-controlled account. */
  destination: KeyLike;
  /** The next CloseRecord when it is provisioned (needed if this payout closes the round), else null. */
  closeRecord: KeyLike | null;
  minimumReceive: bigint;
  /** Optional extra contribution toward allocation rent (advanced; default 0). */
  maxWorkContribution: bigint;
  /** Inline allocation remaining accounts from inlineAllocationAccounts(), or none. */
  inlineAccounts?: AccountMeta[] | null;
}

export function payoutIx(p: PayoutParams): TransactionInstruction {
  return buildIx(p.kind, {
    args: { minimumReceive: p.minimumReceive, maxWorkContribution: p.maxWorkContribution },
    accounts: {
      owner: p.owner,
      user_round: userRoundAddress(p.pool.roundId, p.owner),
      position: positionAddress(p.owner, p.nonce),
      config: configAddress(p.positionConfigVersion),
      destination: p.destination,
      close_record: p.closeRecord,
    },
    remaining: p.inlineAccounts ?? [],
  });
}

export function closeSimplePositionIx(p: { owner: KeyLike; nonce: bigint; configVersion: number }): TransactionInstruction {
  return buildIx('close_simple_position', {
    accounts: {
      owner: p.owner,
      position: positionAddress(p.owner, p.nonce),
      config: configAddress(p.configVersion),
    },
  });
}

/**
 * claim_backer: pays everything claimable across the owner's Series A and
 * Series B rows in ONE instruction (combined claim). Pass the rows that exist;
 * a missing series is Anchor's `None`.
 */
export function claimBackerIx(p: {
  owner: KeyLike;
  destination: KeyLike;
  hasSeriesA: boolean;
  hasSeriesB: boolean;
}): TransactionInstruction {
  return buildIx('claim_backer', {
    accounts: {
      owner: p.owner,
      user: userAddress(p.owner),
      backer_a: p.hasSeriesA ? backerAddress(SERIES_A, p.owner) : null,
      backer_b: p.hasSeriesB ? backerAddress(SERIES_B, p.owner) : null,
      destination: p.destination,
    },
  });
}

// ---------------------------------------------------------------------------
// Allocation ticket keeper steps (six phases)
// ---------------------------------------------------------------------------

/** Rows a batch may carry: bounded by what is left of the captured registry. */
export function batchRows(ticket: NativeTicket, maxRows: number): number {
  if (!Number.isInteger(maxRows) || maxRows < 1 || maxRows > 255) throw new RangeError('rows must be an integer in 1..255');
  const left = ticket.capturedLen - ticket.cursor;
  if (left <= 0n) return 0;
  return Number(left < BigInt(maxRows) ? left : BigInt(maxRows));
}

/** Discover: per slot `[RegistryPage (ro), SnapshotPage (w), Backer key (ro)]`. */
export function discoverRowsIx(p: { ticket: NativeTicket; rows: number; registryKeyAt: (slot: bigint) => Address }): TransactionInstruction {
  const remaining: AccountMeta[] = [];
  for (let i = 0n; i < BigInt(p.rows); i += 1n) {
    const slot = p.ticket.cursor + i;
    remaining.push(meta(registryAddress(slot / PAGE), false), meta(snapshotAddress(slot / PAGE), true), meta(p.registryKeyAt(slot), false));
  }
  return buildIx('discover_rows', {
    args: { expectedTicketId: p.ticket.id, expectedCursor: p.ticket.cursor, rows: p.rows },
    remaining,
  });
}

/** Senior: the tracked oldest open Series A, or Anchor's `None` when the ticket captured none. */
export function applySeniorIx(ticket: NativeTicket): TransactionInstruction {
  return buildIx('apply_senior', {
    args: { expectedTicketId: ticket.id },
    accounts: { backer: ticket.aCount === 0n ? null : ticket.oldest },
  });
}

export function sealWeightsIx(ticket: NativeTicket): TransactionInstruction {
  return buildIx('seal_weights', { args: { expectedTicketId: ticket.id } });
}

/** Allocate: per slot `[SnapshotPage (ro), the Backer the snapshot row names (w)]`. */
export function allocateRowsIx(p: { ticket: NativeTicket; rows: number; snapshotRowAt: (slot: bigint) => SnapshotRow }): TransactionInstruction {
  const remaining: AccountMeta[] = [];
  for (let i = 0n; i < BigInt(p.rows); i += 1n) {
    const slot = p.ticket.cursor + i;
    remaining.push(meta(snapshotAddress(slot / PAGE), false), meta(p.snapshotRowAt(slot).key, true));
  }
  return buildIx('allocate_rows', {
    args: { expectedTicketId: p.ticket.id, expectedCursor: p.ticket.cursor, rows: p.rows },
    remaining,
  });
}

/** A Backer is closed for the registry once its remaining entitlement is at or below this (state.rs). */
export const BACKER_REMAINING_CUTOFF = 10n;

export interface PrunePlan {
  rows: number;
  remaining: AccountMeta[];
  pruned: number;
}

export interface PruneInputs {
  ticket: NativeTicket;
  registryLen: bigint;
  maxRows: number;
  /** Leave rows that only became capped by this ticket's credits for a later ticket. */
  skipNewlyCapped?: boolean;
  /** Current registry key at `slot` (before any swap-delete of this batch). */
  registryKeyAt: (slot: bigint) => Address;
  snapshotRowAt: (slot: bigint) => SnapshotRow;
  /** Live remaining entitlement (entitlement - lifetime) of a Backer. */
  liveRemaining: (backer: Address) => bigint;
}

/**
 * Prune walks DOWN from the ticket cursor, swap-deleting capped/closed rows
 * from the dense registry. Per row: `[SnapshotPage (ro), row Backer (w),
 * RegistryPage of the slot (w), RegistryPage of the last row (w), moved Backer
 * (w)]` for a row to remove, or `[SnapshotPage (ro), 4 x program id (ro)]` for
 * a row to keep. Earlier swap-deletes of the same batch are simulated, and the
 * batch is cut BEFORE a row whose moved Backer was already written earlier in
 * the batch (that would be a duplicate writable account).
 */
export function planPrune(p: PruneInputs): PrunePlan {
  const overrides = new Map<bigint, Address>();
  const keyAt = (slot: bigint): Address => overrides.get(slot) ?? p.registryKeyAt(slot);
  const written = new Set<Address>();
  const remaining: AccountMeta[] = [];
  let len = p.registryLen;
  let rows = 0;
  let pruned = 0;
  const limit = BigInt(p.maxRows) < p.ticket.cursor ? BigInt(p.maxRows) : p.ticket.cursor;
  for (let i = 0n; i < limit; i += 1n) {
    const slot = p.ticket.cursor - 1n - i;
    const row = p.snapshotRowAt(slot);
    const capturedClosed = row.remaining <= BACKER_REMAINING_CUTOFF;
    const newlyCapped = !p.skipNewlyCapped && p.liveRemaining(row.key) <= BACKER_REMAINING_CUTOFF;
    if (capturedClosed || newlyCapped) {
      const last = len - 1n;
      const moved = keyAt(last);
      if (slot !== last && written.has(moved)) break;
      remaining.push(
        meta(snapshotAddress(slot / PAGE), false),
        meta(row.key, true),
        meta(registryAddress(slot / PAGE), true),
        meta(registryAddress(last / PAGE), true),
        meta(moved, true),
      );
      written.add(row.key);
      if (slot !== last) {
        written.add(moved);
        overrides.set(slot, moved);
      }
      len -= 1n;
      pruned += 1;
    } else {
      remaining.push(meta(snapshotAddress(slot / PAGE), false), NONE_PLACEHOLDER(), NONE_PLACEHOLDER(), NONE_PLACEHOLDER(), NONE_PLACEHOLDER());
    }
    rows += 1;
  }
  return { rows, remaining, pruned };
}

export function pruneRowsIx(ticket: NativeTicket, plan: PrunePlan): TransactionInstruction {
  return buildIx('prune_rows', {
    args: { expectedTicketId: ticket.id, expectedCursor: ticket.cursor, rows: plan.rows },
    remaining: plan.remaining,
  });
}

/** Commit: the CloseRecord when this ticket allocates a closing payout's toll, else Anchor's `None`. */
export function commitTicketIx(ticket: NativeTicket): TransactionInstruction {
  return buildIx('commit_ticket', {
    args: { expectedTicketId: ticket.id },
    accounts: { close_record: ticket.closeId === null ? null : closeRecordAddress(ticket.closeId) },
  });
}

export const PHASE_NAME: Record<number, TicketPhaseName> = Object.fromEntries(
  Object.entries(TICKET_PHASE).map(([name, value]) => [value, name as TicketPhaseName]),
);

/** Which of the six typed instructions a ticket phase emits. */
export const PHASE_INSTRUCTION: Record<Exclude<TicketPhaseName, 'Idle'>, string> = {
  Discover: 'discover_rows',
  Senior: 'apply_senior',
  Seal: 'seal_weights',
  Allocate: 'allocate_rows',
  Prune: 'prune_rows',
  Commit: 'commit_ticket',
};

// ---------------------------------------------------------------------------
// Provisioning
// ---------------------------------------------------------------------------

export function provisionPageIx(p: { payer: KeyLike; page: PageKindArg; index: bigint; funding: FundingKind }): TransactionInstruction {
  return buildIx('provision_page', {
    args: { kind: p.page, index: p.index, funding: p.funding },
    accounts: {
      page: pageAddress(p.page, p.index),
      payer: p.payer,
      // Candidate pages belong to a close: its record is the lock source for LockedClose funding.
      close_record: p.page.kind === 'Candidate' ? closeRecordAddress(p.page.closeId) : null,
    },
  });
}

export function provisionCloseRecordIx(p: { payer: KeyLike; closeId: bigint; funding: FundingKind }): TransactionInstruction {
  return buildIx('provision_close_record', {
    args: { closeId: p.closeId, funding: p.funding },
    accounts: { close_record: closeRecordAddress(p.closeId), payer: p.payer },
  });
}

export function increaseProviderBudgetIx(p: { closeId: bigint; extra: bigint }): TransactionInstruction {
  return buildIx('increase_provider_budget', {
    args: { extra: p.extra },
    accounts: { close_record: closeRecordAddress(p.closeId) },
  });
}

// ---------------------------------------------------------------------------
// Closing round: candidates, draw, promotion, finalize
// ---------------------------------------------------------------------------

/** Candidate rows a batch may carry: bounded by the participants left to visit. */
export function candidateBatchRows(close: NativeCloseRecord, maxRows: number): number {
  if (!Number.isInteger(maxRows) || maxRows < 1 || maxRows > 255) throw new RangeError('rows must be an integer in 1..255');
  const left = close.participantCount - close.candidateCursor;
  if (left <= 0n) return 0;
  return Number(left < BigInt(maxRows) ? left : BigInt(maxRows));
}

/**
 * build_candidates: per participant slot `[ParticipantPage (ro), UserRound (ro), User (ro)]`.
 * Each set's candidate page is passed only when provisioned for the page the
 * set's next append lands on; otherwise Anchor's `None`. A batch whose appends
 * would run into an unprovisioned page fails PageNotProvisioned: retry with
 * fewer rows (one row always fits).
 */
export function buildCandidatesIx(p: {
  close: NativeCloseRecord;
  rows: number;
  participantAt: (slot: bigint) => Address;
}): TransactionInstruction {
  const { close } = p;
  const preferredIndex = close.preferredCount / PAGE;
  const fallbackIndex = close.fallbackCount / PAGE;
  const remaining: AccountMeta[] = [];
  for (let i = 0n; i < BigInt(p.rows); i += 1n) {
    const slot = close.candidateCursor + i;
    const owner = p.participantAt(slot);
    remaining.push(
      meta(participantAddress(close.roundId, slot / PAGE), false),
      meta(userRoundAddress(close.roundId, owner), false),
      meta(userAddress(owner), false),
    );
  }
  return buildIx('build_candidates', {
    args: { rows: p.rows },
    accounts: {
      close_record: closeRecordAddress(close.closeId),
      preferred_page: close.provisionedPreferredPages > preferredIndex ? candidateAddress(close.closeId, 0, preferredIndex) : null,
      fallback_page: close.provisionedFallbackPages > fallbackIndex ? candidateAddress(close.closeId, 1, fallbackIndex) : null,
      ops: opsAddress(),
      ops_cash: opsCashAddress(),
      owner_refund: close.ownerRefundDestination,
    },
    remaining,
  });
}

/** The set the close selected: preferred when `selectedSetTag` is 0 else fallback (close.rs `selected_set`). */
export function selectedSet(close: NativeCloseRecord): { tag: number; count: bigint; digest: Uint8Array } {
  return close.selectedSetTag === 0
    ? { tag: 0, count: close.preferredCount, digest: close.preferredDigest }
    : { tag: 1, count: close.fallbackCount, digest: close.fallbackDigest };
}

/** request_round_draw: provider tuple from the close's pinned ConfigVersion; `request` is the ORAO PDA of the protocol seed. */
export function requestRoundDrawIx(p: {
  payer: KeyLike;
  close: NativeCloseRecord;
  config: NativeConfigVersion;
  protocolSeed: Uint8Array;
}): TransactionInstruction {
  return buildIx('request_round_draw', {
    accounts: {
      payer: p.payer,
      close_record: closeRecordAddress(p.close.closeId),
      config: configAddress(p.close.configVersion),
      provider_program: p.config.providerProgram,
      network_state: p.config.providerNetworkState,
      treasury: p.config.providerTreasury,
      client: p.config.providerClient,
      request: oraoRequestAddress(p.config.providerClient, p.protocolSeed, p.config.providerProgram),
    },
  });
}

export function captureRoundDrawIx(p: { close: NativeCloseRecord; config: NativeConfigVersion }): TransactionInstruction {
  return buildIx('capture_round_draw', {
    accounts: {
      close_record: closeRecordAddress(p.close.closeId),
      request: p.close.requestAccount,
      client: p.config.providerClient,
    },
  });
}

/** The winner-side accounts of apply_promotion; all null when the captured output maps to no ordinal. */
export interface ApplyKeys {
  candidatePage: KeyLike | null;
  winner: KeyLike | null;
  winnerUser: KeyLike | null;
  winnerUserRound: KeyLike | null;
  winnerBackerB: KeyLike | null;
  registryTail: KeyLike | null;
}

export const NO_APPLY_KEYS: ApplyKeys = {
  candidatePage: null,
  winner: null,
  winnerUser: null,
  winnerUserRound: null,
  winnerBackerB: null,
  registryTail: null,
};

/** Derive the winner's accounts once the mapped ordinal's winner key is known (read from the candidate page). */
export function applyKeysFor(p: { close: NativeCloseRecord; pool: NativePool; ordinal: bigint; winner: Address }): ApplyKeys {
  const { tag } = selectedSet(p.close);
  return {
    candidatePage: candidateAddress(p.close.closeId, tag, p.ordinal / PAGE),
    winner: p.winner,
    winnerUser: userAddress(p.winner),
    winnerUserRound: userRoundAddress(p.close.roundId, p.winner),
    winnerBackerB: backerAddress(SERIES_B, p.winner),
    registryTail: registryAddress(p.pool.registryLen / PAGE),
  };
}

export function applyPromotionIx(p: { payer: KeyLike; close: NativeCloseRecord; keys: ApplyKeys }): TransactionInstruction {
  return buildIx('apply_promotion', {
    accounts: {
      payer: p.payer,
      close_record: closeRecordAddress(p.close.closeId),
      config: configAddress(p.close.configVersion),
      candidate_page: p.keys.candidatePage,
      winner: p.keys.winner,
      winner_user: p.keys.winnerUser,
      winner_user_round: p.keys.winnerUserRound,
      winner_backer_b: p.keys.winnerBackerB,
      registry_tail: p.keys.registryTail,
      owner_refund: p.close.ownerRefundDestination,
    },
  });
}

export function expirePromotionIx(close: NativeCloseRecord): TransactionInstruction {
  return buildIx('expire_promotion', {
    accounts: { close_record: closeRecordAddress(close.closeId), owner_refund: close.ownerRefundDestination },
  });
}

export function finalizeRoundIx(closeId: bigint): TransactionInstruction {
  return buildIx('finalize_round', { accounts: { close_record: closeRecordAddress(closeId) } });
}

export function deliverOwnerRefundIx(close: NativeCloseRecord): TransactionInstruction {
  return buildIx('deliver_owner_refund', {
    accounts: { close_record: closeRecordAddress(close.closeId), owner_refund: close.ownerRefundDestination },
  });
}

export function observeRequestFulfilledIx(p: { close: NativeCloseRecord; config: NativeConfigVersion }): TransactionInstruction {
  return buildIx('observe_request_fulfilled', {
    accounts: {
      close_record: closeRecordAddress(p.close.closeId),
      request: p.close.requestAccount,
      client: p.config.providerClient,
    },
  });
}

/** Anything that reads a Backer's live remaining entitlement. */
export const liveRemainingOf = (backer: Pick<NativeBacker, 'entitlement' | 'lifetime'>): bigint =>
  backer.entitlement > backer.lifetime ? backer.entitlement - backer.lifetime : 0n;
