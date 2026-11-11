//! Control-plane instructions for the native Musical Chairs program.
//!
//! This module deliberately contains no financial admission or allocation
//! logic.  It binds the singleton accounts to their canonical PDAs, performs
//! the one-time authority proof, and owns the separate operations and
//! admission-control surfaces.

use anchor_lang::prelude::*;
use anchor_lang::__private::BpfWriter;
use musical_chairs_economics::fees;
use anchor_lang::solana_program::{
    bpf_loader_upgradeable,
    program::{invoke, invoke_signed},
    system_instruction,
};

use crate::events::{ConfigActivated, ConfigStaged, SCHEMA_VERSION};
use crate::state::{
    BACKER_REMAINING_CUTOFF,
    pages_for, Admission, CandidatePage, CloseRecord, ConfigVersion, GameVault, Ops, ParticipantPage,
    Pool, Promotion, RegistryPage, RentComponents, RngAuthority, SnapshotPage, SnapshotRow, Ticket,
    ADMISSION_SEED, CANDIDATES_SEED, CLOSE_SEED, CONFIG_SEED, DRAW_NOT_REQUESTED, GAME_VAULT_SEED,
    IDLE, OPS_CASH_SEED, OPS_SEED, PAGE_CAPACITY, PARTICIPANTS_SEED, POOL_SEED, PROMOTION_DEADLINE_SECS,
    QUOTA_CALLS, QUOTA_WINDOW_SECS,
    REGISTRY_SEED, RNG_AUTHORITY_SEED, SNAPSHOT_SEED, TICKET_SEED,
};

/// Squads v4 program used by the production authority profile.
pub const SQUADS_PROGRAM_ID: Pubkey = pubkey!("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf");
const MULTISIG_PREFIX: &[u8] = b"multisig";
const VAULT_PREFIX: &[u8] = b"vault";
const GOVERNANCE_DELAY: u32 = 172_800;
const SQUADS_THRESHOLD: u16 = 2;
/// `rotate_recipient` kinds.
const RECIPIENT_MANAGEMENT: u8 = 0;
const RECIPIENT_SPONSOR: u8 = 1;
const SQUADS_MEMBER_COUNT: usize = 3;
/// Both authorities act through their Squads vault at index 0.
const VAULT_INDEX: u8 = 0;
const PERMISSION_INITIATE: u8 = 1;
const PERMISSION_VOTE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 4;
const PERMISSION_MASK: u8 = PERMISSION_INITIATE | PERMISSION_VOTE | PERMISSION_EXECUTE;
/// Anchor's `account:Multisig` discriminator, `sha256("account:Multisig")[..8]`.
/// Kept as a literal so the SBF build needs no hash crate; the unit test
/// `multisig_discriminator_matches_anchor_namespace` derives it with `sha2`.
pub const SQUADS_MULTISIG_DISCRIMINATOR: [u8; 8] =
    [0xe0, 0x74, 0x79, 0xba, 0x44, 0xa1, 0x4f, 0xec];

/// Genesis configuration.  `sponsor` is intentionally repeated in the args
/// and as a signer account so a fee payer cannot silently become the sponsor.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitializeArgs {
    pub sponsor: Pubkey,
    pub management_recipient: Pubkey,
    /// Immutable cluster domain (devnet and mainnet differ); stored once in
    /// `Pool.cluster_domain` and never writable again.
    pub cluster_domain: [u8; 32],
    /// Provider tuple and launch fields written into `ConfigVersion#1`; they
    /// are launch values, so `ConfigVersion::default_v1` leaves them zeroed.
    pub provider_request_fee_lamports: u64,
    pub provider_fee_headroom_bps: u16,
    pub provider_program: Pubkey,
    pub provider_network_state: Pubkey,
    pub provider_treasury: Pubkey,
    pub provider_client: Pubkey,
    pub public_series_a_enabled: bool,
}

/// The governance-stageable part of a [`ConfigVersion`]: every field except
/// the program-assigned `bump`, `version`, `staged_at` and `activated_at`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigTerms {
    pub effective_round_id: u64,
    pub plan_rates_bps_per_day: [u16; 3],
    pub plan_durations_s: [u32; 3],
    pub compound_factors_q18: [u64; 2],
    pub simple_toll_bps: [u16; 3],
    pub simple_toll_day_bounds: [u16; 2],
    pub jackpot_bps: [u16; 2],
    pub front_end_load_bps: u16,
    pub min_game: u64,
    pub min_a: u64,
    pub quota_count: u8,
    pub quota_window_s: u32,
    pub seed_bps: u16,
    pub senior_bps: u16,
    pub a_bps: u16,
    pub b_bps: u16,
    pub a_entitlement_bps: u16,
    pub b_entitlement_bps: u16,
    pub open_cutoff: u64,
    pub deadline_seconds: u32,
    pub provider_request_fee_lamports: u64,
    pub provider_fee_headroom_bps: u16,
    pub pending_request_bytes: u16,
    pub fulfilled_request_bytes: u16,
    pub provider_program: Pubkey,
    pub provider_network_state: Pubkey,
    pub provider_treasury: Pubkey,
    pub provider_client: Pubkey,
    pub public_series_a_enabled: bool,
    pub page_capacity: u8,
}

impl ConfigTerms {
    /// Bounds every staged (and genesis) rule set must satisfy.  The fields
    /// that fixed helpers consume are pinned to their canonical values rather
    /// than range-checked: the Simple rate (1,100 bps/day), the Series A and
    /// Series B entitlements (12,400 / 11,600 bps), `seed_bps` (5,000 — half
    /// the toll), the open cutoff (10 lamports) and the quota (3 per 3,600 s).
    /// The senior, Series A and Series B shares must cover the allocation
    /// exactly.
    pub fn validate(&self) -> Result<()> {
        const BPS: u16 = 10_000;
        let all_bps_in_range = self
            .plan_rates_bps_per_day
            .iter()
            .chain(self.simple_toll_bps.iter())
            .chain(self.jackpot_bps.iter())
            .chain([
                &self.front_end_load_bps,
                &self.seed_bps,
                &self.senior_bps,
                &self.a_bps,
                &self.b_bps,
                &self.provider_fee_headroom_bps,
            ])
            .all(|bps| *bps <= BPS);
        require!(all_bps_in_range, crate::ErrorCode::InvalidState);
        let split = u32::from(self.senior_bps) + u32::from(self.a_bps) + u32::from(self.b_bps);
        require!(split == u32::from(BPS), crate::ErrorCode::InvalidState);
        require!(
            self.a_entitlement_bps >= BPS && self.b_entitlement_bps >= BPS,
            crate::ErrorCode::InvalidState
        );
        require!(
            self.plan_durations_s.iter().all(|duration| *duration > 0)
                && self.quota_window_s > 0
                && self.deadline_seconds > 0,
            crate::ErrorCode::InvalidState
        );
        // The quota is a frozen contract (spec §3.1: three successful calls per
        // rolling 3,600 s) and `User::record_success` enforces exactly
        // `QUOTA_CALLS` / `QUOTA_WINDOW_SECS` from its fixed three-slot array, so
        // a staged configuration may only restate those values; anything else
        // would be silently ignored at admission time (Codex M5 review).
        require!(
            self.quota_count == QUOTA_CALLS
                && u64::from(self.quota_window_s) == QUOTA_WINDOW_SECS,
            crate::ErrorCode::InvalidState
        );
        // The promotion window is frozen too (spec section 4: `promotion_deadline =
        // close_started_at + 600`, never reset).  The closing payout copies the
        // staged value verbatim, so a staged 1 would re-time every close to a
        // one-second window; only the canonical value may be restated (Codex
        // M7 review).
        require!(
            u64::from(self.deadline_seconds) == PROMOTION_DEADLINE_SECS,
            crate::ErrorCode::InvalidState
        );
        // Five more rule values are consumed by fixed helpers rather than read
        // from the config, so a staged configuration may only restate them
        // (Codex M6 follow-up to the M5 caveat): the Simple rate (economics
        // hard-codes 11%/day and `simple_owed` rejects any other pinned rate
        // after activation), the A/B entitlement multipliers
        // (`fees::series_a_entitlement` 124% / `series_b_entitlement` 116%),
        // the seed half (`fold_slices` takes toll/2) and the open-registry
        // cutoff (`BACKER_REMAINING_CUTOFF` = 10). Anything else would be
        // accepted at staging and then ignored or unusable.
        require!(
            self.plan_rates_bps_per_day[0] == 1_100
                && self.a_entitlement_bps == 12_400
                && self.b_entitlement_bps == 11_600
                && self.seed_bps == 5_000
                && u128::from(self.open_cutoff) == BACKER_REMAINING_CUTOFF,
            crate::ErrorCode::InvalidState
        );
        require!(
            self.page_capacity == PAGE_CAPACITY as u8,
            crate::ErrorCode::InvalidState
        );
        // The Carried Interest table and its tier boundaries are consumed by
        // `positions::simple_toll`, which refuses any tier other than the
        // economics crate's (`fees::simple_toll_bps`: 1,200 / 750 / 300 bps
        // before day 7 / before day 14 / after) at payout time.  A divergent
        // staged table was therefore accepted here and would strand every
        // Simple position opened under it (M10 review SEC-1 / ECO-2): the
        // table may only be restated.
        require!(
            self.simple_toll_bps
                == [
                    fees::simple_toll_bps(0),
                    fees::simple_toll_bps(7 * fees::DAY),
                    fees::simple_toll_bps(14 * fees::DAY)
                ]
                && self.simple_toll_day_bounds == [7, 14],
            crate::ErrorCode::InvalidState
        );
        require!(
            self.simple_toll_day_bounds[0] < self.simple_toll_day_bounds[1],
            crate::ErrorCode::InvalidState
        );
        require!(self.min_game > 0 && self.min_a > 0, crate::ErrorCode::InvalidState);
        Ok(())
    }

    /// Assemble the stored account from these terms plus the program-assigned
    /// fields.
    pub fn into_config(
        self,
        version: u32,
        bump: u8,
        staged_at: u64,
        activated_at: Option<u64>,
    ) -> ConfigVersion {
        ConfigVersion {
            bump,
            version,
            effective_round_id: self.effective_round_id,
            staged_at,
            activated_at,
            plan_rates_bps_per_day: self.plan_rates_bps_per_day,
            plan_durations_s: self.plan_durations_s,
            compound_factors_q18: self.compound_factors_q18,
            simple_toll_bps: self.simple_toll_bps,
            simple_toll_day_bounds: self.simple_toll_day_bounds,
            jackpot_bps: self.jackpot_bps,
            front_end_load_bps: self.front_end_load_bps,
            min_game: self.min_game,
            min_a: self.min_a,
            quota_count: self.quota_count,
            quota_window_s: self.quota_window_s,
            seed_bps: self.seed_bps,
            senior_bps: self.senior_bps,
            a_bps: self.a_bps,
            b_bps: self.b_bps,
            a_entitlement_bps: self.a_entitlement_bps,
            b_entitlement_bps: self.b_entitlement_bps,
            open_cutoff: self.open_cutoff,
            deadline_seconds: self.deadline_seconds,
            provider_request_fee_lamports: self.provider_request_fee_lamports,
            provider_fee_headroom_bps: self.provider_fee_headroom_bps,
            pending_request_bytes: self.pending_request_bytes,
            fulfilled_request_bytes: self.fulfilled_request_bytes,
            provider_program: self.provider_program,
            provider_network_state: self.provider_network_state,
            provider_treasury: self.provider_treasury,
            provider_client: self.provider_client,
            public_series_a_enabled: self.public_series_a_enabled,
            page_capacity: self.page_capacity,
        }
    }
}

impl From<&ConfigVersion> for ConfigTerms {
    fn from(config: &ConfigVersion) -> Self {
        Self {
            effective_round_id: config.effective_round_id,
            plan_rates_bps_per_day: config.plan_rates_bps_per_day,
            plan_durations_s: config.plan_durations_s,
            compound_factors_q18: config.compound_factors_q18,
            simple_toll_bps: config.simple_toll_bps,
            simple_toll_day_bounds: config.simple_toll_day_bounds,
            jackpot_bps: config.jackpot_bps,
            front_end_load_bps: config.front_end_load_bps,
            min_game: config.min_game,
            min_a: config.min_a,
            quota_count: config.quota_count,
            quota_window_s: config.quota_window_s,
            seed_bps: config.seed_bps,
            senior_bps: config.senior_bps,
            a_bps: config.a_bps,
            b_bps: config.b_bps,
            a_entitlement_bps: config.a_entitlement_bps,
            b_entitlement_bps: config.b_entitlement_bps,
            open_cutoff: config.open_cutoff,
            deadline_seconds: config.deadline_seconds,
            provider_request_fee_lamports: config.provider_request_fee_lamports,
            provider_fee_headroom_bps: config.provider_fee_headroom_bps,
            pending_request_bytes: config.pending_request_bytes,
            fulfilled_request_bytes: config.fulfilled_request_bytes,
            provider_program: config.provider_program,
            provider_network_state: config.provider_network_state,
            provider_treasury: config.provider_treasury,
            provider_client: config.provider_client,
            public_series_a_enabled: config.public_series_a_enabled,
            page_capacity: config.page_capacity,
        }
    }
}

