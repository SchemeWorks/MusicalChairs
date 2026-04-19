import React, { useState } from 'react';
import { useAddBackerMoney, useICPBalance } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import BackerMoneyToast from './BackerMoneyToast';
import { AlertTriangle } from 'lucide-react';
import { ICP_TRANSFER_FEE, E8S_PER_ICP } from '../hooks/useLedger';

export default function AddBackerMoney() {
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState('');
  const [showToast, setShowToast] = useState(false);
  const [toastData, setToastData] = useState<{ amount: number; ponziPoints: number } | null>(null);
  const { data: icpBalance } = useICPBalance();
  const addBackerMoneyMutation = useAddBackerMoney();

  const walletBalance = icpBalance ?? 0;
  // Spendable = balance minus TWO ledger fees (icrc2_approve + icrc2_transfer_from).
  const DEPOSIT_FEE_RESERVE = ICP_TRANSFER_FEE * 2n;
  const walletBalanceE8s = BigInt(Math.round(walletBalance * Number(E8S_PER_ICP)));
  const walletSpendableE8s = walletBalanceE8s > DEPOSIT_FEE_RESERVE ? walletBalanceE8s - DEPOSIT_FEE_RESERVE : 0n;
  const minDeposit = 0.1;
  const depositAmount = parseFloat(amount) || 0;
  const depositAmountE8s = amount ? BigInt(Math.round(depositAmount * Number(E8S_PER_ICP))) : 0n;

  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const restricted = restrictToEightDecimals(e.target.value);
    const validation = validateICPInput(restricted);
    setAmount(restricted);
    setInputError(validation.error || '');
  };

  const handleDeposit = async () => {
    if (!amount || depositAmount < minDeposit || depositAmountE8s > walletSpendableE8s) return;
    const v = validateICPInput(amount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try {
      await addBackerMoneyMutation.mutateAsync(depositAmount);
      triggerConfetti();
      setToastData({ amount: depositAmount, ponziPoints: depositAmount * 4000 });
      setShowToast(true);
      setAmount('');
      setInputError('');
    } catch (error: any) {
      console.error('Backer money deposit failed:', error);
    }
  };

  const isAmountValid = depositAmount >= minDeposit && depositAmountE8s <= walletSpendableE8s && !inputError;

  return (
    <div>
      {showToast && toastData && (
        <BackerMoneyToast amount={toastData.amount} ponziPoints={toastData.ponziPoints} onClose={() => { setShowToast(false); setToastData(null); }} />
      )}

      <div className="space-y-3 max-w-sm">
        <div className="flex justify-between items-center">
          <span className="mc-label">Amount</span>
          <span className="text-xs mc-text-dim">Available: {formatICP(walletBalance)} ICP</span>
        </div>

        <div className="flex gap-2">
          <input
            type="number"
            value={amount}
            onChange={handleAmountChange}
            placeholder="Min: 0.1 ICP"
            min={minDeposit}
            step="0.00000001"
            className="mc-input flex-1 text-center"
          />
          <button
            onClick={handleDeposit}
            disabled={!amount || !isAmountValid || addBackerMoneyMutation.isPending || walletBalance < minDeposit || !!inputError}
            className="mc-btn-primary px-4 py-2 text-xs whitespace-nowrap"
          >
            {addBackerMoneyMutation.isPending ? 'Depositing...' : 'Deposit'}
          </button>
        </div>

        {depositAmount > 0 && !addBackerMoneyMutation.isPending && (
          <div className="space-y-1 text-xs">
            {inputError && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />{inputError}</div>}
            {!inputError && depositAmount < minDeposit && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum is {minDeposit} ICP</div>}
            {!inputError && depositAmountE8s > walletSpendableE8s && <div className="mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Insufficient balance (0.0002 ICP in ledger fees applies)</div>}
          </div>
        )}

        <div className="space-y-1">
          <p className="text-xs mc-text-green font-bold">Series A backers get a guaranteed* 24% return on their capital + 4,000 PP per ICP.</p>
          <p className="text-xs mc-text-muted italic">*(Returns not guaranteed)</p>
        </div>
      </div>
    </div>
  );
}
