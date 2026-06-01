# SOL Position Card Parity — Design Spec

**Date:** 2026-05-31
**Scope:** Frontend-only. No backend / canister / candid change.
**File touched:** `frontend/src/components/GameTracking.tsx` (only).

## Problem

In the Profit Center, ICP positions render through the rich `PositionCard`
component (`GameTracking.tsx` ~126–226): plan name + Compounding/Simple badge,
"Xd active", **live-computed** earnings with a "before carry" / "live" label,
**Carried Interest %** badge (+ countdown to next tier), a **Day X / N progress
bar**, and a Withdraw button with a lock state + maturity timer.

SOL positions render through a separate, minimal inline list (~417–471): just
plan name, Deposited, Accrued, and a Settle/Withdraw button. No days-active, no
progress bar, no carried interest, no lock timer, and it shows the *stored*
`accumulatedEarnings` (≈0 for a fresh position) instead of live-computing
earnings. The two were built at different times and never unified.

## Key enabling fact

`SolGameRecord` and `GameRecord` are **structurally identical** TypeScript
interfaces (verified in `declarations/ponzi_math_sol` vs `declarations/ponzi_math`:
same fields `id, startTime, player, plan, isActive, accumulatedEarnings,
lastUpdateTime, isCompounding, totalWithdrawn, amount`; `GamePlan` variants are
identical too). Because TypeScript is structural, every existing ICP helper that
is typed `(game: GameRecord)` — `getExitTollInfo`, `isCompoundingPlanUnlocked`,
`getTimeRemaining`, `getPositionUrgency`, `getPlanName`, `getPlanAccent`,
`getPlanDuration`, `daysActive` — **accepts a `SolGameRecord` value with no type
error and no change**.

Crucially, `useLivePortfolio(games)` (from `useLiveEarnings.ts`) — the exact
engine that feeds the ICP card its live earnings via `computeLiveEarnings` —
also accepts `SolGameRecord[]` structurally. `computeLiveEarnings` IS "the same
math the ICP card uses" (it calls `getDailyRate`/`getPlanDays` and applies the
simple/compounding formulas with proper plan-duration capping and `lastUpdateTime`
handling — a strict superset of `calculateSimpleROI`/`calculateCompoundingROI`).
It ticks every second, so SOL earnings climb live for free.

The carried-interest schedule is **identical** for SOL and ICP (compounding 9%
@15d / 13% @30d via `JACKPOT_FEE_RATE_15D/30D`; simple 12% / 7.5% / 3% by week
via `EXIT_TOLL_*`), so `getExitTollInfo` needs no SOL variant.

## Approach (chosen): generalize `PositionCard`, route both lists through it

Prevents future drift — one card renders both denominations.

### `PositionCard` prop changes

Keep the existing param type `game: GameRecord` (SOL games satisfy it
structurally). Add optional props, all defaulting to current ICP behaviour:

```ts
denomination?: 'ICP' | 'SOL';        // default 'ICP' — selects formatter + unit label
withdrawDisabled?: boolean;          // extra disable guard (SOL: !solanaPubkey). default false
withdrawDisabledTitle?: string;      // button title when withdrawDisabled
settleLabel?: string;                // label for the unlocked-compounding action. default 'Withdraw'
```

Internal derivations:
- `const fmt = denomination === 'SOL' ? formatSolFloat : formatICP;`
- `const unit = denomination;` ('ICP' | 'SOL')
- Amounts render `{fmt(game.amount)} {unit}` and `{fmt(earnings)} {unit}`.
- Button **enabled** = `canWithdraw && (hasEarnings || isMaturedSimpleClose) && !withdrawDisabled`.
- Button **title**: if `withdrawDisabled` → `withdrawDisabledTitle`; else the existing logic.
- Button **label**: `!canWithdraw` → lock timer (unchanged); else `isMaturedSimpleClose` → "Close"; else `game.isCompounding` → `settleLabel`; else "Withdraw".
  - ICP keeps default `settleLabel='Withdraw'` → compounding unlocked still reads "Withdraw" (unchanged).
  - SOL passes `settleLabel='Settle'` → compounding unlocked reads "Settle", simple reads "Withdraw" (matches existing SOL verbs).

Everything else in the card (badge, days-active, progress bar, carried-interest
badge + countdown, "before carry"/"live" label) is denomination-agnostic and
reused as-is.

### ICP call site (`Your Positions`)
Unchanged behaviour — pass no new props (defaults preserve today's output).

### SOL call site (`Your SOL Positions`)
Replace the inline `solGames.map(...)` block (~419–471) with the shared card:

1. Add `const solPortfolio = useLivePortfolio(solGames);` near the existing
   `useLivePortfolio(games)` call (hook called unconditionally; returns empty
   when no SOL games).
2. Render, sorted by the same urgency comparator as ICP:
   ```tsx
   [...solPortfolio.games]
     .sort((a, b) => getPositionUrgency(a.game) - getPositionUrgency(b.game))
     .map(({ game, earnings }) => (
       <PositionCard
         key={game.id.toString()}
         game={game}
         earnings={earnings}
         denomination="SOL"
         settleLabel="Settle"
         withdrawDisabled={!solanaPubkey}
         withdrawDisabledTitle="Reconnect your Solana wallet to withdraw"
         onWithdraw={(g) => handleSolWithdraw(g as SolGameRecord)}
         withdrawPending={solWithdrawMutation.isPending && solWithdrawMutation.variables?.gameId === game.id}
       />
     ))
   ```
3. Keep the section wrapper, `"Your SOL Positions"` header, the error block, and
   the "Withdrawals are sent to your connected Phantom wallet." footnote.
4. `handleSolWithdraw` is unchanged (still calls `useWithdrawSolGameEarnings`
   with `{ gameId, isCompounding }`; the `!solanaPubkey` guard in the mutation
   stays, now mirrored visually by `withdrawDisabled`).

### Imports
- Add `formatSolFloat` (already in `solana/lamports`); add `getPositionUrgency`
  is local (no import). Add `useLivePortfolio` is already imported.
- Remove the now-unused inline `toLamports` helper and the `formatSOL` import if
  no longer referenced after the inline block is deleted.

## Why not a SOL-specific clone of PositionCard?
The fallback in the brief. Rejected because the two records are structurally
identical and all helpers already accept both — a clone would immediately
re-introduce the drift this task exists to remove.

## Testing
No new pure helpers are introduced (earnings come from the existing
`computeLiveEarnings`/`useLivePortfolio`; carried interest from existing
`getExitTollInfo`). Per the guardrail ("no component-test harness; unit-test only
pure helpers") there is nothing new to unit-test. Verification is:
- `npx tsc --noEmit` (root) — clean.
- `npm test` (vitest, root) — existing suite stays green.
- `npm run build` (root) — succeeds.

## Out of scope
Backend, candid, the ICP card's behaviour, the withdrawal dialog, PP section,
fee-disclosure copy.
