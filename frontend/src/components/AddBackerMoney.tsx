import React, { useState } from 'react';
import { useAddBackerMoney, useICPBalance } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import BackerMoneyToast from './BackerMoneyToast';
import { AlertTriangle, BarChart3, TrendingUp } from 'lucide-react';
import { ICP_TRANSFER_FEE, E8S_PER_ICP } from '../hooks/useLedger';
import { UPSTREAM_BACKER_BONUS } from '../lib/gameConstants';

const PP_PER_ICP = 4000;

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

  const setFromE8s = (e8s: bigint) => {
    const wholePart = (e8s / 100_000_000n).toString();
    const fracPart = (e8s % 100_000_000n).toString().padStart(8, '0').replace(/0+$/, '');
    setAmount(fracPart ? `${wholePart}.${fracPart}` : wholePart);
    setInputError('');
  };

  const handleDeposit = async () => {
    if (!amount || depositAmount < minDeposit || depositAmountE8s > walletSpendableE8s) return;
    const v = validateICPInput(amount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try {
      await addBackerMoneyMutation.mutateAsync(depositAmount);
      triggerConfetti();
      setToastData({ amount: depositAmount, ponziPoints: depositAmount * PP_PER_ICP });
      setShowToast(true);
      setAmount('');
      setInputError('');
    } catch (error: any) {
      console.error('Backer money deposit failed:', error);
    }
  };

  const isAmountValid = depositAmount >= minDeposit && depositAmountE8s <= walletSpendableE8s && !inputError;
  const canInteract = walletBalance >= minDeposit;

  // Seed round has no fees — net deposit equals gross.
  const netDeposit = depositAmount;
  const expectedReturn = depositAmount * (1 + UPSTREAM_BACKER_BONUS);
  const ponziPoints = depositAmount * PP_PER_ICP;

  return (
    <>
      {showToast && toastData && (
        <BackerMoneyToast amount={toastData.amount} ponziPoints={toastData.ponziPoints} onClose={() => { setShowToast(false); setToastData(null); }} />
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Left: copy + amount + deposit */}
        <div className="space-y-3">
          <p className="text-sm mc-text-dim">
            Back the next generation of yield innovation. Earn your entitlement — you've earned it.
          </p>

          <div className="flex justify-between items-center">
            <span className="mc-label">Amount</span>
            <span className="text-xs mc-text-dim">Available: {formatICP(walletBalance)} ICP</span>
          </div>

          <div className="flex gap-2">
            <button
              onClick={() => setFromE8s(walletSpendableE8s / 2n)}
              disabled={!canInteract}
              className={`mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap ${
                !canInteract ? 'opacity-50 cursor-not-allowed' : ''
              }`}
            >MID</button>
            <input
              type="number"
              value={amount}
              onChange={handleAmountChange}
              placeholder={`Min: ${minDeposit} ICP`}
              min={minDeposit}
              step="0.00000001"
              className="mc-input flex-1 text-center [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
            />
            <button
              onClick={() => setFromE8s(walletSpendableE8s)}
              disabled={!canInteract}
              className={`mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap ${
                !canInteract ? 'opacity-50 cursor-not-allowed' : ''
              }`}
            >MAX</button>
          </div>

          <button
            onClick={handleDeposit}
            disabled={!amount || !isAmountValid || addBackerMoneyMutation.isPending || !canInteract || !!inputError}
            className="mc-btn-primary w-full py-2 text-sm font-bold"
          >
            {addBackerMoneyMutation.isPending ? 'Depositing...' : 'Deposit'}
          </button>

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

        {/* Right: instant calculator + warning */}
        <div className="space-y-3">
          {depositAmount > 0 ? (
            <div>
              <div className="text-center mb-3">
                <span className="text-xs font-bold mc-text-primary">Expected Series A Return</span>
              </div>
              <div className="mc-card p-4 space-y-3">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="mc-label">Total Payout</div>
                    <div className="text-xl font-bold mc-text-green">{formatICP(expectedReturn)} ICP</div>
                    <div className="text-xs mc-text-green opacity-70">24% bonus</div>
                  </div>
                  <div className="text-right">
                    <div className="mc-label">Ponzi Points</div>
                    <div className="text-xl font-bold mc-text-purple mc-glow-purple">{Math.round(ponziPoints).toLocaleString()}</div>
                    <div className="text-xs mc-text-purple opacity-70">4,000 / ICP</div>
                  </div>
                </div>
                <div className="border-t border-white/10 pt-3 space-y-1.5">
                  <div className="flex justify-between text-sm">
                    <span className="mc-text-muted">Net deposit</span>
                    <span className="mc-text-primary font-medium">{formatICP(netDeposit)} ICP</span>
                  </div>
                  <div className="flex items-center justify-between pt-1">
                    <div className="flex items-center gap-1.5">
                      <TrendingUp className="h-3.5 w-3.5 mc-text-cyan" />
                      <span className="text-xs mc-text-dim">Bonus earned</span>
                    </div>
                    <span className="text-sm font-bold mc-text-cyan">+{formatICP(expectedReturn - netDeposit)} ICP</span>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="h-full flex items-center justify-center text-center min-h-[140px]">
              <div>
                <BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" />
                <p className="text-sm mc-text-muted">Enter an amount to see projected returns</p>
              </div>
            </div>
          )}
          <div className="mc-status-red p-3 text-center text-sm font-bold rounded-lg">
            <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME
            <div className="font-normal text-xs opacity-80 mt-0.5">Only play with money you can afford to lose</div>
          </div>
        </div>
      </div>
    </>
  );
}
