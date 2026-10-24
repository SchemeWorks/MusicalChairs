//! M7 close-lifecycle suite: candidates, the bound ORAO draw, promotion,
//! expiry, finalization and the provider cash path.
//!
//! Every row runs the actual compiled Musical Chairs SBF **and the captured
//! mainnet ORAO VRF-with-callback executable** in LiteSVM
//! (`FinancialRuntime::new_governed_with_provider`).  The provider is
//! initialised, configured (one synthetic fulfill authority, `[0x55; 32]`) and
//! registered with real ORAO transactions; the program's G vault is the
//! `register` payer through the test-only Squads stand-in, so the registered
//! Client is G-owned exactly as spec section 5 requires.  Requests, the
//! Ed25519-precompile fulfillments and the `withdraw` CPI all execute the real
//! provider.  Limits stated up front: ONE synthetic fulfill authority, no
//! cluster latency, no real Squads program, no live randomness.
//!
//! The closing payout of every world is the REAL `withdraw_simple`, so the
//! `CloseRecord` is bound by the program rather than written as a fixture.
//! Participants other than the closer are fixtures with the `UserRound` /
//! `User` values `build_candidates` reads (`World::install_participant`).  The
//! two places a `CloseRecord` is written directly are labelled `synthetic` in
//! their row (the counter-1 mapping and the entropy-exhausted terminal): the
//! first needs a candidate count near 2^63 that no real page set can reach, the
//! second a captured output that maps to no ordinal, which a SHA-256 output
//! cannot be searched for.
//!
//! Labels are `financial.close.<row>` for a row's headline observation and
//! `financial.close.<row>.<variant>` for the supporting ones.  Every rejection
//! asserts the exact program error code parsed from `Observation.error` and
//! fee-only rollback of every account the World can touch; every success
//! asserts the OpsCash and game-vault identities (`World` checks them after
//! every recorded transaction), the partition identity
//! `ops.locked_remaining_total == sum of every CloseRecord's remaining
//! partitions`, and the exact lamport movement.  Events are decoded from the
//! event-CPI inner instructions, never from logs.
//!
//! The ORAO fee/rent figures asserted here are LiteSVM's default rent schedule
//! (6,960 lamports per byte-year) and the harness's synthetic `REQUEST_FEE`;
//! they are not mainnet quotes.

use anchor_lang::{InstructionData, ToAccountMetas};
use musical_chairs_program::{
    accounts,
    allocation::SERIES_A,
    close as pclose, instruction,
    positions::PlanKindArg,
    provider::{self, orao},
    sampling, state, CandidatesSealed, DrawCaptured, DrawRequested, ErrorCode, Funding, PageKind,
    PayoutPaid, PositionOpened, PromotionApplied, PromotionSkipped, RoundFinalized, ID,
};
use sha2::{Digest, Sha256};
use solana_sdk::{
    account::Account,
    ed25519_program,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
    sysvar,
};
use solana_system_interface::program as system_program;

use crate::controls::{assert_outcomes_consistent, assert_unique_labels, custom_code};
use crate::runtime::{anchor_ix, pda, FinancialRuntime, Observation};
use crate::world::{BackerSpec, Outcome, World, CU, DAY, SOL};

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

pub const ROW_PROVIDER: &str = "provider-setup";
pub const ROW_BUILD_SEALS: &str = "build_candidates_visits_every_participant_once_and_seals_digests";
pub const ROW_BUILD_REJECTS: &str = "build_candidates_rejects_skipped_or_reordered_rows";
pub const ROW_AGGREGATION: &str =
    "same_owner_multi_position_aggregation_and_sealed_userround_immutability";
pub const ROW_ZERO_LOSERS: &str = "sealed_with_zero_losers_skips_no_candidates_without_request";
pub const ROW_REQUEST_CREATES: &str =
    "request_round_draw_creates_request_with_protocol_seed_and_marker_atomically";
pub const ROW_SEED_VECTORS: &str =
    "protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector";
pub const ROW_REQUEST_PARTITIONS: &str =
    "request_decrements_remaining_partitions_sponsor_first_and_records_spent_split";
pub const ROW_REQUEST_CPI_FAILURE: &str =
    "request_cpi_failure_leaves_partitions_and_ledger_unchanged";
pub const ROW_FULFILLMENT: &str =
    "fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting";
pub const ROW_OBSERVE: &str =
    "observe_request_fulfilled_once_sets_eligible_and_owner_refundable_remaining_and_rejects_pending";
pub const ROW_CHANGED_RENT: &str = "changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible";
pub const ROW_EXTRA_LAMPORTS: &str =
    "extra_request_account_lamports_reduce_eligibility_never_increase";
pub const ROW_WITHDRAW_RETURN: &str =
    "withdraw_provider_return_executes_orao_withdraw_cpi_and_asserts_client_and_opscash_deltas";
pub const ROW_TWO_PARTIAL: &str =
    "two_partial_provider_returns_then_replay_pay_exact_owner_residual_once";
pub const ROW_RETURN_PREFLIGHT: &str =
    "undeliverable_owner_share_is_refused_before_the_cpi_and_retried_once_the_destination_is_funded";
pub const ROW_G_SIGNATURE: &str =
    "g_signature_without_executed_withdraw_cpi_credits_nothing_and_anonymous_opscash_surplus_is_never_reclassified";
pub const ROW_WITHDRAW_REJECTS: &str =
    "withdraw_rejects_wrong_client_wrong_destination_and_amount_above_eligible";
pub const ROW_WITHDRAW_CPI_FAILURE: &str = "cpi_failure_rolls_back_all_accounting";
pub const ROW_TWO_CLOSES_REIMBURSEMENT: &str = "two_closes_cannot_claim_the_same_reimbursement";
pub const ROW_LATE_FULFILLMENT: &str =
    "late_fulfillment_after_expiry_becomes_eligible_only_when_observed";
pub const ROW_PARTITION_AGGREGATE: &str =
    "locked_remaining_total_aggregate_equals_sum_of_remaining_partitions_after_every_transition";
pub const ROW_DEFICIT_THEN_RETURN: &str =
    "observed_deficit_then_real_return_lowers_it_and_never_blocks_commit_expire_finalize_or_claims";
pub const ROW_EXPIRED_UNFULFILLED: &str =
    "expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later";
pub const ROW_G_WITHDRAWS_ELSEWHERE: &str =
    "g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims";
pub const ROW_BUDGET: &str =
    "increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once";
pub const ROW_SETTLEMENT: &str = "settlement_refunds_only_remaining_partitions_never_spent_amounts";
pub const ROW_ANONYMOUS_CREDIT: &str =
    "anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent";
pub const ROW_REQUEST_REJECTS_PROVIDER: &str =
    "request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner";
pub const ROW_OWNER_FALLBACK: &str =
    "owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution";
pub const ROW_REQUEST_AFTER_DEADLINE: &str = "request_after_deadline_is_rejected_before_fees";
pub const ROW_COLLISION: &str = "collision_with_preexisting_request_fails_closed_no_alternate_seed";
pub const ROW_CAPTURE_DEADLINE: &str =
    "capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop";
pub const ROW_APPLY_PROMOTION: &str =
    "apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116";
pub const ROW_ENTROPY_EXHAUSTED: &str =
    "apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state";
pub const ROW_EXPIRE: &str = "expire_promotion_requires_clock_past_deadline_and_no_capture";
pub const ROW_TIMELY_CAPTURE: &str = "timely_capture_cannot_be_expired_and_applies_after_deadline";
pub const ROW_EXPIRE_DURING_BUILD: &str =
    "expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash";
pub const ROW_FINALIZE: &str =
    "finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once";
pub const ROW_COMPOSED: &str =
    "direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round";
pub const ROW_LATE_AUTHENTIC: &str = "late_authentic_fulfillment_after_skip_cannot_mint_or_revive";
pub const ROW_SECOND_CLOSE_BINDING: &str = "second_close_cannot_reuse_first_close_request_binding";
pub const ROW_TWO_CLOSES_SEQUENCE: &str = "two_closes_two_rounds_sequence";

/// Every row of the suite, in execution order.
pub const ROW_NAMES: [&str; 43] = [
    ROW_PROVIDER,
    ROW_BUILD_SEALS,
    ROW_BUILD_REJECTS,
    ROW_AGGREGATION,
    ROW_ZERO_LOSERS,
    ROW_REQUEST_CREATES,
    ROW_SEED_VECTORS,
    ROW_REQUEST_PARTITIONS,
    ROW_REQUEST_CPI_FAILURE,
    ROW_FULFILLMENT,
    ROW_OBSERVE,
    ROW_CHANGED_RENT,
    ROW_EXTRA_LAMPORTS,
    ROW_WITHDRAW_RETURN,
    ROW_TWO_PARTIAL,
    ROW_RETURN_PREFLIGHT,
    ROW_G_SIGNATURE,
    ROW_WITHDRAW_REJECTS,
    ROW_WITHDRAW_CPI_FAILURE,
    ROW_TWO_CLOSES_REIMBURSEMENT,
    ROW_LATE_FULFILLMENT,
    ROW_PARTITION_AGGREGATE,
    ROW_DEFICIT_THEN_RETURN,
    ROW_EXPIRED_UNFULFILLED,
    ROW_G_WITHDRAWS_ELSEWHERE,
    ROW_BUDGET,
    ROW_SETTLEMENT,
    ROW_ANONYMOUS_CREDIT,
    ROW_REQUEST_REJECTS_PROVIDER,
    ROW_OWNER_FALLBACK,
    ROW_REQUEST_AFTER_DEADLINE,
    ROW_COLLISION,
    ROW_CAPTURE_DEADLINE,
    ROW_APPLY_PROMOTION,
    ROW_ENTROPY_EXHAUSTED,
    ROW_EXPIRE,
    ROW_TIMELY_CAPTURE,
    ROW_EXPIRE_DURING_BUILD,
    ROW_FINALIZE,
    ROW_COMPOSED,
    ROW_LATE_AUTHENTIC,
    ROW_SECOND_CLOSE_BINDING,
    ROW_TWO_CLOSES_SEQUENCE,
];

/// The pure host rows (no transaction, no observation): the sampler's
/// all-64-rejected case (plan row `sampler_all_64_counters_rejected_pure`, also
/// pinned inside the program crate) and the committed seed-vector file.  They
/// run as `lib.rs` tests only.
pub const ROW_PURE_SAMPLER: &str = "sampler_all_64_counters_rejected_pure";
pub const ROW_PURE_FIXTURE: &str = "seed_vectors_file_matches_the_program_sampler";

// ---------------------------------------------------------------------------
// Provider fixture
// ---------------------------------------------------------------------------

/// The ORAO per-request fee of the fixture (the probes' value).
pub const REQUEST_FEE: u64 = 300_000;
const CALLBACK_DEADLINE_SLOTS: u64 = 9_000;
const TREASURY_SEED: [u8; 32] = [0x76; 32];
const TREASURY_LAMPORTS: u64 = 10_000_000;
/// The G vault must pay the Client's rent at `register` and must already be a
/// rent-exempt system account to receive and forward an ORAO `withdraw`.
const G_VAULT_LAMPORTS: u64 = 100_000_000;
const FULFILL_AUTHORITY_SEED: [u8; 32] = [0x55; 32];
const IMPOSTOR_AUTHORITY_SEED: [u8; 32] = [0x33; 32];
/// `Pool.cluster_domain` of the second genesis fixture (the first is the
/// runtime's `FIXTURE_CLUSTER_DOMAIN`); both are pinned by
/// `solana/fixtures/sampling.json` and `solana/fixtures/seed-vectors.json`.
pub const DEVNET_DOMAIN: [u8; 32] = *b"musical-chairs-devnet-domain-v1!";

#[derive(Clone, Copy, Debug)]
pub struct ProviderKeys {
    pub network_state: Pubkey,
    pub treasury: Pubkey,
    pub client: Pubkey,
    pub rng_authority: Pubkey,
}

pub fn provider_keys() -> ProviderKeys {
    ProviderKeys {
        network_state: orao::state::network_state::NetworkState::find_address(&orao::id()).0,
        treasury: Keypair::new_from_array(TREASURY_SEED).pubkey(),
        client: provider::client_address().0,
        rng_authority: provider::rng_authority_address().0,
    }
}

pub(crate) fn fulfill_authority() -> Keypair {
    Keypair::new_from_array(FULFILL_AUTHORITY_SEED)
}

fn impostor_authority() -> Keypair {
    Keypair::new_from_array(IMPOSTOR_AUTHORITY_SEED)
}

pub(crate) fn foreign_ix<A: ToAccountMetas, D: InstructionData>(
    program_id: Pubkey,
    accounts: A,
    data: D,
) -> Instruction {
    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// One Ed25519-precompile signature over `message`, all offsets in the same
/// instruction (the stable precompile ABI the probes use).
fn ed25519_instruction(pubkey: &Pubkey, signature: &[u8; 64], message: &[u8]) -> Instruction {
    let start = 16usize;
    let pubkey_offset = start as u16;
    let signature_offset = (start + 32) as u16;
    let message_offset = (start + 32 + 64) as u16;
    let mut data = Vec::with_capacity(start + 32 + 64 + message.len());
    data.extend_from_slice(&[1, 0]);
    data.extend_from_slice(&signature_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&message_offset.to_le_bytes());
    data.extend_from_slice(&(message.len() as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(pubkey.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction {
        program_id: ed25519_program::id(),
        accounts: Vec::new(),
        data,
    }
}

impl World {
    /// Initialise, configure and register the captured ORAO provider with real
    /// ORAO transactions (setup: not logged by the World), then pin the tuple
    /// into config v1 and bind the `RngAuthority` to the registered Client.
    ///
    /// * `initialize` / `configure`: network authority = the harness payer,
    ///   treasury = a synthetic system account, fee `REQUEST_FEE`, ONE synthetic
    ///   fulfill authority (`[0x55; 32]`), callback deadline 9,000 slots;
    /// * `register`: payer = the G vault (it is the `ProgramData` upgrade
    ///   authority), executed through the Squads stand-in, so the Client's owner
    ///   is the G vault; `state` = this program's `RngAuthority` PDA, no callback.
    ///
    /// Returns the three observations (all asserted successful).
    pub fn provider_setup(&mut self) -> Vec<Observation> {
        let keys = provider_keys();
        let payer = self.rt.payer.pubkey();
        let g_vault = self.rt.g_vault();
        self.rt.credit_lamports(keys.treasury, TREASURY_LAMPORTS);
        self.rt.credit_lamports(g_vault, G_VAULT_LAMPORTS);
        let mut observations = Vec::new();

        let initialize = self.rt.execute(
            "financial.close.provider-setup.initialize",
            vec![foreign_ix(
                orao::id(),
                orao::accounts::Initialize {
                    payer,
                    network_state: keys.network_state,
                    system_program: system_program::ID,
                },
                orao::instruction::Initialize {
                    params: orao::InitializeParams::new(REQUEST_FEE),
                },
            )],
            &[],
            CU,
        );
        assert!(initialize.success, "ORAO initialize failed: {initialize:?}");
        observations.push(initialize);

        let mut config =
            orao::state::network_state::NetworkConfiguration::new(payer, keys.treasury, REQUEST_FEE);
        config.callback_deadline = CALLBACK_DEADLINE_SLOTS;
        config.fulfill_authorities = vec![fulfill_authority().pubkey()];
        let configure = self.rt.execute(
            "financial.close.provider-setup.configure",
            vec![foreign_ix(
                orao::id(),
                orao::accounts::Configure {
                    payer,
                    network_state: keys.network_state,
                    system_program: system_program::ID,
                },
                orao::instruction::Configure {
                    params: orao::ConfigureParams::new(config),
                },
            )],
            &[],
            CU,
        );
        assert!(configure.success, "ORAO configure failed: {configure:?}");
        observations.push(configure);

        let (_, rng_bump) = provider::rng_authority_address();
        let register_ix = foreign_ix(
            orao::id(),
            orao::accounts::Register {
                payer: g_vault,
                program: ID,
                program_data: self.rt.program_data_address(),
                state: keys.rng_authority,
                client: keys.client,
                network_state: keys.network_state,
                system_program: system_program::ID,
            },
            orao::instruction::Register {
                params: orao::RegisterParams::new(vec![
                    state::RNG_AUTHORITY_SEED.to_vec(),
                    vec![rng_bump],
                ]),
            },
        );
        let g_multisig = self.rt.g_multisig();
        let register = self.rt.execute_via_vault(
            "financial.close.provider-setup.register",
            g_multisig,
            0,
            register_ix,
            &[],
            CU,
        );
        assert!(register.success, "ORAO register via the G vault failed: {register:?}");
        observations.push(register);

        self.rt.replace_rng_authority(state::RngAuthority {
            bump: rng_bump,
            client: keys.client,
        });
        let mut pinned = self.config(1);
        pinned.provider_program = orao::id();
        pinned.provider_network_state = keys.network_state;
        pinned.provider_treasury = keys.treasury;
        pinned.provider_client = keys.client;
        pinned.provider_request_fee_lamports = REQUEST_FEE;
        assert_eq!(
            (pinned.pending_request_bytes, pinned.fulfilled_request_bytes),
            (759, 147),
            "config v1 carries the probed request sizes"
        );
        self.rt.replace_config(pinned);
        for key in [
            keys.network_state,
            keys.treasury,
            keys.client,
            keys.rng_authority,
            g_vault,
            orao::id(),
        ] {
            self.watch(key);
        }
        self.rt.conservation_check();
        observations
    }

    /// The decoded ORAO `Client` account.
    pub fn orao_client(&self) -> orao::state::client::Client {
        self.rt.deserialize(provider_keys().client)
    }

    pub fn client_lamports(&self) -> u64 {
        self.lamports(provider_keys().client)
    }

    pub fn treasury_lamports(&self) -> u64 {
        self.lamports(provider_keys().treasury)
    }

    /// The retained request binding of `close_id` (`Pubkey::default()` before
    /// its request).
    pub fn request_account(&self, close_id: u64) -> Pubkey {
        self.close_record(close_id).request_account
    }

    /// The ORAO request account decoded (panics when absent or not ORAO's).
    pub fn request_state(&self, close_id: u64) -> orao::state::request::RequestAccount {
        self.rt.deserialize(self.request_account(close_id))
    }

    /// `true` when the close's request has been fulfilled.
    pub fn request_fulfilled(&self, close_id: u64) -> bool {
        matches!(
            self.request_state(close_id).state,
            orao::state::request::RequestState::Fulfilled(_)
        )
    }

    /// Fulfil the close's request with the synthetic authority: an Ed25519
    /// precompile signature over `client || protocol_seed`, then the real ORAO
    /// `fulfill`.
    pub fn fulfill(&mut self, variant: &str, close_id: u64) -> Outcome {
        self.fulfill_with(variant, close_id, &fulfill_authority())
    }

    /// The same instruction pair signed by a key that is not a configured
    /// fulfill authority.
    pub fn fulfill_with_wrong_authority(&mut self, variant: &str, close_id: u64) -> Outcome {
        self.fulfill_with(variant, close_id, &impostor_authority())
    }

    pub fn fulfill_with(&mut self, variant: &str, close_id: u64, authority: &Keypair) -> Outcome {
        let keys = provider_keys();
        let close = self.close_record(close_id);
        let mut message = Vec::with_capacity(64);
        message.extend_from_slice(keys.client.as_ref());
        message.extend_from_slice(&close.protocol_seed);
        let signature = authority.sign_message(&message);
        let signature_bytes: &[u8; 64] = signature
            .as_ref()
            .try_into()
            .expect("an Ed25519 signature is 64 bytes");
        let ed25519 = ed25519_instruction(&authority.pubkey(), signature_bytes, &message);
        let fulfill = foreign_ix(
            orao::id(),
            orao::accounts::Fulfill {
                payer: self.rt.payer.pubkey(),
                program: ID,
                state: keys.rng_authority,
                client: keys.client,
                request: close.request_account,
                network_state: keys.network_state,
                instruction_acc: sysvar::instructions::id(),
            },
            orao::instruction::Fulfill {
                params: orao::FulfillParams::new(),
            },
        );
        self.send(variant, vec![ed25519, fulfill], &[])
    }

    /// Clock exactly at `promotion_deadline` (equality is still timely).
    pub fn warp_to_deadline(&mut self, close_id: u64) {
        let deadline = self.close_record(close_id).promotion_deadline;
        self.warp_to(deadline);
    }

    /// Clock one second past `promotion_deadline`.
    pub fn warp_past_deadline(&mut self, close_id: u64) {
        let deadline = self.close_record(close_id).promotion_deadline;
        self.warp_to(deadline + 1);
    }
}

// ---------------------------------------------------------------------------
// Identities and the send helpers
// ---------------------------------------------------------------------------

/// Remaining sponsor + owner partitions of every `CloseRecord` that exists.
pub(crate) fn partitions_total(w: &World) -> u64 {
    let pool = w.pool();
    (1..=pool.next_close_id)
        .filter_map(|close_id| {
            w.rt.try_deserialize::<state::CloseRecord>(w.rt.close_record_address(close_id))
        })
        .map(|close| close.locked_sponsor_remaining + close.locked_owner_remaining)
        .sum()
}

/// Both conservation identities plus the partition identity of spec 8.6:
/// `ops.locked_remaining_total == sum of every close's remaining partitions`.
pub fn assert_identities(w: &World) {
    w.assert_vault_identity();
    w.assert_ops_cash_identity();
    assert_eq!(
        w.ops().locked_remaining_total,
        partitions_total(w),
        "L (ops.locked_remaining_total) must equal the sum of the remaining partitions"
    );
}

pub fn ok(w: &mut World, variant: &str, instructions: Vec<Instruction>, signers: &[&Keypair]) -> Observation {
    let observation = w
        .send(variant, instructions, signers)
        .unwrap_or_else(|failed| panic!("{} failed: {failed:?}", failed.label));
    assert_identities(w);
    observation
}

pub(crate) fn ok_g(w: &mut World, variant: &str, inner: Instruction) -> Observation {
    let observation = w
        .send_via_g(variant, inner, &[])
        .unwrap_or_else(|failed| panic!("{} failed: {failed:?}", failed.label));
    assert_identities(w);
    observation
}

pub(crate) fn failure(outcome: Outcome) -> Observation {
    match outcome {
        Err(failed) => failed,
        Ok(succeeded) => panic!("{} unexpectedly succeeded: {succeeded:?}", succeeded.label),
    }
}

/// Exactly `Custom(6000 + expected)` and fee-only rollback of every watched
/// account.
pub(crate) fn reject(
    w: &mut World,
    variant: &str,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
    expected: ErrorCode,
) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send(variant, instructions, signers));
    w.assert_rejected(&failed, expected);
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

pub(crate) fn reject_g(w: &mut World, variant: &str, inner: Instruction, expected: ErrorCode) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send_via_g(variant, inner, &[]));
    w.assert_rejected(&failed, expected);
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

/// A rejection with a framework code (Anchor constraint errors, system errors).
pub(crate) fn reject_framework(
    w: &mut World,
    variant: &str,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
    framework_code: u32,
) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send(variant, instructions, signers));
    w.assert_rejected_with_custom(&failed, framework_code);
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

/// A failure inside the provider (its error code is ORAO's, not ours): the
/// transaction failed and changed nothing but the fee.  The caller checks the
/// diagnostic it expects.
pub(crate) fn reject_provider(
    w: &mut World,
    variant: &str,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send(variant, instructions, signers));
    failed.assert_rollback_except_fee();
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

pub(crate) fn diagnostics(observation: &Observation) -> String {
    format!(
        "{} {}",
        observation.error.as_deref().unwrap_or_default(),
        observation.logs.join(" ")
    )
}

pub(crate) fn delta(observation: &Observation, key: Pubkey) -> i128 {
    observation
        .account_changes
        .iter()
        .find(|change| change.pubkey == key.to_string())
        .map_or(0, |change| change.lamport_delta)
}

pub(crate) fn headline(label: &str, row: &str) -> String {
    if label.is_empty() {
        format!("financial.close.{row}")
    } else {
        format!("financial.close.{row}.{label}")
    }
}

/// Attach `note` to the recorded observation labelled `label`.
pub(crate) fn annotate(log: &mut [Observation], label: &str, note: String) {
    let observation = log
        .iter_mut()
        .find(|observation| observation.label == label)
        .unwrap_or_else(|| panic!("no observation labelled {label} to annotate"));
    observation.unsupported = Some(note);
}

// ---------------------------------------------------------------------------
// Addresses and host-side derivations
// ---------------------------------------------------------------------------

pub(crate) const PAGE: u64 = state::PAGE_CAPACITY as u64;

pub(crate) fn candidate_page_key(close_id: u64, set_tag: u8, index: u64) -> Pubkey {
    pda(&[
        state::CANDIDATES_SEED,
        &close_id.to_le_bytes(),
        &[set_tag],
        &index.to_le_bytes(),
    ])
    .0
}

pub(crate) fn candidate_page(w: &World, close_id: u64, set_tag: u8, index: u64) -> state::CandidatePage {
    w.rt.deserialize(candidate_page_key(close_id, set_tag, index))
}

/// The owner at participant slot `slot` of `round`.
pub(crate) fn participant(w: &World, round: u64, slot: u64) -> Pubkey {
    w.participants(round, slot / PAGE).rows[(slot % PAGE) as usize]
}

/// The protocol seed of spec section 4, computed on the host from the live
/// pool domain and the sealed close.
pub(crate) fn host_seed(w: &World, close_id: u64) -> [u8; 32] {
    let pool = w.pool();
    let close = w.close_record(close_id);
    let (tag, count, digest) = pclose::selected_set(&close);
    sampling::protocol_seed(
        &pool.cluster_domain,
        &ID,
        close.round_id,
        close.close_id,
        tag,
        count,
        &digest,
    )
}

/// `(request PDA, protocol seed)` of a sealed close.
pub(crate) fn host_request(w: &World, close_id: u64) -> (Pubkey, [u8; 32]) {
    let seed = host_seed(w, close_id);
    (provider::request_binding(&provider_keys().client, &seed).0, seed)
}

/// The two candidate lists a keeper must reproduce, derived on the host from
/// the participants' on-chain `UserRound` / `User` (not from the program's
/// own candidate pages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidates {
    pub(crate) preferred: Vec<(Pubkey, u128)>,
    pub(crate) fallback: Vec<(Pubkey, u128)>,
}

pub(crate) fn expected_candidates(w: &World, close_id: u64) -> Candidates {
    expected_candidates_upto(w, close_id, w.close_record(close_id).participant_count)
}

/// [`expected_candidates`] over participant slots `0..upto` only.
pub(crate) fn expected_candidates_upto(w: &World, close_id: u64, upto: u64) -> Candidates {
    let close = w.close_record(close_id);
    let mut candidates = Candidates { preferred: Vec::new(), fallback: Vec::new() };
    for slot in 0..upto.min(close.participant_count) {
        let owner = participant(w, close.round_id, slot);
        let round = w.user_round(owner, close.round_id);
        let user = w.user(owner);
        let candidacy = pclose::classify(round.deposits, round.net_payout, user.ever_a, user.ever_b);
        if candidacy.fallback {
            candidates.fallback.push((owner, candidacy.loss));
        }
        if candidacy.preferred {
            candidates.preferred.push((owner, candidacy.loss));
        }
    }
    candidates
}

/// The ordered candidate digest of one set, by the program's chaining rule.
pub(crate) fn chain(tag: u8, rows: &[(Pubkey, u128)]) -> [u8; 32] {
    rows.iter().fold(sampling::candidate_digest_start(tag), |digest, (owner, loss)| {
        sampling::candidate_digest_next(&digest, owner, *loss)
    })
}

pub(crate) fn pages_of(count: usize) -> u64 {
    count.div_ceil(state::PAGE_CAPACITY) as u64
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

pub(crate) fn reconcile_ix(rt: &FinancialRuntime) -> Instruction {
    anchor_ix(
        accounts::ReconcileOpsCash {
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
        },
        instruction::ReconcileOpsCash,
    )
}

pub(crate) fn reconcile_client_ix(rt: &FinancialRuntime) -> Instruction {
    anchor_ix(
        accounts::ReconcileOpsCashWithClient {
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            client: provider_keys().client,
        },
        instruction::ReconcileOpsCashWithClient,
    )
}

pub(crate) fn provision_ix(
    w: &World,
    close_id: u64,
    set_tag: u8,
    index: u64,
    funding: Funding,
) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::ProvisionPage {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            page: candidate_page_key(close_id, set_tag, index),
            payer: rt.payer.pubkey(),
            close_record: Some(rt.close_record_address(close_id)),
            system_program: system_program::ID,
        },
        instruction::ProvisionPage {
            kind: PageKind::Candidate { close_id, set_tag },
            index,
            funding,
        },
    )
}

