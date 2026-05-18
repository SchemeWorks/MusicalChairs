# ICP Transfer Fee Accounting Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the 0.0001 ICP per-payout leak from `icrc1_transfer({ fee = null, ... })` in `withdrawEarnings`, `settleCompoundingGame`, and `claimBackerRepayment` by pre-deducting the transfer fee from the recipient amount (mirroring the in-tree `sweepCoverCharges` pattern).

**Architecture:** Three-line per-function change. Zero new stable state. Zero migration. Add a reusable verification shell script that compares actual canister balance to internal accounting; run it before to demonstrate the leak (red), apply fixes, re-run to confirm zero gap (green).

**Tech Stack:** Motoko (Internet Computer), Bash + jq + bc for verification harness, dfx CLI for local replica testing.

**Reference:** Design spec at [docs/superpowers/specs/2026-05-15-icp-transfer-fee-accounting-design.md](docs/superpowers/specs/2026-05-15-icp-transfer-fee-accounting-design.md).

**Validation cadence:** Each Motoko-modifying task ends with `dfx build ponzi_math --check` for type-check. The verification script runs at Task 2 (pre-fix, must show leak) and Task 6 (post-fix, must show zero gap).

**Naming guard:** `exitToll` and `coverCharge` identifiers stay unchanged per [CLAUDE.md](CLAUDE.md). Only `Ledger.ICP_TRANSFER_FEE` (already exported by [ponzi_math/ledger.mo:31](ponzi_math/ledger.mo:31)) is referenced; no new constants.

**Mainnet deploy guard:** Per [memory/feedback_deploy_safety.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_deploy_safety.md), DO NOT run `dfx deploy ponzi_math --network ic` at any point in this plan. All deploys are `--network local`. Mainnet deploy is the user's call after PR review.

---

## Phase 1: Verification harness

### Task 1: Create the fee-invariant verification script

**Files:**
- Create: `ponzi_math/scripts/verify-fee-invariant.sh`

- [ ] **Step 1: Create the scripts directory**

Run: `mkdir -p ponzi_math/scripts`

- [ ] **Step 2: Write the verification script**

Create file `ponzi_math/scripts/verify-fee-invariant.sh` with these exact contents:

```bash
#!/usr/bin/env bash
# verify-fee-invariant.sh
#
# Compares ponzi_math's actual ICP balance to its internal accounting:
#   actual_e8s ≟ floor(potBalance * 1e8) + floor(roundSeedReserve * 1e8)
#               + sum(floor(backerRepayments * 1e8)) + coverChargeBalance
#
# Allows up to 10 e8s drift to absorb Float→Nat rounding noise across the
# three Float fields. A diff of >= 10_000 e8s indicates the historic
# fee-leak bug; a diff of <= 10 e8s indicates clean accounting.
#
# Usage:
#   ./ponzi_math/scripts/verify-fee-invariant.sh [network]
#
# network defaults to "local". For mainnet read-only verification:
#   ./ponzi_math/scripts/verify-fee-invariant.sh ic
set -euo pipefail

NETWORK="${1:-local}"
TOLERANCE_E8S=10

# Extract a Float from a Candid response like "(0.96 : float64)" -> "0.96"
parse_float() {
    sed -E 's/.*\(([0-9.eE+-]+) : float64\).*/\1/'
}

# Extract a Nat from "(96000000 : nat)" -> "96000000"
parse_nat() {
    sed -E 's/.*\(([0-9_]+)[[:space:]]*:[[:space:]]*nat\).*/\1/' | tr -d '_'
}

pot=$(dfx canister call --network "$NETWORK" ponzi_math getPlatformStats \
    | grep -oE 'potBalance = [0-9.eE+-]+' | awk '{print $3}')
seed=$(dfx canister call --network "$NETWORK" ponzi_math getRoundSeedReserve | parse_float)
cover=$(dfx canister call --network "$NETWORK" ponzi_math getCoverChargeBalance | parse_nat)
actual=$(dfx canister call --network "$NETWORK" ponzi_math getCanisterICPBalance | parse_nat)

# Sum backer repayments. Output is a vec of records; pull out every "<float> : float64"
# value and sum them.
repayments=$(dfx canister call --network "$NETWORK" ponzi_math getAllBackerRepayments \
    | grep -oE '[0-9.eE+-]+ : float64' \
    | awk '{print $1}' \
    | awk 'BEGIN{s=0} {s+=$1} END{printf "%.8f\n", s+0}')

# Compute internal accounting in e8s. bc handles the Float arithmetic precisely
# enough; we then floor each Float→e8s conversion before summing to mirror
# Motoko's Int.abs(Float.toInt(x * 1e8)) behavior.
internal_e8s=$(echo "scale=0; (${pot} * 100000000) / 1 + (${seed} * 100000000) / 1 + (${repayments} * 100000000) / 1 + ${cover}" | bc)

diff_e8s=$(echo "${actual} - ${internal_e8s}" | bc)
# Absolute value for the tolerance check
abs_diff=$(echo "if (${diff_e8s} < 0) -1 * (${diff_e8s}) else ${diff_e8s}" | bc)

printf "Network:               %s\n" "$NETWORK"
printf "Actual ICP (e8s):      %s\n" "$actual"
printf "Internal sum (e8s):    %s\n" "$internal_e8s"
printf "  potBalance:          %s ICP\n" "$pot"
printf "  roundSeedReserve:    %s ICP\n" "$seed"
printf "  sum(backerRepayments): %s ICP\n" "$repayments"
printf "  coverChargeBalance:  %s e8s\n" "$cover"
printf "Diff (actual - internal): %s e8s\n" "$diff_e8s"
printf "Tolerance:             ±%s e8s\n" "$TOLERANCE_E8S"

if [ "$(echo "${abs_diff} <= ${TOLERANCE_E8S}" | bc)" = "1" ]; then
    printf "RESULT: PASS — accounting balanced within tolerance\n"
    exit 0
else
    printf "RESULT: FAIL — gap exceeds tolerance (likely transfer-fee leak)\n"
    exit 1
fi
```

