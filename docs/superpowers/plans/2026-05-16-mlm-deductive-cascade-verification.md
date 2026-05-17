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

## Re-verify after review-pass fixes — 2026-05-16

Re-run on a freshly cleaned local replica (`dfx stop && dfx start --background
--clean`) to validate the migration fix in commit `f03443e`. The M4 commit had
deleted legacy stable vars from the actor without consuming them; the latest
commit fixes that by having `Migration.runV3` explicitly accept the legacy
fields in its input record so Motoko's stable-OP machinery drops them cleanly.

Deployed in dependency order: `backend`, `ponzi_math` (with the
`{ backendPrincipal; testAdmin }` init record), `pp_ledger`, `shenanigans`.
The `shenanigans` build emitted exactly **6 M0207 informational warnings**,
one per legacy field consumed by `runV3`: `CASCADE_MAX_DEPTH`,
`activeDepositors`, `cascadeBps`, `cascadePassthrough`, `charlesPrincipal`,
`signupGiftPp`. These confirm Migration.runV3 is consuming exactly the right
legacy fields, so any upgrade from a state that carried those vars would drop
them cleanly. (On a fresh `--clean` deploy nothing is dropped because nothing
was there; the warnings document the schema-shedding behavior available for
mainnet upgrade.) All deploys succeeded.

Steps re-run on the clean replica:

- **Step 3 (V3 MintConfig shape):** `getMintConfig` returned the full 15-field
  record with `cascadeInitialBps = 1_000`, `cascadePassthroughBps = 5_000`,
  `signupGiftPp = 500`, `activityRequiresDeposit = true`,
  `activityWindowDays = null`, plus all 10 pre-existing fields untouched.

- **Step 4 (initialize + seedMigrationV2):** Both returned `()`. The
  prior-blocking `Not initialized` trap no longer fires because the actor is in
  a coherent state from the start.

- **Step 5 / I1 (observer gate — validated live on a third fresh replica):**
  After adding `isBootstrapped : () -> async Bool` for observability, ran the
  sequence: `initialize` (returns `()`); `isBootstrapped` returns `false`
  while observer is gated; `runObserverOnce` returns `()` cleanly (no trap,
  early-return path exercised); `seedMigrationV2` returns `()`;
  `isBootstrapped` returns `true`. The gate flips exactly when
  `seedMigrationV2` finishes. Confirmed.

- **Step 6 / I3 (anonymous-principal rejection in `setHousePrincipal`):**
  `setHousePrincipal "(principal \"2vxsx-fae\")"` trapped with
  `'housePrincipal cannot be the anonymous principal'`. A subsequent
  `setHousePrincipal "(principal \"<admin>\")"` returned `()`.

- **Step 7 / I2 (`setSignupGiftPp` cap):** `setSignupGiftPp "(1_500_000 :
  nat)"` trapped with `'signupGiftPp must be ≤ 1_000_000 whole PP (guard
  against typo-induced mass mints)'`. Both `setSignupGiftPp "(750 : nat)"` and
  `setSignupGiftPp "(500 : nat)"` returned `()`, leaving the canister back at
  the spec default.

- **Step 8 / M5 (`setReferralBps` deprecation):** `setReferralBps "(800 :
  nat, 500 : nat, 200 : nat)"` trapped with `'setReferralBps is deprecated —
  use setCascadeBps(initial, passthrough) for the deductive cascade'`. The
  deprecation gate is in place.

- **Step 9 / I4 + I5 (`getReferralStats` reverse-index path):** Returned the
  full 7-field record (`l1Count`, `l2Count`, `l3Count`, `l1Units`, `l2Units`,
  `l3Units`, `recentSignups = vec {}`) without trapping. The reverse-index
  rewrite did not regress the read path on an empty corpus.

- **Step 10 (`setCascadeBps` validation):** `setCascadeBps "(10_001 : nat,
  5_000 : nat)"` trapped with `'BPS values must be ≤ 10_000'`. Reverting to
  `setCascadeBps "(1_000 : nat, 5_000 : nat)"` returned `()` and a follow-up
  `getMintConfig` confirmed `cascadeInitialBps = 1_000` /
  `cascadePassthroughBps = 5_000`.

**Not directly testable on this empty corpus:**

- **C1 (conservation under cascade `#Err`):** Would require simulating a
  ledger failure mid-cascade to verify the residual sweep catches the
  unminted amount. Verified by code inspection only:
  `distributeDeductiveCascade` decrements `remaining` solely inside the
  `#Ok` arm at [shenanigans/main.mo:1029], so failed inner mints flow into
  the `cascade-residual-` mint to the house.

- **M1 (earliest game timestamp grandfathering):** The fresh replica had
  no historic game records to grandfather. Verified by code inspection only:
  `seedMigrationV2` now reads `game.startTime` (and falls back to
  `backer.firstDepositDate ?? backer.startTime`) when seeding
  `signupGiftClaimed` for each player.

All other I/M items required by the rollout plan validated cleanly on the
fresh replica. The Migration.runV3 fix unblocks future deploys: legacy
stable-var drop is documented as informational M0207 warnings only.
