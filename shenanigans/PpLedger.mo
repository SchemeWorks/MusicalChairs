/**
 * PP Ledger integration module (ICRC-1 + ICRC-2).
 * Mirrors backend/ledger.mo shape but points at pp_ledger on mainnet.
 */

import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";

module {
    /// PP Ledger canister ID on mainnet (unchanged across reinstall)
    public let PP_LEDGER_CANISTER_ID : Text = "5xv2o-iiaaa-aaaac-qeclq-cai";

    /// PP uses 8 decimals post-reinstall; 1 whole PP = 10^8 units
    public let PP_DECIMALS : Nat8 = 8;
    public let PP_UNIT_SCALE : Nat = 100_000_000;

    public type Account = {
        owner : Principal;
        subaccount : ?Blob;
    };

    public type TransferArg = {
        from_subaccount : ?Blob;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type TransferError = {
        #BadFee : { expected_fee : Nat };
        #BadBurn : { min_burn_amount : Nat };
        #InsufficientFunds : { balance : Nat };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type TransferResult = { #Ok : Nat; #Err : TransferError };

    public type ApproveArg = {
        from_subaccount : ?Blob;
        spender : Account;
        amount : Nat;
        expected_allowance : ?Nat;
        expires_at : ?Nat64;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type ApproveError = {
        #BadFee : { expected_fee : Nat };
        #InsufficientFunds : { balance : Nat };
        #AllowanceChanged : { current_allowance : Nat };
        #Expired : { ledger_time : Nat64 };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type ApproveResult = { #Ok : Nat; #Err : ApproveError };

    public type TransferFromArg = {
        spender_subaccount : ?Blob;
        from : Account;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    public type TransferFromError = {
        #BadFee : { expected_fee : Nat };
        #BadBurn : { min_burn_amount : Nat };
        #InsufficientFunds : { balance : Nat };
        #InsufficientAllowance : { allowance : Nat };
        #TooOld;
        #CreatedInFuture : { ledger_time : Nat64 };
        #Duplicate : { duplicate_of : Nat };
        #TemporarilyUnavailable;
        #GenericError : { error_code : Nat; message : Text };
    };

    public type TransferFromResult = { #Ok : Nat; #Err : TransferFromError };

    public type LedgerActor = actor {
        icrc1_balance_of : shared query Account -> async Nat;
        icrc1_fee : shared query () -> async Nat;
        icrc1_transfer : shared TransferArg -> async TransferResult;
        icrc2_transfer_from : shared TransferFromArg -> async TransferFromResult;
    };
}
