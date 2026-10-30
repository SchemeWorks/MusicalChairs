#!/usr/bin/env bash
# DEMO-LOCAL: one-command LOCAL click-through of the native Solana screens.
#
# Runs ON THIS MACHINE ONLY:
#   * solana-test-validator (the app uses RPC 127.0.0.1:8899) loaded with a SYNTHETIC "closing world"
#     exported from LiteSVM by `local_cluster export-closing-fixture`: genesis, the real ORAO provider
#     setup, and one 1-SOL practice position opened TEN DAYS ago over a 0.1-SOL pot. That is the
#     fast-forward: the practice owner's first withdrawal is insolvent today and closes the round.
#   * the local oracle: answers draw requests with a synthetic key (not the ORAO network);
#   * the Vite dev server on http://localhost:5180 in local-cluster mode (burner wallet, no SIWS,
#     no ICP canister).
# No transaction goes to devnet, mainnet or a public RPC, no real wallet is used, and nothing is
# rebuilt: every program file is checked against its recorded hash first. Two limits (see
# docs/solana/local-cluster.md): the pinned validator 3.1.0 has no RPC/faucet bind option, so
# ports 8899/9900 listen on all interfaces (--bind-address keeps gossip/TPU on loopback; the
# chain holds only throwaway practice state); and the existing legacy (Chain Fusion) and
# Shenanigans panels make anonymous read-only queries to the live Internet Computer.
# This is a local demonstration, never test-network evidence.
#
# usage: scripts/demo-local.sh [start]    Ctrl-C (or closing the terminal) stops everything and
#                                          deletes this run's ledger, keys and logs
#        scripts/demo-local.sh owner-key  copy the practice owner's key to the clipboard, for the
#                                          wallet import box (a synthetic local key, nothing else)
#        scripts/demo-local.sh status     read-only Pool / Ops / Ticket summary
#        scripts/demo-local.sh stop       clean up a run that was killed without Ctrl-C
set -u

REPO=$(cd "$(dirname "$0")/.." && pwd)
AGAVE_BIN=${MC_AGAVE_BIN:-$HOME/.cache/mc-solana-tools/agave/solana-release/bin}
LOCAL_CLUSTER=${MC_LOCAL_CLUSTER:-$HOME/.cache/mc-solana-tools/harness-target/debug/local_cluster}
STATE_ROOT=${MC_DEMO_STATE:-$HOME/.cache/mc-demo-local}
MARKER=.mc-demo-local-owned
RPC_URL=http://127.0.0.1:8899
VITE_PORT=5180

# Pinned inputs. A mismatch is reported and the demo refuses to start; nothing is rebuilt here.
VALIDATOR_VERSION='solana-test-validator 3.1.0 (src:bd8c57e7'
LOCAL_CLUSTER_SHA=23f690aef5f7406f8c3e8e72239e3b576b3d076e6d63a4db094ec20c9ca2478a
ORAO_SHA=6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b
PROGRAM_SO=$REPO/solana/programs/musical_chairs/artifacts/musical_chairs_program.so
STANDIN_SO=$REPO/solana/tests/financial-runtime/standin/squads-vault-standin/artifacts/squads_vault_standin.so
ORAO_SO=$REPO/solana/probes/runtime/fixtures/orao-callback-mainnet-finalized.so
BUILD_RECORD=$REPO/docs/solana/evidence/latest-build.json

die() { echo "demo-local: $*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | cut -c1-64; }
port_busy() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }
record() { python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print($1)" "$BUILD_RECORD"; }

owned() { # a run directory this script created (marker present, directly under STATE_ROOT)
  case "$1" in "$STATE_ROOT"/run-*) [ -f "$1/$MARKER" ] ;; *) return 1 ;; esac
}

alive() { # $1 pid file, $2 text the process command line must contain
  local pid
  pid=$(cat "$1" 2>/dev/null) || return 1
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && ps -p "$pid" -o command= 2>/dev/null | grep -qF -- "$2"
}

stop_pid() { # stop only a process this run started (pid file + matching command line)
  alive "$1" "$2" || return 0
  local pid i
  pid=$(cat "$1")
  kill "$pid" 2>/dev/null
  for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.5; done
  kill -9 "$pid" 2>/dev/null
}

