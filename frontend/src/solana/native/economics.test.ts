// IMPL-46b step 1, tests 2 and 3: the native adapter projects Simple interest
// on GROSS (not net of the Front-End Load) and shows the 35/45/20 backer split.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  projectSimpleDailyLamports,
  backerSplitText,
  SIMPLE_DAILY_RATE_BPS,
} from './economics';
import {
  BACKER_SENIOR_SHARE,
  BACKER_PROPORTIONAL_A_SHARE,
  BACKER_EQUAL_B_SHARE,
  COVER_CHARGE_RATE,
  DAILY_RATE_SIMPLE,
} from '../../lib/gameConstants';

const REPO_ROOT = new URL('../../../../', import.meta.url);

describe('native Simple projection', () => {
  it('simpleProjectionUsesGrossNotNet', () => {
    const oneSol = 1_000_000_000n;
    // floor(gross x 1,100 / 10,000): the program accrues Simple interest on the GROSS deposit.
    expect(projectSimpleDailyLamports(oneSol)).toBe(110_000_000n);
    // Today's legacy panel projects on the NET deposit (gross x (1 - 4 % Front-End Load) x 11 %).
    const legacyNetBased = BigInt(Math.round(1_000_000_000 * (1 - COVER_CHARGE_RATE) * DAILY_RATE_SIMPLE));
    expect(legacyNetBased).toBe(105_600_000n);
    expect(projectSimpleDailyLamports(oneSol)).not.toBe(legacyNetBased);
  });

  it('floors, handles dust and rejects negatives', () => {
    expect(SIMPLE_DAILY_RATE_BPS).toBe(1_100n);
    expect(projectSimpleDailyLamports(0n)).toBe(0n);
    expect(projectSimpleDailyLamports(9n)).toBe(0n); // 9 x 1,100 / 10,000 = 0.99
    expect(projectSimpleDailyLamports(10n)).toBe(1n);
    expect(projectSimpleDailyLamports(10_000_000n)).toBe(1_100_000n); // 0.01 SOL minimum deposit
    expect(() => projectSimpleDailyLamports(-1n)).toThrow(/negative/i);
  });

  it("agrees with the program's golden vector (solana/fixtures/economics.json simple_one_day)", () => {
    const vectors = JSON.parse(readFileSync(new URL('solana/fixtures/economics.json', REPO_ROOT), 'utf8')) as Record<
      string,
      { principal: number; elapsed_seconds: number; owed: number }
    >;
    expect(projectSimpleDailyLamports(BigInt(vectors.simple_one_day.principal))).toBe(BigInt(vectors.simple_one_day.owed));
  });
});

describe('backer split text', () => {
  it('splitTextShows35_45_20', () => {
    expect(BACKER_SENIOR_SHARE).toBe(0.35);
    expect(BACKER_PROPORTIONAL_A_SHARE).toBe(0.45);
    expect(BACKER_EQUAL_B_SHARE).toBe(0.2);
    expect(BACKER_SENIOR_SHARE + BACKER_PROPORTIONAL_A_SHARE + BACKER_EQUAL_B_SHARE).toBeCloseTo(1, 12);
    const text = backerSplitText();
    expect(text).toBe('35% senior (oldest Series A) · 45% pro rata across Series A · 20% equal across Series B');
    // The stale 35 / 25 / 40 trio must not survive anywhere in the text.
    expect(text).not.toMatch(/25%|40%/);
  });
});
