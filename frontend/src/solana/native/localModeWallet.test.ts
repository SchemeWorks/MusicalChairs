// IMPL-46c: the dev-only local-cluster wallet. Fake adapter / storage / connection throughout:
// nothing here touches a network, a canister or a real wallet.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import { Principal } from '@dfinity/principal';
import bs58 from 'bs58';

// Force the mode on so these tests behave the same with or without VITE_NATIVE_LOCAL_CLUSTER in .env.local;
// the real guard functions stay real.
vi.mock('./localModeFlag', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./localModeFlag')>()),
  LOCAL_CLUSTER_MODE: true,
}));

import {
  adoptLocalSession,
  LOCAL_AIRDROP_BELOW_LAMPORTS,
  LOCAL_AIRDROP_LAMPORTS,
  LOCAL_WALLET_STORAGE_KEY,
  connectLocalClusterWallet,
  disconnectLocalClusterWallet,
  ensureLocalFunds,
  loadLocalClusterWallet,
  localClusterIdentity,
  parseWalletSecret,
  resetLocalClusterWalletForTests,
  restoreLocalClusterWallet,
  type AirdropConnection,
  type BurnerAdapter,
  type LocalModeDeps,
} from './localModeWallet';
import { LoopbackGuardError } from './localModeFlag';
import { getNativeClusterState, setNativeClusterIdentity } from './queries';
import { CLUSTER_DOMAINS, clusterCacheKey } from './cluster';
import { bytesToHex, idl } from './codecs';

const LOCAL = 'http://127.0.0.1:8899';
const MAINNET = 'https://api.mainnet-beta.solana.com';

/** Mirrors UnsafeBurnerWalletAdapter: connect() generates its own keypair into a private `_keypair`. */
class FakeBurner implements BurnerAdapter {
  static created = 0;
  static signed = 0;
  private _keypair: Keypair | null = null;
  constructor() {
    FakeBurner.created += 1;
  }
  async connect() {
    this._keypair = Keypair.generate();
  }
  get publicKey() {
    return this._keypair?.publicKey ?? null;
  }
  async signTransaction(tx: Transaction) {
    FakeBurner.signed += 1;
    tx.partialSign(this._keypair!);
    return tx;
  }
}

function memoryStorage() {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
  };
}

function fakeConnection(balances: { initial: number; afterAirdrop?: number }) {
  let balance = balances.initial;
  const conn = {
    getBalance: vi.fn(async () => balance),
    requestAirdrop: vi.fn(async (_key: PublicKey, _lamports: number) => {
      balance = balances.afterAirdrop ?? Number(LOCAL_AIRDROP_LAMPORTS);
      return 'airdrop-sig';
    }),
  };
  return conn satisfies AirdropConnection;
}

function makeDeps(overrides: Partial<LocalModeDeps> & { conn?: ReturnType<typeof fakeConnection> } = {}) {
  const storage = memoryStorage();
  const conn = overrides.conn ?? fakeConnection({ initial: 0 });
  const deps: LocalModeDeps = {
    endpoint: LOCAL,
    storage,
    createAdapter: async () => new FakeBurner(),
    createConnection: () => conn,
    sleep: async () => undefined,
    ...overrides,
  };
  return { deps, storage, conn };
}

beforeEach(() => {
  resetLocalClusterWalletForTests();
  FakeBurner.created = 0;
  FakeBurner.signed = 0;
});
afterEach(() => resetLocalClusterWalletForTests());

describe('parseWalletSecret', () => {
  const keypair = Keypair.generate();
  it('reads a solana-keygen style JSON byte array and a base58 secret', () => {
    expect(Array.from(parseWalletSecret(JSON.stringify(Array.from(keypair.secretKey))))).toEqual(Array.from(keypair.secretKey));
    expect(Array.from(parseWalletSecret(`  ${bs58.encode(keypair.secretKey)}\n`))).toEqual(Array.from(keypair.secretKey));
  });
  it('rejects malformed input with a readable reason', () => {
    expect(() => parseWalletSecret('   ')).toThrow(/Paste a wallet secret/);
    expect(() => parseWalletSecret('[1,2,')).toThrow(/does not parse/);
    expect(() => parseWalletSecret('[1,2,300]')).toThrow(/byte values/);
    expect(() => parseWalletSecret('{"a":1}')).toThrow(/not valid base58/);
    expect(() => parseWalletSecret('0OIl')).toThrow(/not valid base58/); // excluded base58 characters
    expect(() => parseWalletSecret(JSON.stringify(Array.from(keypair.secretKey.slice(0, 32))))).toThrow(/64 bytes \(got 32\)/);
    const tampered = Uint8Array.from(keypair.secretKey);
    tampered[63] ^= 0xff; // the embedded public half no longer matches
    expect(() => parseWalletSecret(JSON.stringify(Array.from(tampered)))).toThrow(/inconsistent/);
  });
});

