import React, { useState, useRef, useEffect } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useLedger, icpToE8s, formatIcpBalance, ICP_TRANSFER_FEE } from '../hooks/useLedger';
import { useGetInternalWalletBalance, useSendFromInternalWallet, useGetCallerUserProfile, useSaveUserProfile, useGetPonziPoints, useDepositICP, useWithdrawICP } from '../hooks/useQueries';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Copy, Check, RefreshCw, ArrowDown, ArrowUp, Loader2, X } from 'lucide-react';

interface WalletDropdownProps {
  isOpen: boolean;
  onClose: () => void;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
}

export default function WalletDropdown({ isOpen, onClose, buttonRef }: WalletDropdownProps) {
  const { principal, walletType, isConnected } = useWallet();
  const ledger = useLedger();

  const [activeTab, setActiveTab] = useState<'deposit' | 'withdraw' | 'send'>('deposit');
  const [depositAmount, setDepositAmount] = useState('');
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [recipientPrincipal, setRecipientPrincipal] = useState('');
  const [copied, setCopied] = useState(false);
  const [inputError, setInputError] = useState('');
  const [isEditingName, setIsEditingName] = useState(false);
  const [newName, setNewName] = useState('');
  const [isApproving, setIsApproving] = useState(false);
  const [approvalComplete, setApprovalComplete] = useState(false);
  const [externalBalance, setExternalBalance] = useState<bigint | null>(null);
  const [externalBalanceLoading, setExternalBalanceLoading] = useState(false);

  const dropdownRef = useRef<HTMLDivElement>(null);
  const [dropdownStyle, setDropdownStyle] = useState<React.CSSProperties>({});

  const { data: balanceData, isLoading: balanceLoading, refetch: refetchBalance } = useGetInternalWalletBalance();
  const { data: userProfile } = useGetCallerUserProfile();
  const { data: ponziPointsData, isLoading: ponziLoading } = useGetPonziPoints();
  const depositMutation = useDepositICP();
  const withdrawMutation = useWithdrawICP();
  const sendMutation = useSendFromInternalWallet();
  const saveProfileMutation = useSaveUserProfile();

  const principalId = principal || '';
  const walletBalance = balanceData?.internalBalance || 0;
  const ponziPoints = ponziPointsData?.totalPoints || 0;
  const userName = userProfile?.name || 'User';
  const isTestMode = balanceData?.isTestMode ?? true;

  const walletEmoji = walletType === 'internet-identity' ? '🔐' : walletType === 'plug' ? '🔌' : walletType === 'oisy' ? '✨' : '💳';
  const walletName = walletType === 'internet-identity' ? 'Internet Identity' : walletType === 'plug' ? 'Plug' : walletType === 'oisy' ? 'OISY' : 'Wallet';

  // Fetch external balance
  const fetchExternalBalance = async () => {
    if (!ledger.isConnected) return;
    setExternalBalanceLoading(true);
    try { setExternalBalance(await ledger.getBalance()); }
    catch (e) { console.error('Balance fetch failed:', e); }
    finally { setExternalBalanceLoading(false); }
  };

  useEffect(() => {
    if (isOpen && isConnected && !isTestMode) fetchExternalBalance();
  }, [isOpen, isConnected, isTestMode]);

  // Click outside to close
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node) &&
          buttonRef.current && !buttonRef.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [isOpen, onClose, buttonRef]);

  // Position below button
  useEffect(() => {
    if (isOpen && buttonRef.current) {
      const r = buttonRef.current.getBoundingClientRect();
      setDropdownStyle({ position: 'fixed', top: r.bottom + 8, right: window.innerWidth - r.right, zIndex: 50 });
    }
  }, [isOpen, buttonRef]);

  const handleAmountInput = (setter: (v: string) => void, resetApproval = false) =>
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const v = restrictToEightDecimals(e.target.value);
      setter(v);
      setInputError(validateICPInput(v).error || '');
      if (resetApproval) setApprovalComplete(false);
    };

  // ICRC-2 Approve + Deposit
  const handleDeposit = async () => {
    const amount = parseFloat(depositAmount);
    if (!amount || amount <= 0) return;
    const v = validateICPInput(depositAmount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    const e8s = icpToE8s(amount);
    if (!approvalComplete) {
      setIsApproving(true);
      try {
        const res = await ledger.approveForDeposit(e8s);
        if (res.Err) { setInputError(`Approval failed: ${Object.keys(res.Err)[0]}`); setIsApproving(false); return; }
        setApprovalComplete(true);
      } catch (err: any) { setInputError(`Approval failed: ${err.message}`); setIsApproving(false); return; }
      setIsApproving(false);
    }
    try { await depositMutation.mutateAsync(e8s); setDepositAmount(''); setApprovalComplete(false); fetchExternalBalance(); }
    catch (err: any) { setInputError(`Deposit failed: ${err.message}`); }
  };

  const handleWithdraw = async () => {
    const amount = parseFloat(withdrawAmount);
    if (!amount || amount <= 0) return;
    const v = validateICPInput(withdrawAmount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try { await withdrawMutation.mutateAsync(icpToE8s(amount)); setWithdrawAmount(''); fetchExternalBalance(); }
    catch (err: any) { setInputError(`Withdrawal failed: ${err.message}`); }
  };

  const handleSend = async () => {
    const amount = parseFloat(withdrawAmount);
    if (!amount || amount <= 0 || !recipientPrincipal.trim()) return;
    const v = validateICPInput(withdrawAmount);
    if (!v.isValid) { setInputError(v.error || ''); return; }
    try { await sendMutation.mutateAsync({ amount, recipientPrincipal: recipientPrincipal.trim() }); setWithdrawAmount(''); setRecipientPrincipal(''); setInputError(''); }
    catch (e) { console.error('Send failed:', e); }
  };

  const handleMaxDeposit = () => {
    if (externalBalance !== null) {
      const max = externalBalance - ICP_TRANSFER_FEE - 10000n;
      if (max > 0n) setDepositAmount(formatIcpBalance(max));
    }
    setInputError('');
  };

  const handleMaxWithdraw = () => { setWithdrawAmount(walletBalance.toString()); setInputError(''); };

  const handleSaveName = async () => {
    if (newName.trim() && newName.trim() !== userName) {
      try { await saveProfileMutation.mutateAsync({ name: newName.trim() }); setIsEditingName(false); }
      catch (e) { console.error('Name save failed:', e); }
    } else { setIsEditingName(false); }
    setNewName('');
  };

  if (!isOpen) return null;

  const depVal = parseFloat(depositAmount) || 0;
  const witVal = parseFloat(withdrawAmount) || 0;

  return (
    <div ref={dropdownRef} style={dropdownStyle} className="mc-dropdown w-96 max-w-[calc(100vw-2rem)] overflow-hidden">
      {/* Header */}
      <div className="p-4 border-b border-white/10">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <span>{walletEmoji}</span>
            <span className="text-xs mc-text-muted">{walletName}</span>
            <span className="w-1.5 h-1.5 bg-green-400 rounded-full" />
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-white/5"><X className="h-4 w-4 mc-text-muted" /></button>
        </div>

        {/* Name */}
        <div className="flex items-center gap-2 mb-3">
          <span className="mc-text-dim text-sm">Welcome,</span>
          {isEditingName ? (
            <div className="flex items-center gap-1">
              <input value={newName} onChange={e => setNewName(e.target.value)} className="mc-input h-6 text-xs px-2 w-24" autoFocus
                onKeyDown={e => { if (e.key === 'Enter') handleSaveName(); if (e.key === 'Escape') { setIsEditingName(false); setNewName(''); } }} />
              <button onClick={handleSaveName} className="text-xs mc-text-green">✓</button>
              <button onClick={() => { setIsEditingName(false); setNewName(''); }} className="text-xs mc-text-danger">✕</button>
            </div>
          ) : (
            <span className="font-bold text-sm mc-text-primary cursor-pointer hover:mc-text-cyan" onClick={() => { setNewName(userName); setIsEditingName(true); }}>
              {userName} ✏️
            </span>
          )}
        </div>

        {/* Balances */}
        <div className="mc-card p-3 mb-2">
          <div className="flex items-center justify-between">
            <div>
              <div className="mc-label">Game Balance</div>
              <div className="text-lg font-bold mc-text-green mc-glow-green">
                {balanceLoading ? '...' : formatICP(walletBalance)} ICP
              </div>
            </div>
            <button onClick={() => refetchBalance()} className="mc-text-muted hover:mc-text-primary"><RefreshCw className="h-3 w-3" /></button>
          </div>
          {!isTestMode && (
            <div className="flex items-center justify-between mt-2 pt-2 border-t border-white/5">
              <div>
                <div className="mc-label">Wallet</div>
                <div className="text-sm font-bold mc-text-primary">
                  {externalBalanceLoading ? '...' : externalBalance !== null ? formatIcpBalance(externalBalance) : '—'} ICP
                </div>
              </div>
              <button onClick={fetchExternalBalance} className="mc-text-muted hover:mc-text-primary"><RefreshCw className="h-3 w-3" /></button>
            </div>
          )}
        </div>

        <div className="text-center text-xs">
          <span className="mc-text-purple font-bold">{ponziLoading ? '...' : ponziPoints.toLocaleString()} PP</span>
        </div>
        {isTestMode && <div className="mc-status-gold p-2 mt-2 text-center text-xs">Test Mode — Using simulated ICP</div>}
      </div>

      {/* Tabs */}
      <div className="p-4">
        <div className="flex rounded-lg bg-white/5 p-0.5 mb-4">
          {(['deposit', 'withdraw', 'send'] as const).map(tab => (
            <button key={tab} onClick={() => { setActiveTab(tab); setInputError(''); }}
              className={`flex-1 py-1.5 text-xs font-bold rounded-md transition-all capitalize ${
                activeTab === tab ? 'bg-purple-500/20 mc-text-purple' : 'mc-text-muted hover:mc-text-dim'
              }`}>
              {tab}
            </button>
          ))}
        </div>

        {/* Deposit */}
        {activeTab === 'deposit' && (
          isTestMode ? (
            <div className="mc-status-gold p-3 text-xs text-center">Deposits are simulated in test mode. Balance starts at 500 ICP.</div>
          ) : (
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <span className="mc-label">Amount</span>
                <span className="text-xs mc-text-muted">Min: 0.1 ICP</span>
              </div>
              <div className="flex gap-2">
                <input type="number" value={depositAmount} onChange={handleAmountInput(setDepositAmount, true)}
                  placeholder="0.0" min="0.1" step="0.00000001" className="mc-input flex-1 text-sm" />
                <button onClick={handleMaxDeposit} disabled={externalBalance === null} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
              </div>
              <div className="mc-card p-2 text-xs mc-text-muted">
                <span className={approvalComplete ? 'mc-text-green' : ''}>1. Approve transfer{approvalComplete ? ' ✓' : ''}</span>
                <span className="mx-1">→</span>
                <span>2. Confirm deposit</span>
              </div>
              <button onClick={handleDeposit} disabled={depVal < 0.1 || !!inputError || isApproving || depositMutation.isPending}
                className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
                {isApproving ? <><Loader2 className="h-3 w-3 animate-spin" /> Approving...</>
                  : depositMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Depositing...</>
                  : approvalComplete ? <><ArrowDown className="h-3 w-3" /> Confirm Deposit</>
                  : <><ArrowDown className="h-3 w-3" /> Approve &amp; Deposit</>}
              </button>
              {depositMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Deposit successful!</div>}
            </div>
          )
        )}

        {/* Withdraw */}
        {activeTab === 'withdraw' && (
          isTestMode ? (
            <div className="mc-status-gold p-3 text-xs text-center">External withdrawals disabled in test mode. Use "Send" for internal transfers.</div>
          ) : (
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <span className="mc-label">Amount</span>
                <span className="text-xs mc-text-muted">Available: {formatICP(walletBalance)} ICP</span>
              </div>
              <div className="flex gap-2">
                <input type="number" value={withdrawAmount} onChange={handleAmountInput(setWithdrawAmount)}
                  placeholder="0.0" min="0.1" step="0.00000001" className="mc-input flex-1 text-sm" />
                <button onClick={handleMaxWithdraw} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
              </div>
              <p className="text-xs mc-text-muted">0.0001 ICP transfer fee applies.</p>
              {witVal > walletBalance && <div className="mc-status-red p-2 text-xs text-center">Insufficient balance</div>}
              <button onClick={handleWithdraw} disabled={witVal < 0.1 || witVal > walletBalance || !!inputError || withdrawMutation.isPending}
                className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
                {withdrawMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Withdrawing...</>
                  : <><ArrowUp className="h-3 w-3" /> Withdraw to Wallet</>}
              </button>
              {withdrawMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Withdrawal successful!</div>}
            </div>
          )
        )}

        {/* Send */}
        {activeTab === 'send' && (
          <div className="space-y-3">
            <div>
              <span className="mc-label">Recipient Principal</span>
              <input type="text" value={recipientPrincipal} onChange={e => setRecipientPrincipal(e.target.value)}
                placeholder="Enter principal" className="mc-input w-full text-xs mt-1 font-mono" />
            </div>
            <div className="flex justify-between items-center">
              <span className="mc-label">Amount</span>
              <span className="text-xs mc-text-muted">Available: {formatICP(walletBalance)} ICP</span>
            </div>
            <div className="flex gap-2">
              <input type="number" value={withdrawAmount} onChange={handleAmountInput(setWithdrawAmount)}
                placeholder="0.0" min="0.01" step="0.00000001" className="mc-input flex-1 text-sm" />
              <button onClick={handleMaxWithdraw} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
            </div>
            {inputError && <div className="text-xs mc-text-danger">{inputError}</div>}
            {witVal > walletBalance && !inputError && <div className="mc-status-red p-2 text-xs text-center">Insufficient balance</div>}
            <button onClick={handleSend} disabled={witVal <= 0 || witVal > walletBalance || !!inputError || !recipientPrincipal.trim() || sendMutation.isPending}
              className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
              {sendMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Sending...</> : 'Send ICP'}
            </button>
            {sendMutation.isError && <div className="mc-status-red p-2 text-xs text-center">{sendMutation.error?.message || 'Transfer failed'}</div>}
            {sendMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Transfer successful!</div>}
          </div>
        )}

        {/* Principal */}
        <div className="mt-4 pt-3 border-t border-white/5">
          <div className="mc-label mb-1">Principal ID</div>
          <div className="flex gap-2">
            <div className="mc-card flex-1 p-2 text-xs mc-text-muted font-mono truncate">{principalId}</div>
            <button onClick={async () => { await navigator.clipboard.writeText(principalId); setCopied(true); setTimeout(() => setCopied(false), 2000); }}
              className="mc-btn-secondary px-2 py-1 rounded-lg">
              {copied ? <Check className="h-3 w-3 mc-text-green" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
