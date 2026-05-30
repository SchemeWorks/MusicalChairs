# Solana Chain Fusion M3 (Mainnet Flip + Soft Launch) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flip `ponzi_math_sol` from Solana devnet to mainnet-beta via a clean reinstall, seed a 0.1 SOL pot, and soft-launch real-SOL deposits.

**Architecture:** Phase 1 lands all *code* changes (a surgical shenanigans SOL-observer reset hatch + frontend devnet→mainnet copy) — buildable, committable, non-destructive. Phase 2 is the operator-gated mainnet cutover runbook (reinstall, fund, bootstrap, register seed, reset the cross-canister cursor, deploy frontend, smoke-test). Phase 3 is the soft-launch announcement. The chain-fusion architecture is unchanged; M3 changes configuration + data, not structure.

**Tech Stack:** Motoko (`ponzi_math_sol`, `shenanigans`), React/Vite frontend, `dfx` on the `ic` network, threshold Ed25519 (`key_1`), the DFINITY `sol-rpc` canister (`tghme-zyaaa-aaaar-qarca-cai`).

**Spec:** [`docs/superpowers/specs/2026-05-29-solana-chain-fusion-m3-design.md`](../specs/2026-05-29-solana-chain-fusion-m3-design.md).

---

## Critical rules (these bit prior milestones)

1. **No deploy without explicit operator permission.** Memory `feedback_deploy_safety` records a data-loss incident from an unauthorized redeploy. Every Phase 2 step is operator-gated; an agent executing this plan stops at the Phase 1/2 boundary and hands off.
2. **The reinstall is destructive and irreversible** — it wipes `ponzi_math_sol` state by design. It is correct here only because that state is throwaway devnet test data. Triple-check you are reinstalling `ponzi_math_sol` (the SOL canister), never `ponzi_math` (the ICP canister).
3. **`shenanigans` and `ponzi_math` are upgraded/untouched, never reinstalled.** Only `ponzi_math_sol` is reinstalled.
4. **Don't rename `exitToll` / `coverCharge` identifiers** (CLAUDE.md).
5. **Sequencing constraint:** `shenanigans.adminResetSolObserverState()` (Task 9) MUST run *after* the `ponzi_math_sol` reinstall (Task 6) and *before* the first real deposit (Task 11). Otherwise the observer either skips mainnet game 0 or mis-reads cursors.

## Coordination caveat

A separate unmerged plan — `docs/superpowers/plans/2026-05-29-pp-sol-otc-desk-backend.md` — also modifies `ponzi_math_sol` (it adds a `PpLedger` import and, in one draft, a `solRpcCanisterId` init field). If that work lands **before** M3, re-derive the Task 6 init-arg record from the *then-current* `ponzi_math_sol/main.mo` actor-class signature before reinstalling. This plan is written against the current 5-field init record (`main.mo:28-34`).

## File structure

**Modified (Phase 1 code):**
- `shenanigans/main.mo` — add one admin function `adminResetSolObserverState()` (no stable-shape change → no migration).
- `frontend/src/App.tsx` — remove the `DEVNET` chip (lines ~513-528).
- `frontend/src/components/Shenanigans/BuySOLFlyout.tsx` — flip the "DON'T send real SOL" warning (line ~92) and the "Send devnet SOL" label (line ~140).
- `frontend/src/components/GameTracking.tsx` — withdrawal copy (line ~479).
- `frontend/src/hooks/useQueries.ts` (line ~2354) and `frontend/src/hooks/usePonziMathSolActor.ts` (line ~8) — comment hygiene.

**New (Phase 3 content):**
- `docs/superpowers/plans/m3-launch-copy.md` — the landing line + tweet draft artifact (for operator approval; posting is manual).

**No new files of code.** No migration files.

---

## Phase 1 — Pre-flight code changes (build + commit, NO deploy)

### Task 1: Add `adminResetSolObserverState()` to shenanigans

Surgical, SOL-only reset. We deliberately do NOT reuse `primeObserverCursors()` — it also re-primes the **ICP** cursor (`gameIdCursor := maxIcpId`) and ICP `backerSeen`, which would skip any in-flight ICP mints. This hatch touches only SOL-side state.

**Files:**
- Modify: `shenanigans/main.mo`

- [ ] **Step 1: Confirm the SOL-side stable vars and their types**

Run: `grep -nE "var solGameIdCursor|var solBackerSeen|var missedSolGameMints|var missedSolBackerMints|var solGameMintRetries|var solBackerMintRetries" shenanigans/main.mo`

