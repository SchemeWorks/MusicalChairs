import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

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
export interface PlatformStats {
  'daysActive' : bigint,
  'potBalance' : number,
  'totalWithdrawals' : number,
  'activeGames' : bigint,
  'totalDeposits' : number,
}
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
  'calculateCompoundedEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedROI' : ActorMethod<[], number>,
  'calculateEarnings' : ActorMethod<[GameRecord], number>,
  'checkDepositRateLimit' : ActorMethod<[], boolean>,
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
  'initializeAccessControl' : ActorMethod<[], undefined>,
  'isCallerAdmin' : ActorMethod<[], boolean>,
  'isTestMode' : ActorMethod<[], boolean>,
  'saveCallerUserProfile' : ActorMethod<[UserProfile], undefined>,
  'seedGame' : ActorMethod<
    [Principal, GamePlan, number, boolean, bigint],
    bigint
  >,
  'setCanisterPrincipal' : ActorMethod<[Principal], undefined>,
  'setShenanigansPrincipal' : ActorMethod<[Principal], undefined>,
  'setTestMode' : ActorMethod<[boolean], undefined>,
  'transferInternal' : ActorMethod<
    [Principal, bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'transferPonziPoints' : ActorMethod<
    [Principal, Principal, number],
    undefined
  >,
  'withdrawEarnings' : ActorMethod<[bigint], number>,
  'withdrawICP' : ActorMethod<[bigint], { 'Ok' : bigint } | { 'Err' : string }>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
