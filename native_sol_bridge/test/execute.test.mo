// Standalone tests for the bridge-driven execution gate (IMPL-49b / review R2):
// Outbox.beginExecute / applyLegStatus / applyLegError / legsOf / mayExecute.
//
// Run from the repo root:
//   moc -r --package base <base@0.16.0/src> native_sol_bridge/test/execute.test.mo
//
// The actor method `executeReservedLeg` is thin glue over these functions (the async call to
// Shenanigans cannot run under `moc -r`); its end-to-end behaviour is exercised by the replica
// driver (test/replica/drive.py), NOT here.
//
// Entries are built from REAL decoded fixture transactions (flows-007 open, flows-009 Series A,
// closing-009 promotion, closing-012 open), as in outbox.test.mo.
//
// "Negative control" below always means: a valid module and valid inputs in the WRONG state
// (e.g. Forwarded but Unbound), asserting the gate refuses. Mutation-style controls (deleting a
// guard and watching these checks FAIL) are run separately and reported, not encoded here.

import Debug "mo:base/Debug";
import Principal "mo:base/Principal";
import Text "mo:base/Text";

import Decode "../Decode";
import Events "../Events";
import Outbox "../Outbox";
import Verify "../Verify";
import Fx "fixtures.test";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};

let localDomain : Blob = Text.encodeUtf8(Fx.LOCALNET_DOMAIN_TEXT);
let cfg : Verify.Config = { program_id = Fx.PROGRAM; cluster_domain = localDomain; event_authority = Fx.EVENT_AUTHORITY; supported_schema_versions = [1] };
let relayer = Principal.fromText("aaaaa-aa");
let COOLDOWN : Nat64 = 10;

