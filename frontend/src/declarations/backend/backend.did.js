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
    'totalWithdrawn' : IDL.Float64,
    'amount' : IDL.Float64,
  });
  const UserProfile = IDL.Record({ 'name' : IDL.Text });
  const CoverChargeEntry = IDL.Record({
    'id' : IDL.Nat,
    'player' : IDL.Principal,
    'gameId' : IDL.Nat,
    'timestamp' : IDL.Int,
    'amount' : IDL.Nat,
  });
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
  return IDL.Service({
    'addDealerMoney' : IDL.Func(
        [IDL.Float64],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'assignCallerUserRole' : IDL.Func([IDL.Principal, UserRole], [], []),
    'burnPonziPoints' : IDL.Func([IDL.Principal, IDL.Float64], [], []),
    'calculateCompounded30DayEarnings' : IDL.Func(
        [GameRecord],
        [IDL.Float64],
        ['query'],
      ),
    'calculateCompoundedEarnings' : IDL.Func(
        [GameRecord],
        [IDL.Float64],
        ['query'],
      ),
    'calculateCompoundedROI' : IDL.Func([], [IDL.Float64], ['query']),
    'calculateEarnings' : IDL.Func([GameRecord], [IDL.Float64], ['query']),
    'checkDepositRateLimit' : IDL.Func([], [IDL.Bool], ['query']),
    'claimDealerRepayment' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
    'createGame' : IDL.Func(
        [GamePlan, IDL.Float64, IDL.Bool, IDL.Opt(IDL.Principal)],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'deductPonziPoints' : IDL.Func([IDL.Principal, IDL.Float64], [], []),
    'distributeDealerCutFromShenanigans' : IDL.Func([IDL.Float64], [], []),
    'distributeFees' : IDL.Func([IDL.Float64], [], []),
    'getActiveGameCount' : IDL.Func([], [IDL.Nat], ['query']),
    'getAllActiveGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAllGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAvailableBalance' : IDL.Func([], [IDL.Float64], ['query']),
    'getCallerUserProfile' : IDL.Func([], [IDL.Opt(UserProfile)], ['query']),
    'getCallerUserRole' : IDL.Func([], [UserRole], ['query']),
    'getCanisterICPBalance' : IDL.Func([], [IDL.Nat], []),
    'getCanisterPrincipal' : IDL.Func([], [IDL.Opt(IDL.Principal)], ['query']),
    'getCoverChargeBalance' : IDL.Func([], [IDL.Nat], ['query']),
    'getCoverChargeTransactions' : IDL.Func(
        [],
        [IDL.Vec(CoverChargeEntry)],
        ['query'],
      ),
    'getDaysActive' : IDL.Func([], [IDL.Nat], ['query']),
    'getAllDealerRepayments' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Float64))],
        ['query'],
      ),
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
    'getPonziPointsBalanceFor' : IDL.Func([IDL.Principal], [IDL.Float64], []),
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
    'whoAmI' : IDL.Func([], [IDL.Principal], ['query']),
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
    'initializeAccessControl' : IDL.Func([], [], []),
    'isCallerAdmin' : IDL.Func([], [IDL.Bool], ['query']),
    'isTestMode' : IDL.Func([], [IDL.Bool], ['query']),
    'saveCallerUserProfile' : IDL.Func([UserProfile], [], []),
    'seedGame' : IDL.Func(
        [IDL.Principal, GamePlan, IDL.Float64, IDL.Bool, IDL.Int],
        [IDL.Nat],
        [],
      ),
    'seedReferral' : IDL.Func([IDL.Principal, IDL.Principal], [], []),
    'setCanisterPrincipal' : IDL.Func([IDL.Principal], [], []),
    'setShenanigansPrincipal' : IDL.Func([IDL.Principal], [], []),
    'setTestMode' : IDL.Func([IDL.Bool], [], []),
    'settleCompoundingGame' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
    'transferPonziPoints' : IDL.Func(
        [IDL.Principal, IDL.Principal, IDL.Float64],
        [],
        [],
      ),
    'withdrawCoverCharges' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'withdrawEarnings' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
  });
};
export const init = ({ IDL }) => { return []; };
