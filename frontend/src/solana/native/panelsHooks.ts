// React hooks the native Solana panels share (IMPL-46c). They sit on top of
// hooks/useNativeSolana.ts (reads + status) and add what the panels needed that
// it does not provide: a wallet resolver that understands the dev-only local
// cluster wallet, a generic "run a client task" mutation, config reads, and the
// native-vs-legacy availability decision.
//
// Cluster scoping (R1): every read goes through the verified readers of
// getClusterContext(identity) (genesis hash and Pool.clusterDomain checked before the
// first read), and every query key is nativeQueryKey(identity, ...) so a cluster change
// evicts it. The identity is useNativeClusterIdentity() (the build's RPC endpoint, or
// the local validator once the dev-only local wallet connects).
//
// Bundle: no static @solana/web3.js import (bundleHygiene-style). Everything that
// needs web3.js, the client or a wallet is reached through dynamic import(), and
// the local-cluster wallet only through loadLocalModeWallet (localModeLoader.ts).

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useMutation, useQueries, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { useWallet } from '../../hooks/useWallet';
import { NATIVE_QUERY_ROOT, nativeQueryKey, useNativeClusterIdentity, useNativePool, useNativePositions } from '../../hooks/useNativeSolana';
import type { NativeClient } from './client';
import type { NativeReader } from './accounts';
import { NativeClusterError, NativeClusterNotConfiguredError, type NativeClusterIdentity } from './cluster';
import type { NativeConfigVersion, NativePool, NativePosition, NativeTxResult } from './types';
import { quotePosition, type PositionQuote } from './panelsQuote';
import { resolveAvailability, type NativeAvailability } from './panelsRail';
import { LOCAL_CLUSTER_MODE } from './localModeFlag';
import { loadLocalModeWallet } from './localModeLoader';
import type { NativeWallet } from './wallet';

// ---------------------------------------------------------------------------
// Verified readers and cluster-scoped keys
// ---------------------------------------------------------------------------

/** The verified readers of one cluster (genesis hash and Pool.clusterDomain are checked before the first read). */
async function readersFor(identity: NativeClusterIdentity): Promise<NativeReader> {
  return (await import('./client')).getClusterContext(identity).readers;
}

/** No retry once the cluster itself is wrong: asking again cannot fix a mismatch. */
const retryUnlessClusterError = (failureCount: number, error: unknown): boolean => !(error instanceof NativeClusterError) && failureCount < 3;

const noCluster = (name: string): readonly unknown[] => [NATIVE_QUERY_ROOT, 'no-cluster', name];

const CONFIG_STALE_MS = 60_000;

/** One pinned ConfigVersion (plan terms). Versions are immutable once staged, so this caches for a minute. */
export function useNativeConfig(version: number | null | undefined) {
  const { walletType } = useWallet();
  const { identity } = useNativeClusterIdentity();
  return useQuery<NativeConfigVersion | null>({
    queryKey: identity ? nativeQueryKey(identity, 'config', version ?? null) : noCluster('config'),
    queryFn: async () => (await readersFor(identity as NativeClusterIdentity)).readConfig(version as number),
    enabled: walletType === 'siws' && identity !== null && version !== null && version !== undefined,
    staleTime: CONFIG_STALE_MS,
    retry: retryUnlessClusterError,
  });
}