/// `[participant_page, user_round, user]` of one participant slot.
pub(crate) fn row_accounts(w: &World, close: &state::CloseRecord, slot: u64) -> Vec<AccountMeta> {
    let owner = participant(w, close.round_id, slot);
    vec![
        AccountMeta::new_readonly(w.rt.participant_address(close.round_id, slot / PAGE), false),
        AccountMeta::new_readonly(w.rt.user_round_address(close.round_id, owner), false),
        AccountMeta::new_readonly(w.rt.user_address(owner), false),
    ]
}

/// The remaining accounts of the honest batch `from..from+rows`.
pub(crate) fn build_remaining(w: &World, close_id: u64, from: u64, rows: u64) -> Vec<AccountMeta> {
    let close = w.close_record(close_id);
    (from..from + rows)
        .flat_map(|slot| row_accounts(w, &close, slot))
        .collect()
}

/// `build_candidates` with caller-chosen remaining accounts.  The candidate
/// page of each set is supplied exactly as a keeper would: the page at
/// `count / 8` when it is provisioned, else the program-id placeholder.
pub(crate) fn build_ix(w: &World, close_id: u64, rows: u8, remaining: Vec<AccountMeta>) -> Instruction {
    let rt = &w.rt;
    let close = w.close_record(close_id);
    let preferred_index = close.preferred_count / PAGE;
    let fallback_index = close.fallback_count / PAGE;
    let mut instruction = anchor_ix(
        accounts::BuildCandidates {
            pool: rt.pool_address(),
            close_record: rt.close_record_address(close_id),
            preferred_page: (close.provisioned_preferred_pages > preferred_index)
                .then(|| candidate_page_key(close_id, 0, preferred_index)),
            fallback_page: (close.provisioned_fallback_pages > fallback_index)
                .then(|| candidate_page_key(close_id, 1, fallback_index)),
            ops: Some(rt.ops_address()),
            ops_cash: Some(rt.ops_cash_address()),
            owner_refund: Some(close.owner_refund_destination),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::BuildCandidates { rows },
    );
    instruction.accounts.extend(remaining);
    instruction
}

/// The honest next batch of `rows` rows from the close's cursor.
pub(crate) fn build_next(w: &World, close_id: u64, rows: u8) -> Instruction {
    let cursor = w.close_record(close_id).candidate_cursor;
    build_ix(w, close_id, rows, build_remaining(w, close_id, cursor, u64::from(rows)))
}

/// Observation label of a step of close `close_id`: close 1 keeps the bare
/// name, later closes get a `_c<id>` suffix so labels stay unique per world.
pub(crate) fn lbl(close_id: u64, base: &str) -> String {
    if close_id == 1 {
        base.to_string()
    } else {
        format!("{base}_c{close_id}")
    }
}

/// Provision exactly the candidate pages the host-derived lists need (page
/// `ceil(count / 8)` of each non-empty set) from the locked reserve, sponsor
/// partition first.  Returns `(preferred pages, fallback pages)`.
pub fn provision_candidate_pages(w: &mut World, close_id: u64) -> (u64, u64) {
    let expected = expected_candidates(w, close_id);
    let (preferred, fallback) = (pages_of(expected.preferred.len()), pages_of(expected.fallback.len()));
    for index in 0..preferred {
        let instruction = provision_ix(w, close_id, 0, index, Funding::LockedClose);
        ok(w, &lbl(close_id, &format!("provision_preferred{index}")), vec![instruction], &[]);
    }
    for index in 0..fallback {
        let instruction = provision_ix(w, close_id, 1, index, Funding::LockedClose);
        ok(w, &lbl(close_id, &format!("provision_fallback{index}")), vec![instruction], &[]);
    }
    (preferred, fallback)
}

pub(crate) fn request_accounts(w: &World, close_id: u64) -> accounts::RequestRoundDraw {
    let rt = &w.rt;
    let keys = provider_keys();
    let close = w.close_record(close_id);
    accounts::RequestRoundDraw {
        payer: rt.payer.pubkey(),
        pool: rt.pool_address(),
        close_record: rt.close_record_address(close_id),
        config: rt.config_address(close.config_version),
        ops: rt.ops_address(),
        ops_cash: rt.ops_cash_address(),
        rng_authority: keys.rng_authority,
        provider_program: orao::id(),
        network_state: keys.network_state,
        treasury: keys.treasury,
        client: keys.client,
        request: host_request(w, close_id).0,
        system_program: system_program::ID,
        event_authority: rt.event_authority_address(),
        program: ID,
    }
}

pub(crate) fn request_ix(w: &World, close_id: u64) -> Instruction {
    anchor_ix(request_accounts(w, close_id), instruction::RequestRoundDraw)
}

pub(crate) fn capture_accounts(w: &World, close_id: u64) -> accounts::CaptureRoundDraw {
    let rt = &w.rt;
    accounts::CaptureRoundDraw {
        pool: rt.pool_address(),
        close_record: rt.close_record_address(close_id),
        request: w.request_account(close_id),
        client: provider_keys().client,
        event_authority: rt.event_authority_address(),
        program: ID,
    }
}

pub fn capture_ix(w: &World, close_id: u64) -> Instruction {
    anchor_ix(capture_accounts(w, close_id), instruction::CaptureRoundDraw)
}

/// The winner-side accounts of `apply_promotion`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ApplyKeys {
    pub(crate) candidate_page: Option<Pubkey>,
    pub(crate) winner: Option<Pubkey>,
    pub(crate) winner_user: Option<Pubkey>,
    pub(crate) winner_user_round: Option<Pubkey>,
    pub(crate) winner_backer_b: Option<Pubkey>,
    pub(crate) registry_tail: Option<Pubkey>,
}

/// The accounts the captured output maps to, derived on the host with the
/// program's own `map_output` over the sealed candidate page.  An output that
/// maps to no ordinal needs none of them.
pub(crate) fn apply_keys(w: &World, close_id: u64) -> ApplyKeys {
    let close = w.close_record(close_id);
    let (tag, count, _) = pclose::selected_set(&close);
    let Some((ordinal, _counter)) = sampling::map_output(close.randomness, close.round_id, count)
    else {
        return ApplyKeys::default();
    };
    let page = candidate_page(w, close_id, tag, ordinal / PAGE);
    let winner = page.rows[(ordinal % PAGE) as usize];
    ApplyKeys {
        candidate_page: Some(candidate_page_key(close_id, tag, ordinal / PAGE)),
        winner: Some(winner),
        winner_user: Some(w.rt.user_address(winner)),
        winner_user_round: Some(w.rt.user_round_address(close.round_id, winner)),
        winner_backer_b: Some(w.rt.backer_address(1, winner)),
        registry_tail: Some(w.rt.registry_address(w.pool().registry_len / PAGE)),
    }
}

pub(crate) fn apply_ix_with(w: &World, close_id: u64, keys: ApplyKeys) -> Instruction {
    let rt = &w.rt;
    let close = w.close_record(close_id);
    anchor_ix(
        accounts::ApplyPromotion {
            payer: rt.payer.pubkey(),
            pool: rt.pool_address(),
            close_record: rt.close_record_address(close_id),
            config: rt.config_address(close.config_version),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            candidate_page: keys.candidate_page,
            winner: keys.winner,
            winner_user: keys.winner_user,
            winner_user_round: keys.winner_user_round,
            winner_backer_b: keys.winner_backer_b,
            registry_tail: keys.registry_tail,
            owner_refund: close.owner_refund_destination,
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::ApplyPromotion,
    )
}

pub fn apply_ix(w: &World, close_id: u64) -> Instruction {
    apply_ix_with(w, close_id, apply_keys(w, close_id))
}

pub(crate) fn expire_accounts(w: &World, close_id: u64) -> accounts::ExpirePromotion {
    let rt = &w.rt;
    accounts::ExpirePromotion {
        pool: rt.pool_address(),
        close_record: rt.close_record_address(close_id),
        ops: rt.ops_address(),
        ops_cash: rt.ops_cash_address(),
        owner_refund: w.close_record(close_id).owner_refund_destination,
        system_program: system_program::ID,
        event_authority: rt.event_authority_address(),
        program: ID,
    }
}

pub fn expire_ix(w: &World, close_id: u64) -> Instruction {
    anchor_ix(expire_accounts(w, close_id), instruction::ExpirePromotion)
}

pub fn finalize_ix(w: &World, close_id: u64) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::FinalizeRound {
            pool: rt.pool_address(),
            close_record: rt.close_record_address(close_id),
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::FinalizeRound,
    )
}

pub(crate) fn budget_ix(w: &World, close_id: u64, extra: u64) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::IncreaseProviderBudget {
            pool: rt.pool_address(),
            close_record: rt.close_record_address(close_id),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
        },
        instruction::IncreaseProviderBudget { extra },
    )
}

pub(crate) fn observe_accounts(w: &World, close_id: u64) -> accounts::ObserveRequestFulfilled {
    let rt = &w.rt;
    accounts::ObserveRequestFulfilled {
        pool: rt.pool_address(),
        close_record: rt.close_record_address(close_id),
        ops: rt.ops_address(),
        request: w.request_account(close_id),
        client: provider_keys().client,
    }
}

pub(crate) fn observe_ix(w: &World, close_id: u64) -> Instruction {
    anchor_ix(observe_accounts(w, close_id), instruction::ObserveRequestFulfilled)
}

pub(crate) fn withdraw_accounts(w: &World, close_id: u64) -> accounts::WithdrawProviderReturn {
    let rt = &w.rt;
    let close = w.close_record(close_id);
    accounts::WithdrawProviderReturn {
        governance_multisig: rt.g_multisig(),
        governance: rt.g_vault(),
        pool: rt.pool_address(),
        close_record: rt.close_record_address(close_id),
        config: rt.config_address(close.config_version),
        ops: rt.ops_address(),
        ops_cash: rt.ops_cash_address(),
        provider_program: orao::id(),
        client: provider_keys().client,
        owner_refund: close.owner_refund_destination,
        system_program: system_program::ID,
    }
}

pub(crate) fn withdraw_ix(w: &World, close_id: u64, amount: u64) -> Instruction {
    anchor_ix(
        withdraw_accounts(w, close_id),
        instruction::WithdrawProviderReturn { amount },
    )
}

// ---------------------------------------------------------------------------
// Participants and the closing scene
// ---------------------------------------------------------------------------

/// What a participant holds when the round closes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Part {
    pub(crate) deposits: u128,
    pub(crate) net: u128,
    pub(crate) ever_a: bool,
    pub(crate) ever_b: bool,
}

/// A winner: `deposits <= net`, so never a candidate.
pub(crate) fn nonloser() -> Part {
    Part { deposits: 100_000_000, net: 150_000_000, ever_a: false, ever_b: false }
}

/// Break-even: `deposits == net`, so never a candidate.
pub(crate) fn breakeven() -> Part {
    Part { deposits: 100_000_000, net: 100_000_000, ever_a: false, ever_b: false }
}

/// A winner with Series B history: a non-candidate whatever its history.
pub(crate) fn nonloser_b() -> Part {
    Part { ever_b: true, ..nonloser() }
}

/// A loser with no Series A / B history: preferred and fallback candidate.
pub(crate) fn loser(loss: u128) -> Part {
    Part { deposits: 1_000_000_000, net: 1_000_000_000 - loss, ever_a: false, ever_b: false }
}

/// A loser with Series A history: fallback candidate only.
pub(crate) fn loser_a(loss: u128) -> Part {
    Part { ever_a: true, ..loser(loss) }
}

/// Slots 1..=19 of the standard round (slot 0 is the closer, a history-free
/// loser of 912,000,000): losers at slots 2, 6 and 11 (preferred) and 3, 8
/// and 14 (Series A history, fallback only).  With the closer that is 7
/// losers, 4 preferred and 7 fallback among 20 participants.
pub(crate) fn nineteen() -> Vec<Part> {
    // Index `slot - 1` is slot `slot`.
    let mut parts = vec![nonloser(); 19];
    let mut set = |slot: usize, part: Part| parts[slot - 1] = part;
    set(2, loser(150_000_000));
    set(3, loser_a(120_000_000));
    set(5, breakeven());
    set(6, loser(300_000_000));
    set(8, loser_a(80_000_000));
    set(9, nonloser_b());
    set(11, loser(450_000_000));
    set(13, breakeven());
    set(14, loser_a(60_000_000));
    parts
}

/// Slot `slot >= 1` of the standard roster.
pub(crate) fn roster_key(slot: usize) -> Pubkey {
    World::synthetic_participant(slot as u64)
}

/// LiteSVM's default rent (what `FinancialRuntime::rent` charges).
pub(crate) fn rent_of(bytes: usize) -> u64 {
    Rent::default().minimum_balance(bytes)
}

/// How to build a closing scene.
#[derive(Clone, Debug)]
pub(crate) struct Spec {
    /// Participants after the closer, in slot order.
    pub(crate) roster: Vec<Part>,
    /// Their owner keys (defaults to the synthetic roster keys).
    pub(crate) keys: Vec<Pubkey>,
    /// Open Series A Backers; any makes the toll ride a closing ticket.
    pub(crate) backers: Vec<Pubkey>,
    /// The closer's positions `(nonce, gross)`.  All but the last are settled
    /// by real solvent payouts first; the last is the closing payout.
    pub(crate) positions: Vec<(u64, u64)>,
    pub(crate) days: u64,
    pub(crate) pot: u64,
    /// Operations free cash at the closing payout (`None`: exactly the
    /// reserve; the owner opts in for anything short of it).
    pub(crate) sponsor: Option<u64>,
    /// Free cash on top of the reserve (not locked).
    pub(crate) spare: u64,
    pub(crate) domain: Option<[u8; 32]>,
    pub(crate) request_bytes: Option<u16>,
    pub(crate) genesis_pages: bool,
    /// The closing payout's destination (the World's `dest` when `None`).
    pub(crate) dest: Option<Pubkey>,
}

impl Spec {
    pub(crate) fn standard() -> Self {
        Self {
            roster: nineteen(),
            keys: Vec::new(),
            backers: Vec::new(),
            positions: vec![(1, SOL)],
            days: 1,
            pot: 100_000_000,
            sponsor: None,
            spare: 0,
            domain: None,
            request_bytes: None,
            genesis_pages: true,
            dest: None,
        }
    }

    pub(crate) fn with_sponsor(mut self, sponsor: u64) -> Self {
        self.sponsor = Some(sponsor);
        self
    }

    pub(crate) fn with_spare(mut self, spare: u64) -> Self {
        self.spare = spare;
        self
    }

    pub(crate) fn with_backers(mut self, backers: Vec<Pubkey>) -> Self {
        self.backers = backers;
        self.genesis_pages = false;
        self
    }

    pub(crate) fn key(&self, index: usize) -> Pubkey {
        self.keys.get(index).copied().unwrap_or_else(|| roster_key(index + 1))
    }

    /// 21 days at 11% a day against a 1.5 SOL pot: the closer is paid more
    /// than it deposited, so it is a winner and nobody has to be a candidate.
    pub(crate) fn winners_only() -> Self {
        Self {
            roster: vec![nonloser(); 19],
            days: 21,
            pot: 1_500_000_000,
            ..Self::standard()
        }
    }
}

/// A round that has been closed by a real `withdraw_simple`.
pub(crate) struct Scene {
    pub(crate) w: World,
    pub(crate) owner: Keypair,
    pub(crate) dest: Pubkey,
    /// The reserve the payout locked.
    pub(crate) reserve: u64,
    /// The payout event of the closing payout.
    pub(crate) payout: PayoutPaid,
}

impl Scene {
    /// Build the world of `spec` up to and including the closing payout.  The
    /// World's scope is `financial.close.<row>[.<tag>]`.
    pub(crate) fn new(row: &str, tag: &str, spec: Spec) -> Self {
        let mut w = World::genesis_with_provider().scoped_label(headline(tag, row));
        w.provider_setup();
        assert_eq!(rent_of(0), w.rt.rent(0), "LiteSVM's rent schedule is Rent::default()");
        if let Some(bytes) = spec.request_bytes {
            let mut config = w.config(1);
            config.pending_request_bytes = bytes;
            w.rt.replace_config(config);
        }
        if let Some(domain) = spec.domain {
            w.set_pool(|pool| pool.cluster_domain = domain);
        }
        if spec.genesis_pages {
            assert!(spec.backers.is_empty());
            w.rt.strip_pages_to_genesis();
        }
        if !spec.backers.is_empty() {
            let backers: Vec<BackerSpec> = spec
                .backers
                .iter()
                .map(|owner| BackerSpec::new(*owner, SERIES_A, 62_000_000))
                .collect();
            w.install_backer_specs(&backers);
            // A real Series A Backer always has a `User` (`fund_series_a` is
            // `init_if_needed` on it) and `claim_backer` requires it; the
            // fixture Backer must not be an impossible state (first run:
            // Anchor 3012 AccountNotInitialized on `user`).
            for owner in &spec.backers {
                w.install_user(*owner, true, false);
            }
        }

        let owner = w.owner();
        let owner_key = owner.pubkey();
        let dest = spec.dest.unwrap_or(w.dest);
        w.watch(dest);
        for &(nonce, gross) in &spec.positions {
            w.open_fixture_position(owner_key, nonce, PlanKindArg::Simple, gross);
        }
        for (index, part) in spec.roster.iter().enumerate() {
            let key = spec.key(index);
            w.install_participant(key, part.deposits, part.net, part.ever_a, part.ever_b);
            w.watch(w.rt.backer_address(1, key));
        }
        w.watch(w.rt.backer_address(1, owner_key));
        for close_id in 1..=2 {
            for tag in 0..=1 {
                for index in 0..3 {
                    w.watch(candidate_page_key(close_id, tag, index));
                }
            }
        }

        let prior = &spec.positions[..spec.positions.len() - 1];
        w.set_pot_fixture(if prior.is_empty() { spec.pot } else { 10 * SOL });
        w.provision_close_record(1);
        w.warp_seconds(spec.days * DAY);
        for &(nonce, _) in prior {
            w.withdraw_simple(&format!("settle_position{nonce}"), &owner, nonce, 0, dest, 0)
                .unwrap_or_else(|failed| panic!("prior solvent payout failed: {failed:?}"));
        }
        if !prior.is_empty() {
            w.set_pot_fixture(spec.pot);
        }

        let participants = 1 + spec.roster.len() as u64;
        let reserve = w.expected_reserve(participants, spec.backers.len() as u64, 0, 0);
        let free = spec.sponsor.unwrap_or(reserve) + spec.spare;
        w.set_ops_free(free);
        let contribution = reserve.saturating_sub(free);
        let closing_nonce = spec.positions.last().expect("a closing position").0;
        let paid = w
            .withdraw_simple("closing_payout", &owner, closing_nonce, 0, dest, contribution)
            .unwrap_or_else(|failed| panic!("the closing payout failed: {failed:?}"));
        let payout = paid.last_event::<PayoutPaid>();
        assert!(payout.closing && payout.close_id == Some(1), "the payout closed the round");
        let bound = w.close_record(1);
        assert_eq!(bound.close_started_at, w.now(), "the close starts at the payout's Clock second");
        assert_eq!(
            bound.promotion_deadline,
            bound.close_started_at + 600,
            "the pinned deadline is exactly close_started_at + 600 s"
        );
        assert_eq!(bound.owner_refund_destination, dest, "the payout destination is the refund destination");
        assert_identities(&w);
        // The deliverable path: the immutable destination was funded by the
        // payout itself, so every later owner credit lands on a wallet already
        // above the rent-exempt minimum.
        assert!(
            w.lamports(dest) >= w.rt.rent(0),
            "closing destination holds at least the rent-exempt minimum"
        );
        w.watch(w.rt.close_record_address(1));
        Self { w, owner, dest, reserve, payout }
    }

    /// Commit the closing ticket (batches of 8 rows), when the toll rode one.
    pub(crate) fn commit_cash(&mut self) {
        assert!(self.w.close_record(1).closing_ticket_id.is_some(), "no closing ticket");
        self.w.advance_until_idle(8);
        assert_identities(&self.w);
        assert!(self.w.close_record(1).cash_committed);
    }

    /// Provision the host-derived candidate pages and seal the sets (4 rows a
    /// batch, final batch labelled `last`).
    pub(crate) fn provision_and_seal(&mut self, last: &str) {
        provision_candidate_pages(&mut self.w, 1);
        seal_labelled(&mut self.w, 1, 4, last);
    }

    /// `provision_and_seal` + request + fulfil + capture (all inside the
    /// deadline).
    pub(crate) fn through_capture(&mut self) {
        self.provision_and_seal("build_last");
        do_request(&mut self.w, 1, "request");
        self.w.fulfill("fulfill", 1).expect("fulfill");
        let capture = capture_ix(&self.w, 1);
        ok(&mut self.w, "capture", vec![capture], &[]);
    }
}

/// [`seal`] with an explicit label for the final (sealing) batch.
pub fn seal_labelled(w: &mut World, close_id: u64, batch: u8, last: &str) -> Vec<Observation> {
    let mut observations = Vec::new();
    for call in 0.. {
        let close = w.close_record(close_id);
        if close.sealed {
            break;
        }
        let remaining = close.participant_count - close.candidate_cursor;
        let rows = remaining.min(u64::from(batch)) as u8;
        let instruction = build_next(w, close_id, rows);
        let variant = if u64::from(rows) == remaining {
            lbl(close_id, last)
        } else {
            lbl(close_id, &format!("build{call}"))
        };
        observations.push(ok(w, &variant, vec![instruction], &[]));
        assert!(call < 1_000, "sealing did not converge");
    }
    observations
}

/// `request_round_draw` for `close_id`, watching the request PDA for the
/// fee-only snapshots.
pub fn do_request(w: &mut World, close_id: u64, variant: &str) -> Observation {
    let request = host_request(w, close_id).0;
    w.watch(request);
    let instruction = request_ix(w, close_id);
    ok(w, variant, vec![instruction], &[])
}

/// The sponsor cash that leaves the owner partition funding exactly
/// `owner_rent` of the pending-request rent (and none of the fee): two
/// candidate pages, the fee, and the rent less `owner_rent`.
pub(crate) fn sponsor_for_owner_rent(owner_rent: u64) -> u64 {
    2 * rent_of(state::CandidatePage::SPACE) + REQUEST_FEE + rent_of(759) - owner_rent
}

/// A standard round whose owner funded exactly `owner_rent` of the rent,
/// sealed, requested and fulfilled (the reimbursement not yet observed).
pub(crate) fn fulfilled_scene(row: &str, tag: &str, owner_rent: u64) -> Scene {
    let mut scene = Scene::new(
        row,
        tag,
        Spec::standard().with_sponsor(sponsor_for_owner_rent(owner_rent)),
    );
    scene.provision_and_seal("build_last");
    do_request(&mut scene.w, 1, "request");
    scene.w.fulfill("fulfill", 1).expect("fulfill");
    assert_identities(&scene.w);
    scene
}

/// [`fulfilled_scene`] plus `observe_request_fulfilled`.
pub(crate) fn observed_scene(row: &str, tag: &str, owner_rent: u64) -> Scene {
    let mut scene = fulfilled_scene(row, tag, owner_rent);
    let observe = observe_ix(&scene.w, 1);
    ok(&mut scene.w, "observe", vec![observe], &[]);
    scene
}

/// The pending-request rent and the rent the fulfilled account retains, at
/// LiteSVM's default schedule.
pub(crate) fn request_rents() -> (u64, u64) {
    (rent_of(759), rent_of(147))
}

// ---------------------------------------------------------------------------
// Row: the provider fixture itself
// ---------------------------------------------------------------------------

/// SHA-256 over the `UserRound` and `User` bytes of every participant, in slot
/// order: the sealed inputs `build_candidates` reads.
fn participants_digest(w: &World, close_id: u64) -> [u8; 32] {
    let close = w.close_record(close_id);
    let mut hasher = Sha256::new();
    for slot in 0..close.participant_count {
        let owner = participant(w, close.round_id, slot);
        hasher.update(w.data(w.rt.user_round_address(close.round_id, owner)));
        hasher.update(w.data(w.rt.user_address(owner)));
    }
    hasher.finalize().into()
}

pub fn provider_setup_row() -> Vec<Observation> {
    let row = ROW_PROVIDER;
    let mut w = World::genesis_with_provider().scoped_label(headline("", row));
    let mut log = w.provider_setup();
    let keys = provider_keys();
    let g_vault = w.rt.g_vault();

    let elf = w
        .rt
        .provider_artifact_sha256()
        .expect("the ORAO ELF is loaded")
        .to_string();
    // Default profile: the pinned mainnet ELF (unchanged).  `MC_ORAO_PROFILE=devnet`
    // runs the same row against the devnet ELF, held to its own pin.
    assert_eq!(
        elf,
        crate::devnet::active_orao_elf_sha256(),
        "the ELF pinned by the active MC_ORAO_PROFILE"
    );

    let network: orao::state::network_state::NetworkState = w.rt.deserialize(keys.network_state);
    assert_eq!(network.config.request_fee, REQUEST_FEE);
    assert_eq!(network.config.treasury, keys.treasury);
    assert_eq!(network.config.callback_deadline, CALLBACK_DEADLINE_SLOTS);
    assert_eq!(network.config.fulfill_authorities, vec![fulfill_authority().pubkey()]);
    assert_eq!(w.account(keys.network_state).expect("network state").owner, orao::id());

    // The Client is G-owned: the `register` payer was the G vault.
    let client = w.orao_client();
    assert_eq!(client.owner, g_vault, "Client.owner is the G vault");
    assert_eq!(client.owner, w.pool().governance);
    assert_eq!(client.program, ID);
    assert_eq!(client.state, keys.rng_authority);
    assert!(client.callback.is_none(), "no callback is registered");
    assert_eq!(client.num_requests, 0);
    let client_account = w.account(keys.client).expect("Client account");
    assert_eq!(client_account.owner, orao::id());
    assert_eq!(
        client_account.lamports,
        w.rt.rent(client_account.data.len()),
        "the Client holds exactly its rent before any top-up"
    );
    assert_eq!(
        w.rt.deserialize::<state::RngAuthority>(keys.rng_authority).client,
        keys.client
    );
    let config = w.config(1);
    assert_eq!(
        (
            config.provider_program,
            config.provider_network_state,
            config.provider_treasury,
            config.provider_client,
            config.provider_request_fee_lamports,
        ),
        (orao::id(), keys.network_state, keys.treasury, keys.client, REQUEST_FEE)
    );

    // A real instruction of the suite carries the suite-level record: observe
    // the freshly registered Client.
    let reconcile = reconcile_client_ix(&w.rt);
    let observed = ok(&mut w, "", vec![reconcile], &[]);
    let ops = w.ops();
    assert_eq!(ops.client_balance_observed, client_account.lamports);
    assert_eq!(ops.client_protected_rent, w.rt.rent(client_account.data.len()));
    assert_eq!(ops.provider_deficit, 0);
    assert_eq!(
        (ops.free_balance, ops.locked_remaining_total, ops.unallocated_excess),
        (0, 0, 0),
        "observing the Client mints no accounted cash"
    );
    assert!(observed.fits_legacy_packet);

    log.extend(w.take_log());
    let note = serde_json::json!({
        "orao_program": orao::id().to_string(),
        "orao_elf_sha256": elf,
        "pinned_by": match crate::devnet::orao_profile() {
            crate::devnet::OraoProfile::Mainnet => "solana/probes/runtime/fixtures/orao-provenance.json program_binary.elf_sha256",
            crate::devnet::OraoProfile::Devnet => "solana/fixtures/devnet/orao-devnet-provenance.json elf.sha256 (MC_ORAO_PROFILE=devnet)",
        },
        "env_override": "MC_ORAO_SBF (still checked against the pinned hash)",
        "synthetic_fulfill_authority": fulfill_authority().pubkey().to_string(),
        "request_fee_lamports": REQUEST_FEE,
        "client": keys.client.to_string(),
        "client_owner_is_g_vault": g_vault.to_string(),
        "limits": "ONE synthetic fulfill authority; no cluster latency; the Squads program is the test-only stand-in",
    });
    annotate(&mut log, &headline("", row), format!("provider_elf: {note}"));
    log
}

// ---------------------------------------------------------------------------
// Rows: candidate building
// ---------------------------------------------------------------------------

