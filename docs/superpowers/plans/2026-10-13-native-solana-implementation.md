# Native Solana Production Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task once Codex issues its exact final acceptance/go-ahead (Rob delegated plan settlement and local implementation authorization to Codex on 2026-10-13). Steps use checkbox (`- [ ]`) syntax for tracking. **Nothing below is authorized yet.** This plan is for Codex review.

**Goal:** Move the SOL game's custody, accounting, admission, payouts, Backer allocation, promotion and round lifecycle into a native Anchor program, keep the existing ICP-hosted GUI, and feed ICP Ponzi Points from finalized native events.

**Architecture:** One Anchor program with program-owned PDAs is the financial authority (spec §6–§7). The existing React screens call it through a typed adapter and read canonical state from Solana RPC. A new ICP `native_sol_bridge` canister ingests finalized events for PP rewards and history. Keepers continue accepted permissionless work and hold no authority.

**Tech Stack:** Rust 1.92.0 host / Anchor 0.32.1 / Solana SDK 2.3.1 / LiteSVM 0.7.1 (Agave 2.3.13) / cargo-build-sbf 3.1.0-beta.0 + platform-tools (re-pinned in Task 0) / ethnum U256 / `orao-solana-vrf-cb 0.4.0` (`default-features=false, features=["cpi"]`) / existing frontend (web3.js 1.98.4, wallet-adapter 0.15.39, React 18.3.1, Vite 7.3.1, TS 5.6.3, vitest 4.1.7) / Motoko for ICP canisters.

**Spec:** [2026-10-13-native-solana-reconciled-spec.md](../specs/2026-10-13-native-solana-reconciled-spec.md). **Revision 2026-10-13c** after [Codex round-2 review](../../coordination/native-solana-codex-plan-review-round2-20261013.md) (C1 per-set reserve and locked-close provisioning, C2 remaining-lock conservation/provider budget/G returns/owner fallback, C3 two-phase PP protocol and ledger reconciliation, C4 provenance checks and PP acceptance path, C5 canonical source digest and exact ProgramData comparison, C6 wording) layered on **Revision 2026-10-13b** after [Codex review round 1](../../coordination/native-solana-codex-plan-review-20261013.md): M0 (genuine RED via `cargo test` on a pinned stub ELF, corrected toolchain wording), M2 (event-cpi, cluster domain, guardian key, U256 accumulator, captured reserve, Position permanence), M3 (sha2 digest, guardian rotation tests), M4 (closable kinds), M5 (nonce permanence, captured reserve at closing), M6 (six typed phase instructions, alias exceptions, stale rejection, U256 discovery fixture), M7 (cluster-domain seed proof, ORAO roles, provider attribution, split sampler proof), M8 (R2/R3 suites), M9 (inline threshold rule, production SBF CI job), M12 (Shenanigans-owned reservation protocol), M14/M15 (deployed-byte, authority, IDL and ICP served-version evidence; authorized ICP deployment steps). **Decisions:** [decision register](../../coordination/native-solana-decision-register-20261013.md). **Inventory:** [state inventory](../../coordination/native-solana-state-inventory-20261013.md). **Predecessor roadmap:** [2026-10-12-native-solana.md](2026-10-12-native-solana.md) (its Tasks 1–3 are complete as host models; its Tasks 4–8 are replaced by the milestones below).

## Global Constraints

- Economics exactly as spec §3: Front-End Load `floor(P·400/10_000)`; Simple `floor((P·11·elapsed + remainder)/8_640_000)` with 21-day clamp; Carried Interest 1,200/750/300 bps at `<7 d`, `<14 d`, else; Compound15 `floor(P·4_473_565_759_257_037_859/10^18)` fee 900 bps; Compound30 `floor(P·12_267_678_469_131_246_220/10^18)` fee 1,300 bps; minima 10,000,000 / 50,000,000 lamports; quota 3 successes per rolling 3,600 s with exact-hour expiry; strict `owed > pot` close; toll on actual gross paid; split 35/45/20 with folds, sealed post-senior denominator, >10-lamport open cutoff, residue to seed; A ×124/100; B ×116/100; 600 s immutable deadline; 64 counters.
- Widths: u64 cash / u128 lifetime, P&L, global funded claims / U256 weighting (spec §3.6).
- Toolchain: do not upgrade Anchor/SDK/LiteSVM beyond DR-26; no global installs; one build executor at a time; no `@rollup/rollup-darwin-*` in any `package.json`.
- Naming: user-facing "Front-End Load", "Carried Interest", "Jackpot Fee", "Emergency Equity Conversion"; internal `coverCharge`/`exitToll` identifiers in legacy code are not renamed.
- Authorization gates: **Local stage** (Milestones 0–13) begins only on Codex's exact final acceptance/go-ahead (delegated by Rob on 2026-10-13); no further Rob approval is needed for local work. **Devnet stage** (Milestone 14) needs separate authorization for deployment, test-wallet funding and ORAO/Squads registration on devnet. **Mainnet stage** (Milestone 15) needs separate explicit authorization for every deployment, funding and signing action. No milestone pauses, drains or upgrades a legacy canister.
- Branching: work on a new branch `codex/native-solana-production-core` created from `c99a636` in the fresh worktree; the first commit of Task 0 adopts the uncommitted tranche verbatim (so its hashes in the inventory remain the review baseline) before any fix. Planning docs live on `codex/native-solana-plan-20261013` and are merged forward, never force-pushed.
- Every milestone ends with: tests green at the pinned toolchain, `graphify update .` plus a scoped semantic fragment for changed docs, a receipt JSON under `docs/solana/evidence/` with per-file SHA-256 and the measured numbers, and a handoff section in the kanban.
- Explain Solana mechanics in handoffs where a choice depends on them (brief, with ICP/EVM comparison), as the spec does.

---

## Stage map and critical path

```text
M0 workspace ─► M1 host parity ─► M2 schema freeze ─► M3 controls ─► M4 provisioning
      ─► M5 admission+payouts ─► M6 allocation+claims ─► M7 close/draw/finalize
      ─► M8 reserve/refund/cleanup ─► M9 measurement ─► M10 security review ─► [GATE] M14 devnet ─► [GATE] M15 release
Parallel after M2:  M11 frontend adapter (IDL-driven; live tests after M5)
Parallel after M5:  M12 ICP bridge (event schema fixed at M2/M5)
Parallel after M7:  M13 keepers/monitoring
```

Critical path: M0→M1→M2→M3→M5→M6→M7→M8→M9→M10→M14. Rough effort (agent-driven, one coordinator plus workers; assumes no redesign surprises and that measurements at M9 confirm the eight-row chunk or one-row fallback is acceptable):

| Milestone | Effort | Elapsed dependency |
| --- | --- | --- |
| M0 | 1–2 days | — |
| M1 | 1–2 days | M0 |
| M2 | 2–3 days | M1 |
| M3 | 3–5 days | M2 |
| M4 | 2–4 days | M2 |
| M5 | 5–8 days | M3, M4 |
| M6 | 6–10 days | M5 |
| M7 | 6–10 days | M6 |
| M8 | 3–5 days | M7 |
| M9 | 3–5 days | M8 |
| M10 | 3–5 days | M9 |
| M11 | 8–12 days | M2 (IDL), M5 (live) |
| M12 | 8–12 days | M5 (events) |
| M13 | 5–8 days | M7 |
| M14 | 5–8 days + waiting | M10, M11, M12, M13, authorization |
| M15 | 3–5 days + waiting | M14, authorization |

Local stage total roughly 11–17 focused weeks with parallelism; a single sequential executor would be nearer 20 weeks. These are ranges, not commitments; M9 can return a redesign choice that resets later estimates.

---

## Milestone 0 — Workspace, toolchain re-pin, harness compiles (no handler logic)

**Files:** `solana/Cargo.toml` (adopt the two `exclude` entries), `solana/programs/musical_chairs/**` (adopt as-is), `solana/tests/financial-runtime/**` (adopt as-is, then fix compile), `solana/tests/financial-runtime/scripts/build-sbf.sh`, `.github/workflows/native-solana-model.yml` (add a `production-program` job that builds the host harness only; SBF build stays local until Task 9), `docs/solana/evidence/2026-10-NN-m0-toolchain.json`.

**Interfaces produced:** a reproducible `build-sbf.sh` run that records `platform_tools_version`, `cargo_build_sbf_sha256`, `artifact_sha256` and the **canonical source digest** `source_set_sha256` (C5): `SHA-256` over the concatenation, in byte-sorted order of relative path, of `path || 0x00 || SHA-256(content) || 0x0A` for every file in `solana/programs/musical_chairs/{Cargo.toml, Cargo.lock, .cargo/config.toml, src/**/*.rs}`, `solana/tests/financial-runtime/scripts/build-sbf.sh` and `solana/rust-toolchain.toml`. Changing `state.rs` or `allocation.rs` changes it; the current wrapper hashes only `lib.rs` (`build-sbf.sh:7, 76`), so M0 step 5 fixes the wrapper (a harness script, not program source) before the first pinned build; the same digest appears in every build, CI and release receipt. Also a compiling harness binary `musical-chairs-financial-runtime`.

- [ ] **Step 1: Branch and adopt the tranche verbatim**

```bash
cd /Users/robertripley/.codex/worktrees/native-solana-model-probes/musicalchairs
git checkout -b codex/native-solana-production-core
git add solana/Cargo.toml solana/programs/musical_chairs solana/tests/financial-runtime docs/solana/production-accounts.md docs/coordination/kanban.md
git commit -m "chore(solana): adopt uncommitted production account tranche verbatim for review baseline"
```

Expected: the committed blobs hash exactly as the inventory §5 table.

- [ ] **Step 2: Verify the installed platform-tools identity (do not assume v1.52 was used)**

`MC_SBF_BUILDER` is the `cargo-build-sbf` **executable** (`scripts/build-sbf.sh:13,17`). `"$MC_SBF_BUILDER" --version` prints the builder's *default* tools version (v1.52); the wrapper never uses that default because it passes `--tools-version v1.48 --skip-tools-install` and exports `MC_PLATFORM_TOOLS/rust/bin/rustc` (`build-sbf.sh:56–57, 62–70`). Verify the installed tree directly: compare the commit stamps in `"$MC_PLATFORM_TOOLS/version.md"` with the anza `platform-tools` v1.48 release metadata (public, read-only) and record SHA-256 of `rust/bin/rustc`, `rust/bin/cargo` and `llvm/bin/clang`. If they match v1.48, record that and keep the prior 88-case evidence as valid. If they do not, obtain the v1.48 archive (SHA-256 `799e4cfcee38e227155c83fb2294f96f94a843d42e01c0359fb7128e2216d2dd`) into `/private/tmp/mc-solana-runtime-tools-20261013/downloads/`, extract it as the pinned tree, and re-run the runtime probes once. Write the finding and hashes to `docs/solana/evidence/2026-10-NN-m0-toolchain.json`. Do not install anything globally.

