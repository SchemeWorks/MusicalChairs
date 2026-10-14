// Decoded account shapes, status types and result handles for the native Solana
// (Musical Chairs Anchor program) adapter.
//
// Pure types: no runtime imports, so any module (including the main bundle) can
// import them without pulling @solana/web3.js. Field names are the IDL's
// snake_case names converted to camelCase by codecs.ts. Addresses are base58
// strings; u8/u16/u32 are `number`; u64/u128/i64 are `bigint`.

export type Address = string;

/** Plan kinds as the open_position instruction takes them (`PlanKindArg`). */
export type PlanKind = 'Simple' | 'Compound15' | 'Compound30';

/** `Position.kind` (u8) mirrors `PlanKindArg::discriminant`. */
export const POSITION_KIND_SIMPLE = 0;
export const POSITION_KIND_COMPOUND_15 = 1;
export const POSITION_KIND_COMPOUND_30 = 2;

/** Backer series: `Backer.series` and the `[backer, series, owner]` seed byte. */
export const SERIES_A = 0;
export const SERIES_B = 1;
export type BackerSeries = typeof SERIES_A | typeof SERIES_B;

/** Who pays a page's rent (`Funding`): Free 0, Caller 1, LockedClose 2. */
export type FundingKind = 'Free' | 'Caller' | 'LockedClose';

/** Which shared page `provision_page` / `close_shared_account` address (`PageKind`). */
export type PageKindArg =
  | { kind: 'Registry' }
  | { kind: 'Snapshot' }
  | { kind: 'Participant'; round: bigint }
  | { kind: 'Candidate'; closeId: bigint; setTag: number };

/** Allocation ticket phases (`state.rs`). */
export const TICKET_PHASE = {
  Discover: 0,
  Senior: 1,
  Seal: 2,
  Allocate: 3,
  Prune: 4,
  Commit: 5,
  Idle: 6,
} as const;
export type TicketPhaseName = keyof typeof TICKET_PHASE;

/** `CloseRecord.draw_state`. */
export const DRAW_STATE = {
  NotRequested: 0,
  Requested: 1,
  Captured: 2,
  IgnoredLate: 3,
} as const;

/** Candidate set tags: preferred 0, fallback 1. */
export const SET_PREFERRED = 0;
export const SET_FALLBACK = 1;

export interface RentComponents {
  ops: bigint;
  owner: bigint;
  ownerRefund: Address;
}

export interface ReserveSchedule {
  perCandidatePageRent: bigint;
  perRegistryPageRent: bigint;
  bAccountRent: bigint;
  pendingRequestRent: bigint;
  providerFee: bigint;
  headroomBps: number;
}

export interface SpentLedger {
  candidateRentS: bigint;
  candidateRentO: bigint;
  registryRentS: bigint;
  registryRentO: bigint;
  bRentS: bigint;
  bRentO: bigint;
  providerFeeS: bigint;
  providerFeeO: bigint;
  requestRentS: bigint;
  requestRentO: bigint;
}

export type Promotion =
  | { kind: 'Pending' }
  | { kind: 'Applied'; winner: Address; entitlement: bigint }
  | { kind: 'Skipped'; reason: number; stage: number };

export interface NativePool {
  bump: number;
  clusterDomain: Uint8Array;
  roundId: bigint;
  activeConfigVersion: number;
  pot: bigint;
  seed: bigint;
  management: bigint;
  pendingToll: bigint;
  fundedClaims: bigint;
  excess: bigint;
  protectedRent: bigint;
  registryLen: bigint;
  registryVersion: bigint;
  nextSequence: bigint;
  nextTicket: bigint;
  activeTicket: bigint;
  finalizedTicket: bigint;
  snapshotPages: bigint;
  registryPages: bigint;
  participantCount: bigint;
  participantPages: bigint;
  nextCloseId: bigint;
  currentCloseId: bigint | null;
  totalGrossDeposits: bigint;
  totalNetPayouts: bigint;
  governance: Address;
  guardianVault: Address;
  sponsor: Address;
  managementRecipient: Address;
}

export interface NativeAdmission {
  bump: number;
  guardianVault: Address;
  paused: boolean;
  changedAt: bigint;
}

