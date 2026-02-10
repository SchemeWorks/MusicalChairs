/**
 * ICP Ledger Integration Hook
 * 
 * Provides methods to interact with the ICP Ledger canister for:
 * - Balance queries (ICRC-1)
 * - Transfers (ICRC-1) 
 * - Approvals (ICRC-2) for deposit flow
 */

import { useCallback, useMemo } from 'react';
import { Actor, HttpAgent, Identity } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';

// ============================================================================
// Constants
// ============================================================================

// ICP Ledger canister ID (mainnet)
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai';

// Backend canister ID (for ICRC-2 approvals)
const DFX_NETWORK = import.meta.env.VITE_DFX_NETWORK || 'local';
export const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';

// Host configuration
const IC_HOST = DFX_NETWORK === 'ic' ? 'https://icp0.io' : 'http://localhost:4943';

// ICP has 8 decimals (1 ICP = 100_000_000 e8s)
export const ICP_DECIMALS = 8;
export const E8S_PER_ICP = 100_000_000n;

// Standard ICP transfer fee
export const ICP_TRANSFER_FEE = 10_000n; // 0.0001 ICP

// ============================================================================
// IDL Factory for ICRC-1/ICRC-2 Ledger
// ============================================================================

const icrcLedgerIDL = ({ IDL }: { IDL: any }) => {
  const Subaccount = IDL.Vec(IDL.Nat8);
  const Account = IDL.Record({
    owner: IDL.Principal,
    subaccount: IDL.Opt(Subaccount),
  });
  const Tokens = IDL.Nat;
  const Timestamp = IDL.Nat64;
  const Memo = IDL.Vec(IDL.Nat8);

  // ICRC-1 Transfer
  const TransferArg = IDL.Record({
    from_subaccount: IDL.Opt(Subaccount),
    to: Account,
    amount: Tokens,
    fee: IDL.Opt(Tokens),
    memo: IDL.Opt(Memo),
    created_at_time: IDL.Opt(Timestamp),
  });

  const TransferError = IDL.Variant({
    BadFee: IDL.Record({ expected_fee: Tokens }),
    BadBurn: IDL.Record({ min_burn_amount: Tokens }),
    InsufficientFunds: IDL.Record({ balance: Tokens }),
    TooOld: IDL.Null,
    CreatedInFuture: IDL.Record({ ledger_time: Timestamp }),
    Duplicate: IDL.Record({ duplicate_of: IDL.Nat }),
    TemporarilyUnavailable: IDL.Null,
    GenericError: IDL.Record({ error_code: IDL.Nat, message: IDL.Text }),
  });

  const TransferResult = IDL.Variant({
    Ok: IDL.Nat,
    Err: TransferError,
  });

  // ICRC-2 Approve
  const ApproveArg = IDL.Record({
    from_subaccount: IDL.Opt(Subaccount),
    spender: Account,
    amount: Tokens,
    expected_allowance: IDL.Opt(Tokens),
    expires_at: IDL.Opt(Timestamp),
    fee: IDL.Opt(Tokens),
    memo: IDL.Opt(Memo),
    created_at_time: IDL.Opt(Timestamp),
  });

  const ApproveError = IDL.Variant({
    BadFee: IDL.Record({ expected_fee: Tokens }),
    InsufficientFunds: IDL.Record({ balance: Tokens }),
    AllowanceChanged: IDL.Record({ current_allowance: Tokens }),
    Expired: IDL.Record({ ledger_time: Timestamp }),
    TooOld: IDL.Null,
    CreatedInFuture: IDL.Record({ ledger_time: Timestamp }),
    Duplicate: IDL.Record({ duplicate_of: IDL.Nat }),
    TemporarilyUnavailable: IDL.Null,
    GenericError: IDL.Record({ error_code: IDL.Nat, message: IDL.Text }),
  });

  const ApproveResult = IDL.Variant({
    Ok: IDL.Nat,
    Err: ApproveError,
  });

  // ICRC-2 Allowance
  const AllowanceArg = IDL.Record({
    account: Account,
    spender: Account,
  });

  const Allowance = IDL.Record({
    allowance: Tokens,
    expires_at: IDL.Opt(Timestamp),
  });

  return IDL.Service({
    // ICRC-1 methods
    icrc1_balance_of: IDL.Func([Account], [Tokens], ['query']),
    icrc1_transfer: IDL.Func([TransferArg], [TransferResult], []),
    icrc1_fee: IDL.Func([], [Tokens], ['query']),
    icrc1_decimals: IDL.Func([], [IDL.Nat8], ['query']),
    icrc1_name: IDL.Func([], [IDL.Text], ['query']),
    icrc1_symbol: IDL.Func([], [IDL.Text], ['query']),
    icrc1_total_supply: IDL.Func([], [Tokens], ['query']),
    
    // ICRC-2 methods
    icrc2_approve: IDL.Func([ApproveArg], [ApproveResult], []),
    icrc2_allowance: IDL.Func([AllowanceArg], [Allowance], ['query']),
  });
};

