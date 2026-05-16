# ICP Transfer Fee Accounting Fix — Design Spec

**Date:** 2026-05-15
**Status:** Design — implementation pending review.
**Scope:** `ponzi_math/main.mo` only. No frontend code, no migration, no new stable state.

## Problem

Every payout path in `ponzi_math` calls `icpLedger.icrc1_transfer({ ..., fee = null, ... })`. With `fee = null` the ICP ledger applies its default 10,000 e8s (0.0001 ICP) transfer fee on top of `amount`, drawing `amount + 10_000` from the canister's actual balance. The internal accounting fields only reduce by `amount`, so each payout creates a permanent 10,000 e8s gap between actual and internal.

Internal accounting is the sum of these fields:

- `platformStats.potBalance` (Float ICP)
- `roundSeedReserve` (Float ICP) — half of every exit toll accumulates here between rounds and is carried into `potBalance` at `promoteAndReset` ([ponzi_math/main.mo:583-586](ponzi_math/main.mo:583)). Real canister ICP, just bucketed separately from `pot` until round-end.
- `sum(backerRepayments)` (Float ICP)
- `coverChargeBalance` (Nat e8s; ICP = field / 1e8)

Verified on mainnet 2026-05-15:

- Actual canister balance: 39,395,076 e8s (0.39395076 ICP)
- Internal accounting (using above formula): 0.39485 ICP
- Gap: ~0.0009 ICP (~9 prior fee-leaked payouts)

(User's diagnostic listed three fields; `roundSeedReserve` was implicit, plausibly 0 at sample time since the prior round had just promoted. Test plan below assumes the full four-field formula to be safe across all states.)

If left unfixed: a claimant will eventually request an internally-credited balance the canister cannot actually transfer. `icrc1_transfer` will fail, the rollback restores the credited balance, and the user is permanently stuck — the canister will never have the funds to honor the credit. Gap grows by 0.0001 ICP per payout, forever.

## Affected functions

Audit of every `icrc1_transfer` call site in `ponzi_math/main.mo`:

| Function | Line | Current `fee` arg | Behavior |
|---|---|---|---|
| `withdrawEarnings` | 926 | `null` | **BUGGED** — ledger deducts extra 10,000 e8s, no field tracks it |
| `settleCompoundingGame` | 1062 | `null` | **BUGGED** — same as above |
| `claimBackerRepayment` | 1136 | `null` | **BUGGED** — same as above |
| `sweepCoverCharges` | 1192 | `null` | **Already correct** — pre-deducts `Ledger.ICP_TRANSFER_FEE` from `transferAmount` at line 1187, so the field zeroing matches actual outflow |

The `icrc2_transfer_from` calls in `createGame` / `createBackdatedGame` are user → canister deposits and pay the fee on the user's side; not in scope.

## Approach (Option A: Deduct fee from recipient)

For each of the three bugged calls: replace

```motoko
let amountE8s = ...;
if (amountE8s > 0) {
    await icpLedger.icrc1_transfer({ ... amount = amountE8s; fee = null; ... });
}
```

with the pattern already established by `sweepCoverCharges`:

```motoko
let amountE8s = ...;
if (amountE8s > Ledger.ICP_TRANSFER_FEE) {
    let transferAmount = amountE8s - Ledger.ICP_TRANSFER_FEE;
    await icpLedger.icrc1_transfer({ ... amount = transferAmount; fee = null; ... });
} else {
    // see "Below-fee handling" per function
}
```

`Ledger.ICP_TRANSFER_FEE` is already defined as `10_000` at `ponzi_math/ledger.mo:31` and `Ledger` is already imported in `main.mo` — no new imports or constants.

### Why this approach

- **Zero new state.** No new stable fields, no migration on a live mainnet canister carrying real ICP.
- **Identical rollback paths.** The existing `originalGame` / `originalStats` / `originalRepayments` / `originalSeedReserve` snapshots stay valid; nothing new to unwind.
- **Precedent in-house.** `sweepCoverCharges` already does exactly this and is the only currently-balanced payout path.
- **On-brand.** The protocol already openly displays a Front-End Load and Carried Interest; surfacing a Network Fee on claims is consistent.

Alternatives considered (Option B: pot absorbs fee + new `transferFeesAccrued` field, Option C: hybrid) both add a stable field and new failure modes (pot < fee in insolvency paths) for marginal UX gain. Rejected as not worth the risk on a live canister.

## Per-function behavior

### `withdrawEarnings` (line 859)

Today: transfers `netEarningsE8s` (rounded `actualNetEarnings * 1e8`).

Change: if `netEarningsE8s > Ledger.ICP_TRANSFER_FEE`, transfer `netEarningsE8s - Ledger.ICP_TRANSFER_FEE` to the player. If `netEarningsE8s <= Ledger.ICP_TRANSFER_FEE` (including 0), skip the `icrc1_transfer` call entirely but still proceed with the rest of the function (state changes already applied above the transfer, ledger record, insolvency reset). Rationale: the existing `if (netEarningsE8s > 0)` guard already accepts a no-transfer success — we're widening the guard threshold by one fee.

The ledger-record event (`#withdrawal`) still records `netToPlayer = actualNetEarnings` (the credited amount, pre-fee), so historical ledger reads remain comparable to past data. The transfer fee is an implicit, universal IC cost; we don't bookkeep it as a separate field.

### `settleCompoundingGame` (line 980)

Today: transfers `payoutE8s`.

Change: same pattern. If `payoutE8s > Ledger.ICP_TRANSFER_FEE`, transfer `payoutE8s - Ledger.ICP_TRANSFER_FEE`. Otherwise skip the transfer and continue. Ledger record unchanged.

### `claimBackerRepayment` (line 1116)

Today: transfers `balanceE8s`. If balance is zero we already return `#Err("No repayment balance to claim")` up front.

Change: if `balanceE8s > Ledger.ICP_TRANSFER_FEE`, transfer `balanceE8s - Ledger.ICP_TRANSFER_FEE` and zero the per-series repayments as today. If `balanceE8s <= Ledger.ICP_TRANSFER_FEE`, **roll back** the zeroing and return `#Err("Claimable balance is below the network fee (0.0001 ICP); wait until your balance grows past the fee")`.

The roll-back must happen BEFORE the transfer attempt — i.e. we check the fee threshold before mutating `backerRepayments`. (Today the function zeroes first, then transfers, then rolls back on transfer failure; here we add an additional early-exit branch where we never zero in the first place.)

This is the only function that needs an explicit error response, because a backer might have a real but tiny accrued balance and we shouldn't silently delete it from their claimable state.

## Invariants after fix

For each payout call site, the following must hold:

- **Internal-accounting delta = actual-balance delta.**
- For `withdrawEarnings` / `settleCompoundingGame`:
  - Internal: `potBalance` decreases by `actualPotDeduction` (unchanged)
  - Actual: canister balance decreases by `transferAmount + fee = netEarningsE8s` (where `netEarningsE8s ≈ actualNetEarnings * 1e8`)
  - Today: `actualPotDeduction` is `earnings` (or `pot` in insolvency); `actualNetEarnings` is `actualPotDeduction - actualToll`; toll halves go to `roundSeedReserve` + `backerRepayments`. The fee was already implicitly absorbed by the gap between `actualPotDeduction` and `actualNetEarnings + actualToll` — except it wasn't, because `actualToll` is non-zero. So the fee leaked.
  - After fix: `actualPotDeduction = actualNetEarnings + actualToll`, but actual-balance drop is `(actualNetEarnings - feeFloat) + fee` paid to ledger = `actualNetEarnings`. The `actualToll` portion never left the canister — it was credited to internal-only fields (`roundSeedReserve`, `backerRepayments`) which is correct. So the fee deduction must come out of the recipient's portion (= what we're doing). ✓
