import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import HashMap "mo:base/HashMap";
import Nat "mo:base/Nat";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";
import Text "mo:base/Text";

import Sha256 "Sha256";

/// Pda - Solana program-derived addresses in Motoko.
///
/// WHY THIS EXISTS. The keeper must name registry/snapshot pages and the close
/// record, whose addresses depend on a page index or close id. Solana derives a
/// PDA as `sha256(seeds ++ [bump] ++ program_id ++ "ProgramDerivedAddress")`
/// for the highest bump (255 downwards) whose hash is NOT a valid Ed25519
/// point. Motoko has no ed25519 verification, so the point test is done here
/// with big-integer arithmetic (the curve25519 decompression criterion).
///
/// ON-CURVE TEST (RFC 8032 section 5.1.3): with y = the low 255 bits of the
/// 32-byte string (little endian; bit 255 is the x sign, ignored), a point
/// exists iff u/v is a square mod p where u = y^2 - 1, v = d*y^2 + 1,
/// p = 2^255 - 19 and d = -121665/121666. Because v^2 is a square,
/// u/v is a square iff u*v is, i.e. Euler's criterion (u*v)^((p-1)/2) in {0, 1}.
/// u = 0 (y = +-1) is the x = 0 point and is on the curve. Same decision as
/// curve25519-dalek's `CompressedEdwardsY::decompress().is_some()`, which is what
/// `Pubkey::create_program_address` uses. Pinned against @solana/web3.js
/// vectors (27 on / 26 off the curve) and 34 PDAs in `test/pda.test.mo`.
module {
    /// p = 2^255 - 19
    private let P : Nat = 57896044618658097711785492504343953926634992332820282019728792003956564819949;
    /// d = -121665 / 121666 mod p
    private let D : Nat = 37095705934669439343138083508754565189542113879843219016388785533085940283555;
    /// (p - 1) / 2
    private let HALF : Nat = 28948022309329048855892746252171976963317496166410141009864396001978282409974;

    func modPow(base : Nat, exp : Nat, m : Nat) : Nat {
        var result : Nat = 1;
        var b = base % m;
        var e = exp;
        while (e > 0) {
            if (e % 2 == 1) { result := (result * b) % m };
            b := (b * b) % m;
            e := e / 2;
        };
        result;
    };

    public func isOnCurve(point : Blob) : Bool {
        if (point.size() != 32) { return false };
        let a = Blob.toArray(point);
        var y : Nat = 0;
        var i : Nat = 32;
        while (i > 0) {
            i -= 1;
            let byte = if (i == 31) { Nat8.toNat(a[i] & 0x7F) } else { Nat8.toNat(a[i]) };
            y := y * 256 + byte;
        };
        let y2 = (y * y) % P;
        let u = (y2 + P - 1) % P;
        let v = (D * y2 + 1) % P;
        if (v == 0) { return u == 0 };
        let t = (u * v) % P;
        if (t == 0) { return true };
        modPow(t, HALF, P) == 1;
    };

    /// `(address, bump)` of the canonical PDA for `seeds` under `programId`.
    public func find(seeds : [Blob], programId : Blob) : ?(Blob, Nat8) {
        let marker = Blob.toArray("ProgramDerivedAddress" : Blob);
        let pid = Blob.toArray(programId);
        var bump : Nat = 255;
        loop {
            var buf : [Nat8] = [];
            for (s in seeds.vals()) { buf := Array.append<Nat8>(buf, Blob.toArray(s)) };
            buf := Array.append<Nat8>(buf, [Nat8.fromNat(bump)]);
            buf := Array.append<Nat8>(buf, pid);
            buf := Array.append<Nat8>(buf, marker);
            let hash = Sha256.digest(Blob.fromArray(buf));
            if (not isOnCurve(hash)) { return ?(hash, Nat8.fromNat(bump)) };
            if (bump == 0) { return null };
            bump -= 1;
        };
    };

    public func u64le(n : Nat64) : Blob {
        Blob.fromArray(Array.tabulate<Nat8>(8, func(i : Nat) : Nat8 { Nat8.fromNat(Nat64.toNat((n >> (Nat64.fromNat(i) * 8)) & 0xFF)) }));
    };

    public func u32le(n : Nat32) : Blob {
        Blob.fromArray(Array.tabulate<Nat8>(4, func(i : Nat) : Nat8 { Nat8.fromNat(Nat32.toNat((n >> (Nat32.fromNat(i) * 8)) & 0xFF)) }));
    };

    /// The program's canonical addresses (seeds from `state.rs` / the IDL `pda`
    /// entries, cross-checked with `frontend/src/solana/native/accounts.ts`).
    public type Addresses = {
        programId : Blob;
        pool : Blob;
        ticket : Blob;
        ops : Blob;
        opsCash : Blob;
        eventAuthority : Blob;
        systemProgram : Blob;
        registry : Nat -> Blob;
        snapshot : Nat -> Blob;
        close : Nat64 -> Blob;
    };

    public func addresses(programId : Blob) : Addresses {
        func derive(seeds : [Blob]) : Blob {
            switch (find(seeds, programId)) {
                case (?(addr, _)) { addr };
                case (null) { Debug.trap("no program-derived address for these seeds") };
            };
        };
        func text(t : Text) : Blob { Text.encodeUtf8(t) };
        let cache = HashMap.HashMap<Text, Blob>(16, Text.equal, Text.hash);
        func cached(kind : Text, index : Nat64) : Blob {
            let k = kind # ":" # Nat64.toText(index);
            switch (cache.get(k)) {
                case (?b) { b };
                case (null) {
                    let b = derive([text(kind), u64le(index)]);
                    cache.put(k, b);
                    b;
                };
            };
        };
        {
            programId;
            pool = derive([text("pool")]);
            ticket = derive([text("ticket")]);
            ops = derive([text("ops")]);
            opsCash = derive([text("ops-cash")]);
            eventAuthority = derive([text("__event_authority")]);
            systemProgram = Blob.fromArray(Array.tabulate<Nat8>(32, func _ { 0 }));
            registry = func(i : Nat) : Blob { cached("registry", Nat64.fromNat(i)) };
            snapshot = func(i : Nat) : Blob { cached("snapshot", Nat64.fromNat(i)) };
            close = func(id : Nat64) : Blob { cached("close", id) };
        };
    };
};
