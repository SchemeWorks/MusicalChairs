# Native Solana — M4 provisioning, rent components and cleanup handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M3 handoff](native-solana-m3-handoff-20261014.md) (`18a76b8`, incl. the IMPL-20 review fix). Local work only; nothing deployed, signed with live keys, funded, registered, activated or merged.

## Commits since the M3 handoff (oldest first)

| Commit | Subject |
| --- | --- |
| `5d4dcd6` | feat(program): generic page provisioning, rent components, safe cleanup (M4) |
| `2c1a837` | feat(program): regenerate IDL for the M4 provisioning instructions |
| `d4b90b5` | docs(kanban): M5 admission-and-payout cards IMPL-24..IMPL-27 |
| `4585bb2` | feat(program): admission, Series A funding and owner payouts with the closing pin (M5) |
| `fa37be8` | feat(program): regenerate IDL for the M5 admission and payout instructions |
| `fe5d3d0` | test(solana): LiteSVM provisioning and cleanup suite with the local rent table (M4) |

## What M4 delivered

- **Program (IMPL-21, `5d4dcd6`)**: `provision_page(kind, index, funding)` with `PageKind {Registry, Snapshot, Participant{round}, Candidate{close_id, set_tag}}` and `Funding {Free, Caller, LockedClose}` replacing `provision_snapshot`/`provision_registry`; `provision_close_record(close_id, funding)`; `close_shared_account(kind)` with R3 refusals and identity-bound predicates; `reconcile_ops_cash`; `RentComponents` recorded on every provisioned account; `Ops.total_rent_funded`; `protected_rent` seeded at genesis; errors 6032–6036. 86 program unit tests at that commit.
- **Need bound (decision, flagged for Codex)**: sponsor-cash (`Free`) provisioning is bounded by need (Registry/Snapshot `index <= registry_len/8`, Participant `index <= participant_count/8`, next-close Candidate `index < pages_for(max(participants,1))`), because the worker spotted that permissionless Free provisioning could lock sponsor free ops into empty permanent pages that can never be reclaimed. Spec-compatible tightening; `Caller`/`LockedClose` unaffected.
- **Harness (IMPL-22, `fe5d3d0`)**: 17 rows / 173 observations, all green on the first run: 101 successes, 72 exact-code rejections, byte-identical Ops/Pool/OpsCash on every rejection, conservation on every success, `total_rent_funded` asserted on-chain, a measured rent table.
- **ELF lineage**: the M4-only ELF (`dfc40c51d3474ff2…`) was built but superseded before the harness compiled because the M5 program (`4585bb2`) landed first; the suite executed on the M5 ELF `28c9d90b29185d37d58008d96888775b024042546f25a401b6edf4bddf60e4d0` (source set `f77e9d8a5a911b01…`). 51 harness tests pass; the M0 RED trio still fails, now on fixture preconditions (open 6035 no participant page; claim Anchor 2006 on `config`; discover_row 6021) — IMPL-25 turns the first two green.

## Local default rent table (LiteSVM, 6,960 lamports/byte incl. 128-byte overhead; NOT the mainnet schedule, DR-27)

| Account | SPACE | Rent (lamports) |
| --- | --- | --- |
| `candidate_page` | 331 | 3,194,640 |
| `close_record` | 657 | 5,463,600 |
| `participant_page` | 330 | 3,187,680 |
| `registry_page` | 321 | 3,125,040 |
| `snapshot_page` | 529 | 4,572,720 |

Zero-data floor (OpsCash): 890,880 lamports.

## Caveats and deviations

1. Row "index beyond `required_pages_per_set`" is unreachable (sequential check fires first); the boundary `index == required` is tested in both sets.
2. `close_shared_account` takes no index; it re-derives the PDA from the kind seeds plus the page's stored index. A CloseRecord passed as Snapshot/Registry → `NotClosable`; as Participant/Candidate → `WrongAccount`.
3. Prefunded targets debit `rent − existing lamports`; the surplus lands in `unallocated_excess` on close.
4. `Ops.total_rent_funded` also counts `stage_config_version` rent (Free path).
5. `ParticipantPage`/`CandidatePage` are written through unchecked accounts and therefore stay outside the IDL account list until an `Account<T>` references them (IDL after M4: 23 instructions, 14 accounts incl. `CloseRecord`).
6. `Observation.account_changes` reports a zero lamport delta for a deleted (closed) account; the suite measures from before/after balances.

## Proof level

LiteSVM execution of every provisioning/cleanup row on the compiled ELF with complete provenance; rent figures are local defaults; settlement/refund at terminal promotion remains M7/M8; no production game instruction is GREEN yet (M5 in progress). Graph: `2026-10-14-m4-graph-refresh.json` (12,585 / 17,942 / 40; every prior identity retained).

## Staffing

Coordinator (Claude Fable 5.1): build executor, sole graph writer, verifier, IDL regeneration, receipts. Sonnet workers: IMPL-21 (two rounds), IMPL-22 (incl. the minimal M5 builder migration when the program ABI moved under it). No worker ran an SBF build or git.

## Solana notes

- Rent is a refundable deposit proportional to allocated bytes plus a 128-byte metadata overhead; the program reads `Rent::get()` because the schedule is changing (SIMD-0437), so these figures are a dated local measurement.
- Closing an account means moving its lamports out, zeroing and resizing its data and reassigning it to the system program; a second close sees a system-owned empty account (`AlreadyClosed`). "Permanent" pages are a policy enforced by `NotClosable`, not a runtime property.
- Beneficial rent attribution (who gets the refund) is recorded by the program because the CPI payer (OpsCash) is not the beneficial funder.

## Next

M5 (admission and payouts): program handlers already landed (`4585bb2`, 124 unit tests); IMPL-25 (harness World + suite, RED→GREEN) is in flight; IMPL-26 property/differential tests; IMPL-27 close-out.
