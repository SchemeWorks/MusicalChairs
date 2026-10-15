// Standalone tests for Decode (+ Events/Rpc types): base58/base64, the wire
// parser (legacy and v0), the typed-RPC path versus the JSON-shaped fixture
// path, provider-reply classification, and Borsh event bodies.
//
// Run from the repo root:
//   moc -r --package base <base@0.16.0/src> native_sol_bridge/test/decode.test.mo
//
// Real data: all 29 fixtures in fixtures.test.mo (solana/fixtures/rpc/*.json).

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat8 "mo:base/Nat8";
import Result "mo:base/Result";
import Text "mo:base/Text";

import Decode "../Decode";
import Events "../Events";
import Rpc "../Rpc";
import Fx "fixtures.test";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};

func fill(n : Nat, b : Nat8) : [Nat8] { Array.freeze(Array.init<Nat8>(n, b)) };
func key(b : Nat8) : Text { Decode.base58Encode(Blob.fromArray(fill(32, b))) };

// ---------------------------------------------------------------- base58
do {
  check(Decode.base58Encode(Blob.fromArray(fill(32, 0))) == "11111111111111111111111111111111", "base58: 32 zero bytes are the 32-character system program key");
  switch (Decode.base58Decode("11111111111111111111111111111111")) {
    case (?b) { check(Blob.toArray(b) == fill(32, 0), "base58: system program key decodes to 32 zero bytes") };
    case (null) { check(false, "base58: system program key decodes") };
  };
  check(Decode.base58Decode("0OIl") == null, "base58: characters outside the alphabet are rejected");
  var allRoundTrip = true;
  var seen = 0;
  for (f in Fx.all.vals()) {
    for (k in f.json.accountKeys.vals()) {
      seen += 1;
      switch (Decode.base58Decode(k)) {
        case (?b) { if (b.size() != 32 or Decode.base58Encode(b) != k) { allRoundTrip := false } };
        case (null) { allRoundTrip := false };
      };
    };
    for (s in f.json.signatures.vals()) {
      switch (Decode.base58Decode(s)) {
        case (?b) { if (b.size() != 64 or Decode.base58Encode(b) != s) { allRoundTrip := false } };
        case (null) { allRoundTrip := false };
      };
    };
  };
  check(allRoundTrip and seen > 200, "base58: every fixture account key (32 bytes) and signature (64 bytes) round-trips");
};

// ---------------------------------------------------------------- base64
do {
  check(Decode.base64Decode("TWFuTWE=") == ?[77, 97, 110, 77, 97], "base64: padded vector");
  check(Decode.base64Decode("TWFuTWE") == ?[77, 97, 110, 77, 97], "base64: unpadded vector");
  check(Decode.base64Decode("TQ==") == ?[77], "base64: double padding");
  check(Decode.base64Decode("+/8=") == ?[251, 255], "base64: + and / map to 62 and 63");
  check(Decode.base64Decode("T*Fu") == null, "base64: invalid character rejected");
  check(Decode.base64Decode("TQ=Q") == null, "base64: data after padding rejected");
};

// ------------------------------------------------------- base64 encode
do {
  func enc(t : Text) : Text { Decode.base64Encode(Blob.toArray(Text.encodeUtf8(t))) };
  check(enc("") == "" and enc("f") == "Zg==" and enc("fo") == "Zm8=" and enc("foo") == "Zm9v" and enc("foob") == "Zm9vYg==" and enc("fooba") == "Zm9vYmE=" and enc("foobar") == "Zm9vYmFy", "base64Encode: the RFC 4648 section 10 test vectors");
  check(Decode.base64Encode([251, 255]) == "+/8=", "base64Encode: 62 and 63 map to + and /");
  var all = true;
  for (f in Fx.all.vals()) {
    switch (f.rpc.transaction.transaction) {
      case (#binary((s, #base64))) {
        switch (Decode.base64Decode(s)) {
          case (?bytes) { if (Decode.base64Encode(bytes) != s) { all := false } };
          case (null) { all := false };
        };
      };
      case (_) { all := false };
    };
  };
  check(all, "base64Encode(base64Decode(s)) == s for all 29 captured wire transactions");
};

