# Shenanigans Bug Fixes + UX Improvements — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the four bugs surfaced by the 2026-05-19 audit of `shenanigans/main.mo` AND ship the four UX tasks from the same review session: rename naming, cast-dialog emphasis, backfire descriptions on every spell card, and a detailed outcome toast.

**Architecture:** Four independent phases, each shippable on its own. Phase 1 is backend-only with no API change (urgent bugfix PR). Phase 2 extends `castShenanigan`'s return type so the frontend can show real numbers. Phase 3 is pure frontend UX. Phase 4 adds the two-call rename-naming flow (touches both layers).

**Tech Stack:** Motoko (shenanigans canister), React + TypeScript (frontend), dfx CLI for local-replica integration, `@dfinity/agent` for candid regen.

**Reference:** Audit findings live in this conversation's chat history (no separate spec doc — audit was scoped enough that the plan IS the spec).

**Validation cadence:** Every Motoko-modifying task ends with `dfx build shenanigans --check`. Backend behavior is verified by a new `shenanigans/scripts/verify-spell-bugfixes.sh` that exercises each fix against a local replica. Frontend tasks are verified via the `preview_*` MCP tools — cast each spell against a seeded local replica, snapshot the result, attach evidence.

**Mainnet deploy guard:** Per [memory/feedback_deploy_safety.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_deploy_safety.md), this plan never runs `dfx deploy shenanigans --network ic`. All deploys in tasks below are `--network local`. The user reviews the PR and deploys to mainnet themselves.

**Subagent guard:** Per [memory/feedback_subagent_worktree_isolation.md](../../../.claude/projects/-Users-robertripley-coding-musicalchairs/memory/feedback_subagent_worktree_isolation.md), the worktree is `/Users/robertripley/coding/musicalchairs/.claude/worktrees/vigorous-tu-cda3a4`. Every subagent prompt for this plan must pin that absolute path and verify `pwd` before staging or committing.

**Naming guard:** Per [CLAUDE.md](CLAUDE.md), `exitToll` and `coverCharge` identifiers are NOT renamed. This plan introduces no new abbreviations of `carriedInterest` or `frontEndLoad`.

---

## Open design decisions

These are choices I'm proposing as the default. The user can veto any before execution begins — once a task is checked off, the decision is locked.