- [ ] **Step 3: Write the failing harness compile check**

Run: `cd solana/tests/financial-runtime && cargo +1.92.0 check --locked`
Expected: FAIL at `src/runtime.rs:293` with `no method named 'map' found for struct 'Signature'`.

- [ ] **Step 4: Fix the compile error only**

```rust
// src/runtime.rs:293
signature: Some(signature.to_string()),
```

Run the check again. Expected: PASS (warnings allowed for now).

- [ ] **Step 5: Build and pin the current stub ELF**

The checked artifact `programs/musical_chairs/artifacts/musical_chairs_program.so` (SHA-256 `c9f6016e…3c43`) predates the last source edits and has no provenance. First amend `build-sbf.sh` to emit `source_set_sha256` (definition above) alongside `artifact_sha256` and `lock_sha256`, with a unit check that editing `state.rs` changes the digest. Then build the stub from the adopted source with the wrapper: `MC_SBF_BUILDER=<executable> MC_SBF_SDK=<dir> MC_PLATFORM_TOOLS=<dir> MC_SBF_TARGET_DIR=<owned cache> solana/tests/financial-runtime/scripts/build-sbf.sh`. Record `artifact_sha256`, `source_set_sha256`, `lock_sha256` and the Step 2 tool identity into `docs/solana/evidence/2026-10-NN-m0-stub-build.json`. Export `MC_FINANCIAL_SBF=<that artifact>` for every harness run below.

- [ ] **Step 6: Repair fixture validity so RED means `NotImplemented`, not a broken fixture**

Fix only the harness (`financial-runtime/src/runtime.rs`): genesis-shaped `Ticket` (`phase = IDLE`, `id = 0`), `Pool.snapshot_pages = 1` when a page is injected, `Pool.next_sequence = 5` after four seeded Backers, registry/snapshot/`ticket.oldest` keys as **Backer PDAs** (inventory §5.2 #4–#5, #7), and drop the preloaded discover counts (`seed_allocation`). Run `cargo +1.92.0 check --locked`. These fixture repairs do not touch the program.

- [ ] **Step 7: Run the genuine RED assertions and record them RED**

The evidence CLI (`cargo run … -- all`) collects observations into JSON and exits 0; it is receipt generation, not a failing test. The failing assertions are the `#[test]` functions in `financial-runtime/src/lib.rs:31–59`.

Run: `MC_FINANCIAL_SBF=<stub artifact> cargo +1.92.0 test --locked -p musical-chairs-financial-runtime -- --test-threads=1`
Expected: exactly three failures — `open_moves_real_sol`, `claim_moves_real_sol`, `allocation_golden_t10000` — each whose `Observation` shows the program's custom error for `ErrorCode::NotImplemented` (Anchor custom codes start at 6000; record the exact number from `lib.rs`'s enum order) and **fee-only rollback** (`assert_rollback_except_fee` holds, i.e. all accounts except the payer unchanged); `fee_only_rollback_is_observed_for_the_current_open_gate` and `allocation_six_phase_flow_has_unique_row_observations` PASS. An account/PDA/ELF-load failure is **not** RED; fix the fixture until the only failure is `NotImplemented`. Then run the CLI once with `MC_FINANCIAL_EVIDENCE_PATH=docs/solana/evidence/2026-10-NN-m0-red-baseline.json` to save the receipt. This is the RED half of the RED→GREEN proof that Milestones 5–6 must flip with the same test names.

- [ ] **Step 8: Commit**

```bash
git add solana/tests/financial-runtime/src/runtime.rs docs/solana/evidence/2026-10-NN-m0-*.json
git commit -m "fix(financial-runtime): compile harness, repair fixtures, pin stub ELF and record genuine RED baseline"
```

**Graph:** `graphify update .` (AST now covers `solana/programs/musical_chairs/src/*.rs` and the harness); semantic fragment for `production-accounts.md` with status `draft_unimplemented` on the ten stub handlers. **Handoff:** M0 receipt with the pin decision.

---

## Milestone 1 — Host parity: reference model matches the production width contract

**Files:** Modify `solana/crates/economics/src/backers.rs:141` (`a_denominator: u128` → `U256`), `:162` (`global_funded_claims` stays u128), add `solana/crates/economics/tests/widths.rs`; `solana/fixtures/economics.json` gains the three Backer goldens so the SBF harness can read one fixture file.

**Interfaces:** `pub fn backers::sealed_denominator(rows: &[u128], senior_credit: u64) -> U256`, `pub fn backers::pro_rata(slice: u64, weight: u128, denominator: U256) -> Result<u64, Error>` exported for the program to import (production currently re-implements `floor_proportional` in `allocation.rs:149`; after this task it calls the crate).

- [ ] **Step 1: Failing test — denominator above u128**

```rust
// solana/crates/economics/tests/widths.rs
use musical_chairs_economics::backers::{pro_rata, sealed_denominator};
use ethnum::U256;

#[test]
fn sealed_denominator_sums_past_u128_without_overflow() {
    let rows = vec![u128::MAX, u128::MAX, 7];
    let d = sealed_denominator(&rows, 0).expect("U256 sum");
    assert_eq!(d, U256::from(u128::MAX) * 2 + 7);
}

#[test]
fn pro_rata_floors_and_fits_u64() {
    // slice 2,250 on weights 4,000/10,000 sealed after 1,750 senior -> 2,250 * 2,250 / 8,250 = 613
    let d = sealed_denominator(&[4_000, 6_000], 1_750).unwrap();
    assert_eq!(pro_rata(2_250, 4_000 - 1_750, d).unwrap(), 613);
    assert_eq!(pro_rata(2_250, 6_000, d).unwrap(), 1_636);
}
```

Run: `cargo +1.92.0 test --manifest-path solana/Cargo.toml --test widths --locked`
Expected: FAIL (functions do not exist).

- [ ] **Step 2: Implement** the two functions in `backers.rs` using `ethnum::U256` checked ops; widen the host `Book`'s discovery accumulator (`a_denominator: u128`, `backers.rs:141`) to U256 so the host oracle and program share the width; route `discover_chunk_inner` and `allocate_chunk_inner` through them.
- [ ] **Step 3: Run the full economics workspace**: `cargo +1.92.0 test --manifest-path solana/Cargo.toml --workspace --locked` → 72 prior tests + new ones PASS; `cargo clippy ... -D warnings` PASS; `cargo fmt --check` PASS.
- [ ] **Step 4: Replace production `floor_proportional` body** with a call into the crate (keep the golden test at `allocation.rs:389–394`). `cargo +1.92.0 check` on the program.
- [ ] **Step 5: Commit** `feat(economics): U256 sealed denominator shared by host model and program`.

**Graph:** AST update; link `allocation.rs::floor_proportional → economics::pro_rata` (EXTRACTED). **Handoff:** test counts (expected 72 → ≥ 74).

---

## Milestone 2 — Schema freeze and IDL

**Files:** Modify `solana/programs/musical_chairs/src/state.rs` (add `ConfigVersion`, `CloseRecord` with captured-reserve fields, `ParticipantPage`, `CandidatePage`, `RngAuthority`; rent components; `Pool.cluster_domain [u8;32]`; `Admission.guardian_vault` as the authoritative key; `Position.config_version` and `Position.accrued_gross`; `Pool.funded_claims: u128`; `Ticket.a_remainder_total` accumulated as U256; move closing fields; replace unused `*_SHARE_BPS` consts with use sites), `Cargo.toml` (enable the `event-cpi` anchor feature), `src/lib.rs` (new seeds; `#[event_cpi]` on every emitting `Accounts` struct), create `src/events.rs` (all `#[event]` structs from spec §7.5 with `schema_version: u8`), `src/error.rs` (split `ErrorCode` out of `lib.rs`; add `NothingToClaim`, `InsufficientManagement`, `ReserveUnavailable`, `PositionRoundClosed`, `DuplicateWritableAccount`, `LateCapture`, `Collision`, `WrongProviderTuple`, `ConfigNotActivatable`). Create `solana/programs/musical_chairs/idl/musical_chairs.json` (generated) and `solana/scripts/gen-idl.sh`.

**Interfaces produced (consumed by every later milestone):** seeds `POOL_SEED..CANDIDATES_SEED`, `RNG_AUTHORITY_SEED`, `CONFIG_SEED`, `CLOSE_SEED`, `PARTICIPANTS_SEED`; `ConfigVersion { effective_round_id: u64, plan_rates_bps_per_day: [u16;3], plan_durations_s: [u32;3], simple_toll_bps: [u16;3], simple_toll_day_bounds: [u16;2], jackpot_bps: [u16;2], front_end_load_bps: u16, min_game: u64, min_a: u64, quota_count: u8, quota_window_s: u32, seed_bps: u16, senior_bps: u16, a_bps: u16, b_bps: u16, a_entitlement_bps: u16, b_entitlement_bps: u16, open_cutoff: u64, deadline_seconds: u32, provider_request_fee_lamports: u64, provider_fee_headroom_bps: u16, pending_request_bytes: u16, fulfilled_request_bytes: u16, provider_program: Pubkey, provider_network_state: Pubkey, provider_treasury: Pubkey, provider_client: Pubkey, public_series_a_enabled: bool, page_capacity: u8 }`; `CloseRecord { …, required_reserve: u64, reserve_schedule: ReserveSchedule, required_pages_per_set: u64, existing_preferred_pages: u64, existing_fallback_pages: u64, provisioned_preferred_pages: u64, provisioned_fallback_pages: u64, registry_extension_required: bool, locked_sponsor_remaining: u64, locked_owner_remaining: u64, approved_provider_fee_cap: u64, spent: SpentLedger /* ten u64: candidate_rent_s/o, registry_rent_s/o, b_rent_s/o, provider_fee_s/o, request_rent_s/o */, request_rent_funded: u64, request_rent_returned: u64, owner_request_rent_refundable: u64, reserve_settled: bool }`; `Ops { free_balance, locked_remaining_total, unallocated_excess, protected_rent, provider_fee_consumed_total: u128, request_rent_outstanding: u64, provider_returned_total: u128, provider_deficit: u64, client_balance_observed: u64, … }` and pages as spec §6; `RentComponents { ops: u64, owner: u64, owner_refund: Pubkey }` embedded; `pub fn required_close_reserve(participant_count: u64, registry_len: u64, existing_preferred_pages: u64, existing_fallback_pages: u64, schedule: &ReserveSchedule, rent: &Rent) -> Result<u64>` (spec §8.1, per-set saturating subtraction, all u64).