Expected (line numbers may drift; types must match):
```
var solGameIdCursor : Nat = 0;
var solBackerSeen = principalMap.empty<BackerSeen>();
var solGameMintRetries = natMap.empty<Nat>();      // transient
var missedSolGameMints = natMap.empty<Text>();
var solBackerMintRetries = principalMap.empty<Nat>(); // transient
var missedSolBackerMints = principalMap.empty<Text>();
```

- [ ] **Step 2: Find the insertion anchor (`setPonziMathSolPrincipal`)**

Run: `grep -n "func setPonziMathSolPrincipal" shenanigans/main.mo`
Expected: one match. Insert the new function immediately after that function's closing `};`.

- [ ] **Step 3: Add the function**

```motoko
    /// M3: reset ALL SOL-side observer state to a clean slate. Used once during
    /// the mainnet cutover, after ponzi_math_sol is reinstalled (fresh, gameId
    /// restarts at 0) and before the first real deposit, so the observer picks
    /// up mainnet game 0 and re-reads backer positions from zero. SOL-only:
    /// does NOT touch the ICP cursor or ICP backerSeen (unlike
    /// primeObserverCursors). Admin only.
    public shared ({ caller }) func adminResetSolObserverState() : async () {
        requireAdmin(caller);
        solGameIdCursor := 0;
        solBackerSeen := principalMap.empty<BackerSeen>();
        solGameMintRetries := natMap.empty<Nat>();
        missedSolGameMints := natMap.empty<Text>();
        solBackerMintRetries := principalMap.empty<Nat>();
        missedSolBackerMints := principalMap.empty<Text>();
    };
```

- [ ] **Step 4: Build-check (this codebase verifies Motoko via `dfx build --check`, not a unit harness)**

Run: `dfx build shenanigans --check 2>&1 | tail -5`
Expected: `Finished building canisters.` with no errors. If `BackerSeen` is unknown, confirm Step 1's type names against the actual declarations and match them exactly.

- [ ] **Step 5: Commit**

```bash
git add shenanigans/main.mo
git commit -m "feat(shenanigans): add adminResetSolObserverState for M3 cutover

Surgical SOL-only observer reset (solGameIdCursor -> 0; clears
solBackerSeen + missed/retry maps). Used once during the M3 mainnet
cutover after ponzi_math_sol is reinstalled, so the observer picks up
mainnet game 0. Does NOT touch ICP-side cursor/backerSeen (unlike
primeObserverCursors). Additive function, no stable-shape change, no
migration."
```

---

### Task 2: Flip the frontend devnet→mainnet warnings + chip

Pure copy/UI. There is **no** wallet-adapter network to change — `usePonziMathSolActor.ts:8` documents that the Solana cluster config lives entirely on the canister (`solRpcProvider`). The frontend only displays devnet *warnings* that must become mainnet-accurate.

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/components/Shenanigans/BuySOLFlyout.tsx`, `frontend/src/components/GameTracking.tsx`

- [ ] **Step 1: Remove the `DEVNET` chip in `App.tsx`**

Find the block (around lines 513-528) that starts with the comment `{/* DEVNET chip — SIWS users only.` and ends with the closing `)}` after the `DEVNET` span. Delete the entire block:

```jsx
                    {/* DEVNET chip — SIWS users only. Reminds them ponzi_math_sol
                        polls Solana devnet RPC, so mainnet SOL sent to their
                        deposit address is unrecoverable. */}
                    {walletType === 'siws' && (
                      <span
                        title="Connected to Solana devnet. Real SOL sent to a devnet-derived address is unrecoverable."
                        className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider"
                        style={{
                          background: 'rgba(255, 200, 0, 0.12)',
                          border: '1px solid rgba(255, 200, 0, 0.5)',
                          color: 'rgba(255, 200, 0, 0.95)',
                        }}
                      >
                        DEVNET
                      </span>
                    )}
```

(Post-flip the canister polls mainnet-beta, so the "unrecoverable devnet" warning is wrong and the chip is removed entirely. A subtle "LIVE/SOL" indicator is optional polish, out of scope for the soft launch.)

- [ ] **Step 2: Flip the `BuySOLFlyout.tsx` warning (line ~92)**

Replace:
```jsx
        ⚠️ DEVNET SOL ONLY — Do NOT send real SOL. The canister polls Solana devnet RPC; mainnet SOL sent here is lost.
```
with:
```jsx
        Send mainnet SOL only. Deposits are detected automatically within ~60 seconds and credited to your selected plan.
