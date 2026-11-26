// Standalone tests for Planner: the pure next-step planner (ticket phases ->
// closing pipeline). Goldens: the real localnet transactions in
// solana/fixtures/rpc, built by the TypeScript adapter - the planner must emit the
// same instruction data and account order for equivalent inputs. Behaviour tests
// pin batch sizing (cap 8, aligned to page edges, one-row fallback), the Prune
// swap-delete simulation (including the chain cut) and the closing decisions.
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/planner.test.mo

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";

import Accounts "../Accounts";
import Base58 "../Base58";
import Pda "../Pda";
import Planner "../Planner";
import Tx "../Tx";
import Fx "fixtures";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};
func b58(t : Text) : Blob { switch (Base58.decode(t)) { case (?b) { b }; case (null) { Debug.trap("bad base58 " # t) } } };
func key(b : Nat8) : Blob { Blob.fromArray(Array.tabulate<Nat8>(32, func _ { b })) };

let programId = b58(Fx.PROGRAM);
let a = Pda.addresses(programId);

// ---------------------------------------------------------------------
// captured goldens
// ---------------------------------------------------------------------
func captured(step : Text) : Fx.Captured {
  for (c in Fx.captured.vals()) { if (c.step == step) { return c } };
  Debug.trap("no capture for " # step);
};
func goldenIx(step : Text) : Tx.Instruction {
  let c = captured(step).ixs[captured(step).keeper];
  {
    programId = b58(c.program);
    data = c.data;
    accounts = Array.map<Fx.Meta, Tx.Meta>(c.accounts, func((k, s, w) : Fx.Meta) : Tx.Meta { { key = b58(k); isSigner = s; isWritable = w } });
  };
};
func goldenKey(step : Text, i : Nat) : Blob { goldenIx(step).accounts[i].key };
func sameIx(x : Tx.Instruction, y : Tx.Instruction) : Bool {
  if (x.programId != y.programId or x.data != y.data or x.accounts.size() != y.accounts.size()) { return false };
  var i : Nat = 0;
  while (i < x.accounts.size()) {
    if (x.accounts[i].key != y.accounts[i].key or x.accounts[i].isSigner != y.accounts[i].isSigner or x.accounts[i].isWritable != y.accounts[i].isWritable) {
      Debug.print("    account " # debug_show (i) # " differs: " # Base58.encode(x.accounts[i].key) # " " # debug_show (x.accounts[i].isWritable) # " vs " # Base58.encode(y.accounts[i].key) # " " # debug_show (y.accounts[i].isWritable));
      return false;
    };
    i += 1;
  };
  true;
};

// ---------------------------------------------------------------------
// view builders
// ---------------------------------------------------------------------
let emptyRegistry : Accounts.RegistryPage = { index = 0; keys = Array.tabulate<Blob>(8, func _ { key(0) }) };
let emptyRow : Accounts.SnapshotRow = { key = key(0); series = 0; remaining = 0; slot = 0 };

func view(pool : Accounts.Pool, ticket : ?Accounts.Ticket, close : ?Accounts.CloseRecord) : Planner.View {
  {
    addr = a;
    nowSecs = 1_000_000;
    deadlineMarginSecs = 15;
    maxRows = 8;
    skipNewlyCapped = true;
    pool;
    ticket;
    close;
    registryPage = func(_ : Nat) : ?Accounts.RegistryPage { null };
    snapshotPage = func(_ : Nat) : ?Accounts.SnapshotPage { null };
    liveRemaining = func(_ : Blob) : ?Nat { null };
  };
};
func basePool(activeTicket : Nat64, registryLen : Nat64) : Accounts.Pool {
  { Fx.pool2.expect with activeTicket; registryLen; currentCloseId = null };
};
func baseTicket(id : Nat64, phase : Nat8, cursor : Nat64, capturedLen : Nat64) : Accounts.Ticket {
  { Fx.ticket2.expect with id; phase; cursor; capturedLen; closeId = (null : ?Nat64); aCount = (0 : Nat64); bCount = (0 : Nat64) };
};
func keysOf(prefix : Nat8, n : Nat) : [Blob] { Array.tabulate<Blob>(n, func(i : Nat) : Blob { key(prefix + Nat8.fromNat(i)) }) };
/// Registry pages holding keys K(prefix + slot) for slots 0 .. len-1.
func registryOf(prefix : Nat8, len : Nat) : Nat -> ?Accounts.RegistryPage {
  func(page : Nat) : ?Accounts.RegistryPage {
    if (page * 8 >= len) { return null };
    ?{ index = Nat64.fromNat(page); keys = Array.tabulate<Blob>(8, func(i : Nat) : Blob { let slot = page * 8 + i; if (slot < len) { key(prefix + Nat8.fromNat(slot)) } else { key(0) } }) };
  };
};
/// Snapshot pages for ticket `tid`: slot s holds K(prefix + s) with remaining = remainingOf(s).
func snapshotsOf(prefix : Nat8, len : Nat, tid : Nat64, remainingOf : Nat -> Nat) : Nat -> ?Accounts.SnapshotPage {
  func(page : Nat) : ?Accounts.SnapshotPage {
    if (page * 8 >= len) { return null };
    ?{
      index = Nat64.fromNat(page);
      ticketId = tid;
      rows = Array.tabulate<Accounts.SnapshotRow>(8, func(i : Nat) : Accounts.SnapshotRow {
        let slot = page * 8 + i;
        if (slot < len) { { key = key(prefix + Nat8.fromNat(slot)); series = 0; remaining = remainingOf(slot); slot = Nat64.fromNat(slot) } } else { emptyRow };
      });
    };
  };
};
func isStep(p : Planner.Plan) : ?Planner.Step { switch (p) { case (#Step(s)) { ?s }; case (_) { null } } };
func stepOf(p : Planner.Plan) : Planner.Step { switch (p) { case (#Step(s)) { s }; case (_) { Debug.print("    expected a Step, got " # debug_show (p)); Debug.trap("not a step") } } };
func isBlocked(p : Planner.Plan, text : Text) : Bool { switch (p) { case (#Blocked(m)) { m == text }; case (_) { false } } };
func isNeed(p : Planner.Plan) : ?Planner.Need { switch (p) { case (#Need(n)) { ?n }; case (_) { null } } };
func rowsByte(ix : Tx.Instruction) : Nat8 { Blob.toArray(ix.data)[ix.data.size() - 1] };

// =====================================================================
// goldens: the planner reproduces the captured transactions
// =====================================================================
func snapPage(index : Nat64, tid : Nat64, rows : [Accounts.SnapshotRow]) : Accounts.SnapshotPage {
  { index; ticketId = tid; rows = Array.tabulate<Accounts.SnapshotRow>(8, func(i : Nat) : Accounts.SnapshotRow { if (i < rows.size()) { rows[i] } else { emptyRow } }) };
};

do {
  Debug.print("== golden_against_captured_localnet_transactions ==");
  let backer = goldenKey("discover_rows", 4);

  // discover_rows: ticket 1, cursor 0, one row; registry page 0 holds the backer at slot 0.
  let dView : Planner.View = {
    view(basePool(1, 1), ?baseTicket(1, 0, 0, 1), null) with
    registryPage = func(p : Nat) : ?Accounts.RegistryPage {
      if (p == 0) { ?{ index = 0; keys = Array.tabulate<Blob>(8, func(i : Nat) : Blob { if (i == 0) { backer } else { key(0) } }) } } else { null };
    };
  };
  let d = stepOf(Planner.next(dView));
  check(d.name == "discover_rows" and d.rows == 1 and d.position == "discover_rows|t1|c0", "discover: one row (the captured length of 1 clamps the batch of 8)");
  check(sameIx(d.instruction, goldenIx("discover_rows")), "discover_rows: data and account order match the captured transaction");

  let sen = stepOf(Planner.next(view(basePool(1, 1), ?{ baseTicket(1, 1, 0, 1) with aCount = (1 : Nat64); oldest = backer }, null)));
  check(sen.name == "apply_senior" and sameIx(sen.instruction, goldenIx("apply_senior")), "apply_senior matches the captured transaction");

  let seal = stepOf(Planner.next(view(basePool(1, 1), ?baseTicket(1, 2, 0, 1), null)));
  check(seal.name == "seal_weights" and sameIx(seal.instruction, goldenIx("seal_weights")), "seal_weights matches the captured transaction");

  let allocView : Planner.View = {
    view(basePool(1, 1), ?baseTicket(1, 3, 0, 1), null) with
    snapshotPage = func(p : Nat) : ?Accounts.SnapshotPage { if (p == 0) { ?snapPage(0, 1, [{ key = backer; series = 0; remaining = 1_000; slot = 0 }]) } else { null } };
  };
  let alloc = stepOf(Planner.next(allocView));
  check(alloc.name == "allocate_rows" and alloc.rows == 1 and sameIx(alloc.instruction, goldenIx("allocate_rows")), "allocate_rows matches the captured transaction");

  let pruneView : Planner.View = {
    view(basePool(1, 1), ?baseTicket(1, 4, 1, 1), null) with
    snapshotPage = func(p : Nat) : ?Accounts.SnapshotPage { if (p == 0) { ?snapPage(0, 1, [{ key = backer; series = 0; remaining = 1_000; slot = 0 }]) } else { null } };
  };
  let prune = stepOf(Planner.next(pruneView));
  check(prune.name == "prune_rows" and prune.rows == 1 and sameIx(prune.instruction, goldenIx("prune_rows")), "prune_rows (an open row left for a later ticket: snapshot page + four placeholders) matches the captured transaction");

  let commit = stepOf(Planner.next(view(basePool(1, 1), ?baseTicket(1, 5, 0, 1), null)));
  check(commit.name == "commit_ticket" and sameIx(commit.instruction, goldenIx("commit_ticket")), "commit_ticket (non-closing: Anchor None placeholder for the close record) matches the captured transaction");

  // finalize_round: find the close id whose PDA the capture used.
  var cid : Nat64 = 0;
  var found = false;
  var n : Nat64 = 0;
  while (n < 200 and not found) { if (a.close(n) == goldenKey("finalize_round", 1)) { cid := n; found := true }; n += 1 };
  check(found, "the captured close_record address is the PDA of close id " # debug_show (cid) # " (derived independently here)");
  let closeRec : Accounts.CloseRecord = { Fx.close2.expect with closeId = cid; promotion = #Skipped({ reason = (2 : Nat8); stage = (1 : Nat8) }); cashCommitted = true; reserveSettled = true; finalized = false };
  let poolC : Accounts.Pool = { basePool(0, 4) with currentCloseId = ?cid };
  let fin = stepOf(Planner.next(view(poolC, null, ?closeRec)));
  check(fin.name == "finalize_round" and sameIx(fin.instruction, goldenIx("finalize_round")), "finalize_round matches the captured transaction");
};

// =====================================================================
// discover
// =====================================================================
do {
  Debug.print("== discover_batches ==");
  func dv(cursor : Nat64, capturedLen : Nat64, maxRows : Nat) : Planner.View {
    { view(basePool(1, capturedLen), ?baseTicket(1, 0, cursor, capturedLen), null) with maxRows; registryPage = registryOf(0x40, 20) };
  };
  let s0 = stepOf(Planner.next(dv(0, 20, 8)));
  check(s0.rows == 8 and s0.instruction.accounts.size() == 2 + 3 * 8 and rowsByte(s0.instruction) == 8, "cap of 8 rows: 2 fixed + 3 accounts per row");
  // per row: [registry page (ro), snapshot page (W), backer key (ro)]
  var layoutOk = true;
  var i : Nat = 0;
  while (i < 8) {
    let base = 2 + 3 * i;
    let m = s0.instruction.accounts;
    if (m[base].key != a.registry(0) or m[base].isWritable or m[base + 1].key != a.snapshot(0) or not m[base + 1].isWritable or m[base + 2].key != key(0x40 + Nat8.fromNat(i)) or m[base + 2].isWritable) { layoutOk := false };
    i += 1;
  };
  check(layoutOk, "each row is [registry page ro, snapshot page W, backer ro] from the registry's live keys");
  check(stepOf(Planner.next(dv(3, 20, 8))).rows == 5, "aligned to the page edge: cursor 3 -> 5 rows (slots 3..7)");
  check(stepOf(Planner.next(dv(8, 20, 8))).rows == 8, "cursor 8 -> a full page");
  check(stepOf(Planner.next(dv(16, 20, 8))).rows == 4, "the tail is bounded by the captured length (cursor 16 of 20 -> 4)");
  check(stepOf(Planner.next(dv(19, 20, 8))).rows == 1, "last row alone");
  check(stepOf(Planner.next(dv(0, 20, 1))).rows == 1, "maxRows = 1 is the one-row fallback");
  check(stepOf(Planner.next(dv(0, 20, 3))).rows == 3, "maxRows below the cap is honoured");
  check(stepOf(Planner.next(dv(0, 20, 12))).rows == 8, "maxRows above 8 is clamped to the program's batch limit");
  check(stepOf(Planner.next(dv(0, 20, 0))).rows == 1, "maxRows = 0 is clamped up to one row");
  check(stepOf(Planner.next(dv(8, 20, 8))).position == "discover_rows|t1|c8", "the position key carries ticket id and cursor, not the row count");
  switch (isNeed(Planner.next({ dv(0, 20, 8) with registryPage = func(_ : Nat) : ?Accounts.RegistryPage { null } }))) {
    case (? #Registry(0)) { check(true, "missing registry page 0 -> Need(Registry 0)") };
    case (_) { check(false, "missing registry page should be requested") };
  };
  switch (isNeed(Planner.next({ dv(8, 20, 8) with registryPage = func(p : Nat) : ?Accounts.RegistryPage { if (p == 0) { ?emptyRegistry } else { null } } }))) {
    case (? #Registry(1)) { check(true, "cursor 8 asks for registry page 1") };
    case (_) { check(false, "registry page 1 should be requested") };
  };
  check(isBlocked(Planner.next({ dv(0, 20, 8) with registryPage = func(_ : Nat) : ?Accounts.RegistryPage { ?{ index = 5; keys = keysOf(1, 8) } } }), "registry page 0 reports index 5"), "a page that names another index is refused");
  check(isBlocked(Planner.next(dv(21, 20, 8)), "discover cursor 21 is past the captured length 20"), "a cursor past the captured length is refused");
};

// =====================================================================
// allocate + senior + seal + commit
// =====================================================================
do {
  Debug.print("== allocate_senior_commit ==");
  func av(cursor : Nat64, tid : Nat64) : Planner.View {
    { view(basePool(1, 20), ?baseTicket(1, 3, cursor, 20), null) with snapshotPage = snapshotsOf(0x60, 20, tid, func(s : Nat) : Nat { if (s == 2) { 5 } else { 1_000 } }) };
  };
  let s0 = stepOf(Planner.next(av(0, 1)));
  check(s0.name == "allocate_rows" and s0.rows == 8 and s0.instruction.accounts.size() == 1 + 2 * 8, "allocate: 1 fixed + 2 accounts per row");
  var ok = true;
  var i : Nat = 0;
  while (i < 8) {
    let m = s0.instruction.accounts;
    if (m[1 + 2 * i].key != a.snapshot(0) or m[1 + 2 * i].isWritable or m[2 + 2 * i].key != key(0x60 + Nat8.fromNat(i)) or not m[2 + 2 * i].isWritable) { ok := false };
    i += 1;
  };
  check(ok, "each row is [snapshot page ro, the backer the snapshot row names W] - the closed row (remaining 5) is listed too");
  check(stepOf(Planner.next(av(5, 1))).rows == 3, "allocate batches are aligned to page edges too (cursor 5 -> 3 rows)");
  check(isBlocked(Planner.next(av(0, 9)), "snapshot page 0 belongs to ticket 9, not 1"), "a snapshot page tagged by another ticket is refused");
  check(isNeed(Planner.next({ av(0, 1) with snapshotPage = func(_ : Nat) : ?Accounts.SnapshotPage { null } })) != null, "missing snapshot page -> Need");

  let noA = stepOf(Planner.next(view(basePool(1, 4), ?baseTicket(1, 1, 0, 4), null)));
  check(noA.instruction.accounts.size() == 2 and noA.instruction.accounts[1].key == programId and not noA.instruction.accounts[1].isWritable, "senior without an open A passes the program id read-only (Anchor None)");
  let withA = stepOf(Planner.next(view(basePool(1, 4), ?{ baseTicket(1, 1, 0, 4) with aCount = (2 : Nat64); oldest = key(0x77) }, null)));
  check(withA.instruction.accounts[1].key == key(0x77) and withA.instruction.accounts[1].isWritable, "senior with open A passes the tracked oldest Backer writable");

  let closing = stepOf(Planner.next(view(basePool(1, 4), ?{ baseTicket(1, 5, 0, 4) with closeId = (?7 : ?Nat64) }, null)));
  check(closing.name == "commit_ticket" and closing.instruction.accounts.size() == 5 and closing.instruction.accounts[2].key == a.close(7) and closing.instruction.accounts[2].isWritable, "commit of a closing ticket carries the close record writable");
};

// =====================================================================
// prune
// =====================================================================
do {
  Debug.print("== prune_planning ==");
  func K(slot : Nat) : Blob { key(0x10 + Nat8.fromNat(slot)) };
  func pv(len : Nat, cursor : Nat64, closed : [Nat], skipNew : Bool, maxRows : Nat) : Planner.View {
    func rem(s : Nat) : Nat { for (c in closed.vals()) { if (c == s) { return 5 } }; 1_000 };
    {
      view(basePool(1, Nat64.fromNat(len)), ?baseTicket(1, 4, cursor, Nat64.fromNat(len)), null) with
      maxRows;
      skipNewlyCapped = skipNew;
      registryPage = registryOf(0x10, len);
      snapshotPage = snapshotsOf(0x10, len, 1, rem);
      liveRemaining = func(_ : Blob) : ?Nat { null };
    };
  };
  func skipped(m : [Tx.Meta], from : Nat) : Bool {
    // [snapshot (ro), 4 x program id (ro)] starting at `from`
    if (m[from].isWritable) { return false };
    var i : Nat = 1;
    while (i < 5) { if (m[from + i].key != programId or m[from + i].isWritable) { return false }; i += 1 };
    true;
  };

  let p1 = stepOf(Planner.next(pv(10, 10, [], true, 8)));
  check(p1.rows == 2 and p1.instruction.accounts.size() == 2 + 10, "cursor 10 walks down only to the page edge: slots 9 and 8 (2 rows)");
  check(skipped(p1.instruction.accounts, 2) and skipped(p1.instruction.accounts, 7) and p1.instruction.accounts[2].key == a.snapshot(1), "open rows are skipped with the snapshot page and four program-id placeholders");
  check(stepOf(Planner.next(pv(8, 8, [], true, 8))).rows == 8, "cursor 8 -> a full page of 8 rows (slots 7..0)");
  check(stepOf(Planner.next(pv(10, 10, [], true, 1))).rows == 1, "one-row fallback");

  // A row captured closed (remaining <= 10) at the LAST slot prunes with the moved backer aliasing the pruned one.
  let p2 = stepOf(Planner.next(pv(10, 10, [9], true, 8)));
  let m2 = p2.instruction.accounts;
  check(p2.rows == 2 and m2[2].key == a.snapshot(1) and m2[3].key == K(9) and m2[3].isWritable and m2[4].key == a.registry(1) and m2[4].isWritable and m2[5].key == a.registry(1) and m2[6].key == K(9) and m2[6].isWritable, "closed last slot: [snapshot, pruned W, registry page W, last page W, moved = pruned]");
  check(skipped(m2, 7), "...and the next row (slot 8, open) is skipped in the same batch");

  // A closed row below the last slot swaps the registry's last key into its slot.
  let p3 = stepOf(Planner.next(pv(10, 6, [5], true, 8)));
  let m3 = p3.instruction.accounts;
  check(p3.rows == 6 and m3[2].key == a.snapshot(0) and m3[3].key == K(5) and m3[4].key == a.registry(0) and m3[5].key == a.registry(1) and m3[6].key == K(9), "closed slot 5 of 10: the moved backer is the registry's last key K9 (page 1)");

  // Chain cut: the second prune would move a backer the first already wrote.
  let p4 = stepOf(Planner.next(pv(8, 8, [6, 5], true, 8)));
  check(p4.rows == 2, "the batch is cut BEFORE a row whose moved backer was written earlier in the same batch");
  check(p4.instruction.accounts.size() == 2 + 10 and skipped(p4.instruction.accounts, 2) and p4.instruction.accounts[8].key == K(6) and p4.instruction.accounts[11].key == K(7), "slot 7 skipped, slot 6 pruned with moved = K7");
  // No cut when the moved backer is untouched.
  let p5 = stepOf(Planner.next(pv(8, 8, [4, 2], true, 8)));
  let m5 = p5.instruction.accounts;
  check(p5.rows == 8, "two independent prunes share one batch");
  // row for slot 2 is row index 5 -> accounts from 2 + 25; moved = registry key at last (= 6) after the earlier swap
  check(m5[2 + 25 + 1].key == K(2) and m5[2 + 25 + 4].key == K(6), "second prune simulates the first swap-delete: moved = K6");
  check(m5[2 + 15 + 1].key == K(4) and m5[2 + 15 + 4].key == K(7), "first prune: moved = K7");

  // Newly capped rows need the live backer.
  switch (isNeed(Planner.next(pv(10, 10, [], false, 8)))) {
    case (? #Backer(k)) { check(k == K(9), "skipNewlyCapped = false asks for the live backer of slot 9 first") };
    case (_) { check(false, "a live backer read should be requested") };
  };
  let live7 = func(k : Blob) : ?Nat { if (k == K(9)) { ?7 } else { ?500 } };
  let p6 = stepOf(Planner.next({ pv(10, 10, [], false, 8) with liveRemaining = live7 }));
  check(p6.rows == 2 and p6.instruction.accounts[3].key == K(9) and skipped(p6.instruction.accounts, 7), "a row that fell to <= 10 during this ticket is pruned; the open one after it is skipped");
  let p7 = stepOf(Planner.next({ pv(10, 10, [], false, 8) with liveRemaining = func(_ : Blob) : ?Nat { ?11 } }));
  check(skipped(p7.instruction.accounts, 2), "live remaining 11 is still open: skipped");
  check(stepOf(Planner.next({ pv(10, 10, [9], false, 8) with liveRemaining = live7 })).instruction.accounts[3].key == K(9), "captured-closed rows never need the live read");

  switch (isNeed(Planner.next({ pv(10, 10, [], true, 8) with snapshotPage = func(_ : Nat) : ?Accounts.SnapshotPage { null } }))) {
    case (? #Snapshot(1)) { check(true, "missing snapshot page 1 -> Need(Snapshot 1)") };
    case (_) { check(false, "snapshot page should be requested") };
  };
  switch (isNeed(Planner.next({ pv(10, 6, [5], true, 8) with registryPage = func(p : Nat) : ?Accounts.RegistryPage { if (p == 0) { registryOf(0x10, 10)(0) } else { null } } }))) {
    case (? #Registry(1)) { check(true, "a real prune needs the registry page of the LAST slot") };
    case (_) { check(false, "last registry page should be requested") };
  };
  check(isBlocked(Planner.next(pv(10, 0, [], true, 8)), "prune cursor is 0: nothing left to prune (commit expected)"), "prune with cursor 0 is refused rather than emitting an empty batch");
};

// =====================================================================
// closing pipeline
// =====================================================================
do {
  Debug.print("== closing_pipeline ==");
  let deadline : Nat64 = 1_000_100;
  let c0 : Accounts.CloseRecord = {
    Fx.close1.expect with
    closeId = (4 : Nat64);
    promotion = #Pending;
    cashCommitted = true;
    sealed = false;
    drawState = (0 : Nat8);
    promotionDeadline = deadline;
    finalized = false;
    reserveSettled = false;
    requiredPagesPerSet = (2 : Nat64);
    provisionedPreferredPages = (2 : Nat64);
    provisionedFallbackPages = (2 : Nat64);
    ownerRefundDestination = key(0xAF);
  };
  let poolC : Accounts.Pool = { basePool(0, 4) with currentCloseId = (?4 : ?Nat64) };
  func at(now : Nat64, c : Accounts.CloseRecord) : Planner.Plan { Planner.next({ view(poolC, null, ?c) with nowSecs = now }) };

  check(Planner.next(view(basePool(0, 4), null, null)) == #Idle, "no ticket and no close: Idle");
  check(isBlocked(Planner.next(view(poolC, null, null)), "close record not loaded"), "a close in progress whose record is missing is Blocked, not Idle");
  check(Planner.next({ view(poolC, null, ?{ c0 with finalized = true }) with nowSecs = (5 : Nat64) }) == #Idle, "a finalized close is done");

  check(isBlocked(at(1_000_000, { c0 with cashCommitted = false }), "waiting for the closing allocation run to commit its cash"), "pending promotion, cash not committed: wait for the allocation ticket");
  check(isBlocked(at(1_000_000, c0), "unsupported step: build_candidates"), "candidates not sealed, pages provisioned: build_candidates is not a watchdog step");
  check(isBlocked(at(1_000_000, { c0 with provisionedPreferredPages = (1 : Nat64) }), "unsupported step: provision_page"), "candidate pages missing: provision_page is not a watchdog step (it needs a rent-paying payer)");
  check(isBlocked(at(1_000_000, { c0 with sealed = true }), "unsupported step: request_round_draw"), "sealed, draw not requested: request_round_draw is not a watchdog step");
  check(isBlocked(at(1_000_000, { c0 with sealed = true; drawState = (1 : Nat8) }), "unsupported step: capture_round_draw"), "draw requested before the deadline: capture_round_draw is not a watchdog step");
  check(isBlocked(at(1_000_000, { c0 with sealed = true; drawState = (2 : Nat8) }), "unsupported step: apply_promotion"), "randomness captured: apply_promotion is not a watchdog step");
  check(isBlocked(at(2_000_000, { c0 with sealed = true; drawState = (2 : Nat8) }), "unsupported step: apply_promotion"), "a captured draw is applied even after the deadline - never expired over");

  check(isBlocked(at(deadline + 10, { c0 with sealed = true; drawState = (1 : Nat8) }), "waiting for the promotion deadline margin"), "past the deadline but inside the clock margin: wait (no fee-wasting early expire)");
  let exp = stepOf(at(deadline + 16, { c0 with sealed = true; drawState = (1 : Nat8) }));
  check(exp.name == "expire_promotion" and exp.position == "expire_promotion|k4" and exp.rows == 0, "past deadline + margin with no captured draw: expire_promotion");
  let em = exp.instruction.accounts;
  check(exp.instruction.data == Blob.fromArray([68, 234, 84, 81, 126, 32, 246, 152]), "expire_promotion data is the bare 8-byte discriminator");
  check(em.size() == 8 and em[0].key == a.pool and not em[0].isWritable and em[1].key == a.close(4) and em[1].isWritable and em[2].key == a.ops and em[2].isWritable and em[3].key == a.opsCash and em[3].isWritable and em[4].key == key(0xAF) and em[4].isWritable and em[5].key == a.systemProgram and not em[5].isWritable and em[6].key == a.eventAuthority and not em[6].isWritable and em[7].key == programId and not em[7].isWritable, "expire_promotion accounts: pool, close W, ops W, ops_cash W, owner_refund W (from the record), system program, event authority, program");
  check(stepOf(at(deadline + 16, { c0 with drawState = (0 : Nat8); sealed = false })).name == "expire_promotion", "an unsealed close also expires once the deadline passes");

  let settled : Accounts.CloseRecord = { c0 with promotion = #Skipped({ reason = (1 : Nat8); stage = (1 : Nat8) }); reserveSettled = true };
  check(stepOf(at(1_000_000, settled)).name == "finalize_round", "terminal promotion with a settled reserve: finalize_round");
  check(isBlocked(at(1_000_000, { settled with reserveSettled = false }), "settling the round's processing-cost reserve"), "terminal promotion, reserve not settled yet: wait");
  check(isBlocked(at(1_000_000, { settled with cashCommitted = false }), "waiting for the closing allocation run to commit its cash"), "terminal promotion but cash not committed: wait");
  check(stepOf(at(1_000_000, { c0 with promotion = #Applied({ winner = key(5); entitlement = 5 }); reserveSettled = true })).name == "finalize_round", "an applied promotion finalizes the same way");

  // Ticket first, always.
  let both = Planner.next(view({ poolC with activeTicket = (2 : Nat64) }, ?baseTicket(2, 5, 0, 4), ?c0));
  check(stepOf(both).name == "commit_ticket", "an open allocation ticket is driven before any close step");
};

// =====================================================================
// inconsistent inputs
// =====================================================================
do {
  Debug.print("== inconsistent_inputs ==");
  check(isBlocked(Planner.next(view(basePool(3, 4), null, null)), "ticket account not loaded"), "pool names an active ticket but none was loaded");
  check(isBlocked(Planner.next(view(basePool(3, 4), ?baseTicket(2, 0, 0, 4), null)), "ticket 2 is not the pool's active ticket 3"), "ticket id different from pool.active_ticket");
  check(isBlocked(Planner.next(view(basePool(2, 4), ?baseTicket(2, 6, 0, 4), null)), "ticket is idle but the pool names it active"), "idle ticket with an active pool pointer");
  check(isBlocked(Planner.next(view(basePool(2, 4), ?baseTicket(2, 9, 0, 4), null)), "unknown ticket phase 9"), "unknown phase");
  check(Planner.SUPPORTED.size() == 8, "eight supported steps");
  var hasProvision = false;
  for (n in Planner.SUPPORTED.vals()) { if (n == "provision_page" or n == "apply_promotion" or n == "request_round_draw") { hasProvision := true } };
  check(not hasProvision, "the rent-paying and oracle steps are never supported");
  check(Planner.computeUnitLimit("prune_rows") == 300_000 and Planner.computeUnitLimit("discover_rows") == 200_000, "compute-unit limits mirror the reference adapter (200k, prune 300k)");
};

if (failures > 0) { Debug.trap("planner.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("planner.test.mo: ALL PASS");
