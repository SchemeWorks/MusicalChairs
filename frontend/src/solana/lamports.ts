export const LAMPORTS_PER_SOL = 1_000_000_000n;
const DEFAULT_DISPLAY_DECIMALS = 4;

// 9-decimal lamport formatter. Always emits a fractional component for visual
// parity with formatICP. Sub-display-precision values render at full 9-decimal
// width so dust deposits are still legible.
export function formatSOL(lamports: bigint): string {
  if (lamports < 0n) throw new Error('formatSOL: lamports cannot be negative');
  const whole = lamports / LAMPORTS_PER_SOL;
  const remainder = lamports % LAMPORTS_PER_SOL;

  if (remainder === 0n) {
    return `${whole}.${'0'.repeat(DEFAULT_DISPLAY_DECIMALS)}`;
  }

  const fractional = remainder.toString().padStart(9, '0');
  const trimmed = fractional.replace(/0+$/, '');
  const displayWidth = Math.max(DEFAULT_DISPLAY_DECIMALS, trimmed.length);
  return `${whole}.${fractional.slice(0, displayWidth)}`;
}

// Display a SOL amount supplied as a float (e.g. an ROI projection) by reusing
// formatSOL's lamport formatting. Guards NaN / non-finite / non-positive input.
export function formatSolFloat(value: number): string {
  if (!isFinite(value) || value <= 0) return formatSOL(0n);
  return formatSOL(BigInt(Math.round(value * Number(LAMPORTS_PER_SOL))));
}

// Inverse of formatSOL — input is a decimal SOL string, output is bigint lamports.
export function parseSOL(input: string): bigint {
  const trimmed = input.trim();
  if (trimmed.startsWith('-')) {
    throw new Error('parseSOL: SOL amount cannot be negative');
  }
  if (!/^\d+(\.\d+)?$/.test(trimmed)) {
    throw new Error(`parseSOL: invalid input '${input}'`);
  }
  const [whole, fractional = ''] = trimmed.split('.');
  if (fractional.length > 9) {
    throw new Error(`parseSOL: precision exceeds 9 decimal places: '${input}'`);
  }
  const paddedFractional = fractional.padEnd(9, '0');
  return BigInt(whole) * LAMPORTS_PER_SOL + BigInt(paddedFractional);
}
