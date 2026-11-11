# Native Solana — M6 allocation ticket and lazy claims handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M5 handoff](native-solana-m5-handoff-20261014.md) (`f65c380`). Local work only; nothing deployed, signed with live keys, funded, registered, activated or merged.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `e78d652` | feat(program): bounded allocation ticket, prune fixes, lazy Backer claims (M6) |
| `cf8926b` | feat(program): regenerate IDL for the M6 allocation instructions |
| `39acb9d` | docs(kanban): M8 reserve/cleanup, M9 measurement and M10 security cards IMPL-36..IMPL-40 |
| `3959a16` | fix(program): pin the five staged-config fields consumed by fixed helpers (Codex M6 follow-up) |
| `8f4454f` | test(solana): M6 allocation LiteSVM suite; the last M0 RED label turns GREEN |
| `cabfd01` | fix(program): make the apply_senior Backer account optional |
| (this commit) | docs(evidence): M6 receipt, IMPL-30 differential test, combined M5–M6 graph refresh, handoff |

## RED → GREEN on the last M0 label (identical label; M0 RED JSON vs M6 JSON)

| Label | M0 | M6 |
| --- | --- | --- |
| `financial.allocation.golden-t10000.discover-row` | InstructionError(1, Custom(6021)) | ok |

All three M0 RED labels (`open_moves_real_sol`, `claim_moves_real_sol`, `allocation.golden-t10000.discover-row`) are now GREEN with their original labels and assertions on the same ELF.

## What M6 delivered

- **Program (IMPL-28, `e78d652`)**: six typed instructions `discover_rows` / `apply_senior` / `seal_weights` / `allocate_rows` / `prune_rows` / `commit_ticket`, each taking `expected_ticket_id`, `expected_cursor` and 1..=8 rows as remaining accounts (`StaleTicket` 6039, `StaleCursor` 6040). Discovery accumulates Series A remainders in U256 (DR-05); seal freezes the post-senior denominator; a dead A pool and the B floor residue route to seed; allocate pays full-pool floor shares with the A residue derived at the last batch so `credited + seed_credit == toll` by construction. Prune swap-deletes capped rows with the two legitimate aliases (shared registry page; moved == pruned at the last live slot) and rejects duplicate independent writable Backers (`DuplicateWritableAccount` 6026) — draft Defects 8/9 and finding F4 resolved. `claim_backer()` materializes finalized pending lazily over A and B at `pool.finalized_ticket` and pays from the vault (`NothingToClaim` 6022 on replay). 174 unit tests.
- **Codex M6 follow-up (IMPL-41, `3959a16`)**: `ConfigTerms::validate` pins Simple rate 1,100 bps/day, A entitlement 12,400, B entitlement 11,600, seed 5,000 and open cutoff 10; ten rejection unit cases plus five G-signed staging rejection variants. RED on the old validator and old ELF `4c4f3bff…`, GREEN on a worktree ELF `5389cec2…`, carried into the main ELF below.
- **Harness (IMPL-29, `8f4454f`)**: `World::with_backers` / `with_random_backers` / `start_ticket_fixture` / `advance_until_*` / `digest_financial_state` / `claim_backer`; 18 plan rows, 657 observations (626 successes, 31 exact-code rejections: 6001 ×6, 6005 ×2, 6010 ×2, 6018 ×3, 6022 ×3, 6026 ×2, 6039 ×11, 6040 ×2). Chunk sizes 1/4/8 produce an identical state digest that matches the economics `Book`.
- **Optional senior account (`cabfd01`)**: a ticket with no open A row passes the program-id placeholder, which Anchor decodes as `None`; the mandatory `mut` account had failed `ConstraintMut` (2000) on the placeholder.
- **Differential (IMPL-30, this commit)**: `tests/allocation_diff.rs` replays 50 random tickets (seed 0x4D43_4449_4646_3330 (MCDIFF30), rows cycling 1/4/8 plus a fixed-8 rerun) through the host `Book` driven stepwise and the compiled ELF, with interleaved claims, Series A re-fundings and fixture credits; per-Backer pending/funded/lifetime/entitlement/active, `first_sequence`, Pool deltas and both conservation identities compared after every step. 2 passed in 4.86 s; stats: 11 tiny tickets, 4 claims paying 66,986,391 lamports, 3 empty claims rejected, 4 re-fundings, 3 credits. Two coordinator mutation checks fail as they should (expected seed + 1 fails at the fixture check; `Book.reserve_toll(toll + 1)` fails after ticket 1 with `pending_toll` expected 1 / actual 0).

