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
//!
//! The M7 close suite (`close`) additionally loads the CAPTURED mainnet ORAO
//! VRF-with-callback executable (`MC_ORAO_SBF` overrides the committed path;
//! its SHA-256 is pinned by `solana/probes/runtime/fixtures/orao-provenance.json`)
//! at its mainnet id, initialises, configures (ONE synthetic fulfill authority,
//! `[0x55; 32]`) and registers it with real ORAO transactions, and drives every
//! close step against it.  No cluster latency, no real Squads, no live
//! randomness: see `close.rs` for the places a `CloseRecord` is rewritten
//! (labelled `synthetic`).

pub mod allocation;
pub mod close;
pub mod controls;
pub mod financial;
pub mod fuzz;
pub mod positions_suite;
pub mod provisioning;
pub mod reserve;
pub mod runtime;
pub mod world;

pub use allocation::run_allocation_suite;
pub use close::run_close_suite;
pub use controls::run_controls_m3_suite;
pub use financial::{
    run_allocation_golden_flow, run_controls_suite, run_red_allocation, run_red_claim, run_red_open,
};
pub use fuzz::run_fuzz_suite;
pub use positions_suite::run_positions_suite;
pub use provisioning::run_provisioning_suite;
pub use reserve::run_reserve_suite;
pub use runtime::{fixtures, FinancialRuntime, Observation};

