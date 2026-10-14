//! Native financial account/instruction tranche. No deployment or complete-game claim.
#![forbid(unsafe_code)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke_signed, system_instruction, system_program};
pub mod state;
pub mod controls;
pub mod positions;
pub mod allocation;
use controls::*;
use positions::*;
use allocation::*;
pub use state::*;
declare_id!("7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7");

#[error_code]
pub enum ErrorCode {
    Overflow, InvalidState, BackwardsTime, QuotaExceeded, NotAuthorized,
    WrongAccount, WrongOwner, InsufficientOperations, ProtectedDestination,
    AdmissionPaused, AllocationPending, Closing, MinimumDeposit, MinimumReceive,
    NotMature, AlreadyClaimed, StaleRound, Replay, InvalidPhase, CursorMismatch,
    CapExceeded, NotImplemented,
}

pub fn checked_add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| error!(ErrorCode::Overflow))
}
pub fn checked_sub(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b).ok_or_else(|| error!(ErrorCode::InvalidState))
}
pub fn now() -> Result<u64> {
    u64::try_from(Clock::get()?.unix_timestamp).map_err(|_| error!(ErrorCode::BackwardsTime))
}
pub fn game_accounted(pool: &Pool) -> Result<u64> {
    [pool.pot, pool.seed, pool.management, pool.pending_toll, pool.funded_claims, pool.excess]
        .into_iter().try_fold(0, checked_add)
}
/// Protect current runtime rent and classify unsolicited SOL separately.
pub fn sync_game(pool: &mut Pool, vault: &AccountInfo) -> Result<()> {
    require_keys_eq!(*vault.owner, ID, ErrorCode::WrongOwner);
    require_keys_eq!(vault.key(), Pubkey::find_program_address(&[GAME_VAULT_SEED], &ID).0, ErrorCode::WrongAccount);
    let spendable = checked_sub(vault.lamports(), Rent::get()?.minimum_balance(vault.data_len()))?;
    let recorded = game_accounted(pool)?;
    pool.excess = checked_add(pool.excess, checked_sub(spendable, recorded)?)?;
    Ok(())
}
pub fn validate_destination(destination: &AccountInfo) -> Result<()> {
    require!(destination.is_writable && !destination.executable, ErrorCode::ProtectedDestination);
    require_keys_eq!(*destination.owner, system_program::ID, ErrorCode::ProtectedDestination);
    require!(destination.data_is_empty(), ErrorCode::ProtectedDestination);
    // OpsCash is system-owned and must never become a user payout destination.
    require!(destination.key() != Pubkey::find_program_address(&[OPS_CASH_SEED], &ID).0, ErrorCode::ProtectedDestination);
    Ok(())
}
pub fn send_game(vault: &AccountInfo, destination: &AccountInfo, amount: u64) -> Result<()> {
    validate_destination(destination)?;
    require_keys_eq!(*vault.owner, ID, ErrorCode::WrongOwner);
    require_keys_eq!(vault.key(), Pubkey::find_program_address(&[GAME_VAULT_SEED], &ID).0, ErrorCode::WrongAccount);
    require!(vault.is_writable && vault.key() != destination.key(), ErrorCode::ProtectedDestination);
    let after = checked_sub(vault.lamports(), amount)?;
    require!(after >= Rent::get()?.minimum_balance(vault.data_len()), ErrorCode::InvalidState);
    let received = checked_add(destination.lamports(), amount)?;
    **vault.try_borrow_mut_lamports()? = after;
    **destination.try_borrow_mut_lamports()? = received;
    Ok(())
}
pub fn sync_ops(ops: &mut Ops, cash: &AccountInfo) -> Result<()> {
    require_keys_eq!(cash.key(), Pubkey::find_program_address(&[OPS_CASH_SEED], &ID).0, ErrorCode::WrongAccount);
    require_keys_eq!(*cash.owner, system_program::ID, ErrorCode::WrongOwner);
    require!(cash.data_is_empty() && !cash.executable, ErrorCode::WrongAccount);
    let available = checked_sub(cash.lamports(), Rent::get()?.minimum_balance(0))?;
    let recorded = checked_add(checked_add(ops.free, ops.locked)?, ops.excess)?;
    ops.excess = checked_add(ops.excess, checked_sub(available, recorded)?)?;
    Ok(())
}
/// Shared pages are sponsor-funded. Prefunded empty PDAs cannot block provisioning.
/// Canonical target seeds include its bump; both cash and target sign the CPI.
pub fn create_shared<'info>(target: &AccountInfo<'info>, cash: &AccountInfo<'info>, system: &AccountInfo<'info>, seeds: &[&[u8]], space: usize, ops: &mut Ops) -> Result<()> {
    sync_ops(ops, cash)?;
    require_keys_eq!(system.key(), system_program::ID, ErrorCode::WrongAccount);
    require_keys_eq!(target.key(), Pubkey::create_program_address(seeds, &ID).map_err(|_| error!(ErrorCode::WrongAccount))?, ErrorCode::WrongAccount);
    require_keys_eq!(*target.owner, system_program::ID, ErrorCode::WrongOwner);
    require!(target.data_is_empty() && !target.executable && target.is_writable, ErrorCode::WrongAccount);
    let rent = Rent::get()?.minimum_balance(space);
    let debit = rent.saturating_sub(target.lamports());
    require!(ops.free >= debit, ErrorCode::InsufficientOperations);
    let (_, cash_bump) = Pubkey::find_program_address(&[OPS_CASH_SEED], &ID);
    let cash_bump_arr = [cash_bump];
    let cash_seeds: &[&[u8]] = &[OPS_CASH_SEED, &cash_bump_arr];
    if debit > 0 {
        invoke_signed(&system_instruction::transfer(cash.key, target.key, debit), &[cash.clone(), target.clone(), system.clone()], &[cash_seeds])?;
        ops.free = checked_sub(ops.free, debit)?;
    }
    invoke_signed(&system_instruction::allocate(target.key, space as u64), &[target.clone(), system.clone()], &[seeds])?;
    invoke_signed(&system_instruction::assign(target.key, &ID), &[target.clone(), system.clone()], &[seeds])?;
    Ok(())
}

