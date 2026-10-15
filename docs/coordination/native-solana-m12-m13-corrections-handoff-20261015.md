# Native Solana M12/M13 corrections handoff — Codex R1–R5 and the first closure review (2026-10-15)

Branch `codex/native-solana-production-core`. Receipt: `docs/solana/evidence/2026-10-15-m12-m13-corrections.json`.
Reviews adopted as received: `docs/coordination/native-solana-codex-m12-m13-review-20261014.md` (sha256 b75c983f…2404f5c, in 1272b84) and
`docs/coordination/native-solana-r1-r5-closure-review-20261015.md` (sha256 0b6d2284…2bc7b52, in 30e583f). Cards IMPL-49a–h on the
canonical board. Everything is local: standalone `moc -r` suites under moc 1.5.1 and dfx's 0.16.2, MockChain keeper tests, static
renders, the local dfx replica with an RPC stub, the real local PP ledger and the real local ic-siws provider. No public-network action,
no deployment, no live keys, no merge. The only public-network touch was one anonymous read-only `icrc1_metadata` query of the mainnet
PP ledger (see the finding). M14 remains the authorization gate.

## Commits (oldest first)

| commit | what |
| --- | --- |
| `1272b84` | review adopted; cards IMPL-49a–f |
| `9bb3c61` | R5: nine valid wrong-variant negative controls for the watchdog; M13 receipt RED claim withdrawn (IMPL-49e) |
| `f9cc51e` | R3 frontend part 1: useNativeHistory + NativeHistoryFeed beside the labeled legacy ledger (IMPL-49c) |
| `88f30d0` | R4: typed increase_provider_budget / reconcile_ops_cash(+with_client) / close_shared_account in the primary keeper; runbook (IMPL-49d) |
| `f0a67ab` | R2 + R3 backend: executeReservedLeg, getLegStatuses, verified vs intake history pages; dfx-build regression of ca00bbc fixed (IMPL-49b) |
| `fc91677` | R3 frontend part 2: declarations re-synced; bridge cluster_domain guard (IMPL-49c) |
| `883e044` | R1: shared signup-gift reservation across both paths; 3,190-interleaving regression with a labeled negative control (IMPL-49a) |
| `30e583f` | first closure review adopted; cards IMPL-49g/49h; stray bytecode cache untracked |
| `9f86ec5` | R3 (closure): program-id guard and cache identity program + cluster + host + canister (IMPL-49g) |
| `d43a462` | R2 (closure): replica v2 non-admin execution proof; stale COOL-1 oracle corrected with a focused supplement (IMPL-49b) |
| `d20f1bb` | R1 (closure): controlled actor-level interleave proof, G1–G6 run on the local replica (IMPL-49h) |

## Closure per item

- **R1 — shared signup-gift race.** One reservation book (`signupGiftReservations`, one added stable var, stable-compatible) claimed synchronously before any ledger await on both paths; the legacy path changed by exactly three tagged lines and is re-pinned; explicit policy (same-owner retry after Failed, native takeover of a Failed legacy attempt, never while the other owner holds Reserved/Committed; a quarantined native gift leg keeps the reservation). Pure enumeration: 3,190 interleavings, exactly one gift in each; negative control (post-hoc, labeled): the old claim-after-await ordering yields two gifts in 1,452 of them. Actor-level proof (IMPL-49h): G1–G6 run on the local replica, 91 checks / 0 failed — the real shenanigans wasm, the real pp_ledger and the real siws_provider with a legacy-games stub (determinism by stopping/starting the ledger), plus a latch-ledger build (byte-identical sources except the ledger-id constant) for the deterministic held await: Reserved{legacy} observed while the tick was pending, native bind inside the window got no gift, exactly one gift after release; failed legacy mint → native takeover on the real ledger; legacy retry wins (stub); native Reserved blocks legacy (real); durability across identical-wasm upgrades including with the await held. Observed classification: plain unbounded awaits; caught rejects → Failed; native leg vs stopped ledger → SubmittedUnknown; no SYS_UNKNOWN observed or claimed; a caught reject after an already-committed mint was not produced. Not proven: the held-await result rests on the latch stub; an upgrade to a different module version with an await in flight; the enumeration is a model, not actor wiring.
- **R2 — bridge-driven execution.** Permissionless one-leg-per-call `executeReservedLeg` through the bridge's own allowlist (no caller recipient/amount/bytes; Forwarded AND bound; Committed answered from the bridge record; admin fallback retained). Replica v2: 154 checks / 153 pass; the one failure COOL-1 is a stale oracle expecting the old post-forward cooldown (receipt kept as written; corrected assertion in the driver; focused supplement 2/2). The throwaway non-admin identity (never a controller, relayer or allowlist member) executed every initial leg → real mints at ledger blocks 30–33, including a SubmittedUnknown leg committed by the identical call after an upgrade; live negative controls; verified vs intake pages at four stages; four upgrade points.
- **R3 — verified history.** Backend: verified page vs intake page partition the store; Invalid is intake-terminal, not verified delivery. Frontend: `useNativeHistory` calls exactly `getConfig` + `getRecentEvents`, drops intake rows defensively, refuses on cluster_domain OR program_id mismatch with cache identity (host, canister, cluster domain, program id), counts record-level program mismatches; the feed sits beside the labeled legacy ledger; 514 vitest, tsc clean; static renders only (no browser run).
- **R4 — keeper operations.** PASS per the closure review (MockChain + green CI 38696249369 at 88f30d0).
- **R5 — watchdog negative controls.** PASS per the closure review.

## Finding disclosed (pre-existing legacy, not changed)

The legacy mint path sends the event id as the memo unchanged. `signup-<principal>` is 70 bytes and `backer-<principal>-<amount>` is longer; the local PP ledger enforces `max_memo_length = 32` (IC0503 trap, observed for the gift in IMPL-49h), and an anonymous read-only `icrc1_metadata` query shows the MAINNET PP ledger advertises 32 as well. Game-deposit memos (`game-<id>`) are short. Consequence: on such a ledger legacy signup gifts and legacy backer mints cannot commit (the backer case is inferred from source, not observed). Recommended read-only follow-up for Rob/Codex: the mainnet Shenanigans missed-mint counters. Any legacy repair (for example a hashed 32-byte memo as the native path uses) is a separate authorized change.

## Open items

- Release path for a permanently quarantined native gift leg (reviewed recovery; optional future scope).
- The legacy memo finding above.
- The replica keeps test artifacts (three stub canisters, synthetic entries); `run.sh` reinstalls the three M12 canisters at its next start.

## Next

Independent focused closure review of R1 (actor proof), R2 (replica proof), R3 (program-id guard) by Codex; R4/R5 preserved. Then the coherent graph close-out (done in the same evidence commit) and M12/M13 acceptance. Stop before M14.
