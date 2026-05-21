export const idlFactory = ({ IDL }) => {
  const SpellTally = IDL.Record({
    'failures' : IDL.Nat,
    'successes' : IDL.Nat,
    'backfires' : IDL.Nat,
    'totalCast' : IDL.Nat,
    'totalCostPaidUnits' : IDL.Nat,
  });
  const ShenaniganType = IDL.Variant({
    'ppBoosterAura' : IDL.Null,
    'goldenName' : IDL.Null,
    'whaleRebalance' : IDL.Null,
    'downlineBoost' : IDL.Null,
    'moneyTrickster' : IDL.Null,
    'mintTaxSiphon' : IDL.Null,
    'aoeSkim' : IDL.Null,
    'magicMirror' : IDL.Null,
    'downlineHeist' : IDL.Null,
    'renameSpell' : IDL.Null,
    'purseCutter' : IDL.Null,
  });
  const ShenaniganOutcome = IDL.Variant({
    'backfire' : IDL.Null,
    'fail' : IDL.Null,
    'success' : IDL.Null,
  });
  const ShenaniganOutcomeDetail = IDL.Record({
    'affectedTarget' : IDL.Opt(IDL.Principal),
    'affectedCount' : IDL.Nat,
    'outcome' : ShenaniganOutcome,
    'ppDeltaCaster' : IDL.Int,
  });
  const MintMultiplier = IDL.Record({
    'expiresAt' : IDL.Int,
    'multiplierBps' : IDL.Nat,
  });
  const ShieldState = IDL.Record({
    'expiresAt' : IDL.Int,
    'chargesRemaining' : IDL.Nat,
  });
  const DisplayNameOverride = IDL.Record({
    'expiresAt' : IDL.Int,
    'name' : IDL.Text,
  });
  const MintSiphon = IDL.Record({
    'expiresAt' : IDL.Int,
    'pctTimes100' : IDL.Nat,
    'siphonedSoFar' : IDL.Nat,
    'siphoner' : IDL.Principal,
    'capUnits' : IDL.Nat,
  });
  const CascadeBoost = IDL.Record({
    'expiresAt' : IDL.Int,
    'multiplierBps' : IDL.Nat,
  });
  const ActiveSpellEffects = IDL.Record({
    'mintMultiplier' : IDL.Opt(MintMultiplier),
    'shield' : IDL.Opt(ShieldState),
    'displayName' : IDL.Opt(DisplayNameOverride),
    'golden' : IDL.Bool,
    'mintSiphon' : IDL.Opt(MintSiphon),
    'cascadeBoost' : IDL.Opt(CascadeBoost),
  });
  const CashOutEntry = IDL.Record({
    'id' : IDL.Nat,
    'cancelled' : IDL.Bool,
    'player' : IDL.Principal,
    'claimed' : IDL.Bool,
    'claimableAfter' : IDL.Int,
    'amount' : IDL.Nat,
  });
  const ChimeSound = IDL.Record({
    'name' : IDL.Text,
    'mimeType' : IDL.Text,
    'bytes' : IDL.Vec(IDL.Nat8),
    'uploadedAt' : IDL.Int,
  });
  const ChatItemKind = IDL.Variant({
    'roundResult' : IDL.Record({
      'winnerPpUnits' : IDL.Nat,
      'gameId' : IDL.Nat,
      'winner' : IDL.Principal,
    }),
    'pinUpdate' : IDL.Record({ 'body' : IDL.Text }),
    'userMessage' : IDL.Record({
      'body' : IDL.Text,
      'replyTo' : IDL.Opt(IDL.Nat),
    }),
    'signup' : IDL.Record({ 'newUser' : IDL.Principal }),
    'rankUp' : IDL.Record({ 'user' : IDL.Principal, 'newRank' : IDL.Text }),
    'spellCast' : IDL.Record({ 'castId' : IDL.Nat }),
    'reginald' : IDL.Record({ 'line' : IDL.Text, 'triggerKind' : IDL.Text }),
  });
  const Reaction = IDL.Record({
    'karmaPpBurned' : IDL.Nat,
    'emoji' : IDL.Text,
    'reactors' : IDL.Vec(IDL.Principal),
  });
  const ChatItem = IDL.Record({
    'id' : IDL.Nat,
    'deleted' : IDL.Bool,
    'kind' : ChatItemKind,
    'author' : IDL.Principal,
    'timestamp' : IDL.Int,
    'reactions' : IDL.Vec(Reaction),
  });
  const MintConfig = IDL.Record({
    'compounding15DayPpPerIcp' : IDL.Nat,
    'minDepositPp' : IDL.Nat,
    'cascadeInitialBps' : IDL.Nat,
    'compounding30DayPpPerIcp' : IDL.Nat,
    'referralL1Bps' : IDL.Nat,
    'referralL2Bps' : IDL.Nat,
    'referralL3Bps' : IDL.Nat,
    'observerIntervalSeconds' : IDL.Nat,
    'backerPpPerIcp' : IDL.Nat,
    'cashOutDelaySeconds' : IDL.Nat,
    'activityWindowDays' : IDL.Opt(IDL.Nat),
    'activityRequiresDeposit' : IDL.Bool,
    'signupGiftPp' : IDL.Nat,
    'simple21DayPpPerIcp' : IDL.Nat,
    'cascadePassthroughBps' : IDL.Nat,
  });
  const ShenaniganRecord = IDL.Record({
    'id' : IDL.Nat,
    'shenaniganType' : ShenaniganType,
    'cost' : IDL.Float64,
    'user' : IDL.Principal,
    'target' : IDL.Opt(IDL.Principal),
    'timestamp' : IDL.Int,
    'outcome' : ShenaniganOutcome,
  });
  const SignupEntry = IDL.Record({
    'principal' : IDL.Principal,
    'joinedAt' : IDL.Int,
    'level' : IDL.Nat,
  });
  const ReferralStats = IDL.Record({
    'l1Count' : IDL.Nat,
    'l3Units' : IDL.Nat,
    'recentSignups' : IDL.Vec(SignupEntry),
    'l1Units' : IDL.Nat,
    'l2Count' : IDL.Nat,
    'l2Units' : IDL.Nat,
    'l3Count' : IDL.Nat,
  });
  const ShenaniganConfig = IDL.Record({
    'id' : IDL.Nat,
    'backgroundColor' : IDL.Text,
    'duration' : IDL.Nat,
    'costBackfire' : IDL.Float64,
    'successOdds' : IDL.Nat,
    'name' : IDL.Text,
    'backfireOdds' : IDL.Nat,
    'castLimit' : IDL.Nat,
    'description' : IDL.Text,
    'costSuccess' : IDL.Float64,
    'effectValues' : IDL.Vec(IDL.Float64),
    'costFailure' : IDL.Float64,
    'failureOdds' : IDL.Nat,
    'cooldown' : IDL.Nat,
  });
  const ShenaniganStats = IDL.Record({
    'backfires' : IDL.Nat,
    'dealerCut' : IDL.Float64,
    'totalCast' : IDL.Nat,
    'goodOutcomes' : IDL.Nat,
    'totalSpent' : IDL.Float64,
    'badOutcomes' : IDL.Nat,
  });
  const StandardRecord = IDL.Record({ 'url' : IDL.Text, 'name' : IDL.Text });
  const ConsentMessageMetadata = IDL.Record({
    'utc_offset_minutes' : IDL.Opt(IDL.Int16),
    'language' : IDL.Text,
  });
  const DeviceSpec = IDL.Variant({
    'GenericDisplay' : IDL.Null,
    'LineDisplay' : IDL.Record({
      'characters_per_line' : IDL.Nat16,
      'lines_per_page' : IDL.Nat16,
    }),
  });
  const ConsentMessageSpec = IDL.Record({
    'metadata' : ConsentMessageMetadata,
    'device_spec' : IDL.Opt(DeviceSpec),
  });
  const ConsentMessageRequest = IDL.Record({
    'arg' : IDL.Vec(IDL.Nat8),
    'method' : IDL.Text,
    'user_preferences' : ConsentMessageSpec,
  });
  const LineDisplayPage = IDL.Record({ 'lines' : IDL.Vec(IDL.Text) });
  const ConsentMessage = IDL.Variant({
    'LineDisplayMessage' : IDL.Record({ 'pages' : IDL.Vec(LineDisplayPage) }),
    'GenericDisplayMessage' : IDL.Text,
  });
  const ConsentInfo = IDL.Record({
    'metadata' : ConsentMessageMetadata,
    'consent_message' : ConsentMessage,
  });
  const Icrc21Error = IDL.Variant({
    'GenericError' : IDL.Record({
      'description' : IDL.Text,
      'error_code' : IDL.Nat,
    }),
    'UnsupportedCanisterCall' : IDL.Record({ 'description' : IDL.Text }),
    'ConsentMessageUnavailable' : IDL.Record({ 'description' : IDL.Text }),
  });
  const ConsentMessageResponse = IDL.Variant({
    'Ok' : ConsentInfo,
    'Err' : Icrc21Error,
  });
  const TrustedOriginsResponse = IDL.Record({
    'trusted_origins' : IDL.Vec(IDL.Text),
  });
  const ChimeSoundMeta = IDL.Record({
    'name' : IDL.Text,
    'mimeType' : IDL.Text,
    'sizeBytes' : IDL.Nat,
    'uploadedAt' : IDL.Int,
  });
  return IDL.Service({
    'addKarmaReaction' : IDL.Func(
        [IDL.Nat, IDL.Text, IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'addReaction' : IDL.Func(
        [IDL.Nat, IDL.Text],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'adminBackfillSpellTallies' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Nat, SpellTally))],
        [],
      ),
    'adminClearFlavorPool' : IDL.Func([IDL.Text], [], []),
    'adminClearReferrer' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(IDL.Principal)],
        [],
      ),
    'adminDeleteChatItem' : IDL.Func([IDL.Nat], [], []),
    'adminDeleteChimeSound' : IDL.Func([IDL.Text], [], []),
    'adminMint' : IDL.Func(
        [IDL.Principal, IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'adminMuteUser' : IDL.Func([IDL.Principal, IDL.Nat], [], []),
    'adminPostAsReginald' : IDL.Func([IDL.Text], [IDL.Nat], []),
    'adminSeedRankCache' : IDL.Func([], [IDL.Nat], []),
    'adminSeedSignupAnnounced' : IDL.Func([], [IDL.Nat], []),
    'adminSetFlavorPool' : IDL.Func([IDL.Text, IDL.Vec(IDL.Text)], [], []),
    'adminSetPin' : IDL.Func([IDL.Text], [IDL.Nat], []),
    'adminUnmute' : IDL.Func([IDL.Principal], [], []),
    'adminUploadChimeSound' : IDL.Func(
        [IDL.Text, IDL.Text, IDL.Vec(IDL.Nat8)],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'cancelCashOut' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'cancelPendingRename' : IDL.Func([], [], []),
    'castShenanigan' : IDL.Func(
        [ShenaniganType, IDL.Opt(IDL.Principal)],
        [ShenaniganOutcomeDetail],
        [],
      ),
    'claimCashOut' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'clearMissedBackerMint' : IDL.Func([IDL.Principal], [], []),
    'clearMissedGameMint' : IDL.Func([IDL.Nat], [], []),
    'depositChips' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'getActiveShield' : IDL.Func(
        [IDL.Principal],
        [
          IDL.Opt(
            IDL.Record({ 'expiresAt' : IDL.Int, 'chargesRemaining' : IDL.Nat })
          ),
        ],
        ['query'],
      ),
    'getActiveSpellEffects' : IDL.Func(
        [IDL.Principal],
        [ActiveSpellEffects],
        ['query'],
      ),
    'getCashOutsFor' : IDL.Func(
        [IDL.Principal],
        [IDL.Vec(CashOutEntry)],
        ['query'],
      ),
    'getChimeSound' : IDL.Func([IDL.Text], [IDL.Opt(ChimeSound)], ['query']),
    'getCurrentPin' : IDL.Func([], [IDL.Opt(ChatItem)], ['query']),
    'getCustomDisplayName' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(IDL.Text)],
        ['query'],
      ),
    'getFlavorPoolDefaults' : IDL.Func(
        [IDL.Text],
        [IDL.Vec(IDL.Text)],
        ['query'],
      ),
    'getGoldenPlayers' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'getKarmaReceived' : IDL.Func([IDL.Principal], [IDL.Nat], ['query']),
    'getKnownPpHolders' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'getMintConfig' : IDL.Func([], [MintConfig], ['query']),
    'getMissedBackerMints' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Text))],
        ['query'],
      ),
    'getMissedGameMints' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Nat, IDL.Text))],
        ['query'],
      ),
    'getMyCashOuts' : IDL.Func([], [IDL.Vec(CashOutEntry)], ['query']),
    'getObserverStatus' : IDL.Func(
        [],
        [
          IDL.Record({
            'missedBackerMintsCount' : IDL.Nat,
            'gameIdCursor' : IDL.Nat,
            'missedGameMintsCount' : IDL.Nat,
            'intervalSeconds' : IDL.Nat,
            'running' : IDL.Bool,
            'backerSeenCount' : IDL.Nat,
          }),
        ],
        ['query'],
      ),
    'getOrCreateReferralCode' : IDL.Func([], [IDL.Text], []),
    'getPendingRenameForCaller' : IDL.Func(
        [],
        [
          IDL.Opt(
            IDL.Record({ 'expiresAt' : IDL.Int, 'target' : IDL.Principal })
          ),
        ],
        ['query'],
      ),
    'getPpBurnedFor' : IDL.Func([IDL.Principal], [IDL.Nat], ['query']),
    'getRecentChatItems' : IDL.Func([IDL.Nat], [IDL.Vec(ChatItem)], ['query']),
    'getRecentShenanigans' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganRecord)],
        ['query'],
      ),
    'getReferralStats' : IDL.Func([IDL.Principal], [ReferralStats], ['query']),
    'getReferrer' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(IDL.Principal)],
        ['query'],
      ),
    'getShenaniganConfigs' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganConfig)],
        ['query'],
      ),
    'getShenaniganStats' : IDL.Func([], [ShenaniganStats], ['query']),
    'getSpellCooldowns' : IDL.Func(
        [IDL.Principal],
        [IDL.Vec(IDL.Tuple(IDL.Nat, IDL.Int))],
        ['query'],
      ),
    'getSpellTallies' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Nat, SpellTally))],
        ['query'],
      ),
    'getTopPpBurners' : IDL.Func(
        [IDL.Nat],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Nat))],
        ['query'],
      ),
    'getTopSpellCasters' : IDL.Func(
        [IDL.Nat],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Nat))],
        ['query'],
      ),
    'icrc10_supported_standards' : IDL.Func(
        [],
        [IDL.Vec(StandardRecord)],
        ['query'],
      ),
    'icrc21_canister_call_consent_message' : IDL.Func(
        [ConsentMessageRequest],
        [ConsentMessageResponse],
        [],
      ),
    'icrc28_trusted_origins' : IDL.Func(
        [],
        [TrustedOriginsResponse],
        ['query'],
      ),
    'initialize' : IDL.Func([IDL.Principal], [], []),
    'isBootstrapped' : IDL.Func([], [IDL.Bool], ['query']),
    'isMuted' : IDL.Func([IDL.Principal], [IDL.Opt(IDL.Int)], ['query']),
    'listChimeSounds' : IDL.Func([], [IDL.Vec(ChimeSoundMeta)], ['query']),
    'listFlavorPools' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Text, IDL.Vec(IDL.Text)))],
        ['query'],
      ),
    'postChatMessage' : IDL.Func(
        [IDL.Text, IDL.Opt(IDL.Nat)],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'primeObserverCursors' : IDL.Func([], [], []),
    'registerReferral' : IDL.Func([IDL.Principal], [], []),
    'removeReaction' : IDL.Func(
        [IDL.Nat, IDL.Text],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'requestCashOut' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'resetShenaniganConfig' : IDL.Func([IDL.Nat], [], []),
    'resolveReferralCode' : IDL.Func(
        [IDL.Text],
        [IDL.Opt(IDL.Principal)],
        ['query'],
      ),
    'resumeObserver' : IDL.Func([], [], []),
    'rotateAdmin' : IDL.Func([IDL.Principal], [], []),
    'runObserverOnce' : IDL.Func([], [], []),
    'saveAllShenaniganConfigs' : IDL.Func([IDL.Vec(ShenaniganConfig)], [], []),
    'seedMigrationV2' : IDL.Func([], [], []),
    'setActivityRequiresDeposit' : IDL.Func([IDL.Bool], [], []),
    'setActivityWindowDays' : IDL.Func([IDL.Opt(IDL.Nat)], [], []),
    'setBackerPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setCascadeBps' : IDL.Func([IDL.Nat, IDL.Nat], [], []),
    'setCashOutDelaySeconds' : IDL.Func([IDL.Nat], [], []),
    'setCompounding15DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setCompounding30DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setHousePrincipal' : IDL.Func([IDL.Principal], [], []),
    'setMinDepositPp' : IDL.Func([IDL.Nat], [], []),
    'setObserverIntervalSeconds' : IDL.Func([IDL.Nat], [], []),
    'setPendingRenameName' : IDL.Func(
        [IDL.Text],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'setReferralBps' : IDL.Func([IDL.Nat, IDL.Nat, IDL.Nat], [], []),
    'setSignupGiftPp' : IDL.Func([IDL.Nat], [], []),
    'setSimple21DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'stopObserver' : IDL.Func([], [], []),
    'updateShenaniganConfig' : IDL.Func([ShenaniganConfig], [], []),
  });
};
export const init = ({ IDL }) => { return []; };
