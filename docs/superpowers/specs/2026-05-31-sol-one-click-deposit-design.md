# SOL One-Click Deposit + Triggered Scan (Frontend + `ponzi_math_sol`) Design

**Date:** 2026-05-31
**Status:** Design — brainstormed across session, pending spec review
**Component:** `frontend/src` — rework `SolInvestPanel` to deposit via the connected Solana wallet and trigger an immediate detection; plus one new `ponzi_math_sol` method (`pokeMyDeposit`).

## Problem / context

The SIWS deposit flow shipped in PR #104 works but doesn't feel like a native Solana dApp. Today: pick plan + amount → **Reserve Deposit Address** → copy a canister-derived address / scan a QR → **manually** send SOL from Phantom → an on-canister 60s timer detects it → position opens. Two gaps:

1. **The send is manual** (copy/paste or QR) — the biggest "this isn't a dApp" tell.
2. **Detection waits up to 60s** — the position appears only on the canister's next poll tick.

Two smaller issues in the same component: the **copy button is broken** (the clipboard write fails silently — you have to hand-select the address), and the **count-up ROI animation** present on the ICP panel ([GamePlans.tsx](../../../frontend/src/components/GamePlans.tsx) via `useCountUp`) was deliberately omitted from `SolInvestPanel`.

Goal: **one click → Phantom popup → approve → position opens in ~seconds**, with the existing 60s timer as the backstop. (Why it can't be truly instant-on-confirm: the game lives in an ICP canister that *observes* Solana by polling, not a Solana program — so "fast" not "instant." Established in the architecture discussion.)

## Resolved decisions (from brainstorming)

1. **One-click deposit via the wallet adapter.** The app builds the exact SOL transfer and has the connected wallet (Phantom/Solflare) sign + send it — no manual copy/paste on the happy path.
2. **Triggered scan.** After the wallet confirms the transfer, the frontend calls a new **user-scoped** detection method (`pokeMyDeposit`) so the canister credits the position in ~seconds. The existing 60s timer is the backstop — **no separate fallback layer** ("the timer *is* the fallback").
3. **Keep the manual address/QR flow as a graceful fallback** (wallet rejects, send fails, or user prefers manual), with the copy button **fixed** and a **"Check now"** button that calls `pokeMyDeposit`.
4. **Fold in (same component, while we're reworking it):** fix the copy button; add the count-up animation.
5. **OUT of scope (captured separately):** the shared `GameMath.mo` library + per-token-blackhole architecture.

## Scope

**In:** `SolInvestPanel` one-click flow + the manual fallback; one new `ponzi_math_sol` method (`pokeMyDeposit`); a `usePokeMyDeposit` hook; a pure `sendSolDeposit` send helper; copy-button fix; count-up animation.

**Out:** the shared-`GameMath` refactor; the M3 devnet→mainnet RPC flip; any change to the ICP invest flow; withdrawals (already canister-initiated/immediate, not detection-gated); changes to `prepareSolDeposit` (it already returns `{ intentId, depositAddress }` — no change needed).

## Backend — new method `pokeMyDeposit` (`ponzi_math_sol/main.mo`)

A user-callable, abuse-bounded detection trigger that scans **only the caller's own** deposit address. Models on the existing `scanAndCredit(address, principal)` and reuses the `detectionInProgress` guard ([main.mo:3056-3090](../../../ponzi_math_sol/main.mo)).

Shape:
```motoko
public shared ({ caller }) func pokeMyDeposit() : async { #Ok : Nat; #Err : Text } {
  requireAuthenticated(caller);
  if (not bootstrapped) { return #Err("Not bootstrapped") };
  // 1. Per-caller cooldown (cheap, before any work). POKE_COOLDOWN_NS ≈ 5s.
  //    Reject if the caller poked within the cooldown window.
  // 2. Open-intent gate: only proceed if the caller has an open, unexpired
  //    deposit OR buy intent. Otherwise return #Ok(0) — ZERO RPC outcalls.
  //    (This is the key abuse bound: you can only trigger a scan when you
  //    genuinely have a pending deposit, which itself cost a prepare call
  //    under the existing 3-intents/hour rate limit.)
  // 3. Respect detectionInProgress (return #Err("busy") if a tick/poke runs).
  // 4. Look up the caller's deposit address; scanAndCredit(addr, caller).
  //    Record the poke timestamp; release the guard in `finally`.
}
```

- **Guards:** `requireAuthenticated` (any SIWS/II user, for themselves — NOT admin); bootstrapped; per-caller `POKE_COOLDOWN_NS` cooldown (~5s); open-intent gate (no intent → zero outcalls); shared `detectionInProgress`.
- **Cost / abuse:** an idle/no-intent poke makes zero RPC calls. A poke with an open intent costs one `scanAndCredit` (a couple of sol-rpc calls), capped to once per ~5s per caller and only while a real deposit is pending. Bounded and tied to genuine activity.
- **Cooldown state** (`pokeTimestamps : Map<Principal, Int>`) can be `transient` (resetting on upgrade is harmless).
- **Deploy:** operator-gated `ponzi_math_sol` upgrade (`--mode upgrade --wasm-memory-persistence keep`; constant + method addition, no state-shape migration). **Must land before any eventual blackhole** — methods can't be added to a frozen canister.

## Frontend — `SolInvestPanel` rework

**Primary path — "Deposit with Phantom" (one click):**
1. Guard: actor ready, amount ≥ `MIN_DEPOSIT_SOL`, `solanaPubkey` present.
2. `usePrepareSolDeposit().mutateAsync({ plan, expectedAmountLamports })` → `{ intentId, depositAddress }`.
3. **Re-acquire the wallet** at send time (the SIWS session persists only the delegation identity + pubkey, not a live adapter — see [useWallet.tsx](../../../frontend/src/hooks/useWallet.tsx) `connectSiws`/`restoreSiwsSession`). Mirror `connectSiws`'s adapter selection: instantiate the Phantom/Solflare adapters, pick the Installed one, `.connect()` (silent — already authorized), and verify its `publicKey` matches `solanaPubkey`.
4. **Build the transfer** (`@solana/web3.js`, already a dep): `SystemProgram.transfer({ fromPubkey: solanaPubkey, toPubkey: depositAddress, lamports: expectedAmountLamports })` with a recent blockhash from a devnet `Connection`.
5. `adapter.sendTransaction(tx, connection)` → Phantom pops up; user approves → signature. `await connection.confirmTransaction(...)`.
6. `usePokeMyDeposit().mutateAsync()` → immediate canister scan.
7. Show **"Opening your position…"**; watch for credit via `useGetMyPendingSolIntents` (our intent leaves the pending list) and/or `useGetUserSolGames` (the game appears) → success state ("You're in"), then offer **Go to Profit Center**.

**Fallback path (graceful degradation):** if the wallet is unavailable, rejects, or `sendTransaction` fails, fall back to the **current** manual view — deposit address + QR + **fixed copy button** + a **"Check now"** button (calls `pokeMyDeposit`) — plus the standing assurance that the 60s timer will catch it. A "send manually instead" disclosure also exposes this path on demand.

**Copy-button fix:** wrap `navigator.clipboard.writeText` in try/catch with a fallback (select the address `<code>` / hidden-textarea + `execCommand('copy')`), and only show the ✓ confirmation when a write actually succeeds.

**Count-up animation:** add `useCountUp` to the ROI total and projected-PP numbers, matching the ICP panel (reset token bumped via effect on `(lamports, planId)` change — not mutated during render).

**RPC endpoint:** a configurable `SOLANA_RPC_ENDPOINT` constant — `https://api.devnet.solana.com` for now (public devnet RPC; rate-limited but fine), flips to a mainnet endpoint at the M3 cutover. Note in code that this is the devnet default.

## Hooks (`useQueries.ts`)

- **New** `usePokeMyDeposit()` — `useMutation` via `usePonziMathSolActor().actor.pokeMyDeposit()`, unwraps `{ Ok }|{ Err }`. On success, invalidate `['mySolPendingIntents']` and `['userSolGames']`.
- **Reuse** `usePrepareSolDeposit`, `useGetMyPendingSolIntents`, `useGetUserSolGames` (already exist, [useQueries.ts:2357-2422](../../../frontend/src/hooks/useQueries.ts)).

## Error / edge handling

- **Wallet not found / rejects / wrong pubkey / send fails** → fall back to the manual view; surface a clear message; never leave the user stuck.
- **Confirmation timeout** → the SOL may still land; show the manual view + "Check now"; the 60s timer is the backstop.
- **`pokeMyDeposit` `Err`** (cooldown, busy, not bootstrapped) → benign; the timer will credit it — show "Opening… (this can take up to a minute)".
- **±5% match band** (existing, [main.mo:2290-2303](../../../ponzi_math_sol/main.mo)): one-click sends the *exact* `expectedAmountLamports`, landing dead-center — so the stranded-funds class (deposit outside ±5% → unmatched → admin-only recovery) does not occur on the one-click path.
- **Network fee:** the user pays the ~5,000-lamport Solana fee on top; the wallet displays it. The transfer amount is exactly `expectedAmountLamports`.

## Unit conventions

- SOL: `parseSOL`/`formatSOL`/`LAMPORTS_PER_SOL` (lamports = `nat64`/bigint). `expectedAmountLamports` is the exact transfer amount.

## Testing / verification reality

- **Type/build/unit:** `npx tsc --noEmit`, `npm run build`, `npm test` clean. Unit-test the pure pieces (the transfer-builder's lamports/pubkey wiring where extractable; any cooldown/format helpers). No component-test harness — don't invent one.
- **Backend:** `dfx build ponzi_math_sol` clean; after the (operator-gated) deploy, devnet round-trip: open an intent, `pokeMyDeposit` with no deposit → `#Ok(0)` + no credit; send the matching SOL → `pokeMyDeposit` credits within seconds.
- **Frontend wallet path:** requires a real Phantom + devnet SOL → manual/preview verification (one-click → popup → approve → ~seconds → position in Profit Center). The input/ROI/animation render anonymously in preview; the send/poke path is wallet-gated.

## Files

- **Modify** `frontend/src/components/SolInvestPanel.tsx` — one-click flow, manual fallback, copy fix, count-up animation.
- **Create** `frontend/src/solana/sendSolDeposit.ts` (+ test where pure) — re-acquire wallet, build + send + confirm the transfer.
- **Modify** `frontend/src/hooks/useQueries.ts` — add `usePokeMyDeposit`.
- **Modify** `ponzi_math_sol/main.mo` — add `pokeMyDeposit` + `POKE_COOLDOWN_NS` + `pokeTimestamps`.
- (Possibly) **Modify** `frontend/src/lib/gameConstants.ts` or a solana config module — `SOLANA_RPC_ENDPOINT`.

## Decisions I made (flag any you'd change)

- **Manual address/QR kept as a fallback** (not removed) — resilience if the wallet path fails. The copy button + "Check now" live here.
- **Copy + animation folded into this spec** since they're in the same component being reworked. Say the word if you'd rather split them out.
- **Devnet public RPC** (`api.devnet.solana.com`) as the default endpoint for now.
- **`pokeMyDeposit` is `requireAuthenticated` (self-only) + cooldown + open-intent-gated** rather than a broader scan — keeps it cheap and abuse-bounded.

## Out of scope / follow-up

- **Shared `GameMath.mo` library + per-token-blackhole architecture** — noted in project memory (`gamemath_shared_library_plan`); its own spec later.
- **M3 devnet→mainnet flip** — when it lands, `SOLANA_RPC_ENDPOINT` and the DEVNET banner copy flip to mainnet.
