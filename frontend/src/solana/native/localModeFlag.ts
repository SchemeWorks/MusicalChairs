// Dev-only "local cluster" mode switch (IMPL-46c).
//
// When on, the app skips the SIWS provider canister entirely, connects an
// in-browser burner wallet, and talks to a solana-test-validator through
// VITE_SOLANA_RPC_URL (docs/solana/local-cluster.md). It exists so the native
// flows can be driven in a browser without a browser-extension wallet.
//
// Two independent gates, both required:
//   1. import.meta.env.DEV      -- Vite replaces this with the literal `false` in
//                                  every production build, so the whole expression
//                                  below folds to `false`.
//   2. VITE_NATIVE_LOCAL_CLUSTER=1 -- exposed through vite.config.ts `define`
//      (the env files live at the repo root, outside Vite's envDir).
//
// Every use of the mode MUST be written as `if (LOCAL_CLUSTER_MODE && loadLocalModeWallet)` /
// `LOCAL_CLUSTER_MODE ? ... : undefined`, and the local-only module is reached ONLY through
// loadLocalModeWallet (localModeLoader.ts), so the bundler can drop the branch AND the chunk.
// localModeFlag.test.ts bundles those patterns (including inside a hook-style callback) with the
// production define through Rollup and asserts that nothing local-only survives.
//
// This module imports nothing (no web3.js, no React) so any bundle may load it.

/** The gate as a pure function: the build-time expression below must stay equivalent to it. */
export function isLocalClusterMode(dev: boolean, flag: string | undefined): boolean {
  return dev === true && flag === '1';
}

/** Literal-foldable form of `isLocalClusterMode(import.meta.env.DEV, process.env.VITE_NATIVE_LOCAL_CLUSTER)`. */
export const LOCAL_CLUSTER_MODE: boolean = import.meta.env.DEV === true && process.env.VITE_NATIVE_LOCAL_CLUSTER === '1';

// ---------------------------------------------------------------------------
// Loopback guard
// ---------------------------------------------------------------------------

/**
 * The only RPC hosts local-cluster mode may sign, send or airdrop against.
 * Deliberately a closed list (no `*.localhost`, no `0.0.0.0`, no private
 * ranges): a burner or imported development key must never touch a network
 * that other people use.
 */
export const LOOPBACK_RPC_HOSTS: readonly string[] = ['127.0.0.1', 'localhost', '[::1]'];

/** True when `endpoint` is an http(s) URL whose host is exactly 127.0.0.1, localhost or [::1]. */
export function isLoopbackRpcEndpoint(endpoint: string): boolean {
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    return false;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return false;
  return LOOPBACK_RPC_HOSTS.includes(url.hostname);
}

export class LoopbackGuardError extends Error {
  constructor(what: string) {
    super(
      `Local cluster mode refuses to ${what}: VITE_SOLANA_RPC_URL must point at 127.0.0.1, localhost or [::1]. ` +
        'It never signs, sends or airdrops against a shared network.',
    );
    this.name = 'LoopbackGuardError';
  }
}

/** Throws LoopbackGuardError unless `endpoint` is loopback. `what` completes "refuses to ...". */
export function assertLoopbackRpcEndpoint(endpoint: string, what: string): void {
  if (!isLoopbackRpcEndpoint(endpoint)) throw new LoopbackGuardError(what);
}
