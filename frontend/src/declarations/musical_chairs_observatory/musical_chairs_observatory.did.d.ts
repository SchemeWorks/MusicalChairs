import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export type ControlledStatus = {
  'status' : string,
  'memory_size' : bigint,
  'cycles' : bigint,
  'idle_cycles_burned_per_day' : bigint,
  'controllers' : Array<Principal>,
  'observed_at' : bigint,
  'module_hash' : [] | [Uint8Array | number[]],
  'canister_id' : Principal,
  'freezing_threshold_secs' : bigint,
};
export type CycleManagerCriticality = { 'Important' : null } |
  { 'Critical' : null } |
  { 'Standard' : null } |
  { 'Experimental' : null };
export type CycleManagerEnvironment = { 'Archived' : null } |
  { 'Staging' : null } |
  { 'Production' : null } |
  { 'Local' : null } |
  { 'Test' : null };
export type CycleManagerTarget = {
  'canister_id' : Principal,
  'canister_name' : string,
  'display_name' : string,
  'expected_freeze_threshold_secs' : [] | [bigint],
  'owner' : [] | [string],
  'metrics_schema_version' : number,
  'project' : string,
  'expected_controllers' : Array<Principal>,
  'kind' : CycleManagerTargetKind,
  'environment' : CycleManagerEnvironment,
  'low_threshold_cycles' : bigint,
  'tags' : Array<string>,
  'criticality' : CycleManagerCriticality,
  'topup_cycles' : bigint,
};
export type CycleManagerTargetKind = { 'SelfReport' : null } |
  { 'ControllerStatus' : null } |
  { 'InventoryOnly' : null };
export interface MusicalChairsObservatory {
  'add_admin' : ActorMethod<[Principal], undefined>,
  'collect_controlled_statuses' : ActorMethod<
    [],
    { 'Ok' : bigint } |
      { 'Err' : string }
  >,
  'controlled_status' : ActorMethod<[Principal], [] | [ControlledStatus]>,
  'controlled_statuses' : ActorMethod<[], Array<ControlledStatus>>,
  'cycle_manager_targets' : ActorMethod<[], Array<CycleManagerTarget>>,
  'list_admins' : ActorMethod<[], Array<Principal>>,
  'observatory_version' : ActorMethod<[], string>,
  'remove_admin' : ActorMethod<
    [Principal],
    { 'Ok' : null } |
      { 'Err' : string }
  >,
}
export interface _SERVICE extends MusicalChairsObservatory {}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
