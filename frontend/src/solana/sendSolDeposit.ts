import { Connection, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import { SOLANA_RPC_ENDPOINT } from './rpc';

// Re-exported for backward compatibility with existing importers. The canonical
// definition lives in ./rpc (dependency-free) so app-wide code can import the
// endpoint without pulling @solana/web3.js into the main bundle.
export { SOLANA_RPC_ENDPOINT };

// Pure: build a single-instruction SOL transfer. lamports is passed as Number
// at the SystemProgram boundary — exact for deposit-sized amounts (< 2^53) and
// type-safe across @solana/web3.js versions. recentBlockhash is OPTIONAL: the
// preferred Phantom path leaves it unset and lets the wallet fill it in.
export function buildSolTransferTx(params: {
  fromPubkey: PublicKey;
  toPubkey: PublicKey;
  lamports: bigint;
  recentBlockhash?: string;
}): Transaction {
  const tx = new Transaction(
    params.recentBlockhash
      ? { feePayer: params.fromPubkey, recentBlockhash: params.recentBlockhash }
      : { feePayer: params.fromPubkey },
  );
  tx.add(
    SystemProgram.transfer({
      fromPubkey: params.fromPubkey,
      toPubkey: params.toPubkey,
      lamports: Number(params.lamports),
    }),
  );
  return tx;
}

// Reject a hung promise after `ms`. Used to bound the pre-broadcast blockhash
// fetch so a slow/unreachable RPC can't leave the deposit UI stuck on "Confirm
// in your wallet…" forever with no wallet prompt ever appearing.
//
// SAFETY: only ever apply this to PRE-broadcast steps. Timing out the wallet's
// signAndSendTransaction would be dangerous — the timeout could fire AFTER the
// transaction actually broadcast, so the user would retry and double-deposit.
export function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (err) => {
        clearTimeout(timer);
        reject(err);
      },
    );
  });
}

const BLOCKHASH_TIMEOUT_MS = 20_000;

// The injected Phantom provider, if present. Phantom exposes
// `signAndSendTransaction`, which supplies a recent blockhash AND broadcasts via
// Phantom's own RPC — so the app needs no RPC at all for the user's payment.
function getPhantomProvider(): any {
  if (typeof window === 'undefined') return undefined;
  const w = window as any;
  if (w?.phantom?.solana?.isPhantom) return w.phantom.solana;
  if (w?.solana?.isPhantom) return w.solana;
  return undefined;
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
  const fromPubkey = new PublicKey(params.expectedPubkey);

  // A recent blockhash is REQUIRED to build/sign the tx. Phantom does NOT supply
  // one for signAndSendTransaction — its provider serializes the tx client-side
  // first ("Transaction recentBlockhash required" otherwise) — so we must fetch
  // it ourselves. The configured RPC (SOLANA_RPC_ENDPOINT) is the domain-locked
  // Helius endpoint; the public mainnet endpoint 403s browser calls.
  const connection = new Connection(SOLANA_RPC_ENDPOINT, 'confirmed');
  const { blockhash } = await withTimeout(
    connection.getLatestBlockhash('confirmed'),
    BLOCKHASH_TIMEOUT_MS,
    'Could not reach Solana to prepare the transfer (network slow or RPC unavailable). Use the manual deposit address below instead.',
  );
  const tx = buildSolTransferTx({
    fromPubkey,
    toPubkey: new PublicKey(params.toAddress),
    lamports: params.lamports,
    recentBlockhash: blockhash,
  });

  // Preferred path: Phantom's signAndSendTransaction — Phantom BROADCASTS via its
  // own RPC (reliable submit), so Helius is used only for the blockhash above.
  const phantom = getPhantomProvider();
  if (phantom?.signAndSendTransaction) {
    if (!phantom.isConnected) {
      await phantom.connect();
    }
    const connectedPk = phantom.publicKey?.toString?.();
    if (connectedPk && connectedPk !== params.expectedPubkey) {
      throw new Error('Connected wallet does not match your session — reconnect your wallet and retry.');
    }
    const result = await phantom.signAndSendTransaction(tx);
    // Phantom returns { signature } (older builds may return the string directly).
    return typeof result === 'string' ? result : result.signature;
  }

  // Fallback for non-Phantom wallets (e.g. Solflare): adapter submits via the
  // same RPC connection.
  const adapter = await acquireConnectedAdapter(params.expectedPubkey);
  return await adapter.sendTransaction(tx, connection);
}
