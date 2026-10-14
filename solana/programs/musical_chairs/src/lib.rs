//! Native financial account/instruction tranche. No deployment or complete-game claim.
#![forbid(unsafe_code)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    program::{invoke, invoke_signed},
    system_instruction, system_program,
};
pub mod state;
pub mod controls;
pub mod positions;
pub mod allocation;
pub mod claims;
pub mod sampling;
pub mod provider;
pub mod close;
pub mod error;
pub mod events;
pub use error::ErrorCode;
pub use events::*;
use controls::*;
pub use controls::{ConfigTerms, Funding, PageKind};
use positions::*;
use allocation::*;
use claims::*;
use close::*;
pub use state::*;
declare_id!("7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7");

pub fn checked_add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| error!(ErrorCode::Overflow))
}
pub fn checked_sub(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b).ok_or_else(|| error!(ErrorCode::InvalidState))
}
pub fn now() -> Result<u64> {
    u64::try_from(Clock::get()?.unix_timestamp).map_err(|_| error!(ErrorCode::BackwardsTime))
}
/// Every game-vault partition, in u128 because `funded_claims` is u128 and the
/// sum of the u64 partitions plus it can exceed u64.
pub fn game_accounted(pool: &Pool) -> Result<u128> {
    [pool.pot, pool.seed, pool.management, pool.pending_toll, pool.excess]
        .into_iter()
        .map(u128::from)
        .chain([pool.funded_claims])
        .try_fold(0u128, |sum, part| sum.checked_add(part).ok_or_else(|| error!(ErrorCode::Overflow)))
}
/// Protect current runtime rent and classify unsolicited SOL separately as a
/// floating, never-spendable residual.
/// Also refreshes `pool.protected_rent` from the live rent schedule (DR-27),
/// so the recorded floor can never go stale against a rent-schedule change.
pub fn sync_game(pool: &mut Pool, vault: &AccountInfo) -> Result<()> {
    require_keys_eq!(*vault.owner, ID, ErrorCode::WrongOwner);
    require_keys_eq!(vault.key(), Pubkey::find_program_address(&[GAME_VAULT_SEED], &ID).0, ErrorCode::WrongAccount);
    let rent_floor = Rent::get()?.minimum_balance(vault.data_len());
    let spendable = checked_sub(vault.lamports(), rent_floor)?;
    // `excess` floats: it is the residual of the spendable balance over the
    // OWNED partitions at the live floor, re-derived on every sync rather than
    // accumulated.  Nothing can spend it, so the economics are unchanged, and
    // a rent-floor rise within excess classified at a lower floor is absorbed
    // instead of refusing every instruction (M10 review ECO-3).  The owned
    // partitions stay protected: a shortfall against them is `InvalidState`.
    let owned = game_accounted(pool)?
        .checked_sub(u128::from(pool.excess))
        .ok_or_else(|| error!(ErrorCode::InvalidState))?;
    let residual = u128::from(spendable)
        .checked_sub(owned)
        .ok_or_else(|| error!(ErrorCode::InvalidState))?;
    pool.excess = u64::try_from(residual).map_err(|_| error!(ErrorCode::Overflow))?;
    pool.protected_rent = rent_floor;
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
/// Classify unsolicited OpsCash credits as `unallocated_excess` and refresh
/// `ops.protected_rent` from the live rent schedule (DR-27).
pub fn sync_ops(ops: &mut Ops, cash: &AccountInfo) -> Result<()> {
    require_keys_eq!(cash.key(), Pubkey::find_program_address(&[OPS_CASH_SEED], &ID).0, ErrorCode::WrongAccount);
    require_keys_eq!(*cash.owner, system_program::ID, ErrorCode::WrongOwner);
    require!(cash.data_is_empty() && !cash.executable, ErrorCode::WrongAccount);
    let rent_floor = Rent::get()?.minimum_balance(0);
    let available = checked_sub(cash.lamports(), rent_floor)?;
    // Same floating rule as `sync_game` (M10 review ECO-3): the residual over
    // the owned partitions (free + locked) at the live floor; a shortfall
    // against them fails as before.
    let owned = checked_add(ops.free_balance, ops.locked_remaining_total)?;
    ops.unallocated_excess = checked_sub(available, owned)?;
    ops.protected_rent = rent_floor;
    Ok(())
}

/// Shared checks before a PDA is created: the system program is real, the
/// target is the PDA the seeds (which end in the bump) describe, and it is a
/// writable, empty, system-owned account. A prefunded empty PDA passes.
fn check_create_target(target: &AccountInfo, system: &AccountInfo, seeds: &[&[u8]]) -> Result<()> {
    require_keys_eq!(system.key(), system_program::ID, ErrorCode::WrongAccount);
    require_keys_eq!(target.key(), Pubkey::create_program_address(seeds, &ID).map_err(|_| error!(ErrorCode::WrongAccount))?, ErrorCode::WrongAccount);
    require_keys_eq!(*target.owner, system_program::ID, ErrorCode::WrongOwner);
    require!(target.data_is_empty() && !target.executable && target.is_writable, ErrorCode::WrongAccount);
    Ok(())
}
/// Allocate `space` bytes and assign the target to this program; the target PDA signs.
fn allocate_and_assign<'info>(target: &AccountInfo<'info>, system: &AccountInfo<'info>, seeds: &[&[u8]], space: usize) -> Result<()> {
    invoke_signed(&system_instruction::allocate(target.key, space as u64), &[target.clone(), system.clone()], &[seeds])?;
    invoke_signed(&system_instruction::assign(target.key, &ID), &[target.clone(), system.clone()], &[seeds])?;
    Ok(())
}
/// System transfer `amount` out of OpsCash (the OpsCash PDA signs).
fn transfer_from_cash<'info>(cash: &AccountInfo<'info>, target: &AccountInfo<'info>, system: &AccountInfo<'info>, amount: u64) -> Result<()> {
    let (_, cash_bump) = Pubkey::find_program_address(&[OPS_CASH_SEED], &ID);
    let cash_bump_arr = [cash_bump];
    let cash_seeds: &[&[u8]] = &[OPS_CASH_SEED, &cash_bump_arr];
    invoke_signed(&system_instruction::transfer(cash.key, target.key, amount), &[cash.clone(), target.clone(), system.clone()], &[cash_seeds])?;
    Ok(())
}
/// `ops.total_rent_funded + debit` as checked u128 (`Overflow`).
fn rent_funded_after(total: u128, debit: u64) -> Result<u128> {
    total.checked_add(u128::from(debit)).ok_or_else(|| error!(ErrorCode::Overflow))
}
/// Account a `Free` rent debit: `free_balance -= debit` (`InsufficientOperations`
/// when short) and `total_rent_funded += debit`. All-or-nothing: on error `ops`
/// is unchanged. Caller funding never goes through here.
pub fn charge_free(ops: &mut Ops, debit: u64) -> Result<()> {
    require!(ops.free_balance >= debit, ErrorCode::InsufficientOperations);
    let rent_funded = rent_funded_after(ops.total_rent_funded, debit)?;
    ops.free_balance = checked_sub(ops.free_balance, debit)?;
    ops.total_rent_funded = rent_funded;
    Ok(())
}
/// Split a rent debit across a close's locked partitions: sponsor partition
/// first, then owner. `LockNotPermitted` when the two together fall short.
pub fn split_locked(sponsor_remaining: u64, owner_remaining: u64, rent: u64) -> Result<(u64, u64)> {
    let sponsor_part = rent.min(sponsor_remaining);
    let owner_part = rent - sponsor_part;
    require!(owner_part <= owner_remaining, ErrorCode::LockNotPermitted);
    Ok((sponsor_part, owner_part))
}
/// Account a `LockedClose` rent debit (spec §8.3): both partitions and
/// `ops.locked_remaining_total` fall by exactly the lamports that leave
/// OpsCash (sponsor first), `close.spent.candidate_rent_s/o` records the split
/// and `total_rent_funded += debit`. `free_balance` is never touched.
/// All-or-nothing: on error neither `ops` nor `close` is changed.
pub fn charge_locked(ops: &mut Ops, close: &mut CloseRecord, debit: u64) -> Result<(u64, u64)> {
    let (sponsor_part, owner_part) = split_locked(close.locked_sponsor_remaining, close.locked_owner_remaining, debit)?;
    let sponsor_left = checked_sub(close.locked_sponsor_remaining, sponsor_part)?;
    let owner_left = checked_sub(close.locked_owner_remaining, owner_part)?;
    let locked_total = checked_sub(ops.locked_remaining_total, debit)?;
    let spent_s = checked_add(close.spent.candidate_rent_s, sponsor_part)?;
    let spent_o = checked_add(close.spent.candidate_rent_o, owner_part)?;
    let rent_funded = rent_funded_after(ops.total_rent_funded, debit)?;
    close.locked_sponsor_remaining = sponsor_left;
    close.locked_owner_remaining = owner_left;
    ops.locked_remaining_total = locked_total;
    close.spent.candidate_rent_s = spent_s;
    close.spent.candidate_rent_o = spent_o;
    ops.total_rent_funded = rent_funded;
    Ok((sponsor_part, owner_part))
}
/// Shared pages are sponsor-funded. Prefunded empty PDAs cannot block provisioning.
/// Canonical target seeds include its bump; both cash and target sign the CPI.
/// Rent is read from `Rent::get()` (never a constant). Returns the lamports
/// actually debited from `ops.free_balance`, i.e. the rent minus whatever the
/// target already held, which is the page's `ops` rent component. The debit is
/// also added to `ops.total_rent_funded`.
pub fn create_shared<'info>(target: &AccountInfo<'info>, cash: &AccountInfo<'info>, system: &AccountInfo<'info>, seeds: &[&[u8]], space: usize, ops: &mut Ops) -> Result<u64> {
    sync_ops(ops, cash)?;
    check_create_target(target, system, seeds)?;
    let rent = Rent::get()?.minimum_balance(space);
    let debit = rent.saturating_sub(target.lamports());
    charge_free(ops, debit)?;
    if debit > 0 {
        transfer_from_cash(cash, target, system, debit)?;
    }
    allocate_and_assign(target, system, seeds, space)?;
    Ok(debit)
}
/// Like [`create_shared`], but paid from the close's locked reserve instead of
/// `free_balance` (spec §8.3): see [`charge_locked`] for the accounting.
/// Returns `(sponsor part, owner part)`.
/// For an unfunded target the debit is the whole rent; a prefunded target
/// lowers the debit (and every figure derived from it) by what it already holds,
/// so the partitions always equal the unspent cash that remains in OpsCash.
pub fn create_shared_locked<'info>(target: &AccountInfo<'info>, cash: &AccountInfo<'info>, system: &AccountInfo<'info>, seeds: &[&[u8]], space: usize, ops: &mut Ops, close: &mut CloseRecord) -> Result<(u64, u64)> {
    sync_ops(ops, cash)?;
    check_create_target(target, system, seeds)?;
    let rent = Rent::get()?.minimum_balance(space);
    let debit = rent.saturating_sub(target.lamports());
    let parts = charge_locked(ops, close, debit)?;
    if debit > 0 {
        transfer_from_cash(cash, target, system, debit)?;
    }
    allocate_and_assign(target, system, seeds, space)?;
    Ok(parts)
}
/// Create a shared page paid by the transaction's `payer` (the `Caller`
/// funding mode): the payer signs the rent transfer, the PDA signs allocate and
/// assign. Operations cash is not involved. Returns the lamports the payer
/// actually transferred (the rent minus anything the target already held),
/// which is the page's `owner` rent component.
pub fn create_caller_funded<'info>(target: &AccountInfo<'info>, payer: &AccountInfo<'info>, system: &AccountInfo<'info>, seeds: &[&[u8]], space: usize) -> Result<u64> {
    check_create_target(target, system, seeds)?;
    require!(payer.is_signer && payer.is_writable, ErrorCode::NotAuthorized);
    let rent = Rent::get()?.minimum_balance(space);
    let debit = rent.saturating_sub(target.lamports());
    if debit > 0 {
        invoke(&system_instruction::transfer(payer.key, target.key, debit), &[payer.clone(), target.clone(), system.clone()])?;
    }
    allocate_and_assign(target, system, seeds, space)?;
    Ok(debit)
}

