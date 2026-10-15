export const idlFactory = ({ IDL }) => {
  const PublicConfig = IDL.Record({
    'cluster_domain' : IDL.Vec(IDL.Nat8),
    'shenanigans' : IDL.Principal,
    'program_id' : IDL.Text,
    'event_authority' : IDL.Text,
    'supported_schema_versions' : IDL.Vec(IDL.Nat),
  });
  const EventId = IDL.Record({
    'signature' : IDL.Text,
    'cluster_domain' : IDL.Vec(IDL.Nat8),
    'inner_instruction_index' : IDL.Nat,
    'event_schema_version' : IDL.Nat,
    'program_id' : IDL.Text,
    'instruction_index' : IDL.Nat,
  });
  const RejectReason = IDL.Variant({
    'RoundMismatch' : IDL.Null,
    'OwnerNotSigner' : IDL.Null,
    'ForgedProgramId' : IDL.Null,
    'MalformedEvent' : IDL.Text,
    'InvalidAccountIndex' : IDL.Null,
    'ForgedEventAuthority' : IDL.Null,
    'TransactionFailed' : IDL.Null,
    'UnknownEvent' : IDL.Null,
    'KindMismatch' : IDL.Null,
    'PlanMismatch' : IDL.Null,
    'BasisMismatch' : IDL.Null,
    'SchemaMismatch' : IDL.Null,
    'CloseMismatch' : IDL.Null,
    'RecipientMismatch' : IDL.Null,
    'WrongAncestry' : IDL.Null,
    'NotEventCpi' : IDL.Null,
    'NotInnerCpi' : IDL.Null,
    'OuterProgramMismatch' : IDL.Null,
    'ProgramIdMismatch' : IDL.Null,
    'SignatureMismatch' : IDL.Null,
    'ClusterMismatch' : IDL.Null,
  });
  const Unverifiable = IDL.Variant({
    'UnsupportedTransactionVersion' : IDL.Opt(IDL.Nat8),
    'NotFinalized' : IDL.Null,
    'NotFound' : IDL.Null,
    'ProviderError' : IDL.Text,
    'UnsupportedSchema' : IDL.Nat,
    'Undecodable' : IDL.Text,
    'Inconsistent' : IDL.Null,
    'Disagreement' : IDL.Null,
  });
  const ObligationId = IDL.Text;
  const Status = IDL.Variant({
    'Invalid' : IDL.Record({ 'at' : IDL.Nat64, 'reason' : RejectReason }),
    'Unverified' : IDL.Record({
      'attempts' : IDL.Nat,
      'last_at' : IDL.Nat64,
      'reason' : Unverifiable,
    }),
    'Forwarded' : IDL.Record({
      'at' : IDL.Nat64,
      'already_observed' : IDL.Bool,
      'obligation_id' : ObligationId,
    }),
    'Quarantined' : IDL.Record({ 'at' : IDL.Nat64, 'reason' : IDL.Text }),
    'Verified' : IDL.Record({
      'last_reason' : IDL.Opt(IDL.Text),
      'last_at' : IDL.Nat64,
      'retries' : IDL.Nat,
    }),
    'Pending' : IDL.Null,
  });
  const PositionOpened = IDL.Record({
    'owner' : IDL.Text,
    'load' : IDL.Nat64,
    'plan' : IDL.Nat8,
    'schema_version' : IDL.Nat8,
    'close_id' : IDL.Opt(IDL.Nat64),
    'config_version' : IDL.Nat32,
    'gross' : IDL.Nat64,
    'nonce' : IDL.Nat64,
    'round_id' : IDL.Nat64,
  });
  const RoundFinalized = IDL.Record({
    'new_round' : IDL.Nat64,
    'schema_version' : IDL.Nat8,
    'close_id' : IDL.Nat64,
    'seed_carried' : IDL.Nat64,
  });
  const PromotionApplied = IDL.Record({
    'loss' : IDL.Nat64,
    'winner' : IDL.Text,
    'schema_version' : IDL.Nat8,
    'close_id' : IDL.Opt(IDL.Nat64),
    'round_id' : IDL.Nat64,
    'entitlement' : IDL.Nat,
  });
  const OtherEvent = IDL.Record({
    'body' : IDL.Vec(IDL.Nat8),
    'name' : IDL.Text,
    'schema_version' : IDL.Nat8,
  });
  const PayoutPaid = IDL.Record({
    'net' : IDL.Nat64,
    'destination' : IDL.Text,
    'owner' : IDL.Text,
    'toll' : IDL.Nat64,
    'ticket_id' : IDL.Nat64,
    'closing' : IDL.Bool,
    'schema_version' : IDL.Nat8,
    'close_id' : IDL.Opt(IDL.Nat64),
    'nonce' : IDL.Nat64,
    'gross_paid' : IDL.Nat64,
    'round_id' : IDL.Nat64,
  });
  const SeriesAFunded = IDL.Record({
    'owner' : IDL.Text,
    'entitlement_after' : IDL.Nat,
    'schema_version' : IDL.Nat8,
    'close_id' : IDL.Opt(IDL.Nat64),
    'gross' : IDL.Nat64,
    'round_id' : IDL.Nat64,
  });
  const BackerClaimed = IDL.Record({
    'b_part' : IDL.Nat64,
    'owner' : IDL.Text,
    'schema_version' : IDL.Nat8,
    'a_part' : IDL.Nat64,
    'amount' : IDL.Nat64,
    'round_id' : IDL.Nat64,
  });
  const Event = IDL.Variant({
    'PositionOpened' : PositionOpened,
    'RoundFinalized' : RoundFinalized,
    'PromotionApplied' : PromotionApplied,
    'Other' : OtherEvent,
    'PayoutPaid' : PayoutPaid,
    'SeriesAFunded' : SeriesAFunded,
    'BackerClaimed' : BackerClaimed,
  });
  const TxVersion = IDL.Variant({ 'v0' : IDL.Null, 'legacy' : IDL.Null });
  const Kind = IDL.Variant({
    'PositionOpened' : IDL.Null,
    'PromotionApplied' : IDL.Null,
    'SeriesAFunded' : IDL.Null,
  });
  const Plan = IDL.Variant({
    'Compound15' : IDL.Null,
    'Compound30' : IDL.Null,
    'Simple' : IDL.Null,
  });
  const Descriptor = IDL.Record({
    'beneficiary_pubkey' : IDL.Text,
    'basis_lamports' : IDL.Nat64,
    'kind' : Kind,
    'plan' : IDL.Opt(Plan),
    'source_round_id' : IDL.Nat64,
    'event_id' : EventId,
    'source_close_id' : IDL.Opt(IDL.Nat64),
  });
  const Verified = IDL.Record({
    'slot' : IDL.Nat64,
    'event' : Event,
    'tx_version' : TxVersion,
    'forward' : Descriptor,
  });
  const Binding = IDL.Variant({
    'Bound' : IDL.Record({ 'at' : IDL.Nat64 }),
    'Unbound' : IDL.Null,
  });
  const Entry = IDL.Record({
    'seq' : IDL.Nat,
    'event_key' : IDL.Text,
    'status' : Status,
    'verified' : IDL.Opt(Verified),
    'next_attempt_at' : IDL.Nat64,
    'descriptor' : Descriptor,
    'binding' : Binding,
    'submitted_at' : IDL.Nat64,
    'submitted_by' : IDL.Principal,
  });
  const BindResult = IDL.Variant({
    'Reserved' : IDL.Null,
    'UnknownEvent' : IDL.Null,
    'Unbound' : IDL.Null,
    'Rejected' : IDL.Text,
    'AlreadyReserved' : IDL.Null,
  });
  const ReleaseResponse = IDL.Variant({
    'ok' : IDL.Record({
      'shenanigans' : IDL.Opt(BindResult),
      'binding' : Binding,
    }),
    'err' : IDL.Variant({
      'UnknownEvent' : IDL.Null,
      'NotConfigured' : IDL.Null,
      'NotForwarded' : IDL.Null,
      'Cooldown' : IDL.Nat64,
    }),
  });
  const SolanaCluster = IDL.Variant({
    'Mainnet' : IDL.Null,
    'Testnet' : IDL.Null,
    'Devnet' : IDL.Null,
  });
  const HttpHeader = IDL.Record({ 'value' : IDL.Text, 'name' : IDL.Text });
  const RpcEndpoint = IDL.Record({
    'url' : IDL.Text,
    'headers' : IDL.Opt(IDL.Vec(HttpHeader)),
  });
  const SupportedProvider = IDL.Variant({
    'AnkrMainnet' : IDL.Null,
    'AlchemyDevnet' : IDL.Null,
    'DrpcMainnet' : IDL.Null,
    'ChainstackDevnet' : IDL.Null,
    'AlchemyMainnet' : IDL.Null,
    'HeliusDevnet' : IDL.Null,
    'AnkrDevnet' : IDL.Null,
    'DrpcDevnet' : IDL.Null,
    'ChainstackMainnet' : IDL.Null,
    'PublicNodeMainnet' : IDL.Null,
    'HeliusMainnet' : IDL.Null,
  });
  const RpcSource = IDL.Variant({
    'Custom' : RpcEndpoint,
    'Supported' : SupportedProvider,
  });
  const RpcSources = IDL.Variant({
    'Default' : SolanaCluster,
    'Custom' : IDL.Vec(RpcSource),
  });
  const Config = IDL.Record({
    'cluster_domain' : IDL.Vec(IDL.Nat8),
    'shenanigans' : IDL.Principal,
    'program_id' : IDL.Text,
    'event_authority' : IDL.Text,
    'supported_schema_versions' : IDL.Vec(IDL.Nat),
    'rpc_a' : RpcSources,
    'rpc_b' : RpcSources,
  });
  const SubmitResult = IDL.Variant({
    'Duplicate' : IDL.Record({
      'seq' : IDL.Nat,
      'status' : Status,
      'same_descriptor' : IDL.Bool,
    }),
    'Malformed' : IDL.Text,
    'Submitted' : IDL.Record({ 'seq' : IDL.Nat }),
  });
  const SubmitResponse = IDL.Variant({
    'ok' : SubmitResult,
    'err' : IDL.Variant({
      'NotRelayer' : IDL.Null,
      'Rejected' : IDL.Text,
      'NotConfigured' : IDL.Null,
    }),
  });
  const Summary = IDL.Record({
    'seq' : IDL.Nat,
    'status' : Status,
    'cursor' : IDL.Nat,
    'binding' : Binding,
  });
  const CommonError = IDL.Variant({
    'UnknownEvent' : IDL.Null,
    'NotConfigured' : IDL.Null,
    'Cooldown' : IDL.Nat64,
  });
  const VerifyForwardResponse = IDL.Variant({
    'ok' : Summary,
    'err' : CommonError,
  });
  return IDL.Service({
    'getConfig' : IDL.Func([], [IDL.Opt(PublicConfig)], ['query']),
    'getCursor' : IDL.Func([], [IDL.Nat], ['query']),
    'getEvent' : IDL.Func([EventId], [IDL.Opt(Entry)], ['query']),
    'getPending' : IDL.Func([IDL.Nat, IDL.Nat], [IDL.Vec(Entry)], ['query']),
    'getRecentEvents' : IDL.Func(
        [IDL.Opt(IDL.Nat), IDL.Nat],
        [IDL.Vec(Entry)],
        ['query'],
      ),
    'getRelayers' : IDL.Func([], [IDL.Vec(IDL.Principal)], ['query']),
    'getUnbound' : IDL.Func([IDL.Text], [IDL.Vec(Entry)], ['query']),
    'releasePending' : IDL.Func([EventId], [ReleaseResponse], []),
    'setConfig' : IDL.Func(
        [Config],
        [IDL.Variant({ 'ok' : IDL.Null, 'err' : IDL.Text })],
        [],
      ),
    'setRelayers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [IDL.Variant({ 'ok' : IDL.Null, 'err' : IDL.Text })],
        [],
      ),
    'submitEventDescriptor' : IDL.Func([Descriptor], [SubmitResponse], []),
    'verifyAndForward' : IDL.Func([EventId], [VerifyForwardResponse], []),
  });
};
export const init = ({ IDL }) => { return []; };
