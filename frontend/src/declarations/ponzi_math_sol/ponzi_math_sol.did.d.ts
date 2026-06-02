import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface ActivePlanSnapshot {
  'daysToMaturity' : number,
  'daysElapsed' : number,
  'currentGrossEarnings' : number,
  'game' : GameRecord,
  'currentNetClaimable' : number,
  'isMatured' : boolean,
  'wouldBeInsolvent' : boolean,
  'currentExitToll' : number,
}
export interface BackerIntent {
  'id' : bigint,
  'principal' : Principal,
  'expectedAmountLamports' : bigint,
  'expiresAt' : bigint,
  'fulfilled' : boolean,
  'createdAt' : bigint,
}
export type BackerKey = [Principal, BackerType];
export interface BackerPosition {
  'startTime' : bigint,
  'firstDepositDate' : [] | [bigint],
  'owner' : Principal,
  'backerType' : BackerType,
  'isActive' : boolean,
  'amount' : number,
  'entitlement' : number,
}
export type BackerType = { 'seriesA' : null } |
  { 'seriesB' : null };
export interface BuyIntent {
  'id' : bigint,
  'quotedLamports' : bigint,
  'principal' : Principal,
  'expiresAt' : bigint,
  'fulfilled' : boolean,
  'createdAt' : bigint,
  'reserved' : Array<BuyReservation>,
  'ppUnitsReservedTotal' : bigint,
}
export interface BuyReservation {
  'ppUnits' : bigint,
  'ratePpUnitsPer0_1Sol' : bigint,
  'tierIndex' : bigint,
}
export interface ConsentInfo {
  'metadata' : ConsentMessageMetadata,
  'consent_message' : ConsentMessage,
}
export type ConsentMessage = {
    'LineDisplayMessage' : { 'pages' : Array<LineDisplayPage> }
  } |
  { 'GenericDisplayMessage' : string };
export interface ConsentMessageMetadata {
  'utc_offset_minutes' : [] | [number],
  'language' : string,
}
export interface ConsentMessageRequest {
  'arg' : Uint8Array | number[],
  'method' : string,
  'user_preferences' : ConsentMessageSpec,
}
export type ConsentMessageResponse = { 'Ok' : ConsentInfo } |
  { 'Err' : Icrc21Error };
export interface ConsentMessageSpec {
  'metadata' : ConsentMessageMetadata,
  'device_spec' : [] | [DeviceSpec],
}
export interface DepositIntent {
  'id' : bigint,
  'principal' : Principal,
  'expectedAmountLamports' : bigint,
  'expiresAt' : bigint,
  'fulfilled' : boolean,
  'createdAt' : bigint,
  'plan' : GamePlan,
}
export interface DeskQuote {
  'legs' : Array<QuoteLeg>,
  'ppUnitsOut' : bigint,
  'cappedByInventory' : boolean,
}
export interface DeskTier {
  'ratePpUnitsPer0_1Sol' : bigint,
  'ppUnitsTotal' : bigint,
  'ppUnitsReserved' : bigint,
  'ppUnitsSold' : bigint,
}
export type DeviceSpec = { 'GenericDisplay' : null } |
  {
    'LineDisplay' : {
      'characters_per_line' : number,
      'lines_per_page' : number,
    }
  };
export type GamePlan = { 'compounding15Day' : null } |
  { 'simple21Day' : null } |
  { 'compounding30Day' : null };
