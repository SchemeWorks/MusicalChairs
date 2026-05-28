import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Char "mo:base/Char";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Text "mo:base/Text";

module {
    // Solana / Bitcoin base58 alphabet (no 0, O, I, l).
    private let ALPHABET : [Char] = [
        '1','2','3','4','5','6','7','8','9',
        'A','B','C','D','E','F','G','H','J','K','L','M','N','P','Q','R','S','T','U','V','W','X','Y','Z',
        'a','b','c','d','e','f','g','h','i','j','k','m','n','o','p','q','r','s','t','u','v','w','x','y','z'
    ];

    // Reverse lookup table indexed by ASCII codepoint. -1 means invalid char.
    // Hardcoded literal (Motoko modules disallow function calls in module-level
    // let bindings). Generated from the ALPHABET above: position = alphabet index,
    // -1 = character not in alphabet.
    private let DECODE_TABLE : [Int] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8,-1,-1,-1,-1,-1,-1,
        -1, 9,10,11,12,13,14,15,16,-1,17,18,19,20,21,-1,
        22,23,24,25,26,27,28,29,30,31,32,-1,-1,-1,-1,-1,
        -1,33,34,35,36,37,38,39,40,41,42,43,-1,44,45,46,
        47,48,49,50,51,52,53,54,55,56,57,-1,-1,-1,-1,-1
    ];

    /// Encode raw bytes as a base58 string.
    public func encode(bytes : Blob) : Text {
        let arr = Blob.toArray(bytes);
        if (arr.size() == 0) { return "" };

        // Count leading zero bytes — each becomes a leading '1'.
        var leadingZeros : Nat = 0;
        var idx : Nat = 0;
        while (idx < arr.size() and arr[idx] == (0 : Nat8)) {
            leadingZeros += 1;
            idx += 1;
        };

        // Convert the input bytes to a single big-int via base-256 accumulation.
        var num : Nat = 0;
        for (b in arr.vals()) {
            num := num * 256 + Nat8.toNat(b);
        };

        // Divmod 58 down to zero, building the digits in reverse.
        var digits : [var Char] = [var];
        if (num == 0 and leadingZeros == 0) {
            // Empty input was handled above; pure all-zero input produces only
            // leading '1's via the loop below.
        };
        var reverseChars = "";
        while (num > 0) {
            let rem = num % 58;
            num := num / 58;
            reverseChars := Text.fromChar(ALPHABET[rem]) # reverseChars;
        };

        // Prepend one '1' for each leading zero byte.
        var prefix = "";
        var i : Nat = 0;
        while (i < leadingZeros) {
            prefix := prefix # "1";
            i += 1;
        };
        prefix # reverseChars;
    };

    /// Decode a base58 string back to raw bytes.
    /// Returns null on invalid characters.
    public func decode(s : Text) : ?Blob {
        if (s.size() == 0) { return ?Blob.fromArray([]) };

        // Count leading '1's — each becomes a leading zero byte.
        var leadingOnes : Nat = 0;
        let chars = Iter.toArray(Text.toIter(s));
        var i : Nat = 0;
        while (i < chars.size() and chars[i] == '1') {
            leadingOnes += 1;
            i += 1;
        };

        // Accumulate the remaining digits into a big int.
        var num : Nat = 0;
        while (i < chars.size()) {
            let cp = Nat32.toNat(Char.toNat32(chars[i]));
            if (cp >= 128) { return null };
            let v = DECODE_TABLE[cp];
            if (v < 0) { return null };
            num := num * 58 + Int.abs(v);
            i += 1;
        };

        // Convert the big int back to base-256 bytes (most-significant first).
        var revBytes : [var Nat8] = [var];
        var buf = num;
        var byteList : [Nat8] = [];
        if (buf == 0) {
            byteList := [];
        } else {
            // Use a growing array via Buffer; rebuild via Array.
            let tmp = Array.init<Nat8>(64, 0); // 32 input bytes max → 64 output is safe
            var bi : Nat = 0;
            while (buf > 0) {
                tmp[bi] := Nat8.fromNat(buf % 256);
                buf := buf / 256;
                bi += 1;
            };
            // Reverse into the final array.
            let final = Array.init<Nat8>(bi, 0);
            var k : Nat = 0;
            while (k < bi) {
                final[k] := tmp[bi - 1 - k];
                k += 1;
            };
            byteList := Array.freeze(final);
        };

        // Prepend the leading-zero bytes.
        let total = leadingOnes + byteList.size();
        let out = Array.init<Nat8>(total, 0);
        var oi : Nat = leadingOnes;
        for (b in byteList.vals()) {
            out[oi] := b;
            oi += 1;
        };
        ?Blob.fromArray(Array.freeze(out));
    };

    /// Convenience: known well-formed Solana pubkey length check.
    /// Solana pubkeys are 32 bytes → base58 length 32-44 chars.
    public func isPlausibleSolanaAddress(s : Text) : Bool {
        if (s.size() < 32 or s.size() > 44) { return false };
        switch (decode(s)) {
            case (null) { false };
            case (?b) { Blob.toArray(b).size() == 32 };
        };
    };
};
