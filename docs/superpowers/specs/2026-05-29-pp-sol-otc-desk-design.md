# Buy PP with SOL — Founder's Allocation Desk (Solana)

**Date:** 2026-05-29
**Status:** Design — approved in brainstorming, pending spec review
**Component:** `ponzi_math_sol` (backend) + frontend SIWS buy widget + Charles's office admin panel

## Problem

On the ICP build, the sidebar "Buy PP" widget routes buy orders to **PartyDEX** (an ICP DEX with a hybrid pool/order-book). On the Solana build there is no equivalent: the SIWS sidebar slot currently renders `BuySOLFlyout`, which is actually the *plan-based game deposit* (pick a plan → send SOL → PP credited as a position), not a "buy loose PP" flow. There is no Solana DEX we want to route to.

Plan-opening for Solana is being reworked into the **invest tab** (separate effort, out of scope here). That frees the sidebar widget to do one thing: let users **buy loose PP directly from Charles for SOL**.

Rather than integrate a Solana DEX, Charles runs an admin-operated **OTC desk**: he stocks PP inventory, sets laddered prices, and users buy from him. Cross-chain settlement (SOL in on Solana, PP out on ICP) reuses the deposit-observer pipeline already built and hardened in `ponzi_math_sol`.

## Decisions locked (from brainstorming)

| Fork | Decision |
|------|----------|
| SOL routing & payment matching | Buyer pays their **existing canister-controlled deposit address**; the observer matches the payment to an open buy-intent. SOL pools in the canister; Charles withdraws to his Phantom. (Reuses the full existing pipeline; payment-matching is free.) |
| PP inventory custody | **Escrow in the desk.** Charles deposits PP from his wallet into a canister-controlled escrow account, reserved against live tiers. Settlement is atomic — no "paid SOL, got no PP" case. |
| Market side | **Sell-only, laddered tiers.** Users buy from Charles; no bid side. |
| Pricing | **PP per SOL** (displayed/entered as **PP per 0.1 SOL**), set per tier, no oracle. Early tiers give more PP per SOL; the rate drops as tiers sell out. |
| Widget placement | The desk **replaces** `BuySOLFlyout` in the SIWS sidebar slot. Widget is buy-PP-only. Plan-opening moves to the reworked invest tab (out of scope). |

## Scope

**In scope**
- New buy-PP-with-SOL flow in `ponzi_math_sol`: escrow inventory, laddered tiers, free quote query, buy-intent reservation, observer settlement, refund path.
- Admin panel ("PP Desk") in Charles's office: stock/withdraw inventory, manage tiers, view stats, withdraw SOL.
- Buyer widget replacing `BuySOLFlyout` in the `BuySOLWidget` / `BuySOLFab` slot.

**Out of scope**
- Plan-opening via the widget (→ invest-tab rework). The backend plan-deposit path (`prepareSolDeposit` → open position) **stays**; the widget simply stops pointing at it.
- Two-sided market / bidding to buy PP back.
- USD-pegged pricing or any oracle.
- Secondary resale, true price-time-priority limit book, continuous bonding curve.

## Architecture

