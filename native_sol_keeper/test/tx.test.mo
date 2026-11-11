// Standalone tests for Tx: legacy message compilation, serialization, parsing,
// the wire format, compute-budget instructions and the fee estimate.
// Goldens: seven REAL localnet transactions (solana/fixtures/rpc) built by the
// TypeScript adapter; web3.js orders keys within a class by base58 text while
// Tx keeps first-appearance order, so messages are compared SEMANTICALLY (header
// classes, resolved instructions, blockhash) rather than byte for byte; the bytes
// of Tx's own output are pinned by parse(compile(x)) round trips and fixed
// vectors.
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/tx.test.mo

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat32 "mo:base/Nat32";
import Nat8 "mo:base/Nat8";

import Base58 "../Base58";
import Base64 "../Base64";
import Tx "../Tx";
import Fx "fixtures";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};
func b58(t : Text) : Blob { switch (Base58.decode(t)) { case (?b) { b }; case (null) { Debug.trap("bad base58 " # t) } } };
func key(b : Nat8) : Blob { Blob.fromArray(Array.tabulate<Nat8>(32, func _ { b })) };

func Nat32FromBytes(a : [Nat8]) : Nat32 {
  Nat32.fromNat(Nat8.toNat(a[1]) + 256 * Nat8.toNat(a[2]) + 65_536 * Nat8.toNat(a[3]) + 16_777_216 * Nat8.toNat(a[4]));
};

func instructionOf(c : Fx.CapturedIx) : Tx.Instruction {
  {
    programId = b58(c.program);
    data = c.data;
    accounts = Array.map<Fx.Meta, Tx.Meta>(c.accounts, func((k, s, w) : Fx.Meta) : Tx.Meta { { key = b58(k); isSigner = s; isWritable = w } });
  };
};
func sameInstruction(a : Tx.Instruction, b : Tx.Instruction) : Bool {
  if (a.programId != b.programId or a.data != b.data or a.accounts.size() != b.accounts.size()) { return false };
  var i : Nat = 0;
  while (i < a.accounts.size()) {
    if (a.accounts[i].key != b.accounts[i].key or a.accounts[i].isSigner != b.accounts[i].isSigner or a.accounts[i].isWritable != b.accounts[i].isWritable) { return false };
    i += 1;
  };
  true;
};

do {
  Debug.print("== shortvec + wire ==");
  check(Tx.shortvec(0) == [0] and Tx.shortvec(127) == [127] and Tx.shortvec(128) == [0x80, 1] and Tx.shortvec(300) == [0xAC, 2] and Tx.shortvec(16_384) == [0x80, 0x80, 1], "shortvec encodes 0, 127, 128, 300, 16384");
  let msg = Blob.fromArray([9, 9, 9]);
  let sig = Blob.fromArray(Array.tabulate<Nat8>(64, func(i : Nat) : Nat8 { Nat8.fromNat(i) }));
  let w = Tx.wire(msg, sig);
  check(w.size() == 1 + 64 + 3 and Blob.toArray(w)[0] == 1 and Blob.toArray(w)[65] == 9, "wire = shortvec(1) ++ signature ++ message");
  check(Tx.signatureText(sig) == Base58.encode(sig), "the transaction id is the base58 first signature");
};

do {
  Debug.print("== captured transactions ==");
  var parsedOk = 0;
  var semanticOk = 0;
  var headerOk = 0;
  for (c in Fx.captured.vals()) {
    let ixs = Array.map<Fx.CapturedIx, Tx.Instruction>(c.ixs, instructionOf);
    // 1) the captured bytes parse, and resolve to the instructions the generator recorded
    switch (Tx.parse(c.message)) {
      case (?p) {
        let resolved = Tx.resolve(p);
        var same = resolved.size() == ixs.size();
        if (same) { var i : Nat = 0; while (i < ixs.size()) { if (not sameInstruction(resolved[i], ixs[i])) { same := false }; i += 1 } };
        if (same and p.blockhash == b58(c.blockhash) and p.accountKeys[0] == b58(c.feePayer)) { parsedOk += 1 } else { Debug.print("  parse/resolve mismatch: " # c.file) };
        // 2) our compile of the same instructions yields an equivalent message
        let mine = Tx.compile(b58(c.feePayer), b58(c.blockhash), ixs);
        switch (Tx.parse(mine.bytes)) {
          case (?q) {
            let r2 = Tx.resolve(q);
            var same2 = r2.size() == ixs.size();
            if (same2) { var i : Nat = 0; while (i < ixs.size()) { if (not sameInstruction(r2[i], ixs[i])) { same2 := false }; i += 1 } };
            if (same2 and q.blockhash == p.blockhash) { semanticOk += 1 } else { Debug.print("  compile mismatch: " # c.file) };
            if (q.numRequiredSignatures == p.numRequiredSignatures and q.numReadonlySigned == p.numReadonlySigned and q.numReadonlyUnsigned == p.numReadonlyUnsigned and q.accountKeys.size() == p.accountKeys.size()) { headerOk += 1 } else {
              Debug.print("  header mismatch: " # c.file # " mine " # debug_show (q.numReadonlyUnsigned, q.accountKeys.size()) # " theirs " # debug_show (p.numReadonlyUnsigned, p.accountKeys.size()));
            };
          };
          case (null) { Debug.print("  our message does not parse: " # c.file) };
        };
      };
      case (null) { Debug.print("  captured message does not parse: " # c.file) };
    };
  };
  let n = Fx.captured.size();
  check(n == 7, "seven captured keeper transactions");
  check(parsedOk == n, "every captured message parses and resolves to the recorded instructions");
  check(semanticOk == n, "compiling the same instructions with the same fee payer and blockhash gives an equivalent message");
  check(headerOk == n, "header class counts and key count equal the captured (web3.js) messages");
};

do {
  Debug.print("== key ordering rules ==");
  let fee = key(1);
  let w1 = key(2); // writable
  let r1 = key(3); // readonly
  let prog = key(4);
  let s2 = key(5); // an extra signer
  let ix1 : Tx.Instruction = {
    programId = prog;
    accounts = [
      { key = r1; isSigner = false; isWritable = false },
      { key = w1; isSigner = false; isWritable = true },
      { key = s2; isSigner = true; isWritable = false },
      { key = w1; isSigner = false; isWritable = false }, // same key again, read-only here: flags merge to writable
    ];
    data = Blob.fromArray([7]);
  };
  let c = Tx.compile(fee, key(9), [ix1]);
  check(c.accountKeys[0] == fee, "the fee payer is always account 0");
  check(c.numRequiredSignatures == 2 and c.numReadonlySigned == 1, "signers: the fee payer (writable) and one readonly signer");
  check(c.accountKeys[1] == s2, "readonly signers follow the writable signers");
  check(c.accountKeys[2] == w1, "then writable non-signers");
  check(c.accountKeys[3] == r1 and c.accountKeys[4] == prog and c.numReadonlyUnsigned == 2, "then readonly non-signers (accounts, then the program)");
  check(c.accountKeys.size() == 5, "a key used twice appears once");
  check(c.instructionAccountIndexes[0] == [3, 2, 1, 2], "instruction account indexes follow the compiled order (duplicates keep both positions)");
  check(c.programIdIndexes[0] == 4, "program id index");

  // The fee payer passed again as an instruction account stays a single signer+writable key.
  let ix2 : Tx.Instruction = { programId = prog; accounts = [{ key = fee; isSigner = false; isWritable = false }]; data = Blob.fromArray([]) };
  let c2 = Tx.compile(fee, key(9), [ix2]);
  check(c2.accountKeys.size() == 2 and c2.numRequiredSignatures == 1 and c2.numReadonlySigned == 0, "an account equal to the fee payer merges into account 0");
  check(c2.instructionAccountIndexes[0] == [0], "...and is referenced as index 0 (the authority test uses this to prove the keeper never does it)");
};

do {
  Debug.print("== parse rejects malformed messages ==");
  check(Tx.parse(Blob.fromArray([])) == null, "empty");
  check(Tx.parse(Blob.fromArray([1, 0, 0, 1])) == null, "truncated key list");
  let good = Tx.compile(key(1), key(2), [{ programId = key(3); accounts = []; data = Blob.fromArray([1, 2]) }]);
  let b = Blob.toArray(good.bytes);
  check(Tx.parse(Blob.fromArray(Array.tabulate<Nat8>(b.size() - 1, func(i : Nat) : Nat8 { b[i] }))) == null, "a message cut one byte short is rejected");
  check(Tx.parse(Blob.fromArray(Array.append<Nat8>(b, [0]))) == null, "trailing bytes are rejected");
  check(Tx.parse(good.bytes) != null, "the compiled message parses");
};

do {
  Debug.print("== compute budget + fee ==");
  let cb = Tx.computeBudgetInstructions(200_000, 1_000);
  check(cb.size() == 2, "limit and price both set -> two instructions");
  check(Base58.encode(cb[0].programId) == Tx.COMPUTE_BUDGET_PROGRAM, "program id is ComputeBudget111...");
  check(cb[0].data == Blob.fromArray([2, 0x40, 0x0D, 0x03, 0x00]), "SetComputeUnitLimit: [2] ++ u32 LE (200000)");
  check(cb[1].data == Blob.fromArray([3, 0xE8, 0x03, 0, 0, 0, 0, 0, 0]), "SetComputeUnitPrice: [3] ++ u64 LE (1000 micro-lamports)");
  check(Tx.computeBudgetInstructions(0, 0).size() == 0, "limit 0 and price 0 -> none (default budget, base fee only)");
  check(Tx.computeBudgetInstructions(300_000, 0).size() == 1, "limit only");
  // The captured transactions carry a SetComputeUnitLimit as their first instruction: same encoding.
  let first = Fx.captured[0].ixs[0];
  let captured = Blob.toArray(first.data);
  check(Base58.encode(b58(first.program)) == Tx.COMPUTE_BUDGET_PROGRAM and captured[0] == 2 and captured.size() == 5, "captured ComputeBudget instruction has the same shape");
  let limit : Nat32 = Nat32FromBytes(captured);
  let again = Tx.computeBudgetInstructions(limit, 0);
  check(again.size() == 1 and again[0].data == first.data, "re-encoding the captured limit reproduces the captured bytes");

  let c = Tx.compile(key(1), key(2), [{ programId = key(3); accounts = []; data = Blob.fromArray([]) }]);
  check(Tx.feeLamports(c, 0, 0) == 5_000, "base fee: 5000 lamports per signature");
  check(Tx.feeLamports(c, 200_000, 1_000) == 5_200, "priority fee = ceil(limit * price / 1e6) = 200 lamports");
  check(Tx.feeLamports(c, 200_001, 1_000) == 5_201, "priority fee rounds up");
};

do {
  Debug.print("== base64 of a transaction ==");
  let c = Tx.compile(key(1), key(2), [{ programId = key(3); accounts = []; data = Blob.fromArray([1]) }]);
  let w = Tx.wire(c.bytes, Blob.fromArray(Array.tabulate<Nat8>(64, func _ { 1 })));
  check(Base64.decode(Base64.encode(w)) == ?w, "wire bytes round-trip through base64 (the sendTransaction encoding)");
};

if (failures > 0) { Debug.trap("tx.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("tx.test.mo: ALL PASS");
