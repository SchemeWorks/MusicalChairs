# Exit Liquidity — Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the player-facing Exit Liquidity game — the turn-based bank/ride/exit play loop, the silent volatility tell, and the per-Round clout leaderboard — on top of the backend engine.

**Architecture:** A self-contained `exit-liquidity/` component set driven by react-query hooks against the `shenanigans` canister. The canister is authoritative; the UI reads the in-flight run from `getActiveExitRun` and submits discrete `exitRunDecision` calls. Mounted as a new dashboard tab.

**Tech Stack:** React 18, `@tanstack/react-query`, `@dfinity/agent` actors (existing `useShenaniganActor`/`useReadShenaniganActor`), Tailwind (`mc-*` design classes), `lucide-react`, `sonner` toasts, `vitest` for pure logic.

**Depends on:** `2026-05-29-exit-liquidity-backend.md` implemented and declarations regenerated (its Task 5), so `startExitRun`, `exitRunDecision`, `getActiveExitRun`, `getExitLiquidityLeaderboard`, `getExitRunCount`, `getExitBiggestRun`, `getExitLiquidityConfig` and the `ExitRun`/`ExitDecision`/`ExitRunResult`/`ExitLiquidityConfig` types exist in `frontend/src/declarations/shenanigans/`.

**Testing reality:** `vitest` is configured (`npm run test`), but there is **no `@testing-library/react`** — so component/hook tests aren't set up. Real TDD applies to the **pure helpers** (Task 1); components and hooks are verified via the running app (Task 8, `npm run dev`). This matches the repo's existing split.

---

## Conventions to follow (verified against the live code)

