// R1 (cache side): every native query key is scoped to (cluster, programId,
// endpoint), the readers behind them verify the cluster first, and a cluster
// change drops the old cluster's entries and contexts.
import { describe, it, expect, vi } from 'vitest';
import { QueryClient } from '@tanstack/react-query';
import { Keypair, PublicKey } from '@solana/web3.js';
import { SOLANA_RPC_ENDPOINT } from '../rpc';
import { poolAddress, positionAddress } from './accounts';
import { createClusterContext, dropClusterContextsExcept, getClusterContext, type ClusterContext } from './client';
import {
  CLUSTER_DOMAINS,
  NativeClusterMismatchError,
  PUBLIC_GENESIS_HASHES,
  clusterCacheKey,
  defaultClusterIdentity,
  type NativeClusterIdentity,
} from './cluster';
import { idl } from './codecs';
import {
  NATIVE_QUERY_ROOT,
  evictOtherClusters,
  getNativeClusterState,
  nativeQueries,
  nativeQueryKey,
  setNativeClusterIdentity,
  subscribeNativeCluster,
  type ReadersProvider,
} from './queries';
import { buildAccount, fakeConnection, type FakeAccount } from './testFixtures';

const PROGRAM = new PublicKey(idl.address);

function chainWith(domain: Uint8Array, roundId: bigint, extra: Array<[PublicKey, Uint8Array]> = []) {
  const accounts = new Map<string, FakeAccount>();
  accounts.set(poolAddress().toBase58(), { owner: PROGRAM, data: buildAccount('Pool', { clusterDomain: domain, roundId, activeConfigVersion: 1 }) });
  for (const [key, data] of extra) accounts.set(key.toBase58(), { owner: PROGRAM, data });
  return accounts;
}

const LOCAL: NativeClusterIdentity = { cluster: { expectedClusterDomain: 'localnet', programId: idl.address }, rpcEndpoint: 'http://127.0.0.1:8899' };
const DEVNET: NativeClusterIdentity = { cluster: { expectedClusterDomain: 'devnet', programId: idl.address }, rpcEndpoint: 'https://api.devnet.solana.com' };

/** A provider over prepared contexts, looked up by cache key (so a wrong key cannot reach the wrong connection). */
function providerFor(contexts: Record<string, ClusterContext>): ReadersProvider {
  return async (identity) => {
    const context = contexts[clusterCacheKey(identity)];
    if (!context) throw new Error('no context for identity');
    return context.readers;
  };
}

function twoClusters() {
  const local = fakeConnection(chainWith(CLUSTER_DOMAINS.localnet, 11n), { getGenesisHash: vi.fn(async () => 'LocalValidatorGenesis111111111111111111111111') });
  const devnet = fakeConnection(chainWith(CLUSTER_DOMAINS.devnet, 22n), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
  const provider = providerFor({
    [clusterCacheKey(LOCAL)]: createClusterContext(LOCAL, local),
    [clusterCacheKey(DEVNET)]: createClusterContext(DEVNET, devnet),
  });
  return { local, devnet, queries: nativeQueries(provider) };
}

describe('native query cache is scoped per cluster', () => {
  it('twoClustersNeverShareCacheEntries', async () => {
    const { local, devnet, queries } = twoClusters();
    const queryClient = new QueryClient();

    const a = await queryClient.fetchQuery(queries.pool(LOCAL));
    const b = await queryClient.fetchQuery(queries.pool(DEVNET));
    expect([a?.roundId, b?.roundId]).toEqual([11n, 22n]);

    // two entries, distinct keys, each holding its own cluster's data
    const entries = queryClient.getQueryCache().findAll({ queryKey: [NATIVE_QUERY_ROOT] });
    expect(entries).toHaveLength(2);
    expect(new Set(entries.map((e) => JSON.stringify(e.queryKey))).size).toBe(2);
    expect(queryClient.getQueryData<{ roundId: bigint }>(nativeQueryKey(LOCAL, 'pool'))?.roundId).toBe(11n);
    expect(queryClient.getQueryData<{ roundId: bigint }>(nativeQueryKey(DEVNET, 'pool'))?.roundId).toBe(22n);

    // a refetch of one cluster never touches the other's connection
    const devnetReads = devnet.getAccountInfo.mock.calls.length;
    await queryClient.fetchQuery({ ...queries.pool(LOCAL), staleTime: 0 });
    expect(devnet.getAccountInfo.mock.calls.length).toBe(devnetReads);
    expect(local.getAccountInfo.mock.calls.length).toBeGreaterThan(1);
  });

  it('keys also separate program ids and endpoints on the same domain', () => {
    const base = nativeQueryKey(LOCAL, 'pool');
    expect(nativeQueryKey({ ...LOCAL, rpcEndpoint: 'http://127.0.0.1:8900' }, 'pool')).not.toEqual(base);
    expect(nativeQueryKey({ ...LOCAL, cluster: { ...LOCAL.cluster, programId: '11111111111111111111111111111111' } }, 'pool')).not.toEqual(base);
    expect(base[0]).toBe(NATIVE_QUERY_ROOT);
  });

  it('per-owner and per-id queries are keyed by owner/nonce/close as well as cluster', () => {
    const owner = Keypair.generate().publicKey.toBase58();
    const { queries } = twoClusters();
    expect(queries.positions(LOCAL, owner).queryKey).not.toEqual(queries.positions(DEVNET, owner).queryKey);
    expect(queries.position(LOCAL, owner, 1n).queryKey).not.toEqual(queries.position(LOCAL, owner, 2n).queryKey);
    expect(queries.close(LOCAL, 1n).queryKey).not.toEqual(queries.close(LOCAL, 2n).queryKey);
    expect(queries.backers(LOCAL, owner).queryKey).toContain(owner);
  });

  it('a query against the wrong cluster rejects and caches no data; positions never reach getProgramAccounts', async () => {
    const wrong = fakeConnection(chainWith(CLUSTER_DOMAINS.localnet, 99n), { getGenesisHash: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet) });
    const queries = nativeQueries(providerFor({ [clusterCacheKey(DEVNET)]: createClusterContext(DEVNET, wrong) }));
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    await expect(queryClient.fetchQuery(queries.pool(DEVNET))).rejects.toBeInstanceOf(NativeClusterMismatchError);
    await expect(queryClient.fetchQuery(queries.positions(DEVNET, Keypair.generate().publicKey.toBase58()))).rejects.toBeInstanceOf(NativeClusterMismatchError);
    expect(queryClient.getQueryData(nativeQueryKey(DEVNET, 'pool'))).toBeUndefined();
    expect(wrong.getProgramAccounts).not.toHaveBeenCalled();
  });

  it('reads the per-owner data through the verified readers (position by nonce)', async () => {
    const owner = Keypair.generate().publicKey;
    const connection = fakeConnection(
      chainWith(CLUSTER_DOMAINS.localnet, 3n, [[positionAddress(owner, 5n), buildAccount('Position', { owner: owner.toBase58(), nonce: 5n, principal: 1_000n })]]),
    );
    const queries = nativeQueries(providerFor({ [clusterCacheKey(LOCAL)]: createClusterContext(LOCAL, connection) }));
    const queryClient = new QueryClient();
    const position = await queryClient.fetchQuery(queries.position(LOCAL, owner.toBase58(), 5n));
    expect(position?.principal).toBe(1_000n);
  });
});

