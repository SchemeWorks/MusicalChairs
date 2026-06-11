import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Float "mo:base/Float";
import List "mo:base/List";

// Series B promotion eligibility math.
//
// A round can leave a single player holding several positions. They count as a
// round LOSER only on their AGGREGATE net across every position opened this
// round — so a matured, cashed-out winning plan offsets losses on their other
// plans. This is the structural width-subset of GameRecord that the math needs.
module {
    public type GameNet = {
        player : Principal;
        startTime : Int;
        amount : Float;
        totalWithdrawn : Float;
    };

    func roundToEightDecimals(value : Float) : Float {
        let multiplier = 100000000.0;
        Float.fromInt(Float.toInt(value * multiplier)) / multiplier;
    };

    // Aggregate each player's net position (deposits - withdrawals) across every
    // game of the CURRENT round, then keep only the net losers.
    //
    // Round scoping: a game belongs to the current round iff it was opened after
    // the last reset (startTime >= roundStart). Reset force-closes every active
    // game, so a game's whole life sits in one round — current-round games are
    // exactly those with startTime >= roundStart, whether still active or already
    // closed this round (matured claims, partial withdrawals). Pass roundStart =
    // the latest gameReset.resetTime, or 0 for the first round.
    //
    // Unlike the old logic this iterates ALL games (not just active) and sums net
    // UNCONDITIONALLY (no per-game positive-loss filter), so a player's winnings
    // genuinely cancel their losses. netLoss is the rounded aggregate loss and is
    // strictly > 0 for every entry; break-even players are not losers.
    public func underwaterLosers(games : [GameNet], roundStart : Int) : [(Principal, Float)] {
        let pmap = OrderedMap.Make<Principal>(Principal.compare);
        var netByPlayer = pmap.empty<Float>();
        for (g in games.vals()) {
            if (g.startTime >= roundStart) {
                let prev = switch (pmap.get(netByPlayer, g.player)) {
                    case (null) { 0.0 };
                    case (?v) { v };
                };
                netByPlayer := pmap.put(netByPlayer, g.player, prev + (g.amount - g.totalWithdrawn));
            };
        };

        var acc = List.nil<(Principal, Float)>();
        for ((p, net) in pmap.entries(netByPlayer)) {
            let rounded = roundToEightDecimals(net);
            if (rounded > 0.0) { acc := List.push((p, rounded), acc) };
        };
        List.toArray(acc);
    };
};
