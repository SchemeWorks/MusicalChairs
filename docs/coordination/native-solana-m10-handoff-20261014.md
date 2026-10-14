# Native Solana M10 handoff — security review, repairs, review-gap rows, threat model (2026-10-14)

Branch `codex/native-solana-production-core`. Cards IMPL-40 / 40a / 40b / 40c. Program ELF after the
repairs: `8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826` (1,397,160 B; source set
`994a9ff48fa26033…`; lock unchanged `77f859f9…`; 0 "Stack offset" lines), pinned in
`docs/solana/evidence/latest-build.json` with the superseded `d77b23bf…` kept in its lineage.

## Commits (oldest first)

| commit | what |
| --- | --- |
| `ae0e937` | docs(evidence): M9 close-out (Linux CI evidence run 38605447811, M9 graph fragment/refresh, M9 handoff) |
| `7c3d84a` | fix(program): security review findings (IMPL-40a); test(harness): review-gap rows (IMPL-40b); workflow SEC-8 steps; latest-build.json re-pinned |
| this commit | docs(solana): threat model, review reports (verbatim), triage + Codex adjudications, M10 receipts, graph fragment/refresh, this handoff; the two Codex M9 metadata fixes (handoff artifact string; M9 refresh scope pinned to commit blobs with the receipt-time dirty hashes separated) |

## What M10 delivered

1. **Two independent persona reviews** (read-only, Sonnet workers): protocol security
   ([`evidence/2026-10-14-m10-security-review.md`](../solana/evidence/2026-10-14-m10-security-review.md)) and
   economy ([`evidence/2026-10-14-m10-economy-audit.md`](../solana/evidence/2026-10-14-m10-economy-audit.md)),
   published verbatim at Codex's request. No Critical or High finding; no conservation break; both
   converged on the unpinned Carried Interest table and the dust-toll ticket.
2. **Coordinator triage** ([`native-solana-m10-review-triage-20261014.md`](native-solana-m10-review-triage-20261014.md)):
   seven repairs applied failing-test-first, accepted/documented items, and the returned decisions with
   spec citations, reproducers and minimal recommendations — adjudicated by Codex the same day.
3. **Program repairs** (`7c3d84a`, receipt [`evidence/2026-10-14-m10-fixes.json`](../solana/evidence/2026-10-14-m10-fixes.json)):

| id | repair | RED (pre-fix) → GREEN (fixed ELF) |
| --- | --- | --- |
| SEC-1 = ECO-2 | Carried Interest table + day bounds restate-only pinned in `ConfigTerms::validate` | unit `validate_pins_the_simple_toll_table_and_day_bounds`; staging `noncanonical_toll_table` / `noncanonical_toll_bounds` ("unexpectedly succeeded" on `d77b23bf`) |
| SEC-3 = ECO-1 (dust) | `toll_route` seeds a toll whose slices all floor to zero (host `Book::allocate` parity, plan L647) | unit 1..=64 vs the host rule (toll 1 → Ticket pre-fix); LiteSVM toll 3 → seed, toll 6 → ticket; pre-fix ticket evidence `toll=3 seed_delta=3 funded_claims_delta=0 backers_unchanged=true` |
| ECO-3 | floating `excess` / `unallocated_excess` in `sync_game` / `sync_ops` | LiteSVM rent-ratchet row: `Custom(6001)` pre-fix, succeeds and re-floats to 0 after |
| SEC-4 | genesis `provider_client` must equal the derived ORAO Client PDA | `initialize…wrong_provider_client` on a fresh rig: accepted pre-fix → `WrongProviderTuple` |
| SEC-5 | `withdraw_provider_return` through `require_governance` | degraded 1-of-3 G profile: accepted pre-fix → `WrongGovernanceProfile`; rogue multisig now `WrongAccount` (as every other G instruction) |
| SEC-6 / SEC-8 | error-range comment; CI asserts no `Stack offset` warning and runs `check-idl.sh` | CI run of this tranche |

4. **Review-gap rows** (IMPL-40b, receipt [`evidence/2026-10-14-m10-review-gaps.json`](../solana/evidence/2026-10-14-m10-review-gaps.json)):
   `src/review_gaps.rs`, CLI suite `review`, 11 rows / 237 observations, 12/12 tests on first execution against the fixed ELF.