/// Authenticated one-time setup.  The two Multisig accounts and the two Vault
/// accounts are unchecked because Squads is an external program; all owner,
/// PDA, profile, and executable checks are performed before state is written.
pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
    require_keys_eq!(args.sponsor, ctx.accounts.sponsor.key(), crate::ErrorCode::NotAuthorized);
    require!(args.management_recipient != Pubkey::default(), crate::ErrorCode::InvalidState);

    require_keys_eq!(
        ctx.accounts.squads_program.key(),
        SQUADS_PROGRAM_ID,
        crate::ErrorCode::WrongAccount
    );
    require!(
        ctx.accounts.squads_program.executable,
        crate::ErrorCode::WrongAccount
    );
    require!(ctx.accounts.self_program.executable, crate::ErrorCode::WrongAccount);
    require_keys_eq!(
        ctx.accounts.self_program.key(),
        crate::ID,
        crate::ErrorCode::WrongAccount
    );

    // G signs `initialize` as the Squads vault CPI signer; the profile is read
    // from the Multisig account, never trusted from the caller.
    let governance_multisig = ctx.accounts.governance_multisig.to_account_info();
    let governance_vault = validate_profile(
        &governance_multisig,
        GOVERNANCE_DELAY,
        SQUADS_THRESHOLD,
        SQUADS_MEMBER_COUNT,
    )?;
    require_vault_signer(
        &governance_multisig,
        VAULT_INDEX,
        &ctx.accounts.governance.to_account_info(),
    )?;
    require_keys_eq!(
        governance_vault,
        ctx.accounts.governance.key(),
        crate::ErrorCode::NotAuthorized
    );
    let guardian_vault = validate_profile(
        &ctx.accounts.guardian_multisig.to_account_info(),
        0,
        SQUADS_THRESHOLD,
        SQUADS_MEMBER_COUNT,
    )?;
    require_keys_eq!(
        guardian_vault,
        ctx.accounts.guardian.key(),
        crate::ErrorCode::WrongAccount
    );
    require_distinct_authorities(&governance_vault, &guardian_vault)?;
    validate_program_binding(
        &ctx.accounts.self_program.to_account_info(),
        &ctx.accounts.program_data.to_account_info(),
        &governance_vault,
    )?;

    let ops_cash_info = ctx.accounts.ops_cash.to_account_info();
    let (ops_cash_key, _) = Pubkey::find_program_address(&[OPS_CASH_SEED], &crate::ID);
    require_keys_eq!(ops_cash_info.key(), ops_cash_key, crate::ErrorCode::WrongAccount);
    require_keys_eq!(
        *ops_cash_info.owner,
        anchor_lang::solana_program::system_program::ID,
        crate::ErrorCode::WrongOwner
    );
    require!(ops_cash_info.data_is_empty(), crate::ErrorCode::WrongAccount);

    // A zero-data system account is funded by a normal sponsor transfer.  The
    // PDA is deliberately not program-owned: its empty data is the physical
    // operations cash register, while Ops records the accounting partitions.
    let rent = Rent::get()?.minimum_balance(0);
    let missing_rent = rent.saturating_sub(ops_cash_info.lamports());
    if missing_rent > 0 {
        invoke(
            &system_instruction::transfer(&ctx.accounts.sponsor.key(), &ops_cash_key, missing_rent),
            &[
                ctx.accounts.sponsor.to_account_info(),
                ops_cash_info.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    }

    let pool = &mut ctx.accounts.pool;
    pool.bump = ctx.bumps.pool;
    pool.cluster_domain = args.cluster_domain;
    pool.round_id = 1;
    pool.active_config_version = 1;
    pool.pot = 0;
    pool.seed = 0;
    pool.management = 0;
    pool.pending_toll = 0;
    pool.funded_claims = 0;
    pool.excess = 0;
    // Genesis already matches what `sync_game` writes on later calls.
    pool.protected_rent = Rent::get()?.minimum_balance(GameVault::SPACE);
    pool.registry_len = 0;
    pool.registry_version = 0;
    pool.next_sequence = 1;
    pool.next_ticket = 1;
    pool.active_ticket = 0;
    pool.finalized_ticket = 0;
    pool.snapshot_pages = 0;
    pool.registry_pages = 0;
    pool.participant_count = 0;
    pool.participant_pages = 0;
    pool.next_close_id = 1;
    pool.current_close_id = None;
    pool.total_gross_deposits = 0;
    pool.total_net_payouts = 0;
    pool.governance = governance_vault;
    pool.guardian_vault = guardian_vault;
    pool.sponsor = args.sponsor;
    pool.management_recipient = args.management_recipient;

    ctx.accounts.admission.bump = ctx.bumps.admission;
    ctx.accounts.admission.guardian_vault = guardian_vault;
    ctx.accounts.admission.paused = false;
    ctx.accounts.admission.changed_at = 0;

    ctx.accounts.game_vault.bump = ctx.bumps.game_vault;

    ctx.accounts.ops.bump = ctx.bumps.ops;
    ctx.accounts.ops.sponsor = args.sponsor;
    ctx.accounts.ops.free_balance = 0;
    ctx.accounts.ops.locked_remaining_total = 0;
    ctx.accounts.ops.unallocated_excess = ops_cash_info.lamports().saturating_sub(rent);
    // Genesis already matches what `sync_ops` writes on later calls.
    ctx.accounts.ops.protected_rent = rent;

    ctx.accounts.ticket.bump = ctx.bumps.ticket;
    ctx.accounts.ticket.id = 0;
    ctx.accounts.ticket.phase = IDLE;

    // ConfigVersion#1 is the accepted launch rule set plus the genesis
    // provider tuple; it is active from the first round.
    write_genesis_config(&mut ctx.accounts.config, &args, ctx.bumps.config, crate::now()?)?;

    // The draw is bound for good to `RngAuthority.client` (written once here,
    // compared on every `request_round_draw`), and ORAO's `Client` PDA for this
    // program is fully determined by the program id, the `rng_authority` PDA
    // and the ORAO program id.  Any other genesis tuple would disable the draw
    // until a program upgrade, so it fails closed here (M10 review SEC-4).
    require_keys_eq!(
        args.provider_client,
        crate::provider::expected_client(),
        crate::ErrorCode::WrongProviderTuple
    );
    ctx.accounts.rng_authority.bump = ctx.bumps.rng_authority;
    ctx.accounts.rng_authority.client = args.provider_client;

    // Keep the bump live for callers that need to construct PDA signer seeds;
    // no data is stored in the system-owned cash account itself.
    Ok(())
}

/// Fill `ConfigVersion#1` in place.  Kept out of line so the 4 KiB SBF stack
/// frame of `initialize` does not also carry the config and terms temporaries.
#[inline(never)]
fn write_genesis_config(
    config: &mut ConfigVersion,
    args: &InitializeArgs,
    bump: u8,
    now: u64,
) -> Result<()> {
    *config = ConfigVersion::default_v1();
    config.bump = bump;
    config.staged_at = now;
    config.activated_at = Some(now);
    config.provider_request_fee_lamports = args.provider_request_fee_lamports;
    config.provider_fee_headroom_bps = args.provider_fee_headroom_bps;
    config.provider_program = args.provider_program;
    config.provider_network_state = args.provider_network_state;
    config.provider_treasury = args.provider_treasury;
    config.provider_client = args.provider_client;
    config.public_series_a_enabled = args.public_series_a_enabled;
    ConfigTerms::from(&*config).validate()
}

pub fn set_pause(ctx: Context<Pause>, paused: bool) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admission.guardian_vault,
        ctx.accounts.guardian.key(),
        crate::ErrorCode::NotAuthorized
    );
    ctx.accounts.admission.paused = paused;
    ctx.accounts.admission.changed_at = crate::now()?;
    let guardian_vault = ctx.accounts.admission.guardian_vault;
    let changed_at = ctx.accounts.admission.changed_at;
    emit_cpi!(crate::events::AdmissionPauseSet {
        schema_version: crate::events::SCHEMA_VERSION,
        paused,
        guardian_vault,
        changed_at,
    });
    Ok(())
}

pub fn fund_operations(ctx: Context<FundOperations>, amount: u64) -> Result<()> {
    validate_ops_cash(&ctx.accounts.ops_cash)?;
    // Spec §7.1: a zero-lamport contribution is rejected before any CPI.
    require!(amount > 0, crate::ErrorCode::InvalidState);
    let ops = &mut ctx.accounts.ops;
    crate::sync_ops(ops, &ctx.accounts.ops_cash.to_account_info())?;
    invoke(
        &system_instruction::transfer(
            &ctx.accounts.sender.key(),
            &ctx.accounts.ops_cash.key(),
            amount,
        ),
        &[
            ctx.accounts.sender.to_account_info(),
            ctx.accounts.ops_cash.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;
    ops.free_balance = crate::checked_add(ops.free_balance, amount)?;
    ops.total_contributed = ops
        .total_contributed
        .checked_add(u128::from(amount))
        .ok_or(crate::ErrorCode::Overflow)?;
    let free_balance_after = ops.free_balance;
    let funder = ctx.accounts.sender.key();
    emit_cpi!(crate::events::OperationsFunded {
        schema_version: crate::events::SCHEMA_VERSION,
        funder,
        amount,
        free_balance_after,
    });
    Ok(())
}

pub fn withdraw_operations(ctx: Context<WithdrawOperations>, amount: u64) -> Result<()> {
    validate_ops_cash(&ctx.accounts.ops_cash)?;
    let ops = &mut ctx.accounts.ops;
    crate::sync_ops(ops, &ctx.accounts.ops_cash.to_account_info())?;
    require_keys_eq!(
        ops.sponsor,
        ctx.accounts.sponsor.key(),
        crate::ErrorCode::NotAuthorized
    );
    require!(amount <= ops.free_balance, crate::ErrorCode::InsufficientOperations);

    let rent = Rent::get()?.minimum_balance(0);
    let protected = rent
        .checked_add(ops.locked_remaining_total)
        .ok_or(crate::ErrorCode::Overflow)?;
    let required = protected
        .checked_add(amount)
        .ok_or(crate::ErrorCode::Overflow)?;
    require!(
        ctx.accounts.ops_cash.lamports() >= required,
        crate::ErrorCode::InsufficientOperations
    );

    let bump = pda_bump(&[OPS_CASH_SEED], &ctx.accounts.ops_cash.key())?;
    let signer_seeds: &[&[u8]] = &[OPS_CASH_SEED, &[bump]];
    invoke_signed(
        &system_instruction::transfer(
            &ctx.accounts.ops_cash.key(),
            &ctx.accounts.sponsor.key(),
            amount,
        ),
        &[
            ctx.accounts.ops_cash.to_account_info(),
            ctx.accounts.sponsor.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[signer_seeds],
    )?;
    ops.free_balance = crate::checked_sub(ops.free_balance, amount)?;
    ops.total_withdrawn = ops
        .total_withdrawn
        .checked_add(u128::from(amount))
        .ok_or(crate::ErrorCode::Overflow)?;
    let free_balance_after = ops.free_balance;
    let recipient = ctx.accounts.sponsor.key();
    emit_cpi!(crate::events::OperationsWithdrawn {
        schema_version: crate::events::SCHEMA_VERSION,
        recipient,
        amount,
        free_balance_after,
    });
    Ok(())
}

pub fn withdraw_management(ctx: Context<WithdrawManagement>, amount: u64) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.pool.management_recipient,
        ctx.accounts.recipient.key(),
        crate::ErrorCode::NotAuthorized
    );
    reject_protected_destination(
        &ctx.accounts.recipient.key(),
        &[
            ctx.accounts.pool.key(),
            ctx.accounts.game_vault.key(),
            ctx.accounts.ops.key(),
            ctx.accounts.ops_cash.key(),
        ],
    )?;
    crate::sync_game(
        &mut ctx.accounts.pool,
        &ctx.accounts.game_vault.to_account_info(),
    )?;
    require!(
        amount <= ctx.accounts.pool.management,
        crate::ErrorCode::InsufficientManagement
    );
    crate::send_game(
        &ctx.accounts.game_vault.to_account_info(),
        &ctx.accounts.recipient.to_account_info(),
        amount,
    )?;
    ctx.accounts.pool.management = crate::checked_sub(ctx.accounts.pool.management, amount)?;
    let management_after = ctx.accounts.pool.management;
    let recipient = ctx.accounts.recipient.key();
    emit_cpi!(crate::events::ManagementWithdrawn {
        schema_version: crate::events::SCHEMA_VERSION,
        recipient,
        amount,
        management_after,
    });
    Ok(())
}

/// Authenticate a governance (G) instruction: the supplied Multisig must pass
/// the autonomous 2-of-3, 172,800 s profile, its index-0 vault must be the
/// recorded `pool.governance`, and that vault must have signed (as a Squads
/// CPI signer).  `pool.governance` alone is never trusted.
pub(crate) fn require_governance(
    multisig: &AccountInfo<'_>,
    governance: &AccountInfo<'_>,
    pool: &Pool,
) -> Result<()> {
    let vault = validate_profile(multisig, GOVERNANCE_DELAY, SQUADS_THRESHOLD, SQUADS_MEMBER_COUNT)?;
    require_keys_eq!(vault, pool.governance, crate::ErrorCode::NotAuthorized);
    require_vault_signer(multisig, VAULT_INDEX, governance)
}

/// Stage `ConfigVersion#version` (G only).  The new account is paid from the
/// operations free balance and activates no earlier than
/// `terms.effective_round_id`; staging a version that already exists fails
/// in `create_shared` because the target is no longer a system account.
pub fn stage_config_version(
    ctx: Context<StageConfigVersion>,
    version: u32,
    terms: ConfigTerms,
) -> Result<()> {
    require_governance(
        &ctx.accounts.governance_multisig.to_account_info(),
        &ctx.accounts.governance.to_account_info(),
        &ctx.accounts.pool,
    )?;
    let pool = &ctx.accounts.pool;
    require!(
        pool.active_config_version.checked_add(1) == Some(version),
        crate::ErrorCode::InvalidState
    );
    let earliest = pool
        .round_id
        .checked_add(1)
        .ok_or(crate::ErrorCode::Overflow)?;
    require!(
        terms.effective_round_id >= earliest,
        crate::ErrorCode::ConfigNotActivatable
    );
    terms.validate()?;

    let config_info = ctx.accounts.config.to_account_info();
    let bump = pda_bump(
        &[CONFIG_SEED, &version.to_le_bytes()],
        &ctx.accounts.config.key(),
    )?;
    crate::create_shared(
        &config_info,
        &ctx.accounts.ops_cash.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &[CONFIG_SEED, &version.to_le_bytes(), &[bump]],
        ConfigVersion::SPACE,
        &mut ctx.accounts.ops,
    )?;
    let config = terms.into_config(version, bump, crate::now()?, None);
    serialize_account(&config_info, &config)?;
    emit_cpi!(ConfigStaged {
        schema_version: SCHEMA_VERSION,
        version,
        effective_round_id: terms.effective_round_id,
    });
    Ok(())
}

/// Activate a staged version (permissionless).  Only in the idle state with
/// no allocation ticket, and only once the pool has reached the version's
/// `effective_round_id`, so the change is prospective.
pub fn activate_config_version(
    ctx: Context<ActivateConfigVersion>,
    version: u32,
    expected_current: u32,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let config = &mut ctx.accounts.config;
    require!(
        expected_current.checked_add(1) == Some(version)
            && expected_current == pool.active_config_version
            && config.version == version,
        crate::ErrorCode::ConfigNotActivatable
    );
    require!(
        pool.round_id >= config.effective_round_id,
        crate::ErrorCode::ConfigNotActivatable
    );
    // `Admission` is deliberately not consulted: activation is allowed while
    // paused.  The Ticket account only proves the singleton is the canonical one.
    require!(
        !pool.closing() && pool.active_ticket == 0,
        crate::ErrorCode::ConfigNotActivatable
    );
    require!(config.activated_at.is_none(), crate::ErrorCode::ConfigNotActivatable);

    pool.active_config_version = version;
    config.activated_at = Some(crate::now()?);
    let round_id = pool.round_id;
    emit_cpi!(ConfigActivated {
        schema_version: SCHEMA_VERSION,
        version,
        round_id,
    });
    Ok(())
}

/// Rotate a payout recipient (G only).  `kind` 0 is the management recipient
/// (`withdraw_management`), 1 is the sponsor (`withdraw_operations`).  The
/// sponsor is checked on `Ops.sponsor` by `withdraw_operations`, so kind 1
/// updates it together with the `Pool.sponsor` copy.
pub fn rotate_recipient(
    ctx: Context<RotateRecipient>,
    kind: u8,
    new_recipient: Pubkey,
) -> Result<()> {
    require_governance(
        &ctx.accounts.governance_multisig.to_account_info(),
        &ctx.accounts.governance.to_account_info(),
        &ctx.accounts.pool,
    )?;
    require!(kind <= RECIPIENT_SPONSOR, crate::ErrorCode::InvalidState);
    require!(
        new_recipient != Pubkey::default(),
        crate::ErrorCode::ProtectedDestination
    );
    reject_protected_destination(&new_recipient, &program_pda_keys())?;
    if kind == RECIPIENT_MANAGEMENT {
        ctx.accounts.pool.management_recipient = new_recipient;
    } else {
        ctx.accounts.pool.sponsor = new_recipient;
        ctx.accounts.ops.sponsor = new_recipient;
    }
    Ok(())
}

/// Replace the pause guardian (G only).  The new Multisig must itself pass the
/// immediate 2-of-3 profile; `Admission.guardian_vault` (authoritative) and
/// the `Pool.guardian_vault` copy change in this one instruction, so the old
/// guardian is rejected from the next slot and a lost P quorum is recoverable
/// by G alone.
pub fn rotate_guardian(ctx: Context<RotateGuardian>) -> Result<()> {
    require_governance(
        &ctx.accounts.governance_multisig.to_account_info(),
        &ctx.accounts.governance.to_account_info(),
        &ctx.accounts.pool,
    )?;
    let new_vault = validate_profile(
        &ctx.accounts.new_guardian_multisig.to_account_info(),
        0,
        SQUADS_THRESHOLD,
        SQUADS_MEMBER_COUNT,
    )?;
    require_distinct_authorities(&ctx.accounts.pool.governance, &new_vault)?;
    ctx.accounts.admission.guardian_vault = new_vault;
    ctx.accounts.admission.changed_at = crate::now()?;
    ctx.accounts.pool.guardian_vault = new_vault;
    Ok(())
}

// ---------------------------------------------------------------------------
// Milestone 4: generic page provisioning, rent components, safe cleanup.
// ---------------------------------------------------------------------------

/// Which shared page a provisioning or cleanup call addresses. The Borsh
/// encoding is part of the client ABI: variant order (`Registry` 0,
/// `Snapshot` 1, `Participant` 2, `Candidate` 3) is frozen.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    /// Permanent registry page (`["registry", index]`); never closable.
    Registry,
    /// Permanent snapshot page (`["snapshot", index]`); never closable.
    Snapshot,
    /// One page of a round's participant list (`["participants", round, index]`).
    Participant { round: u64 },
    /// One page of a close's candidate set (`["candidates", close_id, set_tag, index]`);
    /// `set_tag` 0 is the preferred set, 1 the fallback set.
    Candidate { close_id: u64, set_tag: u8 },
}

