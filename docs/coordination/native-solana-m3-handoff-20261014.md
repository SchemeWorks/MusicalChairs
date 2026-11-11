# Native Solana — M3 authenticated controls handoff (2026-10-14)

Branch `codex/native-solana-production-core`, continuing from the [M2 handoff](native-solana-m2-handoff-20261014.md) (`c030870`). Local work only; nothing deployed, signed with live keys, funded, registered with Squads or ORAO, activated or merged.

## Commits since the M2 handoff (oldest first)

| Commit | Subject |
| --- | --- |
| `0f6fafe` | docs(kanban): M3 authenticated-controls cards IMPL-16..IMPL-19 |
| `6abc25d` | feat(program): authenticated controls on the pinned Squads v4 layout (M3) |
| `6589475` | test(solana): add the test-only Squads vault stand-in program |
| `5d2038e` | feat(program): regenerate IDL for the M3 control instructions |
| `420f092` | test(solana): LiteSVM authenticated-controls suite with byte-exact Squads and loader fixtures |

## What M3 delivered

- **Squads layout pinned and the two draft parse defects fixed (IMPL-16, `6abc25d`)**: layout from Squads-Protocol/v4 `state/multisig.rs` at commit `887627b3ac3f59548fdb6dfd1dfe23a21bb72b60`; discriminator `sha256("account:Multisig")[..8] = e07479ba44a14fec` (draft ended `c8`); `config_authority` is a plain `Pubkey` that must equal the default key (draft read an `Option`); exact `time_lock` 172,800 (G) / 0 (P); `validate_profile` returns the vault PDA; `require_vault_signer`; new `WrongGovernanceProfile` (6031). 64 program unit tests.
- **Control instructions**: `stage_config_version(version, terms)`, `activate_config_version(version, expected_current)`, `rotate_recipient(kind, new)`, `rotate_guardian()`; `initialize` now creates `ConfigVersion#1` (from `default_v1` plus provider/launch args, 235-byte `InitializeArgs`) and `RngAuthority`; `withdraw_management` returns `InsufficientManagement`.
- **Local vault signer (IMPL-17, `6589475`) — decision flagged for Codex**: a Squads vault is a PDA and cannot sign a LiteSVM transaction, so a 29 KB test-only stand-in program loaded at the Squads program id forwards one inner instruction via `invoke_signed` with the vault seeds. It never ships; real Squads execution and deployed readback remain M14. Built with `rust-version 1.84` and a `blake3 1.5.5` pin because the platform-tools cargo 1.84 cannot parse edition-2024 manifests.
- **IDL v1.1 (`5d2038e`)**: sha `de83a400c1e2e3a0c7c074f82a240d12861fb04fd535138724f26a05b8472436`, 21 instructions, 13 accounts (`ConfigVersion`, `RngAuthority` now included), 32 errors, 37 types; `check-idl.sh` clean.
- **LiteSVM suite and fixtures (IMPL-18, `420f092`)**: 17 rows / 108 labelled observations; byte-exact `fixtures/squads/multisig_{g,p}.bin` (199 B each) and upgradeable-loader `fixtures/loader/{program,programdata}.bin` headers with the G vault as upgrade authority; every rejection asserts the exact code and fee-only rollback; every success asserts ops-cash and game-vault conservation and `Pool.cluster_domain` immutability.
- **Run (IMPL-19)**: main ELF `2be7359e79ee786cf8b9596bd3fb186383715141482e85b80392b451ed5b46b0` (769,024 B) at `6589475caf319c1ef77fe1a26c3e696e791e73ed`, source set `555590325ca5c9a23ab22af47d8afc3eccb00ad7c0e685e8d3a6da57f04650fa`, lock `98d9119c163e1b3e…`; stand-in `eb78e77fc107c45d38d7e45bb365d632a8418909a3b0c79dbca26f7c20f109c8`. 34 harness tests: 31 pass, exactly the three intended M0 RED fail; CLI `all` produced 132 observations.

