import { describe, it, expect, vi } from 'vitest';
import { PublicKey, SystemInstruction } from '@solana/web3.js';
import { buildSolTransferTx, withTimeout } from './sendSolDeposit';

const FROM = new PublicKey('So11111111111111111111111111111111111111112'); // wSOL mint (valid base58 pubkey)
const TO = new PublicKey('Vote111111111111111111111111111111111111111');   // vote program (valid base58 pubkey)
const BLOCKHASH = 'GfVcyD4kkTrj4bKc7WA9sZCin9JDbdT4Zkd3EZD3GcVz';

describe('buildSolTransferTx', () => {
  it('builds a single System transfer of the exact lamports to the exact recipient', () => {
    const tx = buildSolTransferTx({ fromPubkey: FROM, toPubkey: TO, lamports: 11_000_000n, recentBlockhash: BLOCKHASH });
    expect(tx.instructions).toHaveLength(1);
    const decoded = SystemInstruction.decodeTransfer(tx.instructions[0]);
    expect(decoded.fromPubkey.toBase58()).toBe(FROM.toBase58());
    expect(decoded.toPubkey.toBase58()).toBe(TO.toBase58());
    expect(BigInt(decoded.lamports)).toBe(11_000_000n);
    expect(tx.feePayer?.toBase58()).toBe(FROM.toBase58());
    expect(tx.recentBlockhash).toBe(BLOCKHASH);
  });

  // The bigint -> Number narrowing at the SystemProgram boundary must be exact
  // for deposit-sized amounts. Min deposit is 0.01 SOL (10_000_000 lamports).
  it('preserves the exact lamports for the 0.01 SOL minimum deposit', () => {
    const tx = buildSolTransferTx({ fromPubkey: FROM, toPubkey: TO, lamports: 10_000_000n });
    const decoded = SystemInstruction.decodeTransfer(tx.instructions[0]);
    expect(BigInt(decoded.lamports)).toBe(10_000_000n);
  });

  it('preserves exact lamports for a large (whole-SOL) deposit', () => {
    // 1,234.567891234 SOL — well within Number.MAX_SAFE_INTEGER (< 2^53).
    const lamports = 1_234_567_891_234n;
    const tx = buildSolTransferTx({ fromPubkey: FROM, toPubkey: TO, lamports });
    const decoded = SystemInstruction.decodeTransfer(tx.instructions[0]);
    expect(BigInt(decoded.lamports)).toBe(lamports);
  });

  it('omits recentBlockhash when not supplied (Phantom fills it in)', () => {
    const tx = buildSolTransferTx({ fromPubkey: FROM, toPubkey: TO, lamports: 10_000_000n });
    expect(tx.recentBlockhash).toBeUndefined();
    expect(tx.feePayer?.toBase58()).toBe(FROM.toBase58());
  });
});

describe('withTimeout', () => {
  it('resolves with the inner value when it settles before the timeout', async () => {
    await expect(withTimeout(Promise.resolve('ok'), 1000, 'too slow')).resolves.toBe('ok');
  });

  it('propagates the inner rejection (not the timeout message)', async () => {
    await expect(withTimeout(Promise.reject(new Error('inner')), 1000, 'too slow')).rejects.toThrow('inner');
  });

  it('rejects with the timeout message when the inner promise never settles', async () => {
    vi.useFakeTimers();
    const pending = new Promise<string>(() => {}); // never settles
    const guarded = withTimeout(pending, 20_000, 'too slow');
    const assertion = expect(guarded).rejects.toThrow('too slow');
    await vi.advanceTimersByTimeAsync(20_000);
    await assertion;
    vi.useRealTimers();
  });
});
