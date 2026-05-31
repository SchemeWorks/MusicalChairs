/**
 * BuyPPFlyout — the impulse-buy form shared by the desktop sidebar widget and
 * the mobile bottom-left FAB. Type ICP amount → see live PP quote → BUY.
 *
 * Fast Buy toggle defaults ON: approve MAX_NAT once, every subsequent buy is a
 * single popup. Toggle OFF for per-buy exact-amount approval (two popups each).
 */

import { useEffect, useMemo, useState } from 'react';
import { Zap, ZapOff, X, ArrowRight, Check } from 'lucide-react';
import { useWallet } from '../../hooks/useWallet';
import { useICPBalance, useAllowance, useApproveForDeposits, useDepositChips } from '../../hooks/useQueries';
import { ICP_TRANSFER_FEE } from '../../hooks/useLedger';
import {
  useBuyPP,
  useQuotePP,
  useFastBuyPreference,
  useRevokePartyDexApproval,
  useICPUsdRate,
  minICPE8sForMinTrade,
} from '../../hooks/usePartyDexBuy';
import LoadingSpinner from '../LoadingSpinner';

interface Props {
  onClose?: () => void; // mobile sheet uses this; desktop widget omits
  variant?: 'widget' | 'sheet';
}

const E8S = 100_000_000n;

function icpToE8s(icp: number): bigint {
  if (!Number.isFinite(icp) || icp <= 0) return 0n;
  return BigInt(Math.round(icp * 1e8));
}