## Headline rows (full table of 108 in `2026-10-14-m3-controls.json`; wrapped rows carry three stand-in accounts and about 60 extra bytes)

| Row | Result | Error | CU | Packet B | Accounts |
| --- | --- | --- | --- | --- | --- |
| `initialize_succeeds_with_g_vault_signer_and_matching_programdata` | ok | — | 106,780 | 1051 | 19 |
| `initialize_rejects_first_caller_without_g_vault` | rejected | InstructionError(1, Custom(6004)) | 62,892 | 1110 | 19 |
| `initialize_rejects_programdata_authority_mismatch` | rejected | InstructionError(1, Custom(6004)) | 96,579 | 1051 | 19 |
| `initialize_rejects_one_of_three_profile` | rejected | InstructionError(1, Custom(6031)) | 74,272 | 1051 | 19 |
| `initialize_sets_pot_seed_zero_round_one_next_close_one` | ok | — | 106,780 | 1051 | 19 |
| `initialize_accepts_allocation_padded_multisig` | ok | — | 106,802 | 1051 | 19 |
| `set_admission_pause_touches_only_admission_account` | ok | — | 15,262 | 387 | 8 |
| `set_admission_pause_rejects_non_guardian` | rejected | InstructionError(1, Custom(6004)) | 7,236 | 382 | 6 |
| `rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted` | ok | — | 26,971 | 419 | 9 |
| `rotate_guardian_recovers_lost_p_quorum` | ok | — | 26,971 | 419 | 9 |
| `withdraw_management_pays_recipient_only_and_caps_at_accrued` | ok | — | 22,811 | 488 | 9 |
| `fund_operations_any_signer_increases_free_balance_only` | ok | — | 16,543 | 455 | 8 |
| `withdraw_unreserved_operations_respects_locked_and_rent` | rejected | InstructionError(1, Custom(6007)) | 11,365 | 455 | 8 |
| `stage_config_requires_g_and_future_round` | rejected | InstructionError(1, Custom(6030)) | 26,248 | 771 | 12 |
| `activate_config_only_when_active_no_ticket_and_round_reached` | ok | — | 20,827 | 391 | 8 |
| `rotate_recipient_requires_g` | rejected | InstructionError(1, Custom(6004)) | 14,518 | 446 | 7 |
| `initialize_stores_cluster_domain_immutably` | ok | — | 106,780 | 1051 | 19 |

Error tally over the 108 M3 observations: None×37, InstructionError(1, Custom(6031))×20, InstructionError(1, Custom(6004))×19, InstructionError(1, Custom(6005))×9, InstructionError(1, Custom(6030))×6, InstructionError(1, Custom(6007))×4, InstructionError(1, Custom(6001))×4, InstructionError(1, Custom(3010))×2, InstructionError(1, Custom(6023))×2, InstructionError(1, Custom(6008))×2, InstructionError(1, Custom(0))×2, InstructionError(1, Custom(6006))×1.

## Caveats and deviations a reviewer should know

