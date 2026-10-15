#!/usr/bin/env bash
# IMPL-47d: bridge -> Shenanigans -> PP ledger, end to end on the LOCAL dfx replica, with a SOL RPC stub.
#
# LOOPBACK ONLY. Every dfx call below carries `--network local` (127.0.0.1:4943); there is no
# `--network ic` anywhere, no external URL, and the stub's "RPC providers" are *.invalid names that
# only the stub canister interprets. Idempotent: it (re)installs the three canisters it owns
# (shenanigans, native_sol_bridge, sol_rpc_stub) with `--mode reinstall`, reconfigures them, and runs
# the whole scenario again. It never touches pp_ledger / siws_provider except for ordinary calls
# (and a few-second stop/start of pp_ledger for the SubmittedUnknown boundary, restored by a trap).
#
# Usage:   bash native_sol_bridge/test/replica/run.sh
# Needs:   dfx 0.29.x with a running shared local network that has pp_ledger (5xv2o-...) and
#          siws_provider (tcm26-...) installed, python3 (stdlib only).
# Output:  native_sol_bridge/test/replica/receipt-2026-10-14.json (+ a progress log on stdout)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
LEDGER=5xv2o-iiaaa-aaaac-qeclq-cai
SIWS=tcm26-yqaaa-aaaac-qg2lq-cai
SHEN=j56tm-oaaaa-aaaac-qf34q-cai
STUB=tghme-zyaaa-aaaar-qarca-cai
BOUND_PUBKEY=Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew
BOUND_PRINCIPAL=m7ydg-dfxxf-u5wan-ft6m4-xmzeg-wj6mw-npaab-cn6oc-5tgu4-rm2s7-yqe

if [ -n "${DFX_NETWORK:-}" ] && [ "$DFX_NETWORK" != "local" ]; then
  echo "refusing to run: DFX_NETWORK=$DFX_NETWORK (this script is local-only)" >&2; exit 2
fi
unset DFX_NETWORK
command -v dfx >/dev/null || { echo "dfx not found" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 not found" >&2; exit 2; }

say() { printf '\n==== %s\n' "$*"; }

# Always leave the shared ledger and shenanigans running, whatever happens below.
restore() {
  ( dfx canister start "$LEDGER" --network local >/dev/null 2>&1 || true
    dfx canister start "$SHEN" --network local >/dev/null 2>&1 || true ) || true
}
trap restore EXIT

say "preflight (read-only)"
dfx ping local >/dev/null 2>&1 || dfx ping http://127.0.0.1:4943 >/dev/null || { echo "no replica on 127.0.0.1:4943" >&2; exit 2; }
MINTER_OWNER="$(dfx canister call "$LEDGER" icrc1_minting_account '()' --network local --output json | python3 -c 'import json,sys; v=json.load(sys.stdin); print(v[0]["owner"])')"
echo "pp_ledger minting account owner: $MINTER_OWNER"
if [ "$MINTER_OWNER" != "$SHEN" ]; then
  echo "STOP: pp_ledger's minting account is $MINTER_OWNER, not $SHEN. Reporting instead of deploying." >&2; exit 3
fi
dfx canister call "$SIWS" get_principal "(\"$BOUND_PUBKEY\")" --network local --output json | python3 -c '
import json,sys
v=json.load(sys.stdin)
assert "Ok" in v, v
print("siws_provider.get_principal(Bow1...) = Ok(%d bytes)" % len(v["Ok"]))'

# --- create (if needed) -------------------------------------------------------------------
ensure_created() { # dir canister [extra create args...]
  local dir="$1" name="$2"; shift 2
  if ( cd "$dir" && dfx canister id "$name" --network local >/dev/null 2>&1 ); then
    echo "$name: already created ($( cd "$dir" && dfx canister id "$name" --network local ))"
  else
    ( cd "$dir" && dfx canister create "$name" "$@" --network local )
  fi
}

say "create canisters (pinned ids where the code needs them)"
ensure_created "$ROOT" shenanigans --specified-id "$SHEN"
ensure_created "$ROOT" native_sol_bridge
ensure_created "$HERE" sol_rpc_stub --specified-id "$STUB"

say "build with dfx's bundled moc"
( cd "$ROOT" && dfx build shenanigans --network local 2>&1 | grep -v "^WARN\|warning \[M0155\]\|^  Nat$\|^$" || true )
( cd "$ROOT" && dfx build native_sol_bridge --network local 2>&1 | tail -3 )
( cd "$HERE" && dfx build sol_rpc_stub --network local 2>&1 | tail -3 )

say "install (reinstall = clean state; pp_ledger / siws_provider are untouched)"
( cd "$ROOT" && dfx canister install shenanigans --mode reinstall --yes --network local )
( cd "$ROOT" && dfx canister install native_sol_bridge --mode reinstall --yes --network local )
( cd "$HERE" && dfx canister install sol_rpc_stub --mode reinstall --yes --network local )

BRIDGE="$( cd "$ROOT" && dfx canister id native_sol_bridge --network local )"
ME="$(dfx identity get-principal)"
echo "bridge=$BRIDGE shenanigans=$SHEN stub=$STUB ledger=$LEDGER siws=$SIWS operator=$ME"

say "configure"
dfx canister call "$STUB" loadBuiltinFixtures '()' --network local
dfx canister call "$STUB" setMode '(variant { Agree })' --network local
dfx canister call "$STUB" clearCallLog '()' --network local
dfx canister call "$BRIDGE" setConfig "(record {
  program_id = \"7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7\";
  cluster_domain = blob \"musical-chairs-localnet-dev!!!!!\";
  event_authority = \"BuzaujH9dKaTPZGcLmRe3SA1DNvHJGN9GAGrGVAzwZd6\";
  supported_schema_versions = vec { 1 : nat };
  rpc_a = variant { Custom = vec { variant { Custom = record { url = \"http://stub-a.invalid\"; headers = null } } } };
  rpc_b = variant { Custom = vec { variant { Custom = record { url = \"http://stub-b.invalid\"; headers = null } } } };
  shenanigans = principal \"$SHEN\" })" --network local
dfx canister call "$BRIDGE" setRelayers "(vec { principal \"$ME\" })" --network local
# Shenanigans: first caller becomes admin (house falls back to the admin); stop the legacy observer
# (it would poll a ponzi_math canister that does not exist locally).
dfx canister call "$SHEN" initialize "(principal \"$ME\")" --network local
dfx canister call "$SHEN" stopObserver '()' --network local
dfx canister call "$SHEN" setNativeBridgeAllowlist "(vec { principal \"$BRIDGE\" })" --network local
dfx canister call "$SHEN" appendNativePolicyFromMintConfig '(86_400_000_000_000 : nat)' --network local

say "drive the scenarios"
export MC_REPLICA_ROOT="$ROOT" MC_REPLICA_HERE="$HERE" MC_BRIDGE="$BRIDGE" MC_ME="$ME"
export MC_LEDGER="$LEDGER" MC_SIWS="$SIWS" MC_SHEN="$SHEN" MC_STUB="$STUB"
export MC_BOUND_PUBKEY="$BOUND_PUBKEY" MC_BOUND_PRINCIPAL="$BOUND_PRINCIPAL"
python3 -I "$HERE/drive.py" "$@"