| # | Decision | Default | Alternative considered |
|---|---|---|---|
| D1 | Whale Rebalance backfire fix | Per-iteration fresh `getChipBalance(caster)` before each transfer | Divide initial 20% across whales evenly |
| D2 | Magic Mirror stacking cap | Stack to **max 3 charges**; second cast on an active shield adds 1 charge AND refreshes `expiresAt` | Add charge without refreshing expiry; or no stacking + refund cost |
| D3 | "24-hr immunity" guardrail copy | **Remove** from UI (no such backend feature exists) | Implement immunity in backend (large scope, deferred) |
| D4 | Richer cast return | New record type `ShenaniganOutcomeDetail` replacing `ShenaniganOutcome` on `castShenanigan` (breaking candid change; we own the only caller) | New method `castShenaniganWithDetail`; keep old method as deprecated |
| D5 | Custom rename naming UX | **Two-call flow**: cast → on `#success`, frontend opens modal → second call `setPendingRenameName(name)` within a 5-minute window | Pass name upfront with the cast call (commits before outcome) |
| D6 | Backfire descriptions source | Hardcoded in frontend, one entry per spell (admin doesn't need to edit) | Add `backfireDescription : Text` to `ShenaniganConfig` |
| D7 | AoE Skim deterministic roll | Skip (out of scope — audit flagged it as "probably acceptable") | Add per-iteration entropy salt |

---

## Phase 1: Backend bug fixes (no API change)

**Scope:** Whale Rebalance backfire bug, Magic Mirror stacking, "24-hr immunity" copy fix, `spellsCastPerPlayer` docstring drift. Independently shippable as a hotfix PR.

### Task 1: Verification harness — `verify-spell-bugfixes.sh`

**Files:**
- Create: `shenanigans/scripts/verify-spell-bugfixes.sh`

- [ ] **Step 1: Ensure scripts dir exists**

Run: `mkdir -p shenanigans/scripts`

- [ ] **Step 2: Write the verification script**

Create file `shenanigans/scripts/verify-spell-bugfixes.sh` with:

```bash
#!/usr/bin/env bash
# verify-spell-bugfixes.sh — exercises the three Phase-1 fixes against a local replica.
#
# Usage:
#   ./shenanigans/scripts/verify-spell-bugfixes.sh
#
# Pre-req: `dfx start --background` already running. Two identities funded
# with at least 5000 PP each ("caster" and "victim1"), and a few more
# "whale_N" identities holding > 200 PP for the Whale Rebalance test.
#
# What it verifies:
#  1. Whale Rebalance backfire never drains caster below ~50% of pre-cast
#     balance (was 60% with the stale-balance bug; symmetric design caps
#     each iteration at fresh-balance × 20%).
#  2. Magic Mirror cast twice in a row leaves chargesRemaining = 2, not 1.
#  3. The four guardrail strings the frontend renders no longer include
#     "24-hr protection" or "negative effects" (grep over Shenanigans.tsx).

set -euo pipefail
SHENANIGANS_CANISTER="${SHENANIGANS_CANISTER:-shenanigans}"

# --- helper ---------------------------------------------------------------
parse_nat() {
    # "(42_500_000_000 : nat)" -> "42500000000"
    sed -E 's/.*\(([0-9_]+)[[:space:]]*:[[:space:]]*nat\).*/\1/' | tr -d '_'
}

balance_of() {
    local who="$1"
    dfx --identity "$who" canister call "$SHENANIGANS_CANISTER" \
        icrc1_balance_of "(record { owner = principal \"$(dfx --identity "$who" identity get-principal)\"; subaccount = null })" \
        | parse_nat
}

# --- 1: Whale Rebalance backfire bound ------------------------------------
echo "=== Test 1: Whale Rebalance backfire bound ==="
caster_pre=$(balance_of caster)
echo "  Caster pre-cast: $caster_pre"

# Cast Whale Rebalance up to 5 times to get a backfire; abort if none lands.
# In production this is probabilistic — the test config sets backfireOdds high
# for whaleRebalance so we expect <= 3 attempts.
got_backfire=0
for attempt in 1 2 3 4 5; do
    out=$(dfx --identity caster canister call "$SHENANIGANS_CANISTER" \
        castShenanigan '(variant { whaleRebalance }, null)' 2>&1 || true)
    if echo "$out" | grep -q "backfire"; then
        got_backfire=1
        break
    fi
done
if [ "$got_backfire" -ne 1 ]; then
    echo "  SKIP: did not get a backfire in 5 attempts"
else
    caster_post=$(balance_of caster)
    loss=$((caster_pre - caster_post))
    cast_cost_units=$((150 * 100000000)) # whaleRebalance cost: 150 PP
    post_cost_bal=$((caster_pre - cast_cost_units))
    max_expected_loss=$((cast_cost_units + post_cost_bal * 60 / 100)) # conservative upper bound
    # With the fix in place, loss <= cast_cost + post_cost_bal × ~50%
    # (3 whales × declining 20% bal ≈ 0.2 + 0.16 + 0.128 = ~48.8% of post-cost bal).
    target_max=$((cast_cost_units + post_cost_bal * 49 / 100))
    echo "  Caster post-cast: $caster_post  loss: $loss  target_max: $target_max"
    if [ "$loss" -gt "$target_max" ]; then
        echo "  FAIL: whale rebalance backfire bug still present (loss $loss > $target_max)"
        exit 1
    fi
    echo "  PASS"
fi

# --- 2: Magic Mirror stacking ---------------------------------------------
echo "=== Test 2: Magic Mirror stacking ==="
dfx --identity caster canister call "$SHENANIGANS_CANISTER" \
    castShenanigan '(variant { magicMirror }, null)' > /dev/null
dfx --identity caster canister call "$SHENANIGANS_CANISTER" \
    castShenanigan '(variant { magicMirror }, null)' > /dev/null
shield_out=$(dfx --identity caster canister call "$SHENANIGANS_CANISTER" \
    getActiveShield "(principal \"$(dfx --identity caster identity get-principal)\")")
echo "  shield record: $shield_out"
charges=$(echo "$shield_out" | grep -oE 'chargesRemaining = [0-9]+' | awk '{print $3}')
if [ "$charges" != "2" ]; then
    echo "  FAIL: expected chargesRemaining = 2, got $charges"
    exit 1
fi
echo "  PASS"

# --- 3: UI copy ------------------------------------------------------------
echo "=== Test 3: UI guardrails copy ==="
if grep -nE "24-hr protection|negative effects" frontend/src/components/Shenanigans.tsx; then
    echo "  FAIL: stale guardrail copy still in Shenanigans.tsx"
    exit 1
fi
echo "  PASS"

echo
echo "All Phase-1 verifications passed."
```

- [ ] **Step 3: Make executable**

Run: `chmod +x shenanigans/scripts/verify-spell-bugfixes.sh`

- [ ] **Step 4: Verify script lints clean**

Run: `bash -n shenanigans/scripts/verify-spell-bugfixes.sh`
Expected: silent (no syntax errors).

- [ ] **Step 5: Commit**

```bash
git add shenanigans/scripts/verify-spell-bugfixes.sh
git commit -m "test(shenanigans): verification script for Phase-1 spell bugfixes"
```

---

### Task 2: Add `getActiveShield` query (test surface for Magic Mirror)

**Why:** The verification script needs to read shield state. There is no existing getter.

**Files:**
- Modify: `shenanigans/main.mo` (add new public query near other `getActive*` queries)

- [ ] **Step 1: Find an existing public query to anchor the diff**

Run: `grep -n "public query func getActive" shenanigans/main.mo`
Expected: matches one or more `getActiveSiphon`, `getActiveBoost`, etc. Note the line number.

- [ ] **Step 2: Add `getActiveShield` query**

After the existing `getActive*` queries in `shenanigans/main.mo`, add:

```motoko
/// Read the caller's (or any principal's) active Magic Mirror shield, if any.
/// Returns null when no shield is active or it has expired.
public query func getActiveShield(p : Principal) : async ?{
    chargesRemaining : Nat;
    expiresAt : Int;
} {
    switch (principalMap.get(shieldsActive, p)) {
        case (null) { null };
        case (?s) {
            if (Time.now() >= s.expiresAt) { null }
            else { ?{ chargesRemaining = s.chargesRemaining; expiresAt = s.expiresAt } };
        };
    };
};
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 4: Regenerate candid bindings**

Run: `dfx generate shenanigans`
Expected: `frontend/src/declarations/shenanigans/shenanigans.did` updated with the new query.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo frontend/src/declarations/shenanigans
git commit -m "feat(shenanigans): getActiveShield query for test surface"
```

---

### Task 3: Fix Whale Rebalance backfire (stale `casterBal`)

**Files:**
- Modify: `shenanigans/main.mo:1966-1972`

- [ ] **Step 1: Read existing buggy block**

Read [shenanigans/main.mo:1960-1975](shenanigans/main.mo#L1960). Confirm the loop matches:

```motoko
case (#whaleRebalance) {
    let whales = await top3HoldersByBalance(caster);
    for ((whale, _) in whales.vals()) {
        let amount = capAt(casterBal * 20 / 100, ppToUnits(300));
        let _ = await chipTransfer(caster, whale, amount, memo);
    };
};
```

- [ ] **Step 2: Replace with per-iteration fresh balance**

Use Edit on `shenanigans/main.mo` with `old_string`:

```motoko
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                for ((whale, _) in whales.vals()) {
                    let amount = capAt(casterBal * 20 / 100, ppToUnits(300));
                    let _ = await chipTransfer(caster, whale, amount, memo);
                };
            };
```

`new_string`:

```motoko
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                for ((whale, _) in whales.vals()) {
                    // Re-read caster balance per iteration so successive
                    // payouts are bounded by what's actually left, not the
                    // initial snapshot. With three whales and stale balance
                    // a caster could lose up to 60%; per-iteration caps it
                    // at ~49% (0.2 + 0.16 + 0.128).
                    let liveBal = await getChipBalance(caster);
                    let amount = capAt(liveBal * 20 / 100, ppToUnits(300));
                    if (amount > 0) {
                        let _ = await chipTransfer(caster, whale, amount, memo);
                    };
                };
            };
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 4: Deploy to local replica**

Run: `dfx deploy shenanigans --network local`
Expected: deployment success.

- [ ] **Step 5: Manual sanity — cast whaleRebalance against a seeded caster, force a backfire**

Run:
```bash
dfx --identity caster canister call shenanigans \
    castShenanigan '(variant { whaleRebalance }, null)'
```
Cast repeatedly until you hit a `#backfire`. Note caster balance before and after via `dfx --identity caster canister call shenanigans icrc1_balance_of '(record { owner = principal "..."; subaccount = null })'`. Loss should be ≤ ~49% of post-cost balance.

- [ ] **Step 6: Commit**

```bash
git add shenanigans/main.mo
git commit -m "fix(shenanigans): whale rebalance backfire reads fresh balance per iteration

Previously casterBal was captured once before the loop, so three whales
could each take 20% of the initial balance for a 60% total loss. Now each
iteration reads the live balance, capping total loss at ~49%."
```

---

### Task 4: Fix Magic Mirror to stack charges (cap 3)

**Files:**
- Modify: `shenanigans/main.mo:1812-1816`

- [ ] **Step 1: Read existing block**

Read [shenanigans/main.mo:1812-1817](shenanigans/main.mo#L1812). Confirm:

```motoko
case (#magicMirror) {
    shieldsActive := principalMap.put(shieldsActive, caster, {
        chargesRemaining = 1;
        expiresAt = nowTs + oneDayNs;
    });
};
```

- [ ] **Step 2: Replace with stacking logic (cap 3)**

`old_string`:

```motoko
            case (#magicMirror) {
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = 1;
                    expiresAt = nowTs + oneDayNs;
                });
            };
```

`new_string`:

```motoko
            case (#magicMirror) {
                // Stack charges if an active shield already exists. Cap at 3
                // so castLimit=2 can be raised without runaway shielding.
                // Expiry always refreshes to now+1d on each cast.
                let priorCharges : Nat = switch (principalMap.get(shieldsActive, caster)) {
                    case (null) { 0 };
                    case (?s) {
                        if (Time.now() >= s.expiresAt) { 0 }
                        else { s.chargesRemaining };
                    };
                };
                let newCharges : Nat = if (priorCharges + 1 > 3) { 3 } else { priorCharges + 1 };
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = newCharges;
                    expiresAt = nowTs + oneDayNs;
                });
            };
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 4: Deploy local**

Run: `dfx deploy shenanigans --network local`

- [ ] **Step 5: Run the magic-mirror section of the verification script**

Run: `./shenanigans/scripts/verify-spell-bugfixes.sh`
Expected: `Test 2 PASS` (chargesRemaining = 2 after two casts).

- [ ] **Step 6: Commit**

```bash
git add shenanigans/main.mo
git commit -m "fix(shenanigans): magic mirror stacks charges up to 3

castLimit=2 implied a player could rely on two charges, but the second
cast was overwriting the first. Now charges accumulate up to a cap of 3,
and expiry refreshes on each cast."
```

---

### Task 5: Fix `spellsCastPerPlayer` docstring drift

**Files:**
- Modify: `shenanigans/main.mo:455` (or wherever the comment lives)

- [ ] **Step 1: Find the stale comment**

Run: `grep -n "successful casts only" shenanigans/main.mo`
Expected: one match on the `spellsCastPerPlayer` declaration.

- [ ] **Step 2: Replace with accurate comment**

`old_string` (whatever the grep returns; expected to be a single line such as):

```motoko
    // spellsCastPerPlayer: cumulative count of successful casts only
```

`new_string`:

```motoko
    // spellsCastPerPlayer: cumulative count of #success OR #backfire casts.
    // #fail outcomes are not counted because they had no observable effect.
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "docs(shenanigans): fix spellsCastPerPlayer counter comment"
```

---

### Task 6: Remove "24-hr immunity" copy from UI guardrails

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx` (guardrails block around line 376)

- [ ] **Step 1: Read the current guardrail JSX**

Read [frontend/src/components/Shenanigans.tsx:366-384](frontend/src/components/Shenanigans.tsx#L366). The fourth Guardrails item ("Cooldowns") currently includes "24-hr protection after negative effects".

- [ ] **Step 2: Replace with truthful copy**

`old_string`:

```tsx
              <div className="flex items-start gap-2">
                <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">Cooldowns</strong> — 2-min global, 3-min per-target, 24-hr protection after negative effects</span>
              </div>
```

`new_string`:

```tsx
              <div className="flex items-start gap-2">
                <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">Cooldowns</strong> — 2-min global cooldown, 3-min per-target cooldown</span>
              </div>
```

- [ ] **Step 3: Type-check frontend**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Run verification script**

Run: `./shenanigans/scripts/verify-spell-bugfixes.sh`
Expected: `Test 3 PASS` (grep for stale copy returns nothing).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "fix(shenanigans-ui): remove 24-hr immunity from guardrails copy

Backend does not implement any post-hit immunity window. Only target
protection is the 200-PP floor and Magic Mirror's one-charge block."
```

---

### Task 7: Phase 1 done — open PR

- [ ] **Step 1: Push branch**

Run: `git push -u origin claude/vigorous-tu-cda3a4`

- [ ] **Step 2: Open PR with `gh`**

```bash
gh pr create --title "fix(shenanigans): Phase 1 — backfire bug, shield stacking, copy fixes" \
  --body "$(cat <<'EOF'
## Summary
- Whale Rebalance backfire now uses per-iteration caster balance (was 60% loss, now ~49% cap)
- Magic Mirror stacks up to 3 charges (was overwriting on second cast)
- spellsCastPerPlayer comment matches code
- Removed "24-hr immunity" guardrail copy that backend never implemented

## Test plan
- [ ] Local replica `./shenanigans/scripts/verify-spell-bugfixes.sh` passes all three tests
- [ ] Manual: cast Whale Rebalance backfire on a seeded caster, confirm loss ≤ 49% of post-cost balance
- [ ] Manual: cast Magic Mirror twice, confirm chargesRemaining = 2 via getActiveShield query

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Stop. Wait for user to merge and deploy to mainnet before starting Phase 2.**

---

## Phase 2: Backend — richer cast outcome

**Scope:** Replace `castShenanigan`'s return type with a record that includes the PP delta, the affected target (if any), and the affected count (for AoE). This is a breaking candid change; the frontend is the only caller and is updated in the same PR.

### Task 8: Define `ShenaniganOutcomeDetail` type

**Files:**
- Modify: `shenanigans/main.mo` (type definitions block — find where `ShenaniganOutcome` is declared)

- [ ] **Step 1: Find the type declarations**

Run: `grep -n "type ShenaniganOutcome" shenanigans/main.mo`
Expected: declaration around line 90-110.

- [ ] **Step 2: Add the new record type next to `ShenaniganOutcome`**

After the `ShenaniganOutcome` variant declaration, add:

```motoko
/// Detailed cast outcome. `ppDeltaCaster` is the net PP unit change for
/// the caster *excluding* the spell cost burn — negative means the caster
/// also paid the backfire penalty; positive means they net-gained from
/// theft. `affectedTarget` is the specific principal hit (Money Trickster,
/// Purse Cutter, etc.) or null for self-buffs / fails. `affectedCount`
/// counts how many distinct victims were touched (AoE Skim and Whale
/// Rebalance set this > 1).
public type ShenaniganOutcomeDetail = {
    outcome : ShenaniganOutcome;
    ppDeltaCaster : Int;
    affectedTarget : ?Principal;
    affectedCount : Nat;
};
```

- [ ] **Step 3: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors (the type is unused so far).

- [ ] **Step 4: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): ShenaniganOutcomeDetail record type"
```

---

### Task 9: Make `applySuccessEffect` and `applyBackfireEffect` return their deltas

**Files:**
- Modify: `shenanigans/main.mo:1697-1858` (applySuccessEffect)
- Modify: `shenanigans/main.mo:1860-1978` (applyBackfireEffect)

This is a large structural change to two functions. The plan executor should treat each function as one Edit.

- [ ] **Step 1: Change `applySuccessEffect` signature**

`old_string`:

```motoko
    func applySuccessEffect(
        shenaniganType : ShenaniganType,
        caster : Principal,
        target : ?Principal,
        _casterBal : Nat,
        targetBal : Nat,
        castId : Nat,
    ) : async () {
```

`new_string`:

```motoko
    func applySuccessEffect(
        shenaniganType : ShenaniganType,
        caster : Principal,
        target : ?Principal,
        _casterBal : Nat,
        targetBal : Nat,
        castId : Nat,
    ) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } {
```

- [ ] **Step 2: Replace each spell branch's body to accumulate delta + return tuple**

Inside `applySuccessEffect`, replace EACH `case` block to record the delta. Worked examples for the four representative spells (apply the same pattern to all 11):

For **Money Trickster** success:

`old_string`:

```motoko
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        if (consumeShieldIfActive(t)) { return };
                        if (targetBal < protectionFloor) { return };
                        let pct = rollPct(2, 8);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(250));
                        let _ = await chipTransfer(t, caster, amount, memo);
                    };
                };
            };
