import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Nat "mo:base/Nat";
import Nat16 "mo:base/Nat16";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";

import AuthorityTable "AuthorityTable";

/// Accounts - small Anchor/Borsh decoders for the program accounts the watchdog
/// must read, and the progress marker built from them.
///
/// LAYOUT. An Anchor account is `discriminator (8) ++ Borsh(struct)`. Borsh:
/// integers little endian, `bool` = 1 byte, `Pubkey` / `[u8; N]` = raw bytes,
/// `Option<T>` = 1 tag byte (+ `T` when 1), enum = 1 variant byte (+ fields).
/// So every field AFTER an `Option` shifts by 8 when it is `Some`: the decoders
/// walk the struct in field order (never by fixed offsets past an Option).
/// The order comes from the program IDL (`Pool`, `Ticket`, `CloseRecord`,
/// `Backer`) and from `state.rs` (`RegistryPage`, `SnapshotPage`, which the IDL
/// omits). `test/gen/fixtures.py` serializes fixtures from the IDL with an
/// independent Borsh writer and asserts `CloseRecord` max size = 649 bytes
/// (+8 discriminator = 657, `docs/solana/measurements-2026-10.md`).
///
/// Only the fields the watchdog needs are returned; the rest are skipped, not
/// ignored: a truncated account (data shorter than the walk) decodes to `null`.
/// Trailing bytes are fine (accounts are allocated at their maximum size).
module {
    public let PAGE_CAPACITY : Nat = 8;
    /// An open registry row has MORE than this many lamports of entitlement left.
    public let CUTOFF : Nat = 10;

    // Ticket phases (state.rs).
    public let DISCOVER : Nat8 = 0;
    public let SENIOR : Nat8 = 1;
    public let SEAL : Nat8 = 2;
    public let ALLOCATE : Nat8 = 3;
    public let PRUNE : Nat8 = 4;
    public let COMMIT : Nat8 = 5;
    public let IDLE : Nat8 = 6;
    // CloseRecord.draw_state (state.rs).
    public let DRAW_NOT_REQUESTED : Nat8 = 0;
    public let DRAW_REQUESTED : Nat8 = 1;
    public let DRAW_CAPTURED : Nat8 = 2;
    public let DRAW_IGNORED_LATE : Nat8 = 3;

    public type Pool = {
        clusterDomain : Blob;
        roundId : Nat64;
        activeConfigVersion : Nat32;
        pot : Nat64;
        seed : Nat64;
        management : Nat64;
        pendingToll : Nat64;
        registryLen : Nat64;
        registryVersion : Nat64;
        nextSequence : Nat64;
        nextTicket : Nat64;
        activeTicket : Nat64;
        finalizedTicket : Nat64;
        snapshotPages : Nat64;
        registryPages : Nat64;
        participantCount : Nat64;
        participantPages : Nat64;
        nextCloseId : Nat64;
        currentCloseId : ?Nat64;
    };

    public type Ticket = {
        id : Nat64;
        phase : Nat8;
        closeId : ?Nat64;
        capturedLen : Nat64;
        expectedVersion : Nat64;
        cursor : Nat64;
        aCount : Nat64;
        bCount : Nat64;
        oldest : Blob;
        toll : Nat64;
    };

    public type Promotion = {
        #Pending;
        #Applied : { winner : Blob; entitlement : Nat };
        #Skipped : { reason : Nat8; stage : Nat8 };
    };

    public type CloseRecord = {
        closeId : Nat64;
        roundId : Nat64;
        closeStartedAt : Nat64;
        promotionDeadline : Nat64;
        closingTicketId : ?Nat64;
        cashCommitted : Bool;
        participantCount : Nat64;
        candidateCursor : Nat64;
        preferredCount : Nat64;
        fallbackCount : Nat64;
        selectedSetTag : Nat8;
        sealed : Bool;
        drawState : Nat8;
        requestAccount : Blob;
        capturedAt : Nat64;
        promotion : Promotion;
        finalized : Bool;
        requiredPagesPerSet : Nat64;
        provisionedPreferredPages : Nat64;
        provisionedFallbackPages : Nat64;
        ownerRefundDestination : Blob;
        reserveSettled : Bool;
    };

    public type Backer = {
        owner : Blob;
        series : Nat8;
        entitlement : Nat;
        lifetime : Nat;
        active : Bool;
        slot : Nat64;
    };

    public type RegistryPage = { index : Nat64; keys : [Blob] };
    public type SnapshotRow = { key : Blob; series : Nat8; remaining : Nat; slot : Nat64 };
    public type SnapshotPage = { index : Nat64; ticketId : Nat64; rows : [SnapshotRow] };

    // ------------------------------------------------------------------
    // Bounds-checked reader
    // ------------------------------------------------------------------

    type Reader = { data : [Nat8]; var pos : Nat; var bad : Bool };

    func reader(data : Blob) : Reader { { data = Blob.toArray(data); var pos = 0; var bad = false } };

    func take(r : Reader, n : Nat) : [Nat8] {
        if (r.bad or r.pos + n > r.data.size()) {
            r.bad := true;
            return [];
        };
        let start = r.pos;
        r.pos += n;
        Array.tabulate<Nat8>(n, func(i : Nat) : Nat8 { r.data[start + i] });
    };

    func skip(r : Reader, n : Nat) { ignore take(r, n) };

    func uN(r : Reader, n : Nat) : Nat {
        let b = take(r, n);
        var v : Nat = 0;
        var i = b.size();
        while (i > 0) {
            i -= 1;
            v := v * 256 + Nat8.toNat(b[i]);
        };
        v;
    };

    func u8(r : Reader) : Nat8 { Nat8.fromNat(uN(r, 1)) };
    func u32(r : Reader) : Nat32 { Nat32.fromNat(uN(r, 4)) };
    func u64(r : Reader) : Nat64 { Nat64.fromNat(uN(r, 8)) };
    func u128(r : Reader) : Nat { uN(r, 16) };
    func bool(r : Reader) : Bool { u8(r) != 0 };
    func bytes32(r : Reader) : Blob { Blob.fromArray(take(r, 32)) };
    func optU64(r : Reader) : ?Nat64 {
        let tag = u8(r);
        if (r.bad) { return null };
        if (tag == 0) { null } else { ?u64(r) };
    };

    func open(data : Blob, accountType : Text) : ?Reader {
        let r = reader(data);
        let want = AuthorityTable.accountDiscriminator(accountType);
        if (want.size() != 8) { return null };
        let got = take(r, 8);
        if (r.bad) { return null };
        var i : Nat = 0;
        while (i < 8) {
            if (got[i] != want[i]) { return null };
            i += 1;
        };
        ?r;
    };

    // ------------------------------------------------------------------
    // Decoders
    // ------------------------------------------------------------------

    public func decodePool(data : Blob) : ?Pool {
        let ?r = open(data, "Pool") else return null;
        skip(r, 1); // bump
        let clusterDomain = bytes32(r);
        let roundId = u64(r);
        let activeConfigVersion = u32(r);
        let pot = u64(r);
        let seed = u64(r);
        let management = u64(r);
        let pendingToll = u64(r);
        skip(r, 16); // funded_claims
        skip(r, 8); // excess
        skip(r, 8); // protected_rent
        let registryLen = u64(r);
        let registryVersion = u64(r);
        let nextSequence = u64(r);
        let nextTicket = u64(r);
        let activeTicket = u64(r);
        let finalizedTicket = u64(r);
        let snapshotPages = u64(r);
        let registryPages = u64(r);
        let participantCount = u64(r);
        let participantPages = u64(r);
        let nextCloseId = u64(r);
        let currentCloseId = optU64(r);
        if (r.bad) { return null };
        ?{
            clusterDomain; roundId; activeConfigVersion; pot; seed; management; pendingToll; registryLen; registryVersion;
            nextSequence; nextTicket; activeTicket; finalizedTicket; snapshotPages; registryPages; participantCount;
            participantPages; nextCloseId; currentCloseId;
        };
    };

    public func decodeTicket(data : Blob) : ?Ticket {
        let ?r = open(data, "Ticket") else return null;
        skip(r, 1); // bump
        let id = u64(r);
        let phase = u8(r);
        let closeId = optU64(r);
        let capturedLen = u64(r);
        let expectedVersion = u64(r);
        let cursor = u64(r);
        let aCount = u64(r);
        let bCount = u64(r);
        skip(r, 32); // a_remainder_total
        skip(r, 32); // sealed_a_denominator
        let oldest = bytes32(r);
        skip(r, 8); // oldest_sequence
        skip(r, 8); // oldest_slot
        skip(r, 8); // senior_credit
        skip(r, 8); // senior_slice
        skip(r, 8); // a_slice
        skip(r, 8); // b_slice
        skip(r, 8); // seed_credit
        skip(r, 8); // credited
        let toll = u64(r);
        skip(r, 6); // senior_bps, a_bps, b_bps (u16 x 3)
        if (r.bad) { return null };
        ?{ id; phase; closeId; capturedLen; expectedVersion; cursor; aCount; bCount; oldest; toll };
    };

    public func decodeCloseRecord(data : Blob) : ?CloseRecord {
        let ?r = open(data, "CloseRecord") else return null;
        skip(r, 1); // bump
        let closeId = u64(r);
        let roundId = u64(r);
        skip(r, 4); // config_version
        let closeStartedAt = u64(r);
        let promotionDeadline = u64(r);
        let closingTicketId = optU64(r);
        let cashCommitted = bool(r);
        let participantCount = u64(r);
        let candidateCursor = u64(r);
        let preferredCount = u64(r);
        let fallbackCount = u64(r);
        skip(r, 32); // preferred_digest
        skip(r, 32); // fallback_digest
        let selectedSetTag = u8(r);
        let sealed = bool(r);
        let drawState = u8(r);
        skip(r, 32); // protocol_seed
        let requestAccount = bytes32(r);
        skip(r, 64); // randomness
        let capturedAt = u64(r);
        let promotion : Promotion = switch (u8(r)) {
            case 0 { #Pending };
            case 1 { let winner = bytes32(r); let entitlement = u128(r); #Applied({ winner; entitlement }) };
            case 2 { let reason = u8(r); let stage = u8(r); #Skipped({ reason; stage }) };
            case _ { r.bad := true; #Pending };
        };
        let finalized = bool(r);
        skip(r, 8); // required_reserve
        skip(r, 5 * 8 + 2); // reserve_schedule: 5 x u64 + u16
        let requiredPagesPerSet = u64(r);
        skip(r, 8); // existing_preferred_pages
        skip(r, 8); // existing_fallback_pages
        let provisionedPreferredPages = u64(r);
        let provisionedFallbackPages = u64(r);
        skip(r, 1); // registry_extension_required
        skip(r, 8); // locked_sponsor_remaining
        skip(r, 8); // locked_owner_remaining
        skip(r, 8); // approved_provider_fee_cap
        let ownerRefundDestination = bytes32(r);
        skip(r, 10 * 8); // spent: SpentLedger (10 x u64)
        skip(r, 8); // request_rent_funded_s
        skip(r, 8); // request_rent_funded_o
        skip(r, 1); // reimbursement_observed
        skip(r, 8); // eligible_return_remaining
        skip(r, 8); // owner_request_rent_refundable_remaining
        skip(r, 8); // request_rent_returned
        let reserveSettled = bool(r);
        if (r.bad) { return null };
        ?{
            closeId; roundId; closeStartedAt; promotionDeadline; closingTicketId; cashCommitted; participantCount;
            candidateCursor; preferredCount; fallbackCount; selectedSetTag; sealed; drawState; requestAccount; capturedAt;
            promotion; finalized; requiredPagesPerSet; provisionedPreferredPages; provisionedFallbackPages;
            ownerRefundDestination; reserveSettled;
        };
    };

    public func decodeBacker(data : Blob) : ?Backer {
        let ?r = open(data, "Backer") else return null;
        skip(r, 1); // bump
        let owner = bytes32(r);
        let series = u8(r);
        skip(r, 16); // amount
        let entitlement = u128(r);
        let lifetime = u128(r);
        skip(r, 8); // funded
        skip(r, 8); // pending
        skip(r, 8); // pending_ticket
        skip(r, 8); // first_sequence
        let active = bool(r);
        let slot = u64(r);
        if (r.bad) { return null };
        ?{ owner; series; entitlement; lifetime; active; slot };
    };

    public func decodeRegistryPage(data : Blob) : ?RegistryPage {
        let ?r = open(data, "RegistryPage") else return null;
        skip(r, 1); // bump
        let index = u64(r);
        let keys = Array.tabulate<Blob>(PAGE_CAPACITY, func(_ : Nat) : Blob { bytes32(r) });
        if (r.bad) { return null };
        ?{ index; keys };
    };

    public func decodeSnapshotPage(data : Blob) : ?SnapshotPage {
        let ?r = open(data, "SnapshotPage") else return null;
        skip(r, 1); // bump
        let index = u64(r);
        let ticketId = u64(r);
        let rows = Array.tabulate<SnapshotRow>(PAGE_CAPACITY, func(_ : Nat) : SnapshotRow {
            let key = bytes32(r);
            let series = u8(r);
            let remaining = u128(r);
            let slot = u64(r);
            { key; series; remaining; slot };
        });
        if (r.bad) { return null };
        ?{ index; ticketId; rows };
    };

    /// `entitlement - lifetime`, never negative (`Backer::remaining`).
    public func remaining(b : { entitlement : Nat; lifetime : Nat }) : Nat {
        if (b.entitlement > b.lifetime) { b.entitlement - b.lifetime } else { 0 };
    };

    // ------------------------------------------------------------------
    // Progress
    // ------------------------------------------------------------------

    /// Is anything waiting to be driven? An open allocation ticket, or a close
    /// that is not finalized. A close the keeper could not load still counts:
    /// the stall must stay visible.
    public func workPending(pool : Pool, _ticket : ?Ticket, close : ?CloseRecord) : Bool {
        if (pool.activeTicket != 0) { return true };
        switch (pool.currentCloseId) {
            case (null) { false };
            case (?_) { switch (close) { case (null) { true }; case (?c) { not c.finalized } } };
        };
    };

    func b(x : Bool) : Text { if (x) { "1" } else { "0" } };

    func promotionTag(p : Promotion) : Text {
        switch (p) { case (#Pending) { "p" }; case (#Applied(_)) { "a" }; case (#Skipped(_)) { "s" } };
    };

    /// Text form of the on-chain progress markers: round id, ticket phase and
    /// cursor, and the close stage (cash committed, candidates sealed, draw
    /// state, promotion outcome, finalized, reserve settled, candidate cursor).
    /// Any change is progress; money fields are deliberately not part of it.
    public func marker(pool : Pool, ticket : ?Ticket, close : ?CloseRecord) : Text {
        var out = "r" # Nat64.toText(pool.roundId);
        out #= "|t" # Nat64.toText(pool.activeTicket);
        switch (ticket) {
            case (?t) { out #= "." # Nat8.toText(t.phase) # "." # Nat64.toText(t.cursor) };
            case (null) {};
        };
        switch (pool.currentCloseId) {
            case (null) {};
            case (?cid) {
                out #= "|k" # Nat64.toText(cid);
                switch (close) {
                    case (?c) {
                        out #= "." # b(c.cashCommitted) # b(c.sealed) # Nat8.toText(c.drawState) # promotionTag(c.promotion) # b(c.finalized) # b(c.reserveSettled) # "." # Nat64.toText(c.candidateCursor);
                    };
                    case (null) { out #= ".?" };
                };
            };
        };
        out;
    };
};
