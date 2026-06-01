# SOL Position Card Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render SOL positions in the Profit Center through the same rich `PositionCard` the ICP positions use, achieving full visual + functional parity (live earnings, days-active, Day X/N progress bar, Carried Interest %, lock/maturity timer).

**Architecture:** `SolGameRecord` and `GameRecord` are structurally identical TS interfaces, so all existing ICP helpers and `useLivePortfolio` already accept SOL games. Generalize `PositionCard` with a few optional denomination/withdraw props (defaulting to current ICP behaviour), then route the SOL list through it via `useLivePortfolio(solGames)`.

**Tech Stack:** React + TypeScript, Vite, vitest. All commands run from repo ROOT. Single file changed: `frontend/src/components/GameTracking.tsx`.

---

## Pre-flight

- Branch: `feat/sol-position-card-parity` (already created off `fix/sol-deposit-confirm`, which carries `formatSolFloat`).
- **Stage ONLY `frontend/src/components/GameTracking.tsx`** in commits. The working tree has unrelated user WIP (`CLAUDE.md`, `README.md`, `scripts/`, `spec.md`, untracked `docs/*.md`, `.png`) — never `git add -A`.
- No new pure helpers are introduced, so there are no new unit tests (per guardrail: no component-test harness, unit-test only pure helpers). Verification is tsc + existing vitest suite + build.

---

### Task 1: Generalize `PositionCard` for denomination + SOL withdraw semantics

**Files:**
- Modify: `frontend/src/components/GameTracking.tsx` (`PositionCard`, ~123–226) and its imports (~5–24).

This task ONLY changes `PositionCard` + imports. The ICP call site must keep
identical behaviour because every new prop defaults to today's values. The SOL
call site is wired in Task 2.

- [ ] **Step 1: Add the `formatSolFloat` import.**

In the import from `../solana/lamports` (currently `import { formatSOL } from '../solana/lamports';`), add `formatSolFloat`:

```ts
import { formatSOL, formatSolFloat } from '../solana/lamports';
```

(Leave `formatSOL` for now — Task 2 deletes its last use and removes it.)

- [ ] **Step 2: Extend the `PositionCard` prop type.**

Replace the prop destructuring + type (the `function PositionCard({ ... }: { ... })` block, ~123–133) with:

```tsx
function PositionCard({
  game,
  earnings,
  onWithdraw,
  withdrawPending,
  denomination = 'ICP',
  withdrawDisabled = false,
  withdrawDisabledTitle,
  settleLabel = 'Withdraw',
}: {
  game: GameRecord;
  earnings: number;
  onWithdraw: (game: GameRecord) => void;
  withdrawPending: boolean;
  denomination?: 'ICP' | 'SOL';
  withdrawDisabled?: boolean;
  withdrawDisabledTitle?: string;
  settleLabel?: string;
}) {
```

- [ ] **Step 3: Derive the formatter, unit, and fold `withdrawDisabled` into `buttonEnabled`.**

Immediately after the existing `const tollInfo = getExitTollInfo(game);` line (~143), add:

```tsx
  const fmt = denomination === 'SOL' ? formatSolFloat : formatICP;
  const unit = denomination;
```

Then change the `buttonEnabled` line (~141) from:

```tsx
  const buttonEnabled = canWithdraw && (hasEarnings || isMaturedSimpleClose);
```

to:

```tsx
  const buttonEnabled = canWithdraw && (hasEarnings || isMaturedSimpleClose) && !withdrawDisabled;
```

- [ ] **Step 4: Use `fmt`/`unit` for the deposit and earnings amounts.**

In the numbers row, change the deposit line (~164) from:

```tsx
          <div className="text-base font-bold mc-text-primary">{formatICP(game.amount)} ICP</div>
```

to:

```tsx
          <div className="text-base font-bold mc-text-primary">{fmt(game.amount)} {unit}</div>
```

and the earnings line (~169–171) from:

```tsx
          <div className="text-lg sm:text-xl font-bold mc-text-green mc-glow-green">
            {formatICP(earnings)} ICP
          </div>
```

to:

```tsx
          <div className="text-lg sm:text-xl font-bold mc-text-green mc-glow-green">
            {fmt(earnings)} {unit}
          </div>
```

- [ ] **Step 5: Apply `withdrawDisabledTitle` and `settleLabel` to the withdraw button.**

Change the button `title` prop (~213) from:

```tsx
          title={!canWithdraw ? 'Locked until maturity' : isMaturedSimpleClose ? 'Close matured position' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
```

to:

```tsx
          title={withdrawDisabled ? withdrawDisabledTitle : !canWithdraw ? 'Locked until maturity' : isMaturedSimpleClose ? 'Close matured position' : !hasEarnings ? 'No earnings yet' : 'Withdraw'}
```

Change the button label block (~215–221) from:

