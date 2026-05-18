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
