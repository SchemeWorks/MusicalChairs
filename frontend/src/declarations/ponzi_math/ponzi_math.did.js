export const idlFactory = ({ IDL }) => {
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
  const ActivePlanSnapshot = IDL.Record({
    'daysToMaturity' : IDL.Float64,
    'daysElapsed' : IDL.Float64,
    'currentGrossEarnings' : IDL.Float64,
    'game' : GameRecord,
    'currentNetClaimable' : IDL.Float64,
    'isMatured' : IDL.Bool,
    'wouldBeInsolvent' : IDL.Bool,
    'currentExitToll' : IDL.Float64,
  });
  const GeneralLedgerEvent = IDL.Variant({
    'tollDistribution' : IDL.Record({
      'tollAmount' : IDL.Float64,
      'toAllBackers' : IDL.Float64,
      'toSeedReserve' : IDL.Float64,
      'toOtherSeriesA' : IDL.Float64,
      'toOldestSeriesA' : IDL.Float64,
    }),
    'gameReset' : IDL.Record({
      'reason' : IDL.Text,
      'seedReserveCarried' : IDL.Float64,
    }),
    'seriesBPromotion' : IDL.Record({
      'owner' : IDL.Principal,
      'underwater' : IDL.Float64,
      'entitlement' : IDL.Float64,
    }),
    'deposit' : IDL.Record({
      'netToPot' : IDL.Float64,
      'player' : IDL.Principal,
      'plan' : GamePlan,
      'gameId' : IDL.Nat,
      'coverCharge' : IDL.Float64,
      'isCompounding' : IDL.Bool,
      'gross' : IDL.Float64,
    }),
    'coverChargeAccrued' : IDL.Record({
      'player' : IDL.Principal,
      'gameId' : IDL.Nat,
      'amountE8s' : IDL.Nat,
    }),
    'backdatedGameCreated' : IDL.Record({
      'startTime' : IDL.Int,
      'admin' : IDL.Principal,
      'player' : IDL.Principal,
      'gameId' : IDL.Nat,
      'amount' : IDL.Float64,
    }),
    'withdrawal' : IDL.Record({
      'netToPlayer' : IDL.Float64,
      'player' : IDL.Principal,
      'toll' : IDL.Float64,
      'gameId' : IDL.Nat,
      'isInsolvent' : IDL.Bool,
      'potDeduction' : IDL.Float64,
      'grossEarnings' : IDL.Float64,
    }),
    'coverChargeSwept' : IDL.Record({
      'toBackend' : IDL.Principal,
      'blockIndex' : IDL.Nat,
      'amountE8s' : IDL.Nat,
    }),
    'backerDeposit' : IDL.Record({
      'backer' : IDL.Principal,
      'amount' : IDL.Float64,
      'entitlement' : IDL.Float64,
    }),
    'backerRepaymentClaim' : IDL.Record({
      'backer' : IDL.Principal,
      'amount' : IDL.Float64,
    }),
    'settlement' : IDL.Record({
      'netToPlayer' : IDL.Float64,
      'player' : IDL.Principal,
      'toll' : IDL.Float64,
      'gameId' : IDL.Nat,
      'isInsolvent' : IDL.Bool,
      'potDeduction' : IDL.Float64,
      'grossEarnings' : IDL.Float64,
    }),
  });
  const GeneralLedgerEntry = IDL.Record({
    'id' : IDL.Nat,
    'event' : GeneralLedgerEvent,
    'roundId' : IDL.Nat,
    'timestamp' : IDL.Int,
  });
  const RoundSummary = IDL.Record({
    'startTime' : IDL.Int,
    'endTime' : IDL.Opt(IDL.Int),
    'endReason' : IDL.Opt(IDL.Text),
    'roundId' : IDL.Nat,
    'eventCount' : IDL.Nat,
    'seedReserveCarried' : IDL.Float64,
  });
  const BackerType = IDL.Variant({
    'seriesA' : IDL.Null,
    'seriesB' : IDL.Null,
  });
  const BackerKey = IDL.Tuple(IDL.Principal, BackerType);
  const BackerPosition = IDL.Record({
    'startTime' : IDL.Int,
    'firstDepositDate' : IDL.Opt(IDL.Int),
    'owner' : IDL.Principal,
    'backerType' : BackerType,
    'isActive' : IDL.Bool,
    'amount' : IDL.Float64,
    'entitlement' : IDL.Float64,
  });
  const GameResetRecord = IDL.Record({
    'resetTime' : IDL.Int,
    'reason' : IDL.Text,
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
  const CycleManagerMetric = IDL.Record({
    'key' : IDL.Text,
    'count' : IDL.Nat64,
    'value' : IDL.Nat,
    'metric_label' : IDL.Opt(IDL.Text),
  });
  const CycleManagerCyclesStatus = IDL.Record({
    'heap_memory_bytes' : IDL.Opt(IDL.Nat64),
    'balance' : IDL.Nat,
    'low_watermark' : IDL.Nat,
    'stable_memory_bytes' : IDL.Opt(IDL.Nat64),
    'healthy' : IDL.Bool,
    'idle_burn_cycles_per_day' : IDL.Opt(IDL.Nat),
    'freeze_threshold_secs' : IDL.Nat64,
  });
  const PonziMath = IDL.Service({
    'addBackerMoney' : IDL.Func(
        [IDL.Float64],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'cycle_manager_metrics' : IDL.Func(
        [],
        [IDL.Vec(CycleManagerMetric)],
        ['query'],
      ),
    'cycles_status' : IDL.Func([], [CycleManagerCyclesStatus], ['query']),
    'adminClearAllBackerPositions' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'adminForceReset' : IDL.Func(
        [IDL.Text],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'adminGetActivePlansSnapshot' : IDL.Func(
        [],
        [IDL.Vec(ActivePlanSnapshot)],
        ['query'],
      ),
    'adminGetCurrentRoundId' : IDL.Func([], [IDL.Nat], ['query']),
    'adminGetEventsByRound' : IDL.Func(
        [IDL.Nat],
        [IDL.Vec(GeneralLedgerEntry)],
        ['query'],
      ),
    'adminGetEventsForGame' : IDL.Func(
        [IDL.Nat],
        [IDL.Vec(GeneralLedgerEntry)],
        ['query'],
      ),
    'adminGetRoundSummaries' : IDL.Func([], [IDL.Vec(RoundSummary)], ['query']),
    'adminIsAdmin' : IDL.Func([], [IDL.Bool], ['query']),
    'adminMergeBackerPosition' : IDL.Func(
        [IDL.Principal, IDL.Principal],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
    'adminSweepUntracked' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
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
    'claimBackerRepayment' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
    'createBackdatedGame' : IDL.Func(
        [GamePlan, IDL.Float64, IDL.Bool, IDL.Int],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'createGame' : IDL.Func(
        [GamePlan, IDL.Float64, IDL.Bool],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'getActiveGameCount' : IDL.Func([], [IDL.Nat], ['query']),
    'getAllActiveGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAllBackerRepayments' : IDL.Func(
        [],
        [IDL.Vec(IDL.Tuple(BackerKey, IDL.Float64))],
        ['query'],
      ),
    'getAllGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getAvailableBalance' : IDL.Func([], [IDL.Float64], ['query']),
    'getBackerPositions' : IDL.Func([], [IDL.Vec(BackerPosition)], ['query']),
    'getBackerRepaymentBalance' : IDL.Func([], [IDL.Float64], ['query']),
    'getBackerRepaymentBalanceFor' : IDL.Func(
        [IDL.Principal],
        [IDL.Float64],
        ['query'],
      ),
    'getCanisterICPBalance' : IDL.Func([], [IDL.Nat], []),
    'getCoverChargeBalance' : IDL.Func([], [IDL.Nat], ['query']),
    'getCurrentRoundId' : IDL.Func([], [IDL.Nat], ['query']),
    'getDaysActive' : IDL.Func([], [IDL.Nat], ['query']),
    'getGameById' : IDL.Func([IDL.Nat], [IDL.Opt(GameRecord)], ['query']),
    'getGameResetHistory' : IDL.Func([], [IDL.Vec(GameResetRecord)], ['query']),
    'getGeneralLedger' : IDL.Func([], [IDL.Vec(GeneralLedgerEntry)], ['query']),
    'getGeneralLedgerPage' : IDL.Func(
        [IDL.Nat, IDL.Nat],
        [
          IDL.Record({
            'total' : IDL.Nat,
            'entries' : IDL.Vec(GeneralLedgerEntry),
          }),
        ],
        ['query'],
      ),
    'getGeneralLedgerStats' : IDL.Func(
        [],
        [
          IDL.Record({
            'entryCount' : IDL.Nat,
            'netFlow' : IDL.Float64,
            'totalOutflows' : IDL.Float64,
            'totalInflows' : IDL.Float64,
          }),
        ],
        ['query'],
      ),
    'getMaxDepositLimit' : IDL.Func([], [IDL.Float64], ['query']),
    'getOldestSeriesABacker' : IDL.Func(
        [],
        [IDL.Opt(BackerPosition)],
        ['query'],
      ),
    'getPlatformStats' : IDL.Func([], [PlatformStats], ['query']),
    'getRoundSeedReserve' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalBackerDebt' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalDeposits' : IDL.Func([], [IDL.Float64], ['query']),
    'getTotalWithdrawals' : IDL.Func([], [IDL.Float64], ['query']),
    'getUserGames' : IDL.Func([], [IDL.Vec(GameRecord)], ['query']),
    'getUserGamesFor' : IDL.Func(
        [IDL.Principal],
        [IDL.Vec(GameRecord)],
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
    'isCriticalSectionBusy' : IDL.Func([], [IDL.Bool], ['query']),
    'settleCompoundingGame' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
    'sweepCoverCharges' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'withdrawEarnings' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Float64, 'Err' : IDL.Text })],
        [],
      ),
  });
  return PonziMath;
};
export const init = ({ IDL }) => {
  return [
    IDL.Record({
      'backendPrincipal' : IDL.Principal,
      'testAdmin' : IDL.Principal,
    }),
  ];
};