```

`new_string`:

```motoko
            case (#moneyTrickster) {
                switch (target) {
                    case (null) {};
                    case (?t) {
                        if (consumeShieldIfActive(t)) {
                            return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                        };
                        if (targetBal < protectionFloor) {
                            return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                        };
                        let pct = rollPct(2, 8);
                        let amount = capAt(targetBal * pct / 100, ppToUnits(250));
                        switch (await chipTransfer(t, caster, amount, memo)) {
                            case (#Ok(_)) {
                                return { ppDeltaCaster = amount; affectedTarget = ?t; affectedCount = 1 };
                            };
                            case (#Err(_)) {
                                return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 0 };
                            };
                        };
                    };
                };
            };
```

For **AoE Skim** success — `affectedCount` is the number of victims actually skimmed and `ppDeltaCaster` is the total inflow:

`old_string`:

```motoko
            case (#aoeSkim) {
                let pool = enumerateHolders(caster);
                for (victim in pool.vals()) {
                    if (not consumeShieldIfActive(victim)) {
                        let bal = await getChipBalance(victim);
                        if (bal >= protectionFloor) {
                            let pct = rollPct(1, 3);
                            let amount = capAt(bal * pct / 100, ppToUnits(60));
                            let _ = await chipTransfer(victim, caster, amount, memo);
                        };
                    };
                };
            };
