//! Control-plane instructions for the native Musical Chairs program.
//!
//! This module deliberately contains no financial admission or allocation
//! logic.  It binds the singleton accounts to their canonical PDAs, performs
//! the one-time authority proof, and owns the separate operations and
//! admission-control surfaces.

use anchor_lang::prelude::*;
use anchor_lang::__private::BpfWriter;
use anchor_lang::solana_program::{
    bpf_loader_upgradeable,
    program::{invoke, invoke_signed},
    system_instruction,
};

use crate::state::{
    Admission, GameVault, Ops, Pool, RegistryPage, RentComponents, SnapshotPage, SnapshotRow,
    Ticket, ADMISSION_SEED, GAME_VAULT_SEED, IDLE, OPS_CASH_SEED, OPS_SEED, PAGE_CAPACITY,
    POOL_SEED, REGISTRY_SEED, SNAPSHOT_SEED, TICKET_SEED,
};

/// Squads v4 program used by the production authority profile.
pub const SQUADS_PROGRAM_ID: Pubkey = pubkey!("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf");
const MULTISIG_PREFIX: &[u8] = b"multisig";
const VAULT_PREFIX: &[u8] = b"vault";
const GOVERNANCE_DELAY: u32 = 172_800;
const SQUADS_MEMBER_COUNT: usize = 3;
const PERMISSION_INITIATE: u8 = 1;
const PERMISSION_VOTE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 4;

