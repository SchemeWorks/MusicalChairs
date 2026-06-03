# SOL audit — deploy runbook (2026-06-03)

Branch: `feat/sol-self-serve-backer`. Two commits to ship:

- `ed699cc` — **frontend** (verified: tsc, 49 vitest, vite build). Test-infra fix, blockhash timeout, wallet-cancel button.
- `34f12e3` — **backend** (`ponzi_math_sol` + `shenanigans`). Type-checked AND wasm-built with moc 1.5.1 (`--check` + codegen, 0 errors). NOT yet built with the project's dfx toolchain or upgrade-compat-checked.

Canister IDs (mainnet): `ponzi_math_sol` `spc6q-xyaaa-aaaac-qg2ma-cai` · `shenanigans` `j56tm-oaaaa-aaaac-qf34q-cai` · `frontend` `5qu42-fqaaa-aaaac-qecla-cai`.

> **Per `feedback_deploy_safety`: do not deploy any backend canister without explicit go-ahead. This runbook is the procedure, not permission.**

---

## 0. GATING DECISION — resolve before shipping `ponzi_math_sol`

This branch's `ponzi_math_sol` includes the **new self-serve Series A backer** feature (`prepareBackerDeposit`, commit `fcd3e7d`) — *not previously deployed*. Deploying this branch **activates self-serve Series A**, which has an open tokenomics issue:

`distributeExitToll` pays the backer half of every exit toll **flat, not by stake**: oldest Series A backer gets 35% (60% if sole), 25% splits among other Series A, 40% splits **equally across ALL backers**. Since self-serve Series A is open to any SIWS principal for ≥0.05 SOL, an attacker can spin up N cheap wallets, each capturing a perpetual *equal* share of the 40%/25% buckets — **Sybil dilution** of legitimate (and original) backers. Existing older backers keep the 35% "oldest" slot, but per-capita shares dilute.

**Decide one before enabling self-serve Series A:**
1. Cap self-serve Series A (per-principal already merges to one position; add a global/aggregate cap).
2. Make the backer-half **stake-weighted** so a 0.05 SOL Sybil earns proportionally ~nothing.
3. Keep Series A **admin-only** (ship the audit fixes without `prepareBackerDeposit`).
4. Give self-serve users a **separate, non-senior** tier.

**If you want the CRITICAL-1 fix out NOW without this decision:** the CRITICAL-1 lock guard and the observer dedup are *independent* of the self-serve feature. Cherry-pick those onto the currently-deployed state and ship them alone; defer `prepareBackerDeposit` + HIGH-1 until the decision lands.

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

## 2. `ponzi_math_sol` upgrade — pure code change (NO new stable state)

My edits add **no persistent vars** (guards/helper/test-shims only; `globalCriticalLock` etc. are `transient`). So this is a plain upgrade; stable layout is unchanged. Snapshot-protected (per `ponzi_math_deploy_lineage`):

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
