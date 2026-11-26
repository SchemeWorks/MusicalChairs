# Native Solana — M2 schema freeze and IDL v1 handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M0 handoff](native-solana-m0-handoff-20261014.md) (`ca9120c`). Local work only; nothing deployed, signed, funded, registered, activated or merged.

## Commits since the M0 handoff (oldest first)

| Commit | Subject |
| --- | --- |
| `6c3786c` | docs(kanban): M2 schema-freeze cards IMPL-10..IMPL-15 with owners and scopes |
| `5a7bd0f` | feat(program): freeze the account schema (M2 steps 1-3) |
| `40ee1b1` | build(solana): add the Anchor IDL build tool and gen-idl.sh (no Anchor CLI) |
| `27647c7` | feat(program): events, frozen error codes and event-cpi (M2) |
| `3c2d156` | feat(program): commit IDL v1 and a regenerate-and-diff check |
| `df0a4cd` | test(solana): harness follows the frozen schema and event-cpi account lists |
| `da6282e` | test(solana): pad fixture accounts to the declared space |

## What M2 delivered

- **Schema (IMPL-10, `5a7bd0f`)**: spec §6 accounts in `state.rs` with 16 pinned sizes; per-set `required_close_reserve` (§8.1) with vectors; flat reserve removed; `Pool.closing()` derived from `current_close_id`; `guardian_vault` on Pool and Admission. Program unit tests: 55 pass (they did not compile before IMPL-10).
- **Events, errors, event-cpi (IMPL-11, `27647c7`)**: 19 `#[event]` structs with `schema_version`, the bridge trio carrying `round_id`/`close_id`; `ErrorCode` moved to `error.rs`, `NotImplemented` fixed at 6021, 6022–6030 appended; nine `#[event_cpi]` structs; four implemented control handlers emit; encoding fixture `solana/fixtures/event-encodings.json` (sha `de6575130d2d8650…`).
- **IDL tool (IMPL-12, `40ee1b1`) and IDL v1 (IMPL-14, `3c2d156`)**: `solana/tools/idl-build` + `gen-idl.sh`/`check-idl.sh`; IDL sha `6ddce330dc9bfcac936e008a656e6bfa28299a19eaf56d14553fc2a9c9c23cb2` — 17 instructions, 11 referenced accounts, 19 events, 31 errors, 34 types; regenerate-and-diff clean.
- **Harness (IMPL-13, `df0a4cd`, `da6282e`)**: fixtures follow the schema, event-cpi metas on nine builders, fixtures zero-padded to `SPACE`.
- **Build + RED (IMPL-15)**: ELF `340350ddb1c9a6cc01646c7b5a7021bc8deccdc3baf0e99c153fbbc53fda73a4` (667,784 B) at `df0a4cdf18eecad1073a667e3664d21f0371a232`, complete `source_set_sha256 4f12e2abbc778bfc38d2d0b14f189102b588781f69486bf6b928039591b81525`, lock `98d9119c163e1b3e…`, digest equal before/after. RED unchanged in shape: exactly `open_moves_real_sol`, `claim_moves_real_sol`, `allocation_golden_t10000` fail with `Custom(6021)`; two structural tests pass; controls suite passes with the event-CPI self-invoke visible.

## Measured numbers

| Observation | success | error | CU (M0 → M2) | packet bytes | accounts |
| --- | --- | --- | --- | --- | --- |
| `open_moves_real_sol` | False | InstructionError(1, Custom(6021)) | 27,333 → 29,702 | 562 → 596 | 11 → 12 |
| `claim_moves_real_sol` | False | InstructionError(1, Custom(6021)) | 22,466 → 24,985 | 686 → 720 | 15 → 16 |
| `allocation.golden-t10000.discover-row` | False | InstructionError(1, Custom(6021)) | 14,097 → 14,512 | 390 → 390 | 8 → 8 |
| `controls.pause.guardian` | True | — | 3,296 → 9,780 | 348 → 382 | 5 → 6 |
| `controls.pause.wrong-guardian` | False | InstructionError(1, Custom(6004)) | 4,932 → 7,236 | 348 → 382 | 5 → 6 |
| `controls.operations.fund-20000` | True | — | 9,567 → 16,531 | 421 → 455 | 7 → 8 |
| `controls.operations.withdraw-5000` | True | — | 11,793 → 18,926 | 421 → 455 | 7 → 8 |
| `controls.management.empty-balance` | False | InstructionError(1, Custom(6007)) | 11,796 → 14,241 | 454 → 488 | 8 → 9 |

