# SIWS Invest Flow — SOL Plan Deposits Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let SIWS (Phantom/Solana) users open plan positions from the invest tab by depositing devnet SOL, mirroring `BuySOLFlyout`'s `prepareSolDeposit` mechanics but driven by `GamePlans`' selected plan.

**Architecture:** Keep `GamePlans` phases ①–② (mode/plan selection) shared by all wallets, swapping "PP per ICP" → "PP per SOL" copy for SIWS. Branch at phase ③: `walletType === 'siws'` renders a new, focused `SolInvestPanel` (SOL amount → ICP-identical ROI/PP projection → `prepareSolDeposit` → deposit address + QR + pending state) instead of the ICP balance-gate + `useCreateGame` flow. Pure helpers (plan→variant mapping, SOL float formatting) are unit-tested; the components are preview/manual-verified (no component harness exists).

**Tech Stack:** React + TypeScript, React Query hooks (existing SOL hooks in `useQueries.ts`), `qrcode.react`, vitest. Backend: one Motoko constant change in `ponzi_math_sol`.

**Repo layout note:** `package.json`/`tsconfig` live at the **repo root**; frontend source is under `frontend/src/`. All commands run from the repo root. Commit only the files each task lists — **never `git add -A`** (there is unrelated user WIP on other paths).

**Spec:** `docs/superpowers/specs/2026-05-30-siws-invest-flow-design.md`

---

### Task 1: `MIN_DEPOSIT_SOL` constant + `formatSolFloat` helper

`formatSolFloat` displays a SOL amount given as a **float** (the ROI helpers return SOL floats) by reusing the canonical `formatSOL` lamport formatter. `MIN_DEPOSIT_SOL` is the new 0.01 floor.

**Files:**
- Modify: `frontend/src/lib/gameConstants.ts`
- Modify: `frontend/src/solana/lamports.ts`
- Test: `frontend/src/solana/lamports.test.ts`

- [ ] **Step 1: Write the failing test** — append to `frontend/src/solana/lamports.test.ts` (the file already imports `formatSOL, parseSOL, LAMPORTS_PER_SOL` from `./lamports`; update that import line to add `formatSolFloat`):

```ts
import { formatSOL, parseSOL, formatSolFloat, LAMPORTS_PER_SOL } from './lamports';

describe('formatSolFloat', () => {
  it('formats 0 as "0.0000"', () => {
    expect(formatSolFloat(0)).toBe('0.0000');
  });

  it('formats 1.5 SOL as "1.5000"', () => {
    expect(formatSolFloat(1.5)).toBe('1.5000');
  });

  it('formats 0.001 SOL as "0.0010"', () => {
    expect(formatSolFloat(0.001)).toBe('0.0010');
  });

  it('returns "0.0000" for NaN', () => {
    expect(formatSolFloat(NaN)).toBe('0.0000');
  });

  it('returns "0.0000" for negative input', () => {
    expect(formatSolFloat(-1)).toBe('0.0000');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run frontend/src/solana/lamports.test.ts`
Expected: FAIL — `formatSolFloat is not a function` (or an import/type error).

- [ ] **Step 3: Implement `formatSolFloat`** — append to `frontend/src/solana/lamports.ts`:

```ts
// Display a SOL amount supplied as a float (e.g. an ROI projection) by reusing
// formatSOL's lamport formatting. Guards NaN / non-finite / non-positive input.
export function formatSolFloat(value: number): string {
  if (!isFinite(value) || value <= 0) return formatSOL(0n);
  return formatSOL(BigInt(Math.round(value * Number(LAMPORTS_PER_SOL))));
}
```

- [ ] **Step 4: Add the `MIN_DEPOSIT_SOL` constant** — in `frontend/src/lib/gameConstants.ts`, in the "Deposit Limits" block (right after `export const MIN_DEPOSIT_ICP = 0.1;`), add:

```ts
export const MIN_DEPOSIT_SOL = 0.01;   // SOL plan-deposit floor (matches ponzi_math_sol MIN_DEPOSIT_LAMPORTS = 10_000_000)
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `npx vitest run frontend/src/solana/lamports.test.ts`
Expected: PASS (all `formatSolFloat` cases green, existing `formatSOL`/`parseSOL` cases still green).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/solana/lamports.ts frontend/src/solana/lamports.test.ts frontend/src/lib/gameConstants.ts
git commit -m "feat(siws-invest): add MIN_DEPOSIT_SOL and formatSolFloat helper" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `solPlanMapping` helpers (plan id → `SolGamePlan`, PP-per-SOL rate)

Pure mapping from the invest-tab `planId` (the same ids `getDailyRate`/`getPlanDays` use) to the `ponzi_math_sol` `GamePlan` variant and the display PP-per-SOL rate.

**Files:**
- Create: `frontend/src/lib/solPlanMapping.ts`
- Test: `frontend/src/lib/solPlanMapping.test.ts`

- [ ] **Step 1: Write the failing test** — create `frontend/src/lib/solPlanMapping.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { investPlanToSolGamePlan, ppPerSolForPlan } from './solPlanMapping';
import { SolGamePlan } from '../backend';
import {
  PP_PER_SOL_SIMPLE,
  PP_PER_SOL_COMPOUND_15,
  PP_PER_SOL_COMPOUND_30,
} from './gameConstants';

describe('investPlanToSolGamePlan', () => {
  it('maps 21-day-simple → simple21Day', () => {
    expect(investPlanToSolGamePlan('21-day-simple')).toEqual(SolGamePlan.simple21Day);
  });
  it('maps 15-day-compounding → compounding15Day', () => {
    expect(investPlanToSolGamePlan('15-day-compounding')).toEqual(SolGamePlan.compounding15Day);
  });
  it('maps 30-day-compounding → compounding30Day', () => {
    expect(investPlanToSolGamePlan('30-day-compounding')).toEqual(SolGamePlan.compounding30Day);
  });
  it('falls back to simple21Day for an unknown id', () => {
    expect(investPlanToSolGamePlan('nonsense')).toEqual(SolGamePlan.simple21Day);
  });
});

