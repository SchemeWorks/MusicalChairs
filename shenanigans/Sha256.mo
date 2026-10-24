/**
 * Dependency-free SHA-256 (FIPS 180-4) for the Shenanigans canister.
 *
 * mo:base 0.16 (the package dfx 0.29.2 bundles, used to build this canister)
 * ships no SHA-256. `LedgerMemo` needs a deterministic 32-byte digest to fit
 * long event ids under the PP ledger's `icrc1:max_memo_length = 32`, so this is
 * a small pure implementation: wrapping Nat32 arithmetic, no allocation beyond
 * the padded message copy. Ported from the NIST-vector-tested implementation on
 * the native-Solana branch (NativeRewards.sha256); vectors are re-asserted in
 * shenanigans/test/ledger-memo.test.mo.
 *
 * Not constant-time and not for secrets: it hashes public event ids.
 */

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";

module {
    let K256 : [Nat32] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    /// SHA-256 of `msg` as a 32-byte Blob.
    public func sha256(msg : Blob) : Blob {
        let data = Blob.toArray(msg);
        let len = data.size();
        let padLen = ((len + 9 + 63) / 64) * 64;
        let buf = Array.init<Nat8>(padLen, 0);
        var i = 0;
        while (i < len) { buf[i] := data[i]; i += 1 };
        buf[len] := 0x80;
        let bitLen : Nat64 = Nat64.fromNat(len) * 8;
        var k = 0;
        while (k < 8) {
            let shifted : Nat64 = bitLen >> (Nat64.fromNat(k) * 8);
            buf[padLen - 1 - k] := Nat8.fromNat(Nat64.toNat(shifted & 0xff));
            k += 1;
        };

        let h : [var Nat32] = [var 0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        let w = Array.init<Nat32>(64, 0);
        var blk = 0;
        while (blk < padLen / 64) {
            let base = blk * 64;
            var t = 0;
            while (t < 16) {
                let b0 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t]));
                let b1 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 1]));
                let b2 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 2]));
                let b3 = Nat32.fromNat(Nat8.toNat(buf[base + 4 * t + 3]));
                w[t] := (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                t += 1;
            };
            while (t < 64) {
                let w15 = w[t - 15];
                let w2 = w[t - 2];
                let s0 = (w15 <>> 7) ^ (w15 <>> 18) ^ (w15 >> 3);
                let s1 = (w2 <>> 17) ^ (w2 <>> 19) ^ (w2 >> 10);
                w[t] := w[t - 16] +% s0 +% w[t - 7] +% s1;
                t += 1;
            };
            var a = h[0];
            var b = h[1];
            var c = h[2];
            var d = h[3];
            var e = h[4];
            var f = h[5];
            var g = h[6];
            var hh = h[7];
            t := 0;
            while (t < 64) {
                let bigS1 = (e <>> 6) ^ (e <>> 11) ^ (e <>> 25);
                let ch = (e & f) ^ ((^e) & g);
                let temp1 = hh +% bigS1 +% ch +% K256[t] +% w[t];
                let bigS0 = (a <>> 2) ^ (a <>> 13) ^ (a <>> 22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = bigS0 +% maj;
                hh := g;
                g := f;
                f := e;
                e := d +% temp1;
                d := c;
                c := b;
                b := a;
                a := temp1 +% temp2;
                t += 1;
            };
            h[0] := h[0] +% a;
            h[1] := h[1] +% b;
            h[2] := h[2] +% c;
            h[3] := h[3] +% d;
            h[4] := h[4] +% e;
            h[5] := h[5] +% f;
            h[6] := h[6] +% g;
            h[7] := h[7] +% hh;
            blk += 1;
        };
        let out = Array.init<Nat8>(32, 0);
        var j = 0;
        while (j < 8) {
            let v = h[j];
            out[4 * j] := Nat8.fromNat(Nat32.toNat(v >> 24));
            out[4 * j + 1] := Nat8.fromNat(Nat32.toNat((v >> 16) & 0xff));
            out[4 * j + 2] := Nat8.fromNat(Nat32.toNat((v >> 8) & 0xff));
            out[4 * j + 3] := Nat8.fromNat(Nat32.toNat(v & 0xff));
            j += 1;
        };
        Blob.fromArrayMut(out);
    };
};