export interface NativePosition {
  bump: number;
  owner: Address;
  nonce: bigint;
  roundId: bigint;
  configVersion: number;
  kind: number;
  principal: bigint;
  openedAt: bigint;
  maturityAt: bigint;
  lastAccruedAt: bigint;
  remainder: bigint;
  accruedGross: bigint;
  paidGross: bigint;
  netPayout: bigint;
  claimed: boolean;
}

export interface NativeBacker {
  bump: number;
  owner: Address;
  series: number;
  amount: bigint;
  entitlement: bigint;
  lifetime: bigint;
  funded: bigint;
  pending: bigint;
  pendingTicket: bigint;
  firstSequence: bigint;
  active: boolean;
  slot: bigint;
  rentFundedByOps: bigint;
  rentFundedByOwner: bigint;
}

export interface NativeUser {
  bump: number;
  owner: Address;
  everA: boolean;
  everB: boolean;
  successfulAt: bigint[];
  callCount: number;
  bEntitlement: bigint;
}

export interface NativeUserRound {
  bump: number;
  roundId: bigint;
  owner: Address;
  deposits: bigint;
  netPayout: bigint;
  participantSlot: bigint;
}

export interface NativeTicket {
  bump: number;
  id: bigint;
  phase: number;
  closeId: bigint | null;
  capturedLen: bigint;
  expectedVersion: bigint;
  cursor: bigint;
  aCount: bigint;
  bCount: bigint;
  aRemainderTotal: Uint8Array;
  sealedADenominator: Uint8Array;
  oldest: Address;
  oldestSequence: bigint;
  oldestSlot: bigint;
  seniorCredit: bigint;
  seniorSlice: bigint;
  aSlice: bigint;
  bSlice: bigint;
  seedCredit: bigint;
  credited: bigint;
  toll: bigint;
  seniorBps: number;
  aBps: number;
  bBps: number;
}

export interface NativeCloseRecord {
  bump: number;
  closeId: bigint;
  roundId: bigint;
  configVersion: number;
  closeStartedAt: bigint;
  promotionDeadline: bigint;
  closingTicketId: bigint | null;
  cashCommitted: boolean;
  participantCount: bigint;
  candidateCursor: bigint;
  preferredCount: bigint;
  fallbackCount: bigint;
  preferredDigest: Uint8Array;
  fallbackDigest: Uint8Array;
  selectedSetTag: number;
  sealed: boolean;
  drawState: number;
  protocolSeed: Uint8Array;
  requestAccount: Address;
  randomness: Uint8Array;
  capturedAt: bigint;
  promotion: Promotion;
  finalized: boolean;
  requiredReserve: bigint;
  reserveSchedule: ReserveSchedule;
  requiredPagesPerSet: bigint;
  existingPreferredPages: bigint;
  existingFallbackPages: bigint;
  provisionedPreferredPages: bigint;
  provisionedFallbackPages: bigint;
  registryExtensionRequired: boolean;
  lockedSponsorRemaining: bigint;
  lockedOwnerRemaining: bigint;
  approvedProviderFeeCap: bigint;
  ownerRefundDestination: Address;
  spent: SpentLedger;
  requestRentFundedS: bigint;
  requestRentFundedO: bigint;
  reimbursementObserved: boolean;
  eligibleReturnRemaining: bigint;
  ownerRequestRentRefundableRemaining: bigint;
  requestRentReturned: bigint;
  reserveSettled: boolean;
  rent: RentComponents;
}

export interface NativeConfigVersion {
  bump: number;
  version: number;
  effectiveRoundId: bigint;
  stagedAt: bigint;
  activatedAt: bigint | null;
  planRatesBpsPerDay: number[];
  planDurationsS: number[];
  compoundFactorsQ18: bigint[];
  simpleTollBps: number[];
  simpleTollDayBounds: number[];
  jackpotBps: number[];
  frontEndLoadBps: number;
  minGame: bigint;
  minA: bigint;
  quotaCount: number;
  quotaWindowS: number;
  seedBps: number;
  seniorBps: number;
  aBps: number;
  bBps: number;
  aEntitlementBps: number;
  bEntitlementBps: number;
  openCutoff: bigint;
  deadlineSeconds: number;
  providerRequestFeeLamports: bigint;
  providerFeeHeadroomBps: number;
  pendingRequestBytes: number;
  fulfilledRequestBytes: number;
  providerProgram: Address;
  providerNetworkState: Address;
  providerTreasury: Address;
  providerClient: Address;
  publicSeriesAEnabled: boolean;
  pageCapacity: number;
}

