/**
 * NativeRewards -- pure state machine for native-Solana PP reward obligations
 * (plan Milestone 12, Shenanigans half; reconciled spec section 9, R6 per C3).
 *
 * WHY A PURE MODULE
 *   Actor async paths cannot run under `moc -r`, and the exactly-once argument
 *   lives in the ORDER of state writes relative to awaits. So every transition
 *   is a synchronous function `state -> inputs -> (state, result)` with time,
 *   policy, SIWS lookup results, ledger-call outcomes, effect snapshots and the
 *   shared signup marker all injected. The actor is thin glue: it commits the
 *   returned state, performs at most ONE await between two pure steps, and
 *   commits the next returned state.
 *
 * LIFECYCLE (spec section 9)
 *   observe          -> obligation #Observed, policy_version pinned, nothing consumed
 *   prepareBind      -> which SIWS address to look up (no state change)
 *   [await siws.get_principal]
 *   applyBind        -> re-reads the obligation, rejects a second binder, claims the
 *                       shared signup marker, computes EVERY leg once under the pinned
 *                       policy version, consumes effect/siphon caps, freezes memo and
 *                       created_at_time per leg => #Reserved. NO await between the
 *                       marker claim and the obligation write.
 *   beginExecute     -> leg #SubmittedUnknown, committed BEFORE the icrc1_transfer await
 *   [await icrc1_transfer]
 *   applyTransferOutcome -> #Committed / #Retryable / #Quarantined
 *   beginRetry       -> identical frozen fields, only inside the dedup window
 *   reconcileFromLedgerBlock -> commits only on an exact `mint` record from the pinned
 *                       ledger (or a listed archive) with a verified minting account
 *
 * DESIGN NOTES
 *   * Memo is sha256(domain || 0x00 || event key || 0x00 || be32(leg index)), the
 *     spec's domain-separated 32-byte hash; a pure SHA-256 is included (base has none).
 *   * created_at_time is the BINDING time and is never refreshed (contrast
 *     mintInternal, which stamps a new time per call).
 *   * Policy is pinned at observation: a later policy append never changes the legs of
 *     an already observed event.
 *   * Leg arithmetic mirrors the legacy observer: gross basis x PP-per-SOL, cascadeInitialBps
 *     off the top, deductive upline walk at cascadePassthroughBps with flow-around of
 *     inactive uplines, residual to the house; mint multiplier and siphon (with cap) apply
 *     to every recipient exactly as mintWithEffects does. Differences: integer lamport math
 *     (no 10-lamport Float truncation), and every leg is computed once at binding instead
 *     of interleaved with awaits.
 *   * The legacy mintWithEffects / mintInternal are never referenced from here.
 */

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Buffer "mo:base/Buffer";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Text "mo:base/Text";
import Subaccount "Subaccount";

