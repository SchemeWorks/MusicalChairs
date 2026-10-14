//! Production allocation ABI and pure accounting boundaries.
//!
//! The allocation workspace is deliberately a singleton `[ticket]`.  A
//! caller supplies one capture/allocate/prune row per instruction, while
//! pages remain transport units (capacity eight) rather than an economic
//! population limit.  The handlers are RED-gated in this tranche; the pure
//! helpers below pin the arithmetic and cursor rules before account mutation
//! is enabled by the runtime worker.

use anchor_lang::prelude::*;
use ethnum::U256;

use crate::state::{
    Backer, GameVault, Pool, RegistryPage, SnapshotPage, SnapshotRow, Ticket, BACKER_SEED,
    PAGE_CAPACITY, POOL_SEED, REGISTRY_SEED, SNAPSHOT_SEED, TICKET_SEED,
};

/// Series identifiers are PDA inputs and therefore part of the stable ABI.
pub const SERIES_A: u8 = 0;
pub const SERIES_B: u8 = 1;

/// The initial transport ABI is one row per instruction.  These constants
/// describe storage pages only; there is no economic eight-row cap.
pub const ALLOCATION_PAGE_CAPACITY: usize = PAGE_CAPACITY;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedCursor {
    pub expected_cursor: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedTicket {
    pub expected_ticket_id: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimBackerArgs {
    pub minimum_receive: u64,
}

/// The accepted tier fold. `seed` includes the initial toll half, tier floor
/// remainder, and any missing-tier allocation. Allocation-floor residue is
/// added later by `add_allocation_residue`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationSlices {
    pub seed: u64,
    pub senior: u64,
    pub a: u64,
    pub b: u64,
}

/// Split a toll using 35/45/20 of its Backer half, folding a missing tier
/// before credit.  The exact integer remainder is seed credit.  This pure
/// function is also the host/runtime golden-vector seam.
pub fn fold_slices(toll: u64, has_a: bool, has_b: bool) -> Option<AllocationSlices> {
    let initial_seed = toll / 2;
    let backer_half = toll.checked_sub(initial_seed)?;
    // Percent multiplication is deliberately U256: the persisted cash
    // partition is u64, but `u64::MAX * 4_500` is wider than u64.
    let senior = (U256::from(backer_half) * U256::from(3_500u64)
        / U256::from(10_000u64))
    .as_u64();
    let a = (U256::from(backer_half) * U256::from(4_500u64) / U256::from(10_000u64)).as_u64();
    let b = (U256::from(backer_half) * U256::from(2_000u64) / U256::from(10_000u64)).as_u64();
    let rounded = senior.checked_add(a)?.checked_add(b)?;
    let mut result = AllocationSlices {
        seed: initial_seed.checked_add(backer_half.checked_sub(rounded)?)?,
        senior,
        a,
        b,
    };

    match (has_a, has_b) {
        (true, true) => {}
        (true, false) => {
            result.a = result.a.checked_add(result.b)?;
            result.b = 0;
        }
        (false, true) => {
            result.seed = result.seed.checked_add(result.senior)?;
            result.b = result.b.checked_add(result.a)?;
            result.a = 0;
            result.senior = 0;
        }
        (false, false) => {
            result.seed = result
                .seed
                .checked_add(result.senior)?
                .checked_add(result.a)?
                .checked_add(result.b)?;
            result.senior = 0;
            result.a = 0;
            result.b = 0;
        }
    }

    Some(result)
}

/// Count captured rows and sum Series-A weights without narrowing the
/// denominator. Discovery calls this once per supplied row; there is no
/// scan, fixed N limit, or page-capacity assumption here.
pub fn add_discovered_row(
    a_count: u64,
    b_count: u64,
    a_denominator: U256,
    row: &SnapshotRow,
) -> Option<(u64, u64, U256)> {
    match row.series {
        SERIES_A => Some((
            a_count.checked_add(1)?,
            b_count,
            a_denominator.checked_add(U256::from(row.remaining))?,
        )),
        SERIES_B => Some((a_count, b_count.checked_add(1)?, a_denominator)),
        _ => None,
    }
}

/// Add only the two allocation-phase floors' undistributed residue to seed.
/// The senior credit is excluded from `a_allocated`; it was already credited
/// in Senior and is therefore not double-counted here.
pub fn add_allocation_residue(
    seed: u64,
    a_slice: u64,
    b_slice: u64,
    a_allocated: u64,
    b_allocated: u64,
) -> Option<u64> {
    let a_residue = a_slice.checked_sub(a_allocated)?;
    let b_residue = b_slice.checked_sub(b_allocated)?;
    seed.checked_add(a_residue)?.checked_add(b_residue)
}

/// Decode the fixed little-endian denominator persisted on `Ticket`.
pub fn decode_a_denominator(bytes: [u8; 32]) -> U256 {
    U256::from_le_bytes(bytes)
}

/// Encode a weighted denominator without truncating high words.  The caller
/// must persist the resulting bytes unchanged in `Ticket.a_remaining`.
pub fn encode_a_denominator(value: U256) -> [u8; 32] {
    value.to_le_bytes()
}

/// Compute floor(`slice * weight / denominator`) in U256, then require that
/// the result fits the lamport cash partition.  A zero denominator is a
/// sealed-state error and returns `None` rather than silently paying zero.
pub fn floor_proportional(slice: u64, weight: u128, denominator: U256) -> Option<u64> {
    if denominator == U256::from(0u8) {
        return None;
    }
    let whole = U256::from(slice)
        .checked_mul(U256::from(weight))?
        / denominator;
    if whole > U256::from(u64::MAX) {
        return None;
    }
    Some(whole.as_u64())
}

/// Senior is charged against the oldest captured Series-A row only.  The
/// returned tuple is `(credit, overshoot_seed, post_senior_weight)` and
/// makes the cap explicit so a capped senior never changes another row's
/// lifetime accounting.
pub fn cap_senior(senior_slice: u64, remaining: u128) -> Option<(u64, u64, u128)> {
    let cap = remaining.min(u64::MAX as u128);
    let credit = senior_slice.min(cap as u64);
    let overshoot = senior_slice.checked_sub(credit)?;
    let post_weight = remaining.checked_sub(credit as u128)?;
    Some((credit, overshoot, post_weight))
}

/// The descending prune cursor is exclusive at zero: a captured length of
/// `n` starts at `n` and the first accepted row has expected cursor `n - 1`.
pub const fn prune_slot_from_cursor(cursor: u64, expected_cursor: u64) -> Option<u64> {
    if cursor == 0 || expected_cursor != cursor - 1 {
        return None;
    }
    Some(expected_cursor)
}

/// A row is eligible for the open-registry removal pass only at the strict
/// cutoff.  Equality at 10 remains in history but is no longer active.
pub const fn prune_eligible(remaining: u128) -> bool {
    remaining <= 10
}

/// Deterministic oldest-A ordering: first funding sequence, then key bytes,
/// then captured slot.  The final slot tie-break is defensive because a
/// valid dense registry cannot contain the same active key twice.
pub fn oldest_a_key(
    current: Option<(Pubkey, u64, u64)>,
    candidate: (Pubkey, u64, u64),
) -> Option<(Pubkey, u64, u64)> {
    match current {
        None => Some(candidate),
        Some(existing) => {
            let candidate_order = (candidate.1, candidate.0.to_bytes(), candidate.2);
            let existing_order = (existing.1, existing.0.to_bytes(), existing.2);
            if candidate_order < existing_order {
                Some(candidate)
            } else {
                Some(existing)
            }
        }
    }
}

/// Canonical page index for a dense registry slot.
pub const fn registry_page_index(slot: u64) -> u64 {
    slot / PAGE_CAPACITY as u64
}

/// Canonical page index for a captured slot.
pub const fn snapshot_page_index(slot: u64) -> u64 {
    slot / PAGE_CAPACITY as u64
}

/// Discovery row.  Both page accounts are preprovisioned shared accounts;
/// neither is initialized by a financial instruction.
#[derive(Accounts)]
pub struct DiscoverRow<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
    #[account(
        mut,
        seeds = [REGISTRY_SEED, &registry_page.index.to_le_bytes()],
        bump = registry_page.bump,
    )]
    pub registry_page: Account<'info, RegistryPage>,
    #[account(
        mut,
        seeds = [BACKER_SEED, &[backer.series], backer.owner.as_ref()],
        bump = backer.bump,
    )]
    pub backer: Account<'info, Backer>,
    #[account(
        mut,
        seeds = [SNAPSHOT_SEED, &snapshot_page.index.to_le_bytes()],
        bump = snapshot_page.bump,
    )]
    pub snapshot_page: Account<'info, SnapshotPage>,
}

