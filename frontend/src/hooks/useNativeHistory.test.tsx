// IMPL-49c: the verified native activity hook. The mapper is the gate that keeps intake rows out of every
// user-facing feed, whatever the bridge returns; the hook is disabled (no actor, no call) without a canister id.
import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Entry, Status } from '../declarations/native_sol_bridge/native_sol_bridge.did';

import { Actor } from '@dfinity/agent';

import {
  BridgeClusterError,
  NATIVE_HISTORY_PAGE_SIZE,
  cachedBridgeDomain,
  clusterDomainLabel,
  expectedAppCluster,
  type ExpectedCluster,
  fetchGuardedPage,
  fetchHistoryPage,
  judgeBridgeCluster,
  nextHistoryCursor,
  resetBridgeConfigCacheForTests,
  resolveBridgeCanisterId,
  toNativeHistoryItems,
  useNativeHistory,
  verifyBridgeCluster,
} from './useNativeHistory';
import { CLUSTER_DOMAINS, defaultClusterIdentity, type NativeClusterIdentity } from '../solana/native/cluster';
import { bytesToHex } from '../solana/native/codecs';
import { idl } from '../solana/native/codecs';

const SIG = (n: number) => `${n}`.padStart(88, 'S'); // a distinct 88-char stand-in per fixture

function entry(seq: number, status: Status, over: Partial<Entry['descriptor']> = {}, kind: 'PositionOpened' | 'SeriesAFunded' | 'PromotionApplied' = 'PositionOpened'): Entry {
  return {
    seq: BigInt(seq),
    event_key: `key-${seq}`,
    status,
    verified: [],
    next_attempt_at: 0n,
    binding: { Unbound: null },
    submitted_at: 1n,
    submitted_by: { toText: () => 'aaaaa-aa' } as unknown as Entry['submitted_by'],
    descriptor: {
      beneficiary_pubkey: 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp',
      basis_lamports: 1_000_000_000n,
      kind: { [kind]: null } as Entry['descriptor']['kind'],
      plan: kind === 'PositionOpened' ? [{ Simple: null }] : [],
      source_round_id: 3n,
      source_close_id: [],
      event_id: {
        signature: SIG(seq),
        cluster_domain: new Uint8Array(32),
        inner_instruction_index: 0n,
        event_schema_version: 1n,
        program_id: '7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7',
        instruction_index: 2n,
      },
      ...over,
    },
  };
}

const forwarded = (alreadyObserved = false, at = 1_700_000_000_000_000_000n): Status => ({ Forwarded: { at, already_observed: alreadyObserved, obligation_id: 'obligation-1' } });
const quarantined: Status = { Quarantined: { at: 1_700_000_001_000_000_000n, reason: 'caller is not eligible' } };
const pending: Status = { Pending: null };
const unverified: Status = { Unverified: { attempts: 1n, last_at: 1n, reason: { NotFound: null } } };
const verified: Status = { Verified: { retries: 0n, last_reason: [], last_at: 1n } };
const invalid: Status = { Invalid: { at: 1n, reason: { BasisMismatch: null } } };

