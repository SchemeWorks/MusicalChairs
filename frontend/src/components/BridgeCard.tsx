import React, { useState } from 'react';
import { toast } from 'sonner';
import { ArrowDownUp } from 'lucide-react';
import {
  useGetPonziPoints,
  useAllowance,
  useApproveForDeposits,
  useDepositChips,
  useRequestCashOut,
} from '../hooks/useQueries';
import { wholePpToUnits } from '../hooks/usePpLedger';

type Direction = 'deposit' | 'redeem';

const MIN_DEPOSIT = 5000;

export default function BridgeCard() {
  const { data: pp } = useGetPonziPoints();
  const { data: allowance } = useAllowance();
  const approve = useApproveForDeposits();
  const deposit = useDepositChips();
  const request = useRequestCashOut();

  const wallet = pp?.walletPoints ?? 0;
  const position = pp?.chipPoints ?? 0;

  const [direction, setDirection] = useState<Direction>('deposit');
  const [amount, setAmount] = useState<number>(MIN_DEPOSIT);

  const isFirstTime = position === 0 && direction === 'deposit';
  const hasAllowance =
    !!allowance && allowance.allowance >= wholePpToUnits(amount || MIN_DEPOSIT);

  const flip = () =>
    setDirection((d) => {
      const next: Direction = d === 'deposit' ? 'redeem' : 'deposit';
      setAmount(next === 'deposit' ? MIN_DEPOSIT : 1);
      return next;
    });

  const runDeposit = async () => {
    try {
      if (!hasAllowance) {
        await approve.mutateAsync(undefined);
        toast.success('Allowance approved');
      }
      await deposit.mutateAsync(amount);
      toast.success(`Deposited ${amount.toLocaleString()} PP`);
    } catch (e: any) {
      toast.error(e.message);
    }
  };

  const runRedeem = async () => {
    try {
      await request.mutateAsync(amount);
      toast.success('Redemption queued');
    } catch (e: any) {
      toast.error(e.message);
    }
  };

  return (
    <section className="mc-card p-4">
      {isFirstTime && (
        <div className="mb-3 text-sm mc-text-muted">
          You have no Position yet. Deposit PP from your wallet to deploy capital into the fund.
        </div>
      )}

      <div className="flex items-center justify-between mb-3">
        <div className="text-sm">
          <span className="mc-text-muted">From</span>{' '}
          <b>{direction === 'deposit' ? 'Wallet' : 'Position'}</b>
          <span className="mx-2 mc-text-muted">→</span>
          <span className="mc-text-muted">To</span>{' '}
          <b>{direction === 'deposit' ? 'Position' : 'Wallet'}</b>
        </div>
        <button
          onClick={flip}
          className="mc-btn-secondary px-2 py-1 text-xs flex items-center gap-1"
          aria-label="Flip direction"
        >
          <ArrowDownUp className="h-3 w-3" /> Flip
        </button>
      </div>

      <input
        type="number"
        min={direction === 'deposit' ? MIN_DEPOSIT : 1}
        max={direction === 'deposit' ? wallet : position}
        value={amount}
        onChange={(e) => setAmount(Number(e.target.value))}
        className="mc-input w-40 mr-2"
      />
      <span className="text-sm mc-text-muted">PP</span>

      {direction === 'deposit' ? (
        <>
          <p className="text-xs mc-text-muted mt-2">
            Deposited PP becomes your Position — deployable in gameplay, spendable on spells,
            movable by the protocol. Minimum deposit: {MIN_DEPOSIT.toLocaleString()} PP.
          </p>
          <button
            className="mc-btn mc-btn-primary mt-3"
            disabled={deposit.isPending || approve.isPending || amount < MIN_DEPOSIT || amount > wallet}
            onClick={runDeposit}
          >
            {hasAllowance ? 'Deposit' : 'Approve & deposit'}
          </button>
        </>
      ) : (
        <>
          <div className="mt-3 p-3 rounded border border-amber-500/30 bg-amber-500/5 text-sm">
            <b>7-day lockup.</b> Queued PP remains in your Position and stays exposed to spells
            during the lockup window.
          </div>
          <button
            className="mc-btn mc-btn-primary mt-3"
            disabled={request.isPending || amount < 1 || amount > position}
            onClick={runRedeem}
          >
            Queue redemption
          </button>
        </>
      )}
    </section>
  );
}
