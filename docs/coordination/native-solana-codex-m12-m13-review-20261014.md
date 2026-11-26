# Codex M11–M13 independent local review

Candidate: `3508b1a29a734eb3a9b8a13c07168549bccd06ff`, `codex/native-solana-production-core`. Rob explicitly resumed local review coordination in this Codex chat after the earlier pause. The heartbeat remains PAUSED. No public-network deployment, live key access, signing/broadcast/funding, registration, governance activation, production ICP/frontend changes, service activation, legacy shutdown or PR merge is authorized. Claude remains the sole coordinator/implementation dispatcher and build executor; canonical board `docs/coordination/kanban.md`. Preserve all dirty/untracked files and other worktrees.

## Verdict and verified evidence

**M11 corrected packet accepted at its stated local boundary. M12/M13 need the local corrections R1–R5 below before independent acceptance.**

- Root read GitHub Actions run [38683877850](https://github.com/SchemeWorks/MusicalChairs/actions/runs/38683877850), exact head `f9cbcc5ebab80e2d575e609222ebaf8f809a7886`: all four jobs successful, including keeper-worker and production SBF/provenance/LiteSVM. No duplicate build/test/CI run by Codex. Later commits are graph/evidence documentation; this CI does not establish Motoko actor runtime correctness.
- All 53 published M11 screenshot manifest hashes verified. Root inspected the run3 unknown-to-finalized same-signature pair and verifying-state capture, plus prior financial-flow captures. The README correctly distinguishes synthetic Clock/genesis and local oracle execution from live ORAO-network proof. Production legacy forms have static-render proof, not a browser/mainnet-session claim; no public-network action is required to close this disclosed M11 limit.
- Graph 20,696 nodes / 31,163 raw directed links / 49 hyperedges independently preserves the saved M11 graph's node IDs, canonical raw directed edge identity multiplicities and hyperedges; zero duplicate IDs/dangling edges. All 48 coverage hashes match receipt head `f9cbcc5`. These are dated scoped pins, not blanket current-source freshness.
- Replica receipt SHA256 `eafb6247185d8e5c0888e55f85487d6d2d59860774c62dd6eeef27cf0d053a3c` matches the coordinator receipt and records 115/115 local checks using the real local PP ledger/SIWS provider and an RPC stub. This closes the earlier pending actor-integration evidence at its stated local boundary. Real RPC wasm, actual archive/transfer-block/wrong-deployment runs and exact concurrent await interleaving remain explicitly unproven.
- Existing resource reviewer independently inspected M12 Shenanigans; existing bridge reviewer independently inspected M13. Both were read-only, with no builds/tests/network/UI/mutations. Root confirmed the concrete findings against current source and the frozen contract.

## R1 — Shared signup gift race (P1)

`shenanigans/main.mo:1473–1497` checks the shared marker, awaits `mintWithEffects` at 1489, then writes the marker at 1496. Native binding can reserve its own gift during that await; both gifts can commit. The marker-map pure test does not exercise that actor interleave. Accepted spec §9 lines 441/444 and plan M12 Step 1 explicitly require native→legacy AND legacy→native one-gift safety.

Codex adjudication: the explicit exactly-once invariant controls this narrow conflict with spec line 445's broad “legacy observers unchanged” wording. Implement the smallest reviewed shared gift reservation repair, with an explicit failure/retry policy and a controlled actor-level interleave regression. A one-line early marker without failed-mint recovery is not a complete repair. Preserve legacy economic terms and unrelated mint paths. This is local source/test authority only; no production canister upgrade.

## R2 — Initial reward execution stops at Reserved (P1 integration gap)

`native_sol_bridge/main.mo:61–65` declares only observe/bind on Shenanigans; `releasePending` at 300 never executes reserved legs. `shenanigans/main.mo:6188–6192` permits only bridge-allowlist/admin initial execution. The replica driver at 519–523 used admin for every initial leg; public retry cannot start Pending. Spec §9 line 442 calls for bridge or keeper execution.

Codex choice: bounded bridge-driven execution using its existing authorization, preferably one frozen reserved leg per explicit call. Do not accept caller-supplied recipient, amount or arbitrary bytes. Retain idempotent status/retry/reconcile and admin fallback. Prove a non-admin caller can drive observe→bind→execute→receipt locally. Do not silently put an unrelated principal in the observation allowlist or activate a service.

## R3 — Verified history API and frontend incomplete (P2)

`native_sol_bridge/main.mo:360–363` calls `Outbox.recent`; `native_sol_bridge/Outbox.mo:177–187` includes Pending/Unverified/Invalid intake in the API labelled verified history. `frontend/src/hooks/useNativeHistory.ts` is absent and no equivalent bridge-history consumer was found. Spec §9 line 446 requires verified history/feeds; M12 explicitly names the hook.

Filter public native history to verified records; keep intake/audit statuses separate. Test pending/invalid exclusion and wire native history/feed alongside clearly separated legacy records. Keep financial reads on Solana.

## R4 — Primary keeper omits accepted operations (P2)

Spec §10 line 454 names `increase_provider_budget`, `reconcile_ops_cash` and cleanup. The worker StepKind/builders in `solana/keepers/worker/src/plan.rs:127–184` omit these operations; the fee guard at 1209–1228 lacks their discriminators. The runbook delegates them to the primary or a wallet, but the primary cannot construct them.

Add scoped typed operations, required account/predicate reads and focused regressions. Preserve fee-only signing, unknown-outcome rules, page identities and recorded refund destinations. Cleanup means accepted on-chain shared-account cleanup; no file/worktree/ledger deletion authority.

## R5 — Watchdog negative evidence invalid (P2 evidence)

`docs/solana/evidence/2026-10-14-m13-keepers.json:69` records missing-module M0009 imports as RED. Go-ahead line 18 explicitly rejects compile/fixture failures as RED. Supply meaningful assertion-level negative controls with valid modules for stale-sign timing, timer re-arm and prohibited authority signing, then preserve GREEN evidence. Label later mutations honestly as negative controls, not historical failing-test-first development.

Keep actor await/timer/threshold signing explicitly unproven until exercised. If local threshold signing is unavailable, preserve that boundary; no public signing or alternate-executor bypass.

## Other decisions

- PromotionApplied: retain Backer PP/SOL for the B-loss basis under frozen policy (currently 120,000 PP/SOL), not a new economics proposal.
- SIWS marker timing: resolve the principal first, then re-read and atomically reserve marker/effects/legs before any ledger await. This matches spec §9; it does not repair R1's separate legacy await gap.
- Aged Pending legs: retain quarantine and frozen timestamps. No re-stamp/remint exception. New recovery mechanisms are optional future scope.
- Native-only uplines: disclose that native events do not update the legacy activity watermark. Preserve accepted read-only referral/effect-table reuse; no silent eligibility change.
- Memo heuristic: canonical 32-byte native memos make the observed overlong-memo trap unreachable on the supported path. Record the ledger-specific limit and retain Unknown/reconcile handling. No broad ledger redesign gate.
- Decoder: v0 is the highest actually supported transaction version; unsupported versions remain gaps. Checking decoded plan/kind prevents relayer-selected economics and is appropriate.
- Invalid intake/cursor: a provably false descriptor must not permanently squat its event ID; corrected submissions remain admissible. Treat Invalid as an intake/audit terminal state, explicitly distinct from the verified-delivery checkpoint requiring Observed/AlreadyObserved or durable permanent Shenanigans rejection. Clarify API/receipt semantics with R3; never advertise invalid intake as verified delivery.
- Watchdog finalized reads and configurable prune defaults are disclosed operational choices, not blockers. Align prune settings at bring-up. Real RPC cycle/latency calibration belongs to separately authorized M14.

## Delivery

Use owned correction cards on the existing board. Return coherent commits, focused RED/negative-control→GREEN evidence, exact local receipt boundaries, closure of each R item, and normal scoped graph refresh preserving prior raw identities/multiplicities/hyperedges. Include this review document in the appropriate review/evidence commit; do not commit `native-solana-supervision.md`. Request independent focused closure review before calling M12/M13 accepted. No plan reopening or routine Rob permission gate for these already-authorized local repairs. Stop before M14.
