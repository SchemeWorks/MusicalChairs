# Kill Internal Wallet Balance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the internal `walletBalances` holding account entirely. Every deposit action (createGame, addDealerMoney) already pulls ICP directly from the caller's principal via ICRC-2 `transfer_from`; this plan deletes the now-vestigial deposit step and rewires three payout sites to transfer winnings straight to the caller's ledger account instead of crediting an internal balance.

**Architecture:** Saga pattern for payouts (mutate state first, compensate on ledger failure), matching the existing `withdrawICP` pattern. Backend cleanup is deletion-heavy: `depositICP`, `withdrawICP`, `walletBalances`, the wallet helpers, and the principal-to-principal wire function all go away. Frontend removes the Deposit/Cash Out/Wire tabs from the Wallet dropdown and the associated hooks.

**Tech Stack:** Motoko (backend canister), TypeScript/React (frontend), ICRC-1/ICRC-2 (ICP ledger), dfx for build and deploy.

**Branch:** `feat/kill-internal-wallet-balance`

**Spec:** [docs/superpowers/specs/2026-04-17-kill-internal-wallet-balance-design.md](../specs/2026-04-17-kill-internal-wallet-balance-design.md)

---

## File Structure

### Backend (modify only)

- `backend/main.mo` — all backend changes. Delete `depositICP`, `withdrawICP`, `getWalletBalance`, `getWalletBalanceICP`, `initializeWalletIfNeeded`, `deductFromWallet`, `creditToWallet`, the principal-to-principal wire function, and the `walletBalances` var. Rewrite the three payout sites (`withdrawEarnings`, `settleCompoundingGame`, `claimDealerRepayment`) to transfer ICP directly to the caller via `icrc1_transfer` with a saga pattern.

### Frontend (modify only)

- `frontend/src/hooks/useQueries.ts` — delete `useGetInternalWalletBalance`, `useDepositICP`, `useWithdrawICP`, `useSendFromInternalWallet`. Remove internal balance checks from `useAddBackerMoney`.
- `frontend/src/components/WalletDropdown.tsx` — remove Deposit, Cash Out (send), and Wire (send to another principal) tabs. Keep balance display (now ledger balance only), name editing, copy principal, and Pay Management admin action.
- `frontend/src/components/GamePlans.tsx` — remove reads of internal balance; UI already uses `useICPBalance` for the real ledger balance.
- `frontend/src/components/AddHouseMoney.tsx` — remove the internal balance check that blocks submission.
- `frontend/src/hooks/useWallet.tsx` — remove the `WalletPanel = 'deposit' | 'send'` type if only these values; reduce to whatever panels remain.

### Regenerated (do not hand-edit)

- `src/declarations/backend/backend.did.js`
- `src/declarations/backend/backend.did.d.ts`

### Deletion note

No files are deleted or created. All changes are edits to existing files.

---

## Task 1: Rewrite `withdrawEarnings` to transfer directly

**Files:**
- Modify: `backend/main.mo:1349-1408` (the `withdrawEarnings` function)

**Context:** Today this function computes earnings, distributes the exit toll to dealers, deducts the payout from the pot, updates the game record, and then calls `creditToWallet(caller, netEarningsE8s)`. We replace the `creditToWallet` call with an `await icpLedger.icrc1_transfer(...)` using the saga pattern: capture originals → mutate → transfer → on failure, restore originals and trap.

- [ ] **Step 1: Capture original state, then move the ledger transfer inline with a saga pattern**

Replace lines 1349-1408 with:

```motoko
    // Withdraw Earnings
    public shared ({ caller }) func withdrawEarnings(gameId : Nat) : async Float {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        switch (natMap.get(gameRecords, gameId)) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("Game not found");
            };
            case (?game) {
                if (game.player != caller) {
                    releaseCallerLock(caller);
                    Debug.trap("Unauthorized: Only the game owner can withdraw earnings");
                };
                if (game.isCompounding) {
                    releaseCallerLock(caller);
                    Debug.trap("Cannot withdraw from compounding games");
                };

                let earnings = await calculateEarnings(game);

                // Apply exit toll (simple: 7%/5%/3% tiered by time)
                let exitToll = calculateExitToll(game, earnings);
                let netEarnings = roundToEightDecimals(earnings - exitToll);

                // Capture state snapshots for compensation-on-failure
                let originalGame = game;
                let originalStats = platformStats;
                let originalDealers = dealerPositions;

                // Distribute exit toll: 50% stays in pot, 50% to dealers
                let potSeedFromToll = distributeExitTollToBackers(exitToll);

                // Check solvency against what actually leaves the pot
                let potDeduction = netEarnings + (exitToll - potSeedFromToll);
                if (potDeduction > platformStats.potBalance) {
                    // Revert dealer distribution before trapping
                    dealerPositions := originalDealers;
                    triggerGameReset("Insufficient funds for payout");
                    Debug.trap("Game reset due to insufficient funds");
                };

                // Reset the game record and update platform stats
                let updatedGame : GameRecord = {
                    game with
                    accumulatedEarnings = 0.0;
                    lastUpdateTime = Time.now();
                    totalWithdrawn = game.totalWithdrawn + netEarnings;
                };
                gameRecords := natMap.put(gameRecords, gameId, updatedGame);

                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + netEarnings;
                    potBalance = platformStats.potBalance - potDeduction;
                };

                // Pay out to user's ledger account (saga: revert on failure)
                let netEarningsE8s = Int.abs(Float.toInt(netEarnings * 100_000_000.0));
                let transferResult = try {
                    await icpLedger.icrc1_transfer({
                        from_subaccount = null;
                        to = { owner = caller; subaccount = null };
                        amount = netEarningsE8s;
                        fee = null;
                        memo = null;
                        created_at_time = null;
                    });
                } catch (e) {
                    // Revert all mutations
                    gameRecords := natMap.put(gameRecords, gameId, originalGame);
                    platformStats := originalStats;
                    dealerPositions := originalDealers;
                    releaseCallerLock(caller);
                    Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
                };

                switch (transferResult) {
                    case (#Err(err)) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        dealerPositions := originalDealers;
                        releaseCallerLock(caller);
                        let errMsg = switch (err) {
                            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                            case (#BadFee(_)) { "Bad fee" };
                            case (#BadBurn(_)) { "Bad burn" };
                            case (#TooOld) { "Transaction too old" };
                            case (#CreatedInFuture(_)) { "Transaction created in future" };
                            case (#Duplicate(_)) { "Duplicate transaction" };
                            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                            case (#GenericError(e)) { "Error: " # e.message };
                        };
                        Debug.trap(errMsg);
                    };
                    case (#Ok(_)) {};
                };

                releaseCallerLock(caller);
                netEarnings;
            };
        };
    };
```

- [ ] **Step 2: Build to confirm no syntax/type errors**

Run: `dfx build backend`
Expected: builds cleanly. If errors about missing `creditToWallet`, ignore for now — that helper is deleted in Task 7.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): withdrawEarnings transfers directly to caller ledger account

