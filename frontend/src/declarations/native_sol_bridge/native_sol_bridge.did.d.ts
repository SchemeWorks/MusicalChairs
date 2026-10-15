import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface BackerClaimed {
  'b_part' : bigint,
  'owner' : string,
  'schema_version' : number,
  'a_part' : bigint,
  'amount' : bigint,
  'round_id' : bigint,
}
export type BindResult = { 'Reserved' : null } |
  { 'UnknownEvent' : null } |
  { 'Unbound' : null } |
  { 'Rejected' : string } |
  { 'AlreadyReserved' : null };
export type Binding = { 'Bound' : { 'at' : bigint } } |
  { 'Unbound' : null };
export type CommonError = { 'UnknownEvent' : null } |
  { 'NotConfigured' : null } |
  { 'Cooldown' : bigint };
export interface Config {
  'cluster_domain' : Uint8Array | number[],
  'shenanigans' : Principal,
  'program_id' : string,
  'event_authority' : string,
  'supported_schema_versions' : Array<bigint>,
  'rpc_a' : RpcSources,
  'rpc_b' : RpcSources,
}
export interface Descriptor {
  'beneficiary_pubkey' : string,
  'basis_lamports' : bigint,
  'kind' : Kind,
  'plan' : [] | [Plan],
  'source_round_id' : bigint,
  'event_id' : EventId,
  'source_close_id' : [] | [bigint],
}
export interface Entry {
  'seq' : bigint,
  'event_key' : string,
  'status' : Status,
  'verified' : [] | [Verified],
  'next_attempt_at' : bigint,
  'descriptor' : Descriptor,
  'binding' : Binding,
  'submitted_at' : bigint,
  'submitted_by' : Principal,
}
export type Event = { 'PositionOpened' : PositionOpened } |
  { 'RoundFinalized' : RoundFinalized } |
  { 'PromotionApplied' : PromotionApplied } |
  { 'Other' : OtherEvent } |
  { 'PayoutPaid' : PayoutPaid } |
  { 'SeriesAFunded' : SeriesAFunded } |
  { 'BackerClaimed' : BackerClaimed };
export interface EventId {
  'signature' : string,
  'cluster_domain' : Uint8Array | number[],
  'inner_instruction_index' : bigint,
  'event_schema_version' : bigint,
  'program_id' : string,
  'instruction_index' : bigint,
}
export interface HttpHeader { 'value' : string, 'name' : string }
export type Kind = { 'PositionOpened' : null } |
  { 'PromotionApplied' : null } |
  { 'SeriesAFunded' : null };
export type ObligationId = string;
export interface OtherEvent {
  'body' : Uint8Array | number[],
  'name' : string,
  'schema_version' : number,
}
export interface PayoutPaid {
  'net' : bigint,
  'destination' : string,
  'owner' : string,
  'toll' : bigint,
  'ticket_id' : bigint,
  'closing' : boolean,
  'schema_version' : number,
  'close_id' : [] | [bigint],
  'nonce' : bigint,
  'gross_paid' : bigint,
  'round_id' : bigint,
}
export type Plan = { 'Compound15' : null } |
  { 'Compound30' : null } |
  { 'Simple' : null };
export interface PositionOpened {
  'owner' : string,
  'load' : bigint,
  'plan' : number,
  'schema_version' : number,
  'close_id' : [] | [bigint],
  'config_version' : number,
  'gross' : bigint,
  'nonce' : bigint,
  'round_id' : bigint,
}
export interface PromotionApplied {
  'loss' : bigint,
  'winner' : string,
  'schema_version' : number,
  'close_id' : [] | [bigint],
  'round_id' : bigint,
  'entitlement' : bigint,
}
export interface PublicConfig {
  'cluster_domain' : Uint8Array | number[],
  'shenanigans' : Principal,
  'program_id' : string,
  'event_authority' : string,
  'supported_schema_versions' : Array<bigint>,
}
export type RejectReason = { 'RoundMismatch' : null } |
  { 'OwnerNotSigner' : null } |
  { 'ForgedProgramId' : null } |
  { 'MalformedEvent' : string } |
  { 'InvalidAccountIndex' : null } |
  { 'ForgedEventAuthority' : null } |
  { 'TransactionFailed' : null } |
  { 'UnknownEvent' : null } |
  { 'KindMismatch' : null } |
  { 'PlanMismatch' : null } |
  { 'BasisMismatch' : null } |
  { 'SchemaMismatch' : null } |
  { 'CloseMismatch' : null } |
  { 'RecipientMismatch' : null } |
  { 'WrongAncestry' : null } |
  { 'NotEventCpi' : null } |
  { 'NotInnerCpi' : null } |
  { 'OuterProgramMismatch' : null } |
  { 'ProgramIdMismatch' : null } |
  { 'SignatureMismatch' : null } |
  { 'ClusterMismatch' : null };
