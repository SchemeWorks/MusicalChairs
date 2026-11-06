import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Cycles "mo:base/ExperimentalCycles";
import Error "mo:base/Error";
import Int "mo:base/Int";
import Nat "mo:base/Nat";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";
import Principal "mo:base/Principal";
import Text "mo:base/Text";
import Time "mo:base/Time";
import Timer "mo:base/Timer";

import Accounts "Accounts";
import AuthorityTable "AuthorityTable";
import Base58 "Base58";
import Base64 "Base64";
import Gate "Gate";
import Journal "Journal";
import Mgmt "Mgmt";
import Pda "Pda";
import Planner "Planner";
import Policy "Policy";
import Rpc "Rpc";
import Tx "Tx";

/// native_sol_keeper - the ICP WATCHDOG keeper for the native Solana program
/// (plan M13, spec section 7). Thin glue: every decision lives in the pure,
/// separately tested modules (Policy, Journal, Planner, Accounts, Tx, Pda).
///
/// WHAT IT IS. An EMERGENCY BACKUP for the external fee-only worker. A recurring
/// timer polls the program's progress markers (round id, allocation ticket
/// phase/cursor, close stage) through the SOL RPC canister. After
/// `signAfterNs` (default 300 s) without progress it signs ONE keeper step with a
/// dedicated threshold Ed25519 key and submits it. Alert (60 s) and escalation
/// (300 s) are monitoring signals for `getStatus`, not completion guarantees.
///
/// TRUST STATEMENT.
///  * The fee address is derived once from the management canister's threshold
///    Ed25519 key (`schnorr_public_key`, derivation path below). It is only ever
///    the transaction FEE PAYER (message account 0). No supported instruction has a
///    signer or payer account (`test/authority.test.mo`), so the key can pay network
///    fees and nothing else; it holds only the operations SOL an operator sends it,
///    never user, pool, governance, guardian, provider or management authority.
///  * `sign_with_schnorr` is called from exactly one private function
///    (`signMessage`) reached only from `tick`; the message it signs is compiled
///    from a `Planner` step and re-parsed first. NO public method takes bytes to sign.
///  * Caps: it refuses to sign when the fee address balance is below the configured
///    floor, when the hourly signature budget is spent, when a transaction fee would
///    exceed `maxFeeLamportsPerTx`, while any journaled transaction is unresolved,
///    and after `maxAttemptsPerPosition` failed attempts at one position.
///  * A malicious or compromised CONTROLLER can reconfigure RPC sources and
///    thresholds, drain the fee address balance into fees, or point the keeper at
///    another program id; none of that changes economic state, which only the
///    Solana program's public rules can do. Two RPC providers are an observation
///    boundary, not consensus (`Config.rpcConsensus`).
///  * No outbound HTTP of its own: all chain I/O goes through the SOL RPC canister.
///  * Public recovery stays callable by any wallet; the watchdog adds liveness, it
///    is never required.
///
/// WHAT IT DOES NOT DO: provision pages, build candidates, request/capture/apply
/// the randomized promotion (they need a rent-paying payer and oracle accounts).
/// Those steps are `#Blocked "unsupported step: ..."`; the watchdog covers the
/// cash path (ticket phases) and the deadline path (`expire_promotion`,
/// `finalize_round`). See `docs/solana/operations.md`.
persistent actor NativeSolKeeper {
    // ------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------

    public type Config = {
        /// SOL RPC sources (custom endpoints may embed API keys: `getConfig` hides them).
        rpcSources : Rpc.RpcSources;
        rpcConsensus : Rpc.ConsensusStrategy;
        /// Native program id (base58).
        programId : Text;
        /// 32-byte `Pool.cluster_domain`; the keeper refuses to act on any other cluster.
        clusterDomain : Blob;
        /// Threshold key id name: `key_1` (mainnet), `test_key_1`, or `dfx_test_key` (local).
        feeKeyName : Text;
        alertAfterNs : Nat;
        signAfterNs : Nat;
        /// Poll cadence while work is pending / while idle.
        activePollNs : Nat;
        idlePollNs : Nat;
        maxFeeLamportsPerTx : Nat64;
        maxSignaturesPerHour : Nat;
        /// Refuse to sign when the fee address holds less than this (plus the fee).
        minFeeBalanceLamports : Nat64;
        journalMax : Nat;
        maxAttemptsPerPosition : Nat;
        /// 1..8 rows per batch; 1 = permanent one-row mode.
        maxRows : Nat;
        /// Read each open row's live Backer to prune rows capped by this ticket.
        pruneNewlyCapped : Bool;
        /// 0 = no priority fee (and no compute-budget instructions).
        computeUnitPriceMicroLamports : Nat64;
        /// Clock-skew slack before `expire_promotion` is sent.
        deadlineMarginSecs : Nat64;
        /// Cycles attached to each SOL RPC call / each threshold signature (unspent part is refunded).
        rpcCycles : Nat;
        signCycles : Nat;
    };

    /// `getConfig` omits the RPC sources.
    public type PublicConfig = {
        programId : Text;
        clusterDomain : Blob;
        feeKeyName : Text;
        alertAfterNs : Nat;
        signAfterNs : Nat;
        activePollNs : Nat;
        idlePollNs : Nat;
        maxFeeLamportsPerTx : Nat64;
        maxSignaturesPerHour : Nat;
        minFeeBalanceLamports : Nat64;
        journalMax : Nat;
        maxAttemptsPerPosition : Nat;
        maxRows : Nat;
        pruneNewlyCapped : Bool;
        computeUnitPriceMicroLamports : Nat64;
        deadlineMarginSecs : Nat64;
        rpcCycles : Nat;
        signCycles : Nat;
    };

    public type FeeAddress = { keyName : Text; publicKey : Blob; address : Text };

    public type JournalView = {
        signature : Text;
        step : Text;
        position : Text;
        rows : Nat;
        blockhash : Text;
        lastValidBlockHeight : Nat64;
        status : Journal.Status;
        at : Nat;
        updatedAt : Nat;
        note : Text;
    };

    public type Status = {
        /// What an external monitor should page on.
        level : Policy.Level;
        staleForNs : Nat;
        workPending : Bool;
        lastSignature : ?Text;
        journalTail : [JournalView];
        armed : Bool;
        configured : Bool;
        feeAddress : ?Text;
        /// Last tick's decision, e.g. "#Alert", "#Sign", "#Blocked(...)".
        decision : Text;
        blockedReason : ?Text;
        lastOutcome : ?Text;
        lastError : ?Text;
        /// Last poll that actually ran (cadence: `activePollNs` / `idlePollNs`).
        lastTickAt : Nat;
        /// Last time the recurring timer fired (every 15 s while armed) and its age.
        lastTimerAt : Nat;
        timerAgeNs : Nat;
        signaturesLastHour : Nat;
        marker : Text;
        cyclesBalance : Nat;
        nowNs : Nat;
    };

    // ------------------------------------------------------------------
    // State (stable)
    // ------------------------------------------------------------------

    var config : ?Config = null;
    var feeAddress : ?FeeAddress = null;
    /// The stable flag that survives an upgrade; timers do not (re-armed in postupgrade).
    var armed : Bool = false;
    var journal : Journal.Journal = Journal.empty();
    var tracker : Policy.Tracker = Policy.newTracker(0);
    var lastSignedAt : Nat = 0;
    var signTimes : [Nat] = [];
    var nextPollAt : Nat = 0;
    var lastTickAt : Nat = 0;
    var lastTimerAt : Nat = 0;
    var lastDecision : Text = "never ticked";
    var blockedReason : ?Text = null;
    /// One line about the last transaction attempt ("submitted ...", "rejected ...", "blocked: ...").
    var lastOutcome : ?Text = null;
    var lastError : ?Text = null;
    var workPending : Bool = false;
    var lastMarker : Text = "";

    // ------------------------------------------------------------------
    // Transient
    // ------------------------------------------------------------------

    transient let solRpc : Rpc.RpcActor = actor (Rpc.SOL_RPC_CANISTER_ID);
    transient let mgmt : Mgmt.Management = actor (Mgmt.MANAGEMENT_CANISTER);
    transient var timerId : ?Timer.TimerId = null;
    transient var tickStartedAt : Nat = 0;
    transient var deriving : Bool = false;
    transient var addrCache : ?(Text, Pda.Addresses) = null;

    /// Derivation path of the fee key (domain separation from any other use of the canister's key).
    transient let FEE_DERIVATION_PATH : Blob = Text.encodeUtf8("musicalchairs/native-sol-keeper/fee/v1");
    /// The recurring timer fires this often; actual work happens when `Policy.due`.
    transient let TIMER_PERIOD_NS : Nat = 15_000_000_000;
    /// A tick that has run longer than this is treated as dead (a trap after an await leaves the flag set).
    transient let TICK_TIMEOUT_NS : Nat = 300_000_000_000;
    transient let HOUR_NS : Nat = 3_600_000_000_000;
    /// A landing within this window explains a progress-marker change as OURS (rescue continuity).
    transient let OWN_LANDING_WINDOW_NS : Nat = 120_000_000_000;
    /// A transaction's blockhash is valid for 150 blocks past the block it names.
    transient let BLOCKHASH_VALIDITY : Nat64 = 150;

    func nowNs() : Nat { Int.abs(Time.now()) };

    func defaultConfig() : Config {
        {
            rpcSources = #Default(#Devnet);
            rpcConsensus = #Equality;
            programId = AuthorityTable.PROGRAM_ID;
            clusterDomain = Blob.fromArray(Array.tabulate<Nat8>(32, func _ { 0 }));
            feeKeyName = "dfx_test_key";
            alertAfterNs = Policy.ALERT_AFTER_NS;
            signAfterNs = Policy.SIGN_AFTER_NS;
            activePollNs = 30_000_000_000;
            idlePollNs = 300_000_000_000;
            maxFeeLamportsPerTx = 10_000;
            maxSignaturesPerHour = 60;
            minFeeBalanceLamports = 10_000_000;
            journalMax = 64;
            maxAttemptsPerPosition = 3;
            maxRows = 8;
            pruneNewlyCapped = false;
            computeUnitPriceMicroLamports = 0;
            deadlineMarginSecs = 15;
            rpcCycles = 10_000_000_000;
            signCycles = 30_000_000_000;
        };
    };

    func publicConfig(c : Config) : PublicConfig {
        {
            programId = c.programId;
            clusterDomain = c.clusterDomain;
            feeKeyName = c.feeKeyName;
            alertAfterNs = c.alertAfterNs;
            signAfterNs = c.signAfterNs;
            activePollNs = c.activePollNs;
            idlePollNs = c.idlePollNs;
            maxFeeLamportsPerTx = c.maxFeeLamportsPerTx;
            maxSignaturesPerHour = c.maxSignaturesPerHour;
            minFeeBalanceLamports = c.minFeeBalanceLamports;
            journalMax = c.journalMax;
            maxAttemptsPerPosition = c.maxAttemptsPerPosition;
            maxRows = c.maxRows;
            pruneNewlyCapped = c.pruneNewlyCapped;
            computeUnitPriceMicroLamports = c.computeUnitPriceMicroLamports;
            deadlineMarginSecs = c.deadlineMarginSecs;
            rpcCycles = c.rpcCycles;
            signCycles = c.signCycles;
        };
    };

    func configError(c : Config) : ?Text {
        if (not Base58.isOfLength(c.programId, 32)) { return ?"programId must be a base58 32-byte key" };
        if (c.clusterDomain.size() != 32) { return ?"clusterDomain must be 32 bytes" };
        if (c.feeKeyName == "") { return ?"feeKeyName must not be empty" };
        if (c.alertAfterNs == 0) { return ?"alertAfterNs must be positive" };
        if (c.signAfterNs < c.alertAfterNs) { return ?"signAfterNs must be at least alertAfterNs" };
        if (c.activePollNs == 0 or c.idlePollNs == 0) { return ?"poll intervals must be positive" };
        if (c.maxFeeLamportsPerTx < Tx.BASE_FEE_PER_SIGNATURE) { return ?"maxFeeLamportsPerTx must cover the 5000-lamport base fee" };
        if (c.journalMax < 4) { return ?"journalMax must be at least 4" };
        if (c.maxAttemptsPerPosition == 0) { return ?"maxAttemptsPerPosition must be at least 1" };
        if (c.maxRows < 1 or c.maxRows > 8) { return ?"maxRows must be in 1..8" };
        switch (c.rpcSources) {
            case (#Custom(xs)) { if (xs.size() == 0) { return ?"rpcSources: the custom list must not be empty" } };
            case (#Default(_)) {};
        };
        null;
    };

    func requireController(caller : Principal) : Bool { Principal.isController(caller) };

    // ------------------------------------------------------------------
    // Administration (controllers only)
    // ------------------------------------------------------------------

    public query func configTemplate() : async Config { defaultConfig() };

    public shared ({ caller }) func setConfig(c : Config) : async { #ok; #err : Text } {
        if (not requireController(caller)) { return #err("controllers only") };
        switch (configError(c)) {
            case (?m) { return #err(m) };
            case (null) {};
        };
        // A different key name means a different address: it must be derived again.
        switch (feeAddress) {
            case (?f) { if (f.keyName != c.feeKeyName) { feeAddress := null } };
            case (null) {};
        };
        config := ?c;
        addrCache := null;
        #ok;
    };

    public query func getConfig() : async ?PublicConfig {
        switch (config) { case (null) { null }; case (?c) { ?publicConfig(c) } };
    };

    func timerState() : Policy.TimerState { { armed; timerLive = timerId != null } };

    func startTimer<system>() {
        timerId := ?Timer.recurringTimer<system>(#nanoseconds TIMER_PERIOD_NS, onTimer);
    };

    func applyTimer<system>(e : Policy.TimerEvent) {
        let (next, action) = Policy.timerStep(timerState(), config != null, e);
        armed := next.armed;
        switch (action) {
            case (#Start) { startTimer<system>() };
            case (#Cancel) {
                switch (timerId) { case (?id) { Timer.cancelTimer(id) }; case (null) {} };
                timerId := null;
            };
            case (#None) {};
        };
    };

    /// Start the recurring timer and set the stable `armed` flag (re-armed in `postupgrade`).
    public shared ({ caller }) func arm() : async { #ok; #err : Text } {
        if (not requireController(caller)) { return #err("controllers only") };
        if (config == null) { return #err("not configured") };
        if (feeAddress == null) { return #err("derive the fee address first (getFeeAddress)") };
        applyTimer<system>(#Arm);
        #ok;
    };

    public shared ({ caller }) func disarm() : async { #ok; #err : Text } {
        if (not requireController(caller)) { return #err("controllers only") };
        applyTimer<system>(#Disarm);
        #ok;
    };

    /// Timers are not persisted across upgrades of a persistent actor: re-arm from the stable flag.
    system func postupgrade() {
        applyTimer<system>(#PostUpgrade);
    };

    // ------------------------------------------------------------------
    // The fee address (derived ONCE, stored)
    // ------------------------------------------------------------------

    /// The dedicated fee-only address. Derived on the first call from the threshold
    /// Ed25519 key (`schnorr_public_key`, `FEE_DERIVATION_PATH`) and stored; later
    /// calls return the stored value. Fund it with operations SOL only.
    public shared func getFeeAddress() : async { #ok : Text; #err : Text } {
        let cfg = switch (config) { case (null) { return #err("not configured") }; case (?c) { c } };
        switch (feeAddress) {
            case (?f) { if (f.keyName == cfg.feeKeyName) { return #ok(f.address) } };
            case (null) {};
        };
        if (deriving) { return #err("derivation in progress") };
        deriving := true;
        let result : { #ok : Text; #err : Text } = try {
            let r = await mgmt.schnorr_public_key({
                canister_id = null;
                derivation_path = [FEE_DERIVATION_PATH];
                key_id = { algorithm = #ed25519; name = cfg.feeKeyName };
            });
            if (r.public_key.size() != 32) {
                #err("unexpected public key length " # Nat.toText(r.public_key.size()));
            } else {
                let address = Base58.encode(r.public_key);
                feeAddress := ?{ keyName = cfg.feeKeyName; publicKey = r.public_key; address };
                #ok(address);
            };
        } catch (e) { #err(Error.message(e)) };
        deriving := false;
        result;
    };

    // ------------------------------------------------------------------
    // Read side for the external monitor (no outbound HTTP)
    // ------------------------------------------------------------------

    func journalView(e : Journal.Entry) : JournalView {
        {
            signature = e.signature;
            step = e.step;
            position = e.position;
            rows = e.rows;
            blockhash = e.blockhash;
            lastValidBlockHeight = e.lastValidBlockHeight;
            status = e.status;
            at = e.at;
            updatedAt = e.updatedAt;
            note = e.note;
        };
    };

    public query func getStatus() : async Status {
        let now = nowNs();
        let alert = switch (config) { case (?c) { c.alertAfterNs }; case (null) { Policy.ALERT_AFTER_NS } };
        let sign = switch (config) { case (?c) { c.signAfterNs }; case (null) { Policy.SIGN_AFTER_NS } };
        let stale = Policy.staleFor(now, tracker.lastProgressAt);
        let timerAge = Policy.staleFor(now, lastTimerAt);
        let level = Policy.escalateIfDead(Policy.level(workPending, stale, alert, sign), armed, timerAge, 4 * TIMER_PERIOD_NS);
        {
            level;
            staleForNs = if (workPending) { stale } else { 0 };
            workPending;
            lastSignature = Journal.lastSignature(journal);
            journalTail = Array.map<Journal.Entry, JournalView>(Journal.tail(journal, 10), journalView);
            armed;
            configured = config != null;
            feeAddress = switch (feeAddress) { case (?f) { ?f.address }; case (null) { null } };
            decision = lastDecision;
            blockedReason;
            lastOutcome;
            lastError;
            lastTickAt;
            lastTimerAt;
            timerAgeNs = timerAge;
            signaturesLastHour = Policy.signaturesInWindow(signTimes, now, HOUR_NS);
            marker = lastMarker;
            cyclesBalance = Cycles.balance();
            nowNs = now;
        };
    };

    public query func getJournal(limit : Nat) : async [JournalView] {
        Array.map<Journal.Entry, JournalView>(Journal.tail(journal, Nat.min(limit, 64)), journalView);
    };

    // ------------------------------------------------------------------
    // SOL RPC helpers (every chain read/write goes through the SOL RPC canister)
    // ------------------------------------------------------------------

    func rpcConfig(cfg : Config) : ?Rpc.RpcConfig {
        ?{ responseSizeEstimate = ?65_536; responseConsensus = ?cfg.rpcConsensus };
    };

    func programIdBlob(cfg : Config) : Blob {
        switch (Base58.decode(cfg.programId)) { case (?b) { b }; case (null) { Blob.fromArray([]) } };
    };

    func addresses(cfg : Config) : Pda.Addresses {
        switch (addrCache) {
            case (?(pid, a)) { if (pid == cfg.programId) { return a } };
            case (null) {};
        };
        let a = Pda.addresses(programIdBlob(cfg));
        addrCache := ?(cfg.programId, a);
        a;
    };

    /// A program-owned account's bytes at `finalized`, owner-checked. `#ok(null)`: no such account.
    func readAccount(cfg : Config, address : Blob) : async { #ok : ?Blob; #err : Text } {
        let params : Rpc.GetAccountInfoParams = {
            pubkey = Base58.encode(address);
            commitment = ?#finalized;
            encoding = ?#base64;
            dataSlice = null;
            minContextSlot = null;
        };
        let reply : { #ok : Rpc.MultiGetAccountInfoResult; #err : Text } = try {
            #ok(await (with cycles = cfg.rpcCycles) solRpc.getAccountInfo(cfg.rpcSources, rpcConfig(cfg), params));
        } catch (e) { #err(Error.message(e)) };
        switch (reply) {
            case (#err(t)) { #err("getAccountInfo: " # t) };
            case (#ok(m)) {
                switch (Rpc.ok<?Rpc.AccountInfo>(m)) {
                    case (#err(t)) { #err("getAccountInfo: " # t) };
                    case (#ok(null)) { #ok(null) };
                    case (#ok(?info)) {
                        if (info.owner != cfg.programId) { return #err("account owner is " # info.owner # ", not the program") };
                        switch (Rpc.accountBytes(info)) {
                            case (null) { #err("account data is not base64") };
                            case (?b) { #ok(?b) };
                        };
                    };
                };
            };
        };
    };

    func readBalance(cfg : Config, address : Text) : async { #ok : Nat64; #err : Text } {
        let params : Rpc.GetBalanceParams = { pubkey = address; commitment = ?#finalized; minContextSlot = null };
        let reply : { #ok : Rpc.MultiGetBalanceResult; #err : Text } = try {
            #ok(await (with cycles = cfg.rpcCycles) solRpc.getBalance(cfg.rpcSources, rpcConfig(cfg), params));
        } catch (e) { #err(Error.message(e)) };
        switch (reply) {
            case (#err(t)) { #err("getBalance: " # t) };
            case (#ok(m)) {
                switch (Rpc.ok<Nat64>(m)) { case (#err(t)) { #err("getBalance: " # t) }; case (#ok(v)) { #ok(v) } };
            };
        };
    };

    type Tip = { slot : Nat64; blockhash : Text; blockHeight : Nat64 };

    /// A recent finalized block: `getSlot` (rounded down for provider consensus) then
    /// `getBlock` (walking back over skipped slots). `lastValidBlockHeight` of a
    /// transaction using it is `blockHeight + 150`.
    func readTip(cfg : Config) : async { #ok : Tip; #err : Text } {
        let slotCfg : Rpc.GetSlotRpcConfig = { responseSizeEstimate = null; responseConsensus = ?cfg.rpcConsensus; roundingError = ?20 };
        let slotReply : { #ok : Rpc.MultiGetSlotResult; #err : Text } = try {
            #ok(await (with cycles = cfg.rpcCycles) solRpc.getSlot(cfg.rpcSources, ?slotCfg, ?{ commitment = ?#finalized; minContextSlot = null }));
        } catch (e) { #err(Error.message(e)) };
        let slot : Nat64 = switch (slotReply) {
            case (#err(t)) { return #err("getSlot: " # t) };
            case (#ok(m)) { switch (Rpc.ok<Nat64>(m)) { case (#err(t)) { return #err("getSlot: " # t) }; case (#ok(s)) { s } } };
        };
        var probe = slot;
        var tries : Nat = 0;
        loop {
            let params : Rpc.GetBlockParams = {
                slot = probe;
                transactionDetails = ?#none;
                commitment = ?#finalized;
                maxSupportedTransactionVersion = null;
                rewards = ?false;
            };
            let reply : { #ok : Rpc.MultiGetBlockResult; #err : Text } = try {
                #ok(await (with cycles = cfg.rpcCycles) solRpc.getBlock(cfg.rpcSources, rpcConfig(cfg), params));
            } catch (e) { #err(Error.message(e)) };
            switch (reply) {
                case (#err(t)) { return #err("getBlock: " # t) };
                case (#ok(m)) {
                    switch (Rpc.ok<?Rpc.ConfirmedBlock>(m)) {
                        case (#err(t)) { return #err("getBlock: " # t) };
                        case (#ok(?b)) {
                            switch (b.blockHeight) {
                                case (?h) { return #ok({ slot = probe; blockhash = b.blockhash; blockHeight = h }) };
                                case (null) { return #err("getBlock: the block carries no height") };
                            };
                        };
                        case (#ok(null)) {
                            // A skipped slot has no block: step back.
                            tries += 1;
                            if (tries > 4 or probe == 0) { return #err("getBlock: no block near slot " # Nat64.toText(slot)) };
                            probe -= 1;
                        };
                    };
                };
            };
        };
    };

    func decodeAccount<T>(bytes : ?Blob, decode : Blob -> ?T, what : Text) : { #ok : T; #err : Text } {
        switch (bytes) {
            case (null) { #err(what # " account not found") };
            case (?b) { switch (decode(b)) { case (null) { #err(what # " account did not decode") }; case (?x) { #ok(x) } } };
        };
    };

    // ------------------------------------------------------------------
    // Journal refresh: status reads, expiry by block height, identical-bytes re-broadcast
    // ------------------------------------------------------------------

    func observedOf(st : ?Rpc.TransactionStatus) : Journal.Observed {
        switch (st) {
            case (null) { #NotFound };
            case (?s) {
                switch (s.err) {
                    case (?e) { #Err(debug_show (e)) };
                    case (null) {
                        switch (s.confirmationStatus) {
                            case (?#finalized) { #Finalized };
                            case (?#confirmed) { #Confirmed };
                            case (_) { #Processed };
                        };
                    };
                };
            };
        };
    };

    func refreshJournal(cfg : Config, now : Nat) : async () {
        let open = Journal.unresolved(journal);
        if (open.size() == 0) { return };
        let sigs = Array.map<Journal.Entry, Text>(open, func(e : Journal.Entry) : Text { e.signature });
        let reply : { #ok : Rpc.MultiGetSignatureStatusesResult; #err : Text } = try {
            #ok(await (with cycles = cfg.rpcCycles) solRpc.getSignatureStatuses(cfg.rpcSources, rpcConfig(cfg), { signatures = sigs; searchTransactionHistory = ?true }));
        } catch (e) { #err(Error.message(e)) };
        let statuses : [?Rpc.TransactionStatus] = switch (reply) {
            case (#err(t)) { lastError := ?("getSignatureStatuses: " # t); return };
            case (#ok(m)) {
                switch (Rpc.ok<[?Rpc.TransactionStatus]>(m)) {
                    case (#err(t)) { lastError := ?("getSignatureStatuses: " # t); return };
                    case (#ok(xs)) { xs };
                };
            };
        };
        if (statuses.size() != sigs.size()) { lastError := ?"getSignatureStatuses: reply length differs from the request"; return };
        var seen : [Text] = [];
        var i : Nat = 0;
        while (i < sigs.size()) {
            let obs = observedOf(statuses[i]);
            switch (obs) { case (#NotFound) {}; case (_) { seen := Array.append<Text>(seen, [sigs[i]]) } };
            journal := Journal.applyStatus(journal, sigs[i], obs, now).journal;
            i += 1;
        };
        if (Journal.unresolved(journal).size() == 0) { return };

        // Still unresolved: is the blockhash gone, or can the identical bytes go again?
        switch (await readTip(cfg)) {
            case (#err(t)) { lastError := ?("tip: " # t) };
            case (#ok(tip)) {
                journal := Journal.expireUnseen(journal, tip.blockHeight, seen, now).journal;
                switch (Journal.rebroadcast(journal, tip.blockHeight)) {
                    case (null) {};
                    case (?e) {
                        switch (await sendRaw(cfg, e.raw)) {
                            case (#Accepted(sig)) {
                                if (sig == e.signature) { journal := Journal.applyStatus(journal, e.signature, #Processed, now).journal };
                            };
                            case (#Rejected(m)) { lastError := ?("re-broadcast rejected: " # m) };
                            case (#Unknown(m)) { lastError := ?("re-broadcast unknown: " # m) };
                        };
                    };
                };
            };
        };
    };

    func sendRaw(cfg : Config, wire : Blob) : async Rpc.SendOutcome {
        let params : Rpc.SendTransactionParams = {
            transaction = Base64.encode(wire);
            encoding = ?#base64;
            skipPreflight = ?false;
            preflightCommitment = null;
            maxRetries = null;
            minContextSlot = null;
        };
        // A decode failure of the reply lands in the catch: the transaction MAY have been broadcast.
        try {
            Rpc.classifySend(await (with cycles = cfg.rpcCycles) solRpc.sendTransaction(cfg.rpcSources, rpcConfig(cfg), params));
        } catch (e) { #Unknown("call failed: " # Error.message(e)) };
    };

    // ------------------------------------------------------------------
    // Planning: the pure planner, fed with on-demand reads
    // ------------------------------------------------------------------

    func plan(cfg : Config, pool : Accounts.Pool, ticket : ?Accounts.Ticket, close : ?Accounts.CloseRecord, maxRows : Nat) : async Planner.Plan {
        let a = addresses(cfg);
        var regCache : [(Nat, Accounts.RegistryPage)] = [];
        var snapCache : [(Nat, Accounts.SnapshotPage)] = [];
        var liveCache : [(Blob, Nat)] = [];
        var reads : Nat = 0;
        loop {
            let view : Planner.View = {
                addr = a;
                nowSecs = Nat64.fromNat(nowNs() / 1_000_000_000);
                deadlineMarginSecs = cfg.deadlineMarginSecs;
                maxRows;
                skipNewlyCapped = not cfg.pruneNewlyCapped;
                pool;
                ticket;
                close;
                registryPage = func(i : Nat) : ?Accounts.RegistryPage {
                    for ((k, p) in regCache.vals()) { if (k == i) { return ?p } };
                    null;
                };
                snapshotPage = func(i : Nat) : ?Accounts.SnapshotPage {
                    for ((k, p) in snapCache.vals()) { if (k == i) { return ?p } };
                    null;
                };
                liveRemaining = func(key : Blob) : ?Nat {
                    for ((k, v) in liveCache.vals()) { if (k == key) { return ?v } };
                    null;
                };
            };
            switch (Planner.next(view)) {
                case (#Need(n)) {
                    reads += 1;
                    if (reads > 12) { return #Blocked("the planner needs too many reads for one step") };
                    switch (n) {
                        case (#Registry(i)) {
                            switch (await readAccount(cfg, a.registry(i))) {
                                case (#err(t)) { return #Blocked("registry page " # Nat.toText(i) # ": " # t) };
                                case (#ok(b)) {
                                    switch (decodeAccount<Accounts.RegistryPage>(b, Accounts.decodeRegistryPage, "registry page")) {
                                        case (#err(t)) { return #Blocked(t) };
                                        case (#ok(p)) { regCache := Array.append<(Nat, Accounts.RegistryPage)>(regCache, [(i, p)]) };
                                    };
                                };
                            };
                        };
                        case (#Snapshot(i)) {
                            switch (await readAccount(cfg, a.snapshot(i))) {
                                case (#err(t)) { return #Blocked("snapshot page " # Nat.toText(i) # ": " # t) };
                                case (#ok(b)) {
                                    switch (decodeAccount<Accounts.SnapshotPage>(b, Accounts.decodeSnapshotPage, "snapshot page")) {
                                        case (#err(t)) { return #Blocked(t) };
                                        case (#ok(p)) { snapCache := Array.append<(Nat, Accounts.SnapshotPage)>(snapCache, [(i, p)]) };
                                    };
                                };
                            };
                        };
                        case (#Backer(key)) {
                            switch (await readAccount(cfg, key)) {
                                case (#err(t)) { return #Blocked("backer: " # t) };
                                case (#ok(b)) {
                                    switch (decodeAccount<Accounts.Backer>(b, Accounts.decodeBacker, "backer")) {
                                        case (#err(t)) { return #Blocked(t) };
                                        case (#ok(bk)) { liveCache := Array.append<(Blob, Nat)>(liveCache, [(key, Accounts.remaining(bk))]) };
                                    };
                                };
                            };
                        };
                    };
                };
                case (other) { return other };
            };
        };
    };

    // ------------------------------------------------------------------
    // Signing: ONE private function, reached only from `attempt` <- `tick`
    // ------------------------------------------------------------------

    func signMessage(cfg : Config, message : Blob) : async { #ok : Blob; #err : Text } {
        try {
            let res = await (with cycles = cfg.signCycles) mgmt.sign_with_schnorr({
                message;
                derivation_path = [FEE_DERIVATION_PATH];
                key_id = { algorithm = #ed25519; name = cfg.feeKeyName };
                aux = null;
            });
            if (res.signature.size() != 64) { #err("signature is not 64 bytes") } else { #ok(res.signature) };
        } catch (e) { #err(Error.message(e)) };
    };

    /// Build, check, sign, journal and send ONE step. Returns a one-line outcome.
    func attempt(cfg : Config, fee : FeeAddress, step : Planner.Step, now : Nat) : async Text {
        let feePayer = fee.publicKey;
        let price = cfg.computeUnitPriceMicroLamports;
        let limit : Nat32 = if (price == 0) { 0 } else { Planner.computeUnitLimit(step.name) };

        // Balance first, then the freshest blockhash, then sign: the blockhash ages while we wait.
        let balance = switch (await readBalance(cfg, fee.address)) { case (#err(t)) { return "blocked: " # t }; case (#ok(b)) { b } };
        let tip = switch (await readTip(cfg)) { case (#err(t)) { return "blocked: " # t }; case (#ok(t)) { t } };
        let blockhash = switch (Base58.decode(tip.blockhash)) {
            case (?b) { if (b.size() == 32) { b } else { return "blocked: blockhash is not 32 bytes" } };
            case (null) { return "blocked: blockhash is not base58" };
        };
        let ixs = Array.append<Tx.Instruction>(Tx.computeBudgetInstructions(limit, price), [step.instruction]);
        let compiled = Tx.compile(feePayer, blockhash, ixs);
        let feeLamports = Tx.feeLamports(compiled, limit, price);
        switch (Gate.feeGate(balance, cfg.minFeeBalanceLamports, feeLamports, cfg.maxFeeLamportsPerTx)) {
            case (?m) { return "blocked: " # m };
            case (null) {};
        };

        // Last look at what is about to be signed: fee payer in slot 0 and the only signer, no
        // instruction names it, only the native program (and ComputeBudget) is targeted.
        let budgetProgram = switch (Base58.decode(Tx.COMPUTE_BUDGET_PROGRAM)) { case (?b) { b }; case (null) { Blob.fromArray([]) } };
        switch (Gate.checkMessage(compiled.bytes, feePayer, [programIdBlob(cfg), budgetProgram])) {
            case (?m) { return "blocked: " # m };
            case (null) {};
        };

        let signature = switch (await signMessage(cfg, compiled.bytes)) { case (#err(t)) { return "sign failed: " # t }; case (#ok(s)) { s } };
        let wire = Tx.wire(compiled.bytes, signature);
        let sigText = Tx.signatureText(signature);

        // Journal BEFORE the send: state before an await is committed, so a crash or a decode
        // trap during the send leaves an #Unknown entry, never a lost transaction.
        journal := Journal.append(journal, {
            signature = sigText;
            blockhash = tip.blockhash;
            lastValidBlockHeight = tip.blockHeight + BLOCKHASH_VALIDITY;
            step = step.summary;
            position = step.position;
            rows = step.rows;
            raw = wire;
            status = #Unknown;
            at = now;
            updatedAt = now;
            note = "signed; send pending";
        }, cfg.journalMax);
        lastSignedAt := now;
        signTimes := Array.append<Nat>(Policy.pruneWindow(signTimes, now, HOUR_NS), [now]);

        switch (await sendRaw(cfg, wire)) {
            case (#Accepted(sig)) {
                if (sig == sigText) {
                    journal := Journal.applyStatus(journal, sigText, #Processed, nowNs()).journal;
                    "submitted " # step.summary # " " # sigText;
                } else {
                    "submitted " # step.summary # ": providers returned a different signature " # sig;
                };
            };
            case (#Rejected(m)) {
                journal := Journal.applyStatus(journal, sigText, #Err(m), nowNs()).journal;
                "rejected " # step.summary # ": " # m;
            };
            case (#Unknown(m)) {
                // Stays #Unknown: no new transaction until a status read resolves it.
                journal := Journal.markUnknown(journal, sigText, m, nowNs());
                "unknown outcome " # step.summary # ": " # m;
            };
        };
    };

    // ------------------------------------------------------------------
    // The tick
    // ------------------------------------------------------------------

    func recordDecision(d : Text, blocked : ?Text) {
        lastDecision := d;
        blockedReason := blocked;
    };

    func tickBusy(now : Nat) : Bool {
        tickStartedAt != 0 and now >= tickStartedAt and now - tickStartedAt < TICK_TIMEOUT_NS;
    };

    func onTimer() : async () {
        lastTimerAt := nowNs();
        await tick(false);
    };

    func tick(force : Bool) : async () {
        let cfg = switch (config) { case (null) { return }; case (?c) { c } };
        let start = nowNs();
        if (tickBusy(start)) { return };
        if (not force and not Policy.due(start, nextPollAt)) { return };
        tickStartedAt := start;
        try { await tickInner(cfg, start) } catch (e) {
            lastError := ?("tick: " # Error.message(e));
            recordDecision("tick error", null);
        };
        tickStartedAt := 0;
    };

    func tickInner(cfg : Config, now : Nat) : async () {
        lastTickAt := now;
        lastError := null;
        // Keep the next poll bounded even if this tick ends early on an error.
        nextPollAt := now + cfg.activePollNs;

        // 1. Journal: resolve what was sent earlier before looking at the chain.
        await refreshJournal(cfg, now);
        let landed = Journal.landedWithin(journal, nowNs(), OWN_LANDING_WINDOW_NS);

        // 2. Progress markers, finalized.
        let a = addresses(cfg);
        let poolBytes = switch (await readAccount(cfg, a.pool)) {
            case (#err(t)) { lastError := ?t; recordDecision("read error", null); return };
            case (#ok(b)) { b };
        };
        let pool = switch (decodeAccount<Accounts.Pool>(poolBytes, Accounts.decodePool, "pool")) {
            case (#err(t)) { lastError := ?t; recordDecision("read error", ?t); return };
            case (#ok(p)) { p };
        };
        if (pool.clusterDomain != cfg.clusterDomain) {
            recordDecision("#Blocked", ?"cluster domain mismatch: the pool belongs to another cluster");
            return;
        };
        var ticket : ?Accounts.Ticket = null;
        if (pool.activeTicket != 0) {
            switch (await readAccount(cfg, a.ticket)) {
                case (#err(t)) { lastError := ?t; recordDecision("read error", null); return };
                case (#ok(b)) {
                    switch (decodeAccount<Accounts.Ticket>(b, Accounts.decodeTicket, "ticket")) {
                        case (#err(t)) { lastError := ?t; recordDecision("read error", ?t); return };
                        case (#ok(x)) { ticket := ?x };
                    };
                };
            };
        };
        var close : ?Accounts.CloseRecord = null;
        switch (pool.currentCloseId) {
            case (null) {};
            case (?cid) {
                switch (await readAccount(cfg, a.close(cid))) {
                    case (#err(t)) { lastError := ?t; recordDecision("read error", null); return };
                    case (#ok(b)) {
                        switch (decodeAccount<Accounts.CloseRecord>(b, Accounts.decodeCloseRecord, "close record")) {
                            case (#err(t)) { lastError := ?t };
                            case (#ok(x)) { close := ?x };
                        };
                    };
                };
            };
        };

        // 3. Progress tracking + cadence.
        let pending = Accounts.workPending(pool, ticket, close);
        let marker = Accounts.marker(pool, ticket, close);
        workPending := pending;
        lastMarker := marker;
        let t = nowNs();
        tracker := Policy.observe(tracker, marker, pending, t, landed);
        nextPollAt := t + Policy.pollDelayNs(pending, cfg.activePollNs, cfg.idlePollNs);

        // 4. Decide.
        let decision = Policy.decide({
            now = t;
            lastProgressAt = tracker.lastProgressAt;
            lastSignedAt;
            alertAfterNs = cfg.alertAfterNs;
            signAfterNs = cfg.signAfterNs;
            journalHasUnknown = Journal.hasUnknown(journal);
            configured = feeAddress != null;
        });
        switch (decision) {
            case (#Idle) { recordDecision("#Idle", null); return };
            case (#Alert) { recordDecision("#Alert", null); return };
            case (#Blocked(m)) { recordDecision("#Blocked", ?m); return };
            case (#Sign) {};
        };
        let fee = switch (feeAddress) {
            case (?f) { if (f.keyName == cfg.feeKeyName) { f } else { recordDecision("#Blocked", ?"fee address was derived for another key"); return } };
            case (null) { recordDecision("#Blocked", ?"no fee address"); return };
        };

        // 5. #Sign: local gates first (they cost nothing), then plan, then one attempt.
        switch (Gate.beforePlan(journal, signTimes, t, HOUR_NS, cfg.maxSignaturesPerHour)) {
            case (#Wait(m)) { recordDecision("#Alert", ?m); return };
            case (#Blocked(m)) { recordDecision("#Blocked", ?m); return };
            case (#Proceed) {};
        };
        var planned = await plan(cfg, pool, ticket, close, cfg.maxRows);
        // The one-row fallback: a position that already failed or expired is retried with a single row.
        switch (planned) {
            case (#Step(s)) {
                switch (Gate.afterPlan(journal, s.position, s.rows, cfg.maxAttemptsPerPosition)) {
                    case (#Blocked(m)) { recordDecision("#Blocked", ?m); return };
                    case (#Fallback) { planned := await plan(cfg, pool, ticket, close, 1) };
                    case (#Go) {};
                };
            };
            case (_) {};
        };
        switch (planned) {
            case (#Idle) { recordDecision("#Blocked", ?"the planner has nothing to do although progress is stale") };
            case (#Need(_)) { recordDecision("#Blocked", ?"the planner could not finish its reads") };
            case (#Blocked(m)) { recordDecision("#Blocked", ?m) };
            case (#Step(s)) {
                let outcome = await attempt(cfg, fee, s, nowNs());
                lastOutcome := ?outcome;
                recordDecision("#Sign", null);
            };
        };
    };

    /// Run one tick now (controllers only). Same code path and same gates as the timer; it takes no input bytes.
    public shared ({ caller }) func tickNow() : async { #ok; #err : Text } {
        if (not requireController(caller)) { return #err("controllers only") };
        if (config == null) { return #err("not configured") };
        await tick(true);
        #ok;
    };
};