// ----------------- the re-signing transform the replica stub applies (cloneFixture)
do {
  let f = Fx.get("flows-009");
  switch (f.rpc.transaction.transaction) {
    case (#binary((w, #base64))) {
      switch (Decode.base64Decode(w)) {
        case (?bytes) {
          let nb = Array.tabulate<Nat8>(bytes.size(), func(i) { if (i >= 1 and i < 65) { 0x66 } else { bytes[i] } });
          let newSig = Decode.base58Encode(Blob.fromArray(Array.subArray<Nat8>(nb, 1, 64)));
          let copy = { f.rpc with transaction = { f.rpc.transaction with transaction = #binary((Decode.base64Encode(nb), #base64)) } };
          switch (Decode.fromRpc(f.rpc), Decode.fromRpc(copy)) {
            case (#ok(a), #ok(b)) {
              check(b.signature == newSig and b.signature != a.signature, "re-signed copy: the wire transaction decodes with the new first signature");
              check({ b with signature = a.signature } == a, "re-signed copy: everything else (keys, instructions, inner instructions, version, status) is identical to the captured transaction");
            };
            case (_, _) { check(false, "re-signed copy decodes") };
          };
        };
        case (null) { check(false, "base64 decode") };
      };
    };
    case (_) { check(false, "binary base64 fixture") };
  };
};

// ------------------------------------------- wire path == JSON path (real)
do {
  var same = 0;
  var all = true;
  for (f in Fx.all.vals()) {
    let a = Decode.fromRpc(f.rpc);
    let b = Decode.fromJson(f.json);
    switch (a, b) {
      case (#ok(ta), #ok(tb)) {
        if (ta == tb) { same += 1 } else { all := false; Debug.print("  differs: " # f.name) };
      };
      case (_, _) { all := false; Debug.print("  decode failed: " # f.name) };
    };
  };
  check(all and same == 29, "wire-decoded typed RPC result equals the JSON-fixture decode for all 29 real transactions (got " # debug_show (same) # ")");
  // And the signature the wire parse extracts is the fixture's own.
  var sigOk = true;
  for (f in Fx.all.vals()) {
    switch (Decode.fromRpc(f.rpc)) {
      case (#ok(t)) { if (t.signature != f.json.signatures[0] or t.numRequiredSignatures != 1 or not t.succeeded) { sigOk := false } };
      case (#err(_)) { sigOk := false };
    };
  };
  check(sigOk, "wire parse: first signature, numRequiredSignatures = 1 and success flag match the fixtures");
};

// -------------------------------------- events found in the real fixtures
func tx(name : Text) : Decode.Tx {
  switch (Decode.fromRpc(Fx.get(name).rpc)) {
    case (#ok(t)) { t };
    case (#err(_)) { Debug.trap("fixture failed to decode: " # name) };
  };
};

do {
  var manifestOk = true;
  var total = 0;
  for (f in Fx.all.vals()) {
    let found = Decode.scanEvents(tx(f.name), Fx.PROGRAM, Fx.EVENT_AUTHORITY);
    let names = Array.map<Decode.FoundEvent, Text>(found, func(x) { Events.nameOf(x.event) });
    total += names.size();
    if (names != f.manifest_events) { manifestOk := false; Debug.print("  manifest mismatch " # f.name # ": " # debug_show (names)) };
  };
  check(manifestOk and total == 15, "scanEvents finds exactly the events the capture manifest lists in every fixture (15 events over 14 transactions; got " # debug_show (total) # ")");
};

func only(name : Text) : Events.Event {
  let found = Decode.scanEvents(tx(name), Fx.PROGRAM, Fx.EVENT_AUTHORITY);
  found[found.size() - 1].event;
};

do {
  switch (only("flows-007")) {
    case (#PositionOpened(p)) {
      check(p.owner == "HixcV1fZc9B9qqWRFThcs5heon6usJAFfbbaSHYXzniG" and p.nonce == 4895690605773200396 and p.round_id == 1 and p.close_id == null and p.plan == 0 and p.gross == 1_000_000_000 and p.load == 40_000_000 and p.config_version == 1,
        "PositionOpened body decodes (owner, u64 nonce above 2^62, round 1, close None, plan 0, gross 1 SOL, load, config 1)");
    };
    case (_) { check(false, "flows-007 is a PositionOpened") };
  };
  switch (only("closing-012")) {
    case (#PositionOpened(p)) { check(p.plan == 0 and p.gross == 100_000_000 and p.round_id == 2 and p.close_id == null, "PositionOpened after finalize: round 2, close None") };
    case (_) { check(false, "closing-012 is a PositionOpened") };
  };
  switch (only("flows-009")) {
    case (#SeriesAFunded(s)) {
      check(s.owner == "E3jUXb3RFs49VhzVqzjFpPDYQwF4evgc9buQXMN5yV2r" and s.round_id == 1 and s.close_id == null and s.gross == 50_000_000 and s.entitlement_after == 62_000_000, "SeriesAFunded body decodes (u128 entitlement_after)");
    };
    case (_) { check(false, "flows-009 is a SeriesAFunded") };
  };
  switch (only("closing-009")) {
    case (#PromotionApplied(p)) {
      check(p.round_id == 1 and p.close_id == ?1 and p.winner == "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew" and p.loss == 907_500_000 and p.entitlement == 1_052_700_000, "PromotionApplied body decodes (close Some(1), winner, loss, u128 entitlement)");
    };
    case (_) { check(false, "closing-009 is a PromotionApplied") };
  };
  switch (only("flows-008")) {
    case (#PayoutPaid(p)) {
      check(p.gross_paid == 7638 and p.toll == 916 and p.net == 6722 and not p.closing and p.close_id == null and p.ticket_id == 0 and p.destination == p.owner, "PayoutPaid (open round) decodes");
    };
    case (_) { check(false, "flows-008 is a PayoutPaid") };
  };
  switch (Decode.scanEvents(tx("closing-002"), Fx.PROGRAM, Fx.EVENT_AUTHORITY)[0].event) {
    case (#PayoutPaid(p)) { check(p.closing and p.close_id == ?1 and p.gross_paid == 100_000_000 and p.toll == 7_500_000 and p.net == 92_500_000, "PayoutPaid (closing) carries Some(close_id)") };
    case (_) { check(false, "closing-002 first event is a PayoutPaid") };
  };
  switch (only("flows-017")) {
    case (#BackerClaimed(c)) { check(c.round_id == 1 and c.amount == 609 and c.a_part == 609 and c.b_part == 0, "BackerClaimed decodes") };
    case (_) { check(false, "flows-017 is a BackerClaimed") };
  };
  switch (only("closing-010")) {
    case (#RoundFinalized(r)) { check(r.close_id == 1 and r.new_round == 2 and r.seed_carried == 7_500_000, "RoundFinalized decodes") };
    case (_) { check(false, "closing-010 is a RoundFinalized") };
  };
  switch (only("closing-002")) {
    case (#Other(o)) { check(o.name == "RoundClosed" and o.schema_version == 1 and o.body.size() == 25, "RoundClosed decodes to the generic record (name, schema, raw body)") };
    case (_) { check(false, "closing-002 last event is generic RoundClosed") };
  };
};

// -------------------------------------------------- event body strictness
func cpi(name : Text, body : [Nat8]) : Decode.EventCpi {
  var h = "";
  for ((n, d) in Events.EVENT_DISCRIMINATORS.vals()) { if (n == name) { h := d } };
  // hex -> bytes
  let cs = h.chars();
  var bytes : [Nat8] = [];
  var hi : Nat8 = 0;
  var odd = false;
  for (c in cs) {
    let v : Nat8 = switch (c) {
      case ('0') 0; case ('1') 1; case ('2') 2; case ('3') 3; case ('4') 4; case ('5') 5; case ('6') 6; case ('7') 7;
      case ('8') 8; case ('9') 9; case ('a') 10; case ('b') 11; case ('c') 12; case ('d') 13; case ('e') 14; case (_) 15;
    };
    if (odd) { bytes := Array.append(bytes, [hi * 16 + v]); odd := false } else { hi := v; odd := true };
  };
  { discriminator = Blob.fromArray(bytes); body = Blob.fromArray(body) };
};

do {
  check(Result.isOk(Decode.decodeEvent(cpi("RoundFinalized", Array.append<Nat8>([1], fill(24, 0))))), "decodeEvent: exact-size body accepted");
  check(Decode.decodeEvent(cpi("RoundFinalized", Array.append<Nat8>([1], fill(23, 0)))) == #err(#Malformed("RoundFinalized: truncated or invalid body")), "decodeEvent: truncated body rejected");
  check(Decode.decodeEvent(cpi("RoundFinalized", Array.append<Nat8>([1], fill(25, 0)))) == #err(#Malformed("RoundFinalized: trailing bytes")), "decodeEvent: trailing bytes rejected");
  check(Decode.decodeEvent(cpi("RoundFinalized", Array.append<Nat8>([2], fill(24, 0)))) == #err(#UnsupportedSchema(2)), "decodeEvent: schema_version 2 is reported unsupported, not mis-decoded");
  check(Decode.decodeEvent({ discriminator = Blob.fromArray(fill(8, 9)); body = Blob.fromArray([1]) }) == #err(#UnknownDiscriminator("0909090909090909")), "decodeEvent: unknown discriminator");
  // Option<u64> tag must be 0 or 1: PositionOpened with close_id tag 2.
  let badOpt = Array.flatten<Nat8>([[1], fill(32, 1), fill(8, 0), fill(8, 0), [2], [0], fill(8, 0), fill(8, 0), fill(4, 0)]);
  check(Decode.decodeEvent(cpi("PositionOpened", badOpt)) == #err(#Malformed("PositionOpened: truncated or invalid body")), "decodeEvent: Option tag other than 0/1 rejected");
  check(Decode.parseEventCpiData(Blob.fromArray(fill(16, 0))) == null, "parseEventCpiData: data without the event-CPI tag is not an event");
  check(Decode.parseEventCpiData(Blob.fromArray(Events.EVENT_IX_TAG)) == null, "parseEventCpiData: tag without a discriminator is not an event");
};

// --------------------------------------------- synthetic v0 wire + loaded
func rpcWith(bytes : [Nat8], version : ?{ #legacy; #number : Nat8 }, loaded : ?Rpc.LoadedAddresses, status : { #Ok; #Err : Rpc.TransactionError }, inner : ?[Rpc.InnerInstructions]) : Rpc.EncodedConfirmedTransactionWithStatusMeta {
  {
    slot = 99;
    blockTime = null;
    transaction = {
      meta = ?{
        fee = 5000; status; innerInstructions = inner; postTokenBalances = null; preBalances = []; postBalances = [];
        returnData = null; logMessages = null; rewards = null; loadedAddresses = loaded; preTokenBalances = null;
        computeUnitsConsumed = null; cost_units = null;
      };
      transaction = #binary(Decode.base58Encode(Blob.fromArray(bytes)), #base58);
      version;
    };
  };
};

func v0Bytes(prefix : Nat8) : [Nat8] {
  Array.flatten<Nat8>([
    [1], fill(64, 7), // 1 signature
    [prefix, 1, 0, 1], // v0 marker (or other), header
    [3], fill(32, 0x11), fill(32, 0x22), fill(32, 0x33), // 3 static keys
    fill(32, 0xaa), // blockhash
    [1, 1, 2, 0, 3, 2, 9, 9], // 1 ix: program idx 1, accounts [0,3], data [9,9]
    [1], fill(32, 0x44), [1, 5], [1, 6], // 1 lookup table: writable [5], readonly [6]
  ]);
};

do {
  let loaded : Rpc.LoadedAddresses = { writable = [key(0x55)]; readonly = [key(0x66)] };
  switch (Decode.fromRpc(rpcWith(v0Bytes(0x80), ?#number(0), ?loaded, #Ok, ?[]))) {
    case (#ok(t)) {
      check(t.version == #v0 and t.accountKeys == [key(0x11), key(0x22), key(0x33), key(0x55), key(0x66)], "v0 wire: account list is static keys ++ loaded writable ++ loaded readonly");
      check(t.signature == Decode.base58Encode(Blob.fromArray(fill(64, 7))) and t.numRequiredSignatures == 1 and t.instructions.size() == 1 and t.instructions[0].accounts == [0, 3] and Blob.toArray(t.instructions[0].data) == [9, 9], "v0 wire: signature, header and outer instruction parse");
    };
    case (#err(_)) { check(false, "v0 wire decodes") };
  };
  check(Decode.fromRpc(rpcWith(v0Bytes(0x80), ?#number(0), null, #Ok, ?[])) == #err(#Malformed("loadedAddresses do not match the message's lookup tables")), "v0 wire: missing loadedAddresses for a lookup table is malformed");
  check(Decode.fromRpc(rpcWith(v0Bytes(0x81), ?#number(1), ?loaded, #Ok, ?[])) == #err(#UnsupportedVersion(?1)), "transaction version 1 in the reply is an UnsupportedVersion gap");
  check(Decode.fromRpc(rpcWith(v0Bytes(0x81), null, ?loaded, #Ok, ?[])) == #err(#UnsupportedVersion(?1)), "version-1 wire prefix is an UnsupportedVersion gap even if the reply omits `version`");
  check(Decode.fromRpc(rpcWith(v0Bytes(0x80), ?#legacy, ?loaded, #Ok, ?[])) == #err(#Malformed("version mismatch")), "reply `version` that disagrees with the wire prefix is malformed");
  check(Decode.fromRpc(rpcWith(Array.append<Nat8>(v0Bytes(0x80), [0]), ?#number(0), ?loaded, #Ok, ?[])) == #err(#Malformed("trailing bytes")), "trailing bytes after the message are malformed");
  var truncatedAlwaysMalformed = true;
  let full = v0Bytes(0x80);
  var cut = 0;
  while (cut < full.size()) {
    switch (Decode.fromRpc(rpcWith(Array.subArray<Nat8>(full, 0, cut), ?#number(0), ?loaded, #Ok, ?[]))) {
      case (#err(#Malformed(_))) {};
      case (_) { truncatedAlwaysMalformed := false; Debug.print("  cut at " # debug_show (cut) # " was not Malformed") };
    };
    cut += 1;
  };
  check(truncatedAlwaysMalformed, "every proper prefix of a v0 transaction is malformed (never a partial parse)");
  check(Decode.fromRpc(rpcWith(v0Bytes(0x80), ?#number(0), ?loaded, #Ok, null)) == #err(#MissingInnerInstructions), "meta without innerInstructions cannot be verified");
  let f = Fx.get("flows-007").rpc;
  check(Decode.fromRpc({ f with transaction = { f.transaction with meta = null } }) == #err(#MissingMeta), "missing meta cannot be verified");
  switch (Decode.fromRpc(rpcWith(v0Bytes(0x80), ?#number(0), ?loaded, #Err(#AccountInUse), ?[]))) {
    case (#ok(t)) { check(not t.succeeded, "meta.status = Err maps to succeeded = false") };
    case (#err(_)) { check(false, "failed-status transaction still decodes") };
  };
  check(Decode.MAX_SUPPORTED_TX_VERSION == 0, "decoder supports exactly legacy + v0 (MAX_SUPPORTED_TX_VERSION = 0)");
};

// ----------------------------------------------- provider reply classes
do {
  let f = Fx.get("flows-007").rpc;
  let r : Rpc.CommitmentLevel = #finalized;
  switch (Decode.classify(r, #Consistent(#Ok(?f)))) {
    case (#Observed(o)) { check(o.commitment == #finalized and o.tx.signature == Fx.get("flows-007").json.signatures[0], "classify: Consistent Ok(tx) is an observation at the requested commitment") };
    case (_) { check(false, "classify Observed") };
  };
  check(Decode.classify(r, #Consistent(#Ok(null))) == #NotFound, "classify: Ok(null) is NotFound");
  check(Decode.classify(r, #Inconsistent([])) == #Inconsistent, "classify: Inconsistent is not an observation");
  check(Decode.classify(r, #Consistent(#Err(#JsonRpcError({ code = -32015; message = "Transaction version (1) is not supported by the requesting client" })))) == #Undecodable(#UnsupportedVersion(null)), "classify: JSON-RPC -32015 is the unsupported-version gap");
  switch (Decode.classify(r, #Consistent(#Err(#ValidationError("x"))))) {
    case (#Error(_)) { check(true, "classify: other RPC errors are transient provider errors") };
    case (_) { check(false, "classify Error") };
  };
  switch (Decode.classify(r, #Consistent(#Ok(?{ f with transaction = { f.transaction with version = ?#number(3) } })))) {
    case (#Undecodable(#UnsupportedVersion(?3))) { check(true, "classify: reply with transaction version 3 is the unsupported-version gap") };
    case (_) { check(false, "classify unsupported version") };
  };
};

if (failures > 0) { Debug.trap("decode.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("decode.test.mo: ALL PASS");
