// Dev-only wallet for local-cluster mode (IMPL-46c): an in-browser burner (or an
// imported key) that signs for the native program against a solana-test-validator.
//
// LOAD THIS MODULE ONLY THROUGH `loadLocalModeWallet` (localModeLoader.ts), used by
// hooks/useWallet.tsx and panelsHooks.ts. Imports below are web3.js, the wallet
// adapters and the dfinity principal; none of it belongs in a production chunk,
// and the loader (a ternary on the folded flag) is what lets the bundler drop it.
//
// What it does NOT do: call the SIWS provider canister, any ICP canister, or any
// network other than the loopback RPC. Every sign, send and airdrop re-checks the
// RPC host (LOOPBACK_RPC_HOSTS: 127.0.0.1, localhost, [::1]) and refuses otherwise.
//
// Solana vs ICP, for the reader: on ICP the session identity is a delegation the
// canister granted after a SIWS signature. Here there is no canister: the key that
// signs transactions IS the session. The "principal" below is derived locally from
// the pubkey (Principal.selfAuthenticating) purely so the ICP-keyed read hooks have
// a well-formed principal to look up; it is not an identity anyone controls.

import { Keypair, Connection, PublicKey } from '@solana/web3.js';
import type { Transaction } from '@solana/web3.js';
import { Principal } from '@dfinity/principal';
import type { QueryClient } from '@tanstack/react-query';
import bs58 from 'bs58';
import { SOLANA_RPC_ENDPOINT } from '../rpc';
import { idl } from './codecs';
import type { NativeClusterIdentity } from './cluster';
import { LOCAL_CLUSTER_MODE, assertLoopbackRpcEndpoint } from './localModeFlag';
import { setNativeClusterIdentity } from './queries';
import type { NativeWallet } from './wallet';

/** sessionStorage key holding the 64-byte secret (base58) so a reload keeps the same pubkey. */
export const LOCAL_WALLET_STORAGE_KEY = 'mc-native-local-cluster-secret-v1';
export const LOCAL_AIRDROP_LAMPORTS = 5_000_000_000n;
/** Airdrop only when the wallet holds less than this (so reloads and imported funded keys don't keep minting). */
export const LOCAL_AIRDROP_BELOW_LAMPORTS = 1_000_000_000n;
export const LOCAL_SESSION_NAME = 'Local cluster tester';

/** The slice of UnsafeBurnerWalletAdapter the module uses. */
export interface BurnerAdapter {
  connect(): Promise<void>;
  readonly publicKey: PublicKey | null;
  signTransaction(tx: Transaction): Promise<Transaction>;
}

export interface AirdropConnection {
  getBalance(key: PublicKey, commitment?: 'confirmed'): Promise<number>;
  requestAirdrop(key: PublicKey, lamports: number): Promise<string>;
}

export interface LocalModeDeps {
  endpoint?: string;
  storage?: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> | null;
  createAdapter?: () => Promise<BurnerAdapter>;
  createConnection?: (endpoint: string) => AirdropConnection;
  sleep?: (ms: number) => Promise<void>;
}

export interface LocalClusterSession {
  /** Base58 pubkey: the SIWS-shaped `solanaPubkey` for useWallet. */
  pubkey: string;
  /** Locally derived principal text (not an identity anyone controls). */
  principal: string;
  /** True when the key came from an imported secret rather than a fresh burner. */
  imported: boolean;
  /** True when this connect airdropped SOL. */
  airdropped: boolean;
  /** Name for the locally seeded profile. */
  displayName: string;
}

const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function resolveStorage(deps?: LocalModeDeps): LocalModeDeps['storage'] {
  if (deps && 'storage' in deps) return deps.storage ?? null;
  try {
    return typeof sessionStorage === 'undefined' ? null : sessionStorage;
  } catch {
    return null;
  }
}

async function defaultCreateAdapter(): Promise<BurnerAdapter> {
  const { UnsafeBurnerWalletAdapter } = await import('@solana/wallet-adapter-wallets');
  return new UnsafeBurnerWalletAdapter() as unknown as BurnerAdapter;
}

const endpointOf = (deps?: LocalModeDeps): string => deps?.endpoint ?? SOLANA_RPC_ENDPOINT;

/** Throws LoopbackGuardError unless the RPC host is loopback. */
export function guardLoopback(what: string, deps?: LocalModeDeps): void {
  assertLoopbackRpcEndpoint(endpointOf(deps), what);
}

// ---------------------------------------------------------------------------
// Secret parsing
// ---------------------------------------------------------------------------

/**
 * Accepts what `solana-keygen` and the local_cluster tool write (a JSON array of
 * 64 byte values) or the same 64 bytes in base58. Returns the 64-byte secret key.
 * The public half is validated against the private half (Keypair.fromSecretKey).
 */
