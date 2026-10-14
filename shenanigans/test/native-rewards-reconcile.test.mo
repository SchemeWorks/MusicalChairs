// Standalone tests for NativeRewards ledger-history reconciliation, the
// minting-account precondition, receipt forgery rejection, the SHA-256 memo
// derivation, and the archive selector (spec section 9, step 3 final clauses).
//
// Run: moc -r --package base <dfx-base> shenanigans/test/native-rewards-reconcile.test.mo

import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Array "mo:base/Array";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat64 "mo:base/Nat64";
import Int "mo:base/Int";
import Text "mo:base/Text";
import OrderedMap "mo:base/OrderedMap";
import NR "../NativeRewards";

// ---------------------------------------------------------------- fixtures

let SELF = Principal.fromText("j56tm-oaaaa-aaaac-qf34q-cai");
let LEDGER = Principal.fromText("5xv2o-iiaaa-aaaac-qeclq-cai");
let ARCHIVE = Principal.fromText("rdmx6-jaaaa-aaaaa-aaadq-cai");
let alice = Principal.fromText("u7zgw-triai-opf3l-hs7dz-2bf2t-j554r-mnzkx-74hou-6drz5-nati3-bqe");
let bob = Principal.fromText("mi66c-zqlu4-4kxd6-2gtp7-szg5v-6a62a-geoty-fahu5-4trje-xyfby-wqe");
let carol = Principal.fromText("gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe");
let house = Principal.fromText("zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae");
let attacker = Principal.fromText("stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe");

let principalMap = OrderedMap.Make<Principal>(Principal.compare);

let NS : Nat = 1_000_000_000;
let T0 : Int = 1_800_000_000 * NS;
let TBIND : Int = T0 + NS;
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

