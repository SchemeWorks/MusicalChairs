// Standalone tests for NativeRewards observe / bind-and-reserve (spec section 9,
// plan M12 Shenanigans list). Pure module: time, policy, SIWS lookup results,
// effect snapshots and the shared signup marker are all injected.
//
// Run (from the repo root, see the moc rig in the M12 handoff):
//   moc -r --package base <dfx-base> shenanigans/test/native-rewards-observe-bind.test.mo

import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Array "mo:base/Array";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Int "mo:base/Int";
import Text "mo:base/Text";
import OrderedMap "mo:base/OrderedMap";
import NR "../NativeRewards";
import Subaccount "../Subaccount";

// ---------------------------------------------------------------- fixtures

let SELF = Principal.fromText("j56tm-oaaaa-aaaac-qf34q-cai");
let alice = Principal.fromText("u7zgw-triai-opf3l-hs7dz-2bf2t-j554r-mnzkx-74hou-6drz5-nati3-bqe");
let bob = Principal.fromText("mi66c-zqlu4-4kxd6-2gtp7-szg5v-6a62a-geoty-fahu5-4trje-xyfby-wqe");
let carol = Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe");
let dave = Principal.fromText("stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe");
let house = Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae");
let bridge = Principal.fromText("tcm26-yqaaa-aaaac-qg2lq-cai");

let principalMap = OrderedMap.Make<Principal>(Principal.compare);

let NS : Nat = 1_000_000_000;
let T0 : Int = 1_800_000_000 * NS;
let LAMPORTS : Nat64 = 1_000_000_000;
let PP : Nat = 100_000_000; // units per whole PP

let PUBKEY_A = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
let PUBKEY_B = "4Nd1mBQtrMJVYVfKGEfnrXz1pZzU3x9Jhc1sYQhQ6Wq7";

var ok = true;
var passed : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { passed += 1; Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); ok := false };
};

func draft() : NR.PolicyDraft = {
  simple21DayPpPerSol = 6_000;
  compounding15DayPpPerSol = 12_000;
  compounding30DayPpPerSol = 18_000;
  backerPpPerSol = 120_000;
  signupGiftPp = 500;
  cascadeInitialBps = 1_000;
  cascadePassthroughBps = 5_000;
  applyEffects = true;
  dedupWindowNs = 86_400 * NS;
};

