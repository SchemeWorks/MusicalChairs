// Standalone tests for Journal: the bounded transaction journal, the
// one-in-flight submit rule, the identical-bytes re-broadcast rule, block-height
// expiry and the unknown-outcome discipline.
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/journal.test.mo

import Blob "mo:base/Blob";
import Debug "mo:base/Debug";

import Journal "../Journal";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};

let S : Nat = 1_000_000_000;

func entry(sig : Text, position : Text, rows : Nat, status : Journal.Status, at : Nat, lastValid : Nat64) : Journal.Entry {
  {
    signature = sig;
    blockhash = "bh-" # sig;
    lastValidBlockHeight = lastValid;
    step = position # " rows=" # debug_show (rows);
    position;
    rows;
    raw = Blob.fromArray([1, 2, 3, 4, 5]);
    status;
    at;
    updatedAt = at;
    note = "";
  };
};
func statusOf(j : Journal.Journal, sig : Text) : ?Journal.Status {
  for (e in j.vals()) { if (e.signature == sig) { return ?e.status } };
  null;
};
func isStatus(j : Journal.Journal, sig : Text, s : Journal.Status) : Bool {
  switch (statusOf(j, sig)) { case (?x) { x == s }; case (null) { false } };
};
func isYes(m : Journal.May) : Bool { switch (m) { case (#Yes(_)) true; case (_) false } };
func isNo(m : Journal.May) : Bool { switch (m) { case (#No(_)) true; case (_) false } };
func isFallback(m : Journal.May) : Bool { switch (m) { case (#Yes({ fallback })) fallback; case (_) false } };

// =====================================================================
// does_not_resubmit_after_unknown_outcome_until_status_read
// =====================================================================
do {
  Debug.print("== does_not_resubmit_after_unknown_outcome_until_status_read ==");
  let pos = "allocate_rows|t5|c16";
  var j = Journal.append(Journal.empty(), entry("sigA", pos, 8, #Unknown, 100 * S, 500), 16);
  check(Journal.hasUnknown(j), "a send that errored after submit is journaled #Unknown");
  check(isNo(Journal.maySubmit(j, pos, 3)), "the same position may not be submitted while an unknown entry is unresolved");
  check(isNo(Journal.maySubmit(j, "commit_ticket|t5|c0", 3)), "no other step may be submitted either: one transaction in flight at a time");

  // A status read that does not find the signature leaves it unknown.
  let r0 = Journal.applyStatus(j, "sigA", #NotFound, 130 * S);
  check(isStatus(r0.journal, "sigA", #Unknown) and not r0.landed, "a status read that finds nothing leaves the entry #Unknown");
  check(isNo(Journal.maySubmit(r0.journal, pos, 3)), "still no resubmission after an empty status read");

  // A status read that finds it resolves the unknown.
  let r1 = Journal.applyStatus(j, "sigA", #Finalized, 140 * S);
  check(isStatus(r1.journal, "sigA", #Finalized) and r1.landed, "a status read answering finalized resolves the entry and reports a landing");
  check(not Journal.hasUnknown(r1.journal), "no unknown entry is left");
  check(isYes(Journal.maySubmit(r1.journal, "commit_ticket|t5|c0", 3)), "the next step may proceed once the status was read");
};

// =====================================================================
// identical-bytes re-broadcast rule
// =====================================================================
do {
  Debug.print("== rebroadcast_uses_identical_bytes_while_blockhash_valid ==");
  let j = Journal.append(Journal.empty(), entry("sigB", "discover_rows|t2|c0", 8, #Unknown, 10 * S, 1_000), 16);
  switch (Journal.rebroadcast(j, 999)) {
    case (?e) { check(e.signature == "sigB" and e.raw == Blob.fromArray([1, 2, 3, 4, 5]), "an unknown entry inside its validity window is re-broadcast with the SAME signature and bytes") };
    case (null) { check(false, "rebroadcast candidate expected") };
  };
  check(Journal.rebroadcast(j, 1_000) != null, "valid through lastValidBlockHeight inclusive");
  check(Journal.rebroadcast(j, 1_001) == null, "past lastValidBlockHeight nothing may be re-broadcast");
  let confirmed = Journal.append(Journal.empty(), entry("sigC", "p", 1, #Finalized, 10 * S, 1_000), 16);
  check(Journal.rebroadcast(confirmed, 5) == null, "a finalized entry is never re-broadcast");
  let submitted = Journal.append(Journal.empty(), entry("sigD", "p", 1, #Submitted, 10 * S, 1_000), 16);
  check(Journal.rebroadcast(submitted, 5) == null, "a #Submitted entry (the send was acknowledged) is not re-broadcast, only unknown ones are");
};

// =====================================================================
// expiry by block height
// =====================================================================
do {
  Debug.print("== expiry_by_block_height ==");
  var j = Journal.append(Journal.empty(), entry("e1", "p1", 8, #Submitted, 10 * S, 1_000), 16);
  j := Journal.append(j, entry("e2", "p2", 8, #Unknown, 11 * S, 2_000), 16);
  j := Journal.append(j, entry("e3", "p3", 8, #Finalized, 12 * S, 100), 16);
  let notYet = Journal.expireUnseen(j, 1_000, [], 20 * S);
  check(notYet.expired == 0, "height == lastValidBlockHeight is still valid: nothing expires");
  let one = Journal.expireUnseen(j, 1_001, [], 20 * S);
  check(one.expired == 1 and isStatus(one.journal, "e1", #Expired), "height past lastValid expires the unseen #Submitted entry");
  check(isStatus(one.journal, "e2", #Unknown), "an entry still inside its window is untouched");
  check(isStatus(one.journal, "e3", #Finalized), "a finalized entry never expires");
  let seen = Journal.expireUnseen(j, 1_001, ["e1"], 20 * S);
  check(seen.expired == 0 and isStatus(seen.journal, "e1", #Submitted), "an entry the status read has seen is never expired by height");
  let both = Journal.expireUnseen(j, 5_000, [], 20 * S);
  check(both.expired == 2 and not Journal.hasUnknown(both.journal) and Journal.unresolved(both.journal).size() == 0, "expiry resolves unknown entries so the position can be planned again");
};

// =====================================================================
// attempts per position: one-row fallback then block
// =====================================================================
do {
  Debug.print("== failed_position_falls_back_to_one_row_then_blocks ==");
  let pos = "prune_rows|t9|c24";
  let fresh = Journal.empty();
  check(isYes(Journal.maySubmit(fresh, pos, 3)) and not isFallback(Journal.maySubmit(fresh, pos, 3)), "a clean position starts with the full batch");
  var j = Journal.append(fresh, entry("f1", pos, 8, #Failed, 10 * S, 100), 16);
  check(isFallback(Journal.maySubmit(j, pos, 3)), "after one failure the same position falls back to a single row");
  check(not isFallback(Journal.maySubmit(j, "other|t9|c1", 3)), "another position is unaffected");
  j := Journal.append(j, entry("f2", pos, 1, #Expired, 20 * S, 200), 16);
  check(isYes(Journal.maySubmit(j, pos, 3)) and isFallback(Journal.maySubmit(j, pos, 3)), "an expiry counts as an attempt and stays on one row");
  j := Journal.append(j, entry("f3", pos, 1, #Failed, 30 * S, 300), 16);
  switch (Journal.maySubmit(j, pos, 3)) {
    case (#No(m)) { check(m == "position failed 3 times: operator action needed", "after maxAttempts failures the position is blocked: " # m) };
    case (_) { check(false, "the position must be blocked after 3 failures") };
  };
  check(isNo(Journal.maySubmit(Journal.append(fresh, entry("u", "x", 1, #Submitted, 1, 9), 16), "y", 3)), "an in-flight #Submitted entry also serializes the keeper");
  let landed = Journal.append(Journal.empty(), entry("c1", pos, 8, #Confirmed, 1, 9), 16);
  check(isNo(Journal.maySubmit(landed, pos, 3)), "a #Confirmed (not yet finalized) entry is still unresolved");
};

// =====================================================================
// status transitions are monotone and terminal states are sticky
// =====================================================================
do {
  Debug.print("== status_transitions ==");
  let j0 = Journal.append(Journal.empty(), entry("s1", "p", 8, #Submitted, 1 * S, 100), 16);
  let a = Journal.applyStatus(j0, "s1", #Processed, 2 * S);
  check(isStatus(a.journal, "s1", #Submitted) and not a.landed, "processed keeps #Submitted (not landed yet)");
  let b = Journal.applyStatus(a.journal, "s1", #Confirmed, 3 * S);
  check(isStatus(b.journal, "s1", #Confirmed) and b.landed, "confirmed -> #Confirmed and counts as landed");
  let c = Journal.applyStatus(b.journal, "s1", #Finalized, 4 * S);
  check(isStatus(c.journal, "s1", #Finalized) and c.landed, "finalized -> #Finalized");
  let d = Journal.applyStatus(c.journal, "s1", #NotFound, 5 * S);
  check(isStatus(d.journal, "s1", #Finalized) and not d.landed, "a later NotFound never un-finalizes an entry");
  let e = Journal.applyStatus(c.journal, "s1", #Err("InstructionError"), 6 * S);
  check(isStatus(e.journal, "s1", #Finalized), "terminal states are sticky");

  let f = Journal.applyStatus(j0, "s1", #Err("custom program error: 0x1771"), 3 * S);
  check(isStatus(f.journal, "s1", #Failed) and not f.landed, "an execution error marks the entry #Failed");
  var noteOk = false;
  for (x in f.journal.vals()) { if (x.note == "custom program error: 0x1771") { noteOk := true } };
  check(noteOk, "the error text is kept as the entry's note");

  let u = Journal.markUnknown(j0, "s1", "InvalidHttpJsonRpcResponse", 3 * S);
  check(isStatus(u, "s1", #Unknown), "a send error after submit marks the entry #Unknown");
  let u1 = Journal.markUnknown(u, "s1", "second error text", 4 * S);
  var noteKept = false;
  for (x in u1.vals()) { if (x.note == "second error text" and x.status == #Unknown) { noteKept := true } };
  check(noteKept, "markUnknown on an already-unknown entry refreshes the error note");
  let u2 = Journal.markUnknown(c.journal, "s1", "late error", 9 * S);
  check(isStatus(u2, "s1", #Finalized), "markUnknown never downgrades a terminal entry");

  check(Journal.landedWithin(c.journal, 4 * S + 100 * S, 120 * S), "a landing within the window counts as ours");
  check(not Journal.landedWithin(c.journal, 4 * S + 121 * S, 120 * S), "a landing outside the window does not");
  check(not Journal.landedWithin(j0, 10 * S, 120 * S), "an unlanded entry is not a landing");
  check(Journal.lastSignature(c.journal) == ?"s1" and Journal.lastSignature(Journal.empty()) == null, "lastSignature");
};

// =====================================================================
// bounded size
// =====================================================================
do {
  Debug.print("== bounded_size_keeps_unresolved ==");
  var j = Journal.empty();
  var i : Nat = 0;
  while (i < 10) {
    j := Journal.append(j, entry("n" # debug_show (i), "p" # debug_show (i), 8, #Finalized, i * S, 100), 4);
    i += 1;
  };
  check(j.size() == 4, "the journal keeps only the last N entries");
  check(j[0].signature == "n6" and j[3].signature == "n9", "the newest entries survive, oldest first");

  // An unresolved entry is never evicted even when it is the oldest.
  var k = Journal.append(Journal.empty(), entry("keep", "p", 8, #Unknown, 1 * S, 100), 3);
  k := Journal.append(k, entry("a", "p1", 8, #Finalized, 2 * S, 100), 3);
  k := Journal.append(k, entry("b", "p2", 8, #Finalized, 3 * S, 100), 3);
  k := Journal.append(k, entry("c", "p3", 8, #Finalized, 4 * S, 100), 3);
  check(k.size() == 3 and isStatus(k, "keep", #Unknown), "an unresolved entry survives eviction");
  check(statusOf(k, "a") == null and isStatus(k, "c", #Finalized), "resolved entries are evicted oldest first");

  check(Journal.tail(j, 2).size() == 2 and Journal.tail(j, 2)[1].signature == "n9", "tail(n) returns the newest n, oldest first");
  check(Journal.tail(j, 99).size() == 4, "tail larger than the journal returns everything");
  check(Journal.append(Journal.empty(), entry("z", "p", 1, #Finalized, 1, 1), 0).size() == 1, "capacity 0 is treated as 1 (the newest entry is never dropped)");
};

if (failures > 0) { Debug.trap("journal.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("journal.test.mo: ALL PASS");