/// Senior credit.  No-A tickets pass the canonical default key and are
/// rejected or folded by the eventual handler before any account mutation.
#[derive(Accounts)]
pub struct Senior<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
    /// CHECK: the handler verifies the canonical Backer PDA, series A,
    /// active membership, captured slot, owner binding, and ticket oldest key.
    #[account(mut)]
    pub oldest_backer: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct Seal<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
}

/// One captured row is allocated per instruction.  Snapshot pages are
/// immutable after discovery; the handler checks the ticket tag and row key
/// before crediting the canonical Backer PDA.
#[derive(Accounts)]
pub struct AllocateRow<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
    #[account(
        seeds = [SNAPSHOT_SEED, &snapshot_page.index.to_le_bytes()],
        bump = snapshot_page.bump,
    )]
    pub snapshot_page: Account<'info, SnapshotPage>,
    #[account(
        mut,
        seeds = [BACKER_SEED, &[backer.series], backer.owner.as_ref()],
        bump = backer.bump,
    )]
    pub backer: Account<'info, Backer>,
}

/// Descending prune uses unchecked page/row accounts intentionally.  The
/// handler must decode, validate, and write each page once because the
/// original and current-last page may alias, as may both Backer accounts on
/// the final row.  Two typed Anchor accounts would otherwise serialize stale
/// copies in account-exit order.
#[derive(Accounts)]
pub struct PruneRow<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
    /// CHECK: decoded as SnapshotPage and checked against ticket id/index.
    #[account(mut)]
    pub snapshot_page: UncheckedAccount<'info>,
    /// CHECK: decoded as the original active Backer and checked against the
    /// captured key/series/slot before mutation.
    #[account(mut)]
    pub backer: UncheckedAccount<'info>,
    /// CHECK: decoded as RegistryPage, canonical PDA and current index.
    #[account(mut)]
    pub registry_page: UncheckedAccount<'info>,
    /// CHECK: decoded as the current last RegistryPage. It may alias the
    /// original page and is written through one alias-safe codec.
    #[account(mut)]
    pub last_registry_page: UncheckedAccount<'info>,
    /// CHECK: decoded as moved Backer; it may alias `backer` on the last row.
    #[account(mut)]
    pub moved_backer: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct Commit<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Account<'info, Ticket>,
}

