import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Time "mo:base/Time";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import List "mo:base/List";
import Nat "mo:base/Nat";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Blob "mo:base/Blob";
import Error "mo:base/Error";
import Timer "mo:base/Timer";
import Array "mo:base/Array";
import Text "mo:base/Text";

import PpLedger "PpLedger";
import Subaccount "Subaccount";
import Migration "migration";

// TODO(2026-05-11): Rename "chips" terminology in this canister — depositChips,
// claimCashOut, chip subaccount, CashOutEntry, etc. — to non-casino verbiage
// (e.g. credits, PP units, tokens). Deferred from the ponzi_math extraction
// migration to keep that scope tight. See
// docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.

(with migration = Migration.runV3)
persistent actor Self {

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
        cancelled : Bool;          // set true after cancelCashOut succeeds
    };

    /// Mutable mint + economy configuration. All fields admin-tunable.
    public type MintConfig = {
        simple21DayPpPerIcp : Nat;    // initial 1000 (whole PP per ICP)
        compounding15DayPpPerIcp : Nat; // initial 2000
        compounding30DayPpPerIcp : Nat; // initial 3000
        backerPpPerIcp : Nat;          // initial 4000
        referralL1Bps : Nat;           // deprecated; unused by deductive cascade
        referralL2Bps : Nat;           // deprecated; unused by deductive cascade
        referralL3Bps : Nat;           // deprecated; unused by deductive cascade
        minDepositPp : Nat;            // initial 5000 (whole PP)
        cashOutDelaySeconds : Nat;     // initial 604_800
        observerIntervalSeconds : Nat; // initial 10
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
    };

    /// Per-backer cumulative ICP seen by the observer. Used to mint only
    /// on deltas when backers top up.
    public type BackerSeen = Float;

    /// Cumulative referral PP minted to a user from each downline level.
    /// Stored in PP-units (same scale as ledger balances).
    public type ReferralEarnings = {
        l1Units : Nat;
        l2Units : Nat;
        l3Units : Nat;
    };

    /// Per-tier downline counts and cumulative PP earnings for a user.
    /// Counts derive from a single pass over referralChain; earnings come
    /// from the local accumulator updated inside cascadeReferralMint.
    public type ReferralStats = {
        l1Count : Nat;
        l2Count : Nat;
        l3Count : Nat;
        l1Units : Nat;
        l2Units : Nat;
        l3Units : Nat;
    };

    // ================================================================
    // ponzi_math canister interface (query-only; observer polls)
    // ================================================================

    type PonziMathGamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    type PonziMathGameRecord = {
        id : Nat;
        player : Principal;
        plan : PonziMathGamePlan;
        amount : Float;
        startTime : Int;
        isCompounding : Bool;
        isActive : Bool;
        lastUpdateTime : Int;
        accumulatedEarnings : Float;
        totalWithdrawn : Float;
    };

    type PonziMathBackerType = { #seriesA; #seriesB };

    type PonziMathBackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : PonziMathBackerType;
        firstDepositDate : ?Int;
    };

    type PonziMathActor = actor {
        getAllGames : shared query () -> async [PonziMathGameRecord];
        getBackerPositions : shared query () -> async [PonziMathBackerPosition];
    };

    // ================================================================
    // State
    // ================================================================

    transient let natMap = OrderedMap.Make<Nat>(Nat.compare);
    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);
    transient let textMap = OrderedMap.Make<Text>(Text.compare);

    // Spell configs — PRESERVED across migration (admin-tunable spell definitions)
    var shenaniganConfigs = natMap.empty<ShenaniganConfig>();

    // Spell cast history — reset at migration; bounded to last 500 entries
    var shenanigans = natMap.empty<ShenaniganRecord>();
    var shenaniganStats = principalMap.empty<ShenaniganStats>();
    var nextShenaniganId : Nat = 0;

    // Admin state
    var adminPrincipal : ?Principal = null;
    var ponziMathPrincipal : ?Principal = null;

    // Additional principals with admin access. Augments the legacy
    // adminPrincipal so multiple browser wallets can drive the admin
    // panel directly. Hard-coded — to add/remove an admin, edit and
    // redeploy. transient let → no state schema impact, recomputed
    // on every canister start.
    transient let extraAdmins : [Principal] = [
        Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe"),
        Principal.fromText("stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe"),
        Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae"),
    ];

    // Referral chain — moved from backend during the ponzi_math extraction.
    // Maps user → who referred them. First-wins, one-time, immutable per user.
    // Referrals are PP-economy metadata; not money math.
    var referralChain = principalMap.empty<Principal>();

    // Cumulative referral PP minted to each upline user, broken out per
    // downline tier. Incremented inside cascadeReferralMint on successful
    // mints; reads feed getReferralStats. Starts empty on first upgrade
    // (we don't backfill from historic ledger memos).
    var referralEarnings = principalMap.empty<ReferralEarnings>();

    // ────────────────────────────────────────────────────────────────
    // Deductive-cascade state (added 2026-05-16)
    // ────────────────────────────────────────────────────────────────

    // Catch-all upline + residual destination ("Charles"). Initialized
    // to the deploying admin on first init via seedMigrationV2 when null;
    // admin can override via setHousePrincipal.
    var housePrincipal : ?Principal = null;

    // Per-principal signup-gift claim time. Empty = never claimed.
    // Doubles as the "join time" surfaced via getReferralStats.recentSignups.
    var signupGiftClaimed = principalMap.empty<Int>();

    // Per-principal time of last qualifying deposit (≥ 0.1 ICP). Drives
    // isActive() without per-cascade inter-canister calls. Populated by
    // the observer on every qualifying mint event.
    var lastQualifyingDeposit = principalMap.empty<Int>();

    // Bidirectional map of short referral codes ↔ principals. Codes are
    // assigned lazily on first call to getOrCreateReferralCode and never
    // change. URLs ship as `?ref=<code>` instead of the 53-char principal.
    var referralCodeToPrincipal = textMap.empty<Principal>();
    var principalToReferralCode = principalMap.empty<Text>();

    // Mint + economy configuration (mutable, admin-tunable)
    var mintConfig : MintConfig = {
        simple21DayPpPerIcp = 1000;
        compounding15DayPpPerIcp = 2000;
        compounding30DayPpPerIcp = 3000;
        backerPpPerIcp = 4000;
        referralL1Bps = 800;
        referralL2Bps = 500;
        referralL3Bps = 200;
        minDepositPp = 5000;
        cashOutDelaySeconds = 604_800;
        observerIntervalSeconds = 10;
        cascadeInitialBps = 1000;
        cascadePassthroughBps = 5000;
        signupGiftPp = 500;
        activityRequiresDeposit = true;
        activityWindowDays = null;
    };

    // Observer cursors
    var gameIdCursor : Nat = 0;                         // next unprocessed game id
    var backerSeen = principalMap.empty<BackerSeen>();  // cumulative ICP minted-for per backer

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
    // Legacy stable fields — preserved across upgrades so we don't
    // implicitly discard data still living in the deployed canister.
    // Not referenced by current code; a future migration should
    // properly transform or drop these. The literal initializers
    // below are only used on first deploy — existing values are
    // preserved across upgrade by orthogonal persistence.
    // ================================================================
    let CASCADE_MAX_DEPTH : Nat = 0;
    var activeDepositors = principalMap.empty<Bool>();
    var cascadeBps : Nat = 0;
    var cascadePassthrough : Nat = 0;
    var charlesPrincipal : Principal = Principal.fromText("aaaaa-aa");
    var referrerToDownline = principalMap.empty<List.List<Principal>>();
    var signupGiftPp : Nat = 0;

    // ================================================================
    // Initialization
    // ================================================================

    public shared ({ caller }) func initialize(ponziMathCanisterId : Principal) : async () {
        switch (adminPrincipal) {
            case (null) {
                adminPrincipal := ?caller;
                ponziMathPrincipal := ?ponziMathCanisterId;
                if (natMap.size(shenaniganConfigs) == 0) {
                    initializeDefaultShenanigans();
                };
                startObserver();
            };
            case (?admin) {
                if (caller != admin) {
                    Debug.trap("Already initialized. Only admin can reconfigure.");
                };
                ponziMathPrincipal := ?ponziMathCanisterId;
            };
        };
    };

    /// Idempotent referral registration. First call sets the chain entry;
    /// subsequent calls for the same caller are no-ops. Self-referral rejected.
    public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
        if (Principal.isAnonymous(caller)) { Debug.trap("Anonymous principal not allowed") };
        if (caller == referrer) { return };
        switch (principalMap.get(referralChain, caller)) {
            case (?_) { /* already set */ };
            case null { referralChain := principalMap.put(referralChain, caller, referrer) };
        };
    };

    /// One-hop lookup — returns the user's immediate referrer (L1) or null.
    public query func getReferrer(user : Principal) : async ?Principal {
        principalMap.get(referralChain, user);
    };

    /// Per-tier downline counts and cumulative PP earnings for `user`.
    /// Counts are computed by a single pass over the referral chain map;
    /// earnings come from the local accumulator.
    public query func getReferralStats(user : Principal) : async ReferralStats {
        var l1 : Nat = 0;
        var l2 : Nat = 0;
        var l3 : Nat = 0;
        for ((_, l1Ref) in principalMap.entries(referralChain)) {
            if (Principal.equal(l1Ref, user)) {
                l1 += 1;
            } else {
                switch (principalMap.get(referralChain, l1Ref)) {
                    case (?l2Ref) {
                        if (Principal.equal(l2Ref, user)) {
                            l2 += 1;
                        } else {
                            switch (principalMap.get(referralChain, l2Ref)) {
                                case (?l3Ref) {
                                    if (Principal.equal(l3Ref, user)) {
                                        l3 += 1;
                                    };
                                };
                                case null {};
                            };
                        };
                    };
                    case null {};
                };
            };
        };
        let earnings = switch (principalMap.get(referralEarnings, user)) {
            case (?e) { e };
            case null { { l1Units = 0; l2Units = 0; l3Units = 0 } };
        };
        {
            l1Count = l1;
            l2Count = l2;
            l3Count = l3;
            l1Units = earnings.l1Units;
            l2Units = earnings.l2Units;
            l3Units = earnings.l3Units;
        };
    };

    // Base62 alphabet — 0-9, a-z, A-Z. Used for short referral codes.
    transient let BASE62_CHARS : [Char] = [
        '0','1','2','3','4','5','6','7','8','9',
        'a','b','c','d','e','f','g','h','i','j',
        'k','l','m','n','o','p','q','r','s','t',
        'u','v','w','x','y','z',
        'A','B','C','D','E','F','G','H','I','J',
        'K','L','M','N','O','P','Q','R','S','T',
        'U','V','W','X','Y','Z',
    ];
    transient let REFERRAL_CODE_LEN : Nat = 6;
    transient let CASCADE_DEPTH_CAP : Nat = 10;

    func natToBase62(input : Nat, length : Nat) : Text {
        var modulus : Nat = 1;
        var i : Nat = 0;
        while (i < length) { modulus *= 62; i += 1 };
        var current = input % modulus;
        var result : Text = "";
        var j : Nat = 0;
        while (j < length) {
            result := Text.fromChar(BASE62_CHARS[current % 62]) # result;
            current /= 62;
            j += 1;
        };
        result;
    };

    /// Issue (or return existing) short referral code for the caller.
    /// Deterministic on the principal + time-derived nonce; retries on the
    /// astronomically-unlikely collision. Codes are stable once assigned.
    public shared ({ caller }) func getOrCreateReferralCode() : async Text {
        if (Principal.isAnonymous(caller)) {
            Debug.trap("Anonymous principal not allowed");
        };
        switch (principalMap.get(principalToReferralCode, caller)) {
            case (?existing) { return existing };
            case null {};
        };
        let pHash : Nat = Nat32.toNat(Principal.hash(caller));
        let tHash : Nat = Int.abs(Time.now());
        var attempt : Nat = 0;
        loop {
            let seed : Nat = pHash + tHash + attempt;
            let candidate = natToBase62(seed, REFERRAL_CODE_LEN);
            switch (textMap.get(referralCodeToPrincipal, candidate)) {
                case (?_) { attempt += 1 };
                case null {
                    referralCodeToPrincipal := textMap.put(referralCodeToPrincipal, candidate, caller);
                    principalToReferralCode := principalMap.put(principalToReferralCode, caller, candidate);
                    return candidate;
                };
            };
        };
    };

    /// Look up the principal a short referral code resolves to. Returns null
    /// for unknown codes. Used by the frontend to translate `?ref=<code>`
    /// into the principal we register against the downline chain.
    public query func resolveReferralCode(code : Text) : async ?Principal {
        textMap.get(referralCodeToPrincipal, code);
    };

    func bumpReferralEarnings(upline : Principal, level : Nat, units : Nat) {
        if (units == 0) return;
        let current : ReferralEarnings = switch (principalMap.get(referralEarnings, upline)) {
            case (?e) { e };
            case null { { l1Units = 0; l2Units = 0; l3Units = 0 } };
        };
        let updated : ReferralEarnings = switch (level) {
            case 1 { { current with l1Units = current.l1Units + units } };
            case 2 { { current with l2Units = current.l2Units + units } };
            case _ { { current with l3Units = current.l3Units + units } };
        };
        referralEarnings := principalMap.put(referralEarnings, upline, updated);
    };

    // Resolve the house (catch-all) principal. Falls back to admin if
    // housePrincipal hasn't been seeded yet (defensive — seedMigrationV2
    // initializes it).
    func house() : Principal {
        switch (housePrincipal) {
            case (?p) { p };
            case (null) {
                switch (adminPrincipal) {
                    case (?p) { p };
                    case (null) { Debug.trap("housePrincipal not initialized and no admin set") };
                };
            };
        };
    };

    // v1: referralChain.get(current) ?? house(). v2 will swap this to
    // NFT-ownership lookup — keep the function signature stable.
    func getPayoutTarget(current : Principal) : Principal {
        switch (principalMap.get(referralChain, current)) {
            case (?p) { p };
            case (null) { house() };
        };
    };

    // True when the principal meets the configured activity bar.
    // Hot-path: called once per cascade hop. Reads lastQualifyingDeposit
    // (populated by observer) — no inter-canister call here.
    func isActive(p : Principal) : Bool {
        if (not mintConfig.activityRequiresDeposit) { return true };
        switch (principalMap.get(lastQualifyingDeposit, p)) {
            case (null) { false };
            case (?ts) {
                switch (mintConfig.activityWindowDays) {
                    case (null) { true };
                    case (?days) {
                        let now = Time.now();
                        let windowNs : Int = days * 86_400 * 1_000_000_000;
                        (now - ts) <= windowNs;
                    };
                };
            };
        };
    };

    func requireAdmin(caller : Principal) {
        switch (adminPrincipal) {
            case (null) { Debug.trap("Not initialized") };
            case (?admin) {
                if (caller == admin) return;
                for (extra in extraAdmins.vals()) {
                    if (caller == extra) return;
                };
                Debug.trap("Unauthorized: admin only");
            };
        };
    };

    public shared ({ caller }) func rotateAdmin(newAdmin : Principal) : async () {
        requireAdmin(caller);
        adminPrincipal := ?newAdmin;
    };

    func getPonziMath() : PonziMathActor {
        switch (ponziMathPrincipal) {
            case (null) { Debug.trap("ponzi_math canister not configured") };
            case (?p) { actor (Principal.toText(p)) : PonziMathActor };
        };
    };

    // ================================================================
    // Observer (polling timer)
    // ================================================================

    func startObserver<system>() {
        switch (observerTimerId) {
            case (?tid) { Timer.cancelTimer(tid) };
            case (null) {};
        };
        let interval : Nat = mintConfig.observerIntervalSeconds;
        let tid = Timer.recurringTimer<system>(#seconds(interval), observerTick);
        observerTimerId := ?tid;
    };

    /// One observer pass. Mints PP for new deposits and dealer top-ups.
    /// Advances cursors only after successful mint to guarantee at-least-once
    /// minting with ledger-level dedup (via created_at_time + memo) preventing
    /// duplicates.
    func observerTick() : async () {
        if (observerRunning) return;
        observerRunning := true;
        try {
            await processNewGames();
            await processBackerDeltas();
        } catch (e) {
            Debug.print("Observer tick error: " # Error.message(e));
        };
        observerRunning := false;
    };

    func processNewGames() : async () {
        let ponziMath = getPonziMath();
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        let sorted = Array.sort<PonziMathGameRecord>(games, func(a, b) = Nat.compare(a.id, b.id));
        for (game in sorted.vals()) {
            if (game.id >= gameIdCursor) {
                let ppPerIcp = switch (game.plan) {
                    case (#simple21Day) { mintConfig.simple21DayPpPerIcp };
                    case (#compounding15Day) { mintConfig.compounding15DayPpPerIcp };
                    case (#compounding30Day) { mintConfig.compounding30DayPpPerIcp };
                };
                let units = icpFloatToPpUnits(game.amount, ppPerIcp);
                let eventId = "game-" # Nat.toText(game.id);
                let res = await mintInternal(game.player, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(game.player, units, eventId);
                        gameIdCursor := game.id + 1;
                    };
                    case (#Err(msg)) {
                        Debug.print("Mint failed for " # eventId # ": " # msg);
                        return;
                    };
                };
            };
        };
    };

    func processBackerDeltas() : async () {
        let ponziMath = getPonziMath();
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            let seen : Float = switch (principalMap.get(backerSeen, backer.owner)) {
                case (null) { 0.0 };
                case (?v) { v };
            };
            if (backer.amount > seen) {
                let delta : Float = backer.amount - seen;
                let units = icpFloatToPpUnits(delta, mintConfig.backerPpPerIcp);
                let eventId = "backer-" # Principal.toText(backer.owner) # "-"
                    # Float.toText(backer.amount);
                let res = await mintInternal(backer.owner, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(backer.owner, units, eventId);
                        backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                    };
                    case (#Err(msg)) {
                        Debug.print("Backer mint failed: " # msg);
                    };
                };
            };
        };
    };

    /// One-shot catch-up primer. Admin only. Call immediately after the
    /// cutover upgrade completes, before unpausing user traffic.
    public shared ({ caller }) func primeObserverCursors() : async () {
        requireAdmin(caller);
        let ponziMath = getPonziMath();
        let games = await ponziMath.getAllGames();
        var maxId : Nat = 0;
        for (g in games.vals()) { if (g.id >= maxId) { maxId := g.id + 1 } };
        gameIdCursor := maxId;

        let backers = await ponziMath.getBackerPositions();
        for (b in backers.vals()) {
            backerSeen := principalMap.put(backerSeen, b.owner, b.amount);
        };
    };

    // ================================================================
    // Chip custody — deposit / cash-out
    // ================================================================

    /// Pull `amountUnits` PP-units from the caller's wallet into their
    /// chip subaccount. Caller must have signed icrc2_approve on pp_ledger
    /// beforehand.
    public shared ({ caller }) func depositChips(amountUnits : Nat) : async { #Ok : Nat; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        let minUnits = ppToUnits(mintConfig.minDepositPp);
        if (amountUnits < minUnits) {
            return #Err("Minimum deposit is " # Nat.toText(mintConfig.minDepositPp) # " PP");
        };
        try {
            let res = await ppLedger.icrc2_transfer_from({
                spender_subaccount = null;
                from = { owner = caller; subaccount = null };
                to = {
                    owner = Principal.fromActor(Self);
                    subaccount = ?Subaccount.principalToChipSubaccount(caller);
                };
                amount = amountUnits;
                fee = ?0;
                memo = ?Text.encodeUtf8("chip-deposit");
                created_at_time = ?nowNat64();
            });
            switch (res) {
                case (#Ok(idx)) { #Ok(idx) };
                case (#Err(#InsufficientAllowance(_))) {
                    #Err("Approve shenanigans on pp_ledger first");
                };
                case (#Err(#InsufficientFunds({ balance }))) {
                    #Err("Wallet balance too low (" # Nat.toText(balance) # " units)");
                };
                case (#Err(e)) { #Err(describeTransferFromErr(e)) };
            };
        } catch (e) {
            #Err("ppLedger call failed: " # Error.message(e));
        };
    };

    func describeTransferFromErr(err : PpLedger.TransferFromError) : Text {
        switch (err) {
            case (#BadFee({ expected_fee })) { "BadFee expected=" # Nat.toText(expected_fee) };
            case (#BadBurn({ min_burn_amount })) { "BadBurn min=" # Nat.toText(min_burn_amount) };
            case (#InsufficientFunds({ balance })) { "InsufficientFunds balance=" # Nat.toText(balance) };
            case (#InsufficientAllowance({ allowance })) { "InsufficientAllowance=" # Nat.toText(allowance) };
            case (#TooOld) { "TooOld" };
            case (#CreatedInFuture(_)) { "CreatedInFuture" };
            case (#Duplicate({ duplicate_of })) { "Duplicate of=" # Nat.toText(duplicate_of) };
            case (#TemporarilyUnavailable) { "TemporarilyUnavailable" };
            case (#GenericError({ message; error_code = _ })) { "GenericError: " # message };
        };
    };

    public shared ({ caller }) func requestCashOut(amountUnits : Nat) : async { #Ok : Nat; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        if (amountUnits == 0) { return #Err("Amount must be positive") };

        var pending : Nat = 0;
        for (entry in natMap.vals(cashOuts)) {
            if (entry.player == caller and not entry.claimed and not entry.cancelled) {
                pending += entry.amount;
            };
        };

        let chipBalance = await ppLedger.icrc1_balance_of({
            owner = Principal.fromActor(Self);
            subaccount = ?Subaccount.principalToChipSubaccount(caller);
        });
        if (pending + amountUnits > chipBalance) {
            return #Err("Requested amount exceeds unqueued chip balance");
        };

        let id = nextCashOutId;
        nextCashOutId += 1;
        let claimableAfter : Int = Time.now() + (mintConfig.cashOutDelaySeconds * 1_000_000_000);
        let entry : CashOutEntry = {
            id;
            player = caller;
            amount = amountUnits;
            claimableAfter;
            claimed = false;
            cancelled = false;
        };
        cashOuts := natMap.put(cashOuts, id, entry);
        #Ok(id);
    };

    public shared ({ caller }) func claimCashOut(id : Nat) : async { #Ok : Nat; #Err : Text } {
        let entry = switch (natMap.get(cashOuts, id)) {
            case (null) { return #Err("No such cash-out") };
            case (?e) { e };
        };
        if (entry.player != caller) { return #Err("Not your cash-out") };
        if (entry.claimed) { return #Err("Already claimed") };
        if (Time.now() < entry.claimableAfter) {
            return #Err("Claim not yet unlocked");
        };

        let chipBalance = await ppLedger.icrc1_balance_of({
            owner = Principal.fromActor(Self);
            subaccount = ?Subaccount.principalToChipSubaccount(caller);
        });
        let payable : Nat = if (chipBalance < entry.amount) { chipBalance } else { entry.amount };
        if (payable == 0) {
            cashOuts := natMap.put(cashOuts, id, { entry with claimed = true });
            return #Err("No chips left to cash out");
        };

        try {
            let res = await ppLedger.icrc1_transfer({
                from_subaccount = ?Subaccount.principalToChipSubaccount(caller);
                to = { owner = caller; subaccount = null };
                amount = payable;
                fee = ?0;
                memo = ?Text.encodeUtf8("cash-out-" # Nat.toText(id));
                created_at_time = ?nowNat64();
            });
            switch (res) {
                case (#Ok(idx)) {
                    cashOuts := natMap.put(cashOuts, id, { entry with claimed = true });
                    #Ok(idx);
                };
                case (#Err(e)) { #Err(describeTransferErr(e)) };
            };
        } catch (e) {
            #Err("ppLedger call failed: " # Error.message(e));
        };
    };

    public shared ({ caller }) func cancelCashOut(id : Nat) : async { #Ok; #Err : Text } {
        let entry = switch (natMap.get(cashOuts, id)) {
            case (null) { return #Err("No such cash-out") };
            case (?e) { e };
        };
        if (entry.player != caller) { return #Err("Not your cash-out") };
        if (entry.claimed) { return #Err("Already claimed") };
        if (entry.cancelled) { return #Err("Already cancelled") };

        cashOuts := natMap.put(cashOuts, id, { entry with cancelled = true });
        #Ok;
    };

    /// Pending and recently claimed cash-outs for a given user.
    public query func getCashOutsFor(user : Principal) : async [CashOutEntry] {
        let all = Iter.toArray(natMap.vals(cashOuts));
        Array.filter<CashOutEntry>(all, func(e) { e.player == user and not e.cancelled });
    };

    public shared query ({ caller }) func getMyCashOuts() : async [CashOutEntry] {
        let all = Iter.toArray(natMap.vals(cashOuts));
        Array.filter<CashOutEntry>(all, func(e) { e.player == caller and not e.cancelled });
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
    // Mint engine (internal helpers)
    // ================================================================

    func nowNat64() : Nat64 {
        Nat64.fromNat(Int.abs(Time.now()));
    };

    /// Convert whole PP → PP-units.
    func ppToUnits(pp : Nat) : Nat { pp * PpLedger.PP_UNIT_SCALE };

    /// ICP-float * (PP per ICP) → PP-units.
    /// Example: 1.0 ICP * 1000 PP/ICP = 1000 whole PP = 10^11 PP-units.
    func icpFloatToPpUnits(icp : Float, ppPerIcp : Nat) : Nat {
        if (icp <= 0.0) return 0;
        let icpE8s : Nat = Int.abs(Float.toInt(icp * 100_000_000.0));
        icpE8s * ppPerIcp;
    };

    /// Mint PP-units to a player's chip subaccount.
    /// Returns #Ok(blockIndex) or #Err(text).
    func mintInternal(player : Principal, amount : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
        if (amount == 0) { return #Ok(0) };
        let memo = ?Text.encodeUtf8(memoText);
        try {
            let res = await ppLedger.icrc1_transfer({
                from_subaccount = null;
                to = {
                    owner = Principal.fromActor(Self);
                    subaccount = ?Subaccount.principalToChipSubaccount(player);
                };
                amount;
                fee = null;
                memo;
                created_at_time = ?nowNat64();
            });
            switch (res) {
                case (#Ok(idx)) { #Ok(idx) };
                case (#Err(#Duplicate { duplicate_of })) { #Ok(duplicate_of) };
                case (#Err(e)) { #Err(describeTransferErr(e)) };
            };
        } catch (e) {
            #Err("ppLedger call failed: " # Error.message(e));
        };
    };

    func describeTransferErr(err : PpLedger.TransferError) : Text {
        switch (err) {
            case (#BadFee({ expected_fee })) { "BadFee expected=" # Nat.toText(expected_fee) };
            case (#BadBurn({ min_burn_amount })) { "BadBurn min=" # Nat.toText(min_burn_amount) };
            case (#InsufficientFunds({ balance })) { "InsufficientFunds balance=" # Nat.toText(balance) };
            case (#TooOld) { "TooOld" };
            case (#CreatedInFuture(_)) { "CreatedInFuture" };
            case (#Duplicate({ duplicate_of })) { "Duplicate of=" # Nat.toText(duplicate_of) };
            case (#TemporarilyUnavailable) { "TemporarilyUnavailable" };
            case (#GenericError({ message; error_code = _ })) { "GenericError: " # message };
        };
    };

    /// Burn PP-units from a chip subaccount (transfer to minting account).
    func burnFrom(player : Principal, units : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
        if (units == 0) { return #Ok(0) };
        try {
            let res = await ppLedger.icrc1_transfer({
                from_subaccount = ?Subaccount.principalToChipSubaccount(player);
                to = { owner = Principal.fromActor(Self); subaccount = null };
                amount = units;
                fee = null;
                memo = ?Text.encodeUtf8(memoText);
                created_at_time = ?nowNat64();
            });
            switch (res) {
                case (#Ok(idx)) { #Ok(idx) };
                case (#Err(e)) { #Err(describeTransferErr(e)) };
            };
        } catch (e) {
            #Err("ppLedger call failed: " # Error.message(e));
        };
    };

    /// Chip-to-chip transfer (between two player subaccounts).
    func chipTransfer(from : Principal, to : Principal, units : Nat, memoText : Text) : async { #Ok : Nat; #Err : Text } {
        if (units == 0) { return #Ok(0) };
        try {
            let res = await ppLedger.icrc1_transfer({
                from_subaccount = ?Subaccount.principalToChipSubaccount(from);
                to = {
                    owner = Principal.fromActor(Self);
                    subaccount = ?Subaccount.principalToChipSubaccount(to);
                };
                amount = units;
                fee = ?0;
                memo = ?Text.encodeUtf8(memoText);
                created_at_time = ?nowNat64();
            });
            switch (res) {
                case (#Ok(idx)) { #Ok(idx) };
                case (#Err(e)) { #Err(describeTransferErr(e)) };
            };
        } catch (e) {
            #Err("ppLedger call failed: " # Error.message(e));
        };
    };

    func getChipBalance(player : Principal) : async Nat {
        await ppLedger.icrc1_balance_of({
            owner = Principal.fromActor(Self);
            subaccount = ?Subaccount.principalToChipSubaccount(player);
        });
    };

    /// For each of L1/L2/L3, mint referral PP-units derived from the base mint.
    /// Memo tags `referral-LN-<eventId>` so dedup works per-level per-event.
    /// Lookups are local — referralChain lives in this canister.
    /// Successful mints (and ledger-duplicate replays, which mintInternal
    /// promotes to #Ok) bump the per-upline earnings accumulator so
    /// getReferralStats has a cheap read.
    func cascadeReferralMint(originUser : Principal, baseUnits : Nat, eventId : Text) : async () {
        if (baseUnits == 0) return;
        let l1Maybe = principalMap.get(referralChain, originUser);
        switch (l1Maybe) {
            case (null) {};
            case (?l1) {
                let l1Units = baseUnits * mintConfig.referralL1Bps / 10_000;
                switch (await mintInternal(l1, l1Units, "referral-L1-" # eventId)) {
                    case (#Ok(_)) { bumpReferralEarnings(l1, 1, l1Units) };
                    case (#Err(_)) {};
                };
                let l2Maybe = principalMap.get(referralChain, l1);
                switch (l2Maybe) {
                    case (null) {};
                    case (?l2) {
                        let l2Units = baseUnits * mintConfig.referralL2Bps / 10_000;
                        switch (await mintInternal(l2, l2Units, "referral-L2-" # eventId)) {
                            case (#Ok(_)) { bumpReferralEarnings(l2, 2, l2Units) };
                            case (#Err(_)) {};
                        };
                        let l3Maybe = principalMap.get(referralChain, l2);
                        switch (l3Maybe) {
                            case (null) {};
                            case (?l3) {
                                let l3Units = baseUnits * mintConfig.referralL3Bps / 10_000;
                                switch (await mintInternal(l3, l3Units, "referral-L3-" # eventId)) {
                                    case (#Ok(_)) { bumpReferralEarnings(l3, 3, l3Units) };
                                    case (#Err(_)) {};
                                };
                            };
                        };
                    };
                };
            };
        };
    };

    // ================================================================
    // Core Logic
    // ================================================================

    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };

        let config = switch (getConfigForType(shenaniganType)) {
            case (null) { Debug.trap("Unknown shenanigan type") };
            case (?c) { c };
        };
        let costUnits = ppToUnits(Int.abs(Float.toInt(config.cost)));

        let balance = await getChipBalance(caller);
        if (balance < costUnits) { Debug.trap("Insufficient chips to cast this shenanigan") };

        let castId = nextShenaniganId;
        let burnMemo = "cast-" # Nat.toText(castId);
        switch (await burnFrom(caller, costUnits, burnMemo)) {
            case (#Err(msg)) { Debug.trap("Burn failed: " # msg) };
            case (#Ok(_)) {};
        };

        let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) {
            case (null) { 0 };
            case (?n) { n };
        };
        ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + costUnits);

        let outcome = determineOutcome(shenaniganType);

        if (outcome == #backfire) {
            switch (shenaniganType) {
                case (#moneyTrickster) {
                    switch (target) {
                        case (null) {};
                        case (?targetP) {
                            let casterBal = await getChipBalance(caller);
                            let pct = 2 + (Int.abs(Time.now()) % 7);
                            let raw = casterBal * pct / 100;
                            let capped = if (raw > ppToUnits(250)) { ppToUnits(250) } else { raw };
                            let _ = await chipTransfer(caller, targetP, capped, "backfire-" # Nat.toText(castId));
                        };
                    };
                };
                case (#aoeSkim) {
                    let casterBal = await getChipBalance(caller);
                    let pct = 1 + (Int.abs(Time.now()) % 3);
                    let loss = casterBal * pct / 100;
                    let _ = await burnFrom(caller, loss, "backfire-aoe-" # Nat.toText(castId));
                };
                case (#downlineHeist) {
                    switch (target) {
                        case (null) {};
                        case (?t) {
                            Debug.print("Backfire: " # Principal.toText(caller) # " loses L3 downline to " # Principal.toText(t));
                        };
                    };
                };
                case (_) {};
            };
        };

        nextShenaniganId += 1;
        let newShenanigan : ShenaniganRecord = {
            id = castId;
            user = caller;
            shenaniganType;
            target;
            outcome;
            timestamp = Time.now();
            cost = config.cost;
        };
        shenanigans := natMap.put(shenanigans, castId, newShenanigan);
        updateShenaniganStats(caller, config.cost, outcome);
        if (outcome == #success or outcome == #backfire) {
            let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
                case (null) { 0 };
                case (?n) { n };
            };
            spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
        };

        outcome;
    };

    func getConfigForType(t : ShenaniganType) : ?ShenaniganConfig {
        let id : Nat = switch (t) {
            case (#moneyTrickster) { 0 };
            case (#aoeSkim) { 1 };
            case (#renameSpell) { 2 };
            case (#mintTaxSiphon) { 3 };
            case (#downlineHeist) { 4 };
            case (#magicMirror) { 5 };
            case (#ppBoosterAura) { 6 };
            case (#purseCutter) { 7 };
            case (#whaleRebalance) { 8 };
            case (#downlineBoost) { 9 };
            case (#goldenName) { 10 };
        };
        natMap.get(shenaniganConfigs, id);
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
            // dealerCut is kept in stats for UI continuity, but since backend no
            // longer tracks PP dealer pools (Open Question 1 resolution: dealer
            // repayment is ICP-only) this number is purely informational.
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
    // Leaderboard
    // ================================================================

    /// Top-N players by cumulative PP burned. Returns (principal, PP-units).
    public query func getTopPpBurners(limit : Nat) : async [(Principal, Nat)] {
        let entries = Iter.toArray(principalMap.entries(ppBurnedPerPlayer));
        let sorted = Array.sort<(Principal, Nat)>(
            entries,
            func(a, b) = Nat.compare(b.1, a.1),
        );
        let cap = if (limit < sorted.size()) { limit } else { sorted.size() };
        Array.subArray(sorted, 0, cap);
    };

    /// Top-N players by number of spells cast (success + backfire).
    public query func getTopSpellCasters(limit : Nat) : async [(Principal, Nat)] {
        let entries = Iter.toArray(principalMap.entries(spellsCastPerPlayer));
        let sorted = Array.sort<(Principal, Nat)>(
            entries,
            func(a, b) = Nat.compare(b.1, a.1),
        );
        let cap = if (limit < sorted.size()) { limit } else { sorted.size() };
        Array.subArray(sorted, 0, cap);
    };

    public query func getPpBurnedFor(user : Principal) : async Nat {
        switch (principalMap.get(ppBurnedPerPlayer, user)) {
            case (null) { 0 };
            case (?n) { n };
        };
    };

    // ================================================================
    // Admin tunables
    // ================================================================

    public query func getMintConfig() : async MintConfig { mintConfig };

    /// Current observer state — running/paused, cursor positions, and interval.
    /// Surfaced in the admin panel for operational visibility.
    public query func getObserverStatus() : async {
        running : Bool;
        gameIdCursor : Nat;
        backerSeenCount : Nat;
        intervalSeconds : Nat;
    } {
        {
            running = observerTimerId != null;
            gameIdCursor;
            backerSeenCount = principalMap.size(backerSeen);
            intervalSeconds = mintConfig.observerIntervalSeconds;
        };
    };

    /// Admin-triggered manual PP issuance (direct mint to the player's chip
    /// subaccount). Use for fixups, comps, or seeding test accounts.
    public shared ({ caller }) func adminMint(to : Principal, wholePp : Nat) : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);
        await mintInternal(to, ppToUnits(wholePp), "admin-mint-" # Principal.toText(to));
    };

    public shared ({ caller }) func setSimple21DayPpPerIcp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with simple21DayPpPerIcp = v };
    };
    public shared ({ caller }) func setCompounding15DayPpPerIcp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with compounding15DayPpPerIcp = v };
    };
    public shared ({ caller }) func setCompounding30DayPpPerIcp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with compounding30DayPpPerIcp = v };
    };
    public shared ({ caller }) func setBackerPpPerIcp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with backerPpPerIcp = v };
    };
    public shared ({ caller }) func setReferralBps(l1 : Nat, l2 : Nat, l3 : Nat) : async () {
        requireAdmin(caller);
        mintConfig := {
            mintConfig with
            referralL1Bps = l1;
            referralL2Bps = l2;
            referralL3Bps = l3;
        };
    };
    public shared ({ caller }) func setMinDepositPp(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with minDepositPp = v };
    };
    public shared ({ caller }) func setCashOutDelaySeconds(v : Nat) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with cashOutDelaySeconds = v };
    };
    public shared ({ caller }) func setObserverIntervalSeconds(v : Nat) : async () {
        requireAdmin(caller);
        if (v < 1) { Debug.trap("Interval must be >= 1 second") };
        mintConfig := { mintConfig with observerIntervalSeconds = v };
        startObserver();
    };

    public shared ({ caller }) func stopObserver() : async () {
        requireAdmin(caller);
        switch (observerTimerId) {
            case (?tid) { Timer.cancelTimer(tid); observerTimerId := null };
            case (null) {};
        };
    };

    public shared ({ caller }) func resumeObserver() : async () {
        requireAdmin(caller);
        startObserver();
    };

    /// Manual one-shot observer tick (admin debug).
    public shared ({ caller }) func runObserverOnce() : async () {
        requireAdmin(caller);
        await observerTick();
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