#[program]
pub mod musical_chairs_program {
    use super::*;
    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> { controls::initialize(ctx,args) }
    pub fn set_pause(ctx: Context<Pause>, paused: bool) -> Result<()> { controls::set_pause(ctx,paused) }
    pub fn fund_operations(ctx: Context<FundOperations>, amount: u64) -> Result<()> { controls::fund_operations(ctx,amount) }
    pub fn withdraw_operations(ctx: Context<WithdrawOperations>, amount: u64) -> Result<()> { controls::withdraw_operations(ctx,amount) }
    pub fn withdraw_management(ctx: Context<WithdrawManagement>, amount: u64) -> Result<()> { controls::withdraw_management(ctx,amount) }
    pub fn stage_config_version(ctx: Context<StageConfigVersion>, version: u32, terms: ConfigTerms) -> Result<()> { controls::stage_config_version(ctx,version,terms) }
    pub fn activate_config_version(ctx: Context<ActivateConfigVersion>, version: u32, expected_current: u32) -> Result<()> { controls::activate_config_version(ctx,version,expected_current) }
    pub fn rotate_recipient(ctx: Context<RotateRecipient>, kind: u8, new_recipient: Pubkey) -> Result<()> { controls::rotate_recipient(ctx,kind,new_recipient) }
    pub fn rotate_guardian(ctx: Context<RotateGuardian>) -> Result<()> { controls::rotate_guardian(ctx) }
    pub fn provision_page(ctx: Context<ProvisionPage>, kind: PageKind, index: u64, funding: Funding) -> Result<()> { controls::provision_page(ctx,kind,index,funding) }
    pub fn provision_close_record(ctx: Context<ProvisionCloseRecord>, close_id: u64, funding: Funding) -> Result<()> { controls::provision_close_record(ctx,close_id,funding) }
    pub fn close_shared_account(ctx: Context<CloseSharedAccount>, kind: PageKind) -> Result<()> { controls::close_shared_account(ctx,kind) }
    pub fn reconcile_ops_cash(ctx: Context<ReconcileOpsCash>) -> Result<()> { controls::reconcile_ops_cash(ctx) }
    pub fn open_position(ctx: Context<OpenPosition>, plan: PlanKindArg, gross: u64, client_nonce: u64, expected_round_id: u64, expected_config: u32) -> Result<()> { positions::open_position(ctx,plan,gross,client_nonce,expected_round_id,expected_config) }
    pub fn fund_series_a(ctx: Context<FundSeriesA>, gross: u64, expected_round_id: u64) -> Result<()> { positions::fund_series_a(ctx,gross,expected_round_id) }
    pub fn withdraw_simple(ctx: Context<Payout>, minimum_receive: u64, max_work_contribution: u64) -> Result<()> { positions::withdraw_simple(ctx,minimum_receive,max_work_contribution) }
    pub fn settle_compounding(ctx: Context<Payout>, minimum_receive: u64, max_work_contribution: u64) -> Result<()> { positions::settle_compounding(ctx,minimum_receive,max_work_contribution) }
    pub fn close_simple_position(ctx: Context<CloseSimplePosition>) -> Result<()> { positions::close_simple_position(ctx) }
    pub fn discover_rows(ctx: Context<DiscoverRows>, expected_ticket_id: u64, expected_cursor: u64, rows: u8) -> Result<()> { allocation::discover_rows(ctx,expected_ticket_id,expected_cursor,rows) }
    pub fn apply_senior(ctx: Context<ApplySenior>, expected_ticket_id: u64) -> Result<()> { allocation::apply_senior(ctx,expected_ticket_id) }
    pub fn seal_weights(ctx: Context<SealWeights>, expected_ticket_id: u64) -> Result<()> { allocation::seal_weights(ctx,expected_ticket_id) }
    pub fn allocate_rows(ctx: Context<AllocateRows>, expected_ticket_id: u64, expected_cursor: u64, rows: u8) -> Result<()> { allocation::allocate_rows(ctx,expected_ticket_id,expected_cursor,rows) }
    pub fn prune_rows(ctx: Context<PruneRows>, expected_ticket_id: u64, expected_cursor: u64, rows: u8) -> Result<()> { allocation::prune_rows(ctx,expected_ticket_id,expected_cursor,rows) }
    pub fn commit_ticket(ctx: Context<CommitTicket>, expected_ticket_id: u64) -> Result<()> { allocation::commit_ticket(ctx,expected_ticket_id) }
    pub fn claim_backer(ctx: Context<ClaimBacker>) -> Result<()> { claims::claim_backer(ctx) }
    pub fn build_candidates<'info>(ctx: Context<'_, '_, '_, 'info, BuildCandidates<'info>>, rows: u8) -> Result<()> { close::build_candidates(ctx,rows) }
    pub fn request_round_draw(ctx: Context<RequestRoundDraw>) -> Result<()> { close::request_round_draw(ctx) }
    pub fn capture_round_draw(ctx: Context<CaptureRoundDraw>) -> Result<()> { close::capture_round_draw(ctx) }
    pub fn apply_promotion(ctx: Context<ApplyPromotion>) -> Result<()> { close::apply_promotion(ctx) }
    pub fn expire_promotion(ctx: Context<ExpirePromotion>) -> Result<()> { close::expire_promotion(ctx) }
    pub fn finalize_round(ctx: Context<FinalizeRound>) -> Result<()> { close::finalize_round(ctx) }
    pub fn increase_provider_budget(ctx: Context<IncreaseProviderBudget>, extra: u64) -> Result<()> { close::increase_provider_budget(ctx,extra) }
    pub fn observe_request_fulfilled(ctx: Context<ObserveRequestFulfilled>) -> Result<()> { close::observe_request_fulfilled(ctx) }
    pub fn withdraw_provider_return(ctx: Context<WithdrawProviderReturn>, amount: u64) -> Result<()> { close::withdraw_provider_return(ctx,amount) }
    pub fn reconcile_ops_cash_with_client(ctx: Context<ReconcileOpsCashWithClient>) -> Result<()> { close::reconcile_ops_cash_with_client(ctx) }
    pub fn deliver_owner_refund(ctx: Context<DeliverOwnerRefund>) -> Result<()> { close::deliver_owner_refund(ctx) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_accounted_sums_every_partition_in_u128() {
        let pool = Pool {
            pot: 1,
            seed: 2,
            management: 3,
            pending_toll: 4,
            excess: 5,
            funded_claims: 6,
            ..Pool::default()
        };
        assert_eq!(game_accounted(&pool).unwrap(), 21);
    }

    #[test]
    fn game_accounted_exceeds_u64_without_overflowing() {
        let pool = Pool {
            pot: u64::MAX,
            seed: u64::MAX,
            funded_claims: u128::from(u64::MAX) * 4,
            ..Pool::default()
        };
        assert_eq!(game_accounted(&pool).unwrap(), u128::from(u64::MAX) * 6);
    }

    #[test]
    fn game_accounted_overflows_only_past_u128() {
        let pool = Pool { pot: 1, funded_claims: u128::MAX, ..Pool::default() };
        assert_eq!(game_accounted(&pool).unwrap_err(), ErrorCode::Overflow.into());
        let pool = Pool { funded_claims: u128::MAX, ..Pool::default() };
        assert_eq!(game_accounted(&pool).unwrap(), u128::MAX);
    }
}
