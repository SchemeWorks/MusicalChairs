// IMPL-49a / Codex R1: the shared signup-gift reservation, proved by EXHAUSTIVE
// interleaving of the legacy observer's gift attempt and native binds.
//
// Invariant (also in the NativeRewards.mo doc): for every principal, at most one
// signup gift ever reaches the ledger / #Committed, across both sources, across any
// interleaving of {legacy reserve, native bind, legacy finalize ok/err, legacy retry,
// legacy lost callback, native leg commit}, including failed-mint retries.
//
// Model. Legacy attempt k (the observer gift block):  reserve -> (await mint) -> finalize.
// Native event j:  bind (ONE atomic step: SIWS reply, re-read, reserve) -> leg commit later.
// "ledger gifts" counts gift mints that reached the PP ledger. The legacy mint is idempotent per
// eventId (mintWithEffects/mintedEventIds), so retries of the one legacy mint count once.
//
// Two orderings are run through the SAME driver:
//   #reserveBeforeAwait : the shipped protocol (NR.reserveGift before the await, NR.finalizeGift after).
//   #claimAfterAwait    : NEGATIVE CONTROL - the pre-IMPL-49a legacy ordering (check the claimed
//                         map, await the mint, write the map afterwards). It is not historical
//                         failing-test-first evidence; it proves the harness can see the race.
//
// Run: moc -r --package base <dfx-base> shenanigans/test/native-rewards-gift-interleave.test.mo

import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Array "mo:base/Array";
import Nat "mo:base/Nat";
import Int "mo:base/Int";
import Text "mo:base/Text";
import OrderedMap "mo:base/OrderedMap";
import NR "../NativeRewards";

// Flip to true to print the negative control as a hard FAIL (used once to record its output).
let NEGATIVE_CONTROL_AS_FAIL : Bool = false;

let SELF = Principal.fromText("j56tm-oaaaa-aaaac-qf34q-cai");
let alice = Principal.fromText("u7zgw-triai-opf3l-hs7dz-2bf2t-j554r-mnzkx-74hou-6drz5-nati3-bqe");
let bob = Principal.fromText("mi66c-zqlu4-4kxd6-2gtp7-szg5v-6a62a-geoty-fahu5-4trje-xyfby-wqe");
let carol = Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe");
let house = Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae");
let principalMap = OrderedMap.Make<Principal>(Principal.compare);

let NS : Nat = 1_000_000_000;
let T0 : Int = 1_800_000_000 * NS;
let PUBKEY_A = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

var ok = true;
var passed : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { passed += 1; Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); ok := false };
};

func eventId(sig : Text) : NR.EventId = {
  cluster_domain = Text.encodeUtf8("solana-localnet");
  program_id = "MCprog1111111111111111111111111111111111111";
  signature = sig;
  instruction_index = 0;
  inner_instruction_index = 0;
  event_schema_version = 1;
};

let noEffects : NR.EffectsSnapshot = {
  payoutTarget = func(p : Principal) : Principal { if (p == alice) { bob } else if (p == bob) { carol } else { house } };
  isActive = func(p : Principal) : Bool { p != carol };
  house = house;
  mintMultiplierBps = func(_ : Principal) : ?Nat { null };
  siphon = func(_ : Principal) : ?NR.SiphonView { null };
  cascadeBoostBps = func(_ : Principal) : ?Nat { null };
  cascadeDepthCap = 10;
};

