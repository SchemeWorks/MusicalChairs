import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";

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
    // Extends MintConfig with 5 admin-tunable fields:
    //   cascadeInitialBps       (10% deduction off the top)
    //   cascadePassthroughBps   (50% kept by each active upline)
    //   signupGiftPp            (500 PP signup gift; 0 disables)
    //   activityRequiresDeposit (cascade skips inactive uplines)
    //   activityWindowDays      (null = lifetime; ?n = last n days)
    //
    // Old MintConfig fields are preserved verbatim. Old referralL[1-3]Bps
    // remain on the record (deprecated, unused by the new cascade) so the
    // candid signature stays stable for admin tooling that reads them.
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

    public func runV3(old : { var mintConfig : V3OldMintConfig }) : { var mintConfig : V3NewMintConfig } {
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
};
