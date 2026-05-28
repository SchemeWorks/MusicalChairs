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

import Icrc21 "icrc21";

import Base58 "Base58";
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

    // Intent TTL — 10 minutes.
    transient let INTENT_TTL_NS : Int = 10 * 60 * 1_000_000_000;

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

    func validateEightDecimals(value : Float) : Bool {
        let multiplier = 100000000.0;
        let rounded = Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
        rounded == value;
    };

    func formatICP(value : Float) : Text {
        let intValue = Float.toInt(value);
        if (Float.fromInt(intValue) == value) {
            Int.toText(intValue);
        } else {
            let textValue = Float.toText(value);
            let parts = Iter.toArray(Text.split(textValue, #char '.'));
            switch (parts.size()) {
                case (1) { parts[0] };
                case (2) {
                    let trimmed = Text.trimEnd(parts[1], #char '0');
                    if (trimmed == "") { parts[0] } else { parts[0] # "." # trimmed };
                };
                case (_) { textValue };
            };
        };
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

    /// Fetch a recent blockhash, retrying on consensus failures. Used
    /// ONLY by bootstrap — all other outbound txs use the durable nonce.
    func fetchRecentBlockhashWithRetry(attempts : Nat) : async ?Text {
        var i : Nat = 0;
        while (i < attempts) {
            let res = await solRpc.getLatestBlockhash(?{ provider = ?solRpcProvider });
            switch (res) {
                case (#Ok({ blockhash; lastValidBlockHeight = _ })) { return ?blockhash };
                case (#Err(_)) { i += 1 };
            };
        };
        null;
    };

    /// Parse 32 bytes of nonce account body as a base58 blockhash.
    /// Solana nonce-account layout (System program account state):
    ///   bytes 0..4 — version (u32 LE)
    ///   bytes 4..8 — state (u32 LE; 1 = Initialized)
    ///   bytes 8..40 — authority pubkey (32 bytes)
    ///   bytes 40..72 — nonce value (32 bytes — what we want)
    ///   bytes 72..80 — fee_calculator.lamports_per_signature (u64 LE)
    func parseNonceFromAccountData(data : Blob) : ?Text {
        let arr = Blob.toArray(data);
        if (arr.size() < 72) { return null };
        let nonceBytes = Array.tabulate<Nat8>(32, func(i) { arr[40 + i] });
        ?Base58.encode(Blob.fromArray(nonceBytes));
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

        let sendRes = await solRpc.sendTransaction(
            txBytes,
            ?{
                skipPreflight = ?false;
                preflightCommitment = ?"confirmed";
                maxRetries = ?(3 : Nat64);
                encoding = ?"base64";
            },
            ?{ provider = ?solRpcProvider },
        );
        switch (sendRes) {
            case (#Err(e)) { #Err("sendTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(txSig)) {
                // Refresh nonce.
                let acctRes = await solRpc.getAccountInfo(
                    nonceAddr,
                    ?{ commitment = ?"confirmed"; encoding = ?"base64" },
                    ?{ provider = ?solRpcProvider },
                );
                switch (acctRes) {
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

    public shared ({ caller }) func withdrawEarnings(gameId : Nat) : async { #Ok : Float; #Err : Text } {
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
                        let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                            case (?addr) { addr };
                            case (null) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("Caller has no deposit address; cannot pay out. Call getOrCreateDepositAddress first.");
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

    public shared ({ caller }) func settleCompoundingGame(gameId : Nat) : async { #Ok : Float; #Err : Text } {
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
                        let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                            case (?addr) { addr };
                            case (null) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err("Caller has no deposit address; cannot pay out. Call getOrCreateDepositAddress first.");
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

    public shared ({ caller }) func claimBackerRepayment() : async { #Ok : Float; #Err : Text } {
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
            let destination = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?addr) { addr };
                case (null) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err("Caller has no deposit address; call getOrCreateDepositAddress first.");
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
                let res = await solRpc.getBalance(addr, ?{ provider = ?solRpcProvider });
                switch (res) {
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

    // createBackdatedGame — same flow as createGame but with a caller-specified
    // startTime, enabling tests of matured-position payouts.
    public shared ({ caller }) func createBackdatedGame(
        plan : GamePlan,
        amount : Float,
        isCompounding : Bool,
        startTimeNanos : Int,
    ) : async { #Ok : Nat; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };
        if (startTimeNanos > Time.now()) {
            return #Err("startTime must not be in the future");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            // ICP deposit was here. SOL-side deposit not yet wired in Task 8 — see Task 20.
            Debug.trap("test admin SOL payout: not yet wired in Task 8 — see Task 20");

            let coverCharge = amount * 0.04; // COVER_CHARGE_RATE — re-declared inline after deletion of createGame block
            let coverChargeE8s = Int.abs(Float.toInt(coverCharge * 100_000_000.0));
            coverChargeBalance += coverChargeE8s;
            let netAmount = amount - coverCharge;

            let gameId = nextGameId;
            nextGameId += 1;

            if (coverChargeE8s > 0) {
                recordLedger(#coverChargeAccrued({ gameId; player = caller; amountE8s = coverChargeE8s }));
            };

            let newGame : GameRecord = {
                id = gameId;
                player = caller;
                plan;
                amount;
                startTime = startTimeNanos;
                isCompounding;
                isActive = true;
                lastUpdateTime = startTimeNanos;
                accumulatedEarnings = 0.0;
                totalWithdrawn = 0.0;
            };
            gameRecords := natMap.put(gameRecords, gameId, newGame);
            platformStats := {
                platformStats with
                totalDeposits = platformStats.totalDeposits + amount;
                activeGames = platformStats.activeGames + 1;
                potBalance = platformStats.potBalance + netAmount;
            };

            recordLedger(#backdatedGameCreated({
                admin = caller;
                player = caller;
                gameId;
                startTime = startTimeNanos;
                amount;
            }));

            #Ok(gameId);
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

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

    // adminSweepUntracked — recover any actual ICP balance that exceeds
    // internal accounting (potBalance + roundSeedReserve + sum(backerRepayments)
    // + coverChargeBalance). Sends `actual - internal - fee` to the testAdmin.
    // Used to reconcile dust left over by operations like
    // adminClearAllBackerPositions, which zero internal accounting fields
    // without crediting the corresponding ICP elsewhere. No-op if there is no
    // positive untracked balance.
    public shared ({ caller }) func adminSweepUntracked() : async { #Ok : Nat; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            // ICP balance read + sweep was here. SOL-side not yet wired in Task 8 — see Task 20.
            Debug.trap("test admin SOL payout: not yet wired in Task 8 — see Task 20");
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

    public shared ({ caller }) func bootstrap() : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (bootstrapped) { return #Ok("already-bootstrapped") };

        acquireGlobalLock();
        try {
            // 1. Derive pool + nonce addresses.
            let pool = await ensurePoolAddress();
            let nonce = await ensureNonceAccountAddress();

            // 2. Confirm pool funded (~0.003 SOL = 3M lamports minimum).
            let balanceRes = await solRpc.getBalance(pool, ?{ provider = ?solRpcProvider });
            let balance = switch (balanceRes) {
                case (#Ok(b)) { b };
                case (#Err(e)) {
                    return #Err("getBalance(pool) failed: " # rpcErrorText(e));
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

            // 4. Fetch a recent blockhash (with retry).
            let blockhash = switch (await fetchRecentBlockhashWithRetry(5)) {
                case (?h) { h };
                case (null) { return #Err("getLatestBlockhash failed after 5 retries") };
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
            let sendRes = await solRpc.sendTransaction(
                txBytes,
                ?{
                    skipPreflight = ?false;
                    preflightCommitment = ?"confirmed";
                    maxRetries = ?(3 : Nat64);
                    encoding = ?"base64";
                },
                ?{ provider = ?solRpcProvider },
            );
            let txSig = switch (sendRes) {
                case (#Ok(s)) { s };
                case (#Err(e)) { return #Err("sendTransaction failed: " # rpcErrorText(e)) };
            };

            // 8. Fetch nonce account state to read the initial nonce value.
            //    Try a few times — confirmation may lag the send.
            var attempts : Nat = 0;
            var initialNonce : ?Text = null;
            while (attempts < 10 and initialNonce == null) {
                let acctRes = await solRpc.getAccountInfo(nonce, ?{ commitment = ?"confirmed"; encoding = ?"base64" }, ?{ provider = ?solRpcProvider });
                switch (acctRes) {
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

    /// Convenience: render an RpcError for #Err returns.
    func rpcErrorText(e : SolRpc.RpcError) : Text {
        switch (e) {
            case (#ProviderError(m)) { "ProviderError: " # m };
            case (#HttpOutcallError(m)) { "HttpOutcallError: " # m };
            case (#JsonRpcError({ code; message })) { "JsonRpcError(" # Int.toText(code) # "): " # message };
            case (#ConsensusError(m)) { "ConsensusError: " # m };
            case (#ValidationError(m)) { "ValidationError: " # m };
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
        let res = await solRpc.getSignaturesForAddress(
            address,
            ?{
                limit = ?100;
                before = null;
                until = cursor;
                commitment = ?"confirmed";
            },
            ?{ provider = ?solRpcProvider },
        );
        switch (res) {
            case (#Err(_)) { [] };
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

        // Sweep leaves ~5_000 lamports dust on the per-user address for
        // the network fee. If the detected amount is at or below that
        // floor, refuse rather than building a zero/negative transfer.
        if (lamports <= 5_000) {
            return #Err("Detected amount below network-fee floor (≤5000 lamports)");
        };
        let sweepLamports : Nat64 = lamports - 5_000;

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

        let sendRes = await solRpc.sendTransaction(
            txBytes,
            ?{
                skipPreflight = ?false;
                preflightCommitment = ?"confirmed";
                maxRetries = ?(3 : Nat64);
                encoding = ?"base64";
            },
            ?{ provider = ?solRpcProvider },
        );
        switch (sendRes) {
            case (#Err(e)) { #Err("sendTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(txSig)) {
                // Refresh the nonce cache after a successful broadcast so
                // future txs use the advanced value.
                let acctRes = await solRpc.getAccountInfo(
                    nonceAddr,
                    ?{ commitment = ?"confirmed"; encoding = ?"base64" },
                    ?{ provider = ?solRpcProvider },
                );
                switch (acctRes) {
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
    func creditDeposit(sig : DetectedSignature) : async { #Ok : Nat; #Err : Text } {
        // 1. Fetch transaction details.
        let txRes = await solRpc.getTransaction(
            sig.signature,
            ?{
                commitment = ?"confirmed";
                maxSupportedTransactionVersion = ?(0 : Nat64);
                encoding = ?"json";
            },
            ?{ provider = ?solRpcProvider },
        );
        let tx = switch (txRes) {
            case (#Err(e)) { return #Err("getTransaction failed: " # rpcErrorText(e)) };
            case (#Ok(null)) { return #Err("Transaction not found / not confirmed yet") };
            case (#Ok(?t)) { t };
        };

        let meta = switch (tx.meta) {
            case (null) { return #Err("Transaction meta missing") };
            case (?m) { m };
        };
        if (meta.err != null) { return #Err("Transaction failed on-chain") };

        let message = switch (tx.transaction) {
            case (null) { return #Err("Transaction body missing") };
            case (?b) {
                switch (b.message) {
                    case (null) { return #Err("Message missing from transaction body") };
                    case (?m) { m };
                };
            };
        };

        // 2. Locate the deposit address inside accountKeys and compute the
        //    inbound delta.
        var addrIdx : ?Nat = null;
        var i : Nat = 0;
        while (i < message.accountKeys.size()) {
            if (message.accountKeys[i] == sig.address) { addrIdx := ?i };
            i += 1;
        };
        let idx = switch (addrIdx) {
            case (null) {
                return #Err("Deposit address not in transaction account keys (filter bug or false-positive)");
            };
            case (?n) { n };
        };
        if (idx >= meta.preBalances.size() or idx >= meta.postBalances.size()) {
            return #Err("Pre/post balances missing for deposit address");
        };
        let preBal = meta.preBalances[idx];
        let postBal = meta.postBalances[idx];
        if (postBal <= preBal) {
            // Outbound tx (probably a prior sweep we initiated). Advance
            // the cursor without crediting.
            lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
            return #Ok(0);
        };
        let inboundLamports : Nat64 = postBal - preBal;

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
                // Unmatched deposit — log for admin review. DO NOT advance
                // the cursor; operator resolves via adminCreditManualDeposit.
                Debug.print("Unmatched deposit on " # sig.address # ": " # Nat64.toText(inboundLamports) # " lamports sig=" # sig.signature);
                return #Ok(0);
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

        #Ok(gameId);
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

    /// Admin-callable detection sweep. Iterates every known deposit
    /// address, fetches new signatures past the per-address cursor,
    /// credits matching intents, sweeps to pool. Returns the count of
    /// new GameRecords created (zero is normal when nothing arrived).
    public shared ({ caller }) func runDepositDetection() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };

        var credits : Nat = 0;
        for ((address, principal) in textMap.entries(addressToPrincipal)) {
            let sigs = await scanAddress(address, principal);
            for (sig in sigs.vals()) {
                switch (await creditDeposit(sig)) {
                    case (#Ok(gid)) { if (gid > 0) { credits += 1 } };
                    case (#Err(e)) {
                        Debug.print("creditDeposit error for " # sig.signature # ": " # e);
                    };
                };
            };
        };
        #Ok(credits);
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
        let res = await solRpc.getAccountInfo(
            nonceAddr,
            ?{ commitment = ?"confirmed"; encoding = ?"base64" },
            ?{ provider = ?solRpcProvider },
        );
        switch (res) {
            case (#Err(e)) { #Err("getAccountInfo: " # rpcErrorText(e)) };
            case (#Ok(null)) { #Err("Nonce account not found on-chain") };
            case (#Ok(?account)) {
                switch (parseNonceFromAccountData(account.data)) {
                    case (?n) { lastNonceValue := ?n; #Ok(n) };
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

    public shared ({ caller }) func adminRegisterSeriesABacker(owner : Principal, amount : Float) : async { #Ok; #Err : Text } {
        let _ = caller;
        let _ = owner;
        let _ = amount;
        Debug.trap("adminRegisterSeriesABacker: not yet implemented (Task 20)");
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
