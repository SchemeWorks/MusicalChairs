import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Time "mo:base/Time";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import List "mo:base/List";
import Nat "mo:base/Nat";

persistent actor {

    // ================================================================
    // Types
    // ================================================================

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

    public type ShenaniganOutcome = {
        #success;
        #fail;
        #backfire;
    };

    public type ShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : ShenaniganType;
        target : ?Principal;
        outcome : ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
    };

    public type ShenaniganStats = {
        totalSpent : Float;
        totalCast : Nat;
        goodOutcomes : Nat;
        badOutcomes : Nat;
        backfires : Nat;
        dealerCut : Float;
    };

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

    // ================================================================
    // Backend canister interface (for cross-canister calls)
    // Inter-canister calls must be update calls, not queries.
    // ================================================================

    type BackendActor = actor {
        deductPonziPoints : shared (user : Principal, amount : Float) -> async ();
        transferPonziPoints : shared (from : Principal, to : Principal, amount : Float) -> async ();
        distributeDealerCutFromShenanigans : shared (amount : Float) -> async ();
        getPonziPointsBalanceFor : shared (user : Principal) -> async Float;
        burnPonziPoints : shared (user : Principal, amount : Float) -> async ();
    };

    // ================================================================
    // State
    // ================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);

    var shenanigans = natMap.empty<ShenaniganRecord>();
    var shenaniganStats = principalMap.empty<ShenaniganStats>();
    var nextShenaniganId = 0;
    var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Admin state
    var adminPrincipal : ?Principal = null;

    // Backend canister principal (set by admin after deployment)
    var backendPrincipal : ?Principal = null;

    // ================================================================
    // Initialization
    // ================================================================

    public shared ({ caller }) func initialize(backendCanisterId : Principal) : async () {
        switch (adminPrincipal) {
            case (null) {
                adminPrincipal := ?caller;
                backendPrincipal := ?backendCanisterId;
                initializeDefaultShenanigans();
            };
            case (?admin) {
                if (caller != admin) {
                    Debug.trap("Already initialized. Only admin can reconfigure.");
                };
                backendPrincipal := ?backendCanisterId;
            };
        };
    };

    func requireAdmin(caller : Principal) {
        switch (adminPrincipal) {
            case (null) { Debug.trap("Not initialized") };
            case (?admin) {
                if (caller != admin) {
                    Debug.trap("Unauthorized: admin only");
                };
            };
        };
    };

    func getBackend() : BackendActor {
        switch (backendPrincipal) {
            case (null) { Debug.trap("Backend canister not configured") };
            case (?p) { actor (Principal.toText(p)) : BackendActor };
        };
    };

    // ================================================================
    // Default configs (identical to current backend)
    // ================================================================

    func initializeDefaultShenanigans() {
        let defaultConfigs : [ShenaniganConfig] = [
            { id = 0; name = "Money Trickster"; description = "Steals 2\u{2013}8% of target's Ponzi Points (max 250 PP)."; cost = 120.0; successOdds = 60; failureOdds = 25; backfireOdds = 15; duration = 0; cooldown = 2; effectValues = [2.0, 8.0, 250.0]; castLimit = 0; backgroundColor = "#fff9e6" },
            { id = 1; name = "AOE Skim"; description = "Siphons 1\u{2013}3% from each player (max 60 PP per player)."; cost = 600.0; successOdds = 40; failureOdds = 40; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [1.0, 3.0, 60.0]; castLimit = 1; backgroundColor = "#e6f7ff" },
            { id = 2; name = "Rename Spell"; description = "Changes target's display name for 7 days."; cost = 200.0; successOdds = 90; failureOdds = 5; backfireOdds = 5; duration = 168; cooldown = 0; effectValues = [7.0]; castLimit = 0; backgroundColor = "#ffe6f7" },
            { id = 3; name = "Mint Tax Siphon"; description = "Skims 5% of target's new PP for 7 days (max 1000 PP)."; cost = 1200.0; successOdds = 70; failureOdds = 20; backfireOdds = 10; duration = 168; cooldown = 0; effectValues = [5.0, 1000.0]; castLimit = 0; backgroundColor = "#f3e6ff" },
            { id = 4; name = "Downline Heist"; description = "Steals one downline member (favor L3)."; cost = 500.0; successOdds = 30; failureOdds = 60; backfireOdds = 10; duration = 0; cooldown = 0; effectValues = []; castLimit = 1; backgroundColor = "#e6fff2" },
            { id = 5; name = "Magic Mirror"; description = "Equips shield (blocks one hostile shenanigan)."; cost = 200.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = []; castLimit = 2; backgroundColor = "#fff4e6" },
            { id = 6; name = "PP Booster Aura"; description = "Earn +5\u{2013}15% additional PP for rest of round."; cost = 300.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [5.0, 15.0]; castLimit = 1; backgroundColor = "#e6f2ff" },
            { id = 7; name = "Purse Cutter"; description = "Target loses 25\u{2013}50% PP (max 800 PP)."; cost = 900.0; successOdds = 20; failureOdds = 50; backfireOdds = 30; duration = 0; cooldown = 0; effectValues = [25.0, 50.0, 800.0]; castLimit = 0; backgroundColor = "#ffe6e6" },
            { id = 8; name = "Whale Rebalance"; description = "Takes 20% from top 3 holders (max 300 PP/whale)."; cost = 800.0; successOdds = 50; failureOdds = 30; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [20.0, 300.0]; castLimit = 0; backgroundColor = "#f0e6ff" },
            { id = 9; name = "Downline Boost"; description = "Downline referrals kick up 1.3x PP for rest of round."; cost = 400.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [1.3]; castLimit = 1; backgroundColor = "#e6fffa" },
            { id = 10; name = "Golden Name"; description = "Gives gold name on leaderboard (24h or 7d)."; cost = 100.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 24; cooldown = 0; effectValues = [24.0, 168.0]; castLimit = 1; backgroundColor = "#fff0e6" },
        ];
        for (config in defaultConfigs.vals()) {
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };

    // ================================================================
    // Core Logic
    // ================================================================

    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        let backend = getBackend();

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

        // Check balance via cross-canister call
        let userPoints = await backend.getPonziPointsBalanceFor(caller);
        if (userPoints < cost) {
            Debug.trap("Insufficient Ponzi Points to cast this shenanigan");
        };

        // Deduct cost
        await backend.deductPonziPoints(caller, cost);

        // Track burn
        await backend.burnPonziPoints(caller, cost);

        // Determine outcome
        let outcome = determineOutcome(shenaniganType);

        // Apply backfire effects
        if (outcome == #backfire) {
            switch (shenaniganType) {
                case (#moneyTrickster) {
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            let casterPoints = await backend.getPonziPointsBalanceFor(caller);
                            let lossPercentage = 0.02 + (Float.fromInt(Int.abs(Time.now()) % 7) / 100.0);
                            let lossAmount = casterPoints * lossPercentage;
                            let cappedLoss = Float.min(lossAmount, 250.0);
                            await backend.transferPonziPoints(caller, targetPrincipal, cappedLoss);
                        };
                    };
                };
                case (#aoeSkim) {
                    let casterPoints = await backend.getPonziPointsBalanceFor(caller);
                    let lossPercentage = 0.01 + (Float.fromInt(Int.abs(Time.now()) % 3) / 100.0);
                    let lossAmount = casterPoints * lossPercentage;
                    // Simplified: deduct from caster instead of distributing to all players
                    // (cross-canister iteration of all PP holders is complex)
                    await backend.deductPonziPoints(caller, lossAmount);
                };
                case (#downlineHeist) {
                    switch (target) {
                        case (null) {};
                        case (?targetPrincipal) {
                            Debug.print("Backfire: " # Principal.toText(caller) # " loses L3 downline to " # Principal.toText(targetPrincipal));
                        };
                    };
                };
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

        // Update stats
        updateShenaniganStats(caller, cost, outcome);

        // Dealer cut (10% of cost)
        let dealerCut = cost * 0.1;
        await backend.distributeDealerCutFromShenanigans(dealerCut);

        outcome;
    };

    func determineOutcome(shenaniganType : ShenaniganType) : ShenaniganOutcome {
        let randomValue = Int.abs(Time.now()) % 100;
        switch (shenaniganType) {
            case (#moneyTrickster) { if (randomValue < 60) #success else if (randomValue < 85) #fail else #backfire };
            case (#aoeSkim) { if (randomValue < 40) #success else if (randomValue < 80) #fail else #backfire };
            case (#renameSpell) { if (randomValue < 90) #success else if (randomValue < 95) #fail else #backfire };
            case (#mintTaxSiphon) { if (randomValue < 70) #success else if (randomValue < 90) #fail else #backfire };
            case (#downlineHeist) { if (randomValue < 30) #success else if (randomValue < 90) #fail else #backfire };
            case (#magicMirror) { #success };
            case (#ppBoosterAura) { #success };
            case (#purseCutter) { if (randomValue < 20) #success else if (randomValue < 70) #fail else #backfire };
            case (#whaleRebalance) { if (randomValue < 50) #success else if (randomValue < 80) #fail else #backfire };
            case (#downlineBoost) { #success };
            case (#goldenName) { #success };
        };
    };

    func updateShenaniganStats(user : Principal, cost : Float, outcome : ShenaniganOutcome) {
        let currentStats = switch (principalMap.get(shenaniganStats, user)) {
            case (null) { { totalSpent = 0.0; totalCast = 0; goodOutcomes = 0; badOutcomes = 0; backfires = 0; dealerCut = 0.0 } };
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
        shenaniganStats := principalMap.put(shenaniganStats, user, updatedStats);
    };

    // ================================================================
    // Query Functions
    // ================================================================

    public query ({ caller }) func getShenaniganStats() : async ShenaniganStats {
        switch (principalMap.get(shenaniganStats, caller)) {
            case (null) { { totalSpent = 0.0; totalCast = 0; goodOutcomes = 0; badOutcomes = 0; backfires = 0; dealerCut = 0.0 } };
            case (?stats) { stats };
        };
    };

    public query func getRecentShenanigans() : async [ShenaniganRecord] {
        let allShenanigans = Iter.toArray(natMap.vals(shenanigans));
        let sorted = List.fromArray(allShenanigans);
        let recent = List.take(sorted, 12);
        List.toArray(recent);
    };

    public query func getShenaniganConfigs() : async [ShenaniganConfig] {
        Iter.toArray(natMap.vals(shenaniganConfigs));
    };

    // ================================================================
    // Admin Functions
    // ================================================================

    public shared ({ caller }) func updateShenaniganConfig(config : ShenaniganConfig) : async () {
        requireAdmin(caller);
        if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
            Debug.trap("Success, failure, and backfire odds must sum to 100");
        };
        if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
            Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
        };
        shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
    };

    public shared ({ caller }) func resetShenaniganConfig(id : Nat) : async () {
        requireAdmin(caller);
        initializeDefaultShenanigans();
    };

    public shared ({ caller }) func saveAllShenaniganConfigs(configs : [ShenaniganConfig]) : async () {
        requireAdmin(caller);
        for (config in configs.vals()) {
            if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
                Debug.trap("Success, failure, and backfire odds must sum to 100");
            };
            if (config.cost < 0.0 or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
                Debug.trap("Cost, duration, cooldown, and cast limit must be non-negative");
            };
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };
};
