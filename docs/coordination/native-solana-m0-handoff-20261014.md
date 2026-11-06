# Native Solana — M0 close-out and M1 host-parity handoff (2026-10-14)

Branch `codex/native-solana-production-core` (base `c99a636`), HEAD before this handoff `edb4f9a`. Supervision: Codex. Authorization: `docs/coordination/native-solana-local-implementation-go-ahead-20261014.md` (sha256 `69ce19e392693a332839d6689c4592fbb5a589c7a3f4a1199dc29dd5100421c0`), acceptance `docs/solana/evidence/2026-10-14-codex-plan-acceptance.json`. Local work only: no devnet/mainnet deployment, live keys, signing, funding, ORAO/Squads registration, governance, production canister/frontend change, service activation or PR merge happened.

## Commits (oldest first)

| Commit | Subject |
| --- | --- |
| `db48ce3` | docs(solana): reconcile native Solana spec, decision register and implementation plan for Codex review |
| `1313b52` | docs(solana): resolve Codex review round 1 in the native Solana spec, register and plan |
| `5c23b1d` | docs(solana): resolve Codex round-2 review C1-C6 in the native Solana spec, register and plan |
| `43f1b4a` | docs(solana): apply Codex final contract corrections F1-F5 to the native Solana spec, register and plan |
| `88adc7b` | docs(solana): apply Codex's eight final consistency edits to the native Solana spec and plan |
| `3109063` | docs(solana): adopt Codex's two final mechanical edits and re-pin the planning graph |
| `80c543b` | chore(solana): adopt uncommitted production account tranche verbatim for review baseline |
| `39bc31c` | merge: integrate accepted native Solana planning commit 3109063 into production-core |
| `5a34654` | docs(coordination): record Codex local go-ahead, acceptance evidence and M0 implementation cards |
| `1203c77` | feat(financial-runtime): emit canonical source_set_sha256 from the SBF build wrapper |
| `5d9afa5` | feat(economics): U256 discovery accumulator and shared pro-rata oracle; M0 toolchain and compile-RED receipts |
| `05be17f` | docs(evidence): pin the M0 stub SBF artifact, source-set digest and verified toolchain |
| `9ecdf70` | test(solana): fix harness compile error and make LiteSVM fixtures genesis-shaped |
| `e8165cb` | docs(evidence): record the genuine M0 RED baseline on the stub ELF |
| `37f506a` | build(solana): cover the economics crate and the digest helper in the source-set digest |
| `8da631c` | docs(evidence): refresh M0 stub and RED receipts with complete source-set provenance |
| `da11e35` | docs(graph): M0 close-out knowledge-graph receipt and scoped fragment |
| `edb4f9a` | docs(evidence): exercise the mid-build provenance rejection path |

## Changed files since `c99a636` (61 paths)

