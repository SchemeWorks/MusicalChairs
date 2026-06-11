// Standalone test for Promotion.underwaterLosers — the round-net aggregation
// that decides Series B promotion eligibility.
//
// Run: moc -r --package base <dfx-base> ponzi_math/test/promotion.test.mo
// (see scripts; build verification harness drives this)
//
// Scenario mirrors the real bug: "Ass" had three positions in one round; a
// 15-day compounding plan matured and paid out ~4 ICP on a 1 ICP stake, so
// across all three positions Ass was NET POSITIVE and must NOT be promoted.
// "Cat of Wisdom" did a 30-day deposit, never redeemed it, and lost the whole
// stake — a genuine net loser who SHOULD be eligible.

import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Array "mo:base/Array";
import P "../Promotion";

// Distinct, valid principals for the players.
let ass = Principal.fromText("2vxsx-fae"); // anonymous text — just a distinct id here
let cat = Principal.fromText("aaaaa-aa");
let dog = Principal.fromText("u7zgw-triai-opf3l-hs7dz-2bf2t-j554r-mnzkx-74hou-6drz5-nati3-bqe");
let eel = Principal.fromText("mi66c-zqlu4-4kxd6-2gtp7-szg5v-6a62a-geoty-fahu5-4trje-xyfby-wqe");

let roundStart : Int = 1000;

let games : [P.GameNet] = [
  // Ass — net winner across the round (-3 + 1 + 1 = -1):
  { player = ass; startTime = 1100; amount = 1.0; totalWithdrawn = 4.0 }, // matured 15d, claimed
  { player = ass; startTime = 1200; amount = 1.0; totalWithdrawn = 0.0 }, // open loser
  { player = ass; startTime = 1300; amount = 1.0; totalWithdrawn = 0.0 }, // open loser
  // Cat — net loser, lost full 30d stake (+2):
  { player = cat; startTime = 1150; amount = 2.0; totalWithdrawn = 0.0 },
  // Dog — a PRIOR-round loser (startTime < roundStart). Must be excluded by
  // round scoping even though net is +5.
  { player = dog; startTime = 500; amount = 5.0; totalWithdrawn = 0.0 },
  // Eel — break-even this round (net 0). Not a loser.
  { player = eel; startTime = 1400; amount = 1.0; totalWithdrawn = 1.0 },
];

let losers = P.underwaterLosers(games, roundStart);

func has(p : Principal) : Bool {
  for ((q, _) in losers.vals()) { if (q == p) { return true } };
  false;
};

func lossOf(p : Principal) : Float {
  for ((q, v) in losers.vals()) { if (q == p) { return v } };
  -1.0;
};

var ok = true;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); ok := false };
};

check(losers.size() == 1, "exactly one net loser (got " # debug_show (Array.size(losers)) # ")");
check(has(cat), "Cat of Wisdom is a net loser (eligible)");
check(lossOf(cat) == 2.0, "Cat's net loss is exactly 2.0 (got " # debug_show (lossOf(cat)) # ")");
check(not has(ass), "Ass is NET POSITIVE across the round -> NOT eligible");
check(not has(dog), "prior-round straggler excluded by round scoping");
check(not has(eel), "break-even player is not a loser");

if (ok) { Debug.print("ALL PASS") } else { Debug.trap("TEST FAILURES") };
