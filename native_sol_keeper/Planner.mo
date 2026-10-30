import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Debug "mo:base/Debug";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";

import Accounts "Accounts";
import AuthorityTable "AuthorityTable";
import Pda "Pda";
import Tx "Tx";

/// Planner - the watchdog's pure next-step planner. Given the decoded Pool (and
/// Ticket / CloseRecord when open) plus whatever pages the caller has already
/// fetched, it names the ONE instruction that moves the state machine forward,
/// or says what it still needs to read, or why it will not act.
///
/// PORTED FROM the Codex-accepted reference logic in
/// `frontend/src/solana/native/transactions.ts` (`batchRows`, the six ticket
/// phase builders, `planPrune`) and `panelsClosing.ts` (`nextProcessingStep`),
/// checked byte-for-byte against real localnet transactions in
/// `test/planner.test.mo`. Same rules as the Rust worker's planner:
///   * priority: an open allocation ticket (Discover -> Senior -> Seal ->
///     Allocate -> Prune -> Commit), then the closing pipeline;
///   * batch cap 8 rows, ALIGNED so one batch never crosses a registry/snapshot
///     page edge; `maxRows = 1` is the one-row fallback;
///   * Prune walks DOWN from the cursor and simulates earlier swap-deletes of the
///     same batch; the batch is cut BEFORE a row whose moved Backer was already
///     written in it.
///
/// SUPPORTED STEPS (the watchdog's whole repertoire): the six ticket phases plus
/// `expire_promotion` and `finalize_round`. Nothing here ever places the fee
/// address in an instruction: none of these instructions has a signer or payer
/// account (see `test/authority.test.mo`). Every other closing step is
/// `#Blocked("unsupported step: <name>")` on purpose: `provision_page` /
/// `provision_close_record` need a rent-paying `payer`, `request_round_draw` /
/// `apply_promotion` need a payer and oracle accounts, and `build_candidates` /
/// `capture_round_draw` need page lists and provider accounts the watchdog does
/// not decode. The external primary worker (or any wallet) runs those; the
/// watchdog covers the cash path (ticket) and the deadline path (expire,
/// finalize).
module {
    public let SUPPORTED : [Text] = [
        "discover_rows", "apply_senior", "seal_weights", "allocate_rows", "prune_rows", "commit_ticket",
        "expire_promotion", "finalize_round",
    ];

    /// Most rows any batched instruction accepts (the program's `batch_len`).
    public let MAX_BATCH_ROWS : Nat = 8;

    /// Compute-unit limits per instruction, mirroring the reference adapter
    /// (measured maxima are far below: discover 40k, allocate 42k, prune 57k).
    public func computeUnitLimit(name : Text) : Nat32 {
        if (name == "prune_rows") { 300_000 } else { 200_000 };
    };

    public type Step = {
        /// IDL instruction name.
        name : Text;
        /// Pipeline position without the row count, e.g. "allocate_rows|t5|c16".
        position : Text;
        rows : Nat;
        summary : Text;
        instruction : Tx.Instruction;
    };

    public type Need = { #Registry : Nat; #Snapshot : Nat; #Backer : Blob };
    public type Plan = { #Step : Step; #Need : Need; #Blocked : Text; #Idle };

    public type View = {
        addr : Pda.Addresses;
        /// Unix seconds (ICP time; the program checks the Solana Clock).
        nowSecs : Nat64;
        /// Slack past `promotion_deadline` before `expire_promotion` is sent, so
        /// clock skew between ICP and Solana cannot burn a fee on an early expire.
        deadlineMarginSecs : Nat64;
        maxRows : Nat;
        /// Leave rows that only became capped during this ticket for a later ticket.
        skipNewlyCapped : Bool;
        pool : Accounts.Pool;
        ticket : ?Accounts.Ticket;
        close : ?Accounts.CloseRecord;
        registryPage : Nat -> ?Accounts.RegistryPage;
        snapshotPage : Nat -> ?Accounts.SnapshotPage;
        /// Live remaining entitlement of a Backer, when it has been read.
        liveRemaining : Blob -> ?Nat;
    };

    // ------------------------------------------------------------------
    // Encoding + account helpers
    // ------------------------------------------------------------------

    func discriminator(name : Text) : [Nat8] {
        switch (AuthorityTable.instruction(name)) {
            case (?ix) { ix.discriminator };
            case (null) { Debug.trap("no IDL instruction " # name) };
        };
    };
    func u64le(n : Nat64) : [Nat8] {
        Array.tabulate<Nat8>(8, func(i : Nat) : Nat8 { Nat8.fromNat(Nat64.toNat((n >> (Nat64.fromNat(i) * 8)) & 0xFF)) });
    };
    func data(name : Text, tail : [Nat8]) : Blob { Blob.fromArray(Array.append<Nat8>(discriminator(name), tail)) };
    func meta(key : Blob, writable : Bool) : Tx.Meta { { key; isSigner = false; isWritable = writable } };
    /// Anchor's `None` for an optional account: the program id, read-only.
    func none(a : Pda.Addresses) : Tx.Meta { meta(a.programId, false) };
    func ix(a : Pda.Addresses, name : Text, accounts : [Tx.Meta], tail : [Nat8]) : Tx.Instruction {
        { programId = a.programId; accounts; data = data(name, tail) };
    };

    // ------------------------------------------------------------------
    // Instruction builders (pure, one per supported instruction)
    // ------------------------------------------------------------------

    /// Per slot: `[registry page (ro), snapshot page (W), backer (ro)]`.
    public func discoverIx(a : Pda.Addresses, ticketId : Nat64, cursor : Nat64, rows : Nat, backerAt : Nat64 -> Blob) : Tx.Instruction {
        let accounts = Buffer.Buffer<Tx.Meta>(2 + 3 * rows);
        accounts.add(meta(a.ticket, true));
        accounts.add(meta(a.pool, false));
        var i : Nat = 0;
        while (i < rows) {
            let slot = cursor + Nat64.fromNat(i);
            let page = Nat64.toNat(slot) / Accounts.PAGE_CAPACITY;
            accounts.add(meta(a.registry(page), false));
            accounts.add(meta(a.snapshot(page), true));
            accounts.add(meta(backerAt(slot), false));
            i += 1;
        };
        ix(a, "discover_rows", Buffer.toArray(accounts), Array.flatten<Nat8>([u64le(ticketId), u64le(cursor), [Nat8.fromNat(rows)]]));
    };

    /// The tracked oldest open A, or Anchor's `None` when the ticket captured none.
    public func applySeniorIx(a : Pda.Addresses, ticketId : Nat64, oldest : ?Blob) : Tx.Instruction {
        let second = switch (oldest) { case (?k) { meta(k, true) }; case (null) { none(a) } };
        ix(a, "apply_senior", [meta(a.ticket, true), second], u64le(ticketId));
    };

    public func sealWeightsIx(a : Pda.Addresses, ticketId : Nat64) : Tx.Instruction {
        ix(a, "seal_weights", [meta(a.ticket, true)], u64le(ticketId));
    };

    /// Per slot: `[snapshot page (ro), the backer the snapshot row names (W)]`.
    public func allocateIx(a : Pda.Addresses, ticketId : Nat64, cursor : Nat64, rows : Nat, backerAt : Nat64 -> Blob) : Tx.Instruction {
        let accounts = Buffer.Buffer<Tx.Meta>(1 + 2 * rows);
        accounts.add(meta(a.ticket, true));
        var i : Nat = 0;
        while (i < rows) {
            let slot = cursor + Nat64.fromNat(i);
            accounts.add(meta(a.snapshot(Nat64.toNat(slot) / Accounts.PAGE_CAPACITY), false));
            accounts.add(meta(backerAt(slot), true));
            i += 1;
        };
        ix(a, "allocate_rows", Buffer.toArray(accounts), Array.flatten<Nat8>([u64le(ticketId), u64le(cursor), [Nat8.fromNat(rows)]]));
    };

    /// A Prune row to keep: the snapshot page and four program-id placeholders.
    public func skippedPruneRow(a : Pda.Addresses, snapshotPage : Nat) : [Tx.Meta] {
        [meta(a.snapshot(snapshotPage), false), none(a), none(a), none(a), none(a)];
    };

    public func pruneIx(a : Pda.Addresses, ticketId : Nat64, cursor : Nat64, rows : Nat, remaining : [Tx.Meta]) : Tx.Instruction {
        ix(a, "prune_rows", Array.append<Tx.Meta>([meta(a.ticket, true), meta(a.pool, true)], remaining), Array.flatten<Nat8>([u64le(ticketId), u64le(cursor), [Nat8.fromNat(rows)]]));
    };

    /// The close record when this ticket allocates a closing payout's toll, else `None`.
    public func commitIx(a : Pda.Addresses, ticketId : Nat64, closeId : ?Nat64) : Tx.Instruction {
        let close = switch (closeId) { case (?id) { meta(a.close(id), true) }; case (null) { none(a) } };
        ix(a, "commit_ticket", [meta(a.ticket, true), meta(a.pool, true), close, meta(a.eventAuthority, false), meta(a.programId, false)], u64le(ticketId));
    };

    public func expireIx(a : Pda.Addresses, closeId : Nat64, ownerRefund : Blob) : Tx.Instruction {
        ix(a, "expire_promotion", [
            meta(a.pool, false), meta(a.close(closeId), true), meta(a.ops, true), meta(a.opsCash, true), meta(ownerRefund, true),
            meta(a.systemProgram, false), meta(a.eventAuthority, false), meta(a.programId, false),
        ], []);
    };

    public func finalizeIx(a : Pda.Addresses, closeId : Nat64) : Tx.Instruction {
        ix(a, "finalize_round", [meta(a.pool, true), meta(a.close(closeId), true), meta(a.eventAuthority, false), meta(a.programId, false)], []);
    };

    // ------------------------------------------------------------------
    // Planning
    // ------------------------------------------------------------------

    func clampRows(n : Nat) : Nat { if (n < 1) { 1 } else if (n > MAX_BATCH_ROWS) { MAX_BATCH_ROWS } else { n } };
    func min3(a : Nat, b : Nat, c : Nat) : Nat { Nat.min(a, Nat.min(b, c)) };
    func isZeroKey(k : Blob) : Bool { for (b in k.vals()) { if (b != 0) { return false } }; true };

    func stepOf(name : Text, ticketId : Nat64, cursor : Nat64, rows : Nat, instruction : Tx.Instruction) : Step {
        let position = name # "|t" # Nat64.toText(ticketId) # "|c" # Nat64.toText(cursor);
        { name; position; rows; summary = position # " rows=" # Nat.toText(rows); instruction };
    };

    public func next(v : View) : Plan {
        if (v.pool.activeTicket != 0) { return ticketStep(v) };
        switch (v.pool.currentCloseId) {
            case (null) { #Idle };
            case (?cid) { closeStep(v, cid) };
        };
    };

    func ticketStep(v : View) : Plan {
        let t = switch (v.ticket) { case (?t) { t }; case (null) { return #Blocked("ticket account not loaded") } };
        if (t.id != v.pool.activeTicket) {
            return #Blocked("ticket " # Nat64.toText(t.id) # " is not the pool's active ticket " # Nat64.toText(v.pool.activeTicket));
        };
        let a = v.addr;
        switch (t.phase) {
            case 0 { discoverStep(v, t) };
            case 1 {
                let oldest = if (t.aCount == 0) { null } else { ?t.oldest };
                #Step(stepOf("apply_senior", t.id, t.cursor, 0, applySeniorIx(a, t.id, oldest)));
            };
            case 2 { #Step(stepOf("seal_weights", t.id, t.cursor, 0, sealWeightsIx(a, t.id))) };
            case 3 { allocateStep(v, t) };
            case 4 { pruneStep(v, t) };
            case 5 { #Step(stepOf("commit_ticket", t.id, t.cursor, 0, commitIx(a, t.id, t.closeId))) };
            case 6 { #Blocked("ticket is idle but the pool names it active") };
            case n { #Blocked("unknown ticket phase " # Nat8.toText(n)) };
        };
    };

    func discoverStep(v : View, t : Accounts.Ticket) : Plan {
        if (t.cursor >= t.capturedLen) {
            return #Blocked("discover cursor " # Nat64.toText(t.cursor) # " is past the captured length " # Nat64.toText(t.capturedLen));
        };
        let cursor = Nat64.toNat(t.cursor);
        let left = Nat64.toNat(t.capturedLen) - cursor;
        let rows = min3(clampRows(v.maxRows), left, Accounts.PAGE_CAPACITY - cursor % Accounts.PAGE_CAPACITY);
        let pageIndex = cursor / Accounts.PAGE_CAPACITY;
        let page = switch (v.registryPage(pageIndex)) { case (?p) { p }; case (null) { return #Need(#Registry(pageIndex)) } };
        if (Nat64.toNat(page.index) != pageIndex) {
            return #Blocked("registry page " # Nat.toText(pageIndex) # " reports index " # Nat64.toText(page.index));
        };
        var i : Nat = 0;
        while (i < rows) {
            if (isZeroKey(page.keys[(cursor + i) % Accounts.PAGE_CAPACITY])) { return #Blocked("registry slot " # Nat.toText(cursor + i) # " is empty") };
            i += 1;
        };
        let backerAt = func(slot : Nat64) : Blob { page.keys[Nat64.toNat(slot) % Accounts.PAGE_CAPACITY] };
        #Step(stepOf("discover_rows", t.id, t.cursor, rows, discoverIx(v.addr, t.id, t.cursor, rows, backerAt)));
    };

    func allocateStep(v : View, t : Accounts.Ticket) : Plan {
        if (t.cursor >= t.capturedLen) {
            return #Blocked("allocate cursor " # Nat64.toText(t.cursor) # " is past the captured length " # Nat64.toText(t.capturedLen));
        };
        let cursor = Nat64.toNat(t.cursor);
        let left = Nat64.toNat(t.capturedLen) - cursor;
        let rows = min3(clampRows(v.maxRows), left, Accounts.PAGE_CAPACITY - cursor % Accounts.PAGE_CAPACITY);
        let pageIndex = cursor / Accounts.PAGE_CAPACITY;
        let page = switch (v.snapshotPage(pageIndex)) { case (?p) { p }; case (null) { return #Need(#Snapshot(pageIndex)) } };
        if (Nat64.toNat(page.index) != pageIndex) {
            return #Blocked("snapshot page " # Nat.toText(pageIndex) # " reports index " # Nat64.toText(page.index));
        };
        if (page.ticketId != t.id) {
            return #Blocked("snapshot page " # Nat.toText(pageIndex) # " belongs to ticket " # Nat64.toText(page.ticketId) # ", not " # Nat64.toText(t.id));
        };
        var i : Nat = 0;
        while (i < rows) {
            let row = page.rows[(cursor + i) % Accounts.PAGE_CAPACITY];
            if (Nat64.toNat(row.slot) != cursor + i or isZeroKey(row.key)) {
                return #Blocked("snapshot row " # Nat.toText(cursor + i) # " is not captured");
            };
            i += 1;
        };
        let backerAt = func(slot : Nat64) : Blob { page.rows[Nat64.toNat(slot) % Accounts.PAGE_CAPACITY].key };
        #Step(stepOf("allocate_rows", t.id, t.cursor, rows, allocateIx(v.addr, t.id, t.cursor, rows, backerAt)));
    };

    func pruneStep(v : View, t : Accounts.Ticket) : Plan {
        if (t.cursor == 0) { return #Blocked("prune cursor is 0: nothing left to prune (commit expected)") };
        let cursor = Nat64.toNat(t.cursor);
        let limit = min3(clampRows(v.maxRows), cursor, (cursor - 1) % Accounts.PAGE_CAPACITY + 1);
        let snapIndex = (cursor - 1) / Accounts.PAGE_CAPACITY;
        let snap = switch (v.snapshotPage(snapIndex)) { case (?p) { p }; case (null) { return #Need(#Snapshot(snapIndex)) } };
        if (Nat64.toNat(snap.index) != snapIndex or snap.ticketId != t.id) {
            return #Blocked("snapshot page " # Nat.toText(snapIndex) # " does not belong to ticket " # Nat64.toText(t.id));
        };

        // Earlier swap-deletes of THIS batch: slot -> key now stored there.
        var overrides : [(Nat, Blob)] = [];
        var written : [Blob] = [];
        var len : Nat = Nat64.toNat(v.pool.registryLen);
        let remaining = Buffer.Buffer<Tx.Meta>(5 * limit);
        var rows : Nat = 0;
        var pruned : Nat = 0;

        func isWritten(k : Blob) : Bool { for (w in written.vals()) { if (w == k) { return true } }; false };
        func overridden(slot : Nat) : ?Blob { for ((s, k) in overrides.vals()) { if (s == slot) { return ?k } }; null };

        label rowsLoop while (rows < limit) {
            let slot = cursor - 1 - rows;
            let row = snap.rows[slot % Accounts.PAGE_CAPACITY];
            if (Nat64.toNat(row.slot) != slot) { return #Blocked("snapshot row " # Nat.toText(slot) # " is not captured") };
            let capturedClosed = row.remaining <= Accounts.CUTOFF;
            var prune = capturedClosed;
            if (not capturedClosed and not v.skipNewlyCapped) {
                switch (v.liveRemaining(row.key)) {
                    case (null) { return #Need(#Backer(row.key)) };
                    case (?live) { prune := live <= Accounts.CUTOFF };
                };
            };
            if (prune) {
                if (len == 0) { return #Blocked("the registry is empty but a row is to be pruned") };
                let last = len - 1;
                // Registry keys at `slot` (before this row) and at `last`.
                let slotPageIndex = slot / Accounts.PAGE_CAPACITY;
                let lastPageIndex = last / Accounts.PAGE_CAPACITY;
                let slotPage = switch (v.registryPage(slotPageIndex)) { case (?p) { p }; case (null) { return #Need(#Registry(slotPageIndex)) } };
                let lastPage = switch (v.registryPage(lastPageIndex)) { case (?p) { p }; case (null) { return #Need(#Registry(lastPageIndex)) } };
                let slotKey = switch (overridden(slot)) { case (?k) { k }; case (null) { slotPage.keys[slot % Accounts.PAGE_CAPACITY] } };
                if (slotKey != row.key) { return #Blocked("registry slot " # Nat.toText(slot) # " holds a different backer than the snapshot row") };
                let moved = switch (overridden(last)) { case (?k) { k }; case (null) { lastPage.keys[last % Accounts.PAGE_CAPACITY] } };
                if (slot != last and isWritten(moved)) { break rowsLoop };
                remaining.add(meta(v.addr.snapshot(slot / Accounts.PAGE_CAPACITY), false));
                remaining.add(meta(row.key, true));
                remaining.add(meta(v.addr.registry(slotPageIndex), true));
                remaining.add(meta(v.addr.registry(lastPageIndex), true));
                remaining.add(meta(moved, true));
                written := Array.append<Blob>(written, [row.key]);
                if (slot != last) {
                    written := Array.append<Blob>(written, [moved]);
                    overrides := Array.append<(Nat, Blob)>(overrides, [(slot, moved)]);
                };
                len -= 1;
                pruned += 1;
            } else {
                for (m in skippedPruneRow(v.addr, slot / Accounts.PAGE_CAPACITY).vals()) { remaining.add(m) };
            };
            rows += 1;
        };
        let instruction = pruneIx(v.addr, t.id, t.cursor, rows, Buffer.toArray(remaining));
        let s = stepOf("prune_rows", t.id, t.cursor, rows, instruction);
        #Step({ s with summary = s.summary # " pruned=" # Nat.toText(pruned) });
    };

    // ------------------------------------------------------------------
    // Closing pipeline (supported part)
    // ------------------------------------------------------------------

    func closeStep(v : View, cid : Nat64) : Plan {
        let c = switch (v.close) { case (?c) { c }; case (null) { return #Blocked("close record not loaded") } };
        if (c.closeId != cid) { return #Blocked("close record " # Nat64.toText(c.closeId) # " is not the pool's current close " # Nat64.toText(cid)) };
        if (c.finalized) { return #Idle };
        let waitCash = "waiting for the closing allocation run to commit its cash";
        let closeKey = "|k" # Nat64.toText(cid);
        switch (c.promotion) {
            case (#Pending) {};
            case (_) {
                if (not c.cashCommitted) { return #Blocked(waitCash) };
                if (not c.reserveSettled) { return #Blocked("settling the round's processing-cost reserve") };
                let position = "finalize_round" # closeKey;
                return #Step({ name = "finalize_round"; position; rows = 0; summary = position; instruction = finalizeIx(v.addr, cid) });
            };
        };
        if (not c.cashCommitted) { return #Blocked(waitCash) };
        if (c.drawState == Accounts.DRAW_CAPTURED) { return #Blocked("unsupported step: apply_promotion") };
        if (v.nowSecs > c.promotionDeadline) {
            if (v.nowSecs <= c.promotionDeadline + v.deadlineMarginSecs) { return #Blocked("waiting for the promotion deadline margin") };
            let position = "expire_promotion" # closeKey;
            return #Step({ name = "expire_promotion"; position; rows = 0; summary = position; instruction = expireIx(v.addr, cid, c.ownerRefundDestination) });
        };
        if (not c.sealed) {
            if (c.provisionedPreferredPages < c.requiredPagesPerSet or c.provisionedFallbackPages < c.requiredPagesPerSet) {
                return #Blocked("unsupported step: provision_page");
            };
            return #Blocked("unsupported step: build_candidates");
        };
        if (c.drawState == Accounts.DRAW_NOT_REQUESTED) { return #Blocked("unsupported step: request_round_draw") };
        if (c.drawState == Accounts.DRAW_REQUESTED) { return #Blocked("unsupported step: capture_round_draw") };
        #Blocked("unexpected randomness state " # Nat8.toText(c.drawState));
    };
};
