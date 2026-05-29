import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Int "mo:base/Int";
import Int64 "mo:base/Int64";
import Nat8 "mo:base/Nat8";
import Text "mo:base/Text";

/// SolRpc — Motoko actor-type bindings for the DFINITY sol-rpc canister.
///
/// Types and method signatures are derived directly from the live Candid
/// fetched from tghme-zyaaa-aaaar-qarca-cai on 2026-05-27.
///
/// Only the subset of methods used by ponzi_math_sol is declared.
module {
    // ====================================================================
    // Canister ID
    // ====================================================================

    /// The DFINITY-operated sol-rpc canister (same ID for all clusters;
    /// cluster selection is done via the RpcSources argument on every call).
    public let SOL_RPC_CANISTER_ID : Text = "tghme-zyaaa-aaaar-qarca-cai";

    // ====================================================================
    // Cluster / provider / source types
    // ====================================================================

    public type SolanaCluster = {
        #Mainnet;
        #Devnet;
        #Testnet;
    };

    // We declare SupportedProvider only so Multi*Result Inconsistent arms
    // can reference it. We only ever use #Default(SolanaCluster).
    public type SupportedProvider = {
        #AlchemyMainnet;
        #AlchemyDevnet;
        #AnkrMainnet;
        #AnkrDevnet;
        #ChainstackMainnet;
        #ChainstackDevnet;
        #DrpcMainnet;
        #DrpcDevnet;
        #HeliusMainnet;
        #HeliusDevnet;
        #PublicNodeMainnet;
    };

    public type HttpHeader = {
        value : Text;
        name : Text;
    };

    public type RpcEndpoint = {
        url : Text;
        headers : ?[HttpHeader];
    };

    public type RpcSource = {
        #Supported : SupportedProvider;
        #Custom : RpcEndpoint;
    };

    /// The first argument to every sol-rpc method.
    /// Use `#Default(#Devnet)` or `#Default(#Mainnet)`.
    public type RpcSources = {
        #Custom : [RpcSource];
        #Default : SolanaCluster;
    };

    // ====================================================================
    // Our internal provider enum — preserved for upgrade safety.
    // Do NOT change the variant tags; they are stored in stable memory.
    // ====================================================================

    /// Internal stable type. Kept so existing stable `solRpcProvider` field
    /// survives upgrades unchanged. Convert to RpcSources at call sites
    /// using `rpcSources(p)` below.
    public type Provider = {
        #devnet;
        #mainnet;
    };

    /// Convert our internal Provider to the RpcSources value expected by
    /// every sol-rpc method.
    public func rpcSources(p : Provider) : RpcSources {
        switch (p) {
            case (#devnet) { #Default(#Devnet) };
            case (#mainnet) { #Default(#Mainnet) };
        };
    };

    // ====================================================================
    // RpcConfig (optional second arg on every method)
    // ====================================================================

    public type ConsensusStrategy = {
        #Equality;
        #Threshold : { total : ?Nat8; min : Nat8 };
    };

    public type RpcConfig = {
        responseSizeEstimate : ?Nat64;
        responseConsensus : ?ConsensusStrategy;
    };

    // ====================================================================
    // RpcError — exact shape from the candid
    // ====================================================================

    public type RejectionCode = {
        #NoError;
        #CanisterError;
        #SysTransient;
        #DestinationInvalid;
        #Unknown;
        #SysFatal;
        #CanisterReject;
    };

    public type HttpOutcallError = {
        #IcError : { code : RejectionCode; message : Text };
        #InvalidHttpJsonRpcResponse : {
            status : Nat16;
            body : Text;
            parsingError : ?Text;
        };
    };

    public type ProviderError = {
        #TooFewCycles : { expected : Nat; received : Nat };
        #InvalidRpcConfig : Text;
        #UnsupportedCluster : Text;
    };

    public type JsonRpcError = {
        code : Int64;
        message : Text;
    };

    public type RpcError = {
        #JsonRpcError : JsonRpcError;
        #ProviderError : ProviderError;
        #ValidationError : Text;
        #HttpOutcallError : HttpOutcallError;
    };

    // ====================================================================
    // Common scalar aliases
    // ====================================================================

    public type Pubkey = Text;
    public type Signature = Text;
    public type Hash = Text;
    public type Slot = Nat64;
    public type Lamport = Nat64;
    public type Timestamp = Int64;

    public type CommitmentLevel = {
        #processed;
        #confirmed;
        #finalized;
    };

    // ====================================================================
    // getBalance
    // ====================================================================

    public type GetBalanceParams = {
        pubkey : Pubkey;
        commitment : ?CommitmentLevel;
        minContextSlot : ?Slot;
    };

    public type GetBalanceResult = {
        #Ok : Lamport;
        #Err : RpcError;
    };

    public type MultiGetBalanceResult = {
        #Consistent : GetBalanceResult;
        #Inconsistent : [(RpcSource, GetBalanceResult)];
    };

    // ====================================================================
    // getAccountInfo
    // ====================================================================

    public type GetAccountInfoEncoding = {
        #base58;
        #base64;
        #base64_zstd;    // "base64+zstd" in candid
        #jsonParsed;
    };

    public type DataSlice = {
        length : Nat32;
        offset : Nat32;
    };

    public type GetAccountInfoParams = {
        pubkey : Pubkey;
        commitment : ?CommitmentLevel;
        encoding : ?GetAccountInfoEncoding;
        dataSlice : ?DataSlice;
        minContextSlot : ?Slot;
    };

    // AccountData variant — candid has "base64+zstd" as a quoted identifier.
    // We match the tag names the sol-rpc canister sends.
    public type AccountEncoding = {
        #binary;
        #base58;
        #base64;
        #base64_zstd;   // "base64+zstd"
        #jsonParsed;
    };

    public type ParsedAccount = {
        program : Pubkey;
        parsed : Text;
        space : Nat64;
    };

    public type AccountData = {
        #legacyBinary : Text;
        #json : ParsedAccount;
        #binary : (Text, AccountEncoding);
    };

    public type AccountInfo = {
        lamports : Nat64;
        data : AccountData;
        owner : Pubkey;
        executable : Bool;
        rentEpoch : Nat64;
        space : Nat64;
    };

    public type GetAccountInfoResult = {
        #Ok : ?AccountInfo;
        #Err : RpcError;
    };

    public type MultiGetAccountInfoResult = {
        #Consistent : GetAccountInfoResult;
        #Inconsistent : [(RpcSource, GetAccountInfoResult)];
    };

    // ====================================================================
    // getSignaturesForAddress
    // ====================================================================

    public type TransactionConfirmationStatus = {
        #processed;
        #confirmed;
        #finalized;
    };

    // The real type from candid (was ConfirmedTransactionStatusWithSignature)
    public type ConfirmedTransactionStatusWithSignature = {
        signature : Signature;
        slot : Slot;
        err : ?Text;          // opt TransactionError, but we only check null/non-null
        memo : ?Text;
        blockTime : ?Timestamp;
        confirmationStatus : ?TransactionConfirmationStatus;
    };

    public type GetSignaturesForAddressParams = {
        pubkey : Pubkey;
        commitment : ?CommitmentLevel;
        minContextSlot : ?Slot;
        limit : ?Nat32;
        before : ?Signature;
        until : ?Signature;
    };

    public type GetSignaturesForAddressResult = {
        #Ok : [ConfirmedTransactionStatusWithSignature];
        #Err : RpcError;
    };

    public type MultiGetSignaturesForAddressResult = {
        #Consistent : GetSignaturesForAddressResult;
        #Inconsistent : [(RpcSource, GetSignaturesForAddressResult)];
    };

    // ====================================================================
    // getSignatureStatuses
    // ====================================================================

    public type TransactionStatus = {
        slot : Slot;
        // status field omitted — we don't use it directly
        err : ?Text;        // opt TransactionError, checked for null only
        confirmationStatus : ?TransactionConfirmationStatus;
    };

    public type GetSignatureStatusesParams = {
        signatures : [Signature];
        searchTransactionHistory : ?Bool;
    };

    public type GetSignatureStatusesResult = {
        #Ok : [?TransactionStatus];
        #Err : RpcError;
    };

    public type MultiGetSignatureStatusesResult = {
        #Consistent : GetSignatureStatusesResult;
        #Inconsistent : [(RpcSource, GetSignatureStatusesResult)];
    };

    // ====================================================================
    // getTransaction
    // ====================================================================

    // The candid uses a deeply-nested type. We need only a slice of it
    // for deposit detection: the transaction's account keys and pre/post
    // balances. We use a structurally-typed approach: the actor type
    // declares the return as the full Motoko type, but we only access
    // the fields we actually use.

    public type CompiledInstruction = {
        data : Text;
        accounts : Blob;
        programIdIndex : Nat8;
        stackHeight : ?Nat32;
    };

    public type EncodedTransaction = {
        #binary : (Text, { #base58; #base64 });
        #legacyBinary : Text;
    };

    public type InnerInstructions = {
        instructions : [{ #compiled : CompiledInstruction }];
        index : Nat8;
    };

    // We declare LoadedAddresses minimally.
    public type LoadedAddresses = { writable : [Pubkey]; readonly : [Pubkey] };

    public type TokenAmount = {
        decimals : Nat8;
        uiAmount : ?Float;
        uiAmountString : Text;
        amount : Text;
    };

    public type TransactionTokenBalance = {
        owner : ?Pubkey;
        mint : Text;
        programId : ?Pubkey;
        accountIndex : Nat8;
        uiTokenAmount : TokenAmount;
    };

    // Reward type — minimally declared
    public type Reward = {
        lamports : Int64;
        commission : ?Nat8;
        pubkey : Pubkey;
        rewardType : ?{ #fee; #rent; #voting; #staking };
        postBalance : Nat64;
    };

    // TransactionStatusMeta — the fields we actually use.
    // Candid has many more fields; Motoko structural subtyping requires we
    // declare only what we access. BUT: for the actor type we must declare
    // the EXACT return type (the remote end serializes it fully). We use
    // a minimal structural representation — since Motoko is structurally
    // typed, the runtime will just ignore fields we don't declare as long
    // as the Candid-level decoding is permissive. In practice, inter-
    // canister calls use Candid subtyping rules, which allow the receiver
    // to declare only a subset of record fields.
    public type TransactionStatusMeta = {
        fee : Nat64;
        // status omitted — we check err directly
        innerInstructions : ?[InnerInstructions];
        postTokenBalances : ?[TransactionTokenBalance];
        preBalances : [Nat64];
        postBalances : [Nat64];
        returnData : ?{ data : Text; programId : Pubkey };
        logMessages : ?[Text];
        rewards : ?[Reward];
        loadedAddresses : ?LoadedAddresses;
        preTokenBalances : ?[TransactionTokenBalance];
        computeUnitsConsumed : ?Nat64;
        cost_units : ?Nat64;
        // err field: for structural purposes we check if the tx errored via
        // a separate field. In the live candid this is `status: variant { Ok; Err: TransactionError }`.
        // We skip it — callers should verify via checking signatures or blockTime.
    };

    public type EncodedTransactionWithStatusMeta = {
        meta : ?TransactionStatusMeta;
        transaction : EncodedTransaction;
        version : ?{ #legacy; #number : Nat8 };
    };

    public type EncodedConfirmedTransactionWithStatusMeta = {
        slot : Slot;
        blockTime : ?Timestamp;
        transaction : EncodedTransactionWithStatusMeta;
    };

    public type GetTransactionParams = {
        signature : Signature;
        commitment : ?CommitmentLevel;
        maxSupportedTransactionVersion : ?Nat8;
        encoding : ?{ #base58; #base64 };
    };

    public type GetTransactionResult = {
        #Ok : ?EncodedConfirmedTransactionWithStatusMeta;
        #Err : RpcError;
    };

    public type MultiGetTransactionResult = {
        #Consistent : GetTransactionResult;
        #Inconsistent : [(RpcSource, GetTransactionResult)];
    };

    // ====================================================================
    // sendTransaction
    // ====================================================================

    public type SendTransactionEncoding = {
        #base58;
        #base64;
    };

    public type SendTransactionParams = {
        transaction : Text;      // base64-encoded signed transaction bytes
        encoding : ?SendTransactionEncoding;
        skipPreflight : ?Bool;
        preflightCommitment : ?CommitmentLevel;
        maxRetries : ?Nat32;     // nat32 in candid (was nat64 in old SolRpc.mo)
        minContextSlot : ?Slot;
    };

    public type SendTransactionResult = {
        #Ok : Signature;
        #Err : RpcError;
    };

    public type MultiSendTransactionResult = {
        #Consistent : SendTransactionResult;
        #Inconsistent : [(RpcSource, SendTransactionResult)];
    };

    // ====================================================================
    // jsonRequest (raw passthrough — used for getLatestBlockhash)
    // ====================================================================

    public type RequestResult = {
        #Ok : Text;
        #Err : RpcError;
    };

    public type MultiRequestResult = {
        #Consistent : RequestResult;
        #Inconsistent : [(RpcSource, RequestResult)];
    };

    // ====================================================================
    // Actor type — only the methods we call
    // ====================================================================

    public type RpcActor = actor {
        getBalance : shared (RpcSources, ?RpcConfig, GetBalanceParams) -> async MultiGetBalanceResult;
        getAccountInfo : shared (RpcSources, ?RpcConfig, GetAccountInfoParams) -> async MultiGetAccountInfoResult;
        getSignaturesForAddress : shared (RpcSources, ?RpcConfig, GetSignaturesForAddressParams) -> async MultiGetSignaturesForAddressResult;
        getSignatureStatuses : shared (RpcSources, ?RpcConfig, GetSignatureStatusesParams) -> async MultiGetSignatureStatusesResult;
        getTransaction : shared (RpcSources, ?RpcConfig, GetTransactionParams) -> async MultiGetTransactionResult;
        sendTransaction : shared (RpcSources, ?RpcConfig, SendTransactionParams) -> async MultiSendTransactionResult;
        jsonRequest : shared (RpcSources, ?RpcConfig, Text) -> async MultiRequestResult;
    };

    // ====================================================================
    // Consensus-extraction helpers
    // ====================================================================

    /// Format an RpcError as a human-readable string for #Err returns.
    public func rpcErrorText(e : RpcError) : Text {
        switch (e) {
            case (#JsonRpcError({ code; message })) {
                "JsonRpcError(" # Int64.toText(code) # "): " # message
            };
            case (#ProviderError(pe)) {
                switch (pe) {
                    case (#TooFewCycles({ expected; received })) {
                        "TooFewCycles: expected=" # debug_show(expected) # " received=" # debug_show(received)
                    };
                    case (#InvalidRpcConfig(m)) { "InvalidRpcConfig: " # m };
                    case (#UnsupportedCluster(m)) { "UnsupportedCluster: " # m };
                };
            };
            case (#ValidationError(m)) { "ValidationError: " # m };
            case (#HttpOutcallError(he)) {
                switch (he) {
                    case (#IcError({ message; code = _ })) { "IcError: " # message };
                    case (#InvalidHttpJsonRpcResponse({ body; status; parsingError = _ })) {
                        "InvalidHttpJsonRpcResponse(status=" # debug_show(status) # "): " # body
                    };
                };
            };
        };
    };

    /// Extract the consistent balance, or return an Err text.
    public func unwrapMultiBalance(r : MultiGetBalanceResult) : { #Ok : Lamport; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(_)) {
                #Err("Providers returned inconsistent balance results");
            };
        };
    };

    /// Extract the consistent account info, or return an Err text.
    public func unwrapMultiAccountInfo(r : MultiGetAccountInfoResult) : { #Ok : ?AccountInfo; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(_)) {
                #Err("Providers returned inconsistent account info results");
            };
        };
    };

    /// Extract the consistent signatures list, or return an Err text.
    public func unwrapMultiSignatures(r : MultiGetSignaturesForAddressResult) : { #Ok : [ConfirmedTransactionStatusWithSignature]; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(_)) {
                #Err("Providers returned inconsistent signatures results");
            };
        };
    };

    /// Extract the consistent transaction, or return an Err text.
    public func unwrapMultiTransaction(r : MultiGetTransactionResult) : { #Ok : ?EncodedConfirmedTransactionWithStatusMeta; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(_)) {
                #Err("Providers returned inconsistent transaction results");
            };
        };
    };

    /// Extract the send-transaction signature, or return an Err text.
    ///
    /// Unlike the read methods, sendTransaction divergence across providers is
    /// NORMAL and must not be treated as failure: the signature is deterministic
    /// from the signed tx bytes, so any provider that returns #Ok carries THE
    /// signature and the broadcast did reach the network. The others commonly
    /// return transient errors ("already processed", rate-limit, timeout). If we
    /// hard-failed on #Inconsistent we would misreport a successful send AND
    /// leave the durable nonce desynced (the tx consumed the nonce on-chain).
    /// So: accept the first provider that returns a signature; only fail when
    /// EVERY provider errored.
    public func unwrapMultiSend(r : MultiSendTransactionResult) : { #Ok : Signature; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(results)) {
                var firstOk : ?Signature = null;
                var errs : Text = "";
                for ((_, res) in results.vals()) {
                    switch (res) {
                        case (#Ok(sig)) {
                            switch (firstOk) { case (null) { firstOk := ?sig }; case (_) {} };
                        };
                        case (#Err(e)) { errs := errs # " | " # rpcErrorText(e) };
                    };
                };
                switch (firstOk) {
                    case (?sig) { #Ok(sig) };
                    case (null) { #Err("all providers failed to broadcast:" # errs) };
                };
            };
        };
    };

    /// Extract the consistent raw JSON result, or return an Err text.
    public func unwrapMultiRequest(r : MultiRequestResult) : { #Ok : Text; #Err : Text } {
        switch (r) {
            case (#Consistent(inner)) {
                switch (inner) {
                    case (#Ok(v)) { #Ok(v) };
                    case (#Err(e)) { #Err(rpcErrorText(e)) };
                };
            };
            case (#Inconsistent(_)) {
                #Err("Providers returned inconsistent JSON request results");
            };
        };
    };

    // ====================================================================
    // Base64 encoding — needed to convert transaction Blob → Text
    // ====================================================================

    let BASE64_CHARS : [Char] = [
        'A','B','C','D','E','F','G','H','I','J','K','L','M',
        'N','O','P','Q','R','S','T','U','V','W','X','Y','Z',
        'a','b','c','d','e','f','g','h','i','j','k','l','m',
        'n','o','p','q','r','s','t','u','v','w','x','y','z',
        '0','1','2','3','4','5','6','7','8','9','+','/'
    ];

    /// Encode a Blob as standard base64 (with padding).
    public func base64Encode(data : Blob) : Text {
        let bytes = Blob.toArray(data);
        let n = bytes.size();
        var result : Text = "";
        var i : Nat = 0;
        while (i + 2 < n) {
            let b0 = Nat8.toNat(bytes[i]);
            let b1 = Nat8.toNat(bytes[i + 1]);
            let b2 = Nat8.toNat(bytes[i + 2]);
            result := result # Text.fromChar(BASE64_CHARS[b0 / 4]);
            result := result # Text.fromChar(BASE64_CHARS[(b0 % 4) * 16 + b1 / 16]);
            result := result # Text.fromChar(BASE64_CHARS[(b1 % 16) * 4 + b2 / 64]);
            result := result # Text.fromChar(BASE64_CHARS[b2 % 64]);
            i += 3;
        };
        if (i + 1 == n) {
            let b0 = Nat8.toNat(bytes[i]);
            result := result # Text.fromChar(BASE64_CHARS[b0 / 4]);
            result := result # Text.fromChar(BASE64_CHARS[(b0 % 4) * 16]);
            result := result # "==";
        } else if (i + 2 == n) {
            let b0 = Nat8.toNat(bytes[i]);
            let b1 = Nat8.toNat(bytes[i + 1]);
            result := result # Text.fromChar(BASE64_CHARS[b0 / 4]);
            result := result # Text.fromChar(BASE64_CHARS[(b0 % 4) * 16 + b1 / 16]);
            result := result # Text.fromChar(BASE64_CHARS[(b1 % 16) * 4]);
            result := result # "=";
        };
        result;
    };

    /// Tiny helper: extract `"blockhash":"<value>"` from a
    /// getLatestBlockhash JSON response string.
    /// Returns null if the pattern is not found.
    public func parseBlockhashFromJson(json : Text) : ?Text {
        // Look for the literal prefix. The response looks like:
        // {"jsonrpc":"2.0","result":{"context":{...},"value":{"blockhash":"<HASH>","lastValidBlockHeight":...}}}
        let needle = "\"blockhash\":\"";
        let needleSize = Text.size(needle);
        let chars = Text.toIter(json);
        // var _window : Text = ""; // (unused; left as documentation)
        var found = false;
        var result : Text = "";

        // Slide a window of length needleSize across the string.
        // If we match, collect chars until the next double-quote.
        var buf : [var Char] = Array.init<Char>(needleSize, ' ');
        var filled : Nat = 0;
        var matched = false;

        for (c in chars) {
            if (matched) {
                if (Text.fromChar(c) == "\"") {
                    // Done — result holds the blockhash value.
                    found := true;
                } else {
                    result := result # Text.fromChar(c);
                };
            } else {
                // Shift window left, append c.
                var i : Nat = 0;
                while (i + 1 < needleSize) {
                    buf[i] := buf[i + 1];
                    i += 1;
                };
                if (needleSize > 0) {
                    buf[needleSize - 1] := c;
                    if (filled < needleSize) { filled += 1 };
                };
                if (filled == needleSize) {
                    // Check if window == needle.
                    var windowText : Text = "";
                    for (ch in buf.vals()) { windowText := windowText # Text.fromChar(ch) };
                    if (windowText == needle) {
                        matched := true;
                    };
                };
            };
        };

        if (found) { ?result } else { null };
    };
}
