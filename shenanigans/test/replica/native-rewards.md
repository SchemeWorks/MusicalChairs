# Native rewards: local-replica plan (`upgradeAtEveryDurableBoundaryPreservesState` and ledger integration)

Status: PLAN ONLY. Nothing below has been run; every "expected" line is a prediction from the
pure-module tests and the code, not a tested fact. The coordinator runs it on a loopback dfx
replica (never mainnet). No deploy, no network beyond the dfx wasm downloads already named in
`dfx.json`.

What the pure tests (`shenanigans/test/native-rewards-*.test.mo`) already prove, so this plan
does not repeat it: leg arithmetic, policy pinning, write-ahead `SubmittedUnknown`, identical-field
retry, window quarantine, receipt-field exactness, minting-account precondition, forged
kind/archive/index rejection. What only a replica can prove: the actor glue (single await between
two pure steps), real `icrc1_transfer`/`Duplicate` behaviour of the pinned ledger wasm, the Candid
decode of `get_transactions`/`archives`/`icrc1_minting_account`, the SIWS lookup, and that every
durable state survives `--mode upgrade`.

## 0. Static gates (already green at hand-off; re-run first)

```bash
cd <worktree>
source <scratchpad>/moc-rig/env.sh            # MOC, MOC_SOURCES (moc 1.5.1, base 0.16.0)
$MOC --check $MOC_SOURCES shenanigans/main.mo                       # 0 errors (2 pre-existing M0155 warnings)
for t in observe-bind execute reconcile; do $MOC -r $MOC_SOURCES shenanigans/test/native-rewards-$t.test.mo | tail -1; done
bash shenanigans/test/legacy-untouched.test.sh                      # legacy mint code byte-identical, native path never reaches it
# upgrade additivity: new stable vars only
$MOC -c --stable-types -o /tmp/nr-new.wasm $MOC_SOURCES shenanigans/main.mo
$MOC --stable-compatible <pre-M12>.most /tmp/nr-new.most            # exit 0
```

## 1. Setup (loopback replica)

```bash
dfx start --clean --background
# pinned ids, so the ledger's minting account IS the shenanigans canister, exactly as on mainnet
dfx canister create shenanigans   --specified-id j56tm-oaaaa-aaaac-qf34q-cai
dfx canister create pp_ledger     --specified-id 5xv2o-iiaaa-aaaac-qeclq-cai
dfx canister create siws_provider --specified-id tcm26-yqaaa-aaaac-qg2lq-cai
dfx deploy pp_ledger          # init_arg in dfx.json: minting_account = j56tm... , transfer_fee = 0, 8 decimals
dfx deploy siws_provider      # init_arg in dfx.json
dfx deploy shenanigans
ADMIN=$(dfx identity get-principal)
dfx canister call shenanigans initialize "(principal \"$ADMIN\")"     # first caller becomes admin; house falls back to admin
dfx canister call shenanigans setNativeBridgeAllowlist "(vec { principal \"$ADMIN\" })"   # the test identity plays the bridge
dfx canister call shenanigans appendNativePolicyFromMintConfig '(86_400_000_000_000 : nat)'  # v1: 24h window
dfx canister call shenanigans appendNativePolicyFromMintConfig '(660_000_000_000 : nat)'     # v2: 660 s window (window - 5 min margin = 6 min before a leg expires)
dfx canister call shenanigans getNativePolicyTable
```

Expected: two `PolicyVersion` rows, `version = 1` and `2`, rates 6000/12000/18000/120000, `signupGiftPp = 500`,
`cascadeInitialBps = 1000`, `cascadePassthroughBps = 5000`, `applyEffects = true`.
(If `--specified-id j56tm...` is refused locally, create shenanigans first with any id and re-deploy
`pp_ledger` with that id as `minting_account.owner`.)

Bound SIWS identity (IMPL-47d harness, `frontend/src/solana/native/localSiwsLogin.test.ts`):

```bash
MC_LOCAL_IC=http://127.0.0.1:4943 MC_LOCAL_OWNER_KEYPAIR=<keys>/owner.json \
  npx vitest run frontend/src/solana/native/localSiwsLogin.test.ts
```

Known pair from the first run: pubkey `Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew` resolves to principal
`m7ydg-dfxxf-u5wan-ft6m4-xmzeg-wj6mw-npaab-cn6oc-5tgu4-rm2s7-yqe`. An unbound pubkey for the negative path:
`4Nd1mBQtrMJVYVfKGEfnrXz1pZzU3x9Jhc1sYQhQ6Wq7`.

