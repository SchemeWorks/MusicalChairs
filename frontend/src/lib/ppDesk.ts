/** Pure helpers for the Founder's Allocation desk UI. PP has 8 decimals;
 *  0.1 SOL = 1e8 lamports, so "PP units per 0.1 SOL" and the lamport scale
 *  share 1e8 — see effectiveRatePer0_1Sol. */
export const PP_UNIT_SCALE = 100_000_000n;

/** Whole PP-per-0.1-SOL (what Charles types) → ratePpUnitsPer0_1Sol (backend). */
export function tierRateToUnits(wholePpPer0_1Sol: number): bigint {
  if (!Number.isFinite(wholePpPer0_1Sol) || wholePpPer0_1Sol <= 0) return 0n;
  return BigInt(Math.trunc(wholePpPer0_1Sol)) * PP_UNIT_SCALE;
}

/** Inverse of tierRateToUnits, for display. */
export function unitsToTierRate(ratePpUnitsPer0_1Sol: bigint): number {
  return Number(ratePpUnitsPer0_1Sol / PP_UNIT_SCALE);
}

/** PP units → whole-PP display string with thousands separators. */
export function formatPpUnits(units: bigint): string {
  return (Number(units) / 1e8).toLocaleString(undefined, { maximumFractionDigits: 2 });
}

/** Effective whole-PP per 0.1 SOL for a quote = ppUnitsOut / lamports. */
export function effectiveRatePer0_1Sol(ppUnitsOut: bigint, lamports: bigint): string {
  if (lamports <= 0n || ppUnitsOut <= 0n) return '—';
  return (Number(ppUnitsOut) / Number(lamports)).toLocaleString(undefined, { maximumFractionDigits: 0 });
}

/** Format a remaining duration (ms) as "Xm Ys", or "expired" at/under zero. */
export function formatCountdown(msRemaining: number): string {
  if (msRemaining <= 0) return 'expired';
  const totalSec = Math.floor(msRemaining / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}m ${s}s`;
}
