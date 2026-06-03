#!/usr/bin/env bash
#
# Local-replica test for the CRITICAL-1 fix (credit/withdraw lock-disjointness)
# in ponzi_math_sol. Exercises the `globalCriticalLock` guard added to
# creditDeposit's two unlocked platformStats writes.
#
# ⚠️  UNCOMPILED-flagged: depends on two TEST_ADMIN + non-bootstrapped test shims
#     (adminTestSetGlobalLock, adminTestGuardedPotCredit) in ponzi_math_sol/main.mo.
#     Run `dfx build ponzi_math_sol` FIRST. If it doesn't compile, fix the canister
#     before trusting this test. Nothing here touches mainnet (shims are inert once
#     bootstrapped; this script reinstalls a throwaway LOCAL canister).
#
# What it proves (deterministically, no sol-rpc needed):
#   1) Lock HELD (simulating a withdraw mid-payout) → a deposit credit returns
#      "Critical section busy" and does NOT change potBalance  → no lost update.
#   2) Lock FREE → a deposit credit applies normally → the guard didn't break
#      the happy path.
#
# It does NOT reproduce the full async interleave (see the Level-2 note at the end).
#
# Usage:
#   dfx start --background          # if not already running
#   ./ponzi_math_sol/scripts/test-critical1-guard.sh
#
set -euo pipefail

CANISTER=ponzi_math_sol
INBOUND="50_000_000 : nat64"   # 0.05 SOL
PRINCIPAL="$(dfx identity get-principal)"

note() { printf '\n\033[1;36m%s\033[0m\n' "$*"; }
pass() { printf '\033[1;32mPASS:\033[0m %s\n' "$*"; }
fail() {
  printf '\033[1;31mFAIL:\033[0m %s\n' "$*" >&2
  # Best-effort: never leave the lock held on a failed run.
  dfx canister call "$CANISTER" adminTestSetGlobalLock '(false)' >/dev/null 2>&1 || true
  exit 1
}

# Extract the potBalance field from getPlatformStats as a raw string (compared by
# equality, so no float math). Same call both times → identical formatting.
pot() { dfx canister call "$CANISTER" getPlatformStats --query | grep -oE 'potBalance = [0-9.e_+-]+' | head -1; }

note "Reinstalling $CANISTER locally (testAdmin = your identity = $PRINCIPAL)…"
dfx deploy "$CANISTER" --mode reinstall --yes --argument "(record {
  backendPrincipal = principal \"$PRINCIPAL\";
  testAdmin = principal \"$PRINCIPAL\";
  solTreasuryAddress = \"11111111111111111111111111111111\";
  solRpcProvider = variant { Devnet };
  keyId = record { name = \"dfx_test_key\"; curve = variant { ed25519 } };
})"

note "1/4  Baseline credit with lock FREE — expect Ok"
OUT="$(dfx canister call "$CANISTER" adminTestGuardedPotCredit "($INBOUND)")"; echo "$OUT"
[[ "$OUT" == *"Ok"* ]] || fail "baseline credit should succeed"
pass "baseline credit applied"
B0="$(pot)"; note "    potBalance = $B0"

note "2/4  Hold the global lock (simulate a withdraw mid critical-section)…"
dfx canister call "$CANISTER" adminTestSetGlobalLock '(true)' >/dev/null

note "3/4  Credit while lock HELD — expect Err 'Critical section busy'"
OUT="$(dfx canister call "$CANISTER" adminTestGuardedPotCredit "($INBOUND)")"; echo "$OUT"
[[ "$OUT" == *"Critical section busy"* ]] || fail "guard did NOT fire — credit was not blocked while a critical section was active"
pass "guard fired: credit blocked while lock held"
dfx canister call "$CANISTER" adminTestSetGlobalLock '(false)' >/dev/null
B1="$(pot)"; note "    potBalance = $B1"
[[ "$B0" == "$B1" ]] || fail "potBalance changed while lock held ($B0 -> $B1) — LOST UPDATE NOT FIXED"
pass "potBalance unchanged by the blocked credit (no lost update)"

note "4/4  Credit again with lock FREE — expect Ok and potBalance grows"
OUT="$(dfx canister call "$CANISTER" adminTestGuardedPotCredit "($INBOUND)")"; echo "$OUT"
[[ "$OUT" == *"Ok"* ]] || fail "post-release credit should succeed"
B2="$(pot)"; note "    potBalance = $B2"
[[ "$B0" != "$B2" ]] || fail "potBalance did not grow on a free credit — the guard broke the happy path"
pass "happy-path credit still works"

printf '\n\033[1;32m✅ ALL CHECKS PASSED — CRITICAL-1 guard verified on the local replica.\033[0m\n'

cat <<'EOF'

──────────────────────────────────────────────────────────────────────────────
Level-2 (full async-race) — NOT run by this script:
To reproduce the ACTUAL interleave (a credit landing during a withdraw's
`await sendSolPayout`), point sendSolPayout at a MOCK sol-rpc canister that
(a) delays its response so the withdraw is suspended at the await, and
(b) returns an Err to force the snapshot-restore rollback. During the suspension,
fire a credit; confirm the PRE-fix code loses it (potBalance reverts) while the
guarded code blocks it. The deterministic test above is sufficient pre-deploy
evidence that the guard fires and doesn't regress crediting.
──────────────────────────────────────────────────────────────────────────────
EOF