Helpers (bash):

```bash
BOUND=Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew
UNBOUND=4Nd1mBQtrMJVYVfKGEfnrXz1pZzU3x9Jhc1sYQhQ6Wq7
eid()  { echo "record { cluster_domain = blob \"localnet\"; program_id = \"MCprogLocal1111111111111111111111111111111\"; signature = \"$1\"; instruction_index = 0 : nat; inner_instruction_index = 0 : nat; event_schema_version = 1 : nat }"; }
observe() { # $1 sig  $2 pubkey  $3 lamports  $4 plan
  dfx canister call shenanigans observeNativeEvent "(record { event_id = $(eid $1); source_round_id = 1 : nat64; source_close_id = null; kind = variant { PositionOpened }; beneficiary_pubkey = \"$2\"; basis_lamports = $3 : nat64; plan = opt variant { $4 } })"; }
obl()  { dfx canister call shenanigans getNativeObligation "($(eid $1))"; }
upgrade() { dfx canister install shenanigans --mode upgrade --wasm-memory-persistence keep; }
```

## 2. Boundary scenarios (each ends with an upgrade and a state diff)

For every boundary: `before=$(obl SIG)`; `upgrade`; `after=$(obl SIG)`; assert `before == after` (whole
candid text), then continue the flow to prove it still works after the upgrade.

| # | Boundary | Steps | Expected before-upgrade state | After-upgrade continuation |
|---|---|---|---|---|
| B1 | Observed (unbound) | `observe sigU $UNBOUND 1000000000 Simple`; `bindAndReserve(sigU)` | observe returns `Observed("ev1\|6c6f63616c6e6574\|MCprogLocal...\|sigU\|0\|0\|1")`; bind returns `Unbound`; obligation `status = Observed`, `beneficiary = null`, `legs = vec {}`, `policy_version = 1` | append policy v3 (any rates); `upgrade`; bind still `Unbound`; obligation still `policy_version = 1` |
| B2 | Reserved, all legs Pending (bound) | `observe sigB $BOUND 1000000000 Simple`; append a v3 policy with different rates; `bindAndReserve(sigB)` | bind returns `Reserved`; obligation `Reserved`, `policy_version = 1`; 4 legs (no uplines, house inactive): player gift `45_000_000_000`, house residual `5_000_000_000`, player `540_000_000_000`, house residual `60_000_000_000`; every leg `Pending`, 32-byte memo, one shared `created_at_time` | `upgrade`; a SECOND `bindAndReserve(sigB)` returns `AlreadyReserved`; legs byte-identical; `observe sigB2 $BOUND ...` then bind yields NO gift leg (signup marker survived) |
| B3 | SubmittedUnknown | `dfx canister stop pp_ledger`; `executeNativeRewardLeg(sigB, 0)` | call returns `SubmittedUnknown` (the inter-canister call to a stopped canister rejects -> `CallFailed`); leg 0 `attempts = 1` | `upgrade`; `dfx canister start pp_ledger`; `retryNativeRewardLeg(sigB, 0)` returns `Committed{block = N}`; frozen `memo`/`created_at_time` unchanged; `attempts = 2` |
| B4 | Committed + sibling progress | execute legs 1..3 | all legs `Committed`; obligation `Completed`; ledger balance of the player chip subaccount (read `recipient` from `obl`) `= 585_000_000_000` (5,850 PP), admin chip `= 65_000_000_000` | `upgrade`; `executeNativeRewardLeg` on every leg returns the stored `Committed` and the ledger log length does not change (no re-mint) |
| B5 | Quarantined (window) | make the 660 s policy the LATEST version (append another `appendNativePolicyFromMintConfig(660_000_000_000)` if later versions exist), then `observe sigW $BOUND 1000000000 Simple`; `bindAndReserve(sigW)`; wait 6 min 5 s (window 660 s minus the 5 min safety margin); `executeNativeRewardLeg(sigW, 0)` | returns `Quarantined{recoverable = true; reason contains "reconcile"}` and NO `icrc1_transfer` is made (ledger log length unchanged); the other legs are still `Pending` | `upgrade`; leg 0 still `Quarantined`; `retryNativeRewardLeg(sigW, 0)` and `executeNativeRewardLeg(sigW, 0)` both return the same status; frozen `created_at_time`/`memo` unchanged |
| B6 | PartiallyQuarantined | repeat B5 for `sigX`, but execute leg 0 inside the first 5 minutes (it commits); after 6 min 5 s execute legs 1..3 (each quarantines) | leg 0 `Committed`, legs 1..3 `Quarantined{recoverable = true}`; obligation `PartiallyQuarantined` | `upgrade`; unchanged; only `reconcileLegFromLedger` can move a quarantined leg, and only given a real exact mint (none exists for legs 1..3, so it traps) |
| B7 | Lost outcome (snapshot rollback) -> reconcile | bind `sigR` (Reserved, all Pending); `dfx canister stop shenanigans`; `dfx canister snapshot create shenanigans` (note SNAP); `dfx canister start shenanigans`; `executeNativeRewardLeg(sigR, 0)` -> `Committed{block = N}`; `dfx canister stop shenanigans`; `dfx canister snapshot load shenanigans SNAP`; `dfx canister start shenanigans` | canister state reverted: leg 0 `Pending` again while the ledger still holds the mint at block N | `reconcileLegFromLedger(sigR, 0, N)` returns `Committed{block = N}` (exact mint receipt); separately, on a second rolled-back run, `executeNativeRewardLeg(sigR, 0)` returns `Committed{block = N}` via the ledger's `Duplicate{duplicate_of = N}` (identical frozen fields, no second mint, ledger total supply unchanged) |

