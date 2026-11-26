import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Debug "mo:base/Debug";
import Nat "mo:base/Nat";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";

import Base58 "Base58";

/// Tx - Solana legacy transaction messages: compile, serialize, parse, wire.
///
/// The watchdog is the only signer of what it builds and it always signs a
/// message this module compiled, so the format here is deliberately the plain
/// legacy one: `header(3) ++ keys ++ blockhash ++ instructions`.
///
/// KEY ORDER (runtime requirement): signers (writable first), then non-signers
/// (writable first), each class in first-appearance order; the fee payer is
/// always key 0 (signer + writable). A key used several times is one key whose
/// flags are the OR of its uses. web3.js orders each class by base58 text
/// instead; both are valid, `test/tx.test.mo` compares messages semantically.
module {
    public let COMPUTE_BUDGET_PROGRAM : Text = "ComputeBudget111111111111111111111111111111";
    public let BASE_FEE_PER_SIGNATURE : Nat64 = 5_000;

    public type Meta = { key : Blob; isSigner : Bool; isWritable : Bool };
    public type Instruction = { programId : Blob; accounts : [Meta]; data : Blob };

    public type Compiled = {
        bytes : Blob;
        accountKeys : [Blob];
        numRequiredSignatures : Nat;
        numReadonlySigned : Nat;
        numReadonlyUnsigned : Nat;
        programIdIndexes : [Nat];
        instructionAccountIndexes : [[Nat]];
    };

    public type ParsedInstruction = { programIdIndex : Nat; accounts : [Nat]; data : Blob };
    public type Parsed = {
        numRequiredSignatures : Nat;
        numReadonlySigned : Nat;
        numReadonlyUnsigned : Nat;
        accountKeys : [Blob];
        blockhash : Blob;
        instructions : [ParsedInstruction];
    };

    // ------------------------------------------------------------------
    // shortvec (compact-u16)
    // ------------------------------------------------------------------

    public func shortvec(n : Nat) : [Nat8] {
        let out = Buffer.Buffer<Nat8>(3);
        var v = n;
        label l loop {
            let low = v % 128;
            v := v / 128;
            if (v > 0) { out.add(Nat8.fromNat(low + 128)) } else { out.add(Nat8.fromNat(low)); break l };
        };
        Buffer.toArray(out);
    };

    // ------------------------------------------------------------------
    // compile
    // ------------------------------------------------------------------

    type Slot = { key : Blob; var signer : Bool; var writable : Bool };

    func classOf(s : Slot) : Nat {
        if (s.signer and s.writable) { 0 } else if (s.signer) { 1 } else if (s.writable) { 2 } else { 3 };
    };

    public func compile(feePayer : Blob, blockhash : Blob, ixs : [Instruction]) : Compiled {
        let slots = Buffer.Buffer<Slot>(16);
        slots.add({ key = feePayer; var signer = true; var writable = true });
        func touch(key : Blob, signer : Bool, writable : Bool) {
            for (s in slots.vals()) {
                if (s.key == key) {
                    if (signer) { s.signer := true };
                    if (writable) { s.writable := true };
                    return;
                };
            };
            slots.add({ key; var signer; var writable });
        };
        for (ix in ixs.vals()) {
            for (m in ix.accounts.vals()) { touch(m.key, m.isSigner, m.isWritable) };
            touch(ix.programId, false, false);
        };

        let ordered = Buffer.Buffer<Slot>(slots.size());
        var cls : Nat = 0;
        while (cls < 4) {
            for (s in slots.vals()) { if (classOf(s) == cls) { ordered.add(s) } };
            cls += 1;
        };
        if (ordered.size() > 256) { Debug.trap("a legacy message addresses at most 256 accounts") };

        let keys = Array.tabulate<Blob>(ordered.size(), func(i : Nat) : Blob { ordered.get(i).key });
        func indexOf(key : Blob) : Nat {
            var i : Nat = 0;
            while (i < keys.size()) { if (keys[i] == key) { return i }; i += 1 };
            Debug.trap("unreachable: key missing from the compiled key list");
        };
        var numSigners : Nat = 0;
        var roSigned : Nat = 0;
        var roUnsigned : Nat = 0;
        for (s in ordered.vals()) {
            if (s.signer) { numSigners += 1; if (not s.writable) { roSigned += 1 } } else if (not s.writable) { roUnsigned += 1 };
        };

        let programIdIndexes = Array.map<Instruction, Nat>(ixs, func(ix : Instruction) : Nat { indexOf(ix.programId) });
        let accountIndexes = Array.map<Instruction, [Nat]>(ixs, func(ix : Instruction) : [Nat] { Array.map<Meta, Nat>(ix.accounts, func(m : Meta) : Nat { indexOf(m.key) }) });

        let out = Buffer.Buffer<Nat8>(512);
        out.add(Nat8.fromNat(numSigners));
        out.add(Nat8.fromNat(roSigned));
        out.add(Nat8.fromNat(roUnsigned));
        for (b in shortvec(keys.size()).vals()) { out.add(b) };
        for (k in keys.vals()) { for (b in k.vals()) { out.add(b) } };
        for (b in blockhash.vals()) { out.add(b) };
        for (b in shortvec(ixs.size()).vals()) { out.add(b) };
        var i : Nat = 0;
        while (i < ixs.size()) {
            out.add(Nat8.fromNat(programIdIndexes[i]));
            for (b in shortvec(accountIndexes[i].size()).vals()) { out.add(b) };
            for (ai in accountIndexes[i].vals()) { out.add(Nat8.fromNat(ai)) };
            for (b in shortvec(ixs[i].data.size()).vals()) { out.add(b) };
            for (b in ixs[i].data.vals()) { out.add(b) };
            i += 1;
        };
        {
            bytes = Blob.fromArray(Buffer.toArray(out));
            accountKeys = keys;
            numRequiredSignatures = numSigners;
            numReadonlySigned = roSigned;
            numReadonlyUnsigned = roUnsigned;
            programIdIndexes;
            instructionAccountIndexes = accountIndexes;
        };
    };

    // ------------------------------------------------------------------
    // parse (the inverse; used to re-check what is about to be signed)
    // ------------------------------------------------------------------

    public func parse(message : Blob) : ?Parsed {
        let a = Blob.toArray(message);
        var pos : Nat = 0;
        var bad = false;
        func byte() : Nat {
            if (pos >= a.size()) { bad := true; return 0 };
            let v = Nat8.toNat(a[pos]);
            pos += 1;
            v;
        };
        func vec() : Nat {
            var v : Nat = 0;
            var mul : Nat = 1;
            var n : Nat = 0;
            label l loop {
                let b = byte();
                if (bad) { return 0 };
                v += (b % 128) * mul;
                if (b < 128) { break l };
                mul *= 128;
                n += 1;
                if (n >= 3) { bad := true; return 0 };
            };
            v;
        };
        func bytes(n : Nat) : Blob {
            if (pos + n > a.size()) { bad := true; return Blob.fromArray([]) };
            let start = pos;
            pos += n;
            Blob.fromArray(Array.tabulate<Nat8>(n, func(i : Nat) : Nat8 { a[start + i] }));
        };
        let numRequiredSignatures = byte();
        let numReadonlySigned = byte();
        let numReadonlyUnsigned = byte();
        let nKeys = vec();
        if (bad) { return null };
        let keys = Array.tabulate<Blob>(nKeys, func(_ : Nat) : Blob { bytes(32) });
        let blockhash = bytes(32);
        let nIx = vec();
        if (bad) { return null };
        let ixs = Array.tabulate<ParsedInstruction>(nIx, func(_ : Nat) : ParsedInstruction {
            let programIdIndex = byte();
            let nAcc = vec();
            let accounts = Array.tabulate<Nat>(nAcc, func(_ : Nat) : Nat { byte() });
            let nData = vec();
            let data = bytes(nData);
            { programIdIndex; accounts; data };
        });
        if (bad or pos != a.size()) { return null };
        if (numRequiredSignatures > nKeys or numReadonlySigned > numRequiredSignatures or numReadonlyUnsigned > nKeys - numRequiredSignatures) { return null };
        for (ix in ixs.vals()) {
            if (ix.programIdIndex >= nKeys) { return null };
            for (i in ix.accounts.vals()) { if (i >= nKeys) { return null } };
        };
        ?{ numRequiredSignatures; numReadonlySigned; numReadonlyUnsigned; accountKeys = keys; blockhash; instructions = ixs };
    };

    /// Instructions with account flags recovered from the header (a key's flags
    /// are the message-wide OR, exactly what the runtime sees).
    public func resolve(p : Parsed) : [Instruction] {
        let n = p.accountKeys.size();
        func isSigner(i : Nat) : Bool { i < p.numRequiredSignatures };
        func isWritable(i : Nat) : Bool {
            if (isSigner(i)) { i < p.numRequiredSignatures - p.numReadonlySigned } else { i < n - p.numReadonlyUnsigned };
        };
        Array.map<ParsedInstruction, Instruction>(
            p.instructions,
            func(ix : ParsedInstruction) : Instruction {
                {
                    programId = p.accountKeys[ix.programIdIndex];
                    data = ix.data;
                    accounts = Array.map<Nat, Meta>(ix.accounts, func(i : Nat) : Meta { { key = p.accountKeys[i]; isSigner = isSigner(i); isWritable = isWritable(i) } });
                };
            },
        );
    };

    // ------------------------------------------------------------------
    // wire + helpers
    // ------------------------------------------------------------------

    /// `shortvec(1) ++ signature ++ message`: the bytes `sendTransaction` takes.
    public func wire(message : Blob, signature : Blob) : Blob {
        Blob.fromArray(Array.append<Nat8>(Array.append<Nat8>(shortvec(1), Blob.toArray(signature)), Blob.toArray(message)));
    };

    /// A transaction's id is its base58 first signature.
    public func signatureText(signature : Blob) : Text { Base58.encode(signature) };

    func u32le(n : Nat32) : [Nat8] {
        Array.tabulate<Nat8>(4, func(i : Nat) : Nat8 { Nat8.fromNat(Nat32.toNat((n >> (Nat32.fromNat(i) * 8)) & 0xFF)) });
    };
    func u64le(n : Nat64) : [Nat8] {
        Array.tabulate<Nat8>(8, func(i : Nat) : Nat8 { Nat8.fromNat(Nat64.toNat((n >> (Nat64.fromNat(i) * 8)) & 0xFF)) });
    };

    /// `SetComputeUnitLimit` (when `limit > 0`) and `SetComputeUnitPrice` (when
    /// `priceMicroLamports > 0`). Both zero -> none: the default budget and the
    /// base fee only.
    public func computeBudgetInstructions(limit : Nat32, priceMicroLamports : Nat64) : [Instruction] {
        let program = switch (Base58.decode(COMPUTE_BUDGET_PROGRAM)) { case (?b) { b }; case (null) { Debug.trap("compute budget program id") } };
        let out = Buffer.Buffer<Instruction>(2);
        if (limit > 0) {
            out.add({ programId = program; accounts = []; data = Blob.fromArray(Array.append<Nat8>([2], u32le(limit))) });
        };
        if (priceMicroLamports > 0) {
            out.add({ programId = program; accounts = []; data = Blob.fromArray(Array.append<Nat8>([3], u64le(priceMicroLamports))) });
        };
        Buffer.toArray(out);
    };

    /// Lamports the network charges: 5000 per signature plus the priority fee
    /// `ceil(limit * price / 1_000_000)`.
    public func feeLamports(c : Compiled, limit : Nat32, priceMicroLamports : Nat64) : Nat64 {
        let base = BASE_FEE_PER_SIGNATURE * Nat64.fromNat(c.numRequiredSignatures);
        let priority = (Nat32.toNat(limit) * Nat64.toNat(priceMicroLamports) + 999_999) / 1_000_000;
        base + Nat64.fromNat(priority);
    };
};