pub fn build_candidates_visits_every_participant_once_and_seals_digests() -> Vec<Observation> {
    let row = ROW_BUILD_SEALS;
    let mut scene = Scene::new(row, "", Spec::standard());
    let owner_key = scene.owner.pubkey();
    let w = &mut scene.w;

    // The host derives the two lists from the participants' own accounts.
    let expected = expected_candidates(w, 1);
    assert_eq!((expected.preferred.len(), expected.fallback.len()), (4, 7));
    assert_eq!(expected.preferred[0], (owner_key, 912_000_000), "the closer lost 912,000,000");
    assert_eq!(
        expected.preferred.iter().map(|(_, loss)| *loss).collect::<Vec<_>>(),
        vec![912_000_000, 150_000_000, 300_000_000, 450_000_000]
    );
    assert_eq!(
        expected.fallback.iter().map(|(_, loss)| *loss).collect::<Vec<_>>(),
        vec![912_000_000, 150_000_000, 120_000_000, 300_000_000, 80_000_000, 450_000_000, 60_000_000]
    );
    let history_losers = expected
        .fallback
        .iter()
        .filter(|(owner, _)| !expected.preferred.iter().any(|(candidate, _)| candidate == owner))
        .count();
    assert_eq!(history_losers, 3, "3 losers have Series A history: fallback only");

    // Candidate pages come from the locked reserve, sponsor partition first.
    let before = w.close_record(1);
    let page_rent = w.rt.rent(state::CandidatePage::SPACE);
    assert_eq!(provision_candidate_pages(w, 1), (1, 1));
    let provisioned = w.close_record(1);
    assert_eq!(provisioned.spent.candidate_rent_s, 2 * page_rent);
    assert_eq!(provisioned.spent.candidate_rent_o, 0);
    assert_eq!(
        provisioned.locked_sponsor_remaining,
        before.locked_sponsor_remaining - 2 * page_rent
    );
    assert_eq!((provisioned.provisioned_preferred_pages, provisioned.provisioned_fallback_pages), (1, 1));
    assert_eq!(w.ops().free_balance, 0, "provisioning never touched free cash");

    let participants_before = participants_digest(w, 1);
    let pool_before = w.data(w.rt.pool_address());
    let ops_before = w.data(w.rt.ops_address());
    let vault_before = w.lamports(w.rt.game_vault_address());
    let cash_before = w.lamports(w.rt.ops_cash_address());

    // 20 participants, 4 rows a batch: every slot exactly once, in order, the
    // running digests equal to the chain over the host-derived prefix.
    let mut visited = 0u64;
    for call in 0..5u64 {
        assert_eq!(w.close_record(1).candidate_cursor, visited);
        let last = call == 4;
        let instruction = build_next(w, 1, 4);
        let variant = if last { String::new() } else { format!("build{call}") };
        let observation = ok(w, &variant, vec![instruction], &[]);
        assert!(observation.fits_legacy_packet, "a 4-row batch fits the legacy packet");
        visited += 4;
        let close = w.close_record(1);
        assert_eq!(close.candidate_cursor, visited, "the cursor advanced by exactly the rows");
        let prefix = expected_candidates_upto(w, 1, visited);
        assert_eq!(close.preferred_count, prefix.preferred.len() as u64);
        assert_eq!(close.fallback_count, prefix.fallback.len() as u64);
        if !last {
            assert!(!close.sealed);
            for (tag, count, digest, rows) in [
                (0u8, close.preferred_count, close.preferred_digest, &prefix.preferred),
                (1u8, close.fallback_count, close.fallback_digest, &prefix.fallback),
            ] {
                if count == 0 {
                    assert_eq!(digest, [0u8; 32], "an untouched set has no digest yet");
                } else {
                    assert_eq!(digest, chain(tag, rows), "running digest of set {tag} at slot {visited}");
                }
            }
            assert!(observation.events::<CandidatesSealed>().is_empty());
        } else {
            let sealed = observation.last_event::<CandidatesSealed>();
            assert_eq!(
                sealed,
                CandidatesSealed {
                    schema_version: 1,
                    round_id: 1,
                    close_id: 1,
                    tag: 0,
                    count: 4,
                    digest: chain(0, &expected.preferred),
                }
            );
        }
    }

    let sealed = w.close_record(1);
    assert!(sealed.sealed);
    assert_eq!(sealed.candidate_cursor, sealed.participant_count);
    assert_eq!((sealed.preferred_count, sealed.fallback_count), (4, 7));
    assert_eq!(sealed.preferred_digest, chain(0, &expected.preferred), "SHA-256 chain over the ordered pubkeys and losses");
    assert_eq!(sealed.fallback_digest, chain(1, &expected.fallback));
    assert_eq!(sealed.selected_set_tag, 0, "a non-empty preferred set is drawn from");
    assert_eq!(sealed.promotion, state::Promotion::Pending);
    assert_eq!(sealed.draw_state, state::DRAW_NOT_REQUESTED);
    let page = candidate_page(w, 1, 0, 0);
    assert_eq!(page.len, 4);
    assert_eq!(
        page.rows[..4].to_vec(),
        expected.preferred.iter().map(|(owner, _)| *owner).collect::<Vec<_>>()
    );
    let page = candidate_page(w, 1, 1, 0);
    assert_eq!(page.len, 7);
    assert_eq!(
        page.rows[..7].to_vec(),
        expected.fallback.iter().map(|(owner, _)| *owner).collect::<Vec<_>>()
    );
    // Building creates no account and moves no cash; it reads the sealed inputs.
    assert_eq!(participants_digest(w, 1), participants_before, "UserRound / User bytes untouched");
    assert_eq!(w.data(w.rt.pool_address()), pool_before);
    assert_eq!(w.data(w.rt.ops_address()), ops_before);
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault_before);
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash_before);

    // A sealed set is final: another batch is rejected, fee only.
    let instruction = build_ix(w, 1, 1, build_remaining(w, 1, 0, 1));
    reject(w, "build_after_seal", vec![instruction], &[], ErrorCode::InvalidState);
    w.take_log()
}

pub fn build_candidates_rejects_skipped_or_reordered_rows() -> Vec<Observation> {
    let row = ROW_BUILD_REJECTS;
    let mut scene = Scene::new(row, "", Spec::standard());
    let w = &mut scene.w;
    provision_candidate_pages(w, 1);
    let close = w.close_record(1);
    let row_of = |w: &World, slot: u64| row_accounts(w, &close, slot);

    // Reordered: slot 1's accounts first, then slot 0's.
    let remaining = [row_of(w, 1), row_of(w, 0)].concat();
    let ix = build_ix(w, 1, 2, remaining);
    reject(w, "", vec![ix], &[], ErrorCode::WrongAccount);

    // Skipped: the cursor is at slot 0 but slot 1's accounts are supplied.
    let ix = build_ix(w, 1, 1, row_of(w, 1));
    reject(w, "skipped", vec![ix], &[], ErrorCode::WrongAccount);

    // Duplicated: slot 0 twice.
    let remaining = [row_of(w, 0), row_of(w, 0)].concat();
    let ix = build_ix(w, 1, 2, remaining);
    reject(w, "duplicated", vec![ix], &[], ErrorCode::WrongAccount);

    // Slot 0's UserRound with another owner's User.
    let mut remaining = row_of(w, 0);
    remaining[2] = row_of(w, 1)[2].clone();
    let ix = build_ix(w, 1, 1, remaining);
    reject(w, "wrong_user", vec![ix], &[], ErrorCode::WrongAccount);

    // Another owner's UserRound under slot 0.
    let mut remaining = row_of(w, 0);
    remaining[1] = row_of(w, 1)[1].clone();
    let ix = build_ix(w, 1, 1, remaining);
    reject(w, "wrong_user_round", vec![ix], &[], ErrorCode::WrongAccount);

    // Participant page 1 where page 0 holds the cursor's slot.
    let mut remaining = row_of(w, 0);
    remaining[0] = row_of(w, 8)[0].clone();
    let ix = build_ix(w, 1, 1, remaining);
    reject(w, "wrong_participant_page", vec![ix], &[], ErrorCode::WrongAccount);

    // The honest pages omitted (program-id placeholders): WrongAccount, not a panic.
    let mut ix = build_ix(w, 1, 1, row_of(w, 0));
    ix.accounts[2] = AccountMeta::new_readonly(ID, false);
    ix.accounts[3] = AccountMeta::new_readonly(ID, false);
    reject(w, "candidate_pages_omitted", vec![ix], &[], ErrorCode::WrongAccount);

    // The fallback page presented as the preferred page.
    let mut ix = build_ix(w, 1, 1, row_of(w, 0));
    ix.accounts[2] = AccountMeta::new(candidate_page_key(1, 1, 0), false);
    reject(w, "wrong_set_page", vec![ix], &[], ErrorCode::WrongAccount);

    // A malformed batch: row count and account count disagree; zero and
    // nine rows are out of range.
    let ix = build_ix(w, 1, 2, row_of(w, 0));
    reject(w, "account_count_mismatch", vec![ix], &[], ErrorCode::InvalidState);
    let ix = build_ix(w, 1, 0, Vec::new());
    reject(w, "zero_rows", vec![ix], &[], ErrorCode::InvalidState);
    let ix = build_ix(w, 1, 9, build_remaining(w, 1, 0, 9));
    reject(w, "nine_rows", vec![ix], &[], ErrorCode::InvalidState);

    // After every rejection the close is exactly where it started.
    assert_eq!(w.close_record(1).candidate_cursor, 0);
    assert!(!w.close_record(1).sealed);

    // Build 16 rows honestly, then ask for a batch that runs past the list.
    for call in 0..4 {
        let ix = build_next(w, 1, 4);
        ok(w, &format!("honest{call}"), vec![ix], &[]);
    }
    assert_eq!(w.close_record(1).candidate_cursor, 16);
    let remaining = build_remaining(w, 1, 12, 8);
    let ix = build_ix(w, 1, 8, remaining);
    reject(w, "past_the_participant_list", vec![ix], &[], ErrorCode::CursorMismatch);

    // The honest finish still works: the rejections left nothing behind.
    let last = seal_labelled(w, 1, 4, "honest_last");
    assert_eq!(last.len(), 1);
    assert!(w.close_record(1).sealed);
    assert_eq!(w.close_record(1).preferred_count, 4);

    // Pages not provisioned at all: a separate world.
    let mut bare = Scene::new(row, "unprovisioned_page", Spec::standard());
    let ix = build_next(&bare.w, 1, 4);
    reject(&mut bare.w, "page_not_provisioned", vec![ix], &[], ErrorCode::PageNotProvisioned);
    assert_eq!(bare.w.close_record(1).candidate_cursor, 0);

    let mut log = scene.w.take_log();
    log.extend(bare.w.take_log());
    log
}

pub fn same_owner_multi_position_aggregation_and_sealed_userround_immutability() -> Vec<Observation> {
    let row = ROW_AGGREGATION;
    // One owner, three positions: nonces 1 and 2 are settled by real solvent
    // payouts, nonce 3 is the insolvent closing payout.
    let spec = Spec {
        roster: vec![loser(200_000_000), nonloser(), loser_a(100_000_000)],
        positions: vec![(1, 200_000_000), (2, 300_000_000), (3, SOL)],
        ..Spec::standard()
    };
    let mut scene = Scene::new(row, "", spec);
    let owner = scene.owner.insecure_clone();
    let owner_key = owner.pubkey();
    let w = &mut scene.w;

    // One participant row, one aggregate UserRound, one loss figure.
    let round = w.user_round(owner_key, 1);
    let nets: Vec<u128> = (1..=3).map(|nonce| w.position(owner_key, nonce).net_payout).collect();
    assert!(nets.iter().all(|net| *net > 0), "all three positions paid out");
    assert_eq!(round.deposits, 200_000_000 + 300_000_000 + 1_000_000_000);
    assert_eq!(round.net_payout, nets.iter().sum::<u128>(), "UserRound aggregates the three payouts");
    assert_eq!(w.close_record(1).participant_count, 4, "the owner is one participant, once");
    assert_eq!(
        (0..4).filter(|slot| participant(w, 1, *slot) == owner_key).count(),
        1,
        "the owner's three positions share one participant slot"
    );
    let loss = round.deposits - round.net_payout;
    assert_eq!(loss, 1_500_000_000 - nets.iter().sum::<u128>());

    provision_candidate_pages(w, 1);
    let expected = expected_candidates(w, 1);
    assert_eq!(
        expected.preferred.iter().filter(|(candidate, _)| *candidate == owner_key).count(),
        1,
        "the owner is one preferred candidate"
    );
    assert_eq!(expected.preferred[0], (owner_key, loss), "aggregated loss, not a per-position figure");
    assert_eq!(expected.fallback[0], (owner_key, loss));
    seal_labelled(w, 1, 4, "build_last");
    let sealed = w.close_record(1);
    assert_eq!(sealed.preferred_digest, chain(0, &expected.preferred));
    assert_eq!(sealed.fallback_digest, chain(1, &expected.fallback));
    assert_eq!(sealed.preferred_count, 2, "the owner and the other history-free loser");

    // Sealed inputs are immutable: no deposit, no Series A funding, no new
    // participant can change them while the round is Closing.
    let sealed_digest = participants_digest(w, 1);
    let user_bytes: Vec<Vec<u8>> = (0..4)
        .map(|slot| w.data(w.rt.user_address(participant(w, 1, slot))))
        .collect();
    let open = w.open_instruction(owner_key, PlanKindArg::Simple, 100_000_000, 4, 1, 1);
    reject(w, "deposit_while_closing", vec![open], &[&owner], ErrorCode::Closing);
    let series_a = w.fund_series_a_instruction(owner_key, 50_000_000, 1);
    reject(w, "series_a_while_closing", vec![series_a], &[&owner], ErrorCode::Closing);
    assert_eq!(participants_digest(w, 1), sealed_digest, "UserRound / User bytes unchanged by the refused deposits");
    assert_eq!(w.close_record(1).participant_count, 4);

    // The whole draw, then the promotion: only the winner's `User` changes.
    let request = request_ix(w, 1);
    ok(w, "request", vec![request], &[]);
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let user_rounds_before: Vec<Vec<u8>> = (0..4)
        .map(|slot| w.data(w.rt.user_round_address(1, participant(w, 1, slot))))
        .collect();
    let apply = apply_ix(w, 1);
    let applied = ok(w, "", vec![apply], &[]);
    let event = applied.last_event::<PromotionApplied>();
    for slot in 0..4u64 {
        let who = participant(w, 1, slot);
        assert_eq!(
            w.data(w.rt.user_round_address(1, who)),
            user_rounds_before[slot as usize],
            "no UserRound changes at apply_promotion"
        );
        if who == event.winner {
            let user = w.user(who);
            assert!(user.ever_b);
            assert_eq!(user.b_entitlement, event.entitlement);
            assert_eq!(user.ever_a, slot == 3, "Series A history is untouched");
        } else {
            assert_eq!(w.data(w.rt.user_address(who)), user_bytes[slot as usize], "a non-winner's User is unchanged");
        }
    }
    w.take_log()
}

pub fn sealed_with_zero_losers_skips_no_candidates_without_request() -> Vec<Observation> {
    let row = ROW_ZERO_LOSERS;
    let mut scene = Scene::new(row, "", Spec::winners_only());
    let w = &mut scene.w;
    let owner_key = scene.owner.pubkey();
    let round = w.user_round(owner_key, 1);
    assert!(round.net_payout >= round.deposits, "the closer was paid more than it deposited");
    assert_eq!(scene.payout.toll, scene.payout.gross_paid * 3 / 100, "21 days: the 3% tier");
    let expected = expected_candidates(w, 1);
    assert!(expected.preferred.is_empty() && expected.fallback.is_empty());

    let reserve = w.close_record(1).required_reserve;
    let seed_before = w.pool().seed;
    let client_before = w.client_lamports();
    let treasury_before = w.treasury_lamports();
    let dest_before = w.lamports(scene.dest);
    let ops_before = w.ops();
    assert_eq!(ops_before.locked_remaining_total, reserve);

    // No candidate page is needed; the sealing batch settles the reserve.
    let calls = seal_labelled(w, 1, 4, "");
    let sealing = calls.last().expect("a sealing batch");
    assert_eq!(
        sealing.last_event::<CandidatesSealed>(),
        CandidatesSealed {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            tag: 1,
            count: 0,
            digest: sampling::candidate_digest_start(1),
        },
        "an empty set carries its well-defined empty digest; the fallback tag is selected"
    );
    assert_eq!(
        sealing.last_event::<PromotionSkipped>(),
        PromotionSkipped {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            reason: pclose::REASON_NO_CANDIDATES,
            stage: pclose::STAGE_SEALED,
        }
    );
    let close = w.close_record(1);
    assert!(close.sealed);
    assert_eq!(
        close.promotion,
        state::Promotion::Skipped { reason: pclose::REASON_NO_CANDIDATES, stage: pclose::STAGE_SEALED }
    );
    assert_eq!((close.preferred_count, close.fallback_count), (0, 0));
    assert_eq!(close.preferred_digest, sampling::candidate_digest_start(0));
    assert_eq!(close.fallback_digest, sampling::candidate_digest_start(1));
    assert!(close.reserve_settled);
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 0));
    assert_eq!(close.spent, state::SpentLedger::default(), "nothing was spent");
    let ops = w.ops();
    assert_eq!(ops.locked_remaining_total, 0);
    assert_eq!(ops.free_balance, reserve, "the whole sponsor partition returned to free");
    assert_eq!(ops.unallocated_excess, ops_before.unallocated_excess, "settlement parks nothing");
    assert_eq!(ops.provider_fee_consumed_total, 0);
    assert_eq!(w.lamports(scene.dest), dest_before, "no owner partition existed");

    // No request exists and none can be made.
    assert_eq!(close.draw_state, state::DRAW_NOT_REQUESTED);
    assert_eq!(close.request_account, Pubkey::default());
    assert_eq!((w.client_lamports(), w.treasury_lamports()), (client_before, treasury_before));
    let request = request_ix(w, 1);
    reject(w, "request_after_skip", vec![request], &[], ErrorCode::PromotionTerminal);
    let capture = capture_ix(w, 1);
    reject(w, "capture_without_request", vec![capture], &[], ErrorCode::InvalidState);
    let apply = apply_ix_with(w, 1, ApplyKeys::default());
    reject(w, "apply_after_skip", vec![apply], &[], ErrorCode::PromotionTerminal);
    let expire = expire_ix(w, 1);
    reject(w, "expire_after_skip", vec![expire], &[], ErrorCode::PromotionTerminal);

    // The skip is terminal and cash was committed at the closing payout, so
    // the round finalizes at once, carrying the full seed.
    let finalize = finalize_ix(w, 1);
    let finalized = ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(
        finalized.last_event::<RoundFinalized>(),
        RoundFinalized { schema_version: 1, close_id: 1, new_round: 2, seed_carried: seed_before }
    );
    assert_eq!(w.pool().round_id, 2);
    w.take_log()
}

pub fn request_round_draw_creates_request_with_protocol_seed_and_marker_atomically() -> Vec<Observation> {
    let row = ROW_REQUEST_CREATES;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let keys = provider_keys();
    let config = w.config(1);
    let rent = w.rt.rent(usize::from(config.pending_request_bytes));
    assert_eq!(rent, rent_of(759));
    let (request, seed) = host_request(w, 1);

    let before = w.close_record(1);
    assert_eq!(before.draw_state, state::DRAW_NOT_REQUESTED);
    assert_eq!((before.protocol_seed, before.request_account), ([0u8; 32], Pubkey::default()));
    assert!(w.account(request).is_none(), "no request account yet");
    let (client0, treasury0, cash0, vault0) = (
        w.client_lamports(),
        w.treasury_lamports(),
        w.lamports(w.rt.ops_cash_address()),
        w.lamports(w.rt.game_vault_address()),
    );
    let ops0 = w.ops();
    assert_eq!(ops0.free_balance, 0, "nothing but the locked reserve funds the request");

    // The seed is the spec section 4 formula over the live pool domain, the
    // close and the sealed digest, computed here independently of the program.
    let pool = w.pool();
    let mut preimage = Vec::new();
    preimage.extend_from_slice(b"SchemeWorks/MC/EEC/v1");
    preimage.extend_from_slice(&pool.cluster_domain);
    preimage.extend_from_slice(ID.as_ref());
    preimage.extend_from_slice(&1u64.to_le_bytes());
    preimage.extend_from_slice(&1u64.to_le_bytes());
    preimage.push(before.selected_set_tag);
    preimage.extend_from_slice(&before.preferred_count.to_le_bytes());
    preimage.extend_from_slice(&before.preferred_digest);
    assert_eq!(Into::<[u8; 32]>::into(Sha256::digest(&preimage)), seed, "spec section 4 byte layout");

    let instruction = request_ix(w, 1);
    let observation = ok(w, "", vec![instruction], &[]);
    assert!(observation.fits_legacy_packet);
    let after = w.close_record(1);
    assert_eq!(after.protocol_seed, seed);
    assert_eq!(after.request_account, request);
    assert_eq!(after.draw_state, state::DRAW_REQUESTED, "marker written with the CPI");
    assert_eq!(after.promotion, state::Promotion::Pending);
    assert_eq!(request, provider::request_binding(&keys.client, &seed).0);
    assert_eq!(
        observation.last_event::<DrawRequested>(),
        DrawRequested {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            request_account: request,
            fee_paid: REQUEST_FEE,
            request_rent: rent,
        }
    );

    // The provider really created the request: ORAO-owned, pending, bound to
    // the Client and the protocol seed.
    let account = w.account(request).expect("the ORAO request account exists");
    assert_eq!(account.owner, orao::id());
    assert_eq!(account.data.len(), usize::from(config.pending_request_bytes));
    assert_eq!(account.lamports, rent);
    let decoded = w.request_state(1);
    assert_eq!((decoded.client, decoded.seed), (keys.client, seed));
    assert!(matches!(decoded.state, orao::state::request::RequestState::Pending(_)));

    // Money: OpsCash paid fee + rent, the Client is back where it started
    // (topped up, then charged by ORAO), the Treasury got the fee, the request
    // the rent.
    assert_eq!(delta(&observation, w.rt.ops_cash_address()), -((REQUEST_FEE + rent) as i128));
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash0 - REQUEST_FEE - rent);
    assert_eq!(w.client_lamports(), client0);
    assert_eq!(w.treasury_lamports(), treasury0 + REQUEST_FEE);
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault0, "no game cash moved");
    let ops = w.ops();
    assert_eq!(ops.provider_fee_consumed_total, ops0.provider_fee_consumed_total + u128::from(REQUEST_FEE));
    assert_eq!(ops.request_rent_outstanding, ops0.request_rent_outstanding + rent);
    assert_eq!(ops.locked_remaining_total, ops0.locked_remaining_total - REQUEST_FEE - rent);
    assert_eq!(ops.free_balance, 0);

    // A second request for the same close is a replay: refused, fee only.
    let again = request_ix(w, 1);
    reject(w, "replay", vec![again], &[], ErrorCode::InvalidState);
    w.take_log()
}

fn seed_vectors_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/seed-vectors.json")
}

/// `solana/fixtures/seed-vectors.json`, parsed.
pub fn load_seed_vectors() -> serde_json::Value {
    let path = seed_vectors_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is required: {error}", path.display()));
    serde_json::from_str(&text).expect("seed-vectors.json is JSON")
}

fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "even hex length");
    (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[2 * index..2 * index + 2], 16).expect("hex"))
        .collect()
}

fn array32(text: &str) -> [u8; 32] {
    unhex(text).try_into().expect("32 bytes")
}

/// A loser whose loss is exactly `loss` (deposits above the 1,000 it kept).
fn exact_loser(loss: u128) -> Part {
    Part { deposits: loss + 1_000, net: 1_000, ever_a: false, ever_b: false }
}

pub fn protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector(
) -> Vec<Observation> {
    let row = ROW_SEED_VECTORS;
    let vectors = load_seed_vectors();
    assert_eq!(vectors["program_id"].as_str(), Some(ID.to_string().as_str()));
    let candidates: Vec<(Pubkey, u128)> = vectors["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .map(|candidate| {
            (
                Pubkey::new_from_array(array32(candidate["owner_hex"].as_str().expect("owner_hex"))),
                candidate["loss"].as_str().expect("loss").parse::<u128>().expect("u128 loss"),
            )
        })
        .collect();
    let digest = array32(vectors["sealed_digest_hex"].as_str().expect("sealed_digest_hex"));
    assert_eq!(chain(0, &candidates), digest, "the committed digest is the program's chain");
    let clusters = vectors["vectors"].as_array().expect("vectors");
    assert_eq!(clusters.len(), 2, "two cluster-domain vectors");

    let mut log = Vec::new();
    let mut seen = Vec::new();
    for (index, vector) in clusters.iter().enumerate() {
        let cluster = vector["cluster"].as_str().expect("cluster");
        let domain = array32(vector["cluster_domain_hex"].as_str().expect("cluster_domain_hex"));
        let seed_hex = vector["seed_hex"].as_str().expect("seed_hex");
        let tag = if index == 0 { String::new() } else { cluster.to_string() };

        // A real round: the closer is a winner, the four candidates (pinned
        // owners and losses) are the only losers, and the pool genesis carries
        // `domain`.
        let spec = Spec {
            roster: candidates.iter().map(|(_, loss)| exact_loser(*loss)).collect(),
            keys: candidates.iter().map(|(owner, _)| *owner).collect(),
            domain: Some(domain),
            ..Spec::winners_only()
        };
        let mut scene = Scene::new(row, &tag, spec);
        assert_eq!(scene.w.pool().cluster_domain, domain);
        scene.provision_and_seal("build_last");
        let w = &mut scene.w;
        let sealed = w.close_record(1);
        assert_eq!(
            (sealed.selected_set_tag, sealed.preferred_count, sealed.round_id, sealed.close_id),
            (vectors["set_tag"].as_u64().expect("set_tag") as u8, 4, 1, 1)
        );
        assert_eq!(sealed.preferred_digest, digest, "the on-chain digest equals the committed one");

        let (request_pda, seed) = host_request(w, 1);
        assert_eq!(hex(&seed), seed_hex, "host seed for the {cluster} domain");
        let instruction = request_ix(w, 1);
        let requested = ok(w, if index == 0 { "" } else { "request" }, vec![instruction], &[]);
        let close = w.close_record(1);
        assert_eq!(hex(&close.protocol_seed), seed_hex, "on-chain protocol_seed for {cluster}");
        assert_eq!(close.request_account, request_pda);
        assert_eq!(
            request_pda,
            provider::request_binding(&provider_keys().client, &array32(seed_hex)).0,
            "the ORAO request PDA is derived from the committed vector"
        );
        assert_eq!(w.request_state(1).seed, array32(seed_hex));
        assert_eq!(requested.last_event::<DrawRequested>().request_account, request_pda);
        seen.push((close.protocol_seed, request_pda));
        log.extend(scene.w.take_log());
    }
    assert_ne!(seen[0].0, seen[1].0, "devnet and the fixture domain seed differently");
    assert_ne!(seen[0].1, seen[1].1, "so their request PDAs differ and a binding cannot be replayed across clusters");
    log
}

// ---------------------------------------------------------------------------
// Rows: the request and the fulfillment
// ---------------------------------------------------------------------------

pub fn request_decrements_remaining_partitions_sponsor_first_and_records_spent_split(
) -> Vec<Observation> {
    let row = ROW_REQUEST_PARTITIONS;
    // The sponsor partition runs out inside the pending-request rent: the
    // owner partition funds the last 173,520 lamports of it.
    let spec = Spec::standard().with_sponsor(sponsor_for_owner_rent(173_520));
    let mut scene = Scene::new(row, "", spec);
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let closed = w.close_record(1);
    assert_eq!(closed.promotion_deadline, closed.close_started_at + 600, "deadline = start + 600 s");
    assert!(closed.locked_owner_remaining > 0, "the owner opted in for the rest of the reserve");
    let (rent, _) = request_rents();

    let before = w.close_record(1);
    let ops0 = w.ops();
    let cash0 = w.lamports(w.rt.ops_cash_address());
    assert_eq!(before.locked_sponsor_remaining, REQUEST_FEE + rent - 173_520);
    let expected = pclose::plan_request_spend(
        before.locked_sponsor_remaining,
        before.locked_owner_remaining,
        REQUEST_FEE,
        rent,
        pclose::promotion_holdback(&before).expect("holdback"),
    )
    .expect("the request is affordable");
    assert_eq!(
        expected,
        pclose::RequestSpend { fee_s: REQUEST_FEE, fee_o: 0, rent_s: rent - 173_520, rent_o: 173_520 },
        "sponsor first: the fee, then the rent until the sponsor partition is empty"
    );

    let request = do_request(w, 1, "");
    assert_eq!(delta(&request, w.rt.ops_cash_address()), -((REQUEST_FEE + rent) as i128), "OpsCash -(fee + rent)");
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash0 - REQUEST_FEE - rent);
    let after = w.close_record(1);
    assert_eq!(after.locked_sponsor_remaining, 0, "sponsor partition drained first");
    assert_eq!(
        after.locked_owner_remaining,
        before.locked_owner_remaining - 173_520,
        "owner partition pays only the rest"
    );
    assert_eq!(after.spent.provider_fee_s, REQUEST_FEE);
    assert_eq!(after.spent.provider_fee_o, 0);
    assert_eq!(after.spent.request_rent_s, rent - 173_520);
    assert_eq!(after.spent.request_rent_o, 173_520);
    assert_eq!(
        after.spent.provider_fee_s + after.spent.provider_fee_o + after.spent.request_rent_s + after.spent.request_rent_o,
        REQUEST_FEE + rent,
        "the spent split sums exactly"
    );
    assert_eq!(after.request_rent_funded_s, after.spent.request_rent_s);
    assert_eq!(after.request_rent_funded_o, after.spent.request_rent_o);
    let ops = w.ops();
    assert_eq!(ops.provider_fee_consumed_total, ops0.provider_fee_consumed_total + u128::from(REQUEST_FEE));
    assert_eq!(ops.request_rent_outstanding, ops0.request_rent_outstanding + rent);
    assert_eq!(ops.locked_remaining_total, ops0.locked_remaining_total - REQUEST_FEE - rent);
    assert_eq!(ops.free_balance, ops0.free_balance, "free cash is never the source");
    // Nothing is refundable until the fulfillment is observed.
    assert_eq!(
        (after.eligible_return_remaining, after.owner_request_rent_refundable_remaining),
        (0, 0)
    );
    assert_eq!(ops.eligible_return_budget, 0);
    assert!(!after.reimbursement_observed);
    w.take_log()
}