/// Who pays a page's rent. Frozen Borsh order: `Free` 0, `Caller` 1, `LockedClose` 2.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Funding {
    /// Operations `free_balance`, via OpsCash.
    Free,
    /// The transaction's `payer`; refunded to it when the page is closed.
    Caller,
    /// The close's locked reserve (sponsor partition first, then owner).
    LockedClose,
}

/// `Candidate.set_tag` values.
const SET_PREFERRED: u8 = 0;
const SET_FALLBACK: u8 = 1;

/// A page's identity resolved against current state: the seed parts (without
/// the bump), the account size, and the one index that may be provisioned next.
#[derive(Debug, PartialEq, Eq)]
struct PageTarget {
    seeds: Vec<Vec<u8>>,
    space: usize,
    expected_index: u64,
}

/// Resolve `kind`/`index` to seeds, size and the next sequential index, and
/// reject kinds that are not provisionable right now (`InvalidState`).
fn page_target(
    kind: &PageKind,
    index: u64,
    pool: &Pool,
    close: Option<&CloseRecord>,
) -> Result<PageTarget> {
    let index_seed = index.to_le_bytes().to_vec();
    Ok(match *kind {
        PageKind::Registry => PageTarget {
            seeds: vec![REGISTRY_SEED.to_vec(), index_seed],
            space: RegistryPage::SPACE,
            expected_index: pool.registry_pages,
        },
        PageKind::Snapshot => PageTarget {
            seeds: vec![SNAPSHOT_SEED.to_vec(), index_seed],
            space: SnapshotPage::SPACE,
            expected_index: pool.snapshot_pages,
        },
        PageKind::Participant { round } => {
            require!(round == pool.round_id, crate::ErrorCode::InvalidState);
            PageTarget {
                seeds: vec![PARTICIPANTS_SEED.to_vec(), round.to_le_bytes().to_vec(), index_seed],
                space: ParticipantPage::SPACE,
                expected_index: pool.participant_pages,
            }
        }
        PageKind::Candidate { close_id, set_tag } => {
            require!(set_tag <= SET_FALLBACK, crate::ErrorCode::InvalidState);
            let close = close.ok_or_else(|| error!(crate::ErrorCode::InvalidState))?;
            require!(close.close_id == close_id, crate::ErrorCode::InvalidState);
            require!(
                close_id == pool.next_close_id || Some(close_id) == pool.current_close_id,
                crate::ErrorCode::InvalidState
            );
            PageTarget {
                seeds: vec![
                    CANDIDATES_SEED.to_vec(),
                    close_id.to_le_bytes().to_vec(),
                    vec![set_tag],
                    index_seed,
                ],
                space: CandidatePage::SPACE,
                expected_index: if set_tag == SET_PREFERRED {
                    close.provisioned_preferred_pages
                } else {
                    close.provisioned_fallback_pages
                },
            }
        }
    })
}

/// `LockedClose` is allowed only for a candidate page of the close currently
/// in progress, and only inside the pages that close reserved for (spec §7.1),
/// and only until its reserve is settled: after settlement the owner partition
/// can hold a deferred owner refund (spec §8.5, DR-14), which is owed to the
/// owner destination and is never spendable as page rent.
/// `Free` and `Caller` are always allowed here.
fn locked_permission(
    kind: &PageKind,
    funding: Funding,
    index: u64,
    pool: &Pool,
    close: Option<&CloseRecord>,
) -> Result<()> {
    if funding != Funding::LockedClose {
        return Ok(());
    }
    match (kind, close) {
        (PageKind::Candidate { close_id, .. }, Some(close))
            if Some(*close_id) == pool.current_close_id
                && !close.reserve_settled
                && index < close.required_pages_per_set =>
        {
            Ok(())
        }
        _ => err!(crate::ErrorCode::LockNotPermitted),
    }
}

/// Need bound for sponsor-funded pages: free sponsor cash cannot be locked
/// into pages nobody needs. Applies only to `Funding::Free`; `Caller` (the
/// caller risks only their own refundable rent) and `LockedClose` (bounded by
/// `required_pages_per_set`) are never restricted here. Violations are
/// `WrongIndex`.
///
/// - Registry and Snapshot (permanent, never closable):
///   `index <= registry_len / PAGE_CAPACITY` (one page beyond the registry's
///   current extent, because the tail page may be full).
/// - Participant: `index <= participant_count / PAGE_CAPACITY`.
/// - Candidate of the NEXT close (pre-capture): `index <
///   pages_for(max(participant_count, 1), PAGE_CAPACITY)`; more pages can be
///   provisioned later as participants grow.
/// - Candidate of the CURRENT close: no extra bound (a Free page beyond
///   `required_pages_per_set` is allowed by the spec).
fn free_need_bound(kind: &PageKind, funding: Funding, index: u64, pool: &Pool) -> Result<()> {
    if funding != Funding::Free {
        return Ok(());
    }
    let capacity = PAGE_CAPACITY as u64;
    let within = match *kind {
        PageKind::Registry | PageKind::Snapshot => index <= pool.registry_len / capacity,
        PageKind::Participant { .. } => index <= pool.participant_count / capacity,
        PageKind::Candidate { close_id, .. } if close_id == pool.next_close_id => {
            index < pages_for(pool.participant_count.max(1), capacity)?
        }
        PageKind::Candidate { .. } => true,
    };
    require!(within, crate::ErrorCode::WrongIndex);
    Ok(())
}

/// Bump the counters a freshly created page feeds. Registry, Snapshot and
/// Participant counters live on the pool; candidate counters on their close,
/// and while that close is still the *next* one (pre-capture) the page also
/// lowers that set's missing count at capture by raising `existing_<set>_pages`.
/// Counters never decrement (R3).
fn record_provisioned(
    kind: &PageKind,
    pool: &mut Pool,
    close: Option<&mut CloseRecord>,
) -> Result<()> {
    match *kind {
        PageKind::Registry => pool.registry_pages = crate::checked_add(pool.registry_pages, 1)?,
        PageKind::Snapshot => pool.snapshot_pages = crate::checked_add(pool.snapshot_pages, 1)?,
        PageKind::Participant { .. } => {
            pool.participant_pages = crate::checked_add(pool.participant_pages, 1)?
        }
        PageKind::Candidate { close_id, set_tag } => {
            let close = close.ok_or_else(|| error!(crate::ErrorCode::InvalidState))?;
            let pre_capture = close_id == pool.next_close_id;
            match set_tag {
                SET_PREFERRED => {
                    close.provisioned_preferred_pages =
                        crate::checked_add(close.provisioned_preferred_pages, 1)?;
                    if pre_capture {
                        close.existing_preferred_pages = close.provisioned_preferred_pages;
                    }
                }
                SET_FALLBACK => {
                    close.provisioned_fallback_pages =
                        crate::checked_add(close.provisioned_fallback_pages, 1)?;
                    if pre_capture {
                        close.existing_fallback_pages = close.provisioned_fallback_pages;
                    }
                }
                _ => return err!(crate::ErrorCode::InvalidState),
            }
        }
    }
    Ok(())
}

/// A page about to be created must be empty and system-owned. A correct
/// sequential index never points at an existing account, so anything else is
/// an index error and a repeated provision cannot debit twice.
fn is_fresh_account(owner: &Pubkey, data_is_empty: bool) -> bool {
    *owner == anchor_lang::solana_program::system_program::ID && data_is_empty
}

fn require_fresh_account(account: &UncheckedAccount<'_>) -> Result<()> {
    require!(
        is_fresh_account(account.owner, account.data_is_empty()),
        crate::ErrorCode::WrongIndex
    );
    Ok(())
}

/// The canonical `CloseRecord` PDA for `close_id`.
fn close_record_address(close_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CLOSE_SEED, &close_id.to_le_bytes()], &crate::ID)
}

/// Create one shared page (permissionless). See spec §7.1 `provision_page`.
///
/// Order of checks: kind rules (`InvalidState`), the page's PDA
/// (`WrongAccount`), the next sequential index and a fresh account
/// (`WrongIndex`), the sponsor-cash need bound, then `LockedClose` permission
/// (`LockNotPermitted`). Free sponsor cash cannot be locked into pages nobody
/// needs: with `Funding::Free` the index is also bounded by current need (see
/// [`free_need_bound`]; `WrongIndex`), while `Caller` and `LockedClose` are not.
pub fn provision_page(
    ctx: Context<ProvisionPage>,
    kind: PageKind,
    index: u64,
    funding: Funding,
) -> Result<()> {
    let accounts = &mut *ctx.accounts;
    validate_ops_cash(&accounts.ops_cash)?;
    let close_key = accounts.close_record.as_ref().map(|record| record.key());
    let mut close = accounts.close_record.as_mut().map(|record| &mut ***record);
    if let PageKind::Candidate { close_id, .. } = kind {
        let close_key = close_key.ok_or_else(|| error!(crate::ErrorCode::InvalidState))?;
        require_keys_eq!(close_key, close_record_address(close_id).0, crate::ErrorCode::WrongAccount);
    }

    let target = page_target(&kind, index, &accounts.pool, close.as_deref())?;
    let seed_refs: Vec<&[u8]> = target.seeds.iter().map(|seed| seed.as_slice()).collect();
    let (page_key, bump) = Pubkey::find_program_address(&seed_refs, &crate::ID);
    require_keys_eq!(accounts.page.key(), page_key, crate::ErrorCode::WrongAccount);
    require!(index == target.expected_index, crate::ErrorCode::WrongIndex);
    require_fresh_account(&accounts.page)?;
    free_need_bound(&kind, funding, index, &accounts.pool)?;
    locked_permission(&kind, funding, index, &accounts.pool, close.as_deref())?;

    let bump_seed = [bump];
    let mut signer_seeds = seed_refs.clone();
    signer_seeds.push(&bump_seed);
    let page_info = accounts.page.to_account_info();
    let cash_info = accounts.ops_cash.to_account_info();
    let system_info = accounts.system_program.to_account_info();
    let ops = &mut accounts.ops;
    let rent = match funding {
        Funding::Free => {
            let debit = crate::create_shared(
                &page_info,
                &cash_info,
                &system_info,
                &signer_seeds,
                target.space,
                ops,
            )?;
            RentComponents { ops: debit, owner: 0, owner_refund: Pubkey::default() }
        }
        Funding::Caller => {
            // The refund must always be deliverable: the payer is a signing,
            // writable, system-owned wallet, never OpsCash or a program account.
            crate::validate_destination(&accounts.payer.to_account_info())?;
            crate::sync_ops(ops, &cash_info)?;
            let paid = crate::create_caller_funded(
                &page_info,
                &accounts.payer.to_account_info(),
                &system_info,
                &signer_seeds,
                target.space,
            )?;
            RentComponents { ops: 0, owner: paid, owner_refund: accounts.payer.key() }
        }
        Funding::LockedClose => {
            let close = close
                .as_deref_mut()
                .ok_or_else(|| error!(crate::ErrorCode::LockNotPermitted))?;
            let (sponsor_part, owner_part) = crate::create_shared_locked(
                &page_info,
                &cash_info,
                &system_info,
                &signer_seeds,
                target.space,
                ops,
                close,
            )?;
            RentComponents {
                ops: sponsor_part,
                owner: owner_part,
                owner_refund: close.owner_refund_destination,
            }
        }
    };

    match kind {
        PageKind::Registry => serialize_account(
            &page_info,
            &RegistryPage { bump, index, keys: [Pubkey::default(); PAGE_CAPACITY], rent },
        )?,
        PageKind::Snapshot => serialize_account(
            &page_info,
            &SnapshotPage {
                bump,
                index,
                ticket_id: 0,
                rows: [SnapshotRow::default(); PAGE_CAPACITY],
                rent,
            },
        )?,
        PageKind::Participant { round } => serialize_account(
            &page_info,
            &ParticipantPage {
                bump,
                round_id: round,
                page: index,
                len: 0,
                rows: [Pubkey::default(); PAGE_CAPACITY],
                rent,
            },
        )?,
        PageKind::Candidate { close_id, set_tag } => serialize_account(
            &page_info,
            &CandidatePage {
                bump,
                close_id,
                set_tag,
                page: index,
                len: 0,
                rows: [Pubkey::default(); PAGE_CAPACITY],
                rent,
            },
        )?,
    }
    record_provisioned(&kind, &mut accounts.pool, close)
}

/// Create `CloseRecord(close_id)` for the next close (permissionless; `Free`
/// or `Caller` only). The closing payout binds everything else later.
pub fn provision_close_record(
    ctx: Context<ProvisionCloseRecord>,
    close_id: u64,
    funding: Funding,
) -> Result<()> {
    let accounts = &mut *ctx.accounts;
    validate_ops_cash(&accounts.ops_cash)?;
    require!(close_id == accounts.pool.next_close_id, crate::ErrorCode::InvalidState);
    let (record_key, bump) = close_record_address(close_id);
    require_keys_eq!(accounts.close_record.key(), record_key, crate::ErrorCode::WrongAccount);
    require_fresh_account(&accounts.close_record)?;
    require!(funding != Funding::LockedClose, crate::ErrorCode::LockNotPermitted);

    let id_seed = close_id.to_le_bytes();
    let bump_seed = [bump];
    let signer_seeds: &[&[u8]] = &[CLOSE_SEED, &id_seed, &bump_seed];
    let record_info = accounts.close_record.to_account_info();
    let cash_info = accounts.ops_cash.to_account_info();
    let system_info = accounts.system_program.to_account_info();
    let ops = &mut accounts.ops;
    let rent = if funding == Funding::Free {
        let debit = crate::create_shared(
            &record_info,
            &cash_info,
            &system_info,
            signer_seeds,
            CloseRecord::SPACE,
            ops,
        )?;
        RentComponents { ops: debit, owner: 0, owner_refund: Pubkey::default() }
    } else {
        crate::validate_destination(&accounts.payer.to_account_info())?;
        crate::sync_ops(ops, &cash_info)?;
        let paid = crate::create_caller_funded(
            &record_info,
            &accounts.payer.to_account_info(),
            &system_info,
            signer_seeds,
            CloseRecord::SPACE,
        )?;
        RentComponents { ops: 0, owner: paid, owner_refund: accounts.payer.key() }
    };
    let record = CloseRecord {
        bump,
        close_id,
        promotion: Promotion::Pending,
        draw_state: DRAW_NOT_REQUESTED,
        rent,
        ..CloseRecord::default()
    };
    serialize_account(&record_info, &record)
}

