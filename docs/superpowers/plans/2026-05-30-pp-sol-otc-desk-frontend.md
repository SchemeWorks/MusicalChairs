# Founder's Allocation Desk (Frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the frontend for the Founder's Allocation desk — a SIWS buyer flyout (quote → lock → pay → credited) replacing `BuySOLFlyout` in the sidebar slot, plus an MVP "Desk" admin tab in Charles's office (tiers, inventory, stats, proceeds withdrawal) — against the `ponzi_math_sol` desk candid merged in PR #98.

**Architecture:** Pure assembly of existing patterns. Anon reads via `useReadPonziMathSol`, SIWS/admin updates via `usePonziMathSolActor`, ledger approve via `useAuthPpLedger`. New hook modules (`useBuyPpDesk.ts` buyer, `useDeskAdmin.ts` admin), pure helpers (`lib/ppDesk.ts`, unit-tested), a buyer flyout (`BuyPpDeskFlyout.tsx`, modeled on `BuySOLFlyout` + `BuyPPFlyout`), and an admin panel (`PpDeskPanel.tsx`, modeled on `ShenanigansAdminPanel`). Wiring: swap the flyout in `BuySOLWidget`/`BuySOLFab`, add a `'desk'` tab in `App.tsx`.

**Tech Stack:** React 18 + TypeScript, `@tanstack/react-query`, `@dfinity/agent`, the generated `ponzi_math_sol`/`pp_ledger` declarations, `vitest`, `vite`.

**Scope:** MVP per the spec (`docs/superpowers/specs/2026-05-30-pp-sol-otc-desk-frontend-design.md`). OUT: open-intent table, refund UI, drag-reorder, any change to `BuySOLFlyout` itself (left for the invest-tab rework).

