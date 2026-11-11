// R1: cluster identity. A native read or action is only allowed after the
// connection's genesis hash and the on-chain Pool.clusterDomain agree with the
// identity the caller pinned; cache keys are (cluster, programId, endpoint).
import { describe, it, expect, vi } from 'vitest';
import { bytesToHex, idl } from './codecs';
import { readSamplingFixture } from './testFixtures';
import {
  CLUSTER_DOMAINS,
  PUBLIC_GENESIS_HASHES,
  NativeClusterMismatchError,
  NativeClusterNotConfiguredError,
  clusterCacheKey,
  defaultClusterIdentity,
  ensureClusterVerified,
  inferClusterName,
  resolveExpectedDomain,
  verifyCluster,
  type NativeClusterConfig,
  type NativeClusterIdentity,
} from './cluster';

const text = (s: string) => new TextEncoder().encode(s);
const LOCALNET = CLUSTER_DOMAINS.localnet;
const DEVNET = CLUSTER_DOMAINS.devnet;
const cluster = (expectedClusterDomain: NativeClusterConfig['expectedClusterDomain'], extra: Partial<NativeClusterConfig> = {}): NativeClusterConfig => ({
  expectedClusterDomain,
  programId: idl.address,
  ...extra,
});
const identity = (c: NativeClusterConfig, rpcEndpoint = 'http://127.0.0.1:8899'): NativeClusterIdentity => ({ cluster: c, rpcEndpoint });

/** The two probes a verification needs, as spies. */
function probes(genesis: string, poolDomain: Uint8Array | null) {
  return { genesis: vi.fn(async () => genesis), poolDomain: vi.fn(async () => poolDomain) };
}

describe('cluster domains', () => {
  it('the named domains are the 32-byte genesis constants (devnet matches the seed-vector fixture)', () => {
    expect(LOCALNET).toHaveLength(32);
    expect(new TextDecoder().decode(LOCALNET)).toBe('musical-chairs-localnet-dev!!!!!');
    expect(DEVNET).toHaveLength(32);
    const fixtureDevnet = readSamplingFixture().protocol_seed.vectors.find((v) => v.cluster === 'devnet')!;
    expect(new TextDecoder().decode(DEVNET)).toBe(fixtureDevnet.cluster_domain);
  });

  it('resolves names and raw bytes, and fails closed for a mainnet domain that is not chosen yet', () => {
    expect(bytesToHex(resolveExpectedDomain(cluster('localnet')))).toBe(bytesToHex(LOCALNET));
    expect(bytesToHex(resolveExpectedDomain(cluster('devnet')))).toBe(bytesToHex(DEVNET));
    const raw = text('musical-chairs-custom-domain-xy!');
    expect(raw).toHaveLength(32);
    expect(bytesToHex(resolveExpectedDomain(cluster(raw)))).toBe(bytesToHex(raw));
    expect(() => resolveExpectedDomain(cluster('mainnet-beta'))).toThrow(NativeClusterNotConfiguredError);
    expect(() => resolveExpectedDomain(cluster(new Uint8Array(31)))).toThrow(/32 bytes/);
  });
});

