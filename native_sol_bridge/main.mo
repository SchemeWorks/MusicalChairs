import Error "mo:base/Error";
import Int "mo:base/Int";
import Nat64 "mo:base/Nat64";
import Principal "mo:base/Principal";
import Time "mo:base/Time";

import Decode "Decode";
import Events "Events";
import Outbox "Outbox";
import Rpc "Rpc";
import Verify "Verify";

/// native_sol_bridge — ICP side of native Solana ingestion (spec section 9,
/// DR-24/DR-25). Thin glue: ALL decisions live in the pure modules
/// (Decode/Verify/Outbox/Events), which are tested standalone.
///
/// TRUST STATEMENT. The bridge forwards a native game event to Shenanigans
/// only when (1) an allowlisted relayer proposed a descriptor, (2) BOTH
/// configured RPC providers, queried through the SOL RPC canister at
/// `finalized`, returned the same successful transaction, and (3) on that
/// transaction the named instruction is a stackHeight-2 inner event-CPI of the
/// configured program with the configured event authority as account 0 whose
/// decoded body equals every descriptor field. Two RPC providers are a trusted
/// observation boundary, not consensus: if both are compromised or collude,
/// the bridge can be shown a false history. The relayer is trusted for
/// liveness only (it can withhold or delay events, never fabricate one). The
/// controllers can reconfigure the program id, providers and Shenanigans
/// target; nothing here is blackholed.
persistent actor NativeSolBridge {
    // ------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------

    public type Config = {
        /// Native Solana program id (base58).
        program_id : Text;
        /// 32-byte `Pool.cluster_domain`.
        cluster_domain : Blob;
        /// Canonical `["__event_authority"]` PDA of `program_id` (base58).
        event_authority : Text;
        /// Event schema versions this build can decode (currently [1]).
        supported_schema_versions : [Nat];
        /// Provider A and provider B: two DIFFERENT explicit sources, so one
        /// `Consistent` reply is one provider's view and agreement between the
        /// two replies is checked here.
        rpc_a : Rpc.RpcSources;
        rpc_b : Rpc.RpcSources;
        /// Shenanigans canister (the observe/bind target).
        shenanigans : Principal;
    };

    /// `getConfig` omits the RPC sources: custom endpoints can embed API keys.
    public type PublicConfig = {
        program_id : Text;
        cluster_domain : Blob;
        event_authority : Text;
        supported_schema_versions : [Nat];
        shenanigans : Principal;
    };

    /// Shenanigans' side of the protocol (plan M12; `shenanigans/NativeRewards.mo`).
    type ShenanigansActor = actor {
        observeNativeEvent : shared Events.Descriptor -> async Events.ObserveResult;
        bindAndReserve : shared Events.EventId -> async Events.BindResult;
        /// Bridge-or-admin on Shenanigans. The bridge passes only (event id, leg index);
        /// recipient, amount, memo and created_at_time are the legs Shenanigans froze at bind.
        executeNativeRewardLeg : shared (Events.EventId, Nat) -> async Events.LegStatus;
    };

    public type Summary = {
        seq : Nat;
        status : Outbox.Status;
        binding : Outbox.Binding;
        cursor : Nat;
    };

    public type CommonError = { #NotConfigured; #UnknownEvent; #Cooldown : Nat64 };

    // ------------------------------------------------------------------
    // State (stable)
    // ------------------------------------------------------------------

    var config : ?Config = null;
    var relayers : [Principal] = [];
    let outbox : Outbox.State = Outbox.empty();
    /// Per-leg statuses Shenanigans returned to executeReservedLeg. A separate additive
    /// stable field: `Outbox.Entry` and `Outbox.State` keep their stored shape.
    let legs : Outbox.LegStore = Outbox.emptyLegs();

    // ------------------------------------------------------------------
    // Constants
    // ------------------------------------------------------------------

    transient let solRpc : Rpc.RpcActor = actor (Rpc.SOL_RPC_CANISTER_ID);

    /// Cycles attached per `getTransaction` (unspent cycles are refunded by the
    /// SOL RPC canister). To be calibrated against `getTransactionCyclesCost`
    /// at integration; same order as ponzi_math_sol's RPC_CYCLES.
    transient let RPC_CYCLES : Nat = 25_000_000_000;
    /// Expected reply size (bytes) passed as `responseSizeEstimate`.
    transient let RESPONSE_SIZE_ESTIMATE : Nat64 = 131_072;
    /// Minimum gap between verify/forward attempts on one event (ns).
    transient let ATTEMPT_COOLDOWN_NS : Nat64 = 30_000_000_000;
    transient let BIND_COOLDOWN_NS : Nat64 = 10_000_000_000;
    /// Per-event gap between calls that reach Shenanigans to execute a leg (shares the
    /// Entry.next_attempt_at gate with verify and bind; a successful forward or bind clears it,
    /// and a Committed leg is answered from the record with no call and no cooldown).
    transient let EXECUTE_COOLDOWN_NS : Nat64 = 10_000_000_000;

    func nowNs() : Nat64 { Nat64.fromNat(Int.abs(Time.now())) };

    func verifyConfig(c : Config) : Verify.Config {
        {
            program_id = c.program_id;
            cluster_domain = c.cluster_domain;
            event_authority = c.event_authority;
            supported_schema_versions = c.supported_schema_versions;
        };
    };

    func summary(e : Outbox.Entry) : Summary {
        { seq = e.seq; status = e.status; binding = e.binding; cursor = Outbox.cursor(outbox) };
    };

    func isRelayer(p : Principal) : Bool {
        for (r in relayers.vals()) { if (r == p) { return true } };
        false;
    };

    // ------------------------------------------------------------------
    // Administration (controllers)
    // ------------------------------------------------------------------

    func configError(c : Config) : ?Text {
        if (not Decode.isBase58Of(c.program_id, 32)) { return ?"program_id must be a base58 32-byte key" };
        if (not Decode.isBase58Of(c.event_authority, 32)) { return ?"event_authority must be a base58 32-byte key" };
        if (c.cluster_domain.size() != 32) { return ?"cluster_domain must be 32 bytes" };
        if (c.supported_schema_versions.size() == 0) { return ?"supported_schema_versions must not be empty" };
        for (v in c.supported_schema_versions.vals()) {
            if (v == 0 or v > 255) { return ?"schema versions must be in 1..255" };
        };
        switch (c.rpc_a, c.rpc_b) {
            case (#Custom(a), #Custom(b)) {
                if (a.size() == 0 or b.size() == 0) { return ?"each provider needs at least one source" };
                for (x in a.vals()) {
                    for (y in b.vals()) {
                        if (x == y) { return ?"providers A and B must not share a source" };
                    };
                };
            };
            case (_) { return ?"rpc_a and rpc_b must be explicit #Custom source lists (two distinct providers)" };
        };
        null;
    };

    public shared ({ caller }) func setConfig(c : Config) : async { #ok; #err : Text } {
        if (not Principal.isController(caller)) { return #err("controllers only") };
        switch (configError(c)) {
            case (?m) { #err(m) };
            case (null) { config := ?c; #ok };
        };
    };

    public shared ({ caller }) func setRelayers(ps : [Principal]) : async { #ok; #err : Text } {
        if (not Principal.isController(caller)) { return #err("controllers only") };
        relayers := ps;
        #ok;
    };

    public query func getConfig() : async ?PublicConfig {
        switch (config) {
            case (null) { null };
            case (?c) {
                ?{
                    program_id = c.program_id;
                    cluster_domain = c.cluster_domain;
                    event_authority = c.event_authority;
                    supported_schema_versions = c.supported_schema_versions;
                    shenanigans = c.shenanigans;
                };
            };
        };
    };

    public query func getRelayers() : async [Principal] { relayers };

    // ------------------------------------------------------------------
    // Relayer ingress
    // ------------------------------------------------------------------

    public type SubmitResponse = {
        #ok : Outbox.SubmitResult;
        #err : { #NotConfigured; #NotRelayer; #Rejected : Text };
    };

    /// Relayer-authenticated. Descriptors that contradict this canister's own
    /// configuration (wrong cluster/program) are refused without being stored.
    public shared ({ caller }) func submitEventDescriptor(d : Events.Descriptor) : async SubmitResponse {
        let cfg = switch (config) { case (null) { return #err(#NotConfigured) }; case (?c) { c } };
        if (not isRelayer(caller)) { return #err(#NotRelayer) };
        switch (Verify.precheck(verifyConfig(cfg), d)) {
            case (? #Invalid(r)) { return #err(#Rejected(Verify.reasonText(r))) };
            case (_) {};
        };
        #ok(Outbox.submit(outbox, caller, d, nowNs()));
    };

    // ------------------------------------------------------------------
    // Verify + forward (permissionless, one event per call)
    // ------------------------------------------------------------------

    public type VerifyForwardResponse = {
        #ok : Summary;
        #err : CommonError;
    };

    func observe(commitment : Rpc.CommitmentLevel, r : { #ok : Rpc.MultiGetTransactionResult; #err : Text }) : Decode.ProviderObservation {
        switch (r) {
            case (#ok(m)) { Decode.classify(commitment, m) };
            case (#err(t)) { #Error("call failed: " # t) };
        };
    };

    /// One `getTransaction` per provider (concurrent), then pure verification.
    func fetchAndVerify(cfg : Config, d : Events.Descriptor) : async Verify.Verdict {
        let params : Rpc.GetTransactionParams = {
            signature = d.event_id.signature;
            commitment = ?#finalized;
            maxSupportedTransactionVersion = ?Decode.MAX_SUPPORTED_TX_VERSION;
            encoding = ?#base64;
        };
        let rpcCfg : ?Rpc.RpcConfig = ?{
            responseSizeEstimate = ?RESPONSE_SIZE_ESTIMATE;
            responseConsensus = ?#Equality;
        };
        let fa = (with cycles = RPC_CYCLES) solRpc.getTransaction(cfg.rpc_a, rpcCfg, params);
        let fb = (with cycles = RPC_CYCLES) solRpc.getTransaction(cfg.rpc_b, rpcCfg, params);
        let ra : { #ok : Rpc.MultiGetTransactionResult; #err : Text } = try { #ok(await fa) } catch (e) { #err(Error.message(e)) };
        let rb : { #ok : Rpc.MultiGetTransactionResult; #err : Text } = try { #ok(await fb) } catch (e) { #err(Error.message(e)) };
        Verify.verify(verifyConfig(cfg), d, observe(#finalized, ra), observe(#finalized, rb));
    };

    /// Drive one event as far as it will go: verify (two RPC calls) if not yet
    /// verified, then `observeNativeEvent` (one Shenanigans call) if verified.
    /// Idempotent and safe to call concurrently: every state write goes
    /// through a guarded transition, and a per-event cooldown (persisted
    /// before the first await) bounds the cycles a caller can burn.
    public shared func verifyAndForward(eventId : Events.EventId) : async VerifyForwardResponse {
        let cfg = switch (config) { case (null) { return #err(#NotConfigured) }; case (?c) { c } };
        let key = Events.eventKey(eventId);
        var current : Outbox.Entry = switch (Outbox.beginAttempt(outbox, key, nowNs(), ATTEMPT_COOLDOWN_NS)) {
            case (#Unknown) { return #err(#UnknownEvent) };
            case (#Terminal(e)) { return #ok(summary(e)) };
            case (#Cooldown(t)) { return #err(#Cooldown(t)) };
            case (#Go(e)) { e };
        };

        // Step 1: verification.
        switch (current.status) {
            case (#Pending or #Unverified(_)) {
                let verdict : Verify.Verdict = switch (Verify.precheck(verifyConfig(cfg), current.descriptor)) {
                    case (?v) { v };
                    case (null) { await fetchAndVerify(cfg, current.descriptor) };
                };
                switch (Outbox.applyVerdict(outbox, key, verdict, nowNs())) {
                    case (#Applied(e)) { current := e };
                    case (#Stale(e)) { current := e };
                    case (#Unknown) { return #err(#UnknownEvent) };
                };
            };
            case (_) {};
        };

        // Step 2: forward to Shenanigans (idempotent on the event id).
        switch (current.status, current.verified) {
            case (#Verified(_), ?v) {
                let sh : ShenanigansActor = actor (Principal.toText(cfg.shenanigans));
                let res : { #ok : Events.ObserveResult; #err : Text } = try {
                    #ok(await sh.observeNativeEvent(v.forward));
                } catch (e) { #err(Error.message(e)) };
                let applied = switch (res) {
                    case (#ok(r)) { Outbox.applyForward(outbox, key, r, nowNs()) };
                    case (#err(m)) { Outbox.applyForwardError(outbox, key, m, nowNs()) };
                };
                switch (applied) {
                    case (#Applied(e)) { current := e };
                    case (#Stale(e)) { current := e };
                    case (#Unknown) { return #err(#UnknownEvent) };
                };
            };
            case (_) {};
        };
        #ok(summary(current));
    };

    // ------------------------------------------------------------------
    // SIWS binding release (permissionless)
    // ------------------------------------------------------------------

    public type ReleaseResponse = {
        /// `shenanigans` is Shenanigans' answer (null when the call itself failed).
        #ok : { binding : Outbox.Binding; shenanigans : ?Events.BindResult };
        #err : CommonError or { #NotForwarded };
    };

    /// For an event Shenanigans already holds as Observed: ask it to bind the
    /// beneficiary pubkey to a principal and reserve the legs. `#Unbound`
    /// leaves the event pubkey-keyed (retry after the user signs in with that
    /// wallet); the cursor is not involved either way.
    public shared func releasePending(eventId : Events.EventId) : async ReleaseResponse {
        let cfg = switch (config) { case (null) { return #err(#NotConfigured) }; case (?c) { c } };
        let key = Events.eventKey(eventId);
        switch (Outbox.beginBind(outbox, key, nowNs(), BIND_COOLDOWN_NS)) {
            case (#Unknown) { #err(#UnknownEvent) };
            case (#Cooldown(t)) { #err(#Cooldown(t)) };
            case (#Terminal(e)) {
                switch (e.status) {
                    case (#Forwarded(_)) { #ok({ binding = e.binding; shenanigans = null }) };
                    case (_) { #err(#NotForwarded) };
                };
            };
            case (#Go(_)) {
                let sh : ShenanigansActor = actor (Principal.toText(cfg.shenanigans));
                let res : { #ok : Events.BindResult; #err : Text } = try {
                    #ok(await sh.bindAndReserve(eventId));
                } catch (e) { #err(Error.message(e)) };
                switch (res) {
                    case (#err(_)) {
                        switch (Outbox.get(outbox, key)) {
                            case (?e) { #ok({ binding = e.binding; shenanigans = null }) };
                            case (null) { #err(#UnknownEvent) };
                        };
                    };
                    case (#ok(r)) {
                        switch (Outbox.applyBind(outbox, key, r, nowNs())) {
                            case (#Applied(e)) { #ok({ binding = e.binding; shenanigans = ?r }) };
                            case (#Stale(e)) { #ok({ binding = e.binding; shenanigans = ?r }) };
                            case (#Unknown) { #err(#UnknownEvent) };
                        };
                    };
                };
            };
        };
    };

    // ------------------------------------------------------------------
    // Bridge-driven execution of one frozen reserved leg (permissionless)
    // ------------------------------------------------------------------

    public type ExecuteResponse = {
        #ok : {
            leg : Outbox.LegRecord;
            /// The status Shenanigans returned (or the stored Committed status).
            status : Events.LegStatus;
            /// true: a Committed leg answered from the record, no call was made.
            from_record : Bool;
        };
        #err : {
            #NotConfigured;
            #Anonymous;
            #UnknownEvent;
            #BadLegIndex;
            /// The outbox entry is not Forwarded (Shenanigans does not hold it as Observed).
            #NotForwarded;
            /// Forwarded but not bound to a principal yet: nothing is Reserved to execute.
            #NotBound;
            #Cooldown : Nat64;
            /// Shenanigans rejected or trapped the call (for example an index it has no leg for).
            #CallFailed : Text;
        };
    };

    /// Execute ONE frozen leg of a Reserved obligation. Anyone may call it (the anonymous
    /// principal is refused); the caller supplies no recipient, amount or bytes: the bridge
    /// calls `shenanigans.executeNativeRewardLeg(event_id, leg_index)` as itself (it is on
    /// Shenanigans' execute allowlist) and Shenanigans re-presents the fields it froze at bind.
    /// Requires the outbox entry to be Forwarded AND bound. Idempotent: a Committed leg returns
    /// the stored status with no call; Pending / SubmittedUnknown / Retryable / Quarantined legs
    /// call again and Shenanigans' own idempotency (write-ahead SubmittedUnknown, identical
    /// retry, ledger Duplicate, dedup-window quarantine) decides. Per-event cooldown applies.
    /// The admin path on Shenanigans stays available as a fallback; the observation allowlist is
    /// never touched here.
    public shared ({ caller }) func executeReservedLeg(eventId : Events.EventId, legIndex : Nat) : async ExecuteResponse {
        if (not Outbox.mayExecute(caller)) { return #err(#Anonymous) };
        let cfg = switch (config) { case (null) { return #err(#NotConfigured) }; case (?c) { c } };
        let key = Events.eventKey(eventId);
        switch (Outbox.beginExecute(outbox, legs, key, legIndex, nowNs(), EXECUTE_COOLDOWN_NS)) {
            case (#Unknown) { #err(#UnknownEvent) };
            case (#BadLegIndex) { #err(#BadLegIndex) };
            case (#NotForwarded) { #err(#NotForwarded) };
            case (#NotBound) { #err(#NotBound) };
            case (#Cooldown(t)) { #err(#Cooldown(t)) };
            case (#Stored(r)) {
                switch (r.status) {
                    case (?st) { #ok({ leg = r; status = st; from_record = true }) };
                    case (null) { #err(#CallFailed("unreachable: stored leg without status")) };
                };
            };
            case (#Go(_)) {
                let sh : ShenanigansActor = actor (Principal.toText(cfg.shenanigans));
                let res : { #ok : Events.LegStatus; #err : Text } = try {
                    #ok(await sh.executeNativeRewardLeg(eventId, legIndex));
                } catch (e) { #err(Error.message(e)) };
                switch (res) {
                    case (#err(m)) {
                        ignore Outbox.applyLegError(legs, key, legIndex, m, nowNs());
                        #err(#CallFailed(m));
                    };
                    case (#ok(st)) {
                        switch (Outbox.applyLegStatus(outbox, legs, key, legIndex, st, nowNs())) {
                            case (#Applied(r)) { #ok({ leg = r; status = st; from_record = false }) };
                            // A Committed record is final: report it, not the stale answer.
                            case (#Stale(r)) {
                                switch (r.status) {
                                    case (?kept) { #ok({ leg = r; status = kept; from_record = true }) };
                                    case (null) { #ok({ leg = r; status = st; from_record = false }) };
                                };
                            };
                            case (#Unknown) { #err(#UnknownEvent) };
                        };
                    };
                };
            };
        };
    };

    // ------------------------------------------------------------------
    // Reads (relayer cursor, history and feeds - DR-25)
    // ------------------------------------------------------------------

    /// Every entry with seq <= cursor is durable-done (forwarded, quarantined
    /// or provably invalid). A gap (e.g. unsupported transaction version)
    /// holds it back.
    public query func getCursor() : async Nat { Outbox.cursor(outbox) };

    /// Open (cursor-blocking) entries with seq > `after`, oldest first; pass
    /// `getCursor()` to see exactly what holds the cursor back.
    public query func getPending(after : Nat, limit : Nat) : async [Outbox.Entry] {
        Outbox.pending(outbox, after, limit);
    };

    public query func getEvent(eventId : Events.EventId) : async ?Outbox.Entry {
        Outbox.get(outbox, Events.eventKey(eventId));
    };

    /// Forwarded events whose beneficiary has no SIWS binding yet.
    public query func getUnbound(pubkey : Text) : async [Outbox.Entry] {
        Outbox.unboundFor(outbox, pubkey);
    };

    /// PUBLIC verified history (DR-25, spec section 9): newest-first page of entries at the
    /// verified-delivery checkpoint only: `Forwarded` (Shenanigans answered Observed or
    /// AlreadyObserved) and `Quarantined` (a durable PERMANENT Shenanigans rejection, returned
    /// with its status). `before`: only entries with seq < before. Pending, Unverified,
    /// Verified-but-unacknowledged and Invalid entries are NOT here; see `getIntakeEvents`.
    public query func getRecentEvents(before : ?Nat, limit : Nat) : async [Outbox.Entry] {
        Outbox.verifiedHistory(outbox, before, limit);
    };

    /// Operator/audit intake view, newest-first, the complement of `getRecentEvents`:
    /// Pending, Unverified (including gaps), Verified-but-unacknowledged and Invalid.
    /// `Invalid` is an intake/audit terminal state: the descriptor was proven false from an
    /// agreed finalized observation, the cursor may pass it, and a corrected descriptor for the
    /// same event id is admitted as a new entry. It is NOT verified delivery and never appears
    /// in the public history.
    public query func getIntakeEvents(before : ?Nat, limit : Nat) : async [Outbox.Entry] {
        Outbox.intake(outbox, before, limit);
    };

    /// What the bridge has recorded from Shenanigans about this event's reward legs
    /// (ascending leg index): the last status returned, the last call error, call count.
    public query func getLegStatuses(eventId : Events.EventId) : async [Outbox.LegRecord] {
        Outbox.legsOf(legs, Events.eventKey(eventId));
    };
};
