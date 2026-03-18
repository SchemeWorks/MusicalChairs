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
  const ShenaniganRecord = IDL.Record({
    'id' : IDL.Nat,
    'shenaniganType' : ShenaniganType,
    'cost' : IDL.Float64,
    'user' : IDL.Principal,
    'target' : IDL.Opt(IDL.Principal),
    'timestamp' : IDL.Int,
    'outcome' : ShenaniganOutcome,
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
    'castShenanigan' : IDL.Func(
        [ShenaniganType, IDL.Opt(IDL.Principal)],
        [ShenaniganOutcome],
        [],
      ),
    'getRecentShenanigans' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganRecord)],
        ['query'],
      ),
    'getShenaniganConfigs' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganConfig)],
        ['query'],
      ),
    'getShenaniganStats' : IDL.Func([], [ShenaniganStats], ['query']),
    'initialize' : IDL.Func([IDL.Principal], [], []),
    'resetShenaniganConfig' : IDL.Func([IDL.Nat], [], []),
    'saveAllShenaniganConfigs' : IDL.Func([IDL.Vec(ShenaniganConfig)], [], []),
    'updateShenaniganConfig' : IDL.Func([ShenaniganConfig], [], []),
  });
};
export const init = ({ IDL }) => { return []; };
