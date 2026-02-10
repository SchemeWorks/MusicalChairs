import React, { useState, useRef, useEffect } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useLedger, icpToE8s, formatIcpBalance, BACKEND_CANISTER_ID, ICP_TRANSFER_FEE } from '../hooks/useLedger';
import { useGetInternalWalletBalance, useSendFromInternalWallet, useGetCallerUserProfile, useSaveUserProfile, useGetPonziPoints, useDepositICP, useWithdrawICP } from '../hooks/useQueries';
import { formatICP, validateICPInput, restrictToEightDecimals } from '../lib/formatICP';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Copy, Check, RefreshCw, ArrowDown, ArrowUp, Loader2 } from 'lucide-react';

interface WalletDropdownProps {
  isOpen: boolean;
  onClose: () => void;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
}

export default function WalletDropdown({ isOpen, onClose, buttonRef }: WalletDropdownProps) {
  const { principal, walletType, isConnected } = useWallet();
  const ledger = useLedger();
  
  // Form state
  const [depositAmount, setDepositAmount] = useState('');
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [recipientPrincipal, setRecipientPrincipal] = useState('');
  const [copied, setCopied] = useState(false);
  const [inputError, setInputError] = useState<string>('');
  const [isEditingName, setIsEditingName] = useState(false);
  const [newName, setNewName] = useState('');
  
  // Approval state for deposits
  const [isApproving, setIsApproving] = useState(false);
  const [approvalComplete, setApprovalComplete] = useState(false);
  
  const dropdownRef = useRef<HTMLDivElement>(null);
  
  // Queries
  const { data: balanceData, isLoading: balanceLoading, refetch: refetchBalance } = useGetInternalWalletBalance();
  const { data: userProfile } = useGetCallerUserProfile();
  const { data: ponziPointsData, isLoading: ponziLoading } = useGetPonziPoints();
  
  // Mutations
  const depositMutation = useDepositICP();
  const withdrawMutation = useWithdrawICP();
  const sendMutation = useSendFromInternalWallet();
  const saveProfileMutation = useSaveUserProfile();

  // External wallet balance (for deposits)
  const [externalBalance, setExternalBalance] = useState<bigint | null>(null);
  const [externalBalanceLoading, setExternalBalanceLoading] = useState(false);

  const principalId = principal || '';
  const walletBalance = balanceData?.internalBalance || 0;
  const ponziPoints = ponziPointsData?.totalPoints || 0;
  const userName = userProfile?.name || 'User';
  const isTestMode = balanceData?.isTestMode ?? true;

  // Get wallet icon and name
  const getWalletInfo = () => {
    switch (walletType) {
      case 'internet-identity':
        return { icon: '🔐', name: 'Internet Identity' };
      case 'plug':
        return { icon: '🔌', name: 'Plug Wallet' };
      case 'oisy':
        return { icon: '✨', name: 'OISY Wallet' };
      default:
        return { icon: '💳', name: 'Wallet' };
    }
  };

  const walletInfo = getWalletInfo();

  // Fetch external wallet balance when dropdown opens
  useEffect(() => {
    if (isOpen && isConnected && !isTestMode) {
      fetchExternalBalance();
    }
  }, [isOpen, isConnected, isTestMode]);

  const fetchExternalBalance = async () => {
    if (!ledger.isConnected) return;
    setExternalBalanceLoading(true);
    try {
      const balance = await ledger.getBalance();
      setExternalBalance(balance);
    } catch (error) {
      console.error('Failed to fetch external balance:', error);
    } finally {
      setExternalBalanceLoading(false);
    }
  };

  // Handle deposit amount input
  const handleDepositAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const input = e.target.value;
    const restrictedInput = restrictToEightDecimals(input);
    const validation = validateICPInput(restrictedInput);
    
    setDepositAmount(restrictedInput);
    setInputError(validation.error || '');
    setApprovalComplete(false); // Reset approval when amount changes
  };

  // Handle withdraw amount input
  const handleWithdrawAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const input = e.target.value;
    const restrictedInput = restrictToEightDecimals(input);
    const validation = validateICPInput(restrictedInput);
    
    setWithdrawAmount(restrictedInput);
    setInputError(validation.error || '');
  };

  // Handle click outside to close dropdown
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        dropdownRef.current && 
        !dropdownRef.current.contains(event.target as Node) &&
        buttonRef.current &&
        !buttonRef.current.contains(event.target as Node)
      ) {
        onClose();
      }
    }

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen, onClose, buttonRef]);

  // Position dropdown below button
  const [dropdownStyle, setDropdownStyle] = useState<React.CSSProperties>({});

  useEffect(() => {
    if (isOpen && buttonRef.current) {
      const buttonRect = buttonRef.current.getBoundingClientRect();
      setDropdownStyle({
        position: 'fixed',
        top: buttonRect.bottom + 8,
        right: window.innerWidth - buttonRect.right,
        zIndex: 50,
      });
    }
  }, [isOpen, buttonRef]);

  // Handle ICRC-2 Approve + Deposit flow
  const handleDeposit = async () => {
    const amount = parseFloat(depositAmount);
    if (!amount || amount <= 0) return;
    
    const validation = validateICPInput(depositAmount);
    if (!validation.isValid) {
      setInputError(validation.error || '');
      return;
    }

    const amountE8s = icpToE8s(amount);

    // Step 1: Approve if not already approved
    if (!approvalComplete) {
      setIsApproving(true);
      try {
        const approvalResult = await ledger.approveForDeposit(amountE8s);
        if (approvalResult.Err) {
          const errMsg = Object.keys(approvalResult.Err)[0];
          setInputError(`Approval failed: ${errMsg}`);
          setIsApproving(false);
          return;
        }
        setApprovalComplete(true);
      } catch (error: any) {
        setInputError(`Approval failed: ${error.message}`);
        setIsApproving(false);
        return;
      }
      setIsApproving(false);
    }

    // Step 2: Call backend depositICP
    try {
      await depositMutation.mutateAsync(amountE8s);
      setDepositAmount('');
      setApprovalComplete(false);
      fetchExternalBalance(); // Refresh external balance
    } catch (error: any) {
      setInputError(`Deposit failed: ${error.message}`);
    }
  };

  // Handle withdrawal to external wallet
  const handleWithdraw = async () => {
    const amount = parseFloat(withdrawAmount);
    if (!amount || amount <= 0) return;
    
    const validation = validateICPInput(withdrawAmount);
    if (!validation.isValid) {
      setInputError(validation.error || '');
      return;
    }

    const amountE8s = icpToE8s(amount);

    try {
      await withdrawMutation.mutateAsync(amountE8s);
      setWithdrawAmount('');
      fetchExternalBalance(); // Refresh external balance
    } catch (error: any) {
      setInputError(`Withdrawal failed: ${error.message}`);
    }
  };

  // Handle internal transfer
  const handleSendInternal = async () => {
    const amount = parseFloat(withdrawAmount);
    if (!amount || amount <= 0 || !recipientPrincipal.trim()) return;
    
    const validation = validateICPInput(withdrawAmount);
    if (!validation.isValid) {
      setInputError(validation.error || '');
      return;
    }
    
    try {
      await sendMutation.mutateAsync({ 
        amount, 
        recipientPrincipal: recipientPrincipal.trim() 
      });
      setWithdrawAmount('');
      setRecipientPrincipal('');
      setInputError('');
    } catch (error) {
      console.error('Send failed:', error);
    }
  };

  const handleMaxDeposit = () => {
    if (externalBalance !== null) {
      // Subtract fee and a small buffer
      const maxAmount = externalBalance - ICP_TRANSFER_FEE - 10000n;
      if (maxAmount > 0n) {
        setDepositAmount(formatIcpBalance(maxAmount));
      }
    }
    setInputError('');
  };

  const handleMaxWithdraw = () => {
    setWithdrawAmount(walletBalance.toString());
    setInputError('');
  };

  const copyPrincipalToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(principalId);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const handleEditName = () => {
    setNewName(userName);
    setIsEditingName(true);
  };

  const handleSaveName = async () => {
    if (newName.trim() && newName.trim() !== userName) {
      try {
        await saveProfileMutation.mutateAsync({ name: newName.trim() });
        setIsEditingName(false);
        setNewName('');
      } catch (error) {
        console.error('Failed to save name:', error);
      }
    } else {
      setIsEditingName(false);
      setNewName('');
    }
  };

  const handleCancelEdit = () => {
    setIsEditingName(false);
    setNewName('');
  };

  if (!isOpen) return null;

  const depositAmountValue = parseFloat(depositAmount) || 0;
  const withdrawAmountValue = parseFloat(withdrawAmount) || 0;
  const isDepositValid = depositAmountValue >= 0.1 && !inputError;
  const isWithdrawValid = withdrawAmountValue >= 0.1 && withdrawAmountValue <= walletBalance && !inputError;
  const isSendValid = withdrawAmountValue > 0 && withdrawAmountValue <= walletBalance && !inputError && recipientPrincipal.trim();

  return (
    <div
      ref={dropdownRef}
      style={dropdownStyle}
      className={`wallet-dropdown-card w-96 bg-gray-900/95 backdrop-blur-sm border border-gray-700 rounded-xl shadow-2xl overflow-hidden transition-all duration-200 ${
        isOpen ? 'animate-fade-slide-in' : ''
      }`}
    >
      {/* Header with welcome and inline name editing */}
      <div className="bg-gradient-to-r from-green-600 to-blue-600 p-4 text-center">
        {/* Connected Wallet Badge */}
        <div className="flex items-center justify-center gap-2 mb-2">
          <span className="text-lg">{walletInfo.icon}</span>
          <span className="text-white/90 text-sm font-medium">{walletInfo.name}</span>
          <span className="w-2 h-2 bg-green-400 rounded-full animate-pulse" title="Connected"></span>
        </div>
        
        <div className="text-white font-bold text-lg mb-2 flex items-center justify-center gap-2">
          🎭 Welcome, 
          {isEditingName ? (
            <div className="flex items-center gap-2">
              <Input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                className="w-24 h-6 text-sm px-2 py-1 bg-white text-black"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    handleSaveName();
                  } else if (e.key === 'Escape') {
                    handleCancelEdit();
                  }
                }}
              />
              <button
                onClick={handleSaveName}
                disabled={saveProfileMutation.isPending}
                className="text-xs bg-white text-green-600 px-2 py-1 rounded hover:bg-gray-100 transition-colors"
              >
                ✓
              </button>
              <button
                onClick={handleCancelEdit}
                className="text-xs bg-white text-red-600 px-2 py-1 rounded hover:bg-gray-100 transition-colors"
              >
                ✕
              </button>
            </div>
          ) : (
            <>
              {userName}!
              <button
                onClick={handleEditName}
                className="text-white/80 hover:text-white transition-colors ml-1"
                title="Edit name"
              >
                ✏️
              </button>
            </>
          )}
        </div>
        {saveProfileMutation.isError && (
          <div className="text-red-200 text-xs mt-1">
            Failed to update name
          </div>
        )}
      </div>

      {/* Balance section */}
      <div className="bg-gradient-to-r from-green-600 to-blue-600 px-4 pb-4 text-center">
        <div className="text-2xl mb-1">🎰</div>
        <div className="text-white font-bold text-lg flex items-center justify-center gap-2">
          Game Balance: {balanceLoading ? '...' : formatICP(walletBalance)} ICP
          <button 
            onClick={() => refetchBalance()}
            className="text-white/70 hover:text-white transition-colors"
            title="Refresh balance"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        </div>
        
        {/* External wallet balance (when not in test mode) */}
        {!isTestMode && (
          <div className="text-white/80 text-sm mt-1 flex items-center justify-center gap-2">
            Wallet: {externalBalanceLoading ? '...' : externalBalance !== null ? formatIcpBalance(externalBalance) : '—'} ICP
            <button 
              onClick={fetchExternalBalance}
              className="text-white/60 hover:text-white transition-colors"
              title="Refresh wallet balance"
            >
              <RefreshCw className="w-3 h-3" />
            </button>
          </div>
        )}
        
        <div className="text-white/90 text-sm mt-2">
          🎯 Ponzi Points: {ponziLoading ? '...' : ponziPoints.toLocaleString()} PP
        </div>
        
        {isTestMode && (
          <div className="text-yellow-300/80 text-xs mt-1">
            ⚠️ Test Mode - Using simulated ICP
          </div>
        )}
      </div>

      {/* Tabs for Deposit/Withdraw/Send */}
      <div className="p-4">
        <Tabs defaultValue="deposit" className="w-full">
          <TabsList className="grid w-full grid-cols-3 bg-gray-800 border border-gray-600">
            <TabsTrigger 
              value="deposit" 
              className="text-gray-300 data-[state=active]:bg-blue-600 data-[state=active]:text-white"
            >
              <ArrowDown className="w-4 h-4 mr-1" />
              Deposit
            </TabsTrigger>
            <TabsTrigger 
              value="withdraw" 
              className="text-gray-300 data-[state=active]:bg-green-600 data-[state=active]:text-white"
            >
              <ArrowUp className="w-4 h-4 mr-1" />
              Withdraw
            </TabsTrigger>
            <TabsTrigger 
              value="send" 
              className="text-gray-300 data-[state=active]:bg-purple-600 data-[state=active]:text-white"
            >
              Send
            </TabsTrigger>
          </TabsList>

          {/* Deposit Tab */}
          <TabsContent value="deposit" className="space-y-3 mt-4">
            {isTestMode ? (
              <div className="bg-yellow-900/30 border border-yellow-500/30 rounded-lg p-4 text-center">
                <p className="text-yellow-200 text-sm mb-2">
                  <strong>Test Mode Active</strong>
                </p>
                <p className="text-yellow-200/80 text-xs">
                  Deposits are simulated. Your balance starts at 500 ICP for testing.
                  Real ICP deposits will be enabled when test mode is disabled.
                </p>
              </div>
            ) : (
              <div className="space-y-3">
                {/* External wallet balance display */}
                <div className="bg-blue-900/30 border border-blue-500/30 rounded-lg p-3">
                  <div className="flex justify-between items-center text-sm">
                    <span className="text-blue-200">Your Wallet Balance:</span>
                    <span className="text-white font-medium">
                      {externalBalanceLoading ? '...' : externalBalance !== null ? formatIcpBalance(externalBalance) : '—'} ICP
                    </span>
                  </div>
                </div>

                {/* Deposit amount input */}
                <div>
                  <div className="flex justify-between items-center">
                    <Label className="text-gray-300 text-sm">Deposit Amount</Label>
                    <span className="text-gray-400 text-xs">Min: 0.1 ICP</span>
                  </div>
                  <div className="flex space-x-2 mt-1">
                    <Input
                      type="number"
                      value={depositAmount}
                      onChange={handleDepositAmountChange}
                      placeholder="0.0"
                      min="0.1"
                      step="0.00000001"
                      className="bg-gray-800 border-gray-600 text-gray-200 flex-1"
                    />
                    <Button
                      onClick={handleMaxDeposit}
                      variant="outline"
                      size="sm"
                      className="border-gray-600 text-gray-300 hover:bg-gray-700 px-3"
                      disabled={externalBalance === null}
                    >
                      MAX
                    </Button>
                  </div>
                </div>

                {/* Deposit flow explanation */}
                <div className="bg-gray-800/50 border border-gray-600 rounded-lg p-3">
                  <p className="text-gray-300 text-xs">
                    <strong>Deposit Flow:</strong>
                  </p>
                  <ol className="text-gray-400 text-xs mt-1 space-y-1 list-decimal list-inside">
                    <li className={approvalComplete ? 'text-green-400' : ''}>
                      {approvalComplete ? '✓ ' : ''}Approve transfer (wallet popup)
                    </li>
                    <li>Confirm deposit to Musical Chairs</li>
                  </ol>
                </div>

                {/* Deposit button */}
                <Button
                  onClick={handleDeposit}
                  disabled={!isDepositValid || isApproving || depositMutation.isPending}
                  className="w-full bg-blue-600 hover:bg-blue-700 text-white disabled:opacity-50"
                >
                  {isApproving ? (
                    <>
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      Approving...
                    </>
                  ) : depositMutation.isPending ? (
                    <>
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      Depositing...
                    </>
                  ) : approvalComplete ? (
                    <>
                      <ArrowDown className="w-4 h-4 mr-2" />
                      Confirm Deposit
                    </>
                  ) : (
                    <>
                      <ArrowDown className="w-4 h-4 mr-2" />
                      Approve & Deposit
                    </>
                  )}
                </Button>

                {depositMutation.isSuccess && (
                  <Alert className="bg-green-900/50 border-green-700">
                    <AlertDescription className="text-sm text-green-200">
                      ✅ Deposit successful!
                    </AlertDescription>
                  </Alert>
                )}
              </div>
            )}
          </TabsContent>

          {/* Withdraw Tab */}
          <TabsContent value="withdraw" className="space-y-3 mt-4">
            {isTestMode ? (
              <div className="bg-yellow-900/30 border border-yellow-500/30 rounded-lg p-4 text-center">
                <p className="text-yellow-200 text-sm mb-2">
                  <strong>Test Mode Active</strong>
                </p>
                <p className="text-yellow-200/80 text-xs">
                  Withdrawals to external wallets are disabled in test mode.
                  Use the "Send" tab to transfer within Musical Chairs.
                </p>
              </div>
            ) : (
              <div className="space-y-3">
                <div>
                  <div className="flex justify-between items-center">
                    <Label className="text-gray-300 text-sm">Withdraw Amount</Label>
                    <span className="text-gray-400 text-xs">
                      Available: {formatICP(walletBalance)} ICP
                    </span>
                  </div>
                  <div className="flex space-x-2 mt-1">
                    <Input
                      type="number"
                      value={withdrawAmount}
                      onChange={handleWithdrawAmountChange}
                      placeholder="0.0"
                      min="0.1"
                      max={walletBalance}
                      step="0.00000001"
                      className="bg-gray-800 border-gray-600 text-gray-200 flex-1"
                    />
                    <Button
                      onClick={handleMaxWithdraw}
                      variant="outline"
                      size="sm"
                      className="border-gray-600 text-gray-300 hover:bg-gray-700 px-3"
                    >
                      MAX
                    </Button>
                  </div>
                </div>

                <div className="bg-gray-800/50 border border-gray-600 rounded-lg p-3">
                  <p className="text-gray-400 text-xs">
                    Withdraws ICP from your game balance to your connected wallet.
                    A transfer fee of 0.0001 ICP will be deducted.
                  </p>
                </div>

                {withdrawAmountValue > walletBalance && (
                  <Alert variant="destructive" className="bg-red-900/50 border-red-700">
                    <AlertDescription className="text-sm text-red-200">
                      Insufficient balance
                    </AlertDescription>
                  </Alert>
                )}

                <Button
                  onClick={handleWithdraw}
                  disabled={!isWithdrawValid || withdrawMutation.isPending}
                  className="w-full bg-green-600 hover:bg-green-700 text-white disabled:opacity-50"
                >
                  {withdrawMutation.isPending ? (
                    <>
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      Withdrawing...
                    </>
                  ) : (
                    <>
                      <ArrowUp className="w-4 h-4 mr-2" />
                      Withdraw to Wallet
                    </>
                  )}
                </Button>

                {withdrawMutation.isSuccess && (
                  <Alert className="bg-green-900/50 border-green-700">
                    <AlertDescription className="text-sm text-green-200">
                      ✅ Withdrawal successful!
                    </AlertDescription>
                  </Alert>
                )}
              </div>
            )}
          </TabsContent>

          {/* Send Tab (Internal Transfer) */}
          <TabsContent value="send" className="space-y-3 mt-4">
            <div className="space-y-3">
              <div>
                <Label className="text-gray-300 text-sm">Recipient Principal</Label>
                <Input
                  type="text"
                  value={recipientPrincipal}
                  onChange={(e) => setRecipientPrincipal(e.target.value)}
                  placeholder="Enter recipient address"
                  className="font-mono text-sm bg-gray-800 border-gray-600 text-gray-200 mt-1"
                />
              </div>

              <div>
                <div className="flex justify-between items-center">
                  <Label className="text-gray-300 text-sm">Amount</Label>
                  <span className="text-gray-400 text-xs">
                    Available: {formatICP(walletBalance)} ICP
                  </span>
                </div>
                <div className="flex space-x-2 mt-1">
                  <Input
                    type="number"
                    value={withdrawAmount}
                    onChange={handleWithdrawAmountChange}
                    placeholder="0.0"
                    min="0.01"
                    max={walletBalance}
                    step="0.00000001"
                    className="bg-gray-800 border-gray-600 text-gray-200 flex-1"
                  />
                  <Button
                    onClick={handleMaxWithdraw}
                    variant="outline"
                    size="sm"
                    className="border-gray-600 text-gray-300 hover:bg-gray-700 px-3"
                  >
                    MAX
                  </Button>
                </div>
              </div>

              {inputError && (
                <div className="text-red-400 text-xs">
                  {inputError}
                </div>
              )}

              {withdrawAmountValue > walletBalance && !inputError && (
                <Alert variant="destructive" className="bg-red-900/50 border-red-700">
                  <AlertDescription className="text-sm text-red-200">
                    Insufficient balance
                  </AlertDescription>
                </Alert>
              )}

              <Button
                onClick={handleSendInternal}
                disabled={!isSendValid || sendMutation.isPending}
                className="w-full bg-purple-600 hover:bg-purple-700 text-white disabled:opacity-50"
              >
                {sendMutation.isPending ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    Sending...
                  </>
                ) : (
                  'Send ICP'
                )}
              </Button>

              {sendMutation.isError && (
                <Alert variant="destructive" className="bg-red-900/50 border-red-700">
                  <AlertDescription className="text-sm text-red-200">
                    {sendMutation.error?.message || 'Transfer failed'}
                  </AlertDescription>
                </Alert>
              )}

              {sendMutation.isSuccess && (
                <Alert className="bg-green-900/50 border-green-700">
                  <AlertDescription className="text-sm text-green-200">
                    ✅ Transfer successful!
                  </AlertDescription>
                </Alert>
              )}
            </div>
          </TabsContent>
        </Tabs>

        {/* Principal ID section */}
        <div className="mt-4 pt-4 border-t border-gray-700">
          <Label className="text-gray-400 text-xs">Your Principal ID:</Label>
          <div className="flex space-x-2 mt-1">
            <Input
              value={principalId}
              readOnly
              className="font-mono text-xs bg-gray-800 border-gray-600 text-gray-400 flex-1"
            />
            <Button
              onClick={copyPrincipalToClipboard}
              size="sm"
              className="bg-gray-700 hover:bg-gray-600 text-white px-3"
            >
              {copied ? <Check className="w-4 h-4" /> : <Copy className="w-4 h-4" />}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
