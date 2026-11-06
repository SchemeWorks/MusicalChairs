// Position listing that keeps the two Solana back ends apart.
//
// The legacy ponzi_math_sol canister (SOL deposits observed by the ICP
// canister) and the native Musical Chairs program (positions in program-owned
// accounts) can both hold positions for the same wallet at the same time, and
// their numeric ids collide (legacy game id 1, native client nonce 1). They are
// different games with different custody, so they are NEVER merged, summed or
// de-duplicated: every entry carries a source-qualified id and exactly one
// payload.
//
// Pure module: no web3.js, no React.

import type { WalletPositionEntry } from './types';

export const NATIVE_PROGRAM_LABEL = 'musical-chairs-native';
export const LEGACY_CANISTER_LABEL = 'ponzi_math_sol';

export const legacyEntryId = (gameId: bigint | number | string): string =>
  `legacy-canister:${LEGACY_CANISTER_LABEL}:${String(gameId)}`;

export const nativeEntryId = (owner: string, nonce: bigint): string =>
  `native-program:${NATIVE_PROGRAM_LABEL}:${owner}:${nonce.toString()}`;

export interface LegacyPositionInput {
  id: bigint | number | string;
}

export interface NativePositionInput {
  owner: string;
  nonce: bigint;
}

const compareBigInt = (a: bigint, b: bigint): number => (a < b ? -1 : a > b ? 1 : 0);

export function selectWalletPositions<L extends LegacyPositionInput, N extends NativePositionInput>(args: {
  wallet: string;
  legacy: readonly L[];
  native: readonly N[];
}): Array<WalletPositionEntry<L, N>> {
  const legacyEntries: Array<WalletPositionEntry<L, N>> = args.legacy
    .map((legacy) => ({
      id: legacyEntryId(legacy.id),
      source: 'legacy-canister' as const,
      localId: BigInt(legacy.id),
      wallet: args.wallet,
      legacy,
    }))
    .sort((a, b) => compareBigInt(a.localId, b.localId));

  // A native position belongs to its Position.owner. Anything else is not this
  // wallet's, whatever the caller handed in.
  const nativeEntries: Array<WalletPositionEntry<L, N>> = args.native
    .filter((native) => native.owner === args.wallet)
    .map((native) => ({
      id: nativeEntryId(native.owner, native.nonce),
      source: 'native-program' as const,
      localId: native.nonce,
      wallet: args.wallet,
      native,
    }))
    .sort((a, b) => compareBigInt(a.localId, b.localId));

  return [...legacyEntries, ...nativeEntries];
}
