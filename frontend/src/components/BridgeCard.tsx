import React, { useEffect, useState } from 'react';
import { toast } from 'sonner';
import {
  useGetPonziPoints,
  useAllowance,
  useApproveForDeposits,
  useDepositChips,
  useRequestCashOut,
  useGetMintConfig,
} from '../hooks/useQueries';
import { wholePpToUnits } from '../hooks/usePpLedger';

type Direction = 'deposit' | 'redeem';

export default function BridgeCard() {
  const { data: pp } = useGetPonziPoints();
  const { data: allowance } = useAllowance();
  const { data: mintConfig } = useGetMintConfig();
  const approve = useApproveForDeposits();
  const deposit = useDepositChips();
  const request = useRequestCashOut();

  const wallet = pp?.walletPoints ?? 0;
  const position = pp?.chipPoints ?? 0;
  const MIN_DEPOSIT = mintConfig ? Number(mintConfig.minDepositPp) : 0;

  const [direction, setDirection] = useState<Direction>('deposit');
  const [amount, setAmount] = useState<number>(0);
  const [amountTouched, setAmountTouched] = useState(false);

  useEffect(() => {
    if (!amountTouched && direction === 'deposit' && MIN_DEPOSIT > 0) {
      setAmount(MIN_DEPOSIT);
    }
  }, [MIN_DEPOSIT, direction, amountTouched]);

  const isFirstTime = position === 0 && direction === 'deposit';
  const hasAllowance =
    !!allowance && allowance.allowance >= wholePpToUnits(amount || MIN_DEPOSIT);

  const setDir = (next: Direction) => {
    setDirection(next);
    setAmount(next === 'deposit' ? MIN_DEPOSIT : 1);
  };

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

  const maxValue = direction === 'deposit' ? wallet : position;

  return (
    <section className="mc-card p-4">
      {isFirstTime && (
        <div className="mb-3 text-sm mc-text-muted">
          You have no Position yet. Deposit PP from your wallet to deploy capital into the fund.
        </div>
      )}

      {/* Segmented direction toggle */}
      <div className="flex rounded-lg bg-white/5 p-0.5 mb-4">
        {(['deposit', 'redeem'] as const).map((d) => {
          const active = direction === d;
          const label = d === 'deposit' ? 'Deposit' : 'Withdraw';
          const sub = d === 'deposit' ? 'Wallet → Position' : 'Position → Wallet';
          return (
            <button
              key={d}
              onClick={() => setDir(d)}
              className={`flex-1 py-2 px-3 rounded-md text-sm font-bold transition-all ${
                active
                  ? 'bg-[var(--mc-purple)]/25 mc-text-primary border border-[var(--mc-purple)]/30'
                  : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
              }`}
            >
              <div>{label}</div>
              <div className="text-[10px] mc-text-muted font-normal mt-0.5">{sub}</div>
            </button>
          );
        })}
      </div>

      {/* Amount input + MAX */}
      <div className="flex items-center gap-2">
        <input
          type="number"
          min={direction === 'deposit' ? MIN_DEPOSIT : 1}
          max={maxValue}
          value={amount}
          onChange={(e) => { setAmount(Number(e.target.value)); setAmountTouched(true); }}
          className="mc-input flex-1 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
        <span className="text-sm mc-text-muted">PP</span>
        <button
          onClick={() => setAmount(maxValue)}
          disabled={maxValue <= 0}
          className="mc-btn-secondary px-3 py-1.5 text-xs rounded-lg whitespace-nowrap disabled:opacity-50"
        >
          MAX
        </button>
      </div>

      {direction === 'deposit' ? (
        <>
          <p className="text-xs mc-text-muted mt-2">
            Deposited PP becomes your Position — deployable in gameplay, spendable on spells,
            movable by the protocol. Minimum deposit: {MIN_DEPOSIT.toLocaleString()} PP.
          </p>
          <button
            className="mc-btn mc-btn-primary mt-3"
            disabled={deposit.isPending || approve.isPending || !mintConfig || amount < MIN_DEPOSIT || amount > wallet}
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