/// `close_shared_account` closability (spec §8.7, R3). Registry and Snapshot
/// pages are permanent: never closable, whatever their state. A participant
/// page closes once the round's own close record is finalized; a candidate
/// page once its own close record's promotion is terminal. Both predicates
/// read the supplied record's identity, never a reusable tag.
fn closable(kind: &PageKind, close: &CloseRecord, close_is_canonical: bool) -> bool {
    match *kind {
        PageKind::Registry | PageKind::Snapshot => false,
        PageKind::Participant { round } => {
            close_is_canonical && close.round_id == round && close.finalized
        }
        PageKind::Candidate { close_id, .. } => {
            close_is_canonical
                && close.close_id == close_id
                && close.promotion != Promotion::Pending
        }
    }
}

/// How a closing page's lamports divide: `(to ops free_balance, to the owner
/// refund destination, to unallocated_excess)`. The `ops` and `owner` rent
/// components return exactly; whatever else the page holds is an unsolicited
/// credit and goes to excess.
fn closure_split(rent: &RentComponents, page_lamports: u64) -> Result<(u64, u64, u64)> {
    let components = crate::checked_add(rent.ops, rent.owner)?;
    let remainder = crate::checked_sub(page_lamports, components)?;
    Ok((rent.ops, rent.owner, remainder))
}

/// Close a participant or candidate page and return its rent exactly once.
pub fn close_shared_account(ctx: Context<CloseSharedAccount>, kind: PageKind) -> Result<()> {
    let accounts = &mut *ctx.accounts;
    let close_is_canonical = matches!(kind, PageKind::Participant { .. } | PageKind::Candidate { .. })
        && accounts.close_record.key() == close_record_address(accounts.close_record.close_id).0;
    require!(
        closable(&kind, &accounts.close_record, close_is_canonical),
        crate::ErrorCode::NotClosable
    );

    let page = accounts.page.to_account_info();
    require!(
        !is_fresh_account(page.owner, page.data_is_empty()),
        crate::ErrorCode::AlreadyClosed
    );
    require_keys_eq!(*page.owner, crate::ID, crate::ErrorCode::WrongOwner);
    let (rent, derived) = {
        let data = page.try_borrow_data()?;
        let mut bytes: &[u8] = &data;
        match kind {
            PageKind::Participant { round } => {
                let decoded = ParticipantPage::try_deserialize(&mut bytes)
                    .map_err(|_| error!(crate::ErrorCode::WrongAccount))?;
                require!(decoded.round_id == round, crate::ErrorCode::WrongAccount);
                let derived = Pubkey::find_program_address(
                    &[PARTICIPANTS_SEED, &round.to_le_bytes(), &decoded.page.to_le_bytes()],
                    &crate::ID,
                )
                .0;
                (decoded.rent, derived)
            }
            PageKind::Candidate { close_id, set_tag } => {
                let decoded = CandidatePage::try_deserialize(&mut bytes)
                    .map_err(|_| error!(crate::ErrorCode::WrongAccount))?;
                require!(
                    decoded.close_id == close_id && decoded.set_tag == set_tag,
                    crate::ErrorCode::WrongAccount
                );
                let derived = Pubkey::find_program_address(
                    &[
                        CANDIDATES_SEED,
                        &close_id.to_le_bytes(),
                        &[set_tag],
                        &decoded.page.to_le_bytes(),
                    ],
                    &crate::ID,
                )
                .0;
                (decoded.rent, derived)
            }
            PageKind::Registry | PageKind::Snapshot => return err!(crate::ErrorCode::NotClosable),
        }
    };
    require_keys_eq!(page.key(), derived, crate::ErrorCode::WrongAccount);

    validate_ops_cash(&accounts.ops_cash)?;
    let cash = accounts.ops_cash.to_account_info();
    let ops = &mut accounts.ops;
    crate::sync_ops(ops, &cash)?;
    let (to_free, to_owner, to_excess) = closure_split(&rent, page.lamports())?;
    let owner_refund = accounts.owner_refund.to_account_info();
    if to_owner > 0 {
        require_keys_eq!(owner_refund.key(), rent.owner_refund, crate::ErrorCode::WrongAccount);
        crate::validate_destination(&owner_refund)?;
    }
    let to_cash = crate::checked_add(to_free, to_excess)?;
    let cash_after = crate::checked_add(cash.lamports(), to_cash)?;
    let owner_after = if to_owner > 0 {
        crate::checked_add(owner_refund.lamports(), to_owner)?
    } else {
        0
    };
    ops.free_balance = crate::checked_add(ops.free_balance, to_free)?;
    ops.unallocated_excess = crate::checked_add(ops.unallocated_excess, to_excess)?;
    **page.try_borrow_mut_lamports()? = 0;
    **cash.try_borrow_mut_lamports()? = cash_after;
    if to_owner > 0 {
        **owner_refund.try_borrow_mut_lamports()? = owner_after;
    }
    page.try_borrow_mut_data()?.fill(0);
    page.resize(0)?;
    page.assign(&anchor_lang::solana_program::system_program::ID);
    Ok(())
}

/// Fold any physical OpsCash surplus into `unallocated_excess` (idempotent).
/// The ORAO Client observation arrives with M7.
pub fn reconcile_ops_cash(ctx: Context<ReconcileOpsCash>) -> Result<()> {
    validate_ops_cash(&ctx.accounts.ops_cash)?;
    crate::sync_ops(&mut ctx.accounts.ops, &ctx.accounts.ops_cash.to_account_info())
}

fn serialize_account<T: AccountSerialize>(account: &AccountInfo<'_>, value: &T) -> Result<()> {
    let mut data = account.try_borrow_mut_data()?;
    let mut writer = BpfWriter::new(&mut data[..]);
    value.try_serialize(&mut writer)
}

fn pda_bump(seeds: &[&[u8]], expected: &Pubkey) -> Result<u8> {
    let (key, bump) = Pubkey::find_program_address(seeds, &crate::ID);
    require_keys_eq!(key, *expected, crate::ErrorCode::WrongAccount);
    Ok(bump)
}

fn validate_ops_cash(account: &UncheckedAccount<'_>) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(&[OPS_CASH_SEED], &crate::ID);
    require_keys_eq!(account.key(), expected, crate::ErrorCode::WrongAccount);
    require_keys_eq!(
        *account.owner,
        anchor_lang::solana_program::system_program::ID,
        crate::ErrorCode::WrongOwner
    );
    require!(account.data_is_empty(), crate::ErrorCode::WrongAccount);
    Ok(())
}

/// The program's own singleton PDAs, none of which may become a recipient.
fn program_pda_keys() -> [Pubkey; 6] {
    [
        POOL_SEED,
        ADMISSION_SEED,
        GAME_VAULT_SEED,
        OPS_SEED,
        OPS_CASH_SEED,
        TICKET_SEED,
    ]
    .map(|seed| Pubkey::find_program_address(&[seed], &crate::ID).0)
}

fn reject_protected_destination(destination: &Pubkey, protected: &[Pubkey]) -> Result<()> {
    require!(
        protected.iter().all(|key| key != destination),
        crate::ErrorCode::ProtectedDestination
    );
    Ok(())
}

/// The Vault PDA of a Squads v4 multisig:
/// `["multisig", multisig, "vault", vault_index]` under the Squads program.
fn derive_vault(multisig: &Pubkey, vault_index: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[MULTISIG_PREFIX, multisig.as_ref(), VAULT_PREFIX, &[vault_index]],
        &SQUADS_PROGRAM_ID,
    )
    .0
}

/// Decode and validate the compact Squads v4 `Multisig` account without taking
/// a dependency on the SDK's account layout, and return its index-0 Vault PDA.
///
/// Layout (Anchor, Borsh) at squads-protocol/v4 commit
/// 887627b3ac3f59548fdb6dfd1dfe23a21bb72b60:
/// `discriminator[8] | create_key: Pubkey | config_authority: Pubkey (plain,
/// not an Option) | threshold: u16 | time_lock: u32 | transaction_index: u64 |
/// stale_transaction_index: u64 | rent_collector: Option<Pubkey> | bump: u8 |
/// members: Vec<{ key: Pubkey, permissions: u8 }>` (u32 LE length prefix).
///
/// The profile is exact: autonomous (`config_authority == default`),
/// `threshold == expected_threshold`, `time_lock == expected_delay`, exactly
/// `expected_members` distinct non-default voting members.  Wrong owner,
/// discriminator, PDA or bump are `WrongAccount`; every shape failure
/// (including a truncated body) is `WrongGovernanceProfile`.
pub(crate) fn validate_profile(
    multisig: &AccountInfo<'_>,
    expected_delay: u32,
    expected_threshold: u16,
    expected_members: usize,
) -> Result<Pubkey> {
    require_keys_eq!(*multisig.owner, SQUADS_PROGRAM_ID, crate::ErrorCode::WrongAccount);
    let account_data = multisig.try_borrow_data()?;
    let data: &[u8] = &account_data;
    require!(
        data.len() >= 8 && data[..8] == SQUADS_MULTISIG_DISCRIMINATOR,
        crate::ErrorCode::WrongAccount
    );
    let mut cursor = 8usize;
    let create_key = read_pubkey(data, &mut cursor)?;
    let config_authority = read_pubkey(data, &mut cursor)?;
    let threshold = read_u16(data, &mut cursor)?;
    let time_lock = read_u32(data, &mut cursor)?;
    let _transaction_index = read_u64(data, &mut cursor)?;
    let _stale_transaction_index = read_u64(data, &mut cursor)?;
    let _rent_collector = read_option_pubkey(data, &mut cursor)?;
    let stored_bump = read_u8(data, &mut cursor)?;
    let member_len = read_u32(data, &mut cursor)? as usize;

    require_keys_eq!(
        config_authority,
        Pubkey::default(),
        crate::ErrorCode::WrongGovernanceProfile
    );
    require_eq!(threshold, expected_threshold, crate::ErrorCode::WrongGovernanceProfile);
    require_eq!(time_lock, expected_delay, crate::ErrorCode::WrongGovernanceProfile);
    require_eq!(member_len, expected_members, crate::ErrorCode::WrongGovernanceProfile);

    let (expected_multisig, expected_bump) = Pubkey::find_program_address(
        &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
        &SQUADS_PROGRAM_ID,
    );
    require_keys_eq!(expected_multisig, *multisig.key, crate::ErrorCode::WrongAccount);
    require_eq!(stored_bump, expected_bump, crate::ErrorCode::WrongAccount);

    // Members are walked in place (no allocation); each key is compared with
    // the keys before it, which is cheap for the bounded member count.
    let members_start = cursor;
    let mut has_initiate = false;
    let mut has_execute = false;
    for index in 0..member_len {
        let key = read_pubkey(data, &mut cursor)?;
        let mask = read_u8(data, &mut cursor)?;
        require!(key != Pubkey::default(), crate::ErrorCode::WrongGovernanceProfile);
        require!(mask & PERMISSION_VOTE != 0, crate::ErrorCode::WrongGovernanceProfile);
        require!(mask & !PERMISSION_MASK == 0, crate::ErrorCode::WrongGovernanceProfile);
        let mut prior_cursor = members_start;
        for _ in 0..index {
            let prior = read_pubkey(data, &mut prior_cursor)?;
            let _ = read_u8(data, &mut prior_cursor)?;
            require!(prior != key, crate::ErrorCode::WrongGovernanceProfile);
        }
        has_initiate |= mask & PERMISSION_INITIATE != 0;
        has_execute |= mask & PERMISSION_EXECUTE != 0;
    }
    // A profile in which nobody can initiate or execute a vault transaction
    // could never move the authority it guards.
    require!(has_initiate && has_execute, crate::ErrorCode::WrongGovernanceProfile);
    Ok(derive_vault(multisig.key, VAULT_INDEX))
}

/// Require that `signer` is the Squads Vault PDA for `vault_index` of
/// `multisig` and that it signed this transaction (Squads signs for the PDA
/// when it executes a vault transaction via CPI).  The multisig's own profile
/// is checked separately by [`validate_profile`].
pub(crate) fn require_vault_signer(
    multisig: &AccountInfo<'_>,
    vault_index: u8,
    signer: &AccountInfo<'_>,
) -> Result<()> {
    let vault = derive_vault(multisig.key, vault_index);
    require!(
        *signer.key == vault && signer.is_signer,
        crate::ErrorCode::NotAuthorized
    );
    Ok(())
}

pub(crate) fn validate_program_binding(
    self_program: &AccountInfo<'_>,
    program_data: &AccountInfo<'_>,
    governance: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        *self_program.owner,
        bpf_loader_upgradeable::ID,
        crate::ErrorCode::WrongOwner
    );
    let program_account_data = self_program.try_borrow_data()?;
    require!(program_account_data.len() >= 36, crate::ErrorCode::InvalidState);
    require_eq!(
        u32::from_le_bytes(program_account_data[0..4].try_into().unwrap()),
        2,
        crate::ErrorCode::WrongAccount
    );
    let linked_data = Pubkey::new_from_array(program_account_data[4..36].try_into().unwrap());
    require_keys_eq!(linked_data, *program_data.key, crate::ErrorCode::WrongAccount);

    require_keys_eq!(
        *program_data.owner,
        bpf_loader_upgradeable::ID,
        crate::ErrorCode::WrongOwner
    );
    let expected = bpf_loader_upgradeable::get_program_data_address(&crate::ID);
    require_keys_eq!(*program_data.key, expected, crate::ErrorCode::WrongAccount);
    let account_data = program_data.try_borrow_data()?;
    let data: &[u8] = &account_data;
    require!(data.len() >= 45, crate::ErrorCode::InvalidState);
    // UpgradeableLoaderState::ProgramData { slot: u64, upgrade_authority_address }
    // is encoded as variant=3, slot, option-tag, pubkey.
    require_eq!(u32::from_le_bytes(data[0..4].try_into().unwrap()), 3, crate::ErrorCode::WrongAccount);
    require_eq!(data[12], 1, crate::ErrorCode::NotAuthorized);
    let authority = Pubkey::new_from_array(data[13..45].try_into().unwrap());
    require_keys_eq!(authority, *governance, crate::ErrorCode::NotAuthorized);
    Ok(())
}

pub(crate) fn require_distinct_authorities(governance: &Pubkey, guardian: &Pubkey) -> Result<()> {
    require!(governance != guardian, crate::ErrorCode::InvalidState);
    Ok(())
}

/// Bounds-checked little-endian readers for the Squads account.  A short read
/// or an invalid Option tag means the account is not a well-formed profile.
fn read_bytes<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or(crate::ErrorCode::Overflow)?;
    let bytes = data
        .get(*cursor..end)
        .ok_or(crate::ErrorCode::WrongGovernanceProfile)?;
    *cursor = end;
    Ok(bytes)
}

fn read_u8(data: &[u8], cursor: &mut usize) -> Result<u8> {
    Ok(read_bytes(data, cursor, 1)?[0])
}