| row | rejections (code × count) | successes | fee-only rollback on every rejection |
| --- | --- | --- | --- |
| `wrong_owner_refund_is_refused_while_an_owner_remainder_is_locked` | 3 rejections (6005×3) | 27 | yes |
| `claim_backer_substitutions_are_refused` | 14 rejections (6005×7, 6001×1, 6008×6) | 1 | yes |
| `open_and_fund_series_a_need_their_provisioned_pages` | 4 rejections (6035×2, 6005×2) | 2 | yes |
| `payout_with_registry_beyond_the_snapshot_pages_is_refused_at_runtime` | 2 rejections (6035×2) | 1 | yes |
| `position_keeps_its_pinned_config_across_a_later_activation` | 2 rejections (6037×1, 2006×1) | 8 | yes |
| `exit_paths_work_while_paused` | 2 rejections (6009×2) | 6 | yes |
| `wrong_ops_cash_and_wrong_config_pda_are_refused` | 8 rejections (6005×8) | 3 | yes |
| `fuzz_keeps_per_backer_funded_equal_to_funded_claims` | 21 rejections (6014×6, 6022×5, 6010×10) | 98 | yes |
| `inline_eight_capped_rows_matches_the_chunked_ticket` | 0 rejections (—) | 8 | yes |
| `dust_loss_candidate_entitlement_cutoff` | 0 rejections (—) | 24 | yes |
| `caller_funded_snapshot_page_is_not_closable` | 2 rejections (6033×2) | 1 | yes |

5. **Threat model** [`docs/solana/threat-model.md`](../solana/threat-model.md): assets, actors and trust,
   per-instruction authority with every hand-validated account, the 16-item checklist mapped to tests
   (items 15/16 by execution + documented reason), accepted limitations, ownerless-lamport inventory,
   residual risks.
6. **Final run on the fixed ELF**: 187 lib + 23 integration tests green; CLI 9,505 observations with
   0 rejections lacking fee-only rollback; program and harness clippy `-D warnings` clean.
7. **Graph**: `graphify update .` on the final tree then the M10 fragment (6 nodes / 39 edges / 1 hyperedge)
   → 17,042 / 25,745 / 46; every prior node id, hyperedge and edge direction preserved
   ([`evidence/2026-10-14-m10-graph-refresh.json`](../solana/evidence/2026-10-14-m10-graph-refresh.json)).

## Codex adjudications recorded (no code change)

- **R-1 prospective economics** (SEC-2): NOT pinned — DR-04, spec L248/L296–297 and the
  accepts-later-variation test keep genuinely consumed `ConfigVersion` terms tunable; only fixed-helper
  values are restate-only pinned (now including the Carried Interest table). Launch economics are
  canonical v1; any future G economics proposal needs explicit review/preflight. Provider cost inputs tunable.
- **R-2 `effective_round_id` mis-staging**: accepted/documented; current play, funds and claims unaffected;
  horizon or in-place re-stage deferred as a future contract amendment; G preflight/runbook checks (M13).
- **R-3 / R-4**: no loss floor, minimum ticket toll or cooldown; keeper priority, page-edge batching and the
  one-row fallback carried to M11.

## Caveats a reviewer should know

- Linux CI for the fixed ELF is the run of commit `7c3d84a` / this commit (pins updated together); the
  Linux ELF will differ from `8f97e83d…` by the known path-derived metadata, with source set `994a9ff4…`
  and lock `77f859f9…` asserted.
- The rent-ratchet row drives the Rent sysvar to one tenth and back; the cluster has never raised the
  rent floor. LiteSVM still skips rent-state checks for data-less accounts (`tests/transfer_constraints.rs`).
- `cargo fmt --check` shows pre-existing drift in `programs/musical_chairs/src/{allocation,close}.rs` and
  `financial-runtime/src/close.rs` on lines nobody touched in this tranche; CI does not fmt-check those
  crates. Tidy at the next program change (it alters the source set).
- The persona reports are model output: severities and "cluster" statements are theirs; the coordinator's
  verification of each finding is in the triage document.

## Proof level

Local LiteSVM 0.7.1 on the production SBF ELF built with the pinned tools (macOS executor); RED on the
pre-fix ELF / source for every repair; unit, property, randomized-walk and CLI evidence; Linux CI for the
fix commit pending at push time. No deployment, live keys, funds, governance or service activation.

## Staffing

Coordinator (Claude Fable 5.1): triage, all program repairs and their RED/GREEN rows, builds, runs,
receipts, threat model, graph. Two Sonnet review personas (read-only) and one Sonnet harness worker
(IMPL-40b, scratch worktree, compile + clippy only; coordinator applied and executed).

## Next

M11 frontend native adapter (plan L667–676) with the keeper contract notes from this tranche (ticket
priority, page-edge batching, one-row fallback, `credit_is_deliverable` preflight on destinations, G
staging preflight); M12 bridge; M13 keepers/monitoring. Stop before M14/M15 public-network actions.