/** ConfigVersions by number (positions pin the version they opened under). */
export function useNativeConfigMap(versions: readonly number[]): Map<number, NativeConfigVersion> {
  const { walletType } = useWallet();
  const { identity } = useNativeClusterIdentity();
  const unique = useMemo(() => [...new Set(versions)].sort((a, b) => a - b), [versions.join(',')]); // eslint-disable-line react-hooks/exhaustive-deps
  const results = useQueries({
    queries: unique.map((version) => ({
      queryKey: identity ? nativeQueryKey(identity, 'config', version) : noCluster('config'),
      queryFn: async () => (await readersFor(identity as NativeClusterIdentity)).readConfig(version),
      enabled: walletType === 'siws' && identity !== null,
      staleTime: CONFIG_STALE_MS,
      retry: retryUnlessClusterError,
    })),
  });
  const data = results.map((r) => r.data).join('|');
  return useMemo(() => {
    const map = new Map<number, NativeConfigVersion>();
    results.forEach((result, i) => {
      if (result.data) map.set(unique[i], result.data);
    });
    return map;
    // `data` changes identity per fetch; the joined marker keeps this stable between polls.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unique, data, results.length]);
}

/** The ConfigVersion new positions open under (pool.activeConfigVersion). */
export function useNativeActiveConfig(pool: NativePool | null | undefined) {
  return useNativeConfig(pool ? pool.activeConfigVersion : null);
}

// ---------------------------------------------------------------------------
// Native vs legacy
// ---------------------------------------------------------------------------

export type { NativeAvailability, NativeAvailabilityState } from './panelsRail';

/**
 * Decides each SOL panel's rail (panelsRail.ts has the rule). Native is the default (DR-22)
 * whenever the program's Pool exists on the connected, VERIFIED cluster. Every other outcome,
 * including the default mainnet endpoint where cluster verification fails closed
 * (NativeClusterNotConfiguredError), is `unavailable`: the panel shows the legacy rail only and
 * no native UI.
 */
export function useNativeAvailability(): NativeAvailability {
  const { walletType } = useWallet();
  const cluster = useNativeClusterIdentity();
  const pool = useNativePool();
  return resolveAvailability({
    siws: walletType === 'siws',
    hasIdentity: cluster.identity !== null,
    identityError: cluster.error,
    pool: pool.data,
    poolLoading: pool.isLoading,
    poolError: pool.error,
  });
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

const nowSeconds = () => BigInt(Math.floor(Date.now() / 1000));

/** Unix seconds, re-rendering every `tickMs` (live accrual, countdowns). */
export function useNowSeconds(tickMs = 1000): bigint {
  const [now, setNow] = useState(nowSeconds);
  useEffect(() => {
    const id = setInterval(() => setNow(nowSeconds()), tickMs);
    return () => clearInterval(id);
  }, [tickMs]);
  return now;
}

// ---------------------------------------------------------------------------
// Client + actions
// ---------------------------------------------------------------------------

async function resolveNativeWallet(sessionPubkey: string): Promise<NativeWallet> {
  if (LOCAL_CLUSTER_MODE && loadLocalModeWallet) {
    const local = await loadLocalModeWallet();
    const wallet = await local.loadLocalClusterWallet(sessionPubkey);
    if (wallet) return wallet;
  }
  const { acquireNativeWallet } = await import('./wallet');
  return acquireNativeWallet(sessionPubkey);
}

/**
 * Builds a NativeClient for the current session on the ACTIVE cluster at call time. Same as
 * useNativeClient() in useNativeSolana.ts (pinned to the cluster identity, shared verified
 * connection), except that in local-cluster mode the session's own (burner/imported) wallet signs.
 */
export function usePanelNativeClient(): () => Promise<NativeClient> {
  const { walletType, solanaPubkey } = useWallet();
  const { identity, error } = useNativeClusterIdentity();
  return useCallback(async () => {
    if (walletType !== 'siws' || !solanaPubkey) {
      throw new Error('Connect a Solana wallet to play the native game.');
    }
    if (!identity) throw error ?? new NativeClusterNotConfiguredError('No Solana cluster is configured for the native game.');
    const [{ createNativeClient, getClusterContext }, wallet] = await Promise.all([import('./client'), resolveNativeWallet(solanaPubkey)]);
    return createNativeClient({
      connection: getClusterContext(identity).connection,
      wallet,
      sessionPubkey: solanaPubkey,
      cluster: identity.cluster,
    });
  }, [walletType, solanaPubkey, identity, error]);
}

/** Refresh every native query once `result` reaches confirmed / finalized / failed (best effort; polling is the backstop). */
export function refreshWhenLanded(queryClient: QueryClient, result: NativeTxResult | null | undefined): void {
  if (!result) return;
  void (async () => {
    try {
      for await (const update of result.status$) {
        if (update.status === 'confirmed' || update.status === 'finalized' || update.status === 'failed') {
          void queryClient.invalidateQueries({ queryKey: [NATIVE_QUERY_ROOT] });
        }
      }
    } catch {
      // status polling never throws on RPC trouble; the 5 s refetch interval backs this up
    }
  })();
}

export function useNativeRefresh(): () => void {
  const queryClient = useQueryClient();
  return useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: [NATIVE_QUERY_ROOT] });
  }, [queryClient]);
}

/**
 * Run an arbitrary multi-step task with a NativeClient and the cluster's VERIFIED readers (e.g. provision
 * a missing page, then open a position). `watch` lists the transactions to follow for a cache refresh
 * once the task finishes.
 */