Account sizes (`T::SPACE`, discriminator included; rent is charged on `SPACE + 128`):

| Account | SPACE |
| --- | --- |
| `Pool` | 374 |
| `Admission` | 50 |
| `GameVault` | 9 |
| `Ops` | 193 |
| `RngAuthority` | 41 |
| `ConfigVersion` | 277 |
| `User` | 84 |
| `UserRound` | 89 |
| `Position` | 159 |
| `Backer` | 147 |
| `RegistryPage` | 321 |
| `SnapshotPage` | 529 |
| `Ticket` | 241 |
| `ParticipantPage` | 330 |
| `CandidatePage` | 331 |
| `CloseRecord` | 657 |

## Caveats a reviewer should know

1. **IDL coverage.** Anchor emits only types reachable from instructions, accounts and events. `ConfigVersion`, `CloseRecord`, `RngAuthority`, `ParticipantPage`, `CandidatePage`, `ReserveSchedule`, `SpentLedger` and `Promotion` are therefore **not in IDL v1**; they enter when M3+ instructions reference them. Their layout is frozen by the `state.rs` SPACE-table test. M11 should not assume IDL v1 is the complete account catalogue.
2. **IDL toolchain.** The nested `cargo test --features idl-build` runs on the pinned 1.92.0 with `RUSTC_BOOTSTRAP=1` (anchor-syn needs `Span::unwrap` under `procmacro2_semver_exempt`); no new toolchain was installed. The nested build is `--locked`; a stale program lock fails loudly (negative test run).
3. **Interface refinement.** `required_close_reserve` takes the captured `ReserveSchedule`; `Rent` is consumed by `ReserveSchedule::capture(rent, config)` instead of a second parameter.
4. **Not wired yet (later cards).** `total_contributed`/`total_withdrawn`, `protected_rent` consultation, page `RentComponents` recording, `fold_slices` parameterisation from the ticket's pinned bps, `validate_existing_page` for the two new page kinds, and `Pool.guardian_vault` rotation (M3).
5. **`Promotion.reason`/`stage` are u8 codes**; the enumeration of reasons and diagnostic stages is defined when M7 writes them.

## Proof level

Host unit tests; compiled SBF stub with complete provenance; RED preserved on the new ELF in LiteSVM; event-CPI observed on an implemented control instruction. No production financial instruction is GREEN. Graph: `docs/solana/evidence/2026-10-14-m2-graph-refresh.json` (11,240 / 15,393 / 38; all prior identities retained).

## Staffing

Coordinator (Claude Fable 5.1): single build executor, sole graph writer, verifier, author of IDL v1 commit, harness padding fix and receipts. Sonnet workers: IMPL-10 (schema), IMPL-11 (events/errors/event-cpi), IMPL-13 (harness). Haiku worker: IMPL-12 (IDL tool, two rounds). No worker ran an SBF build or git.

## Solana notes

- The IDL is the Candid analogue: generated from the program and consumed by clients; freezing it is what lets M11 start, with caveat 1.
- `emit_cpi!` makes the program invoke itself with the event bytes as instruction data, signed by the `__event_authority` PDA, so events live in inner instructions rather than truncatable logs (EVM `emit` can be truncated; ICP has no logs). Cost in LiteSVM: two extra accounts and about 2,100 CU per emit.
- `InitSpace` reserves the maximum encoding for `Option`/enum fields; Borsh writes a shorter prefix and the remainder stays zero; rent is paid on the reserved size, like a fixed-size stable-memory record rather than a Solidity dynamic slot.

## Next

M3 (authenticated controls under LiteSVM): Squads `Multisig` discriminator and `config_authority` parse fixes, `stage_config_version`/`activate_config_version`/`rotate_recipient`/`rotate_guardian`, byte-exact Squads and loader fixtures, the LiteSVM controls suite.
