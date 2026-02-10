export const idlFactory = ({ IDL }) => {
  const UserRole = IDL.Variant({
    'admin' : IDL.Null,
    'user' : IDL.Null,
    'guest' : IDL.Null,
  });
  const GamePlan = IDL.Variant({
    'compounding15Day' : IDL.Null,
    'simple21Day' : IDL.Null,
    'compounding30Day' : IDL.Null,
  });
  const GameRecord = IDL.Record({
    'id' : IDL.Nat,
    'startTime' : IDL.Int,
    'player' : IDL.Principal,
    'plan' : GamePlan,
    'isActive' : IDL.Bool,
    'accumulatedEarnings' : IDL.Float64,
    'lastUpdateTime' : IDL.Int,
    'isCompounding' : IDL.Bool,
    'amount' : IDL.Float64,
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
  const UserProfile = IDL.Record({ 'name' : IDL.Text });
  const DealerType = IDL.Variant({
    'downstream' : IDL.Null,
    'upstream' : IDL.Null,
  });
  const DealerPosition = IDL.Record({
    'startTime' : IDL.Int,
    'firstDepositDate' : IDL.Opt(IDL.Int),
    'owner' : IDL.Principal,
    'name' : IDL.Text,
    'isActive' : IDL.Bool,
    'dealerType' : DealerType,
    'amount' : IDL.Float64,
    'entitlement' : IDL.Float64,
  });
  const GameResetRecord = IDL.Record({
    'resetTime' : IDL.Int,
    'reason' : IDL.Text,
  });
  const HouseLedgerRecord = IDL.Record({
    'id' : IDL.Nat,
    'description' : IDL.Text,
    'timestamp' : IDL.Int,
    'amount' : IDL.Float64,
  });
  const PlatformStats = IDL.Record({
    'daysActive' : IDL.Nat,
    'potBalance' : IDL.Float64,
    'totalWithdrawals' : IDL.Float64,
    'activeGames' : IDL.Nat,
    'totalDeposits' : IDL.Float64,
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
  const WalletTransaction = IDL.Record({
    'id' : IDL.Nat,
    'user' : IDL.Principal,
    'description' : IDL.Text,
    'timestamp' : IDL.Int,
    'txType' : IDL.Variant({
      'gameWithdrawal' : IDL.Null,
      'gameDeposit' : IDL.Null,
      'deposit' : IDL.Null,
      'withdrawal' : IDL.Null,
      'transfer' : IDL.Null,
    }),
    'ledgerBlockIndex' : IDL.Opt(IDL.Nat),
    'amount' : IDL.Nat,
  });
  return IDL.Service({
    'addDealerMoney' : IDL.Func([IDL.Float64], [], []),
    'addDownstreamDealer' : IDL.Func([IDL.Float64, IDL.Float64], [], []),
    'addHouseMoney' : IDL.Func([IDL.Float64, IDL.Text], [], []),
    'assignCallerUserRole' : IDL.Func([IDL.Principal, UserRole], [], []),
    'calculateCompoundedEarnings' : IDL.Func(
        [GameRecord],
        [IDL.Float64],
        ['query'],
      ),
    'calculateCompoundedROI' : IDL.Func([], [IDL.Float64], ['query']),
    'calculateEarnings' : IDL.Func([GameRecord], [IDL.Float64], ['query']),
    'castShenanigan' : IDL.Func(
        [ShenaniganType, IDL.Opt(IDL.Principal)],
        [ShenaniganOutcome],
        [],
      ),
    'checkDepositRateLimit' : IDL.Func([], [IDL.Bool], ['query']),
    'createGame' : IDL.Func(
        [GamePlan, IDL.Float64, IDL.Bool, IDL.Opt(IDL.Principal)],
        [IDL.Nat],
        [],
      ),
    'depositICP' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'distributeFees' : IDL.Func([IDL.Float64], [], []),
    'getActiveGameCount' : IDL.Func([], [IDL.Nat], ['query']),
    'getAllActiveGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAllGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAvailableBalance' : IDL.Func([], [IDL.Float64], ['query']),
    'getCallerUserProfile' : IDL.Func([], [IDL.Opt(UserProfile)], ['query']),
    'getCallerUserRole' : IDL.Func([], [UserRole], ['query']),
    'getCanisterICPBalance' : IDL.Func([], [IDL.Nat], []),
    'getCanisterPrincipal' : IDL.Func([], [IDL.Opt(IDL.Principal)], ['query']),
    'getDaysActive' : IDL.Func([], [IDL.Nat], ['query']),
    'getDealerPositions' : IDL.Func([], [IDL.Vec(DealerPosition)], ['query']),
    'getDealerRepaymentBalance' : IDL.Func([], [IDL.Float64], ['query']),
    'getGameById' : IDL.Func([IDL.Nat], [IDL.Opt(GameRecord)], ['query']),
    'getGameResetHistory' : IDL.Func([], [IDL.Vec(GameResetRecord)], ['query']),
    'getHouseLedger' : IDL.Func([], [IDL.Vec(HouseLedgerRecord)], ['query']),
    'getHouseLedgerStats' : IDL.Func(
        [],
        [
          IDL.Record({
            'totalWithdrawals' : IDL.Float64,
            'netBalance' : IDL.Float64,
            'totalDeposits' : IDL.Float64,
            'recordCount' : IDL.Nat,
          }),
        ],
        ['query'],
      ),
    'getMaxDepositLimit' : IDL.Func([], [IDL.Float64], ['query']),
    'getOldestUpstreamDealer' : IDL.Func(
        [],
        [IDL.Opt(DealerPosition)],
        ['query'],
      ),
    'getPlatformStats' : IDL.Func([], [PlatformStats], ['query']),
    'getPonziPoints' : IDL.Func([], [IDL.Float64], ['query']),
    'getPonziPointsBalance' : IDL.Func(
        [],
        [
          IDL.Record({
            'depositPoints' : IDL.Float64,
            'referralPoints' : IDL.Float64,
            'totalPoints' : IDL.Float64,
          }),
        ],
        ['query'],
      ),
    'getRecentShenanigans' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganRecord)],
        ['query'],
      ),
    'getReferralEarnings' : IDL.Func([IDL.Principal], [IDL.Float64], ['query']),
    'getReferralTierPoints' : IDL.Func(
        [],
        [
          IDL.Record({
            'level3Points' : IDL.Float64,
            'level1Points' : IDL.Float64,
            'totalPoints' : IDL.Float64,
            'level2Points' : IDL.Float64,
          }),
        ],
        ['query'],
      ),
    'getShenaniganConfigs' : IDL.Func(
        [],
        [IDL.Vec(ShenaniganConfig)],
        ['query'],
      ),
    'getShenaniganStats' : IDL.Func([], [ShenaniganStats], ['query']),
    'getTopPonziPointsBurners' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Float64))],
        ['query'],
      ),
    'getTopPonziPointsHolders' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Float64))],
        ['query'],
      ),
    'getTotalDealerDebt' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalDeposits' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalHouseMoneyAdded' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalWithdrawals' : IDL.Func([], [IDL.Float64], ['query']),
    'getUserGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getUserProfile' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(UserProfile)],
        ['query'],
      ),
    'getWalletBalance' : IDL.Func([], [IDL.Nat], ['query']),
    'getWalletBalanceICP' : IDL.Func([], [IDL.Float64], ['query']),
    'getWalletTransactions' : IDL.Func(
        [],
        [IDL.Vec(WalletTransaction)],
        ['query'],
      ),
    'initializeAccessControl' : IDL.Func([], [], []),
    'isCallerAdmin' : IDL.Func([], [IDL.Bool], ['query']),
    'isTestMode' : IDL.Func([], [IDL.Bool], ['query']),
    'resetShenaniganConfig' : IDL.Func([IDL.Nat], [], []),
    'saveAllShenaniganConfigs' : IDL.Func([IDL.Vec(ShenaniganConfig)], [], []),
    'saveCallerUserProfile' : IDL.Func([UserProfile], [], []),
    'setCanisterPrincipal' : IDL.Func([IDL.Principal], [], []),
    'setTestMode' : IDL.Func([IDL.Bool], [], []),
    'transferInternal' : IDL.Func(
        [IDL.Principal, IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'updateShenaniganConfig' : IDL.Func([ShenaniganConfig], [], []),
    'withdrawEarnings' : IDL.Func([IDL.Nat], [IDL.Float64], []),
    'withdrawICP' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
  });
};
export const init = ({ IDL }) => { return []; };