- [ ] **Step 3: Make the script executable**

Run: `chmod +x ponzi_math/scripts/verify-fee-invariant.sh`

- [ ] **Step 4: Verify the script's syntax parses**

Run: `bash -n ponzi_math/scripts/verify-fee-invariant.sh`
Expected: no output (success). If error, fix the syntax before continuing.

- [ ] **Step 5: Commit**

```bash
git add ponzi_math/scripts/verify-fee-invariant.sh
git commit -m "test(ponzi_math): add fee-invariant verification script

Compares getCanisterICPBalance to internal accounting (potBalance +
roundSeedReserve + sum(backerRepayments) + coverChargeBalance), with
a 10 e8s tolerance for Float→Nat conversion drift. Used pre- and
post-fix to demonstrate the transfer-fee leak and confirm the fix."
```

---

## Phase 2: Demonstrate the bug (RED)

### Task 2: Reproduce the leak on a local replica

**Files:** none — read-only ops + temporary local-replica state

**Pre-req:** dfx installed (`dfx --version` should print a version). If the engineer doesn't have a local ICP ledger fixture configured, set one up before continuing — the rest of this task assumes deposits and payouts work against a local ICRC-1/2 ICP ledger. (Search the repo for prior local-replica setup notes; the 2026-05-11 ponzi-math-extraction plan has working examples in its Phase 4.)

- [ ] **Step 1: Start a clean local replica**

Run: `dfx start --clean --background`
Wait until it prints "Replica API running...".

- [ ] **Step 2: Deploy ponzi_math with the dfx identity as testAdmin**

```bash
BACKEND_PRINCIPAL=$(dfx canister id backend 2>/dev/null || echo "$(dfx identity get-principal)")
dfx deploy ponzi_math --network local --argument '(record {
    backendPrincipal = principal "'$BACKEND_PRINCIPAL'";
    testAdmin = principal "'$(dfx identity get-principal)'";
})'
```

Capture the canister ID:
```bash
PONZI_MATH_ID=$(dfx canister id ponzi_math)
echo "ponzi_math = $PONZI_MATH_ID"
```

- [ ] **Step 3: Confirm baseline invariant holds on an empty canister**

