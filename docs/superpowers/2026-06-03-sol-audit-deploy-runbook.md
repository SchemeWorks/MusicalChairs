# SOL audit — deploy runbook (2026-06-03)

Branch: `feat/sol-self-serve-backer`. Two commits to ship:

- `ed699cc` — **frontend** (verified: tsc, 49 vitest, vite build). Test-infra fix, blockhash timeout, wallet-cancel button.
- `34f12e3` — **backend** (`ponzi_math_sol` + `shenanigans`). Type-checked AND wasm-built with moc 1.5.1 (`--check` + codegen, 0 errors). NOT yet built with the project's dfx toolchain or upgrade-compat-checked.

Canister IDs (mainnet): `ponzi_math_sol` `spc6q-xyaaa-aaaac-qg2ma-cai` · `shenanigans` `j56tm-oaaaa-aaaac-qf34q-cai` · `frontend` `5qu42-fqaaa-aaaac-qecla-cai`.

> **Per `feedback_deploy_safety`: do not deploy any backend canister without explicit go-ahead. This runbook is the procedure, not permission.**

---

## 0. Self-serve Series A — DECIDED: ships gated OFF

This branch's `ponzi_math_sol` includes the **new self-serve Series A backer** feature (`prepareBackerDeposit`, commit `fcd3e7d`). **Decision (2026-06-03): ship it gated OFF.** A persistent flag `selfServeBackingEnabled` defaults to `false`, so `prepareBackerDeposit` rejects and the SOL backer panel renders a placeholder. The CRITICAL-1 / observer / HIGH-1 fixes ship now; self-serve stays dark until the economics below are settled, then an admin opens it with one call (§2). The deferred tokenomics issue:

`distributeExitToll` pays the backer half of every exit toll **flat, not by stake**: oldest Series A backer gets 35% (60% if sole), 25% splits among other Series A, 40% splits **equally across ALL backers**. Since self-serve Series A is open to any SIWS principal for ≥0.05 SOL, an attacker can spin up N cheap wallets, each capturing a perpetual *equal* share of the 40%/25% buckets — **Sybil dilution** of legitimate (and original) backers. Existing older backers keep the 35% "oldest" slot, but per-capita shares dilute.

**Before opening self-serve (`adminSetSelfServeBacking '(true)'`), pick one:**
1. Cap self-serve Series A (per-principal already merges to one position; add a global/aggregate cap).
2. Make the backer-half **stake-weighted** so a 0.05 SOL Sybil earns proportionally ~nothing.
3. Keep Series A **admin-only** (never open self-serve; remove the panel).
4. Give self-serve users a **separate, non-senior** tier.

All audit fixes (CRITICAL-1, observer dedup, HIGH-1 guards) ship together in this deploy; only the self-serve *entry point* is gated off, so no cherry-pick is needed.

> **Update (2026-06-03, backer cap-and-close):** `distributeExitToll` now enforces the promised "principal + 24% then close" ceiling — a position stops receiving toll once its cumulative lifetime repayment reaches its `entitlement`, and closed positions are dropped from the per-head counts (see §2a). This **bounds each backer's TOTAL payout** but does **not** remove the per-head Sybil *advantage* (a splitter still gets paid faster and jumps the senior queue). So the §0 decision still stands: cap-and-close caps the prize, **stake-weighting (option 2)** is still the lever that neutralises per-head Sybil. Treat them as complementary, not substitutes.

---

## 1. Pre-flight (all canisters)

```bash
# Branch must descend from each canister's currently-deployed commit (else M0169/M0216).
git log --oneline -3
dfx build ponzi_math_sol      # confirm with the PROJECT toolchain (moc 1.5.1 already passed here)
dfx build shenanigans
./ponzi_math_sol/scripts/test-critical1-guard.sh   # local replica: CRITICAL-1 guard (needs `dfx start`)
```

Capture pre-upgrade baselines for the diff-after check:
```bash
dfx canister --network ic call ponzi_math_sol getPlatformStats
dfx canister --network ic call ponzi_math_sol getMyPendingIntents      # + any backer/buy intent queries
# shenanigans: note current observer cursors / a few recent chat items
```

---

## 2. `ponzi_math_sol` upgrade — additive stable var, default-OFF self-serve

