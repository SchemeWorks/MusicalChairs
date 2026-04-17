import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

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
export interface CoverChargeEntry {
  'id' : bigint,
  'player' : Principal,
  'gameId' : bigint,
  'timestamp' : bigint,
  'amount' : bigint,
}
export interface DealerPosition {
  'startTime' : bigint,
  'firstDepositDate' : [] | [bigint],
  'owner' : Principal,
  'name' : string,
  'isActive' : boolean,
  'dealerType' : DealerType,
  'amount' : number,
  'entitlement' : number,
}
export type DealerType = { 'downstream' : null } |
  { 'upstream' : null };
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
export interface HouseLedgerRecord {
  'id' : bigint,
  'description' : string,
  'timestamp' : bigint,
  'amount' : number,
}
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
export interface StandardRecord { 'url' : string, 'name' : string }
export interface TrustedOriginsResponse { 'trusted_origins' : Array<string> }
export interface UserProfile { 'name' : string }
export type UserRole = { 'admin' : null } |
  { 'user' : null } |
  { 'guest' : null };
export interface WalletTransaction {
  'id' : bigint,
  'user' : Principal,
  'description' : string,
  'timestamp' : bigint,
  'txType' : { 'gameWithdrawal' : null } |
    { 'gameDeposit' : null } |
    { 'deposit' : null } |
    { 'withdrawal' : null } |
    { 'transfer' : null },
  'ledgerBlockIndex' : [] | [bigint],
  'amount' : bigint,
}
export interface _SERVICE {
  'addDealerMoney' : ActorMethod<[number], undefined>,
  'addDownstreamDealer' : ActorMethod<[number, number], undefined>,
  'addHouseMoney' : ActorMethod<[number, string], undefined>,
  'assignCallerUserRole' : ActorMethod<[Principal, UserRole], undefined>,
  'burnPonziPoints' : ActorMethod<[Principal, number], undefined>,
  'calculateCompounded30DayEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedROI' : ActorMethod<[], number>,
  'calculateEarnings' : ActorMethod<[GameRecord], number>,
  'checkDepositRateLimit' : ActorMethod<[], boolean>,
  'claimDealerRepayment' : ActorMethod<[], number>,
  'createGame' : ActorMethod<
    [GamePlan, number, boolean, [] | [Principal]],
    bigint
  >,
  'deductPonziPoints' : ActorMethod<[Principal, number], undefined>,
  'depositICP' : ActorMethod<[bigint], { 'Ok' : bigint } | { 'Err' : string }>,
  'distributeDealerCutFromShenanigans' : ActorMethod<[number], undefined>,
  'distributeFees' : ActorMethod<[number], undefined>,
  'getActiveGameCount' : ActorMethod<[], bigint>,
  'getAllActiveGames' : ActorMethod<[], Array<GameRecord>>,
  'getAllGames' : ActorMethod<[], Array<GameRecord>>,
  'getAvailableBalance' : ActorMethod<[], number>,
  'getCallerUserProfile' : ActorMethod<[], [] | [UserProfile]>,
  'getCallerUserRole' : ActorMethod<[], UserRole>,
  'getCanisterICPBalance' : ActorMethod<[], bigint>,
  'getCanisterPrincipal' : ActorMethod<[], [] | [Principal]>,
  'getCoverChargeBalance' : ActorMethod<[], bigint>,
  'getCoverChargeTransactions' : ActorMethod<[], Array<CoverChargeEntry>>,
  'getDaysActive' : ActorMethod<[], bigint>,
  'getDealerPositions' : ActorMethod<[], Array<DealerPosition>>,
  'getDealerRepaymentBalance' : ActorMethod<[], number>,
  'getGameById' : ActorMethod<[bigint], [] | [GameRecord]>,
  'getGameResetHistory' : ActorMethod<[], Array<GameResetRecord>>,
  'getHouseLedger' : ActorMethod<[], Array<HouseLedgerRecord>>,
  'getHouseLedgerStats' : ActorMethod<
    [],
    {
      'totalWithdrawals' : number,
      'netBalance' : number,
      'totalDeposits' : number,
      'recordCount' : bigint,
    }
  >,
  'getMaxDepositLimit' : ActorMethod<[], number>,
  'getOldestUpstreamDealer' : ActorMethod<[], [] | [DealerPosition]>,
  'getPlatformStats' : ActorMethod<[], PlatformStats>,
  'getPonziPoints' : ActorMethod<[], number>,
  'getPonziPointsBalance' : ActorMethod<
    [],
    {
      'depositPoints' : number,
      'referralPoints' : number,
      'totalPoints' : number,
    }
  >,
  'getPonziPointsBalanceFor' : ActorMethod<[Principal], number>,
  'getReferralEarnings' : ActorMethod<[Principal], number>,
  'getReferralTierPoints' : ActorMethod<
    [],
    {
      'level3Points' : number,
      'level1Points' : number,
      'totalPoints' : number,
      'level2Points' : number,
    }
  >,
  'getTopPonziPointsBurners' : ActorMethod<[], Array<[Principal, number]>>,
  'getTopPonziPointsHolders' : ActorMethod<[], Array<[Principal, number]>>,
  'getTotalDealerDebt' : ActorMethod<[], number>,
  'getTotalDeposits' : ActorMethod<[], number>,
  'getTotalHouseMoneyAdded' : ActorMethod<[], number>,
  'getTotalWithdrawals' : ActorMethod<[], number>,
  'getUserGames' : ActorMethod<[], Array<GameRecord>>,
  'getUserProfile' : ActorMethod<[Principal], [] | [UserProfile]>,
  'getWalletBalance' : ActorMethod<[], bigint>,
  'getWalletBalanceICP' : ActorMethod<[], number>,
  'getWalletTransactions' : ActorMethod<[], Array<WalletTransaction>>,
  'icrc10_supported_standards' : ActorMethod<[], Array<StandardRecord>>,
  'icrc21_canister_call_consent_message' : ActorMethod<
    [ConsentMessageRequest],
    ConsentMessageResponse
  >,
  'icrc28_trusted_origins' : ActorMethod<[], TrustedOriginsResponse>,
  'initializeAccessControl' : ActorMethod<[], undefined>,
  'isCallerAdmin' : ActorMethod<[], boolean>,
  'isTestMode' : ActorMethod<[], boolean>,
  'saveCallerUserProfile' : ActorMethod<[UserProfile], undefined>,
  'seedGame' : ActorMethod<
    [Principal, GamePlan, number, boolean, bigint],
    bigint
  >,
  'seedReferral' : ActorMethod<[Principal, Principal], undefined>,
  'setCanisterPrincipal' : ActorMethod<[Principal], undefined>,
  'setShenanigansPrincipal' : ActorMethod<[Principal], undefined>,
  'setTestMode' : ActorMethod<[boolean], undefined>,
  'settleCompoundingGame' : ActorMethod<[bigint], number>,
  'transferInternal' : ActorMethod<
    [Principal, bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'transferPonziPoints' : ActorMethod<
    [Principal, Principal, number],
    undefined
  >,
  'withdrawCoverCharges' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'withdrawEarnings' : ActorMethod<[bigint], number>,
  'withdrawICP' : ActorMethod<[bigint], { 'Ok' : bigint } | { 'Err' : string }>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
