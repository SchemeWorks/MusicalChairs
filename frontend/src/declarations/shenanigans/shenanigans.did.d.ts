import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface CashOutEntry {
  'id' : bigint,
  'player' : Principal,
  'claimed' : boolean,
  'claimableAfter' : bigint,
  'amount' : bigint,
}
export interface MintConfig {
  'compounding15DayPpPerIcp' : bigint,
  'minDepositPp' : bigint,
  'dealerPpPerIcp' : bigint,
  'compounding30DayPpPerIcp' : bigint,
  'referralL1Bps' : bigint,
  'referralL2Bps' : bigint,
  'referralL3Bps' : bigint,
  'observerIntervalSeconds' : bigint,
  'cashOutDelaySeconds' : bigint,
  'simple21DayPpPerIcp' : bigint,
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
export interface _SERVICE {
  /**
   * / Admin-triggered manual PP issuance (direct mint to the player's chip
   * / subaccount). Use for fixups, comps, or seeding test accounts.
   */
  'adminMint' : ActorMethod<
    [Principal, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'castShenanigan' : ActorMethod<
    [ShenaniganType, [] | [Principal]],
    ShenaniganOutcome
  >,
  'claimCashOut' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  /**
   * / Pull `amountUnits` PP-units from the caller's wallet into their
   * / chip subaccount. Caller must have signed icrc2_approve on pp_ledger
   * / beforehand.
   */
  'depositChips' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  /**
   * / Pending and recently claimed cash-outs for a given user.
   */
  'getCashOutsFor' : ActorMethod<[Principal], Array<CashOutEntry>>,
  'getMintConfig' : ActorMethod<[], MintConfig>,
  'getMyCashOuts' : ActorMethod<[], Array<CashOutEntry>>,
  /**
   * / Current observer state — running/paused, cursor positions, and interval.
   * / Surfaced in the admin panel for operational visibility.
   */
  'getObserverStatus' : ActorMethod<
    [],
    {
      'gameIdCursor' : bigint,
      'intervalSeconds' : bigint,
      'dealerSeenCount' : bigint,
      'running' : boolean,
    }
  >,
  'getPpBurnedFor' : ActorMethod<[Principal], bigint>,
  'getRecentShenanigans' : ActorMethod<[], Array<ShenaniganRecord>>,
  'getShenaniganConfigs' : ActorMethod<[], Array<ShenaniganConfig>>,
  'getShenaniganStats' : ActorMethod<[], ShenaniganStats>,
  /**
   * / Top-N players by cumulative PP burned. Returns (principal, PP-units).
   */
  'getTopPpBurners' : ActorMethod<[bigint], Array<[Principal, bigint]>>,
  /**
   * / Top-N players by number of spells cast (success + backfire).
   */
  'getTopSpellCasters' : ActorMethod<[bigint], Array<[Principal, bigint]>>,
  'initialize' : ActorMethod<[Principal], undefined>,
  /**
   * / One-shot catch-up primer. Admin only. Call immediately after the
   * / cutover upgrade completes, before unpausing user traffic.
   */
  'primeObserverCursors' : ActorMethod<[], undefined>,
  'requestCashOut' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'resetShenaniganConfig' : ActorMethod<[bigint], undefined>,
  'resumeObserver' : ActorMethod<[], undefined>,
  'rotateAdmin' : ActorMethod<[Principal], undefined>,
  /**
   * / Manual one-shot observer tick (admin debug).
   */
  'runObserverOnce' : ActorMethod<[], undefined>,
  'saveAllShenaniganConfigs' : ActorMethod<
    [Array<ShenaniganConfig>],
    undefined
  >,
  'setCashOutDelaySeconds' : ActorMethod<[bigint], undefined>,
  'setCompounding15DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setCompounding30DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setDealerPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setMinDepositPp' : ActorMethod<[bigint], undefined>,
  'setObserverIntervalSeconds' : ActorMethod<[bigint], undefined>,
  'setReferralBps' : ActorMethod<[bigint, bigint, bigint], undefined>,
  'setSimple21DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'stopObserver' : ActorMethod<[], undefined>,
  'updateShenaniganConfig' : ActorMethod<[ShenaniganConfig], undefined>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
