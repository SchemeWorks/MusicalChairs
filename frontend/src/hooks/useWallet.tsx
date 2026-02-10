import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { AuthClient } from '@dfinity/auth-client';
import { Identity, AnonymousIdentity } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';

// ============================================================================
// Types
// ============================================================================

export type WalletType = 'none' | 'internet-identity' | 'plug' | 'oisy';

export interface WalletState {
  walletType: WalletType;
  identity: Identity | null;
  principal: string | null;
  isConnected: boolean;
  isConnecting: boolean;
  isInitializing: boolean;
}

export interface WalletContextType extends WalletState {
  connect: (walletType: WalletType) => Promise<void>;
  disconnect: () => Promise<void>;
  // Ledger operations (will be used for deposits/withdrawals)
  getICPBalance: () => Promise<bigint>;
  transferICP: (to: string, amount: bigint) => Promise<{ height: bigint }>;
  approveICP: (spender: string, amount: bigint) => Promise<{ allowance: bigint }>;
}

// ============================================================================
// Constants
// ============================================================================

// Internet Identity URLs - Using II 2.0 (id.ai)
const II_URL_MAINNET = 'https://id.ai';
const II_URL_LOCAL = 'http://localhost:4943?canisterId=rdmx6-jaaaa-aaaaa-aaadq-cai';

// OISY uses Internet Identity under the hood
const OISY_URL = 'https://oisy.com';

// ICP Ledger canister ID (mainnet)
const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai';

// Your backend canister ID (for ICRC-2 approvals)
const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';

// Host configuration - detect based on URL
const IS_LOCAL = typeof window !== 'undefined' && window.location.hostname === 'localhost';
const IC_HOST = IS_LOCAL ? 'http://localhost:4943' : 'https://icp0.io';

// ============================================================================
// Plug Wallet Types (from window.ic.plug)
// ============================================================================

interface PlugAgent {
  getPrincipal: () => Promise<Principal>;
}

interface PlugWallet {
  isConnected: () => Promise<boolean>;
  requestConnect: (options?: {
    whitelist?: string[];
    host?: string;
    timeout?: number;
  }) => Promise<boolean>;
  disconnect: () => Promise<void>;
  createAgent: (options?: { whitelist?: string[]; host?: string }) => Promise<PlugAgent>;
  agent: PlugAgent | null;
  principalId: string | null;
  accountId: string | null;
  requestTransfer: (params: {
    to: string;
    amount: number; // in e8s
    opts?: { fee?: number; memo?: string };
  }) => Promise<{ height: bigint }>;
  requestBalance: () => Promise<Array<{ amount: number; canisterId: string; decimals: number; name: string; symbol: string }>>;
}

declare global {
  interface Window {
    ic?: {
      plug?: PlugWallet;
    };
  }
}

// ============================================================================
// Context
// ============================================================================

const WalletContext = createContext<WalletContextType | undefined>(undefined);

// ============================================================================
// Provider
// ============================================================================

interface WalletProviderProps {
  children: ReactNode;
}