pub fn request_cpi_failure_leaves_partitions_and_ledger_unchanged() -> Vec<Observation> {
    let row = ROW_REQUEST_CPI_FAILURE;
    // The pinned pending-request size understates the real one (100 bytes
    // against ORAO's 759), so `request_round_draw` tops the Client up with too
    // little and the provider's own charge fails inside the CPI.
    let spec = Spec { request_bytes: Some(100), ..Spec::standard() };
    let mut scene = Scene::new(row, "", spec);
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let (request, _) = host_request(w, 1);
    w.watch(request);
    let close_before = w.data(w.rt.close_record_address(1));
    let ops_before = w.data(w.rt.ops_address());
    let (client0, treasury0, cash0) =
        (w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address()));
    let instruction = request_ix(w, 1);
    let failed = reject_provider(w, "", vec![instruction], &[]);
    // The failure is the PROVIDER's: the ORAO program was invoked and failed
    // (for want of lamports, by construction), not one of our own checks.
    let diag = diagnostics(&failed);
    assert!(
        failed.logs.iter().any(|line| line.contains(&format!("Program {} invoke", orao::id()))),
        "the ORAO request CPI was reached: {diag}"
    );
    assert!(
        failed.logs.iter().any(|line| line.contains(&format!("Program {} failed", orao::id()))),
        "and failed inside the provider: {diag}"
    );
    assert_ne!(custom_code(&failed), Some(6041), "not ProviderFeeExceeded");
    assert!(w.account(request).is_none(), "no request account");
    assert_eq!(w.data(w.rt.close_record_address(1)), close_before, "CloseRecord byte-identical");
    assert_eq!(w.data(w.rt.ops_address()), ops_before, "Ops byte-identical");
    let close = w.close_record(1);
    assert_eq!(close.draw_state, state::DRAW_NOT_REQUESTED);
    assert_eq!(close.spent.provider_fee_s + close.spent.provider_fee_o, 0);
    assert_eq!((close.request_rent_funded_s, close.request_rent_funded_o), (0, 0));
    assert_eq!(w.ops().provider_fee_consumed_total, 0);
    assert_eq!(w.ops().request_rent_outstanding, 0);
    assert_eq!(
        (w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address())),
        (client0, treasury0, cash0),
        "the Client top-up reverted with the failed CPI"
    );
    w.take_log()
}

pub fn fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting(
) -> Vec<Observation> {
    let row = ROW_FULFILLMENT;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    let request = w.request_account(1);
    let (pending_rent, retained_rent) = request_rents();
    let keys = provider_keys();

    // A key that is not a configured fulfill authority fulfils nothing: ORAO
    // refuses with its own NotAuthorized (6000) and only the fee moves.
    let before = w.snapshot_all();
    let bad = failure(w.fulfill_with_wrong_authority("wrong_authority", 1));
    bad.assert_rollback_except_fee();
    w.assert_fee_only(&before, &bad);
    let diag = diagnostics(&bad);
    assert!(
        ["NotAuthorized", "0x1770", "Custom(6000)"].iter().any(|needle| diag.contains(needle)),
        "ORAO's NotAuthorized: {diag}"
    );
    assert!(!w.request_fulfilled(1), "still pending");
    assert_eq!(bad.expected_fee_lamports, 10_000, "the Ed25519 signature is charged like a transaction signature");

    let (client0, treasury0, request0) = (w.client_lamports(), w.treasury_lamports(), w.lamports(request));
    assert_eq!(request0, pending_rent);
    let cash0 = w.lamports(w.rt.ops_cash_address());
    let vault0 = w.lamports(w.rt.game_vault_address());
    let (ops0, close0, pool0) = (
        w.data(w.rt.ops_address()),
        w.data(w.rt.close_record_address(1)),
        w.data(w.rt.pool_address()),
    );
    let (dest0, owner0) = (w.lamports(scene.dest), w.lamports(scene.owner.pubkey()));

    let fulfilled = w.fulfill("", 1).expect("authentic fulfillment");
    assert_identities(w);
    let reimbursed = pending_rent - retained_rent;
    assert_eq!(reimbursed, 4_259_520, "the runtime receipt: Client +4,259,520 at local default rent");
    assert_eq!(delta(&fulfilled, keys.client), reimbursed as i128, "Client +reimbursement");
    assert_eq!(delta(&fulfilled, request), -(reimbursed as i128), "RequestAccount -reimbursement");
    assert_eq!(delta(&fulfilled, keys.treasury), 0, "the fee went to the Treasury at request time");
    assert_eq!(w.client_lamports(), client0 + reimbursed);
    assert_eq!(w.treasury_lamports(), treasury0);
    assert_eq!(w.lamports(request), retained_rent);
    assert!(w.request_fulfilled(1));
    match w.request_state(1).state {
        orao::state::request::RequestState::Fulfilled(result) => {
            assert_ne!(result.randomness, [0u8; 64], "the provider output");
        }
        _ => panic!("expected a fulfilled request"),
    }

    // Provider cash moved only between RequestAccount and Client: OpsCash, the
    // books, the game vault and the participants are untouched.
    assert_eq!(delta(&fulfilled, w.rt.ops_cash_address()), 0);
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash0);
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault0);
    assert_eq!(w.data(w.rt.ops_address()), ops0, "Ops (free_balance, partitions, ledgers) untouched");
    assert_eq!(w.data(w.rt.close_record_address(1)), close0, "CloseRecord untouched");
    assert_eq!(w.data(w.rt.pool_address()), pool0);
    assert_eq!((w.lamports(scene.dest), w.lamports(scene.owner.pubkey())), (dest0, owner0));
    for account in &fulfilled.account_changes {
        let key: Pubkey = account.pubkey.parse().expect("pubkey");
        assert!(
            [keys.client, request, w.rt.payer.pubkey()].contains(&key),
            "fulfillment changed {key}, which is neither the Client, the request nor the fee payer"
        );
    }

    // The Client observation is refreshed only by the reconcile instruction.
    assert_eq!(w.ops().client_balance_observed, 0);
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "reconcile_with_client", vec![reconcile], &[]);
    let ops = w.ops();
    assert_eq!(ops.client_balance_observed, client0 + reimbursed);
    assert_eq!(ops.client_balance_observed, w.client_lamports());
    assert_eq!(ops.provider_deficit, 0);
    assert_eq!(ops.eligible_return_budget, 0, "the reimbursement is not yet eligible: it is only observed");
    assert_eq!(ops.free_balance, 0);
    w.take_log()
}

pub fn observe_request_fulfilled_once_sets_eligible_and_owner_refundable_remaining_and_rejects_pending(
) -> Vec<Observation> {
    let row = ROW_OBSERVE;
    let mut scene = Scene::new(
        row,
        "",
        Spec::standard().with_sponsor(sponsor_for_owner_rent(173_520)),
    );
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let (pending_rent, retained_rent) = request_rents();

    // No request yet: nothing to observe.
    let observe = observe_ix(w, 1);
    reject(w, "before_request", vec![observe], &[], ErrorCode::InvalidState);

    do_request(w, 1, "request");
    let request = w.request_account(1);
    // A pending request has no reimbursement: refused, nothing recorded.
    let observe = observe_ix(w, 1);
    reject(w, "pending", vec![observe], &[], ErrorCode::RequestNotFulfilled);
    assert!(!w.close_record(1).reimbursement_observed);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);

    w.fulfill("fulfill", 1).expect("fulfill");
    // The binding is the close's own request and the pinned Client.
    let mut accounts = observe_accounts(w, 1);
    accounts.request = w.rt.close_record_address(1);
    reject(w, "wrong_request", vec![anchor_ix(accounts, instruction::ObserveRequestFulfilled)], &[], ErrorCode::WrongAccount);
    let mut accounts = observe_accounts(w, 1);
    accounts.client = w.rt.pool_address();
    reject(w, "wrong_client", vec![anchor_ix(accounts, instruction::ObserveRequestFulfilled)], &[], ErrorCode::WrongProviderTuple);

    let before = w.close_record(1);
    let ops0 = w.ops();
    let actual = w.lamports(request);
    assert_eq!(actual, retained_rent);
    let (funded_s, funded_o) = (before.request_rent_funded_s, before.request_rent_funded_o);
    assert_eq!((funded_s, funded_o), (pending_rent - 173_520, 173_520));
    let (cash0, client0) = (w.lamports(w.rt.ops_cash_address()), w.client_lamports());

    let observe = observe_ix(w, 1);
    let observed = ok(w, "", vec![observe], &[]);
    let after = w.close_record(1);
    let eligible = (funded_s + funded_o).saturating_sub(actual);
    assert_eq!(eligible, 4_259_520, "eligible = funded - the request's actual retained lamports");
    assert_eq!(after.eligible_return_remaining, eligible);
    assert_eq!(
        after.owner_request_rent_refundable_remaining,
        funded_o.min(eligible),
        "owner first, but only up to what it funded"
    );
    assert_eq!(after.owner_request_rent_refundable_remaining, 173_520);
    assert!(after.reimbursement_observed);
    let ops = w.ops();
    assert_eq!(ops.eligible_return_budget, ops0.eligible_return_budget + eligible);
    assert_eq!(ops.request_rent_outstanding, ops0.request_rent_outstanding - (funded_s + funded_o));
    assert_eq!(ops.request_rent_outstanding, 0);
    assert_eq!((ops.free_balance, ops.locked_remaining_total), (ops0.free_balance, ops0.locked_remaining_total));
    // Observing moves no cash at all.
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash0);
    assert_eq!(w.client_lamports(), client0);
    assert_eq!(w.lamports(request), actual);
    assert!(observed.account_changes.iter().all(|change| {
        let key: Pubkey = change.pubkey.parse().expect("pubkey");
        change.lamport_delta == 0 || key == w.rt.payer.pubkey()
    }));

    // Once only: a second observation is refused and changes nothing.
    let observe = observe_ix(w, 1);
    reject(w, "again", vec![observe], &[], ErrorCode::AlreadyObserved);
    assert_eq!(w.close_record(1).eligible_return_remaining, eligible);
    w.take_log()
}

pub fn changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible() -> Vec<Observation> {
    let row = ROW_CHANGED_RENT;
    let mut scene = fulfilled_scene(row, "", 173_520);
    let w = &mut scene.w;
    let request = w.request_account(1);
    let close = w.close_record(1);
    let funded = close.request_rent_funded_s + close.request_rent_funded_o;
    let actual = w.lamports(request);

    // The rent quote halves between the fulfillment and the observation.
    let mut lowered = Rent::default();
    lowered.lamports_per_byte_year /= 2;
    let quote_based = funded - lowered.minimum_balance(147);
    assert!(quote_based > funded - actual, "a quote-based bound would overstate the reimbursement");

    let observe = observe_ix(w, 1);
    w.send_with_rent_rate("", vec![observe], &[], lowered.lamports_per_byte_year)
        .unwrap_or_else(|failed| panic!("observe under the lowered quote failed: {failed:?}"));
    assert_eq!(w.rt.svm.get_sysvar::<Rent>(), Rent::default(), "the schedule is restored");
    assert_identities(w);
    let after = w.close_record(1);
    assert_eq!(after.eligible_return_remaining, funded - actual, "funded minus the account's actual retained lamports");
    assert_eq!(after.eligible_return_remaining, 4_259_520);
    assert_ne!(after.eligible_return_remaining, quote_based, "not funded minus the new quote");
    assert!(after.eligible_return_remaining <= funded - actual);
    assert_eq!(w.lamports(request), actual, "observation changed no lamports");
    w.take_log()
}

pub fn extra_request_account_lamports_reduce_eligibility_never_increase() -> Vec<Observation> {
    let row = ROW_EXTRA_LAMPORTS;
    let mut log = Vec::new();

    // Unsolicited lamports on the fulfilled request lower the bound.
    let mut scene = fulfilled_scene(row, "", 173_520);
    let w = &mut scene.w;
    let request = w.request_account(1);
    let close = w.close_record(1);
    let funded = close.request_rent_funded_s + close.request_rent_funded_o;
    let honest = funded - w.lamports(request);
    w.rt.add_lamports(request, 1_000_000);
    let observe = observe_ix(w, 1);
    ok(w, "", vec![observe], &[]);
    let after = w.close_record(1);
    assert_eq!(after.eligible_return_remaining, honest - 1_000_000, "eligibility shrank by the airdrop");
    assert_eq!(after.owner_request_rent_refundable_remaining, 173_520.min(after.eligible_return_remaining));

    // After observation nothing can raise it again: more lamports, or a replay.
    let frozen = w.data(w.rt.close_record_address(1));
    w.rt.add_lamports(request, 7_000_000);
    let observe = observe_ix(w, 1);
    reject(w, "replay_after_more_lamports", vec![observe], &[], ErrorCode::AlreadyObserved);
    assert_eq!(w.data(w.rt.close_record_address(1)), frozen);
    log.extend(scene.w.take_log());

    // An airdrop larger than the reimbursement saturates the bound at zero.
    let mut saturated = fulfilled_scene(row, "saturating", 173_520);
    let w = &mut saturated.w;
    let request = w.request_account(1);
    w.rt.add_lamports(request, 5_000_000);
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let after = w.close_record(1);
    assert_eq!(
        (after.eligible_return_remaining, after.owner_request_rent_refundable_remaining),
        (0, 0),
        "saturating, never wrapping"
    );
    assert_eq!(w.ops().eligible_return_budget, 0);
    assert!(after.reimbursement_observed);
    let withdraw = withdraw_ix(w, 1, 1);
    reject_g(w, "nothing_eligible", withdraw, ErrorCode::NothingEligible);
    log.extend(saturated.w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Rows: the authenticated provider return
// ---------------------------------------------------------------------------

/// The Squads stand-in wrapper that `FinancialRuntime::execute_via_vault`
/// builds, so a vault-signed instruction can share a transaction with others.
pub(crate) fn via_vault(multisig: Pubkey, inner: Instruction) -> Instruction {
    let (vault, bump) = crate::runtime::fixtures::squads::vault_address(&multisig, 0);
    let mut data = vec![0u8, bump];
    data.extend_from_slice(&inner.data);
    let mut accounts = vec![
        AccountMeta::new_readonly(multisig, false),
        AccountMeta::new_readonly(vault, false),
        AccountMeta::new_readonly(inner.program_id, false),
    ];
    accounts.extend(inner.accounts.into_iter().map(|mut meta| {
        if meta.pubkey == vault {
            meta.is_signer = false;
        }
        meta
    }));
    Instruction {
        program_id: musical_chairs_program::controls::SQUADS_PROGRAM_ID,
        accounts,
        data,
    }
}

/// A program-owned copy of config v1 at an address that is NOT its PDA: Anchor
/// decodes it, then the `seeds` constraint refuses it (`ConstraintSeeds`, 2006).
pub(crate) fn decoy_config(w: &mut World) -> Pubkey {
    let key = Pubkey::new_from_array([0x95; 32]);
    let original = w.account(w.rt.config_address(1)).expect("config v1");
    w.rt.svm.set_account(key, original).expect("decoy config");
    key
}

/// A failure of a G-signed instruction inside the provider (ORAO's codes).
pub(crate) fn reject_provider_g(w: &mut World, variant: &str, inner: Instruction) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send_via_g(variant, inner, &[]));
    failed.assert_rollback_except_fee();
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

/// A rejection of an instruction executed as another Squads multisig's vault.
pub(crate) fn reject_vault(
    w: &mut World,
    variant: &str,
    multisig: Pubkey,
    inner: Instruction,
    expected: ErrorCode,
) -> Observation {
    let before = w.snapshot_all();
    let failed = failure(w.send_via_vault(variant, multisig, inner, &[]));
    w.assert_rejected(&failed, expected);
    w.assert_fee_only(&before, &failed);
    assert_identities(w);
    failed
}

/// G's own ORAO `withdraw` (not the program's instruction): the Client owner
/// pulls `amount` out of the Client to itself.
pub(crate) fn raw_withdraw_ix(w: &World, amount: u64) -> Instruction {
    foreign_ix(
        orao::id(),
        orao::accounts::Withdraw {
            payer: w.rt.g_vault(),
            client: provider_keys().client,
        },
        orao::instruction::Withdraw {
            params: orao::WithdrawParams::new(amount),
        },
    )
}

/// Client lamports above its own rent: what ORAO will let the owner withdraw.
pub(crate) fn client_spendable(w: &World) -> u64 {
    let client = w.account(provider_keys().client).expect("Client account");
    client.lamports - w.rt.rent(client.data.len())
}

pub fn withdraw_provider_return_executes_orao_withdraw_cpi_and_asserts_client_and_opscash_deltas(
) -> Vec<Observation> {
    let row = ROW_WITHDRAW_RETURN;
    let mut scene = observed_scene(row, "", 173_520);
    let w = &mut scene.w;
    let keys = provider_keys();
    let g_vault = w.rt.g_vault();
    let cash = w.rt.ops_cash_address();
    let dest = scene.dest;
    let (close0, ops0) = (w.close_record(1), w.ops());
    let amount = close0.eligible_return_remaining;
    let share = amount.min(close0.owner_request_rent_refundable_remaining);
    assert_eq!((amount, share), (4_259_520, 173_520));
    let (client0, cash0, g0, dest0) =
        (w.client_lamports(), w.lamports(cash), w.lamports(g_vault), w.lamports(dest));

    let instruction = withdraw_ix(w, 1, amount);
    let returned = ok_g(w, "", instruction);
    assert!(
        returned.logs.iter().any(|line| line.contains(&format!("Program {} invoke", orao::id()))),
        "the ORAO withdraw CPI really executed"
    );
    assert!(
        returned.logs.iter().any(|line| line.contains(&format!("Program {} success", orao::id()))),
        "and succeeded"
    );
    // Client -amount (the CPI), the G vault is only a pass-through, OpsCash
    // +amount in the same instruction minus the owner share paid on.
    assert_eq!(w.client_lamports(), client0 - amount);
    assert_eq!(delta(&returned, keys.client), -(amount as i128));
    assert_eq!(w.lamports(g_vault), g0, "the G vault forwarded exactly what ORAO paid it");
    assert_eq!(w.lamports(cash), cash0 + amount - share);
    assert_eq!(delta(&returned, cash), (amount - share) as i128);
    assert_eq!(w.lamports(dest), dest0 + share, "the owner share reaches the immutable destination");
    assert_eq!(delta(&returned, dest), share as i128);

    let (close, ops) = (w.close_record(1), w.ops());
    assert_eq!(close.eligible_return_remaining, 0);
    assert_eq!(close.owner_request_rent_refundable_remaining, 0);
    assert_eq!(close.request_rent_returned, amount);
    assert_eq!(ops.eligible_return_budget, ops0.eligible_return_budget - amount);
    assert_eq!(ops.eligible_return_budget, 0);
    assert_eq!(ops.free_balance, ops0.free_balance + amount - share, "the remainder goes to free");
    assert_eq!(ops.provider_returned_total, ops0.provider_returned_total + u128::from(amount));
    assert_eq!(ops.unallocated_excess, ops0.unallocated_excess, "a return is not an anonymous credit");
    assert_eq!(ops.locked_remaining_total, ops0.locked_remaining_total);
    let client_account = w.account(keys.client).expect("Client");
    assert_eq!(ops.client_balance_observed, client_account.lamports);
    assert_eq!(ops.client_protected_rent, w.rt.rent(client_account.data.len()));
    assert_eq!(ops.provider_deficit, 0);
    w.take_log()
}

pub fn two_partial_provider_returns_then_replay_pay_exact_owner_residual_once() -> Vec<Observation> {
    let row = ROW_TWO_PARTIAL;
    let mut scene = observed_scene(row, "", 173_520);
    let w = &mut scene.w;
    let cash = w.rt.ops_cash_address();
    let dest = scene.dest;
    let funded_o = w.close_record(1).request_rent_funded_o;
    assert_eq!(funded_o, 173_520);
    let excess0 = w.ops().unallocated_excess;
    let eligible = w.close_record(1).eligible_return_remaining;
    let dest0 = w.lamports(dest);
    let free0 = w.ops().free_balance;

    // 1. Smaller than the owner's refundable share: it is all owner money.
    let first = 100_000;
    let instruction = withdraw_ix(w, 1, first);
    ok_g(w, "first_partial", instruction);
    let close = w.close_record(1);
    assert_eq!(w.lamports(dest) - dest0, first, "all of it went to the owner destination");
    assert_eq!(w.ops().free_balance, free0, "none of it reached free");
    assert_eq!(close.owner_request_rent_refundable_remaining, funded_o - first);
    assert_eq!(close.eligible_return_remaining, eligible - first);

    // 2. The rest: the owner's residual once, everything else to free.
    let second = eligible - first;
    let (cash1, free1) = (w.lamports(cash), w.ops().free_balance);
    let instruction = withdraw_ix(w, 1, second);
    ok_g(w, "", instruction);
    let close = w.close_record(1);
    assert_eq!(w.lamports(dest) - dest0, funded_o, "the owner received exactly what it funded, once");
    assert_eq!(w.ops().free_balance, free1 + second - (funded_o - first));
    assert_eq!(w.lamports(cash), cash1 + second - (funded_o - first));
    assert_eq!(close.owner_request_rent_refundable_remaining, 0);
    assert_eq!(close.eligible_return_remaining, 0);
    assert_eq!(close.request_rent_returned, eligible);
    assert_eq!(w.ops().eligible_return_budget, 0);

    // 3. A third call, and a replay of the first, find nothing.
    let frozen = (w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1)));
    let instruction = withdraw_ix(w, 1, 1);
    reject_g(w, "third_call", instruction, ErrorCode::NothingEligible);
    let instruction = withdraw_ix(w, 1, first);
    reject_g(w, "replay_of_first", instruction, ErrorCode::NothingEligible);
    assert_eq!((w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1))), frozen);
    assert_eq!(w.lamports(dest) - dest0, funded_o, "the owner share was not paid twice");
    assert_eq!(w.ops().unallocated_excess, excess0, "no return was parked");
    w.take_log()
}

/// IMPL-45 (Codex review of 366df88, DR-14): a provider return whose owner
/// share cannot be delivered to the bound destination is refused BEFORE the
/// ORAO CPI, fee-only, with the Client, the request account and the
/// eligibility / owner-refundable counters untouched; it is never re-locked
/// into the spendable owner partition.  Expiry and finalization proceed
/// regardless; once the destination can receive the credit the same return
/// pays the owner exactly once.  RED on the IMPL-42/43 ELF (which re-locked
/// the share), GREEN on the IMPL-45 ELF.
pub fn undeliverable_owner_share_is_refused_before_the_cpi_and_retried_once_the_destination_is_funded(
) -> Vec<Observation> {
    let row = ROW_RETURN_PREFLIGHT;
    let mut scene = observed_scene(row, "", 173_520);
    let dest = scene.dest;
    let w = &mut scene.w;
    let keys = provider_keys();
    let cash = w.rt.ops_cash_address();
    // The bound destination no longer exists; a 100,000-lamport credit would
    // leave it below the rent-exempt minimum, so it is not deliverable.
    w.rt.remove_account(dest);
    assert_eq!(w.lamports(dest), 0);
    let minimum = w.rt.rent(0);
    let first = 100_000u64;
    assert!(first < minimum);
    let (close0, ops0) = (w.close_record(1), w.ops());
    assert!(close0.owner_request_rent_refundable_remaining >= first, "the owner is owed a share of this return");
    assert_eq!(close0.promotion, state::Promotion::Pending, "the close is still pending");
    let (client0, cash0) = (w.lamports(keys.client), w.lamports(cash));
    let frozen = (w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1)));
    let request_data = w.data(close0.request_account);

    // 1. Refused before the CPI: fee-only; Client, request and counters untouched.
    let instruction = withdraw_ix(w, 1, first);
    let rejected = reject_g(w, "refused_before_cpi", instruction, ErrorCode::RefundNotDeliverable);
    let orao_invoke = format!("Program {} invoke", orao::id());
    assert!(
        !rejected.logs.iter().any(|line| line.starts_with(&orao_invoke)),
        "the ORAO withdraw CPI never ran: {:?}",
        rejected.logs
    );
    assert!(
        rejected.logs.iter().any(|line| line.contains("Error Code: RefundNotDeliverable")),
        "refused by the preflight: {:?}",
        rejected.logs
    );
    assert_eq!((w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1))), frozen, "nothing booked");
    assert_eq!(w.data(close0.request_account), request_data, "the request account is untouched");
    assert_eq!((w.lamports(keys.client), w.lamports(cash)), (client0, cash0), "Client and OpsCash untouched");
    let close = w.close_record(1);
    assert_eq!(close.locked_owner_remaining, close0.locked_owner_remaining, "never re-locked into the owner partition");
    assert_eq!(close.eligible_return_remaining, close0.eligible_return_remaining);
    assert_eq!(close.owner_request_rent_refundable_remaining, close0.owner_request_rent_refundable_remaining);
    assert_eq!(w.ops().unallocated_excess, ops0.unallocated_excess);
    assert_identities(w);

    // 2. Expiry and finalization never depend on the provider return.
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    let close = w.close_record(1);
    assert!(close.finalized && close.reserve_settled);
    assert_eq!(close.eligible_return_remaining, close0.eligible_return_remaining, "eligibility survives settlement and finalization");
    assert_eq!(close.owner_request_rent_refundable_remaining, close0.owner_request_rent_refundable_remaining);
    assert_identities(w);

    // 3. Funded destination: the same return pays the owner exactly once.
    w.rt.credit_lamports(dest, minimum);
    let (dest1, free1, excess1) = (w.lamports(dest), w.ops().free_balance, w.ops().unallocated_excess);
    let instruction = withdraw_ix(w, 1, first);
    // The headline observation of the row: the funded-destination retry.
    ok_g(w, "", instruction);
    assert_eq!(w.lamports(dest) - dest1, first, "all of it is owner money");
    assert_eq!(w.ops().free_balance, free1, "none of it reached free");
    assert_eq!(w.ops().unallocated_excess, excess1, "none of it was parked");
    let close = w.close_record(1);
    assert_eq!(close.owner_request_rent_refundable_remaining, close0.owner_request_rent_refundable_remaining - first);
    assert_eq!(close.eligible_return_remaining, close0.eligible_return_remaining - first);
    assert_eq!(close.request_rent_returned, first);
    assert_identities(w);
    w.take_log()
}

pub fn g_signature_without_executed_withdraw_cpi_credits_nothing_and_anonymous_opscash_surplus_is_never_reclassified(
) -> Vec<Observation> {
    let row = ROW_G_SIGNATURE;
    let mut scene = observed_scene(row, "", 173_520);
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    let keys = provider_keys();
    let g_vault = w.rt.g_vault();
    let cash = w.rt.ops_cash_address();
    let (close0, ops0) = (w.close_record(1), w.ops());
    let eligible = close0.eligible_return_remaining;

    // G signs a plain system transfer of 777,777 lamports into OpsCash: no
    // ORAO withdraw ran, so it is an anonymous credit.  The same transaction
    // reconciles OpsCash (the registers must balance after every transaction).
    let credit = 777_777u64;
    let g0 = w.lamports(g_vault);
    let transfer = solana_system_interface::instruction::transfer(&g_vault, &cash, credit);
    let wrapped = via_vault(w.rt.g_multisig(), transfer);
    let reconcile = reconcile_ix(&w.rt);
    ok(w, "g_plain_transfer", vec![wrapped, reconcile], &[]);
    assert_eq!(w.lamports(g_vault), g0 - credit);
    let (close, ops) = (w.close_record(1), w.ops());
    assert_eq!(ops.unallocated_excess, ops0.unallocated_excess + credit, "classified as unallocated excess");
    assert_eq!(ops.free_balance, ops0.free_balance, "never free");
    assert_eq!(ops.locked_remaining_total, ops0.locked_remaining_total, "never a partition");
    assert_eq!(ops.eligible_return_budget, ops0.eligible_return_budget);
    assert_eq!(ops.provider_returned_total, ops0.provider_returned_total);
    assert_eq!(close.eligible_return_remaining, eligible, "a signature credits no return");
    assert_eq!(close.request_rent_returned, 0);

    // Anonymous Client credits may physically back a withdrawal but never
    // raise any eligible budget.
    let anon = 1_000_000u64;
    let instruction = solana_system_interface::instruction::transfer(&attacker.pubkey(), &keys.client, anon);
    ok(w, "anonymous_client_credit", vec![instruction], &[&attacker]);
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "observe_client", vec![reconcile], &[]);
    let ops = w.ops();
    assert_eq!(ops.client_balance_observed, w.client_lamports());
    assert_eq!(ops.eligible_return_budget, ops0.eligible_return_budget, "the budget never follows the Client balance");
    assert_eq!(w.close_record(1).eligible_return_remaining, eligible);
    assert!(client_spendable(w) > eligible, "the Client could physically cover more than is eligible");
    let instruction = withdraw_ix(w, 1, eligible + 1);
    reject_g(w, "more_than_eligible", instruction, ErrorCode::NothingEligible);

    // The honest return still works and leaves the surplus alone.
    let free_before = w.ops().free_balance;
    let share = eligible.min(close.owner_request_rent_refundable_remaining);
    let instruction = withdraw_ix(w, 1, eligible);
    ok_g(w, "", instruction);
    let ops = w.ops();
    assert_eq!(ops.unallocated_excess, ops0.unallocated_excess + credit, "the surplus was not reclassified");
    assert_eq!(ops.free_balance, free_before + eligible - share);
    assert_eq!(client_spendable(w), anon, "the anonymous Client credit stays in the Client");
    w.take_log()
}

