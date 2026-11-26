# Native Solana — M7 close lifecycle handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M6 handoff](native-solana-m6-handoff-20261014.md) (`c6fdaa7`). Local work only; nothing deployed, signed with live keys, funded, registered with ORAO or Squads, activated or merged.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `7b3f4f9` | docs(evidence): consistency fixes from the Codex M5/M6 handoff check |
| `8fdade5` | feat(program): M7 close lifecycle — candidates, ORAO draw, promotion, expiry, finalization, provider budget (IMPL-32) |
| `366df88` | fix(program): pin the 600 s promotion window and defer undeliverable owner refunds instead of forfeiting them (Codex M7 review; IMPL-42/43/44) |
| (this commit) | test(solana): M7 close-lifecycle LiteSVM suite with the captured ORAO executable (IMPL-33), M7 receipt, graph fragment and handoff (IMPL-34) |

## What M7 delivered

- **Program (IMPL-32, `8fdade5`)**: `close.rs` (`build_candidates(rows)`, `request_round_draw`, `capture_round_draw`, `apply_promotion`, `expire_promotion`, `finalize_round`, `increase_provider_budget(extra)`, `observe_request_fulfilled`, `withdraw_provider_return(amount)`, `reconcile_ops_cash_with_client`), `provider.rs` (ORAO VRF v2 CPI wrapper, `orao-solana-vrf-cb 0.4.0` with `default-features = false, features = ["cpi"]`), `sampling.rs` (protocol seed, 64-counter ordinal mapping, chained (owner, loss) candidate digest; fixture `solana/fixtures/sampling.json`); errors 6041–6049 appended. Coordinator correction while applying: the worker's SBF-only `anchor_lang::solana_program::hash` branch did not compile under cargo-build-sbf (no such facade in anchor-lang 0.32.1; host tests never compile that cfg) and was replaced by one `solana_sha256_hasher::hashv` path.
- **Codex M7 static review (IMPL-42/43/44, `366df88`)**: `deadline_seconds` pinned to 600; undeliverable owner refunds deferred to the owner (held in `locked_owner_remaining`, paid by permissionless `deliver_owner_refund`, event `OwnerRefundDelivered`, codes 6050/6051) instead of forfeited to `unallocated_excess`; `credit_is_deliverable := c + a ≥ rent minimum` per the Agave rent-state rule; supplement [`native-solana-m7-contract-supplement-20261014.md`](native-solana-m7-contract-supplement-20261014.md) (canonical digest, deadline pin, deferred-refund reading of §8.5 proposed for Codex review). Receipts `2026-10-14-m7-deadline-pin.json`, `2026-10-14-m7-owner-refund.json`.
- **Harness (IMPL-33, this commit)**: `src/close.rs` (~5,000 lines): 42 rows / 784 observations (676 successes, 108 exact-code rejections: InstructionError(1, ArithmeticOverflow) ×2, 0 ×1, 2006 ×2, 3010 ×1, 6001 ×15, 6004 ×2, 6005 ×23, 6006 ×2, 6007 ×2, 6011 ×3, 6019 ×1, 6028 ×2, 6029 ×12, 6035 ×1, 6041 ×1, 6042 ×3, 6043 ×2, 6044 ×2, 6045 ×13, 6046 ×1, 6048 ×11, 6049 ×3, InstructionError(1, InsufficientFunds) ×1, 6000 ×1, 6016 ×1) plus two host rows; the captured ORAO executable (`6d180b1e…`, hash enforced, provenance pinned) is loaded in LiteSVM, the Client registered by the G vault through the Squads stand-in, fulfilment driven with the Ed25519 precompile and ORAO `Fulfill` under one synthetic authority; `solana/fixtures/seed-vectors.json` pins the two cluster-domain seed vectors. Worker compile-checked only; the coordinator executed it: 43/47 on the first run, one fixture gap (fixture Series A Backers lacked the `User` a real `fund_series_a` creates; Anchor 3012 on claim) fixed, 47/47; the `undeliverable-destination` observation upgraded to assert the IMPL-43 deferral.
- **IMPL-34 (this commit)**: receipt `2026-10-14-m7-close-lifecycle.json` + `2026-10-14-m7-cli.json`, graph fragment, this handoff.

## ELF under test

`fc2c794058867a6c99df919e364dc2328da38669bc9da12194a968366960c6dd` (1,363,760 B; source set `e7e2ffcb…`, lock `77f859f9…`, before/after guard clean), IDL `11840ae0…` (36 instructions, 20 events, 52 errors), 244 program unit tests. Full harness: 141 library tests (94 prior + 47 close), `allocation_diff` 2, `owner_refund` 4, `payout_props` 7 (+1 ignored), `transfer_constraints` 2; CLI `all` exit 0 with 1,830 observations.

## Compute and packet (close suite successes, max)

| Instruction | CU | packet B | accounts | n |
| --- | --- | --- | --- | --- |
| `BuildCandidates` | 49,810 | 816 | 21 | 256 |
| `RequestRoundDraw` | — | — | — | 0 (not exercised inside the close suite; see tests/owner_refund.rs) |
| `CaptureRoundDraw` | 35,661 | 383 | 8 | 21 |
| `ApplyPromotion` | 71,164 | 681 | 17 | 13 |
| `ExpirePromotion` | 28,219 | 449 | 10 | 21 |
| `FinalizeRound` | 17,413 | 317 | 6 | 19 |
| `IncreaseProviderBudget` | 15,651 | 357 | 7 | 4 |
| `ObserveRequestFulfilled` | 30,863 | 382 | 8 | 19 |
| `WithdrawProviderReturn` | — | — | — | 0 (not exercised inside the close suite; see tests/owner_refund.rs) |
| `ReconcileOpsCashWithClient` | 14,948 | 316 | 6 | 6 |
| `DeliverOwnerRefund` | — | — | — | 0 (not exercised inside the close suite; see tests/owner_refund.rs) |