#[program]
pub mod musical_chairs_program {
    use super::*;
    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> { controls::initialize(ctx,args) }
    pub fn set_pause(ctx: Context<Pause>, paused: bool) -> Result<()> { controls::set_pause(ctx,paused) }
    pub fn fund_operations(ctx: Context<FundOperations>, amount: u64) -> Result<()> { controls::fund_operations(ctx,amount) }
    pub fn withdraw_operations(ctx: Context<WithdrawOperations>, amount: u64) -> Result<()> { controls::withdraw_operations(ctx,amount) }
    pub fn withdraw_management(ctx: Context<WithdrawManagement>, amount: u64) -> Result<()> { controls::withdraw_management(ctx,amount) }
    pub fn provision_snapshot(ctx: Context<ProvisionSnapshot>, index: u64) -> Result<()> { controls::provision_snapshot(ctx,index) }
    pub fn provision_registry(ctx: Context<ProvisionRegistry>, index: u64) -> Result<()> { controls::provision_registry(ctx,index) }
    pub fn open_position(ctx: Context<OpenPosition>, plan: PlanKindArg, gross_lamports:u64, client_nonce:u64) -> Result<()> { positions::open_position(ctx,plan,gross_lamports,client_nonce) }
    pub fn fund_series_a(ctx: Context<FundSeriesA>, gross_lamports:u64) -> Result<()> { positions::fund_series_a(ctx,gross_lamports) }
    pub fn claim_position(ctx: Context<ClaimPosition>, minimum_receive:u64, close_simple:bool) -> Result<()> { positions::claim_position(ctx,minimum_receive,close_simple) }
    pub fn discover_row(ctx:Context<DiscoverRow>,expected_cursor:u64)->Result<()> { allocation::discover_row(ctx,expected_cursor) }
    pub fn senior(ctx:Context<Senior>,expected_ticket_id:u64)->Result<()> { allocation::senior(ctx,expected_ticket_id) }
    pub fn seal_weights(ctx:Context<Seal>,expected_ticket_id:u64)->Result<()> { allocation::seal_weights(ctx,expected_ticket_id) }
    pub fn allocate_row(ctx:Context<AllocateRow>,expected_cursor:u64)->Result<()> { allocation::allocate_row(ctx,expected_cursor) }
    pub fn prune_row(ctx:Context<PruneRow>,expected_cursor:u64)->Result<()> { allocation::prune_row(ctx,expected_cursor) }
    pub fn commit(ctx:Context<Commit>,expected_ticket_id:u64)->Result<()> { allocation::commit(ctx,expected_ticket_id) }
    pub fn claim_backer(ctx:Context<ClaimBacker>,minimum_receive:u64)->Result<()> { allocation::claim_backer(ctx,minimum_receive) }
}
