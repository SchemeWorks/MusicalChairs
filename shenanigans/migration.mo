import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Nat "mo:base/Nat";

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
};
