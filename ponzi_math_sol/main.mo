import Array "mo:base/Array";
import Buffer "mo:base/Buffer";
import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Time "mo:base/Time";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat64 "mo:base/Nat64";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Text "mo:base/Text";
import List "mo:base/List";
import Iter "mo:base/Iter";
import Debug "mo:base/Debug";
import Blob "mo:base/Blob";
import Error "mo:base/Error";
import Cycles "mo:base/ExperimentalCycles";
import Timer "mo:base/Timer";

import Icrc21 "icrc21";

import Base58 "Base58";
import PpLedger "PpLedger";
import SolRpc "SolRpc";
import SolSigner "SolSigner";
import SolTx "SolTx";

persistent actor class PonziMathSol(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
    solTreasuryAddress : Text;
    solRpcProvider : SolRpc.Provider;
    keyId : SolSigner.KeyId;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let solRpc : SolRpc.RpcActor = actor(SolRpc.SOL_RPC_CANISTER_ID);
    transient let ic : actor { raw_rand : () -> async Blob } = actor "aaaaa-aa";
    transient let ppLedger : PpLedger.LedgerActor = actor (PpLedger.PP_LEDGER_CANISTER_ID);

    // SOL-side config — captured at init, then mirrored to mutable state
    // below so admin can update it post-deploy.
    transient let _INIT_TREASURY : Text = initArgs.solTreasuryAddress;
    transient let _INIT_RPC_PROVIDER : SolRpc.Provider = initArgs.solRpcProvider;
    transient let _INIT_KEY_ID : SolSigner.KeyId = initArgs.keyId;

    // ========================================================================
    // Public types
    // ========================================================================

    public type GamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    public type GameRecord = {
        id : Nat;
        player : Principal;
        plan : GamePlan;
        amount : Float;
        startTime : Int;
        isCompounding : Bool;
        isActive : Bool;
        lastUpdateTime : Int;
        accumulatedEarnings : Float;
        totalWithdrawn : Float;
    };

    public type PlatformStats = {
        totalDeposits : Float;
        totalWithdrawals : Float;
        activeGames : Nat;
        potBalance : Float;
        daysActive : Nat;
    };

    public type GameResetRecord = {
        resetTime : Int;
        reason : Text;
    };

    public type BackerType = {
        #seriesA;
        #seriesB;
    };

    public type BackerKey = (Principal, BackerType);

    func backerKeyCompare(a : BackerKey, b : BackerKey) : { #less; #equal; #greater } {
        switch (Principal.compare(a.0, b.0)) {
            case (#less) #less;
            case (#greater) #greater;
            case (#equal) {
                switch (a.1, b.1) {
                    case (#seriesA, #seriesA) #equal;
                    case (#seriesB, #seriesB) #equal;
                    case (#seriesA, #seriesB) #less;
                    case (#seriesB, #seriesA) #greater;
                };
            };
        };
    };

    public type BackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : BackerType;
        firstDepositDate : ?Int;
    };

    public type GeneralLedgerEntry = {
        id : Nat;
        timestamp : Int;
        roundId : Nat;
        event : GeneralLedgerEvent;
    };

    public type RoundSummary = {
        roundId : Nat;
        startTime : Int;          // canister-genesis time for round 1; otherwise prior gameReset.resetTime
        endTime : ?Int;           // null for the in-flight round; resetTime of the closing gameReset for past rounds
        endReason : ?Text;        // null for the in-flight round
        eventCount : Nat;
        seedReserveCarried : Float; // gameReset.seedReserveCarried for closed rounds; 0.0 for the in-flight round
    };

    public type ActivePlanSnapshot = {
        game : GameRecord;
        currentGrossEarnings : Float;
        currentExitToll : Float;
        currentNetClaimable : Float;
        daysElapsed : Float;
        daysToMaturity : Float;   // 0.0 once matured
        isMatured : Bool;
        wouldBeInsolvent : Bool;  // true if pot < currentGrossEarnings
    };

    public type GeneralLedgerEvent = {
        #deposit : {
            player : Principal;
            gameId : Nat;
            gross : Float;
            coverCharge : Float;
            netToPot : Float;
            plan : GamePlan;
            isCompounding : Bool;
        };
        #backerDeposit : {
            backer : Principal;
            amount : Float;
            entitlement : Float;
        };
        #withdrawal : {
            player : Principal;
            gameId : Nat;
            grossEarnings : Float;
            toll : Float;
            netToPlayer : Float;
            potDeduction : Float;
            isInsolvent : Bool;
        };
        #settlement : {
            player : Principal;
            gameId : Nat;
            grossEarnings : Float;
            toll : Float;
            netToPlayer : Float;
            potDeduction : Float;
            isInsolvent : Bool;
        };
        #tollDistribution : {
            tollAmount : Float;
            toSeedReserve : Float;
            toOldestSeriesA : Float;
            toOtherSeriesA : Float;
            toAllBackers : Float;
        };
        #backerRepaymentClaim : {
            backer : Principal;
            amount : Float;
        };
        #coverChargeAccrued : {
            gameId : Nat;
            player : Principal;
            amountE8s : Nat;
        };
        #coverChargeSwept : {
            amountE8s : Nat;
            toBackend : Principal;
            blockIndex : Nat;
        };
        #gameReset : {
            reason : Text;
            seedReserveCarried : Float;
        };
        #backdatedGameCreated : {
            admin : Principal;
            player : Principal;
            gameId : Nat;
            startTime : Int;
            amount : Float;
        };
        #seriesBPromotion : {
            owner : Principal;
            underwater : Float;
            entitlement : Float;
        };
    };

    // ========================================================================
    // State
    // ========================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMapNat = OrderedMap.Make<Principal>(Principal.compare);
    transient let intMap = OrderedMap.Make<Int>(Int.compare);
    transient let backerKeyMap = OrderedMap.Make<BackerKey>(backerKeyCompare);

    var gameRecords = natMap.empty<GameRecord>();
    var nextGameId : Nat = 0;

    var platformStats : PlatformStats = {
        totalDeposits = 0.0;
        totalWithdrawals = 0.0;
        activeGames = 0;
        potBalance = 0.0;
        daysActive = 0;
    };

    var gameResetHistory = intMap.empty<GameResetRecord>();
    var roundSeedReserve : Float = 0.0;
    var depositTimestamps = principalMapNat.empty<List.List<Int>>();
    var backerPositions = backerKeyMap.empty<BackerPosition>();
    var backerRepayments = backerKeyMap.empty<Float>();
    var coverChargeBalance : Nat = 0;
    var generalLedger = natMap.empty<GeneralLedgerEntry>();
    var nextGeneralLedgerId : Nat = 0;
    var currentRoundId : Nat = 1;

    // ============== Chain fusion / SOL state ==============

    // Admin-tunable Solana RPC + signing config.
    var solRpcProvider : SolRpc.Provider = initArgs.solRpcProvider;
    var keyId : SolSigner.KeyId = initArgs.keyId;
    var solTreasuryAddress : Text = initArgs.solTreasuryAddress;

    // Pool address — singleton, derivation path ["pool"]. Holds all pot lamports.
    var poolAddress : ?Text = null;

    // Nonce account — singleton, derivation path ["nonce"]. Durable nonce
    // for outbound txs.
    var nonceAccountAddress : ?Text = null;
    var lastNonceValue : ?Text = null;
    var bootstrapped : Bool = false;

    // Per-user deposit addresses (caller principal → base58 pubkey).
    // `principalMapNat` already exists in the original state block — it's
    // a principal-keyed OrderedMap whose value type is per-empty<>(). We
    // reuse it for `depositAddresses`. The new `textMap` alias serves
    // address-keyed maps (reverse lookup + signature cursor).
    transient let textMap = OrderedMap.Make<Text>(Text.compare);
    var depositAddresses = principalMapNat.empty<Text>();
    var addressToPrincipal = textMap.empty<Principal>();

    // Desk PP inventory lives in a fixed subaccount of THIS canister on pp_ledger.
    // Bytes spell "PPDESK" then zero-padded to 32. Distinct from the default
    // (null) subaccount so desk inventory never mixes with any other PP the
    // canister might hold.
    transient let DESK_ESCROW_SUBACCOUNT : Blob = Blob.fromArray([
        0x50, 0x50, 0x44, 0x45, 0x53, 0x4b, // "PPDESK"
        0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
    ]);

    func deskEscrowAccount() : PpLedger.Account {
        { owner = Principal.fromActor(Self); subaccount = ?DESK_ESCROW_SUBACCOUNT };
    };

    public query func getDeskEscrowAccount() : async { owner : Principal; subaccount : Blob } {
        { owner = Principal.fromActor(Self); subaccount = DESK_ESCROW_SUBACCOUNT };
    };

    // Deposit-detection cursors per address.
    var lastSeenSignature = textMap.empty<Text>();

    // Deposit intents — caller commits to a plan + amount before sending SOL.
    public type DepositIntent = {
        id : Nat;
        principal : Principal;
        plan : GamePlan;
        expectedAmountLamports : Nat64;
        createdAt : Int;
        expiresAt : Int;
        fulfilled : Bool;
    };
    var pendingIntents = natMap.empty<DepositIntent>();
    var nextIntentId : Nat = 0;

    // Cover charge accrual in lamports. Lives on the pool address until
    // payManagementSol sweeps it.
    var coverChargeAccrualLamports : Nat64 = 0;

    // Min deposit gate — 0.05 SOL (50_000_000 lamports). Mirrors
    // ponzi_math's 0.1 ICP gate at deploy-time prices.
    transient let MIN_DEPOSIT_LAMPORTS : Nat64 = 50_000_000;

    // Cycles budget attached to every sol-rpc canister call.
    // The sol-rpc canister (tghme-zyaaa-aaaar-qarca-cai) charges 5-20G per
    // call depending on method and response size. 20G is a defensive buffer
    // that covers the heaviest methods; unused cycles are refunded.
    transient let RPC_CYCLES : Nat = 20_000_000_000;

    // Intent TTL — 2 hours. Long enough for Solana finality plus a human
    // switching to their wallet to send the SOL. Deposit detection polls
    // every DETECTION_INTERVAL_SECONDS, so a funded intent is matched well
    // within this window. (Was 10 minutes, which expired before manual
    // detection was ever triggered — the deposit-not-credited root cause.)
    transient let INTENT_TTL_NS : Int = 2 * 60 * 60 * 1_000_000_000;

    // Deposit-detection auto-poll. The recurring timer scans ONLY addresses
    // with an open, non-expired intent (runDetectionForOpenIntents), so an
    // idle canister makes zero RPC outcalls per tick. detectionTimerId is a
    // plain var: it survives upgrades as a STALE id, so postupgrade re-arms
    // (the IC clears all timers on upgrade). detectionInProgress is the
    // overlap guard, transient (resets on upgrade, safe by construction).
    transient let DETECTION_INTERVAL_SECONDS : Nat = 60;
    var detectionTimerId : ?Timer.TimerId = null;
    transient var detectionInProgress : Bool = false;

    // Transient concurrency state — resets on upgrade (safe by construction)
    transient var callerLocks = principalMapNat.empty<Bool>();
    transient var globalCriticalLock : Bool = false;

    // ========================================================================
    // Concurrency: per-caller lock and global critical-section lock
    // ========================================================================

    func acquireCallerLock(caller : Principal) {
        switch (principalMapNat.get(callerLocks, caller)) {
            case (?true) { Debug.trap("Another operation is already in progress for this caller") };
            case _ { callerLocks := principalMapNat.put(callerLocks, caller, true) };
        };
    };

    func releaseCallerLock(caller : Principal) {
        callerLocks := principalMapNat.delete(callerLocks, caller);
    };

    func acquireGlobalLock() {
        if (globalCriticalLock) {
            Debug.trap("Critical section busy — another operation is in progress, please retry");
        };
        globalCriticalLock := true;
    };

    func releaseGlobalLock() {
        globalCriticalLock := false;
    };

    public query func isCriticalSectionBusy() : async Bool {
        globalCriticalLock;
    };

    // ========================================================================
    // Validation + formatting helpers
    // ========================================================================

    func requireAuthenticated(caller : Principal) {
        if (Principal.isAnonymous(caller)) {
            Debug.trap("Anonymous principal not allowed");
        };
    };

    // Admin allowlist — kept in sync with the frontend CHARLES_PRINCIPALS list
    // in frontend/src/lib/charles.tsx. These principals can call the admin
    // god-view queries (adminGetActivePlansSnapshot, adminGetEventsByRound, ...).
    // Rotation = code redeploy. Caller is verified by the IC request envelope
    // signature, so this guard is effective even on `query` methods.
    transient let ADMIN_PRINCIPALS : [Principal] = [
        Principal.fromText("zs6vm-4yyag-sbw7x-6ipms-h4tmz-ox4pu-mcq3b-thtt4-de25x-wmsh4-rqe"),
        Principal.fromText("stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe"),
        Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae"),
        Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe"),
    ];

    func isAdmin(caller : Principal) : Bool {
        if (caller == TEST_ADMIN) { return true };
        for (admin in ADMIN_PRINCIPALS.vals()) {
            if (caller == admin) { return true };
        };
        false;
    };

    func requireAdmin(caller : Principal) {
        requireAuthenticated(caller);
        if (not isAdmin(caller)) {
            Debug.trap("Unauthorized: admin only");
        };
    };

    func validateAmount(amount : Float) {
        if (Float.isNaN(amount)) { Debug.trap("Amount cannot be NaN") };
        if (Float.isNaN(amount - amount) and not Float.isNaN(amount)) {
            Debug.trap("Amount must be finite");
        };
        if (amount < 0.0) { Debug.trap("Amount cannot be negative") };
    };

    func roundToEightDecimals(value : Float) : Float {
        let multiplier = 100000000.0;
        Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
    };


    // ========================================================================
    // General Ledger event recording
    // ========================================================================

    func recordLedger(event : GeneralLedgerEvent) {
        let entry : GeneralLedgerEntry = {
            id = nextGeneralLedgerId;
            timestamp = Time.now();
            roundId = currentRoundId;
            event;
        };
        generalLedger := natMap.put(generalLedger, nextGeneralLedgerId, entry);
        nextGeneralLedgerId += 1;
    };

    // ========================================================================
    // Exit toll calculation
    // Simple: 12% (< 7 days), 7.5% (7-14), 3% (>= 14)
    // Compounding: 9% (15-day plan), 13% (30-day plan)
    // ========================================================================

    func calculateExitToll(game : GameRecord, earnings : Float) : Float {
        if (game.isCompounding) {
            switch (game.plan) {
                case (#compounding15Day) { earnings * 0.09 };
                case (#compounding30Day) { earnings * 0.13 };
                case (#simple21Day) { 0.0 };
            };
        } else {
            let elapsedSeconds = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
            let elapsedDays = elapsedSeconds / 86400.0;
            if (elapsedDays < 7.0) { earnings * 0.12 }
            else if (elapsedDays < 14.0) { earnings * 0.075 }
            else { earnings * 0.03 };
        };
    };

    // ========================================================================
    // Backer repayment crediting + 35/25/40 exit-toll distribution
    // ========================================================================

    func creditBackerRepayment(key : BackerKey, amount : Float) {
        let current = switch (backerKeyMap.get(backerRepayments, key)) {
            case (null) { 0.0 };
            case (?existing) { existing };
        };
        backerRepayments := backerKeyMap.put(backerRepayments, key, current + amount);
    };

    type TollDistributionDetails = {
        tollAmount : Float;
        toSeedReserve : Float;
        toOldestSeriesA : Float;
        toOtherSeriesA : Float;
        toAllBackers : Float;
    };

    // 50% of the toll seeds the next round (routed to roundSeedReserve, OUT of
    // the pot). The other 50% credits backer repayment balances via 35/25/40.
    //
    // Returns a TollDistributionDetails describing what was distributed; caller
    // is responsible for recording the #tollDistribution ledger event AFTER the
    // payout transfer succeeds. This split prevents a phantom ledger entry when
    // the transfer fails and the state mutations are rolled back.
    func distributeExitToll(tollAmount : Float) : TollDistributionDetails {
        let seedAmount = tollAmount * 0.5;
        let backerRepaymentAmount = tollAmount * 0.5;
        roundSeedReserve += seedAmount;

        let allBackers = Iter.toArray(backerKeyMap.vals(backerPositions));
        if (allBackers.size() == 0) {
            // No backers yet — backer half also flows to seed reserve (not pot).
            roundSeedReserve += backerRepaymentAmount;
            return {
                tollAmount;
                toSeedReserve = tollAmount;
                toOldestSeriesA = 0.0;
                toOtherSeriesA = 0.0;
                toAllBackers = 0.0;
            };
        };

        let seriesABackers = List.toArray(
            List.filter(
                List.fromArray(allBackers),
                func(b : BackerPosition) : Bool { b.backerType == #seriesA },
            )
        );

        var oldestBacker : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in seriesABackers.vals()) {
            switch (b.firstDepositDate) {
                case (null) {};
                case (?date) {
                    if (oldestBacker == null or date < oldestTime) {
                        oldestBacker := ?b;
                        oldestTime := date;
                    };
                };
            };
        };

        let otherSeriesA = List.toArray(
            List.filter(
                List.fromArray(seriesABackers),
                func(b : BackerPosition) : Bool {
                    switch (oldestBacker) {
                        case (null) { true };
                        case (?oldest) { b.owner != oldest.owner };
                    };
                },
            )
        );

        // If there's only one Series A backer (no "others"), the 25% portion
        // also goes to that lone backer. Total to oldest in that case: 60%.
        let toOldest : Float =
            if (otherSeriesA.size() == 0) {
                backerRepaymentAmount * 0.60;
            } else {
                backerRepaymentAmount * 0.35;
            };
        switch (oldestBacker) {
            case (null) {};
            case (?b) { creditBackerRepayment((b.owner, b.backerType), toOldest) };
        };

        var toOthers : Float = 0.0;
        if (otherSeriesA.size() > 0) {
            let perBacker = backerRepaymentAmount * 0.25 / Float.fromInt(otherSeriesA.size());
            toOthers := perBacker * Float.fromInt(otherSeriesA.size());
            for (b in otherSeriesA.vals()) { creditBackerRepayment((b.owner, b.backerType), perBacker) };
        };

        let perAll = backerRepaymentAmount * 0.4 / Float.fromInt(allBackers.size());
        let toAll = perAll * Float.fromInt(allBackers.size());
        for (b in allBackers.vals()) { creditBackerRepayment((b.owner, b.backerType), perAll) };

        {
            tollAmount;
            toSeedReserve = seedAmount;
            toOldestSeriesA = toOldest;
            toOtherSeriesA = toOthers;
            toAllBackers = toAll;
        };
    };

    // ========================================================================
    // Chain fusion: derivation path helpers + pool address caching.
    // These are private — callers go through adminDerivePoolAddress or
    // ensurePoolAddress.
    // ========================================================================

    func derivationPathPool() : [Blob] {
        [Text.encodeUtf8("pool")];
    };

    func derivationPathNonce() : [Blob] {
        [Text.encodeUtf8("nonce")];
    };

    func derivationPathForPrincipal(p : Principal) : [Blob] {
        [Principal.toBlob(p)];
    };

    func ensurePoolAddress() : async Text {
        switch (poolAddress) {
            case (?addr) { addr };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathPool());
                poolAddress := ?addr;
                addr;
            };
        };
    };

    // ====================================================================
    // Bootstrap helpers
    // ====================================================================

    func ensureNonceAccountAddress() : async Text {
        switch (nonceAccountAddress) {
            case (?addr) { addr };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathNonce());
                nonceAccountAddress := ?addr;
                addr;
            };
        };
    };

    /// Fetch a recent blockhash via jsonRequest passthrough, retrying on
    /// consensus failures. Used ONLY by bootstrap — all other outbound txs
    /// use the durable nonce. The sol-rpc canister does not expose a typed
    /// getLatestBlockhash method; we use the raw JSON-RPC passthrough.
    func fetchRecentBlockhashWithRetry(attempts : Nat) : async ?Text {
        let payload = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getLatestBlockhash\",\"params\":[{\"commitment\":\"finalized\"}]}";
        var i : Nat = 0;
        while (i < attempts) {
            Cycles.add<system>(RPC_CYCLES);
            let multiRes = await solRpc.jsonRequest(
                SolRpc.rpcSources(solRpcProvider),
                null,
                payload,
            );
            switch (SolRpc.unwrapMultiRequest(multiRes)) {
                case (#Ok(json)) {
                    switch (SolRpc.parseBlockhashFromJson(json)) {
                        case (?h) { return ?h };
                        case (null) { i += 1 };
                    };
                };
                case (#Err(_)) { i += 1 };
            };
        };
        null;
    };

    /// Extract raw bytes from an AccountData variant (base58 encoding only).
    /// We request `encoding = #base58` for nonce accounts (80 bytes, well
    /// within the 129-byte base58 limit), so the response is
    /// `#binary(base58Text, #base58)`. Returns null for other variants.
    func accountDataToBlob(data : SolRpc.AccountData) : ?Blob {
        switch (data) {
            case (#binary(encoded, #base58)) { Base58.decode(encoded) };
            case (#binary(_encoded, #base64)) {
                // Fallback: not used in practice — we always request base58.
                // Return null; caller will skip nonce refresh.
                null
            };
            case (_) { null };
        };
    };

    /// Parse 32 bytes of nonce account body as a base58 blockhash.
    /// Solana nonce-account layout (System program account state):
    ///   bytes 0..4 — version (u32 LE)
    ///   bytes 4..8 — state (u32 LE; 1 = Initialized)
    ///   bytes 8..40 — authority pubkey (32 bytes)
    ///   bytes 40..72 — nonce value (32 bytes — what we want)
    ///   bytes 72..80 — fee_calculator.lamports_per_signature (u64 LE)
    func parseNonceFromAccountData(data : SolRpc.AccountData) : ?Text {
        switch (accountDataToBlob(data)) {
            case (null) { null };
            case (?rawBytes) {
                let arr = Blob.toArray(rawBytes);
                if (arr.size() < 72) { return null };
                let nonceBytes = Array.tabulate<Nat8>(32, func(i) { arr[40 + i] });
                ?Base58.encode(Blob.fromArray(nonceBytes));
            };
        };
    };

    // ========================================================================
    // Series B promotion: pick a random underwater player at round-reset time
    // and grant them a Series B backer position with entitlement
    // (amount - totalWithdrawn) * 1.16.
    // ========================================================================

    // Pick a Series B promotion candidate from the current round's losers.
    // Eligibility (phase 1): underwater players who currently have ZERO entries
    // in backerPositions. If none qualify (phase 2 — every underwater player
    // already has a position), fall back to all underwater players. Uses
    // raw_rand for selection — caller must be in an async update context.
    // Returns null if no one is underwater.
    func selectPromotionCandidate() : async ?{ owner : Principal; underwater : Float } {
        var underwaterByPlayer = principalMapNat.empty<Float>();
        for (g in natMap.vals(gameRecords)) {
            if (g.isActive) {
                let loss = g.amount - g.totalWithdrawn;
                if (loss > 0.0) {
                    let prev = switch (principalMapNat.get(underwaterByPlayer, g.player)) {
                        case (null) { 0.0 };
                        case (?v) { v };
                    };
                    underwaterByPlayer := principalMapNat.put(underwaterByPlayer, g.player, prev + loss);
                };
            };
        };

        let allLosers = Iter.toArray(principalMapNat.entries(underwaterByPlayer));
        if (allLosers.size() == 0) { return null };

        let withoutBacker = List.toArray(
            List.filter(
                List.fromArray(allLosers),
                func((p, _) : (Principal, Float)) : Bool {
                    let aHas = switch (backerKeyMap.get(backerPositions, (p, #seriesA))) {
                        case (null) { false };
                        case (?_) { true };
                    };
                    let bHas = switch (backerKeyMap.get(backerPositions, (p, #seriesB))) {
                        case (null) { false };
                        case (?_) { true };
                    };
                    not aHas and not bHas;
                },
            )
        );

        let pool = if (withoutBacker.size() > 0) { withoutBacker } else { allLosers };

        let entropy = try {
            await ic.raw_rand();
        } catch (_) {
            // raw_rand failure: skip promotion this round. The caller
            // (promoteAndReset) still fires triggerGameReset on the null
            // return, so the round closes cleanly and only the Series B
            // grant is lost.
            return null;
        };
        let bytes = Blob.toArray(entropy);
        var seed : Nat = 0;
        var i = 0;
        while (i < 8 and i < bytes.size()) {
            seed := seed * 256 + Nat8.toNat(bytes[i]);
            i += 1;
        };
        let idx = seed % pool.size();
        let (chosen, loss) = pool[idx];
        ?{ owner = chosen; underwater = loss };
    };

    // Apply a Series B promotion. If the player already has a Series B
    // position, merge: sum amount and entitlement. Otherwise create a new
    // Series B row alongside any existing Series A.
    func applySeriesBPromotion(owner : Principal, underwater : Float) {
        let entitlement = underwater * 1.16;
        let now = Time.now();
        let key : BackerKey = (owner, #seriesB);
        switch (backerKeyMap.get(backerPositions, key)) {
            case (null) {
                let fresh : BackerPosition = {
                    owner;
                    amount = underwater;
                    entitlement;
                    startTime = now;
                    isActive = true;
                    backerType = #seriesB;
                    firstDepositDate = ?now;
                };
                backerPositions := backerKeyMap.put(backerPositions, key, fresh);
            };
            case (?existing) {
                let merged : BackerPosition = {
                    existing with
                    amount = existing.amount + underwater;
                    entitlement = existing.entitlement + entitlement;
                };
                backerPositions := backerKeyMap.put(backerPositions, key, merged);
            };
        };

        recordLedger(#seriesBPromotion({ owner; underwater; entitlement }));
    };

    // Async wrapper that performs the Series B promotion (if any eligible
    // candidate exists) before zeroing the round state. Used by all four
    // pot-empty paths in withdrawEarnings and settleCompoundingGame.
    func promoteAndReset(reason : Text) : async () {
        switch (await selectPromotionCandidate()) {
            case (?c) { applySeriesBPromotion(c.owner, c.underwater) };
            case (null) { /* nobody underwater — straight reset */ };
        };
        triggerGameReset(reason);
    };

    // ========================================================================
    // Game reset (called on insolvency)
    //
    // Preserves the full game history: active games at reset time are marked
    // isActive=false with lastUpdateTime=now (their close timestamp). The
    // gameRecords map is NOT wiped, and nextGameId continues monotonically
    // across rounds so historical game IDs remain stable references.
    // ========================================================================

    func triggerGameReset(reason : Text) {
        let now = Time.now();
        let resetRecord : GameResetRecord = {
            resetTime = now;
            reason;
        };
        gameResetHistory := intMap.put(gameResetHistory, now, resetRecord);

        // Mark every active game as closed-by-reset. Preserve all other fields.
        for ((gid, game) in natMap.entries(gameRecords)) {
            if (game.isActive) {
                let closed : GameRecord = {
                    game with
                    isActive = false;
                    lastUpdateTime = now;
                };
                gameRecords := natMap.put(gameRecords, gid, closed);
            };
        };

        // Carry the seed reserve into the new round's pot.
        let newPot = roundSeedReserve;
        let carried = newPot;
        roundSeedReserve := 0.0;
        platformStats := {
            totalDeposits = 0.0;
            totalWithdrawals = 0.0;
            activeGames = 0;
            potBalance = newPot;
            daysActive = 0;
        };
        // nextGameId is NOT reset — IDs stay monotonic across rounds so the
        // full history of games remains addressable by stable IDs.
        recordLedger(#gameReset({ reason; seedReserveCarried = carried }));

        // The gameReset event above carries the OLD roundId (the round being
        // ended). Subsequent events fall under the new round.
        currentRoundId += 1;
    };

    // ========================================================================
    // Earnings calculations (public queries — frontend passes the GameRecord)
    // ========================================================================

    public query func calculateEarnings(game : GameRecord) : async Float {
        let dailyRate = switch (game.plan) {
            case (#simple21Day) { 0.11 };
            case (#compounding15Day) { 0.12 };
            case (#compounding30Day) { 0.09 };
        };
        let maxDurationSeconds = switch (game.plan) {
            case (#simple21Day) { 21.0 * 86400.0 };
            case (#compounding15Day) { 15.0 * 86400.0 };
            case (#compounding30Day) { 30.0 * 86400.0 };
        };
        let timeAlreadyAccounted = Float.fromInt((game.lastUpdateTime - game.startTime) / 1_000_000_000);
        let remainingAllowedTime = Float.max(0.0, maxDurationSeconds - timeAlreadyAccounted);
        let timeSinceLastUpdate = Float.fromInt((Time.now() - game.lastUpdateTime) / 1_000_000_000);
        let timeElapsed = Float.min(timeSinceLastUpdate, remainingAllowedTime);
        let earnings = game.amount * dailyRate * (timeElapsed / 86400.0);
        roundToEightDecimals(game.accumulatedEarnings + earnings);
    };

    public query func calculateCompoundedEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding15Day) {
            Debug.trap("This calculation is only for the 15-day compounding plan");
        };
        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 15.0);
        let dailyRate = 0.12;
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);
        roundToEightDecimals(compoundedEarnings);
    };

    public query func calculateCompounded30DayEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding30Day) {
            Debug.trap("This calculation is only for the 30-day compounding plan");
        };
        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 30.0);
        let dailyRate = 0.09;
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);
        roundToEightDecimals(compoundedEarnings);
    };

    public query func calculateCompoundedROI() : async Float {
        let dailyRate = 0.12;
        let days = 15.0;
        let roi = Float.pow(1.0 + dailyRate, days) - 1.0;
        roundToEightDecimals(roi);
    };

    // ====================================================================
    // SOL payouts (used by withdraw, settle, claimRepayment, payManagement)
    // ====================================================================

    /// Build and broadcast a SOL transfer FROM the pool TO `toAddress`
    /// for `lamports` lamports. Returns the tx signature on success.
    /// Bumps lastNonceValue on success.
    func sendSolPayout(toAddress : Text, lamports : Nat64) : async { #Ok : Text; #Err : Text } {
        let pool = switch (poolAddress) {
            case (null) { return #Err("Pool address not derived") };
            case (?p) { p };
        };
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        let nonceVal = switch (lastNonceValue) {
            case (null) { return #Err("Nonce value cache empty — call adminRefreshNonce") };
            case (?n) { n };
        };
        if (not Base58.isPlausibleSolanaAddress(toAddress)) {
            return #Err("Destination is not a valid Solana address: " # toAddress);
        };

        let advanceIx = SolTx.advanceNonceIx(nonceAddr, pool);
        let transferIx = SolTx.transferIx(pool, toAddress, lamports);
        let compiled = SolTx.compile(pool, nonceVal, [advanceIx, transferIx]);
        let msgBytes = SolTx.serializeMessage(compiled);

        // Pool is the only signer (both feePayer and nonce authority).
        let sigs = await SolSigner.signMulti(keyId, [derivationPathPool()], msgBytes);
        let txBytes = SolTx.assembleTransaction(msgBytes, sigs);

        Cycles.add<system>(RPC_CYCLES);
        let sendRes = await solRpc.sendTransaction(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                transaction = SolRpc.base64Encode(txBytes);
                encoding = ?#base64;
                skipPreflight = ?false;
                preflightCommitment = ?#confirmed;
                maxRetries = ?(3 : Nat32);
                minContextSlot = null;
            },
        );
        switch (SolRpc.unwrapMultiSend(sendRes)) {
            case (#Err(e)) { #Err("sendTransaction failed: " # e) };
            case (#Ok(txSig)) {
                // Refresh nonce.
                Cycles.add<system>(RPC_CYCLES);
                let acctRes = await solRpc.getAccountInfo(
                    SolRpc.rpcSources(solRpcProvider),
                    null,
                    {
                        pubkey = nonceAddr;
                        commitment = ?#confirmed;
                        encoding = ?#base58;
                        dataSlice = null;
                        minContextSlot = null;
                    },
                );
                switch (SolRpc.unwrapMultiAccountInfo(acctRes)) {
                    case (#Ok(?account)) {
                        switch (parseNonceFromAccountData(account.data)) {
                            case (?n) { lastNonceValue := ?n };
                            case (null) {};
                        };
                    };
                    case (_) {};
                };
                #Ok(txSig);
            };
        };
    };

    /// Convert SOL (Float) → lamports (Nat64).
    /// Returns 0 for negative input.
    func solToLamports(sol : Float) : Nat64 {
        let lam = sol * 1_000_000_000.0;
        if (lam < 0.0) { return 0 };
        Nat64.fromNat(Int.abs(Float.toInt(lam)));
    };

    // ========================================================================
    // withdrawEarnings — simple-plan payout, applies tiered exit toll
    // ========================================================================

    /// Resolve a SOL payout destination: prefer a caller-supplied target
    /// address (validated as a plausible Solana pubkey), else fall back to the
    /// caller's canister-derived deposit address. The deposit address is
    /// canister-controlled, so for a real withdrawal the caller MUST pass their
    /// own wallet (e.g. the SIWS Phantom pubkey) as targetAddress to receive
    /// funds they can actually spend.
    func resolvePayoutDestination(caller : Principal, targetAddress : ?Text) : { #Ok : Text; #Err : Text } {
        switch (targetAddress) {
            case (?addr) {
                if (Base58.isPlausibleSolanaAddress(addr)) { #Ok(addr) } else {
                    #Err("Invalid target Solana address: " # addr);
                };
            };
            case (null) {
                switch (principalMapNat.get(depositAddresses, caller)) {
                    case (?addr) { #Ok(addr) };
                    case (null) { #Err("No target address provided and no deposit address on file; pass a Solana address or call getOrCreateDepositAddress first.") };
                };
            };
        };
    };

    public shared ({ caller }) func withdrawEarnings(gameId : Nat, targetAddress : ?Text) : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            switch (natMap.get(gameRecords, gameId)) {
                case (null) { #Err("Game not found") };
                case (?game) {
                    if (game.player != caller) {
                        return #Err("Unauthorized: Only the game owner can withdraw earnings");
                    };
                    if (game.isCompounding) {
                        return #Err("Cannot withdraw from compounding games");
                    };
                    if (not game.isActive) {
                        return #Err("Game is closed (no longer active)");
                    };

                    let earnings = await calculateEarnings(game);
                    let exitToll = calculateExitToll(game, earnings);
                    let netEarnings = roundToEightDecimals(earnings - exitToll);

                    let elapsedDays = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000) / 86400.0;
                    let closePosition = elapsedDays >= 21.0;

                    let originalGame = game;
                    let originalStats = platformStats;
                    let originalRepayments = backerRepayments;
                    let originalSeedReserve = roundSeedReserve;

                    let pot = platformStats.potBalance;
                    let isInsolvent = earnings > pot;

                    if (isInsolvent and pot <= 0.0) {
                        await promoteAndReset("Insufficient funds for payout (pot empty)");
                        return #Err("Game reset: pot is empty");
                    };

                    let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                    let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                    let actualToll = exitToll * scaleFactor;
                    let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                    let tollDetails = distributeExitToll(actualToll);

                    let willClose = closePosition or isInsolvent;
                    let updatedGame : GameRecord = {
                        game with
                        accumulatedEarnings = 0.0;
                        lastUpdateTime = Time.now();
                        totalWithdrawn = game.totalWithdrawn + actualNetEarnings;
                        isActive = if (willClose) false else game.isActive;
                    };
                    gameRecords := natMap.put(gameRecords, gameId, updatedGame);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames =
                            if (willClose and platformStats.activeGames > 0) {
                                platformStats.activeGames - 1
                            } else { platformStats.activeGames };
                    };

                    let netLamports = solToLamports(actualNetEarnings);
                    let solFee : Nat64 = 5_000; // Solana network fee floor
                    if (netLamports > solFee) {
                        let payoutLamports : Nat64 = netLamports - solFee;
                        let destination = switch (resolvePayoutDestination(caller, targetAddress)) {
                            case (#Ok(addr)) { addr };
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(e);
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("SOL payout failed: " # e);
                            };
                            case (#Ok(_txSig)) {};
                        };
                    };

                    recordLedger(#tollDistribution(tollDetails));
                    recordLedger(#withdrawal({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));

                    if (isInsolvent) {
                        await promoteAndReset("Pot drained (partial payout)");
                    };

                    #Ok(actualNetEarnings);
                };
            };
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

    // ========================================================================
    // settleCompoundingGame — compounding-plan payout at maturity
    // ========================================================================

    public shared ({ caller }) func settleCompoundingGame(gameId : Nat, targetAddress : ?Text) : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            switch (natMap.get(gameRecords, gameId)) {
                case (null) { #Err("Game not found") };
                case (?game) {
                    if (game.player != caller) {
                        return #Err("Unauthorized: Only the game owner can settle this game");
                    };
                    if (not game.isCompounding) {
                        return #Err("This function is only for compounding games. Use withdrawEarnings instead.");
                    };
                    if (not game.isActive) {
                        return #Err("Game is already settled");
                    };

                    let timeElapsedSec = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
                    let daysElapsed = timeElapsedSec / 86400.0;
                    let maturityDaysOpt : ?Float = switch (game.plan) {
                        case (#compounding15Day) { ?15.0 };
                        case (#compounding30Day) { ?30.0 };
                        case (#simple21Day) { null };
                    };
                    let maturityDays = switch (maturityDaysOpt) {
                        case (null) {
                            return #Err("Simple games cannot be settled this way");
                        };
                        case (?d) { d };
                    };
                    if (daysElapsed < maturityDays) {
                        return #Err("Game has not matured yet. " # Float.toText(maturityDays - daysElapsed) # " days remaining.");
                    };

                    let dailyRate = switch (game.plan) {
                        case (#compounding15Day) { 0.12 };
                        case (#compounding30Day) { 0.09 };
                        case (#simple21Day) { 0.0 };
                    };
                    let earnings = game.amount * (Float.pow(1.0 + dailyRate, maturityDays) - 1.0);
                    let exitToll = calculateExitToll(game, earnings);
                    let netEarnings = roundToEightDecimals(earnings - exitToll);

                    let originalGame = game;
                    let originalStats = platformStats;
                    let originalRepayments = backerRepayments;
                    let originalSeedReserve = roundSeedReserve;

                    let pot = platformStats.potBalance;
                    let isInsolvent = earnings > pot;

                    if (isInsolvent and pot <= 0.0) {
                        await promoteAndReset("Insufficient funds for compounding game settlement (pot empty)");
                        return #Err("Game reset: pot is empty");
                    };

                    let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                    let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                    let actualToll = exitToll * scaleFactor;
                    let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                    let tollDetails = distributeExitToll(actualToll);

                    let settled : GameRecord = {
                        game with
                        isActive = false;
                        accumulatedEarnings = actualNetEarnings;
                        totalWithdrawn = actualNetEarnings;
                        lastUpdateTime = Time.now();
                    };
                    gameRecords := natMap.put(gameRecords, gameId, settled);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                    };

                    let netLamports = solToLamports(actualNetEarnings);
                    let solFee : Nat64 = 5_000;
                    if (netLamports > solFee) {
                        let payoutLamports : Nat64 = netLamports - solFee;
                        let destination = switch (resolvePayoutDestination(caller, targetAddress)) {
                            case (#Ok(addr)) { addr };
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(e);
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("SOL payout failed: " # e);
                            };
                            case (#Ok(_txSig)) {};
                        };
                    };

                    recordLedger(#tollDistribution(tollDetails));
                    recordLedger(#settlement({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));

                    if (isInsolvent) {
                        await promoteAndReset("Pot drained (partial payout)");
                    };

                    #Ok(actualNetEarnings);
                };
            };
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

    // ========================================================================
    // claimBackerRepayment — transfers backer's accrued repayment balance
    // ========================================================================

    public shared ({ caller }) func claimBackerRepayment(targetAddress : ?Text) : async { #Ok : Float; #Err : Text } {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            let aBalance = switch (backerKeyMap.get(backerRepayments, (caller, #seriesA))) {
                case (null) { 0.0 };
                case (?b) { b };
            };
            let bBalance = switch (backerKeyMap.get(backerRepayments, (caller, #seriesB))) {
                case (null) { 0.0 };
                case (?b) { b };
            };
            let balance = aBalance + bBalance;
            if (balance <= 0.0) { return #Err("No repayment balance to claim") };

            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), 0.0);
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), 0.0);

            let netLamports = solToLamports(balance);
            let solFee : Nat64 = 5_000;
            if (netLamports <= solFee) {
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                return #Err("Claimable balance is below the Solana network fee; wait until your balance grows past 5,000 lamports");
            };
            let payoutLamports : Nat64 = netLamports - solFee;
            let destination = switch (resolvePayoutDestination(caller, targetAddress)) {
                case (#Ok(addr)) { addr };
                case (#Err(e)) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err(e);
                };
            };
            switch (await sendSolPayout(destination, payoutLamports)) {
                case (#Err(e)) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err("SOL payout failed: " # e);
                };
                case (#Ok(_txSig)) {};
            };

            recordLedger(#backerRepaymentClaim({ backer = caller; amount = balance }));

            #Ok(balance);
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

    // ========================================================================
    // Public queries — platform state
    // ========================================================================

    public query func getPlatformStats() : async PlatformStats {
        {
            platformStats with
            daysActive = Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
        };
    };

    public query func getAllGames() : async [GameRecord] {
        Iter.toArray(natMap.vals(gameRecords));
    };

    public query func getAllActiveGames() : async [GameRecord] {
        var active = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.isActive) { active := List.push(g, active) };
        };
        List.toArray(active);
    };

    public query ({ caller }) func getUserGames() : async [GameRecord] {
        var games = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.player == caller) { games := List.push(g, games) };
        };
        List.toArray(games);
    };

    public query func getUserGamesFor(user : Principal) : async [GameRecord] {
        var games = List.nil<GameRecord>();
        for (g in natMap.vals(gameRecords)) {
            if (g.player == user) { games := List.push(g, games) };
        };
        List.toArray(games);
    };

    public query func getGameById(gameId : Nat) : async ?GameRecord {
        natMap.get(gameRecords, gameId);
    };

    public query func getActiveGameCount() : async Nat {
        var count = 0;
        for (g in natMap.vals(gameRecords)) { if (g.isActive) { count += 1 } };
        count;
    };

    public query func getAvailableBalance() : async Float {
        platformStats.potBalance;
    };

    public query func getTotalDeposits() : async Float {
        platformStats.totalDeposits;
    };

    public query func getTotalWithdrawals() : async Float {
        platformStats.totalWithdrawals;
    };

    public query func getDaysActive() : async Nat {
        Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
    };

    public query func getMaxDepositLimit() : async Float {
        Float.max(platformStats.potBalance * 0.2, 5.0);
    };

    public query ({ caller }) func checkDepositRateLimit() : async Bool {
        let oneHourAgo = Time.now() - 3_600_000_000_000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { t > oneHourAgo });
                List.size(filtered) < 3;
            };
        };
    };

    public query func getRoundSeedReserve() : async Float {
        roundSeedReserve;
    };

    public query func getGameResetHistory() : async [GameResetRecord] {
        Iter.toArray(intMap.vals(gameResetHistory));
    };

    // ========================================================================
    // Public queries — backer state
    // ========================================================================

    public query func getBackerPositions() : async [BackerPosition] {
        Iter.toArray(backerKeyMap.vals(backerPositions));
    };

    public query ({ caller }) func getBackerRepaymentBalance() : async Float {
        let a = switch (backerKeyMap.get(backerRepayments, (caller, #seriesA))) {
            case (null) { 0.0 };
            case (?b) { b };
        };
        let b = switch (backerKeyMap.get(backerRepayments, (caller, #seriesB))) {
            case (null) { 0.0 };
            case (?v) { v };
        };
        a + b;
    };

    public query func getBackerRepaymentBalanceFor(user : Principal) : async Float {
        let a = switch (backerKeyMap.get(backerRepayments, (user, #seriesA))) {
            case (null) { 0.0 };
            case (?b) { b };
        };
        let b = switch (backerKeyMap.get(backerRepayments, (user, #seriesB))) {
            case (null) { 0.0 };
            case (?v) { v };
        };
        a + b;
    };

    public query func getAllBackerRepayments() : async [(BackerKey, Float)] {
        Iter.toArray(backerKeyMap.entries(backerRepayments));
    };

    public query func getTotalBackerDebt() : async Float {
        var total = 0.0;
        for (b in backerKeyMap.vals(backerPositions)) { total += b.entitlement };
        total;
    };

    public query func getOldestSeriesABacker() : async ?BackerPosition {
        var oldest : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in backerKeyMap.vals(backerPositions)) {
            if (b.backerType == #seriesA) {
                switch (b.firstDepositDate) {
                    case (null) {};
                    case (?date) {
                        if (oldest == null or date < oldestTime) {
                            oldest := ?b;
                            oldestTime := date;
                        };
                    };
                };
            };
        };
        oldest;
    };

    // ========================================================================
    // Public queries — cover charge, general ledger, canister balance
    // ========================================================================

    public query func getCoverChargeBalance() : async Nat {
        coverChargeBalance;
    };

    public query func getGeneralLedger() : async [GeneralLedgerEntry] {
        Iter.toArray(natMap.vals(generalLedger));
    };

    // Paginated alternative for callers that risk the ~3 MiB query response
    // cap as the ledger grows. `offset` is the 0-based starting ledger ID;
    // `limit` caps the entry count returned. `total` reports the full ledger
    // size so callers can drive a paginator. Past-end requests return an
    // empty vec but still report `total`.
    public query func getGeneralLedgerPage(offset : Nat, limit : Nat) : async {
        entries : [GeneralLedgerEntry];
        total : Nat;
    } {
        let total = nextGeneralLedgerId;
        if (offset >= total or limit == 0) {
            return { entries = []; total };
        };
        let endId = if (offset + limit > total) { total } else { offset + limit };
        var result = List.nil<GeneralLedgerEntry>();
        var id = offset;
        while (id < endId) {
            switch (natMap.get(generalLedger, id)) {
                case (?entry) { result := List.push(entry, result) };
                case (null) {};
            };
            id += 1;
        };
        { entries = List.toArray(List.reverse(result)); total };
    };

    public query func getGeneralLedgerStats() : async {
        totalInflows : Float;
        totalOutflows : Float;
        netFlow : Float;
        entryCount : Nat;
    } {
        var inflows = 0.0;
        var outflows = 0.0;
        var count = 0;
        for (entry in natMap.vals(generalLedger)) {
            count += 1;
            switch (entry.event) {
                case (#deposit(d)) { inflows += d.gross };
                case (#backerDeposit(b)) { inflows += b.amount };
                case (#withdrawal(w)) { outflows += w.netToPlayer + w.toll };
                case (#settlement(s)) { outflows += s.netToPlayer + s.toll };
                case (#backerRepaymentClaim(c)) { outflows += c.amount };
                case (#coverChargeSwept(s)) {
                    outflows += Float.fromInt(s.amountE8s) / 100_000_000.0;
                };
                case (_) {};
            };
        };
        { totalInflows = inflows; totalOutflows = outflows; netFlow = inflows - outflows; entryCount = count };
    };

    public shared ({ caller }) func getCanisterSolBalance() : async Nat64 {
        requireAdmin(caller);
        switch (poolAddress) {
            case (null) { 0 };
            case (?addr) {
                Cycles.add<system>(RPC_CYCLES);
                let res = await solRpc.getBalance(
                    SolRpc.rpcSources(solRpcProvider),
                    null,
                    { pubkey = addr; commitment = ?#confirmed; minContextSlot = null },
                );
                switch (SolRpc.unwrapMultiBalance(res)) {
                    case (#Ok(lamports)) { lamports };
                    case (#Err(_)) { 0 };
                };
            };
        };
    };

    // ========================================================================
    // Admin god-view queries — gated by isAdmin(caller). The IC request
    // envelope is signed by the caller's identity and the replica verifies
    // the signature even on `query` methods, so this is effective access
    // control. Responses are NOT certified (single-replica reads) — sufficient
    // for read-only viewing; an admin wanting paranoia can repeat as update.
    // ========================================================================

    public query ({ caller }) func adminIsAdmin() : async Bool {
        isAdmin(caller);
    };

    public query func getCurrentRoundId() : async Nat {
        currentRoundId;
    };

    public query ({ caller }) func adminGetCurrentRoundId() : async Nat {
        requireAdmin(caller);
        currentRoundId;
    };

    public query ({ caller }) func adminGetEventsByRound(roundId : Nat) : async [GeneralLedgerEntry] {
        requireAdmin(caller);
        var matches = List.nil<GeneralLedgerEntry>();
        // natMap iterates in id (key) order ascending; List.push reverses, so
        // reverse at the end to recover ascending order.
        for (entry in natMap.vals(generalLedger)) {
            if (entry.roundId == roundId) {
                matches := List.push(entry, matches);
            };
        };
        List.toArray(List.reverse(matches));
    };

    public query ({ caller }) func adminGetRoundSummaries() : async [RoundSummary] {
        requireAdmin(caller);

        var counts = natMap.empty<Nat>();
        var seedCarried = natMap.empty<Float>();
        var endTimes = natMap.empty<Int>();
        var endReasons = natMap.empty<Text>();

        for (entry in natMap.vals(generalLedger)) {
            let prev = switch (natMap.get(counts, entry.roundId)) {
                case (null) { 0 };
                case (?n) { n };
            };
            counts := natMap.put(counts, entry.roundId, prev + 1);
            switch (entry.event) {
                case (#gameReset(r)) {
                    seedCarried := natMap.put(seedCarried, entry.roundId, r.seedReserveCarried);
                    endTimes := natMap.put(endTimes, entry.roundId, entry.timestamp);
                    endReasons := natMap.put(endReasons, entry.roundId, r.reason);
                };
                case (_) {};
            };
        };

        var resultList = List.nil<RoundSummary>();
        var r : Nat = 1;
        while (r <= currentRoundId) {
            let count = switch (natMap.get(counts, r)) {
                case (null) { 0 };
                case (?n) { n };
            };
            let end = natMap.get(endTimes, r);
            let reason = natMap.get(endReasons, r);
            let seed = switch (natMap.get(seedCarried, r)) {
                case (null) { 0.0 };
                case (?v) { v };
            };
            // Round 1's startTime is canister genesis (we don't track it
            // explicitly — frontend can fall back to the earliest event
            // timestamp for that round). For round N>1, startTime = prior
            // round's gameReset timestamp.
            let startT : Int = if (r == 1) { 0 } else {
                switch (natMap.get(endTimes, r - 1)) {
                    case (null) { 0 };
                    case (?t) { t };
                };
            };
            resultList := List.push({
                roundId = r;
                startTime = startT;
                endTime = end;
                endReason = reason;
                eventCount = count;
                seedReserveCarried = seed;
            } : RoundSummary, resultList);
            r += 1;
        };
        List.toArray(List.reverse(resultList));
    };

    // Compute the live snapshot for an active game — what they'd get if they
    // withdrew/settled right now, accounting for pot solvency.
    func computeActivePlanSnapshot(game : GameRecord, now : Int, pot : Float) : ActivePlanSnapshot {
        let elapsedSec = Float.fromInt((now - game.startTime) / 1_000_000_000);
        let daysElapsed = elapsedSec / 86400.0;

        let (maxDays, grossEarnings) : (Float, Float) = if (game.isCompounding) {
            switch (game.plan) {
                case (#compounding15Day) {
                    let d = Float.min(daysElapsed, 15.0);
                    (15.0, roundToEightDecimals(game.amount * (Float.pow(1.12, d) - 1.0)));
                };
                case (#compounding30Day) {
                    let d = Float.min(daysElapsed, 30.0);
                    (30.0, roundToEightDecimals(game.amount * (Float.pow(1.09, d) - 1.0)));
                };
                case (#simple21Day) { (0.0, 0.0) }; // unreachable — isCompounding implies a compounding plan
            };
        } else {
            // Simple plan — mirror calculateEarnings (11% daily, 21-day cap,
            // includes already-accumulated earnings on partial withdrawals).
            let timeAlreadyAccounted = Float.fromInt((game.lastUpdateTime - game.startTime) / 1_000_000_000);
            let remainingAllowedTime = Float.max(0.0, 21.0 * 86400.0 - timeAlreadyAccounted);
            let timeSinceLastUpdate = Float.fromInt((now - game.lastUpdateTime) / 1_000_000_000);
            let timeElapsed = Float.min(timeSinceLastUpdate, remainingAllowedTime);
            let increment = game.amount * 0.11 * (timeElapsed / 86400.0);
            (21.0, roundToEightDecimals(game.accumulatedEarnings + increment));
        };

        let exitToll = calculateExitToll(game, grossEarnings);
        let netClaimable = roundToEightDecimals(grossEarnings - exitToll);
        let isMatured = daysElapsed >= maxDays;
        let daysToMaturity = if (isMatured) { 0.0 } else { maxDays - daysElapsed };

        {
            game;
            currentGrossEarnings = grossEarnings;
            currentExitToll = exitToll;
            currentNetClaimable = netClaimable;
            daysElapsed;
            daysToMaturity;
            isMatured;
            wouldBeInsolvent = grossEarnings > pot;
        };
    };

    public query ({ caller }) func adminGetActivePlansSnapshot() : async [ActivePlanSnapshot] {
        requireAdmin(caller);
        let now = Time.now();
        let pot = platformStats.potBalance;
        var snapshots = List.nil<ActivePlanSnapshot>();
        for (game in natMap.vals(gameRecords)) {
            if (game.isActive) {
                snapshots := List.push(computeActivePlanSnapshot(game, now, pot), snapshots);
            };
        };
        List.toArray(List.reverse(snapshots));
    };

    // Convenience: every event referencing a specific gameId. Used by the
    // admin god view to show withdrawal/deposit history for one plan.
    public query ({ caller }) func adminGetEventsForGame(gameId : Nat) : async [GeneralLedgerEntry] {
        requireAdmin(caller);
        var matches = List.nil<GeneralLedgerEntry>();
        for (entry in natMap.vals(generalLedger)) {
            let belongs = switch (entry.event) {
                case (#deposit(d)) { d.gameId == gameId };
                case (#withdrawal(w)) { w.gameId == gameId };
                case (#settlement(s)) { s.gameId == gameId };
                case (#coverChargeAccrued(c)) { c.gameId == gameId };
                case (#backdatedGameCreated(b)) { b.gameId == gameId };
                case (_) { false };
            };
            if (belongs) { matches := List.push(entry, matches) };
        };
        List.toArray(List.reverse(matches));
    };

    // ========================================================================
    // PRE-BLACKHOLE TEST HATCHES — DELETE THIS ENTIRE BLOCK BEFORE BLACKHOLING.
    // All methods below are gated on caller == TEST_ADMIN (init arg).
    // ========================================================================

    // adminMergeBackerPosition — merge `from`'s backer position into `to`.
    // Sums amount + entitlement, keeps the earlier startTime / firstDepositDate,
    // also moves any accumulated backerRepayments. Used to consolidate smoke-test
    // backer positions left over from pre-cutover.
    public shared ({ caller }) func adminMergeBackerPosition(
        from : Principal,
        to : Principal,
    ) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (from == to) { return #Err("from and to must differ") };

        acquireGlobalLock();
        try {
            let fromPos = switch (backerKeyMap.get(backerPositions, (from, #seriesA))) {
                case (null) { return #Err("from principal has no backer position") };
                case (?p) { p };
            };

            switch (backerKeyMap.get(backerPositions, (to, #seriesA))) {
                case (null) {
                    backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                        fromPos with owner = to;
                    });
                };
                case (?toPos) {
                    let mergedStart = if (toPos.startTime <= fromPos.startTime) { toPos.startTime } else { fromPos.startTime };
                    let mergedFirst = switch (toPos.firstDepositDate, fromPos.firstDepositDate) {
                        case (?d1, ?d2) { if (d1 <= d2) { ?d1 } else { ?d2 } };
                        case (?d, null) { ?d };
                        case (null, ?d) { ?d };
                        case (null, null) { null };
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                        toPos with
                        amount = toPos.amount + fromPos.amount;
                        entitlement = toPos.entitlement + fromPos.entitlement;
                        startTime = mergedStart;
                        firstDepositDate = mergedFirst;
                    });
                };
            };

            backerPositions := backerKeyMap.delete(backerPositions, (from, #seriesA));

            let fromRepay = switch (backerKeyMap.get(backerRepayments, (from, #seriesA))) {
                case (null) { 0.0 };
                case (?r) { r };
            };
            if (fromRepay > 0.0) {
                let toRepay = switch (backerKeyMap.get(backerRepayments, (to, #seriesA))) {
                    case (null) { 0.0 };
                    case (?r) { r };
                };
                backerRepayments := backerKeyMap.put(backerRepayments, (to, #seriesA), toRepay + fromRepay);
            };
            backerRepayments := backerKeyMap.delete(backerRepayments, (from, #seriesA));

            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };

    // adminClearAllBackerPositions — wipes every backer position and every
    // pending backer repayment balance. Intended for a one-shot "start fresh"
    // reset when all live positions are admin/test sock puppets. Does NOT
    // touch platformStats.potBalance — the ICP previously contributed via
    // addBackerMoney stays in the canister and remains tracked in the pot.
    public shared ({ caller }) func adminClearAllBackerPositions() : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            backerPositions := backerKeyMap.empty<BackerPosition>();
            backerRepayments := backerKeyMap.empty<Float>();
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };

    /// Admin: compute the difference between the pool address's actual
    /// on-chain balance and the sum of internal accounting (pot +
    /// roundSeedReserve + repayments + coverChargeAccrual). If positive
    /// (untracked dust), send it to the testAdmin's deposit address.
    /// No-op otherwise.
    public shared ({ caller }) func adminSweepUntracked() : async { #Ok : Text; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            let pool = switch (poolAddress) {
                case (null) { return #Err("Pool not derived") };
                case (?p) { p };
            };
            Cycles.add<system>(RPC_CYCLES);
            let balRes = await solRpc.getBalance(
                SolRpc.rpcSources(solRpcProvider),
                null,
                { pubkey = pool; commitment = ?#confirmed; minContextSlot = null },
            );
            let actualLamports = switch (SolRpc.unwrapMultiBalance(balRes)) {
                case (#Ok(b)) { b };
                case (#Err(e)) { return #Err("getBalance: " # e) };
            };

            var repaymentSum : Float = 0.0;
            for ((_, amount) in backerKeyMap.entries(backerRepayments)) {
                repaymentSum += amount;
            };
            let internalFloat = platformStats.potBalance + roundSeedReserve + repaymentSum;
            let internalLamports : Nat64 = solToLamports(internalFloat) + coverChargeAccrualLamports;

            if (actualLamports <= internalLamports) {
                return #Err("No untracked balance (actual=" # Nat64.toText(actualLamports) # ", internal=" # Nat64.toText(internalLamports) # ")");
            };
            let untracked : Nat64 = actualLamports - internalLamports;
            let solFee : Nat64 = 5_000;
            if (untracked <= solFee) {
                return #Err("Untracked balance below fee");
            };
            let payout : Nat64 = untracked - solFee;

            let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?addr) { addr };
                case (null) {
                    return #Err("testAdmin has no deposit address; call getOrCreateDepositAddress as testAdmin first");
                };
            };
            switch (await sendSolPayout(destination, payout)) {
                case (#Ok(txSig)) { #Ok(txSig) };
                case (#Err(e)) { #Err(e) };
            };
        } finally {
            releaseGlobalLock();
        };
    };

    // adminForceReset — manually close the current round. Acquires the global
    // lock and runs triggerGameReset. Use for stuck-state recovery if a prior
    // pot-empty path trapped after a successful payout but before the round
    // closed (e.g., raw_rand failure inside promoteAndReset before Task 4 was
    // applied, or any future analogous edge case).
    public shared ({ caller }) func adminForceReset(reason : Text) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            triggerGameReset("admin force-reset: " # reason);
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };

    // ========================================================================
    // ICRC-21 consent messages, ICRC-28 trusted origins, ICRC-10 standards
    // ========================================================================

    public shared func icrc21_canister_call_consent_message(request : Icrc21.ConsentMessageRequest) : async Icrc21.ConsentMessageResponse {
        Icrc21.consentMessage(request);
    };

    public query func icrc28_trusted_origins() : async Icrc21.TrustedOriginsResponse {
        Icrc21.trustedOrigins();
    };

    public query func icrc10_supported_standards() : async [Icrc21.StandardRecord] {
        Icrc21.supportedStandards();
    };

    // ====================================================================
    // STUBS — bodies land in later tasks. Each function traps for now so
    // the IDL is present and the canister upgrades smoothly later.
    // ====================================================================

    public shared ({ caller }) func bootstrap(recentBlockhashOverride : ?Text) : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (bootstrapped) { return #Ok("already-bootstrapped") };

        acquireGlobalLock();
        try {
            // 1. Derive pool + nonce addresses.
            let pool = await ensurePoolAddress();
            let nonce = await ensureNonceAccountAddress();

            // 2. Confirm pool funded (~0.003 SOL = 3M lamports minimum).
            Cycles.add<system>(RPC_CYCLES);
            let balanceRes = await solRpc.getBalance(
                SolRpc.rpcSources(solRpcProvider),
                null,
                { pubkey = pool; commitment = ?#confirmed; minContextSlot = null },
            );
            let balance = switch (SolRpc.unwrapMultiBalance(balanceRes)) {
                case (#Ok(b)) { b };
                case (#Err(e)) {
                    return #Err("getBalance(pool) failed: " # e);
                };
            };
            if (balance < (3_000_000 : Nat64)) {
                return #Err("Pool address " # pool # " has only " # Nat64.toText(balance) # " lamports; needs ≥3,000,000 (≈0.003 SOL). Fund and retry.");
            };

            // 3. Build the bootstrap tx: createAccount(pool → nonce, 1.5M lamports, 80 bytes, SystemProgram) + initializeNonceAccount(nonce, pool).
            let createIx = SolTx.createAccountIx(
                pool,
                nonce,
                1_500_000 : Nat64,   // rent-exempt minimum for 80 bytes is ~1.44M; round up.
                SolTx.NONCE_ACCOUNT_SPACE,
                SolTx.SYSTEM_PROGRAM_ID,
            );
            let initIx = SolTx.initializeNonceIx(nonce, pool);

            // 4. Fetch a recent blockhash. Prefer the admin-supplied
            //    override (operator pastes a known-fresh blockhash) so we
            //    don't depend on sol-rpc's jsonRequest passthrough, which
            //    has been flaky. Fall back to fetchRecentBlockhashWithRetry
            //    when no override is provided.
            let blockhash = switch (recentBlockhashOverride) {
                case (?h) { h };
                case (null) {
                    switch (await fetchRecentBlockhashWithRetry(5)) {
                        case (?h) { h };
                        case (null) { return #Err("getLatestBlockhash failed after 5 retries. Retry with an admin-supplied blockhash via bootstrap(opt \"<base58>\").") };
                    };
                };
            };

            // 5. Compile + serialize the message.
            let compiled = SolTx.compile(pool, blockhash, [createIx, initIx]);
            let msgBytes = SolTx.serializeMessage(compiled);

            // 6. Sign with pool + nonce derivation paths. The bootstrap tx
            //    is unique in that BOTH the funder (pool) and the new
            //    account (nonce) must sign.
            let sigs = await SolSigner.signMulti(
                keyId,
                [derivationPathPool(), derivationPathNonce()],
                msgBytes,
            );

            // 7. Assemble + broadcast.
            let txBytes = SolTx.assembleTransaction(msgBytes, sigs);
            Cycles.add<system>(RPC_CYCLES);
            let sendRes = await solRpc.sendTransaction(
                SolRpc.rpcSources(solRpcProvider),
                null,
                {
                    transaction = SolRpc.base64Encode(txBytes);
                    encoding = ?#base64;
                    skipPreflight = ?false;
                    preflightCommitment = ?#confirmed;
                    maxRetries = ?(3 : Nat32);
                    minContextSlot = null;
                },
            );
            let txSig = switch (SolRpc.unwrapMultiSend(sendRes)) {
                case (#Ok(s)) { s };
                case (#Err(e)) { return #Err("sendTransaction failed: " # e) };
            };

            // 8. Fetch nonce account state to read the initial nonce value.
            //    Try a few times — confirmation may lag the send.
            var attempts : Nat = 0;
            var initialNonce : ?Text = null;
            while (attempts < 10 and initialNonce == null) {
                Cycles.add<system>(RPC_CYCLES);
                let acctRes = await solRpc.getAccountInfo(
                    SolRpc.rpcSources(solRpcProvider),
                    null,
                    {
                        pubkey = nonce;
                        commitment = ?#confirmed;
                        encoding = ?#base58;
                        dataSlice = null;
                        minContextSlot = null;
                    },
                );
                switch (SolRpc.unwrapMultiAccountInfo(acctRes)) {
                    case (#Ok(?account)) {
                        initialNonce := parseNonceFromAccountData(account.data);
                    };
                    case (_) {};
                };
                attempts += 1;
            };
            switch (initialNonce) {
                case (?n) {
                    lastNonceValue := ?n;
                    bootstrapped := true;
                    // Arm the recurring deposit-detection timer on first
                    // bootstrap (fresh install). Upgrades re-arm via postupgrade.
                    startDetectionTimer<system>();
                    #Ok("bootstrapped; nonce-account=" # nonce # " initial-nonce=" # n # " tx=" # txSig);
                };
                case (null) {
                    #Err("createAccount+initializeNonceAccount broadcast as tx " # txSig # ", but getAccountInfo could not parse the nonce body after 10 retries. Inspect on devnet explorer and re-run bootstrap.");
                };
            };
        } finally {
            releaseGlobalLock();
        };
    };

    // ====================================================================
    // Deposit detection
    // ====================================================================

    /// A single new-signature record discovered for a deposit address.
    type DetectedSignature = {
        address : Text;
        principal : Principal;
        signature : Text;
        slot : Nat64;
    };

    /// Scan a single deposit address for new inbound signatures.
    /// Returns the list of signatures observed past lastSeenSignature
    /// (chronologically ordered: oldest first). DOES NOT mutate
    /// lastSeenSignature; that happens after the credit step succeeds in
    /// Task 14.
    func scanAddress(address : Text, principal : Principal) : async [DetectedSignature] {
        let cursor = textMap.get(lastSeenSignature, address);
        // getSignaturesForAddress returns newest-first. We page until we
        // see the cursor (or exhaust). For M1 we do one page (up to 100
        // signatures) — devnet test volume is well under that.
        Cycles.add<system>(RPC_CYCLES);
        let res = await solRpc.getSignaturesForAddress(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                pubkey = address;
                limit = ?(100 : Nat32);
                before = null;
                until = cursor;
                commitment = ?#confirmed;
                minContextSlot = null;
            },
        );
        switch (SolRpc.unwrapMultiSignatures(res)) {
            case (#Err(e)) {
                // Log instead of silently returning [] — a real RPC/consensus
                // failure would otherwise look identical to "no new deposits".
                Debug.print("scanAddress getSignaturesForAddress failed for " # address # ": " # e);
                [];
            };
            case (#Ok(sigs)) {
                // Reverse so we process oldest-first.
                let buf = Buffer.Buffer<DetectedSignature>(sigs.size());
                var i : Nat = sigs.size();
                while (i > 0) {
                    i -= 1;
                    let s = sigs[i];
                    if (s.err == null) {
                        buf.add({
                            address;
                            principal;
                            signature = s.signature;
                            slot = s.slot;
                        });
                    };
                };
                Buffer.toArray(buf);
            };
        };
    };

    // ====================================================================
    // Deposit credit + sweep helpers (Task 14)
    // ====================================================================

    /// Cover charge rate: 4% expressed as basis points (400 / 10_000).
    transient let COVER_CHARGE_RATE_LAMPORTS_BPS : Nat64 = 400;

    /// Apply basis points to a Nat64 amount. bpsApply(x, 400) = 4% of x.
    func bpsApply(amount : Nat64, bps : Nat64) : Nat64 {
        amount * bps / 10_000;
    };

    /// Convert lamports → SOL Float. Matches the Float convention used
    /// by Game.amount, platformStats, etc. throughout the actor.
    func lamportsToSol(lamports : Nat64) : Float {
        Float.fromInt(Nat64.toNat(lamports)) / 1_000_000_000.0;
    };

    /// Build + sign + broadcast a sweep tx from `fromAddress` (per-user
    /// deposit address) to the pool address for `lamports` lamports.
    /// Two signers: per-user address (feePayer + transfer source) and pool
    /// address (nonce authority on advance_nonce_account). Order: [per-user, pool].
    /// Leaves ~5_000 lamports dust on the per-user address (tx fee floor).
    /// Bumps lastNonceValue on success.
    func sweepToPool(fromAddress : Text, fromDerivationPath : [Blob], lamports : Nat64) : async { #Ok : Text; #Err : Text } {
        let pool = switch (poolAddress) {
            case (null) { return #Err("Pool address not derived") };
            case (?p) { p };
        };
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        let nonceVal = switch (lastNonceValue) {
            case (null) { return #Err("Nonce value cache empty — call adminRefreshNonce") };
            case (?n) { n };
        };

        // Sweep tx has TWO signatures (per-user address as feePayer, and
        // pool as nonce authority), so the on-chain fee is 10_000 lamports.
        // Reserve that off the transfer amount; refuse if balance is below
        // the floor to avoid building a tx Solana will reject with
        // ResultWithNegativeLamports (custom program error 0x1).
        let solFee : Nat64 = 10_000;
        if (lamports <= solFee) {
            return #Err("Detected amount below network-fee floor (≤" # Nat64.toText(solFee) # " lamports)");
        };
        let sweepLamports : Nat64 = lamports - solFee;

        let nonceIx = SolTx.advanceNonceIx(nonceAddr, pool);
        let transferIx = SolTx.transferIx(fromAddress, pool, sweepLamports);
        // Compile with the per-user address as feePayer so the sweep
        // tx's network fee comes out of the per-user dust — pool pays nothing.
        let compiled = SolTx.compile(fromAddress, nonceVal, [nonceIx, transferIx]);
        let msgBytes = SolTx.serializeMessage(compiled);
        // Two signers: per-user (feePayer + transfer source) first, then pool
        // (nonce authority).
        let sigs = await SolSigner.signMulti(keyId, [fromDerivationPath, derivationPathPool()], msgBytes);
        let txBytes = SolTx.assembleTransaction(msgBytes, sigs);

        Cycles.add<system>(RPC_CYCLES);
        let sendRes = await solRpc.sendTransaction(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                transaction = SolRpc.base64Encode(txBytes);
                encoding = ?#base64;
                skipPreflight = ?false;
                preflightCommitment = ?#confirmed;
                maxRetries = ?(3 : Nat32);
                minContextSlot = null;
            },
        );
        switch (SolRpc.unwrapMultiSend(sendRes)) {
            case (#Err(e)) { #Err("sendTransaction failed: " # e) };
            case (#Ok(txSig)) {
                // Refresh the nonce cache after a successful broadcast so
                // future txs use the advanced value.
                Cycles.add<system>(RPC_CYCLES);
                let acctRes = await solRpc.getAccountInfo(
                    SolRpc.rpcSources(solRpcProvider),
                    null,
                    {
                        pubkey = nonceAddr;
                        commitment = ?#confirmed;
                        encoding = ?#base58;
                        dataSlice = null;
                        minContextSlot = null;
                    },
                );
                switch (SolRpc.unwrapMultiAccountInfo(acctRes)) {
                    case (#Ok(?account)) {
                        switch (parseNonceFromAccountData(account.data)) {
                            case (?n) { lastNonceValue := ?n };
                            case (null) {};
                        };
                    };
                    case (_) {};
                };
                #Ok(txSig);
            };
        };
    };

    /// For a single DetectedSignature, fetch the tx, compute the inbound
    /// lamports, match against an open intent, credit the game, and
    /// sweep to the pool.
    ///
    /// Safety properties:
    /// - lastSeenSignature advances ONLY after credit + sweep complete
    ///   (at-least-once guarantee — a failure leaves cursor unmoved so the
    ///   next detection pass retries).
    /// - Outbound txs (postBalance ≤ preBalance) advance cursor without
    ///   crediting (catches re-observations of our own sweep txs).
    /// - Unmatched deposits (no intent, or TTL expired) log via Debug.print
    ///   and return Ok(0) without advancing cursor — admin resolves later
    ///   via adminCreditManualDeposit (Task 20).
    // Returns #Ok(?gameId): ?gid identifies the GameRecord created by a genuine
    // credit (gid can be 0 — the first game on a fresh canister, i.e. the M3
    // mainnet case), and null means the signature was a non-deposit / unmatched
    // tx that only advanced the cursor. The optional disambiguates "credited
    // game 0" from "no credit" — a plain Nat would collide on 0.
    func creditDeposit(sig : DetectedSignature) : async { #Ok : ?Nat; #Err : Text } {
        // 1. Fetch transaction details.
        // We use encoding = base64 (the default binary form).  The sol-rpc
        // canister returns the full meta (preBalances/postBalances) which is
        // all we need to detect inbound SOL. We cannot easily extract
        // accountKeys from the binary-encoded tx, so we scan all balance
        // deltas and pick the largest positive increase — which for a simple
        // SOL transfer is unambiguous.
        Cycles.add<system>(RPC_CYCLES);
        let txRes = await solRpc.getTransaction(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                signature = sig.signature;
                commitment = ?#confirmed;
                maxSupportedTransactionVersion = ?(0 : Nat8);
                encoding = ?#base64;
            },
        );
        let confirmedTx = switch (SolRpc.unwrapMultiTransaction(txRes)) {
            case (#Err(e)) { return #Err("getTransaction failed: " # e) };
            case (#Ok(null)) { return #Err("Transaction not found / not confirmed yet") };
            case (#Ok(?t)) { t };
        };

        let meta = switch (confirmedTx.transaction.meta) {
            case (null) { return #Err("Transaction meta missing") };
            case (?m) { m };
        };

        // 2. Find the largest positive balance delta across all accounts.
        //    For a simple SOL inbound transfer this is the receiving account
        //    (our deposit address). We take the MAX positive delta to handle
        //    the case where multiple accounts received dust in the same tx.
        let preBalances = meta.preBalances;
        let postBalances = meta.postBalances;
        let n = Nat.min(preBalances.size(), postBalances.size());
        var maxDelta : Nat64 = 0;
        var k : Nat = 0;
        while (k < n) {
            let pre = preBalances[k];
            let post = postBalances[k];
            if (post > pre) {
                let delta : Nat64 = post - pre;
                if (delta > maxDelta) { maxDelta := delta };
            };
            k += 1;
        };
        let inboundLamports : Nat64 = maxDelta;
        if (inboundLamports == 0) {
            // All balance deltas are non-positive → outbound or no-op tx.
            // Advance the cursor without crediting (matches our own sweeps).
            lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
            return #Ok(null);  // no game credited: outbound / no-op tx
        };

        // 3. Find an open intent for this principal that matches the amount
        //    within ±5% tolerance and is not TTL-expired.
        var matched : ?DepositIntent = null;
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == sig.principal and not intent.fulfilled) {
                let expected = intent.expectedAmountLamports;
                let tol = bpsApply(expected, 500); // 5% tolerance
                let lo : Nat64 = if (expected > tol) { expected - tol } else { 0 };
                let hi : Nat64 = expected + tol;
                if (inboundLamports >= lo and inboundLamports <= hi and Time.now() <= intent.expiresAt) {
                    matched := ?intent;
                };
            };
        };

        let intent = switch (matched) {
            case (null) {
                // Unmatched deposit: confirmed inbound, but no open intent
                // matches the amount. Advance the cursor anyway — otherwise we
                // re-scan it every tick AND, critically, a stale deposit could
                // later match a NEW same-amount intent for the same principal
                // and double-credit. Funds stay on the address and remain
                // recoverable via adminCreditManualDeposit; the cursor only
                // controls auto-detection, never fund recovery. (Normal flow
                // is intent-before-deposit, so a genuine deposit is matched on
                // the tick that observes it, before this branch is reached.)
                Debug.print("Unmatched deposit on " # sig.address # ": " # Nat64.toText(inboundLamports) # " lamports sig=" # sig.signature # " (cursor advanced)");
                lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
                return #Ok(null);  // no game credited: no matching open intent
            };
            case (?i) { i };
        };

        // 4. Compute cover charge + net.
        let coverChargeLamports = bpsApply(inboundLamports, COVER_CHARGE_RATE_LAMPORTS_BPS);
        let netLamports = inboundLamports - coverChargeLamports;
        let depositSol = lamportsToSol(inboundLamports);
        let coverChargeSol = lamportsToSol(coverChargeLamports);
        let netSol = lamportsToSol(netLamports);

        // 5. Create the GameRecord, mark intent fulfilled, update stats,
        //    record ledger events.
        let gameId = nextGameId;
        nextGameId += 1;
        coverChargeAccrualLamports += coverChargeLamports;

        if (coverChargeLamports > 0) {
            recordLedger(#coverChargeAccrued({
                gameId;
                player = intent.principal;
                // Field is named amountE8s but we reuse it for lamports per spec.
                amountE8s = Nat64.toNat(coverChargeLamports);
            }));
        };

        let newGame : GameRecord = {
            id = gameId;
            player = intent.principal;
            plan = intent.plan;
            amount = depositSol;
            startTime = Time.now();
            isCompounding = switch (intent.plan) {
                case (#simple21Day) { false };
                case (_) { true };
            };
            isActive = true;
            lastUpdateTime = Time.now();
            accumulatedEarnings = 0.0;
            totalWithdrawn = 0.0;
        };
        gameRecords := natMap.put(gameRecords, gameId, newGame);
        platformStats := {
            platformStats with
            totalDeposits = platformStats.totalDeposits + depositSol;
            activeGames = platformStats.activeGames + 1;
            potBalance = platformStats.potBalance + netSol;
        };
        recordLedger(#deposit({
            player = intent.principal;
            gameId;
            gross = depositSol;
            coverCharge = coverChargeSol;
            netToPot = netSol;
            plan = intent.plan;
            isCompounding = newGame.isCompounding;
        }));

        // Record the per-user deposit timestamp (rate-limit bookkeeping).
        let now = Time.now();
        let oneHourAgo = now - 3_600_000_000_000;
        switch (principalMapNat.get(depositTimestamps, intent.principal)) {
            case (null) {
                depositTimestamps := principalMapNat.put(
                    depositTimestamps,
                    intent.principal,
                    List.push(now, List.nil()),
                );
            };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { t > oneHourAgo });
                depositTimestamps := principalMapNat.put(
                    depositTimestamps,
                    intent.principal,
                    List.push(now, filtered),
                );
            };
        };

        // Mark intent fulfilled.
        pendingIntents := natMap.put(pendingIntents, intent.id, { intent with fulfilled = true });

        // 6. Sweep deposit address → pool.
        switch (await sweepToPool(sig.address, derivationPathForPrincipal(intent.principal), inboundLamports)) {
            case (#Err(e)) {
                // Pot already credited. Sweep failure leaves SOL on the
                // per-user address; admin can retry. We still advance the
                // cursor so the detection pass doesn't loop on this sig.
                Debug.print("Sweep failed for " # sig.signature # ": " # e);
            };
            case (#Ok(_)) {};
        };

        // 7. Advance the per-address cursor — only here, after all mutations.
        lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);

        #Ok(?gameId);
    };

    public query func getPoolAddress() : async ?Text { poolAddress };
    public query func getNonceAccountAddress() : async ?Text { nonceAccountAddress };
    public query func isBootstrapped() : async Bool { bootstrapped };

    /// Admin-callable: derive the pool address via threshold-Schnorr and
    /// cache it. Idempotent — subsequent calls just return the cached
    /// value. Must be called once before bootstrap() so the operator can
    /// fund the pool.
    public shared ({ caller }) func adminDerivePoolAddress() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        try {
            let addr = await ensurePoolAddress();
            #Ok(addr);
        } catch (e) {
            #Err("Failed to derive pool address: " # Error.message(e));
        };
    };

    public shared ({ caller }) func getOrCreateDepositAddress() : async { #Ok : Text; #Err : Text } {
        requireAuthenticated(caller);
        switch (principalMapNat.get(depositAddresses, caller)) {
            case (?addr) { #Ok(addr) };
            case (null) {
                acquireCallerLock(caller);
                try {
                    let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                    depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                    addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                    #Ok(addr);
                } catch (e) {
                    #Err("Failed to derive deposit address: " # Error.message(e));
                } finally {
                    releaseCallerLock(caller);
                };
            };
        };
    };

    public query ({ caller }) func getMyDepositAddress() : async ?Text {
        principalMapNat.get(depositAddresses, caller);
    };

    public query func getDepositAddressFor(p : Principal) : async ?Text {
        principalMapNat.get(depositAddresses, p);
    };

    public shared ({ caller }) func prepareSolDeposit(args : {
        plan : GamePlan;
        expectedAmountLamports : Nat64;
    }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        if (args.expectedAmountLamports < MIN_DEPOSIT_LAMPORTS) {
            return #Err("Minimum deposit is 0.05 SOL (50,000,000 lamports)");
        };
        if (not bootstrapped) {
            return #Err("Canister not bootstrapped yet — operator must run bootstrap() first");
        };

        acquireCallerLock(caller);
        try {
            // Per-user rate limit, identical to ponzi_math's 3-deposits-per-hour
            // gate. Sourced from the existing depositTimestamps map.
            let currentTime = Time.now();
            let oneHourAgo = currentTime - 3_600_000_000_000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(
                        timestamps,
                        func(t) { t > oneHourAgo },
                    );
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };

            // Ensure the user has a deposit address.
            let depositAddr = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?a) { a };
                case (null) {
                    // Derive inline. Same logic as getOrCreateDepositAddress
                    // but without recursive lock acquisition.
                    let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                    depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                    addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                    addr;
                };
            };

            let intent : DepositIntent = {
                id = nextIntentId;
                principal = caller;
                plan = args.plan;
                expectedAmountLamports = args.expectedAmountLamports;
                createdAt = currentTime;
                expiresAt = currentTime + INTENT_TTL_NS;
                fulfilled = false;
            };
            pendingIntents := natMap.put(pendingIntents, nextIntentId, intent);
            let intentId = nextIntentId;
            nextIntentId += 1;

            #Ok({ intentId; depositAddress = depositAddr });
        } finally {
            releaseCallerLock(caller);
        };
    };

    public query ({ caller }) func getMyPendingIntents() : async [DepositIntent] {
        var out = List.nil<DepositIntent>();
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == caller and not intent.fulfilled) {
                out := List.push(intent, out);
            };
        };
        List.toArray(out);
    };

    public query ({ caller }) func adminGetAllIntents() : async [DepositIntent] {
        requireAdmin(caller);
        Iter.toArray(natMap.vals(pendingIntents));
    };

    /// Scan one deposit address for new signatures and credit any that match
    /// an open intent. Returns the number of GameRecords created. Shared by
    /// the manual admin sweep and the auto-detection timer.
    func scanAndCredit(address : Text, principal : Principal) : async Nat {
        var credits : Nat = 0;
        let sigs = await scanAddress(address, principal);
        for (sig in sigs.vals()) {
            switch (await creditDeposit(sig)) {
                // ?gid (incl. gid 0, the first game on a fresh canister) is a
                // real credit; null is a non-deposit / unmatched tx that only
                // advanced the cursor. Counting any #Ok(?_) fixes the prior
                // `if (gid > 0)` off-by-one that dropped game 0 from the count.
                case (#Ok(?_gid)) { credits += 1 };
                case (#Ok(null)) {};
                case (#Err(e)) {
                    Debug.print("creditDeposit error for " # sig.signature # ": " # e);
                };
            };
        };
        credits;
    };

    /// Auto-detection pass: scan ONLY the deposit addresses that currently
    /// have an open, non-expired intent. This bounds RPC cost to active
    /// deposit flows — when there are no pending intents the tick makes zero
    /// outcalls. An open intent's address keeps being scanned every tick
    /// until the deposit is credited or the intent expires, giving free
    /// retry across transient RPC failures / not-yet-confirmed transactions.
    func runDetectionForOpenIntents() : async Nat {
        let now = Time.now();
        // Distinct deposit addresses with a live intent (dedups multiple
        // intents from the same principal).
        var toScan = textMap.empty<Principal>();
        for (intent in natMap.vals(pendingIntents)) {
            if (not intent.fulfilled and now <= intent.expiresAt) {
                switch (principalMapNat.get(depositAddresses, intent.principal)) {
                    case (?addr) { toScan := textMap.put(toScan, addr, intent.principal) };
                    case (null) {};
                };
            };
        };
        var credits : Nat = 0;
        for ((address, principal) in textMap.entries(toScan)) {
            credits += await scanAndCredit(address, principal);
        };
        credits;
    };

    /// Admin-callable manual detection sweep. Scans EVERY known deposit
    /// address (not just those with open intents) so the operator can use it
    /// for diagnostics / recovery. Returns the count of new GameRecords
    /// created (zero is normal when nothing arrived). Shares the
    /// detectionInProgress guard with the auto-detection timer so the two
    /// never run concurrently.
    public shared ({ caller }) func runDepositDetection() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };
        if (detectionInProgress) { return #Err("Detection already in progress") };
        detectionInProgress := true;
        try {
            var credits : Nat = 0;
            for ((address, principal) in textMap.entries(addressToPrincipal)) {
                credits += await scanAndCredit(address, principal);
            };
            #Ok(credits);
        } catch (e) {
            #Err("Detection failed: " # Error.message(e));
        } finally {
            detectionInProgress := false;
        };
    };

    /// Timer callback — runs the cheap open-intents pass. Guarded against
    /// overlapping runs and against running before bootstrap. Errors are
    /// caught and logged (never trapped) so a bad tick doesn't kill the
    /// recurring timer. The guard is released in `finally` so a trap in the
    /// post-await continuation can't wedge it permanently.
    func detectionTick() : async () {
        if (detectionInProgress) return;
        if (not bootstrapped) return;
        detectionInProgress := true;
        try {
            ignore await runDetectionForOpenIntents();
        } catch (e) {
            Debug.print("Detection tick error: " # Error.message(e));
        } finally {
            detectionInProgress := false;
        };
    };

    /// (Re)arm the recurring detection timer. Cancels any existing timer
    /// first, so it is idempotent. Requires <system> capability.
    func startDetectionTimer<system>() {
        switch (detectionTimerId) {
            case (?tid) { Timer.cancelTimer(tid) };
            case (null) {};
        };
        let tid = Timer.recurringTimer<system>(#seconds(DETECTION_INTERVAL_SECONDS), detectionTick);
        detectionTimerId := ?tid;
    };

    /// Re-arm the recurring detection timer on every canister upgrade. The IC
    /// clears all pending timers on upgrade, so detectionTimerId survives in
    /// stable state pointing at a dead timer. Without this, after an upgrade
    /// deposits silently stop auto-crediting. Only arms if already
    /// bootstrapped; a fresh install arms via bootstrap().
    system func postupgrade() {
        if (bootstrapped) { startDetectionTimer<system>() };
    };

    /// Admin: start (or restart) the auto-detection timer.
    public shared ({ caller }) func adminStartDetectionTimer() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };
        startDetectionTimer<system>();
        #Ok("Detection timer armed (" # Nat.toText(DETECTION_INTERVAL_SECONDS) # "s interval)");
    };

    /// Admin: stop the auto-detection timer. Manual runDepositDetection and
    /// adminCreditManualDeposit still work while it is stopped.
    public shared ({ caller }) func adminStopDetectionTimer() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        switch (detectionTimerId) {
            case (?tid) { Timer.cancelTimer(tid); detectionTimerId := null; #Ok("Detection timer stopped") };
            case (null) { #Ok("Detection timer was not running") };
        };
    };

    /// Status of the auto-detection timer and the open-intent backlog.
    public query func getDetectionStatus() : async {
        timerArmed : Bool;
        intervalSeconds : Nat;
        inProgress : Bool;
        openIntents : Nat;
    } {
        let now = Time.now();
        var openCount : Nat = 0;
        for (intent in natMap.vals(pendingIntents)) {
            if (not intent.fulfilled and now <= intent.expiresAt) { openCount += 1 };
        };
        {
            timerArmed = detectionTimerId != null;
            intervalSeconds = DETECTION_INTERVAL_SECONDS;
            inProgress = detectionInProgress;
            openIntents = openCount;
        };
    };

    /// Admin: refresh the cached nonce by reading account info on-chain.
    /// Use to recover from a nonce desync (e.g., broadcast succeeded but
    /// the local nonce-refresh read failed). Idempotent.
    public shared ({ caller }) func adminRefreshNonce() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account not initialized") };
            case (?n) { n };
        };
        Cycles.add<system>(RPC_CYCLES);
        let res = await solRpc.getAccountInfo(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                pubkey = nonceAddr;
                commitment = ?#confirmed;
                encoding = ?#base58;
                dataSlice = null;
                minContextSlot = null;
            },
        );
        switch (SolRpc.unwrapMultiAccountInfo(res)) {
            case (#Err(e)) { #Err("getAccountInfo: " # e) };
            case (#Ok(null)) { #Err("Nonce account not found on-chain") };
            case (#Ok(?account)) {
                switch (parseNonceFromAccountData(account.data)) {
                    case (?n) { lastNonceValue := ?n; #Ok(n) };
                    case (null) { #Err("Could not parse nonce from account data") };
                };
            };
        };
    };

    /// Admin: retry the sweep from a per-user deposit address to the pool.
    /// Use when creditDeposit credited the game but sweepToPool failed
    /// (Debug.print'd "Sweep failed for ..."). Looks up the principal,
    /// reads the current on-chain balance, and reissues the sweep tx.
    /// Safe to call repeatedly — if the balance is already at dust,
    /// returns an error rather than building a no-op tx.
    public shared ({ caller }) func adminSweepDepositAddress(address : Text) : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        let principal = switch (textMap.get(addressToPrincipal, address)) {
            case (null) { return #Err("Address not registered: " # address) };
            case (?p) { p };
        };
        Cycles.add<system>(RPC_CYCLES);
        let balRes = await solRpc.getBalance(
            SolRpc.rpcSources(solRpcProvider),
            null,
            { pubkey = address; commitment = ?#confirmed; minContextSlot = null },
        );
        let lamports = switch (SolRpc.unwrapMultiBalance(balRes)) {
            case (#Err(e)) { return #Err("getBalance: " # e) };
            case (#Ok(b)) { b };
        };
        if (lamports == 0) { return #Err("Address has zero balance") };
        await sweepToPool(address, derivationPathForPrincipal(principal), lamports);
    };

    /// Admin: mark the canister as bootstrapped from on-chain state.
    /// Use when the bootstrap tx broadcast succeeded on Solana but the
    /// IC-side bookkeeping didn't commit (e.g. decode trap on a stale
    /// sol-rpc binding). Reads the nonce account, parses the initial
    /// nonce, sets `lastNonceValue` and flips `bootstrapped := true`.
    /// Idempotent.
    public shared ({ caller }) func adminMarkBootstrapped() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        let nonceAddr = switch (nonceAccountAddress) {
            case (null) { return #Err("Nonce account address not derived — call adminDerivePoolAddress first") };
            case (?n) { n };
        };
        Cycles.add<system>(RPC_CYCLES);
        let res = await solRpc.getAccountInfo(
            SolRpc.rpcSources(solRpcProvider),
            null,
            {
                pubkey = nonceAddr;
                commitment = ?#confirmed;
                encoding = ?#base58;
                dataSlice = null;
                minContextSlot = null;
            },
        );
        switch (SolRpc.unwrapMultiAccountInfo(res)) {
            case (#Err(e)) { #Err("getAccountInfo: " # e) };
            case (#Ok(null)) { #Err("Nonce account not found on-chain") };
            case (#Ok(?account)) {
                switch (parseNonceFromAccountData(account.data)) {
                    case (?n) {
                        lastNonceValue := ?n;
                        bootstrapped := true;
                        #Ok("bootstrapped=true; nonce=" # n);
                    };
                    case (null) { #Err("Could not parse nonce from account data") };
                };
            };
        };
    };

    public shared ({ caller }) func payManagementSol() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (coverChargeAccrualLamports == 0) { return #Err("Nothing to sweep") };
        if (not Base58.isPlausibleSolanaAddress(solTreasuryAddress)) {
            return #Err("solTreasuryAddress is not a valid Solana address: " # solTreasuryAddress);
        };

        acquireGlobalLock();
        try {
            let amount = coverChargeAccrualLamports;
            let solFee : Nat64 = 5_000;
            if (amount <= solFee) {
                return #Err("Accumulated balance below transfer fee");
            };
            let payout : Nat64 = amount - solFee;

            // Zero internal accrual BEFORE the outbound call — same
            // pattern as sweepCoverCharges.
            coverChargeAccrualLamports := 0;

            switch (await sendSolPayout(solTreasuryAddress, payout)) {
                case (#Err(e)) {
                    coverChargeAccrualLamports := amount;
                    #Err("SOL payout failed: " # e);
                };
                case (#Ok(txSig)) {
                    recordLedger(#coverChargeSwept({
                        amountE8s = Nat64.toNat(amount); // Reusing the field as lamports.
                        toBackend = BACKEND_PRINCIPAL; // For audit only; the actual destination is solTreasuryAddress.
                        blockIndex = 0;                 // No block index — SOL tx; signature lives in the ledger as a separate note.
                    }));
                    #Ok(txSig);
                };
            };
        } finally {
            releaseGlobalLock();
        };
    };

    /// Admin: update solTreasuryAddress (the destination of payManagementSol).
    public shared ({ caller }) func adminSetSolTreasuryAddress(addr : Text) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        if (not Base58.isPlausibleSolanaAddress(addr)) {
            return #Err("Not a valid Solana address: " # addr);
        };
        solTreasuryAddress := addr;
        #Ok;
    };

    public query func getSolTreasuryAddress() : async Text { solTreasuryAddress };
    public query func getCoverChargeAccrualLamports() : async Nat64 { coverChargeAccrualLamports };

    /// Admin: record a Series A backer position for `owner` of `amount`
    /// SOL. Use ONCE at deploy to register the operator's pre-deposited
    /// pool seed. Mirrors ponzi_math.addBackerMoney's bookkeeping but
    /// skips the synchronous transfer-from (the SOL is already on the
    /// pool address, deposited out-of-band by the operator).
    public shared ({ caller }) func adminRegisterSeriesABacker(owner : Principal, amount : Float) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        validateAmount(amount);
        if (amount < 0.05) { return #Err("Minimum is 0.05 SOL") };

        acquireGlobalLock();
        try {
            let entitlement = amount * 1.24;
            switch (backerKeyMap.get(backerPositions, (owner, #seriesA))) {
                case (null) {
                    let pos : BackerPosition = {
                        owner;
                        amount;
                        entitlement;
                        startTime = Time.now();
                        isActive = true;
                        backerType = #seriesA;
                        firstDepositDate = ?Time.now();
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (owner, #seriesA), pos);
                };
                case (?existing) {
                    let updated : BackerPosition = {
                        existing with
                        amount = existing.amount + amount;
                        entitlement = existing.entitlement + entitlement;
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (owner, #seriesA), updated);
                };
            };
            platformStats := { platformStats with potBalance = platformStats.potBalance + amount };
            recordLedger(#backerDeposit({ backer = owner; amount; entitlement }));
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };

    /// Admin: manually credit an unmatched / TTL-expired SOL deposit.
    /// `lamports` is the gross detected amount; cover charge is
    /// computed at the standard 4% rate. Used to clear admin-review
    /// entries flagged by creditDeposit when no intent matched.
    public shared ({ caller }) func adminCreditManualDeposit(
        player : Principal,
        plan : GamePlan,
        lamports : Nat64,
    ) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (lamports < MIN_DEPOSIT_LAMPORTS) { return #Err("Below minimum deposit") };

        acquireGlobalLock();
        try {
            let coverChargeLamports = bpsApply(lamports, COVER_CHARGE_RATE_LAMPORTS_BPS);
            let netLamports = lamports - coverChargeLamports;
            let depositSol = lamportsToSol(lamports);
            let coverChargeSol = lamportsToSol(coverChargeLamports);
            let netSol = lamportsToSol(netLamports);

            let gameId = nextGameId;
            nextGameId += 1;
            coverChargeAccrualLamports += coverChargeLamports;

            let isCompounding = switch (plan) { case (#simple21Day) { false }; case (_) { true } };
            let game : GameRecord = {
                id = gameId;
                player;
                plan;
                amount = depositSol;
                startTime = Time.now();
                isCompounding;
                isActive = true;
                lastUpdateTime = Time.now();
                accumulatedEarnings = 0.0;
                totalWithdrawn = 0.0;
            };
            gameRecords := natMap.put(gameRecords, gameId, game);
            platformStats := {
                platformStats with
                totalDeposits = platformStats.totalDeposits + depositSol;
                activeGames = platformStats.activeGames + 1;
                potBalance = platformStats.potBalance + netSol;
            };
            recordLedger(#deposit({
                player;
                gameId;
                gross = depositSol;
                coverCharge = coverChargeSol;
                netToPot = netSol;
                plan;
                isCompounding;
            }));
            recordLedger(#backdatedGameCreated({
                admin = caller;
                player;
                gameId;
                startTime = Time.now();
                amount = depositSol;
            }));

            #Ok(gameId);
        } finally {
            releaseGlobalLock();
        };
    };

    // ====================================================================
    // Self-test queries (used by Task 21 to smoke-test pure modules
    // without needing devnet round-trips).
    // ====================================================================

    public query func selfTestBase58() : async Bool {
        // 32 zero bytes → "11111111111111111111111111111111" (System Program ID).
        let zeros = Blob.fromArray(Array.tabulate<Nat8>(32, func(_) { 0 }));
        let encoded = Base58.encode(zeros);
        encoded == SolTx.SYSTEM_PROGRAM_ID;
    };

    public query func selfTestSolTx() : async {
        compactU16_42 : [Nat8];
        compactU16_128 : [Nat8];
        compactU16_300 : [Nat8];
        u64Le_1 : [Nat8];
        u64Le_1B : [Nat8];
    } {
        {
            compactU16_42 = SolTx.compactU16(42);          // expect [42]
            compactU16_128 = SolTx.compactU16(128);        // expect [0x80, 0x01]
            compactU16_300 = SolTx.compactU16(300);        // expect [0xAC, 0x02]
            u64Le_1 = SolTx.u64Le(1);                       // expect [1, 0, 0, 0, 0, 0, 0, 0]
            u64Le_1B = SolTx.u64Le(1_000_000_000);          // expect [0,0xCA,0x9A,0x3B,0,0,0,0]
        };
    };
};
