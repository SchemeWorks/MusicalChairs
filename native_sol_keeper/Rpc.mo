import Base64 "Base64";
import Int64 "mo:base/Int64";

/// Rpc - Motoko actor-type bindings for the DFINITY SOL RPC canister, restricted
/// to the six methods the watchdog calls: `getAccountInfo`, `getBalance`,
/// `getSignatureStatuses`, `getSlot`, `getBlock`, `sendTransaction`.
///
/// PINNED to `native_sol_bridge/candid/sol_rpc_canister.did` (mainnet
/// `tghme-zyaaa-aaaar-qarca-cai`, module 0x1733b1e5...3115, captured 2026-10-13).
/// The shared types (sources, errors, scalars, `TransactionError`) are the same
/// text as `native_sol_bridge/Rpc.mo`; the per-method types below are copied from
/// the candid with EXACT field names, optionality and integer widths, because
/// the reply is decoded by the Candid runtime and a width mismatch (`nat16`
/// declared as `Nat`) or a variant tag the reply carries but the declared variant
/// lacks traps AFTER the call - after a `sendTransaction` that would be after the
/// transaction was broadcast, which is why the journal entry is written BEFORE
/// the send and a decode failure is an #Unknown outcome, never a retry.
/// `ConfirmedBlock` declares only the five fields the watchdog reads (record
/// width subtyping lets the reply carry more). Verified with
/// `didc check <pinned.did> <moc --idl of a probe actor>`; see the handoff report.
///
/// Consensus: every method answers with a `Multi*Result`, either `Consistent`
/// (all contacted providers agreed) or `Inconsistent` (per-provider results).
/// The watchdog acts only on `Consistent(Ok(..))`.
module {
    public let SOL_RPC_CANISTER_ID : Text = "tghme-zyaaa-aaaar-qarca-cai";

    // ------------------------------------------------------------------
    // Sources / config
    // ------------------------------------------------------------------

    public type SolanaCluster = { #Mainnet; #Devnet; #Testnet };

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

    public type HttpHeader = { value : Text; name : Text };

    public type RpcEndpoint = { url : Text; headers : ?[HttpHeader] };

    public type RpcSource = { #Supported : SupportedProvider; #Custom : RpcEndpoint };

    public type RpcSources = { #Custom : [RpcSource]; #Default : SolanaCluster };

    public type ConsensusStrategy = {
        #Equality;
        #Threshold : { total : ?Nat8; min : Nat8 };
    };

    public type RpcConfig = {
        responseSizeEstimate : ?Nat64;
        responseConsensus : ?ConsensusStrategy;
    };

    // ------------------------------------------------------------------
    // Errors
    // ------------------------------------------------------------------

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

    public type JsonRpcError = { code : Int64; message : Text };

    public type RpcError = {
        #JsonRpcError : JsonRpcError;
        #ProviderError : ProviderError;
        #ValidationError : Text;
        #HttpOutcallError : HttpOutcallError;
    };

    public let JSON_RPC_PREFLIGHT_FAILED : Int64 = -32002;

    public func errorText(e : RpcError) : Text {
        switch (e) {
            case (#JsonRpcError({ code; message })) { "JsonRpcError(" # Int64.toText(code) # "): " # message };
            case (#ProviderError(#TooFewCycles({ expected; received }))) {
                "ProviderError(TooFewCycles): expected=" # debug_show (expected) # " received=" # debug_show (received);
            };
            case (#ProviderError(#InvalidRpcConfig(m))) { "ProviderError(InvalidRpcConfig): " # m };
            case (#ProviderError(#UnsupportedCluster(m))) { "ProviderError(UnsupportedCluster): " # m };
            case (#ValidationError(m)) { "ValidationError: " # m };
            case (#HttpOutcallError(#IcError({ code; message }))) { "HttpOutcallError(IcError " # debug_show (code) # "): " # message };
            case (#HttpOutcallError(#InvalidHttpJsonRpcResponse({ status; body }))) {
                "HttpOutcallError(InvalidHttpJsonRpcResponse status=" # debug_show (status) # "): " # body;
            };
        };
    };

    // ------------------------------------------------------------------
    // Transaction errors (verbatim from the candid; `Err` of `status`)
    // ------------------------------------------------------------------

    public type InstructionError = {
        #ModifiedProgramId;
        #CallDepth;
        #Immutable;
        #GenericError;
        #ExecutableAccountNotRentExempt;
        #IncorrectAuthority;
        #PrivilegeEscalation;
        #ReentrancyNotAllowed;
        #InvalidInstructionData;
        #RentEpochModified;
        #IllegalOwner;
        #ComputationalBudgetExceeded;
        #ExecutableDataModified;
        #ExecutableLamportChange;
        #UnbalancedInstruction;
        #ProgramEnvironmentSetupFailure;
        #IncorrectProgramId;
        #UnsupportedSysvar;
        #UnsupportedProgramId;
        #AccountDataTooSmall;
        #NotEnoughAccountKeys;
        #AccountBorrowFailed;
        #InvalidRealloc;
        #AccountNotExecutable;
        #AccountNotRentExempt;
        #Custom : Nat32;
        #AccountDataSizeChanged;
        #MaxAccountsDataAllocationsExceeded;
        #ExternalAccountLamportSpend;
        #ExternalAccountDataModified;
        #MissingAccount;
        #ProgramFailedToComplete;
        #MaxInstructionTraceLengthExceeded;
        #InvalidAccountData;
        #ProgramFailedToCompile;
        #ExecutableModified;
        #InvalidAccountOwner;
        #MaxSeedLengthExceeded;
        #AccountAlreadyInitialized;
        #AccountBorrowOutstanding;
        #ReadonlyDataModified;
        #UninitializedAccount;
        #InvalidArgument;
        #BorshIoError : Text;
        #BuiltinProgramsMustConsumeComputeUnits;
        #MissingRequiredSignature;
        #DuplicateAccountOutOfSync;
        #MaxAccountsExceeded;
        #ArithmeticOverflow;
        #InvalidError;
        #InvalidSeeds;
        #DuplicateAccountIndex;
        #ReadonlyLamportChange;
        #InsufficientFunds;
    };

    public type TransactionError = {
        #ProgramCacheHitMaxLimit;
        #InvalidAccountForFee;
        #AddressLookupTableNotFound;
        #MissingSignatureForFee;
        #WouldExceedAccountDataBlockLimit;
        #AccountInUse;
        #DuplicateInstruction : Nat8;
        #AccountNotFound;
        #TooManyAccountLocks;
        #InvalidAccountIndex;
        #AlreadyProcessed;
        #WouldExceedAccountDataTotalLimit;
        #InvalidAddressLookupTableIndex;
        #SanitizeFailure;
        #ResanitizationNeeded;
        #InvalidRentPayingAccount;
        #MaxLoadedAccountsDataSizeExceeded;
        #InvalidAddressLookupTableData;
        #InvalidWritableAccount;
        #WouldExceedMaxAccountCostLimit;
        #InvalidLoadedAccountsDataSizeLimit;
        #InvalidProgramForExecution;
        #InstructionError : (Nat8, InstructionError);
        #InsufficientFundsForRent : { account_index : Nat8 };
        #UnsupportedVersion;
        #ClusterMaintenance;
        #WouldExceedMaxVoteCostLimit;
        #SignatureFailure;
        #ProgramAccountNotFound;
        #AccountLoadedTwice;
        #ProgramExecutionTemporarilyRestricted : { account_index : Nat8 };
        #AccountBorrowOutstanding;
        #WouldExceedMaxBlockCostLimit;
        #InvalidAddressLookupTableOwner;
        #InsufficientFundsForFee;
        #CallChainTooDeep;
        #UnbalancedTransaction;
        #CommitCancelled;
        #BlockhashNotFound;
    };

    // ------------------------------------------------------------------
    // Scalars
    // ------------------------------------------------------------------

    public type Pubkey = Text;
    public type Signature = Text;
    public type Hash = Text;
    public type Slot = Nat64;
    public type Timestamp = Int64;

    public type CommitmentLevel = { #processed; #confirmed; #finalized };
    public type TransactionConfirmationStatus = { #processed; #confirmed; #finalized };

    // ------------------------------------------------------------------
    // getAccountInfo
    // ------------------------------------------------------------------

    /// Only what the watchdog sends is declared (an argument type may have FEWER variant cases).
    public type GetAccountInfoEncoding = { #base64 };
    public type DataSlice = { length : Nat32; offset : Nat32 };

    public type GetAccountInfoParams = {
        pubkey : Pubkey;
        commitment : ?CommitmentLevel;
        encoding : ?GetAccountInfoEncoding;
        dataSlice : ?DataSlice;
        minContextSlot : ?Slot;
    };

    /// A REPLY type: every case of the canister's variant must be declared or the decode traps.
    /// `#_671227429_` is the Candid label "base64+zstd" (Motoko cannot spell a "+").
    public type AccountEncoding = { #binary; #base58; #base64; #_671227429_; #jsonParsed };
    public type ParsedAccount = { program : Pubkey; parsed : Text; space : Nat64 };
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
    public type GetAccountInfoResult = { #Ok : ?AccountInfo; #Err : RpcError };
    public type MultiGetAccountInfoResult = {
        #Consistent : GetAccountInfoResult;
        #Inconsistent : [(RpcSource, GetAccountInfoResult)];
    };

    // ------------------------------------------------------------------
    // getBalance
    // ------------------------------------------------------------------

    public type GetBalanceParams = { pubkey : Pubkey; commitment : ?CommitmentLevel; minContextSlot : ?Slot };
    public type GetBalanceResult = { #Ok : Nat64; #Err : RpcError };
    public type MultiGetBalanceResult = {
        #Consistent : GetBalanceResult;
        #Inconsistent : [(RpcSource, GetBalanceResult)];
    };

    // ------------------------------------------------------------------
    // getSignatureStatuses
    // ------------------------------------------------------------------

    public type GetSignatureStatusesParams = { signatures : [Signature]; searchTransactionHistory : ?Bool };
    public type TransactionStatus = {
        slot : Slot;
        status : { #Ok; #Err : TransactionError };
        err : ?TransactionError;
        confirmationStatus : ?TransactionConfirmationStatus;
    };
    public type GetSignatureStatusesResult = { #Ok : [?TransactionStatus]; #Err : RpcError };
    public type MultiGetSignatureStatusesResult = {
        #Consistent : GetSignatureStatusesResult;
        #Inconsistent : [(RpcSource, GetSignatureStatusesResult)];
    };

    // ------------------------------------------------------------------
    // getSlot (+ rounding config) and getBlock
    // ------------------------------------------------------------------

    public type GetSlotRpcConfig = {
        responseSizeEstimate : ?Nat64;
        responseConsensus : ?ConsensusStrategy;
        roundingError : ?Nat64;
    };
    public type GetSlotParams = { commitment : ?CommitmentLevel; minContextSlot : ?Slot };
    public type GetSlotResult = { #Ok : Slot; #Err : RpcError };
    public type MultiGetSlotResult = {
        #Consistent : GetSlotResult;
        #Inconsistent : [(RpcSource, GetSlotResult)];
    };

    public type TransactionDetails = { #accounts; #none; #signatures };
    public type GetBlockParams = {
        slot : Slot;
        transactionDetails : ?TransactionDetails;
        commitment : ?{ #confirmed; #finalized };
        maxSupportedTransactionVersion : ?Nat8;
        rewards : ?Bool;
    };
    /// Only the fields the watchdog reads (the canister's `ConfirmedBlock` has more).
    public type ConfirmedBlock = {
        previousBlockhash : Hash;
        blockhash : Hash;
        parentSlot : Slot;
        blockTime : ?Timestamp;
        blockHeight : ?Nat64;
    };
    public type GetBlockResult = { #Ok : ?ConfirmedBlock; #Err : RpcError };
    public type MultiGetBlockResult = {
        #Consistent : GetBlockResult;
        #Inconsistent : [(RpcSource, GetBlockResult)];
    };

    // ------------------------------------------------------------------
    // sendTransaction
    // ------------------------------------------------------------------

    public type SendTransactionEncoding = { #base58; #base64 };
    public type SendTransactionParams = {
        transaction : Text;
        encoding : ?SendTransactionEncoding;
        skipPreflight : ?Bool;
        preflightCommitment : ?CommitmentLevel;
        maxRetries : ?Nat32;
        minContextSlot : ?Slot;
    };
    public type SendTransactionResult = { #Ok : Signature; #Err : RpcError };
    public type MultiSendTransactionResult = {
        #Consistent : SendTransactionResult;
        #Inconsistent : [(RpcSource, SendTransactionResult)];
    };

    // ------------------------------------------------------------------
    // Actor type
    // ------------------------------------------------------------------

    public type RpcActor = actor {
        getAccountInfo : shared (RpcSources, ?RpcConfig, GetAccountInfoParams) -> async MultiGetAccountInfoResult;
        getBalance : shared (RpcSources, ?RpcConfig, GetBalanceParams) -> async MultiGetBalanceResult;
        getSignatureStatuses : shared (RpcSources, ?RpcConfig, GetSignatureStatusesParams) -> async MultiGetSignatureStatusesResult;
        getSlot : shared (RpcSources, ?GetSlotRpcConfig, ?GetSlotParams) -> async MultiGetSlotResult;
        getBlock : shared (RpcSources, ?RpcConfig, GetBlockParams) -> async MultiGetBlockResult;
        sendTransaction : shared (RpcSources, ?RpcConfig, SendTransactionParams) -> async MultiSendTransactionResult;
    };

    // ------------------------------------------------------------------
    // Reply helpers
    // ------------------------------------------------------------------

    /// `Consistent(Ok x)` -> `#ok x`. A provider error or a disagreement between
    /// providers is `#err` with text: the watchdog does not act on it.
    public func ok<T>(m : { #Consistent : { #Ok : T; #Err : RpcError }; #Inconsistent : [(RpcSource, { #Ok : T; #Err : RpcError })] }) : { #ok : T; #err : Text } {
        switch (m) {
            case (#Consistent(#Ok(x))) { #ok(x) };
            case (#Consistent(#Err(e))) { #err(errorText(e)) };
            case (#Inconsistent(rs)) { #err("providers disagree (" # debug_show (rs.size()) # " answers)") };
        };
    };

    /// The raw account bytes of a base64-encoded `AccountInfo` (null for any other encoding).
    public func accountBytes(info : AccountInfo) : ?Blob {
        switch (info.data) {
            case (#binary((text, #base64))) { Base64.decode(text) };
            case (_) { null };
        };
    };

    public type SendOutcome = {
        /// Every provider returned the signature.
        #Accepted : Signature;
        /// The node answered with a JSON-RPC error object (preflight/simulation failure,
        /// malformed transaction): the transaction was refused, not broadcast.
        #Rejected : Text;
        /// Anything else after a send attempt (HTTP outcall error, providers disagree,
        /// decode failure): the transaction MAY have been broadcast.
        #Unknown : Text;
    };

    public func classifySend(m : MultiSendTransactionResult) : SendOutcome {
        switch (m) {
            case (#Consistent(#Ok(sig))) { #Accepted(sig) };
            case (#Consistent(#Err(#JsonRpcError(e)))) { #Rejected(errorText(#JsonRpcError(e))) };
            case (#Consistent(#Err(e))) { #Unknown(errorText(e)) };
            case (#Inconsistent(rs)) { #Unknown("providers disagree (" # debug_show (rs.size()) # " answers)") };
        };
    };
};