My edits add **one persistent var** (`selfServeBackingEnabled : Bool = false`) plus guards/helper/test-shims (`globalCriticalLock` etc. are `transient`). The new var is additive (default-on-upgrade), so the upgrade is compatible. **Self-serve Series A ships OFF** — `prepareBackerDeposit` rejects and the panel shows a placeholder until you run (after settling §0's economics):
```bash
dfx canister --network ic call ponzi_math_sol adminSetSelfServeBacking '(true)'
```
Snapshot-protected upgrade (per `ponzi_math_deploy_lineage`):

```bash
dfx canister --network ic stop ponzi_math_sol
dfx canister --network ic snapshot create ponzi_math_sol        # reversible backup
dfx canister --network ic install ponzi_math_sol --mode upgrade --wasm-memory-persistence keep \
  --argument '<EXACT --argument record from the LAST ponzi_math_sol deploy>'   # backendPrincipal; testAdmin; solTreasuryAddress; solRpcProvider=variant{Mainnet}; keyId
dfx canister --network ic start ponzi_math_sol
```
Verify, then **restore the snapshot if anything differs**:
```bash
dfx canister --network ic call ponzi_math_sol getPlatformStats   # potBalance / activeGames / games / intents == baseline
# rollback if mismatch:  dfx canister --network ic snapshot load ponzi_math_sol <snapshot-id>
```
Note: the two test shims are gated `not bootstrapped`; mainnet is bootstrapped, so they're **inert** (immediate `#Err`). Safe to ship, or strip before build if you prefer a clean prod surface.

---

## 2a. `ponzi_math_sol` — backer cap-and-close (second additive var, NO migration)

This change adds **one more persistent var** — `backerLifetimeRepaid : OrderedMap<BackerKey, Float>` (the cumulative lifetime repayment high-water mark). It is a new top-level stable field, so the upgrade stays **migration-free / compatible** — verified with `moc --stable-compatible` (HEAD→working-tree diff is exactly this one `stable var`, plus cosmetic type-hash renames). **Do NOT attach a migration module.** Same snapshot-protected `--mode upgrade --wasm-memory-persistence keep` procedure as §2.

What changed in behaviour:
- `distributeExitToll` now credits each position **capped at its remaining entitlement**; overshoot is routed to `roundSeedReserve` (the same tracked sink as the no-backers branch — solvency invariant `pot + seedReserve + Σrepayments` preserved). Closed positions (lifetime ≥ entitlement) are excluded from **both** crediting and the per-head counts, so closing one stops it diluting the rest.
- Bonus fix folded in: the SOL build previously **dropped** the 35–60% senior slice when the backer set was all Series-B (no Series-A → orphaned slice credited to nobody and not seeded — a real solvency leak). It now routes to `roundSeedReserve`, matching `ponzi_math` (ICP).
- New: `getBackerLifetimeRepaid : query → [(BackerKey, Float)]` (audit/reconcile), `adminSetBackerLifetimeRepaid(owner, backerType, amount)` (TEST_ADMIN — backfill lever; joins the pre-launch hatch to remove before blackhole).

> ### ⚠️ LIVE-FUNDS GATE — backfill before trusting the cap on existing backers
> `backerLifetimeRepaid` initialises **empty** on upgrade, so every pre-existing backer reads as **0 lifetime-repaid** → the cap thinks they've been paid nothing and will let them collect their **full entitlement again**. Before the cap is meaningful for current mainnet backers, **the owner must pick one (requires explicit sign-off):**
> 1. **Backfill** — for each live position set `adminSetBackerLifetimeRepaid(owner, type, Σclaims + currentUnclaimed)`, reconstructed from `#backerRepaymentClaim` ledger events + `getAllBackerRepayments`. (Caveat: claims collapse Series A+B into one principal-keyed amount, so a backer holding *both* types needs a manual A/B split.) Verify with `getBackerLifetimeRepaid`.
> 2. **One-time reset** — `adminClearAllBackerPositions` then re-register with fresh entitlements (only sane if all live positions are smoke-test sock-puppets).
> 3. **Accept the reset semantics** — knowingly let existing backers' caps start from 0 now (they get up to a fresh principal+24% from this point).
>
> Pick #1 if any backer is a real counterparty; #2/#3 only for test positions. Do this **before** opening self-serve or relying on auto-close. `getBackerLifetimeRepaid` + `getBackerPositions` lets you confirm `remaining = entitlement − lifetime` per position.

**ICP (`ponzi_math`) got the identical fix** on its own branch (the gap was shared — same uncapped `creditBackerRepayment`/`distributeExitToll`/`claimBackerRepayment`). It is a separate canister + separate deploy with the **same migration-free additive var and the same backfill gate**. NB: branch it off the **deployed** ICP commit `011612f` (current `feat/sol-self-serve-backer` tip for `ponzi_math/main.mo`), **not** `main` — `main` is ~166/34 lines behind and still lacks the deployed audit hardening (F-002/F-009/orphan-slice).

---

## 3. `shenanigans` upgrade — pure-additive stable var (NO migration attach)

Unlike the V6–V8 deploys, this change adds only one new stable var (`mintedEventIds`) and changes no stored record shape — so it migrates under default EOP persistence. **Do NOT attach a migration module.** Build as-is (the actor stays the bare `persistent actor Self {`).

```bash
dfx canister --network ic stop shenanigans
dfx canister --network ic snapshot create shenanigans
dfx canister --network ic install shenanigans --mode upgrade --wasm-memory-persistence keep \
  --argument '<EXACT --argument from the LAST shenanigans deploy, if any>'
dfx canister --network ic start shenanigans
```
**Safety reasoning (no mass re-mint):** `mintedEventIds` starts empty post-upgrade, but the observer cursors (`gameIdCursor`/`solGameIdCursor`) persist, so the observer only processes games *beyond* the cursor — already-minted games are never re-presented. The dedup is a forward-looking guard (trap-replay + the M3 reset), not a backfill. Verify the observer keeps ticking and recent chat/state is intact; restore snapshot on any anomaly.

---

## 4. `frontend` deploy

```bash
npm run build            # tsc && vite build  (already green)
dfx deploy frontend --network ic
```
No CSP / `SOLANA_RPC_ENDPOINT` change this session (those move only at the M3 mainnet RPC flip — see `frontend_csp_connect_src`). The wallet-cancel button + blockhash timeout are pure UI.

---

## 5. Post-deploy smoke

- `ponzi_math_sol`: open a tiny deposit intent; confirm a credit still lands (guard didn't regress the happy path); confirm a withdraw still pays out.
- `shenanigans`: confirm the observer mints a new game once (not twice) and chat items appear once.
- frontend: deposit flow shows the Cancel button mid-spinner and falls back to manual cleanly.

## 6. Rollback
Any canister: `dfx canister --network ic snapshot load <canister> <snapshot-id>` then `start`. Frontend: redeploy the prior `dist`.