- [ ] **Step 1: Failing layout tests** in `state.rs` tests module: for each new account, `assert_eq!(T::SPACE, 8 + borsh_len(default))`; a test that `ConfigVersion::default_v1()` encodes the Global Constraints values exactly (400, 10_000_000, 50_000_000, 3, 3600, 5000/3500/4500/2000, 12400, 11600, 10, 600, 8); `required_close_reserve` unit vectors: participants 8 → 1 page per set (2 total), 9 → 2 per set (4 total); registry_len 8 → extension 1, 9 → 0; uneven pre-provisioning (preferred over-provisioned by 3, fallback absent → missing_preferred 0, missing_fallback = required) ; over-provisioning never underflows (saturating); participant_count 262,145 → 32,769 pages per set in u64 (arithmetic-only fixture, no accounts); one-lamport arithmetic exactness; `cluster_domain` round-trips and has no setter. Run `cargo +1.92.0 test -p musical-chairs-program --lib` → FAIL (types missing).
- [ ] **Step 2: Add the types** with `#[derive(InitSpace)]`, bump-first layout, and the fields in spec §6. Make `Pool.funded_claims: u128`.
- [ ] **Step 3: Run tests** → PASS. Build the program host-side (`cargo check`).
- [ ] **Step 4: Generate the IDL** with the pinned Anchor (`anchor idl build` is unavailable without the CLI; use `anchor-lang`'s `idl-build` feature via `cargo test --features idl-build -- __anchor_private_print_idl` as documented for 0.32, captured by `gen-idl.sh`). Verify the IDL lists every event and the `event_authority`/`program` accounts on each emitting instruction. Commit the JSON plus `solana/fixtures/event-encodings.json` (one Borsh vector per event); add a CI step that regenerates and diffs the IDL.
- [ ] **Step 5: Commit** `feat(program): freeze account schema, events and IDL v1`.

**Graph:** AST update; semantic fragment linking each account node to its spec §6 row (`implements`, EXTRACTED). **Handoff:** IDL SHA-256; account sizes table (first real numbers for DR-27's rent model).

Solana note for the handoff: the IDL is Solana's equivalent of a Candid file, generated from the program; the frontend client in M11 is generated from it, so freezing it here is what lets M11 run in parallel.

---

## Milestone 3 — Authenticated controls under LiteSVM

**Files:** Modify `src/controls.rs` (fix discriminator `:438`, parse `config_authority: Pubkey` `:446`, `InsufficientManagement` `:246`, add `stage_config_version`, `activate_config_version`, `rotate_recipient`, `rotate_guardian`); create `solana/tests/financial-runtime/src/controls.rs` (LiteSVM suite), `solana/tests/financial-runtime/fixtures/squads/{multisig_g.bin,multisig_p.bin,README.md}` (byte-exact `Multisig` accounts built from the pinned Squads v4 source layout, with the derivation documented), `fixtures/loader/{program.bin,programdata.bin}` (loader-shaped fixtures as the runtime probes already do).

**Interfaces:** `controls::validate_profile(&AccountInfo, expected_delay: u32, expected_threshold: u8, expected_members: usize) -> Result<Pubkey /*vault*/>`; `controls::require_vault_signer(multisig, vault_index=0, signer)`.

- [ ] **Step 1: Failing unit test — discriminator derived, not hard-coded**

```rust
#[test]
fn multisig_discriminator_matches_anchor_namespace() {
    let d = <sha2::Sha256 as sha2::Digest>::digest(b"account:Multisig"); // sha2 =0.10.9 is already pinned; anchor_lang 0.32.1 does not re-export solana_program::hash
    assert_eq!(&d[..8], &controls::SQUADS_MULTISIG_DISCRIMINATOR);
}
```

Run → FAIL (`…0xc8` vs `…0xec`). Fix the constant. PASS.

- [ ] **Step 2: Failing unit test — autonomous layout**: encode a `Multisig` with `config_authority = Pubkey::default()`, `threshold = 2`, `time_lock = 172_800`, three `Member{key, permissions: 7}` entries using the field order from `programs/squads_multisig_program/src/state/multisig.rs` at the pinned commit (document commit hash in `fixtures/squads/README.md`); assert `validate_profile` returns the vault PDA `find_program_address(&[b"multisig", multisig.key, b"vault", &[0]], SQUADS_ID)`. Run → FAIL (Option tag desynchronizes). Rewrite the parser. PASS.
- [ ] **Step 3: Failing LiteSVM tests** (`controls.rs` suite), each asserting exact error and fee-only rollback:
  - `initialize_succeeds_with_g_vault_signer_and_matching_programdata`
  - `initialize_rejects_first_caller_without_g_vault` → `Unauthorized`
  - `initialize_rejects_programdata_authority_mismatch`
  - `initialize_rejects_one_of_three_profile` (threshold 1) → `WrongGovernanceProfile`
  - `initialize_sets_pot_seed_zero_round_one_next_close_one`
  - `set_admission_pause_touches_only_admission_account` (assert the instruction's account metas are exactly `[Admission (writable), P vault (signer)]`; no writable financial account)
  - `set_admission_pause_rejects_non_guardian`
  - `rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted` (assert `Admission.guardian_vault == Pool.guardian_vault == new` after one instruction)
  - `rotate_guardian_recovers_lost_p_quorum` (old P never signs again; G alone rotates)
  - `withdraw_management_pays_recipient_only_and_caps_at_accrued` (seed `management = 1_000` via a test-only fixture write, then withdraw 1_001 → `InsufficientManagement`; withdraw 1_000 → vault −1_000, recipient +1_000)
  - `fund_operations_any_signer_increases_free_balance_only`
  - `withdraw_unreserved_operations_respects_locked_and_rent`
  - `stage_config_requires_g_and_future_round` (effective = current → `ConfigNotActivatable`)
  - `activate_config_only_when_active_no_ticket_and_round_reached`
  - `rotate_recipient_requires_g`
  - `initialize_stores_cluster_domain_immutably` (no instruction can change it; devnet and mainnet genesis vectors differ)
  Run → FAIL (handlers missing / fixtures missing).
- [ ] **Step 4: Implement** the new handlers and fixture builders. Run → PASS. Conservation check after every test: `ops_cash.lamports - rent == free + locked`.
- [ ] **Step 5: Commit** `feat(program): authenticated controls with byte-exact Squads profile fixtures`.

**Runtime proof:** LiteSVM transactions for every row above with CU and packet bytes captured into the receipt. **Graph:** AST + fragment marking `validate_profile` `local_runtime_executed`. **Handoff:** note that real Squads execution and deployed readback remain M14.

---

## Milestone 4 — Provisioning, rent components, cleanup

**Files:** Modify `src/controls.rs` (generalize `provision_snapshot/registry` into `provision_page(kind, index)` + `provision_close_record`; add `close_shared_account(kind)`), `src/lib.rs::create_shared` (record `RentComponents`), harness `src/provisioning.rs`.

**Interfaces:** `PageKind { Registry, Snapshot, Participant{round}, Candidate{close_id, set_tag} }`; `provision_page(kind, index, funding: Funding::{Ops, Caller})` (candidate pages may be provisioned for `pool.next_close_id`); `close_shared_account(kind)` accepts only `Participant{round}` (predicate: that round's `CloseRecord.finalized`) and `Candidate{close_id, set}` (predicate: `CloseRecord(close_id).promotion` terminal); every other kind → `NotClosable`.

- [ ] **Step 1: Failing LiteSVM tests**: `provision_snapshot_sequential_only` (index 2 before 1 → `WrongIndex`); `provision_from_ops_debits_free_balance_by_exact_rent`; `provision_by_caller_records_owner_component_and_refund_destination`; `close_snapshot_and_registry_pages_always_refused_not_closable` (R3: tail and middle attempts, before and after a ticket, all `NotClosable`); `snapshot_pages_counter_never_decrements`; `close_candidate_page_refused_until_its_own_close_record_is_terminal` (an older close's page cannot be closed by a newer ticket's state); `close_participant_page_refused_until_that_round_finalized`; `close_candidate_page_refunds_components_once` (second call → `AlreadyClosed`); `close_close_record_always_refused`; `provision_close_record_requires_next_close_id`; `provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only` (preferred pages do not reduce fallback's missing count); `provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent` (all free Ops locked at close, `free_balance == 0`, provisioning still succeeds from `locked_sponsor_remaining`); `provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice`; `provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set`.
- [ ] **Step 2: Implement.** Rent read from `Rent::get()`, never a constant (DR-27).
- [ ] **Step 3: PASS; commit** `feat(program): generic page provisioning, rent components, safe cleanup`.

**Graph/Handoff:** measured rent per account kind at local default rent, labeled as local default (6,960 coefficient) distinct from mainnet schedule.

---

## Milestone 5 — Admission and payouts (RED→GREEN on real lamports)

**Files:** Modify `src/positions.rs` (replace stubs `:319, :323, :331` with handlers; split `claim_position` into `withdraw_simple`, `settle_compounding`, `close_simple_position`; **no** `close_position_account` — Positions are permanent, DR-30; add `ParticipantPage` and `CloseRecord(next)` accounts to `OpenPosition`/payout structs; add `max_work_contribution_lamports` and `expected_round_id`/`expected_config` args; compute and lock `required_close_reserve` on the closing path), reuse `/private/tmp/mc-positions-handlers-20261013.rs` logic after fixes (call `allocation::fold_slices`, drop dead assignments, write `User.b_entitlement` prospective check only, use `rounds::PROMOTION_DEADLINE_SECS`), harness `src/financial.rs` (fix `:44` sequence assertion; fixtures use Backer PDAs and genesis-shaped `Pool`/`Ticket`).

**Interfaces:** `OpenPositionArgs { plan: u8, gross: u64, client_nonce: u64, expected_round_id: u64, expected_config: u32 }`; `PayoutArgs { minimum_receive: u64, max_work_contribution_lamports: u64 }` with `destination` as an account; events `PositionOpened`, `SeriesAFunded`, `PayoutPaid`, `RoundClosed`.

- [ ] **Step 1: Flip the RED baseline — failing LiteSVM tests with exact lamports**

```rust
// solana/tests/financial-runtime/src/financial.rs
#[test]
fn open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol() {
    let mut w = World::genesis();                       // initialized program, funded owner
    let before = w.lamports(w.game_vault);
    w.open_position(owner, Plan::Simple, 1_000_000_000, nonce = 1).expect("open");
    assert_eq!(w.lamports(w.game_vault) - before, 1_000_000_000);
    let pool = w.pool();
    assert_eq!(pool.management, 40_000_000);
    assert_eq!(pool.pot, 960_000_000);
    assert_eq!(w.position(owner, 1).gross, 1_000_000_000);   // accrual base is GROSS
    assert_eq!(w.user_round(owner, 1).gross_deposits, 1_000_000_000);
    assert_eq!(w.participants(1).rows[0], owner);
    assert_eq!(pool.next_first_funding_seq, 1);                 // unchanged by game open
}

#[test]
fn withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000() {
    let mut w = World::genesis_with_simple_position_and_one_open_backer(1_000_000_000);
    w.warp_seconds(86_400);
    let before = w.lamports(dest);
    w.withdraw_simple(owner, nonce = 1, minimum_receive = 96_800_000, dest, max_contrib = 0).expect("pay");
    assert_eq!(w.lamports(dest) - before, 96_800_000);           // no 5,000 deduction
    let pool = w.pool();
    assert_eq!(pool.pot, 960_000_000 - 110_000_000);
    assert_eq!(pool.pending_toll, 13_200_000);
    assert!(pool.ticket_active);                                 // one open Backer: toll escrowed and ticket created
}

#[test]
fn toll_with_no_backers_goes_straight_to_seed_without_ticket() {
    // registry_len == 0: whole toll -> seed, no ticket (reference-model shortcut, spec §3.5)
    let mut w = World::genesis_with_simple_position(1_000_000_000);
    w.warp_seconds(86_400);
    w.withdraw_simple(owner, 1, 0, dest, 0).unwrap();
    let pool = w.pool();
    assert_eq!(pool.seed, 13_200_000);
    assert_eq!(pool.pending_toll, 0);
    assert!(!pool.ticket_active);
}

#[test]
fn minimum_receive_failure_is_atomic_and_fee_only() {
    let mut w = World::genesis_with_simple_position(1_000_000_000);
    w.warp_seconds(86_400);
    let snap = w.snapshot_all();
    let err = w.withdraw_simple(owner, 1, 96_800_001, dest, 0).unwrap_err();
    assert_eq!(err, ProgramError::MinimumReceiveNotMet);
    assert_eq!(w.snapshot_all().except_payer_fee(5_000), snap);
}

#[test]
fn strict_insolvency_closes_and_equality_does_not() {
    // pot exactly equals owed -> solvent, pot ends 0, round stays Active
    let mut w = World::genesis();
    w.set_pot_fixture(110_000_000);                 // test-only helper writes Pool.pot and vault lamports together
    w.open_fixture_simple(owner, 1_000_000_000);    // fixture position without touching pot
    w.warp_seconds(86_400);
    w.withdraw_simple(owner, 1, 0, dest, 0).unwrap();
    assert_eq!(w.pool().pot, 0);
    assert!(matches!(w.pool().phase, Phase::Active));
    // one lamport less -> closing
    let mut w2 = World::genesis(); w2.set_pot_fixture(109_999_999); w2.open_fixture_simple(owner, 1_000_000_000);
    w2.provision_close_record(1);
    w2.warp_seconds(86_400);
    w2.withdraw_simple(owner, 1, 0, dest, 0).unwrap();
    assert!(matches!(w2.pool().phase, Phase::Closing));
    let cr = w2.close_record(1);
    assert_eq!(cr.promotion_deadline, cr.close_started_at + 600);
    assert_eq!(cr.participant_count, 1);
    assert_eq!(cr.required_reserve, w.expected_reserve(participants = 1, registry_len = 0, existing_pages = 0)); // R2: captured, not configured
    assert_eq!(cr.locked_sponsor + cr.locked_owner, cr.required_reserve);
}

#[test]
fn closing_reserve_scales_with_participants_and_registry_tail() {
    // R2 boundaries: participants 8 -> 2 candidate pages, 9 -> 4; registry_len 8 -> extension page, 9 -> none
    for (m, n, per_set, ext) in [(8u64, 8u64, 1u64, true), (9, 9, 2, false), (1, 0, 1, true)] {
        let w = World::closing_fixture(participants = m, registry_len = n);
        let cr = w.close_record(1);
        assert_eq!(cr.required_pages_per_set, per_set);
        assert_eq!(cr.registry_extension_required, ext);
        assert_eq!(cr.locked_sponsor_remaining + cr.locked_owner_remaining, cr.required_reserve); // remaining == unspent at lock time
    }
}

#[test]
fn closing_payout_short_by_one_lamport_rejects_before_mutation() {
    let mut w = World::closing_fixture(participants = 9, registry_len = 8);
    w.set_ops_free(w.expected_reserve(9, 8, 0) - 1);
    let snap = w.snapshot_all();
    assert_eq!(w.withdraw_simple(owner, 1, 0, dest, 0).unwrap_err(), ProgramError::ReserveUnavailable);
    assert_eq!(w.snapshot_all().except_payer_fee(5_000), snap);
    w.set_ops_free(w.expected_reserve(9, 8, 0) + 1_000_000);               // spare cash is not mistaken for a smaller obligation
    w.withdraw_simple(owner, 1, 0, dest, 0).unwrap();
    assert_eq!(w.close_record(1).locked_sponsor_remaining, w.expected_reserve(9, 8, 0));
    assert_eq!(w.ops().free_balance, 1_000_000);
}

#[test]
fn compound_15_maturity_vector() {
    let mut w = World::genesis_with_position(Plan::Compound15, 1_000_000_000);
    w.set_pot_fixture(10_000_000_000);
    w.warp_seconds(15 * 86_400 - 1);
    assert_eq!(w.settle_compounding(owner, 1, 0, dest, 0).unwrap_err(), ProgramError::NotMature);
    w.warp_seconds(1);
    w.settle_compounding(owner, 1, 0, dest, 0).unwrap();
    assert_eq!(w.last_event::<PayoutPaid>().gross_paid, 4_473_565_759);
    assert_eq!(w.last_event::<PayoutPaid>().toll, 402_620_918);
    assert_eq!(w.last_event::<PayoutPaid>().net, 4_070_944_841);
}

#[test]
fn quota_third_success_blocks_fourth_until_exact_hour() {
    let mut w = World::genesis();
    for n in 1..=3 { w.open_position(owner, Plan::Simple, 10_000_000, n).unwrap(); }
    assert_eq!(w.open_position(owner, Plan::Simple, 10_000_000, 4).unwrap_err(), ProgramError::QuotaExceeded);
    w.warp_seconds(3_599);
    assert_eq!(w.open_position(owner, Plan::Simple, 10_000_000, 4).unwrap_err(), ProgramError::QuotaExceeded);
    w.warp_seconds(1);
    w.open_position(owner, Plan::Simple, 10_000_000, 4).unwrap();      // exactly one hour old has expired
}

#[test]
fn failed_open_does_not_consume_quota_and_nonce_replay_fails() {
    let mut w = World::genesis();
    assert_eq!(w.open_position(owner, Plan::Simple, 9_999_999, 1).unwrap_err(), ProgramError::BelowMinimum);
    w.open_position(owner, Plan::Simple, 10_000_000, 1).unwrap();
    assert_eq!(w.open_position(owner, Plan::Simple, 10_000_000, 1).unwrap_err(), ProgramError::AccountAlreadyInitialized);
    assert_eq!(w.user(owner).successful_calls(), 1);
}

#[test]
fn fund_series_a_full_gross_to_pot_entitlement_124() {
    let mut w = World::genesis();
    w.fund_series_a(owner, 50_000_000, 1).unwrap();
    assert_eq!(w.pool().pot, 50_000_000);
    assert_eq!(w.pool().management, 0);
    assert_eq!(w.backer(owner, Series::A).entitlement, 62_000_000);
    assert_eq!(w.backer(owner, Series::A).first_funding_seq, 1);
    assert_eq!(w.registry(0).rows[0], (owner, Series::A));
    assert!(w.user(owner).has_ever_a);
}

#[test]
fn fund_series_a_rejected_while_ticket_active_and_when_flag_off() { /* ticket_active fixture -> TicketActive; config flag false -> PublicSeriesADisabled */ }

#[test]
fn stale_round_position_is_terminal_without_write() { /* round advanced fixture -> PositionRoundClosed on withdraw/settle/close_simple; Position bytes unchanged; no reclamation instruction exists */ }

#[test]
fn same_nonce_cannot_open_a_second_position_after_settlement() {
    // R1: Position is permanent, so the PDA is the nonce tombstone
    let mut w = World::genesis_with_simple_position(1_000_000_000);  // nonce 1
    w.warp_seconds(21 * 86_400);
    w.withdraw_simple(owner, 1, 0, dest, 0).unwrap();                 // matured -> Closed
    w.advance_round_fixture();                                         // round invalidation path
    assert_eq!(w.open_position(owner, Plan::Simple, 10_000_000, 1).unwrap_err(), ProgramError::AccountAlreadyInitialized);
    w.open_position(owner, Plan::Simple, 10_000_000, 2).unwrap();     // a different nonce remains valid
    assert_eq!(w.user(owner).successful_calls(), 1);                   // failed replay consumed no quota
}

#[test]
fn unknown_outcome_retry_with_same_nonce_is_idempotent() { /* submit open; simulate unknown client outcome; resubmit identical nonce -> AccountAlreadyInitialized; exactly one Position funded; pot credited once */ }

#[test]
fn fund_series_a_retry_after_unknown_outcome_keeps_original_signature() { /* client keeps signature, reads finality, does not auto-resubmit; a deliberate second funding merges by accepted repeatable ABI */ }

#[test]
fn third_party_fee_payer_gains_no_authority() { /* fee payer != owner: owner still lamport source; destination fixed by owner; payer cannot open for owner without owner signature -> MissingRequiredSignature */ }

#[test]
fn destination_cannot_be_game_vault_ops_cash_or_program_pda() { /* each -> ProtectedDestination */ }
```

Run: `cargo +1.92.0 run --locked --bin musical-chairs-financial-runtime -- positions`
Expected: all FAIL with `NotImplemented` (the M0 RED baseline).

- [ ] **Step 2: Implement** the handlers from the fixed draft logic. Keep the lazy-error rule (`ok_or_else`) in every loop. Write `User.b_entitlement` only via the prospective check (promotion writes it in M7). Simple accrual follows spec §3.2: `increment` added to `accrued_gross`, `owed = accrued_gross − gross_paid`, so a partial claim never drops unpaid earnings.
- [ ] **Step 3: Run → PASS.** Record CU/packet per instruction into the receipt.
- [ ] **Step 4: Property and differential tests** `tests/payout_props.rs`: for random `(P ∈ [10^7, 10^15], elapsed ∈ [0, 21 d], pot ∈ [0, 2^63])` check `gross_paid + (pot - gross_paid) == pot`, `toll + net == gross_paid`, and that the SBF handler and `economics::Payout::from_gross` agree on 200 sampled cases executed through LiteSVM. The explicit production differential matrix: repeated Simple claims (remainder carry and `accrued_gross − gross_paid`), backwards time, fee boundaries at day 7/14 ± 1 s, the 21-day clamp, Compound30 maturity, owed above u64 narrowed against pot, and only-B folds/caps/dust.
- [ ] **Step 5: Commit** `feat(program): admission, Series A funding and owner payouts with closing pin`.

**Runtime proof:** RED baseline JSON (M0) and GREEN JSON (this task) with identical test labels. **Graph:** AST + fragment flipping the three handler nodes from `draft_unimplemented` to `local_runtime_executed`. **Handoff:** vector table actual vs expected, CU table.

---

## Milestone 6 — Allocation ticket and combined claims

**Files:** Modify `src/allocation.rs` (replace stubs `:357–381`; port `/private/tmp/mc-allocation-handlers-20261013.rs` after fixing: same-page swap-delete (`:464–515`), inverted alias write (`:517–530`), missing `Pool` import, `use super` path; add the duplicate-independent-row guard with the two legitimate alias exceptions; keep the draft's **six typed instructions** (DR-29) renamed `discover_rows`, `apply_senior`, `seal_weights`, `allocate_rows`, `prune_rows`, `commit_ticket`, each with `expected_ticket_id` and, for row phases, `expected_cursor` and `rows`; accumulate `a_remainder_total` as U256), `src/claims.rs` (new: `claim_backer` with lazy materialization over A and B), harness `src/allocation.rs`.

**Interfaces:** `discover_rows(expected_ticket_id, expected_cursor, rows)` accounts `[Ticket, Pool, RegistryPage(s), SnapshotPage(s), Backer×rows]`; `apply_senior(expected_ticket_id)` `[Ticket, Backer(oldest)]`; `seal_weights(expected_ticket_id)` `[Ticket]`; `allocate_rows(expected_ticket_id, expected_cursor, rows)` `[Ticket, SnapshotPage(s), Backer×rows]`; `prune_rows(expected_ticket_id, expected_cursor, rows)` per logical row `[SnapshotPage, Backer(pruned), RegistryPage(pruned slot), RegistryPage(last slot), Backer(moved)]` with aliases allowed exactly where spec §7.3 says; `commit_ticket(expected_ticket_id)` `[Ticket, Pool, CloseRecord?]`. Stale `expected_*` → `StaleTicket`/`StaleCursor`.

- [ ] **Step 1: Failing LiteSVM goldens** (the M0 RED `allocation_golden_t10000` plus):

```rust
#[test]
fn golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500() {
    let mut w = World::with_backers(&[(a1, A, 4_000), (a2, A, 6_000), (b1, B, 1_000_000), (b2, B, 1_000_000)]);
    w.start_ticket_fixture(10_000);
    w.advance_until_idle(rows = 8);
    assert_eq!(w.pool().seed, 5_001);
    assert_eq!(w.backer(a1, A).pending_credit, 2_363);
    assert_eq!(w.backer(a2, A).pending_credit, 1_636);
    assert_eq!(w.backer(b1, B).pending_credit, 500);
    assert_eq!(w.backer(b2, B).pending_credit, 500);
    assert_eq!(w.pool().global_funded_claims, 2_363 + 1_636 + 500 + 500);
    assert_eq!(w.pool().pending_toll, 0);
}

#[test]
fn golden_no_b_a1000_a10000_seed_5750_a1000_a3250() { /* ... */ }

#[test]
fn golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed() { /* ... */ }

#[test]
fn chunk_sizes_1_4_8_produce_identical_state() {
    for rows in [1u8, 4, 8] {
        let mut w = World::with_random_backers(seed = 7, n = 37);
        w.start_ticket_fixture(123_457);
        w.advance_until_idle(rows);
        assert_eq!(w.digest_financial_state(), EXPECTED_DIGEST_37_123457); // computed once from the host oracle
    }
}

#[test]
fn replaying_a_completed_discover_chunk_fails_without_credit() { /* cursor mismatch -> StaleCursor; snapshot unchanged */ }

#[test]
fn stale_ticket_id_is_rejected_on_every_phase_instruction() { /* expected_ticket_id = previous ticket -> StaleTicket for all six */ }

#[test]
fn discovery_accumulates_a_remainders_above_u128_in_u256() {
    // two A rows with remaining u128::MAX each (fixture entitlements), plus one small row
    let mut w = World::with_backers(&[(a1, A, u128::MAX), (a2, A, u128::MAX), (a3, A, 7)]);
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(Phase::SealWeights, 8);
    assert_eq!(w.ticket().a_remainder_total, U256::from(u128::MAX) * 2 + 7);
    w.seal_weights().unwrap();
    assert_eq!(w.ticket().sealed_a_denominator, U256::from(u128::MAX) * 2 + 7 - U256::from(w.ticket().senior_credit));
}

#[test]
fn prune_same_page_swap_delete_clears_last_slot_on_first_page() {
    // regression for draft bug: pruned slot 1 and last slot 3 both on page 0
    let mut w = World::with_backers(&[(a1, A, 100_000), (a2, A, 5), (a3, A, 100_000), (a4, A, 100_000)]);
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(Phase::Prune, 8);
    w.prune_rows(w.ticket().ticket_id, w.ticket().prune_cursor, 1).unwrap();  // prunes a2 (remaining 5 <= 10)
    assert_eq!(w.registry(0).rows[1], (a4, A));         // a4 moved into slot 1
    assert_eq!(w.registry(0).rows[3], EMPTY);
    assert_eq!(w.backer(a4, A).registry_slot, Some(1));
    assert_eq!(w.backer(a2, A).registry_slot, None);     // regression for inverted alias write
    assert_eq!(w.pool().registry_len, 3);
}

#[test]
fn prune_alias_when_pruned_row_is_last_row_marks_it_inactive() { /* moved == pruned */ }

#[test]
fn duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once() {
    // same Backer passed as two independent rows in allocate_rows -> DuplicateWritableAccount
    // prune_rows with RegistryPage(pruned) == RegistryPage(last) succeeds and writes the page once
    // prune_rows with Backer(moved) == Backer(pruned) (last live slot) succeeds, marks it inactive, performs no moved write
}

#[test]
fn claim_during_ticket_pays_only_finalized_and_commit_adds_once() {
    // backer with 100 funded and 20 pending from the active ticket
    assert_eq!(w.claim_backer(owner, dest).unwrap(), 100);
    w.advance_until_idle(8);                             // commit adds 20 to global once
    assert_eq!(w.pool().global_funded_claims, 20);
    assert_eq!(w.claim_backer(owner, dest).unwrap(), 20); // lazy materialization, no second lifetime credit
    assert_eq!(w.backer(owner, A).lifetime_credited, 120);
}

#[test]
fn claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history() { /* has_ever_a stays true; registry_slot None; lifetime unchanged after claim */ }

#[test]
fn reopen_capped_a_keeps_first_funding_seq() { /* fund again -> appended with original seq; seniority preserved */ }

#[test]
fn normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait() { /* open OK; fund_series_a -> TicketActive; withdraw_simple -> TicketActive */ }

#[test]
fn conservation_identity_holds_after_every_phase() { /* assert game_vault - rent == pot+seed+management+pending_toll+global_funded_claims+excess after each advance */ }
```

Run → FAIL (`NotImplemented`).
- [ ] **Step 2: Implement** the six phase instructions and `claim_backer`. Snapshot pages must carry `ticket_id` and be rejected if stale.
- [ ] **Step 3: Run → PASS.** Capture CU and packet bytes for rows 1/4/8 at N = 16, 64, 100, 256 (production rows, not the kernel).
- [ ] **Step 4: Differential test** against the host `Book`: replay 50 random tickets through both and compare per-Backer `pending/lifetime` and `seed`.
- [ ] **Step 5: Commit** `feat(program): bounded allocation ticket, prune fixes, lazy claims`.

**Runtime proof:** GREEN for `allocation_golden_t10000`. **Graph:** flip seven handler nodes; link `allocate_rows → economics::pro_rata`. **Handoff:** CU/packet table; first evidence on whether 8 rows fit with production accounts.

---

## Milestone 7 — Close, candidates, draw, promotion, expiry, finalization

**Files:** Create `src/close.rs` (`build_candidates`, `request_round_draw`, `capture_round_draw`, `apply_promotion`, `expire_promotion`, `finalize_round`), `src/provider.rs` (ORAO CPI wrapper reusing `solana/probes/runtime/program/src/lib.rs` request/capture logic; add `orao-solana-vrf-cb = "=0.4.0", default-features=false, features=["cpi"]` to the program `Cargo.toml` and re-lock), `src/sampling.rs` (ordinal mapping, ported from `economics::rounds::map_output` with a shared test vector file `solana/fixtures/sampling.json`), harness `src/close.rs` with the captured ORAO executable from `solana/probes/runtime/fixtures/` loaded under synthetic fulfill authority as in the probes.

**Interfaces:** instructions as spec §7.4; `CloseRecord` transitions; events `CandidatesSealed`, `DrawRequested`, `DrawCaptured`, `PromotionApplied`, `PromotionSkipped`, `RoundFinalized`.

- [ ] **Step 1: Failing LiteSVM tests**
  - `build_candidates_visits_every_participant_once_and_seals_digests` (M = 20 participants; 7 losers, 3 of them with A history → preferred 4, fallback 7; digest equals SHA-256 over the ordered pubkeys)
  - `build_candidates_rejects_skipped_or_reordered_rows`
  - `same_owner_multi_position_aggregation_and_sealed_userround_immutability` (Codex answer 4: one owner with three positions, two closed, aggregates to one loss figure; `UserRound` and A/B history bits cannot change between seal and `apply_promotion`; a deposit attempt during Closing fails and leaves them unchanged)
  - `sealed_with_zero_losers_skips_no_candidates_without_request`
  - `request_round_draw_creates_request_with_protocol_seed_and_marker_atomically` (assert seed bytes equal the §4 formula computed host-side using `Pool.cluster_domain`; failed CPI leaves `draw_state == NotRequested`)
  - `protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector` (two genesis fixtures; exact 32-byte vectors committed in `solana/fixtures/seed-vectors.json`)
  - `request_decrements_remaining_partitions_sponsor_first_and_records_spent_split` (OpsCash −(fee+rent); `locked_sponsor_remaining` −min; `locked_owner_remaining` −rest; `spent.provider_fee_s/o` and `spent.request_rent_s/o` sum exactly; `ops.provider_fee_consumed_total += fee`; `ops.request_rent_outstanding += rent`; `owner_request_rent_refundable == owner-sourced rent share`)
  - `request_cpi_failure_leaves_partitions_and_ledger_unchanged`
  - `fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting` (Client +4,259,520 and Treasury +300,000 at local default rent as in the runtime receipt; OpsCash, `free_balance`, partitions and owner unchanged; `client_balance_observed` refreshed by `reconcile_ops_cash`)
  - `record_provider_return_pays_owner_refundable_share_first_then_free_and_requires_physical_surplus` (G-signed; amount above outstanding rejected; no double return)
  - `expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later`
  - `g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims`
  - `increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once` (live fee above original cap → `ProviderFeeExceeded`; after budget increase the request succeeds; unused extra returns at settlement; owner partition unchanged)
  - `settlement_refunds_only_remaining_partitions_never_spent_amounts`
  - `anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent`
  - `request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner` (R5: `Client.owner != G`, `Client.state != RngAuthority`, account not owned by ORAO program)
  - `owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution` (owner-funded fee consumed non-refundable; owner-funded rent recorded refundable and paid only through `record_provider_return`)
  - `request_after_deadline_is_rejected_before_fees`
  - `collision_with_preexisting_request_fails_closed_no_alternate_seed`
  - `capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop`
  - `apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116` (fixture output chosen so counter 0 is rejected and counter 1 accepted; winner loss 300,000,000 → B entitlement 348,000,000; `has_ever_b` set; registry append; no cash change)
  - `sampler_all_64_counters_rejected_pure` (pure host sampler test in `src/sampling.rs` with **constructed** sample words, or an N near 2^63 where `limit` leaves most words rejected; no search for a natural small-N all-rejection output)
  - `apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state` (LiteSVM test that writes a clearly labeled synthetic `CloseRecord` whose captured output is pre-mapped to exhaustion by the pure sampler; asserts the terminal transition only; **no** production test-only RNG or winner override exists)
  - `expire_promotion_requires_clock_past_deadline_and_no_capture`
  - `timely_capture_cannot_be_expired_and_applies_after_deadline`
  - `expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash`
  - `finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once` (seed 13,200,000 → pot; `round_id` 1→2; second call Ok no-op)
  - `direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round`
  - `late_authentic_fulfillment_after_skip_cannot_mint_or_revive`
  - `second_close_cannot_reuse_first_close_request_binding`
  Run → FAIL (module missing).
- [ ] **Step 2: Implement.** Every spend decrements `locked_sponsor_remaining` then `locked_owner_remaining` at the instant lamports leave OpsCash and records the split in `spent` (spec §8.3); candidate pages are created only through `provision_page(…, LockedClose)` before `build_candidates` appends to them (C1); `request_round_draw` checks `approved_provider_fee_cap`; `record_provider_return` and `reconcile_ops_cash` implement §8.4–8.5. Promotion must never need a sponsor top-up after a timely capture: assert `apply_promotion` and all `LockedClose` provisioning succeed with `ops.free_balance == 0`.
- [ ] **Step 3: Run → PASS** with the captured ORAO executable; record CU/packet for each instruction.
- [ ] **Step 4: Commit** `feat(program): claim-triggered close, sealed candidates, bound ORAO draw, deadline expiry, finalization`.

**Runtime proof:** full lifecycle receipt from closing payout to reopened round in LiteSVM. **Graph:** fragment linking `CloseRecord` states to availability contract nodes. **Handoff:** explicit limits — one synthetic fulfill authority, no cluster latency.

---

## Milestone 8 — Reserve, refund and cleanup end-to-end; randomized conservation

**Files:** Modify `src/positions.rs` (reserve check order, locked partitions), `src/close.rs` (settle reserve at terminal promotion), `src/controls.rs::close_shared_account` predicates; harness `src/reserve.rs`, `src/fuzz.rs` (deterministic random operation sequences).

- [ ] **Step 1: Failing tests**: `reserve_short_by_one_lamport_rejects_closing_payout_before_mutation`; `required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser` (all preferred also appear in fallback; both sets fill; no mid-close provisioning dependency); `missing_b_account_is_created_from_locked_reserve_with_ops_free_zero`; `registry_tail_full_extension_page_created_from_locked_reserve`; `owner_contribution_covers_program_storage_shortfall_only_never_provider_fee`; `sponsor_partition_consumed_first_then_owner`; `spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation`; `uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages`; `all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds`; `unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination`; `client_reimbursement_never_becomes_free_ops_or_owner_refund_without_record_provider_return`; `conservation_transitions_table_8_6_holds_for_every_row`; `sponsor_withdrawal_cannot_touch_locked`; `snapshot_and_registry_pages_are_not_closable` (R3); `candidate_pages_closable_only_after_their_own_close_is_terminal`; `participant_pages_closable_only_after_their_round_finalized`; `old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts` (R3: nothing can close it at all); `full_cash_progress_with_depleted_free_ops_funds` (commit, finalize, claims succeed with `ops.free_balance == 0`); `cleanup_is_never_required_for_commit_finalize_or_open`; `randomized_300_step_sequence_keeps_both_conservation_identities` (operations drawn from open/fund/withdraw/settle/discover/senior/seal/allocate/prune/commit/claim/build/request/capture/apply/expire/finalize/provision/top_up/close with a fixed RNG seed; every step re-checks both identities; failures dump the sequence).
- [ ] **Step 2: Implement; PASS; commit** `feat(program): reserve partitions, once-only refunds, safe cleanup, randomized conservation`.

---

## Milestone 9 — Measurement and inline feasibility

**Files:** Create `solana/tests/financial-runtime/src/measure.rs`, `docs/solana/measurements-2026-10.md`, receipt `docs/solana/evidence/2026-10-NN-m9-measurements.json`; update `docs/coordination/native-solana-icp-budget.md` with a "measured" section (do not delete the scenario).

- [ ] **Step 1: Matrix** (each row records CU, packet bytes, message keys, writable count, declared account bytes, rent at local default, success): inline candidates N = 0/1/16/17 for a payout that would allocate inline in one transaction (to decide whether an inline path is feasible at all with production accounts); chunk populations N = 64/100/256/1,024 with rows 1/4/8; churn (reopen/prune cycles); dust claims (owed 1–10 lamports); full close path M = 100 participants; keeper transaction count K for the pilot (P = 1,000, C = 10, N = M = 100) computed from measured per-phase counts rather than the `ceil(N/8)` scenario.
- [ ] **Step 2: Decision output (DR-31)**: (a) chunk size: confirm eight-row chunks fit the 1,232-byte envelope with production accounts, or set the default rows to the largest measured fit, with one-row fallback always proved; (b) inline threshold: measure a complete inline allocation inside the payout at N = 0, 1, 2, 4, 8, 16, 17 with production accounts and the event-cpi accounts; enable the inline path for every N up to the largest N that fits packet, 64-account-lock and CU limits with margin; the zero-toll and zero-slice shortcuts stay inline regardless. A failure at N = 16 never disables smaller feasible N. If the M5/M6 handlers only support chunks, add the inline step here (`withdraw_simple`/`settle_compounding` perform Discover→Commit in-instruction when `registry_len <= inline_threshold`), with its own golden and conservation tests. No economics change under any outcome.
- [ ] **Step 3: Budget rebuild** from measured sizes using the SIMD-0437 step active at measurement time and the 696-lamport terminal step as a second column (DR-27); separate retained vs reclaimable rent, consumed oracle/transaction fees, deployment buffer, RPC/host subscriptions, ICP cycles at 26,153,846,153 per signature for the watchdog's expected signature count. Keep the 12-SOL / 0.5–1.5-SOL figures labeled as superseded scenarios.
- [ ] **Step 4: Production SBF CI job.** Add `production-sbf` to `.github/workflows/native-solana-model.yml`: checkout; restore or download the pinned `cargo-build-sbf 3.1.0-beta.0` and platform-tools v1.48 archive by SHA-256 into a cached, job-local directory (no global install); run `scripts/build-sbf.sh` with the three `MC_*` variables; assert `artifact_sha256`, `source_sha256`, `lock_sha256` against `docs/solana/evidence/<latest>-build.json`; then run `cargo +1.92.0 test --locked -p musical-chairs-financial-runtime` with `MC_FINANCIAL_SBF` pointing at the fresh artifact, and `cargo clippy --all-targets -D warnings`. If the hosted runner cannot execute the SBF toolchain, the job must fail visibly rather than fall back to host-only compilation; a documented self-hosted runner is the fallback. Host compilation alone is never the final proof.

- [ ] **Step 5: Commit** `docs(solana): measured compute, packet, rent and pilot budget; ci: locked production SBF job`.

**Handoff:** this milestone may return a concrete redesign choice to Rob (e.g., page capacity, keeper format per DR-28); it never changes game rules silently.

---

## Milestone 10 — Targeted security review and fixes

**Files:** `docs/solana/threat-model.md` (new), review receipts under `docs/solana/evidence/`.

- [ ] **Step 1:** Run the repo personas `musicalchairs-protocol-security-reviewer` and `musicalchairs-ponzi-economy-auditor` plus the `solana-vulnerability-scanner` skill against the program, each producing file:line findings with severity; coordinator triages.
- [ ] **Step 2: Checklist (each must have a test or a documented reason)**: signer/owner/PDA/discriminator substitution on every instruction; duplicate mutable aliases; reinitialization; arithmetic overflow paths; replay of every permissionless instruction; double claims; closing/A-funding/payout barrier bypass; destination substitution to protected accounts; third-party fee payer authority; config activation races; late/duplicate ORAO fulfillment; expiry vs capture race at the deadline; sponsor withdrawal vs locked reserve; cleanup predicates; CU exhaustion on worst-case rows; heap use in loops.
- [ ] **Step 3:** Fix confirmed findings with failing-test-first; re-run the whole suite; commit `fix(program): security review findings`.

---

## Milestone 11 — Frontend native adapter (GUI preserved)

**Files:** Create `frontend/src/solana/native/{client.ts,accounts.ts,transactions.ts,status.ts,types.ts,idl/musical_chairs.json}`, `frontend/src/hooks/useNativeSolana.ts`, tests `frontend/src/solana/native/*.test.ts`; modify `frontend/src/components/SolInvestPanel.tsx` (native open; gross projection), `GameTracking.tsx` (native payouts; minimum receive; advanced contribution field default 0), `SolBackerPanel.tsx`/`AddBackerMoney.tsx`/`HouseDashboard.tsx` (native A funding, combined claim, 35/45/20 text, legacy section per DR-22), `GameStatusBar.tsx`, `WalletDropdown.tsx` (cluster indicator), `frontend/src/lib/gameConstants.ts:42–44` (replace stale constants with `BACKER_SENIOR_SHARE = 0.35`, `BACKER_PROPORTIONAL_A_SHARE = 0.45`, `BACKER_EQUAL_B_SHARE = 0.20`), `DocsPage.tsx:230–232`, `frontend/public/.ic-assets.json` only if a new HTTPS origin is needed. Do **not** change `package.json` dependencies except adding `@coral-xyz/anchor` **only if** the generated-client approach needs it; prefer hand-written Borsh codecs from the IDL to avoid a new dependency (decision recorded in the handoff).

**Interfaces consumed:** IDL v1 from M2; events from M5–M7. **Produced:** `NativeClient { openPosition, fundSeriesA, withdrawSimple, settleCompounding, closeSimplePosition, claimBacker, discoverRows, applySenior, sealWeights, allocateRows, pruneRows, commitTicket, advancePhase /* client-side orchestration that reads Ticket.phase and emits exactly one of the six typed instructions; not an on-chain ABI */, provisionPage, increaseProviderBudget, buildCandidates, requestDraw, captureDraw, applyPromotion, expirePromotion, finalizeRound, composeExpireFinalizeOpen }` returning `{ signature, nonce, status$ }`; `readPool()`, `readPosition(owner, nonce)`, `readBacker(owner, series)`, `readCloseRecord(id)`, `readTicket()`.

- [ ] **Step 1: Failing vitest tests** (pure functions first): `encodesOpenPositionArgsExactlyAsIdl` (byte vector from the Rust side committed in `solana/fixtures/ix-encodings.json`); `simpleProjectionUsesGrossNotNet` (1 SOL → 110,000,000 lamports/day; today's code yields 105,600,000); `splitTextShows35_45_20`; `rejectsSendWhenSelectedWalletPubkeyDiffers`; `unknownOutcomeKeepsSignatureAndNonceAndDoesNotRebuild`; `statusLadderSubmittedConfirmedFinalized`; `legacyAndNativePositionsNeverMergeForSameWallet`.
- [ ] **Step 2: Implement adapter and hooks; wire panels; keep layout/styling.**
- [ ] **Step 3: Local cluster for browser verification.** Install Agave 2.3.13 `solana-test-validator` into an owned temp dir (dev tooling, no global install), deploy the locally built `.so`, run the ORAO fixture program as in the probes, and drive the flows in the built-in browser: deposit → payout → A funding → claim → closing → resume processing → finalize/open. Capture screenshots per major screen against the current baseline. `npm test` and `npm run build` must pass.
- [ ] **Step 4: Commit** `feat(frontend): native Solana adapter behind existing SOL panels; gross projections; correct split text`.

**Handoff:** screenshot pairs; confirmation that no `@rollup/rollup-darwin-*` was added; whether a new dependency was introduced and why.

---

## Milestone 12 — ICP bridge canister and PP exactly-once

**Files:** Create `native_sol_bridge/{main.mo,Events.mo,Outbox.mo,Rpc.mo,Decode.mo}`, `native_sol_bridge/test/*.test.mo`, `dfx.json` entry; modify `shenanigans/main.mo` to add (R6 completed per C3, spec §9): a bridge allowlist, an append-only versioned policy table, stable `NativeObservation` and `NativeRewardObligation` storage, `observeNativeEvent(event)` (idempotent; pins `policy_version` at first validated observation; no effects, no await), `bindAndReserve(event_id)` (permissionless; resolves the principal via `siws_provider.get_principal`; on first binding writes the shared `signupGiftClaimed` marker before any await, computes every leg once under the pinned policy version, consumes effects/siphon caps, freezes `created_at_time`/memo per leg), `executeNativeRewardLeg(event_id, leg_index)` (`SubmittedUnknown` written before the single `icrc1_transfer`; `Ok → Committed`; `Duplicate{of} → Committed{of}`; `TemporarilyUnavailable → Retryable`; `BadFee|InsufficientFunds|GenericError → Quarantined{definite}`; `TooOld|CreatedInFuture → Quarantined{reconcile}`) and `reconcileLegFromLedger(event_id, leg_index, block_index)` (verifies an identical transfer via `pp_ledger.get_transactions`/`archives` before marking `Committed`); legacy `mintWithEffects`/`mintInternal` untouched and never called on the native path; `frontend/src/hooks/useNativeHistory.ts`.

**Interfaces:** bridge: `submitEventDescriptor(desc)` (relayer-authenticated), `getPending(cursor)`, `getEvent(id)`, `releasePending(event_id)`; Shenanigans: `observeNativeEvent(event) : async { #Observed : ObligationId; #AlreadyObserved : ObligationId; #Rejected : { reason : Text; permanent : Bool } }`, `bindAndReserve(event_id) : async { #Reserved; #AlreadyReserved; #Unbound }`, `executeNativeRewardLeg(event_id, leg_index) : async LegStatus`, `reconcileLegFromLedger(event_id, leg_index, block_index) : async LegStatus`, `getNativeObligation(event_id)`; leg record `{ index, recipient: Account, amount: Nat, fee: null, memo: Blob(32), created_at_time: Nat64, status: #Pending | #SubmittedUnknown | #Committed { block } | #Retryable { reason; attempts } | #Quarantined { reason } }`. Beneficiary resolution via `siws_provider.get_principal(address)` (`siws_provider.did:85`; ic-siws v0.1.0). Bridge cursor advances only on `#Observed`/`#AlreadyObserved`; `#Rejected{permanent=true}` writes a durable quarantined outbox record.

- [ ] **Step 1: Failing Motoko tests** (mo-test / local replica): bridge: `descriptorRequiresTwoAgreeingFinalizedObservations` (one provider disagrees → pending); `unsupportedTransactionVersionLeavesGapNeverAdvancesCursor`; `eventIdTupleIsImmutableAndDedups`; `cursorAdvancesOnlyAfterReservationReturned`; `missingSiwsBindingKeepsPubkeyPendingAndReleasesOnBind`. Bridge provenance (C4): `rejectsForgedProgramId`, `rejectsForgedEventAuthority`, `rejectsFailedTransaction`, `rejectsWrongAncestryOrInnerIndex`, `rejectsSchemaOrClusterMismatch`, `rejectsDescriptorRecipientOrBasisMismatch`, `promotionBeneficiaryComesFromEventNotKeeperSigner`, `ownerEventRequiresOwnerAmongSigners`. Shenanigans boundary (R6/C3): `observeIsIdempotentAndPinsPolicyVersionAtFirstObservation`; `policyChangeBetweenObservationAndBindingDoesNotChangeLegs`; `bindAndReserveComputesLegsOnceAndConsumesEffectsAtBinding`; `unboundPubkeyStaysObservedAndBindsLaterWithoutRebind`; `signupGiftMarkerWrittenBeforeAwaitBothOrderings` (native→legacy and legacy→native; one gift); `twoSimultaneousNativeFirstEventsYieldOneSignupGift`; `rejectedPermanentQuarantinesWithReasonAndCursorPassesOnlyOverDurableRecord`; `transientRejectionRetainsRetry`; `executeWritesSubmittedUnknownBeforeAwait` (trap injected after the transfer; status is `SubmittedUnknown`, not `Pending`); `retryRepeatsIdenticalFieldsAndDuplicateReconciles` (against a local `pp_ledger` from `dfx.json` with `--specified-id 5xv2o-iiaaa-aaaac-qeclq-cai` as in the OTC desk recipe); `temporarilyUnavailableIsRetryableAndBadFeeIsQuarantinedDefinite`; `tooOldQuarantinesUntilLedgerHistoryReconcilesWithIdenticalFields`; `forgedReceiptBlockIndexDoesNotCommit`; `failedSiblingLegDoesNotRepeatOrStrandCommittedLegs`; `upgradeAtEveryDurableBoundaryPreservesState` (observed, bound, submitted-unknown, quarantined); `legacyMintWithEffectsUntouchedByNativePath`.
- [ ] **Step 2: Implement.** Decode self-CPI events from `getTransaction` with `maxSupportedTransactionVersion = 1` via a raw-RPC adapter; verify the SOL RPC canister Candid (`tghme-zyaaa-aaaar-qarca-cai`, module `1733b1e5…3115` as of 2026-10-13) before binding (memory lesson: fetch live Candid; `Multi*Result` consensus variants).
- [ ] **Step 3: PASS; `moc` via mops check; commit** `feat(icp): native_sol_bridge with two-RPC finalized ingestion and exactly-once PP legs`.

**Handoff:** explicit trust statement; no deployment.

---

## Milestone 13 — Keepers and monitoring (code only, nothing started)

**Files:** Create `solana/keepers/worker/` (Rust CLI using the same SDK pins: journal, cursor reads, chunk sizing, retry with identical blockhash handling, metrics), `native_sol_keeper/main.mo` (ICP watchdog: timers, fee-only threshold address, journal, re-arm after upgrade), `docs/solana/operations.md` (alerts 60 s / 300 s, escalation steps, no credentials).

- [ ] **Step 1: Failing tests**: worker `prepares_one_row_fallback_when_simulation_exceeds_budget`; `does_not_resubmit_after_unknown_outcome_until_status_read`; `any_wallet_can_run_the_same_plan` (no privileged key path); watchdog `signs_only_after_stale_progress_threshold`; `rearms_timer_after_upgrade`; `fee_address_has_no_program_authority` (static check against the program's instruction account lists).
- [ ] **Step 2: Implement; PASS; commit** `feat(keepers): fee-only worker and ICP watchdog, no services started`.

---

## Milestone 14 — Devnet acceptance (AUTHORIZATION GATE)

Requires Rob's explicit approval for: devnet deployment, creating two devnet Squads multisigs with test voters, ORAO devnet client registration and funding, test-wallet funding.

- [ ] Deploy the reviewed `.so`; record `Program`/`ProgramData` and upgrade authority = G vault; `initialize`.
- [ ] Register and fund ORAO client; run: deposit, Simple payout, both compound settlements, A funding, combined claim, cap completion, partial payout, closing with candidates, draw request/capture latency (record slots and seconds), apply promotion, expiry path on a second close, finalize/reopen via composed transaction from a plain wallet.
- [ ] Readback: G/P profiles, `time_lock`, vault derivation; `Admission.guardian_vault == Pool.guardian_vault == P`; ORAO `Client.owner == G Vault`, `Client.state == RngAuthority`, account owner == ORAO program; test G reducing its own time lock on devnet and document it.
- [ ] **Deployed-byte evidence (exact, C5):** fetch the on-chain `ProgramData` account; verify the 45-byte loader-v3 metadata header (`UpgradeableLoaderState::size_of_programdata_metadata() == 45`, pinned `solana-loader-v3-interface 3.0.0 state.rs:47–48`) decodes to `ProgramData { slot, upgrade_authority_address }`; assert `data[45 .. 45 + elf_len] == reviewed ELF` where `elf_len` is the recorded artifact length, then separately assert `data[45 + elf_len ..]` is all zero and record its length as allocation padding; never trim trailing zeros from the ELF itself. Record `elf_len`, `data.len()`, loader format, `upgrade_authority`, deployment slot, `source_set_sha256`, lock/artifact/IDL SHA-256 and the IDL account hash if published. Distinguish in the receipt: uploaded buffer, deployed program bytes, `initialize`/config activation, broadcast signatures, finality, reconciled balances.
- [ ] **PP path acceptance (C4):** on devnet, drive one genuine `PositionOpened` and one `PromotionApplied` through finalized event → two RPC observations → provenance verification → `observeNativeEvent` (unbound) → SIWS binding → `bindAndReserve` → `executeNativeRewardLeg` → ledger receipt and `reconcileLegFromLedger` → assert no legacy duplicate reward for the same principal. Local fixtures prove only local paths; this is the first cross-canister evidence.
- [ ] **ICP devnet-stage deployments (separately authorized):** deploy `native_sol_bridge` and `native_sol_keeper` to the IC with devnet RPC configuration and upgrade `shenanigans` with the R6 methods behind the allowlist; record each canister's module hash (`dfx canister info`/ic-api) against the built Wasm SHA-256; deploy the frontend build with devnet CSP and record the served asset hash and version marker against the build output.
- [ ] Reconcile both conservation identities from RPC after every action; record receipts in `docs/solana/evidence/2026-1N-NN-devnet-*.json`.
- [ ] Frontend against devnet behind the ICP-hosted build with devnet RPC in `connect-src`.

---

## Milestone 15 — Separately authorized release preparation (not executed in planning)

- [ ] Mainnet manifest: program ID, G/P vaults and voters, recipients, ORAO tuple, config v1 values including the close cost schedule (`provider_request_fee_lamports`, `provider_fee_headroom_bps`) and pre-provisioned pages, RPC endpoints, CSP origins, `SOLANA_RPC_ENDPOINT`, operations funding amount (from M9 at the then-active rent step).
- [ ] Runbook: stop conditions, pause via P, monitoring, rollback limits (there is no rollback of cash).
- [ ] **Mainnet verification evidence (each after its own authorized action):** on-chain `ProgramData` bytes equal the reviewed ELF (header/padding handled as in M14); `upgrade_authority == G Vault`; ORAO registration readback of the three roles; `Pool.cluster_domain` equals the mainnet genesis vector; `Admission.guardian_vault == P`; IDL hash; ICP module hashes for `native_sol_bridge`, `native_sol_keeper`, upgraded `shenanigans` against built Wasm; served frontend asset hash/version marker against the approved build; `ProgramData` bytes compared exactly as in M14 (header decode, `elf_len` equality, zero padding asserted separately); the PP path receipt of M14 repeated on mainnet with real funds only under its own authorization; first finalized lifecycle receipts and reconciled balances. Uploaded bytes, deployed bytes, activation, broadcast/finality and balances are recorded as separate facts.
- [ ] Each deployment/funding/signing action is a separate explicit go.

---

## Test matrix — accepted invariant → test

| Invariant (spec §) | Test IDs |
| --- | --- |
| Gross accrual, 4 % load (3.1) | M5 `open_one_sol…`, M11 `simpleProjectionUsesGrossNotNet` |
| Simple remainder carry, clamp, backwards clock (3.2) | M1/M5 differential `payout_props`, host `vectors.rs` |
| Carried Interest tiers (3.2) | M5 vectors at day 7−1 s / 7 / 14−1 s / 14 / 21 |
| Q18 maturity and fees (3.3) | M5 `compound_15_maturity_vector`, Compound30 twin |
| Strict insolvency, equality, zero-pot close (3.4, 4) | M5 `strict_insolvency_closes_and_equality_does_not`, zero-pot min-0 test |
| No hidden fee, no silent dust (3.4) | M5 `withdraw…pays_96_800_000`, dust claim tests in M9 |
| Quota atomic, exact hour (3.1) | M5 `quota_third_success…`, `failed_open_does_not_consume…` |
| Split goldens, folds, sealed denominator (3.5) | M6 three goldens, M1 `pro_rata_floors…` |
| Chunk equivalence, replay, alias, stale calls (7.3, 7.6) | M6 `chunk_sizes_1_4_8…`, `replaying…`, `stale_ticket_id…`, `duplicate_independent_backer_rows…`, prune regressions |
| U256 discovery accumulation (3.6) | M1 `sealed_denominator_sums_past_u128…`, M6 `discovery_accumulates_a_remainders_above_u128_in_u256` |
| Nonce permanence (R1) | M5 `same_nonce_cannot_open_a_second_position_after_settlement`, `unknown_outcome_retry…`, `fund_series_a_retry…` |
| Captured close reserve (R2) | M2 `required_close_reserve` vectors, M5 `closing_reserve_scales…`, `closing_payout_short_by_one_lamport…`, M8 R2 suite |
| Permanent snapshots, identity-bound cleanup (R3) | M4 `close_snapshot_and_registry_pages_always_refused…`, `snapshot_pages_counter_never_decrements`, M8 R3 suite |
| Guardian key (R4) | M3 `rotate_guardian_requires_g…`, `rotate_guardian_recovers_lost_p_quorum`, pause metas test |
| ORAO roles and provider attribution (R5, DR-32, C2) | M7 `request_rejects_wrong_client_owner…`, `request_decrements_remaining_partitions…`, `fulfillment_reimburses_excess_rent_to_client_only…`, `record_provider_return…`, `increase_provider_budget…`, `owner_fallback_may_fund_provider…`, M8 `conservation_transitions_table_8_6…`, M14 readback |
| Per-set candidate reserve and locked-close provisioning (C1) | M2 vectors, M4 `provision_locked_close_*`, M5 `closing_reserve_scales…`, M8 `uneven_preprovisioning…`, `all_free_ops_locked…` |
| PP two-phase protocol and provenance (C3, C4) | M12 bridge provenance suite and Shenanigans boundary suite; M14 PP path acceptance |
| Canonical source digest and exact deployed bytes (C5) | M0 step 5 wrapper digest check; M9 CI assertions; M14/M15 `ProgramData` comparison |
| Shenanigans reservation protocol (R6) | M12 boundary suite |
| Immutable cluster domain (4) | M3 `initialize_stores_cluster_domain_immutably`, M7 `protocol_seed_differs_between_devnet_and_mainnet…` |
| Lazy credit, claims never restore headroom (7.3) | M6 `claim_during_ticket…`, `claim_never_restores…` |
| Barrier matrix (4.1) | M6 `normal_deposit_allowed_during_nonclosing…`, M5 A-funding rejection |
| Candidate completeness, preference (4) | M7 `build_candidates_visits…`, reorder rejection |
| One bound draw, deadline boundaries, no reroll (4) | M7 request/capture/collision/expiry tests |
| 64-counter mapping (4) | M7 `apply_promotion_first_valid…`, `all_64…` |
| Seed carry once, composed reopen (4) | M7 `finalize_requires…`, composed test |
| Conservation (3.7) | M6/M8 identity checks, M8 randomized sequence |
| Reserve, refund once, cleanup never blocks (8) | M8 suite |
| Governance profile, pause scope, recipients (5) | M3 suite |
| Rent attribution (DR-13) | M4 suite |
| Bridge trust and exactly-once (9) | M12 suite |
| Wallet/nonce/status (11) | M11 vitest |

## Graph and handoff obligations per milestone

1. `graphify update .` from the worktree root (structural; never a subfolder rebuild).
2. Semantic fragment for changed docs via Sonnet/Haiku workers writing `graphify-out/.graphify_chunk_*.json`; coordinator merges, preserving all prior nodes, raw directed edges and hyperedges; validates no dangling IDs and no shrinkage; records per-file SHA-256 and scoped freshness in `docs/solana/evidence/<date>-<milestone>-graph-refresh.json`.
3. Query checks: allocation phases, claim-triggered close/expiry, authority/ICP boundary, costs, workstream dependencies.
4. Kanban row per task with state, owner, evidence path and limits.

## Self-review against the spec

- Every spec §7 instruction has an implementing milestone (controls M3/M4, game M5, allocation M6, close M7, reserve M8). Events (§7.5) in M2 and emitted by M5–M7. Bridge (§9) M12. Keepers (§10) M13. Frontend (§11) M11. Governance (§5) M3 + M14 readback.
- Names used consistently: `discover_rows`, `apply_senior`, `seal_weights`, `allocate_rows`, `prune_rows`, `commit_ticket`, `claim_backer`, `build_candidates`, `request_round_draw`, `capture_round_draw`, `apply_promotion`, `expire_promotion`, `finalize_round`, `increase_provider_budget`, `record_provider_return`, `reconcile_ops_cash`, `provision_page` (funding `Free|Caller|LockedClose`), `provision_close_record`, `close_shared_account`, `stage_config_version`, `activate_config_version`, `rotate_guardian`, `required_close_reserve`, `observeNativeEvent`, `bindAndReserve`, `executeNativeRewardLeg`, `reconcileLegFromLedger`.
- No task depends on an unresolved Rob decision except M11's legacy-section arrangement (DR-22), which can be implemented either way late in M11.
- Codex review round 1 items R1–R6 and round 2 items C1–C6 each map to a milestone step and test above; the matrices with line references are in the review packet.