Widest close-suite transaction: 1,183 B / 32 accounts (`build_candidates_rejects_skipped_or_reordered_rows.nine_rows`), under the 1,232-byte legacy packet; every close observation fits. LiteSVM measurement, not a cluster measurement.

## Findings for Codex

1. **Pre-funded request PDA blocks the draw until the deadline (M7-F1, design; no fund loss).** The request PDA is derived from public inputs once the candidate set is sealed. The captured ORAO executable creates it with create_account, which fails (system Custom(0) AccountAlreadyInUse) when anyone has pre-funded the address; request_round_draw fails closed as designed (Collision / no alternate seed), so the promotion for that close can only expire Skipped(Deadline). Cost to a griefer on the cluster: the rent-exempt minimum for an empty account (890,880 lamports locally) per close, lost to the griefer; impact: that round's Series B promotion is skipped, the seed carries, every cash path (settlement, finalize, claims) completes, no lamports are lost by the project. The plan accepted fail-closed collisions; it did not price this vector. Options: accept and document (the promotion is a bonus draw, not owed cash; ~0.0009 SOL per grief); ask ORAO whether request creation can tolerate a pre-funded PDA (upstream; not ours); any seed-unpredictability change would reopen the accepted deterministic-seed design (not recommended here). No code change made.
2. **capture_round_draw records IgnoredLate on an already-terminal close before the terminal check (M7-F2, cosmetic).** close.rs capture_round_draw: a late capture on a terminal close sets draw_state = IGNORED_LATE (a harmless field write) whereas a timely capture on a terminal close returns PromotionTerminal; ordering only, no cash or state of consequence. Proposed for the M8 program pass.
3. **Error codes LateCapture 6027 and CandidatesIncomplete 6047 are declared but never raised (M7-F3).** Late capture is the IgnoredLate no-op; incomplete candidates expire with STAGE_CANDIDATES_INCOMPLETE. Frozen codes stay reserved.
4. **Treasury fee moves at request time, not at fulfillment (M7-F4, plan wording).** Probe receipts and the suite: request: Client −(fee+rent), request account +rent, Treasury +fee; fulfillment: request −excess rent, Client +excess rent. The plan sketch said "Treasury +300,000 at fulfillment".
5. **Deferred owner refund reading of §8.5 (IMPL-43)** awaits Codex acceptance: a settled close with `locked_owner_remaining > 0` is a deferred owner refund; alternative is a dedicated field (frozen-layout change). `deliver_owner_refund` is permissionless (destination immutable).

## Caveats a reviewer should know

1. One synthetic ORAO fulfil authority; no cluster latency, quorum or retention behaviour (the devnet gap stays open).
2. LiteSVM 0.7.1 does not check rent-state transitions for data-less accounts, so the deliverability predicate is validated against the Agave rule text (`tests/transfer_constraints.rs` pins the gap), not against local execution.
3. Rows that cannot be driven for real are labelled SYNTHETIC in the suite: entropy-exhausted (`preferred_count` rewritten to 0 on a captured close) and the counter-1 promotion (`randomness` from the pinned `sampling.json` vector); the terminal transitions themselves are the real program.
4. The composed `expire → finalize → open` transaction needs a `provision_page(Participant{2})` between finalize and open (participant pages exist per current round); the plan's three-instruction form runs with a fixture page.
5. The worktree-built and main-tree-built ELFs from the same source set are not byte-identical (same size, identical strings; hypothesis: cargo's path-derived metadata hash changes layout); only same-path rebuilds reproduce. The main-tree ELF is the artifact.
6. The Squads stand-in proves application guards only (M3).

## Proof level

GREEN on real lamports in LiteSVM for the whole close lifecycle with the captured ORAO executable: candidate build and seal, request bound by the protocol seed, fulfilment, timely/late capture, promotion (real draw and synthetic counter-1), expiry stages, finalization carrying the full seed once, provider budget increase, observation, authenticated returns with owner attribution, two-close sequence, and the Codex M7 corrections with RED→GREEN evidence; one synthetic authority and no cluster behaviour; no deployment or live action.

## Staffing

Coordinator (Claude Fable 5.1): applied IMPL-32, hasher fix, IMPL-42/43/44 (isolated worktree, RED→GREEN), first execution and two fixes of the IMPL-33 suite, receipts, graph. Sonnet workers: IMPL-32 (program, isolated worktree), IMPL-33 (suite, compile-checked). Codex: M7 static review at `8fdade5` (two corrections, one docs note), M5/M6 handoff check.

## Solana notes

- A VRF request on Solana is two transactions by design: the program asks the provider (CPI into ORAO, paying fee + request rent from the locked reserve) and a provider-side fulfilment later writes the randomness into the request account; the program only ever reads it back (`capture_round_draw`). The ICP analogue is `raw_rand` in one call; here the gap is a real waiting state with a deadline, which is why expiry is its own instruction.
- The request address is a PDA of public inputs, so anyone can pre-fund it; the system program's `create_account` then refuses (M7-F1). This has no ICP analogue and is a property of the account model, not of the program.
- Rent-exemption is enforced by the runtime per transaction: a transfer that would leave a wallet rent-paying fails the whole transaction. A payout or refund destination is therefore a runtime constraint on cash movement (IMPL-43), unlike ICP ledger transfers which never fail on the recipient's balance.

## Next

M8 (IMPL-36..38): reserve/locked partitions, §8.6 conservation table rows incl. the deferred-refund rows, cleanup predicates, the 300-step randomized sequence; fold M7-F2 into the M8 program pass. Then M9 (IMPL-39) measurement and M10 (IMPL-40) security.