module {

    // ================================================================
    // Constants
    // ================================================================

    public let LAMPORTS_PER_SOL : Nat = 1_000_000_000;
    /// 1 whole PP = 10^8 units (PpLedger.PP_UNIT_SCALE).
    public let PP_UNIT_SCALE : Nat = 100_000_000;
    /// Event schema versions the observe step accepts. Anything else is a
    /// permanent rejection (the bridge's decoder gap handling happens upstream).
    public let SUPPORTED_SCHEMA_VERSIONS : [Nat] = [1];
    /// A leg may only be (re-)presented while at least this much of the ledger
    /// dedup window remains, so a retry can never race the window's edge.
    public let DEDUP_SAFETY_MARGIN_NS : Nat = 300_000_000_000; // 5 minutes
    /// The ICRC-1 ledger transaction window (24h). A policy may not claim a
    /// longer verified window.
    public let MAX_DEDUP_WINDOW_NS : Nat = 86_400_000_000_000;
    public let MEMO_DOMAIN : Text = "musicalchairs/native-reward-leg/v1";
    public let MAX_SIGNUP_GIFT_PP : Nat = 1_000_000;

    // ================================================================
    // Event input (declared for worker A to mirror exactly)
    // ================================================================

    public type EventId = {
        cluster_domain : Blob;
        program_id : Text;
        signature : Text;
        instruction_index : Nat;
        inner_instruction_index : Nat;
        event_schema_version : Nat;
    };

    public type EventKind = { #PositionOpened; #SeriesAFunded; #PromotionApplied };

    public type Plan = { #Simple; #Compound15; #Compound30 };

    public type NativeEvent = {
        event_id : EventId;
        source_round_id : Nat64;
        source_close_id : ?Nat64;
        kind : EventKind;
        beneficiary_pubkey : Text; // base58
        basis_lamports : Nat64; // gross game / Series A amount, or the Series B loss
        plan : ?Plan;
    };

    /// Canonical text key of the immutable event id; also the ObligationId.
    public type ObligationId = Text;

    // ================================================================
    // Stored records
    // ================================================================

    public type NativeObservation = {
        event_id : EventId;
        source_round : Nat64;
        source_close : ?Nat64;
        basis : Nat64;
        beneficiary_pubkey : Text;
        kind : EventKind;
        plan : ?Plan;
        policy_version : Nat;
        observed_at : Int;
    };

    public type Account = { owner : Principal; subaccount : ?Blob };

    public type LegStatus = {
        #Pending;
        #SubmittedUnknown;
        #Committed : { block : Nat };
        #Retryable : { reason : Text; attempts : Nat };
        #Quarantined : { reason : Text; recoverable : Bool };
    };

    /// Informational tag for what a leg pays (also drives the referral-stats
    /// bump on first commit). Not used for any exactly-once decision.
    public type LegRole = {
        #player;
        #signupGift;
        #cascade : { rank : Nat; gross : Nat; forGift : Bool };
        #residual : { forGift : Bool };
        #siphon : { from : Principal };
    };

    public type Leg = {
        index : Nat;
        recipient : Account;
        /// The player/upline/house/siphoner whose chip subaccount `recipient` is.
        holder : Principal;
        role : LegRole;
        amount : Nat;
        fee : Null;
        memo : Blob; // 32 bytes
        created_at_time : Nat64;
        /// Number of submissions made (incremented BEFORE each ledger await).
        attempts : Nat;
        status : LegStatus;
    };

    public type ObligationStatus = { #Observed; #Reserved; #Completed; #PartiallyQuarantined };

    public type NativeRewardObligation = {
        event_id : EventId;
        status : ObligationStatus;
        policy_version : Nat;
        beneficiary : ?Principal;
        reserved_at : ?Int;
        legs : [Leg];
    };

    public type PolicyVersion = {
        version : Nat;
        effectiveFrom : Int;
        simple21DayPpPerSol : Nat;
        compounding15DayPpPerSol : Nat;
        compounding30DayPpPerSol : Nat;
        backerPpPerSol : Nat;
        signupGiftPp : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        /// Whether the mint multiplier / mint-tax siphon snapshot applies at binding.
        applyEffects : Bool;
        /// Verified ledger transaction (dedup) window in ns; <= 24h.
        dedupWindowNs : Nat;
    };

    public type PolicyDraft = {
        simple21DayPpPerSol : Nat;
        compounding15DayPpPerSol : Nat;
        compounding30DayPpPerSol : Nat;
        backerPpPerSol : Nat;
        signupGiftPp : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        applyEffects : Bool;
        dedupWindowNs : Nat;
    };

    public type ObsMap = OrderedMap.Map<Text, NativeObservation>;
    public type OblMap = OrderedMap.Map<Text, NativeRewardObligation>;
    /// The shared canonical `signupGiftClaimed` map (principal -> claim time ns).
    public type SignupMarkers = OrderedMap.Map<Principal, Int>;

    public type State = {
        bridgeAllowlist : [Principal];
        policyTable : [PolicyVersion]; // append-only, version = index + 1
        observations : ObsMap;
        obligations : OblMap;
    };

    // Library modules must have a static body, so the ordered-map operation
    // objects are built on use (cheap record of closures) rather than at module level.
    func tm() : OrderedMap.Operations<Text> = OrderedMap.Make<Text>(Text.compare);
    func pm() : OrderedMap.Operations<Principal> = OrderedMap.Make<Principal>(Principal.compare);

    public func emptyState() : State = {
        bridgeAllowlist = [];
        policyTable = [];
        observations = tm().empty<NativeObservation>();
        obligations = tm().empty<NativeRewardObligation>();
    };

    // ================================================================
    // Ledger-facing types (structurally identical to the pinned PP ledger)
    // ================================================================

    public type TransferError = {
        #BadFee : { expected_fee : Nat };
        #BadBurn : { min_burn_amount : Nat };
        #InsufficientFunds : { balance : Nat };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    /// What the actor observed from the single `icrc1_transfer` await.
    public type TransferOutcome = {
        #Ok : Nat;
        #Err : TransferError;
        /// Trap / reject / timeout: the ledger may or may not have minted.
        #CallFailed : Text;
    };

    /// The exact arguments of the (single) icrc1_transfer. The actor adds
    /// `from_subaccount = null` and `fee = null`.
    public type TransferArg = {
        to : Account;
        amount : Nat;
        memo : Blob;
        created_at_time : Nat64;
    };

    // PP ledger query surface used for reconciliation (subsets of ledger.did).
    public type GetTransactionsRequest = { start : Nat; length : Nat };
    public type LedgerMint = { to : Account; memo : ?Blob; created_at_time : ?Nat64; amount : Nat };
    public type LedgerTransfer = { from : Account; to : Account; memo : ?Blob; created_at_time : ?Nat64; amount : Nat };
    public type LedgerTx = { kind : Text; mint : ?LedgerMint; transfer : ?LedgerTransfer };
    public type GetTransactionsResponse = { log_length : Nat; first_index : Nat; transactions : [LedgerTx] };
    public type TransactionRange = { transactions : [LedgerTx] };
    public type ArchiveInfo = { canister_id : Principal; block_range_start : Nat; block_range_end : Nat };

    public type LedgerQueryActor = actor {
        icrc1_minting_account : shared query () -> async ?Account;
        archives : shared query () -> async [ArchiveInfo];
        get_transactions : shared query GetTransactionsRequest -> async GetTransactionsResponse;
    };

    public type ArchiveQueryActor = actor {
        get_transactions : shared query GetTransactionsRequest -> async TransactionRange;
    };

    /// A transaction as read from `source` at `block_index`.
    public type LedgerReceipt = {
        source : Principal; // the canister the transaction was read from
        block_index : Nat; // the index the reader asked/answered for
        tx : ?LedgerTx;
    };

    public type ReconcileEnv = {
        ledger : Principal; // pinned PP ledger
        archives : [Principal]; // canister ids currently listed by ledger.archives()
        configuredMinter : Account; // {owner = this canister; subaccount = null}
        reportedMinter : ?Account; // ledger.icrc1_minting_account(), read just before
    };

    /// Pick the archive that holds `block` from the ledger's own `archives()`
    /// listing (ranges inclusive). Only listed archives can ever be selected.
    public func selectArchive(archives : [ArchiveInfo], block : Nat) : ?Principal {
        for (a in archives.vals()) {
            if (block >= a.block_range_start and block <= a.block_range_end) { return ?a.canister_id };
        };
        null;
    };

    // ================================================================
    // Effects snapshot (read-only view of the legacy referral/effect maps)
    // ================================================================

    public type SiphonView = {
        siphoner : Principal;
        pctTimes100 : Nat; // 1000 = 10%
        remainingCap : Nat; // capUnits - siphonedSoFar for an ACTIVE siphon
    };

    /// The actor builds this from its existing maps at binding time. All
    /// lookups are pure and must already account for expiry.
    public type EffectsSnapshot = {
        payoutTarget : Principal -> Principal; // referralChain.get(p) ?? house
        isActive : Principal -> Bool;
        house : Principal;
        mintMultiplierBps : Principal -> ?Nat; // active (unexpired) multiplier
        siphon : Principal -> ?SiphonView; // active, unsaturated siphon
        cascadeBoostBps : Principal -> ?Nat; // active (unexpired) Downline Boost
        cascadeDepthCap : Nat;
    };

    public type BindEnv = {
        self : Principal; // the Shenanigans canister (owner of the chip subaccounts)
        signupMarkers : SignupMarkers;
        effects : EffectsSnapshot;
    };

    // ================================================================
    // Small helpers
    // ================================================================

    func nat64OfInt(i : Int) : Nat64 = Nat64.fromNat(Int.abs(i));

    let HEX : [Text] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

    func hexOf(b : Blob) : Text {
        var t = "";
        for (x in b.vals()) {
            t #= HEX[Nat8.toNat(x / 16)] # HEX[Nat8.toNat(x % 16)];
        };
        t;
    };

    /// Canonical, unambiguous text key of the immutable event id.
    public func eventKey(id : EventId) : Text {
        "ev1|" # hexOf(id.cluster_domain) # "|" # id.program_id # "|" # id.signature # "|"
        # Nat.toText(id.instruction_index) # "|" # Nat.toText(id.inner_instruction_index) # "|"
        # Nat.toText(id.event_schema_version);
    };

    func hasPipe(t : Text) : Bool = Text.contains(t, #char '|');

    let BASE58 : Text = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    func isBase58Pubkey(t : Text) : Bool {
        let n = t.size();
        if (n < 32 or n > 44) { return false };
        for (c in t.chars()) {
            if (not Text.contains(BASE58, #char c)) { return false };
        };
        true;
    };

    func arrayHasNat(xs : [Nat], v : Nat) : Bool {
        for (x in xs.vals()) { if (x == v) { return true } };
        false;
    };

    // ================================================================
    // SHA-256 (FIPS 180-4). Pure, wrapping Nat32 arithmetic.
    // ================================================================

    let K256 : [Nat32] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    public func sha256(msg : Blob) : Blob {
        let data = Blob.toArray(msg);
        let len = data.size();
        let padLen = ((len + 9 + 63) / 64) * 64;
        let buf = Array.init<Nat8>(padLen, 0);
        var i = 0;
        while (i < len) { buf[i] := data[i]; i += 1 };
        buf[len] := 0x80;
        let bitLen : Nat64 = Nat64.fromNat(len) * 8;
        var k = 0;
        while (k < 8) {
            let shifted : Nat64 = bitLen >> (Nat64.fromNat(k) * 8);
            buf[padLen - 1 - k] := Nat8.fromNat(Nat64.toNat(shifted & 0xff));
            k += 1;
        };

        let h : [var Nat32] = [var 0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        let w = Array.init<Nat32>(64, 0);
        var blk = 0;
        while (blk < padLen / 64) {
            let base = blk * 64;
            var t = 0;
            while (t < 16) {
                let b0 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t]));
                let b1 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 1]));
                let b2 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 2]));
                let b3 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 3]));
                w[t] := (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                t += 1;
            };
            while (t < 64) {
                let w15 = w[t - 15];
                let w2 = w[t - 2];
                let s0 = (w15 <>> 7) ^ (w15 <>> 18) ^ (w15 >> 3);
                let s1 = (w2 <>> 17) ^ (w2 <>> 19) ^ (w2 >> 10);
                w[t] := w[t - 16] +% s0 +% w[t - 7] +% s1;
                t += 1;
            };
            var a = h[0];
            var b = h[1];
            var c = h[2];
            var d = h[3];
            var e = h[4];
            var f = h[5];
            var g = h[6];
            var hh = h[7];
            t := 0;
            while (t < 64) {
                let bigS1 = (e <>> 6) ^ (e <>> 11) ^ (e <>> 25);
                let ch = (e & f) ^ ((^e) & g);
                let temp1 = hh +% bigS1 +% ch +% K256[t] +% w[t];
                let bigS0 = (a <>> 2) ^ (a <>> 13) ^ (a <>> 22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = bigS0 +% maj;
                hh := g;
                g := f;
                f := e;
                e := d +% temp1;
                d := c;
                c := b;
                b := a;
                a := temp1 +% temp2;
                t += 1;
            };
            h[0] := h[0] +% a;
            h[1] := h[1] +% b;
            h[2] := h[2] +% c;
            h[3] := h[3] +% d;
            h[4] := h[4] +% e;
            h[5] := h[5] +% f;
            h[6] := h[6] +% g;
            h[7] := h[7] +% hh;
            blk += 1;
        };
        let out = Array.init<Nat8>(32, 0);
        var j = 0;
        while (j < 8) {
            let v = h[j];
            out[4 * j] := Nat8.fromNat(Nat32.toNat(v >> 24));
            out[4 * j + 1] := Nat8.fromNat(Nat32.toNat((v >> 16) & 0xff));
            out[4 * j + 2] := Nat8.fromNat(Nat32.toNat((v >> 8) & 0xff));
            out[4 * j + 3] := Nat8.fromNat(Nat32.toNat(v & 0xff));
            j += 1;
        };
        Blob.fromArrayMut(out);
    };

    /// Frozen leg memo: sha256(MEMO_DOMAIN || 0x00 || event key || 0x00 || be32(index)).
    public func legMemo(key : Text, index : Nat) : Blob {
        let idx = Nat32.fromNat(index);
        let tail : [Nat8] = [
            Nat8.fromNat(Nat32.toNat(idx >> 24)),
            Nat8.fromNat(Nat32.toNat((idx >> 16) & 0xff)),
            Nat8.fromNat(Nat32.toNat((idx >> 8) & 0xff)),
            Nat8.fromNat(Nat32.toNat(idx & 0xff)),
        ];
        let pre = Blob.toArray(Text.encodeUtf8(MEMO_DOMAIN # "\u{00}" # key # "\u{00}"));
        sha256(Blob.fromArray(Array.append<Nat8>(pre, tail)));
    };

    // ================================================================
    // Allowlist and append-only policy table
    // ================================================================

    public func isBridge(s : State, p : Principal) : Bool {
        for (b in s.bridgeAllowlist.vals()) { if (b == p) { return true } };
        false;
    };

    public func setBridgeAllowlist(s : State, list : [Principal]) : State = { s with bridgeAllowlist = list };

    public func currentPolicy(s : State) : ?PolicyVersion {
        let n = s.policyTable.size();
        if (n == 0) { null } else { ?s.policyTable[n - 1] };
    };

    public func getPolicy(s : State, version : Nat) : ?PolicyVersion {
        if (version == 0 or version > s.policyTable.size()) { return null };
        ?s.policyTable[version - 1];
    };

    public func appendPolicy(s : State, d : PolicyDraft, now : Int) : { #Ok : { state : State; version : Nat }; #Err : Text } {
        if (d.cascadeInitialBps > 10_000 or d.cascadePassthroughBps > 10_000) { return #Err("bps values must be <= 10_000") };
        if (d.signupGiftPp > MAX_SIGNUP_GIFT_PP) { return #Err("signupGiftPp must be <= 1_000_000 whole PP") };
        if (d.dedupWindowNs > MAX_DEDUP_WINDOW_NS) { return #Err("dedupWindowNs must not exceed the ledger's 24h transaction window") };
        if (d.dedupWindowNs <= 2 * DEDUP_SAFETY_MARGIN_NS) { return #Err("dedupWindowNs must exceed twice the retry safety margin") };
        let version = s.policyTable.size() + 1;
        let entry : PolicyVersion = {
            version = version;
            effectiveFrom = now;
            simple21DayPpPerSol = d.simple21DayPpPerSol;
            compounding15DayPpPerSol = d.compounding15DayPpPerSol;
            compounding30DayPpPerSol = d.compounding30DayPpPerSol;
            backerPpPerSol = d.backerPpPerSol;
            signupGiftPp = d.signupGiftPp;
            cascadeInitialBps = d.cascadeInitialBps;
            cascadePassthroughBps = d.cascadePassthroughBps;
            applyEffects = d.applyEffects;
            dedupWindowNs = d.dedupWindowNs;
        };
        #Ok({
            state = { s with policyTable = Array.append<PolicyVersion>(s.policyTable, [entry]) };
            version = version;
        });
    };

    // ================================================================
    // Queries
    // ================================================================

    public func getObligation(s : State, id : EventId) : ?NativeRewardObligation = tm().get(s.obligations, eventKey(id));

    public func getObservation(s : State, id : EventId) : ?NativeObservation = tm().get(s.observations, eventKey(id));

    // ================================================================
    // Step 1: observe
    // ================================================================

    public type ObserveResult = {
        #Observed : ObligationId;
        #AlreadyObserved : ObligationId;
        #Rejected : { reason : Text; permanent : Bool };
    };

    func validateEvent(ev : NativeEvent) : ?Text {
        let id = ev.event_id;
        if (not arrayHasNat(SUPPORTED_SCHEMA_VERSIONS, id.event_schema_version)) {
            return ?("unsupported event schema version " # Nat.toText(id.event_schema_version));
        };
        if (id.cluster_domain.size() == 0) { return ?"invalid cluster_domain (empty)" };
        if (id.program_id == "" or hasPipe(id.program_id)) { return ?"invalid program_id (empty or contains '|')" };
        if (id.signature == "" or hasPipe(id.signature)) { return ?"invalid signature (empty or contains '|')" };
        if (ev.basis_lamports == 0) { return ?"zero basis" };
        if (not isBase58Pubkey(ev.beneficiary_pubkey)) { return ?"malformed beneficiary pubkey (expected base58, 32-44 chars)" };
        switch (ev.kind) {
            case (#PositionOpened) {
                if (ev.plan == null) { return ?"PositionOpened requires a plan" };
                if (ev.source_close_id != null) { return ?"close id must be absent for PositionOpened" };
            };
            case (#SeriesAFunded) {
                if (ev.source_close_id != null) { return ?"close id must be absent for SeriesAFunded" };
            };
            case (#PromotionApplied) {
                if (ev.source_close_id == null) { return ?"close id is required for PromotionApplied" };
            };
        };
        null;
    };

    func sameEvent(o : NativeObservation, ev : NativeEvent) : Bool {
        o.event_id == ev.event_id and o.source_round == ev.source_round_id and o.source_close == ev.source_close_id
        and o.basis == ev.basis_lamports and o.beneficiary_pubkey == ev.beneficiary_pubkey and o.kind == ev.kind and o.plan == ev.plan;
    };

    /// Idempotent on the immutable event id. The first validated observation
    /// pins the CURRENT policy version. No effect is consumed and nothing is awaited.
    public func observe(s : State, ev : NativeEvent, now : Int) : (State, ObserveResult) {
        let key = eventKey(ev.event_id);
        switch (tm().get(s.observations, key)) {
            case (?existing) {
                if (sameEvent(existing, ev)) { return (s, #AlreadyObserved(key)) };
                return (s, #Rejected({ reason = "event id re-presented with different fields"; permanent = true }));
            };
            case null {};
        };
        switch (validateEvent(ev)) {
            case (?reason) { return (s, #Rejected({ reason = reason; permanent = true })) };
            case null {};
        };
        let policy = switch (currentPolicy(s)) {
            case (?p) { p };
            case null { return (s, #Rejected({ reason = "reward policy table is empty"; permanent = false })) };
        };
        let obs : NativeObservation = {
            event_id = ev.event_id;
            source_round = ev.source_round_id;
            source_close = ev.source_close_id;
            basis = ev.basis_lamports;
            beneficiary_pubkey = ev.beneficiary_pubkey;
            kind = ev.kind;
            plan = ev.plan;
            policy_version = policy.version;
            observed_at = now;
        };
        let obl : NativeRewardObligation = {
            event_id = ev.event_id;
            status = #Observed;
            policy_version = policy.version;
            beneficiary = null;
            reserved_at = null;
            legs = [];
        };
        (
            {
                s with
                observations = tm().put(s.observations, key, obs);
                obligations = tm().put(s.obligations, key, obl);
            },
            #Observed(key),
        );
    };

    // ================================================================
    // Step 2: bind and reserve (split around the single SIWS await)
    // ================================================================

    public type PrepareBindResult = {
        #Lookup : { address : Text; policy_version : Nat };
        #AlreadyReserved;
        #UnknownEvent;
    };

    /// Pre-await: what to look up. No state change.
    public func prepareBind(s : State, id : EventId) : PrepareBindResult {
        let key = eventKey(id);
        switch (tm().get(s.obligations, key), tm().get(s.observations, key)) {
            case (?obl, ?obs) {
                if (obl.status != #Observed) { return #AlreadyReserved };
                #Lookup({ address = obs.beneficiary_pubkey; policy_version = obs.policy_version });
            };
            case _ { #UnknownEvent };
        };
    };

    /// The shared canonical signup marker step. First claim wins and is never
    /// overwritten. `applyBind` runs it in the same atomic block as the
    /// reservation; the actor must not insert an await between them.
    public func claimSignupGiftMarker(m : SignupMarkers, p : Principal, now : Int) : (SignupMarkers, Bool) {
        switch (pm().get(m, p)) {
            case (?_) { (m, false) };
            case null { (pm().put(m, p, now), true) };
        };
    };

    public type BindResult = {
        #Reserved;
        #AlreadyReserved;
        #Unbound;
        #UnknownEvent;
        #Rejected : Text;
    };

    public type BindOutput = {
        state : State;
        signupMarkers : SignupMarkers;
        /// (holder, units) siphon cap consumed at this binding; the actor adds it
        /// to `mintSiphons[holder].siphonedSoFar`. Empty unless the bind reserved.
        siphonConsumed : [(Principal, Nat)];
        result : BindResult;
    };

    func ppPerSolFor(policy : PolicyVersion, kind : EventKind, plan : ?Plan) : ?Nat {
        switch (kind) {
            case (#PositionOpened) {
                switch (plan) {
                    case (?#Simple) { ?policy.simple21DayPpPerSol };
                    case (?#Compound15) { ?policy.compounding15DayPpPerSol };
                    case (?#Compound30) { ?policy.compounding30DayPpPerSol };
                    case null { null };
                };
            };
            // Series A funding and the Series B promotion loss are both Backer-class bases.
            case (#SeriesAFunded) { ?policy.backerPpPerSol };
            case (#PromotionApplied) { ?policy.backerPpPerSol };
        };
    };

    /// gross lamports x PP-per-SOL, in PP units (1 SOL x 6_000 = 6_000 PP = 6e11 units).
    func baseUnitsOf(lamports : Nat64, ppPerSol : Nat) : Nat {
        Nat64.toNat(lamports) * ppPerSol * PP_UNIT_SCALE / LAMPORTS_PER_SOL;
    };

    type Computed = { legs : [Leg]; siphonConsumed : [(Principal, Nat)] };

    func computeLegs(env : BindEnv, policy : PolicyVersion, obs : NativeObservation, key : Text, beneficiary : Principal, giftLeg : Bool, ppPerSol : Nat, now : Int) : Computed {
        let createdAt = nat64OfInt(now);
        let legs = Buffer.Buffer<Leg>(8);
        var consumed = pm().empty<Nat>();
        let fx = env.effects;

        func addLeg(holder : Principal, role : LegRole, amount : Nat) {
            if (amount == 0) { return }; // the legacy mint path treats a zero mint as a no-op too
            let index = legs.size();
            legs.add({
                index = index;
                recipient = { owner = env.self; subaccount = ?Subaccount.principalToChipSubaccount(holder) };
                holder = holder;
                role = role;
                amount = amount;
                fee = null;
                memo = legMemo(key, index);
                created_at_time = createdAt;
                attempts = 0;
                status = #Pending;
            });
        };

        // mintWithEffects equivalent: multiplier first, then the capped siphon.
        func emit(holder : Principal, role : LegRole, amount : Nat) {
            if (amount == 0) { return };
            var toHolder = amount;
            var siphonLeg : ?(Principal, Nat) = null;
            if (policy.applyEffects) {
                let multiplied : Nat = switch (fx.mintMultiplierBps(holder)) {
                    case (?bps) { amount * bps / 10_000 };
                    case null { amount };
                };
                toHolder := multiplied;
                switch (fx.siphon(holder)) {
                    case (?sv) {
                        let already : Nat = switch (pm().get(consumed, holder)) { case (?n) { n }; case null { 0 } };
                        let capLeft : Nat = if (sv.remainingCap > already) { sv.remainingCap - already } else { 0 };
                        let raw = multiplied * sv.pctTimes100 / 10_000;
                        let take = if (raw < capLeft) { raw } else { capLeft };
                        if (take > 0) {
                            consumed := pm().put(consumed, holder, already + take);
                            toHolder := if (multiplied >= take) { multiplied - take } else { 0 };
                            siphonLeg := ?(sv.siphoner, take);
                        };
                    };
                    case null {};
                };
            };
            addLeg(holder, role, toHolder);
            switch (siphonLeg) {
                case (?(siphoner, take)) { addLeg(siphoner, #siphon({ from = holder }), take) };
                case null {};
            };
        };

        // distributeDeductiveCascade equivalent (no awaits; every payout is a leg).
        func cascade(origin : Principal, cascadeUnits : Nat, forGift : Bool) {
            if (cascadeUnits == 0) { return };
            var remaining = cascadeUnits;
            var visited = pm().put(pm().empty<()>(), origin, ());
            var depth = 0;
            var activeRank = 0;
            var current = origin;
            label walk loop {
                if (remaining == 0 or depth >= fx.cascadeDepthCap) { break walk };
                let next = fx.payoutTarget(current);
                switch (pm().get(visited, next)) {
                    case (?_) { break walk }; // cycle: bail to residual
                    case null {};
                };
                visited := pm().put(visited, next, ());
                depth += 1;
                if (not fx.isActive(next)) {
                    current := next;
                    continue walk;
                };
                activeRank += 1;
                let basePayout = remaining * policy.cascadePassthroughBps / 10_000;
                if (basePayout == 0) { break walk };
                let payout : Nat = switch (fx.cascadeBoostBps(next)) {
                    case (?bps) { basePayout * bps / 10_000 };
                    case null { basePayout };
                };
                emit(next, #cascade({ rank = activeRank; gross = payout; forGift = forGift }), payout);
                // pre-boost decrement keeps the distribution conservative (as legacy)
                remaining -= basePayout;
                current := next;
            };
            if (remaining > 0) { emit(fx.house, #residual({ forGift = forGift }), remaining) };
        };

        // Legacy order: signup gift (and its cascade) first, then the event itself.
        if (giftLeg) {
            let giftBase = policy.signupGiftPp * PP_UNIT_SCALE;
            let giftCascade = giftBase * policy.cascadeInitialBps / 10_000;
            let giftNet : Nat = if (giftBase > giftCascade) { giftBase - giftCascade } else { 0 };
            emit(beneficiary, #signupGift, giftNet);
            cascade(beneficiary, giftCascade, true);
        };
        let baseUnits = baseUnitsOf(obs.basis, ppPerSol);
        let cascadeUnits = baseUnits * policy.cascadeInitialBps / 10_000;
        let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
        emit(beneficiary, #player, playerNet);
        cascade(beneficiary, cascadeUnits, false);

        { legs = Buffer.toArray(legs); siphonConsumed = Iter.toArray(pm().entries(consumed)) };
    };

    func recomputeStatus(current : ObligationStatus, legs : [Leg]) : ObligationStatus {
        if (current == #Observed) { return #Observed };
        var committed = 0;
        var quarantined = 0;
        for (l in legs.vals()) {
            switch (l.status) {
                case (#Committed _) { committed += 1 };
                case (#Quarantined _) { quarantined += 1 };
                case _ {};
            };
        };
        if (committed == legs.size()) { #Completed } else if (committed + quarantined == legs.size()) { #PartiallyQuarantined } else { #Reserved };
    };

    /// Post-await: ONE atomic, synchronous step. Re-reads the obligation (another
    /// caller may have bound it while this one awaited SIWS), then claims the
    /// signup marker, computes every leg once under the pinned policy version,
    /// consumes effect/siphon caps and freezes the legs. The actor must commit
    /// the returned state, markers and siphon consumption without any await in between.
    public func applyBind(s : State, env : BindEnv, id : EventId, lookup : ?Principal, now : Int) : BindOutput {
        func unchanged(r : BindResult) : BindOutput = {
            state = s;
            signupMarkers = env.signupMarkers;
            siphonConsumed = [];
            result = r;
        };
        let key = eventKey(id);
        switch (tm().get(s.obligations, key), tm().get(s.observations, key)) {
            case (?obl, ?obs) {
                if (obl.status != #Observed) { return unchanged(#AlreadyReserved) };
                let beneficiary = switch (lookup) {
                    case null { return unchanged(#Unbound) };
                    case (?p) { p };
                };
                if (Principal.isAnonymous(beneficiary)) { return unchanged(#Unbound) };
                let policy = switch (getPolicy(s, obs.policy_version)) {
                    case (?p) { p };
                    case null { return unchanged(#Rejected("pinned policy version " # Nat.toText(obs.policy_version) # " is missing")) };
                };
                let ppPerSol = switch (ppPerSolFor(policy, obs.kind, obs.plan)) {
                    case (?r) { r };
                    case null { return unchanged(#Rejected("observation has no rate (PositionOpened without a plan)")) };
                };
                // Signup gift only on a PositionOpened (a backer-class event never gifts, as in the
                // legacy observer). The marker is claimed in THIS atomic step.
                let giftWanted = obs.kind == #PositionOpened and policy.signupGiftPp > 0;
                let (markers, giftLeg) = if (giftWanted) { claimSignupGiftMarker(env.signupMarkers, beneficiary, now) } else { (env.signupMarkers, false) };
                let computed = computeLegs({ env with signupMarkers = markers }, policy, obs, key, beneficiary, giftLeg, ppPerSol, now);
                let reserved : NativeRewardObligation = {
                    obl with
                    status = recomputeStatus(#Reserved, computed.legs);
                    beneficiary = ?beneficiary;
                    reserved_at = ?now;
                    legs = computed.legs;
                };
                {
                    state = { s with obligations = tm().put(s.obligations, key, reserved) };
                    signupMarkers = markers;
                    siphonConsumed = computed.siphonConsumed;
                    result = #Reserved;
                };
            };
            case _ { unchanged(#UnknownEvent) };
        };
    };

    // ================================================================
    // Step 3: execute / retry / outcome
    // ================================================================

    func describeError(e : TransferError) : Text {
        switch (e) {
            case (#BadFee(x)) { "BadFee expected=" # Nat.toText(x.expected_fee) };
            case (#BadBurn(x)) { "BadBurn min=" # Nat.toText(x.min_burn_amount) };
            case (#InsufficientFunds(x)) { "InsufficientFunds balance=" # Nat.toText(x.balance) };
            case (#TooOld) { "TooOld" };
            case (#CreatedInFuture(_)) { "CreatedInFuture" };
            case (#Duplicate(x)) { "Duplicate of=" # Nat.toText(x.duplicate_of) };
            case (#TemporarilyUnavailable) { "TemporarilyUnavailable" };
            case (#GenericError(x)) { "GenericError: " # x.message };
        };
    };

    func windowOf(s : State, policyVersion : Nat) : Nat {
        switch (getPolicy(s, policyVersion)) {
            case (?p) { p.dedupWindowNs };
            case null { 0 }; // unreachable (append-only table); fail closed
        };
    };

    /// True while at least DEDUP_SAFETY_MARGIN_NS of the verified ledger dedup
    /// window remains for this leg's frozen created_at_time.
    public func inDedupWindow(leg : Leg, windowNs : Nat, now : Int) : Bool {
        now + DEDUP_SAFETY_MARGIN_NS <= Nat64.toNat(leg.created_at_time) + windowNs;
    };

    func updateLeg(s : State, key : Text, idx : Nat, f : Leg -> Leg) : State {
        switch (tm().get(s.obligations, key)) {
            case null { s };
            case (?obl) {
                let legs = Array.tabulate<Leg>(obl.legs.size(), func(i) { if (i == idx) { f(obl.legs[i]) } else { obl.legs[i] } });
                let updated : NativeRewardObligation = { obl with legs = legs; status = recomputeStatus(obl.status, legs) };
                { s with obligations = tm().put(s.obligations, key, updated) };
            };
        };
    };

    public type BeginResult = {
        /// Proceed: call icrc1_transfer with exactly these arguments. The leg is
        /// already stored as #SubmittedUnknown in the returned state.
        #Submit : TransferArg;
        /// Nothing to do (committed, quarantined, or not in the entry point's state set).
        #Skip : { status : LegStatus; reason : Text };
        /// The dedup window has passed: the leg was moved to Quarantined{reconcile}; no transfer.
        #Expired : LegStatus;
        #UnknownLeg;
        #NotReserved;
    };

    func beginSubmit(s : State, id : EventId, idx : Nat, mode : { #Execute; #Retry }, now : Int) : (State, BeginResult) {
        let key = eventKey(id);
        let obl = switch (tm().get(s.obligations, key)) {
            case (?o) { o };
            case null { return (s, #NotReserved) };
        };
        if (obl.status == #Observed) { return (s, #NotReserved) };
        if (idx >= obl.legs.size()) { return (s, #UnknownLeg) };
        let leg = obl.legs[idx];
        let allowed = switch (mode, leg.status) {
            case (#Execute, #Pending) { true };
            case (#Execute, #SubmittedUnknown) { true };
            case (#Retry, #Retryable _) { true };
            case (#Retry, #SubmittedUnknown) { true };
            case _ { false };
        };
        if (not allowed) {
            let reason = switch (leg.status) {
                case (#Committed _) { "leg already committed" };
                case (#Quarantined _) { "leg is quarantined; only ledger-history reconciliation can complete it" };
                case (#Retryable _) { "leg is Retryable; use retryNativeRewardLeg" };
                case (#Pending) { "leg was never submitted; use executeNativeRewardLeg" };
                case (#SubmittedUnknown) { "leg in unexpected state" };
            };
            return (s, #Skip({ status = leg.status; reason = reason }));
        };
        if (not inDedupWindow(leg, windowOf(s, obl.policy_version), now)) {
            let q : LegStatus = #Quarantined({ reason = "past the ledger dedup window; reconcile from ledger history (reconcileLegFromLedger)"; recoverable = true });
            return (updateLeg(s, key, idx, func(l) { { l with status = q } }), #Expired(q));
        };
        let s2 = updateLeg(s, key, idx, func(l) { { l with status = #SubmittedUnknown; attempts = l.attempts + 1 } });
        (s2, #Submit({ to = leg.recipient; amount = leg.amount; memo = leg.memo; created_at_time = leg.created_at_time }));
    };

    /// Execute entry point (bridge/keeper): Pending or SubmittedUnknown legs.
    /// The returned state (leg = #SubmittedUnknown) MUST be committed before the transfer await.
    public func beginExecute(s : State, id : EventId, idx : Nat, now : Int) : (State, BeginResult) = beginSubmit(s, id, idx, #Execute, now);

    /// Retry entry point (permissionless): Retryable or SubmittedUnknown legs, identical frozen fields.
    public func beginRetry(s : State, id : EventId, idx : Nat, now : Int) : (State, BeginResult) = beginSubmit(s, id, idx, #Retry, now);

    public type OutcomeResult = { status : LegStatus; newlyCommitted : Bool };

    func memoRejected(msg : Text) : Bool {
        Text.contains(msg, #text "memo") or Text.contains(msg, #text "Memo") or Text.contains(msg, #text "MEMO");
    };

    /// Apply the result of the single icrc1_transfer await.
    public func applyTransferOutcome(s : State, id : EventId, idx : Nat, o : TransferOutcome, now : Int) : (State, OutcomeResult) {
        let key = eventKey(id);
        let obl = switch (tm().get(s.obligations, key)) {
            case (?x) { x };
            case null { Debug.trap("NativeRewards.applyTransferOutcome: unknown obligation") };
        };
        if (idx >= obl.legs.size()) { Debug.trap("NativeRewards.applyTransferOutcome: unknown leg") };
        let leg = obl.legs[idx];
        func keep() : (State, OutcomeResult) = (s, { status = leg.status; newlyCommitted = false });
        func commit(block : Nat) : (State, OutcomeResult) {
            let c : LegStatus = #Committed({ block = block });
            (updateLeg(s, key, idx, func(l) { { l with status = c } }), { status = c; newlyCommitted = true });
        };
        func set(st : LegStatus) : (State, OutcomeResult) {
            (updateLeg(s, key, idx, func(l) { { l with status = st } }), { status = st; newlyCommitted = false });
        };
        switch (leg.status) {
            case (#Committed _) { return keep() }; // a committed leg is never rewritten
            case _ {};
        };
        switch (o) {
            case (#Ok(block)) { commit(block) };
            case (#Err(#Duplicate(d))) { commit(d.duplicate_of) };
            case (#CallFailed(_)) { keep() }; // stays #SubmittedUnknown; the next call re-presents identical fields
            case (#Err(e)) {
                // A failure applies only to the in-flight submission it answers.
                if (leg.status != #SubmittedUnknown) { return keep() };
                switch (e) {
                    case (#TooOld) {
                        set(#Quarantined({ reason = "TooOld: ledger window passed; reconcile from ledger history"; recoverable = true }));
                    };
                    case (#CreatedInFuture(_)) {
                        set(#Quarantined({ reason = "CreatedInFuture: reconcile from ledger history"; recoverable = true }));
                    };
                    case (#BadBurn(_)) {
                        set(#Quarantined({ reason = "permanent: " # describeError(e) # " (immutable-field rejection; needs reviewed upgrade, never a replacement timestamp or recipient)"; recoverable = false }));
                    };
                    case (#GenericError(g)) {
                        if (memoRejected(g.message)) {
                            set(#Quarantined({ reason = "permanent: ledger rejected the frozen memo (" # describeError(e) # ")"; recoverable = false }));
                        } else {
                            definite(s, key, idx, leg, obl.policy_version, e, now);
                        };
                    };
                    case _ { definite(s, key, idx, leg, obl.policy_version, e, now) };
                };
            };
        };
    };

    // Definite rejection: nothing was minted. Retryable while the window lasts,
    // otherwise only ledger history can settle it.
    func definite(s : State, key : Text, idx : Nat, leg : Leg, policyVersion : Nat, e : TransferError, now : Int) : (State, OutcomeResult) {
        let st : LegStatus = if (inDedupWindow(leg, windowOf(s, policyVersion), now)) {
            #Retryable({ reason = describeError(e); attempts = leg.attempts });
        } else {
            #Quarantined({ reason = describeError(e) # "; past the ledger dedup window, reconcile from ledger history"; recoverable = true });
        };
        (updateLeg(s, key, idx, func(l) { { l with status = st } }), { status = st; newlyCommitted = false });
    };

    // ================================================================
    // Step 4: reconcile from ledger history
    // ================================================================

    public type ReconcileResult = {
        #Committed : OutcomeResult;
        #Rejected : Text;
        #UnknownLeg; // unknown event id or leg index
    };

    func accountEq(a : Account, b : Account) : Bool = a.owner == b.owner and a.subaccount == b.subaccount;

    func isListed(env : ReconcileEnv, p : Principal) : Bool {
        if (p == env.ledger) { return true };
        for (a in env.archives.vals()) { if (a == p) { return true } };
        false;
    };

    /// Commit a leg only from an exact `mint` record. Every precondition is
    /// checked before the leg is read for commitment, and the configured minting
    /// Account must equal the one the ledger reports.
    public func reconcileFromLedgerBlock(s : State, id : EventId, idx : Nat, blockIndex : Nat, receipt : LedgerReceipt, env : ReconcileEnv, _now : Int) : (State, ReconcileResult) {
        let key = eventKey(id);
        let obl = switch (tm().get(s.obligations, key)) {
            case (?o) { o };
            case null { return (s, #UnknownLeg) };
        };
        if (obl.status == #Observed or idx >= obl.legs.size()) { return (s, #UnknownLeg) };
        let leg = obl.legs[idx];
        // 1. minting Account: configured must equal what the ledger reports right now
        switch (env.reportedMinter) {
            case (?m) {
                if (not accountEq(m, env.configuredMinter)) { return (s, #Rejected("minting account mismatch: the ledger's icrc1_minting_account differs from the configured one")) };
            };
            case null { return (s, #Rejected("minting account mismatch: the ledger reports no minting account")) };
        };
        // 2. the record must come from the pinned ledger or an archive it lists
        if (not isListed(env, receipt.source)) { return (s, #Rejected("receipt source is neither the pinned ledger nor an archive listed by archives()")) };
        // 3. the answer must be for the block that was asked about
        if (receipt.block_index != blockIndex) {
            return (s, #Rejected("receipt block index " # Nat.toText(receipt.block_index) # " does not match requested block " # Nat.toText(blockIndex)));
        };
        let tx = switch (receipt.tx) {
            case (?t) { t };
            case null { return (s, #Rejected("no transaction at block " # Nat.toText(blockIndex))) };
        };
        // 4. kind must be exactly "mint"
        if (tx.kind != "mint") { return (s, #Rejected("wrong transaction kind \"" # tx.kind # "\" (expected \"mint\")")) };
        let m = switch (tx.mint) {
            case (?m) { m };
            case null { return (s, #Rejected("mint transaction has no mint body")) };
        };
        // 5. exact recipient owner + subaccount, amount, memo and created_at_time
        if (not accountEq(m.to, leg.recipient)) { return (s, #Rejected("mint recipient (owner+subaccount) differs from the frozen leg")) };
        if (m.amount != leg.amount) { return (s, #Rejected("mint amount differs from the frozen leg")) };
        if (m.memo != ?leg.memo) { return (s, #Rejected("mint memo differs from the frozen leg")) };
        if (m.created_at_time != ?leg.created_at_time) { return (s, #Rejected("mint created_at_time differs from the frozen leg")) };
        switch (leg.status) {
            case (#Committed _) { return (s, #Committed({ status = leg.status; newlyCommitted = false })) };
            case _ {};
        };
        let c : LegStatus = #Committed({ block = blockIndex });
        (updateLeg(s, key, idx, func(l) { { l with status = c } }), #Committed({ status = c; newlyCommitted = true }));
    };
};