pub fn withdraw_rejects_wrong_client_wrong_destination_and_amount_above_eligible() -> Vec<Observation> {
    let row = ROW_WITHDRAW_REJECTS;

    // A degraded G profile (threshold 1 instead of 2-of-3) must be refused with
    // `WrongGovernanceProfile` like every other G instruction: the pre-fix
    // program only checked the vault signer here (M10 review SEC-5).  Own
    // scene so a pre-fix success cannot disturb the tamper rows below.
    let mut degraded = observed_scene(row, "degraded_g_profile", 173_520);
    {
        let w = &mut degraded.w;
        let eligible = w.close_record(1).eligible_return_remaining;
        let g_multisig = w.rt.g_multisig();
        w.rt.install_squads_account(
            g_multisig,
            musical_chairs_program::controls::SQUADS_PROGRAM_ID,
            crate::controls::g_multisig_bytes_with_threshold(1),
        );
        let instruction = withdraw_ix(w, 1, eligible);
        reject_g(w, "degraded_g_profile", instruction, ErrorCode::WrongGovernanceProfile);
    }

    let mut scene = observed_scene(row, "", 173_520);
    let w = &mut scene.w;
    let eligible = w.close_record(1).eligible_return_remaining;

    // Wrong accounts, one at a time (each against the otherwise honest call).
    type Tamper = fn(&mut accounts::WithdrawProviderReturn);
    let cases: Vec<(&str, Tamper, ErrorCode)> = vec![
        ("wrong_client", |a| a.client = Pubkey::new_from_array([0x91; 32]), ErrorCode::WrongProviderTuple),
        ("wrong_provider_program", |a| a.provider_program = system_program::ID, ErrorCode::WrongProviderTuple),
        ("wrong_destination", |a| a.owner_refund = Pubkey::new_from_array([0x92; 32]), ErrorCode::WrongAccount),
        // A key with no Squads account behind it fails the profile read first
        // (`WrongAccount`), exactly like every other G instruction, since M10
        // SEC-5 routed this instruction through `require_governance` (it was
        // `NotAuthorized` from the bare vault-signer check before).
        ("rogue_multisig", |a| a.governance_multisig = Pubkey::new_from_array([0x93; 32]), ErrorCode::WrongAccount),
    ];
    for (index, (name, tamper, expected)) in cases.into_iter().enumerate() {
        let mut accounts = withdraw_accounts(w, 1);
        tamper(&mut accounts);
        let instruction = anchor_ix(accounts, instruction::WithdrawProviderReturn { amount: eligible });
        let variant = if index == 0 { "" } else { name };
        reject_g(w, variant, instruction, expected);
    }

    // The config account must be the close's pinned version: a decodable
    // ConfigVersion at the wrong address fails Anchor's seeds constraint.
    let decoy = decoy_config(w);
    let mut accounts = withdraw_accounts(w, 1);
    accounts.config = decoy;
    let instruction = anchor_ix(accounts, instruction::WithdrawProviderReturn { amount: eligible });
    reject_framework(w, "wrong_config_version", vec![via_vault(w.rt.g_multisig(), instruction)], &[], 2006);

    // Amounts: above eligible and zero are NothingEligible.
    let instruction = withdraw_ix(w, 1, eligible + 1);
    reject_g(w, "amount_above_eligible", instruction, ErrorCode::NothingEligible);
    let instruction = withdraw_ix(w, 1, u64::MAX);
    reject_g(w, "amount_max", instruction, ErrorCode::NothingEligible);
    let instruction = withdraw_ix(w, 1, 0);
    reject_g(w, "amount_zero", instruction, ErrorCode::NothingEligible);

    // Only G: the P vault cannot stand in for it, and an unsigned vault is not
    // a signer at all.
    let p_multisig = w.rt.p_multisig();
    let mut accounts = withdraw_accounts(w, 1);
    accounts.governance = w.rt.p_vault();
    let instruction = anchor_ix(accounts, instruction::WithdrawProviderReturn { amount: eligible });
    reject_vault(w, "p_vault_signer", p_multisig, instruction, ErrorCode::NotAuthorized);
    let mut instruction = withdraw_ix(w, 1, eligible);
    instruction.accounts[1].is_signer = false;
    reject_framework(
        w,
        "governance_not_a_signer",
        vec![instruction],
        &[],
        anchor_lang::error::ErrorCode::AccountNotSigner as u32,
    );

    // Everything above left the books byte-identical, and the honest call
    // still pays.
    let close = w.close_record(1);
    assert_eq!(close.eligible_return_remaining, eligible);
    assert_eq!(close.request_rent_returned, 0);
    let instruction = withdraw_ix(w, 1, eligible);
    ok_g(w, "honest", instruction);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);
    w.take_log()
}

pub fn cpi_failure_rolls_back_all_accounting() -> Vec<Observation> {
    let row = ROW_WITHDRAW_CPI_FAILURE;
    let mut scene = observed_scene(row, "", 173_520);
    let w = &mut scene.w;
    let dest = scene.dest;
    let close = w.close_record(1);
    let eligible = close.eligible_return_remaining;

    // G pulls the Client down to its own rent elsewhere (outside the program).
    let drain = client_spendable(w);
    assert!(drain >= eligible);
    let (ops_before_drain, close_before_drain) =
        (w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1)));
    let instruction = raw_withdraw_ix(w, drain);
    ok_g(w, "g_drains_client", instruction);
    assert_eq!(client_spendable(w), 0);
    assert_eq!(w.data(w.rt.ops_address()), ops_before_drain, "the program's books do not see it");
    assert_eq!(w.data(w.rt.close_record_address(1)), close_before_drain);

    // Now the program's `withdraw_provider_return` clears its own checks (the
    // close still has `eligible` on the books) and the ORAO CPI fails: the
    // whole transaction rolls back, G's signature credits nothing.
    let (ops_before, close_before) = (w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1)));
    let (cash0, dest0, g0) = (
        w.lamports(w.rt.ops_cash_address()),
        w.lamports(dest),
        w.lamports(w.rt.g_vault()),
    );
    let instruction = withdraw_ix(w, 1, eligible);
    let failed = reject_provider_g(w, "", instruction);
    assert_ne!(custom_code(&failed), Some(6048), "not our NothingEligible: the books allowed it");
    assert_eq!(w.data(w.rt.ops_address()), ops_before, "Ops byte-identical after the failed CPI");
    assert_eq!(w.data(w.rt.close_record_address(1)), close_before, "CloseRecord byte-identical");
    assert_eq!(w.close_record(1).eligible_return_remaining, eligible);
    assert_eq!(w.close_record(1).request_rent_returned, 0);
    assert_eq!(
        (w.lamports(w.rt.ops_cash_address()), w.lamports(dest), w.lamports(w.rt.g_vault())),
        (cash0, dest0, g0)
    );
    w.take_log()
}

// ---------------------------------------------------------------------------
// A second close in a second round
// ---------------------------------------------------------------------------

/// The facts of a second close.
pub(crate) struct SecondClose {
    pub(crate) payout: PayoutPaid,
}

/// With close 1 finalized (round 2, no close in progress), set up a fresh
/// round and close it with a real `withdraw_simple` by `closer` (nonce
/// `nonce`) to `dest`.  `backers` become open Series A Backers first; `others`
/// extra winners join the round.  Round 2 has its own `CloseRecord(2)`.
pub(crate) fn start_second_close(
    w: &mut World,
    closer: &Keypair,
    nonce: u64,
    dest: Pubkey,
    backers: &[Pubkey],
    others: u64,
) -> SecondClose {
    let pool = w.pool();
    assert_eq!((pool.round_id, pool.current_close_id, pool.next_close_id), (2, None, 2));
    if !backers.is_empty() {
        let specs: Vec<BackerSpec> = backers
            .iter()
            .map(|owner| BackerSpec::new(*owner, SERIES_A, 62_000_000))
            .collect();
        w.install_backer_specs(&specs);
        // Fixture Backers get the `User` a real `fund_series_a` creates (claim
        // requires it); see `Scene::new`.
        for owner in backers {
            w.install_user(*owner, true, false);
        }
    }
    // A toll rides a ticket when the registry is non-empty, and the ticket
    // needs snapshot pages covering the registry.
    let registry_len = w.pool().registry_len;
    let needed = registry_len.div_ceil(PAGE);
    for index in w.pool().snapshot_pages..needed {
        w.rt.replace_snapshot(state::SnapshotPage {
            bump: pda(&[state::SNAPSHOT_SEED, &index.to_le_bytes()]).1,
            index,
            ..state::SnapshotPage::default()
        });
    }
    w.set_pool(|pool| pool.snapshot_pages = pool.snapshot_pages.max(needed));
    w.watch(dest);
    w.open_fixture_position(closer.pubkey(), nonce, PlanKindArg::Simple, SOL);
    for index in 0..others {
        w.install_participant(World::synthetic_participant(40 + index), 100_000_000, 150_000_000, false, false);
    }
    w.set_pot_fixture(50_000_000);
    w.provision_close_record(2);
    w.warp_seconds(DAY);
    let reserve = w.expected_reserve(1 + others, registry_len, 0, 0);
    w.set_ops_free(reserve);
    let paid = w
        .withdraw_simple("closing_payout_c2", closer, nonce, 0, dest, 0)
        .unwrap_or_else(|failed| panic!("the second closing payout failed: {failed:?}"));
    let payout = paid.last_event::<PayoutPaid>();
    assert!(payout.closing && payout.close_id == Some(2), "the second payout closes round 2 as close 2");
    let bound = w.close_record(2);
    assert_eq!(bound.promotion_deadline, bound.close_started_at + 600, "deadline = start + 600 s");
    assert_eq!(bound.owner_refund_destination, dest);
    assert_identities(w);
    w.watch(w.rt.close_record_address(2));
    SecondClose { payout }
}

/// Warp past the deadline, expire and finalize `close_id`.
pub(crate) fn finish_by_expiry(w: &mut World, close_id: u64) {
    w.warp_past_deadline(close_id);
    let expire = expire_ix(w, close_id);
    ok(w, &lbl(close_id, "expire"), vec![expire], &[]);
    let finalize = finalize_ix(w, close_id);
    ok(w, &lbl(close_id, "finalize"), vec![finalize], &[]);
}

/// Round 2 with `attacker` as an open Series A Backer: commit the closing
/// ticket, let the Backer claim, then expire and finalize the close.  Returns
/// the claimed lamports.
pub(crate) fn round2_cash_progress(w: &mut World, closer: &Keypair, attacker: &Keypair) -> u64 {
    let dest2 = Pubkey::new_from_array([0xDF; 32]);
    start_second_close(w, closer, 2, dest2, &[attacker.pubkey()], 3);
    assert!(!w.close_record(2).cash_committed, "the toll rides a closing ticket");
    w.advance_until_idle(8);
    assert_identities(w);
    assert!(w.close_record(2).cash_committed, "the ticket committed");
    let claimed = w
        .claim_backer("claim", attacker, Pubkey::new_from_array([0xE1; 32]))
        .unwrap_or_else(|failed| panic!("the Backer's claim failed: {failed:?}"));
    assert_identities(w);
    assert!(claimed > 0, "the Backer was paid");
    finish_by_expiry(w, 2);
    claimed
}

/// Replace a program-owned or ORAO-owned account, returning what it was.
pub(crate) fn tamper(w: &mut World, key: Pubkey, change: impl FnOnce(&mut Account)) -> Account {
    let original = w.account(key).expect("account to tamper with");
    let mut changed = original.clone();
    change(&mut changed);
    w.rt.svm.set_account(key, changed).expect("tampered account");
    original
}

pub(crate) fn restore(w: &mut World, key: Pubkey, original: Account) {
    w.rt.svm.set_account(key, original).expect("restored account");
}

// ---------------------------------------------------------------------------
// Rows: reimbursements across closes, deficits, late results
// ---------------------------------------------------------------------------

pub fn two_closes_cannot_claim_the_same_reimbursement() -> Vec<Observation> {
    let row = ROW_TWO_CLOSES_REIMBURSEMENT;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let owner = scene.owner.insecure_clone();
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    finish_by_expiry(w, 1);

    let dest2 = Pubkey::new_from_array([0xDF; 32]);
    start_second_close(w, &owner, 2, dest2, &[], 3);
    provision_candidate_pages(w, 2);
    seal_labelled(w, 2, 4, "build_last");
    do_request(w, 2, &lbl(2, "request"));
    w.fulfill(&lbl(2, "fulfill"), 2).expect("fulfill");
    let observe = observe_ix(w, 2);
    ok(w, &lbl(2, "observe"), vec![observe], &[]);

    // Each close froze its OWN eligibility from its OWN retained binding.
    let (first, second) = (w.close_record(1), w.close_record(2));
    let (pending, retained) = request_rents();
    assert_ne!(first.request_account, second.request_account);
    assert_eq!(first.eligible_return_remaining, pending - retained);
    assert_eq!(second.eligible_return_remaining, pending - retained);
    let both = first.eligible_return_remaining + second.eligible_return_remaining;
    assert_eq!(w.ops().eligible_return_budget, both, "the aggregate is the sum of the two closes");
    assert_eq!(client_spendable(w), both, "the Client holds both reimbursements");

    // The Client physically holds close 1's money, but close 2 can claim only
    // its own.
    let over = withdraw_ix(w, 2, second.eligible_return_remaining + 1);
    reject_g(w, "second_claims_more_than_its_own", over, ErrorCode::NothingEligible);
    let free0 = w.ops().free_balance;
    let instruction = withdraw_ix(w, 1, first.eligible_return_remaining);
    ok_g(w, "", instruction);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);
    assert_eq!(w.close_record(2).eligible_return_remaining, second.eligible_return_remaining);
    assert_eq!(w.ops().free_balance, free0 + first.eligible_return_remaining, "sponsor-funded: all of it to free");
    assert_eq!(client_spendable(w), second.eligible_return_remaining);
    // Close 1 is spent: it cannot reach into close 2's reimbursement now
    // sitting in the Client.
    let again = withdraw_ix(w, 1, 1);
    reject_g(w, "first_claims_again", again, ErrorCode::NothingEligible);
    let instruction = withdraw_ix(w, 2, second.eligible_return_remaining);
    ok_g(w, "second_return", instruction);
    assert_eq!(w.ops().eligible_return_budget, 0);
    assert_eq!(client_spendable(w), 0);
    w.take_log()
}

pub fn second_close_cannot_reuse_first_close_request_binding() -> Vec<Observation> {
    let row = ROW_SECOND_CLOSE_BINDING;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let owner = scene.owner.insecure_clone();
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    finish_by_expiry(w, 1);
    let first = w.close_record(1);

    let dest2 = Pubkey::new_from_array([0xDF; 32]);
    start_second_close(w, &owner, 2, dest2, &[], 3);
    provision_candidate_pages(w, 2);
    seal_labelled(w, 2, 4, "build_last");
    let (request2, seed2) = host_request(w, 2);
    assert_ne!(seed2, first.protocol_seed, "round and close are bound into the seed");
    assert_ne!(request2, first.request_account, "so the second close's request PDA is its own");

    // The second close cannot request into the first close's account...
    let mut accounts = request_accounts(w, 2);
    accounts.request = first.request_account;
    reject(w, "request_into_first_binding", vec![anchor_ix(accounts, instruction::RequestRoundDraw)], &[], ErrorCode::WrongAccount);
    do_request(w, 2, &lbl(2, "request"));
    w.fulfill(&lbl(2, "fulfill"), 2).expect("fulfill");
    assert_eq!(w.close_record(2).request_account, request2);

    // ... nor capture or observe through it.
    let mut accounts = capture_accounts(w, 2);
    accounts.request = first.request_account;
    reject(w, "capture_first_binding", vec![anchor_ix(accounts, instruction::CaptureRoundDraw)], &[], ErrorCode::WrongAccount);
    let mut accounts = observe_accounts(w, 2);
    accounts.request = first.request_account;
    reject(w, "observe_first_binding", vec![anchor_ix(accounts, instruction::ObserveRequestFulfilled)], &[], ErrorCode::WrongAccount);
    // The first close's reimbursement was observed once and stays observed.
    let observe = observe_ix(w, 1);
    reject(w, "first_observed_again", vec![observe], &[], ErrorCode::AlreadyObserved);

    // Each close captures and observes only through its own binding.
    let capture = capture_ix(w, 2);
    ok(w, &lbl(2, "capture"), vec![capture], &[]);
    let observe = observe_ix(w, 2);
    ok(w, "", vec![observe], &[]);
    assert_eq!(w.close_record(1).protocol_seed, first.protocol_seed, "close 1 untouched");
    assert_eq!(w.close_record(1).request_account, first.request_account);
    assert_ne!(w.close_record(2).protocol_seed, w.close_record(1).protocol_seed);
    w.take_log()
}

pub fn observed_deficit_then_real_return_lowers_it_and_never_blocks_commit_expire_finalize_or_claims(
) -> Vec<Observation> {
    let row = ROW_DEFICIT_THEN_RETURN;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let owner = scene.owner.insecure_clone();
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let eligible = w.close_record(1).eligible_return_remaining;
    assert_eq!(w.ops().eligible_return_budget, eligible);

    // G pulls the reimbursement out of the Client: a recorded deficit.
    let drain = client_spendable(w);
    assert_eq!(drain, eligible);
    let instruction = raw_withdraw_ix(w, drain);
    ok_g(w, "g_drains_client", instruction);
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "observe_deficit", vec![reconcile], &[]);
    let ops = w.ops();
    let deficit = ops.provider_deficit;
    assert_eq!(
        deficit,
        pclose::provider_deficit(ops.eligible_return_budget, ops.client_balance_observed, ops.client_protected_rent)
    );
    assert_eq!(deficit, eligible, "the whole reimbursement is missing from the Client");
    let game_before = (w.pool().pot, w.pool().seed, w.pool().funded_claims);

    // Deficits are informational: expiry and finalization proceed, and so do
    // a later close's commit and a Backer's claim.
    finish_by_expiry(w, 1);
    assert_eq!(w.pool().round_id, 2, "finalized with a deficit on the books");
    assert!(w.pool().pot >= game_before.0, "no game funds were taken to cover the deficit");
    let claimed = round2_cash_progress(w, &owner, &attacker);
    assert!(claimed > 0);
    assert_eq!(w.ops().provider_deficit, deficit, "none of that touched the deficit");

    // Someone puts the money back into the Client (an anonymous credit); the
    // recorded deficit is stale until the next observation of the Client...
    let instruction = solana_system_interface::instruction::transfer(&attacker.pubkey(), &provider_keys().client, eligible);
    ok(w, "anonymous_client_credit", vec![instruction], &[&attacker]);
    assert_eq!(w.ops().provider_deficit, deficit);
    // ... and a REAL return (the executed ORAO withdraw) refreshes it: lower.
    let free0 = w.ops().free_balance;
    let instruction = withdraw_ix(w, 1, eligible);
    ok_g(w, "", instruction);
    let ops = w.ops();
    assert!(ops.provider_deficit < deficit, "the real return lowered the recorded deficit");
    assert_eq!(
        ops.provider_deficit,
        pclose::provider_deficit(ops.eligible_return_budget, ops.client_balance_observed, ops.client_protected_rent)
    );
    assert_eq!(ops.provider_deficit, 0);
    assert_eq!(ops.free_balance, free0 + eligible, "the return reached free cash");
    w.take_log()
}

pub fn g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims(
) -> Vec<Observation> {
    let row = ROW_G_WITHDRAWS_ELSEWHERE;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let owner = scene.owner.insecure_clone();
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let eligible = w.close_record(1).eligible_return_remaining;
    let ops0 = w.ops();
    let ops0_bytes = w.data(w.rt.ops_address());
    assert_eq!(ops0.provider_deficit, 0);

    // G withdraws the Client elsewhere.  The program's books do not move; the
    // next observation of the Client records the shortfall.
    let drain = client_spendable(w);
    let instruction = raw_withdraw_ix(w, drain);
    ok_g(w, "", instruction);
    assert_eq!(w.data(w.rt.ops_address()), ops0_bytes, "Ops did not move");
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "observe_deficit", vec![reconcile], &[]);
    let ops = w.ops();
    assert_eq!(ops.provider_deficit, eligible, "informational deficit = the eligible budget the Client no longer covers");
    assert_eq!(ops.client_balance_observed, w.client_lamports());
    assert_eq!(ops.eligible_return_budget, ops0.eligible_return_budget, "nothing else changed");
    assert_eq!((ops.free_balance, ops.locked_remaining_total), (ops0.free_balance, ops0.locked_remaining_total));

    // The return itself fails inside the provider and credits nothing.
    let instruction = withdraw_ix(w, 1, eligible);
    reject_provider_g(w, "return_fails", instruction);
    assert_eq!(w.close_record(1).eligible_return_remaining, eligible);

    // Expiry and finalization of the close, then a second round's commit and
    // a Backer's claim, all with the deficit on the books.
    let game_before = w.pool().pot + w.pool().seed;
    finish_by_expiry(w, 1);
    assert!(w.pool().pot >= game_before, "game funds only grew (the seed carried into the pot)");
    let claimed = round2_cash_progress(w, &owner, &attacker);
    assert!(claimed > 0);
    assert_eq!(w.ops().provider_deficit, eligible, "the deficit is unchanged by commit, claims, expiry and finalization");
    assert_eq!(w.pool().round_id, 3, "both closes finalized");
    w.take_log()
}

pub fn expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later(
) -> Vec<Observation> {
    let row = ROW_EXPIRED_UNFULFILLED;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    let request = w.request_account(1);
    let (pending, retained) = request_rents();
    let client0 = w.client_lamports();

    // The deadline passes with the request unfulfilled.
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    let expired = ok(w, "", vec![expire], &[]);
    assert_eq!(
        expired.last_event::<PromotionSkipped>(),
        PromotionSkipped {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            reason: pclose::REASON_DEADLINE,
            stage: pclose::STAGE_REQUESTED_UNFULFILLED,
        }
    );
    let ops = w.ops();
    assert_eq!(ops.request_rent_outstanding, pending, "the pending rent stays outstanding");
    assert_eq!(w.lamports(request), pending, "the provider still holds the full pending rent");
    assert_eq!(w.client_lamports(), client0, "no reimbursement yet");
    assert_eq!(ops.eligible_return_budget, 0);
    // Not blocked: the close finalizes while the provider is silent.
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(w.pool().round_id, 2);

    // The provider answers late: the Client is reimbursed, the books are not.
    let (ops_before, close_before) = (w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1)));
    w.fulfill("late_fulfill", 1).expect("late authentic fulfillment");
    assert_identities(w);
    assert_eq!(w.client_lamports(), client0 + pending - retained, "the reimbursement reaches the Client only");
    assert_eq!(w.data(w.rt.ops_address()), ops_before);
    assert_eq!(w.data(w.rt.close_record_address(1)), close_before);
    assert_eq!(w.ops().request_rent_outstanding, pending, "still outstanding until observed");
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "observe_client", vec![reconcile], &[]);
    assert_eq!(w.ops().client_balance_observed, client0 + pending - retained);
    assert_eq!(w.ops().provider_deficit, 0);
    // Only the observation of the fulfillment makes it returnable.
    let withdraw = withdraw_ix(w, 1, 1);
    reject_g(w, "not_yet_eligible", withdraw, ErrorCode::NothingEligible);
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    assert_eq!(w.ops().request_rent_outstanding, 0);
    assert_eq!(w.close_record(1).eligible_return_remaining, pending - retained);
    let (free0, excess0) = (w.ops().free_balance, w.ops().unallocated_excess);
    let withdraw = withdraw_ix(w, 1, pending - retained);
    ok_g(w, "return", withdraw);
    assert_eq!(w.ops().free_balance, free0 + pending - retained);
    assert_eq!(w.ops().unallocated_excess, excess0);
    w.take_log()
}

pub fn late_fulfillment_after_expiry_becomes_eligible_only_when_observed() -> Vec<Observation> {
    let row = ROW_LATE_FULFILLMENT;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    let (pending, retained) = request_rents();

    // Not fulfilled yet: nothing to capture.
    let capture = capture_ix(w, 1);
    reject(w, "capture_pending", vec![capture], &[], ErrorCode::RequestNotFulfilled);
    // The result arrives one second late.
    w.warp_past_deadline(1);
    w.fulfill("late_fulfill", 1).expect("late fulfillment");
    assert_identities(w);
    assert_eq!(w.ops().eligible_return_budget, 0, "a fulfillment alone makes nothing eligible");
    // A late capture is a recorded no-op, not a result.
    let capture = capture_ix(w, 1);
    let captured = ok(w, "late_capture", vec![capture], &[]);
    assert!(captured.events::<DrawCaptured>().is_empty());
    let close = w.close_record(1);
    assert_eq!(close.draw_state, state::DRAW_IGNORED_LATE);
    assert_eq!((close.randomness, close.captured_at), ([0u8; 64], 0));
    // The promotion expires (a late result never promotes).
    let expire = expire_ix(w, 1);
    let expired = ok(w, "expire", vec![expire], &[]);
    assert_eq!(expired.last_event::<PromotionSkipped>().stage, pclose::STAGE_REQUESTED_UNFULFILLED);

    // Eligible only when observed.
    let withdraw = withdraw_ix(w, 1, 1);
    reject_g(w, "before_observation", withdraw, ErrorCode::NothingEligible);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);
    let observe = observe_ix(w, 1);
    ok(w, "", vec![observe], &[]);
    let close = w.close_record(1);
    assert_eq!(close.eligible_return_remaining, pending - retained);
    assert_eq!(w.ops().eligible_return_budget, pending - retained);
    let withdraw = withdraw_ix(w, 1, pending - retained);
    ok_g(w, "return", withdraw);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);
    w.take_log()
}

