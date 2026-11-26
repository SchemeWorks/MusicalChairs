// Standalone tests for the byte-level primitives the step builder needs:
// SHA-256, base58, base64, the Ed25519 on-curve test and program-derived
// address search. Goldens: Python hashlib/base64 and @solana/web3.js (see
// test/gen/fixtures.py and pda_vectors.cjs); base58 is also cross-checked against
// the bridge's pinned copy.
//
// Run from the repo root (zsh: pass ${=MOC_SOURCES}):
//   $MOC -r ${=MOC_SOURCES} native_sol_keeper/test/pda.test.mo

import Blob "mo:base/Blob";
import Debug "mo:base/Debug";

import Decode "../../native_sol_bridge/Decode";
import Base58 "../Base58";
import Base64 "../Base64";
import Pda "../Pda";
import Sha256 "../Sha256";
import Fx "fixtures";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};
func b58(t : Text) : Blob { switch (Base58.decode(t)) { case (?b) { b }; case (null) { Debug.trap("bad base58 " # t) } } };

// =====================================================================
do {
  Debug.print("== sha256 ==");
  var ok = 0;
  for (v in Fx.sha256Vectors.vals()) {
    if (Sha256.digest(v.message) == v.digest) { ok += 1 } else { Debug.print("  mismatch for a " # debug_show (v.message.size()) # "-byte message") };
  };
  check(ok == Fx.sha256Vectors.size(), "all " # debug_show (Fx.sha256Vectors.size()) # " SHA-256 vectors match (block boundaries 55/56/63/64/65/119/120/128 bytes, 1000 bytes)");
};

do {
  Debug.print("== base58 ==");
  var enc = 0;
  var dec = 0;
  var parity = 0;
  for (v in Fx.base58Vectors.vals()) {
    if (Base58.encode(v.bytes) == v.text) { enc += 1 } else { Debug.print("  encode mismatch: " # v.text) };
    if (Base58.decode(v.text) == ?v.bytes) { dec += 1 } else { Debug.print("  decode mismatch: " # v.text) };
    if (Base58.encode(v.bytes) == Decode.base58Encode(v.bytes)) { parity += 1 };
  };
  let n = Fx.base58Vectors.size();
  check(enc == n, "encode matches every vector (leading zeros -> '1's)");
  check(dec == n, "decode round-trips every vector");
  check(parity == n, "identical to the bridge's pinned base58Encode");
  check(Base58.decode("0OIl") == null, "characters outside the alphabet are rejected");
  check(Base58.decode("") == ?Blob.fromArray([]), "empty string decodes to empty bytes");
  check(Base58.isOfLength(Fx.PROGRAM, 32) and not Base58.isOfLength(Fx.PROGRAM, 31), "isOfLength");
};

do {
  Debug.print("== base64 ==");
  var enc = 0;
  var dec = 0;
  for (v in Fx.base64Vectors.vals()) {
    if (Base64.encode(v.bytes) == v.text) { enc += 1 } else { Debug.print("  encode mismatch: " # v.text) };
    if (Base64.decode(v.text) == ?v.bytes) { dec += 1 } else { Debug.print("  decode mismatch: " # v.text) };
  };
  check(enc == Fx.base64Vectors.size(), "encode matches every vector (all padding lengths)");
  check(dec == Fx.base64Vectors.size(), "decode round-trips every vector");
  check(Base64.decode("!!!!") == null and Base64.decode("abc") == null, "invalid alphabet and bad length are rejected");
};

do {
  Debug.print("== ed25519 on-curve test ==");
  var ok = 0;
  var yes = 0;
  var no = 0;
  for (v in Fx.curveVectors.vals()) {
    let got = Pda.isOnCurve(v.point);
    if (got == v.onCurve) { ok += 1 } else { Debug.print("  mismatch: " # v.name) };
    if (v.onCurve) { yes += 1 } else { no += 1 };
  };
  check(ok == Fx.curveVectors.size(), "agrees with web3.js on all " # debug_show (Fx.curveVectors.size()) # " points (" # debug_show (yes) # " on curve, " # debug_show (no) # " off)");
  check(yes >= 10 and no >= 10, "the vectors exercise both outcomes");
  check(not Pda.isOnCurve(Blob.fromArray([1, 2, 3])), "wrong length is not a point");
};

do {
  Debug.print("== program-derived addresses ==");
  var addrOk = 0;
  var bumpOk = 0;
  var lowBump = 0;
  for (v in Fx.pdaVectors.vals()) {
    switch (Pda.find(v.seeds, b58(v.program))) {
      case (?(addr, bump)) {
        if (Base58.encode(addr) == v.address) { addrOk += 1 } else { Debug.print("  address mismatch: " # v.name # " got " # Base58.encode(addr) # " want " # v.address) };
        if (bump == v.bump) { bumpOk += 1 } else { Debug.print("  bump mismatch: " # v.name) };
        if (v.bump < 253) { lowBump += 1 };
      };
      case (null) { Debug.print("  no PDA found: " # v.name) };
    };
  };
  let n = Fx.pdaVectors.size();
  check(addrOk == n, "all " # debug_show (n) # " PDA addresses match web3.js");
  check(bumpOk == n, "all canonical bumps match");
  check(lowBump >= 3, "vectors with a bump below 253 exercise the search loop");

  let a = Pda.addresses(b58(Fx.PROGRAM));
  func addrIs(name : Text, got : Blob) : Bool {
    for (v in Fx.pdaVectors.vals()) { if (v.name == name) { return Base58.encode(got) == v.address } };
    false;
  };
  check(addrIs("pool", a.pool) and addrIs("ticket", a.ticket) and addrIs("ops", a.ops) and addrIs("ops-cash", a.opsCash) and addrIs("event-authority", a.eventAuthority), "Addresses carries the five fixed PDAs");
  check(addrIs("registry-0", a.registry(0)) and addrIs("registry-40", a.registry(40)) and addrIs("registry-256", a.registry(256)), "registry page addresses");
  check(addrIs("snapshot-1", a.snapshot(1)) and addrIs("snapshot-9", a.snapshot(9)), "snapshot page addresses");
  check(addrIs("close-7", a.close(7)) and addrIs("close-70000", a.close(70_000)), "close record addresses");
  check(Base58.encode(a.systemProgram) == "11111111111111111111111111111111", "system program id");
  check(Base58.encode(a.programId) == Fx.PROGRAM, "program id is carried through");
  check(Pda.u64le(0x0102030405060708) == Blob.fromArray([8, 7, 6, 5, 4, 3, 2, 1]), "u64le is little-endian");
};

if (failures > 0) { Debug.trap("pda.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("pda.test.mo: ALL PASS");
