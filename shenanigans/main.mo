import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Time "mo:base/Time";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import List "mo:base/List";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Blob "mo:base/Blob";
import Error "mo:base/Error";
import Timer "mo:base/Timer";
import Array "mo:base/Array";
import Text "mo:base/Text";

import PpLedger "PpLedger";
import Subaccount "Subaccount";

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

    /// A queued cash-out. Stays in the chip subaccount until claimed so
    /// hostile spells can still drain it during the delay window.
    public type CashOutEntry = {
        id : Nat;
        player : Principal;
        amount : Nat;              // PP-units requested
        claimableAfter : Int;      // nanoseconds (Time.now() + delay)
        claimed : Bool;            // set true after claimCashOut succeeds
    };

    /// Mutable mint + economy configuration. All fields admin-tunable.
    public type MintConfig = {
        simple21DayPpPerIcp : Nat;    // initial 1000 (whole PP per ICP)
        compounding15DayPpPerIcp : Nat; // initial 2000
        compounding30DayPpPerIcp : Nat; // initial 3000
        dealerPpPerIcp : Nat;          // initial 4000
        referralL1Bps : Nat;           // basis points; initial 800 (= 8%)
        referralL2Bps : Nat;           // initial 500
        referralL3Bps : Nat;           // initial 200
        minDepositPp : Nat;            // initial 5000 (whole PP)
        cashOutDelaySeconds : Nat;     // initial 604_800
        observerIntervalSeconds : Nat; // initial 10
    };

    /// Per-dealer cumulative ICP seen by the observer. Used to mint only
    /// on deltas when dealers top up.
    public type DealerSeen = Float;

    // ================================================================
    // Backend canister interface (query-only; observer polls + referral lookups)
    // ================================================================

    type BackendGamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    type BackendGameRecord = {
        id : Nat;
        player : Principal;
        plan : BackendGamePlan;
        amount : Float;
        startTime : Int;
        isCompounding : Bool;
        isActive : Bool;
        lastUpdateTime : Int;
        accumulatedEarnings : Float;
        totalWithdrawn : Float;
    };

    type BackendDealerType = { #upstream; #downstream };

    type BackendDealerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        name : Text;
        dealerType : BackendDealerType;
        firstDepositDate : ?Int;
    };

    type BackendActor = actor {
        getAllGames : shared query () -> async [BackendGameRecord];
        getDealerPositions : shared query () -> async [BackendDealerPosition];
        getReferrer : shared query (Principal) -> async ?Principal;
    };

    // ================================================================
    // State
    // ================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);

    // Spell configs — PRESERVED across migration (admin-tunable spell definitions)
    var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Spell cast history — reset at migration; bounded to last 500 entries
    var shenanigans = natMap.empty<ShenaniganRecord>();
    var shenaniganStats = principalMap.empty<ShenaniganStats>();
    var nextShenaniganId : Nat = 0;

    // Admin state
    var adminPrincipal : ?Principal = null;
    var backendPrincipal : ?Principal = null;

    // Mint + economy configuration (mutable, admin-tunable)
    var mintConfig : MintConfig = {
        simple21DayPpPerIcp = 1000;
        compounding15DayPpPerIcp = 2000;
        compounding30DayPpPerIcp = 3000;
        dealerPpPerIcp = 4000;
        referralL1Bps = 800;
        referralL2Bps = 500;
        referralL3Bps = 200;
        minDepositPp = 5000;
        cashOutDelaySeconds = 604_800;
        observerIntervalSeconds = 10;
    };

    // Observer cursors
    var gameIdCursor : Nat = 0;                         // next unprocessed game id
    var dealerSeen = principalMap.empty<DealerSeen>();  // cumulative ICP minted-for per dealer

    // Observer lock to prevent concurrent ticks
    transient var observerRunning : Bool = false;
    var observerTimerId : ?Timer.TimerId = null;

    // Cash-out queue
    var cashOuts = natMap.empty<CashOutEntry>();
    var nextCashOutId : Nat = 0;

    // Leaderboard (local state — not derived from ledger)
    var ppBurnedPerPlayer = principalMap.empty<Nat>();  // cumulative PP units burned
    var spellsCastPerPlayer = principalMap.empty<Nat>(); // successful casts only

    // PP ledger actor reference
    transient let ppLedger : PpLedger.LedgerActor = actor (PpLedger.PP_LEDGER_CANISTER_ID);

    // ================================================================
    // Initialization
    // ================================================================

    public shared ({ caller }) func initialize(backendCanisterId : Principal) : async () {
        switch (adminPrincipal) {
            case (null) {
                adminPrincipal := ?caller;
                backendPrincipal := ?backendCanisterId;
                if (natMap.size(shenaniganConfigs) == 0) {
                    initializeDefaultShenanigans();
                };
                startObserver();
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
                if (caller != admin) { Debug.trap("Unauthorized: admin only") };
            };
        };
    };

    public shared ({ caller }) func rotateAdmin(newAdmin : Principal) : async () {
        requireAdmin(caller);
        adminPrincipal := ?newAdmin;
    };

    func getBackend() : BackendActor {
        switch (backendPrincipal) {
            case (null) { Debug.trap("Backend canister not configured") };
            case (?p) { actor (Principal.toText(p)) : BackendActor };
        };
    };

    // Implemented in Task 8; stub here so Task 5 scaffolding compiles.
    func startObserver() {
        // Implemented in Task 8
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

    public shared ({ caller = _ }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        ignore shenaniganType;
        ignore target;
        Debug.trap("Migration in progress — castShenanigan will be restored in Task 11");
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
