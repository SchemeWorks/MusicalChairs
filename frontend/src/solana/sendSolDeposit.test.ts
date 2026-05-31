import { describe, it, expect } from 'vitest';
import { PublicKey, SystemInstruction } from '@solana/web3.js';
import { buildSolTransferTx } from './sendSolDeposit';

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
});
