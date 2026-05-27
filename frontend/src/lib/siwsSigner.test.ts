import { describe, it, expect } from 'vitest';

import { buildSiwsMessageText, truncateSolanaPubkey } from './siwsSigner';
import type { SiwsMessage } from '../declarations/siws_provider/siws_provider.did';

describe('buildSiwsMessageText', () => {
  // CRITICAL regression guard. The byte sequence below MUST match what the
  // ic_siws_provider canister reconstructs server-side via the upstream Rust
  // crate (packages/ic_siws/src/siws.rs, `From<SiwsMessage> for String`).
  //
  // If you have to update this expectation, do it deliberately:
  //   1. Confirm the upstream Rust source still produces the same bytes for
  //      the fixture below
  //   2. Confirm the canister wasm we deploy is built from a version that
  //      produces matching output
  //   3. Re-run a real Phantom sign-in end-to-end against the canister to
  //      catch anything this fixture can't (e.g., wallet-side encoding quirks)
  //
  // Deliberately updating without (1)-(3) means production SIWS sign-in
  // will silently break — the canister will reject every signature.
  it('matches the upstream Rust canonical format byte-for-byte', () => {
    const msg: SiwsMessage = {
      uri: 'https://example.com',
      // 1704067200 seconds since epoch = 2024-01-01T00:00:00.000Z exactly.
      // Multiplied to nanoseconds to match what the canister returns.
      issued_at: 1_704_067_200_000_000_000n,
      domain: 'example.com',
      statement: 'Test SIWS message.',
      version: 1,
      chain_id: 'mainnet',
      address: 'TestAddress',
      nonce: 'testNonce',
      // +5 minutes from issued_at.
      expiration_time: 1_704_067_500_000_000_000n,
    };

    const expected =
      'example.com wants you to sign in with your Solana account:\n' +
      'TestAddress\n' +
      '\n' +
      'Test SIWS message.\n' +
      '\n' +
      'URI: https://example.com\n' +
      'Version: 1\n' +
      'Chain ID: mainnet\n' +
      'Nonce: testNonce\n' +
      'Issued At: 2024-01-01T00:00:00.000Z\n' +
      'Expiration Time: 2024-01-01T00:05:00.000Z';

    expect(buildSiwsMessageText(msg)).toBe(expected);
  });

  it('preserves Unicode characters in domain and statement', () => {
    const msg: SiwsMessage = {
      uri: 'https://example.com',
      issued_at: 1_704_067_200_000_000_000n,
      domain: 'example.com',
      // Edge case: statement with non-ASCII characters. The upstream Rust
      // string formatter is UTF-8 native; our JS code uses TextEncoder which
      // is also UTF-8. They should produce identical bytes for non-ASCII.
      statement: 'Sign in — let’s play.',
      version: 1,
      chain_id: 'mainnet',
      address: 'TestAddress',
      nonce: 'n',
      expiration_time: 1_704_067_500_000_000_000n,
    };

    const result = buildSiwsMessageText(msg);
    expect(result).toContain('Sign in — let’s play.');
    // Verify the UTF-8 byte length matches what we expect (the em-dash and
    // typographic apostrophe each occupy multiple bytes when encoded).
    const utf8Bytes = new TextEncoder().encode(result);
    expect(utf8Bytes.byteLength).toBeGreaterThan(result.length);
  });
});

describe('truncateSolanaPubkey', () => {
  it('truncates a 44-character base58 pubkey to A1b2…Z9y8 format', () => {
    expect(truncateSolanaPubkey('5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2')).toBe(
      '5EVd…8QQ2',
    );
  });

  it('returns short inputs unchanged', () => {
    expect(truncateSolanaPubkey('short')).toBe('short');
    expect(truncateSolanaPubkey('exactly12chr')).toBe('exactly12chr');
  });

  it('truncates anything strictly longer than 12 chars', () => {
    expect(truncateSolanaPubkey('thirteen-char')).toBe('thir…char');
  });
});
