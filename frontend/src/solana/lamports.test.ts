import { describe, it, expect } from 'vitest';
import { formatSOL, parseSOL, LAMPORTS_PER_SOL } from './lamports';

describe('formatSOL', () => {
  it('formats 0 lamports as "0.0000"', () => {
    expect(formatSOL(0n)).toBe('0.0000');
  });

  it('formats 1 lamport as "0.000000001"', () => {
    expect(formatSOL(1n)).toBe('0.000000001');
  });

  it('formats 1 SOL (1_000_000_000 lamports) as "1.0000"', () => {
    expect(formatSOL(1_000_000_000n)).toBe('1.0000');
  });

  it('formats 1.5 SOL as "1.5000"', () => {
    expect(formatSOL(1_500_000_000n)).toBe('1.5000');
  });

  it('handles values beyond Number.MAX_SAFE_INTEGER', () => {
    const huge = 10_000_000n * LAMPORTS_PER_SOL;
    expect(formatSOL(huge)).toBe('10000000.0000');
  });
});

describe('parseSOL', () => {
  it('parses "1" as 1_000_000_000n', () => {
    expect(parseSOL('1')).toBe(1_000_000_000n);
  });

  it('parses "1.5" as 1_500_000_000n', () => {
    expect(parseSOL('1.5')).toBe(1_500_000_000n);
  });

  it('parses "0.000000001" as 1n', () => {
    expect(parseSOL('0.000000001')).toBe(1n);
  });

  it('trims whitespace', () => {
    expect(parseSOL('  2  ')).toBe(2_000_000_000n);
  });

  it('throws on negative input', () => {
    expect(() => parseSOL('-1')).toThrow(/negative/i);
  });

  it('throws on more than 9 decimal places', () => {
    expect(() => parseSOL('0.0000000001')).toThrow(/precision/i);
  });

  it('throws on non-numeric input', () => {
    expect(() => parseSOL('abc')).toThrow(/invalid/i);
  });
});
