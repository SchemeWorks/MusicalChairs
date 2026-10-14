// Wallet binding for native transactions.
//
// The SIWS session records the Solana pubkey the user signed in with
// (useWallet().solanaPubkey). Every native transaction must be signed by THAT
// key: if the user switched accounts inside Phantom/Solflare after signing in,
// the connected wallet no longer matches the session and the client refuses to
// build, sign or send (WalletMismatchError) before any network I/O.
//
// Why signTransaction + our own sendRawTransaction (not Phantom's
// signAndSendTransaction, which sendSolDeposit.ts uses for the legacy path):
// the signature of a signed legacy transaction is known BEFORE it is
// broadcast. If the RPC dies during sendRawTransaction we still hold the
// signature and can keep polling it, which is what keeps an unknown outcome
// from turning into a second, conflicting transaction. signAndSendTransaction
// would only hand the signature back on success.
//
// Type-only web3.js import: this module is safe for any bundle.

import type { Transaction } from '@solana/web3.js';

export interface NativeWallet {
  /** Base58 public key of the currently selected account, or null when disconnected. */
  readonly publicKey: string | null;
  signTransaction(tx: Transaction): Promise<Transaction>;
}

export class WalletMismatchError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'WalletMismatchError';
  }
}

/** The wallet/provider returned something other than the transaction we asked it to sign. Nothing was broadcast. */
export class SignedTransactionMismatchError extends WalletMismatchError {
  constructor(
    readonly reason: 'fee-payer' | 'message-mutated' | 'signature',
    message: string,
  ) {
    super(message);
    this.name = 'SignedTransactionMismatchError';
  }
}

const sameBytes = (a: Uint8Array, b: Uint8Array): boolean => a.length === b.length && a.every((byte, i) => byte === b[i]);

/**
 * The signed transaction must be exactly what we built: (1) the fee payer is
 * the intended one, (2) the message bytes are identical to those captured
 * BEFORE the wallet saw the transaction (a wallet can mutate the object in
 * place, so the caller passes bytes, not the object), (3) slot 0 holds a
 * 64-byte signature attributed to the fee payer, and (4) every signature
 * verifies against that message. Returns the fee payer's signature.
 */
export function assertSignedTransactionIntact(args: {
  intendedFeePayer: string;
  intendedMessage: Uint8Array;
  signed: Transaction;
}): Uint8Array {
  const { intendedFeePayer, intendedMessage, signed } = args;
  if (signed.feePayer?.toBase58() !== intendedFeePayer) {
    throw new SignedTransactionMismatchError(
      'fee-payer',
      'The wallet changed the fee payer of the transaction — nothing was sent. Reconnect your wallet and retry.',
    );
  }
  let message: Uint8Array | null = null;
  try {
    message = signed.serializeMessage();
  } catch {
    message = null;
  }
  if (!message || !sameBytes(message, intendedMessage)) {
    throw new SignedTransactionMismatchError(
      'message-mutated',
      'The wallet returned a different transaction than the one it was asked to sign — nothing was sent.',
    );
  }
  const first = signed.signatures[0];
  const signature = first?.signature;
  if (
    !first ||
    first.publicKey.toBase58() !== intendedFeePayer ||
    !signature ||
    signature.length !== 64 ||
    signature.every((byte) => byte === 0) ||
    !signed.verifySignatures()
  ) {
    throw new SignedTransactionMismatchError(
      'signature',
      'The wallet did not return a valid signature from your session account — reconnect your wallet and retry.',
    );
  }
  return signature;
}

/** Throws unless the selected wallet is the one the SIWS session belongs to. Returns the pubkey. */
export function assertWalletMatchesSession(wallet: NativeWallet, sessionPubkey: string | null): string {
  if (!sessionPubkey) {
    throw new WalletMismatchError('No Solana session — sign in with a Solana wallet first.');
  }
  const connected = wallet.publicKey;
  if (!connected) {
    throw new WalletMismatchError('Wallet is not connected — reconnect your wallet and retry.');
  }
  if (connected !== sessionPubkey) {
    throw new WalletMismatchError(
      'Connected wallet does not match your session — switch back to the account you signed in with, or reconnect, and retry.',
    );
  }
  return connected;
}

// The injected Phantom provider, if present (same lookup as sendSolDeposit.ts).
function getPhantomProvider(): any {
  if (typeof window === 'undefined') return undefined;
  const w = window as any;
  if (w?.phantom?.solana?.isPhantom) return w.phantom.solana;
  if (w?.solana?.isPhantom) return w.solana;
  return undefined;
}

/**
 * Re-acquire the connected wallet at send time (the SIWS session persists only
 * the pubkey, not a live adapter) and wrap it as a NativeWallet. Phantom's
 * injected provider is preferred; Solflare/Phantom wallet-adapter instances are
 * the fallback. The pubkey is checked against the session here AND again on
 * every send.
 */
export async function acquireNativeWallet(sessionPubkey: string): Promise<NativeWallet> {
  const phantom = getPhantomProvider();
  if (phantom?.signTransaction) {
    if (!phantom.isConnected) await phantom.connect();
    const wallet: NativeWallet = {
      get publicKey() {
        return phantom.publicKey?.toString?.() ?? null;
      },
      signTransaction: (tx) => phantom.signTransaction(tx),
    };
    assertWalletMatchesSession(wallet, sessionPubkey);
    return wallet;
  }

  const base = await import('@solana/wallet-adapter-base');
  const wallets = await import('@solana/wallet-adapter-wallets');
  const adapters = [new wallets.PhantomWalletAdapter(), new wallets.SolflareWalletAdapter()];
  const adapter = adapters.find((a) => a.readyState === base.WalletReadyState.Installed);
  if (!adapter) throw new Error('No Solana wallet detected. Open Phantom or Solflare and try again.');
  if (!adapter.connected) await adapter.connect();
  const signer = adapter as unknown as { signTransaction?: (tx: Transaction) => Promise<Transaction> };
  if (!signer.signTransaction) throw new Error('This wallet cannot sign transactions for the native program.');
  const wallet: NativeWallet = {
    get publicKey() {
      return adapter.publicKey?.toBase58() ?? null;
    },
    signTransaction: (tx) => signer.signTransaction!(tx),
  };
  assertWalletMatchesSession(wallet, sessionPubkey);
  return wallet;
}
