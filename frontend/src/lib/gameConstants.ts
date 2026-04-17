/**
 * Shared game constants — single source of truth for all game figures.
 *
 * These mirror the values hard-coded in `backend/main.mo`. If the backend
 * changes, update here and every UI / docs reference picks it up automatically.
 */

// ── Daily Rates ──────────────────────────────────────────────────────
export const DAILY_RATE_SIMPLE = 0.11;          // 11 %
export const DAILY_RATE_COMPOUND_15 = 0.12;     // 12 %
export const DAILY_RATE_COMPOUND_30 = 0.09;     //  9 %

// ── Plan Durations (days) ────────────────────────────────────────────
export const PLAN_DAYS_SIMPLE = 21;
export const PLAN_DAYS_COMPOUND_15 = 15;
export const PLAN_DAYS_COMPOUND_30 = 30;

// ── Deposit Limits ───────────────────────────────────────────────────
export const MIN_DEPOSIT_ICP = 0.1;
export const SIMPLE_MAX_DEPOSIT_POT_FRACTION = 0.2;   // 20 % of pot
export const SIMPLE_MAX_DEPOSIT_FLOOR_ICP = 5;         // or 5 ICP, whichever is greater
export const DEPOSIT_RATE_LIMIT = 3;                   // positions per hour

// ── Entry Fees ───────────────────────────────────────────────────────
export const COVER_CHARGE_RATE = 0.02;          //  2 %  (routes 100% to Management)

// ── Exit Tolls — Simple ─────────────────────────────────────────────
export const EXIT_TOLL_EARLY = 0.07;            //  7 % (day 0–3)
export const EXIT_TOLL_MID = 0.05;              //  5 % (day 3–10)
export const EXIT_TOLL_LATE = 0.03;             //  3 % (day 10+)
export const EXIT_TOLL_EARLY_DAYS = 3;
export const EXIT_TOLL_MID_DAYS = 10;

// ── Exit Tolls — Compounding ────────────────────────────────────────
export const JACKPOT_FEE_RATE = 0.13;           // 13 %

// ── Fee Distribution ────────────────────────────────────────────────
export const FEE_POT_SHARE = 0.5;               // 50 % to pot
export const FEE_BACKER_SHARE = 0.5;            // 50 % to backers
export const BACKER_OLDEST_UPSTREAM_SHARE = 0.35;   // 35 % of backer half
export const BACKER_OTHER_UPSTREAM_SHARE = 0.25;    // 25 % of backer half
export const BACKER_ALL_SHARE = 0.40;               // 40 % of backer half

// ── Backer (Seed Round) Bonuses ─────────────────────────────────────
export const UPSTREAM_BACKER_BONUS = 0.24;      // 24 % (Series A)
export const DOWNSTREAM_BACKER_BONUS = 0.16;   // 16 % (Series B)

// ── Ponzi Points per ICP ────────────────────────────────────────────
export const PP_PER_ICP_SIMPLE = 1_000;
export const PP_PER_ICP_COMPOUND_15 = 2_000;
export const PP_PER_ICP_COMPOUND_30 = 3_000;
export const PP_PER_ICP_SEED_ROUND = 4_000;

// ── Referral (MLM) — PP only ────────────────────────────────────────
export const REFERRAL_L1_RATE = 0.08;           //  8 % of referred's PP earnings
export const REFERRAL_L2_RATE = 0.05;           //  5 %
export const REFERRAL_L3_RATE = 0.02;           //  2 %

// ── Shenanigan Protection Floor ─────────────────────────────────────
export const SHENANIGAN_PROTECTION_FLOOR = 200; // PP

// ── Helpers ─────────────────────────────────────────────────────────

/** Format a decimal rate as a percentage string, e.g. 0.11 → "11%" */
export function pct(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

/** Format a number with commas, e.g. 4000 → "4,000" */
export function fmt(n: number): string {
  return n.toLocaleString('en-US');
}
