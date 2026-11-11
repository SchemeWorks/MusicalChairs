// IMPL-46b step 1, test 7: legacy (ponzi_math_sol canister) and native (program)
// positions for the SAME wallet are separate entries, never merged or summed.
import { describe, it, expect } from 'vitest';
import { selectWalletPositions, legacyEntryId, nativeEntryId } from './selectors';

const WALLET = 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp';
const OTHER = 'BwrtBnSeoK7hbfXDfPqr8p2aYj5c7JDqX6yJSaG42yFX';

describe('wallet position selector', () => {
  it('legacyAndNativePositionsNeverMergeForSameWallet', () => {
    // Same wallet, and deliberately the SAME numeric id (1) in both namespaces.
    const legacy = [{ id: 1n, amountLamports: 500_000_000n }];
    const native = [{ owner: WALLET, nonce: 1n, principal: 1_000_000_000n }];
    const entries = selectWalletPositions({ wallet: WALLET, legacy, native });

    expect(entries).toHaveLength(2);
    const [a, b] = entries;
    expect(new Set(entries.map((e) => e.id)).size).toBe(2);
    expect(new Set(entries.map((e) => e.source))).toEqual(new Set(['legacy-canister', 'native-program']));
    expect(a.localId).toBe(b.localId); // same number, still two positions
    expect(a.id).not.toBe(b.id);

    const legacyEntry = entries.find((e) => e.source === 'legacy-canister')!;
    const nativeEntry = entries.find((e) => e.source === 'native-program')!;
    expect(legacyEntry.id).toBe(legacyEntryId(1n));
    expect(nativeEntry.id).toBe(nativeEntryId(WALLET, 1n));
    expect(legacyEntry.native).toBeUndefined();
    expect(nativeEntry.legacy).toBeUndefined();
    expect(legacyEntry.legacy).toBe(legacy[0]);
    expect(nativeEntry.native).toBe(native[0]);
  });

  it('keeps every entry of both sources and orders legacy before native deterministically', () => {
    const entries = selectWalletPositions({
      wallet: WALLET,
      legacy: [{ id: 2n }, { id: 1n }],
      native: [{ owner: WALLET, nonce: 9n }, { owner: WALLET, nonce: 3n }],
    });
    expect(entries.map((e) => e.id)).toEqual([
      legacyEntryId(1n),
      legacyEntryId(2n),
      nativeEntryId(WALLET, 3n),
      nativeEntryId(WALLET, 9n),
    ]);
  });

  it("drops native positions owned by a different wallet (a stale or spoofed read can't leak in)", () => {
    const entries = selectWalletPositions({ wallet: WALLET, legacy: [], native: [{ owner: OTHER, nonce: 1n }, { owner: WALLET, nonce: 2n }] });
    expect(entries.map((e) => e.localId)).toEqual([2n]);
  });
});
