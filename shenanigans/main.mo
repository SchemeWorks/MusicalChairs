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
import Buffer "mo:base/Buffer";
import Char "mo:base/Char";
import Text "mo:base/Text";

import PpLedger "PpLedger";
import Reginald "Reginald";
import Subaccount "Subaccount";
import Icrc21 "icrc21";

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

    /// Detailed cast outcome. `ppDeltaCaster` is the net PP unit change for
    /// the caster *excluding* the spell cost burn — negative means the caster
    /// also paid the backfire penalty; positive means they net-gained from
    /// theft. `affectedTarget` is the specific principal hit (Money Trickster,
    /// Purse Cutter, etc.) or null for self-buffs / fails. `affectedCount`
    /// counts how many distinct victims were touched (AoE Skim and Whale
    /// Rebalance set this > 1).
    public type ShenaniganOutcomeDetail = {
        outcome : ShenaniganOutcome;
        ppDeltaCaster : Int;
        affectedTarget : ?Principal;
        affectedCount : Nat;
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
        costSuccess : Float;
        costFailure : Float;
        costBackfire : Float;
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

    public type SignupEntry = {
        principal : Principal;
        joinedAt : Int;  // ns since epoch
        level : Nat;     // 1, 2, or 3 — chain level relative to caller
    };

    /// Per-tier downline counts and cumulative PP earnings for a user.
    /// Counts derive from a single pass over referralChain; earnings come
    /// from the local accumulator updated inside distributeDeductiveCascade.
    public type ReferralStats = {
        l1Count : Nat;
        l2Count : Nat;
        l3Count : Nat;
        l1Units : Nat;
        l2Units : Nat;
        l3Units : Nat;
        recentSignups : [SignupEntry];  // L1/L2/L3 only; sorted joinedAt desc; capped 20
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
    // Trollbox types
    // ================================================================

    public type ChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : { castId : Nat };
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    public type Reaction = {
        emoji : Text;
        reactors : [Principal];
        karmaPpBurned : Nat;
    };

    public type ChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : ChatItemKind;
        reactions : [Reaction];
        deleted : Bool;
    };

    public type ChimeSound = {
        name : Text;
        bytes : Blob;
        mimeType : Text;
        uploadedAt : Int;
    };

    public type ChimeSoundMeta = {
        name : Text;
        mimeType : Text;
        sizeBytes : Nat;
        uploadedAt : Int;
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

    // ================================================================
    // Trollbox state
    // ================================================================

    /// Unified chat + system-event ring buffer. Bounded to last 500 entries.
    /// Newest items have the highest id. Stored in insertion order.
    var chatItems : [ChatItem] = [];
    var nextChatItemId : Nat = 0;

    /// Per-user mute expirations (ns since epoch). Lazily expired on read;
    /// no GC on write — mutedUntilFor filters stale entries on every lookup.
    var mutedUntilEntries = principalMap.empty<Int>();

    /// Id of the most recent #pinUpdate. null clears the pin in the UI.
    /// May point to an evicted item; getCurrentPin returns null in that case.
    var currentPinId : ?Nat = null;

    /// Last-known referral rank per user. Drives #rankUp upward-edge detection.
    var previousRankEntries = principalMap.empty<Text>();

    /// Per-user last accepted post timestamp + 5-min sliding window count.
    /// Used by the postChatMessage rate limit.
    var lastChatPostEntries = principalMap.empty<Int>();
    var recentPostCountEntries = principalMap.empty<[Int]>(); // ns timestamps in window

    /// Admin-uploaded @-mention chime pool. Bounded by per-file 200KB cap and
    /// the chimeSoundCount upper bound below. Keyed by name (case-sensitive).
    var chimeSoundPool : [ChimeSound] = [];

    /// Last accepted reaction timestamp per principal. Used by the
    /// free-reaction min-gap limit. Lazy GC on overwrite.
    var lastReactionEntries = principalMap.empty<Int>();

    /// Admin-overridable flavor pools, keyed by canonical pool name.
    /// Empty pool means "admin explicitly set an empty list" — downstream
    /// behavior treats that as "this trigger never fires" (which is desired:
    /// admin can disable a specific Reginald category). To revert to the
    /// hardcoded default, admin clears the override entirely.
    var flavorPoolOverrides : [(Text, [Text])] = [];

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
    // active-rank tier. Incremented inside distributeDeductiveCascade on
    // successful mints; reads feed getReferralStats. Buckets L1/L2/L3
    // correspond to the closest, second-closest, third-closest active
    // upline (not raw chain position) since the cascade flows around
    // inactive uplines.
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

    // Per-principal first-seen timestamp for #signup chat announcements.
    // Written independently of signupGiftPp so the announcement fires even
    // when the gift is disabled (signupGiftPp = 0).
    var signupAnnouncedSet = principalMap.empty<Int>();  // principal -> first-seen ns

    // Per-principal time of last qualifying deposit (≥ 0.1 ICP). Drives
    // isActive() without per-cascade inter-canister calls. Populated by
    // the observer on every qualifying mint event.
    var lastQualifyingDeposit = principalMap.empty<Int>();

    // Set to true by seedMigrationV2 once existing players have been
    // grandfathered into signupGiftClaimed / lastQualifyingDeposit. The
    // observer refuses to mint until this flips — closes the upgrade-time
    // race where the first tick would otherwise hand 500 PP signup gifts
    // to every existing player.
    var bootstrapped : Bool = false;

    // Reverse index of referralChain: referrer → List<downliner>. Backfilled
    // and maintained by seedMigrationV2 + registerReferral. Used by
    // getReferralStats for O(downline) queries instead of O(N) scans of
    // referralChain.
    var referrerToDownline = principalMap.empty<List.List<Principal>>();

    // Per-principal timestamp of when registerReferral first wrote the
    // user's referralChain entry. Used as the authoritative "joined" time
    // for getReferralStats.recentSignups, falling back to signupAnnouncedSet
    // / signupGiftClaimed for users who joined before this map existed
    // (those upgrade-time entries gracefully degrade to whichever earlier
    // timestamp was captured). Lets users who registered via ?ref= but
    // haven't deposited yet still show up in their upline's MLM view.
    var referralChainJoinedAt = principalMap.empty<Int>();

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
    // spellsCastPerPlayer: cumulative count of #success OR #backfire casts.
    // #fail outcomes are not counted because they had no observable effect.
    var spellsCastPerPlayer = principalMap.empty<Nat>();
    // Cumulative PP units received via karma reactions (the 40% recipient cut).
    // Prestige stat — surfaced in Hall of Fame.
    var karmaReceivedPerPlayer = principalMap.empty<Nat>();

    // Active spell-effect state — see type docs above. All empty on first
    // deploy; orthogonal persistence carries values across upgrades.
    var customDisplayNames = principalMap.empty<DisplayNameOverride>();
    var mintSiphons = principalMap.empty<MintSiphon>();
    var shieldsActive = principalMap.empty<ShieldState>();
    var mintMultipliers = principalMap.empty<MintMultiplier>();
    var cascadeBoosts = principalMap.empty<CascadeBoost>();
    var goldenUntil = principalMap.empty<Int>();

    // When Rename Spell lands on #success, the new name is NOT applied
    // immediately. Instead the caster gets a 5-minute window to pick a
    // name via setPendingRenameName. If the window lapses, the slot
    // simply expires (no automatic fallback rename — the cast cost is
    // already burned).
    var pendingRenames = principalMap.empty<{ target : Principal; expiresAt : Int }>();

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
                startObserver<system>();
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
    /// Admin principals (adminPrincipal + extraAdmins) are silently rejected:
    /// Charles sits at the top of the chain by design and must never be
    /// registered as someone else's downline, even if a stale `?ref=` survives
    /// in localStorage from earlier testing.
    public shared ({ caller }) func registerReferral(referrer : Principal) : async () {
        if (Principal.isAnonymous(caller)) { Debug.trap("Anonymous principal not allowed") };
        if (caller == referrer) { return };
        if (isAdminPrincipal(caller)) { return };
        switch (principalMap.get(referralChain, caller)) {
            case (?_) { /* already set */ };
            case null {
                referralChain := principalMap.put(referralChain, caller, referrer);
                // Maintain the reverse index so getReferralStats stays O(downline).
                let existing = switch (principalMap.get(referrerToDownline, referrer)) {
                    case (?list) { list };
                    case (null) { List.nil<Principal>() };
                };
                referrerToDownline := principalMap.put(referrerToDownline, referrer, List.push(caller, existing));
                // Capture the join time so the user appears in their upline's
                // recentSignups immediately, even before they deposit. Without
                // this, pushSignup falls back to signupAnnouncedSet (set on
                // first qualifying deposit) and the user is invisible to the
                // upline until then.
                referralChainJoinedAt := principalMap.put(referralChainJoinedAt, caller, Time.now());
            };
        };
    };

    /// One-hop lookup — returns the user's immediate referrer (L1) or null.
    public query func getReferrer(user : Principal) : async ?Principal {
        principalMap.get(referralChain, user);
    };

    /// Admin-only: scrub `user`'s referralChain entry and the reverse-index
    /// edge. Used to clean up bad mappings created before the
    /// `isAdminPrincipal` guard landed in registerReferral (e.g. Charles
    /// auto-registered against someone else's `?ref=` code). No-op if `user`
    /// has no entry. Returns the principal that was removed, or null.
    public shared ({ caller }) func adminClearReferrer(user : Principal) : async ?Principal {
        requireAdmin(caller);
        switch (principalMap.get(referralChain, user)) {
            case (null) { null };
            case (?referrer) {
                referralChain := principalMap.delete(referralChain, user);
                // Rebuild the reverse-index list for `referrer` without `user`.
                switch (principalMap.get(referrerToDownline, referrer)) {
                    case (null) {};
                    case (?downliners) {
                        let filtered = List.filter<Principal>(
                            downliners,
                            func(p) { p != user },
                        );
                        referrerToDownline := principalMap.put(referrerToDownline, referrer, filtered);
                    };
                };
                ?referrer;
            };
        };
    };

    /// Per-tier downline counts and cumulative PP earnings for `user`.
    /// Walks the referrerToDownline reverse index — O(L1+L2+L3 size) instead
    /// of O(all referrals). Recent-signup join times come from signupGiftClaimed.
    func computeReferralStats(user : Principal) : ReferralStats {
        var l1Count : Nat = 0;
        var l2Count : Nat = 0;
        var l3Count : Nat = 0;
        var bufRef : List.List<SignupEntry> = List.nil<SignupEntry>();

        let downlineOf = func(p : Principal) : List.List<Principal> {
            switch (principalMap.get(referrerToDownline, p)) {
                case (?list) { list };
                case (null) { List.nil<Principal>() };
            };
        };

        // Resolve a join timestamp for a downliner using the best available
        // signal. Preference order:
        //   1. referralChainJoinedAt — set the moment registerReferral lands.
        //      Accurate to the actual moment they joined the chain.
        //   2. signupAnnouncedSet — set on first observed game. Earlier
        //      cohorts who joined before referralChainJoinedAt existed.
        //   3. signupGiftClaimed — set on first signup gift mint (only when
        //      signupGiftPp > 0). Older fallback still.
        //   4. 0 — sentinel for "joined but no timestamp recorded anywhere".
        //      Surfaces them in recentSignups (sorted to the bottom) so the
        //      upline can at least see they exist.
        let resolveJoinedAt = func(p : Principal) : Int {
            switch (principalMap.get(referralChainJoinedAt, p)) {
                case (?t) { t };
                case (null) {
                    switch (principalMap.get(signupAnnouncedSet, p)) {
                        case (?t) { t };
                        case (null) {
                            switch (principalMap.get(signupGiftClaimed, p)) {
                                case (?t) { t };
                                case (null) { 0 };
                            };
                        };
                    };
                };
            };
        };

        let pushSignup = func(downliner : Principal, level : Nat) {
            let ts = resolveJoinedAt(downliner);
            bufRef := List.push({ principal = downliner; joinedAt = ts; level }, bufRef);
        };

        for (l1 in List.toIter(downlineOf(user))) {
            l1Count += 1;
            pushSignup(l1, 1);
            for (l2 in List.toIter(downlineOf(l1))) {
                l2Count += 1;
                pushSignup(l2, 2);
                for (l3 in List.toIter(downlineOf(l2))) {
                    l3Count += 1;
                    pushSignup(l3, 3);
                };
            };
        };

        let allSignups = List.toArray(bufRef);
        let sorted = Array.sort<SignupEntry>(allSignups, func(a, b) = Int.compare(b.joinedAt, a.joinedAt));
        let capped = if (sorted.size() <= 20) { sorted } else { Array.subArray(sorted, 0, 20) };

        let earnings = switch (principalMap.get(referralEarnings, user)) {
            case (?e) { e };
            case null { { l1Units = 0; l2Units = 0; l3Units = 0 } };
        };
        {
            l1Count;
            l2Count;
            l3Count;
            l1Units = earnings.l1Units;
            l2Units = earnings.l2Units;
            l3Units = earnings.l3Units;
            recentSignups = capped;
        };
    };

    public query func getReferralStats(user : Principal) : async ReferralStats {
        computeReferralStats(user);
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

    // Non-trapping admin predicate — for guards that should silently no-op
    // (e.g. blocking admin/Charles from being registered as someone's downline)
    // rather than trap. Matches `requireAdmin`'s membership rules.
    func isAdminPrincipal(p : Principal) : Bool {
        switch (adminPrincipal) {
            case (?admin) { if (p == admin) return true };
            case (null) {};
        };
        for (extra in extraAdmins.vals()) {
            if (p == extra) return true;
        };
        false;
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

    /// Re-arm the recurring observer timer on every canister upgrade.
    /// The IC clears all pending timers when a canister is upgraded, so a
    /// stale `observerTimerId` survives in stable state but points at a timer
    /// that no longer fires. Without this hook, after every shenanigans
    /// upgrade `getObserverStatus` reports `running = true` while the timer
    /// is silently dead — admin has to manually call `resumeObserver` or
    /// `runObserverOnce` to restart minting. This hook re-registers the
    /// timer automatically so deposits get processed within
    /// observerIntervalSeconds of any upgrade. Only fires if the canister
    /// has already been initialized (adminPrincipal set); fresh deploys
    /// still go through `initialize`.
    system func postupgrade() {
        switch (adminPrincipal) {
            case (?_) { startObserver<system>() };
            case (null) {};
        };
    };

    /// One observer pass. Mints PP for new deposits and dealer top-ups.
    /// Advances cursors only after successful mint to guarantee at-least-once
    /// minting with ledger-level dedup (via created_at_time + memo) preventing
    /// duplicates.
    func observerTick() : async () {
        if (observerRunning) return;
        // Upgrade-safety: refuse to mint until seedMigrationV2 has
        // grandfathered existing players. Without this gate, the first
        // post-upgrade tick would treat every existing player as a brand-
        // new signup and mint them all the 500 PP gift.
        if (not bootstrapped) return;
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
                let baseUnits = icpFloatToPpUnits(game.amount, ppPerIcp);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = "game-" # Nat.toText(game.id);

                // Announce signup in chat unconditionally on first observation —
                // independent of whether the gift is enabled. This ensures the
                // #signup item fires even when signupGiftPp = 0.
                switch (principalMap.get(signupAnnouncedSet, game.player)) {
                    case (?_) {};
                    case (null) {
                        signupAnnouncedSet := principalMap.put(signupAnnouncedSet, game.player, Time.now());
                        let _ = appendChatItem(Principal.fromActor(Self), #signup({ newUser = game.player }));
                    };
                };

                // Signup gift — gated on first qualifying game record.
                // Gift itself goes through the deductive cascade (mint event).
                if (mintConfig.signupGiftPp > 0) {
                    switch (principalMap.get(signupGiftClaimed, game.player)) {
                        case (?_) {}; // already claimed
                        case (null) {
                            signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, Time.now());
                            let giftBase = ppToUnits(mintConfig.signupGiftPp);
                            let giftCascade = giftBase * mintConfig.cascadeInitialBps / 10_000;
                            let giftNet : Nat = if (giftBase > giftCascade) { giftBase - giftCascade } else { 0 };
                            let giftEventId = "signup-" # Principal.toText(game.player);
                            switch (await mintWithEffects(game.player, giftNet, giftEventId)) {
                                case (#Ok(_)) {
                                    await distributeDeductiveCascade(game.player, giftCascade, giftEventId);
                                };
                                case (#Err(msg)) {
                                    Debug.print("Signup-gift mint failed for " # giftEventId # ": " # msg);
                                };
                            };
                        };
                    };
                };

                let res = await mintWithEffects(game.player, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(game.player, cascadeUnits, eventId);
                        // Track qualifying deposit for isActive() — observer is the
                        // single source of truth for activity timestamps.
                        if (game.amount >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, Time.now());
                        };
                        gameMintRetries := natMap.delete(gameMintRetries, game.id);
                        gameIdCursor := game.id + 1;

                        let _ = appendChatItem(Principal.fromActor(Self), #roundResult({
                            gameId = game.id;
                            winner = game.player;
                            winnerPpUnits = playerNet;
                        }));

                        let coin = Int.abs(Time.now()) % 7; // ~15%
                        if (coin == 0) {
                            switch (reginaldPickFor("roundResult")) {
                                case (?line) {
                                    let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "roundResult" }));
                                };
                                case (null) {};
                            };
                        };
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
                let baseUnits = icpFloatToPpUnits(delta, mintConfig.backerPpPerIcp);
                let cascadeUnits = baseUnits * mintConfig.cascadeInitialBps / 10_000;
                let playerNet : Nat = if (baseUnits > cascadeUnits) { baseUnits - cascadeUnits } else { 0 };
                let eventId = "backer-" # Principal.toText(backer.owner) # "-" # Float.toText(backer.amount);

                let res = await mintWithEffects(backer.owner, playerNet, eventId);
                switch (res) {
                    case (#Ok(_)) {
                        await distributeDeductiveCascade(backer.owner, cascadeUnits, eventId);
                        if (delta >= 0.1) {
                            lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, backer.owner, Time.now());
                        };
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
        // Defaults reproduce the legacy single-cost behavior: all three
        // outcome costs are set equal to the previous single value. Admin
        // tunes per-outcome from the admin panel. The per-outcome split is
        // what makes the "aggressive spell, free on success" pattern
        // possible (set costSuccess = 0 in admin and leave the rest).
        let defaultConfigs : [ShenaniganConfig] = [
            { id = 0; name = "MEV Attack"; description = "Sandwich-attacks the target for 2\u{2013}8% of their Ponzi Points (max 250 PP)."; costSuccess = 120.0; costFailure = 120.0; costBackfire = 120.0; successOdds = 60; failureOdds = 25; backfireOdds = 15; duration = 0; cooldown = 2; effectValues = [2.0, 8.0, 250.0]; castLimit = 0; backgroundColor = "#fff9e6" },
            { id = 1; name = "Contagion"; description = "Losses get socialized \u{2014} every player surrenders 1\u{2013}3% (max 60 PP each)."; costSuccess = 600.0; costFailure = 600.0; costBackfire = 600.0; successOdds = 40; failureOdds = 40; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [1.0, 3.0, 60.0]; castLimit = 1; backgroundColor = "#e6f7ff" },
            { id = 2; name = "Cease & Desist"; description = "Target is forced to change their display name for 7 days."; costSuccess = 200.0; costFailure = 200.0; costBackfire = 200.0; successOdds = 90; failureOdds = 5; backfireOdds = 5; duration = 168; cooldown = 0; effectValues = [7.0]; castLimit = 0; backgroundColor = "#ffe6f7" },
            { id = 3; name = "Trailing Commission"; description = "Skims 5% of target's new PP for 7 days (max 1000 PP)."; costSuccess = 1200.0; costFailure = 1200.0; costBackfire = 1200.0; successOdds = 70; failureOdds = 20; backfireOdds = 10; duration = 168; cooldown = 0; effectValues = [5.0, 1000.0]; castLimit = 0; backgroundColor = "#f3e6ff" },
            { id = 4; name = "Crossline Poach"; description = "Poach one member from target's downline (favors L3)."; costSuccess = 500.0; costFailure = 500.0; costBackfire = 500.0; successOdds = 30; failureOdds = 60; backfireOdds = 10; duration = 0; cooldown = 0; effectValues = []; castLimit = 1; backgroundColor = "#e6fff2" },
            { id = 5; name = "Poison Pill"; description = "Defensive measure \u{2014} blocks one hostile shenanigan."; costSuccess = 200.0; costFailure = 200.0; costBackfire = 200.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = []; castLimit = 2; backgroundColor = "#fff4e6" },
            { id = 6; name = "Yield Boost"; description = "Earn +5\u{2013}15% additional PP for the rest of the round."; costSuccess = 300.0; costFailure = 300.0; costBackfire = 300.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [5.0, 15.0]; castLimit = 1; backgroundColor = "#e6f2ff" },
            { id = 7; name = "Bridge Exploit"; description = "Target loses 25\u{2013}50% of their PP (max 800 PP)."; costSuccess = 900.0; costFailure = 900.0; costBackfire = 900.0; successOdds = 20; failureOdds = 50; backfireOdds = 30; duration = 0; cooldown = 0; effectValues = [25.0, 50.0, 800.0]; castLimit = 0; backgroundColor = "#ffe6e6" },
            { id = 8; name = "Wealth Tax"; description = "A socialist mayor takes office \u{2014} 20% from the top 3 PP holders (max 300 PP/whale)."; costSuccess = 800.0; costFailure = 800.0; costBackfire = 800.0; successOdds = 50; failureOdds = 30; backfireOdds = 20; duration = 0; cooldown = 0; effectValues = [20.0, 300.0]; castLimit = 0; backgroundColor = "#f0e6ff" },
            { id = 9; name = "Override Bonus"; description = "Your downline kicks up 1.3x PP for the rest of the round."; costSuccess = 400.0; costFailure = 400.0; costBackfire = 400.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 0; cooldown = 0; effectValues = [1.3]; castLimit = 1; backgroundColor = "#e6fffa" },
            { id = 10; name = "Whitelisted"; description = "Gold name on the leaderboard (24h or 7d) \u{2014} the only clout that matters."; costSuccess = 100.0; costFailure = 100.0; costBackfire = 100.0; successOdds = 100; failureOdds = 0; backfireOdds = 0; duration = 24; cooldown = 0; effectValues = [24.0, 168.0]; castLimit = 1; backgroundColor = "#fff0e6" },
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

    // Deductive cascade: 10% off the top (cascadeInitialBps) distributed
    // up the chain at 50% passthrough (cascadePassthroughBps) per active
    // upline. Inactive uplines are skipped (flow-around). Cycles detected
    // via visited-set. Residual after depth cap → house.
    //
    // Per-upline effects honored: applyCascadeBoost multiplies the payout
    // if the upline has an active Downline Boost. Mints go through
    // mintWithEffects so PP Booster Aura / Mint Tax Siphon also apply to
    // the upline's incoming cascade PP.
    //
    // Caller is responsible for minting the player's NET (base - cascadeUnits)
    // before invoking. This function only handles the cascade share.
    func distributeDeductiveCascade(originUser : Principal, cascadeUnits : Nat, eventId : Text) : async () {
        if (cascadeUnits == 0) return;

        var remaining : Nat = cascadeUnits;
        var visited = principalMap.empty<()>();
        visited := principalMap.put(visited, originUser, ());

        var depth : Nat = 0;
        var activeRank : Nat = 0;
        var current : Principal = originUser;

        label walk loop {
            if (remaining == 0 or depth >= CASCADE_DEPTH_CAP) { break walk };

            let next = getPayoutTarget(current);
            switch (principalMap.get(visited, next)) {
                case (?_) { break walk }; // cycle — bail to residual
                case (null) {};
            };
            visited := principalMap.put(visited, next, ());
            depth += 1;

            if (not isActive(next)) { current := next; continue walk };

            activeRank += 1;
            let basePayout = remaining * mintConfig.cascadePassthroughBps / 10_000;
            if (basePayout == 0) { break walk };
            let payout = applyCascadeBoost(next, basePayout);

            switch (await mintWithEffects(next, payout, "cascade-A" # Nat.toText(activeRank) # "-" # eventId)) {
                case (#Ok(_)) {
                    // Display buckets are L1/L2/L3 only. activeRank ≥ 4 still
                    // receives the payout via the mint above; we just don't
                    // inflate the L3 bucket with their share.
                    if (activeRank <= 3) { bumpReferralEarnings(next, activeRank, payout) };
                    maybeEmitRankUp(next);
                    // Decrement remaining by the pre-boost amount so the cascade
                    // distribution math stays conservative regardless of boosts.
                    remaining -= basePayout;
                };
                case (#Err(_)) {
                    // Mint failed (e.g. ledger TemporarilyUnavailable). Leave
                    // `remaining` untouched so the failed payout flows into the
                    // residual sweep instead of vanishing. Conservation holds.
                };
            };

            current := next;
        };

        // Residual to house: covers depth cap, cycle break, exhausted chain.
        if (remaining > 0) {
            let _ = await mintWithEffects(house(), remaining, "cascade-residual-" # eventId);
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

    /// Pick a name from the rename pool (or the admin override) using Time.now()
    /// as the seed. Returns "Anonymous Investor" when the effective pool is empty
    /// (admin disabled it) so the rename spell still completes its mechanical
    /// effect rather than requiring callers to handle an option type.
    func pickRenameName() : Text {
        let pool = effectivePool("renameNamePool", renameNamePool);
        if (pool.size() == 0) { return "Anonymous Investor" };
        pool[Int.abs(Time.now()) % pool.size()];
    };

    /// Validate + sanitize a player-chosen rename. Rules:
    ///  - Trim leading/trailing spaces
    ///  - 1 to 32 chars after trim
    ///  - Allowed: a-z A-Z 0-9 space - _
    func sanitizeRenameName(raw : Text) : { #Ok : Text; #Err : Text } {
        let trimmed = Text.trim(raw, #char ' ');
        if (Text.size(trimmed) == 0) { return #Err("Name cannot be empty") };
        if (Text.size(trimmed) > 32) { return #Err("Name too long (max 32 chars)") };
        for (c in trimmed.chars()) {
            let ok =
                (c >= 'a' and c <= 'z') or
                (c >= 'A' and c <= 'Z') or
                (c >= '0' and c <= '9') or
                c == ' ' or c == '-' or c == '_';
            if (not ok) { return #Err("Invalid character in name") };
        };
        #Ok(trimmed);
    };

    /// Lazy cleanup: drop every pendingRenames entry whose expiresAt has
    /// already passed. Called on the warm path of setPendingRenameName so
    /// the map doesn't accumulate stubs from casters who never commit.
    func sweepExpiredPendingRenames() {
        let now = Time.now();
        for ((p, slot) in principalMap.entries(pendingRenames)) {
            if (now >= slot.expiresAt) {
                pendingRenames := principalMap.delete(pendingRenames, p);
            };
        };
    };

    /// Caller commits a chosen name for their most recent successful Rename
    /// Spell. Must be called within 5 minutes of the cast. Name is sanitized:
    /// trimmed, 1-32 chars, alphanumeric + space + dash + underscore only.
    public shared ({ caller }) func setPendingRenameName(name : Text) : async { #Ok; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        sweepExpiredPendingRenames();
        let slot = switch (principalMap.get(pendingRenames, caller)) {
            case (null) { return #Err("No pending rename") };
            case (?s) { s };
        };
        if (Time.now() >= slot.expiresAt) {
            pendingRenames := principalMap.delete(pendingRenames, caller);
            return #Err("Pending rename expired");
        };
        switch (sanitizeRenameName(name)) {
            case (#Err(msg)) { return #Err(msg) };
            case (#Ok(text)) {
                let sevenDaysNs : Int = 86_400_000_000_000 * 7;
                customDisplayNames := principalMap.put(customDisplayNames, slot.target, {
                    name = text;
                    expiresAt = Time.now() + sevenDaysNs;
                });
                pendingRenames := principalMap.delete(pendingRenames, caller);
                return #Ok;
            };
        };
    };

    /// Returns the active pending-rename slot for the caller, if any.
    /// Drives the frontend modal that prompts for a name post-success.
    public query ({ caller }) func getPendingRenameForCaller() : async ?{
        target : Principal;
        expiresAt : Int;
    } {
        switch (principalMap.get(pendingRenames, caller)) {
            case (null) { null };
            case (?s) {
                if (Time.now() >= s.expiresAt) { null }
                else { ?s };
            };
        };
    };

    /// Caller explicitly cancels their pending-rename slot. Idempotent —
    /// safe to call when no slot exists. Used by the "Skip" button on the
    /// rename modal so the slot doesn't dangle and re-trigger the
    /// mount-time prompt. The cast cost is already burned and is not
    /// refunded.
    public shared ({ caller }) func cancelPendingRename() : async () {
        pendingRenames := principalMap.delete(pendingRenames, caller);
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

    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcomeDetail {
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
        let costSuccessUnits = ppToUnits(Int.abs(Float.toInt(config.costSuccess)));
        let costFailureUnits = ppToUnits(Int.abs(Float.toInt(config.costFailure)));
        let costBackfireUnits = ppToUnits(Int.abs(Float.toInt(config.costBackfire)));

        let casterBalPre = await getChipBalance(caller);
        // Pre-cast gate is the *minimum* the caster commits to paying — i.e.
        // costSuccess. They might roll a worse outcome and owe more than
        // they have; in that case the burn below clamps to their balance and
        // they zero out (no trap, no debt — debt is a follow-up feature).
        if (casterBalPre < costSuccessUnits) {
            Debug.trap("Insufficient chips to cast this shenanigan");
        };

        let castId = nextShenaniganId;

        // Roll the outcome BEFORE burning — the cost charged depends on the
        // outcome rolled, so we have to know the outcome first. Rubber-band
        // modifier still applies to aggressive spells; buff/cosmetic and
        // 100%-success spells get modifier 0 (no-op).
        let targetBalForRoll : Nat = switch (target) {
            case (?t) { await getChipBalance(t) };
            case null { 0 };
        };
        let isAggressive = switch (shenaniganType) {
            case (#moneyTrickster) { true };
            case (#aoeSkim) { true };
            case (#mintTaxSiphon) { true };
            case (#downlineHeist) { true };
            case (#purseCutter) { true };
            case (#whaleRebalance) { true };
            case (_) { false };
        };
        let modPct : Int = if (isAggressive) { rubberBandMod(casterBalPre, targetBalForRoll) } else { 0 };
        let outcome = determineOutcomeWithMod(shenaniganType, modPct);

        // Determine the cost this outcome charges, then clamp to balance so
        // an unaffordable backfire/failure just zeros the caster instead of
        // trapping mid-cast. (When debt is added in a follow-up phase, the
        // shortfall will be written here instead of vanishing.)
        let costForOutcomeUnits = switch (outcome) {
            case (#success) { costSuccessUnits };
            case (#fail) { costFailureUnits };
            case (#backfire) { costBackfireUnits };
        };
        let actualBurnedUnits : Nat = if (costForOutcomeUnits <= casterBalPre) {
            costForOutcomeUnits
        } else {
            casterBalPre
        };

        if (actualBurnedUnits > 0) {
            let burnMemo = "cast-" # Nat.toText(castId);
            switch (await burnFrom(caller, actualBurnedUnits, burnMemo)) {
                case (#Err(msg)) { Debug.trap("Burn failed: " # msg) };
                case (#Ok(_)) {};
            };
        };

        let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) {
            case (null) { 0 };
            case (?n) { n };
        };
        ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + actualBurnedUnits);

        // Caster balance after burn — what they have left when effects fire.
        let casterBal : Nat = casterBalPre - actualBurnedUnits;
        let targetBal : Nat = targetBalForRoll;

        let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } = switch (outcome) {
            case (#success) {
                await applySuccessEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#backfire) {
                await applyBackfireEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#fail) {
                { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
        };

        nextShenaniganId += 1;
        // Record the actual amount paid (after clamp), not the nominal
        // config cost — keeps stats and history honest about what was burned.
        let actualCostFloat = Float.fromInt(actualBurnedUnits) / Float.fromInt(PpLedger.PP_UNIT_SCALE);
        let newShenanigan : ShenaniganRecord = {
            id = castId;
            user = caller;
            shenaniganType;
            target;
            outcome;
            timestamp = Time.now();
            cost = actualCostFloat;
        };
        shenanigans := natMap.put(shenanigans, castId, newShenanigan);

        let _ = appendChatItem(Principal.fromActor(Self), #spellCast({ castId }));

        if (outcome == #backfire) {
            // 25% chance: use the low nibble of timestamp as a coarse coin flip.
            let coin = Int.abs(Time.now()) % 4;
            if (coin == 0) {
                switch (reginaldPickFor("spellBackfire")) {
                    case (?line) {
                        let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "spellBackfire" }));
                    };
                    case (null) {};
                };
            };
        };

        updateShenaniganStats(caller, actualCostFloat, outcome);
        if (outcome == #success or outcome == #backfire) {
            let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
                case (null) { 0 };
                case (?n) { n };
            };
            spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
        };

        {
            outcome;
            ppDeltaCaster = detail.ppDeltaCaster;
            affectedTarget = detail.affectedTarget;
            affectedCount = detail.affectedCount;
        };
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
    ) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } {
        let memo = "spell-" # Nat.toText(castId);
        let nowTs = Time.now();
        let oneDayNs : Int = 86_400_000_000_000;
        let sevenDaysNs : Int = oneDayNs * 7;

        switch (shenaniganType) {
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        if (consumeShieldIfActive(t)) {
                            return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                        };
                        let pct = rollPct(2, 8);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(250));
                        switch (await chipTransfer(t, caster, amount, memo)) {
                            case (#Ok(_)) {
                                return { ppDeltaCaster = amount; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case (#Err(_)) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
            case (#aoeSkim) {
                let pool = enumerateHolders(caster);
                var total : Nat = 0;
                var victims : Nat = 0;
                for (victim in pool.vals()) {
                    if (not consumeShieldIfActive(victim)) {
                        let bal = await getChipBalance(victim);
                        let pct = rollPct(1, 3);
                        let amount = capAt(bal * pct / 100, ppToUnits(60));
                        if (amount > 0) {
                            switch (await chipTransfer(victim, caster, amount, memo)) {
                                case (#Ok(_)) {
                                    total += amount;
                                    victims += 1;
                                };
                                case (#Err(_)) {};
                            };
                        };
                    };
                };
                return { ppDeltaCaster = total; affectedTarget = null; affectedCount = victims };
            };
            case (#renameSpell) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        // If the caster already has an active pending-rename
                        // slot, auto-commit the previous target with a random
                        // name from the pool before stashing the new slot.
                        // Otherwise the prior cast's rename would be silently
                        // overwritten and lost.
                        switch (principalMap.get(pendingRenames, caster)) {
                            case (?prior) {
                                if (Time.now() < prior.expiresAt) {
                                    customDisplayNames := principalMap.put(customDisplayNames, prior.target, {
                                        name = pickRenameName();
                                        expiresAt = nowTs + sevenDaysNs;
                                    });
                                };
                            };
                            case null {};
                        };
                        // Stash a pending-rename slot. Caster has 5 minutes
                        // via setPendingRenameName to choose a name. If the
                        // window lapses the slot simply expires; the cast
                        // cost is already burned.
                        let fiveMinNs : Int = 300_000_000_000;
                        pendingRenames := principalMap.put(pendingRenames, caster, {
                            target = t;
                            expiresAt = nowTs + fiveMinNs;
                        });
                        return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                    };
                };
            };
            case (#mintTaxSiphon) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        if (consumeShieldIfActive(t)) {
                            return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                        };
                        mintSiphons := principalMap.put(mintSiphons, t, {
                            siphoner = caster;
                            expiresAt = nowTs + sevenDaysNs;
                            pctTimes100 = 500;
                            capUnits = ppToUnits(1000);
                            siphonedSoFar = 0;
                        });
                        return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                    };
                };
            };
            case (#downlineHeist) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
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
                            case (null) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                            case (?v) {
                                if (v != caster) {
                                    referralChain := principalMap.put(referralChain, v, caster);
                                    return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                                };
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
            case (#magicMirror) {
                // Stack charges if an active shield already exists. Cap at 3
                // so castLimit=2 can be raised without runaway shielding.
                // Expiry always refreshes to now+1d on each cast.
                let priorCharges : Nat = switch (principalMap.get(shieldsActive, caster)) {
                    case (null) { 0 };
                    case (?s) {
                        if (Time.now() >= s.expiresAt) { 0 }
                        else { s.chargesRemaining };
                    };
                };
                let newCharges : Nat = if (priorCharges + 1 > 3) { 3 } else { priorCharges + 1 };
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = newCharges;
                    expiresAt = nowTs + oneDayNs;
                });
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#ppBoosterAura) {
                let pct = rollPct(105, 115);
                mintMultipliers := principalMap.put(mintMultipliers, caster, {
                    multiplierBps = pct * 100;
                    expiresAt = nowTs + oneDayNs;
                });
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#purseCutter) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        if (consumeShieldIfActive(t)) {
                            return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                        };
                        let pct = rollPct(25, 50);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(800));
                        switch (await burnFrom(t, amount, memo)) {
                            case (#Ok(_)) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case (#Err(_)) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                var total : Nat = 0;
                var victims : Nat = 0;
                for ((whale, bal) in whales.vals()) {
                    if (not consumeShieldIfActive(whale)) {
                        let amount = capAt(bal * 20 / 100, ppToUnits(300));
                        if (amount > 0) {
                            switch (await chipTransfer(whale, caster, amount, memo)) {
                                case (#Ok(_)) {
                                    total += amount;
                                    victims += 1;
                                };
                                case (#Err(_)) {};
                            };
                        };
                    };
                };
                return { ppDeltaCaster = total; affectedTarget = null; affectedCount = victims };
            };
            case (#downlineBoost) {
                cascadeBoosts := principalMap.put(cascadeBoosts, caster, {
                    multiplierBps = 13_000;
                    expiresAt = nowTs + oneDayNs;
                });
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#goldenName) {
                goldenUntil := principalMap.put(goldenUntil, caster, nowTs + oneDayNs);
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
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
    ) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } {
        let memo = "backfire-" # Nat.toText(castId);
        let nowTs = Time.now();
        let oneDayNs : Int = 86_400_000_000_000;
        let sevenDaysNs : Int = oneDayNs * 7;
        let halfWeekNs : Int = oneDayNs * 3;

        switch (shenaniganType) {
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        let pct = rollPct(2, 8);
                        let amount = capAt(casterBal * pct / 100, ppToUnits(250));
                        switch (await chipTransfer(caster, t, amount, memo)) {
                            case (#Ok(_)) {
                                return { ppDeltaCaster = -amount; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case (#Err(_)) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
            case (#aoeSkim) {
                let pct = rollPct(1, 3);
                let loss = casterBal * pct / 100;
                switch (await burnFrom(caster, loss, memo)) {
                    case (#Ok(_)) {
                        return { ppDeltaCaster = -loss; affectedTarget = null; affectedCount = 0 };
                    };
                    case (#Err(_)) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                };
            };
            case (#renameSpell) {
                customDisplayNames := principalMap.put(customDisplayNames, caster, {
                    name = pickRenameName();
                    expiresAt = nowTs + sevenDaysNs;
                });
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#mintTaxSiphon) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        mintSiphons := principalMap.put(mintSiphons, caster, {
                            siphoner = t;
                            expiresAt = nowTs + halfWeekNs;
                            pctTimes100 = 500;
                            capUnits = ppToUnits(1000);
                            siphonedSoFar = 0;
                        });
                        // Target IS affected — they become the siphoner of
                        // caster's mints for 3 days. Mirror the success
                        // branch's affectedCount = 1 so the UI doesn't say
                        // "no observable effect."
                        return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                    };
                };
            };
            case (#downlineHeist) {
                // Caster loses deepest downline to target
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
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
                            case (null) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                            case (?v) {
                                if (v != t) {
                                    referralChain := principalMap.put(referralChain, v, t);
                                    return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                                };
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
            case (#purseCutter) {
                let pct = rollPct(25, 50);
                let amount = capAt(casterBal * pct / 100, ppToUnits(800));
                switch (await burnFrom(caster, amount, memo)) {
                    case (#Ok(_)) {
                        return { ppDeltaCaster = -amount; affectedTarget = null; affectedCount = 0 };
                    };
                    case (#Err(_)) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                };
            };
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                var total : Nat = 0;
                var victims : Nat = 0;
                for ((whale, _) in whales.vals()) {
                    // Re-read caster balance per iteration so successive
                    // payouts are bounded by what's actually left, not the
                    // initial snapshot. With three whales and stale balance
                    // a caster could lose up to 60%; per-iteration caps it
                    // at ~49% (0.2 + 0.16 + 0.128).
                    let liveBal = await getChipBalance(caster);
                    let amount = capAt(liveBal * 20 / 100, ppToUnits(300));
                    if (amount > 0) {
                        switch (await chipTransfer(caster, whale, amount, memo)) {
                            case (#Ok(_)) {
                                total += amount;
                                victims += 1;
                            };
                            case (#Err(_)) {};
                        };
                    };
                };
                return { ppDeltaCaster = -total; affectedTarget = null; affectedCount = victims };
            };
            case (#magicMirror) {
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#ppBoosterAura) {
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#downlineBoost) {
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
            case (#goldenName) {
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
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
    // Trollbox helpers
    // ================================================================

    /// Returns the override for poolName if one exists, else the supplied defaults.
    /// Note: an EMPTY override is honored — caller decides what empty means.
    func effectivePool(poolName : Text, defaultLines : [Text]) : [Text] {
        for ((name, lines) in flavorPoolOverrides.vals()) {
            if (name == poolName) { return lines };
        };
        defaultLines;
    };

    /// Pick a Reginald line for the given trigger, respecting any admin override.
    /// Returns null if the effective pool is empty (admin disabled it) or unknown.
    func reginaldPickFor(triggerKind : Text) : ?Text {
        let defaultLines = Reginald.defaults(triggerKind);
        let poolName = "reginald." # triggerKind;
        let pool = effectivePool(poolName, defaultLines);
        if (pool.size() == 0) { return null };
        ?pool[Int.abs(Time.now()) % pool.size()];
    };

    let CHAT_BUFFER_CAP : Nat = 500;
    let CHAT_MSG_MAX_LEN : Nat = 280;
    let CHAT_RATE_MIN_GAP_NS : Int = 3_000_000_000;      // 3s between posts
    let REACTION_MIN_GAP_NS : Int = 250_000_000;  // 250ms between reactions per user
    let CHAT_RATE_WINDOW_NS : Int = 5 * 60 * 1_000_000_000; // 5-min window
    let CHAT_RATE_WINDOW_MAX : Nat = 15;                   // 15 posts / window
    let KARMA_MIN_PP : Nat = 10;
    let KARMA_REGINALD_THRESHOLD_PP : Nat = 100;
    let CHIME_SOUND_MAX_BYTES : Nat = 200_000;       // 200 KB per file
    let CHIME_SOUND_MAX_COUNT : Nat = 20;            // up to 20 sounds in the pool
    let CHIME_SOUND_NAME_MAX_LEN : Nat = 64;

    // Free emojis = boring, utilitarian acknowledgements. Karma emojis =
    // expressive flair, gated behind a PP burn + recipient payout.
    // Lists are disjoint: 👍 etc. are free-only, 🔥/🚀/etc. are karma-only.
    let FREE_EMOJIS : [Text] = ["👍", "👎", "✅", "❓", "👀"];
    let KARMA_EMOJIS : [Text] = [
        "🔥", "🚀", "💀", "🤣", "😂", "💰", "🎯", "🙏", "💎", "🤡",
        "🐂", "🐻", "⚰️", "🍾", "🥂", "📈", "📉", "💸", "💩", "🫡",
        "😎", "🥹", "🫠", "🚨", "🤝"
    ];

    let BUZZWORDS : [Text] = ["guaranteed", "no risk", "100%", "pump"];

    func emojiAllowed(emoji : Text, allow : [Text]) : Bool {
        for (e in allow.vals()) { if (e == emoji) return true };
        false;
    };

    /// Append a chat item, assigning id, enforcing the 500-item cap by
    /// dropping the oldest. Returns the new id.
    func appendChatItem(author : Principal, kind : ChatItemKind) : Nat {
        let id = nextChatItemId;
        nextChatItemId += 1;
        let item : ChatItem = {
            id;
            author;
            timestamp = Time.now();
            kind;
            reactions = [];
            deleted = false;
        };
        let combined = Array.append(chatItems, [item]);
        let len = combined.size();
        chatItems := if (len > CHAT_BUFFER_CAP) {
            Array.tabulate<ChatItem>(
                CHAT_BUFFER_CAP,
                func(i) { combined[len - CHAT_BUFFER_CAP + i] }
            );
        } else { combined };
        id;
    };

    func findChatItemIndex(id : Nat) : ?Nat {
        var i : Nat = 0;
        let n = chatItems.size();
        while (i < n) {
            if (chatItems[i].id == id) { return ?i };
            i += 1;
        };
        null;
    };

    func updateChatItem(id : Nat, transform : (ChatItem) -> ChatItem) : Bool {
        switch (findChatItemIndex(id)) {
            case (null) { false };
            case (?idx) {
                chatItems := Array.tabulate<ChatItem>(
                    chatItems.size(),
                    func(i) {
                        if (i == idx) { transform(chatItems[i]) } else { chatItems[i] };
                    },
                );
                true;
            };
        };
    };

    // Lazily-expired mute lookup. Expired entries are not garbage-collected
    // on write — setMutedUntil simply overwrites the value, and clearMute
    // deletes it. Expired entries waste a tiny amount of stable memory but
    // cause no correctness issues; mutedUntilFor always filters by Time.now().
    func mutedUntilFor(p : Principal) : ?Int {
        switch (principalMap.get(mutedUntilEntries, p)) {
            case (?exp) { if (exp > Time.now()) { ?exp } else { null } };
            case (null) { null };
        };
    };

    func setMutedUntil(p : Principal, exp : Int) {
        mutedUntilEntries := principalMap.put(mutedUntilEntries, p, exp);
    };

    func clearMute(p : Principal) {
        mutedUntilEntries := principalMap.delete(mutedUntilEntries, p);
    };

    /// Strip ASCII control characters (0x00-0x1F, 0x7F) except 0x09 (tab)
    /// and 0x0A (newline). Keep all multibyte UTF-8 as-is.
    func stripControlChars(s : Text) : Text {
        let buf = Buffer.Buffer<Char>(s.size());
        for (c in s.chars()) {
            let code = Char.toNat32(c);
            let isCtrl = code < 0x20 and code != 0x09 and code != 0x0A;
            let isDel = code == 0x7F;
            if (not isCtrl and not isDel) { buf.add(c) };
        };
        Text.fromIter(buf.vals());
    };

    func textLength(s : Text) : Nat { s.size() };

    /// Returns the canonical rank label for a referral profile. Pure function
    /// over the thresholds in the spec.
    func rankForStats(directs : Nat, totalDownline : Nat) : Text {
        if (directs >= 100 or totalDownline >= 500) { return "Triple-Diamond Founder's Circle" };
        if (directs >= 60 or totalDownline >= 250) { return "Diamond Director" };
        if (directs >= 30 or totalDownline >= 100) { return "Regional Director" };
        if (directs >= 15 or totalDownline >= 40) { return "Senior Advisor" };
        if (directs >= 5 or totalDownline >= 10) { return "Junior Partner" };
        if (directs >= 1) { return "Affiliate" };
        "Cold Lead";
    };

    /// Numeric rank order for upward-only detection.
    func rankOrder(rank : Text) : Nat {
        if (rank == "Cold Lead") { return 0 };
        if (rank == "Affiliate") { return 1 };
        if (rank == "Junior Partner") { return 2 };
        if (rank == "Senior Advisor") { return 3 };
        if (rank == "Regional Director") { return 4 };
        if (rank == "Diamond Director") { return 5 };
        if (rank == "Triple-Diamond Founder's Circle") { return 6 };
        0;
    };

    func previousRankFor(p : Principal) : Text {
        switch (principalMap.get(previousRankEntries, p)) {
            case (?r) { r };
            case (null) { "Cold Lead" };
        };
    };

    func setPreviousRank(p : Principal, rank : Text) {
        previousRankEntries := principalMap.put(previousRankEntries, p, rank);
    };

    /// Compute the recipient's current rank from ReferralStats and emit
    /// #rankUp if it's strictly higher than the cached previous rank.
    /// Also fires the rankUp Reginald trigger when crossing into Affiliate+.
    func maybeEmitRankUp(user : Principal) {
        let stats = computeReferralStats(user);
        let newRank = rankForStats(stats.l1Count, stats.l1Count + stats.l2Count + stats.l3Count);
        let prev = previousRankFor(user);
        if (rankOrder(newRank) > rankOrder(prev)) {
            setPreviousRank(user, newRank);
            let _ = appendChatItem(Principal.fromActor(Self), #rankUp({ user; newRank }));
            if (rankOrder(newRank) >= 1) {
                switch (reginaldPickFor("rankUp")) {
                    case (?line) {
                        let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "rankUp" }));
                    };
                    case (null) {};
                };
            };
        };
    };

    /// Returns #Ok if caller may post now; updates the rate-limit accounting
    /// as a side effect on success. Returns #Err with a user-visible message
    /// on failure (no side effect).
    func checkAndRecordRate(caller : Principal) : { #Ok; #Err : Text } {
        let now = Time.now();

        // Per-caller last-post check.
        let lastPostNs : Int = switch (principalMap.get(lastChatPostEntries, caller)) {
            case (?ts) { ts };
            case (null) { 0 };
        };
        if (lastPostNs != 0 and (now - lastPostNs) < CHAT_RATE_MIN_GAP_NS) {
            return #Err("Slow down.");
        };

        // Per-caller window check. Prune stale timestamps lazily on read.
        let windowStamps : [Int] = switch (principalMap.get(recentPostCountEntries, caller)) {
            case (?stamps) { stamps };
            case (null) { [] };
        };
        let kept = Buffer.Buffer<Int>(windowStamps.size() + 1);
        for (ts in windowStamps.vals()) {
            if ((now - ts) <= CHAT_RATE_WINDOW_NS) { kept.add(ts) };
        };
        if (kept.size() >= CHAT_RATE_WINDOW_MAX) {
            return #Err("Slow down.");
        };

        // Record acceptance.
        kept.add(now);
        lastChatPostEntries := principalMap.put(lastChatPostEntries, caller, now);
        recentPostCountEntries := principalMap.put(recentPostCountEntries, caller, Buffer.toArray(kept));

        #Ok;
    };

    /// Returns #Ok if the caller may add/remove a reaction now; updates the
    /// per-user clock on success. No-op on failure (returns #Err).
    func checkAndRecordReactionRate(caller : Principal) : { #Ok; #Err : Text } {
        let now = Time.now();
        let last : Int = switch (principalMap.get(lastReactionEntries, caller)) {
            case (?ts) { ts };
            case (null) { 0 };
        };
        if (last != 0 and (now - last) < REACTION_MIN_GAP_NS) {
            return #Err("Slow down.");
        };
        lastReactionEntries := principalMap.put(lastReactionEntries, caller, now);
        #Ok;
    };

    func containsBuzzword(body : Text) : Bool {
        let lower = Text.map(body, func(c : Char) : Char {
            let code = Char.toNat32(c);
            if (code >= 0x41 and code <= 0x5A) {
                Char.fromNat32(code + 32);
            } else { c };
        });
        for (kw in BUZZWORDS.vals()) {
            if (Text.contains(lower, #text kw)) { return true };
        };
        false;
    };

    // ================================================================
    // Trollbox public reads
    // ================================================================

    /// Returns the most-recent chat items newest-first. Capped server-side
    /// at 100 per call regardless of the caller's requested limit.
    public query func getRecentChatItems(limit : Nat) : async [ChatItem] {
        let cap : Nat = if (limit > 100) { 100 } else { limit };
        let total = chatItems.size();
        let n : Nat = if (cap > total) { total } else { cap };
        Array.tabulate<ChatItem>(n, func(i) { chatItems[total - 1 - i] });
    };

    public query func getCurrentPin() : async ?ChatItem {
        switch (currentPinId) {
            case (null) { null };
            case (?pid) {
                switch (findChatItemIndex(pid)) {
                    case (null) { null };
                    case (?idx) { ?chatItems[idx] };
                };
            };
        };
    };

    public query func isMuted(user : Principal) : async ?Int {
        mutedUntilFor(user);
    };

    // ================================================================
    // Trollbox public writes
    // ================================================================

    public shared ({ caller }) func postChatMessage(body : Text, replyTo : ?Nat) : async { #Ok : Nat; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };

        let cleaned = stripControlChars(body);
        let len = textLength(cleaned);
        if (len == 0) { return #Err("Message cannot be empty") };
        if (len > CHAT_MSG_MAX_LEN) { return #Err("Message exceeds 280 characters") };

        // Reject whitespace-only messages (parity with frontend trim check).
        var hasNonWhitespace = false;
        for (c in cleaned.chars()) {
            let code = Char.toNat32(c);
            if (code != 0x20 and code != 0x09 and code != 0x0A and code != 0x0D) {
                hasNonWhitespace := true;
            };
        };
        if (not hasNonWhitespace) { return #Err("Message cannot be empty") };

        switch (mutedUntilFor(caller)) {
            case (?exp) { return #Err("You are muted until " # Int.toText(exp)) };
            case (null) {};
        };

        switch (checkAndRecordRate(caller)) {
            case (#Err(msg)) { return #Err(msg) };
            case (#Ok) {};
        };

        let id = appendChatItem(caller, #userMessage({ body = cleaned; replyTo }));

        if (containsBuzzword(cleaned)) {
            switch (reginaldPickFor("buzzword")) {
                case (?line) {
                    let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "buzzword" }));
                };
                case (null) {};
            };
        };

        #Ok(id);
    };

    public shared ({ caller }) func addReaction(itemId : Nat, emoji : Text) : async { #Ok; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        if (not emojiAllowed(emoji, FREE_EMOJIS)) { return #Err("Emoji not allowed") };
        switch (checkAndRecordReactionRate(caller)) {
            case (#Err(msg)) { return #Err(msg) };
            case (#Ok) {};
        };

        let updated = updateChatItem(itemId, func(item : ChatItem) : ChatItem {
            let buf = Buffer.Buffer<Reaction>(item.reactions.size() + 1);
            var matched = false;
            for (r in item.reactions.vals()) {
                if (r.emoji == emoji) {
                    matched := true;
                    var has = false;
                    for (p in r.reactors.vals()) { if (p == caller) { has := true } };
                    if (has) {
                        buf.add(r);
                    } else {
                        let reactors = Array.append(r.reactors, [caller]);
                        buf.add({ emoji = r.emoji; reactors; karmaPpBurned = r.karmaPpBurned });
                    };
                } else {
                    buf.add(r);
                };
            };
            if (not matched) {
                buf.add({ emoji; reactors = [caller]; karmaPpBurned = 0 });
            };
            { item with reactions = Buffer.toArray(buf) };
        });
        if (updated) { #Ok } else { #Err("No such item") };
    };

    public shared ({ caller }) func removeReaction(itemId : Nat, emoji : Text) : async { #Ok; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        switch (checkAndRecordReactionRate(caller)) {
            case (#Err(msg)) { return #Err(msg) };
            case (#Ok) {};
        };
        let updated = updateChatItem(itemId, func(item : ChatItem) : ChatItem {
            let buf = Buffer.Buffer<Reaction>(item.reactions.size());
            for (r in item.reactions.vals()) {
                if (r.emoji == emoji) {
                    let kept = Buffer.Buffer<Principal>(r.reactors.size());
                    for (p in r.reactors.vals()) {
                        if (p != caller) { kept.add(p) };
                    };
                    if (kept.size() > 0 or r.karmaPpBurned > 0) {
                        buf.add({ emoji = r.emoji; reactors = Buffer.toArray(kept); karmaPpBurned = r.karmaPpBurned });
                    };
                } else {
                    buf.add(r);
                };
            };
            { item with reactions = Buffer.toArray(buf) };
        });
        if (updated) { #Ok } else { #Err("No such item") };
    };

    // Karma reaction with 40/50/10 split:
    //   40% → message author (recipient payout)
    //   50% → burn (transfer to minting account)
    //   10% → house principal (Management's cut)
    // Self-karma is blocked. Canister-authored messages (e.g. Reginald) route
    // the 40% to house as well — tipping Management for the snark.
    public shared ({ caller }) func addKarmaReaction(itemId : Nat, emoji : Text, ppToBurn : Nat) : async { #Ok; #Err : Text } {
        if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
        if (not emojiAllowed(emoji, KARMA_EMOJIS)) { return #Err("Emoji not allowed") };
        if (ppToBurn < KARMA_MIN_PP) { return #Err("Minimum 10 PP") };

        let idx = switch (findChatItemIndex(itemId)) {
            case (null) { return #Err("No such item") };
            case (?i) { i };
        };
        let item = chatItems[idx];
        if (item.author == caller) { return #Err("Can't karma your own message") };

        let units = ppToUnits(ppToBurn);
        let burnUnits = units * 50 / 100;
        let mgmtUnits = units * 10 / 100;
        let recipientUnits : Nat = units - burnUnits - mgmtUnits;

        // Pre-check balance to avoid partial-spend on failure mid-sequence.
        let balance = await getChipBalance(caller);
        if (balance < units) {
            return #Err("Insufficient PP: need " # Nat.toText(ppToBurn) # ", have " # Nat.toText(balance / PpLedger.PP_UNIT_SCALE));
        };

        // Recipient: canister-authored items route to house (Management).
        let recipient = if (item.author == Principal.fromActor(Self)) { house() } else { item.author };

        // 1. Burn (50%).
        let burnMemo = "karma-burn-" # Nat.toText(itemId);
        switch (await burnFrom(caller, burnUnits, burnMemo)) {
            case (#Err(msg)) { return #Err("Burn failed: " # msg) };
            case (#Ok(_)) {};
        };

        // 2. Pay recipient (40%).
        let payMemo = "karma-pay-" # Nat.toText(itemId);
        switch (await chipTransfer(caller, recipient, recipientUnits, payMemo)) {
            case (#Err(msg)) { return #Err("Recipient pay failed: " # msg) };
            case (#Ok(_)) {};
        };

        // 3. Management cut (10%).
        let mgmtMemo = "karma-mgmt-" # Nat.toText(itemId);
        switch (await chipTransfer(caller, house(), mgmtUnits, mgmtMemo)) {
            case (#Err(msg)) { return #Err("Management cut failed: " # msg) };
            case (#Ok(_)) {};
        };

        let updated = updateChatItem(itemId, func(it : ChatItem) : ChatItem {
            let buf = Buffer.Buffer<Reaction>(it.reactions.size() + 1);
            var matched = false;
            for (r in it.reactions.vals()) {
                if (r.emoji == emoji) {
                    matched := true;
                    var has = false;
                    for (p in r.reactors.vals()) { if (p == caller) { has := true } };
                    let reactors = if (has) { r.reactors } else { Array.append(r.reactors, [caller]) };
                    // karmaPpBurned tracks total karma value SPENT on this
                    // reaction (display signal for prestige). The actual burn
                    // is 50%; the rest flows to recipient + management.
                    buf.add({ emoji = r.emoji; reactors; karmaPpBurned = r.karmaPpBurned + units });
                } else {
                    buf.add(r);
                };
            };
            if (not matched) {
                buf.add({ emoji; reactors = [caller]; karmaPpBurned = units });
            };
            { it with reactions = Buffer.toArray(buf) };
        });

        if (not updated) { return #Err("No such item") };

        let priorBurn = switch (principalMap.get(ppBurnedPerPlayer, caller)) {
            case (null) { 0 };
            case (?n) { n };
        };
        ppBurnedPerPlayer := principalMap.put(ppBurnedPerPlayer, caller, priorBurn + burnUnits);

        let priorRecv = switch (principalMap.get(karmaReceivedPerPlayer, recipient)) {
            case (null) { 0 };
            case (?n) { n };
        };
        karmaReceivedPerPlayer := principalMap.put(karmaReceivedPerPlayer, recipient, priorRecv + recipientUnits);

        if (ppToBurn >= KARMA_REGINALD_THRESHOLD_PP) {
            switch (reginaldPickFor("karma")) {
                case (?line) {
                    let _ = appendChatItem(Principal.fromActor(Self), #reginald({ line; triggerKind = "karma" }));
                };
                case (null) {};
            };
        };

        #Ok;
    };

    public query func getKarmaReceived(p : Principal) : async Nat {
        switch (principalMap.get(karmaReceivedPerPlayer, p)) {
            case (null) { 0 };
            case (?n) { n };
        };
    };

    public query func listChimeSounds() : async [ChimeSoundMeta] {
        Array.tabulate<ChimeSoundMeta>(chimeSoundPool.size(), func(i) {
            let s = chimeSoundPool[i];
            { name = s.name; mimeType = s.mimeType; sizeBytes = s.bytes.size(); uploadedAt = s.uploadedAt };
        });
    };

    public query func getChimeSound(name : Text) : async ?ChimeSound {
        for (s in chimeSoundPool.vals()) {
            if (s.name == name) { return ?s };
        };
        null;
    };

    // ================================================================
    // Trollbox admin
    // ================================================================

    public shared ({ caller }) func adminSetPin(body : Text) : async Nat {
        requireAdmin(caller);
        let cleaned = stripControlChars(body);
        let id = appendChatItem(caller, #pinUpdate({ body = cleaned }));
        currentPinId := if (textLength(cleaned) == 0) { null } else { ?id };
        id;
    };

    public shared ({ caller }) func adminDeleteChatItem(itemId : Nat) : async () {
        requireAdmin(caller);
        let _ = updateChatItem(itemId, func(item : ChatItem) : ChatItem {
            { item with deleted = true };
        });
    };

    public shared ({ caller }) func adminUploadChimeSound(name : Text, mimeType : Text, bytes : Blob) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        if (name.size() == 0 or name.size() > CHIME_SOUND_NAME_MAX_LEN) {
            return #Err("Name must be 1-" # Nat.toText(CHIME_SOUND_NAME_MAX_LEN) # " characters");
        };
        if (not Text.startsWith(mimeType, #text "audio/")) {
            return #Err("mimeType must begin with audio/");
        };
        if (bytes.size() == 0) { return #Err("Empty file") };
        if (bytes.size() > CHIME_SOUND_MAX_BYTES) {
            return #Err("File exceeds " # Nat.toText(CHIME_SOUND_MAX_BYTES) # " bytes");
        };

        // Upsert by name. If full and not replacing, reject.
        let buf = Buffer.Buffer<ChimeSound>(chimeSoundPool.size() + 1);
        var replaced = false;
        for (s in chimeSoundPool.vals()) {
            if (s.name == name) {
                buf.add({ name; bytes; mimeType; uploadedAt = Time.now() });
                replaced := true;
            } else {
                buf.add(s);
            };
        };
        if (not replaced) {
            if (chimeSoundPool.size() >= CHIME_SOUND_MAX_COUNT) {
                return #Err("Pool is full (max " # Nat.toText(CHIME_SOUND_MAX_COUNT) # " sounds)");
            };
            buf.add({ name; bytes; mimeType; uploadedAt = Time.now() });
        };
        chimeSoundPool := Buffer.toArray(buf);
        #Ok;
    };

    public shared ({ caller }) func adminDeleteChimeSound(name : Text) : async () {
        requireAdmin(caller);
        let buf = Buffer.Buffer<ChimeSound>(chimeSoundPool.size());
        for (s in chimeSoundPool.vals()) {
            if (s.name != name) { buf.add(s) };
        };
        chimeSoundPool := Buffer.toArray(buf);
    };

    public shared ({ caller }) func adminMuteUser(user : Principal, durationSeconds : Nat) : async () {
        requireAdmin(caller);
        let now = Time.now();
        let durNs : Int = durationSeconds * 1_000_000_000;
        setMutedUntil(user, now + durNs);
    };

    public shared ({ caller }) func adminUnmute(user : Principal) : async () {
        requireAdmin(caller);
        clearMute(user);
    };

    public shared ({ caller }) func adminPostAsReginald(line : Text) : async Nat {
        requireAdmin(caller);
        let cleaned = stripControlChars(line);
        appendChatItem(caller, #reginald({ line = cleaned; triggerKind = "manual" }));
    };

    /// One-shot deploy-time backfill: populate previousRankEntries with each
    /// known principal's current rank. Prevents #rankUp spam after the trollbox
    /// deploys. Idempotent — safe to call multiple times. Admin-only.
    public shared ({ caller }) func adminSeedRankCache() : async Nat {
        requireAdmin(caller);
        var count : Nat = 0;
        for ((p, _) in principalMap.entries(referralChain)) {
            let stats = computeReferralStats(p);
            let rank = rankForStats(stats.l1Count, stats.l1Count + stats.l2Count + stats.l3Count);
            previousRankEntries := principalMap.put(previousRankEntries, p, rank);
            count += 1;
        };
        count;
    };

    /// One-shot deploy-time backfill: mark every principal who has ever been
    /// granted a signup gift as already-announced. Prevents #signup spam after
    /// the trollbox deploys. Idempotent. Admin-only.
    public shared ({ caller }) func adminSeedSignupAnnounced() : async Nat {
        requireAdmin(caller);
        let now = Time.now();
        var count : Nat = 0;
        for ((p, _) in principalMap.entries(signupGiftClaimed)) {
            switch (principalMap.get(signupAnnouncedSet, p)) {
                case (?_) {};
                case (null) {
                    signupAnnouncedSet := principalMap.put(signupAnnouncedSet, p, now);
                    count += 1;
                };
            };
        };
        count;
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

    /// Read the caller's (or any principal's) active Magic Mirror shield, if any.
    /// Returns null when no shield is active or it has expired.
    public query func getActiveShield(p : Principal) : async ?{
        chargesRemaining : Nat;
        expiresAt : Int;
    } {
        switch (principalMap.get(shieldsActive, p)) {
            case (null) { null };
            case (?s) {
                if (Time.now() >= s.expiresAt) { null }
                else { ?{ chargesRemaining = s.chargesRemaining; expiresAt = s.expiresAt } };
            };
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
    /// Deprecated. The deductive cascade ignores referralL[1-3]Bps.
    /// Use setCascadeBps(initial, passthrough) instead.
    public shared ({ caller }) func setReferralBps(_l1 : Nat, _l2 : Nat, _l3 : Nat) : async () {
        requireAdmin(caller);
        Debug.trap("setReferralBps is deprecated — use setCascadeBps(initial, passthrough) for the deductive cascade");
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
        startObserver<system>();
    };

    public shared ({ caller }) func setHousePrincipal(p : Principal) : async () {
        requireAdmin(caller);
        if (Principal.isAnonymous(p)) {
            Debug.trap("housePrincipal cannot be the anonymous principal");
        };
        housePrincipal := ?p;
    };

    public shared ({ caller }) func setCascadeBps(initial : Nat, passthrough : Nat) : async () {
        requireAdmin(caller);
        if (initial > 10_000 or passthrough > 10_000) {
            Debug.trap("BPS values must be ≤ 10_000");
        };
        mintConfig := {
            mintConfig with
            cascadeInitialBps = initial;
            cascadePassthroughBps = passthrough;
        };
    };

    public shared ({ caller }) func setSignupGiftPp(v : Nat) : async () {
        requireAdmin(caller);
        if (v > 1_000_000) {
            Debug.trap("signupGiftPp must be ≤ 1_000_000 whole PP (guard against typo-induced mass mints)");
        };
        mintConfig := { mintConfig with signupGiftPp = v };
    };

    public shared ({ caller }) func setActivityRequiresDeposit(b : Bool) : async () {
        requireAdmin(caller);
        mintConfig := { mintConfig with activityRequiresDeposit = b };
    };

    public shared ({ caller }) func setActivityWindowDays(d : ?Nat) : async () {
        requireAdmin(caller);
        switch (d) {
            case (null) {};
            case (?n) {
                if (n == 0 or n > 3650) {
                    Debug.trap("activityWindowDays must be in [1, 3650] or null");
                };
            };
        };
        mintConfig := { mintConfig with activityWindowDays = d };
    };

    /// One-shot post-upgrade seeding for the deductive-cascade rollout.
    ///
    /// 1. housePrincipal := ?caller if null
    /// 2. For every player with an existing game record: signupGiftClaimed
    ///    [player] := earliest game timestamp (prevents retroactive gifts).
    /// 3. For every player with ≥0.1 ICP cumulative deposit (game or backer):
    ///    lastQualifyingDeposit[player] := Time.now() (conservative: all
    ///    existing depositors are treated as just-qualified).
    /// 4. Backfill referrerToDownline from referralChain.
    ///
    /// Idempotent: re-running produces the same end state. Admin-only.
    public shared ({ caller }) func seedMigrationV2() : async () {
        requireAdmin(caller);

        // 1. Initialize housePrincipal if needed.
        switch (housePrincipal) {
            case (?_) {};
            case (null) { housePrincipal := ?caller };
        };

        let ponziMath = getPonziMath();
        let now = Time.now();

        // 2 & 3. Grandfather signupGiftClaimed + seed lastQualifyingDeposit from games.
        let games = try { await ponziMath.getAllGames() } catch (_) { [] };
        for (game in games.vals()) {
            // signupGiftClaimed: take the earliest game.startTime per player so
            // recentSignups reflects real join times, not the seeding moment.
            switch (principalMap.get(signupGiftClaimed, game.player)) {
                case (?existing) {
                    if (game.startTime < existing) {
                        signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, game.startTime);
                    };
                };
                case (null) {
                    signupGiftClaimed := principalMap.put(signupGiftClaimed, game.player, game.startTime);
                };
            };
            // lastQualifyingDeposit: any ≥0.1 ICP game qualifies; set to now (conservative).
            if (game.amount >= 0.1) {
                lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, game.player, now);
            };
        };

        // 3 (cont). Same for backer positions.
        let backers = try { await ponziMath.getBackerPositions() } catch (_) { [] };
        for (backer in backers.vals()) {
            if (backer.amount >= 0.1) {
                lastQualifyingDeposit := principalMap.put(lastQualifyingDeposit, backer.owner, now);
                // Also grandfather signupGiftClaimed for backers without game records.
                // Prefer firstDepositDate; fall back to backer.startTime.
                let backerJoinTime : Int = switch (backer.firstDepositDate) {
                    case (?t) { t };
                    case (null) { backer.startTime };
                };
                switch (principalMap.get(signupGiftClaimed, backer.owner)) {
                    case (?existing) {
                        if (backerJoinTime < existing) {
                            signupGiftClaimed := principalMap.put(signupGiftClaimed, backer.owner, backerJoinTime);
                        };
                    };
                    case (null) {
                        signupGiftClaimed := principalMap.put(signupGiftClaimed, backer.owner, backerJoinTime);
                    };
                };
            };
        };

        // 4. Backfill referrerToDownline from referralChain.
        // Reset to empty first to ensure idempotency.
        referrerToDownline := principalMap.empty<List.List<Principal>>();
        for ((downliner, referrer) in principalMap.entries(referralChain)) {
            let existing = switch (principalMap.get(referrerToDownline, referrer)) {
                case (?list) { list };
                case (null) { List.nil<Principal>() };
            };
            referrerToDownline := principalMap.put(referrerToDownline, referrer, List.push(downliner, existing));
        };

        // 5. Release the observer. From this point onward, processNewGames /
        // processBackerDeltas will mint for new events; existing players are
        // already grandfathered above.
        bootstrapped := true;
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
        startObserver<system>();
    };

    /// Manual one-shot observer tick (admin debug).
    public shared ({ caller }) func runObserverOnce() : async () {
        requireAdmin(caller);
        await observerTick();
    };

    /// Inspect the bootstrap gate. Useful during deploy to confirm that
    /// seedMigrationV2 has flipped the flag before player traffic resumes.
    public query func isBootstrapped() : async Bool { bootstrapped };

    // ================================================================
    // Admin Functions
    // ================================================================

    public shared ({ caller }) func updateShenaniganConfig(config : ShenaniganConfig) : async () {
        requireAdmin(caller);
        if (config.successOdds + config.failureOdds + config.backfireOdds != 100) {
            Debug.trap("Success, failure, and backfire odds must sum to 100");
        };
        if (config.costSuccess < 0.0 or config.costFailure < 0.0 or config.costBackfire < 0.0
            or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
            Debug.trap("Costs, duration, cooldown, and cast limit must be non-negative");
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
            if (config.costSuccess < 0.0 or config.costFailure < 0.0 or config.costBackfire < 0.0
                or config.duration < 0 or config.cooldown < 0 or config.castLimit < 0) {
                Debug.trap("Costs, duration, cooldown, and cast limit must be non-negative");
            };
            shenaniganConfigs := natMap.put(shenaniganConfigs, config.id, config);
        };
    };

    /// Admin-only: replace a flavor pool's override. Pass an empty list to
    /// explicitly disable a Reginald trigger or empty the rename pool.
    /// To restore defaults, call adminClearFlavorPool instead.
    public shared ({ caller }) func adminSetFlavorPool(name : Text, lines : [Text]) : async () {
        requireAdmin(caller);
        let buf = Buffer.Buffer<(Text, [Text])>(flavorPoolOverrides.size() + 1);
        var replaced = false;
        for ((n, l) in flavorPoolOverrides.vals()) {
            if (n == name) {
                buf.add((name, lines));
                replaced := true;
            } else {
                buf.add((n, l));
            };
        };
        if (not replaced) { buf.add((name, lines)) };
        flavorPoolOverrides := Buffer.toArray(buf);
    };

    /// Admin-only: remove the override entirely, restoring the hardcoded default.
    public shared ({ caller }) func adminClearFlavorPool(name : Text) : async () {
        requireAdmin(caller);
        let buf = Buffer.Buffer<(Text, [Text])>(flavorPoolOverrides.size());
        for ((n, l) in flavorPoolOverrides.vals()) {
            if (n != name) { buf.add((n, l)) };
        };
        flavorPoolOverrides := Buffer.toArray(buf);
    };

    public query func listFlavorPools() : async [(Text, [Text])] {
        flavorPoolOverrides;
    };

    /// Returns the hardcoded default lines for a known pool name. Useful for
    /// the admin UI to show "this is what defaults look like" without
    /// duplicating the lists in the frontend.
    public query func getFlavorPoolDefaults(name : Text) : async [Text] {
        if (name == "renameNamePool") { return renameNamePool };
        if (Text.startsWith(name, #text "reginald.")) {
            let trigger = Text.trimStart(name, #text "reginald.");
            return Reginald.defaults(trigger);
        };
        []; // Unknown pool — frontend uses its own defaults for spellFlavor.* etc.
    };

    // ========================================================================
    // ICRC-21 consent messages, ICRC-28 trusted origins, ICRC-10 standards.
    //
    // Required for Oisy (any ICRC-25 signer wallet) to display a consent
    // message before signing any update call on this canister. Without these
    // methods Oisy's icrc49_call_canister fails entirely — it does NOT fall
    // back to a blind-signing warning for custom canister methods.
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