func reserved() : (NR.State, NR.EventId) {
  let id = eventId("sigRec");
  let ev : NR.NativeEvent = {
    event_id = id; source_round_id = 3; source_close_id = null; kind = #PositionOpened;
    beneficiary_pubkey = PUBKEY_A; basis_lamports = LAMPORTS; plan = ?#Simple;
  };
  let s0 = switch (NR.appendPolicy(NR.emptyState(), draft(), T0)) { case (#Ok r) { r.state }; case (#Err e) { Debug.trap(e) } };
  let (s1, _) = NR.observe(s0, ev, T0);
  let markers = principalMap.put(principalMap.empty<Int>(), alice, T0 - 1000);
  let out = NR.applyBind(s1, { self = SELF; signupMarkers = markers; effects = noEffects }, id, ?alice, TBIND);
  (out.state, id);
};

func legOf(s : NR.State, id : NR.EventId, i : Nat) : NR.Leg {
  switch (NR.getObligation(s, id)) { case (?o) { o.legs[i] }; case null { Debug.trap("no obligation") } };
};
func statusOf(s : NR.State, id : NR.EventId) : NR.ObligationStatus {
  switch (NR.getObligation(s, id)) { case (?o) { o.status }; case null { Debug.trap("no obligation") } };
};

// put leg i into SubmittedUnknown (the lost-outcome case)
func inFlight(s : NR.State, id : NR.EventId, i : Nat) : NR.State {
  let (s1, _) = NR.beginExecute(s, id, i, TBIND + 10 * NS);
  s1;
};

let goodMinter : NR.Account = { owner = SELF; subaccount = null };

let env : NR.ReconcileEnv = {
  ledger = LEDGER;
  archives = [ARCHIVE];
  configuredMinter = goodMinter;
  reportedMinter = ?goodMinter;
};

func mintTx(l : NR.Leg) : NR.LedgerTx = {
  kind = "mint";
  mint = ?{ to = l.recipient; amount = l.amount; memo = ?l.memo; created_at_time = ?l.created_at_time };
  transfer = null;
};

func receipt(l : NR.Leg, block : Nat) : NR.LedgerReceipt = { source = LEDGER; block_index = block; tx = ?mintTx(l) };

func rec(s : NR.State, id : NR.EventId, i : Nat, block : Nat, r : NR.LedgerReceipt, e : NR.ReconcileEnv) : (NR.State, NR.ReconcileResult) =
  NR.reconcileFromLedgerBlock(s, id, i, block, r, e, TBIND + 3_600 * NS);

func isRejected(r : NR.ReconcileResult, needle : Text) : Bool =
  switch (r) { case (#Rejected m) { Text.contains(m, #text needle) }; case _ { false } };

func isCommitted(r : NR.ReconcileResult, block : Nat) : Bool =
  switch (r) { case (#Committed x) { x.status == #Committed({ block }) }; case _ { false } };

func hex(b : Blob) : Text {
  let digits = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];
  var t = "";
  for (byte in b.vals()) { t #= digits[Nat8.toNat(byte / 16)] # digits[Nat8.toNat(byte % 16)] };
  t;
};

// ---------------------------------------------------------------- tests

// SHA-256 known answers (FIPS 180-4 / NIST vectors)
do {
  check(hex(NR.sha256(Text.encodeUtf8(""))) == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "sha256(\"\")");
  check(hex(NR.sha256(Text.encodeUtf8("abc"))) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "sha256(\"abc\")");
  check(hex(NR.sha256(Text.encodeUtf8("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"))) == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1", "sha256 two-block message");
  let big = Text.join("", Array.tabulate<Text>(1000, func _ = "a").vals()); // 1000 x "a"
  check(hex(NR.sha256(Text.encodeUtf8(big))) == "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3", "sha256 of 1000 x 'a' (multi-block)");
};

// leg memo = sha256(domain || 0x00 || event key || 0x00 || leg index u32 big-endian)
do {
  let id = eventId("sigRec");
  let key = NR.eventKey(id);
  let m0 = NR.legMemo(key, 0);
  let m1 = NR.legMemo(key, 1);
  check(m0.size() == 32 and m1.size() == 32, "leg memos are exactly 32 bytes");
  check(m0 != m1, "different leg indexes -> different memos");
  check(NR.legMemo(key, 0) == m0, "memo derivation is deterministic");
  check(NR.legMemo(NR.eventKey(eventId("other")), 0) != m0, "different event ids -> different memos");
  let expectedPreimage = Text.encodeUtf8(NR.MEMO_DOMAIN # "\00" # key # "\00") ;
  let withIndex = Array.append<Nat8>(Blob.toArray(expectedPreimage), [0, 0, 0, 1]);
  check(m1 == NR.sha256(Blob.fromArray(withIndex)), "memo preimage is domain || 0x00 || key || 0x00 || be32(index)");
  let (s, rid) = reserved();
  check(legOf(s, rid, 2).memo == NR.legMemo(NR.eventKey(rid), 2), "stored leg memo matches the derivation");
};

// event key is canonical and unambiguous over the id tuple
do {
  let a = eventId("sigRec");
  let b = { a with instruction_index = 1 };
  let c = { a with inner_instruction_index = 1 };
  let d = { a with cluster_domain = Text.encodeUtf8("solana-mainnet") };
  let e = { a with event_schema_version = 2 };
  let keys = [NR.eventKey(a), NR.eventKey(b), NR.eventKey(c), NR.eventKey(d), NR.eventKey(e)];
  var distinct = true;
  for (i in keys.keys()) { for (j in keys.keys()) { if (i != j and keys[i] == keys[j]) { distinct := false } } };
  check(distinct, "every component of the event id tuple changes the key");
};

Debug.print("== reconcileRequiresKindMintToSubaccountAmountMemoAndTimeExactly");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  let (s1, r1) = rec(s, id, 0, 321, receipt(leg, 321), env);
  check(isCommitted(r1, 321), "exact mint receipt from the ledger commits the SubmittedUnknown leg");
  check(legOf(s1, id, 0).status == #Committed({ block = 321 }), "stored leg is Committed{321}");

  func mutated(f : NR.LedgerMint -> NR.LedgerMint) : NR.LedgerReceipt {
    let m = switch (mintTx(leg).mint) { case (?m) { m }; case null { Debug.trap("mint") } };
    { source = LEDGER; block_index = 321; tx = ?{ kind = "mint"; mint = ?f(m); transfer = null } };
  };
  let cases : [(Text, NR.LedgerReceipt)] = [
    ("owner", mutated(func m = { m with to = { owner = attacker; subaccount = m.to.subaccount } })),
    ("subaccount: default instead of chip", mutated(func m = { m with to = { owner = m.to.owner; subaccount = null } })),
    ("subaccount: another player's chip", mutated(func m = { m with to = { owner = m.to.owner; subaccount = legOf(s, id, 1).recipient.subaccount } })),
    ("amount + 1", mutated(func m = { m with amount = m.amount + 1 })),
    ("amount - 1", mutated(func m = { m with amount = m.amount - 1 })),
    ("memo: other leg's memo", mutated(func m = { m with memo = ?legOf(s, id, 1).memo })),
    ("memo: absent", mutated(func m = { m with memo = null })),
    ("created_at_time + 1ns", mutated(func m = { m with created_at_time = switch (m.created_at_time) { case (?t) { ?(t + 1) }; case null { null } } })),
    ("created_at_time: absent", mutated(func m = { m with created_at_time = null })),
  ];
  for ((name, r) in cases.vals()) {
    let (s2, res) = rec(s, id, 0, 321, r, env);
    check((switch (res) { case (#Rejected _) { true }; case _ { false } }) and legOf(s2, id, 0).status == #SubmittedUnknown, "mismatch in " # name # " -> rejected, leg unchanged");
  };
  // reconcile is idempotent and never rewrites a committed block
  let (s3, r2) = rec(s1, id, 0, 321, receipt(leg, 321), env);
  check(isCommitted(r2, 321) and (switch (r2) { case (#Committed x) { not x.newlyCommitted }; case _ { false } }), "re-reconciling a committed leg is an idempotent no-op");
  let (s4, r3) = rec(s3, id, 0, 999, receipt(leg, 999), env);
  check(isCommitted(r3, 321) and legOf(s4, id, 0).status == #Committed({ block = 321 }), "a different block for an already committed leg never rewrites it");
};

Debug.print("== tooOldQuarantinesUntilLedgerHistoryReconcilesWithIdenticalFields");
do {
  let (s0, id) = reserved();
  let (s1, _) = NR.beginExecute(s0, id, 0, TBIND + 10 * NS);
  let (s2, qr) = NR.applyTransferOutcome(s1, id, 0, #Err(#TooOld), TBIND + 11 * NS);
  check(switch (qr.status) { case (#Quarantined q) { q.recoverable } ; case _ { false } }, "TooOld -> recoverable quarantine");
  // complete the siblings so the obligation shows PartiallyQuarantined
  var st = s2;
  for (i in [1, 2, 3].vals()) {
    let (a, _) = NR.beginExecute(st, id, i, TBIND + 12 * NS);
    let (b, _) = NR.applyTransferOutcome(a, id, i, #Ok(100 + i), TBIND + 13 * NS);
    st := b;
  };
  check(statusOf(st, id) == #PartiallyQuarantined, "obligation is #PartiallyQuarantined while the TooOld leg waits for history");
  let leg = legOf(st, id, 0);
  // receipt with a one-nanosecond different created_at_time (e.g. a replacement stamp) does not satisfy it
  func bumpTime(r : NR.LedgerReceipt) : NR.LedgerReceipt {
    switch (r.tx) {
      case (?t) {
        switch (t.mint) {
          case (?m) {
            let t2 : ?Nat64 = switch (m.created_at_time) { case (?x) { ?(x + 1) }; case null { null } };
            let m2 : NR.LedgerMint = { m with created_at_time = t2 };
            { r with tx = ?{ t with mint = ?m2 } };
          };
          case null { r };
        };
      };
      case null { r };
    };
  };
  let (st2, bad) = rec(st, id, 0, 55, bumpTime(receipt(leg, 55)), env);
  check((switch (bad) { case (#Rejected _) { true }; case _ { false } }) and statusOf(st2, id) == #PartiallyQuarantined, "a mint with a replacement timestamp does not reconcile the leg");
  let (st3, good) = rec(st, id, 0, 55, receipt(leg, 55), env);
  check(isCommitted(good, 55) and statusOf(st3, id) == #Completed, "ledger history with identical fields reconciles the quarantined leg and completes the obligation");
  // permanent quarantine can be reconciled only by an exact receipt too (never by assertion)
  let (p1, _) = NR.beginExecute(s0, id, 1, TBIND + 10 * NS);
  let (p2, _) = NR.applyTransferOutcome(p1, id, 1, #Err(#BadBurn({ min_burn_amount = 1 })), TBIND + 11 * NS);
  let (_, pr) = rec(p2, id, 1, 7, { source = LEDGER; block_index = 7; tx = null }, env);
  check(switch (pr) { case (#Rejected _) { true }; case _ { false } }, "an empty block never reconciles a permanently quarantined leg");
};

Debug.print("== mintingAccountVerifiedAgainstIcrc1MintingAccountBeforeReceiptAcceptance");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  let perfect = receipt(leg, 12);
  func withReported(r : ?NR.Account) : NR.ReconcileEnv = { env with reportedMinter = r };
  let variants : [(Text, ?NR.Account)] = [
    ("ledger reports a different owner", ?{ owner = attacker; subaccount = null }),
    ("ledger reports a minting subaccount", ?{ owner = SELF; subaccount = ?Blob.fromArray([1, 2, 3]) }),
    ("ledger reports no minting account", null),
  ];
  for ((name, rep) in variants.vals()) {
    let (s2, res) = rec(s, id, 0, 12, perfect, withReported(rep));
    check(isRejected(res, "minting account") and legOf(s2, id, 0).status == #SubmittedUnknown, name # " -> every receipt rejected");
  };
  let wrongConfigured : NR.ReconcileEnv = { env with configuredMinter = { owner = attacker; subaccount = null } };
  let (_, res2) = rec(s, id, 0, 12, perfect, wrongConfigured);
  check(isRejected(res2, "minting account"), "a configured minting account that differs from the ledger's rejects the receipt");
  let (_, res3) = rec(s, id, 0, 12, perfect, env);
  check(isCommitted(res3, 12), "with the verified minting account the same receipt commits");
};

Debug.print("== wrongTransactionKindRejectsReceipt");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  let asMint = mintTx(leg);
  for (kind in ["transfer", "burn", "approve", "", "MINT", "mint "].vals()) {
    let tx : NR.LedgerTx = { asMint with kind = kind };
    let (s2, res) = rec(s, id, 0, 5, { source = LEDGER; block_index = 5; tx = ?tx }, env);
    check((switch (res) { case (#Rejected _) { true }; case _ { false } }) and legOf(s2, id, 0).status == #SubmittedUnknown, "kind \"" # kind # "\" with mint-shaped fields -> rejected");
  };
  let hollow : NR.LedgerTx = { kind = "mint"; mint = null; transfer = null };
  let (_, res) = rec(s, id, 0, 5, { source = LEDGER; block_index = 5; tx = ?hollow }, env);
  check(switch (res) { case (#Rejected _) { true }; case _ { false } }, "kind \"mint\" with no mint body -> rejected");
};

Debug.print("== thirdPartySelfFundedTransferWithSameFieldsDoesNotCommit");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  // an attacker funds the exact recipient/amount/memo/time with a real transfer from their own balance
  let tx : NR.LedgerTx = {
    kind = "transfer";
    mint = null;
    transfer = ?{ from = { owner = attacker; subaccount = null }; to = leg.recipient; amount = leg.amount; memo = ?leg.memo; created_at_time = ?leg.created_at_time };
  };
  let (s2, res) = rec(s, id, 0, 9, { source = LEDGER; block_index = 9; tx = ?tx }, env);
  check(isRejected(res, "kind") and legOf(s2, id, 0).status == #SubmittedUnknown, "a third-party self-funded transfer with identical fields does not commit");
  // even if it claims to be funded by the minting account itself, kind != mint (the ledger would label a real mint as mint)
  let tx2 : NR.LedgerTx = { tx with transfer = switch (tx.transfer) { case (?t) { ?{ t with from = goodMinter } }; case null { null } } };
  let (_, res2) = rec(s, id, 0, 9, { source = LEDGER; block_index = 9; tx = ?tx2 }, env);
  check(switch (res2) { case (#Rejected _) { true }; case _ { false } }, "a transfer-kind record never satisfies a mint obligation");
};

Debug.print("== forgedArchiveCallbackNotListedByArchivesIsRejected");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  let fromEvil : NR.LedgerReceipt = { receipt(leg, 40) with source = attacker };
  let (s2, res) = rec(s, id, 0, 40, fromEvil, env);
  check(isRejected(res, "archive") and legOf(s2, id, 0).status == #SubmittedUnknown, "a receipt sourced from a canister not listed by archives() is rejected");
  let fromListed : NR.LedgerReceipt = { receipt(leg, 40) with source = ARCHIVE };
  let (_, res2) = rec(s, id, 0, 40, fromListed, env);
  check(isCommitted(res2, 40), "the same receipt from a listed archive commits");
  let noArchives : NR.ReconcileEnv = { env with archives = [] };
  let (_, res3) = rec(s, id, 0, 40, fromListed, noArchives);
  check(isRejected(res3, "archive"), "once archives() no longer lists it, the archive is rejected");
  // archive selection from the ledger's archives() listing
  let infos : [NR.ArchiveInfo] = [
    { canister_id = ARCHIVE; block_range_start = 0; block_range_end = 999 },
    { canister_id = carol; block_range_start = 1_000; block_range_end = 1_999 },
  ];
  check(NR.selectArchive(infos, 0) == ?ARCHIVE and NR.selectArchive(infos, 999) == ?ARCHIVE, "selectArchive: inclusive range edges map to the listed archive");
  check(NR.selectArchive(infos, 1_000) == ?carol and NR.selectArchive(infos, 1_999) == ?carol, "selectArchive: second archive");
  check(NR.selectArchive(infos, 2_000) == null, "selectArchive: a block past every listed range -> null (the ledger itself must hold it)");
};

Debug.print("== forgedReceiptBlockIndexDoesNotCommit");
do {
  let (s0, id) = reserved();
  let s = inFlight(s0, id, 0);
  let leg = legOf(s, id, 0);
  // caller asks about block 70 but the source answered for block 71
  let (s1, res1) = rec(s, id, 0, 70, receipt(leg, 71), env);
  check(isRejected(res1, "block") and legOf(s1, id, 0).status == #SubmittedUnknown, "receipt for a different block index than requested -> rejected");
  // block beyond the log (no tx at that index)
  let (_, res2) = rec(s, id, 0, 10_000_000, { source = LEDGER; block_index = 10_000_000; tx = null }, env);
  check(isRejected(res2, "no transaction"), "a block index with no transaction -> rejected");
  // a real, unrelated mint at some block (another leg's receipt) cannot be used for this leg
  let other = legOf(s, id, 1);
  let (_, res3) = rec(s, id, 0, 33, receipt(other, 33), env);
  check(switch (res3) { case (#Rejected _) { true }; case _ { false } }, "another leg's genuine mint cannot satisfy this leg");
  // unknown leg / unknown event
  let (_, res4) = rec(s, id, 9, 33, receipt(leg, 33), env);
  check(res4 == #UnknownLeg, "unknown leg index -> #UnknownLeg");
};

// receipt from the ledger commits a Retryable leg too, and completes the obligation
do {
  let (s0, id) = reserved();
  var st = s0;
  for (i in [0, 1, 2, 3].vals()) {
    let (a, _) = NR.beginExecute(st, id, i, TBIND + 10 * NS);
    let o : NR.TransferOutcome = if (i == 2) { #Err(#BadFee({ expected_fee = 1 })) } else { #Ok(200 + i) };
    let (b, _) = NR.applyTransferOutcome(a, id, i, o, TBIND + 11 * NS);
    st := b;
  };
  check(switch (legOf(st, id, 2).status) { case (#Retryable _) { true }; case _ { false } }, "leg 2 is Retryable after BadFee");
  let (st2, res) = rec(st, id, 2, 301, receipt(legOf(st, id, 2), 301), env);
  check(isCommitted(res, 301) and statusOf(st2, id) == #Completed, "a verified mint receipt commits a Retryable leg and completes the obligation");
};

if (ok) { Debug.print("ALL PASS (" # Nat.toText(passed) # " checks)") } else { Debug.trap("TEST FAILURES") };
