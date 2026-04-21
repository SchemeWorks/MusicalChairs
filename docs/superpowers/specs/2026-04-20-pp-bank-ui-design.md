# PP "Bank" UI — chip custody bridge

**Date**: 2026-04-20
**Status**: Design approved; implementation plan pending
**Depends on**: [2026-04-19 PP real-token architecture](2026-04-19-pp-real-token-architecture-design.md)

## Problem

The PP-as-real-token refactor landed. Chip custody (shenanigans-owned subaccounts), external deposits (ICRC-2 approve + `depositChips`), and the 7-day cash-out queue are all live on-chain. But the player-facing surface is weak:

- [WalletDropdown.tsx:296](../../../frontend/src/components/WalletDropdown.tsx#L296) still shows one limp line: `1,234 PP`. That number is actually **two distinct balances now** — wallet PP and chip PP — and conflating them hides the mental model the game depends on.
- [ChipWallet.tsx](../../../frontend/src/components/ChipWallet.tsx) exists at `#chips` but is an unlinked hash route. No nav, no discoverability, generic three-card layout.
- There is no reason for a player to come back and claim an unlocked cash-out — nothing alerts them.
- First-time users have no explanation of why "chips" and "wallet PP" are different things, why deposits require approval, or why the cash-out has a 7-day timer.

This spec covers the UI-side design needed to turn the existing on-chain mechanics into an honest, discoverable player surface. It also specifies one small backend addition (cancel cash-out) that the UI depends on.

## Goal

A clear two-surface design for PP chip custody:

1. **Wallet dropdown** = glance. Shows both balances and a link to the full page. No operational controls.
2. **Bank page** = depth. All deposit, cash-out, cancel, claim, and allowance controls live here, plus the pending queue with live shortfall visibility.

Plus a header "Bank" link with a notification badge when the player has PP to claim.

## Non-goals

- No changes to `backend` (ICP / ponzi / money code). Blackhole-readiness is unaffected.
- No changes to `pp_ledger` (already re-inited on mainnet).
- No changes to the 7-day lockup semantics, minimum-deposit floor, or the "queued PP stays spell-exposed" rule from the prior spec.
- No peer-to-peer PP transfer UI from the main wallet. `pp_ledger` supports it, but it's out of scope; the Bank is about the chip bridge only.
- No transaction history / activity log. The pending queue is enough for v1.
- No rename of "chips" in code or UI. The word is load-bearing — it distinguishes shenanigans-custody PP from wallet PP, and renaming loses that wedge. ([CLAUDE.md](../../../CLAUDE.md) rename-restraint pattern applies here too.)
- No custom allowance slider. Unlimited default is fine; rescoping externally still works.

## Voice

MLM/VC jargon, not casino framing. "Bank," "allowance," "deposit," "cash-out," "lockup." Avoid "cashier," "table," "pit," "chip stack." "Chips" and "dealer" stay in the game because they're entrenched, but no new casino terms.

## Surfaces

### Wallet dropdown (glance)

Replace the single PP line at [WalletDropdown.tsx:296](../../../frontend/src/components/WalletDropdown.tsx#L296) with a two-row balance block plus an inline link:

```
Wallet  1,234 PP
Chips   87,500 PP           Bank →
```

`Bank →` opens the Bank page (same target as the header link). No controls in the dropdown — this is purely glance.

**Data sources:**
- Wallet PP: `pp_ledger.icrc1_balance_of({ owner = P, subaccount = null })`
- Chip PP: `pp_ledger.icrc1_balance_of({ owner = shenanigans, subaccount = chip_sub(P) })` (already wired via `useGetPonziPoints`)

### Header nav link

Add a "Bank" link next to "Docs" in the main header. Plain text link, same visual weight as Docs.

**Badge rules:**
- Badge activates **only when one or more cash-out entries are unlocked and ready to claim**.
- Badge shows the count of claimable entries (not total pending, not "unlocking soon").
- Off in all other states (no pending, pending but locked, etc.).
- Rationale: the badge's only job is "come claim your money" — actionable. "Something will unlock tomorrow" is noise.

### Bank page

Top-to-bottom layout:

#### 1. Summary row

Three items on one line:

- **Wallet PP** — `pp_ledger` balance at `(P, null)`.
- **Chips PP** — `pp_ledger` balance at `(shenanigans, chip_sub(P))`.
- **Allowance indicator** (right-aligned) — current approved allowance, with a "Revoke" action. Shows `∞` when set to the unlimited default; otherwise the specific amount.

Small footnote under the row:

> PP earned from deposits and gameplay lands in chips within ~10 seconds.

This quietly discloses observer lag without alarming anyone.

#### 2. Bridge card

One card with a direction toggle (flip arrow in the center). The form reveals direction-specific UI:

**Direction: Wallet → Chips (Deposit)**

- Amount input, minimum **5,000 PP** (matches `MIN_DEPOSIT` on shenanigans; enforce client-side with a tooltip, backend rejects regardless).
- First-ever use (no existing allowance): CTA reads "Approve allowance," then "Deposit." Two signatures.
- Subsequent uses (allowance exists and covers amount): single "Deposit" CTA. One signature.
- Helper text under input: *"PP you deposit becomes chips — spendable on spells, movable by the game."*

**Direction: Chips → Wallet (Cash out)**

- Amount input. No minimum.
- Prominent lockup warning card (amber): *"7-day lockup. Queued PP stays in chips and can still be hit by spells during the lockup."*
- CTA: "Queue cash-out."

**Empty state (0 chips, first-time user):**
Bridge is pre-set to Wallet → Chips direction, with a header on the card:

> You don't have any chips yet. Deposit PP from your wallet to play.

#### 3. Pending queue card

List of all pending (not yet claimed, not yet cancelled) cash-out entries, ordered **FIFO** by `claimableAfter` timestamp ascending. Each row surfaces:

- **Normal** (chips cover queued amount):
  `50,000 PP · unlocks in 18h · [Cancel]`
- **Shortfall** (chips < queued):
  `50,000 PP · unlocks in 18h · ⚠ spells reduced this to ~12,000 · [Cancel]`
  The "~12,000" is computed live as `min(queued_amount, current_chip_balance)`, re-queried with the pending list. For multiple pending entries, chips are attributed FIFO — oldest entry gets first claim on the available chip balance, remaining entries show reduced payouts against what's left.
- **Claimable** (unlocked):
  `50,000 PP · ready now · [Claim]`
  Shortfall display applies here too — if chips are short at claim time, the row shows the effective payout before the player commits.

**Empty state:** *"No pending cash-outs."*

## Mechanics

### Allowance model

- First deposit triggers an `icrc2_approve` for the unlimited ICRC-1 sentinel (`2^64 − 1`) from the player's main account, with shenanigans as spender.
- Summary row reads the current allowance (`pp_ledger.icrc2_allowance({ account = (P, null), spender = (shenanigans, null) })`) and displays it.
- "Revoke" sets allowance to 0 via `icrc2_approve(0)`. Next deposit re-prompts.
- Rationale: players are already trusting shenanigans with all chip-subaccount custody. Per-deposit signature friction has no real safety gain. The revoke escape hatch covers anyone who wants to rescope.
- Current [useApproveForDeposits](../../../frontend/src/hooks/useQueries.ts#L1072) approves `depositAmount * 10` — replace with unlimited default.

### Cancel cash-out (new backend method)

Add to shenanigans:

```motoko
public shared ({ caller }) func cancelCashOut(id : Nat) : async Result<(), Text> {
    // Look up entry by id, verify caller == entry.player.
    // Verify not already claimed, not already cancelled.
    // Flip entry to cancelled state.
    // No ledger call — PP never left the chip subaccount.
};
```

Semantics:
- Cancellation is **free** (no burn, no fee).
- PP was never moved out of the chip subaccount during queue window; the entry is purely metadata. Flipping to cancelled removes it from the "pending" view. No ledger work needed.
- Cancellation **does not** grant any timer credit. Re-queuing waits a fresh 7 days. Anti-shuttle friction intact.
- Rationale: 7-day lockup is the anti-shuttle friction. Locking the decision on top of that is friction tax for no gameplay benefit. Misclicks happen.

Query filter:
- `getMyCashOuts` should return non-cancelled entries only (or include a `cancelled` flag that the frontend filters on). TBD at implementation — either approach works.

### Observer lag disclosure

The shenanigans observer polls backend every ~10s to mint PP for gameplay events. After a `createGame` or dealer top-up, the player's chip balance updates on the next tick.

The Bank page handles this with:
- The footnote under the summary row (above).
- Polling-based refresh on the balance queries (already in `useGetPonziPoints` via `refetchInterval`).

No optimistic UI updates. Wait for the real chip balance to arrive from `pp_ledger`. Cosmetic lag; acceptable.

## Edge cases

- **Player tries to deposit < 5,000 PP** → frontend validation; if bypassed, backend `depositChips` rejects with an error the UI surfaces.
- **Player tries to cash out more than chip balance** → backend rejects at `requestCashOut`. UI should validate client-side first (chip balance query), but backend is the authority.
- **Player queues cash-out, spells drain chips to zero, player claims** → claim pays 0 PP. Entry is marked claimed. Row disappears (or moves to a "claimed" style if we want a short-lived visual confirmation; TBD at implementation).
- **Multiple pending entries, chips cover some but not all** → FIFO: oldest entry draws first from current chips in the shortfall display. Backend claim enforces the actual chip balance at claim time regardless of display order.
- **Player cancels while a race condition is in flight (e.g., spell cast same tick)** → cancel only flips metadata; chip balance is whatever the ledger says after the spell. No conflict.
- **Allowance is lower than requested deposit** → frontend detects mismatch on the summary row's allowance read; deposit CTA switches back to "Approve allowance" flow.
- **Player revokes allowance mid-session** → summary row reflects `0`; next deposit re-prompts. Existing chips unaffected.

## Frontend components (rough inventory)

- **`BankPage.tsx`** (rename of `ChipWallet.tsx`) — top-level page at the new route.
- **`BankSummary.tsx`** — balance row + allowance indicator + revoke action + observer-lag footnote.
- **`BridgeCard.tsx`** — direction toggle, amount input, direction-specific CTA logic.
- **`PendingQueueCard.tsx`** — list of pending entries with live shortfall computation, cancel/claim actions.
- **`WalletDropdown.tsx`** — modify to show two-row balance + `Bank →` link.
- **`Header` / main nav** — add Bank link with badge.

New hooks needed:
- `useAllowance()` — reads `icrc2_allowance` for `(P, null) → (shenanigans, null)`.
- `useRevokeAllowance()` — mutation that calls `icrc2_approve(0)`.
- `useCancelCashOut()` — mutation against new `shenanigans.cancelCashOut(id)`.
- `useWalletPp()` / `useChipPp()` — may already exist inside `useGetPonziPoints`; extract if needed for the dropdown to keep them separate.

Existing hooks reused as-is: `useDepositChips`, `useRequestCashOut`, `useClaimCashOut`, `usePendingCashOuts`.

## Backend addition

Single new method on `shenanigans/main.mo`:

- `cancelCashOut(id : Nat) : async Result<(), Text>` — owner-checked, flips a pending entry to cancelled. No ledger call.

If `getMyCashOuts` currently returns cancelled entries, add filtering (either server-side or client-side; server-side preferred to save round-trips).

Admin setter for `CASH_OUT_DELAY_SECONDS` already exists (from prior spec).

## Migration / rollout

- No on-chain state migration needed. The change is UI + one new method.
- Existing `#chips` route continues to work during rollout; the new nav item targets the same (eventually renamed) page.
- No player-visible lockups need to be honored specially — existing pending cash-outs carry over unchanged.

## Open questions

None blocking. Implementation-time decisions:

- **Cancelled entry retention** — soft-delete vs hard-delete in shenanigans state. Soft-delete is safer for debuggability; hard-delete is cleaner state. Pick at implementation.
- **Claimed row UX** — after clicking "Claim" with a successful payout, do we show a brief success state in the row before it disappears, or just remove immediately? Cosmetic, decide at implementation.
- **Mobile layout for the Bank page** — bridge card and queue card should stack vertically; summary row wraps. Standard responsive treatment.

## Out of scope (deferred)

- Peer-to-peer PP send from main wallet (supported by `pp_ledger` natively; separate feature).
- Activity / transaction history for chips.
- Per-deposit allowance granularity (slider).
- Push notifications for claimable cash-outs (badge-only for now).
- Bulk cancel / bulk claim (one-at-a-time is fine for v1).
