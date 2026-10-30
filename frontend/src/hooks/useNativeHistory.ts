// Verified native activity from the ICP bridge (plan M12, spec section 9 "history/feeds", Codex R3).
//
//   native_sol_bridge.getRecentEvents(before, limit) -> newest-first bridge entries
//
// This hook is the ONLY consumer of the bridge canister in the frontend. It is informational:
// balances, payouts and positions are read from Solana (hooks/useNativeSolana.ts), never from here.
//
// WHAT A USER MAY SEE. The bridge stores every submission it has seen, including intake states
// (Pending, Unverified, Verified-but-not-acknowledged, Invalid). Those are audit/operator data. A
// user-facing feed carries ONLY verified-delivery records:
//   Forwarded    Shenanigans returned Observed / AlreadyObserved (durably holds the event)  -> 'delivered'
//   Quarantined  Shenanigans permanently rejected it (a durable record)                      -> 'rejected'
// `toNativeHistoryItems` enforces that itself and drops everything else, so an older bridge that still
// returns intake rows from getRecentEvents cannot leak them (the bridge is also being changed to filter).
//
// Paging: `before` is the oldest RAW sequence number of the previous page, including rows the mapper
// dropped, so a page made only of intake rows still advances the cursor.
//
// Canister id and host:
//   * id   process.env.VITE_NATIVE_BRIDGE_CANISTER_ID (vite.config.ts define). Absent, empty or not a
//          principal -> the hook is disabled: `{ enabled: false }`, no actor is built, no call is made.
//   * host IC_HOST from hooks/useWallet.tsx, the repo's convention: the page on localhost -> the local
//          replica http://localhost:4943 (with fetchRootKey), anything else -> https://icp0.io.
// The actor is anonymous (queries only): no identity, no signing.
//
// CLUSTER AND PROGRAM GUARD (IMPL-49c part 2 and 49g). Before anything is listed, the hook reads the
// bridge's own public `getConfig()` and compares BOTH
//   * `cluster_domain` (32 bytes) with the domain the app's native adapter expects (the same
//     `expectedClusterDomain` it verifies against Pool.clusterDomain, cluster.ts), and
//   * `program_id` (base58) with the adapter identity's `cluster.programId`,
// because a bridge for ANOTHER program on the same cluster would otherwise list unrelated verified
// events as this app's history (program_id is part of event identity, spec section 9; the cache
// identity is cluster + programId, section 11). On any mismatch, an unresolved app cluster (e.g. the
// default mainnet endpoint, which has no cluster domain yet), an uninitialized bridge or a failed
// config read the feed is disabled with a reason and nothing is listed. As a second line of defense
// the mapper drops any record whose own event id carries a different program id (and counts them).
//
// Identity: the config check, the config cache and the history query key are all bound to
//   (host, bridge canister id, expected cluster domain, expected program id)
// so a change of any of them starts a fresh check and a fresh page cache; an earlier verdict or page
// is never reused. Clusters and programs are only ever named by label, never by key.