- **docs**: `M docs/coordination/kanban.md`, `A docs/coordination/native-solana-codex-plan-final-corrections-20261013.md`, `A docs/coordination/native-solana-codex-plan-review-20261013.md`, `A docs/coordination/native-solana-codex-plan-review-round2-20261013.md`, `A docs/coordination/native-solana-codex-review-packet-20261013.md`, `A docs/coordination/native-solana-decision-register-20261013.md`, `A docs/coordination/native-solana-local-implementation-go-ahead-20261014.md`, `A docs/coordination/native-solana-plan-final-consistency-20261013.md`, `A docs/coordination/native-solana-state-inventory-20261013.md`, `A docs/solana/evidence/2026-10-13-codex-plan-review-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-codex-plan-review-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-revision-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-revision-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-revision2-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-revision2-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-revision3-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-revision3-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-revision4-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-revision4-graph-refresh.json`, `A docs/solana/evidence/2026-10-13-planning-revision5-graph-fragment.json`, `A docs/solana/evidence/2026-10-13-planning-revision5-graph-refresh.json`, `A docs/solana/evidence/2026-10-14-codex-plan-acceptance.json`, `A docs/solana/evidence/2026-10-14-m0-compile-red.json`, `A docs/solana/evidence/2026-10-14-m0-graph-fragment.json`, `A docs/solana/evidence/2026-10-14-m0-graph-refresh.json`, `A docs/solana/evidence/2026-10-14-m0-provenance-guard.json`, `A docs/solana/evidence/2026-10-14-m0-red-baseline-cli.json`, `A docs/solana/evidence/2026-10-14-m0-red-baseline.json`, `A docs/solana/evidence/2026-10-14-m0-stub-build.json`, `A docs/solana/evidence/2026-10-14-m0-toolchain.json`, `A docs/solana/production-accounts.md`, `M docs/superpowers/plans/2026-10-12-native-solana.md`, `A docs/superpowers/plans/2026-10-13-native-solana-implementation.md`, `M docs/superpowers/specs/2026-10-12-native-solana-design.md`, `A docs/superpowers/specs/2026-10-13-native-solana-reconciled-spec.md`
- **solana/Cargo.toml**: `M solana/Cargo.toml`
- **solana/crates/economics**: `M solana/crates/economics/src/backers.rs`, `A solana/crates/economics/tests/widths.rs`
- **solana/fixtures/economics.json**: `M solana/fixtures/economics.json`
- **solana/programs/musical_chairs**: `A solana/programs/musical_chairs/.cargo/config.toml`, `A solana/programs/musical_chairs/.gitignore`, `A solana/programs/musical_chairs/Cargo.lock`, `A solana/programs/musical_chairs/Cargo.toml`, `A solana/programs/musical_chairs/src/allocation.rs`, `A solana/programs/musical_chairs/src/controls.rs`, `A solana/programs/musical_chairs/src/lib.rs`, `A solana/programs/musical_chairs/src/positions.rs`, `A solana/programs/musical_chairs/src/state.rs`
- **solana/tests/financial-runtime**: `A solana/tests/financial-runtime/.cargo/config.toml`, `A solana/tests/financial-runtime/.gitignore`, `A solana/tests/financial-runtime/Cargo.lock`, `A solana/tests/financial-runtime/Cargo.toml`, `A solana/tests/financial-runtime/scripts/build-sbf.sh`, `A solana/tests/financial-runtime/scripts/source-set-digest-test.sh`, `A solana/tests/financial-runtime/scripts/source-set-digest.sh`, `A solana/tests/financial-runtime/src/financial.rs`, `A solana/tests/financial-runtime/src/lib.rs`, `A solana/tests/financial-runtime/src/main.rs`, `A solana/tests/financial-runtime/src/runtime.rs`

## Cards, commands and outcomes