export interface GameRecord {
  'id' : bigint,
  'startTime' : bigint,
  'player' : Principal,
  'plan' : GamePlan,
  'isActive' : boolean,
  'accumulatedEarnings' : number,
  'lastUpdateTime' : bigint,
  'isCompounding' : boolean,
  'totalWithdrawn' : number,
  'amount' : number,
}
export interface GameResetRecord { 'resetTime' : bigint, 'reason' : string }
export interface GeneralLedgerEntry {
  'id' : bigint,
  'event' : GeneralLedgerEvent,
  'roundId' : bigint,
  'timestamp' : bigint,
}
export type GeneralLedgerEvent = {
    'tollDistribution' : {
      'tollAmount' : number,
      'toAllBackers' : number,
      'toSeedReserve' : number,
      'toOtherSeriesA' : number,
      'toOldestSeriesA' : number,
    }
  } |
  { 'gameReset' : { 'reason' : string, 'seedReserveCarried' : number } } |
  {
    'seriesBPromotion' : {
      'owner' : Principal,
      'underwater' : number,
      'entitlement' : number,
    }
  } |
  {
    'deposit' : {
      'netToPot' : number,
      'player' : Principal,
      'plan' : GamePlan,
      'gameId' : bigint,
      'coverCharge' : number,
      'isCompounding' : boolean,
      'gross' : number,
    }
  } |
  {
    'coverChargeAccrued' : {
      'player' : Principal,
      'gameId' : bigint,
      'amountE8s' : bigint,
    }
  } |
  {
    'backdatedGameCreated' : {
      'startTime' : bigint,
      'admin' : Principal,
      'player' : Principal,
      'gameId' : bigint,
      'amount' : number,
    }
  } |
  {
    'deskSale' : {
      'intentId' : bigint,
      'ppUnitsCredited' : bigint,
      'lamportsReceived' : bigint,
      'buyer' : Principal,
    }
  } |
  {
    'withdrawal' : {
      'netToPlayer' : number,
      'player' : Principal,
      'toll' : number,
      'gameId' : bigint,
      'isInsolvent' : boolean,
      'potDeduction' : number,
      'grossEarnings' : number,
    }
  } |
  {
    'coverChargeSwept' : {
      'toBackend' : Principal,
      'blockIndex' : bigint,
      'amountE8s' : bigint,
    }
  } |
  {
    'deskRefund' : {
      'txSig' : string,
      'lamports' : bigint,
      'toAddress' : string,
    }
  } |
  {
    'backerDeposit' : {
      'backer' : Principal,
      'amount' : number,
      'entitlement' : number,
    }
  } |
  { 'backerRepaymentClaim' : { 'backer' : Principal, 'amount' : number } } |
  {
    'deskProceedsWithdrawal' : {
      'txSig' : string,
      'lamports' : bigint,
      'toAddress' : string,
    }
  } |
  {
    'settlement' : {
      'netToPlayer' : number,
      'player' : Principal,
      'toll' : number,
      'gameId' : bigint,
      'isInsolvent' : boolean,
      'potDeduction' : number,
      'grossEarnings' : number,
    }
  };
export type Icrc21Error = {
    'GenericError' : { 'description' : string, 'error_code' : bigint }
  } |
  { 'UnsupportedCanisterCall' : { 'description' : string } } |
  { 'ConsentMessageUnavailable' : { 'description' : string } };