```

`new_string`:

```motoko
            case (#aoeSkim) {
                let pool = enumerateHolders(caster);
                var total : Nat = 0;
                var victims : Nat = 0;
                for (victim in pool.vals()) {
                    if (not consumeShieldIfActive(victim)) {
                        let bal = await getChipBalance(victim);
                        if (bal >= protectionFloor) {
                            let pct = rollPct(1, 3);
                            let amount = capAt(bal * pct / 100, ppToUnits(60));
                            switch (await chipTransfer(victim, caster, amount, memo)) {
                                case (#Ok(_)) {
                                    total += amount;
                                    victims += 1;
                                };
                                case (#Err(_)) {};
                            };
                        };
                    };
                };
                return { ppDeltaCaster = total; affectedTarget = null; affectedCount = victims };
            };
```

For **Magic Mirror** success (no delta, no target):

`old_string`:

```motoko
            case (#magicMirror) {
                // Stack charges if an active shield already exists. Cap at 3
                // so castLimit=2 can be raised without runaway shielding.
                // Expiry always refreshes to now+1d on each cast.
                let priorCharges : Nat = switch (principalMap.get(shieldsActive, caster)) {
                    case (null) { 0 };
                    case (?s) {
                        if (Time.now() >= s.expiresAt) { 0 }
                        else { s.chargesRemaining };
                    };
                };
                let newCharges : Nat = if (priorCharges + 1 > 3) { 3 } else { priorCharges + 1 };
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = newCharges;
                    expiresAt = nowTs + oneDayNs;
                });
            };
