export const idlFactory = ({ IDL }) => {
  const ControlledStatus = IDL.Record({
    'status' : IDL.Text,
    'memory_size' : IDL.Nat,
    'cycles' : IDL.Nat,
    'idle_cycles_burned_per_day' : IDL.Nat,
    'controllers' : IDL.Vec(IDL.Principal),
    'observed_at' : IDL.Nat64,
    'module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'canister_id' : IDL.Principal,
    'freezing_threshold_secs' : IDL.Nat64,
  });
  const CycleManagerCriticality = IDL.Variant({
    'Important' : IDL.Null,
    'Critical' : IDL.Null,
    'Standard' : IDL.Null,
    'Experimental' : IDL.Null,
  });
  const CycleManagerEnvironment = IDL.Variant({
    'Archived' : IDL.Null,
    'Staging' : IDL.Null,
    'Production' : IDL.Null,
    'Local' : IDL.Null,
    'Test' : IDL.Null,
  });
  const CycleManagerTargetKind = IDL.Variant({
    'SelfReport' : IDL.Null,
    'ControllerStatus' : IDL.Null,
    'InventoryOnly' : IDL.Null,
  });
  const CycleManagerTarget = IDL.Record({
    'canister_id' : IDL.Principal,
    'canister_name' : IDL.Text,
    'display_name' : IDL.Text,
    'expected_freeze_threshold_secs' : IDL.Opt(IDL.Nat64),
    'owner' : IDL.Opt(IDL.Text),
    'metrics_schema_version' : IDL.Nat32,
    'project' : IDL.Text,
    'expected_controllers' : IDL.Vec(IDL.Principal),
    'kind' : CycleManagerTargetKind,
    'environment' : CycleManagerEnvironment,
    'low_threshold_cycles' : IDL.Nat,
    'tags' : IDL.Vec(IDL.Text),
    'criticality' : CycleManagerCriticality,
    'topup_cycles' : IDL.Nat,
  });
  const MusicalChairsObservatory = IDL.Service({
    'add_admin' : IDL.Func([IDL.Principal], [], []),
    'collect_controlled_statuses' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text })],
        [],
      ),
    'controlled_status' : IDL.Func(
        [IDL.Principal],
        [IDL.Opt(ControlledStatus)],
        ['query'],
      ),
    'controlled_statuses' : IDL.Func(
        [],
        [IDL.Vec(ControlledStatus)],
        ['query'],
      ),
    'cycle_manager_targets' : IDL.Func(
        [],
        [IDL.Vec(CycleManagerTarget)],
        ['query'],
      ),
    'list_admins' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'observatory_version' : IDL.Func([], [IDL.Text], ['query']),
    'remove_admin' : IDL.Func(
        [IDL.Principal],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text })],
        [],
      ),
  });
  return MusicalChairsObservatory;
};
export const init = ({ IDL }) => { return []; };
