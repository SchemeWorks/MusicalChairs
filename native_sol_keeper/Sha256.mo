import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Nat8 "mo:base/Nat8";

/// SHA-256 (FIPS 180-4). Needed only to derive program-derived addresses:
/// `sha256(seeds ++ [bump] ++ program id ++ "ProgramDerivedAddress")`.
/// Verified against Python `hashlib` vectors at every block boundary in
/// `test/pda.test.mo`.
module {
    private let K : [Nat32] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    public func digest(data : Blob) : Blob {
        let msg = Blob.toArray(data);
        let len = msg.size();
        let total = ((len + 9 + 63) / 64) * 64;
        let buf = Array.init<Nat8>(total, 0);
        var i : Nat = 0;
        while (i < len) { buf[i] := msg[i]; i += 1 };
        buf[len] := 0x80;
        let bits : Nat64 = Nat64.fromNat(len) * 8;
        var k : Nat = 0;
        while (k < 8) {
            buf[total - 1 - k] := Nat8.fromNat(Nat64.toNat((bits >> (Nat64.fromNat(k) * 8)) & 0xFF));
            k += 1;
        };

        var h0 : Nat32 = 0x6a09e667;
        var h1 : Nat32 = 0xbb67ae85;
        var h2 : Nat32 = 0x3c6ef372;
        var h3 : Nat32 = 0xa54ff53a;
        var h4 : Nat32 = 0x510e527f;
        var h5 : Nat32 = 0x9b05688c;
        var h6 : Nat32 = 0x1f83d9ab;
        var h7 : Nat32 = 0x5be0cd19;
        let w = Array.init<Nat32>(64, 0);

        var block : Nat = 0;
        while (block < total) {
            var t : Nat = 0;
            while (t < 16) {
                let o = block + t * 4;
                w[t] := (Nat32.fromNat(Nat8.toNat(buf[o])) << 24) | (Nat32.fromNat(Nat8.toNat(buf[o + 1])) << 16) | (Nat32.fromNat(Nat8.toNat(buf[o + 2])) << 8) | Nat32.fromNat(Nat8.toNat(buf[o + 3]));
                t += 1;
            };
            while (t < 64) {
                let x = w[t - 15];
                let y = w[t - 2];
                let s0 = (x <>> 7) ^ (x <>> 18) ^ (x >> 3);
                let s1 = (y <>> 17) ^ (y <>> 19) ^ (y >> 10);
                w[t] := w[t - 16] +% s0 +% w[t - 7] +% s1;
                t += 1;
            };
            var a = h0;
            var b = h1;
            var c = h2;
            var d = h3;
            var e = h4;
            var f = h5;
            var g = h6;
            var h = h7;
            t := 0;
            while (t < 64) {
                let bigS1 = (e <>> 6) ^ (e <>> 11) ^ (e <>> 25);
                let ch = (e & f) ^ ((e ^ 0xFFFFFFFF) & g);
                let t1 = h +% bigS1 +% ch +% K[t] +% w[t];
                let bigS0 = (a <>> 2) ^ (a <>> 13) ^ (a <>> 22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = bigS0 +% maj;
                h := g;
                g := f;
                f := e;
                e := d +% t1;
                d := c;
                c := b;
                b := a;
                a := t1 +% t2;
                t += 1;
            };
            h0 +%= a;
            h1 +%= b;
            h2 +%= c;
            h3 +%= d;
            h4 +%= e;
            h5 +%= f;
            h6 +%= g;
            h7 +%= h;
            block += 64;
        };

        let hs : [Nat32] = [h0, h1, h2, h3, h4, h5, h6, h7];
        Blob.fromArray(Array.tabulate<Nat8>(32, func(j : Nat) : Nat8 {
            let word = hs[j / 4];
            Nat8.fromNat(Nat32.toNat((word >> (Nat32.fromNat(3 - (j % 4)) * 8)) & 0xFF));
        }));
    };
};
