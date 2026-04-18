# Kill the Internal Wallet Balance

**Date**: 2026-04-17
**Status**: Design approved; implementation pending
**Branch context**: Follows `fix/stuck-caller-lock-after-ledger-throw`

## Problem

Today, depositing ICP into a plan or seed round is a two-step flow:

1. **Approve + deposit**: User approves ICRC-2 allowance, canister calls `depositICP`, which credits an internal `walletBalances[user]` map.
2. **Spend**: User opens the wallet UI, picks a plan or seed round, backend deducts from `walletBalances[user]` and moves funds into the game pot.

This is confusing ("why do I have to deposit *again* after my ICP already shows up?"), introduces an extra ledger round-trip (extra fee, extra stuck-lock risk — see the branch this lands on top of), and has no upside for users connected via Plug or Oisy who already hold ICP in an external wallet.

## Goal

Collapse the flow to one step: every plan creation or seed round deposit pulls ICP directly from the user's principal account via ICRC-2 `transfer_from`. No holding bucket. Winnings likewise land in the user's principal account (this is already the case for Cash Out; confirm and preserve).

This works uniformly across Plug, Oisy, and Internet Identity — the only difference between wallets is how many signature popups the user sees (a wallet-level limitation, not ours).

## Non-goals

- No changes to existing open plans, dealer rounds, cover charge accounting, Ponzi Points, or game state.
- No change to persistent-allowance UX (every deposit requires its own approve; we do not grant the canister a standing allowance).
- No migration of user funds from `walletBalances` to principal accounts — only the live-test user has state, and they accept wiping it.

## Design

### Data flow

**Before:**

```
User wallet --icrc2_approve--> Ledger
Canister   --depositICP (transfer_from)--> walletBalances[user]
walletBalances[user] --createGame/addDealerMoney--> game pot
```

**After:**

```
User wallet --icrc2_approve--> Ledger
Canister   --createGame/addDealerMoney (transfer_from)--> game pot
```

The `walletBalances` map is removed. The canister never holds user funds outside of an active game pot or the `coverChargeBalance` admin bucket.

### Backend (`src/musicalChairs_backend/main.mo`)

**Remove**:

- `depositICP` entrypoint.
- `walletBalances : HashMap<Principal, Nat>` (or equivalent) and any stable-variable backing.
- `withdrawICP` *if and only if* its sole purpose was to drain the internal bucket. Verify before deleting — if it also serves other withdrawal paths, adjust accordingly.
- Any helpers that read/write `walletBalances` (balance query, internal-balance checks in createGame/addDealerMoney).

**Modify `createGame`**:

- Accept the ICP amount as a parameter (as today).
- Replace the "deduct from `walletBalances`" step with an `icrc2_transfer_from(from = caller, to = canister subaccount for this game, amount)` call.
- Preserve the existing cover-charge split: from the transferred amount, route the cover-charge portion into `coverChargeBalance` and the net into the game pot.
- Preserve the existing Ponzi Points award.
- Preserve the fix from `fix/stuck-caller-lock-after-ledger-throw` — the single `transfer_from` await must release the caller lock on any throw.

**Modify `addDealerMoney`**:

- Same pattern: `icrc2_transfer_from` from caller into the dealer pot.
- Preserve the same stuck-lock-safety fix.

**Keep untouched**:

- Game state, open plans, dealer rounds.
- `coverChargeBalance` and the "Pay Management" flow.
- Ponzi Points balance and awards.
- Cash Out (already sends `icrc1_transfer` directly to the user's principal).
- Wire (direct principal-to-principal send; still useful for some workflows).

**Upgrade migration**:

- In `postupgrade`, clear `walletBalances` stable storage (or simply drop the stable variable declaration so the upgrade discards it).
- No migration logic for existing balances — confirmed only the live-test user has any, and they accept wipe.
- All other stable state (games, Ponzi Points, cover charge) preserved as normal.

### Frontend

**Wallet.tsx (or the current wallet shell)**:

- Remove the "Deposit" tab and all its state/handlers.
- Keep "Cash Out" (winnings → user principal).
- Keep "Wire" if still useful for sending to another principal. Reassess; remove if redundant.
- The displayed "ICP Balance" becomes purely the user's principal ledger balance. Remove any display of the internal balance.

**GamePlans.tsx**:

- On plan creation: `icrc2_approve(amount + ledger_fee)` then `createGame(planType, amount, ...)`.
- Error handling: if approve succeeds but createGame fails, surface the error; the allowance will sit unused on the ledger (expected ICRC-2 behavior, no cleanup needed).

**AddHouseMoney.tsx**:

- Same pattern: `icrc2_approve` then `addDealerMoney(amount)`.

**Oisy (ICRC-112 batching)**:

- Keep the existing batched signing path in `useQueries.ts`. It already batches approve + a backend call — update the second call from `depositICP` to `createGame` / `addDealerMoney` directly.

**Plug**:

- Two sequential calls (approve, then create/addDealer). Two signature popups. Acceptable — matches standard ICP DeFi UX.

**Internet Identity**:

- Both calls fire automatically after the user clicks Create Plan. No extra popups (II authenticates canister calls without per-call prompts).

**Remove**:

- `useGetInternalWalletBalance` and any hooks/queries that read the internal balance.
- Any `depositICP` mutation hook.
- Any UI state tracking the "how much is in my internal wallet" number.

### Testing

Manual verification after deploy:

1. Create a plan as Internet Identity — confirm ICP moves directly from principal account to game pot.
2. Create a plan as Oisy (single batched signature) — confirm same.
3. Create a plan as Plug (two popups) if available — confirm same.
4. Seed a dealer round from each wallet.
5. Cash out winnings — confirm ICP lands in the user's principal account.
6. Existing pre-migration open plans display and remain playable.
7. Attempt a plan creation where the user has insufficient allowance — confirm the error surfaces cleanly, no stuck caller lock.
8. Repeat the scenario that previously triggered the stuck-lock bug — confirm it cannot occur on the new path.

### Deploy notes

- Backend upgrade drops `walletBalances` state; all other stable state preserved.
- No user-visible migration banner; the only affected user is the developer.
- Follows and does not conflict with `fix/stuck-caller-lock-after-ledger-throw`.

## Open items

None. Proceed to implementation plan.
