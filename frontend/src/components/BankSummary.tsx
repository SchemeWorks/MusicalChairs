import React from 'react';
import { toast } from 'sonner';
import { HelpCircle } from 'lucide-react';
import { useGetPonziPoints, useAllowance, useRevokeAllowance, useChipsTaxCredit } from '../hooks/useQueries';
import { ppUnitsToWhole } from '../hooks/usePpLedger';

const UNLIMITED = 18_446_744_073_709_551_615n;

export default function BankSummary() {
  const { data: pp } = useGetPonziPoints();
  const { data: allowance } = useAllowance();
  const { data: taxCreditUnits } = useChipsTaxCredit();
  const revoke = useRevokeAllowance();

  const wallet = pp?.walletPoints ?? 0;
  const position = pp?.chipPoints ?? 0;
  const taxCreditPP = taxCreditUnits ? ppUnitsToWhole(taxCreditUnits) : 0;
  const allowanceDisplay = (() => {
    if (!allowance) return '—';
    if (allowance.allowance === 0n) return 'none';
    if (allowance.allowance >= UNLIMITED / 2n) return '∞';
    return `${ppUnitsToWhole(allowance.allowance).toLocaleString()} PP`;
  })();

  return (
    <section className="mc-card p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-4">
        <div className="flex gap-8">
          <div>
            <div className="mc-label">Wallet</div>
            <div className="text-2xl font-bold mc-text-primary">
              {wallet.toLocaleString()} <span className="text-sm mc-text-muted">PP</span>
            </div>
          </div>
          <div>
            <div className="mc-label">Position</div>
            <div className="text-2xl font-bold mc-text-green">
              {position.toLocaleString()} <span className="text-sm mc-text-muted">PP</span>
            </div>
          </div>
        </div>
        <div className="text-right">
          <div className="mc-label flex items-center justify-end gap-1">
            <span>Allowance</span>
            <span
              title="The amount of PP the protocol is pre-approved to pull from your Wallet when you deposit. ∞ means you've approved unlimited deposits — convenient, but you can revoke any time. Revoking forces an Approve step on the next deposit."
              className="cursor-help inline-flex"
              aria-label="What is Allowance?"
            >
              <HelpCircle className="h-3 w-3 mc-text-muted" />
            </span>
          </div>
          <div className="text-sm">
            <span className="mc-text-primary">{allowanceDisplay}</span>
            {allowance && allowance.allowance > 0n && (
              <button
                className="ml-2 mc-btn-secondary text-xs px-2 py-0.5"
                disabled={revoke.isPending}
                onClick={async () => {
                  try {
                    await revoke.mutateAsync();
                    toast.success('Allowance revoked');
                  } catch (e: any) {
                    toast.error(e.message);
                  }
                }}
              >
                Revoke
              </button>
            )}
          </div>
        </div>
      </div>
      <p className="text-xs mc-text-muted mt-3">
        PP earned from deposits and gameplay lands in your Position within ~10 seconds.
      </p>
      {/* Tax-free deposit credit — accrued from chip withdrawals, consumed by
          deposits. Anything deposited beyond this kicks 10% up the cascade. */}
      <div className="mt-3 pt-3 border-t mc-border-subtle flex items-center justify-between gap-2">
        <div className="flex items-center gap-1">
          <span className="mc-label">Tax-free deposit</span>
          <span
            title="When you deposit PP to your Position, 10% kicks up to your upline — same as the cascade on minted PP. PP you previously withdrew from your Position is exempt up to the amount you withdrew (you already paid the tax once). Bring more than your credit and the excess pays 10%."
            className="cursor-help inline-flex"
            aria-label="What is tax-free deposit?"
          >
            <HelpCircle className="h-3 w-3 mc-text-muted" />
          </span>
        </div>
        <div className="text-sm mc-text-gold font-bold">
          {taxCreditPP > 0 ? `${taxCreditPP.toLocaleString()} PP` : 'none'}
        </div>
      </div>
    </section>
  );
}
