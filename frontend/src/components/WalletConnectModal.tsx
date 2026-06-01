import React, { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useWallet, WalletType } from '../hooks/useWallet';
import { X, Wallet, ExternalLink, Loader2, Check, AlertCircle } from 'lucide-react';

type ChainTag = 'solana' | 'icp';

interface WalletOption {
  type: WalletType;
  name: string;
  description: string;
  icon: string;
  // Which chains this wallet is wired up to in OUR app. Rendered as small
  // pills next to the wallet name so a Solana newcomer can tell at a glance
  // which option fits. Plug supports Solana inside its own UI but exposes no
  // dapp-facing Solana API, so it stays ICP-only here.
  chains: ChainTag[];
  installed?: boolean;
  installUrl?: string;
  // Comes-soon wallets render disabled with a tooltip and aren't connectable.
  comingSoon?: boolean;
}

interface WalletConnectModalProps {
  isOpen: boolean;
  onClose: () => void;
}

// Small chain-compat badge shown next to the wallet name.
function ChainPill({ chain }: { chain: ChainTag }) {
  const isSolana = chain === 'solana';
  // Solana brand purple #9945FF, ICP brand cyan #29abe2 — softened for dark bg.
  const styles = isSolana
    ? 'bg-[#9945FF]/15 text-[#c4a3ff] border-[#9945FF]/30'
    : 'bg-[#29abe2]/15 text-[#7dd3fc] border-[#29abe2]/30';
  return (
    <span
      className={`text-[10px] uppercase tracking-wider font-semibold px-1.5 py-0.5 rounded border ${styles}`}
    >
      {isSolana ? 'SOL' : 'ICP'}
    </span>
  );
}

