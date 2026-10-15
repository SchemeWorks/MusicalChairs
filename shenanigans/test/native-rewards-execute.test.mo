// Standalone tests for NativeRewards leg execution: write-ahead SubmittedUnknown,
// identical-field retry, ledger-outcome classification, dedup-window quarantine,
// and sibling-leg isolation (spec section 9, step 3).
//
// Run: moc -r --package base <dfx-base> shenanigans/test/native-rewards-execute.test.mo

import Principal "mo:base/Principal";
import Debug "mo:base/Debug";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Int "mo:base/Int";
import Text "mo:base/Text";
import OrderedMap "mo:base/OrderedMap";
import NR "../NativeRewards";

// ---------------------------------------------------------------- fixtures

let SELF = Principal.fromText("j56tm-oaaaa-aaaac-qf34q-cai");
let alice = Principal.fromText("u7zgw-triai-opf3l-hs7dz-2bf2t-j554r-mnzkx-74hou-6drz5-nati3-bqe");
let bob = Principal.fromText("mi66c-zqlu4-4kxd6-2gtp7-szg5v-6a62a-geoty-fahu5-4trje-xyfby-wqe");
let carol = Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe");
let house = Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae");

let principalMap = OrderedMap.Make<Principal>(Principal.compare);

let NS : Nat = 1_000_000_000;
let T0 : Int = 1_800_000_000 * NS;
let TBIND : Int = T0 + NS; // legs are stamped created_at_time = TBIND
let WINDOW : Int = 86_400 * NS;
let MARGIN : Int = NR.DEDUP_SAFETY_MARGIN_NS;
let LAMPORTS : Nat64 = 1_000_000_000;
let PUBKEY_A = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

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

func eventId(sig : Text) : NR.EventId = {
  cluster_domain = Text.encodeUtf8("solana-localnet");
  program_id = "MCprog1111111111111111111111111111111111111";
  signature = sig;
  instruction_index = 0;
  inner_instruction_index = 0;
  event_schema_version = 1;
};

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

