// Cluster-scoped query plumbing for the native Solana hooks (R1).
//
// Everything here is React-free and web3-free (type-only imports of the
// web3-using modules; they are reached through dynamic import()). That keeps the
// main bundle clean and lets the cache behavior be tested without rendering.
//
//  * Every query key is [NATIVE_QUERY_ROOT, clusterCacheKey(identity), ...parts],
//    so two clusters, two program ids or two endpoints never share an entry.
//  * The readers behind a query verify the cluster (genesis hash and
//    Pool.clusterDomain) before the first read: a mismatch is a query error with
//    a typed cause, never cached data.
//  * The active identity lives in a tiny store (`useNativeClusterIdentity` in
//    hooks/useNativeSolana.ts subscribes to it). A change evicts the previous
//    cluster's query entries and context.

import type { QueryClient } from '@tanstack/react-query';
import { SOLANA_RPC_ENDPOINT } from '../rpc';
import type { NativeReader } from './accounts';
import { clusterCacheKey, defaultClusterIdentity, type NativeClusterIdentity } from './cluster';
import type { NativeBacker, NativeCloseRecord, NativePool, NativePosition, NativeTicket } from './types';

/** Root of every native query key. Disjoint from every legacy key. */
export const NATIVE_QUERY_ROOT = 'native' as const;

/** The marker `clusterCacheKey` values start with; eviction only touches keys carrying it. */
const CLUSTER_KEY_PREFIX = 'cluster:';

export function nativeQueryKey(identity: NativeClusterIdentity, ...parts: Array<string | number | bigint | null>): readonly unknown[] {
  return [NATIVE_QUERY_ROOT, clusterCacheKey(identity), ...parts.map((part) => (typeof part === 'bigint' ? part.toString() : part))];
}

// ---------------------------------------------------------------------------
// Readers and query definitions
// ---------------------------------------------------------------------------

/** Supplies the verified readers of one cluster identity. */
export type ReadersProvider = (identity: NativeClusterIdentity) => Promise<NativeReader>;

/** The production provider: one connection and one verification memo per identity, loaded on demand. */
export const lazyReaders: ReadersProvider = async (identity) => (await import('./client')).getClusterContext(identity).readers;

/**
 * Query options for every native read. `provider` is injectable so the cache
 * behavior is testable with fake connections.
 */
export function nativeQueries(provider: ReadersProvider = lazyReaders) {
  return {
    pool: (identity: NativeClusterIdentity) => ({
      queryKey: nativeQueryKey(identity, 'pool'),
      queryFn: async (): Promise<NativePool | null> => (await provider(identity)).readPool(),
    }),
    ticket: (identity: NativeClusterIdentity) => ({
      queryKey: nativeQueryKey(identity, 'ticket'),
      queryFn: async (): Promise<NativeTicket | null> => (await provider(identity)).readTicket(),
    }),
    close: (identity: NativeClusterIdentity, closeId: bigint) => ({
      queryKey: nativeQueryKey(identity, 'close', closeId),
      queryFn: async (): Promise<NativeCloseRecord | null> => (await provider(identity)).readCloseRecord(closeId),
    }),
    positions: (identity: NativeClusterIdentity, owner: string) => ({
      queryKey: nativeQueryKey(identity, 'positions', owner),
      queryFn: async (): Promise<NativePosition[]> => (await provider(identity)).listPositions(owner),
    }),
    position: (identity: NativeClusterIdentity, owner: string, nonce: bigint) => ({
      queryKey: nativeQueryKey(identity, 'position', owner, nonce),
      queryFn: async (): Promise<NativePosition | null> => (await provider(identity)).readPosition(owner, nonce),
    }),
    backers: (identity: NativeClusterIdentity, owner: string) => ({
      queryKey: nativeQueryKey(identity, 'backers', owner),
      queryFn: async (): Promise<{ seriesA: NativeBacker | null; seriesB: NativeBacker | null }> => {
        const readers = await provider(identity);
        const [seriesA, seriesB] = await Promise.all([readers.readBacker(owner, 0), readers.readBacker(owner, 1)]);
        return { seriesA, seriesB };
      },
    }),
  };
}

/** Remove every native query entry that belongs to a cluster other than `keepKey`. Unscoped keys are left alone. */
export function evictOtherClusters(queryClient: QueryClient, keepKey: string): void {
  queryClient.removeQueries({
    predicate: (query) => {
      const [root, scope] = query.queryKey;
      return root === NATIVE_QUERY_ROOT && typeof scope === 'string' && scope.startsWith(CLUSTER_KEY_PREFIX) && scope !== keepKey;
    },
  });
}

// ---------------------------------------------------------------------------
// Active identity store
// ---------------------------------------------------------------------------

export interface NativeClusterState {
  /** The identity every native hook uses; null when the build's endpoint names no cluster. */
  identity: NativeClusterIdentity | null;
  /** Why `identity` is null. */
  error: Error | null;
}

function deriveDefault(): NativeClusterState {
  try {
    return { identity: defaultClusterIdentity(SOLANA_RPC_ENDPOINT), error: null };
  } catch (error) {
    return { identity: null, error: error as Error };
  }
}

let override: NativeClusterIdentity | null = null;
let state: NativeClusterState | undefined;
const listeners = new Set<() => void>();
const switchHandlers = new Set<(keepKey: string | null) => void>();

/**
 * Run `handler(keepKey)` whenever the active cluster identity changes, before
 * subscribers are notified. The web3-using client module registers its context
 * cleanup here when it loads, so a switch drops the old cluster's connection
 * and verification memo without this module importing web3.js.
 */
export function onNativeClusterSwitch(handler: (keepKey: string | null) => void): () => void {
  switchHandlers.add(handler);
  return () => {
    switchHandlers.delete(handler);
  };
}

/** The current identity (an explicit override, else the one derived from the build's RPC endpoint). */
export function getNativeClusterState(): NativeClusterState {
  if (!state) state = override ? { identity: override, error: null } : deriveDefault();
  return state;
}

/**
 * Switch the cluster every native hook talks to; `null` restores the default
 * derived from VITE_SOLANA_RPC_URL. Subscribers are notified only when the
 * identity actually changes.
 */
export function setNativeClusterIdentity(identity: NativeClusterIdentity | null): void {
  const previous = getNativeClusterState();
  override = identity;
  state = undefined;
  const next = getNativeClusterState();
  if (previous.identity && next.identity && clusterCacheKey(previous.identity) === clusterCacheKey(next.identity)) {
    state = previous; // unchanged: keep the snapshot stable for useSyncExternalStore, no notification
    return;
  }
  const keepKey = next.identity ? clusterCacheKey(next.identity) : null;
  switchHandlers.forEach((handler) => handler(keepKey));
  listeners.forEach((listener) => listener());
}

export function subscribeNativeCluster(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
