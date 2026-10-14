// Standalone provenance tests for Verify (spec section 9 / C4 / F3).
//
// Run from the repo root:
//   moc -r --package base <base@0.16.0/src> native_sol_bridge/test/verify.test.mo
//
// Real data: transactions captured from two local-validator runs
// (solana/fixtures/rpc), fed through the SAME typed path a live call uses
// (wire-encoded transaction + typed meta -> Decode.classify), so the accept
// cases exercise the standard `innerInstructions` JSON-equivalent form
// (programIdIndex / accounts / data / stackHeight, no signer flags, the event
// authority absent from the signers). Forgery cases mutate a copy of the real
// decoded transaction (or the real descriptor) one field at a time and expect a
// specific rejection.
//
//   open ........ flows-007   (PositionOpened, Simple, owner = signer)
//   open #2 ..... closing-012 (PositionOpened after finalize, round 2)
//   Series A .... flows-009   (SeriesAFunded, owner = signer)
//   promotion ... closing-009 (PromotionApplied, close Some(1), winner = keeper here)
//   finalize .... closing-010 (RoundFinalized: a real event that is not a forwarded kind)

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Nat8 "mo:base/Nat8";
import Text "mo:base/Text";

import Decode "../Decode";
import Events "../Events";
import Rpc "../Rpc";
import Verify "../Verify";
import Fx "fixtures.test";