export default function WalletConnectModal({ isOpen, onClose }: WalletConnectModalProps) {
  const { connect, isConnecting } = useWallet();
  const [selectedWallet, setSelectedWallet] = useState<WalletType | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Lock background scroll while the modal is open. The backdrop is a fixed
  // `backdrop-filter: blur()` overlay; scrolling the page behind it forces the
  // GPU to re-rasterize the blur every frame against the moving (and animated
  // mc-splash-bg) content underneath, which flickers. Locking scroll keeps the
  // backdrop static. The modal itself stays scrollable via its own overflow-y-auto
  // container. Mirrors the pattern in TrollboxPanel.
  useEffect(() => {
    if (!isOpen) return;
    const prevOverflow = document.body.style.overflow;
    const prevOverscroll = document.body.style.overscrollBehavior;
    document.body.style.overflow = 'hidden';
    document.body.style.overscrollBehavior = 'contain';
    return () => {
      document.body.style.overflow = prevOverflow;
      document.body.style.overscrollBehavior = prevOverscroll;
    };
  }, [isOpen]);

  const isPlugInstalled = typeof window !== 'undefined' && !!window.ic?.plug;
  // Phantom injects `window.phantom.solana.isPhantom` in modern installs; very
  // old installs also exposed `window.solana.isPhantom`. Accept either so a
  // user with the legacy global doesn't get a false "Not Installed" pill.
  const isPhantomInstalled = typeof window !== 'undefined' && (
    !!(window as any).phantom?.solana?.isPhantom ||
    !!(window as any).solana?.isPhantom
  );

  // Order is deliberate: Solana-first so a Phantom user lands on their wallet
  // immediately, then ICP-native II, then Plug (multi-chain wallet but
  // ICP-only in our integration — see WalletConnectModal copy), then OISY
  // grayed at the bottom while it's still Coming Soon.
  const walletOptions: WalletOption[] = [
    { type: 'siws', name: 'Phantom', description: 'The Solana standard. Allocations gratefully accepted.', icon: '/phantom-logo.svg', chains: ['solana'], installed: isPhantomInstalled, installUrl: 'https://phantom.com/' },
    { type: 'internet-identity', name: 'Internet Identity', description: 'The institutional choice. Clean, native, no questions asked.', icon: '/ii-logo.svg', chains: ['icp'], installed: true },
    { type: 'plug', name: 'Plug Wallet', description: 'For those who like to keep their keys close.', icon: '/plug-logo.svg', chains: ['icp'], installed: isPlugInstalled, installUrl: 'https://plugwallet.ooo/' },
    { type: 'oisy', name: 'OISY Wallet', description: 'Multi-chain. For the diversified degen.', icon: '/oisy-logo.svg', chains: ['solana', 'icp'], installed: true, comingSoon: true },
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

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-start justify-center p-4 pt-[15vh] overflow-y-auto">
      {/* Backdrop */}
      <div className="fixed inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />

      {/* Modal */}
      <div className="relative mc-dropdown w-full max-w-md mx-auto overflow-hidden flex-shrink-0">
        {/* Header */}
        <div className="flex items-center justify-between p-5 border-b border-white/10">
          <div className="flex items-center gap-2">
            <Wallet className="h-5 w-5 mc-text-purple" />
            <h2 className="font-display text-lg mc-text-primary">Step Right In</h2>
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
            // Asset icons start with '/'; anything else (e.g. Unicode '◎') renders as text.
            const isAssetIcon = wallet.icon.startsWith('/');

            if (wallet.comingSoon) {
              return (
                <div
                  key={wallet.type}
                  className="flex items-center gap-3 p-4 rounded-xl mc-card opacity-50 cursor-not-allowed"
                  title="Coming soon"
                >
                  {isAssetIcon ? (
                    <img src={wallet.icon} alt={wallet.name} className="h-8 w-8 object-contain grayscale" />
                  ) : (
                    <span aria-label={wallet.name} className="h-8 w-8 flex items-center justify-center text-2xl mc-text-muted">{wallet.icon}</span>
                  )}
                  <div className="flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-bold text-sm mc-text-primary">{wallet.name}</span>
                      {wallet.chains.map((c) => <ChainPill key={c} chain={c} />)}
                      <span className="text-xs px-2 py-0.5 bg-white/5 mc-text-muted rounded-full">Coming soon</span>
                    </div>
                    <p className="text-xs mc-text-muted mt-0.5">{wallet.description}</p>
                  </div>
                </div>
              );
            }

            if (isNotInstalled) {
              return (
                <a
                  key={wallet.type}
                  href={wallet.installUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-3 p-4 rounded-xl mc-card hover:border-white/20 transition-all"
                >
                  {isAssetIcon ? (
                    <img src={wallet.icon} alt={wallet.name} className="h-8 w-8 object-contain" />
                  ) : (
                    <span aria-label={wallet.name} className="h-8 w-8 flex items-center justify-center text-2xl mc-text-primary">{wallet.icon}</span>
                  )}
                  <div className="flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-bold text-sm mc-text-primary">{wallet.name}</span>
                      {wallet.chains.map((c) => <ChainPill key={c} chain={c} />)}
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
                    ? 'bg-[var(--mc-purple)]/15 border-[var(--mc-purple)]/40'
                    : 'bg-white/3 border-white/8 hover:bg-white/6 hover:border-white/15'
                } disabled:opacity-50`}
              >
                {isAssetIcon ? (
                  <img src={wallet.icon} alt={wallet.name} className="h-8 w-8 object-contain" />
                ) : (
                  <span aria-label={wallet.name} className="h-8 w-8 flex items-center justify-center text-2xl mc-text-primary">{wallet.icon}</span>
                )}
                <div className="flex-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-bold text-sm mc-text-primary">{wallet.name}</span>
                    {wallet.chains.map((c) => <ChainPill key={c} chain={c} />)}
                  </div>
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
            By connecting, you agree that this is a gambling game and that you&rsquo;re fine with that.
          </p>
        </div>
      </div>
    </div>,
    document.body
  );
}