// ============================================================================
// Types
// ============================================================================

export interface Account {
  owner: Principal;
  subaccount?: Uint8Array;
}

export interface TransferResult {
  Ok?: bigint;
  Err?: TransferError;
}

export interface TransferError {
  BadFee?: { expected_fee: bigint };
  InsufficientFunds?: { balance: bigint };
  TooOld?: null;
  CreatedInFuture?: { ledger_time: bigint };
  Duplicate?: { duplicate_of: bigint };
  TemporarilyUnavailable?: null;
  GenericError?: { error_code: bigint; message: string };
}

export interface ApproveResult {
  Ok?: bigint;
  Err?: ApproveError;
}

export interface ApproveError {
  BadFee?: { expected_fee: bigint };
  InsufficientFunds?: { balance: bigint };
  AllowanceChanged?: { current_allowance: bigint };
  Expired?: { ledger_time: bigint };
  TooOld?: null;
  CreatedInFuture?: { ledger_time: bigint };
  Duplicate?: { duplicate_of: bigint };
  TemporarilyUnavailable?: null;
  GenericError?: { error_code: bigint; message: string };
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Convert ICP amount (float) to e8s (bigint)
 */
export function icpToE8s(icp: number): bigint {
  return BigInt(Math.round(icp * 100_000_000));
}

/**
 * Convert e8s (bigint) to ICP amount (float)
 */
export function e8sToIcp(e8s: bigint): number {
  return Number(e8s) / 100_000_000;
}

/**
 * Format e8s as ICP string with proper decimals
 */
export function formatIcpBalance(e8s: bigint): string {
  const icp = e8sToIcp(e8s);
  // Remove trailing zeros but keep up to 8 decimals
  return icp.toFixed(8).replace(/\.?0+$/, '') || '0';
}

// ============================================================================
// Hook
// ============================================================================

export function useLedger() {
  const { identity, principal, walletType, isConnected } = useWallet();

  /**
   * Create a ledger actor with the current identity
   */
  const createLedgerActor = useCallback(async () => {
    if (!identity || !isConnected) {
      throw new Error('Wallet not connected');
    }

    // For Plug wallet, use Plug's built-in methods instead
    if (walletType === 'plug') {
      return null; // Will use Plug's native methods
    }

    const agent = await HttpAgent.create({
      identity,
      host: IC_HOST,
    });

    // Fetch root key for local development
    if (DFX_NETWORK !== 'ic') {
      await agent.fetchRootKey();
    }

    return Actor.createActor(icrcLedgerIDL, {
      agent,
      canisterId: ICP_LEDGER_CANISTER_ID,
    });
  }, [identity, isConnected, walletType]);

  /**
   * Get ICP balance for an account
   */
  const getBalance = useCallback(async (account?: Account): Promise<bigint> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    // Use Plug's native method if connected with Plug
    if (walletType === 'plug' && window.ic?.plug) {
      const balances = await window.ic.plug.requestBalance();
      const icpBalance = balances.find((b: any) => b.symbol === 'ICP');
      // Plug returns balance as a number (not e8s), convert to e8s
      return BigInt(Math.round((icpBalance?.amount || 0) * 100_000_000));
    }

    // For II/OISY, query the ledger directly
    const actor = await createLedgerActor();
    if (!actor) throw new Error('Failed to create ledger actor');

    const targetAccount = account || {
      owner: Principal.fromText(principal),
      subaccount: [],
    };

    const balance = await actor.icrc1_balance_of(targetAccount);
    return balance as bigint;
  }, [isConnected, principal, walletType, createLedgerActor]);