| Card | Owner | Command / action | Outcome | Evidence |
| --- | --- | --- | --- | --- |
| IMPL-00 | coordinator | adopt the uncommitted tranche verbatim; verify every inventory hash | `80c543b`; Codex independently matched all 21 baseline hashes | kanban row, `2026-10-13` inventory |
| IMPL-01 | Haiku worker + coordinator | hash the cached platform-tools tree against the 422 MB v1.48 archive | tree IS v1.48 (byte-identical); cargo-build-sbf 3.1.0-beta.0 | `2026-10-14-m0-toolchain.json` |
| IMPL-02 | coordinator | `cargo +1.92.0 check --locked` on the harness at adoption | exit 101 at `runtime.rs:293` (compile RED recorded, not a test RED) | `2026-10-14-m0-compile-red.json` |
| IMPL-03 | coordinator | add `source-set-digest.sh`, wire into `build-sbf.sh` | 10-file set (`1203c77`) — superseded by IMPL-09 | — |
| IMPL-08 (M1) | coordinator | `cargo test -p musical-chairs-economics` | 77 tests pass; `accumulate_a_remainder`, `seal_denominator`, `sealed_denominator`, `pro_rata` (U256 denominator); `floor_proportional` delegates; `backer_goldens` fixture | `5d9afa5`, `widths.rs` |
| IMPL-05 | coordinator (build executor) | `build-sbf.sh` with the pinned wrapper | stub ELF `d7538b07aa834a28efbf583bf6420244a131a67f02122b469519dad92e91f6ce`, 616,064 B | `2026-10-14-m0-stub-build.json` |
| IMPL-04 | Sonnet worker; coordinator verified | fix `Signature.map`; genesis-shaped fixtures; Backer-PDA keys; computed sequences | `cargo check` clean; `9ecdf70` | commit message |
| IMPL-06 | coordinator | `cargo test -p musical-chairs-financial-runtime` on the pinned ELF | exactly `open_moves_real_sol`, `claim_moves_real_sol`, `allocation_golden_t10000` FAIL with `Custom(6021)` NotImplemented, fee-only rollback, 0 data bytes; 2 pass; CLI 24 observations | `2026-10-14-m0-red-baseline.json`, `-cli.json` |
| IMPL-09 | Haiku worker; coordinator verified, rebuilt | DR-35/C5 correction: 19-file set incl. `crates/economics/{Cargo.toml,src}`, root `Cargo.toml`, helper itself; before/after guard; self-test 8/8; incremental + clean rebuild byte-identical; RED re-run identical; rejection path exercised | complete `source_set_sha256 613eb0c5d7bbda62eaf22ad4d0ff5ff299f15f86e2794f9551a6903a3c8e786f` (old `886b8fa6…` not comparable) | `37f506a`, `8da631c`, `2026-10-14-m0-provenance-guard.json` |
| IMPL-07 | coordinator (sole graph writer) | `graphify update .` + repair + scoped fragment | 8,071/11,422/36 → 10,860/14,919/37; every prior id, raw edge and hyperedge retained; 16 collapsed edges and 321 stripped statuses restored; 366 `same_symbol_as` links | `2026-10-14-m0-graph-refresh.json`, `-fragment.json` |

## Proof level

- **Verbatim adoption and toolchain identity:** hash-verified facts.
- **Compile and genuine RED:** executed locally on the exact compiled ELF in LiteSVM 0.7.1 with valid genesis-shaped fixtures. Nothing is GREEN; no production financial instruction has succeeded in SBF.
- **Provenance:** complete per the corrected DR-35 set; clean build from an empty target directory reproduces the artifact bytes; the mid-build rejection path was exercised with a fake builder (exit 3, artifact moved aside, no receipt).
- **M1:** host-only parity (economics crate); program-side handlers still return `NotImplemented`.
- **Graph:** navigation evidence only.

## Staffing and ownership

Coordinator (Claude Fable 5.1): single build executor (`CARGO_HOME`, harness/program/SBF target dirs under `/private/tmp/mc-solana-runtime-tools-20261013/`), sole writer of the shared graph, verifier of every worker change. Sonnet worker: IMPL-04 harness fix and fixtures. Haiku workers: IMPL-01 toolchain hashing, IMPL-09 scripts. No worker ran cargo, build or git. Source writers were idle during both SBF builds.

## Remaining blockers and open items

- None blocking M2. The plan's M2 (schema freeze and IDL) starts next; M3 carries the Squads discriminator (`controls.rs:438`) and `config_authority` parse fixes.
- `SYNTHETIC_CLOSE_RESERVE` in the harness is a fixture placeholder; the launch reserve value is still a register item. Fixture governance/guardian keys are `Pubkey::default()` until M3's byte-exact Squads fixtures.
- `positions.rs:13` unused imports (`REGISTRY_SEED`, `RegistryPage`) remain; touching them changes the source set, so they are deferred to M2.
- `graphify update .` collapses same-endpoint multi-edges and strips `status` from prior nodes; the repair step must precede every future fragment merge. Tranche symbols exist under two id schemes.
- `docs/coordination/native-solana-supervision.md` (Codex, untracked) was left untouched.