func idOf(name : Text, ixIndex : Nat, innerIndex : Nat) : Events.EventId {
  { cluster_domain = localDomain; program_id = Fx.PROGRAM; signature = Fx.get(name).json.signatures[0]; instruction_index = ixIndex; inner_instruction_index = innerIndex; event_schema_version = 1 };
};
let openD : Events.Descriptor = { event_id = idOf("flows-007", 1, 4); source_round_id = 1; source_close_id = null; kind = #PositionOpened; beneficiary_pubkey = "HixcV1fZc9B9qqWRFThcs5heon6usJAFfbbaSHYXzniG"; basis_lamports = 1_000_000_000; plan = ?#Simple };
let seriesAD : Events.Descriptor = { event_id = idOf("flows-009", 1, 3); source_round_id = 1; source_close_id = null; kind = #SeriesAFunded; beneficiary_pubkey = "E3jUXb3RFs49VhzVqzjFpPDYQwF4evgc9buQXMN5yV2r"; basis_lamports = 50_000_000; plan = null };
let promoD : Events.Descriptor = { event_id = idOf("closing-009", 1, 3); source_round_id = 1; source_close_id = ?1; kind = #PromotionApplied; beneficiary_pubkey = "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew"; basis_lamports = 907_500_000; plan = null };
let open2D : Events.Descriptor = { event_id = idOf("closing-012", 1, 3); source_round_id = 2; source_close_id = null; kind = #PositionOpened; beneficiary_pubkey = "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew"; basis_lamports = 100_000_000; plan = ?#Simple };

func txOf(name : Text) : Decode.Tx {
  switch (Decode.fromRpc(Fx.get(name).rpc)) { case (#ok(t)) { t }; case (#err(_)) { Debug.trap("decode " # name) } };
};
func sourceTx(d : Events.Descriptor) : Decode.Tx {
  for (f in Fx.all.vals()) { if (f.json.signatures[0] == d.event_id.signature) { return txOf(f.name) } };
  Debug.trap("no fixture for descriptor");
};
func obs(tx : Decode.Tx) : Decode.ProviderObservation { #Observed({ commitment = #finalized; tx }) };
func realVerdict(d : Events.Descriptor) : Verify.Verdict { let o = obs(sourceTx(d)); Verify.verify(cfg, d, o, o) };
func key(d : Events.Descriptor) : Text { Events.eventKey(d.event_id) };
func submitOk(s : Outbox.State, d : Events.Descriptor) : Nat {
  switch (Outbox.submit(s, relayer, d, 1)) { case (#Submitted({ seq })) { seq }; case (_) { Debug.trap("submit failed") } };
};
func isApplied(a : Outbox.Applied) : Bool { switch (a) { case (#Applied(_)) true; case (_) false } };
func verify(s : Outbox.State, d : Events.Descriptor) : Bool { isApplied(Outbox.applyVerdict(s, key(d), realVerdict(d), 2)) };
func entryOf(s : Outbox.State, d : Events.Descriptor) : Outbox.Entry { switch (Outbox.get(s, key(d))) { case (?e) e; case (null) Debug.trap("no entry") } };

/// Forwarded (Observed) and bound (Reserved): the only state in which a leg may be executed.
func forwardedBound(s : Outbox.State, d : Events.Descriptor) {
  ignore submitOk(s, d);
  check(verify(s, d), "(setup) verified against the real fixture");
  check(isApplied(Outbox.applyForward(s, key(d), #Observed("obl"), 3)), "(setup) Shenanigans #Observed");
  check(isApplied(Outbox.applyBind(s, key(d), #Reserved, 4)), "(setup) Shenanigans #Reserved");
};
func forwardedUnbound(s : Outbox.State, d : Events.Descriptor) {
  ignore submitOk(s, d);
  check(verify(s, d), "(setup) verified");
  check(isApplied(Outbox.applyForward(s, key(d), #Observed("obl"), 3)), "(setup) #Observed");
};
func isGo(g : Outbox.ExecGate) : Bool { switch (g) { case (#Go(_)) true; case (_) false } };
func gateIs(g : Outbox.ExecGate, want : Outbox.ExecGate) : Bool { g == want };

// =====================================================================
// executeGatesOnForwardedAndBound
// =====================================================================
do {
  Debug.print("== executeGatesOnForwardedAndBound ==");
  let s = Outbox.empty();
  let l = Outbox.emptyLegs();
  check(gateIs(Outbox.beginExecute(s, l, "no-such-event", 0, 100, COOLDOWN), #Unknown), "unknown event key -> Unknown");

  // NEGATIVE CONTROLS (valid module, valid inputs, wrong state): none of these may reach Shenanigans.
  ignore submitOk(s, openD);
  check(gateIs(Outbox.beginExecute(s, l, key(openD), 0, 100, COOLDOWN), #NotForwarded), "negative control: a Pending entry is refused (NotForwarded)");
  check(verify(s, openD), "(setup) verify");
  check(gateIs(Outbox.beginExecute(s, l, key(openD), 0, 100, COOLDOWN), #NotForwarded), "negative control: a Verified-but-unacknowledged entry is refused (NotForwarded)");
  ignore submitOk(s, seriesAD);
  check(isApplied(Outbox.applyVerdict(s, key(seriesAD), #Unverifiable(#NotFound), 2)), "(setup) Unverified");
  check(gateIs(Outbox.beginExecute(s, l, key(seriesAD), 0, 100, COOLDOWN), #NotForwarded), "negative control: an Unverified entry is refused (NotForwarded)");
  ignore submitOk(s, promoD);
  check(isApplied(Outbox.applyVerdict(s, key(promoD), #Invalid(#BasisMismatch), 2)), "(setup) Invalid");
  check(gateIs(Outbox.beginExecute(s, l, key(promoD), 0, 100, COOLDOWN), #NotForwarded), "negative control: an Invalid entry is refused (NotForwarded)");
  ignore submitOk(s, open2D);
  check(verify(s, open2D), "(setup) verify");
  check(isApplied(Outbox.applyForward(s, key(open2D), #Rejected({ reason = "permanent policy rejection"; permanent = true }), 3)), "(setup) Quarantined");
  check(gateIs(Outbox.beginExecute(s, l, key(open2D), 0, 100, COOLDOWN), #NotForwarded), "negative control: a Quarantined entry is refused (NotForwarded)");

  // Forwarded but Unbound: the headline negative control.
  let s2 = Outbox.empty();
  let l2 = Outbox.emptyLegs();
  forwardedUnbound(s2, openD);
  check(gateIs(Outbox.beginExecute(s2, l2, key(openD), 0, 100, COOLDOWN), #NotBound), "negative control: Forwarded but Unbound (no SIWS binding) is refused (NotBound)");
  for (r in [#Unbound, #UnknownEvent, #Rejected("not yet")].vals()) { ignore Outbox.applyBind(s2, key(openD), r, 5) };
  check(gateIs(Outbox.beginExecute(s2, l2, key(openD), 0, 100, COOLDOWN), #NotBound), "negative control: Shenanigans answering Unbound / UnknownEvent / Rejected keeps it refused (NotBound)");
  check(entryOf(s2, openD).next_attempt_at == 0, "a refused gate consumes no cooldown");
  check(Outbox.legsOf(l2, key(openD)).size() == 0, "a refused gate records no leg");

  // Forwarded + Bound: the one positive.
  let s3 = Outbox.empty();
  let l3 = Outbox.emptyLegs();
  forwardedBound(s3, openD);
  switch (Outbox.beginExecute(s3, l3, key(openD), 0, 100, COOLDOWN)) {
    case (#Go(e)) { check(e.next_attempt_at == 110 and e.seq == 1, "Forwarded + Bound -> Go, and the per-event cooldown is armed (100 + 10)") };
    case (_) { check(false, "Forwarded + Bound -> Go") };
  };
  // Leg-index bound.
  check(gateIs(Outbox.beginExecute(s3, l3, key(openD), Outbox.MAX_LEG_INDEX + 1, 1_000, COOLDOWN), #BadLegIndex), "leg index above MAX_LEG_INDEX is refused (BadLegIndex)");
  check(isGo(Outbox.beginExecute(s3, l3, key(openD), Outbox.MAX_LEG_INDEX, 1_000, COOLDOWN)), "leg index == MAX_LEG_INDEX is allowed");
  // The gate never edits what the relayer submitted or what was verified.
  let before = entryOf(s3, openD);
  ignore Outbox.beginExecute(s3, l3, key(openD), 2, 5_000, COOLDOWN);
  let after = entryOf(s3, openD);
  check(after.descriptor == before.descriptor and after.verified == before.verified and after.status == before.status and after.binding == before.binding, "beginExecute changes only the attempt gate (descriptor, verified event, status, binding untouched)");
};

// =====================================================================
// executeCooldownIsPerEventAndClearedByBind
// =====================================================================
do {
  Debug.print("== executeCooldownIsPerEventAndClearedByBind ==");
  let s = Outbox.empty();
  let l = Outbox.emptyLegs();
  forwardedBound(s, openD);
  check(isGo(Outbox.beginExecute(s, l, key(openD), 0, 1_000, COOLDOWN)), "first call is granted");
  switch (Outbox.beginExecute(s, l, key(openD), 1, 1_005, COOLDOWN)) {
    case (#Cooldown(1_010)) { check(true, "a call for ANOTHER leg of the same event inside the cooldown is refused (per-event gate)") };
    case (_) { check(false, "per-event cooldown") };
  };
  check(isGo(Outbox.beginExecute(s, l, key(openD), 1, 1_010, COOLDOWN)), "granted again when the cooldown ends");
  // a different event is unaffected
  forwardedBound(s, seriesAD);
  check(isGo(Outbox.beginExecute(s, l, key(seriesAD), 0, 1_011, COOLDOWN)), "another event has its own gate");

  // Regression (same class as IMPL-47d finding 1): bind arms its own cooldown, a successful bind must clear it.
  let s2 = Outbox.empty();
  let l2 = Outbox.emptyLegs();
  forwardedUnbound(s2, promoD);
  switch (Outbox.beginBind(s2, key(promoD), 2_000, 60)) { case (#Go(e)) { check(e.next_attempt_at == 2_060, "(setup) a bind attempt arms the gate for 60") }; case (_) { check(false, "bind go") } };
  check(isApplied(Outbox.applyBind(s2, key(promoD), #Reserved, 2_001)), "(setup) Reserved");
  check(isGo(Outbox.beginExecute(s2, l2, key(promoD), 0, 2_002, COOLDOWN)), "the first execute right after a successful bind is not throttled by the bind's cooldown");
};

// =====================================================================
// executeRecordsPerLegStatusAndIsIdempotent
// =====================================================================
do {
  Debug.print("== executeRecordsPerLegStatusAndIsIdempotent ==");
  let s = Outbox.empty();
  let l = Outbox.emptyLegs();
  forwardedBound(s, openD);
  let k = key(openD);
  func rec(i : Nat) : ?Outbox.LegRecord { Outbox.getLeg(l, k, i) };
  func isApplied2(a : Outbox.LegApplied) : Bool { switch (a) { case (#Applied(_)) true; case (_) false } };
  func isStale2(a : Outbox.LegApplied) : Bool { switch (a) { case (#Stale(_)) true; case (_) false } };

  check(isApplied2(Outbox.applyLegStatus(s, l, k, 0, #SubmittedUnknown, 10)), "leg 0 answered SubmittedUnknown: recorded");
  switch (rec(0)) { case (?r) { check(r.status == ?#SubmittedUnknown and r.calls == 1 and r.last_error == null and r.checked_at == 10 and r.event_key == k and r.index == 0, "record: status, calls = 1, no error, time, identity") }; case (null) { check(false, "record leg 0") } };
  check(isApplied2(Outbox.applyLegStatus(s, l, k, 1, #Pending, 11)), "leg 1 answered Pending: recorded independently");
  check(rec(0) != null and (switch (rec(1)) { case (?r) r.status == ?#Pending and r.calls == 1; case (null) false }), "legs are recorded independently");

  // not final -> the bridge calls again (Pending / SubmittedUnknown / Retryable / Quarantined / never answered)
  var t : Nat64 = 1_000;
  func nextGo() : Bool { t += 100; isGo(Outbox.beginExecute(s, l, k, 0, t, COOLDOWN)) };
  check(nextGo(), "SubmittedUnknown is not final: execute calls Shenanigans again");
  check(isApplied2(Outbox.applyLegStatus(s, l, k, 0, #Retryable({ reason = "InsufficientFunds balance=0"; attempts = 2 }), t)), "answered Retryable");
  check(nextGo(), "Retryable is not final: execute calls again (identical frozen fields; Shenanigans decides)");
  check(isApplied2(Outbox.applyLegStatus(s, l, k, 0, #Quarantined({ reason = "past the dedup window"; recoverable = true }), t)), "answered Quarantined");
  check(nextGo(), "Quarantined is not final here: execute asks again (it may have been reconciled meanwhile)");
  check(nextGo() and isGo(Outbox.beginExecute(s, l, k, 5, t + 1_000, COOLDOWN)), "a leg the bridge never heard about may be asked");
  switch (rec(0)) { case (?r) { check(r.calls == 3 and r.status == ?#Quarantined({ reason = "past the dedup window"; recoverable = true }), "calls count every answer; the record shows the LAST status") }; case (null) { check(false, "leg 0") } };

  // a failed call attaches an error to a known leg and keeps its last status
  check(isApplied2(Outbox.applyLegError(l, k, 0, "canister j56tm is stopped", 5_000)), "a failed call is recorded on a known leg");
  switch (rec(0)) { case (?r) { check(r.last_error == ?"canister j56tm is stopped" and r.status == ?#Quarantined({ reason = "past the dedup window"; recoverable = true }) and r.calls == 4, "...status unchanged, error kept, call counted") }; case (null) { check(false, "leg 0") } };
  check(isApplied2(Outbox.applyLegStatus(s, l, k, 0, #SubmittedUnknown, 5_100)), "the next answer arrives");
  switch (rec(0)) { case (?r) { check(r.last_error == null, "...and clears the last error") }; case (null) { check(false, "leg 0") } };
  // storage is bounded by real legs
  switch (Outbox.applyLegError(l, k, 40, "unknown native reward leg", 5_200)) { case (#Unknown) { check(Outbox.getLeg(l, k, 40) == null, "a failed call for an index Shenanigans never confirmed creates NO record (garbage indices cannot grow the store)") }; case (_) { check(false, "unknown leg error") } };
  check(Outbox.legsOf(l, k).size() == 2, "legsOf lists only recorded legs (0 and 1), ascending");
  check(Outbox.legsOf(l, k)[0].index == 0 and Outbox.legsOf(l, k)[1].index == 1, "...in ascending order");

  // Committed is final and idempotent
  check(isApplied2(Outbox.applyLegStatus(s, l, k, 0, #Committed({ block = 77 }), 6_000)), "leg 0 answered Committed{77}");
  let armed = entryOf(s, openD).next_attempt_at;
  let g = Outbox.beginExecute(s, l, k, 0, 6_001, COOLDOWN);
  switch (g) {
    case (#Stored(r)) { check(r.status == ?#Committed({ block = 77 }), "a Committed leg is answered from the record: Stored, no call") };
    case (_) { check(false, "committed leg is Stored") };
  };
  check(entryOf(s, openD).next_attempt_at == armed, "...and it consumes no cooldown");
  var repeated = true;
  var q : Nat64 = 6_002;
  while (q < 6_040) { switch (Outbox.beginExecute(s, l, k, 0, q, COOLDOWN)) { case (#Stored(_)) {}; case (_) { repeated := false } }; q += 1 };
  check(repeated, "repeated calls for a Committed leg never trigger a call, even inside any cooldown window");
  check(isStale2(Outbox.applyLegStatus(s, l, k, 0, #Pending, 6_100)) and isStale2(Outbox.applyLegStatus(s, l, k, 0, #Committed({ block = 99 }), 6_101)) and isStale2(Outbox.applyLegError(l, k, 0, "late failure", 6_102)), "a stale answer (Pending, a different block, an error) never overwrites a Committed record");
  switch (rec(0)) { case (?r) { check(r.status == ?#Committed({ block = 77 }) and r.last_error == null, "...the record still says Committed{77}") }; case (null) { check(false, "leg 0") } };
  // other legs of the same event still execute
  check(isGo(Outbox.beginExecute(s, l, k, 1, 7_000, COOLDOWN)), "a different leg of the same event is still executable");
};

// =====================================================================
// executeRefusesAnonymous
// =====================================================================
do {
  Debug.print("== executeRefusesAnonymous ==");
  check(not Outbox.mayExecute(Principal.fromText("2vxsx-fae")), "the anonymous principal may not execute");
  check(Outbox.mayExecute(Principal.fromText("aaaaa-aa")) and Outbox.mayExecute(Principal.fromText("m7ydg-dfxxf-u5wan-ft6m4-xmzeg-wj6mw-npaab-cn6oc-5tgu4-rm2s7-yqe")), "any authenticated principal may (permissionless)");
};

if (failures > 0) { Debug.trap("execute.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("execute.test.mo: ALL PASS");
