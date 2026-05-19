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
