// The ONLY place the dev-only local-cluster wallet module is imported (IMPL-46c).
//
//   export const loadLocalModeWallet = LOCAL_CLUSTER_MODE ? () => import('./localModeWallet') : undefined;
//
// Why a loader module instead of `if (LOCAL_CLUSTER_MODE) { await import(...) }` at each use site:
// Rollup (Vite's bundler) includes a dynamic import that sits in a function it cannot prove is
// never called (any function handed to a hook such as useCallback / useEffect), even when the `if`
// test folds to false afterwards. The branch body is then removed at render time, but the module
// has already been chunked, which left an orphan `localModeWallet-*.js` (secret-key name, airdrop
// helper) in the production output. Here the import expression is the consequent of a ternary
// whose test is the literal-folded flag, so it is never included: in a production build
// `loadLocalModeWallet` is `undefined`, nothing imports the module, and no chunk is emitted.
//
// Use sites test `if (loadLocalModeWallet) { const local = await loadLocalModeWallet(); ... }`.
// localModeFlag.test.ts bundles this exact pattern (inside a hook-style callback) with the
// production define and asserts that nothing local-only survives; panelsHygiene.test.ts pins that
// this file holds the only import of localModeWallet.

import { LOCAL_CLUSTER_MODE } from './localModeFlag';

export type LocalModeWalletModule = typeof import('./localModeWallet');

export const loadLocalModeWallet: (() => Promise<LocalModeWalletModule>) | undefined = LOCAL_CLUSTER_MODE
  ? () => import('./localModeWallet')
  : undefined;
