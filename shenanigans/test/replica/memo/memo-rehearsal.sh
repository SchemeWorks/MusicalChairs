#!/usr/bin/env bash
# PP-ledger memo fix: local-replica rehearsal (see memo_rehearsal.py for the full description).
#
# LOOPBACK ONLY: every dfx call carries `--network local` (127.0.0.1:4943). Refuses any other network.
# Needs: dfx 0.29.x, python3 (stdlib only), a running local replica with the real pp_ledger at
# 5xv2o-iiaaa-aaaac-qeclq-cai (icrc1:max_memo_length = 32, minting account = j56tm-...), and the pre-fix wasm
# at artifacts/shenanigans-prefix-main-ee9a402.wasm (built from the unedited main@ee9a402 source;
# override with MC_MEMO_PREFIX_WASM).
#
# Side effects (all local): deploys/upgrades the memo_games_stub canister; REINSTALLS shenanigans at
# j56tm-... (wipes its state; the native-branch test state there is expendable), then upgrades it to the fixed
# build; adds blocks and chip balances to the shared local pp_ledger (never reinstalled or stopped here);
# writes .dfx/local/canister_ids.json entries in the repo root and artifacts/shenanigans-fixed.wasm.
# Idempotent: re-runnable at any time (fresh per-run principals, shenanigans reinstalled each run).
#
# Usage:   bash shenanigans/test/replica/memo/memo-rehearsal.sh
# Output:  shenanigans/test/replica/memo/receipt-2026-10-23.json   (override with MC_MEMO_RECEIPT)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -n "${DFX_NETWORK:-}" ] && [ "$DFX_NETWORK" != "local" ]; then
  echo "refusing to run: DFX_NETWORK=$DFX_NETWORK (this script is local-only)" >&2; exit 2
fi
unset DFX_NETWORK
command -v dfx >/dev/null || { echo "dfx not found" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 not found" >&2; exit 2; }
LEDGER=5xv2o-iiaaa-aaaac-qeclq-cai
SHEN=j56tm-oaaaa-aaaac-qf34q-cai
# A trap restores anything this script (or an interrupted `install --mode upgrade`) may have left stopped.
restore() {
  for c in "$LEDGER" "$SHEN"; do dfx canister start "$c" --network local >/dev/null 2>&1 || true; done
  ( cd "$HERE/stub" && dfx canister start memo_games_stub --network local >/dev/null 2>&1 ) || true
}
trap restore EXIT
dfx ping local >/dev/null 2>&1 || { echo "no replica on 127.0.0.1:4943 (run: dfx start --background)" >&2; exit 2; }
export MC_MEMO_HERE="$HERE"
python3 -I "$HERE/memo_rehearsal.py" "$@"
