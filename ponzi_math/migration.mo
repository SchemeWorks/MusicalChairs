import OrderedMap "mo:base/OrderedMap";
import Principal "mo:base/Principal";

module {

    type BackerType = { #seriesA; #seriesB };

    type BackerPosition = {
        owner : Principal;
        amount : Float;
        entitlement : Float;
        startTime : Int;
        isActive : Bool;
        backerType : BackerType;
        firstDepositDate : ?Int;
    };

    type BackerKey = (Principal, BackerType);

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

    // Re-key both backer maps from Principal to (Principal, BackerType).
    // Each old backerPositions entry carries its own backerType field.
    // Each old backerRepayments entry is re-keyed using the matching
    // position's type; orphaned entries (repayment with no position) are
    // dropped — they would never have been paid out anyway.
    public func run(old : {
        var backerPositions : OrderedMap.Map<Principal, BackerPosition>;
        var backerRepayments : OrderedMap.Map<Principal, Float>;
    }) : {
        var backerPositions : OrderedMap.Map<BackerKey, BackerPosition>;
        var backerRepayments : OrderedMap.Map<BackerKey, Float>;
    } {
        let oldOps = OrderedMap.Make<Principal>(Principal.compare);
        let newOps = OrderedMap.Make<BackerKey>(backerKeyCompare);

        var newPositions = newOps.empty<BackerPosition>();
        for ((p, pos) in oldOps.entries(old.backerPositions)) {
            newPositions := newOps.put(newPositions, (p, pos.backerType), pos);
        };

        var newRepayments = newOps.empty<Float>();
        for ((p, r) in oldOps.entries(old.backerRepayments)) {
            switch (oldOps.get(old.backerPositions, p)) {
                case (?pos) {
                    newRepayments := newOps.put(newRepayments, (p, pos.backerType), r);
                };
                case (null) { /* orphan — drop */ };
            };
        };

        {
            var backerPositions = newPositions;
            var backerRepayments = newRepayments;
        };
    };
};