describe('toNativeHistoryItems: only verified-delivery records are user-facing', () => {
  it('drops Pending, Unverified, Verified and Invalid, whatever order they arrive in', () => {
    const items = toNativeHistoryItems([
      entry(9, pending),
      entry(8, unverified),
      entry(7, verified),
      entry(6, invalid),
      entry(5, forwarded()),
      entry(4, quarantined),
    ]);
    expect(items.map((i) => i.seq)).toEqual([5n, 4n]);
    const text = JSON.stringify(items, (_k, v) => (typeof v === 'bigint' ? v.toString() : v));
    for (const intake of [9, 8, 7, 6]) expect(text, `seq ${intake}`).not.toContain(SIG(intake));
  });

  it('an all-intake page maps to nothing (an older bridge cannot leak intake rows)', () => {
    expect(toNativeHistoryItems([entry(3, pending), entry(2, unverified), entry(1, invalid)])).toEqual([]);
    expect(toNativeHistoryItems([])).toEqual([]);
  });

  it('Forwarded maps to a delivered item with the obligation id, Observed and AlreadyObserved alike', () => {
    const [observed] = toNativeHistoryItems([entry(10, forwarded(false))]);
    expect(observed).toMatchObject({
      seq: 10n,
      kind: 'PositionOpened',
      plan: 'Simple',
      beneficiaryPubkey: 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp',
      basisLamports: 1_000_000_000n,
      sourceRound: 3n,
      sourceCloseId: null,
      signature: SIG(10),
      instructionIndex: 2n,
      deliveredAt: 1_700_000_000_000_000_000n,
      outcome: 'delivered',
      obligationId: 'obligation-1',
      alreadyObserved: false,
    });
    expect(observed.reason).toBeUndefined();
    const [already] = toNativeHistoryItems([entry(11, forwarded(true))]);
    expect(already).toMatchObject({ outcome: 'delivered', alreadyObserved: true });
  });

  it('Quarantined maps to a rejected item with the reason (a durable permanent Shenanigans rejection)', () => {
    const [q] = toNativeHistoryItems([entry(12, quarantined)]);
    expect(q).toMatchObject({ outcome: 'rejected', reason: 'caller is not eligible', deliveredAt: 1_700_000_001_000_000_000n });
    expect(q.obligationId).toBeUndefined();
  });

  it('maps every kind, the optional close id and the plan', () => {
    const items = toNativeHistoryItems([
      entry(3, forwarded(), { source_close_id: [7n], basis_lamports: 250_000_000n }, 'PromotionApplied'),
      entry(2, forwarded(), { basis_lamports: 50_000_000n }, 'SeriesAFunded'),
      entry(1, forwarded(), { plan: [{ Compound15: null }] }, 'PositionOpened'),
    ]);
    expect(items.map((i) => i.kind)).toEqual(['PromotionApplied', 'SeriesAFunded', 'PositionOpened']);
    expect(items[0]).toMatchObject({ sourceCloseId: 7n, basisLamports: 250_000_000n, plan: null });
    expect(items[1].plan).toBeNull();
    expect(items[2].plan).toBe('Compound15');
  });

  it('is newest-first by sequence regardless of input order, and drops an event kind it does not know', () => {
    const items = toNativeHistoryItems([entry(1, forwarded()), entry(3, forwarded()), entry(2, forwarded())]);
    expect(items.map((i) => i.seq)).toEqual([3n, 2n, 1n]);
    const unknown = entry(4, forwarded());
    (unknown.descriptor as { kind: unknown }).kind = { RoundFinalized: null };
    expect(toNativeHistoryItems([unknown])).toEqual([]);
  });
});

describe('paging by `before`', () => {
  const page = (seqs: number[], status: Status = forwarded()) => seqs.map((n) => entry(n, status));

  it('asks for the newest page with no bound, then below the oldest RAW seq of the previous page', async () => {
    const getRecentEvents = vi.fn(async (_before: [] | [bigint], _limit: bigint) => page([40, 39, 38]));
    const first = await fetchHistoryPage({ getRecentEvents }, null, 3);
    expect(getRecentEvents).toHaveBeenLastCalledWith([], 3n);
    expect(first.oldestSeq).toBe(38n);
    await fetchHistoryPage({ getRecentEvents }, first.oldestSeq, 3);
    expect(getRecentEvents).toHaveBeenLastCalledWith([38n], 3n);
  });

  it('the cursor counts rows the mapper dropped, so a page of only intake rows still advances', async () => {
    const getRecentEvents = vi.fn(async () => [entry(30, pending), entry(29, unverified), entry(28, invalid)]);
    const p = await fetchHistoryPage({ getRecentEvents }, null, 3);
    expect(p.items).toEqual([]);
    expect(p.rawCount).toBe(3);
    expect(nextHistoryCursor(p, 3)).toBe(28n);
  });

  it('a short or empty page is the end', async () => {
    const short = await fetchHistoryPage({ getRecentEvents: async () => page([5, 4]) }, null, 3);
    expect(nextHistoryCursor(short, 3)).toBeUndefined();
    const empty = await fetchHistoryPage({ getRecentEvents: async () => [] }, null, 3);
    expect(empty.oldestSeq).toBeNull();
    expect(nextHistoryCursor(empty, 3)).toBeUndefined();
  });

  it('defaults to a page of 20', () => {
    expect(NATIVE_HISTORY_PAGE_SIZE).toBe(20);
  });
});

