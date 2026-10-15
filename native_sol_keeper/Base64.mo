import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Char "mo:base/Char";
import Iter "mo:base/Iter";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Text "mo:base/Text";

/// Base64, standard alphabet with '=' padding (the `sendTransaction` encoding).
/// Decoding is strict: length must be a multiple of four, padding only at the end.
module {
    private let ALPHABET : [Char] = [
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '+', '/',
    ];

    func sextet(c : Char) : ?Nat {
        let n = Nat32.toNat(Char.toNat32(c));
        if (n >= 65 and n <= 90) { return ?(n - 65) };
        if (n >= 97 and n <= 122) { return ?(n - 97 + 26) };
        if (n >= 48 and n <= 57) { return ?(n - 48 + 52) };
        if (n == 43) { return ?62 };
        if (n == 47) { return ?63 };
        null;
    };

    public func encode(bytes : Blob) : Text {
        let a = Blob.toArray(bytes);
        var out = "";
        var i : Nat = 0;
        while (i < a.size()) {
            let b0 = Nat8.toNat(a[i]);
            let b1 = if (i + 1 < a.size()) { Nat8.toNat(a[i + 1]) } else { 0 };
            let b2 = if (i + 2 < a.size()) { Nat8.toNat(a[i + 2]) } else { 0 };
            let n = b0 * 65_536 + b1 * 256 + b2;
            out #= Text.fromChar(ALPHABET[n / 262_144]);
            out #= Text.fromChar(ALPHABET[(n / 4_096) % 64]);
            out #= if (i + 1 < a.size()) { Text.fromChar(ALPHABET[(n / 64) % 64]) } else { "=" };
            out #= if (i + 2 < a.size()) { Text.fromChar(ALPHABET[n % 64]) } else { "=" };
            i += 3;
        };
        out;
    };

    public func decode(s : Text) : ?Blob {
        let chars = Iter.toArray(Text.toIter(s));
        if (chars.size() % 4 != 0) { return null };
        var out : [Nat8] = [];
        var i : Nat = 0;
        while (i < chars.size()) {
            let last = i + 4 == chars.size();
            var pad : Nat = 0;
            var vals : [Nat] = [];
            var k : Nat = 0;
            while (k < 4) {
                let c = chars[i + k];
                if (c == '=') {
                    if (not last or k < 2) { return null };
                    pad += 1;
                } else {
                    if (pad > 0) { return null };
                    switch (sextet(c)) {
                        case (null) { return null };
                        case (?v) { vals := Array.tabulate<Nat>(vals.size() + 1, func(j : Nat) : Nat { if (j < vals.size()) { vals[j] } else { v } }) };
                    };
                };
                k += 1;
            };
            let n = vals[0] * 262_144 + vals[1] * 4_096 + (if (vals.size() > 2) { vals[2] * 64 } else { 0 }) + (if (vals.size() > 3) { vals[3] } else { 0 });
            let produced = 3 - pad;
            let chunk = Array.tabulate<Nat8>(produced, func(j : Nat) : Nat8 { Nat8.fromNat((n / (if (j == 0) { 65_536 } else if (j == 1) { 256 } else { 1 })) % 256) });
            out := Array.append<Nat8>(out, chunk);
            i += 4;
        };
        ?Blob.fromArray(out);
    };
};
