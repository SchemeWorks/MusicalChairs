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

import Ledger "ledger";
import Icrc21 "icrc21";

persistent actor class PonziMath(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    // Privileged principals are pinned to their first-init values (Audit F-013).
    // On the very first install storedBackend/storedTestAdmin are null, so we
    // adopt the constructor args and persist them below; on every later upgrade
    // the persisted values win and the constructor args are IGNORED, so a stale
    // or wrong --argument on a redeploy cannot silently rotate the cover-charge
    // sweep destination (BACKEND_PRINCIPAL) or the test-admin god account
    // (TEST_ADMIN). A deliberate rotation would require an explicit, code-reviewed
    // admin method, which intentionally does not exist.
    var storedBackend : ?Principal = null;
    var storedTestAdmin : ?Principal = null;
    transient let BACKEND_PRINCIPAL : Principal = switch (storedBackend) {
        case (?p) { p };
        case (null) { initArgs.backendPrincipal };
    };
    transient let TEST_ADMIN : Principal = switch (storedTestAdmin) {
        case (?p) { p };
        case (null) { initArgs.testAdmin };
    };
    // Persist on first init (no-op on every later start).
    if (storedBackend == null) { storedBackend := ?BACKEND_PRINCIPAL };
    if (storedTestAdmin == null) { storedTestAdmin := ?TEST_ADMIN };
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
    transient let ic : actor { raw_rand : () -> async Blob } = actor "aaaaa-aa";

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

    // Index of currently-active game ids, maintained incrementally so the
    // round-reset and promotion paths never scan the full, never-pruned
    // gameRecords history (Audit F-002). A drift guard falls back to a full
    // scan if this index ever disagrees with platformStats.activeGames, so
    // correctness never depends on the index being perfectly maintained.
    var activeGameIds = natMap.empty<()>();
    var activeGameIndexBackfilled : Bool = false;

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

    // Query the ICP ledger's current transfer fee, falling back to the known
    // default if the ledger query fails (Audit F-009). Used by every payout
    // path so a future governance fee change cannot make the canister
    // over/under-pay or trap. Callers hold the global lock across this await,
    // so it cannot interleave another critical operation.
    func liveIcpFee() : async Nat {
        try { await icpLedger.icrc1_fee() } catch (_) { Ledger.ICP_TRANSFER_FEE };
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
    // 1e-8 ICP = 1 e8s, the smallest representable ICP unit.
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

        // Only OPEN positions (lifetime repaid < entitlement) take part. The
        // backer half splits 35 / 45 / 20:
        //   - 35% SENIOR bonus to the single oldest open Series A (kept).
        //   - 45% SERIES A POOL, shared PROPORTIONAL to each open A's remaining
        //     entitlement. Sybil-neutral: splitting a stake across N wallets
        //     yields N remainders summing to the same total, so total take is
        //     unchanged, and all open A close at the same rate (when the pool
        //     exceeds total remaining they all cap out together).
        //   - 20% SERIES B POOL, shared PER-HEAD across open B (B is assigned by
        //     promotion and can't be Sybil'd, so egalitarian is fine).
        // An absent tier's pool folds into the other so backer money stays with
        // backers. The senior slice (when there is no Series A) and any
        // capped-out overshoot go to the seed reserve — the same tracked sink
        // the no-backers branch uses — so 100% of every toll always lands in a
        // tracked destination and the solvency invariant holds.
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
        let seriesBBackers = List.toArray(
            List.filter(
                List.fromArray(allBackers),
                func(b : BackerPosition) : Bool { b.backerType == #seriesB },
            )
        );

        let seniorAmt = backerRepaymentAmount * 0.35;
        var aPoolAmt = backerRepaymentAmount * 0.45;
        var bPoolAmt = backerRepaymentAmount * 0.20;
        // Fold an absent tier's pool into the other (at least one is non-empty
        // here — the all-empty case returned above).
        if (seriesBBackers.size() == 0) { aPoolAmt += bPoolAmt; bPoolAmt := 0.0 };
        if (seriesABackers.size() == 0) { bPoolAmt += aPoolAmt; aPoolAmt := 0.0 };

        var overshootToSeed : Float = 0.0;

        // --- 35% senior bonus: single oldest open Series A ---
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
        var creditedToOldest : Float = 0.0;
        switch (oldestBacker) {
            // No Series A: senior slice has no recipient — route to seed reserve.
            case (null) { overshootToSeed += seniorAmt };
            case (?b) {
                let over = creditBackerRepayment(b, seniorAmt);
                creditedToOldest := seniorAmt - over;
                overshootToSeed += over;
            };
        };

        // --- 45% Series A pool: proportional to remaining entitlement ---
        var creditedAPool : Float = 0.0;
        if (aPoolAmt > 0.0) {
            var totalRemainingA : Float = 0.0;
            for (b in seriesABackers.vals()) { totalRemainingA += remainingEntitlement(b) };
            if (totalRemainingA > 0.0) {
                for (b in seriesABackers.vals()) {
                    let share = remainingEntitlement(b) / totalRemainingA * aPoolAmt;
                    let over = creditBackerRepayment(b, share);
                    creditedAPool += share - over;
                    overshootToSeed += over;
                };
            } else {
                overshootToSeed += aPoolAmt;
            };
        };

        // --- 20% Series B pool: per-head across open B ---
        var creditedBPool : Float = 0.0;
        if (bPoolAmt > 0.0 and seriesBBackers.size() > 0) {
            let perB = bPoolAmt / Float.fromInt(seriesBBackers.size());
            for (b in seriesBBackers.vals()) {
                let over = creditBackerRepayment(b, perB);
                creditedBPool += perB - over;
                overshootToSeed += over;
            };
        } else if (bPoolAmt > 0.0) {
            overshootToSeed += bPoolAmt;
        };

        roundSeedReserve += overshootToSeed;

        // Ledger fields keep their stored names (stable type) but now mean:
        //   toOldestSeriesA = senior bonus credited
        //   toOtherSeriesA  = Series A pool credited (proportional, all open A)
        //   toAllBackers    = Series B pool credited (per-head)
        {
            tollAmount;
            toSeedReserve = seedAmount + overshootToSeed;
            toOldestSeriesA = creditedToOldest;
            toOtherSeriesA = creditedAPool;
            toAllBackers = creditedBPool;
        };
    };

    // ========================================================================
    // Series B promotion: pick a random underwater player at round-reset time
    // and grant them a Series B backer position with entitlement
    // (amount - totalWithdrawn) * 1.16.
    // ========================================================================

    // Lazily backfill activeGameIds from gameRecords the first time a consumer
    // needs it (Audit F-002). After the upgrade that introduces the index this
    // runs once (cheap while history is small) and sets the flag; later calls
    // are no-ops. Idempotent and self-healing: it rebuilds from the source of
    // truth (gameRecords.isActive), so a fresh install stays correct too.
    func ensureActiveIndexBackfilled() {
        if (not activeGameIndexBackfilled) {
            for ((gid, game) in natMap.entries(gameRecords)) {
                if (game.isActive) { activeGameIds := natMap.put(activeGameIds, gid, ()) };
            };
            activeGameIndexBackfilled := true;
        };
    };

    // Returns all currently-active game records. Reads via the activeGameIds
    // index when its size agrees with the activeGames counter; otherwise falls
    // back to a full history scan so correctness never depends on the index
    // being perfectly maintained (Audit F-002).
    func activeGamesSnapshot() : [GameRecord] {
        ensureActiveIndexBackfilled();
        var acc = List.nil<GameRecord>();
        if (natMap.size(activeGameIds) == platformStats.activeGames) {
            for ((gid, _) in natMap.entries(activeGameIds)) {
                switch (natMap.get(gameRecords, gid)) {
                    case (?g) { if (g.isActive) { acc := List.push(g, acc) } };
                    case (null) {};
                };
            };
        } else {
            for (g in natMap.vals(gameRecords)) {
                if (g.isActive) { acc := List.push(g, acc) };
            };
        };
        List.toArray(acc);
    };

    // Pick a Series B promotion candidate from the current round's losers.
    // Eligibility (phase 1): underwater players who currently have ZERO entries
    // in backerPositions. If none qualify (phase 2 — every underwater player
    // already has a position), fall back to all underwater players. Uses
    // raw_rand for selection — caller must be in an async update context.
    // Returns null if no one is underwater.
    func selectPromotionCandidate() : async ?{ owner : Principal; underwater : Float } {
        var underwaterByPlayer = principalMapNat.empty<Float>();
        // Iterate active games via the index (drift-guarded). (Audit F-002.)
        for (g in activeGamesSnapshot().vals()) {
            let loss = g.amount - g.totalWithdrawn;
            if (loss > 0.0) {
                let prev = switch (principalMapNat.get(underwaterByPlayer, g.player)) {
                    case (null) { 0.0 };
                    case (?v) { v };
                };
                underwaterByPlayer := principalMapNat.put(underwaterByPlayer, g.player, prev + loss);
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
        // Iterate the active-id index (drift-guarded) instead of the full,
        // never-pruned history. (Audit F-002.)
        for (game in activeGamesSnapshot().vals()) {
            let closed : GameRecord = {
                game with
                isActive = false;
                lastUpdateTime = now;
            };
            gameRecords := natMap.put(gameRecords, game.id, closed);
        };
        activeGameIds := natMap.empty<()>();

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
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentTime, List.nil()));
                };
                case (?timestamps) {
                    let filtered = List.filter<Int>(timestamps, func(t) { t > oneHourAgo });
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentTime, filtered));
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
            activeGameIds := natMap.put(activeGameIds, gameId, ()); // Audit F-002
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
                    let originalActiveIds = activeGameIds; // Audit F-002
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
                    if (willClose) { activeGameIds := natMap.delete(activeGameIds, gameId) }; // Audit F-002
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
                    let icpFee = await liveIcpFee(); // Audit F-009: live ledger fee
                    if (netEarningsE8s > icpFee) {
                        let transferAmount : Nat = netEarningsE8s - icpFee;
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = transferAmount;
                                fee = ?icpFee;
                                memo = null;
                                created_at_time = null;
                            });
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            activeGameIds := originalActiveIds; // Audit F-002
                            backerLifetimeRepaid := originalLifetime;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                activeGameIds := originalActiveIds; // Audit F-002
                                backerLifetimeRepaid := originalLifetime;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
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
                    let originalActiveIds = activeGameIds; // Audit F-002
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
                    activeGameIds := natMap.delete(activeGameIds, gameId); // Audit F-002
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                    };

                    let payoutE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    let icpFee = await liveIcpFee(); // Audit F-009: live ledger fee
                    if (payoutE8s > icpFee) {
                        let transferAmount : Nat = payoutE8s - icpFee;
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = transferAmount;
                                fee = ?icpFee;
                                memo = null;
                                created_at_time = null;
                            });
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            activeGameIds := originalActiveIds; // Audit F-002
                            backerLifetimeRepaid := originalLifetime;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                activeGameIds := originalActiveIds; // Audit F-002
                                backerLifetimeRepaid := originalLifetime;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
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

            let balanceE8s = Int.abs(Float.toInt(roundToEightDecimals(balance) * 100_000_000.0));
            let icpFee = await liveIcpFee(); // Audit F-009: live ledger fee
            if (balanceE8s <= icpFee) {
                return #Err("Claimable balance is below the network fee (0.0001 ICP); wait until your balance grows past the fee");
            };
            let transferAmount : Nat = balanceE8s - icpFee;

            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), 0.0);
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), 0.0);

            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = caller; subaccount = null };
                    amount = transferAmount;
                    fee = ?icpFee;
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
            let icpFee = await liveIcpFee(); // Audit F-009: live ledger fee
            if (amount <= icpFee) {
                return #Err("Accumulated balance below transfer fee");
            };
            let transferAmount : Nat = amount - icpFee;

            coverChargeBalance := 0;

            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = BACKEND_PRINCIPAL; subaccount = null };
                    amount = transferAmount;
                    fee = ?icpFee;
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

    // Paginated alternative to getAllGames for callers that risk the ~3 MiB
    // query response cap as the game history grows (Audit F-011). `offset` is
    // the 0-based starting gameId; `limit` caps the entry count. `total` is the
    // full game count (nextGameId) so callers can drive a paginator.
    public query func getAllGamesPage(offset : Nat, limit : Nat) : async {
        entries : [GameRecord];
        total : Nat;
    } {
        let total = nextGameId;
        if (offset >= total or limit == 0) {
            return { entries = []; total };
        };
        let endId = if (offset + limit > total) { total } else { offset + limit };
        var result = List.nil<GameRecord>();
        var id = offset;
        while (id < endId) {
            switch (natMap.get(gameRecords, id)) {
                case (?g) { result := List.push(g, result) };
                case (null) {};
            };
            id += 1;
        };
        { entries = List.toArray(List.reverse(result)); total };
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

    public shared ({ caller }) func getCanisterICPBalance() : async Nat {
        requireAdmin(caller);
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
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
            activeGameIds := natMap.put(activeGameIds, gameId, ()); // Audit F-002
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
            let selfPrincipal = Principal.fromActor(Self);
            let actual = try {
                await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
            } catch (e) {
                return #Err("Failed to read canister balance: " # Error.message(e));
            };

            var repaymentSum : Float = 0.0;
            for ((_, amount) in backerKeyMap.entries(backerRepayments)) {
                repaymentSum += amount;
            };
            let internalFloat = platformStats.potBalance + roundSeedReserve + repaymentSum;
            let internalE8s = Int.abs(Float.toInt(internalFloat * 100_000_000.0)) + coverChargeBalance;

            if (actual <= internalE8s) {
                return #Err("No untracked balance to sweep (actual=" # Nat.toText(actual) # " e8s, internal=" # Nat.toText(internalE8s) # " e8s)");
            };
            let untracked : Nat = actual - internalE8s;

            if (untracked <= Ledger.ICP_TRANSFER_FEE) {
                return #Err("Untracked balance below network fee (untracked=" # Nat.toText(untracked) # " e8s)");
            };
            let transferAmount : Nat = untracked - Ledger.ICP_TRANSFER_FEE;

            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = caller; subaccount = null };
                    amount = transferAmount;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
            } catch (e) {
                return #Err("Failed to contact ICP ledger: " # Error.message(e));
            };

            switch (transferResult) {
                case (#Err(err)) { #Err(transferErrorMessage(err)) };
                case (#Ok(blockIndex)) { #Ok(blockIndex) };
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
};
