/**
 * PartyDEX buy hooks:
 *   useQuotePP(icpE8s)  — debounced live quote via quote_trade (free query, anonymous)
 *   useBuyPP()          — mutation: optional icrc2_approve → non_atomic_trade
 *
 * Fast Buy preference is persisted in localStorage. When ON we approve MAX_NAT
 * once (next buy only requires one popup); when OFF we approve (amount + fee)
 * each buy (two popups, exact spend authorization).
 */

import { useEffect, useState, useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useWallet } from './useWallet';
import { useLedger } from './useLedger';
import {
  getPartyDexReadActor,
  createPartyDexAuthedActor,
  getPartyDexSpenderPrincipal,
  LIMIT_TICK_NONE,
  DEFAULT_SLIPPAGE_BPS,
  MAX_NAT_E8S,
  QuoteResult,
} from '../lib/partyDex';

const FAST_BUY_STORAGE_KEY = 'mc.fastBuyPP';
const QUOTE_DEBOUNCE_MS = 300;

// PartyDEX enforces a USD-denominated minimum trade size (~$1). We pad it to
// $1.02 in our local check so we never get caught by rounding right at the
// boundary. Tracked in cents to avoid float drift in the conversion.
export const PARTYDEX_MIN_TRADE_USD_CENTS = 102n;

// Convert PartyDEX's quote_usd_rate_e12 (USD value of 1 natural ICP, × 10^12)
// to the minimum ICP-e8s amount that clears $1.02. Math:
//   min_icp_e8s = ceil(1.02 * 10^8 / (rate_e12 / 10^12))
//              = ceil(102 * 10^18 / rate_e12)     [factoring out the cents]
// Using bigint arithmetic to keep precision.
export function minICPE8sForMinTrade(quoteUsdRateE12: bigint): bigint {
  if (quoteUsdRateE12 <= 0n) return 0n;
  const numerator = PARTYDEX_MIN_TRADE_USD_CENTS * 10n ** 18n;
  // ceil division
  return (numerator + quoteUsdRateE12 - 1n) / quoteUsdRateE12;
}

// ---------------------------------------------------------------------------
// Fast Buy preference (default ON per design)
// ---------------------------------------------------------------------------

export function useFastBuyPreference(): [boolean, (v: boolean) => void] {
  const [enabled, setEnabled] = useState<boolean>(() => {
    if (typeof window === 'undefined') return true;
    const raw = window.localStorage.getItem(FAST_BUY_STORAGE_KEY);
    return raw === null ? true : raw === 'true';
  });

  const update = useCallback((v: boolean) => {
    setEnabled(v);
    try {
      window.localStorage.setItem(FAST_BUY_STORAGE_KEY, String(v));
    } catch {
      // localStorage may be unavailable (private mode, etc.) — preference is
      // ephemeral in that case, which is fine.
    }
  }, []);

  return [enabled, update];
}

// ---------------------------------------------------------------------------
// ICP/USD rate — polled from PartyDEX's get_routing_state. Used to compute
// the minimum trade size in ICP so the user can't click BUY on an amount
// PartyDEX would reject. Refreshes every 30s.
// ---------------------------------------------------------------------------

export function useICPUsdRate() {
  return useQuery({
    queryKey: ['partyDexQuoteUsdRate'],
    queryFn: async () => {
      const actor = getPartyDexReadActor();
      const state = await actor.get_routing_state();
      return state.quote_usd_rate_e12;
    },
    staleTime: 30_000,
    refetchInterval: 30_000,
  });
}

// ---------------------------------------------------------------------------
// Live quote — debounced to keep query traffic sane while the user types.
// quote_trade is a query call (free, ~80ms), so refetch on every change is OK
// but unnecessary; 300ms debounce smooths the experience.
// ---------------------------------------------------------------------------

export function useQuotePP(icpE8s: bigint) {
  const [debounced, setDebounced] = useState<bigint>(icpE8s);

  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(icpE8s), QUOTE_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [icpE8s]);

  return useQuery<QuoteResult | null>({
    queryKey: ['ppQuote', debounced.toString()],
    queryFn: async () => {
      if (debounced <= 0n) return null;
      const actor = getPartyDexReadActor();
      const result = await actor.quote_trade(
        { buy: null },
        debounced,
        [LIMIT_TICK_NONE],
        [DEFAULT_SLIPPAGE_BPS],
      );
      if ('err' in result) {
        // Surface validation/state errors quietly — UI will show "no quote".
        return null;
      }
      return result.ok;
    },
    staleTime: 5_000,
    refetchInterval: 10_000,
    enabled: debounced > 0n,
  });
}