describe('bridge canister configuration', () => {
  it('absent, empty and blank mean "not configured"; a non-principal is rejected, a principal is accepted', () => {
    expect(resolveBridgeCanisterId(undefined)).toEqual({ disabled: 'not-configured' });
    expect(resolveBridgeCanisterId('')).toEqual({ disabled: 'not-configured' });
    expect(resolveBridgeCanisterId('   ')).toEqual({ disabled: 'not-configured' });
    expect(resolveBridgeCanisterId('not a principal')).toEqual({ disabled: 'invalid-canister-id' });
    expect(resolveBridgeCanisterId(' ryjl3-tyaaa-aaaaa-aaaba-cai ')).toEqual({ id: 'ryjl3-tyaaa-aaaaa-aaaba-cai' });
  });
});

describe('the hook', () => {
  function probe(configuredId: string | undefined) {
    let seen: ReturnType<typeof useNativeHistory> | undefined;
    const Probe = () => {
      seen = useNativeHistory(configuredId);
      return null;
    };
    renderToStaticMarkup(
      <QueryClientProvider client={new QueryClient()}>
        <Probe />
      </QueryClientProvider>,
    );
    return seen!;
  }

  it('without a canister id it returns { enabled: false } and builds no agent or actor', () => {
    const createActor = vi.spyOn(Actor, 'createActor');
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const absent = probe(undefined);
    expect(absent).toEqual({ enabled: false, reason: 'not-configured' });
    expect(probe('')).toEqual({ enabled: false, reason: 'not-configured' });
    expect(probe('nonsense')).toEqual({ enabled: false, reason: 'invalid-canister-id' });
    expect(createActor).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
    createActor.mockRestore();
    fetchSpy.mockRestore();
  });

  it('with a canister id it is enabled and starts empty and loading', () => {
    const on = probe('ryjl3-tyaaa-aaaaa-aaaba-cai');
    expect(on.enabled).toBe(true);
    if (!on.enabled) return;
    expect(on.items).toEqual([]);
    expect(on.hasMore).toBe(false);
    expect(on.error).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Cluster guard: the bridge's cluster_domain must be the app's, or nothing is listed
// ---------------------------------------------------------------------------

const identity = (name: 'localnet' | 'devnet' | 'mainnet-beta'): NativeClusterIdentity => ({
  cluster: { expectedClusterDomain: name, programId: idl.address },
  rpcEndpoint: 'http://127.0.0.1:8899',
});
const LOCAL = CLUSTER_DOMAINS.localnet;
const DEVNET = CLUSTER_DOMAINS.devnet;

describe('the app\'s expected cluster', () => {
  it('is the adapter\'s expectedClusterDomain, labeled by name', () => {
    const local = expectedAppCluster(identity('localnet'));
    expect(local).toMatchObject({ ok: true, label: 'localnet' });
    expect(local.ok && Array.from(local.domain)).toEqual(Array.from(LOCAL));
    expect(expectedAppCluster(identity('devnet'))).toMatchObject({ ok: true, label: 'devnet' });
    expect(expectedAppCluster(defaultClusterIdentity('https://api.devnet.solana.com'))).toMatchObject({ ok: true, label: 'devnet' });
  });

  it('is unresolved without an identity and for mainnet-beta (no cluster domain exists yet)', () => {
    expect(expectedAppCluster(null)).toEqual({ ok: false });
    expect(expectedAppCluster(identity('mainnet-beta'))).toEqual({ ok: false });
  });

  it('labels raw 32-byte domains by name, never by key', () => {
    expect(clusterDomainLabel(LOCAL)).toBe('localnet');
    expect(clusterDomainLabel(Array.from(DEVNET))).toBe('devnet');
    expect(clusterDomainLabel(new Uint8Array(32).fill(7))).toBe('another deployment');
    const custom = expectedAppCluster({ cluster: { expectedClusterDomain: new Uint8Array(32).fill(7), programId: idl.address }, rpcEndpoint: 'http://127.0.0.1:8899' });
    expect(custom).toMatchObject({ ok: true, label: 'another deployment' });
  });
});

describe('judgeBridgeCluster', () => {
  const app = expectedAppCluster(identity('localnet'));

  it('match: the bridge follows the app\'s cluster', () => {
    expect(judgeBridgeCluster({ expected: app, bridge: { domain: Uint8Array.from(LOCAL) } })).toEqual({ state: 'ok' });
    expect(judgeBridgeCluster({ expected: app, bridge: { domain: Array.from(LOCAL) } })).toEqual({ state: 'ok' }); // candid may hand back number[]
  });

  it('mismatch: disabled, naming BOTH clusters by label and neither by key', () => {
    const v = judgeBridgeCluster({ expected: app, bridge: { domain: DEVNET } });
    expect(v).toEqual({ state: 'disabled', reason: 'cluster-mismatch', appCluster: 'localnet', bridgeCluster: 'devnet' });
    const unknown = judgeBridgeCluster({ expected: app, bridge: { domain: new Uint8Array(32).fill(9) } });
    expect(unknown).toMatchObject({ reason: 'cluster-mismatch', bridgeCluster: 'another deployment' });
    for (const verdict of [v, unknown]) {
      const text = JSON.stringify(verdict);
      expect(text).not.toContain(bytesToHex(LOCAL));
      expect(text).not.toContain(bytesToHex(DEVNET));
      expect(text).not.toMatch(/musical-chairs/);
    }
  });

  it('unconfigured app cluster: disabled with a reason, before the bridge is even consulted', () => {
    const unresolved = expectedAppCluster(identity('mainnet-beta'));
    expect(judgeBridgeCluster({ expected: unresolved, bridge: { domain: Uint8Array.from(LOCAL) } })).toEqual({ state: 'disabled', reason: 'cluster-unresolved' });
    expect(judgeBridgeCluster({ expected: unresolved, bridge: { loading: true } })).toEqual({ state: 'disabled', reason: 'cluster-unresolved' });
  });

  it('config read failure and an uninitialized bridge are disabled, not "ok"', () => {
    expect(judgeBridgeCluster({ expected: app, bridge: { error: true } })).toEqual({ state: 'disabled', reason: 'config-unavailable', appCluster: 'localnet' });
    expect(judgeBridgeCluster({ expected: app, bridge: { domain: null } })).toEqual({ state: 'disabled', reason: 'bridge-uninitialized', appCluster: 'localnet' });
  });

  it('while the config is loading the verdict is "checking" (not ok, not disabled)', () => {
    expect(judgeBridgeCluster({ expected: app, bridge: { loading: true } })).toEqual({ state: 'checking' });
  });
});

describe('verifyBridgeCluster and the config cache', () => {
  const app = expectedAppCluster(identity('localnet'));

  it('reads the config once per (host, canister id) and keeps a real domain', async () => {
    resetBridgeConfigCacheForTests();
    const load = vi.fn(async () => Uint8Array.from(LOCAL));
    const key = 'https://icp0.io|ryjl3-tyaaa-aaaaa-aaaba-cai';
    await Promise.all([cachedBridgeDomain(key, load), cachedBridgeDomain(key, load)]);
    await cachedBridgeDomain(key, load);
    expect(load).toHaveBeenCalledTimes(1);
    await cachedBridgeDomain('http://localhost:4943|ryjl3-tyaaa-aaaaa-aaaba-cai', load); // another host: its own entry
    await cachedBridgeDomain('https://icp0.io|rrkah-fqaaa-aaaaa-aaaaq-cai', load); // another canister: its own entry
    expect(load).toHaveBeenCalledTimes(3);
  });

  it('a failed read and an uninitialized bridge (null) are NOT cached: the next check asks again', async () => {
    resetBridgeConfigCacheForTests();
    const load = vi.fn().mockRejectedValueOnce(new Error('replica down')).mockResolvedValueOnce(null).mockResolvedValue(Uint8Array.from(LOCAL));
    await expect(cachedBridgeDomain('k', load)).rejects.toThrow('replica down');
    expect(await cachedBridgeDomain('k', load)).toBeNull();
    expect(await cachedBridgeDomain('k', load)).toEqual(Uint8Array.from(LOCAL));
    expect(await cachedBridgeDomain('k', load)).toEqual(Uint8Array.from(LOCAL));
    expect(load).toHaveBeenCalledTimes(3);
  });

  it('turns a throwing config read into config-unavailable', async () => {
    const verdict = await verifyBridgeCluster(app, async () => {
      throw new Error('canister rejected the call');
    });
    expect(verdict).toEqual({ state: 'disabled', reason: 'config-unavailable', appCluster: 'localnet' });
  });

  it('does not read the bridge at all when the app\'s own cluster is unresolved', async () => {
    const load = vi.fn(async () => Uint8Array.from(LOCAL));
    expect(await verifyBridgeCluster(expectedAppCluster(identity('mainnet-beta')), load)).toEqual({ state: 'disabled', reason: 'cluster-unresolved' });
    expect(load).not.toHaveBeenCalled();
  });
});

describe('fetchGuardedPage lists nothing unless the clusters match', () => {
  const forwardedStatus = (): Status => ({ Forwarded: { at: 1n, already_observed: false, obligation_id: 'o' } });
  const quarantinedStatus = (): Status => ({ Quarantined: { at: 2n, reason: 'no' } });
  const page = (): Entry[] => [entry(5, forwardedStatus()), entry(4, quarantinedStatus())];
  const app = expectedAppCluster(identity('localnet'));
  const actor = () => ({ getRecentEvents: vi.fn(async (_b: [] | [bigint], _l: bigint) => page()) });

  it('match: the verified page is fetched and mapped', async () => {
    const a = actor();
    const out = await fetchGuardedPage(a, app, async () => Uint8Array.from(LOCAL), null, 20);
    expect(a.getRecentEvents).toHaveBeenCalledTimes(1);
    expect(out.items.map((i) => i.seq)).toEqual([5n, 4n]);
  });

  const refusals: Array<[string, ExpectedCluster, () => Promise<Uint8Array | null>, string]> = [
    ['mismatch', app, async () => Uint8Array.from(DEVNET), 'cluster-mismatch'],
    ['unconfigured app cluster', expectedAppCluster(identity('mainnet-beta')), async () => Uint8Array.from(LOCAL), 'cluster-unresolved'],
    ['config read failure', app, async () => { throw new Error('boom'); }, 'config-unavailable'],
    ['bridge without config', app, async () => null, 'bridge-uninitialized'],
  ];
  it.each(refusals)('%s: refused with a reason and getRecentEvents is never called', async (_name, expected, load, reason) => {
    const a = actor();
    const error = await fetchGuardedPage(a, expected, load, null, 20).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BridgeClusterError);
    expect((error as BridgeClusterError).verdict.reason).toBe(reason);
    expect(a.getRecentEvents).not.toHaveBeenCalled();
  });
});