pub fn late_authentic_fulfillment_after_skip_cannot_mint_or_revive() -> Vec<Observation> {
    let row = ROW_LATE_AUTHENTIC;
    let mut log = Vec::new();

    // A late authentic result after the deadline skip.
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    let skipped = w.close_record(1).promotion;
    assert_eq!(
        skipped,
        state::Promotion::Skipped { reason: pclose::REASON_DEADLINE, stage: pclose::STAGE_REQUESTED_UNFULFILLED }
    );
    let (pool0, registry0) = (w.pool(), w.pool().registry_len);
    w.fulfill("", 1).expect("late authentic fulfillment");
    assert_identities(w);
    // IMPL-36 (M7-F2): a capture on an already-terminal close is refused as
    // PromotionTerminal whether it is timely or late, and writes nothing (the
    // IMPL-32 program recorded IgnoredLate here first).
    let capture = capture_ix(w, 1);
    reject(w, "late_capture", vec![capture], &[], ErrorCode::PromotionTerminal);
    let close = w.close_record(1);
    assert_eq!(close.promotion, skipped, "the skip stands");
    assert_eq!((close.randomness, close.captured_at), ([0u8; 64], 0), "no randomness was captured");
    assert_eq!(close.draw_state, state::DRAW_REQUESTED, "no draw_state write on a terminal close");
    // Nothing can be minted or revived.
    let apply = apply_ix_with(w, 1, ApplyKeys::default());
    reject(w, "apply_after_skip", vec![apply], &[], ErrorCode::PromotionTerminal);
    let expire = expire_ix(w, 1);
    reject(w, "expire_again", vec![expire], &[], ErrorCode::PromotionTerminal);
    let request = request_ix(w, 1);
    reject(w, "request_again", vec![request], &[], ErrorCode::PromotionTerminal);
    let pool = w.pool();
    assert_eq!((pool.registry_len, pool.next_sequence), (registry0, pool0.next_sequence), "no Backer was minted");
    assert!(w.account(w.rt.backer_address(1, participant(w, 1, 2))).is_none());
    assert!(!w.user(participant(w, 1, 2)).ever_b);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(w.pool().registry_len, registry0);
    log.extend(scene.w.take_log());

    // A backdated Clock cannot revive it either: the result is "timely" by
    // the clock, but the promotion is already terminal.
    let mut backdated = Scene::new(row, "backdated_clock", Spec::standard());
    backdated.provision_and_seal("build_last");
    let w = &mut backdated.w;
    do_request(w, 1, "request");
    let deadline = w.close_record(1).promotion_deadline;
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    w.fulfill("late_fulfill", 1).expect("late fulfillment");
    w.backdate_clock(deadline - 1);
    let capture = capture_ix(w, 1);
    reject(w, "backdated_capture", vec![capture], &[], ErrorCode::PromotionTerminal);
    w.restore_clock();
    assert_eq!(w.close_record(1).randomness, [0u8; 64]);
    assert_eq!(w.close_record(1).draw_state, state::DRAW_REQUESTED);
    log.extend(backdated.w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Rows: the promotion
// ---------------------------------------------------------------------------

/// `(pot, seed, management, pending_toll, funded_claims, excess)`: every game
/// partition a promotion must leave alone.
pub(crate) fn game_cash(pool: &state::Pool) -> (u64, u64, u64, u64, u128, u64) {
    (pool.pot, pool.seed, pool.management, pool.pending_toll, pool.funded_claims, pool.excess)
}

/// Install a `CandidatePage` fixture at its canonical PDA (the synthetic
/// rows only).
fn write_candidate_page(w: &mut World, close_id: u64, set_tag: u8, index: u64, rows: &[Pubkey]) {
    use anchor_lang::AccountSerialize;
    let key = candidate_page_key(close_id, set_tag, index);
    let mut page = state::CandidatePage {
        bump: pda(&[
            state::CANDIDATES_SEED,
            &close_id.to_le_bytes(),
            &[set_tag],
            &index.to_le_bytes(),
        ])
        .1,
        close_id,
        set_tag,
        page: index,
        len: rows.len() as u8,
        rows: [Pubkey::default(); state::PAGE_CAPACITY],
        rent: state::RentComponents::default(),
    };
    page.rows[..rows.len()].copy_from_slice(rows);
    let mut data = Vec::new();
    page.try_serialize(&mut data).expect("CandidatePage serializes");
    data.resize(state::CandidatePage::SPACE, 0);
    let lamports = w.rt.rent(state::CandidatePage::SPACE);
    w.rt.svm
        .set_account(key, Account { lamports, data, owner: ID, executable: false, rent_epoch: 0 })
        .expect("synthetic candidate page");
}

/// The `output_vectors` entry of `solana/fixtures/sampling.json` labelled
/// `label`: `(output, round_id, candidate_count, ordinal, counter)`.
fn sampling_vector(label: &str) -> ([u8; 64], u64, u64, u64, u64) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sampling.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is required: {error}", path.display()));
    let doc: serde_json::Value = serde_json::from_str(&text).expect("sampling.json is JSON");
    let vector = doc["output_vectors"]
        .as_array()
        .expect("output_vectors")
        .iter()
        .find(|vector| vector["label"].as_str() == Some(label))
        .unwrap_or_else(|| panic!("sampling.json has no vector labelled {label}"));
    let output: [u8; 64] = unhex(vector["output_hex"].as_str().expect("output_hex"))
        .try_into()
        .expect("64 bytes");
    (
        output,
        vector["round_id"].as_u64().expect("round_id"),
        vector["candidate_count"].as_u64().expect("candidate_count"),
        vector["ordinal"].as_u64().expect("ordinal"),
        vector["counter"].as_u64().expect("counter"),
    )
}

/// What a successful `apply_promotion` must have done for `winner` with a
/// loss of `loss`, checked against the state before it.
#[allow(clippy::too_many_arguments)]
fn assert_promotion_applied(
    w: &World,
    applied: &Observation,
    winner: Pubkey,
    loss: u128,
    close_before: &state::CloseRecord,
    pool_before: &state::Pool,
    cash_before: u64,
    vault_before: u64,
    dest: Pubkey,
    b_preexisted: bool,
) {
    let entitlement = loss * 11_600 / 10_000;
    assert_eq!(
        applied.last_event::<PromotionApplied>(),
        PromotionApplied {
            schema_version: 1,
            round_id: 1,
            close_id: Some(1),
            winner,
            loss: u64::try_from(loss).expect("loss fits u64"),
            entitlement,
        }
    );
    let b_rent = w.rt.rent(state::Backer::SPACE);
    let registry_rent = w.rt.rent(state::RegistryPage::SPACE);
    let close = w.close_record(1);
    assert_eq!(close.promotion, state::Promotion::Applied { winner, entitlement });
    assert!(close.reserve_settled);
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 0));

    // The Series B account: created from the locked reserve, 116% of the loss.
    let backer = w.backer(winner, 1);
    assert_eq!((backer.owner, backer.series), (winner, 1));
    assert_eq!(backer.entitlement, entitlement, "floor(loss * 116 / 100)");
    assert_eq!((backer.lifetime, backer.funded, backer.pending), (0, 0, 0));
    assert!(backer.active, "an open Backer joins the registry");
    assert_eq!(backer.slot, pool_before.registry_len);
    assert_eq!(backer.first_sequence, pool_before.next_sequence);
    // A winner with no Series B account gets one created from the locked
    // reserve; the closer's inactive Series B account (part of the canonical
    // fixture) is merged into instead and costs no rent.
    let created_b = if b_preexisted { 0 } else { b_rent };
    assert_eq!(backer.rent_funded_by_ops + backer.rent_funded_by_owner, created_b);
    assert_eq!(close.spent.b_rent_s + close.spent.b_rent_o, created_b);
    let user = w.user(winner);
    assert!(user.ever_b, "has_ever_b is set");
    assert_eq!(user.b_entitlement, entitlement);

    // The registry append, in a page created from the locked reserve when the
    // tail page does not exist yet.
    let pool = w.pool();
    assert_eq!(close_before.locked_owner_remaining, 0, "these closes are sponsor-funded");
    assert_eq!(pool.registry_len, pool_before.registry_len + 1);
    assert_eq!(pool.next_sequence, pool_before.next_sequence + 1);
    let creates = pool_before.registry_len / PAGE >= pool_before.registry_pages;
    assert_eq!(pool.registry_pages, pool_before.registry_pages + u64::from(creates));
    let page = w.registry(pool_before.registry_len / PAGE);
    assert_eq!(page.keys[(pool_before.registry_len % PAGE) as usize], w.rt.backer_address(1, winner));
    if creates {
        assert_eq!(close.spent.registry_rent_s, registry_rent);
        assert_eq!(close.spent.registry_rent_o, 0);
        assert_eq!(page.rent.ops, registry_rent);
        assert_eq!(page.rent.owner, 0);
        assert_eq!(page.rent.owner_refund, dest);
    }
    assert_eq!(close.spent.b_rent_o, 0);

    // No game cash moved: only the storage rents left OpsCash, and the unspent
    // sponsor partition was settled back to free cash.
    assert_eq!(game_cash(&pool), game_cash(pool_before), "pot, seed, management, toll, claims, excess untouched");
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault_before);
    w.assert_vault_identity();
    let storage = created_b + if creates { registry_rent } else { 0 };
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash_before - storage);
    let ops = w.ops();
    assert_eq!(ops.locked_remaining_total, 0);
    assert_eq!(ops.unallocated_excess, 0, "settlement parks nothing: the owner credit was deliverable");
    assert_eq!(
        ops.free_balance,
        close_before.locked_sponsor_remaining - storage,
        "the unspent sponsor partition returned to free"
    );
}

pub fn apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116() -> Vec<Observation> {
    let row = ROW_APPLY_PROMOTION;
    let mut log = Vec::new();

    // ---- A real draw: the provider's own output ------------------------------
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.through_capture();
    let w = &mut scene.w;
    let close = w.close_record(1);
    assert_eq!(close.draw_state, state::DRAW_CAPTURED);
    match w.request_state(1).state {
        orao::state::request::RequestState::Fulfilled(result) => {
            assert_eq!(result.randomness, close.randomness, "captured verbatim from the provider");
        }
        _ => panic!("the request is fulfilled"),
    }
    assert_eq!(w.ops().free_balance, 0, "promotion needs no sponsor top-up: free cash is zero");
    let expected = expected_candidates(w, 1);
    let (ordinal, counter) =
        sampling::map_output(close.randomness, close.round_id, expected.preferred.len() as u64)
            .expect("a real output maps to an ordinal");
    let (winner, loss) = expected.preferred[ordinal as usize];

    // Every winner-side account is verified: refused, fee only, nothing booked.
    let good = apply_keys(w, 1);
    assert_eq!(good.winner, Some(winner));
    let other = (0..20)
        .map(|slot| participant(w, 1, slot))
        .find(|key| *key != winner && *key != scene.owner.pubkey())
        .expect("another participant");
    type Keys = ApplyKeys;
    let cases: Vec<(&str, Keys)> = vec![
        ("omitted_winner_accounts", ApplyKeys::default()),
        ("wrong_winner", Keys { winner: Some(other), ..good }),
        ("wrong_candidate_page", Keys { candidate_page: Some(candidate_page_key(1, 1, 0)), ..good }),
        ("wrong_user", Keys { winner_user: Some(w.rt.user_address(other)), ..good }),
        ("wrong_user_round", Keys { winner_user_round: Some(w.rt.user_round_address(1, other)), ..good }),
        ("wrong_backer_b", Keys { winner_backer_b: Some(w.rt.backer_address(1, other)), ..good }),
        ("wrong_registry_tail", Keys { registry_tail: Some(w.rt.registry_address(1)), ..good }),
    ];
    for (name, keys) in cases {
        let instruction = apply_ix_with(w, 1, keys);
        reject(w, name, vec![instruction], &[], ErrorCode::WrongAccount);
    }
    assert_eq!(w.close_record(1).promotion, state::Promotion::Pending);

    let (close_before, pool_before) = (w.close_record(1), w.pool());
    let (cash_before, vault_before) = (w.lamports(w.rt.ops_cash_address()), w.lamports(w.rt.game_vault_address()));
    let b_preexisted = w.account(w.rt.backer_address(1, winner)).is_some();
    let user_rounds: Vec<Vec<u8>> = (0..20)
        .map(|slot| w.data(w.rt.user_round_address(1, participant(w, 1, slot))))
        .collect();
    let instruction = apply_ix(w, 1);
    let applied = ok(w, "", vec![instruction], &[]);
    assert_promotion_applied(w, &applied, winner, loss, &close_before, &pool_before, cash_before, vault_before, scene.dest, b_preexisted);
    for slot in 0..20u64 {
        assert_eq!(
            w.data(w.rt.user_round_address(1, participant(w, 1, slot))),
            user_rounds[slot as usize],
            "no UserRound changed"
        );
    }
    // Terminal: a second application is refused.
    let instruction = apply_ix(w, 1);
    reject(w, "replay", vec![instruction], &[], ErrorCode::PromotionTerminal);
    let note = format!(
        "real draw: ordinal {ordinal} of {} at counter {counter} (the first valid counter wins; the pinned counter-1 vector runs in the synthetic variant)",
        expected.preferred.len()
    );
    log.extend(scene.w.take_log());
    annotate(&mut log, &headline("", row), note);

    // ---- A synthetic counter-1 draw: sampling.json's pinned vector -----------
    // SYNTHETIC: counter 0 can only be rejected when the candidate count is near
    // 2^63 (limit == N), which no real page set reaches.  The close is real up
    // to the captured draw; then `randomness` is replaced by the pinned output
    // and `preferred_count` by the pinned N, and the one candidate page that
    // ordinal needs is written at its canonical PDA with the loser of
    // 300,000,000 in it.  Everything `apply_promotion` then does is the real
    // program.
    let (output, round_id, n, pinned_ordinal, pinned_counter) =
        sampling_vector("large N, counter one accepted");
    assert_eq!((round_id, pinned_counter), (1, 1));
    assert_eq!(sampling::map_output(output, 1, n), Some((pinned_ordinal, 1)));
    let word0 = {
        let digest = sampling::sha256(&[sampling::INDEX_DOMAIN, &output, &1u64.to_le_bytes(), &0u64.to_le_bytes()]);
        u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
    };
    let limit = ((1u128 << 64) / u128::from(n) * u128::from(n)) as u64;
    assert!(word0 >= limit, "counter 0 is rejected by the pinned vector");
    assert_eq!(pinned_ordinal % PAGE, 0, "the winner sits at offset 0 of its page");

    let mut synthetic = Scene::new(row, "synthetic_counter1", Spec::standard());
    synthetic.through_capture();
    let w = &mut synthetic.w;
    let winner = roster_key(6);
    assert_eq!(w.user_round(winner, 1).deposits - w.user_round(winner, 1).net_payout, 300_000_000);
    let mut record = w.close_record(1);
    record.randomness = output;
    record.preferred_count = n;
    record.selected_set_tag = 0;
    w.rt.replace_close_record(record);
    write_candidate_page(w, 1, 0, pinned_ordinal / PAGE, &[winner]);
    w.watch(candidate_page_key(1, 0, pinned_ordinal / PAGE));
    let (close_before, pool_before) = (w.close_record(1), w.pool());
    let (cash_before, vault_before) = (w.lamports(w.rt.ops_cash_address()), w.lamports(w.rt.game_vault_address()));
    let keys = apply_keys(w, 1);
    assert_eq!(keys.winner, Some(winner), "ordinal {pinned_ordinal} maps to the 300,000,000 loser");
    let b_preexisted = w.account(w.rt.backer_address(1, winner)).is_some();
    assert!(!b_preexisted, "a roster winner has no Series B account yet");
    let instruction = apply_ix(w, 1);
    let applied = ok(w, "apply", vec![instruction], &[]);
    assert_eq!(applied.last_event::<PromotionApplied>().entitlement, 348_000_000, "116% of 300,000,000");
    assert_promotion_applied(w, &applied, winner, 300_000_000, &close_before, &pool_before, cash_before, vault_before, synthetic.dest, b_preexisted);
    let mut synthetic_log = synthetic.w.take_log();
    annotate(
        &mut synthetic_log,
        &headline("synthetic_counter1.apply", row),
        format!(
            "SYNTHETIC CloseRecord: randomness = sampling.json 'large N, counter one accepted' output {}, preferred_count = {n}, one fixture candidate page at index {}; counter 0 rejected, counter 1 accepted, ordinal {pinned_ordinal}",
            hex(&output),
            pinned_ordinal / PAGE
        ),
    );
    log.extend(synthetic_log);
    log
}

pub fn apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state() -> Vec<Observation> {
    let row = ROW_ENTROPY_EXHAUSTED;
    // SYNTHETIC: no SHA-256 output can be searched for that makes all 64
    // counters fail (each fails with probability < 1/2 even at N = 2^63), and
    // sampling.json's all-rejected vector is a constructed sample-word vector,
    // not an output.  The sampler also returns None for N == 0 ("or N == 0"),
    // which production never reaches (a sealed empty set skips as
    // NoCandidates), so the labelled synthetic state is a real captured close
    // whose selected count is rewritten to 0.  The terminal transition that
    // follows is the real program.
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.through_capture();
    let w = &mut scene.w;
    let mut record = w.close_record(1);
    assert_eq!(record.draw_state, state::DRAW_CAPTURED);
    record.preferred_count = 0;
    record.selected_set_tag = 0;
    w.rt.replace_close_record(record);
    assert_eq!(sampling::map_output(w.close_record(1).randomness, 1, 0), None, "the sampler returns no ordinal");
    assert_eq!(apply_keys(w, 1).winner, None);

    let (close_before, pool_before) = (w.close_record(1), w.pool());
    let (cash_before, vault_before) = (w.lamports(w.rt.ops_cash_address()), w.lamports(w.rt.game_vault_address()));
    let ops_before = w.ops();
    let instruction = apply_ix(w, 1);
    let applied = ok(w, "", vec![instruction], &[]);
    assert_eq!(
        applied.last_event::<PromotionSkipped>(),
        PromotionSkipped {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            reason: pclose::REASON_ENTROPY_EXHAUSTED,
            stage: pclose::STAGE_SEALED,
        }
    );
    assert!(applied.events::<PromotionApplied>().is_empty());
    let close = w.close_record(1);
    assert_eq!(
        close.promotion,
        state::Promotion::Skipped { reason: pclose::REASON_ENTROPY_EXHAUSTED, stage: pclose::STAGE_SEALED }
    );
    // The terminal transition settles the reserve and mints nothing.
    assert!(close.reserve_settled);
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 0));
    assert_eq!(close.spent, close_before.spent, "no storage was needed");
    let pool = w.pool();
    assert_eq!((pool.registry_len, pool.next_sequence, pool.registry_pages), (pool_before.registry_len, pool_before.next_sequence, pool_before.registry_pages));
    assert_eq!(game_cash(&pool), game_cash(&pool_before));
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault_before);
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash_before, "no account was created and the sponsor partition stays in OpsCash");
    assert_eq!(w.ops().free_balance, ops_before.free_balance + close_before.locked_sponsor_remaining);
    assert_eq!(w.ops().locked_remaining_total, 0);
    assert_eq!(w.ops().unallocated_excess, ops_before.unallocated_excess, "settlement parks nothing");
    assert!(w.account(w.rt.backer_address(1, participant(&*w, 1, 2))).is_none(), "no Series B account");
    assert!(!w.user(participant(&*w, 1, 2)).ever_b);
    let instruction = apply_ix(w, 1);
    reject(w, "replay", vec![instruction], &[], ErrorCode::PromotionTerminal);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    let mut log = scene.w.take_log();
    annotate(
        &mut log,
        &headline("", row),
        "SYNTHETIC CloseRecord: a real captured close with preferred_count rewritten to 0 so the sampler returns no ordinal (the only reachable None); expected terminal Skipped(EntropyExhausted)".to_string(),
    );
    log
}

// ---------------------------------------------------------------------------
// Rows: expiry, capture timing, finalization
// ---------------------------------------------------------------------------

/// `expire_promotion` accepted at `stage`, with the settlement it must book.
pub(crate) fn expire_ok(w: &mut World, variant: &str, dest: Pubkey, stage: u8) -> Observation {
    let (close_before, ops_before) = (w.close_record(1), w.ops());
    let (dest_before, cash_before) = (w.lamports(dest), w.lamports(w.rt.ops_cash_address()));
    let game_before = game_cash(&w.pool());
    let expire = expire_ix(w, 1);
    let expired = ok(w, variant, vec![expire], &[]);
    assert_eq!(
        expired.last_event::<PromotionSkipped>(),
        PromotionSkipped {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            reason: pclose::REASON_DEADLINE,
            stage,
        }
    );
    let close = w.close_record(1);
    assert_eq!(close.promotion, state::Promotion::Skipped { reason: pclose::REASON_DEADLINE, stage });
    assert!(close.reserve_settled);
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 0));
    assert_eq!(close.spent, close_before.spent, "the spent ledger is retained");
    let ops = w.ops();
    let (sponsor, owner) = (close_before.locked_sponsor_remaining, close_before.locked_owner_remaining);
    assert_eq!(ops.locked_remaining_total, ops_before.locked_remaining_total - sponsor - owner);
    assert_eq!(ops.free_balance, ops_before.free_balance + sponsor, "the sponsor remainder returns to free");
    assert_eq!(w.lamports(dest), dest_before + owner, "the owner remainder is paid to the immutable destination");
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash_before - owner);
    assert_eq!(ops.unallocated_excess, ops_before.unallocated_excess, "settlement parks nothing");
    assert_eq!(game_cash(&w.pool()), game_before);
    expired
}

pub fn expire_promotion_requires_clock_past_deadline_and_no_capture() -> Vec<Observation> {
    let row = ROW_EXPIRE;
    let mut log = Vec::new();

    // Sealed, never requested.
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let dest = scene.dest;
    let w = &mut scene.w;
    let close = w.close_record(1);
    assert_eq!(close.promotion_deadline, close.close_started_at + 600);
    w.warp_to_deadline(1);
    let expire = expire_ix(w, 1);
    reject(w, "at_deadline", vec![expire], &[], ErrorCode::DeadlineNotReached);
    w.warp_past_deadline(1);
    expire_ok(w, "", dest, pclose::STAGE_UNREQUESTED);
    let expire = expire_ix(w, 1);
    reject(w, "replay", vec![expire], &[], ErrorCode::PromotionTerminal);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    log.extend(scene.w.take_log());

    // Requested, never answered.
    let mut requested = Scene::new(row, "requested_unfulfilled", Spec::standard());
    requested.provision_and_seal("build_last");
    let dest = requested.dest;
    let w = &mut requested.w;
    do_request(w, 1, "request");
    w.warp_to_deadline(1);
    let expire = expire_ix(w, 1);
    reject(w, "at_deadline", vec![expire], &[], ErrorCode::DeadlineNotReached);
    w.warp_past_deadline(1);
    expire_ok(w, "expire", dest, pclose::STAGE_REQUESTED_UNFULFILLED);
    log.extend(requested.w.take_log());

    // Fulfilled one second late: the capture is ignored, so it expires.
    let mut late = Scene::new(row, "ignored_late_capture", Spec::standard());
    late.provision_and_seal("build_last");
    let dest = late.dest;
    let w = &mut late.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    w.warp_past_deadline(1);
    let capture = capture_ix(w, 1);
    ok(w, "late_capture", vec![capture], &[]);
    assert_eq!(w.close_record(1).draw_state, state::DRAW_IGNORED_LATE);
    expire_ok(w, "expire", dest, pclose::STAGE_REQUESTED_UNFULFILLED);
    log.extend(late.w.take_log());

    // Candidate build unfinished: expiry is the abandonment.
    let mut unsealed = Scene::new(row, "candidates_incomplete", Spec::standard());
    provision_candidate_pages(&mut unsealed.w, 1);
    let dest = unsealed.dest;
    let w = &mut unsealed.w;
    let build = build_next(w, 1, 4);
    ok(w, "build0", vec![build], &[]);
    assert!(!w.close_record(1).sealed);
    w.warp_past_deadline(1);
    expire_ok(w, "expire", dest, pclose::STAGE_CANDIDATES_INCOMPLETE);
    let build = build_next(w, 1, 4);
    reject(w, "build_after_expiry", vec![build], &[], ErrorCode::PromotionTerminal);
    log.extend(unsealed.w.take_log());
    log
}

pub fn timely_capture_cannot_be_expired_and_applies_after_deadline() -> Vec<Observation> {
    let row = ROW_TIMELY_CAPTURE;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    // Captured exactly at the deadline: timely (equality is inclusive).
    w.warp_to_deadline(1);
    let deadline = w.close_record(1).promotion_deadline;
    let capture = capture_ix(w, 1);
    let captured = ok(w, "", vec![capture], &[]);
    assert_eq!(
        captured.last_event::<DrawCaptured>(),
        DrawCaptured { schema_version: 1, round_id: 1, close_id: 1, captured_at: deadline }
    );
    assert_eq!(w.close_record(1).draw_state, state::DRAW_CAPTURED);

    // One second later the capture is immutable: it cannot be expired...
    w.warp_past_deadline(1);
    let frozen = w.data(w.rt.close_record_address(1));
    let expire = expire_ix(w, 1);
    reject(w, "expire_after_capture", vec![expire], &[], ErrorCode::InvalidState);
    assert_eq!(w.data(w.rt.close_record_address(1)), frozen);
    // ... and `apply_promotion` has no deadline: it applies after it.
    let instruction = apply_ix(w, 1);
    let applied = ok(w, "apply_after_deadline", vec![instruction], &[]);
    assert!(matches!(w.close_record(1).promotion, state::Promotion::Applied { .. }));
    assert_eq!(applied.events::<PromotionApplied>().len(), 1);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    w.take_log()
}

pub fn capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop(
) -> Vec<Observation> {
    let row = ROW_CAPTURE_DEADLINE;
    let mut log = Vec::new();

    // ---- Equality: timely ----------------------------------------------------
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    let capture = capture_ix(w, 1);
    reject(w, "pending", vec![capture], &[], ErrorCode::RequestNotFulfilled);
    w.fulfill("fulfill", 1).expect("fulfill");
    w.warp_to_deadline(1);
    let deadline = w.close_record(1).promotion_deadline;
    let mut accounts = capture_accounts(w, 1);
    accounts.client = w.rt.pool_address();
    reject(w, "wrong_client", vec![anchor_ix(accounts, instruction::CaptureRoundDraw)], &[], ErrorCode::WrongProviderTuple);
    let mut accounts = capture_accounts(w, 1);
    accounts.request = w.rt.close_record_address(1);
    reject(w, "wrong_request", vec![anchor_ix(accounts, instruction::CaptureRoundDraw)], &[], ErrorCode::WrongAccount);

    let capture = capture_ix(w, 1);
    let captured = ok(w, "", vec![capture], &[]);
    let close = w.close_record(1);
    let provider_output = match w.request_state(1).state {
        orao::state::request::RequestState::Fulfilled(result) => result.randomness,
        _ => panic!("fulfilled"),
    };
    assert_eq!(close.draw_state, state::DRAW_CAPTURED);
    assert_eq!(close.randomness, provider_output, "64 bytes copied verbatim");
    assert_eq!(close.captured_at, deadline);
    assert_eq!(captured.events::<DrawCaptured>().len(), 1);
    // Idempotent after capture: a no-op, no event, nothing changes.
    let frozen = w.data(w.rt.close_record_address(1));
    let capture = capture_ix(w, 1);
    let again = ok(w, "replay", vec![capture], &[]);
    assert!(again.events::<DrawCaptured>().is_empty());
    assert_eq!(w.data(w.rt.close_record_address(1)), frozen);
    log.extend(scene.w.take_log());

    // ---- One second later: ignored, a recorded no-op -------------------------
    let mut late = Scene::new(row, "one_second_late", Spec::standard());
    late.provision_and_seal("build_last");
    let w = &mut late.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    w.warp_past_deadline(1);
    let capture = capture_ix(w, 1);
    let ignored = ok(w, "capture", vec![capture], &[]);
    assert!(ignored.events::<DrawCaptured>().is_empty(), "no result was captured");
    let close = w.close_record(1);
    assert_eq!(close.draw_state, state::DRAW_IGNORED_LATE);
    assert_eq!((close.randomness, close.captured_at), ([0u8; 64], 0));
    assert_eq!(close.promotion, state::Promotion::Pending, "the capture alone skips nothing");
    let frozen = w.data(w.rt.close_record_address(1));
    let capture = capture_ix(w, 1);
    ok(w, "replay", vec![capture], &[]);
    assert_eq!(w.data(w.rt.close_record_address(1)), frozen);
    // Nothing to apply; the deadline path is the only way out.
    let apply = apply_ix_with(w, 1, ApplyKeys::default());
    reject(w, "apply_ignored", vec![apply], &[], ErrorCode::InvalidState);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    log.extend(late.w.take_log());
    log
}

pub fn expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash() -> Vec<Observation> {
    let row = ROW_EXPIRE_DURING_BUILD;
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let mut scene = Scene::new(row, "", Spec::standard().with_backers(vec![attacker.pubkey()]));
    scene.commit_cash();
    let dest = scene.dest;
    let w = &mut scene.w;
    provision_candidate_pages(w, 1);
    for call in 0..2 {
        let build = build_next(w, 1, 4);
        ok(w, &format!("build{call}"), vec![build], &[]);
    }
    let close = w.close_record(1);
    assert_eq!((close.candidate_cursor, close.sealed), (8, false), "8 of 20 participants visited");
    let pages = (
        w.data(candidate_page_key(1, 0, 0)),
        w.data(candidate_page_key(1, 1, 0)),
    );
    let (pool_before, vault_before) = (w.pool(), w.lamports(w.rt.game_vault_address()));
    assert!(pool_before.funded_claims > 0, "the closing toll was committed and funded");

    w.warp_past_deadline(1);
    expire_ok(w, "", dest, pclose::STAGE_CANDIDATES_INCOMPLETE);
    // Abandoned, not reclaimed: the pages are still there byte for byte, and
    // the rent they cost stays spent.
    assert_eq!(w.data(candidate_page_key(1, 0, 0)), pages.0);
    assert_eq!(w.data(candidate_page_key(1, 1, 0)), pages.1);
    assert!(w.close_record(1).spent.candidate_rent_s > 0);
    // Cash is preserved: the committed allocation, seed and funded claims did
    // not move, and the Backer still claims.
    let pool = w.pool();
    assert_eq!(game_cash(&pool), game_cash(&pool_before));
    assert_eq!(w.lamports(w.rt.game_vault_address()), vault_before);
    w.assert_vault_identity();
    let claimed = w
        .claim_backer("claim", &attacker, Pubkey::new_from_array([0xE1; 32]))
        .unwrap_or_else(|failed| panic!("claim after expiry failed: {failed:?}"));
    assert!(claimed > 0);
    assert_identities(w);
    let build = build_next(w, 1, 4);
    reject(w, "build_after_expiry", vec![build], &[], ErrorCode::PromotionTerminal);
    let seed_before = w.pool().seed;
    let finalize = finalize_ix(w, 1);
    let finalized = ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(finalized.last_event::<RoundFinalized>().seed_carried, seed_before);
    w.take_log()
}

