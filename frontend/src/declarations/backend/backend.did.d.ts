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
export interface ShenaniganConfig {
  'id' : bigint,
  'backgroundColor' : string,
  'duration' : bigint,
  'cost' : number,
  'successOdds' : bigint,
  'name' : string,
  'backfireOdds' : bigint,
  'castLimit' : bigint,
  'description' : string,
  'effectValues' : Array<number>,
  'failureOdds' : bigint,
  'cooldown' : bigint,
}
export type ShenaniganOutcome = { 'backfire' : null } |
  { 'fail' : null } |
  { 'success' : null };
export interface ShenaniganRecord {
  'id' : bigint,
  'shenaniganType' : ShenaniganType,
  'cost' : number,
  'user' : Principal,
  'target' : [] | [Principal],
  'timestamp' : bigint,
  'outcome' : ShenaniganOutcome,
}
export interface ShenaniganStats {
  'backfires' : bigint,
  'dealerCut' : number,
  'totalCast' : bigint,
  'goodOutcomes' : bigint,
  'totalSpent' : number,
  'badOutcomes' : bigint,
}
export type ShenaniganType = { 'ppBoosterAura' : null } |
  { 'goldenName' : null } |
  { 'whaleRebalance' : null } |
  { 'downlineBoost' : null } |
  { 'moneyTrickster' : null } |
  { 'mintTaxSiphon' : null } |
  { 'aoeSkim' : null } |
  { 'magicMirror' : null } |
  { 'downlineHeist' : null } |
  { 'renameSpell' : null } |
  { 'purseCutter' : null };
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
  'calculateCompoundedEarnings' : ActorMethod<[GameRecord], number>,
  'calculateCompoundedROI' : ActorMethod<[], number>,
  'calculateEarnings' : ActorMethod<[GameRecord], number>,
  'castShenanigan' : ActorMethod<
    [ShenaniganType, [] | [Principal]],
    ShenaniganOutcome
  >,
  'checkDepositRateLimit' : ActorMethod<[], boolean>,
  'createGame' : ActorMethod<
    [GamePlan, number, boolean, [] | [Principal]],
    bigint
  >,
  'depositICP' : ActorMethod<[bigint], { 'Ok' : bigint } | { 'Err' : string }>,
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
  'getRecentShenanigans' : ActorMethod<[], Array<ShenaniganRecord>>,
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
  'getShenaniganConfigs' : ActorMethod<[], Array<ShenaniganConfig>>,
  'getShenaniganStats' : ActorMethod<[], ShenaniganStats>,
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
  'resetShenaniganConfig' : ActorMethod<[bigint], undefined>,
  'saveAllShenaniganConfigs' : ActorMethod<
    [Array<ShenaniganConfig>],
    undefined
  >,
  'saveCallerUserProfile' : ActorMethod<[UserProfile], undefined>,
  'setCanisterPrincipal' : ActorMethod<[Principal], undefined>,
  'setTestMode' : ActorMethod<[boolean], undefined>,
  'transferInternal' : ActorMethod<
    [Principal, bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'updateShenaniganConfig' : ActorMethod<[ShenaniganConfig], undefined>,
  'withdrawEarnings' : ActorMethod<[bigint], number>,
  'withdrawICP' : ActorMethod<[bigint], { 'Ok' : bigint } | { 'Err' : string }>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
