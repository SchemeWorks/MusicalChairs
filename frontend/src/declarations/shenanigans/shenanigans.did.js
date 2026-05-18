export const idlFactory = ({ IDL }) => {
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
  const MintConfig = IDL.Record({
    'compounding15DayPpPerIcp' : IDL.Nat,
    'minDepositPp' : IDL.Nat,
    'compounding30DayPpPerIcp' : IDL.Nat,
    'referralL1Bps' : IDL.Nat,
    'referralL2Bps' : IDL.Nat,
    'referralL3Bps' : IDL.Nat,
    'observerIntervalSeconds' : IDL.Nat,
    'backerPpPerIcp' : IDL.Nat,
    'cashOutDelaySeconds' : IDL.Nat,
    'simple21DayPpPerIcp' : IDL.Nat,
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
    'l1Units' : IDL.Nat,
    'l2Count' : IDL.Nat,
    'l2Units' : IDL.Nat,
    'l3Count' : IDL.Nat,
    'recentSignups' : IDL.Vec(SignupEntry),
  });
  const ShenaniganConfig = IDL.Record({
    'id' : IDL.Nat,
    'backgroundColor' : IDL.Text,
    'duration' : IDL.Nat,
    'cost' : IDL.Float64,
    'successOdds' : IDL.Nat,
    'name' : IDL.Text,
    'backfireOdds' : IDL.Nat,
    'castLimit' : IDL.Nat,
    'description' : IDL.Text,
    'effectValues' : IDL.Vec(IDL.Float64),
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
  return IDL.Service({
    'adminMint' : IDL.Func(
        [IDL.Principal, IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'cancelCashOut' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'castShenanigan' : IDL.Func(
        [ShenaniganType, IDL.Opt(IDL.Principal)],
        [ShenaniganOutcome],
        [],
      ),
    'claimCashOut' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'depositChips' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
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
    'getCustomDisplayName' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(IDL.Text)],
        ['query'],
      ),
    'getGoldenPlayers' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'getKnownPpHolders' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'getMintConfig' : IDL.Func([], [MintConfig], ['query']),
    'getMyCashOuts' : IDL.Func([], [IDL.Vec(CashOutEntry)], ['query']),
    'getObserverStatus' : IDL.Func(
        [],
        [
          IDL.Record({
            'gameIdCursor' : IDL.Nat,
            'intervalSeconds' : IDL.Nat,
            'running' : IDL.Bool,
            'backerSeenCount' : IDL.Nat,
          }),
        ],
        ['query'],
      ),
    'getOrCreateReferralCode' : IDL.Func([], [IDL.Text], []),
    'getPpBurnedFor' : IDL.Func([IDL.Principal], [IDL.Nat], ['query']),
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
    'initialize' : IDL.Func([IDL.Principal], [], []),
    'primeObserverCursors' : IDL.Func([], [], []),
    'registerReferral' : IDL.Func([IDL.Principal], [], []),
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
    'setBackerPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setCashOutDelaySeconds' : IDL.Func([IDL.Nat], [], []),
    'setCompounding15DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setCompounding30DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'setMinDepositPp' : IDL.Func([IDL.Nat], [], []),
    'setObserverIntervalSeconds' : IDL.Func([IDL.Nat], [], []),
    'setReferralBps' : IDL.Func([IDL.Nat, IDL.Nat, IDL.Nat], [], []),
    'setSimple21DayPpPerIcp' : IDL.Func([IDL.Nat], [], []),
    'stopObserver' : IDL.Func([], [], []),
    'updateShenaniganConfig' : IDL.Func([ShenaniganConfig], [], []),
  });
};
export const init = ({ IDL }) => { return []; };
