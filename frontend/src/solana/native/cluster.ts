// Cluster identity for the native adapter (plan M11, spec L467: cache keys are
// (cluster, programId)).
//
// A native read or action is only allowed once, per connection and identity,
// the adapter has checked BOTH:
//   1. the connection's genesis hash (`getGenesisHash`) against the cluster the
//      identity names (devnet and mainnet-beta have public, fixed hashes; a
//      local validator's hash is private, but it must never be a public one), and
//   2. the on-chain `Pool.clusterDomain` (written once at `initialize`, never
//      writable again) against the 32-byte domain the identity expects.
// The first guards against pointing a devnet build at a mainnet RPC (or the
// reverse); the second against an RPC that serves a different deployment.
// Both fail with a typed error before anything is read for display or signed.
//
// ICP comparison: this is the analogue of pinning a canister id AND checking
// you talk to the right replica subnet root key (`fetchRootKey` is only safe
// locally). On Solana the cluster is just a URL, so the genesis hash is the
// root key and `Pool.clusterDomain` is the application-level instance id.
//
// Web3-free (no @solana/web3.js import): safe for the main bundle.

import { bytesToHex, idl } from './codecs';

export type NativeClusterName = 'localnet' | 'devnet' | 'mainnet-beta';

const text = (s: string) => new TextEncoder().encode(s);

/**
 * The 32-byte `Pool.clusterDomain` constants. localnet and devnet are the values
 * the local-cluster genesis and the devnet fixture (solana/fixtures/seed-vectors.json)
 * write. Mainnet has none until its genesis chooses one, so a 'mainnet-beta'
 * identity fails closed (NativeClusterNotConfiguredError) rather than guessing.
 */
export const CLUSTER_DOMAINS: Readonly<Record<'localnet' | 'devnet', Uint8Array>> = Object.freeze({
  localnet: text('musical-chairs-localnet-dev!!!!!'),
  devnet: text('musical-chairs-devnet-domain-v1!'),
});

/** Set when the mainnet genesis picks its domain (the production constant). */
export const MAINNET_CLUSTER_DOMAIN: Uint8Array | null = null;

/** Public cluster genesis hashes (`solana genesis-hash --url <cluster>`). */
export const PUBLIC_GENESIS_HASHES = Object.freeze({
  'mainnet-beta': '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',
  devnet: 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG',
  testnet: '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY',
});

export interface NativeClusterConfig {
  /** A known cluster name, or the 32 raw bytes of `Pool.clusterDomain`. */
  expectedClusterDomain: Uint8Array | NativeClusterName;
  /**
   * The program this adapter talks to. The adapter's PDAs and instructions are
   * built for the IDL's program id, so anything else is a mismatch.
   */
  programId: string;
  /** Optional genesis pin; default: the public hash implied by the domain (none for localnet). */
  expectedGenesisHash?: string;
}

/** What a cache entry, a reader and a client are bound to. */
export interface NativeClusterIdentity {
  cluster: NativeClusterConfig;
  rpcEndpoint: string;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class NativeClusterError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'NativeClusterError';
  }
}

export type ClusterMismatchReason = 'genesis-hash' | 'cluster-domain' | 'program-id';

/** The connection or the program is not the cluster the identity pins. Nothing was read for display or signed. */
export class NativeClusterMismatchError extends NativeClusterError {
  constructor(
    readonly reason: ClusterMismatchReason,
    readonly expected: string,
    readonly actual: string,
    message: string,
  ) {
    super(message);
    this.name = 'NativeClusterMismatchError';
  }
}

