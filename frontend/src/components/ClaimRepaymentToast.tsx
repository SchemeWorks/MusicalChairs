import React, { useEffect, useState } from 'react';
import { formatICP } from '../lib/formatICP';
import { Banknote } from 'lucide-react';

interface ClaimRepaymentToastProps {
  amount: number;
  onClose: () => void;
}

const charlesQuips = [
  "Cha-ching. The fund pays its loyal LPs.",
  "Distributions hit different.",
  "Charles tips his hat. You played that one right.",
  "That's what passive income looks like, baby.",
  "Repayment received. Reinvest? You know you want to.",
];

export default function ClaimRepaymentToast({ amount, onClose }: ClaimRepaymentToastProps) {
  const [isVisible, setIsVisible] = useState(false);
  const [isExiting, setIsExiting] = useState(false);
  const [quip] = useState(() => charlesQuips[Math.floor(Math.random() * charlesQuips.length)]);

  useEffect(() => {
    setTimeout(() => setIsVisible(true), 10);
  }, []);

  const handleClose = () => {
    setIsExiting(true);
    setTimeout(() => onClose(), 300);
  };

  return (
    <div
      className={`fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999] transition-all duration-300 ${
        isVisible && !isExiting ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-4'
      }`}
    >
      <div className="mc-toast text-center">
        <div className="flex items-center justify-center gap-2 mb-2">
          <Banknote className="h-5 w-5 mc-text-green" />
          <div className="font-display text-xl mc-text-green mc-glow-green">Distribution Received</div>
          <Banknote className="h-5 w-5 mc-text-green" />
        </div>
        <p className="text-sm mc-text-dim">
          <span className="mc-toast-accent">{formatICP(amount)} ICP</span> claimed from your backer entitlement
        </p>
        <p className="text-xs mc-text-muted mt-2 font-accent italic">{quip}</p>
        <button onClick={handleClose} className="mc-btn-primary px-6 py-2 rounded-full text-sm mt-4 w-full">
          Cash Money
        </button>
      </div>
    </div>
  );
}
