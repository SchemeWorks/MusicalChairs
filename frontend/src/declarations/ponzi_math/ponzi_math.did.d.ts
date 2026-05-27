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
    'backerDeposit' : {
      'backer' : Principal,
      'amount' : number,
      'entitlement' : number,
    }
  } |
  { 'backerRepaymentClaim' : { 'backer' : Principal, 'amount' : number } } |
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
export interface LineDisplayPage { 'lines' : Array<string> }
export interface PlatformStats {
  'daysActive' : bigint,
  'potBalance' : number,
  'totalWithdrawals' : number,
  'activeGames' : bigint,
  'totalDeposits' : number,
}
export interface PonziMath {
  'addBackerMoney' : ActorMethod<
    [number],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'adminClearAllBackerPositions' : ActorMethod<
    [],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'adminForceReset' : ActorMethod<
    [string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'adminGetActivePlansSnapshot' : ActorMethod<[], Array<ActivePlanSnapshot>>,
  'adminGetCurrentRoundId' : ActorMethod<[], bigint>,
  'adminGetEventsByRound' : ActorMethod<[bigint], Array<GeneralLedgerEntry>>,
  'adminGetEventsForGame' : ActorMethod<[bigint], Array<GeneralLedgerEntry>>,
  'adminGetRoundSummaries' : ActorMethod<[], Array<RoundSummary>>,
  'adminIsAdmin' : ActorMethod<[], boolean>,
  'adminMergeBackerPosition' : ActorMethod<
    [Principal, Principal],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'adminSweepUntracked' : ActorMethod<
    [],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'calculateCompounded30DayEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedROI' : ActorMethod<[], number>,
  'calculateEarnings' : ActorMethod<[GameRecord], number>,
  'checkDepositRateLimit' : ActorMethod<[], boolean>,
  'claimBackerRepayment' : ActorMethod<
    [],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
  'createBackdatedGame' : ActorMethod<
    [GamePlan, number, boolean, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'createGame' : ActorMethod<
    [GamePlan, number, boolean],
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
  'getCanisterICPBalance' : ActorMethod<[], bigint>,
  'getCoverChargeBalance' : ActorMethod<[], bigint>,
  'getCurrentRoundId' : ActorMethod<[], bigint>,
  'getDaysActive' : ActorMethod<[], bigint>,
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
  'getOldestSeriesABacker' : ActorMethod<[], [] | [BackerPosition]>,
  'getPlatformStats' : ActorMethod<[], PlatformStats>,
  'getRoundSeedReserve' : ActorMethod<[], number>,
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
  'isCriticalSectionBusy' : ActorMethod<[], boolean>,
  'settleCompoundingGame' : ActorMethod<
    [bigint],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
  'sweepCoverCharges' : ActorMethod<[], { 'Ok' : bigint } | { 'Err' : string }>,
  'withdrawEarnings' : ActorMethod<
    [bigint],
    { 'Ok' : number } |
      { 'Err' : string }
  >,
}
export interface RoundSummary {
  'startTime' : bigint,
  'endTime' : [] | [bigint],
  'endReason' : [] | [string],
  'roundId' : bigint,
  'eventCount' : bigint,
  'seedReserveCarried' : number,
}
export interface StandardRecord { 'url' : string, 'name' : string }
export interface TrustedOriginsResponse { 'trusted_origins' : Array<string> }
export interface _SERVICE extends PonziMath {}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