  /**
   * Transfer ICP to another account
   */
  const transfer = useCallback(async (
    to: string | Account,
    amount: bigint,
    memo?: Uint8Array
  ): Promise<TransferResult> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    // Convert string to Account if needed
    const toAccount: Account = typeof to === 'string' 
      ? { owner: Principal.fromText(to), subaccount: undefined }
      : to;

    // Use Plug's native transfer if connected with Plug
    if (walletType === 'plug' && window.ic?.plug) {
      try {
        const result = await window.ic.plug.requestTransfer({
          to: toAccount.owner.toString(),
          amount: Number(amount), // Plug expects e8s as number
        });
        return { Ok: result.height };
      } catch (error: any) {
        return { 
          Err: { 
            GenericError: { 
              error_code: 0n, 
              message: error.message || 'Plug transfer failed' 
            } 
          } 
        };
      }
    }

    // For II/OISY, call the ledger directly
    const actor = await createLedgerActor();
    if (!actor) throw new Error('Failed to create ledger actor');

    const result = await actor.icrc1_transfer({
      from_subaccount: [],
      to: {
        owner: toAccount.owner,
        subaccount: toAccount.subaccount ? [toAccount.subaccount] : [],
      },
      amount,
      fee: [],
      memo: memo ? [memo] : [],
      created_at_time: [],
    });

    return result as TransferResult;
  }, [isConnected, principal, walletType, createLedgerActor]);

  /**
   * Approve a spender to transfer ICP on behalf of the user (ICRC-2)
   * Used for the deposit flow: user approves backend canister to pull funds
   */
  const approve = useCallback(async (
    spender: string | Account,
    amount: bigint,
    expiresAt?: bigint
  ): Promise<ApproveResult> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    // Convert string to Account if needed
    const spenderAccount: Account = typeof spender === 'string'
      ? { owner: Principal.fromText(spender), subaccount: undefined }
      : spender;

    // Plug doesn't support ICRC-2 approve natively, so we need to use the actor
    // For all wallet types, we'll use the actor approach
    const actor = await createLedgerActor();
    if (!actor) {
      // For Plug, we need to create an actor using Plug's agent
      if (walletType === 'plug' && window.ic?.plug) {
        // Plug's createActor for ICRC-2 approve
        // Note: This requires Plug to support calling arbitrary canisters
        throw new Error('ICRC-2 approve via Plug requires additional setup. Please use Internet Identity for deposits.');
      }
      throw new Error('Failed to create ledger actor');
    }

    const result = await actor.icrc2_approve({
      from_subaccount: [],
      spender: {
        owner: spenderAccount.owner,
        subaccount: spenderAccount.subaccount ? [spenderAccount.subaccount] : [],
      },
      amount,
      expected_allowance: [],
      expires_at: expiresAt ? [expiresAt] : [],
      fee: [],
      memo: [],
      created_at_time: [],
    });

    return result as ApproveResult;
  }, [isConnected, principal, walletType, createLedgerActor]);

  /**
   * Get current allowance for a spender
   */
  const getAllowance = useCallback(async (spender: string | Account): Promise<{ allowance: bigint; expires_at?: bigint }> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    const spenderAccount: Account = typeof spender === 'string'
      ? { owner: Principal.fromText(spender), subaccount: undefined }
      : spender;

    const actor = await createLedgerActor();
    if (!actor) throw new Error('Failed to create ledger actor');

    const result = await actor.icrc2_allowance({
      account: {
        owner: Principal.fromText(principal),
        subaccount: [],
      },
      spender: {
        owner: spenderAccount.owner,
        subaccount: spenderAccount.subaccount ? [spenderAccount.subaccount] : [],
      },
    });

    return {
      allowance: (result as any).allowance as bigint,
      expires_at: (result as any).expires_at?.[0] as bigint | undefined,
    };
  }, [isConnected, principal, createLedgerActor]);

  /**
   * Approve and deposit ICP to the Musical Chairs backend
   * This is a two-step process:
   * 1. Approve the backend canister to transfer ICP from user's wallet
   * 2. Call backend's deposit method which uses transfer_from
   */
  const approveForDeposit = useCallback(async (amount: bigint): Promise<ApproveResult> => {
    // Add fee to the approval amount to cover transfer costs
    const amountWithFee = amount + ICP_TRANSFER_FEE;
    return approve(BACKEND_CANISTER_ID, amountWithFee);
  }, [approve]);

  return {
    // State
    isConnected,
    principal,
    walletType,
    
    // Methods
    getBalance,
    transfer,
    approve,
    getAllowance,
    approveForDeposit,
    
    // Utilities
    icpToE8s,
    e8sToIcp,
    formatIcpBalance,
    
    // Constants
    ICP_DECIMALS,
    E8S_PER_ICP,
    ICP_TRANSFER_FEE,
    BACKEND_CANISTER_ID,
  };
}

export default useLedger;