/// A claim authenticates the owner signer against the canonical Backer PDA.
/// GameVault is the only spendable custody source; `destination` remains
/// unchecked so the handler can reject all protected aliases explicitly.
#[derive(Accounts)]
pub struct ClaimBacker<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [crate::state::GAME_VAULT_SEED], bump = game_vault.bump)]
    pub game_vault: Account<'info, GameVault>,
    pub owner_signer: Signer<'info>,
    #[account(
        mut,
        seeds = [BACKER_SEED, &[backer.series], owner_signer.key().as_ref()],
        bump = backer.bump,
        constraint = backer.owner == owner_signer.key(),
    )]
    pub backer: Account<'info, Backer>,
    /// CHECK: handler rejects protected aliases and validates system ownership.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
}

/// Initial ABI handlers.  These names and account layouts are frozen before
/// enabling financial mutation.  They intentionally return NotImplemented
/// until the RED runtime tests capture atomic cursor, PDA, conservation,
/// alias, and custody failures.
pub fn discover_row(_ctx: Context<DiscoverRow>, _expected_cursor: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn senior(_ctx: Context<Senior>, _expected_ticket_id: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn seal_weights(_ctx: Context<Seal>, _expected_ticket_id: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn allocate_row(_ctx: Context<AllocateRow>, _expected_cursor: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn prune_row(_ctx: Context<PruneRow>, _expected_cursor: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn commit(_ctx: Context<Commit>, _expected_ticket_id: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

pub fn claim_backer(_ctx: Context<ClaimBacker>, _minimum_receive: u64) -> Result<()> {
    Err(crate::ErrorCode::NotImplemented.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_full_tier_split_matches_host() {
        assert_eq!(
            fold_slices(10_000, true, true),
            Some(AllocationSlices { seed: 5_000, senior: 1_750, a: 2_250, b: 1_000 })
        );
    }

    #[test]
    fn missing_b_folds_equal_slice_into_a() {
        let split = fold_slices(10_000, true, false).unwrap();
        assert_eq!(split.seed, 5_000);
        assert_eq!(split.senior, 1_750);
        assert_eq!(split.a, 3_250);
        assert_eq!(split.b, 0);
    }

    #[test]
    fn missing_a_folds_senior_into_a_and_seed() {
        let split = fold_slices(10_000, false, true).unwrap();
        assert_eq!(split.seed, 6_750);
        assert_eq!(split.senior, 0);
        assert_eq!(split.a, 0);
        assert_eq!(split.b, 3_250);
    }

    #[test]
    fn no_backers_makes_all_toll_seed() {
        let split = fold_slices(10_000, false, false).unwrap();
        assert_eq!(split, AllocationSlices { seed: 10_000, ..AllocationSlices::default() });
    }

    #[test]
    fn discovered_counts_and_u256_weight_are_row_local() {
        let a = SnapshotRow { series: SERIES_A, remaining: u128::MAX, ..SnapshotRow::default() };
        let b = SnapshotRow { series: SERIES_B, remaining: 77, ..SnapshotRow::default() };
        let (a_count, b_count, denominator) =
            add_discovered_row(0, 0, U256::from(0u8), &a).unwrap();
        let (a_count, b_count, denominator) =
            add_discovered_row(a_count, b_count, denominator, &b).unwrap();
        assert_eq!((a_count, b_count), (1, 1));
        assert_eq!(denominator, U256::from(u128::MAX));
    }

    #[test]
    fn allocation_floor_residue_excludes_senior_credit() {
        assert_eq!(add_allocation_residue(5_000, 2_250, 1_000, 2_249, 999), Some(5_002));
        assert_eq!(add_allocation_residue(5_000, 2_250, 1_000, 2_251, 999), None);
    }

    #[test]
    fn u256_denominator_round_trip_and_floor() {
        let original = U256::from(u128::MAX) + U256::from(9u8);
        assert_eq!(decode_a_denominator(encode_a_denominator(original)), original);
        assert_eq!(floor_proportional(4_500, 4_000, U256::from(10_000u64)), Some(1_800));
    }

    #[test]
    fn senior_cap_turns_overshoot_into_seed() {
        assert_eq!(cap_senior(100, 11), Some((11, 89, 0)));
        assert_eq!(cap_senior(100, 10_000), Some((100, 0, 9_900)));
    }

    #[test]
    fn reverse_prune_cursor_cannot_skip_or_repeat() {
        assert_eq!(prune_slot_from_cursor(4, 3), Some(3));
        assert_eq!(prune_slot_from_cursor(3, 2), Some(2));
        assert_eq!(prune_slot_from_cursor(1, 0), Some(0));
        assert_eq!(prune_slot_from_cursor(0, 0), None);
        assert_eq!(prune_slot_from_cursor(4, 2), None);
    }

    #[test]
    fn oldest_a_uses_sequence_then_key_then_slot() {
        let first = Pubkey::new_from_array([2; 32]);
        let second = Pubkey::new_from_array([1; 32]);
        assert_eq!(oldest_a_key(Some((first, 7, 1)), (second, 7, 2)), Some((second, 7, 2)));
        assert_eq!(oldest_a_key(Some((first, 7, 1)), (second, 8, 0)), Some((first, 7, 1)));
    }

    #[test]
    fn page_mapping_is_storage_only() {
        assert_eq!(registry_page_index(0), 0);
        assert_eq!(registry_page_index(7), 0);
        assert_eq!(registry_page_index(8), 1);
        assert_eq!(snapshot_page_index(16), 2);
    }
}
