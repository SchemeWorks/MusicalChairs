#!/usr/bin/env bash
set -euo pipefail

NETWORK="${DFX_NETWORK:-ic}"

if [[ "$NETWORK" == "ic" && "${CONFIRM_MAINNET:-}" != "1" ]]; then
  echo "Refusing to update mainnet controllers without CONFIRM_MAINNET=1" >&2
  exit 1
fi

OBSERVATORY_ID="$(
  node -e '
const fs = require("fs");
const ids = JSON.parse(fs.readFileSync("canister_ids.json", "utf8"));
const network = process.env.DFX_NETWORK || "ic";
const id = ids.musical_chairs_observatory && (ids.musical_chairs_observatory[network] || ids.musical_chairs_observatory.ic);
if (!id) process.exit(2);
process.stdout.write(id);
'
)" || {
  echo "Missing musical_chairs_observatory.${NETWORK} in canister_ids.json" >&2
  exit 1
}

targets=(pp_ledger siws_provider frontend pp_assets)

echo "Adding ${OBSERVATORY_ID} as an additional controller on ${NETWORK}."
echo "Targets: ${targets[*]}"

for target in "${targets[@]}"; do
  echo
  echo "== ${target}"
  dfx canister --network "$NETWORK" update-settings "$target" --add-controller "$OBSERVATORY_ID"
  dfx canister --network "$NETWORK" status "$target"
done