export interface NativeOps {
  bump: number;
  sponsor: Address;
  freeBalance: bigint;
  lockedRemainingTotal: bigint;
  unallocatedExcess: bigint;
  protectedRent: bigint;
  providerFeeConsumedTotal: bigint;
  requestRentOutstanding: bigint;
  eligibleReturnBudget: bigint;
  providerReturnedTotal: bigint;
  providerDeficit: bigint;
  clientBalanceObserved: bigint;
  clientProtectedRent: bigint;
  totalContributed: bigint;
  totalWithdrawn: bigint;
  totalRentFunded: bigint;
}

export interface SnapshotRow {
  key: Address;
  series: number;
  remaining: bigint;
  slot: bigint;
}

export interface RegistryPageData {
  bump: number;
  index: bigint;
  keys: Address[];
  rent: RentComponents;
}

export interface SnapshotPageData {
  bump: number;
  index: bigint;
  ticketId: bigint;
  rows: SnapshotRow[];
  rent: RentComponents;
}

export interface ParticipantPageData {
  bump: number;
  roundId: bigint;
  page: bigint;
  len: number;
  rows: Address[];
  rent: RentComponents;
}

export interface CandidatePageData {
  bump: number;
  closeId: bigint;
  setTag: number;
  page: bigint;
  len: number;
  rows: Address[];
  rent: RentComponents;
}

// ---------------------------------------------------------------------------
// Transaction status
// ---------------------------------------------------------------------------

/**
 * The confirmation ladder for one submitted transaction.
 *
 *   submitted -> confirmed -> finalized          (landed)
 *   submitted -> failed                           (landed with a program error; state rolled back)
 *   unknown                                       (could not tell: RPC timeout / still not seen)
 *   expired                                       (blockhash height passed and the signature never
 *                                                  appeared: the transaction can no longer land)
 *
 * `unknown` is NOT failure. The signature and nonce stay valid and the caller
 * re-polls the SAME signature; it never rebuilds or re-signs while unknown.
 */
export type NativeTxStatus = 'submitted' | 'confirmed' | 'finalized' | 'failed' | 'unknown' | 'expired';

export interface StatusUpdate {
  signature: string;
  status: NativeTxStatus;
  /** Slot reported by the cluster, when it has seen the transaction. */
  slot?: number;
  /** The cluster's transaction error (`status === 'failed'`), or the RPC error behind `unknown`. */
  error?: unknown;
}

/** What every NativeClient transaction method returns. */
export interface NativeTxResult {
  signature: string;
  /** The open_position `client_nonce` (idempotency key); null for every other instruction. */
  nonce: bigint | null;
  /** The latest status observed so far (the first value `status$` yields). */
  status: NativeTxStatus;
  /** submitted -> confirmed -> finalized, from getSignatureStatuses polling (no wss). */
  status$: AsyncIterable<StatusUpdate>;
  /**
   * Re-poll the SAME signature (and re-send the SAME signed bytes if the first
   * send never reported success). Never rebuilds, re-signs or changes the nonce.
   */
  retry(): Promise<StatusUpdate>;
  /**
   * True once the transaction can no longer land (`failed` or `expired`), i.e.
   * it is safe to build a NEW transaction. Reusing an open_position nonce while
   * this is false risks a `Replay` on chain.
   */
  readonly safeToRebuild: boolean;
}

// ---------------------------------------------------------------------------
// Unified positions (legacy canister vs native program)
// ---------------------------------------------------------------------------

export type PositionSource = 'legacy-canister' | 'native-program';

export interface WalletPositionEntry<L = unknown, N = unknown> {
  /** Stable, source-qualified identity. Never equal across sources. */
  id: string;
  source: PositionSource;
  /** The numeric id in its own namespace: legacy game id, or native client nonce. */
  localId: bigint;
  wallet: Address;
  legacy?: L;
  native?: N;
}