/// Genesis configuration.  `sponsor` is intentionally repeated in the args
/// and as a signer account so a fee payer cannot silently become the sponsor.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitializeArgs {
    pub sponsor: Pubkey,
    pub management_recipient: Pubkey,
    /// Immutable cluster domain (devnet and mainnet differ); stored once in
    /// `Pool.cluster_domain` and never writable again.
    pub cluster_domain: [u8; 32],
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

    validate_profile(
        &ctx.accounts.governance_multisig.to_account_info(),
        &ctx.accounts.governance.to_account_info(),
        GOVERNANCE_DELAY,
    )?;
    validate_profile(
        &ctx.accounts.guardian_multisig.to_account_info(),
        &ctx.accounts.guardian.to_account_info(),
        0,
    )?;
    require_distinct_authorities(&ctx.accounts.governance.key(), &ctx.accounts.guardian.key())?;
    validate_program_binding(
        &ctx.accounts.self_program.to_account_info(),
        &ctx.accounts.program_data.to_account_info(),
        &ctx.accounts.governance.key(),
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
    pool.protected_rent = 0;
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
    pool.governance = ctx.accounts.governance.key();
    pool.guardian_vault = ctx.accounts.guardian.key();
    pool.sponsor = args.sponsor;
    pool.management_recipient = args.management_recipient;

    ctx.accounts.admission.bump = ctx.bumps.admission;
    ctx.accounts.admission.guardian_vault = ctx.accounts.guardian.key();
    ctx.accounts.admission.paused = false;
    ctx.accounts.admission.changed_at = 0;

    ctx.accounts.game_vault.bump = ctx.bumps.game_vault;

    ctx.accounts.ops.bump = ctx.bumps.ops;
    ctx.accounts.ops.sponsor = args.sponsor;
    ctx.accounts.ops.free_balance = 0;
    ctx.accounts.ops.locked_remaining_total = 0;
    ctx.accounts.ops.unallocated_excess = ops_cash_info.lamports().saturating_sub(rent);

    ctx.accounts.ticket.bump = ctx.bumps.ticket;
    ctx.accounts.ticket.id = 0;
    ctx.accounts.ticket.phase = IDLE;

    // Keep the bump live for callers that need to construct PDA signer seeds;
    // no data is stored in the system-owned cash account itself.
    Ok(())
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
        crate::ErrorCode::InsufficientOperations
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

pub fn provision_snapshot(
    ctx: Context<ProvisionSnapshot>,
    index: u64,
) -> Result<()> {
    let expected = ctx.accounts.pool.snapshot_pages;
    require!(index <= expected, crate::ErrorCode::InvalidState);
    let page_key = canonical_page(SNAPSHOT_SEED, index)?;
    require_keys_eq!(
        ctx.accounts.snapshot_page.key(),
        page_key,
        crate::ErrorCode::WrongAccount
    );

    let (ops_cash_key, _) = Pubkey::find_program_address(&[OPS_CASH_SEED], &crate::ID);
    require_keys_eq!(ctx.accounts.ops_cash.key(), ops_cash_key, crate::ErrorCode::WrongAccount);
    require_keys_eq!(
        *ctx.accounts.ops_cash.owner,
        anchor_lang::solana_program::system_program::ID,
        crate::ErrorCode::WrongOwner
    );
    let ops = &mut ctx.accounts.ops;
    crate::sync_ops(ops, &ctx.accounts.ops_cash.to_account_info())?;

    if index == expected {
        let bump = pda_bump(&[SNAPSHOT_SEED, &index.to_le_bytes()], &page_key)?;
        crate::create_shared(
            &ctx.accounts.snapshot_page.to_account_info(),
            &ctx.accounts.ops_cash.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
            &[SNAPSHOT_SEED, &index.to_le_bytes(), &[bump]],
            SnapshotPage::SPACE,
            ops,
        )?;
        let page = SnapshotPage {
            bump,
            index,
            ticket_id: 0,
            rows: [SnapshotRow::default(); PAGE_CAPACITY],
            rent: RentComponents::default(),
        };
        serialize_account(&ctx.accounts.snapshot_page.to_account_info(), &page)?;
        ctx.accounts.pool.snapshot_pages = expected
            .checked_add(1)
            .ok_or(crate::ErrorCode::Overflow)?;
    } else {
        validate_existing_page::<SnapshotPage>(&ctx.accounts.snapshot_page, index)?;
    }
    Ok(())
}

pub fn provision_registry(
    ctx: Context<ProvisionRegistry>,
    index: u64,
) -> Result<()> {
    require!(ctx.accounts.pool.active_ticket == 0, crate::ErrorCode::AllocationPending);
    let needed = ctx
        .accounts
        .pool
        .registry_len
        .checked_div(PAGE_CAPACITY as u64)
        .ok_or(crate::ErrorCode::Overflow)?;
    require!(index <= needed, crate::ErrorCode::InvalidState);
    let page_key = canonical_page(REGISTRY_SEED, index)?;
    require_keys_eq!(
        ctx.accounts.registry_page.key(),
        page_key,
        crate::ErrorCode::WrongAccount
    );

    validate_ops_cash(&ctx.accounts.ops_cash)?;
    let ops = &mut ctx.accounts.ops;
    crate::sync_ops(ops, &ctx.accounts.ops_cash.to_account_info())?;
    if !ctx.accounts.registry_page.data_is_empty() {
        return validate_existing_page::<RegistryPage>(&ctx.accounts.registry_page, index);
    }

    let bump = pda_bump(&[REGISTRY_SEED, &index.to_le_bytes()], &page_key)?;
    crate::create_shared(
        &ctx.accounts.registry_page.to_account_info(),
        &ctx.accounts.ops_cash.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &[REGISTRY_SEED, &index.to_le_bytes(), &[bump]],
        RegistryPage::SPACE,
        ops,
    )?;
    let page = RegistryPage {
        bump,
        index,
        keys: [Pubkey::default(); PAGE_CAPACITY],
        rent: RentComponents::default(),
    };
    serialize_account(&ctx.accounts.registry_page.to_account_info(), &page)?;
    Ok(())
}

fn canonical_page(seed: &[u8], index: u64) -> Result<Pubkey> {
    Ok(Pubkey::find_program_address(&[seed, &index.to_le_bytes()], &crate::ID).0)
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

fn reject_protected_destination(destination: &Pubkey, protected: &[Pubkey]) -> Result<()> {
    require!(
        protected.iter().all(|key| key != destination),
        crate::ErrorCode::ProtectedDestination
    );
    Ok(())
}

fn validate_existing_page<T: AccountDeserialize + AnchorSerialize + Discriminator>(
    account: &UncheckedAccount<'_>,
    index: u64,
) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        crate::ID,
        crate::ErrorCode::WrongOwner
    );
    let account_data = account.try_borrow_data()?;
    let mut data: &[u8] = &account_data;
    require!(
        data.len() >= 8 && &data[..8] == T::DISCRIMINATOR,
        crate::ErrorCode::WrongAccount
    );
    let decoded = T::try_deserialize(&mut data)?;
    // Both shared page types carry `index` as their second field after bump;
    // serialize-and-compare keeps this helper generic without unsafe casts.
    let bytes = decoded.try_to_vec()?;
    require!(bytes.len() >= 9 + 8, crate::ErrorCode::InvalidState);
    let stored = u64::from_le_bytes(
        bytes[1..9]
            .try_into()
            .map_err(|_| error!(crate::ErrorCode::InvalidState))?,
    );
    require_eq!(stored, index, crate::ErrorCode::InvalidState);
    Ok(())
}

