import Array "mo:base/Array";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";

import Decode "Decode";
import Events "Events";

/// Verify — the provenance rules of spec section 9 (C4), as pure functions.
///
/// A descriptor is VERIFIED only when both configured providers return a
/// finalized observation of the same successful transaction, and on that
/// transaction the event instruction is:
///   * an inner instruction (stackHeight == 2) directly under the outer
///     instruction named by `instruction_index`, whose program is the
///     configured program;
///   * a CPI into the configured program whose account 0 is the configured
///     event authority PDA, with data = EVENT_IX_TAG ++ discriminator ++ body;
///   * decodable at a supported schema version, matching the descriptor's
///     kind and every descriptor field (beneficiary, basis, round, close,
///     plan), and, for owner events, signed by the owner.
///
/// The authority guard is inferred, not read: the program emits events only
/// via `invoke_signed` with the authority as a signing meta, and its
/// `__event_dispatch` rejects any event-CPI whose authority is not a signer
/// or not the canonical PDA. The standard RPC form carries no signer flags,
/// and the event authority is never a transaction signer, so a successful
/// inner event-CPI from the configured program with the canonical authority
/// is authentic by construction; a direct top-level event-CPI, a forged
/// authority or a forged program cannot produce such an inner instruction.
///
/// Two RPC providers are a trusted observation boundary, not consensus.
module {
    public type Config = {
        program_id : Text;
        /// 32 bytes (`Pool.cluster_domain`).
        cluster_domain : Blob;
        /// Canonical `["__event_authority"]` PDA of `program_id`.
        event_authority : Text;
        supported_schema_versions : [Nat];
    };

    /// Provably false: the observation is agreed, finalized, and does not
    /// contain what the descriptor claims. Permanent.
    public type RejectReason = {
        #ClusterMismatch;
        #ProgramIdMismatch;
        #SignatureMismatch;
        #TransactionFailed;
        #InvalidAccountIndex;
        #WrongAncestry;
        #OuterProgramMismatch;
        #NotInnerCpi;
        #ForgedProgramId;
        #ForgedEventAuthority;
        #NotEventCpi;
        #SchemaMismatch;
        #UnknownEvent;
        #MalformedEvent : Text;
        #KindMismatch;
        #RecipientMismatch;
        #BasisMismatch;
        #RoundMismatch;
        #CloseMismatch;
        #PlanMismatch;
        #OwnerNotSigner;
    };

    /// Not (yet) an agreed finalized observation, or beyond what this build
    /// can decode. Never skipped: the event stays pending and the cursor does
    /// not pass it.
    public type Unverifiable = {
        #NotFound;
        #ProviderError : Text;
        #Inconsistent;
        #NotFinalized;
        #Disagreement;
        /// Transaction version above `Decode.MAX_SUPPORTED_TX_VERSION` (a GAP).
        #UnsupportedTransactionVersion : ?Nat8;
        /// Event schema version this build cannot decode (a GAP).
        #UnsupportedSchema : Nat;
        #Undecodable : Text;
    };

    public type Verified = {
        /// Re-derived from the decoded event; this is what is forwarded.
        forward : Events.Descriptor;
        event : Events.Event;
        slot : Nat64;
        tx_version : Decode.TxVersion;
    };

    public type Verdict = {
        #Verified : Verified;
        #Invalid : RejectReason;
        #Unverifiable : Unverifiable;
    };

    public func reasonText(r : RejectReason) : Text {
        switch (r) {
            case (#ClusterMismatch) { "cluster_domain mismatch" };
            case (#ProgramIdMismatch) { "program_id mismatch" };
            case (#SignatureMismatch) { "provider returned a different transaction signature" };
            case (#TransactionFailed) { "transaction failed (meta.err != null)" };
            case (#InvalidAccountIndex) { "instruction references an account index outside the account list" };
            case (#WrongAncestry) { "no such outer/inner instruction position" };
            case (#OuterProgramMismatch) { "outer instruction is not the configured program" };
            case (#NotInnerCpi) { "event instruction is not a stackHeight-2 inner instruction" };
            case (#ForgedProgramId) { "event instruction program is not the configured program" };
            case (#ForgedEventAuthority) { "event instruction account 0 is not the event authority" };
            case (#NotEventCpi) { "instruction data does not start with the event-CPI tag" };
            case (#SchemaMismatch) { "descriptor schema version differs from the event's" };
            case (#UnknownEvent) { "unknown event discriminator" };
            case (#MalformedEvent(m)) { "malformed event: " # m };
            case (#KindMismatch) { "descriptor kind differs from the event" };
            case (#RecipientMismatch) { "descriptor beneficiary differs from the event" };
            case (#BasisMismatch) { "descriptor basis differs from the event" };
            case (#RoundMismatch) { "descriptor round_id differs from the event" };
            case (#CloseMismatch) { "descriptor close_id differs from the event" };
            case (#PlanMismatch) { "descriptor plan differs from the event" };
            case (#OwnerNotSigner) { "event owner is not among the transaction signers" };
        };
    };

    public func unverifiableText(u : Unverifiable) : Text {
        switch (u) {
            case (#NotFound) { "transaction not found at finalized commitment" };
            case (#ProviderError(t)) { "provider error: " # t };
            case (#Inconsistent) { "a provider's own sources disagree" };
            case (#NotFinalized) { "observation is not finalized" };
            case (#Disagreement) { "the two providers disagree" };
            case (#UnsupportedTransactionVersion(?v)) { "unsupported transaction version " # Nat8.toText(v) };
            case (#UnsupportedTransactionVersion(null)) { "unsupported transaction version" };
            case (#UnsupportedSchema(v)) { "unsupported event schema version " # Nat.toText(v) };
            case (#Undecodable(t)) { t };
        };
    };

    // ------------------------------------------------------------------

    func schemaSupported(cfg : Config, v : Nat) : Bool {
        for (s in cfg.supported_schema_versions.vals()) { if (s == v) { return true } };
        false;
    };

    func keyAt(tx : Decode.Tx, idx : Nat8) : ?Text {
        let i = Nat8.toNat(idx);
        if (i < tx.accountKeys.size()) { ?tx.accountKeys[i] } else { null };
    };

    /// Checks that need no network: run before spending cycles on RPC. `null`
    /// means "go fetch".
    public func precheck(cfg : Config, d : Events.Descriptor) : ?Verdict {
        if (d.event_id.cluster_domain != cfg.cluster_domain) { return ?#Invalid(#ClusterMismatch) };
        if (d.event_id.program_id != cfg.program_id) { return ?#Invalid(#ProgramIdMismatch) };
        if (not schemaSupported(cfg, d.event_id.event_schema_version)) {
            return ?#Unverifiable(#UnsupportedSchema(d.event_id.event_schema_version));
        };
        null;
    };

    /// Two providers, finalized, same transaction, then provenance.
    public func verify(
        cfg : Config,
        d : Events.Descriptor,
        a : Decode.ProviderObservation,
        b : Decode.ProviderObservation,
    ) : Verdict {
        switch (precheck(cfg, d)) {
            case (?v) { return v };
            case (null) {};
        };
        // A version gap anywhere wins: the cursor must not pass it.
        for (o in [a, b].vals()) {
            switch (o) {
                case (#Undecodable(#UnsupportedVersion(v))) { return #Unverifiable(#UnsupportedTransactionVersion(v)) };
                case (_) {};
            };
        };
        let oa = switch (a) {
            case (#Observed(x)) { x };
            case (other) { return #Unverifiable(notObserved(other)) };
        };
        let ob = switch (b) {
            case (#Observed(x)) { x };
            case (other) { return #Unverifiable(notObserved(other)) };
        };
        if (oa.commitment != #finalized or ob.commitment != #finalized) { return #Unverifiable(#NotFinalized) };
        if (oa.tx != ob.tx) { return #Unverifiable(#Disagreement) };
        checkTx(cfg, d, oa.tx);
    };

    func notObserved(o : Decode.ProviderObservation) : Unverifiable {
        switch (o) {
            case (#Observed(_)) { #Undecodable("unreachable") };
            case (#NotFound) { #NotFound };
            case (#Error(t)) { #ProviderError(t) };
            case (#Inconsistent) { #Inconsistent };
            case (#Undecodable(f)) { #Undecodable(Decode.failureText(f)) };
        };
    };

    /// Provenance of one descriptor against one (already agreed) finalized
    /// transaction.
    public func checkTx(cfg : Config, d : Events.Descriptor, tx : Decode.Tx) : Verdict {
        let id = d.event_id;
        if (tx.signature != id.signature) { return #Invalid(#SignatureMismatch) };
        if (not tx.succeeded) { return #Invalid(#TransactionFailed) };

        // Outer instruction: must exist and be the configured program.
        if (id.instruction_index >= tx.instructions.size()) { return #Invalid(#WrongAncestry) };
        let outer = tx.instructions[id.instruction_index];
        switch (keyAt(tx, outer.programIdIndex)) {
            case (null) { return #Invalid(#InvalidAccountIndex) };
            case (?p) { if (p != cfg.program_id) { return #Invalid(#OuterProgramMismatch) } };
        };

        // Inner instruction: unique group for that outer index, valid position.
        let groups = Array.filter<Decode.InnerGroup>(tx.inner, func(g) { g.index == id.instruction_index });
        if (groups.size() != 1) { return #Invalid(#WrongAncestry) };
        let group = groups[0];
        if (id.inner_instruction_index >= group.instructions.size()) { return #Invalid(#WrongAncestry) };
        let ix = group.instructions[id.inner_instruction_index];
        if (ix.stackHeight != ?2) { return #Invalid(#NotInnerCpi) };
        switch (keyAt(tx, ix.programIdIndex)) {
            case (null) { return #Invalid(#InvalidAccountIndex) };
            case (?p) { if (p != cfg.program_id) { return #Invalid(#ForgedProgramId) } };
        };
        if (ix.accounts.size() == 0) { return #Invalid(#ForgedEventAuthority) };
        switch (keyAt(tx, ix.accounts[0])) {
            case (null) { return #Invalid(#InvalidAccountIndex) };
            case (?a0) { if (a0 != cfg.event_authority) { return #Invalid(#ForgedEventAuthority) } };
        };

        // Event bytes.
        let data = switch (Decode.base58Decode(ix.data)) {
            case (null) { return #Invalid(#NotEventCpi) };
            case (?x) { x };
        };
        let cpi = switch (Decode.parseEventCpiData(data)) {
            case (null) { return #Invalid(#NotEventCpi) };
            case (?c) { c };
        };
        switch (Decode.peekSchemaVersion(cpi)) {
            case (null) { return #Invalid(#MalformedEvent("empty body")) };
            case (?sv) {
                if (Nat8.toNat(sv) != id.event_schema_version) { return #Invalid(#SchemaMismatch) };
            };
        };
        let event = switch (Decode.decodeEvent(cpi)) {
            case (#ok(e)) { e };
            case (#err(#UnknownDiscriminator(_))) { return #Invalid(#UnknownEvent) };
            case (#err(#UnsupportedSchema(v))) { return #Unverifiable(#UnsupportedSchema(Nat8.toNat(v))) };
            case (#err(#Malformed(m))) { return #Invalid(#MalformedEvent(m)) };
        };

        // Descriptor vs decoded event: every field, nothing else.
        let kind = switch (Events.kindOf(event)) {
            case (null) { return #Invalid(#KindMismatch) };
            case (?k) { k };
        };
        if (kind != d.kind) { return #Invalid(#KindMismatch) };
        let canon = switch (Events.canonical(id, event)) {
            case (null) { return #Invalid(#MalformedEvent("plan byte out of range")) };
            case (?c) { c };
        };
        if (not Events.closeShapeOk(kind, canon.source_close_id)) {
            return #Invalid(#MalformedEvent("close_id shape"));
        };
        if (d.beneficiary_pubkey != canon.beneficiary_pubkey) { return #Invalid(#RecipientMismatch) };
        if (d.basis_lamports != canon.basis_lamports) { return #Invalid(#BasisMismatch) };
        if (d.source_round_id != canon.source_round_id) { return #Invalid(#RoundMismatch) };
        if (d.source_close_id != canon.source_close_id) { return #Invalid(#CloseMismatch) };
        if (d.plan != canon.plan) { return #Invalid(#PlanMismatch) };

        // Signer provenance: owner events are signed by the owner; a
        // promotion's beneficiary is the event's winner, whoever signed.
        if (Events.ownerMustSign(kind)) {
            let n = Nat.min(tx.numRequiredSignatures, tx.accountKeys.size());
            var signed = false;
            var i = 0;
            while (i < n) {
                if (tx.accountKeys[i] == canon.beneficiary_pubkey) { signed := true };
                i += 1;
            };
            if (not signed) { return #Invalid(#OwnerNotSigner) };
        };

        #Verified({
            forward = canon;
            event;
            slot = tx.slot;
            tx_version = tx.version;
        });
    };
};
