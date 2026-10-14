import Array "mo:base/Array";
import Buffer "mo:base/Buffer";
import Int "mo:base/Int";
import Nat "mo:base/Nat";
import OrderedMap "mo:base/OrderedMap";
import Text "mo:base/Text";

import Decode "Decode";
import Events "Events";
import Verify "Verify";

/// Outbox — the bridge's durable event store and cursor (spec section 9,
/// "Cursor"; DR-24/DR-25). Pure state transitions over a stable `State`;
/// the actor supplies `now` and performs the awaits, so every rule here is
/// testable without a replica.
///
/// Lifecycle of one entry (keyed by the immutable event id):
///
///   Pending --verify--> Verified --observeNativeEvent--> Forwarded
///      |  ^                 |  ^                              |
///      v  |                 |  +-- transient rejection / call error (retry)
///   Unverified (incl. GAPs) |
///      |                    +-- permanent rejection --> Quarantined
///      +-- provably false --> Invalid
///
/// Cursor: the highest sequence number such that EVERY entry up to it is
/// Forwarded (Shenanigans returned `#Observed`/`#AlreadyObserved`),
/// Quarantined (Shenanigans permanently rejected it) or Invalid (the bridge
/// proved the descriptor false from an agreed finalized observation), each a
/// durable record. Pending, Unverified (a GAP such as an unsupported
/// transaction version) and Verified-but-unacknowledged entries block it.
/// It is derived from the set of open entries, so it can never run ahead of
/// a gap and never moves backwards.
///
/// Binding is a separate, later fact: a Forwarded entry whose beneficiary has
/// no SIWS binding yet stays in the pubkey-keyed `unbound` index until
/// `applyBind` records `#Reserved`/`#AlreadyReserved`. It never blocks the
/// cursor (Shenanigans already holds the durable Observed record).
module {
    public let MAX_PAGE : Nat = 100;

    public type Status = {
        /// Submitted by a relayer; not verified yet.
        #Pending;
        /// Not an agreed finalized observation (or beyond this build). GAPs
        /// (`UnsupportedTransactionVersion`, `UnsupportedSchema`) live here.
        #Unverified : { reason : Verify.Unverifiable; attempts : Nat; last_at : Nat64 };
        /// Verified; Shenanigans has not acknowledged it yet.
        #Verified : { retries : Nat; last_reason : ?Text; last_at : Nat64 };
        /// Shenanigans durably holds the observation.
        #Forwarded : { obligation_id : Events.ObligationId; already_observed : Bool; at : Nat64 };
        /// Shenanigans permanently rejected it; durable record, cursor may pass.
        #Quarantined : { reason : Text; at : Nat64 };
        /// The bridge proved the descriptor false; durable record, cursor may pass.
        #Invalid : { reason : Verify.RejectReason; at : Nat64 };
    };

    public type Binding = { #Unbound; #Bound : { at : Nat64 } };

    public type Entry = {
        seq : Nat;
        event_key : Text;
        descriptor : Events.Descriptor;
        submitted_by : Principal;
        submitted_at : Nat64;
        status : Status;
        verified : ?Verify.Verified;
        binding : Binding;
        /// Cooldown gate; written BEFORE any await so a trap cannot reopen it.
        next_attempt_at : Nat64;
    };

    public type State = {
        var next_seq : Nat;
        var entries : OrderedMap.Map<Nat, Entry>;
        /// event_key -> latest seq
        var index : OrderedMap.Map<Text, Nat>;
        /// seqs that block the cursor
        var open : OrderedMap.Map<Nat, Bool>;
        /// beneficiary pubkey -> seqs forwarded but not yet bound
        var unbound : OrderedMap.Map<Text, [Nat]>;
    };

    func nat() : OrderedMap.Operations<Nat> { OrderedMap.Make<Nat>(Nat.compare) };
    func txt() : OrderedMap.Operations<Text> { OrderedMap.Make<Text>(Text.compare) };

    public func empty() : State {
        {
            var next_seq = 1;
            var entries = nat().empty<Entry>();
            var index = txt().empty<Nat>();
            var open = nat().empty<Bool>();
            var unbound = txt().empty<[Nat]>();
        };
    };

    // ------------------------------------------------------------------
    // Classification helpers
    // ------------------------------------------------------------------

    /// True when the cursor may pass an entry in this status.
    public func passable(s : Status) : Bool {
        switch (s) {
            case (#Forwarded(_)) { true };
            case (#Quarantined(_)) { true };
            case (#Invalid(_)) { true };
            case (_) { false };
        };
    };

    public func isGap(s : Status) : Bool {
        switch (s) {
            case (#Unverified({ reason = #UnsupportedTransactionVersion(_) })) { true };
            case (#Unverified({ reason = #UnsupportedSchema(_) })) { true };
            case (_) { false };
        };
    };

    /// `null` when the descriptor is well-formed enough to key and store.
    public func shapeError(d : Events.Descriptor) : ?Text {
        let id = d.event_id;
        if (id.cluster_domain.size() != 32) { return ?"cluster_domain must be 32 bytes" };
        if (not Decode.isBase58Of(id.program_id, 32)) { return ?"program_id must be a base58 32-byte key" };
        if (not Decode.isBase58Of(id.signature, 64)) { return ?"signature must be a base58 64-byte signature" };
        if (not Decode.isBase58Of(d.beneficiary_pubkey, 32)) { return ?"beneficiary_pubkey must be a base58 32-byte key" };
        if (id.event_schema_version == 0 or id.event_schema_version > 255) { return ?"event_schema_version out of range" };
        null;
    };

    // ------------------------------------------------------------------
    // Reads
    // ------------------------------------------------------------------

    public func get(s : State, key : Text) : ?Entry {
        switch (txt().get(s.index, key)) {
            case (null) { null };
            case (?seq) { nat().get(s.entries, seq) };
        };
    };

    public func getBySeq(s : State, seq : Nat) : ?Entry { nat().get(s.entries, seq) };

    /// The ingestion cursor: every entry with `seq <= cursor` is durable-done.
    public func cursor(s : State) : Nat {
        switch (nat().minEntry(s.open)) {
            case (null) { Int.abs(s.next_seq - 1) };
            case (?(seq, _)) { Int.abs(seq - 1) };
        };
    };

    /// Entries still open (blocking the cursor) with `seq > after`, oldest first.
    public func pending(s : State, after : Nat, limit : Nat) : [Entry] {
        let cap = Nat.min(limit, MAX_PAGE);
        let out = Buffer.Buffer<Entry>(cap);
        label scan for ((seq, _) in nat().entries(s.open)) {
            if (out.size() >= cap) { break scan };
            if (seq > after) {
                switch (nat().get(s.entries, seq)) {
                    case (?e) { out.add(e) };
                    case (null) {};
                };
            };
        };
        Buffer.toArray(out);
    };

    /// Forwarded entries whose beneficiary has no SIWS binding yet.
    public func unboundFor(s : State, pubkey : Text) : [Entry] {
        switch (txt().get(s.unbound, pubkey)) {
            case (null) { [] };
            case (?seqs) {
                Array.mapFilter<Nat, Entry>(seqs, func(q) { nat().get(s.entries, q) });
            };
        };
    };

    /// Newest-first page of the whole store (history/feeds, DR-25).
    /// `before`: only entries with `seq < before`.
    public func recent(s : State, before : ?Nat, limit : Nat) : [Entry] {
        let cap = Nat.min(limit, MAX_PAGE);
        let out = Buffer.Buffer<Entry>(cap);
        label scan for ((seq, e) in nat().entriesRev(s.entries)) {
            if (out.size() >= cap) { break scan };
            let ok = switch (before) { case (null) { true }; case (?b) { seq < b } };
            if (ok) { out.add(e) };
        };
        Buffer.toArray(out);
    };

    // ------------------------------------------------------------------
    // Writes
    // ------------------------------------------------------------------

    func put(s : State, e : Entry) {
        s.entries := nat().put(s.entries, e.seq, e);
        s.index := txt().put(s.index, e.event_key, e.seq);
    };

    func setOpen(s : State, seq : Nat, status : Status) {
        if (passable(status)) {
            s.open := nat().delete(s.open, seq);
        } else {
            s.open := nat().put(s.open, seq, true);
        };
    };

    public type SubmitResult = {
        #Submitted : { seq : Nat };
        /// The event id already has an entry; nothing was changed.
        #Duplicate : { seq : Nat; status : Status; same_descriptor : Bool };
        #Malformed : Text;
    };

    /// Relayer ingress. The caller's allowlist check is the actor's job.
    public func submit(s : State, caller : Principal, d : Events.Descriptor, now : Nat64) : SubmitResult {
        switch (shapeError(d)) {
            case (?m) { return #Malformed(m) };
            case (null) {};
        };
        let key = Events.eventKey(d.event_id);
        switch (get(s, key)) {
            case (?existing) {
                switch (existing.status) {
                    // A provably-false descriptor must not be able to occupy
                    // an event id forever (that would let a bad relayer
                    // censor a real event): a corrected descriptor replaces
                    // it as a NEW entry; the Invalid one stays as history.
                    case (#Invalid(_)) {};
                    case (_) {
                        return #Duplicate({
                            seq = existing.seq;
                            status = existing.status;
                            same_descriptor = existing.descriptor == d;
                        });
                    };
                };
            };
            case (null) {};
        };
        let seq = s.next_seq;
        s.next_seq += 1;
        put(s, {
            seq;
            event_key = key;
            descriptor = d;
            submitted_by = caller;
            submitted_at = now;
            status = #Pending;
            verified = null;
            binding = #Unbound;
            next_attempt_at = 0;
        });
        setOpen(s, seq, #Pending);
        #Submitted({ seq });
    };

    public type Attempt = {
        #Unknown;
        /// Already durable-done (Forwarded / Quarantined / Invalid).
        #Terminal : Entry;
        /// Too soon; retry after this time (ns).
        #Cooldown : Nat64;
        /// Proceed. `next_attempt_at` was advanced; this is the updated entry.
        #Go : Entry;
    };

    /// Claim the right to verify/forward `key`. Persisting the cooldown
    /// before the first await bounds how often permissionless callers can
    /// make the bridge spend cycles on one event.
    public func beginAttempt(s : State, key : Text, now : Nat64, cooldown_ns : Nat64) : Attempt {
        switch (get(s, key)) {
            case (null) { #Unknown };
            case (?e) {
                if (passable(e.status)) { return #Terminal(e) };
                if (now < e.next_attempt_at) { return #Cooldown(e.next_attempt_at) };
                let upd = { e with next_attempt_at = now + cooldown_ns };
                put(s, upd);
                #Go(upd);
            };
        };
    };

    /// Same gate for the bind step (Forwarded + still unbound only).
    public func beginBind(s : State, key : Text, now : Nat64, cooldown_ns : Nat64) : Attempt {
        switch (get(s, key)) {
            case (null) { #Unknown };
            case (?e) {
                switch (e.status, e.binding) {
                    case (#Forwarded(_), #Unbound) {
                        if (now < e.next_attempt_at) { return #Cooldown(e.next_attempt_at) };
                        let upd = { e with next_attempt_at = now + cooldown_ns };
                        put(s, upd);
                        #Go(upd);
                    };
                    case (_) { #Terminal(e) };
                };
            };
        };
    };

    public type Applied = {
        #Applied : Entry;
        /// The entry was not in the state this transition requires (another
        /// caller got there first); nothing was changed.
        #Stale : Entry;
        #Unknown;
    };

    /// Record the outcome of verification (entry must be Pending/Unverified).
    public func applyVerdict(s : State, key : Text, v : Verify.Verdict, now : Nat64) : Applied {
        switch (get(s, key)) {
            case (null) { #Unknown };
            case (?e) {
                let prevAttempts : Nat = switch (e.status) {
                    case (#Pending) { 0 };
                    case (#Unverified(u)) { u.attempts };
                    case (_) { return #Stale(e) };
                };
                let upd : Entry = switch (v) {
                    case (#Verified(ok)) {
                        { e with status = #Verified({ retries = 0; last_reason = null; last_at = now }); verified = ?ok };
                    };
                    case (#Invalid(r)) { { e with status = #Invalid({ reason = r; at = now }) } };
                    case (#Unverifiable(u)) {
                        { e with status = #Unverified({ reason = u; attempts = prevAttempts + 1; last_at = now }) };
                    };
                };
                put(s, upd);
                setOpen(s, upd.seq, upd.status);
                #Applied(upd);
            };
        };
    };

    func addUnbound(s : State, pubkey : Text, seq : Nat) {
        let cur = switch (txt().get(s.unbound, pubkey)) { case (null) { [] }; case (?x) { x } };
        s.unbound := txt().put(s.unbound, pubkey, Array.append<Nat>(cur, [seq]));
    };

    func removeUnbound(s : State, pubkey : Text, seq : Nat) {
        switch (txt().get(s.unbound, pubkey)) {
            case (null) {};
            case (?cur) {
                let rest = Array.filter<Nat>(cur, func(q) { q != seq });
                if (rest.size() == 0) {
                    s.unbound := txt().delete(s.unbound, pubkey);
                } else { s.unbound := txt().put(s.unbound, pubkey, rest) };
            };
        };
    };

    func retryStatus(prev : Status, reason : Text, now : Nat64) : Status {
        let n : Nat = switch (prev) { case (#Verified(v)) { v.retries }; case (_) { 0 } };
        #Verified({ retries = n + 1; last_reason = ?reason; last_at = now });
    };

    /// Record Shenanigans' answer to `observeNativeEvent` (entry must be
    /// Verified). Only `#Observed`/`#AlreadyObserved` (Forwarded) and a
    /// permanent rejection (Quarantined) are durable-done; a transient
    /// rejection keeps the entry open for retry.
    public func applyForward(s : State, key : Text, r : Events.ObserveResult, now : Nat64) : Applied {
        switch (get(s, key)) {
            case (null) { #Unknown };
            case (?e) {
                switch (e.status) {
                    case (#Verified(_)) {};
                    case (_) { return #Stale(e) };
                };
                let upd : Entry = switch (r) {
                    case (#Observed(id)) {
                        { e with status = #Forwarded({ obligation_id = id; already_observed = false; at = now }) };
                    };
                    case (#AlreadyObserved(id)) {
                        { e with status = #Forwarded({ obligation_id = id; already_observed = true; at = now }) };
                    };
                    case (#Rejected({ reason; permanent = true })) {
                        { e with status = #Quarantined({ reason; at = now }) };
                    };
                    case (#Rejected({ reason; permanent = false })) {
                        { e with status = retryStatus(e.status, reason, now) };
                    };
                };
                put(s, upd);
                setOpen(s, upd.seq, upd.status);
                switch (upd.status) {
                    case (#Forwarded(_)) {
                        switch (upd.verified) {
                            case (?v) { addUnbound(s, v.forward.beneficiary_pubkey, upd.seq) };
                            case (null) {};
                        };
                    };
                    case (_) {};
                };
                #Applied(upd);
            };
        };
    };

    /// The forward call itself failed (reject/trap/timeout): outcome unknown,
    /// retain for an identical retry (`observeNativeEvent` is idempotent).
    public func applyForwardError(s : State, key : Text, message : Text, now : Nat64) : Applied {
        applyForward(s, key, #Rejected({ reason = message; permanent = false }), now);
    };

    /// Record Shenanigans' answer to `bindAndReserve` (entry must be
    /// Forwarded). `#Unbound` leaves the entry in the pubkey-keyed index.
    public func applyBind(s : State, key : Text, r : Events.BindResult, now : Nat64) : Applied {
        switch (get(s, key)) {
            case (null) { #Unknown };
            case (?e) {
                switch (e.status) {
                    case (#Forwarded(_)) {};
                    case (_) { return #Stale(e) };
                };
                switch (r) {
                    // Not bound (yet): the event stays pubkey-keyed.
                    case (#Unbound or #UnknownEvent or #Rejected(_)) { #Applied(e) };
                    case (#Reserved or #AlreadyReserved) {
                        let upd = { e with binding = #Bound({ at = now }) };
                        put(s, upd);
                        switch (upd.verified) {
                            case (?v) { removeUnbound(s, v.forward.beneficiary_pubkey, upd.seq) };
                            case (null) {};
                        };
                        #Applied(upd);
                    };
                };
            };
        };
    };
};
