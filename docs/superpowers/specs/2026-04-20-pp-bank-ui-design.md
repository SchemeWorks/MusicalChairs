# PP "Bank" UI — in-fund custody bridge

**Date**: 2026-04-20
**Status**: Design approved; implementation plan pending
**Depends on**: [2026-04-19 PP real-token architecture](2026-04-19-pp-real-token-architecture-design.md)

## Problem

The PP-as-real-token refactor landed. In-fund custody (shenanigans-owned subaccounts), external deposits (ICRC-2 approve + `depositChips`), and the 7-day withdrawal queue are all live on-chain. But the player-facing surface is weak:

- [WalletDropdown.tsx:296](../../../frontend/src/components/WalletDropdown.tsx#L296) still shows one limp line: `1,234 PP`. That number is actually **two distinct balances now** — Wallet PP and Position PP — and conflating them hides the mental model the game depends on.
- [ChipWallet.tsx](../../../frontend/src/components/ChipWallet.tsx) exists at `#chips` but is an unlinked hash route. No nav, no discoverability, generic three-card layout.
- There is no reason for a player to come back and claim an unlocked withdrawal — nothing alerts them.
- First-time users have no explanation of why Wallet PP and Position PP are different, why deposits require an allowance, or why withdrawal has a 7-day lockup.

This spec covers the UI-side design needed to turn the existing on-chain mechanics into an honest, discoverable player surface. It also specifies one small backend addition (cancel withdrawal) that the UI depends on.

## Voice

All user-facing strings use **MLM/VC jargon**, not casino framing. Bank, Position, Allocation, Lockup, Allowance, Commitment, Redeem, Deploy. No "chips," no "table," no "cashier," no "pit" in any string the user reads.

Internal code identifiers (`chipSubaccount`, `depositChips`, `requestCashOut`, etc.) stay as-is, matching the existing rename-restraint pattern in [CLAUDE.md](../../../CLAUDE.md) — `exitToll` code / "Carried Interest" UI, `coverCharge` code / "Front-End Load" UI. Now: `chip*` code / "Position" UI.

## Goal

A clear two-surface design for PP in-fund custody:

1. **Wallet dropdown** = glance. Shows both balances and a link to the full page. No operational controls.
2. **Bank page** = depth. All deposit, withdrawal, cancel, claim, and allowance controls live here, plus the pending queue with live shortfall visibility.

Plus a header "Bank" link with a notification badge when the player has PP ready to redeem.

## Non-goals

- No changes to `backend` (ICP / ponzi / money code). Blackhole-readiness is unaffected.
- No changes to `pp_ledger` (already re-inited on mainnet).
- No changes to the 7-day lockup semantics, minimum-deposit floor, or the "queued PP stays spell-exposed" rule from the prior spec.
- No peer-to-peer PP transfer UI from the main wallet. `pp_ledger` supports it, but the Bank is about the custody bridge only.
- No transaction history / activity log. The pending queue is enough for v1.
- No rename of internal code identifiers. "Position" is the UI label for PP held at `(shenanigans, chip_sub(P))`. The underlying `chip*` code names don't change.
- No custom allowance slider. Unlimited default is fine; rescoping externally still works.

## Surfaces

### Wallet dropdown (glance)

Replace the single PP line at [WalletDropdown.tsx:296](../../../frontend/src/components/WalletDropdown.tsx#L296) with a two-row balance block plus an inline link:

```
Wallet     1,234 PP
Position   87,500 PP          Bank →
```

`Bank →` opens the Bank page (same target as the header link). No controls in the dropdown — this is purely glance.

**Data sources:**
- Wallet PP: `pp_ledger.icrc1_balance_of({ owner = P, subaccount = null })`
- Position PP: `pp_ledger.icrc1_balance_of({ owner = shenanigans, subaccount = chip_sub(P) })` — already wired via `useGetPonziPoints`, which returns `walletPoints` and `chipPoints` internally. Surface both separately in the dropdown.

### Header nav link

Add a "Bank" link next to "Docs" in the main header. Plain text link, same visual weight as Docs.

**Badge rules:**
- Badge activates **only when one or more withdrawal entries are unlocked and ready to redeem**.
- Badge shows the count of redeemable entries (not total pending, not "unlocking soon").
- Off in all other states (no pending, pending but locked, etc.).
- Rationale: the badge's only job is "come redeem your PP" — actionable. "Something will unlock tomorrow" is noise.

### Bank page

Top-to-bottom layout:

#### 1. Summary row

Three items on one line:

- **Wallet PP** — `pp_ledger` balance at `(P, null)`.
- **Position PP** — `pp_ledger` balance at `(shenanigans, chip_sub(P))`.
- **Allowance indicator** (right-aligned) — current approved allowance, with a "Revoke" action. Shows `∞` when set to the unlimited default; otherwise the specific amount.

Small footnote under the row:

> PP earned from deposits and gameplay lands in your Position within ~10 seconds.

This quietly discloses observer lag without alarming anyone.

#### 2. Bridge card

One card with a direction toggle (flip arrow in the center). The form reveals direction-specific UI:

**Direction: Wallet → Position (Deposit)**

- Amount input, minimum **5,000 PP** (matches `MIN_DEPOSIT` on shenanigans; enforce client-side with a tooltip, backend rejects regardless).
- First-ever use (no existing allowance): CTA reads "Approve allowance," then "Deposit." Two signatures.
- Subsequent uses (allowance exists and covers amount): single "Deposit" CTA. One signature.
- Helper text under input: *"Deposited PP becomes your Position — deployable in gameplay, spendable on spells, movable by the protocol."*

**Direction: Position → Wallet (Redeem)**

- Amount input. No minimum.
- Prominent lockup warning card (amber): *"7-day lockup. Queued PP remains in your Position and stays exposed to spells during the lockup window."*
- CTA: "Queue redemption."

**Empty state (zero Position, first-time user):**
Bridge is pre-set to Wallet → Position direction, with a header on the card:

> You have no Position yet. Deposit PP from your wallet to deploy capital into the fund.

#### 3. Pending queue card

List of all pending (not yet redeemed, not yet cancelled) withdrawal entries, ordered **FIFO** by `claimableAfter` timestamp ascending. Each row surfaces:

- **Normal** (Position covers queued amount):
  `50,000 PP · unlocks in 18h · [Cancel]`
- **Shortfall** (Position < queued):
  `50,000 PP · unlocks in 18h · ⚠ spells reduced this to ~12,000 · [Cancel]`
  The "~12,000" is computed live as `min(queued_amount, current_position_balance)`, re-queried alongside the pending list. For multiple pending entries, the Position balance is attributed FIFO — oldest entry gets first claim on the available balance, remaining entries show reduced payouts against what's left.
- **Redeemable** (unlocked):
  `50,000 PP · ready now · [Redeem]`
  Shortfall display applies here too — if Position is short at redeem time, the row shows the effective payout before the player commits.

**Empty state:** *"No pending redemptions."*

## Mechanics

### Allowance model

- First deposit triggers an `icrc2_approve` for the unlimited ICRC-1 sentinel (`2^64 − 1`) from the player's main account, with shenanigans as spender.
- Summary row reads the current allowance (`pp_ledger.icrc2_allowance({ account = (P, null), spender = (shenanigans, null) })`) and displays it.
- "Revoke" sets allowance to 0 via `icrc2_approve(0)`. Next deposit re-prompts.
- Rationale: players are already trusting shenanigans with all Position custody. Per-deposit signature friction has no real safety gain. The revoke escape hatch covers anyone who wants to rescope.
- Current [useApproveForDeposits](../../../frontend/src/hooks/useQueries.ts#L1072) approves `depositAmount * 10` — replace with unlimited default.

### Cancel withdrawal (new backend method)

Add to shenanigans:

```motoko
public shared ({ caller }) func cancelCashOut(id : Nat) : async Result<(), Text> {
    // Look up entry by id, verify caller == entry.player.
    // Verify not already claimed, not already cancelled.
    // Flip entry to cancelled state.
    // No ledger call — PP never left the Position subaccount.
};
```

(Method name keeps the internal `CashOut` vocabulary. UI labels it "Cancel redemption.")

Semantics:
- Cancellation is **free** (no burn, no fee).
- PP was never moved out of the Position subaccount during the queue window; the entry is purely metadata. Flipping to cancelled removes it from the "pending" view. No ledger work needed.
- Cancellation **does not** grant any timer credit. Re-queuing waits a fresh 7 days. Anti-shuttle friction intact.
- Rationale: 7-day lockup is the anti-shuttle friction. Locking the decision on top of that is friction tax for no gameplay benefit. Misclicks happen.

Query filter:
- `getMyCashOuts` should return non-cancelled entries only (or include a `cancelled` flag that the frontend filters on). TBD at implementation — either approach works.

### Observer lag disclosure

The shenanigans observer polls backend every ~10s to mint PP for gameplay events. After a `createGame` or backer top-up, the player's Position updates on the next tick.

The Bank page handles this with:
- The footnote under the summary row (above).
- Polling-based refresh on the balance queries (already in `useGetPonziPoints` via `refetchInterval`).

No optimistic UI updates. Wait for the real Position balance to arrive from `pp_ledger`. Cosmetic lag; acceptable.

## Edge cases

- **Player tries to deposit < 5,000 PP** → frontend validation; if bypassed, backend `depositChips` rejects with an error the UI surfaces.
- **Player tries to redeem more than Position balance** → backend rejects at `requestCashOut`. UI should validate client-side first (Position balance query), but backend is the authority.
- **Player queues redemption, spells drain Position to zero, player redeems** → redeem pays 0 PP. Entry is marked claimed. Row disappears (or moves to a "claimed" style if we want a short-lived visual confirmation; TBD at implementation).
- **Multiple pending entries, Position covers some but not all** → FIFO: oldest entry draws first from current Position in the shortfall display. Backend claim enforces the actual Position balance at redeem time regardless of display order.
- **Player cancels while a race condition is in flight (e.g., spell cast same tick)** → cancel only flips metadata; Position balance is whatever the ledger says after the spell. No conflict.
- **Allowance is lower than requested deposit** → frontend detects mismatch on the summary row's allowance read; deposit CTA switches back to "Approve allowance" flow.
- **Player revokes allowance mid-session** → summary row reflects `0`; next deposit re-prompts. Existing Position unaffected.

## Frontend components (rough inventory)

- **`BankPage.tsx`** (rename of `ChipWallet.tsx`) — top-level page at the new route.
- **`BankSummary.tsx`** — balance row + allowance indicator + revoke action + observer-lag footnote.
- **`BridgeCard.tsx`** — direction toggle, amount input, direction-specific CTA logic.
- **`PendingQueueCard.tsx`** — list of pending entries with live shortfall computation, cancel/redeem actions.
- **`WalletDropdown.tsx`** — modify to show two-row balance + `Bank →` link.
- **`Header` / main nav** — add Bank link with badge.

New hooks needed:
- `useAllowance()` — reads `icrc2_allowance` for `(P, null) → (shenanigans, null)`.
- `useRevokeAllowance()` — mutation that calls `icrc2_approve(0)`.
- `useCancelCashOut()` — mutation against new `shenanigans.cancelCashOut(id)`.
- `useWalletPp()` / `usePositionPp()` — may already exist inside `useGetPonziPoints`; extract if needed for the dropdown to keep them separate.

Existing hooks reused as-is: `useDepositChips`, `useRequestCashOut`, `useClaimCashOut`, `usePendingCashOuts`.

## Backend addition

Single new method on `shenanigans/main.mo`:

- `cancelCashOut(id : Nat) : async Result<(), Text>` — owner-checked, flips a pending entry to cancelled. No ledger call.

If `getMyCashOuts` currently returns cancelled entries, add filtering (either server-side or client-side; server-side preferred to save round-trips).

Admin setter for `CASH_OUT_DELAY_SECONDS` already exists (from prior spec).

## Migration / rollout

- No on-chain state migration needed. The change is UI + one new method.
- Existing `#chips` route continues to work during rollout; the new nav item targets the same (eventually renamed) page.
- No player-visible lockups need to be honored specially — existing pending withdrawals carry over unchanged.

## Open questions

None blocking. Implementation-time decisions:

- **Cancelled entry retention** — soft-delete vs hard-delete in shenanigans state. Soft-delete is safer for debuggability; hard-delete is cleaner state. Pick at implementation.
- **Redeemed row UX** — after clicking "Redeem" with a successful payout, do we show a brief success state in the row before it disappears, or just remove immediately? Cosmetic, decide at implementation.
- **Mobile layout for the Bank page** — bridge card and queue card should stack vertically; summary row wraps. Standard responsive treatment.

## Out of scope (deferred)

- Peer-to-peer PP send from main wallet (supported by `pp_ledger` natively; separate feature).
- Activity / transaction history for Position.
- Per-deposit allowance granularity (slider).
- Push notifications for redeemable withdrawals (badge-only for now).
- Bulk cancel / bulk redeem (one-at-a-time is fine for v1).