```

- [ ] **Step 3: Flip the `BuySOLFlyout.tsx` address label (line ~140)**

Replace `Send devnet SOL to this address` with `Send SOL to this address`.

- [ ] **Step 4: Flip the `GameTracking.tsx` withdrawal copy (line ~479)**

Replace `Withdrawals are sent to your connected Phantom wallet on Solana devnet.` with `Withdrawals are sent to your connected Phantom wallet.`

- [ ] **Step 5: Comment hygiene**

In `frontend/src/hooks/useQueries.ts` (~line 2354) change the comment `SOL hooks — backed by ponzi_math_sol on the IC, talking to Solana devnet` → `...talking to Solana mainnet-beta`. In `frontend/src/hooks/usePonziMathSolActor.ts` (~line 8) change `Devnet RPC config lives on the canister itself.` → `Mainnet RPC config lives on the canister itself.`

- [ ] **Step 6: Typecheck/build the frontend**

Run: `cd frontend && npm run build 2>&1 | tail -15`
Expected: build succeeds. (If it fails with `Cannot find module @rollup/rollup-darwin-*`, run `npm ci` — never add that package, per global rules.)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/Shenanigans/BuySOLFlyout.tsx frontend/src/components/GameTracking.tsx frontend/src/hooks/useQueries.ts frontend/src/hooks/usePonziMathSolActor.ts
git commit -m "feat(frontend): flip Solana copy devnet -> mainnet for M3

Removes the DEVNET chip and the 'do not send real SOL' warnings now
that ponzi_math_sol points at mainnet-beta. Pure copy: the cluster
config lives on the canister, so there is no wallet-adapter network to
change. Deploy is gated on the M3 cutover (Phase 2)."
```

---

### Task 3: Draft the soft-launch copy (landing line + tweet)

Content artifact for operator approval. Brand voice: **load the `musical-chairs-brand` skill before finalizing** — MLM/VC jargon, satire aimed at the user-as-investor, never casino framing; use **Front-End Load** (entry fee) and **Carried Interest** (Simple-plan withdrawal fee).

**Files:**
- Create: `docs/superpowers/plans/m3-launch-copy.md`

- [ ] **Step 1: Write the copy artifact**

Create `docs/superpowers/plans/m3-launch-copy.md` with a landing line and a tweet draft, e.g.:

```markdown
# M3 Soft-Launch Copy (draft — finalize via musical-chairs-brand skill)

## Landing line (hero / sign-in area)
> Now onboarding Solana LPs. Bring your SOL, claim your allocation, and
> start compounding Ponzi Points alongside our ICP cap table.

## @musicalchairsIC tweet (soft)
> Musical Chairs now accepts SOL. Same cap table, same Carried Interest,
> a fresh pot for our Solana-native investors. Connect Phantom, take a
> seat, and let the Front-End Load do its work. (Not financial advice.
> Obviously.)
```

- [ ] **Step 2: Refine with the brand skill**

Invoke the `musical-chairs-brand` skill and tighten both strings to voice. Keep the tweet within one post (no thread — soft launch per the spec).

- [ ] **Step 3 (optional): Add the landing line to the UI**

If the operator wants the landing line live (vs. tweet-only): `grep -rn "Sign in with" frontend/src` to locate the sign-in/hero surface, add the approved line as a small subheading, and rebuild (`cd frontend && npm run build`). Otherwise skip — the flipped warnings (Task 2) already make the SOL path usable.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/m3-launch-copy.md
git commit -m "docs(solana): M3 soft-launch copy draft (landing line + tweet)"
```

- [ ] **Step 5: Open the Phase 1 PR**

```bash
git push -u origin HEAD
gh pr create --title "Solana M3 Phase 1 — cutover code (observer reset hatch + frontend copy)" --body "Phase 1 of the M3 mainnet flip (spec: docs/superpowers/specs/2026-05-29-solana-chain-fusion-m3-design.md). Adds the shenanigans adminResetSolObserverState hatch and flips the frontend devnet->mainnet copy. NO deploys here — the mainnet cutover (Phase 2) is operator-gated.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

**STOP. End of agent-executable work.** Phase 2 is operator-gated (Critical rule 1). Do not proceed without explicit operator permission.

---

## Phase 2 — Mainnet cutover runbook (OPERATOR-GATED)

> This is an operational runbook, not TDD code. Each step is an exact command with expected output, run by the operator (identity `CharlesPonzi`) on the `ic` network, after the go/no-go gate. Real money is involved.

### Task 4: Pre-flight verification

- [ ] **Step 1: Identity + principals**

