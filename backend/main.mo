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

    // Initialize Access Control
    public shared ({ caller }) func initializeAccessControl() : async () {
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

    // Referral Record
    public type ReferralRecord = {
        referrer : Principal;
        level : Nat;
        earnings : Float;
        depositCount : Nat;
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
    var referralRecords = principalMapNat.empty<ReferralRecord>();
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

    // Get wallet balance for caller (in e8s)
    public query ({ caller }) func getWalletBalance() : async Nat {
        switch (principalMapNat.get(walletBalances, caller)) {
            case (null) {
                // In test mode, give new users 500 ICP (50_000_000_000 e8s)
                if (testMode) { 50_000_000_000 } else { 0 };
            };
            case (?balance) { balance };
        };
    };

    // Get wallet balance as ICP (Float) for display
    public query ({ caller }) func getWalletBalanceICP() : async Float {
        let e8s = switch (principalMapNat.get(walletBalances, caller)) {
            case (null) {
                if (testMode) { 50_000_000_000 } else { 0 };
            };
            case (?balance) { balance };
        };
        Float.fromInt(e8s) / 100_000_000.0;
    };

    // Initialize wallet for a user (internal helper)
    func initializeWalletIfNeeded(user : Principal) {
        switch (principalMapNat.get(walletBalances, user)) {
            case (null) {
                // In test mode, give 500 ICP
                let initialBalance = if (testMode) { 50_000_000_000 } else { 0 };
                walletBalances := principalMapNat.put(walletBalances, user, initialBalance);
            };
            case (?_) { /* Already initialized */ };
        };
    };

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

    // ========================================================================
    // Real ICP Deposit (ICRC-2 transfer_from)
    // ========================================================================
    
    // Deposit ICP from user's wallet to Musical Chairs
    // User must first call icrc2_approve on the ICP ledger to authorize this canister
    public shared ({ caller }) func depositICP(amount : Nat) : async { #Ok : Nat; #Err : Text } {
        if (amount < 10_000_000) {  // Minimum 0.1 ICP
            return #Err("Minimum deposit is 0.1 ICP (10_000_000 e8s)");
        };

        // Get this canister's principal
        let selfPrincipal = switch (canisterPrincipal) {
            case (null) { return #Err("Canister principal not set. Please contact admin.") };
            case (?p) { p };
        };
        
        try {
            // Use ICRC-2 transfer_from to pull funds from user
            let transferResult = await icpLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = { owner = selfPrincipal; subaccount = null };
                amount = amount;
                fee = null;
                memo = null;
                created_at_time = null;
            });
            
            switch (transferResult) {
                case (#Ok(blockIndex)) {
                    // Credit the user's internal wallet
                    initializeWalletIfNeeded(caller);
                    let currentBalance = switch (principalMapNat.get(walletBalances, caller)) {
                        case (null) { 0 };
                        case (?b) { b };
                    };
                    walletBalances := principalMapNat.put(walletBalances, caller, currentBalance + amount);
                    
                    // Record transaction
                    recordWalletTransaction(caller, #deposit, amount, ?blockIndex, "ICP deposit via ICRC-2");
                    
                    #Ok(blockIndex);
                };
                case (#Err(err)) {
                    let errMsg = switch (err) {
                        case (#BadFee(_)) { "Bad fee" };
                        case (#BadBurn(_)) { "Bad burn" };
                        case (#InsufficientFunds(_)) { "Insufficient funds in your wallet" };
                        case (#InsufficientAllowance(_)) { "Insufficient allowance. Please approve the deposit first." };
                        case (#TooOld) { "Transaction too old" };
                        case (#CreatedInFuture(_)) { "Transaction created in future" };
                        case (#Duplicate(_)) { "Duplicate transaction" };
                        case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                        case (#GenericError(e)) { "Error: " # e.message };
                    };
                    #Err(errMsg);
                };
            };
        } catch (e) {
            #Err("Failed to contact ICP ledger: " # Error.message(e));
        };
    };

    // ========================================================================
    // Real ICP Withdrawal (ICRC-1 transfer)
    // ========================================================================
    
    // Withdraw ICP from Musical Chairs to user's wallet
    public shared ({ caller }) func withdrawICP(amount : Nat) : async { #Ok : Nat; #Err : Text } {
        if (amount < 10_000_000) {  // Minimum 0.1 ICP
            return #Err("Minimum withdrawal is 0.1 ICP (10_000_000 e8s)");
        };

        // Check internal balance
        initializeWalletIfNeeded(caller);
        let currentBalance = switch (principalMapNat.get(walletBalances, caller)) {
            case (null) { 0 };
            case (?b) { b };
        };
        
        // Include transfer fee in the check
        let totalNeeded = amount + Ledger.ICP_TRANSFER_FEE;
        if (currentBalance < totalNeeded) {
            return #Err("Insufficient balance. You have " # Nat.toText(currentBalance) # " e8s but need " # Nat.toText(totalNeeded) # " e8s (including fee)");
        };

        try {
            // Transfer ICP from canister to user
            let transferResult = await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = caller; subaccount = null };
                amount = amount;
                fee = null;
                memo = null;
                created_at_time = null;
            });
            
            switch (transferResult) {
                case (#Ok(blockIndex)) {
                    // Deduct from internal wallet (amount + fee)
                    walletBalances := principalMapNat.put(walletBalances, caller, currentBalance - totalNeeded);
                    
                    // Record transaction
                    recordWalletTransaction(caller, #withdrawal, amount, ?blockIndex, "ICP withdrawal");
                    
                    #Ok(blockIndex);
                };
                case (#Err(err)) {
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
        } catch (e) {
            #Err("Failed to contact ICP ledger: " # Error.message(e));
        };
    };

    // ========================================================================
    // Internal Wallet Operations (for game mechanics)
    // ========================================================================
    
    // Deduct from internal wallet (for game deposits)
    func deductFromWallet(user : Principal, amount : Nat) : Bool {
        initializeWalletIfNeeded(user);
        let currentBalance = switch (principalMapNat.get(walletBalances, user)) {
            case (null) { 0 };
            case (?b) { b };
        };
        
        if (currentBalance < amount) {
            return false;
        };
        
        walletBalances := principalMapNat.put(walletBalances, user, currentBalance - amount);
        recordWalletTransaction(user, #gameDeposit, amount, null, "Game deposit");
        true;
    };

    // Credit to internal wallet (for game withdrawals/earnings)
    func creditToWallet(user : Principal, amount : Nat) {
        initializeWalletIfNeeded(user);
        let currentBalance = switch (principalMapNat.get(walletBalances, user)) {
            case (null) { 0 };
            case (?b) { b };
        };
        
        walletBalances := principalMapNat.put(walletBalances, user, currentBalance + amount);
        recordWalletTransaction(user, #gameWithdrawal, amount, null, "Game withdrawal");
    };

    // Transfer between internal wallets
    public shared ({ caller }) func transferInternal(to : Principal, amount : Nat) : async { #Ok; #Err : Text } {
        if (amount < 1_000_000) {  // Minimum 0.01 ICP
            return #Err("Minimum transfer is 0.01 ICP");
        };

        initializeWalletIfNeeded(caller);
        let senderBalance = switch (principalMapNat.get(walletBalances, caller)) {
            case (null) { 0 };
            case (?b) { b };
        };
        
        if (senderBalance < amount) {
            return #Err("Insufficient balance");
        };

        // Deduct from sender
        walletBalances := principalMapNat.put(walletBalances, caller, senderBalance - amount);
        
        // Credit to recipient
        initializeWalletIfNeeded(to);
        let recipientBalance = switch (principalMapNat.get(walletBalances, to)) {
            case (null) { 0 };
            case (?b) { b };
        };
        walletBalances := principalMapNat.put(walletBalances, to, recipientBalance + amount);
        
        // Record transactions
        recordWalletTransaction(caller, #transfer, amount, null, "Transfer to " # Principal.toText(to));
        recordWalletTransaction(to, #transfer, amount, null, "Transfer from " # Principal.toText(caller));
        
        #Ok;
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

        gameId;
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
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

        // Validate 8 decimal places
        if (not validateEightDecimals(amount)) {
            Debug.trap("Amount cannot have more than 8 decimal places");
        };

        // Transfer ICP from user to canister via ICRC-2 transfer_from
        // User must have called icrc2_approve on the ICP ledger first
        let selfPrincipal = switch (canisterPrincipal) {
            case (null) { Debug.trap("Canister principal not set") };
            case (?p) { p };
        };

        let amountE8s = Int.abs(Float.toInt(amount * 100_000_000.0));

        let transferResult = await icpLedger.icrc2_transfer_from({
            spender_subaccount = null;
            from = { owner = caller; subaccount = null };
            to = { owner = selfPrincipal; subaccount = null };
            amount = amountE8s;
            fee = null;
            memo = null;
            created_at_time = null;
        });

        switch (transferResult) {
            case (#Err(err)) {
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

        // Check deposit rate limit
        let currentTime = Time.now();
        let currentHour = currentTime / 3600000000000; // Convert to hours

        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) {
                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, List.nil()));
            };
            case (?timestamps) {
                let filteredTimestamps = List.filter<Int>(
                    timestamps,
                    func(timestamp) {
                        currentHour - timestamp < 1;
                    },
                );

                if (List.size(filteredTimestamps) >= 3) {
                    Debug.trap("You can only open 3 positions per hour");
                };

                depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, filteredTimestamps));
            };
        };

        // Check maximum deposit limit for simple mode only
        if (not isCompounding) {
            let maxDeposit = Float.max(platformStats.potBalance * 0.2, 5.0);
            if (amount > maxDeposit) {
                Debug.trap("Maximum deposit for simple mode is the greater of 20% of current pot balance or 5 ICP (" # formatICP(maxDeposit) # " ICP)");
            };
        };

        // Calculate dealer maintenance fee (3%)
        let dealerFee = amount * 0.03;

        // Check if caller is the only dealer
        let isOnlyDealer = isCallerOnlyDealer(caller);

        // If caller is the only dealer, credit half of the dealer fee back to their wallet
        if (isOnlyDealer) {
            let repaymentAmount = dealerFee * 0.5;
            creditDealerRepayment(caller, repaymentAmount);
        };

        let gameId = nextGameId;
        nextGameId += 1;

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
            potBalance = platformStats.potBalance + amount;
        };

        // Handle referrals
        switch (referrer) {
            case (null) {};
            case (?ref) {
                processReferral(caller, ref, amount);
            };
        };

        // Award Ponzi Points based on plan
        let points = switch (plan) {
            case (#simple21Day) { amount * 1000.0 };
            case (#compounding15Day) { amount * 2000.0 };
            case (#compounding30Day) { amount * 3000.0 };
        };
        awardPonziPoints(caller, points);

        gameId;
    };

    // Add Dealer Money (Seed Round — transfers ICP directly from user's wallet)
    public shared ({ caller }) func addDealerMoney(amount : Float) : async () {
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

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

        let transferResult = await icpLedger.icrc2_transfer_from({
            spender_subaccount = null;
            from = { owner = caller; subaccount = null };
            to = { owner = selfPrincipal; subaccount = null };
            amount = amountE8s;
            fee = null;
            memo = null;
            created_at_time = null;
        });

        switch (transferResult) {
            case (#Err(err)) {
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
    };

    // Award Ponzi Points
    func awardPonziPoints(user : Principal, points : Float) {
        switch (principalMapNat.get(ponziPoints, user)) {
            case (null) {
                ponziPoints := principalMapNat.put(ponziPoints, user, points);
            };
            case (?existingPoints) {
                ponziPoints := principalMapNat.put(ponziPoints, user, existingPoints + points);
            };
        };
    };

    // Check if caller is the only dealer
    func isCallerOnlyDealer(caller : Principal) : Bool {
        var dealerCount = 0;
        for ((principal, _) in principalMapNat.entries(dealerPositions)) {
            if (principal == caller) {
                dealerCount += 1;
            } else {
                dealerCount += 1;
            };
        };
        dealerCount == 1;
    };

    // Credit dealer repayment to caller's wallet
    func creditDealerRepayment(caller : Principal, amount : Float) {
        switch (principalMapNat.get(dealerRepayments, caller)) {
            case (null) {
                dealerRepayments := principalMapNat.put(dealerRepayments, caller, amount);
            };
            case (?existingAmount) {
                dealerRepayments := principalMapNat.put(dealerRepayments, caller, existingAmount + amount);
            };
        };
    };

    // Process Referral
    func processReferral(newUser : Principal, referrer : Principal, amount : Float) {
        // Level 1 referral
        updateReferralRecord(referrer, 1, amount, 0.10);

        // Level 2 referral
        switch (principalMapNat.get(referralRecords, referrer)) {
            case (null) {};
            case (?level1Record) {
                updateReferralRecord(level1Record.referrer, 2, amount, 0.05);

                // Level 3 referral
                switch (principalMapNat.get(referralRecords, level1Record.referrer)) {
                    case (null) {};
                    case (?level2Record) {
                        updateReferralRecord(level2Record.referrer, 3, amount, 0.03);
                    };
                };
            };
        };
    };

    // Update Referral Record
    func updateReferralRecord(referrer : Principal, level : Nat, amount : Float, percentage : Float) {
        switch (principalMapNat.get(referralRecords, referrer)) {
            case (null) {
                let newRecord : ReferralRecord = {
                    referrer;
                    level;
                    earnings = amount * percentage;
                    depositCount = 1;
                };
                referralRecords := principalMapNat.put(referralRecords, referrer, newRecord);
            };
            case (?record) {
                if (record.depositCount < 2) {
                    let updatedRecord : ReferralRecord = {
                        record with
                        earnings = record.earnings + (amount * percentage);
                        depositCount = record.depositCount + 1;
                    };
                    referralRecords := principalMapNat.put(referralRecords, referrer, updatedRecord);
                };
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
        switch (natMap.get(gameRecords, gameId)) {
            case (null) { Debug.trap("Game not found") };
            case (?game) {
                if (game.player != caller) {
                    Debug.trap("Unauthorized: Only the game owner can withdraw earnings");
                };
                if (game.isCompounding) {
                    Debug.trap("Cannot withdraw from compounding games");
                };

                let earnings = await calculateEarnings(game);
                if (earnings > platformStats.potBalance) {
                    triggerGameReset("Insufficient funds for payout");
                    Debug.trap("Game reset due to insufficient funds");
                };

                // Reset the game record: zero out accumulated earnings & reset lastUpdateTime
                let updatedGame : GameRecord = {
                    game with
                    accumulatedEarnings = 0.0;
                    lastUpdateTime = Time.now();
                    totalWithdrawn = game.totalWithdrawn + earnings;
                };
                gameRecords := natMap.put(gameRecords, gameId, updatedGame);

                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + earnings;
                    potBalance = platformStats.potBalance - earnings;
                };

                earnings;
            };
        };
    };

    // Calculate Earnings
    public query func calculateEarnings(game : GameRecord) : async Float {
        let timeElapsed = Float.fromInt((Time.now() - game.lastUpdateTime) / 1000000000); // Convert to seconds
        let dailyRate = switch (game.plan) {
            case (#simple21Day) { 0.11 }; // Updated to 11% daily rate for simple mode
            case (#compounding15Day) { 0.12 };
            case (#compounding30Day) { 0.09 };
        };
        let earnings = game.amount * dailyRate * (timeElapsed / 86400.0); // Convert seconds to days
        roundToEightDecimals(game.accumulatedEarnings + earnings);
    };

    // Calculate Compounded Earnings for 15-Day Plan
    public query func calculateCompoundedEarnings(game : GameRecord) : async Float {
        if (game.plan != #compounding15Day) {
            Debug.trap("This calculation is only for the 15-day compounding plan");
        };

        let timeElapsed = Float.fromInt((Time.now() - game.startTime) / 1000000000); // Convert to seconds
        let daysElapsed = timeElapsed / 86400.0; // Convert seconds to days

        if (daysElapsed > 15.0) {
            Debug.trap("The 15-day compounding period has ended");
        };

        let dailyRate = 0.12; // 12% daily rate for 15-day compounding
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

    // Get Referral Earnings
    public query func getReferralEarnings(user : Principal) : async Float {
        switch (principalMapNat.get(referralRecords, user)) {
            case (null) { 0.0 };
            case (?record) { record.earnings };
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

    // Get Days Active
    public query func getDaysActive() : async Nat {
        Int.abs((Time.now() - 0) / 86400000000000);
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

        // Calculate referral points (from referral earnings)
        let referralPoints = switch (principalMapNat.get(referralRecords, caller)) {
            case (null) { 0.0 };
            case (?record) { record.earnings };
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
        var level1Points = 0.0;
        var level2Points = 0.0;
        var level3Points = 0.0;

        // Calculate points for each referral level
        for (record in principalMapNat.vals(referralRecords)) {
            if (record.referrer == caller) {
                switch (record.level) {
                    case (1) { level1Points += record.earnings };
                    case (2) { level2Points += record.earnings };
                    case (3) { level3Points += record.earnings };
                    case (_) {};
                };
            };
        };

        let totalPoints = level1Points + level2Points + level3Points;

        {
            level1Points;
            level2Points;
            level3Points;
            totalPoints;
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

    public shared func getPonziPointsBalanceFor(user : Principal) : async Float {
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

    // Distribute Fees to Dealers
    public shared func distributeFees(totalFees : Float) : async () {
        let dealerRepaymentAmount = totalFees * 0.5; // 50% of fees earmarked for dealer repayment

        // Get all dealers
        let allDealers = Iter.toArray(principalMapNat.vals(dealerPositions));
        let upstreamDealers = List.toArray(
            List.filter(
                List.fromArray(allDealers),
                func(dealer : DealerPosition) : Bool {
                    dealer.dealerType == #upstream;
                },
            )
        );

        // Find oldest upstream dealer
        var oldestDealer : ?DealerPosition = null;
        var oldestTime : Int = 0;
        for (dealer in upstreamDealers.vals()) {
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

        // 35% to oldest upstream dealer
        switch (oldestDealer) {
            case (null) {};
            case (?dealer) {
                let amount = dealerRepaymentAmount * 0.35;
                creditDealerRepayment(dealer.owner, amount);
            };
        };

        // 25% split among other upstream dealers
        let otherUpstreamDealers = List.toArray(
            List.filter(
                List.fromArray(upstreamDealers),
                func(dealer : DealerPosition) : Bool {
                    switch (oldestDealer) {
                        case (null) { true };
                        case (?oldest) { dealer.owner != oldest.owner };
                    };
                },
            )
        );
        if (otherUpstreamDealers.size() > 0) {
            let amount = dealerRepaymentAmount * 0.25 / Float.fromInt(otherUpstreamDealers.size());
            for (dealer in otherUpstreamDealers.vals()) {
                creditDealerRepayment(dealer.owner, amount);
            };
        };

        // 40% split among all dealers
        if (allDealers.size() > 0) {
            let amount = dealerRepaymentAmount * 0.4 / Float.fromInt(allDealers.size());
            for (dealer in allDealers.vals()) {
                creditDealerRepayment(dealer.owner, amount);
            };
        };
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

