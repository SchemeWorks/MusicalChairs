import Array "mo:base/Array";
import Nat "mo:base/Nat";

/// Journal - the watchdog's bounded record of every transaction it signed.
/// Pure: every function takes the journal and returns the next one, so the
/// persistent actor stores it as a plain stable array and `test/journal.test.mo`
/// can pin every rule.
///
/// RULES (the contract; plan M13 "does_not_resubmit_after_unknown_outcome"):
///   * ONE transaction in flight. `maySubmit` refuses while ANY entry is
///     unresolved (#Submitted, #Unknown or #Confirmed-not-yet-finalized).
///   * An RPC error AFTER the send is #Unknown. An unknown entry blocks every
///     further submission until a status read resolves it (found -> its real
///     status; not found and past `lastValidBlockHeight` -> #Expired).
///   * IDENTICAL-BYTES RE-BROADCAST. While an unknown entry's blockhash is still
///     valid, the only thing that may be broadcast for it is the SAME signed
///     bytes (`raw`, same signature). A new blockhash is taken only after the
///     entry expired, i.e. the first transaction can no longer land.
///   * Expiry is by block height: valid THROUGH `lastValidBlockHeight`
///     (inclusive); an entry the status read has SEEN never expires.
///   * Attempts per pipeline position: the first try uses the full batch, any
///     later try (after a #Failed or #Expired attempt) falls back to ONE row,
///     and `maxAttempts` failed attempts block the position for an operator.
///   * Bounded: the last `cap` entries are kept; unresolved entries are never
///     evicted.
module {
    public type Status = { #Submitted; #Unknown; #Confirmed; #Finalized; #Failed; #Expired };

    public type Entry = {
        /// Base58 first signature = the transaction id.
        signature : Text;
        blockhash : Text;
        lastValidBlockHeight : Nat64;
        /// Human label, e.g. "allocate_rows|t5|c16 rows=8".
        step : Text;
        /// Pipeline position, without the row count: "allocate_rows|t5|c16".
        position : Text;
        rows : Nat;
        /// The exact serialized transaction (what a re-broadcast must send).
        raw : Blob;
        status : Status;
        /// Submitted at (ns since epoch).
        at : Nat;
        updatedAt : Nat;
        note : Text;
    };

    public type Journal = [Entry];

    public func empty() : Journal { [] };

    public func isUnresolved(s : Status) : Bool {
        switch (s) {
            case (#Submitted or #Unknown or #Confirmed) { true };
            case (#Finalized or #Failed or #Expired) { false };
        };
    };

    public func unresolved(j : Journal) : [Entry] {
        Array.filter<Entry>(j, func(e : Entry) : Bool { isUnresolved(e.status) });
    };

    public func hasUnknown(j : Journal) : Bool {
        for (e in j.vals()) { if (e.status == #Unknown) { return true } };
        false;
    };

    public func lastSignature(j : Journal) : ?Text {
        if (j.size() == 0) { null } else { ?j[j.size() - 1].signature };
    };

    /// The newest `n` entries, oldest first.
    public func tail(j : Journal, n : Nat) : [Entry] {
        if (n >= j.size()) { return j };
        Array.tabulate<Entry>(n, func(i : Nat) : Entry { j[j.size() - n + i] });
    };

    /// Append, then evict the oldest RESOLVED entries until at most `cap` remain.
    /// Unresolved entries are never evicted, and the newest entry never is.
    public func append(j : Journal, e : Entry, cap : Nat) : Journal {
        let limit = if (cap == 0) { 1 } else { cap };
        var out : [Entry] = Array.tabulate<Entry>(j.size() + 1, func(i : Nat) : Entry { if (i < j.size()) { j[i] } else { e } });
        var excess : Nat = if (out.size() > limit) { out.size() - limit } else { 0 };
        if (excess == 0) { return out };
        // Drop the oldest resolved entries (never the newest, never unresolved).
        let newest = out.size() - 1;
        var idx : Nat = 0;
        out := Array.filter<Entry>(
            out,
            func(x : Entry) : Bool {
                let i = idx;
                idx += 1;
                if (excess > 0 and i != newest and not isUnresolved(x.status)) {
                    excess -= 1;
                    false;
                } else { true };
            },
        );
        out;
    };

    // ------------------------------------------------------------------
    // May we submit a new transaction for this position?
    // ------------------------------------------------------------------

    public type May = { #Yes : { fallback : Bool }; #No : Text };

    public func maySubmit(j : Journal, position : Text, maxAttempts : Nat) : May {
        if (unresolved(j).size() > 0) {
            return #No("an earlier transaction is unresolved");
        };
        var failures : Nat = 0;
        for (e in j.vals()) {
            if (e.position == position and (e.status == #Failed or e.status == #Expired)) { failures += 1 };
        };
        if (failures >= maxAttempts) {
            return #No("position failed " # Nat.toText(failures) # " times: operator action needed");
        };
        #Yes({ fallback = failures >= 1 });
    };

    // ------------------------------------------------------------------
    // Status reads
    // ------------------------------------------------------------------

    /// What a `getSignatureStatuses` read says about one signature.
    public type Observed = { #NotFound; #Processed; #Confirmed; #Finalized; #Err : Text };

    public type Applied = { journal : Journal; changed : Bool; landed : Bool };

    /// Fold one observation into the entry with that signature. Terminal states
    /// (#Finalized, #Failed, #Expired) are sticky. `landed` is true when the
    /// entry newly reaches #Confirmed or #Finalized.
    public func applyStatus(j : Journal, signature : Text, o : Observed, now : Nat) : Applied {
        var changed = false;
        var landed = false;
        let next = Array.map<Entry, Entry>(
            j,
            func(e : Entry) : Entry {
                if (e.signature != signature) { return e };
                switch (e.status) {
                    case (#Finalized or #Failed or #Expired) { return e };
                    case (_) {};
                };
                let upd = func(s : Status, note : Text) : Entry {
                    changed := true;
                    { e with status = s; updatedAt = now; note = if (note == "") { e.note } else { note } };
                };
                switch (o) {
                    case (#NotFound) { e };
                    case (#Processed) {
                        // Seen on chain: no longer an unknown outcome, not landed yet.
                        if (e.status == #Unknown) { upd(#Submitted, "") } else { e };
                    };
                    case (#Confirmed) {
                        if (e.status == #Confirmed) { e } else { landed := true; upd(#Confirmed, "") };
                    };
                    case (#Finalized) { landed := true; upd(#Finalized, "") };
                    case (#Err(m)) { upd(#Failed, m) };
                };
            },
        );
        { journal = next; changed; landed };
    };

    /// The send errored after submission: the outcome is unknown (the note keeps
    /// the error text). Never downgrades a terminal or #Confirmed entry.
    public func markUnknown(j : Journal, signature : Text, note : Text, now : Nat) : Journal {
        Array.map<Entry, Entry>(
            j,
            func(e : Entry) : Entry {
                if (e.signature != signature) { return e };
                switch (e.status) {
                    case (#Submitted or #Unknown) { { e with status = #Unknown; updatedAt = now; note } };
                    case (_) { e };
                };
            },
        );
    };

    /// Expire every unresolved entry whose blockhash can no longer land
    /// (`currentHeight > lastValidBlockHeight`) AND that the status read has not
    /// seen (`seen` = signatures the read found).
    public func expireUnseen(j : Journal, currentHeight : Nat64, seen : [Text], now : Nat) : { journal : Journal; expired : Nat } {
        var expired : Nat = 0;
        let next = Array.map<Entry, Entry>(
            j,
            func(e : Entry) : Entry {
                if (not isUnresolved(e.status)) { return e };
                if (currentHeight <= e.lastValidBlockHeight) { return e };
                for (s in seen.vals()) { if (s == e.signature) { return e } };
                expired += 1;
                { e with status = #Expired; updatedAt = now; note = "blockhash expired before the transaction landed" };
            },
        );
        { journal = next; expired };
    };

    /// The unknown entry (oldest first) that may be re-broadcast with its
    /// identical bytes: still inside its validity window.
    public func rebroadcast(j : Journal, currentHeight : Nat64) : ?Entry {
        for (e in j.vals()) {
            if (e.status == #Unknown and currentHeight <= e.lastValidBlockHeight) { return ?e };
        };
        null;
    };

    /// Did one of our transactions reach #Confirmed/#Finalized within `windowNs`?
    public func landedWithin(j : Journal, now : Nat, windowNs : Nat) : Bool {
        for (e in j.vals()) {
            if ((e.status == #Confirmed or e.status == #Finalized) and now >= e.updatedAt and now - e.updatedAt <= windowNs) { return true };
        };
        false;
    };
};
