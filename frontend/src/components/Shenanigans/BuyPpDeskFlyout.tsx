/** Founder's Allocation — buy loose PP with SOL from Charles's desk.
 *  Replaces BuySOLFlyout in the SIWS sidebar slot. Lifecycle:
 *  quote → lock (createBuyIntent) → pay (address+QR+countdown) → credited. */
import { useEffect, useMemo, useState } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, ArrowRight, X } from 'lucide-react';
import { useWallet } from '../../hooks/useWallet';
import { useAllowance, useApproveForDeposits, useDepositChips, useGetPonziPoints } from '../../hooks/useQueries';
import { useQuoteBuyPP, useCreateBuyIntent, useGetMyPendingBuyIntents, type CreateBuyIntentResult } from '../../hooks/useBuyPpDesk';
import { formatSOL, parseSOL } from '../../solana/lamports';
import { formatPpUnits, effectiveRatePer0_1Sol, formatCountdown } from '../../lib/ppDesk';
import LoadingSpinner from '../LoadingSpinner';

interface Props { onClose?: () => void; variant?: 'widget' | 'sheet'; }

export default function BuyPpDeskFlyout({ onClose, variant = 'widget' }: Props) {
  const { isConnected, principal } = useWallet();
  const [solInput, setSolInput] = useState('');
  const [locked, setLocked] = useState<(CreateBuyIntentResult & { quotedLamports: bigint; walletBefore: number }) | null>(null);
  const [copied, setCopied] = useState(false);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const [justBought, setJustBought] = useState<number>(0);
  const [pendingCredit, setPendingCredit] = useState<{ walletBefore: number; until: number } | null>(null);

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);

  const { data: ppBalances } = useGetPonziPoints();
  const { data: quote, isFetching: quoteFetching } = useQuoteBuyPP(locked ? 0n : lamports);
  const createIntent = useCreateBuyIntent();
  const { data: pendingIntents } = useGetMyPendingBuyIntents();

  useEffect(() => {
    if (!locked) return;
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [locked]);

  useEffect(() => {
    if (!locked || !pendingIntents) return;
    const stillOpen = pendingIntents.some((bi) => bi.id === locked.intentId);
    if (!stillOpen) {
      setPendingCredit({ walletBefore: locked.walletBefore, until: Date.now() + 30_000 });
      setLocked(null);
      setSolInput('');
    }
  }, [pendingIntents, locked]);

  useEffect(() => {
    if (!pendingCredit || !ppBalances) return;
    const delta = ppBalances.walletPoints - pendingCredit.walletBefore;
    if (delta > 0) { setJustBought(delta); setPendingCredit(null); }
    else if (Date.now() > pendingCredit.until) { setPendingCredit(null); } // expired/unfilled: no false success
  }, [ppBalances, pendingCredit]);

  const { data: allowance } = useAllowance();
  const approveDeposits = useApproveForDeposits();
  const depositChips = useDepositChips();
  const depositPending = approveDeposits.isPending || depositChips.isPending;
  const handleDeposit = async () => {
    try {
      const neededUnits = BigInt(Math.round(justBought * 1e8));
      const have = (allowance?.allowance ?? 0n) >= neededUnits;
      if (!have) await approveDeposits.mutateAsync(undefined);
      await depositChips.mutateAsync(justBought);
      setJustBought(0);
    } catch { /* toasts surface failure; keep prompt for retry */ }
  };

  const handleLock = async () => {
    if (lamports <= 0n) return;
    if (createIntent.isPending) return;
    const res = await createIntent.mutateAsync(lamports).catch(() => null);
    if (res) { setLocked({ ...res, quotedLamports: lamports, walletBefore: ppBalances?.walletPoints ?? 0 }); setNowMs(Date.now()); }
  };

  const handleCopy = async () => {
    if (!locked) return;
    await navigator.clipboard.writeText(locked.depositAddress);
    setCopied(true); setTimeout(() => setCopied(false), 1500);
  };

  const msRemaining = locked ? Number(locked.expiresAt / 1_000_000n) - nowMs : 0;
  const expired = locked != null && msRemaining <= 0;
  const qrPayload = locked ? `solana:${locked.depositAddress}?amount=${formatSOL(locked.quotedLamports)}` : null;
  const canLock = isConnected && !!principal && lamports > 0n && !!quote && quote.ppUnitsOut > 0n && !createIntent.isPending && ppBalances !== undefined;

  return (
    <div className={variant === 'sheet' ? 'p-5' : ''}>
      <h2 className="font-display text-lg mc-text-primary mb-2">Founder's Allocation</h2>
      <div className="mc-status-amber p-3 mb-4 text-xs font-bold">
        ⚠️ DEVNET SOL ONLY — the canister polls Solana devnet. Mainnet SOL sent here is lost.
      </div>

      {justBought > 0 && (
        <div className="mc-buy-pp-deposit-prompt">
          <div className="flex items-start gap-2 mb-2">
            <Check className="h-4 w-4 mc-text-gold flex-shrink-0 mt-0.5" />
            <div className="flex-1 leading-tight">
              <div className="text-sm font-bold mc-text-primary">Bought {justBought.toLocaleString()} PP</div>
              <div className="text-[11px] mc-text-muted mt-0.5">Deposit to your side pocket to use them for shenanigans.</div>
            </div>
            <button type="button" onClick={() => setJustBought(0)} className="p-1 rounded hover:bg-white/5 mc-text-muted -mt-1 -mr-1" aria-label="Keep in wallet"><X className="h-3 w-3" /></button>
          </div>
          <button type="button" onClick={handleDeposit} disabled={depositPending} className="mc-buy-pp-deposit-button">
            {depositPending ? <LoadingSpinner /> : (<><span>Deposit {justBought.toLocaleString()} PP → Side Pocket</span><ArrowRight className="h-4 w-4" /></>)}
          </button>
        </div>
      )}

      {!locked ? (
        <>
          <div className="mc-buy-pp-input-row">
            <input type="text" inputMode="decimal" placeholder="0.0" value={solInput}
              onChange={(e) => setSolInput(e.target.value)} className="mc-buy-pp-input font-mono" />
            <span className="mc-buy-pp-input-suffix">SOL</span>
          </div>
          <div className="mc-buy-pp-quote">
            <span className="text-[11px] mc-text-muted uppercase tracking-wider">You receive</span>
            <div className="mc-buy-pp-quote-amount">
              {lamports <= 0n ? <span className="mc-text-muted">—</span>
                : quote && quote.ppUnitsOut > 0n ? <>~{formatPpUnits(quote.ppUnitsOut)} <span className="text-sm mc-text-muted">PP</span></>
                : quoteFetching ? <span className="mc-text-muted">…</span>
                : <span className="mc-text-muted">Desk is out of stock</span>}
            </div>
            {quote && quote.ppUnitsOut > 0n && (
              <div className="flex items-center justify-between text-[10px] mc-text-muted mt-1">
                <span>≈ {effectiveRatePer0_1Sol(quote.ppUnitsOut, lamports)} PP / 0.1 SOL</span>
                {quote.cappedByInventory && <span className="mc-text-gold">limited stock</span>}
              </div>
            )}
          </div>
          <button type="button" onClick={handleLock} disabled={!canLock} className="mc-buy-pp-button">
            {createIntent.isPending ? <LoadingSpinner /> : <span>Lock & Buy</span>}
          </button>
          {!isConnected && <div className="text-[10px] mc-text-muted mt-2 text-center">Connect a wallet to buy</div>}
        </>
      ) : (
        <div className="mc-card p-3 space-y-2">
          <div className="text-xs mc-text-muted">
            Send <span className="mc-text-gold font-bold">{formatSOL(locked.quotedLamports)} SOL</span> to receive <span className="mc-text-gold font-bold">{formatPpUnits(locked.ppUnitsReserved)} PP</span>.
          </div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={locked.depositAddress}>{locked.depositAddress}</code>
            <button onClick={handleCopy} className="mc-btn-secondary text-xs">{copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}</button>
          </div>
          {qrPayload && <div className="flex justify-center pt-2"><QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" /></div>}
          <div className={`text-center text-xs ${expired ? 'mc-text-danger' : 'mc-text-muted'}`}>
            {expired ? 'Quote expired — start over for a fresh price.' : `Reserved for ${formatCountdown(msRemaining)} · PP arrives ~1 min after you send.`}
          </div>
          <button type="button" onClick={() => { setLocked(null); setSolInput(''); }} className="mc-btn-secondary w-full">{expired ? 'Get a new quote' : 'Start over'}</button>
        </div>
      )}

      {onClose && <button onClick={onClose} className="mc-btn-secondary w-full mt-3">Close</button>}
    </div>
  );
}