- **Actors:** queries use `useReadShenaniganActor()` (anonymous — Oisy's signer would otherwise upgrade queries to update calls and open a popup); updates use `useShenaniganActor()` → `{ actor }`. (`useShenaniganActor.ts`)
- **Mutation shape:** mirror `useCastShenanigan` (`useQueries.ts:709`) — `if (!actor) throw`, call the method, `onSuccess` invalidates query keys. PP balance key is `['ponziPointsBalance']`.
- **Query shape + candid optionals:** mirror `useGetRoundBurnedLeaderboard` (`useQueries.ts:1407`) — `?Nat` arg becomes `[] | [bigint]`, counts are `BigInt(n)`, returns `[Principal, bigint][]`. Candid `?T` results unwrap as `res[0] ?? null`.
- **Current round:** `useGetCurrentRoundId()` (`useQueries.ts:1442`) — pass the live round id explicitly so the board doesn't lag on the canister's 30s cache.
- **Errors:** `prettifyCanisterError(err, ...)` + `ErrorKind` from `../lib/errorMessages`, surfaced via `toast` (`sonner`). (pattern in `Shenanigans.tsx`)
- **Names:** `GoldenNameByPrincipal` from `./GoldenName` renders display name + paid-gold treatment.
- **Styling:** `mc-card-elevated`, `mc-text-primary`/`-dim`/`-gold`/`-green`/`-danger`, `font-display`; icons from `lucide-react`; confetti via `triggerConfetti` from `./ConfettiCanvas`.
- **Tabs:** `TabType` union in `App.tsx:30`; desktop `headerNavItems` (`App.tsx:32`); mobile `navItems` + `sectionLabels` + `sectionSubtitles` + `renderContent` switch in `Dashboard.tsx`.

---

## Brand copy (use these exact user-facing strings)

| UI element | String |
|-----------|--------|
| Tab / title | **Exit Liquidity** |
| Start button | **Commit Capital — {n} PP** |
| Bank decision | **Take Distribution** |
| Ride decision | **Let It Ride** |
| Exit decision | **Cash Out** |
| Rotation outcome | **The music stopped. You were the exit liquidity.** |
| Clean-exit outcome | **Clean exit.** |
| Leaderboard | **The Cap Table** |
| Locked score | **Banked (safe)** · at-risk score **Riding** |

Never render a rotation probability anywhere (the inviolable balance law from the spec). The tell is qualitative only.

---

## File structure

- **Create:** `frontend/src/lib/exitLiquidity.ts` — pure formatters/tell logic.
- **Create:** `frontend/src/lib/exitLiquidity.test.ts` — vitest.
- **Modify:** `frontend/src/hooks/useQueries.ts` — 7 hooks (append at end).
- **Create:** `frontend/src/components/exit-liquidity/ExitLiquidityGame.tsx` — play loop.
- **Create:** `frontend/src/components/exit-liquidity/ExitLiquidityLeaderboard.tsx` — the cap table.
- **Create:** `frontend/src/components/ExitLiquidity.tsx` — wrapper composing game + board.
- **Modify:** `frontend/src/App.tsx` — `TabType`, `headerNavItems`.
- **Modify:** `frontend/src/components/Dashboard.tsx` — `navItems`, `sectionLabels`, `sectionSubtitles`, `renderContent`.
- **Modify:** `frontend/src/components/DocsPage.tsx` — short docs blurb.

**Placement decision (resolves the spec's open golden-name question):** Exit Liquidity gets its **own tab**. The champion (#1 on the cap table) is marked with a distinct **`Crown` + `mc-text-gold` badge**, NOT the paid `goldenName` gild — so the free clout title never undercuts the paid spell. A player can be both paid-gold (name gilded by `GoldenNameByPrincipal`) and champion (crown) independently.

---

## Task 1: Pure helpers (TDD with vitest)

**Files:**
- Create: `frontend/src/lib/exitLiquidity.ts`
- Test: `frontend/src/lib/exitLiquidity.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// frontend/src/lib/exitLiquidity.test.ts
import { describe, it, expect } from 'vitest';
import { bpsToMultiplier, tellForStage, nextStageRewardBps, TELL_TIERS } from './exitLiquidity';

describe('bpsToMultiplier', () => {
  it('formats bps as an x-multiplier', () => {
    expect(bpsToMultiplier(10000)).toBe('1.00×');
    expect(bpsToMultiplier(16000)).toBe('1.60×');
    expect(bpsToMultiplier(0)).toBe('0.00×');
  });
  it('accepts bigint', () => {
    expect(bpsToMultiplier(25600n)).toBe('2.56×');
  });
});

describe('tellForStage', () => {
  it('ramps from calm to critical across the stages', () => {
    expect(tellForStage(1, 5)).toBe('Calm');
    expect(tellForStage(5, 5)).toBe('Critical');
  });
  it('clamps and handles a single-stage config', () => {
    expect(tellForStage(9, 5)).toBe('Critical');
    expect(tellForStage(1, 1)).toBe(TELL_TIERS[TELL_TIERS.length - 1]);
  });
});

describe('nextStageRewardBps', () => {
  it('projects riding growth for the reward preview', () => {
    expect(nextStageRewardBps(10000, 16000)).toBe(16000);
    expect(nextStageRewardBps(16000, 16000)).toBe(25600);
  });
});
```

- [ ] **Step 2: Run it, verify it fails**

Run: `npm run test -- exitLiquidity`
Expected: FAIL — `Cannot find module './exitLiquidity'`.

- [ ] **Step 3: Implement**

```ts
// frontend/src/lib/exitLiquidity.ts
// Pure display helpers for the Exit Liquidity game. No canister/odds logic
// lives here — the rotation hazard is server-side and never surfaced.

/** Format an integer-bps multiplier (10000 = 1.0x) as "1.60×". */
export function bpsToMultiplier(bps: bigint | number): string {
  const n = typeof bps === 'bigint' ? Number(bps) : bps;
  return `${(n / 10000).toFixed(2)}×`;
}

/**
 * Qualitative volatility tell. Coarse on purpose: it is the readable signal,
 * NOT the probability. Maps the current stage onto an escalating tier.
 */
export const TELL_TIERS = ['Calm', 'Firm', 'Choppy', 'Toppy', 'Critical'] as const;
export type TellTier = (typeof TELL_TIERS)[number];

export function tellForStage(stage: number, stageCount: number): TellTier {
  const last = TELL_TIERS.length - 1;
  if (stageCount <= 1) return TELL_TIERS[last];
  const frac = (stage - 1) / (stageCount - 1);
  const idx = Math.max(0, Math.min(last, Math.round(frac * last)));
  return TELL_TIERS[idx];
}

/** Project riding growth for the "if you survive" reward preview (reward is safe to show). */
export function nextStageRewardBps(ridingBps: number, stageStepBps: number): number {
  return Math.floor((ridingBps * stageStepBps) / 10000);
}
```

- [ ] **Step 4: Run, verify pass**

Run: `npm run test -- exitLiquidity`
Expected: PASS (3 suites).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/exitLiquidity.ts frontend/src/lib/exitLiquidity.test.ts
git commit -m "feat(frontend): Exit Liquidity pure helpers + tests"
```

---

## Task 2: react-query hooks

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts` (append at end)

- [ ] **Step 1: Add the type import** (near the other declaration imports at the top, e.g. with the `ChatItem` import at `:24`)

```ts
import type { ExitRun, ExitDecision, ExitRunResult, ExitLiquidityConfig } from '../declarations/shenanigans/shenanigans.did';
```

- [ ] **Step 2: Append the hooks**

```ts
// ===== Exit Liquidity =====

export function useGetExitLiquidityConfig() {
  const actor = useReadShenaniganActor();
  return useQuery<ExitLiquidityConfig>({
    queryKey: ['exitLiquidity', 'config'],
    queryFn: async () => actor.getExitLiquidityConfig(),
    enabled: !!actor,
    staleTime: 5 * 60_000,
  });
}

export function useGetActiveExitRun(principal?: string) {
  const actor = useReadShenaniganActor();
  return useQuery<ExitRun | null>({
    queryKey: ['exitLiquidity', 'activeRun', principal],
    queryFn: async () => {
      if (!principal) return null;
      const res = await actor.getActiveExitRun(Principal.fromText(principal));
      return res[0] ?? null;
    },
    enabled: !!actor && !!principal,
  });
}

export function useGetExitRunCount(principal?: string, roundId?: number) {
  const actor = useReadShenaniganActor();
  return useQuery<bigint>({
    queryKey: ['exitLiquidity', 'runCount', principal, roundId],
    queryFn: async () => {
      if (!principal) return 0n;
      const arg: [] | [bigint] = roundId !== undefined ? [BigInt(roundId)] : [];
      return actor.getExitRunCount(Principal.fromText(principal), arg);
    },
    enabled: !!actor && !!principal,
    staleTime: 10_000,
  });
}

export function useGetExitBiggestRun(principal?: string) {
  const actor = useReadShenaniganActor();
  return useQuery<bigint>({
    queryKey: ['exitLiquidity', 'biggestRun', principal],
    queryFn: async () => {
      if (!principal) return 0n;
      return actor.getExitBiggestRun(Principal.fromText(principal));
    },
    enabled: !!actor && !!principal,
    staleTime: 10_000,
  });
}

export function useGetExitLiquidityLeaderboard(roundId?: number, limit = 25) {
  const actor = useReadShenaniganActor();
  return useQuery<[Principal, bigint][]>({
    queryKey: ['exitLiquidity', 'leaderboard', roundId, limit],
    queryFn: async () => {
      const arg: [] | [bigint] = roundId !== undefined ? [BigInt(roundId)] : [];
      return actor.getExitLiquidityLeaderboard(arg, BigInt(limit));
    },
    enabled: !!actor,
    staleTime: 15_000,
  });
}

export function useStartExitRun() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation<ExitRun, Error>({
    mutationFn: async () => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.startExitRun();
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['exitLiquidity', 'activeRun'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
    },
  });
}

export function useExitRunDecision() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation<ExitRunResult, Error, ExitDecision>({
    mutationFn: async (decision: ExitDecision) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.exitRunDecision(decision);
    },
    onSuccess: () => {
      // Re-read the authoritative in-flight run (the decision result doesn't
      // carry the next riding stack), plus the boards and PP balance.
      queryClient.invalidateQueries({ queryKey: ['exitLiquidity', 'activeRun'] });
      queryClient.invalidateQueries({ queryKey: ['exitLiquidity', 'leaderboard'] });
      queryClient.invalidateQueries({ queryKey: ['exitLiquidity', 'runCount'] });
      queryClient.invalidateQueries({ queryKey: ['exitLiquidity', 'biggestRun'] });
    },
  });
}
```

- [ ] **Step 3: Typecheck**

Run: `npx tsc --noEmit`
Expected: no errors (confirms the declaration types from the backend plan are present and used correctly). If `ExitDecision` is `{ bank: null } | { ride: null } | { exit: null }`, callers in Task 3 build it accordingly.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "feat(frontend): Exit Liquidity react-query hooks"
```

---

## Task 3: `ExitLiquidityGame` — the play loop

**Files:**
- Create: `frontend/src/components/exit-liquidity/ExitLiquidityGame.tsx`

State machine, driven off the server-authoritative `getActiveExitRun`:
- `activeRun === null` and no `lastResult` → **Idle** (Commit Capital).
- `activeRun !== null` → **In-flight** (stage, Banked/Riding, tell, three controls).
- `activeRun === null` and `lastResult` set → **Result** (outcome splash, Play Again).

- [ ] **Step 1: Implement the component**

```tsx
// frontend/src/components/exit-liquidity/ExitLiquidityGame.tsx
import { useState } from 'react';
import { toast } from 'sonner';
import { TrendingUp, Lock, Coins, Dice5 } from 'lucide-react';
import { useWallet } from '../../hooks/useWallet';
import {
  useGetExitLiquidityConfig, useGetActiveExitRun, useGetExitRunCount,
  useStartExitRun, useExitRunDecision,
} from '../../hooks/useQueries';
import { bpsToMultiplier, tellForStage, nextStageRewardBps } from '../../lib/exitLiquidity';
import { prettifyCanisterError } from '../../lib/errorMessages';
import { triggerConfetti } from '../ConfettiCanvas';
import type { ExitDecision, ExitRunResult } from '../../declarations/shenanigans/shenanigans.did';
import LoadingSpinner from '../LoadingSpinner';

const TELL_CLASS: Record<string, string> = {
  Calm: 'mc-text-green', Firm: 'mc-text-green', Choppy: 'mc-text-gold',
  Toppy: 'mc-text-gold', Critical: 'mc-text-danger',
};

export default function ExitLiquidityGame() {
  const { principal } = useWallet();
  const { data: config } = useGetExitLiquidityConfig();
  const { data: activeRun, isLoading } = useGetActiveExitRun(principal ?? undefined);
  const { data: runCount } = useGetExitRunCount(principal ?? undefined);
  const start = useStartExitRun();
  const decide = useExitRunDecision();
  const [lastResult, setLastResult] = useState<ExitRunResult | null>(null);

  const stageCount = config ? Number(config.stageCount) : 5;
  const windowSize = config ? Number(config.windowSize) : 25;
  const buyInPp = config ? Number(config.buyInUnits) / 1e8 : 0;

  const onStart = async () => {
    try { setLastResult(null); await start.mutateAsync(); }
    catch (e) { toast.error(prettifyCanisterError(e).message); }
  };

  const onDecide = async (decision: ExitDecision) => {
    try {
      const result = await decide.mutateAsync(decision);
      const ended = result.rotated || ('exit' in decision) || Number(result.finalStage) >= stageCount;
      if (ended) {
        setLastResult(result);
        if (!result.rotated && Number(result.runScoreBps) > 20000) triggerConfetti();
      }
    } catch (e) { toast.error(prettifyCanisterError(e).message); }
  };

  if (isLoading) return <LoadingSpinner />;

  // ---- Result splash ----
  if (!activeRun && lastResult) {
    const r = lastResult;
    return (
      <div className="mc-card-elevated p-6 text-center max-w-md mx-auto">
        <h3 className={`font-display text-lg mb-2 ${r.rotated ? 'mc-text-danger' : 'mc-text-green'}`}>
          {r.rotated ? 'The music stopped. You were the exit liquidity.' : 'Clean exit.'}
        </h3>
        <p className="text-3xl font-display mc-text-primary my-3">{bpsToMultiplier(r.runScoreBps)}</p>
        <p className="text-sm mc-text-dim mb-4">
          {r.qualified
            ? `Best window this round: ${bpsToMultiplier(r.bestWindowAvgBps)}`
            : `${Number(runCount ?? 0n)}/${windowSize} runs to qualify for the cap table`}
        </p>
        <button className="mc-btn-primary w-full" onClick={onStart} disabled={start.isPending}>
          Commit Capital — {buyInPp} PP
        </button>
      </div>
    );
  }

  // ---- In-flight ----
  if (activeRun) {
    const stage = Number(activeRun.stage);
    const riding = Number(activeRun.ridingBps);
    const banked = Number(activeRun.bankedBps);
    const tell = tellForStage(stage, stageCount);
    const reward = config ? nextStageRewardBps(riding, Number(config.stageStepBps)) : riding;
    const busy = decide.isPending;
    return (
      <div className="mc-card-elevated p-6 max-w-md mx-auto">
        <div className="flex justify-between text-sm mb-4">
          <span className="mc-text-dim">Stage {stage}/{stageCount}</span>
          <span className={`font-bold ${TELL_CLASS[tell]}`}>{tell}</span>
        </div>
        <div className="grid grid-cols-2 gap-3 mb-4">
          <div className="text-center">
            <div className="flex items-center justify-center gap-1 mc-text-dim text-xs"><Lock className="h-3 w-3" /> Banked (safe)</div>
            <div className="font-display text-2xl mc-text-green">{bpsToMultiplier(banked)}</div>
          </div>
          <div className="text-center">
            <div className="flex items-center justify-center gap-1 mc-text-dim text-xs"><TrendingUp className="h-3 w-3" /> Riding</div>
            <div className="font-display text-2xl mc-text-gold">{bpsToMultiplier(riding)}</div>
          </div>
        </div>
        <p className="text-center text-xs mc-text-dim mb-4">Survive the next rotation → riding grows to ~{bpsToMultiplier(reward)}</p>
        <div className="grid grid-cols-3 gap-2">
          <button className="mc-btn-secondary" disabled={busy} onClick={() => onDecide({ bank: null } as ExitDecision)}>Take Distribution</button>
          <button className="mc-btn-secondary" disabled={busy} onClick={() => onDecide({ ride: null } as ExitDecision)}>Let It Ride</button>
          <button className="mc-btn-primary" disabled={busy} onClick={() => onDecide({ exit: null } as ExitDecision)}>Cash Out</button>
        </div>
      </div>
    );
  }

  // ---- Idle ----
  return (
    <div className="mc-card-elevated p-6 text-center max-w-md mx-auto">
      <Dice5 className="h-10 w-10 mc-text-green mx-auto mb-3" />
      <h3 className="font-display text-lg mc-text-primary mb-1">Exit Liquidity</h3>
      <p className="text-sm mc-text-dim mb-4">Ride the position, take distributions, cash out before the rotation. The only prize is the cap table.</p>
      <p className="text-xs mc-text-dim mb-4 flex items-center justify-center gap-1"><Coins className="h-3 w-3" /> {Number(runCount ?? 0n)}/{windowSize} runs to qualify this round</p>
      <button className="mc-btn-primary w-full" onClick={onStart} disabled={start.isPending || !principal}>
        Commit Capital — {buyInPp} PP
      </button>
    </div>
  );
}
```

> Implementer note: confirm the button utility classes (`mc-btn-primary`/`mc-btn-secondary`) exist in the design system; if the repo uses a `<Button>` component (`components/ui`) instead, swap to that. Confirm `prettifyCanisterError` returns `{ message }` (check its signature in `lib/errorMessages.ts`) and adjust if it returns a plain string.

- [ ] **Step 2: Typecheck**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/exit-liquidity/ExitLiquidityGame.tsx
git commit -m "feat(frontend): Exit Liquidity play loop"
```

---

## Task 4: `ExitLiquidityLeaderboard` — the cap table

**Files:**
- Create: `frontend/src/components/exit-liquidity/ExitLiquidityLeaderboard.tsx`

- [ ] **Step 1: Implement**

```tsx
// frontend/src/components/exit-liquidity/ExitLiquidityLeaderboard.tsx
import { Crown } from 'lucide-react';
import { Principal } from '@dfinity/principal';
import { useGetExitLiquidityLeaderboard, useGetCurrentRoundId } from '../../hooks/useQueries';
import { GoldenNameByPrincipal } from '../GoldenName';
import { bpsToMultiplier } from '../../lib/exitLiquidity';

export default function ExitLiquidityLeaderboard() {
  const { data: roundId } = useGetCurrentRoundId();
  const { data: rows = [] } = useGetExitLiquidityLeaderboard(
    roundId !== undefined ? Number(roundId) : undefined, 25,
  );

  return (
    <div className="mc-card-elevated p-4 max-w-md mx-auto mt-6">
      <h3 className="font-display mc-text-primary mb-3">The Cap Table</h3>
      {rows.length === 0 ? (
        <p className="text-sm mc-text-dim">No qualified players yet this round. Be the first to survive 25 runs.</p>
      ) : (
        <ol className="space-y-2">
          {rows.map(([p, scoreBps]: [Principal, bigint], i: number) => (
            <li key={p.toText()} className="flex items-center justify-between text-sm">
              <span className="flex items-center gap-2 min-w-0">
                <span className="mc-text-dim w-5 text-right">{i + 1}</span>
                {i === 0 && <Crown className="h-4 w-4 mc-text-gold flex-shrink-0" aria-label="Champion" />}
                <GoldenNameByPrincipal principal={p} className="truncate max-w-[160px]" />
              </span>
              <span className="font-display mc-text-gold">{bpsToMultiplier(scoreBps)}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Typecheck + commit**

Run: `npx tsc --noEmit` (expect: no errors)

```bash
git add frontend/src/components/exit-liquidity/ExitLiquidityLeaderboard.tsx
git commit -m "feat(frontend): Exit Liquidity cap-table leaderboard"
```

---

## Task 5: `ExitLiquidity` wrapper

**Files:**
- Create: `frontend/src/components/ExitLiquidity.tsx`

- [ ] **Step 1: Implement**

```tsx
// frontend/src/components/ExitLiquidity.tsx
import ExitLiquidityGame from './exit-liquidity/ExitLiquidityGame';
import ExitLiquidityLeaderboard from './exit-liquidity/ExitLiquidityLeaderboard';

export default function ExitLiquidity() {
  return (
    <div className="py-2">
      <ExitLiquidityGame />
      <ExitLiquidityLeaderboard />
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/ExitLiquidity.tsx
git commit -m "feat(frontend): Exit Liquidity wrapper"
```

---

## Task 6: Mount as a dashboard tab

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/Dashboard.tsx`

- [ ] **Step 1: Extend `TabType` and desktop nav** (`App.tsx`)

Change the union (`:30`):
```ts
export type TabType = 'profitCenter' | 'invest' | 'seedRound' | 'mlm' | 'shenanigans' | 'exitLiquidity';
```
Add to `headerNavItems` (`:32`), after the `shenanigans` entry, and add `CircleDollarSign` to the existing `lucide-react` import (`:19`) if not already present:
```ts
  { id: 'exitLiquidity', label: 'Exit Liquidity', icon: <CircleDollarSign className="h-4 w-4" /> },
```

- [ ] **Step 2: Wire the tab in `Dashboard.tsx`**

Import (top):
```ts
import ExitLiquidity from './ExitLiquidity';
import { CircleDollarSign } from 'lucide-react';
```
Add to `navItems` (`:36`) after the `shenanigans` entry:
```ts
  { id: 'exitLiquidity', mobileLabel: 'Exit Liq', icon: <CircleDollarSign className="h-5 w-5" /> },
```
Add to `sectionLabels` (`:90`): `exitLiquidity: 'Exit Liquidity',`
Add an `exitLiquiditySubtitles` array near the others and reference it in `sectionSubtitles` (`:82`):
```ts
const exitLiquiditySubtitles = [
  "Time the top. You won't. Try anyway.",
  "Don't be the one still holding when the music stops.",
  "Discipline is rewarded. Greed is exit liquidity.",
];
// in sectionSubtitles:
  exitLiquidity: pickRandom(exitLiquiditySubtitles),
```
Add the case to `renderContent` (`:139`):
```ts
      case 'exitLiquidity': return <div className={cls}>{shenanigansEnabled ? <ExitLiquidity /> : <ShenanigansComingSoon />}</div>;
```

- [ ] **Step 3: Fix any `Record<TabType, …>` literals the new key breaks**

Run: `npx tsc --noEmit`
Expected: TS errors flag every `Record<TabType, …>` literal missing the `exitLiquidity` key (e.g. the `badges` record built in `App.tsx`). Add an `exitLiquidity` entry to each (mirror the `shenanigans` value). Re-run until clean.

- [ ] **Step 4: Build + commit**

Run: `npm run build`
Expected: `tsc && vite build` succeeds.

```bash
git add frontend/src/App.tsx frontend/src/components/Dashboard.tsx
git commit -m "feat(frontend): mount Exit Liquidity tab"
```

---

## Task 7: Docs blurb

**Files:**
- Modify: `frontend/src/components/DocsPage.tsx`

- [ ] **Step 1: Add a short section** (follow the existing section markup in `DocsPage.tsx` — find a sibling `<section>`/heading and mirror it)

Content (on-brand, one joke, no probabilities):
> **Exit Liquidity.** Commit Ponzi Points, ride a position through up to five rotations, and take distributions before the music stops. Banked gains are safe; whatever's still riding is forfeit if the rotation catches you. The only payout is your rank on the Cap Table — best 25-run average wins. Discipline is rewarded. Greed is exit liquidity.

- [ ] **Step 2: Build + commit**

Run: `npm run build` (expect: success)

```bash
git add frontend/src/components/DocsPage.tsx
git commit -m "docs(frontend): Exit Liquidity blurb on DocsPage"
```

---

## Task 8: Manual browser verification

No component test runner (no `@testing-library/react`), so verify in the running app against a local replica (or mainnet read-only for the board). Precondition: backend plan deployed; signed in with a PP-funded identity.

Run: `npm run dev`

- [ ] **Step 1: Tab + idle.** The **Exit Liquidity** tab appears (desktop header + mobile bottom nav). Idle screen shows "Commit Capital — {n} PP" and "0/25 runs to qualify". A signed-out user sees the Coming Soon gate.
- [ ] **Step 2: Start.** Click Commit Capital → PP balance drops by the buy-in (check the header PP readout / Bank page) → in-flight screen at Stage 1, Banked 0.00×, Riding 1.00×.
- [ ] **Step 3: Tell escalates.** Let It Ride / Take Distribution to advance; the tier label climbs Calm → … → Critical with stage, and color shifts green → gold → red. **Confirm no percentage is ever shown.**
- [ ] **Step 4: Banking protects.** Take Distribution a few times, then keep riding until a rotation: outcome shows "The music stopped…" and the score equals the banked total (non-zero). A never-banked rotation shows 0.00×.
- [ ] **Step 5: Cash Out is safe.** Cash Out → "Clean exit." with score = Banked + Riding; confetti on a fat exit.
- [ ] **Step 6: One run at a time.** Starting a run while one is active is impossible (UI only shows controls for the active run); the backend also rejects it (toast on the edge case).
- [ ] **Step 7: Cap Table.** Before 25 runs the board reads "No qualified players yet". After a player qualifies, they appear ranked by best-window multiplier; #1 shows the `Crown`; paid-gold names still render gilded independently.

---

## Self-review (against the spec + backend plan)

- **Spec coverage:** turn-based bank/ride/exit play loop (Task 3) ✓; banking-locks-gains reflected in Banked/Riding split + result (Task 3) ✓; silent qualitative tell, never a probability (Tasks 1/3, enforced in copy + verification) ✓; per-Round cap table ranked by best-window average (Task 4) ✓; champion crown distinct from paid gold — resolves the spec's open golden-name question (Task 4) ✓; vanity surfaced via score display, board excludes unqualified (Tasks 3/4) ✓.
- **Type consistency:** `ExitDecision` built as `{ bank: null }`/`{ ride: null }`/`{ exit: null }` in Task 3 matches the candid variant; `ExitRun`/`ExitRunResult`/`ExitLiquidityConfig` fields (`stage`, `ridingBps`, `bankedBps`, `stageCount`, `windowSize`, `buyInUnits`, `stageStepBps`, `runScoreBps`, `rotated`, `finalStage`, `qualified`, `bestWindowAvgBps`) match the backend plan's Task 1.
- **Known follow-ups:** (1) `exitRunDecision` doesn't return the next riding stack, so the UI re-reads `getActiveExitRun` after each decision — a future backend tweak could return the full `ExitRun` to save the round-trip. (2) The tell is deterministic per stage in v1; adding noise/conflicting signals (the spec's richer ambiguity) is a v1.1 enhancement. (3) Leaderboard uses a simple list; upgrading to the `hall-of-fame/` Podium components is optional polish.
- **Placeholders:** the only soft spots are the two implementer notes (button-class naming, `prettifyCanisterError` return shape) — both call out exactly what to confirm against existing code.

---

## Execution handoff

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between. REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**2. Inline Execution** — this session, checkpoints. REQUIRED SUB-SKILL: superpowers:executing-plans.
