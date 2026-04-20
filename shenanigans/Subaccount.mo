import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Array "mo:base/Array";
import Nat8 "mo:base/Nat8";

module {
    /// Default (all-zero) 32-byte subaccount.
    public func defaultSubaccount() : Blob {
        Blob.fromArray(Array.tabulate<Nat8>(32, func(_) = 0));
    };

    /// Map a player principal to a deterministic 32-byte chip subaccount.
    /// Encoding: principal bytes, left-padded on the right with 0x00 to 32 bytes.
    /// Principal byte representations are ≤29 bytes and globally unique, so
    /// two distinct principals always yield distinct subaccounts.
    public func principalToChipSubaccount(p : Principal) : Blob {
        let bytes = Blob.toArray(Principal.toBlob(p));
        let size = bytes.size();
        // Principals are always <= 29 bytes in practice, but guard defensively.
        assert (size <= 32);
        let padded = Array.tabulate<Nat8>(32, func(i) {
            if (i < size) { bytes[i] } else { 0 : Nat8 }
        });
        Blob.fromArray(padded);
    };
}