cleanup_run() {
  owned "$1" || return 0
  stop_pid "$1/vite.pid" "vite"
  stop_pid "$1/oracle.pid" "local_cluster"
  stop_pid "$1/validator.pid" "solana-test-validator"
  rm -rf "$1"
}

latest_run() { ls -1d "$STATE_ROOT"/run-* 2>/dev/null | tail -1; }

preflight() {
  case "$REPO$STATE_ROOT" in *" "*) die "paths with spaces are not supported ($REPO, $STATE_ROOT)" ;; esac
  [ -x "$AGAVE_BIN/solana-test-validator" ] || die "missing $AGAVE_BIN/solana-test-validator (the pinned Agave 3.1.0 release)"
  "$AGAVE_BIN/solana-test-validator" --version 2>/dev/null | grep -qF "$VALIDATOR_VERSION" \
    || die "validator is not the pinned '$VALIDATOR_VERSION…)'"
  [ -x "$LOCAL_CLUSTER" ] || die "missing $LOCAL_CLUSTER (the harness local_cluster tool)"
  [ "$(sha "$LOCAL_CLUSTER")" = "$LOCAL_CLUSTER_SHA" ] \
    || die "local_cluster differs from the pinned build ${LOCAL_CLUSTER_SHA:0:8}…; not rebuilt automatically (cargo build --locked --bin local_cluster, then review and re-pin)"
  local want
  want=$(record "d['artifact']['sha256']") || die "cannot read $BUILD_RECORD"
  [ -f "$PROGRAM_SO" ] && [ "$(sha "$PROGRAM_SO")" = "$want" ] \
    || die "program build $PROGRAM_SO does not match latest-build.json (${want:0:8}…); not rebuilt automatically"
  want=$(record "d['standin_artifact_sha256']")
  [ -f "$STANDIN_SO" ] && [ "$(sha "$STANDIN_SO")" = "$want" ] || die "Squads stand-in does not match latest-build.json (${want:0:8}…)"
  [ -f "$ORAO_SO" ] && [ "$(sha "$ORAO_SO")" = "$ORAO_SHA" ] || die "ORAO fixture does not match its recorded hash (${ORAO_SHA:0:8}…)"
  [ -x "$REPO/node_modules/.bin/vite" ] || die "frontend dependencies are missing (run 'npm ci' at $REPO)"
  local p
  for p in 8899 8900 9900 "$VITE_PORT"; do
    port_busy "$p" && die "port $p is already in use; this script never stops processes it did not start"
  done
  local free_kb
  free_kb=$(df -k "$HOME" | awk 'NR==2 {print $4}')
  [ "${free_kb:-0}" -ge 3145728 ] || die "less than 3 GiB free on the home volume"
  local old
  for old in "$STATE_ROOT"/run-*; do
    [ -d "$old" ] || continue
    owned "$old" || continue
    if alive "$old/validator.pid" "solana-test-validator"; then
      die "a demo is already running ($old); stop it with Ctrl-C or 'scripts/demo-local.sh stop'"
    fi
    rm -rf "$old" # a leftover from a run that was killed without cleanup
  done
}

