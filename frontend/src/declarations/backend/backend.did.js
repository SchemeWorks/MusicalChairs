export const idlFactory = ({ IDL }) => {
  const UserRole = IDL.Variant({
    'admin' : IDL.Null,
    'user' : IDL.Null,
    'guest' : IDL.Null,
  });
  const UserProfile = IDL.Record({ 'name' : IDL.Text });
  const CycleManagerMetric = IDL.Record({
    'key' : IDL.Text,
    'count' : IDL.Nat64,
    'value' : IDL.Nat,
    'label' : IDL.Opt(IDL.Text),
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
    'assignCallerUserRole' : IDL.Func([IDL.Principal, UserRole], [], []),
    'cycle_manager_metrics' : IDL.Func(
        [],
        [IDL.Vec(CycleManagerMetric)],
        ['query'],
      ),
    'cycles_status' : IDL.Func([], [CycleManagerCyclesStatus], ['query']),
    'getBackendICPBalance' : IDL.Func([], [IDL.Nat], []),
    'getCallerUserProfile' : IDL.Func([], [IDL.Opt(UserProfile)], ['query']),
    'getCallerUserRole' : IDL.Func([], [UserRole], ['query']),
    'getPonziMathPrincipal' : IDL.Func([], [IDL.Opt(IDL.Principal)], ['query']),
    'getUserProfile' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(UserProfile)],
        ['query'],
      ),
    'getUserRole' : IDL.Func([IDL.Principal], [UserRole], ['query']),
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
    'isAdmin' : IDL.Func([IDL.Principal], [IDL.Bool], ['query']),
    'isCallerAdmin' : IDL.Func([], [IDL.Bool], ['query']),
    'payManagement' : IDL.Func(
        [IDL.Principal, IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'saveCallerUserProfile' : IDL.Func([UserProfile], [], []),
    'setPonziMathPrincipal' : IDL.Func([IDL.Principal], [], []),
  });
};
export const init = ({ IDL }) => { return []; };