```tsx
          {!canWithdraw ? (
            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h {timeRem.minutes}m</span>
          ) : isMaturedSimpleClose ? (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Close</span>
          ) : (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Withdraw</span>
          )}
```

to:

```tsx
          {!canWithdraw ? (
            <span className="flex items-center gap-1"><Lock className="h-3 w-3" />{timeRem.days}d {timeRem.hours}h {timeRem.minutes}m</span>
          ) : isMaturedSimpleClose ? (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />Close</span>
          ) : (
            <span className="flex items-center gap-1"><ArrowDownCircle className="h-3 w-3" />{game.isCompounding ? settleLabel : 'Withdraw'}</span>
          )}
```

(ICP default `settleLabel='Withdraw'` ⇒ unchanged ICP output. SOL will pass `'Settle'` in Task 2.)

- [ ] **Step 6: Verify types + build still pass (ICP behaviour unchanged, SOL not yet wired).**

Run from repo ROOT:

```bash
npx tsc --noEmit
```

Expected: no errors. (`formatSolFloat` is now imported and used; `formatSOL` is still imported and still used by the inline SOL block, so no unused-import error yet.)

- [ ] **Step 7: Commit.**

```bash
git add frontend/src/components/GameTracking.tsx
git commit -m "feat(sol-parity): generalize PositionCard with denomination + SOL withdraw props

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Route the SOL list through the generalized `PositionCard`

**Files:**
- Modify: `frontend/src/components/GameTracking.tsx` (SOL section ~414–482, the `useLivePortfolio` call site ~270, imports).

- [ ] **Step 1: Add a live portfolio for SOL games.**

Right after the existing `const portfolio = useLivePortfolio(games);` line (~270), add:

```tsx
  const solPortfolio = useLivePortfolio(solGames);
```

(`useLivePortfolio` is already imported. It accepts `solGames` structurally and returns `{ games: [], ... }` when empty, so calling it unconditionally is safe.)

- [ ] **Step 2: Replace the inline SOL `.map` with the shared card.**

Replace the entire inner block of the SOL section — from `<div className="space-y-3">` through its closing `</div>` that wraps `solGames.map(...)` (the block spanning ~418–472, i.e. the `<div className="space-y-3">{solGames.map((game) => { ... })}</div>`) — with:

```tsx
            <div className="space-y-3">
              {[...solPortfolio.games]
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
                    withdrawPending={
                      solWithdrawMutation.isPending &&
                      solWithdrawMutation.variables?.gameId === game.id
                    }
                  />
                ))}
            </div>
```

Leave the surrounding section wrapper intact: the `{walletType === 'siws' && solGames.length > 0 && (` guard, the `<h2>Your SOL Positions</h2>` header, the `solWithdrawMutation.isError` error block, and the "Withdrawals are sent to your connected Phantom wallet." footnote all stay exactly as they are.

- [ ] **Step 3: Remove the now-unused `formatSOL` import.**

The inline block was the only consumer of `formatSOL` and the local `toLamports` helper (both deleted in Step 2). Change the lamports import back to only what is used:

```ts
import { formatSolFloat } from '../solana/lamports';
```

(If `tsc` in Step 4 reports `formatSOL` is still referenced somewhere, restore it instead — but the inline block was its only use in this file.)

- [ ] **Step 4: Verify types.**

Run from repo ROOT:

```bash
npx tsc --noEmit
```

Expected: no errors. In particular: no "unused `formatSOL`" (removed), no "unused `toLamports`" (deleted), `solanaPubkey` is already destructured from `useWallet()` (~261), `SolGameRecord` is already imported (~6), `getPositionUrgency`/`handleSolWithdraw` are local.

- [ ] **Step 5: Run the existing test suite.**

Run from repo ROOT:

```bash
npm test
```

Expected: PASS (all existing vitest tests green — no tests touch `GameTracking`).

- [ ] **Step 6: Production build.**

Run from repo ROOT:

```bash
npm run build
```

Expected: build succeeds with no type/bundup errors.

- [ ] **Step 7: Commit.**

```bash
git add frontend/src/components/GameTracking.tsx
git commit -m "feat(sol-parity): route SOL positions through shared PositionCard

SOL positions now render with live-computed earnings, days-active, Day X/N
progress bar, Carried Interest %, and lock/maturity timer — full parity with
ICP cards. Earnings come from useLivePortfolio(solGames) (same computeLiveEarnings
math as ICP); amounts denominated in SOL via formatSolFloat. Settle/Withdraw
preserved with the !solanaPubkey disabled guard.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final verification (after both tasks)

From repo ROOT, all must pass:

```bash
npx tsc --noEmit
npm test
npm run build
```

Then confirm only the intended file is staged across the two commits:

```bash
git diff --stat fix/sol-deposit-confirm..HEAD
```

Expected: only `frontend/src/components/GameTracking.tsx` (plus the spec/plan docs committed separately). No `CLAUDE.md`, `README.md`, `scripts/`, `spec.md`, `.png`.