type Ordering = { #reserveBeforeAwait; #claimAfterAwait };
type Step = { #LReserve : Nat; #LFinalize : Nat; #LLost : Nat; #NBind : Nat; #NCommit : Nat };
type Outs = { ok1 : Bool; ok2 : Bool; lostReached : Bool };

type World = {
  chain : NR.State;
  book : NR.GiftBook;
  claimed : NR.SignupMarkers;
  ledgerGifts : Nat; // gift mints that reached the PP ledger
  peak : Nat; // max ledgerGifts seen at any prefix
  inflight : [Bool]; // legacy attempt granted and not yet finalized
  legacyMinted : Bool; // the (idempotent) legacy gift mint reached the ledger
  giftLeg : [?Nat]; // per native event: index of its gift leg, if its bind took the gift
};

func initWorld() : World {
  let s0 = switch (NR.appendPolicy(NR.emptyState(), {
    simple21DayPpPerSol = 6_000; compounding15DayPpPerSol = 12_000; compounding30DayPpPerSol = 18_000;
    backerPpPerSol = 120_000; signupGiftPp = 500; cascadeInitialBps = 1_000; cascadePassthroughBps = 5_000;
    applyEffects = true; dedupWindowNs = 86_400 * NS;
  }, T0)) { case (#Ok r) { r.state }; case (#Err e) { Debug.trap(e) } };
  var s = s0;
  for (sig in ["ev1", "ev2"].vals()) {
    let ev : NR.NativeEvent = {
      event_id = eventId(sig); source_round_id = 1; source_close_id = null; kind = #PositionOpened;
      beneficiary_pubkey = PUBKEY_A; basis_lamports = 1_000_000_000; plan = ?#Simple;
    };
    let (s2, _) = NR.observe(s, ev, T0);
    s := s2;
  };
  {
    chain = s;
    book = principalMap.empty<NR.GiftReservation>();
    claimed = principalMap.empty<Int>();
    ledgerGifts = 0;
    peak = 0;
    inflight = [false, false];
    legacyMinted = false;
    giftLeg = [null, null];
  };
};

let textMap = OrderedMap.Make<Text>(Text.compare);
var bindCache : [(Text, NR.BindOutput)] = [];
func cacheGet(k : Text) : ?NR.BindOutput {
  for ((kk, v) in bindCache.vals()) { if (kk == k) { return ?v } };
  null;
};
func cachePut(k : Text, v : NR.BindOutput) { bindCache := Array.append<(Text, NR.BindOutput)>(bindCache, [(k, v)]) };
func optStatus(o : ?NR.NativeRewardObligation) : ?NR.ObligationStatus = switch (o) { case (?x) { ?x.status }; case null { null } };

func setAt<T>(xs : [T], i : Nat, v : T) : [T] = Array.tabulate<T>(xs.size(), func(j) { if (j == i) { v } else { xs[j] } });

func withGift(w : World, gifts : Nat) : World = { w with ledgerGifts = gifts; peak = if (gifts > w.peak) { gifts } else { w.peak } };

// The idempotent legacy mint (same eventId) reaching the ledger.
func legacyMint(w : World) : World {
  if (w.legacyMinted) { w } else { withGift({ w with legacyMinted = true }, w.ledgerGifts + 1) };
};

func apply(ord : Ordering, outs : Outs, w : World, st : Step, now : Int) : World {
  switch (st) {
    case (#LReserve(k)) {
      switch (ord) {
        case (#reserveBeforeAwait) {
          let (b, r) = NR.reserveGift(w.book, w.claimed, alice, #legacy, now);
          switch (r) {
            case (#Granted) { { w with book = b; inflight = setAt<Bool>(w.inflight, k, true) } };
            case (#Denied(_)) { w };
          };
        };
        case (#claimAfterAwait) {
          // pre-IMPL-49a: only the claimed map is consulted, and it is written after the await
          if (principalMap.contains(w.claimed, alice)) { w } else { { w with inflight = setAt<Bool>(w.inflight, k, true) } };
        };
      };
    };
    case (#LFinalize(k)) {
      if (not w.inflight[k]) { return w };
      let out = if (k == 0) { outs.ok1 } else { outs.ok2 };
      let w1 = { w with inflight = setAt<Bool>(w.inflight, k, false) };
      switch (ord) {
        case (#reserveBeforeAwait) {
          if (out) {
            let w2 = legacyMint(w1);
            let (b, c) = NR.finalizeGift(w2.book, w2.claimed, alice, #legacy, #Ok, now);
            { w2 with book = b; claimed = c };
          } else {
            let (b, c) = NR.finalizeGift(w1.book, w1.claimed, alice, #legacy, #Err, now);
            { w1 with book = b; claimed = c };
          };
        };
        case (#claimAfterAwait) {
          if (out) {
            let w2 = legacyMint(w1);
            { w2 with claimed = principalMap.put(w2.claimed, alice, now) };
          } else { w1 };
        };
      };
    };
    case (#LLost(k)) {
      // the legacy callback never ran (trap / interruption), so the reservation stays #Reserved(legacy).
      // The mint may or may not have reached the ledger (see outsInterrupted for the dedup assumption)
      if (not w.inflight[k]) { return w };
      let w1 = { w with inflight = setAt<Bool>(w.inflight, k, false) };
      if (outs.lostReached) { legacyMint(w1) } else { w1 };
    };
    case (#NBind(j)) {
      let id = eventId(if (j == 0) { "ev1" } else { "ev2" });
      // Only the obligation's own state, alice's reservation and her claim can influence the
      // outcome of a bind, so the (expensive: SHA-256 memos) real NR.applyBind result is memoized
      // on exactly that key. The cached value IS a real applyBind output for an identical input.
      let key = Nat.toText(j) # "|" # debug_show (optStatus(NR.getObligation(w.chain, id))) # "|" # debug_show (principalMap.get(w.book, alice)) # "|" # debug_show (principalMap.contains(w.claimed, alice));
      let out : NR.BindOutput = switch (cacheGet(key)) {
        case (?o) { o };
        case null {
          let env : NR.BindEnv = { self = SELF; signupMarkers = w.claimed; giftBook = w.book; effects = noEffects };
          let o = NR.applyBind(w.chain, env, id, ?alice, now);
          cachePut(key, o);
          o;
        };
      };
      var leg : ?Nat = null;
      var chain = w.chain;
      switch (NR.getObligation(out.state, id)) {
        case (?o) {
          for (l in o.legs.vals()) { if (l.role == #signupGift) { leg := ?l.index } };
          chain := { chain with obligations = textMap.put(chain.obligations, NR.eventKey(id), o) };
        };
        case null {};
      };
      { w with chain = chain; book = out.giftBook; giftLeg = setAt<?Nat>(w.giftLeg, j, leg) };
    };
    case (#NCommit(j)) {
      switch (w.giftLeg[j]) {
        case null { w };
        case (?idx) {
          let id = eventId(if (j == 0) { "ev1" } else { "ev2" });
          let (s1, br) = NR.beginExecute(w.chain, id, idx, now);
          switch (br) {
            case (#Submit(_)) {
              let (s2, res) = NR.applyTransferOutcome(s1, id, idx, #Ok(1_000 + j), now);
              var w2 = { w with chain = s2 };
              if (res.newlyCommitted) {
                w2 := withGift(w2, w2.ledgerGifts + 1);
                switch (NR.getObligation(s2, id)) {
                  case (?o) {
                    let (b, c) = NR.onLegCommitted(w2.book, w2.claimed, o.legs[idx], now);
                    w2 := { w2 with book = b; claimed = c };
                  };
                  case null {};
                };
              };
              w2;
            };
            case _ { { w with chain = s1 } };
          };
        };
      };
    };
  };
};

func stepName(st : Step, outs : Outs) : Text {
  switch (st) {
    case (#LReserve(k)) { "legacy-reserve#" # Nat.toText(k + 1) };
    case (#LFinalize(k)) { "legacy-finalize#" # Nat.toText(k + 1) # (if (if (k == 0) { outs.ok1 } else { outs.ok2 }) { "(ok)" } else { "(err)" }) };
    case (#LLost(k)) { "legacy-lost#" # Nat.toText(k + 1) # (if (outs.lostReached) { "(mint reached ledger)" } else { "(mint did not reach ledger)" }) };
    case (#NBind(j)) { "native-bind#" # Nat.toText(j + 1) };
    case (#NCommit(j)) { "native-commit#" # Nat.toText(j + 1) };
  };
};

// Depth-first enumeration of every interleaving that preserves per-process order.
func enumerate(ord : Ordering, outs : Outs, procs : [[Step]], onDone : (World, [Text]) -> ()) : Nat {
  let n = procs.size();
  var count = 0;
  func go(w : World, pos : [Nat], trace : [Text], t : Nat) {
    var any = false;
    var i = 0;
    while (i < n) {
      let pi = i;
      if (pos[pi] < procs[pi].size()) {
        any := true;
        let st = procs[pi][pos[pi]];
        let w2 = apply(ord, outs, w, st, T0 + t * NS);
        go(w2, setAt<Nat>(pos, pi, pos[pi] + 1), Array.append<Text>(trace, [stepName(st, outs)]), t + 1);
      };
      i += 1;
    };
    if (not any) { count += 1; onDone(w, trace) };
  };
  go(initWorld(), Array.tabulate<Nat>(n, func(_) { 0 }), [], 0);
  count;
};

func join(xs : [Text]) : Text = Text.join(" > ", xs.vals());

let LA : [Step] = [#LReserve(0), #LFinalize(0), #LReserve(1), #LFinalize(1)]; // attempt 1, then the retry
let L1 : [Step] = [#LReserve(0), #LFinalize(0)];
let LI : [Step] = [#LReserve(0), #LLost(0), #LReserve(1), #LFinalize(1)]; // interrupted attempt, then retry
let N1 : [Step] = [#NBind(0), #NCommit(0)];
let N2 : [Step] = [#NBind(1), #NCommit(1)];

func outsAll() : [Outs] = [
  { ok1 = true; ok2 = true; lostReached = false },
  { ok1 = true; ok2 = false; lostReached = false },
  { ok1 = false; ok2 = true; lostReached = false },
  { ok1 = false; ok2 = false; lostReached = false },
];
func outsOk1() : [Outs] = [{ ok1 = true; ok2 = true; lostReached = false }, { ok1 = false; ok2 = true; lostReached = false }];
// Interrupted attempt 1 (callback never ran), then the retry. If the lost attempt's mint DID reach the
// ledger, mintWithEffects had already recorded its eventId, so the retry is deduplicated and returns
// #Ok(0): that retry cannot fail (hence no {lostReached, ok2 = false} combination). If it did not reach
// the ledger, the retry may succeed or fail. (A trap in the sub-instruction window between the ledger
// commit and the mintedEventIds write is a PRE-EXISTING residual of mintWithEffects that this change
// neither introduces nor closes; see the NativeRewards.mo doc.)
func outsInterrupted() : [Outs] = [
  { ok1 = true; ok2 = true; lostReached = true },
  { ok1 = true; ok2 = true; lostReached = false },
  { ok1 = true; ok2 = false; lostReached = false },
];
func outsNone() : [Outs] = [{ ok1 = true; ok2 = true; lostReached = false }];

let scenarios : [(Text, [[Step]], [Outs], Nat)] = [
  ("legacy attempt + retry vs 1 native event", [LA, N1], outsAll(), 60),
  ("legacy attempt + retry vs 2 native events", [LA, N1, N2], outsAll(), 1_680),
  ("legacy single attempt vs 2 native events", [L1, N1, N2], outsOk1(), 180),
  ("2 native events, no legacy", [N1, N2], outsNone(), 6),
  ("legacy attempt + retry alone", [LA], outsAll(), 4),
  ("interrupted legacy attempt + retry vs 2 native events", [LI, N1, N2], outsInterrupted(), 1_260),
];

var totalInterleavings = 0;
var totalRuns = 0;
// negative control accumulators
var ncViolations = 0;
var ncFirst : ?Text = null;
var ncTotal = 0;

Debug.print("== sharedSignupGiftAtMostOneCommittedAcrossEveryInterleaving");
for ((name, procs, outsList, expected) in scenarios.vals()) {
  var scenarioCount = 0;
  var bad : Nat = 0;
  var firstBad : ?Text = null;
  for (outs in outsList.vals()) {
    let c = enumerate(#reserveBeforeAwait, outs, procs, func(w, trace) {
      totalRuns += 1;
      var why = "";
      if (w.peak > 1) { why := "two gifts reached the ledger at some prefix" };
      let committed = switch (principalMap.get(w.book, alice)) { case (?r) { r.state == #Committed }; case null { false } };
      let claimedHas = principalMap.contains(w.claimed, alice);
      if (committed != (w.ledgerGifts == 1)) { why := "book Committed != (ledger gifts == 1)" };
      if (claimedHas != (w.ledgerGifts == 1)) { why := "claimed map != (ledger gifts == 1)" };
      if (w.ledgerGifts == 0) {
        // never lost: nobody is in flight at the end, and a fresh native claim is still granted
        let (_, probe) = NR.reserveGift(w.book, w.claimed, alice, #native, T0 + 99 * NS);
        if (probe != #Granted) { why := "zero gifts but the gift can no longer be claimed" };
      };
      if (why != "") {
        bad += 1;
        if (firstBad == null) { firstBad := ?(why # " :: " # join(trace)) };
      };
    });
    scenarioCount += c;
  };
  totalInterleavings += scenarioCount;
  check(scenarioCount == expected, name # ": enumerated " # Nat.toText(scenarioCount) # " interleavings (expected " # Nat.toText(expected) # ")");
  check(bad == 0, name # ": every interleaving satisfies the invariant" # (switch (firstBad) { case (?t) { " - FIRST VIOLATION " # t }; case null { "" } }));
};
check(totalInterleavings == 3_190, "total interleavings enumerated = " # Nat.toText(totalInterleavings));

// ---------------------------------------------------------------- NEGATIVE CONTROL
// The SAME driver and scenarios, with the legacy side in its pre-IMPL-49a ordering
// (claim after the await). The invariant is expected to BREAK: two gifts reach the ledger.
Debug.print("== negativeControlOldOrderingClaimAfterAwaitDoubleGifts (NEGATIVE CONTROL, not a historical RED)");
for ((name, procs, outsList, _) in scenarios.vals()) {
  for (outs in outsList.vals()) {
    let c = enumerate(#claimAfterAwait, outs, procs, func(w, trace) {
      ncTotal += 1;
      if (w.peak > 1) {
        ncViolations += 1;
        if (ncFirst == null) { ncFirst := ?(name # " :: " # join(trace) # " => ledger gifts = " # Nat.toText(w.peak)) };
      };
    });
    ignore c;
  };
};
let firstText = switch (ncFirst) { case (?t) { t }; case null { "(none)" } };
Debug.print("NEGATIVE CONTROL: " # Nat.toText(ncViolations) # " of " # Nat.toText(ncTotal) # " old-ordering interleavings put TWO gifts on the ledger; first: " # firstText);
if (NEGATIVE_CONTROL_AS_FAIL) {
  check(ncViolations == 0, "old ordering (claim after await) satisfies at-most-one-gift");
} else {
  check(ncViolations > 0, "negative control: the harness detects the old claim-after-await double gift (" # Nat.toText(ncViolations) # " violating interleavings)");
  check(ncTotal == totalInterleavings, "negative control ran the same " # Nat.toText(ncTotal) # " interleavings");
};

if (ok) { Debug.print("ALL PASS (" # Nat.toText(passed) # " checks)") } else { Debug.trap("TEST FAILURES") };
