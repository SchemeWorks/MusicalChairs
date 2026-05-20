#!/usr/bin/env bash
# verify-spell-bugfixes.sh — exercises the Phase-1 fixes against a local replica.
#
# Usage:
#   ./shenanigans/scripts/verify-spell-bugfixes.sh
#
# Pre-req: `dfx start --background` already running, shenanigans deployed,
# and an identity "caster" that has enough PP to cast Magic Mirror twice
# (2 × 200 PP cost) plus a small buffer.
#
# What it verifies:
#  1. Magic Mirror cast twice in a row leaves chargesRemaining = 2, not 1.
#  2. The UI guardrails copy no longer claims "24-hr protection after
#     negative effects" (false claim — backend implements no such window).
#
# Deferred to Phase 2:
#  - Whale Rebalance backfire loss-bound check. Verifying this from outside
#    the canister currently requires reading PP balances, which live in a
#    separate ppLedger keyed by chip-subaccount. Rather than adding a new
#    canister query just for this smoke test, we wait until Phase 2 lands
#    `ShenaniganOutcomeDetail.ppDeltaCaster`, which lets us read the
#    bounded loss directly off the cast record via `getRecentShenanigans`.
#    The Phase-1 fix itself is verified by code review + dfx build --check;
#    this script's other tests are the live-replica smoke check.

set -euo pipefail
SHENANIGANS_CANISTER="${SHENANIGANS_CANISTER:-shenanigans}"

# --- 1: Magic Mirror stacking ---------------------------------------------
echo "=== Test 1: Magic Mirror stacking ==="
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

# --- 2: UI copy ------------------------------------------------------------
echo "=== Test 2: UI guardrails copy ==="
if grep -nE "24-hr protection|negative effects" frontend/src/components/Shenanigans.tsx; then
    echo "  FAIL: stale guardrail copy still in Shenanigans.tsx"
    exit 1
fi
echo "  PASS"

echo
echo "All Phase-1 verifications passed."