describe('cluster change', () => {
  it('evictOtherClusters drops only the other clusters’ native entries', () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(nativeQueryKey(LOCAL, 'pool'), 'local');
    queryClient.setQueryData(nativeQueryKey(LOCAL, 'positions', 'x'), 'local-positions');
    queryClient.setQueryData(nativeQueryKey(DEVNET, 'pool'), 'devnet');
    queryClient.setQueryData([NATIVE_QUERY_ROOT, 'config', 1], 'unscoped-native-key');
    queryClient.setQueryData(['legacy', 'sol-games'], 'legacy');

    evictOtherClusters(queryClient, clusterCacheKey(DEVNET));

    expect(queryClient.getQueryData(nativeQueryKey(LOCAL, 'pool'))).toBeUndefined();
    expect(queryClient.getQueryData(nativeQueryKey(LOCAL, 'positions', 'x'))).toBeUndefined();
    expect(queryClient.getQueryData(nativeQueryKey(DEVNET, 'pool'))).toBe('devnet');
    expect(queryClient.getQueryData([NATIVE_QUERY_ROOT, 'config', 1])).toBe('unscoped-native-key');
    expect(queryClient.getQueryData(['legacy', 'sol-games'])).toBe('legacy');
  });

  it('the identity store switches, notifies subscribers once per change, and null restores the build default', () => {
    // endpoints no .env.local would use, so the default can never coincide with these
    const first: NativeClusterIdentity = { ...DEVNET, rpcEndpoint: 'https://api.devnet.solana.com/odd' };
    const second: NativeClusterIdentity = { ...LOCAL, rpcEndpoint: 'http://127.0.0.1:18899' };
    const seen: Array<string | null> = [];
    const unsubscribe = subscribeNativeCluster(() => seen.push(getNativeClusterState().identity ? clusterCacheKey(getNativeClusterState().identity!) : null));
    setNativeClusterIdentity(first);
    expect(getNativeClusterState().identity).toBe(first);
    const snapshot = getNativeClusterState();
    setNativeClusterIdentity(first); // same identity: no churn, same snapshot object
    expect(getNativeClusterState()).toBe(snapshot);
    setNativeClusterIdentity(second);
    expect(getNativeClusterState().identity).toBe(second);
    setNativeClusterIdentity(null);
    const fallback = getNativeClusterState();
    let expected: string | null = null;
    try {
      expected = clusterCacheKey(defaultClusterIdentity(SOLANA_RPC_ENDPOINT));
    } catch {
      expected = null;
    }
    expect(fallback.identity ? clusterCacheKey(fallback.identity) : null).toBe(expected);
    unsubscribe();
    expect(seen).toHaveLength(3);
    expect(seen.slice(0, 2)).toEqual([clusterCacheKey(first), clusterCacheKey(second)]);
  });

  it('the context registry hands out one context per identity and drops the others on a switch', () => {
    const a = getClusterContext(LOCAL);
    expect(getClusterContext(LOCAL)).toBe(a);
    const b = getClusterContext(DEVNET);
    expect(b).not.toBe(a);
    dropClusterContextsExcept(clusterCacheKey(DEVNET));
    expect(getClusterContext(DEVNET)).toBe(b);
    expect(getClusterContext(LOCAL)).not.toBe(a); // recreated: the old context (and its verification memo) is gone
    dropClusterContextsExcept('none');
  });

  it('switching the active identity drops every other cluster’s context (connection and verification memo)', () => {
    const odd: NativeClusterIdentity = { ...LOCAL, rpcEndpoint: 'http://127.0.0.1:18898' };
    const before = getClusterContext(odd);
    const other: NativeClusterIdentity = { ...DEVNET, rpcEndpoint: 'https://api.devnet.solana.com/switch' };
    setNativeClusterIdentity(other);
    expect(getClusterContext(odd)).not.toBe(before);
    const kept = getClusterContext(other);
    setNativeClusterIdentity(null);
    expect(getClusterContext(other)).not.toBe(kept); // the switch away from `other` dropped it too
    dropClusterContextsExcept('none');
  });
});
