#!/usr/bin/env bash
# IMPL-49h: actor-level signup-gift interleave regression (Codex R1) on the LOCAL dfx replica.
#
# LOOPBACK ONLY: every dfx call carries `--network local` (127.0.0.1:4943). Needs dfx 0.29.x, python3
# (stdlib), a running shared local network with pp_ledger (5xv2o-...), siws_provider (tcm26-...) and
# shenanigans (j56tm-...), and the SIWS-bound owner Bow1CG... -> m7ydg-... (see native-rewards.md).
#
# Side effects (all local): creates/reinstalls legacy_games_stub, latch_ledger and shenanigans_latch;
# REINSTALLS shenanigans (j56tm-...) once per scenario (wipes whatever state it held); briefly STOPS and
# STARTS pp_ledger (restored by the trap below); never touches siws_provider or native_sol_bridge.
# Usage:  bash shenanigans/test/replica/gift-interleave.sh [A1 A2 A3 H1 H3 H4a H4b]   (default: all)
# Output: shenanigans/test/replica/receipt-gift-interleave-2026-10-15.json
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
LEDGER=5xv2o-iiaaa-aaaac-qeclq-cai
SHEN=j56tm-oaaaa-aaaac-qf34q-cai
if [ -n "${DFX_NETWORK:-}" ] && [ "$DFX_NETWORK" != "local" ]; then
  echo "refusing to run: DFX_NETWORK=$DFX_NETWORK (this script is local-only)" >&2; exit 2
fi
unset DFX_NETWORK
command -v dfx >/dev/null || { echo "dfx not found" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 not found" >&2; exit 2; }
restore() { ( dfx canister start "$LEDGER" --network local >/dev/null 2>&1 || true; dfx canister start "$SHEN" --network local >/dev/null 2>&1 || true ) || true; }
trap restore EXIT
dfx ping local >/dev/null 2>&1 || { echo "no replica on 127.0.0.1:4943" >&2; exit 2; }
export MC_REPLICA_ROOT="$ROOT" MC_REPLICA_HERE="$HERE"
python3 -I "$HERE/gift_interleave.py" "$@"
