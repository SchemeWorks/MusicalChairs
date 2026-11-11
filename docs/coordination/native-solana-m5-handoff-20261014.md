# Native Solana — M5 admission and payouts handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M4 handoff](native-solana-m4-handoff-20261014.md) (`093124a`). Local work only; nothing deployed, signed with live keys, funded, registered, activated or merged.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `8049d45` | docs(kanban): M6 allocation-ticket cards IMPL-28..IMPL-31 |
| `3c488ad` | test(solana): M5 World and admission/payout suite; M0 open and claim labels turn GREEN |
| `110961e` | docs(kanban): M7 close/draw/promotion/finalization cards IMPL-32..IMPL-34 |
| `9dcbb87` | test(solana): payout property and differential tests against the host oracle (M5 step 4) |
| `e130d17` | fix(program): pin the staged quota to the frozen three-per-3600s contract (Codex M5 review) |
| `bd1aad7` | docs(evidence): M5 admission-and-payouts receipt with the RED-to-GREEN comparison |

## RED → GREEN on the M0 labels (identical labels; M0 RED JSON vs M5 JSON)

| Label | M0 | M5 | CU | packet bytes |
| --- | --- | --- | --- | --- |
| `open_moves_real_sol` | InstructionError(1, Custom(6021)) | ok | 27,333 → 49,920 | 562 → 674 |
| `claim_moves_real_sol` | InstructionError(1, Custom(6021)) | ok | 22,466 → 48,529 | 686 → 728 |
| `allocation.golden-t10000.discover-row` | InstructionError(1, Custom(6021)) | InstructionError(1, Custom(6021)) | 14,097 → 14,537 | 390 → 390 |

`discover_row` stays RED until M6 (its ABI is now `discover_rows`; the label is preserved by the M6 harness card).

## What M5 delivered

- **Program (IMPL-24, `4585bb2`)**: `open_position(plan, gross, client_nonce, expected_round_id, expected_config)`, `fund_series_a(gross, expected_round_id)`, `withdraw_simple` / `settle_compounding(minimum_receive, max_work_contribution)` on a shared `Payout` struct, `close_simple_position()`; Front-End Load from the pinned config; Simple accrual per spec §3.2 (increment, remainder, `accrued_gross − gross_paid`); compound settlement from the pinned Q18 factors; `gross_paid = min(owed, pot)` compared in u128 before narrowing; toll → seed when the registry is empty, else `pending_toll` plus a Discover ticket pinned to the position's config bps; strict `owed > pot` close binds `CloseRecord(next)`, locks the reserve sponsor-first with a physical owner→OpsCash transfer, advances the close counters atomically and emits `RoundClosed`; errors 6037 `StaleConfig`, 6038 `PublicSeriesADisabled`. 124 unit tests.
- **Harness (IMPL-25, `3c488ad`)**: `World` plus the 19 plan rows (73 observations) with exact lamports; events decoded from the event-CPI inner instruction data.
- **Property tests (IMPL-26, `9dcbb87`)**: 200 seeded Simple payouts agree with the economics oracle on every field; remainder carry, backwards clock, tier edges, Compound30, owed above u64, empty-registry routing. 7 passed, 1 ignored (M6).
- **Codex M5 review correction (IMPL-35, `e130d17`)**: `ConfigTerms::validate` pins the quota to 3 per 3,600 s because `User::record_success` hard-codes it; RED shown on the old validator and old ELF, GREEN on a worktree-built ELF `132eab6c…`.

## Vectors (actual == expected on the compiled ELF)

| Case | Values |
| --- | --- |
| open 1 SOL Simple | management 40,000,000; pot 960,000,000; Position.gross 1,000,000,000 |
| withdraw_simple after 1 day on 1 SOL | owed 110,000,000; toll (1,200 bps) 13,200,000; net 96,800,000 |
| Compound15 1 SOL | gross 4,473,565,759; toll 402,620,918; net 4,070,944,841 |
| Compound30 1 SOL | gross 12,267,678,469; toll 1,594,798,200; net 10,672,880,269 |
| Series A 50,000,000 | pot 50,000,000; management 0; entitlement 62,000,000; first_sequence 1 |

## Compute (LiteSVM, successes)

| Instruction | min CU | max CU | n |
| --- | --- | --- | --- |
| `OpenPosition` | 46,703 | 51,203 | 10 |
| `WithdrawSimple` | 48,530 | 62,800 | 13 |
| `SettleCompounding` | 48,062 | 48,062 | 1 |
| `FundSeriesA` | 37,685 | 42,249 | 4 |
| `CloseSimplePosition` | 16,137 | 16,137 | 1 |

## Caveats a reviewer should know

1. `Payout.user_round` is an unchecked PDA-verified account so a stale-round position returns `PositionRoundClosed` (6025) rather than an Anchor load error.
2. The Simple accrual guard requires the pinned config's `plan_rates_bps_per_day[0] == 1,100`; the economics crate hard-codes 11%/day. Config fields `a_entitlement_bps`/`b_entitlement_bps`/`seed_bps`/`open_cutoff` are not consulted by handlers (hard-coded contracts); the quota is now pinned at staging (IMPL-35).
3. Ticket start requires `ticket.phase == IDLE`; `commit_ticket` (M6) resets it.
4. Closing fixtures fund Ops with exactly the reserve; the plan sketch omitted that funding.
5. Two test-side fixes by the coordinator: the owner-as-destination delta is compared signed; the executable program account as destination is rejected by Anchor's `mut` constraint (2000) before the handler's `ProtectedDestination`.
6. The ELF under test for M5 is `28c9d90b29185d37d58008d96888775b024042546f25a401b6edf4bddf60e4d0`; IMPL-35's change is in the current main-tree ELF `4c4f3bff…` (M6 build) and will be covered by the M6 run.

## Proof level

GREEN on real lamports in LiteSVM for admission, Series A funding, Simple and compound payouts and the closing pin, with host-oracle agreement over 200 sampled payouts; allocation phases GREEN at the program-unit level (M6 program landed) and pending LiteSVM proof (IMPL-29); no deployment or live action. Graph refresh for M5 follows with the M6 refresh at the next stable tree (the graph receipt will name both fragments).

## Staffing

Coordinator (Claude Fable 5.1): build executor, verifier, two assertion fixes, IMPL-35, receipts. Sonnet workers: IMPL-24 (program), IMPL-25 (harness), IMPL-26 (property tests).

## Solana notes

- A payout is one atomic transaction: owner contribution transfer, vault payout and every state write revert together, so a minimum-receive failure leaves only the fee.
- Rent for new PDAs (Position, User, UserRound) is paid by the owner as init payer, separate from the deposit; dust payouts to a brand-new wallet are refused by the runtime's rent-exempt minimum, which is a runtime rule, not program logic.
- A program's `#[account(mut)]` on a destination means Anchor refuses executable accounts before the handler runs; defence in depth, but the exact error code differs from the program's own check.
