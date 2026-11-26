// Standalone tests for Accounts: the small Anchor/Borsh decoders (Pool, Ticket,
// CloseRecord, Backer, RegistryPage, SnapshotPage) and the progress marker.
// Fixtures are serialized by test/gen/fixtures.py from the program IDL with an
// independent Borsh serializer (distinct, wide values in every field; Borsh
// Options both Some and None so every later field shifts).
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/accounts.test.mo

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat8 "mo:base/Nat8";

import Accounts "../Accounts";
import Fx "fixtures";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};
func flipFirst(b : Blob) : Blob {
  let a = Blob.toArray(b);
  Blob.fromArray(Array.tabulate<Nat8>(a.size(), func(i : Nat) : Nat8 { if (i == 0) { a[0] ^ 0xFF } else { a[i] } }));
};
func truncate(b : Blob, n : Nat) : Blob {
  let a = Blob.toArray(b);
  Blob.fromArray(Array.tabulate<Nat8>(n, func(i : Nat) : Nat8 { a[i] }));
};
func padded(b : Blob, extra : Nat) : Blob {
  let a = Blob.toArray(b);
  Blob.fromArray(Array.tabulate<Nat8>(a.size() + extra, func(i : Nat) : Nat8 { if (i < a.size()) { a[i] } else { 0 } }));
};

do {
  Debug.print("== decoders ==");
  check(Accounts.decodePool(Fx.pool1.bytes) == ?Fx.pool1.expect, "Pool with current_close_id = Some decodes to every expected field");
  check(Accounts.decodePool(Fx.pool2.bytes) == ?Fx.pool2.expect, "Pool with current_close_id = None decodes (activeTicket 0)");
  check(Accounts.decodeTicket(Fx.ticket1.bytes) == ?Fx.ticket1.expect, "Ticket with close_id = Some decodes (fields after the Option shift by 8)");
  check(Accounts.decodeTicket(Fx.ticket2.bytes) == ?Fx.ticket2.expect, "Ticket with close_id = None decodes");
  check(Accounts.decodeCloseRecord(Fx.close1.bytes) == ?Fx.close1.expect, "CloseRecord: Some(closing_ticket), promotion Pending");
  check(Accounts.decodeCloseRecord(Fx.close2.bytes) == ?Fx.close2.expect, "CloseRecord: None closing_ticket, promotion Applied{winner, entitlement}, finalized, reserve settled");
  check(Accounts.decodeCloseRecord(Fx.close3.bytes) == ?Fx.close3.expect, "CloseRecord: promotion Skipped{reason, stage}");
  check(Accounts.decodeBacker(Fx.backer1.bytes) == ?Fx.backer1.expect, "Backer decodes (u128 entitlement/lifetime beyond u64)");

  switch (Accounts.decodeRegistryPage(Fx.registryPage.bytes)) {
    case (?p) { check(p.index == Fx.registryPage.index and p.keys == Fx.registryPage.keys, "RegistryPage: index and the eight keys") };
    case (null) { check(false, "RegistryPage decodes") };
  };
  switch (Accounts.decodeSnapshotPage(Fx.snapshotPage.bytes)) {
    case (?p) { check(p.index == Fx.snapshotPage.index and p.ticketId == Fx.snapshotPage.ticketId and p.rows == Fx.snapshotPage.rows, "SnapshotPage: index, ticket tag and the eight rows (key, series, u128 remaining, slot)") };
    case (null) { check(false, "SnapshotPage decodes") };
  };
};

do {
  Debug.print("== rejection ==");
  check(Accounts.decodePool(flipFirst(Fx.pool1.bytes)) == null, "a wrong discriminator is rejected");
  check(Accounts.decodePool(Fx.ticket1.bytes) == null and Accounts.decodeTicket(Fx.pool1.bytes) == null, "an account of another type is rejected");
  check(Accounts.decodeCloseRecord(Fx.pool1.bytes) == null and Accounts.decodeBacker(Fx.close1.bytes) == null, "cross-type decode is rejected");
  check(Accounts.decodePool(Blob.fromArray([])) == null, "empty data is rejected");
  check(Accounts.decodePool(truncate(Fx.pool1.bytes, 100)) == null, "truncated Pool is rejected, not read past the end");
  check(Accounts.decodeTicket(truncate(Fx.ticket1.bytes, 30)) == null, "truncated Ticket is rejected");
  check(Accounts.decodeCloseRecord(truncate(Fx.close1.bytes, 200)) == null, "truncated CloseRecord is rejected");
  check(Accounts.decodeRegistryPage(truncate(Fx.registryPage.bytes, 100)) == null, "truncated RegistryPage is rejected");
  check(Accounts.decodeSnapshotPage(truncate(Fx.snapshotPage.bytes, 300)) == null, "truncated SnapshotPage is rejected");
  check(Accounts.decodePool(padded(Fx.pool1.bytes, 64)) == ?Fx.pool1.expect, "trailing zero padding (allocated space) is fine");
  check(Accounts.decodeCloseRecord(padded(Fx.close2.bytes, 9)) == ?Fx.close2.expect, "CloseRecord allocated at its maximum size decodes even when the serialized form is shorter");
};