```bash
dfx identity whoami                       # expect: CharlesPonzi
dfx identity get-principal                # = operator principal (testAdmin + Series A owner)
dfx canister --network ic id backend      # = backendPrincipal for the init arg
dfx canister --network ic id ponzi_math_sol   # expect: spc6q-xyaaa-aaaac-qg2ma-cai
```

- [ ] **Step 2: Snapshot current (devnet) state for the record**

```bash
dfx canister --network ic call ponzi_math_sol getAllGames 2>&1 | head -20
dfx canister --network ic call ponzi_math_sol isBootstrapped
dfx canister --network ic call shenanigans getObserverStatus 2>&1 | grep -i sol
```
Expected: the devnet game 0 / `bootstrapped = true` / `solGameIdCursor = 1`. This is what the reinstall discards.

- [ ] **Step 3: Confirm the treasury address literal**

The mainnet cover-charge treasury is `5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2` (the operator's Phantom address, per spec). Verify this string character-for-character now — a wrong value sends SOL irrecoverably on the first `payManagementSol`.

### Task 5: GO / NO-GO gate

- [ ] **Step 1: Operator self-assessment (write it down):** "I commit to maintaining the `ponzi_math_sol` canister and the real SOL pot." Yes → continue. No → stop.
- [ ] **Step 2: Go/no-go:** confirm you are ready to put real SOL at risk and run an irreversible reinstall. Everything below this line moves real money.

### Task 6: Reinstall `ponzi_math_sol` with mainnet init args

- [ ] **Step 1: Stop, reinstall, start**

```bash
dfx canister --network ic stop ponzi_math_sol

dfx deploy --network ic ponzi_math_sol --mode reinstall --yes --argument '(record {
  backendPrincipal = principal "'$(dfx canister --network ic id backend)'";
  testAdmin = principal "'$(dfx identity get-principal)'";
  solTreasuryAddress = "5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2";
  solRpcProvider = variant { mainnet };
  keyId = record { algorithm = variant { ed25519 }; name = "key_1" };
})'

dfx canister --network ic start ponzi_math_sol
```

Notes: `--mode reinstall` wipes all state (intended). No `--wasm-memory-persistence` flag — that is for upgrades; reinstall wipes memory by design. `--yes` confirms the destructive reinstall non-interactively.

- [ ] **Step 2: Verify fresh state**

```bash
dfx canister --network ic call ponzi_math_sol getAllGames     # expect: (vec {}) — empty
dfx canister --network ic call ponzi_math_sol isBootstrapped  # expect: (false)
```

### Task 7: Fund the pool + bootstrap the nonce on mainnet-beta

- [ ] **Step 1: Get the pool address**

```bash
dfx canister --network ic call ponzi_math_sol adminDerivePoolAddress
```
Expected: `(variant { Ok = "<base58 pool address>" })`. (Same address as devnet — derivation is cluster-independent.)

- [ ] **Step 2: Fund it with 0.1 SOL (real)**

From the operator's Phantom (set to **mainnet-beta**), send **0.1 SOL** to the pool address from Step 1. Confirm arrival on a mainnet explorer (`https://explorer.solana.com/address/<pool>` — no `?cluster` param = mainnet).

- [ ] **Step 3: Bootstrap the durable nonce account**

```bash
dfx canister --network ic call ponzi_math_sol bootstrap '(null)'
```
Expected: `(variant { Ok = "<tx signature>" })`. If it returns an `Err` about blockhash, re-run (the endpoint is consensus-flaky; the call retries internally and is idempotent once `bootstrapped`).

- [ ] **Step 4: Verify bootstrap**

```bash
dfx canister --network ic call ponzi_math_sol isBootstrapped   # expect: (true)
```

### Task 8: Register the Series A seed + arm the detection timer

- [ ] **Step 1: Register the operator's 0.1 SOL Series A position**

```bash
dfx canister --network ic call ponzi_math_sol adminRegisterSeriesABacker "(principal \"$(dfx identity get-principal)\", 0.1 : float64)"
```
Expected: `(variant { Ok })`.

- [ ] **Step 2: Arm the recurring deposit-detection timer**

Reinstall did not run `postupgrade`, so the 60s timer is not auto-armed.
```bash
dfx canister --network ic call ponzi_math_sol adminStartDetectionTimer
```
Expected: `(variant { Ok = "Detection timer armed ..." })`.

### Task 9: Deploy the shenanigans reset hatch + reset SOL observer state

- [ ] **Step 1: Deploy shenanigans (Task 1 code) — additive, no migration**

```bash
dfx canister --network ic stop shenanigans
dfx deploy --network ic shenanigans --mode upgrade --wasm-memory-persistence keep --yes
dfx canister --network ic start shenanigans
```
(Use the standard shenanigans upgrade args from your prior deploys / `project_shenanigans_deploy_lineage`; this upgrade attaches **no** `(with migration = ...)` because the hatch adds no stable fields. `--wasm-memory-persistence keep` preserves all existing shenanigans state.)

- [ ] **Step 2: Reset SOL observer state (sequencing rule 5: after Task 6, before Task 11)**

```bash
dfx canister --network ic call shenanigans adminResetSolObserverState
```
Expected: `()`.

- [ ] **Step 3: Verify the cursor**

```bash
dfx canister --network ic call shenanigans getObserverStatus 2>&1 | grep -i solGameIdCursor
```
Expected: `solGameIdCursor = 0`.

### Task 10: Deploy the frontend (flipped copy)

- [ ] **Step 1: Build + deploy the asset canister**

```bash
cd frontend && npm run build
dfx deploy --network ic frontend
```
Expected: assets uploaded; the live site no longer shows the `DEVNET` chip or "do not send real SOL" warnings.

### Task 11: Smoke test end-to-end (gates the announcement)

- [ ] **Step 1: Real deposit via the production frontend**

On the live site, sign in with Phantom (mainnet-beta), open Buy SOL, `prepareSolDeposit` for the smallest plan amount, and send that SOL from Phantom. Within ~60s confirm: the deposit auto-credits, PP is minted, and chat shows `#signup`/`#roundResult` with `denomination = sol`.

```bash
dfx canister --network ic call ponzi_math_sol getAllGames        # expect: game 0 present
dfx canister --network ic call shenanigans getObserverStatus 2>&1 | grep -i solGameIdCursor   # expect: 1 (advanced past game 0)
```

- [ ] **Step 2: Withdrawal round-trip**

Withdraw from that game to a Phantom address; confirm real SOL arrives on a mainnet explorer. If anything fails here, **do not announce** — debug; the 0.1 SOL pot is recoverable via the pool address / admin sweep hatches.

- [ ] **Step 3: Verify CycleOps controller**

```bash
dfx canister --network ic info ponzi_math_sol | grep -i controllers   # expect cpbhu-5iaaa-aaaad-aalta-cai present
```

---

## Phase 3 — Soft launch (OPERATOR action)

### Task 12: Announce

- [ ] **Step 1:** Make the landing line live (if Task 3 Step 3 was chosen) — already deployed in Task 10.
- [ ] **Step 2:** Operator posts the approved `@musicalchairsIC` tweet (from `m3-launch-copy.md`). Posting is a manual operator action — do not auto-post.
- [ ] **Step 3:** Watch the first real deposits via `getObserverStatus` / chat; top up the pot beyond 0.1 SOL if/when volume justifies it.

---

## Post-launch housekeeping

- [ ] Run `graphify update .` after the code changes land.
- [ ] Update memory `project_ponzi_math_sol_m1_state` and `MEMORY.md`: M3 live, `ponzi_math_sol` on mainnet-beta, pool funded 0.1 SOL, devnet state discarded.
- [ ] Optional: re-anchor the SOL PP rates to live SOL/ICP price via `setSimple21DayPpPerSol` / `setCompounding15DayPpPerSol` / `setCompounding30DayPpPerSol` / `setBackerPpPerSol` (spec D6 — not a launch gate).

---

## Self-review

**Spec coverage:** §2 cutover → Tasks 6-8; §3 cross-canister reset → Tasks 1 + 9; §4 launch ops (go/no-go, smoke test, CycleOps) → Tasks 5, 11; §5 frontend flip → Task 2 + Task 10, marketing → Task 3 + Task 12; §6 verification/rollback → Task 11. D1-D6 all reflected (D5 reinstall = Task 6; D3 0.1 SOL = Tasks 7-8; D2 single gate = Task 5; D4 soft launch = Tasks 3/12; D6 rates = housekeeping). All covered.

**Placeholder scan:** init-arg record, the reset function body, the exact frontend strings, and every `dfx` command are concrete. Principals are sourced via `$(dfx ... id ...)` substitution (exact commands, not placeholders).

**Type/name consistency:** `adminResetSolObserverState` referenced identically in Tasks 1 and 9; init fields match `main.mo:28-34`; `keyId` shape matches `SolSigner.KeyId`; `solRpcProvider = variant { mainnet }` matches `SolRpc.Provider = { #devnet; #mainnet }`.
