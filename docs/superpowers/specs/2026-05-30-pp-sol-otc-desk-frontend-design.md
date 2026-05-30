# Buy PP with SOL — Founder's Allocation Desk (Frontend) Design

**Date:** 2026-05-30
**Status:** Design — approved in brainstorming, pending spec review
**Component:** `frontend/src` — a buyer flyout (SIWS sidebar), a Charles's-office admin panel, and a hook module, all against the `ponzi_math_sol` desk candid merged in PR #98.

## Problem / context

The backend "Founder's Allocation desk" is merged to `main` (PR #98): `ponzi_math_sol` now lets SIWS users buy loose `pp_ledger` PP with SOL from an admin-run laddered order book. There is no frontend yet — the SIWS sidebar slot currently renders `BuySOLFlyout` (the plan-based game deposit), and Charles has no UI to stock/price/cash-out the desk. This spec covers the frontend: a **buyer flyout** that replaces `BuySOLFlyout` in the SIWS sidebar slot, and an **admin "PP Desk" panel** in Charles's office. Plan-opening is moving to a reworked invest tab (separate effort); the sidebar widget becomes buy-PP-only.

User-facing name: **Founder's Allocation** (flyout title + admin tab label).

## Scope

**In (v1, MVP):**
- Buyer hook module + buyer flyout (quote → lock → pay → credited lifecycle).
- Admin "Desk" tab in Charles's office: manage tiers (add/edit/remove), deposit/withdraw PP inventory, view stats, withdraw SOL proceeds.

**Out (deferred, backend already supports):**
- Open-buy-intent inspection table (`adminGetAllBuyIntents`), refund UI (`adminRefundDeskSol`), drag/reorder tier editor (`deskReorderTiers`).
- Any change to the plan-deposit `BuySOLFlyout` — it stays in the tree for the invest-tab rework; this spec only stops the sidebar widget from pointing at it.

**Approach:** assemble established patterns, no new architecture — anon queries via `useReadPonziMathSol`, SIWS/admin updates via `usePonziMathSolActor`, hook shapes mirroring `useGetMyDepositAddress`/`usePrepareSolDeposit`, UI assembled from `BuySOLFlyout` (address+QR) + `BuyPPFlyout` (quote + deposit-to-side-pocket) + `ShenanigansAdminPanel` (admin form).

## Integration map (verified against the live frontend)

- **Anon query actor:** `useReadPonziMathSol()` (`frontend/src/hooks/useReadPonziMathSol.ts`) — cached `HttpAgent` actor at `spc6q-xyaaa-aaaac-qg2ma-cai`. Use for `quoteBuyPP`, `deskListTiers`, `deskInventory`, `deskStats`, `getDepositAddressFor`.
- **Auth actor:** `usePonziMathSolActor()` (`frontend/src/hooks/usePonziMathSolActor.ts`) → `{ actor }`, routes on `walletType` (SignerAgent for Oisy, `HttpAgent`+identity for II/SIWS). Use for `createBuyIntent`, `getMyPendingBuyIntents`, and all `desk*` admin methods (backend enforces admin by caller principal).
- **Wallet:** `useWallet()` → `{ identity, principal, solanaPubkey, walletType }`. `walletType === 'siws'` gates the buyer widget; `solanaPubkey` is the proceeds-withdrawal target.
- **Hook templates:** `useGetMyDepositAddress` / `useGetMyPendingSolIntents` / `usePrepareSolDeposit` at `frontend/src/hooks/useQueries.ts:2373-2422`.
- **Sidebar slot:** `Shenanigans.tsx:895` (`<BuySOLWidget/>`) and `:908` (`<BuySOLFab/>`), gated `walletType === 'siws'`. `BuySOLWidget` renders `BuySOLFlyout variant="widget"`; `BuySOLFab` is the mobile sheet.
- **Admin nav:** `App.tsx` — `adminPanelTab: 'godView' | 'tuning'` (`:295`), rendered at `:763` as `CharlesGodView` / `ShenanigansAdminPanel`, with tab buttons at `:741`/`:751`. Admin gating: `isCharles(principal)` + `CHARLES_PRINCIPALS` (`frontend/src/lib/charles.tsx`).
- **PP ledger:** `usePpLedger.ts` — `PP_UNIT_SCALE = 100_000_000n`, `ppUnitsToWhole`/`wholePpToUnits`, `useAuthPpLedger`. `useLedger.ts` — `icrc2_approve`/`icrc2_allowance`.
- **Deposit-to-side-pocket prompt:** `BuyPPFlyout.tsx:170-189` (`useDepositChips` + `useAllowance`) — reuse for the desk's post-buy success state.
- **Formatting:** `frontend/src/solana/lamports.ts` — `formatSOL`/`parseSOL`/`LAMPORTS_PER_SOL`. PP display: `formatPP(units) = (Number(units)/1e8).toLocaleString(undefined,{maximumFractionDigits:2})` (inline in `BuyPPFlyout`).

## Files

- **Create** `frontend/src/hooks/useBuyPpDesk.ts` — the desk hooks.
- **Create** `frontend/src/components/Shenanigans/BuyPpDeskFlyout.tsx` — buyer flyout (`variant: 'widget' | 'sheet'`).
- **Create** `frontend/src/components/PpDeskPanel.tsx` — admin panel.
- **Modify** `frontend/src/components/Shenanigans/BuySOLWidget.tsx` and `BuySOLFab.tsx` — render `BuyPpDeskFlyout` instead of `BuySOLFlyout`. (The `Shenanigans.tsx` slot lines are unchanged.)
- **Modify** `frontend/src/App.tsx` — add `'desk'` to `adminPanelTab`, a "Desk" tab button, and the render branch `adminPanelTab === 'desk' ? <PpDeskPanel /> : ...`.

## Hook module — `useBuyPpDesk.ts`

- `useQuoteBuyPP(lamports: bigint)` — debounced (300ms, mirror `useQuotePP`) anon query via `useReadPonziMathSol().quoteBuyPP(lamports)`. Returns `DeskQuote { ppUnitsOut; legs; cappedByInventory }` or null when `lamports <= 0`. `enabled: lamports > 0n`, light `refetchInterval` so a freshly-stocked desk updates.
- `useCreateBuyIntent()` — `useMutation` via `usePonziMathSolActor().actor.createBuyIntent(lamports)`. Unwraps `{ Ok: { intentId; depositAddress; ppUnitsReserved; legs; expiresAt } } | { Err }`, throwing `Err` (surfaced as a toast — covers backend rejections: not bootstrapped, min-buy, no inventory, and the I-1 overlap guard). On success, invalidate `['myPpDeskPendingIntents']`.
- `useGetMyPendingBuyIntents()` — auth query via `usePonziMathSolActor().actor.getMyPendingBuyIntents()`, `queryKey: ['myPpDeskPendingIntents', principal]`, `refetchInterval: 10_000`, `enabled: walletType==='siws' && !!actor`. Drives settlement detection.
- The buyer's deposit address comes from `createBuyIntent`'s `Ok` response (the backend derives-or-reuses it) — no separate address-fetch hook is needed for the buy flow.

## Buyer flyout — `BuyPpDeskFlyout.tsx`

State machine (single active lock at a time):

1. **Quote** — SOL amount input (`parseSOL`); `useQuoteBuyPP` shows "You get **`formatPP(ppUnitsOut)`** PP", an effective "≈ R PP per 0.1 SOL" line, and — if `cappedByInventory` — a scarcity nudge: "Only `formatPP(ppUnitsOut)` PP left at this price." If the quote is `ppUnitsOut === 0` (desk empty), disable Buy with "Desk is out of stock." Keep the **DEVNET** warning banner (devnet RPC).
2. **Lock** — "Lock & buy" calls `useCreateBuyIntent`. On success, capture `{ intentId, depositAddress, ppUnitsReserved, expiresAt }`.
3. **Pay** — show the deposit address + copy button + QR (`solana:<addr>?amount=<formatSOL(lamports)>`), a **TTL countdown** to `expiresAt`, and "Send SOL from Phantom — your PP arrives within ~a minute." A "Start over" link clears the locked state (the reservation simply expires server-side).
4. **Credited** — settlement detected when our `intentId` leaves `useGetMyPendingBuyIntents()`. Show **"You got `formatPP(ppUnitsReserved)` PP"** (the reserved/expected amount; exact balance is the source of truth for the next step) and the reused **Deposit-to-Side-Pocket** prompt (`useDepositChips`/`useAllowance` from the `BuyPPFlyout` pattern), then offer "Buy more" to reset. Invalidate the PP balance query.

Rendered by `BuySOLWidget` (desktop `variant="widget"`) and `BuySOLFab` (mobile sheet).

## Admin panel — `PpDeskPanel.tsx` (Charles's office → "Desk" tab)

Gated `isCharles(principal)` (advisory; backend enforces). All mutations via `usePonziMathSolActor().actor`, each a `useMutation` invalidating the relevant query (mirror `useUpdateShenaniganConfig`). Three sections:

- **Inventory & stats** — `deskStats()`/`deskInventory()` show balance / reserved / available PP and accrued SOL proceeds + sold/open-intents. **Deposit PP**: input whole PP → `wholePpToUnits` → `icrc2_approve(ponzi_math_sol, units)` (via `useLedger`/`useAuthPpLedger`, skip if allowance sufficient) → `deskDepositInventory(units)`. **Withdraw PP**: `deskWithdrawInventory(units, principal)` back to Charles.
- **Tiers** — `deskListTiers()` as editable rows. Each row: **rate entered as whole PP per 0.1 SOL** (`N`), stored as `ratePpUnitsPer0_1Sol = N * 1e8`; **quantity** in whole PP (`qty * 1e8`); shows sold/remaining (`/1e8`). Add (`deskAddTier`), edit (`deskUpdateTier`), remove (`deskRemoveTier`). Order is top-down = best-deal-first.
- **Cash out** — accrued proceeds (`formatSOL(deskStats.proceedsLamports)`); **Withdraw SOL to Phantom** → `adminWithdrawDeskProceeds(solanaPubkey)`.

## Unit conventions (single source of truth)

- PP: 8 decimals. whole→units `* 1e8`; units→display `/1e8` (`formatPP`). Admin tier rate `N` (whole PP per 0.1 SOL) ↔ `ratePpUnitsPer0_1Sol = N * 1e8` (e.g. `N=250_000` ⇒ `25_000_000_000_000`).
- SOL: 9 decimals. `parseSOL`/`formatSOL`, `LAMPORTS_PER_SOL`.

## Error / edge handling

- **Quote:** `quote_trade`-style errors surface as "no quote"; `cappedByInventory` shows the scarcity nudge; `ppUnitsOut===0` disables Buy.
- **Lock rejections:** the `createBuyIntent` `Err` string is shown verbatim via toast — covers not-bootstrapped, below-min-buy, no-inventory, and the I-1 "open deposit of a similar amount" guard.
- **TTL expiry:** countdown reaching 0 returns the flyout to the Quote state with a "quote expired, get a new one" note; the reservation releases server-side.
- **Settlement latency / partial fill:** detection is the intent leaving the pending list; the credited figure displayed is the reserved/expected amount, and the live PP balance (refreshed) is authoritative for the side-pocket deposit. Underpay (partial fill) is a rare edge; v1 shows the expected figure and the real balance, not a reconciled delta.
- **Admin deposit:** approve-then-deposit; if `icrc2_approve` or `deskDepositInventory` errors, show the message and leave inventory unchanged. **Withdraw PP** beyond available returns the backend's "Only X PP available" `Err`.
- **Empty states:** no tiers / no inventory → buyer sees "Desk is out of stock"; admin sees an empty tier list with an "Add tier" affordance.

## Testing / verification reality

- **Unit (vitest):** pure helpers only — the PP-rate ↔ units conversion (`N ↔ ratePpUnitsPer0_1Sol`), PP/SOL display formatting, and the TTL-countdown formatter. Add a `*.test.ts` beside the helpers (the project's vitest suite already covers `lamports`/`base58`/`siwsSigner`).
- **Type:** `tsc --noEmit` clean (the new candid types are already generated).
- **Preview (dev server):** render the buyer flyout and admin panel; the **quote path is exercisable anonymously** against the live devnet canister once tiers are stocked (`quoteBuyPP` is an anon query). The authed flows (lock/buy, all admin mutations) require the right wallet and are verified manually; full SOL→PP e2e is the operator devnet round-trip from the backend plan.
- No component-test harness exists; do not invent one.

## Naming
User-facing: **Founder's Allocation** (flyout `<h2>` and the "Desk" admin tab can read "Founder's Allocation"). Internal identifiers use `ppDesk`/`PpDesk`. Follows the VC/MLM register (Seed Round, Series A, Carried Interest, Front-End Load) — not casino language.
