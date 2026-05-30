import { describe, it, expect } from 'vitest';
import { tierRateToUnits, unitsToTierRate, formatPpUnits, effectiveRatePer0_1Sol, formatCountdown } from './ppDesk';

describe('ppDesk helpers', () => {
  it('tierRateToUnits: whole PP-per-0.1-SOL → PP units', () => {
    expect(tierRateToUnits(250_000)).toBe(25_000_000_000_000n); // 250k * 1e8
    expect(tierRateToUnits(0)).toBe(0n);
  });
  it('unitsToTierRate is the inverse', () => {
    expect(unitsToTierRate(25_000_000_000_000n)).toBe(250_000);
  });
  it('formatPpUnits renders whole PP with thousands separators', () => {
    expect(formatPpUnits(25_000_000_000_000n)).toBe('250,000');
    expect(formatPpUnits(0n)).toBe('0');
  });
  it('effectiveRatePer0_1Sol = ppUnitsOut / lamports', () => {
    expect(effectiveRatePer0_1Sol(25_000_000_000_000n, 100_000_000n)).toBe('250,000');
    expect(effectiveRatePer0_1Sol(0n, 0n)).toBe('—');
  });
  it('formatCountdown formats remaining ms (and "expired")', () => {
    expect(formatCountdown(125_000)).toBe('2m 5s');
    expect(formatCountdown(0)).toBe('expired');
    expect(formatCountdown(-5)).toBe('expired');
  });
});