describe('connect a fresh burner', () => {
  it('seats a keypair, persists the secret for a reload, airdrops 5 SOL and derives a local principal', async () => {
    const { deps, storage, conn } = makeDeps();
    const session = await connectLocalClusterWallet({}, deps);
    expect(session.imported).toBe(false);
    expect(session.airdropped).toBe(true);
    expect(conn.requestAirdrop).toHaveBeenCalledTimes(1);
    expect(conn.requestAirdrop.mock.calls[0][1]).toBe(Number(LOCAL_AIRDROP_LAMPORTS));
    expect(LOCAL_AIRDROP_LAMPORTS).toBe(5_000_000_000n);
    expect(session.pubkey).toMatch(/^[1-9A-HJ-NP-Za-km-z]{32,44}$/);
    expect(Principal.fromText(session.principal).toText()).toBe(session.principal); // well-formed
    expect(session.displayName).toBeTruthy();
    // sessionStorage holds the 64-byte secret whose public half is the session pubkey
    const stored = bs58.decode(storage.map.get(LOCAL_WALLET_STORAGE_KEY)!);
    expect(stored).toHaveLength(64);
    expect(Keypair.fromSecretKey(stored).publicKey.toBase58()).toBe(session.pubkey);
  });

  it('a reload (fresh module state) restores the SAME pubkey with no network call and no new airdrop', async () => {
    const first = makeDeps();
    const session = await connectLocalClusterWallet({}, first.deps);
    resetLocalClusterWalletForTests(); // page reload: memory gone, sessionStorage kept
    const reload = makeDeps({
      storage: first.storage,
      createConnection: () => {
        throw new Error('restore must not touch the network');
      },
    });
    const restored = await restoreLocalClusterWallet(reload.deps);
    expect(restored?.pubkey).toBe(session.pubkey);
    expect(restored?.principal).toBe(session.principal);
    expect(restored?.airdropped).toBe(false);
  });

  it('does not airdrop a wallet that already holds SOL', async () => {
    const funded = fakeConnection({ initial: Number(LOCAL_AIRDROP_BELOW_LAMPORTS) });
    const { deps } = makeDeps({ conn: funded });
    const session = await connectLocalClusterWallet({}, deps);
    expect(session.airdropped).toBe(false);
    expect(funded.requestAirdrop).not.toHaveBeenCalled();
  });

  it('reports a faucet that never lands', async () => {
    const stuck = { getBalance: vi.fn(async () => 0), requestAirdrop: vi.fn(async () => 'sig') };
    const { deps } = makeDeps({ conn: stuck as unknown as ReturnType<typeof fakeConnection> });
    await expect(ensureLocalFunds(Keypair.generate().publicKey.toBase58(), deps)).rejects.toThrow(/did not land within 30 seconds/);
  });
});

