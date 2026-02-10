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

persistent actor {
    // Access Control State
    transient let accessControlState = AccessControl.initState();

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
    transient var userProfiles = principalMap.empty<UserProfile>();

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

    // Shenanigan Types
    public type ShenaniganType = {
        #moneyTrickster;
        #aoeSkim;
        #renameSpell;
        #mintTaxSiphon;
        #downlineHeist;
        #magicMirror;
        #ppBoosterAura;
        #purseCutter;
        #whaleRebalance;
        #downlineBoost;
        #goldenName;
    };

    // Shenanigan Outcome Types
    public type ShenaniganOutcome = {
        #success;
        #fail;
        #backfire;
    };

    // Shenanigan Record
    public type ShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : ShenaniganType;
        target : ?Principal;
        outcome : ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
    };

    // Shenanigan Stats
    public type ShenaniganStats = {
        totalSpent : Float;
        totalCast : Nat;
        goodOutcomes : Nat;
        badOutcomes : Nat;
        backfires : Nat;
        dealerCut : Float;
    };

    // Shenanigan Config
    public type ShenaniganConfig = {
        id : Nat;
        name : Text;
        description : Text;
        cost : Float;
        successOdds : Nat;
        failureOdds : Nat;
        backfireOdds : Nat;
        duration : Nat;
        cooldown : Nat;
        effectValues : [Float];
        castLimit : Nat;
        backgroundColor : Text;
    };

    // Initialize OrderedMaps
    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMapNat = OrderedMap.Make<Principal>(Principal.compare);
    transient let intMap = OrderedMap.Make<Int>(Int.compare);

    transient var gameRecords = natMap.empty<GameRecord>();
    transient var referralRecords = principalMapNat.empty<ReferralRecord>();
    transient var platformStats : PlatformStats = {
        totalDeposits = 0.0;
        totalWithdrawals = 0.0;
        activeGames = 0;
        potBalance = 0.0;
        daysActive = 0;
    };
    transient var gameResetHistory = intMap.empty<GameResetRecord>();
    transient var nextGameId = 0;

    // Deposit Rate Limiting
    transient var depositTimestamps = principalMapNat.empty<List.List<Int>>();

    // Dealer Repayment Tracking
    transient var dealerRepayments = principalMapNat.empty<Float>();

    // Dealer Positions
    transient var dealerPositions = principalMapNat.empty<DealerPosition>();

    // Ponzi Points Tracking
    transient var ponziPoints = principalMapNat.empty<Float>();

    // Shenanigans Tracking
    transient var shenanigans = natMap.empty<ShenaniganRecord>();
    transient var shenaniganStats = principalMapNat.empty<ShenaniganStats>();
    transient var nextShenaniganId = 0;

    // Ponzi Points Burned Tracking
    transient var ponziPointsBurned = principalMapNat.empty<Float>();

    // ========================================================================
    // Musical Chairs Wallet System (Real ICP Integration)
    // ========================================================================
    
    // User wallet balances (in e8s - 1 ICP = 100_000_000 e8s)
    transient var walletBalances = principalMapNat.empty<Nat>();
    
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
    transient var walletTransactions = natMap.empty<WalletTransaction>();
    transient var nextWalletTxId = 0;
    
    // Test mode flag - when true, gives users 500 fake ICP for testing
    transient var testMode : Bool = true;
    
    // This canister's ID (set during init or known from dfx.json)
    // For local: uxrrr-q7777-77774-qaaaq-cai
    // This will be updated when deployed to mainnet
    stable var canisterPrincipal : ?Principal = null;
    
    // ICP Ledger actor reference (mainnet)
    let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
    
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

    transient var houseLedger = natMap.empty<HouseLedgerRecord>();
    transient var nextHouseLedgerId = 0;

    // Shenanigan Configurations
    transient var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Initialize default shenanigan configs
    func initializeDefaultShenanigans() {
        let defaultConfigs : [ShenaniganConfig] = [
            {
                id = 0;
                name = "Money Trickster";
                description = "Steals 2–8% of target's Ponzi Points (max 250 PP).";
                cost = 120.0;
                successOdds = 60;
                failureOdds = 25;
                backfireOdds = 15;
                duration = 0;
                cooldown = 2;
                effectValues = [2.0, 8.0, 250.0];
                castLimit = 0;
                backgroundColor = "#fff9e6";
            },
            {
                id = 1;
                name = "AOE Skim";
                description = "Siphons 1–3% from each player (max 60 PP per player).";
                cost = 600.0;
                successOdds = 40;
                failureOdds = 40;
                backfireOdds = 20;
                duration = 0;
                cooldown = 0;
                effectValues = [1.0, 3.0, 60.0];
                castLimit = 1;
                backgroundColor = "#e6f7ff";
            },
            {
                id = 2;
                name = "Rename Spell";
                description = "Changes target's display name for 7 days.";
                cost = 200.0;
                successOdds = 90;
                failureOdds = 5;
                backfireOdds = 5;
                duration = 168;
                cooldown = 0;
                effectValues = [7.0];
                castLimit = 0;
                backgroundColor = "#ffe6f7";
            },
            {
                id = 3;
                name = "Mint Tax Siphon";
                description = "Skims 5% of target's new PP for 7 days (max 1000 PP).";
                cost = 1200.0;
                successOdds = 70;
                failureOdds = 20;
                backfireOdds = 10;
                duration = 168;
                cooldown = 0;
                effectValues = [5.0, 1000.0];
                castLimit = 0;
                backgroundColor = "#f3e6ff";
            },
            {
                id = 4;
                name = "Downline Heist";
                description = "Steals one downline member (favor L3).";
                cost = 500.0;
                successOdds = 30;
                failureOdds = 60;
                backfireOdds = 10;
                duration = 0;
                cooldown = 0;
                effectValues = [];
                castLimit = 1;
                backgroundColor = "#e6fff2";
            },
            {
                id = 5;
                name = "Magic Mirror";
                description = "Equips shield (blocks one hostile shenanigan).";
                cost = 200.0;
                successOdds = 100;
                failureOdds = 0;
                backfireOdds = 0;
                duration = 0;
                cooldown = 0;
                effectValues = [];
                castLimit = 2;
                backgroundColor = "#fff4e6";
            },
            {
                id = 6;
                name = "PP Booster Aura";
                description = "Earn +5–15% additional PP for rest of round.";
                cost = 300.0;
                successOdds = 100;
                failureOdds = 0;
                backfireOdds = 0;
                duration = 0;
                cooldown = 0;
                effectValues = [5.0, 15.0];
                castLimit = 1;
                backgroundColor = "#e6f2ff";
            },
            {
                id = 7;
                name = "Purse Cutter";
                description = "Target loses 25–50% PP (max 800 PP).";
                cost = 900.0;
                successOdds = 20;
                failureOdds = 50;
                backfireOdds = 30;
                duration = 0;
                cooldown = 0;
                effectValues = [25.0, 50.0, 800.0];
                castLimit = 0;
                backgroundColor = "#ffe6e6";
            },
            {
                id = 8;
                name = "Whale Rebalance";
                description = "Takes 20% from top 3 holders (max 300 PP/whale).";
                cost = 800.0;
                successOdds = 50;
                failureOdds = 30;
                backfireOdds = 20;
                duration = 0;
                cooldown = 0;
                effectValues = [20.0, 300.0];
                castLimit = 0;
                backgroundColor = "#f0e6ff";
            },
            {
                id = 9;
                name = "Downline Boost";
                description = "Downline referrals kick up 1.3x PP for rest of round.";
                cost = 400.0;
                successOdds = 100;
                failureOdds = 0;
                backfireOdds = 0;
                duration = 0;
                cooldown = 0;
                effectValues = [1.3];
                castLimit = 1;
                backgroundColor = "#e6fffa";
            },
            {
                id = 10;
                name = "Golden Name";
                description = "Gives gold name on leaderboard (24h or 7d).";
                cost = 100.0;
                successOdds = 100;
                failureOdds = 0;
                backfireOdds = 0;
                duration = 24;
                cooldown = 0;
                effectValues = [24.0, 168.0];
                castLimit = 1;
                backgroundColor = "#fff0e6";
            },
        ];

        for (config in defaultConfigs.vals()) {
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };

    // Initialize default shenanigans on first deployment
    initializeDefaultShenanigans();

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

        // Update dealer entitlement for the admin
        let entitlement = amount * 1.12; // 12% bonus

        // Get the admin's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Unknown Dealer" };
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

    // Add Dealer Money
    public shared ({ caller }) func addDealerMoney(amount : Float) : async () {
        if (amount < 0.1) {
            Debug.trap("Minimum deposit is 0.1 ICP");
        };

        // Validate 8 decimal places
        if (not validateEightDecimals(amount)) {
            Debug.trap("Amount cannot have more than 8 decimal places");
        };

        let entitlement = amount * 1.12; // 12% bonus

        // Get the user's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Unknown Dealer" };
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
            daysActive = Int.abs((Time.now() - 0) / 86400000000000);
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

    // Cast Shenanigan
    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        let cost = switch (shenaniganType) {
            case (#moneyTrickster) { 120.0 };
            case (#aoeSkim) { 600.0 };
            case (#renameSpell) { 200.0 };
            case (#mintTaxSiphon) { 1200.0 };
            case (#downlineHeist) { 500.0 };
            case (#magicMirror) { 200.0 };
            case (#ppBoosterAura) { 300.0 };
            case (#purseCutter) { 900.0 };
            case (#whaleRebalance) { 800.0 };
            case (#downlineBoost) { 400.0 };
            case (#goldenName) { 100.0 };
        };

        // Check if user has enough Ponzi Points
        let userPoints = switch (principalMapNat.get(ponziPoints, caller)) {
            case (null) { 0.0 };
            case (?points) { points };
        };

        if (userPoints < cost) {
            Debug.trap("Insufficient Ponzi Points to cast this shenanigan");
        };

        // Deduct Ponzi Points
        ponziPoints := principalMapNat.put(ponziPoints, caller, userPoints - cost);

        // Track Ponzi Points burned
        let burned = switch (principalMapNat.get(ponziPointsBurned, caller)) {
            case (null) { 0.0 };
            case (?existingBurned) { existingBurned };
        };
        ponziPointsBurned := principalMapNat.put(ponziPointsBurned, caller, burned + cost);

        // Determine outcome
        let outcome = determineOutcome(shenaniganType);

        // Apply backfire effect if outcome is backfire
        if (outcome == #backfire) {
            switch (shenaniganType) {
                case (#moneyTrickster) {
                    // If backfire, caster loses 2–8% of their own PP to the target
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            let casterPoints = switch (principalMapNat.get(ponziPoints, caller)) {
                                case (null) { 0.0 };
                                case (?points) { points };
                            };
                            let lossPercentage = 0.02 + (Float.fromInt(Int.abs(Time.now()) % 7) / 100.0); // 2–8%
                            let lossAmount = casterPoints * lossPercentage;
                            let cappedLoss = Float.min(lossAmount, 250.0);

                            // Deduct from caster
                            ponziPoints := principalMapNat.put(ponziPoints, caller, casterPoints - cappedLoss);

                            // Add to target
                            let targetPoints = switch (principalMapNat.get(ponziPoints, targetPrincipal)) {
                                case (null) { cappedLoss };
                                case (?points) { points + cappedLoss };
                            };
                            ponziPoints := principalMapNat.put(ponziPoints, targetPrincipal, targetPoints);
                        };
                    };
                };
                case (#aoeSkim) {
                    // If backfire, caster loses 1–3% of their PP, distributed to all other players
                    let casterPoints = switch (principalMapNat.get(ponziPoints, caller)) {
                        case (null) { 0.0 };
                        case (?points) { points };
                    };
                    let lossPercentage = 0.01 + (Float.fromInt(Int.abs(Time.now()) % 3) / 100.0); // 1–3%
                    let lossAmount = casterPoints * lossPercentage;

                    // Deduct from caster
                    ponziPoints := principalMapNat.put(ponziPoints, caller, casterPoints - lossAmount);

                    // Distribute to all other players
                    let allPlayers = Iter.toArray(principalMapNat.entries(ponziPoints));
                    let otherPlayers = List.toArray(
                        List.filter(
                            List.fromArray(allPlayers),
                            func(entry : (Principal, Float)) : Bool {
                                entry.0 != caller;
                            },
                        )
                    );
                    if (otherPlayers.size() > 0) {
                        let perPlayerAmount = lossAmount / Float.fromInt(otherPlayers.size());
                        for (entry in otherPlayers.vals()) {
                            let (player, points) = entry;
                            ponziPoints := principalMapNat.put(ponziPoints, player, points + perPlayerAmount);
                        };
                    };
                };
                case (#downlineHeist) {
                    // If backfire, caster loses an L3 downline member to the target
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            // Remove L3 downline from caster and add to target
                            // This is a simplified implementation; actual downline management would be more complex
                            // For now, just log the action
                            Debug.print("Backfire: " # Principal.toText(caller) # " loses an L3 downline member to " # Principal.toText(targetPrincipal));
                        };
                    };
                };
                // Add similar backfire logic for other shenanigans as needed
                case (_) {};
            };
        };

        // Record shenanigan
        let shenaniganId = nextShenaniganId;
        nextShenaniganId += 1;

        let newShenanigan : ShenaniganRecord = {
            id = shenaniganId;
            user = caller;
            shenaniganType;
            target;
            outcome;
            timestamp = Time.now();
            cost;
        };

        shenanigans := natMap.put(shenanigans, shenaniganId, newShenanigan);

        // Update user stats
        updateShenaniganStats(caller, cost, outcome);

        // Update dealer cut (10% of cost goes to dealers)
        let dealerCut = cost * 0.1;
        updateDealerCut(dealerCut);

        outcome;
    };

    // Determine outcome based on shenanigan type
    func determineOutcome(shenaniganType : ShenaniganType) : ShenaniganOutcome {
        let randomValue = Int.abs(Time.now()) % 100;

        switch (shenaniganType) {
            case (#moneyTrickster) {
                if (randomValue < 60) { #success } // 60% success
                else if (randomValue < 85) { #fail } // 25% fail
                else { #backfire }; // 15% backfire
            };
            case (#aoeSkim) {
                if (randomValue < 40) { #success } // 40% success
                else if (randomValue < 80) { #fail } // 40% fail
                else { #backfire }; // 20% backfire
            };
            case (#renameSpell) {
                if (randomValue < 90) { #success } // 90% success
                else if (randomValue < 95) { #fail } // 5% fail
                else { #backfire }; // 5% backfire
            };
            case (#mintTaxSiphon) {
                if (randomValue < 70) { #success } // 70% success
                else if (randomValue < 90) { #fail } // 20% fail
                else { #backfire }; // 10% backfire
            };
            case (#downlineHeist) {
                if (randomValue < 30) { #success } // 30% success
                else if (randomValue < 50) { #fail } // 20% fail
                else if (randomValue < 60) { #fail } // 10% fail
                else if (randomValue < 90) { #fail } // 30% fail
                else { #backfire }; // 10% backfire
            };
            case (#magicMirror) { #success }; // 100% success
            case (#ppBoosterAura) { #success }; // 100% success
            case (#purseCutter) {
                if (randomValue < 20) { #success } // 20% success
                else if (randomValue < 70) { #fail } // 50% fail
                else { #backfire }; // 30% backfire
            };
            case (#whaleRebalance) {
                if (randomValue < 50) { #success } // 50% success
                else if (randomValue < 80) { #fail } // 30% fail
                else { #backfire }; // 20% backfire
            };
            case (#downlineBoost) { #success }; // 100% success
            case (#goldenName) { #success }; // 100% success
        };
    };

    // Update user shenanigan stats
    func updateShenaniganStats(user : Principal, cost : Float, outcome : ShenaniganOutcome) {
        let currentStats = switch (principalMapNat.get(shenaniganStats, user)) {
            case (null) {
                {
                    totalSpent = 0.0;
                    totalCast = 0;
                    goodOutcomes = 0;
                    badOutcomes = 0;
                    backfires = 0;
                    dealerCut = 0.0;
                };
            };
            case (?stats) { stats };
        };

        let updatedStats = {
            currentStats with
            totalSpent = currentStats.totalSpent + cost;
            totalCast = currentStats.totalCast + 1;
            goodOutcomes = currentStats.goodOutcomes + (if (outcome == #success) 1 else 0);
            badOutcomes = currentStats.badOutcomes + (if (outcome == #fail) 1 else 0);
            backfires = currentStats.backfires + (if (outcome == #backfire) 1 else 0);
            dealerCut = currentStats.dealerCut + (cost * 0.1);
        };

        shenaniganStats := principalMapNat.put(shenaniganStats, user, updatedStats);
    };

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

    // Get user shenanigan stats
    public query ({ caller }) func getShenaniganStats() : async ShenaniganStats {
        switch (principalMapNat.get(shenaniganStats, caller)) {
            case (null) {
                {
                    totalSpent = 0.0;
                    totalCast = 0;
                    goodOutcomes = 0;
                    badOutcomes = 0;
                    backfires = 0;
                    dealerCut = 0.0;
                };
            };
            case (?stats) { stats };
        };
    };

    // Get recent shenanigans (last 12)
    public query func getRecentShenanigans() : async [ShenaniganRecord] {
        let allShenanigans = Iter.toArray(natMap.vals(shenanigans));
        let sorted = List.fromArray(allShenanigans);
        let recent = List.take(sorted, 12);
        List.toArray(recent);
    };

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

        let entitlement = underwaterAmount * 1.12; // 12% bonus on underwater amount

        // Get the user's name from their profile
        let name = switch (principalMap.get(userProfiles, caller)) {
            case (null) { "Unknown Dealer" };
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

    // Get Shenanigan Configurations
    public query func getShenaniganConfigs() : async [ShenaniganConfig] {
        Iter.toArray(natMap.vals(shenaniganConfigs));
    };

    // Update Shenanigan Config
    public shared ({ caller }) func updateShenaniganConfig(config : ShenaniganConfig) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can update shenanigan configs");
        };

        // Validate odds sum to 100
        if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
            Debug.trap("Success, failure, and backfire odds must sum to 100");
        };

        // Validate non-negative values
        if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
            Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
        };

        shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
    };

    // Reset Shenanigan Config to Default
    public shared ({ caller }) func resetShenaniganConfig(id : Nat) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can reset shenanigan configs");
        };

        switch (natMap.get(shenaniganConfigs, id)) {
            case (null) { Debug.trap("Shenanigan config not found") };
            case (?_) {
                let defaultConfigs : [ShenaniganConfig] = [
                    {
                        id = 0;
                        name = "Money Trickster";
                        description = "Steals 2–8% of target's Ponzi Points (max 250 PP).";
                        cost = 120.0;
                        successOdds = 60;
                        failureOdds = 25;
                        backfireOdds = 15;
                        duration = 0;
                        cooldown = 2;
                        effectValues = [2.0, 8.0, 250.0];
                        castLimit = 0;
                        backgroundColor = "#fff9e6";
                    },
                    {
                        id = 1;
                        name = "AOE Skim";
                        description = "Siphons 1–3% from each player (max 60 PP per player).";
                        cost = 600.0;
                        successOdds = 40;
                        failureOdds = 40;
                        backfireOdds = 20;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [1.0, 3.0, 60.0];
                        castLimit = 1;
                        backgroundColor = "#e6f7ff";
                    },
                    {
                        id = 2;
                        name = "Rename Spell";
                        description = "Changes target's display name for 7 days.";
                        cost = 200.0;
                        successOdds = 90;
                        failureOdds = 5;
                        backfireOdds = 5;
                        duration = 168;
                        cooldown = 0;
                        effectValues = [7.0];
                        castLimit = 0;
                        backgroundColor = "#ffe6f7";
                    },
                    {
                        id = 3;
                        name = "Mint Tax Siphon";
                        description = "Skims 5% of target's new PP for 7 days (max 1000 PP).";
                        cost = 1200.0;
                        successOdds = 70;
                        failureOdds = 20;
                        backfireOdds = 10;
                        duration = 168;
                        cooldown = 0;
                        effectValues = [5.0, 1000.0];
                        castLimit = 0;
                        backgroundColor = "#f3e6ff";
                    },
                    {
                        id = 4;
                        name = "Downline Heist";
                        description = "Steals one downline member (favor L3).";
                        cost = 500.0;
                        successOdds = 30;
                        failureOdds = 60;
                        backfireOdds = 10;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [];
                        castLimit = 1;
                        backgroundColor = "#e6fff2";
                    },
                    {
                        id = 5;
                        name = "Magic Mirror";
                        description = "Equips shield (blocks one hostile shenanigan).";
                        cost = 200.0;
                        successOdds = 100;
                        failureOdds = 0;
                        backfireOdds = 0;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [];
                        castLimit = 2;
                        backgroundColor = "#fff4e6";
                    },
                    {
                        id = 6;
                        name = "PP Booster Aura";
                        description = "Earn +5–15% additional PP for rest of round.";
                        cost = 300.0;
                        successOdds = 100;
                        failureOdds = 0;
                        backfireOdds = 0;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [5.0, 15.0];
                        castLimit = 1;
                        backgroundColor = "#e6f2ff";
                    },
                    {
                        id = 7;
                        name = "Purse Cutter";
                        description = "Target loses 25–50% PP (max 800 PP).";
                        cost = 900.0;
                        successOdds = 20;
                        failureOdds = 50;
                        backfireOdds = 30;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [25.0, 50.0, 800.0];
                        castLimit = 0;
                        backgroundColor = "#ffe6e6";
                    },
                    {
                        id = 8;
                        name = "Whale Rebalance";
                        description = "Takes 20% from top 3 holders (max 300 PP/whale).";
                        cost = 800.0;
                        successOdds = 50;
                        failureOdds = 30;
                        backfireOdds = 20;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [20.0, 300.0];
                        castLimit = 0;
                        backgroundColor = "#f0e6ff";
                    },
                    {
                        id = 9;
                        name = "Downline Boost";
                        description = "Downline referrals kick up 1.3x PP for rest of round.";
                        cost = 400.0;
                        successOdds = 100;
                        failureOdds = 0;
                        backfireOdds = 0;
                        duration = 0;
                        cooldown = 0;
                        effectValues = [1.3];
                        castLimit = 1;
                        backgroundColor = "#e6fffa";
                    },
                    {
                        id = 10;
                        name = "Golden Name";
                        description = "Gives gold name on leaderboard (24h or 7d).";
                        cost = 100.0;
                        successOdds = 100;
                        failureOdds = 0;
                        backfireOdds = 0;
                        duration = 24;
                        cooldown = 0;
                        effectValues = [24.0, 168.0];
                        castLimit = 1;
                        backgroundColor = "#fff0e6";
                    },
                ];

                for (config in defaultConfigs.vals()) {
                    if (config.id == id) {
                        shenaniganConfigs := natMap.put(shenaniganConfigs, id, config);
                    };
                };
            };
        };
    };

    // Save All Shenanigan Configs
    public shared ({ caller }) func saveAllShenaniganConfigs(configs : [ShenaniganConfig]) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: Only admins can save all shenanigan configs");
        };

        for (config in configs.vals()) {
            // Validate odds sum to 100
            if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
                Debug.trap("Success, failure, and backfire odds must sum to 100");
            };

            // Validate non-negative values
            if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
                Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
            };

            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };
};