**Testing reality:** No component-test harness exists. Verification per task = `npx tsc --noEmit` (type gate against the generated candid) + `vitest` for the **pure helpers only** (the project's vitest suite already covers `lamports`/`base58`/`siwsSigner`). Component/hook behavior is verified by rendering on `npm run dev` — the **anon quote path works against the live devnet canister once tiers are stocked** (`quoteBuyPP` is an anon query); authed flows (lock/buy, admin mutations) and full SOL→PP e2e are manual with the right wallet (the operator devnet round-trip). Do NOT invent a component-test harness.

---

## Conventions to follow (verified against the live code)

- **Anon reads:** `useReadPonziMathSol()` (`frontend/src/hooks/useReadPonziMathSol.ts`) → `ActorSubclass<PonziMathSolService>`. Use for `quoteBuyPP`, `deskListTiers`, `deskInventory`, `deskStats`, `getDeskEscrowAccount`.
- **Auth updates:** `usePonziMathSolActor()` (`frontend/src/hooks/usePonziMathSolActor.ts`) → `{ actor, isFetching, error }`; `actor` is null until ready. Use for `createBuyIntent`, `getMyPendingBuyIntents`, and all `desk*` admin methods.
- **Wallet:** `useWallet()` → `{ identity, principal, solanaPubkey, walletType, isConnected }`.
- **Auth pp_ledger:** `useAuthPpLedger()` (`frontend/src/hooks/usePpLedger.ts`) → `ActorSubclass<_SERVICE> | null`; has `icrc2_approve`. Helpers: `PP_UNIT_SCALE = 100_000_000n`, `ppUnitsToWhole`, `wholePpToUnits`.
- **Candid types** (`frontend/src/declarations/ponzi_math_sol/ponzi_math_sol.did.d.ts`): `DeskQuote { legs: QuoteLeg[]; ppUnitsOut: bigint; cappedByInventory: boolean }`; `DeskTier { ratePpUnitsPer0_1Sol; ppUnitsTotal; ppUnitsReserved; ppUnitsSold }` (all `bigint`); `BuyIntent { id; quotedLamports; principal; expiresAt; fulfilled; createdAt; reserved; ppUnitsReservedTotal }`; `createBuyIntent: [bigint] → { Ok: { expiresAt; intentId; legs; depositAddress; ppUnitsReserved } } | { Err: string }`; `deskAddTier: [bigint,bigint]→{Ok:bigint}|{Err}`; `deskUpdateTier: [bigint,bigint,bigint]→{Ok:null}|{Err}`; `deskRemoveTier: [bigint]→{Ok:null}|{Err}`; `deskDepositInventory: [bigint]→{Ok:bigint}|{Err}`; `deskWithdrawInventory: [bigint,Principal]→{Ok:bigint}|{Err}`; `adminWithdrawDeskProceeds: [string]→{Ok:string}|{Err}`; `deskInventory: []→{balanceUnits;reservedUnits;availableUnits}`; `deskStats: []→{inventoryUnits;totalSoldUnits;openBuyIntents;proceedsLamports;reservedUnits;availableUnits;tierCount}`.
- **Reusable deposit-to-side-pocket** (`BuyPPFlyout.tsx:70-137,171-212`): `useAllowance`, `useApproveForDeposits`, `useDepositChips` from `../../hooks/useQueries`; CSS `mc-buy-pp-deposit-prompt` / `mc-buy-pp-deposit-button`; `formatPP(e8s)=(Number(e8s)/1e8).toLocaleString(undefined,{maximumFractionDigits:2})`.
- **Buyer-widget CSS** (reuse from `BuyPPFlyout`/`BuySOLFlyout`): `mc-buy-pp-widget`, `mc-buy-pp-input-row`, `mc-buy-pp-input`, `mc-buy-pp-input-suffix`, `mc-buy-pp-quote`, `mc-buy-pp-quote-amount`, `mc-buy-pp-button`, `mc-card`, `mc-status-amber`, `mc-btn-secondary`.
- **Address/QR** (`BuySOLFlyout.tsx:73-162`): `QRCodeCanvas` from `qrcode.react`, payload `solana:<addr>?amount=<formatSOL(lamports)>`, copy button with `navigator.clipboard`.
- **SOL fmt:** `formatSOL`/`parseSOL`/`LAMPORTS_PER_SOL` from `frontend/src/solana/lamports.ts`.
- **Admin gate:** `isCharles(principal)` from `frontend/src/lib/charles.tsx`.
- **Admin form pattern** (`ShenanigansAdminPanel.tsx:78-96`): `AdminInput` (label/hint/type/value/onChange/min/max) — replicate locally; `toast` from `sonner`; `LoadingSpinner`.
- **Admin tab block:** `App.tsx:294-295` (`adminPanelTab` state), `:739-760` (tab toggle group), `:763` (render). `Shenanigans.tsx:895/908` (slot), unchanged.

## File structure

- **Create** `frontend/src/lib/ppDesk.ts` — pure helpers (rate conversion, formatting, countdown, effective rate).
- **Create** `frontend/src/lib/ppDesk.test.ts` — vitest for the helpers.
- **Create** `frontend/src/hooks/useBuyPpDesk.ts` — buyer hooks.
- **Create** `frontend/src/hooks/useDeskAdmin.ts` — admin hooks.
- **Create** `frontend/src/components/Shenanigans/BuyPpDeskFlyout.tsx` — buyer flyout.
- **Create** `frontend/src/components/PpDeskPanel.tsx` — admin panel.
- **Modify** `frontend/src/components/Shenanigans/BuySOLWidget.tsx`, `BuySOLFab.tsx` — render `BuyPpDeskFlyout`.
- **Modify** `frontend/src/App.tsx` — `'desk'` tab.

---

## Task 1: Pure helpers (`lib/ppDesk.ts`) — TDD

**Files:** Create `frontend/src/lib/ppDesk.ts`, `frontend/src/lib/ppDesk.test.ts`

- [ ] **Step 1: Write the failing test** — `frontend/src/lib/ppDesk.test.ts`:

```ts
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
    // 0.1 SOL (1e8 lamports) buying 250k PP (25e12 units) → 250,000 PP per 0.1 SOL
    expect(effectiveRatePer0_1Sol(25_000_000_000_000n, 100_000_000n)).toBe('250,000');
    expect(effectiveRatePer0_1Sol(0n, 0n)).toBe('—');
  });
  it('formatCountdown formats remaining ms (and "expired")', () => {
    expect(formatCountdown(125_000)).toBe('2m 5s');
    expect(formatCountdown(0)).toBe('expired');
    expect(formatCountdown(-5)).toBe('expired');
  });
});
```

- [ ] **Step 2: Run it, verify it fails** — Run: `npx vitest run frontend/src/lib/ppDesk.test.ts`. Expected: FAIL (module `./ppDesk` not found).

- [ ] **Step 3: Implement** — `frontend/src/lib/ppDesk.ts`:

```ts
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
```

- [ ] **Step 4: Run it, verify it passes** — Run: `npx vitest run frontend/src/lib/ppDesk.test.ts`. Expected: PASS (5 tests).

- [ ] **Step 5: Commit** — `git add frontend/src/lib/ppDesk.ts frontend/src/lib/ppDesk.test.ts && git commit -m "feat(desk-fe): pure helpers for the Founder's Allocation desk"`

---

## Task 2: Buyer hooks (`useBuyPpDesk.ts`)

**Files:** Create `frontend/src/hooks/useBuyPpDesk.ts`

- [ ] **Step 1: Implement the three buyer hooks:**

```ts
/** Founder's Allocation desk — buyer hooks. Mirrors usePartyDexBuy / the SOL
 *  deposit hooks: anon debounced quote, auth lock mutation, auth pending poll. */
import { useEffect, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useReadPonziMathSol } from './useReadPonziMathSol';
import { usePonziMathSolActor } from './usePonziMathSolActor';
import { useWallet } from './useWallet';
import type { DeskQuote, BuyIntent } from '../declarations/ponzi_math_sol/ponzi_math_sol.did';

const QUOTE_DEBOUNCE_MS = 300;

export function useQuoteBuyPP(lamports: bigint) {
  const actor = useReadPonziMathSol();
  const [debounced, setDebounced] = useState<bigint>(lamports);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(lamports), QUOTE_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [lamports]);

  return useQuery<DeskQuote | null>({
    queryKey: ['ppDeskQuote', debounced.toString()],
    queryFn: async () => {
      if (debounced <= 0n) return null;
      return actor.quoteBuyPP(debounced);
    },
    enabled: debounced > 0n,
    staleTime: 5_000,
    refetchInterval: 15_000,
  });
}

export interface CreateBuyIntentResult {
  intentId: bigint;
  depositAddress: string;
  ppUnitsReserved: bigint;
  expiresAt: bigint;
}

export function useCreateBuyIntent() {
  const { actor } = usePonziMathSolActor();
  const queryClient = useQueryClient();
  return useMutation<CreateBuyIntentResult, Error, bigint>({
    mutationFn: async (lamports: bigint) => {
      if (!actor) throw new Error('Wallet not connected');
      const res = await actor.createBuyIntent(lamports);
      if ('Err' in res) throw new Error(res.Err);
      return {
        intentId: res.Ok.intentId,
        depositAddress: res.Ok.depositAddress,
        ppUnitsReserved: res.Ok.ppUnitsReserved,
        expiresAt: res.Ok.expiresAt,
      };
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ppDeskPendingIntents'] });
    },
    onError: (e) => {
      toast.error('Could not lock your buy', { description: e.message });
    },
  });
}

export function useGetMyPendingBuyIntents() {
  const { actor } = usePonziMathSolActor();
  const { principal, walletType } = useWallet();
  return useQuery<BuyIntent[]>({
    queryKey: ['ppDeskPendingIntents', principal],
    queryFn: async () => {
      if (!actor) return [];
      return actor.getMyPendingBuyIntents();
    },
    enabled: walletType === 'siws' && !!actor && !!principal,
    refetchInterval: 10_000,
  });
}
```

- [ ] **Step 2: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors. (If the import path `../declarations/ponzi_math_sol/ponzi_math_sol.did` is wrong, use the path that `usePonziMathSolActor` / `backend.ts` uses for `PonziMathSolService` and import the types from there.)

- [ ] **Step 3: Commit** — `git add frontend/src/hooks/useBuyPpDesk.ts && git commit -m "feat(desk-fe): buyer hooks (quote, createBuyIntent, pending intents)"`

---

## Task 3: Buyer flyout (`BuyPpDeskFlyout.tsx`)

**Files:** Create `frontend/src/components/Shenanigans/BuyPpDeskFlyout.tsx`

- [ ] **Step 1: Implement the flyout** (quote → lock → pay → credited, reusing the deposit-to-side-pocket prompt):

```tsx
/** Founder's Allocation — buy loose PP with SOL from Charles's desk.
 *  Replaces BuySOLFlyout in the SIWS sidebar slot. Lifecycle:
 *  quote → lock (createBuyIntent) → pay (address+QR+countdown) → credited. */
import { useEffect, useMemo, useState } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, ArrowRight, X } from 'lucide-react';
import { useWallet } from '../../hooks/useWallet';
import { useAllowance, useApproveForDeposits, useDepositChips } from '../../hooks/useQueries';
import { useQuoteBuyPP, useCreateBuyIntent, useGetMyPendingBuyIntents, type CreateBuyIntentResult } from '../../hooks/useBuyPpDesk';
import { formatSOL, parseSOL, LAMPORTS_PER_SOL } from '../../solana/lamports';
import { formatPpUnits, effectiveRatePer0_1Sol, formatCountdown } from '../../lib/ppDesk';
import LoadingSpinner from '../LoadingSpinner';

interface Props { onClose?: () => void; variant?: 'widget' | 'sheet'; }

export default function BuyPpDeskFlyout({ onClose, variant = 'widget' }: Props) {
  const { isConnected, principal } = useWallet();
  const [solInput, setSolInput] = useState('');
  const [locked, setLocked] = useState<CreateBuyIntentResult | null>(null);
  const [copied, setCopied] = useState(false);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const [justBoughtPpUnits, setJustBoughtPpUnits] = useState<bigint>(0n);

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);

  const { data: quote, isFetching: quoteFetching } = useQuoteBuyPP(locked ? 0n : lamports);
  const createIntent = useCreateBuyIntent();
  const { data: pendingIntents } = useGetMyPendingBuyIntents();

  // Tick once a second while a lock is active, for the countdown.
  useEffect(() => {
    if (!locked) return;
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [locked]);

  // Settlement detection: our locked intent leaving the pending list = filled.
  useEffect(() => {
    if (!locked || !pendingIntents) return;
    const stillOpen = pendingIntents.some((bi) => bi.id === locked.intentId);
    const expiresMs = Number(locked.expiresAt / 1_000_000n);
    if (!stillOpen) {
      // Filled (it left the list before expiry). Show success + deposit prompt.
      if (Date.now() < expiresMs + 5_000) setJustBoughtPpUnits(locked.ppUnitsReserved);
      setLocked(null);
      setSolInput('');
    }
  }, [pendingIntents, locked]);

  // Reused deposit-to-side-pocket (same as BuyPPFlyout).
  const { data: allowance } = useAllowance();
  const approveDeposits = useApproveForDeposits();
  const depositChips = useDepositChips();
  const depositPending = approveDeposits.isPending || depositChips.isPending;
  const handleDeposit = async () => {
    try {
      const have = (allowance?.allowance ?? 0n) >= justBoughtPpUnits;
      if (!have) await approveDeposits.mutateAsync(undefined);
      await depositChips.mutateAsync(Number(justBoughtPpUnits) / 1e8);
      setJustBoughtPpUnits(0n);
    } catch { /* toasts surface failure; keep prompt for retry */ }
  };

  const handleLock = async () => {
    if (lamports <= 0n) return;
    const res = await createIntent.mutateAsync(lamports).catch(() => null);
    if (res) { setLocked(res); setNowMs(Date.now()); }
  };

  const handleCopy = async () => {
    if (!locked) return;
    await navigator.clipboard.writeText(locked.depositAddress);
    setCopied(true); setTimeout(() => setCopied(false), 1500);
  };

  const msRemaining = locked ? Number(locked.expiresAt / 1_000_000n) - nowMs : 0;
  const expired = locked != null && msRemaining <= 0;
  const qrPayload = locked ? `solana:${locked.depositAddress}?amount=${formatSOL(locked.quotedLamportsFallback ?? 0n)}` : null;

  const canLock = isConnected && !!principal && lamports > 0n && !!quote && quote.ppUnitsOut > 0n && !createIntent.isPending;

  return (
    <div className={variant === 'sheet' ? 'p-5' : ''}>
      <h2 className="font-display text-lg mc-text-primary mb-2">Founder's Allocation</h2>
      <div className="mc-status-amber p-3 mb-4 text-xs font-bold">
        ⚠️ DEVNET SOL ONLY — the canister polls Solana devnet. Mainnet SOL sent here is lost.
      </div>

      {/* Post-buy deposit-to-side-pocket prompt */}
      {justBoughtPpUnits > 0n && (
        <div className="mc-buy-pp-deposit-prompt">
          <div className="flex items-start gap-2 mb-2">
            <Check className="h-4 w-4 mc-text-gold flex-shrink-0 mt-0.5" />
            <div className="flex-1 leading-tight">
              <div className="text-sm font-bold mc-text-primary">Bought {formatPpUnits(justBoughtPpUnits)} PP</div>
              <div className="text-[11px] mc-text-muted mt-0.5">Deposit to your side pocket to use them for shenanigans.</div>
            </div>
            <button type="button" onClick={() => setJustBoughtPpUnits(0n)} className="p-1 rounded hover:bg-white/5 mc-text-muted -mt-1 -mr-1" aria-label="Keep in wallet"><X className="h-3 w-3" /></button>
          </div>
          <button type="button" onClick={handleDeposit} disabled={depositPending} className="mc-buy-pp-deposit-button">
            {depositPending ? <LoadingSpinner /> : (<><span>Deposit {formatPpUnits(justBoughtPpUnits)} PP → Side Pocket</span><ArrowRight className="h-4 w-4" /></>)}
          </button>
        </div>
      )}

      {!locked ? (
        <>
          <div className="mc-buy-pp-input-row">
            <input type="text" inputMode="decimal" placeholder="0.0" value={solInput}
              onChange={(e) => setSolInput(e.target.value)} className="mc-buy-pp-input font-mono" />
            <span className="mc-buy-pp-input-suffix">SOL</span>
          </div>
          <div className="mc-buy-pp-quote">
            <span className="text-[11px] mc-text-muted uppercase tracking-wider">You receive</span>
            <div className="mc-buy-pp-quote-amount">
              {lamports <= 0n ? <span className="mc-text-muted">—</span>
                : quote && quote.ppUnitsOut > 0n ? <>~{formatPpUnits(quote.ppUnitsOut)} <span className="text-sm mc-text-muted">PP</span></>
                : quoteFetching ? <span className="mc-text-muted">…</span>
                : <span className="mc-text-muted">Desk is out of stock</span>}
            </div>
            {quote && quote.ppUnitsOut > 0n && (
              <div className="flex items-center justify-between text-[10px] mc-text-muted mt-1">
                <span>≈ {effectiveRatePer0_1Sol(quote.ppUnitsOut, lamports)} PP / 0.1 SOL</span>
                {quote.cappedByInventory && <span className="mc-text-gold">limited stock</span>}
              </div>
            )}
          </div>
          <button type="button" onClick={handleLock} disabled={!canLock} className="mc-buy-pp-button">
            {createIntent.isPending ? <LoadingSpinner /> : <span>Lock & Buy</span>}
          </button>
          {!isConnected && <div className="text-[10px] mc-text-muted mt-2 text-center">Connect a wallet to buy</div>}
        </>
      ) : (
        <div className="mc-card p-3 space-y-2">
          <div className="text-xs mc-text-muted">
            Send <span className="mc-text-gold font-bold">{formatSOL(locked.quotedLamportsFallback ?? 0n)} SOL</span> to receive <span className="mc-text-gold font-bold">{formatPpUnits(locked.ppUnitsReserved)} PP</span>.
          </div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={locked.depositAddress}>{locked.depositAddress}</code>
            <button onClick={handleCopy} className="mc-btn-secondary text-xs">{copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}</button>
          </div>
          {qrPayload && <div className="flex justify-center pt-2"><QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" /></div>}
          <div className={`text-center text-xs ${expired ? 'mc-text-danger' : 'mc-text-muted'}`}>
            {expired ? 'Quote expired — start over for a fresh price.' : `Reserved for ${formatCountdown(msRemaining)} · PP arrives ~1 min after you send.`}
          </div>
          <button type="button" onClick={() => { setLocked(null); setSolInput(''); }} className="mc-btn-secondary w-full">{expired ? 'Get a new quote' : 'Start over'}</button>
        </div>
      )}

      {onClose && <button onClick={onClose} className="mc-btn-secondary w-full mt-3">Close</button>}
    </div>
  );
}
```

- [ ] **Step 2: Fix the `quotedLamports` reference** — the flyout shows the quoted SOL on the pay screen. `createBuyIntent`'s result does NOT include the lamports, so capture it at lock time: change `CreateBuyIntentResult` usage by storing the input `lamports` alongside `locked`. Replace `locked.quotedLamportsFallback ?? 0n` (×3) by adding `quotedLamports: bigint` to the locked state: set `setLocked({ ...res, quotedLamports: lamports } as any)` is NOT allowed — instead widen local state:
  - Change `const [locked, setLocked] = useState<CreateBuyIntentResult | null>(null);` to `const [locked, setLocked] = useState<(CreateBuyIntentResult & { quotedLamports: bigint }) | null>(null);`
  - In `handleLock`: `if (res) { setLocked({ ...res, quotedLamports: lamports }); setNowMs(Date.now()); }`
  - Replace the three `locked.quotedLamportsFallback ?? 0n` with `locked.quotedLamports`.

- [ ] **Step 3: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors.

- [ ] **Step 4: Commit** — `git add frontend/src/components/Shenanigans/BuyPpDeskFlyout.tsx && git commit -m "feat(desk-fe): buyer flyout (quote/lock/pay/credited)"`

---

## Task 4: Wire the flyout into the sidebar slot

**Files:** Modify `frontend/src/components/Shenanigans/BuySOLWidget.tsx`, `frontend/src/components/Shenanigans/BuySOLFab.tsx`

- [ ] **Step 1: `BuySOLWidget.tsx`** — change the import and the rendered flyout:

```tsx
import BuyPpDeskFlyout from './BuyPpDeskFlyout';

export default function BuySOLWidget() {
  return (
    <div className="mc-buy-pp-widget">
      <BuyPpDeskFlyout variant="widget" />
    </div>
  );
}
```

- [ ] **Step 2: `BuySOLFab.tsx`** — read it first; it renders `BuySOLFlyout` inside a mobile sheet. Replace the `import BuySOLFlyout from './BuySOLFlyout'` with `import BuyPpDeskFlyout from './BuyPpDeskFlyout'`, and the `<BuySOLFlyout ... variant="sheet" .../>` usage with `<BuyPpDeskFlyout ... variant="sheet" .../>` (keep its existing `onClose`/sheet props unchanged). Do NOT modify `BuySOLFlyout.tsx` itself.

- [ ] **Step 3: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors.

- [ ] **Step 4: Preview render** — Run: `npm run dev`; in a browser, connect a SIWS (Phantom) wallet, open the Shenanigans page; the sidebar widget shows "Founder's Allocation" with the SOL input. With tiers stocked on devnet, entering SOL shows a live PP quote (anon query). (Lock/pay is manual e2e.)

- [ ] **Step 5: Commit** — `git add frontend/src/components/Shenanigans/BuySOLWidget.tsx frontend/src/components/Shenanigans/BuySOLFab.tsx && git commit -m "feat(desk-fe): point SIWS sidebar widget at the desk flyout"`

---

## Task 5: Admin hooks (`useDeskAdmin.ts`)

**Files:** Create `frontend/src/hooks/useDeskAdmin.ts`

- [ ] **Step 1: Implement admin reads + mutations.** The "Deposit PP" flow approves `ponzi_math_sol` as spender on `pp_ledger`, then calls `deskDepositInventory`.

```ts
/** Founder's Allocation desk — admin (Charles) hooks. Reads via the anon actor,
 *  mutations via the SIWS/II-authed actor; deposit also approves on pp_ledger. */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Principal } from '@dfinity/principal';
import { toast } from 'sonner';
import { useReadPonziMathSol } from './useReadPonziMathSol';
import { usePonziMathSolActor } from './usePonziMathSolActor';
import { useAuthPpLedger } from './usePpLedger';
import type { DeskTier } from '../declarations/ponzi_math_sol/ponzi_math_sol.did';

const PONZI_MATH_SOL_CANISTER_ID = 'spc6q-xyaaa-aaaac-qg2ma-cai';
const invalidate = (qc: ReturnType<typeof useQueryClient>) => {
  qc.invalidateQueries({ queryKey: ['deskTiers'] });
  qc.invalidateQueries({ queryKey: ['deskStats'] });
};

export function useDeskTiers() {
  const actor = useReadPonziMathSol();
  return useQuery<DeskTier[]>({ queryKey: ['deskTiers'], queryFn: () => actor.deskListTiers(), refetchInterval: 15_000 });
}

export function useDeskStats() {
  const actor = useReadPonziMathSol();
  return useQuery({ queryKey: ['deskStats'], queryFn: () => actor.deskStats(), refetchInterval: 15_000 });
}

function useDeskMutation<TArgs>(fn: (actor: NonNullable<ReturnType<typeof usePonziMathSolActor>['actor']>, args: TArgs) => Promise<{ Ok: unknown } | { Err: string }>) {
  const { actor } = usePonziMathSolActor();
  const qc = useQueryClient();
  return useMutation<unknown, Error, TArgs>({
    mutationFn: async (args: TArgs) => {
      if (!actor) throw new Error('Admin wallet not connected');
      const res = await fn(actor, args);
      if ('Err' in res) throw new Error(res.Err);
      return (res as { Ok: unknown }).Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Desk action failed', { description: e.message }),
  });
}

// rate/qty are already in PP-units (caller converts via lib/ppDesk).
export const useDeskAddTier = () => useDeskMutation<{ rateUnits: bigint; qtyUnits: bigint }>((a, { rateUnits, qtyUnits }) => a.deskAddTier(rateUnits, qtyUnits));
export const useDeskUpdateTier = () => useDeskMutation<{ index: bigint; rateUnits: bigint; qtyUnits: bigint }>((a, { index, rateUnits, qtyUnits }) => a.deskUpdateTier(index, rateUnits, qtyUnits));
export const useDeskRemoveTier = () => useDeskMutation<{ index: bigint }>((a, { index }) => a.deskRemoveTier(index));
export const useDeskWithdrawInventory = () => useDeskMutation<{ units: bigint; to: Principal }>((a, { units, to }) => a.deskWithdrawInventory(units, to));
export const useWithdrawDeskProceeds = () => useDeskMutation<{ toAddress: string }>((a, { toAddress }) => a.adminWithdrawDeskProceeds(toAddress));

/** Deposit PP into the desk escrow: approve ponzi_math_sol on pp_ledger, then deskDepositInventory. */
export function useDeskDepositInventory() {
  const { actor } = usePonziMathSolActor();
  const ledger = useAuthPpLedger();
  const qc = useQueryClient();
  return useMutation<bigint, Error, { units: bigint }>({
    mutationFn: async ({ units }) => {
      if (!actor || !ledger) throw new Error('Admin wallet not connected');
      const approve = await ledger.icrc2_approve({
        from_subaccount: [], spender: { owner: Principal.fromText(PONZI_MATH_SOL_CANISTER_ID), subaccount: [] },
        amount: units, expected_allowance: [], expires_at: [], fee: [], memo: [], created_at_time: [],
      });
      if ('Err' in approve) throw new Error('Approve failed: ' + JSON.stringify(approve.Err));
      const res = await actor.deskDepositInventory(units);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => invalidate(qc),
    onError: (e) => toast.error('Deposit failed', { description: e.message }),
  });
}
```

- [ ] **Step 2: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors. (If `icrc2_approve`'s arg type differs from the optional-field shape above, match the generated `ApproveArgs` in `frontend/src/declarations/pp_ledger`.)

- [ ] **Step 3: Commit** — `git add frontend/src/hooks/useDeskAdmin.ts && git commit -m "feat(desk-fe): admin desk hooks (tiers, inventory, proceeds)"`

---

## Task 6: Admin panel (`PpDeskPanel.tsx`)

**Files:** Create `frontend/src/components/PpDeskPanel.tsx`

- [ ] **Step 1: Implement the panel** (inventory & stats, tiers, cash out). Rate entered as whole PP-per-0.1-SOL; quantity as whole PP; both converted to units before the call.

```tsx
/** Founder's Allocation — Charles's-office admin panel (MVP): inventory, tiers, cash out. */
import { useState } from 'react';
import { Principal } from '@dfinity/principal';
import { toast } from 'sonner';
import { useWallet } from '../hooks/useWallet';
import { isCharles } from '../lib/charles';
import { useDeskTiers, useDeskStats, useDeskAddTier, useDeskUpdateTier, useDeskRemoveTier, useDeskDepositInventory, useDeskWithdrawInventory, useWithdrawDeskProceeds } from '../hooks/useDeskAdmin';
import { tierRateToUnits, unitsToTierRate, formatPpUnits } from '../lib/ppDesk';
import { wholePpToUnits } from '../hooks/usePpLedger';
import { formatSOL } from '../solana/lamports';
import LoadingSpinner from './LoadingSpinner';

function Field({ label, value, onChange, placeholder }: { label: string; value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <label className="block">
      <span className="block text-xs font-bold uppercase tracking-wider text-white/50 mb-1.5">{label}</span>
      <input className="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white placeholder-white/30 focus:outline-none focus:border-purple-500/50 font-body"
        value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} inputMode="decimal" />
    </label>
  );
}

export default function PpDeskPanel() {
  const { principal, solanaPubkey } = useWallet();
  const { data: tiers } = useDeskTiers();
  const { data: stats } = useDeskStats();
  const addTier = useDeskAddTier();
  const updateTier = useDeskUpdateTier();
  const removeTier = useDeskRemoveTier();
  const deposit = useDeskDepositInventory();
  const withdrawPp = useDeskWithdrawInventory();
  const withdrawProceeds = useWithdrawDeskProceeds();

  const [depositPp, setDepositPp] = useState('');
  const [withdrawPpAmt, setWithdrawPpAmt] = useState('');
  const [newRate, setNewRate] = useState('');
  const [newQty, setNewQty] = useState('');

  if (!principal || !isCharles(principal)) {
    return <div className="text-center py-12 mc-text-muted text-sm">Charles only.</div>;
  }

  const num = (s: string) => { const n = Number(s); return Number.isFinite(n) && n > 0 ? n : 0; };

  return (
    <div className="space-y-6">
      {/* Inventory & stats */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-3">Inventory & Stats</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-center mb-4">
          <div><div className="mc-label">Available</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.availableUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Reserved</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.reservedUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Sold</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.totalSoldUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Open buys</div><div className="font-bold mc-text-primary">{stats ? stats.openBuyIntents.toString() : '…'}</div></div>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div className="flex items-end gap-2">
            <div className="flex-1"><Field label="Deposit PP (whole)" value={depositPp} onChange={setDepositPp} placeholder="500000" /></div>
            <button className="mc-btn-primary" disabled={deposit.isPending || num(depositPp) === 0} onClick={() => deposit.mutate({ units: wholePpToUnits(Math.trunc(num(depositPp))) })}>{deposit.isPending ? <LoadingSpinner /> : 'Deposit'}</button>
          </div>
          <div className="flex items-end gap-2">
            <div className="flex-1"><Field label="Withdraw PP (whole)" value={withdrawPpAmt} onChange={setWithdrawPpAmt} placeholder="100000" /></div>
            <button className="mc-btn-secondary" disabled={withdrawPp.isPending || num(withdrawPpAmt) === 0} onClick={() => withdrawPp.mutate({ units: wholePpToUnits(Math.trunc(num(withdrawPpAmt))), to: Principal.fromText(principal) })}>{withdrawPp.isPending ? <LoadingSpinner /> : 'Withdraw'}</button>
          </div>
        </div>
      </div>

      {/* Tiers */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-1">Price Ladder</h3>
        <p className="text-[11px] mc-text-muted mb-3">Top = best deal. Rate is whole PP per 0.1 SOL; quantity is whole PP. Early buyers get the top tier.</p>
        <div className="space-y-2 mb-4">
          {(tiers ?? []).map((t, i) => (
            <div key={i} className="mc-card p-3 flex flex-wrap items-center gap-3 text-sm">
              <span className="mc-text-muted">#{i}</span>
              <span className="mc-text-primary font-bold">{unitsToTierRate(t.ratePpUnitsPer0_1Sol).toLocaleString()} PP / 0.1 SOL</span>
              <span className="mc-text-muted">{formatPpUnits(t.ppUnitsSold)} / {formatPpUnits(t.ppUnitsTotal)} sold · {formatPpUnits(t.ppUnitsReserved)} reserved</span>
              <button className="mc-btn-secondary text-xs ml-auto" disabled={removeTier.isPending} onClick={() => removeTier.mutate({ index: BigInt(i) })}>Remove</button>
            </div>
          ))}
          {(tiers ?? []).length === 0 && <div className="mc-text-muted text-sm">No tiers yet — add one below to open the desk.</div>}
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex-1 min-w-[140px]"><Field label="Rate (PP / 0.1 SOL)" value={newRate} onChange={setNewRate} placeholder="250000" /></div>
          <div className="flex-1 min-w-[140px]"><Field label="Quantity (PP)" value={newQty} onChange={setNewQty} placeholder="1000000" /></div>
          <button className="mc-btn-primary" disabled={addTier.isPending || num(newRate) === 0 || num(newQty) === 0}
            onClick={() => { addTier.mutate({ rateUnits: tierRateToUnits(num(newRate)), qtyUnits: wholePpToUnits(Math.trunc(num(newQty))) }); setNewRate(''); setNewQty(''); }}>
            {addTier.isPending ? <LoadingSpinner /> : 'Add tier'}
          </button>
        </div>
      </div>

      {/* Cash out */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-3">Cash Out</h3>
        <div className="flex items-center justify-between gap-3">
          <div><div className="mc-label">Accrued proceeds</div><div className="font-bold mc-text-gold">{stats ? formatSOL(stats.proceedsLamports) : '…'} SOL</div></div>
          <button className="mc-btn-primary" disabled={withdrawProceeds.isPending || !solanaPubkey || (stats?.proceedsLamports ?? 0n) === 0n}
            onClick={() => { if (!solanaPubkey) { toast.error('No Phantom address on this session'); return; } withdrawProceeds.mutate({ toAddress: solanaPubkey }); }}>
            {withdrawProceeds.isPending ? <LoadingSpinner /> : 'Withdraw SOL to Phantom'}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors. (If `useWallet` doesn't expose `solanaPubkey`, source the Phantom address the same way `useGetMyDepositAddress`/withdrawal code does — see `useWallet.tsx`.)

- [ ] **Step 3: Commit** — `git add frontend/src/components/PpDeskPanel.tsx && git commit -m "feat(desk-fe): Charles's-office PP Desk admin panel (MVP)"`

---

## Task 7: Wire the "Desk" admin tab

**Files:** Modify `frontend/src/App.tsx`

- [ ] **Step 1: Import + widen the tab state.**
  - Add `import PpDeskPanel from './components/PpDeskPanel';` near the `CharlesGodView`/`ShenanigansAdminPanel` imports (`:15-16`).
  - Change `useState<'godView' | 'tuning'>('godView')` (`:295`) to `useState<'godView' | 'tuning' | 'desk'>('godView')`.

- [ ] **Step 2: Add the tab button** — after the "Tuning" button (`App.tsx:751-759`), add:

```tsx
                      <button
                        onClick={() => setAdminPanelTab('desk')}
                        className={`px-4 py-1.5 rounded-full text-xs font-bold transition-all ${
                          adminPanelTab === 'desk'
                            ? 'bg-[var(--mc-gold)]/20 mc-text-gold border border-[var(--mc-gold)]/40'
                            : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
                        }`}
                      >
                        Desk
                      </button>
```

- [ ] **Step 3: Update the render branch** (`App.tsx:763`) from the ternary to cover all three:

```tsx
                  {adminPanelTab === 'godView' ? <CharlesGodView /> : adminPanelTab === 'tuning' ? <ShenanigansAdminPanel /> : <PpDeskPanel />}
```

- [ ] **Step 4: Type-check** — Run: `npx tsc --noEmit`. Expected: no errors.

- [ ] **Step 5: Preview** — Run: `npm run dev`; as a Charles principal, open Charles's Office → "Desk" tab renders the panel (stats load via anon query; tier add/deposit/withdraw are authed and manual).

- [ ] **Step 6: Commit** — `git add frontend/src/App.tsx && git commit -m "feat(desk-fe): add Desk tab to Charles's office"`

---

## Task 8: Full verification + build

- [ ] **Step 1: Unit tests** — Run: `npm test`. Expected: PASS, including the `ppDesk` suite (5 tests) and the pre-existing suites.
- [ ] **Step 2: Type + build** — Run: `npm run build` (`tsc && vite build`). Expected: clean type-check + successful production build.
- [ ] **Step 3: Preview smoke** — Run: `npm run dev`; verify (a) SIWS sidebar shows "Founder's Allocation" with a working live quote against stocked devnet tiers, and (b) the Charles "Desk" tab renders stats/tiers. Note any console errors.
- [ ] **Step 4: Commit anything outstanding** — if `npm run build` produced declaration/format tweaks, `git add -p` the relevant files only and commit `chore(desk-fe): build fixes`. Do NOT stage unrelated user WIP (`README.md`, `scripts/`, `spec.md`, etc.).

---

## Self-review

- **Spec coverage:** buyer hooks (T2) ✓; buyer flyout quote→lock→pay→credited + side-pocket reuse + DEVNET banner + capped nudge (T3) ✓; slot replacement leaving `BuySOLFlyout` intact (T4) ✓; admin hooks incl. approve→deposit (T5) ✓; admin panel tiers/inventory/stats/proceeds (T6) ✓; "Desk" tab (T7) ✓; pure-helper TDD + rate conversion `N·1e8` (T1) ✓; vitest+tsc+build+preview testing reality (T1,T8) ✓. Deferred items (intent table, refund UI, reorder) intentionally absent.
- **Type consistency:** `ppUnitsOut`/`ppUnitsReserved`/`ratePpUnitsPer0_1Sol`/`quotedLamports`/`expiresAt` (all `bigint`) used consistently; `CreateBuyIntentResult` widened with `quotedLamports` in T3 Step 2; `tierRateToUnits`/`unitsToTierRate`/`formatPpUnits`/`effectiveRatePer0_1Sol`/`formatCountdown` defined in T1 and used identically in T3/T6; `wholePpToUnits` imported from `usePpLedger`.
- **Placeholder scan:** none — every step has full code or an exact command. The two "if the generated type differs, match it" notes (T2 import path, T5 `ApproveArgs`) are real verify-against-generated-candid instructions, not placeholders. T3 Step 2 is an explicit fix-up of the `quotedLamports` capture, kept as its own step so the engineer doesn't miss it.

## Frontend follow-up (deferred)
Open-buy-intent inspection table (`adminGetAllBuyIntents`), refund UI (`adminRefundDeskSol`), drag-reorder (`deskReorderTiers`) — a later plan once the MVP desk is exercised.