describe('verifyCluster', () => {
  it('accepts a matching devnet genesis hash and Pool.clusterDomain', async () => {
    const p = probes(PUBLIC_GENESIS_HASHES.devnet, DEVNET);
    await expect(verifyCluster(p, cluster('devnet'))).resolves.toBe('verified');
  });

  it('accepts a local validator: any private genesis hash, the localnet domain', async () => {
    await expect(verifyCluster(probes('SomeLocalValidatorGenesis111111111111111111111', LOCALNET), cluster('localnet'))).resolves.toBe('verified');
  });

  it('rejects a Pool.clusterDomain that is not the expected domain, with both values in the error', async () => {
    const error = await verifyCluster(probes(PUBLIC_GENESIS_HASHES.devnet, LOCALNET), cluster('devnet')).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterMismatchError);
    expect((error as NativeClusterMismatchError).reason).toBe('cluster-domain');
    expect((error as NativeClusterMismatchError).expected).toBe(bytesToHex(DEVNET));
    expect((error as NativeClusterMismatchError).actual).toBe(bytesToHex(LOCALNET));
  });

  it('rejects the wrong genesis hash for devnet (a mainnet RPC behind a devnet identity) before reading the Pool', async () => {
    const p = probes(PUBLIC_GENESIS_HASHES['mainnet-beta'], DEVNET);
    const error = await verifyCluster(p, cluster('devnet')).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterMismatchError);
    expect((error as NativeClusterMismatchError).reason).toBe('genesis-hash');
    expect(p.poolDomain).not.toHaveBeenCalled();
  });

  it('refuses the localnet domain on a public cluster even if the Pool says localnet', async () => {
    for (const hash of Object.values(PUBLIC_GENESIS_HASHES)) {
      const error = await verifyCluster(probes(hash, LOCALNET), cluster('localnet')).catch((e: unknown) => e);
      expect(error).toBeInstanceOf(NativeClusterMismatchError);
      expect((error as NativeClusterMismatchError).reason).toBe('genesis-hash');
    }
  });

  it('honors an explicit genesis pin for a raw domain', async () => {
    const raw = text('musical-chairs-custom-domain-xy!');
    const pinned = cluster(raw, { expectedGenesisHash: 'PinnedGenesis1111111111111111111111111111111' });
    await expect(verifyCluster(probes('PinnedGenesis1111111111111111111111111111111', raw), pinned)).resolves.toBe('verified');
    await expect(verifyCluster(probes('OtherGenesis111111111111111111111111111111111', raw), pinned)).rejects.toBeInstanceOf(NativeClusterMismatchError);
  });

  it('a program id other than the one the adapter was built for is a mismatch', async () => {
    const error = await verifyCluster(probes(PUBLIC_GENESIS_HASHES.devnet, DEVNET), cluster('devnet', { programId: '11111111111111111111111111111111' })).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(NativeClusterMismatchError);
    expect((error as NativeClusterMismatchError).reason).toBe('program-id');
  });

  it('an uninitialized program (no Pool) is reported, not verified', async () => {
    await expect(verifyCluster(probes(PUBLIC_GENESIS_HASHES.devnet, null), cluster('devnet'))).resolves.toBe('uninitialized');
  });

  it('mainnet-beta fails closed with a typed error before any network call', async () => {
    const p = probes(PUBLIC_GENESIS_HASHES['mainnet-beta'], DEVNET);
    await expect(verifyCluster(p, cluster('mainnet-beta'))).rejects.toBeInstanceOf(NativeClusterNotConfiguredError);
    expect(p.genesis).not.toHaveBeenCalled();
    expect(p.poolDomain).not.toHaveBeenCalled();
  });

  it('a connection that cannot report its genesis hash is unverifiable', async () => {
    const p = { genesis: vi.fn(async () => { throw new Error('boom'); }), poolDomain: vi.fn(async () => DEVNET) };
    await expect(verifyCluster(p, cluster('devnet'))).rejects.toThrow(/boom/);
  });
});

describe('ensureClusterVerified (per connection and identity)', () => {
  it('verifies once and shares the result across callers, concurrently or later', async () => {
    const connection = {};
    const p = probes(PUBLIC_GENESIS_HASHES.devnet, DEVNET);
    const results = await Promise.all([1, 2, 3].map(() => ensureClusterVerified(connection, cluster('devnet'), p)));
    expect(results).toEqual(['verified', 'verified', 'verified']);
    await ensureClusterVerified(connection, cluster('devnet'), p);
    expect(p.genesis).toHaveBeenCalledTimes(1);
    expect(p.poolDomain).toHaveBeenCalledTimes(1);
  });

  it('never caches a failure or an uninitialized pool: the next call probes again', async () => {
    const connection = {};
    let domain: Uint8Array | null = null;
    const p = { genesis: vi.fn(async () => PUBLIC_GENESIS_HASHES.devnet), poolDomain: vi.fn(async () => domain) };
    expect(await ensureClusterVerified(connection, cluster('devnet'), p)).toBe('uninitialized');
    domain = LOCALNET;
    await expect(ensureClusterVerified(connection, cluster('devnet'), p)).rejects.toBeInstanceOf(NativeClusterMismatchError);
    domain = DEVNET;
    expect(await ensureClusterVerified(connection, cluster('devnet'), p)).toBe('verified');
    expect(p.poolDomain).toHaveBeenCalledTimes(3);
  });

  it('two connections (or two identities) are verified independently', async () => {
    const a = {};
    const b = {};
    const good = probes(PUBLIC_GENESIS_HASHES.devnet, DEVNET);
    const bad = probes(PUBLIC_GENESIS_HASHES.devnet, LOCALNET);
    await ensureClusterVerified(a, cluster('devnet'), good);
    await expect(ensureClusterVerified(b, cluster('devnet'), bad)).rejects.toBeInstanceOf(NativeClusterMismatchError);
    await ensureClusterVerified(a, cluster('localnet'), probes('LocalGenesis1111111111111111111111111111111', LOCALNET));
    expect(good.genesis).toHaveBeenCalledTimes(1);
  });
});

