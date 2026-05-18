import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface ActiveSpellEffects {
  'mintMultiplier' : [] | [MintMultiplier],
  'shield' : [] | [ShieldState],
  'displayName' : [] | [DisplayNameOverride],
  'golden' : boolean,
  'mintSiphon' : [] | [MintSiphon],
  'cascadeBoost' : [] | [CascadeBoost],
}
export interface CascadeBoost { 'expiresAt' : bigint, 'multiplierBps' : bigint }
export interface CashOutEntry {
  'id' : bigint,
  'cancelled' : boolean,
  'player' : Principal,
  'claimed' : boolean,
  'claimableAfter' : bigint,
  'amount' : bigint,
}
export interface DisplayNameOverride { 'expiresAt' : bigint, 'name' : string }
export interface MintConfig {
  'compounding15DayPpPerIcp' : bigint,
  'minDepositPp' : bigint,
  'compounding30DayPpPerIcp' : bigint,
  'referralL1Bps' : bigint,
  'referralL2Bps' : bigint,
  'referralL3Bps' : bigint,
  'observerIntervalSeconds' : bigint,
  'backerPpPerIcp' : bigint,
  'cashOutDelaySeconds' : bigint,
  'simple21DayPpPerIcp' : bigint,
}
export interface MintMultiplier {
  'expiresAt' : bigint,
  'multiplierBps' : bigint,
}
export interface MintSiphon {
  'expiresAt' : bigint,
  'pctTimes100' : bigint,
  'siphonedSoFar' : bigint,
  'siphoner' : Principal,
  'capUnits' : bigint,
}
export interface SignupEntry {
  'principal' : Principal,
  'joinedAt' : bigint,
  'level' : bigint,
}
export interface ReferralStats {
  'l1Count' : bigint,
  'l3Units' : bigint,
  'l1Units' : bigint,
  'l2Count' : bigint,
  'l2Units' : bigint,
  'l3Count' : bigint,
  'recentSignups' : Array<SignupEntry>,
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
export interface ShieldState {
  'expiresAt' : bigint,
  'chargesRemaining' : bigint,
}
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
  'cancelCashOut' : ActorMethod<[bigint], { 'Ok' : null } | { 'Err' : string }>,
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
   * / All active spell effects on `user`. Expired entries are filtered
   * / out of the result but not deleted from state (cleanup happens
   * / lazily on next write/cast).
   */
  'getActiveSpellEffects' : ActorMethod<[Principal], ActiveSpellEffects>,
  /**
   * / Pending and recently claimed cash-outs for a given user.
   */
  'getCashOutsFor' : ActorMethod<[Principal], Array<CashOutEntry>>,
  /**
   * / Active rename-spell name for `user`, if any. Expired entries return null.
   */
  'getCustomDisplayName' : ActorMethod<[Principal], [] | [string]>,
  /**
   * / Currently-golden players. Used by frontend for leaderboard styling.
   */
  'getGoldenPlayers' : ActorMethod<[], Array<Principal>>,
  /**
   * / All principals we've ever minted PP to. Frontend target-pickers can
   * / use this to populate a candidate list. Updated lazily — entries are
   * / added in mintInternal and never removed (cheap, bounded by player count).
   */
  'getKnownPpHolders' : ActorMethod<[], Array<Principal>>,
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
      'running' : boolean,
      'backerSeenCount' : bigint,
    }
  >,
  /**
   * / Issue (or return existing) short referral code for the caller.
   * / Deterministic on the principal + time-derived nonce; retries on the
   * / astronomically-unlikely collision. Codes are stable once assigned.
   */
  'getOrCreateReferralCode' : ActorMethod<[], string>,
  'getPpBurnedFor' : ActorMethod<[Principal], bigint>,
  'getRecentShenanigans' : ActorMethod<[], Array<ShenaniganRecord>>,
  /**
   * / Per-tier downline counts and cumulative PP earnings for `user`.
   * / Counts are computed by a single pass over the referral chain map;
   * / earnings come from the local accumulator.
   */
  'getReferralStats' : ActorMethod<[Principal], ReferralStats>,
  /**
   * / One-hop lookup — returns the user's immediate referrer (L1) or null.
   */
  'getReferrer' : ActorMethod<[Principal], [] | [Principal]>,
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
  /**
   * / Idempotent referral registration. First call sets the chain entry;
   * / subsequent calls for the same caller are no-ops. Self-referral rejected.
   */
  'registerReferral' : ActorMethod<[Principal], undefined>,
  'requestCashOut' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'resetShenaniganConfig' : ActorMethod<[bigint], undefined>,
  /**
   * / Look up the principal a short referral code resolves to. Returns null
   * / for unknown codes. Used by the frontend to translate `?ref=<code>`
   * / into the principal we register against the downline chain.
   */
  'resolveReferralCode' : ActorMethod<[string], [] | [Principal]>,
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
  'setBackerPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setCashOutDelaySeconds' : ActorMethod<[bigint], undefined>,
  'setCompounding15DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setCompounding30DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setMinDepositPp' : ActorMethod<[bigint], undefined>,
  'setObserverIntervalSeconds' : ActorMethod<[bigint], undefined>,
  'setReferralBps' : ActorMethod<[bigint, bigint, bigint], undefined>,
  'setSimple21DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'stopObserver' : ActorMethod<[], undefined>,
  'updateShenaniganConfig' : ActorMethod<[ShenaniganConfig], undefined>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
