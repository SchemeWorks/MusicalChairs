import { describe, it, expect } from 'vitest';
import { isValidSolanaAddress } from './base58';

describe('isValidSolanaAddress', () => {
  it('accepts a canonical 44-char base58 address', () => {
    expect(isValidSolanaAddress('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM')).toBe(true);
  });

  it('accepts a 43-char base58 address', () => {
    expect(isValidSolanaAddress('5pcgZcakK3PUmaMTzKw6oZbN8wjcaB7AcEh3yJfHCqi')).toBe(true);
  });

  it('rejects an empty string', () => {
    expect(isValidSolanaAddress('')).toBe(false);
  });

  it('rejects strings under 32 chars', () => {
    expect(isValidSolanaAddress('shortAddr123')).toBe(false);
  });

  it('rejects strings over 44 chars', () => {
    expect(isValidSolanaAddress('9'.repeat(45))).toBe(false);
  });

  it('rejects strings containing base58-illegal chars (0, O, I, l)', () => {
    expect(isValidSolanaAddress('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWW0')).toBe(false);
    expect(isValidSolanaAddress('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWO')).toBe(false);
    expect(isValidSolanaAddress('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWI')).toBe(false);
    expect(isValidSolanaAddress('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWl')).toBe(false);
  });

  it('rejects ICP principals (which contain dashes)', () => {
    expect(isValidSolanaAddress('5zxxg-tyaaa-aaaac-qeckq-cai')).toBe(false);
  });
});