do {
  Debug.print("== backer remaining + discriminators ==");
  let b = Fx.backer1.expect;
  check(Accounts.remaining({ b with entitlement = (2 ** 80) + 1_000; lifetime = (2 ** 80) + 400 }) == 600, "remaining = entitlement - lifetime (beyond u64)");
  check(Accounts.remaining(b) == (if (b.entitlement > b.lifetime) { b.entitlement - b.lifetime } else { 0 }), "remaining of the decoded fixture");
  check(Accounts.remaining({ b with entitlement = 5; lifetime = 9 }) == 0, "remaining never goes negative");
  check(Accounts.CUTOFF == 10, "the open-registry cutoff is 10 lamports");
};

do {
  Debug.print("== progress marker ==");
  let pool0 = Fx.pool2.expect; // no close, activeTicket = 0
  check(not Accounts.workPending(pool0, null, null), "no open ticket and no close: nothing pending");
  check(Accounts.marker(pool0, null, null) == Accounts.marker(pool0, null, null), "the marker is deterministic");

  let pool1 : Accounts.Pool = { Fx.pool1.expect with activeTicket = (3 : Nat64); currentCloseId = (null : ?Nat64) };
  let t : Accounts.Ticket = { Fx.ticket1.expect with id = (3 : Nat64); phase = (0 : Nat8); cursor = (0 : Nat64); closeId = (null : ?Nat64) };
  check(Accounts.workPending(pool1, ?t, null), "an open ticket is pending work");
  let m0 = Accounts.marker(pool1, ?t, null);
  check(m0 != Accounts.marker(pool1, ?{ t with cursor = (8 : Nat64) }, null), "the ticket cursor moving is progress");
  check(m0 != Accounts.marker(pool1, ?{ t with phase = (1 : Nat8) }, null), "the ticket phase moving is progress");
  check(m0 != Accounts.marker({ pool1 with roundId = pool1.roundId + 1 }, ?t, null), "the round id moving is progress");
  check(m0 == Accounts.marker({ pool1 with pot = pool1.pot + 1; seed = (7 : Nat64) }, ?t, null), "unrelated money fields are not progress markers");

  let poolC : Accounts.Pool = { pool1 with activeTicket = (0 : Nat64); currentCloseId = (?4 : ?Nat64) };
  let c : Accounts.CloseRecord = { Fx.close1.expect with closeId = (4 : Nat64); promotion = #Pending; finalized = false; cashCommitted = false; sealed = false; drawState = (0 : Nat8); candidateCursor = (0 : Nat64) };
  check(Accounts.workPending(poolC, null, ?c), "a close in progress is pending work");
  check(not Accounts.workPending(poolC, null, ?{ c with finalized = true }), "a finalized close is done");
  let mc = Accounts.marker(poolC, null, ?c);
  check(mc != Accounts.marker(poolC, null, ?{ c with cashCommitted = true }), "cash commit is progress");
  check(mc != Accounts.marker(poolC, null, ?{ c with candidateCursor = (8 : Nat64) }), "candidate cursor is progress");
  check(mc != Accounts.marker(poolC, null, ?{ c with sealed = true }), "sealing candidates is progress");
  check(mc != Accounts.marker(poolC, null, ?{ c with drawState = (1 : Nat8) }), "the draw state is progress");
  check(mc != Accounts.marker(poolC, null, ?{ c with promotion = #Skipped({ reason = (1 : Nat8); stage = (1 : Nat8) }) }), "a terminal promotion is progress");
  check(mc != Accounts.marker(poolC, null, ?{ c with reserveSettled = true }), "settling the reserve is progress");
  check(mc != Accounts.marker({ poolC with currentCloseId = (?5 : ?Nat64) }, null, ?c), "a different close id is progress");
  check(Accounts.workPending(poolC, null, null), "a close the keeper could not load still counts as pending work (the stall must stay visible)");
};

if (failures > 0) { Debug.trap("accounts.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("accounts.test.mo: ALL PASS");
