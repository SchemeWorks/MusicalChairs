// IMPL-49h test fixture (LOCAL REPLICA ONLY): a PP-ledger stand-in with a deterministic LATCH.
//
// It exists for ONE reason: the real pp_ledger answers an icrc1_transfer within about one round, and
// stopping it makes calls fail fast instead of pending, so the legacy observer's gift-mint await
// cannot be held open from outside. This stub is installed as the ledger of a `shenanigans_latch`
// build whose ONLY difference from the committed shenanigans is the PP ledger canister id constant
// in PpLedger.mo (the driver patches that one line and records both file hashes). It implements just
// icrc1_transfer with ICRC-1 mint semantics (every transfer from the Shenanigans canister is a mint)
// and Duplicate dedup on (to, amount, memo, created_at_time). It does NOT enforce the real ledger's
// 32-byte memo limit (that is why the legacy 70-byte gift memo can commit here but not on the real ledger).
//
// Latch: calls whose memo starts with `holdPrefix` (the legacy gift mint uses "signup-<principal>")
// are held while `gate` is engaged, until release():
//   #commitThenHold : the mint is RECORDED, the reply is withheld  (the ledger has committed, the reply is in flight)
//   #holdThenCommit : nothing is recorded until release, then it commits and replies
//   #holdThenReject : nothing is ever recorded; on release the call is rejected (a SYNTHETIC reject, not a real ledger error)
// While held, the reply is withheld by yielding on a management-canister round trip (raw_rand) per
// iteration, which throttles the wait loop to roughly one iteration per round.
import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Text "mo:base/Text";
import Time "mo:base/Time";
import Error "mo:base/Error";
import Random "mo:base/Random";
import Array "mo:base/Array";

persistent actor LatchLedger {
  public type Account = { owner : Principal; subaccount : ?Blob };
  public type TransferArg = {
    from_subaccount : ?Blob;
    to : Account;
    amount : Nat;
    fee : ?Nat;
    memo : ?Blob;
    created_at_time : ?Nat64;
  };
  public type TransferError = {
    #BadFee : { expected_fee : Nat };
    #BadBurn : { min_burn_amount : Nat };
    #InsufficientFunds : { balance : Nat };
    #TooOld;
    #CreatedInFuture : { ledger_time : Nat64 };
    #Duplicate : { duplicate_of : Nat };
    #TemporarilyUnavailable;
    #GenericError : { error_code : Nat; message : Text };
  };
  public type TransferResult = { #Ok : Nat; #Err : TransferError };
  public type Mint = { block : Nat; to : Account; amount : Nat; memo : ?Blob; created_at_time : ?Nat64; caller : Principal; at : Int };
  public type HoldMode = { #off; #commitThenHold; #holdThenCommit; #holdThenReject };
  public type Status = { mode : HoldMode; prefix : Text; gate : Bool; heldNow : Nat; heldEver : Nat; mints : Nat; rejected : Nat };

  var mints : [Mint] = [];
  var holdPrefix : Text = "signup-";
  var holdMode : HoldMode = #off;
  var gate : Bool = false;
  var heldNow : Nat = 0;
  var heldEver : Nat = 0;
  var rejected : Nat = 0;
  var failAll : Bool = false; // every icrc1_transfer rejects immediately (like a stopped ledger)

  func memoMatches(m : ?Blob) : Bool {
    switch (m) {
      case (?b) {
        switch (Text.decodeUtf8(b)) {
          case (?t) { Text.startsWith(t, #text holdPrefix) };
          case null { false };
        };
      };
      case null { false };
    };
  };

  func existing(arg : TransferArg) : ?Nat {
    if (arg.created_at_time == null) { return null };
    for (m in mints.vals()) {
      if (m.to == arg.to and m.amount == arg.amount and m.memo == arg.memo and m.created_at_time == arg.created_at_time) { return ?m.block };
    };
    null;
  };

  func record(caller : Principal, arg : TransferArg) : Nat {
    let block = mints.size();
    mints := Array.append<Mint>(mints, [{ block = block; to = arg.to; amount = arg.amount; memo = arg.memo; created_at_time = arg.created_at_time; caller = caller; at = Time.now() }]);
    block;
  };

  // Withhold the reply while the gate is engaged; one management round trip per iteration.
  func waitForRelease() : async () {
    heldNow += 1;
    heldEver += 1;
    while (gate) { ignore await Random.blob() };
    heldNow -= 1;
  };

  public shared ({ caller }) func icrc1_transfer(arg : TransferArg) : async TransferResult {
    if (failAll) { throw Error.reject("latch ledger: simulated stopped ledger (every call rejected)") };
    switch (existing(arg)) {
      case (?b) { return #Err(#Duplicate({ duplicate_of = b })) };
      case null {};
    };
    let held = gate and holdMode != #off and memoMatches(arg.memo);
    if (not held) { return #Ok(record(caller, arg)) };
    switch (holdMode) {
      case (#commitThenHold) {
        let b = record(caller, arg);
        await waitForRelease();
        #Ok(b);
      };
      case (#holdThenCommit) {
        await waitForRelease();
        #Ok(record(caller, arg));
      };
      case (#holdThenReject) {
        await waitForRelease();
        rejected += 1;
        throw Error.reject("latch ledger: synthetic reject (nothing was minted)");
      };
      case (#off) { #Ok(record(caller, arg)) };
    };
  };

  // ---- latch control (test driver only) ----
  public shared func setHold(mode : HoldMode, prefix : Text) : async () {
    holdMode := mode;
    holdPrefix := prefix;
    gate := (mode != #off);
  };
  public shared func release() : async () { gate := false };
  public shared func setFailAll(b : Bool) : async () { failAll := b };
  public shared func resetAll() : async () {
    mints := [];
    holdMode := #off;
    gate := false;
    heldNow := 0;
    heldEver := 0;
    rejected := 0;
    failAll := false;
  };
  public query func getStatus() : async Status {
    { mode = holdMode; prefix = holdPrefix; gate = gate; heldNow = heldNow; heldEver = heldEver; mints = mints.size(); rejected = rejected };
  };
  public query func getMints() : async [Mint] { mints };
};
