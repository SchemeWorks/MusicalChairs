import React, { useState } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useQueryClient } from '@tanstack/react-query';
import WalletConnectModal from './WalletConnectModal';
import { Wallet, LogOut, Loader2 } from 'lucide-react';

export default function LoginButton() {
  const { isConnected, isConnecting, isInitializing, disconnect, walletType } = useWallet();
  const queryClient = useQueryClient();
  const [isModalOpen, setIsModalOpen] = useState(false);

  const handleClick = async () => {
    if (isConnected) {
      await disconnect();
      queryClient.clear();
    } else {
      setIsModalOpen(true);
    }
  };

  // Get wallet display name
  const getWalletName = () => {
    switch (walletType) {
      case 'internet-identity':
        return 'II';
      case 'plug':
        return 'Plug';
      case 'oisy':
        return 'OISY';
      default:
        return '';
    }
  };

  // Show loading state during initialization
  if (isInitializing) {
    return (
      <button
        disabled
        className="px-6 py-3 rounded-full bg-purple-700/50 text-purple-300 font-black text-lg shadow-lg flex items-center gap-2"
      >
        <Loader2 className="w-5 h-5 animate-spin" />
        Loading...
      </button>
    );
  }

  return (
    <>
      <button
        onClick={handleClick}
        disabled={isConnecting}
        className={`px-6 py-3 rounded-full transition-all font-black text-lg hover:scale-105 shadow-lg flex items-center gap-2 ${
          isConnected
            ? 'bg-gradient-to-r from-red-500 to-red-600 hover:from-red-600 hover:to-red-700 text-white'
            : 'bg-gradient-to-r from-green-500 to-blue-500 hover:from-green-600 hover:to-blue-600 text-white'
        } disabled:opacity-50 disabled:hover:scale-100`}
      >
        {isConnecting ? (
          <>
            <Loader2 className="w-5 h-5 animate-spin" />
            Connecting...
          </>
        ) : isConnected ? (
          <>
            <LogOut className="w-5 h-5" />
            Disconnect {getWalletName()}
          </>
        ) : (
          <>
            <Wallet className="w-5 h-5" />
            Connect Wallet
          </>
        )}
      </button>

      <WalletConnectModal 
        isOpen={isModalOpen} 
        onClose={() => setIsModalOpen(false)} 
      />
    </>
  );
}
