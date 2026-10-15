// Standalone tests for Outbox: the durable pending outbox, the cursor, dedup on
// the immutable event id, the GAP rule and pubkey-keyed pending/bind release.
//
// Run from the repo root:
//   moc -r --package base <base@0.16.0/src> native_sol_bridge/test/outbox.test.mo
//
// Entries are built from REAL decoded fixture transactions: each "verified"
// entry below carries the Verify.Verified that Verify.verify produced from a
// captured finalized transaction (flows-007 open, flows-009 Series A,
// closing-009 promotion, closing-012 open).

import Array "mo:base/Array";
import Debug "mo:base/Debug";
import Nat64 "mo:base/Nat64";
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

func idOf(name : Text, ixIndex : Nat, innerIndex : Nat) : Events.EventId {
  { cluster_domain = localDomain; program_id = Fx.PROGRAM; signature = Fx.get(name).json.signatures[0]; instruction_index = ixIndex; inner_instruction_index = innerIndex; event_schema_version = 1 };
};
let HIXC = "HixcV1fZc9B9qqWRFThcs5heon6usJAFfbbaSHYXzniG";
let E3JU = "E3jUXb3RFs49VhzVqzjFpPDYQwF4evgc9buQXMN5yV2r";
let BOW1 = "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew";

let openD : Events.Descriptor = { event_id = idOf("flows-007", 1, 4); source_round_id = 1; source_close_id = null; kind = #PositionOpened; beneficiary_pubkey = HIXC; basis_lamports = 1_000_000_000; plan = ?#Simple };
let seriesAD : Events.Descriptor = { event_id = idOf("flows-009", 1, 3); source_round_id = 1; source_close_id = null; kind = #SeriesAFunded; beneficiary_pubkey = E3JU; basis_lamports = 50_000_000; plan = null };
let promoD : Events.Descriptor = { event_id = idOf("closing-009", 1, 3); source_round_id = 1; source_close_id = ?1; kind = #PromotionApplied; beneficiary_pubkey = BOW1; basis_lamports = 907_500_000; plan = null };
let open2D : Events.Descriptor = { event_id = idOf("closing-012", 1, 3); source_round_id = 2; source_close_id = null; kind = #PositionOpened; beneficiary_pubkey = BOW1; basis_lamports = 100_000_000; plan = ?#Simple };

func txOf(name : Text) : Decode.Tx {
  switch (Decode.fromRpc(Fx.get(name).rpc)) { case (#ok(t)) { t }; case (#err(_)) { Debug.trap("decode " # name) } };
};
func sourceTx(d : Events.Descriptor) : Decode.Tx {
  // descriptor -> the fixture transaction holding its signature
  for (f in Fx.all.vals()) { if (f.json.signatures[0] == d.event_id.signature) { return txOf(f.name) } };
  Debug.trap("no fixture for descriptor");
};
func obs(tx : Decode.Tx) : Decode.ProviderObservation { #Observed({ commitment = #finalized; tx }) };
func realVerdict(d : Events.Descriptor) : Verify.Verdict { let o = obs(sourceTx(d)); Verify.verify(cfg, d, o, o) };
func key(d : Events.Descriptor) : Text { Events.eventKey(d.event_id) };

func submitOk(s : Outbox.State, d : Events.Descriptor, now : Nat64) : Nat {
  switch (Outbox.submit(s, relayer, d, now)) { case (#Submitted({ seq })) { seq }; case (_) { Debug.trap("submit failed") } };
};
func statusOf(s : Outbox.State, d : Events.Descriptor) : Outbox.Status {
  switch (Outbox.get(s, key(d))) { case (?e) { e.status }; case (null) { Debug.trap("no entry") } };
};
func isMember(es : [Outbox.Entry], seq : Nat) : Bool { Array.find<Outbox.Entry>(es, func(e) { e.seq == seq }) != null };
func isApplied(a : Outbox.Applied) : Bool { switch (a) { case (#Applied(_)) true; case (_) false } };
func isStale(a : Outbox.Applied) : Bool { switch (a) { case (#Stale(_)) true; case (_) false } };
func verify(s : Outbox.State, d : Events.Descriptor, now : Nat64) : Bool { isApplied(Outbox.applyVerdict(s, key(d), realVerdict(d), now)) };
func isForwarded(st : Outbox.Status) : Bool { switch (st) { case (#Forwarded(_)) true; case (_) false } };

// =====================================================================
// eventIdTupleIsImmutableAndDedups
// =====================================================================
do {
  Debug.print("== eventIdTupleIsImmutableAndDedups ==");
  let s = Outbox.empty();
  check(submitOk(s, openD, 10) == 1, "first descriptor gets seq 1");
  switch (Outbox.submit(s, relayer, openD, 11)) {
    case (#Duplicate({ seq = 1; same_descriptor = true })) { check(true, "re-submitting the same descriptor is a Duplicate of seq 1") };
    case (_) { check(false, "identical resubmission is a Duplicate") };
  };
  let conflicting : Events.Descriptor = { openD with beneficiary_pubkey = E3JU; basis_lamports = 1 };
  switch (Outbox.submit(s, relayer, conflicting, 12)) {
    case (#Duplicate({ seq = 1; same_descriptor = false })) { check(true, "same event id with different fields is a Duplicate that reports the conflict") };
    case (_) { check(false, "conflicting resubmission is a Duplicate") };
  };
  switch (Outbox.get(s, key(openD))) {
    case (?e) { check(e.descriptor == openD and e.submitted_at == 10 and e.seq == 1, "the stored entry is untouched by the duplicates (descriptor, time, seq)") };
    case (null) { check(false, "entry exists") };
  };
  check(submitOk(s, seriesAD, 13) == 2, "duplicates consumed no sequence number (next distinct event is seq 2)");

  // Every tuple component is part of the identity: change one at a time.
  let base = openD.event_id;
  let variants : [Events.EventId] = [
    { base with cluster_domain = Text.encodeUtf8("musical-chairs-devnet-domain-v1!") },
    { base with program_id = "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf" },
    { base with signature = Fx.get("flows-009").json.signatures[0] },
    { base with instruction_index = 2 },
    { base with inner_instruction_index = 3 },
    { base with event_schema_version = 2 },
  ];
  var distinct = true;
  var i = 0;
  while (i < variants.size()) {
    if (Events.eventKey(variants[i]) == Events.eventKey(base)) { distinct := false };
    var j = i + 1;
    while (j < variants.size()) { if (Events.eventKey(variants[i]) == Events.eventKey(variants[j])) { distinct := false }; j += 1 };
    i += 1;
  };
  check(distinct, "changing any one of the six tuple components yields a different event key");
  var allNew = true;
  for (v in variants.vals()) { switch (Outbox.submit(s, relayer, { openD with event_id = v }, 14)) { case (#Submitted(_)) {}; case (_) { allNew := false } } };
  check(allNew, "each of those six variants is a distinct new event");

  // Once verified and forwarded the entry stays put under further submissions.
  check(verify(s, openD, 20), "(setup) open verifies against the real fixture");
  check(isApplied(Outbox.applyForward(s, key(openD), #Observed("obl-1"), 21)), "(setup) Shenanigans acknowledges #Observed");
  switch (Outbox.submit(s, relayer, openD, 22)) {
    case (#Duplicate({ status = #Forwarded(_) })) { check(true, "a forwarded event stays Forwarded under resubmission") };
    case (_) { check(false, "forwarded event resubmission is a Duplicate with its Forwarded status") };
  };

  // Malformed shapes never get a key.
  switch (Outbox.submit(s, relayer, { openD with event_id = { base with signature = "not-base58-0OIl" } }, 30)) { case (#Malformed(_)) { check(true, "non-base58 signature is Malformed") }; case (_) { check(false, "malformed signature") } };
  switch (Outbox.submit(s, relayer, { openD with event_id = { base with cluster_domain = Text.encodeUtf8("short") } }, 30)) { case (#Malformed(_)) { check(true, "31-byte cluster_domain is Malformed") }; case (_) { check(false, "malformed domain") } };
  switch (Outbox.submit(s, relayer, { openD with beneficiary_pubkey = "abc" }, 30)) { case (#Malformed(_)) { check(true, "non-pubkey beneficiary is Malformed") }; case (_) { check(false, "malformed beneficiary") } };
  switch (Outbox.submit(s, relayer, { openD with event_id = { base with signature = "1111111111111111111111111111111111111111111111111111111111111111111" } }, 30)) { case (#Malformed(_)) { check(true, "a base58 string that decodes to the wrong length is Malformed") }; case (_) { check(false, "wrong-length signature") } };
};

// A provably false descriptor cannot squat on an event id.
do {
  let s = Outbox.empty();
  let forged : Events.Descriptor = { openD with basis_lamports = 1 };
  check(submitOk(s, forged, 1) == 1, "forged descriptor (wrong basis) is accepted for verification");
  switch (Outbox.applyVerdict(s, key(forged), realVerdict(forged), 2)) {
    case (#Applied(e)) { check(e.status == #Invalid({ reason = #BasisMismatch; at = 2 }), "it is proven false: Invalid(BasisMismatch), a durable record") };
    case (_) { check(false, "forged descriptor verdict applied") };
  };
  check(Outbox.cursor(s) == 1, "the cursor may pass a durable Invalid record");
  check(submitOk(s, openD, 3) == 2, "a corrected descriptor for the same event id is accepted as a NEW entry (seq 2)");
  switch (Outbox.get(s, key(openD))) { case (?e) { check(e.seq == 2 and e.descriptor == openD, "the event id now points at the corrected entry") }; case (null) { check(false, "index") } };
  switch (Outbox.getBySeq(s, 1)) { case (?e) { check(e.status == #Invalid({ reason = #BasisMismatch; at = 2 }), "the Invalid entry stays as history") }; case (null) { check(false, "history") } };
  check(Outbox.cursor(s) == 1, "the corrected entry is open, so the cursor stays behind it");
};

// =====================================================================
// cursorAdvancesOnlyAfterReservationReturned
// =====================================================================
do {
  Debug.print("== cursorAdvancesOnlyAfterReservationReturned ==");
  let s = Outbox.empty();
  ignore submitOk(s, openD, 1);
  ignore submitOk(s, seriesAD, 1);
  ignore submitOk(s, promoD, 1);
  check(Outbox.cursor(s) == 0, "three open entries: cursor 0");
  check(verify(s, openD, 2) and verify(s, seriesAD, 2) and verify(s, promoD, 2), "(setup) all three verify against real fixtures");
  check(Outbox.cursor(s) == 0, "verified is not forwarded: cursor still 0");

  // The forward call to Shenanigans fails (reject / trap / timeout): outcome unknown, retain.
  check(isApplied(Outbox.applyForwardError(s, key(openD), "IC0504: canister rejected", 3)), "forward call failure is recorded");
  switch (statusOf(s, openD)) { case (#Verified({ retries = 1; last_reason = ?_ })) { check(true, "after a call failure the entry is still Verified with retries = 1") }; case (_) { check(false, "retained for retry") } };
  check(Outbox.cursor(s) == 0, "a failed forward call does not move the cursor");

  // Entry 3 is acknowledged first: the cursor must not jump over entries 1 and 2.
  check(isApplied(Outbox.applyForward(s, key(promoD), #Observed("obl-3"), 4)), "entry 3 acknowledged #Observed");
  check(Outbox.cursor(s) == 0, "entry 3 done but entries 1 and 2 open: cursor 0 (no skipping)");

  // A transient Shenanigans rejection retains retry.
  check(isApplied(Outbox.applyForward(s, key(seriesAD), #Rejected({ reason = "temporarily unavailable"; permanent = false }), 5)), "entry 2 transiently rejected");
  switch (statusOf(s, seriesAD)) { case (#Verified({ retries = 1; last_reason = ?"temporarily unavailable" })) { check(true, "transient rejection: Verified with the reason, retry retained") }; case (_) { check(false, "transient rejection retains retry") } };
  check(Outbox.cursor(s) == 0, "a transient rejection does not move the cursor");

  // #AlreadyObserved counts like #Observed.
  check(isApplied(Outbox.applyForward(s, key(openD), #AlreadyObserved("obl-1"), 6)), "entry 1 acknowledged #AlreadyObserved");
  check(Outbox.cursor(s) == 1, "entries 1 done, entry 2 still open: cursor 1");

  // A permanent rejection writes a durable quarantine record; only then may the cursor pass.
  check(isApplied(Outbox.applyForward(s, key(seriesAD), #Rejected({ reason = "schema not supported by policy"; permanent = true }), 7)), "entry 2 permanently rejected");
  switch (Outbox.get(s, key(seriesAD))) { case (?e) { check(e.status == #Quarantined({ reason = "schema not supported by policy"; at = 7 }), "the permanent rejection is a durable Quarantined record carrying the reason") }; case (null) { check(false, "quarantined entry exists") } };
  check(Outbox.cursor(s) == 3, "cursor passes the quarantine record and everything before it: cursor 3");
  check(Outbox.pending(s, 0, 10).size() == 0, "nothing is pending any more");

  // Stale transitions change nothing.
  check(isStale(Outbox.applyForward(s, key(openD), #Rejected({ reason = "late"; permanent = true }), 8)), "a late rejection for an already Forwarded entry is Stale");
  check(isForwarded(statusOf(s, openD)), "and the entry is still Forwarded");
  check(isStale(Outbox.applyVerdict(s, key(openD), #Invalid(#BasisMismatch), 9)), "a late verdict for a Forwarded entry is Stale");
  check(Outbox.cursor(s) == 3, "cursor is monotonic");
};

// =====================================================================
// unsupportedTransactionVersionLeavesGapNeverAdvancesCursor
// =====================================================================
do {
  Debug.print("== unsupportedTransactionVersionLeavesGapNeverAdvancesCursor ==");
  let s = Outbox.empty();
  ignore submitOk(s, openD, 1);
  ignore submitOk(s, seriesAD, 1);
  ignore submitOk(s, promoD, 1);
  // entry 2: provider A reports a version-1 transaction (the gap); B is fine.
  let gapVerdict = Verify.verify(cfg, seriesAD, #Undecodable(#UnsupportedVersion(?1)), obs(sourceTx(seriesAD)));
  check(gapVerdict == #Unverifiable(#UnsupportedTransactionVersion(?1)), "(setup) the version-1 observation yields an UnsupportedTransactionVersion verdict");
  check(verify(s, openD, 2), "(setup) entry 1 verifies");
  check(isApplied(Outbox.applyVerdict(s, key(seriesAD), gapVerdict, 2)), "entry 2 recorded as Unverified (a GAP)");
  check(Outbox.isGap(statusOf(s, seriesAD)), "entry 2 is classified as a gap");
  check(verify(s, promoD, 2), "(setup) entry 3 verifies");
  check(isApplied(Outbox.applyForward(s, key(openD), #Observed("obl-1"), 3)) and isApplied(Outbox.applyForward(s, key(promoD), #Observed("obl-3"), 3)), "entries 1 and 3 are forwarded");
  check(Outbox.cursor(s) == 1, "cursor stops at 1: it never advances over the gap at 2, although 3 is done");
  // Re-trying the gap does not move it (the gap persists; attempts count up).
  var n = 0;
  while (n < 5) { ignore Outbox.applyVerdict(s, key(seriesAD), gapVerdict, 4 + Nat64.fromNat(n)); n += 1 };
  switch (statusOf(s, seriesAD)) { case (#Unverified({ attempts = 6 })) { check(true, "six attempts recorded on the gap entry") }; case (_) { check(false, "attempts counted") } };
  check(Outbox.cursor(s) == 1, "cursor is still 1 after repeated attempts");
  let held = Outbox.pending(s, Outbox.cursor(s), 10);
  check(held.size() == 1 and held[0].seq == 2 and Outbox.isGap(held[0].status), "pending(cursor) names exactly the gap entry as what holds the cursor back");
  // The same gap from a JSON-RPC -32015 reply.
  let viaRpc = Decode.classify(#finalized, #Consistent(#Err(#JsonRpcError({ code = -32015; message = "Transaction version (1) is not supported by the requesting client" }))));
  check(Verify.verify(cfg, seriesAD, viaRpc, viaRpc) == #Unverifiable(#UnsupportedTransactionVersion(null)), "(control) a provider -32015 reply is the same gap");
  // A schema gap behaves the same.
  check(Outbox.isGap(#Unverified({ reason = #UnsupportedSchema(2); attempts = 1; last_at = 0 })) and not Outbox.isGap(#Unverified({ reason = #NotFound; attempts = 1; last_at = 0 })), "unsupported schema is a gap; NotFound is merely pending");
  // Once this build can decode it, it verifies, forwards, and only then does the cursor pass.
  check(verify(s, seriesAD, 20), "the gap entry later verifies (decoder support arrives)");
  check(Outbox.cursor(s) == 1, "verified is still not forwarded: cursor 1");
  check(isApplied(Outbox.applyForward(s, key(seriesAD), #Observed("obl-2"), 21)), "forwarded");
  check(Outbox.cursor(s) == 3, "the cursor now passes 2 and 3");
};

// =====================================================================
// missingSiwsBindingKeepsPubkeyPendingAndReleasesOnBind
// =====================================================================
do {
  Debug.print("== missingSiwsBindingKeepsPubkeyPendingAndReleasesOnBind ==");
  let s = Outbox.empty();
  // Two forwarded events share the beneficiary BOW1 (a promotion and an opening); one is for HIXC.
  ignore submitOk(s, promoD, 1);
  ignore submitOk(s, open2D, 1);
  ignore submitOk(s, openD, 1);
  check(verify(s, promoD, 2) and verify(s, open2D, 2) and verify(s, openD, 2), "(setup) three events verify against real fixtures");
  check(Outbox.unboundFor(s, BOW1).size() == 0, "nothing is unbound before Shenanigans acknowledges");
  check(isApplied(Outbox.applyForward(s, key(promoD), #Observed("obl-p"), 3)) and isApplied(Outbox.applyForward(s, key(open2D), #Observed("obl-o"), 3)) and isApplied(Outbox.applyForward(s, key(openD), #Observed("obl-h"), 3)), "(setup) all three acknowledged #Observed");
  check(Outbox.cursor(s) == 3, "the cursor has passed all three: observation does not wait for a SIWS binding");
  check(Outbox.unboundFor(s, BOW1).size() == 2 and Outbox.unboundFor(s, HIXC).size() == 1, "events with no binding are kept under their beneficiary pubkey (2 for Bow1, 1 for Hixc)");

  // Shenanigans cannot resolve a principal yet: stays pubkey-keyed. Every non-bound answer leaves it so.
  for (r in [#Unbound, #UnknownEvent, #Rejected("not yet")].vals()) {
    check(isApplied(Outbox.applyBind(s, key(promoD), r, 4)), "bind attempt answered " # debug_show (r) # " is recorded without effect");
  };
  check(Outbox.unboundFor(s, BOW1).size() == 2, "still pubkey-pending after unbound answers");
  switch (Outbox.get(s, key(promoD))) { case (?e) { check(e.binding == #Unbound and isForwarded(e.status), "binding is still Unbound and the event is still Forwarded") }; case (null) { check(false, "entry") } };

  // The user signs in with that wallet; releasePending(event_id) -> #Reserved for one, #AlreadyReserved for the other.
  check(isApplied(Outbox.applyBind(s, key(promoD), #Reserved, 5)), "promotion bound: #Reserved");
  check(Outbox.unboundFor(s, BOW1).size() == 1, "released from the pubkey index");
  check(isApplied(Outbox.applyBind(s, key(open2D), #AlreadyReserved, 6)), "opening bound: #AlreadyReserved (another caller got there first)");
  check(Outbox.unboundFor(s, BOW1).size() == 0, "all of Bow1's events are released");
  check(Outbox.unboundFor(s, HIXC).size() == 1, "an unrelated beneficiary is unaffected");
  switch (Outbox.get(s, key(promoD))) {
    case (?e) {
      let v = switch (e.verified) { case (?x) x; case (null) { Debug.trap("verified present") } };
      check(e.binding == #Bound({ at = 5 }) and v.forward.beneficiary_pubkey == BOW1 and e.descriptor.beneficiary_pubkey == BOW1 and e.descriptor == promoD, "binding adds a fact only: the stored descriptor and the verified beneficiary (the event's winner) are not rewritten");
    };
    case (null) { check(false, "entry") };
  };
  check(Outbox.cursor(s) == 3, "binding never moves the cursor");
  // Binding something that is not Forwarded is stale.
  let s2 = Outbox.empty();
  ignore submitOk(s2, openD, 1);
  check(isStale(Outbox.applyBind(s2, key(openD), #Reserved, 2)), "a Pending entry cannot be bound (Stale)");
};

// =====================================================================
// attempt gating (cycles bound for permissionless callers)
// =====================================================================
do {
  let s = Outbox.empty();
  ignore submitOk(s, openD, 1);
  switch (Outbox.beginAttempt(s, key(openD), 1_000, 30)) { case (#Go(e)) { check(e.next_attempt_at == 1_030, "first attempt is granted and the cooldown is persisted") }; case (_) { check(false, "first attempt granted") } };
  switch (Outbox.beginAttempt(s, key(openD), 1_010, 30)) { case (#Cooldown(1_030)) { check(true, "a second attempt inside the cooldown is refused") }; case (_) { check(false, "cooldown enforced") } };
  switch (Outbox.beginAttempt(s, key(openD), 1_030, 30)) { case (#Go(_)) { check(true, "attempts resume when the cooldown ends") }; case (_) { check(false, "attempt after cooldown") } };
  switch (Outbox.beginAttempt(s, "nope", 5_000, 30)) { case (#Unknown) { check(true, "unknown event key") }; case (_) { check(false, "unknown key") } };
  switch (Outbox.beginAttempt(s, key(openD), 1_990, 30)) { case (#Go(e)) { check(e.next_attempt_at == 2_020, "(setup) a verification attempt just before the forward arms the cooldown") }; case (_) { check(false, "setup attempt before forward") } };
  check(verify(s, openD, 2000), "(setup) verified");
  check(isApplied(Outbox.applyForward(s, key(openD), #Observed("o"), 2001)), "(setup) forwarded");
  switch (Outbox.beginBind(s, key(openD), 2_002, 30)) { case (#Go(_)) { check(true, "a bind right after a successful forward is not throttled by the forward's cooldown (IMPL-47d finding 1)") }; case (#Cooldown(t)) { check(false, "bind throttled after forward until " # debug_show t) }; case (_) { check(false, "bind go after forward") } };
  switch (Outbox.beginAttempt(s, key(openD), 9_999, 30)) { case (#Terminal(_)) { check(true, "a durable-done entry needs no further attempts") }; case (_) { check(false, "terminal") } };
  switch (Outbox.beginBind(s, key(openD), 10_000, 30)) { case (#Go(_)) { check(true, "bind attempt allowed for Forwarded + Unbound") }; case (_) { check(false, "bind go") } };
  switch (Outbox.beginBind(s, key(openD), 10_001, 30)) { case (#Cooldown(_)) { check(true, "bind attempts are rate limited too") }; case (_) { check(false, "bind cooldown") } };
};

// =====================================================================
// verifiedHistoryExcludesIntake (IMPL-49b / review R3): the PUBLIC history is the
// verified-delivery checkpoint only; intake/audit statuses are a separate page.
// =====================================================================
do {
  Debug.print("== verifiedHistoryExcludesIntake ==");
  let s = Outbox.empty();
  func variant(d : Events.Descriptor, inner : Nat) : Events.Descriptor { { d with event_id = { d.event_id with inner_instruction_index = inner } } };
  let dPending = variant(openD, 9);   // seq 1: submitted, never verified
  let dUnverified = variant(openD, 8); // seq 2: providers have no such tx
  let dInvalid = variant(openD, 7);   // seq 3: provably false
  ignore submitOk(s, dPending, 1);
  ignore submitOk(s, dUnverified, 1);
  ignore submitOk(s, dInvalid, 1);
  ignore submitOk(s, openD, 1);    // seq 4: Verified, not yet acknowledged by Shenanigans
  ignore submitOk(s, seriesAD, 1); // seq 5: Forwarded (Observed)
  ignore submitOk(s, promoD, 1);   // seq 6: Forwarded (AlreadyObserved)
  ignore submitOk(s, open2D, 1);   // seq 7: Quarantined (durable permanent Shenanigans rejection)
  check(isApplied(Outbox.applyVerdict(s, key(dUnverified), #Unverifiable(#NotFound), 2)), "(setup) seq 2 Unverified(NotFound)");
  check(isApplied(Outbox.applyVerdict(s, key(dInvalid), #Invalid(#BasisMismatch), 2)), "(setup) seq 3 Invalid(BasisMismatch)");
  check(verify(s, openD, 2) and verify(s, seriesAD, 2) and verify(s, promoD, 2) and verify(s, open2D, 2), "(setup) four real events verify");
  check(isApplied(Outbox.applyForward(s, key(seriesAD), #Observed("obl-5"), 3)), "(setup) seq 5 #Observed");
  check(isApplied(Outbox.applyForward(s, key(promoD), #AlreadyObserved("obl-6"), 3)), "(setup) seq 6 #AlreadyObserved");
  check(isApplied(Outbox.applyForward(s, key(open2D), #Rejected({ reason = "policy refuses this event"; permanent = true }), 3)), "(setup) seq 7 permanently rejected");
  func seqs(es : [Outbox.Entry]) : [Nat] { Array.map<Outbox.Entry, Nat>(es, func(e) { e.seq }) };

  let pub = Outbox.verifiedHistory(s, null, 100);
  check(seqs(pub) == [7, 6, 5], "verified history = Quarantined(7), Forwarded/AlreadyObserved(6), Forwarded/Observed(5), newest first (got " # debug_show (seqs(pub)) # ")");
  check(not isMember(pub, 1) and not isMember(pub, 2) and not isMember(pub, 3) and not isMember(pub, 4), "Pending, Unverified, Invalid and Verified-unacknowledged are EXCLUDED from the verified history");
  switch (pub[0].status, pub[1].status, pub[2].status) {
    case (#Quarantined({ reason = "policy refuses this event" }), #Forwarded({ already_observed = true }), #Forwarded({ already_observed = false })) {
      check(true, "returned statuses are right: Quarantined carries its reason; Forwarded keeps already_observed true/false");
    };
    case (_) { check(false, "returned statuses") };
  };
  var allVerified = true;
  for (e in pub.vals()) { if (e.verified == null) { allVerified := false } };
  check(allVerified, "every verified-history entry carries the decoded event and the forwarded descriptor");

  let intake = Outbox.intake(s, null, 100);
  check(seqs(intake) == [4, 3, 2, 1], "intake page = Verified-unacknowledged(4), Invalid(3), Unverified(2), Pending(1) (got " # debug_show (seqs(intake)) # ")");
  check(Array.size(pub) + Array.size(intake) == 7, "the two pages partition the store: 3 + 4 = 7 entries, no overlap");
  switch (intake[1].status) { case (#Invalid(_)) { check(true, "the Invalid entry is visible to operators in the intake page") }; case (_) { check(false, "invalid in intake") } };

  // paging by `before` is unchanged: strictly older entries, within each page
  check(seqs(Outbox.verifiedHistory(s, ?7, 100)) == [6, 5], "verifiedHistory(before = 7) -> [6, 5]");
  check(seqs(Outbox.verifiedHistory(s, ?6, 100)) == [5], "verifiedHistory(before = 6) -> [5]");
  check(seqs(Outbox.verifiedHistory(s, ?5, 100)) == [], "verifiedHistory(before = 5) -> [] (older entries are all intake)");
  check(seqs(Outbox.verifiedHistory(s, null, 2)) == [7, 6], "verifiedHistory(limit = 2) -> newest two");
  check(seqs(Outbox.intake(s, ?4, 100)) == [3, 2, 1], "intake(before = 4) -> [3, 2, 1]");
  check(seqs(Outbox.intake(s, null, 1)) == [4], "intake(limit = 1) -> [4]");
  check(seqs(Outbox.pending(s, 0, 1_000)) == [1, 2, 4], "pending lists exactly the cursor-blocking entries (Pending 1, Unverified 2, Verified-unacknowledged 4); Invalid 3 is passable");
  check(Outbox.pending(s, 4, 10).size() == 0, "pending(after = 4): nothing open beyond seq 4");
  let big = Outbox.empty();
  var k = 0;
  while (k < 105) { ignore submitOk(big, variant(openD, 20 + k), 1); k += 1 };
  check(Outbox.intake(big, null, 1_000).size() == Outbox.MAX_PAGE and Outbox.pending(big, 0, 1_000).size() == Outbox.MAX_PAGE and Outbox.verifiedHistory(big, null, 1_000).size() == 0,
    "page sizes are capped at MAX_PAGE (100) whatever the caller's limit; 105 pending entries give a 100-entry intake page and an empty verified page");

  // Invalid is an intake terminal, not delivery: it is passable (cursor) yet never "verified history".
  check(Outbox.passable(statusOf(s, dInvalid)) and not Outbox.inVerifiedHistory(statusOf(s, dInvalid)), "Invalid is passable for the cursor but NOT verified delivery");
  check(not Outbox.passable(statusOf(s, openD)) and not Outbox.inVerifiedHistory(statusOf(s, openD)), "Verified-unacknowledged blocks the cursor and is not verified delivery");
  // a corrected descriptor for the Invalid id stays admissible and appears in intake until it is delivered
  check(submitOk(s, dInvalid, 4) == 8, "a corrected descriptor for the Invalid event id is admitted as a new entry (seq 8)");
  check(seqs(Outbox.intake(s, null, 100)) == [8, 4, 3, 2, 1] and seqs(Outbox.verifiedHistory(s, null, 100)) == [7, 6, 5], "...it joins the intake page, the public history is unchanged");
};

if (failures > 0) { Debug.trap("outbox.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("outbox.test.mo: ALL PASS");
