import React, { useState } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useQueryClient } from '@tanstack/react-query';
import WalletConnectModal from './WalletConnectModal';
import { Wallet, LogOut, Loader2 } from 'lucide-react';

export default function LoginButton({ compact }: { compact?: boolean } = {}) {
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

  const walletName = walletType === 'internet-identity' ? 'II' : walletType === 'plug' ? 'Plug' : walletType === 'oisy' ? 'OISY' : '';

  if (isInitializing) {
    return (
      <button disabled className="mc-btn-secondary px-5 py-2 rounded-full flex items-center gap-2 opacity-50">
        <Loader2 className="h-4 w-4 animate-spin" /> Loading...
      </button>
    );
  }

  return (
    <>
      <button
        onClick={handleClick}
        disabled={isConnecting}
        className={`px-5 py-2.5 rounded-full font-bold text-sm transition-all flex items-center gap-2 ${
          isConnected ? 'mc-btn-danger' : 'mc-btn-primary'
        } disabled:opacity-50`}
      >
        {isConnecting ? (
          <><Loader2 className="h-4 w-4 animate-spin" /> Connecting...</>
        ) : isConnected ? (
          <><LogOut className="h-4 w-4" /> Disconnect {walletName}</>
        ) : (
          <><Wallet className="h-4 w-4" /> {compact ? 'Connect' : 'Connect Wallet'}</>
        )}
      </button>

      <WalletConnectModal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} />
    </>
  );
}
