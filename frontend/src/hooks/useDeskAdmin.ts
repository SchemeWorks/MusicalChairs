/** Founder's Allocation desk — admin (Charles) hooks. Reads via the anon actor,
 *  mutations via the SIWS/II-authed actor; deposit also approves on pp_ledger. */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Principal } from '@dfinity/principal';
import { toast } from 'sonner';
import { useReadPonziMathSol } from './useReadPonziMathSol';
import { usePonziMathSolActor } from './usePonziMathSolActor';
import { useAuthPpLedger } from './usePpLedger';
import type { DeskTier } from '../declarations/ponzi_math_sol';

const PONZI_MATH_SOL_CANISTER_ID = 'spc6q-xyaaa-aaaac-qg2ma-cai';

const invalidate = (qc: ReturnType<typeof useQueryClient>) => {
  qc.invalidateQueries({ queryKey: ['deskTiers'] });
  qc.invalidateQueries({ queryKey: ['deskStats'] });
  qc.invalidateQueries({ queryKey: ['ppBalances'] });
};

export function useDeskTiers() {
  const actor = useReadPonziMathSol();
  return useQuery<DeskTier[]>({
    queryKey: ['deskTiers'],
    queryFn: () => actor.deskListTiers(),
    refetchInterval: 15_000,
  });
}

export function useDeskStats() {
  const actor = useReadPonziMathSol();
  return useQuery({
    queryKey: ['deskStats'],
    queryFn: () => actor.deskStats(),
    refetchInterval: 15_000,
  });
}

export function useDeskAddTier() {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<bigint, Error, { rateUnits: bigint; qtyUnits: bigint }>({
    mutationFn: async ({ rateUnits, qtyUnits }) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await actor.deskAddTier(rateUnits, qtyUnits);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

export function useDeskUpdateTier() {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<null, Error, { index: bigint; rateUnits: bigint; qtyUnits: bigint }>({
    mutationFn: async ({ index, rateUnits, qtyUnits }) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await actor.deskUpdateTier(index, rateUnits, qtyUnits);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

export function useDeskRemoveTier() {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<null, Error, { index: bigint }>({
    mutationFn: async ({ index }) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await actor.deskRemoveTier(index);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

export function useDeskWithdrawInventory() {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<bigint, Error, { units: bigint; to: Principal }>({
    mutationFn: async ({ units, to }) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await actor.deskWithdrawInventory(units, to);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

export function useWithdrawDeskProceeds() {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<string, Error, { toAddress: string }>({
    mutationFn: async ({ toAddress }) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await actor.adminWithdrawDeskProceeds(toAddress);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

/** Deposit PP into the desk escrow: approve ponzi_math_sol on pp_ledger, then deskDepositInventory. */
export function useDeskDepositInventory() {
  const { actor } = usePonziMathSolActor();
  const ledger = useAuthPpLedger();
  const qc = useQueryClient();
  return useMutation<bigint, Error, { units: bigint }>({
    mutationFn: async ({ units }) => {
      if (!actor || !ledger) throw new Error('Admin wallet not connected');
      const approve = await ledger.icrc2_approve({
        from_subaccount: [],
        spender: { owner: Principal.fromText(PONZI_MATH_SOL_CANISTER_ID), subaccount: [] },
        amount: units,
        expected_allowance: [],
        expires_at: [],
        fee: [],
        memo: [],
        created_at_time: [],
      });
      if ('Err' in approve) throw new Error('Approve failed: ' + JSON.stringify(approve.Err));
      const res = await actor.deskDepositInventory(units);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Deposit failed', { description: e.message }),
  });
}
