import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";
import Nat "mo:base/Nat";
import Int "mo:base/Int";
import Iter "mo:base/Iter";

module {

    // ----------------------------------------------------------------------
    // Migration: add per-event `roundId` to GeneralLedgerEntry, and bring up
    // a top-level `currentRoundId` counter.
    //
    // Old entries are backfilled by walking gameResetHistory in ascending
    // time order: an event at timestamp T belongs to round
    //     1 + #{ resetTime : resetTime < T }
    // i.e. the gameReset event that ENDS round N is itself recorded under
    // round N (the round being closed).
    //
    // Field semantics per Motoko inline migration:
    //   - `generalLedger`        : transformed (entry shape changes)
    //   - `gameResetHistory`     : passed through (identity) — read to compute
    //                              roundIds, then preserved in the new state
    //   - `currentRoundId`       : produced from scratch (output-only)
    //   - Everything else        : carried through by the runtime (not named
    //                              in either record, so untouched)
    // ----------------------------------------------------------------------

    // GeneralLedgerEvent — structurally identical to the live actor's variant.
    // Inlined here per Motoko migration guidance (don't import from main.mo;
    // future renames to those types would silently break this migration).
    type GamePlan = {
        #simple21Day;
        #compounding15Day;
        #compounding30Day;
    };

    type GeneralLedgerEvent = {
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

    type OldGeneralLedgerEntry = {
        id : Nat;
        timestamp : Int;
        event : GeneralLedgerEvent;
    };

    type NewGeneralLedgerEntry = {
        id : Nat;
        timestamp : Int;
        roundId : Nat;
        event : GeneralLedgerEvent;
    };

    type GameResetRecord = {
        resetTime : Int;
        reason : Text;
    };

    public func run(old : {
        var generalLedger : OrderedMap.Map<Nat, OldGeneralLedgerEntry>;
        var gameResetHistory : OrderedMap.Map<Int, GameResetRecord>;
    }) : {
        var generalLedger : OrderedMap.Map<Nat, NewGeneralLedgerEntry>;
        var gameResetHistory : OrderedMap.Map<Int, GameResetRecord>;
        var currentRoundId : Nat;
    } {
        let natOps = OrderedMap.Make<Nat>(Nat.compare);
        let intOps = OrderedMap.Make<Int>(Int.compare);

        // OrderedMap.keys() iterates in key order, so resetTimes is sorted ascending.
        let resetTimes = Iter.toArray(intOps.keys(old.gameResetHistory));

        // Round 1 is the initial round (before any reset). Round (K+1) begins
        // after the K-th reset fires. A gameReset event at exactly time T
        // counts as ending round R = 1 + #{rt : rt < T}, so it carries roundId R.
        func roundIdFor(t : Int) : Nat {
            var rounds : Nat = 1;
            for (rt in resetTimes.vals()) {
                if (rt < t) { rounds += 1 };
            };
            rounds;
        };

        var newLedger = natOps.empty<NewGeneralLedgerEntry>();
        for ((id, entry) in natOps.entries(old.generalLedger)) {
            let migrated : NewGeneralLedgerEntry = {
                id = entry.id;
                timestamp = entry.timestamp;
                roundId = roundIdFor(entry.timestamp);
                event = entry.event;
            };
            newLedger := natOps.put(newLedger, id, migrated);
        };

        // currentRoundId = total resets + 1 (we're already in the round AFTER
        // the most recent reset).
        let nextRound : Nat = resetTimes.size() + 1;

        {
            var generalLedger = newLedger;
            var gameResetHistory = old.gameResetHistory;
            var currentRoundId = nextRound;
        };
    };
};