The desk lives **inside `ponzi_math_sol`**, because that canister already owns:
- per-user Solana deposit addresses (`depositAddresses`, `getOrCreateDepositAddress`),
- the RPC observer with 60s detection timer, open-intent scan, cursor-advance-on-unmatched, and 2h deposit TTL,
- withdraw-to-caller-specified-Solana-address (PRs #93/#94/#95).

The desk is modeled as a **new intent type** on that existing machinery. When the observer matches a payment to a `#buyPP` intent, instead of opening a game position it releases escrowed PP to the buyer.

**Rejected alternatives:** a separate `pp_desk` canister (would duplicate the SOL observer/address/withdrawal plumbing or require awkward cross-canister polling); hosting in `shenanigans` (it can mint PP but has no SOL observer, and we're selling existing escrowed PP, not minting).

PP is the real ICRC-1 `pp_ledger` token (symbol "PP", 8 decimals, transfer fee 0, icrc2 on, minting account = `shenanigans` canister `j56tm-...`). Moving PP is a plain `icrc1_transfer`.

## Backend design (`ponzi_math_sol`)

### State (new)
- **Escrow account:** a dedicated subaccount of the `ponzi_math_sol` principal on `pp_ledger`. Balance is queryable from the ledger; the canister additionally tracks `escrowReservedUnits : Nat` (PP held against open buy-intents). `available = ledgerBalance(escrow) − escrowReservedUnits`.
- **Tiers:** ordered `Tier = { ratePpUnitsPer0_1Sol : Nat; ppUnitsTotal : Nat; ppUnitsSold : Nat }`, consumed **top-down in Charles's configured order** (he lists them best-deal-first — highest PP per 0.1 SOL at the top — so early buyers get the better rate). Active tier = the first with `ppUnitsSold < ppUnitsTotal`.
- **Buy-intents:** new variant in the intent system:
  `#buyPP { buyer : Principal; reserved : [{ tierIndex : Nat; ppUnits : Nat }]; ppUnitsReservedTotal : Nat; quotedLamports : Nat; depositAddress : Text; expiresAt : Int }`.

### Admin methods (gated to Charles's principal via the existing admin gate)
- `deskDepositInventory(ppUnits)` — pulls PP from Charles via `icrc2_transfer_from` (he `icrc2_approve`s the canister first) into the escrow subaccount.
- `deskWithdrawInventory(ppUnits, toAccount)` — returns unsold (unreserved) PP from escrow to Charles. Must not draw below `escrowReservedUnits`.
- `deskAddTier / deskUpdateTier / deskRemoveTier / deskReorderTiers` — manage the ladder. Editing a tier never touches PP already reserved by open intents (reservations carry their own locked rate). Reducing a tier's `ppUnitsTotal` is clamped to ≥ `ppUnitsSold + reserved-in-this-tier`.
- `deskWithdrawSol(toAddress)` — existing withdraw path; sends pooled SOL to Charles's Phantom.
- Refund controls (see Edge cases).

### Public methods
- `quoteBuyPP(lamports : Nat) : query → { ppUnitsOut; tierBreakdown : [{ tierIndex; ppUnits; ratePpUnitsPer0_1Sol }]; cappedByInventory : Bool }` — free query; walks available tiers top-down in list order. `cappedByInventory` is true when inventory can't fully fill the requested SOL.
- `createBuyIntent(lamports : Nat) : update → { intentId; depositAddress; ppUnitsReserved; tierBreakdown; expiresAt }` — caller is the SIWS-derived IC principal. Re-quotes against current inventory, **reserves** the PP (increments `escrowReservedUnits` and each tier's reserved counter), and creates the `#buyPP` intent on the caller's deposit address with the buy-quote TTL. Rejects below the minimum buy. (Caller is the buyer's IC principal — SIWS-derived in the widget flow; the backend doesn't hard-restrict to SIWS.)

### Settlement (observer extension)
When the observer matches an incoming payment to an open `#buyPP` intent (by deposit address + expected lamports):
1. Walk the intent's `reserved` chunks in order, consuming `lamportsReceived` (`ppUnits` per chunk = `lamports_for_chunk × ratePpUnitsPer0_1Sol / 1e8`), to compute `ppToCredit ≤ ppUnitsReservedTotal`.
2. `icrc1_transfer(escrow → buyer)` `ppToCredit` PP. **Transfer first, then record** — only on success do we mark tiers `ppUnitsSold +=`, decrement `escrowReservedUnits`, and emit a `#deskSale` general-ledger event. This mirrors the existing transfer-then-record pattern in `ponzi_math_sol` (the comment at ~L485: "prevents a phantom ledger entry when the transfer fails and state mutations are rolled back").
3. Release any unfilled reservation (underpay) back to the pool.
4. SOL remains in the canister pool for Charles to withdraw.

PP lands in the buyer's **main wallet account** (not the side pocket), exactly like a PartyDEX buy. The post-buy "Deposit to Side Pocket" prompt (already in the codebase) applies.

## Quote & tier math

PP has 8 decimals (1 PP = 1e8 units); SOL has 9 (0.1 SOL = 1e8 lamports). Storing the rate as **PP-units per 0.1 SOL** makes the conversion exact integer arithmetic:

```
ppUnitsOut(lamports) = floor( lamports × ratePpUnitsPer0_1Sol / 1e8 )
```

For a whole-PP rate `N` (what Charles types as "PP per 0.1 SOL"), `ratePpUnitsPer0_1Sol = N × 1e8`, and the formula reduces to `ppUnitsOut = lamports × N` — no rounding. Example: `N = 270,000` ⇒ 0.1 SOL (1e8 lamports) buys 270,000 PP; 1 SOL buys 2,700,000 PP. A multi-tier quote sums `ppUnitsOut` across the SOL spent in each tier, walking top-down through the ladder. Floor rounding favors the house.

## Frontend design

### Buyer widget (replaces `BuySOLFlyout` in the slot)
New `BuyPpDeskFlyout.tsx`, rendered by the existing `BuySOLWidget` (sidebar, `Shenanigans.tsx:895`, `BankPage.tsx:42`) and `BuySOLFab` (mobile, `Shenanigans.tsx:908`) wrappers — minimal churn, just swap the inner flyout. Modeled on `BuyPPFlyout` + `usePartyDexBuy.ts`.

New hook file `useBuyPpDesk.ts`:
- `useQuoteBuyPP(lamports)` — debounced free query (mirrors `useQuotePP`), returns PP out, effective rate, tier breakdown, `cappedByInventory`.
- `useCreateBuyIntent()` — mutation calling `createBuyIntent`; on success surfaces the deposit address.
- Reuse `useGetMyDepositAddress`, `useGetMyPendingSolIntents`, `formatSOL`/`parseSOL`/`LAMPORTS_PER_SOL`.

Flow: enter SOL amount → live quote (PP out, effective PP-per-0.1-SOL rate, "X PP available" if capped) → **Lock & get address** (creates intent, reserves PP) → show deposit address + QR + countdown to TTL + pending state → on credit: success toast + "Deposit to Side Pocket" prompt. Keep the DEVNET warning while on devnet RPC.

### Admin panel — "PP Desk" (Charles's office)
New `PpDeskPanel.tsx` in the admin area (`CharlesGodView.tsx` / `ShenanigansAdminPanel.tsx`), gated like other admin tools:
- **Inventory:** escrow balance, available vs reserved; **Deposit PP** (approve → `deskDepositInventory`) and **Withdraw PP** (`deskWithdrawInventory`).
- **Tiers:** editable ordered list — each row a rate (entered as **PP per 0.1 SOL**) + quantity (PP), showing sold/remaining; add / remove / reorder; save.
- **Stats:** PP sold, SOL taken, remaining inventory, open intents.
- **Cash out:** pooled SOL balance + **Withdraw SOL to Phantom** (`deskWithdrawSol`).

## Edge cases & failure handling

The core simplifier: a buy-intent locks the **rate + reserved PP**, not a fixed SOL amount. Settlement credits PP by consuming the reserved chunks with whatever SOL actually arrives.

- **Underpay** → partial fill: SOL fills the reserved chunks in order, buyer gets proportional PP, the remaining reservation releases. No refund needed.
- **Overpay** → fills the full quote; only SOL beyond the reserved cap is excess (refundable, below).
- **Quote TTL ≈ 15 min** — a dedicated buy-quote TTL, separate from the 2h deposit TTL. Safe to keep short because the 60s detection timer auto-matches payments. On expiry the reservation returns to the pool.
- **Tier edits / races** — an open intent locked its rate at confirm-time, so later tier edits never change it. Two buyers racing for the last chunk are resolved first-to-lock; the loser is re-quoted against remaining inventory.
- **Inventory exhaustion** — `quoteBuyPP` returns `cappedByInventory = true` and the partial fillable amount; the UI shows "only X PP available."
- **Refund path (the one place SOL must go back)** — excess SOL beyond the reserved cap, or a payment landing after TTL (reservation already released), is logged and refunded to the **captured sender address** via the existing withdraw path (one-click in the office for the MVP; auto-refund is a later enhancement). The observer must capture the source address from the incoming Solana transfer.
- **Minimum buy** — small configurable lamport floor to avoid dust intents.
- **PP transfer failure on settlement** — handled by transfer-first-then-record: state mutations roll back and the intent stays open/retryable; the buyer's SOL is safe in the pool and refundable.

## Naming (open, not blocking)

Internal: `ppDesk` / "PP Desk". User-facing should follow the established VC/MLM register (Seed Round, Series A, Carried Interest, Front-End Load) — **not** casino language. Candidates: **Founder's Allocation**, **Private Placement**, **Direct Allocation**, **Founder's Round**. Working title in this doc: *Founder's Allocation Desk*. Charles to pick the final string.

## Implementation notes / to verify during build
- Confirm the exact admin-gating helper in `ponzi_math_sol` and reuse it for all `desk*` admin methods.
- Confirm the observer can extract the **sender** pubkey from a matched incoming transfer (needed for refunds); if not directly available, capture it during RPC parsing.
- Adding `#buyPP` plus the new desk state to `ponzi_math_sol` is a stable-state change → requires an explicit migration (`with migration = Migration.runVN`), enumerating every current intent/state variant per the project's migration discipline. Backend deploy only with explicit permission.
- Reuse the existing post-PartyDEX-buy "Deposit to Side Pocket" prompt for the desk's success state.
