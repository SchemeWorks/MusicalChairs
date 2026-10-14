//! M3 authenticated-controls suite.
//!
//! Every row runs the actual compiled Musical Chairs SBF in LiteSVM against
//! byte-exact LOCAL fixtures: Squads v4 `Multisig` accounts (G = governance,
//! P = pause guardian) and the upgradeable-loader `Program`/`ProgramData`
//! pair.  Instructions that must be signed by a Squads vault are wrapped in
//! the TEST-ONLY stand-in program loaded at the Squads id, which signs as the
//! vault PDA with `invoke_signed`.  No real Squads execution, deployment or
//! time lock is exercised here (that stays M14).
//!
//! Labels are `financial.controls.m3.<row>` for a row's headline observation
//! and `financial.controls.m3.<row>.<variant>` for the supporting ones.  Every
//! rejection asserts the exact program error code parsed from
//! `Observation.error` and fee-only rollback; every success asserts the two
//! cash-register conservation invariants and, through [`Rig`], that
//! `Pool.cluster_domain` is unchanged after every single instruction.

use std::collections::BTreeSet;

use anchor_lang::AnchorSerialize;
use musical_chairs_program::{
    accounts,
    controls::{InitializeArgs, SQUADS_PROGRAM_ID},
    instruction, state, ConfigTerms, ErrorCode, ID,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::runtime::{
    anchor_ix, fixtures, pda, FinancialRuntime, Observation, ATTACKER_SEED, FIXTURE_CLUSTER_DOMAIN,
    G_CREATE_KEY, MANAGEMENT_SEED, OWNER_SEED, P_CREATE_KEY, SPONSOR_SEED,
};

const CU: u32 = 1_400_000;
/// Synthetic `Pool.cluster_domain` vectors.  They differ so the immutability
/// row can tell a devnet-shaped genesis from a mainnet-shaped one; they are
/// fixture labels, not real cluster genesis hashes.
pub const DEVNET_DOMAIN: [u8; 32] = *b"musical-chairs-devnet-fixture!!!";
pub const MAINNET_DOMAIN: [u8; 32] = *b"musical-chairs-mainnet-fixture!!";
/// A second, structurally valid G profile that is NOT `Pool.governance`.
const ROGUE_G_CREATE_KEY: [u8; 32] = [0x48; 32];
/// The replacement P profile used by the rotation rows.
const NEW_P_CREATE_KEY: [u8; 32] = [0x51; 32];
/// A valid P profile that is NOT the recorded guardian.
const ROGUE_P_CREATE_KEY: [u8; 32] = [0x52; 32];

pub const ROW_INITIALIZE_SUCCEEDS: &str =
    "initialize_succeeds_with_g_vault_signer_and_matching_programdata";
pub const ROW_INITIALIZE_FIRST_CALLER: &str = "initialize_rejects_first_caller_without_g_vault";
pub const ROW_INITIALIZE_PROGRAMDATA: &str = "initialize_rejects_programdata_authority_mismatch";
pub const ROW_INITIALIZE_PROFILE: &str = "initialize_rejects_one_of_three_profile";
pub const ROW_INITIALIZE_GENESIS: &str = "initialize_sets_pot_seed_zero_round_one_next_close_one";
pub const ROW_INITIALIZE_PADDED: &str = "initialize_accepts_allocation_padded_multisig";
pub const ROW_PAUSE_TOUCHES_ONLY_ADMISSION: &str =
    "set_admission_pause_touches_only_admission_account";
pub const ROW_PAUSE_REJECTS_NON_GUARDIAN: &str = "set_admission_pause_rejects_non_guardian";
pub const ROW_ROTATE_GUARDIAN: &str =
    "rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted";
pub const ROW_ROTATE_GUARDIAN_RECOVERY: &str = "rotate_guardian_recovers_lost_p_quorum";
pub const ROW_WITHDRAW_MANAGEMENT: &str =
    "withdraw_management_pays_recipient_only_and_caps_at_accrued";
pub const ROW_FUND_OPERATIONS: &str = "fund_operations_any_signer_increases_free_balance_only";
pub const ROW_WITHDRAW_OPERATIONS: &str = "withdraw_unreserved_operations_respects_locked_and_rent";
pub const ROW_STAGE_CONFIG: &str = "stage_config_requires_g_and_future_round";
pub const ROW_ACTIVATE_CONFIG: &str =
    "activate_config_only_when_active_no_ticket_and_round_reached";
pub const ROW_ROTATE_RECIPIENT: &str = "rotate_recipient_requires_g";
pub const ROW_CLUSTER_DOMAIN: &str = "initialize_stores_cluster_domain_immutably";

/// Every row, in suite order.
pub const ROW_NAMES: [&str; 17] = [
    ROW_INITIALIZE_SUCCEEDS,
    ROW_INITIALIZE_FIRST_CALLER,
    ROW_INITIALIZE_PROGRAMDATA,
    ROW_INITIALIZE_PROFILE,
    ROW_INITIALIZE_GENESIS,
    ROW_INITIALIZE_PADDED,
    ROW_PAUSE_TOUCHES_ONLY_ADMISSION,
    ROW_PAUSE_REJECTS_NON_GUARDIAN,
    ROW_ROTATE_GUARDIAN,
    ROW_ROTATE_GUARDIAN_RECOVERY,
    ROW_WITHDRAW_MANAGEMENT,
    ROW_FUND_OPERATIONS,
    ROW_WITHDRAW_OPERATIONS,
    ROW_STAGE_CONFIG,
    ROW_ACTIVATE_CONFIG,
    ROW_ROTATE_RECIPIENT,
    ROW_CLUSTER_DOMAIN,
];

/// The whole M3 suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_controls_m3_suite() -> Vec<Observation> {
    let rows: [fn() -> Vec<Observation>; 17] = [
        initialize_succeeds_with_g_vault_signer_and_matching_programdata,
        initialize_rejects_first_caller_without_g_vault,
        initialize_rejects_programdata_authority_mismatch,
        initialize_rejects_one_of_three_profile,
        initialize_sets_pot_seed_zero_round_one_next_close_one,
        initialize_accepts_allocation_padded_multisig,
        set_admission_pause_touches_only_admission_account,
        set_admission_pause_rejects_non_guardian,
        rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted,
        rotate_guardian_recovers_lost_p_quorum,
        withdraw_management_pays_recipient_only_and_caps_at_accrued,
        fund_operations_any_signer_increases_free_balance_only,
        withdraw_unreserved_operations_respects_locked_and_rent,
        stage_config_requires_g_and_future_round,
        activate_config_only_when_active_no_ticket_and_round_reached,
        rotate_recipient_requires_g,
        initialize_stores_cluster_domain_immutably,
    ];
    let mut observations = Vec::new();
    for (row, name) in rows.into_iter().zip(ROW_NAMES) {
        let produced = row();
        assert_headline(&produced, name);
        assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    assert_unique_labels(&observations);
    observations
}

/// A row must contain an observation labelled exactly
/// `financial.controls.m3.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = label(row, "");
    assert!(
        observations
            .iter()
            .any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

/// Every observation is internally consistent: a success carries no error,
/// a failure carries one and rolled back everything except the fee.
pub fn assert_outcomes_consistent(observations: &[Observation]) {
    for observation in observations {
        if observation.success {
            assert!(
                observation.error.is_none(),
                "{} succeeded with an error: {observation:?}",
                observation.label
            );
        } else {
            assert!(
                observation.error.is_some(),
                "{} failed without an error: {observation:?}",
                observation.label
            );
            observation.assert_rollback_except_fee();
        }
    }
}

pub fn assert_unique_labels(observations: &[Observation]) {
    let mut seen = BTreeSet::new();
    for observation in observations {
        assert!(
            seen.insert(observation.label.clone()),
            "duplicate observation label {}",
            observation.label
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn label(row: &str, variant: &str) -> String {
    if variant.is_empty() {
        format!("financial.controls.m3.{row}")
    } else {
        format!("financial.controls.m3.{row}.{variant}")
    }
}

/// Anchor numbers `#[error_code]` variants from 6000 in declaration order.
fn code(error: ErrorCode) -> u32 {
    error as u32 + 6000
}

/// Anchor's own framework code for a `Signer` account that did not sign.
fn account_not_signer() -> u32 {
    anchor_lang::error::ErrorCode::AccountNotSigner as u32
}

/// The `N` of `Custom(N)` in `Observation.error`
/// (`InstructionError(<top-level index>, Custom(N))`).  The top-level index is
/// 1 for both direct and stand-in-wrapped transactions because the compute
/// budget instruction is index 0 and a CPI failure surfaces on the outer
/// instruction, so only the custom code is asserted.
pub fn custom_code(observation: &Observation) -> Option<u32> {
    parse_custom_code(observation.error.as_deref()?)
}

/// The `N` of the first `Custom(N)` in an error string, if any.
pub fn parse_custom_code(error: &str) -> Option<u32> {
    let start = error.find("Custom(")? + "Custom(".len();
    let digits: String = error[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn sponsor() -> Keypair {
    Keypair::new_from_array(SPONSOR_SEED)
}
fn attacker() -> Keypair {
    Keypair::new_from_array(ATTACKER_SEED)
}
fn owner() -> Keypair {
    Keypair::new_from_array(OWNER_SEED)
}
fn management() -> Keypair {
    Keypair::new_from_array(MANAGEMENT_SEED)
}

fn key(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

fn ser<T: AnchorSerialize>(value: &T) -> Vec<u8> {
    value.try_to_vec().expect("Borsh serialization")
}

/// Little-endian bytes of the two Ops lifetime counters
/// (`total_contributed`, `total_withdrawn`), so a rollback regression can be
/// asserted byte-for-byte.
fn counter_bytes(ops: &state::Ops) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(&ops.total_contributed.to_le_bytes());
    bytes[16..].copy_from_slice(&ops.total_withdrawn.to_le_bytes());
    bytes
}

fn changed(observation: &Observation) -> BTreeSet<String> {
    observation
        .account_changes
        .iter()
        .map(|change| change.pubkey.clone())
        .collect()
}

fn keys(list: &[Pubkey]) -> BTreeSet<String> {
    list.iter().map(ToString::to_string).collect()
}

/// Lamport delta of `key` in the observation (0 when the account did not change).
fn delta(observation: &Observation, key: Pubkey) -> i128 {
    observation
        .account_changes
        .iter()
        .find(|change| change.pubkey == key.to_string())
        .map(|change| change.lamport_delta)
        .unwrap_or(0)
}

fn metas(instruction: &Instruction) -> Vec<(Pubkey, bool, bool)> {
    instruction
        .accounts
        .iter()
        .map(|meta| (meta.pubkey, meta.is_signer, meta.is_writable))
        .collect()
}

/// A knob-per-field Squads `Multisig` fixture, for malformed variants.
#[derive(Clone)]
struct Spec {
    create_key: [u8; 32],
    config_authority: Pubkey,
    threshold: u16,
    time_lock: u32,
    rent_collector: Option<Pubkey>,
    bump_offset: u8,
    members: Vec<(Pubkey, u8)>,
}

impl Spec {
    fn canonical(create_key: [u8; 32], time_lock: u32) -> Self {
        Self {
            create_key,
            config_authority: Pubkey::default(),
            threshold: 2,
            time_lock,
            rent_collector: None,
            bump_offset: 0,
            members: fixtures::squads::canonical_members(&Pubkey::new_from_array(create_key)),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let create_key = Pubkey::new_from_array(self.create_key);
        let bump = fixtures::squads::multisig_address(&create_key)
            .1
            .wrapping_add(self.bump_offset);
        fixtures::squads::multisig_bytes(
            create_key,
            self.config_authority,
            self.threshold,
            self.time_lock,
            self.rent_collector,
            bump,
            &self.members,
        )
    }
}

fn tweak(create_key: [u8; 32], time_lock: u32, change: impl FnOnce(&mut Spec)) -> Vec<u8> {
    let mut spec = Spec::canonical(create_key, time_lock);
    change(&mut spec);
    spec.bytes()
}

/// One runtime plus the genesis `cluster_domain` expected for it.  Every
/// instruction goes through [`Rig`], which re-checks the domain afterwards.
struct Rig {
    rt: FinancialRuntime,
    domain: [u8; 32],
}

impl Rig {
    /// Fresh runtime: programs, loader fixture and Squads accounts only.
    fn uninitialized(domain: [u8; 32]) -> Self {
        Self {
            rt: FinancialRuntime::new_uninitialized(),
            domain,
        }
    }

    /// Post-genesis fixture state bound to the G and P vaults.
    fn governed() -> Self {
        Self {
            rt: FinancialRuntime::new_governed(),
            domain: FIXTURE_CLUSTER_DOMAIN,
        }
    }

    fn initialized(&self) -> bool {
        self.rt
            .svm
            .get_account(&self.rt.pool_address())
            .is_some_and(|account| account.owner == ID && !account.data.is_empty())
    }

    fn pool(&self) -> state::Pool {
        self.rt.deserialize(self.rt.pool_address())
    }
    fn ops(&self) -> state::Ops {
        self.rt.deserialize(self.rt.ops_address())
    }
    fn admission(&self) -> state::Admission {
        self.rt.deserialize(self.rt.admission_address())
    }
    fn config(&self, version: u32) -> state::ConfigVersion {
        self.rt.deserialize(self.rt.config_address(version))
    }

    fn set_pool(&mut self, change: impl FnOnce(&mut state::Pool)) {
        let mut pool = self.pool();
        change(&mut pool);
        self.rt.replace_pool(pool);
    }

    fn set_paused(&mut self, paused: bool) {
        let mut admission = self.admission();
        admission.paused = paused;
        self.rt.replace_admission(admission);
    }

    /// `Pool.cluster_domain` is written once by `initialize`; assert it after
    /// every instruction (a no-op before the Pool exists).
    fn guard(&self) {
        if self.initialized() {
            assert_eq!(
                self.pool().cluster_domain,
                self.domain,
                "Pool.cluster_domain changed after an instruction"
            );
        }
    }

    /// Execute `instruction` as a plain transaction (payer plus `signers`).
    fn direct(
        &mut self,
        label: String,
        instruction: Instruction,
        signers: &[&Keypair],
    ) -> Observation {
        let observation = self.rt.execute(label, vec![instruction], signers, CU);
        self.guard();
        observation
    }

    /// Execute `instruction` as the index-0 vault of `multisig`, through the
    /// Squads stand-in.
    fn via(
        &mut self,
        label: String,
        multisig: Pubkey,
        instruction: Instruction,
        signers: &[&Keypair],
    ) -> Observation {
        let observation = self
            .rt
            .execute_via_vault(label, multisig, 0, instruction, signers, CU);
        self.guard();
        observation
    }

    fn via_g(
        &mut self,
        label: String,
        instruction: Instruction,
        signers: &[&Keypair],
    ) -> Observation {
        let multisig = self.rt.g_multisig();
        self.via(label, multisig, instruction, signers)
    }

    fn via_p(
        &mut self,
        label: String,
        instruction: Instruction,
        signers: &[&Keypair],
    ) -> Observation {
        let multisig = self.rt.p_multisig();
        self.via(label, multisig, instruction, signers)
    }

    /// Install a second structurally valid G multisig that is not
    /// `Pool.governance`; returns `(multisig, vault)`.
    fn rogue_g(&mut self) -> (Pubkey, Pubkey) {
        let multisig = self
            .rt
            .install_multisig(ROGUE_G_CREATE_KEY, fixtures::squads::GOVERNANCE_TIME_LOCK);
        (multisig, fixtures::squads::vault_address(&multisig, 0).0)
    }

    /// Install a P-profile multisig for `create_key`; returns `(multisig, vault)`.
    fn p_profile(&mut self, create_key: [u8; 32]) -> (Pubkey, Pubkey) {
        let multisig = self
            .rt
            .install_multisig(create_key, fixtures::squads::GUARDIAN_TIME_LOCK);
        (multisig, fixtures::squads::vault_address(&multisig, 0).0)
    }

    /// Put the committed G and P fixtures back after a malformed variant.
    fn restore_canonical_multisigs(&mut self) {
        let g_multisig = self.rt.g_multisig();
        self.rt.install_squads_account(
            g_multisig,
            SQUADS_PROGRAM_ID,
            fixtures::squads::committed_governance(),
        );
        let p_multisig = self.rt.p_multisig();
        self.rt.install_squads_account(
            p_multisig,
            SQUADS_PROGRAM_ID,
            fixtures::squads::committed_guardian(),
        );
    }

    /// Success: the transaction succeeded and both cash registers conserve.
    fn ok(&self, observation: &Observation) {
        assert!(
            observation.success,
            "{} expected success: {observation:?}",
            observation.label
        );
        if self.initialized() {
            self.rt.conservation_check();
        }
    }

    /// Rejection: exact `Custom(expected)` and fee-only rollback.  Conservation
    /// still holds because nothing but the fee changed.
    fn rejected(&self, observation: &Observation, expected: u32) {
        self.rejected_unbalanced(observation, expected);
        if self.initialized() {
            self.rt.conservation_check();
        }
    }

    /// [`Rig::rejected`] without the conservation check, for fixtures that
    /// deliberately desynchronise the cash registers.
    fn rejected_unbalanced(&self, observation: &Observation, expected: u32) {
        assert!(
            !observation.success,
            "{} unexpectedly succeeded: {observation:?}",
            observation.label
        );
        assert_eq!(
            custom_code(observation),
            Some(expected),
            "{}: expected Custom({expected}), got {:?}",
            observation.label,
            observation.error
        );
        observation.assert_rollback_except_fee();
    }

    fn rejected_program(&self, observation: &Observation, expected: ErrorCode) {
        self.rejected(observation, code(expected));
    }

    fn fund(&mut self, label: String, funder: &Keypair, amount: u64) -> Observation {
        let instruction = fund_ix(&self.rt, funder.pubkey(), amount);
        self.direct(label, instruction, &[funder])
    }
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn initialize_args(rt: &FinancialRuntime, domain: [u8; 32]) -> InitializeArgs {
    InitializeArgs {
        sponsor: rt.sponsor.pubkey(),
        management_recipient: rt.management.pubkey(),
        cluster_domain: domain,
        // Synthetic provider tuple: every value is a fixture label.
        provider_request_fee_lamports: 1_000_000,
        provider_fee_headroom_bps: 2_000,
        provider_program: key(0xE1),
        provider_network_state: key(0xE2),
        provider_treasury: key(0xE3),
        provider_client: key(0xE4),
        public_series_a_enabled: false,
    }
}

fn initialize_accounts(rt: &FinancialRuntime) -> accounts::Initialize {
    accounts::Initialize {
        sponsor: rt.sponsor.pubkey(),
        pool: rt.pool_address(),
        admission: rt.admission_address(),
        game_vault: rt.game_vault_address(),
        ops: rt.ops_address(),
        ticket: rt.ticket_address(),
        rng_authority: rt.rng_authority_address(),
        config: rt.config_address(1),
        ops_cash: rt.ops_cash_address(),
        squads_program: SQUADS_PROGRAM_ID,
        governance_multisig: rt.g_multisig(),
        governance: rt.g_vault(),
        guardian_multisig: rt.p_multisig(),
        guardian: rt.p_vault(),
        self_program: ID,
        program_data: rt.program_data_address(),
        system_program: system_program::ID,
    }
}

fn initialize_ix(rt: &FinancialRuntime, args: InitializeArgs) -> Instruction {
    anchor_ix(initialize_accounts(rt), instruction::Initialize { args })
}

fn pause_ix(rt: &FinancialRuntime, guardian: Pubkey, paused: bool) -> Instruction {
    anchor_ix(
        accounts::Pause {
            admission: rt.admission_address(),
            guardian,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::SetPause { paused },
    )
}

fn fund_ix(rt: &FinancialRuntime, sender: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::FundOperations {
            sender,
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::FundOperations { amount },
    )
}

fn withdraw_operations_ix(rt: &FinancialRuntime, sponsor: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::WithdrawOperations {
            sponsor,
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::WithdrawOperations { amount },
    )
}

fn withdraw_management_ix(rt: &FinancialRuntime, recipient: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::WithdrawManagement {
            pool: rt.pool_address(),
            game_vault: rt.game_vault_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            recipient,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::WithdrawManagement { amount },
    )
}

fn rotate_guardian_ix(
    rt: &FinancialRuntime,
    governance_multisig: Pubkey,
    governance: Pubkey,
    new_guardian_multisig: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::RotateGuardian {
            governance_multisig,
            governance,
            admission: rt.admission_address(),
            pool: rt.pool_address(),
            new_guardian_multisig,
        },
        instruction::RotateGuardian,
    )
}

fn rotate_recipient_ix(
    rt: &FinancialRuntime,
    governance_multisig: Pubkey,
    governance: Pubkey,
    kind: u8,
    new_recipient: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::RotateRecipient {
            governance_multisig,
            governance,
            pool: rt.pool_address(),
            ops: rt.ops_address(),
        },
        instruction::RotateRecipient {
            kind,
            new_recipient,
        },
    )
}

fn stage_ix(
    rt: &FinancialRuntime,
    governance_multisig: Pubkey,
    governance: Pubkey,
    version: u32,
    terms: ConfigTerms,
) -> Instruction {
    anchor_ix(
        accounts::StageConfigVersion {
            governance_multisig,
            governance,
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            config: rt.config_address(version),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::StageConfigVersion { version, terms },
    )
}

fn activate_ix(rt: &FinancialRuntime, version: u32, expected_current: u32) -> Instruction {
    anchor_ix(
        accounts::ActivateConfigVersion {
            pool: rt.pool_address(),
            config: rt.config_address(version),
            ticket: rt.ticket_address(),
            admission: rt.admission_address(),
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::ActivateConfigVersion {
            version,
            expected_current,
        },
    )
}

/// Launch-equivalent terms (`ConfigVersion::default_v1`) with a chosen
/// `effective_round_id`.
fn terms(effective_round_id: u64) -> ConfigTerms {
    let mut terms = ConfigTerms::from(&state::ConfigVersion::default_v1());
    terms.effective_round_id = effective_round_id;
    terms
}

// ---------------------------------------------------------------------------
// Genesis assertions
// ---------------------------------------------------------------------------

/// Every account `initialize` creates exists at its declared size, owned by
/// the program, rent exempt, bound to the G and P vaults and the args.
fn assert_genesis_accounts(rt: &FinancialRuntime, args: &InitializeArgs) {
    for (name, address, space) in [
        ("pool", rt.pool_address(), state::Pool::SPACE),
        ("admission", rt.admission_address(), state::Admission::SPACE),
        (
            "game_vault",
            rt.game_vault_address(),
            state::GameVault::SPACE,
        ),
        ("ops", rt.ops_address(), state::Ops::SPACE),
        ("ticket", rt.ticket_address(), state::Ticket::SPACE),
        (
            "rng_authority",
            rt.rng_authority_address(),
            state::RngAuthority::SPACE,
        ),
        (
            "config#1",
            rt.config_address(1),
            state::ConfigVersion::SPACE,
        ),
    ] {
        let account = rt.account(address);
        assert_eq!(account.owner, ID, "{name}: owner");
        assert_eq!(account.data.len(), space, "{name}: declared size");
        assert_eq!(
            account.lamports,
            rt.rent(space),
            "{name}: rent-exempt minimum"
        );
    }
    let ops_cash = rt.account(rt.ops_cash_address());
    assert_eq!(
        ops_cash.owner,
        system_program::ID,
        "ops_cash stays system-owned"
    );
    assert!(ops_cash.data.is_empty(), "ops_cash stays empty");
    assert_eq!(
        ops_cash.lamports,
        rt.rent(0),
        "ops_cash holds exactly its rent floor"
    );

    let pool: state::Pool = rt.deserialize(rt.pool_address());
    assert_eq!(pool.bump, pda(&[state::POOL_SEED]).1);
    assert_eq!(
        pool.governance,
        rt.g_vault(),
        "Pool.governance is the G vault"
    );
    assert_eq!(
        pool.guardian_vault,
        rt.p_vault(),
        "Pool.guardian_vault is the P vault"
    );
    assert_eq!(pool.sponsor, args.sponsor);
    assert_eq!(pool.management_recipient, args.management_recipient);
    assert_eq!(
        pool.cluster_domain, args.cluster_domain,
        "cluster_domain == args"
    );

    let admission: state::Admission = rt.deserialize(rt.admission_address());
    assert_eq!(admission.bump, pda(&[state::ADMISSION_SEED]).1);
    assert_eq!(
        admission.guardian_vault,
        rt.p_vault(),
        "Admission.guardian_vault is the P vault"
    );
    assert!(!admission.paused);
    assert_eq!(admission.changed_at, 0);

    let game_vault: state::GameVault = rt.deserialize(rt.game_vault_address());
    assert_eq!(game_vault.bump, pda(&[state::GAME_VAULT_SEED]).1);

    let ops: state::Ops = rt.deserialize(rt.ops_address());
    assert_eq!(ops.bump, pda(&[state::OPS_SEED]).1);
    assert_eq!(ops.sponsor, args.sponsor);
    assert_eq!(ops.free_balance, 0);
    assert_eq!(ops.locked_remaining_total, 0);
    assert_eq!(ops.unallocated_excess, 0);
    assert_eq!(
        ops.total_rent_funded, 0,
        "no rent has been funded at genesis"
    );
    assert_eq!(
        ops.protected_rent,
        rt.rent(0),
        "Ops.protected_rent is the live OpsCash rent floor at genesis"
    );

    let ticket: state::Ticket = rt.deserialize(rt.ticket_address());
    assert_eq!(ticket.bump, pda(&[state::TICKET_SEED]).1);
    assert_eq!(ticket.id, 0);
    assert_eq!(ticket.phase, state::IDLE);

    let rng: state::RngAuthority = rt.deserialize(rt.rng_authority_address());
    assert_eq!(rng.bump, pda(&[state::RNG_AUTHORITY_SEED]).1);
    assert_eq!(rng.client, args.provider_client);

    let config: state::ConfigVersion = rt.deserialize(rt.config_address(1));
    let mut expected = state::ConfigVersion::default_v1();
    expected.bump = pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).1;
    expected.staged_at = 0;
    expected.activated_at = Some(0);
    expected.provider_request_fee_lamports = args.provider_request_fee_lamports;
    expected.provider_fee_headroom_bps = args.provider_fee_headroom_bps;
    expected.provider_program = args.provider_program;
    expected.provider_network_state = args.provider_network_state;
    expected.provider_treasury = args.provider_treasury;
    expected.provider_client = args.provider_client;
    expected.public_series_a_enabled = args.public_series_a_enabled;
    assert_eq!(
        config, expected,
        "ConfigVersion#1 is the launch rule set plus the genesis tuple"
    );
}

/// The genesis counters: no money, round 1, first close id 1, nothing open.
fn assert_genesis_counters(pool: &state::Pool, game_vault_rent: u64) {
    assert_eq!(pool.pot, 0, "pot");
    assert_eq!(pool.seed, 0, "seed");
    assert_eq!(pool.management, 0, "management");
    assert_eq!(pool.pending_toll, 0, "pending_toll");
    assert_eq!(pool.funded_claims, 0, "funded_claims");
    assert_eq!(pool.excess, 0, "excess");
    assert_eq!(
        pool.protected_rent, game_vault_rent,
        "protected_rent is the live GameVault rent floor at genesis"
    );
    assert_eq!(pool.round_id, 1, "round_id");
    assert_eq!(pool.next_close_id, 1, "next_close_id");
    assert_eq!(pool.current_close_id, None, "current_close_id");
    assert_eq!(pool.active_config_version, 1, "active_config_version");
    assert_eq!(pool.next_sequence, 1, "next_sequence");
    assert_eq!(pool.next_ticket, 1, "next_ticket");
    assert_eq!(pool.active_ticket, 0, "active_ticket");
    assert_eq!(pool.finalized_ticket, 0, "finalized_ticket");
    assert_eq!(pool.registry_len, 0, "registry_len");
    assert_eq!(pool.registry_version, 0, "registry_version");
    assert_eq!(pool.snapshot_pages, 0, "snapshot_pages");
    assert_eq!(pool.registry_pages, 0, "registry_pages");
    assert_eq!(pool.participant_count, 0, "participant_count");
    assert_eq!(pool.participant_pages, 0, "participant_pages");
    assert_eq!(pool.total_gross_deposits, 0, "total_gross_deposits");
    assert_eq!(pool.total_net_payouts, 0, "total_net_payouts");
}

/// Genesis through the actual G-vault path; returns the rig and the args.
fn genesis(domain: [u8; 32], row: &str, variant: &str) -> (Rig, InitializeArgs, Observation) {
    let mut rig = Rig::uninitialized(domain);
    let args = initialize_args(&rig.rt, domain);
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(label(row, variant), instruction, &[&sponsor()]);
    rig.ok(&observation);
    (rig, args, observation)
}

// ---------------------------------------------------------------------------
// initialize rows
// ---------------------------------------------------------------------------

pub fn initialize_succeeds_with_g_vault_signer_and_matching_programdata() -> Vec<Observation> {
    let mut rig = Rig::uninitialized(DEVNET_DOMAIN);
    for address in [
        rig.rt.pool_address(),
        rig.rt.admission_address(),
        rig.rt.game_vault_address(),
        rig.rt.ops_address(),
        rig.rt.ticket_address(),
        rig.rt.rng_authority_address(),
        rig.rt.config_address(1),
    ] {
        assert!(
            rig.rt.svm.get_account(&address).is_none(),
            "{address} must not exist before initialize"
        );
    }
    let args = initialize_args(&rig.rt, DEVNET_DOMAIN);
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(
        label(ROW_INITIALIZE_SUCCEEDS, ""),
        instruction,
        &[&sponsor()],
    );
    rig.ok(&observation);
    assert_genesis_accounts(&rig.rt, &args);
    // The programdata authority, not the caller, is what bound this genesis:
    // the loader fixture's authority is the same G vault that signed.
    let programdata = rig.rt.account(rig.rt.program_data_address());
    assert_eq!(
        &programdata.data[..45],
        fixtures::loader::governance_programdata_header().as_slice()
    );
    vec![observation]
}

pub fn initialize_rejects_first_caller_without_g_vault() -> Vec<Observation> {
    let mut rig = Rig::uninitialized(DEVNET_DOMAIN);
    let args = initialize_args(&rig.rt, DEVNET_DOMAIN);
    let mut observations = Vec::new();

    // A plain keypair in the `governance` slot signs, so Anchor's `Signer`
    // constraint passes and the handler's `require_vault_signer` rejects it:
    // `NotAuthorized` (6004), not a framework error.
    let mut forged = initialize_accounts(&rig.rt);
    forged.governance = attacker().pubkey();
    let instruction = anchor_ix(forged, instruction::Initialize { args });
    let observation = rig.direct(
        label(ROW_INITIALIZE_FIRST_CALLER, ""),
        instruction,
        &[&sponsor(), &attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    // The genuine G vault listed but NOT signing (a direct transaction cannot
    // sign for a PDA): Anchor's `Signer` check fires before the handler.
    let g_vault = rig.rt.g_vault();
    let mut instruction = initialize_ix(&rig.rt, args);
    for meta in instruction.accounts.iter_mut() {
        if meta.pubkey == g_vault {
            meta.is_signer = false;
        }
    }
    let observation = rig.direct(
        label(ROW_INITIALIZE_FIRST_CALLER, "vault_unsigned"),
        instruction,
        &[&sponsor()],
    );
    rig.rejected(&observation, account_not_signer());
    observations.push(observation);

    // The P vault (a valid vault of a different multisig) in the G slot.
    let p_multisig = rig.rt.p_multisig();
    let p_vault = rig.rt.p_vault();
    let mut wrong_vault = initialize_accounts(&rig.rt);
    wrong_vault.governance = p_vault;
    let instruction = anchor_ix(wrong_vault, instruction::Initialize { args });
    let observation = rig.via(
        label(ROW_INITIALIZE_FIRST_CALLER, "p_vault_in_g_slot"),
        p_multisig,
        instruction,
        &[&sponsor()],
    );
    // The G multisig is supplied, but the vault that signed is P's:
    // `governance` != the G vault derived from the G multisig.
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    assert!(
        !rig.initialized(),
        "no rejected initialize may create the Pool"
    );
    observations
}

pub fn initialize_rejects_programdata_authority_mismatch() -> Vec<Observation> {
    let mut rig = Rig::uninitialized(DEVNET_DOMAIN);
    let args = initialize_args(&rig.rt, DEVNET_DOMAIN);
    let mut observations = Vec::new();

    for (variant, authority) in [
        ("", Some(attacker().pubkey())),
        ("upgrade_authority_none", None),
        ("authority_is_guardian_vault", Some(rig.rt.p_vault())),
    ] {
        rig.rt.set_programdata_authority(authority);
        let instruction = initialize_ix(&rig.rt, args);
        let observation = rig.via_g(
            label(ROW_INITIALIZE_PROGRAMDATA, variant),
            instruction,
            &[&sponsor()],
        );
        rig.rejected_program(&observation, ErrorCode::NotAuthorized);
        observations.push(observation);
        assert!(!rig.initialized());
    }

    // Control: with the authority restored to the G vault the very same
    // transaction succeeds, so the rejections above were the authority check.
    let governance = rig.rt.g_vault();
    rig.rt.set_programdata_authority(Some(governance));
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(
        label(ROW_INITIALIZE_PROGRAMDATA, "control_authority_restored"),
        instruction,
        &[&sponsor()],
    );
    rig.ok(&observation);
    observations.push(observation);
    observations
}

pub fn initialize_rejects_one_of_three_profile() -> Vec<Observation> {
    let mut rig = Rig::uninitialized(DEVNET_DOMAIN);
    let args = initialize_args(&rig.rt, DEVNET_DOMAIN);
    let g_multisig = rig.rt.g_multisig();
    let p_multisig = rig.rt.p_multisig();
    let delay = fixtures::squads::GOVERNANCE_TIME_LOCK;
    let mut observations = Vec::new();

    // Headline: a 1-of-3 G multisig.
    rig.rt.install_squads_account(
        g_multisig,
        SQUADS_PROGRAM_ID,
        tweak(G_CREATE_KEY, delay, |spec| spec.threshold = 1),
    );
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(
        label(ROW_INITIALIZE_PROFILE, ""),
        instruction,
        &[&sponsor()],
    );
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);

    // Other shape failures of the G account: every one is
    // `WrongGovernanceProfile` (6031).
    let shape_cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "threshold_zero",
            tweak(G_CREATE_KEY, delay, |s| s.threshold = 0),
        ),
        (
            "threshold_three",
            tweak(G_CREATE_KEY, delay, |s| s.threshold = 3),
        ),
        ("time_lock_zero", tweak(G_CREATE_KEY, 0, |_| {})),
        (
            "time_lock_one_second_short",
            tweak(G_CREATE_KEY, delay - 1, |_| {}),
        ),
        (
            "time_lock_one_second_long",
            tweak(G_CREATE_KEY, delay + 1, |_| {}),
        ),
        (
            "two_members",
            tweak(G_CREATE_KEY, delay, |s| s.members.truncate(2)),
        ),
        (
            "four_members",
            tweak(G_CREATE_KEY, delay, |s| {
                s.members
                    .push((key(0xAA), fixtures::squads::ALL_PERMISSIONS))
            }),
        ),
        (
            "config_authority_set",
            tweak(G_CREATE_KEY, delay, |s| s.config_authority = key(0xAB)),
        ),
        (
            "member_cannot_vote",
            tweak(G_CREATE_KEY, delay, |s| s.members[0].1 = 5),
        ),
        (
            "duplicate_member",
            tweak(G_CREATE_KEY, delay, |s| s.members[1].0 = s.members[0].0),
        ),
        (
            "nobody_can_execute",
            tweak(G_CREATE_KEY, delay, |s| {
                for member in s.members.iter_mut() {
                    member.1 = 3;
                }
            }),
        ),
        ("truncated_body", {
            let mut bytes = tweak(G_CREATE_KEY, delay, |_| {});
            bytes.truncate(60);
            bytes
        }),
    ];
    for (variant, bytes) in shape_cases {
        rig.rt
            .install_squads_account(g_multisig, SQUADS_PROGRAM_ID, bytes);
        let instruction = initialize_ix(&rig.rt, args);
        let observation = rig.via_g(
            label(ROW_INITIALIZE_PROFILE, variant),
            instruction,
            &[&sponsor()],
        );
        rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
        observations.push(observation);
    }

    // Wrong Squads owner / discriminator / PDA / bump: `WrongAccount` (6005).
    let canonical = tweak(G_CREATE_KEY, delay, |_| {});
    let mut bad_discriminator = canonical.clone();
    bad_discriminator[0] ^= 0xFF;
    let account_cases: Vec<(&str, Pubkey, Vec<u8>)> = vec![
        (
            "wrong_owner_system_program",
            system_program::ID,
            canonical.clone(),
        ),
        ("wrong_owner_main_program", ID, canonical.clone()),
        ("bad_discriminator", SQUADS_PROGRAM_ID, bad_discriminator),
        (
            "bad_bump",
            SQUADS_PROGRAM_ID,
            tweak(G_CREATE_KEY, delay, |s| s.bump_offset = 1),
        ),
        (
            "bytes_of_another_create_key",
            SQUADS_PROGRAM_ID,
            tweak([0x49; 32], delay, |_| {}),
        ),
        ("empty_account", SQUADS_PROGRAM_ID, Vec::new()),
    ];
    for (variant, owner_program, bytes) in account_cases {
        rig.rt
            .install_squads_account(g_multisig, owner_program, bytes);
        let instruction = initialize_ix(&rig.rt, args);
        let observation = rig.via_g(
            label(ROW_INITIALIZE_PROFILE, variant),
            instruction,
            &[&sponsor()],
        );
        rig.rejected_program(&observation, ErrorCode::WrongAccount);
        observations.push(observation);
    }

    // The guardian side: its profile is checked too (immediate 2-of-3).
    rig.restore_canonical_multisigs();
    for (variant, bytes, expected) in [
        (
            "guardian_threshold_one",
            tweak(P_CREATE_KEY, 0, |s| s.threshold = 1),
            ErrorCode::WrongGovernanceProfile,
        ),
        (
            "guardian_time_lock_set",
            tweak(P_CREATE_KEY, 60, |_| {}),
            ErrorCode::WrongGovernanceProfile,
        ),
        (
            "guardian_bad_discriminator",
            {
                let mut bytes = tweak(P_CREATE_KEY, 0, |_| {});
                bytes[0] ^= 0xFF;
                bytes
            },
            ErrorCode::WrongAccount,
        ),
    ] {
        rig.rt
            .install_squads_account(p_multisig, SQUADS_PROGRAM_ID, bytes);
        let instruction = initialize_ix(&rig.rt, args);
        let observation = rig.via_g(
            label(ROW_INITIALIZE_PROFILE, variant),
            instruction,
            &[&sponsor()],
        );
        rig.rejected_program(&observation, expected);
        observations.push(observation);
    }

    // A guardian account that is not the vault derived from the guardian
    // multisig.
    rig.restore_canonical_multisigs();
    let mut mismatched = initialize_accounts(&rig.rt);
    mismatched.guardian = owner().pubkey();
    let instruction = anchor_ix(mismatched, instruction::Initialize { args });
    let observation = rig.via_g(
        label(ROW_INITIALIZE_PROFILE, "guardian_vault_mismatch"),
        instruction,
        &[&sponsor()],
    );
    rig.rejected_program(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // Control: with both canonical fixtures back the same call succeeds.
    assert!(
        !rig.initialized(),
        "no rejected initialize may create the Pool"
    );
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(
        label(ROW_INITIALIZE_PROFILE, "control_canonical_profiles"),
        instruction,
        &[&sponsor()],
    );
    rig.ok(&observation);
    observations.push(observation);
    observations
}

pub fn initialize_sets_pot_seed_zero_round_one_next_close_one() -> Vec<Observation> {
    let (rig, args, observation) = genesis(DEVNET_DOMAIN, ROW_INITIALIZE_GENESIS, "");
    assert_genesis_counters(&rig.pool(), rig.rt.rent(state::GameVault::SPACE));
    assert_genesis_accounts(&rig.rt, &args);
    // `initialize` moved no game money: the vault holds only its rent floor.
    assert_eq!(
        rig.rt.account(rig.rt.game_vault_address()).lamports,
        rig.rt.rent(state::GameVault::SPACE)
    );
    vec![observation]
}

/// Real Squads accounts are allocated at `Multisig::size(n)`, which reserves
/// 33 bytes for `rent_collector` even when it is `None`.  The committed
/// fixtures are the exact unpadded layout; this row proves the profile reader
/// also accepts the allocation-padded `None` shape (G) and a populated
/// `rent_collector` (P).
pub fn initialize_accepts_allocation_padded_multisig() -> Vec<Observation> {
    let mut rig = Rig::uninitialized(DEVNET_DOMAIN);
    let g_multisig = rig.rt.g_multisig();
    let p_multisig = rig.rt.p_multisig();
    let padded_g = fixtures::squads::pad_to_allocation(
        tweak(G_CREATE_KEY, fixtures::squads::GOVERNANCE_TIME_LOCK, |_| {}),
        fixtures::squads::MEMBER_COUNT,
    );
    assert_eq!(padded_g.len(), fixtures::squads::multisig_space(3));
    assert_eq!(
        &padded_g[..fixtures::squads::committed_governance().len()],
        fixtures::squads::committed_governance().as_slice()
    );
    let collector_p = tweak(P_CREATE_KEY, fixtures::squads::GUARDIAN_TIME_LOCK, |s| {
        s.rent_collector = Some(key(0xCC))
    });
    assert_eq!(collector_p.len(), fixtures::squads::multisig_space(3));
    rig.rt
        .install_squads_account(g_multisig, SQUADS_PROGRAM_ID, padded_g);
    rig.rt
        .install_squads_account(p_multisig, SQUADS_PROGRAM_ID, collector_p);
    let args = initialize_args(&rig.rt, DEVNET_DOMAIN);
    let instruction = initialize_ix(&rig.rt, args);
    let observation = rig.via_g(label(ROW_INITIALIZE_PADDED, ""), instruction, &[&sponsor()]);
    rig.ok(&observation);
    assert_genesis_accounts(&rig.rt, &args);
    vec![observation]
}

// ---------------------------------------------------------------------------
// set_pause rows
// ---------------------------------------------------------------------------

pub fn set_admission_pause_touches_only_admission_account() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let admission_address = rig.rt.admission_address();
    let p_vault = rig.rt.p_vault();
    let payer = rig.rt.payer.pubkey();
    let before = rig.admission();
    assert!(!before.paused);

    let inner = pause_ix(&rig.rt, p_vault, true);
    assert_eq!(
        metas(&inner),
        vec![
            (admission_address, false, true),
            (p_vault, true, false),
            (rig.rt.event_authority_address(), false, false),
            (ID, false, false),
        ],
        "set_pause's account list is exactly [Admission (writable), P vault (signer)] + the two event-cpi accounts"
    );
    let observation = rig.via_p(label(ROW_PAUSE_TOUCHES_ONLY_ADMISSION, ""), inner, &[]);
    rig.ok(&observation);
    assert_eq!(
        changed(&observation),
        keys(&[admission_address, payer]),
        "only Admission and the fee payer change"
    );
    assert_eq!(
        delta(&observation, admission_address),
        0,
        "Admission lamports are untouched"
    );
    assert_eq!(
        observation.changed_data_bytes, 1,
        "only Admission.paused flips (changed_at is 0 at clock 0)"
    );
    let after = rig.admission();
    assert!(after.paused);
    assert_eq!(after.guardian_vault, before.guardian_vault);
    assert_eq!(after.changed_at, 0);

    // The P vault can also lift the pause.
    let inner = pause_ix(&rig.rt, p_vault, false);
    let lifted = rig.via_p(
        label(ROW_PAUSE_TOUCHES_ONLY_ADMISSION, "unpause"),
        inner,
        &[],
    );
    rig.ok(&lifted);
    assert!(!rig.admission().paused);
    vec![observation, lifted]
}

pub fn set_admission_pause_rejects_non_guardian() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let mut observations = Vec::new();

    // A plain keypair signing as the guardian.
    let inner = pause_ix(&rig.rt, attacker().pubkey(), true);
    let observation = rig.direct(
        label(ROW_PAUSE_REJECTS_NON_GUARDIAN, ""),
        inner,
        &[&attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    // The G vault: governance cannot pause, only the guardian can.
    let g_vault = rig.rt.g_vault();
    let inner = pause_ix(&rig.rt, g_vault, true);
    let observation = rig.via_g(label(ROW_PAUSE_REJECTS_NON_GUARDIAN, "g_vault"), inner, &[]);
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    // A structurally valid P multisig that is not the recorded guardian.
    let (rogue_multisig, rogue_vault) = rig.p_profile(ROGUE_P_CREATE_KEY);
    let inner = pause_ix(&rig.rt, rogue_vault, true);
    let observation = rig.via(
        label(ROW_PAUSE_REJECTS_NON_GUARDIAN, "rogue_p_vault"),
        rogue_multisig,
        inner,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    // The genuine P vault listed but not signing: Anchor's `Signer` check.
    let p_vault = rig.rt.p_vault();
    let mut inner = pause_ix(&rig.rt, p_vault, true);
    for meta in inner.accounts.iter_mut() {
        if meta.pubkey == p_vault {
            meta.is_signer = false;
        }
    }
    let observation = rig.direct(
        label(ROW_PAUSE_REJECTS_NON_GUARDIAN, "vault_unsigned"),
        inner,
        &[],
    );
    rig.rejected(&observation, account_not_signer());
    observations.push(observation);

    assert!(
        !rig.admission().paused,
        "no rejected pause may flip Admission.paused"
    );
    observations
}

// ---------------------------------------------------------------------------
// rotate_guardian rows
// ---------------------------------------------------------------------------

pub fn rotate_guardian_requires_g_and_valid_p_profile_then_old_p_rejected_new_p_accepted(
) -> Vec<Observation> {
    let mut rig = Rig::governed();
    let g_multisig = rig.rt.g_multisig();
    let g_vault = rig.rt.g_vault();
    let old_p_multisig = rig.rt.p_multisig();
    let old_p_vault = rig.rt.p_vault();
    let (new_p_multisig, new_p_vault) = rig.p_profile(NEW_P_CREATE_KEY);
    let mut observations = Vec::new();

    // Not G: a plain keypair, a structurally valid but unrecorded G vault, and
    // the P vault itself (wrong profile entirely).
    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, attacker().pubkey(), new_p_multisig);
    let observation = rig.direct(
        label(ROW_ROTATE_GUARDIAN, "non_g_keypair"),
        instruction,
        &[&attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let (rogue_multisig, rogue_vault) = rig.rogue_g();
    let instruction = rotate_guardian_ix(&rig.rt, rogue_multisig, rogue_vault, new_p_multisig);
    let observation = rig.via(
        label(ROW_ROTATE_GUARDIAN, "rogue_g_vault"),
        rogue_multisig,
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let instruction = rotate_guardian_ix(&rig.rt, old_p_multisig, old_p_vault, new_p_multisig);
    let observation = rig.via_p(label(ROW_ROTATE_GUARDIAN, "p_vault_as_g"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);

    // G, but the replacement is not a valid P profile: a G-profile multisig
    // (time lock 172,800) and an account that is not a Squads multisig.
    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, g_multisig);
    let observation = rig.via_g(
        label(ROW_ROTATE_GUARDIAN, "new_guardian_has_time_lock"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);

    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, owner().pubkey());
    let observation = rig.via_g(
        label(ROW_ROTATE_GUARDIAN, "new_guardian_not_squads"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    let (one_of_three, _) = rig.p_profile([0x53; 32]);
    rig.rt.install_squads_account(
        one_of_three,
        SQUADS_PROGRAM_ID,
        tweak([0x53; 32], 0, |s| s.threshold = 1),
    );
    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, one_of_three);
    let observation = rig.via_g(
        label(ROW_ROTATE_GUARDIAN, "new_guardian_one_of_three"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);

    assert_eq!(
        rig.admission().guardian_vault,
        old_p_vault,
        "rejections leave the guardian alone"
    );

    // G rotates to the new P in ONE instruction: both copies change together.
    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, new_p_multisig);
    let observation = rig.via_g(label(ROW_ROTATE_GUARDIAN, ""), instruction, &[]);
    rig.ok(&observation);
    assert_eq!(rig.admission().guardian_vault, new_p_vault);
    assert_eq!(rig.pool().guardian_vault, new_p_vault);
    assert_eq!(
        changed(&observation),
        keys(&[
            rig.rt.admission_address(),
            rig.rt.pool_address(),
            rig.rt.payer.pubkey()
        ]),
        "rotate_guardian writes Admission and Pool only"
    );
    observations.push(observation);

    // The old P is rejected from the next instruction; the new P is accepted.
    let inner = pause_ix(&rig.rt, old_p_vault, true);
    let observation = rig.via_p(label(ROW_ROTATE_GUARDIAN, "old_p_rejected"), inner, &[]);
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);
    assert!(!rig.admission().paused);

    let inner = pause_ix(&rig.rt, new_p_vault, true);
    let observation = rig.via(
        label(ROW_ROTATE_GUARDIAN, "new_p_accepted"),
        new_p_multisig,
        inner,
        &[],
    );
    rig.ok(&observation);
    assert!(rig.admission().paused);
    observations.push(observation);
    observations
}

pub fn rotate_guardian_recovers_lost_p_quorum() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let g_multisig = rig.rt.g_multisig();
    let g_vault = rig.rt.g_vault();
    let old_p_vault = rig.rt.p_vault();
    let (new_p_multisig, new_p_vault) = rig.p_profile(NEW_P_CREATE_KEY);
    let mut observations = Vec::new();

    // The original P pauses admissions, and then its quorum is lost: from here
    // on it never signs again.
    let inner = pause_ix(&rig.rt, old_p_vault, true);
    let paused = rig.via_p(
        label(ROW_ROTATE_GUARDIAN_RECOVERY, "pause_before_loss"),
        inner,
        &[],
    );
    rig.ok(&paused);
    assert!(rig.admission().paused);
    observations.push(paused);

    // G alone rotates to a fresh P.
    let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, new_p_multisig);
    let rotated = rig.via_g(label(ROW_ROTATE_GUARDIAN_RECOVERY, ""), instruction, &[]);
    rig.ok(&rotated);
    assert_eq!(rig.admission().guardian_vault, new_p_vault);
    assert_eq!(rig.pool().guardian_vault, new_p_vault);
    assert!(
        rig.admission().paused,
        "rotation does not itself lift the pause"
    );
    observations.push(rotated);

    // The replacement P lifts the pause the lost quorum left behind.
    let inner = pause_ix(&rig.rt, new_p_vault, false);
    let lifted = rig.via(
        label(ROW_ROTATE_GUARDIAN_RECOVERY, "new_p_lifts_pause"),
        new_p_multisig,
        inner,
        &[],
    );
    rig.ok(&lifted);
    assert!(!rig.admission().paused);
    observations.push(lifted);

    // And the lost P can no longer act.
    let inner = pause_ix(&rig.rt, old_p_vault, true);
    let stale = rig.via_p(
        label(ROW_ROTATE_GUARDIAN_RECOVERY, "old_p_cannot_act"),
        inner,
        &[],
    );
    rig.rejected_program(&stale, ErrorCode::NotAuthorized);
    assert!(!rig.admission().paused);
    observations.push(stale);
    observations
}

// ---------------------------------------------------------------------------
// Money-moving rows
// ---------------------------------------------------------------------------

pub fn withdraw_management_pays_recipient_only_and_caps_at_accrued() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let recipient = rig.rt.management.pubkey();
    let game_vault = rig.rt.game_vault_address();
    let pool_address = rig.rt.pool_address();
    let payer = rig.rt.payer.pubkey();
    let mut observations = Vec::new();

    // Fixture write: 1,000 lamports accrued to management, backed by 1,000
    // real lamports in the game vault so conservation keeps holding.
    rig.set_pool(|pool| pool.management = 1_000);
    rig.rt.add_lamports(game_vault, 1_000);
    rig.rt.conservation_check();

    let over = withdraw_management_ix(&rig.rt, recipient, 1_001);
    let observation = rig.direct(
        label(ROW_WITHDRAW_MANAGEMENT, "over_accrued"),
        over,
        &[&management()],
    );
    rig.rejected_program(&observation, ErrorCode::InsufficientManagement);
    observations.push(observation);

    // Not the recorded recipient (a different signer): rejected.
    let wrong = withdraw_management_ix(&rig.rt, attacker().pubkey(), 1);
    let observation = rig.direct(
        label(ROW_WITHDRAW_MANAGEMENT, "not_the_recipient"),
        wrong,
        &[&attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let before = rig.pool();
    let exact = withdraw_management_ix(&rig.rt, recipient, 1_000);
    let observation = rig.direct(label(ROW_WITHDRAW_MANAGEMENT, ""), exact, &[&management()]);
    rig.ok(&observation);
    assert_eq!(
        delta(&observation, game_vault),
        -1_000,
        "the vault pays exactly the accrued amount"
    );
    assert_eq!(
        delta(&observation, recipient),
        1_000,
        "the recipient receives exactly the accrued amount"
    );
    assert_eq!(
        changed(&observation),
        keys(&[pool_address, game_vault, recipient, payer]),
        "only Pool, the game vault, the recipient and the fee payer change"
    );
    let mut expected = before.clone();
    expected.management = 0;
    assert_eq!(
        ser(&rig.pool()),
        ser(&expected),
        "only Pool.management changed"
    );
    observations.push(observation);

    // The accrual is spent: nothing left to withdraw.
    let again = withdraw_management_ix(&rig.rt, recipient, 1);
    let observation = rig.direct(
        label(ROW_WITHDRAW_MANAGEMENT, "after_drain"),
        again,
        &[&management()],
    );
    rig.rejected_program(&observation, ErrorCode::InsufficientManagement);
    observations.push(observation);
    observations
}

pub fn fund_operations_any_signer_increases_free_balance_only() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let ops_address = rig.rt.ops_address();
    let ops_cash = rig.rt.ops_cash_address();
    let payer = rig.rt.payer.pubkey();
    let mut observations = Vec::new();

    // Neither funder is the sponsor: any signer may fund operations.
    let attacker_key = attacker();
    let before = rig.ops();
    let observation = rig.fund(label(ROW_FUND_OPERATIONS, ""), &attacker_key, 7_000);
    rig.ok(&observation);
    let mut expected = before.clone();
    expected.free_balance += 7_000;
    expected.total_contributed += 7_000u128;
    assert_eq!(
        ser(&rig.ops()),
        ser(&expected),
        "only Ops.free_balance and Ops.total_contributed changed"
    );
    assert_eq!(
        changed(&observation),
        keys(&[attacker_key.pubkey(), ops_address, ops_cash, payer]),
        "only the funder, Ops, OpsCash and the fee payer change"
    );
    assert_eq!(delta(&observation, attacker_key.pubkey()), -7_000);
    assert_eq!(delta(&observation, ops_cash), 7_000);
    observations.push(observation);

    let owner_key = owner();
    let observation = rig.fund(
        label(ROW_FUND_OPERATIONS, "second_funder"),
        &owner_key,
        3_000,
    );
    rig.ok(&observation);
    assert_eq!(rig.ops().free_balance, 10_000);
    assert_eq!(rig.rt.account(ops_cash).lamports, rig.rt.rent(0) + 10_000);
    let funded = rig.ops();
    assert_eq!(
        funded.total_contributed,
        before.total_contributed + 7_000u128 + 3_000u128,
        "total_contributed is the sum of both fundings"
    );
    assert_eq!(
        funded.total_withdrawn, before.total_withdrawn,
        "funding never touches total_withdrawn"
    );
    assert_eq!(funded.total_withdrawn, 0, "nothing has been withdrawn");
    observations.push(observation);

    // A zero-lamport contribution is rejected before the transfer CPI
    // (`InvalidState`, 6001); Ops is untouched and only the fee moves.
    let ops_before = ser(&rig.ops());
    let observation = rig.fund(label(ROW_FUND_OPERATIONS, "zero_amount"), &owner_key, 0);
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    assert_eq!(custom_code(&observation), Some(6001));
    assert_eq!(
        ser(&rig.ops()),
        ops_before,
        "a rejected zero funding leaves Ops byte-identical"
    );
    observations.push(observation);
    observations
}

pub fn withdraw_unreserved_operations_respects_locked_and_rent() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let ops_cash = rig.rt.ops_cash_address();
    let sponsor_key = sponsor();
    let sponsor_address = sponsor_key.pubkey();
    let mut observations = Vec::new();

    let initial = rig.ops();
    let funded = rig.fund(
        label(ROW_WITHDRAW_OPERATIONS, "fund_20000"),
        &sponsor_key,
        20_000,
    );
    rig.ok(&funded);
    assert_eq!(
        rig.ops().total_contributed,
        initial.total_contributed + 20_000u128,
        "the setup funding is recorded in total_contributed"
    );
    assert_eq!(rig.ops().total_withdrawn, initial.total_withdrawn);
    observations.push(funded);

    // Fixture write: 5,000 lamports locked for a close, physically held in
    // OpsCash so `cash - rent == free + locked` still holds.
    let mut ops = rig.ops();
    ops.locked_remaining_total = 5_000;
    rig.rt.replace_ops(ops);
    rig.rt.add_lamports(ops_cash, 5_000);
    rig.rt.conservation_check();

    // Both lifetime counters as of now; every rejected withdrawal below must
    // leave them byte-identical (rollback regression).
    let counters_before = counter_bytes(&rig.ops());

    let by_other = withdraw_operations_ix(&rig.rt, attacker().pubkey(), 1);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, "not_the_sponsor"),
        by_other,
        &[&attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    assert_eq!(counter_bytes(&rig.ops()), counters_before);
    observations.push(observation);

    // free (20,000) + locked (5,000) is physically in OpsCash, but only the
    // unreserved free balance may leave.
    let reserved = withdraw_operations_ix(&rig.rt, sponsor_address, 25_000);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, ""),
        reserved,
        &[&sponsor_key],
    );
    rig.rejected_program(&observation, ErrorCode::InsufficientOperations);
    assert_eq!(
        counter_bytes(&rig.ops()),
        counters_before,
        "a rejected over-free withdrawal leaves both counters byte-identical"
    );
    observations.push(observation);

    let one_over = withdraw_operations_ix(&rig.rt, sponsor_address, 20_001);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, "one_over_free"),
        one_over,
        &[&sponsor_key],
    );
    rig.rejected_program(&observation, ErrorCode::InsufficientOperations);
    assert_eq!(
        counter_bytes(&rig.ops()),
        counters_before,
        "a rejected one-over withdrawal leaves both counters byte-identical"
    );
    observations.push(observation);

    let all_free = withdraw_operations_ix(&rig.rt, sponsor_address, 20_000);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, "all_free"),
        all_free,
        &[&sponsor_key],
    );
    rig.ok(&observation);
    assert_eq!(delta(&observation, sponsor_address), 20_000);
    assert_eq!(delta(&observation, ops_cash), -20_000);
    assert_eq!(rig.ops().free_balance, 0);
    assert_eq!(rig.ops().locked_remaining_total, 5_000);
    assert_eq!(
        rig.ops().total_withdrawn,
        initial.total_withdrawn + 20_000u128,
        "total_withdrawn equals the withdrawn amount"
    );
    assert_eq!(
        rig.ops().total_contributed,
        initial.total_contributed + 20_000u128,
        "withdrawing never touches total_contributed"
    );
    assert_eq!(
        rig.rt.account(ops_cash).lamports,
        rig.rt.rent(0) + 5_000,
        "OpsCash keeps its rent floor plus the locked reserve"
    );
    observations.push(observation);

    let counters_drained = counter_bytes(&rig.ops());
    let after = withdraw_operations_ix(&rig.rt, sponsor_address, 1);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, "after_drain"),
        after,
        &[&sponsor_key],
    );
    rig.rejected_program(&observation, ErrorCode::InsufficientOperations);
    assert_eq!(
        counter_bytes(&rig.ops()),
        counters_drained,
        "a rejected post-drain withdrawal leaves both counters byte-identical"
    );
    observations.push(observation);

    // Zero-amount withdrawal: the spec does not forbid it, so this is an
    // observation, not an asserted error code.  Whether it succeeds (a
    // no-op) or is rejected, Ops.free_balance and both counters are unchanged.
    let ops_free_before = rig.ops().free_balance;
    let zero = withdraw_operations_ix(&rig.rt, sponsor_address, 0);
    let observation = rig.direct(
        label(ROW_WITHDRAW_OPERATIONS, "zero_amount"),
        zero,
        &[&sponsor_key],
    );
    if observation.success {
        rig.ok(&observation);
    } else {
        observation.assert_rollback_except_fee();
        rig.rt.conservation_check();
    }
    assert_eq!(rig.ops().free_balance, ops_free_before);
    assert_eq!(
        counter_bytes(&rig.ops()),
        counters_drained,
        "a zero-amount withdrawal changes neither counter"
    );
    observations.push(observation);

    // A lock that is not physically backed breaks the OpsCash identity;
    // `sync_ops` refuses to operate on it (fail closed), `InvalidState`.
    let mut broken = Rig::governed();
    let funded = broken.fund(
        label(ROW_WITHDRAW_OPERATIONS, "unbacked_lock.fund"),
        &sponsor_key,
        20_000,
    );
    broken.ok(&funded);
    observations.push(funded);
    let mut ops = broken.ops();
    ops.locked_remaining_total = 5_000;
    broken.rt.replace_ops(ops);
    let broken_counters = counter_bytes(&broken.ops());
    let unbacked = withdraw_operations_ix(&broken.rt, sponsor_address, 1);
    let observation = broken.direct(
        label(ROW_WITHDRAW_OPERATIONS, "unbacked_lock"),
        unbacked,
        &[&sponsor_key],
    );
    broken.rejected_unbalanced(&observation, code(ErrorCode::InvalidState));
    assert_eq!(
        counter_bytes(&broken.ops()),
        broken_counters,
        "a fail-closed rejection leaves both counters byte-identical"
    );
    observations.push(observation);
    observations
}

// ---------------------------------------------------------------------------
// Configuration rows
// ---------------------------------------------------------------------------

pub fn stage_config_requires_g_and_future_round() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let g_multisig = rig.rt.g_multisig();
    let g_vault = rig.rt.g_vault();
    let mut observations = Vec::new();

    // Staging pays the new account's rent from the operations free balance.
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, terms(2));
    let observation = rig.via_g(label(ROW_STAGE_CONFIG, "unfunded_ops"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::InsufficientOperations);
    observations.push(observation);

    let funded = rig.fund(label(ROW_STAGE_CONFIG, "fund_ops"), &sponsor(), 10_000_000);
    rig.ok(&funded);
    observations.push(funded);

    // Headline: the effective round must be strictly after the current one.
    // Round 1 is current, so effective round 1 is not in the future:
    // `ConfigNotActivatable` (6030).
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, terms(1));
    let observation = rig.via_g(label(ROW_STAGE_CONFIG, ""), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);

    // Not G.
    let instruction = stage_ix(&rig.rt, g_multisig, attacker().pubkey(), 2, terms(2));
    let observation = rig.direct(
        label(ROW_STAGE_CONFIG, "non_g_keypair"),
        instruction,
        &[&attacker()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let (rogue_multisig, rogue_vault) = rig.rogue_g();
    let instruction = stage_ix(&rig.rt, rogue_multisig, rogue_vault, 2, terms(2));
    let observation = rig.via(
        label(ROW_STAGE_CONFIG, "rogue_g_vault"),
        rogue_multisig,
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let p_multisig = rig.rt.p_multisig();
    let p_vault = rig.rt.p_vault();
    let instruction = stage_ix(&rig.rt, p_multisig, p_vault, 2, terms(2));
    let observation = rig.via_p(label(ROW_STAGE_CONFIG, "p_vault_as_g"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);

    // Versions are sequential: only `active + 1` may be staged.
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 3, terms(2));
    let observation = rig.via_g(label(ROW_STAGE_CONFIG, "version_gap"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // Terms are validated (the three shares must cover the allocation).
    let mut invalid = terms(2);
    invalid.a_bps += 1;
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, invalid);
    let observation = rig.via_g(label(ROW_STAGE_CONFIG, "invalid_terms"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // The quota is frozen at three successes per 3,600 s (spec §3.1) because
    // `User::record_success` enforces exactly that; a staged quota of one, or a
    // different window, is rejected at staging instead of being silently
    // ignored at admission (Codex M5 review).
    let mut quota_count = terms(2);
    quota_count.quota_count = 1;
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, quota_count);
    let observation = rig.via_g(
        label(ROW_STAGE_CONFIG, "noncanonical_quota_count"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    observations.push(observation);
    let mut quota_window = terms(2);
    quota_window.quota_window_s = 3_599;
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, quota_window);
    let observation = rig.via_g(
        label(ROW_STAGE_CONFIG, "noncanonical_quota_window"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // Five more fields are consumed by fixed helpers (Simple rate, A/B
    // entitlement multipliers, seed half, open cutoff); a staged value other
    // than the frozen one is rejected at staging with fee-only rollback
    // (Codex M6 follow-up).
    let pinned: [(&str, fn(&mut ConfigTerms)); 7] = [
        ("noncanonical_simple_rate", |t| t.plan_rates_bps_per_day[0] = 1_000),
        ("noncanonical_a_entitlement", |t| t.a_entitlement_bps = 12_000),
        ("noncanonical_b_entitlement", |t| t.b_entitlement_bps = 12_000),
        ("noncanonical_seed_half", |t| t.seed_bps = 4_000),
        ("noncanonical_open_cutoff", |t| t.open_cutoff = 100),
        // The promotion window is frozen at 600 s (spec section 4); the closing
        // payout copies the staged value, so 599/601 are rejected at staging
        // (Codex M7 review, IMPL-42).
        ("noncanonical_deadline_short", |t| t.deadline_seconds = 599),
        ("noncanonical_deadline_long", |t| t.deadline_seconds = 601),
    ];
    for (variant, mutate) in pinned {
        let mut altered = terms(2);
        mutate(&mut altered);
        let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, altered);
        let observation = rig.via_g(label(ROW_STAGE_CONFIG, variant), instruction, &[]);
        rig.rejected_program(&observation, ErrorCode::InvalidState);
        observations.push(observation);
    }

    // G stages version 2 for round 2.
    let free_before = rig.ops().free_balance;
    let rent_funded_before = rig.ops().total_rent_funded;
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, terms(2));
    let observation = rig.via_g(label(ROW_STAGE_CONFIG, "accepted"), instruction, &[]);
    rig.ok(&observation);
    let staged = rig.config(2);
    assert_eq!(staged.version, 2);
    assert_eq!(staged.effective_round_id, 2);
    assert_eq!(staged.activated_at, None, "staging never activates");
    assert_eq!(staged.staged_at, 0);
    assert_eq!(
        ConfigTerms::from(&staged),
        terms(2),
        "the stored terms are exactly the staged terms"
    );
    assert_eq!(
        rig.pool().active_config_version,
        1,
        "the active version is unchanged"
    );
    let rent = rig.rt.rent(state::ConfigVersion::SPACE);
    assert_eq!(
        rig.ops().free_balance,
        free_before - rent,
        "the operations free balance pays exactly the new account's rent"
    );
    assert_eq!(
        rig.ops().total_rent_funded,
        rent_funded_before + u128::from(rent),
        "the sponsor-funded ConfigVersion rent is added to Ops.total_rent_funded"
    );
    assert_eq!(rig.rt.account(rig.rt.config_address(2)).lamports, rent);
    assert_eq!(rig.rt.account(rig.rt.config_address(2)).owner, ID);
    observations.push(observation);

    // A staged version cannot be staged again: the target is no longer a
    // system account, so `create_shared` rejects it (`WrongOwner`, 6006).
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, terms(2));
    let observation = rig.via_g(
        label(ROW_STAGE_CONFIG, "restage_existing"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::WrongOwner);
    observations.push(observation);
    observations
}

pub fn activate_config_only_when_active_no_ticket_and_round_reached() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let g_multisig = rig.rt.g_multisig();
    let g_vault = rig.rt.g_vault();
    let mut observations = Vec::new();

    let funded = rig.fund(
        label(ROW_ACTIVATE_CONFIG, "fund_ops"),
        &sponsor(),
        10_000_000,
    );
    rig.ok(&funded);
    observations.push(funded);
    let instruction = stage_ix(&rig.rt, g_multisig, g_vault, 2, terms(2));
    let staged = rig.via_g(
        label(ROW_ACTIVATE_CONFIG, "stage_v2_for_round_2"),
        instruction,
        &[],
    );
    rig.ok(&staged);
    observations.push(staged);

    // The pool is still in round 1, so version 2 (effective round 2) is not
    // yet reachable.
    let instruction = activate_ix(&rig.rt, 2, 1);
    let observation = rig.direct(
        label(ROW_ACTIVATE_CONFIG, "round_not_reached"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);

    // A wrong `expected_current` is not activatable either.
    let instruction = activate_ix(&rig.rt, 2, 5);
    let observation = rig.direct(
        label(ROW_ACTIVATE_CONFIG, "wrong_expected_current"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);

    // Fixture write: the pool reaches round 2.
    rig.set_pool(|pool| pool.round_id = 2);

    // An allocation ticket in flight blocks activation.
    rig.set_pool(|pool| pool.active_ticket = 1);
    let instruction = activate_ix(&rig.rt, 2, 1);
    let observation = rig.direct(
        label(ROW_ACTIVATE_CONFIG, "active_ticket"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);
    rig.set_pool(|pool| pool.active_ticket = 0);

    // So does a close in progress.
    rig.set_pool(|pool| pool.current_close_id = Some(1));
    let instruction = activate_ix(&rig.rt, 2, 1);
    let observation = rig.direct(label(ROW_ACTIVATE_CONFIG, "closing"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);
    rig.set_pool(|pool| pool.current_close_id = None);

    // Activation is permissionless and deliberately allowed while admissions
    // are paused.
    rig.set_paused(true);
    let instruction = activate_ix(&rig.rt, 2, 1);
    let observation = rig.direct(label(ROW_ACTIVATE_CONFIG, ""), instruction, &[]);
    rig.ok(&observation);
    assert_eq!(rig.pool().active_config_version, 2);
    assert_eq!(rig.config(2).activated_at, Some(0));
    assert_eq!(
        changed(&observation),
        keys(&[
            rig.rt.pool_address(),
            rig.rt.config_address(2),
            rig.rt.payer.pubkey()
        ]),
        "activation writes Pool and the activated ConfigVersion only"
    );
    observations.push(observation);

    // Activating again is refused.
    let instruction = activate_ix(&rig.rt, 2, 1);
    let observation = rig.direct(label(ROW_ACTIVATE_CONFIG, "replay"), instruction, &[]);
    rig.rejected_program(&observation, ErrorCode::ConfigNotActivatable);
    observations.push(observation);
    assert_eq!(rig.pool().active_config_version, 2);
    observations
}

pub fn rotate_recipient_requires_g() -> Vec<Observation> {
    let mut rig = Rig::governed();
    let g_multisig = rig.rt.g_multisig();
    let g_vault = rig.rt.g_vault();
    let old_management = rig.rt.management.pubkey();
    let old_sponsor = rig.rt.sponsor.pubkey();
    let new_recipient = rig.rt.owner.pubkey();
    let mut observations = Vec::new();

    // Not G: a plain keypair, an unrecorded G vault, and the P vault.
    let instruction =
        rotate_recipient_ix(&rig.rt, g_multisig, attacker().pubkey(), 0, new_recipient);
    let observation = rig.direct(label(ROW_ROTATE_RECIPIENT, ""), instruction, &[&attacker()]);
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let (rogue_multisig, rogue_vault) = rig.rogue_g();
    let instruction = rotate_recipient_ix(&rig.rt, rogue_multisig, rogue_vault, 0, new_recipient);
    let observation = rig.via(
        label(ROW_ROTATE_RECIPIENT, "rogue_g_vault"),
        rogue_multisig,
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);

    let p_multisig = rig.rt.p_multisig();
    let p_vault = rig.rt.p_vault();
    let instruction = rotate_recipient_ix(&rig.rt, p_multisig, p_vault, 0, new_recipient);
    let observation = rig.via_p(
        label(ROW_ROTATE_RECIPIENT, "p_vault_as_g"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::WrongGovernanceProfile);
    observations.push(observation);
    assert_eq!(rig.pool().management_recipient, old_management);

    // Argument validation (after the G check).
    let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 2, new_recipient);
    let observation = rig.via_g(
        label(ROW_ROTATE_RECIPIENT, "unknown_kind"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 0, Pubkey::default());
    let observation = rig.via_g(
        label(ROW_ROTATE_RECIPIENT, "default_recipient"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::ProtectedDestination);
    observations.push(observation);

    let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 0, rig.rt.pool_address());
    let observation = rig.via_g(
        label(ROW_ROTATE_RECIPIENT, "protected_pool_pda"),
        instruction,
        &[],
    );
    rig.rejected_program(&observation, ErrorCode::ProtectedDestination);
    observations.push(observation);

    // G rotates the management recipient (kind 0).
    let before = rig.pool();
    let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 0, new_recipient);
    let observation = rig.via_g(
        label(ROW_ROTATE_RECIPIENT, "g_rotates_management"),
        instruction,
        &[],
    );
    rig.ok(&observation);
    assert_eq!(rig.pool().management_recipient, new_recipient);
    let mut expected = before.clone();
    expected.management_recipient = new_recipient;
    assert_eq!(
        ser(&rig.pool()),
        ser(&expected),
        "only Pool.management_recipient changed"
    );
    assert_eq!(
        changed(&observation),
        keys(&[rig.rt.pool_address(), rig.rt.payer.pubkey()]),
        "kind 0 writes Pool only"
    );
    observations.push(observation);

    // G rotates the sponsor (kind 1): Pool.sponsor and Ops.sponsor together.
    let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 1, new_recipient);
    let observation = rig.via_g(
        label(ROW_ROTATE_RECIPIENT, "g_rotates_sponsor"),
        instruction,
        &[],
    );
    rig.ok(&observation);
    assert_eq!(rig.pool().sponsor, new_recipient);
    assert_eq!(rig.ops().sponsor, new_recipient);
    assert_eq!(
        changed(&observation),
        keys(&[
            rig.rt.pool_address(),
            rig.rt.ops_address(),
            rig.rt.payer.pubkey()
        ]),
        "kind 1 writes Pool and Ops only"
    );
    observations.push(observation);

    // The previous sponsor lost its withdrawal right.
    let stale = withdraw_operations_ix(&rig.rt, old_sponsor, 1);
    let observation = rig.direct(
        label(ROW_ROTATE_RECIPIENT, "old_sponsor_rejected"),
        stale,
        &[&sponsor()],
    );
    rig.rejected_program(&observation, ErrorCode::NotAuthorized);
    observations.push(observation);
    observations
}

// ---------------------------------------------------------------------------
// cluster_domain immutability
// ---------------------------------------------------------------------------

/// Two fresh genesis runs with different `cluster_domain` args store
/// different values, and the stored value never changes: [`Rig`] re-checks it
/// after every instruction of this row (and of every other row), including a
/// second `initialize` that tries to write the other domain.
pub fn initialize_stores_cluster_domain_immutably() -> Vec<Observation> {
    assert_ne!(DEVNET_DOMAIN, MAINNET_DOMAIN);
    let (mut devnet, devnet_args, first) = genesis(DEVNET_DOMAIN, ROW_CLUSTER_DOMAIN, "");
    let (mut mainnet, mainnet_args, second) =
        genesis(MAINNET_DOMAIN, ROW_CLUSTER_DOMAIN, "mainnet_genesis");
    assert_eq!(devnet.pool().cluster_domain, DEVNET_DOMAIN);
    assert_eq!(mainnet.pool().cluster_domain, MAINNET_DOMAIN);
    assert_ne!(devnet.pool().cluster_domain, mainnet.pool().cluster_domain);
    let mut observations = vec![first, second];

    for (name, rig, args, other) in [
        ("devnet", &mut devnet, devnet_args, MAINNET_DOMAIN),
        ("mainnet", &mut mainnet, mainnet_args, DEVNET_DOMAIN),
    ] {
        // A second initialize naming the other domain cannot overwrite it.
        let mut retry = args;
        retry.cluster_domain = other;
        let instruction = initialize_ix(&rig.rt, retry);
        let observation = rig.via_g(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_reinitialize")),
            instruction,
            &[&sponsor()],
        );
        assert!(
            !observation.success,
            "re-initialize must fail: {observation:?}"
        );
        observation.assert_rollback_except_fee();
        observations.push(observation);

        // Every other control instruction leaves the domain untouched.
        let funder = attacker();
        let funded = rig.fund(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_fund")),
            &funder,
            1_000,
        );
        rig.ok(&funded);
        observations.push(funded);

        let p_vault = rig.rt.p_vault();
        let inner = pause_ix(&rig.rt, p_vault, true);
        let paused = rig.via_p(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_pause")),
            inner,
            &[],
        );
        rig.ok(&paused);
        observations.push(paused);

        let g_multisig = rig.rt.g_multisig();
        let g_vault = rig.rt.g_vault();
        let recipient = rig.rt.owner.pubkey();
        let instruction = rotate_recipient_ix(&rig.rt, g_multisig, g_vault, 0, recipient);
        let rotated = rig.via_g(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_rotate_recipient")),
            instruction,
            &[],
        );
        rig.ok(&rotated);
        observations.push(rotated);

        let (new_p_multisig, new_p_vault) = rig.p_profile(NEW_P_CREATE_KEY);
        let instruction = rotate_guardian_ix(&rig.rt, g_multisig, g_vault, new_p_multisig);
        let guardian = rig.via_g(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_rotate_guardian")),
            instruction,
            &[],
        );
        rig.ok(&guardian);
        observations.push(guardian);

        let inner = pause_ix(&rig.rt, new_p_vault, false);
        let unpaused = rig.via(
            label(ROW_CLUSTER_DOMAIN, &format!("{name}_unpause_new_guardian")),
            new_p_multisig,
            inner,
            &[],
        );
        rig.ok(&unpaused);
        observations.push(unpaused);

        assert_eq!(
            rig.pool().cluster_domain,
            args.cluster_domain,
            "{name}: domain survived every instruction"
        );
    }
    assert_ne!(devnet.pool().cluster_domain, mainnet.pool().cluster_domain);
    observations
}
