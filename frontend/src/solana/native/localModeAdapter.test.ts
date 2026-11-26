// IMPL-46c: the real UnsafeBurnerWalletAdapter (not a fake) can be seated with a restored/imported key.
// localModeWallet.ts replaces the adapter's private `_keypair` after connect(); this pins that this still
// works against the installed adapter package, so an upgrade that renames the field fails here.
// (The app loads it through @solana/wallet-adapter-wallets, which re-exports this same class but does not
// load under Node because of its Ledger dependency; the test therefore injects the class directly.)
import { describe, expect, it, vi } from 'vitest';
import { Keypair, SystemProgram, Transaction } from '@solana/web3.js';
import bs58 from 'bs58';

vi.mock('./localModeFlag', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./localModeFlag')>()),
  LOCAL_CLUSTER_MODE: true,
}));

import { connectLocalClusterWallet, loadLocalClusterWallet, resetLocalClusterWalletForTests, type BurnerAdapter } from './localModeWallet';

describe('the real UnsafeBurnerWalletAdapter', () => {
  it('signs as an imported key after connect() generated its own', async () => {
    resetLocalClusterWalletForTests();
    const owner = Keypair.generate();
    const store = new Map<string, string>();
    const { UnsafeBurnerWalletAdapter } = await import('@solana/wallet-adapter-unsafe-burner');
    const deps = {
      endpoint: 'http://127.0.0.1:8899',
      createAdapter: async () => new UnsafeBurnerWalletAdapter() as unknown as BurnerAdapter,
      storage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) },
      createConnection: () => ({ getBalance: async () => 10_000_000_000, requestAirdrop: async () => 'unused' }),
      sleep: async () => undefined,
    };
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined); // the adapter warns on construction
    try {
      const session = await connectLocalClusterWallet({ secret: bs58.encode(owner.secretKey) }, deps);
      expect(session.pubkey).toBe(owner.publicKey.toBase58());
      const wallet = (await loadLocalClusterWallet(session.pubkey, deps))!;
      expect(wallet.publicKey).toBe(owner.publicKey.toBase58());
      const tx = new Transaction({ feePayer: owner.publicKey, recentBlockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi' }).add(
        SystemProgram.transfer({ fromPubkey: owner.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 }),
      );
      expect((await wallet.signTransaction(tx)).verifySignatures()).toBe(true);
    } finally {
      warn.mockRestore();
      resetLocalClusterWalletForTests();
    }
  });
});
