// Which rail (native program or legacy canister) a SOL panel shows (IMPL-46c, DR-22).
// Pure and web3-free: panelsHooks.useNativeAvailability feeds it the Pool query, the tests feed it
// every outcome.
//
// The rule, stated once so no panel re-derives it:
//   * native      the program's Pool exists on the connected, VERIFIED cluster -> native UI first,
//                 legacy canister positions in their own labeled section.
//   * loading     the Pool read is in flight -> neutral "connecting" state, no rail yet.
//   * unavailable ANY other outcome -> the LEGACY rail only, with no native UI at all. That
//                 includes the default endpoint (mainnet-beta has no cluster domain until its
//                 genesis chooses one, so cluster verification fails closed with
//                 NativeClusterNotConfiguredError), an RPC that names no cluster, a cluster
//                 mismatch, an RPC error and a cluster where the program is not initialized.
//                 `reason` says which, for the local-cluster message and for debugging.

import { NativeClusterMismatchError, NativeClusterNotConfiguredError } from './cluster';
import type { NativePool } from './types';

export type NativeAvailabilityState =
  /** Not a Solana session: nothing to decide. */
  | 'inactive'
  | 'loading'
  | 'native'
  | 'unavailable';

export type UnavailableReason =
  /** The endpoint names no cluster, so no native identity exists. */
  | 'no-cluster'
  /** The cluster is known but has no verifiable deployment yet (e.g. mainnet-beta before its domain is chosen). */
  | 'cluster-not-configured'
  /** The RPC is not the expected cluster, or the program there is a different deployment. */
  | 'cluster-mismatch'
  /** The Pool could not be read (RPC trouble). */
  | 'rpc-error'
  /** A verified cluster with no Pool account: the program is not initialized there. */
  | 'program-not-initialized';

export interface NativeAvailability {
  state: NativeAvailabilityState;
  pool: NativePool | null;
  reason: UnavailableReason | null;
  /** The error behind `reason`, when there is one. */
  error: Error | null;
}

export type Rail = 'inactive' | 'loading' | 'native' | 'legacy';

export function resolveAvailability(input: {
  siws: boolean;
  /** An identity resolved for the build/session (false when the endpoint names no cluster). */
  hasIdentity: boolean;
  /** Why no identity resolved. */
  identityError: Error | null;
  pool: NativePool | null | undefined;
  poolLoading: boolean;
  poolError: unknown;
}): NativeAvailability {
  const unavailable = (reason: UnavailableReason, error: unknown = null): NativeAvailability => ({
    state: 'unavailable',
    pool: null,
    reason,
    error: error instanceof Error ? error : null,
  });
  if (!input.siws) return { state: 'inactive', pool: null, reason: null, error: null };
  if (input.pool) return { state: 'native', pool: input.pool, reason: null, error: null };
  if (!input.hasIdentity) return unavailable('no-cluster', input.identityError);
  if (input.poolLoading) return { state: 'loading', pool: null, reason: null, error: null };
  if (input.poolError instanceof NativeClusterNotConfiguredError) return unavailable('cluster-not-configured', input.poolError);
  if (input.poolError instanceof NativeClusterMismatchError) return unavailable('cluster-mismatch', input.poolError);
  if (input.poolError) return unavailable('rpc-error', input.poolError);
  return unavailable('program-not-initialized');
}

/** The rail a panel renders. `unavailable` is ALWAYS the legacy rail: no native UI, never a blank panel. */
export function railOf(availability: Pick<NativeAvailability, 'state'>): Rail {
  switch (availability.state) {
    case 'native':
      return 'native';
    case 'loading':
      return 'loading';
    case 'unavailable':
      return 'legacy';
    default:
      return 'inactive';
  }
}

/** One sentence for why the native rail is unavailable (shown in local-cluster mode, where there is no legacy rail to fall back to). */
export function describeUnavailable(availability: Pick<NativeAvailability, 'reason' | 'error'>): string {
  switch (availability.reason) {
    case 'cluster-mismatch':
      return availability.error?.message ?? 'The connected RPC is not the cluster this app expects. Nothing was read or signed.';
    case 'cluster-not-configured':
    case 'no-cluster':
      return "The Solana RPC endpoint doesn't resolve to a cluster the native program can be verified on.";
    case 'rpc-error':
      return 'Could not read the native program from the Solana RPC. Is the validator running?';
    default:
      return "The native program isn't initialized on this cluster. Start the local validator and run the genesis step (docs/solana/local-cluster.md), then reload.";
  }
}
