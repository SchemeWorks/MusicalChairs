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

import { useMemo } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { Actor, HttpAgent, type ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { idlFactory } from '../declarations/native_sol_bridge/native_sol_bridge.did.js';
import type { Entry, Status, _SERVICE } from '../declarations/native_sol_bridge/native_sol_bridge.did';
import { IC_HOST } from './useWallet';

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

/**
 * Bridge entries -> user-facing items, newest first. DEFENSIVE: only Forwarded and Quarantined survive;
 * Pending, Unverified, Verified and Invalid (and anything unrecognised) are dropped.
 */
export function toNativeHistoryItems(entries: readonly BridgeEntry[]): NativeHistoryItem[] {
  const items: NativeHistoryItem[] = [];
  for (const entry of entries) {
    const status: Status = entry.status;
    const kind = variantName(entry.descriptor.kind) as NativeHistoryKind | undefined;
    if (!kind || !KINDS.includes(kind)) continue;
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
    // every other status is intake/audit data and never reaches a user-facing feed
  }
  return items.sort((a, b) => (a.seq < b.seq ? 1 : a.seq > b.seq ? -1 : 0));
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

export type HistoryDisabledReason = 'not-configured' | 'invalid-canister-id';

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
// The hook
// ---------------------------------------------------------------------------

export interface HistoryPage {
  items: NativeHistoryItem[];
  /** Raw entries the bridge returned (before filtering). A full page means there may be more. */
  rawCount: number;
  /** Oldest raw sequence number of the page: the next `before`. */
  oldestSeq: bigint | null;
}

export type NativeHistoryResult =
  | { enabled: false; reason: HistoryDisabledReason }
  | {
      enabled: true;
      items: NativeHistoryItem[];
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

/** Fetch one page: `before` is the previous page's oldest raw seq (null = newest). */
export async function fetchHistoryPage(
  actor: { getRecentEvents: (before: [] | [bigint], limit: bigint) => Promise<BridgeEntry[]> },
  before: bigint | null,
  limit: number = NATIVE_HISTORY_PAGE_SIZE,
): Promise<HistoryPage> {
  const raw = await actor.getRecentEvents(before === null ? [] : [before], BigInt(limit));
  let oldest: bigint | null = null;
  for (const entry of raw) if (oldest === null || entry.seq < oldest) oldest = entry.seq;
  return { items: toNativeHistoryItems(raw), rawCount: raw.length, oldestSeq: oldest };
}

/**
 * Verified native activity, newest first, paged by `before`. Disabled (no call, no actor) unless a bridge
 * canister id is configured. The bridge is queried anonymously; nothing here signs or moves funds.
 */
export function useNativeHistory(configuredId: string | undefined = process.env.VITE_NATIVE_BRIDGE_CANISTER_ID): NativeHistoryResult {
  const configured = useMemo(() => resolveBridgeCanisterId(configuredId), [configuredId]);
  const canisterId = 'id' in configured ? configured.id : null;
  const query = useInfiniteQuery<HistoryPage, Error, { pages: HistoryPage[] }, readonly unknown[], bigint | null>({
    queryKey: ['native-bridge-history', canisterId],
    initialPageParam: null,
    queryFn: async ({ pageParam }) => fetchHistoryPage(await bridgeActor(canisterId as string), pageParam),
    getNextPageParam: (last) => nextHistoryCursor(last),
    enabled: canisterId !== null,
    refetchInterval: REFRESH_MS,
    staleTime: REFRESH_MS / 2,
    retry: 2,
  });
  const items = useMemo(() => {
    const merged = (query.data?.pages ?? []).flatMap((page) => page.items);
    // a refetch of earlier pages can overlap a later page boundary: keep one row per seq
    const seen = new Set<bigint>();
    return merged.filter((item) => (seen.has(item.seq) ? false : (seen.add(item.seq), true)));
  }, [query.data]);
  if (canisterId === null) return { enabled: false, reason: (configured as { disabled: HistoryDisabledReason }).disabled };
  return {
    enabled: true,
    items,
    isLoading: query.isLoading,
    error: query.error,
    hasMore: !!query.hasNextPage,
    isLoadingMore: query.isFetchingNextPage,
    loadMore: () => void query.fetchNextPage(),
    refetch: () => void query.refetch(),
  };
}