1. **Stand-in signer.** The main program cannot distinguish the stand-in's `invoke_signed` from the real Squads program's; that is precisely the boundary M14 verifies on devnet with real Squads accounts and proposals. Byte-exact Multisig fixtures establish the parse boundary only.
2. **No cancel for a staged config.** `stage_config_version` requires `version == active + 1`; an unwanted staged version blocks `n+2` until it activates (matches the spec's "fails: version exists"). Worth a register note if G wants a cancel.
3. **Error choices.** stage with `effective_round_id <= round_id` → `ConfigNotActivatable`; version gap → `InvalidState`; re-stage → `WrongOwner` (from `create_shared`); Squads owner mismatch → `WrongAccount`; truncated body → `WrongGovernanceProfile`. `withdraw_operations`'s rent-plus-locked branch is unreachable because `sync_ops` fails closed first (`InvalidState` on an unbacked lock; covered by a variant row).
4. **Expected drift.** The M0 row `financial.controls.management.empty-balance` now returns 6023 `InsufficientManagement` (was 6007); the M0 fee-only-rollback test still passes.
5. **Kept names.** `set_pause`, `withdraw_operations`, `withdraw_management` keep the draft names (spec uses longer ones). `rotate_recipient` kind 1 writes both `Pool.sponsor` and `Ops.sponsor`. `validate_profile` keeps the draft's "some member can Initiate and some can Execute" requirement.
6. **Real Squads account size.** Live accounts are allocated at `Multisig::size(n)` (231 B for three members, 32 trailing zero bytes when `rent_collector` is `None`); the committed fixtures are the exact unpadded encoding and a padded-row test covers the allocated shape.
7. **Not wired yet.** `total_contributed`/`total_withdrawn`, `protected_rent`, page `RentComponents` recording, `fold_slices` from pinned bps — unchanged from the M2 list.

## Proof level

Host unit tests; compiled SBF with complete provenance; LiteSVM execution of every control row with vault-PDA signatures from the test-only stand-in; event-CPI on the implemented control handlers. No production game instruction is GREEN. Graph: `2026-10-14-m3-graph-refresh.json` (11,928 / 16,580 / 39; every prior identity retained).

## Staffing

Coordinator (Claude Fable 5.1): single build executor (both ELFs), sole graph writer, verifier, IDL regeneration, stand-in lock/MSRV fix, receipts. Sonnet workers: IMPL-16 (controls), IMPL-18 (suite + fixtures). Haiku worker: IMPL-17 (stand-in). No worker ran an SBF build or git.

## Solana notes

- A PDA has no private key; only its owning program can make it sign, through `invoke_signed`. On ICP the analogue is a canister calling another canister as itself; on EVM a contract calling another contract. Squads vaults are PDAs of the Squads program, so locally a program at that id is the only way to produce the signature.
- `ProgramData.upgrade_authority` is the loader's record of who may upgrade the code, read by `initialize` like a controller list, but it lives in the loader's account, not ours. LiteSVM's `add_program` installs a non-upgradeable loader shape, so the fixture rewrites both loader accounts.
- Compute: `initialize` costs about 107k CU of the 1.4M budget, mostly seven `create_account` CPIs and two profile parses; the four new control instructions sit between 20k and 47k CU.

## Codex bounded source review (2026-10-14)

- **Pass:** signer/profile/PDA/loader binding, guardian rotation, config stage/activate and the locked-cash guards.
- **Gap (pending, not a design gate):** `fund_operations` accepts `amount == 0` and omits the checked `Ops.total_contributed += amount`; `withdraw_operations` omits `Ops.total_withdrawn`; the harness row `fund_operations_any_signer_increases_free_balance_only` asserts only `free_balance` changes. Spec §7.1 requires positive funding and both transitions. This was deferred in the M2 handoff and is now assigned to **IMPL-20** (reject zero before the CPI, atomic u128 counters, corrected fixture delta, zero-funding/counter/rollback regressions).
- **Labels:** M3 **authentication proof: PASS**. **Ops metadata/counters: DONE (IMPL-20)** — `fund_operations` rejects zero before the CPI (`InvalidState`), `total_contributed`/`total_withdrawn` are maintained atomically, regressions pass on the rebuilt ELF `990ebc02b744ab1a…` (source set `50ec232cd68ee066…`; 31 pass + 3 intended RED; 134 observations). A zero-amount withdraw succeeds with no delta (spec requires only `amount <= free_balance`). The test-only Squads stand-in is **accepted for application guards only**, not as real quorum/timelock proof (M14).

## Next

M4 (provisioning, rent components, cleanup): `provision_page(kind, index, funding)` for all four page kinds with `Funding::{Free, Caller, LockedClose}`, `provision_close_record`, `RentComponents` recording, `close_shared_account` predicates, `reconcile_ops_cash`, `protected_rent` consultation.
