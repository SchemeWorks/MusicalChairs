# MLM Deductive Cascade — Local replica smoke test

Local-replica verification run on 2026-05-16 after Tasks 1–14 of the deductive
cascade rollout. Replica was started clean via `dfx start --background --clean`,
all canisters deployed via `dfx deploy --network local` (with `ponzi_math`
receiving its `{ backendPrincipal; testAdmin }` init record), then
`shenanigans` was bootstrapped with `initialize(<local ponzi_math id>)` before
the verification calls below. All `dfx canister` calls used `--network local`.
The mainnet canister `j56tm-oaaaa-aaaac-qf34q-cai` was not touched.

## Step 3 — `getMintConfig` after a fresh deploy

Ran `dfx canister --network local call shenanigans getMintConfig`. The candid
record came back with all five new fields populated with the migration
defaults, plus the existing fields untouched:

```
(
  record {
    compounding15DayPpPerIcp = 2_000 : nat;
    minDepositPp = 5_000 : nat;
    cascadeInitialBps = 1_000 : nat;
    compounding30DayPpPerIcp = 3_000 : nat;
    referralL1Bps = 800 : nat;
    referralL2Bps = 500 : nat;
    referralL3Bps = 200 : nat;
    observerIntervalSeconds = 10 : nat;
    backerPpPerIcp = 4_000 : nat;
    cashOutDelaySeconds = 604_800 : nat;
    activityWindowDays = null;
    activityRequiresDeposit = true;
    signupGiftPp = 500 : nat;
    simple21DayPpPerIcp = 1_000 : nat;
    cascadePassthroughBps = 5_000 : nat;
  },
)
```

That confirms `cascadeInitialBps = 1_000`, `cascadePassthroughBps = 5_000`,
`signupGiftPp = 500`, `activityRequiresDeposit = true`, and
`activityWindowDays = null` — exactly the spec defaults. The migration
applied cleanly.

## Step 4 — `seedMigrationV2` is a no-op on a fresh replica

The first call to `seedMigrationV2` trapped with `Not initialized`, which is
expected: the new admin-gated path requires `initialize(ponziMathCanisterId)`
first. After `dfx canister --network local call shenanigans initialize
"(principal \"<local ponzi_math id>\")"` returned `()`, the second call to
`seedMigrationV2` also returned `()`. With no historical `MintEvent` /
`WithdrawalEvent` rows on the replica, the seeding loops are a no-op; the call
still wires `housePrincipal := ?caller` and is idempotent on a re-run.

## Step 6 — `getReferralStats` exposes `recentSignups`

Ran `dfx canister --network local call shenanigans getReferralStats "(principal
\"<admin>\")"`. Response:

```
(
  record {
    l1Count = 0 : nat;
    l3Units = 0 : nat;
    recentSignups = vec {};
    l1Units = 0 : nat;
    l2Count = 0 : nat;
    l2Units = 0 : nat;
    l3Count = 0 : nat;
  },
)
```

The new `recentSignups` field is present alongside the existing L1/L2/L3 unit/
count fields. It is an empty vector on this fresh replica because no MLM
sign-ups have been recorded yet — the shape is what the frontend will rely on.

## Step 7 — Cascade tunables round-trip through admin setters

Ran `setCascadeBps "(2_000 : nat, 4_000 : nat)"`, then re-read `getMintConfig`;
the response showed `cascadeInitialBps = 2_000` and `cascadePassthroughBps =
4_000` with every other field unchanged from Step 3. Then
`setCascadeBps "(1_000 : nat, 5_000 : nat)"` reverted to the spec defaults, and
a follow-up `getMintConfig` confirmed `cascadeInitialBps = 1_000` /
`cascadePassthroughBps = 5_000` are back. The admin path is mutable and the
revert leaves no drift.

## Supplementary — validation traps and `setActivityWindowDays`

Two additional negative-path checks not required by the report format but worth
recording. `setCascadeBps "(10_001 : nat, 5_000 : nat)"` trapped with
`BPS values must be ≤ 10_000`, confirming the upper-bound guard. For
`setActivityWindowDays`, `null` and `opt 30` both returned `()`, while
`opt 0` trapped with `activityWindowDays must be in [1, 3650] or null` — the
boundary check holds. Final state was reverted to `null` so the canister is
left in the spec-default configuration.
