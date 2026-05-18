# Ponzi_math Security Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land six security fixes identified in the May 17 [ponzi_math/](../../../ponzi_math/) audit: (1) admin-gate the unauthenticated cycle-burn endpoint, (2) move `#tollDistribution` ledger writes after successful transfer to stop phantom entries on failure, (3) make `raw_rand` failure non-fatal and add a manual reset hatch, (4) acquire the global lock in admin mutators, (5) replace the bucket rate limit with a sliding window, (6) add paginated ledger query to stay under the IC response size cap.

**Architecture:** Six independent commits, one per fix, each on its own task. No new stable state. No data migration (the rate-limit semantic change naturally invalidates old values, see Task 5 commentary). Each Motoko-modifying task ends with `dfx build ponzi_math --check`. Behavior tests are dfx-based shell scripts modeled on [ponzi_math/scripts/verify-fee-invariant.sh](../../../ponzi_math/scripts/verify-fee-invariant.sh). Refactors without a clean behavior boundary (Task 2 lock hygiene, Task 4's `raw_rand` try/catch) rely on compile-check + code review.

**Tech Stack:** Motoko (`persistent actor` on Internet Computer), dfx 0.29.2, bash for verification harnesses, no mops dependency.

**Reference:** Audit findings — captured inline in each task. Issue numbers reference the audit's HIGH/MEDIUM/LOW severity tags.

**Validation cadence:** `dfx build ponzi_math --check` after every Motoko edit. Behavior scripts run before-and-after for Tasks 1, 4, 6. Existing [verify-fee-invariant.sh](../../../ponzi_math/scripts/verify-fee-invariant.sh) re-run after Task 5 to confirm Float accounting still reconciles.

**Naming guard:** Per [CLAUDE.md](../../../CLAUDE.md), do NOT rename `exitToll` / `coverCharge` / `EXIT_TOLL_*` / `COVER_CHARGE_RATE` internal identifiers. New types introduced in Task 5 (`TollDistributionDetails`) use neutral neighbor naming, not a rename.

**Mainnet deploy guard:** Per [memory/feedback_deploy_safety.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_deploy_safety.md), DO NOT run `dfx deploy ponzi_math --network ic` at any point in this plan. All `dfx` invocations use `--network local`. Mainnet deploy is the user's call after PR review. Per [memory/project_ponzi_math_deploy_lineage.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/project_ponzi_math_deploy_lineage.md), this branch needs rebase onto current `main` before any mainnet deploy.

---

## File Structure

This plan only touches `ponzi_math/` files. No backend, frontend, ledger.mo, icrc21.mo, dfx.json, or mops.toml changes.

**Modified:**
- `ponzi_math/main.mo` — every task edits this file

**Created:**
- `ponzi_math/scripts/verify-admin-auth.sh` — Task 1
- `ponzi_math/scripts/verify-force-reset.sh` — Task 4
- `ponzi_math/scripts/verify-pagination.sh` — Task 6

**Untouched:**
- `ponzi_math/ledger.mo`, `ponzi_math/icrc21.mo`, `ponzi_math/migration.mo` (the last is dead-code-pending-deletion, tracked in a separate spawned task), `backend/`, `frontend/`, `dfx.json`

---

## Phase 1: Quick wins — admin auth + lock hygiene + rate limit

### Task 1: Admin-gate `getCanisterICPBalance` (HIGH)

**Issue (HIGH):** [`getCanisterICPBalance`](../../../ponzi_math/main.mo:1471) is `public shared func` with no caller check. Each call performs an inter-canister `await icpLedger.icrc1_balance_of(...)`. An attacker can spam this in a tight loop and drain the canister's cycles until it freezes.

**Fix:** Add `requireAdmin(caller)` guard. Function becomes admin-only. The two internal callers — [`adminSweepUntracked`](../../../ponzi_math/main.mo:1819) (already admin-gated, calls `icpLedger` directly, doesn't go through this method) and [verify-fee-invariant.sh](../../../ponzi_math/scripts/verify-fee-invariant.sh) (admin operator context) — both remain functional. The frontend does not call this endpoint.

**Files:**
- Modify: `ponzi_math/main.mo` — `getCanisterICPBalance` signature change
- Create: `ponzi_math/scripts/verify-admin-auth.sh`

- [ ] **Step 1: Create scripts directory entry and the verification script**

Create file `ponzi_math/scripts/verify-admin-auth.sh` with these exact contents:

```bash
#!/usr/bin/env bash
# verify-admin-auth.sh
#
# Verifies that getCanisterICPBalance is admin-only.
# Pre-fix: non-admin call returns the balance (FAIL).
# Post-fix: non-admin call traps with "Unauthorized" (PASS).
#
# Usage: ./ponzi_math/scripts/verify-admin-auth.sh [network]
# network defaults to "local". DO NOT run against "ic" without explicit go-ahead.
set -euo pipefail

NETWORK="${1:-local}"

# Save current identity so we can restore it.
ORIG_IDENTITY=$(dfx identity whoami)
trap 'dfx identity use "$ORIG_IDENTITY" >/dev/null 2>&1 || true' EXIT

# Create a deterministic non-admin identity if it doesn't already exist.
if ! dfx identity list 2>/dev/null | grep -qx "test-non-admin"; then
    dfx identity new test-non-admin --storage-mode plaintext >/dev/null 2>&1
fi

echo "=== Non-admin call (expect Unauthorized) ==="
result=$(dfx --identity test-non-admin canister call --network "$NETWORK" ponzi_math getCanisterICPBalance 2>&1 || true)
echo "Response: $result"
if echo "$result" | grep -qiE "unauthorized|admin only"; then
    echo "PASS: non-admin rejected"
else
    echo "FAIL: non-admin call did not return Unauthorized"
    exit 1
fi

echo
echo "=== Admin call (expect a Nat balance) ==="
result=$(dfx --identity "$ORIG_IDENTITY" canister call --network "$NETWORK" ponzi_math getCanisterICPBalance 2>&1)
echo "Response: $result"
if echo "$result" | grep -qE "[0-9_]+[[:space:]]*:[[:space:]]*nat"; then
    echo "PASS: admin call returned a Nat"
else
    echo "FAIL: admin call did not return a Nat"
    exit 1
fi

echo
echo "All checks passed."
```

Then run: `chmod +x ponzi_math/scripts/verify-admin-auth.sh`

- [ ] **Step 2: Deploy local ponzi_math (if not already up)**

Run:
```bash
dfx start --background --clean
dfx deploy --network local ponzi_math --argument '(record { backendPrincipal = principal "aaaaa-aa"; testAdmin = principal "'$(dfx identity get-principal)'" })'
```

Expected: canister deploys, default identity becomes testAdmin.

- [ ] **Step 3: Run script — confirm it FAILS (current broken behavior)**

Run: `./ponzi_math/scripts/verify-admin-auth.sh local`

Expected: `FAIL: non-admin call did not return Unauthorized` because the current implementation returns the balance to anyone.

- [ ] **Step 4: Apply the auth guard**

In [ponzi_math/main.mo](../../../ponzi_math/main.mo) locate the function (currently at lines 1471-1476):

```motoko
    public shared func getCanisterICPBalance() : async Nat {
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
    };
```

Replace with:

```motoko
    public shared ({ caller }) func getCanisterICPBalance() : async Nat {
        requireAdmin(caller);
        let selfPrincipal = Principal.fromActor(Self);
        try {
            await icpLedger.icrc1_balance_of({ owner = selfPrincipal; subaccount = null });
        } catch (_) { 0 };
    };
```

Two changes: (a) add `({ caller })` destructuring to the shared declaration, (b) call `requireAdmin(caller)` as the first line. `requireAdmin` is already defined at [main.mo:289](../../../ponzi_math/main.mo:289) and traps on non-admin/anonymous.

- [ ] **Step 5: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds without warnings or errors.

- [ ] **Step 6: Redeploy locally**

Run:
```bash
dfx deploy --network local ponzi_math --argument '(record { backendPrincipal = principal "aaaaa-aa"; testAdmin = principal "'$(dfx identity get-principal)'" })'
```

The `--argument` is required even on upgrade because the canister declares mandatory init args. Expected: upgrade succeeds (no state shape change, no migration needed).

- [ ] **Step 7: Re-run script — confirm it PASSES**

Run: `./ponzi_math/scripts/verify-admin-auth.sh local`

Expected: both checks pass.

- [ ] **Step 8: Commit**

```bash
git add ponzi_math/main.mo ponzi_math/scripts/verify-admin-auth.sh
git commit -m "$(cat <<'EOF'
fix(ponzi_math): admin-gate getCanisterICPBalance against cycle-burn DOS

The unauthenticated public method performed an inter-canister
icrc1_balance_of call on every invocation. Anyone could spam it to
drain canister cycles. requireAdmin(caller) now gates the entry; the
only internal users (adminSweepUntracked, verify-fee-invariant.sh)
already run in admin context.

verify-admin-auth.sh covers both the rejection path and the admin
happy path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Acquire global lock in admin mutators (LOW)

**Issue (LOW):** [`adminClearAllBackerPositions`](../../../ponzi_math/main.mo:1805) and [`adminMergeBackerPosition`](../../../ponzi_math/main.mo:1746) mutate `backerPositions` / `backerRepayments` without acquiring `globalCriticalLock`. They run synchronously (no `await`), so they execute atomically on their own, but they can interleave **between** awaits of a user's in-flight `claimBackerRepayment`. The rollback in `claimBackerRepayment`'s `catch` then writes the user's old balance back into a map the admin just cleared, partially undoing the admin's action.

**Fix:** Both methods acquire `globalCriticalLock` before mutating and release it in `finally`. They don't acquire the per-caller lock — the only caller is `TEST_ADMIN`, and serializing admin ops with themselves is unnecessary.

**Files:**
- Modify: `ponzi_math/main.mo` — `adminMergeBackerPosition`, `adminClearAllBackerPositions`

- [ ] **Step 1: Add lock to `adminClearAllBackerPositions`**

Locate the function (currently at lines 1805-1810):

```motoko
    public shared ({ caller }) func adminClearAllBackerPositions() : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        backerPositions := backerKeyMap.empty<BackerPosition>();
        backerRepayments := backerKeyMap.empty<Float>();
        #Ok;
    };
```

Replace with:

```motoko
    public shared ({ caller }) func adminClearAllBackerPositions() : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            backerPositions := backerKeyMap.empty<BackerPosition>();
            backerRepayments := backerKeyMap.empty<Float>();
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };
```

- [ ] **Step 2: Add lock to `adminMergeBackerPosition`**

Locate the function (currently starts at line 1746). The whole body needs to be wrapped in `try { ... } finally { releaseGlobalLock(); }` after `acquireGlobalLock()`. The existing function:

```motoko
    public shared ({ caller }) func adminMergeBackerPosition(
        from : Principal,
        to : Principal,
    ) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (from == to) { return #Err("from and to must differ") };

        let fromPos = switch (backerKeyMap.get(backerPositions, (from, #seriesA))) {
            case (null) { return #Err("from principal has no backer position") };
            case (?p) { p };
        };

        switch (backerKeyMap.get(backerPositions, (to, #seriesA))) {
            case (null) {
                backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                    fromPos with owner = to;
                });
            };
            case (?toPos) {
                let mergedStart = if (toPos.startTime <= fromPos.startTime) { toPos.startTime } else { fromPos.startTime };
                let mergedFirst = switch (toPos.firstDepositDate, fromPos.firstDepositDate) {
                    case (?d1, ?d2) { if (d1 <= d2) { ?d1 } else { ?d2 } };
                    case (?d, null) { ?d };
                    case (null, ?d) { ?d };
                    case (null, null) { null };
                };
                backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                    toPos with
                    amount = toPos.amount + fromPos.amount;
                    entitlement = toPos.entitlement + fromPos.entitlement;
                    startTime = mergedStart;
                    firstDepositDate = mergedFirst;
                });
            };
        };

        backerPositions := backerKeyMap.delete(backerPositions, (from, #seriesA));

        let fromRepay = switch (backerKeyMap.get(backerRepayments, (from, #seriesA))) {
            case (null) { 0.0 };
            case (?r) { r };
        };
        if (fromRepay > 0.0) {
            let toRepay = switch (backerKeyMap.get(backerRepayments, (to, #seriesA))) {
                case (null) { 0.0 };
                case (?r) { r };
            };
            backerRepayments := backerKeyMap.put(backerRepayments, (to, #seriesA), toRepay + fromRepay);
        };
        backerRepayments := backerKeyMap.delete(backerRepayments, (from, #seriesA));

        #Ok;
    };
```

Replace with (gate checks stay BEFORE the lock acquisition so early-return paths don't acquire/release for no reason):

```motoko
    public shared ({ caller }) func adminMergeBackerPosition(
        from : Principal,
        to : Principal,
    ) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        if (from == to) { return #Err("from and to must differ") };

        acquireGlobalLock();
        try {
            let fromPos = switch (backerKeyMap.get(backerPositions, (from, #seriesA))) {
                case (null) { return #Err("from principal has no backer position") };
                case (?p) { p };
            };

            switch (backerKeyMap.get(backerPositions, (to, #seriesA))) {
                case (null) {
                    backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                        fromPos with owner = to;
                    });
                };
                case (?toPos) {
                    let mergedStart = if (toPos.startTime <= fromPos.startTime) { toPos.startTime } else { fromPos.startTime };
                    let mergedFirst = switch (toPos.firstDepositDate, fromPos.firstDepositDate) {
                        case (?d1, ?d2) { if (d1 <= d2) { ?d1 } else { ?d2 } };
                        case (?d, null) { ?d };
                        case (null, ?d) { ?d };
                        case (null, null) { null };
                    };
                    backerPositions := backerKeyMap.put(backerPositions, (to, #seriesA), {
                        toPos with
                        amount = toPos.amount + fromPos.amount;
                        entitlement = toPos.entitlement + fromPos.entitlement;
                        startTime = mergedStart;
                        firstDepositDate = mergedFirst;
                    });
                };
            };

            backerPositions := backerKeyMap.delete(backerPositions, (from, #seriesA));

            let fromRepay = switch (backerKeyMap.get(backerRepayments, (from, #seriesA))) {
                case (null) { 0.0 };
                case (?r) { r };
            };
            if (fromRepay > 0.0) {
                let toRepay = switch (backerKeyMap.get(backerRepayments, (to, #seriesA))) {
                    case (null) { 0.0 };
                    case (?r) { r };
                };
                backerRepayments := backerKeyMap.put(backerRepayments, (to, #seriesA), toRepay + fromRepay);
            };
            backerRepayments := backerKeyMap.delete(backerRepayments, (from, #seriesA));

            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };
```

Note the `return #Err(...)` inside the `case (null)` branch still works — Motoko's `try/finally` runs the finally on early return, so the lock is released.

- [ ] **Step 3: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "$(cat <<'EOF'
fix(ponzi_math): acquire global lock in admin mutators

adminClearAllBackerPositions and adminMergeBackerPosition mutated
backerPositions / backerRepayments without the global lock. A user's
in-flight claimBackerRepayment could interleave between its awaits
and an admin call, causing the claim's rollback to write stale balance
back into the map the admin just cleared.

Both methods now acquire globalCriticalLock and release in finally.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Sliding-window deposit rate limit (LOW)

**Issue (LOW):** [Rate limit](../../../ponzi_math/main.mo:724-737) uses bucket math: `currentHour = currentTime / 3600000000000`, then filters `currentHour - t < 1`. This keeps only the current hour bucket. At 12:59 a user can deposit 3 times; at 13:00 the bucket resets and they can deposit 3 more — 6 deposits in 2 minutes.

**Fix:** Store actual nanosecond timestamps (not hour buckets) and filter on `t > now - 3600000000000` (sliding 1-hour window). The same change in [`checkDepositRateLimit`](../../../ponzi_math/main.mo:1353).

**Migration note:** No migration code needed. Existing `depositTimestamps` values are hour-bucket integers (~500,000). After the upgrade, the new filter computes `oneHourAgo ≈ 1.78e18` nanos and rejects everything below it, so all old values are naturally filtered out on first access. Every user effectively gets a fresh per-hour quota at upgrade time. Acceptable.

**Files:**
- Modify: `ponzi_math/main.mo` — `createGame` rate-limit logic, `checkDepositRateLimit`

- [ ] **Step 1: Update `createGame`'s rate-limit window**

Locate the rate-limit block in `createGame` (currently lines 724-737 and 768-776). The two existing blocks:

**Block A (gate, lines 724-737):**
```motoko
            let currentTime = Time.now();
            let currentHour = currentTime / 3600000000000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(
                        timestamps,
                        func(t) { currentHour - t < 1 },
                    );
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };
```

Replace with:

```motoko
            let currentTime = Time.now();
            let oneHourAgo = currentTime - 3_600_000_000_000;
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {};
                case (?timestamps) {
                    let filtered = List.filter<Int>(
                        timestamps,
                        func(t) { t > oneHourAgo },
                    );
                    if (List.size(filtered) >= 3) {
                        return #Err("You can only open 3 positions per hour");
                    };
                };
            };
```

**Block B (record, lines 768-776):**
```motoko
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, List.nil()));
                };
                case (?timestamps) {
                    let filtered = List.filter<Int>(timestamps, func(t) { currentHour - t < 1 });
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentHour, filtered));
                };
            };
```

Replace with (store `currentTime` not `currentHour`, filter on `oneHourAgo`):

```motoko
            switch (principalMapNat.get(depositTimestamps, caller)) {
                case (null) {
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentTime, List.nil()));
                };
                case (?timestamps) {
                    let filtered = List.filter<Int>(timestamps, func(t) { t > oneHourAgo });
                    depositTimestamps := principalMapNat.put(depositTimestamps, caller, List.push(currentTime, filtered));
                };
            };
```

- [ ] **Step 2: Update `checkDepositRateLimit` query**

Locate the function (currently lines 1353-1362):

```motoko
    public query ({ caller }) func checkDepositRateLimit() : async Bool {
        let currentHour = Time.now() / 3600000000000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { currentHour - t < 1 });
                List.size(filtered) < 3;
            };
        };
    };
```

Replace with:

```motoko
    public query ({ caller }) func checkDepositRateLimit() : async Bool {
        let oneHourAgo = Time.now() - 3_600_000_000_000;
        switch (principalMapNat.get(depositTimestamps, caller)) {
            case (null) { true };
            case (?ts) {
                let filtered = List.filter<Int>(ts, func(t) { t > oneHourAgo });
                List.size(filtered) < 3;
            };
        };
    };
```

- [ ] **Step 3: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "$(cat <<'EOF'
fix(ponzi_math): sliding-window deposit rate limit

createGame's previous rate limit was per hour-bucket: 3 deposits in
12:59 + 3 deposits in 13:00 = 6 in 2 minutes. Now stores actual
nanosecond timestamps and filters on t > now - 1h, so the cap is 3
deposits in any rolling 60-minute window.

No migration: old hour-bucket values (~5e5) fall below the new
oneHourAgo threshold (~1.8e18) and are filtered out naturally. Every
user gets a fresh quota at the upgrade boundary.

checkDepositRateLimit query updated to the same window.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2: `raw_rand` fault tolerance + recovery hatch

### Task 4: Make `raw_rand` failure non-fatal + add `adminForceReset` (MEDIUM)

**Issue (MEDIUM):** In `withdrawEarnings` / `settleCompoundingGame`, the partial-payout flow does:
1. Mutate state, transfer ICP (commits at the transfer's `await`)
2. `recordLedger(#withdrawal{...})`
3. `if (isInsolvent) await promoteAndReset(...)`

`promoteAndReset` calls `selectPromotionCandidate`, which `await`s [`ic.raw_rand()`](../../../ponzi_math/main.mo:545). If `raw_rand` errors (system glitch, management-canister timeout), the whole call traps. By then the payout already happened, so the user's ICP is out but the round didn't reset and no Series B promotion fired. Recovery requires admin intervention, but no admin reset method exists.

**Fix (a):** Wrap `await ic.raw_rand()` in a `try/catch`. On failure, `selectPromotionCandidate` returns `null` (skip promotion this round). `triggerGameReset` still fires, currentRoundId still bumps. Acceptable loss: at most one underwater player misses their Series B promotion this round.

**Fix (b) — belt and suspenders:** Add `adminForceReset(reason : Text)` test-hatch (TEST_ADMIN-only) that acquires the global lock and calls `triggerGameReset`. Recovers from any stuck-state edge case.

**Files:**
- Modify: `ponzi_math/main.mo` — `selectPromotionCandidate` raw_rand call
- Modify: `ponzi_math/main.mo` — add `adminForceReset` in the test-hatch block
- Create: `ponzi_math/scripts/verify-force-reset.sh`

- [ ] **Step 1: Wrap `raw_rand` in try/catch**

Locate `selectPromotionCandidate` (starts at [main.mo:508](../../../ponzi_math/main.mo:508)). The await call is at line 545:

```motoko
        let entropy = await ic.raw_rand();
        let bytes = Blob.toArray(entropy);
```

Replace those two lines with:

```motoko
        let entropy = try {
            await ic.raw_rand();
        } catch (_) {
            // raw_rand failure: skip promotion this round. The caller
            // (promoteAndReset) still fires triggerGameReset on the null
            // return, so the round closes cleanly and only the Series B
            // grant is lost.
            return null;
        };
        let bytes = Blob.toArray(entropy);
```

- [ ] **Step 2: Add `adminForceReset` to the test-hatch block**

Locate the end of the test-hatch block — `adminSweepUntracked` ends around line 1867, right before the ICRC-21 section header at line 1869. Insert this new method directly after `adminSweepUntracked`'s closing `};` and before the `// ====...` header for ICRC-21:

```motoko
    // adminForceReset — manually close the current round. Acquires the global
    // lock and runs triggerGameReset. Use for stuck-state recovery if a prior
    // pot-empty path trapped after a successful payout but before the round
    // closed (e.g., raw_rand failure inside promoteAndReset before Task 4 was
    // applied, or any future analogous edge case).
    public shared ({ caller }) func adminForceReset(reason : Text) : async { #Ok; #Err : Text } {
        if (caller != TEST_ADMIN) { return #Err("Unauthorized: testAdmin only") };
        acquireGlobalLock();
        try {
            triggerGameReset("admin force-reset: " # reason);
            #Ok;
        } finally {
            releaseGlobalLock();
        };
    };
```

- [ ] **Step 3: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds cleanly.

- [ ] **Step 4: Create verification script for `adminForceReset`**

Create `ponzi_math/scripts/verify-force-reset.sh` with these contents:

```bash
#!/usr/bin/env bash
# verify-force-reset.sh
#
# Confirms adminForceReset bumps currentRoundId by 1 and is admin-only.
#
# Usage: ./ponzi_math/scripts/verify-force-reset.sh [network]
# network defaults to "local". DO NOT run against "ic" without explicit go-ahead.
set -euo pipefail

NETWORK="${1:-local}"

ORIG_IDENTITY=$(dfx identity whoami)
trap 'dfx identity use "$ORIG_IDENTITY" >/dev/null 2>&1 || true' EXIT

if ! dfx identity list 2>/dev/null | grep -qx "test-non-admin"; then
    dfx identity new test-non-admin --storage-mode plaintext >/dev/null 2>&1
fi

parse_nat() {
    sed -E 's/.*\(([0-9_]+)[[:space:]]*:[[:space:]]*nat\).*/\1/' | tr -d '_'
}

echo "=== Non-admin rejection ==="
result=$(dfx --identity test-non-admin canister call --network "$NETWORK" ponzi_math adminForceReset '("attempt")' 2>&1 || true)
echo "Response: $result"
if echo "$result" | grep -qiE "unauthorized"; then
    echo "PASS: non-admin rejected"
else
    echo "FAIL: non-admin succeeded"
    exit 1
fi

echo
echo "=== Admin reset bumps roundId ==="
before=$(dfx --identity "$ORIG_IDENTITY" canister call --network "$NETWORK" ponzi_math adminGetCurrentRoundId | parse_nat)
echo "Before: roundId = $before"
dfx --identity "$ORIG_IDENTITY" canister call --network "$NETWORK" ponzi_math adminForceReset '("smoke test")' >/dev/null
after=$(dfx --identity "$ORIG_IDENTITY" canister call --network "$NETWORK" ponzi_math adminGetCurrentRoundId | parse_nat)
echo "After:  roundId = $after"

expected=$((before + 1))
if [[ "$after" -eq "$expected" ]]; then
    echo "PASS: roundId bumped $before -> $after"
else
    echo "FAIL: expected $expected, got $after"
    exit 1
fi

echo
echo "All checks passed."
```

Then run: `chmod +x ponzi_math/scripts/verify-force-reset.sh`

- [ ] **Step 5: Redeploy and run the script**

Run:
```bash
dfx ping local >/dev/null 2>&1 || dfx start --background --clean
dfx deploy --network local ponzi_math --argument '(record { backendPrincipal = principal "aaaaa-aa"; testAdmin = principal "'$(dfx identity get-principal)'" })'
./ponzi_math/scripts/verify-force-reset.sh local
```

The `--argument` is required even on upgrade because the canister declares mandatory init args. Expected: both checks pass.

- [ ] **Step 6: Commit**

```bash
git add ponzi_math/main.mo ponzi_math/scripts/verify-force-reset.sh
git commit -m "$(cat <<'EOF'
fix(ponzi_math): make raw_rand failure non-fatal + adminForceReset hatch

Previously, if ic.raw_rand() inside selectPromotionCandidate trapped
during a partial-payout flow, the trap would land AFTER the payout
already committed at the transfer's await. The user's ICP was out but
the round didn't close and no Series B promotion fired — and no admin
method existed to recover.

selectPromotionCandidate now catches raw_rand errors and returns null,
so promoteAndReset still fires triggerGameReset. Acceptable loss: one
round's Series B promotion is skipped on raw_rand failure.

adminForceReset(reason) is a TEST_ADMIN-only recovery hatch in the
pre-blackhole test block — acquires the global lock and calls
triggerGameReset. Useful for any future stuck-state edge case.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3: Phantom `#tollDistribution` fix

### Task 5: Move `recordLedger(#tollDistribution)` after successful transfer (MEDIUM)

**Issue (MEDIUM):** In [`withdrawEarnings`](../../../ponzi_math/main.mo:911) and [`settleCompoundingGame`](../../../ponzi_math/main.mo:1033), [`distributeExitToll`](../../../ponzi_math/main.mo:411) is called BEFORE the ICP transfer. `distributeExitToll` both mutates state (`roundSeedReserve`, `backerRepayments`) AND writes a `#tollDistribution` ledger entry inline. On transfer failure, the rollback restores money state but NOT `generalLedger` / `nextGeneralLedgerId` — the phantom `#tollDistribution` entry stays. On retry, a second `#tollDistribution` is written, doubling the audit trail.

**Fix:** Split `distributeExitToll` into two pieces:
1. `distributeExitToll(tollAmount) : TollDistributionDetails` — mutates state, returns a struct describing what was distributed (no ledger write).
2. The caller invokes `recordLedger(#tollDistribution(details))` ONLY on the success path, after the ICP transfer succeeds.

The variant payload type already exists in `GeneralLedgerEvent` — `TollDistributionDetails` is structurally identical, defined locally for the helper's return type.

**Validation:** No clean dfx behavior test (triggering a real transfer failure is hard without mocking the ledger). Instead: (a) `dfx build --check` for type safety, (b) re-run [verify-fee-invariant.sh](../../../ponzi_math/scripts/verify-fee-invariant.sh) after a happy-path withdrawal to confirm Float accounting still reconciles, (c) inspect ledger via `getGeneralLedger` to confirm exactly ONE `#tollDistribution` per `#withdrawal` / `#settlement`.

**Files:**
- Modify: `ponzi_math/main.mo` — `distributeExitToll` refactor
- Modify: `ponzi_math/main.mo` — `withdrawEarnings` success/failure paths
- Modify: `ponzi_math/main.mo` — `settleCompoundingGame` success/failure paths

- [ ] **Step 1: Introduce `TollDistributionDetails` type and refactor `distributeExitToll`**

Locate the existing `distributeExitToll` function (currently lines 411-494). Replace the entire function with this version, which adds the local type and returns details instead of recording inline:

```motoko
    type TollDistributionDetails = {
        tollAmount : Float;
        toSeedReserve : Float;
        toOldestSeriesA : Float;
        toOtherSeriesA : Float;
        toAllBackers : Float;
    };

    // 50% of the toll seeds the next round (routed to roundSeedReserve, OUT of
    // the pot). The other 50% credits backer repayment balances via 35/25/40.
    //
    // Returns a TollDistributionDetails describing what was distributed; caller
    // is responsible for recording the #tollDistribution ledger event AFTER the
    // payout transfer succeeds. This split prevents a phantom ledger entry when
    // the transfer fails and the state mutations are rolled back.
    func distributeExitToll(tollAmount : Float) : TollDistributionDetails {
        let seedAmount = tollAmount * 0.5;
        let backerRepaymentAmount = tollAmount * 0.5;
        roundSeedReserve += seedAmount;

        let allBackers = Iter.toArray(backerKeyMap.vals(backerPositions));
        if (allBackers.size() == 0) {
            // No backers yet — backer half also flows to seed reserve (not pot).
            roundSeedReserve += backerRepaymentAmount;
            return {
                tollAmount;
                toSeedReserve = tollAmount;
                toOldestSeriesA = 0.0;
                toOtherSeriesA = 0.0;
                toAllBackers = 0.0;
            };
        };

        let seriesABackers = List.toArray(
            List.filter(
                List.fromArray(allBackers),
                func(b : BackerPosition) : Bool { b.backerType == #seriesA },
            )
        );

        var oldestBacker : ?BackerPosition = null;
        var oldestTime : Int = 0;
        for (b in seriesABackers.vals()) {
            switch (b.firstDepositDate) {
                case (null) {};
                case (?date) {
                    if (oldestBacker == null or date < oldestTime) {
                        oldestBacker := ?b;
                        oldestTime := date;
                    };
                };
            };
        };

        let otherSeriesA = List.toArray(
            List.filter(
                List.fromArray(seriesABackers),
                func(b : BackerPosition) : Bool {
                    switch (oldestBacker) {
                        case (null) { true };
                        case (?oldest) { b.owner != oldest.owner };
                    };
                },
            )
        );

        // If there's only one Series A backer (no "others"), the 25% portion
        // also goes to that lone backer. Total to oldest in that case: 60%.
        let toOldest : Float =
            if (otherSeriesA.size() == 0) {
                backerRepaymentAmount * 0.60;
            } else {
                backerRepaymentAmount * 0.35;
            };
        switch (oldestBacker) {
            case (null) {};
            case (?b) { creditBackerRepayment((b.owner, b.backerType), toOldest) };
        };

        var toOthers : Float = 0.0;
        if (otherSeriesA.size() > 0) {
            let perBacker = backerRepaymentAmount * 0.25 / Float.fromInt(otherSeriesA.size());
            toOthers := perBacker * Float.fromInt(otherSeriesA.size());
            for (b in otherSeriesA.vals()) { creditBackerRepayment((b.owner, b.backerType), perBacker) };
        };

        let perAll = backerRepaymentAmount * 0.4 / Float.fromInt(allBackers.size());
        let toAll = perAll * Float.fromInt(allBackers.size());
        for (b in allBackers.vals()) { creditBackerRepayment((b.owner, b.backerType), perAll) };

        {
            tollAmount;
            toSeedReserve = seedAmount;
            toOldestSeriesA = toOldest;
            toOtherSeriesA = toOthers;
            toAllBackers = toAll;
        };
    };
```

The only changes vs. the existing function: (a) return type is `TollDistributionDetails`, (b) the two `recordLedger(#tollDistribution(...))` calls are removed and replaced by returning a record literal with the same fields. State mutations are identical and in the same order.

- [ ] **Step 2: Update `withdrawEarnings` to record after successful transfer**

In `withdrawEarnings` (currently lines 911-1027), there's exactly one call to `distributeExitToll(actualToll);` followed by state updates, then the transfer. Locate this region:

```motoko
                    distributeExitToll(actualToll);

                    let willClose = closePosition or isInsolvent;
                    let updatedGame : GameRecord = {
                        game with
                        accumulatedEarnings = 0.0;
                        lastUpdateTime = Time.now();
                        totalWithdrawn = game.totalWithdrawn + actualNetEarnings;
                        isActive = if (willClose) false else game.isActive;
                    };
                    gameRecords := natMap.put(gameRecords, gameId, updatedGame);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames =
                            if (willClose and platformStats.activeGames > 0) {
                                platformStats.activeGames - 1
                            } else { platformStats.activeGames };
                    };

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
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

                    recordLedger(#withdrawal({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));
```

Replace with (capture details from `distributeExitToll`, record `#tollDistribution` AFTER successful transfer, before `#withdrawal`):

```motoko
                    let tollDetails = distributeExitToll(actualToll);

                    let willClose = closePosition or isInsolvent;
                    let updatedGame : GameRecord = {
                        game with
                        accumulatedEarnings = 0.0;
                        lastUpdateTime = Time.now();
                        totalWithdrawn = game.totalWithdrawn + actualNetEarnings;
                        isActive = if (willClose) false else game.isActive;
                    };
                    gameRecords := natMap.put(gameRecords, gameId, updatedGame);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames =
                            if (willClose and platformStats.activeGames > 0) {
                                platformStats.activeGames - 1
                            } else { platformStats.activeGames };
                    };

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
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

                    recordLedger(#tollDistribution(tollDetails));
                    recordLedger(#withdrawal({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));
```

Two changes only: (a) capture the return value as `let tollDetails = distributeExitToll(actualToll);`, (b) add `recordLedger(#tollDistribution(tollDetails));` immediately before the existing `recordLedger(#withdrawal(...))`. Everything else stays byte-identical.

- [ ] **Step 3: Update `settleCompoundingGame` to record after successful transfer**

In `settleCompoundingGame` (currently lines 1033-1164), apply the same pattern. Locate:

```motoko
                    distributeExitToll(actualToll);

                    let settled : GameRecord = {
                        game with
                        isActive = false;
                        accumulatedEarnings = actualNetEarnings;
                        totalWithdrawn = actualNetEarnings;
                        lastUpdateTime = Time.now();
                    };
                    gameRecords := natMap.put(gameRecords, gameId, settled);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                    };

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
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

                    recordLedger(#settlement({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));
```

Replace with:

```motoko
                    let tollDetails = distributeExitToll(actualToll);

                    let settled : GameRecord = {
                        game with
                        isActive = false;
                        accumulatedEarnings = actualNetEarnings;
                        totalWithdrawn = actualNetEarnings;
                        lastUpdateTime = Time.now();
                    };
                    gameRecords := natMap.put(gameRecords, gameId, settled);
                    platformStats := {
                        platformStats with
                        totalWithdrawals = platformStats.totalWithdrawals + actualNetEarnings;
                        potBalance = platformStats.potBalance - actualPotDeduction;
                        activeGames = if (platformStats.activeGames > 0) { platformStats.activeGames - 1 } else { 0 };
                    };

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
                        } catch (e) {
                            gameRecords := natMap.put(gameRecords, gameId, originalGame);
                            platformStats := originalStats;
                            backerRepayments := originalRepayments;
                            roundSeedReserve := originalSeedReserve;
                            return #Err("Failed to contact ICP ledger: " # Error.message(e));
                        };
                        switch (transferResult) {
                            case (#Err(err)) {
                                gameRecords := natMap.put(gameRecords, gameId, originalGame);
                                platformStats := originalStats;
                                backerRepayments := originalRepayments;
                                roundSeedReserve := originalSeedReserve;
                                return #Err(transferErrorMessage(err));
                            };
                            case (#Ok(_)) {};
                        };
                    };

                    recordLedger(#tollDistribution(tollDetails));
                    recordLedger(#settlement({
                        player = caller;
                        gameId;
                        grossEarnings = earnings;
                        toll = actualToll;
                        netToPlayer = actualNetEarnings;
                        potDeduction = actualPotDeduction;
                        isInsolvent;
                    }));
```

Same two changes as Step 2.

- [ ] **Step 4: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds cleanly. If it fails on the `recordLedger(#tollDistribution(tollDetails))` call, double-check that `TollDistributionDetails`'s fields match the `#tollDistribution` variant payload in `GeneralLedgerEvent` (lines 156-162 — they should be identical: `tollAmount`, `toSeedReserve`, `toOldestSeriesA`, `toOtherSeriesA`, `toAllBackers`, all `Float`).

- [ ] **Step 5: Redeploy and run a happy-path smoke test**

```bash
dfx deploy --network local ponzi_math --argument '(record { backendPrincipal = principal "aaaaa-aa"; testAdmin = principal "'$(dfx identity get-principal)'" })'
# (Approve a small ICP amount via icrc2_approve, then:)
# dfx canister call --network local ponzi_math createGame '(variant { simple21Day }, 1.0, false)'
# (Wait, then withdraw...)
# dfx canister call --network local ponzi_math withdrawEarnings '(0)'
# Verify ledger:
dfx canister call --network local ponzi_math getGeneralLedgerStats
```

Expected: `entryCount` increases by exactly 3 per withdrawal (`#coverChargeAccrued` from createGame is already there; the withdrawal adds `#tollDistribution` + `#withdrawal`, plus a `#deposit` from the createGame itself = check the math by counting individual events). The point is there's exactly ONE `#tollDistribution` per `#withdrawal`.

Optional spot-check:
```bash
dfx canister call --network local ponzi_math getGeneralLedger | grep -c "tollDistribution"
dfx canister call --network local ponzi_math getGeneralLedger | grep -c "withdrawal ="
```

Expected: the two counts are equal.

- [ ] **Step 6: Re-run verify-fee-invariant.sh**

Run: `./ponzi_math/scripts/verify-fee-invariant.sh local`

Expected: diff within 10 e8s tolerance. The refactor didn't change Float arithmetic, so accounting remains consistent.

- [ ] **Step 7: Commit**

```bash
git add ponzi_math/main.mo
git commit -m "$(cat <<'EOF'
fix(ponzi_math): record #tollDistribution after successful transfer

Previously distributeExitToll mutated state AND wrote #tollDistribution
inline. On ICP transfer failure, the rollback restored money state
(backerRepayments, roundSeedReserve) but generalLedger /
nextGeneralLedgerId were not part of the snapshot — the phantom toll
distribution entry stayed. A retry then wrote a second
#tollDistribution for the same withdrawal, doubling the audit trail.

distributeExitToll now returns a TollDistributionDetails record;
withdrawEarnings and settleCompoundingGame call recordLedger
(#tollDistribution(details)) only after the transfer succeeds (just
before the existing #withdrawal / #settlement entries). On failure no
ledger entries are written.

State mutations and Float arithmetic are unchanged — verify-fee-
invariant.sh still passes within tolerance.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4: Paginated query for unbounded ledger

### Task 6: Add `getGeneralLedgerPage` (MEDIUM)

**Issue (MEDIUM):** [`getGeneralLedger`](../../../ponzi_math/main.mo:1441) returns the entire `generalLedger` map as one array. Each entry is ~200-500 bytes. The IC query response cap is ~3 MiB, so once the ledger exceeds ~6,000-15,000 entries, this query will fail at the network layer and consumers (frontend, admin god view, audit scripts) lose visibility into history.

**Fix:** Add `getGeneralLedgerPage(offset : Nat, limit : Nat) : async { entries : [GeneralLedgerEntry]; total : Nat }`. Keep `getGeneralLedger` for backward compatibility until consumers migrate. The new function uses `natMap.get(...)` lookups against the (0, 1, 2, ...) ID space, which is O(limit · log N) instead of O(N).

**Files:**
- Modify: `ponzi_math/main.mo` — add `getGeneralLedgerPage` next to existing `getGeneralLedger`
- Create: `ponzi_math/scripts/verify-pagination.sh`

- [ ] **Step 1: Write the failing verification script**

Create `ponzi_math/scripts/verify-pagination.sh` with these contents:

```bash
#!/usr/bin/env bash
# verify-pagination.sh
#
# Confirms getGeneralLedgerPage:
#   - reports the same total as getGeneralLedgerStats.entryCount
#   - returns at most `limit` entries
#   - returns empty + correct total when offset >= total
#
# Usage: ./ponzi_math/scripts/verify-pagination.sh [network]
set -euo pipefail

NETWORK="${1:-local}"

parse_nat_field() {
    local field="$1"
    grep -oE "${field} = [0-9_]+" | head -n1 | awk '{print $3}' | tr -d '_'
}

echo "=== Total count via getGeneralLedgerStats ==="
stats=$(dfx canister call --network "$NETWORK" ponzi_math getGeneralLedgerStats)
total_stats=$(echo "$stats" | parse_nat_field entryCount)
echo "entryCount = $total_stats"

echo
echo "=== Page (offset=0, limit=10) ==="
page=$(dfx canister call --network "$NETWORK" ponzi_math getGeneralLedgerPage '(0:nat, 10:nat)')
total_page=$(echo "$page" | parse_nat_field total)
echo "total field = $total_page"

if [[ "$total_stats" == "$total_page" ]]; then
    echo "PASS: totals match ($total_stats)"
else
    echo "FAIL: getGeneralLedgerStats=$total_stats getGeneralLedgerPage.total=$total_page"
    exit 1
fi

# Count entries returned (each entry has exactly one top-level "id =" field).
# `grep -c` exits non-zero on zero matches; tolerate that under `set -e`.
entries_returned=$(echo "$page" | grep -cE '^\s*id =' || true)
echo "entries returned = $entries_returned"

expected_max=10
if [[ "$total_stats" -lt 10 ]]; then expected_max="$total_stats"; fi

if [[ "$entries_returned" -le "$expected_max" ]]; then
    echo "PASS: returned $entries_returned <= limit $expected_max"
else
    echo "FAIL: returned $entries_returned > limit $expected_max"
    exit 1
fi

echo
echo "=== Page past end (offset=1_000_000, limit=10) ==="
page=$(dfx canister call --network "$NETWORK" ponzi_math getGeneralLedgerPage '(1_000_000:nat, 10:nat)')
total_page=$(echo "$page" | parse_nat_field total)
entries_returned=$(echo "$page" | grep -cE '^\s*id =' || true)

if [[ "$total_page" == "$total_stats" && "$entries_returned" -eq 0 ]]; then
    echo "PASS: past-end returns 0 entries, total still $total_page"
else
    echo "FAIL: past-end total=$total_page entries=$entries_returned"
    exit 1
fi

echo
echo "All checks passed."
```

Then run: `chmod +x ponzi_math/scripts/verify-pagination.sh`

- [ ] **Step 2: Run script — confirm it FAILS (method doesn't exist yet)**

Run: `./ponzi_math/scripts/verify-pagination.sh local`

Expected: dfx returns an error like `Canister has no query method 'getGeneralLedgerPage'` and the script exits non-zero.

- [ ] **Step 3: Add the paginated function**

In [ponzi_math/main.mo](../../../ponzi_math/main.mo), locate the existing `getGeneralLedger` function (currently line 1441):

```motoko
    public query func getGeneralLedger() : async [GeneralLedgerEntry] {
        Iter.toArray(natMap.vals(generalLedger));
    };
```

Insert the new function directly below it (before `getGeneralLedgerStats` which begins at line 1445):

```motoko
    // Paginated alternative for callers that risk the ~3 MiB query response
    // cap as the ledger grows. `offset` is the 0-based starting ledger ID;
    // `limit` caps the entry count returned. `total` reports the full ledger
    // size so callers can drive a paginator. Past-end requests return an
    // empty vec but still report `total`.
    public query func getGeneralLedgerPage(offset : Nat, limit : Nat) : async {
        entries : [GeneralLedgerEntry];
        total : Nat;
    } {
        let total = nextGeneralLedgerId;
        if (offset >= total or limit == 0) {
            return { entries = []; total };
        };
        let endId = if (offset + limit > total) { total } else { offset + limit };
        var result = List.nil<GeneralLedgerEntry>();
        var id = offset;
        while (id < endId) {
            switch (natMap.get(generalLedger, id)) {
                case (?entry) { result := List.push(entry, result) };
                case (null) {};
            };
            id += 1;
        };
        { entries = List.toArray(List.reverse(result)); total };
    };
```

Note: `total = nextGeneralLedgerId` (not `natMap.size(generalLedger)`) — IDs are dense 0..nextGeneralLedgerId-1 and never deleted, so the count equals the next-ID. Using the counter avoids an O(N) walk.

- [ ] **Step 4: Compile-check**

Run: `dfx build ponzi_math --check`

Expected: builds cleanly.

- [ ] **Step 5: Redeploy and re-run the script**

```bash
dfx deploy --network local ponzi_math --argument '(record { backendPrincipal = principal "aaaaa-aa"; testAdmin = principal "'$(dfx identity get-principal)'" })'
./ponzi_math/scripts/verify-pagination.sh local
```

Expected: all three checks pass. (If the local replica has zero ledger entries, the first two checks pass trivially because `0 == 0` and `0 <= 0`. The past-end check is the most meaningful.)

- [ ] **Step 6: Commit**

```bash
git add ponzi_math/main.mo ponzi_math/scripts/verify-pagination.sh
git commit -m "$(cat <<'EOF'
feat(ponzi_math): paginated getGeneralLedgerPage query

getGeneralLedger returns the entire ledger in one response and will
eventually hit the IC's ~3 MiB query response cap (~6-15k entries
depending on event mix). getGeneralLedgerPage(offset, limit) takes a
slice using direct natMap.get lookups against the dense 0..nextId-1
key space — O(limit · log N) instead of O(N) — and reports `total` so
callers can paginate.

getGeneralLedger is kept for backward compatibility; frontend and
admin god view can migrate when convenient.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Post-plan checklist

After all six commits land:

- [ ] **Run all four verification scripts on local replica:**

```bash
./ponzi_math/scripts/verify-admin-auth.sh local
./ponzi_math/scripts/verify-force-reset.sh local
./ponzi_math/scripts/verify-pagination.sh local
./ponzi_math/scripts/verify-fee-invariant.sh local
```

All four should pass.

- [ ] **Confirm `dfx build` (no `--check`) succeeds:**

```bash
dfx build ponzi_math
```

This catches anything `--check` misses.

- [ ] **Skim git log:**

```bash
git log --oneline -10
```

Expect six commits in this order (Task 1 → Task 6), each with the `Co-Authored-By` trailer.

- [ ] **Stop. Hand off to the user for review and mainnet deploy.**

Per [memory/feedback_deploy_safety.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_deploy_safety.md), the user owns the mainnet deploy decision. This branch may need rebase onto current `main` before deploy per [memory/project_ponzi_math_deploy_lineage.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/project_ponzi_math_deploy_lineage.md).

---

## Out of scope (deliberately deferred)

- **Float → Nat e8s as accounting source of truth.** The whole pot/seed-reserve/repayments Float plumbing is fragile at large scale. `validateEightDecimals` is also subtly wrong for some valid decimals due to IEEE rounding. Fixing this is a separate, larger refactor — flagged in the audit summary as a longer-term concern.
- **`generalLedger` stable-memory migration.** With `persistent actor` (EOP), upgrade serialization no longer iterates the heap, so the medium-long-term "upgrade trap" concern from the canister-security skill doesn't apply here. The relevant residual concern is the query response cap, which Task 6 addresses. A move to a stable BTree is unnecessary at current scale.
- **Pre-blackhole hardening.** The TEST_ADMIN test-hatch block (`createBackdatedGame`, `adminMergeBackerPosition`, `adminClearAllBackerPositions`, `adminSweepUntracked`, and now `adminForceReset`) is correctly gated and clearly labeled `"DELETE THIS ENTIRE BLOCK BEFORE BLACKHOLING."` Removal is a deploy-time decision, not a code change to make now.
- **Pagination for `getAllGames`, `adminGetEventsByRound`, etc.** These also grow unbounded but more slowly. Add pagination if they hit the cap; out of scope here.
- **Delete dead `ponzi_math/migration.mo`.** Tracked in a separate spawned task — that file was unwired in PR #31 but never deleted; not a security issue.
