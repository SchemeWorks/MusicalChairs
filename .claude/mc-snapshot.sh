#!/usr/bin/env bash
# Read-only snapshot of mainnet state for the test-plan diff.
# Usage:
#   .claude/mc-snapshot.sh                    # prints to stdout
#   .claude/mc-snapshot.sh > snap-00-before.txt
#   .claude/mc-snapshot.sh > snap-01-rob-claim.txt
#   diff snap-00-before.txt snap-01-rob-claim.txt
#
# Takes ~30-60s on mainnet. All calls are queries — no state mutation.

set -uo pipefail

BACKEND="5zxxg-tyaaa-aaaac-qeckq-cai"
SHENANIGANS="j56tm-oaaaa-aaaac-qf34q-cai"
PP_LEDGER="5xv2o-iiaaa-aaaac-qeclq-cai"
ICP_LEDGER="ryjl3-tyaaa-aaaaa-aaaba-cai"

ROB="gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe"
STEVE="stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe"
BILL="zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae"

call() { dfx canister --network ic call --query "$@" 2>&1; }

echo "================================================================"
echo "MC snapshot @ $(date '+%Y-%m-%d %H:%M:%S %Z')"
echo "================================================================"
echo

echo "### Platform stats"
call "$BACKEND" getPlatformStats '()'
echo
echo "### Active games"
call "$BACKEND" getAllActiveGames '()'
echo
echo "### Reset history"
call "$BACKEND" getGameResetHistory '()'
echo
echo "### Backer positions (entitlements)"
call "$BACKEND" getDealerPositions '()'
echo
echo "### All backer repayment balances"
call "$BACKEND" getAllDealerRepayments '()'
echo

for entry in "rob:$ROB" "steve:$STEVE" "bill:$BILL"; do
  name="${entry%%:*}"
  p="${entry##*:}"
  echo "----------------------------------------------------------------"
  echo "## $name  ($p)"
  echo "----------------------------------------------------------------"
  echo "ICP wallet:"
  call "$ICP_LEDGER" icrc1_balance_of "(record { owner = principal \"$p\"; subaccount = null })"
  echo "PP ledger balance:"
  call "$PP_LEDGER" icrc1_balance_of "(record { owner = principal \"$p\"; subaccount = null })"
  echo "Repayment claimable:"
  call "$BACKEND" getDealerRepaymentBalanceFor "(principal \"$p\")"
  echo "Cash-outs (shenanigans):"
  call "$SHENANIGANS" getCashOutsFor "(principal \"$p\")"
  echo "Active games for $name:"
  call "$BACKEND" getUserGamesFor "(principal \"$p\")"
  echo
done

echo "================================================================"
echo "End snapshot @ $(date '+%H:%M:%S')"
echo "================================================================"
