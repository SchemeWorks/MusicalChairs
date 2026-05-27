import React from 'react';
import { Landmark } from 'lucide-react';
import { usePendingCashOuts } from '../hooks/useQueries';

interface BankNavLinkProps {
  onClick: () => void;
}

export default function BankNavLink({ onClick }: BankNavLinkProps) {
  const { data: pending } = usePendingCashOuts();

  const now = Date.now();
  const claimable = (pending ?? []).filter(
    (p) => p.claimableAfter.getTime() <= now,
  ).length;

  return (
    <button
      onClick={onClick}
      aria-label="Bank"
      title="Bank — deposit and withdraw"
      className="relative hidden sm:flex items-center gap-1.5 px-3 py-2 rounded-full text-xs sm:text-sm font-display mc-text-dim hover:mc-text-primary hover:bg-white/5 transition-all border border-white/15 hover:border-white/30"
    >
      <Landmark className="h-4 w-4" />
      <span className="hidden sm:inline">Bank</span>
      {claimable > 0 && (
        <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 rounded-full bg-green-500 text-white text-[10px] font-bold flex items-center justify-center">
          {claimable}
        </span>
      )}
    </button>
  );
}
