import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { AuthClient } from '@dfinity/auth-client';
import { Identity, AnonymousIdentity } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';

// ============================================================================
// Types
// ============================================================================

export type WalletType = 'none' | 'internet-identity' | 'plug' | 'oisy' | 'siws';

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
  // Wallet dropdown open/close state
  isOpen: boolean;
  openWallet: () => void;
  closeWallet: () => void;
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

// Mainnet IDs for the other dapp canisters Plug must be able to call.
// Duplicated string-literally here (rather than imported) to keep the
// whitelist isolated to one file — these IDs are stable and only ever
// change when the canister is reinstalled, which is essentially never.
// Canonical definitions live in useShenaniganActor.ts, usePonziMathActor.ts,
// and usePpLedger.ts respectively.
const SHENANIGANS_CANISTER_ID = 'j56tm-oaaaa-aaaac-qf34q-cai';
const PONZI_MATH_CANISTER_ID = 'guy42-yqaaa-aaaaj-qr5pq-cai';
const PP_LEDGER_CANISTER_ID = '5xv2o-iiaaa-aaaac-qeclq-cai';

// Plug rejects update calls to canisters that aren't in the whitelist
// it was connected with. ALL canisters this dapp talks to via update
// calls must be listed here. Missing entries cause silent registerReferral
// / deposit / spell failures — see PR that added shenanigans+ponzi_math
// after a referral-chain bug on Plug.
//
// Exported so failure-recovery paths (e.g. the registerReferral fail toast
// in useQueries.ts) can re-issue requestConnect with the exact same set
// when Plug's extension authorization is stale.
export const PLUG_WHITELIST = [
  BACKEND_CANISTER_ID,
  ICP_LEDGER_CANISTER_ID,
  SHENANIGANS_CANISTER_ID,
  PONZI_MATH_CANISTER_ID,
  PP_LEDGER_CANISTER_ID,
];