// A reserved obligation with the four-leg player chain (alice's gift already
// claimed by the legacy observer): player 5.4e11, bob 3e10, house 1.5e10, house residual 1.5e10.
func reserved() : (NR.State, NR.EventId) {
  let id = eventId("sigExec");
  let ev : NR.NativeEvent = {
    event_id = id; source_round_id = 3; source_close_id = null; kind = #PositionOpened;
    beneficiary_pubkey = PUBKEY_A; basis_lamports = LAMPORTS; plan = ?#Simple;
  };
  let s0 = switch (NR.appendPolicy(NR.emptyState(), draft(), T0)) { case (#Ok r) { r.state }; case (#Err e) { Debug.trap(e) } };
  let (s1, _) = NR.observe(s0, ev, T0);
  let markers = principalMap.put(principalMap.empty<Int>(), alice, T0 - 1000);
  let out = NR.applyBind(s1, { self = SELF; signupMarkers = markers; giftBook = principalMap.empty<NR.GiftReservation>(); effects = noEffects }, id, ?alice, TBIND);
  (out.state, id);
};

func legOf(s : NR.State, id : NR.EventId, i : Nat) : NR.Leg {
  switch (NR.getObligation(s, id)) { case (?o) { o.legs[i] }; case null { Debug.trap("no obligation") } };
};
func statusOf(s : NR.State, id : NR.EventId) : NR.ObligationStatus {
  switch (NR.getObligation(s, id)) { case (?o) { o.status }; case null { Debug.trap("no obligation") } };
};

func submit(s : NR.State, id : NR.EventId, i : Nat, now : Int) : (NR.State, ?NR.TransferArg) {
  let (s2, r) = NR.beginExecute(s, id, i, now);
  switch (r) { case (#Submit a) { (s2, ?a) }; case _ { (s2, null) } };
};
func retry(s : NR.State, id : NR.EventId, i : Nat, now : Int) : (NR.State, NR.BeginResult) = NR.beginRetry(s, id, i, now);

func outcome(s : NR.State, id : NR.EventId, i : Nat, o : NR.TransferOutcome, now : Int) : (NR.State, NR.OutcomeResult) =
  NR.applyTransferOutcome(s, id, i, o, now);

func argMatchesLeg(a : NR.TransferArg, l : NR.Leg) : Bool =
  a.to == l.recipient and a.amount == l.amount and a.memo == l.memo and a.created_at_time == l.created_at_time;

func isQuarantined(st : NR.LegStatus, recoverable : Bool, needle : Text) : Bool {
  switch (st) { case (#Quarantined q) { q.recoverable == recoverable and Text.contains(q.reason, #text needle) }; case _ { false } };
};

// ---------------------------------------------------------------- tests

Debug.print("== executeWritesSubmittedUnknownBeforeAwait");
do {
  let (s, id) = reserved();
  check(legOf(s, id, 0).status == #Pending, "freshly reserved leg is #Pending");
  let (s1, a1) = submit(s, id, 0, TBIND + 10 * NS);
  // beginExecute is the step the actor commits BEFORE awaiting icrc1_transfer
  check(legOf(s1, id, 0).status == #SubmittedUnknown, "after beginExecute (before any transfer outcome) the leg is #SubmittedUnknown, not #Pending");
  check(legOf(s1, id, 0).attempts == 1, "the submission is counted before the await");
  check(switch (a1) { case (?a) { argMatchesLeg(a, legOf(s, id, 0)) }; case null { false } }, "the transfer args are exactly the frozen leg fields");
  check(statusOf(s1, id) == #Reserved, "obligation stays #Reserved while a leg is in flight");
  // trap injected after the transfer: no outcome is applied; the next call re-presents identical fields
  let (s2, a2) = submit(s1, id, 0, TBIND + 20 * NS);
  check(legOf(s2, id, 0).status == #SubmittedUnknown, "still #SubmittedUnknown after a lost outcome");
  check(switch (a1, a2) { case (?x, ?y) { x == y }; case _ { false } }, "re-presentation after a lost outcome uses IDENTICAL fields");
  // other legs untouched
  check(legOf(s2, id, 1).status == #Pending and legOf(s2, id, 1).attempts == 0, "sibling legs are untouched");
};

Debug.print("== retryRepeatsIdenticalFieldsAndDuplicateReconciles");
do {
  let (s, id) = reserved();
  let (s1, a1) = submit(s, id, 0, TBIND + 10 * NS);
  let (s2, r1) = outcome(s1, id, 0, #CallFailed("timeout"), TBIND + 11 * NS);
  check(r1.status == #SubmittedUnknown and not r1.newlyCommitted, "a trapped/timed-out call leaves #SubmittedUnknown");
  let (s3, b) = retry(s2, id, 0, TBIND + 60 * NS);
  let a2 = switch (b) { case (#Submit a) { ?a }; case _ { null } };
  check(a1 != null and a1 == a2, "retry re-presents identical fields (memo, created_at_time, recipient, amount)");
  let (s4, r2) = outcome(s3, id, 0, #Err(#Duplicate({ duplicate_of = 77 })), TBIND + 61 * NS);
  check(r2.status == #Committed({ block = 77 }) and r2.newlyCommitted, "Duplicate{duplicate_of} reconciles to Committed{duplicate_of}");
  check(legOf(s4, id, 0).status == #Committed({ block = 77 }), "stored leg is Committed{77}");
  // a committed leg is never re-presented
  let (_, again) = NR.beginExecute(s4, id, 0, TBIND + 70 * NS);
  check(switch (again) { case (#Skip x) { x.status == #Committed({ block = 77 }) }; case _ { false } }, "executing a committed leg is a no-op skip");
  let (_, again2) = retry(s4, id, 0, TBIND + 70 * NS);
  check(switch (again2) { case (#Skip _) { true }; case _ { false } }, "retrying a committed leg is a no-op skip");
  // Ok after Committed does not rewrite the block
  let (s5, r3) = outcome(s4, id, 0, #Ok(99), TBIND + 80 * NS);
  check(r3.status == #Committed({ block = 77 }) and not r3.newlyCommitted and legOf(s5, id, 0).status == #Committed({ block = 77 }), "a late Ok never rewrites an existing Committed block");
};

Debug.print("== definiteRejectionIsRetryableThenFundingCorrectionAndIdenticalRetryCommits");
do {
  let errs : [(Text, NR.TransferError)] = [
    ("TemporarilyUnavailable", #TemporarilyUnavailable),
    ("BadFee", #BadFee({ expected_fee = 10_000 })),
    ("InsufficientFunds", #InsufficientFunds({ balance = 0 })),
    ("GenericError", #GenericError({ error_code = 5; message = "anything transient" })),
  ];
  for ((name, e) in errs.vals()) {
    let (s, id) = reserved();
    let (s1, a1) = submit(s, id, 1, TBIND + 10 * NS);
    let (s2, r1) = outcome(s1, id, 1, #Err(e), TBIND + 11 * NS);
    check(switch (r1.status) { case (#Retryable x) { x.attempts == 1 and Text.contains(x.reason, #text name) }; case _ { false } }, name # " -> Retryable{reason, attempts = 1}");
    check(statusOf(s2, id) == #Reserved, name # ": obligation stays #Reserved");
    // execute (first-presentation path) does not touch a Retryable leg; retry is the entry point
    let (_, viaExec) = NR.beginExecute(s2, id, 1, TBIND + 12 * NS);
    check(switch (viaExec) { case (#Skip _) { true }; case _ { false } }, name # ": executeNativeRewardLeg does not re-present a Retryable leg");
    // funding / config correction happens off-line; identical retry inside the window
    let (s3, b) = retry(s2, id, 1, TBIND + 3_600 * NS);
    let a2 = switch (b) { case (#Submit a) { ?a }; case _ { null } };
    check(a1 != null and a1 == a2, name # ": retry re-presents the identical frozen fields");
    check(legOf(s3, id, 1).status == #SubmittedUnknown and legOf(s3, id, 1).attempts == 2, name # ": retry writes #SubmittedUnknown first and counts attempt 2");
    let (_s4, r2) = outcome(s3, id, 1, #Ok(500), TBIND + 3_601 * NS);
    check(r2.status == #Committed({ block = 500 }) and r2.newlyCommitted, name # ": identical retry commits after the correction");
    // second failure increments attempts
    let (s5, _) = submit(s, id, 2, TBIND + 10 * NS);
    let (s6, _) = outcome(s5, id, 2, #Err(e), TBIND + 11 * NS);
    let (s7, _) = retry(s6, id, 2, TBIND + 20 * NS);
    let (_, r3) = outcome(s7, id, 2, #Err(e), TBIND + 21 * NS);
    check(switch (r3.status) { case (#Retryable x) { x.attempts == 2 }; case _ { false } }, name # ": attempts counts every submission");
  };
};

Debug.print("== retryOutsideWindowQuarantinesForReconcile");
do {
  let (s, id) = reserved();
  let (s1, _) = submit(s, id, 0, TBIND + 10 * NS);
  let (s2, _) = outcome(s1, id, 0, #Err(#TemporarilyUnavailable), TBIND + 11 * NS);
  // exactly at the edge of (window - margin): still allowed
  let (_, edge) = retry(s2, id, 0, TBIND + WINDOW - MARGIN);
  check(switch (edge) { case (#Submit _) { true }; case _ { false } }, "retry exactly at (window - safety margin) is still inside the window");
  // one nanosecond later: quarantined, no transfer
  let (s3, late) = retry(s2, id, 0, TBIND + WINDOW - MARGIN + 1);
  check(switch (late) { case (#Expired st) { isQuarantined(st, true, "reconcile") }; case _ { false } }, "retry past the dedup window -> Quarantined{reconcile, recoverable}");
  check(isQuarantined(legOf(s3, id, 0).status, true, "reconcile"), "the leg is stored as Quarantined{recoverable = true}");
  let (_, noMore) = retry(s3, id, 0, TBIND + WINDOW + 100 * NS);
  check(switch (noMore) { case (#Skip x) { isQuarantined(x.status, true, "") }; case _ { false } }, "a quarantined leg cannot be retried or re-stamped");
  // a SubmittedUnknown leg past the window also quarantines
  let (s4, _) = submit(s, id, 1, TBIND + 10 * NS);
  let (_s5, late2) = NR.beginExecute(s4, id, 1, TBIND + WINDOW + NS);
  check(switch (late2) { case (#Expired st) { isQuarantined(st, true, "reconcile") }; case _ { false } }, "SubmittedUnknown past the window -> Quarantined{reconcile}");
  // a never-submitted Pending leg past the window also quarantines (frozen timestamp is never refreshed)
  let (s6, late3) = NR.beginExecute(s, id, 2, TBIND + WINDOW + NS);
  check(switch (late3) { case (#Expired st) { isQuarantined(st, true, "reconcile") }; case _ { false } }, "Pending leg first presented past the window -> Quarantined{reconcile}, never a fresh timestamp");
  check(legOf(s6, id, 2).created_at_time == legOf(s, id, 2).created_at_time, "created_at_time is never changed");
  // outcome arriving past the window: a definite rejection can no longer be re-presented
  let (s7, _) = submit(s, id, 3, TBIND + WINDOW - MARGIN - 10 * NS);
  let (_, late4) = outcome(s7, id, 3, #Err(#BadFee({ expected_fee = 1 })), TBIND + WINDOW);
  check(isQuarantined(late4.status, true, "reconcile"), "a rejection that returns past the window -> Quarantined{reconcile} instead of Retryable");
  // but an Ok that returns past the window is still a commit
  let (s8, _) = submit(s, id, 3, TBIND + WINDOW - MARGIN - 10 * NS);
  let (_, late5) = outcome(s8, id, 3, #Ok(42), TBIND + WINDOW + 5 * NS);
  check(late5.status == #Committed({ block = 42 }), "an Ok outcome is a commit regardless of the window");
};

Debug.print("== permanentImmutableFieldErrorQuarantinesWithReason");
do {
  let (s, id) = reserved();
  let (s1, _) = submit(s, id, 0, TBIND + 10 * NS);
  let (s2, r1) = outcome(s1, id, 0, #Err(#BadBurn({ min_burn_amount = 100 })), TBIND + 11 * NS);
  check(isQuarantined(r1.status, false, "BadBurn"), "BadBurn -> Quarantined{permanent, reason names BadBurn, recoverable = false}");
  let (s3, _) = submit(s2, id, 1, TBIND + 12 * NS);
  let (s4, r2) = outcome(s3, id, 1, #Err(#GenericError({ error_code = 1; message = "The memo field is too large." })), TBIND + 13 * NS);
  check(isQuarantined(r2.status, false, "memo"), "a ledger GenericError naming the memo -> Quarantined{permanent}");
  let (_, rr) = retry(s4, id, 0, TBIND + 20 * NS);
  check(switch (rr) { case (#Skip x) { isQuarantined(x.status, false, "BadBurn") }; case _ { false } }, "a permanent quarantine is never retried");
  let (_, ex) = NR.beginExecute(s4, id, 1, TBIND + 20 * NS);
  check(switch (ex) { case (#Skip x) { isQuarantined(x.status, false, "memo") }; case _ { false } }, "a permanent quarantine is never re-executed");
  check(legOf(s4, id, 0).created_at_time == legOf(s, id, 0).created_at_time and legOf(s4, id, 0).memo == legOf(s, id, 0).memo and legOf(s4, id, 0).recipient == legOf(s, id, 0).recipient, "frozen fields untouched by quarantine (no replacement timestamp or recipient)");
};

// TooOld / CreatedInFuture quarantine for ledger-history reconciliation
do {
  let (s, id) = reserved();
  let (s1, _) = submit(s, id, 0, TBIND + 10 * NS);
  let (s2, r1) = outcome(s1, id, 0, #Err(#TooOld), TBIND + 11 * NS);
  check(isQuarantined(r1.status, true, "reconcile"), "TooOld -> Quarantined{reconcile, recoverable = true}");
  let (s3, _) = submit(s2, id, 1, TBIND + 12 * NS);
  let (_, r2) = outcome(s3, id, 1, #Err(#CreatedInFuture({ ledger_time = 5 })), TBIND + 13 * NS);
  check(isQuarantined(r2.status, true, "reconcile"), "CreatedInFuture -> Quarantined{reconcile, recoverable = true}");
  let (_, rr) = retry(s2, id, 0, TBIND + 14 * NS);
  check(switch (rr) { case (#Skip _) { true }; case _ { false } }, "a recoverable quarantine is not re-presented to the ledger; only reconcileFromLedgerBlock can complete it");
};

Debug.print("== failedSiblingLegDoesNotRepeatOrStrandCommittedLegs");
do {
  let (s, id) = reserved();
  var st = s;
  func run(i : Nat, o : NR.TransferOutcome) {
    let (s1, _) = NR.beginExecute(st, id, i, TBIND + 10 * NS);
    let (s2, _) = NR.applyTransferOutcome(s1, id, i, o, TBIND + 11 * NS);
    st := s2;
  };
  run(0, #Ok(10));
  check(statusOf(st, id) == #Reserved, "obligation is #Reserved while legs remain");
  run(1, #Err(#BadBurn({ min_burn_amount = 1 })));
  run(2, #Ok(11));
  check(statusOf(st, id) == #Reserved, "obligation is still #Reserved while leg 3 is Pending");
  run(3, #Ok(12));
  check(statusOf(st, id) == #PartiallyQuarantined, "all legs terminal with one quarantined -> #PartiallyQuarantined");
  check(legOf(st, id, 0).status == #Committed({ block = 10 }) and legOf(st, id, 2).status == #Committed({ block = 11 }) and legOf(st, id, 3).status == #Committed({ block = 12 }), "the quarantined sibling did not disturb committed legs");
  // re-running execute on every leg repeats nothing
  var repeated = false;
  var i = 0;
  while (i < 4) {
    let (s1, r) = NR.beginExecute(st, id, i, TBIND + 100 * NS);
    switch (r) { case (#Submit _) { repeated := true }; case _ {} };
    st := s1;
    i += 1;
  };
  check(not repeated, "no leg (committed or quarantined) is ever re-submitted");
  check(legOf(st, id, 0).attempts == 1 and legOf(st, id, 2).attempts == 1, "committed legs were submitted exactly once");

  // a Retryable sibling keeps the obligation open and later completes it
  let (s0b, idb) = reserved();
  var sb = s0b;
  func runB(i : Nat, o : NR.TransferOutcome) {
    let (s1, _) = NR.beginExecute(sb, idb, i, TBIND + 10 * NS);
    let (s2, _) = NR.applyTransferOutcome(s1, idb, i, o, TBIND + 11 * NS);
    sb := s2;
  };
  runB(0, #Ok(1)); runB(1, #Err(#InsufficientFunds({ balance = 0 }))); runB(2, #Ok(3)); runB(3, #Ok(4));
  check(statusOf(sb, idb) == #Reserved, "a Retryable sibling keeps the obligation #Reserved (not Completed)");
  let (s9, b) = NR.beginRetry(sb, idb, 1, TBIND + 600 * NS);
  check(switch (b) { case (#Submit _) { true }; case _ { false } }, "the retryable sibling alone is retried");
  let (s10, _) = NR.applyTransferOutcome(s9, idb, 1, #Ok(5), TBIND + 601 * NS);
  check(statusOf(s10, idb) == #Completed, "obligation is #Completed once every leg is Committed");
  check(legOf(s10, idb, 0).attempts == 1 and legOf(s10, idb, 2).attempts == 1 and legOf(s10, idb, 3).attempts == 1, "retrying one leg never re-submits the committed legs");
};

// guards: unknown legs / unreserved obligation
do {
  let (s, id) = reserved();
  let (_, r1) = NR.beginExecute(s, id, 9, TBIND);
  check(r1 == #UnknownLeg, "unknown leg index -> #UnknownLeg");
  let (_, r2) = NR.beginExecute(s, eventId("nope"), 0, TBIND);
  check(r2 == #NotReserved, "unknown event id -> #NotReserved");
  // an Observed (unbound) obligation has no legs and cannot execute
  let ev2 : NR.NativeEvent = { event_id = eventId("sigObs"); source_round_id = 1; source_close_id = null; kind = #SeriesAFunded; beneficiary_pubkey = PUBKEY_A; basis_lamports = 1_000_000; plan = null };
  let (s2, _) = NR.observe(s, ev2, T0);
  let (_, r3) = NR.beginExecute(s2, ev2.event_id, 0, TBIND);
  check(r3 == #NotReserved, "an unbound #Observed obligation cannot execute");
  let (_, r4) = outcome(s, id, 0, #Err(#TemporarilyUnavailable), TBIND);
  check(r4.status == #Pending and not r4.newlyCommitted, "a failure outcome for a leg that is not in flight is ignored (no regression of status)");
  let (_, r5) = outcome(s, id, 0, #Ok(1), TBIND);
  check(r5.status == #Committed({ block = 1 }) and r5.newlyCommitted, "a ledger Ok is proof of commit even if the SubmittedUnknown write was somehow lost");
};

if (ok) { Debug.print("ALL PASS (" # Nat.toText(passed) # " checks)") } else { Debug.trap("TEST FAILURES") };
