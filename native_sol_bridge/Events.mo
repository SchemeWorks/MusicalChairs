import Blob "mo:base/Blob";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";

/// Events — the bridge's data vocabulary: event identity, the descriptor a
/// relayer proposes, the typed events the program emits through Anchor's
/// event CPI, the beneficiary/basis rules, and the result types exchanged
/// with Shenanigans.
///
/// Wire layout of one emitted event (spec section 9; `solana/fixtures/
/// event-encodings.json`, `programs/musical_chairs/src/events.rs`):
///   inner-instruction data = EVENT_IX_TAG_LE (8) || discriminator (8) || borsh(body)
/// with `discriminator = sha256("event:<Name>")[..8]` and `schema_version`
/// as the first body byte.
module {
    // ------------------------------------------------------------------
    // Identity and descriptor
    // ------------------------------------------------------------------

    /// Immutable event identity. Spec section 9:
    /// (cluster_domain, program_id, signature, instruction_index,
    ///  inner_instruction_index, event_schema_version).
    /// `instruction_index` is the OUTER instruction index (the value of
    /// `meta.innerInstructions[].index`); `inner_instruction_index` is the
    /// 0-based position inside that group's `instructions` array.
    public type EventId = {
        cluster_domain : Blob;
        program_id : Text;
        signature : Text;
        instruction_index : Nat;
        inner_instruction_index : Nat;
        event_schema_version : Nat;
    };

    public type Kind = { #PositionOpened; #SeriesAFunded; #PromotionApplied };

    public type Plan = { #Simple; #Compound15; #Compound30 };

    /// The event a relayer proposes AND the payload forwarded to
    /// `shenanigans.observeNativeEvent` once verified. The shape is dictated
    /// by Shenanigans (`shenanigans/NativeRewards.mo`, worker B); the bridge
    /// forwards the descriptor it re-derived from the decoded event, never
    /// the relayer's copy.
    public type Descriptor = {
        event_id : EventId;
        source_round_id : Nat64;
        source_close_id : ?Nat64;
        kind : Kind;
        beneficiary_pubkey : Text;
        basis_lamports : Nat64;
        plan : ?Plan;
    };

    // ------------------------------------------------------------------
    // Shenanigans-facing result types
    // ------------------------------------------------------------------

    /// Shenanigans' obligation handle: the canonical text key of the event id
    /// (`shenanigans/NativeRewards.mo`: `ObligationId = Text`).
    public type ObligationId = Text;

    public type ObserveResult = {
        #Observed : ObligationId;
        #AlreadyObserved : ObligationId;
        #Rejected : { reason : Text; permanent : Bool };
    };

    /// Mirrors `NativeRewards.BindResult` (the reply may carry any of these
    /// tags; a narrower declaration here would trap the reply decode).
    public type BindResult = {
        #Reserved;
        #AlreadyReserved;
        #Unbound;
        #UnknownEvent;
        #Rejected : Text;
    };

    /// Per-leg status as Shenanigans reports it. Mirrors
    /// `NativeRewards.LegStatus` EXACTLY (shenanigans/NativeRewards.mo): the
    /// reply of `executeNativeRewardLeg` is decoded against this type, and a
    /// narrower variant here would trap the decode after the leg was already
    /// submitted to the ledger.
    public type LegStatus = {
        #Pending;
        #SubmittedUnknown;
        #Committed : { block : Nat };
        #Retryable : { reason : Text; attempts : Nat };
        #Quarantined : { reason : Text; recoverable : Bool };
    };

    // ------------------------------------------------------------------
    // Typed events (schema_version = 1)
    // ------------------------------------------------------------------
    // Pubkeys are base58 text. `u128` fields are Nat.

    public type PositionOpened = {
        schema_version : Nat8;
        owner : Text;
        nonce : Nat64;
        round_id : Nat64;
        close_id : ?Nat64;
        plan : Nat8;
        gross : Nat64;
        load : Nat64;
        config_version : Nat32;
    };

    public type SeriesAFunded = {
        schema_version : Nat8;
        owner : Text;
        round_id : Nat64;
        close_id : ?Nat64;
        gross : Nat64;
        entitlement_after : Nat;
    };

    public type PayoutPaid = {
        schema_version : Nat8;
        owner : Text;
        nonce : Nat64;
        round_id : Nat64;
        gross_paid : Nat64;
        toll : Nat64;
        net : Nat64;
        destination : Text;
        closing : Bool;
        close_id : ?Nat64;
        ticket_id : Nat64;
    };

    public type BackerClaimed = {
        schema_version : Nat8;
        owner : Text;
        round_id : Nat64;
        amount : Nat64;
        a_part : Nat64;
        b_part : Nat64;
    };

    public type PromotionApplied = {
        schema_version : Nat8;
        round_id : Nat64;
        close_id : ?Nat64;
        winner : Text;
        loss : Nat64;
        entitlement : Nat;
    };

    public type RoundFinalized = {
        schema_version : Nat8;
        close_id : Nat64;
        new_round : Nat64;
        seed_carried : Nat64;
    };

    /// Any other program event: identified by name, body kept raw.
    public type OtherEvent = { name : Text; schema_version : Nat8; body : Blob };

    public type Event = {
        #PositionOpened : PositionOpened;
        #SeriesAFunded : SeriesAFunded;
        #PayoutPaid : PayoutPaid;
        #BackerClaimed : BackerClaimed;
        #PromotionApplied : PromotionApplied;
        #RoundFinalized : RoundFinalized;
        #Other : OtherEvent;
    };

    // ------------------------------------------------------------------
    // Event-CPI constants
    // ------------------------------------------------------------------

    /// Anchor `EVENT_IX_TAG_LE` (pinned anchor-lang 0.32.1 src/event.rs):
    /// the bytes e445a52e51cb9a1d, in this order.
    public let EVENT_IX_TAG : [Nat8] = [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d];

    /// (name, discriminator hex) for all 20 program events, copied from
    /// `solana/fixtures/event-encodings.json`.
    public let EVENT_DISCRIMINATORS : [(Text, Text)] = [
        ("AdmissionPauseSet", "aedcf8ecc820dac4"),
        ("BackerClaimed", "f020a678e5dbd777"),
        ("CandidatesSealed", "b0138da710befa3b"),
        ("ConfigActivated", "eb742ac171fb8989"),
        ("ConfigStaged", "51a9ab631bc6d312"),
        ("DrawCaptured", "296a83e019ed5493"),
        ("DrawRequested", "3bacdb2a1c5e5d4b"),
        ("ManagementWithdrawn", "1f6bce1befaf5645"),
        ("OperationsFunded", "cc703980d21f561f"),
        ("OperationsWithdrawn", "199d63447cb9442c"),
        ("OwnerRefundDelivered", "a227d6661e971dbf"),
        ("PayoutPaid", "40cfc1b00e66369f"),
        ("PositionClosed", "9da3e3e40d618a79"),
        ("PositionOpened", "edaff3e693756579"),
        ("PromotionApplied", "200abf9bf0df8cbb"),
        ("PromotionSkipped", "43a52b0011b62073"),
        ("RoundClosed", "2df31c168446afe2"),
        ("RoundFinalized", "2bbb11c124f13052"),
        ("SeriesAFunded", "ded16088231ea1fe"),
        ("TicketCommitted", "21c045b338cccb7e"),
    ];

    public func nameOfDiscriminator(hexDisc : Text) : ?Text {
        for ((name, h) in EVENT_DISCRIMINATORS.vals()) {
            if (h == hexDisc) { return ?name };
        };
        null;
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    private let HEX : [Text] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

    public func hex(b : Blob) : Text {
        var out = "";
        for (byte in Blob.toArray(b).vals()) {
            let n = Nat8.toNat(byte);
            out #= HEX[n / 16] # HEX[n % 16];
        };
        out;
    };

    /// Injective, human-readable key for the immutable event identity.
    /// `program_id` and `signature` are validated as base58 on submission, so
    /// "|" cannot occur inside a component.
    public func eventKey(id : EventId) : Text {
        hex(id.cluster_domain) # "|" # id.program_id # "|" # id.signature # "|" #
        Nat.toText(id.instruction_index) # "|" # Nat.toText(id.inner_instruction_index) # "|" #
        Nat.toText(id.event_schema_version);
    };

    public func kindOf(e : Event) : ?Kind {
        switch (e) {
            case (#PositionOpened(_)) { ?#PositionOpened };
            case (#SeriesAFunded(_)) { ?#SeriesAFunded };
            case (#PromotionApplied(_)) { ?#PromotionApplied };
            case (_) { null };
        };
    };

    public func nameOf(e : Event) : Text {
        switch (e) {
            case (#PositionOpened(_)) { "PositionOpened" };
            case (#SeriesAFunded(_)) { "SeriesAFunded" };
            case (#PayoutPaid(_)) { "PayoutPaid" };
            case (#BackerClaimed(_)) { "BackerClaimed" };
            case (#PromotionApplied(_)) { "PromotionApplied" };
            case (#RoundFinalized(_)) { "RoundFinalized" };
            case (#Other(o)) { o.name };
        };
    };

    public func schemaVersionOf(e : Event) : Nat8 {
        switch (e) {
            case (#PositionOpened(x)) { x.schema_version };
            case (#SeriesAFunded(x)) { x.schema_version };
            case (#PayoutPaid(x)) { x.schema_version };
            case (#BackerClaimed(x)) { x.schema_version };
            case (#PromotionApplied(x)) { x.schema_version };
            case (#RoundFinalized(x)) { x.schema_version };
            case (#Other(x)) { x.schema_version };
        };
    };

    /// Owner events must be signed by the owner; a promotion's beneficiary is
    /// the event's own `winner` and the keeper signer is irrelevant.
    public func ownerMustSign(k : Kind) : Bool {
        switch (k) {
            case (#PositionOpened) { true };
            case (#SeriesAFunded) { true };
            case (#PromotionApplied) { false };
        };
    };

    /// `close_id` shape pinned by the program: an opening or a Series A
    /// funding is never attributed to a close (`None`); a promotion belongs
    /// to exactly one close (`Some`).
    public func closeShapeOk(k : Kind, close : ?Nat64) : Bool {
        switch (k, close) {
            case (#PromotionApplied, ?_) { true };
            case (#PromotionApplied, null) { false };
            case (_, null) { true };
            case (_, ?_) { false };
        };
    };

    public func planOfByte(b : Nat8) : ?Plan {
        switch (b) {
            case (0) { ?#Simple };
            case (1) { ?#Compound15 };
            case (2) { ?#Compound30 };
            case (_) { null };
        };
    };

    /// The descriptor the bridge derives from a decoded event: beneficiary,
    /// basis (gross for an opening or a Series A funding, loss for a
    /// promotion), source round/close and plan all come from the event body,
    /// never from the keeper, relayer or any transaction signer.
    /// `null` for kinds the bridge does not forward and for an out-of-range
    /// plan byte.
    public func canonical(id : EventId, e : Event) : ?Descriptor {
        switch (e) {
            case (#PositionOpened(p)) {
                switch (planOfByte(p.plan)) {
                    case (null) { null };
                    case (?plan) {
                        ?{
                            event_id = id;
                            source_round_id = p.round_id;
                            source_close_id = p.close_id;
                            kind = #PositionOpened;
                            beneficiary_pubkey = p.owner;
                            basis_lamports = p.gross;
                            plan = ?plan;
                        };
                    };
                };
            };
            case (#SeriesAFunded(s)) {
                ?{
                    event_id = id;
                    source_round_id = s.round_id;
                    source_close_id = s.close_id;
                    kind = #SeriesAFunded;
                    beneficiary_pubkey = s.owner;
                    basis_lamports = s.gross;
                    plan = null;
                };
            };
            case (#PromotionApplied(p)) {
                ?{
                    event_id = id;
                    source_round_id = p.round_id;
                    source_close_id = p.close_id;
                    kind = #PromotionApplied;
                    beneficiary_pubkey = p.winner;
                    basis_lamports = p.loss;
                    plan = null;
                };
            };
            case (_) { null };
        };
    };
};