describe('import a wallet secret', () => {
  const owner = Keypair.generate();

  it('connects as exactly that key from a JSON byte array, persists it, and signs for it', async () => {
    const { deps, storage } = makeDeps({ conn: fakeConnection({ initial: 10_000_000_000 }) });
    const session = await connectLocalClusterWallet({ secret: JSON.stringify(Array.from(owner.secretKey)) }, deps);
    expect(session.pubkey).toBe(owner.publicKey.toBase58());
    expect(session.imported).toBe(true);
    expect(session.airdropped).toBe(false); // funded fixture owner
    expect(bs58.decode(storage.map.get(LOCAL_WALLET_STORAGE_KEY)!)).toEqual(Uint8Array.from(owner.secretKey));

    const wallet = await loadLocalClusterWallet(session.pubkey, deps);
    expect(wallet?.publicKey).toBe(owner.publicKey.toBase58());
    const tx = new Transaction({ feePayer: owner.publicKey, recentBlockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi' });
    tx.add(SystemProgram.transfer({ fromPubkey: owner.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 }));
    const signed = await wallet!.signTransaction(tx);
    expect(signed.verifySignatures()).toBe(true);
    expect(signed.signature).toBeTruthy();
  });

  it('accepts base58 too, and survives a reload', async () => {
    const first = makeDeps({ conn: fakeConnection({ initial: 10_000_000_000 }) });
    await connectLocalClusterWallet({ secret: bs58.encode(owner.secretKey) }, first.deps);
    resetLocalClusterWalletForTests();
    const restored = await restoreLocalClusterWallet(makeDeps({ storage: first.storage }).deps);
    expect(restored?.pubkey).toBe(owner.publicKey.toBase58());
  });

  it('rejects a bad secret before anything is stored or any wallet created', async () => {
    const { deps, storage } = makeDeps();
    await expect(connectLocalClusterWallet({ secret: '[1,2,3]' }, deps)).rejects.toThrow(/64 bytes/);
    expect(storage.map.size).toBe(0);
    expect(FakeBurner.created).toBe(0);
  });

  it('the signing wallet is only handed out for the session pubkey', async () => {
    const { deps } = makeDeps({ conn: fakeConnection({ initial: 10_000_000_000 }) });
    const session = await connectLocalClusterWallet({}, deps);
    expect(await loadLocalClusterWallet(Keypair.generate().publicKey.toBase58(), deps)).toBeNull();
    expect(await loadLocalClusterWallet(session.pubkey, deps)).not.toBeNull();
  });
});

describe('loopback guard: refuse to sign, send or airdrop off 127.0.0.1 / localhost / [::1]', () => {
  it.each([[MAINNET], ['https://api.devnet.solana.com'], ['http://127.0.0.1.evil.com:8899'], ['http://192.168.1.10:8899']])(
    'connect refuses against %s: nothing created, stored or airdropped',
    async (endpoint) => {
      const { deps, storage, conn } = makeDeps({ endpoint });
      await expect(connectLocalClusterWallet({}, deps)).rejects.toThrow(LoopbackGuardError);
      expect(FakeBurner.created).toBe(0);
      expect(storage.map.size).toBe(0);
      expect(conn.getBalance).not.toHaveBeenCalled();
      expect(conn.requestAirdrop).not.toHaveBeenCalled();
    },
  );

  it('an import is refused too (the secret never reaches storage)', async () => {
    const { deps, storage } = makeDeps({ endpoint: MAINNET });
    await expect(connectLocalClusterWallet({ secret: JSON.stringify(Array.from(Keypair.generate().secretKey)) }, deps)).rejects.toThrow(LoopbackGuardError);
    expect(storage.map.size).toBe(0);
  });

  it('ensureLocalFunds refuses to airdrop on a shared network', async () => {
    const { deps, conn } = makeDeps({ endpoint: MAINNET });
    await expect(ensureLocalFunds(Keypair.generate().publicKey.toBase58(), deps)).rejects.toThrow(/refuses to airdrop/);
    expect(conn.requestAirdrop).not.toHaveBeenCalled();
  });

  it('restore against a non-loopback endpoint discards the stored key and restores nothing', async () => {
    const first = makeDeps();
    await connectLocalClusterWallet({}, first.deps);
    resetLocalClusterWalletForTests();
    const off = makeDeps({ storage: first.storage, endpoint: MAINNET });
    expect(await restoreLocalClusterWallet(off.deps)).toBeNull();
    expect(first.storage.map.has(LOCAL_WALLET_STORAGE_KEY)).toBe(false);
  });

  it('the wallet re-checks the endpoint on EVERY signature, not just at connect', async () => {
    const { deps } = makeDeps({ conn: fakeConnection({ initial: 10_000_000_000 }) });
    const session = await connectLocalClusterWallet({}, deps);
    const wallet = (await loadLocalClusterWallet(session.pubkey, deps))!;
    const payer = new PublicKey(session.pubkey);
    const tx = () => new Transaction({ feePayer: payer, recentBlockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi' }).add(
      SystemProgram.transfer({ fromPubkey: payer, toPubkey: Keypair.generate().publicKey, lamports: 1 }),
    );
    await expect(wallet.signTransaction(tx())).resolves.toBeTruthy();
    expect(FakeBurner.signed).toBe(1);
    deps.endpoint = MAINNET; // the endpoint changes underneath the live wallet
    await expect(wallet.signTransaction(tx())).rejects.toThrow(LoopbackGuardError);
    expect(FakeBurner.signed).toBe(1); // the key never signed the second transaction
  });
});

describe('disconnect', () => {
  it('forgets the key: nothing to restore afterwards', async () => {
    const { deps, storage } = makeDeps();
    await connectLocalClusterWallet({}, deps);
    disconnectLocalClusterWallet(deps);
    expect(storage.map.size).toBe(0);
    expect(await restoreLocalClusterWallet(deps)).toBeNull();
  });
});

describe('adoptLocalSession', () => {
  const calls: Array<[string, unknown]> = [];
  const makeHost = () => {
    calls.length = 0;
    return {
      queryClient: {
        setQueryDefaults: (key: unknown, options: unknown) => void calls.push(['defaults', { key, options }]),
        setQueryData: (key: unknown, data: unknown) => void calls.push(['data', { key, data }]),
      } as unknown as Parameters<typeof adoptLocalSession>[1]['queryClient'],
      setIdentity: (v: null) => void calls.push(['identity', v]),
      setPrincipal: (v: string) => void calls.push(['principal', v]),
      setSolanaPubkey: (v: string) => void calls.push(['pubkey', v]),
      setWalletType: (v: 'siws') => void calls.push(['walletType', v]),
      markLocalSession: () => void calls.push(['marked', true]),
    };
  };
  afterEach(() => setNativeClusterIdentity(null));

  it("becomes a 'siws' session with a pubkey and NO identity, and seeds the profile so the dashboard opens without an ICP write", () => {
    adoptLocalSession({ pubkey: 'PUBKEY', principal: 'aaaaa-aa', displayName: 'Local cluster tester' }, makeHost(), { endpoint: LOCAL });
    const byName = Object.fromEntries(calls);
    expect(byName.identity).toBeNull();
    expect(byName.walletType).toBe('siws');
    expect(byName.pubkey).toBe('PUBKEY');
    expect(byName.principal).toBe('aaaaa-aa');
    expect(byName.marked).toBe(true);
    expect(byName.defaults).toEqual({ key: ['currentUserProfile'], options: { staleTime: Infinity } });
    expect(byName.data).toEqual({ key: ['currentUserProfile', 'aaaaa-aa'], data: { name: 'Local cluster tester' } });
  });

  it('points every native hook at the local validator: localnet Pool.clusterDomain, this build\'s program, the loopback RPC', () => {
    adoptLocalSession({ pubkey: 'PUBKEY', principal: 'aaaaa-aa', displayName: 'x' }, makeHost(), { endpoint: 'http://localhost:8899' });
    const { identity } = getNativeClusterState();
    expect(identity).toEqual({ cluster: { expectedClusterDomain: 'localnet', programId: idl.address }, rpcEndpoint: 'http://localhost:8899' });
    expect(localClusterIdentity('http://localhost:8899')).toEqual(identity);
    expect(clusterCacheKey(identity!)).toContain(bytesToHex(CLUSTER_DOMAINS.localnet));
  });

  it('refuses to adopt a session against a non-loopback RPC (identity untouched, nothing adopted)', () => {
    const before = getNativeClusterState().identity;
    expect(() => adoptLocalSession({ pubkey: 'PUBKEY', principal: 'aaaaa-aa', displayName: 'x' }, makeHost(), { endpoint: MAINNET })).toThrow(LoopbackGuardError);
    expect(calls).toHaveLength(0);
    expect(getNativeClusterState().identity).toBe(before);
  });

  it('disconnect puts the cluster identity back to the one the build\'s endpoint implies', () => {
    adoptLocalSession({ pubkey: 'PUBKEY', principal: 'aaaaa-aa', displayName: 'x' }, makeHost(), { endpoint: 'http://[::1]:8899' });
    expect(getNativeClusterState().identity?.rpcEndpoint).toBe('http://[::1]:8899');
    disconnectLocalClusterWallet({ storage: memoryStorage() });
    expect(getNativeClusterState().identity?.rpcEndpoint).not.toBe('http://[::1]:8899');
  });
});
