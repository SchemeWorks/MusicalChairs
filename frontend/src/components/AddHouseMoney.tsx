import React, { useState } from 'react';
import { useAddDealerMoney, useGetInternalWalletBalance } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import HouseMoneyToast from './HouseMoneyToast';

export default function AddHouseMoney() {
  const [amount, setAmount] = useState('');
  const [inputError, setInputError] = useState<string>('');
  const [showToast, setShowToast] = useState(false);
  const [toastData, setToastData] = useState<{ amount: number; ponziPoints: number } | null>(null);
  const { data: balanceData } = useGetInternalWalletBalance();
  const addDealerMoneyMutation = useAddDealerMoney();

  const walletBalance = balanceData?.internalBalance || 0;
  const minDeposit = 0.1;
  const depositAmount = parseFloat(amount) || 0;

  // Handle amount input with 8 decimal place validation
  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const input = e.target.value;
    const restrictedInput = restrictToEightDecimals(input);
    const validation = validateICPInput(restrictedInput);
    
    setAmount(restrictedInput);
    setInputError(validation.error || '');
  };

  const handleDeposit = async () => {
    if (!amount || depositAmount < minDeposit || depositAmount > walletBalance) return;
    
    // Final validation check
    const validation = validateICPInput(amount);
    if (!validation.isValid) {
      setInputError(validation.error || '');
      return;
    }

    try {
      await addDealerMoneyMutation.mutateAsync(depositAmount);
      
      // Trigger confetti celebration
      triggerConfetti();
      
      // Calculate Ponzi Points (4,000 per ICP)
      const ponziPoints = depositAmount * 4000;
      
      // Show custom toast notification
      setToastData({ amount: depositAmount, ponziPoints });
      setShowToast(true);
      
      setAmount('');
      setInputError('');
    } catch (error: any) {
      console.error('Dealer money deposit failed:', error);
      alert(`❌ Failed to deposit house money: ${error?.message || 'Unknown error'}`);
    }
  };

  const isAmountValid = depositAmount >= minDeposit && depositAmount <= walletBalance && !inputError;
  const hasValidAmount = amount && isAmountValid;

  return (
    <div>
      {/* Toast notification */}
      {showToast && toastData && (
        <HouseMoneyToast
          amount={toastData.amount}
          ponziPoints={toastData.ponziPoints}
          onClose={() => {
            setShowToast(false);
            setToastData(null);
          }}
        />
      )}
      
      <div className="text-center mb-4">
        <div className="text-sm font-bold text-white text-with-backdrop mb-2">💰 Own The Casino 💰</div>
      </div>
      
      <div className="space-y-3">
        {/* Amount label and available balance on same line */}
        <div className="flex justify-between items-center">
          <Label className="text-sm font-bold text-white text-with-backdrop">Amount</Label>
          <span className="text-sm text-white">Available: {formatICP(walletBalance)} ICP</span>
        </div>
        
        {/* Input and Button side by side */}
        <div className="flex gap-2">
          <Input
            type="number"
            value={amount}
            onChange={handleAmountChange}
            placeholder="Min: 0.1 ICP"
            min={minDeposit}
            step="0.00000001"
            className="flex-1 text-center font-bold bg-white text-black placeholder:text-gray-500"
          />
          <button
            onClick={handleDeposit}
            disabled={
              !amount || 
              !isAmountValid ||
              addDealerMoneyMutation.isPending ||
              walletBalance < minDeposit ||
              !!inputError
            }
            className={`px-6 py-2 text-sm font-black rounded-xl transition-all duration-300 ease-in-out whitespace-nowrap ${
              !amount || 
              !isAmountValid ||
              addDealerMoneyMutation.isPending ||
              walletBalance < minDeposit ||
              !!inputError
                ? 'start-game-button-disabled'
                : hasValidAmount ? 'start-game-button-active-with-glow' : 'start-game-button-active'
            }`}
          >
            {addDealerMoneyMutation.isPending ? (
              <div className="flex items-center justify-center">
                <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                Depositing...
              </div>
            ) : walletBalance < minDeposit ? (
              '💰 Fund Wallet First'
            ) : inputError ? (
              '❌ Fix Input Error'
            ) : (
              'Deposit House Money'
            )}
          </button>
        </div>

        {/* Error messages */}
        {depositAmount > 0 && (
          <div className="space-y-1">
            {inputError && (
              <div className="text-white text-sm">
                ⚠️ {inputError}
              </div>
            )}
            {!inputError && depositAmount < minDeposit && (
              <div className="text-white text-sm">
                ⚠️ Minimum deposit is {minDeposit} ICP
              </div>
            )}
            {!inputError && depositAmount > walletBalance && (
              <div className="text-white text-sm">
                ⚠️ Insufficient balance. Please fund your Musical Chairs Wallet first.
              </div>
            )}
          </div>
        )}

        {/* Tagline and disclaimer */}
        <div className="text-center space-y-1">
          <div className="text-sm font-bold text-white">
            Earn a guaranteed 12% return* + 4000 Ponzi Points per ICP deposited!
          </div>
          <div className="text-xs text-white/80">
            *(Returns not guaranteed)
          </div>
        </div>
      </div>
    </div>
  );
}

