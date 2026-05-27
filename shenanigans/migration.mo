import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Nat "mo:base/Nat";
import Buffer "mo:base/Buffer";

module {

    // ================================================================
    // V2 types — Dealer→Backer rename (already deployed; left for reference)
    // ================================================================

    type V2OldMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        dealerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
    };

    type V2NewMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;
        referralL2Bps : Nat;
        referralL3Bps : Nat;
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
    };

    type SeenMap = OrderedMap.Map<Principal, Float>;

    public func runV2(old : {
        var mintConfig : V2OldMintConfig;
        var backendPrincipal : ?Principal;
        var dealerSeen : SeenMap;
    }) : {
        var mintConfig : V2NewMintConfig;
        var ponziMathPrincipal : ?Principal;
        var backerSeen : SeenMap;
    } {
        let oldCfg = old.mintConfig;
        {
            var mintConfig = {
                simple21DayPpPerIcp = oldCfg.simple21DayPpPerIcp;
                compounding15DayPpPerIcp = oldCfg.compounding15DayPpPerIcp;
                compounding30DayPpPerIcp = oldCfg.compounding30DayPpPerIcp;
                backerPpPerIcp = oldCfg.dealerPpPerIcp;
                referralL1Bps = oldCfg.referralL1Bps;
                referralL2Bps = oldCfg.referralL2Bps;
                referralL3Bps = oldCfg.referralL3Bps;
                minDepositPp = oldCfg.minDepositPp;
                cashOutDelaySeconds = oldCfg.cashOutDelaySeconds;
                observerIntervalSeconds = oldCfg.observerIntervalSeconds;
            };
            var ponziMathPrincipal = old.backendPrincipal;
            var backerSeen = old.dealerSeen;
        };
    };

    // ================================================================
    // V3 — Deductive cascade rollout
    //
    // Two transformations in one migration:
    //
    // (a) Extends MintConfig with 5 admin-tunable fields:
    //       cascadeInitialBps       (10% deduction off the top)
    //       cascadePassthroughBps   (50% kept by each active upline)
    //       signupGiftPp            (500 PP signup gift; 0 disables)
    //       activityRequiresDeposit (cascade skips inactive uplines)
    //       activityWindowDays      (null = lifetime; ?n = last n days)
    //
    // (b) Drops legacy unused stable fields that an earlier
    //     half-finished MLM experiment left on the actor. The deployed
    //     canister still carries empty/zero values for these — the
    //     migration explicitly drops them so the new actor's stable
    //     signature is clean.
    //
    // Old referralL[1-3]Bps fields stay on the record (deprecated,
    // unused by the new cascade) so the candid signature stays stable
    // for admin tooling that reads them.
    // ================================================================

    type V3OldMintConfig = V2NewMintConfig;

    type V3NewMintConfig = {
        simple21DayPpPerIcp : Nat;
        compounding15DayPpPerIcp : Nat;
        compounding30DayPpPerIcp : Nat;
        backerPpPerIcp : Nat;
        referralL1Bps : Nat;       // deprecated; unused by new cascade
        referralL2Bps : Nat;       // deprecated; unused by new cascade
        referralL3Bps : Nat;       // deprecated; unused by new cascade
        minDepositPp : Nat;
        cashOutDelaySeconds : Nat;
        observerIntervalSeconds : Nat;
        cascadeInitialBps : Nat;
        cascadePassthroughBps : Nat;
        signupGiftPp : Nat;
        activityRequiresDeposit : Bool;
        activityWindowDays : ?Nat;
    };

    type ActiveDepositorsLegacy = OrderedMap.Map<Principal, Bool>;

    public func runV3(
        old : {
            var mintConfig : V3OldMintConfig;
            // Legacy fields explicitly named so the migration drops them by
            // omission from the output record. All carry trivial values on
            // the deployed canister (zero / empty / anonymous principal).
            var CASCADE_MAX_DEPTH : Nat;
            var activeDepositors : ActiveDepositorsLegacy;
            var cascadeBps : Nat;
            var cascadePassthrough : Nat;
            var charlesPrincipal : Principal;
            var signupGiftPp : Nat;
        },
    ) : {
        var mintConfig : V3NewMintConfig;
    } {
        let o = old.mintConfig;
        {
            var mintConfig = {
                simple21DayPpPerIcp = o.simple21DayPpPerIcp;
                compounding15DayPpPerIcp = o.compounding15DayPpPerIcp;
                compounding30DayPpPerIcp = o.compounding30DayPpPerIcp;
                backerPpPerIcp = o.backerPpPerIcp;
                referralL1Bps = o.referralL1Bps;
                referralL2Bps = o.referralL2Bps;
                referralL3Bps = o.referralL3Bps;
                minDepositPp = o.minDepositPp;
                cashOutDelaySeconds = o.cashOutDelaySeconds;
                observerIntervalSeconds = o.observerIntervalSeconds;
                cascadeInitialBps = 1000;        // 10%
                cascadePassthroughBps = 5000;    // 50%
                signupGiftPp = 500;
                activityRequiresDeposit = true;
                activityWindowDays = null;       // lifetime
            };
        };
    };

    // ================================================================
    // V4 — Per-outcome shenanigan costs
    //
    // Splits the single `cost` field on each ShenaniganConfig into three
    // outcome-specific fields. Migration policy: map the old single cost
    // to all three new fields so no spell's economics change implicitly —
    // admin retunes per-outcome from the admin panel after the upgrade.
    //
    // Only `shenaniganConfigs` is named in the domain/codomain; every
    // other stable field on the actor (player principals, balances,
    // referral chain, etc.) flows through unchanged because its type is
    // unchanged.
    // ================================================================

    type V4OldShenaniganConfig = {
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

    type V4NewShenaniganConfig = {
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

    type V4OldConfigMap = OrderedMap.Map<Nat, V4OldShenaniganConfig>;
    type V4NewConfigMap = OrderedMap.Map<Nat, V4NewShenaniganConfig>;

    public func runV4(
        old : { var shenaniganConfigs : V4OldConfigMap }
    ) : { var shenaniganConfigs : V4NewConfigMap } {
        let natMap = OrderedMap.Make<Nat>(Nat.compare);
        let migrated = natMap.map<V4OldShenaniganConfig, V4NewShenaniganConfig>(
            old.shenaniganConfigs,
            func(_id : Nat, c : V4OldShenaniganConfig) : V4NewShenaniganConfig {
                {
                    id = c.id;
                    name = c.name;
                    description = c.description;
                    costSuccess = c.cost;
                    costFailure = c.cost;
                    costBackfire = c.cost;
                    successOdds = c.successOdds;
                    failureOdds = c.failureOdds;
                    backfireOdds = c.backfireOdds;
                    duration = c.duration;
                    cooldown = c.cooldown;
                    effectValues = c.effectValues;
                    castLimit = c.castLimit;
                    backgroundColor = c.backgroundColor;
                };
            },
        );
        { var shenaniganConfigs = migrated };
    };

    // ================================================================
    // V5 — Add optional backfireDescription field to ShenaniganConfig
    //
    // The stable-interface compatibility check rejects adding ANY field
    // (even optional) to a record stored in stable state without an
    // explicit migration. This migration copies every existing config
    // through and sets backfireDescription = null. Frontend falls back
    // to a hardcoded TS map when null, so no visible behavior changes
    // until admin saves a value via the panel.
    // ================================================================

    type V5OldShenaniganConfig = V4NewShenaniganConfig;

    type V5NewShenaniganConfig = {
        id : Nat;
        name : Text;
        description : Text;
        backfireDescription : ?Text;
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

    type V5OldConfigMap = OrderedMap.Map<Nat, V5OldShenaniganConfig>;
    type V5NewConfigMap = OrderedMap.Map<Nat, V5NewShenaniganConfig>;

    public func runV5(
        old : { var shenaniganConfigs : V5OldConfigMap }
    ) : { var shenaniganConfigs : V5NewConfigMap } {
        let natMap = OrderedMap.Make<Nat>(Nat.compare);
        let migrated = natMap.map<V5OldShenaniganConfig, V5NewShenaniganConfig>(
            old.shenaniganConfigs,
            func(_id : Nat, c : V5OldShenaniganConfig) : V5NewShenaniganConfig {
                {
                    id = c.id;
                    name = c.name;
                    description = c.description;
                    backfireDescription = null;  // admin sets via the panel
                    costSuccess = c.costSuccess;
                    costFailure = c.costFailure;
                    costBackfire = c.costBackfire;
                    successOdds = c.successOdds;
                    failureOdds = c.failureOdds;
                    backfireOdds = c.backfireOdds;
                    duration = c.duration;
                    cooldown = c.cooldown;
                    effectValues = c.effectValues;
                    castLimit = c.castLimit;
                    backgroundColor = c.backgroundColor;
                };
            },
        );
        { var shenaniganConfigs = migrated };
    };

    // ================================================================
    // V6 — Embed spell-cast metadata in #spellCast chat items
    //
    // Previously the #spellCast variant carried only { castId : Nat }, so the
    // trollbox had to join against the shenanigans map to render the caster
    // name, spell type, target, and outcome. getRecentShenanigans is capped at
    // a small number of records, so most chat items couldn't find their record
    // and degraded to an anonymous "Someone cast a spell." fallback. This
    // migration enriches each historical #spellCast chat item with the fields
    // looked up from the shenanigans map. New casts emit the enriched form
    // directly (see main.mo cast site).
    //
    // The shenanigans map carries through unchanged but is referenced in the
    // input so the migration can read it. If a record is missing for a given
    // castId (shouldn't happen — shenanigans are never pruned — but defensive
    // against past data quirks), the chat item is dropped. currentPinId and
    // findChatItemIndex tolerate missing ids, so dropping is safe.
    // ================================================================

    type V6ShenaniganType = {
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

    type V6ShenaniganOutcome = {
        #success;
        #fail;
        #backfire;
    };

    type V6ShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : V6ShenaniganType;
        target : ?Principal;
        outcome : V6ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
    };

    type V6Reaction = {
        emoji : Text;
        reactors : [Principal];
        karmaPpBurned : Nat;
    };

    type V6OldChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : { castId : Nat };
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    type V6NewChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : {
            castId : Nat;
            caster : Principal;
            shenaniganType : V6ShenaniganType;
            target : ?Principal;
            outcome : V6ShenaniganOutcome;
        };
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    type V6OldChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V6OldChatItemKind;
        reactions : [V6Reaction];
        deleted : Bool;
    };

    type V6NewChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V6NewChatItemKind;
        reactions : [V6Reaction];
        deleted : Bool;
    };

    type V6ShenaniganMap = OrderedMap.Map<Nat, V6ShenaniganRecord>;

    public func runV6(
        old : {
            var chatItems : [V6OldChatItem];
            var shenanigans : V6ShenaniganMap;
        }
    ) : {
        var chatItems : [V6NewChatItem];
        var shenanigans : V6ShenaniganMap;
    } {
        let natMap = OrderedMap.Make<Nat>(Nat.compare);
        let migrated = Buffer.Buffer<V6NewChatItem>(old.chatItems.size());
        for (item in old.chatItems.vals()) {
            let newKind : ?V6NewChatItemKind = switch (item.kind) {
                case (#spellCast({ castId })) {
                    switch (natMap.get(old.shenanigans, castId)) {
                        case (?record) {
                            ?#spellCast({
                                castId;
                                caster = record.user;
                                shenaniganType = record.shenaniganType;
                                target = record.target;
                                outcome = record.outcome;
                            });
                        };
                        case (null) {
                            // No record for this castId — drop the chat item.
                            // Shenanigans are never pruned by the runtime, so
                            // hitting this branch indicates either pre-existing
                            // data corruption or a future cast path that side-
                            // stepped record insertion. Either way, the item is
                            // unrenderable, so drop rather than carry orphans.
                            null;
                        };
                    };
                };
                case (#userMessage(x)) { ?#userMessage(x) };
                case (#signup(x)) { ?#signup(x) };
                case (#rankUp(x)) { ?#rankUp(x) };
                case (#roundResult(x)) { ?#roundResult(x) };
                case (#reginald(x)) { ?#reginald(x) };
                case (#pinUpdate(x)) { ?#pinUpdate(x) };
            };
            switch (newKind) {
                case (?k) {
                    migrated.add({
                        id = item.id;
                        author = item.author;
                        timestamp = item.timestamp;
                        kind = k;
                        reactions = item.reactions;
                        deleted = item.deleted;
                    });
                };
                case (null) {};  // drop
            };
        };
        {
            var chatItems = Buffer.toArray(migrated);
            var shenanigans = old.shenanigans;
        };
    };

    // ================================================================
    // V7 — Add optional outcome-detail fields to cast records + chat items
    //
    // Adds four optional fields (ppDelta, affectedCount, renameDetail,
    // shieldDeflected) to ShenaniganRecord and to the #spellCast chat
    // item kind. Forward-only: every historical record gets null for
    // the new fields. New casts populate them at write time.
    //
    // Also extends the ShenaniganType variant set with #tenderOffer,
    // #stimulusCheck, #bearRaid. These don't appear in pre-V7
    // persisted data (the spells didn't exist), but listing them keeps
    // the type aligned with the post-migration shape.
    //
    // See docs/superpowers/plans/2026-05-27-shenanigans-feed-and-new-spells.md.
    // ================================================================

    type V7ShenaniganType = {
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
        #tenderOffer;
        #stimulusCheck;
        #bearRaid;
    };

    type V7ShenaniganOutcome = {
        #success;
        #fail;
        #backfire;
    };

    type V7OldShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : V7ShenaniganType;
        target : ?Principal;
        outcome : V7ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
    };

    type V7NewShenaniganRecord = {
        id : Nat;
        user : Principal;
        shenaniganType : V7ShenaniganType;
        target : ?Principal;
        outcome : V7ShenaniganOutcome;
        timestamp : Int;
        cost : Float;
        ppDelta : ?Int;
        affectedCount : ?Nat;
        renameDetail : ?{ oldName : Text; newName : Text };
        shieldDeflected : ?Bool;
    };

    type V7Reaction = {
        emoji : Text;
        reactors : [Principal];
        karmaPpBurned : Nat;
    };

    type V7OldChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : {
            castId : Nat;
            caster : Principal;
            shenaniganType : V7ShenaniganType;
            target : ?Principal;
            outcome : V7ShenaniganOutcome;
        };
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    type V7NewChatItemKind = {
        #userMessage : { body : Text; replyTo : ?Nat };
        #spellCast : {
            castId : Nat;
            caster : Principal;
            shenaniganType : V7ShenaniganType;
            target : ?Principal;
            outcome : V7ShenaniganOutcome;
            ppDelta : ?Int;
            affectedCount : ?Nat;
            renameDetail : ?{ oldName : Text; newName : Text };
            shieldDeflected : ?Bool;
        };
        #signup : { newUser : Principal };
        #rankUp : { user : Principal; newRank : Text };
        #roundResult : { gameId : Nat; winner : Principal; winnerPpUnits : Nat };
        #reginald : { line : Text; triggerKind : Text };
        #pinUpdate : { body : Text };
    };

    type V7OldChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V7OldChatItemKind;
        reactions : [V7Reaction];
        deleted : Bool;
    };

    type V7NewChatItem = {
        id : Nat;
        author : Principal;
        timestamp : Int;
        kind : V7NewChatItemKind;
        reactions : [V7Reaction];
        deleted : Bool;
    };

    type V7OldShenaniganMap = OrderedMap.Map<Nat, V7OldShenaniganRecord>;
    type V7NewShenaniganMap = OrderedMap.Map<Nat, V7NewShenaniganRecord>;

    public func runV7(
        old : {
            var chatItems : [V7OldChatItem];
            var shenanigans : V7OldShenaniganMap;
        }
    ) : {
        var chatItems : [V7NewChatItem];
        var shenanigans : V7NewShenaniganMap;
    } {
        let natMap = OrderedMap.Make<Nat>(Nat.compare);

        // ShenaniganRecord: backfill the four optional fields with null
        // on every historical record. New casts populate them at write.
        var newShenanigans : V7NewShenaniganMap = natMap.empty<V7NewShenaniganRecord>();
        for ((id, rec) in natMap.entries(old.shenanigans)) {
            let newRec : V7NewShenaniganRecord = {
                id = rec.id;
                user = rec.user;
                shenaniganType = rec.shenaniganType;
                target = rec.target;
                outcome = rec.outcome;
                timestamp = rec.timestamp;
                cost = rec.cost;
                ppDelta = null;
                affectedCount = null;
                renameDetail = null;
                shieldDeflected = null;
            };
            newShenanigans := natMap.put(newShenanigans, id, newRec);
        };

        // ChatItem: only #spellCast changes shape. Other variants pass
        // through unchanged.
        let migrated = Buffer.Buffer<V7NewChatItem>(old.chatItems.size());
        for (item in old.chatItems.vals()) {
            let newKind : V7NewChatItemKind = switch (item.kind) {
                case (#spellCast(sc)) {
                    #spellCast({
                        castId = sc.castId;
                        caster = sc.caster;
                        shenaniganType = sc.shenaniganType;
                        target = sc.target;
                        outcome = sc.outcome;
                        ppDelta = null;
                        affectedCount = null;
                        renameDetail = null;
                        shieldDeflected = null;
                    });
                };
                case (#userMessage(x)) { #userMessage(x) };
                case (#signup(x)) { #signup(x) };
                case (#rankUp(x)) { #rankUp(x) };
                case (#roundResult(x)) { #roundResult(x) };
                case (#reginald(x)) { #reginald(x) };
                case (#pinUpdate(x)) { #pinUpdate(x) };
            };
            migrated.add({
                id = item.id;
                author = item.author;
                timestamp = item.timestamp;
                kind = newKind;
                reactions = item.reactions;
                deleted = item.deleted;
            });
        };

        {
            var chatItems = Buffer.toArray(migrated);
            var shenanigans = newShenanigans;
        };
    };
};
