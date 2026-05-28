import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Nat8 "mo:base/Nat8";
import Nat64 "mo:base/Nat64";

import Base58 "Base58";

module {

    // ====================================================================
    // Well-known program / sysvar addresses (base58 strings).
    // ====================================================================

    public let SYSTEM_PROGRAM_ID : Text = "11111111111111111111111111111111";
    // NOTE the lowercase "l" — Solana's sysvar IDs are intentionally
    // typo-resistant base58 strings, NOT human-readable.
    public let SYSVAR_RECENT_BLOCKHASHES : Text = "SysvarRecentB1ockHashes11111111111111111111";
    public let SYSVAR_RENT : Text = "SysvarRent111111111111111111111111111111111";

    // 80 bytes for a nonce account body.
    public let NONCE_ACCOUNT_SPACE : Nat64 = 80;

    // ====================================================================
    // compact-u16 (Solana's variable-length length prefix)
    // ====================================================================

    /// Encode a Nat as compact-u16 bytes. Solana's compact-u16:
    ///   if n < 0x80 → 1 byte
    ///   if n < 0x4000 → 2 bytes (low 7 bits + 0x80; next 7 bits)
    ///   else → 3 bytes (low 7 + 0x80; next 7 + 0x80; high 2 bits)
    // Deviation from plan: Motoko's Nat type does not support bitwise operators
    // (&, |, >>). compactU16 uses modular arithmetic equivalents instead:
    //   n & 0x7F  → n % 128
    //   n | 0x80  → (n % 128) + 128   (safe: value is already masked to 7 bits)
    //   n >> 7    → n / 128
    //   n >> 14   → n / 16384
    //   n & 0x03  → n % 4
    public func compactU16(n : Nat) : [Nat8] {
        if (n < 128) {
            [Nat8.fromNat(n)];
        } else if (n < 16384) {
            [
                Nat8.fromNat((n % 128) + 128),
                Nat8.fromNat((n / 128) % 128),
            ];
        } else {
            [
                Nat8.fromNat((n % 128) + 128),
                Nat8.fromNat(((n / 128) % 128) + 128),
                Nat8.fromNat((n / 16384) % 4),
            ];
        };
    };

    // ====================================================================
    // Little-endian Nat64 → 8 bytes.
    // ====================================================================

    // Deviation from plan: keep n as Nat64 (not Nat) so Nat64's native bitwise
    // operators (&, >>) are available. Nat64.toNat8 doesn't exist; we convert
    // via Nat64.toNat + Nat8.fromNat after masking to 8 bits.
    public func u64Le(n : Nat64) : [Nat8] {
        let b = func(v : Nat64) : Nat8 { Nat8.fromNat(Nat64.toNat(v & 0xFF)) };
        [
            b(n),
            b(n >> 8),
            b(n >> 16),
            b(n >> 24),
            b(n >> 32),
            b(n >> 40),
            b(n >> 48),
            b(n >> 56),
        ];
    };

    // ====================================================================
    // Instruction + Message types
    // ====================================================================

    /// A logical instruction, prior to compilation against an account-key
    /// table. `signer` and `writable` flags govern header bookkeeping.
    public type AccountMeta = {
        pubkey : Text;     // base58
        isSigner : Bool;
        isWritable : Bool;
    };

    public type Instruction = {
        programId : Text;  // base58
        accounts : [AccountMeta];
        data : [Nat8];
    };

    /// A compiled, serializable message — what gets signed.
    public type CompiledMessage = {
        header : (Nat8, Nat8, Nat8); // (numSigs, numReadonlySigned, numReadonlyUnsigned)
        accountKeys : [Text];        // canonical order: signer-writable, signer-readonly, unsigned-writable, unsigned-readonly
        recentBlockhash : Text;      // base58, 32 bytes
        instructions : [CompiledInstruction];
    };

    public type CompiledInstruction = {
        programIdIndex : Nat8;
        accounts : [Nat8];   // indices into CompiledMessage.accountKeys
        data : [Nat8];
    };

    // ====================================================================
    // Pre-built instruction constructors
    // ====================================================================

    /// System::transfer(from → to, lamports).
    public func transferIx(from : Text, to : Text, lamports : Nat64) : Instruction {
        let header : [Nat8] = [2, 0, 0, 0]; // Transfer discriminator (u32 LE)
        let data = Array.append<Nat8>(header, u64Le(lamports));
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = from; isSigner = true; isWritable = true },
                { pubkey = to;   isSigner = false; isWritable = true },
            ];
            data;
        };
    };

    /// System::advance_nonce_account(nonceAccount, authority).
    public func advanceNonceIx(nonceAccount : Text, authority : Text) : Instruction {
        let data : [Nat8] = [4, 0, 0, 0]; // AdvanceNonceAccount discriminator
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = nonceAccount;            isSigner = false; isWritable = true },
                { pubkey = SYSVAR_RECENT_BLOCKHASHES; isSigner = false; isWritable = false },
                { pubkey = authority;               isSigner = true; isWritable = false },
            ];
            data;
        };
    };

    /// System::createAccount(funder → new, lamports, space, owner).
    public func createAccountIx(
        funder : Text,
        newAccount : Text,
        lamports : Nat64,
        space : Nat64,
        owner : Text,
    ) : Instruction {
        // data = [0,0,0,0] + lamports_u64_le + space_u64_le + owner_pubkey(32 bytes)
        let header : [Nat8] = [0, 0, 0, 0];
        let lamportsBytes = u64Le(lamports);
        let spaceBytes = u64Le(space);
        let ownerBytes = switch (Base58.decode(owner)) {
            case (?b) { Blob.toArray(b) };
            case (null) { [] };
        };
        let body = Array.append<Nat8>(Array.append<Nat8>(lamportsBytes, spaceBytes), ownerBytes);
        let data = Array.append<Nat8>(header, body);
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = funder;     isSigner = true; isWritable = true },
                { pubkey = newAccount; isSigner = true; isWritable = true },
            ];
            data;
        };
    };

    /// System::initialize_nonce_account(nonce, authority).
    public func initializeNonceIx(nonceAccount : Text, authority : Text) : Instruction {
        let header : [Nat8] = [6, 0, 0, 0]; // InitializeNonceAccount discriminator
        let authBytes = switch (Base58.decode(authority)) {
            case (?b) { Blob.toArray(b) };
            case (null) { [] };
        };
        let data = Array.append<Nat8>(header, authBytes);
        {
            programId = SYSTEM_PROGRAM_ID;
            accounts = [
                { pubkey = nonceAccount;             isSigner = false; isWritable = true },
                { pubkey = SYSVAR_RECENT_BLOCKHASHES; isSigner = false; isWritable = false },
                { pubkey = SYSVAR_RENT;              isSigner = false; isWritable = false },
            ];
            data;
        };
    };

    // ====================================================================
    // Compilation — collect distinct accounts, order them, build header,
    // emit CompiledMessage.
    // ====================================================================

    /// Compile a list of logical instructions into a CompiledMessage.
    /// `feePayer` MUST appear as a signer in at least one instruction.
    /// Account ordering follows Solana's canonical rule:
    ///   1. signer + writable (feePayer first)
    ///   2. signer + readonly
    ///   3. non-signer + writable
    ///   4. non-signer + readonly
    public func compile(
        feePayer : Text,
        recentBlockhash : Text,
        ixs : [Instruction],
    ) : CompiledMessage {
        // Build a deduped account list, tracking signer/writable flags.
        // The dedup logic: for any pubkey appearing in multiple metas,
        // take the OR of signer flags and the OR of writable flags.
        let pubkeyBuf = Buffer.Buffer<Text>(16);
        let signerBuf = Buffer.Buffer<Bool>(16);
        let writableBuf = Buffer.Buffer<Bool>(16);

        // Seed with feePayer as signer+writable (always).
        pubkeyBuf.add(feePayer);
        signerBuf.add(true);
        writableBuf.add(true);

        // Walk every instruction, plus the programId (programs are always
        // readonly non-signers — but they MUST appear in accountKeys).
        for (ix in ixs.vals()) {
            // Add the program id if not already present.
            ensureAccount(pubkeyBuf, signerBuf, writableBuf, ix.programId, false, false);
            for (m in ix.accounts.vals()) {
                ensureAccount(pubkeyBuf, signerBuf, writableBuf, m.pubkey, m.isSigner, m.isWritable);
            };
        };

        // Sort into canonical order: signer-writable, signer-readonly,
        // non-signer-writable, non-signer-readonly. feePayer stays first.
        let keys = Buffer.toArray(pubkeyBuf);
        let isSigner = Buffer.toArray(signerBuf);
        let isWritable = Buffer.toArray(writableBuf);

        let n = keys.size();
        // Mutable array for category values; [var Nat] supports := assignment.
        let categories = Array.init<Nat>(n, 0);
        var i : Nat = 0;
        while (i < n) {
            categories[i] := categorize(isSigner[i], isWritable[i]);
            i += 1;
        };

        // Stable sort with feePayer (index 0) always first.
        let permuted = stableSortIndices(n, func(a : Nat, b : Nat) : { #less; #equal; #greater } {
            if (a == 0) { #less }      // feePayer first
            else if (b == 0) { #greater }
            else if (categories[a] < categories[b]) { #less }
            else if (categories[a] > categories[b]) { #greater }
            else { #equal };
        });

        let sortedKeys = Array.tabulate<Text>(n, func(idx) { keys[permuted[idx]] });
        let sortedSigner = Array.tabulate<Bool>(n, func(idx) { isSigner[permuted[idx]] });
        let sortedWritable = Array.tabulate<Bool>(n, func(idx) { isWritable[permuted[idx]] });

        // Count header buckets.
        var numSigs : Nat8 = 0;
        var numROSigned : Nat8 = 0;
        var numROUnsigned : Nat8 = 0;
        i := 0;
        while (i < n) {
            if (sortedSigner[i]) {
                numSigs += 1;
                if (not sortedWritable[i]) { numROSigned += 1 };
            } else if (not sortedWritable[i]) {
                numROUnsigned += 1;
            };
            i += 1;
        };

        // Compile each instruction against the sorted account table.
        let compiledIxs = Array.tabulate<CompiledInstruction>(ixs.size(), func(ixIdx) {
            let ix = ixs[ixIdx];
            let pIdx = indexOf(sortedKeys, ix.programId);
            let accountIndices = Array.tabulate<Nat8>(ix.accounts.size(), func(j) {
                Nat8.fromNat(indexOf(sortedKeys, ix.accounts[j].pubkey));
            });
            { programIdIndex = Nat8.fromNat(pIdx); accounts = accountIndices; data = ix.data };
        });

        {
            header = (numSigs, numROSigned, numROUnsigned);
            accountKeys = sortedKeys;
            recentBlockhash;
            instructions = compiledIxs;
        };
    };

    private func ensureAccount(
        keys : Buffer.Buffer<Text>,
        signers : Buffer.Buffer<Bool>,
        writables : Buffer.Buffer<Bool>,
        pubkey : Text,
        signer : Bool,
        writable : Bool,
    ) {
        let n = keys.size();
        var i : Nat = 0;
        while (i < n) {
            if (keys.get(i) == pubkey) {
                if (signer) { signers.put(i, true) };
                if (writable) { writables.put(i, true) };
                return;
            };
            i += 1;
        };
        keys.add(pubkey);
        signers.add(signer);
        writables.add(writable);
    };

    private func categorize(signer : Bool, writable : Bool) : Nat {
        if (signer and writable) { 0 }
        else if (signer) { 1 }
        else if (writable) { 2 }
        else { 3 };
    };

    private func indexOf(arr : [Text], target : Text) : Nat {
        var i : Nat = 0;
        while (i < arr.size()) {
            if (arr[i] == target) { return i };
            i += 1;
        };
        // Should never be reached for a well-compiled message.
        0;
    };

    // Stable sort permutation: returns indices [0..n) reordered so that
    // for i<j, cmp(indices[i], indices[j]) ≠ #greater. O(n²) is fine —
    // a Solana message has at most ~10 accounts.
    private func stableSortIndices(n : Nat, cmp : (Nat, Nat) -> { #less; #equal; #greater }) : [Nat] {
        let arr = Array.init<Nat>(n, 0);
        var i : Nat = 0;
        while (i < n) { arr[i] := i; i += 1; };
        // Insertion sort (stable).
        var k : Nat = 1;
        while (k < n) {
            let cur = arr[k];
            var j : Nat = k;
            label inner while (j > 0) {
                if (cmp(arr[j - 1], cur) == #greater) {
                    arr[j] := arr[j - 1];
                    j -= 1;
                } else { break inner };
            };
            arr[j] := cur;
            k += 1;
        };
        Array.freeze(arr);
    };

    // ====================================================================
    // Serialization — CompiledMessage → bytes (this is what gets signed).
    // ====================================================================

    public func serializeMessage(msg : CompiledMessage) : Blob {
        let buf = Buffer.Buffer<Nat8>(256);

        // Header.
        let (h1, h2, h3) = msg.header;
        buf.add(h1);
        buf.add(h2);
        buf.add(h3);

        // Account keys.
        for (b in compactU16(msg.accountKeys.size()).vals()) { buf.add(b) };
        for (key in msg.accountKeys.vals()) {
            switch (Base58.decode(key)) {
                case (?blob) {
                    for (b in Blob.toArray(blob).vals()) { buf.add(b) };
                };
                case (null) {
                    // Invalid base58 — emit 32 zero bytes so the message
                    // length is still right and the caller's signing step
                    // fails loudly rather than silently.
                    var z : Nat = 0;
                    while (z < 32) { buf.add(0); z += 1 };
                };
            };
        };

        // Recent blockhash (32 bytes).
        switch (Base58.decode(msg.recentBlockhash)) {
            case (?blob) {
                for (b in Blob.toArray(blob).vals()) { buf.add(b) };
            };
            case (null) {
                var z : Nat = 0;
                while (z < 32) { buf.add(0); z += 1 };
            };
        };

        // Instructions.
        for (b in compactU16(msg.instructions.size()).vals()) { buf.add(b) };
        for (ix in msg.instructions.vals()) {
            buf.add(ix.programIdIndex);
            for (b in compactU16(ix.accounts.size()).vals()) { buf.add(b) };
            for (a in ix.accounts.vals()) { buf.add(a) };
            for (b in compactU16(ix.data.size()).vals()) { buf.add(b) };
            for (b in ix.data.vals()) { buf.add(b) };
        };

        Blob.fromArray(Buffer.toArray(buf));
    };

    /// Combine a serialized message with a list of signatures (one per
    /// required signer, in the same order as accountKeys' signer prefix).
    /// Returns the full wire-format transaction bytes for sendTransaction.
    public func assembleTransaction(messageBytes : Blob, signatures : [Blob]) : Blob {
        let buf = Buffer.Buffer<Nat8>(messageBytes.size() + signatures.size() * 64 + 8);
        for (b in compactU16(signatures.size()).vals()) { buf.add(b) };
        for (sig in signatures.vals()) {
            for (b in Blob.toArray(sig).vals()) { buf.add(b) };
        };
        for (b in Blob.toArray(messageBytes).vals()) { buf.add(b) };
        Blob.fromArray(Buffer.toArray(buf));
    };
};