import { useMemo } from 'react';
import { infiniteQueryOptions, queryOptions, useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Actor, HttpAgent, type ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { idlFactory } from '../declarations/native_sol_bridge/native_sol_bridge.did.js';
import type { Entry, Status, _SERVICE } from '../declarations/native_sol_bridge/native_sol_bridge.did';
import { CLUSTER_DOMAINS, MAINNET_CLUSTER_DOMAIN, resolveExpectedDomain, type NativeClusterIdentity } from '../solana/native/cluster';
import { bytesToHex } from '../solana/native/codecs';
import { IC_HOST } from './useWallet';
import { useNativeClusterIdentity } from './useNativeSolana';

export type BridgeEntry = Entry;

export type NativeHistoryKind = 'PositionOpened' | 'SeriesAFunded' | 'PromotionApplied';
export type NativeHistoryPlan = 'Simple' | 'Compound15' | 'Compound30';

export interface NativeHistoryItem {
  /** The bridge's sequence number (newest = largest). */
  seq: bigint;
  kind: NativeHistoryKind;
  beneficiaryPubkey: string;
  /** PositionOpened / SeriesAFunded: the gross amount; PromotionApplied: the loss the conversion is based on. */
  basisLamports: bigint;
  sourceRound: bigint;
  sourceCloseId: bigint | null;
  plan: NativeHistoryPlan | null;
  /** The Solana transaction signature (base58). */
  signature: string;
  /** Outer instruction index within that transaction. */
  instructionIndex: bigint;
  /** Nanoseconds since the epoch (ICP time) at which the bridge recorded the delivery or rejection. */
  deliveredAt: bigint;
  outcome: 'delivered' | 'rejected';
  /** delivered: the Shenanigans obligation id; Observed and AlreadyObserved both land here. */
  obligationId?: string;
  alreadyObserved?: boolean;
  /** rejected: why Shenanigans permanently refused it. */
  reason?: string;
}

const KINDS: readonly NativeHistoryKind[] = ['PositionOpened', 'SeriesAFunded', 'PromotionApplied'];
const PLANS: readonly NativeHistoryPlan[] = ['Simple', 'Compound15', 'Compound30'];

const variantName = (variant: object): string | undefined => Object.keys(variant)[0];

export interface NativeHistoryMapping {
  items: NativeHistoryItem[];
  /** Verified-delivery records dropped because their event id names a different program than the app's. */
  droppedProgramMismatch: number;
}

/**
 * Bridge entries -> user-facing items, newest first. DEFENSIVE:
 *  * only Forwarded and Quarantined survive; Pending, Unverified, Verified and Invalid (and anything
 *    unrecognised) are dropped silently;
 *  * with `expectedProgramId`, a record whose event id carries a DIFFERENT program id is dropped too and
 *    counted in `droppedProgramMismatch` (program_id is part of event identity).
 */
export function mapNativeHistory(entries: readonly BridgeEntry[], expectedProgramId?: string): NativeHistoryMapping {
  const items: NativeHistoryItem[] = [];
  let droppedProgramMismatch = 0;
  for (const entry of entries) {
    const status: Status = entry.status;
    const kind = variantName(entry.descriptor.kind) as NativeHistoryKind | undefined;
    if (!kind || !KINDS.includes(kind)) continue;
    const verifiedDelivery = 'Forwarded' in status || 'Quarantined' in status;
    if (!verifiedDelivery) continue; // intake/audit data never reaches a user-facing feed
    if (expectedProgramId !== undefined && entry.descriptor.event_id.program_id !== expectedProgramId) {
      droppedProgramMismatch += 1;
      continue;
    }
    const plan = entry.descriptor.plan[0] ? (variantName(entry.descriptor.plan[0]) as NativeHistoryPlan | undefined) : undefined;
    const base = {
      seq: entry.seq,
      kind,
      beneficiaryPubkey: entry.descriptor.beneficiary_pubkey,
      basisLamports: entry.descriptor.basis_lamports,
      sourceRound: entry.descriptor.source_round_id,
      sourceCloseId: entry.descriptor.source_close_id[0] ?? null,
      plan: plan && PLANS.includes(plan) ? plan : null,
      signature: entry.descriptor.event_id.signature,
      instructionIndex: entry.descriptor.event_id.instruction_index,
    };
    if ('Forwarded' in status) {
      items.push({
        ...base,
        deliveredAt: status.Forwarded.at,
        outcome: 'delivered',
        obligationId: status.Forwarded.obligation_id,
        alreadyObserved: status.Forwarded.already_observed,
      });
    } else if ('Quarantined' in status) {
      items.push({ ...base, deliveredAt: status.Quarantined.at, outcome: 'rejected', reason: status.Quarantined.reason });
    }
  }
  items.sort((a, b) => (a.seq < b.seq ? 1 : a.seq > b.seq ? -1 : 0));
  return { items, droppedProgramMismatch };
}

/** `mapNativeHistory(...).items`: the items only (pass `expectedProgramId` to also enforce the program). */
export function toNativeHistoryItems(entries: readonly BridgeEntry[], expectedProgramId?: string): NativeHistoryItem[] {
  return mapNativeHistory(entries, expectedProgramId).items;
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

export type HistoryDisabledReason =
  /** No bridge canister id in this build. */
  | 'not-configured'
  | 'invalid-canister-id'
  /** The app's own cluster has no resolvable domain (an unconfigured endpoint, mainnet before its domain exists). */
  | 'cluster-unresolved'
  /** The bridge follows a different cluster than the app. */
  | 'cluster-mismatch'
  /** The bridge follows the app's cluster but ANOTHER program deployment on it. */
  | 'program-mismatch'
  /** The bridge's config could not be read, so its cluster cannot be checked. */
  | 'config-unavailable'
  /** The bridge canister has no config yet (it follows no cluster). */
  | 'bridge-uninitialized';

/** The configured bridge canister id, or why there is none. Pure (the env value is injected for tests). */
export function resolveBridgeCanisterId(
  raw: string | undefined = process.env.VITE_NATIVE_BRIDGE_CANISTER_ID,
): { id: string } | { disabled: HistoryDisabledReason } {
  const text = (raw ?? '').trim();
  if (!text) return { disabled: 'not-configured' };
  try {
    return { id: Principal.fromText(text).toText() };
  } catch {
    return { disabled: 'invalid-canister-id' };
  }
}

/** Page size of one getRecentEvents call (the bridge caps a page at 100). */
export const NATIVE_HISTORY_PAGE_SIZE = 20;
const REFRESH_MS = 30_000;

const actors = new Map<string, Promise<ActorSubclass<_SERVICE>>>();

/** One anonymous actor per canister id and host (queries only; fetchRootKey only against the local replica). */
function bridgeActor(canisterId: string, host: string = IC_HOST): Promise<ActorSubclass<_SERVICE>> {
  const key = `${host}|${canisterId}`;
  let actor = actors.get(key);
  if (!actor) {
    actor = (async () => {
      const agent = new HttpAgent({ host });
      if (/^https?:\/\/(localhost|127\.0\.0\.1)(:|\/|$)/.test(host)) await agent.fetchRootKey();
      return Actor.createActor<_SERVICE>(idlFactory, { agent, canisterId });
    })();
    // a failed attempt must be retryable
    actor.catch(() => actors.delete(key));
    actors.set(key, actor);
  }
  return actor;
}

// ---------------------------------------------------------------------------
// Cluster and program guard
// ---------------------------------------------------------------------------

const sameBytes = (a: ArrayLike<number>, b: ArrayLike<number>): boolean => a.length === b.length && Array.from(a).every((byte, i) => byte === b[i]);

/** Name a 32-byte cluster domain: a known cluster, else "another deployment". Never a key. */
export function clusterDomainLabel(domain: ArrayLike<number>): string {
  if (sameBytes(domain, CLUSTER_DOMAINS.localnet)) return 'localnet';
  if (sameBytes(domain, CLUSTER_DOMAINS.devnet)) return 'devnet';
  if (MAINNET_CLUSTER_DOMAIN && sameBytes(domain, MAINNET_CLUSTER_DOMAIN)) return 'mainnet-beta';
  return 'another deployment';
}

/** What the app expects the bridge to follow: the cluster domain AND the program id of the adapter's identity. */
export type ExpectedCluster = { ok: true; domain: Uint8Array; label: string; programId: string } | { ok: false };

/** The identity the app's native adapter verifies against (`expectedClusterDomain` + `programId`), or not-ok when it cannot be resolved. */
export function expectedAppCluster(identity: NativeClusterIdentity | null): ExpectedCluster {
  if (!identity) return { ok: false };
  try {
    const domain = resolveExpectedDomain(identity.cluster);
    const named = identity.cluster.expectedClusterDomain;
    return { ok: true, domain, label: typeof named === 'string' ? named : clusterDomainLabel(domain), programId: identity.cluster.programId };
  } catch {
    return { ok: false }; // e.g. NativeClusterNotConfiguredError: mainnet-beta has no domain yet
  }
}

type GuardReason = 'cluster-unresolved' | 'cluster-mismatch' | 'program-mismatch' | 'config-unavailable' | 'bridge-uninitialized';

export type ClusterVerdict =
  | { state: 'checking' }
  | { state: 'ok' }
  | { state: 'disabled'; reason: GuardReason; appCluster?: string; bridgeCluster?: string };

/** The fields of the bridge's public `getConfig()` the guard compares. */
export interface BridgeConfig {
  domain: ArrayLike<number>;
  /** Base58 program id. */
  programId: string;
}

/** What was learned about the bridge: still loading, failed, no config, or its config. */
export type BridgeRead = { loading: true } | { error: true } | { config: BridgeConfig | null };

/** Pure verdict. The order matters: nothing is ever listed unless this returns `ok`. */
export function judgeBridgeCluster(input: { expected: ExpectedCluster; bridge: BridgeRead }): ClusterVerdict {
  const { expected, bridge } = input;
  if (!expected.ok) return { state: 'disabled', reason: 'cluster-unresolved' };
  if ('loading' in bridge) return { state: 'checking' };
  if ('error' in bridge) return { state: 'disabled', reason: 'config-unavailable', appCluster: expected.label };
  if (bridge.config === null) return { state: 'disabled', reason: 'bridge-uninitialized', appCluster: expected.label };
  if (!sameBytes(bridge.config.domain, expected.domain)) {
    return { state: 'disabled', reason: 'cluster-mismatch', appCluster: expected.label, bridgeCluster: clusterDomainLabel(bridge.config.domain) };
  }
  // Same cluster: it must also be the same PROGRAM deployment (program_id is part of event identity).
  if (bridge.config.programId.trim() !== expected.programId) {
    return { state: 'disabled', reason: 'program-mismatch', appCluster: expected.label, bridgeCluster: expected.label };
  }
  return { state: 'ok' };
}

/**
 * The four things a verdict, a config read and a page cache depend on. A change of ANY of them is a different
 * scope: it gets a fresh config check and a fresh query key, never an earlier verdict or page.
 */
export interface BridgeScope {
  host: string;
  canisterId: string;
  expected: ExpectedCluster;
}

/** Key parts of a scope: host, canister id, expected cluster domain (hex), expected program id. */
export function scopeParts(scope: BridgeScope): [string, string, string, string] {
  const { expected } = scope;
  return [scope.host, scope.canisterId, expected.ok ? bytesToHex(expected.domain) : 'unresolved', expected.ok ? expected.programId : 'unresolved'];
}

export const bridgeCacheKey = (scope: BridgeScope): string => scopeParts(scope).join('|');
export const clusterCheckQueryKey = (scope: BridgeScope): readonly unknown[] => ['native-bridge-cluster', ...scopeParts(scope)];
export const historyQueryKey = (scope: BridgeScope): readonly unknown[] => ['native-bridge-history', ...scopeParts(scope)];

const bridgeConfigs = new Map<string, Promise<BridgeConfig | null>>();

/**
 * The bridge's config, cached per scope key (host, canister id, expected domain, expected program). A failed
 * read and an uninitialized bridge (null) are NOT cached, so the next check asks again; a real config is kept
 * for the life of the page, for that scope only.
 */
export function cachedBridgeConfig(key: string, load: () => Promise<BridgeConfig | null>): Promise<BridgeConfig | null> {
  const existing = bridgeConfigs.get(key);
  if (existing) return existing;
  const attempt = load().then(
    (config) => {
      if (config === null) bridgeConfigs.delete(key);
      return config;
    },
    (error: unknown) => {
      bridgeConfigs.delete(key);
      throw error;
    },
  );
  bridgeConfigs.set(key, attempt);
  return attempt;
}

export function resetBridgeConfigCacheForTests(): void {
  bridgeConfigs.clear();
}

/** Read the bridge's public config through its anonymous actor (cached per scope). */
function readBridgeConfig(scope: BridgeScope): Promise<BridgeConfig | null> {
  return cachedBridgeConfig(bridgeCacheKey(scope), async () => {
    const config = await (await bridgeActor(scope.canisterId, scope.host)).getConfig();
    return config[0] ? { domain: Uint8Array.from(config[0].cluster_domain), programId: config[0].program_id } : null;
  });
}

/** Resolve the verdict for `expected` against the bridge, reading its config only when the app's cluster is resolved. */
export async function verifyBridgeCluster(expected: ExpectedCluster, loadConfig: () => Promise<BridgeConfig | null>): Promise<ClusterVerdict> {
  if (!expected.ok) return judgeBridgeCluster({ expected, bridge: { loading: true } }); // unresolved wins before any read
  try {
    return judgeBridgeCluster({ expected, bridge: { config: await loadConfig() } });
  } catch {
    return judgeBridgeCluster({ expected, bridge: { error: true } });
  }
}

/** A listing was refused because the bridge is not the app's cluster and program (or could not be checked). */
export class BridgeClusterError extends Error {
  constructor(readonly verdict: Extract<ClusterVerdict, { state: 'disabled' }>) {
    super(`The ICP bridge is not usable for this cluster and program (${verdict.reason}).`);
    this.name = 'BridgeClusterError';
  }
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

export interface HistoryPage {
  items: NativeHistoryItem[];
  /** Raw entries the bridge returned (before filtering). A full page means there may be more. */
  rawCount: number;
  /** Oldest raw sequence number of the page: the next `before`. */
  oldestSeq: bigint | null;
  /** Verified-delivery records dropped because their event id names another program. */
  droppedProgramMismatch: number;
}

export type NativeHistoryResult =
  | {
      enabled: false;
      reason: HistoryDisabledReason;
      /** Cluster labels for a mismatch (or the app's own for an unreadable bridge): names, never keys. */
      appCluster?: string;
      bridgeCluster?: string;
    }
  | {
      enabled: true;
      items: NativeHistoryItem[];
      /** Records dropped (across the loaded pages) for carrying another program's id. Normally 0. */
      droppedProgramMismatch: number;
      isLoading: boolean;
      error: Error | null;
      hasMore: boolean;
      isLoadingMore: boolean;
      loadMore: () => void;
      refetch: () => void;
    };

/** The `before` for the next page: the oldest RAW seq of a full page; undefined when the bridge has nothing older. */
export function nextHistoryCursor(page: Pick<HistoryPage, 'rawCount' | 'oldestSeq'>, pageSize: number = NATIVE_HISTORY_PAGE_SIZE): bigint | undefined {
  return page.rawCount >= pageSize && page.oldestSeq !== null ? page.oldestSeq : undefined;
}

type RecentEventsActor = { getRecentEvents: (before: [] | [bigint], limit: bigint) => Promise<BridgeEntry[]> };

/** Fetch one page: `before` is the previous page's oldest raw seq (null = newest). `expectedProgramId` also drops other programs' records. */
export async function fetchHistoryPage(
  actor: RecentEventsActor,
  before: bigint | null,
  limit: number = NATIVE_HISTORY_PAGE_SIZE,
  expectedProgramId?: string,
): Promise<HistoryPage> {
  const raw = await actor.getRecentEvents(before === null ? [] : [before], BigInt(limit));
  let oldest: bigint | null = null;
  for (const entry of raw) if (oldest === null || entry.seq < oldest) oldest = entry.seq;
  const { items, droppedProgramMismatch } = mapNativeHistory(raw, expectedProgramId);
  return { items, rawCount: raw.length, oldestSeq: oldest, droppedProgramMismatch };
}

/**
 * Fetch one page, but only after the guard passes: nothing is listed on a cluster or program mismatch, an
 * unresolved app cluster, an uninitialized bridge or a failed config read (the listing method is never called
 * then). Records of another program are dropped from what is listed.
 */
export async function fetchGuardedPage(
  actor: RecentEventsActor,
  expected: ExpectedCluster,
  loadConfig: () => Promise<BridgeConfig | null>,
  before: bigint | null,
  limit: number = NATIVE_HISTORY_PAGE_SIZE,
): Promise<HistoryPage> {
  const verdict = await verifyBridgeCluster(expected, loadConfig);
  if (verdict.state !== 'ok' || !expected.ok) {
    throw new BridgeClusterError(verdict.state === 'disabled' ? verdict : { state: 'disabled', reason: 'cluster-unresolved' });
  }
  return fetchHistoryPage(actor, before, limit, expected.programId);
}

// ---------------------------------------------------------------------------
// Query definitions (injectable, so identity-switch behavior is testable with a real QueryClient)
// ---------------------------------------------------------------------------

export interface BridgeDeps {
  actor(scope: BridgeScope): Promise<RecentEventsActor>;
  config(scope: BridgeScope): Promise<BridgeConfig | null>;
}

export const productionBridgeDeps: BridgeDeps = {
  actor: (scope) => bridgeActor(scope.canisterId, scope.host),
  config: readBridgeConfig,
};

/** The guard query: one verdict per scope, never shared across scopes. */
export function clusterCheckOptions(scope: BridgeScope, deps: BridgeDeps = productionBridgeDeps) {
  return queryOptions<ClusterVerdict, Error>({
    queryKey: clusterCheckQueryKey(scope),
    queryFn: () => verifyBridgeCluster(scope.expected, () => deps.config(scope)),
    staleTime: 60_000,
    retry: false,
  });
}

/** The listing query: pages are cached per scope and every page re-checks the guard (through the per-scope config cache). */
export function historyOptions(scope: BridgeScope, deps: BridgeDeps = productionBridgeDeps) {
  return infiniteQueryOptions({
    queryKey: historyQueryKey(scope),
    initialPageParam: null as bigint | null,
    queryFn: async ({ pageParam }: { pageParam: bigint | null }): Promise<HistoryPage> =>
      fetchGuardedPage(await deps.actor(scope), scope.expected, () => deps.config(scope), pageParam),
    getNextPageParam: (last: HistoryPage) => nextHistoryCursor(last),
    staleTime: REFRESH_MS / 2,
  });
}

// ---------------------------------------------------------------------------
// The hook
// ---------------------------------------------------------------------------

/**
 * Verified native activity, newest first, paged by `before`. Disabled (no call, no actor) unless a bridge
 * canister id is configured. The bridge is queried anonymously; nothing here signs or moves funds.
 */
export function useNativeHistory(configuredId: string | undefined = process.env.VITE_NATIVE_BRIDGE_CANISTER_ID): NativeHistoryResult {
  const configured = useMemo(() => resolveBridgeCanisterId(configuredId), [configuredId]);
  const canisterId = 'id' in configured ? configured.id : null;
  const cluster = useNativeClusterIdentity();
  const expected = useMemo(() => expectedAppCluster(cluster.identity), [cluster.key]); // eslint-disable-line react-hooks/exhaustive-deps
  const scope: BridgeScope = { host: IC_HOST, canisterId: canisterId ?? '', expected };

  // 1. The guard: the bridge's cluster_domain AND program_id against the app's, once per scope.
  const check = useQuery({ ...clusterCheckOptions(scope), enabled: canisterId !== null });
  const verdict: ClusterVerdict = canisterId === null ? { state: 'checking' } : check.data ?? { state: 'checking' };

  // 2. The listing: only once the guard says cluster AND program match, and it re-checks before each page.
  const query = useInfiniteQuery({
    ...historyOptions(scope),
    enabled: canisterId !== null && verdict.state === 'ok',
    refetchInterval: REFRESH_MS,
    retry: (count: number, error: Error) => !(error instanceof BridgeClusterError) && count < 2,
  });
  const pages = query.data?.pages ?? [];
  const items = useMemo(() => {
    const merged = pages.flatMap((page) => page.items);
    // a refetch of earlier pages can overlap a later page boundary: keep one row per seq
    const seen = new Set<bigint>();
    return merged.filter((item) => (seen.has(item.seq) ? false : (seen.add(item.seq), true)));
  }, [query.data]); // eslint-disable-line react-hooks/exhaustive-deps

  if (canisterId === null) return { enabled: false, reason: (configured as { disabled: HistoryDisabledReason }).disabled };
  const refused = query.error instanceof BridgeClusterError ? query.error.verdict : verdict.state === 'disabled' ? verdict : null;
  if (refused) {
    // Nothing is listed: not from this query, not from an earlier scope's cache (the key carries all four identity parts).
    return { enabled: false, reason: refused.reason, appCluster: refused.appCluster, bridgeCluster: refused.bridgeCluster };
  }
  return {
    enabled: true,
    items: verdict.state === 'ok' ? items : [],
    droppedProgramMismatch: verdict.state === 'ok' ? pages.reduce((sum, page) => sum + page.droppedProgramMismatch, 0) : 0,
    isLoading: verdict.state !== 'ok' || query.isLoading,
    error: query.error,
    hasMore: verdict.state === 'ok' && !!query.hasNextPage,
    isLoadingMore: query.isFetchingNextPage,
    loadMore: () => void query.fetchNextPage(),
    refetch: () => void query.refetch(),
  };
}