- For `claimBackerRepayment`:
  - Internal: relevant `backerRepayments` entries go to 0; sum drops by `balance`
  - Actual: canister balance drops by `transferAmount + fee = balanceE8s`
  - Match. ✓

## Out of scope

- The existing ~0.0009 ICP historical drift. Documented and accepted per user direction. No corrective payout, no zeroing adjustment, no audit field.
- Frontend display of "network fee will be deducted from claim" copy. Belongs in a follow-up frontend PR; the fix itself is backend-only and complete without it.
- Renaming any `exitToll` / `coverCharge` internal identifiers. Per CLAUDE.md these are intentionally preserved.
- Touching the `icrc2_transfer_from` deposit paths.
- Re-auditing `sweepCoverCharges` — verified already correct.

## Test plan

No Motoko unit test framework is set up in this repo today. Verification is on-chain integration via the test-admin hatch, on a local replica, in this order:

1. `dfx start --clean` on a local replica with the ICRC ledger fixture.
2. Deploy `ponzi_math` with `testAdmin = CharlesPonzi` identity.
3. Top up the canister with test ICP so it has a non-zero pot.
4. **withdrawEarnings path.** As a test user: `createBackdatedGame` (simple plan, startTime 30 days ago) → `withdrawEarnings`. Assert: `getCanisterICPBalance` (e8s) equals `(potBalance + roundSeedReserve + sum(backerRepayments)) * 1e8 + coverChargeBalance` (within Float→Nat rounding tolerance — allow up to 10 e8s drift to absorb floating-point conversion noise across the three Float fields, but NOT 10,000+ e8s which would indicate the fee bug). Repeat 5x to confirm gap does not accumulate.
5. **settleCompoundingGame path.** As a test user: `createBackdatedGame` (compounding15Day, startTime 16 days ago) → `settleCompoundingGame`. Same assertion.
6. **claimBackerRepayment path.** Build a state where a backer has a non-trivial repayment balance (via tolls from steps 4–5). Call `claimBackerRepayment`. Same assertion.
7. **Below-fee path for `claimBackerRepayment`.** Construct a state where some backer's repayment balance is below 10,000 e8s. Call `claimBackerRepayment` from that backer. Assert: returns `#Err`, repayment balance is unchanged, canister balance is unchanged.
8. **Below-fee path for the two game-payout functions.** Set up a withdrawal where `netEarningsE8s == 0` (existing behavior) and `0 < netEarningsE8s <= 10_000`. In both cases assert state mutates as today minus the transfer, and the canister balance is unchanged on the no-transfer branch.

Verification is on a local replica only. No mainnet deploy in this change. The user explicitly authorizes mainnet deployment in a separate step.

## Constraints

- **Live mainnet canister with real ICP.** No deploy without explicit user permission. Per `memory/feedback_deploy_safety.md`: a prior incident caused data loss from an accidental redeploy.
- **No identifier renames.** `exitToll` and `coverCharge` stay as internal names per `CLAUDE.md`.
- **No new stable fields.** Anything added to `var` state would require migration thinking; we're not paying that cost for a fee fix.

## Acceptance

After implementation:

1. All three previously-bugged call sites use the `amount - Ledger.ICP_TRANSFER_FEE` pattern.
2. Local-replica integration runs through steps 4–8 above with zero accumulated gap.
3. `sweepCoverCharges` is untouched.
4. No new stable state fields. `dfx canister install --mode upgrade` would not trigger a state migration.
5. PR description acknowledges historical 0.0009 ICP drift as accepted.
