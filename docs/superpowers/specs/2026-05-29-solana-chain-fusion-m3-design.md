# Musical Chairs — Solana Chain Fusion M3 (Mainnet Flip + Soft Launch) — Design Spec

**Date:** 2026-05-29
**Status:** Design approved — implementation plan to follow.
**Parent spec:** [2026-05-25-solana-chain-fusion-design.md](2026-05-25-solana-chain-fusion-design.md) — §"Milestones › M3: Mainnet rollout". This spec settles the *operational* decisions that the parent left high-level.
**Predecessors shipped:** M0 (SIWS sign-in), M1 (`ponzi_math_sol` on devnet, PR #91), M2 (shenanigans observer wiring, PR #92), Frontend M2 (PR #93), SOL withdrawals (PR #95), deferred-cleanup (PR #96). M3 is the **sole remaining Solana milestone**.

## Why this exists

`ponzi_math_sol` (`spc6q-xyaaa-aaaac-qg2ma-cai`) is live on IC mainnet but configured to talk to Solana **devnet** RPC. M3 flips it to Solana **mainnet-beta**, seeds a real pot, and turns on real-SOL deposits with a deliberately quiet launch. The parent spec's M3 section is four bullets; this spec pins down *how* to flip a already-deployed money canister, the cross-canister coordination it requires, and the soft-launch wrapper.

## Goal

A Phantom user can deposit **real SOL** into `ponzi_math_sol`, get PP minted by the shenanigans observer, play in the shared Shenanigans game, and withdraw real SOL — all on Solana mainnet-beta, with the operator's real-money exposure capped at a **0.1 SOL** seed at launch.

## Decisions (settled in brainstorming 2026-05-29)

| # | Decision | Choice |
|---|---|---|
| D1 | Plan scope | **Full launch playbook** — canister cutover + launch ops + marketing copy. |
| D2 | Pre-launch gates | **Operator self-assessment + a single go/no-go gate** immediately before the irreversible steps. No blocking audience-pilot phase (devnet exposure substitutes). |
| D3 | Pot seed | **0.1 SOL**, registered as the operator's Series A position. Top-up-able later. |
| D4 | Marketing loudness | **Soft launch** — landing-page "now accepting SOL" update + one `@musicalchairsIC` tweet. |
| D5 | Flip mechanism | **Clean reinstall** (`dfx deploy --mode reinstall` with mainnet init args). Wipes throwaway devnet state for correct fresh accounting; zero new `ponzi_math_sol` code. |
| D6 | PP SOL rates | **Keep current 30× rates** (`simple21DayPpPerSol = 6_000`, `12_000`, `18_000`, `backerPpPerSol = 120_000`). Re-anchoring to live SOL/ICP price is an **optional** admin-tunable step, not a launch gate. |

## Non-goals

- **Wiping or redeploying `shenanigans`.** Only `ponzi_math_sol` is reinstalled. Shenanigans keeps all state (ICP games, chat, PP economy, referrals); it receives a SOL-observer cursor reset only (§3).
- **Touching `ponzi_math` (ICP) or the ICP pot.** Untouched.
- **Migrating devnet positions to mainnet.** Devnet state is throwaway test data; the reinstall discards it. The mainnet pot starts at zero.
- **Cleaning the devnet PP already minted to `tester1`** (2,700 PP). Internal principal; left in the unified PP ledger as a negligible artifact (admins/test principals are leaderboard-exempt).
- **Re-deriving threshold addresses.** `key_1` is unchanged; pool/nonce/per-user addresses are identical on devnet and mainnet-beta (derivation is cluster-independent).
- **A loud launch / paid campaign / partnership outreach.** Explicitly deferred per D4.
- **Renaming `exitToll` / `coverCharge` identifiers.** Per CLAUDE.md.

## Key technical facts (verified against the live code, 2026-05-29)

1. **Cluster selection is `solRpcProvider`**, a stable `var` seeded from init args (`main.mo:245`). There is **no runtime setter** for it — which is *why* the flip needs a reinstall (or new code). `SolRpc.Provider` already has mainnet variants (`#HeliusMainnet`, `#Default(#Mainnet)`, …).
2. **Treasury already has a setter** — `adminSetSolTreasuryAddress` (`main.mo:2772`). Init args also carry it, so reinstall sets it directly; the setter is a backstop.
3. **Pool/nonce addresses are cluster-independent.** Derivation paths are `["pool"]` / `["nonce"]` over `key_1` (`main.mo:553–558`), with no Solana-cluster input. The same pool address that holds devnet SOL is the mainnet-beta pool address — but on mainnet-beta it starts at **0 balance** and the **nonce account does not exist**, so a fresh `bootstrap()` is mandatory.
4. **`adminForceReset` is a game *round* reset, not a wipe.** It calls `triggerGameReset` (`main.mo:1832–1841`); it does **not** zero `bootstrapped`, the nonce cache, or `platformStats`. So an in-place flip would need *new* wipe code — reinforcing the reinstall choice.
5. **`--mode reinstall` does not run `postupgrade`.** The detection timer is armed in `postupgrade` (on upgrades) and inside `bootstrap()`/`adminStartDetectionTimer`. After a reinstall the timer must be armed explicitly post-bootstrap (§2 step 6).
6. **shenanigans SOL observer state is stateful and stale.** `solGameIdCursor = 1` (advanced past devnet game 0); `solBackerSeen` holds stale devnet entries for the operator + `tester1`. These live on the shenanigans canister and survive the `ponzi_math_sol` reinstall — see §3.

## §1 — Architecture (unchanged from parent)

No architectural change. The chain-fusion topology (SIWS → deterministic principal → `ponzi_math_sol` deposits via per-user t-Ed25519 addresses → sweep to pool → withdrawals signed by `["pool"]` → shenanigans observer mints PP) is exactly as shipped in M1/M2. M3 changes **configuration and data**, not structure: which Solana cluster the RPC sources point at, and a one-time state reset on both canisters.

## §2 — Workstream 1: Canister cutover

Ordered sequence, entirely behind the D2 go/no-go gate. The exact `dfx`/`icp` commands and stop→deploy→start dance land in the implementation plan.

1. **Confirm mainnet init args.** `solRpcProvider = #Default(#Mainnet)` (mirrors the devnet `#Default(#Devnet)` multi-provider set — exact value confirmed against the current deploy config at plan time); `keyId = key_1`; `solTreasuryAddress = 5EVdR6qcPuDqJb6W69fmcvTJjbEUGZqQtefEB8sK8QQ2` (**verify byte-for-byte — sending SOL to a wrong treasury is irreversible**); `backendPrincipal` / `testAdmin` unchanged from the current deploy.
2. **Stop → `dfx deploy ponzi_math_sol --mode reinstall` (mainnet args) → start.** No migration attached (reinstall, not upgrade). Result: all stable state wiped; `nextGameId = 0`; `bootstrapped = false`; `platformStats` zeroed. Same canister ID, so the frontend's `ponzi_math_sol` id and shenanigans' `ponziMathSolPrincipal` stay valid.
3. **Derive + fund the pool.** `adminDerivePoolAddress()` (returns the same address as devnet). Operator sends **0.1 SOL** of real SOL to it. (0.1 comfortably covers the ~0.003 SOL nonce-rent + fee floor with the rest as pot liquidity.)
4. **`bootstrap()` on mainnet-beta.** Creates + initializes the durable nonce account; retries on the consensus-flaky `getLatestBlockhash` (up to the existing retry budget). The single fiddliest step; if it fails it is safely retryable.
5. **`adminRegisterSeriesABacker(operator, 0.1)`** — record the seed as the operator's Series A position (entitlement bonus per the standard backer rules).
6. **Arm the detection timer** — call `adminStartDetectionTimer()` explicitly (reinstall skipped `postupgrade`, so the recurring 60s deposit-detection timer is not auto-armed).
7. **Verify CycleOps controller** (`cpbhu-5iaaa-aaaad-aalta-cai`) is still a controller of `ponzi_math_sol` post-reinstall.

## §3 — Workstream 1b: Cross-canister observer reset (the coupling)

After reinstall, `ponzi_math_sol`'s first real deposit is **game 0**. The shenanigans observer mints only for games with `id >= solGameIdCursor`, and that cursor is at **1** — so the first real user's game would be **silently skipped and never minted**. This is the one mandatory cross-canister fix.

- **MUST:** reset shenanigans `solGameIdCursor → 0`.
  - **Open detail for the plan:** determine whether `primeObserverCursors` resets the cursor to the SOL source's current max (which is 0 immediately post-reinstall, before any deposits) — if so, calling it after reinstall but before the first deposit suffices. If `primeObserverCursors` only *advances* (never lowers) the cursor, add a one-line admin setter (`adminSetSolGameIdCursor`) to shenanigans. The plan resolves this by reading `primeObserverCursors`' semantics first.
- **SHOULD (low stakes):** clear the operator's stale `solBackerSeen` entry. Otherwise the operator's own 0.1 SOL Series A won't mint backer PP (stale devnet `seen` > 0.1 → no positive delta). Negligible because the operator is leaderboard-exempt; include only if the cursor reset already requires a shenanigans deploy.
- **Sequencing:** do the cursor reset **after** the `ponzi_math_sol` reinstall and **before** the operator's first smoke-test deposit, so game 0 is observed cleanly.

Any shenanigans change here follows the documented stop → deploy → start dance and (if it touches stable shape) the migration pattern — but a cursor reset via an existing/added setter needs no migration.

## §4 — Workstream 2: Launch ops

- **Go/no-go gate (D2).** A single explicit operator sign-off immediately before §2 step 2 (the irreversible reinstall + funding). Implemented as a plan checkpoint, not code.
- **Smoke test before announcing.** Operator performs one tiny real-SOL deposit end-to-end on mainnet-beta **through the production frontend** (which also validates the §5 cluster config): `prepareSolDeposit` → send SOL from Phantom → confirm auto-credit (detection timer) → confirm PP minted + `#signup`/`#roundResult` in chat with `denomination = sol` → confirm a withdrawal round-trips real SOL back to a Phantom address. Only after this passes do we proceed to §5.
- **Operator self-assessment (D2).** A written one-line confirmation in the plan that the operator commits to maintaining the SOL pot + canister.

## §5 — Workstream 3: Frontend cutover + marketing (soft)

**Frontend cluster config (technical — must ship and be verified before announcing):**

- **Solana wallet-adapter network** flips devnet → mainnet-beta so Phantom signs/sends against mainnet. Confirm where the cluster is configured (wallet-adapter provider + any `@solana/web3` / connection endpoint).
- **Remove/flip the `DEVNET` chip** added in Frontend M2 (`feat(frontend): auto-select Buy widget by walletType + DEVNET chip`) — at mainnet it should read mainnet or disappear.
- **Explorer / cluster-tagged links** — drop any `?cluster=devnet` query params on Solana explorer links.
- QR `solana:<address>` URLs are cluster-agnostic (address only) — no change.

**Marketing copy** — drafted in **Musical Chairs brand voice** (load the `musical-chairs-brand` skill first): MLM/VC jargon, satire aimed at the user-as-investor, never casino framing. Use current user-facing names — **Front-End Load** (entry fee), **Carried Interest** (Simple withdrawal fee).

- **Landing-page "now accepting SOL"** surface — copy + making the SOL path visibly live (SIWS sign-in + `BuySOLFlyout` already exist from Frontend M2, so this is largely copy + announcement, not new UI).
- **One `@musicalchairsIC` tweet** — soft announcement. The plan produces the draft for operator approval before posting; posting itself is an operator action.

## §6 — Verification & rollback

- The **smoke test (§4) gates the announcement** — nothing is marketed until a real deposit + withdrawal round-trips on mainnet-beta.
- **Blast radius is capped at 0.1 SOL.** The only real money at risk is the seed.
- **Reinstall is the riskiest step** and is gated; `bootstrap()` failure is retryable; if anything looks wrong post-flip, the recovery is simply *do not announce* and debug (the 0.1 SOL is recoverable via the pool address / admin sweep hatches).
- **No data-loss risk to other canisters** — shenanigans and `ponzi_math` are untouched except the deliberate SOL-cursor reset.

## Risks

| Risk | Mitigation |
|---|---|
| Real-money irreversibility of mainnet txs | 0.1 SOL cap + single go/no-go gate + byte-for-byte treasury verify + smoke test before announce |
| **shenanigans cursor coupling** — game 0 skipped, first real user not minted | Mandatory `solGameIdCursor → 0` reset (§3), sequenced before first deposit |
| Operator's own seed doesn't mint backer PP (stale `solBackerSeen`) | Optional `solBackerSeen` clear; negligible (operator leaderboard-exempt) |
| Nonce bootstrap flakiness on mainnet-beta | Existing retry budget on `getLatestBlockhash`; retryable; `adminMarkBootstrapped`/`adminRefreshNonce` recovery hatches exist |
| RPC provider SLA on mainnet-beta | `#Default(#Mainnet)` multi-provider consensus via the DFINITY sol-rpc canister; monitor |
| Frontend still pointing at devnet (Phantom signs on wrong network; `DEVNET` chip visible) | §5 cluster-config flip shipped + verified in the §4 smoke test (run through the production frontend) before announcing |
| Accidental unauthorized/incorrect deploy (per `feedback_deploy_safety`) | Reinstall is the most destructive deploy — explicit operator permission required; stop→deploy→start dance; no autonomous deploy |
| Stale devnet PP (`tester1`, 2,700 PP) in the unified ledger | Left as-is; internal principal, leaderboard-exempt, ledger surgery not worth it |

## Open items for the implementation plan

1. **Exact `solRpcProvider` mainnet value** — confirm against the current deploy's devnet provider config (`#Default(#Devnet)` → `#Default(#Mainnet)`, or a specific provider variant if that's what's in use).
2. **`primeObserverCursors` semantics** — does it reset `solGameIdCursor` to 0 post-reinstall, or only advance? Determines whether a new shenanigans `adminSetSolGameIdCursor` setter is needed (§3).
3. **Exact reinstall command + init-arg literal** — the init-args record the canister was originally deployed with, swapped to mainnet values.
4. **Frontend Solana cluster-config location** — where the wallet-adapter network, the `DEVNET` chip, and explorer-link cluster params are set (§5); and whether the "now accepting SOL" landing surface is copy-only or needs an affordance/flag flip.

## Appendix: relevant skills / memory

- Skills: `solana-dev` (mainnet-beta tx/nonce reference), `migrating-motoko` (only if §3 needs a shenanigans stable-shape change), `cycles-management` (CycleOps verify), `musical-chairs-brand` (§5 copy), `icp-cli`/dfx (deploy).
- Memory: `feedback_deploy_safety` (reinstall gating), `project_shenanigans_deploy_lineage` (stop→deploy→start dance), `project_ponzi_math_sol_m1_state` (current canister/pool/nonce state), `sol_deposit_ttl_bug` (detection timer + module history), `reference_cycleops` (controller id).