/// Run the small evidence CLI without making a live transaction claim.
pub fn run_suite() -> Vec<Observation> {
    let mut observations = vec![run_red_open(), run_red_claim(), run_red_allocation()];
    observations.extend(run_allocation_golden_flow());
    observations.extend(run_controls_suite());
    observations.extend(run_controls_m3_suite());
    observations.extend(run_provisioning_suite());
    observations.extend(run_positions_suite());
    observations.extend(run_allocation_suite());
    observations.extend(run_close_suite());
    observations.extend(run_reserve_suite());
    observations.extend(run_fuzz_suite());
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

    /// The M0 RED, now GREEN under the same label: one `discover_rows` call on
    /// the seeded golden (toll 10,000; A 4,000 / 6,000; two B).
    #[test]
    fn allocation_golden_t10000() {
        let observation = run_red_allocation();
        assert_eq!(
            observation.label,
            "financial.allocation.golden-t10000.discover-row"
        );
        assert!(
            observation.success,
            "allocation_golden_t10000 expected a successful compiled-SBF discovery: {observation:?}"
        );
    }

    /// The six typed instructions plus `claim_backer`, one row per
    /// instruction, driven to `IDLE` on the golden vector: 4 Discover, Senior,
    /// Seal, 4 Allocate, 4 Prune, Commit and the claim.
    #[test]
    fn allocation_six_phase_flow_has_unique_row_observations() {
        let observations = run_allocation_golden_flow();
        assert_eq!(observations.len(), 4 + 1 + 1 + 4 + 4 + 1 + 1);
        assert!(observations.iter().all(|observation| observation.success));
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

/// M4 provisioning and cleanup rows.  Each test runs one row of
/// `provisioning.rs` against the actual compiled SBF (`MC_FINANCIAL_SBF`; the
/// canonical `FinancialRuntime::new` fixture needs no stand-in).  The row
/// asserts the exact program error code of every rejection (parsed from
/// `Observation.error`), fee-only rollback on every rejection, both
/// cash-register conservation invariants after every transaction, and the exact
/// rent debit read from the runtime, never from a constant.
#[cfg(test)]
mod provisioning_tests {
    use crate::provisioning::{
        self as m4, assert_headline, assert_outcomes_consistent, assert_unique_labels,
    };
    use crate::runtime::Observation;

    fn run_row(name: &str, row: fn() -> Vec<Observation>) {
        let observations = row();
        assert_headline(&observations, name);
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
    }

    #[test]
    fn provision_snapshot_sequential_only() {
        run_row(
            m4::ROW_SNAPSHOT_SEQUENTIAL,
            m4::provision_snapshot_sequential_only,
        );
    }

    #[test]
    fn provision_from_ops_debits_free_balance_by_exact_rent() {
        run_row(
            m4::ROW_FROM_OPS,
            m4::provision_from_ops_debits_free_balance_by_exact_rent,
        );
    }

    #[test]
    fn provision_by_caller_records_owner_component_and_refund_destination() {
        run_row(
            m4::ROW_BY_CALLER,
            m4::provision_by_caller_records_owner_component_and_refund_destination,
        );
    }

    #[test]
    fn close_snapshot_and_registry_pages_always_refused_not_closable() {
        run_row(
            m4::ROW_CLOSE_SNAPSHOT_REGISTRY,
            m4::close_snapshot_and_registry_pages_always_refused_not_closable,
        );
    }

    #[test]
    fn snapshot_pages_counter_never_decrements() {
        run_row(
            m4::ROW_COUNTER_MONOTONE,
            m4::snapshot_pages_counter_never_decrements,
        );
    }

    #[test]
    fn close_candidate_page_refused_until_its_own_close_record_is_terminal() {
        run_row(
            m4::ROW_CLOSE_CANDIDATE,
            m4::close_candidate_page_refused_until_its_own_close_record_is_terminal,
        );
    }

    #[test]
    fn close_participant_page_refused_until_that_round_finalized() {
        run_row(
            m4::ROW_CLOSE_PARTICIPANT,
            m4::close_participant_page_refused_until_that_round_finalized,
        );
    }

    #[test]
    fn close_candidate_page_refunds_components_once() {
        run_row(
            m4::ROW_CANDIDATE_REFUNDS,
            m4::close_candidate_page_refunds_components_once,
        );
    }

    #[test]
    fn close_close_record_always_refused() {
        run_row(
            m4::ROW_CLOSE_CLOSE_RECORD,
            m4::close_close_record_always_refused,
        );
    }

    #[test]
    fn provision_close_record_requires_next_close_id() {
        run_row(
            m4::ROW_CLOSE_RECORD_NEXT,
            m4::provision_close_record_requires_next_close_id,
        );
    }

    #[test]
    fn provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only() {
        run_row(
            m4::ROW_CANDIDATE_RESERVE,
            m4::provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only,
        );
    }

    #[test]
    fn provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent() {
        run_row(
            m4::ROW_LOCKED_DEBITS,
            m4::provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent,
        );
    }

    #[test]
    fn provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice() {
        run_row(
            m4::ROW_LOCKED_REJECTS,
            m4::provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice,
        );
    }

    #[test]
    fn provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set() {
        run_row(
            m4::ROW_LOCKED_BEYOND,
            m4::provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set,
        );
    }

    #[test]
    fn free_funding_respects_need_bound() {
        run_row(m4::ROW_NEED_BOUND, m4::free_funding_respects_need_bound);
    }

    #[test]
    fn reconcile_ops_cash_classifies_anonymous_credit_as_unallocated_excess() {
        run_row(
            m4::ROW_RECONCILE,
            m4::reconcile_ops_cash_classifies_anonymous_credit_as_unallocated_excess,
        );
    }

    #[test]
    fn rent_table() {
        let observations = m4::rent_table();
        assert_headline(&observations, m4::ROW_RENT_TABLE);
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
        let carrier = observations
            .iter()
            .find(|observation| observation.label == "financial.provisioning.rent-table")
            .expect("rent-table observation");
        let note = carrier.unsupported.as_deref().expect("rent table note");
        assert!(note.contains("local default rent (LiteSVM)"), "{note}");
        for kind in [
            "registry_page",
            "snapshot_page",
            "participant_page",
            "candidate_page",
            "close_record",
        ] {
            assert!(note.contains(kind), "rent table lists {kind}: {note}");
        }
    }

    #[test]
    fn provisioning_suite_aggregates_every_row_with_unique_labels() {
        let observations = m4::run_provisioning_suite();
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
        for row in m4::ROW_NAMES {
            assert_headline(&observations, row);
        }
        assert_eq!(m4::ROW_NAMES.len(), 17);
    }
}

/// M5 admission and payout rows.  Each test runs one row of
/// `positions_suite.rs` against the actual compiled SBF (`MC_FINANCIAL_SBF`;
/// the governed genesis fixture also loads the Squads stand-in,
/// `MC_STANDIN_SBF`).  The row asserts the exact program error code of every
/// rejection (parsed from `Observation.error`), fee-only rollback of every
/// account the `World` can touch on every rejection, both cash-register
/// identities after every transaction, and the exact lamport movement and
/// event of every success.
#[cfg(test)]
mod positions_tests {
    use crate::positions_suite::{self as m5, assert_headline};
    use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
    use crate::runtime::Observation;

    fn run_row(name: &str, row: fn() -> Vec<Observation>) {
        let observations = row();
        assert_headline(&observations, name);
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
    }

    #[test]
    fn open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol() {
        run_row(
            m5::ROW_OPEN_ONE_SOL,
            m5::open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol,
        );
    }

    #[test]
    fn withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000() {
        run_row(
            m5::ROW_WITHDRAW_ONE_DAY,
            m5::withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000,
        );
    }

    #[test]
    fn toll_with_no_backers_goes_straight_to_seed_without_ticket() {
        run_row(
            m5::ROW_TOLL_NO_BACKERS,
            m5::toll_with_no_backers_goes_straight_to_seed_without_ticket,
        );
    }

    #[test]
    fn minimum_receive_failure_is_atomic_and_fee_only() {
        run_row(
            m5::ROW_MINIMUM_RECEIVE,
            m5::minimum_receive_failure_is_atomic_and_fee_only,
        );
    }

    #[test]
    fn strict_insolvency_closes_and_equality_does_not() {
        run_row(
            m5::ROW_STRICT_INSOLVENCY,
            m5::strict_insolvency_closes_and_equality_does_not,
        );
    }

    #[test]
    fn closing_reserve_scales_with_participants_and_registry_tail() {
        run_row(
            m5::ROW_RESERVE_SCALES,
            m5::closing_reserve_scales_with_participants_and_registry_tail,
        );
    }

    #[test]
    fn closing_payout_short_by_one_lamport_rejects_before_mutation() {
        run_row(
            m5::ROW_SHORT_BY_ONE,
            m5::closing_payout_short_by_one_lamport_rejects_before_mutation,
        );
    }

    #[test]
    fn owner_funded_close_transfers_contribution_to_opscash_and_rolls_back_on_failed_payout() {
        run_row(
            m5::ROW_OWNER_FUNDED_CLOSE,
            m5::owner_funded_close_transfers_contribution_to_opscash_and_rolls_back_on_failed_payout,
        );
    }

    #[test]
    fn compound_15_maturity_vector() {
        run_row(
            m5::ROW_COMPOUND_15,
            m5::compound_15_maturity_vector,
        );
    }

    #[test]
    fn quota_third_success_blocks_fourth_until_exact_hour() {
        run_row(
            m5::ROW_QUOTA,
            m5::quota_third_success_blocks_fourth_until_exact_hour,
        );
    }

    #[test]
    fn failed_open_does_not_consume_quota_and_nonce_replay_fails() {
        run_row(
            m5::ROW_FAILED_OPEN,
            m5::failed_open_does_not_consume_quota_and_nonce_replay_fails,
        );
    }

    #[test]
    fn fund_series_a_full_gross_to_pot_entitlement_124() {
        run_row(
            m5::ROW_FUND_SERIES_A,
            m5::fund_series_a_full_gross_to_pot_entitlement_124,
        );
    }

    #[test]
    fn fund_series_a_rejected_while_ticket_active_and_when_flag_off() {
        run_row(
            m5::ROW_SERIES_A_REJECTED,
            m5::fund_series_a_rejected_while_ticket_active_and_when_flag_off,
        );
    }

    #[test]
    fn stale_round_position_is_terminal_without_write() {
        run_row(
            m5::ROW_STALE_ROUND,
            m5::stale_round_position_is_terminal_without_write,
        );
    }

    #[test]
    fn same_nonce_cannot_open_a_second_position_after_settlement() {
        run_row(
            m5::ROW_SAME_NONCE,
            m5::same_nonce_cannot_open_a_second_position_after_settlement,
        );
    }

    #[test]
    fn unknown_outcome_retry_with_same_nonce_is_idempotent() {
        run_row(
            m5::ROW_UNKNOWN_OUTCOME_OPEN,
            m5::unknown_outcome_retry_with_same_nonce_is_idempotent,
        );
    }

    #[test]
    fn fund_series_a_retry_after_unknown_outcome_keeps_original_signature() {
        run_row(
            m5::ROW_SERIES_A_RETRY,
            m5::fund_series_a_retry_after_unknown_outcome_keeps_original_signature,
        );
    }

    #[test]
    fn third_party_fee_payer_gains_no_authority() {
        run_row(
            m5::ROW_THIRD_PARTY_PAYER,
            m5::third_party_fee_payer_gains_no_authority,
        );
    }

    #[test]
    fn destination_cannot_be_game_vault_ops_cash_or_program_pda() {
        run_row(
            m5::ROW_DESTINATION,
            m5::destination_cannot_be_game_vault_ops_cash_or_program_pda,
        );
    }

    #[test]
    fn positions_suite_aggregates_every_row_with_unique_labels() {
        let observations = m5::run_positions_suite();
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
        for row in m5::ROW_NAMES {
            assert_headline(&observations, row);
        }
        assert_eq!(m5::ROW_NAMES.len(), 19);
    }
}

/// M6 allocation rows.  Each test runs one row of `allocation.rs` against the
/// actual compiled SBF (`MC_FINANCIAL_SBF`; the governed genesis fixture also
/// loads the Squads stand-in, `MC_STANDIN_SBF`).  The row asserts the exact
/// program error code of every rejection (parsed from `Observation.error`),
/// fee-only rollback of every account the `World` can touch on every
/// rejection, both cash-register identities after every transaction, and the
/// exact credits, books and events of every success.  The first three rows are
/// the plan's goldens; the last three rows go beyond the plan list (CU and
/// packet capture at four registry sizes, malformed batch shapes, and the
/// writable stand-in `apply_senior` needs when the ticket has no open A).
#[cfg(test)]
mod allocation_tests {
    use crate::allocation::{self as m6, assert_headline};
    use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
    use crate::runtime::Observation;

    fn run_row(name: &str, row: fn() -> Vec<Observation>) {
        let observations = row();
        assert_headline(&observations, name);
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
    }

    #[test]
    fn golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500() {
        run_row(
            m6::ROW_GOLDEN_T10000,
            m6::golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500,
        );
    }

    #[test]
    fn golden_no_b_a1000_a10000_seed_5750_a1000_a3250() {
        run_row(
            m6::ROW_GOLDEN_NO_B,
            m6::golden_no_b_a1000_a10000_seed_5750_a1000_a3250,
        );
    }

    #[test]
    fn golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed() {
        run_row(
            m6::ROW_GOLDEN_ONLY_A_AND_B1,
            m6::golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed,
        );
    }

    #[test]
    fn chunk_sizes_1_4_8_produce_identical_state() {
        run_row(
            m6::ROW_CHUNK_SIZES,
            m6::chunk_sizes_1_4_8_produce_identical_state,
        );
    }

    #[test]
    fn replaying_a_completed_discover_chunk_fails_without_credit() {
        run_row(
            m6::ROW_REPLAY,
            m6::replaying_a_completed_discover_chunk_fails_without_credit,
        );
    }

    #[test]
    fn stale_ticket_id_is_rejected_on_every_phase_instruction() {
        run_row(
            m6::ROW_STALE_TICKET,
            m6::stale_ticket_id_is_rejected_on_every_phase_instruction,
        );
    }

    #[test]
    fn discovery_accumulates_a_remainders_above_u128_in_u256() {
        run_row(
            m6::ROW_U256,
            m6::discovery_accumulates_a_remainders_above_u128_in_u256,
        );
    }

    #[test]
    fn prune_same_page_swap_delete_clears_last_slot_on_first_page() {
        run_row(
            m6::ROW_SAME_PAGE_SWAP,
            m6::prune_same_page_swap_delete_clears_last_slot_on_first_page,
        );
    }

    #[test]
    fn prune_alias_when_pruned_row_is_last_row_marks_it_inactive() {
        run_row(
            m6::ROW_PRUNE_ALIAS,
            m6::prune_alias_when_pruned_row_is_last_row_marks_it_inactive,
        );
    }

    #[test]
    fn duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once(
    ) {
        run_row(
            m6::ROW_DUPLICATES,
            m6::duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once,
        );
    }

    #[test]
    fn claim_during_ticket_pays_only_finalized_and_commit_adds_once() {
        run_row(
            m6::ROW_CLAIM_DURING_TICKET,
            m6::claim_during_ticket_pays_only_finalized_and_commit_adds_once,
        );
    }

    #[test]
    fn claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history() {
        run_row(
            m6::ROW_CLAIM_CAP,
            m6::claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history,
        );
    }

    #[test]
    fn reopen_capped_a_keeps_first_funding_seq() {
        run_row(m6::ROW_REOPEN, m6::reopen_capped_a_keeps_first_funding_seq);
    }

    #[test]
    fn normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait() {
        run_row(
            m6::ROW_NORMAL_DEPOSIT,
            m6::normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait,
        );
    }

    #[test]
    fn conservation_identity_holds_after_every_phase() {
        run_row(
            m6::ROW_CONSERVATION,
            m6::conservation_identity_holds_after_every_phase,
        );
    }

    #[test]
    fn registry_sizes_16_64_100_256_fit_the_legacy_packet_at_8_rows() {
        run_row(
            m6::ROW_SIZES,
            m6::registry_sizes_16_64_100_256_fit_the_legacy_packet_at_8_rows,
        );
    }

    #[test]
    fn malformed_batches_are_invalid_state_and_fee_only() {
        run_row(
            m6::ROW_MALFORMED,
            m6::malformed_batches_are_invalid_state_and_fee_only,
        );
    }

    #[test]
    fn senior_without_an_open_a_needs_a_writable_stand_in() {
        run_row(
            m6::ROW_SENIOR_NO_A,
            m6::senior_without_an_open_a_needs_a_writable_stand_in,
        );
    }

    #[test]
    fn allocation_suite_aggregates_every_row_with_unique_labels() {
        let observations = m6::run_allocation_suite();
        assert_outcomes_consistent(&observations);
        assert_unique_labels(&observations);
        for row in m6::ROW_NAMES {
            assert_headline(&observations, row);
        }
        assert_eq!(m6::ROW_NAMES.len(), 18);
        assert!(
            observations
                .iter()
                .all(|observation| observation.label.starts_with("financial.allocation.m6.")),
            "every label lives under the m6 root"
        );
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
    fn error_codes_asserted_by_the_provisioning_suite_are_frozen() {
        assert_eq!(ErrorCode::InvalidState as u32 + 6000, 6001);
        assert_eq!(ErrorCode::WrongAccount as u32 + 6000, 6005);
        assert_eq!(ErrorCode::WrongOwner as u32 + 6000, 6006);
        assert_eq!(ErrorCode::InsufficientOperations as u32 + 6000, 6007);
        assert_eq!(ErrorCode::WrongIndex as u32 + 6000, 6032);
        assert_eq!(ErrorCode::NotClosable as u32 + 6000, 6033);
        assert_eq!(ErrorCode::AlreadyClosed as u32 + 6000, 6034);
        assert_eq!(ErrorCode::PageNotProvisioned as u32 + 6000, 6035);
        assert_eq!(ErrorCode::LockNotPermitted as u32 + 6000, 6036);
    }

    #[test]
    fn error_codes_asserted_by_the_allocation_suite_are_frozen() {
        assert_eq!(ErrorCode::InvalidState as u32 + 6000, 6001);
        assert_eq!(ErrorCode::WrongAccount as u32 + 6000, 6005);
        assert_eq!(ErrorCode::AllocationPending as u32 + 6000, 6010);
        assert_eq!(ErrorCode::InvalidPhase as u32 + 6000, 6018);
        assert_eq!(ErrorCode::NothingToClaim as u32 + 6000, 6022);
        assert_eq!(ErrorCode::DuplicateWritableAccount as u32 + 6000, 6026);
        assert_eq!(ErrorCode::StaleTicket as u32 + 6000, 6039);
        assert_eq!(ErrorCode::StaleCursor as u32 + 6000, 6040);
    }

    /// The Borsh client ABI of `PageKind` and `Funding`, and the placement of
    /// the arguments in the instruction data, read byte by byte.
    #[test]
    fn page_kind_and_funding_borsh_encodings_are_frozen() {
        use anchor_lang::{AnchorSerialize, Discriminator, InstructionData};
        use musical_chairs_program::{instruction, Funding, PageKind};

        fn bytes<T: AnchorSerialize>(value: T) -> Vec<u8> {
            value.try_to_vec().expect("Borsh")
        }
        assert_eq!(bytes(PageKind::Registry), vec![0]);
        assert_eq!(bytes(PageKind::Snapshot), vec![1]);
        let mut participant = vec![2];
        participant.extend_from_slice(&7u64.to_le_bytes());
        assert_eq!(bytes(PageKind::Participant { round: 7 }), participant);
        let mut candidate = vec![3];
        candidate.extend_from_slice(&9u64.to_le_bytes());
        candidate.push(1);
        assert_eq!(
            bytes(PageKind::Candidate {
                close_id: 9,
                set_tag: 1
            }),
            candidate
        );
        assert_eq!(bytes(Funding::Free), vec![0]);
        assert_eq!(bytes(Funding::Caller), vec![1]);
        assert_eq!(bytes(Funding::LockedClose), vec![2]);

        // discriminator || kind || index (u64 LE) || funding
        let data = instruction::ProvisionPage {
            kind: PageKind::Candidate {
                close_id: 9,
                set_tag: 1,
            },
            index: 5,
            funding: Funding::LockedClose,
        }
        .data();
        let mut expected = instruction::ProvisionPage::DISCRIMINATOR.to_vec();
        expected.extend_from_slice(&candidate);
        expected.extend_from_slice(&5u64.to_le_bytes());
        expected.push(2);
        assert_eq!(data, expected);

        // discriminator || close_id (u64 LE) || funding
        let data = instruction::ProvisionCloseRecord {
            close_id: 4,
            funding: Funding::Caller,
        }
        .data();
        let mut expected = instruction::ProvisionCloseRecord::DISCRIMINATOR.to_vec();
        expected.extend_from_slice(&4u64.to_le_bytes());
        expected.push(1);
        assert_eq!(data, expected);

        // discriminator || kind (no index: the page stores its own)
        let data = instruction::CloseSharedAccount {
            kind: PageKind::Participant { round: 7 },
        }
        .data();
        let mut expected = instruction::CloseSharedAccount::DISCRIMINATOR.to_vec();
        expected.extend_from_slice(&participant);
        assert_eq!(data, expected);

        // discriminator only
        assert_eq!(
            instruction::ReconcileOpsCash.data(),
            instruction::ReconcileOpsCash::DISCRIMINATOR.to_vec()
        );
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

/// M7 close-lifecycle rows.  Each test runs one row of `close.rs` against the
/// actual compiled SBF, the captured ORAO executable and the test-only Squads
/// vault stand-in (`MC_FINANCIAL_SBF`, `MC_STANDIN_SBF`; `MC_ORAO_SBF` optional).
/// A row asserts the exact program error code of every rejection (parsed from
/// `Observation.error`), fee-only rollback on every rejection, and both
/// cash-register identities plus the partition identity on every success.
#[cfg(test)]
mod close_tests {
    use crate::close as m7;
    use musical_chairs_program::ErrorCode;

    #[test]
    fn provider_setup_registers_a_g_owned_client_against_the_pinned_orao_elf() {
        m7::run_row(m7::ROW_PROVIDER, m7::provider_setup_row);
    }

    #[test]
    fn build_candidates_visits_every_participant_once_and_seals_digests() {
        m7::run_row(m7::ROW_BUILD_SEALS, m7::build_candidates_visits_every_participant_once_and_seals_digests);
    }

    #[test]
    fn build_candidates_rejects_skipped_or_reordered_rows() {
        m7::run_row(m7::ROW_BUILD_REJECTS, m7::build_candidates_rejects_skipped_or_reordered_rows);
    }

    #[test]
    fn same_owner_multi_position_aggregation_and_sealed_userround_immutability() {
        m7::run_row(m7::ROW_AGGREGATION, m7::same_owner_multi_position_aggregation_and_sealed_userround_immutability);
    }

    #[test]
    fn sealed_with_zero_losers_skips_no_candidates_without_request() {
        m7::run_row(m7::ROW_ZERO_LOSERS, m7::sealed_with_zero_losers_skips_no_candidates_without_request);
    }

    #[test]
    fn request_round_draw_creates_request_with_protocol_seed_and_marker_atomically() {
        m7::run_row(m7::ROW_REQUEST_CREATES, m7::request_round_draw_creates_request_with_protocol_seed_and_marker_atomically);
    }

    #[test]
    fn protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector() {
        m7::run_row(m7::ROW_SEED_VECTORS, m7::protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector);
    }

    #[test]
    fn request_decrements_remaining_partitions_sponsor_first_and_records_spent_split() {
        m7::run_row(m7::ROW_REQUEST_PARTITIONS, m7::request_decrements_remaining_partitions_sponsor_first_and_records_spent_split);
    }

    #[test]
    fn request_cpi_failure_leaves_partitions_and_ledger_unchanged() {
        m7::run_row(m7::ROW_REQUEST_CPI_FAILURE, m7::request_cpi_failure_leaves_partitions_and_ledger_unchanged);
    }

    #[test]
    fn fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting() {
        m7::run_row(m7::ROW_FULFILLMENT, m7::fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting);
    }

    #[test]
    fn observe_request_fulfilled_once_sets_eligible_and_owner_refundable_remaining_and_rejects_pending() {
        m7::run_row(m7::ROW_OBSERVE, m7::observe_request_fulfilled_once_sets_eligible_and_owner_refundable_remaining_and_rejects_pending);
    }

    #[test]
    fn changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible() {
        m7::run_row(m7::ROW_CHANGED_RENT, m7::changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible);
    }

    #[test]
    fn extra_request_account_lamports_reduce_eligibility_never_increase() {
        m7::run_row(m7::ROW_EXTRA_LAMPORTS, m7::extra_request_account_lamports_reduce_eligibility_never_increase);
    }

    #[test]
    fn withdraw_provider_return_executes_orao_withdraw_cpi_and_asserts_client_and_opscash_deltas() {
        m7::run_row(m7::ROW_WITHDRAW_RETURN, m7::withdraw_provider_return_executes_orao_withdraw_cpi_and_asserts_client_and_opscash_deltas);
    }

    #[test]
    fn two_partial_provider_returns_then_replay_pay_exact_owner_residual_once() {
        m7::run_row(m7::ROW_TWO_PARTIAL, m7::two_partial_provider_returns_then_replay_pay_exact_owner_residual_once);
    }

    #[test]
    fn undeliverable_owner_share_is_refused_before_the_cpi_and_retried_once_the_destination_is_funded() {
        m7::run_row(m7::ROW_RETURN_PREFLIGHT, m7::undeliverable_owner_share_is_refused_before_the_cpi_and_retried_once_the_destination_is_funded);
    }

    #[test]
    fn g_signature_without_executed_withdraw_cpi_credits_nothing_and_anonymous_opscash_surplus_is_never_reclassified() {
        m7::run_row(m7::ROW_G_SIGNATURE, m7::g_signature_without_executed_withdraw_cpi_credits_nothing_and_anonymous_opscash_surplus_is_never_reclassified);
    }

    #[test]
    fn withdraw_rejects_wrong_client_wrong_destination_and_amount_above_eligible() {
        m7::run_row(m7::ROW_WITHDRAW_REJECTS, m7::withdraw_rejects_wrong_client_wrong_destination_and_amount_above_eligible);
    }

    #[test]
    fn cpi_failure_rolls_back_all_accounting() {
        m7::run_row(m7::ROW_WITHDRAW_CPI_FAILURE, m7::cpi_failure_rolls_back_all_accounting);
    }

    #[test]
    fn two_closes_cannot_claim_the_same_reimbursement() {
        m7::run_row(m7::ROW_TWO_CLOSES_REIMBURSEMENT, m7::two_closes_cannot_claim_the_same_reimbursement);
    }

    #[test]
    fn late_fulfillment_after_expiry_becomes_eligible_only_when_observed() {
        m7::run_row(m7::ROW_LATE_FULFILLMENT, m7::late_fulfillment_after_expiry_becomes_eligible_only_when_observed);
    }

    #[test]
    fn locked_remaining_total_aggregate_equals_sum_of_remaining_partitions_after_every_transition() {
        m7::run_row(m7::ROW_PARTITION_AGGREGATE, m7::locked_remaining_total_aggregate_equals_sum_of_remaining_partitions_after_every_transition);
    }

    #[test]
    fn observed_deficit_then_real_return_lowers_it_and_never_blocks_commit_expire_finalize_or_claims() {
        m7::run_row(m7::ROW_DEFICIT_THEN_RETURN, m7::observed_deficit_then_real_return_lowers_it_and_never_blocks_commit_expire_finalize_or_claims);
    }

    #[test]
    fn expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later() {
        m7::run_row(m7::ROW_EXPIRED_UNFULFILLED, m7::expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later);
    }

    #[test]
    fn g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims() {
        m7::run_row(m7::ROW_G_WITHDRAWS_ELSEWHERE, m7::g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims);
    }

    #[test]
    fn increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once() {
        m7::run_row(m7::ROW_BUDGET, m7::increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once);
    }

    #[test]
    fn settlement_refunds_only_remaining_partitions_never_spent_amounts() {
        m7::run_row(m7::ROW_SETTLEMENT, m7::settlement_refunds_only_remaining_partitions_never_spent_amounts);
    }

    #[test]
    fn anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent() {
        m7::run_row(m7::ROW_ANONYMOUS_CREDIT, m7::anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent);
    }

    #[test]
    fn request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner() {
        m7::run_row(m7::ROW_REQUEST_REJECTS_PROVIDER, m7::request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner);
    }

    #[test]
    fn owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution() {
        m7::run_row(m7::ROW_OWNER_FALLBACK, m7::owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution);
    }

    #[test]
    fn request_after_deadline_is_rejected_before_fees() {
        m7::run_row(m7::ROW_REQUEST_AFTER_DEADLINE, m7::request_after_deadline_is_rejected_before_fees);
    }

    #[test]
    fn collision_with_preexisting_request_fails_closed_no_alternate_seed() {
        m7::run_row(m7::ROW_COLLISION, m7::collision_with_preexisting_request_fails_closed_no_alternate_seed);
    }

    #[test]
    fn capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop() {
        m7::run_row(m7::ROW_CAPTURE_DEADLINE, m7::capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop);
    }

    #[test]
    fn apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116() {
        m7::run_row(m7::ROW_APPLY_PROMOTION, m7::apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116);
    }

    #[test]
    fn apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state() {
        m7::run_row(m7::ROW_ENTROPY_EXHAUSTED, m7::apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state);
    }

    #[test]
    fn expire_promotion_requires_clock_past_deadline_and_no_capture() {
        m7::run_row(m7::ROW_EXPIRE, m7::expire_promotion_requires_clock_past_deadline_and_no_capture);
    }

    #[test]
    fn timely_capture_cannot_be_expired_and_applies_after_deadline() {
        m7::run_row(m7::ROW_TIMELY_CAPTURE, m7::timely_capture_cannot_be_expired_and_applies_after_deadline);
    }

    #[test]
    fn expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash() {
        m7::run_row(m7::ROW_EXPIRE_DURING_BUILD, m7::expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash);
    }

    #[test]
    fn finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once() {
        m7::run_row(m7::ROW_FINALIZE, m7::finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once);
    }

    #[test]
    fn direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round() {
        m7::run_row(m7::ROW_COMPOSED, m7::direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round);
    }

    #[test]
    fn late_authentic_fulfillment_after_skip_cannot_mint_or_revive() {
        m7::run_row(m7::ROW_LATE_AUTHENTIC, m7::late_authentic_fulfillment_after_skip_cannot_mint_or_revive);
    }

    #[test]
    fn second_close_cannot_reuse_first_close_request_binding() {
        m7::run_row(m7::ROW_SECOND_CLOSE_BINDING, m7::second_close_cannot_reuse_first_close_request_binding);
    }

    #[test]
    fn two_closes_two_rounds_sequence() {
        m7::run_row(m7::ROW_TWO_CLOSES_SEQUENCE, m7::two_closes_two_rounds_sequence);
    }

    /// Pure host row: the sampler's all-64-rejected exhaustion path.
    #[test]
    fn sampler_all_64_counters_rejected_pure() {
        m7::sampler_all_64_counters_rejected_pure();
    }

    /// Pure host row: the committed seed-vector file is the program sampler's.
    #[test]
    fn seed_vectors_file_matches_the_program_sampler() {
        m7::seed_vectors_file_matches_the_program_sampler();
    }

    #[test]
    fn close_suite_lists_every_plan_row_once() {
        let rows = m7::rows();
        assert_eq!(rows.len(), m7::ROW_NAMES.len());
        for ((name, _), expected) in rows.iter().zip(m7::ROW_NAMES) {
            assert_eq!(*name, expected);
        }
        let unique: std::collections::BTreeSet<_> = m7::ROW_NAMES.iter().collect();
        assert_eq!(unique.len(), m7::ROW_NAMES.len());
    }

    #[test]
    fn close_suite_aggregates_every_row_with_unique_labels() {
        let observations = m7::run_close_suite();
        for (name, _) in m7::rows() {
            m7::assert_headline(&observations, name);
        }
        assert!(observations.iter().all(|observation| observation.label.starts_with("financial.close.")));
    }

    #[test]
    fn error_codes_asserted_by_the_close_suite_are_frozen() {
        let code = |error: ErrorCode| error as u32 + 6000;
        assert_eq!(code(ErrorCode::InvalidState), 6001);
        assert_eq!(code(ErrorCode::NotAuthorized), 6004);
        assert_eq!(code(ErrorCode::WrongAccount), 6005);
        assert_eq!(code(ErrorCode::WrongOwner), 6006);
        assert_eq!(code(ErrorCode::InsufficientOperations), 6007);
        assert_eq!(code(ErrorCode::Closing), 6011);
        assert_eq!(code(ErrorCode::StaleRound), 6016);
        assert_eq!(code(ErrorCode::CursorMismatch), 6019);
        assert_eq!(code(ErrorCode::Collision), 6028);
        assert_eq!(code(ErrorCode::WrongProviderTuple), 6029);
        assert_eq!(code(ErrorCode::PageNotProvisioned), 6035);
        assert_eq!(code(ErrorCode::ProviderFeeExceeded), 6041);
        assert_eq!(code(ErrorCode::RequestNotFulfilled), 6042);
        assert_eq!(code(ErrorCode::DeadlinePassed), 6043);
        assert_eq!(code(ErrorCode::DeadlineNotReached), 6044);
        assert_eq!(code(ErrorCode::PromotionTerminal), 6045);
        assert_eq!(code(ErrorCode::NotSealed), 6046);
        assert_eq!(code(ErrorCode::CandidatesIncomplete), 6047);
        assert_eq!(code(ErrorCode::NothingEligible), 6048);
        assert_eq!(code(ErrorCode::AlreadyObserved), 6049);
        // Framework codes the suite asserts.
        assert_eq!(anchor_lang::error::ErrorCode::AccountNotSigner as u32, 3010);
        assert_eq!(anchor_lang::error::ErrorCode::ConstraintSeeds as u32, 2006);
    }
}

#[cfg(test)]
mod reserve_tests {
    use crate::fuzz as fuzz8;
    use crate::reserve as m8;
    use musical_chairs_program::ErrorCode;

    #[test]
    fn reserve_short_by_one_lamport_rejects_closing_payout_before_mutation() {
        m8::run_row(m8::ROW_SHORT_BY_ONE, m8::reserve_short_by_one_lamport_rejects_closing_payout_before_mutation);
    }

    #[test]
    fn required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser() {
        m8::run_row(m8::ROW_BOTH_SETS, m8::required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser);
    }

    #[test]
    fn missing_b_account_is_created_from_locked_reserve_with_ops_free_zero() {
        m8::run_row(m8::ROW_MISSING_B, m8::missing_b_account_is_created_from_locked_reserve_with_ops_free_zero);
    }

    #[test]
    fn registry_tail_full_extension_page_created_from_locked_reserve() {
        m8::run_row(m8::ROW_REGISTRY_TAIL, m8::registry_tail_full_extension_page_created_from_locked_reserve);
    }

    #[test]
    fn owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution() {
        m8::run_row(m8::ROW_OWNER_QUOTED, m8::owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution);
    }

    #[test]
    fn sponsor_partition_consumed_first_then_owner() {
        m8::run_row(m8::ROW_SPONSOR_FIRST, m8::sponsor_partition_consumed_first_then_owner);
    }

    #[test]
    fn spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation() {
        m8::run_row(m8::ROW_SPARE, m8::spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation);
    }

    #[test]
    fn uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages() {
        m8::run_row(m8::ROW_UNEVEN, m8::uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages);
    }

    #[test]
    fn all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds() {
        m8::run_row(m8::ROW_ALL_FREE_LOCKED, m8::all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds);
    }

    #[test]
    fn unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination() {
        m8::run_row(m8::ROW_UNUSED_REFUNDED, m8::unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination);
    }

    #[test]
    fn client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return() {
        m8::run_row(m8::ROW_CLIENT_REIMBURSEMENT, m8::client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return);
    }

    #[test]
    fn conservation_transitions_table_8_6_holds_for_every_row() {
        m8::run_row(m8::ROW_TABLE_8_6, m8::conservation_transitions_table_8_6_holds_for_every_row);
    }

    #[test]
    fn sponsor_withdrawal_cannot_touch_locked() {
        m8::run_row(m8::ROW_SPONSOR_WITHDRAWAL, m8::sponsor_withdrawal_cannot_touch_locked);
    }

    #[test]
    fn snapshot_and_registry_pages_are_not_closable() {
        m8::run_row(m8::ROW_PERMANENT_PAGES, m8::snapshot_and_registry_pages_are_not_closable);
    }

    #[test]
    fn candidate_pages_closable_only_after_their_own_close_is_terminal() {
        m8::run_row(m8::ROW_CANDIDATE_CLOSABLE, m8::candidate_pages_closable_only_after_their_own_close_is_terminal);
    }

    #[test]
    fn participant_pages_closable_only_after_their_round_finalized() {
        m8::run_row(m8::ROW_PARTICIPANT_CLOSABLE, m8::participant_pages_closable_only_after_their_round_finalized);
    }

    #[test]
    fn old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts() {
        m8::run_row(m8::ROW_OLD_TAG, m8::old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts);
    }

    #[test]
    fn full_cash_progress_with_depleted_free_ops_funds() {
        m8::run_row(m8::ROW_DEPLETED_FREE, m8::full_cash_progress_with_depleted_free_ops_funds);
    }

    #[test]
    fn settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable() {
        m8::run_row(m8::ROW_SETTLED_LOCKS_NOTHING, m8::settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable);
    }

    #[test]
    fn cleanup_is_never_required_for_commit_finalize_or_open() {
        m8::run_row(m8::ROW_NO_CLEANUP, m8::cleanup_is_never_required_for_commit_finalize_or_open);
    }

    #[test]
    fn randomized_300_step_sequence_keeps_both_conservation_identities() {
        let observations = fuzz8::run_fuzz_suite();
        let headline = format!("financial.fuzz.{}", fuzz8::ROW);
        assert!(
            observations.iter().any(|observation| observation.label == headline),
            "the sequence produced no headline observation {headline}"
        );
        assert!(observations.iter().all(|observation| observation.label.starts_with("financial.fuzz.")));
        crate::controls::assert_outcomes_consistent(&observations);
        crate::controls::assert_unique_labels(&observations);
        assert_eq!(fuzz8::SEED, 0x4D43_4655_5A5A_3038, "the seed is MCFUZZ08");
        assert_eq!(fuzz8::STEPS, 300);
    }

    #[test]
    fn reserve_suite_lists_every_plan_row_once() {
        let rows = m8::rows();
        assert_eq!(rows.len(), m8::ROW_NAMES.len());
        for ((name, _), expected) in rows.iter().zip(m8::ROW_NAMES) {
            assert_eq!(*name, expected);
        }
        let unique: std::collections::BTreeSet<_> = m8::ROW_NAMES.iter().collect();
        assert_eq!(unique.len(), m8::ROW_NAMES.len());
        // 19 plan rows + the Codex G1 runtime-proof row here, + the randomized row in fuzz.rs.
        assert_eq!(m8::ROW_NAMES.len() + 1, 21);
    }

    #[test]
    fn reserve_suite_aggregates_every_row_with_unique_labels() {
        let observations = m8::run_reserve_suite();
        for (name, _) in m8::rows() {
            m8::assert_headline(&observations, name);
        }
        assert!(observations.iter().all(|observation| observation.label.starts_with("financial.reserve.")));
    }

    #[test]
    fn error_codes_asserted_by_the_reserve_suite_are_frozen() {
        let code = |error: ErrorCode| error as u32 + 6000;
        assert_eq!(code(ErrorCode::InvalidState), 6001);
        assert_eq!(code(ErrorCode::NotAuthorized), 6004);
        assert_eq!(code(ErrorCode::WrongAccount), 6005);
        assert_eq!(code(ErrorCode::InsufficientOperations), 6007);
        assert_eq!(code(ErrorCode::AllocationPending), 6010);
        assert_eq!(code(ErrorCode::Closing), 6011);
        assert_eq!(code(ErrorCode::StaleRound), 6016);
        assert_eq!(code(ErrorCode::ReserveUnavailable), 6024);
        assert_eq!(code(ErrorCode::WrongIndex), 6032);
        assert_eq!(code(ErrorCode::NotClosable), 6033);
        assert_eq!(code(ErrorCode::AlreadyClosed), 6034);
        assert_eq!(code(ErrorCode::LockNotPermitted), 6036);
        assert_eq!(code(ErrorCode::StaleTicket), 6039);
        assert_eq!(code(ErrorCode::DeadlineNotReached), 6044);
        assert_eq!(code(ErrorCode::PromotionTerminal), 6045);
        assert_eq!(code(ErrorCode::NotSealed), 6046);
        assert_eq!(code(ErrorCode::AlreadyObserved), 6049);
        assert_eq!(code(ErrorCode::NothingDeferred), 6050);
        assert_eq!(code(ErrorCode::RefundNotDeliverable), 6051);
    }
}