```bash
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: `RESULT: PASS` — all fields zero, actual zero, diff zero.

- [ ] **Step 4: Approve ICP allowance + create a simple game**

Replace `<ICP_LEDGER>` with the local ICP ledger canister ID (e.g., what `dfx canister id ic_ledger` or your repo's ledger fixture gives).

```bash
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createGame "(variant { simple21Day }, 1.0, false)"
```

Expected: `(variant { Ok = 0 : nat })` (gameId 0). Pot is now 0.96 ICP after 4% cover charge.

- [ ] **Step 5: Backdate a game and withdraw to trigger a real payout**

```bash
NOW_NS=$(date +%s%N)
TWELVE_HOURS_AGO_NS=$((NOW_NS - 43200000000000))
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createBackdatedGame "(variant { simple21Day }, 1.0, false, $TWELVE_HOURS_AGO_NS : int)"
dfx canister call ponzi_math withdrawEarnings "(1 : nat)"
```

Expected: `(variant { Ok = <some positive float> })`. ~0.055 ICP gross earnings, less exit toll, less the 10,000 e8s ledger fee that the bug currently lets leak.

- [ ] **Step 6: Re-run the verification script — DEMONSTRATE THE BUG**

```bash
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: `RESULT: FAIL — gap exceeds tolerance (likely transfer-fee leak)`.
The "Diff" line should read approximately `-10000` e8s (actual is short of internal by one fee). If it reads 0 or near-0, the bug has NOT been reproduced — DO NOT proceed; investigate setup (wrong network, no actual payout occurred, etc.).

- [ ] **Step 7: Stop the replica (optional, save for Task 6)**

The same replica state will be re-used in Task 6 to verify the fix. Either:
- Leave it running and continue to Task 3.
- `dfx stop` now; we'll redeploy from scratch in Task 6.

Leaving it running is cleaner (the post-fix verification reproduces the same state with one extra payout).

- [ ] **Step 8: Document the demonstrated bug in a working note**

No commit — this is verification, not code. Note the observed Diff value (e.g., `-10000`) for the eventual PR description.

---

## Phase 3: Apply the fixes (GREEN)

### Task 3: Fix `withdrawEarnings`

**Files:**
- Modify: `ponzi_math/main.mo:923-933`

- [ ] **Step 1: Locate the buggy block**

In `ponzi_math/main.mo`, find this block inside `withdrawEarnings` (~line 923):

```motoko
                    let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (netEarningsE8s > 0) {
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = netEarningsE8s;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
```

- [ ] **Step 2: Apply the fix**

Use Edit to change exactly this:

Old:
```motoko
                    let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (netEarningsE8s > 0) {
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = netEarningsE8s;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
```

New:
```motoko
                    let netEarningsE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (netEarningsE8s > Ledger.ICP_TRANSFER_FEE) {
                        let transferAmount : Nat = netEarningsE8s - Ledger.ICP_TRANSFER_FEE;
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = transferAmount;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
```

Note: the guard changes from `> 0` to `> Ledger.ICP_TRANSFER_FEE`. The previous zero-amount branch is subsumed — anything from 0 to fee inclusive now skips the transfer (state changes above the transfer still apply, ledger record still gets written, insolvency reset still runs). This matches the spec's "Below-fee handling" for `withdrawEarnings`.

- [ ] **Step 3: Type-check**

Run: `dfx build ponzi_math --check`
Expected: builds cleanly (just type errors / warnings if any). If "unresolved variable Ledger" — `Ledger` should already be imported in main.mo; confirm with `grep -n '^import Ledger' ponzi_math/main.mo`. It is.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "fix(ponzi_math): pre-deduct ICP fee in withdrawEarnings

Mirrors sweepCoverCharges. Stops the 0.0001 ICP per-withdrawal leak
between internal accounting and actual canister balance. Player
receives netEarnings - 10_000 e8s; the difference is the standard
ICP ledger transfer fee, previously absorbed silently by the canister.

