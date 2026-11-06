import Int64 "mo:base/Int64";

/// Rpc — Motoko actor-type bindings for the DFINITY SOL RPC canister,
/// restricted to the two methods the bridge calls: `getTransaction` and
/// `getSignaturesForAddress`.
///
/// PINNED to `native_sol_bridge/candid/sol_rpc_canister.did`
/// (mainnet `tghme-zyaaa-aaaar-qarca-cai`, module 0x1733b1e5…3115, captured
/// 2026-10-13). Every type below mirrors that file's field names, optionality
/// and integer widths EXACTLY, because the reply is decoded by the Candid
/// runtime and a width mismatch (`nat16` declared as `Nat`, a variant tag the
/// reply carries but the declared variant lacks) traps AFTER the call.
/// Nothing here is shared with `ponzi_math_sol/SolRpc.mo`; that file's
/// `TransactionStatusMeta` omits `status` and uses a looser `err`, which is
/// not good enough for provenance (`meta.err == null` is a hard requirement).
///
/// Consensus: the SOL RPC canister answers every method with a `Multi*Result`
/// that is either `Consistent` (all contacted providers agreed) or
/// `Inconsistent` (per-provider results). The bridge treats anything other
/// than `Consistent(Ok(?tx))` from a provider as "not an observation".
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

    /// JSON-RPC code Solana nodes return when `maxSupportedTransactionVersion`
    /// is lower than the transaction's version ("Transaction version (N) is
    /// not supported by the requesting client").
    public let JSON_RPC_UNSUPPORTED_TX_VERSION : Int64 = -32015;

    public func errorText(e : RpcError) : Text {
        switch (e) {
            case (#JsonRpcError({ code; message })) {
                "JsonRpcError(" # Int64.toText(code) # "): " # message;
            };
            case (#ProviderError(#TooFewCycles({ expected; received }))) {
                "ProviderError(TooFewCycles): expected=" # debug_show (expected) # " received=" # debug_show (received);
            };
            case (#ProviderError(#InvalidRpcConfig(m))) { "ProviderError(InvalidRpcConfig): " # m };
            case (#ProviderError(#UnsupportedCluster(m))) { "ProviderError(UnsupportedCluster): " # m };
            case (#ValidationError(m)) { "ValidationError: " # m };
            case (#HttpOutcallError(#IcError({ code; message }))) {
                "HttpOutcallError(IcError " # debug_show (code) # "): " # message;
            };
            case (#HttpOutcallError(#InvalidHttpJsonRpcResponse({ status; body }))) {
                "HttpOutcallError(InvalidHttpJsonRpcResponse status=" # debug_show (status) # "): " # body;
            };
        };
    };

    // ------------------------------------------------------------------
    // Scalars
    // ------------------------------------------------------------------

    public type Pubkey = Text;
    public type Signature = Text;
    public type Slot = Nat64;
    public type Timestamp = Int64;

    public type CommitmentLevel = { #processed; #confirmed; #finalized };

    public type TransactionConfirmationStatus = { #processed; #confirmed; #finalized };

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
    // getTransaction
    // ------------------------------------------------------------------

    /// `data` is base58, `accounts` are indices into the full account list
    /// (static keys, then loaded writable, then loaded readonly addresses).
    public type CompiledInstruction = {
        data : Text;
        accounts : Blob;
        programIdIndex : Nat8;
        stackHeight : ?Nat32;
    };

    public type Instruction = { #compiled : CompiledInstruction };

    public type InnerInstructions = {
        instructions : [Instruction];
        index : Nat8;
    };

    public type LoadedAddresses = { writable : [Pubkey]; readonly : [Pubkey] };

    public type Reward = {
        lamports : Int64;
        commission : ?Nat8;
        pubkey : Pubkey;
        rewardType : ?{ #fee; #rent; #voting; #staking };
        postBalance : Nat64;
    };

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

    public type TransactionStatusMeta = {
        fee : Nat64;
        /// `#Ok` iff the transaction succeeded (Solana `meta.err == null`).
        status : { #Ok; #Err : TransactionError };
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
    };

    public type EncodedTransaction = {
        /// (encoded serialized transaction, encoding)
        #binary : (Text, { #base58; #base64 });
        /// Legacy base58 string form.
        #legacyBinary : Text;
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

    // ------------------------------------------------------------------
    // getSignaturesForAddress (declared for gap scans; not called by main)
    // ------------------------------------------------------------------

    public type GetSignaturesForAddressParams = {
        pubkey : Pubkey;
        commitment : ?CommitmentLevel;
        minContextSlot : ?Slot;
        limit : ?Nat32;
        before : ?Signature;
        until : ?Signature;
    };

    public type ConfirmedTransactionStatusWithSignature = {
        signature : Signature;
        slot : Slot;
        err : ?TransactionError;
        memo : ?Text;
        blockTime : ?Timestamp;
        confirmationStatus : ?TransactionConfirmationStatus;
    };

    public type GetSignaturesForAddressResult = {
        #Ok : [ConfirmedTransactionStatusWithSignature];
        #Err : RpcError;
    };

    public type MultiGetSignaturesForAddressResult = {
        #Consistent : GetSignaturesForAddressResult;
        #Inconsistent : [(RpcSource, GetSignaturesForAddressResult)];
    };

    // ------------------------------------------------------------------
    // Actor type
    // ------------------------------------------------------------------

    public type RpcActor = actor {
        getTransaction : shared (RpcSources, ?RpcConfig, GetTransactionParams) -> async MultiGetTransactionResult;
        getSignaturesForAddress : shared (RpcSources, ?RpcConfig, GetSignaturesForAddressParams) -> async MultiGetSignaturesForAddressResult;
    };
};
