import React, { useState } from 'react';
import { useWallet, WalletType } from '../hooks/useWallet';
import { X, Wallet, ExternalLink, Loader2, Check, AlertCircle } from 'lucide-react';

interface WalletOption {
  type: WalletType;
  name: string;
  description: string;
  iconEmoji: string;
  installed?: boolean;
  installUrl?: string;
}

interface WalletConnectModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function WalletConnectModal({ isOpen, onClose }: WalletConnectModalProps) {
  const { connect, isConnecting } = useWallet();
  const [selectedWallet, setSelectedWallet] = useState<WalletType | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isPlugInstalled = typeof window !== 'undefined' && !!window.ic?.plug;

  const walletOptions: WalletOption[] = [
    { type: 'internet-identity', name: 'Internet Identity', description: 'ICP native identity', iconEmoji: '🔐', installed: true },
    { type: 'plug', name: 'Plug Wallet', description: 'Browser wallet extension', iconEmoji: '🔌', installed: isPlugInstalled, installUrl: 'https://plugwallet.ooo/' },
    { type: 'oisy', name: 'OISY Wallet', description: 'Multi-chain wallet via II', iconEmoji: '✨', installed: true },
  ];

  const handleConnect = async (walletType: WalletType) => {
    setError(null);
    setSelectedWallet(walletType);
    try {
      await connect(walletType);
      onClose();
    } catch (err: any) {
      console.error('Connection error:', err);
      setError(err.message || 'Failed to connect wallet');
      setSelectedWallet(null);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />

      {/* Modal */}
      <div className="relative mc-dropdown w-full max-w-md mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between p-5 border-b border-white/10">
          <div className="flex items-center gap-2">
            <Wallet className="h-5 w-5 mc-text-purple" />
            <h2 className="font-display text-lg mc-text-primary">Connect Wallet</h2>
          </div>
          <button onClick={onClose} className="p-2 rounded-lg hover:bg-white/5 transition-colors">
            <X className="h-4 w-4 mc-text-muted" />
          </button>
        </div>

        {/* Options */}
        <div className="p-5 space-y-2">
          {walletOptions.map(wallet => {
            const isSelected = selectedWallet === wallet.type;
            const isThisConnecting = isConnecting && isSelected;
            const isNotInstalled = !wallet.installed && wallet.installUrl;

            if (isNotInstalled) {
              return (
                <a
                  key={wallet.type}
                  href={wallet.installUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-3 p-4 rounded-xl mc-card hover:border-white/20 transition-all"
                >
                  <span className="text-2xl">{wallet.iconEmoji}</span>
                  <div className="flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-bold text-sm mc-text-primary">{wallet.name}</span>
                      <span className="text-xs px-2 py-0.5 bg-yellow-500/20 mc-text-gold rounded-full">Not Installed</span>
                    </div>
                    <p className="text-xs mc-text-muted mt-0.5">{wallet.description}</p>
                  </div>
                  <ExternalLink className="h-4 w-4 mc-text-muted" />
                </a>
              );
            }

            return (
              <button
                key={wallet.type}
                onClick={() => handleConnect(wallet.type)}
                disabled={isConnecting}
                className={`flex items-center gap-3 p-4 rounded-xl w-full text-left transition-all border ${
                  isSelected
                    ? 'bg-purple-500/15 border-purple-400/40'
                    : 'bg-white/3 border-white/8 hover:bg-white/6 hover:border-white/15'
                } disabled:opacity-50`}
              >
                <span className="text-2xl">{wallet.iconEmoji}</span>
                <div className="flex-1">
                  <span className="font-bold text-sm mc-text-primary">{wallet.name}</span>
                  <p className="text-xs mc-text-muted mt-0.5">{wallet.description}</p>
                </div>
                {isThisConnecting ? (
                  <Loader2 className="h-4 w-4 mc-text-purple animate-spin" />
                ) : isSelected ? (
                  <Check className="h-4 w-4 mc-text-green" />
                ) : null}
              </button>
            );
          })}
        </div>

        {/* Error */}
        {error && (
          <div className="mx-5 mb-5 mc-status-red p-3 flex items-start gap-2 text-xs">
            <AlertCircle className="h-4 w-4 mc-text-danger flex-shrink-0 mt-0.5" />
            <span>{error}</span>
          </div>
        )}

        {/* Footer */}
        <div className="px-5 pb-5">
          <p className="text-xs mc-text-muted text-center font-accent italic">
            By connecting, you acknowledge this is a gambling game.
          </p>
        </div>
      </div>
    </div>
  );
}