Guard widened from > 0 to > ICP_TRANSFER_FEE: tiny earnings below the
fee threshold skip the transfer (no money moves either way), and the
rest of the function continues as today (state, ledger record, reset)."
```

---

### Task 4: Fix `settleCompoundingGame`

**Files:**
- Modify: `ponzi_math/main.mo:1059-1069`

- [ ] **Step 1: Apply the same pattern in `settleCompoundingGame`**

Use Edit to change exactly this:

Old:
```motoko
                    let payoutE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (payoutE8s > 0) {
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = payoutE8s;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
```

New:
```motoko
                    let payoutE8s = Int.abs(Float.toInt(actualNetEarnings * 100_000_000.0));
                    if (payoutE8s > Ledger.ICP_TRANSFER_FEE) {
                        let transferAmount : Nat = payoutE8s - Ledger.ICP_TRANSFER_FEE;
                        let transferResult = try {
                            await icpLedger.icrc1_transfer({
                                from_subaccount = null;
                                to = { owner = caller; subaccount = null };
                                amount = transferAmount;
                                fee = null;
                                memo = null;
                                created_at_time = null;
                            });
```

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "fix(ponzi_math): pre-deduct ICP fee in settleCompoundingGame

Same pattern as withdrawEarnings: amount = payoutE8s - fee. Closes
the per-settlement leak between internal accounting and actual
canister balance."
```

---

### Task 5: Fix `claimBackerRepayment`

**Files:**
- Modify: `ponzi_math/main.mo:1116-1166`

This function is different from the previous two: the below-fee branch must explicitly roll back the zeroing of `backerRepayments` and return an `#Err`. Tiny accrued backer balances should not be silently destroyed.

- [ ] **Step 1: Apply the fix with explicit below-fee rollback**

Use Edit to change exactly this:

Old:
```motoko
            let balance = aBalance + bBalance;
            if (balance <= 0.0) { return #Err("No repayment balance to claim") };
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), 0.0);
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), 0.0);

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
```

New:
```motoko
            let balance = aBalance + bBalance;
            if (balance <= 0.0) { return #Err("No repayment balance to claim") };

            let balanceE8s = Int.abs(Float.toInt(roundToEightDecimals(balance) * 100_000_000.0));
            if (balanceE8s <= Ledger.ICP_TRANSFER_FEE) {
                return #Err("Claimable balance is below the network fee (0.0001 ICP); wait until your balance grows past the fee");
            };
            let transferAmount : Nat = balanceE8s - Ledger.ICP_TRANSFER_FEE;

            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesA), 0.0);
            backerRepayments := backerKeyMap.put(backerRepayments, (caller, #seriesB), 0.0);

            let transferResult = try {
                await icpLedger.icrc1_transfer({
                    from_subaccount = null;
                    to = { owner = caller; subaccount = null };
                    amount = transferAmount;
                    fee = null;
                    memo = null;
                    created_at_time = null;
                });
```

Three things changed:
1. The fee guard runs BEFORE we zero `backerRepayments` — early-exit with `#Err` if below fee, leaving state untouched.
2. `transferAmount` computed once after the guard.
3. `amount = transferAmount` (not `balanceE8s`) in the icrc1_transfer call.

The existing rollback paths inside the `catch` and `#Err` branches (which restore `aBalance` and `bBalance`) are unchanged and still correct — they restore the zeroing that we now only perform after the fee guard passes.

- [ ] **Step 2: Type-check**

Run: `dfx build ponzi_math --check`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "fix(ponzi_math): pre-deduct ICP fee in claimBackerRepayment

Closes the per-claim leak. Below-fee branch returns an explicit #Err
('Claimable balance is below the network fee...') and leaves the
repayment balance intact — never silently zero tiny accruals.

Guard runs before any state mutation; existing rollback paths in the
ledger-failure branches are unchanged."
```

---

## Phase 4: Verify the fix (GREEN)

### Task 6: Re-run the full integration on the local replica

**Files:** none — read-only verification + temporary local state

- [ ] **Step 1: Redeploy ponzi_math with the fixes**

If the replica from Task 2 is still running:
```bash
dfx deploy ponzi_math --network local --argument '(record {
    backendPrincipal = principal "'$BACKEND_PRINCIPAL'";
    testAdmin = principal "'$(dfx identity get-principal)'";
})' --mode reinstall
```

`--mode reinstall` is required because we want a fresh state (any leftover repayments / pot from Task 2 are not in scope). Confirm at the prompt.

If the replica was stopped, restart with `dfx start --clean --background` and re-deploy from scratch (no `--mode reinstall` needed).

- [ ] **Step 2: Confirm baseline invariant holds on fresh canister**

```bash
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: `RESULT: PASS`, all zeros, diff zero.

- [ ] **Step 3: Verify `withdrawEarnings` path**

```bash
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createGame "(variant { simple21Day }, 1.0, false)"
NOW_NS=$(date +%s%N)
TWELVE_HOURS_AGO_NS=$((NOW_NS - 43200000000000))
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createBackdatedGame "(variant { simple21Day }, 1.0, false, $TWELVE_HOURS_AGO_NS : int)"
dfx canister call ponzi_math withdrawEarnings "(1 : nat)"
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected on the final verification: `RESULT: PASS`, diff ≤ 10 e8s. Compare to Task 2 Step 6 where the diff was ~−10,000 e8s.

- [ ] **Step 4: Verify `settleCompoundingGame` path**

Repeat for a compounding plan:
```bash
SIXTEEN_DAYS_AGO_NS=$((NOW_NS - 1382400000000000))
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createBackdatedGame "(variant { compounding15Day }, 1.0, true, $SIXTEEN_DAYS_AGO_NS : int)"
dfx canister call ponzi_math settleCompoundingGame "(2 : nat)"
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: `RESULT: PASS`, diff ≤ 10 e8s. (Note: if pot is too small to cover the compounded payout, the partial-payout / promoteAndReset path is exercised — which is fine and still expected to balance.)

The exact gameId for `settleCompoundingGame` depends on previous calls; adjust if you've created more games. Use `dfx canister call ponzi_math getAllActiveGames` to see active gameIds if unsure.

- [ ] **Step 5: Verify `claimBackerRepayment` happy path**

By this point a backer should exist? No — we haven't called `addBackerMoney`. Add a backer, drive a payout that generates a non-trivial repayment balance, then claim:

```bash
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math addBackerMoney "(1.0 : float64)"

# Re-trigger a withdraw to generate exit toll → backer repayment credit
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
dfx canister call ponzi_math createBackdatedGame "(variant { simple21Day }, 1.0, false, $TWELVE_HOURS_AGO_NS : int)"
dfx canister call ponzi_math withdrawEarnings "(3 : nat)"

# Now claim
dfx canister call ponzi_math getBackerRepaymentBalance
dfx canister call ponzi_math claimBackerRepayment
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: `getBackerRepaymentBalance` returns a small positive Float (well above 0.0001 ICP — call it again if too small after only one toll). `claimBackerRepayment` returns `(variant { Ok = <float> })`. Final verification: `RESULT: PASS`, diff ≤ 10 e8s.

- [ ] **Step 6: Verify `claimBackerRepayment` below-fee branch**

Drain the backer's repayment balance to a tiny value (the claim itself in Step 5 zeroed it; subsequent tolls credit a small fresh amount). Repeat one small withdrawal:

```bash
dfx canister call <ICP_LEDGER> icrc2_approve "(record {
    spender = record { owner = principal \"$PONZI_MATH_ID\"; subaccount = null };
    amount = 200_000_000 : nat
})"
ONE_HOUR_AGO_NS=$((NOW_NS - 3600000000000))
dfx canister call ponzi_math createBackdatedGame "(variant { simple21Day }, 0.1, false, $ONE_HOUR_AGO_NS : int)"
# Game with tiny earnings → tiny toll → tiny repayment credit
dfx canister call ponzi_math withdrawEarnings "(4 : nat)"
dfx canister call ponzi_math getBackerRepaymentBalance
```

If `getBackerRepaymentBalance` is at or below 0.0001 ICP, attempt to claim:
```bash
dfx canister call ponzi_math claimBackerRepayment
```
Expected: `(variant { Err = "Claimable balance is below the network fee..." })`. Verify the balance is unchanged:
```bash
dfx canister call ponzi_math getBackerRepaymentBalance
./ponzi_math/scripts/verify-fee-invariant.sh local
```
Expected: same balance as before the failed claim, invariant `RESULT: PASS`.

If `getBackerRepaymentBalance` was still above 0.0001, this branch can't be exercised with this setup; document that and proceed (the code path is small enough to be confidence-reviewed). Do not add invented test infrastructure to force it.

- [ ] **Step 7: Stop the local replica**

Run: `dfx stop`
Expected: replica stops cleanly.

- [ ] **Step 8: Capture verification output for the PR**

Save the output from Steps 3, 4, 5 (and 6 if exercised) into a working note for the PR description's "Test plan" section. No commit yet — this is logging, not code.

---

### Task 7: Full canister type-check

**Files:** none

- [ ] **Step 1: Run a full build check on every canister to confirm we didn't break a downstream consumer**

```bash
dfx build ponzi_math --check
dfx build backend --check
dfx build shenanigans --check
```
Expected: each prints something like "Successfully built ..." or no errors. If `backend` or `shenanigans` fail, look for any code that depends on `ponzi_math`'s Candid surface — but since we only changed function bodies (not signatures), this should be a clean pass.

- [ ] **Step 2: No commit** — this is verification only.

---

## Phase 5: Ship

### Task 8: Open the pull request

**Files:** none

- [ ] **Step 1: Push the branch**

```bash
git push -u origin HEAD
```

- [ ] **Step 2: Open the PR**

```bash
gh pr create --title "fix(ponzi_math): stop ICP transfer fee leak in payout paths" --body "$(cat <<'EOF'
## Summary

