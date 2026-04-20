import React, { useState } from 'react';
import { toast } from 'sonner';
import {
  useGetPonziPoints,
  useApproveForDeposits,
  useDepositChips,
  useRequestCashOut,
  useClaimCashOut,
  usePendingCashOuts,
} from '../hooks/useQueries';

export default function ChipWallet() {
  const { data } = useGetPonziPoints();
  const { data: pending } = usePendingCashOuts();
  const approve = useApproveForDeposits();
  const deposit = useDepositChips();
  const request = useRequestCashOut();
  const claim = useClaimCashOut();

  const [depositAmount, setDepositAmount] = useState<number>(5000);
  const [cashOutAmount, setCashOutAmount] = useState<number>(5000);

  const wallet = data?.walletPoints ?? 0;
  const chips = data?.chipPoints ?? 0;

  return (
    <div className="space-y-6 p-4">
      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Bring chips to the table</h2>
        <p className="text-sm mc-text-muted mb-3">
          Wallet: <b>{wallet.toLocaleString()} PP</b> · Chips: <b>{chips.toLocaleString()} PP</b>
        </p>
        <input
          type="number"
          min={5000}
          value={depositAmount}
          onChange={(e) => setDepositAmount(Number(e.target.value))}
          className="mc-input w-40 mr-2"
        />
        <button
          className="mc-btn mc-btn-secondary mr-2"
          onClick={async () => {
            try {
              await approve.mutateAsync(depositAmount * 10);
              toast.success('Approved pp_ledger spend');
            } catch (e: any) {
              toast.error(e.message);
            }
          }}
        >
          Approve (one-time)
        </button>
        <button
          className="mc-btn mc-btn-primary"
          disabled={deposit.isPending}
          onClick={async () => {
            try {
              await deposit.mutateAsync(depositAmount);
              toast.success(`Deposited ${depositAmount} PP`);
            } catch (e: any) {
              toast.error(e.message);
            }
          }}
        >
          Deposit
        </button>
      </section>

      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Cash out</h2>
        <p className="text-sm mc-text-muted mb-3">
          7-day lockup. Chips stay exposed to spells during the window.
        </p>
        <input
          type="number"
          min={1}
          value={cashOutAmount}
          onChange={(e) => setCashOutAmount(Number(e.target.value))}
          className="mc-input w-40 mr-2"
        />
        <button
          className="mc-btn mc-btn-primary"
          disabled={request.isPending}
          onClick={async () => {
            try {
              await request.mutateAsync(cashOutAmount);
              toast.success('Cash-out queued');
            } catch (e: any) {
              toast.error(e.message);
            }
          }}
        >
          Request cash-out
        </button>
      </section>

      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-2">Pending cash-outs</h2>
        {!pending || pending.length === 0 ? (
          <p className="text-sm mc-text-muted">None.</p>
        ) : (
          <ul className="space-y-2">
            {pending.map((p) => {
              const now = new Date();
              const ready = !p.claimed && p.claimableAfter <= now;
              return (
                <li key={String(p.id)} className="flex items-center justify-between">
                  <span>
                    {p.amount} PP ·{' '}
                    {p.claimed
                      ? 'claimed'
                      : ready
                        ? 'ready'
                        : `unlocks ${p.claimableAfter.toLocaleString()}`}
                  </span>
                  {ready && !p.claimed && (
                    <button
                      className="mc-btn mc-btn-success"
                      onClick={async () => {
                        try {
                          await claim.mutateAsync(p.id);
                          toast.success('Claimed');
                        } catch (e: any) {
                          toast.error(e.message);
                        }
                      }}
                    >
                      Claim
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
