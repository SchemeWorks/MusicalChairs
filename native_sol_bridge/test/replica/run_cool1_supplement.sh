#!/usr/bin/env bash
# IMPL-49b: focused COOL-1 supplement. Runs ONLY the corrected scenario against the replica state the
# v2 run left behind: nothing is reinstalled, no leg is executed, nothing is minted. Local network only.
# Needs the throwaway mc-nonadmin identity that run.sh created.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
[ -z "${DFX_NETWORK:-}" ] || [ "$DFX_NETWORK" = "local" ] || { echo "refusing: DFX_NETWORK=$DFX_NETWORK" >&2; exit 2; }
unset DFX_NETWORK
export MC_REPLICA_ROOT="$ROOT" MC_REPLICA_HERE="$HERE"
export MC_BRIDGE="$( cd "$ROOT" && dfx canister id native_sol_bridge --network local )"
export MC_ME="$(dfx identity get-principal)"
export MC_LEDGER=5xv2o-iiaaa-aaaac-qeclq-cai MC_SIWS=tcm26-yqaaa-aaaac-qg2lq-cai MC_SHEN=j56tm-oaaaa-aaaac-qf34q-cai MC_STUB=tghme-zyaaa-aaaar-qarca-cai
export MC_BOUND_PUBKEY=Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew MC_BOUND_PRINCIPAL=m7ydg-dfxxf-u5wan-ft6m4-xmzeg-wj6mw-npaab-cn6oc-5tgu4-rm2s7-yqe
export MC_NONADMIN_IDENTITY=mc-nonadmin
export MC_NONADMIN_PRINCIPAL="$(dfx --identity mc-nonadmin identity get-principal)"
exec python3 -I "$HERE/drive.py" --cool1-supplement
