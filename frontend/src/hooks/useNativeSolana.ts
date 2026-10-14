// React Query hooks over the native Solana adapter (frontend/src/solana/native).
//
// These sit BESIDE the legacy SOL hooks in useQueries.ts (useGetUserSolGames,
// useGetSolBackerPositions ... = the ponzi_math_sol canister path), which keep
// working untouched. Native query keys all start with 'native', so nothing here
// can alias or invalidate a legacy cache entry, and useNativeWalletPositions
// lists legacy and native positions as separate entries (never merged/summed).
//
// Bundle: this file imports only web3-free modules statically (types,
// selectors, economics). Everything that needs @solana/web3.js (PDA
// derivation, the client, wallet access) is reached through dynamic import()
// at call time, the same way useQueries.ts loads web3.js for useSolBalance, so
// the main chunk stays free of web3.js.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query';
import { useWallet } from './useWallet';
import { selectWalletPositions, type LegacyPositionInput } from '../solana/native/selectors';
import type { NativeClient, PayoutRequest } from '../solana/native/client';
import type {
  Address,
  NativeBacker,
  NativeCloseRecord,
  NativePool,
  NativePosition,
  NativeTicket,
  NativeTxResult,
  PlanKind,
  StatusUpdate,
  WalletPositionEntry,
} from '../solana/native/types';

type NativeReader = import('../solana/native/accounts').NativeReader;

/** Root of every native query key. Disjoint from every legacy key. */
export const NATIVE_QUERY_ROOT = 'native' as const;
const NATIVE_REFETCH_MS = 5_000;

// ---------------------------------------------------------------------------
// Lazy plumbing
// ---------------------------------------------------------------------------

let readersPromise: Promise<NativeReader> | undefined;
/** Read-only access to the program (no wallet needed). One shared reader per page load. */
function getReaders(): Promise<NativeReader> {
  if (!readersPromise) {
    readersPromise = Promise.all([import('../solana/native/accounts'), import('../solana/native/client')]).then(
      ([accounts, client]) => accounts.createReaders(client.createDefaultConnection()),
    );
    // A failed chunk load must be retryable.
    readersPromise.catch(() => {
      readersPromise = undefined;
    });
  }
  return readersPromise;
}

/**
 * Returns a function that builds a NativeClient for the CURRENT SIWS session.
 * The wallet is re-acquired at call time (not render time) so the pubkey check
 * sees whichever account is selected when the user clicks.
 */
