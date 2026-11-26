# Codex R1–R5 closure review, first pass (2026-10-15)

Candidate: `883e044e87fee5107212a809dbbe5fc5b391fb5e`. Review scope: committed fixes after `3508b1a`, preserving coordinator-owned dirty replica driver/receipts and all other work. Heartbeat independently confirmed PAUSED. No deployment, signing, funding, activation or merge. Two fresh native GPT-5.6 Luna reviewers were used only for bounded read-only source/evidence review; no competing implementation/build executor.

## Verdict

R4 and R5 independently PASS at their stated local source/test boundaries. R1's nominal shared-reservation repair is useful but the requested controlled actor-level proof is still missing. R2 source PASS; the draft non-admin replica run needs its stale expectation corrected/documented. R3 needs the program-ID guard below. Full M12/M13 acceptance is not yet granted.

### R1 — controlled actor-level interleave/failure proof still required

The source now reserves the gift synchronously on both paths before the ledger await, and the 3,190-state enumeration covers the pure reservation protocol. However `shenanigans/test/replica/native-rewards.md:113–152` explicitly labels the real-actor G1–G6 scenarios unrun. The original review R1 specifically required a controlled actor-level interleave regression, not only a model that manually applies transitions.

Complete the smallest local fixture that observes the actual legacy reservation before its ledger await resolves, interleaves native binding, resumes the original actor, and counts one gift on the ledger; include the failed-mint/retry/takeover policy and durable reservation across the relevant boundary. A deterministic test latch/instrumented fixture is preferable to an unobserved timing race. Preserve production authority and existing local state; no public call or broad unrelated legacy repair.

Adjudication of a reviewer hypothesis: a generic caught await failure becoming legacy Failed could warrant a conservative distinction, but this pass did NOT establish an actual post-commit caught rejection against the pinned ledger. Do not report it as a proven P1 or require a broad redesign solely on that hypothesis. Plain Motoko await is unbounded by default; SYS_UNKNOWN belongs to bounded/best-effort calls ([ICP call documentation](https://docs.internetcomputer.org/guides/canister-calls/inter-canister-calls/), [message execution properties](https://docs.internetcomputer.org/references/message-execution-properties/)). The concrete remaining acceptance item is the requested actor proof, with observed failure classification stated honestly.

### R2 — source PASS; draft replica has one stale expectation

`executeReservedLeg` preserves one-leg-per-call, anonymous refusal, Forwarded+Bound gating, cooldown before await, event/index-only forwarding and committed-record protection. Frozen financial fields stay in Shenanigans.

Observed draft `native_sol_bridge/test/replica/receipt-2026-10-14-v2.json` hash `edde5567ea051b3b89ef98187992ca92b273dc2811da209ee5323435edf98865`, run 2026-10-15T07:15:26Z–07:25:36Z: 134 checks, 133 pass, only COOL-1 fails. That assertion expects the OLD bind cooldown after successful forward; actual reply is a successful Unbound result, consistent with the already accepted cooldown-clearing repair. Thus this failure is a stale test oracle, not evidence of a new protocol defect. GIFT-1/2 pass, and a non-admin identity is recorded for execution.

Correct the oracle and stale finding prose; retain the original 133/134 receipt honestly. A focused rerun/supplement for the corrected assertion is sufficient if the other successful exact-source cases remain valid; do not relabel the failed run as historically all-green or repeat the whole financial run merely for its count. Publish the coherent final driver/receipt/source pins for independent runtime closure.

### R3 — same cluster, wrong program can pass the history guard (P2)

`frontend/src/hooks/useNativeHistory.ts:184–218` represents/checks only the expected cluster domain. `readBridgeDomain` at 248–253 discards `getConfig().program_id`; query/cache keys at 342–358 omit expected program identity. A bridge for another program on the same cluster passes and can display unrelated verified events as this application's history. Spec §9 line 436 includes program_id in event identity; §11 line 467 requires cluster+programId cache identity.

Compare the bridge's program_id with `identity.cluster.programId`, fail closed with a clear program mismatch, bind config/history cache keys to expected program as well as cluster/host/canister, and reject mismatching record program IDs defensively. Add same-domain/wrong-program and identity-switch/cache regressions; financial reads remain Solana-only. Backend verified-vs-intake filtering, native/legacy separation and cluster refusal behavior otherwise PASS. Invalid intake remains explicitly distinct from verified delivery as previously adjudicated; no new cursor redesign gate.

### R4 — PASS, local source/MockChain/CI boundary

Typed provider-budget, reconciliation and cleanup operations exist. Sponsor/owner reserve accounting, opt-in sponsor-only top-up, page identity/close/round/refund/deliverability/protected-account checks and fee-only signing are consistent with the on-chain predicates. Unknown journal entries remain blocked; replay uses original bytes. Root independently read green existing GitHub run [38696249369](https://github.com/SchemeWorks/MusicalChairs/actions/runs/38696249369) at `88f30d0342253d7b313367e57d45f39e50a32c77`. MockChain tests are not live service proof.

### R5 — PASS, meaningful negative controls

The nine valid API-compatible wrong modules fail assertions for timing/rearm/prohibited authority, while retaining the real test assertions; compile/import failures are disallowed. The old RED claim is explicitly withdrawn and later experiments labelled negative controls. Root ran the read-only `python3 -I native_sol_keeper/test/negative/negative.py check`: generated files current, assertions identical, API complete. No compiler/build/runtime suite was duplicated. Actor awaits, real timer re-arm and threshold signing remain explicitly unproven as before.

## Delivery

Route only these focused closure items through the existing Claude coordinator and canonical cards. Preserve R4/R5 acceptance; no broad re-review/build. Complete scoped graph/evidence close-out after coherent corrections, preserving all prior node IDs/raw directed edge identities/multiplicities/hyperedges. No new coordinator/heartbeat or M14 authority.
