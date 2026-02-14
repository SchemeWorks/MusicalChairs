import React, { useState } from 'react';
import { useAddDealerMoney, useGetInternalWalletBalance } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import HouseMoneyToast from './HouseMoneyToast';

export default function AddHouseMoney() {
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState('');
  const [showToast, setShowToast] = useState(false);
  const [toastData, setToastData] = useState<{ amount: number; ponziPoints: number } | null>(null);
  const { data: balanceData } = useGetInternalWalletBalance();
  const addDealerMoneyMutation = useAddDealerMoney();

  const walletBalance = balanceData?.internalBalance || 0;
  const minDeposit = 0.1;
  const depositAmount = parseFloat(amount) || 0;

  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const restricted = restrictToEightDecimals(e.target.value);
    const validation = validateICPInput(restricted);
    setAmount(restricted);
    setInputError(validation.error || '');
  };

  const handleDeposit = async () => {
    if (!amount || depositAmount < minDeposit || depositAmount > walletBalance) return;
    const v = validateICPInput(amount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try {
      await addDealerMoneyMutation.mutateAsync(depositAmount);
      triggerConfetti();
      setToastData({ amount: depositAmount, ponziPoints: depositAmount * 4000 });
      setShowToast(true);
      setAmount('');
      setInputError('');
    } catch (error: any) {
      console.error('Dealer money deposit failed:', error);
    }
  };

  const isAmountValid = depositAmount >= minDeposit && depositAmount <= walletBalance && !inputError;

  return (
    <div>
      {showToast && toastData && (
        <HouseMoneyToast amount={toastData.amount} ponziPoints={toastData.ponziPoints} onClose={() => { setShowToast(false); setToastData(null); }} />
      )}

      <div className="text-center mb-3">
        <span className="font-display text-sm mc-text-gold">Own The Casino</span>
      </div>

      <div className="space-y-3">
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
            disabled={!amount || !isAmountValid || addDealerMoneyMutation.isPending || walletBalance < minDeposit || !!inputError}
            className="mc-btn-primary px-4 py-2 text-xs whitespace-nowrap"
          >
            {addDealerMoneyMutation.isPending ? 'Depositing...' : 'Deposit'}
          </button>
        </div>

        {depositAmount > 0 && (
          <div className="space-y-1 text-xs">
            {inputError && <div className="mc-text-danger">⚠️ {inputError}</div>}
            {!inputError && depositAmount < minDeposit && <div className="mc-text-danger">⚠️ Minimum is {minDeposit} ICP</div>}
            {!inputError && depositAmount > walletBalance && <div className="mc-text-danger">⚠️ Insufficient balance</div>}
          </div>
        )}

        <div className="text-center space-y-1">
          <p className="text-xs mc-text-green font-bold">Earn a guaranteed 12% return* + 4,000 PP per ICP!</p>
          <p className="text-xs mc-text-muted italic">*(Returns not guaranteed)</p>
        </div>
      </div>
    </div>
  );
}