export function useNativeClient(): () => Promise<NativeClient> {
  const { walletType, solanaPubkey } = useWallet();
  return useCallback(async () => {
    if (walletType !== 'siws' || !solanaPubkey) {
      throw new Error('Connect a Solana wallet to play the native game.');
    }
    const [{ createNativeClient, createDefaultConnection }, { acquireNativeWallet }] = await Promise.all([
      import('../solana/native/client'),
      import('../solana/native/wallet'),
    ]);
    const wallet = await acquireNativeWallet(solanaPubkey);
    return createNativeClient({ connection: createDefaultConnection(), wallet, sessionPubkey: solanaPubkey });
  }, [walletType, solanaPubkey]);
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

interface ReadOptions {
  /** Set false to keep a panel from polling the RPC (default true for SIWS sessions). */
  enabled?: boolean;
}

function useSiwsGate(options?: ReadOptions): boolean {
  const { walletType } = useWallet();
  return walletType === 'siws' && options?.enabled !== false;
}

export function useNativePool(options?: ReadOptions): UseQueryResult<NativePool | null> {
  const enabled = useSiwsGate(options);
  return useQuery<NativePool | null>({
    queryKey: [NATIVE_QUERY_ROOT, 'pool'],
    queryFn: async () => (await getReaders()).readPool(),
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

export function useNativeTicket(options?: ReadOptions): UseQueryResult<NativeTicket | null> {
  const enabled = useSiwsGate(options);
  return useQuery<NativeTicket | null>({
    queryKey: [NATIVE_QUERY_ROOT, 'ticket'],
    queryFn: async () => (await getReaders()).readTicket(),
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

export function useNativeCloseRecord(closeId: bigint | null | undefined, options?: ReadOptions): UseQueryResult<NativeCloseRecord | null> {
  const enabled = useSiwsGate(options) && closeId !== null && closeId !== undefined;
  return useQuery<NativeCloseRecord | null>({
    queryKey: [NATIVE_QUERY_ROOT, 'close', closeId?.toString() ?? null],
    queryFn: async () => (await getReaders()).readCloseRecord(closeId as bigint),
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

/** Every native Position the session wallet holds (getProgramAccounts on the owner). */
export function useNativePositions(options?: ReadOptions): UseQueryResult<NativePosition[]> {
  const { solanaPubkey } = useWallet();
  const enabled = useSiwsGate(options) && !!solanaPubkey;
  return useQuery<NativePosition[]>({
    queryKey: [NATIVE_QUERY_ROOT, 'positions', solanaPubkey],
    queryFn: async () => (await getReaders()).listPositions(solanaPubkey as string),
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

export function useNativePosition(nonce: bigint | null | undefined, options?: ReadOptions): UseQueryResult<NativePosition | null> {
  const { solanaPubkey } = useWallet();
  const enabled = useSiwsGate(options) && !!solanaPubkey && nonce !== null && nonce !== undefined;
  return useQuery<NativePosition | null>({
    queryKey: [NATIVE_QUERY_ROOT, 'position', solanaPubkey, nonce?.toString() ?? null],
    queryFn: async () => (await getReaders()).readPosition(solanaPubkey as string, nonce as bigint),
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

export interface NativeBackers {
  seriesA: NativeBacker | null;
  seriesB: NativeBacker | null;
}

/** The session wallet's Series A and Series B rows (null when it never funded / was never promoted). */
export function useNativeBackers(options?: ReadOptions): UseQueryResult<NativeBackers> {
  const { solanaPubkey } = useWallet();
  const enabled = useSiwsGate(options) && !!solanaPubkey;
  return useQuery<NativeBackers>({
    queryKey: [NATIVE_QUERY_ROOT, 'backers', solanaPubkey],
    queryFn: async () => {
      const readers = await getReaders();
      const [seriesA, seriesB] = await Promise.all([
        readers.readBacker(solanaPubkey as string, 0),
        readers.readBacker(solanaPubkey as string, 1),
      ]);
      return { seriesA, seriesB };
    },
    enabled,
    refetchInterval: NATIVE_REFETCH_MS,
  });
}

/**
 * Legacy canister positions and native program positions for the session
 * wallet, as ONE list of SEPARATE entries. A legacy game and a native
 * position never merge, even when their numeric ids coincide.
 */
export function useNativeWalletPositions<L extends LegacyPositionInput>(
  legacy: readonly L[] | undefined,
  options?: ReadOptions,
): { entries: Array<WalletPositionEntry<L, NativePosition>>; isLoading: boolean } {
  const { solanaPubkey } = useWallet();
  const native = useNativePositions(options);
  const entries = useMemo(
    () => (solanaPubkey ? selectWalletPositions<L, NativePosition>({ wallet: solanaPubkey, legacy: legacy ?? [], native: native.data ?? [] }) : []),
    [solanaPubkey, legacy, native.data],
  );
  return { entries, isLoading: native.isLoading };
}

// ---------------------------------------------------------------------------
// Transaction status
// ---------------------------------------------------------------------------

/** Follow a submitted transaction's confirmation ladder (polling; no websocket). */
export function useNativeTxStatus(tx: NativeTxResult | null | undefined): StatusUpdate | null {
  const [latest, setLatest] = useState<StatusUpdate | null>(null);
  useEffect(() => {
    if (!tx) {
      setLatest(null);
      return undefined;
    }
    let cancelled = false;
    (async () => {
      try {
        for await (const update of tx.status$) {
          if (cancelled) return;
          setLatest(update);
        }
      } catch {
        // The ladder never throws on RPC trouble (it reports `unknown`); a thrown error here is a bug elsewhere.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [tx]);
  return latest;
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/**
 * Run one NativeClient action as a mutation. The action resolves as soon as the
 * transaction is signed and sent (or its outcome is unknown); the cache is
 * refreshed when the ladder reaches `confirmed`/`finalized`, so panels update
 * without the caller waiting on the cluster.
 */
export function useNativeAction<TVars>(run: (client: NativeClient, vars: TVars) => Promise<NativeTxResult>) {
  const getClient = useNativeClient();
  const queryClient = useQueryClient();
  return useMutation<NativeTxResult, Error, TVars>({
    mutationFn: async (vars) => run(await getClient(), vars),
    onSuccess: (result) => {
      const refresh = () => queryClient.invalidateQueries({ queryKey: [NATIVE_QUERY_ROOT] });
      void (async () => {
        try {
          for await (const update of result.status$) {
            if (update.status === 'confirmed' || update.status === 'finalized' || update.status === 'failed') {
              void refresh();
            }
          }
        } catch {
          // status polling is best-effort; the 5 s refetch interval is the backstop
        }
      })();
    },
  });
}

export const useNativeOpenPosition = () =>
  useNativeAction((client, vars: { plan: PlanKind; gross: bigint; nonce?: bigint }) => client.openPosition(vars));

export const useNativeFundSeriesA = () => useNativeAction((client, vars: { gross: bigint }) => client.fundSeriesA(vars));

export const useNativeWithdrawSimple = () => useNativeAction((client, vars: PayoutRequest) => client.withdrawSimple(vars));

export const useNativeSettleCompounding = () => useNativeAction((client, vars: PayoutRequest) => client.settleCompounding(vars));

export const useNativeCloseSimplePosition = () => useNativeAction((client, vars: { nonce: bigint }) => client.closeSimplePosition(vars));

export const useNativeClaimBacker = () => useNativeAction((client, vars: { destination?: Address }) => client.claimBacker(vars));

/** Keeper step: advance whatever phase the allocation ticket is in (one typed instruction). */
export const useNativeAdvancePhase = () =>
  useNativeAction((client, vars: { rows?: number; skipNewlyCapped?: boolean }) => client.advancePhase(vars));

export const useNativeComposeExpireFinalizeOpen = () =>
  useNativeAction((client, vars: { closeId: bigint; plan: PlanKind; gross: bigint; nonce?: bigint }) =>
    client.composeExpireFinalizeOpen(vars),
  );
