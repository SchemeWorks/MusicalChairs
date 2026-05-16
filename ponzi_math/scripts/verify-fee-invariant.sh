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
