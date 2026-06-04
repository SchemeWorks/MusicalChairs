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
        #deskSale : { buyer : Principal; ppUnitsCredited : Nat; lamportsReceived : Nat; intentId : Nat };
        #deskProceedsWithdrawal : { toAddress : Text; lamports : Nat; txSig : Text };
        #deskRefund : { toAddress : Text; lamports : Nat; txSig : Text };
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
    // Cumulative LIFETIME repayment credited per backer position. Unlike
    // backerRepayments (the UNCLAIMED balance, zeroed on claim) this only ever
    // grows — it is the high-water mark that enforces the "principal + 24% then
    // close" cap. Migration-free: a new top-level stable field, so on upgrade it
    // initialises empty. Existing backers therefore start at 0 lifetime-repaid,
    // UNDER-counting what they were already paid pre-upgrade; before the cap can
    // be trusted for pre-existing mainnet backers the owner must either backfill
    // this map (adminSetBackerLifetimeRepaid) or accept a one-time reset
    // (adminClearAllBackerPositions). See the deploy runbook.
    var backerLifetimeRepaid = backerKeyMap.empty<Float>();
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

    // Series A backer-deposit intents. Parallel to pendingIntents/pendingBuyIntents
    // (the upgrade-safe "new stable var" pattern — no migration). A BackerIntent
    // means "the next matching SOL landing on this principal's deposit address
    // registers a Series A backer position", not a game. Shares nextIntentId so
    // ids are globally unique across the three intent kinds.
    public type BackerIntent = {
        id : Nat;
        principal : Principal;
        expectedAmountLamports : Nat64;
        createdAt : Int;
        expiresAt : Int;
        fulfilled : Bool;
    };
    var pendingBackerIntents = natMap.empty<BackerIntent>();

    // ===== Founder's Allocation Desk (buy PP with SOL) =====
    public type DeskTier = {
        ratePpUnitsPer0_1Sol : Nat; // PP-units per 0.1 SOL (=1e8 lamports)
        ppUnitsTotal : Nat;
        ppUnitsSold : Nat;
        ppUnitsReserved : Nat;      // held against open buy intents
    };
    var deskTiers : [DeskTier] = [];

    public type BuyReservation = {
        tierIndex : Nat;
        ppUnits : Nat;
        ratePpUnitsPer0_1Sol : Nat; // LOCKED at quote time
    };
    public type BuyIntent = {
        id : Nat;
        principal : Principal;
        reserved : [BuyReservation];
        ppUnitsReservedTotal : Nat;
        quotedLamports : Nat64;
        createdAt : Int;
        expiresAt : Int;
        fulfilled : Bool;
    };
    var pendingBuyIntents = natMap.empty<BuyIntent>();
    var nextBuyIntentId : Nat = 0;

    // Desk SOL revenue, tracked separately from the game pot (mirrors
    // coverChargeAccrualLamports). Physically lives on the pool address after
    // sweep; this counter bounds adminWithdrawDeskProceeds.
    var deskProceedsAccrualLamports : Nat64 = 0;

    transient let DESK_BUY_INTENT_TTL_NS : Int = 15 * 60 * 1_000_000_000;
    transient let PP_S : Nat = 100_000_000; // PP unit scale; 0.1 SOL = 1e8 lamports

    // Cover charge accrual in lamports. Lives on the pool address until
    // payManagementSol sweeps it.
    var coverChargeAccrualLamports : Nat64 = 0;

    // Min deposit gate — 0.01 SOL (10_000_000 lamports). Lowered from 0.05 SOL
    // for the SIWS self-serve invest flow (frontend MIN_DEPOSIT_SOL = 0.01).
    transient let MIN_DEPOSIT_LAMPORTS : Nat64 = 10_000_000;

    // Min Series A backer deposit — 0.05 SOL (50_000_000 lamports). Mirrors
    // adminRegisterSeriesABacker's 0.05 SOL floor so the self-serve and admin
    // backer paths share the same minimum.
    transient let MIN_BACKER_LAMPORTS : Nat64 = 50_000_000;

    // Self-serve Series A gate. Default OFF: prepareBackerDeposit rejects until an
    // admin flips it via adminSetSelfServeBacking. Holds self-serve Series A dark
    // after deploy until the toll-distribution economics are settled (distributeExitToll
    // pays the backer half FLAT per-head, not by stake, so self-serve + free SIWS
    // identities = Sybil dilution). Persistent var → the choice survives upgrades.
    var selfServeBackingEnabled : Bool = false;

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

    // Per-caller cooldown for user-triggered pokeMyDeposit scans (5s). Transient:
    // resetting on upgrade is harmless (worst case one extra allowed poke).
    transient let POKE_COOLDOWN_NS : Int = 5_000_000_000;
    transient var pokeTimestamps = principalMapNat.empty<Int>();

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

    // Dust threshold for the entitlement cap: a position with less than this
    // much headroom left is treated as fully repaid (CLOSED), so a float
    // residue can't keep a repaid position alive and diluting the rest forever.
    // 1e-8 SOL = 10 lamports, far below the 5_000-lamport payout floor.
    transient let BACKER_CAP_EPSILON : Float = 1.0e-8;

    func lifetimeRepaidOf(key : BackerKey) : Float {
        switch (backerKeyMap.get(backerLifetimeRepaid, key)) {
            case (null) { 0.0 };
            case (?v) { v };
        };
    };

    // Headroom left under a position's entitlement cap (never negative).
    func remainingEntitlement(pos : BackerPosition) : Float {
        let remaining = pos.entitlement - lifetimeRepaidOf((pos.owner, pos.backerType));
        if (remaining > 0.0) { remaining } else { 0.0 };
    };

    // A position is OPEN while it still has more than dust headroom. Closed
    // positions receive no further toll and are dropped from the per-head counts.
    func isBackerOpen(pos : BackerPosition) : Bool {
        remainingEntitlement(pos) > BACKER_CAP_EPSILON;
    };

    // Credit `amount` toward `pos`, CAPPED at its remaining entitlement.
    // Advances both the unclaimed balance (backerRepayments) and the lifetime
    // high-water mark (backerLifetimeRepaid) by the credited portion, and
    // returns the uncredited OVERSHOOT so the caller can route it to a tracked
    // sink (the seed reserve) rather than over-paying past the cap.
    func creditBackerRepayment(pos : BackerPosition, amount : Float) : Float {
        if (amount <= 0.0) { return 0.0 };
        let key : BackerKey = (pos.owner, pos.backerType);
        let remaining = remainingEntitlement(pos);
        let credited = if (amount < remaining) { amount } else { remaining };
        if (credited > 0.0) {
            let current = switch (backerKeyMap.get(backerRepayments, key)) {
                case (null) { 0.0 };
                case (?existing) { existing };
            };
            backerRepayments := backerKeyMap.put(backerRepayments, key, current + credited);
            backerLifetimeRepaid := backerKeyMap.put(backerLifetimeRepaid, key, lifetimeRepaidOf(key) + credited);
        };
        amount - credited;
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

        // Only OPEN positions (lifetime repaid < entitlement) take part — in
        // crediting AND in the per-head counts, so a position closing stops it
        // diluting the rest. Any slice that can't reach a backer (a capped-out
        // recipient, or the orphaned senior slice when there is no Series A) is
        // redirected to the seed reserve — the same tracked sink the no-backers
        // branch uses — so 100% of every toll always lands in a tracked
        // destination and the solvency invariant holds.
        let allBackers = List.toArray(
            List.filter(
                List.fromArray(Iter.toArray(backerKeyMap.vals(backerPositions))),
                isBackerOpen,
            )
        );
        if (allBackers.size() == 0) {
            // No OPEN backers — the whole backer half flows to seed reserve.
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

        // Overshoot from capped-out recipients (and the orphaned senior slice)
        // accumulates here and is added to the seed reserve at the end.
        var overshootToSeed : Float = 0.0;

        // If there's only one Series A backer (no "others"), the 25% portion
        // also goes to that lone backer. Total to oldest in that case: 60%.
        let toOldest : Float =
            if (otherSeriesA.size() == 0) {
                backerRepaymentAmount * 0.60;
            } else {
                backerRepaymentAmount * 0.35;
            };
        var creditedToOldest : Float = 0.0;
        switch (oldestBacker) {
            // No Series A backer (set is all Series B): the senior slice has no
            // recipient — redirect it to the seed reserve instead of dropping it.
            case (null) { overshootToSeed += toOldest };
            case (?b) {
                let over = creditBackerRepayment(b, toOldest);
                creditedToOldest := toOldest - over;
                overshootToSeed += over;
            };
        };

        var toOthers : Float = 0.0;
        if (otherSeriesA.size() > 0) {
            let perBacker = backerRepaymentAmount * 0.25 / Float.fromInt(otherSeriesA.size());
            for (b in otherSeriesA.vals()) {
                let over = creditBackerRepayment(b, perBacker);
                toOthers += perBacker - over;
                overshootToSeed += over;
            };
        };

        let perAll = backerRepaymentAmount * 0.4 / Float.fromInt(allBackers.size());
        var toAll : Float = 0.0;
        for (b in allBackers.vals()) {
            let over = creditBackerRepayment(b, perAll);
            toAll += perAll - over;
            overshootToSeed += over;
        };

        roundSeedReserve += overshootToSeed;

        {
            tollAmount;
            toSeedReserve = seedAmount + overshootToSeed;
            toOldestSeriesA = creditedToOldest;
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
                    let originalLifetime = backerLifetimeRepaid;

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
                                backerLifetimeRepaid := originalLifetime;
                                return #Err(e);
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                backerLifetimeRepaid := originalLifetime;
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
                    let originalLifetime = backerLifetimeRepaid;

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
                                backerLifetimeRepaid := originalLifetime;
                                return #Err(e);
                            };
                        };
                        switch (await sendSolPayout(destination, payoutLamports)) {
                            case (#Err(e)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                backerLifetimeRepaid := originalLifetime;
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

    // Lifetime repayment high-water mark per position — the cumulative credited
    // amount that enforces the entitlement cap (remaining = entitlement −
    // lifetimeRepaid; a position closes once this reaches its entitlement).
    // Use alongside getBackerPositions to compute remaining/outstanding debt and
    // to reconcile a backfill (adminSetBackerLifetimeRepaid).
    public query func getBackerLifetimeRepaid() : async [(BackerKey, Float)] {
        Iter.toArray(backerKeyMap.entries(backerLifetimeRepaid));
    };

    // Total gross promised entitlement across ALL positions (incl. closed) —
    // a lifetime "promised" stat, unchanged by the cap.
    public query func getTotalBackerDebt() : async Float {
        var total = 0.0;
        for (b in backerKeyMap.vals(backerPositions)) { total += b.entitlement };
        total;
    };

    // True OUTSTANDING liability under the cap: Σ remaining (entitlement −
    // lifetime) over open positions; closed positions contribute 0.
    public query func getOutstandingBackerDebt() : async Float {
        var total = 0.0;
        for (b in backerKeyMap.vals(backerPositions)) { total += remainingEntitlement(b) };
        total;
    };

    // Oldest OPEN Series-A backer — the actual recipient of the senior toll
    // slice. Filters out closed positions so the display tracks distribution.
    public query func getOldestSeriesABacker() : async ?BackerPosition {
        var oldest : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in backerKeyMap.vals(backerPositions)) {
            if (b.backerType == #seriesA and isBackerOpen(b)) {
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

            // Move the lifetime high-water mark too, so the merged position's
            // remaining entitlement (Σentitlement − Σlifetime) is preserved and
            // the cap stays correct.
            let fromLifetime = lifetimeRepaidOf((from, #seriesA));
            if (fromLifetime > 0.0) {
                backerLifetimeRepaid := backerKeyMap.put(backerLifetimeRepaid, (to, #seriesA), lifetimeRepaidOf((to, #seriesA)) + fromLifetime);
            };
            backerLifetimeRepaid := backerKeyMap.delete(backerLifetimeRepaid, (from, #seriesA));

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
            backerLifetimeRepaid := backerKeyMap.empty<Float>();
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };

    // adminSetBackerLifetimeRepaid — seed/overwrite the lifetime-repaid
    // high-water mark for one position. backerLifetimeRepaid is a migration-free
    // addition that starts EMPTY on upgrade, so for backers paid before the cap
    // existed the owner must backfill their true lifetime-repaid (reconstruct
    // from #backerRepaymentClaim ledger events + the current unclaimed balance)
    // before the "principal + 24% then close" cap can be trusted. Setting this
    // at/above a position's entitlement CLOSES it (excluded from future tolls).
    // TEST_ADMIN only — part of the pre-launch admin hatch that must be
    // removed/secured before blackholing (see audit).
    public shared ({ caller }) func adminSetBackerLifetimeRepaid(
        owner : Principal,
        backerType : BackerType,
        amount : Float,
    ) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (amount < 0.0) { return #Err("amount must be >= 0") };
        acquireGlobalLock();
        try {
            backerLifetimeRepaid := backerKeyMap.put(backerLifetimeRepaid, (owner, backerType), amount);
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
                // Before treating as unmatched: try an open Series A backer
                // intent for this principal, amount within ±5%, not expired.
                // On match, register/merge the Series A position (NO Front-End
                // Load — mirrors addBackerMoney / adminRegisterSeriesABacker),
                // sweep, advance cursor. Checked before buy intents.
                var matchedBacker : ?BackerIntent = null;
                for (bi in natMap.vals(pendingBackerIntents)) {
                    if (bi.principal == sig.principal and not bi.fulfilled) {
                        let tol = bpsApply(bi.expectedAmountLamports, 500); // 5%
                        let lo : Nat64 = if (bi.expectedAmountLamports > tol) { bi.expectedAmountLamports - tol } else { 0 };
                        let hi : Nat64 = bi.expectedAmountLamports + tol;
                        if (inboundLamports >= lo and inboundLamports <= hi and Time.now() <= bi.expiresAt) {
                            matchedBacker := ?bi;
                        };
                    };
                };
                switch (matchedBacker) {
                    case (?bi) {
                        // CRITICAL-1 guard (UNCOMPILED — run `dfx build` before deploy):
                        // the credit path runs under `detectionInProgress` only, NOT
                        // `globalCriticalLock`. A concurrent withdraw/settle holding the
                        // global lock snapshots `platformStats` and, on a payout-failure
                        // rollback, restores that snapshot — erasing this credit's
                        // potBalance increment (the deposit's SOL is in the pool but the
                        // books understate it). Bail (the signature is re-scanned next
                        // tick) BEFORE mutating any state. There is no `await` between
                        // here and the platformStats write below, so this check and that
                        // write are one atomic region — the lock cannot change in between.
                        if (globalCriticalLock) {
                            return #Err("Critical section busy — backer credit deferred to next detection tick");
                        };
                        let depositSol = lamportsToSol(inboundLamports);
                        let entitlement = depositSol * 1.24; // Series A 24% bonus
                        switch (backerKeyMap.get(backerPositions, (bi.principal, #seriesA))) {
                            case (null) {
                                let pos : BackerPosition = {
                                    owner = bi.principal;
                                    amount = depositSol;
                                    entitlement;
                                    startTime = Time.now();
                                    isActive = true;
                                    backerType = #seriesA;
                                    firstDepositDate = ?Time.now();
                                };
                                backerPositions := backerKeyMap.put(backerPositions, (bi.principal, #seriesA), pos);
                            };
                            case (?existing) {
                                let updated : BackerPosition = {
                                    existing with
                                    amount = existing.amount + depositSol;
                                    entitlement = existing.entitlement + entitlement;
                                };
                                backerPositions := backerKeyMap.put(backerPositions, (bi.principal, #seriesA), updated);
                            };
                        };
                        // No cover charge on backer deposits: full gross to pot.
                        platformStats := {
                            platformStats with
                            potBalance = platformStats.potBalance + depositSol;
                        };
                        recordLedger(#backerDeposit({ backer = bi.principal; amount = depositSol; entitlement }));

                        // Rate-limit bookkeeping (mirrors the game-credit path).
                        let bnow = Time.now();
                        let bOneHourAgo = bnow - 3_600_000_000_000;
                        switch (principalMapNat.get(depositTimestamps, bi.principal)) {
                            case (null) {
                                depositTimestamps := principalMapNat.put(depositTimestamps, bi.principal, List.push(bnow, List.nil()));
                            };
                            case (?ts) {
                                let filtered = List.filter<Int>(ts, func(t) { t > bOneHourAgo });
                                depositTimestamps := principalMapNat.put(depositTimestamps, bi.principal, List.push(bnow, filtered));
                            };
                        };

                        // Mark intent fulfilled.
                        pendingBackerIntents := natMap.put(pendingBackerIntents, bi.id, { bi with fulfilled = true });

                        // Sweep deposit address → pool (pot already credited).
                        switch (await sweepToPool(sig.address, derivationPathForPrincipal(bi.principal), inboundLamports)) {
                            case (#Err(e)) { Debug.print("Backer sweep failed " # sig.signature # ": " # e) };
                            case (#Ok(_)) {};
                        };
                        lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
                        return #Ok(null); // backer credited, no game id
                    };
                    case (null) {};
                };
                // Before treating as unmatched: try an open buy intent for this
                // principal, amount within ±5%, not expired. On match, settle by
                // releasing escrowed PP, then sweep + advance cursor like a deposit.
                var matchedBuy : ?BuyIntent = null;
                for (bi in natMap.vals(pendingBuyIntents)) {
                    if (bi.principal == sig.principal and not bi.fulfilled) {
                        let tol = bpsApply(bi.quotedLamports, 500);
                        let lo : Nat64 = if (bi.quotedLamports > tol) { bi.quotedLamports - tol } else { 0 };
                        let hi : Nat64 = bi.quotedLamports + tol;
                        if (inboundLamports >= lo and inboundLamports <= hi and Time.now() <= bi.expiresAt) {
                            matchedBuy := ?bi;
                        };
                    };
                };
                switch (matchedBuy) {
                    case (?bi) {
                        switch (await settleBuyIntent(bi, inboundLamports)) {
                            case (#Ok(_pp)) {
                                switch (await sweepToPool(sig.address, derivationPathForPrincipal(sig.principal), inboundLamports)) {
                                    case (#Err(e)) { Debug.print("Desk sweep failed " # sig.signature # ": " # e) };
                                    case (#Ok(_)) {};
                                };
                                lastSeenSignature := textMap.put(lastSeenSignature, sig.address, sig.signature);
                                return #Ok(null);
                            };
                            case (#Err(e)) {
                                // Leave cursor UNADVANCED → retried next tick (e.g. transient ledger error).
                                Debug.print("Desk settle failed " # sig.signature # ": " # e);
                                return #Err(e);
                            };
                        };
                    };
                    case (null) {};
                };
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

        // CRITICAL-1 guard (UNCOMPILED — run `dfx build` before deploy): same
        // rationale as the backer path above. This credit is NOT under
        // `globalCriticalLock`, so a concurrent withdraw/settle's payout-failure
        // rollback (`platformStats := originalStats`) would erase this credit's
        // potBalance increment. Bail (re-scanned next tick) before mutating any
        // state. No `await` between here and the platformStats write below.
        if (globalCriticalLock) {
            return #Err("Critical section busy — deposit credit deferred to next detection tick");
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

    // HIGH-1 helper (UNCOMPILED — run `dfx build` before deploy): band-overlap
    // guard shared by the three intent entry points. creditDeposit matches one
    // inbound SOL transfer against open intents in the order deposit → backer →
    // buy, all sharing one per-user deposit address and the same ±5% match
    // window. Opening an intent of one kind whose amount band overlaps an open
    // intent of ANOTHER kind makes a single transfer ambiguous — it could be
    // credited as the wrong product. Each entry point passes which of the OTHER
    // two kinds to inspect (its own kind is skipped, so multiple same-kind
    // intents stay allowed). Callers must hold the per-caller lock so a
    // concurrent prepare can't slip a new overlapping intent in after the check.
    func bandOverlapsOpenIntents(
        p : Principal,
        lo : Nat64,
        hi : Nat64,
        checkDeposit : Bool,
        checkBacker : Bool,
        checkBuy : Bool,
    ) : Bool {
        let nowT = Time.now();
        func overlaps(elo : Nat64, ehi : Nat64) : Bool { lo <= ehi and elo <= hi };
        if (checkDeposit) {
            for (di in natMap.vals(pendingIntents)) {
                if (di.principal == p and not di.fulfilled and nowT <= di.expiresAt) {
                    let t = bpsApply(di.expectedAmountLamports, 500);
                    let elo : Nat64 = if (di.expectedAmountLamports > t) { di.expectedAmountLamports - t } else { 0 };
                    if (overlaps(elo, di.expectedAmountLamports + t)) { return true };
                };
            };
        };
        if (checkBacker) {
            for (bi in natMap.vals(pendingBackerIntents)) {
                if (bi.principal == p and not bi.fulfilled and nowT <= bi.expiresAt) {
                    let t = bpsApply(bi.expectedAmountLamports, 500);
                    let elo : Nat64 = if (bi.expectedAmountLamports > t) { bi.expectedAmountLamports - t } else { 0 };
                    if (overlaps(elo, bi.expectedAmountLamports + t)) { return true };
                };
            };
        };
        if (checkBuy) {
            for (yi in natMap.vals(pendingBuyIntents)) {
                if (yi.principal == p and not yi.fulfilled and nowT <= yi.expiresAt) {
                    let t = bpsApply(yi.quotedLamports, 500);
                    let elo : Nat64 = if (yi.quotedLamports > t) { yi.quotedLamports - t } else { 0 };
                    if (overlaps(elo, yi.quotedLamports + t)) { return true };
                };
            };
        };
        false;
    };

    public shared ({ caller }) func prepareSolDeposit(args : {
        plan : GamePlan;
        expectedAmountLamports : Nat64;
    }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        if (args.expectedAmountLamports < MIN_DEPOSIT_LAMPORTS) {
            return #Err("Minimum deposit is 0.01 SOL (10,000,000 lamports)");
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

            // HIGH-1 cross-kind ambiguity guard: block if an open BACKER or BUY
            // intent of a similar amount exists for this caller — one transfer must
            // not be routable to two different products. (Same-kind deposits stay
            // allowed.) Inside the caller lock so a concurrent prepare can't race.
            let dTol = bpsApply(args.expectedAmountLamports, 500);
            let dLo : Nat64 = if (args.expectedAmountLamports > dTol) { args.expectedAmountLamports - dTol } else { 0 };
            let dHi : Nat64 = args.expectedAmountLamports + dTol;
            if (bandOverlapsOpenIntents(caller, dLo, dHi, false, true, true)) {
                return #Err("You have an open Series A backing or PP buy of a similar SOL amount; finish or let it expire first (the two would be indistinguishable on-chain).");
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

    /// Self-serve Series A backing for SIWS/SOL users — the SOL analog of
    /// ponzi_math.addBackerMoney. Creates a BackerIntent; the next matching SOL
    /// landing on the caller's deposit address registers/merges their Series A
    /// position (in creditDeposit). NO Front-End Load (matches the ICP + admin
    /// backer paths). Min 0.05 SOL.
    public shared ({ caller }) func prepareBackerDeposit(args : {
        expectedAmountLamports : Nat64;
    }) : async { #Ok : { intentId : Nat; depositAddress : Text }; #Err : Text } {
        requireAuthenticated(caller);
        if (not selfServeBackingEnabled) {
            return #Err("Series A backing isn't open yet — check back soon.");
        };
        if (args.expectedAmountLamports < MIN_BACKER_LAMPORTS) {
            return #Err("Minimum Series A backing is 0.05 SOL (50,000,000 lamports)");
        };
        if (not bootstrapped) {
            return #Err("Canister not bootstrapped yet — operator must run bootstrap() first");
        };

        acquireCallerLock(caller);
        try {
            // Per-user rate limit — shares the 3-positions-per-hour gate.
            let currentTime = Time.now();
            let oneHourAgo = currentTime - 3_600_000_000_000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(timestamps, func(t) { t > oneHourAgo });
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };

            // HIGH-1 cross-kind ambiguity guard: block if an open DEPOSIT or BUY
            // intent of a similar amount exists. Without this, a 0.05 SOL deposit-
            // intent band and a 0.05 SOL backer-intent band overlap, and creditDeposit
            // (deposit matched FIRST) would open a game instead of a Series A position.
            let bTol = bpsApply(args.expectedAmountLamports, 500);
            let bLo : Nat64 = if (args.expectedAmountLamports > bTol) { args.expectedAmountLamports - bTol } else { 0 };
            let bHi : Nat64 = args.expectedAmountLamports + bTol;
            if (bandOverlapsOpenIntents(caller, bLo, bHi, true, false, true)) {
                return #Err("You have an open deposit or PP buy of a similar SOL amount; finish or let it expire first (the two would be indistinguishable on-chain).");
            };

            // Ensure the user has a deposit address (same logic as prepareSolDeposit).
            let depositAddr = switch (principalMapNat.get(depositAddresses, caller)) {
                case (?a) { a };
                case (null) {
                    let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(caller));
                    depositAddresses := principalMapNat.put(depositAddresses, caller, addr);
                    addressToPrincipal := textMap.put(addressToPrincipal, addr, caller);
                    addr;
                };
            };

            let intent : BackerIntent = {
                id = nextIntentId;
                principal = caller;
                expectedAmountLamports = args.expectedAmountLamports;
                createdAt = currentTime;
                expiresAt = currentTime + INTENT_TTL_NS;
                fulfilled = false;
            };
            pendingBackerIntents := natMap.put(pendingBackerIntents, nextIntentId, intent);
            let intentId = nextIntentId;
            nextIntentId += 1;

            #Ok({ intentId; depositAddress = depositAddr });
        } finally {
            releaseCallerLock(caller);
        };
    };

    public query ({ caller }) func getMyPendingBackerIntents() : async [BackerIntent] {
        var out = List.nil<BackerIntent>();
        for (intent in natMap.vals(pendingBackerIntents)) {
            if (intent.principal == caller and not intent.fulfilled) {
                out := List.push(intent, out);
            };
        };
        List.toArray(out);
    };

    // Whether self-serve Series A backing is open (prepareBackerDeposit). The
    // frontend reads this to show/hide the SOL backer panel; the backend gate above
    // is the authoritative check.
    public query func isSelfServeBackingEnabled() : async Bool { selfServeBackingEnabled };

    // Admin: open/close self-serve Series A backing. Default closed; flip on only
    // once the toll-distribution economics are Sybil-safe (see the flag's note).
    public shared ({ caller }) func adminSetSelfServeBacking(enabled : Bool) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        selfServeBackingEnabled := enabled;
        #Ok;
    };

    public query func deskListTiers() : async [DeskTier] { deskTiers };

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

    // Append a tier. Charles lists best-deal-first (highest rate at the top).
    public shared ({ caller }) func deskAddTier(ratePpUnitsPer0_1Sol : Nat, ppUnitsTotal : Nat) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (ratePpUnitsPer0_1Sol == 0) { return #Err("Rate must be > 0") };
        if (ppUnitsTotal == 0) { return #Err("Quantity must be > 0") };
        let tier : DeskTier = { ratePpUnitsPer0_1Sol; ppUnitsTotal; ppUnitsSold = 0; ppUnitsReserved = 0 };
        deskTiers := Array.append(deskTiers, [tier]);
        #Ok(deskTiers.size() - 1 : Nat);
    };

    // Update a tier's rate and/or total. Total cannot drop below sold+reserved.
    public shared ({ caller }) func deskUpdateTier(index : Nat, ratePpUnitsPer0_1Sol : Nat, ppUnitsTotal : Nat) : async { #Ok : (); #Err : Text } {
        requireAdmin(caller);
        if (index >= deskTiers.size()) { return #Err("No such tier") };
        if (ratePpUnitsPer0_1Sol == 0) { return #Err("Rate must be > 0") };
        let t = deskTiers[index];
        if (ppUnitsTotal < t.ppUnitsSold + t.ppUnitsReserved) {
            return #Err("Total cannot be below already sold+reserved");
        };
        let updated = { t with ratePpUnitsPer0_1Sol; ppUnitsTotal };
        deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) { if (i == index) { updated } else { deskTiers[i] } });
        #Ok();
    };

    // Remove a tier. Blocked while ANY tier has reserved PP, so open intents' tierIndex stays valid.
    public shared ({ caller }) func deskRemoveTier(index : Nat) : async { #Ok : (); #Err : Text } {
        requireAdmin(caller);
        if (index >= deskTiers.size()) { return #Err("No such tier") };
        for (t in deskTiers.vals()) {
            if (t.ppUnitsReserved > 0) { return #Err("Cannot restructure tiers while buy intents are open; wait for them to settle or expire (max 15 min)") };
        };
        deskTiers := Array.tabulate<DeskTier>(deskTiers.size() - 1 : Nat, func(i) { if (i < index) { deskTiers[i] } else { deskTiers[i + 1] } });
        #Ok();
    };

    // Replace the full ordered list (reorder/bulk edit). Blocked while reservations exist.
    public shared ({ caller }) func deskReorderTiers(newOrder : [DeskTier]) : async { #Ok : (); #Err : Text } {
        requireAdmin(caller);
        for (t in deskTiers.vals()) {
            if (t.ppUnitsReserved > 0) { return #Err("Cannot reorder while buy intents are open") };
        };
        for (t in newOrder.vals()) {
            if (t.ratePpUnitsPer0_1Sol == 0 or t.ppUnitsTotal < t.ppUnitsSold) { return #Err("Invalid tier in new order") };
        };
        deskTiers := newOrder;
        #Ok();
    };

    func deskReservedTotal() : Nat {
        var sum : Nat = 0;
        for (t in deskTiers.vals()) { sum += t.ppUnitsReserved };
        sum;
    };

    // Charles must icrc2_approve THIS canister as spender on pp_ledger first,
    // then call this to pull `ppUnits` from his wallet into the escrow subaccount.
    public shared ({ caller }) func deskDepositInventory(ppUnits : Nat) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (ppUnits == 0) { return #Err("Amount must be > 0") };
        try {
            let res = await ppLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = deskEscrowAccount();
                amount = ppUnits;
                fee = null; // pp_ledger fee is 0
                memo = null;
                created_at_time = null;
            });
            switch (res) {
                case (#Ok(block)) { #Ok(block) };
                case (#Err(e)) { #Err("transfer_from failed: " # debug_show (e)) };
            };
        } catch (e) { #Err("ppLedger call failed: " # Error.message(e)) };
    };

    // Return unsold, unreserved PP from escrow to `toOwner`.
    public shared ({ caller }) func deskWithdrawInventory(ppUnits : Nat, toOwner : Principal) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        if (ppUnits == 0) { return #Err("Amount must be > 0") };
        let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
        let reserved = deskReservedTotal();
        let available : Nat = if (bal > reserved) { bal - reserved } else { 0 };
        if (ppUnits > available) { return #Err("Only " # Nat.toText(available) # " PP available (rest is reserved)") };
        try {
            let res = await ppLedger.icrc1_transfer({
                from_subaccount = ?DESK_ESCROW_SUBACCOUNT;
                to = { owner = toOwner; subaccount = null };
                amount = ppUnits;
                fee = null;
                memo = null;
                created_at_time = null;
            });
            switch (res) {
                case (#Ok(block)) { #Ok(block) };
                case (#Err(e)) { #Err("transfer failed: " # debug_show (e)) };
            };
        } catch (e) { #Err("ppLedger call failed: " # Error.message(e)) };
    };

    public func deskInventory() : async { balanceUnits : Nat; reservedUnits : Nat; availableUnits : Nat } {
        let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
        let reserved = deskReservedTotal();
        { balanceUnits = bal; reservedUnits = reserved; availableUnits = if (bal > reserved) { bal - reserved } else { 0 } };
    };

    public type QuoteLeg = { tierIndex : Nat; ppUnits : Nat; lamports : Nat64; ratePpUnitsPer0_1Sol : Nat };
    public type DeskQuote = { ppUnitsOut : Nat; legs : [QuoteLeg]; cappedByInventory : Bool };

    // Pure: walk tiers top-down spending `lamports` against each tier's available PP.
    func computeQuote(lamports : Nat64) : DeskQuote {
        var remaining : Nat = Nat64.toNat(lamports);
        var totalPp : Nat = 0;
        var legs = List.nil<QuoteLeg>();
        // Consume BEST-DEAL-FIRST: the highest PP-per-0.1-SOL tier (most PP for
        // the buyer) is sold first, regardless of the order tiers were added.
        // As the best tier sells out the next-best becomes active. `order` holds
        // storage indices sorted by rate descending; legs carry the ORIGINAL
        // storage index so reservation/settlement stay valid.
        let order = Array.sort<Nat>(
            Array.tabulate<Nat>(deskTiers.size(), func(k) { k }),
            func(a : Nat, b : Nat) { Nat.compare(deskTiers[b].ratePpUnitsPer0_1Sol, deskTiers[a].ratePpUnitsPer0_1Sol) },
        );
        var oi : Nat = 0;
        while (oi < order.size() and remaining > 0) {
            let i = order[oi];
            let t = deskTiers[i];
            let availablePp : Nat = if (t.ppUnitsTotal > t.ppUnitsSold + t.ppUnitsReserved) {
                t.ppUnitsTotal - t.ppUnitsSold - t.ppUnitsReserved : Nat;
            } else { 0 };
            if (availablePp > 0) {
                let maxLamportsForTier : Nat = availablePp * PP_S / t.ratePpUnitsPer0_1Sol;
                let spend : Nat = Nat.min(remaining, maxLamportsForTier);
                let ppFromTier : Nat = spend * t.ratePpUnitsPer0_1Sol / PP_S;
                if (ppFromTier > 0) {
                    totalPp += ppFromTier;
                    legs := List.push({ tierIndex = i; ppUnits = ppFromTier; lamports = Nat64.fromNat(spend); ratePpUnitsPer0_1Sol = t.ratePpUnitsPer0_1Sol }, legs);
                    remaining -= spend;
                };
            };
            oi += 1;
        };
        { ppUnitsOut = totalPp; legs = List.toArray(List.reverse(legs)); cappedByInventory = remaining > 0 };
    };

    public query func quoteBuyPP(lamports : Nat64) : async DeskQuote { computeQuote(lamports) };

    // Add (+) or release (-) reserved PP on the named tiers.
    func applyReservation(legs : [QuoteLeg], release : Bool) {
        deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) {
            var t = deskTiers[i];
            for (leg in legs.vals()) {
                if (leg.tierIndex == i) {
                    if (release) {
                        let newReserved : Nat = if (t.ppUnitsReserved > leg.ppUnits) { t.ppUnitsReserved - leg.ppUnits } else { 0 };
                        t := { t with ppUnitsReserved = newReserved };
                    } else {
                        t := { t with ppUnitsReserved = t.ppUnitsReserved + leg.ppUnits };
                    };
                };
            };
            t;
        });
    };

    // Core reserve+create, shared by the public method and the test shim.
    // NO auth/lock/bootstrap checks here — callers add those.
    func reserveBuyIntentFor(buyer : Principal, lamports : Nat64) : async {
        #Ok : { intentId : Nat; depositAddress : Text; ppUnitsReserved : Nat; legs : [QuoteLeg]; expiresAt : Int };
        #Err : Text;
    } {
        let quote = computeQuote(lamports);
        if (quote.ppUnitsOut == 0) { return #Err("Desk has no inventory available") };
        // Best-effort over-reservation guard. Two distinct principals can race
        // past this check (separate caller locks, plus an await before the reserve
        // mutates state), transiently reserving beyond the escrow balance. That is
        // bounded and self-healing: settlement's icrc1_transfer from escrow is the
        // authoritative backstop (a short escrow simply fails that fill with #Err),
        // and expiry releases the phantom reservation. Acceptable for an admin-
        // stocked OTC desk; a global lock here would serialize every buyer through
        // a threshold-Ed25519 derivation await for a mostly-theoretical race.
        let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
        if (bal < deskReservedTotal() + quote.ppUnitsOut) {
            return #Err("Insufficient desk inventory to reserve this buy");
        };
        let depositAddr = switch (principalMapNat.get(depositAddresses, buyer)) {
            case (?a) { a };
            case (null) {
                let addr = await SolSigner.deriveAddress(keyId, derivationPathForPrincipal(buyer));
                depositAddresses := principalMapNat.put(depositAddresses, buyer, addr);
                addressToPrincipal := textMap.put(addressToPrincipal, addr, buyer);
                addr;
            };
        };
        applyReservation(quote.legs, false);
        let now = Time.now();
        let reserved : [BuyReservation] = Array.map<QuoteLeg, BuyReservation>(quote.legs, func(l) {
            { tierIndex = l.tierIndex; ppUnits = l.ppUnits; ratePpUnitsPer0_1Sol = l.ratePpUnitsPer0_1Sol };
        });
        let intent : BuyIntent = {
            id = nextBuyIntentId; principal = buyer; reserved;
            ppUnitsReservedTotal = quote.ppUnitsOut; quotedLamports = lamports;
            createdAt = now; expiresAt = now + DESK_BUY_INTENT_TTL_NS; fulfilled = false;
        };
        pendingBuyIntents := natMap.put(pendingBuyIntents, nextBuyIntentId, intent);
        let intentId = nextBuyIntentId;
        nextBuyIntentId += 1;
        #Ok({ intentId; depositAddress = depositAddr; ppUnitsReserved = quote.ppUnitsOut; legs = quote.legs; expiresAt = intent.expiresAt });
    };

    public shared ({ caller }) func createBuyIntent(lamports : Nat64) : async {
        #Ok : { intentId : Nat; depositAddress : Text; ppUnitsReserved : Nat; legs : [QuoteLeg]; expiresAt : Int };
        #Err : Text;
    } {
        requireAuthenticated(caller);
        // No minimum buy on SOL — any amount that yields >= 1 PP unit is allowed
        // (the quote's ppUnitsOut == 0 check below is the only natural floor).
        if (not bootstrapped) { return #Err("Canister not bootstrapped yet") };
        acquireCallerLock(caller);
        try {
            // I-1 guard (now via the shared bandOverlapsOpenIntents helper): the desk,
            // game-deposit, and Series A backer flows share one per-user deposit address
            // and the same ±5% observer match window. creditDeposit matches deposit →
            // backer → buy, so an open DEPOSIT or BACKER intent of a similar amount would
            // consume this buyer's SOL as the wrong product. Reject here (inside the lock,
            // so a concurrent prepare can't race a new overlapping intent in). Originally
            // checked deposit intents only; extended to backer intents when self-serve
            // Series A backing shipped.
            let buyTol = bpsApply(lamports, 500);
            let buyLo : Nat64 = if (lamports > buyTol) { lamports - buyTol } else { 0 };
            let buyHi : Nat64 = lamports + buyTol;
            if (bandOverlapsOpenIntents(caller, buyLo, buyHi, true, true, false)) {
                return #Err("You have an open deposit or Series A backing of a similar SOL amount; finish or let it expire before buying PP at this amount (the two would be indistinguishable on-chain).");
            };
            await reserveBuyIntentFor(caller, lamports);
        } finally {
            releaseCallerLock(caller);
        };
    };

    // TEST/diagnostic (TEST_ADMIN only): create a buy intent for `buyer`, bypassing
    // auth/lock so the reserve+settle path is testable on a local replica. Gated to a
    // NON-bootstrapped canister so it is structurally inert in production — a
    // bootstrapped (mainnet) canister rejects it, making "local testing only" enforced
    // rather than merely documented.
    public shared ({ caller }) func adminTestCreateBuyIntent(buyer : Principal, lamports : Nat64) : async {
        #Ok : { intentId : Nat; depositAddress : Text; ppUnitsReserved : Nat; legs : [QuoteLeg]; expiresAt : Int };
        #Err : Text;
    } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (bootstrapped) { return #Err("Test shim disabled on a bootstrapped canister") };
        await reserveBuyIntentFor(buyer, lamports);
    };

    // TEST/diagnostic (TEST_ADMIN only): drive settleBuyIntent directly with a
    // simulated inbound amount, bypassing the Solana observer. Gated to a
    // NON-bootstrapped canister so it is structurally inert in production.
    public shared ({ caller }) func adminTestSettleBuyIntent(intentId : Nat, inboundLamports : Nat64) : async { #Ok : Nat; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (bootstrapped) { return #Err("Test shim disabled on a bootstrapped canister") };
        switch (natMap.get(pendingBuyIntents, intentId)) {
            case (null) { #Err("No such buy intent") };
            case (?bi) { if (bi.fulfilled) { #Err("Already fulfilled") } else { await settleBuyIntent(bi, inboundLamports) } };
        };
    };

    // TEST/diagnostic (TEST_ADMIN, non-bootstrapped only): force globalCriticalLock so
    // a local test can simulate "a withdraw is mid critical-section" WITHOUT a real
    // sol-rpc payout. Sets the lock and returns WITHOUT releasing it, so the NEXT
    // message observes it held; call with `false` to release. Inert on a bootstrapped
    // (mainnet) canister so it can never wedge production. (Supports the CRITICAL-1
    // guard test — see ponzi_math_sol/scripts/test-critical1-guard.sh.)
    public shared ({ caller }) func adminTestSetGlobalLock(held : Bool) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (bootstrapped) { return #Err("Test shim disabled on a bootstrapped canister") };
        globalCriticalLock := held;
        #Ok;
    };

    // TEST/diagnostic (TEST_ADMIN, non-bootstrapped only): exercise creditDeposit's
    // CRITICAL-1 guard + potBalance write in isolation, bypassing the sol-rpc
    // getTransaction the real path needs. Mirrors EXACTLY the guard and the
    // platformStats mutation from creditDeposit's matched-deposit branch (minus the
    // game record + sweep). Returns the post-credit potBalance, or the busy error if
    // the guard fired. A test asserts: with the lock held (adminTestSetGlobalLock true)
    // this returns #Err and leaves potBalance unchanged; with it free, potBalance grows.
    public shared ({ caller }) func adminTestGuardedPotCredit(inboundLamports : Nat64) : async { #Ok : Float; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (bootstrapped) { return #Err("Test shim disabled on a bootstrapped canister") };
        // EXACT copy of creditDeposit's CRITICAL-1 guard (keep in sync):
        if (globalCriticalLock) {
            return #Err("Critical section busy — deposit credit deferred to next detection tick");
        };
        let coverChargeLamports = bpsApply(inboundLamports, COVER_CHARGE_RATE_LAMPORTS_BPS);
        let netLamports = inboundLamports - coverChargeLamports;
        let netSol = lamportsToSol(netLamports);
        platformStats := {
            platformStats with
            totalDeposits = platformStats.totalDeposits + lamportsToSol(inboundLamports);
            activeGames = platformStats.activeGames + 1;
            potBalance = platformStats.potBalance + netSol;
        };
        #Ok(platformStats.potBalance);
    };

    // Credit PP for a matched buy payment. Pure-computes the proportional fill,
    // transfers PP from escrow FIRST, and ONLY on transfer success mutates
    // tier/intent/accrual. A failed transfer leaves the intent open for retry.
    func settleBuyIntent(intent : BuyIntent, inboundLamports : Nat64) : async { #Ok : Nat; #Err : Text } {
        // 1. Walk locked legs, consume inboundLamports → creditPp + per-leg filled.
        var remaining : Nat = Nat64.toNat(inboundLamports);
        var creditPp : Nat = 0;
        let filled = Array.map<BuyReservation, Nat>(intent.reserved, func(leg) {
            let legLamports : Nat = leg.ppUnits * PP_S / leg.ratePpUnitsPer0_1Sol;
            let spend : Nat = Nat.min(remaining, legLamports);
            let ppFilled : Nat = if (spend >= legLamports) { leg.ppUnits } else { spend * leg.ratePpUnitsPer0_1Sol / PP_S };
            remaining := if (remaining > spend) { remaining - spend } else { 0 };
            creditPp += ppFilled;
            ppFilled;
        });
        if (creditPp == 0) { return #Err("Zero fill") };
        // 2. Transfer PP escrow → buyer FIRST.
        let xfer = try {
            await ppLedger.icrc1_transfer({
                from_subaccount = ?DESK_ESCROW_SUBACCOUNT;
                to = { owner = intent.principal; subaccount = null };
                amount = creditPp;
                fee = null; memo = null; created_at_time = null;
            });
        } catch (e) { #Err(#GenericError({ error_code = 0; message = Error.message(e) })) };
        switch (xfer) {
            case (#Err(e)) { return #Err("PP transfer failed: " # debug_show (e)) };
            case (#Ok(_)) {};
        };
        // 3. On success: release each leg's full reservation, add filled to sold.
        // INVARIANT (load-bearing — do NOT break): there must be NO `await` between the
        // icrc1_transfer above and the `fulfilled = true` write below. The next await
        // (sweepToPool, in the caller) is the commit point that durably persists
        // `fulfilled`; an await inserted here would let a trap in its continuation roll
        // back `fulfilled` while the ledger transfer already committed — a replay /
        // double-credit window. Keep this tail synchronous.
        deskTiers := Array.tabulate<DeskTier>(deskTiers.size(), func(i) {
            var t = deskTiers[i];
            var j : Nat = 0;
            for (leg in intent.reserved.vals()) {
                if (leg.tierIndex == i) {
                    let rel : Nat = if (t.ppUnitsReserved > leg.ppUnits) { t.ppUnitsReserved - leg.ppUnits } else { 0 };
                    t := { t with ppUnitsReserved = rel; ppUnitsSold = t.ppUnitsSold + filled[j] };
                };
                j += 1;
            };
            t;
        });
        pendingBuyIntents := natMap.put(pendingBuyIntents, intent.id, { intent with fulfilled = true });
        // Books the full matched inbound as proceeds. A payment only reaches here if it
        // matched within ±5% of the quote, so any overpay is bounded to that tolerance
        // and kept as desk proceeds (the buyer received the full reserved PP). Larger
        // overpays never match → fall through to unmatched and are refundable. No
        // micro-refund leg for the in-tolerance remainder.
        deskProceedsAccrualLamports += inboundLamports;
        recordLedger(#deskSale({ buyer = intent.principal; ppUnitsCredited = creditPp; lamportsReceived = Nat64.toNat(inboundLamports); intentId = intent.id }));
        #Ok(creditPp);
    };

    public query ({ caller }) func getMyPendingBuyIntents() : async [BuyIntent] {
        var out = List.nil<BuyIntent>();
        for (intent in natMap.vals(pendingBuyIntents)) {
            if (intent.principal == caller and not intent.fulfilled) { out := List.push(intent, out) };
        };
        List.toArray(out);
    };

    public query ({ caller }) func adminGetAllBuyIntents() : async [BuyIntent] {
        requireAdmin(caller);
        Iter.toArray(natMap.vals(pendingBuyIntents));
    };

    // Release reservations for expired, unfulfilled buy intents (PP returns to the pool).
    func releaseExpiredBuyIntents() {
        let now = Time.now();
        for (bi in natMap.vals(pendingBuyIntents)) {
            if (not bi.fulfilled and now > bi.expiresAt) {
                let legs = Array.map<BuyReservation, QuoteLeg>(bi.reserved, func(r) {
                    { tierIndex = r.tierIndex; ppUnits = r.ppUnits; lamports = 0 : Nat64; ratePpUnitsPer0_1Sol = r.ratePpUnitsPer0_1Sol };
                });
                applyReservation(legs, true);
                pendingBuyIntents := natMap.put(pendingBuyIntents, bi.id, { bi with fulfilled = true });
            };
        };
    };

    // Withdraw accrued desk SOL revenue from the pool to Charles's address.
    public shared ({ caller }) func adminWithdrawDeskProceeds(toAddress : Text) : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (deskProceedsAccrualLamports == 0) { return #Err("No desk proceeds to withdraw") };
        let amount = deskProceedsAccrualLamports;
        switch (await sendSolPayout(toAddress, amount)) {
            case (#Ok(txSig)) {
                deskProceedsAccrualLamports := 0;
                recordLedger(#deskProceedsWithdrawal({ toAddress; lamports = Nat64.toNat(amount); txSig }));
                #Ok(txSig);
            };
            case (#Err(e)) { #Err(e) };
        };
    };

    // Refund SOL to a buyer-supplied address, drawn from MATCHED desk proceeds so it can
    // never dip into commingled game-pot SOL — the accrual counter bounds it. The observer
    // can't extract the sender, so Charles supplies the address. Records a distinct
    // #deskRefund event (NOT #deskProceedsWithdrawal) so refunds are not conflated with
    // revenue withdrawals in the ledger.
    // OPERATOR NOTES: (1) refunds net against the SAME counter as adminWithdrawDeskProceeds,
    // so issue refunds BEFORE withdrawing proceeds. (2) This is for refunding proceeds the
    // desk actually took in; an OUT-OF-tolerance overpay (>±5%, never matched/settled) is
    // not counted here — it stays on the buyer's per-user deposit address and is recovered
    // via the existing deposit-address recovery path, not this method.
    public shared ({ caller }) func adminRefundDeskSol(toAddress : Text, lamports : Nat64) : async { #Ok : Text; #Err : Text } {
        requireAdmin(caller);
        if (lamports == 0 or lamports > deskProceedsAccrualLamports) { return #Err("Amount exceeds accrued desk proceeds") };
        switch (await sendSolPayout(toAddress, lamports)) {
            case (#Ok(txSig)) {
                let newAccrual : Nat64 = if (deskProceedsAccrualLamports > lamports) { deskProceedsAccrualLamports - lamports } else { 0 };
                deskProceedsAccrualLamports := newAccrual;
                recordLedger(#deskRefund({ toAddress; lamports = Nat64.toNat(lamports); txSig }));
                #Ok(txSig);
            };
            case (#Err(e)) { #Err(e) };
        };
    };

    public func deskStats() : async {
        inventoryUnits : Nat; reservedUnits : Nat; availableUnits : Nat;
        proceedsLamports : Nat; openBuyIntents : Nat; tierCount : Nat; totalSoldUnits : Nat;
    } {
        let bal = await ppLedger.icrc1_balance_of(deskEscrowAccount());
        let reserved = deskReservedTotal();
        var open : Nat = 0;
        for (bi in natMap.vals(pendingBuyIntents)) { if (not bi.fulfilled) { open += 1 } };
        var sold : Nat = 0;
        for (t in deskTiers.vals()) { sold += t.ppUnitsSold };
        {
            inventoryUnits = bal; reservedUnits = reserved;
            availableUnits = if (bal > reserved) { bal - reserved } else { 0 };
            proceedsLamports = Nat64.toNat(deskProceedsAccrualLamports);
            openBuyIntents = open; tierCount = deskTiers.size(); totalSoldUnits = sold;
        };
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
        releaseExpiredBuyIntents();
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
        for (bi in natMap.vals(pendingBuyIntents)) {
            if (not bi.fulfilled and now <= bi.expiresAt) {
                switch (principalMapNat.get(depositAddresses, bi.principal)) {
                    case (?addr) { toScan := textMap.put(toScan, addr, bi.principal) };
                    case (null) {};
                };
            };
        };
        for (bi in natMap.vals(pendingBackerIntents)) {
            if (not bi.fulfilled and now <= bi.expiresAt) {
                switch (principalMapNat.get(depositAddresses, bi.principal)) {
                    case (?addr) { toScan := textMap.put(toScan, addr, bi.principal) };
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

    /// User-triggered detection for the CALLER's own deposit address only.
    /// Lets the frontend get a near-instant credit right after the user's
    /// wallet confirms the SOL transfer, instead of waiting for the 60s timer.
    /// Abuse-bounded: makes ZERO RPC outcalls unless the caller has an open,
    /// unexpired intent (deposit or buy), and is rate-limited to once per
    /// POKE_COOLDOWN_NS per caller. Shares the detectionInProgress guard with
    /// the auto-timer so the two never run concurrently.
    public shared ({ caller }) func pokeMyDeposit() : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };

        let now = Time.now();

        // Per-caller cooldown — cheap, before any work.
        switch (principalMapNat.get(pokeTimestamps, caller)) {
            case (?last) {
                if (now - last < POKE_COOLDOWN_NS) {
                    return #Err("Please wait a few seconds before checking again");
                };
            };
            case (null) {};
        };
        // HIGH-2a: stamp the poke NOW — right after the cooldown passes — rather
        // than after the `detectionInProgress` early-return below. Otherwise a caller
        // could bypass the cooldown entirely whenever detection happens to be busy,
        // re-scanning the three intent maps on every call.
        pokeTimestamps := principalMapNat.put(pokeTimestamps, caller, now);

        // Open-intent gate: only scan if the caller has an open, unexpired
        // intent. Otherwise return #Ok(0) with ZERO RPC outcalls.
        var hasOpen = false;
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == caller and not intent.fulfilled and now <= intent.expiresAt) {
                hasOpen := true;
            };
        };
        if (not hasOpen) {
            for (bi in natMap.vals(pendingBuyIntents)) {
                if (bi.principal == caller and not bi.fulfilled and now <= bi.expiresAt) {
                    hasOpen := true;
                };
            };
        };
        if (not hasOpen) {
            for (bi in natMap.vals(pendingBackerIntents)) {
                if (bi.principal == caller and not bi.fulfilled and now <= bi.expiresAt) {
                    hasOpen := true;
                };
            };
        };
        if (not hasOpen) { return #Ok(0) };

        if (detectionInProgress) { return #Err("Detection busy — try again shortly") };

        let addr = switch (principalMapNat.get(depositAddresses, caller)) {
            case (?a) { a };
            case (null) { return #Ok(0) };
        };

        detectionInProgress := true;
        try {
            let credits = await scanAndCredit(addr, caller);
            #Ok(credits);
        } catch (e) {
            #Err("Scan failed: " # Error.message(e));
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
