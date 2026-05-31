# SOL One-Click Deposit + Triggered Scan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make SIWS plan deposits feel native — one click → Phantom popup → approve → position opens in ~seconds — backed by a user-triggered canister scan, with the existing 60s timer as the only backstop.

**Architecture:** A new `requireAuthenticated`, cooldown'd, open-intent-gated `pokeMyDeposit` method on `ponzi_math_sol` scans only the caller's own deposit address (zero RPC when nothing is pending). The frontend `SolInvestPanel` is reworked: "Deposit with Phantom" prepares the intent, builds the exact `SystemProgram.transfer`, has the connected wallet sign+send it, confirms, then pokes — and falls back to the manual address/QR (with a fixed copy button + "Check now") if the wallet path fails. Plus the count-up animation from the ICP panel.

**Tech Stack:** Motoko (`ponzi_math_sol`), React + TypeScript, `@solana/web3.js` (v1.98), `@solana/wallet-adapter-*`, React Query, vitest.

**Repo layout note:** `package.json`/`tsconfig`/`dfx.json` live at the **repo root**; frontend source is under `frontend/src/`. Run all commands from the repo root. There is unrelated uncommitted user WIP in the tree — **stage only the files each task lists; never `git add -A`.**

**Spec:** `docs/superpowers/specs/2026-05-31-sol-one-click-deposit-design.md`

---

### Task 1: Backend `pokeMyDeposit` (`ponzi_math_sol`) + regenerate declarations

Adds a user-scoped detection trigger and regenerates the frontend candid bindings so the hook (Task 2) can call it. **Code only — the deploy is operator-gated (see end of task).** No Motoko unit-test harness exists, so this task verifies via `dfx build` + careful logic.

**Files:**
- Modify: `ponzi_math_sol/main.mo`
- Modify (generated): `frontend/src/declarations/ponzi_math_sol/*`

- [ ] **Step 1: Add the cooldown constant + per-caller timestamp map.** In `ponzi_math_sol/main.mo`, find this exact line (~:369):

```motoko
    transient var detectionInProgress : Bool = false;
```
and replace it with:
```motoko
    transient var detectionInProgress : Bool = false;

    // Per-caller cooldown for user-triggered pokeMyDeposit scans (5s). Transient:
    // resetting on upgrade is harmless (worst case one extra allowed poke).
    transient let POKE_COOLDOWN_NS : Int = 5_000_000_000;
    transient var pokeTimestamps = principalMapNat.empty<Int>();
```

- [ ] **Step 2: Add the `pokeMyDeposit` method.** Find this exact line (the start of the timer-callback comment, ~:3074):

```motoko
    /// Timer callback — runs the cheap open-intents pass. Guarded against
```
and insert the following method **immediately before** it (so the new method sits between `runDepositDetection` and `detectionTick`):

```motoko
    /// User-triggered detection for the CALLER's own deposit address only.
    /// Lets the frontend get a near-instant credit right after the user's
    /// wallet confirms the SOL transfer, instead of waiting for the 60s timer.
    /// Abuse-bounded: makes ZERO RPC outcalls unless the caller has an open,
    /// unexpired intent (deposit or buy), and is rate-limited to once per
    /// POKE_COOLDOWN_NS per caller. Shares the detectionInProgress guard with
    /// the auto-timer so the two never run concurrently.
    public shared ({ caller }) func pokeMyDeposit() : async { #Ok : Nat; #Err : Text } {
        requireAuthenticated(caller);
        if (not bootstrapped) { return #Err("Not bootstrapped") };

        let now = Time.now();

        // Per-caller cooldown — cheap, before any work.
        switch (principalMapNat.get(pokeTimestamps, caller)) {
            case (?last) {
                if (now - last < POKE_COOLDOWN_NS) {
                    return #Err("Please wait a few seconds before checking again");
                };
            };
            case (null) {};
        };

        // Open-intent gate: only scan if the caller has an open, unexpired
        // intent. Otherwise return #Ok(0) with ZERO RPC outcalls.
        var hasOpen = false;
        for (intent in natMap.vals(pendingIntents)) {
            if (intent.principal == caller and not intent.fulfilled and now <= intent.expiresAt) {
                hasOpen := true;
            };
        };
        if (not hasOpen) {
            for (bi in natMap.vals(pendingBuyIntents)) {
                if (bi.principal == caller and not bi.fulfilled and now <= bi.expiresAt) {
                    hasOpen := true;
                };
            };
        };
        if (not hasOpen) { return #Ok(0) };

        if (detectionInProgress) { return #Err("Detection busy — try again shortly") };

        let addr = switch (principalMapNat.get(depositAddresses, caller)) {
            case (?a) { a };
            case (null) { return #Ok(0) };
        };

        pokeTimestamps := principalMapNat.put(pokeTimestamps, caller, now);
        detectionInProgress := true;
        try {
            let credits = await scanAndCredit(addr, caller);
            #Ok(credits);
        } catch (e) {
            #Err("Scan failed: " # Error.message(e));
        } finally {
            detectionInProgress := false;
        };
    };

```

