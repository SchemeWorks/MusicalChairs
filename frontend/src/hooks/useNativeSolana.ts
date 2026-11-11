// React Query hooks over the native Solana adapter (frontend/src/solana/native).
//
// These sit BESIDE the legacy SOL hooks in useQueries.ts (useGetUserSolGames,
// useGetSolBackerPositions ... = the ponzi_math_sol canister path), which keep
// working untouched. Native query keys all start with 'native', so nothing here
// can alias or invalidate a legacy cache entry, and useNativeWalletPositions
// lists legacy and native positions as separate entries (never merged/summed).
//
// Cluster scoping (spec L467: cache keys are (cluster, programId)): every key is
// [ 'native', <cluster cache key>, ...parts ] where the cluster cache key covers
// the expected Pool.clusterDomain, the program id and the RPC endpoint. The
// readers and the client behind a hook verify the cluster (genesis hash and
// Pool.clusterDomain) before the first read or action, and a change of the
// active cluster identity evicts the old cluster's cache entries. The active
// identity defaults to the one implied by VITE_SOLANA_RPC_URL
// (queries.ts getNativeClusterState) and can be switched with
// setNativeClusterIdentity.
//
// Bundle: this file imports only web3-free modules statically (types, selectors,
// economics, cluster, queries). Everything that needs @solana/web3.js (PDA
// derivation, the client, wallet access) is reached through dynamic import() at
// call time, the same way useQueries.ts loads web3.js for useSolBalance, so the
// main chunk stays free of web3.js.

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { useMutation, useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query';
import { useWallet } from './useWallet';
import { selectWalletPositions, type LegacyPositionInput } from '../solana/native/selectors';
import type { NativeClient, PayoutRequest } from '../solana/native/client';
import { NativeClusterError, NativeClusterNotConfiguredError, clusterCacheKey, type NativeClusterIdentity } from '../solana/native/cluster';
import {
  NATIVE_QUERY_ROOT,
  evictOtherClusters,
  getNativeClusterState,
  nativeQueries,
  nativeQueryKey,
  setNativeClusterIdentity,
  subscribeNativeCluster,
  type NativeClusterState,
} from '../solana/native/queries';
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

export { NATIVE_QUERY_ROOT, nativeQueryKey, setNativeClusterIdentity };
export type { NativeClusterIdentity };

const NATIVE_REFETCH_MS = 5_000;
const queries = nativeQueries();

// ---------------------------------------------------------------------------
// Active cluster identity
// ---------------------------------------------------------------------------

let warned = false;

/**
 * The cluster identity every native hook uses, plus its cache key. A change
 * evicts the previous cluster's query entries (and, through queries.ts, its
 * connection and verification memo). `identity` is null when the build's RPC
 * endpoint names no cluster; native hooks are then disabled and fall back to
 * the legacy rail.
 */
export function useNativeClusterIdentity(): NativeClusterState & { key: string | null } {
  const state = useSyncExternalStore(subscribeNativeCluster, getNativeClusterState, getNativeClusterState);
  const queryClient = useQueryClient();
  const key = state.identity ? clusterCacheKey(state.identity) : null;
  useEffect(() => {
    evictOtherClusters(queryClient, key ?? '');
  }, [queryClient, key]);
  if (!state.identity && state.error && !warned) {
    warned = true;
    console.warn(`[native solana] disabled: ${state.error.message}`);
  }
  return { ...state, key };
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/**
 * Returns a function that builds a NativeClient for the CURRENT SIWS session on
 * the active cluster. The wallet is re-acquired at call time (not render time)
 * so the pubkey check sees whichever account is selected when the user clicks;
 * the connection (and its cluster verification) is shared per cluster identity.
 */
export function useNativeClient(identity?: NativeClusterIdentity): () => Promise<NativeClient> {
  const { walletType, solanaPubkey } = useWallet();
  const cluster = useNativeClusterIdentity();
  const active = identity ?? cluster.identity;
  const clusterError = cluster.error;
  return useCallback(async () => {
    if (walletType !== 'siws' || !solanaPubkey) {
      throw new Error('Connect a Solana wallet to play the native game.');
    }
    if (!active) throw clusterError ?? new NativeClusterNotConfiguredError('No Solana cluster is configured for the native game.');
    const [{ createNativeClient, getClusterContext }, { acquireNativeWallet }] = await Promise.all([
      import('../solana/native/client'),
      import('../solana/native/wallet'),
    ]);
    const wallet = await acquireNativeWallet(solanaPubkey);
    return createNativeClient({
      connection: getClusterContext(active).connection,
      wallet,
      sessionPubkey: solanaPubkey,
      cluster: active.cluster,
    });
  }, [walletType, solanaPubkey, active, clusterError]);
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

interface ReadOptions {
  /** Set false to keep a panel from polling the RPC (default true for SIWS sessions). */
  enabled?: boolean;
  /** Read from this cluster instead of the active one (default: the active identity). */
  identity?: NativeClusterIdentity;
}

/** Polling that stops for good once the cluster itself is wrong: re-asking cannot fix a mismatch. */
const pollUnlessClusterError = (query: { state: { error: unknown } }): number | false =>
  query.state.error instanceof NativeClusterError ? false : NATIVE_REFETCH_MS;
const retryUnlessClusterError = (failureCount: number, error: unknown): boolean => !(error instanceof NativeClusterError) && failureCount < 3;

/** The identity a read hook uses and whether it may run (SIWS session, a resolvable cluster, not switched off). */
function useReadScope(options?: ReadOptions): { identity: NativeClusterIdentity | null; enabled: boolean } {
  const { walletType } = useWallet();
  const cluster = useNativeClusterIdentity();
  const identity = options?.identity ?? cluster.identity;
  return { identity, enabled: walletType === 'siws' && options?.enabled !== false && identity !== null };
}

/** Query options for `identity`, or a never-run placeholder while no cluster resolves (the hook is disabled then). */
function scoped<T>(
  identity: NativeClusterIdentity | null,
  name: string,
  make: (identity: NativeClusterIdentity) => { queryKey: readonly unknown[]; queryFn: () => Promise<T> },
) {
  if (identity) return make(identity);
  return {
    queryKey: [NATIVE_QUERY_ROOT, 'no-cluster', name] as readonly unknown[],
    queryFn: async (): Promise<T> => {
      throw new NativeClusterNotConfiguredError('No Solana cluster is configured for the native game.');
    },
  };
}

const pollingOptions = { refetchInterval: pollUnlessClusterError, retry: retryUnlessClusterError } as const;

export function useNativePool(options?: ReadOptions): UseQueryResult<NativePool | null> {
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativePool | null>({ ...scoped(identity, 'pool', queries.pool), enabled, ...pollingOptions });
}

export function useNativeTicket(options?: ReadOptions): UseQueryResult<NativeTicket | null> {
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativeTicket | null>({ ...scoped(identity, 'ticket', queries.ticket), enabled, ...pollingOptions });
}

export function useNativeCloseRecord(closeId: bigint | null | undefined, options?: ReadOptions): UseQueryResult<NativeCloseRecord | null> {
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativeCloseRecord | null>({
    ...scoped(identity, 'close', (id) => queries.close(id, closeId as bigint)),
    enabled: enabled && closeId !== null && closeId !== undefined,
    ...pollingOptions,
  });
}

/** Every native Position the session wallet holds (getProgramAccounts on the owner). */
export function useNativePositions(options?: ReadOptions): UseQueryResult<NativePosition[]> {
  const { solanaPubkey } = useWallet();
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativePosition[]>({
    ...scoped(identity, 'positions', (id) => queries.positions(id, solanaPubkey as string)),
    enabled: enabled && !!solanaPubkey,
    ...pollingOptions,
  });
}

export function useNativePosition(nonce: bigint | null | undefined, options?: ReadOptions): UseQueryResult<NativePosition | null> {
  const { solanaPubkey } = useWallet();
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativePosition | null>({
    ...scoped(identity, 'position', (id) => queries.position(id, solanaPubkey as string, nonce as bigint)),
    enabled: enabled && !!solanaPubkey && nonce !== null && nonce !== undefined,
    ...pollingOptions,
  });
}

export interface NativeBackers {
  seriesA: NativeBacker | null;
  seriesB: NativeBacker | null;
}

/** The session wallet's Series A and Series B rows (null when it never funded / was never promoted). */
export function useNativeBackers(options?: ReadOptions): UseQueryResult<NativeBackers> {
  const { solanaPubkey } = useWallet();
  const { identity, enabled } = useReadScope(options);
  return useQuery<NativeBackers>({
    ...scoped(identity, 'backers', (id) => queries.backers(id, solanaPubkey as string)),
    enabled: enabled && !!solanaPubkey,
    ...pollingOptions,
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
export function useNativeAction<TVars>(run: (client: NativeClient, vars: TVars) => Promise<NativeTxResult>, identity?: NativeClusterIdentity) {
  const getClient = useNativeClient(identity);
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