// ---------------------------------------------------------------------------
// Buy mutation
// ---------------------------------------------------------------------------

interface BuyArgs {
  icpE8s: bigint;
  quote: QuoteResult;
  fastBuy: boolean;
}

export function useBuyPP() {
  const queryClient = useQueryClient();
  const { identity, walletType, principal } = useWallet();
  const ledger = useLedger();

  return useMutation({
    mutationFn: async ({ icpE8s, quote, fastBuy }: BuyArgs) => {
      if (!principal) throw new Error('Wallet not connected');

      // Allowance check — skip the approve popup if we already have headroom.
      // PartyDEX pulls (icpE8s + ledger fee) via transfer_from, so we need at
      // least that much remaining allowance.
      const spender = getPartyDexSpenderPrincipal();
      const needed = icpE8s + ledger.ICP_TRANSFER_FEE;
      const current = await ledger.getAllowance(spender.toText());

      if (current.allowance < needed) {
        const approvalAmount = fastBuy ? MAX_NAT_E8S : needed;
        const approveResult = await ledger.approve(spender.toText(), approvalAmount);
        if ('Err' in approveResult) {
          throw new Error(`Approval failed: ${JSON.stringify(approveResult.Err)}`);
        }
      }

      // Execute the trade. PartyDEX pulls ICP from caller via transfer_from,
      // executes the route, and pushes PP back to caller's wallet in one call.
      const partyDex = await createPartyDexAuthedActor({ walletType, identity });
      const tradeResult = await partyDex.non_atomic_trade({
        book_orders: quote.book_orders,
        pool_swaps: quote.pool_swaps,
        min_output: [], // We already passed slippage_bps in the quote; no extra floor.
        allow_partial: true,
      });

      if ('err' in tradeResult) {
        throw new Error(tradeResult.err.message || 'Trade rejected');
      }

      // Check outbound transfer legs. In the happy path one of base/quote will be
      // `transferred`; the other `not_attempted`. A `failed` arm means PP is
      // stuck in the user's PartyDEX trading balance — recoverable via withdraw
      // (not implemented here; surfaced via toast for now so the user knows).
      const { base, quote: refundQuote } = tradeResult.ok.output;
      let ppCredited = 0n;
      if ('transferred' in base) ppCredited = base.transferred.amount;
      if ('failed' in base) {
        throw new Error(
          `PP swap succeeded but the transfer to your wallet failed (${base.failed.code}). ` +
            'Your PP is safe on PartyDEX — contact support to recover.',
        );
      }
      // refund leg is for ICP returned on partial fills; surface but don't fail.
      void refundQuote;

      return { ppCredited };
    },

    onSuccess: ({ ppCredited }) => {
      const ppDisplay = (Number(ppCredited) / 1e8).toLocaleString(undefined, {
        maximumFractionDigits: 2,
      });
      toast.success(`Bought ${ppDisplay} PP`, {
        description: 'Your bag is heavier. Use it wisely (or don\'t).',
      });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.invalidateQueries({ queryKey: ['icpLedgerBalance'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
    },

    onError: (error: Error) => {
      toast.error('Buy failed', { description: error.message });
    },
  });
}

// ---------------------------------------------------------------------------
// Revoke — sets PartyDEX allowance back to 0 (one popup).
// ---------------------------------------------------------------------------

export function useRevokePartyDexApproval() {
  const ledger = useLedger();
  return useMutation({
    mutationFn: async () => {
      const spender = getPartyDexSpenderPrincipal();
      const result = await ledger.approve(spender.toText(), 0n);
      if ('Err' in result) throw new Error(JSON.stringify(result.Err));
    },
    onSuccess: () => {
      toast.success('Fast Buy revoked', {
        description: 'Next purchase will require a fresh approval.',
      });
    },
    onError: (error: Error) => {
      toast.error('Revoke failed', { description: error.message });
    },
  });
}
