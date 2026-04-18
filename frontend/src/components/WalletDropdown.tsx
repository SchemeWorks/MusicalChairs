import React, { useState, useRef, useEffect } from 'react';
import { Principal } from '@dfinity/principal';
import { useQueryClient } from '@tanstack/react-query';
import { useWallet } from '../hooks/useWallet';
import { ICP_TRANSFER_FEE, useLedger, E8S_PER_ICP } from '../hooks/useLedger';
import { useGetCallerUserProfile, useSaveUserProfile, useGetPonziPoints, useGetCoverChargeBalance, useWithdrawCoverCharges, isCoverChargeAdmin, useICPBalance } from '../hooks/useQueries';
import { formatICP } from '../lib/formatICP';
import { Copy, Check, Loader2, X, Pencil, CreditCard, Briefcase, Send } from 'lucide-react';

interface WalletDropdownProps {
  isOpen: boolean;
  onClose: () => void;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
}

export default function WalletDropdown({ isOpen, onClose, buttonRef }: WalletDropdownProps) {
  const { principal, walletType } = useWallet();

  const [copied, setCopied] = useState(false);
  const [isEditingName, setIsEditingName] = useState(false);
  const [newName, setNewName] = useState('');

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

  const { data: icpBalance, isLoading: icpBalanceLoading } = useICPBalance();
  const ledger = useLedger();
  const queryClient = useQueryClient();
  const [sendOpen, setSendOpen] = useState(false);
  const [sendTo, setSendTo] = useState('');
  const [sendAmount, setSendAmount] = useState('');
  const [sendError, setSendError] = useState('');
  const [sendSuccess, setSendSuccess] = useState(false);
  const [sendBusy, setSendBusy] = useState(false);

  const handleSend = async () => {
    setSendError('');
    setSendSuccess(false);
    const toTrim = sendTo.trim();
    if (!toTrim) { setSendError('Enter a destination principal'); return; }
    let toPrincipal: Principal;
    try { toPrincipal = Principal.fromText(toTrim); }
    catch { setSendError('Invalid principal'); return; }
    if (principal && toPrincipal.toText() === principal) {
      setSendError('Cannot send to yourself');
      return;
    }
    const amt = Number(sendAmount);
    if (!Number.isFinite(amt) || amt <= 0) { setSendError('Enter a valid amount'); return; }
    const amountE8s = BigInt(Math.round(amt * Number(E8S_PER_ICP)));
    if (amountE8s <= 0n) { setSendError('Amount too small'); return; }
    const balE8s = icpBalance !== undefined ? BigInt(Math.round(icpBalance * Number(E8S_PER_ICP))) : 0n;
    if (amountE8s + ICP_TRANSFER_FEE > balE8s) {
      setSendError('Insufficient balance (includes 0.0001 ICP fee)');
      return;
    }
    setSendBusy(true);
    try {
      const result = await ledger.transfer(toPrincipal.toText(), amountE8s);
      if ('Err' in result) {
        const err: any = result.Err;
        const msg = err.InsufficientFunds ? 'Insufficient funds'
          : err.BadFee ? 'Bad fee'
          : err.GenericError?.message || 'Transfer failed';
        setSendError(msg);
      } else {
        setSendSuccess(true);
        setSendAmount('');
        setSendTo('');
        queryClient.invalidateQueries({ queryKey: ['icpLedgerBalance'] });
      }
    } catch (e: any) {
      setSendError(e?.message || 'Transfer failed');
    } finally {
      setSendBusy(false);
    }
  };

  const { data: userProfile } = useGetCallerUserProfile();
  const { data: ponziPointsData, isLoading: ponziLoading } = useGetPonziPoints();
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
  const ponziPoints = ponziPointsData?.totalPoints || 0;
  const userName = userProfile?.name || 'User';

  const walletIcon = walletType === 'internet-identity' ? <img src="/ii-logo.svg" alt="II" className="h-4 w-4" /> : walletType === 'plug' ? <img src="/plug-logo.svg" alt="Plug" className="h-4 w-4" /> : walletType === 'oisy' ? <img src="/oisy-logo.svg" alt="OISY" className="h-4 w-4" /> : <CreditCard className="h-4 w-4 mc-text-muted" />;
  const walletName = walletType === 'internet-identity' ? 'Internet Identity' : walletType === 'plug' ? 'Plug' : walletType === 'oisy' ? 'OISY' : 'Wallet';

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

  const handleSaveName = async () => {
    if (newName.trim() && newName.trim() !== userName) {
      try { await saveProfileMutation.mutateAsync({ name: newName.trim() }); setIsEditingName(false); }
      catch (e) { console.error('Name save failed:', e); }
    } else { setIsEditingName(false); }
    setNewName('');
  };

  if (!isOpen) return null;

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

        {/* Balance — real ledger balance from the connected wallet */}
        <div className="mc-card p-3 mb-2">
          <div className="flex items-center justify-between">
            <div>
              <div className="mc-label">ICP Balance</div>
              <div className="text-lg font-bold mc-text-green mc-glow-green">
                {icpBalanceLoading ? '...' : icpBalance !== undefined ? formatICP(icpBalance) : '—'} ICP
              </div>
            </div>
            <button
              onClick={() => { setSendOpen(v => !v); setSendError(''); setSendSuccess(false); }}
              className="mc-btn-secondary px-3 py-1.5 text-xs flex items-center gap-1"
            >
              <Send className="h-3 w-3" /> Send
            </button>
          </div>
          {sendOpen && (
            <div className="mt-3 pt-3 border-t border-white/10 space-y-2">
              <input
                value={sendTo}
                onChange={e => setSendTo(e.target.value)}
                placeholder="Destination principal"
                className="mc-input w-full h-8 text-xs px-2 font-mono"
              />
              <div className="flex gap-2">
                <input
                  value={sendAmount}
                  onChange={e => setSendAmount(e.target.value)}
                  placeholder="Amount (ICP)"
                  inputMode="decimal"
                  className="mc-input flex-1 h-8 text-xs px-2"
                />
                <button
                  onClick={handleSend}
                  disabled={sendBusy}
                  className="mc-btn-primary px-3 py-1 text-xs flex items-center gap-1 disabled:opacity-50"
                >
                  {sendBusy ? <><Loader2 className="h-3 w-3 animate-spin" /> Sending…</> : 'Send'}
                </button>
              </div>
              <div className="text-[10px] mc-text-muted">
                Fee: 0.0001 ICP. Transfers go directly from your wallet.
              </div>
              {sendError && <div className="mc-status-red p-2 text-xs text-center">{sendError}</div>}
              {sendSuccess && <div className="mc-status-green p-2 text-xs text-center">Transfer sent.</div>}
            </div>
          )}
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

      {/* Principal */}
      <div className="p-4">
        <div className="mc-label mb-1">Principal ID</div>
        <div className="flex gap-2">
          <div className="mc-card flex-1 p-2 text-xs mc-text-muted font-mono truncate">{principalId}</div>
          <button onClick={async () => { await navigator.clipboard.writeText(principalId); setCopied(true); setTimeout(() => setCopied(false), 2000); }}
            className="mc-btn-secondary px-2 py-1 rounded-lg">
            {copied ? <Check className="h-3 w-3 mc-text-green" /> : <Copy className="h-3 w-3" />}
          </button>
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
