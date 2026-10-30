// IMPL-49h test fixture (LOCAL REPLICA ONLY). Stands in for ponzi_math as the legacy observer's
// game source: it returns the games the driver sets, so the legacy observer has one qualifying
// game for a chosen principal and runs its real signup-gift block. No financial logic.
//
// The shapes mirror `PonziMathActor` / `PonziMathGameRecord` / `PonziMathBackerPosition` in
// shenanigans/main.mo (structural Candid subtyping; extra fields would be ignored, missing ones trap).
persistent actor LegacyGamesStub {
  type Game = {
    id : Nat;
    player : Principal;
    plan : { #simple21Day; #compounding15Day; #compounding30Day };
    amount : Float;
    startTime : Int;
    isCompounding : Bool;
    isActive : Bool;
    lastUpdateTime : Int;
    accumulatedEarnings : Float;
    totalWithdrawn : Float;
  };

  type Backer = {
    owner : Principal;
    amount : Float;
    entitlement : Float;
    startTime : Int;
    isActive : Bool;
    backerType : { #seriesA; #seriesB };
    firstDepositDate : ?Int;
  };

  var games : [Game] = [];

  public shared func setGames(g : [Game]) : async () { games := g };
  public query func getAllGames() : async [Game] { games };
  public query func getBackerPositions() : async [Backer] { [] };
  public query func getCurrentRoundId() : async Nat { 1 };
};
