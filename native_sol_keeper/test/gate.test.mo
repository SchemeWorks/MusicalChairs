// Standalone tests for Gate: the pure local gates the tick applies before it
// spends a signature - one transaction in flight, the hourly signature budget, the
// per-position attempt cap with its one-row fallback, the fee caps (balance floor
// and per-transaction ceiling) and the last-look check of the compiled message.
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/gate.test.mo

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat8 "mo:base/Nat8";

import Gate "../Gate";
import Journal "../Journal";
import Tx "../Tx";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};
func key(b : Nat8) : Blob { Blob.fromArray(Array.tabulate<Nat8>(32, func _ { b })) };

let S : Nat = 1_000_000_000;
let HOUR : Nat = 3_600 * S;

func entry(sig : Text, position : Text, status : Journal.Status) : Journal.Entry {
  { signature = sig; blockhash = "bh"; lastValidBlockHeight = 100; step = position; position; rows = 8; raw = Blob.fromArray([1]); status; at = 1; updatedAt = 1; note = "" };
};
func isProceed(l : Gate.Local) : Bool { switch (l) { case (#Proceed) true; case (_) false } };

do {
  Debug.print("== serializes_one_transaction_in_flight ==");
  check(isProceed(Gate.beforePlan(Journal.empty(), [], 100 * S, HOUR, 10)), "an empty journal and an unused budget proceed");
  for (s in [#Submitted, #Unknown, #Confirmed].vals()) {
    switch (Gate.beforePlan(Journal.append(Journal.empty(), entry("a", "p", s), 8), [], 100 * S, HOUR, 10)) {
      case (#Wait(m)) { check(m == "waiting for the previous transaction to resolve", "an unresolved entry makes the tick wait: " # m) };
      case (_) { check(false, "unresolved entries must make the tick wait") };
    };
  };
  for (s in [#Finalized, #Failed, #Expired].vals()) {
    check(isProceed(Gate.beforePlan(Journal.append(Journal.empty(), entry("a", "p", s), 8), [], 100 * S, HOUR, 10)), "a resolved entry does not hold the keeper back");
  };
};

do {
  Debug.print("== refuses_when_the_hourly_budget_is_spent ==");
  let now = 10_000 * S;
  let used3 = [now - 3_000 * S, now - 2_000 * S, now - 10 * S];
  check(isProceed(Gate.beforePlan(Journal.empty(), used3, now, HOUR, 4)), "3 of 4 signatures used: proceed");
  switch (Gate.beforePlan(Journal.empty(), used3, now, HOUR, 3)) {
    case (#Blocked(m)) { check(m == "hourly signature budget spent", "3 of 3 used: " # m) };
    case (_) { check(false, "a spent budget must block") };
  };
  check(isProceed(Gate.beforePlan(Journal.empty(), [now - 3_700 * S, now - 3_650 * S, now - 3_601 * S], now, HOUR, 3)), "signatures older than an hour no longer count");
  switch (Gate.beforePlan(Journal.empty(), [], now, HOUR, 0)) {
    case (#Blocked(_)) { check(true, "a zero budget never signs") };
    case (_) { check(false, "zero budget") };
  };
};

do {
  Debug.print("== position attempts: one-row fallback then block ==");
  let pos = "prune_rows|t3|c16";
  check(Gate.afterPlan(Journal.empty(), pos, 8, 3) == #Go, "a clean position goes with the planned batch");
  let failed = Journal.append(Journal.empty(), entry("f1", pos, #Failed), 8);
  check(Gate.afterPlan(failed, pos, 8, 3) == #Fallback, "after a failure an 8-row plan falls back to one row");
  check(Gate.afterPlan(failed, pos, 1, 3) == #Go, "a plan that is already one row simply goes");
  let three = Journal.append(Journal.append(failed, entry("f2", pos, #Expired), 8), entry("f3", pos, #Failed), 8);
  switch (Gate.afterPlan(three, pos, 1, 3)) {
    case (#Blocked(m)) { check(m == "position failed 3 times: operator action needed", "three failures block the position: " # m) };
    case (_) { check(false, "the position must block") };
  };
  check(Gate.afterPlan(three, "other", 8, 3) == #Go, "another position is unaffected");
};

do {
  Debug.print("== refuses_to_sign_below_the_fee_balance_floor ==");
  // balance must cover floor + this transaction's fee
  check(Gate.feeGate(20_000_000, 10_000_000, 5_000, 10_000) == null, "balance above floor + fee: ok");
  check(Gate.feeGate(10_005_000, 10_000_000, 5_000, 10_000) == null, "balance exactly floor + fee: ok");
  switch (Gate.feeGate(10_004_999, 10_000_000, 5_000, 10_000)) {
    case (?m) { check(m == "fee address balance 10004999 is below the floor 10000000 plus the 5000 fee", "one lamport short: " # m) };
    case (null) { check(false, "one lamport short of floor + fee must be refused") };
  };
  check(Gate.feeGate(0, 0, 5_000, 10_000) != null, "an empty address cannot pay even with a zero floor");
  switch (Gate.feeGate(100_000_000, 10_000_000, 12_000, 10_000)) {
    case (?m) { check(m == "fee 12000 exceeds maxFeeLamportsPerTx 10000", "the per-transaction ceiling: " # m) };
    case (null) { check(false, "a fee above the ceiling must be refused") };
  };
};

do {
  Debug.print("== refuses_a_message_that_names_the_fee_address ==");
  let fee = key(0xFE);
  let program = key(0x07);
  let budget = key(0xC0);
  let good = Tx.compile(fee, key(1), [{ programId = program; accounts = [{ key = key(2); isSigner = false; isWritable = true }]; data = Blob.fromArray([1]) }]);
  check(Gate.checkMessage(good.bytes, fee, [program, budget]) == null, "a fee-payer-only message for the program passes");

  let withBudget = Tx.compile(fee, key(1), Array.append(Tx.computeBudgetInstructions(200_000, 5), [{ programId = program; accounts = []; data = Blob.fromArray([1]) }]));
  let budgetProgram = Tx.computeBudgetInstructions(1, 0)[0].programId;
  check(Gate.checkMessage(withBudget.bytes, fee, [program, budgetProgram]) == null, "ComputeBudget instructions are allowed");

  let referenced = Tx.compile(fee, key(1), [{ programId = program; accounts = [{ key = fee; isSigner = false; isWritable = false }]; data = Blob.fromArray([1]) }]);
  check(Gate.checkMessage(referenced.bytes, fee, [program, budget]) == ?"an instruction references the fee address", "an instruction listing the fee address is refused");

  let foreign = Tx.compile(fee, key(1), [{ programId = key(0x99); accounts = []; data = Blob.fromArray([1]) }]);
  check(Gate.checkMessage(foreign.bytes, fee, [program, budget]) == ?"the message names a foreign program", "an instruction for any other program (e.g. the system program) is refused");

  let extraSigner = Tx.compile(fee, key(1), [{ programId = program; accounts = [{ key = key(2); isSigner = true; isWritable = false }]; data = Blob.fromArray([1]) }]);
  check(Gate.checkMessage(extraSigner.bytes, fee, [program, budget]) == ?"the message is not fee-payer-only", "a second signer is refused");

  check(Gate.checkMessage(good.bytes, key(0xAA), [program, budget]) == ?"the message is not fee-payer-only", "a message whose account 0 is not the fee address is refused");
  check(Gate.checkMessage(Blob.fromArray([1, 2]), fee, [program]) == ?"the compiled message does not parse", "garbage does not parse");
};

if (failures > 0) { Debug.trap("gate.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("gate.test.mo: ALL PASS");