func withPolicy(s : NR.State, d : NR.PolicyDraft, now : Int) : NR.State {
  switch (NR.appendPolicy(s, d, now)) { case (#Ok r) { r.state }; case (#Err e) { Debug.trap("appendPolicy: " # e) } };
};

func freshState() : NR.State = withPolicy(NR.emptyState(), draft(), T0);

func eventId(sig : Text) : NR.EventId = {
  cluster_domain = Text.encodeUtf8("solana-localnet");
  program_id = "MCprog1111111111111111111111111111111111111";
  signature = sig;
  instruction_index = 0;
  inner_instruction_index = 0;
  event_schema_version = 1;
};

func posEvent(sig : Text, lamports : Nat64, plan : NR.Plan, pk : Text) : NR.NativeEvent = {
  event_id = eventId(sig);
  source_round_id = 3;
  source_close_id = null;
  kind = #PositionOpened;
  beneficiary_pubkey = pk;
  basis_lamports = lamports;
  plan = ?plan;
};

func seriesAEvent(sig : Text, lamports : Nat64, pk : Text) : NR.NativeEvent = {
  event_id = eventId(sig);
  source_round_id = 3;
  source_close_id = null;
  kind = #SeriesAFunded;
  beneficiary_pubkey = pk;
  basis_lamports = lamports;
  plan = null;
};

func promoEvent(sig : Text, loss : Nat64, pk : Text) : NR.NativeEvent = {
  event_id = eventId(sig);
  source_round_id = 3;
  source_close_id = ?7;
  kind = #PromotionApplied;
  beneficiary_pubkey = pk;
  basis_lamports = loss;
  plan = null;
};

// alice -> bob (active) -> carol (inactive) -> house (active, catch-all)
let noEffects : NR.EffectsSnapshot = {
  payoutTarget = func(p : Principal) : Principal {
    if (p == alice) { bob } else if (p == bob) { carol } else { house };
  };
  isActive = func(p : Principal) : Bool { p != carol };
  house = house;
  mintMultiplierBps = func(_ : Principal) : ?Nat { null };
  siphon = func(_ : Principal) : ?NR.SiphonView { null };
  cascadeBoostBps = func(_ : Principal) : ?Nat { null };
  cascadeDepthCap = 10;
};

let noMarkers : NR.SignupMarkers = principalMap.empty<Int>();

func bindEnv(markers : NR.SignupMarkers, fx : NR.EffectsSnapshot) : NR.BindEnv = {
  self = SELF;
  signupMarkers = markers;
  effects = fx;
};

func observeOk(s : NR.State, ev : NR.NativeEvent, now : Int) : NR.State {
  let (s2, r) = NR.observe(s, ev, now);
  switch (r) { case (#Observed _) { s2 }; case _ { Debug.trap("observe not #Observed for " # ev.event_id.signature) } };
};

func amounts(legs : [NR.Leg]) : [Nat] = Array.map<NR.Leg, Nat>(legs, func l = l.amount);

func sum(xs : [Nat]) : Nat {
  var t = 0;
  for (x in xs.vals()) { t += x };
  t;
};

func oblOf(s : NR.State, id : NR.EventId) : NR.NativeRewardObligation {
  switch (NR.getObligation(s, id)) { case (?o) { o }; case null { Debug.trap("no obligation") } };
};

func obsOf(s : NR.State, id : NR.EventId) : NR.NativeObservation {
  switch (NR.getObservation(s, id)) { case (?o) { o }; case null { Debug.trap("no observation") } };
};

func eqArr(a : [Nat], b : [Nat]) : Bool = Array.equal<Nat>(a, b, Nat.equal);

func roleCount(legs : [NR.Leg], f : NR.LegRole -> Bool) : Nat {
  var n = 0;
  for (l in legs.vals()) { if (f(l.role)) { n += 1 } };
  n;
};

func isGift(r : NR.LegRole) : Bool = switch (r) { case (#signupGift) { true }; case _ { false } };

// ---------------------------------------------------------------- tests

Debug.print("== observeIsIdempotentAndPinsPolicyVersionAtFirstObservation");
do {
  let s1 = freshState(); // policy v1
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let (s2, r1) = NR.observe(s1, e1, T0 + 10 * NS);
  check(r1 == #Observed(NR.eventKey(e1.event_id)), "first observation returns #Observed(obligation id = canonical event key)");
  let o1 = obsOf(s2, e1.event_id);
  check(o1.policy_version == 1 and o1.observed_at == T0 + 10 * NS, "first observation pins policy v1 at observed_at");
  check(oblOf(s2, e1.event_id).status == #Observed and oblOf(s2, e1.event_id).legs.size() == 0, "observation has status #Observed and no legs (no effects consumed)");

  let d2 : NR.PolicyDraft = { draft() with simple21DayPpPerSol = 1; signupGiftPp = 0 };
  let s3 = withPolicy(s2, d2, T0 + 20 * NS);
  let (s4, r2) = NR.observe(s3, e1, T0 + 30 * NS);
  check(r2 == #AlreadyObserved(NR.eventKey(e1.event_id)), "re-presenting the same event id is #AlreadyObserved");
  check(obsOf(s4, e1.event_id).policy_version == 1 and obsOf(s4, e1.event_id).observed_at == T0 + 10 * NS, "re-observation after a policy change keeps v1 and the first observed_at");
  let e2 = posEvent("sigB", LAMPORTS, #Simple, PUBKEY_A);
  let s5 = observeOk(s4, e2, T0 + 40 * NS);
  check(obsOf(s5, e2.event_id).policy_version == 2, "a NEW event after the change pins policy v2");
  check(s5.policyTable.size() == 2 and s5.policyTable[0].simple21DayPpPerSol == 6_000, "policy table is append-only (v1 untouched)");
};

// observation is keyed on the immutable id: same id, different payload is a permanent reject, not an ack
do {
  let s1 = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(s1, e1, T0);
  let tampered = { e1 with basis_lamports = LAMPORTS * 2 };
  let (_, r) = NR.observe(s2, tampered, T0 + NS);
  check(switch (r) { case (#Rejected x) { x.permanent }; case _ { false } }, "same event id with a different basis is #Rejected permanent, never #AlreadyObserved");
  let tamperedKey = { e1 with beneficiary_pubkey = PUBKEY_B };
  let (_, r2) = NR.observe(s2, tamperedKey, T0 + NS);
  check(switch (r2) { case (#Rejected x) { x.permanent }; case _ { false } }, "same event id with a different beneficiary is #Rejected permanent");
};

Debug.print("== policyChangeBetweenObservationAndBindingDoesNotChangeLegs");
do {
  let s1 = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(s1, e1, T0 + NS);
  let d2 : NR.PolicyDraft = { draft() with simple21DayPpPerSol = 99_999; signupGiftPp = 0; cascadeInitialBps = 0 };
  let s3 = withPolicy(s2, d2, T0 + 2 * NS);
  let out = NR.applyBind(s3, bindEnv(noMarkers, noEffects), e1.event_id, ?alice, T0 + 3 * NS);
  let legs = oblOf(out.state, e1.event_id).legs;
  check(out.result == #Reserved, "bind after a policy change still reserves");
  check(legs.size() == 8 and legs[4].amount == 540_000_000_000, "player leg uses pinned v1 rate (5_400 PP), not v2 99_999");
  check(sum(amounts(legs)) == 6_000 * PP + 500 * PP, "legs conserve base (6_000 PP) + signup gift (500 PP) under v1");
  check(oblOf(out.state, e1.event_id).policy_version == 1, "obligation carries policy_version 1");
};

Debug.print("== bindAndReserveComputesLegsOnceAndConsumesEffectsAtBinding");
do {
  let s1 = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(s1, e1, T0);
  switch (NR.prepareBind(s2, e1.event_id)) {
    case (#Lookup x) { check(x.address == PUBKEY_A and x.policy_version == 1, "prepareBind asks the SIWS lookup for the observed pubkey") };
    case _ { check(false, "prepareBind should return #Lookup") };
  };
  let tBind = T0 + 5 * NS;
  let out = NR.applyBind(s2, bindEnv(noMarkers, noEffects), e1.event_id, ?alice, tBind);
  check(out.result == #Reserved, "applyBind with a resolved principal returns #Reserved");
  let obl = oblOf(out.state, e1.event_id);
  check(obl.status == #Reserved and obl.beneficiary == ?alice, "obligation is #Reserved with the resolved beneficiary");
  // expected: gift 4.5e10, gift cascade (bob 2.5e9, house 1.25e9, residual 1.25e9), player 5.4e11, cascade (bob 3e10, house 1.5e10, residual 1.5e10)
  let expected : [Nat] = [
    45_000_000_000, 2_500_000_000, 1_250_000_000, 1_250_000_000,
    540_000_000_000, 30_000_000_000, 15_000_000_000, 15_000_000_000,
  ];
  check(eqArr(amounts(obl.legs), expected), "PositionOpened 1 SOL Simple: exact leg amounts (gift + cascade + player + cascade)");
  check(sum(expected) == 6_000 * PP + 500 * PP, "arithmetic: legs sum to 6_000 PP base + 500 PP gift");
  var allFrozen = true;
  var idx = 0;
  for (l in obl.legs.vals()) {
    if (l.index != idx or l.fee != null or l.memo.size() != 32 or l.created_at_time != Nat64.fromNat(Int.abs(tBind)) or l.status != #Pending or l.attempts != 0 or l.recipient.owner != SELF) { allFrozen := false };
    switch (l.recipient.subaccount) {
      case (?sa) { if (sa != Subaccount.principalToChipSubaccount(l.holder)) { allFrozen := false } };
      case null { allFrozen := false };
    };
    idx += 1;
  };
  check(allFrozen, "every leg is Pending with frozen index, null fee, 32-byte memo, created_at_time = binding time, chip-subaccount recipient");
  var distinctMemos = true;
  for (a in obl.legs.vals()) { for (b in obl.legs.vals()) { if (a.index != b.index and a.memo == b.memo) { distinctMemos := false } } };
  check(distinctMemos, "every leg has a distinct memo");
  check(principalMap.get(out.signupMarkers, alice) == ?tBind, "signup marker for alice written at binding time");
  check(roleCount(obl.legs, isGift) == 1, "exactly one signup gift leg");

  // second bind: nothing recomputed, nothing consumed
  let again = NR.applyBind(out.state, bindEnv(out.signupMarkers, noEffects), e1.event_id, ?alice, tBind + 99 * NS);
  check(again.result == #AlreadyReserved, "second bind is #AlreadyReserved");
  check(oblOf(again.state, e1.event_id).legs == obl.legs and again.siphonConsumed.size() == 0, "second bind leaves legs untouched and consumes no effect cap");
  check(principalMap.get(again.signupMarkers, alice) == ?tBind, "second bind does not rewrite the signup marker");
};

// effects consumed exactly once at binding (multiplier + siphon with cap)
do {
  // alice already claimed her gift (legacy), so the legs are just the 4-leg player chain
  let markers = principalMap.put(noMarkers, alice, T0 - 1000);
  let fx : NR.EffectsSnapshot = {
    noEffects with
    mintMultiplierBps = func(p : Principal) : ?Nat { if (p == alice) { ?20_000 } else { null } };
    siphon = func(p : Principal) : ?NR.SiphonView {
      if (p == alice) { ?{ siphoner = dave; pctTimes100 = 1_000; remainingCap = 50_000_000_000 } } else { null };
    };
  };
  let s1 = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(s1, e1, T0);
  let out = NR.applyBind(s2, bindEnv(markers, fx), e1.event_id, ?alice, T0 + NS);
  let obl = oblOf(out.state, e1.event_id);
  // player net 5.4e11 x2 = 1.08e12; 10% = 1.08e11 capped at 5e10 -> player 1.03e12, siphon 5e10 to dave
  check(obl.legs.size() == 5, "multiplier+siphon: player leg, siphon leg, 3 cascade legs");
  check(obl.legs[0].amount == 1_030_000_000_000 and obl.legs[0].holder == alice, "player leg = net x multiplier minus capped siphon");
  check(obl.legs[1].amount == 50_000_000_000 and obl.legs[1].holder == dave, "siphon leg pays the siphoner the capped take");
  check(out.siphonConsumed == [(alice, 50_000_000_000)], "siphon cap consumption (alice, 5e10) reported once for the actor to apply");
  let again = NR.applyBind(out.state, bindEnv(out.signupMarkers, fx), e1.event_id, ?alice, T0 + 2 * NS);
  check(again.result == #AlreadyReserved and again.siphonConsumed.size() == 0, "re-binding consumes nothing further");
};

// numeric example per kind
do {
  let s1 = freshState();
  let markers = principalMap.put(noMarkers, alice, T0 - 1000); // no gift in this example
  let a = posEvent("sigPos", LAMPORTS, #Simple, PUBKEY_A);
  let b = seriesAEvent("sigA", 50_000_000, PUBKEY_A); // 0.05 SOL
  let c = promoEvent("sigB", 250_000_000, PUBKEY_A); // 0.25 SOL loss
  let d = posEvent("sigC15", LAMPORTS, #Compound15, PUBKEY_A);
  let e = posEvent("sigC30", LAMPORTS, #Compound30, PUBKEY_A);
  var st = s1;
  for (ev in [a, b, c, d, e].vals()) { st := observeOk(st, ev, T0) };
  func total(ev : NR.NativeEvent) : Nat {
    let out = NR.applyBind(st, bindEnv(markers, noEffects), ev.event_id, ?alice, T0 + NS);
    sum(amounts(oblOf(out.state, ev.event_id).legs));
  };
  check(total(a) == 6_000 * PP, "PositionOpened 1 SOL Simple = 6_000 PP total");
  check(total(d) == 12_000 * PP, "PositionOpened 1 SOL Compound15 = 12_000 PP total");
  check(total(e) == 18_000 * PP, "PositionOpened 1 SOL Compound30 = 18_000 PP total");
  check(total(b) == 6_000 * PP, "SeriesAFunded 0.05 SOL = 6_000 PP total (backer rate 120_000/SOL)");
  check(total(c) == 30_000 * PP, "PromotionApplied loss 0.25 SOL = 30_000 PP total (backer rate on the B loss)");
  let out = NR.applyBind(st, bindEnv(markers, noEffects), b.event_id, ?alice, T0 + NS);
  check(oblOf(out.state, b.event_id).legs[0].amount == 5_400 * PP, "SeriesAFunded player net = 90% of 6_000 PP");
};

Debug.print("== unboundPubkeyStaysObservedAndBindsLaterWithoutRebind");
do {
  let s1 = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(s1, e1, T0);
  let miss = NR.applyBind(s2, bindEnv(noMarkers, noEffects), e1.event_id, null, T0 + NS);
  check(miss.result == #Unbound, "unresolved SIWS lookup returns #Unbound");
  let o = oblOf(miss.state, e1.event_id);
  check(o.status == #Observed and o.beneficiary == null and o.legs.size() == 0, "obligation stays #Observed, pubkey-keyed, no legs");
  check(principalMap.get(miss.signupMarkers, alice) == null and miss.siphonConsumed.size() == 0, "nothing consumed while unbound");
  check(obsOf(miss.state, e1.event_id).beneficiary_pubkey == PUBKEY_A, "observation keeps the pubkey");
  let later = NR.applyBind(miss.state, bindEnv(miss.signupMarkers, noEffects), e1.event_id, ?alice, T0 + 3_600 * NS);
  check(later.result == #Reserved and oblOf(later.state, e1.event_id).beneficiary == ?alice, "binding later reserves for the principal that resolves then");
  check(oblOf(later.state, e1.event_id).policy_version == 1 and oblOf(later.state, e1.event_id).legs[0].created_at_time == Nat64.fromNat(Int.abs(T0 + 3_600 * NS)), "late binding keeps v1 and stamps the legs at the late binding time");
  let rebind = NR.applyBind(later.state, bindEnv(later.signupMarkers, noEffects), e1.event_id, ?bob, T0 + 4_000 * NS);
  check(rebind.result == #AlreadyReserved and oblOf(rebind.state, e1.event_id).beneficiary == ?alice, "a later different lookup never rebinds the reserved obligation");
  let anon = NR.applyBind(s2, bindEnv(noMarkers, noEffects), e1.event_id, ?Principal.fromText("2vxsx-fae"), T0 + NS);
  check(anon.result == #Unbound, "a lookup that resolves to the anonymous principal is treated as unbound");
  let unknown = NR.applyBind(s2, bindEnv(noMarkers, noEffects), eventId("never-seen"), ?alice, T0 + NS);
  check(unknown.result == #UnknownEvent, "binding an unobserved event id is #UnknownEvent");
};

Debug.print("== signupGiftMarkerWrittenBeforeAwaitBothOrderings");
do {
  // pure marker step
  let (m1, first) = NR.claimSignupGiftMarker(noMarkers, alice, T0);
  let (m2, second) = NR.claimSignupGiftMarker(m1, alice, T0 + NS);
  check(first and not second, "claimSignupGiftMarker: first claim wins, second is refused");
  check(principalMap.get(m2, alice) == ?T0, "claimSignupGiftMarker never overwrites the first claim time");

  // native -> legacy: native binds first; the legacy observer's check (marker present) sees it
  let s1 = observeOk(freshState(), posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A), T0);
  let nativeFirst = NR.applyBind(s1, bindEnv(noMarkers, noEffects), eventId("sigA"), ?alice, T0 + NS);
  check(roleCount(oblOf(nativeFirst.state, eventId("sigA")).legs, isGift) == 1, "native->legacy: native emitted the single gift leg");
  check(principalMap.get(nativeFirst.signupMarkers, alice) != null, "native->legacy: marker is set in the same atomic step as the reservation, so the legacy observer skips its gift");

  // legacy -> native: the legacy observer already minted the gift and wrote the shared marker
  let legacyMarkers = principalMap.put(noMarkers, alice, T0 - 5 * NS);
  let legacyFirst = NR.applyBind(s1, bindEnv(legacyMarkers, noEffects), eventId("sigA"), ?alice, T0 + NS);
  check(roleCount(oblOf(legacyFirst.state, eventId("sigA")).legs, isGift) == 0, "legacy->native: native emits no gift leg");
  check(principalMap.get(legacyFirst.signupMarkers, alice) == ?(T0 - 5 * NS), "legacy->native: legacy marker value preserved");
  check(oblOf(legacyFirst.state, eventId("sigA")).legs.size() == 4, "legacy->native: only the player chain remains (4 legs)");

  // gift only on PositionOpened (mirrors the legacy observer: backer deltas never gift)
  let s2 = observeOk(freshState(), seriesAEvent("sigB", 50_000_000, PUBKEY_A), T0);
  let a = NR.applyBind(s2, bindEnv(noMarkers, noEffects), eventId("sigB"), ?alice, T0 + NS);
  check(roleCount(oblOf(a.state, eventId("sigB")).legs, isGift) == 0 and principalMap.get(a.signupMarkers, alice) == null, "SeriesAFunded never claims the signup marker");

  // signupGiftPp = 0 disables the gift and does not claim the marker
  let zero = withPolicy(NR.emptyState(), { draft() with signupGiftPp = 0 }, T0);
  let s3 = observeOk(zero, posEvent("sigC", LAMPORTS, #Simple, PUBKEY_A), T0);
  let z = NR.applyBind(s3, bindEnv(noMarkers, noEffects), eventId("sigC"), ?alice, T0 + NS);
  check(roleCount(oblOf(z.state, eventId("sigC")).legs, isGift) == 0 and principalMap.get(z.signupMarkers, alice) == null, "signupGiftPp = 0 emits no gift and claims no marker");
};

Debug.print("== twoSimultaneousNativeFirstEventsYieldOneSignupGift");
do {
  var s = freshState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let e2 = posEvent("sigB", LAMPORTS, #Compound15, PUBKEY_A);
  s := observeOk(s, e1, T0);
  s := observeOk(s, e2, T0);
  // both callers pass prepareBind before either SIWS await returns
  let p1 = NR.prepareBind(s, e1.event_id);
  let p2 = NR.prepareBind(s, e2.event_id);
  check(switch (p1, p2) { case (#Lookup _, #Lookup _) { true }; case _ { false } }, "both events are bindable (#Observed) while both awaits are in flight");
  // the awaits return in either order; each resumption is one atomic applyBind
  let first = NR.applyBind(s, bindEnv(noMarkers, noEffects), e2.event_id, ?alice, T0 + NS);
  let second = NR.applyBind(first.state, bindEnv(first.signupMarkers, noEffects), e1.event_id, ?alice, T0 + 2 * NS);
  let gifts = roleCount(oblOf(second.state, e1.event_id).legs, isGift) + roleCount(oblOf(second.state, e2.event_id).legs, isGift);
  check(gifts == 1, "two simultaneous first events for one principal yield exactly one signup gift leg");
  check(roleCount(oblOf(second.state, e2.event_id).legs, isGift) == 1 and roleCount(oblOf(second.state, e1.event_id).legs, isGift) == 0, "the gift belongs to whichever binding resumed first");
  // the loser still reserves its full player chain
  check(oblOf(second.state, e1.event_id).legs.size() == 4 and oblOf(second.state, e1.event_id).status == #Reserved, "the second event still reserves its player chain");
};

Debug.print("== bindAndReserveReReadsObligationAfterSiwsAwaitAndRejectsSecondBinder");
do {
  let s = observeOk(freshState(), posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A), T0);
  let id = eventId("sigA");
  let prepA = NR.prepareBind(s, id);
  let prepB = NR.prepareBind(s, id);
  check(switch (prepA, prepB) { case (#Lookup _, #Lookup _) { true }; case _ { false } }, "two callers both pass the pre-await status check");
  let a = NR.applyBind(s, bindEnv(noMarkers, noEffects), id, ?alice, T0 + NS);
  check(a.result == #Reserved, "caller A resumes first and reserves");
  let b = NR.applyBind(a.state, bindEnv(a.signupMarkers, noEffects), id, ?alice, T0 + 2 * NS);
  check(b.result == #AlreadyReserved, "caller B re-reads the obligation after its await and is rejected");
  check(oblOf(b.state, id).legs == oblOf(a.state, id).legs and principalMap.get(b.signupMarkers, alice) == principalMap.get(a.signupMarkers, alice), "second binder changes neither legs nor marker");
  check(NR.prepareBind(a.state, id) == #AlreadyReserved, "prepareBind on a reserved obligation is #AlreadyReserved (no SIWS await spent)");
};

Debug.print("== rejectedPermanentQuarantinesWithReason");
do {
  let s = freshState();
  func isPerm(r : NR.ObserveResult, needle : Text) : Bool {
    switch (r) { case (#Rejected x) { x.permanent and Text.contains(x.reason, #text needle) }; case _ { false } };
  };
  let base = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let badSchema = { base with event_id = { base.event_id with event_schema_version = 99 } };
  let (s1, r1) = NR.observe(s, badSchema, T0);
  check(isPerm(r1, "schema"), "unknown schema version -> permanent reject naming the schema");
  let (_, r2) = NR.observe(s, { base with basis_lamports = 0 }, T0);
  check(isPerm(r2, "basis"), "zero basis -> permanent reject");
  let (_, r3) = NR.observe(s, { base with beneficiary_pubkey = "not-base58-0OIl" }, T0);
  check(isPerm(r3, "beneficiary"), "malformed beneficiary pubkey -> permanent reject");
  let (_, r4) = NR.observe(s, { base with plan = null }, T0);
  check(isPerm(r4, "plan"), "PositionOpened without a plan -> permanent reject");
  let (_, r5) = NR.observe(s, { seriesAEvent("sigB", 50_000_000, PUBKEY_A) with source_close_id = ?1 }, T0);
  check(isPerm(r5, "close"), "SeriesAFunded carrying a close id -> permanent reject");
  let (_, r6) = NR.observe(s, { promoEvent("sigC", 250_000_000, PUBKEY_A) with source_close_id = null }, T0);
  check(isPerm(r6, "close"), "PromotionApplied without a close id -> permanent reject");
  let (_, r7) = NR.observe(s, { base with event_id = { base.event_id with signature = "" } }, T0);
  check(isPerm(r7, "signature"), "empty signature -> permanent reject");
  check(NR.getObservation(s1, badSchema.event_id) == null and s1.observations.size == 0, "a rejected event leaves no observation behind (the bridge owns the durable quarantine record)");
};

Debug.print("== transientRejectionRetainsRetry");
do {
  let empty = NR.emptyState();
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let (s1, r1) = NR.observe(empty, e1, T0);
  check(switch (r1) { case (#Rejected x) { not x.permanent and Text.contains(x.reason, #text "policy") }; case _ { false } }, "empty policy table -> transient reject naming the policy table");
  check(s1.observations.size == 0, "transient reject stores nothing");
  let s2 = withPolicy(s1, draft(), T0 + NS);
  let (_, r2) = NR.observe(s2, e1, T0 + 2 * NS);
  check(r2 == #Observed(NR.eventKey(e1.event_id)), "the same event is observed once the policy exists (retry works)");
};

// an event whose legs all floor to zero reserves nothing and completes at once
do {
  let zeroRates = withPolicy(NR.emptyState(), { draft() with simple21DayPpPerSol = 0; signupGiftPp = 0 }, T0);
  let e1 = posEvent("sigA", LAMPORTS, #Simple, PUBKEY_A);
  let s2 = observeOk(zeroRates, e1, T0);
  let out = NR.applyBind(s2, bindEnv(noMarkers, noEffects), e1.event_id, ?alice, T0 + NS);
  check(out.result == #Reserved and oblOf(out.state, e1.event_id).legs.size() == 0 and oblOf(out.state, e1.event_id).status == #Completed, "no payable legs -> reserved with zero legs and immediately #Completed");
  // a sub-unit amount floors to zero as well (integer lamport math)
  let dust = posEvent("sigD", 9, #Simple, PUBKEY_A); // 9 lamports x 6_000 / 10 = 5_400 units (non-zero)
  let one = posEvent("sigE", 1, #Compound30, PUBKEY_A); // 1 lamport x 18_000 / 10 = 1_800 units
  let s3 = observeOk(observeOk(freshState(), dust, T0), one, T0);
  let markers = principalMap.put(noMarkers, alice, T0 - 1000);
  let d = NR.applyBind(s3, bindEnv(markers, noEffects), dust.event_id, ?alice, T0 + NS);
  check(sum(amounts(oblOf(d.state, dust.event_id).legs)) == 5_400, "9 lamports Simple = 5_400 PP units, exactly (no 10-lamport truncation)");
  let o = NR.applyBind(s3, bindEnv(markers, noEffects), one.event_id, ?alice, T0 + NS);
  check(sum(amounts(oblOf(o.state, one.event_id).legs)) == 1_800, "1 lamport Compound30 = 1_800 PP units, exactly");
};

// allowlist + policy table guards
do {
  let s = NR.setBridgeAllowlist(NR.emptyState(), [bridge]);
  check(NR.isBridge(s, bridge) and not NR.isBridge(s, alice), "bridge allowlist membership");
  check(switch (NR.appendPolicy(NR.emptyState(), { draft() with cascadeInitialBps = 10_001 }, T0)) { case (#Err _) { true }; case _ { false } }, "policy rejects bps over 10_000");
  check(switch (NR.appendPolicy(NR.emptyState(), { draft() with dedupWindowNs = 86_401 * NS }, T0)) { case (#Err _) { true }; case _ { false } }, "policy rejects a dedup window longer than the ledger's 24h window");
  check(switch (NR.appendPolicy(NR.emptyState(), { draft() with dedupWindowNs = 60 * NS }, T0)) { case (#Err _) { true }; case _ { false } }, "policy rejects a dedup window shorter than the safety margin");
  check(switch (NR.appendPolicy(NR.emptyState(), { draft() with signupGiftPp = 1_000_001 }, T0)) { case (#Err _) { true }; case _ { false } }, "policy rejects an absurd signup gift");
  let s2 = withPolicy(withPolicy(NR.emptyState(), draft(), T0), draft(), T0 + NS);
  check(s2.policyTable.size() == 2 and s2.policyTable[0].version == 1 and s2.policyTable[1].version == 2, "versions are assigned 1, 2, ... in append order");
};

if (ok) { Debug.print("ALL PASS (" # Nat.toText(passed) # " checks)") } else { Debug.trap("TEST FAILURES") };
