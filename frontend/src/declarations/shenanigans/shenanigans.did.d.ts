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
export interface ChatItem {
  'id' : bigint,
  'deleted' : boolean,
  'kind' : ChatItemKind,
  'author' : Principal,
  'timestamp' : bigint,
  'reactions' : Array<Reaction>,
}
export type ChatItemKind = {
    'roundResult' : {
      'winnerPpUnits' : bigint,
      'gameId' : bigint,
      'winner' : Principal,
    }
  } |
  { 'pinUpdate' : { 'body' : string } } |
  { 'userMessage' : { 'body' : string, 'replyTo' : [] | [bigint] } } |
  { 'signup' : { 'newUser' : Principal } } |
  { 'rankUp' : { 'user' : Principal, 'newRank' : string } } |
  { 'spellCast' : { 'castId' : bigint } } |
  { 'reginald' : { 'line' : string, 'triggerKind' : string } };
export interface ChimeSound {
  'name' : string,
  'mimeType' : string,
  'bytes' : Uint8Array | number[],
  'uploadedAt' : bigint,
}
export interface ChimeSoundMeta {
  'name' : string,
  'mimeType' : string,
  'sizeBytes' : bigint,
  'uploadedAt' : bigint,
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
export type DeviceSpec = { 'GenericDisplay' : null } |
  {
    'LineDisplay' : {
      'characters_per_line' : number,
      'lines_per_page' : number,
    }
  };
export interface DisplayNameOverride { 'expiresAt' : bigint, 'name' : string }
export type Icrc21Error = {
    'GenericError' : { 'description' : string, 'error_code' : bigint }
  } |
  { 'UnsupportedCanisterCall' : { 'description' : string } } |
  { 'ConsentMessageUnavailable' : { 'description' : string } };
export interface LineDisplayPage { 'lines' : Array<string> }
export interface MintConfig {
  'compounding15DayPpPerIcp' : bigint,
  'minDepositPp' : bigint,
  'cascadeInitialBps' : bigint,
  'compounding30DayPpPerIcp' : bigint,
  'referralL1Bps' : bigint,
  'referralL2Bps' : bigint,
  'referralL3Bps' : bigint,
  'observerIntervalSeconds' : bigint,
  'backerPpPerIcp' : bigint,
  'cashOutDelaySeconds' : bigint,
  'activityWindowDays' : [] | [bigint],
  'activityRequiresDeposit' : boolean,
  'signupGiftPp' : bigint,
  'simple21DayPpPerIcp' : bigint,
  'cascadePassthroughBps' : bigint,
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
export interface Reaction {
  'karmaPpBurned' : bigint,
  'emoji' : string,
  'reactors' : Array<Principal>,
}
export interface ReferralStats {
  'l1Count' : bigint,
  'l3Units' : bigint,
  'recentSignups' : Array<SignupEntry>,
  'l1Units' : bigint,
  'l2Count' : bigint,
  'l2Units' : bigint,
  'l3Count' : bigint,
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
export interface ShenaniganOutcomeDetail {
  'affectedTarget' : [] | [Principal],
  'affectedCount' : bigint,
  'outcome' : ShenaniganOutcome,
  'ppDeltaCaster' : bigint,
}
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
export interface SignupEntry {
  'principal' : Principal,
  'joinedAt' : bigint,
  'level' : bigint,
}
export interface StandardRecord { 'url' : string, 'name' : string }
export interface TrustedOriginsResponse { 'trusted_origins' : Array<string> }
export interface _SERVICE {
  'addKarmaReaction' : ActorMethod<
    [bigint, string, bigint],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'addReaction' : ActorMethod<
    [bigint, string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Admin-only: remove the override entirely, restoring the hardcoded default.
   */
  'adminClearFlavorPool' : ActorMethod<[string], undefined>,
  /**
   * / Admin-only: scrub `user`'s referralChain entry and the reverse-index
   * / edge. Used to clean up bad mappings created before the
   * / `isAdminPrincipal` guard landed in registerReferral (e.g. Charles
   * / auto-registered against someone else's `?ref=` code). No-op if `user`
   * / has no entry. Returns the principal that was removed, or null.
   */
  'adminClearReferrer' : ActorMethod<[Principal], [] | [Principal]>,
  'adminDeleteChatItem' : ActorMethod<[bigint], undefined>,
  'adminDeleteChimeSound' : ActorMethod<[string], undefined>,
  /**
   * / Admin-triggered manual PP issuance (direct mint to the player's chip
   * / subaccount). Use for fixups, comps, or seeding test accounts.
   */
  'adminMint' : ActorMethod<
    [Principal, bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'adminMuteUser' : ActorMethod<[Principal, bigint], undefined>,
  'adminPostAsReginald' : ActorMethod<[string], bigint>,
  /**
   * / One-shot deploy-time backfill: populate previousRankEntries with each
   * / known principal's current rank. Prevents #rankUp spam after the trollbox
   * / deploys. Idempotent — safe to call multiple times. Admin-only.
   */
  'adminSeedRankCache' : ActorMethod<[], bigint>,
  /**
   * / One-shot deploy-time backfill: mark every principal who has ever been
   * / granted a signup gift as already-announced. Prevents #signup spam after
   * / the trollbox deploys. Idempotent. Admin-only.
   */
  'adminSeedSignupAnnounced' : ActorMethod<[], bigint>,
  /**
   * / Admin-only: replace a flavor pool's override. Pass an empty list to
   * / explicitly disable a Reginald trigger or empty the rename pool.
   * / To restore defaults, call adminClearFlavorPool instead.
   */
  'adminSetFlavorPool' : ActorMethod<[string, Array<string>], undefined>,
  'adminSetPin' : ActorMethod<[string], bigint>,
  'adminUnmute' : ActorMethod<[Principal], undefined>,
  'adminUploadChimeSound' : ActorMethod<
    [string, string, Uint8Array | number[]],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  'cancelCashOut' : ActorMethod<[bigint], { 'Ok' : null } | { 'Err' : string }>,
  'castShenanigan' : ActorMethod<
    [ShenaniganType, [] | [Principal]],
    ShenaniganOutcomeDetail
  >,
  'claimCashOut' : ActorMethod<
    [bigint],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'clearMissedBackerMint' : ActorMethod<[Principal], undefined>,
  /**
   * / Dismiss a missed game-mint entry. Use after manually compensating
   * / the player via adminMint, so the missed-mints list stays clean.
   */
  'clearMissedGameMint' : ActorMethod<[bigint], undefined>,
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
   * / Read the caller's (or any principal's) active Magic Mirror shield, if any.
   * / Returns null when no shield is active or it has expired.
   */
  'getActiveShield' : ActorMethod<
    [Principal],
    [] | [{ 'expiresAt' : bigint, 'chargesRemaining' : bigint }]
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
  'getChimeSound' : ActorMethod<[string], [] | [ChimeSound]>,
  'getCurrentPin' : ActorMethod<[], [] | [ChatItem]>,
  /**
   * / Active rename-spell name for `user`, if any. Expired entries return null.
   */
  'getCustomDisplayName' : ActorMethod<[Principal], [] | [string]>,
  /**
   * / Returns the hardcoded default lines for a known pool name. Useful for
   * / the admin UI to show "this is what defaults look like" without
   * / duplicating the lists in the frontend.
   */
  'getFlavorPoolDefaults' : ActorMethod<[string], Array<string>>,
  /**
   * / Currently-golden players. Used by frontend for leaderboard styling.
   */
  'getGoldenPlayers' : ActorMethod<[], Array<Principal>>,
  'getKarmaReceived' : ActorMethod<[Principal], bigint>,
  /**
   * / All principals we've ever minted PP to. Frontend target-pickers can
   * / use this to populate a candidate list. Updated lazily — entries are
   * / added in mintInternal and never removed (cheap, bounded by player count).
   */
  'getKnownPpHolders' : ActorMethod<[], Array<Principal>>,
  'getMintConfig' : ActorMethod<[], MintConfig>,
  /**
   * / Backer principals whose delta mint was permanently skipped.
   */
  'getMissedBackerMints' : ActorMethod<[], Array<[Principal, string]>>,
  /**
   * / Games the observer permanently gave up on after MAX_MINT_RETRIES failures.
   * / Admin can fixup via adminMint and then clearMissedGameMint to dismiss.
   */
  'getMissedGameMints' : ActorMethod<[], Array<[bigint, string]>>,
  'getMyCashOuts' : ActorMethod<[], Array<CashOutEntry>>,
  /**
   * / Current observer state — running/paused, cursor positions, and interval.
   * / Surfaced in the admin panel for operational visibility.
   */
  'getObserverStatus' : ActorMethod<
    [],
    {
      'missedBackerMintsCount' : bigint,
      'gameIdCursor' : bigint,
      'missedGameMintsCount' : bigint,
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
  /**
   * / Returns the active pending-rename slot for the caller, if any.
   * / Drives the frontend modal that prompts for a name post-success.
   */
  'getPendingRenameForCaller' : ActorMethod<
    [],
    [] | [{ 'expiresAt' : bigint, 'target' : Principal }]
  >,
  'getPpBurnedFor' : ActorMethod<[Principal], bigint>,
  /**
   * / Returns the most-recent chat items newest-first. Capped server-side
   * / at 100 per call regardless of the caller's requested limit.
   */
  'getRecentChatItems' : ActorMethod<[bigint], Array<ChatItem>>,
  'getRecentShenanigans' : ActorMethod<[], Array<ShenaniganRecord>>,
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
  'icrc10_supported_standards' : ActorMethod<[], Array<StandardRecord>>,
  'icrc21_canister_call_consent_message' : ActorMethod<
    [ConsentMessageRequest],
    ConsentMessageResponse
  >,
  'icrc28_trusted_origins' : ActorMethod<[], TrustedOriginsResponse>,
  'initialize' : ActorMethod<[Principal], undefined>,
  /**
   * / Inspect the bootstrap gate. Useful during deploy to confirm that
   * / seedMigrationV2 has flipped the flag before player traffic resumes.
   */
  'isBootstrapped' : ActorMethod<[], boolean>,
  'isMuted' : ActorMethod<[Principal], [] | [bigint]>,
  'listChimeSounds' : ActorMethod<[], Array<ChimeSoundMeta>>,
  'listFlavorPools' : ActorMethod<[], Array<[string, Array<string>]>>,
  'postChatMessage' : ActorMethod<
    [string, [] | [bigint]],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  /**
   * / One-shot catch-up primer. Admin only. Call immediately after the
   * / cutover upgrade completes, before unpausing user traffic.
   */
  'primeObserverCursors' : ActorMethod<[], undefined>,
  /**
   * / Idempotent referral registration. First call sets the chain entry;
   * / subsequent calls for the same caller are no-ops. Self-referral rejected.
   * / Admin principals (adminPrincipal + extraAdmins) are silently rejected:
   * / Charles sits at the top of the chain by design and must never be
   * / registered as someone else's downline, even if a stale `?ref=` survives
   * / in localStorage from earlier testing.
   */
  'registerReferral' : ActorMethod<[Principal], undefined>,
  'removeReaction' : ActorMethod<
    [bigint, string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
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
  /**
   * / One-shot post-upgrade seeding for the deductive-cascade rollout.
   * /
   * / 1. housePrincipal := ?caller if null
   * / 2. For every player with an existing game record: signupGiftClaimed
   * /    [player] := earliest game timestamp (prevents retroactive gifts).
   * / 3. For every player with ≥0.1 ICP cumulative deposit (game or backer):
   * /    lastQualifyingDeposit[player] := Time.now() (conservative: all
   * /    existing depositors are treated as just-qualified).
   * / 4. Backfill referrerToDownline from referralChain.
   * /
   * / Idempotent: re-running produces the same end state. Admin-only.
   */
  'seedMigrationV2' : ActorMethod<[], undefined>,
  'setActivityRequiresDeposit' : ActorMethod<[boolean], undefined>,
  'setActivityWindowDays' : ActorMethod<[[] | [bigint]], undefined>,
  'setBackerPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setCascadeBps' : ActorMethod<[bigint, bigint], undefined>,
  'setCashOutDelaySeconds' : ActorMethod<[bigint], undefined>,
  'setCompounding15DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setCompounding30DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'setHousePrincipal' : ActorMethod<[Principal], undefined>,
  'setMinDepositPp' : ActorMethod<[bigint], undefined>,
  'setObserverIntervalSeconds' : ActorMethod<[bigint], undefined>,
  /**
   * / Caller commits a chosen name for their most recent successful Rename
   * / Spell. Must be called within 5 minutes of the cast. Name is sanitized:
   * / trimmed, 1-32 chars, alphanumeric + space + dash + underscore only.
   */
  'setPendingRenameName' : ActorMethod<
    [string],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
  /**
   * / Deprecated. The deductive cascade ignores referralL[1-3]Bps.
   * / Use setCascadeBps(initial, passthrough) instead.
   */
  'setReferralBps' : ActorMethod<[bigint, bigint, bigint], undefined>,
  'setSignupGiftPp' : ActorMethod<[bigint], undefined>,
  'setSimple21DayPpPerIcp' : ActorMethod<[bigint], undefined>,
  'stopObserver' : ActorMethod<[], undefined>,
  'updateShenaniganConfig' : ActorMethod<[ShenaniganConfig], undefined>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
