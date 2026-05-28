import React, { useEffect } from 'react';
import { X } from 'lucide-react';
import BankSummary from './BankSummary';
import BridgeCard from './BridgeCard';
import PendingQueueCard from './PendingQueueCard';
import BuyPPWidget from './Shenanigans/BuyPPWidget';
import BuySOLWidget from './Shenanigans/BuySOLWidget';
import { useWallet } from '../hooks/useWallet';

interface BankPageProps {
  onClose: () => void;
}

export default function BankPage({ onClose }: BankPageProps) {
  const { walletType } = useWallet();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="space-y-6 p-4">
      <div className="fixed inset-x-0 top-32 z-50 pointer-events-none">
        <div className="max-w-3xl mx-auto px-4 relative h-0">
          <button
            onClick={onClose}
            aria-label="Return to main screen"
            title="Return to main screen"
            className="absolute right-4 top-0 pointer-events-auto mc-bg-elev-2 hover:mc-bg-elev-3 mc-border-subtle border rounded-full p-2 shadow-lg transition-colors"
          >
            <X className="h-4 w-4 mc-text-primary" />
          </button>
        </div>
      </div>
      <BankSummary />
      <BridgeCard />
      {/* Quick-buy PP from PartyDEX — sits between bridge and pending queue
          so users who need more PP before depositing have it one click away
          without leaving the Bank page. */}
      <div className="max-w-md mx-auto w-full">
        {walletType === 'siws' ? <BuySOLWidget /> : <BuyPPWidget />}
      </div>
      <PendingQueueCard />
    </div>
  );
}