export function WalletProvider({ children }: WalletProviderProps) {
  const [walletType, setWalletType] = useState<WalletType>('none');
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [principal, setPrincipal] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [isInitializing, setIsInitializing] = useState(true);
  const [authClient, setAuthClient] = useState<AuthClient | null>(null);

  const isConnected = !!identity && walletType !== 'none';

  // ============================================================================
  // Initialization - Check for existing sessions
  // ============================================================================

  useEffect(() => {
    initializeWallet();
  }, []);

  const initializeWallet = async () => {
    try {
      // Check for saved wallet type
      const savedWalletType = localStorage.getItem('musical-chairs-wallet-type') as WalletType | null;

      if (savedWalletType === 'plug') {
        // Try to restore Plug connection
        await restorePlugConnection();
      } else if (savedWalletType === 'internet-identity' || savedWalletType === 'oisy') {
        // Try to restore II/OISY connection
        await restoreIIConnection(savedWalletType);
      }
    } catch (error) {
      console.error('Failed to restore wallet connection:', error);
      // Clear saved state on error
      localStorage.removeItem('musical-chairs-wallet-type');
    } finally {
      setIsInitializing(false);
    }
  };

  const restorePlugConnection = async () => {
    if (!window.ic?.plug) {
      return;
    }

    try {
      const connected = await window.ic.plug.isConnected();
      if (connected && window.ic.plug.principalId) {
        // Create agent if not exists
        if (!window.ic.plug.agent) {
          await window.ic.plug.createAgent({
            whitelist: [BACKEND_CANISTER_ID, ICP_LEDGER_CANISTER_ID],
            host: IC_HOST,
          });
        }

        setWalletType('plug');
        setPrincipal(window.ic.plug.principalId);
        // Plug doesn't expose a standard Identity, we'll handle this in useActor
        setIdentity(new AnonymousIdentity()); // Placeholder - Plug uses its own agent
      }
    } catch (error) {
      console.error('Failed to restore Plug connection:', error);
    }
  };

  const restoreIIConnection = async (type: 'internet-identity' | 'oisy') => {
    try {
      const client = await AuthClient.create({
        idleOptions: {
          idleTimeout: 1000 * 60 * 30, // 30 minutes
          disableDefaultIdleCallback: true,
        },
      });

      setAuthClient(client);

      const isAuthenticated = await client.isAuthenticated();
      if (isAuthenticated) {
        const identity = client.getIdentity();
        setIdentity(identity);
        setPrincipal(identity.getPrincipal().toString());
        setWalletType(type);
      }
    } catch (error) {
      console.error('Failed to restore II connection:', error);
    }
  };

  // ============================================================================
  // Connect
  // ============================================================================

  const connect = useCallback(async (type: WalletType) => {
    if (type === 'none') return;

    setIsConnecting(true);

    try {
      switch (type) {
        case 'plug':
          await connectPlug();
          break;
        case 'internet-identity':
          await connectInternetIdentity();
          break;
        case 'oisy':
          await connectOisy();
          break;
      }

      // Save wallet type for session restoration
      localStorage.setItem('musical-chairs-wallet-type', type);
    } catch (error) {
      console.error(`Failed to connect ${type}:`, error);
      throw error;
    } finally {
      setIsConnecting(false);
    }
  }, [authClient]);

  const connectPlug = async () => {
    if (!window.ic?.plug) {
      throw new Error('Plug wallet is not installed. Please install the Plug browser extension.');
    }

    const whitelist = [BACKEND_CANISTER_ID, ICP_LEDGER_CANISTER_ID];

    const connected = await window.ic.plug.requestConnect({
      whitelist,
      host: IC_HOST,
      timeout: 50000,
    });

    if (!connected) {
      throw new Error('User rejected Plug connection');
    }

    // Create agent for the whitelisted canisters
    await window.ic.plug.createAgent({ whitelist, host: IC_HOST });

    if (!window.ic.plug.principalId) {
      throw new Error('Failed to get principal from Plug');
    }

    setWalletType('plug');
    setPrincipal(window.ic.plug.principalId);
    setIdentity(new AnonymousIdentity()); // Placeholder for Plug
  };

  const connectInternetIdentity = async () => {
    let client = authClient;

    if (!client) {
      client = await AuthClient.create({
        idleOptions: {
          idleTimeout: 1000 * 60 * 30,
          disableDefaultIdleCallback: true,
        },
      });
      setAuthClient(client);
    }

    const iiUrl = IS_LOCAL ? II_URL_LOCAL : II_URL_MAINNET;
    console.log('Connecting to II at:', iiUrl);
    console.log('IS_LOCAL:', IS_LOCAL);

    return new Promise<void>((resolve, reject) => {
      client!.login({
        identityProvider: iiUrl,
        maxTimeToLive: BigInt(7 * 24 * 60 * 60 * 1000 * 1000 * 1000), // 7 days
        windowOpenerFeatures: `
          left=${window.screen.width / 2 - 250},
          top=${window.screen.height / 2 - 300},
          toolbar=0,location=0,menubar=0,width=500,height=600
        `,
        onSuccess: () => {
          console.log('II login successful!');
          const identity = client!.getIdentity();
          setIdentity(identity);
          setPrincipal(identity.getPrincipal().toString());
          setWalletType('internet-identity');
          resolve();
        },
        onError: (error) => {
          console.error('II login error:', error);
          reject(new Error(error || 'Internet Identity login failed'));
        },
      });
    });
  };

  const connectOisy = async () => {
    // OISY uses the signer standards (ICRC-25, ICRC-27, etc.)
    // Import dynamically to avoid bundling issues
    const { IcpWallet } = await import('@dfinity/oisy-wallet-signer/icp-wallet');
    
    try {
      // Connect to OISY wallet signer
      const wallet = await IcpWallet.connect({
        url: 'https://oisy.com/sign',
      });

      // Get accounts from OISY - returns { accounts: Account[] }
      const response = await wallet.accounts();
      console.log('OISY accounts response:', response);
      
      // The response structure is { accounts: [{ owner: Principal, subaccount?: Uint8Array }] }
      const accounts = response?.accounts || response;
      
      if (!accounts || accounts.length === 0) {
        throw new Error('No accounts returned from OISY');
      }

      const account = accounts[0];
      // owner is a Principal object
      const principalText = typeof account.owner === 'string' 
        ? account.owner 
        : account.owner.toText 
          ? account.owner.toText() 
          : account.owner.toString();

      console.log('OISY principal:', principalText);

      // Store the wallet reference for later use
      (window as any).__oisyWallet = wallet;

      setPrincipal(principalText);
      setIdentity(new AnonymousIdentity()); // OISY handles signing via signer protocol
      setWalletType('oisy');
    } catch (error) {
      console.error('OISY connection error:', error);
      throw new Error(error instanceof Error ? error.message : 'OISY connection failed');
    }
  };

  // ============================================================================
  // Disconnect
  // ============================================================================

  const disconnect = useCallback(async () => {
    try {
      if (walletType === 'plug' && window.ic?.plug) {
        await window.ic.plug.disconnect();
      } else if (authClient) {
        await authClient.logout();
      }
    } catch (error) {
      console.error('Error during disconnect:', error);
    }

    // Clear state
    setWalletType('none');
    setIdentity(null);
    setPrincipal(null);
    localStorage.removeItem('musical-chairs-wallet-type');

    // Reload to clear all cached state
    window.location.reload();
  }, [walletType, authClient]);

  // ============================================================================
  // ICP Ledger Operations
  // ============================================================================

  const getICPBalance = useCallback(async (): Promise<bigint> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    if (walletType === 'plug' && window.ic?.plug) {
      const balances = await window.ic.plug.requestBalance();
      const icpBalance = balances.find(b => b.symbol === 'ICP');
      return BigInt(icpBalance?.amount || 0);
    }

    // For II/OISY, we need to query the ledger directly
    // This will be implemented with the ledger actor
    throw new Error('ICP balance query not yet implemented for this wallet type');
  }, [isConnected, principal, walletType]);

  const transferICP = useCallback(async (to: string, amount: bigint): Promise<{ height: bigint }> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    if (walletType === 'plug' && window.ic?.plug) {
      const result = await window.ic.plug.requestTransfer({
        to,
        amount: Number(amount), // Plug expects number in e8s
      });
      return result;
    }

    // For II/OISY, we need to call the ledger directly
    throw new Error('ICP transfer not yet implemented for this wallet type');
  }, [isConnected, principal, walletType]);

  const approveICP = useCallback(async (spender: string, amount: bigint): Promise<{ allowance: bigint }> => {
    if (!isConnected || !principal) {
      throw new Error('Wallet not connected');
    }

    // ICRC-2 approve will be implemented with ledger actor
    throw new Error('ICP approval not yet implemented');
  }, [isConnected, principal, walletType]);

  // ============================================================================
  // Context Value
  // ============================================================================

  const value: WalletContextType = {
    walletType,
    identity,
    principal,
    isConnected,
    isConnecting,
    isInitializing,
    connect,
    disconnect,
    getICPBalance,
    transferICP,
    approveICP,
  };

  return (
    <WalletContext.Provider value={value}>
      {children}
    </WalletContext.Provider>
  );
}

// ============================================================================
// Hook
// ============================================================================

export function useWallet(): WalletContextType {
  const context = useContext(WalletContext);

  if (context === undefined) {
    throw new Error('useWallet must be used within a WalletProvider');
  }

  return context;
}

export default useWallet;