/// Decode and validate the compact Squads v4 Multisig account without taking
/// a dependency on the SDK's account layout.  The bounded member count is a
/// safety property as well as a policy property.
pub(crate) fn validate_profile(
    multisig: &AccountInfo<'_>,
    vault: &AccountInfo<'_>,
    timelock: u32,
) -> Result<()> {
    require_keys_eq!(*multisig.owner, SQUADS_PROGRAM_ID, crate::ErrorCode::WrongOwner);
    require_keys_eq!(
        *vault.owner,
        anchor_lang::solana_program::system_program::ID,
        crate::ErrorCode::WrongOwner
    );
    let account_data = multisig.try_borrow_data()?;
    let data: &[u8] = &account_data;
    require!(data.len() >= 8, crate::ErrorCode::InvalidState);
    // Anchor's `account:Multisig` discriminator, SHA-256 truncated to eight
    // bytes.  Keep this constant local so controls does not need a hash crate
    // in the SBF dependency surface.
    const MULTISIG_DISCRIMINATOR: [u8; 8] = [
        0xe0, 0x74, 0x79, 0xba, 0x44, 0xa1, 0x4f, 0xc8,
    ];
    require!(
        &data[..8] == MULTISIG_DISCRIMINATOR,
        crate::ErrorCode::WrongAccount
    );
    let mut cursor = 8usize;
    let create_key = read_pubkey(&data, &mut cursor)?;
    let config_authority = read_option_pubkey(&data, &mut cursor)?;
    require!(config_authority.is_none(), crate::ErrorCode::NotAuthorized);
    let threshold = read_u16(&data, &mut cursor)?;
    let actual_timelock = read_u32(&data, &mut cursor)?;
    let _transaction_index = read_u64(&data, &mut cursor)?;
    let _stale_transaction_index = read_u64(&data, &mut cursor)?;
    let _rent_collector = read_option_pubkey(&data, &mut cursor)?;
    let stored_bump = read_u8(&data, &mut cursor)?;
    let member_len = read_u32(&data, &mut cursor)? as usize;

    require_eq!(threshold, 2, crate::ErrorCode::NotAuthorized);
    if timelock == GOVERNANCE_DELAY {
        require!(actual_timelock >= GOVERNANCE_DELAY, crate::ErrorCode::NotAuthorized);
    } else {
        require_eq!(actual_timelock, 0, crate::ErrorCode::NotAuthorized);
    }
    require_eq!(member_len, SQUADS_MEMBER_COUNT, crate::ErrorCode::NotAuthorized);

    let (expected_multisig, expected_bump) = Pubkey::find_program_address(
        &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
        &SQUADS_PROGRAM_ID,
    );
    require_keys_eq!(expected_multisig, *multisig.key, crate::ErrorCode::WrongAccount);
    require_eq!(stored_bump, expected_bump, crate::ErrorCode::WrongAccount);
    let (expected_vault, _) = Pubkey::find_program_address(
        &[MULTISIG_PREFIX, multisig.key.as_ref(), VAULT_PREFIX, &[0]],
        &SQUADS_PROGRAM_ID,
    );
    require_keys_eq!(expected_vault, *vault.key, crate::ErrorCode::WrongAccount);

    let mut seen = [Pubkey::default(); SQUADS_MEMBER_COUNT];
    let mut has_initiate = false;
    let mut has_execute = false;
    for index in 0..SQUADS_MEMBER_COUNT {
        let key = read_pubkey(&data, &mut cursor)?;
        let mask = read_u8(&data, &mut cursor)?;
        require!(key != Pubkey::default(), crate::ErrorCode::NotAuthorized);
        require!(mask & PERMISSION_VOTE != 0, crate::ErrorCode::NotAuthorized);
        require!(mask & !(PERMISSION_INITIATE | PERMISSION_VOTE | PERMISSION_EXECUTE) == 0, crate::ErrorCode::NotAuthorized);
        require!(
            seen[..index].iter().all(|prior| *prior != key),
            crate::ErrorCode::NotAuthorized
        );
        seen[index] = key;
        has_initiate |= mask & PERMISSION_INITIATE != 0;
        has_execute |= mask & PERMISSION_EXECUTE != 0;
    }
    require!(has_initiate && has_execute, crate::ErrorCode::NotAuthorized);
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

fn read_u8(data: &[u8], cursor: &mut usize) -> Result<u8> {
    let value = *data.get(*cursor).ok_or(crate::ErrorCode::InvalidState)?;
    *cursor += 1;
    Ok(value)
}

fn read_u16(data: &[u8], cursor: &mut usize) -> Result<u16> {
    let end = cursor.checked_add(2).ok_or(crate::ErrorCode::Overflow)?;
    let value = u16::from_le_bytes(data.get(*cursor..end).ok_or(crate::ErrorCode::InvalidState)?.try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor.checked_add(4).ok_or(crate::ErrorCode::Overflow)?;
    let value = u32::from_le_bytes(data.get(*cursor..end).ok_or(crate::ErrorCode::InvalidState)?.try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_u64(data: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor.checked_add(8).ok_or(crate::ErrorCode::Overflow)?;
    let value = u64::from_le_bytes(data.get(*cursor..end).ok_or(crate::ErrorCode::InvalidState)?.try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_pubkey(data: &[u8], cursor: &mut usize) -> Result<Pubkey> {
    let end = cursor.checked_add(32).ok_or(crate::ErrorCode::Overflow)?;
    let value = Pubkey::new_from_array(data.get(*cursor..end).ok_or(crate::ErrorCode::InvalidState)?.try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_option_pubkey(data: &[u8], cursor: &mut usize) -> Result<Option<Pubkey>> {
    match read_u8(data, cursor)? {
        0 => Ok(None),
        1 => Ok(Some(read_pubkey(data, cursor)?)),
        _ => Err(crate::ErrorCode::InvalidState.into()),
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
pub struct ProvisionSnapshot<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical system-owned PDA or an already-created page.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: canonical page PDA; created and deserialized by the handler.
    #[account(mut)]
    pub snapshot_page: UncheckedAccount<'info>,
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProvisionRegistry<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical system-owned PDA; checked by the handler/helper.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: canonical page PDA; created and deserialized by the handler.
    #[account(mut)]
    pub registry_page: UncheckedAccount<'info>,
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::system_program;

    fn push_pubkey(bytes: &mut Vec<u8>, key: &Pubkey) {
        bytes.extend_from_slice(key.as_ref());
    }

    fn multisig_bytes(
        create_key: Pubkey,
        timelock: u32,
        threshold: u16,
        config_authority: bool,
        bump: u8,
        members: &[(Pubkey, u8); 3],
    ) -> Vec<u8> {
        const DISC: [u8; 8] = [0xe0, 0x74, 0x79, 0xba, 0x44, 0xa1, 0x4f, 0xc8];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&DISC);
        push_pubkey(&mut bytes, &create_key);
        bytes.push(if config_authority { 1 } else { 0 });
        if config_authority {
            bytes.extend_from_slice(&[9u8; 32]);
        }
        bytes.extend_from_slice(&threshold.to_le_bytes());
        bytes.extend_from_slice(&timelock.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.push(0); // rent_collector: None
        bytes.push(bump);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        for (key, permissions) in members {
            push_pubkey(&mut bytes, key);
            bytes.push(*permissions);
        }
        bytes
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

    fn profile_accounts(
        data: Vec<u8>,
        multisig: Pubkey,
        vault: Pubkey,
    ) -> (AccountInfo<'static>, AccountInfo<'static>) {
        (
            account(multisig, SQUADS_PROGRAM_ID, data, false, false),
            account(vault, system_program::ID, vec![], true, false),
        )
    }

    fn assert_error<T: std::fmt::Debug>(result: Result<T>, expected: &str) {
        let error = result.expect_err("validator unexpectedly accepted fixture");
        assert!(
            format!("{error:?}").contains(expected),
            "expected {expected}, got {error:?}"
        );
    }

    fn valid_profile(
        timelock: u32,
        masks: [u8; 3],
    ) -> (AccountInfo<'static>, AccountInfo<'static>, Pubkey, u8) {
        let create_key = Pubkey::new_from_array([42u8; 32]);
        let (multisig, bump) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
            &SQUADS_PROGRAM_ID,
        );
        let (vault, _) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, multisig.as_ref(), VAULT_PREFIX, &[0]],
            &SQUADS_PROGRAM_ID,
        );
        let members = [
            (Pubkey::new_from_array([1u8; 32]), masks[0]),
            (Pubkey::new_from_array([2u8; 32]), masks[1]),
            (Pubkey::new_from_array([3u8; 32]), masks[2]),
        ];
        let data = multisig_bytes(create_key, timelock, 2, false, bump, &members);
        let (multisig_account, vault_account) = profile_accounts(data, multisig, vault);
        (multisig_account, vault_account, multisig, bump)
    }

    #[test]
    fn initialize_args_carry_the_cluster_domain_and_no_close_reserve() {
        let args = InitializeArgs {
            sponsor: Pubkey::new_from_array([1; 32]),
            management_recipient: Pubkey::new_from_array([2; 32]),
            cluster_domain: [0xC3; 32],
        };
        let bytes = args.try_to_vec().unwrap();
        // sponsor + management_recipient + cluster_domain, nothing else.
        assert_eq!(bytes.len(), 32 + 32 + 32);
        assert_eq!(&bytes[64..], &[0xC3; 32]);
        assert_eq!(InitializeArgs::try_from_slice(&bytes).unwrap(), args);
    }

    #[test]
    fn accepts_canonical_governance_and_guardian_profiles() {
        let (governance, governance_vault, _, _) = valid_profile(172_800, [7, 7, 7]);
        validate_profile(&governance, &governance_vault, GOVERNANCE_DELAY).unwrap();

        let (guardian, guardian_vault, _, _) = valid_profile(0, [7, 7, 7]);
        validate_profile(&guardian, &guardian_vault, 0).unwrap();
    }

    #[test]
    fn rejects_bad_discriminator_configuration_threshold_and_delay() {
        let (valid, vault, multisig, _) = valid_profile(172_800, [7, 7, 7]);
        let mut bad_disc = valid.try_borrow_data().unwrap().to_vec();
        bad_disc[0] ^= 1;
        let (bad_disc, bad_vault) = profile_accounts(bad_disc, multisig, vault.key());
        assert_error(
            validate_profile(&bad_disc, &bad_vault, GOVERNANCE_DELAY),
            "WrongAccount",
        );

        let create_key = Pubkey::new_from_array([42u8; 32]);
        let (_, bump) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
            &SQUADS_PROGRAM_ID,
        );
        let members = [
            (Pubkey::new_from_array([1u8; 32]), 7),
            (Pubkey::new_from_array([2u8; 32]), 7),
            (Pubkey::new_from_array([3u8; 32]), 7),
        ];
        let (canonical_multisig, _) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
            &SQUADS_PROGRAM_ID,
        );
        let (canonical_vault, _) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, canonical_multisig.as_ref(), VAULT_PREFIX, &[0]],
            &SQUADS_PROGRAM_ID,
        );
        let (config, config_vault) = profile_accounts(
            multisig_bytes(create_key, 172_800, 2, true, bump, &members),
            canonical_multisig,
            canonical_vault,
        );
        assert_error(
            validate_profile(&config, &config_vault, GOVERNANCE_DELAY),
            "NotAuthorized",
        );

        let (threshold, threshold_vault) = profile_accounts(
            multisig_bytes(create_key, 172_800, 1, false, bump, &members),
            canonical_multisig,
            canonical_vault,
        );
        assert_error(
            validate_profile(&threshold, &threshold_vault, GOVERNANCE_DELAY),
            "NotAuthorized",
        );

        let (delay, delay_vault, _, _) = valid_profile(60, [7, 7, 7]);
        assert_error(
            validate_profile(&delay, &delay_vault, GOVERNANCE_DELAY),
            "NotAuthorized",
        );
    }

    #[test]
    fn rejects_member_permissions_duplicates_pda_bump_and_truncation() {
        let (no_vote, no_vote_vault, _, _) = valid_profile(172_800, [5, 7, 7]);
        assert_error(
            validate_profile(&no_vote, &no_vote_vault, GOVERNANCE_DELAY),
            "NotAuthorized",
        );

        let create_key = Pubkey::new_from_array([42u8; 32]);
        let (multisig, bump) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, MULTISIG_PREFIX, create_key.as_ref()],
            &SQUADS_PROGRAM_ID,
        );
        let (vault, _) = Pubkey::find_program_address(
            &[MULTISIG_PREFIX, multisig.as_ref(), VAULT_PREFIX, &[0]],
            &SQUADS_PROGRAM_ID,
        );
        let duplicate_members = [
            (Pubkey::new_from_array([1u8; 32]), 7),
            (Pubkey::new_from_array([1u8; 32]), 7),
            (Pubkey::new_from_array([3u8; 32]), 7),
        ];
        let (duplicate, duplicate_vault) = profile_accounts(
            multisig_bytes(create_key, 172_800, 2, false, bump, &duplicate_members),
            multisig,
            vault,
        );
        assert_error(
            validate_profile(&duplicate, &duplicate_vault, GOVERNANCE_DELAY),
            "NotAuthorized",
        );

        let (wrong_pda, wrong_pda_vault, _, _) = valid_profile(172_800, [7, 7, 7]);
        let wrong_key = Pubkey::new_from_array([99u8; 32]);
        let wrong_pda_account = account(
            wrong_key,
            SQUADS_PROGRAM_ID,
            wrong_pda.try_borrow_data().unwrap().to_vec(),
            false,
            false,
        );
        assert_error(
            validate_profile(&wrong_pda_account, &wrong_pda_vault, GOVERNANCE_DELAY),
            "WrongAccount",
        );

        let (canonical, _, canonical_key, _) = valid_profile(172_800, [7, 7, 7]);
        let mut bad_bump_data = canonical.try_borrow_data().unwrap().to_vec();
        bad_bump_data[8 + 32 + 1 + 2 + 4 + 8 + 8 + 1] ^= 1;
        let (bad_bump, bad_bump_vault) = profile_accounts(bad_bump_data, canonical_key, vault);
        assert_error(
            validate_profile(&bad_bump, &bad_bump_vault, GOVERNANCE_DELAY),
            "WrongAccount",
        );

        let (truncated, _truncated_vault, _, _) = valid_profile(172_800, [7, 7, 7]);
        let truncated_data = truncated.try_borrow_data().unwrap()[..20].to_vec();
        let (truncated, truncated_vault) = profile_accounts(truncated_data, canonical_key, vault);
        assert_error(
            validate_profile(&truncated, &truncated_vault, GOVERNANCE_DELAY),
            "InvalidState",
        );
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
}

