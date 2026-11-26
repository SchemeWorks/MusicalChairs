# Native Solana M13 handoff — keepers and monitoring, code only, nothing started (2026-10-14)

Branch `codex/native-solana-production-core`. Receipt: `docs/solana/evidence/2026-10-14-m13-keepers.json`.
Kanban: IMPL-48a (Rust worker), IMPL-48b (ICP watchdog + runbook), IMPL-48c (close-out).

## Commits

| commit | what |
| --- | --- |
| `a93d4d0` | feat(keepers): fee-only keeper worker CLI — planner, journal, metrics; no services started (IMPL-48a); CI job `keeper-worker` |
| `58bd446` | feat(keepers): ICP watchdog canister `native_sol_keeper` and `docs/solana/operations.md`; no services started (IMPL-48b) |

## What M13 delivered

1. **`solana/keepers/worker`** (Rust, the harness pins, own workspace, excluded from the root workspace like the harness): `mc-keeper once|run` with ANY ed25519 fee-payer keypair; no other key, no transfer, no lease. Pure planner in the reference order (ticket phases Discover → Senior → Seal → Allocate → Prune → Commit, then the closing pipeline: wait for the cash commit; apply a captured draw even past the deadline; expire past the deadline; candidate pages for the preferred then fallback set funded from the close's own reserve; build_candidates aligned to the participant AND both candidate-set edges; request with the program's protocol seed / request binding; capture when fulfilled, Blocked while pending; finalize once the promotion is terminal and the reserve settled; then idle work: opt-in CloseRecord pre-provisioning, deferred owner refund only when the destination passes the program's `credit_is_deliverable` with the live Rent sysvar, observe a fulfilled request once). Batches cut downward at page edges, cap 8, one-row fallback when the 1.4M-CU simulation exceeds the step's own limit. Fee-only guard before every signature (ComputeBudget + this program, 16 keeper discriminators, the payer the only signer in at most one writable slot, exactly one required signature). Append-only journal with the signed bytes: write-ahead `submitted`, then `unknown`/`failed`; an unresolved entry blocks the step; unseen-but-valid entries are re-broadcast byte-identical with no new line; only `expired` (block height past last-valid) allows a fresh blockhash. Metrics: on-chain progress markers, blocked-vs-idle, 60 s alert / 300 s escalation, JSON report.
2. **`native_sol_keeper`** (Motoko persistent actor, dfx.json entry, built and never deployed): recurring timer re-armed in `postupgrade` behind a stable flag; fee-only threshold Ed25519 key via the management canister (`schnorr_public_key` / `sign_with_schnorr`, `#ed25519`, signatures taken from the interface embedded in dfx 0.29.2), derived once; stale-progress policy (alert 60 s, sign only after 300 s, never with an unresolved unknown entry, never unconfigured; own landed steps do not reset the clock); journal with the same rules as the worker; reads at `finalized` through the SOL RPC canister with exact pinned types (`didc check` clean with negative controls); step builder for the six ticket phases, `expire_promotion` and `finalize_round` with the same batching rule; `provision_page`, `build_candidates`, `request_round_draw`, `capture_round_draw`, `apply_promotion` are `#Blocked "unsupported step"`. No public method takes bytes to sign. Caps: fee-address floor and hourly signature budget. `AuthorityTable.mo` generated from the program IDL; decoders checked against IDL-serialized fixtures; PDAs and on-curve vectors checked against @solana/web3.js offline; the planner reproduces the seven real localnet transactions in `solana/fixtures/rpc`.
3. **`docs/solana/operations.md`**: roles (external primary worker, ICP watchdog backup), what 60 s / 300 s mean, escalation from any wallet, both keepers' metrics fields, fee floor and hourly budget, the not-automated list, bring-up, no credentials.

## Proof level

Worker: RED recorded by the coordinator on the phase-1 stubs (0 passed / 3 failed at the two `unimplemented!` sites) → GREEN 13 passed / 0 failed on the first execution of phase 2; fmt and clippy (`-D warnings`) clean; Linux CI job added. Watchdog: eight standalone `moc -r` suites (392 checks) ALL PASS identically under moc 1.5.1 and dfx's 0.16.2, RED first as M0009 import errors; `moc --check` and full wasm builds clean under both. Everything is local; no service runs; the real RPC backend, the actor's await paths, the timer re-arm on a replica and threshold Ed25519 on a local replica are NOT proven (the replica plan `native_sol_keeper/test/replica/native-sol-keeper.md` is unrun).

## Decisions recorded for Codex

- Worker: `next_step` returns `Result<Option<Step>, PlanError>` (a missing page is an error, not silence); step key excludes `rows` so a one-row retry stays blocked behind an unknown 8-row attempt; CloseRecord pre-provisioning is opt-in and off; a missing record inside the closing pipeline is a `PlanError` (the closing payout binds an already-provisioned record on chain); past the deadline a close with a fulfilled-but-uncaptured request expires (the reference order); page pre-provisioning is not planned.
- Watchdog: a JSON-RPC error object on send is journaled Failed (mirrors the adapter's `classifySendError`), every other post-send error Unknown; `finalized` reads (≈40 s blockhash validity; a `confirmed` option may follow M14 measurement); `pruneNewlyCapped` defaults false while the worker's `--skip-newly-capped` defaults off (both configurable — align if Codex prefers); idle polling adapts up to 300 s; `configTemplate` defaults to Equality consensus on `#Default(#Devnet)` (threshold 2-of-3 needs devnet evidence); `rpcCycles` / `signCycles` placeholders to calibrate at M14; the pause conditions in operations.md step 4 are a proposed policy.

## Staffing

Coordinator: briefs, RED/GREEN runs, clippy/format repairs, CI job, verification, receipts, handoff, graph. Two Sonnet workers (48a Rust, 48b Motoko + runbook).

## Next

M14 devnet acceptance is an AUTHORIZATION GATE (Rob): nothing in M13 starts a service, funds a key or touches a public network.
