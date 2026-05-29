// Solana addresses are 32-byte Ed25519 public keys encoded in base58. The
// encoded form is 32-44 characters (length varies with leading zeros). We do
// a cheap character-set + length check — no curve check. Off-curve / PDA
// rejection is V2 follow-up.
const BASE58_ALPHABET = /^[123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]+$/;

export function isValidSolanaAddress(s: string): boolean {
  if (typeof s !== 'string') return false;
  if (s.length < 32 || s.length > 44) return false;
  return BASE58_ALPHABET.test(s);
}