- `withdrawEarnings`, `settleCompoundingGame`, `claimBackerRepayment` now pre-deduct `Ledger.ICP_TRANSFER_FEE` from the transfer amount, mirroring the in-tree `sweepCoverCharges` pattern.
- Closes a 0.0001 ICP per-payout gap between actual canister balance and internal accounting (`potBalance` + `roundSeedReserve` + `sum(backerRepayments)` + `coverChargeBalance`).
- No new stable state, no migration. `dfx canister install --mode upgrade` is sufficient on deploy.

## Why

`icrc1_transfer({ ..., fee = null, ... })` makes the ICP ledger apply its 10,000 e8s default fee on top of `amount`, deducting `amount + fee` from the canister. No internal accounting field was tracking this, so each payout grew a permanent gap. Verified on mainnet 2026-05-15: actual = 0.39395076 ICP, internal = 0.39485 ICP, gap = ~0.0009 ICP from ~9 prior payouts. Left unfixed, a future claim eventually fails because the canister's actual balance can no longer honor an internally-credited amount.

## Historical drift

The existing ~0.0009 ICP gap is **acknowledged and accepted as a sunk cost** — not corrected. This PR stops accumulation going forward.

## Test plan

Verified on local replica with `ponzi_math/scripts/verify-fee-invariant.sh`:

