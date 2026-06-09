# Backer cap-and-close — investigation, implementation, and open decisions (2026-06-03)

Scope: the "Series A backers are promised principal + 24%, then their position
closes" promise was **never enforced** — positions collected exit-toll shares
forever, uncapped. This report covers (a) the ICP findings, (b) the implemented
+ moc-verified (not deployed) cap-and-close with its migration/backfill
rationale, (c) a recommendation on the oldest/re-deposit reset, and (d) a note
on per-head vs stake-weighted distribution.

**Nothing here is deployed.** Both money canisters (`ponzi_math` ICP
`guy42-yqaaa-aaaaj-qr5pq-cai`, `ponzi_math_sol` SOL `spc6q-xyaaa-aaaac-qg2ma-cai`)
hold live funds — see the LIVE-FUNDS GATE before relying on the cap.

---

## (a) ICP investigation — does `ponzi_math` share the gap?

**Yes — identically.** The SOL backer subsystem was ported from ICP and both
carry the same uncapped design. Precise findings in `ponzi_math/main.mo`
(pre-edit, commit `011612f`):

| Concern | `ponzi_math` (ICP) | `ponzi_math_sol` (SOL) |
|---|---|---|
| `BackerPosition.entitlement` set at creation, never read for a payment decision | yes — type ~L108; only summed for the `getTotalBackerDebt` display stat (~L1561) | yes — L108 / L1495 |
| `creditBackerRepayment` accumulates with **no cap** | yes — L437 | yes — L529 |
| `distributeExitToll` credits **every** position, no `isActive`/entitlement filter | yes — L460 | yes — L552 |
| `claimBackerRepayment` pays accrued balance, **no entitlement cap** | yes — L1293 | yes — L1318 |
| Positions removed only by admin; nothing auto-closes | yes — `adminMergeBackerPosition` L1931, `adminClearAllBackerPositions` L1995 | yes — L1784 / L1848 |
| `BackerPosition.isActive` exists but is **dead** (set `true`, never read) | yes — L110 | yes — L110 |

