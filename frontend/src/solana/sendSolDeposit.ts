import { Connection, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';

// Devnet for now; flips to a mainnet endpoint at the M3 cutover.
export const SOLANA_RPC_ENDPOINT = 'https://api.devnet.solana.com';

// Pure: build a single-instruction SOL transfer. lamports is passed as Number
// at the SystemProgram boundary — exact for deposit-sized amounts (< 2^53) and
// type-safe across @solana/web3.js versions.
export function buildSolTransferTx(params: {
  fromPubkey: PublicKey;
  toPubkey: PublicKey;
  lamports: bigint;
  recentBlockhash: string;
}): Transaction {
  const tx = new Transaction({ feePayer: params.fromPubkey, recentBlockhash: params.recentBlockhash });
  tx.add(
    SystemProgram.transfer({
      fromPubkey: params.fromPubkey,
      toPubkey: params.toPubkey,
      lamports: Number(params.lamports),
    }),
  );
  return tx;
}

// Re-acquire the connected wallet at send time. The SIWS session persists only
// the delegation identity + pubkey (see useWallet.tsx), not a live adapter, so
// we re-instantiate and silently reconnect (already authorized for this origin)
// — mirrors connectSiws's adapter selection. Verifies the wallet matches the
// session pubkey so we never send from an unexpected account.
async function acquireConnectedAdapter(expectedPubkey: string) {
  const base = await import('@solana/wallet-adapter-base');
  const wallets = await import('@solana/wallet-adapter-wallets');
  const adapters = [new wallets.PhantomWalletAdapter(), new wallets.SolflareWalletAdapter()];
  const adapter = adapters.find((a) => a.readyState === base.WalletReadyState.Installed);
  if (!adapter) {
    throw new Error('No Solana wallet detected. Open Phantom or Solflare and try again.');
  }
  if (!adapter.connected) {
    await adapter.connect();
  }
  if (!adapter.publicKey || adapter.publicKey.toBase58() !== expectedPubkey) {
    throw new Error('Connected wallet does not match your session — reconnect your wallet and retry.');
  }
  return adapter;
}

// Build + sign + SUBMIT the exact deposit transfer via the connected wallet.
// Returns the submitted transaction signature. Throws on rejection / failure
// (caller falls back to the manual deposit-address flow).
//
// We deliberately do NOT browser-confirm the transaction. web3.js
// confirmTransaction waits on a `signatureSubscribe` over wss://, which the
// asset-canister CSP doesn't allow (connect-src sources are scheme-specific, so
// an https:// allowance does not cover wss://) — it would always fall through to
// a "block height exceeded" expiry after ~60-90s even when the tx landed fine.
// The ICP canister observer is the source of truth: it detects the deposit
// (pokeMyDeposit + the 60s timer) and opens the position. We only need the tx
// submitted; getLatestBlockhash + sendTransaction are plain https calls.
export async function sendSolDeposit(params: {
  toAddress: string;
  lamports: bigint;
  expectedPubkey: string;
}): Promise<string> {
  const adapter = await acquireConnectedAdapter(params.expectedPubkey);
  const connection = new Connection(SOLANA_RPC_ENDPOINT, 'confirmed');
  const { blockhash } = await connection.getLatestBlockhash('confirmed');
  const tx = buildSolTransferTx({
    fromPubkey: new PublicKey(params.expectedPubkey),
    toPubkey: new PublicKey(params.toAddress),
    lamports: params.lamports,
    recentBlockhash: blockhash,
  });
  return await adapter.sendTransaction(tx, connection);
}