describe('cache keys: (cluster, programId, endpoint)', () => {
  const base = identity(cluster('localnet'), 'http://127.0.0.1:8899');

  it('differs by cluster domain, program id, genesis pin and endpoint', () => {
    const keys = new Set([
      clusterCacheKey(base),
      clusterCacheKey(identity(cluster('devnet'), 'http://127.0.0.1:8899')),
      clusterCacheKey(identity(cluster('localnet', { programId: '11111111111111111111111111111111' }), 'http://127.0.0.1:8899')),
      clusterCacheKey(identity(cluster('localnet', { expectedGenesisHash: 'PinnedGenesis1111111111111111111111111111111' }), 'http://127.0.0.1:8899')),
      clusterCacheKey(identity(cluster('localnet'), 'http://127.0.0.1:8900')),
    ]);
    expect(keys.size).toBe(5);
  });

  it('is the same for a name and its raw bytes, and stable across calls', () => {
    expect(clusterCacheKey(identity(cluster(LOCALNET)))).toBe(clusterCacheKey(base));
    expect(clusterCacheKey(base)).toBe(clusterCacheKey({ ...base, cluster: { ...base.cluster } }));
  });

  it('never carries an RPC provider key from the endpoint path or query', () => {
    const key = clusterCacheKey(identity(cluster('devnet'), 'https://devnet.helius-rpc.com/v1/SECRET-API-KEY?api-key=SECRET-API-KEY'));
    expect(key).not.toContain('SECRET');
    expect(key).toContain('devnet.helius-rpc.com');
    // two keys on the same host stay distinct
    expect(key).not.toBe(clusterCacheKey(identity(cluster('devnet'), 'https://devnet.helius-rpc.com/v1/OTHER')));
  });
});

describe('endpoint inference', () => {
  it('names the cluster from the host only', () => {
    expect(inferClusterName('http://127.0.0.1:8899')).toBe('localnet');
    expect(inferClusterName('http://localhost:8899')).toBe('localnet');
    expect(inferClusterName('http://[::1]:8899')).toBe('localnet');
    expect(inferClusterName('https://api.devnet.solana.com')).toBe('devnet');
    expect(inferClusterName('https://devnet.helius-rpc.com/?api-key=abc')).toBe('devnet');
    expect(inferClusterName('https://api.mainnet-beta.solana.com')).toBe('mainnet-beta');
    expect(inferClusterName('https://solana-mainnet.g.alchemy.com/v2/key')).toBe('mainnet-beta');
    expect(inferClusterName('https://api.testnet.solana.com')).toBeNull();
    expect(inferClusterName('https://rpc.example.com/devnet')).toBeNull(); // path words do not count
    expect(inferClusterName('not a url')).toBeNull();
  });

  it('the default identity pins the program id of the IDL and refuses an unidentifiable endpoint', () => {
    const local = defaultClusterIdentity('http://127.0.0.1:8899');
    expect(local.cluster).toEqual({ expectedClusterDomain: 'localnet', programId: idl.address });
    expect(local.rpcEndpoint).toBe('http://127.0.0.1:8899');
    expect(() => defaultClusterIdentity('https://rpc.example.com')).toThrow(NativeClusterNotConfiguredError);
  });
});