**One pre-existing divergence (in ICP's favour):** ICP's `distributeExitToll`
already routes the **orphaned senior slice** to `roundSeedReserve` when there is
no Series-A backer (only Series-B positions exist) — L519-537. The SOL build did
**not**: its `null`-oldest arm was empty (old L611-614), so 35–60% of the backer
half (up to 30% of the whole toll) was carved out and credited to **nobody**,
nor seeded — a real solvency leak + phantom `#tollDistribution` ledger entry.
The cap-and-close rewrite fixes this in SOL as a side effect (every uncredited
portion now routes to `roundSeedReserve`).

Net: both needed the cap-and-close fix; SOL additionally needed the orphan fix.

---

## (b) Implemented cap-and-close (moc-verified, NOT deployed)

### Mechanism (identical in both canisters)

1. **New top-level stable map** `backerLifetimeRepaid : OrderedMap<BackerKey, Float>`
   — the cumulative LIFETIME repayment credited per position. Unlike
   `backerRepayments` (the UNCLAIMED balance, zeroed by `claimBackerRepayment`),
   it only ever grows. It is the high-water mark that enforces the cap.
   `remaining = entitlement − lifetimeRepaid`; a position is **closed** once
   `remaining ≤ ε` (`ε = 1e-8`, a dust threshold so a float residue can't keep
   a repaid position alive forever).

2. **`creditBackerRepayment` caps at remaining entitlement** and returns the
   uncredited OVERSHOOT. It advances both `backerRepayments` (claimable) and
   `backerLifetimeRepaid` (high-water) by the credited portion only.

3. **`distributeExitToll` excludes closed positions** from BOTH crediting and
   the per-head counts (so closing one *concentrates* future tolls on the
   remaining open backers instead of leaving a dead head in the denominator).
   All overshoot — capped-out recipients **and** the orphaned senior slice — is
   routed to `roundSeedReserve`, the same tracked sink the no-backers branch
   uses. The `#tollDistribution` ledger fields now report **actually-credited**
   amounts (overshoot folded into `toSeedReserve`).

4. **Admin maintenance:** `adminMergeBackerPosition` now also sums the lifetime
   high-water (so merged `remaining = Σentitlement − Σlifetime` is preserved);
   `adminClearAllBackerPositions` also empties the lifetime map.

5. **Observability + backfill lever (additive):**
   - `getBackerLifetimeRepaid : query → [(BackerKey, Float)]`
   - `adminSetBackerLifetimeRepaid(owner, backerType, amount)` (TEST_ADMIN only)

### Overshoot routing — decision & justification

Overshoot goes to **`roundSeedReserve`**, not redistributed among still-open
backers. Rationale: (1) consistent with the existing no-backers / orphaned-slice
sinks; (2) preserves the solvency invariant `pot + roundSeedReserve + Σrepayments`
exactly (money moves *within* the sum); (3) redistribution would cascade (a
re-credit can push another backer over their cap, requiring iteration) and would
partially *defeat* the bound by accelerating the remaining backers toward their
own caps; (4) it's the conservative choice for live funds — the excess stays in
the pot ecosystem (next round's seed) benefiting players rather than over-paying
past the promise. The seed reserve carries into the next round's pot on reset.

### Migration approach — decision & justification

**Migration-free.** `backerLifetimeRepaid` is a new top-level stable `var`, which
is an *implicit/compatible* upgrade under enhanced orthogonal persistence
("adding actor fields"). The alternative — adding a field to the **stored**
`BackerPosition` record — would have required an explicit `(with migration = …)`
function (old records lack the field). The map avoids that entirely.

Verified with `moc 1.5.1` (via the mops scratch toolchain):

| Check | `ponzi_math_sol` | `ponzi_math` |
|---|---|---|
| `--check` (type-check) | **0 errors** | **0 errors** |
| full wasm codegen (`-o out.wasm`) | **0 errors**, 1.39 MB | **0 errors**, 749 KB |
| `--stable-compatible <HEAD.most> <new.most>` | **COMPATIBLE** | **COMPATIBLE** |

The stable-signature diff in both is exactly one line — `stable var
backerLifetimeRepaid : …Tree<BackerKey, Float>` — confirming a clean,
migration-free additive upgrade. (Warnings are unchanged from baseline:
pre-existing M0154 cycles-deprecation etc.)

### ⚠️ LIVE-FUNDS GATE — existing mainnet backers (needs owner sign-off)

`backerLifetimeRepaid` initialises **empty** on upgrade. Every pre-existing
backer therefore reads as **0 lifetime-repaid** — the cap thinks they've been
paid nothing and will let them collect their **full entitlement again** on top
of whatever they already received. **This under-counting must be resolved before
the cap is trusted for current backers.** I did **not** silently choose; the
options (pick one, owner decides):

1. **Backfill** (real counterparties): for each live position call
   `adminSetBackerLifetimeRepaid(owner, type, Σclaims + currentUnclaimed)`,
   reconstructed from `#backerRepaymentClaim` ledger events + `getAllBackerRepayments`.
   *Caveat:* a `#backerRepaymentClaim` collapses Series A+B into one
   principal-keyed amount, so a backer holding **both** types needs a manual A/B
   split — on-chain data alone can't disambiguate. Verify with `getBackerLifetimeRepaid`.
2. **One-time reset** (only if all live positions are test sock-puppets):
   `adminClearAllBackerPositions`, then re-register with fresh entitlements.
3. **Accept reset semantics:** knowingly let existing caps start from 0 now.

`getBackerLifetimeRepaid` + `getBackerPositions` lets you confirm
`remaining = entitlement − lifetime` per position after whichever path.

### Known minor follow-up (not changed, flagged)

- `getOldestSeriesABacker` (display query) still returns the oldest among **all**
  Series-A incl. closed; distribution uses the oldest **open**. Purely cosmetic
  (no money impact). Filter to open if you want the "senior backer" badge to
  track the actual senior recipient.
- `getTotalBackerDebt` still sums **all** entitlement (gross promised). True
  *outstanding* debt is now `Σ(entitlement − lifetime)` over open positions —
  derivable from the two queries; left unchanged to avoid frontend churn.
- `adminSetBackerLifetimeRepaid` joins the **pre-launch TEST_ADMIN hatch** that
  must be removed/secured before blackholing (see the ponzi_math audit).

---

## (c) Recommendation — should a new deposit reset `firstDepositDate := now`?

**Recommendation: NO, do not reset on re-deposit. Rely on cap-and-close's natural
seniority rotation. If further limiting is wanted, prefer stake-weighting the
senior slice (or a volume-weighted age) over a naive reset.**

Reasoning:

- **A naive reset is largely ineffective AND perverse.** The "oldest" slot is
  held by the earliest `firstDepositDate`. A holder does **not** need to top up
  to keep it — they keep it by simply *not* depositing again. Resetting on
  top-up only forces a choice ("top up **or** stay senior") on someone who
  *chooses* to re-invest; it does nothing to the dominant strategy (deposit once
  early, sit). Meanwhile it **penalises re-investment** and redefines "oldest" as
  "the position that has gone longest without adding" — a strange thing to reward
  with 35–60% of every toll.

- **Cap-and-close already fixes the durability half of the concern.** The senior
  slot now **rotates**: once the oldest backer is repaid to principal + 24% their
  position **closes** and seniority passes to the next-oldest **open** backer.
  Perpetual capture now requires being perpetually *owed* money — i.e.
  continuously injecting fresh capital — which is legitimate investment, not a
  free ride. Raising the cap by topping up is fair: you deposited more, you're
  owed more (`entitlement = Σdeposits × 1.24`).

- **The residual issue is structural, not date-based.** "One head gets a flat
  35–60% of every toll" is a *positional, winner-take-most* reward — the same
  class of problem as per-head distribution (§d). A timestamp reset just moves
  which head wins; it doesn't make the reward proportional. If you want to act:
  - **Stake-weight the senior slice**, or split it among the **top-K oldest**,
    instead of winner-take-all to one head; or
  - **Time-box** seniority (rotate every N tolls / per round) independent of
    deposits; or
  - if you specifically dislike that a top-up preserves an ancient date while
    adding new stake, use a **volume-weighted average date**
    `(oldAmt·oldDate + newAmt·now) / (oldAmt + newAmt)` — a big top-up pulls your
    effective age forward proportionally without fully sending a re-investor to
    the back. Principled middle ground; only worth it if the senior slice stays
    winner-take-most.

This is analysis only — **no code changed for #3** (per instructions).

---

## (d) Note — per-HEAD vs stake-weighted distribution (separate decision)

Distribution is **per-HEAD**, not per-stake: the senior gets a flat 35–60%, the
other Series-A split 25% equally, and **all** backers split 40% equally — none of
it scales with how much you staked.

- **Cap-and-close bounds the TOTAL and makes it Sybil-invariant.** Splitting a
  deposit `D` into `N` wallets gives `N` entitlements of `(D/N)·1.24`, summing to
  the same `D·1.24` as one wallet. So total extraction per principal is now
  capped regardless of splitting — a real improvement over the old uncapped design.
- **But the per-head SPEED / PRIORITY / DILUTION advantage remains.** `N` wallets
  each draw a per-head share, so a splitter accrues toward their (smaller) caps
  **faster**, gets repaid **sooner** (a priority edge in an insolvency-prone
  pot), occupies multiple of the "other Series-A" heads, and **dilutes honest
  backers** (the per-head denominator is inflated by Sybil heads) for as long as
  those positions are open.
- **Stake-weighting (per-SOL / per-ICP) would neutralise all of it** — your share
  ∝ your stake no matter how many wallets you split into. This is runbook §0
  option 2, and it pairs with (not replaces) cap-and-close.

Flagged for a separate decision — **not implemented** (per instructions).

---

## Verification recipe (reproducible)

```bash
# scratch toolchain (no repo mops.toml needed)
mkdir -p /tmp/moccheck && printf '[dependencies]\n\n[toolchain]\nmoc = "1.5.1"\n' > /tmp/moccheck/mops.toml
( cd /tmp/moccheck && mops add base && mops toolchain use moc 1.5.1 )
MOC="$(cd /tmp/moccheck && mops toolchain bin moc)"; BASE=/tmp/moccheck/.mops/base@0.16.0/src

# type-check + codegen
"$MOC" --check --package base "$BASE" ponzi_math_sol/main.mo      # 0 errors
"$MOC"        --package base "$BASE" -o /tmp/sol.wasm ponzi_math_sol/main.mo

# upgrade compatibility (baseline = committed HEAD before the edit)
"$MOC" --stable-types --package base "$BASE" -o /tmp/new.wasm ponzi_math_sol/main.mo   # emits /tmp/new.most
#   (compile git-HEAD copy similarly to /tmp/base/out.most, then:)
"$MOC" --stable-compatible /tmp/base/out.most /tmp/new.most       # => no output = COMPATIBLE
# (identical procedure for ponzi_math/main.mo)
```
