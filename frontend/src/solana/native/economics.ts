// Pure display economics for the native Solana panels. No web3.js, no React.
//
// The native program accrues Simple interest on the GROSS deposit: the 4 %
// Front-End Load is skimmed to Management at open, but interest still runs on
// the full amount the player put in (docs/solana/economics.md; golden vector
// solana/fixtures/economics.json `simple_one_day`: 1 SOL -> 110,000,000
// lamports a day). The legacy ponzi_math_sol panel projects on the NET deposit
// and understates the figure by about 4 %; the native adapter must not.

import {
  BACKER_EQUAL_B_SHARE,
  BACKER_PROPORTIONAL_A_SHARE,
  BACKER_SENIOR_SHARE,
  DAILY_RATE_SIMPLE,
  pct,
} from '../../lib/gameConstants';

export const BPS_DENOMINATOR = 10_000n;

/** Simple plan daily rate in basis points (11 % -> 1,100), derived from the shared constant. */
export const SIMPLE_DAILY_RATE_BPS = BigInt(Math.round(DAILY_RATE_SIMPLE * 10_000));

/** Lamports of Simple interest per day on `grossLamports`: floor(gross x 1,100 / 10,000). */
export function projectSimpleDailyLamports(grossLamports: bigint): bigint {
  if (grossLamports < 0n) throw new RangeError('projectSimpleDailyLamports: gross lamports cannot be negative');
  return (grossLamports * SIMPLE_DAILY_RATE_BPS) / BPS_DENOMINATOR;
}

/**
 * One-line description of how the backer half of every toll is split:
 * senior (oldest open Series A), pro rata across Series A, equal across Series B.
 */
export function backerSplitText(): string {
  return (
    `${pct(BACKER_SENIOR_SHARE)} senior (oldest Series A) · ` +
    `${pct(BACKER_PROPORTIONAL_A_SHARE)} pro rata across Series A · ` +
    `${pct(BACKER_EQUAL_B_SHARE)} equal across Series B`
  );
}