export interface KeyId { 'algorithm' : SchnorrAlgorithm, 'name' : string }
export interface LineDisplayPage { 'lines' : Array<string> }
export interface PlatformStats {
  'daysActive' : bigint,
  'potBalance' : number,
  'totalWithdrawals' : number,
  'activeGames' : bigint,
  'totalDeposits' : number,
}
export interface PonziMathSol {
  'adminClearAllBackerPositions' : ActorMethod<
    [],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Admin: manually credit an unmatched / TTL-expired SOL deposit.
   * / `lamports` is the gross detected amount; cover charge is
   * / computed at the standard 4% rate. Used to clear admin-review
   * / entries flagged by creditDeposit when no intent matched.
   */
  'adminCreditManualDeposit' : ActorMethod<
    [Principal, GamePlan, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  /**
   * / Admin-callable: derive the pool address via threshold-Schnorr and
   * / cache it. Idempotent — subsequent calls just return the cached
   * / value. Must be called once before bootstrap() so the operator can
   * / fund the pool.
   */
  'adminDerivePoolAddress' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'adminForceReset' : ActorMethod<
    [string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'adminGetActivePlansSnapshot' : ActorMethod<[], Array<ActivePlanSnapshot>>,
  'adminGetAllBuyIntents' : ActorMethod<[], Array<BuyIntent>>,
  'adminGetAllIntents' : ActorMethod<[], Array<DepositIntent>>,
  'adminGetCurrentRoundId' : ActorMethod<[], bigint>,
  'adminGetEventsByRound' : ActorMethod<[bigint], Array<GeneralLedgerEntry>>,
  'adminGetEventsForGame' : ActorMethod<[bigint], Array<GeneralLedgerEntry>>,
  'adminGetRoundSummaries' : ActorMethod<[], Array<RoundSummary>>,
  'adminIsAdmin' : ActorMethod<[], boolean>,
  /**
   * / Admin: mark the canister as bootstrapped from on-chain state.
   * / Use when the bootstrap tx broadcast succeeded on Solana but the
   * / IC-side bookkeeping didn't commit (e.g. decode trap on a stale
   * / sol-rpc binding). Reads the nonce account, parses the initial
   * / nonce, sets `lastNonceValue` and flips `bootstrapped := true`.
   * / Idempotent.
   */
  'adminMarkBootstrapped' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'adminMergeBackerPosition' : ActorMethod<
    [Principal, Principal],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Admin: refresh the cached nonce by reading account info on-chain.
   * / Use to recover from a nonce desync (e.g., broadcast succeeded but
   * / the local nonce-refresh read failed). Idempotent.
   */
  'adminRefreshNonce' : ActorMethod<[], { 'Ok' : string } | { 'Err' : string }>,
  'adminRefundDeskSol' : ActorMethod<
    [string, bigint],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  /**
   * / Admin: record a Series A backer position for `owner` of `amount`
   * / SOL. Use ONCE at deploy to register the operator's pre-deposited
   * / pool seed. Mirrors ponzi_math.addBackerMoney's bookkeeping but
   * / skips the synchronous transfer-from (the SOL is already on the
   * / pool address, deposited out-of-band by the operator).
   */
  'adminRegisterSeriesABacker' : ActorMethod<
    [Principal, number],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Admin: update solTreasuryAddress (the destination of payManagementSol).
   */
  'adminSetSolTreasuryAddress' : ActorMethod<
    [string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Admin: start (or restart) the auto-detection timer.
   */
  'adminStartDetectionTimer' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  /**
   * / Admin: stop the auto-detection timer. Manual runDepositDetection and
   * / adminCreditManualDeposit still work while it is stopped.
   */
  'adminStopDetectionTimer' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  /**
   * / Admin: retry the sweep from a per-user deposit address to the pool.
   * / Use when creditDeposit credited the game but sweepToPool failed
   * / (Debug.print'd "Sweep failed for ..."). Looks up the principal,
   * / reads the current on-chain balance, and reissues the sweep tx.
   * / Safe to call repeatedly — if the balance is already at dust,
   * / returns an error rather than building a no-op tx.
   */
  'adminSweepDepositAddress' : ActorMethod<
    [string],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  /**
   * / Admin: compute the difference between the pool address's actual
   * / on-chain balance and the sum of internal accounting (pot +
   * / roundSeedReserve + repayments + coverChargeAccrual). If positive
   * / (untracked dust), send it to the testAdmin's deposit address.
   * / No-op otherwise.
   */
  'adminSweepUntracked' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'adminTestCreateBuyIntent' : ActorMethod<
    [Principal, bigint],
    {
        'Ok' : {
          'expiresAt' : bigint,
          'intentId' : bigint,
          'legs' : Array<QuoteLeg>,
          'depositAddress' : string,
          'ppUnitsReserved' : bigint,
        }
      } |
      { 'Err' : string }
  >,
  'adminTestSettleBuyIntent' : ActorMethod<
    [bigint, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'adminWithdrawDeskProceeds' : ActorMethod<
    [string],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'bootstrap' : ActorMethod<
    [[] | [string]],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'calculateCompounded30DayEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedROI' : ActorMethod<[], number>,
  'calculateEarnings' : ActorMethod<[GameRecord], number>,
  'checkDepositRateLimit' : ActorMethod<[], boolean>,
  'claimBackerRepayment' : ActorMethod<
    [[] | [string]],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
  'createBuyIntent' : ActorMethod<
    [bigint],
    {
        'Ok' : {
          'expiresAt' : bigint,
          'intentId' : bigint,
          'legs' : Array<QuoteLeg>,
          'depositAddress' : string,
          'ppUnitsReserved' : bigint,
        }
      } |
      { 'Err' : string }
  >,
  'deskAddTier' : ActorMethod<
    [bigint, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'deskDepositInventory' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'deskInventory' : ActorMethod<
    [],
    {
      'balanceUnits' : bigint,
      'reservedUnits' : bigint,
      'availableUnits' : bigint,
    }
  >,
  'deskListTiers' : ActorMethod<[], Array<DeskTier>>,
  'deskRemoveTier' : ActorMethod<
    [bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'deskReorderTiers' : ActorMethod<
    [Array<DeskTier>],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'deskStats' : ActorMethod<
    [],
    {
      'inventoryUnits' : bigint,
      'totalSoldUnits' : bigint,
      'openBuyIntents' : bigint,
      'proceedsLamports' : bigint,
      'reservedUnits' : bigint,
      'availableUnits' : bigint,
      'tierCount' : bigint,
    }
  >,
  'deskUpdateTier' : ActorMethod<
    [bigint, bigint, bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'deskWithdrawInventory' : ActorMethod<
    [bigint, Principal],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'getActiveGameCount' : ActorMethod<[], bigint>,
  'getAllActiveGames' : ActorMethod<[], Array<GameRecord>>,
  'getAllBackerRepayments' : ActorMethod<[], Array<[BackerKey, number]>>,
  'getAllGames' : ActorMethod<[], Array<GameRecord>>,
  'getAvailableBalance' : ActorMethod<[], number>,
  'getBackerPositions' : ActorMethod<[], Array<BackerPosition>>,
  'getBackerRepaymentBalance' : ActorMethod<[], number>,
  'getBackerRepaymentBalanceFor' : ActorMethod<[Principal], number>,
  'getCanisterSolBalance' : ActorMethod<[], bigint>,
  'getCoverChargeAccrualLamports' : ActorMethod<[], bigint>,
  'getCoverChargeBalance' : ActorMethod<[], bigint>,
  'getCurrentRoundId' : ActorMethod<[], bigint>,
  'getDaysActive' : ActorMethod<[], bigint>,
  'getDepositAddressFor' : ActorMethod<[Principal], [] | [string]>,
  'getDeskEscrowAccount' : ActorMethod<
    [],
    { 'owner' : Principal, 'subaccount' : Uint8Array | number[] }
  >,
  /**
   * / Status of the auto-detection timer and the open-intent backlog.
   */
  'getDetectionStatus' : ActorMethod<
    [],
    {
      'intervalSeconds' : bigint,
      'openIntents' : bigint,
      'timerArmed' : boolean,
      'inProgress' : boolean,
    }
  >,
  'getGameById' : ActorMethod<[bigint], [] | [GameRecord]>,
  'getGameResetHistory' : ActorMethod<[], Array<GameResetRecord>>,
  'getGeneralLedger' : ActorMethod<[], Array<GeneralLedgerEntry>>,
  'getGeneralLedgerPage' : ActorMethod<
    [bigint, bigint],
    { 'total' : bigint, 'entries' : Array<GeneralLedgerEntry> }
  >,
  'getGeneralLedgerStats' : ActorMethod<
    [],
    {
      'entryCount' : bigint,
      'netFlow' : number,
      'totalOutflows' : number,
      'totalInflows' : number,
    }
  >,
  'getMaxDepositLimit' : ActorMethod<[], number>,
  'getMyDepositAddress' : ActorMethod<[], [] | [string]>,
  'getMyPendingBackerIntents' : ActorMethod<[], Array<BackerIntent>>,
  'getMyPendingBuyIntents' : ActorMethod<[], Array<BuyIntent>>,
  'getMyPendingIntents' : ActorMethod<[], Array<DepositIntent>>,
  'getNonceAccountAddress' : ActorMethod<[], [] | [string]>,
  'getOldestSeriesABacker' : ActorMethod<[], [] | [BackerPosition]>,
  'getOrCreateDepositAddress' : ActorMethod<
    [],
    { 'Ok' : string } |
      { 'Err' : string }
  >,
  'getPlatformStats' : ActorMethod<[], PlatformStats>,
  'getPoolAddress' : ActorMethod<[], [] | [string]>,
  'getRoundSeedReserve' : ActorMethod<[], number>,
  'getSolTreasuryAddress' : ActorMethod<[], string>,
  'getTotalBackerDebt' : ActorMethod<[], number>,
  'getTotalDeposits' : ActorMethod<[], number>,
  'getTotalWithdrawals' : ActorMethod<[], number>,
  'getUserGames' : ActorMethod<[], Array<GameRecord>>,
  'getUserGamesFor' : ActorMethod<[Principal], Array<GameRecord>>,
  'icrc10_supported_standards' : ActorMethod<[], Array<StandardRecord>>,
  'icrc21_canister_call_consent_message' : ActorMethod<
    [ConsentMessageRequest],
    ConsentMessageResponse
  >,
  'icrc28_trusted_origins' : ActorMethod<[], TrustedOriginsResponse>,
  'isBootstrapped' : ActorMethod<[], boolean>,
  'isCriticalSectionBusy' : ActorMethod<[], boolean>,
  'payManagementSol' : ActorMethod<[], { 'Ok' : string } | { 'Err' : string }>,
  /**
   * / User-triggered detection for the CALLER's own deposit address only.
   * / Lets the frontend get a near-instant credit right after the user's
   * / wallet confirms the SOL transfer, instead of waiting for the 60s timer.
   * / Abuse-bounded: makes ZERO RPC outcalls unless the caller has an open,
   * / unexpired intent (deposit or buy), and is rate-limited to once per
   * / POKE_COOLDOWN_NS per caller. Shares the detectionInProgress guard with
   * / the auto-timer so the two never run concurrently.
   */
  'pokeMyDeposit' : ActorMethod<[], { 'Ok' : bigint } | { 'Err' : string }>,
  /**
   * / Self-serve Series A backing for SIWS/SOL users — the SOL analog of
   * / ponzi_math.addBackerMoney. Creates a BackerIntent; the next matching SOL
   * / landing on the caller's deposit address registers/merges their Series A
   * / position (in creditDeposit). NO Front-End Load (matches the ICP + admin
   * / backer paths). Min 0.05 SOL.
   */
  'prepareBackerDeposit' : ActorMethod<
    [{ 'expectedAmountLamports' : bigint }],
    { 'Ok' : { 'intentId' : bigint, 'depositAddress' : string } } |
      { 'Err' : string }
  >,
  'prepareSolDeposit' : ActorMethod<
    [{ 'expectedAmountLamports' : bigint, 'plan' : GamePlan }],
    { 'Ok' : { 'intentId' : bigint, 'depositAddress' : string } } |
      { 'Err' : string }
  >,
  'quoteBuyPP' : ActorMethod<[bigint], DeskQuote>,
  /**
   * / Admin-callable manual detection sweep. Scans EVERY known deposit
   * / address (not just those with open intents) so the operator can use it
   * / for diagnostics / recovery. Returns the count of new GameRecords
   * / created (zero is normal when nothing arrived). Shares the
   * / detectionInProgress guard with the auto-detection timer so the two
   * / never run concurrently.
   */
  'runDepositDetection' : ActorMethod<
    [],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'selfTestBase58' : ActorMethod<[], boolean>,
  'selfTestSolTx' : ActorMethod<
    [],
    {
      'compactU16_42' : Uint8Array | number[],
      'u64Le_1B' : Uint8Array | number[],
      'u64Le_1' : Uint8Array | number[],
      'compactU16_128' : Uint8Array | number[],
      'compactU16_300' : Uint8Array | number[],
    }
  >,
  'settleCompoundingGame' : ActorMethod<
    [bigint, [] | [string]],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
  'withdrawEarnings' : ActorMethod<
    [bigint, [] | [string]],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
}
export type Provider = { 'mainnet' : null } |
  { 'devnet' : null };
export interface QuoteLeg {
  'ppUnits' : bigint,
  'lamports' : bigint,
  'ratePpUnitsPer0_1Sol' : bigint,
  'tierIndex' : bigint,
}
export interface RoundSummary {
  'startTime' : bigint,
  'endTime' : [] | [bigint],
  'endReason' : [] | [string],
  'roundId' : bigint,
  'eventCount' : bigint,
  'seedReserveCarried' : number,
}
export type SchnorrAlgorithm = { 'ed25519' : null } |
  { 'bip340secp256k1' : null };
export interface StandardRecord { 'url' : string, 'name' : string }
export interface TrustedOriginsResponse { 'trusted_origins' : Array<string> }
export interface _SERVICE extends PonziMathSol {}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
