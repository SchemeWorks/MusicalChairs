import React, { useEffect, useState } from 'react';
import { formatICP } from '../lib/formatICP';

interface HouseMoneyToastProps {
  amount: number;
  ponziPoints: number;
  onClose: () => void;
}

const charlesQuips = [
  "The house always appreciates a generous patron.",
  "Smart money. Or at least, money.",
  "Every dollar helps. Mostly me.",
  "You just made the pot a little heavier. Charles approves.",
  "That's the spirit. Keep it coming.",
];

export default function HouseMoneyToast({ amount, ponziPoints, onClose }: HouseMoneyToastProps) {
  const [isVisible, setIsVisible] = useState(false);
  const [isExiting, setIsExiting] = useState(false);
  const [quip] = useState(() => charlesQuips[Math.floor(Math.random() * charlesQuips.length)]);

  useEffect(() => {
    setTimeout(() => setIsVisible(true), 10);
    const timer = setTimeout(() => handleClose(), 4000);
    return () => clearTimeout(timer);
  }, []);

  const handleClose = () => {
    setIsExiting(true);
    setTimeout(() => onClose(), 300);
  };

  return (
    <div
      className={`fixed top-8 left-1/2 -translate-x-1/2 z-[9999] transition-all duration-300 ${
        isVisible && !isExiting ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-4'
      }`}
    >
      <div className="mc-toast text-center">
        <div className="font-display text-xl mc-text-primary mb-2">You're In</div>
        <p className="text-sm mc-text-dim">
          <span className="mc-toast-accent">{formatICP(amount)} ICP</span> added to the house
          {ponziPoints > 0 && <> &middot; <span className="mc-toast-accent">{ponziPoints.toLocaleString()} PP</span> earned</>}
        </p>
        <p className="text-xs mc-text-muted mt-2 font-accent italic">&ldquo;{quip}&rdquo; &mdash; Charles</p>
        <button onClick={handleClose} className="mc-btn-primary px-6 py-2 rounded-full text-sm mt-4 w-full">
          Nice
        </button>
      </div>
    </div>
  );
}