## Goldens (actual == expected on the compiled ELF)

| Case | Seed | A credits | B credits | Notes |
| --- | --- | --- | --- | --- |
| toll 10,000; A 4,000 / 6,000; two B | 5,001 | 2,363 / 1,636 | 500 / 500 | `funded_claims` 4,999; `pending_toll` 0 |
| toll 10,000; A 1,000 / 10,000; no B | 5,750 | 1,000 / 3,250 | — | A1 capped at its remaining; B floor residue → seed |
| toll 10,000; A 1,000; B 1 | 8,000 | 1,000 | 1,000 | zero post-senior denominator → A pool to seed |

## Chunk fit (LiteSVM, production account sizes; packet max / accounts max / CU max)

| Phase | rows = 1 | rows = 4 | rows = 8 |
| --- | --- | --- | --- |
| `discover` | 399 B / 8 / 18,836 | 504 B / 11 / 27,165 | 644 B / 15 / 39,309 |
| `senior` | 291 B / 5 / 7,987 | 291 B / 5 / 7,987 | 291 B / 5 / 7,987 |
| `seal` | 258 B / 4 / 4,519 | 258 B / 4 / 4,519 | 258 B / 4 / 4,519 |
| `allocate` | 333 B / 6 / 11,988 | 435 B / 9 / 23,525 | 571 B / 13 / 36,994 |
| `prune` | 465 B / 10 / 25,326 | 704 B / 17 / 50,492 | 852 B / 21 / 66,898 |
| `commit` | 326 B / 6 / 14,752 | 326 B / 6 / 14,752 | — |

Registry-size sweep (max over batches; `discover` / `allocate` / `prune` only):

| Registry | Phase | packet B | accounts | CU | fits 1,232 B |
| --- | --- | --- | --- | --- | --- |
| N = 16 (8 rows) | `discover` | 644 | 15 | 40,451 | yes |
| N = 16 (8 rows) | `allocate` | 571 | 13 | 40,371 | yes |
| N = 16 (8 rows) | `prune` | 372 | 6 | 16,585 | yes |
| N = 64 (8 rows) | `discover` | 644 | 15 | 40,451 | yes |
| N = 64 (8 rows) | `allocate` | 571 | 13 | 40,371 | yes |
| N = 64 (8 rows) | `prune` | 372 | 6 | 16,585 | yes |
| N = 100 (8 rows) | `discover` | 644 | 15 | 39,330 | yes |
| N = 100 (8 rows) | `allocate` | 571 | 13 | 40,336 | yes |
| N = 100 (8 rows) | `prune` | 404 | 7 | 19,568 | yes |
| N = 256 (8 rows) | `discover` | 644 | 15 | 40,451 | yes |
| N = 256 (8 rows) | `allocate` | 571 | 13 | 40,371 | yes |
| N = 256 (8 rows) | `prune` | 372 | 6 | 16,585 | yes |
| prune-worst, rows = 4 | `discover` | 644 | 15 | 39,330 | yes |
| prune-worst, rows = 4 | `allocate` | 571 | 13 | 40,336 | yes |
| prune-worst, rows = 4 | `prune` | 672 | 16 | 47,784 | yes |
| prune-worst, rows = 8 | `discover` | 644 | 15 | 39,330 | yes |
| prune-worst, rows = 8 | `allocate` | 571 | 13 | 40,336 | yes |
| prune-worst, rows = 8 | `prune` | 1,044 | 27 | 88,887 | yes |

Widest successful batch: 1,044 B / 27 accounts / 88,887 CU (an 8-row prune in the sweep) — under the 1,232-byte legacy packet without address-lookup tables. This is the first DR-31 evidence; it is a LiteSVM measurement with local rent, not a cluster measurement.

## ELF under test

`solana/programs/musical_chairs/artifacts/musical_chairs_program.so` sha256 `24716634e2b0aa59a4c6b19d34cdc9ddfe1607e01230458179dcc69534c69754`, source set `bdd7e69f44fa171b11d99cb9e0c51c5c4b838fafdc83f8fb19a5b7211cb55214` (19-file digest, before/after guard clean), stand-in `eb78e77f…`. 94 harness tests, `payout_props` 7 passed / 1 ignored, `allocation_diff` 2 passed, CLI `all` exit 0.

## Caveats a reviewer should know

