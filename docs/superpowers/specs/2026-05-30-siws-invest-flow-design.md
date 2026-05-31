# SIWS Invest Flow — SOL Plan Deposits (Frontend) Design

**Date:** 2026-05-30
**Status:** Design — approved in brainstorming, pending spec review
**Component:** `frontend/src` — a SIWS branch in the invest tab (`GamePlans.tsx`) plus a new `SolInvestPanel` component, against the already-deployed `ponzi_math_sol` deposit candid. One small `ponzi_math_sol` backend constant change (deposit minimum) ships with it.

## Problem / context

SIWS (Phantom/Solana) users **cannot open a plan position.** The invest tab — [`frontend/src/components/GamePlans.tsx`](../../../frontend/src/components/GamePlans.tsx) — is 100% ICP: it gates on `useICPBalance` with a `minDeposit = 0.1` ICP floor and deposits via `useCreateGame` (ICRC-2). There is no SIWS/SOL branch, so a SIWS user (whose `icpBalance` is 0) always sees "Fund Your Wallet First… Add ICP to your connected wallet" ([GamePlans.tsx:370-376](../../../frontend/src/components/GamePlans.tsx)) and has no way to deposit their devnet SOL.

The SOL plan-deposit capability already exists end-to-end on the backend and in hooks; it is only *orphaned* in the UI. `ponzi_math_sol` (mainnet canister `spc6q-xyaaa-aaaac-qg2ma-cai`, polling Solana **devnet** RPC) exposes `prepareSolDeposit({ plan; expectedAmountLamports }) -> { Ok: { intentId; depositAddress } } | { Err }`. The user sends devnet SOL to the returned address; an on-canister observer (~60s timer) detects it and opens the position. The flow that used this — [`BuySOLFlyout.tsx`](../../../frontend/src/components/Shenanigans/BuySOLFlyout.tsx) — is now rendered nowhere (replaced by the Founder's Allocation buy-PP desk), but remains a working mechanical reference.

This spec wires that capability into the invest tab so SIWS users can open positions, mirroring `BuySOLFlyout`'s mechanics but driven by `GamePlans`' selected plan and showing SOL/plan figures.

## Resolved decisions (from brainstorming)

1. **SOL deposit minimum → 0.01 SOL.** Lower than the current backend floor (0.05 SOL), so this includes a one-constant `ponzi_math_sol` change + an operator-gated SOL deploy. The deposit minimum is kept (not removed) — a floor is reasonable for plan positions.
2. **ROI display mirrors the ICP panel exactly (net base).** Both canisters already pay interest on the **gross** deposit; the ICP *frontend* projects ROI on the net (gross − 4%) deposit and the SOL panel will do the identical thing, keeping the two panels formula-identical. (The pre-existing frontend-vs-backend net/gross nuance is addressed separately — see Out of scope.)
3. **Architecture = branch at phase ③, extract a `SolInvestPanel` component.** Phases ①–② (mode/plan selection) stay shared in `GamePlans`.
4. **Pay-on-net game-math change is OUT of scope** → its own spec (see Out of scope / follow-up).

## Scope

**In:**
- A SIWS branch in `GamePlans` phase ③ that renders `SolInvestPanel` instead of the ICP balance-gate + `useCreateGame` flow.
- `SolInvestPanel.tsx`: SOL amount → ROI/PP projection → `prepareSolDeposit` → deposit address + QR + pending state.
- SOL-denominated rate copy ("PP per SOL") in the phase ①–② plan cards and summary strips for SIWS.
- A pure `solPlanMapping` helper (invest plan id → `SolGamePlan`) with unit tests.
- `MIN_DEPOSIT_SOL = 0.01` constant; matching backend change to `ponzi_math_sol` (`MIN_DEPOSIT_LAMPORTS` 50M → 10M) + one operator-gated SOL deploy.

**Out (deferred / separate):**
- **Pay-on-net** game-math change to `ponzi_math` + `ponzi_math_sol` (forward-only at creation) — own spec + premortem + deploy plan.
- Any change to ICP wallets' existing phase-③ flow (untouched).
- Any change to `BuySOLFlyout` / the Founder's Allocation desk.
- A SOL on-chain balance read / MAX button (we cannot read the external Phantom balance; not needed).

**Approach:** assemble established patterns, no new hooks or architecture. Anon reads via `useReadPonziMathSol`; SIWS updates via `usePonziMathSolActor` (null-until-identity). UI assembled from the ICP phase-③ layout (amount + ROI two-column) + `BuySOLFlyout`'s address/QR/pending mechanics.

## Integration map (verified against the live frontend/backend)

- **Auth actor:** `usePonziMathSolActor()` → `{ actor }`. For SIWS it is **null until the delegation identity is ready** ([usePonziMathSolActor.ts:56-59](../../../frontend/src/hooks/usePonziMathSolActor.ts)). Gate the reserve CTA on a non-null `actor`.
- **Anon read actor:** `useReadPonziMathSol()` — used by `useGetMyDepositAddress`.
- **Wallet:** `useWallet()` → `{ walletType, identity, principal, solanaPubkey }`. `walletType === 'siws'` gates the SOL branch.
- **SOL hooks (reused as-is):** [`useQueries.ts:2373-2422`](../../../frontend/src/hooks/useQueries.ts) — `useGetMyDepositAddress` (anon, `?Text` → `[]|[string]`), `useGetMyPendingSolIntents` (auth, `refetchInterval 10s`, `enabled: walletType==='siws' && !!actor && !!principal`), `usePrepareSolDeposit` (auth mutation, unwraps `{Ok}|{Err}`).
- **`SolGamePlan` variant object:** re-exported as `SolGamePlan` from `../backend` (def at [`declarations/ponzi_math_sol/index.ts:43`](../../../frontend/src/declarations/ponzi_math_sol/index.ts)) — `{ simple21Day | compounding15Day | compounding30Day }`, each `{ key: null }`.
- **Lamports:** [`solana/lamports.ts`](../../../frontend/src/solana/lamports.ts) — `parseSOL` (throws on invalid / >9 decimals), `formatSOL`, `LAMPORTS_PER_SOL`.
- **ROI / PP helpers (reused):** `calculateSimpleROI`, `calculateCompoundingROI`, `getDailyRate`, `getPlanDays` ([useQueries.ts:1067-1112](../../../frontend/src/hooks/useQueries.ts)) take an `amount` and operate on it directly. `COVER_CHARGE_RATE = 0.04`, `PP_PER_SOL_SIMPLE/COMPOUND_15/COMPOUND_30 = 6_000/12_000/18_000` ([gameConstants.ts](../../../frontend/src/lib/gameConstants.ts)).
- **Render site:** `GamePlans` is rendered in the `invest` tab at [`Dashboard.tsx:141`](../../../frontend/src/components/Dashboard.tsx).
- **Post-deposit destination (already exists):** `GameTracking.tsx` already renders SIWS SOL positions (`useGetUserSolGames`, gated `walletType==='siws'`, refetch 5s) with withdraw/settle ([GameTracking.tsx:261-263](../../../frontend/src/components/GameTracking.tsx)). The observer opens the position ~60s after the deposit; it then appears there.

## Architecture & data flow (Approach A)

`GamePlans.tsx` keeps ownership of `selectedMode` / `selectedPlan` and the phase ①–② UI. Two edits:
1. **Rate copy:** in the plan cards and summary strips, show "PP per SOL" using `PP_PER_SOL_*` when `walletType === 'siws'` (instead of the live `mintConfig` "PP per ICP"). Everything else (daily rates, durations, Carried Interest %s) is chain-agnostic and unchanged.
2. **Phase ③ branch:** `walletType === 'siws' ? <SolInvestPanel mode={selectedMode} planId={selectedPlan} onNavigateToProfitCenter={…} /> : <existing ICP phase 3>`. The ICP-only "Fund Your Wallet First" empty-state and `useCreateGame` flow live entirely inside the non-SIWS branch.

`SolInvestPanel.tsx` (new) owns the SOL mechanic. Props: the resolved plan (`mode`, `planId`) and the Profit Center nav callback. Internally:
- `parseSOL(input)` → `lamports` (try/catch → `0n` on invalid, mirroring `BuySOLFlyout`).
- ROI via the shared helpers on the **net** SOL amount (`net = sol × (1 − COVER_CHARGE_RATE)`), exactly as the ICP panel.
- Projected PP on the **gross** SOL amount: `round(sol × PP_PER_SOL[mappedKey])` (matches ICP, which computes PP on gross).
- `usePrepareSolDeposit` on the CTA; `useGetMyDepositAddress` + `useGetMyPendingSolIntents` for the address/pending block.
- Gated on a non-null `usePonziMathSolActor().actor`.

No new hooks. The plan→variant mapping is a small pure helper (`solPlanMapping.ts`), unit-tested.

## Plan → `SolGamePlan` mapping

| invest `mode` | invest `planId` | `SolGamePlan` |
|---|---|---|
| `simple` | `21-day-simple` | `simple21Day` |
| `compounding` | `15-day-compounding` | `compounding15Day` |
| `compounding` | `30-day-compounding` | `compounding30Day` |

The same `planId` values already drive `getDailyRate`/`getPlanDays`, so the mapping keys off `planId`.

## Phase ③ — `SolInvestPanel` states

**① Actor not ready** (`usePonziMathSolActor().actor === null`, e.g. the SIWS connect window): a disabled "Connecting your Solana session…" state. Fire no update calls.

**② Input** (no active reservation) — mirrors the ICP phase-③ two-column layout:
- *Left:* SOL amount input (`inputMode="decimal"`, `parseSOL`), a **MIN** button (0.01 SOL). **No "Available" balance and no MAX** — we can't read the external Phantom balance, and `prepareSolDeposit` enforces no max for plan deposits (only min + a 3-positions/hour rate limit + a bootstrap check; verified at [main.mo:2498-2552](../../../ponzi_math_sol/main.mo)). Inline validation: ≥ `MIN_DEPOSIT_SOL`, valid ≤9-decimal input.
- *Right:* the **same ROI calculator as ICP**, SOL-denominated — payout (interest / compounded) via `calculateSimpleROI`/`calculateCompoundingROI(net, planId, days)`, ROI %, a Front-End Load (4%) + net-deposit breakdown, daily earnings in SOL/day, and Ponzi Points (`PP_PER_SOL_*`, shown as "… / SOL").
- A prominent **DEVNET** warning banner (the canister polls Solana devnet RPC) and the existing **GAMBLING** warning band.
- CTA **"Reserve Deposit Address"** → `usePrepareSolDeposit({ plan: <mapped>, expectedAmountLamports: lamports })`. Disabled while `!actor`, below-min, or pending.

**③ Reserved** (intent created — `{ intentId, depositAddress }` captured):
- **The amount locks** (the backend matches the deposit to the intent by exact amount; changing it would orphan the deposit). Show the locked amount.
- Deposit **address + copy button + QR** (`solana:<depositAddress>?amount=<formatSOL(lamports)>`), the pending-intent count from `useGetMyPendingSolIntents`, and on-brand copy: "Send exactly {amount} devnet SOL from Phantom — your position opens automatically within ~a minute."
- **"Start over"** clears the reservation locally (it expires server-side via the intent TTL) and returns to Input.
- **No synchronous success toast** (SOL settlement is async). Instead, a **"Go to Profit Center"** CTA (the position lands in `GameTracking` once detected). *Optional polish (mark optional in the plan):* detect our `intentId` leaving the pending list and show a brief "You're in" nudge, mirroring the desk's "credited" state.

## Backend change (this spec — one operator-gated SOL deploy)

- [`ponzi_math_sol/main.mo:346`](../../../ponzi_math_sol/main.mo): `MIN_DEPOSIT_LAMPORTS : Nat64 = 50_000_000` → `10_000_000`.
- [`:2504`](../../../ponzi_math_sol/main.mo): update the `prepareSolDeposit` Err string to "Minimum deposit is 0.01 SOL (10,000,000 lamports)".
- The constant is also read by `adminCreditManualDeposit` ([:3354](../../../ponzi_math_sol/main.mo), admin recovery) — lowering the floor there is harmless; its Err string is generic and may be left as-is. The Founder's Allocation desk has its own min logic and is unaffected.
- **No state-shape change → no migration.** Deploy as `dfx deploy ponzi_math_sol --network ic --mode upgrade --wasm-memory-persistence keep` with the standard argument record (see repo guardrails). Verify `getActiveGameCount` / `getTotalDeposits` are preserved before/after, and restore the dfx identity to `rumi_identity` afterward. **Operator-gated — do not deploy without explicit per-deploy permission.**

Frontend: add `MIN_DEPOSIT_SOL = 0.01` to `gameConstants.ts`; `SolInvestPanel` validates against it.

## Error / edge handling

- **Actor null** (connect window) → disabled hint, no update calls.
- **Invalid input** → `parseSOL` throws → `lamports = 0n` → CTA disabled, no error spam.
- **Below min** → inline error + disabled CTA.
- **`prepareSolDeposit` `Err`** (rate limit "3 positions per hour", "not bootstrapped", below-min) → surfaced verbatim in an `mc-status-red` band (mirrors the ICP error band + `BuySOLFlyout`).
- **No balance check** — we cannot read the external wallet; the observer handles non/under-payment (the intent simply never matches and expires via TTL).
- **Amount lock in the reserved state** prevents an amount/intent mismatch.

## Unit conventions

- **SOL:** 9 decimals — `parseSOL`/`formatSOL`, `LAMPORTS_PER_SOL`. `expectedAmountLamports` is `nat64` (bigint).
- **Net basis (ROI display only):** `net = sol × (1 − COVER_CHARGE_RATE)`, `COVER_CHARGE_RATE = 0.04`.
- **PP projection:** gross `sol × PP_PER_SOL[mappedKey]`, rounded.

## Testing / verification reality

- **Type:** `npx tsc --noEmit` clean (SOL candid types are already generated).
- **Unit (vitest):** pure helpers only — the plan→`SolGamePlan` mapping and the min/PP projection. The ROI helpers are already covered for ICP and are reused unchanged. Add a `*.test.ts` beside the mapping helper (the suite already covers `lamports`/`base58`/`siwsSigner`).
- **Build:** `npm run build` clean.
- **Preview (dev server):** the Input state renders **anonymously** — amount field, ROI/PP projection, validation, the DEVNET banner — and is verifiable in preview. The authed reserve → address path requires a real SIWS wallet (Phantom) and is verified manually; the full SOL → position e2e is the operator's devnet round-trip.
- **No component-test harness exists — do not invent one.**
- **Backend min change:** after the (operator-gated) SOL upgrade, confirm on devnet that `prepareSolDeposit` accepts a 0.01 SOL intent and that game state is preserved.

## Files

- **Create** `frontend/src/components/SolInvestPanel.tsx` — the SIWS phase-③ SOL deposit panel.
- **Create** `frontend/src/lib/solPlanMapping.ts` (+ `solPlanMapping.test.ts`) — pure plan id → `SolGamePlan` mapping.
- **Modify** `frontend/src/components/GamePlans.tsx` — phase-③ SIWS branch + "PP per SOL" copy in the cards/summary strips.
- **Modify** `frontend/src/lib/gameConstants.ts` — add `MIN_DEPOSIT_SOL = 0.01`.
- **Modify** `ponzi_math_sol/main.mo` — `MIN_DEPOSIT_LAMPORTS` 50M → 10M and the Err string.

## Naming / voice

VC/MLM register, never casino (per `CLAUDE.md` and the `musical-chairs-brand` skill): "Front-End Load," "Carried Interest," sibling to "Founder's Allocation." The reserve CTA and pending copy stay on-brand; exact microcopy is finalized in the plan. The DEVNET banner is explicit about devnet SOL. (Note: `BuySOLFlyout` currently says "mainnet SOL" — stale relative to the live devnet canister; not in scope here. The devnet→mainnet copy flip is tracked under the Solana M3 cutover.)

## Out of scope / follow-up

- **Pay-on-net game-math change** (decided in this brainstorm, deferred): change `ponzi_math` and `ponzi_math_sol` to store the **net** deposit as the game's `amount` at creation, **forward-only** (so open positions in the current round are untouched — verified: earnings read the live `game.amount`, and a `--wasm-memory-persistence keep` upgrade preserves existing records byte-for-byte). This aligns each canister's actual payout with the net-based ROI the frontend already displays. It touches both live canisters (including `ponzi_math` with real ICP positions), so it gets its own spec, premortem, and two operator-gated deploys with before/after `getActiveGameCount`/`getTotalDeposits` checks.
- **Solana M3 mainnet cutover** (operator) — when the canister flips from devnet to mainnet RPC, the DEVNET banner copy here (and the stale `BuySOLFlyout` string) flip to mainnet.