fn read_u16(data: &[u8], cursor: &mut usize) -> Result<u16> {
    let bytes = read_bytes(data, cursor, 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Result<u32> {
    let bytes = read_bytes(data, cursor, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], cursor: &mut usize) -> Result<u64> {
    let mut array = [0u8; 8];
    array.copy_from_slice(read_bytes(data, cursor, 8)?);
    Ok(u64::from_le_bytes(array))
}

fn read_pubkey(data: &[u8], cursor: &mut usize) -> Result<Pubkey> {
    let mut array = [0u8; 32];
    array.copy_from_slice(read_bytes(data, cursor, 32)?);
    Ok(Pubkey::new_from_array(array))
}

fn read_option_pubkey(data: &[u8], cursor: &mut usize) -> Result<Option<Pubkey>> {
    match read_u8(data, cursor)? {
        0 => Ok(None),
        1 => Ok(Some(read_pubkey(data, cursor)?)),
        _ => Err(crate::ErrorCode::WrongGovernanceProfile.into()),
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,
    #[account(init, payer = sponsor, space = Pool::SPACE, seeds = [POOL_SEED], bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(init, payer = sponsor, space = Admission::SPACE, seeds = [ADMISSION_SEED], bump)]
    pub admission: Box<Account<'info, Admission>>,
    #[account(init, payer = sponsor, space = GameVault::SPACE, seeds = [GAME_VAULT_SEED], bump)]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(init, payer = sponsor, space = Ops::SPACE, seeds = [OPS_SEED], bump)]
    pub ops: Box<Account<'info, Ops>>,
    #[account(init, payer = sponsor, space = Ticket::SPACE, seeds = [TICKET_SEED], bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(init, payer = sponsor, space = RngAuthority::SPACE, seeds = [RNG_AUTHORITY_SEED], bump)]
    pub rng_authority: Box<Account<'info, RngAuthority>>,
    #[account(
        init,
        payer = sponsor,
        space = ConfigVersion::SPACE,
        seeds = [CONFIG_SEED, &1u32.to_le_bytes()],
        bump
    )]
    pub config: Box<Account<'info, ConfigVersion>>,
    /// CHECK: validated as the canonical empty system-owned operations PDA.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: validated as a Squads v4 executable program.
    pub squads_program: UncheckedAccount<'info>,
    /// CHECK: validated against the canonical Squads multisig PDA and profile.
    pub governance_multisig: UncheckedAccount<'info>,
    /// CHECK: validated as the governance vault PDA.
    pub governance: Signer<'info>,
    /// CHECK: validated against the canonical Squads multisig PDA and profile.
    pub guardian_multisig: UncheckedAccount<'info>,
    /// CHECK: validated as the guardian vault PDA.
    pub guardian: UncheckedAccount<'info>,
    /// CHECK: validated as this program's executable account.
    pub self_program: UncheckedAccount<'info>,
    /// CHECK: validated as this program's upgradeable-loader ProgramData.
    pub program_data: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct StageConfigVersion<'info> {
    /// CHECK: Squads Multisig; owner, discriminator, PDA and profile are
    /// validated by the handler.
    pub governance_multisig: UncheckedAccount<'info>,
    /// The G Vault PDA, signing through a Squads vault-transaction CPI.
    pub governance: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned PDA; checked by `create_shared`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: canonical `["config", version]` PDA; created and written by the
    /// handler.
    #[account(mut)]
    pub config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
#[instruction(version: u32)]
pub struct ActivateConfigVersion<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CONFIG_SEED, &version.to_le_bytes()], bump = config.bump)]
    pub config: Box<Account<'info, ConfigVersion>>,
    #[account(seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(seeds = [ADMISSION_SEED], bump = admission.bump)]
    pub admission: Box<Account<'info, Admission>>,
}

#[derive(Accounts)]
pub struct RotateRecipient<'info> {
    /// CHECK: Squads Multisig; validated by the handler.
    pub governance_multisig: UncheckedAccount<'info>,
    /// The G Vault PDA, signing through a Squads vault-transaction CPI.
    pub governance: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
}

#[derive(Accounts)]
pub struct RotateGuardian<'info> {
    /// CHECK: Squads Multisig; validated by the handler.
    pub governance_multisig: UncheckedAccount<'info>,
    /// The G Vault PDA, signing through a Squads vault-transaction CPI.
    pub governance: Signer<'info>,
    #[account(mut, seeds = [ADMISSION_SEED], bump = admission.bump)]
    pub admission: Box<Account<'info, Admission>>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: replacement P Squads Multisig; validated by the handler.
    pub new_guardian_multisig: UncheckedAccount<'info>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct Pause<'info> {
    #[account(mut, seeds = [ADMISSION_SEED], bump = admission.bump)]
    pub admission: Box<Account<'info, Admission>>,
    pub guardian: Signer<'info>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct FundOperations<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned PDA; checked by the handler.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct WithdrawOperations<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical system-owned signer PDA; checked by the handler.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct WithdrawManagement<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [GAME_VAULT_SEED], bump = game_vault.bump)]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: only used for protected-destination rejection.
    pub ops_cash: UncheckedAccount<'info>,
    #[account(mut)]
    pub recipient: Signer<'info>,
}

