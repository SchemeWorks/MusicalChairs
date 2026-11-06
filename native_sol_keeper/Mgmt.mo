/// Mgmt - the two management-canister (`aaaaa-aa`) methods the keeper uses for
/// its fee-only Ed25519 address: `schnorr_public_key` and `sign_with_schnorr`.
///
/// SOURCE: the management canister interface embedded in dfx 0.29.2
/// (`~/.cache/dfinity/versions/0.29.2/dfx`, section `ic.did`; extracted with
/// `strings`), which mirrors the IC interface specification:
///
///   type schnorr_algorithm = variant { bip340secp256k1; ed25519 };
///   type schnorr_public_key_args = record {
///     canister_id : opt canister_id; derivation_path : vec blob;
///     key_id : record { algorithm : schnorr_algorithm; name : text } };
///   type schnorr_public_key_result = record { public_key : blob; chain_code : blob };
///   type schnorr_aux = variant { bip341 : record { merkle_root_hash : blob } };
///   type sign_with_schnorr_args = record {
///     message : blob; derivation_path : vec blob;
///     key_id : record { algorithm : schnorr_algorithm; name : text };
///     aux : opt schnorr_aux };
///   type sign_with_schnorr_result = record { signature : blob };
///   service ic : {
///     schnorr_public_key : (schnorr_public_key_args) -> (schnorr_public_key_result);
///     sign_with_schnorr : (sign_with_schnorr_args) -> (sign_with_schnorr_result);
///   }
///
/// `sign_with_schnorr` must be called with cycles attached (production `key_1`
/// charges 26_153_846_153 cycles per signature, `docs/coordination/native-solana-icp-budget.md`);
/// the caller attaches `Config.signCycles` and the unused part is refunded.
/// For `ed25519` the message is signed as raw bytes (a Solana message), `aux`
/// stays null, and `public_key` is the 32-byte compressed Edwards point, i.e. the
/// Solana address bytes.
module {
    public type SchnorrAlgorithm = { #bip340secp256k1; #ed25519 };
    public type KeyId = { algorithm : SchnorrAlgorithm; name : Text };
    public type SchnorrAux = { #bip341 : { merkle_root_hash : Blob } };

    public type PublicKeyArgs = {
        canister_id : ?Principal;
        derivation_path : [Blob];
        key_id : KeyId;
    };
    public type PublicKeyResult = { public_key : Blob; chain_code : Blob };

    public type SignArgs = {
        message : Blob;
        derivation_path : [Blob];
        key_id : KeyId;
        aux : ?SchnorrAux;
    };
    public type SignResult = { signature : Blob };

    public type Management = actor {
        schnorr_public_key : shared PublicKeyArgs -> async PublicKeyResult;
        sign_with_schnorr : shared SignArgs -> async SignResult;
    };

    public let MANAGEMENT_CANISTER : Text = "aaaaa-aa";
};
