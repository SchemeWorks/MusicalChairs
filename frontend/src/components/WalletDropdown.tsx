import React, { useState, useRef, useEffect } from 'react';
import { useWallet, WalletPanel } from '../hooks/useWallet';
import { useLedger, icpToE8s, formatIcpBalance, ICP_TRANSFER_FEE } from '../hooks/useLedger';
import { useGetInternalWalletBalance, useSendFromInternalWallet, useGetCallerUserProfile, useSaveUserProfile, useGetPonziPoints, useDepositICP, useWithdrawICP, useGetCoverChargeBalance, useWithdrawCoverCharges, isCoverChargeAdmin } from '../hooks/useQueries';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Copy, Check, RefreshCw, ArrowDown, ArrowUp, Loader2, X, Pencil, CreditCard, Briefcase } from 'lucide-react';

interface WalletDropdownProps {
  isOpen: boolean;
  onClose: () => void;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
}

export default function WalletDropdown({ isOpen, onClose, buttonRef }: WalletDropdownProps) {
  const { principal, walletType, isConnected, initialPanel } = useWallet();
  const ledger = useLedger();

  const isIIUser = walletType === 'internet-identity';
  const [activeTab, setActiveTab] = useState<WalletPanel>(isIIUser ? 'deposit' : 'send');
  const [depositAmount, setDepositAmount] = useState('');

  // Reset active tab when wallet type changes (e.g. logout/login with different wallet)
  useEffect(() => {
    setActiveTab(isIIUser ? 'deposit' : 'send');
  }, [isIIUser]);

  // When the wallet opens, jump to the requested panel
  useEffect(() => {
    if (isOpen) {
      // Non-II wallets only support the 'send' panel — clamp to that for them
      setActiveTab(isIIUser ? initialPanel : 'send');
    }
  }, [isOpen, initialPanel, isIIUser]);
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [recipientPrincipal, setRecipientPrincipal] = useState('');
  const [copied, setCopied] = useState(false);
  const [inputError, setInputError] = useState('');
  const [shakeInput, setShakeInput] = useState(false);
  const triggerShake = () => { setShakeInput(true); setTimeout(() => setShakeInput(false), 400); };
  const [isEditingName, setIsEditingName] = useState(false);
  const [newName, setNewName] = useState('');
  const [isApproving, setIsApproving] = useState(false);
  const [approvalComplete, setApprovalComplete] = useState(false);
  const [externalBalance, setExternalBalance] = useState<bigint | null>(null);
  const [externalBalanceLoading, setExternalBalanceLoading] = useState(false);

  const dropdownRef = useRef<HTMLDivElement>(null);
  const [dropdownStyle, setDropdownStyle] = useState<React.CSSProperties>({});
  const [isMobile, setIsMobile] = useState(false);

  // Drag-to-dismiss touch state
  const [touchStartY, setTouchStartY] = useState<number | null>(null);
  const [dragDelta, setDragDelta] = useState(0);
  const [isDragging, setIsDragging] = useState(false);

  const handleTouchStart = (e: React.TouchEvent) => {
    setTouchStartY(e.touches[0].clientY);
    setIsDragging(false);
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (touchStartY === null) return;
    const delta = e.touches[0].clientY - touchStartY;
    if (delta > 0) {
      setDragDelta(delta);
      setIsDragging(true);
    }
  };

  const handleTouchEnd = () => {
    if (dragDelta > 100) {
      onClose();
    }
    setTouchStartY(null);
    setDragDelta(0);
    setIsDragging(false);
  };

  useEffect(() => {
    const check = () => setIsMobile(window.innerWidth < 769);
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, []);

  const { data: balanceData, isLoading: balanceLoading, refetch: refetchBalance } = useGetInternalWalletBalance();
  const { data: userProfile } = useGetCallerUserProfile();
  const { data: ponziPointsData, isLoading: ponziLoading } = useGetPonziPoints();
  const depositMutation = useDepositICP();
  const withdrawMutation = useWithdrawICP();
  const sendMutation = useSendFromInternalWallet();
  const saveProfileMutation = useSaveUserProfile();

  // Cover Charge — admin only. Hooks below are no-ops unless the connected
  // principal matches COVER_CHARGE_RECIPIENT (backend enforces independently).
  const isAdmin = isCoverChargeAdmin(principal);
  const { data: coverChargeData, isLoading: coverChargeLoading } = useGetCoverChargeBalance();
  const payManagementMutation = useWithdrawCoverCharges();
  const [payManagementError, setPayManagementError] = useState('');

  const handlePayManagement = async () => {
    setPayManagementError('');
    const bucketE8s = coverChargeData?.e8s ?? 0n;
    // Transfer fee is absorbed by the bucket; need more than the fee to proceed.
    if (bucketE8s <= ICP_TRANSFER_FEE) {
      setPayManagementError('Balance must exceed the ledger transfer fee');
      return;
    }
    try {
      await payManagementMutation.mutateAsync(bucketE8s);
    } catch (err: any) {
      setPayManagementError(err?.message || 'Pay Management failed');
    }
  };

  const principalId = principal || '';
  const walletBalance = balanceData?.internalBalance || 0;
  const ponziPoints = ponziPointsData?.totalPoints || 0;
  const userName = userProfile?.name || 'User';
  const isTestMode = balanceData?.isTestMode ?? true;

  const walletIcon = walletType === 'internet-identity' ? <img src="/ii-logo.svg" alt="II" className="h-4 w-4" /> : walletType === 'plug' ? <img src="/plug-logo.svg" alt="Plug" className="h-4 w-4" /> : walletType === 'oisy' ? <img src="/oisy-logo.svg" alt="OISY" className="h-4 w-4" /> : <CreditCard className="h-4 w-4 mc-text-muted" />;
  const walletName = walletType === 'internet-identity' ? 'Internet Identity' : walletType === 'plug' ? 'Plug' : walletType === 'oisy' ? 'OISY' : 'Wallet';

  // Fetch external balance (only relevant for II users who have a separate wallet)
  const fetchExternalBalance = async () => {
    if (!ledger.isConnected || !isIIUser) return;
    setExternalBalanceLoading(true);
    try { setExternalBalance(await ledger.getBalance()); }
    catch (e) { console.error('Balance fetch failed:', e); }
    finally { setExternalBalanceLoading(false); }
  };

  useEffect(() => {
    if (isOpen && isConnected) fetchExternalBalance();
  }, [isOpen, isConnected, isTestMode, isIIUser]);

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
    if (!v.isValid) { setInputError(v.error || ''); triggerShake(); return; }
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

  const walletContent = (
    <>
      {/* Header */}
      <div className="p-4 border-b border-white/10">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <span className="flex items-center">{walletIcon}</span>
            <span className="text-xs mc-text-muted">{walletName}</span>
            <span className="w-1.5 h-1.5 bg-green-400 rounded-full" />
          </div>
          <button onClick={onClose} className="p-2 rounded hover:bg-white/5"><X className="h-4 w-4 mc-text-muted" /></button>
        </div>

        {/* Name */}
        <div className="flex items-center gap-2 mb-3">
          <span className="mc-text-dim text-sm">Welcome,</span>
          {isEditingName ? (
            <div className="flex items-center gap-1">
              <input value={newName} onChange={e => setNewName(e.target.value)} className="mc-input h-6 text-xs px-2 w-24" autoFocus
                onKeyDown={e => { if (e.key === 'Enter') handleSaveName(); if (e.key === 'Escape') { setIsEditingName(false); setNewName(''); } }} />
              <button onClick={handleSaveName} className="text-xs mc-text-green p-2">✓</button>
              <button onClick={() => { setIsEditingName(false); setNewName(''); }} className="text-xs mc-text-danger p-2">✕</button>
            </div>
          ) : (
            <span className="font-bold text-sm mc-text-primary cursor-pointer hover:mc-text-cyan" onClick={() => { setNewName(userName); setIsEditingName(true); }}>
              {userName} <Pencil className="h-3 w-3 inline ml-1 mc-text-muted" />
            </span>
          )}
        </div>

        {/* Balance */}
        <div className="mc-card p-3 mb-2">
          <div className="flex items-center justify-between">
            <div>
              <div className="mc-label">ICP Balance</div>
              <div className="text-lg font-bold mc-text-green mc-glow-green">
                {externalBalanceLoading ? '...' : externalBalance !== null ? formatIcpBalance(externalBalance) : '—'} ICP
              </div>
            </div>
            <button onClick={fetchExternalBalance} className="mc-text-muted hover:mc-text-primary p-2"><RefreshCw className="h-3 w-3" /></button>
          </div>
        </div>

        {/* Cover Charges — admin only. Displayed as a separate sub-account
            so it never mingles with the player-facing ICP balance above. */}
        {isAdmin && (
          <div className="mc-card p-3 mb-2 border border-amber-500/30 bg-amber-500/5">
            <div className="flex items-center justify-between">
              <div>
                <div className="mc-label flex items-center gap-1.5">
                  <Briefcase className="h-3 w-3 mc-text-gold" />
                  Cover Charges
                </div>
                <div className="text-lg font-bold mc-text-gold">
                  {coverChargeLoading ? '...' : coverChargeData ? formatICP(coverChargeData.icp) : '—'} ICP
                </div>
              </div>
              <button
                onClick={handlePayManagement}
                disabled={
                  payManagementMutation.isPending ||
                  !coverChargeData ||
                  coverChargeData.e8s <= ICP_TRANSFER_FEE
                }
                className="mc-btn-primary px-3 py-1.5 text-xs flex items-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {payManagementMutation.isPending ? (
                  <><Loader2 className="h-3 w-3 animate-spin" /> Paying…</>
                ) : (
                  <>Pay Management</>
                )}
              </button>
            </div>
            {payManagementError && (
              <div className="mc-status-red p-2 text-xs text-center mt-2">{payManagementError}</div>
            )}
            {payManagementMutation.isSuccess && !payManagementError && (
              <div className="mc-status-green p-2 text-xs text-center mt-2">Management has been paid.</div>
            )}
          </div>
        )}

        <div className="text-center text-xs">
          <span className="mc-text-purple font-bold">{ponziLoading ? '...' : ponziPoints.toLocaleString()} PP</span>
        </div>
      </div>

      {/* Tabs */}
      <div className="p-4">
        {isIIUser ? (
          <div className="flex rounded-lg bg-white/5 p-0.5 mb-4">
            {([
              { key: 'deposit' as const, label: 'Deposit' },
              { key: 'withdraw' as const, label: 'Cash Out' },
              { key: 'send' as const, label: 'Wire' },
            ]).map(tab => (
              <button key={tab.key} onClick={() => { setActiveTab(tab.key); setInputError(''); }}
                className={`flex-1 py-2 text-xs font-bold rounded-md transition-all ${
                  activeTab === tab.key ? 'bg-[var(--mc-purple)]/25 mc-text-primary border border-[var(--mc-purple)]/30 shadow-[0_0_8px_rgba(168,85,247,0.15)]' : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
                }`}>
                {tab.label}
              </button>
            ))}
          </div>
        ) : (
          <div className="mb-4">
            <div className="text-xs font-bold mc-text-muted uppercase tracking-wider">Wire ICP</div>
          </div>
        )}

        {/* Deposit (II users only) */}
        {isIIUser && activeTab === 'deposit' && (
          <div className="space-y-3">
            <div className="flex justify-between items-center">
              <span className="mc-label">Amount</span>
              <span className="text-xs mc-text-muted">Available: {externalBalanceLoading ? '...' : externalBalance !== null ? formatIcpBalance(externalBalance) : '—'} ICP</span>
            </div>
            <div className="flex gap-2">
              <input type="number" value={depositAmount} onChange={handleAmountInput(setDepositAmount, true)}
                placeholder="0.0" min="0.1" step="0.00000001" className={`mc-input flex-1 text-sm ${shakeInput ? 'mc-shake' : ''}`} />
              <button onClick={handleMaxDeposit} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
            </div>
            <p className="text-xs mc-text-muted">Two steps: approve then deposit. 0.0001 ICP fee each.</p>
            {inputError && <div className="text-xs mc-text-danger">{inputError}</div>}
            <button onClick={handleDeposit} disabled={depVal <= 0 || !!inputError || isApproving || depositMutation.isPending}
              className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
              {isApproving ? <><Loader2 className="h-3 w-3 animate-spin" /> Approving...</>
                : depositMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Depositing...</>
                : approvalComplete ? <><ArrowDown className="h-3 w-3" /> Confirm Deposit</>
                : <><ArrowDown className="h-3 w-3" /> Deposit ICP</>}
            </button>
            {depositMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Deposited. Now go lose it.</div>}
          </div>
        )}

        {/* Withdraw (II users only) */}
        {isIIUser && activeTab === 'withdraw' && (
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <span className="mc-label">Amount</span>
                <span className="text-xs mc-text-muted">Available: {formatICP(walletBalance)} ICP</span>
              </div>
              <div className="flex gap-2">
                <input type="number" value={withdrawAmount} onChange={handleAmountInput(setWithdrawAmount)}
                  placeholder="0.0" min="0.1" step="0.00000001" className={`mc-input flex-1 text-sm ${shakeInput ? 'mc-shake' : ''}`} />
                <button onClick={handleMaxWithdraw} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
              </div>
              <p className="text-xs mc-text-muted">0.0001 ICP transfer fee applies.</p>
              {witVal > walletBalance && <div className="mc-status-red p-2 text-xs text-center">Insufficient balance</div>}
              <button onClick={handleWithdraw} disabled={witVal < 0.1 || witVal > walletBalance || !!inputError || withdrawMutation.isPending}
                className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
                {withdrawMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Cashing Out...</>
                  : <><ArrowUp className="h-3 w-3" /> Cash Out to Wallet</>}
              </button>
              {withdrawMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Cashed out. Smart move &mdash; or was it?</div>}
            </div>
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
                placeholder="0.0" min="0.01" step="0.00000001" className={`mc-input flex-1 text-sm ${shakeInput ? 'mc-shake' : ''}`} />
              <button onClick={handleMaxWithdraw} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg">MAX</button>
            </div>
            {inputError && <div className="text-xs mc-text-danger">{inputError}</div>}
            {witVal > walletBalance && !inputError && <div className="mc-status-red p-2 text-xs text-center">Insufficient balance</div>}
            <button onClick={handleSend} disabled={witVal <= 0 || witVal > walletBalance || !!inputError || !recipientPrincipal.trim() || sendMutation.isPending}
              className="w-full mc-btn-primary py-2 text-xs flex items-center justify-center gap-2">
              {sendMutation.isPending ? <><Loader2 className="h-3 w-3 animate-spin" /> Wiring...</> : 'Wire ICP'}
            </button>
            {sendMutation.isError && <div className="mc-status-red p-2 text-xs text-center">{sendMutation.error?.message || 'Wire failed'}</div>}
            {sendMutation.isSuccess && <div className="mc-status-green p-2 text-xs text-center">Wired. The money's gone.</div>}
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
    </>
  );

  if (isMobile) {
    return (
      <>
        <div className="mc-sheet-backdrop" onClick={onClose} />
        <div
          ref={dropdownRef}
          className="mc-bottom-sheet"
          style={{
            transform: dragDelta > 0 ? `translateY(${dragDelta}px)` : undefined,
            transition: isDragging ? 'none' : 'transform 0.3s ease-out',
            willChange: 'transform',
          }}
        >
          <div
            className="flex justify-center pt-3 pb-1 cursor-grab active:cursor-grabbing"
            onTouchStart={handleTouchStart}
            onTouchMove={handleTouchMove}
            onTouchEnd={handleTouchEnd}
          >
            <div className="w-10 h-1 rounded-full bg-white/20" />
          </div>
          {walletContent}
        </div>
      </>
    );
  }

  return (
    <div ref={dropdownRef} style={dropdownStyle} className="mc-dropdown w-96 max-w-[calc(100vw-2rem)] overflow-hidden">
      {walletContent}
    </div>
  );
}