function formatPP(e8s: bigint): string {
  return (Number(e8s) / 1e8).toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function formatRate(input: bigint, output: bigint): string {
  if (input <= 0n) return '—';
  const rate = Number(output) / Number(input);
  return rate.toLocaleString(undefined, { maximumFractionDigits: 0 });
}

function formatIcpFee(e8s: bigint): string {
  return (Number(e8s) / 1e8).toFixed(4);
}

function formatIcpAmount(e8s: bigint): string {
  return (Number(e8s) / 1e8).toFixed(4).replace(/\.?0+$/, '');
}

export default function BuyPPFlyout({ onClose, variant = 'widget' }: Props) {
  const { isConnected, principal } = useWallet();
  const { data: icpBalance } = useICPBalance();
  const [icpInput, setIcpInput] = useState<string>('');
  const [fastBuy, setFastBuy] = useFastBuyPreference();

  const icpE8s = useMemo(() => {
    const parsed = parseFloat(icpInput);
    return icpToE8s(parsed);
  }, [icpInput]);

  const { data: quote, isFetching: quoteFetching, isError: quoteError } = useQuotePP(icpE8s);
  const { data: quoteUsdRateE12 } = useICPUsdRate();
  const buy = useBuyPP();
  const revoke = useRevokePartyDexApproval();

  // Post-buy "Deposit to Side Pocket" mode — wallet PP isn't usable for
  // shenanigans until it's deposited into the side pocket. After a successful buy we flip the widget
  // into a single-action deposit prompt with the amount pre-filled. The user
  // can deposit in one tap (or two if no allowance yet) or dismiss back to
  // the normal buy form to top up more.
  const [justBoughtPpUnits, setJustBoughtPpUnits] = useState<bigint>(0n);
  const { data: allowance } = useAllowance();
  const approveDeposits = useApproveForDeposits();
  const depositChips = useDepositChips();

  // When the buy mutation resolves, capture the credited amount so we can
  // show the deposit prompt. ppCredited is in PP units (8 decimals).
  useEffect(() => {
    if (buy.isSuccess && buy.data?.ppCredited && buy.data.ppCredited > 0n) {
      setJustBoughtPpUnits(buy.data.ppCredited);
      // Clear the input so the form is fresh when the user goes back to "buy more".
      setIcpInput('');
      // Reset the mutation so isSuccess won't re-fire if the user revisits.
      buy.reset();
    }
  }, [buy.isSuccess, buy.data, buy]);

  const minICPE8s = quoteUsdRateE12 ? minICPE8sForMinTrade(quoteUsdRateE12) : 0n;
  const belowMin = icpE8s > 0n && minICPE8s > 0n && icpE8s < minICPE8s;
  const maxICP = Math.max(0, (icpBalance ?? 0) - 0.0002); // leave 2 ledger fees behind
  const insufficientBalance = (icpBalance ?? 0) > 0 && parseFloat(icpInput) > maxICP;
  const canBuy =
    isConnected &&
    !!principal &&
    icpE8s > 0n &&
    !!quote &&
    !buy.isPending &&
    !insufficientBalance &&
    !belowMin;

  const handleMax = () => {
    setIcpInput(maxICP > 0 ? maxICP.toFixed(4).replace(/\.?0+$/, '') : '');
  };

  const handleBuy = () => {
    if (!quote) return;
    buy.mutate({ icpE8s, quote, fastBuy });
  };

  // Deposit the just-bought PP into the side-pocket subaccount. Mirrors the BuyPP
  // approval pattern: check existing allowance, only fire icrc2_approve if
  // we don't have headroom. wholePpToUnits is what useDepositChips expects,
  // but we already have units from the buy result — convert to whole PP.
  const justBoughtWhole = Number(justBoughtPpUnits) / 1e8;
  const depositPending = approveDeposits.isPending || depositChips.isPending;

  const handleDeposit = async () => {
    try {
      const haveAllowance = (allowance?.allowance ?? 0n) >= justBoughtPpUnits;
      if (!haveAllowance) {
        await approveDeposits.mutateAsync(undefined); // default: unlimited (matches Fast Buy semantics)
      }
      await depositChips.mutateAsync(justBoughtWhole);
      setJustBoughtPpUnits(0n);
    } catch {
      // Toasts surface the failure; keep the deposit prompt up so the user
      // can retry without re-buying.
    }
  };

  const handleKeepInWallet = () => {
    setJustBoughtPpUnits(0n);
  };

  const headerCls =
    variant === 'sheet'
      ? 'flex items-center justify-between mb-4'
      : 'flex items-center gap-2 mb-3';

  return (
    <div className={variant === 'sheet' ? 'p-5' : ''}>
      <div className={headerCls}>
        <div className="flex items-center gap-2">
          <img
            src="/pp-coin.png"
            alt="PP"
            className="w-8 h-8 mc-buy-pp-coin"
            draggable={false}
          />
          <div className="leading-tight">
            <div className="text-xs mc-text-muted uppercase tracking-wider">Founder's Allocation</div>
            <div className="text-base font-bold mc-text-primary">Buy PP</div>
          </div>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded hover:bg-white/5 mc-text-muted"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      {/* Post-buy deposit prompt. Wallet PP isn't usable for shenanigans until
          it's deposited into the side pocket, so we make the deposit step hard to ignore
          right after a successful buy. Form remains below for "buy more". */}
      {justBoughtPpUnits > 0n && (
        <div className="mc-buy-pp-deposit-prompt">
          <div className="flex items-start gap-2 mb-2">
            <Check className="h-4 w-4 mc-text-gold flex-shrink-0 mt-0.5" />
            <div className="flex-1 leading-tight">
              <div className="text-sm font-bold mc-text-primary">
                Bought {formatPP(justBoughtPpUnits)} PP
              </div>
              <div className="text-[11px] mc-text-muted mt-0.5">
                Deposit to your side pocket to use them for shenanigans.
              </div>
            </div>
            <button
              type="button"
              onClick={handleKeepInWallet}
              className="p-1 rounded hover:bg-white/5 mc-text-muted -mt-1 -mr-1"
              aria-label="Keep in wallet"
              title="Keep in wallet"
            >
              <X className="h-3 w-3" />
            </button>
          </div>
          <button
            type="button"
            onClick={handleDeposit}
            disabled={depositPending}
            className="mc-buy-pp-deposit-button"
          >
            {depositPending ? (
              <LoadingSpinner />
            ) : (
              <>
                <span>Deposit {formatPP(justBoughtPpUnits)} PP → Side Pocket</span>
                <ArrowRight className="h-4 w-4" />
              </>
            )}
          </button>
        </div>
      )}

      <div className="mc-buy-pp-input-row">
        <input
          type="number"
          inputMode="decimal"
          min={0}
          step="0.01"
          placeholder="0.00"
          value={icpInput}
          onChange={(e) => setIcpInput(e.target.value)}
          className="mc-buy-pp-input"
        />
        <span className="mc-buy-pp-input-suffix">ICP</span>
        <button
          type="button"
          onClick={handleMax}
          disabled={!isConnected || maxICP <= 0}
          className="mc-buy-pp-max"
        >
          MAX
        </button>
      </div>

      {isConnected && (
        <div className="text-[10px] mc-text-muted mt-1 text-right">
          Balance: {(icpBalance ?? 0).toFixed(4)} ICP
        </div>
      )}

      <div className="mc-buy-pp-quote">
        <div className="flex items-baseline justify-between">
          <span className="text-[11px] mc-text-muted uppercase tracking-wider">You receive</span>
          {quoteFetching && icpE8s > 0n && (
            <span className="text-[10px] mc-text-muted">refreshing…</span>
          )}
        </div>
        <div className="mc-buy-pp-quote-amount">
          {icpE8s <= 0n ? (
            <span className="mc-text-muted">—</span>
          ) : quoteError || (icpE8s > 0n && !quote && !quoteFetching) ? (
            <span className="mc-text-muted">No route</span>
          ) : quote ? (
            <>
              ~{formatPP(quote.output_amount)} <span className="text-sm mc-text-muted">PP</span>
            </>
          ) : (
            <span className="mc-text-muted">…</span>
          )}
        </div>
        {quote && (
          <div className="flex items-center justify-between text-[10px] mc-text-muted mt-1">
            <span>Market: {formatRate(quote.input_amount, quote.output_amount)} PP/ICP</span>
            <span>Fees: {formatIcpFee(quote.total_fees)} ICP</span>
          </div>
        )}
      </div>

      <button
        type="button"
        onClick={handleBuy}
        disabled={!canBuy}
        className="mc-buy-pp-button"
      >
        {buy.isPending ? (
          <LoadingSpinner />
        ) : (
          <>
            <Zap className="h-4 w-4" />
            <span>BUY PP NOW</span>
          </>
        )}
      </button>

      {insufficientBalance && (
        <div className="text-[10px] mc-text-danger mt-2 text-center">
          Not enough ICP (need to leave 0.0002 ICP for ledger fees)
        </div>
      )}

      {!insufficientBalance && belowMin && (
        <div className="text-[10px] mc-text-danger mt-2 text-center">
          PartyDEX minimum: {formatIcpAmount(minICPE8s)} ICP (~$1.02)
        </div>
      )}

      {!insufficientBalance && !belowMin && minICPE8s > 0n && icpE8s === 0n && (
        <div className="text-[10px] mc-text-muted mt-2 text-center">
          Minimum buy: {formatIcpAmount(minICPE8s)} ICP (~$1.02)
        </div>
      )}

      {!isConnected && (
        <div className="text-[10px] mc-text-muted mt-2 text-center">
          Connect a wallet to buy
        </div>
      )}

      <div className="mc-buy-pp-fast-row">
        <button
          type="button"
          onClick={() => setFastBuy(!fastBuy)}
          className={`mc-buy-pp-fast-toggle ${fastBuy ? 'on' : 'off'}`}
          title={
            fastBuy
              ? 'Approve once, skip popup next purchase. Click to require per-purchase approval.'
              : 'Each purchase requires approval. Click for one-time max approval (faster).'
          }
        >
          {fastBuy ? <Zap className="h-3 w-3" /> : <ZapOff className="h-3 w-3" />}
          <span>Fast Buy {fastBuy ? 'ON' : 'OFF'}</span>
        </button>
        {fastBuy && isConnected && (
          <button
            type="button"
            onClick={() => revoke.mutate()}
            disabled={revoke.isPending}
            className="mc-buy-pp-revoke"
          >
            {revoke.isPending ? '…' : 'revoke'}
          </button>
        )}
      </div>
      <div className="mc-buy-pp-fast-blurb">
        {fastBuy
          ? 'Approve once for easy purchases.'
          : 'Confirm each purchase manually.'}
      </div>
    </div>
  );
}