- [ ] **Step 3: Build the canister to type-check the Motoko**

Run: `dfx build ponzi_math_sol`
Expected: PASS — compiles with only the pre-existing warnings (deprecated `ExperimentalCycles.add`, unused identifiers, M0155 Nat-trap warnings). No errors. (If `dfx` is unavailable, report NEEDS_CONTEXT — do not guess.)

- [ ] **Step 4: Regenerate the frontend candid bindings**

Run: `dfx generate ponzi_math_sol`
Expected: regenerates `frontend/src/declarations/ponzi_math_sol/*` — the diff should add a `pokeMyDeposit : () -> (variant { Ok : nat; Err : text })` method to `ponzi_math_sol.did`, `.did.d.ts`, `.did.js`, and the service type. If `dfx generate` touches any files outside `frontend/src/declarations/ponzi_math_sol/`, do NOT stage them.

- [ ] **Step 5: Commit**

```bash
git add ponzi_math_sol/main.mo frontend/src/declarations/ponzi_math_sol
git commit -m "feat(sol-one-click): add user-scoped pokeMyDeposit detection trigger" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

> **Deploy (operator-gated — DO NOT run without the user's explicit per-deploy permission).** After approval, with the `CharlesPonzi` identity, restoring `rumi_identity` after, and verifying `getActiveGameCount`/`getTotalDeposits` are preserved:
> ```
> dfx deploy ponzi_math_sol --network ic --mode upgrade --wasm-memory-persistence keep --yes --argument '(record { backendPrincipal = principal "5zxxg-tyaaa-aaaac-qeckq-cai"; testAdmin = principal "6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe"; solTreasuryAddress = "5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2"; solRpcProvider = variant { devnet }; keyId = record { algorithm = variant { ed25519 }; name = "key_1" } })'
> ```
> No state-shape migration (method + constant only). Must land before any eventual blackhole.

---

### Task 2: `usePokeMyDeposit` hook

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

- [ ] **Step 1: Add the hook.** In `frontend/src/hooks/useQueries.ts`, find this exact line (~:2424, the comment above `useWithdrawSolGameEarnings`):

```ts
// Cash out a SOL position to the user's OWN Solana wallet. Simple positions
```
and insert the following **immediately before** it:

```ts
// User-triggered immediate scan of the caller's own deposit address. Called
// by SolInvestPanel right after the wallet confirms the SOL transfer so the
// position opens in ~seconds instead of waiting for the 60s detection timer.
// Backend is abuse-bounded (self-only, cooldown'd, open-intent-gated).
export function usePokeMyDeposit() {
  const { actor } = usePonziMathSolActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      if (!actor) throw new Error('SOL actor not ready');
      const result = await actor.pokeMyDeposit();
      if ('Err' in result) throw new Error(result.Err);
      return result.Ok; // bigint — number of games credited (0 or more)
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['mySolPendingIntents'] });
      queryClient.invalidateQueries({ queryKey: ['userSolGames'] });
    },
  });
}

