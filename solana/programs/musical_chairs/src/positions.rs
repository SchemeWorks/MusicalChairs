//! Position admission, Series-A funding, and position payout handlers.
//!
//! Five instructions live here: `open_position`, `fund_series_a`,
//! `withdraw_simple`, `settle_compounding` and `close_simple_position`.
//! Every handler runs its checks against read-only views first and only then
//! moves lamports or writes state.  Anything that can be decided without an
//! `AccountInfo` is a pure function below, so the arithmetic and the check
//! order are unit-tested on the host; the account plumbing around them is
//! exercised by the LiteSVM financial harness.
//!
//! Custody rules shared by all of them: game cash only ever moves through
//! [`crate::send_game`] (out) or a system transfer signed by the owner (in);
//! the owner's `destination` is chosen by the signed owner and may never be a
//! program account; and a closing payout locks its whole reserve before any
//! cash leaves the vault.

use anchor_lang::__private::BpfWriter;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke, system_instruction};
use musical_chairs_economics::{earnings, fees};

use crate::events::{
    PayoutPaid, PositionClosed, PositionOpened, RoundClosed, SeriesAFunded, SCHEMA_VERSION,
};
use crate::state::{
    pages_for, required_close_reserve, Admission, Backer, CloseRecord, ConfigVersion, GameVault,
    Ops, ParticipantPage, Pool, Position, RegistryPage, ReserveSchedule, Ticket, User, UserRound,
    ADMISSION_SEED, BACKER_REMAINING_CUTOFF, BACKER_SEED, CLOSE_SEED, CONFIG_SEED, DISCOVER,
    GAME_VAULT_SEED, IDLE, OPS_SEED, PAGE_CAPACITY, PARTICIPANTS_SEED, POOL_SEED, POSITION_SEED,
    REGISTRY_SEED, TICKET_SEED, USER_ROUND_SEED, USER_SEED,
};
use crate::ErrorCode as Code;


/// Series identifiers are part of PDA derivation and must remain stable.
pub const SERIES_A: u8 = 0;
pub const SERIES_B: u8 = 1;

/// The three plan discriminants are part of the instruction ABI.  They are
/// deliberately separate from the host enum so Anchor serializes a stable
/// on-chain representation without importing host state.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanKindArg {
    Simple,
    Compound15,
    Compound30,
}

impl PlanKindArg {
    pub const fn discriminant(self) -> u8 {
        match self {
            Self::Simple => 0,
            Self::Compound15 => 1,
            Self::Compound30 => 2,
        }
    }

    pub const fn duration_seconds(self) -> u64 {
        match self {
            Self::Simple => musical_chairs_economics::earnings::SIMPLE_EXPIRY_SECONDS,
            Self::Compound15 => 15 * musical_chairs_economics::earnings::SECONDS_PER_DAY,
            Self::Compound30 => 30 * musical_chairs_economics::earnings::SECONDS_PER_DAY,
        }
    }

    pub const fn is_simple(self) -> bool {
        matches!(self, Self::Simple)
    }

    pub const fn is_compound(self) -> bool {
        !self.is_simple()
    }

    pub const fn compound_days(self) -> Option<u64> {
        match self {
            Self::Simple => None,
            Self::Compound15 => Some(15),
            Self::Compound30 => Some(30),
        }
    }

    pub const fn from_discriminant(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Simple),
            1 => Some(Self::Compound15),
            2 => Some(Self::Compound30),
            _ => None,
        }
    }

    pub fn host_kind(self) -> musical_chairs_economics::earnings::PlanKind {
        match self {
            Self::Simple => musical_chairs_economics::earnings::PlanKind::Simple,
            Self::Compound15 => musical_chairs_economics::earnings::PlanKind::Compound15,
            Self::Compound30 => musical_chairs_economics::earnings::PlanKind::Compound30,
        }
    }
}


/// Named client-side shapes of the instruction arguments.  The instruction
/// methods below take the same fields as direct scalars so the generated IDL
/// stays flat; these structs exist for clients that prefer one value.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenPositionArgs {
    pub plan: PlanKindArg,
    pub gross: u64,
    pub client_nonce: u64,
    pub expected_round_id: u64,
    pub expected_config: u32,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundSeriesAArgs {
    pub gross: u64,
    pub expected_round_id: u64,
}

/// Arguments of `withdraw_simple` and `settle_compounding`; `destination` is
/// an account, not an argument.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PayoutArgs {
    pub minimum_receive: u64,
    pub max_work_contribution: u64,
}

/// The source-compatible simple fee boundary.  Kept pure so runtime handlers
/// can preflight without mutating accounts or duplicating rate branches.
pub fn simple_toll_bps(elapsed_seconds: u64) -> u16 {
    musical_chairs_economics::fees::simple_toll_bps(elapsed_seconds)
}

pub const fn compound_toll_bps(plan: PlanKindArg) -> Option<u16> {
    match plan {
        PlanKindArg::Simple => None,
        PlanKindArg::Compound15 => Some(900),
        PlanKindArg::Compound30 => Some(1_300),
    }
}

/// Strict source semantics: equality with the pot remains solvent.
pub const fn starts_close(gross_owed: u128, pot: u64) -> bool {
    gross_owed > pot as u128
}

/// The open-registry cutoff is a semantic boundary, not a population cap.
pub const fn remains_open(remaining: u128) -> bool {
    remaining > BACKER_REMAINING_CUTOFF
}

/// Registry page containing the current append position.  A page must have
/// been preprovisioned before a financial instruction accepts the mutation.
pub const fn registry_page_index(registry_len: u64) -> u64 {
    registry_len / PAGE_CAPACITY as u64
}


/// User-owned open position.  Position is `init`, never `init_if_needed`, so
/// reusing an owner/nonce PDA fails at the account boundary before state or
/// custody mutation; the PDA is also the permanent nonce tombstone (DR-30).
///
/// `config` is the pool's ACTIVE version (the position pins it); the handler
/// rejects a stale `expected_config` before any change.
#[event_cpi]
#[derive(Accounts)]
#[instruction(
    plan: PlanKindArg,
    gross: u64,
    client_nonce: u64,
    expected_round_id: u64,
    expected_config: u32
)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [POOL_SEED],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        seeds = [ADMISSION_SEED],
        bump = admission.bump,
    )]
    pub admission: Account<'info, Admission>,
    #[account(
        mut,
        seeds = [GAME_VAULT_SEED],
        bump = game_vault.bump,
    )]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(
        init_if_needed,
        payer = owner,
        space = User::SPACE,
        seeds = [USER_SEED, owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,
    #[account(
        init_if_needed,
        payer = owner,
        space = UserRound::SPACE,
        seeds = [USER_ROUND_SEED, &pool.round_id.to_le_bytes(), owner.key().as_ref()],
        bump,
    )]
    pub user_round: Box<Account<'info, UserRound>>,
    #[account(
        init,
        payer = owner,
        space = Position::SPACE,
        seeds = [POSITION_SEED, owner.key().as_ref(), &client_nonce.to_le_bytes()],
        bump,
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(
        seeds = [CONFIG_SEED, &pool.active_config_version.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, ConfigVersion>>,
    /// CHECK: the current round's participant page at index
    /// `pool.participant_count / 8`.  Read and written only when this is the
    /// owner's first position of the round; otherwise any account is accepted.
    /// The handler derives the PDA, the program owner, the discriminator, the
    /// round, the index and the free capacity before it appends.
    #[account(mut)]
    pub participant_page: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Public same-owner Series-A funding.  The registry page is supplied as an
/// already provisioned shared account; creating it inside this financial
/// instruction would make acceptance depend on a later rent budget.
#[event_cpi]
#[derive(Accounts)]
#[instruction(gross: u64, expected_round_id: u64)]
pub struct FundSeriesA<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [POOL_SEED],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        mut,
        seeds = [ADMISSION_SEED],
        bump = admission.bump,
    )]
    pub admission: Account<'info, Admission>,
    #[account(
        mut,
        seeds = [GAME_VAULT_SEED],
        bump = game_vault.bump,
    )]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(
        init_if_needed,
        payer = owner,
        space = User::SPACE,
        seeds = [USER_SEED, owner.key().as_ref()],
        bump,
    )]
    pub user: Box<Account<'info, User>>,
    #[account(
        init_if_needed,
        payer = owner,
        space = Backer::SPACE,
        seeds = [BACKER_SEED, &[SERIES_A], owner.key().as_ref()],
        bump,
    )]
    pub backer: Box<Account<'info, Backer>>,
    /// CHECK: the handler validates the canonical preprovisioned tail page
    /// only when a new/reopened row appends; active same-owner top-ups need
    /// no page and ignore this account.
    #[account(mut)]
    pub registry: UncheckedAccount<'info>,
    /// The pool's active version; `public_series_a_enabled` and `min_a` are
    /// read from it.
    #[account(
        seeds = [CONFIG_SEED, &pool.active_config_version.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, ConfigVersion>>,
    pub system_program: Program<'info, System>,
}