pub fn finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once(
) -> Vec<Observation> {
    let row = ROW_FINALIZE;
    let mut log = Vec::new();

    // ---- A terminal promotion does not finalize uncommitted cash --------------
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let mut ticket = Scene::new(row, "uncommitted_cash", Spec::standard().with_backers(vec![attacker.pubkey()]));
    let w = &mut ticket.w;
    assert!(!w.close_record(1).cash_committed);
    let build = build_next(w, 1, 4);
    reject(w, "build_before_commit", vec![build], &[], ErrorCode::InvalidState);
    let request = request_ix(w, 1);
    reject(w, "request_before_sealed", vec![request], &[], ErrorCode::NotSealed);
    let finalize = finalize_ix(w, 1);
    reject(w, "finalize_pending_and_uncommitted", vec![finalize], &[], ErrorCode::InvalidState);
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    assert!(matches!(w.close_record(1).promotion, state::Promotion::Skipped { .. }));
    let frozen = (w.data(w.rt.pool_address()), w.data(w.rt.close_record_address(1)));
    let finalize = finalize_ix(w, 1);
    reject(w, "finalize_terminal_but_uncommitted", vec![finalize], &[], ErrorCode::InvalidState);
    assert_eq!((w.data(w.rt.pool_address()), w.data(w.rt.close_record_address(1))), frozen);
    ticket.commit_cash();
    let w = &mut ticket.w;
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(w.pool().round_id, 2);
    log.extend(ticket.w.take_log());

    // ---- A committed close with a pending promotion is not final ---------------
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.through_capture();
    let w = &mut scene.w;
    let finalize = finalize_ix(w, 1);
    reject(w, "finalize_pending", vec![finalize], &[], ErrorCode::InvalidState);
    let instruction = apply_ix(w, 1);
    ok(w, "apply", vec![instruction], &[]);

    // Terminal + committed: the full seed carries into the pot, once.
    let before = w.pool();
    assert!(before.seed > 0, "the closing toll went to the seed");
    let finalize = finalize_ix(w, 1);
    let finalized = ok(w, "", vec![finalize], &[]);
    let after = w.pool();
    assert_eq!(
        finalized.last_event::<RoundFinalized>(),
        RoundFinalized { schema_version: 1, close_id: 1, new_round: 2, seed_carried: before.seed }
    );
    assert_eq!((after.pot, after.seed), (before.pot + before.seed, 0), "pot += seed; seed = 0");
    assert_eq!(after.round_id, before.round_id + 1);
    assert_eq!((after.participant_count, after.participant_pages), (0, 0));
    assert_eq!(after.current_close_id, None);
    assert_eq!(after.next_close_id, before.next_close_id, "next_close_id was already advanced at the close");
    assert!(w.close_record(1).finalized);
    assert!(!after.closing());
    assert_eq!(after.registry_len, before.registry_len, "the Backer promoted at apply stays");
    // The same close again is a no-op: no event, no change.
    let frozen = (w.data(w.rt.pool_address()), w.data(w.rt.close_record_address(1)));
    let again = finalize_ix(w, 1);
    let replay = ok(w, "replay", vec![again], &[]);
    assert!(replay.events::<RoundFinalized>().is_empty());
    assert_eq!((w.data(w.rt.pool_address()), w.data(w.rt.close_record_address(1))), frozen);
    log.extend(scene.w.take_log());
    log
}

/// `open_position` for `owner` against `round`'s accounts (the World's own
/// builder reads the pool's CURRENT round, which a composed transaction
/// changes mid-flight).
fn open_ix_for_round(
    w: &World,
    owner: Pubkey,
    round: u64,
    nonce: u64,
    gross: u64,
    expected_round_id: u64,
) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::OpenPosition {
            owner,
            pool: rt.pool_address(),
            admission: rt.admission_address(),
            game_vault: rt.game_vault_address(),
            user: rt.user_address(owner),
            user_round: rt.user_round_address(round, owner),
            position: rt.position_address(owner, nonce),
            config: rt.config_address(1),
            participant_page: rt.participant_address(round, 0),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::OpenPosition {
            plan: PlanKindArg::Simple,
            gross,
            client_nonce: nonce,
            expected_round_id,
            expected_config: 1,
        },
    )
}

fn participant_page_ix(w: &World, round: u64) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::ProvisionPage {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            page: rt.participant_address(round, 0),
            payer: rt.payer.pubkey(),
            close_record: None,
            system_program: system_program::ID,
        },
        instruction::ProvisionPage {
            kind: PageKind::Participant { round },
            index: 0,
            funding: Funding::Free,
        },
    )
}

pub fn direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round(
) -> Vec<Observation> {
    let row = ROW_COMPOSED;
    let mut log = Vec::new();
    let mut scene = Scene::new(row, "", Spec::standard());
    let owner = scene.owner.insecure_clone();
    let owner_key = owner.pubkey();
    let w = &mut scene.w;

    // A direct deposit against the Closing round is refused.
    let direct = w.open_instruction(owner_key, PlanKindArg::Simple, 100_000_000, 5, 1, 1);
    reject(w, "direct_open", vec![direct], &[&owner], ErrorCode::Closing);

    // The promotion can only end by deadline here (nothing was requested).
    w.warp_past_deadline(1);
    let frozen = (
        w.data(w.rt.pool_address()),
        w.data(w.rt.ops_address()),
        w.data(w.rt.close_record_address(1)),
    );
    // A stale expected round makes the WHOLE composed transaction revert,
    // including the expiry and the finalization that ran before the open.
    let stale = vec![
        expire_ix(w, 1),
        finalize_ix(w, 1),
        open_ix_for_round(w, owner_key, 2, 5, 100_000_000, 1),
    ];
    reject(w, "stale_expected_round", stale, &[&owner], ErrorCode::StaleRound);
    assert_eq!(
        (w.data(w.rt.pool_address()), w.data(w.rt.ops_address()), w.data(w.rt.close_record_address(1))),
        frozen,
        "atomic: expire and finalize reverted with the failed open"
    );
    assert_eq!(w.close_record(1).promotion, state::Promotion::Pending);

    // expire -> finalize -> (provision the new round's first participant page,
    // which exists only for the pool's CURRENT round) -> open(expected round 2).
    let composed = vec![
        expire_ix(w, 1),
        finalize_ix(w, 1),
        participant_page_ix(w, 2),
        open_ix_for_round(w, owner_key, 2, 5, 100_000_000, 2),
    ];
    let done = ok(w, "", composed, &[&owner]);
    assert_eq!(
        done.last_event::<PromotionSkipped>(),
        PromotionSkipped {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            reason: pclose::REASON_DEADLINE,
            stage: pclose::STAGE_CANDIDATES_INCOMPLETE,
        }
    );
    assert_eq!(done.last_event::<RoundFinalized>().new_round, 2);
    let opened = done.last_event::<PositionOpened>();
    assert_eq!((opened.owner, opened.nonce, opened.round_id, opened.gross), (owner_key, 5, 2, 100_000_000));
    let pool = w.pool();
    assert_eq!((pool.round_id, pool.participant_count, pool.participant_pages), (2, 1, 1));
    assert_eq!(pool.current_close_id, None);
    assert_eq!(w.position(owner_key, 5).round_id, 2);
    assert_eq!(w.user_round(owner_key, 2).participant_slot, 0, "round 2's first participant index is 0");
    assert!(w.close_record(1).finalized);
    log.extend(scene.w.take_log());

    // The plan's literal three-instruction form, when the page already exists.
    let mut literal = Scene::new(row, "three_instructions", Spec::standard());
    let owner = literal.owner.insecure_clone();
    let owner_key = owner.pubkey();
    let w = &mut literal.w;
    w.warp_past_deadline(1);
    w.rt.replace_participant_page(state::ParticipantPage {
        bump: pda(&[state::PARTICIPANTS_SEED, &2u64.to_le_bytes(), &0u64.to_le_bytes()]).1,
        round_id: 2,
        page: 0,
        ..state::ParticipantPage::default()
    });
    w.watch(w.rt.participant_address(2, 0));
    let composed = vec![
        expire_ix(w, 1),
        finalize_ix(w, 1),
        open_ix_for_round(w, owner_key, 2, 5, 100_000_000, 2),
    ];
    ok(w, "composed", composed, &[&owner]);
    assert_eq!((w.pool().round_id, w.pool().participant_count), (2, 1));
    assert_eq!(w.position(owner_key, 5).round_id, 2);
    log.extend(literal.w.take_log());
    log
}

pub fn two_closes_two_rounds_sequence() -> Vec<Observation> {
    let row = ROW_TWO_CLOSES_SEQUENCE;
    let mut scene = Scene::new(row, "", Spec::standard());
    let owner = scene.owner.insecure_clone();
    let dest1 = scene.dest;
    scene.through_capture();
    let w = &mut scene.w;
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    let finalize = finalize_ix(w, 1);
    let finalized = ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(finalized.last_event::<RoundFinalized>().new_round, 2);

    // F2: round 1 closed as close 1 and reopened as round 2.
    let pool = w.pool();
    assert_eq!(
        (pool.round_id, pool.participant_count, pool.participant_pages, pool.current_close_id, pool.next_close_id),
        (2, 0, 0, None, 2)
    );
    let first = w.close_record(1);
    assert!(first.finalized);
    assert_eq!(first.owner_refund_destination, dest1);
    let round1_page = w.data(w.rt.participant_address(1, 0));
    assert!(!round1_page.is_empty(), "round 1's participant pages stay, bound to their round seed");
    assert_eq!(w.participants(1, 0).round_id, 1);

    // Round 2: its own participants from index 0, its own CloseRecord(2), its
    // own destination.  The promoted Backer of round 1 is in the registry, so
    // this toll rides a closing ticket.
    let dest2 = Pubkey::new_from_array([0xDF; 32]);
    let second = start_second_close(w, &owner, 2, dest2, &[], 3);
    assert_eq!(w.user_round(owner.pubkey(), 2).participant_slot, 0, "round 2's first participant index is 0");
    assert_eq!(w.participants(2, 0).rows[0], owner.pubkey());
    assert_eq!(w.data(w.rt.participant_address(1, 0)), round1_page, "round 1's page was not touched");
    assert_eq!(second.payout.close_id, Some(2));
    let record2 = w.close_record(2);
    assert_eq!(record2.owner_refund_destination, dest2);
    assert_ne!(record2.owner_refund_destination, dest1);
    assert_eq!(record2.promotion_deadline, record2.close_started_at + 600);
    assert!(record2.closing_ticket_id.is_some(), "the registry is not empty any more");
    w.advance_until_idle(8);
    assert_identities(w);
    assert!(w.close_record(2).cash_committed);

    provision_candidate_pages(w, 2);
    seal_labelled(w, 2, 4, "build_last");
    let (request2, seed2) = host_request(w, 2);
    assert_ne!(seed2, first.protocol_seed);
    assert_ne!(request2, first.request_account, "a distinct request binding");
    do_request(w, 2, &lbl(2, "request"));
    w.fulfill(&lbl(2, "fulfill"), 2).expect("fulfill");
    let capture = capture_ix(w, 2);
    ok(w, &lbl(2, "capture"), vec![capture], &[]);
    let apply = apply_ix(w, 2);
    ok(w, &lbl(2, "apply"), vec![apply], &[]);
    let finalize = finalize_ix(w, 2);
    let finalized = ok(w, "", vec![finalize], &[]);
    let event = finalized.last_event::<RoundFinalized>();
    assert_eq!((event.close_id, event.new_round), (2, 3));

    // Each close kept its own destination and binding through the other.
    let (first_after, second_after) = (w.close_record(1), w.close_record(2));
    assert_eq!(first_after.owner_refund_destination, dest1);
    assert_eq!(second_after.owner_refund_destination, dest2);
    assert_eq!(first_after.request_account, first.request_account);
    assert_eq!(first_after.protocol_seed, first.protocol_seed);
    assert_eq!(second_after.request_account, request2);
    assert!(first_after.finalized && second_after.finalized);
    let pool = w.pool();
    assert_eq!((pool.round_id, pool.next_close_id, pool.current_close_id), (3, 3, None));
    w.take_log()
}

// ---------------------------------------------------------------------------
// Rows: budget, settlement, provider validation, deadlines, collisions
// ---------------------------------------------------------------------------

impl World {
    /// Fixture write: the live ORAO `request_fee` (a provider price change).
    pub fn set_provider_fee(&mut self, fee: u64) {
        use anchor_lang::AccountSerialize;
        let key = provider_keys().network_state;
        let mut network: orao::state::network_state::NetworkState = self.rt.deserialize(key);
        network.config.request_fee = fee;
        let mut bytes = Vec::new();
        network.try_serialize(&mut bytes).expect("NetworkState serializes");
        tamper(self, key, |account| account.data[..bytes.len()].copy_from_slice(&bytes));
    }
}

/// `increase_provider_budget` etc. that must keep `L == sum of partitions`:
/// the change of `ops.locked_remaining_total` across a step.
fn step(
    w: &mut World,
    table: &mut Vec<serde_json::Value>,
    variant: &str,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
    expected_dl: i128,
) -> Observation {
    let before = w.ops().locked_remaining_total;
    let observation = ok(w, variant, instructions, signers);
    record_step(w, table, variant, before, expected_dl);
    observation
}

fn step_g(
    w: &mut World,
    table: &mut Vec<serde_json::Value>,
    variant: &str,
    inner: Instruction,
    expected_dl: i128,
) -> Observation {
    let before = w.ops().locked_remaining_total;
    let observation = ok_g(w, variant, inner);
    record_step(w, table, variant, before, expected_dl);
    observation
}

fn record_step(w: &World, table: &mut Vec<serde_json::Value>, variant: &str, before: u64, expected_dl: i128) {
    let ops = w.ops();
    let close = w.close_record(1);
    let (s, o) = (close.locked_sponsor_remaining, close.locked_owner_remaining);
    assert_eq!(
        i128::from(ops.locked_remaining_total) - i128::from(before),
        expected_dl,
        "change of L at `{variant}` (spec 8.6)"
    );
    assert_eq!(ops.locked_remaining_total, s + o, "L == s + o after `{variant}`");
    table.push(serde_json::json!({
        "step": variant,
        "L": ops.locked_remaining_total,
        "sponsor_remaining": s,
        "owner_remaining": o,
        "free": ops.free_balance,
        "unallocated_excess": ops.unallocated_excess,
    }));
}

pub fn locked_remaining_total_aggregate_equals_sum_of_remaining_partitions_after_every_transition(
) -> Vec<Observation> {
    let row = ROW_PARTITION_AGGREGATE;
    let mut table: Vec<serde_json::Value> = Vec::new();
    // Owner-funded, so both partitions move; the table is spec 8.6.
    let spec = Spec::standard().with_sponsor(sponsor_for_owner_rent(173_520));
    let mut scene = Scene::new(row, "", spec);
    let w = &mut scene.w;
    let reserve = scene.reserve;
    record_step(w, &mut table, "closing_payout", 0, i128::from(reserve));
    assert_eq!(w.ops().locked_remaining_total, reserve);

    let page = w.rt.rent(state::CandidatePage::SPACE) as i128;
    let expected = expected_candidates(w, 1);
    assert_eq!((pages_of(expected.preferred.len()), pages_of(expected.fallback.len())), (1, 1));
    let instruction = provision_ix(w, 1, 0, 0, Funding::LockedClose);
    step(w, &mut table, "provision_preferred0", vec![instruction], &[], -page);
    let instruction = provision_ix(w, 1, 1, 0, Funding::LockedClose);
    step(w, &mut table, "provision_fallback0", vec![instruction], &[], -page);
    for call in 0..5 {
        let last = call == 4;
        let build = build_next(w, 1, 4);
        step(w, &mut table, &format!("build{call}"), vec![build], &[], 0);
        assert_eq!(w.close_record(1).sealed, last);
    }

    // A budget increase moves free cash INTO the locked sponsor partition.
    w.set_ops_free(1_000_000);
    let extra = 100_000u64;
    let before = (w.ops().free_balance, w.close_record(1).locked_sponsor_remaining);
    let budget = budget_ix(w, 1, extra);
    step(w, &mut table, "increase_provider_budget", vec![budget], &[], i128::from(extra));
    assert_eq!(w.ops().free_balance, before.0 - extra);
    assert_eq!(w.close_record(1).locked_sponsor_remaining, before.1 + extra);

    // The request spends fee + rent out of the partitions.
    let (rent, retained) = request_rents();
    let request_ix_now = request_ix(w, 1);
    w.watch(host_request(w, 1).0);
    step(w, &mut table, "request", vec![request_ix_now], &[], -((REQUEST_FEE + rent) as i128));
    // Provider and capture steps move no locked cash.
    let fulfilled = w.fulfill("fulfill", 1).expect("fulfill");
    assert_identities(w);
    assert_eq!(delta(&fulfilled, w.rt.ops_cash_address()), 0);
    let capture = capture_ix(w, 1);
    step(w, &mut table, "capture", vec![capture], &[], 0);
    let observe = observe_ix(w, 1);
    step(w, &mut table, "observe", vec![observe], &[], 0);

    // Promotion: the two storage rents, then settlement of everything left.
    let l_before = i128::from(w.ops().locked_remaining_total);
    let apply = apply_ix(w, 1);
    step(w, &mut table, "apply_and_settle", vec![apply], &[], -l_before);
    assert_eq!(w.ops().locked_remaining_total, 0);
    let eligible = w.close_record(1).eligible_return_remaining;
    assert_eq!(eligible, rent - retained);
    let withdraw = withdraw_ix(w, 1, eligible);
    step_g(w, &mut table, "", withdraw, 0);
    let finalize = finalize_ix(w, 1);
    step(w, &mut table, "finalize", vec![finalize], &[], 0);
    let mut log = scene.w.take_log();
    annotate(
        &mut log,
        &headline("", row),
        format!("partition_table: {}", serde_json::Value::Array(table)),
    );
    log
}

pub fn increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once(
) -> Vec<Observation> {
    let row = ROW_BUDGET;
    let mut log = Vec::new();
    let (rent, _) = request_rents();

    // ---- The live fee rises above the cap -------------------------------------
    let spec = Spec::standard().with_sponsor(sponsor_for_owner_rent(173_520));
    let mut scene = Scene::new(row, "", spec);
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    w.set_ops_free(2_000_000);
    let close0 = w.close_record(1);
    assert_eq!(close0.approved_provider_fee_cap, REQUEST_FEE + REQUEST_FEE * 2_000 / 10_000, "fee + 20% headroom");
    w.set_provider_fee(400_000);
    let request = request_ix(w, 1);
    reject(w, "fee_above_cap", vec![request], &[], ErrorCode::ProviderFeeExceeded);
    assert_eq!(w.close_record(1).draw_state, state::DRAW_NOT_REQUESTED);

    // Refused budget calls change nothing.
    let zero = budget_ix(w, 1, 0);
    reject(w, "zero_extra", vec![zero], &[], ErrorCode::InvalidState);
    let too_much = budget_ix(w, 1, 2_000_001);
    reject(w, "more_than_free", vec![too_much], &[], ErrorCode::InsufficientOperations);

    // Move 150,000 from free into the sponsor partition and the cap.
    let extra = 150_000u64;
    let (ops0, close0) = (w.ops(), w.close_record(1));
    let cash0 = w.lamports(w.rt.ops_cash_address());
    let budget = budget_ix(w, 1, extra);
    ok(w, "", vec![budget], &[]);
    let (ops, close) = (w.ops(), w.close_record(1));
    assert_eq!(ops.free_balance, ops0.free_balance - extra);
    assert_eq!(close.locked_sponsor_remaining, close0.locked_sponsor_remaining + extra);
    assert_eq!(close.locked_owner_remaining, close0.locked_owner_remaining, "the owner quote never rises implicitly");
    assert_eq!(close.approved_provider_fee_cap, close0.approved_provider_fee_cap + extra);
    assert_eq!(close.required_reserve, close0.required_reserve + extra);
    assert_eq!(ops.locked_remaining_total, ops0.locked_remaining_total + extra);
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash0, "an internal reclassification: no lamports moved");

    // The request now succeeds at the live fee, sponsor first.
    let spend = pclose::plan_request_spend(
        close.locked_sponsor_remaining,
        close.locked_owner_remaining,
        400_000,
        rent,
        pclose::promotion_holdback(&close).expect("holdback"),
    )
    .expect("affordable after the increase");
    let requested = do_request(w, 1, "request");
    assert_eq!(requested.last_event::<DrawRequested>().fee_paid, 400_000);
    let close = w.close_record(1);
    assert_eq!(close.spent.provider_fee_s, spend.fee_s);
    assert_eq!(close.spent.provider_fee_o, spend.fee_o);
    assert_eq!((close.spent.request_rent_s, close.spent.request_rent_o), (spend.rent_s, spend.rent_o));
    assert_eq!(w.ops().provider_fee_consumed_total, 400_000);
    // A budget increase after the request is refused (only while NotRequested).
    let budget = budget_ix(w, 1, 1);
    reject(w, "after_request", vec![budget], &[], ErrorCode::InvalidState);

    // The rest of the lifecycle settles whatever is unspent, once.
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let (before, ops_before) = (w.close_record(1), w.ops());
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    let after = w.close_record(1);
    let storage = (after.spent.b_rent_s + after.spent.b_rent_o + after.spent.registry_rent_s + after.spent.registry_rent_o)
        - (before.spent.b_rent_s + before.spent.b_rent_o + before.spent.registry_rent_s + before.spent.registry_rent_o);
    let sponsor_storage = (after.spent.b_rent_s + after.spent.registry_rent_s)
        - (before.spent.b_rent_s + before.spent.registry_rent_s);
    assert!(storage > 0);
    assert_eq!(
        w.ops().free_balance,
        ops_before.free_balance + before.locked_sponsor_remaining - sponsor_storage,
        "the unspent sponsor partition (extra included) returned to free exactly once"
    );
    let again = apply_ix_with(w, 1, ApplyKeys::default());
    reject(w, "settle_again", vec![again], &[], ErrorCode::PromotionTerminal);
    log.extend(scene.w.take_log());

    // ---- Unused extra, never requested: it comes back exactly once ---------------
    let mut unused = Scene::new(row, "unused_extra", Spec::standard().with_spare(1_000_000));
    unused.provision_and_seal("build_last");
    let dest = unused.dest;
    let w = &mut unused.w;
    let (ops0, close0) = (w.ops(), w.close_record(1));
    assert_eq!(ops0.free_balance, 1_000_000);
    let budget = budget_ix(w, 1, extra);
    ok(w, "budget", vec![budget], &[]);
    w.warp_past_deadline(1);
    let expired = expire_ok(w, "expire", dest, pclose::STAGE_UNREQUESTED);
    let _ = expired;
    assert_eq!(
        w.ops().free_balance,
        ops0.free_balance + close0.locked_sponsor_remaining,
        "free cash ends where it started plus the whole sponsor partition: the extra returned once, nothing doubled"
    );
    log.extend(unused.w.take_log());

    // ---- The deadline closes the budget window ------------------------------------
    let mut late = Scene::new(row, "after_deadline", Spec::standard().with_spare(1_000_000));
    late.provision_and_seal("build_last");
    let w = &mut late.w;
    w.warp_to_deadline(1);
    let budget = budget_ix(w, 1, extra);
    ok(w, "at_deadline", vec![budget], &[]);
    w.warp_past_deadline(1);
    let budget = budget_ix(w, 1, extra);
    reject(w, "past_deadline", vec![budget], &[], ErrorCode::DeadlinePassed);
    log.extend(late.w.take_log());
    log
}

/// The reserve of the standard 20-participant, empty-registry round, from the
/// rent schedule alone (cross-checked against the program's own figure).
pub(crate) fn standard_reserve() -> u64 {
    6 * rent_of(state::CandidatePage::SPACE)
        + rent_of(state::RegistryPage::SPACE)
        + rent_of(state::Backer::SPACE)
        + rent_of(759)
        + REQUEST_FEE
        + REQUEST_FEE * 2_000 / 10_000
}

pub fn settlement_refunds_only_remaining_partitions_never_spent_amounts() -> Vec<Observation> {
    let row = ROW_SETTLEMENT;
    let mut log = Vec::new();
    let page = rent_of(state::CandidatePage::SPACE);

    // ---- Settlement at expiry: the pages' rent stays spent ----------------------
    let sponsor = 2 * page + 1_000_000;
    let mut scene = Scene::new(row, "", Spec::standard().with_sponsor(sponsor));
    assert_eq!(scene.reserve, standard_reserve());
    scene.provision_and_seal("build_last");
    let dest = scene.dest;
    let w = &mut scene.w;
    let close = w.close_record(1);
    assert_eq!(close.locked_sponsor_remaining, 1_000_000);
    assert_eq!(close.locked_owner_remaining, scene.reserve - sponsor, "the owner opted in for the rest");
    assert_eq!(close.spent.candidate_rent_s, 2 * page);
    w.warp_past_deadline(1);
    expire_ok(w, "", dest, pclose::STAGE_UNREQUESTED);
    let close = w.close_record(1);
    assert_eq!(close.spent.candidate_rent_s, 2 * page, "the spent ledger kept the pages' rent");
    assert_eq!(w.ops().locked_remaining_total, 0);
    log.extend(scene.w.take_log());

    // ---- Settlement after a request and a promotion ---------------------------------
    let mut full = Scene::new(row, "after_promotion", Spec::standard().with_sponsor(sponsor_for_owner_rent(173_520)));
    full.through_capture();
    let dest = full.dest;
    let w = &mut full.w;
    let (before, ops_before) = (w.close_record(1), w.ops());
    let (cash_before, dest_before) = (w.lamports(w.rt.ops_cash_address()), w.lamports(dest));
    assert_eq!(before.locked_sponsor_remaining, 0, "the request drained the sponsor partition");
    assert!(before.spent.provider_fee_s > 0 && before.spent.request_rent_s > 0);
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    let after = w.close_record(1);
    let b_and_registry = (after.spent.b_rent_s + after.spent.b_rent_o + after.spent.registry_rent_s + after.spent.registry_rent_o)
        - (before.spent.b_rent_s + before.spent.b_rent_o + before.spent.registry_rent_s + before.spent.registry_rent_o);
    let owner_remainder = before.locked_owner_remaining - b_and_registry;
    assert_eq!(w.lamports(dest), dest_before + owner_remainder, "only the owner's UNSPENT partition is refunded");
    assert_eq!(w.lamports(w.rt.ops_cash_address()), cash_before - b_and_registry - owner_remainder);
    assert_eq!(w.ops().free_balance, ops_before.free_balance, "no sponsor remainder: all was spent");
    assert_eq!(w.ops().unallocated_excess, ops_before.unallocated_excess);
    assert_eq!(after.spent.provider_fee_s, before.spent.provider_fee_s, "the fee is gone for good");
    assert_eq!(after.spent.request_rent_o, before.spent.request_rent_o);
    log.extend(full.w.take_log());

    // ---- Undeliverable owner destination: DEFERRED to the owner (IMPL-43) ----------------
    // The owner remainder (500 lamports, below the rent-exempt minimum) cannot
    // be credited to a wallet that no longer exists (zero lamports).  Expiry
    // still succeeds; the remainder stays owed on the CloseRecord
    // (`locked_owner_remaining`, counted in `locked_remaining_total`) and is
    // never converted into anonymous excess.  Delivery itself is covered by
    // `tests/owner_refund.rs`; this row pins the settlement side in the full
    // close lifecycle.
    let reserve = standard_reserve();
    let mut parked = Scene::new(row, "undeliverable-destination", Spec::standard().with_sponsor(reserve - 500));
    assert_eq!(parked.reserve, reserve);
    provision_candidate_pages(&mut parked.w, 1);
    let dest = parked.dest;
    let w = &mut parked.w;
    w.rt.remove_account(dest);
    w.warp_past_deadline(1);
    let (before, ops_before) = (w.close_record(1), w.ops());
    let expire = expire_ix(w, 1);
    let outcome = w.send("", vec![expire], &[]);
    let (close, ops) = (w.close_record(1), w.ops());
    assert!(outcome.is_ok(), "expiry never depends on the state of the owner destination: {outcome:?}");
    assert_eq!(close.locked_owner_remaining, before.locked_owner_remaining, "the owner remainder is deferred, still owed on the CloseRecord");
    assert!(close.reserve_settled, "settlement completes");
    assert_eq!(ops.unallocated_excess, ops_before.unallocated_excess, "owner lamports never become anonymous excess");
    assert_eq!(ops.locked_remaining_total, close.locked_owner_remaining, "only the deferred owner refund stays locked");
    assert_eq!(ops.free_balance, ops_before.free_balance + before.locked_sponsor_remaining, "the sponsor remainder is released");
    assert_eq!(w.lamports(dest), 0, "nothing was pushed at a destination that cannot receive it");
    let note = serde_json::json!({
        "expected_behaviour": "IMPL-43: deferred to the owner; deliver_owner_refund pays it once the destination can receive the credit",
        "owner_remainder_before": before.locked_owner_remaining,
        "destination_lamports_before": 0,
        "destination_lamports_after": w.lamports(dest),
        "transaction_succeeded": outcome.is_ok(),
        "locked_owner_remaining_after": close.locked_owner_remaining,
        "reserve_settled_after": close.reserve_settled,
        "unallocated_excess_before_after": [ops_before.unallocated_excess, ops.unallocated_excess],
        "locked_remaining_total_before_after": [ops_before.locked_remaining_total, ops.locked_remaining_total],
        "free_balance_before_after": [ops_before.free_balance, ops.free_balance],
    });
    w.assert_ops_cash_identity();
    let mut parked_log = parked.w.take_log();
    annotate(
        &mut parked_log,
        &headline("undeliverable-destination", row),
        format!("undeliverable_destination (IMPL-43 deferral asserted): {note}"),
    );
    log.extend(parked_log);
    log
}