start() {
  preflight
  RUN="$STATE_ROOT/run-$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$RUN/fixture" "$RUN/keys" "$RUN/ledger" || die "cannot create $RUN"
  touch "$RUN/$MARKER"
  chmod 700 "$RUN/keys"
  trap 'cleanup_run "$RUN"' EXIT
  trap 'echo; echo "demo-local: stopping (removing $RUN)"; exit 0' INT TERM HUP
  export MC_FINANCIAL_SBF="$PROGRAM_SO" MC_STANDIN_SBF="$STANDIN_SO" MC_ORAO_SBF="$ORAO_SO"

  echo "[1/4] Building the practice game (synthetic, LiteSVM; position backdated ten days)…"
  if ! "$LOCAL_CLUSTER" export-closing-fixture "$RUN/fixture" "$RUN/keys" >"$RUN/fixture.log" 2>&1; then
    tail -20 "$RUN/fixture.log" >&2; die "fixture export failed"
  fi

  echo "[2/4] Starting the local validator (loopback only, capped ledger in $RUN/ledger)…"
  local args
  args=$(sed -e "s#<PROGRAM_SO>#$PROGRAM_SO#" -e "s#<STANDIN_SO>#$STANDIN_SO#" -e "s#<ORAO_SO>#$ORAO_SO#" \
    "$RUN/fixture/validator-args.txt" | tr -d '\\' | tr '\n' ' ')
  # shellcheck disable=SC2086  # the exported argument list is space-separated with no spaces inside paths
  set -- $args
  "$AGAVE_BIN/solana-test-validator" "$@" --ledger "$RUN/ledger" --limit-ledger-size 10000 \
    --bind-address 127.0.0.1 >"$RUN/validator.log" 2>&1 &
  echo $! >"$RUN/validator.pid"
  local i ok=
  for i in $(seq 1 120); do
    if curl -s "$RPC_URL" -H 'content-type: application/json' \
      -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q '"ok"'; then ok=1; break; fi
    alive "$RUN/validator.pid" "solana-test-validator" || { tail -20 "$RUN/validator.log" >&2; die "the validator exited"; }
    sleep 1
  done
  [ -n "$ok" ] || { tail -20 "$RUN/validator.log" >&2; die "the validator did not become healthy in 120 s"; }
  "$LOCAL_CLUSTER" status "$RPC_URL" >"$RUN/status.log" 2>&1 || { cat "$RUN/status.log" >&2; die "the practice game did not load"; }

  echo "[3/4] Starting the local oracle (synthetic answer key, not the ORAO network)…"
  "$LOCAL_CLUSTER" oracle "$RPC_URL" "$RUN/keys" >"$RUN/oracle.log" 2>&1 &
  echo $! >"$RUN/oracle.pid"

  local owner
  owner=$(awk '/^closing fixture/ {for (i = 1; i <= NF; i++) if ($i == "owner") {print $(i + 1); exit}}' "$RUN/fixture/manifest.txt" | tr -d ';')
  cat <<EOF
[4/4] Starting the website…

  Open:  http://localhost:$VITE_PORT        (LOCAL DEMO: synthetic practice game, local oracle)
  Part 1, as a new player: a burner wallet connects and is funded locally; deposit, see the
          position, withdraw, fund the Series A, claim.
  Part 2, the ten-day-old player: run 'scripts/demo-local.sh owner-key' in another terminal,
          paste into the wallet's import box (owner $owner), then withdraw. The round closes:
          draw, promotion, finalize, round 2.
  Stop:  Ctrl-C here (removes this run's ledger, keys and logs).
EOF
  (cd "$REPO" && VITE_SOLANA_RPC_URL="$RPC_URL" VITE_NATIVE_LOCAL_CLUSTER=1 VITE_NATIVE_BRIDGE_CANISTER_ID= \
    DFX_NETWORK=local exec "$REPO/node_modules/.bin/vite" --port "$VITE_PORT" --strictPort) &
  echo $! >"$RUN/vite.pid"
  wait "$(cat "$RUN/vite.pid")"
}

owner_key() {
  local run
  run=$(latest_run)
  [ -n "$run" ] && owned "$run" && alive "$run/validator.pid" "solana-test-validator" \
    || die "no demo is running; start one with 'scripts/demo-local.sh'"
  pbcopy <"$run/keys/owner.json" || die "could not copy to the clipboard"
  echo "Copied the practice owner's key (synthetic, this local demo only) to the clipboard."
  echo "Paste it into the wallet's import box on http://localhost:$VITE_PORT."
}

stop_all() {
  local run n=0
  for run in "$STATE_ROOT"/run-*; do
    [ -d "$run" ] && owned "$run" || continue
    cleanup_run "$run"; n=$((n + 1))
  done
  echo "demo-local: cleaned up $n run(s) under $STATE_ROOT"
}

case "${1:-start}" in
  start) start ;;
  owner-key) owner_key ;;
  status) "$LOCAL_CLUSTER" status "$RPC_URL" ;;
  stop) stop_all ;;
  *) sed -n '2,25p' "$0"; exit 2 ;;
esac
