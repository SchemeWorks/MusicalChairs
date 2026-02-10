/**
 * Compatibility layer for useInternetIdentity
 * 
 * This hook now wraps the new useWallet hook to maintain backward compatibility
 * with existing components that use useInternetIdentity.
 * 
 * New code should use useWallet directly for multi-wallet support.
 */

import React, { createContext, useContext, ReactNode } from 'react';
import { useWallet, WalletProvider as WalletProviderBase } from './useWallet';
import { Identity } from '@dfinity/agent';

// ============================================================================
// Types (maintaining backward compatibility)
// ============================================================================

interface InternetIdentityContextType {
  identity: Identity | null;
  principal: string | null;
  isInitializing: boolean;
  isAuthenticated: boolean;
  login: () => Promise<void>;
  logout: () => Promise<void>;
  clear: () => Promise<void>;
  loginStatus: 'idle' | 'logging-in' | 'logged-in';
  authClient: any | null;
}

// ============================================================================
// Context (for backward compatibility)
// ============================================================================

const InternetIdentityContext = createContext<InternetIdentityContextType | undefined>(undefined);

// ============================================================================
// Provider
// ============================================================================

interface InternetIdentityProviderProps {
  children: ReactNode;
}

export function InternetIdentityProvider({ children }: InternetIdentityProviderProps) {
  // Wrap children with the new WalletProvider
  return (
    <WalletProviderBase>
      <InternetIdentityBridge>
        {children}
      </InternetIdentityBridge>
    </WalletProviderBase>
  );
}

// Bridge component that provides the old API using the new wallet system
function InternetIdentityBridge({ children }: { children: ReactNode }) {
  const wallet = useWallet();

  // Map wallet state to old II API
  const value: InternetIdentityContextType = {
    identity: wallet.identity,
    principal: wallet.principal,
    isInitializing: wallet.isInitializing,
    isAuthenticated: wallet.isConnected,
    login: async () => {
      // Default to Internet Identity for backward compatibility
      await wallet.connect('internet-identity');
    },
    logout: wallet.disconnect,
    clear: wallet.disconnect,
    loginStatus: wallet.isConnecting 
      ? 'logging-in' 
      : wallet.isConnected 
        ? 'logged-in' 
        : 'idle',
    authClient: null, // Not exposed in new system
  };

  return (
    <InternetIdentityContext.Provider value={value}>
      {children}
    </InternetIdentityContext.Provider>
  );
}

// ============================================================================
// Hook
// ============================================================================

export function useInternetIdentity(): InternetIdentityContextType {
  const context = useContext(InternetIdentityContext);

  if (context === undefined) {
    throw new Error('useInternetIdentity must be used within an InternetIdentityProvider');
  }

  return context;
}

export default useInternetIdentity;
