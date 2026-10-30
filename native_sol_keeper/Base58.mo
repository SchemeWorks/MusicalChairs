import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Char "mo:base/Char";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Text "mo:base/Text";

/// Base58 (Bitcoin / Solana alphabet). Behaviour is pinned against the bridge's
/// copy (`native_sol_bridge/Decode.mo`) and Python vectors in `test/pda.test.mo`.
module {
    private let ALPHABET : [Char] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9',
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    ];

    /// Reverse lookup by ASCII code point, -1 = not in the alphabet.
    private let DECODE_TABLE : [Int] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, -1, -1, -1, -1, -1, -1,
        -1, 9, 10, 11, 12, 13, 14, 15, 16, -1, 17, 18, 19, 20, 21, -1,
        22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, -1, -1, -1, -1, -1,
        -1, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, -1, 44, 45, 46,
        47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, -1, -1, -1, -1, -1,
    ];

    public func encode(bytes : Blob) : Text {
        let arr = Blob.toArray(bytes);
        if (arr.size() == 0) { return "" };
        var leadingZeros : Nat = 0;
        while (leadingZeros < arr.size() and arr[leadingZeros] == (0 : Nat8)) { leadingZeros += 1 };
        var num : Nat = 0;
        for (b in arr.vals()) { num := num * 256 + Nat8.toNat(b) };
        var digits : [Char] = [];
        while (num > 0) {
            let rem = num % 58;
            num := num / 58;
            digits := Array.tabulate<Char>(digits.size() + 1, func(i : Nat) : Char { if (i == 0) { ALPHABET[rem] } else { digits[i - 1] } });
        };
        var out = "";
        var z : Nat = 0;
        while (z < leadingZeros) { out #= "1"; z += 1 };
        for (c in digits.vals()) { out #= Text.fromChar(c) };
        out;
    };

    /// `null` on a character outside the alphabet.
    public func decode(s : Text) : ?Blob {
        if (s.size() == 0) { return ?Blob.fromArray([]) };
        let chars = Iter.toArray(Text.toIter(s));
        var leadingOnes : Nat = 0;
        while (leadingOnes < chars.size() and chars[leadingOnes] == '1') { leadingOnes += 1 };
        var num : Nat = 0;
        var i = leadingOnes;
        while (i < chars.size()) {
            let cp = Nat32.toNat(Char.toNat32(chars[i]));
            if (cp >= 128) { return null };
            let v = DECODE_TABLE[cp];
            if (v < 0) { return null };
            num := num * 58 + Int.abs(v);
            i += 1;
        };
        // base58 -> base256 never needs more bytes than characters.
        let tmp = Array.init<Nat8>(chars.size() + 1, 0);
        var n : Nat = 0;
        while (num > 0) {
            tmp[n] := Nat8.fromNat(num % 256);
            num := num / 256;
            n += 1;
        };
        let out = Array.tabulate<Nat8>(leadingOnes + n, func(k : Nat) : Nat8 { if (k < leadingOnes) { 0 } else { tmp[n - 1 - (k - leadingOnes)] } });
        ?Blob.fromArray(out);
    };

    /// A base58 string that decodes to exactly `n` bytes.
    public func isOfLength(s : Text, n : Nat) : Bool {
        switch (decode(s)) { case (null) { false }; case (?b) { b.size() == n } };
    };
};
