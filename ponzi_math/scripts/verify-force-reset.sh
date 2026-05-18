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
