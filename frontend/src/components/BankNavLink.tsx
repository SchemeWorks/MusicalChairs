import React from 'react';
import { Landmark } from 'lucide-react';
import { usePendingCashOuts } from '../hooks/useQueries';

interface BankNavLinkProps {
  onClick: () => void;
}

export default function BankNavLink({ onClick }: BankNavLinkProps) {
  const { data: pending } = usePendingCashOuts();

  // Badge count = entries whose claimableAfter is in the past and not yet claimed/cancelled.
  // (usePendingCashOuts already filters out claimed; cancelled is filtered by backend.)
  const now = Date.now();
  const claimable = (pending ?? []).filter(
    (p) => !p.claimed && p.claimableAfter.getTime() <= now,
  ).length;

  return (
    <button
      onClick={onClick}
      className="relative flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-display mc-text-dim hover:mc-text-primary hover:bg-white/5 transition-all border border-white/10 hover:border-white/20"
    >
      <Landmark className="h-4 w-4" />
      <span>Bank</span>
      {claimable > 0 && (
        <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 rounded-full bg-green-500 text-white text-[10px] font-bold flex items-center justify-center">
          {claimable}
        </span>
      )}
    </button>
  );
}