export function parseWalletSecret(input: string): Uint8Array {
  const text = input.trim();
  if (!text) throw new Error('Paste a wallet secret: a JSON byte array or a base58 string.');
  let bytes: Uint8Array;
  if (text.startsWith('[')) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      throw new Error('That looks like a JSON byte array but does not parse.');
    }
    if (!Array.isArray(parsed) || !parsed.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)) {
      throw new Error('A JSON wallet secret must be an array of byte values (0-255).');
    }
    bytes = Uint8Array.from(parsed as number[]);
  } else {
    try {
      bytes = bs58.decode(text);
    } catch {
      throw new Error('That is not valid base58. Paste a JSON byte array or a base58 secret key.');
    }
  }
  if (bytes.length !== 64) {
    throw new Error(`A wallet secret key is 64 bytes (got ${bytes.length}).`);
  }
  try {
    Keypair.fromSecretKey(bytes); // verifies the embedded public key matches the private key
  } catch {
    throw new Error('The secret key is inconsistent (its public half does not match).');
  }
  return bytes;
}

// ---------------------------------------------------------------------------
// The wallet (module-level singleton: one local session per page)
// ---------------------------------------------------------------------------

interface ActiveWallet {
  adapter: BurnerAdapter;
  keypair: Keypair;
  deps: LocalModeDeps;
}
let active: ActiveWallet | null = null;

/**
 * Seat `secret` (or a fresh keypair) in an UnsafeBurnerWalletAdapter. The adapter
 * generates its own keypair on connect() and offers no way to supply one, so a
 * restored/imported key replaces its `_keypair` field (a TypeScript-private
 * field, plain at runtime). signTransaction then signs with the restored key.
 */
async function seatWallet(deps: LocalModeDeps, secret: Uint8Array | null): Promise<ActiveWallet> {
  const adapter = await (deps.createAdapter ?? defaultCreateAdapter)();
  await adapter.connect();
  const holder = adapter as unknown as { _keypair: Keypair | null };
  if (secret) holder._keypair = Keypair.fromSecretKey(secret);
  if (!holder._keypair) throw new Error('The burner wallet adapter did not produce a keypair.');
  return { adapter, keypair: holder._keypair, deps };
}

function toSession(wallet: ActiveWallet, imported: boolean, airdropped: boolean): LocalClusterSession {
  return {
    pubkey: wallet.keypair.publicKey.toBase58(),
    principal: Principal.selfAuthenticating(wallet.keypair.publicKey.toBytes()).toText(),
    imported,
    airdropped,
    displayName: LOCAL_SESSION_NAME,
  };
}