1. The coordinator's brief for IMPL-28 described a slice-decrement residue scheme that under-pays later A rows (A2 1,190 instead of 1,636); the worker correctly implemented full-pool shares with the residue `toll − credited − seed_credit` applied at the last allocate batch, bounded by the open A count. The brief was wrong; the code and the goldens are right.
2. A `discover_rows` on a new ticket while the previous one is uncommitted cannot be driven (the payout cannot start a ticket while `active_ticket` is set); the suite models it as `expected_ticket_id = id + 1 → StaleTicket`.
3. `allocation_diff` has no RED of its own: the worker only compile-checked; the coordinator executed it and ran the two mutation checks recorded in the receipt. Registry slot numbers, the pending ticket tag and pre-ticket-1 liveness are deliberately not compared (different swap-delete histories; the fixture leaves closed rows until the first prune).
4. The 2.7 MB `2026-10-14-m6-cli.json` has no graphify file node; the M6 fragment references the receipt's `cli_copy` leaf instead.
5. The IDL still omits account types no instruction references (M2 caveat).
6. Codex's committed-source review of `e78d652` PASSED at the source/unit boundary; the LiteSVM proof (IMPL-29/30) and `cabfd01` post-date that review.

## Compute (LiteSVM, successes, max CU)

| Instruction | rows = 1 | rows = 4 | rows = 8 |
| --- | --- | --- | --- |
| `DiscoverRows` | 18,836 | 27,165 | 39,309 |
| `AllocateRows` | 11,988 | 23,525 | 36,994 |
| `PruneRows` | 25,326 | 50,492 | 66,898 (worst 88,887 in the sweep) |
| `ApplySenior` / `SealWeights` / `CommitTicket` | 7,987 / 4,519 / 14,752 | same | same |

## Graph

`graphify update .` at `cabfd01` (12,585 → 13,912 nodes, 0 prior ids lost), the idempotent 16-edge/321-status repair, then two fragments merged: M5 `2026-10-14-m5-graph-fragment.json` (8 nodes, 89 edges, community 1332) and M6 `2026-10-14-m6-graph-fragment.json` (7 nodes, 74 edges, community 1333). Result 13,927 nodes / 20,250 edges / 42 hyperedges; all prior node ids, raw edges and hyperedges retained; plan nodes `plan2026_10_13_m5_admission_payouts` and `plan2026_10_13_m6_allocation_claims` → `completed_receipt`; defects 08/09 and F4 → `resolved_in_local_implementation`. Receipt `2026-10-14-m6-graph-refresh.json` names both fragments and the query checks. `graphify-out/` itself is gitignored; the fragments and receipts are the committed artifacts.

## Proof level

GREEN on real lamports in LiteSVM for the whole allocation ticket (discover → senior → seal → allocate → prune → commit) and lazy claims, with three goldens, chunk-size invariance, a 50-ticket differential against the host `Book`, and the M0 RED baseline entirely GREEN; the Squads stand-in still stands in for governance (application guards only); no deployment or live action.

## Staffing

Coordinator (Claude Fable 5.1): build executor, verifier, `cabfd01`, IMPL-41, IMPL-30 execution and mutation checks, receipts, graph. Sonnet workers: IMPL-28 (program), IMPL-29 (harness suite), IMPL-30 (differential test, compile-checked). Codex: committed-source review of `e78d652` and the five-pin follow-up.

## Solana notes

- Every row a phase touches must be passed as an account up front, and a legacy transaction is capped at 1,232 bytes (about 27–30 accounts here); the ICP analogue would be a hard argument-size limit on an update call. That is why the ticket is chunked into 1..=8 rows with a cursor instead of looping over the registry, and why the sweep above is the thing to watch as the registry grows.
- Anchor's `Option<Account>` convention encodes `None` as the program id in that slot; the harness builder maps the placeholder accordingly. The runtime still rejects a writable account passed twice (two `AccountInfo`s aliasing one lamport balance), so prune distinguishes the two legitimate aliases from a duplicate attack at decode time.
- Compute is per instruction (200k CU default, raisable to 1.4M per transaction); the 88,887 CU worst prune leaves headroom under the default without a compute-budget instruction.

## Next

Apply IMPL-32 (M7 program, delivered in an isolated worktree: `close.rs`, `provider.rs`, `sampling.rs`, ORAO dependency, 241 unit tests) to the main tree, regenerate the IDL, relock the harness, rebuild the ELF, then dispatch IMPL-33 (close-lifecycle suite with the captured ORAO executable) and close M7 at IMPL-34.
