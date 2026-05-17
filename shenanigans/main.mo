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

// TODO(2026-05-11): Rename "chips" terminology in this canister — depositChips,
// claimCashOut, chip subaccount, CashOutEntry, etc. — to non-casino verbiage
// (e.g. credits, PP units, tokens). Deferred from the ponzi_math extraction
// migration to keep that scope tight. See
// docs/superpowers/specs/2026-05-11-ponzi-math-extraction-design.md.

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
        referralL1Bps : Nat;           // basis points; initial 800 (= 8%)
        referralL2Bps : Nat;           // initial 500
        referralL3Bps : Nat;           // initial 200
        minDepositPp : Nat;            // initial 5000 (whole PP)
        cashOutDelaySeconds : Nat;     // initial 604_800
        observerIntervalSeconds : Nat; // initial 10
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

    /// Active spell-effect state, all keyed by affected principal.
    /// `expiresAt` is nanoseconds (Time.now() compatible). Entries are
    /// lazily cleaned — a query returning null/false for an expired entry
    /// is fine, and writers may overwrite expired entries in place.

    /// Rename Spell — overrides display name for a window.
    public type DisplayNameOverride = { name : Text; expiresAt : Int };

    /// Mint Tax Siphon — `siphoner` skims `pctTimes100` basis-points-of-100
    /// from each mint to `target` until `expiresAt` or `siphonedSoFar`
    /// reaches `capUnits`.
    public type MintSiphon = {
        siphoner : Principal;
        expiresAt : Int;
        pctTimes100 : Nat;  // e.g. 500 = 5%
        capUnits : Nat;
        siphonedSoFar : Nat;
    };

    /// Magic Mirror — `chargesRemaining` hostile-spell deflections before
    /// the shield drops. Backfire-mode hostile spells still hit the caster
    /// regardless.
    public type ShieldState = { chargesRemaining : Nat; expiresAt : Int };

    /// PP Booster Aura — multiplies observer mints to this user by
    /// `multiplierBps / 10_000`. e.g. 11_500 = 1.15x.
    public type MintMultiplier = { multiplierBps : Nat; expiresAt : Int };

    /// Downline Boost — multiplies referral cascade mints whose upline is
    /// this user (any tier) by `multiplierBps / 10_000`.
    public type CascadeBoost = { multiplierBps : Nat; expiresAt : Int };

    /// Snapshot of every active spell effect for one user — fed to the UI.
    public type ActiveSpellEffects = {
        shield : ?ShieldState;
        mintMultiplier : ?MintMultiplier;
        cascadeBoost : ?CascadeBoost;
        displayName : ?DisplayNameOverride;
        mintSiphon : ?MintSiphon;
        golden : Bool;
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
    };

    // Observer cursors
    var gameIdCursor : Nat = 0;                         // next unprocessed game id
    var backerSeen = principalMap.empty<BackerSeen>();  // cumulative ICP minted-for per backer

    // Observer lock to prevent concurrent ticks
    transient var observerRunning : Bool = false;
    var observerTimerId : ?Timer.TimerId = null;

    // Per-game mint retry counters. A failed mintWithEffects (#Err) increments;
    // a successful mint clears the entry. After MAX_MINT_RETRIES consecutive
    // failures, the observer gives up and advances past the game, recording it
    // in missedGameMints for admin inspection / manual retry. Transient: on
    // upgrade the counter resets to 0, which is fine — next tick re-tries.
    transient var gameMintRetries = natMap.empty<Nat>();
    transient let MAX_MINT_RETRIES : Nat = 10;  // ~100s @ 10s tick interval

    // Permanently-skipped game ids (cursor advanced past them after exhausting
    // retries). Stable so admin can see missed mints across upgrades and
    // manually retry via adminMint. Maps game.id → last error message.
    var missedGameMints = natMap.empty<Text>();

    // Same pattern for backer-delta mints. Keyed by backer principal because
    // backer rows don't have an id — backerSeen tracks "amount minted-for so far",
    // and a failed delta mint blocks the same principal on subsequent ticks.
    transient var backerMintRetries = principalMap.empty<Nat>();
    var missedBackerMints = principalMap.empty<Text>();

    // Cash-out queue
    var cashOuts = natMap.empty<CashOutEntry>();
    var nextCashOutId : Nat = 0;

    // Leaderboard (local state — not derived from ledger)
    var ppBurnedPerPlayer = principalMap.empty<Nat>();  // cumulative PP units burned
    var spellsCastPerPlayer = principalMap.empty<Nat>(); // successful casts only

    // Active spell-effect state — see type docs above. All empty on first
    // deploy; orthogonal persistence carries values across upgrades.
    var customDisplayNames = principalMap.empty<DisplayNameOverride>();
    var mintSiphons = principalMap.empty<MintSiphon>();
    var shieldsActive = principalMap.empty<ShieldState>();
    var mintMultipliers = principalMap.empty<MintMultiplier>();
    var cascadeBoosts = principalMap.empty<CascadeBoost>();
    var goldenUntil = principalMap.empty<Int>();

    // Set of principals we've ever minted PP to. Used by AOE Skim and
    // Whale Rebalance to enumerate possible victims without scanning the
    // whole ledger. Populated inside mintInternal.
    var knownPpHolders = principalMap.empty<Bool>();

    // Pool of satirical names Rename Spell pulls from. Keep PG-13.
    transient let renameNamePool : [Text] = [
        "Cap Table Casualty",
        "Series A Lemming",
        "Unvested Tears",
        "Dilution Daddy",
        "Term Sheet Terror",
        "Burn Rate Brenda",
        "Down-Round Donnie",
        "Liquidation Larry",
    ];

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
                let res = await mintWithEffects(game.player, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(game.player, units, eventId);
                        gameMintRetries := natMap.delete(gameMintRetries, game.id);
                        gameIdCursor := game.id + 1;
                    };
                    case (#Err(msg)) {
                        let attempts = switch (natMap.get(gameMintRetries, game.id)) {
                            case (?n) { n + 1 };
                            case (null) { 1 };
                        };
                        if (attempts >= MAX_MINT_RETRIES) {
                            // Exhausted retries — record the miss and advance
                            // past this game so it doesn't block subsequent ones.
                            // Admin can call adminMint to compensate the player.
                            Debug.print("Giving up on " # eventId # " after "
                                # Nat.toText(attempts) # " attempts: " # msg);
                            missedGameMints := natMap.put(missedGameMints, game.id, msg);
                            gameMintRetries := natMap.delete(gameMintRetries, game.id);
                            gameIdCursor := game.id + 1;
                            // Fall through — continue to next game in the loop.
                        } else {
                            gameMintRetries := natMap.put(gameMintRetries, game.id, attempts);
                            Debug.print("Mint attempt " # Nat.toText(attempts)
                                # "/" # Nat.toText(MAX_MINT_RETRIES)
                                # " failed for " # eventId # ": " # msg);
                            return;  // Try again on next tick.
                        };
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
                let res = await mintWithEffects(backer.owner, units, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await cascadeReferralMint(backer.owner, units, eventId);
                        backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                        backerMintRetries := principalMap.delete(backerMintRetries, backer.owner);
                    };
                    case (#Err(msg)) {
                        let attempts = switch (principalMap.get(backerMintRetries, backer.owner)) {
                            case (?n) { n + 1 };
                            case (null) { 1 };
                        };
                        if (attempts >= MAX_MINT_RETRIES) {
                            // Exhausted retries — record the miss and advance
                            // backerSeen so the same delta isn't retried forever.
                            Debug.print("Giving up on backer mint for "
                                # Principal.toText(backer.owner) # " at amount "
                                # Float.toText(backer.amount) # " after "
                                # Nat.toText(attempts) # " attempts: " # msg);
                            missedBackerMints := principalMap.put(missedBackerMints, backer.owner, msg);
                            backerMintRetries := principalMap.delete(backerMintRetries, backer.owner);
                            backerSeen := principalMap.put(backerSeen, backer.owner, backer.amount);
                        } else {
                            backerMintRetries := principalMap.put(backerMintRetries, backer.owner, attempts);
                            Debug.print("Backer mint attempt " # Nat.toText(attempts)
                                # "/" # Nat.toText(MAX_MINT_RETRIES)
                                # " failed for " # eventId # ": " # msg);
                        };
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
                case (#Ok(idx)) {
                    knownPpHolders := principalMap.put(knownPpHolders, player, true);
                    #Ok(idx);
                };
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
    /// Honors active `cascadeBoosts` per upline — Downline Boost multiplies
    /// the cascade mint at that level only (independent per level).
    func cascadeReferralMint(originUser : Principal, baseUnits : Nat, eventId : Text) : async () {
        if (baseUnits == 0) return;
        let l1Maybe = principalMap.get(referralChain, originUser);
        switch (l1Maybe) {
            case (null) {};
            case (?l1) {
                let l1Units = applyCascadeBoost(l1, baseUnits * mintConfig.referralL1Bps / 10_000);
                switch (await mintInternal(l1, l1Units, "referral-L1-" # eventId)) {
                    case (#Ok(_)) { bumpReferralEarnings(l1, 1, l1Units) };
                    case (#Err(_)) {};
                };
                let l2Maybe = principalMap.get(referralChain, l1);
                switch (l2Maybe) {
                    case (null) {};
                    case (?l2) {
                        let l2Units = applyCascadeBoost(l2, baseUnits * mintConfig.referralL2Bps / 10_000);
                        switch (await mintInternal(l2, l2Units, "referral-L2-" # eventId)) {
                            case (#Ok(_)) { bumpReferralEarnings(l2, 2, l2Units) };
                            case (#Err(_)) {};
                        };
                        let l3Maybe = principalMap.get(referralChain, l2);
                        switch (l3Maybe) {
                            case (null) {};
                            case (?l3) {
                                let l3Units = applyCascadeBoost(l3, baseUnits * mintConfig.referralL3Bps / 10_000);
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

    /// Returns `units` multiplied by `upline`'s active cascadeBoost, if any.
    /// Expired entries are treated as absent.
    func applyCascadeBoost(upline : Principal, units : Nat) : Nat {
        switch (principalMap.get(cascadeBoosts, upline)) {
            case (null) { units };
            case (?boost) {
                if (Time.now() >= boost.expiresAt) { units }
                else { units * boost.multiplierBps / 10_000 };
            };
        };
    };

    // ================================================================
    // Spell effect helpers (Phase E + F)
    // ================================================================

    /// Inclusive random percentage in [min, max]. Uses Time.now() — same
    /// pattern as determineOutcome. Not cryptographic.
    func rollPct(min : Nat, max : Nat) : Nat {
        if (max <= min) { return min };
        // Safe: branch above guarantees max > min, so subtraction won't trap.
        let span : Nat = Nat.sub(max, min) + 1;
        min + (Int.abs(Time.now()) % span);
    };

    /// Cap a Nat at `ceiling`.
    func capAt(value : Nat, ceiling : Nat) : Nat {
        if (value > ceiling) { ceiling } else { value };
    };

    /// Returns true if `target` is currently shielded; decrements charges
    /// and clears the shield if charges hit zero. Expired shields are
    /// silently cleared.
    func consumeShieldIfActive(target : Principal) : Bool {
        switch (principalMap.get(shieldsActive, target)) {
            case (null) { false };
            case (?shield) {
                if (Time.now() >= shield.expiresAt) {
                    shieldsActive := principalMap.delete(shieldsActive, target);
                    false;
                } else if (shield.chargesRemaining == 0) {
                    shieldsActive := principalMap.delete(shieldsActive, target);
                    false;
                } else {
                    let remaining : Nat = shield.chargesRemaining - 1;
                    if (remaining == 0) {
                        shieldsActive := principalMap.delete(shieldsActive, target);
                    } else {
                        shieldsActive := principalMap.put(shieldsActive, target, { chargesRemaining = remaining; expiresAt = shield.expiresAt });
                    };
                    true;
                };
            };
        };
    };

    /// Pick a name from `renameNamePool` using Time.now() as the seed.
    func pickRenameName() : Text {
        let idx = Int.abs(Time.now()) % renameNamePool.size();
        renameNamePool[idx];
    };

    /// Enumerate every known PP holder except `excluded`. Caller-side
    /// async fetch of balances follows separately.
    func enumerateHolders(excluded : Principal) : [Principal] {
        let buf = Array.filter<Principal>(
            Iter.toArray(principalMap.keys(knownPpHolders)),
            func(p) = p != excluded,
        );
        buf;
    };

    /// Fetch top-3 PP holders by current chip balance, excluding caster.
    /// Returns up to 3 (Principal, balance) pairs sorted descending.
    func top3HoldersByBalance(excluded : Principal) : async [(Principal, Nat)] {
        let candidates = enumerateHolders(excluded);
        let buf = Array.init<(Principal, Nat)>(candidates.size(), (excluded, 0));
        var i = 0;
        for (p in candidates.vals()) {
            let bal = await getChipBalance(p);
            buf[i] := (p, bal);
            i += 1;
        };
        let pairs = Array.freeze(buf);
        let sorted = Array.sort<(Principal, Nat)>(pairs, func(a, b) = Nat.compare(b.1, a.1));
        let take = if (sorted.size() < 3) { sorted.size() } else { 3 };
        Array.subArray(sorted, 0, take);
    };

    /// Compute the rubber-band success-rate modifier (in percentage points)
    /// based on caster vs target chip balance. Positive = underdog bonus,
    /// negative = top-dog penalty, clamped to ±25.
    func rubberBandMod(casterBal : Nat, targetBal : Nat) : Int {
        if (targetBal == 0) { return 0 };
        let ratio = casterBal * 1000 / targetBal;
        if (ratio < 1000) {
            let bonus : Int = ((1000 - ratio) * 25) / 1000;
            if (bonus > 25) { 25 } else { bonus };
        } else {
            let penalty : Int = ((ratio - 1000) * 25) / 1000;
            if (penalty > 25) { -25 } else { -penalty };
        };
    };

    /// Roll outcome with rubber-band modifier baked into success odds.
    /// Clamped to [5, 95]. backfireOdds reads off the configured tail
    /// unchanged — modifier shifts mass between success and failure only.
    func determineOutcomeWithMod(shenaniganType : ShenaniganType, modPct : Int) : ShenaniganOutcome {
        let baseSuccess : Int = switch (shenaniganType) {
            case (#moneyTrickster) { 60 };
            case (#aoeSkim) { 40 };
            case (#renameSpell) { 90 };
            case (#mintTaxSiphon) { 70 };
            case (#downlineHeist) { 30 };
            case (#magicMirror) { 100 };
            case (#ppBoosterAura) { 100 };
            case (#purseCutter) { 20 };
            case (#whaleRebalance) { 50 };
            case (#downlineBoost) { 100 };
            case (#goldenName) { 100 };
        };
        let baseBackfireTail : Int = switch (shenaniganType) {
            case (#moneyTrickster) { 85 };
            case (#aoeSkim) { 80 };
            case (#renameSpell) { 95 };
            case (#mintTaxSiphon) { 90 };
            case (#downlineHeist) { 90 };
            case (#magicMirror) { 100 };
            case (#ppBoosterAura) { 100 };
            case (#purseCutter) { 70 };
            case (#whaleRebalance) { 80 };
            case (#downlineBoost) { 100 };
            case (#goldenName) { 100 };
        };
        let adjustedRaw : Int = baseSuccess + modPct;
        let adjusted : Int = if (adjustedRaw < 5) { 5 } else if (adjustedRaw > 95) { 95 } else { adjustedRaw };
        let randomValue : Int = Int.abs(Time.now()) % 100;
        if (randomValue < adjusted) { #success }
        else if (randomValue < baseBackfireTail) { #fail }
        else { #backfire };
    };

    /// Apply mint multiplier + siphon to a player mint. Used by the
    /// observer for game/backer mints. Cascade and siphoner mints skip
    /// this wrapper to avoid recursion.
    func mintWithEffects(player : Principal, baseUnits : Nat, eventId : Text) : async { #Ok : Nat; #Err : Text } {
        if (baseUnits == 0) { return #Ok(0) };
        // Apply mint multiplier first (boost amount)
        let multiplied : Nat = switch (principalMap.get(mintMultipliers, player)) {
            case (null) { baseUnits };
            case (?mult) {
                if (Time.now() >= mult.expiresAt) { baseUnits }
                else { baseUnits * mult.multiplierBps / 10_000 };
            };
        };
        // Then compute siphon, if any
        let (toPlayer, siphonTuple) = switch (principalMap.get(mintSiphons, player)) {
            case (null) { (multiplied, null : ?(Principal, Nat)) };
            case (?siphon) {
                if (Time.now() >= siphon.expiresAt) {
                    mintSiphons := principalMap.delete(mintSiphons, player);
                    (multiplied, null);
                } else if (siphon.siphonedSoFar >= siphon.capUnits) {
                    mintSiphons := principalMap.delete(mintSiphons, player);
                    (multiplied, null);
                } else {
                    let rawSiphon = multiplied * siphon.pctTimes100 / 10_000;
                    let remainingCap : Nat = siphon.capUnits - siphon.siphonedSoFar;
                    let take = capAt(rawSiphon, remainingCap);
                    let newSiphoned = siphon.siphonedSoFar + take;
                    mintSiphons := principalMap.put(mintSiphons, player, {
                        siphoner = siphon.siphoner;
                        expiresAt = siphon.expiresAt;
                        pctTimes100 = siphon.pctTimes100;
                        capUnits = siphon.capUnits;
                        siphonedSoFar = newSiphoned;
                    });
                    let remaining : Nat = if (multiplied >= take) { multiplied - take } else { 0 };
                    (remaining, ?(siphon.siphoner, take));
                };
            };
        };
        let primary = await mintInternal(player, toPlayer, eventId);
        switch (siphonTuple) {
            case (?(siphoner, take)) {
                let _ = await mintInternal(siphoner, take, "siphon-" # eventId);
            };
            case null {};
        };
        primary;
    };

    // ================================================================
    // Core Logic
    // ================================================================

    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
        if (Principal.isAnonymous(caller)) { Debug.trap("Authentication required") };

        // Reject target-required spells called without one. Without this trap
        // the success branch would silently no-op and the caster's PP would
        // burn for no observable effect.
        let needsTarget = switch (shenaniganType) {
            case (#moneyTrickster) { true };
            case (#renameSpell) { true };
            case (#mintTaxSiphon) { true };
            case (#downlineHeist) { true };
            case (#purseCutter) { true };
            case (_) { false };
        };
        if (needsTarget) {
            switch (target) {
                case (null) { Debug.trap("This shenanigan requires a target") };
                case (?t) {
                    if (t == caller) { Debug.trap("Pick someone other than yourself") };
                };
            };
        };

        let config = switch (getConfigForType(shenaniganType)) {
            case (null) { Debug.trap("Unknown shenanigan type") };
            case (?c) { c };
        };
        let costUnits = ppToUnits(Int.abs(Float.toInt(config.cost)));

        let casterBalPre = await getChipBalance(caller);
        if (casterBalPre < costUnits) { Debug.trap("Insufficient chips to cast this shenanigan") };

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

        // Caster balance after burn — what they have left when effects fire.
        let casterBal : Nat = if (casterBalPre >= costUnits) { casterBalPre - costUnits : Nat } else { 0 };
        let targetBal : Nat = switch (target) {
            case (?t) { await getChipBalance(t) };
            case null { 0 };
        };

        // Rubber-band only the aggressive spells. Buff/cosmetic and
        // 100%-success spells get modifier 0 (no-op).
        let isAggressive = switch (shenaniganType) {
            case (#moneyTrickster) { true };
            case (#aoeSkim) { true };
            case (#mintTaxSiphon) { true };
            case (#downlineHeist) { true };
            case (#purseCutter) { true };
            case (#whaleRebalance) { true };
            case (_) { false };
        };
        let modPct : Int = if (isAggressive) { rubberBandMod(casterBal, targetBal) } else { 0 };
        let outcome = determineOutcomeWithMod(shenaniganType, modPct);

        switch (outcome) {
            case (#success) {
                await applySuccessEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#backfire) {
                await applyBackfireEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#fail) {};
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

    // ================================================================
    // Spell effect dispatch
    // ================================================================

    /// Apply each spell's effect on `#success`. Caster balance is post-burn;
    /// target balance is pre-effect. `castId` feeds memo strings.
    /// Returns `()`; errors are logged and swallowed so the cast record
    /// is still written.
    func applySuccessEffect(
        shenaniganType : ShenaniganType,
        caster : Principal,
        target : ?Principal,
        _casterBal : Nat,
        targetBal : Nat,
        castId : Nat,
    ) : async () {
        let memo = "spell-" # Nat.toText(castId);
        let protectionFloor = ppToUnits(200);
        let nowTs = Time.now();
        let oneDayNs : Int = 86_400_000_000_000;
        let sevenDaysNs : Int = oneDayNs * 7;

        switch (shenaniganType) {
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        if (consumeShieldIfActive(t)) { return };
                        if (targetBal < protectionFloor) { return };
                        let pct = rollPct(2, 8);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(250));
                        let _ = await chipTransfer(t, caster, amount, memo);
                    };
                };
            };
            case (#aoeSkim) {
                let pool = enumerateHolders(caster);
                for (victim in pool.vals()) {
                    if (not consumeShieldIfActive(victim)) {
                        let bal = await getChipBalance(victim);
                        if (bal >= protectionFloor) {
                            let pct = rollPct(1, 3);
                            let amount = capAt(bal * pct / 100, ppToUnits(60));
                            let _ = await chipTransfer(victim, caster, amount, memo);
                        };
                    };
                };
            };
            case (#renameSpell) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        customDisplayNames := principalMap.put(customDisplayNames, t, {
                            name = pickRenameName();
                            expiresAt = nowTs + sevenDaysNs;
                        });
                    };
                };
            };
            case (#mintTaxSiphon) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        if (consumeShieldIfActive(t)) { return };
                        if (targetBal < protectionFloor) { return };
                        mintSiphons := principalMap.put(mintSiphons, t, {
                            siphoner = caster;
                            expiresAt = nowTs + sevenDaysNs;
                            pctTimes100 = 500;
                            capUnits = ppToUnits(1000);
                            siphonedSoFar = 0;
                        });
                    };
                };
            };
            case (#downlineHeist) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        // Find deepest downline of target — prefer L3, then L2, then L1
                        let entries = principalMap.entries(referralChain);
                        var l1Victim : ?Principal = null;
                        var l2Victim : ?Principal = null;
                        var l3Victim : ?Principal = null;
                        for ((user, ref1) in entries) {
                            if (ref1 == t) {
                                l1Victim := ?user;
                            } else {
                                switch (principalMap.get(referralChain, ref1)) {
                                    case (?ref2) {
                                        if (ref2 == t) {
                                            l2Victim := ?user;
                                        } else {
                                            switch (principalMap.get(referralChain, ref2)) {
                                                case (?ref3) {
                                                    if (ref3 == t) { l3Victim := ?user };
                                                };
                                                case null {};
                                            };
                                        };
                                    };
                                    case null {};
                                };
                            };
                        };
                        let victim = switch (l3Victim) {
                            case (?v) { ?v };
                            case null { switch (l2Victim) {
                                case (?v) { ?v };
                                case null { l1Victim };
                            } };
                        };
                        switch (victim) {
                            case (null) {};
                            case (?v) {
                                if (v != caster) {
                                    referralChain := principalMap.put(referralChain, v, caster);
                                };
                            };
                        };
                    };
                };
            };
            case (#magicMirror) {
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = 1;
                    expiresAt = nowTs + oneDayNs;
                });
            };
            case (#ppBoosterAura) {
                let pct = rollPct(105, 115);
                mintMultipliers := principalMap.put(mintMultipliers, caster, {
                    multiplierBps = pct * 100;
                    expiresAt = nowTs + oneDayNs;
                });
            };
            case (#purseCutter) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        if (consumeShieldIfActive(t)) { return };
                        if (targetBal < protectionFloor) { return };
                        let pct = rollPct(25, 50);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(800));
                        let _ = await burnFrom(t, amount, memo);
                    };
                };
            };
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                for ((whale, bal) in whales.vals()) {
                    if (not consumeShieldIfActive(whale)) {
                        if (bal >= protectionFloor) {
                            let amount = capAt(bal * 20 / 100, ppToUnits(300));
                            let _ = await chipTransfer(whale, caster, amount, memo);
                        };
                    };
                };
            };
            case (#downlineBoost) {
                cascadeBoosts := principalMap.put(cascadeBoosts, caster, {
                    multiplierBps = 13_000;
                    expiresAt = nowTs + oneDayNs;
                });
            };
            case (#goldenName) {
                goldenUntil := principalMap.put(goldenUntil, caster, nowTs + oneDayNs);
            };
        };
    };

    /// Apply each spell's effect on `#backfire`. Mirror image of success —
    /// the caster pays. Buff/cosmetic spells with 100% success rate cannot
    /// backfire (they never produce this outcome).
    func applyBackfireEffect(
        shenaniganType : ShenaniganType,
        caster : Principal,
        target : ?Principal,
        casterBal : Nat,
        _targetBal : Nat,
        castId : Nat,
    ) : async () {
        let memo = "backfire-" # Nat.toText(castId);
        let nowTs = Time.now();
        let oneDayNs : Int = 86_400_000_000_000;
        let sevenDaysNs : Int = oneDayNs * 7;
        let halfWeekNs : Int = oneDayNs * 3;

        switch (shenaniganType) {
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        let pct = rollPct(2, 8);
                        let amount = capAt(casterBal * pct / 100, ppToUnits(250));
                        let _ = await chipTransfer(caster, t, amount, memo);
                    };
                };
            };
            case (#aoeSkim) {
                let pct = rollPct(1, 3);
                let loss = casterBal * pct / 100;
                let _ = await burnFrom(caster, loss, memo);
            };
            case (#renameSpell) {
                customDisplayNames := principalMap.put(customDisplayNames, caster, {
                    name = pickRenameName();
                    expiresAt = nowTs + sevenDaysNs;
                });
            };
            case (#mintTaxSiphon) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        mintSiphons := principalMap.put(mintSiphons, caster, {
                            siphoner = t;
                            expiresAt = nowTs + halfWeekNs;
                            pctTimes100 = 500;
                            capUnits = ppToUnits(1000);
                            siphonedSoFar = 0;
                        });
                    };
                };
            };
            case (#downlineHeist) {
                // Caster loses deepest downline to target
                switch (target) {
                    case (null) {};
                    case (?t) {
                        let entries = principalMap.entries(referralChain);
                        var l1Victim : ?Principal = null;
                        var l2Victim : ?Principal = null;
                        var l3Victim : ?Principal = null;
                        for ((user, ref1) in entries) {
                            if (ref1 == caster) {
                                l1Victim := ?user;
                            } else {
                                switch (principalMap.get(referralChain, ref1)) {
                                    case (?ref2) {
                                        if (ref2 == caster) {
                                            l2Victim := ?user;
                                        } else {
                                            switch (principalMap.get(referralChain, ref2)) {
                                                case (?ref3) {
                                                    if (ref3 == caster) { l3Victim := ?user };
                                                };
                                                case null {};
                                            };
                                        };
                                    };
                                    case null {};
                                };
                            };
                        };
                        let victim = switch (l3Victim) {
                            case (?v) { ?v };
                            case null { switch (l2Victim) {
                                case (?v) { ?v };
                                case null { l1Victim };
                            } };
                        };
                        switch (victim) {
                            case (null) {};
                            case (?v) {
                                if (v != t) {
                                    referralChain := principalMap.put(referralChain, v, t);
                                };
                            };
                        };
                    };
                };
            };
            case (#purseCutter) {
                let pct = rollPct(25, 50);
                let amount = capAt(casterBal * pct / 100, ppToUnits(800));
                let _ = await burnFrom(caster, amount, memo);
            };
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                for ((whale, _) in whales.vals()) {
                    let amount = capAt(casterBal * 20 / 100, ppToUnits(300));
                    let _ = await chipTransfer(caster, whale, amount, memo);
                };
            };
            case (#magicMirror) {};
            case (#ppBoosterAura) {};
            case (#downlineBoost) {};
            case (#goldenName) {};
        };
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

    /// All active spell effects on `user`. Expired entries are filtered
    /// out of the result but not deleted from state (cleanup happens
    /// lazily on next write/cast).
    public query func getActiveSpellEffects(user : Principal) : async ActiveSpellEffects {
        let now = Time.now();
        let liveShield = switch (principalMap.get(shieldsActive, user)) {
            case (?s) { if (now < s.expiresAt and s.chargesRemaining > 0) { ?s } else { null } };
            case null { null };
        };
        let liveMult = switch (principalMap.get(mintMultipliers, user)) {
            case (?m) { if (now < m.expiresAt) { ?m } else { null } };
            case null { null };
        };
        let liveBoost = switch (principalMap.get(cascadeBoosts, user)) {
            case (?b) { if (now < b.expiresAt) { ?b } else { null } };
            case null { null };
        };
        let liveName = switch (principalMap.get(customDisplayNames, user)) {
            case (?d) { if (now < d.expiresAt) { ?d } else { null } };
            case null { null };
        };
        let liveSiphon = switch (principalMap.get(mintSiphons, user)) {
            case (?s) { if (now < s.expiresAt and s.siphonedSoFar < s.capUnits) { ?s } else { null } };
            case null { null };
        };
        let isGolden = switch (principalMap.get(goldenUntil, user)) {
            case (?t) { now < t };
            case null { false };
        };
        {
            shield = liveShield;
            mintMultiplier = liveMult;
            cascadeBoost = liveBoost;
            displayName = liveName;
            mintSiphon = liveSiphon;
            golden = isGolden;
        };
    };

    /// Currently-golden players. Used by frontend for leaderboard styling.
    public query func getGoldenPlayers() : async [Principal] {
        let now = Time.now();
        let entries = principalMap.entries(goldenUntil);
        let buf = Array.filter<(Principal, Int)>(Iter.toArray(entries), func(e) = now < e.1);
        Array.map<(Principal, Int), Principal>(buf, func(e) = e.0);
    };

    /// Active rename-spell name for `user`, if any. Expired entries return null.
    public query func getCustomDisplayName(user : Principal) : async ?Text {
        switch (principalMap.get(customDisplayNames, user)) {
            case (?d) { if (Time.now() < d.expiresAt) { ?d.name } else { null } };
            case null { null };
        };
    };

    /// All principals we've ever minted PP to. Frontend target-pickers can
    /// use this to populate a candidate list. Updated lazily — entries are
    /// added in mintInternal and never removed (cheap, bounded by player count).
    public query func getKnownPpHolders() : async [Principal] {
        Iter.toArray(principalMap.keys(knownPpHolders));
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
        missedGameMintsCount : Nat;
        missedBackerMintsCount : Nat;
    } {
        {
            running = observerTimerId != null;
            gameIdCursor;
            backerSeenCount = principalMap.size(backerSeen);
            intervalSeconds = mintConfig.observerIntervalSeconds;
            missedGameMintsCount = natMap.size(missedGameMints);
            missedBackerMintsCount = principalMap.size(missedBackerMints);
        };
    };

    /// Games the observer permanently gave up on after MAX_MINT_RETRIES failures.
    /// Admin can fixup via adminMint and then clearMissedGameMint to dismiss.
    public query func getMissedGameMints() : async [(Nat, Text)] {
        Iter.toArray(natMap.entries(missedGameMints));
    };

    /// Backer principals whose delta mint was permanently skipped.
    public query func getMissedBackerMints() : async [(Principal, Text)] {
        Iter.toArray(principalMap.entries(missedBackerMints));
    };

    /// Dismiss a missed game-mint entry. Use after manually compensating
    /// the player via adminMint, so the missed-mints list stays clean.
    public shared ({ caller }) func clearMissedGameMint(gameId : Nat) : async () {
        requireAdmin(caller);
        missedGameMints := natMap.delete(missedGameMints, gameId);
    };

    public shared ({ caller }) func clearMissedBackerMint(owner : Principal) : async () {
        requireAdmin(caller);
        missedBackerMints := principalMap.delete(missedBackerMints, owner);
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