describe('ppPerSolForPlan', () => {
  it('returns the simple rate', () => {
    expect(ppPerSolForPlan('21-day-simple')).toBe(PP_PER_SOL_SIMPLE);
  });
  it('returns the 15-day rate', () => {
    expect(ppPerSolForPlan('15-day-compounding')).toBe(PP_PER_SOL_COMPOUND_15);
  });
  it('returns the 30-day rate', () => {
    expect(ppPerSolForPlan('30-day-compounding')).toBe(PP_PER_SOL_COMPOUND_30);
  });
  it('falls back to the simple rate for an unknown id', () => {
    expect(ppPerSolForPlan('nonsense')).toBe(PP_PER_SOL_SIMPLE);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run frontend/src/lib/solPlanMapping.test.ts`
Expected: FAIL — cannot find module `./solPlanMapping`.

- [ ] **Step 3: Implement the helpers** — create `frontend/src/lib/solPlanMapping.ts`:

```ts
import { SolGamePlan } from '../backend';
import {
  PP_PER_SOL_SIMPLE,
  PP_PER_SOL_COMPOUND_15,
  PP_PER_SOL_COMPOUND_30,
} from './gameConstants';

// Invest-tab plan id (as used by getDailyRate/getPlanDays) → ponzi_math_sol
// GamePlan variant. Unknown ids fall back to the simple plan defensively.
export function investPlanToSolGamePlan(planId: string): SolGamePlan {
  switch (planId) {
    case '15-day-compounding':
      return SolGamePlan.compounding15Day;
    case '30-day-compounding':
      return SolGamePlan.compounding30Day;
    case '21-day-simple':
    default:
      return SolGamePlan.simple21Day;
  }
}

// Display-only PP-per-SOL rate for an invest-tab plan id.
export function ppPerSolForPlan(planId: string): number {
  switch (planId) {
    case '15-day-compounding':
      return PP_PER_SOL_COMPOUND_15;
    case '30-day-compounding':
      return PP_PER_SOL_COMPOUND_30;
    case '21-day-simple':
    default:
      return PP_PER_SOL_SIMPLE;
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run frontend/src/lib/solPlanMapping.test.ts`
Expected: PASS (all 8 cases green).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/solPlanMapping.ts frontend/src/lib/solPlanMapping.test.ts
git commit -m "feat(siws-invest): add invest-plan → SolGamePlan mapping helpers" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `SolInvestPanel` component

The SIWS phase-③ panel. States: **actor-not-ready** (disabled hint), **input** (amount + ICP-identical ROI/PP, DEVNET + GAMBLING banners, "Reserve Deposit Address" CTA), **reserved** (locked amount + address + copy + QR + pending count + "Start over" / "Go to Profit Center"). No component unit test (no harness) — verified by `tsc` + `build` here and preview later.

**Files:**
- Create: `frontend/src/components/SolInvestPanel.tsx`

- [ ] **Step 1: Create the component** — create `frontend/src/components/SolInvestPanel.tsx`:

```tsx
import { useMemo, useState } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, BarChart3, TrendingUp, Dices } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyPendingSolIntents,
  calculateSimpleROI,
  calculateCompoundingROI,
  getDailyRate,
  getPlanDays,
} from '../hooks/useQueries';
import { usePonziMathSolActor } from '../hooks/usePonziMathSolActor';
import { formatSOL, formatSolFloat, parseSOL, LAMPORTS_PER_SOL } from '../solana/lamports';
import { COVER_CHARGE_RATE, MIN_DEPOSIT_SOL, pct } from '../lib/gameConstants';
import { investPlanToSolGamePlan, ppPerSolForPlan } from '../lib/solPlanMapping';

// Computed once: 0.01 SOL as lamports.
const MIN_LAMPORTS = parseSOL(String(MIN_DEPOSIT_SOL));

interface SolInvestPanelProps {
  planId: string;
  onNavigateToProfitCenter?: () => void;
}

export default function SolInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { actor } = usePonziMathSolActor();
  const prepareMut = usePrepareSolDeposit();
  const { data: pendingIntents } = useGetMyPendingSolIntents();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [intentResult, setIntentResult] = useState<
    { intentId: bigint; depositAddress: string; lamports: bigint } | null
  >(null);

  const isCompounding = planId !== '21-day-simple';
  const days = getPlanDays(planId);
  const ppPerSol = ppPerSolForPlan(planId);

  const lamports = useMemo(() => {
    try {
      return solInput.trim() ? parseSOL(solInput) : 0n;
    } catch {
      return 0n;
    }
  }, [solInput]);

  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const belowMin = lamports > 0n && lamports < MIN_LAMPORTS;
  const canReserve = !!actor && lamports >= MIN_LAMPORTS && !prepareMut.isPending && !intentResult;

  // ROI mirrors the ICP panel exactly: projected on the NET deposit; PP on gross.
  const net = solFloat * (1 - COVER_CHARGE_RATE);
  const roi = solFloat > 0
    ? (isCompounding ? calculateCompoundingROI(net, planId, days) : calculateSimpleROI(net, planId, days))
    : null;
  const projectedPP = solFloat > 0 ? Math.round(solFloat * ppPerSol) : 0;
  const dailyEarnings = roi ? net * getDailyRate(planId) : 0;

  const roiColor = !roi ? 'mc-text-green'
    : roi.roiPercent < 50 ? 'mc-text-green'
    : roi.roiPercent < 200 ? 'mc-text-purple mc-glow-purple'
    : 'mc-text-gold mc-glow-gold';

  const handleReserve = async () => {
    if (!canReserve) return;
    try {
      const result = await prepareMut.mutateAsync({
        plan: investPlanToSolGamePlan(planId),
        expectedAmountLamports: lamports,
      });
      setIntentResult({ intentId: result.intentId, depositAddress: result.depositAddress, lamports });
    } catch {
      // surfaces via prepareMut.isError below
    }
  };

  const handleStartOver = () => {
    setIntentResult(null);
    prepareMut.reset();
  };

  const handleCopy = async () => {
    if (!intentResult) return;
    await navigator.clipboard.writeText(intentResult.depositAddress);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (!actor) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting your Solana session…</p>
      </div>
    );
  }

  const qrPayload = intentResult
    ? `solana:${intentResult.depositAddress}?amount=${formatSOL(intentResult.lamports)}`
    : null;

  return (
    <div className="space-y-6">
      <div className="mc-status-amber p-3 text-center text-xs font-bold">
        <AlertTriangle className="h-4 w-4 inline mr-1" /> DEVNET — send devnet SOL only. This position is funded on Solana devnet.
      </div>

      {!intentResult ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Amount input + CTA */}
          <div>
            <div className="flex justify-between items-center mb-2">
              <span className="text-xs font-bold mc-text-primary">Amount</span>
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => setSolInput(String(MIN_DEPOSIT_SOL))}
                className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap"
              >MIN</button>
              <input
                type="text"
                inputMode="decimal"
                value={solInput}
                onChange={(e) => setSolInput(e.target.value)}
                placeholder={`Min: ${MIN_DEPOSIT_SOL} SOL`}
                className="mc-input flex-1 text-center text-lg font-mono"
              />
            </div>

            {belowMin && (
              <div className="mt-2 text-xs mc-text-danger">
                <AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {MIN_DEPOSIT_SOL} SOL
              </div>
            )}

            <div className="mc-status-red p-3 text-center text-sm font-bold mt-3">
              <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
              <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
            </div>

            <button
              onClick={handleReserve}
              disabled={!canReserve}
              className={`w-full py-3 mt-3 text-sm font-bold rounded-xl transition-all mc-btn-primary inline-flex items-center justify-center gap-2 ${canReserve ? 'pulse' : ''}`}
            >
              {prepareMut.isPending
                ? 'Reserving…'
                : <><Dices className="h-5 w-5" /> RESERVE DEPOSIT ADDRESS</>}
            </button>

            {prepareMut.isError && (
              <p className="text-xs mc-text-danger mt-2 text-center">
                {(prepareMut.error as Error).message}
              </p>
            )}
          </div>

          {/* ROI calculator — same figures/breakdown as the ICP panel, SOL-denominated */}
          <div>
            {roi ? (
              <div>
                <div className="text-center mb-3">
                  <span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span>
                </div>
                <div className="mc-card p-4 space-y-3">
                  <div className="flex justify-between items-center">
                    <div>
                      <div className="mc-label">{isCompounding ? 'Compounded Interest' : 'Interest Payout'}</div>
                      <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatSolFloat(roi.totalReturn)} SOL</div>
                      <div className={`text-xs opacity-70 ${roiColor}`}>
                        {isCompounding
                          ? `${roi.roiPercent.toFixed(1)}% ROI`
                          : `${(roi.totalReturn / net).toFixed(2)}x ROI (${roi.roiPercent.toFixed(0)}%)`}
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="mc-label">Ponzi Points</div>
                      <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{projectedPP.toLocaleString()}</div>
                      <div className="text-xs mc-text-purple opacity-70">{ppPerSol.toLocaleString()} / SOL</div>
                    </div>
                  </div>
                  <div className="border-t border-white/10 pt-3 space-y-1.5">
                    <div className="flex justify-between text-sm">
                      <span className="mc-text-muted">Front-End Load ({pct(COVER_CHARGE_RATE)})</span>
                      <span className="mc-text-primary font-medium">-{formatSolFloat(solFloat * COVER_CHARGE_RATE)} SOL</span>
                    </div>
                    <div className="flex justify-between text-sm">
                      <span className="mc-text-muted">Net deposit</span>
                      <span className="mc-text-primary font-medium">{formatSolFloat(net)} SOL</span>
                    </div>
                    <div className="flex items-center justify-between pt-1">
                      <div className="flex items-center gap-1.5">
                        <TrendingUp className="h-3.5 w-3.5 mc-text-cyan" />
                        <span className="text-xs mc-text-dim">Daily earnings</span>
                      </div>
                      <span className="text-sm font-bold mc-text-cyan">{formatSolFloat(dailyEarnings)} SOL/day</span>
                    </div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="h-full flex items-center justify-center text-center">
                <div>
                  <BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" />
                  <p className="text-sm mc-text-muted">Enter an amount to see projected returns</p>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : (
        /* Reserved state — locked amount, address, QR, pending */
        <div className="mc-card p-4 space-y-3 max-w-md mx-auto">
          <div className="text-center">
            <div className="mc-label">Send exactly</div>
            <div className="text-2xl font-bold mc-text-gold">{formatSOL(intentResult.lamports)} SOL</div>
            <div className="text-xs mc-text-dim mt-1">
              devnet SOL from Phantom — your position opens automatically within ~a minute.
            </div>
          </div>

          <div className="mc-label">Deposit address</div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={intentResult.depositAddress}>
              {intentResult.depositAddress}
            </code>
            <button onClick={handleCopy} className="mc-btn-secondary text-xs">
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>

          {qrPayload && (
            <div className="flex justify-center pt-2">
              <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
            </div>
          )}

          {pendingIntents && pendingIntents.length > 0 && (
            <div className="text-[10px] mc-text-muted text-center">
              {pendingIntents.length} pending deposit{pendingIntents.length === 1 ? '' : 's'} awaiting confirmation
            </div>
          )}

          <div className="flex gap-3 justify-center pt-2">
            <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">
              Start over
            </button>
            <button
              onClick={() => onNavigateToProfitCenter?.()}
              className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2"
            >
              <TrendingUp className="h-4 w-4" /> Go to Profit Center
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS (no errors). If `usePrepareSolDeposit`'s return type doesn't expose `intentId`/`depositAddress`, re-check the unwrap at `useQueries.ts:2416` — it returns `result.Ok` which is `{ intentId: bigint; depositAddress: string }`.

- [ ] **Step 3: Build**

Run: `npm run build`
Expected: PASS (`tsc && vite build` complete with no errors).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/SolInvestPanel.tsx
git commit -m "feat(siws-invest): add SolInvestPanel (SOL deposit panel for invest tab)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Wire `GamePlans` — SIWS PP copy + phase-③ branch

Three edits: imports, SIWS-aware PP display values + the five "PP per ICP" → "PP per {unit}" copy swaps, and the phase-③ SIWS branch.

**Files:**
- Modify: `frontend/src/components/GamePlans.tsx`

- [ ] **Step 1: Add imports** — replace the gameConstants import line (currently `import { COVER_CHARGE_RATE, pct } from '../lib/gameConstants';`):

```ts
import { COVER_CHARGE_RATE, pct, PP_PER_SOL_SIMPLE, PP_PER_SOL_COMPOUND_15, PP_PER_SOL_COMPOUND_30 } from '../lib/gameConstants';
import SolInvestPanel from './SolInvestPanel';
```

- [ ] **Step 2: Add SIWS-aware PP display values** — immediately after the `ppRates` object literal (the block ending `comp30Day: mintConfig ? Number(mintConfig.compounding30DayPpPerIcp) : 0, };`), insert:

```ts
  // SIWS users deposit SOL — show SOL-denominated PP rates in the plan cards/strips.
  const isSiws = walletType === 'siws';
  const ppUnit = isSiws ? 'SOL' : 'ICP';
  const ppSimpleDisplay = isSiws ? PP_PER_SOL_SIMPLE : ppRates.simple21Day;
  const ppComp15Display = isSiws ? PP_PER_SOL_COMPOUND_15 : ppRates.comp15Day;
  const ppComp30Display = isSiws ? PP_PER_SOL_COMPOUND_30 : ppRates.comp30Day;
```

(`walletType` is already destructured from `useWallet()` near the top of the component.)

- [ ] **Step 3: Swap the five "PP per ICP" copy sites.** Make these exact replacements:

Compounding summary strip (the ternary inside the `phase > 2 && selectedMode === 'compounding'` strip):
```tsx
            {selectedPlan === '15-day-compounding'
              ? `15 days · 12%/day · ${ppRates.comp15Day.toLocaleString()} PP/ICP`
              : `30 days · 9%/day · ${ppRates.comp30Day.toLocaleString()} PP/ICP`}
```
→
```tsx
            {selectedPlan === '15-day-compounding'
              ? `15 days · 12%/day · ${ppComp15Display.toLocaleString()} PP/${ppUnit}`
              : `30 days · 9%/day · ${ppComp30Display.toLocaleString()} PP/${ppUnit}`}
```

Phase 1 — Starter card:
```tsx
                <li>• Ponzi Points: {ppRates.simple21Day.toLocaleString()} PP per ICP</li>
```
→
```tsx
                <li>• Ponzi Points: {ppSimpleDisplay.toLocaleString()} PP per {ppUnit}</li>
```

Phase 1 — VIP card:
```tsx
                <li>• Ponzi Points: {ppRates.comp15Day.toLocaleString()}–{ppRates.comp30Day.toLocaleString()} PP per ICP</li>
```
→
```tsx
                <li>• Ponzi Points: {ppComp15Display.toLocaleString()}–{ppComp30Display.toLocaleString()} PP per {ppUnit}</li>
```

Phase 2 — Executive card:
```tsx
                <li>• {ppRates.comp15Day.toLocaleString()} Ponzi Points per ICP</li>
```
→
```tsx
                <li>• {ppComp15Display.toLocaleString()} Ponzi Points per {ppUnit}</li>
```

Phase 2 — Chairman's card:
```tsx
                <li>• {ppRates.comp30Day.toLocaleString()} Ponzi Points per ICP</li>
```
→
```tsx
                <li>• {ppComp30Display.toLocaleString()} Ponzi Points per {ppUnit}</li>
```

- [ ] **Step 4: Branch phase ③ for SIWS.** Replace this exact block:

```tsx
        {/* ============ PHASE 3: Amount + ROI + CTA ============ */}
        {phase === 3 && (
          <div className="space-y-6">
```
→
```tsx
        {/* ============ PHASE 3: SIWS → SOL deposit panel ============ */}
        {phase === 3 && walletType === 'siws' && (
          <SolInvestPanel planId={selectedPlan} onNavigateToProfitCenter={onNavigateToProfitCenter} />
        )}

        {/* ============ PHASE 3: Amount + ROI + CTA (ICP wallets) ============ */}
        {phase === 3 && walletType !== 'siws' && (
          <div className="space-y-6">
```

The existing ICP phase-③ body (empty-wallet CTA, amount input, ROI calc, `createGameMutation` error) is unchanged — it now only renders for non-SIWS wallets.

- [ ] **Step 5: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Build**

Run: `npm run build`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/GamePlans.tsx
git commit -m "feat(siws-invest): branch invest tab to SolInvestPanel for SIWS + SOL PP copy" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Lower the `ponzi_math_sol` deposit minimum to 0.01 SOL (code only)

Code change only. **The deploy is operator-gated and is NOT part of this execution** (see "Deploy" below). This makes the backend accept the 0.01 SOL the frontend now allows.

**Files:**
- Modify: `ponzi_math_sol/main.mo`

- [ ] **Step 1: Lower the constant** — replace:

```motoko
    transient let MIN_DEPOSIT_LAMPORTS : Nat64 = 50_000_000;
```
→
```motoko
    transient let MIN_DEPOSIT_LAMPORTS : Nat64 = 10_000_000;
```

- [ ] **Step 2: Update the `prepareSolDeposit` error string** — replace:

```motoko
            return #Err("Minimum deposit is 0.05 SOL (50,000,000 lamports)");
```
→
```motoko
            return #Err("Minimum deposit is 0.01 SOL (10,000,000 lamports)");
```

(The `adminCreditManualDeposit` check at `main.mo:3354` reads the same constant; its generic `"Below minimum deposit"` string needs no change. The Founder's Allocation desk has separate min logic and is untouched.)

- [ ] **Step 3: Type-check the Motoko (build, do not deploy)**

Run: `dfx build ponzi_math_sol`
Expected: PASS (compiles with no errors). If `dfx` is unavailable in the execution environment, skip and note it — the change is a single literal + string and is verified at deploy time.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math_sol/main.mo
git commit -m "feat(siws-invest): lower ponzi_math_sol deposit minimum to 0.01 SOL" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

> **Deploy (operator-gated — DO NOT run without the user's explicit per-deploy permission).** After approval, with the `CharlesPonzi` dfx identity:
> ```
> dfx deploy ponzi_math_sol --network ic --mode upgrade --wasm-memory-persistence keep --yes --argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; solTreasuryAddress = "5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2"; solRpcProvider = variant { devnet }; keyId = record { algorithm = variant { ed25519 }; name = "key_1" } })'
> ```
> No migration is needed (constant-only change; `--wasm-memory-persistence keep` preserves state). **Before and after**, verify game state is preserved via `dfx canister --network ic call ponzi_math_sol getActiveGameCount` and `getTotalDeposits`. **Restore the dfx identity to `rumi_identity` afterward** (`dfx identity use rumi_identity`). Then confirm a 0.01 SOL `prepareSolDeposit` is accepted on devnet.

---

### Task 6: Full-suite verification

- [ ] **Step 1: Run the full test suite**

Run: `npm test`
Expected: PASS (all vitest files green, including the new `solPlanMapping` and `formatSolFloat` cases).

- [ ] **Step 2: Type-check the whole frontend**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Production build**

Run: `npm run build`
Expected: PASS.

- [ ] **Step 4: Preview verification (manual, document findings)**

Start the dev server and, as an anonymous/SIWS session on the invest tab, confirm: the plan cards read "PP per SOL"; selecting a plan reaches a SOL amount field (no ICP balance gate); entering ≥ 0.01 SOL shows the ROI/PP projection and the DEVNET + GAMBLING banners; below-0.01 shows the min error and disables the CTA. The authed reserve→address path needs a real Phantom (SIWS) wallet and is verified manually; full SOL→position e2e is the operator's devnet round-trip. **No component-test harness exists — do not invent one.**

---

## Notes / deliberate simplifications

- **No count-up animation in `SolInvestPanel`.** The ICP panel animates its ROI numbers via `useCountUp`; the SOL panel shows the same figures statically to keep the component focused. The *math/breakdown* is identical (the stated goal). Adding the animation later is a trivial, isolated change if desired.
- **No deposit-address pre-fetch.** The address comes from the `prepareSolDeposit` `Ok` response, so `useGetMyDepositAddress` is intentionally not used (mirrors the desk flow).
- **No "opened!" auto-detection.** The reserved state ends at "Go to Profit Center"; the position appears in `GameTracking` once the observer credits it. Detecting our `intentId` leaving the pending list to show an inline "You're in" nudge is an optional future polish, not built here.
- **Out of scope (own spec):** the "pay-on-net" game-math change to `ponzi_math` + `ponzi_math_sol`.
