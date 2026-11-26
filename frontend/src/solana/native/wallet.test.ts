// R4: the signed transaction the wallet hands back must be exactly the one we
// asked it to sign: same fee payer, a valid signature by that key in slot 0,
// and byte-identical message.
import { describe, it, expect } from 'vitest';
import { Keypair, SystemProgram, Transaction } from '@solana/web3.js';
import { SignedTransactionMismatchError, WalletMismatchError, assertSignedTransactionIntact } from './wallet';

const BLOCKHASH = '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi';

function build(payer: Keypair, lamports = 5) {
  const tx = new Transaction({ feePayer: payer.publicKey, blockhash: BLOCKHASH, lastValidBlockHeight: 1 });
  tx.add(SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: Keypair.generate().publicKey, lamports }));
  return tx;
}
const intended = (tx: Transaction) => Uint8Array.from(tx.serializeMessage());

describe('assertSignedTransactionIntact', () => {
  it('accepts an honest signature and returns it', () => {
    const payer = Keypair.generate();
    const tx = build(payer);
    const message = intended(tx);
    tx.partialSign(payer);
    const signature = assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: tx });
    expect(signature).toHaveLength(64);
    expect(Buffer.from(signature).equals(tx.signature!)).toBe(true);
  });

  it('accepts a wallet that returns a re-deserialized copy of the signed transaction', () => {
    const payer = Keypair.generate();
    const tx = build(payer);
    const message = intended(tx);
    tx.partialSign(payer);
    const copy = Transaction.from(tx.serialize());
    expect(() => assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: copy })).not.toThrow();
  });

  it('rejects a changed fee payer (reason fee-payer) even when the wallet signed that new message', () => {
    const payer = Keypair.generate();
    const attacker = Keypair.generate();
    const tx = build(payer);
    const message = intended(tx);
    const swapped = new Transaction({ feePayer: attacker.publicKey, blockhash: BLOCKHASH, lastValidBlockHeight: 1 });
    swapped.add(...tx.instructions);
    swapped.partialSign(attacker);
    const error = (() => {
      try {
        assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: swapped });
      } catch (e) {
        return e;
      }
    })();
    expect(error).toBeInstanceOf(SignedTransactionMismatchError);
    expect(error).toBeInstanceOf(WalletMismatchError);
    expect((error as SignedTransactionMismatchError).reason).toBe('fee-payer');
  });

  it('rejects a swapped or extended message signed by the right key (reason message-mutated)', () => {
    const payer = Keypair.generate();
    const tx = build(payer, 5);
    const message = intended(tx);
    const tampered = build(payer, 5_000_000_000); // same payer, different transfer amount
    tampered.partialSign(payer);
    expect(() => assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: tampered })).toThrow(
      expect.objectContaining({ reason: 'message-mutated' }),
    );

    const extended = build(payer, 5);
    extended.add(SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 }));
    extended.partialSign(payer);
    expect(() => assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: extended })).toThrow(
      expect.objectContaining({ reason: 'message-mutated' }),
    );
  });

  it('rejects an unsigned transaction, a signature by another key in slot 0, and a corrupted signature (reason signature)', () => {
    const payer = Keypair.generate();
    const other = Keypair.generate();
    const check = (mutate: (tx: Transaction) => void) => {
      const tx = build(payer);
      const message = intended(tx);
      mutate(tx);
      return () => assertSignedTransactionIntact({ intendedFeePayer: payer.publicKey.toBase58(), intendedMessage: message, signed: tx });
    };

    expect(check((tx) => { tx.signatures = [{ publicKey: payer.publicKey, signature: null }]; })).toThrow(expect.objectContaining({ reason: 'signature' }));
    // slot 0 claims a different signer
    // (web3.js then recompiles the message with `other` as a required signer, so the bytes differ too)
    expect(check((tx) => { tx.signatures = [{ publicKey: other.publicKey, signature: Buffer.alloc(64, 7) }]; })).toThrow(SignedTransactionMismatchError);
    // an all-zero placeholder is not a signature
    expect(check((tx) => { tx.signatures = [{ publicKey: payer.publicKey, signature: Buffer.alloc(64) }]; })).toThrow(expect.objectContaining({ reason: 'signature' }));
    // a signature that does not verify against the message
    expect(
      check((tx) => {
        tx.partialSign(payer);
        const bytes = Uint8Array.from(tx.signature!);
        bytes[0] ^= 0xff;
        tx.signatures = [{ publicKey: payer.publicKey, signature: Buffer.from(bytes) }];
      }),
    ).toThrow(expect.objectContaining({ reason: 'signature' }));
  });
});