var failures : Nat = 0;
func check(cond : Bool, msg : Text) {
  if (cond) { Debug.print("PASS: " # msg) } else { Debug.print("FAIL: " # msg); failures += 1 };
};

let localDomain : Blob = Text.encodeUtf8(Fx.LOCALNET_DOMAIN_TEXT);
let devnetDomain : Blob = Text.encodeUtf8("musical-chairs-devnet-domain-v1!");

let cfg : Verify.Config = {
  program_id = Fx.PROGRAM;
  cluster_domain = localDomain;
  event_authority = Fx.EVENT_AUTHORITY;
  supported_schema_versions = [1];
};

// ------------------------------------------------------------------ data
func rpcOf(name : Text) : Rpc.EncodedConfirmedTransactionWithStatusMeta { Fx.get(name).rpc };

func txOf(name : Text) : Decode.Tx {
  switch (Decode.fromRpc(rpcOf(name))) {
    case (#ok(t)) { t };
    case (#err(_)) { Debug.trap("fixture failed to decode: " # name) };
  };
};

func obs(tx : Decode.Tx) : Decode.ProviderObservation { #Observed({ commitment = #finalized; tx }) };

/// Both providers return the same finalized transaction.
func verifyTx(d : Events.Descriptor, tx : Decode.Tx) : Verify.Verdict { Verify.verify(cfg, d, obs(tx), obs(tx)) };

func idOf(name : Text, ixIndex : Nat, innerIndex : Nat) : Events.EventId {
  {
    cluster_domain = localDomain;
    program_id = Fx.PROGRAM;
    signature = Fx.get(name).json.signatures[0];
    instruction_index = ixIndex;
    inner_instruction_index = innerIndex;
    event_schema_version = 1;
  };
};

let HIXC = "HixcV1fZc9B9qqWRFThcs5heon6usJAFfbbaSHYXzniG";
let E3JU = "E3jUXb3RFs49VhzVqzjFpPDYQwF4evgc9buQXMN5yV2r";
let BOW1 = "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew";
let OTHER = "8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR";

let openD : Events.Descriptor = {
  event_id = idOf("flows-007", 1, 4);
  source_round_id = 1; source_close_id = null; kind = #PositionOpened;
  beneficiary_pubkey = HIXC; basis_lamports = 1_000_000_000; plan = ?#Simple;
};
let open2D : Events.Descriptor = {
  event_id = idOf("closing-012", 1, 3);
  source_round_id = 2; source_close_id = null; kind = #PositionOpened;
  beneficiary_pubkey = BOW1; basis_lamports = 100_000_000; plan = ?#Simple;
};
let seriesAD : Events.Descriptor = {
  event_id = idOf("flows-009", 1, 3);
  source_round_id = 1; source_close_id = null; kind = #SeriesAFunded;
  beneficiary_pubkey = E3JU; basis_lamports = 50_000_000; plan = null;
};
let promoD : Events.Descriptor = {
  event_id = idOf("closing-009", 1, 3);
  source_round_id = 1; source_close_id = ?1; kind = #PromotionApplied;
  beneficiary_pubkey = BOW1; basis_lamports = 907_500_000; plan = null;
};

// ------------------------------------------------------- tx mutation tools
func setKey(tx : Decode.Tx, i : Nat, k : Text) : Decode.Tx {
  { tx with accountKeys = Array.tabulate<Text>(tx.accountKeys.size(), func(j) { if (j == i) { k } else { tx.accountKeys[j] } }) };
};

func mapInnerAt(tx : Decode.Tx, group : Nat, pos : Nat, f : Decode.InnerIx -> Decode.InnerIx) : Decode.Tx {
  {
    tx with inner = Array.map<Decode.InnerGroup, Decode.InnerGroup>(
      tx.inner,
      func(g) {
        if (g.index != group) { g } else {
          { g with instructions = Array.tabulate<Decode.InnerIx>(g.instructions.size(), func(j) { if (j == pos) { f(g.instructions[j]) } else { g.instructions[j] } }) };
        };
      },
    );
  };
};

func innerAt(tx : Decode.Tx, group : Nat, pos : Nat) : Decode.InnerIx {
  for (g in tx.inner.vals()) { if (g.index == group) { return g.instructions[pos] } };
  Debug.trap("no such inner group");
};

func dataBytes(ix : Decode.InnerIx) : [Nat8] {
  switch (Decode.base58Decode(ix.data)) { case (?b) { Blob.toArray(b) }; case (null) { Debug.trap("bad base58") } };
};

func withData(tx : Decode.Tx, group : Nat, pos : Nat, bytes : [Nat8]) : Decode.Tx {
  mapInnerAt(tx, group, pos, func(ix) { { ix with data = Decode.base58Encode(Blob.fromArray(bytes)) } });
};

func keyIndex(tx : Decode.Tx, k : Text) : Nat {
  var i = 0;
  for (x in tx.accountKeys.vals()) { if (x == k) { return i }; i += 1 };
  Debug.trap("key not in transaction: " # k);
};

func signers(tx : Decode.Tx) : [Text] { Array.tabulate<Text>(tx.numRequiredSignatures, func(i) { tx.accountKeys[i] }) };
func contains(xs : [Text], x : Text) : Bool { Array.find<Text>(xs, func(y) { y == x }) != null };

func isInvalid(v : Verify.Verdict, r : Verify.RejectReason) : Bool { v == #Invalid(r) };
func isUnverifiable(v : Verify.Verdict, u : Verify.Unverifiable) : Bool { v == #Unverifiable(u) };
func isVerified(v : Verify.Verdict) : Bool { switch (v) { case (#Verified(_)) true; case (_) false } };

// =====================================================================
// acceptsValidEventFromStandardRpcFixtureWithAuthorityAbsentFromSigners
// =====================================================================
do {
  Debug.print("== acceptsValidEventFromStandardRpcFixtureWithAuthorityAbsentFromSigners ==");
  // Everything below goes through Decode.classify of the typed reply, exactly as a live provider reply would.
  func provider(name : Text) : Decode.ProviderObservation { Decode.classify(#finalized, #Consistent(#Ok(?rpcOf(name)))) };
  let cases : [(Text, Events.Descriptor, Text)] = [
    ("flows-007", openD, "open (PositionOpened)"),
    ("closing-012", open2D, "open after finalize (PositionOpened, round 2)"),
    ("flows-009", seriesAD, "Series A (SeriesAFunded)"),
    ("closing-009", promoD, "promotion (PromotionApplied)"),
  ];
  for ((name, d, lbl) in cases.vals()) {
    let tx = txOf(name);
    let ix = innerAt(tx, d.event_id.instruction_index, d.event_id.inner_instruction_index);
    check(not contains(signers(tx), Fx.EVENT_AUTHORITY), lbl # ": event authority is NOT among the transaction signers (it is a PDA that only the program signs for)");
    check(ix.stackHeight == ?2 and tx.accountKeys[Nat8.toNat(ix.programIdIndex)] == Fx.PROGRAM and tx.accountKeys[Nat8.toNat(ix.accounts[0])] == Fx.EVENT_AUTHORITY, lbl # ": the event instruction is a stackHeight-2 inner instruction of the program with the authority as account 0");
    switch (Verify.verify(cfg, d, provider(name), provider(name))) {
      case (#Verified(v)) {
        check(v.forward == d, lbl # ": VERIFIED, and the forwarded descriptor equals the fixture-derived one field for field");
      };
      case (other) { check(false, lbl # ": verified (got " # debug_show (other) # ")") };
    };
  };
};

// =====================================================================
// descriptorRequiresTwoAgreeingFinalizedObservations
// =====================================================================
do {
  Debug.print("== descriptorRequiresTwoAgreeingFinalizedObservations ==");
  let tx = txOf("flows-007");
  let good = obs(tx);
  check(isVerified(Verify.verify(cfg, openD, good, good)), "two agreeing finalized observations verify");
  check(isUnverifiable(Verify.verify(cfg, openD, good, #Inconsistent), #Inconsistent), "a provider whose own sources are Inconsistent is not an observation");
  check(isUnverifiable(Verify.verify(cfg, openD, #NotFound, good), #NotFound), "one provider has no such transaction: pending, not verified");
  check(isUnverifiable(Verify.verify(cfg, openD, good, #Error("HTTP outcall failed")), #ProviderError("HTTP outcall failed")), "one provider errored: pending, not verified");
  // One provider disagrees (a different but individually well-formed transaction).
  let ixB = innerAt(tx, 1, 4);
  let bytes = dataBytes(ixB);
  let tampered = Array.tabulate<Nat8>(bytes.size(), func(i) { if (i + 1 == bytes.size()) { bytes[i] ^ 1 } else { bytes[i] } });
  let txB = withData(tx, 1, 4, tampered);
  check(isUnverifiable(Verify.verify(cfg, openD, good, obs(txB)), #Disagreement), "providers return different inner-instruction bytes: pending (Disagreement), not skipped");
  check(isUnverifiable(Verify.verify(cfg, openD, good, obs({ tx with slot = tx.slot + 1 })), #Disagreement), "providers report different slots: Disagreement");
  check(isUnverifiable(Verify.verify(cfg, openD, good, obs(setKey(tx, 3, Fx.EVENT_AUTHORITY))), #Disagreement), "providers report different account lists: Disagreement");
  check(isUnverifiable(Verify.verify(cfg, openD, good, #Observed({ commitment = #confirmed; tx })), #NotFinalized), "a confirmed (not finalized) observation does not count");
  check(isUnverifiable(Verify.verify(cfg, openD, #Observed({ commitment = #processed; tx }), good), #NotFinalized), "a processed observation does not count");
  // The tamper above hit `config_version`, a field no descriptor compares, so B alone would still verify:
  // agreement is judged on the whole transaction, not only on the descriptor's fields.
  check(isVerified(verifyTx(openD, txB)), "(control) the config_version-only tamper verifies on its own, so the Disagreement above comes from comparing providers");
  // Tamper a compared field (gross, 8 bytes at offset 67 of the instruction data).
  let grossTampered = Array.tabulate<Nat8>(bytes.size(), func(i) { if (i == 67) { bytes[i] ^ 1 } else { bytes[i] } });
  let txC = withData(tx, 1, 4, grossTampered);
  check(isUnverifiable(Verify.verify(cfg, openD, good, obs(txC)), #Disagreement), "providers disagree on the gross amount: Disagreement");
  check(isInvalid(verifyTx(openD, txC), #BasisMismatch), "(control) a lone tampered-gross observation fails the descriptor's basis");
};

// =====================================================================
// unsupportedTransactionVersion (verification half; the cursor half is in outbox.test.mo)
// =====================================================================
do {
  Debug.print("== unsupportedTransactionVersion ==");
  let good = obs(txOf("flows-007"));
  check(isUnverifiable(Verify.verify(cfg, openD, #Undecodable(#UnsupportedVersion(?1)), good), #UnsupportedTransactionVersion(?1)), "provider A sees a version-1 transaction: GAP (UnsupportedTransactionVersion)");
  check(isUnverifiable(Verify.verify(cfg, openD, #NotFound, #Undecodable(#UnsupportedVersion(?1))), #UnsupportedTransactionVersion(?1)), "a version gap wins over another provider's NotFound (the cursor must not pass it)");
  let viaRpcError = Decode.classify(#finalized, #Consistent(#Err(#JsonRpcError({ code = -32015; message = "Transaction version (1) is not supported by the requesting client" }))));
  check(isUnverifiable(Verify.verify(cfg, openD, viaRpcError, good), #UnsupportedTransactionVersion(null)), "JSON-RPC -32015 from a provider is the same GAP");
};

// =====================================================================
// rejectsForgedProgramId
// =====================================================================
do {
  Debug.print("== rejectsForgedProgramId ==");
  let tx = txOf("flows-007");
  let computeBudget = keyIndex(tx, "ComputeBudget111111111111111111111111111111");
  // (a) The inner event instruction invokes some other program.
  let forgedInner = mapInnerAt(tx, 1, 4, func(ix) { { ix with programIdIndex = Nat8.fromNat(computeBudget) } });
  check(isInvalid(verifyTx(openD, forgedInner), #ForgedProgramId), "inner event instruction whose program is not the configured program: ForgedProgramId");
  // (b) The outer instruction is a different program that CPIs into ours (wrapper / confused deputy).
  let wrapped = { tx with instructions = Array.tabulate<Decode.Ix>(tx.instructions.size(), func(i) { if (i == 1) { { tx.instructions[1] with programIdIndex = Nat8.fromNat(computeBudget) } } else { tx.instructions[i] } }) };
  check(isInvalid(verifyTx(openD, wrapped), #OuterProgramMismatch), "outer instruction is not the configured program: OuterProgramMismatch");
  // (c) The descriptor names a different program id.
  let forgedId = { openD with event_id = { openD.event_id with program_id = "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf" } };
  check(isInvalid(verifyTx(forgedId, tx), #ProgramIdMismatch), "descriptor program_id differs from the configured program: ProgramIdMismatch");
  // (d) An index that falls outside the account list.
  let oob = mapInnerAt(tx, 1, 4, func(ix) { { ix with programIdIndex = 200 } });
  check(isInvalid(verifyTx(openD, oob), #InvalidAccountIndex), "program index outside the account list: InvalidAccountIndex");
};

// =====================================================================
// rejectsForgedEventAuthority
// =====================================================================
do {
  Debug.print("== rejectsForgedEventAuthority ==");
  let tx = txOf("flows-007");
  // (a) account 0 of the event instruction is a different account (here: the fee payer).
  let a0 = mapInnerAt(tx, 1, 4, func(ix) { { ix with accounts = [0] } });
  check(isInvalid(verifyTx(openD, a0), #ForgedEventAuthority), "event instruction account 0 is not the event authority: ForgedEventAuthority");
  // (b) no accounts at all.
  let none = mapInnerAt(tx, 1, 4, func(ix) { { ix with accounts = [] } });
  check(isInvalid(verifyTx(openD, none), #ForgedEventAuthority), "event instruction with no accounts: ForgedEventAuthority");
  // (c) the transaction's authority is an attacker-chosen account, not the canonical PDA.
  let fakeAuthority = setKey(tx, keyIndex(tx, Fx.EVENT_AUTHORITY), OTHER);
  check(isInvalid(verifyTx(openD, fakeAuthority), #ForgedEventAuthority), "authority account is not the configured canonical PDA: ForgedEventAuthority");
  // (d) the real authority sits at account 1, not 0.
  let auth = keyIndex(tx, Fx.EVENT_AUTHORITY);
  let moved = mapInnerAt(tx, 1, 4, func(ix) { { ix with accounts = [0, Nat8.fromNat(auth)] } });
  check(isInvalid(verifyTx(openD, moved), #ForgedEventAuthority), "authority present only as a later account: ForgedEventAuthority");
  // (e) index out of range.
  let oob = mapInnerAt(tx, 1, 4, func(ix) { { ix with accounts = [250] } });
  check(isInvalid(verifyTx(openD, oob), #InvalidAccountIndex), "authority index outside the account list: InvalidAccountIndex");
};

// =====================================================================
// rejectsDirectTopLevelEventCpi
// =====================================================================
do {
  Debug.print("== rejectsDirectTopLevelEventCpi ==");
  let tx = txOf("flows-007");
  for (h in [?1, null, ?3].vals()) {
    let shallow = mapInnerAt(tx, 1, 4, func(ix) { { ix with stackHeight = h } });
    check(isInvalid(verifyTx(openD, shallow), #NotInnerCpi), "event instruction at stackHeight " # debug_show (h) # " is not an inner CPI directly under the program instruction: NotInnerCpi");
  };
  // A genuine top-level (stack height 1) instruction carrying event-CPI data: the program id,
  // authority and bytes are all "right", but it is an outer instruction, not a self-CPI.
  let ev = innerAt(tx, 1, 4);
  let prog = keyIndex(tx, Fx.PROGRAM);
  let auth = keyIndex(tx, Fx.EVENT_AUTHORITY);
  let top : Decode.Ix = { programIdIndex = Nat8.fromNat(prog); accounts = [Nat8.fromNat(auth)]; data = Blob.fromArray(dataBytes(ev)) };
  let withTop = { tx with instructions = Array.append<Decode.Ix>(tx.instructions, [top]) };
  let topD = { openD with event_id = { openD.event_id with instruction_index = tx.instructions.size(); inner_instruction_index = 0 } };
  check(isInvalid(verifyTx(topD, withTop), #WrongAncestry), "direct top-level event-CPI (an outer instruction, stack height 1) has no inner instruction to point at: WrongAncestry");
  // The same bytes cannot be redirected by pointing the descriptor at the real inner slot of a transaction that lacks it.
  let stripped = { tx with inner = [] };
  check(isInvalid(verifyTx(openD, stripped), #WrongAncestry), "transaction with no inner instructions: WrongAncestry");
};

// =====================================================================
// rejectsFailedTransaction
// =====================================================================
do {
  Debug.print("== rejectsFailedTransaction ==");
  let tx = txOf("flows-007");
  check(isInvalid(verifyTx(openD, { tx with succeeded = false }), #TransactionFailed), "meta.err != null: TransactionFailed (inner instructions of a failed transaction prove nothing)");
  // Same through the typed reply: meta.status = #Err.
  let r = rpcOf("flows-007");
  let failedReply = switch (r.transaction.meta) {
    case (?m) { { r with transaction = { r.transaction with meta = ?{ m with status = #Err(#InstructionError((1 : Nat8, #Custom(6000 : Nat32)))) } } } };
    case (null) { Debug.trap("fixture has meta") };
  };
  let p = Decode.classify(#finalized, #Consistent(#Ok(?failedReply)));
  check(isInvalid(Verify.verify(cfg, openD, p, p), #TransactionFailed), "a typed reply with status = Err(InstructionError ...) is rejected end to end");
  check(isInvalid(verifyTx(openD, { tx with signature = Fx.get("flows-009").json.signatures[0] }), #SignatureMismatch), "a provider that returns a different transaction than the one named is rejected: SignatureMismatch");
};

// =====================================================================
// rejectsWrongAncestryOrInnerIndex
// =====================================================================
do {
  Debug.print("== rejectsWrongAncestryOrInnerIndex ==");
  let tx = txOf("flows-007");
  // inner index 3 is a real system-program CPI in the same group (create account), not the event.
  let wrongInner = { openD with event_id = { openD.event_id with inner_instruction_index = 3 } };
  check(isInvalid(verifyTx(wrongInner, tx), #ForgedProgramId), "pointing at a neighbouring real CPI (system program) instead of the event: rejected");
  let pastEnd = { openD with event_id = { openD.event_id with inner_instruction_index = 5 } };
  check(isInvalid(verifyTx(pastEnd, tx), #WrongAncestry), "inner_instruction_index past the group: WrongAncestry");
  let wayPast = { openD with event_id = { openD.event_id with inner_instruction_index = 99 } };
  check(isInvalid(verifyTx(wayPast, tx), #WrongAncestry), "inner_instruction_index far past the group: WrongAncestry");
  // outer instruction 0 is ComputeBudget, not our program.
  let wrongOuter = { openD with event_id = { openD.event_id with instruction_index = 0 } };
  check(isInvalid(verifyTx(wrongOuter, tx), #OuterProgramMismatch), "instruction_index naming the ComputeBudget outer instruction: OuterProgramMismatch");
  let noOuter = { openD with event_id = { openD.event_id with instruction_index = 7 } };
  check(isInvalid(verifyTx(noOuter, tx), #WrongAncestry), "instruction_index past the outer instructions: WrongAncestry");
  // ambiguous group (two groups claiming the same outer index) is refused rather than guessed.
  let dup = { tx with inner = Array.append<Decode.InnerGroup>(tx.inner, [tx.inner[0]]) };
  check(isInvalid(verifyTx(openD, dup), #WrongAncestry), "two inner groups for one outer index: WrongAncestry");
  // pointing a promotion descriptor at the (real) open event of another transaction.
  check(isInvalid(verifyTx(promoD, tx), #SignatureMismatch), "descriptor whose signature belongs to a different transaction: SignatureMismatch");
  // a real, valid event but a different position than the descriptor's inner index (RoundFinalized sits at 0 in closing-010)
  let finalizeAsOpen = { openD with event_id = idOf("closing-010", 1, 0) };
  check(isInvalid(verifyTx(finalizeAsOpen, txOf("closing-010")), #KindMismatch), "descriptor claims PositionOpened but the event at that position is RoundFinalized: KindMismatch");
};

// =====================================================================
// rejectsSchemaOrClusterMismatch
// =====================================================================
do {
  Debug.print("== rejectsSchemaOrClusterMismatch ==");
  let tx = txOf("flows-007");
  let devnetD = { openD with event_id = { openD.event_id with cluster_domain = devnetDomain } };
  check(isInvalid(verifyTx(devnetD, tx), #ClusterMismatch), "descriptor cluster_domain differs from the configured cluster: ClusterMismatch");
  check(isInvalid(Verify.verify({ cfg with cluster_domain = devnetDomain }, openD, obs(tx), obs(tx)), #ClusterMismatch), "configured cluster differs from the descriptor's: ClusterMismatch");
  let v2D = { openD with event_id = { openD.event_id with event_schema_version = 2 } };
  check(isUnverifiable(verifyTx(v2D, tx), #UnsupportedSchema(2)), "descriptor at schema version 2 on a build that supports [1]: pending (UnsupportedSchema), not forwarded, not skipped");
  check(isInvalid(Verify.verify({ cfg with supported_schema_versions = [1, 2] }, v2D, obs(tx), obs(tx)), #SchemaMismatch), "descriptor says schema 2 but the event body says 1: SchemaMismatch");
  // The event body itself at schema 2 (first body byte 2), descriptor honest about it, build knows [1, 2] but cannot decode 2.
  let bytes = dataBytes(innerAt(tx, 1, 4));
  let bumped = Array.tabulate<Nat8>(bytes.size(), func(i) { if (i == 16) { 2 } else { bytes[i] } });
  let tx2 = withData(tx, 1, 4, bumped);
  check(isUnverifiable(Verify.verify({ cfg with supported_schema_versions = [1, 2] }, v2D, obs(tx2), obs(tx2)), #UnsupportedSchema(2)), "event body at schema 2 that this build cannot decode: pending (UnsupportedSchema)");
  check(isInvalid(verifyTx(openD, tx2), #SchemaMismatch), "event body bumped to schema 2 under a descriptor claiming schema 1: SchemaMismatch");
};

// =====================================================================
// rejectsDescriptorRecipientBasisRoundOrCloseMismatch
// =====================================================================
do {
  Debug.print("== rejectsDescriptorRecipientBasisRoundOrCloseMismatch ==");
  let o = txOf("flows-007");
  check(isInvalid(verifyTx({ openD with beneficiary_pubkey = OTHER }, o), #RecipientMismatch), "open: wrong beneficiary: RecipientMismatch");
  check(isInvalid(verifyTx({ openD with beneficiary_pubkey = Fx.PROGRAM }, o), #RecipientMismatch), "open: keeper/program as beneficiary: RecipientMismatch");
  check(isInvalid(verifyTx({ openD with basis_lamports = 1_000_000_001 }, o), #BasisMismatch), "open: basis off by one lamport: BasisMismatch");
  check(isInvalid(verifyTx({ openD with basis_lamports = 960_000_000 }, o), #BasisMismatch), "open: NET (after Front-End Load) instead of gross: BasisMismatch");
  check(isInvalid(verifyTx({ openD with source_round_id = 2 }, o), #RoundMismatch), "open: wrong round: RoundMismatch");
  check(isInvalid(verifyTx({ openD with source_close_id = ?1 }, o), #CloseMismatch), "open: Some(close) where the event carries None: CloseMismatch");
  check(isInvalid(verifyTx({ openD with plan = ?#Compound30 }, o), #PlanMismatch), "open: Compound30 claimed for a Simple position: PlanMismatch");
  check(isInvalid(verifyTx({ openD with plan = null }, o), #PlanMismatch), "open: plan omitted: PlanMismatch");
  check(isInvalid(verifyTx({ openD with kind = #SeriesAFunded }, o), #KindMismatch), "open: kind SeriesAFunded: KindMismatch");
  let s = txOf("flows-009");
  check(isInvalid(verifyTx({ seriesAD with beneficiary_pubkey = OTHER }, s), #RecipientMismatch), "Series A: wrong beneficiary: RecipientMismatch");
  check(isInvalid(verifyTx({ seriesAD with basis_lamports = 50_000_001 }, s), #BasisMismatch), "Series A: wrong basis: BasisMismatch");
  check(isInvalid(verifyTx({ seriesAD with source_round_id = 9 }, s), #RoundMismatch), "Series A: wrong round: RoundMismatch");
  check(isInvalid(verifyTx({ seriesAD with source_close_id = ?1 }, s), #CloseMismatch), "Series A: Some(close): CloseMismatch");
  check(isInvalid(verifyTx({ seriesAD with plan = ?#Simple }, s), #PlanMismatch), "Series A: a plan on a funding event: PlanMismatch");
  let p = txOf("closing-009");
  check(isInvalid(verifyTx({ promoD with beneficiary_pubkey = OTHER }, p), #RecipientMismatch), "promotion: wrong beneficiary: RecipientMismatch");
  check(isInvalid(verifyTx({ promoD with basis_lamports = 1_052_700_000 }, p), #BasisMismatch), "promotion: entitlement instead of the loss as basis: BasisMismatch");
  check(isInvalid(verifyTx({ promoD with source_round_id = 2 }, p), #RoundMismatch), "promotion: wrong round: RoundMismatch");
  check(isInvalid(verifyTx({ promoD with source_close_id = ?2 }, p), #CloseMismatch), "promotion: wrong close: CloseMismatch");
  check(isInvalid(verifyTx({ promoD with plan = ?#Simple }, p), #PlanMismatch), "promotion: a plan on a promotion: PlanMismatch");
  check(isInvalid(verifyTx({ promoD with kind = #PositionOpened }, p), #KindMismatch), "promotion: kind PositionOpened: KindMismatch");
};

// =====================================================================
// promotionEventCarriesSomeCloseIdAndOpenOrFundingCarryNone
// =====================================================================
do {
  Debug.print("== promotionEventCarriesSomeCloseIdAndOpenOrFundingCarryNone ==");
  // From the real transactions: Some for the promotion, None for the opening and the funding.
  let found = func(name : Text) : Events.Event {
    let f = Decode.scanEvents(txOf(name), Fx.PROGRAM, Fx.EVENT_AUTHORITY);
    f[f.size() - 1].event;
  };
  switch (found("closing-009")) { case (#PromotionApplied(p)) { check(p.close_id == ?1, "real PromotionApplied carries Some(close_id)") }; case (_) { check(false, "promotion event") } };
  switch (found("flows-007")) { case (#PositionOpened(p)) { check(p.close_id == null, "real PositionOpened carries None") }; case (_) { check(false, "open event") } };
  switch (found("flows-009")) { case (#SeriesAFunded(s)) { check(s.close_id == null, "real SeriesAFunded carries None") }; case (_) { check(false, "funding event") } };
  check(Events.closeShapeOk(#PromotionApplied, ?7) and not Events.closeShapeOk(#PromotionApplied, null) and Events.closeShapeOk(#PositionOpened, null) and not Events.closeShapeOk(#PositionOpened, ?7) and Events.closeShapeOk(#SeriesAFunded, null) and not Events.closeShapeOk(#SeriesAFunded, ?7), "close shape rule: promotion Some, open and funding None");
  // A promotion whose body says close_id = None (impossible from the program) is rejected even with a matching descriptor.
  let ptx = txOf("closing-009");
  let pb = dataBytes(innerAt(ptx, 1, 3));
  // data = tag(8) disc(8) | sv(1) round(8) opt(1)=1 close(8) winner(32) loss(8) entitlement(16)
  let noClose = Array.flatten<Nat8>([Array.subArray<Nat8>(pb, 0, 25), [0], Array.subArray<Nat8>(pb, 34, pb.size() - 34)]);
  let ptxNone = withData(ptx, 1, 3, noClose);
  check(isInvalid(verifyTx({ promoD with source_close_id = null }, ptxNone), #MalformedEvent("close_id shape")), "promotion event with close_id None is malformed even if the descriptor agrees");
  // An opening whose body claims Some(close).
  let otx = txOf("flows-007");
  let ob = dataBytes(innerAt(otx, 1, 4));
  // data = tag disc | sv(1) owner(32) nonce(8) round(8) opt(1)=0 plan ...
  let withClose = Array.flatten<Nat8>([Array.subArray<Nat8>(ob, 0, 65), [1, 1, 0, 0, 0, 0, 0, 0, 0], Array.subArray<Nat8>(ob, 66, ob.size() - 66)]);
  let otxSome = withData(otx, 1, 4, withClose);
  check(isInvalid(verifyTx({ openD with source_close_id = ?1 }, otxSome), #MalformedEvent("close_id shape")), "opening event with Some(close_id) is malformed even if the descriptor agrees");
};

// =====================================================================
// promotionBeneficiaryComesFromEventNotKeeperSigner
// =====================================================================
do {
  Debug.print("== promotionBeneficiaryComesFromEventNotKeeperSigner ==");
  let ptx = txOf("closing-009");
  check(signers(ptx)[0] == BOW1, "(fixture) in the local run the keeper wallet and the promotion winner are the same key");
  // A different keeper signs; the event's winner is unchanged.
  let otherKeeper = setKey(ptx, 0, OTHER);
  switch (verifyTx(promoD, otherKeeper)) {
    case (#Verified(v)) {
      check(v.forward.beneficiary_pubkey == BOW1 and signers(otherKeeper)[0] == OTHER, "promotion verifies with a different keeper as signer, and the forwarded beneficiary is the event's winner");
    };
    case (other) { check(false, "promotion with a different keeper signer verifies (got " # debug_show (other) # ")") };
  };
  // The winner need not sign at all, and a descriptor naming the keeper as beneficiary is rejected.
  check(isInvalid(verifyTx({ promoD with beneficiary_pubkey = OTHER }, otherKeeper), #RecipientMismatch), "descriptor naming the keeper signer as beneficiary is rejected: RecipientMismatch");
  check(not contains(signers(otherKeeper), BOW1), "(control) the winner is not a signer in that variant");
};

// =====================================================================
// ownerEventRequiresOwnerAmongSigners
// =====================================================================
do {
  Debug.print("== ownerEventRequiresOwnerAmongSigners ==");
  let o = txOf("flows-007");
  check(signers(o)[0] == HIXC, "(fixture) the owner is the fee payer / sole signer");
  // Someone else signs; the owner key is still present in the account list but only as a non-signer.
  let moved = setKey(setKey(o, 0, OTHER), o.accountKeys.size() - 1, HIXC);
  check(isInvalid(verifyTx(openD, moved), #OwnerNotSigner), "PositionOpened whose owner appears in the account list only as a non-signer: OwnerNotSigner");
  check(isInvalid(verifyTx(openD, setKey(o, 0, OTHER)), #OwnerNotSigner), "PositionOpened signed by someone else (owner absent): OwnerNotSigner");
  // Positive controls: the owner as the second of two signers is fine.
  let twoSigners = setKey({ o with numRequiredSignatures = 2 }, 1, HIXC);
  check(isVerified(verifyTx(openD, twoSigners)), "owner among two signers (not first) verifies");
  let s = txOf("flows-009");
  check(isInvalid(verifyTx(seriesAD, setKey(s, 0, OTHER)), #OwnerNotSigner), "SeriesAFunded signed by someone else: OwnerNotSigner");
  check(isVerified(verifyTx(seriesAD, s)), "SeriesAFunded signed by its owner verifies");
};

if (failures > 0) { Debug.trap("verify.test.mo: " # debug_show (failures) # " check(s) FAILED") };
Debug.print("verify.test.mo: ALL PASS");
