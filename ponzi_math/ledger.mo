/**
 * ICP Ledger Integration Module
 * 
 * This module provides types and interfaces for interacting with the ICP Ledger
 * canister using ICRC-1 and ICRC-2 standards.
 */

import Principal "mo:base/Principal";
import Blob "mo:base/Blob";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Result "mo:base/Result";
import Int "mo:base/Int";
import Float "mo:base/Float";

module {
    // ========================================================================
    // Constants
    // ========================================================================
    
    /// ICP Ledger canister ID on mainnet
    public let ICP_LEDGER_CANISTER_ID : Text = "ryjl3-tyaaa-aaaaa-aaaba-cai";
    
    /// ICP has 8 decimal places
    public let ICP_DECIMALS : Nat8 = 8;
    
    /// 1 ICP = 100,000,000 e8s
    public let E8S_PER_ICP : Nat = 100_000_000;
    
    /// Standard ICP transfer fee (0.0001 ICP)
    public let ICP_TRANSFER_FEE : Nat = 10_000;

    // ========================================================================
    // Types
    // ========================================================================
    
    /// Account identifier for ICRC-1/ICRC-2
    public type Account = {
        owner : Principal;
        subaccount : ?Blob;
    };

    /// ICRC-1 Transfer Arguments
    public type TransferArg = {
        from_subaccount : ?Blob;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    /// ICRC-1 Transfer Error
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

    /// ICRC-1 Transfer Result
    public type TransferResult = {
        #Ok : Nat;  // Block index
        #Err : TransferError;
    };

    /// ICRC-2 Approve Arguments
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

    /// ICRC-2 Approve Error
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

    /// ICRC-2 Approve Result
    public type ApproveResult = {
        #Ok : Nat;  // Block index
        #Err : ApproveError;
    };

    /// ICRC-2 Transfer From Arguments
    public type TransferFromArg = {
        spender_subaccount : ?Blob;
        from : Account;
        to : Account;
        amount : Nat;
        fee : ?Nat;
        memo : ?Blob;
        created_at_time : ?Nat64;
    };

    /// ICRC-2 Transfer From Error
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

    /// ICRC-2 Transfer From Result
    public type TransferFromResult = {
        #Ok : Nat;  // Block index
        #Err : TransferFromError;
    };

    /// Allowance query arguments
    public type AllowanceArg = {
        account : Account;
        spender : Account;
    };

    /// Allowance response
    public type Allowance = {
        allowance : Nat;
        expires_at : ?Nat64;
    };

    // ========================================================================
    // Ledger Actor Interface
    // ========================================================================
    
    /// Interface for the ICP Ledger canister
    public type LedgerActor = actor {
        // ICRC-1 methods
        icrc1_balance_of : shared query Account -> async Nat;
        icrc1_transfer : shared TransferArg -> async TransferResult;
        icrc1_fee : shared query () -> async Nat;
        icrc1_decimals : shared query () -> async Nat8;
        icrc1_name : shared query () -> async Text;
        icrc1_symbol : shared query () -> async Text;
        icrc1_total_supply : shared query () -> async Nat;
        
        // ICRC-2 methods
        icrc2_approve : shared ApproveArg -> async ApproveResult;
        icrc2_transfer_from : shared TransferFromArg -> async TransferFromResult;
        icrc2_allowance : shared query AllowanceArg -> async Allowance;
    };

    // ========================================================================
    // Helper Functions
    // ========================================================================
    
    /// Convert ICP (Float) to e8s (Nat)
    public func icpToE8s(icp : Float) : Nat {
        let e8s = icp * 100_000_000.0;
        if (e8s < 0.0) { return 0 };
        return Int.abs(Float.toInt(e8s));
    };

    /// Convert e8s (Nat) to ICP (Float)
    public func e8sToIcp(e8s : Nat) : Float {
        return Float.fromInt(e8s) / 100_000_000.0;
    };

    /// Create an Account from a Principal (default subaccount)
    public func principalToAccount(p : Principal) : Account {
        return {
            owner = p;
            subaccount = null;
        };
    };

    /// Get the default subaccount (32 zero bytes)
    public func defaultSubaccount() : Blob {
        return Blob.fromArray([0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]);
    };
};
