/** Founder's Allocation desk — buyer hooks. Mirrors usePartyDexBuy / the SOL
 *  deposit hooks: anon debounced quote, auth lock mutation, auth pending poll. */
import { useEffect, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useReadPonziMathSol } from './useReadPonziMathSol';
import { usePonziMathSolActor } from './usePonziMathSolActor';
import { useWallet } from './useWallet';
import type { DeskQuote, BuyIntent } from '../declarations/ponzi_math_sol';

const QUOTE_DEBOUNCE_MS = 300;

export function useQuoteBuyPP(lamports: bigint) {
  const actor = useReadPonziMathSol();
  const [debounced, setDebounced] = useState<bigint>(lamports);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(lamports), QUOTE_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [lamports]);

  return useQuery<DeskQuote | null>({
    queryKey: ['ppDeskQuote', debounced.toString()],
    queryFn: async () => {
      if (debounced <= 0n) return null;
      return actor.quoteBuyPP(debounced);
    },
    enabled: debounced > 0n,
    staleTime: 5_000,
    refetchInterval: 15_000,
  });
}

export interface CreateBuyIntentResult {
  intentId: bigint;
  depositAddress: string;
  ppUnitsReserved: bigint;
  expiresAt: bigint;
}

export function useCreateBuyIntent() {
  const { actor } = usePonziMathSolActor();
  const queryClient = useQueryClient();
  return useMutation<CreateBuyIntentResult, Error, bigint>({
    mutationFn: async (lamports: bigint) => {
      if (!actor) throw new Error('Wallet not connected');
      const res = await actor.createBuyIntent(lamports);
      if ('Err' in res) throw new Error(res.Err);
      return {
        intentId: res.Ok.intentId,
        depositAddress: res.Ok.depositAddress,
        ppUnitsReserved: res.Ok.ppUnitsReserved,
        expiresAt: res.Ok.expiresAt,
      };
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ppDeskPendingIntents'] });
    },
    onError: (e) => {
      toast.error('Could not lock your buy', { description: e.message });
    },
  });
}

export function useGetMyPendingBuyIntents() {
  const { actor } = usePonziMathSolActor();
  const { principal, walletType } = useWallet();
  return useQuery<BuyIntent[]>({
    queryKey: ['ppDeskPendingIntents', principal],
    queryFn: async () => {
      if (!actor) return [];
      return actor.getMyPendingBuyIntents();
    },
    enabled: walletType === 'siws' && !!actor && !!principal,
    refetchInterval: 10_000,
  });
}