/** The identity cannot be resolved (no mainnet domain yet, or an endpoint that names no cluster). */
export class NativeClusterNotConfiguredError extends NativeClusterError {
  constructor(message: string) {
    super(message);
    this.name = 'NativeClusterNotConfiguredError';
  }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

const equalBytes = (a: Uint8Array, b: Uint8Array): boolean => a.length === b.length && a.every((byte, i) => byte === b[i]);

/** The 32 bytes `Pool.clusterDomain` must equal. Throws NativeClusterNotConfiguredError for an unchosen mainnet domain. */
export function resolveExpectedDomain(cluster: NativeClusterConfig): Uint8Array {
  const expected = cluster.expectedClusterDomain;
  if (typeof expected !== 'string') {
    if (expected.length !== 32) throw new RangeError('expectedClusterDomain must be 32 bytes');
    return expected;
  }
  if (expected === 'mainnet-beta') {
    if (!MAINNET_CLUSTER_DOMAIN) {
      throw new NativeClusterNotConfiguredError(
        'The mainnet-beta cluster domain has not been chosen yet, so the native program cannot be verified there.',
      );
    }
    return MAINNET_CLUSTER_DOMAIN;
  }
  return CLUSTER_DOMAINS[expected];
}

/** The genesis hash this domain must run on (`pin`), and the public hashes it must never run on (`forbidden`). */
function genesisExpectations(cluster: NativeClusterConfig, domain: Uint8Array): { pin: string | null; forbidden: string[] } {
  if (cluster.expectedGenesisHash) return { pin: cluster.expectedGenesisHash, forbidden: [] };
  if (equalBytes(domain, CLUSTER_DOMAINS.devnet)) return { pin: PUBLIC_GENESIS_HASHES.devnet, forbidden: [] };
  if (MAINNET_CLUSTER_DOMAIN && equalBytes(domain, MAINNET_CLUSTER_DOMAIN)) {
    return { pin: PUBLIC_GENESIS_HASHES['mainnet-beta'], forbidden: [] };
  }
  // A local validator's hash is private, but a local-dev domain must not be accepted on a public cluster.
  if (equalBytes(domain, CLUSTER_DOMAINS.localnet)) return { pin: null, forbidden: Object.values(PUBLIC_GENESIS_HASHES) };
  return { pin: null, forbidden: [] };
}

// ---------------------------------------------------------------------------
// Endpoint inference and cache keys
// ---------------------------------------------------------------------------

const LOOPBACK = ['127.0.0.1', 'localhost', '[::1]'];

/** Names the cluster from the RPC URL's HOST only (never its path or query, which carry provider keys). */
export function inferClusterName(endpoint: string): NativeClusterName | null {
  let host: string;
  try {
    host = new URL(endpoint).hostname.toLowerCase();
  } catch {
    return null;
  }
  if (LOOPBACK.includes(host)) return 'localnet';
  if (/(^|[.-])testnet([.-]|$)/.test(host)) return null; // no native deployment targets testnet
  if (/(^|[.-])devnet([.-]|$)/.test(host)) return 'devnet';
  if (/(^|[.-])mainnet([.-]|$)/.test(host)) return 'mainnet-beta';
  return null;
}

/** The identity implied by an RPC URL for this build's program. Throws when the host names no cluster. */
export function defaultClusterIdentity(rpcEndpoint: string, programId: string = idl.address): NativeClusterIdentity {
  const name = inferClusterName(rpcEndpoint);
  if (!name) {
    throw new NativeClusterNotConfiguredError(
      'The Solana RPC endpoint does not name a cluster (localnet, devnet or mainnet-beta); configure an explicit cluster identity.',
    );
  }
  return { cluster: { expectedClusterDomain: name, programId }, rpcEndpoint };
}

function fnv1a(value: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

function domainLabel(cluster: NativeClusterConfig): string {
  try {
    return bytesToHex(resolveExpectedDomain(cluster));
  } catch {
    return String(cluster.expectedClusterDomain); // an unresolved name, e.g. 'mainnet-beta'
  }
}

/**
 * The cache key every native query, reader and client is scoped to:
 * (cluster domain, program id, genesis pin, endpoint). The endpoint contributes
 * its host plus a hash of the full URL, so two endpoints never share an entry
 * but a provider key in the path or query never appears in a key.
 */
export function clusterCacheKey(identity: NativeClusterIdentity): string {
  let host = 'invalid-endpoint';
  try {
    host = new URL(identity.rpcEndpoint).host;
  } catch {
    // keep the placeholder; the hash below still separates distinct strings
  }
  const { cluster } = identity;
  return `cluster:${domainLabel(cluster)}:${cluster.programId}:${cluster.expectedGenesisHash ?? '-'}:${host}#${fnv1a(identity.rpcEndpoint)}`;
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

export interface ClusterProbes {
  /** `getGenesisHash()` of the connection. */
  genesis(): Promise<string>;
  /** `Pool.clusterDomain` read from the program's Pool account, or null when the Pool does not exist. */
  poolDomain(): Promise<Uint8Array | null>;
}

export type ClusterVerification = 'verified' | 'uninitialized';

/**
 * One verification. Order: program id and the expected domain (no network),
 * then the genesis hash, then Pool.clusterDomain. 'uninitialized' means the
 * program has no Pool on this cluster (nothing to verify against yet).
 */
export async function verifyCluster(probes: ClusterProbes, cluster: NativeClusterConfig): Promise<ClusterVerification> {
  if (cluster.programId !== idl.address) {
    throw new NativeClusterMismatchError(
      'program-id',
      idl.address,
      cluster.programId,
      `This build of the adapter talks to program ${idl.address}, not ${cluster.programId}.`,
    );
  }
  const domain = resolveExpectedDomain(cluster);
  const { pin, forbidden } = genesisExpectations(cluster, domain);

  const genesis = await probes.genesis();
  if ((pin && genesis !== pin) || forbidden.includes(genesis)) {
    throw new NativeClusterMismatchError(
      'genesis-hash',
      pin ?? `not one of the public clusters`,
      genesis,
      `The connected RPC is not the cluster this app expects (genesis hash ${genesis}). Nothing was read or signed.`,
    );
  }

  const actual = await probes.poolDomain();
  if (actual === null) return 'uninitialized';
  if (!equalBytes(actual, domain)) {
    throw new NativeClusterMismatchError(
      'cluster-domain',
      bytesToHex(domain),
      bytesToHex(actual),
      'The program on the connected cluster belongs to a different deployment (Pool.clusterDomain differs). Nothing was signed.',
    );
  }
  return 'verified';
}

const verifiedFor = new WeakMap<object, Map<string, Promise<ClusterVerification>>>();

/**
 * `verifyCluster`, memoized per (connection object, identity): concurrent
 * callers share one probe, and a 'verified' result is reused for the life of
 * the connection. A failure and an 'uninitialized' pool are NEVER cached, so
 * the next call probes again.
 */
export function ensureClusterVerified(connection: object, cluster: NativeClusterConfig, probes: ClusterProbes): Promise<ClusterVerification> {
  const key = clusterCacheKey({ cluster, rpcEndpoint: '' });
  let perConnection = verifiedFor.get(connection);
  if (!perConnection) {
    perConnection = new Map();
    verifiedFor.set(connection, perConnection);
  }
  const existing = perConnection.get(key);
  if (existing) return existing;
  const attempt = verifyCluster(probes, cluster).then(
    (result) => {
      if (result !== 'verified') perConnection!.delete(key);
      return result;
    },
    (error: unknown) => {
      perConnection!.delete(key);
      throw error;
    },
  );
  perConnection.set(key, attempt);
  return attempt;
}
