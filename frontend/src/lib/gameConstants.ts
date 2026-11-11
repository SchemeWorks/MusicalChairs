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
export const MIN_DEPOSIT_SOL = 0.01;   // SOL plan-deposit floor (matches ponzi_math_sol MIN_DEPOSIT_LAMPORTS = 10_000_000)
export const SIMPLE_MAX_DEPOSIT_POT_FRACTION = 0.2;   // 20 % of AUM
export const SIMPLE_MAX_DEPOSIT_FLOOR_ICP = 5;         // or 5 ICP, whichever is greater
export const DEPOSIT_RATE_LIMIT = 3;                   // positions per hour

// ── Entry Fees ───────────────────────────────────────────────────────
export const COVER_CHARGE_RATE = 0.04;          //  4 %  (routes 100% to Management)

// ── Exit Tolls — Simple ─────────────────────────────────────────────
export const EXIT_TOLL_EARLY = 0.12;            // 12 %  (week 1: day 0–7)
export const EXIT_TOLL_MID = 0.075;             //  7.5% (week 2: day 7–14)
export const EXIT_TOLL_LATE = 0.03;             //  3 %  (week 3: day 14+)
export const EXIT_TOLL_EARLY_DAYS = 7;
export const EXIT_TOLL_MID_DAYS = 14;

// ── Exit Tolls — Compounding ────────────────────────────────────────
export const JACKPOT_FEE_RATE_15D = 0.09;       //  9 % (15-day plan)
export const JACKPOT_FEE_RATE_30D = 0.13;       // 13 % (30-day plan)

// ── Fee Distribution ────────────────────────────────────────────────
export const FEE_POT_SHARE = 0.5;               // 50 % to AUM
export const FEE_BACKER_SHARE = 0.5;            // 50 % to backers
// Backer-half split (native Solana program, docs/solana/backer-allocation.md): after the
// 50/50 fee split, the backer half of every Carried Interest toll goes 35 % to the oldest
// open Series A backer (senior), 45 % pro rata across open Series A backers by remaining
// entitlement, and 20 % split equally across open Series B backers.
export const BACKER_SENIOR_SHARE = 0.35;            // 35 % of backer half (oldest Series A)
export const BACKER_PROPORTIONAL_A_SHARE = 0.45;    // 45 % of backer half (pro rata across Series A)
export const BACKER_EQUAL_B_SHARE = 0.20;           // 20 % of backer half (equal across Series B)

// ── Backer (Seed Round) Bonuses ─────────────────────────────────────
export const UPSTREAM_BACKER_BONUS = 0.24;      // 24 % (Series A)
export const DOWNSTREAM_BACKER_BONUS = 0.16;   // 16 % (Series B)

// ── Ponzi Points per ICP ────────────────────────────────────────────
export const PP_PER_ICP_SIMPLE = 1_000;
export const PP_PER_ICP_COMPOUND_15 = 2_000;
export const PP_PER_ICP_COMPOUND_30 = 3_000;
export const PP_PER_ICP_SEED_ROUND = 4_000;

// SOL-side PP rates — values match the live ponzi_math_sol MintConfig at deploy
// time (2026-05-28). Admin-tunable post-deploy via `shenanigans.setMintConfig`,
// so these constants are display-only — the actual mint amount is decided by
// the observer against whatever the canister has stored.
//
// Source: `dfx canister --network ic call shenanigans getMintConfig` at plan time.
// If the values drift, update here for display accuracy.
export const PP_PER_SOL_SIMPLE = 6_000;
export const PP_PER_SOL_COMPOUND_15 = 12_000;
export const PP_PER_SOL_COMPOUND_30 = 18_000;

// ── Referral (MLM) — PP only ────────────────────────────────────────
export const REFERRAL_L1_RATE = 0.08;           //  8 % of referred's PP earnings
export const REFERRAL_L2_RATE = 0.05;           //  5 %
export const REFERRAL_L3_RATE = 0.02;           //  2 %

// ── Helpers ─────────────────────────────────────────────────────────

/** Format a decimal rate as a percentage string, e.g. 0.11 → "11%" */
export function pct(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

/** Like pct, but preserves up to 1 decimal place. 0.075 → "7.5%", 0.12 → "12%" */
export function pctPrecise(rate: number): string {
  return `${Math.round(rate * 1000) / 10}%`;
}

/** Format a number with commas, e.g. 4000 → "4,000" */
export function fmt(n: number): string {
  return n.toLocaleString('en-US');
}
