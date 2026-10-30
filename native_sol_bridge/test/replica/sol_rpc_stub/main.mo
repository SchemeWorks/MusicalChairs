import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Cycles "mo:base/ExperimentalCycles";
import Nat "mo:base/Nat";
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Text "mo:base/Text";

import Decode "../../../Decode";
import Rpc "../../../Rpc";
import Fx "../../fixtures.test";

/// SOL RPC canister STUB for the local-replica run (IMPL-47d). NOT a model of the
/// real canister's behaviour beyond what the bridge consumes:
///   * its candid is the pinned `native_sol_bridge/candid/sol_rpc_canister.did`
///     for the two methods the bridge calls (types imported from the bridge's own
///     Rpc.mo, which was checked against the pinned file with `didc check`);
///   * `getTransaction` answers from a controller-settable table
///     signature -> typed transaction (the 29 captured fixtures are built in);
///   * provider A / B is told apart by the first `#Custom` source URL
///     (contains "stub-a" / "stub-b");
///   * request fidelity: it returns a ValidationError unless the bridge asked for
///     commitment finalized, maxSupportedTransactionVersion 0 and encoding base64,
///     and it records the cycles attached to every call (a real call needs them);
///   * a controller-settable mode: Agree, DisagreeB, DisagreeBNone,
///     UnsupportedVersion, Inconsistent.
persistent actor SolRpcStub {
    public type Mode = {
        /// Both providers return the fixture.
        #Agree;
        /// Provider B returns the same transaction at a different slot.
        #DisagreeB;
        /// Provider B returns Ok(null) (not found).
        #DisagreeBNone;
        /// Both providers answer JSON-RPC -32015 (version above maxSupportedTransactionVersion).
        #UnsupportedVersion;
        /// Both providers answer `Inconsistent` (their own sources disagree).
        #Inconsistent;
    };

    public type CallRecord = {
        method : Text;
        provider : Text;
        signature : Text;
        commitment : ?Rpc.CommitmentLevel;
        maxSupportedTransactionVersion : ?Nat8;
        encoding : ?{ #base58; #base64 };
        cycles_available : Nat;
        mode : Mode;
        served : Text;
    };

    func tm() : OrderedMap.Operations<Text> { OrderedMap.Make<Text>(Text.compare) };

    var mode : Mode = #Agree;
    var fixtures : OrderedMap.Map<Text, Rpc.EncodedConfirmedTransactionWithStatusMeta> = tm().empty<Rpc.EncodedConfirmedTransactionWithStatusMeta>();
    var callLog : [CallRecord] = [];
    var callCount : Nat = 0;

    transient let MAX_LOG : Nat = 200;

    func requireController(caller : Principal) {
        assert (Principal.isController(caller));
    };

    // ------------------------------------------------------------------
    // Controller surface
    // ------------------------------------------------------------------

    public shared ({ caller }) func loadBuiltinFixtures() : async Nat {
        requireController(caller);
        for (f in Fx.all.vals()) {
            fixtures := tm().put(fixtures, f.json.signatures[0], f.rpc);
        };
        tm().size(fixtures);
    };

    public shared ({ caller }) func setFixture(signature : Text, tx : ?Rpc.EncodedConfirmedTransactionWithStatusMeta) : async () {
        requireController(caller);
        switch (tx) {
            case (?t) { fixtures := tm().put(fixtures, signature, t) };
            case (null) { fixtures := tm().delete(fixtures, signature) };
        };
    };

    /// Test helper (IMPL-49b): store a SYNTHETIC COPY of a captured fixture under a fresh
    /// signature (the 64 signature bytes of the wire transaction are replaced by `tag`), so the
    /// replica run can have more distinct events than the captured transactions provide. The
    /// copy is NOT a captured transaction and its signature is not a valid ed25519 signature;
    /// the bridge does not verify signatures (it trusts two providers) and only checks that the
    /// returned transaction carries the signature it asked for. Returns the new signature.
    public shared ({ caller }) func cloneFixture(fromSignature : Text, tag : Nat8) : async ?Text {
        requireController(caller);
        let t = switch (tm().get(fixtures, fromSignature)) { case (?x) { x }; case (null) { return null } };
        let wire : Text = switch (t.transaction.transaction) { case (#binary((w, #base64))) { w }; case (_) { return null } };
        let bytes = switch (Decode.base64Decode(wire)) { case (?b) { b }; case (null) { return null } };
        if (bytes.size() < 65 or bytes[0] != 1) { return null };
        let nb = Array.tabulate<Nat8>(bytes.size(), func(i) { if (i >= 1 and i < 65) { tag } else { bytes[i] } });
        let newSig = Decode.base58Encode(Blob.fromArray(Array.subArray<Nat8>(nb, 1, 64)));
        let t2 : Rpc.EncodedConfirmedTransactionWithStatusMeta = { t with transaction = { t.transaction with transaction = #binary((Decode.base64Encode(nb), #base64)) } };
        fixtures := tm().put(fixtures, newSig, t2);
        ?newSig;
    };

    public shared ({ caller }) func setMode(m : Mode) : async () {
        requireController(caller);
        mode := m;
    };

    public shared ({ caller }) func clearCallLog() : async () {
        requireController(caller);
        callLog := [];
        callCount := 0;
    };

    public query func getMode() : async Mode { mode };
    public query func fixtureCount() : async Nat { tm().size(fixtures) };
    public query func getCallLog() : async [CallRecord] { callLog };
    public query func getCallCount() : async Nat { callCount };

    // ------------------------------------------------------------------
    // Pinned-candid surface
    // ------------------------------------------------------------------

    func providerOf(sources : Rpc.RpcSources) : Text {
        switch (sources) {
            case (#Custom(list)) {
                if (list.size() == 0) { return "?" };
                switch (list[0]) {
                    case (#Custom(e)) {
                        if (Text.contains(e.url, #text "stub-a")) { "A" } else if (Text.contains(e.url, #text "stub-b")) { "B" } else { "?" };
                    };
                    case (#Supported(_)) { "?" };
                };
            };
            case (#Default(_)) { "?" };
        };
    };

    func record(r : CallRecord) {
        callCount += 1;
        let appended = Array.append<CallRecord>(callLog, [r]);
        callLog := if (appended.size() > MAX_LOG) { Array.tabulate<CallRecord>(MAX_LOG, func(i) { appended[appended.size() - MAX_LOG + i] }) } else { appended };
    };

    public shared func getTransaction(
        sources : Rpc.RpcSources,
        _cfg : ?Rpc.RpcConfig,
        p : Rpc.GetTransactionParams,
    ) : async Rpc.MultiGetTransactionResult {
        let provider = providerOf(sources);
        let avail = Cycles.available();
        func answer(served : Text, r : Rpc.MultiGetTransactionResult) : Rpc.MultiGetTransactionResult {
            record({
                method = "getTransaction"; provider; signature = p.signature; commitment = p.commitment;
                maxSupportedTransactionVersion = p.maxSupportedTransactionVersion; encoding = p.encoding;
                cycles_available = avail; mode; served;
            });
            r;
        };
        // Request fidelity: what the bridge MUST ask for.
        if (p.commitment != ?#finalized) {
            return answer("ValidationError: commitment", #Consistent(#Err(#ValidationError("stub: commitment must be finalized"))));
        };
        if (p.maxSupportedTransactionVersion != ?0) {
            return answer("ValidationError: maxSupportedTransactionVersion", #Consistent(#Err(#ValidationError("stub: maxSupportedTransactionVersion must be 0"))));
        };
        if (p.encoding != ?#base64) {
            return answer("ValidationError: encoding", #Consistent(#Err(#ValidationError("stub: encoding must be base64"))));
        };
        let found = tm().get(fixtures, p.signature);
        switch (mode) {
            case (#Agree) {
                answer(if (found == null) { "Ok(null)" } else { "Ok(tx)" }, #Consistent(#Ok(found)));
            };
            case (#DisagreeB) {
                if (provider == "B") {
                    switch (found) {
                        case (?t) { answer("Ok(tx at slot+1)", #Consistent(#Ok(?{ t with slot = t.slot + 1 }))) };
                        case (null) { answer("Ok(null)", #Consistent(#Ok(null))) };
                    };
                } else { answer(if (found == null) { "Ok(null)" } else { "Ok(tx)" }, #Consistent(#Ok(found))) };
            };
            case (#DisagreeBNone) {
                if (provider == "B") { answer("Ok(null)", #Consistent(#Ok(null))) } else {
                    answer(if (found == null) { "Ok(null)" } else { "Ok(tx)" }, #Consistent(#Ok(found)));
                };
            };
            case (#UnsupportedVersion) {
                answer(
                    "JsonRpcError -32015",
                    #Consistent(#Err(#JsonRpcError({
                        code = -32015;
                        message = "Transaction version (1) is not supported by the requesting client. Please try the request again with the following configuration parameter: \"maxSupportedTransactionVersion\": 1";
                    }))),
                );
            };
            case (#Inconsistent) {
                let srcs : [Rpc.RpcSource] = switch (sources) {
                    case (#Custom(l)) { l };
                    case (#Default(_)) { [] };
                };
                let a : Rpc.RpcSource = if (srcs.size() > 0) { srcs[0] } else { #Custom({ url = "http://stub-a.invalid"; headers = null }) };
                let b : Rpc.RpcSource = if (srcs.size() > 1) { srcs[1] } else { #Custom({ url = "http://stub-other.invalid"; headers = null }) };
                answer("Inconsistent", #Inconsistent([(a, #Ok(found)), (b, #Ok(null))]));
            };
        };
    };

    public shared func getSignaturesForAddress(
        sources : Rpc.RpcSources,
        _cfg : ?Rpc.RpcConfig,
        p : Rpc.GetSignaturesForAddressParams,
    ) : async Rpc.MultiGetSignaturesForAddressResult {
        record({
            method = "getSignaturesForAddress"; provider = providerOf(sources); signature = p.pubkey; commitment = p.commitment;
            maxSupportedTransactionVersion = null; encoding = null; cycles_available = Cycles.available(); mode; served = "Ok([])";
        });
        #Consistent(#Ok([]));
    };
};