```

- [ ] **Step 2: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS. (`usePonziMathSolActor`, `useMutation`, `useQueryClient` are already imported in this file; `actor.pokeMyDeposit` exists because Task 1 regenerated the declarations.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "feat(sol-one-click): add usePokeMyDeposit hook" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `sendSolDeposit` module (`buildSolTransferTx` TDD + wallet send)

Pure transfer-builder (unit-tested) + the wallet-re-acquisition/send/confirm wrapper (integration, verified manually).

**Files:**
- Create: `frontend/src/solana/sendSolDeposit.ts`
- Test: `frontend/src/solana/sendSolDeposit.test.ts`

- [ ] **Step 1: Write the failing test** — create `frontend/src/solana/sendSolDeposit.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { PublicKey, SystemInstruction } from '@solana/web3.js';
import { buildSolTransferTx } from './sendSolDeposit';

const FROM = new PublicKey('So11111111111111111111111111111111111111112'); // wSOL mint (valid base58 pubkey)
const TO = new PublicKey('Vote111111111111111111111111111111111111111');   // vote program (valid base58 pubkey)
const BLOCKHASH = 'GfVcyD4kkTrj4bKc7WA9sZCin9JDbdT4Zkd3EZD3GcVz';

describe('buildSolTransferTx', () => {
  it('builds a single System transfer of the exact lamports to the exact recipient', () => {
    const tx = buildSolTransferTx({ fromPubkey: FROM, toPubkey: TO, lamports: 11_000_000n, recentBlockhash: BLOCKHASH });
    expect(tx.instructions).toHaveLength(1);
    const decoded = SystemInstruction.decodeTransfer(tx.instructions[0]);
    expect(decoded.fromPubkey.toBase58()).toBe(FROM.toBase58());
    expect(decoded.toPubkey.toBase58()).toBe(TO.toBase58());
    expect(BigInt(decoded.lamports)).toBe(11_000_000n);
    expect(tx.feePayer?.toBase58()).toBe(FROM.toBase58());
    expect(tx.recentBlockhash).toBe(BLOCKHASH);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run frontend/src/solana/sendSolDeposit.test.ts`
Expected: FAIL — cannot find module `./sendSolDeposit` / `buildSolTransferTx` is not a function.

- [ ] **Step 3: Implement the module** — create `frontend/src/solana/sendSolDeposit.ts`:

```ts
import { Connection, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';

// Devnet for now; flips to a mainnet endpoint at the M3 cutover.
export const SOLANA_RPC_ENDPOINT = 'https://api.devnet.solana.com';

// Pure: build a single-instruction SOL transfer. lamports is passed as Number
// at the SystemProgram boundary — exact for deposit-sized amounts (< 2^53) and
// type-safe across @solana/web3.js versions.
export function buildSolTransferTx(params: {
  fromPubkey: PublicKey;
  toPubkey: PublicKey;
  lamports: bigint;
  recentBlockhash: string;
}): Transaction {
  const tx = new Transaction({ feePayer: params.fromPubkey, recentBlockhash: params.recentBlockhash });
  tx.add(
    SystemProgram.transfer({
      fromPubkey: params.fromPubkey,
      toPubkey: params.toPubkey,
      lamports: Number(params.lamports),
    }),
  );
  return tx;
}

// Re-acquire the connected wallet at send time. The SIWS session persists only
// the delegation identity + pubkey (see useWallet.tsx), not a live adapter, so
// we re-instantiate and silently reconnect (already authorized for this origin)
// — mirrors connectSiws's adapter selection. Verifies the wallet matches the
// session pubkey so we never send from an unexpected account.
async function acquireConnectedAdapter(expectedPubkey: string) {
  const base = await import('@solana/wallet-adapter-base');
  const wallets = await import('@solana/wallet-adapter-wallets');
  const adapters = [new wallets.PhantomWalletAdapter(), new wallets.SolflareWalletAdapter()];
  const adapter = adapters.find((a) => a.readyState === base.WalletReadyState.Installed);
  if (!adapter) {
    throw new Error('No Solana wallet detected. Open Phantom or Solflare and try again.');
  }
  if (!adapter.connected) {
    await adapter.connect();
  }
  if (!adapter.publicKey || adapter.publicKey.toBase58() !== expectedPubkey) {
    throw new Error('Connected wallet does not match your session — reconnect your wallet and retry.');
  }
  return adapter;
}

// Build + sign + send + confirm the exact deposit transfer via the connected
// wallet. Returns the confirmed transaction signature. Throws on rejection /
// failure (caller falls back to the manual deposit-address flow).
export async function sendSolDeposit(params: {
  toAddress: string;
  lamports: bigint;
  expectedPubkey: string;
}): Promise<string> {
  const adapter = await acquireConnectedAdapter(params.expectedPubkey);
  const connection = new Connection(SOLANA_RPC_ENDPOINT, 'confirmed');
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash('confirmed');
  const tx = buildSolTransferTx({
    fromPubkey: new PublicKey(params.expectedPubkey),
    toPubkey: new PublicKey(params.toAddress),
    lamports: params.lamports,
    recentBlockhash: blockhash,
  });
  const signature = await adapter.sendTransaction(tx, connection);
  await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, 'confirmed');
  return signature;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run frontend/src/solana/sendSolDeposit.test.ts`
Expected: PASS (1 test).

- [ ] **Step 5: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/solana/sendSolDeposit.ts frontend/src/solana/sendSolDeposit.test.ts
git commit -m "feat(sol-one-click): add sendSolDeposit (build + wallet send + confirm)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Rework `SolInvestPanel` (one-click + manual fallback + copy fix + animation)

Full rewrite of the component into a flow state machine. No component-test harness — verified by `tsc` + `build` here and preview/manual later.

**Files:**
- Modify (replace contents): `frontend/src/components/SolInvestPanel.tsx`

- [ ] **Step 1: Replace the file** — overwrite `frontend/src/components/SolInvestPanel.tsx` with:

```tsx
import { useState, useEffect, useMemo, useRef } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, BarChart3, TrendingUp, Dices, Loader2 } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyPendingSolIntents,
  useGetUserSolGames,
  usePokeMyDeposit,
  calculateSimpleROI,
  calculateCompoundingROI,
  getDailyRate,
  getPlanDays,
} from '../hooks/useQueries';
import { usePonziMathSolActor } from '../hooks/usePonziMathSolActor';
import { useWallet } from '../hooks/useWallet';
import { useCountUp } from '../hooks/useCountUp';
import { formatSOL, formatSolFloat, parseSOL, LAMPORTS_PER_SOL } from '../solana/lamports';
import { sendSolDeposit } from '../solana/sendSolDeposit';
import { COVER_CHARGE_RATE, MIN_DEPOSIT_SOL, pct } from '../lib/gameConstants';
import { investPlanToSolGamePlan, ppPerSolForPlan } from '../lib/solPlanMapping';

const MIN_LAMPORTS = parseSOL(String(MIN_DEPOSIT_SOL));

type Flow =
  | { kind: 'input' }
  | { kind: 'awaitingWallet' }
  | { kind: 'opening'; lamports: bigint; baselineGames: number }
  | { kind: 'manual'; depositAddress: string; lamports: bigint; baselineGames: number; note?: string }
  | { kind: 'opened' };

interface SolInvestPanelProps {
  planId: string;
  onNavigateToProfitCenter?: () => void;
}

function friendlyWalletError(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/reject|denied|cancel/i.test(msg)) return 'Wallet request was cancelled.';
  return msg || 'Could not complete the transfer from your wallet.';
}

export default function SolInvestPanel({ planId, onNavigateToProfitCenter }: SolInvestPanelProps) {
  const { actor } = usePonziMathSolActor();
  const { solanaPubkey } = useWallet();
  const prepareMut = usePrepareSolDeposit();
  const pokeMut = usePokeMyDeposit();
  const { data: pendingIntents } = useGetMyPendingSolIntents();
  const { data: solGames } = useGetUserSolGames();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [flow, setFlow] = useState<Flow>({ kind: 'input' });

  const isCompounding = planId === '15-day-compounding' || planId === '30-day-compounding';
  const days = getPlanDays(planId);
  const ppPerSol = ppPerSolForPlan(planId);

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);

  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const belowMin = lamports > 0n && lamports < MIN_LAMPORTS;
  const canDeposit = !!actor && !!solanaPubkey && lamports >= MIN_LAMPORTS && !prepareMut.isPending;

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

  // Count-up animation (matches the ICP panel). Reset token bumps when the
  // amount/plan changes; bumped in an effect (never mutated during render).
  const roiResetToken = useRef(0);
  const prevKey = useRef('');
  useEffect(() => {
    const key = `${lamports.toString()}-${planId}`;
    if (key !== prevKey.current) { roiResetToken.current += 1; prevKey.current = key; }
  }, [lamports, planId]);
  const animatedReturn = useCountUp(roi?.totalReturn || 0, 800, roiResetToken.current);
  const animatedPP = useCountUp(projectedPP, 800, roiResetToken.current);

  // Credit detection: while opening/manual, a new SOL game beyond the baseline
  // captured at entry means the position opened.
  useEffect(() => {
    if ((flow.kind === 'opening' || flow.kind === 'manual') && solGames) {
      if (solGames.length > flow.baselineGames) setFlow({ kind: 'opened' });
    }
  }, [flow, solGames]);

  const handleCopy = async (text: string) => {
    let ok = false;
    try {
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); ok = true; }
    } catch { ok = false; }
    if (!ok) {
      try {
        const ta = document.createElement('textarea');
        ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
        document.body.appendChild(ta); ta.focus(); ta.select();
        ok = document.execCommand('copy');
        document.body.removeChild(ta);
      } catch { ok = false; }
    }
    if (ok) { setCopied(true); setTimeout(() => setCopied(false), 1500); }
  };

  const handleOneClick = async () => {
    if (!canDeposit || !solanaPubkey) return;
    setFlow({ kind: 'awaitingWallet' });
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ plan: investPlanToSolGamePlan(planId), expectedAmountLamports: lamports });
    } catch {
      setFlow({ kind: 'input' }); // prepareMut.isError surfaces the message in the input view
      return;
    }
    const baselineGames = solGames?.length ?? 0;
    try {
      await sendSolDeposit({ toAddress: prepared.depositAddress, lamports, expectedPubkey: solanaPubkey });
      try { await pokeMut.mutateAsync(); } catch { /* best-effort; the 60s timer is the backstop */ }
      setFlow({ kind: 'opening', lamports, baselineGames });
    } catch (e) {
      setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, baselineGames, note: friendlyWalletError(e) });
    }
  };

  const handleManual = async () => {
    if (!canDeposit) return;
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ plan: investPlanToSolGamePlan(planId), expectedAmountLamports: lamports });
    } catch { return; }
    setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, baselineGames: solGames?.length ?? 0 });
  };

  const handleCheckNow = () => { pokeMut.mutateAsync().catch(() => {}); };
  const handleStartOver = () => { setFlow({ kind: 'input' }); prepareMut.reset(); pokeMut.reset(); };

  if (!actor) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting your Solana session…</p>
      </div>
    );
  }

  const Devnet = (
    <div className="mc-status-amber p-3 text-center text-xs font-bold">
      <AlertTriangle className="h-4 w-4 inline mr-1" /> DEVNET — uses devnet SOL only. This position is funded on Solana devnet.
    </div>
  );

  if (flow.kind === 'opened') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <div className="font-display text-xl mc-text-primary">You're In.</div>
          <p className="text-sm mc-text-dim">Your position is open and earning.</p>
          <div className="flex gap-3 justify-center pt-1">
            <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">Open another</button>
            <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-primary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2">
              <TrendingUp className="h-4 w-4" /> Go to Profit Center
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (flow.kind === 'awaitingWallet') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
          <p className="text-sm mc-text-primary font-bold">Confirm the transfer in your wallet…</p>
          <p className="text-xs mc-text-dim">Approve the {formatSOL(lamports)} SOL transfer in Phantom.</p>
        </div>
      </div>
    );
  }

  if (flow.kind === 'opening') {
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
          <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
          <p className="text-sm mc-text-primary font-bold">Opening your position…</p>
          <p className="text-xs mc-text-dim">Your {formatSOL(flow.lamports)} SOL transfer is confirmed — the position usually opens within a few seconds (up to a minute).</p>
          <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-secondary px-5 py-2 rounded-full text-sm inline-flex items-center gap-2 mt-2">
            <TrendingUp className="h-4 w-4" /> Go to Profit Center
          </button>
        </div>
      </div>
    );
  }

  if (flow.kind === 'manual') {
    const qrPayload = `solana:${flow.depositAddress}?amount=${formatSOL(flow.lamports)}`;
    return (
      <div className="space-y-6">
        {Devnet}
        <div className="mc-card p-4 space-y-3 max-w-md mx-auto">
          {flow.note && (
            <div className="mc-status-red p-2 text-xs text-center">{flow.note} Send manually below — your position will still open.</div>
          )}
          <div className="text-center">
            <div className="mc-label">Send exactly</div>
            <div className="text-2xl font-bold mc-text-gold">{formatSOL(flow.lamports)} SOL</div>
            <div className="text-xs mc-text-dim mt-1">devnet SOL from your wallet — the position opens automatically within ~a minute.</div>
          </div>
          <div className="mc-label">Deposit address</div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={flow.depositAddress}>{flow.depositAddress}</code>
            <button onClick={() => handleCopy(flow.depositAddress)} className="mc-btn-secondary text-xs">
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>
          <div className="flex justify-center pt-2">
            <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
          </div>
          {pendingIntents && pendingIntents.length > 0 && (
            <div className="text-[10px] mc-text-muted text-center">{pendingIntents.length} pending deposit{pendingIntents.length === 1 ? '' : 's'} awaiting confirmation</div>
          )}
          <div className="flex gap-2 justify-center pt-2 flex-wrap">
            <button onClick={handleCheckNow} disabled={pokeMut.isPending} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">
              {pokeMut.isPending ? 'Checking…' : 'Check now'}
            </button>
            <button onClick={handleStartOver} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">Start over</button>
            <button onClick={() => onNavigateToProfitCenter?.()} className="mc-btn-primary px-4 py-2 rounded-full text-sm inline-flex items-center gap-2">
              <TrendingUp className="h-4 w-4" /> Profit Center
            </button>
          </div>
        </div>
      </div>
    );
  }

  // input (default)
  return (
    <div className="space-y-6">
      {Devnet}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div>
          <div className="flex justify-between items-center mb-2">
            <span className="text-xs font-bold mc-text-primary">Amount</span>
          </div>
          <div className="flex gap-2">
            <button onClick={() => setSolInput(String(MIN_DEPOSIT_SOL))} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap">MIN</button>
            <input type="text" inputMode="decimal" value={solInput} onChange={(e) => setSolInput(e.target.value)} placeholder={`Min: ${MIN_DEPOSIT_SOL} SOL`} className="mc-input flex-1 text-center text-lg font-mono" />
          </div>
          {belowMin && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum deposit is {MIN_DEPOSIT_SOL} SOL</div>)}
          {!solanaPubkey && (<div className="mt-2 text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Reconnect your Solana wallet to deposit.</div>)}

          <div className="mc-status-red p-3 text-center text-sm font-bold mt-3">
            <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
            <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
          </div>

          <button onClick={handleOneClick} disabled={!canDeposit} className={`w-full py-3 mt-3 text-sm font-bold rounded-xl transition-all mc-btn-primary inline-flex items-center justify-center gap-2 ${canDeposit ? 'pulse' : ''}`}>
            {prepareMut.isPending ? 'Starting…' : <><Dices className="h-5 w-5" /> DEPOSIT WITH PHANTOM</>}
          </button>

          {prepareMut.isError && (<p className="text-xs mc-text-danger mt-2 text-center">{(prepareMut.error as Error).message}</p>)}

          <button onClick={handleManual} disabled={!canDeposit} className="w-full text-xs mc-text-muted hover:mc-text-primary transition-colors mt-2 underline">
            or get a deposit address to send manually
          </button>
        </div>

        <div>
          {roi ? (
            <div>
              <div className="text-center mb-3"><span className="text-xs font-bold mc-text-primary">Expected ROI (if plan matures)</span></div>
              <div className="mc-card p-4 space-y-3">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="mc-label">{isCompounding ? 'Compounded Interest' : 'Interest Payout'}</div>
                    <div className={`text-xl font-bold mc-roi-pop ${roiColor}`}>{formatSolFloat(animatedReturn)} SOL</div>
                    <div className={`text-xs opacity-70 ${roiColor}`}>
                      {isCompounding ? `${roi.roiPercent.toFixed(1)}% ROI` : `${(roi.totalReturn / net).toFixed(2)}x ROI (${roi.roiPercent.toFixed(0)}%)`}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="mc-label">Ponzi Points</div>
                    <div className="text-xl font-bold mc-text-purple mc-glow-purple mc-roi-pop">{Math.round(animatedPP).toLocaleString()}</div>
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
                    <div className="flex items-center gap-1.5"><TrendingUp className="h-3.5 w-3.5 mc-text-cyan" /><span className="text-xs mc-text-dim">Daily earnings</span></div>
                    <span className="text-sm font-bold mc-text-cyan">{formatSolFloat(dailyEarnings)} SOL/day</span>
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="h-full flex items-center justify-center text-center">
              <div><BarChart3 className="h-8 w-8 mc-text-muted mb-2 mx-auto opacity-30" /><p className="text-sm mc-text-muted">Enter an amount to see projected returns</p></div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS. (Confirms `useGetUserSolGames`/`usePokeMyDeposit` are exported from `useQueries`, `useWallet` exposes `solanaPubkey`, and `Loader2` exists in `lucide-react`.)

- [ ] **Step 3: Build**

Run: `npm run build`
Expected: PASS (`tsc && vite build`; only pre-existing chunk-size warnings).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/SolInvestPanel.tsx
git commit -m "feat(sol-one-click): rework SolInvestPanel — one-click deposit, manual fallback, copy fix, count-up" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Full-suite verification

- [ ] **Step 1: Full test suite** — Run: `npm test` — Expected: PASS (all vitest files green, including the new `buildSolTransferTx` test and the existing `solPlanMapping`/`lamports`/`formatSolFloat` cases).
- [ ] **Step 2: Type-check** — Run: `npx tsc --noEmit` — Expected: PASS.
- [ ] **Step 3: Build** — Run: `npm run build` — Expected: PASS.
- [ ] **Step 4: Preview verification (manual — document findings).** As a SIWS session on the invest tab: pick a plan, confirm the count-up animation runs as you type the amount; confirm "DEPOSIT WITH PHANTOM" is the primary CTA and "send manually" is present. The full one-click path (Phantom popup → approve → "Opening…" → position in Profit Center within seconds) requires a real Phantom + devnet SOL **and** the Task 1 backend deploy live — verify manually. In the manual-fallback view, confirm the **copy button now works** (clipboard or execCommand fallback) and "Check now" triggers `pokeMyDeposit`. **No component-test harness — do not invent one.**

> **Reminder:** the one-click path is only fully functional after the operator-gated `ponzi_math_sol` deploy from Task 1 is live (so `pokeMyDeposit` exists on-chain). The deposit/send itself works without it; only the instant-poke does not.

---

## Notes / deliberate choices

- **Manual address/QR retained as a graceful fallback** (wallet missing/rejects/fails), with the fixed copy button + "Check now". The one-click path is primary.
- **Credit detection = a new SOL game appearing beyond the baseline** captured when entering `opening`/`manual` (via the existing 5s `useGetUserSolGames` refetch). Simple and race-free; the position appearing is the user-meaningful signal.
- **`pokeMyDeposit` failures are swallowed** in the one-click path — it's a best-effort speedup; the 60s timer is the backstop.
- **`lamports` passed as `Number` at the `SystemProgram.transfer` boundary** — exact for deposit-sized amounts and type-safe across web3.js versions.
- **Out of scope (own spec):** shared `GameMath.mo` + per-token-blackhole architecture (`gamemath_shared_library_plan` memory).