- [ ] Pre-fix: deposit + backdated withdraw → script reports `Diff: -10000 e8s, RESULT: FAIL`
- [ ] Post-fix `withdrawEarnings`: same flow → `RESULT: PASS`, diff ≤ 10 e8s
- [ ] Post-fix `settleCompoundingGame` (matured compounding game) → `RESULT: PASS`
- [ ] Post-fix `claimBackerRepayment` (happy path) → `RESULT: PASS`
- [ ] Post-fix `claimBackerRepayment` (below-fee branch) → returns `#Err`, repayment balance unchanged
- [ ] `dfx build ponzi_math --check`, `dfx build backend --check`, `dfx build shenanigans --check` all pass

The verification script (`ponzi_math/scripts/verify-fee-invariant.sh`) is re-runnable on mainnet for read-only spot-checks: `./ponzi_math/scripts/verify-fee-invariant.sh ic`.

## Deploy

DO NOT auto-deploy. This PR is code-only. Mainnet deploy is a separate, owner-gated step.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Return the PR URL**

Print the URL `gh pr create` emitted.

---

## Done criteria

- All three previously bugged call sites in [ponzi_math/main.mo](ponzi_math/main.mo) use the `amount - Ledger.ICP_TRANSFER_FEE` pattern.
- `sweepCoverCharges` is unchanged.
- No new stable state fields. `var` declarations in `ponzi_math/main.mo` are identical to pre-PR.
- `ponzi_math/scripts/verify-fee-invariant.sh` exists, is executable, and passes on the post-fix local replica after running all three payout flows.
- PR open with the description above.
- No mainnet deploy.
