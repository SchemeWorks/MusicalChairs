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
