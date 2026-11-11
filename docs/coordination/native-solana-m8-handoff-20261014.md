# Native Solana — M8 reserve, refund and cleanup handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M7 handoff](native-solana-m7-handoff-20261014.md) (`276b6a3`) and the Codex-driven corrections `366df88`, `5f19cd2`, `565c1e9`. Local work only; nothing deployed, signed with live keys, funded, registered, activated or merged.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `3e26f20` | fix(program): M8 program pass — capture on a terminal close is PromotionTerminal; settled closes lock no further pages (IMPL-36) |
| (this commit) | test(solana): M8 reserve/refund/cleanup suite and 300-step randomized conservation (IMPL-37), M8 receipt, graph fragment and handoff (IMPL-38) |

## What M8 delivered

- **Program pass (IMPL-36, `3e26f20`, isolated worktree)**: every plan M8 row holds at the program level (per-row table in the receipt). Two failing-test-first changes: **M7-F2** — `capture_round_draw` is now `capture_gate` + `book_capture` with `require_pending` ahead of the deadline branch, so a capture on an already-terminal close is `PromotionTerminal` whether timely, at the deadline or late, and writes nothing; **G1** — `locked_permission` requires `!close.reserve_settled`, so a settled close's deferred owner refund (IMPL-43) can never be spent as LockedClose page rent before finalization (`LockNotPermitted`). 243 → 248 unit tests; no ABI, IDL, layout or error-code change.
- **Reserve suite (IMPL-37, this commit)**: `src/reserve.rs` — 19 plan rows / 448 observations (376 successes, 72 exact-code rejections with fee-only rollback), including the spec §8.6 conservation-transitions table row by row (with the deferred-settlement and `deliver_owner_refund` rows added by IMPL-43/45) and the R3 identity-bound cleanup predicates driven through real closes. Rows:
- `all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds` — HOLDS — create_shared_locked → charge_locked never touches free_balance (runtime part in the reserve suite)
- `candidate_pages_closable_only_after_their_own_close_is_terminal` — HOLDS — closable requires canonical record, close_id match and promotion != Pending; handler checks decoded close_id/set_tag and derived PDA
- `cleanup_is_never_required_for_commit_finalize_or_open` — HOLDS — same account-set argument; a new round's participant page is provisioned, not cleaned up
- `client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return` — HOLDS — book_observation touches only eligibility counters; observe_client records the Client balance; sync_ops classifies other credits as excess; only withdraw_provider
- `conservation_transitions_table_8_6_holds_for_every_row` — HOLDS, coverage added — pure Model test locked_total_equals_the_remaining_partitions_after_every_transition + NEW the_identity_holds_through_an_undeliverable_settlement_a
- `full_cash_progress_with_depleted_free_ops_funds` — HOLDS — CommitTicket/ClaimBacker/FinalizeRound carry no Ops/OpsCash account; book_finalize reads only cash_committed and reserve_settled (deferred refund counts as settle
- `missing_b_account_is_created_from_locked_reserve_with_ops_free_zero` — HOLDS — apply_winner → create_locked_account → book_locked_rent(Spend::BAccount); apply_promotion never reads free_balance; promotion_holdback keeps B/registry rent in th
- `old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts` — HOLDS — snapshot pages never closable regardless of tag/ticket state
- `owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution` — HOLDS — plan_request_spend (fee then rent, sponsor first) → book_request (spent.provider_fee_s/o, request_rent_s/o, request_rent_funded_s/o); consumed fee only in provide
- `participant_pages_closable_only_after_their_round_finalized` — HOLDS — closable requires canonical record, round match and finalized; handler checks decoded round_id and derived PDA
- `registry_tail_full_extension_page_created_from_locked_reserve` — HOLDS — append_to_registry (Spend::RegistryPage), registry_extension_required in bind_close_record, holdback, extension term in required_close_reserve; unit a_registry_ta
- `required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser` — HOLDS — state::required_close_reserve sums per-set missing pages with saturating_sub; unit close_reserve_counts_one_page_per_set_for_eight_participants, closing_reserve_s
- `reserve_short_by_one_lamport_rejects_closing_payout_before_mutation` — HOLDS — positions::lock_split: sponsor = min(R, free), owner = min(R − sponsor, max_work_contribution), sponsor + owner == R else ReserveUnavailable; payout_preflight rea
- `snapshot_and_registry_pages_are_not_closable` — HOLDS — controls::closable false for both; close_shared_account arm returns NotClosable; unit closability_matrix_per_kind_and_predicate
- `spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation` — HOLDS — required comes only from schedule/pool/record counts, never free_balance; lock_split(2,5,0) == (2,0); only lock.sponsor leaves free_balance
- `sponsor_partition_consumed_first_then_owner` — HOLDS — lib::split_locked, charge_locked, close::book_locked_rent compute-then-assign, exact _s/_o sources, O(1) locked_remaining_total == s + o; unit locked_split_pays_s
- `sponsor_withdrawal_cannot_touch_locked` — HOLDS — withdraw_operations requires amount <= free_balance and lamports >= rent_floor + L + amount; fund_operations sync_ops then free_balance/total_contributed (harness
- `uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages` — HOLDS — per-set saturating_sub; unit close_reserve_never_lets_one_over_provisioned_set_offset_the_other, over_provisioned_candidate_sets_never_offset_each_other
- `unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination` — HOLDS + G1 FIXED — settle_books guarded by reserve_settled; sponsor remainder → free; owner remainder paid or deferred (never excess); book_delivery once; NEW: locked_per
- **Randomized conservation (IMPL-37)**: `src/fuzz.rs` — 300 steps, seed `0x4D43_4655_5A5A_3038 (MCFUZZ08), xorshift64*`; coverage sweep every 5th step before step 215 (least-attempted family); otherwise 52% progress-engine advance, 30% perturbation, 18% adversarial draws (15 kinds); advance forced when behind schedule; close 1 full draw path (request, fulfil, observe, G-signed provider return, apply; owner refund deferred by draining the destination in the settling transaction, OWNER_PART 400,000); close 2 expiry; families: 32 plus Aux, incl. provision_page Free|Caller|LockedClose and fund/withdraw_operations (sponsor-signed / permissionless as the program defines them). Per-step invariants: both conservation identities and L == Σ(s+o); terminal ⇔ reserve_settled; remaining ≤ required_reserve; Σ eligible_return_remaining == ops.eligible_return_budget; outstanding request rent == funded rent of unobserved requests. Coverage asserted: ≥2 finalized rounds, ≥1 committed ticket, ≥1 paid claim, ≥1 provider return, ≥1 delivered deferred refund, ≥1 booked deferral, all 32 families attempted. 285 observations.
- **First execution**: 20 of 22 reserve_tests passed on the first run (fuzz excluded); the two failures shared one cause. full_cash_progress_with_depleted_free_ops_funds: 19 history-free losers fill three candidate pages per set and the row built in batches of 4, so a batch straddled a candidate-page edge; one build_candidates call carries ONE page per set and the program reports the straddle as PageNotProvisioned 6035 (close.rs build_batch, `page.page == index`); fixed by building one participant per call (coordinator); the aggregate test failed only through that row 1 passed in 1.66 s (300 steps; coverage counters asserted inside the test; observation set recorded from the CLI fuzz suite in the receipt)

## ELF under test

`d7f508ba3ae2c6a29af3ec5fa23f0469937c554d2a45be41a815080fe0ae4f07` (source set `9e33c98b…`, lock `77f859f9…`, guard clean); IDL unchanged since M7 (`11840ae0…`, check-idl ok); 248 program unit tests. Full harness: ok. 165 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 131.89s; CLI `all` exit 0 with 2,578 observations; every M8 observation fits the legacy packet: True.

## Compute and packet (M8 suites, successes, max; top instructions)

| Instruction | CU | packet B | accounts | n |
| --- | --- | --- | --- | --- |
| `ApplyPromotion` | 73,184 | 794 | 18 | 17 |
| `WithdrawSimple` | 67,559 | 760 | 17 | 38 |
| `OpenPosition` | 61,733 | 674 | 14 | 30 |
| `BuildCandidates` | 50,383 | 816 | 21 | 125 |
| `FundSeriesA` | 43,300 | 628 | 13 | 1 |
| `AllocateRows` | 42,501 | 571 | 13 | 13 |
| `DiscoverRows` | 41,029 | 644 | 15 | 13 |
| `ProvisionPage` | 36,628 | 498 | 9 | 90 |
| `CaptureRoundDraw` | 29,742 | 383 | 8 | 17 |
| `ExpirePromotion` | 28,277 | 449 | 10 | 13 |
| `ClaimBacker` | 28,173 | 514 | 10 | 4 |
| `ProvisionCloseRecord` | 27,209 | 392 | 8 | 5 |
| `ObserveRequestFulfilled` | 26,421 | 382 | 8 | 5 |
| `DeliverOwnerRefund` | 25,913 | 529 | 10 | 5 |

## Findings for Codex

1. **close_shared_account owner rent component has no deliverability preflight (M8-F1, design (no fund loss)).** Both M8 workers flagged independently: the owner component of a page's rent is credited to the recorded owner_refund destination with validate_destination only; a sub-rent-exempt component to a fresh or rent-paying destination would fail the transaction under the cluster rent rule (LiteSVM cannot show it for data-less accounts). Consequence: that page (and its sponsor component) stays unclosable until the destination is funded; no lamports are lost and cleanup is never a precondition for commit, finalize or open. Options: (a) accept and document; (b) defer the owner component into the settled close's locked_owner_remaining when undeliverable (same mechanism as IMPL-43; needs a Codex decision because it changes DR-13 wording). Status: open for Codex; no code change made.
2. **A build_candidates batch that straddles a candidate-page edge fails as PageNotProvisioned (M8-F2, documentation / error naming).** BuildCandidates carries one candidate page per set, so the keeper must size each batch to stay inside one page of BOTH sets (up to 8 rows otherwise). A straddle is refused fee-only with PageNotProvisioned 6035 (close.rs build_batch), which misnames the condition. Keepers should batch at page edges; the M11 keeper documentation must say so, and a dedicated code could be appended later (frozen codes are append-only). No cash impact; the build is retryable with aligned batches. Status: documented; no code change.

## Codex M8 review outcome

PASS on the M8 source, reserve and fuzz work. One runtime proof was missing and is now row 20 of the reserve suite, `settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable`: a settled close with a deferred owner remainder large enough to pay for a candidate page refuses `provision_page(Candidate, LockedClose)` for both sets with exact `LockNotPermitted`, fee-only and byte-identical state, then finalizes and delivers. RED on the pre-guard IMPL-45 ELF (the page was provisioned out of the refund), GREEN on the current ELF. M8-F1 is accepted as documented behaviour (money stays on the page until the immutable destination can receive it; cleanup never gates commit, finalize or open; no new deferral mechanism). M8-F2 is accepted as a keeper requirement: build batches align with participant and both candidate page edges, the one-row fallback stays, and the M11 keeper contract and tests must say so; no new error code.

## Caveats a reviewer should know

1. LiteSVM local default rent fixture (3,480 lamports per byte-year × 2-year exemption); LiteSVM does not check rent state for data-less accounts.
2. one synthetic ORAO fulfil authority; Squads stand-in proves application guards only.
3. the fuzz sequence is one fixed seed; coverage counters are asserted, not a distribution claim.
4. `fund_operations` is permissionless and `withdraw_operations` is sponsor-signed in the program; the IMPL-37 brief wrongly said both were G-signed, and the suite uses the real signers.
5. The M7 `Scene` builder hard-codes the `financial.close.` label root, so the reserve suite has its own mirror (`build_scene`) with a candidate-page pre-provision hook; M7 assertions are untouched (M7 helpers were made `pub(crate)` only).

## Proof level

GREEN on real lamports in LiteSVM for the reserve lifecycle end to end (lock, sponsor-first spend, once-only settlement, deferred delivery, R3 cleanup, cash progress with zero free funds) and for a 300-step randomized operation sequence that re-checks both conservation identities and the partition identity after every step; one synthetic ORAO authority; the Squads stand-in proves application guards only; no deployment or live action.

## Staffing

Coordinator (Claude Fable 5.1): IMPL-36 application and commit, first execution and one batch-alignment fix of IMPL-37, receipts, graph. Sonnet workers: IMPL-36 (program pass, isolated worktree; found G1), IMPL-37 (reserve suite + fuzz, compile-checked). Codex: M7/IMPL-45 reviews that shaped the deferred-refund rows.

## Solana notes

- Page-bounded instructions are the Solana way to do unbounded work: one `build_candidates` call carries one candidate page per set, so a keeper batches at page edges (M8-F2); on ICP the same loop would simply iterate inside one update call.
- Rent is the reason cleanup exists at all: closing a finished page returns its rent components to the parties that funded them (DR-13); cleanup is never a precondition for cash progress, which is why the depleted-free-funds row can commit, finalize, claim and deliver with `free_balance == 0`.

## Next

M9 (IMPL-39): measurement matrix (inline N, chunk populations × rows, churn, dust claims, M = 100 close path, keeper count), DR-31 decision output, `production-sbf` CI job skeleton; then M10 (IMPL-40) security review with the review personas.
