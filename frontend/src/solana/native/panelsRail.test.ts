// IMPL-46c: which rail a SOL panel shows. `unavailable` is ALWAYS the legacy rail, never a native UI:
// the default mainnet endpoint verifies as NativeClusterNotConfiguredError until mainnet has a cluster domain.
import { describe, expect, it } from 'vitest';
import { describeUnavailable, railOf, resolveAvailability } from './panelsRail';
import { NativeClusterMismatchError, NativeClusterNotConfiguredError, defaultClusterIdentity, verifyCluster } from './cluster';
import { testPool } from './panelsTestFixtures';

const base = { siws: true, hasIdentity: true, identityError: null, pool: undefined, poolLoading: false, poolError: null };

describe('native availability and the rail it selects', () => {
  it('a non-Solana session has nothing to decide', () => {
    const a = resolveAvailability({ ...base, siws: false });
    expect(a.state).toBe('inactive');
    expect(railOf(a)).toBe('inactive');
  });

  it('a readable Pool on the verified cluster is the native rail', () => {
    const pool = testPool();
    const a = resolveAvailability({ ...base, pool });
    expect(a).toMatchObject({ state: 'native', pool, reason: null });
    expect(railOf(a)).toBe('native');
  });

  it('is loading while the Pool read is in flight (and only then)', () => {
    expect(railOf(resolveAvailability({ ...base, poolLoading: true }))).toBe('loading');
    // once a Pool is there, a refetch in flight does not drop back to loading
    expect(resolveAvailability({ ...base, pool: testPool(), poolLoading: true }).state).toBe('native');
  });

  it('the default mainnet endpoint: verification fails closed, so the legacy rail shows and no native UI', async () => {
    const identity = defaultClusterIdentity('https://api.mainnet-beta.solana.com');
    // This is exactly the error the verified readers raise for that identity.
    const failure = await verifyCluster({ genesis: async () => 'x', poolDomain: async () => null }, identity.cluster).catch((e) => e);
    expect(failure).toBeInstanceOf(NativeClusterNotConfiguredError);
    const a = resolveAvailability({ ...base, poolError: failure });
    expect(a).toMatchObject({ state: 'unavailable', reason: 'cluster-not-configured', pool: null });
    expect(railOf(a)).toBe('legacy');
  });

  it('an endpoint that names no cluster has no identity: legacy rail, never a spinner', () => {
    const a = resolveAvailability({ ...base, hasIdentity: false, identityError: new NativeClusterNotConfiguredError('no cluster') });
    expect(a).toMatchObject({ state: 'unavailable', reason: 'no-cluster' });
    expect(railOf(a)).toBe('legacy');
    // loading never masks a missing identity (the query is disabled, so it would never finish)
    expect(resolveAvailability({ ...base, hasIdentity: false, poolLoading: true }).state).toBe('unavailable');
  });

  it('a cluster mismatch, an RPC error and an uninitialized program are all legacy-rail outcomes with their own reason', () => {
    const mismatch = new NativeClusterMismatchError('genesis-hash', 'a', 'b', 'wrong cluster');
    expect(resolveAvailability({ ...base, poolError: mismatch })).toMatchObject({ reason: 'cluster-mismatch', state: 'unavailable' });
    expect(resolveAvailability({ ...base, poolError: new Error('fetch failed') })).toMatchObject({ reason: 'rpc-error' });
    const uninitialized = resolveAvailability({ ...base, pool: null });
    expect(uninitialized).toMatchObject({ reason: 'program-not-initialized', state: 'unavailable' });
    for (const a of [resolveAvailability({ ...base, poolError: mismatch }), uninitialized]) expect(railOf(a)).toBe('legacy');
  });

  it('says why in one sentence (local-cluster mode shows it instead of a legacy panel)', () => {
    const mismatch = new NativeClusterMismatchError('genesis-hash', 'a', 'b', 'The connected RPC is not the cluster this app expects.');
    expect(describeUnavailable({ reason: 'cluster-mismatch', error: mismatch })).toMatch(/not the cluster this app expects/);
    expect(describeUnavailable({ reason: 'cluster-not-configured', error: null })).toMatch(/resolve to a cluster/);
    expect(describeUnavailable({ reason: 'rpc-error', error: null })).toMatch(/validator running/);
    expect(describeUnavailable({ reason: 'program-not-initialized', error: null })).toMatch(/genesis step/);
  });
});
