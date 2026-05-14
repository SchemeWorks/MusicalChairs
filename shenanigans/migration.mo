import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";

module {

    // ================================================================
    // Old types — match the deployed canister's stable signature
    // ================================================================

    type OldMintConfig = {
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

    type NewMintConfig = {
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

    // ================================================================
    // run — pure renames inherited from the Dealer→Backer refactor.
    // No values change; the migration only relabels fields so the
    // deployed stable signature lines up with the new actor:
    //
    //   mintConfig.dealerPpPerIcp -> mintConfig.backerPpPerIcp
    //   backendPrincipal          -> ponziMathPrincipal
    //   dealerSeen                -> backerSeen
    // ================================================================

    public func run(old : {
        var mintConfig : OldMintConfig;
        var backendPrincipal : ?Principal;
        var dealerSeen : SeenMap;
    }) : {
        var mintConfig : NewMintConfig;
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
};
