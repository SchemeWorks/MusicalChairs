import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Time "mo:base/Time";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Text "mo:base/Text";
import List "mo:base/List";
import Iter "mo:base/Iter";
import Debug "mo:base/Debug";
import Blob "mo:base/Blob";
import Error "mo:base/Error";

import Ledger "ledger";
import Icrc21 "icrc21";
import Migration "migration";

(with migration = Migration.run)
persistent actor class PonziMath(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);

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
        event : GeneralLedgerEvent;
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

    func transferFromErrorMessage(err : Ledger.TransferFromError) : Text {
        switch (err) {
            case (#InsufficientFunds(_)) { "Insufficient ICP balance" };
            case (#InsufficientAllowance(_)) { "Please approve the transfer first" };
            case (#BadFee(_)) { "Bad fee" };
            case (#BadBurn(_)) { "Bad burn" };
            case (#TooOld) { "Transaction too old" };
            case (#CreatedInFuture(_)) { "Transaction created in future" };
            case (#Duplicate(_)) { "Duplicate transaction" };
            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
            case (#GenericError(e)) { "Error: " # e.message };
        };
    };

    func transferErrorMessage(err : Ledger.TransferError) : Text {
        switch (err) {
            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
            case (#BadFee(_)) { "Bad fee" };
            case (#BadBurn(_)) { "Bad burn" };
            case (#TooOld) { "Transaction too old" };
            case (#CreatedInFuture(_)) { "Transaction created in future" };
            case (#Duplicate(_)) { "Duplicate transaction" };
            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
            case (#GenericError(e)) { "Error: " # e.message };
        };
    };

    // ========================================================================
    // General Ledger event recording
    // ========================================================================

    func recordLedger(event : GeneralLedgerEvent) {
        let entry : GeneralLedgerEntry = {
            id = nextGeneralLedgerId;
            timestamp = Time.now();
            event;
        };
        generalLedger := natMap.put(generalLedger, nextGeneralLedgerId, entry);
        nextGeneralLedgerId += 1;
    };

    // ========================================================================
    // Exit toll calculation
    // Simple: 7% (< 3 days), 5% (3-10), 3% (> 10)
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
            if (elapsedDays < 3.0) { earnings * 0.07 }
            else if (elapsedDays < 10.0) { earnings * 0.05 }
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

    // 50% of the toll seeds the next round (routed to roundSeedReserve, OUT of
    // the pot). The other 50% credits backer repayment balances via 35/25/40.
    func distributeExitToll(tollAmount : Float) {
        let seedAmount = tollAmount * 0.5;
        let backerRepaymentAmount = tollAmount * 0.5;
        roundSeedReserve += seedAmount;

        let allBackers = Iter.toArray(backerKeyMap.vals(backerPositions));
        if (allBackers.size() == 0) {
            // No backers yet — backer half also flows to seed reserve (not pot).
            roundSeedReserve += backerRepaymentAmount;
            recordLedger(#tollDistribution({
                tollAmount;
                toSeedReserve = tollAmount;
                toOldestSeriesA = 0.0;
                toOtherSeriesA = 0.0;
                toAllBackers = 0.0;
            }));
            return;
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

        recordLedger(#tollDistribution({
            tollAmount;
            toSeedReserve = seedAmount;
            toOldestSeriesA = toOldest;
            toOtherSeriesA = toOthers;
            toAllBackers = toAll;
        }));
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

    // ========================================================================
    // createGame — opens a new investment position
    // ========================================================================

    transient let COVER_CHARGE_RATE : Float = 0.04;

    public shared ({ caller }) func createGame(
        plan : GamePlan,
        amount : Float,
        isCompounding : Bool,
    ) : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            let currentTime = Time.now();
            let currentHour = currentTime / 3600000000000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(
                        timestamps,
                        func(t) { currentHour - t < 1 },
                    );
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };

            if (not isCompounding) {
                let maxDeposit = Float.max(platformStats.potBalance * 0.2, 5.0);
                if (amount > maxDeposit) {
                    return #Err("Maximum deposit for simple mode is the greater of 20% of current pot balance or 5 ICP (" # formatICP(maxDeposit) # " ICP)");
                };
            };

            let selfPrincipal = Principal.fromActor(Self);
            let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

            let transferResult = try {
                await icpLedger.icrc2_transfer_from({
                    spender_subaccount = null;
                    from = { owner = caller; subaccount = null };
                    to = { owner = selfPrincipal; subaccount = null };
                    amount = amountE8s;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            switch (transferResult) {
                case (#Err(err)) { return #Err(transferFromErrorMessage(err)) };
                case (#Ok(_)) {};
            };

            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, List.nil()));
                };
                case (?timestamps) {
                    let filtered = List.filter<Int>(timestamps, func(t) { currentHour - t < 1 });
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, filtered));
                };
            };

            let coverCharge = amount * COVER_CHARGE_RATE;
            let coverChargeE8s = Int.abs(Float.toInt(coverCharge * 100_000_000.0));
            coverChargeBalance += coverChargeE8s;
            let netAmount = amount - coverCharge;

            let gameId = nextGameId;
            nextGameId += 1;

            if (coverChargeE8s > 0) {
                recordLedger(#coverChargeAccrued({
                    gameId;
                    player = caller;
                    amountE8s = coverChargeE8s;
                }));
            };

            let newGame : GameRecord = {
                id = gameId;
                player = caller;
                plan;
                amount;
                startTime = Time.now();
                isCompounding;
                isActive = true;
                lastUpdateTime = Time.now();
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

            recordLedger(#deposit({
                player = caller;
                gameId;
                gross = amount;
                coverCharge;
                netToPot = netAmount;
                plan;
                isCompounding;
            }));

            #Ok(gameId);
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

    // ========================================================================
    // addBackerMoney — funds a Series A backer position
    // ========================================================================

    public shared ({ caller }) func addBackerMoney(amount : Float) : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) { return #Err("Minimum deposit is 0.1 ICP") };
        if (not validateEightDecimals(amount)) {
            return #Err("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);
        acquireGlobalLock();
        try {
            let selfPrincipal = Principal.fromActor(Self);
            let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

            let transferResult = try {
                await icpLedger.icrc2_transfer_from({
                    spender_subaccount = null;
                    from = { owner = caller; subaccount = null };
                    to = { owner = selfPrincipal; subaccount = null };
                    amount = amountE8s;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            let blockIndex = switch (transferResult) {
                case (#Err(err)) { return #Err(transferFromErrorMessage(err)) };
                case (#Ok(idx)) { idx };
            };

            let entitlement = amount * 1.24; // Series A 24% bonus

            switch (backerKeyMap.get(backerPositions, (caller, #seriesA))) {
                case (null) {
                    let newBacker : BackerPosition = {
                        owner = caller;
                        amount;
                        entitlement;
                        startTime = Time.now();
                        isActive = true;
                        backerType = #seriesA;
                        firstDepositDate = ?Time.now();
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), newBacker);
                };
                case (?existing) {
                    let updated : BackerPosition = {
                        existing with
                        amount = existing.amount + amount;
                        entitlement = existing.entitlement + entitlement;
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (caller, #seriesA), updated);
                };
            };

            platformStats := {
                platformStats with
                potBalance = platformStats.potBalance + amount;
            };

            recordLedger(#backerDeposit({ backer = caller; amount; entitlement }));

            #Ok(blockIndex);
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
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
                        triggerGameReset("Insufficient funds for payout (pot empty)");
                        return #Err("Game reset: pot is empty");
                    };

                    let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                    let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                    let actualToll = exitToll * scaleFactor;
                    let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                    distributeExitToll(actualToll);

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

                    let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (netEarningsE8s > 0) {
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = netEarningsE8s;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

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
                        triggerGameReset("Pot drained (partial payout)");
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
                        triggerGameReset("Insufficient funds for compounding game settlement (pot empty)");
                        return #Err("Game reset: pot is empty");
                    };

                    let scaleFactor = if (isInsolvent) { pot / earnings } else { 1.0 };
                    let actualNetEarnings = roundToEightDecimals(netEarnings * scaleFactor);
                    let actualToll = exitToll * scaleFactor;
                    let actualPotDeduction = if (isInsolvent) { pot } else { earnings };

                    distributeExitToll(actualToll);

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

                    let payoutE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (payoutE8s > 0) {
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = payoutE8s;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

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
                        triggerGameReset("Pot drained (partial payout)");
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

            let balanceE8s = Int.abs(Float.toInt(roundToEightDecimals(balance) * 100_000_000.0));
            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = caller; subaccount = null };
                    amount = balanceE8s;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            switch (transferResult) {
                case (#Err(err)) {
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), aBalance);
                    backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), bBalance);
                    return #Err(transferErrorMessage(err));
                };
                case (#Ok(_)) {};
            };

            recordLedger(#backerRepaymentClaim({ backer = caller; amount = balance }));

            #Ok(balance);
        } finally {
            releaseGlobalLock();
            releaseCallerLock(caller);
        };
    };

    // ========================================================================
    // sweepCoverCharges — gated on backend canister principal.
    // Transfers full coverChargeBalance to BACKEND_PRINCIPAL.
    // ========================================================================

    public shared ({ caller }) func sweepCoverCharges() : async { #Ok : Nat; #Err : Text } {
        if (caller != BACKEND_PRINCIPAL) {
            return #Err("Unauthorized: only backend canister can sweep");
        };
        if (coverChargeBalance == 0) {
            return #Err("Nothing to sweep");
        };
        if (coverChargeBalance <= Ledger.ICP_TRANSFER_FEE) {
            return #Err("Accumulated balance below transfer fee");
        };

        acquireGlobalLock();
        try {
            let amount = coverChargeBalance;
            let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;

            coverChargeBalance := 0;

            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = BACKEND_PRINCIPAL; subaccount = null };
                    amount = transferAmount;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                coverChargeBalance := amount;
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            switch (transferResult) {
                case (#Err(err)) {
                    coverChargeBalance := amount;
                    return #Err(transferErrorMessage(err));
                };
                case (#Ok(blockIndex)) {
                    recordLedger(#coverChargeSwept({
                        amountE8s = amount;
                        toBackend = BACKEND_PRINCIPAL;
                        blockIndex;
                    }));
                    #Ok(blockIndex);
                };
            };
        } finally {
            releaseGlobalLock();
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
        let currentHour = Time.now() / 3600000000000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { currentHour - t < 1 });
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

    public shared func getCanisterICPBalance() : async Nat {
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
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
            let selfPrincipal = Principal.fromActor(Self);
            let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

            let transferResult = try {
                await icpLedger.icrc2_transfer_from({
                    spender_subaccount = null;
                    from = { owner = caller; subaccount = null };
                    to = { owner = selfPrincipal; subaccount = null };
                    amount = amountE8s;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            switch (transferResult) {
                case (#Err(err)) { return #Err(transferFromErrorMessage(err)) };
                case (#Ok(_)) {};
            };

            let coverCharge = amount * COVER_CHARGE_RATE;
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
};