export type ReleaseResponse = {
    'ok' : { 'shenanigans' : [] | [BindResult], 'binding' : Binding }
  } |
  {
    'err' : { 'UnknownEvent' : null } |
      { 'NotConfigured' : null } |
      { 'NotForwarded' : null } |
      { 'Cooldown' : bigint }
  };
export interface RoundFinalized {
  'new_round' : bigint,
  'schema_version' : number,
  'close_id' : bigint,
  'seed_carried' : bigint,
}
export interface RpcEndpoint {
  'url' : string,
  'headers' : [] | [Array<HttpHeader>],
}
export type RpcSource = { 'Custom' : RpcEndpoint } |
  { 'Supported' : SupportedProvider };
export type RpcSources = { 'Default' : SolanaCluster } |
  { 'Custom' : Array<RpcSource> };
export interface SeriesAFunded {
  'owner' : string,
  'entitlement_after' : bigint,
  'schema_version' : number,
  'close_id' : [] | [bigint],
  'gross' : bigint,
  'round_id' : bigint,
}
export type SolanaCluster = { 'Mainnet' : null } |
  { 'Testnet' : null } |
  { 'Devnet' : null };
export type Status = {
    'Invalid' : { 'at' : bigint, 'reason' : RejectReason }
  } |
  {
    'Unverified' : {
      'attempts' : bigint,
      'last_at' : bigint,
      'reason' : Unverifiable,
    }
  } |
  {
    'Forwarded' : {
      'at' : bigint,
      'already_observed' : boolean,
      'obligation_id' : ObligationId,
    }
  } |
  { 'Quarantined' : { 'at' : bigint, 'reason' : string } } |
  {
    'Verified' : {
      'last_reason' : [] | [string],
      'last_at' : bigint,
      'retries' : bigint,
    }
  } |
  { 'Pending' : null };
export type SubmitResponse = { 'ok' : SubmitResult } |
  {
    'err' : { 'NotRelayer' : null } |
      { 'Rejected' : string } |
      { 'NotConfigured' : null }
  };
export type SubmitResult = {
    'Duplicate' : {
      'seq' : bigint,
      'status' : Status,
      'same_descriptor' : boolean,
    }
  } |
  { 'Malformed' : string } |
  { 'Submitted' : { 'seq' : bigint } };
export interface Summary {
  'seq' : bigint,
  'status' : Status,
  'cursor' : bigint,
  'binding' : Binding,
}
export type SupportedProvider = { 'AnkrMainnet' : null } |
  { 'AlchemyDevnet' : null } |
  { 'DrpcMainnet' : null } |
  { 'ChainstackDevnet' : null } |
  { 'AlchemyMainnet' : null } |
  { 'HeliusDevnet' : null } |
  { 'AnkrDevnet' : null } |
  { 'DrpcDevnet' : null } |
  { 'ChainstackMainnet' : null } |
  { 'PublicNodeMainnet' : null } |
  { 'HeliusMainnet' : null };
export type TxVersion = { 'v0' : null } |
  { 'legacy' : null };
export type Unverifiable = { 'UnsupportedTransactionVersion' : [] | [number] } |
  { 'NotFinalized' : null } |
  { 'NotFound' : null } |
  { 'ProviderError' : string } |
  { 'UnsupportedSchema' : bigint } |
  { 'Undecodable' : string } |
  { 'Inconsistent' : null } |
  { 'Disagreement' : null };
export interface Verified {
  'slot' : bigint,
  'event' : Event,
  'tx_version' : TxVersion,
  'forward' : Descriptor,
}
export type VerifyForwardResponse = { 'ok' : Summary } |
  { 'err' : CommonError };
export interface _SERVICE {
  'getConfig' : ActorMethod<[], [] | [PublicConfig]>,
  'getCursor' : ActorMethod<[], bigint>,
  'getEvent' : ActorMethod<[EventId], [] | [Entry]>,
  'getPending' : ActorMethod<[bigint, bigint], Array<Entry>>,
  'getRecentEvents' : ActorMethod<[[] | [bigint], bigint], Array<Entry>>,
  'getRelayers' : ActorMethod<[], Array<Principal>>,
  'getUnbound' : ActorMethod<[string], Array<Entry>>,
  'releasePending' : ActorMethod<[EventId], ReleaseResponse>,
  'setConfig' : ActorMethod<[Config], { 'ok' : null } | { 'err' : string }>,
  'setRelayers' : ActorMethod<
    [Array<Principal>],
    { 'ok' : null } |
      { 'err' : string }
  >,
  'submitEventDescriptor' : ActorMethod<[Descriptor], SubmitResponse>,
  'verifyAndForward' : ActorMethod<[EventId], VerifyForwardResponse>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
