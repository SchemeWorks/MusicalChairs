import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Cycles "mo:base/ExperimentalCycles";

import Base58 "Base58";

module {

    // ====================================================================
    // Management-canister actor — Schnorr (Ed25519) signing.
    // ====================================================================

    public type SchnorrAlgorithm = { #ed25519; #bip340secp256k1 };

    public type KeyId = {
        algorithm : SchnorrAlgorithm;
        name : Text;
    };

    public type PublicKeyArgs = {
        canister_id : ?Principal;
        derivation_path : [Blob];
        key_id : KeyId;
    };

    public type PublicKeyResult = {
        public_key : Blob;
        chain_code : Blob;
    };

    public type SignArgs = {
        message : Blob;
        derivation_path : [Blob];
        key_id : KeyId;
    };

    public type SignResult = {
        signature : Blob;
    };

    let ic : actor {
        schnorr_public_key : shared PublicKeyArgs -> async PublicKeyResult;
        sign_with_schnorr : shared SignArgs -> async SignResult;
    } = actor "aaaaa-aa";

    // ====================================================================
    // Well-known key ids.
    // Mainnet uses "key_1" (production threshold key). Local dev uses
    // "dfx_test_key" — only works against a local replica configured with
    // the test key, and we don't actually exercise local signing in M1.
    // ====================================================================

    public let KEY_ID_MAINNET : KeyId = { algorithm = #ed25519; name = "key_1" };
    public let KEY_ID_LOCAL : KeyId = { algorithm = #ed25519; name = "dfx_test_key" };

    /// Approximate cycle cost of one sign_with_schnorr call on mainnet
    /// (Q1 2026 pricing ≈ 26 G cycles). 30 G is a defensive buffer.
    public let SIGN_CYCLES : Nat = 30_000_000_000;

    // ====================================================================
    // Public helpers — pubkey derivation and signing.
    // ====================================================================

    /// Derive a Solana address by base58-encoding the threshold Ed25519
    /// pubkey for the given derivation path. canister_id = null means
    /// "use the calling canister's id" (this canister).
    public func deriveAddress(keyId : KeyId, derivationPath : [Blob]) : async Text {
        let res = await ic.schnorr_public_key({
            canister_id = null;
            derivation_path = derivationPath;
            key_id = keyId;
        });
        // Ed25519 pubkey is 32 bytes; Solana addresses are exactly those
        // 32 bytes base58-encoded.
        Base58.encode(res.public_key);
    };

    /// Sign a raw message blob with the threshold key at the given
    /// derivation path. Returns the 64-byte signature.
    public func sign(keyId : KeyId, derivationPath : [Blob], message : Blob) : async Blob {
        Cycles.add(SIGN_CYCLES);
        let res = await ic.sign_with_schnorr({
            message;
            derivation_path = derivationPath;
            key_id = keyId;
        });
        res.signature;
    };

    /// Convenience: sign one message with multiple derivation paths
    /// (e.g., pool + nonce-account for bootstrap). Returns signatures in
    /// the same order as `derivationPaths`.
    public func signMulti(keyId : KeyId, derivationPaths : [[Blob]], message : Blob) : async [Blob] {
        let sigs = Buffer.Buffer<Blob>(derivationPaths.size());
        for (path in derivationPaths.vals()) {
            let sig = await sign(keyId, path, message);
            sigs.add(sig);
        };
        Buffer.toArray(sigs);
    };
};