/// Owner-authorized Simple withdrawal or compounding settlement (both
/// instructions share this account set).
///
/// `owner` is writable because it is the lamport source of any owner
/// contribution to a closing reserve.  `config` is the version the position
/// pinned at open (not the pool's active one).  `user_round` is deliberately
/// unchecked at the type level: a position from a closed round must reach
/// the handler so it can fail with `PositionRoundClosed` even when the owner
/// has no `UserRound` in the current round; the PDA is still derived and
/// verified for the CURRENT round before it is read.  `destination` is
/// unchecked so protected aliases can be rejected with the exact error.
#[event_cpi]
#[derive(Accounts)]
pub struct Payout<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [POOL_SEED],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        seeds = [ADMISSION_SEED],
        bump = admission.bump,
    )]
    pub admission: Account<'info, Admission>,
    #[account(
        mut,
        seeds = [GAME_VAULT_SEED],
        bump = game_vault.bump,
    )]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(
        mut,
        seeds = [OPS_SEED],
        bump = ops.bump,
    )]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: validated by `sync_ops` as the canonical empty system-owned
    /// `[ops-cash]` PDA before any reserve is locked or cash moves.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: the current round's `UserRound` PDA for this owner (seeds
    /// below); loaded and written by the handler only after the position is
    /// proven to belong to the current round.
    #[account(
        mut,
        seeds = [USER_ROUND_SEED, &pool.round_id.to_le_bytes(), owner.key().as_ref()],
        bump,
    )]
    pub user_round: UncheckedAccount<'info>,
    #[account(
        mut,
        has_one = owner,
        seeds = [POSITION_SEED, owner.key().as_ref(), &position.nonce.to_le_bytes()],
        bump = position.bump,
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(
        mut,
        seeds = [TICKET_SEED],
        bump = ticket.bump,
    )]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(
        seeds = [CONFIG_SEED, &position.config_version.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, ConfigVersion>>,
    /// CHECK: the handler rejects protected aliases and non-system accounts
    /// before `send_game`.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
    /// Required only when this payout closes the round: the pre-provisioned
    /// `CloseRecord(pool.next_close_id)`.
    #[account(mut)]
    pub close_record: Option<Box<Account<'info, CloseRecord>>>,
    pub system_program: Program<'info, System>,
}

/// Terminal bookkeeping for a matured Simple position with nothing left to
/// withdraw.  It moves no cash and touches no shared account, so it needs
/// only the position, the pool (round check) and the position's pinned
/// config (accrual-rate guard shared with `withdraw_simple`).
#[event_cpi]
#[derive(Accounts)]
pub struct CloseSimplePosition<'info> {
    pub owner: Signer<'info>,
    #[account(
        seeds = [POOL_SEED],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        mut,
        has_one = owner,
        seeds = [POSITION_SEED, owner.key().as_ref(), &position.nonce.to_le_bytes()],
        bump = position.bump,
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(
        seeds = [CONFIG_SEED, &position.config_version.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, ConfigVersion>>,
}

// ---------------------------------------------------------------------------
// Pure helpers: error mapping, gates, admission math
// ---------------------------------------------------------------------------

fn map_fee(e: fees::MathError) -> anchor_lang::error::Error {
    match e {
        fees::MathError::Overflow => error!(Code::Overflow),
        fees::MathError::InvalidBps | fees::MathError::InvalidCompoundDuration => {
            error!(Code::InvalidState)
        }
    }
}

fn map_earn(e: earnings::MathError) -> anchor_lang::error::Error {
    use earnings::MathError as M;
    match e {
        M::Overflow => error!(Code::Overflow),
        M::BackwardsTime => error!(Code::BackwardsTime),
        M::NotMature => error!(Code::NotMature),
        M::MinimumReceiveNotMet => error!(Code::MinimumReceive),
        M::InvalidTollBps | M::InvalidState | M::InvalidRemainder => error!(Code::InvalidState),
    }
}

fn add128(a: u128, b: u128) -> Result<u128> {
    a.checked_add(b).ok_or_else(|| error!(Code::Overflow))
}

/// The pool/admission facts every admission gate reads.
#[derive(Clone, Copy, Debug)]
struct Gate {
    paused: bool,
    closing: bool,
    active_ticket: u64,
    round_id: u64,
    config_version: u32,
}

impl Gate {
    fn of(pool: &Pool, admission: &Admission) -> Self {
        Self {
            paused: admission.paused,
            closing: pool.closing(),
            active_ticket: pool.active_ticket,
            round_id: pool.round_id,
            config_version: pool.active_config_version,
        }
    }
}

/// `open_position` preconditions, in the order they must fail (spec §7.2).
/// A nonclosing ticket is allowed here: only `fund_series_a` rejects it.
fn open_gate(
    gate: &Gate,
    expected_round_id: u64,
    expected_config: u32,
    gross: u64,
    min_game: u64,
) -> Result<()> {
    require!(!gate.paused, Code::AdmissionPaused);
    require!(!gate.closing, Code::Closing);
    require!(gate.round_id == expected_round_id, Code::StaleRound);
    require!(gate.config_version == expected_config, Code::StaleConfig);
    require!(gross >= min_game, Code::MinimumDeposit);
    Ok(())
}

/// `fund_series_a` preconditions, in the order they must fail.
fn series_a_gate(
    gate: &Gate,
    public_series_a_enabled: bool,
    expected_round_id: u64,
    gross: u64,
    min_a: u64,
) -> Result<()> {
    require!(public_series_a_enabled, Code::PublicSeriesADisabled);
    require!(!gate.paused, Code::AdmissionPaused);
    require!(!gate.closing, Code::Closing);
    require!(gate.active_ticket == 0, Code::AllocationPending);
    require!(gate.round_id == expected_round_id, Code::StaleRound);
    require!(gross >= min_a, Code::MinimumDeposit);
    Ok(())
}

/// The quota after one more success, computed on a copy so a later failure
/// in the same instruction can never have consumed a slot.  A fresh
/// `init_if_needed` User gets its identity here.
fn quota_after(user: &User, bump: u8, owner: Pubkey, now: u64) -> Result<User> {
    let mut next = user.clone();
    if next.owner == Pubkey::default() {
        next.bump = bump;
        next.owner = owner;
    }
    next.record_success(now)?;
    Ok(next)
}

/// `(front-end load, pot credit)` for a deposit; the load is floored and the
/// two always sum to `gross`.
fn entry_split(gross: u64, load_bps: u16) -> Result<(u64, u64)> {
    let load = u128::from(gross)
        .checked_mul(u128::from(load_bps))
        .ok_or_else(|| error!(Code::Overflow))?
        / fees::BPS_DENOMINATOR;
    let load = u64::try_from(load).map_err(|_| error!(Code::Overflow))?;
    let pot_credit = gross.checked_sub(load).ok_or_else(|| error!(Code::InvalidState))?;
    Ok((load, pot_credit))
}

/// Series-B width guard (spec §3.1): the entitlement a later promotion could
/// grant this user must still fit u128 after this deposit.
fn series_b_width_ok(deposits_after: u128, net_payout: u128, b_entitlement: u128) -> Result<u128> {
    fees::prospective_series_b_entitlement(deposits_after, net_payout, b_entitlement)
        .map_err(|_| error!(Code::Overflow))
}

/// `(entitlement after funding, whether this funding appends a registry
/// row)`.  A row is appended only for an inactive Backer whose remaining
/// entitlement stays above the open-registry cutoff.
fn series_a_outcome(backer: &Backer, minted: u128) -> Result<(u128, bool)> {
    let entitlement_after = add128(backer.entitlement, minted)?;
    let remaining_after = entitlement_after
        .checked_sub(backer.lifetime)
        .ok_or_else(|| error!(Code::InvalidState))?;
    Ok((entitlement_after, !backer.active && remains_open(remaining_after)))
}

// ---------------------------------------------------------------------------
// Shared-account plumbing (participant and registry pages, UserRound)
// ---------------------------------------------------------------------------

fn participant_page_key(round_id: u64, page: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[PARTICIPANTS_SEED, &round_id.to_le_bytes(), &page.to_le_bytes()],
        &crate::ID,
    )
    .0
}

fn registry_page_key(index: u64) -> Pubkey {
    Pubkey::find_program_address(&[REGISTRY_SEED, &index.to_le_bytes()], &crate::ID).0
}

fn close_record_key(close_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[CLOSE_SEED, &close_id.to_le_bytes()], &crate::ID).0
}

/// Decode a program-owned account; anything absent, foreign or of another
/// type is reported as `missing`.
fn read_account<T: AccountDeserialize>(info: &AccountInfo, missing: Code) -> Result<T> {
    if *info.owner != crate::ID || info.data_is_empty() {
        return Err(error!(missing));
    }
    let data = info.try_borrow_data()?;
    let mut bytes: &[u8] = &data;
    T::try_deserialize(&mut bytes).map_err(|_| error!(missing))
}

fn write_account<T: AccountSerialize>(info: &AccountInfo, value: &T) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    let mut writer = BpfWriter::new(&mut data[..]);
    value.try_serialize(&mut writer)
}

fn check_participant_page(page: &ParticipantPage, round_id: u64, index: u64) -> Result<()> {
    require!(page.round_id == round_id && page.page == index, Code::PageNotProvisioned);
    require!(usize::from(page.len) < PAGE_CAPACITY, Code::InvalidState);
    Ok(())
}

fn append_participant(page: &mut ParticipantPage, owner: Pubkey) -> Result<()> {
    let slot = usize::from(page.len);
    require!(slot < PAGE_CAPACITY, Code::InvalidState);
    page.rows[slot] = owner;
    page.len = page.len.checked_add(1).ok_or_else(|| error!(Code::Overflow))?;
    Ok(())
}

/// The participant page this round's next participant lands on, fully
/// validated: canonical PDA, program-owned, right round and index, room left.
fn load_participant_page(
    info: &AccountInfo,
    round_id: u64,
    participant_count: u64,
) -> Result<ParticipantPage> {
    let index = participant_count / PAGE_CAPACITY as u64;
    require_keys_eq!(info.key(), participant_page_key(round_id, index), Code::WrongAccount);
    let page: ParticipantPage = read_account(info, Code::PageNotProvisioned)?;
    check_participant_page(&page, round_id, index)?;
    Ok(page)
}

fn check_registry_page(page: &RegistryPage, index: u64) -> Result<()> {
    require!(page.index == index, Code::PageNotProvisioned);
    Ok(())
}

fn append_registry(page: &mut RegistryPage, slot: u64, key: Pubkey) -> Result<()> {
    let row = usize::try_from(slot % PAGE_CAPACITY as u64).map_err(|_| error!(Code::Overflow))?;
    page.keys[row] = key;
    Ok(())
}

/// The registry tail page the next appended row lands on, fully validated.
fn load_registry_tail(info: &AccountInfo, registry_len: u64) -> Result<RegistryPage> {
    let index = registry_page_index(registry_len);
    require_keys_eq!(info.key(), registry_page_key(index), Code::WrongAccount);
    let page: RegistryPage = read_account(info, Code::PageNotProvisioned)?;
    check_registry_page(&page, index)?;
    Ok(page)
}

