//! Actual compiled-SBF financial runtime harness.
//!
//! The fixture is intentionally local and synthetic.  It loads the production
//! Musical Chairs ELF selected by `MC_FINANCIAL_SBF`, injects Anchor-serialized
//! production accounts, and records what LiteSVM actually executed.  It does
//! not make provider, wallet, Squads, deployment, or live-settlement claims.
//!
//! The M3 authenticated-controls suite (`controls`) additionally loads the
//! TEST-ONLY Squads vault stand-in ELF selected by `MC_STANDIN_SBF` at the
//! Squads program id, plus byte-exact local Squads `Multisig` and
//! upgradeable-loader fixtures (`fixtures`).  The stand-in is never deployed and
//! proves no real Squads execution.

pub mod controls;
pub mod financial;
pub mod runtime;

pub use controls::run_controls_m3_suite;
pub use financial::{
    run_allocation_golden_flow, run_controls_suite, run_red_allocation, run_red_claim, run_red_open,
};
pub use runtime::{fixtures, FinancialRuntime, Observation};

/// Run the small evidence CLI without making a live transaction claim.
pub fn run_suite() -> Vec<Observation> {
    let mut observations = vec![run_red_open(), run_red_claim(), run_red_allocation()];
    observations.extend(run_allocation_golden_flow());
    observations.extend(run_controls_suite());
    observations.extend(run_controls_m3_suite());
    observations
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This is intentionally RED while the production handler is still a
    /// `NotImplemented` gate.  It must be the first meaningful failure when
    /// the compiled SBF artifact is loaded by the root build executor.
    #[test]
    fn open_moves_real_sol() {
        let observation = run_red_open();
        assert!(
            observation.success,
            "open_moves_real_sol expected a successful compiled-SBF admission: {observation:?}"
        );
    }

    /// Claim is also RED until the production payout handler is enabled.  It
    /// stays separate from the open test so cargo test reports both financial
    /// instruction boundaries once the first gate is repaired.
    #[test]
    fn claim_moves_real_sol() {
        let observation = run_red_claim();
        assert!(
            observation.success,
            "claim_moves_real_sol expected a successful compiled-SBF payout: {observation:?}"
        );
    }

    #[test]
    fn allocation_golden_t10000() {
        let observation = run_red_allocation();
        assert!(
            observation.success,
            "allocation_golden_t10000 expected a successful compiled-SBF discovery: {observation:?}"
        );
    }

    #[test]
    fn allocation_six_phase_flow_has_unique_row_observations() {
        let observations = run_allocation_golden_flow();
        assert_eq!(observations.len(), 4 + 1 + 1 + 4 + 4 + 1 + 1);
        let labels: std::collections::BTreeSet<_> =
            observations.iter().map(|observation| observation.label.as_str()).collect();
        assert_eq!(labels.len(), observations.len());
        assert!(observations.iter().all(|observation| observation.fits_legacy_packet));
    }

    #[test]
    fn fee_only_rollback_is_observed_for_the_current_open_gate() {
        let observation = run_red_open();
        if !observation.success {
            observation.assert_rollback_except_fee();
        }
    }
}

/// M3 authenticated-controls rows.  Each test runs one row of
/// `controls.rs` against the actual compiled SBF plus the test-only Squads
/// vault stand-in (`MC_FINANCIAL_SBF`, `MC_STANDIN_SBF`).  The row asserts the
/// exact program error code of every rejection (parsed from
/// `Observation.error`), fee-only rollback on every rejection, and both
/// cash-register conservation invariants on every success.
#[cfg(test)]
mod controls_m3_tests {
    use crate::controls::{self as m3, assert_headline, assert_outcomes_consistent};
    use crate::runtime::Observation;

    fn run_row(name: &str, row: fn() -> Vec<Observation>) {
        let observations = row();
        assert_headline(&observations, name);
        assert_outcomes_consistent(&observations);
        m3::assert_unique_labels(&observations);
    }

    #[test]
    fn initialize_succeeds_with_g_vault_signer_and_matching_programdata() {
        run_row(
            m3::ROW_INITIALIZE_SUCCEEDS,
            m3::initialize_succeeds_with_g_vault_signer_and_matching_programdata,
        );
    }

    #[test]
    fn initialize_rejects_first_caller_without_g_vault() {
        run_row(
            m3::ROW_INITIALIZE_FIRST_CALLER,
            m3::initialize_rejects_first_caller_without_g_vault,
        );
    }

    #[test]
    fn initialize_rejects_programdata_authority_mismatch() {
        run_row(
            m3::ROW_INITIALIZE_PROGRAMDATA,
            m3::initialize_rejects_programdata_authority_mismatch,
        );
    }

    #[test]
    fn initialize_rejects_one_of_three_profile() {
        run_row(
            m3::ROW_INITIALIZE_PROFILE,
            m3::initialize_rejects_one_of_three_profile,
        );
    }