// Host configuration - detect based on URL
const IS_LOCAL = typeof window !== 'undefined' && window.location.hostname === 'localhost';
export const IC_HOST = IS_LOCAL ? 'http://localhost:4943' : 'https://icp0.io';

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
  const [isOpen, setIsOpen] = useState(false);
  const openWallet = useCallback(() => setIsOpen(true), []);
  const closeWallet = useCallback(() => setIsOpen(false), []);

  const isConnected = walletType !== 'none' && (!!identity || walletType === 'oisy' || walletType === 'siws');

  // ============================================================================
  // Initialization - Check for existing sessions
  // ============================================================================

  useEffect(() => {
    initializeWallet();
  }, []);

  const initializeWallet = async () => {
    try {
      // Always pre-create AuthClient so it's ready when the user clicks login.
      // Safari blocks popups if window.open() is called after an async gap —
      // creating the client here ensures the click→login()→window.open() path is synchronous.
      const client = await AuthClient.create({
        idleOptions: {
          idleTimeout: 1000 * 60 * 30, // 30 minutes
          disableDefaultIdleCallback: true,
        },
      });
      setAuthClient(client);

      // Check for saved wallet type
      const savedWalletType = localStorage.getItem('musical-chairs-wallet-type') as WalletType | null;

      if (savedWalletType === 'plug') {
        // Try to restore Plug connection
        await restorePlugConnection();
      } else if (savedWalletType === 'internet-identity') {
        // Restore II using the already-created client
        const isAuthenticated = await client.isAuthenticated();
        if (isAuthenticated) {
          const ident = client.getIdentity();
          setIdentity(ident);
          setPrincipal(ident.getPrincipal().toString());
          setWalletType(savedWalletType);
        }
      } else if (savedWalletType === 'oisy') {
        const { restoreOisySession } = await import('../lib/oisySigner');
        const principalText = await restoreOisySession();
        if (principalText) {
          setPrincipal(principalText);
          setWalletType('oisy');
        } else {
          // Session expired or signer has no active session — downgrade cleanly.
          localStorage.removeItem('musical-chairs-wallet-type');
        }
      } else if (savedWalletType === 'siws') {
        // Option C prefetch: kick off the wallet-adapter chunk download in the
        // background. Returning SIWS users whose delegation has expired will
        // re-sign-in via the modal — and the modal needs these modules. By
        // firing the import here we eliminate the ~500ms click hitch.
        // Fire-and-forget; failure just falls back to the connectSiws() click
        // path which lazy-loads the same modules.
        void Promise.all([
          import('@solana/wallet-adapter-base'),
          import('@solana/wallet-adapter-wallets'),
        ]).catch(() => { /* network blip; retry on click */ });

        const { restoreSiwsSession } = await import('../lib/siwsSigner');
        const connection = await restoreSiwsSession();
        if (connection) {
          setIdentity(connection.identity);
          setPrincipal(connection.principal);
          setWalletType('siws');
        } else {
          // Session expired or saved state was corrupted — downgrade cleanly.
          localStorage.removeItem('musical-chairs-wallet-type');
        }
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
            whitelist: PLUG_WHITELIST,
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
        case 'siws':
          await connectSiws();
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

    const connected = await window.ic.plug.requestConnect({
      whitelist: PLUG_WHITELIST,
      host: IC_HOST,
      timeout: 50000,
    });

    if (!connected) {
      throw new Error('User rejected Plug connection');
    }

    // Create agent for the whitelisted canisters
    await window.ic.plug.createAgent({ whitelist: PLUG_WHITELIST, host: IC_HOST });

    if (!window.ic.plug.principalId) {
      throw new Error('Failed to get principal from Plug');
    }

    setWalletType('plug');
    setPrincipal(window.ic.plug.principalId);
    setIdentity(new AnonymousIdentity()); // Placeholder for Plug
  };

  const connectInternetIdentity = async () => {
    // authClient is pre-created during initialization (see initializeWallet).
    // This keeps the click→login()→window.open() path synchronous,
    // which is required for Safari/iOS to allow the popup.
    if (!authClient) {
      throw new Error('AuthClient not initialized — please wait for the app to finish loading.');
    }

    const iiUrl = IS_LOCAL ? II_URL_LOCAL : II_URL_MAINNET;

    // Frontend canister origin — used as derivationOrigin so II recognises
    // requests from the custom domain (musicalchairs.fun).
    const FRONTEND_CANISTER_ORIGIN = 'https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io';

    return new Promise<void>((resolve, reject) => {
      authClient.login({
        identityProvider: iiUrl,
        maxTimeToLive: BigInt(30 * 24 * 60 * 60 * 1000 * 1000 * 1000), // 30 days (II hard cap)
        // Tell II to derive the principal from the canister origin, not the custom domain
        ...(IS_LOCAL ? {} : { derivationOrigin: FRONTEND_CANISTER_ORIGIN }),
        onSuccess: () => {
          console.log('II login successful!');
          const identity = authClient!.getIdentity();
          setIdentity(identity);
          setPrincipal(identity.getPrincipal().toString());
          setWalletType('internet-identity');
          resolve();
        },
        onError: (error: string | undefined) => {
          console.error('II login error:', error);
          reject(new Error(error || 'Internet Identity login failed'));
        },
      });
    });
  };

  const connectOisy = async () => {
    const { oisySigner } = await import('../lib/oisySigner');

    try {
      const accounts = await oisySigner.getAccounts();

      if (!accounts || accounts.length === 0) {
        throw new Error('No accounts returned from OISY');
      }

      const account = accounts[0];
      const principalText = account.owner.toText();

      console.log('OISY principal:', principalText);

      setPrincipal(principalText);
      // Don't set identity — Oisy path uses SignerAgent in useActor, not an Identity
      setWalletType('oisy');
    } catch (error) {
      console.error('OISY connection error:', error);
      throw new Error(error instanceof Error ? error.message : 'OISY connection failed');
    }
  };

  const connectSiws = async () => {
    const { connectSiws: doConnect } = await import('../lib/siwsSigner');
    const walletAdapterMod = await import('@solana/wallet-adapter-base');
    const walletsMod = await import('@solana/wallet-adapter-wallets');

    // Detect available Solana wallets. Phantom and Solflare ship explicit
    // adapters because they predate the Wallet Standard protocol; newer
    // wallets like Backpack/Coinbase/Glow use Wallet Standard auto-discovery
    // and have no per-wallet adapter package. To support them we'd switch
    // to `@wallet-standard/core::getWallets()` and iterate the registered
    // wallet-standard wallets — a heavier refactor. For now we list only
    // the legacy-adapter wallets explicitly.
    const wallets = [
      new walletsMod.PhantomWalletAdapter(),
      new walletsMod.SolflareWalletAdapter(),
    ];

    const adapter = wallets.find(
      (w) => w.readyState === walletAdapterMod.WalletReadyState.Installed,
    );

    if (!adapter) {
      throw new Error(
        'No Solana wallet detected. Install Phantom or Solflare to continue.',
      );
    }

    // Connect to the wallet. This pops the wallet's "approve connection" UI.
    await adapter.connect();

    if (!adapter.publicKey) {
      throw new Error('Wallet did not return a public key.');
    }

    const pubkeyBase58 = adapter.publicKey.toBase58();

    // Wrap adapter.signMessage so the SIWS signer can call it via a generic
    // (msg: Uint8Array) => Promise<Uint8Array> callback.
    const signMessage = async (msg: Uint8Array): Promise<Uint8Array> => {
      if (!adapter.signMessage) {
        throw new Error('Selected wallet does not support signMessage.');
      }
      return adapter.signMessage(msg);
    };

    const connection = await doConnect(signMessage, pubkeyBase58);

    setIdentity(connection.identity);
    setPrincipal(connection.principal);
    setWalletType('siws');
  };

  // ============================================================================
  // Disconnect
  // ============================================================================

  const disconnect = useCallback(async () => {
    try {
      if (walletType === 'plug' && window.ic?.plug) {
        await window.ic.plug.disconnect();
      } else if (walletType === 'oisy') {
        const { clearOisySigner } = await import('../lib/oisySigner');
        clearOisySigner();
      } else if (walletType === 'siws') {
        const { clearSiwsSession } = await import('../lib/siwsSigner');
        clearSiwsSession();
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
    isOpen,
    openWallet,
    closeWallet,
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