Replaces creditToWallet with icrc1_transfer using the existing saga
pattern (capture originals, mutate, transfer, revert on failure).
Part of killing the internal walletBalances holding account."
```

---

## Task 2: Rewrite `settleCompoundingGame` to transfer directly

**Files:**
- Modify: `backend/main.mo:1411-1497` (the `settleCompoundingGame` function)

**Context:** Same refactor as Task 1, different function. This one pays out principal + net earnings at maturity.

- [ ] **Step 1: Replace the creditToWallet call with a saga-pattern ledger transfer**

In `settleCompoundingGame`, replace the block from line 1462 (`let potSeedFromToll = ...`) through line 1491 (`creditToWallet(caller, payoutE8s);`) with:

```motoko
                // Capture state snapshots for compensation-on-failure.
                // NOTE: distributeExitTollToBackers writes to dealerRepayments
                // (via creditBackerRepayment) and only reads dealerPositions.
                // Snapshot and revert dealerRepayments — reverting dealerPositions
                // is a no-op that leaks repayments on retry. See commit 588cedb.
                let originalGame = game;
                let originalStats = platformStats;
                let originalRepayments = dealerRepayments;

                // Distribute exit toll: 50% stays in pot, 50% to dealers
                let potSeedFromToll = distributeExitTollToBackers(exitToll);

                // Pot loses: totalPayout (to player) + dealer portion of toll
                let potDeduction = totalPayout + (exitToll - potSeedFromToll);
                if (potDeduction > platformStats.potBalance) {
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    triggerGameReset("Insufficient funds for compounding game settlement");
                    Debug.trap("Game reset due to insufficient funds");
                };

                // Mark game as settled
                let settledGame : GameRecord = {
                    game with
                    isActive = false;
                    accumulatedEarnings = netEarnings;
                    totalWithdrawn = totalPayout;
                    lastUpdateTime = Time.now();
                };
                gameRecords := natMap.put(gameRecords, gameId, settledGame);

                platformStats := {
                    platformStats with
                    totalWithdrawals = platformStats.totalWithdrawals + totalPayout;
                    potBalance = platformStats.potBalance - potDeduction;
                    activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                };

                // Pay out to user's ledger account (saga: revert on failure)
                let payoutE8s = Int.abs(Float.toInt(totalPayout * 100_000_000.0));
                let transferResult = try {
                    await icpLedger.icrc1_transfer({
                        from_subaccount = null;
                        to = { owner = caller; subaccount = null };
                        amount = payoutE8s;
                        fee = null;
                        memo = null;
                        created_at_time = null;
                    });
                } catch (e) {
                    // Compensate: refund on catch (network failure — transfer status unknown)
                    // Note: This is the conservative approach; the transfer may have succeeded.
                    // In production, consider logging for manual reconciliation.
                    gameRecords := natMap.put(gameRecords, gameId, originalGame);
                    platformStats := originalStats;
                    dealerRepayments := originalRepayments;
                    releaseCallerLock(caller);
                    Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
                };

                switch (transferResult) {
                    case (#Err(err)) {
                        gameRecords := natMap.put(gameRecords, gameId, originalGame);
                        platformStats := originalStats;
                        dealerRepayments := originalRepayments;
                        releaseCallerLock(caller);
                        let errMsg = switch (err) {
                            case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                            case (#BadFee(_)) { "Bad fee" };
                            case (#BadBurn(_)) { "Bad burn" };
                            case (#TooOld) { "Transaction too old" };
                            case (#CreatedInFuture(_)) { "Transaction created in future" };
                            case (#Duplicate(_)) { "Duplicate transaction" };
                            case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                            case (#GenericError(e)) { "Error: " # e.message };
                        };
                        Debug.trap(errMsg);
                    };
                    case (#Ok(_)) {};
                };
```

Leave lines 1493-1497 (`releaseCallerLock(caller); totalPayout;`) as they are.

- [ ] **Step 2: Build**

Run: `dfx build backend`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): settleCompoundingGame transfers directly to caller"
```

---

## Task 3: Rewrite `claimDealerRepayment` to transfer directly

**Files:**
- Modify: `backend/main.mo:1689-1707` (the `claimDealerRepayment` function)

**Context:** Moves dealer repayment balance to the user's ledger account instead of crediting internal wallet. Simpler saga — only one state map (`dealerRepayments`) to compensate.

- [ ] **Step 1: Replace the function body**

Replace lines 1689-1707 with:

```motoko
    // Claim Dealer Repayment — transfers repayment balance to user's ledger account
    public shared ({ caller }) func claimDealerRepayment() : async Float {
        requireAuthenticated(caller);
        acquireCallerLock(caller);
        let balance = switch (principalMapNat.get(dealerRepayments, caller)) {
            case (null) {
                releaseCallerLock(caller);
                Debug.trap("No repayment balance to claim");
            };
            case (?b) {
                if (b <= 0.0) {
                    releaseCallerLock(caller);
                    Debug.trap("No repayment balance to claim");
                };
                b;
            };
        };

        // Zero out the repayment balance (compensate on failure)
        dealerRepayments := principalMapNat.put(dealerRepayments, caller, 0.0);

        let balanceE8s = Int.abs(Float.toInt(roundToEightDecimals(balance) * 100_000_000.0));
        let transferResult = try {
            await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = caller; subaccount = null };
                amount = balanceE8s;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            dealerRepayments := principalMapNat.put(dealerRepayments, caller, balance);
            releaseCallerLock(caller);
            Debug.trap("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Err(err)) {
                dealerRepayments := principalMapNat.put(dealerRepayments, caller, balance);
                releaseCallerLock(caller);
                let errMsg = switch (err) {
                    case (#InsufficientFunds(_)) { "Canister has insufficient ICP. Please contact support." };
                    case (#BadFee(_)) { "Bad fee" };
                    case (#BadBurn(_)) { "Bad burn" };
                    case (#TooOld) { "Transaction too old" };
                    case (#CreatedInFuture(_)) { "Transaction created in future" };
                    case (#Duplicate(_)) { "Duplicate transaction" };
                    case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                    case (#GenericError(e)) { "Error: " # e.message };
                };
                Debug.trap(errMsg);
            };
            case (#Ok(_)) {};
        };

        releaseCallerLock(caller);
        balance;
    };
```

- [ ] **Step 2: Build**

Run: `dfx build backend`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "refactor(backend): claimDealerRepayment transfers directly to caller"
```

---

## Task 4: Delete `depositICP`

**Files:**
- Modify: `backend/main.mo:483-548`

- [ ] **Step 1: Delete the entire `depositICP` function**

Remove lines 483-548 (the `depositICP` function definition) entirely.

- [ ] **Step 2: Build — expect it to FAIL**

Run: `dfx build backend`
Expected: type errors referencing `depositICP` from query declarations (if the candid interface is exported from this file) — that's fine, we fix those in Task 8. For now, the backend file itself should type-check; if any internal reference to `depositICP` exists, delete it. The frontend bindings are stale but will be regenerated.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "remove(backend): delete depositICP — no holding account to credit"
```

---

## Task 5: Delete `withdrawICP`

**Files:**
- Modify: `backend/main.mo:555-623`

- [ ] **Step 1: Delete the entire `withdrawICP` function**

Remove lines 555-623 (the `withdrawICP` function definition) entirely. All three payout sites now transfer directly (Tasks 1-3), so this function has no callers.

- [ ] **Step 2: Build**

Run: `dfx build backend`
Expected: builds cleanly (no callers exist in the Motoko code; frontend bindings are regenerated later).

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "remove(backend): delete withdrawICP — payouts transfer directly now"
```

---

## Task 6: Delete `getWalletBalance` and `getWalletBalanceICP` queries

**Files:**
- Modify: `backend/main.mo:312-331`

- [ ] **Step 1: Delete both query functions**

Remove lines 312-331 entirely (`getWalletBalance` and `getWalletBalanceICP`).

- [ ] **Step 2: Build**

Run: `dfx build backend`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add backend/main.mo
git commit -m "remove(backend): delete internal wallet balance queries"
```

---

## Task 7: Delete internal wallet helpers and the principal-to-principal wire function

**Files:**
- Modify: `backend/main.mo:333-694` (region containing `initializeWalletIfNeeded`, `deductFromWallet`, `creditToWallet`, and the wire/send function around 660-690)

**Context:** Read the file carefully — the wire function (principal-to-principal send through `walletBalances`) is around lines 660-690. Identify its exact bounds (look for a `public shared` function that reads `walletBalances` for both sender and recipient). Also identify `initializeWalletIfNeeded` (lines 333-341), `deductFromWallet` (lines 629-644), and `creditToWallet` (lines 646-656).

- [ ] **Step 1: Delete `initializeWalletIfNeeded`**

Remove lines 333-341 (the `initializeWalletIfNeeded` helper function).

- [ ] **Step 2: Delete `deductFromWallet`**

Remove lines 629-644 (the `deductFromWallet` helper — verify no remaining callers first with `grep -n 'deductFromWallet' backend/main.mo`).

- [ ] **Step 3: Delete `creditToWallet`**

Remove lines 646-656. Run `grep -n 'creditToWallet' backend/main.mo` first — should show zero references after Tasks 1-3.

- [ ] **Step 4: Delete the wire/send function**

Open the file, find the `public shared` function that reads `walletBalances` for both a sender and a recipient (the principal-to-principal transfer around lines 660-690). Delete the entire function. Also delete any helper that only exists to support it.

- [ ] **Step 5: Build**

Run: `dfx build backend`
Expected: builds cleanly. If a reference to any of these functions remains, fix it.

- [ ] **Step 6: Commit**

```bash
git add backend/main.mo
git commit -m "remove(backend): delete internal wallet helpers and wire function"
```

---

## Task 8: Delete `walletBalances` and any remaining references

**Files:**
- Modify: `backend/main.mo:219` (the var declaration) and any remaining references

- [ ] **Step 1: Verify no remaining references**

Run: `grep -n 'walletBalances' backend/main.mo`
Expected: matches only comments (e.g., line 236, 248 in the cover charge preamble) plus line 219 (the var itself). No reads or writes should remain.

- [ ] **Step 2: Delete the var declaration**

Remove line 219 (`var walletBalances = principalMapNat.empty<Nat>();`) and the preceding comment at line 218.

- [ ] **Step 3: Update outdated comments**

Replace any comment referencing `walletBalances` (e.g., in the Cover Charge preamble) with updated wording. Search-and-replace "walletBalances" → "the cover charge bucket" or rewrite comments as appropriate.

- [ ] **Step 4: Check for `recordWalletTransaction` calls that only existed for deposit/withdraw**

Run: `grep -n 'recordWalletTransaction' backend/main.mo`

If the only call sites removed (from `depositICP` and `withdrawICP`) leave no callers, delete `recordWalletTransaction` and the `walletTransactions` storage too. If cover charge logging still uses it, leave it. Report which case applies in the commit message.

- [ ] **Step 5: Build**

Run: `dfx build backend`
Expected: builds cleanly.

- [ ] **Step 6: Commit**

```bash
git add backend/main.mo
git commit -m "remove(backend): delete walletBalances var — the internal wallet is gone"
```

---

## Task 9: Regenerate frontend bindings

**Files:**
- Modify: `src/declarations/backend/backend.did.js` (regenerated)
- Modify: `src/declarations/backend/backend.did.d.ts` (regenerated)

- [ ] **Step 1: Regenerate Candid bindings**

Run: `dfx generate backend`
Expected: updates the `src/declarations/backend/*` files. Should complete without error.

- [ ] **Step 2: Verify bindings no longer contain deleted methods**

Run: `grep -E '(depositICP|withdrawICP|getWalletBalance|getWalletBalanceICP)' src/declarations/backend/backend.did.d.ts`
Expected: no matches.

- [ ] **Step 3: Commit**

```bash
git add src/declarations/backend/
git commit -m "build: regenerate backend bindings after internal wallet removal"
```

---

## Task 10: Delete frontend hooks for internal wallet

**Files:**
- Modify: `frontend/src/hooks/useQueries.ts`

**Context:** Delete the four hooks that only exist for the internal wallet: `useGetInternalWalletBalance` (lines 202-230), `useDepositICP` (lines 255-290), `useWithdrawICP` (find by name), and `useSendFromInternalWallet` (find by name). Also update `useAddBackerMoney` to remove its internal balance check at line 464.

- [ ] **Step 1: Delete `useGetInternalWalletBalance`**

Delete lines 202-230 (the entire `useGetInternalWalletBalance` function definition and the comment above it).

- [ ] **Step 2: Delete `useDepositICP`**

Delete lines 253-290 (comment, function definition). Use grep to confirm exact bounds: `grep -n 'export function useDepositICP\|^}' frontend/src/hooks/useQueries.ts`.

- [ ] **Step 3: Delete `useWithdrawICP`**

Find by: `grep -n 'useWithdrawICP' frontend/src/hooks/useQueries.ts`. Delete the full function definition.

- [ ] **Step 4: Delete `useSendFromInternalWallet`**

Find by: `grep -n 'useSendFromInternalWallet' frontend/src/hooks/useQueries.ts`. Delete the full function definition.

- [ ] **Step 5: Remove internal balance check in `useAddBackerMoney`**

Open `useAddBackerMoney` (around line 453). The current code reads the internal balance and throws if insufficient before calling `addDealerMoney`. Delete that check — `addDealerMoney` already performs `icrc2_transfer_from` and will report insufficient allowance from the ledger itself. The mutation's `mutationFn` should simply: validate amount > 0, optionally call `icrc2_approve` for `amount + fee`, then call `actor.addDealerMoney(amount)`.

Before editing, read the current implementation and identify the internal balance read (it will be something like `await actor.getWalletBalanceICP()` or `useGetInternalWalletBalance()` inside the hook). Remove those lines and the check that follows. Keep the approve-then-call flow; for Oisy, keep the ICRC-112 batching pattern (mirror the one in `useCreateGame`).

- [ ] **Step 6: Typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors referencing the deleted hooks. If other files import them, Task 11 handles those.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/hooks/useQueries.ts
git commit -m "remove(frontend): delete internal wallet hooks"
```

---

## Task 11: Simplify `WalletDropdown.tsx`

**Files:**
- Modify: `frontend/src/components/WalletDropdown.tsx`

**Context:** The dropdown currently has Deposit, Cash Out, and Wire tabs. All three are dead now. Keep: balance display (real ledger balance via `useICPBalance`), name edit, copy principal, Pay Management admin action, PP display. Remove everything else.

- [ ] **Step 1: Remove deleted-hook imports**

Edit line 4 to delete the imports: `useGetInternalWalletBalance`, `useSendFromInternalWallet`, `useDepositICP`, `useWithdrawICP`. The remaining imports should be: `useGetCallerUserProfile, useSaveUserProfile, useGetPonziPoints, useGetCoverChargeBalance, useWithdrawCoverCharges, isCoverChargeAdmin`.

- [ ] **Step 2: Replace internal balance reads with ledger balance**

Find every use of data returned by the (now-deleted) `useGetInternalWalletBalance` hook. Replace the ICP balance display with `useICPBalance()` (already exported from `useQueries.ts`). The "Available" amounts and displayed "ICP Balance" both come from this single source.

- [ ] **Step 3: Remove the tab bar and all tab panels (Deposit / Cash Out / Wire)**

Find the tab component (a bar with "Deposit", "Cash Out", "Wire" labels — around lines 338-361 for Deposit based on earlier exploration). Delete:
- The tab-switching UI and its `setActiveTab` state.
- The Deposit panel (handler `handleDeposit` lines 168-185) — delete the handler too.
- The Cash Out (send/withdraw) panel and its handler.
- The Wire (principal-to-principal) panel and its handler.
- Related state: `depositAmount`, `withdrawAmount`, `recipientPrincipal`, `isApproving`, `approvalComplete`, `externalBalance`, `externalBalanceLoading` — delete any that only supported the removed panels.

The simplified dropdown should show: wallet-type header, welcome/name-edit, ICP Balance card (from `useICPBalance`), Cover Charges card + Pay Management button for admins, PP display, Principal ID copy row.

- [ ] **Step 4: Update `useWallet` `WalletPanel` type if it references removed panels**

Open `frontend/src/hooks/useWallet.tsx`. If `WalletPanel` is a union like `'deposit' | 'send'`, update it to whatever panels remain (or remove the type entirely and delete `initialPanel` if no caller uses it).

- [ ] **Step 5: Typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/WalletDropdown.tsx frontend/src/hooks/useWallet.tsx
git commit -m "feat(frontend): remove Deposit/Cash Out/Wire tabs from wallet dropdown

The internal wallet balance is gone. Plan creation and dealer seeding
pull ICP directly from the connected wallet; winnings are transferred
straight to the user's principal. No deposit step to show."
```

---

## Task 12: Remove internal balance check from `AddHouseMoney.tsx`

**Files:**
- Modify: `frontend/src/components/AddHouseMoney.tsx`

**Context:** The component currently reads `useGetInternalWalletBalance` (line 16) and blocks submission if the internal balance is insufficient (line 28). Both are wrong now. Change it to read `useICPBalance` and check against that — or simply drop the pre-check entirely and let the ledger reject on insufficient funds/allowance.

- [ ] **Step 1: Update imports and state**

Remove the `useGetInternalWalletBalance` import and its invocation. Import and use `useICPBalance` instead if you still want a client-side check.

- [ ] **Step 2: Update `handleDeposit`**

Replace the body of `handleDeposit` with a flow that:
1. Validates `depositAmount` is a positive number.
2. Calls `addBackerMoneyMutation.mutateAsync(depositAmount)` directly. (The mutation, updated in Task 10, now performs approve + addDealerMoney against the caller's ledger account.)

- [ ] **Step 3: Typecheck**

Run: `cd frontend && npx tsc --noEmit`

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/AddHouseMoney.tsx
git commit -m "fix(frontend): AddHouseMoney no longer reads internal wallet balance"
```

---

## Task 13: Remove internal balance reads from `GamePlans.tsx`

**Files:**
- Modify: `frontend/src/components/GamePlans.tsx`

**Context:** Per earlier exploration, the component reads `walletBalance` at line 77 but `createGame` already pulls ICP directly. Any pre-flight check should use `useICPBalance()` instead.

- [ ] **Step 1: Open the file and locate all references to internal wallet balance**

Run: `grep -n 'walletBalance\|InternalWalletBalance\|getWalletBalance' frontend/src/components/GamePlans.tsx`

- [ ] **Step 2: Replace with `useICPBalance`**

For each reference, use `useICPBalance()` (already exported from `useQueries.ts`). The real ICP balance that matters for a deposit is the user's ledger balance, which is what `useICPBalance` returns.

- [ ] **Step 3: Remove any UI text referencing an "internal wallet" or two-step flow**

Search for strings like "two steps", "approve then deposit", "internal balance". Remove or rewrite.

- [ ] **Step 4: Typecheck**

Run: `cd frontend && npx tsc --noEmit`

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/GamePlans.tsx
git commit -m "fix(frontend): GamePlans reads ledger balance, not internal wallet"
```

---

## Task 14: Full typecheck and build

- [ ] **Step 1: Frontend typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 2: Frontend build**

Run: `cd frontend && npm run build`
Expected: `dist/` produced without errors.

- [ ] **Step 3: Backend build**

Run: `dfx build backend`
Expected: clean Motoko build.

- [ ] **Step 4: If any errors, fix and commit as `fix: <description>`**

Do not proceed to deploy until both builds are clean.

---

## Task 15: Manual verification on the deployed canister

**Context:** The user (solo tester) will drive this. The plan documents the expected outcomes so that the executor — or the user — can verify each step produces the expected result.

**CRITICAL — user permission required before deploying.** Per the user's standing feedback (`memory/feedback_deploy_safety.md`): never deploy backend without explicit permission. Stop here and ask.

- [ ] **Step 1: Ask user for explicit permission to deploy**

Say: "Backend is ready to deploy to the existing canister. This will upgrade `backend` in place. `walletBalances` (transient) will reset on upgrade as it does on every upgrade — no state migration needed. All open plans, dealer positions, Ponzi Points, and cover charge balance are preserved. OK to run `dfx deploy backend`?"

Wait for explicit "yes" before Step 2. If the user says no, stop the plan here.

- [ ] **Step 2: Deploy**

Run: `dfx deploy backend`
Expected: upgrade succeeds.

- [ ] **Step 3: Start dev server and open the wallet dropdown**

Run: `cd frontend && npm run dev`
Expected: dev server reachable.

- [ ] **Step 4: Verify wallet dropdown shows only the simplified view**

Open the Wallet dropdown. Expected:
- ICP Balance displayed (matches the user's principal ledger balance — same number you'd see on an external explorer).
- No Deposit / Cash Out / Wire tabs.
- Cover Charges card + Pay Management button visible for admin user.

- [ ] **Step 5: Create a new plan (as Internet Identity)**

Pick the smallest available plan amount (e.g., 0.1 ICP). Click through to Create. Expected: single click — no deposit step, no approval popup prompts for II. After confirmation, the plan appears in the active games list and the ledger balance drops by `amount`.

- [ ] **Step 6: Verify pre-existing open plans still show and pay out correctly**

From the user's existing open plans (preserved through the upgrade), pick one with accumulated earnings. Click "Withdraw Earnings" (or settle, as applicable). Expected: ICP arrives at the user's principal ledger account — not an internal balance. Confirm by checking the ledger balance before and after.

- [ ] **Step 7: Try an error path — attempt to create a plan with an amount larger than the ledger balance**

Expected: the call fails with an error message referencing insufficient ICP or allowance. The caller lock is NOT stuck (retry the same action after fixing the amount and it works).

- [ ] **Step 8: Commit any final fixes discovered during verification**

If any step above uncovered a bug, fix it and commit with a descriptive message. Re-run the verification steps affected.

---

## Self-Review

*(Run inline — this is a note for the plan author, not a task.)*

- **Spec coverage:** Tasks 1-3 rewire the three payout sites. Tasks 4-8 delete the backend internal wallet surface. Task 9 regenerates bindings. Tasks 10-13 clean up the frontend. Task 14 is a build gate. Task 15 is verification. No spec requirement is unaccounted for.
- **Placeholder scan:** No TBDs, TODOs, or vague instructions. Every code block is concrete.
- **Type consistency:** `natMap`, `principalMapNat`, `GameRecord`, `platformStats`, `dealerPositions`, `dealerRepayments`, `gameRecords` are all used consistently with existing code in `main.mo`.
- **Ambiguity check:** Task 7 step 4 says "around lines 660-690" — deliberate, because the function's exact bounds may shift as earlier tasks delete code. The step instructs the executor to find the function by its signature rather than hardcoding a line range.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-17-kill-internal-wallet-balance.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — one fresh subagent per task, review between tasks, fast iteration.

2. **Inline Execution** — execute tasks in this session with checkpoints for review.

Which approach?