    #[test]
    fn initialize_sets_pot_seed_zero_round_one_next_close_one() {
        run_row(
            m3::ROW_INITIALIZE_GENESIS,
            m3::initialize_sets_pot_seed_zero_round_one_next_close_one,
        );
    }

    #[test]
    fn initialize_accepts_allocation_padded_multisig() {
        run_row(
            m3::ROW_INITIALIZE_PADDED,
            m3::initialize_accepts_allocation_padded_multisig,
        );
    }

    #[test]
    fn set_admission_pause_touches_only_admission_account() {
        run_row(
            m3::ROW_PAUSE_TOUCHES_ONLY_ADMISSION,
            m3::set_admission_pause_touches_only_admission_account,
        );
    }

    #[test]
    fn set_admission_pause_rejects_non_guardian() {
        run_row(
            m3::ROW_PAUSE_REJECTS_NON_GUARDIAN,
            m3::set_admission_pause_rejects_non_guardian,
        );
    }

    #[test]
    fn rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted() {
        run_row(
            m3::ROW_ROTATE_GUARDIAN,
            m3::rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted,
        );
    }

    #[test]
    fn rotate_guardian_recovers_lost_p_quorum() {
        run_row(
            m3::ROW_ROTATE_GUARDIAN_RECOVERY,
            m3::rotate_guardian_recovers_lost_p_quorum,
        );
    }

    #[test]
    fn withdraw_management_pays_recipient_only_and_caps_at_accrued() {
        run_row(
            m3::ROW_WITHDRAW_MANAGEMENT,
            m3::withdraw_management_pays_recipient_only_and_caps_at_accrued,
        );
    }

    #[test]
    fn fund_operations_any_signer_increases_free_balance_only() {
        run_row(
            m3::ROW_FUND_OPERATIONS,
            m3::fund_operations_any_signer_increases_free_balance_only,
        );
    }

    #[test]
    fn withdraw_unreserved_operations_respects_locked_and_rent() {
        run_row(
            m3::ROW_WITHDRAW_OPERATIONS,
            m3::withdraw_unreserved_operations_respects_locked_and_rent,
        );
    }

    #[test]
    fn stage_config_requires_g_and_future_round() {
        run_row(
            m3::ROW_STAGE_CONFIG,
            m3::stage_config_requires_g_and_future_round,
        );
    }

    #[test]
    fn activate_config_only_when_active_no_ticket_and_round_reached() {
        run_row(
            m3::ROW_ACTIVATE_CONFIG,
            m3::activate_config_only_when_active_no_ticket_and_round_reached,
        );
    }

    #[test]
    fn rotate_recipient_requires_g() {
        run_row(m3::ROW_ROTATE_RECIPIENT, m3::rotate_recipient_requires_g);
    }

    #[test]
    fn initialize_stores_cluster_domain_immutably() {
        run_row(
            m3::ROW_CLUSTER_DOMAIN,
            m3::initialize_stores_cluster_domain_immutably,
        );
    }

    #[test]
    fn m3_suite_aggregates_every_row_with_unique_labels() {
        let observations = m3::run_controls_m3_suite();
        assert_outcomes_consistent(&observations);
        m3::assert_unique_labels(&observations);
        for row in m3::ROW_NAMES {
            assert_headline(&observations, row);
        }
    }
}

/// Byte-exact fixture tests.  These need no ELF: they prove the committed
/// `.bin` files are exactly the generator output, decode them with an
/// independent reader, and keep the fixture READMEs honest.
#[cfg(test)]
mod fixture_tests {
    use std::{fs, path::Path};

    use musical_chairs_program::{
        controls::{SQUADS_MULTISIG_DISCRIMINATOR, SQUADS_PROGRAM_ID},
        ErrorCode, ID,
    };
    use sha2::{Digest, Sha256};
    use solana_sdk::pubkey::Pubkey;

    use crate::controls::parse_custom_code;
    use crate::runtime::{
        fixtures::{loader, squads},
        FinancialRuntime, G_CREATE_KEY, P_CREATE_KEY,
    };

    fn key(bytes: [u8; 32]) -> Pubkey {
        Pubkey::new_from_array(bytes)
    }

    fn fixture_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    #[test]
    fn multisig_discriminator_is_sha256_account_multisig() {
        let digest = Sha256::digest(b"account:Multisig");
        assert_eq!(&digest[..8], &SQUADS_MULTISIG_DISCRIMINATOR);
    }

    #[test]
    fn committed_squads_fixtures_equal_the_generator_output() {
        assert_eq!(squads::committed_governance(), squads::governance_bytes());
        assert_eq!(squads::committed_guardian(), squads::guardian_bytes());
        // 8 + 32 + 32 + 2 + 4 + 8 + 8 + 1 (None tag) + 1 + 4 + 3 x 33.
        assert_eq!(squads::committed_governance().len(), 199);
        assert_eq!(squads::committed_guardian().len(), 199);
        assert_ne!(squads::committed_governance(), squads::committed_guardian());
    }