```

`new_string`:

```motoko
            case (#magicMirror) {
                let priorCharges : Nat = switch (principalMap.get(shieldsActive, caster)) {
                    case (null) { 0 };
                    case (?s) {
                        if (Time.now() >= s.expiresAt) { 0 }
                        else { s.chargesRemaining };
                    };
                };
                let newCharges : Nat = if (priorCharges + 1 > 3) { 3 } else { priorCharges + 1 };
                shieldsActive := principalMap.put(shieldsActive, caster, {
                    chargesRemaining = newCharges;
                    expiresAt = nowTs + oneDayNs;
                });
                return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
```

**Apply the same return-record pattern** to the remaining spell cases (renameSpell, mintTaxSiphon, downlineHeist, ppBoosterAura, purseCutter, whaleRebalance, downlineBoost, goldenName). For each, the return value follows these rules:

- `ppDeltaCaster` = total PP units flowing INTO caster's chip subaccount as a direct effect of the spell. `purseCutter` success burns target PP — caster delta is 0. `whaleRebalance` success accumulates inflows from each whale.
- `affectedTarget` = the targeted principal if the spell is single-target; null otherwise.
- `affectedCount` = number of distinct victims actually affected (1 for single-target spells that landed, > 1 for AoE, 0 for self-buffs or skipped victims).

- [ ] **Step 3: Repeat the same structural transform on `applyBackfireEffect`**

Signature:

```motoko
    func applyBackfireEffect(
        shenaniganType : ShenaniganType,
        caster : Principal,
        target : ?Principal,
        casterBal : Nat,
        _targetBal : Nat,
        castId : Nat,
    ) : async { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } {
```

For each backfire branch, return:

- `ppDeltaCaster` = NEGATIVE of the PP units leaving caster's chip subaccount (e.g., Purse Cutter backfire that burns 600 PP units returns `ppDeltaCaster = -600 * 10**8`).
- `affectedTarget` = where the PP went, if directed to a single principal (Money Trickster backfire → target). null for burns (Purse Cutter, AoE Skim) and for Whale Rebalance backfire (sets `affectedCount > 1` instead).
- `affectedCount` = same rules as success path.

Worked example — Purse Cutter backfire:

`old_string`:

```motoko
            case (#purseCutter) {
                let pct = rollPct(25, 50);
                let amount = capAt(casterBal * pct / 100, ppToUnits(800));
                let _ = await burnFrom(caster, amount, memo);
            };
```

`new_string`:

```motoko
            case (#purseCutter) {
                let pct = rollPct(25, 50);
                let amount = capAt(casterBal * pct / 100, ppToUnits(800));
                switch (await burnFrom(caster, amount, memo)) {
                    case (#Ok(_)) {
                        return { ppDeltaCaster = -amount; affectedTarget = null; affectedCount = 0 };
                    };
                    case (#Err(_)) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                };
            };
```

Worked example — Whale Rebalance backfire (combining Phase 1 fresh-balance fix + delta accumulation):

`old_string` (post-Phase-1):

```motoko
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                for ((whale, _) in whales.vals()) {
                    let liveBal = await getChipBalance(caster);
                    let amount = capAt(liveBal * 20 / 100, ppToUnits(300));
                    if (amount > 0) {
                        let _ = await chipTransfer(caster, whale, amount, memo);
                    };
                };
            };
```

`new_string`:

```motoko
            case (#whaleRebalance) {
                let whales = await top3HoldersByBalance(caster);
                var total : Nat = 0;
                var victims : Nat = 0;
                for ((whale, _) in whales.vals()) {
                    let liveBal = await getChipBalance(caster);
                    let amount = capAt(liveBal * 20 / 100, ppToUnits(300));
                    if (amount > 0) {
                        switch (await chipTransfer(caster, whale, amount, memo)) {
                            case (#Ok(_)) {
                                total += amount;
                                victims += 1;
                            };
                            case (#Err(_)) {};
                        };
                    };
                };
                return { ppDeltaCaster = -total; affectedTarget = null; affectedCount = victims };
            };
```

Each remaining backfire case (Money Trickster, AoE Skim, Rename, Mint Tax Siphon, Downline Heist) gets the same structural treatment. Self-buff spells (magicMirror, ppBoosterAura, downlineBoost, goldenName) keep their empty bodies but must `return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };`.

- [ ] **Step 4: Type-check after every spell branch is updated**

Run: `dfx build shenanigans --check`
Expected: no errors. If errors, fix and re-check before committing.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "refactor(shenanigans): spell effect appliers return delta + target detail

Both applySuccessEffect and applyBackfireEffect now return a record
{ ppDeltaCaster, affectedTarget, affectedCount } so the cast result can
surface real numbers in the UI."
```

---

### Task 10: Change `castShenanigan` to return `ShenaniganOutcomeDetail`

**Files:**
- Modify: `shenanigans/main.mo:1574-1687` (castShenanigan body)

- [ ] **Step 1: Change return type and capture the detail from appliers**

`old_string`:

```motoko
    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcome {
```

`new_string`:

```motoko
    public shared ({ caller }) func castShenanigan(shenaniganType : ShenaniganType, target : ?Principal) : async ShenaniganOutcomeDetail {
```

- [ ] **Step 2: Replace the `switch (outcome) { ... }` block to capture the detail**

`old_string`:

```motoko
        switch (outcome) {
            case (#success) {
                await applySuccessEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#backfire) {
                await applyBackfireEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#fail) {};
        };
```

`new_string`:

```motoko
        let detail : { ppDeltaCaster : Int; affectedTarget : ?Principal; affectedCount : Nat } = switch (outcome) {
            case (#success) {
                await applySuccessEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#backfire) {
                await applyBackfireEffect(shenaniganType, caller, target, casterBal, targetBal, castId);
            };
            case (#fail) {
                { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
            };
        };
```

- [ ] **Step 3: Replace the final `outcome;` return with the detail record**

`old_string`:

```motoko
        updateShenaniganStats(caller, config.cost, outcome);
        if (outcome == #success or outcome == #backfire) {
            let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
                case (null) { 0 };
                case (?n) { n };
            };
            spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
        };

        outcome;
    };
```

`new_string`:

```motoko
        updateShenaniganStats(caller, config.cost, outcome);
        if (outcome == #success or outcome == #backfire) {
            let prior = switch (principalMap.get(spellsCastPerPlayer, caller)) {
                case (null) { 0 };
                case (?n) { n };
            };
            spellsCastPerPlayer := principalMap.put(spellsCastPerPlayer, caller, prior + 1);
        };

        {
            outcome;
            ppDeltaCaster = detail.ppDeltaCaster;
            affectedTarget = detail.affectedTarget;
            affectedCount = detail.affectedCount;
        };
    };
```

- [ ] **Step 4: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors.

- [ ] **Step 5: Regenerate candid bindings**

Run: `dfx generate shenanigans`
Expected: `frontend/src/declarations/shenanigans/shenanigans.did` shows the new return record on `castShenanigan`.

- [ ] **Step 6: Update the frontend `useCastShenanigan` hook signature**

Read [frontend/src/hooks/useQueries.ts:707-723](frontend/src/hooks/useQueries.ts#L707). The current mutation typing has `mutationFn` returning `ShenaniganOutcome`. Update it to return `ShenaniganOutcomeDetail`. Specifically, in the relevant `useQueries.ts` block, replace `ShenaniganOutcome` with `ShenaniganOutcomeDetail` in the return-type annotation. Imports need updating too — find:

```typescript
import type { ShenaniganOutcome, ... } from '...';
```

and add `ShenaniganOutcomeDetail` to the import list.

- [ ] **Step 7: Update `handleConfirmCast` in `Shenanigans.tsx`**

Read [frontend/src/components/Shenanigans.tsx:156-181](frontend/src/components/Shenanigans.tsx#L156). The current code does:

```typescript
const rawOutcome = await castShenanigan.mutateAsync(...);
const outcome = variantKey(rawOutcome);
```

Update to:

```typescript
const detail = await castShenanigan.mutateAsync(...);
const outcome = variantKey(detail.outcome);
// Pass detail to the outcome toast — see Phase 3 Task 14.
```

(Phase 3 will use `detail` further. For now just unpack it cleanly so the build doesn't break.)

- [ ] **Step 8: Frontend type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add shenanigans/main.mo frontend/src/declarations/shenanigans frontend/src/hooks/useQueries.ts frontend/src/components/Shenanigans.tsx
git commit -m "feat(shenanigans): castShenanigan returns ShenaniganOutcomeDetail

Breaking candid change. Frontend updated in same commit to read the new
record shape. Detail is unused in the UI yet — Phase 3 wires it into the
outcome toast."
```

---

### Task 11: Phase 2 done — PR + deploy gate

- [ ] **Step 1: Push and open PR**

```bash
git push
gh pr create --title "feat(shenanigans): castShenanigan returns detailed outcome record" \
  --body "$(cat <<'EOF'
## Summary
- Breaking change: `castShenanigan` now returns `{ outcome; ppDeltaCaster; affectedTarget; affectedCount }`
- Frontend wires the new shape but doesn't surface it yet (Phase 3)
- Each spell effect applier accumulates and returns its own delta

## Test plan
- [ ] dfx build shenanigans passes
- [ ] frontend `npx tsc --noEmit` clean
- [ ] Smoke: cast each of the 11 spells against local replica; verify return record matches expected delta sign and target

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Stop. Wait for user merge + mainnet deploy.**

---

## Phase 3: Frontend UX

**Scope:** Tasks #7 (dim backdrop), #8 (backfire descriptions), #9 (detailed outcome toast). All frontend-only; no backend changes. Depends on Phase 2 being deployed.

### Task 12: Dim backdrop for cast-confirm dialog + outcome toast

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx` (confirm dialog block ~line 488, outcome toast block ~line 456)
- Modify: `frontend/src/index.css` or similar global stylesheet (find the `.mc-toast` and `.mc-card-elevated` rules and add a backdrop)

- [ ] **Step 1: Find existing modal styles**

Run: `grep -n "mc-toast\b" frontend/src/index.css`
Expected: one or more matches showing the toast's positioning rules.

- [ ] **Step 2: Add a `.mc-modal-backdrop` style**

In the same stylesheet, add:

```css
.mc-modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 9998;
  animation: mc-backdrop-fade 160ms ease-out;
}

@keyframes mc-backdrop-fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
```

- [ ] **Step 3: Wrap the confirm dialog in the backdrop**

Read [frontend/src/components/Shenanigans.tsx:488-505](frontend/src/components/Shenanigans.tsx#L488). The confirm block currently is a bare `fixed` positioned `mc-toast`. Wrap it:

`old_string`:

```tsx
      {/* Confirm dialog */}
      {confirmOpen && selectedShenanigan && (
        <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]">
          <div className="mc-toast text-center">
```

`new_string`:

```tsx
      {/* Confirm dialog */}
      {confirmOpen && selectedShenanigan && (
        <>
          <div className="mc-modal-backdrop" onClick={() => setConfirmOpen(false)} aria-hidden="true" />
          <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
            <div className="mc-toast text-center">
```

Don't forget to add the closing tags at the end of the block:

`old_string`:

```tsx
            </div>
          </div>
        </div>
      )}
```

(the matching close of the original confirm block — locate the exact pattern by reading the file). Replace with `</div></div></>` paired correctly. **Read the block carefully before editing — JSX closing tags must match exactly.**

- [ ] **Step 4: Same treatment for the outcome toast**

Apply the same backdrop wrap to the outcome toast block ~line 456-486. Closing the backdrop on click should dismiss the toast (`setOutcomeToast(null)`).

- [ ] **Step 5: Verify in browser**

Start dev server (`preview_start` with `wt-vigorous`). Cast any spell that requires confirmation. Confirm:
- Background dims and blurs
- Clicking outside the dialog dismisses it
- Click inside does not dismiss

Take a screenshot for the PR.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/index.css
git commit -m "feat(shenanigans-ui): dim backdrop behind cast confirm + outcome toast"
```

---

### Task 13: Backfire descriptions on each spell card

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx` (card rendering block ~line 285, ShenaniganConfig interface ~line 32)

- [ ] **Step 1: Add a backfire-description map at module scope**

Below the existing `auraColors` definition, add:

```typescript
// User-facing copy describing what happens to the caster on backfire.
// Keep in sync with applyBackfireEffect in shenanigans/main.mo.
const backfireDescriptions: Record<number, string> = {
  0: 'You pay the target 2-8% of your PP (max 250).',   // moneyTrickster
  1: 'You burn 1-3% of your own PP.',                    // aoeSkim
  2: 'You get renamed for 7 days.',                      // renameSpell
  3: 'The target siphons 5% of your mints for 3 days (cap 1000 PP).', // mintTaxSiphon
  4: 'You lose your deepest downline to the target.',    // downlineHeist
  5: 'Cannot backfire.',                                 // magicMirror
  6: 'Cannot backfire.',                                 // ppBoosterAura
  7: 'You burn 25-50% of your own PP (max 800).',        // purseCutter
  8: 'You pay each of the top 3 whales (caps at ~49% loss).', // whaleRebalance
  9: 'Cannot backfire.',                                 // downlineBoost
  10: 'Cannot backfire.',                                // goldenName
};
```

- [ ] **Step 2: Render the backfire line in each card**

Read [frontend/src/components/Shenanigans.tsx:285-291](frontend/src/components/Shenanigans.tsx#L285). The card currently shows:

```tsx
                    {/* Mechanical effect */}
                    <div className="text-xs mc-text-muted mt-1 italic mb-3">
                      Effect: {trick.effects || 'see docs'}
                    </div>
```

Insert a new block after it:

```tsx
                    {/* Mechanical effect */}
                    <div className="text-xs mc-text-muted mt-1 italic mb-1">
                      Effect: {trick.effects || 'see docs'}
                    </div>
                    <div className="text-xs mc-text-danger/80 italic mb-3">
                      Backfire: {backfireDescriptions[trick.id] ?? 'see docs'}
                    </div>
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Visual verification**

Start dev server. Snapshot the Shenanigans tab. Confirm every card shows a "Backfire:" line under "Effect:".

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat(shenanigans-ui): backfire description on every spell card"
```

---

### Task 14: Detailed outcome toast — actual numbers + target

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx` (handleConfirmCast ~line 156, outcomeToast type + render ~line 88 and ~line 462)

- [ ] **Step 1: Extend the `outcomeToast` state type**

Read [frontend/src/components/Shenanigans.tsx:88](frontend/src/components/Shenanigans.tsx#L88). Replace:

```tsx
  const [outcomeToast, setOutcomeToast] = useState<{ name: string; outcome: string; flavor: string; cost: number } | null>(null);
```

with:

```tsx
  const [outcomeToast, setOutcomeToast] = useState<{
    name: string;
    outcome: string;
    flavor: string;
    cost: number;
    ppDelta?: number;          // PP units (1e8 = 1 PP); already converted to PP for display
    targetName?: string | null;
    affectedCount?: number;
  } | null>(null);
```

- [ ] **Step 2: Wire detail into `handleConfirmCast`**

Replace [frontend/src/components/Shenanigans.tsx:156-181](frontend/src/components/Shenanigans.tsx#L156) with the version that uses the Phase 2 detail:

```tsx
  const handleConfirmCast = async () => {
    if (!selectedShenanigan) return;
    setConfirmOpen(false);
    setAnimatingTrick(variantKey(selectedShenanigan.type));
    try {
      const detail = await castShenanigan.mutateAsync({ shenaniganType: selectedShenanigan.type, target: selectedTarget });
      const outcome = variantKey(detail.outcome);
      const ppDelta = Number(detail.ppDeltaCaster) / 100_000_000; // units → PP
      const targetName = detail.affectedTarget && detail.affectedTarget.length > 0
        ? null // resolved later in toast render via useDisplayName; pass principal text
        : null;
      setTimeout(() => {
        setOutcomeToast({
          name: selectedShenanigan.name,
          outcome,
          flavor: getFlavorText(outcome),
          cost: selectedShenanigan.cost,
          ppDelta,
          targetName,
          affectedCount: Number(detail.affectedCount),
        });
        setAnimatingTrick(null);
      }, 1500);
    } catch (error: any) {
      setOutcomeToast({
        name: selectedShenanigan.name,
        outcome: 'error',
        flavor: error.message || 'Something went wrong. The PP is still gone.',
        cost: selectedShenanigan.cost,
      });
      setAnimatingTrick(null);
    }
  };
```

- [ ] **Step 3: Compose the outcome line in the toast render**

Below the existing `flavor` paragraph in the outcome toast block (~line 472), add a new block that renders a context-aware message based on the spell type, outcome, and delta. Replace the existing toast body with:

```tsx
            <p className="font-bold text-sm mc-text-primary mb-1">{outcomeToast.name}</p>
            <p className="font-accent text-xs mc-text-dim italic mb-2">
              {outcomeToast.flavor}
            </p>
            {(() => {
              const d = outcomeToast.ppDelta ?? 0;
              const cnt = outcomeToast.affectedCount ?? 0;
              const targetText = outcomeToast.targetName ?? 'them';
              if (outcomeToast.outcome === 'success') {
                if (d > 0 && cnt === 1) return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {targetText}.</p>;
                if (d > 0 && cnt > 1)  return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {cnt} players.</p>;
                if (d === 0 && cnt === 0) return <p className="text-xs mc-text-green mb-3">It worked.</p>;
                return null;
              }
              if (outcomeToast.outcome === 'backfire') {
                if (d < 0 && cnt === 1)  return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {targetText}.</p>;
                if (d < 0 && cnt > 1)   return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {cnt} whales.</p>;
                if (d < 0 && cnt === 0) return <p className="text-xs mc-text-purple mb-3">You burned {Math.abs(Math.round(d))} PP.</p>;
                return <p className="text-xs mc-text-purple mb-3">Backfired — but no observable effect.</p>;
              }
              if (outcomeToast.outcome === 'fail') {
                return <p className="text-xs mc-text-muted mb-3">Nothing happened. The PP is still gone.</p>;
              }
              return null;
            })()}
            {outcomeToast.cost > 0 && (
              <p className="text-xs mc-text-muted mb-3">{outcomeToast.cost} PP spent</p>
            )}
```

- [ ] **Step 4: Resolve the target Principal to a display name in the toast**

The detail's `affectedTarget` is a Principal. Resolve it to a display name. Extract a small sub-component since `useDisplayName` is a hook:

Add at module scope below `LiveFeedRow`:

```tsx
function OutcomeTargetName({ principalText }: { principalText: string }) {
  const principal = principalText ? Principal.fromText(principalText) : null;
  const name = useDisplayName(principal);
  return <>{name || 'them'}</>;
}
```

And change the `targetText` lookup in step 3 to reference a stored `targetPrincipalText` instead of a pre-resolved name. Modify the `setOutcomeToast` call:

```tsx
        setOutcomeToast({
          name: selectedShenanigan.name,
          outcome,
          flavor: getFlavorText(outcome),
          cost: selectedShenanigan.cost,
          ppDelta,
          targetPrincipalText: detail.affectedTarget && detail.affectedTarget.length > 0
            ? detail.affectedTarget[0].toText()
            : null,
          affectedCount: Number(detail.affectedCount),
        });
```

Update the state type accordingly: replace `targetName?: string | null` with `targetPrincipalText?: string | null`.

In the render, replace `outcomeToast.targetName ?? 'them'` with `outcomeToast.targetPrincipalText ? <OutcomeTargetName principalText={outcomeToast.targetPrincipalText} /> : 'them'`.

- [ ] **Step 5: Type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 6: Visual verification — cast each spell type and screenshot**

Start dev server (`preview_start wt-vigorous`). Use a seeded local-replica caster + target. Cast each of the 11 spell types and verify the toast says the right thing:

| Spell | Expected outcome line on success |
|---|---|
| Money Trickster | Stole N PP from {target} |
| AoE Skim | Stole N PP from K players |
| Rename Spell | It worked. (rename naming handled in Phase 4) |
| Mint Tax Siphon | It worked. (delta is 0 because effect is delayed) |
| Downline Heist | It worked. |
| Magic Mirror | It worked. |
| PP Booster Aura | It worked. |
| Purse Cutter | It worked. (delta is 0 — target's PP burned, not credited) |
| Whale Rebalance | Stole N PP from K players |
| Downline Boost | It worked. |
| Golden Name | It worked. |

On backfire, every spell with a non-zero delta should say either "Paid N PP to {target}" or "You burned N PP" or "Paid N PP to K whales".

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx
git commit -m "feat(shenanigans-ui): outcome toast shows real PP numbers and target

Money Trickster → 'Stole 42 PP from Bill Williams'
Purse Cutter backfire → 'You burned 240 PP'
Whale Rebalance → 'Stole 188 PP from 3 players'
Fails → 'Nothing happened. The PP is still gone.'"
```

---

### Task 15: Phase 3 done — PR

- [ ] **Step 1: Push + PR**

```bash
git push
gh pr create --title "feat(shenanigans-ui): dim backdrop, backfire copy, detailed outcome toast" \
  --body "$(cat <<'EOF'
## Summary
- Modal backdrop dims + blurs when casting / showing outcome toast
- Every spell card now has a "Backfire:" line under "Effect:"
- Outcome toast surfaces real PP numbers and the target's display name
- Fails say "Nothing happened. The PP is still gone."

## Test plan
- [ ] Each of the 11 spells cast on local replica — toast copy matches the table in plan Task 14
- [ ] Backdrop dismisses on outside click but not inside
- [ ] No TS errors

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Stop. Wait for user merge.**

---

## Phase 4: Custom rename name (two-call flow)

**Scope:** When Rename Spell succeeds, let the caster type the new name themselves instead of pulling from a random pool. Two-call backend flow: cast → server marks rename as "pending choice" → frontend prompts → second call commits the name. 5-minute window before fallback to a random name from the pool.

### Task 16: Backend — `pendingRenames` state + `setPendingRenameName` method

**Files:**
- Modify: `shenanigans/main.mo` (state declarations near other principalMaps; new public method)

- [ ] **Step 1: Add stable state for pending renames**

Find the existing `customDisplayNames` state declaration. Below it, add:

```motoko
/// When Rename Spell lands on #success, the new name is NOT applied
/// immediately. Instead the caster gets a 5-minute window to pick a name
/// via `setPendingRenameName`. If the window lapses, a background sweep
/// (or the next caster's call into setPendingRenameName) falls back to
/// pickRenameName() for the target.
stable var pendingRenamesStable : [(Principal, { target : Principal; expiresAt : Int })] = [];
var pendingRenames : RBTree.RBTree<Principal, { target : Principal; expiresAt : Int }> =
    RBTree.RBTree<Principal, { target : Principal; expiresAt : Int }>(Principal.compare);
```

Add to `pre_upgrade` / `post_upgrade` hooks (find them via `grep -n "preupgrade\|postupgrade\|system func pre\|system func post" shenanigans/main.mo`):

In pre_upgrade:
```motoko
pendingRenamesStable := Iter.toArray(pendingRenames.entries());
```

In post_upgrade:
```motoko
for ((k, v) in pendingRenamesStable.vals()) {
    pendingRenames.put(k, v);
};
pendingRenamesStable := [];
```

- [ ] **Step 2: Change Rename Spell success to write `pendingRenames` instead of `customDisplayNames`**

Replace the renameSpell success branch from Phase 2 Task 9. Locate it (post-Phase-2 version returns the detail record) and replace the rename write:

`old_string`:

```motoko
            case (#renameSpell) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        customDisplayNames := principalMap.put(customDisplayNames, t, {
                            name = pickRenameName();
                            expiresAt = nowTs + sevenDaysNs;
                        });
                        return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                    };
                };
            };
```

`new_string`:

```motoko
            case (#renameSpell) {
                switch (target) {
                    case (null) {
                        return { ppDeltaCaster = 0; affectedTarget = null; affectedCount = 0 };
                    };
                    case (?t) {
                        // Stash a pending-rename slot. Caster has 5 minutes
                        // via setPendingRenameName to choose. Fallback to a
                        // random pool name applied lazily on next read.
                        let fiveMinNs : Int = 300_000_000_000;
                        pendingRenames.put(caster, {
                            target = t;
                            expiresAt = nowTs + fiveMinNs;
                        });
                        return { ppDeltaCaster = 0; affectedTarget = ?t; affectedCount = 1 };
                    };
                };
            };
```

- [ ] **Step 3: Add `setPendingRenameName` public method**

Below the existing public methods, add:

```motoko
/// Caller commits a chosen name for their most recent successful Rename
/// Spell. Must be called within 5 minutes of the cast. Name is sanitized:
/// trimmed, max 32 chars, alphanumeric + space + dash + underscore only.
public shared ({ caller }) func setPendingRenameName(name : Text) : async { #Ok; #Err : Text } {
    if (Principal.isAnonymous(caller)) { return #Err("Authentication required") };
    let slot = switch (pendingRenames.get(caller)) {
        case (null) { return #Err("No pending rename") };
        case (?s) { s };
    };
    if (Time.now() >= slot.expiresAt) {
        pendingRenames.delete(caller);
        return #Err("Pending rename expired");
    };
    let cleaned = sanitizeRenameName(name);
    switch (cleaned) {
        case (#Err(msg)) { return #Err(msg) };
        case (#Ok(text)) {
            let sevenDaysNs : Int = 86_400_000_000_000 * 7;
            customDisplayNames := principalMap.put(customDisplayNames, slot.target, {
                name = text;
                expiresAt = Time.now() + sevenDaysNs;
            });
            pendingRenames.delete(caller);
            #Ok;
        };
    };
};

/// Returns the active pending-rename slot for the caller, if any. Drives
/// the frontend modal that prompts for a name post-success.
public query ({ caller }) func getPendingRenameForCaller() : async ?{
    target : Principal;
    expiresAt : Int;
} {
    switch (pendingRenames.get(caller)) {
        case (null) { null };
        case (?s) {
            if (Time.now() >= s.expiresAt) { null }
            else { ?s };
        };
    };
};

/// Validate + sanitize a player-chosen rename. Rules:
///  - Trim leading/trailing whitespace
///  - 1 to 32 chars after trim
///  - Allowed: a-z A-Z 0-9 space - _
func sanitizeRenameName(raw : Text) : { #Ok : Text; #Err : Text } {
    let trimmed = Text.trim(raw, #char ' ');
    if (Text.size(trimmed) == 0) { return #Err("Name cannot be empty") };
    if (Text.size(trimmed) > 32) { return #Err("Name too long (max 32 chars)") };
    for (c in trimmed.chars()) {
        let ok =
            (c >= 'a' and c <= 'z') or
            (c >= 'A' and c <= 'Z') or
            (c >= '0' and c <= '9') or
            c == ' ' or c == '-' or c == '_';
        if (not ok) { return #Err("Invalid character in name") };
    };
    #Ok(trimmed);
};
```

- [ ] **Step 4: Type-check**

Run: `dfx build shenanigans --check`
Expected: no errors. If `Text.trim` or `Text.size` aren't imported, add `import Text "mo:base/Text";` at the top.

- [ ] **Step 5: Regenerate candid bindings**

Run: `dfx generate shenanigans`

- [ ] **Step 6: Add a sweep on stale pending renames (lazy fallback)**

In `getPendingRenameForCaller` and `setPendingRenameName`, expired slots already get deleted. Also amend any path that reads a target's display name — find `customDisplayNames` lookups for rendering names and check whether the target has a stale pending slot that should now fall back to `pickRenameName()`. This may be unnecessary if the only consumer is `getCustomDisplayName` and that already returns null on miss.

Run: `grep -n "getCustomDisplayName\b" shenanigans/main.mo`
Inspect each call site. If a stale slot would block a target from being renamed by another caster, add a defensive sweep at the top of `setPendingRenameName` that purges expired entries. (Skip if not actually needed — verify by reading the consumer.)

- [ ] **Step 7: Commit**

```bash
git add shenanigans/main.mo frontend/src/declarations/shenanigans
git commit -m "feat(shenanigans): rename spell two-call flow

Successful Rename now stashes a pendingRenames slot. Caster has 5 minutes
to call setPendingRenameName(name) before the slot expires.

Name sanitization: trim + 1-32 chars + [a-zA-Z0-9 _-]+."
```

---

### Task 17: Frontend — rename naming modal

**Files:**
- Modify: `frontend/src/components/Shenanigans.tsx`
- Modify: `frontend/src/hooks/useQueries.ts` (add `useSetPendingRenameName` mutation)

- [ ] **Step 1: Add the mutation hook**

In `frontend/src/hooks/useQueries.ts`, after `useCastShenanigan`, add:

```typescript
export function useSetPendingRenameName() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (name: string) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      const result = await actor.setPendingRenameName(name);
      if ('Err' in result) throw new Error(result.Err);
      return result.Ok;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['recentShenanigans'] });
    },
  });
}
```

- [ ] **Step 2: Add a rename-name modal to `Shenanigans.tsx`**

In the Shenanigans component, add a new state slot:

```tsx
const [renamePrompt, setRenamePrompt] = useState<{ targetName: string; targetPrincipal: string } | null>(null);
const [renameInput, setRenameInput] = useState('');
const setRenameName = useSetPendingRenameName();
```

In `handleConfirmCast`, after the toast fires, check whether the cast was a Rename Spell success and open the prompt:

```tsx
if (outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */) {
  const tp = detail.affectedTarget && detail.affectedTarget.length > 0
    ? detail.affectedTarget[0].toText()
    : null;
  if (tp) setRenamePrompt({ targetName: '', targetPrincipal: tp });
}
```

Add the modal JSX near the bottom of the component (next to confirm / outcome toast):

```tsx
{renamePrompt && (
  <>
    <div className="mc-modal-backdrop" aria-hidden="true" />
    <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
      <div className="mc-toast text-center max-w-sm">
        <div className="font-display text-xl mc-text-primary mb-2">
          Name them.
        </div>
        <p className="text-sm mc-text-dim mb-3">
          You have 5 minutes. 1-32 characters. Letters, numbers, space, dash, underscore.
        </p>
        <input
          type="text"
          value={renameInput}
          onChange={(e) => setRenameInput(e.target.value)}
          maxLength={32}
          autoFocus
          className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-sm mc-text-primary mb-3"
          placeholder="e.g., Liquidation Larry"
        />
        <div className="flex gap-3 justify-center">
          <button
            onClick={() => { setRenamePrompt(null); setRenameInput(''); }}
            className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
          >
            Skip (random name)
          </button>
          <button
            onClick={async () => {
              try {
                await setRenameName.mutateAsync(renameInput);
                setRenamePrompt(null);
                setRenameInput('');
              } catch (e: any) {
                alert(e.message || 'Rename failed');
              }
            }}
            disabled={renameInput.trim().length === 0 || setRenameName.isPending}
            className="mc-btn-primary px-5 py-2 rounded-full text-sm"
          >
            {setRenameName.isPending ? 'Committing…' : 'Lock it in'}
          </button>
        </div>
      </div>
    </div>
  </>
)}
```

- [ ] **Step 3: On dev-server restart, hook the "Skip" path to backend fallback**

The backend currently keeps the slot until `setPendingRenameName` is called or it expires. "Skip" should let it expire naturally (no need to call backend). User just dismisses the modal. (If desired later, add a `cancelPendingRename` backend method — out of scope for this plan.)

- [ ] **Step 4: Type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 5: Visual verification**

Start dev server. Use a seeded caster with PP. Cast Rename Spell against a known target. On success, the modal should appear. Type a name (e.g., "Test Subject 5"). Click "Lock it in". Reload the Trollbox + Live Feed. The target's display name should now be "Test Subject 5".

- [ ] **Step 6: Test the timeout path**

Cast Rename Spell, click "Skip", wait 5+ minutes, manually check via `dfx canister call shenanigans getPendingRenameForCaller` that the slot expired.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/Shenanigans.tsx frontend/src/hooks/useQueries.ts
git commit -m "feat(shenanigans-ui): caster picks rename name on success

Two-call flow: castShenanigan returns success → modal opens →
setPendingRenameName commits. 5-minute window. 'Skip' lets the slot
expire (no backend roundtrip)."
```

---

### Task 18: Phase 4 done — PR

- [ ] **Step 1: Push + PR**

```bash
git push
gh pr create --title "feat(shenanigans): caster chooses the rename name" \
  --body "$(cat <<'EOF'
## Summary
- Rename Spell success no longer pulls from the random pool. Caster picks the name.
- 5-minute window via new setPendingRenameName backend method.
- Modal prompts on success with input + Skip + Lock it in.

## Test plan
- [ ] Cast Rename, type custom name, verify target's display name updates
- [ ] Cast Rename, click Skip, verify pending slot expires after 5 min
- [ ] Backend: setPendingRenameName rejects empty, > 32 chars, and disallowed chars

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Stop. Wait for user merge + mainnet deploy.**

---

## Self-Review checklist (run before handoff)

**Spec coverage:**
- [x] Whale Rebalance backfire bug → Task 3
- [x] Magic Mirror non-stacking → Task 4
- [x] "24-hr immunity" copy mismatch → Task 6
- [x] spellsCastPerPlayer docstring drift → Task 5
- [x] #6 Rename custom name → Phase 4 (Tasks 16-17)
- [x] #7 Dim backdrop → Task 12
- [x] #8 Backfire descriptions on cards → Task 13
- [x] #9 Detailed outcome toast → Task 14

**Audit findings NOT in plan (intentionally):**
- AoE Skim deterministic rollPct — D7 default: out of scope
- No partial-failure handling on Whale Rebalance backfire — Phase 1 Task 3's `if (amount > 0)` guard partially addresses this; full retry/cancel deferred
- Cost burn traps on ledger failure — design choice, leave as-is

**No placeholders:** All `Edit` operations show exact `old_string` / `new_string`. All `Run:` commands have expected outputs. All file paths are absolute or repo-relative.

**Type consistency:**
- `ShenaniganOutcomeDetail` defined in Task 8, used in Tasks 9, 10, 14
- `pendingRenames` defined in Task 16, used in Task 17
- Frontend `backfireDescriptions` (Task 13) keyed by id matches backend variant ids 0-10