pub fn anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent() -> Vec<Observation> {
    let row = ROW_ANONYMOUS_CREDIT;
    let mut scene = Scene::new(row, "", Spec::standard());
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    let cash = w.rt.ops_cash_address();
    let credit = 5_000_000u64;
    let (ops0, reserve) = (w.ops(), scene.reserve);
    assert_eq!(ops0.free_balance, 0);

    // An anonymous credit, reconciled in the same transaction.
    let transfer = solana_system_interface::instruction::transfer(&attacker.pubkey(), &cash, credit);
    let reconcile = reconcile_ix(&w.rt);
    ok(w, "", vec![transfer, reconcile], &[&attacker]);
    let ops = w.ops();
    assert_eq!(ops.unallocated_excess, ops0.unallocated_excess + credit);
    assert_eq!((ops.free_balance, ops.locked_remaining_total), (0, reserve), "neither free nor locked");

    // It cannot fund anything: free cash is zero, the lamports are not free.
    let page = provision_ix(w, 1, 0, 0, Funding::Free);
    reject(w, "cannot_fund_a_free_page", vec![page], &[], ErrorCode::InsufficientOperations);
    let excess = ops.unallocated_excess;

    // The close runs on its partitions alone and never touches the excess.
    provision_candidate_pages(w, 1);
    seal_labelled(w, 1, 4, "build_last");
    assert_eq!(w.ops().unallocated_excess, excess);
    let (cash0, ops0) = (w.lamports(cash), w.ops());
    let requested = do_request(w, 1, "request");
    let (rent, _) = request_rents();
    assert_eq!(delta(&requested, cash), -((REQUEST_FEE + rent) as i128), "exactly fee + rent left OpsCash");
    assert_eq!(w.lamports(cash), cash0 - REQUEST_FEE - rent);
    assert_eq!(w.ops().unallocated_excess, excess);
    assert_eq!(w.ops().locked_remaining_total, ops0.locked_remaining_total - REQUEST_FEE - rent);
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    let ops = w.ops();
    assert_eq!(ops.unallocated_excess, excess, "still segregated after settlement");
    assert_eq!(ops.locked_remaining_total, 0);
    // free == the reserve less everything spent; the credit is not in it.
    let close = w.close_record(1);
    let spent = close.spent;
    let total_spent = spent.candidate_rent_s + spent.candidate_rent_o + spent.registry_rent_s + spent.registry_rent_o
        + spent.b_rent_s + spent.b_rent_o + spent.provider_fee_s + spent.provider_fee_o
        + spent.request_rent_s + spent.request_rent_o;
    assert_eq!(ops.free_balance, reserve - total_spent);
    assert_eq!(w.lamports(cash) - w.rt.rent(0), ops.free_balance + ops.unallocated_excess);
    w.take_log()
}

pub fn request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner() -> Vec<Observation> {
    let row = ROW_REQUEST_REJECTS_PROVIDER;
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let keys = provider_keys();
    w.watch(host_request(w, 1).0);
    let send = |w: &mut World, name: &str, accounts: accounts::RequestRoundDraw, expected: ErrorCode| {
        reject(w, name, vec![anchor_ix(accounts, instruction::RequestRoundDraw)], &[], expected)
    };

    // Account keys that differ from the pinned tuple.
    type Tamper = fn(&mut accounts::RequestRoundDraw);
    let keyed: Vec<(&str, Tamper, ErrorCode)> = vec![
        ("wrong_client_key", |a| a.client = Pubkey::new_from_array([0x94; 32]), ErrorCode::WrongProviderTuple),
        ("wrong_treasury", |a| a.treasury = Pubkey::new_from_array([0x94; 32]), ErrorCode::WrongProviderTuple),
        ("wrong_network_state", |a| a.network_state = Pubkey::new_from_array([0x94; 32]), ErrorCode::WrongProviderTuple),
        ("wrong_provider_program", |a| a.provider_program = system_program::ID, ErrorCode::WrongProviderTuple),
        ("wrong_request_account", |a| a.request = Pubkey::new_from_array([0x94; 32]), ErrorCode::WrongAccount),
        ("wrong_ops_cash", |a| a.ops_cash = Pubkey::new_from_array([0x94; 32]), ErrorCode::WrongAccount),
    ];
    for (index, (name, change, expected)) in keyed.into_iter().enumerate() {
        let mut accounts = request_accounts(w, 1);
        change(&mut accounts);
        let variant = if index == 0 { "" } else { name };
        send(w, variant, accounts, expected);
    }

    // R5: the registered Client must be G-owned, signed by this program's
    // RngAuthority, registered for this program, and owned by the provider.
    let original = tamper_client(w, |client| client.owner = Pubkey::new_from_array([0x94; 32]));
    send(w, "client_owner_is_not_g", request_accounts(w, 1), ErrorCode::WrongProviderTuple);
    restore(w, keys.client, original);
    let original = tamper_client(w, |client| client.state = Pubkey::new_from_array([0x94; 32]));
    send(w, "client_state_is_not_rng_authority", request_accounts(w, 1), ErrorCode::WrongProviderTuple);
    restore(w, keys.client, original);
    let original = tamper_client(w, |client| client.program = Pubkey::new_from_array([0x94; 32]));
    send(w, "client_registered_for_another_program", request_accounts(w, 1), ErrorCode::WrongProviderTuple);
    restore(w, keys.client, original);
    let original = tamper(w, keys.client, |account| account.owner = system_program::ID);
    send(w, "client_not_owned_by_the_provider", request_accounts(w, 1), ErrorCode::WrongOwner);
    restore(w, keys.client, original);
    let original = tamper(w, keys.network_state, |account| account.owner = system_program::ID);
    send(w, "network_state_not_owned_by_the_provider", request_accounts(w, 1), ErrorCode::WrongOwner);
    restore(w, keys.network_state, original);
    // The RngAuthority is bound to a different client.
    let (_, rng_bump) = provider::rng_authority_address();
    w.rt.replace_rng_authority(state::RngAuthority { bump: rng_bump, client: Pubkey::new_from_array([0x94; 32]) });
    send(w, "rng_authority_bound_elsewhere", request_accounts(w, 1), ErrorCode::WrongProviderTuple);
    w.rt.replace_rng_authority(state::RngAuthority { bump: rng_bump, client: keys.client });
    // The config account must be the close's pinned version.
    let decoy = decoy_config(w);
    let mut accounts = request_accounts(w, 1);
    accounts.config = decoy;
    reject_framework(w, "wrong_config_version", vec![anchor_ix(accounts, instruction::RequestRoundDraw)], &[], 2006);

    // Every tampering was restored: the honest request goes through.
    assert_eq!(w.close_record(1).draw_state, state::DRAW_NOT_REQUESTED);
    do_request(w, 1, "request");
    assert_eq!(w.close_record(1).draw_state, state::DRAW_REQUESTED);
    w.take_log()
}

/// Rewrite the registered ORAO `Client` in place, returning the account as it
/// was.
pub(crate) fn tamper_client(w: &mut World, change: impl FnOnce(&mut orao::state::client::Client)) -> Account {
    use anchor_lang::AccountSerialize;
    let key = provider_keys().client;
    let mut client: orao::state::client::Client = w.rt.deserialize(key);
    change(&mut client);
    let mut bytes = Vec::new();
    client.try_serialize(&mut bytes).expect("Client serializes");
    tamper(w, key, |account| account.data[..bytes.len()].copy_from_slice(&bytes))
}

pub fn owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution(
) -> Vec<Observation> {
    let row = ROW_OWNER_FALLBACK;
    let mut log = Vec::new();
    let (rent, retained) = request_rents();
    let page = rent_of(state::CandidatePage::SPACE);

    // The sponsor partition runs out INSIDE the fee: the owner funds the rest
    // of the fee and all of the rent.
    let mut scene = Scene::new(row, "", Spec::standard().with_sponsor(2 * page + 100_000));
    scene.provision_and_seal("build_last");
    let dest = scene.dest;
    let w = &mut scene.w;
    let before = w.close_record(1);
    assert_eq!(before.locked_sponsor_remaining, 100_000);
    let spend = pclose::plan_request_spend(
        before.locked_sponsor_remaining,
        before.locked_owner_remaining,
        REQUEST_FEE,
        rent,
        pclose::promotion_holdback(&before).expect("holdback"),
    )
    .expect("affordable");
    assert_eq!(
        spend,
        pclose::RequestSpend { fee_s: 100_000, fee_o: REQUEST_FEE - 100_000, rent_s: 0, rent_o: rent }
    );
    let dest0 = w.lamports(dest);
    do_request(w, 1, "");
    let after = w.close_record(1);
    assert_eq!((after.spent.provider_fee_s, after.spent.provider_fee_o), (100_000, 200_000), "exact fee attribution");
    assert_eq!((after.spent.request_rent_s, after.spent.request_rent_o), (0, rent), "the owner funded all of the rent");
    assert_eq!((after.request_rent_funded_s, after.request_rent_funded_o), (0, rent));
    assert_eq!(after.locked_sponsor_remaining, 0);
    assert_eq!(after.locked_owner_remaining, before.locked_owner_remaining - 200_000 - rent);
    assert_eq!(w.ops().provider_fee_consumed_total, u128::from(REQUEST_FEE), "the whole fee is consumed");

    // Owner-funded rent is refundable ONLY through withdraw_provider_return
    // after observe_request_fulfilled; the owner-funded fee never is.
    w.fulfill("fulfill", 1).expect("fulfill");
    assert_eq!(w.lamports(dest), dest0, "nothing is paid to the owner by the provider's fulfillment");
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let close = w.close_record(1);
    assert_eq!(close.eligible_return_remaining, rent - retained);
    assert_eq!(
        close.owner_request_rent_refundable_remaining,
        rent.min(rent - retained),
        "attributed owner-first: all of the real reimbursement belongs to the owner"
    );
    assert_eq!(w.lamports(dest), dest0, "observing pays nothing either");
    let (free0, excess0) = (w.ops().free_balance, w.ops().unallocated_excess);
    let withdraw = withdraw_ix(w, 1, rent - retained);
    ok_g(w, "return", withdraw);
    assert_eq!(w.lamports(dest), dest0 + rent - retained, "the owner receives the reimbursement, not the fee");
    assert_eq!(w.ops().unallocated_excess, excess0, "the owner share was deliverable, not parked");
    assert_eq!(w.ops().free_balance, free0, "none of it reached free cash");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    assert_eq!(w.close_record(1).locked_owner_remaining, 0, "settlement paid the owner's unspent partition out");
    log.extend(scene.w.take_log());

    // The sponsor partition is EMPTY by the time of the request.
    let mut empty = Scene::new(row, "sponsor_exhausted", Spec::standard().with_sponsor(2 * page));
    empty.provision_and_seal("build_last");
    let w = &mut empty.w;
    assert_eq!(w.close_record(1).locked_sponsor_remaining, 0);
    do_request(w, 1, "request");
    let close = w.close_record(1);
    assert_eq!((close.spent.provider_fee_s, close.spent.provider_fee_o), (0, REQUEST_FEE));
    assert_eq!((close.spent.request_rent_s, close.spent.request_rent_o), (0, rent));
    log.extend(empty.w.take_log());
    log
}

pub fn request_after_deadline_is_rejected_before_fees() -> Vec<Observation> {
    let row = ROW_REQUEST_AFTER_DEADLINE;
    let mut log = Vec::new();

    // One second late: refused before any fee moves.
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    w.watch(host_request(w, 1).0);
    w.warp_past_deadline(1);
    let (client0, treasury0, cash0) = (w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address()));
    let ops0 = w.data(w.rt.ops_address());
    let request = request_ix(w, 1);
    reject(w, "", vec![request], &[], ErrorCode::DeadlinePassed);
    assert_eq!((w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address())), (client0, treasury0, cash0));
    assert_eq!(w.data(w.rt.ops_address()), ops0);
    assert_eq!(w.close_record(1).draw_state, state::DRAW_NOT_REQUESTED);
    // The deadline path stays open: expiry skips it cleanly.
    expire_ok(w, "expire", scene.dest, pclose::STAGE_UNREQUESTED);
    log.extend(scene.w.take_log());

    // Exactly at the deadline: still timely.
    let mut timely = Scene::new(row, "at_deadline", Spec::standard());
    timely.provision_and_seal("build_last");
    let w = &mut timely.w;
    w.warp_to_deadline(1);
    do_request(w, 1, "request");
    assert_eq!(w.close_record(1).draw_state, state::DRAW_REQUESTED);
    log.extend(timely.w.take_log());
    log
}

pub fn collision_with_preexisting_request_fails_closed_no_alternate_seed() -> Vec<Observation> {
    use anchor_lang::AccountSerialize;
    let row = ROW_COLLISION;
    let mut log = Vec::new();
    let keys = provider_keys();
    let (rent, _) = request_rents();

    // ---- An ORAO-owned account already sits at the request PDA ----------------
    let mut scene = Scene::new(row, "", Spec::standard());
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let (request, seed) = host_request(w, 1);
    w.watch(request);
    let (_, bump) = provider::request_binding(&keys.client, &seed);
    let fake = orao::state::request::RequestAccount::new(
        bump,
        1,
        keys.client,
        seed,
        orao::state::request::RequestState::Pending(orao::state::request::Pending::new(Vec::new(), None, false)),
    );
    let mut data = Vec::new();
    fake.try_serialize(&mut data).expect("RequestAccount serializes");
    let lamports = w.rt.rent(data.len());
    w.rt.svm
        .set_account(request, Account { lamports, data, owner: orao::id(), executable: false, rent_epoch: 0 })
        .expect("pre-existing request account");
    let (close_before, ops_before) = (w.data(w.rt.close_record_address(1)), w.data(w.rt.ops_address()));
    let (client0, treasury0, cash0) = (w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address()));
    let instruction = request_ix(w, 1);
    reject(w, "", vec![instruction], &[], ErrorCode::Collision);
    // Fails closed: no alternate seed was tried, nothing was booked, no fee moved.
    assert_eq!(w.data(w.rt.close_record_address(1)), close_before);
    assert_eq!(w.data(w.rt.ops_address()), ops_before);
    assert_eq!((w.client_lamports(), w.treasury_lamports(), w.lamports(w.rt.ops_cash_address())), (client0, treasury0, cash0));
    assert_eq!(w.close_record(1).protocol_seed, [0u8; 32], "no seed was recorded");
    assert_eq!(host_request(w, 1), (request, seed), "the one and only seed of this close");
    let instruction = request_ix(w, 1);
    reject(w, "retry_fails_again", vec![instruction], &[], ErrorCode::Collision);
    // Remove the squatter: the very same seed and PDA now work.
    w.rt.remove_account(request);
    let requested = do_request(w, 1, "request_after_removal");
    assert_eq!(requested.last_event::<DrawRequested>().request_account, request);
    assert_eq!(w.close_record(1).protocol_seed, seed);
    log.extend(scene.w.take_log());

    // ---- A pre-funded SYSTEM-owned account at the request PDA --------------------------
    // Recorded, not predicted: whether the provider's `request` accepts a
    // pre-funded empty PDA decides whether an attacker can grief a close by
    // pre-funding its (public) request address.  Either outcome must leave a
    // consistent state; the outcome is annotated on the observation.
    let mut funded = Scene::new(row, "prefunded_system_account", Spec::standard());
    funded.provision_and_seal("build_last");
    let w = &mut funded.w;
    let (request, seed) = host_request(w, 1);
    w.watch(request);
    w.rt.credit_lamports(request, 1_000_000);
    let before = w.snapshot_all();
    let (close_before, ops_before) = (w.data(w.rt.close_record_address(1)), w.data(w.rt.ops_address()));
    let instruction = request_ix(w, 1);
    let outcome = w.send("request", vec![instruction], &[]);
    let accepted = outcome.is_ok();
    let observed = match outcome {
        Ok(observation) => {
            assert_identities(w);
            let close = w.close_record(1);
            assert_eq!(close.draw_state, state::DRAW_REQUESTED);
            assert_eq!(close.protocol_seed, seed);
            let account = w.account(request).expect("request account");
            assert_eq!(account.owner, orao::id(), "the provider took over the pre-funded PDA");
            assert!(account.lamports >= rent);
            observation
        }
        Err(failed) => {
            failed.assert_rollback_except_fee();
            w.assert_fee_only(&before, &failed);
            assert_identities(w);
            assert_eq!(w.data(w.rt.close_record_address(1)), close_before);
            assert_eq!(w.data(w.rt.ops_address()), ops_before);
            assert_eq!(w.close_record(1).draw_state, state::DRAW_NOT_REQUESTED);
            assert_eq!(w.close_record(1).protocol_seed, [0u8; 32], "no alternate seed");
            failed
        }
    };
    let label = observed.label.clone();
    let mut funded_log = funded.w.take_log();
    annotate(
        &mut funded_log,
        &label,
        format!(
            "prefunded_system_account: the request {} (a pre-funded empty system-owned account at the request PDA)",
            if accepted { "was ACCEPTED by the provider (init takes over the pre-funded PDA)" } else { "FAILED CLOSED (the provider refuses a pre-funded PDA; a public-address griefing vector until the deadline)" }
        ),
    );
    log.extend(funded_log);
    log
}

// ---------------------------------------------------------------------------
// Pure host rows (no transaction)
// ---------------------------------------------------------------------------

/// `sampler_all_64_counters_rejected_pure`: the sampler's exhaustion path on
/// CONSTRUCTED sample words (a SHA-256 output cannot be searched for that makes
/// every counter fail), read from `solana/fixtures/sampling.json`.
pub fn sampler_all_64_counters_rejected_pure() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sampling.json");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("sampling.json")).expect("JSON");
    let constructed = doc["constructed_samples"].as_array().expect("constructed_samples");
    assert_eq!(constructed.len(), 2);

    // N = 3: limit = 2^64 - 1, so the all-ones word is rejected at every counter.
    let exhausted = &constructed[0];
    assert_eq!(exhausted["result"].as_str(), Some("exhausted"));
    let n: u64 = exhausted["candidate_count"].as_u64().expect("candidate_count");
    let word: u64 = exhausted["word_for_every_counter"].as_str().expect("word").parse().expect("u64");
    let mut tried = Vec::new();
    assert_eq!(
        sampling::map_samples(n, |counter| {
            tried.push(counter);
            word
        }),
        None,
        "entropy exhausted"
    );
    assert_eq!(tried, (0..sampling::MAX_COUNTERS).collect::<Vec<_>>(), "all 64 counters, in order");

    // N = 2^63 + 1: limit = N; counter 0 rejected, counter 1 accepted.
    let second = &constructed[1];
    let n: u64 = second["candidate_count"].as_u64().expect("candidate_count");
    let words: Vec<u64> = second["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|word| word.as_str().expect("word").parse().expect("u64"))
        .collect();
    assert_eq!(
        sampling::map_samples(n, |counter| words[counter as usize]),
        Some((second["ordinal"].as_u64().expect("ordinal"), second["counter"].as_u64().expect("counter")))
    );
    assert_eq!(sampling::map_samples(n, |_| u64::MAX), None);
    assert_eq!(sampling::map_samples(0, |_| 0), None, "no candidates, no ordinal");
    assert_eq!(sampling::map_output([7; 64], 1, 0), None);
}

/// `solana/fixtures/seed-vectors.json` is the program sampler's output and
/// agrees with `sampling.json`: the digest chain, both cluster-domain seeds.
pub fn seed_vectors_file_matches_the_program_sampler() {
    let vectors = load_seed_vectors();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sampling.json");
    let sampling_doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("sampling.json")).expect("JSON");
    let pinned = &sampling_doc["protocol_seed"];
    let candidates: Vec<(Pubkey, u128)> = vectors["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .map(|candidate| {
            (
                Pubkey::new_from_array(array32(candidate["owner_hex"].as_str().expect("owner_hex"))),
                candidate["loss"].as_str().expect("loss").parse().expect("u128"),
            )
        })
        .collect();
    let digest = array32(vectors["sealed_digest_hex"].as_str().expect("digest"));
    assert_eq!(chain(0, &candidates), digest);
    assert_eq!(hex(&digest), pinned["sealed_digest_hex"].as_str().expect("sampling digest"));
    assert_eq!(vectors["program_id"].as_str(), Some(ID.to_string().as_str()));
    let (round, close, tag, count) = (
        vectors["round_id"].as_u64().expect("round"),
        vectors["close_id"].as_u64().expect("close"),
        vectors["set_tag"].as_u64().expect("tag") as u8,
        vectors["selected_count"].as_u64().expect("count"),
    );
    assert_eq!(count, candidates.len() as u64);
    assert_eq!(
        (round, close, tag, count),
        (
            pinned["round_id"].as_u64().expect("round"),
            pinned["close_id"].as_u64().expect("close"),
            pinned["set_tag"].as_u64().expect("tag") as u8,
            pinned["selected_count"].as_u64().expect("count"),
        )
    );
    let ours = vectors["vectors"].as_array().expect("vectors");
    let theirs = pinned["vectors"].as_array().expect("pinned vectors");
    assert_eq!(ours.len(), theirs.len());
    for (vector, pinned_vector) in ours.iter().zip(theirs) {
        let domain = array32(vector["cluster_domain_hex"].as_str().expect("domain"));
        assert_eq!(vector["cluster_domain_hex"], pinned_vector["cluster_domain_hex"]);
        assert_eq!(
            std::str::from_utf8(&domain).expect("ASCII domain"),
            vector["cluster_domain"].as_str().expect("domain label")
        );
        let seed = sampling::protocol_seed(&domain, &ID, round, close, tag, count, &digest);
        assert_eq!(hex(&seed), vector["seed_hex"].as_str().expect("seed"), "program sampler");
        assert_eq!(vector["seed_hex"], pinned_vector["seed_hex"], "agrees with sampling.json");
    }
    assert_ne!(ours[0]["seed_hex"], ours[1]["seed_hex"], "the cluster domain is bound into the seed");
}

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

/// One suite row: its headline name and the function that runs it.
pub type RowEntry = (&'static str, fn() -> Vec<Observation>);

/// Every transaction row, in suite order.
pub fn rows() -> Vec<RowEntry> {
    vec![
        (ROW_PROVIDER, provider_setup_row),
        (ROW_BUILD_SEALS, build_candidates_visits_every_participant_once_and_seals_digests),
        (ROW_BUILD_REJECTS, build_candidates_rejects_skipped_or_reordered_rows),
        (ROW_AGGREGATION, same_owner_multi_position_aggregation_and_sealed_userround_immutability),
        (ROW_ZERO_LOSERS, sealed_with_zero_losers_skips_no_candidates_without_request),
        (ROW_REQUEST_CREATES, request_round_draw_creates_request_with_protocol_seed_and_marker_atomically),
        (ROW_SEED_VECTORS, protocol_seed_differs_between_devnet_and_mainnet_cluster_domains_and_matches_host_vector),
        (ROW_REQUEST_PARTITIONS, request_decrements_remaining_partitions_sponsor_first_and_records_spent_split),
        (ROW_REQUEST_CPI_FAILURE, request_cpi_failure_leaves_partitions_and_ledger_unchanged),
        (ROW_FULFILLMENT, fulfillment_reimburses_excess_rent_to_client_only_and_changes_no_ops_accounting),
        (ROW_OBSERVE, observe_request_fulfilled_once_sets_eligible_and_owner_refundable_remaining_and_rejects_pending),
        (ROW_CHANGED_RENT, changed_rent_sysvar_after_fulfillment_does_not_overstate_eligible),
        (ROW_EXTRA_LAMPORTS, extra_request_account_lamports_reduce_eligibility_never_increase),
        (ROW_WITHDRAW_RETURN, withdraw_provider_return_executes_orao_withdraw_cpi_and_asserts_client_and_opscash_deltas),
        (ROW_TWO_PARTIAL, two_partial_provider_returns_then_replay_pay_exact_owner_residual_once),
        (ROW_RETURN_PREFLIGHT, undeliverable_owner_share_is_refused_before_the_cpi_and_retried_once_the_destination_is_funded),
        (ROW_G_SIGNATURE, g_signature_without_executed_withdraw_cpi_credits_nothing_and_anonymous_opscash_surplus_is_never_reclassified),
        (ROW_WITHDRAW_REJECTS, withdraw_rejects_wrong_client_wrong_destination_and_amount_above_eligible),
        (ROW_WITHDRAW_CPI_FAILURE, cpi_failure_rolls_back_all_accounting),
        (ROW_TWO_CLOSES_REIMBURSEMENT, two_closes_cannot_claim_the_same_reimbursement),
        (ROW_LATE_FULFILLMENT, late_fulfillment_after_expiry_becomes_eligible_only_when_observed),
        (ROW_PARTITION_AGGREGATE, locked_remaining_total_aggregate_equals_sum_of_remaining_partitions_after_every_transition),
        (ROW_DEFICIT_THEN_RETURN, observed_deficit_then_real_return_lowers_it_and_never_blocks_commit_expire_finalize_or_claims),
        (ROW_EXPIRED_UNFULFILLED, expired_unfulfilled_request_keeps_rent_outstanding_and_late_fulfillment_reimburses_client_later),
        (ROW_G_WITHDRAWS_ELSEWHERE, g_withdraws_client_elsewhere_records_deficit_without_blocking_commit_expire_finalize_or_claims),
        (ROW_BUDGET, increase_provider_budget_moves_extra_from_free_raises_cap_and_refunds_unused_once),
        (ROW_SETTLEMENT, settlement_refunds_only_remaining_partitions_never_spent_amounts),
        (ROW_ANONYMOUS_CREDIT, anonymous_ops_cash_credit_becomes_unallocated_excess_and_is_never_spent),
        (ROW_REQUEST_REJECTS_PROVIDER, request_rejects_wrong_client_owner_wrong_client_state_or_wrong_account_owner),
        (ROW_OWNER_FALLBACK, owner_fallback_may_fund_provider_fee_and_rent_after_sponsor_exhausted_with_exact_attribution),
        (ROW_REQUEST_AFTER_DEADLINE, request_after_deadline_is_rejected_before_fees),
        (ROW_COLLISION, collision_with_preexisting_request_fails_closed_no_alternate_seed),
        (ROW_CAPTURE_DEADLINE, capture_at_deadline_equality_is_timely_and_capture_one_second_later_is_ignored_late_noop),
        (ROW_APPLY_PROMOTION, apply_promotion_first_valid_counter_selects_candidate_and_grants_b_116),
        (ROW_ENTROPY_EXHAUSTED, apply_promotion_terminalizes_entropy_exhausted_from_labeled_synthetic_state),
        (ROW_EXPIRE, expire_promotion_requires_clock_past_deadline_and_no_capture),
        (ROW_TIMELY_CAPTURE, timely_capture_cannot_be_expired_and_applies_after_deadline),
        (ROW_EXPIRE_DURING_BUILD, expire_during_candidate_build_abandons_candidate_pages_but_preserves_cash),
        (ROW_FINALIZE, finalize_requires_cash_committed_and_terminal_promotion_then_carries_full_seed_once),
        (ROW_COMPOSED, direct_open_against_closing_round_fails_and_composed_expire_finalize_open_succeeds_with_expected_round),
        (ROW_LATE_AUTHENTIC, late_authentic_fulfillment_after_skip_cannot_mint_or_revive),
        (ROW_SECOND_CLOSE_BINDING, second_close_cannot_reuse_first_close_request_binding),
        (ROW_TWO_CLOSES_SEQUENCE, two_closes_two_rounds_sequence),
    ]
}

/// A row must contain an observation labelled exactly
/// `financial.close.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = headline("", row);
    assert!(
        observations.iter().any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

/// Run one row and check it is internally consistent and its labels unique.
pub fn run_row(name: &str, row: fn() -> Vec<Observation>) {
    let observations = row();
    assert_headline(&observations, name);
    assert_outcomes_consistent(&observations);
    assert_unique_labels(&observations);
}

/// The whole close suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_close_suite() -> Vec<Observation> {
    sampler_all_64_counters_rejected_pure();
    seed_vectors_file_matches_the_program_sampler();
    let mut observations = Vec::new();
    for (name, row) in rows() {
        let produced = row();
        assert_headline(&produced, name);
        assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    assert_unique_labels(&observations);
    observations
}