    /// An independent reader of the pinned Borsh layout, so a generator bug
    /// cannot hide behind byte-equality with itself.
    #[test]
    fn committed_squads_fixtures_decode_to_the_pinned_profiles() {
        for (bytes, create_key, time_lock) in [
            (squads::committed_governance(), G_CREATE_KEY, 172_800u32),
            (squads::committed_guardian(), P_CREATE_KEY, 0u32),
        ] {
            assert_eq!(&bytes[0..8], &SQUADS_MULTISIG_DISCRIMINATOR);
            assert_eq!(&bytes[8..40], &create_key);
            assert_eq!(&bytes[40..72], &[0u8; 32], "config_authority is a plain, default Pubkey");
            assert_eq!(u16::from_le_bytes([bytes[72], bytes[73]]), 2, "threshold");
            assert_eq!(u32::from_le_bytes(bytes[74..78].try_into().unwrap()), time_lock);
            assert_eq!(u64::from_le_bytes(bytes[78..86].try_into().unwrap()), 0);
            assert_eq!(u64::from_le_bytes(bytes[86..94].try_into().unwrap()), 0);
            assert_eq!(bytes[94], 0, "rent_collector is None");
            assert_eq!(bytes[95], squads::multisig_address(&key(create_key)).1, "stored bump");
            assert_eq!(u32::from_le_bytes(bytes[96..100].try_into().unwrap()), 3, "member count");
            let mut seen = Vec::new();
            for member in 0..3 {
                let start = 100 + member * 33;
                seen.push(bytes[start..start + 32].to_vec());
                assert_eq!(bytes[start + 32], 7, "member {member} permissions mask");
            }
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), 3, "members are distinct");
            assert_eq!(bytes.len(), 100 + 3 * 33);
        }
    }

    #[test]
    fn squads_addresses_follow_the_documented_seeds() {
        let (g_multisig, _) = Pubkey::find_program_address(
            &[b"multisig", b"multisig", &G_CREATE_KEY],
            &SQUADS_PROGRAM_ID,
        );
        let (g_vault, _) = Pubkey::find_program_address(
            &[b"multisig", g_multisig.as_ref(), b"vault", &[0]],
            &SQUADS_PROGRAM_ID,
        );
        assert_eq!(squads::multisig_address(&key(G_CREATE_KEY)).0, g_multisig);
        assert_eq!(squads::vault_address(&g_multisig, 0).0, g_vault);
        assert_ne!(g_vault, g_multisig);
        assert_ne!(
            squads::vault_address(&g_multisig, 0).0,
            squads::vault_address(&g_multisig, 1).0
        );
    }

    #[test]
    fn allocation_padding_matches_multisig_size() {
        assert_eq!(squads::multisig_space(3), 231);
        let padded = squads::pad_to_allocation(squads::governance_bytes(), 3);
        assert_eq!(padded.len(), 231);
        assert_eq!(&padded[..199], squads::governance_bytes().as_slice());
        assert!(padded[199..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn committed_loader_fixtures_equal_the_generator_output() {
        let g_vault = squads::vault_address(&squads::multisig_address(&key(G_CREATE_KEY)).0, 0).0;
        assert_eq!(loader::committed_program(), loader::program_header());
        assert_eq!(
            loader::committed_programdata(),
            loader::programdata_header(1, Some(g_vault))
        );
        assert_eq!(loader::committed_programdata(), loader::governance_programdata_header());

        let program = loader::committed_program();
        assert_eq!(program.len(), 36);
        assert_eq!(u32::from_le_bytes(program[0..4].try_into().unwrap()), 2, "Program variant");
        assert_eq!(&program[4..36], loader::program_data_address().as_ref());

        let programdata = loader::committed_programdata();
        assert_eq!(programdata.len(), 45);
        assert_eq!(u32::from_le_bytes(programdata[0..4].try_into().unwrap()), 3, "ProgramData variant");
        assert_eq!(u64::from_le_bytes(programdata[4..12].try_into().unwrap()), 1, "slot");
        assert_eq!(programdata[12], 1, "Some(authority)");
        assert_eq!(&programdata[13..45], g_vault.as_ref());
        assert_eq!(
            loader::program_data_address(),
            Pubkey::find_program_address(&[ID.as_ref()], &solana_sdk_ids::bpf_loader_upgradeable::id()).0
        );
    }

    #[test]
    fn immutable_programdata_header_is_zero_padded_to_the_metadata_size() {
        let header = loader::programdata_header(1, None);
        assert_eq!(header.len(), 45);
        assert_eq!(header[12], 0, "None authority tag");
        assert!(header[13..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn fixture_readmes_document_the_derived_addresses() {
        let squads_readme = fs::read_to_string(fixture_dir().join("squads/README.md"))
            .expect("fixtures/squads/README.md");
        let loader_readme = fs::read_to_string(fixture_dir().join("loader/README.md"))
            .expect("fixtures/loader/README.md");
        let g_multisig = squads::multisig_address(&key(G_CREATE_KEY)).0;
        let p_multisig = squads::multisig_address(&key(P_CREATE_KEY)).0;
        let g_vault = squads::vault_address(&g_multisig, 0).0;
        let p_vault = squads::vault_address(&p_multisig, 0).0;
        for documented in [
            squads::PINNED_COMMIT.to_string(),
            SQUADS_PROGRAM_ID.to_string(),
            g_multisig.to_string(),
            g_vault.to_string(),
            p_multisig.to_string(),
            p_vault.to_string(),
        ] {
            assert!(
                squads_readme.contains(&documented),
                "fixtures/squads/README.md must document {documented}"
            );
        }
        for documented in [
            ID.to_string(),
            loader::program_data_address().to_string(),
            g_vault.to_string(),
        ] {
            assert!(
                loader_readme.contains(&documented),
                "fixtures/loader/README.md must document {documented}"
            );
        }
        for (readme, path) in [
            (&squads_readme, "squads/multisig_g.bin"),
            (&squads_readme, "squads/multisig_p.bin"),
            (&loader_readme, "loader/program.bin"),
            (&loader_readme, "loader/programdata.bin"),
        ] {
            let bytes = fs::read(fixture_dir().join(path)).expect("committed fixture");
            let sha: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert!(readme.contains(&sha), "README must record the sha256 of {path}");
            assert!(
                readme.contains(&format!("{} bytes", bytes.len())),
                "README must record the byte length of {path}"
            );
        }
    }

    #[test]
    fn error_codes_asserted_by_the_m3_suite_are_frozen() {
        assert_eq!(ErrorCode::InvalidState as u32 + 6000, 6001);
        assert_eq!(ErrorCode::NotAuthorized as u32 + 6000, 6004);
        assert_eq!(ErrorCode::WrongAccount as u32 + 6000, 6005);
        assert_eq!(ErrorCode::WrongOwner as u32 + 6000, 6006);
        assert_eq!(ErrorCode::InsufficientOperations as u32 + 6000, 6007);
        assert_eq!(ErrorCode::ProtectedDestination as u32 + 6000, 6008);
        assert_eq!(ErrorCode::InsufficientManagement as u32 + 6000, 6023);
        assert_eq!(ErrorCode::ConfigNotActivatable as u32 + 6000, 6030);
        assert_eq!(ErrorCode::WrongGovernanceProfile as u32 + 6000, 6031);
        assert_eq!(anchor_lang::error::ErrorCode::AccountNotSigner as u32, 3010);
    }

    #[test]
    fn custom_code_parses_instruction_error_strings() {
        assert_eq!(parse_custom_code("InstructionError(1, Custom(6004))"), Some(6004));
        assert_eq!(parse_custom_code("InstructionError(2, Custom(6031))"), Some(6031));
        assert_eq!(parse_custom_code("InstructionError(1, InvalidAccountData)"), None);
        assert_eq!(parse_custom_code("transaction signing failed: not enough signers"), None);
    }

    /// Constructing the runtime loads the ELFs, so this is the one fixture
    /// test that needs `MC_FINANCIAL_SBF` and `MC_STANDIN_SBF`: the loader and
    /// Squads fixtures are installed exactly as committed, and the stand-in
    /// is loaded at the Squads id.
    #[test]
    fn new_uninitialized_installs_the_committed_fixtures() {
        let runtime = FinancialRuntime::new_uninitialized();
        let g = runtime.account(runtime.g_multisig());
        let p = runtime.account(runtime.p_multisig());
        assert_eq!(g.owner, SQUADS_PROGRAM_ID);
        assert_eq!(p.owner, SQUADS_PROGRAM_ID);
        assert_eq!(g.data, squads::committed_governance());
        assert_eq!(p.data, squads::committed_guardian());
        let program = runtime.account(ID);
        assert!(program.executable);
        assert_eq!(program.data, loader::committed_program());
        let programdata = runtime.account(runtime.program_data_address());
        assert_eq!(&programdata.data[..45], loader::committed_programdata().as_slice());
        assert!(programdata.data.len() > 45, "the ELF follows the 45-byte header");
        assert!(runtime.account(SQUADS_PROGRAM_ID).executable, "stand-in loaded at the Squads id");
        assert!(runtime.standin_artifact_sha256().is_some());
        assert!(runtime.svm.get_account(&runtime.pool_address()).is_none(), "no state injected");
    }
}