Negative reconcile checks on the same replica (each must trap with a reason and leave state unchanged):

* `reconcileLegFromLedger(sigR, 1, N)` where block N is leg 0's mint -> `reconcile rejected: mint ... differs from the frozen leg`.
* `reconcileLegFromLedger(sigR, 0, <log_length + 1000>)` -> `no transaction at block ...`.
* A block that is a real `transfer` (do any `icrc1_transfer` from the funded initial-balances account, then reconcile against that block) -> `wrong transaction kind "transfer"`.
* `reconcileLegFromLedger` on an already `Committed` leg returns its stored status with NO ledger call (cycle-drain guard); the positive B7 run is what exercises the real `get_transactions` + `archives` + `icrc1_minting_account` Candid decode against the pinned ledger wasm.
* Anonymous callers are refused by `bindAndReserve`, `retryNativeRewardLeg` and `reconcileLegFromLedger` (`Authentication required`).

Archive path (optional, long): drive the ledger past `trigger_threshold = 2000` transactions so blocks archive
(`archive_options` in `dfx.json`), then reconcile an archived block; it must go through an archive that `archives()` lists
(`selectArchive`), never through any other canister.

## 3. Concurrency checks (cannot be modelled without real await interleaving)

* Two simultaneous first events for one principal: `observe sigC1 $BOUND ...`, `observe sigC2 $BOUND ...`, then
  `bindAndReserve(sigC1) & bindAndReserve(sigC2) & wait`. Expected: both `Reserved`, and exactly ONE `signupGift` leg across
  the two obligations (marker claimed in the await-free block after the SIWS reply).
* Second binder: two concurrent `bindAndReserve(sigD)` -> one `Reserved`, one `AlreadyReserved`; legs identical to a single bind.
* Concurrent execute of one leg: two concurrent `executeNativeRewardLeg(sigD, 0)` -> both return `Committed` with the same
  block (the loser sees the ledger's `Duplicate`); `getNativeObligation` shows `attempts = 2`, one ledger mint.

## 4. Wrong-deployment guard (minting-account verification, `Retryable` boundary)

Deploy `shenanigans` at a NON-pinned id (plain `dfx canister create shenanigans` with no `--specified-id`) next to a ledger whose
minting account owner is `j56tm-...`.

* `executeNativeRewardLeg` -> the ledger sees a normal `transfer` from this canister (no balance) -> `InsufficientFunds`
  -> leg `Retryable{reason contains "InsufficientFunds"; attempts = 1}` (a definite rejection; nothing minted). `upgrade`;
  status unchanged; fixing the deployment (re-init ledger with this canister as minter) and `retryNativeRewardLeg` re-presents
  the identical fields and commits.
* `reconcileLegFromLedger` on any block -> `reconcile rejected: minting account mismatch ...` before any receipt is read as valid.

## 5. Teardown

`dfx stop`; remove `.dfx/` local state and any snapshot ids. No mainnet command appears anywhere in this plan.
