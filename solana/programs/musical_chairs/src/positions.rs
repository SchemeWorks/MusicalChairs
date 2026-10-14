//! Position admission, Series-A funding, and position payout ABI.
//!
//! This file owns the production account boundary for the first financial
//! tranche.  The public handlers are intentionally gated while the coordinator
//! captures the required RED runtime assertions.  The contexts and argument
//! types are already the frozen interface: all later state changes must happen
//! through these accounts, with owner custody and nonce replay checks enforced
//! by the handler before any transfer or economic mutation.

use anchor_lang::prelude::*;

use crate::state::{
    Admission, Backer, GameVault, Pool, Position, Ticket, User, UserRound,
    ADMISSION_SEED, BACKER_REMAINING_CUTOFF, BACKER_SEED, GAME_VAULT_SEED, OPS_CASH_SEED,
    OPS_SEED, PAGE_CAPACITY, POOL_SEED, POSITION_SEED, TICKET_SEED,
    USER_ROUND_SEED, USER_SEED,
};

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

/// Arguments kept as a named client-side shape in addition to the direct
/// Anchor instruction arguments.  The instruction methods below retain direct
/// scalar arguments so the generated IDL remains simple and explicit.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenPositionArgs {
    pub plan: PlanKindArg,
    pub gross_lamports: u64,
    pub client_nonce: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundSeriesAArgs {
    pub gross_lamports: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimPositionArgs {
    pub minimum_receive: u64,
    pub close_simple: bool,
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
/// custody mutation.
#[event_cpi]
#[derive(Accounts)]
#[instruction(plan: PlanKindArg, gross_lamports: u64, client_nonce: u64)]
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
    pub system_program: Program<'info, System>,
}

/// Public same-owner Series-A funding.  The registry page is supplied as an
/// already provisioned shared account; creating it inside this financial
/// instruction would make acceptance depend on a later rent budget.
#[event_cpi]
#[derive(Accounts)]
#[instruction(gross_lamports: u64)]
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
    /// CHECK: the handler validates the canonical preprovisioned page only
    /// when a new/reopened row appends; active same-owner top-ups need no page.
    #[account(mut)]
    pub registry: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Owner-authorized simple/compound payout.  `ops_cash` is intentionally
/// readonly: the handler reads the system-owned reserve PDA while mutating the
/// Ops state account, and never treats a caller-supplied cash account as an
/// authority.  `destination` is deliberately unchecked at the type level so
/// the handler can reject protected aliases while permitting any ordinary
/// system-owned destination selected by the signed owner.
#[event_cpi]
#[derive(Accounts)]
#[instruction(minimum_receive: u64, close_simple: bool)]
pub struct ClaimPosition<'info> {
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
    pub ops: Box<Account<'info, crate::state::Ops>>,
    /// CHECK: validated by the shared custody helper as the canonical
    /// system-owned `[ops-cash]` PDA before any close reserve is locked.
    #[account(seeds = [OPS_CASH_SEED], bump)]
    pub ops_cash: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [USER_SEED, owner.key().as_ref()],
        bump = user.bump,
        has_one = owner,
    )]
    pub user: Box<Account<'info, User>>,
    #[account(
        mut,
        seeds = [USER_ROUND_SEED, &pool.round_id.to_le_bytes(), owner.key().as_ref()],
        bump = user_round.bump,
        constraint = user_round.round_id == pool.round_id,
        constraint = user_round.owner == owner.key(),
    )]
    pub user_round: Box<Account<'info, UserRound>>,
    #[account(
        mut,
        has_one = owner,
        constraint = position.round_id == pool.round_id,
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
    /// CHECK: the handler rejects protected aliases and validates the owner
    /// signature/destination policy before `send_game`.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Initial ABI stubs.  The coordinator will replace these bodies only after
/// RED assertions capture the expected account/state failures.  Keeping the
/// handlers present now lets the root program expose stable instruction names
/// without accidentally claiming financial behavior is already implemented.
pub fn open_position(
    _ctx: Context<OpenPosition>,
    _plan: PlanKindArg,
    _gross_lamports: u64,
    _client_nonce: u64,
) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn fund_series_a(_ctx: Context<FundSeriesA>, _gross_lamports: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn claim_position(
    _ctx: Context<ClaimPosition>,
    _minimum_receive: u64,
    _close_simple: bool,
) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
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
}