function toNativeWallet(wallet: ActiveWallet): NativeWallet {
  return {
    get publicKey() {
      return wallet.adapter.publicKey?.toBase58() ?? null;
    },
    signTransaction: async (tx) => {
      // Re-checked on every signature, not just at connect: the endpoint is a build-time constant,
      // but a refusal here is the last line before a development key signs anything.
      guardLoopback('sign a transaction', wallet.deps);
      return wallet.adapter.signTransaction(tx);
    },
  };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

function requireMode(): void {
  if (!LOCAL_CLUSTER_MODE) throw new Error('Local cluster mode is not enabled in this build.');
}

/** Airdrop `LOCAL_AIRDROP_LAMPORTS` when the wallet holds less than `LOCAL_AIRDROP_BELOW_LAMPORTS`. Returns whether it did. */
export async function ensureLocalFunds(pubkey: string, deps?: LocalModeDeps): Promise<boolean> {
  guardLoopback('airdrop', deps);
  const connection = (deps?.createConnection ?? ((endpoint: string) => new Connection(endpoint, 'confirmed') as unknown as AirdropConnection))(endpointOf(deps));
  const key = new PublicKey(pubkey);
  const sleep = deps?.sleep ?? defaultSleep;
  if (BigInt(await connection.getBalance(key, 'confirmed')) >= LOCAL_AIRDROP_BELOW_LAMPORTS) return false;
  await connection.requestAirdrop(key, Number(LOCAL_AIRDROP_LAMPORTS));
  // No websocket: poll the balance (the faucet lands in a slot or two).
  for (let attempt = 0; attempt < 60; attempt += 1) {
    await sleep(500);
    if (BigInt(await connection.getBalance(key, 'confirmed')) >= LOCAL_AIRDROP_BELOW_LAMPORTS) return true;
  }
  throw new Error('The airdrop did not land within 30 seconds. Is solana-test-validator running with its faucet?');
}

/**
 * Connect a local-cluster wallet: a fresh burner, or `secret` (JSON byte array or
 * base58) when importing. Persists the secret in sessionStorage, airdrops SOL if the
 * wallet is low, and returns the session to hand to useWallet. No ICP call.
 */
export async function connectLocalClusterWallet(opts: { secret?: string } = {}, deps: LocalModeDeps = {}): Promise<LocalClusterSession> {
  requireMode();
  guardLoopback('connect a wallet', deps);
  const secret = opts.secret && opts.secret.trim() ? parseWalletSecret(opts.secret) : null;
  const wallet = await seatWallet(deps, secret);
  active = wallet;
  const storage = resolveStorage(deps);
  try {
    storage?.setItem(LOCAL_WALLET_STORAGE_KEY, bs58.encode(wallet.keypair.secretKey));
  } catch {
    // sessionStorage blocked: the wallet works for this page load only.
  }
  const airdropped = await ensureLocalFunds(wallet.keypair.publicKey.toBase58(), deps);
  return toSession(wallet, secret !== null, airdropped);
}

/**
 * Restore the session from sessionStorage after a reload. Local only: no network
 * (funding top-up is a separate best-effort call). Returns null when there is no
 * stored key, or when the RPC is not loopback (the stored key is then discarded).
 */
export async function restoreLocalClusterWallet(deps: LocalModeDeps = {}): Promise<LocalClusterSession | null> {
  if (!LOCAL_CLUSTER_MODE) return null;
  const storage = resolveStorage(deps);
  let stored: string | null = null;
  try {
    stored = storage?.getItem(LOCAL_WALLET_STORAGE_KEY) ?? null;
  } catch {
    stored = null;
  }
  if (!stored) return null;
  try {
    guardLoopback('restore a wallet', deps);
    active = await seatWallet(deps, parseWalletSecret(stored));
    return toSession(active, false, false);
  } catch {
    try {
      storage?.removeItem(LOCAL_WALLET_STORAGE_KEY);
    } catch {
      // ignore
    }
    active = null;
    return null;
  }
}

/** The signing wallet for the current local session, or null when `sessionPubkey` is not this wallet (the caller then falls back to a real wallet). */
export async function loadLocalClusterWallet(sessionPubkey: string, deps: LocalModeDeps = {}): Promise<NativeWallet | null> {
  if (!LOCAL_CLUSTER_MODE) return null;
  if (!active) {
    const restored = await restoreLocalClusterWallet(deps);
    if (!restored) return null;
  }
  if (!active || active.keypair.publicKey.toBase58() !== sessionPubkey) return null;
  return toNativeWallet(active);
}

/** The cluster identity of the local validator at `endpoint` (localnet domain, this build's program). */
export function localClusterIdentity(endpoint: string = SOLANA_RPC_ENDPOINT): NativeClusterIdentity {
  return { cluster: { expectedClusterDomain: 'localnet', programId: idl.address }, rpcEndpoint: endpoint };
}

/** The state hooks/useWallet.tsx owns, handed over so the local-session logic stays in this (lazy) module. */
export interface LocalSessionHost {
  queryClient: Pick<QueryClient, 'setQueryDefaults' | 'setQueryData'>;
  setIdentity(identity: null): void;
  setPrincipal(principal: string): void;
  setSolanaPubkey(pubkey: string): void;
  setWalletType(type: 'siws'): void;
  markLocalSession(): void;
}

/**
 * Adopt a local-cluster wallet as the app session: walletType 'siws' and its pubkey, NO identity (so
 * every authenticated ICP actor stays null and nothing is ever signed or sent to a canister). App.tsx
 * gates the dashboard on a cached profile and would otherwise route to ProfileSetup, which is an ICP
 * write, so the profile is seeded locally and kept fresh.
 */
export function adoptLocalSession(
  session: Pick<LocalClusterSession, 'pubkey' | 'principal' | 'displayName'>,
  host: LocalSessionHost,
  deps?: LocalModeDeps,
): void {
  guardLoopback('adopt a local session', deps);
  // Point every native hook at the local validator: Pool.clusterDomain must be the localnet domain
  // (musical-chairs-localnet-dev!!!!!) and the genesis hash must not be a public cluster's.
  setNativeClusterIdentity(localClusterIdentity(endpointOf(deps)));
  host.queryClient.setQueryDefaults(['currentUserProfile'], { staleTime: Infinity });
  host.queryClient.setQueryData(['currentUserProfile', session.principal], { name: session.displayName });
  host.markLocalSession();
  host.setIdentity(null);
  host.setPrincipal(session.principal);
  host.setSolanaPubkey(session.pubkey);
  host.setWalletType('siws');
}

/** Forget the local session (called from disconnect). */
export function disconnectLocalClusterWallet(deps: LocalModeDeps = {}): void {
  active = null;
  // Back to the identity the build's RPC endpoint implies.
  setNativeClusterIdentity(null);
  try {
    resolveStorage(deps)?.removeItem(LOCAL_WALLET_STORAGE_KEY);
  } catch {
    // ignore
  }
}

/** Test hook: drop the in-memory wallet without touching storage. */
export function resetLocalClusterWalletForTests(): void {
  active = null;
}