export function usePanelNativeTask<TVars, TResult>(
  run: (client: NativeClient, vars: TVars, readers: NativeReader) => Promise<TResult>,
  watch: (result: TResult) => Array<NativeTxResult | null | undefined>,
) {
  const getClient = usePanelNativeClient();
  const { identity, error } = useNativeClusterIdentity();
  const queryClient = useQueryClient();
  return useMutation<TResult, Error, TVars>({
    mutationFn: async (vars) => {
      const client = await getClient();
      if (!identity) throw error ?? new NativeClusterNotConfiguredError('No Solana cluster is configured for the native game.');
      return run(client, vars, await readersFor(identity));
    },
    onSuccess: (result) => {
      for (const tx of watch(result)) refreshWhenLanded(queryClient, tx);
    },
  });
}

/** Run one NativeClient method that returns a transaction handle. */
export function usePanelNativeAction<TVars>(run: (client: NativeClient, vars: TVars) => Promise<NativeTxResult>) {
  return usePanelNativeTask<TVars, NativeTxResult>(run, (result) => [result]);
}

// ---------------------------------------------------------------------------
// Request account (is the randomized-selection request fulfilled yet?)
// ---------------------------------------------------------------------------

/**
 * Data length of the provider's request account, polled while `enabled`; null when it does not exist yet.
 * The cluster is verified first (a guarded Pool read is memoized per connection), then the raw account read.
 */
export function useRequestAccountLength(requestAccount: string | null | undefined, enabled: boolean) {
  const { identity } = useNativeClusterIdentity();
  return useQuery<number | null>({
    queryKey: identity ? nativeQueryKey(identity, 'request-account', requestAccount ?? null) : noCluster('request-account'),
    queryFn: async () => {
      const [{ PublicKey }, { getClusterContext }] = await Promise.all([import('@solana/web3.js'), import('./client')]);
      const context = getClusterContext(identity as NativeClusterIdentity);
      await context.readers.readPool(); // verifies genesis hash and Pool.clusterDomain once per connection
      const info = await context.connection.getAccountInfo(new PublicKey(requestAccount as string), 'confirmed');
      return info ? info.data.length : null;
    },
    enabled: enabled && identity !== null && !!requestAccount,
    refetchInterval: 2_000,
    retry: retryUnlessClusterError,
  });
}

// ---------------------------------------------------------------------------
// Portfolio: the session wallet's native positions with live quotes
// ---------------------------------------------------------------------------

export interface NativePositionRow {
  position: NativePosition;
  config: NativeConfigVersion | null;
  /** Null until the position's ConfigVersion has loaded. */
  quote: PositionQuote | null;
  /** Open and in the pool's current round: the only positions a payout can act on. */
  active: boolean;
}

export interface NativePortfolio {
  pool: NativePool | null;
  rows: NativePositionRow[];
  active: NativePositionRow[];
  closed: NativePositionRow[];
  now: bigint;
  /** Lamports: sum of principal over active positions. */
  totalDeposits: bigint;
  /** Lamports: earnings claimable right now (Simple accrual; Compound once matured), gross of Carried Interest / Jackpot Fee. */
  totalEarned: bigint;
  isLoading: boolean;
}

/**
 * Native positions with live quotes, ticking every second. Legacy canister
 * positions are never mixed in (selectors.ts keeps the two rails apart); callers
 * show this next to, not summed with, the legacy portfolio.
 */
export function useNativePortfolio(): NativePortfolio {
  const { pool } = useNativeAvailability();
  const positionsQuery = useNativePositions();
  const positions = positionsQuery.data;
  const configs = useNativeConfigMap((positions ?? []).map((p) => p.configVersion));
  const now = useNowSeconds();
  return useMemo(() => {
    const rows: NativePositionRow[] = (positions ?? [])
      .map((position) => {
        const config = configs.get(position.configVersion) ?? null;
        return {
          position,
          config,
          quote: pool && config ? quotePosition({ position, config, pot: pool.pot, now }) : null,
          active: !!pool && !position.claimed && position.roundId === pool.roundId,
        };
      })
      .sort((a, b) => (a.position.openedAt < b.position.openedAt ? -1 : a.position.openedAt > b.position.openedAt ? 1 : 0));
    const active = rows.filter((r) => r.active);
    let totalDeposits = 0n;
    let totalEarned = 0n;
    for (const row of active) {
      totalDeposits += row.position.principal;
      if (row.quote && (row.quote.plan === 'Simple' || row.quote.matured)) totalEarned += row.quote.owed;
    }
    return {
      pool,
      rows,
      active,
      closed: rows.filter((r) => !r.active),
      now,
      totalDeposits,
      totalEarned,
      isLoading: positionsQuery.isLoading,
    };
  }, [pool, positions, configs, now, positionsQuery.isLoading]);
}