fn transfer_from_owner<'info>(
    owner: &Signer<'info>,
    to: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    lamports: u64,
) -> Result<()> {
    invoke(
        &system_instruction::transfer(&owner.key(), to.key, lamports),
        &[owner.to_account_info(), to.clone(), system_program.to_account_info()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// open_position
// ---------------------------------------------------------------------------

/// Open a position: take `gross` from the owner into the game vault, skim
/// the Front-End Load into `management`, credit the rest to the pot and pin
/// the active config's terms on a new permanent `Position`.
pub fn open_position(
    ctx: Context<OpenPosition>,
    plan: PlanKindArg,
    gross: u64,
    client_nonce: u64,
    expected_round_id: u64,
    expected_config: u32,
) -> Result<()> {
    let now = crate::now()?;
    let owner_key = ctx.accounts.owner.key();
    let (user_bump, user_round_bump, position_bump) =
        (ctx.bumps.user, ctx.bumps.user_round, ctx.bumps.position);
    let kind = plan.discriminant();
    let (round_id, config_version, load) = {
        let a = &mut *ctx.accounts;

        // Every check, against read-only views, before any change.
        open_gate(
            &Gate::of(&a.pool, &a.admission),
            expected_round_id,
            expected_config,
            gross,
            a.config.min_game,
        )?;
        let quota = quota_after(&a.user, user_bump, owner_key, now)?;
        let deposits_after = add128(a.user_round.deposits, u128::from(gross))?;
        series_b_width_ok(deposits_after, a.user_round.net_payout, quota.b_entitlement)?;
        // A fresh `init_if_needed` UserRound has no deposits; every opened
        // position adds at least `min_game`, so zero means "first this round".
        let first_in_round = a.user_round.deposits == 0;
        let participant_page = if first_in_round {
            Some(load_participant_page(
                &a.participant_page.to_account_info(),
                a.pool.round_id,
                a.pool.participant_count,
            )?)
        } else {
            None
        };
        let (load, pot_credit) = entry_split(gross, a.config.front_end_load_bps)?;
        let duration = u64::from(a.config.plan_durations_s[usize::from(kind)]);
        let maturity_at = now.checked_add(duration).ok_or_else(|| error!(Code::Overflow))?;
        let management = crate::checked_add(a.pool.management, load)?;
        let pot = crate::checked_add(a.pool.pot, pot_credit)?;
        let total_gross = add128(a.pool.total_gross_deposits, u128::from(gross))?;
        let participant_count = if first_in_round {
            crate::checked_add(a.pool.participant_count, 1)?
        } else {
            a.pool.participant_count
        };

        // Effects.
        crate::sync_game(&mut a.pool, &a.game_vault.to_account_info())?;
        transfer_from_owner(&a.owner, &a.game_vault.to_account_info(), &a.system_program, gross)?;
        a.pool.management = management;
        a.pool.pot = pot;
        a.pool.total_gross_deposits = total_gross;

        let pinned_version = a.pool.active_config_version;
        let round_id = a.pool.round_id;
        let position: &mut Position = &mut a.position;
        *position = Position {
            bump: position_bump,
            owner: owner_key,
            nonce: client_nonce,
            round_id,
            config_version: pinned_version,
            kind,
            principal: gross,
            opened_at: now,
            maturity_at,
            last_accrued_at: now,
            remainder: 0,
            accrued_gross: 0,
            paid_gross: 0,
            net_payout: 0,
            claimed: false,
        };

        let user_round: &mut UserRound = &mut a.user_round;
        if user_round.owner == Pubkey::default() {
            user_round.bump = user_round_bump;
            user_round.round_id = round_id;
            user_round.owner = owner_key;
        }
        user_round.deposits = deposits_after;
        if let Some(mut page) = participant_page {
            append_participant(&mut page, owner_key)?;
            write_account(&a.participant_page.to_account_info(), &page)?;
            user_round.participant_slot = a.pool.participant_count;
            a.pool.participant_count = participant_count;
        }

        let user: &mut User = &mut a.user;
        *user = quota;
        (round_id, pinned_version, load)
    };
    emit_cpi!(PositionOpened {
        schema_version: SCHEMA_VERSION,
        owner: owner_key,
        nonce: client_nonce,
        round_id,
        close_id: None,
        plan: kind,
        gross,
        load,
        config_version,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// fund_series_a
// ---------------------------------------------------------------------------

/// Fund Series A: the full gross goes to the pot (no Front-End Load), the
/// Backer's entitlement grows by `floor(gross * 124 / 100)`, and a new or
/// reopened Backer is appended to the dense registry.
pub fn fund_series_a(ctx: Context<FundSeriesA>, gross: u64, expected_round_id: u64) -> Result<()> {
    let now = crate::now()?;
    let owner_key = ctx.accounts.owner.key();
    let (user_bump, backer_bump) = (ctx.bumps.user, ctx.bumps.backer);
    let (round_id, entitlement_after) = {
        let a = &mut *ctx.accounts;

        series_a_gate(
            &Gate::of(&a.pool, &a.admission),
            a.config.public_series_a_enabled,
            expected_round_id,
            gross,
            a.config.min_a,
        )?;
        let mut quota = quota_after(&a.user, user_bump, owner_key, now)?;
        quota.ever_a = true;
        let minted = fees::series_a_entitlement(gross).map_err(map_fee)?;
        let (entitlement_after, appends) = series_a_outcome(&a.backer, minted)?;
        let amount_after = add128(a.backer.amount, u128::from(gross))?;
        let registry_page = if appends {
            Some(load_registry_tail(&a.registry.to_account_info(), a.pool.registry_len)?)
        } else {
            None
        };
        let pot = crate::checked_add(a.pool.pot, gross)?;
        let next_sequence = if a.backer.first_sequence == 0 {
            crate::checked_add(a.pool.next_sequence, 1)?
        } else {
            a.pool.next_sequence
        };
        let registry_len = if appends {
            crate::checked_add(a.pool.registry_len, 1)?
        } else {
            a.pool.registry_len
        };

        crate::sync_game(&mut a.pool, &a.game_vault.to_account_info())?;
        transfer_from_owner(&a.owner, &a.game_vault.to_account_info(), &a.system_program, gross)?;
        a.pool.pot = pot;

        let backer_key = a.backer.key();
        let slot = a.pool.registry_len;
        let backer: &mut Backer = &mut a.backer;
        if backer.owner == Pubkey::default() {
            backer.bump = backer_bump;
            backer.owner = owner_key;
            backer.series = SERIES_A;
        }
        backer.amount = amount_after;
        backer.entitlement = entitlement_after;
        if backer.first_sequence == 0 {
            backer.first_sequence = a.pool.next_sequence;
            a.pool.next_sequence = next_sequence;
        }
        if let Some(mut page) = registry_page {
            append_registry(&mut page, slot, backer_key)?;
            write_account(&a.registry.to_account_info(), &page)?;
            backer.active = true;
            backer.slot = slot;
            a.pool.registry_len = registry_len;
        }

        let user: &mut User = &mut a.user;
        *user = quota;
        (a.pool.round_id, entitlement_after)
    };
    emit_cpi!(SeriesAFunded {
        schema_version: SCHEMA_VERSION,
        owner: owner_key,
        round_id,
        close_id: None,
        gross,
        entitlement_after,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Payout pipeline (withdraw_simple, settle_compounding, close_simple_position)
// ---------------------------------------------------------------------------

/// The Simple accrual rate the economics crate hard-codes (11 %/day).  A
/// pinned config that disagrees would silently mis-pay, so it is rejected.
const SIMPLE_RATE_BPS_PER_DAY: u16 = 1_100;

fn plan_of(position: &Position) -> Result<PlanKindArg> {
    PlanKindArg::from_discriminant(position.kind).ok_or_else(|| error!(Code::InvalidState))
}

/// Gate shared by the two payout instructions, in failure order.  A
/// position from a closed round is terminal and fails before anything else
/// is read or written (spec §7.2).
fn payout_gate(position: &Position, pool: &Pool) -> Result<()> {
    require!(position.round_id == pool.round_id, Code::PositionRoundClosed);
    require!(!position.claimed, Code::AlreadyClaimed);
    require!(!pool.closing(), Code::Closing);
    require!(pool.active_ticket == 0, Code::AllocationPending);
    Ok(())
}

/// Simple accrual brought up to `now` on a copy, and what is claimable:
/// `accrued_gross' - paid_gross`.  Unpaid earnings always carry forward.
fn simple_owed(
    position: &Position,
    now: u64,
    config: &ConfigVersion,
) -> Result<(earnings::SimpleAccrual, u128)> {
    require!(
        config.plan_rates_bps_per_day[0] == SIMPLE_RATE_BPS_PER_DAY,
        Code::InvalidState
    );
    require!(now >= position.last_accrued_at, Code::BackwardsTime);
    let mut accrual = earnings::SimpleAccrual {
        principal: position.principal,
        last_accrued_at: position.last_accrued_at,
        expires_at: position.maturity_at,
        remainder: position.remainder,
        accrued_gross: position.accrued_gross,
    };
    accrual.accrue(now).map_err(map_earn)?;
    let owed = accrual
        .accrued_gross
        .checked_sub(position.paid_gross)
        .ok_or_else(|| error!(Code::InvalidState))?;
    Ok((accrual, owed))
}

/// Carried Interest bps for `elapsed` seconds since open.  The economics
/// crate owns the tier boundaries; the pinned config must carry the same
/// table or the payout is refused rather than mispriced.
fn simple_toll(config: &ConfigVersion, elapsed: u64) -> Result<u16> {
    let host = fees::simple_toll_bps(elapsed);
    let [early, mid] = config.simple_toll_day_bounds;
    let tier = if elapsed < u64::from(early) * fees::DAY {
        config.simple_toll_bps[0]
    } else if elapsed < u64::from(mid) * fees::DAY {
        config.simple_toll_bps[1]
    } else {
        config.simple_toll_bps[2]
    };
    require!(tier == host, Code::InvalidState);
    Ok(host)
}

/// Index into the config's two-entry compounding tables.
fn compound_index(position: &Position) -> Result<usize> {
    match PlanKindArg::from_discriminant(position.kind) {
        Some(PlanKindArg::Compound15) => Ok(0),
        Some(PlanKindArg::Compound30) => Ok(1),
        _ => Err(error!(Code::InvalidState)),
    }
}

/// `floor(principal * factor_q18 / 10^18)`.  A u64 times a u64 always fits
/// u128, so the product cannot overflow; the checks are belt and braces.
fn compound_owed(position: &Position, config: &ConfigVersion) -> Result<u128> {
    let factor = config.compound_factors_q18[compound_index(position)?];
    u128::from(position.principal)
        .checked_mul(u128::from(factor))
        .ok_or_else(|| error!(Code::Overflow))?
        .checked_div(earnings::Q18)
        .ok_or_else(|| error!(Code::InvalidState))
}

/// What a position owes right now, before the pot caps it.
#[derive(Clone, Copy, Debug)]
struct Settlement {
    /// Present for Simple: the accrual to write back.
    accrual: Option<earnings::SimpleAccrual>,
    owed: u128,
    toll_bps: u16,
}

fn simple_settlement(position: &Position, config: &ConfigVersion, now: u64) -> Result<Settlement> {
    let (accrual, owed) = simple_owed(position, now, config)?;
    let elapsed = now
        .checked_sub(position.opened_at)
        .ok_or_else(|| error!(Code::BackwardsTime))?;
    Ok(Settlement { accrual: Some(accrual), owed, toll_bps: simple_toll(config, elapsed)? })
}

fn compound_settlement(position: &Position, config: &ConfigVersion, now: u64) -> Result<Settlement> {
    require!(now >= position.maturity_at, Code::NotMature);
    let owed = compound_owed(position, config)?;
    let toll_bps = config.jackpot_bps[compound_index(position)?];
    Ok(Settlement { accrual: None, owed, toll_bps })
}

/// Cap `owed` at the pot, price the toll and enforce the owner's floor.
/// `insolvent` is strict: `owed == pot` pays in full and stays solvent.
fn settle_fee(owed: u128, pot: u64, toll_bps: u16, minimum_receive: u64) -> Result<fees::FeePayout> {
    let fee = fees::payout(owed, pot, toll_bps).map_err(map_fee)?;
    require!(fee.net >= minimum_receive, Code::MinimumReceive);
    Ok(fee)
}

/// Write the settlement into the Position.  Simple keeps its accrual and
/// terminates only at maturity; a compounding settlement is one-shot.
fn record_settlement(
    position: &mut Position,
    settlement: &Settlement,
    fee: &fees::FeePayout,
    now: u64,
) -> Result<()> {
    let paid = u128::from(fee.paid_gross);
    let net = u128::from(fee.net);
    match &settlement.accrual {
        Some(accrual) => {
            position.accrued_gross = accrual.accrued_gross;
            position.remainder = accrual.remainder;
            position.last_accrued_at = accrual.last_accrued_at;
            position.paid_gross = add128(position.paid_gross, paid)?;
            position.net_payout = add128(position.net_payout, net)?;
            position.claimed = now >= position.maturity_at;
        }
        None => {
            position.accrued_gross = settlement.owed;
            position.paid_gross = paid;
            position.net_payout = net;
            position.claimed = true;
        }
    }
    Ok(())
}

/// Where a payout's toll goes: nowhere, straight to seed (no Backers to
/// share it), or into escrow behind an allocation ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TollRoute {
    None,
    Seed,
    Ticket,
}

fn toll_route(toll: u64, registry_len: u64) -> TollRoute {
    if toll == 0 {
        TollRoute::None
    } else if registry_len == 0 {
        TollRoute::Seed
    } else {
        TollRoute::Ticket
    }
}

/// A ticket walks the registry through snapshot pages, so every captured
/// row needs a provisioned snapshot slot before the toll is accepted.
fn snapshots_cover(snapshot_pages: u64, registry_len: u64) -> Result<()> {
    let capacity = snapshot_pages
        .checked_mul(PAGE_CAPACITY as u64)
        .ok_or_else(|| error!(Code::Overflow))?;
    require!(capacity >= registry_len, Code::PageNotProvisioned);
    Ok(())
}

/// The allocation ticket for a freshly escrowed toll: pinned split bps from
/// the position's config, captured registry extent and version, and every
/// cursor, count, slice, accumulator and credit at zero.
fn ticket_start(
    bump: u8,
    id: u64,
    close_id: Option<u64>,
    registry_len: u64,
    registry_version: u64,
    toll: u64,
    config: &ConfigVersion,
) -> Ticket {
    Ticket {
        bump,
        id,
        phase: DISCOVER,
        close_id,
        captured_len: registry_len,
        expected_version: registry_version,
        cursor: 0,
        a_count: 0,
        b_count: 0,
        a_remainder_total: [0; 32],
        sealed_a_denominator: [0; 32],
        oldest: Pubkey::default(),
        oldest_sequence: 0,
        oldest_slot: 0,
        senior_credit: 0,
        senior_slice: 0,
        a_slice: 0,
        b_slice: 0,
        seed_credit: 0,
        credited: 0,
        toll,
        senior_bps: config.senior_bps,
        a_bps: config.a_bps,
        b_bps: config.b_bps,
    }
}

/// Split a close reserve between sponsor cash and the owner's opt-in
/// contribution (spec §8.2): sponsor first, owner up to its cap, and the
/// payout fails if the two together cannot cover the whole requirement.
fn lock_split(required: u64, free_balance: u64, max_contribution: u64) -> Result<(u64, u64)> {
    let sponsor = required.min(free_balance);
    let owner = (required - sponsor).min(max_contribution);
    require!(sponsor + owner == required, Code::ReserveUnavailable);
    Ok((sponsor, owner))
}

/// A close reserve ready to lock: the captured schedule, the requirement it
/// implies for the current participants and registry tail, and the split.
#[derive(Clone, Copy, Debug)]
struct CloseLock {
    required: u64,
    sponsor: u64,
    owner: u64,
    schedule: ReserveSchedule,
}

fn close_lock(
    rent: &Rent,
    config: &ConfigVersion,
    record: &CloseRecord,
    pool: &Pool,
    free_balance: u64,
    max_contribution: u64,
) -> Result<CloseLock> {
    let schedule = ReserveSchedule::capture(rent, config);
    let required = required_close_reserve(
        pool.participant_count,
        pool.registry_len,
        record.existing_preferred_pages,
        record.existing_fallback_pages,
        &schedule,
    )?;
    let (sponsor, owner) = lock_split(required, free_balance, max_contribution)?;
    Ok(CloseLock { required, sponsor, owner, schedule })
}

/// The provider fee plus its floored headroom: the initial cap the draw
/// request may spend.
fn provider_fee_cap(schedule: &ReserveSchedule) -> Result<u64> {
    let headroom = u128::from(schedule.provider_fee)
        .checked_mul(u128::from(schedule.headroom_bps))
        .ok_or_else(|| error!(Code::Overflow))?
        / fees::BPS_DENOMINATOR;
    let headroom = u64::try_from(headroom).map_err(|_| error!(Code::Overflow))?;
    schedule.provider_fee.checked_add(headroom).ok_or_else(|| error!(Code::Overflow))
}

/// Everything a closing payout pins onto its pre-provisioned `CloseRecord`.
struct CloseBinding<'a> {
    round_id: u64,
    config_version: u32,
    close_started_at: u64,
    deadline_seconds: u32,
    closing_ticket_id: Option<u64>,
    participant_count: u64,
    registry_len: u64,
    lock: &'a CloseLock,
    destination: Pubkey,
}

/// Bind the close: deadline pinned once, participant count and reserve
/// captured, cash marked committed when no ticket allocates the toll.
/// Identity, rent and provisioning counters of the record are left alone.
fn bind_close_record(record: &mut CloseRecord, binding: &CloseBinding) -> Result<()> {
    let deadline = binding
        .close_started_at
        .checked_add(u64::from(binding.deadline_seconds))
        .ok_or_else(|| error!(Code::Overflow))?;
    record.round_id = binding.round_id;
    record.config_version = binding.config_version;
    record.close_started_at = binding.close_started_at;
    record.promotion_deadline = deadline;
    record.closing_ticket_id = binding.closing_ticket_id;
    record.cash_committed = binding.closing_ticket_id.is_none();
    record.participant_count = binding.participant_count;
    record.required_reserve = binding.lock.required;
    record.reserve_schedule = binding.lock.schedule;
    record.required_pages_per_set = pages_for(binding.participant_count, PAGE_CAPACITY as u64)?;
    record.registry_extension_required = binding.registry_len % PAGE_CAPACITY as u64 == 0;
    record.locked_sponsor_remaining = binding.lock.sponsor;
    record.locked_owner_remaining = binding.lock.owner;
    record.approved_provider_fee_cap = provider_fee_cap(&binding.lock.schedule)?;
    record.owner_refund_destination = binding.destination;
    Ok(())
}

/// Accounts a payout may never pay into.
fn is_protected(destination: &Pubkey, protected: &[Pubkey]) -> bool {
    protected.iter().any(|key| key == destination)
}

/// Everything decided before the first lamport moves.
struct PayoutPlan {
    fee: fees::FeePayout,
    route: TollRoute,
    user_round: UserRound,
    close: Option<ClosePlan>,
}

struct ClosePlan {
    close_id: u64,
    next_close_id: u64,
    lock: CloseLock,
}

/// What the instruction reports once the payout has been applied.
struct PayoutDone {
    fee: fees::FeePayout,
    close_id: Option<u64>,
    ticket_id: u64,
    deadline: u64,
}

/// All payout checks, read-only: fee and floor, destination, snapshot
/// coverage, UserRound, and (for a closing payout) the full reserve lock.
fn payout_preflight(
    a: &Payout<'_>,
    settlement: &Settlement,
    minimum_receive: u64,
    max_work_contribution: u64,
) -> Result<PayoutPlan> {
    let fee = settle_fee(settlement.owed, a.pool.pot, settlement.toll_bps, minimum_receive)?;

    let destination = a.destination.to_account_info();
    crate::validate_destination(&destination)?;
    let protected = [
        a.game_vault.key(),
        a.ops_cash.key(),
        a.pool.key(),
        a.admission.key(),
        a.ops.key(),
        a.ticket.key(),
        a.position.key(),
        a.user_round.key(),
        a.config.key(),
    ];
    require!(!is_protected(&destination.key(), &protected), Code::ProtectedDestination);
    if let Some(record) = a.close_record.as_ref() {
        require!(record.key() != destination.key(), Code::ProtectedDestination);
    }

    let route = toll_route(fee.toll, a.pool.registry_len);
    if route == TollRoute::Ticket {
        snapshots_cover(a.pool.snapshot_pages, a.pool.registry_len)?;
        require!(a.ticket.phase == IDLE, Code::InvalidState);
        crate::checked_add(a.pool.next_ticket, 1)?;
        crate::checked_add(a.pool.pending_toll, fee.toll)?;
    }

    let user_round: UserRound = read_account(&a.user_round.to_account_info(), Code::WrongAccount)?;
    require!(
        user_round.owner == a.owner.key() && user_round.round_id == a.pool.round_id,
        Code::WrongAccount
    );

    let close = if fee.insolvent {
        let record = a
            .close_record
            .as_ref()
            .ok_or_else(|| error!(Code::PageNotProvisioned))?;
        let close_id = a.pool.next_close_id;
        require!(record.close_id == close_id, Code::PageNotProvisioned);
        require_keys_eq!(record.key(), close_record_key(close_id), Code::PageNotProvisioned);
        let next_close_id = close_id.checked_add(1).ok_or_else(|| error!(Code::Overflow))?;
        let lock = close_lock(
            &Rent::get()?,
            &a.config,
            record,
            &a.pool,
            a.ops.free_balance,
            max_work_contribution,
        )?;
        Some(ClosePlan { close_id, next_close_id, lock })
    } else {
        None
    };
    Ok(PayoutPlan { fee, route, user_round, close })
}

/// Apply a preflighted payout.  Order matters: books are synced first so the
/// owner's contribution is not read as unsolicited cash, the contribution
/// and the vault payout move next, and the bookkeeping follows.
fn payout_apply(
    a: &mut Payout<'_>,
    settlement: &Settlement,
    plan: PayoutPlan,
    now: u64,
) -> Result<PayoutDone> {
    let PayoutPlan { fee, route, mut user_round, close } = plan;
    let ops_cash = a.ops_cash.to_account_info();
    let destination = a.destination.to_account_info();

    crate::sync_game(&mut a.pool, &a.game_vault.to_account_info())?;
    crate::sync_ops(&mut a.ops, &ops_cash)?;
    if let Some(close) = &close {
        if close.lock.owner > 0 {
            transfer_from_owner(&a.owner, &ops_cash, &a.system_program, close.lock.owner)?;
        }
    }
    crate::send_game(&a.game_vault.to_account_info(), &destination, fee.net)?;

    a.pool.pot = crate::checked_sub(a.pool.pot, fee.paid_gross)?;
    let close_id = close.as_ref().map(|c| c.close_id);
    let mut ticket_id = 0;
    match route {
        TollRoute::None => {}
        TollRoute::Seed => a.pool.seed = crate::checked_add(a.pool.seed, fee.toll)?,
        TollRoute::Ticket => {
            ticket_id = a.pool.next_ticket;
            a.pool.next_ticket = crate::checked_add(ticket_id, 1)?;
            a.pool.active_ticket = ticket_id;
            a.pool.pending_toll = crate::checked_add(a.pool.pending_toll, fee.toll)?;
            let ticket: &mut Ticket = &mut a.ticket;
            *ticket = ticket_start(
                ticket.bump,
                ticket_id,
                close_id,
                a.pool.registry_len,
                a.pool.registry_version,
                fee.toll,
                &a.config,
            );
        }
    }

    record_settlement(&mut a.position, settlement, &fee, now)?;
    user_round.net_payout = add128(user_round.net_payout, u128::from(fee.net))?;
    write_account(&a.user_round.to_account_info(), &user_round)?;
    a.pool.total_net_payouts = add128(a.pool.total_net_payouts, u128::from(fee.net))?;

    let mut deadline = 0;
    if let Some(close) = &close {
        a.pool.current_close_id = Some(close.close_id);
        a.pool.next_close_id = close.next_close_id;
        let binding = CloseBinding {
            round_id: a.pool.round_id,
            config_version: a.position.config_version,
            close_started_at: now,
            deadline_seconds: a.config.deadline_seconds,
            closing_ticket_id: if route == TollRoute::Ticket { Some(ticket_id) } else { None },
            participant_count: a.pool.participant_count,
            registry_len: a.pool.registry_len,
            lock: &close.lock,
            destination: destination.key(),
        };
        let record = a
            .close_record
            .as_mut()
            .ok_or_else(|| error!(Code::PageNotProvisioned))?;
        bind_close_record(record, &binding)?;
        deadline = record.promotion_deadline;
        a.ops.free_balance = crate::checked_sub(a.ops.free_balance, close.lock.sponsor)?;
        a.ops.locked_remaining_total =
            crate::checked_add(a.ops.locked_remaining_total, close.lock.required)?;
    }
    Ok(PayoutDone { fee, close_id, ticket_id, deadline })
}

/// Run a settled payout and emit its events.
fn finish_payout(
    ctx: Context<Payout>,
    now: u64,
    settlement: Settlement,
    minimum_receive: u64,
    max_work_contribution: u64,
) -> Result<()> {
    let plan = payout_preflight(ctx.accounts, &settlement, minimum_receive, max_work_contribution)?;
    let done = payout_apply(ctx.accounts, &settlement, plan, now)?;
    let owner = ctx.accounts.owner.key();
    let nonce = ctx.accounts.position.nonce;
    let round_id = ctx.accounts.pool.round_id;
    let destination = ctx.accounts.destination.key();
    emit_cpi!(PayoutPaid {
        schema_version: SCHEMA_VERSION,
        owner,
        nonce,
        round_id,
        gross_paid: done.fee.paid_gross,
        toll: done.fee.toll,
        net: done.fee.net,
        destination,
        closing: done.close_id.is_some(),
        close_id: done.close_id,
        ticket_id: done.ticket_id,
    });
    if let Some(close_id) = done.close_id {
        emit_cpi!(RoundClosed {
            schema_version: SCHEMA_VERSION,
            round_id,
            close_id,
            deadline: done.deadline,
        });
    }
    Ok(())
}

/// Withdraw what a Simple position has accrued and not yet been paid.
/// May repeat; terminates at maturity.
pub fn withdraw_simple(
    ctx: Context<Payout>,
    minimum_receive: u64,
    max_work_contribution: u64,
) -> Result<()> {
    let now = crate::now()?;
    let settlement = {
        let a = &ctx.accounts;
        require!(plan_of(&a.position)? == PlanKindArg::Simple, Code::InvalidState);
        payout_gate(&a.position, &a.pool)?;
        simple_settlement(&a.position, &a.config, now)?
    };
    finish_payout(ctx, now, settlement, minimum_receive, max_work_contribution)
}

/// Settle a matured Compound15/Compound30 position in one payment.
pub fn settle_compounding(
    ctx: Context<Payout>,
    minimum_receive: u64,
    max_work_contribution: u64,
) -> Result<()> {
    let now = crate::now()?;
    let settlement = {
        let a = &ctx.accounts;
        require!(plan_of(&a.position)?.is_compound(), Code::InvalidState);
        payout_gate(&a.position, &a.pool)?;
        compound_settlement(&a.position, &a.config, now)?
    };
    finish_payout(ctx, now, settlement, minimum_receive, max_work_contribution)
}

/// `close_simple_position` preconditions: a matured, open, current-round
/// Simple position with nothing left to withdraw.
fn close_simple_gate(
    position: &Position,
    pool_round_id: u64,
    now: u64,
    config: &ConfigVersion,
) -> Result<()> {
    require!(position.round_id == pool_round_id, Code::PositionRoundClosed);
    require!(!position.claimed, Code::AlreadyClaimed);
    require!(now >= position.maturity_at, Code::NotMature);
    let (_, owed) = simple_owed(position, now, config)?;
    // Closing with earnings outstanding would forfeit them: use withdraw_simple.
    require!(owed == 0, Code::InvalidState);
    Ok(())
}

/// Terminate a matured Simple position that has nothing left to withdraw.
/// Moves no cash and never closes a round.
pub fn close_simple_position(ctx: Context<CloseSimplePosition>) -> Result<()> {
    let now = crate::now()?;
    let (nonce, round_id) = {
        let a = &mut *ctx.accounts;
        require!(plan_of(&a.position)? == PlanKindArg::Simple, Code::InvalidState);
        close_simple_gate(&a.position, a.pool.round_id, now, &a.config)?;
        a.position.claimed = true;
        (a.position.nonce, a.position.round_id)
    };
    let owner = ctx.accounts.owner.key();
    emit_cpi!(PositionClosed { schema_version: SCHEMA_VERSION, owner, nonce, round_id });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn plan_discriminants_and_durations_match_host_contract() {
        assert_eq!(PlanKindArg::Simple.discriminant(), 0);
        assert_eq!(PlanKindArg::Compound15.discriminant(), 1);
        assert_eq!(PlanKindArg::Compound30.discriminant(), 2);
        assert_eq!(
            PlanKindArg::Simple.duration_seconds(),
            musical_chairs_economics::earnings::SIMPLE_EXPIRY_SECONDS
        );
        assert_eq!(
            PlanKindArg::Compound15.duration_seconds(),
            15 * musical_chairs_economics::earnings::SECONDS_PER_DAY
        );
        assert_eq!(
            PlanKindArg::Compound30.duration_seconds(),
            30 * musical_chairs_economics::earnings::SECONDS_PER_DAY
        );
    }

    #[test]
    fn fee_boundaries_and_strict_close_match_host_contract() {
        assert_eq!(simple_toll_bps(7 * 86_400 - 1), 1_200);
        assert_eq!(simple_toll_bps(7 * 86_400), 750);
        assert_eq!(simple_toll_bps(14 * 86_400 - 1), 750);
        assert_eq!(simple_toll_bps(14 * 86_400), 300);
        assert_eq!(compound_toll_bps(PlanKindArg::Compound15), Some(900));
        assert_eq!(compound_toll_bps(PlanKindArg::Compound30), Some(1_300));
        assert_eq!(compound_toll_bps(PlanKindArg::Simple), None);
        assert!(!starts_close(10, 10));
        assert!(starts_close(11, 10));
    }

    #[test]
    fn registry_cutoff_and_page_mapping_are_fixed() {
        assert!(!remains_open(10));
        assert!(remains_open(11));
        assert_eq!(registry_page_index(0), 0);
        assert_eq!(registry_page_index(7), 0);
        assert_eq!(registry_page_index(8), 1);
        assert_eq!(registry_page_index(16), 2);
    }

    // ----- Milestone 5: admission, Series A and payouts (pure rules) -----

    use crate::state::{CandidatePage, RentComponents};
    use anchor_lang::solana_program::system_program;

    const DAY: u64 = 86_400;

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn code_of<T>(result: Result<T>) -> u32 {
        match result {
            Ok(_) => panic!("expected an error, got Ok"),
            Err(anchor_lang::error::Error::AnchorError(error)) => error.error_code_number,
            Err(other) => panic!("expected an Anchor error, got {other:?}"),
        }
    }

    fn assert_code<T>(result: Result<T>, expected: Code) {
        assert_eq!(code_of(result), expected as u32 + 6000, "{expected:?}");
    }

    fn config() -> ConfigVersion {
        ConfigVersion::default_v1()
    }

    fn simple_position(principal: u64, opened_at: u64) -> Position {
        Position {
            bump: 255,
            owner: key(1),
            nonce: 1,
            round_id: 1,
            config_version: 1,
            kind: PlanKindArg::Simple.discriminant(),
            principal,
            opened_at,
            maturity_at: opened_at + 21 * DAY,
            last_accrued_at: opened_at,
            ..Position::default()
        }
    }

    fn compound_position(plan: PlanKindArg, principal: u64, opened_at: u64) -> Position {
        Position {
            kind: plan.discriminant(),
            maturity_at: opened_at + plan.duration_seconds(),
            ..simple_position(principal, opened_at)
        }
    }

    fn open_gate_view() -> Gate {
        Gate { paused: false, closing: false, active_ticket: 0, round_id: 1, config_version: 1 }
    }

    /// An account backed by leaked memory, as the control-plane tests do.
    fn info(key: Pubkey, owner: Pubkey, data: Vec<u8>) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(key));
        let owner = Box::leak(Box::new(owner));
        let lamports = Box::leak(Box::new(1_000_000u64));
        let data = Box::leak(data.into_boxed_slice());
        AccountInfo::new(key, false, true, lamports, data, owner, false, 0)
    }

    fn serialized<T: AccountSerialize>(value: &T) -> Vec<u8> {
        let mut bytes = Vec::new();
        value.try_serialize(&mut bytes).expect("account serializes");
        bytes
    }

    fn participant_page(round_id: u64, page: u64, len: u8) -> ParticipantPage {
        ParticipantPage {
            bump: 1,
            round_id,
            page,
            len,
            rows: [Pubkey::default(); PAGE_CAPACITY],
            rent: RentComponents::default(),
        }
    }

    // --- Simple and compounding settlement vectors ---

    #[test]
    fn simple_one_sol_after_one_day_owes_110m_and_pays_96_8m_net_of_the_12_percent_toll() {
        let position = simple_position(1_000_000_000, 1_000);
        let settlement = simple_settlement(&position, &config(), 1_000 + DAY).unwrap();
        assert_eq!((settlement.owed, settlement.toll_bps), (110_000_000, 1_200));
        let fee = settle_fee(settlement.owed, 960_000_000, settlement.toll_bps, 96_800_000).unwrap();
        assert_eq!(
            (fee.paid_gross, fee.toll, fee.net, fee.insolvent),
            (110_000_000, 13_200_000, 96_800_000, false)
        );
        assert_code(
            settle_fee(settlement.owed, 960_000_000, settlement.toll_bps, 96_800_001),
            Code::MinimumReceive,
        );
    }

    #[test]
    fn owed_above_the_pot_narrows_to_the_pot_and_is_insolvent() {
        let position = simple_position(100_000_000_000, 0);
        let settlement = simple_settlement(&position, &config(), DAY).unwrap();
        assert_eq!(settlement.owed, 11_000_000_000);
        let fee = settle_fee(settlement.owed, 5_000_000_000, settlement.toll_bps, 0).unwrap();
        assert_eq!(
            (fee.paid_gross, fee.toll, fee.net, fee.insolvent),
            (5_000_000_000, 600_000_000, 4_400_000_000, true)
        );
    }

    #[test]
    fn owed_beyond_u64_narrows_to_the_pot_at_the_u64_boundary() {
        let position = simple_position(u64::MAX, 0);
        let settlement = simple_settlement(&position, &config(), 21 * DAY).unwrap();
        let expected = u128::from(u64::MAX) * 11 * u128::from(21 * DAY) / 8_640_000;
        assert_eq!(settlement.owed, expected);
        assert!(settlement.owed > u128::from(u64::MAX));
        assert_eq!(settlement.toll_bps, 300);
        let fee = settle_fee(settlement.owed, u64::MAX, settlement.toll_bps, 0).unwrap();
        assert_eq!(fee.paid_gross, u64::MAX);
        assert!(fee.insolvent);
        assert_eq!(fee.toll + fee.net, fee.paid_gross);
    }

    #[test]
    fn insolvency_is_strict_equality_pays_in_full_and_stays_solvent() {
        let equal = settle_fee(110, 110, 0, 0).unwrap();
        assert_eq!((equal.paid_gross, equal.insolvent), (110, false));
        let short = settle_fee(111, 110, 0, 0).unwrap();
        assert_eq!((short.paid_gross, short.insolvent), (110, true));
        assert!(!starts_close(110, 110));
        assert!(starts_close(111, 110));
        // An empty pot with earnings owed still closes; nothing is paid.
        let broke = settle_fee(1, 0, 1_200, 0).unwrap();
        assert_eq!((broke.paid_gross, broke.toll, broke.net, broke.insolvent), (0, 0, 0, true));
    }

    #[test]
    fn repeated_simple_claims_carry_remainder_and_unpaid_earnings_forward() {
        let config = config();
        let mut position = simple_position(1_000_000_001, 500);

        // A pot that can pay only part of what is owed.
        let first_at = 500 + 1_000;
        let first = simple_settlement(&position, &config, first_at).unwrap();
        let first_fee = settle_fee(first.owed, 1_000_000, first.toll_bps, 0).unwrap();
        assert_eq!(first.owed, 1_273_148);
        assert_eq!(first_fee.paid_gross, 1_000_000);
        record_settlement(&mut position, &first, &first_fee, first_at).unwrap();
        assert_eq!((position.paid_gross, position.accrued_gross), (1_000_000, 1_273_148));
        assert!(!position.claimed);

        // The unpaid 273_148 is still owed; the sub-lamport remainder carried.
        let second_at = 500 + 2_000;
        let second = simple_settlement(&position, &config, second_at).unwrap();
        let reference =
            simple_settlement(&simple_position(1_000_000_001, 500), &config, second_at).unwrap();
        assert_eq!(second.owed, reference.owed - 1_000_000);
        assert_eq!(
            second.accrual.unwrap().remainder,
            reference.accrual.unwrap().remainder,
            "two steps accrue exactly what one step would"
        );
        assert_eq!(
            second.accrual.unwrap().accrued_gross,
            reference.accrual.unwrap().accrued_gross
        );
    }

    #[test]
    fn simple_position_terminates_at_maturity_even_with_nothing_paid() {
        let config = config();
        let mut position = simple_position(1_000_000_000, 0);
        let before = simple_settlement(&position, &config, 21 * DAY - 1).unwrap();
        let fee = settle_fee(before.owed, 10_000_000_000, before.toll_bps, 0).unwrap();
        let mut early = position.clone();
        record_settlement(&mut early, &before, &fee, 21 * DAY - 1).unwrap();
        assert!(!early.claimed, "one second before maturity the position stays open");

        let settlement = simple_settlement(&position, &config, 21 * DAY).unwrap();
        assert_eq!(settlement.owed, 2_310_000_000);
        let empty_pot = settle_fee(settlement.owed, 0, settlement.toll_bps, 0).unwrap();
        record_settlement(&mut position, &settlement, &empty_pot, 21 * DAY).unwrap();
        assert!(position.claimed);
        assert_eq!(position.paid_gross, 0);
        // Accrual is clamped at the 21-day cap however late the claim is.
        let late = simple_settlement(&simple_position(1_000_000_000, 0), &config, 40 * DAY).unwrap();
        assert_eq!(late.owed, 2_310_000_000);
    }

    #[test]
    fn backwards_clock_is_rejected_by_the_settlement() {
        let position = simple_position(1_000_000_000, 100);
        assert_code(simple_settlement(&position, &config(), 99), Code::BackwardsTime);
        simple_settlement(&position, &config(), 100).unwrap();
    }

    #[test]
    fn carried_interest_tiers_switch_exactly_at_day_7_and_day_14() {
        let config = config();
        for (elapsed, bps) in [
            (0, 1_200),
            (7 * DAY - 1, 1_200),
            (7 * DAY, 750),
            (14 * DAY - 1, 750),
            (14 * DAY, 300),
            (21 * DAY, 300),
            (400 * DAY, 300),
        ] {
            assert_eq!(simple_toll(&config, elapsed).unwrap(), bps, "elapsed {elapsed}");
        }
    }

    #[test]
    fn pinned_config_must_agree_with_the_hard_coded_simple_tables() {
        let position = simple_position(1_000_000_000, 0);
        let mut slower = config();
        slower.plan_rates_bps_per_day[0] = 1_000;
        assert_code(simple_owed(&position, DAY, &slower), Code::InvalidState);

        let mut harsher = config();
        harsher.simple_toll_bps = [1_300, 750, 300];
        assert_code(simple_toll(&harsher, 0), Code::InvalidState);

        let mut earlier = config();
        earlier.simple_toll_day_bounds = [6, 14];
        assert_eq!(simple_toll(&earlier, 6 * DAY - 1).unwrap(), 1_200);
        assert_code(simple_toll(&earlier, 6 * DAY), Code::InvalidState);
        assert_eq!(simple_toll(&earlier, 7 * DAY).unwrap(), 750);
    }

    #[test]
    fn compound_15_matures_at_exactly_fifteen_days_with_the_9_percent_jackpot_fee() {
        let position = compound_position(PlanKindArg::Compound15, 1_000_000_000, 7);
        assert_code(
            compound_settlement(&position, &config(), 7 + 15 * DAY - 1),
            Code::NotMature,
        );
        let settlement = compound_settlement(&position, &config(), 7 + 15 * DAY).unwrap();
        assert_eq!((settlement.owed, settlement.toll_bps), (4_473_565_759, 900));
        assert!(settlement.accrual.is_none());
        let fee = settle_fee(settlement.owed, 10_000_000_000, settlement.toll_bps, 0).unwrap();
        assert_eq!(
            (fee.paid_gross, fee.toll, fee.net, fee.insolvent),
            (4_473_565_759, 402_620_918, 4_070_944_841, false)
        );
    }

    #[test]
    fn compound_30_matures_at_exactly_thirty_days_with_the_13_percent_jackpot_fee() {
        let position = compound_position(PlanKindArg::Compound30, 1_000_000_000, 0);
        assert_code(
            compound_settlement(&position, &config(), 30 * DAY - 1),
            Code::NotMature,
        );
        let settlement = compound_settlement(&position, &config(), 30 * DAY).unwrap();
        assert_eq!((settlement.owed, settlement.toll_bps), (12_267_678_469, 1_300));
        let fee = settle_fee(settlement.owed, 20_000_000_000, settlement.toll_bps, 0).unwrap();
        assert_eq!(
            (fee.paid_gross, fee.toll, fee.net),
            (12_267_678_469, 1_594_798_200, 10_672_880_269)
        );
    }

    #[test]
    fn compound_owed_never_overflows_and_narrows_to_the_pot() {
        let position = compound_position(PlanKindArg::Compound30, u64::MAX, 0);
        let settlement = compound_settlement(&position, &config(), 30 * DAY).unwrap();
        assert!(settlement.owed > u128::from(u64::MAX));
        let fee = settle_fee(settlement.owed, 7, settlement.toll_bps, 0).unwrap();
        assert_eq!((fee.paid_gross, fee.insolvent), (7, true));
    }

    #[test]
    fn compounding_settlement_is_one_shot_and_records_the_full_payout() {
        let mut position = compound_position(PlanKindArg::Compound15, 1_000_000_000, 0);
        let settlement = compound_settlement(&position, &config(), 15 * DAY).unwrap();
        let fee = settle_fee(settlement.owed, 1_000, settlement.toll_bps, 0).unwrap();
        record_settlement(&mut position, &settlement, &fee, 15 * DAY).unwrap();
        assert!(position.claimed);
        assert_eq!(
            (position.accrued_gross, position.paid_gross, position.net_payout),
            (4_473_565_759, 1_000, u128::from(fee.net))
        );
        let pool = Pool { round_id: 1, ..Pool::default() };
        assert_code(payout_gate(&position, &pool), Code::AlreadyClaimed);
    }

    #[test]
    fn plan_kind_selects_the_settlement_table() {
        let simple = simple_position(1, 0);
        assert_code(compound_owed(&simple, &config()), Code::InvalidState);
        assert_code(compound_settlement(&simple, &config(), 40 * DAY), Code::InvalidState);
        let mut unknown = simple_position(1, 0);
        unknown.kind = 9;
        assert_code(plan_of(&unknown), Code::InvalidState);
        assert_eq!(
            compound_index(&compound_position(PlanKindArg::Compound15, 1, 0)).unwrap(),
            0
        );
        assert_eq!(
            compound_index(&compound_position(PlanKindArg::Compound30, 1, 0)).unwrap(),
            1
        );
    }

    #[test]
    fn payout_gate_fails_in_the_documented_order() {
        let mut pool = Pool {
            round_id: 2,
            active_ticket: 9,
            current_close_id: Some(1),
            ..Pool::default()
        };
        let mut position = Position { claimed: true, round_id: 1, ..simple_position(1, 0) };
        assert_code(payout_gate(&position, &pool), Code::PositionRoundClosed);
        position.round_id = 2;
        assert_code(payout_gate(&position, &pool), Code::AlreadyClaimed);
        position.claimed = false;
        assert_code(payout_gate(&position, &pool), Code::Closing);
        pool.current_close_id = None;
        assert_code(payout_gate(&position, &pool), Code::AllocationPending);
        pool.active_ticket = 0;
        payout_gate(&position, &pool).unwrap();
    }

    #[test]
    fn close_simple_requires_a_matured_open_current_round_position_with_nothing_owed() {
        let config = config();
        let mut position = simple_position(1_000_000_000, 0);
        assert_code(close_simple_gate(&position, 2, 21 * DAY, &config), Code::PositionRoundClosed);
        assert_code(close_simple_gate(&position, 1, 21 * DAY - 1, &config), Code::NotMature);
        assert_code(
            close_simple_gate(&position, 1, 21 * DAY, &config),
            Code::InvalidState,
        );
        // Everything accrued has been withdrawn.
        position.accrued_gross = 2_310_000_000;
        position.paid_gross = 2_310_000_000;
        position.last_accrued_at = 21 * DAY;
        close_simple_gate(&position, 1, 21 * DAY, &config).unwrap();
        close_simple_gate(&position, 1, 40 * DAY, &config).unwrap();
        position.claimed = true;
        assert_code(close_simple_gate(&position, 1, 21 * DAY, &config), Code::AlreadyClaimed);
    }

    // --- Close reserve lock and CloseRecord binding ---

    #[test]
    fn lock_split_takes_sponsor_cash_first_then_the_capped_owner_contribution() {
        assert_eq!(lock_split(2, 1, 1).unwrap(), (1, 1));
        assert_eq!(lock_split(2, 5, 0).unwrap(), (2, 0));
        assert_eq!(lock_split(2, 2, 9).unwrap(), (2, 0));
        assert_eq!(lock_split(2, 0, 9).unwrap(), (0, 2));
        assert_eq!(lock_split(2, 0, 2).unwrap(), (0, 2));
        assert_eq!(lock_split(10, 4, u64::MAX).unwrap(), (4, 6));
        assert_eq!(lock_split(0, 0, 0).unwrap(), (0, 0));
        assert_code(lock_split(2, 1, 0), Code::ReserveUnavailable);
        assert_code(lock_split(2, 0, 1), Code::ReserveUnavailable);
        assert_code(lock_split(u64::MAX, 0, u64::MAX - 1), Code::ReserveUnavailable);
    }

    fn expected_reserve(per_set: u64, sets_missing: u64, extension: bool, config: &ConfigVersion) -> u64 {
        let rent = Rent::default();
        let fee = config.provider_request_fee_lamports;
        rent.minimum_balance(CandidatePage::SPACE) * per_set * sets_missing
            + if extension { rent.minimum_balance(RegistryPage::SPACE) } else { 0 }
            + rent.minimum_balance(Backer::SPACE)
            + rent.minimum_balance(usize::from(config.pending_request_bytes))
            + fee
            + fee * u64::from(config.provider_fee_headroom_bps) / 10_000
    }

    fn binding<'a>(
        lock: &'a CloseLock,
        participants: u64,
        registry_len: u64,
        ticket: Option<u64>,
        config: &ConfigVersion,
    ) -> CloseBinding<'a> {
        CloseBinding {
            round_id: 3,
            config_version: 1,
            close_started_at: 1_000,
            deadline_seconds: config.deadline_seconds,
            closing_ticket_id: ticket,
            participant_count: participants,
            registry_len,
            lock,
            destination: key(9),
        }
    }

    #[test]
    fn closing_reserve_scales_with_participants_and_registry_tail() {
        let mut config = config();
        config.provider_request_fee_lamports = 10_000;
        for (participants, registry_len, per_set, extension) in
            [(8u64, 8u64, 1u64, true), (9, 9, 2, false), (1, 0, 1, true), (16, 7, 2, false)]
        {
            let pool = Pool {
                round_id: 3,
                participant_count: participants,
                registry_len,
                ..Pool::default()
            };
            let rent_components =
                RentComponents { ops: 7, owner: 0, owner_refund: Pubkey::default() };
            let mut record = CloseRecord {
                bump: 250,
                close_id: 4,
                rent: rent_components,
                ..CloseRecord::default()
            };
            let lock = close_lock(&Rent::default(), &config, &record, &pool, u64::MAX, 0).unwrap();
            assert_eq!(lock.required, expected_reserve(per_set, 2, extension, &config));
            assert_eq!((lock.sponsor, lock.owner), (lock.required, 0));

            bind_close_record(&mut record, &binding(&lock, participants, registry_len, None, &config))
                .unwrap();
            assert_eq!(record.required_pages_per_set, per_set);
            assert_eq!(record.registry_extension_required, extension);
            assert_eq!(record.participant_count, participants);
            assert_eq!(record.required_reserve, lock.required);
            assert_eq!(
                record.locked_sponsor_remaining + record.locked_owner_remaining,
                record.required_reserve
            );
            assert_eq!(record.reserve_schedule, lock.schedule);
            assert_eq!(record.approved_provider_fee_cap, 12_000);
            // Identity and provisioning fields are never overwritten.
            assert_eq!((record.bump, record.close_id), (250, 4));
            assert_eq!(record.rent, rent_components);
            assert!(!record.finalized && !record.sealed);
        }
    }

    #[test]
    fn over_provisioned_candidate_sets_never_offset_each_other() {
        let config = config();
        let pool = Pool { participant_count: 9, registry_len: 9, ..Pool::default() };
        let base = CloseRecord::default();
        let all_missing = close_lock(&Rent::default(), &config, &base, &pool, u64::MAX, 0).unwrap();
        let record = CloseRecord {
            existing_preferred_pages: 5,
            existing_fallback_pages: 1,
            ..CloseRecord::default()
        };
        let lock = close_lock(&Rent::default(), &config, &record, &pool, u64::MAX, 0).unwrap();
        // Preferred is fully covered; fallback still misses one of two pages.
        assert_eq!(lock.required, expected_reserve(1, 1, false, &config));
        assert!(lock.required < all_missing.required);
    }

    #[test]
    fn closing_binding_pins_deadline_destination_and_cash_commitment() {
        let config = config();
        let pool = Pool { participant_count: 1, registry_len: 0, ..Pool::default() };
        let mut record = CloseRecord::default();
        // Sponsor covers half the reserve, the owner the rest.
        let required =
            close_lock(&Rent::default(), &config, &record, &pool, u64::MAX, 0).unwrap().required;
        let lock = close_lock(&Rent::default(), &config, &record, &pool, required / 2, required)
            .unwrap();
        assert_eq!((lock.sponsor, lock.owner), (required / 2, required - required / 2));

        bind_close_record(&mut record, &binding(&lock, 1, 0, None, &config)).unwrap();
        assert_eq!(record.close_started_at, 1_000);
        assert_eq!(record.promotion_deadline, 1_000 + 600);
        assert_eq!(record.owner_refund_destination, key(9));
        assert_eq!((record.round_id, record.config_version), (3, 1));
        assert!(record.cash_committed, "no ticket means the closing cash is already committed");
        assert_eq!(record.closing_ticket_id, None);
        assert_eq!(record.locked_sponsor_remaining, required / 2);
        assert_eq!(record.locked_owner_remaining, required - required / 2);

        bind_close_record(&mut record, &binding(&lock, 1, 0, Some(5), &config)).unwrap();
        assert!(!record.cash_committed, "a ticketed toll commits at ticket commit");
        assert_eq!(record.closing_ticket_id, Some(5));
    }

    #[test]
    fn provider_fee_cap_adds_floored_headroom() {
        let schedule = |fee: u64, headroom_bps: u16| ReserveSchedule {
            provider_fee: fee,
            headroom_bps,
            ..ReserveSchedule::default()
        };
        assert_eq!(provider_fee_cap(&schedule(10_000, 2_000)).unwrap(), 12_000);
        assert_eq!(provider_fee_cap(&schedule(5, 2_000)).unwrap(), 6);
        assert_eq!(provider_fee_cap(&schedule(4, 2_000)).unwrap(), 4);
        assert_eq!(provider_fee_cap(&schedule(0, 2_000)).unwrap(), 0);
        assert_code(provider_fee_cap(&schedule(u64::MAX, 1)), Code::Overflow);
    }

    // --- Toll routing and tickets ---

    #[test]
    fn toll_routes_to_nothing_the_seed_or_a_ticket() {
        assert_eq!(toll_route(0, 0), TollRoute::None);
        assert_eq!(toll_route(0, 9), TollRoute::None);
        assert_eq!(toll_route(1, 0), TollRoute::Seed);
        assert_eq!(toll_route(1, 1), TollRoute::Ticket);
        assert_eq!(toll_route(13_200_000, 8), TollRoute::Ticket);
    }

    #[test]
    fn snapshot_pages_must_cover_the_registry_before_a_toll_is_taken() {
        snapshots_cover(0, 0).unwrap();
        snapshots_cover(1, 8).unwrap();
        snapshots_cover(2, 9).unwrap();
        assert_code(snapshots_cover(0, 1), Code::PageNotProvisioned);
        assert_code(snapshots_cover(1, 9), Code::PageNotProvisioned);
        assert_code(snapshots_cover(u64::MAX, 1), Code::Overflow);
    }

    #[test]
    fn a_started_ticket_pins_the_config_split_and_zeroes_every_cursor() {
        let mut config = config();
        config.senior_bps = 3_000;
        config.a_bps = 5_000;
        config.b_bps = 2_000;
        let ticket = ticket_start(251, 7, Some(3), 17, 4, 13_200_000, &config);
        let expected = Ticket {
            bump: 251,
            id: 7,
            phase: DISCOVER,
            close_id: Some(3),
            captured_len: 17,
            expected_version: 4,
            toll: 13_200_000,
            senior_bps: 3_000,
            a_bps: 5_000,
            b_bps: 2_000,
            ..Ticket::default()
        };
        assert_eq!(ticket.try_to_vec().unwrap(), expected.try_to_vec().unwrap());
        assert_eq!(ticket.a_remainder_total, [0; 32]);
        assert_eq!(ticket.sealed_a_denominator, [0; 32]);
        assert_eq!(
            ticket_start(251, 8, None, 1, 0, 1, &config).close_id,
            None,
            "a nonclosing payout carries no close id"
        );
    }

    #[test]
    fn protected_destinations_are_matched_by_key() {
        let protected = [key(1), key(2), key(3)];
        assert!(is_protected(&key(2), &protected));
        assert!(!is_protected(&key(4), &protected));
        assert!(!is_protected(&key(1), &[]));
    }

    // --- Admission gates, quota, entry math ---

    #[test]
    fn open_gate_fails_in_the_documented_order() {
        let mut gate = Gate { paused: true, closing: true, active_ticket: 3, round_id: 2, config_version: 5 };
        assert_code(open_gate(&gate, 1, 4, 1, 10), Code::AdmissionPaused);
        gate.paused = false;
        assert_code(open_gate(&gate, 1, 4, 1, 10), Code::Closing);
        gate.closing = false;
        assert_code(open_gate(&gate, 1, 4, 1, 10), Code::StaleRound);
        assert_code(open_gate(&gate, 2, 4, 1, 10), Code::StaleConfig);
        assert_code(open_gate(&gate, 2, 5, 9, 10), Code::MinimumDeposit);
        // A nonclosing allocation ticket does not block opening.
        open_gate(&gate, 2, 5, 10, 10).unwrap();
    }

    #[test]
    fn series_a_gate_fails_in_the_documented_order() {
        let mut gate = Gate { paused: true, closing: true, active_ticket: 3, round_id: 2, config_version: 5 };
        assert_code(series_a_gate(&gate, false, 1, 1, 50), Code::PublicSeriesADisabled);
        assert_code(series_a_gate(&gate, true, 1, 1, 50), Code::AdmissionPaused);
        gate.paused = false;
        assert_code(series_a_gate(&gate, true, 1, 1, 50), Code::Closing);
        gate.closing = false;
        assert_code(series_a_gate(&gate, true, 1, 1, 50), Code::AllocationPending);
        gate.active_ticket = 0;
        assert_code(series_a_gate(&gate, true, 1, 1, 50), Code::StaleRound);
        assert_code(series_a_gate(&gate, true, 2, 49, 50), Code::MinimumDeposit);
        series_a_gate(&gate, true, 2, 50, 50).unwrap();
    }

    #[test]
    fn gate_reads_the_pool_and_admission_facts() {
        let pool = Pool {
            round_id: 4,
            active_config_version: 2,
            active_ticket: 6,
            current_close_id: Some(1),
            ..Pool::default()
        };
        let admission = Admission { paused: true, ..Admission::default() };
        let gate = Gate::of(&pool, &admission);
        assert_eq!(
            (gate.paused, gate.closing, gate.active_ticket, gate.round_id, gate.config_version),
            (true, true, 6, 4, 2)
        );
        let idle = Gate::of(&Pool::default(), &Admission::default());
        assert!(!idle.paused && !idle.closing);
        open_gate(&open_gate_view(), 1, 1, 10, 10).unwrap();
    }

    #[test]
    fn quota_blocks_the_fourth_success_until_the_oldest_is_exactly_an_hour_old() {
        let owner = key(7);
        let mut user = User::default();
        for (index, at) in [1_000u64, 1_001, 1_002].into_iter().enumerate() {
            user = quota_after(&user, 254, owner, at).unwrap();
            assert_eq!(usize::from(user.call_count), index + 1);
        }
        assert_eq!((user.owner, user.bump), (owner, 254));
        let before = user.try_to_vec().unwrap();
        assert_code(quota_after(&user, 254, owner, 1_000 + 3_599), Code::QuotaExceeded);
        assert_eq!(user.try_to_vec().unwrap(), before, "a rejected call consumes nothing");
        let user = quota_after(&user, 254, owner, 1_000 + 3_600).unwrap();
        assert_eq!(user.successful_at, [1_001, 1_002, 4_600]);
        assert_code(quota_after(&user, 254, owner, 4_599), Code::BackwardsTime);
    }

    #[test]
    fn quota_is_computed_on_a_copy_and_existing_identity_is_kept() {
        let fresh = User::default();
        let next = quota_after(&fresh, 9, key(3), 50).unwrap();
        assert_eq!(fresh.call_count, 0, "the original is untouched until write-back");
        assert_eq!((next.call_count, next.owner, next.bump), (1, key(3), 9));
        // An initialised user keeps its bump and owner.
        let again = quota_after(&next, 200, key(4), 51).unwrap();
        assert_eq!((again.owner, again.bump), (key(3), 9));
    }

    #[test]
    fn front_end_load_is_floored_and_conserves_the_deposit() {
        assert_eq!(entry_split(1_000_000_000, 400).unwrap(), (40_000_000, 960_000_000));
        assert_eq!(entry_split(10_000_000, 400).unwrap(), (400_000, 9_600_000));
        assert_eq!(entry_split(10_000_001, 400).unwrap(), (400_000, 9_600_001));
        assert_eq!(entry_split(5, 0).unwrap(), (0, 5));
        for gross in [10_000_000u64, 12_345_678_901, u64::MAX] {
            let host = fees::front_end_load(gross).unwrap();
            let (load, pot_credit) = entry_split(gross, 400).unwrap();
            assert_eq!((load, pot_credit), (host.entry_fee, host.pot_credit));
            assert_eq!(load + pot_credit, gross);
        }
        assert_code(entry_split(100, 20_000), Code::InvalidState);
    }

    #[test]
    fn prospective_b_width_rejects_a_deposit_a_later_promotion_could_not_hold() {
        assert_eq!(series_b_width_ok(1_000_000_000, 0, 0).unwrap(), 1_160_000_000);
        assert_eq!(
            series_b_width_ok(1_000_000_000, 400_000_000, 5).unwrap(),
            600_000_000 * 116 / 100 + 5
        );
        // Payouts above deposits never make the loss negative.
        assert_eq!(series_b_width_ok(10, 100, 3).unwrap(), 3);
        assert_code(series_b_width_ok(u128::MAX, 0, 0), Code::Overflow);
        assert_code(series_b_width_ok(1_000, 0, u128::MAX), Code::Overflow);
    }

    #[test]
    fn series_a_outcome_appends_only_inactive_backers_above_the_cutoff() {
        let minted = fees::series_a_entitlement(50_000_000).unwrap();
        assert_eq!(minted, 62_000_000);

        let fresh = Backer::default();
        assert_eq!(series_a_outcome(&fresh, minted).unwrap(), (62_000_000, true));

        // An active same-owner top-up never appends again.
        let active = Backer { active: true, entitlement: 62_000_000, lifetime: 10, ..Backer::default() };
        assert_eq!(series_a_outcome(&active, minted).unwrap(), (124_000_000, false));

        // A pruned Backer reopens only if funding lifts it above 10 remaining.
        let pruned = Backer { entitlement: 100, lifetime: 100, ..Backer::default() };
        assert_eq!(series_a_outcome(&pruned, 10).unwrap(), (110, false));
        assert_eq!(series_a_outcome(&pruned, 11).unwrap(), (111, true));

        let overfull = Backer { entitlement: u128::MAX, ..Backer::default() };
        assert_code(series_a_outcome(&overfull, 1), Code::Overflow);
        let corrupt = Backer { entitlement: 1, lifetime: 9, ..Backer::default() };
        assert_code(series_a_outcome(&corrupt, 0), Code::InvalidState);
    }

    // --- Participant and registry page plumbing ---

    #[test]
    fn page_addresses_are_distinct_per_round_and_index() {
        assert_ne!(participant_page_key(1, 0), participant_page_key(1, 1));
        assert_ne!(participant_page_key(1, 0), participant_page_key(2, 0));
        assert_eq!(participant_page_key(7, 3), participant_page_key(7, 3));
        assert_ne!(registry_page_key(0), registry_page_key(1));
        assert_ne!(close_record_key(1), close_record_key(2));
    }

    #[test]
    fn participant_page_loads_only_when_canonical_provisioned_and_roomy() {
        let (round, count) = (3u64, 9u64); // index 1
        let address = participant_page_key(round, 1);
        let page = participant_page(round, 1, 1);
        let good = info(address, crate::ID, serialized(&page));
        assert_eq!(load_participant_page(&good, round, count).unwrap().len, 1);

        let wrong_key = info(participant_page_key(round, 2), crate::ID, serialized(&page));
        assert_code(load_participant_page(&wrong_key, round, count), Code::WrongAccount);

        let unprovisioned = info(address, system_program::ID, vec![]);
        assert_code(load_participant_page(&unprovisioned, round, count), Code::PageNotProvisioned);
        let foreign = info(address, key(9), serialized(&page));
        assert_code(load_participant_page(&foreign, round, count), Code::PageNotProvisioned);
        let wrong_type = info(address, crate::ID, serialized(&RegistryPage::default()));
        assert_code(load_participant_page(&wrong_type, round, count), Code::PageNotProvisioned);

        let other_round = info(address, crate::ID, serialized(&participant_page(4, 1, 1)));
        assert_code(load_participant_page(&other_round, round, count), Code::PageNotProvisioned);
        let other_index = info(address, crate::ID, serialized(&participant_page(round, 0, 1)));
        assert_code(load_participant_page(&other_index, round, count), Code::PageNotProvisioned);
        let full = info(address, crate::ID, serialized(&participant_page(round, 1, 8)));
        assert_code(load_participant_page(&full, round, count), Code::InvalidState);
    }

    #[test]
    fn participant_append_round_trips_through_the_account_bytes() {
        let round = 5u64;
        let address = participant_page_key(round, 0);
        let account = info(address, crate::ID, serialized(&participant_page(round, 0, 0)));
        let owner = key(8);
        let mut page = load_participant_page(&account, round, 0).unwrap();
        append_participant(&mut page, owner).unwrap();
        write_account(&account, &page).unwrap();

        let reloaded = load_participant_page(&account, round, 1).unwrap();
        assert_eq!(reloaded.len, 1);
        assert_eq!(reloaded.rows[0], owner);

        let mut full = participant_page(round, 0, 8);
        assert_code(append_participant(&mut full, owner), Code::InvalidState);
    }

    #[test]
    fn registry_tail_page_is_the_page_the_next_row_lands_on() {
        let page = RegistryPage { bump: 1, index: 1, ..RegistryPage::default() };
        let account = info(registry_page_key(1), crate::ID, serialized(&page));
        // registry_len 8..=15 appends on page 1.
        let mut loaded = load_registry_tail(&account, 9).unwrap();
        append_registry(&mut loaded, 9, key(6)).unwrap();
        assert_eq!(loaded.keys[1], key(6));
        write_account(&account, &loaded).unwrap();
        assert_eq!(load_registry_tail(&account, 9).unwrap().keys[1], key(6));

        // len 16 needs page 2: this account is the wrong address.
        assert_code(load_registry_tail(&account, 16), Code::WrongAccount);
        let missing = info(registry_page_key(0), system_program::ID, vec![]);
        assert_code(load_registry_tail(&missing, 3), Code::PageNotProvisioned);
        let mislabelled = RegistryPage { index: 3, ..RegistryPage::default() };
        let bad = info(registry_page_key(1), crate::ID, serialized(&mislabelled));
        assert_code(load_registry_tail(&bad, 8), Code::PageNotProvisioned);
        assert_eq!(registry_page_index(8), 1);
    }
}
