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
import Option "mo:base/Option";

import AccessControl "authorization/access-control";
import Ledger "ledger";
import Icrc21 "icrc21";

persistent actor {
    // Access Control State
    let accessControlState = AccessControl.initState();

    // ========================================================================
    // Security Guards (functions — collections declared after map initializers)
    // ========================================================================

    // Reject anonymous principals on authenticated endpoints
    func requireAuthenticated(caller : Principal) {
        if (Principal.isAnonymous(caller)) {
            Debug.trap("Anonymous principal not allowed");
        };
    };

    // Input validation for Float amounts (reject NaN, Infinity, negative)
    func validateAmount(amount : Float) {
        if (Float.isNaN(amount)) { Debug.trap("Amount cannot be NaN") };
        // Check for Infinity: NaN != NaN is true, but Inf == Inf is true and Inf - Inf is NaN
        if (Float.isNaN(amount - amount) and not Float.isNaN(amount)) {
            Debug.trap("Amount must be finite");
        };
        if (amount < 0.0) { Debug.trap("Amount cannot be negative") };
    };

    // Text length limit
    func validateTextLength(text : Text, maxLen : Nat, fieldName : Text) {
        if (Text.size(text) > maxLen) {
            Debug.trap(fieldName # " exceeds maximum length of " # Nat.toText(maxLen) # " characters");
        };
    };

    // Initialize Access Control (first caller becomes admin; cannot be re-initialized)
    public shared ({ caller }) func initializeAccessControl() : async () {
        requireAuthenticated(caller);
        AccessControl.initialize(accessControlState, caller);
    };

    // Get Caller User Role
    public query ({ caller }) func getCallerUserRole() : async AccessControl.UserRole {
        AccessControl.getUserRole(accessControlState, caller);
    };

    // Assign Caller User Role
    public shared ({ caller }) func assignCallerUserRole(user : Principal, role : AccessControl.UserRole) : async () {
        AccessControl.assignRole(accessControlState, caller, user, role);
    };

    // Check if Caller is Admin
    public query ({ caller }) func isCallerAdmin() : async Bool {
        AccessControl.isAdmin(accessControlState, caller);
    };

    // User Profile
    public type UserProfile = {
        name : Text;
    };

    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);
    var userProfiles = principalMap.empty<UserProfile>();

    public query ({ caller }) func getCallerUserProfile() : async ?UserProfile {
        principalMap.get(userProfiles, caller);
    };

    public query func getUserProfile(user : Principal) : async ?UserProfile {
        principalMap.get(userProfiles, user);
    };

    public shared ({ caller }) func saveCallerUserProfile(profile : UserProfile) : async () {
        requireAuthenticated(caller);
        validateTextLength(profile.name, 64, "Display name");
        userProfiles := principalMap.put(userProfiles, caller, profile);
    };

    // Game Plan Types
    public type GamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    // Game Record
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

    // Referral Earnings (PP earned through referrals, tracked per level for display)
    public type ReferralEarnings = {
        level1Points : Float;
        level2Points : Float;
        level3Points : Float;
    };

    // Platform Stats
    public type PlatformStats = {
        totalDeposits : Float;
        totalWithdrawals : Float;
        activeGames : Nat;
        potBalance : Float;
        daysActive : Nat;
    };

    // Game Reset Record
    public type GameResetRecord = {
        resetTime : Int;
        reason : Text;
    };

    // Dealer Types
    public type DealerType = {
        #upstream;
        #downstream;
    };

    // Dealer Position Record
    public type DealerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        name : Text;
        dealerType : DealerType;
        firstDepositDate : ?Int;
    };

    // Initialize OrderedMaps
    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMapNat = OrderedMap.Make<Principal>(Principal.compare);
    transient let intMap = OrderedMap.Make<Int>(Int.compare);

    var gameRecords = natMap.empty<GameRecord>();
    // DEPRECATED: kept for stable variable compatibility (old ICP-based referral system, now PP-only)
    var referralRecords = principalMapNat.empty<{
        referrer : Principal;
        level : Nat;
        earnings : Float;
        depositCount : Nat;
    }>();
    // Maps user → who referred them (one-time, immutable chain)
    var referralChain = principalMapNat.empty<Principal>();
    // Tracks PP earned through referrals, per referrer principal
    var referralEarnings = principalMapNat.empty<ReferralEarnings>();
    var platformStats : PlatformStats = {
        totalDeposits = 0.0;
        totalWithdrawals = 0.0;
        activeGames = 0;
        potBalance = 0.0;
        daysActive = 0;
    };
    var gameResetHistory = intMap.empty<GameResetRecord>();
    var nextGameId = 0;

    // Deposit Rate Limiting
    var depositTimestamps = principalMapNat.empty<List.List<Int>>();

    // Dealer Repayment Tracking
    var dealerRepayments = principalMapNat.empty<Float>();

    // Dealer Positions
    var dealerPositions = principalMapNat.empty<DealerPosition>();

    // Ponzi Points Tracking
    var ponziPoints = principalMapNat.empty<Float>();

    // Ponzi Points Burned Tracking
    var ponziPointsBurned = principalMapNat.empty<Float>();

    // Per-caller reentrancy lock to prevent TOCTOU exploits (transient — resets on upgrade, which is safe)
    transient var callerLocks = principalMapNat.empty<Bool>();

    func acquireCallerLock(caller : Principal) {
        switch (principalMapNat.get(callerLocks, caller)) {
            case (?true) { Debug.trap("Another operation is already in progress for this caller") };
            case _ {
                callerLocks := principalMapNat.put(callerLocks, caller, true);
            };
        };
    };

    func releaseCallerLock(caller : Principal) {
        callerLocks := principalMapNat.delete(callerLocks, caller);
    };

    // ========================================================================
    // Musical Chairs Wallet System (Real ICP Integration)
    // ========================================================================
    
    // User wallet balances (in e8s - 1 ICP = 100_000_000 e8s)
    var walletBalances = principalMapNat.empty<Nat>();
    
    // Wallet transaction history
    public type WalletTransaction = {
        id : Nat;
        user : Principal;
        txType : { #deposit; #withdrawal; #gameDeposit; #gameWithdrawal; #transfer };
        amount : Nat;  // in e8s
        timestamp : Int;
        ledgerBlockIndex : ?Nat;  // Block index on ICP ledger (if applicable)
        description : Text;
    };
    var walletTransactions = natMap.empty<WalletTransaction>();
    var nextWalletTxId = 0;

    // ========================================================================
    // Cover Charge — 2% skimmed from every deposit, routed to a dedicated
    // admin bucket ("Pay Management"). Separate from walletBalances and
    // walletTransactions so it doesn't meddle with player-visible accounting.
    // Exit tolls continue to use the 50/50 pot/backer split in
    // distributeExitTollToBackers — this change only touches the entry fee.
    // ========================================================================

    // Hardcoded admin principal — the only caller allowed to query or
    // withdraw from coverChargeBalance.
    transient let COVER_CHARGE_RECIPIENT : Principal =
        Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe");
    transient let COVER_CHARGE_RATE : Float = 0.02;

    // Dedicated bucket — never mingled with walletBalances.
    var coverChargeBalance : Nat = 0;

    // Separate audit log — never mingled with walletTransactions.
    public type CoverChargeEntry = {
        id : Nat;
        gameId : Nat;
        player : Principal;
        amount : Nat;  // in e8s
        timestamp : Int;
    };
    var coverChargeTransactions = natMap.empty<CoverChargeEntry>();
    var nextCoverChargeTxId : Nat = 0;

    // Test mode flag - when true, gives users 500 fake ICP for testing
    var testMode : Bool = false;
    
    // This canister's ID (set during init or known from dfx.json)
    // For local: uxrrr-q7777-77774-qaaaq-cai
    // This will be updated when deployed to mainnet
    stable var canisterPrincipal : ?Principal = null;
    
    // ICP Ledger actor reference (mainnet)
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
    
    // Set this canister's principal (called once by admin after deployment)
    public shared ({ caller }) func setCanisterPrincipal(p : Principal) : async () {
        requireAuthenticated(caller);
        // Only allow setting if not already set, or by admin
        switch (canisterPrincipal) {
            case (null) { canisterPrincipal := ?p };
            case (?_) {
                if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
                    Debug.trap("Canister principal already set. Only admin can change it.");
                };
                canisterPrincipal := ?p;
            };
        };
    };
    
    // Get this canister's principal
    public query func getCanisterPrincipal() : async ?Principal {
        canisterPrincipal;
    };

    // House Ledger Record
    public type HouseLedgerRecord = {
        id : Nat;
        amount : Float;
        timestamp : Int;
        description : Text;
    };

    var houseLedger = natMap.empty<HouseLedgerRecord>();
    var nextHouseLedgerId = 0;

    // Authorized shenanigans canister principal
    var shenanigansPrincipal : ?Principal = null;

    // ========================================================================
    // Musical Chairs Wallet API
    // ========================================================================

    // Record a wallet transaction
    func recordWalletTransaction(
        user : Principal,
        txType : { #deposit; #withdrawal; #gameDeposit; #gameWithdrawal; #transfer },
        amount : Nat,
        ledgerBlockIndex : ?Nat,
        description : Text
    ) {
        let tx : WalletTransaction = {
            id = nextWalletTxId;
            user;
            txType;
            amount;
            timestamp = Time.now();
            ledgerBlockIndex;
            description;
        };
        walletTransactions := natMap.put(walletTransactions, nextWalletTxId, tx);
        nextWalletTxId += 1;
    };

    // Get wallet transaction history for caller
    public query ({ caller }) func getWalletTransactions() : async [WalletTransaction] {
        let allTxs = Iter.toArray(natMap.vals(walletTransactions));
        let userTxs = List.filter(
            List.fromArray(allTxs),
            func(tx : WalletTransaction) : Bool { tx.user == caller }
        );
        List.toArray(userTxs);
    };

    // Record a cover charge entry (separate audit log — see the Cover Charge
    // state block above).
    func recordCoverChargeTransaction(gameId : Nat, player : Principal, amount : Nat) {
        let entry : CoverChargeEntry = {
            id = nextCoverChargeTxId;
            gameId;
            player;
            amount;
            timestamp = Time.now();
        };
        coverChargeTransactions := natMap.put(coverChargeTransactions, nextCoverChargeTxId, entry);
        nextCoverChargeTxId += 1;
    };

    // ========================================================================
    // Cover Charge — admin-only queries and withdrawal ("Pay Management")
    // ========================================================================

    // Current accumulated cover-charge balance (e8s). Admin only.
    public query ({ caller }) func getCoverChargeBalance() : async Nat {
        if (caller != COVER_CHARGE_RECIPIENT) {
            Debug.trap("Unauthorized");
        };
        coverChargeBalance;
    };

    // Full cover-charge audit log. Admin only.
    public query ({ caller }) func getCoverChargeTransactions() : async [CoverChargeEntry] {
        if (caller != COVER_CHARGE_RECIPIENT) {
            Debug.trap("Unauthorized");
        };
        Iter.toArray(natMap.vals(coverChargeTransactions));
    };

    // Pay Management — withdraw accumulated cover charges to the admin's
    // external ICP wallet. Admin only. Follows the saga pattern:
    // deduct first, refund on ledger failure. No minimum.
    //
    // `amount` is the number of e8s to deduct from the cover-charge bucket.
    // The admin receives (amount - ICP_TRANSFER_FEE) e8s in their wallet;
    // the fee is absorbed by the bucket itself. Callers must pass an amount
    // greater than the transfer fee or the call rejects.
    public shared ({ caller }) func withdrawCoverCharges(amount : Nat) : async { #Ok : Nat; #Err : Text } {
        if (caller != COVER_CHARGE_RECIPIENT) {
            return #Err("Unauthorized");
        };
        if (amount == 0) {
            return #Err("Amount must be greater than zero");
        };
        if (amount <= Ledger.ICP_TRANSFER_FEE) {
            return #Err("Amount must exceed the ledger transfer fee of " # Nat.toText(Ledger.ICP_TRANSFER_FEE) # " e8s");
        };
        if (amount > coverChargeBalance) {
            return #Err("Insufficient cover-charge balance. Have " # Nat.toText(coverChargeBalance) # " e8s, requested " # Nat.toText(amount) # " e8s");
        };

        acquireCallerLock(caller);

        // Deduct from bucket BEFORE the transfer (saga pattern).
        coverChargeBalance -= amount;

        try {
            let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;
            let transferResult = await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = caller; subaccount = null };
                amount = transferAmount;
                fee = null;
                memo = null;
                created_at_time = null;
            });

            let result : { #Ok : Nat; #Err : Text } = switch (transferResult) {
                case (#Ok(blockIndex)) { #Ok(blockIndex) };
                case (#Err(err)) {
                    // Compensate: refund the bucket since the transfer failed.
                    coverChargeBalance += amount;
                    let errMsg = switch (err) {
                        case (#BadFee(_)) { "Bad fee" };
                        case (#BadBurn(_)) { "Bad burn" };
                        case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                        case (#TooOld) { "Transaction too old" };
                        case (#CreatedInFuture(_)) { "Transaction created in future" };
                        case (#Duplicate(_)) { "Duplicate transaction" };
                        case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                        case (#GenericError(e)) { "Error: " # e.message };
                    };
                    #Err(errMsg);
                };
            };
            releaseCallerLock(caller);
            result;
        } catch (e) {
            // Compensate on catch (network failure — transfer status unknown;
            // conservative refund — the transfer may have actually succeeded).
            coverChargeBalance += amount;
            releaseCallerLock(caller);
            #Err("Failed to contact ICP ledger: " # Error.message(e));
        };
    };

    // ========================================================================
    // Admin Functions
    // ========================================================================
    
    // Toggle test mode (admin only)
    public shared ({ caller }) func setTestMode(enabled : Bool) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can toggle test mode");
        };
        testMode := enabled;
    };

    // Check if test mode is enabled
    public query func isTestMode() : async Bool {
        testMode;
    };

    // Get canister's ICP balance (admin only)
    public shared ({ caller }) func getCanisterICPBalance() : async Nat {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized");
        };
        
        let selfPrincipal = switch (canisterPrincipal) {
            case (null) { return 0 };
            case (?p) { p };
        };
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) {
            0;
        };
    };

    // Admin: Seed a game record with a custom start time (for recovery/testing)
    // Also registers referral chain and awards PP (matching createGame behavior)
    public shared ({ caller }) func seedGame(player : Principal, plan : GamePlan, amount : Float, isCompounding : Bool, startTimeNanos : Int) : async Nat {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can seed games");
        };

        let gameId = nextGameId;
        nextGameId += 1;

        let newGame : GameRecord = {
            id = gameId;
            player;
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
            potBalance = platformStats.potBalance + amount;
        };

        // Award Ponzi Points (same as createGame)
        let points = switch (plan) {
            case (#simple21Day) { amount * 1000.0 };
            case (#compounding15Day) { amount * 2000.0 };
            case (#compounding30Day) { amount * 3000.0 };
        };
        awardPonziPoints(player, points);

        gameId;
    };

    // Admin: Register a referral relationship (for testing/recovery)
    public shared ({ caller }) func seedReferral(user : Principal, referrer : Principal) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can seed referrals");
        };
        registerReferral(user, referrer);
    };

    // Add House Money
    public shared ({ caller }) func addHouseMoney(amount : Float, description : Text) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can add house money");
        };

        if (amount <= 0.0) {
            Debug.trap("Amount must be greater than 0");
        };

        let record : HouseLedgerRecord = {
            id = nextHouseLedgerId;
            amount;
            timestamp = Time.now();
            description;
        };

        houseLedger := natMap.put(houseLedger, nextHouseLedgerId, record);
        nextHouseLedgerId += 1;

        // Update platform stats
        platformStats := {
            platformStats with
            potBalance = platformStats.potBalance + amount;
        };

        // Update dealer entitlement for the admin (Series A: 24% bonus)
        let entitlement = amount * 1.24;

        // Get the admin's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Anonymous Backer" };
            case (?profile) { profile.name };
        };

        switch (principalMapNat.get(dealerPositions, caller)) {
            case (null) {
                let newDealer : DealerPosition = {
                    owner = caller;
                    amount;
                    entitlement;
                    startTime = Time.now();
                    isActive = true;
                    name;
                    dealerType = #upstream;
                    firstDepositDate = ?Time.now();
                };
                dealerPositions := principalMapNat.put(dealerPositions, caller, newDealer);
            };
            case (?existingDealer) {
                let updatedDealer : DealerPosition = {
                    existingDealer with
                    amount = existingDealer.amount + amount;
                    entitlement = existingDealer.entitlement + entitlement;
                    name;
                };
                dealerPositions := principalMapNat.put(dealerPositions, caller, updatedDealer);
            };
        };
    };

    // Get House Ledger
    public query func getHouseLedger() : async [HouseLedgerRecord] {
        Iter.toArray(natMap.vals(houseLedger));
    };

    // Get House Ledger Stats
    public query func getHouseLedgerStats() : async {
        totalDeposits : Float;
        totalWithdrawals : Float;
        netBalance : Float;
        recordCount : Nat;
    } {
        var totalDeposits = 0.0;
        var totalWithdrawals = 0.0;
        var recordCount = 0;

        for (record in natMap.vals(houseLedger)) {
            if (record.amount > 0.0) {
                totalDeposits += record.amount;
            } else {
                totalWithdrawals += -record.amount;
            };
            recordCount += 1;
        };

        {
            totalDeposits;
            totalWithdrawals;
            netBalance = totalDeposits - totalWithdrawals;
            recordCount;
        };
    };

    // Create New Game
    public shared ({ caller }) func createGame(plan : GamePlan, amount : Float, isCompounding : Bool, referrer : ?Principal) : async Nat {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

        // Validate 8 decimal places
        if (not validateEightDecimals(amount)) {
            Debug.trap("Amount cannot have more than 8 decimal places");
        };

        acquireCallerLock(caller);

        // Check deposit rate limit BEFORE the transfer (prevents TOCTOU on rate limit)
        let currentTime = Time.now();
        let currentHour = currentTime / 3600000000000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) {};
            case (?timestamps) {
                let filteredTimestamps = List.filter<Int>(
                    timestamps,
                    func(timestamp) { currentHour - timestamp < 1 },
                );
                if (List.size(filteredTimestamps) >= 3) {
                    releaseCallerLock(caller);
                    Debug.trap("You can only open 3 positions per hour");
                };
            };
        };

        // Check maximum deposit limit BEFORE the transfer
        if (not isCompounding) {
            let maxDeposit = Float.max(platformStats.potBalance * 0.2, 5.0);
            if (amount > maxDeposit) {
                releaseCallerLock(caller);
                Debug.trap("Maximum deposit for simple mode is the greater of 20% of current pot balance or 5 ICP (" # formatICP(maxDeposit) # " ICP)");
            };
        };

        // Transfer ICP from user to canister via ICRC-2 transfer_from
        // User must have called icrc2_approve on the ICP ledger first
        let selfPrincipal = switch (canisterPrincipal) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("Canister principal not set");
            };
            case (?p) { p };
        };

        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        // Wrap the ledger await in try/catch: if the inter-canister call
        // throws (destination trap, transport failure, etc.) instead of
        // returning #Err, the lock acquired above is already committed at the
        // await and would otherwise stay held forever. Release it before
        // re-trapping so the caller isn't permanently stuck.
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
            releaseCallerLock(caller);
            Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                releaseCallerLock(caller);
                let errMsg = switch (err) {
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
                Debug.trap(errMsg);
            };
            case (#Ok(_blockIndex)) {};
        };

        // Record the rate limit timestamp (check was done before transfer)
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) {
                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, List.nil()));
            };
            case (?timestamps) {
                let filteredTimestamps = List.filter<Int>(
                    timestamps,
                    func(timestamp) { currentHour - timestamp < 1 },
                );
                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, filteredTimestamps));
            };
        };

        // Cover Charge: 2% skimmed from every deposit, routed 100% to the
        // admin bucket (see coverChargeBalance). Exit tolls still use the
        // 50/50 pot/backer split via distributeExitTollToBackers — only the entry
        // fee changed. Pot receives 98% of the gross deposit.
        let coverCharge = amount * COVER_CHARGE_RATE;
        let coverChargeE8s = Int.abs(Float.toInt(coverCharge * 100_000_000.0));
        coverChargeBalance += coverChargeE8s;
        let netAmount = amount - coverCharge;

        let gameId = nextGameId;
        nextGameId += 1;

        // Log the cover charge (separate audit trail from walletTransactions).
        // Skip zero-amount entries — only possible with sub-satoshi deposits.
        if (coverChargeE8s > 0) {
            recordCoverChargeTransaction(gameId, caller, coverChargeE8s);
        };

        // Game record tracks the full deposit amount (for earnings calculation)
        let newGame : GameRecord = {
            id = gameId;
            player = caller;
            plan;
            amount; // Earnings calculated on full deposit
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

        // Register referral relationship (one-time, first referrer wins)
        switch (referrer) {
            case (null) {};
            case (?ref) {
                registerReferral(caller, ref);
            };
        };

        // Award Ponzi Points based on plan
        let points = switch (plan) {
            case (#simple21Day) { amount * 1000.0 };
            case (#compounding15Day) { amount * 2000.0 };
            case (#compounding30Day) { amount * 3000.0 };
        };
        awardPonziPoints(caller, points);

        releaseCallerLock(caller);
        gameId;
    };

    // Add Dealer Money (Seed Round — transfers ICP directly from user's wallet)
    public shared ({ caller }) func addDealerMoney(amount : Float) : async () {
        requireAuthenticated(caller);
        validateAmount(amount);
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

        acquireCallerLock(caller);

        // Validate 8 decimal places
        if (not validateEightDecimals(amount)) {
            Debug.trap("Amount cannot have more than 8 decimal places");
        };

        // Transfer ICP from user to canister via ICRC-2 transfer_from
        let selfPrincipal = switch (canisterPrincipal) {
            case (null) { Debug.trap("Canister principal not set") };
            case (?p) { p };
        };

        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        // See createGame for the rationale — wrap the ledger await so that an
        // inter-canister exception doesn't strand the caller lock.
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
            releaseCallerLock(caller);
            Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                releaseCallerLock(caller);
                let errMsg = switch (err) {
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
                Debug.trap(errMsg);
            };
            case (#Ok(_blockIndex)) {};
        };

        let entitlement = amount * 1.24; // Series A: 24% bonus

        // Get the user's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Anonymous Backer" };
            case (?profile) { profile.name };
        };

        switch (principalMapNat.get(dealerPositions, caller)) {
            case (null) {
                let newDealer : DealerPosition = {
                    owner = caller;
                    amount;
                    entitlement;
                    startTime = Time.now();
                    isActive = true;
                    name;
                    dealerType = #upstream;
                    firstDepositDate = ?Time.now();
                };
                dealerPositions := principalMapNat.put(dealerPositions, caller, newDealer);
            };
            case (?existingDealer) {
                let updatedDealer : DealerPosition = {
                    existingDealer with
                    amount = existingDealer.amount + amount;
                    entitlement = existingDealer.entitlement + entitlement;
                    name;
                };
                dealerPositions := principalMapNat.put(dealerPositions, caller, updatedDealer);
            };
        };

        // Award 4,000 Ponzi Points per ICP deposited
        awardPonziPoints(caller, amount * 4000.0);

        // Add the deposited amount directly to the pot
        platformStats := {
            platformStats with
            potBalance = platformStats.potBalance + amount;
        };

        releaseCallerLock(caller);
    };

    // ========================================================================
    // Ponzi Points & Referral System (PP-only, never ICP)
    // Referral rates: Level 1 = 8%, Level 2 = 5%, Level 3 = 2% of PP earned
    // ========================================================================

    // Award Ponzi Points to a user AND cascade referral PP up the chain
    func awardPonziPoints(user : Principal, points : Float) {
        creditPonziPointsDirect(user, points);
        awardReferralPP(user, points);
    };

    // Credit PP directly without triggering referral cascade (prevents infinite recursion)
    func creditPonziPointsDirect(user : Principal, points : Float) {
        let current = switch (principalMapNat.get(ponziPoints, user)) {
            case (null) { 0.0 };
            case (?existing) { existing };
        };
        ponziPoints := principalMapNat.put(ponziPoints, user, current + points);
    };

    // Walk the referral chain and award PP at each level (8% / 5% / 2%)
    func awardReferralPP(user : Principal, pointsEarned : Float) {
        // Level 1: direct referrer gets 8%
        switch (principalMapNat.get(referralChain, user)) {
            case (null) {}; // no referrer
            case (?level1Referrer) {
                let l1Points = pointsEarned * 0.08;
                creditPonziPointsDirect(level1Referrer, l1Points);
                creditReferralEarnings(level1Referrer, l1Points, 1);

                // Level 2: referrer's referrer gets 5%
                switch (principalMapNat.get(referralChain, level1Referrer)) {
                    case (null) {};
                    case (?level2Referrer) {
                        let l2Points = pointsEarned * 0.05;
                        creditPonziPointsDirect(level2Referrer, l2Points);
                        creditReferralEarnings(level2Referrer, l2Points, 2);

                        // Level 3: one more hop up the chain gets 2%
                        switch (principalMapNat.get(referralChain, level2Referrer)) {
                            case (null) {};
                            case (?level3Referrer) {
                                let l3Points = pointsEarned * 0.02;
                                creditPonziPointsDirect(level3Referrer, l3Points);
                                creditReferralEarnings(level3Referrer, l3Points, 3);
                            };
                        };
                    };
                };
            };
        };
    };

    // Track referral earnings by level (for display in the MLM dashboard)
    func creditReferralEarnings(referrer : Principal, points : Float, level : Nat) {
        let current = switch (principalMapNat.get(referralEarnings, referrer)) {
            case (null) { { level1Points = 0.0; level2Points = 0.0; level3Points = 0.0 } };
            case (?existing) { existing };
        };
        let updated : ReferralEarnings = switch (level) {
            case (1) { { current with level1Points = current.level1Points + points } };
            case (2) { { current with level2Points = current.level2Points + points } };
            case (3) { { current with level3Points = current.level3Points + points } };
            case (_) { current };
        };
        referralEarnings := principalMapNat.put(referralEarnings, referrer, updated);
    };

    // Register a referral relationship (one-time, first referrer wins)
    func registerReferral(user : Principal, referrer : Principal) {
        // Can't refer yourself
        if (user == referrer) { return };
        // Only register if user doesn't already have a referrer
        switch (principalMapNat.get(referralChain, user)) {
            case (?_) {}; // already has a referrer
            case (null) {
                referralChain := principalMapNat.put(referralChain, user, referrer);
            };
        };
    };

    // ========================================================================
    // Fee Distribution System
    // ========================================================================

    // Credit a backer's repayment balance (internal bookkeeping, not yet in wallet).
    // Note: the underlying state map (dealerRepayments) and Candid type
    // (DealerPosition) retain their original names — renaming those is a
    // stable-storage migration tracked separately. This is a pure internal rename.
    func creditBackerRepayment(backer : Principal, amount : Float) {
        let current = switch (principalMapNat.get(dealerRepayments, backer)) {
            case (null) { 0.0 };
            case (?existing) { existing };
        };
        dealerRepayments := principalMapNat.put(dealerRepayments, backer, current + amount);
    };

    // Distribute an exit toll using the 35/25/40 formula to backer repayment
    // balances. `tollAmount` is the FULL toll — this function handles the
    // 50/50 pot/backer split. Returns the portion that stays in the pot.
    //
    // This is called ONLY from exit toll paths after the cover-charge refactor.
    // The entry fee (cover charge) routes 100% to Management and never enters
    // this function.
    func distributeExitTollToBackers(tollAmount : Float) : Float {
        let potSeedAmount = tollAmount * 0.5;           // 50% seeds the next round
        let backerRepaymentAmount = tollAmount * 0.5;   // 50% to backer repayment

        // Get all backers
        let allBackers = Iter.toArray(principalMapNat.vals(dealerPositions));
        if (allBackers.size() == 0) {
            // No backers — everything stays in pot
            return tollAmount;
        };

        let seriesABackers = List.toArray(
            List.filter(
                List.fromArray(allBackers),
                func(dealer : DealerPosition) : Bool {
                    dealer.dealerType == #upstream;
                },
            )
        );

        // Find oldest Series A (upstream) backer
        var oldestBacker : ?DealerPosition = null;
        var oldestTime : Int = 0;
        for (dealer in seriesABackers.vals()) {
            switch (dealer.firstDepositDate) {
                case (null) {};
                case (?date) {
                    if (oldestBacker == null or date < oldestTime) {
                        oldestBacker := ?dealer;
                        oldestTime := date;
                    };
                };
            };
        };

        // 35% to oldest Series A backer
        switch (oldestBacker) {
            case (null) {};
            case (?dealer) {
                creditBackerRepayment(dealer.owner, backerRepaymentAmount * 0.35);
            };
        };

        // 25% split among other Series A backers
        let otherSeriesABackers = List.toArray(
            List.filter(
                List.fromArray(seriesABackers),
                func(dealer : DealerPosition) : Bool {
                    switch (oldestBacker) {
                        case (null) { true };
                        case (?oldest) { dealer.owner != oldest.owner };
                    };
                },
            )
        );
        if (otherSeriesABackers.size() > 0) {
            let perBacker = backerRepaymentAmount * 0.25 / Float.fromInt(otherSeriesABackers.size());
            for (dealer in otherSeriesABackers.vals()) {
                creditBackerRepayment(dealer.owner, perBacker);
            };
        };

        // 40% split among all backers
        let perBacker = backerRepaymentAmount * 0.4 / Float.fromInt(allBackers.size());
        for (dealer in allBackers.vals()) {
            creditBackerRepayment(dealer.owner, perBacker);
        };

        potSeedAmount; // Return the portion that stays in pot
    };

    // Calculate exit toll fee based on game type and elapsed time
    // Simple: 7% (< 3 days), 5% (3-10 days), 3% (> 10 days)
    // Compounding: flat 13%
    func calculateExitToll(game : GameRecord, earnings : Float) : Float {
        if (game.isCompounding) {
            earnings * 0.13
        } else {
            let elapsedSeconds = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
            let elapsedDays = elapsedSeconds / 86400.0;
            if (elapsedDays < 3.0) {
                earnings * 0.07
            } else if (elapsedDays < 10.0) {
                earnings * 0.05
            } else {
                earnings * 0.03
            };
        };
    };

    // Get All Active Games
    public query func getAllActiveGames() : async [GameRecord] {
        var activeGames = List.nil<GameRecord>();
        for (game in natMap.vals(gameRecords)) {
            if (game.isActive) {
                activeGames := List.push(game, activeGames);
            };
        };
        List.toArray(activeGames);
    };

    // Withdraw Earnings
    public shared ({ caller }) func withdrawEarnings(gameId : Nat) : async Float {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        switch (natMap.get(gameRecords, gameId)) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("Game not found");
            };
            case (?game) {
                if (game.player != caller) {
                    releaseCallerLock(caller);
                    Debug.trap("Unauthorized: Only the game owner can withdraw earnings");
                };
                if (game.isCompounding) {
                    releaseCallerLock(caller);
                    Debug.trap("Cannot withdraw from compounding games");
                };

                let earnings = await calculateEarnings(game);

                // Apply exit toll (simple: 7%/5%/3% tiered by time)
                let exitToll = calculateExitToll(game, earnings);
                let netEarnings = roundToEightDecimals(earnings - exitToll);

                // Capture state snapshots for compensation-on-failure
                let originalGame = game;
                let originalStats = platformStats;
                let originalRepayments = dealerRepayments;

                // Distribute exit toll: 50% stays in pot, 50% to dealers
                let potSeedFromToll = distributeExitTollToBackers(exitToll);

                // Check solvency against what actually leaves the pot
                let potDeduction = netEarnings + (exitToll - potSeedFromToll);
                if (potDeduction > platformStats.potBalance) {
                    // Revert dealer distribution before trapping
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    triggerGameReset("Insufficient funds for payout");
                    Debug.trap("Game reset due to insufficient funds");
                };

                // Reset the game record and update platform stats
                let updatedGame : GameRecord = {
                    game with
                    accumulatedEarnings = 0.0;
                    lastUpdateTime = Time.now();
                    totalWithdrawn = game.totalWithdrawn + netEarnings;
                };
                gameRecords := natMap.put(gameRecords, gameId, updatedGame);

                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + netEarnings;
                    potBalance = platformStats.potBalance - potDeduction;
                };

                // Pay out to user's ledger account (saga: revert on failure)
                let netEarningsE8s = Int.abs(Float.toInt(netEarnings * 100_000_000.0));
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
                    // Compensate: refund on catch (network failure — transfer status unknown)
                    // Note: This is the conservative approach; the transfer may have succeeded.
                    // In production, consider logging for manual reconciliation.
                    gameRecords := natMap.put(gameRecords, gameId, originalGame);
                    platformStats := originalStats;
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
                };

                switch (transferResult) {
                    case (#Err(err)) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        dealerRepayments := originalRepayments;
                        releaseCallerLock(caller);
                        let errMsg = switch (err) {
                            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                            case (#BadFee(_)) { "Bad fee" };
                            case (#BadBurn(_)) { "Bad burn" };
                            case (#TooOld) { "Transaction too old" };
                            case (#CreatedInFuture(_)) { "Transaction created in future" };
                            case (#Duplicate(_)) { "Duplicate transaction" };
                            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                            case (#GenericError(e)) { "Error: " # e.message };
                        };
                        Debug.trap(errMsg);
                    };
                    case (#Ok(_)) {};
                };

                releaseCallerLock(caller);
                netEarnings;
            };
        };
    };

    // Settle a compounding game at maturity (credits principal + earnings to wallet)
    public shared ({ caller }) func settleCompoundingGame(gameId : Nat) : async Float {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        switch (natMap.get(gameRecords, gameId)) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("Game not found");
            };
            case (?game) {
                if (game.player != caller) {
                    releaseCallerLock(caller);
                    Debug.trap("Unauthorized: Only the game owner can settle this game");
                };
                if (not game.isCompounding) {
                    releaseCallerLock(caller);
                    Debug.trap("This function is only for compounding games. Use withdrawEarnings instead.");
                };
                if (not game.isActive) {
                    releaseCallerLock(caller);
                    Debug.trap("Game is already settled");
                };

                // Check that the plan has matured
                let timeElapsedSeconds = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
                let daysElapsed = timeElapsedSeconds / 86400.0;
                let maturityDays = switch (game.plan) {
                    case (#compounding15Day) { 15.0 };
                    case (#compounding30Day) { 30.0 };
                    case (#simple21Day) {
                        releaseCallerLock(caller);
                        Debug.trap("Simple games cannot be settled this way");
                    };
                };
                if (daysElapsed < maturityDays) {
                    releaseCallerLock(caller);
                    Debug.trap("Game has not matured yet. " # Float.toText(maturityDays - daysElapsed) # " days remaining.");
                };

                // Calculate final compounded earnings (capped at maturity)
                let dailyRate = switch (game.plan) {
                    case (#compounding15Day) { 0.12 };
                    case (#compounding30Day) { 0.09 };
                    case (#simple21Day) { 0.0 }; // unreachable
                };
                let earnings = game.amount * (Float.pow(1.0 + dailyRate, maturityDays) - 1.0);

                // Apply exit toll: flat 13% on earnings for compounding games
                let exitToll = calculateExitToll(game, earnings);
                let netEarnings = roundToEightDecimals(earnings - exitToll);
                let totalPayout = roundToEightDecimals(game.amount + netEarnings); // principal + net earnings

                // Capture state snapshots for compensation-on-failure.
                // NOTE: distributeExitTollToBackers writes to dealerRepayments
                // (via creditBackerRepayment) and only reads dealerPositions.
                // Snapshot and revert dealerRepayments — reverting dealerPositions
                // is a no-op that leaks repayments on retry. See commit 588cedb.
                let originalGame = game;
                let originalStats = platformStats;
                let originalRepayments = dealerRepayments;

                // Distribute exit toll: 50% stays in pot, 50% to dealers
                let potSeedFromToll = distributeExitTollToBackers(exitToll);

                // Pot loses: totalPayout (to player) + dealer portion of toll
                let potDeduction = totalPayout + (exitToll - potSeedFromToll);
                if (potDeduction > platformStats.potBalance) {
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    triggerGameReset("Insufficient funds for compounding game settlement");
                    Debug.trap("Game reset due to insufficient funds");
                };

                // Mark game as settled
                let settledGame : GameRecord = {
                    game with
                    isActive = false;
                    accumulatedEarnings = netEarnings;
                    totalWithdrawn = totalPayout;
                    lastUpdateTime = Time.now();
                };
                gameRecords := natMap.put(gameRecords, gameId, settledGame);

                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + totalPayout;
                    potBalance = platformStats.potBalance - potDeduction;
                    activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                };

                // Pay out to user's ledger account (saga: revert on failure)
                let payoutE8s = Int.abs(Float.toInt(totalPayout * 100_000_000.0));
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
                    // Compensate: refund on catch (network failure — transfer status unknown)
                    // Note: This is the conservative approach; the transfer may have succeeded.
                    // In production, consider logging for manual reconciliation.
                    gameRecords := natMap.put(gameRecords, gameId, originalGame);
                    platformStats := originalStats;
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
                };

                switch (transferResult) {
                    case (#Err(err)) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        dealerRepayments := originalRepayments;
                        releaseCallerLock(caller);
                        let errMsg = switch (err) {
                            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                            case (#BadFee(_)) { "Bad fee" };
                            case (#BadBurn(_)) { "Bad burn" };
                            case (#TooOld) { "Transaction too old" };
                            case (#CreatedInFuture(_)) { "Transaction created in future" };
                            case (#Duplicate(_)) { "Duplicate transaction" };
                            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                            case (#GenericError(e)) { "Error: " # e.message };
                        };
                        Debug.trap(errMsg);
                    };
                    case (#Ok(_)) {};
                };

                releaseCallerLock(caller);
                totalPayout;
            };
        };
    };

    // Calculate Earnings (with time cap at plan duration)
    public query func calculateEarnings(game : GameRecord) : async Float {
        let dailyRate = switch (game.plan) {
            case (#simple21Day) { 0.11 }; // 11% daily rate for simple mode
            case (#compounding15Day) { 0.12 };
            case (#compounding30Day) { 0.09 };
        };

        // Plan duration cap in seconds
        let maxDurationSeconds = switch (game.plan) {
            case (#simple21Day) { 21.0 * 86400.0 };
            case (#compounding15Day) { 15.0 * 86400.0 };
            case (#compounding30Day) { 30.0 * 86400.0 };
        };

        // Calculate how much time was already accounted for (previous claims)
        let timeAlreadyAccounted = Float.fromInt((game.lastUpdateTime - game.startTime) / 1_000_000_000);
        let remainingAllowedTime = Float.max(0.0, maxDurationSeconds - timeAlreadyAccounted);

        // Time since last claim, capped at remaining allowed time
        let timeSinceLastUpdate = Float.fromInt((Time.now() - game.lastUpdateTime) / 1_000_000_000);
        let timeElapsed = Float.min(timeSinceLastUpdate, remainingAllowedTime);

        let earnings = game.amount * dailyRate * (timeElapsed / 86400.0);
        roundToEightDecimals(game.accumulatedEarnings + earnings);
    };

    // Calculate Compounded Earnings for 15-Day Plan (caps at 15 days)
    public query func calculateCompoundedEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding15Day) {
            Debug.trap("This calculation is only for the 15-day compounding plan");
        };

        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 15.0); // Cap at 15 days

        let dailyRate = 0.12; // 12% daily rate for 15-day compounding
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);

        roundToEightDecimals(compoundedEarnings);
    };

    // Calculate Compounded Earnings for 30-Day Plan (caps at 30 days)
    public query func calculateCompounded30DayEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding30Day) {
            Debug.trap("This calculation is only for the 30-day compounding plan");
        };

        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1_000_000_000);
        let daysElapsed = Float.min(timeElapsed / 86400.0, 30.0); // Cap at 30 days

        let dailyRate = 0.09; // 9% daily rate for 30-day compounding
        let compoundedEarnings = game.amount * (Float.pow(1.0 + dailyRate, daysElapsed) - 1.0);

        roundToEightDecimals(compoundedEarnings);
    };

    // Calculate Compounded ROI for 15-Day Plan
    public query func calculateCompoundedROI() : async Float {
        let dailyRate = 0.12; // 12% daily rate for 15-day compounding
        let days = 15.0;
        let roi = Float.pow(1.0 + dailyRate, days) - 1.0;
        roundToEightDecimals(roi);
    };

    // Get Platform Stats
    public query func getPlatformStats() : async PlatformStats {
        {
            platformStats with
            // March 16 2026 00:00 PST — first mainnet deployment
            daysActive = Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
        };
    };

    // Trigger Game Reset
    func triggerGameReset(reason : Text) {
        let resetRecord : GameResetRecord = {
            resetTime = Time.now();
            reason;
        };
        gameResetHistory := intMap.put(gameResetHistory, Time.now(), resetRecord);
        gameRecords := natMap.empty<GameRecord>();
        platformStats := {
            totalDeposits = 0.0;
            totalWithdrawals = 0.0;
            activeGames = 0;
            potBalance = 0.0;
            daysActive = 0;
        };
        nextGameId := 0;
    };

    // Get Game Reset History
    public query func getGameResetHistory() : async [GameResetRecord] {
        Iter.toArray(intMap.vals(gameResetHistory));
    };

    // Get User Games
    public query ({ caller }) func getUserGames() : async [GameRecord] {
        var userGames = List.nil<GameRecord>();
        for (game in natMap.vals(gameRecords)) {
            if (game.player == caller) {
                userGames := List.push(game, userGames);
            };
        };
        List.toArray(userGames);
    };

    // Get Available Balance
    public query func getAvailableBalance() : async Float {
        platformStats.potBalance;
    };

    // Get Game By ID
    public query func getGameById(gameId : Nat) : async ?GameRecord {
        natMap.get(gameRecords, gameId);
    };

    // Get Referral Earnings (total PP earned through referrals)
    public query func getReferralEarnings(user : Principal) : async Float {
        switch (principalMapNat.get(referralEarnings, user)) {
            case (null) { 0.0 };
            case (?earnings) { earnings.level1Points + earnings.level2Points + earnings.level3Points };
        };
    };

    // Get All Games
    public query func getAllGames() : async [GameRecord] {
        Iter.toArray(natMap.vals(gameRecords));
    };

    // Get Active Game Count
    public query func getActiveGameCount() : async Nat {
        var count = 0;
        for (game in natMap.vals(gameRecords)) {
            if (game.isActive) {
                count += 1;
            };
        };
        count;
    };

    // Get Total Deposits
    public query func getTotalDeposits() : async Float {
        platformStats.totalDeposits;
    };

    // Get Total Withdrawals
    public query func getTotalWithdrawals() : async Float {
        platformStats.totalWithdrawals;
    };

    // Get Days Active (since first mainnet deployment: March 16 2026 00:00 PST)
    public query func getDaysActive() : async Nat {
        Int.abs((Time.now() - 1_773_644_400_000_000_000) / 86_400_000_000_000);
    };

    // Get Maximum Deposit Limit
    public query func getMaxDepositLimit() : async Float {
        Float.max(platformStats.potBalance * 0.2, 5.0);
    };

    // Check Deposit Rate Limit
    public query ({ caller }) func checkDepositRateLimit() : async Bool {
        let currentTime = Time.now();
        let currentHour = currentTime / 3600000000000; // Convert to hours

        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?timestamps) {
                let filteredTimestamps = List.filter<Int>(
                    timestamps,
                    func(timestamp) {
                        currentHour - timestamp < 1;
                    },
                );
                List.size(filteredTimestamps) < 3;
            };
        };
    };

    // Get Dealer Repayment Balance
    public query ({ caller }) func getDealerRepaymentBalance() : async Float {
        switch (principalMapNat.get(dealerRepayments, caller)) {
            case (null) { 0.0 };
            case (?balance) { balance };
        };
    };

    // Claim Dealer Repayment — transfers repayment balance to user's ledger account
    public shared ({ caller }) func claimDealerRepayment() : async Float {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        let balance = switch (principalMapNat.get(dealerRepayments, caller)) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("No repayment balance to claim");
            };
            case (?b) {
                if (b <= 0.0) {
                    releaseCallerLock(caller);
                    Debug.trap("No repayment balance to claim");
                };
                b;
            };
        };

        // Zero out the repayment balance (compensate on failure)
        dealerRepayments := principalMapNat.put(dealerRepayments, caller, 0.0);

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
            // Compensate: refund on catch (network failure — transfer status unknown)
            // Note: This is the conservative approach; the transfer may have succeeded.
            // In production, consider logging for manual reconciliation.
            dealerRepayments := principalMapNat.put(dealerRepayments, caller, balance);
            releaseCallerLock(caller);
            Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                dealerRepayments := principalMapNat.put(dealerRepayments, caller, balance);
                releaseCallerLock(caller);
                let errMsg = switch (err) {
                    case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                    case (#BadFee(_)) { "Bad fee" };
                    case (#BadBurn(_)) { "Bad burn" };
                    case (#TooOld) { "Transaction too old" };
                    case (#CreatedInFuture(_)) { "Transaction created in future" };
                    case (#Duplicate(_)) { "Duplicate transaction" };
                    case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                    case (#GenericError(e)) { "Error: " # e.message };
                };
                Debug.trap(errMsg);
            };
            case (#Ok(_)) {};
        };

        releaseCallerLock(caller);
        balance;
    };

    // Get Dealer Positions
    public query func getDealerPositions() : async [DealerPosition] {
        Iter.toArray(principalMapNat.vals(dealerPositions));
    };

    // Get Ponzi Points
    public query ({ caller }) func getPonziPoints() : async Float {
        switch (principalMapNat.get(ponziPoints, caller)) {
            case (null) { 0.0 };
            case (?points) { points };
        };
    };

    // Get Ponzi Points Balance (for Rewards page)
    public query ({ caller }) func getPonziPointsBalance() : async {
        totalPoints : Float;
        depositPoints : Float;
        referralPoints : Float;
    } {
        let totalPoints = switch (principalMapNat.get(ponziPoints, caller)) {
            case (null) { 0.0 };
            case (?points) { points };
        };

        // Calculate deposit points (from games and dealer positions)
        var depositPoints = 0.0;
        for (game in natMap.vals(gameRecords)) {
            if (game.player == caller) {
                depositPoints += switch (game.plan) {
                    case (#simple21Day) { game.amount * 1000.0 };
                    case (#compounding15Day) { game.amount * 2000.0 };
                    case (#compounding30Day) { game.amount * 3000.0 };
                };
            };
        };
        switch (principalMapNat.get(dealerPositions, caller)) {
            case (null) {};
            case (?dealer) {
                depositPoints += dealer.amount * 4000.0;
            };
        };

        // Calculate referral points (PP earned through the referral chain)
        let referralPoints = switch (principalMapNat.get(referralEarnings, caller)) {
            case (null) { 0.0 };
            case (?earnings) { earnings.level1Points + earnings.level2Points + earnings.level3Points };
        };

        {
            totalPoints;
            depositPoints;
            referralPoints;
        };
    };

    // Get Referral Tier Points (for Multi-Level Marketing page)
    public query ({ caller }) func getReferralTierPoints() : async {
        level1Points : Float;
        level2Points : Float;
        level3Points : Float;
        totalPoints : Float;
    } {
        let earnings = switch (principalMapNat.get(referralEarnings, caller)) {
            case (null) { { level1Points = 0.0; level2Points = 0.0; level3Points = 0.0 } };
            case (?e) { e };
        };

        {
            level1Points = earnings.level1Points;
            level2Points = earnings.level2Points;
            level3Points = earnings.level3Points;
            totalPoints = earnings.level1Points + earnings.level2Points + earnings.level3Points;
        };
    };

    // Calculate Total Dealer Debt (including 12% bonus)
    public query func getTotalDealerDebt() : async Float {
        var totalDebt = 0.0;
        for (dealer in principalMapNat.vals(dealerPositions)) {
            totalDebt += dealer.entitlement;
        };
        totalDebt;
    };

    // Format ICP values without unnecessary trailing zeros
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
                    if (trimmed == "") {
                        parts[0];
                    } else {
                        parts[0] # "." # trimmed;
                    };
                };
                case (_) { textValue };
            };
        };
    };

    // Round to 8 decimal places
    func roundToEightDecimals(value : Float) : Float {
        let multiplier = 100000000.0;
        Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
    };

    // Validate 8 decimal places
    func validateEightDecimals(value : Float) : Bool {
        let multiplier = 100000000.0;
        let rounded = Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
        rounded == value;
    };

    // === Cross-canister API for Shenanigans canister ===

    func requireShenanigansCanister(caller : Principal) {
        switch (shenanigansPrincipal) {
            case (null) { Debug.trap("Shenanigans canister not configured") };
            case (?p) {
                if (caller != p) {
                    Debug.trap("Unauthorized: only shenanigans canister can call this");
                };
            };
        };
    };

    public shared ({ caller }) func setShenanigansPrincipal(p : Principal) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can set shenanigans principal");
        };
        shenanigansPrincipal := ?p;
    };

    public shared ({ caller }) func deductPonziPoints(user : Principal, amount : Float) : async () {
        requireShenanigansCanister(caller);
        let current = switch (principalMapNat.get(ponziPoints, user)) {
            case (null) { 0.0 };
            case (?points) { points };
        };
        if (current < amount) { Debug.trap("Insufficient points") };
        ponziPoints := principalMapNat.put(ponziPoints, user, current - amount);
    };

    public shared ({ caller }) func transferPonziPoints(from : Principal, to : Principal, amount : Float) : async () {
        requireShenanigansCanister(caller);
        let fromBalance = switch (principalMapNat.get(ponziPoints, from)) {
            case (null) { 0.0 };
            case (?points) { points };
        };
        if (fromBalance < amount) { Debug.trap("Insufficient points") };
        ponziPoints := principalMapNat.put(ponziPoints, from, fromBalance - amount);
        let toBalance = switch (principalMapNat.get(ponziPoints, to)) {
            case (null) { 0.0 };
            case (?points) { points };
        };
        ponziPoints := principalMapNat.put(ponziPoints, to, toBalance + amount);
    };

    public shared ({ caller }) func distributeDealerCutFromShenanigans(amount : Float) : async () {
        requireShenanigansCanister(caller);
        updateDealerCut(amount);
    };

    // Restricted to shenanigans canister (used for balance checks before casting)
    public shared ({ caller }) func getPonziPointsBalanceFor(user : Principal) : async Float {
        requireShenanigansCanister(caller);
        switch (principalMapNat.get(ponziPoints, user)) {
            case (null) { 0.0 };
            case (?points) { points };
        };
    };

    public shared ({ caller }) func burnPonziPoints(user : Principal, amount : Float) : async () {
        requireShenanigansCanister(caller);
        let burned = switch (principalMapNat.get(ponziPointsBurned, user)) {
            case (null) { 0.0 };
            case (?existing) { existing };
        };
        ponziPointsBurned := principalMapNat.put(ponziPointsBurned, user, burned + amount);
    };

    // castShenanigan, determineOutcome, updateShenaniganStats — moved to shenanigans canister

    // Update dealer cut (distribute among active dealers)
    func updateDealerCut(amount : Float) {
        let activeDealers = Iter.toArray(principalMapNat.vals(dealerPositions));
        let activeCount = activeDealers.size();

        if (activeCount > 0) {
            let perDealerAmount = amount / Float.fromInt(activeCount);
            for (dealer in activeDealers.vals()) {
                let currentRepayment = switch (principalMapNat.get(dealerRepayments, dealer.owner)) {
                    case (null) { 0.0 };
                    case (?repayment) { repayment };
                };
                dealerRepayments := principalMapNat.put(dealerRepayments, dealer.owner, currentRepayment + perDealerAmount);
            };
        };
    };

    // getShenaniganStats, getRecentShenanigans — moved to shenanigans canister

    // Get Top Ponzi Points Holders
    public query func getTopPonziPointsHolders() : async [(Principal, Float)] {
        let allPoints = Iter.toArray(principalMapNat.entries(ponziPoints));
        let sorted = List.fromArray(allPoints);
        let top = List.take(sorted, 50);
        List.toArray(top);
    };

    // Get Top Ponzi Points Burners
    public query func getTopPonziPointsBurners() : async [(Principal, Float)] {
        let allBurned = Iter.toArray(principalMapNat.entries(ponziPointsBurned));
        let sorted = List.fromArray(allBurned);
        let top = List.take(sorted, 50);
        List.toArray(top);
    };

    // Get Total House Money Added
    public query func getTotalHouseMoneyAdded() : async Float {
        var totalHouseMoney = 0.0;
        for (record in natMap.vals(houseLedger)) {
            if (record.amount > 0.0) {
                totalHouseMoney += record.amount;
            };
        };
        totalHouseMoney;
    };

    // Add Downstream Dealer (The Redistribution Event)
    public shared ({ caller }) func addDownstreamDealer(amount : Float, underwaterAmount : Float) : async () {
        requireAuthenticated(caller);
        validateAmount(amount);
        validateAmount(underwaterAmount);
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

        // Validate 8 decimal places
        if (not validateEightDecimals(amount)) {
            Debug.trap("Amount cannot have more than 8 decimal places");
        };

        let entitlement = underwaterAmount * 1.16; // Series B: 16% bonus on underwater amount

        // Get the user's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Anonymous Backer" };
            case (?profile) { profile.name };
        };

        let newDealer : DealerPosition = {
            owner = caller;
            amount;
            entitlement;
            startTime = Time.now();
            isActive = true;
            name;
            dealerType = #downstream;
            firstDepositDate = null;
        };
        dealerPositions := principalMapNat.put(dealerPositions, caller, newDealer);

        // Award 4,000 Ponzi Points per ICP deposited
        awardPonziPoints(caller, amount * 4000.0);

        // Add the deposited amount directly to the pot
        platformStats := {
            platformStats with
            potBalance = platformStats.potBalance + amount;
        };
    };

    // Get Oldest Upstream Dealer
    public query func getOldestUpstreamDealer() : async ?DealerPosition {
        var oldestDealer : ?DealerPosition = null;
        var oldestTime : Int = 0;

        for (dealer in principalMapNat.vals(dealerPositions)) {
            if (dealer.dealerType == #upstream) {
                switch (dealer.firstDepositDate) {
                    case (null) {};
                    case (?date) {
                        if (oldestDealer == null or date < oldestTime) {
                            oldestDealer := ?dealer;
                            oldestTime := date;
                        };
                    };
                };
            };
        };
        oldestDealer;
    };

    // Manual backer-repayment distribution (admin only).
    // Uses the same 50/50 pot/backer split as automatic exit-toll distribution.
    // The public Candid name stays `distributeFees` for backward compatibility;
    // internally it delegates to `distributeExitTollToBackers`.
    public shared ({ caller }) func distributeFees(totalFees : Float) : async () {
        requireAuthenticated(caller);
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can distribute fees");
        };
        validateAmount(totalFees);
        ignore distributeExitTollToBackers(totalFees);
    };

    // getShenaniganConfigs, updateShenaniganConfig, resetShenaniganConfig,
    // saveAllShenaniganConfigs — moved to shenanigans canister

    // ICRC-21 Consent Messages
    public shared func icrc21_canister_call_consent_message(request : Icrc21.ConsentMessageRequest) : async Icrc21.ConsentMessageResponse {
        Icrc21.consentMessage(request);
    };

    // ICRC-28 Trusted Origins
    public query func icrc28_trusted_origins() : async Icrc21.TrustedOriginsResponse {
        Icrc21.trustedOrigins();
    };

    // ICRC-10 Supported Standards
    public query func icrc10_supported_standards() : async [Icrc21.StandardRecord] {
        Icrc21.supportedStandards();
    };
};

