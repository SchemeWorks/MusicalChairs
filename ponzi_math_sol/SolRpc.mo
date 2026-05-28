import Blob "mo:base/Blob";
import Nat64 "mo:base/Nat64";
import Principal "mo:base/Principal";

module {
    // DFINITY-operated sol-rpc canister (mainnet).
    // Same canister ID is used whether we're talking to Solana devnet or
    // mainnet — the difference is conveyed via the `RpcConfig` passed on
    // every call (see `commitmentLevel` / `responseConsensus` / network
    // selection per the sol-rpc candid).
    public let SOL_RPC_CANISTER_ID : Text = "tghme-zyaaa-aaaar-qarca-cai";

    // ====================================================================
    // Common types — these mirror the sol-rpc candid shapes we depend on.
    // The full candid covers many more variants; we declare only what
    // ponzi_math_sol actually invokes. If a method we add later needs a
    // type that isn't here, extend this module in the same task.
    // ====================================================================

    public type Provider = {
        #devnet;
        #mainnet;
    };

    public type RpcConfig = {
        provider : ?Provider;
        // Other fields (responseConsensus, requestCostMultiplier, etc.) are
        // omitted; defaults are fine for M1.
    };

    public type RpcError = {
        #ProviderError : Text;
        #HttpOutcallError : Text;
        #JsonRpcError : { code : Int; message : Text };
        #ConsensusError : Text;
        #ValidationError : Text;
    };

    public type RpcResult<T> = { #Ok : T; #Err : RpcError };

    // getBalance returns lamports as Nat64.
    public type Lamports = Nat64;

    // ConfirmedSignature returned by getSignaturesForAddress.
    public type ConfirmedSignature = {
        signature : Text; // base58
        slot : Nat64;
        err : ?Text;
        memo : ?Text;
        blockTime : ?Int;
        confirmationStatus : ?Text; // "processed" | "confirmed" | "finalized"
    };

    public type GetSignaturesConfig = {
        limit : ?Nat;          // max 1000; default 1000
        before : ?Text;        // start before this signature
        until : ?Text;         // stop at this signature
        commitment : ?Text;    // "processed" | "confirmed" | "finalized"
    };

    public type GetTransactionConfig = {
        commitment : ?Text;
        maxSupportedTransactionVersion : ?Nat64; // 0 covers the common cases
        encoding : ?Text; // "json" | "jsonParsed" | "base64"
    };

    // Simplified Transaction shape: we only need the postTokenBalances /
    // accountKeys / lamports deltas to detect inbound transfers.
    public type ParsedTransaction = {
        slot : Nat64;
        blockTime : ?Int;
        meta : ?TransactionMeta;
        transaction : ?TransactionDetail;
    };

    public type TransactionMeta = {
        err : ?Text;
        fee : Nat64;
        preBalances : [Nat64];
        postBalances : [Nat64];
    };

    public type TransactionDetail = {
        message : ?TransactionMessage;
        signatures : [Text];
    };

    public type TransactionMessage = {
        accountKeys : [Text]; // base58 pubkeys in canonical account order
        recentBlockhash : Text;
    };

    public type GetAccountInfoConfig = {
        commitment : ?Text;
        encoding : ?Text;
    };

    public type AccountInfo = {
        lamports : Nat64;
        owner : Text;
        executable : Bool;
        rentEpoch : Nat64;
        data : Blob;
    };

    public type SendTransactionConfig = {
        skipPreflight : ?Bool;
        preflightCommitment : ?Text;
        maxRetries : ?Nat64;
        encoding : ?Text; // "base58" | "base64"
    };

    public type SignatureStatus = {
        slot : Nat64;
        confirmations : ?Nat64;
        err : ?Text;
        confirmationStatus : ?Text;
    };

    // ====================================================================
    // Actor type
    // ====================================================================

    public type RpcActor = actor {
        getBalance : shared (Text, ?RpcConfig) -> async RpcResult<Lamports>;
        getSignaturesForAddress : shared (Text, ?GetSignaturesConfig, ?RpcConfig) -> async RpcResult<[ConfirmedSignature]>;
        getTransaction : shared (Text, ?GetTransactionConfig, ?RpcConfig) -> async RpcResult<?ParsedTransaction>;
        getAccountInfo : shared (Text, ?GetAccountInfoConfig, ?RpcConfig) -> async RpcResult<?AccountInfo>;
        sendTransaction : shared (Blob, ?SendTransactionConfig, ?RpcConfig) -> async RpcResult<Text>;
        getSignatureStatuses : shared ([Text], ?RpcConfig) -> async RpcResult<[?SignatureStatus]>;
        getLatestBlockhash : shared (?RpcConfig) -> async RpcResult<{ blockhash : Text; lastValidBlockHeight : Nat64 }>;
    };
};
