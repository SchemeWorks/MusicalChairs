// LOCAL-REPLICA TEST FIXTURE ONLY (PP-ledger memo rehearsal). Stands in for ponzi_math as the
// Shenanigans observer's game + backer source: it returns whatever the driver sets, so the real
// observer runs its real signup-gift and backer-delta mint paths. No financial logic.
//
// Shapes mirror `PonziMathActor` / `PonziMathGameRecord` / `PonziMathBackerPosition` in
// shenanigans/main.mo (structural Candid subtyping: extra fields are ignored, missing ones trap).
persistent actor MemoGamesStub {
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
  var backers : [Backer] = [];

  public shared func setGames(g : [Game]) : async () { games := g };
  public shared func setBackers(b : [Backer]) : async () { backers := b };
  public query func getAllGames() : async [Game] { games };
  public query func getBackerPositions() : async [Backer] { backers };
  public query func getCurrentRoundId() : async Nat { 1 };
};