#[derive(Accounts)]
pub struct ProvisionPage<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned OpsCash PDA; checked by the handler.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: canonical page PDA for `kind`/`index`; created and written by the handler.
    #[account(mut)]
    pub page: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// Required for `Candidate`; ignored for every other kind.
    #[account(mut)]
    pub close_record: Option<Box<Account<'info, CloseRecord>>>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProvisionCloseRecord<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned OpsCash PDA; checked by the handler.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: canonical `CloseRecord` PDA; created and written by the handler.
    #[account(mut)]
    pub close_record: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseSharedAccount<'info> {
    /// CHECK: participant or candidate page; decoded and re-derived by the handler.
    #[account(mut)]
    pub page: UncheckedAccount<'info>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned OpsCash PDA; checked by the handler.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: must equal the page's recorded owner refund destination when an
    /// owner component exists; validated as a payout destination by the handler.
    #[account(mut)]
    pub owner_refund: UncheckedAccount<'info>,
    pub close_record: Box<Account<'info, CloseRecord>>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReconcileOpsCash<'info> {
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical empty system-owned OpsCash PDA; checked by the handler.
    pub ops_cash: UncheckedAccount<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::system_program;
    use sha2::{Digest, Sha256};

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// A Squads v4 `Multisig` account in the pinned layout (see
    /// [`validate_profile`]), with every field a test might vary.
    #[derive(Clone)]
    struct Fixture {
        create_key: Pubkey,
        config_authority: Pubkey,
        threshold: u16,
        time_lock: u32,
        rent_collector: Option<Pubkey>,
        bump_override: Option<u8>,
        members: Vec<(Pubkey, u8)>,
    }

    impl Fixture {
        fn new(create_byte: u8, time_lock: u32) -> Self {
            Self {
                create_key: key(create_byte),
                config_authority: Pubkey::default(),
                threshold: 2,
                time_lock,
                rent_collector: None,
                bump_override: None,
                members: vec![(key(1), 7), (key(2), 7), (key(3), 7)],
            }
        }

        fn governance() -> Self {
            Self::new(42, 172_800)
        }

        fn guardian() -> Self {
            Self::new(43, 0)
        }

        fn with_members(mut self, members: &[(Pubkey, u8)]) -> Self {
            self.members = members.to_vec();
            self
        }

        fn with_masks(self, masks: [u8; 3]) -> Self {
            self.with_members(&[(key(1), masks[0]), (key(2), masks[1]), (key(3), masks[2])])
        }

        /// Multisig PDA and canonical bump, derived from the literal seeds.
        fn multisig(&self) -> (Pubkey, u8) {
            Pubkey::find_program_address(
                &[b"multisig", b"multisig", self.create_key.as_ref()],
                &SQUADS_PROGRAM_ID,
            )
        }

        fn vault(&self, index: u8) -> Pubkey {
            Pubkey::find_program_address(
                &[b"multisig", self.multisig().0.as_ref(), b"vault", &[index]],
                &SQUADS_PROGRAM_ID,
            )
            .0
        }

        fn bytes(&self) -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&SQUADS_MULTISIG_DISCRIMINATOR);
            bytes.extend_from_slice(self.create_key.as_ref());
            // `config_authority` is a plain Pubkey, not an Option.
            bytes.extend_from_slice(self.config_authority.as_ref());
            bytes.extend_from_slice(&self.threshold.to_le_bytes());
            bytes.extend_from_slice(&self.time_lock.to_le_bytes());
            bytes.extend_from_slice(&7u64.to_le_bytes()); // transaction_index
            bytes.extend_from_slice(&3u64.to_le_bytes()); // stale_transaction_index
            match self.rent_collector {
                None => bytes.push(0),
                Some(collector) => {
                    bytes.push(1);
                    bytes.extend_from_slice(collector.as_ref());
                }
            }
            bytes.push(self.bump_override.unwrap_or(self.multisig().1));
            bytes.extend_from_slice(&(self.members.len() as u32).to_le_bytes());
            for (member, permissions) in &self.members {
                bytes.extend_from_slice(member.as_ref());
                bytes.push(*permissions);
            }
            bytes
        }

        fn account(&self) -> AccountInfo<'static> {
            account(self.multisig().0, SQUADS_PROGRAM_ID, self.bytes(), false, false)
        }
    }

    fn account(
        key: Pubkey,
        owner: Pubkey,
        data: Vec<u8>,
        signer: bool,
        executable: bool,
    ) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(key));
        let owner = Box::leak(Box::new(owner));
        let lamports = Box::leak(Box::new(0u64));
        let data = Box::leak(data.into_boxed_slice());
        AccountInfo::new(key, signer, false, lamports, data, owner, executable, 0)
    }

    fn assert_error<T: std::fmt::Debug>(result: Result<T>, expected: &str) {
        let error = result.expect_err("validator unexpectedly accepted fixture");
        assert!(
            format!("{error:?}").contains(expected),
            "expected {expected}, got {error:?}"
        );
    }

    fn validate_g(fixture: &Fixture) -> Result<Pubkey> {
        validate_profile(&fixture.account(), GOVERNANCE_DELAY, 2, 3)
    }

    fn validate_p(fixture: &Fixture) -> Result<Pubkey> {
        validate_profile(&fixture.account(), 0, 2, 3)
    }

    #[test]
    fn multisig_discriminator_matches_anchor_namespace() {
        let digest = Sha256::digest(b"account:Multisig");
        assert_eq!(&digest[..8], &SQUADS_MULTISIG_DISCRIMINATOR);
        assert_eq!(
            SQUADS_MULTISIG_DISCRIMINATOR,
            [0xe0, 0x74, 0x79, 0xba, 0x44, 0xa1, 0x4f, 0xec]
        );
    }

    #[test]
    fn initialize_args_carry_the_cluster_domain_and_provider_tuple() {
        let args = InitializeArgs {
            sponsor: key(1),
            management_recipient: key(2),
            cluster_domain: [0xC3; 32],
            provider_request_fee_lamports: 0x0102_0304_0506_0708,
            provider_fee_headroom_bps: 2_000,
            provider_program: key(3),
            provider_network_state: key(4),
            provider_treasury: key(5),
            provider_client: key(6),
            public_series_a_enabled: true,
        };
        let bytes = args.try_to_vec().unwrap();
        // 32+32+32 genesis, 8+2 provider fee, 4x32 provider tuple, 1 flag.
        assert_eq!(bytes.len(), 32 + 32 + 32 + 8 + 2 + 32 + 32 + 32 + 32 + 1);
        assert_eq!(bytes.len(), 235);
        assert_eq!(&bytes[64..96], &[0xC3; 32]);
        assert_eq!(&bytes[96..104], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&bytes[104..106], &2_000u16.to_le_bytes());
        assert_eq!(&bytes[106..138], key(3).as_ref());
        assert_eq!(&bytes[202..234], key(6).as_ref());
        assert_eq!(bytes[234], 1);
        assert_eq!(InitializeArgs::try_from_slice(&bytes).unwrap(), args);
    }

    #[test]
    fn fixture_follows_the_pinned_squads_layout() {
        // 8 disc + 32 create_key + 32 config_authority + 2 threshold + 4
        // time_lock + 8 + 8 indices + 1 rent_collector tag + 1 bump + 4 len
        // + 3 members x (32 + 1).
        let governance = Fixture::governance();
        assert_eq!(governance.bytes().len(), 8 + 32 + 32 + 2 + 4 + 8 + 8 + 1 + 1 + 4 + 3 * 33);
        assert_eq!(governance.bytes().len(), 199);
        let mut with_collector = governance;
        with_collector.rent_collector = Some(key(77));
        assert_eq!(with_collector.bytes().len(), 199 + 32);
    }

    #[test]
    fn validate_profile_returns_the_vault_pda_for_governance_and_guardian() {
        let governance = Fixture::governance();
        let governance_vault = validate_g(&governance).unwrap();
        assert_eq!(governance_vault, governance.vault(0));
        assert_ne!(governance_vault, governance.vault(1));
        assert_ne!(governance_vault, governance.multisig().0);

        let guardian = Fixture::guardian();
        let guardian_vault = validate_p(&guardian).unwrap();
        assert_eq!(guardian_vault, guardian.vault(0));
        assert_ne!(guardian_vault, governance_vault);

        // A populated rent_collector (Option tag 1 + 32 bytes) must not
        // desynchronize the members that follow it.
        let mut collector = Fixture::governance();
        collector.rent_collector = Some(key(77));
        assert_eq!(validate_g(&collector).unwrap(), governance_vault);

        // Trailing realloc padding after the member list is tolerated.
        let mut padded = Fixture::governance().bytes();
        padded.extend_from_slice(&[0u8; 64]);
        let padded = account(governance.multisig().0, SQUADS_PROGRAM_ID, padded, false, false);
        assert_eq!(
            validate_profile(&padded, GOVERNANCE_DELAY, 2, 3).unwrap(),
            governance_vault
        );
    }

    #[test]
    fn rejects_non_autonomous_threshold_delay_and_member_count() {
        let mut config_authority = Fixture::governance();
        config_authority.config_authority = key(9);
        assert_error(validate_g(&config_authority), "WrongGovernanceProfile");

        for threshold in [0u16, 1, 3] {
            let mut fixture = Fixture::governance();
            fixture.threshold = threshold;
            assert_error(validate_g(&fixture), "WrongGovernanceProfile");
        }

        // The governance delay is exact: neither shorter nor longer passes.
        for delay in [0u32, 60, 172_799, 172_801] {
            assert_error(validate_g(&Fixture::new(42, delay)), "WrongGovernanceProfile");
        }
        // The guardian delay is exact zero.
        for delay in [1u32, 60, 172_800] {
            assert_error(validate_p(&Fixture::new(43, delay)), "WrongGovernanceProfile");
        }
        // A guardian-shaped account is not a governance profile and vice versa.
        assert_error(validate_g(&Fixture::guardian()), "WrongGovernanceProfile");
        assert_error(validate_p(&Fixture::governance()), "WrongGovernanceProfile");

        let two = Fixture::governance().with_members(&[(key(1), 7), (key(2), 7)]);
        assert_error(validate_g(&two), "WrongGovernanceProfile");
        let four = Fixture::governance()
            .with_members(&[(key(1), 7), (key(2), 7), (key(3), 7), (key(4), 7)]);
        assert_error(validate_g(&four), "WrongGovernanceProfile");
        // The expected member count is a parameter, not a constant.
        assert_eq!(
            validate_profile(&four.account(), GOVERNANCE_DELAY, 2, 4).unwrap(),
            four.vault(0)
        );
    }

    #[test]
    fn rejects_bad_member_sets() {
        // Missing VOTE bit.
        for masks in [[5u8, 7, 7], [7, 1, 7], [7, 7, 4]] {
            assert_error(
                validate_g(&Fixture::governance().with_masks(masks)),
                "WrongGovernanceProfile",
            );
        }
        // Unknown permission bits.
        for unknown in [0x08u8, 0x80, 0xf8] {
            assert_error(
                validate_g(&Fixture::governance().with_masks([7, 7 | unknown, 7])),
                "WrongGovernanceProfile",
            );
        }
        // Nobody can initiate, or nobody can execute.
        assert_error(
            validate_g(&Fixture::governance().with_masks([2, 2, 6])),
            "WrongGovernanceProfile",
        );
        assert_error(
            validate_g(&Fixture::governance().with_masks([3, 3, 3])),
            "WrongGovernanceProfile",
        );
        // Default key.
        assert_error(
            validate_g(&Fixture::governance().with_members(&[
                (key(1), 7),
                (Pubkey::default(), 7),
                (key(3), 7),
            ])),
            "WrongGovernanceProfile",
        );
        // Duplicates, adjacent and not.
        for members in [
            [(key(1), 7), (key(1), 7), (key(3), 7)],
            [(key(1), 7), (key(2), 7), (key(1), 7)],
            [(key(1), 7), (key(2), 7), (key(2), 7)],
        ] {
            assert_error(
                validate_g(&Fixture::governance().with_members(&members)),
                "WrongGovernanceProfile",
            );
        }
        // Distinct initiate / vote / execute specialists are fine.
        validate_g(&Fixture::governance().with_masks([3, 2, 6])).unwrap();
    }

    #[test]
    fn rejects_wrong_owner_discriminator_pda_bump_and_truncation() {
        let governance = Fixture::governance();
        let valid = governance.bytes();
        let multisig_key = governance.multisig().0;

        let wrong_owner = account(multisig_key, system_program::ID, valid.clone(), false, false);
        assert_error(
            validate_profile(&wrong_owner, GOVERNANCE_DELAY, 2, 3),
            "WrongAccount",
        );

        let mut bad_disc = valid.clone();
        bad_disc[0] ^= 1;
        let bad_disc = account(multisig_key, SQUADS_PROGRAM_ID, bad_disc, false, false);
        assert_error(validate_profile(&bad_disc, GOVERNANCE_DELAY, 2, 3), "WrongAccount");

        // The pre-fix constant ended in 0xc8; it must not be accepted.
        let mut old_disc = valid.clone();
        old_disc[7] = 0xc8;
        let old_disc = account(multisig_key, SQUADS_PROGRAM_ID, old_disc, false, false);
        assert_error(validate_profile(&old_disc, GOVERNANCE_DELAY, 2, 3), "WrongAccount");

        let too_short = account(multisig_key, SQUADS_PROGRAM_ID, valid[..7].to_vec(), false, false);
        assert_error(validate_profile(&too_short, GOVERNANCE_DELAY, 2, 3), "WrongAccount");

        let wrong_key = account(key(99), SQUADS_PROGRAM_ID, valid.clone(), false, false);
        assert_error(validate_profile(&wrong_key, GOVERNANCE_DELAY, 2, 3), "WrongAccount");

        let mut bad_bump = governance.clone();
        bad_bump.bump_override = Some(governance.multisig().1 ^ 1);
        assert_error(validate_g(&bad_bump), "WrongAccount");

        // Body cut inside create_key, and one byte short of the last member.
        for length in [20usize, valid.len() - 1] {
            let truncated = account(
                multisig_key,
                SQUADS_PROGRAM_ID,
                valid[..length].to_vec(),
                false,
                false,
            );
            assert_error(
                validate_profile(&truncated, GOVERNANCE_DELAY, 2, 3),
                "WrongGovernanceProfile",
            );
        }

        // Option tag other than 0/1 for rent_collector.
        let mut bad_tag = valid;
        bad_tag[8 + 32 + 32 + 2 + 4 + 8 + 8] = 2;
        let bad_tag = account(multisig_key, SQUADS_PROGRAM_ID, bad_tag, false, false);
        assert_error(validate_profile(&bad_tag, GOVERNANCE_DELAY, 2, 3), "WrongGovernanceProfile");
    }

    #[test]
    fn require_vault_signer_demands_the_derived_vault_and_a_signature() {
        let governance = Fixture::governance();
        let multisig = governance.account();
        let vault0 = governance.vault(0);
        let vault1 = governance.vault(1);

        let signed = |address: Pubkey, signed: bool| {
            account(address, system_program::ID, vec![], signed, false)
        };
        require_vault_signer(&multisig, 0, &signed(vault0, true)).unwrap();
        require_vault_signer(&multisig, 1, &signed(vault1, true)).unwrap();

        // Right key, no signature.
        assert_error(
            require_vault_signer(&multisig, 0, &signed(vault0, false)),
            "NotAuthorized",
        );
        // Signature from some other key (a member, not the vault).
        assert_error(
            require_vault_signer(&multisig, 0, &signed(key(1), true)),
            "NotAuthorized",
        );
        // Another vault index of the same multisig.
        assert_error(
            require_vault_signer(&multisig, 0, &signed(vault1, true)),
            "NotAuthorized",
        );
        // The P vault cannot stand in for the G vault.
        assert_error(
            require_vault_signer(&multisig, 0, &signed(Fixture::guardian().vault(0), true)),
            "NotAuthorized",
        );
    }

    fn terms() -> ConfigTerms {
        ConfigTerms::from(&ConfigVersion::default_v1())
    }

    #[test]
    fn config_terms_accept_the_launch_rule_set_and_its_variations() {
        terms().validate().unwrap();

        let mut later = terms();
        later.effective_round_id = 9;
        later.front_end_load_bps = 0;
        later.a_bps = 5_000;
        later.b_bps = 1_500;
        later.provider_fee_headroom_bps = 10_000;
        later.public_series_a_enabled = true;
        later.validate().unwrap();
    }

    /// One staged-config mutation and its name.
    type BoundCase = (&'static str, fn(&mut ConfigTerms));

    #[test]
    fn config_terms_reject_each_bound_once() {
        let cases: [BoundCase; 43] = [
            ("plan rate over 100%", |t| t.plan_rates_bps_per_day[0] = 10_001),
            ("simple toll over 100%", |t| t.simple_toll_bps[2] = 10_001),
            ("jackpot over 100%", |t| t.jackpot_bps[1] = 10_001),
            ("front-end load over 100%", |t| t.front_end_load_bps = 10_001),
            ("seed over 100%", |t| t.seed_bps = 10_001),
            ("senior over 100%", |t| t.senior_bps = 10_001),
            ("a over 100%", |t| t.a_bps = 10_001),
            ("b over 100%", |t| t.b_bps = 10_001),
            ("headroom over 100%", |t| t.provider_fee_headroom_bps = 10_001),
            ("split short of 100%", |t| t.senior_bps = 3_400),
            ("split over 100%", |t| t.senior_bps = 3_600),
            ("a entitlement below 100%", |t| t.a_entitlement_bps = 9_999),
            ("b entitlement below 100%", |t| t.b_entitlement_bps = 9_999),
            ("zero plan duration", |t| t.plan_durations_s[1] = 0),
            ("zero quota window", |t| t.quota_window_s = 0),
            ("zero deadline", |t| t.deadline_seconds = 0),
            // The promotion window is frozen at 600 s (Codex M7 review).
            ("deadline one second short", |t| t.deadline_seconds = 599),
            ("deadline one second long", |t| t.deadline_seconds = 601),
            ("zero quota count", |t| t.quota_count = 0),
            ("quota count beyond the User array", |t| t.quota_count = QUOTA_CALLS + 1),
            ("page capacity too small", |t| t.page_capacity = PAGE_CAPACITY as u8 - 1),
            ("page capacity too large", |t| t.page_capacity = PAGE_CAPACITY as u8 + 1),
            ("toll day bounds equal", |t| t.simple_toll_day_bounds = [14, 14]),
            ("toll day bounds inverted", |t| t.simple_toll_day_bounds = [15, 14]),
            ("zero game minimum", |t| t.min_game = 0),
            ("zero series A minimum", |t| t.min_a = 0),
            ("zero first plan duration", |t| t.plan_durations_s[0] = 0),
            ("zero last plan duration", |t| t.plan_durations_s[2] = 0),
            ("b entitlement zero", |t| t.b_entitlement_bps = 0),
            // Spec §3.1 fixes the quota at three successes per 3,600 s and
            // `User::record_success` enforces exactly that, so any other
            // staged value would be silently ignored (Codex M5 review).
            ("quota count one", |t| t.quota_count = 1),
            ("quota count two", |t| t.quota_count = 2),
            ("quota window one second short", |t| t.quota_window_s = 3_599),
            ("quota window one second long", |t| t.quota_window_s = 3_601),
            // Consumed by fixed helpers; staging may only restate them (Codex
            // M6 follow-up): any other value would be ignored or unusable.
            ("simple rate one bps low", |t| t.plan_rates_bps_per_day[0] = 1_099),
            ("simple rate one bps high", |t| t.plan_rates_bps_per_day[0] = 1_101),
            ("a entitlement one bps low", |t| t.a_entitlement_bps = 12_399),
            ("a entitlement one bps high", |t| t.a_entitlement_bps = 12_401),
            ("b entitlement one bps low", |t| t.b_entitlement_bps = 11_599),
            ("b entitlement one bps high", |t| t.b_entitlement_bps = 11_601),
            ("seed half one bps low", |t| t.seed_bps = 4_999),
            ("seed half full toll", |t| t.seed_bps = 10_000),
            ("open cutoff nine", |t| t.open_cutoff = 9),
            ("open cutoff eleven", |t| t.open_cutoff = 11),
        ];
        for (name, mutate) in cases {
            let mut candidate = terms();
            mutate(&mut candidate);
            let error = candidate
                .validate()
                .expect_err(&format!("{name}: validator accepted the terms"));
            assert!(
                format!("{error:?}").contains("InvalidState"),
                "{name}: expected InvalidState, got {error:?}"
            );
        }
    }

    #[test]
    fn config_terms_round_trip_into_the_stored_account() {
        let launch = ConfigVersion::default_v1();
        assert_eq!(terms().into_config(1, 0, 0, None), launch);

        let staged = terms().into_config(2, 253, 1_000, None);
        assert_eq!((staged.version, staged.bump, staged.staged_at), (2, 253, 1_000));
        assert_eq!(staged.activated_at, None);
        assert_eq!(ConfigTerms::from(&staged), terms());
        let activated = terms().into_config(2, 253, 1_000, Some(2_000));
        assert_eq!(activated.activated_at, Some(2_000));

        // Everything but bump (1), version (4), staged_at (8) and the
        // activated_at Option<u64> (9) is in the instruction argument.
        let bytes = terms().try_to_vec().unwrap();
        assert_eq!(bytes.len(), ConfigVersion::SPACE - 8 - 1 - 4 - 8 - 9);
        assert_eq!(ConfigTerms::try_from_slice(&bytes).unwrap(), terms());
    }

    #[test]
    fn genesis_config_is_default_v1_plus_the_provider_arguments() {
        let mut args = InitializeArgs {
            sponsor: key(1),
            management_recipient: key(2),
            cluster_domain: [0xC3; 32],
            provider_request_fee_lamports: 4_200,
            provider_fee_headroom_bps: 1_500,
            provider_program: key(3),
            provider_network_state: key(4),
            provider_treasury: key(5),
            provider_client: key(6),
            public_series_a_enabled: true,
        };
        let mut config = ConfigVersion::default();
        write_genesis_config(&mut config, &args, 251, 1_700).unwrap();
        let launch = ConfigVersion::default_v1();
        assert_eq!((config.version, config.effective_round_id), (1, 1));
        assert_eq!((config.bump, config.staged_at), (251, 1_700));
        assert_eq!(config.activated_at, Some(1_700));
        assert_eq!(config.provider_request_fee_lamports, 4_200);
        assert_eq!(config.provider_fee_headroom_bps, 1_500);
        assert_eq!(
            [
                config.provider_program,
                config.provider_network_state,
                config.provider_treasury,
                config.provider_client,
            ],
            [key(3), key(4), key(5), key(6)]
        );
        assert!(config.public_series_a_enabled);
        // Economic terms are untouched launch values.
        assert_eq!(config.senior_bps, launch.senior_bps);
        assert_eq!(config.plan_durations_s, launch.plan_durations_s);
        assert_eq!(config.front_end_load_bps, launch.front_end_load_bps);

        // Genesis arguments go through the same bounds as staged terms.
        args.provider_fee_headroom_bps = 10_001;
        assert_error(
            write_genesis_config(&mut ConfigVersion::default(), &args, 251, 1_700),
            "InvalidState",
        );
    }

    #[test]
    fn recipient_rotation_protects_every_program_pda() {
        let protected = program_pda_keys();
        let expected = [
            POOL_SEED,
            ADMISSION_SEED,
            GAME_VAULT_SEED,
            OPS_SEED,
            OPS_CASH_SEED,
            TICKET_SEED,
        ]
        .map(|seed| Pubkey::find_program_address(&[seed], &crate::ID).0);
        assert_eq!(protected, expected);
        for (index, address) in protected.iter().enumerate() {
            assert!(protected[..index].iter().all(|other| other != address));
            assert_error(
                reject_protected_destination(address, &protected),
                "ProtectedDestination",
            );
        }
        reject_protected_destination(&key(5), &protected).unwrap();
        assert_eq!((RECIPIENT_MANAGEMENT, RECIPIENT_SPONSOR), (0, 1));
    }

    #[test]
    fn rejects_equal_governance_guardian_and_accepts_loader_binding() {
        let same = Pubkey::new_from_array([8u8; 32]);
        assert_error(require_distinct_authorities(&same, &same), "InvalidState");

        let governance = Pubkey::new_from_array([7u8; 32]);
        let program_data_key = bpf_loader_upgradeable::get_program_data_address(&crate::ID);
        let program = account(
            crate::ID,
            bpf_loader_upgradeable::ID,
            [2u32.to_le_bytes().as_slice(), program_data_key.as_ref()].concat(),
            false,
            true,
        );
        let program_data = account(
            program_data_key,
            bpf_loader_upgradeable::ID,
            [
                3u32.to_le_bytes().as_slice(),
                0u64.to_le_bytes().as_slice(),
                &[1],
                governance.as_ref(),
            ]
            .concat(),
            false,
            false,
        );
        validate_program_binding(&program, &program_data, &governance).unwrap();

        let wrong_pointer = account(
            crate::ID,
            bpf_loader_upgradeable::ID,
            [2u32.to_le_bytes().as_slice(), Pubkey::new_from_array([4u8; 32]).as_ref()].concat(),
            false,
            true,
        );
        assert_error(
            validate_program_binding(&wrong_pointer, &program_data, &governance),
            "WrongAccount",
        );

        let wrong_authority = Pubkey::new_from_array([6u8; 32]);
        let wrong_data = account(
            program_data_key,
            bpf_loader_upgradeable::ID,
            [
                3u32.to_le_bytes().as_slice(),
                0u64.to_le_bytes().as_slice(),
                &[1],
                wrong_authority.as_ref(),
            ]
            .concat(),
            false,
            false,
        );
        assert_error(
            validate_program_binding(&program, &wrong_data, &governance),
            "NotAuthorized",
        );

        let wrong_variant = account(
            program_data_key,
            bpf_loader_upgradeable::ID,
            [
                2u32.to_le_bytes().as_slice(),
                0u64.to_le_bytes().as_slice(),
                &[1],
                governance.as_ref(),
            ]
            .concat(),
            false,
            false,
        );
        assert_error(
            validate_program_binding(&program, &wrong_variant, &governance),
            "WrongAccount",
        );
    }

    // ----- Milestone 4: provisioning, rent components, cleanup (pure rules) -----

    /// A pool whose every page counter is distinct, so a wrong counter shows.
    fn counted_pool() -> Pool {
        Pool {
            round_id: 4,
            registry_pages: 3,
            snapshot_pages: 5,
            participant_pages: 7,
            next_close_id: 2,
            current_close_id: Some(1),
            ..Pool::default()
        }
    }

    fn counted_close(close_id: u64) -> CloseRecord {
        CloseRecord {
            close_id,
            provisioned_preferred_pages: 11,
            provisioned_fallback_pages: 13,
            ..CloseRecord::default()
        }
    }

    fn le(value: u64) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    #[test]
    fn page_kind_and_funding_borsh_encodings_are_frozen() {
        assert_eq!(PageKind::Registry.try_to_vec().unwrap(), vec![0]);
        assert_eq!(PageKind::Snapshot.try_to_vec().unwrap(), vec![1]);
        assert_eq!(
            PageKind::Participant { round: 7 }.try_to_vec().unwrap(),
            [vec![2], le(7)].concat()
        );
        assert_eq!(
            PageKind::Candidate { close_id: 9, set_tag: 1 }.try_to_vec().unwrap(),
            [vec![3], le(9), vec![1]].concat()
        );
        assert_eq!(Funding::Free.try_to_vec().unwrap(), vec![0]);
        assert_eq!(Funding::Caller.try_to_vec().unwrap(), vec![1]);
        assert_eq!(Funding::LockedClose.try_to_vec().unwrap(), vec![2]);
        for kind in [
            PageKind::Registry,
            PageKind::Snapshot,
            PageKind::Participant { round: u64::MAX },
            PageKind::Candidate { close_id: 3, set_tag: 0 },
        ] {
            let bytes = kind.try_to_vec().unwrap();
            assert_eq!(PageKind::try_from_slice(&bytes).unwrap(), kind);
        }
    }

    #[test]
    fn page_target_resolves_seeds_space_and_next_index_per_kind() {
        let pool = counted_pool();
        let close = counted_close(2);

        let registry = page_target(&PageKind::Registry, 3, &pool, None).unwrap();
        assert_eq!(registry.seeds, vec![REGISTRY_SEED.to_vec(), le(3)]);
        assert_eq!((registry.space, registry.expected_index), (RegistryPage::SPACE, 3));

        let snapshot = page_target(&PageKind::Snapshot, 5, &pool, None).unwrap();
        assert_eq!(snapshot.seeds, vec![SNAPSHOT_SEED.to_vec(), le(5)]);
        assert_eq!((snapshot.space, snapshot.expected_index), (SnapshotPage::SPACE, 5));

        let participant =
            page_target(&PageKind::Participant { round: 4 }, 7, &pool, None).unwrap();
        assert_eq!(participant.seeds, vec![PARTICIPANTS_SEED.to_vec(), le(4), le(7)]);
        assert_eq!((participant.space, participant.expected_index), (ParticipantPage::SPACE, 7));

        let preferred = page_target(
            &PageKind::Candidate { close_id: 2, set_tag: 0 },
            11,
            &pool,
            Some(&close),
        )
        .unwrap();
        assert_eq!(preferred.seeds, vec![CANDIDATES_SEED.to_vec(), le(2), vec![0], le(11)]);
        assert_eq!((preferred.space, preferred.expected_index), (CandidatePage::SPACE, 11));

        let fallback = page_target(
            &PageKind::Candidate { close_id: 2, set_tag: 1 },
            13,
            &pool,
            Some(&close),
        )
        .unwrap();
        assert_eq!(fallback.seeds, vec![CANDIDATES_SEED.to_vec(), le(2), vec![1], le(13)]);
        assert_eq!(fallback.expected_index, 13);

        // The two sets (and kinds) never share an address.
        let address = |target: &PageTarget| {
            let refs: Vec<&[u8]> = target.seeds.iter().map(|seed| seed.as_slice()).collect();
            Pubkey::find_program_address(&refs, &crate::ID).0
        };
        let other_set =
            page_target(&PageKind::Candidate { close_id: 2, set_tag: 1 }, 11, &pool, Some(&close))
                .unwrap();
        assert_ne!(address(&preferred), address(&other_set));
        assert_ne!(address(&registry), address(&snapshot));
    }

    #[test]
    fn sequential_index_counter_is_per_kind_and_per_candidate_set() {
        let mut pool = counted_pool();
        let mut close = counted_close(2);
        let expect = |kind: PageKind, pool: &Pool, close: &CloseRecord| {
            page_target(&kind, 0, pool, Some(close)).unwrap().expected_index
        };
        let preferred = PageKind::Candidate { close_id: 2, set_tag: 0 };
        let fallback = PageKind::Candidate { close_id: 2, set_tag: 1 };
        assert_eq!(expect(PageKind::Registry, &pool, &close), 3);
        assert_eq!(expect(PageKind::Snapshot, &pool, &close), 5);
        assert_eq!(expect(PageKind::Participant { round: 4 }, &pool, &close), 7);
        assert_eq!(expect(preferred, &pool, &close), 11);
        assert_eq!(expect(fallback, &pool, &close), 13);

        // Moving one counter moves only its own kind or set.
        pool.registry_pages = 20;
        close.provisioned_fallback_pages = 21;
        assert_eq!(expect(PageKind::Registry, &pool, &close), 20);
        assert_eq!(expect(PageKind::Snapshot, &pool, &close), 5);
        assert_eq!(expect(preferred, &pool, &close), 11);
        assert_eq!(expect(fallback, &pool, &close), 21);
    }

    #[test]
    fn page_target_rejects_kinds_that_cannot_be_provisioned_now() {
        let pool = counted_pool();
        let close = counted_close(2);
        // Participant pages exist only for the pool's current round.
        assert_error(page_target(&PageKind::Participant { round: 3 }, 0, &pool, None), "InvalidState");
        assert_error(page_target(&PageKind::Participant { round: 5 }, 0, &pool, None), "InvalidState");
        // Only set tags 0 and 1 exist.
        assert_error(
            page_target(&PageKind::Candidate { close_id: 2, set_tag: 2 }, 0, &pool, Some(&close)),
            "InvalidState",
        );
        assert_error(
            page_target(&PageKind::Candidate { close_id: 2, set_tag: u8::MAX }, 0, &pool, Some(&close)),
            "InvalidState",
        );
        // A candidate page needs its close record, and the record's own id.
        let preferred = PageKind::Candidate { close_id: 2, set_tag: 0 };
        assert_error(page_target(&preferred, 0, &pool, None), "InvalidState");
        assert_error(page_target(&preferred, 0, &pool, Some(&counted_close(9))), "InvalidState");
        // Only the next close or the close in progress may receive pages.
        let stale = PageKind::Candidate { close_id: 5, set_tag: 0 };
        assert_error(page_target(&stale, 0, &pool, Some(&counted_close(5))), "InvalidState");
        let older = Pool { current_close_id: Some(1), next_close_id: 3, ..counted_pool() };
        assert!(page_target(
            &PageKind::Candidate { close_id: 1, set_tag: 0 },
            0,
            &older,
            Some(&counted_close(1))
        )
        .is_ok());
        assert_error(
            page_target(
                &PageKind::Candidate { close_id: 2, set_tag: 0 },
                0,
                &older,
                Some(&counted_close(2)),
            ),
            "InvalidState",
        );
        // With no close in progress the next close is still provisionable.
        let idle = Pool { current_close_id: None, ..counted_pool() };
        assert!(page_target(&preferred, 0, &idle, Some(&close)).is_ok());
        let idle_old = Pool { current_close_id: None, ..older };
        assert_error(
            page_target(
                &PageKind::Candidate { close_id: 1, set_tag: 0 },
                0,
                &idle_old,
                Some(&counted_close(1)),
            ),
            "InvalidState",
        );
    }

    #[test]
    fn locked_close_funding_permission_matrix() {
        let pool = counted_pool(); // close 1 in progress, close 2 next
        let mut current = counted_close(1);
        current.required_pages_per_set = 4;
        let next = counted_close(2);
        let candidate = |close_id| PageKind::Candidate { close_id, set_tag: 0 };

        // Free and Caller are never restricted by this rule.
        for funding in [Funding::Free, Funding::Caller] {
            for kind in [PageKind::Registry, PageKind::Snapshot, candidate(2)] {
                assert!(locked_permission(&kind, funding, 99, &pool, Some(&next)).is_ok());
            }
        }
        // LockedClose is for candidate pages only.
        for kind in [PageKind::Registry, PageKind::Snapshot, PageKind::Participant { round: 4 }] {
            assert_error(
                locked_permission(&kind, Funding::LockedClose, 0, &pool, Some(&current)),
                "LockNotPermitted",
            );
        }
        // ... of the close in progress, not the next close.
        assert_error(
            locked_permission(&candidate(2), Funding::LockedClose, 0, &pool, Some(&next)),
            "LockNotPermitted",
        );
        let idle = Pool { current_close_id: None, ..counted_pool() };
        assert_error(
            locked_permission(&candidate(2), Funding::LockedClose, 0, &idle, Some(&next)),
            "LockNotPermitted",
        );
        // ... strictly inside required_pages_per_set.
        for index in [0, 3] {
            assert!(
                locked_permission(&candidate(1), Funding::LockedClose, index, &pool, Some(&current))
                    .is_ok()
            );
        }
        for index in [4, 5, u64::MAX] {
            assert_error(
                locked_permission(&candidate(1), Funding::LockedClose, index, &pool, Some(&current)),
                "LockNotPermitted",
            );
        }
        // A close that reserved nothing locks nothing.
        let empty = counted_close(1);
        assert_error(
            locked_permission(&candidate(1), Funding::LockedClose, 0, &pool, Some(&empty)),
            "LockNotPermitted",
        );
        assert_error(
            locked_permission(&candidate(1), Funding::LockedClose, 0, &pool, None),
            "LockNotPermitted",
        );
    }

    /// M8: settlement happens once at terminal promotion; a deferred owner
    /// refund (`reserve_settled` with `locked_owner_remaining > 0`) is owed to
    /// the owner destination and may not be spent as page rent by anyone.
    #[test]
    fn a_settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable() {
        let pool = counted_pool(); // close 1 is still the close in progress
        let mut open = counted_close(1);
        open.required_pages_per_set = 4;
        open.locked_owner_remaining = 5_000_000;
        let candidate = |set_tag| PageKind::Candidate { close_id: 1, set_tag };
        // Unsettled: the owner partition is still the close's spendable reserve.
        for set_tag in [0, 1] {
            assert!(locked_permission(&candidate(set_tag), Funding::LockedClose, 1, &pool, Some(&open)).is_ok());
        }
        // Settled with a deferred owner refund: the same page is refused.
        let mut deferred = counted_close(1);
        deferred.required_pages_per_set = 4;
        deferred.locked_owner_remaining = 5_000_000;
        deferred.reserve_settled = true;
        for set_tag in [0, 1] {
            assert_error(
                locked_permission(&candidate(set_tag), Funding::LockedClose, 1, &pool, Some(&deferred)),
                "LockNotPermitted",
            );
        }
        // Free and Caller funding never touch the partitions and stay open.
        for funding in [Funding::Free, Funding::Caller] {
            assert!(locked_permission(&candidate(0), funding, 1, &pool, Some(&deferred)).is_ok());
        }
    }

    #[test]
    fn locked_split_pays_sponsor_partition_first_then_owner() {
        let cases: [(u64, u64, u64, (u64, u64)); 9] = [
            (10, 5, 0, (0, 0)),
            (10, 5, 4, (4, 0)),
            (10, 5, 10, (10, 0)),
            (10, 5, 12, (10, 2)),
            (10, 5, 15, (10, 5)),
            (0, 5, 5, (0, 5)),
            (7, 0, 7, (7, 0)),
            (u64::MAX, u64::MAX, u64::MAX, (u64::MAX, 0)),
            (1, u64::MAX, u64::MAX, (1, u64::MAX - 1)),
        ];
        for (sponsor, owner, rent, expected) in cases {
            let (sponsor_part, owner_part) = crate::split_locked(sponsor, owner, rent).unwrap();
            assert_eq!((sponsor_part, owner_part), expected, "split of {rent}");
            // Exactly the rent leaves the partitions, and neither goes negative.
            assert_eq!(sponsor_part + owner_part, rent);
            assert!(sponsor_part <= sponsor && owner_part <= owner);
        }
    }

    #[test]
    fn locked_split_refuses_when_the_two_partitions_together_fall_short() {
        for (sponsor, owner, rent) in [
            (10, 5, 16),
            (0, 0, 1),
            (0, 4, 5),
            (9, 0, 10),
            (u64::MAX - 1, 0, u64::MAX),
            (1, u64::MAX - 2, u64::MAX),
        ] {
            assert_error(crate::split_locked(sponsor, owner, rent), "LockNotPermitted");
        }
    }

    #[test]
    fn provisioning_counters_advance_per_kind_and_never_cross_sets() {
        let mut pool = counted_pool();
        for (kind, read) in [
            (PageKind::Registry, (|p: &Pool| p.registry_pages) as fn(&Pool) -> u64),
            (PageKind::Snapshot, |p| p.snapshot_pages),
            (PageKind::Participant { round: 4 }, |p| p.participant_pages),
        ] {
            let before = read(&pool);
            record_provisioned(&kind, &mut pool, None).unwrap();
            assert_eq!(read(&pool), before + 1);
        }
        assert_eq!((pool.registry_pages, pool.snapshot_pages, pool.participant_pages), (4, 6, 8));

        // Pre-capture preferred page: raises its own set and mirrors it into
        // `existing_preferred_pages`; the fallback set is untouched.
        let mut close = counted_close(2);
        close.existing_fallback_pages = 6;
        record_provisioned(
            &PageKind::Candidate { close_id: 2, set_tag: 0 },
            &mut pool,
            Some(&mut close),
        )
        .unwrap();
        assert_eq!((close.provisioned_preferred_pages, close.existing_preferred_pages), (12, 12));
        assert_eq!((close.provisioned_fallback_pages, close.existing_fallback_pages), (13, 6));

        // Pre-capture fallback page: the mirror image.
        record_provisioned(
            &PageKind::Candidate { close_id: 2, set_tag: 1 },
            &mut pool,
            Some(&mut close),
        )
        .unwrap();
        assert_eq!((close.provisioned_fallback_pages, close.existing_fallback_pages), (14, 14));
        assert_eq!((close.provisioned_preferred_pages, close.existing_preferred_pages), (12, 12));

        // Post-capture (close in progress): provisioned grows, existing is frozen.
        let mut current = counted_close(1);
        current.existing_preferred_pages = 2;
        current.existing_fallback_pages = 3;
        record_provisioned(
            &PageKind::Candidate { close_id: 1, set_tag: 0 },
            &mut pool,
            Some(&mut current),
        )
        .unwrap();
        record_provisioned(
            &PageKind::Candidate { close_id: 1, set_tag: 1 },
            &mut pool,
            Some(&mut current),
        )
        .unwrap();
        assert_eq!((current.provisioned_preferred_pages, current.existing_preferred_pages), (12, 2));
        assert_eq!((current.provisioned_fallback_pages, current.existing_fallback_pages), (14, 3));

        // Candidate counters never touch the pool's page counters.
        assert_eq!((pool.registry_pages, pool.snapshot_pages, pool.participant_pages), (4, 6, 8));
    }

    #[test]
    fn provisioning_counters_fail_closed() {
        let mut pool = counted_pool();
        let mut close = counted_close(2);
        assert_error(
            record_provisioned(&PageKind::Candidate { close_id: 2, set_tag: 2 }, &mut pool, Some(&mut close)),
            "InvalidState",
        );
        assert_error(
            record_provisioned(&PageKind::Candidate { close_id: 2, set_tag: 0 }, &mut pool, None),
            "InvalidState",
        );
        pool.registry_pages = u64::MAX;
        assert_error(record_provisioned(&PageKind::Registry, &mut pool, None), "Overflow");
        close.provisioned_fallback_pages = u64::MAX;
        assert_error(
            record_provisioned(&PageKind::Candidate { close_id: 2, set_tag: 1 }, &mut pool, Some(&mut close)),
            "Overflow",
        );
        // A failed bump leaves the counter where it was.
        assert_eq!(close.provisioned_fallback_pages, u64::MAX);
        assert_eq!(close.existing_fallback_pages, 0);
    }

    #[test]
    fn closability_matrix_per_kind_and_predicate() {
        let finalized = |round: u64| CloseRecord { round_id: round, finalized: true, ..counted_close(5) };
        let applied = Promotion::Applied { winner: key(1), entitlement: 1 };
        let skipped = Promotion::Skipped { reason: 0, stage: 0 };
        let terminal = |close_id: u64, promotion: Promotion| CloseRecord { promotion, ..counted_close(close_id) };

        // R3: Registry and Snapshot are never closable, whatever the record says.
        let everything = CloseRecord { finalized: true, promotion: applied, ..counted_close(5) };
        assert!(!closable(&PageKind::Registry, &everything, true));
        assert!(!closable(&PageKind::Snapshot, &everything, true));

        // Participant: that round's close record is finalized.
        let participant = PageKind::Participant { round: 3 };
        assert!(closable(&participant, &finalized(3), true));
        assert!(!closable(&participant, &finalized(2), true), "another round's record");
        assert!(!closable(&participant, &CloseRecord { round_id: 3, ..counted_close(5) }, true));
        assert!(!closable(&participant, &finalized(3), false), "non-canonical record");
        assert!(
            !closable(&participant, &terminal(5, applied), true),
            "promotion alone does not finalize a round"
        );

        // Candidate: its own close record's promotion is terminal.
        let candidate = PageKind::Candidate { close_id: 5, set_tag: 0 };
        assert!(closable(&candidate, &terminal(5, applied), true));
        assert!(closable(&candidate, &terminal(5, skipped), true));
        assert!(!closable(&candidate, &terminal(5, Promotion::Pending), true));
        assert!(
            !closable(&candidate, &terminal(6, applied), true),
            "a newer close's terminal state cannot close an older close's page"
        );
        assert!(!closable(&candidate, &terminal(5, applied), false), "non-canonical record");
        assert!(
            !closable(&candidate, &finalized(1), true),
            "finalization alone does not terminate a promotion"
        );
        // The fallback set follows the same close record.
        assert!(closable(
            &PageKind::Candidate { close_id: 5, set_tag: 1 },
            &terminal(5, skipped),
            true
        ));
    }

    #[test]
    fn closure_split_returns_components_exactly_and_routes_the_rest_to_excess() {
        let rent = RentComponents { ops: 7, owner: 3, owner_refund: key(9) };
        assert_eq!(closure_split(&rent, 10).unwrap(), (7, 3, 0));
        assert_eq!(closure_split(&rent, 12).unwrap(), (7, 3, 2), "unsolicited credits");
        assert_error(closure_split(&rent, 9), "InvalidState");
        assert_eq!(closure_split(&RentComponents::default(), 5).unwrap(), (0, 0, 5));
        assert_eq!(
            closure_split(&RentComponents { ops: 0, owner: 4, owner_refund: key(1) }, 4).unwrap(),
            (0, 4, 0)
        );
        let huge = RentComponents { ops: u64::MAX, owner: 1, owner_refund: key(1) };
        assert_error(closure_split(&huge, u64::MAX), "Overflow");
    }

    #[test]
    fn fresh_account_is_empty_and_system_owned() {
        assert!(is_fresh_account(&system_program::ID, true));
        assert!(!is_fresh_account(&system_program::ID, false));
        assert!(!is_fresh_account(&crate::ID, true), "an emptied but program-owned page");
        assert!(!is_fresh_account(&crate::ID, false));
        assert!(!is_fresh_account(&key(7), true));
    }

    #[test]
    fn close_record_address_is_canonical_per_id() {
        let (one, bump) = close_record_address(1);
        assert_eq!(
            Pubkey::create_program_address(&[CLOSE_SEED, &le(1), &[bump]], &crate::ID).unwrap(),
            one
        );
        assert_ne!(one, close_record_address(2).0);
        assert_ne!(close_record_address(0).0, close_record_address(u64::MAX).0);
    }

    // ----- Follow-up: need bound for sponsor-funded pages -----

    /// A pool with `registry_len` and `participant_count` set; next close is 2,
    /// close 1 is in progress.
    fn needy_pool(registry_len: u64, participant_count: u64) -> Pool {
        Pool { registry_len, participant_count, ..counted_pool() }
    }

    #[test]
    fn free_registry_and_snapshot_pages_are_bounded_by_registry_extent() {
        // 16 registry rows fill pages 0 and 1; page 2 is the (possibly empty) tail.
        let pool = needy_pool(2 * PAGE_CAPACITY as u64, 0);
        for kind in [PageKind::Registry, PageKind::Snapshot] {
            for index in [0, 1, 2] {
                assert!(free_need_bound(&kind, Funding::Free, index, &pool).is_ok(), "{kind:?} {index}");
            }
            assert_error(free_need_bound(&kind, Funding::Free, 3, &pool), "WrongIndex");
            assert_error(free_need_bound(&kind, Funding::Free, u64::MAX, &pool), "WrongIndex");
        }
        // A partly filled tail needs no further page: 17 rows -> pages 0..=2.
        let partial = needy_pool(2 * PAGE_CAPACITY as u64 + 1, 0);
        assert!(free_need_bound(&PageKind::Registry, Funding::Free, 2, &partial).is_ok());
        assert_error(free_need_bound(&PageKind::Registry, Funding::Free, 3, &partial), "WrongIndex");
        // An empty registry still allows its first page, and only that.
        let empty = needy_pool(0, 0);
        assert!(free_need_bound(&PageKind::Snapshot, Funding::Free, 0, &empty).is_ok());
        assert_error(free_need_bound(&PageKind::Snapshot, Funding::Free, 1, &empty), "WrongIndex");
        // One row below a boundary: 7 rows still fit page 0, so only page 0 is allowed.
        let seven = needy_pool(PAGE_CAPACITY as u64 - 1, 0);
        assert!(free_need_bound(&PageKind::Registry, Funding::Free, 0, &seven).is_ok());
        assert_error(free_need_bound(&PageKind::Registry, Funding::Free, 1, &seven), "WrongIndex");
    }

    #[test]
    fn free_participant_pages_are_bounded_by_participant_count() {
        let kind = PageKind::Participant { round: 4 };
        // 9 participants fill page 0 and start page 1.
        let pool = needy_pool(0, PAGE_CAPACITY as u64 + 1);
        assert!(free_need_bound(&kind, Funding::Free, 0, &pool).is_ok());
        assert!(free_need_bound(&kind, Funding::Free, 1, &pool).is_ok());
        assert_error(free_need_bound(&kind, Funding::Free, 2, &pool), "WrongIndex");
        // 8 participants exactly fill page 0; page 1 is the allowed tail, page 2 is not.
        let full = needy_pool(0, PAGE_CAPACITY as u64);
        assert!(free_need_bound(&kind, Funding::Free, 1, &full).is_ok());
        assert_error(free_need_bound(&kind, Funding::Free, 2, &full), "WrongIndex");
        // No participants yet: the first page only.
        let none = needy_pool(0, 0);
        assert!(free_need_bound(&kind, Funding::Free, 0, &none).is_ok());
        assert_error(free_need_bound(&kind, Funding::Free, 1, &none), "WrongIndex");
        // The registry's size is irrelevant to participant pages.
        let big_registry = needy_pool(1_000, 0);
        assert_error(free_need_bound(&kind, Funding::Free, 1, &big_registry), "WrongIndex");
    }

    #[test]
    fn free_candidate_pages_for_the_next_close_are_bounded_by_participants() {
        let next = PageKind::Candidate { close_id: 2, set_tag: 0 };
        let fallback = PageKind::Candidate { close_id: 2, set_tag: 1 };
        for kind in [next, fallback] {
            // 9 participants need 2 pages per set: indices 0 and 1, not 2.
            let nine = needy_pool(0, PAGE_CAPACITY as u64 + 1);
            assert!(free_need_bound(&kind, Funding::Free, 0, &nine).is_ok());
            assert!(free_need_bound(&kind, Funding::Free, 1, &nine).is_ok());
            assert_error(free_need_bound(&kind, Funding::Free, 2, &nine), "WrongIndex");
            // 8 participants need exactly 1 page: no tail allowance for candidates.
            let eight = needy_pool(0, PAGE_CAPACITY as u64);
            assert!(free_need_bound(&kind, Funding::Free, 0, &eight).is_ok());
            assert_error(free_need_bound(&kind, Funding::Free, 1, &eight), "WrongIndex");
            // With nobody yet, one page (max(count, 1)) may be pre-provisioned.
            let none = needy_pool(0, 0);
            assert!(free_need_bound(&kind, Funding::Free, 0, &none).is_ok());
            assert_error(free_need_bound(&kind, Funding::Free, 1, &none), "WrongIndex");
            // Registry size does not matter.
            let big_registry = needy_pool(1_000, 0);
            assert_error(free_need_bound(&kind, Funding::Free, 1, &big_registry), "WrongIndex");
        }
    }

    #[test]
    fn free_candidate_pages_for_the_current_close_have_no_extra_bound() {
        let current = PageKind::Candidate { close_id: 1, set_tag: 1 };
        let pool = needy_pool(0, 0);
        for index in [0, 1, 100, u64::MAX] {
            assert!(free_need_bound(&current, Funding::Free, index, &pool).is_ok(), "{index}");
        }
    }

    #[test]
    fn need_bound_applies_to_free_funding_only() {
        let pool = needy_pool(0, 0);
        for funding in [Funding::Caller, Funding::LockedClose] {
            for kind in [
                PageKind::Registry,
                PageKind::Snapshot,
                PageKind::Participant { round: 4 },
                PageKind::Candidate { close_id: 2, set_tag: 0 },
                PageKind::Candidate { close_id: 1, set_tag: 0 },
            ] {
                assert!(free_need_bound(&kind, funding, u64::MAX, &pool).is_ok(), "{funding:?} {kind:?}");
            }
        }
        // ... while Free is refused for the same far-ahead indices (except the current close).
        for kind in [
            PageKind::Registry,
            PageKind::Snapshot,
            PageKind::Participant { round: 4 },
            PageKind::Candidate { close_id: 2, set_tag: 0 },
        ] {
            assert_error(free_need_bound(&kind, Funding::Free, 1, &pool), "WrongIndex");
        }
    }

    // ----- Follow-up: rent accounting reachable without AccountInfo -----

    fn funded_ops(free: u64, locked: u64) -> Ops {
        Ops { free_balance: free, locked_remaining_total: locked, ..Ops::default() }
    }

    #[test]
    fn free_charge_debits_free_balance_and_counts_rent_funded() {
        let mut ops = funded_ops(1_000, 40);
        ops.total_rent_funded = 5;
        crate::charge_free(&mut ops, 300).unwrap();
        assert_eq!((ops.free_balance, ops.locked_remaining_total, ops.total_rent_funded), (700, 40, 305));
        // A zero debit (fully prefunded target) is a no-op.
        crate::charge_free(&mut ops, 0).unwrap();
        assert_eq!((ops.free_balance, ops.total_rent_funded), (700, 305));
        // Exactly the free balance is spendable; locked cash is not.
        crate::charge_free(&mut ops, 700).unwrap();
        assert_eq!((ops.free_balance, ops.locked_remaining_total, ops.total_rent_funded), (0, 40, 1_005));
    }

    #[test]
    fn free_charge_fails_closed() {
        let mut ops = funded_ops(99, 1_000);
        let before = ops.try_to_vec().unwrap();
        assert_error(crate::charge_free(&mut ops, 100), "InsufficientOperations");
        assert_eq!(ops.try_to_vec().unwrap(), before, "locked cash never funds a Free page");
        let mut full = funded_ops(10, 0);
        full.total_rent_funded = u128::MAX;
        let before = full.try_to_vec().unwrap();
        assert_error(crate::charge_free(&mut full, 1), "Overflow");
        assert_eq!(full.try_to_vec().unwrap(), before);
        // Zero debit at the counter ceiling still succeeds.
        crate::charge_free(&mut full, 0).unwrap();
    }

    #[test]
    fn locked_charge_decrements_partitions_sponsor_first_and_records_everything() {
        let mut ops = funded_ops(0, 150);
        ops.total_rent_funded = 7;
        let mut close = CloseRecord {
            locked_sponsor_remaining: 100,
            locked_owner_remaining: 50,
            ..CloseRecord::default()
        };
        // Sponsor-only spend.
        assert_eq!(crate::charge_locked(&mut ops, &mut close, 60).unwrap(), (60, 0));
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (40, 50));
        assert_eq!(ops.locked_remaining_total, 90);
        assert_eq!((close.spent.candidate_rent_s, close.spent.candidate_rent_o), (60, 0));
        assert_eq!(ops.total_rent_funded, 67);
        // Straddling spend: rest of the sponsor partition, then owner.
        assert_eq!(crate::charge_locked(&mut ops, &mut close, 55).unwrap(), (40, 15));
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 35));
        assert_eq!(ops.locked_remaining_total, 35);
        assert_eq!((close.spent.candidate_rent_s, close.spent.candidate_rent_o), (100, 15));
        assert_eq!(ops.total_rent_funded, 122);
        // Partitions always equal what remains locked; free_balance never moved.
        assert_eq!(close.locked_sponsor_remaining + close.locked_owner_remaining, ops.locked_remaining_total);
        assert_eq!(ops.free_balance, 0);
        // Exact exhaustion.
        assert_eq!(crate::charge_locked(&mut ops, &mut close, 35).unwrap(), (0, 35));
        assert_eq!((ops.locked_remaining_total, ops.total_rent_funded), (0, 157));
        assert_eq!((close.spent.candidate_rent_s, close.spent.candidate_rent_o), (100, 50));
    }

    #[test]
    fn locked_charge_fails_closed_and_leaves_both_records_untouched() {
        let base_close = CloseRecord {
            locked_sponsor_remaining: 10,
            locked_owner_remaining: 5,
            ..CloseRecord::default()
        };
        let cases: [(Ops, &str); 3] = [
            (funded_ops(0, 15), "LockNotPermitted"), // 16 > 10 + 5
            (funded_ops(0, 3), "InvalidState"),      // aggregate smaller than the debit
            (
                Ops { total_rent_funded: u128::MAX, ..funded_ops(0, 15) },
                "Overflow",
            ),
        ];
        for (index, (mut ops, expected)) in cases.into_iter().enumerate() {
            let debit = if index == 0 { 16 } else { 15 };
            let mut close = base_close.clone();
            let (ops_before, close_before) = (ops.try_to_vec().unwrap(), close.try_to_vec().unwrap());
            assert_error(crate::charge_locked(&mut ops, &mut close, debit), expected);
            assert_eq!(ops.try_to_vec().unwrap(), ops_before, "{expected}: ops unchanged");
            assert_eq!(close.try_to_vec().unwrap(), close_before, "{expected}: close unchanged");
        }
    }
}

#[cfg(test)]
mod review_pins {
    //! M10 review SEC-1 / ECO-2: the Carried Interest table may only be restated.
    use super::*;
    use crate::state::ConfigVersion;

    #[test]
    fn validate_pins_the_simple_toll_table_and_day_bounds() {
        let canonical = ConfigTerms::from(&ConfigVersion::default_v1());
        canonical.validate().expect("the canonical table validates");
        let mut table = canonical;
        table.simple_toll_bps = [1_200, 750, 250];
        assert!(table.validate().is_err(), "a divergent late tier would strand every Simple position opened under it");
        let mut bounds = canonical;
        bounds.simple_toll_day_bounds = [7, 15];
        assert!(bounds.validate().is_err(), "a divergent tier boundary mis-tiers days 14..15");
        let mut early = canonical;
        early.simple_toll_bps[0] = 1_100;
        assert!(early.validate().is_err());
    }
}
