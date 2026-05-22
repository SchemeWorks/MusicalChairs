#!/usr/bin/env bash
# Sets the icrc1:logo metadata on the pp_ledger canister.
#
# Reads ponzipoints/ponzi_logo.svg, base64-encodes it into a data: URL,
# and runs `dfx canister install pp_ledger --mode upgrade` against the
# network passed as $1 (default: ic).
#
# The icrc1_metadata auto-generated entries (name, symbol, decimals, fee,
# max_memo_length) are unaffected by this upgrade — only user-set metadata
# is replaced.
set -euo pipefail

NETWORK="${1:-ic}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOGO_PATH="$REPO_ROOT/ponzipoints/ponzi_logo.svg"
ARG_FILE="$(mktemp -t pp_ledger_upgrade.XXXXXX.arg)"
trap 'rm -f "$ARG_FILE"' EXIT

if [[ ! -f "$LOGO_PATH" ]]; then
  echo "Logo not found at $LOGO_PATH" >&2
  exit 1
fi

B64=$(base64 -i "$LOGO_PATH" | tr -d '\n')
printf '(variant { Upgrade = opt record { metadata = opt vec { record { "icrc1:logo"; variant { Text = "data:image/svg+xml;base64,%s" } } } } })' "$B64" > "$ARG_FILE"

echo "Upgrading pp_ledger on $NETWORK with logo from $LOGO_PATH ($(wc -c < "$LOGO_PATH") bytes -> $(wc -c < "$ARG_FILE") byte arg)..."
dfx canister install pp_ledger --network "$NETWORK" --mode upgrade --upgrade-unchanged --argument-file "$ARG_FILE" --yes

echo "Verifying icrc1_metadata..."
dfx canister call pp_ledger icrc1_metadata --network "$NETWORK" --query | grep -c "icrc1:logo" > /dev/null && echo "icrc1:logo is set."
