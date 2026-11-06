//! The bounded allocation ticket: six phase instructions that walk the dense
//! Backer registry for one escrowed toll, plus the pure rules behind them.
//!
//! The workspace is the singleton `["ticket"]`.  `withdraw_simple` (and
//! `settle_compounding`) start a ticket in `DISCOVER`; the instructions below
//! advance it through `SENIOR`, `SEAL`, `ALLOCATE` and `PRUNE` to `COMMIT`, and
//! `commit_ticket` retires it to `IDLE`.  Row phases take `rows` (1..=8) rows
//! per call so a keeper chooses the batch size its transaction can carry;
//! pages are transport units of eight, never an economic population limit.
//!
//! # Single cursor, reused per phase
//!
//! `Ticket` is a frozen schema with one `cursor`.  It is reused sequentially:
//! Discover counts it up `0..captured_len`; `seal_weights` resets it to `0`
//! and Allocate counts it up again; the final Allocate batch parks it at
//! `captured_len` and Prune counts it down to `0` (exclusive upper bound, so a
//! batch of `rows` visits slots `cursor-1, cursor-2, ...`).  Every row phase
//! therefore passes `expected_cursor == ticket.cursor`.
//!
//! # Pools are constants; the floor residue is found by difference
//!
//! After Discover, `a_slice` and `b_slice` are the fixed A and B pools and
//! `seed_credit` collects every lamport whose destination is already known:
//! the initial seed half and tier remainder, a missing tier's fold, the senior
//! overshoot, a dead A pool (sealed denominator zero), the B floor residue
//! (moved at seal so every B share is the same constant), and each row's cap
//! overshoot as it is paid.  The only remainder left unattributed is the A
//! floor-division residue `a_slice - sum(floor(a_slice * w_i / D))`, which is
//! below the open A row count.  The last Allocate batch finds it as
//! `toll - credited - seed_credit`, requires it below `max(a_count, 1)` (any
//! larger gap is a lost lamport and fails the ticket) and adds it to seed, so
//! `credited + seed_credit == toll` holds by construction and the per-row
//! shares keep using the full pool (a shrinking balance would change them).
//!
//! # Closed rows
//!
//! A row is "open" while more than ten lamports of entitlement remain.  Every
//! row is snapshotted, but only open rows are counted and paid.  Prune removes
//! a row captured closed, and a row this ticket's own credits just capped when
//! the keeper supplies its real accounts (see [`prune_decision`]).  A ticket
//! with no open row at all sends the whole toll to seed and goes straight from
//! Discover to `COMMIT`; any closed rows it leaves are pruned by the next
//! ticket that has an open row.
//!
//! Every handler is a thin account shell around a pure `*_batch` / `*_step`
//! function that works on decoded structs and `AccountInfo` slices, so the
//! host unit tests drive the same code the program runs.
//!
//! # The inline path (registry of at most `INLINE_THRESHOLD` rows)
//!
//! For a small registry a payout may run the whole ticket in its own
//! instruction ([`run_inline_ticket`], opt-in by remaining accounts, non-closing
//! payouts only).  It is not a second implementation: it drives
//! [`discover_batch`], [`senior_step`], [`seal_step`], [`allocate_batch`] and
//! [`commit_step`] unchanged, and its Prune pass shares [`prune_decision`] and
//! [`retire_row`] with [`prune_batch`].  The result is the state the six chunk
//! instructions leave (the unit tests compare account bytes, ticket and pool),
//! with the keeper's one free choice (prune a row this ticket's own credits just
//! capped) fixed to "prune it".

use anchor_lang::__private::BpfWriter;
use anchor_lang::prelude::*;
use ethnum::U256;
use musical_chairs_economics::backers as econ;

use crate::events::{TicketCommitted, SCHEMA_VERSION};
use crate::state::{
    Backer, CloseRecord, Pool, RegistryPage, SnapshotPage, SnapshotRow, Ticket, ALLOCATE,
    BACKER_SEED, CLOSE_SEED, COMMIT, DISCOVER, IDLE, INLINE_THRESHOLD, PAGE_CAPACITY, POOL_SEED,
    PRUNE, REGISTRY_SEED, SEAL, SENIOR, SNAPSHOT_SEED, TICKET_SEED,
};
use crate::ErrorCode as Code;

/// Series identifiers are PDA inputs and therefore part of the stable ABI.
pub const SERIES_A: u8 = 0;
pub const SERIES_B: u8 = 1;

/// Storage pages hold eight slots.  This describes a transport unit only;
/// there is no economic eight-row cap on the registry.
pub const ALLOCATION_PAGE_CAPACITY: usize = PAGE_CAPACITY;

/// Most rows one phase instruction accepts (one page's worth of slots).
pub const MAX_BATCH_ROWS: u8 = PAGE_CAPACITY as u8;
/// Remaining accounts per Discover row: `[registry_page, snapshot_page, backer]`.
pub const DISCOVER_ACCOUNTS_PER_ROW: usize = 3;
/// Remaining accounts per Allocate row: `[snapshot_page, backer]`.
pub const ALLOCATE_ACCOUNTS_PER_ROW: usize = 2;
/// Remaining accounts per Prune row:
/// `[snapshot_page, backer_pruned, registry_page_pruned, registry_page_last, backer_moved]`.
pub const PRUNE_ACCOUNTS_PER_ROW: usize = 5;
/// The placeholder a keeper passes for accounts a skipped Prune row does not
/// need: the program's own id.  It is never read or written.
pub const PRUNE_PLACEHOLDER: Pubkey = crate::ID;

/// The accepted 35/45/20 split of the Backer half, in basis points of it.
pub const DEFAULT_TIER_BPS: (u16, u16, u16) = (3_500, 4_500, 2_000);

/// The accepted tier fold. `seed` includes the initial toll half, tier floor
/// remainder, and any missing-tier allocation. The allocation-floor residue
/// joins it only when Allocate closes (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationSlices {
    pub seed: u64,
    pub senior: u64,
    pub a: u64,
    pub b: u64,
}

/// Split a toll using the accepted 35/45/20 of its Backer half, folding a
/// missing tier before credit.  The exact integer remainder is seed credit.
/// This pure function is also the host/runtime golden-vector seam.
pub fn fold_slices(toll: u64, has_a: bool, has_b: bool) -> Option<AllocationSlices> {
    fold_slices_with_bps(toll, DEFAULT_TIER_BPS, has_a, has_b)
}

/// [`fold_slices`] with the `(senior, a, b)` basis points a ticket pinned from
/// its `ConfigVersion` (each a share of the Backer half).  `None` when the
/// three shares exceed the whole half or a width overflows.
pub fn fold_slices_with_bps(
    toll: u64,
    bps: (u16, u16, u16),
    has_a: bool,
    has_b: bool,
) -> Option<AllocationSlices> {
    let initial_seed = toll / 2;
    let backer_half = toll.checked_sub(initial_seed)?;
    // Percent multiplication is deliberately U256: the persisted cash
    // partition is u64, but `u64::MAX * 4_500` is wider than u64.
    let tier = |tier_bps: u16| -> Option<u64> {
        let share = U256::from(backer_half) * U256::from(tier_bps) / U256::from(10_000u64);
        u64::try_from(share).ok()
    };
    let senior = tier(bps.0)?;
    let a = tier(bps.1)?;
    let b = tier(bps.2)?;
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

/// Count one captured row and add its Series-A weight without narrowing the
/// denominator.  The handlers drive the same fold through [`Tally`] (open rows
/// only); this plain-row form is kept as the reference the unit tests pin.
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

/// Reference form of the allocation-floor residue from explicit per-pool
/// allocated totals.  The handlers do not keep per-pool totals; they find the
/// same residue by difference (`allocation_residue`).  The senior credit is
/// excluded from `a_allocated`; it was already credited in Senior.
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
/// must persist the resulting bytes unchanged in `Ticket.a_remainder_total`
/// (discovery accumulator) or `Ticket.sealed_a_denominator` (frozen at seal).
pub fn encode_a_denominator(value: U256) -> [u8; 32] {
    value.to_le_bytes()
}

/// Compute floor(`slice * weight / denominator`) in U256, then require that
/// the result fits the lamport cash partition.  A zero denominator is a
/// sealed-state error and returns `None` rather than silently paying zero.
/// Delegates to the host model's `backers::pro_rata` so the program and the
/// reference oracle share one implementation of the width contract.
pub fn floor_proportional(slice: u64, weight: u128, denominator: U256) -> Option<u64> {
    musical_chairs_economics::backers::pro_rata(slice, weight, denominator).ok()
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

/// One-row form of the descending prune cursor, which is exclusive at zero: a
/// captured length of `n` starts at `n` and the first row visited is slot
/// `n - 1`.  `prune_rows` takes `expected_cursor == ticket.cursor` and visits
/// slots `cursor - 1 - i`; this helper pins the slot arithmetic for one row.
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

// ---------------------------------------------------------------------------
// Batch shape, ticket gates and account plumbing
// ---------------------------------------------------------------------------

fn overflow() -> anchor_lang::error::Error {
    error!(Code::Overflow)
}

fn invalid_state() -> anchor_lang::error::Error {
    error!(Code::InvalidState)
}

/// Rows per instruction: `1..=8` (`InvalidState` otherwise).
pub fn batch_len(rows: u8) -> Result<usize> {
    require!((1..=MAX_BATCH_ROWS).contains(&rows), Code::InvalidState);
    Ok(usize::from(rows))
}

/// Cursor after a forward batch.  A batch may not run past `limit`
/// (`InvalidState`); it never silently shortens.
pub fn batch_end(cursor: u64, rows: u8, limit: u64) -> Result<u64> {
    let end = cursor.checked_add(u64::from(rows)).ok_or_else(overflow)?;
    require!(end <= limit, Code::InvalidState);
    Ok(end)
}

/// "Open" means more than ten lamports of entitlement remain (spec 3.5).
pub const fn is_open(remaining: u128) -> bool {
    crate::positions::remains_open(remaining)
}

/// Offset of a dense slot inside its page.
pub const fn page_offset(slot: u64) -> usize {
    (slot % PAGE_CAPACITY as u64) as usize
}

/// Ticket identity gates, in the order every phase instruction applies them:
/// stale id first (so a stale caller is told so whatever the phase), then, when
/// the instruction carries the pool, that this ticket is the active one, then
/// the phase.  Instructions without the pool rely on the phase alone: a
/// retired ticket is `IDLE`.
fn require_ticket(
    ticket: &Ticket,
    pool: Option<&Pool>,
    expected_ticket_id: u64,
    phase: u8,
) -> Result<()> {
    require!(ticket.id == expected_ticket_id, Code::StaleTicket);
    if let Some(pool) = pool {
        require!(ticket.id != 0 && pool.active_ticket == ticket.id, Code::StaleTicket);
    }
    require!(ticket.phase == phase, Code::InvalidPhase);
    Ok(())
}

fn require_cursor(ticket: &Ticket, expected_cursor: u64) -> Result<()> {
    require!(ticket.cursor == expected_cursor, Code::StaleCursor);
    Ok(())
}

/// Highest ticket id whose credit may be folded into `Backer.funded`.  Tickets
/// are strictly serial and ids dense (`next_ticket` starts at 1 and a new
/// ticket starts only from `IDLE`), so every id below the active one is
/// committed and equals `pool.finalized_ticket`.
fn materialize_watermark(ticket: &Ticket) -> Result<u64> {
    ticket.id.checked_sub(1).ok_or_else(invalid_state)
}

/// Decode a program-owned account of type `T` (discriminator checked).
pub fn read_program_account<T: AccountDeserialize>(info: &AccountInfo) -> Result<T> {
    require_keys_eq!(*info.owner, crate::ID, Code::WrongOwner);
    let data = info.try_borrow_data()?;
    let mut bytes: &[u8] = &data;
    T::try_deserialize(&mut bytes).map_err(|_| error!(Code::WrongAccount))
}

/// Serialize `value` over a writable program-owned account.
pub fn write_program_account<T: AccountSerialize>(info: &AccountInfo, value: &T) -> Result<()> {
    require!(info.is_writable, Code::WrongAccount);
    let mut data = info.try_borrow_mut_data()?;
    let mut writer = BpfWriter::new(&mut data[..]);
    value.try_serialize(&mut writer)
}

/// A shared page's address is `[seed, index u64 LE]` with the bump it stores.
fn page_key_matches(key: &Pubkey, seed: &[u8], index: u64, bump: u8) -> bool {
    Pubkey::create_program_address(&[seed, &index.to_le_bytes(), &[bump]], &crate::ID)
        .is_ok_and(|derived| derived == *key)
}

/// A Backer's address is `[backer, series, owner]` with the bump it stores.
fn backer_key_matches(key: &Pubkey, backer: &Backer) -> bool {
    Pubkey::create_program_address(
        &[BACKER_SEED, &[backer.series], backer.owner.as_ref(), &[backer.bump]],
        &crate::ID,
    )
    .is_ok_and(|derived| derived == *key)
}

/// A page decoded once for a whole batch.  Several rows of a batch share
/// pages (eight slots per page), and two decoded copies of one account would
/// serialize stale state over each other, so each page is decoded on first use,
/// edited in memory by every row, and written once at the end.
struct Cached<'i, T> {
    info: AccountInfo<'i>,
    value: T,
    dirty: bool,
}

fn cache_slot<'i, T: AccountDeserialize>(
    cache: &mut Vec<Cached<'i, T>>,
    info: &AccountInfo<'i>,
    verify: impl FnOnce(&T) -> bool,
) -> Result<usize> {
    if let Some(at) = cache.iter().position(|cached| cached.info.key == info.key) {
        return Ok(at);
    }
    let value = read_program_account::<T>(info)?;
    require!(verify(&value), Code::WrongAccount);
    cache.push(Cached { info: info.clone(), value, dirty: false });
    Ok(cache.len() - 1)
}

fn flush<T: AccountSerialize>(cache: &[Cached<'_, T>]) -> Result<()> {
    for cached in cache.iter().filter(|cached| cached.dirty) {
        write_program_account(&cached.info, &cached.value)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pure ticket rules
// ---------------------------------------------------------------------------

/// Running Discover accumulators over the open rows seen so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tally {
    pub a_count: u64,
    pub b_count: u64,
    /// U256 sum of open Series-A remainders; never narrowed.
    pub a_total: U256,
    /// Oldest open A so far: `(key, first_sequence, slot)`.
    pub oldest: Option<(Pubkey, u64, u64)>,
}

impl Tally {
    pub fn of(ticket: &Ticket) -> Self {
        Self {
            a_count: ticket.a_count,
            b_count: ticket.b_count,
            a_total: decode_a_denominator(ticket.a_remainder_total),
            oldest: if ticket.oldest == Pubkey::default() {
                None
            } else {
                Some((ticket.oldest, ticket.oldest_sequence, ticket.oldest_slot))
            },
        }
    }

    pub fn store(&self, ticket: &mut Ticket) {
        ticket.a_count = self.a_count;
        ticket.b_count = self.b_count;
        ticket.a_remainder_total = encode_a_denominator(self.a_total);
        if let Some((key, sequence, slot)) = self.oldest {
            ticket.oldest = key;
            ticket.oldest_sequence = sequence;
            ticket.oldest_slot = slot;
        }
    }

    /// Count one OPEN row.  Closed rows (`remaining <= 10`) are snapshotted
    /// for the prune pass but never reach here.
    pub fn add_open(
        &mut self,
        key: Pubkey,
        series: u8,
        first_sequence: u64,
        slot: u64,
        remaining: u128,
    ) -> Result<()> {
        match series {
            SERIES_A => {
                let a_total = econ::accumulate_a_remainder(self.a_total, remaining)
                    .map_err(|_| overflow())?;
                self.a_count = self.a_count.checked_add(1).ok_or_else(overflow)?;
                self.a_total = a_total;
                self.oldest = oldest_a_key(self.oldest, (key, first_sequence, slot));
            }
            SERIES_B => {
                self.b_count = self.b_count.checked_add(1).ok_or_else(overflow)?;
            }
            _ => return Err(error!(Code::WrongAccount)),
        }
        Ok(())
    }
}

/// First touch of a snapshot page by `ticket_id` resets rows left by an older
/// ticket; a page tagged by a LATER ticket cannot be this ticket's.  Only one
/// ticket is active at a time, so every older tag belongs to a committed one.
pub fn claim_snapshot_page(page: &mut SnapshotPage, ticket_id: u64) -> Result<()> {
    if page.ticket_id == ticket_id {
        return Ok(());
    }
    require!(page.ticket_id < ticket_id, Code::WrongAccount);
    page.rows = [SnapshotRow::default(); PAGE_CAPACITY];
    page.ticket_id = ticket_id;
    Ok(())
}

/// Close Discover: compute the slices from the pinned split and the open
/// membership captured BEFORE senior, then move on.  No open Backer at all
/// sends the whole toll to seed and skips to `COMMIT`.
pub fn finish_discover(ticket: &mut Ticket) -> Result<()> {
    let slices = fold_slices_with_bps(
        ticket.toll,
        (ticket.senior_bps, ticket.a_bps, ticket.b_bps),
        ticket.a_count > 0,
        ticket.b_count > 0,
    )
    .ok_or_else(overflow)?;
    ticket.seed_credit = slices.seed;
    ticket.senior_slice = slices.senior;
    ticket.a_slice = slices.a;
    ticket.b_slice = slices.b;
    ticket.phase = if ticket.a_count == 0 && ticket.b_count == 0 { COMMIT } else { SENIOR };
    Ok(())
}

/// Apply a credit to a Backer and the ticket: lifetime and pending rise, the
/// cap holds, and the pending credit is tagged with the ticket.  Zero is a
/// no-op.  An untagged or same-ticket pending balance is required: an older
/// ticket's credit must have been materialized first.
pub fn credit_backer(backer: &mut Backer, ticket: &mut Ticket, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    require!(
        backer.pending_ticket == 0 || backer.pending_ticket == ticket.id,
        Code::InvalidState
    );
    let lifetime = backer.lifetime.checked_add(u128::from(amount)).ok_or_else(overflow)?;
    require!(lifetime <= backer.entitlement, Code::CapExceeded);
    let pending = backer.pending.checked_add(amount).ok_or_else(overflow)?;
    let credited = ticket.credited.checked_add(amount).ok_or_else(overflow)?;
    backer.lifetime = lifetime;
    backer.pending = pending;
    backer.pending_ticket = ticket.id;
    ticket.credited = credited;
    Ok(())
}

/// `apply_senior`: charge the senior slice against the tracked oldest open A.
/// With no open A the slice was already folded to seed by `fold_slices`, so
/// the instruction only advances (and reads no account).
pub fn senior_step(
    ticket: &mut Ticket,
    oldest_backer: Option<&AccountInfo>,
    expected_ticket_id: u64,
) -> Result<()> {
    require_ticket(ticket, None, expected_ticket_id, SENIOR)?;
    if ticket.a_count == 0 {
        // No open A: the senior tier was folded at Discover; whatever account
        // (or none) was supplied is ignored.
        ticket.senior_credit = 0;
        ticket.phase = SEAL;
        return Ok(());
    }
    let oldest_backer = oldest_backer.ok_or_else(|| error!(Code::WrongAccount))?;
    require!(ticket.oldest != Pubkey::default(), Code::InvalidState);
    require_keys_eq!(*oldest_backer.key, ticket.oldest, Code::WrongAccount);
    let mut backer = read_program_account::<Backer>(oldest_backer)?;
    require!(
        backer_key_matches(oldest_backer.key, &backer)
            && backer.series == SERIES_A
            && backer.active
            && backer.slot == ticket.oldest_slot
            && backer.first_sequence == ticket.oldest_sequence,
        Code::WrongAccount
    );
    backer.materialize(materialize_watermark(ticket)?)?;
    let (credit, overshoot, _) =
        cap_senior(ticket.senior_slice, backer.remaining()?).ok_or_else(overflow)?;
    credit_backer(&mut backer, ticket, credit)?;
    ticket.senior_credit = credit;
    ticket.seed_credit = ticket.seed_credit.checked_add(overshoot).ok_or_else(overflow)?;
    write_program_account(oldest_backer, &backer)?;
    ticket.phase = SEAL;
    Ok(())
}

/// Split the B slice into `count` equal floor shares: `(distributed, residue)`.
/// `count == 0` leaves the whole slice as residue.
pub fn b_split(b_slice: u64, b_count: u64) -> Result<(u64, u64)> {
    let share = b_slice.checked_div(b_count).unwrap_or(0);
    let distributed = share.checked_mul(b_count).ok_or_else(overflow)?;
    let residue = b_slice.checked_sub(distributed).ok_or_else(invalid_state)?;
    Ok((distributed, residue))
}

/// `seal_weights`: freeze the A denominator (`sum(captured A remainders) -
/// senior_credit`, U256) and route what cannot be paid to seed.
///
/// - A denominator of zero (senior closed the only A) sends the whole A slice
///   to seed; it is NOT re-folded to B (spec 3.5).
/// - The B pool becomes an exact multiple of the open B count; the floor
///   residue goes to seed now, so every B share is the same constant.
///
/// Resets the cursor for Allocate.
pub fn seal_step(ticket: &mut Ticket, expected_ticket_id: u64) -> Result<()> {
    require_ticket(ticket, None, expected_ticket_id, SEAL)?;
    require!(ticket.a_count != 0 || ticket.b_count != 0, Code::InvalidState);
    let sealed = econ::seal_denominator(
        decode_a_denominator(ticket.a_remainder_total),
        ticket.senior_credit,
    )
    .map_err(|_| invalid_state())?;
    let mut seed = ticket.seed_credit;
    let mut a_slice = ticket.a_slice;
    if sealed == U256::ZERO {
        seed = seed.checked_add(a_slice).ok_or_else(overflow)?;
        a_slice = 0;
    }
    let (b_distributed, b_residue) = b_split(ticket.b_slice, ticket.b_count)?;
    seed = seed.checked_add(b_residue).ok_or_else(overflow)?;
    ticket.sealed_a_denominator = encode_a_denominator(sealed);
    ticket.seed_credit = seed;
    ticket.a_slice = a_slice;
    ticket.b_slice = b_distributed;
    ticket.cursor = 0;
    ticket.phase = ALLOCATE;
    Ok(())
}

/// Uncapped Series-A share `floor(a_slice * weight / sealed)`, zero when the
/// sealed denominator is zero (the A slice was already sent to seed).
pub fn a_share(a_slice: u64, weight: u128, sealed: U256) -> Result<u64> {
    if sealed == U256::ZERO {
        return Ok(0);
    }
    floor_proportional(a_slice, weight, sealed).ok_or_else(invalid_state)
}

/// A captured A row's weight: its captured remainder, less the senior credit
/// on the senior row only.
pub fn a_weight(ticket: &Ticket, row: &SnapshotRow) -> Result<u128> {
    if row.key == ticket.oldest {
        row.remaining.checked_sub(u128::from(ticket.senior_credit)).ok_or_else(invalid_state)
    } else {
        Ok(row.remaining)
    }
}

/// Cap a share at what the Backer can still be credited.
pub fn cap_share(uncapped: u64, live_remaining: u128) -> u64 {
    let room = u64::try_from(live_remaining).unwrap_or(u64::MAX);
    uncapped.min(room)
}

/// Pay one open captured row: its uncapped share of the (constant) pool, the
/// cap overshoot to seed at once, and the capped share credited.
pub fn allocate_row(
    ticket: &mut Ticket,
    row: &SnapshotRow,
    backer: &mut Backer,
    sealed: U256,
) -> Result<()> {
    let live = backer.remaining()?;
    let uncapped = match row.series {
        SERIES_A => a_share(ticket.a_slice, a_weight(ticket, row)?, sealed)?,
        // `seal_step` made the B pool an exact multiple of the open B count.
        SERIES_B => ticket.b_slice.checked_div(ticket.b_count).ok_or_else(invalid_state)?,
        _ => return Err(error!(Code::WrongAccount)),
    };
    let credit = cap_share(uncapped, live);
    let overshoot = uncapped.checked_sub(credit).ok_or_else(invalid_state)?;
    ticket.seed_credit = ticket.seed_credit.checked_add(overshoot).ok_or_else(overflow)?;
    credit_backer(backer, ticket, credit)
}

/// The A floor-division residue: the one remainder not yet attributed when
/// Allocate ends.  It is below the open A count (zero with no open A).
pub fn allocation_residue(ticket: &Ticket) -> Result<u64> {
    let residue = ticket
        .toll
        .checked_sub(ticket.credited)
        .and_then(|rest| rest.checked_sub(ticket.seed_credit))
        .ok_or_else(invalid_state)?;
    require!(residue < ticket.a_count.max(1), Code::InvalidState);
    Ok(residue)
}

/// Close Allocate: the floor residue joins seed, and the cursor is parked at
/// `captured_len` for the descending Prune pass.
pub fn finish_allocate(ticket: &mut Ticket) -> Result<()> {
    let residue = allocation_residue(ticket)?;
    ticket.seed_credit = ticket.seed_credit.checked_add(residue).ok_or_else(overflow)?;
    ticket.cursor = ticket.captured_len;
    ticket.phase = PRUNE;
    Ok(())
}

/// Whether Prune removes a row.  A row captured at `<= 10` is eligible for
/// certain (a Backer's remaining never grows during a ticket) and its real
/// Backer account is then REQUIRED.  A row captured open is pruned only if the
/// keeper supplied its real Backer and the live remaining (after this ticket's
/// credits) is now `<= 10`; a placeholder skips it, to be pruned by a later
/// ticket's pass.
pub fn prune_decision(captured_remaining: u128, live_remaining: Option<u128>) -> Result<bool> {
    match live_remaining {
        Some(live) => Ok(prune_eligible(live)),
        None => {
            require!(!prune_eligible(captured_remaining), Code::WrongAccount);
            Ok(false)
        }
    }
}

/// Swap-delete `slot` out of the dense registry: the last live key moves into
/// `slot` and the last position is cleared (`moved == None` when `slot` is the
/// last live slot).  `pruned_at`/`last_at` index the batch's page cache and may
/// name the SAME page; each cached page is edited in memory and written once.
fn swap_delete(
    pages: &mut [Cached<'_, RegistryPage>],
    pruned_at: usize,
    last_at: usize,
    slot: u64,
    last: u64,
    moved: Option<Pubkey>,
) {
    match moved {
        None => pages[pruned_at].value.keys[page_offset(slot)] = Pubkey::default(),
        Some(key) => {
            pages[pruned_at].value.keys[page_offset(slot)] = key;
            pages[last_at].value.keys[page_offset(last)] = Pubkey::default();
            pages[last_at].dirty = true;
        }
    }
    pages[pruned_at].dirty = true;
}

/// The state edits of pruning one row, shared by [`prune_batch`] and the inline
/// path: swap-delete `slot` out of the dense registry, retire the pruned Backer
/// (inactive, slot cleared; its history stays) and re-home the Backer that
/// moved down into the vacated slot.  `moved` is the registry's last key and
/// its Backer, `None` when `slot` is itself the last live slot.
fn retire_row(
    pages: &mut [Cached<'_, RegistryPage>],
    pruned_at: usize,
    last_at: usize,
    (slot, last): (u64, u64),
    pruned: &mut Backer,
    moved: Option<(Pubkey, &mut Backer)>,
) {
    swap_delete(pages, pruned_at, last_at, slot, last, moved.as_ref().map(|(key, _)| *key));
    pruned.active = false;
    pruned.slot = 0;
    if let Some((_, moved)) = moved {
        moved.slot = slot;
    }
}

/// `commit_ticket`: conservation, then books.  `credited + seed_credit` must
/// equal the toll; the toll leaves `pending_toll` and re-enters as seed and as
/// funded claims (the sum of the `game_accounted` partitions is unchanged).
/// A closing ticket also marks its close's cash committed.
pub fn commit_step(
    ticket: &mut Ticket,
    pool: &mut Pool,
    close: Option<(Pubkey, &mut CloseRecord)>,
    expected_ticket_id: u64,
) -> Result<TicketCommitted> {
    require_ticket(ticket, Some(pool), expected_ticket_id, COMMIT)?;
    let accounted = ticket.credited.checked_add(ticket.seed_credit).ok_or_else(overflow)?;
    require!(accounted == ticket.toll, Code::InvalidState);
    let pending_toll = pool.pending_toll.checked_sub(ticket.toll).ok_or_else(invalid_state)?;
    let seed = pool.seed.checked_add(ticket.seed_credit).ok_or_else(overflow)?;
    let funded_claims = pool
        .funded_claims
        .checked_add(u128::from(ticket.credited))
        .ok_or_else(overflow)?;
    let registry_version = if pool.registry_len != ticket.captured_len {
        pool.registry_version.checked_add(1).ok_or_else(overflow)?
    } else {
        pool.registry_version
    };
    let record = match ticket.close_id {
        Some(close_id) => {
            let (key, record) = close.ok_or_else(|| error!(Code::WrongAccount))?;
            let derived = Pubkey::create_program_address(
                &[CLOSE_SEED, &close_id.to_le_bytes(), &[record.bump]],
                &crate::ID,
            )
            .map_err(|_| error!(Code::WrongAccount))?;
            require!(record.close_id == close_id && key == derived, Code::WrongAccount);
            require!(
                record.closing_ticket_id == Some(ticket.id) && !record.cash_committed,
                Code::InvalidState
            );
            Some(record)
        }
        None => None,
    };

    if let Some(record) = record {
        record.cash_committed = true;
    }
    pool.pending_toll = pending_toll;
    pool.seed = seed;
    pool.funded_claims = funded_claims;
    pool.registry_version = registry_version;
    pool.finalized_ticket = ticket.id;
    pool.active_ticket = 0;
    ticket.phase = IDLE;
    Ok(TicketCommitted {
        schema_version: SCHEMA_VERSION,
        ticket_id: ticket.id,
        round_id: pool.round_id,
        toll: ticket.toll,
        credited: ticket.credited,
        seed: ticket.seed_credit,
    })
}

// ---------------------------------------------------------------------------
// Batch phases over remaining accounts
// ---------------------------------------------------------------------------

/// `discover_rows`: snapshot the registry slots `cursor..cursor+rows`.
///
/// `accounts` is `[registry_page, snapshot_page, backer] x rows`; a page may
/// repeat across rows and is decoded and written once.  Every row is
/// snapshotted (so a closed row can be pruned later) but only OPEN rows reach
/// the counts, the A denominator and the oldest-A tracker.
pub fn discover_batch<'i>(
    ticket: &mut Ticket,
    pool: &Pool,
    accounts: &[AccountInfo<'i>],
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    require_ticket(ticket, Some(pool), expected_ticket_id, DISCOVER)?;
    require_cursor(ticket, expected_cursor)?;
    require!(
        pool.registry_version == ticket.expected_version
            && pool.registry_len == ticket.captured_len,
        Code::StaleTicket
    );
    let count = batch_len(rows)?;
    let end = batch_end(ticket.cursor, rows, ticket.captured_len)?;
    require!(accounts.len() == count * DISCOVER_ACCOUNTS_PER_ROW, Code::InvalidState);

    let mut registry: Vec<Cached<RegistryPage>> = Vec::with_capacity(2);
    let mut snapshots: Vec<Cached<SnapshotPage>> = Vec::with_capacity(2);
    let mut tally = Tally::of(ticket);
    for (i, row) in accounts.chunks_exact(DISCOVER_ACCOUNTS_PER_ROW).enumerate() {
        let slot = ticket.cursor + i as u64;
        let (registry_info, snapshot_info, backer_info) = (&row[0], &row[1], &row[2]);
        let page_index = registry_page_index(slot);
        let offset = page_offset(slot);

        let at = cache_slot(&mut registry, registry_info, |page: &RegistryPage| {
            page_key_matches(registry_info.key, REGISTRY_SEED, page.index, page.bump)
        })?;
        let page = &registry[at].value;
        require!(page.index == page_index, Code::WrongAccount);
        require!(page.keys[offset] == *backer_info.key, Code::WrongAccount);

        let backer = read_program_account::<Backer>(backer_info)?;
        require!(
            backer_key_matches(backer_info.key, &backer)
                && backer.active
                && backer.slot == slot
                && (backer.series == SERIES_A || backer.series == SERIES_B),
            Code::WrongAccount
        );
        let remaining = backer.remaining()?;

        let at = cache_slot(&mut snapshots, snapshot_info, |page: &SnapshotPage| {
            page_key_matches(snapshot_info.key, SNAPSHOT_SEED, page.index, page.bump)
        })?;
        let cached = &mut snapshots[at];
        require!(cached.value.index == snapshot_page_index(slot), Code::WrongAccount);
        claim_snapshot_page(&mut cached.value, ticket.id)?;
        cached.value.rows[offset] =
            SnapshotRow { key: *backer_info.key, series: backer.series, remaining, slot };
        cached.dirty = true;

        if is_open(remaining) {
            tally.add_open(*backer_info.key, backer.series, backer.first_sequence, slot, remaining)?;
        }
    }
    flush(&snapshots)?;
    tally.store(ticket);
    ticket.cursor = end;
    if end == ticket.captured_len {
        finish_discover(ticket)?;
    }
    Ok(())
}

/// `allocate_rows`: pay the captured rows `cursor..cursor+rows`.
///
/// `accounts` is `[snapshot_page, backer] x rows`.  A Backer account may not
/// appear twice in a batch (`DuplicateWritableAccount`, checked before the
/// row-identity check so the duplicate is named as such).  Rows captured
/// closed are paid nothing and their Backer is neither decoded nor written.
pub fn allocate_batch<'i>(
    ticket: &mut Ticket,
    accounts: &[AccountInfo<'i>],
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    require_ticket(ticket, None, expected_ticket_id, ALLOCATE)?;
    require_cursor(ticket, expected_cursor)?;
    let count = batch_len(rows)?;
    let end = batch_end(ticket.cursor, rows, ticket.captured_len)?;
    require!(accounts.len() == count * ALLOCATE_ACCOUNTS_PER_ROW, Code::InvalidState);

    let ticket_id = ticket.id;
    let sealed = decode_a_denominator(ticket.sealed_a_denominator);
    let watermark = materialize_watermark(ticket)?;
    let mut snapshots: Vec<Cached<SnapshotPage>> = Vec::with_capacity(2);
    let mut supplied: Vec<Pubkey> = Vec::with_capacity(count);
    for (i, row_accounts) in accounts.chunks_exact(ALLOCATE_ACCOUNTS_PER_ROW).enumerate() {
        let slot = ticket.cursor + i as u64;
        let (snapshot_info, backer_info) = (&row_accounts[0], &row_accounts[1]);
        require!(!supplied.contains(backer_info.key), Code::DuplicateWritableAccount);
        supplied.push(*backer_info.key);

        let at = cache_slot(&mut snapshots, snapshot_info, |page: &SnapshotPage| {
            page.ticket_id == ticket_id
                && page_key_matches(snapshot_info.key, SNAPSHOT_SEED, page.index, page.bump)
        })?;
        let page = &snapshots[at].value;
        require!(page.index == snapshot_page_index(slot), Code::WrongAccount);
        let row = page.rows[page_offset(slot)];
        require!(row.slot == slot && row.key == *backer_info.key, Code::WrongAccount);
        if !is_open(row.remaining) {
            continue;
        }

        let mut backer = read_program_account::<Backer>(backer_info)?;
        require!(
            backer_key_matches(backer_info.key, &backer)
                && backer.series == row.series
                && backer.active
                && backer.slot == slot,
            Code::WrongAccount
        );
        backer.materialize(watermark)?;
        allocate_row(ticket, &row, &mut backer, sealed)?;
        write_program_account(backer_info, &backer)?;
    }
    ticket.cursor = end;
    if end == ticket.captured_len {
        finish_allocate(ticket)?;
    }
    Ok(())
}

/// `prune_rows`: walk the captured rows DOWN from the cursor (exclusive), so a
/// swap-delete only ever moves a row that was already visited.
///
/// Per logical row `accounts` holds
/// `[snapshot_page, backer_pruned, registry_page_pruned, registry_page_last,
/// backer_moved]`.  The snapshot page is always real (it carries the captured
/// remainder).  For a row the keeper expects to skip, the other four are
/// [`PRUNE_PLACEHOLDER`].  For a row to be pruned:
///
/// - the two registry pages may be the same account (decoded and written once);
/// - when the pruned slot is the last live slot, `backer_moved` is
///   `backer_pruned` itself and no moved write happens;
/// - otherwise `backer_moved` is the registry's last key and may not repeat any
///   other Backer written in the batch (`DuplicateWritableAccount`).
pub fn prune_batch<'i>(
    ticket: &mut Ticket,
    pool: &mut Pool,
    accounts: &[AccountInfo<'i>],
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    require_ticket(ticket, Some(pool), expected_ticket_id, PRUNE)?;
    require_cursor(ticket, expected_cursor)?;
    let count = batch_len(rows)?;
    require!(u64::from(rows) <= ticket.cursor, Code::InvalidState);
    require!(accounts.len() == count * PRUNE_ACCOUNTS_PER_ROW, Code::InvalidState);

    let ticket_id = ticket.id;
    let mut registry: Vec<Cached<RegistryPage>> = Vec::with_capacity(2);
    let mut snapshots: Vec<Cached<SnapshotPage>> = Vec::with_capacity(2);
    // Every real `backer_pruned` supplied (any outcome) and every Backer
    // actually written.  A skipped row's real Backer may legitimately be the
    // `moved` row of a later prune (it was only read), so the sets differ.
    let mut supplied: Vec<Pubkey> = Vec::with_capacity(count);
    let mut written: Vec<Pubkey> = Vec::with_capacity(2 * count);
    let mut registry_len = pool.registry_len;
    for (i, row_accounts) in accounts.chunks_exact(PRUNE_ACCOUNTS_PER_ROW).enumerate() {
        let slot = ticket.cursor - 1 - i as u64;
        let (snapshot_info, pruned_info, pruned_page_info, last_page_info, moved_info) = (
            &row_accounts[0],
            &row_accounts[1],
            &row_accounts[2],
            &row_accounts[3],
            &row_accounts[4],
        );

        let at = cache_slot(&mut snapshots, snapshot_info, |page: &SnapshotPage| {
            page.ticket_id == ticket_id
                && page_key_matches(snapshot_info.key, SNAPSHOT_SEED, page.index, page.bump)
        })?;
        let page = &snapshots[at].value;
        require!(page.index == snapshot_page_index(slot), Code::WrongAccount);
        let row = page.rows[page_offset(slot)];
        require!(row.slot == slot, Code::WrongAccount);

        let mut pruned = None;
        if *pruned_info.key != PRUNE_PLACEHOLDER {
            require!(!supplied.contains(pruned_info.key), Code::DuplicateWritableAccount);
            supplied.push(*pruned_info.key);
            require!(*pruned_info.key == row.key, Code::WrongAccount);
            let backer = read_program_account::<Backer>(pruned_info)?;
            require!(backer_key_matches(pruned_info.key, &backer), Code::WrongAccount);
            pruned = Some(backer);
        }
        let live = match &pruned {
            Some(backer) => Some(backer.remaining()?),
            None => None,
        };
        if !prune_decision(row.remaining, live)? {
            continue;
        }

        let mut pruned = pruned.ok_or_else(invalid_state)?;
        require!(
            pruned.active && pruned.slot == slot && pruned.series == row.series,
            Code::WrongAccount
        );
        let last = registry_len.checked_sub(1).ok_or_else(invalid_state)?;
        require!(slot <= last, Code::InvalidState);
        let pruned_at = cache_slot(&mut registry, pruned_page_info, |page: &RegistryPage| {
            page_key_matches(pruned_page_info.key, REGISTRY_SEED, page.index, page.bump)
        })?;
        let last_at = cache_slot(&mut registry, last_page_info, |page: &RegistryPage| {
            page_key_matches(last_page_info.key, REGISTRY_SEED, page.index, page.bump)
        })?;
        require!(
            registry[pruned_at].value.index == registry_page_index(slot)
                && registry[last_at].value.index == registry_page_index(last),
            Code::WrongAccount
        );
        require!(
            registry[pruned_at].value.keys[page_offset(slot)] == *pruned_info.key,
            Code::WrongAccount
        );
        written.push(*pruned_info.key);

        let moved_key =
            if slot == last { None } else { Some(registry[last_at].value.keys[page_offset(last)]) };
        let mut moved = match moved_key {
            // The pruned row IS the last live row: the moved account aliases it.
            None => {
                require!(*moved_info.key == *pruned_info.key, Code::WrongAccount);
                None
            }
            Some(key) => {
                require!(key != Pubkey::default(), Code::WrongAccount);
                require!(!written.contains(moved_info.key), Code::DuplicateWritableAccount);
                require!(*moved_info.key == key, Code::WrongAccount);
                let moved = read_program_account::<Backer>(moved_info)?;
                require!(
                    backer_key_matches(moved_info.key, &moved)
                        && moved.active
                        && moved.slot == last,
                    Code::WrongAccount
                );
                written.push(key);
                Some(moved)
            }
        };

        retire_row(
            &mut registry,
            pruned_at,
            last_at,
            (slot, last),
            &mut pruned,
            moved_key.zip(moved.as_mut()),
        );
        write_program_account(pruned_info, &pruned)?;
        if let Some(moved) = &moved {
            write_program_account(moved_info, moved)?;
        }
        registry_len = last;
    }
    flush(&registry)?;
    pool.registry_len = registry_len;
    ticket.cursor -= u64::from(rows);
    if ticket.cursor == 0 {
        ticket.phase = COMMIT;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Inline path: the whole ticket inside one payout instruction
// ---------------------------------------------------------------------------

/// Remaining accounts the inline path appends after the registry's Backers:
/// `[registry_page_0, snapshot_page_0]`.
pub const INLINE_PAGE_ACCOUNTS: usize = 2;

/// Shape-check the remaining accounts of an inline payout and return the number
/// of Backer accounts (`registry_len`) they must start with.
///
/// The layout is `[Backer of registry slot 0 .. slot registry_len - 1 (in
/// registry order)] ++ [RegistryPage 0, SnapshotPage 0]`, every account
/// writable.  Nothing is decoded here; identity is proven by the phase
/// functions the run drives.
///
/// - A registry outside `1..=INLINE_THRESHOLD` is not offered inline
///   (`InvalidState`): above one page the keeper must use chunks.
/// - Any other count is a partial or padded layout (`WrongAccount`).
/// - A repeated account is `DuplicateWritableAccount`, checked before identity
///   like Allocate does; a read-only account is `WrongAccount`.
pub fn inline_layout(registry_len: u64, accounts: &[AccountInfo]) -> Result<usize> {
    require!(registry_len != 0 && registry_len <= INLINE_THRESHOLD, Code::InvalidState);
    // `INLINE_THRESHOLD <= PAGE_CAPACITY`, so this cannot truncate.
    let backers = registry_len as usize;
    require!(accounts.len() == backers + INLINE_PAGE_ACCOUNTS, Code::WrongAccount);
    for (i, info) in accounts.iter().enumerate() {
        require!(
            !accounts[..i].iter().any(|earlier| earlier.key == info.key),
            Code::DuplicateWritableAccount
        );
    }
    require!(accounts.iter().all(|info| info.is_writable), Code::WrongAccount);
    Ok(backers)
}

/// Run a freshly started ticket (`DISCOVER`, cursor `0`, `pool.active_ticket ==
/// ticket.id`) through all six phases in this instruction and retire it.
///
/// `accounts` is the layout described at [`inline_layout`].  The run calls the
/// same phase functions the keeper's six instructions call, in order, over the
/// registry's single page:
///
/// 1. [`discover_batch`] with `rows = registry_len` (`[registry_page_0,
///    snapshot_page_0, backer_i]` per row);
/// 2. [`senior_step`] with the tracked oldest open A, taken from the Backers;
/// 3. [`seal_step`];
/// 4. [`allocate_batch`] with `rows = registry_len`;
/// 5. the Prune pass, descending from `captured_len` ([`prune_inline`]), which
///    removes every row whose live remaining is now `<= 10` (the canonical
///    keeper choice) through the same [`prune_decision`] and [`retire_row`] as
///    [`prune_batch`];
/// 6. [`commit_step`], whose `TicketCommitted` is returned for the caller to
///    emit.
///
/// A ticket with no open row goes straight from Discover to Commit, exactly as
/// the chunked ticket does.  Chunk size never changes a ticket's result (the
/// stepwise suites pin that), so the one-batch run equals any chunked run.  A
/// failure anywhere reverts the whole payout.
///
/// Compute and memory: this is the sum of the phases' work, so expect it to pass
/// the 200,000 default for four or more Backers (the caller adds a compute-budget
/// instruction).  Each phase keeps its own page caches; the run adds one account
/// list (`3 * registry_len` entries at most) and, in Prune, one decoded copy of
/// each Backer.  The program heap is a bump allocator that never frees, so these
/// allocations add up for the whole instruction: about 7.5 KB at eight Backers.
#[inline(never)]
pub fn run_inline_ticket<'i>(
    ticket: &mut Ticket,
    pool: &mut Pool,
    accounts: &[AccountInfo<'i>],
) -> Result<TicketCommitted> {
    let id = ticket.id;
    let n = inline_layout(ticket.captured_len, accounts)?;
    let (backers, pages) = accounts.split_at(n);
    let (registry_page, snapshot_page) = (&pages[0], &pages[1]);

    // One account list, refilled per phase (the allocator never frees).
    let mut rows: Vec<AccountInfo<'i>> = Vec::with_capacity(DISCOVER_ACCOUNTS_PER_ROW * n);
    for backer in backers {
        rows.push(registry_page.clone());
        rows.push(snapshot_page.clone());
        rows.push(backer.clone());
    }
    discover_batch(ticket, pool, &rows, id, 0, n as u8)?;

    if ticket.phase == SENIOR {
        let oldest = if ticket.a_count == 0 {
            None
        } else {
            backers.iter().find(|backer| *backer.key == ticket.oldest)
        };
        senior_step(ticket, oldest, id)?;
        seal_step(ticket, id)?;

        rows.clear();
        for backer in backers {
            rows.push(snapshot_page.clone());
            rows.push(backer.clone());
        }
        // Seal reset the cursor to 0.
        allocate_batch(ticket, &rows, id, 0, n as u8)?;
        prune_inline(ticket, pool, backers, registry_page, snapshot_page)?;
    }
    // Discover found no open row, or Prune just ran: Commit is next either way.
    commit_step(ticket, pool, None, id)
}

/// The Prune pass of [`run_inline_ticket`]: walk the captured rows DOWN from
/// `captured_len`, prune every row whose live remaining is `<= 10`, then leave
/// the ticket in `COMMIT` with its cursor at `0`.
///
/// This is [`prune_batch`] with the keeper's choices made canonically: a row is
/// pruned exactly when [`prune_decision`] says so for its live Backer, the
/// registry edit is [`retire_row`] on the single page, and a Backer moved down
/// by an earlier prune is the one a later prune moves again (chained moves are
/// fine here: every Backer is decoded once and written once at the end).
#[inline(never)]
fn prune_inline<'i>(
    ticket: &mut Ticket,
    pool: &mut Pool,
    backers: &[AccountInfo<'i>],
    registry_page: &AccountInfo<'i>,
    snapshot_page: &AccountInfo<'i>,
) -> Result<()> {
    let ticket_id = ticket.id;
    require_ticket(ticket, Some(pool), ticket_id, PRUNE)?;
    require_cursor(ticket, ticket.captured_len)?;

    let mut registry: Vec<Cached<RegistryPage>> = Vec::with_capacity(1);
    let mut snapshots: Vec<Cached<SnapshotPage>> = Vec::with_capacity(1);
    let page_at = cache_slot(&mut registry, registry_page, |page: &RegistryPage| {
        page_key_matches(registry_page.key, REGISTRY_SEED, page.index, page.bump)
    })?;
    let snapshot_at = cache_slot(&mut snapshots, snapshot_page, |page: &SnapshotPage| {
        page.ticket_id == ticket_id
            && page_key_matches(snapshot_page.key, SNAPSHOT_SEED, page.index, page.bump)
    })?;
    require!(
        registry[page_at].value.index == 0 && snapshots[snapshot_at].value.index == 0,
        Code::WrongAccount
    );
    // Discover and Allocate just proved each of these as the canonical Backer
    // of its slot, so they are decoded without re-deriving their addresses.
    let mut live: Vec<Cached<Backer>> = Vec::with_capacity(backers.len());
    for info in backers {
        cache_slot(&mut live, info, |backer: &Backer| backer.active)?;
    }

    let mut registry_len = pool.registry_len;
    for slot in (0..ticket.captured_len).rev() {
        let row = snapshots[snapshot_at].value.rows[page_offset(slot)];
        require!(row.slot == slot, Code::WrongAccount);
        let pruned_at = live
            .iter()
            .position(|cached| *cached.info.key == row.key)
            .ok_or_else(|| error!(Code::WrongAccount))?;
        if !prune_decision(row.remaining, Some(live[pruned_at].value.remaining()?))? {
            continue;
        }

        let mut pruned = live[pruned_at].value.clone();
        require!(
            pruned.active && pruned.slot == slot && pruned.series == row.series,
            Code::WrongAccount
        );
        let last = registry_len.checked_sub(1).ok_or_else(invalid_state)?;
        require!(slot <= last, Code::InvalidState);
        require!(registry[page_at].value.keys[page_offset(slot)] == row.key, Code::WrongAccount);

        let moved_key =
            if slot == last { None } else { Some(registry[page_at].value.keys[page_offset(last)]) };
        let moved_at = match moved_key {
            None => None,
            Some(key) => {
                require!(key != Pubkey::default(), Code::WrongAccount);
                let at = live
                    .iter()
                    .position(|cached| *cached.info.key == key)
                    .ok_or_else(|| error!(Code::WrongAccount))?;
                require!(
                    live[at].value.active && live[at].value.slot == last,
                    Code::WrongAccount
                );
                Some(at)
            }
        };
        let mut moved = moved_at.map(|at| live[at].value.clone());

        retire_row(
            &mut registry,
            page_at,
            page_at,
            (slot, last),
            &mut pruned,
            moved_key.zip(moved.as_mut()),
        );
        live[pruned_at].value = pruned;
        live[pruned_at].dirty = true;
        if let (Some(at), Some(moved)) = (moved_at, moved) {
            live[at].value = moved;
            live[at].dirty = true;
        }
        registry_len = last;
    }
    flush(&registry)?;
    flush(&live)?;
    pool.registry_len = registry_len;
    ticket.cursor = 0;
    ticket.phase = COMMIT;
    Ok(())
}

// ---------------------------------------------------------------------------
// Instruction accounts and handlers
// ---------------------------------------------------------------------------

/// Remaining accounts: `[registry_page, snapshot_page, backer] x rows`.
#[derive(Accounts)]
pub struct DiscoverRows<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
}

/// `backer` is the tracked oldest open A.  A ticket with no open A reads no
/// account here, so any account (conventionally the program id) will do.
#[derive(Accounts)]
pub struct ApplySenior<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    /// CHECK: decoded and fully verified by `senior_step`: it must be the
    /// ticket's tracked oldest open A, a canonical Backer PDA. Optional so a
    /// ticket with no open A (senior already folded to seed) can pass the
    /// program id as the Anchor `None` placeholder instead of a writable
    /// stand-in (a read-only program account would fail the `mut` check).
    #[account(mut)]
    pub backer: Option<UncheckedAccount<'info>>,
}

#[derive(Accounts)]
pub struct SealWeights<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
}

/// Remaining accounts: `[snapshot_page, backer] x rows`.
#[derive(Accounts)]
pub struct AllocateRows<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
}

/// Remaining accounts per logical row:
/// `[snapshot_page, backer_pruned, registry_page_pruned, registry_page_last,
/// backer_moved]`.
#[derive(Accounts)]
pub struct PruneRows<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
}

/// `close_record` is required exactly when the ticket allocates a closing
/// payout's toll.
#[event_cpi]
#[derive(Accounts)]
pub struct CommitTicket<'info> {
    #[account(mut, seeds = [TICKET_SEED], bump = ticket.bump)]
    pub ticket: Box<Account<'info, Ticket>>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut)]
    pub close_record: Option<Box<Account<'info, CloseRecord>>>,
}

pub fn discover_rows(
    ctx: Context<DiscoverRows>,
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    discover_batch(
        &mut ctx.accounts.ticket,
        &ctx.accounts.pool,
        ctx.remaining_accounts,
        expected_ticket_id,
        expected_cursor,
        rows,
    )
}

pub fn apply_senior(ctx: Context<ApplySenior>, expected_ticket_id: u64) -> Result<()> {
    let backer = ctx.accounts.backer.as_ref().map(|account| account.to_account_info());
    senior_step(&mut ctx.accounts.ticket, backer.as_ref(), expected_ticket_id)
}

pub fn seal_weights(ctx: Context<SealWeights>, expected_ticket_id: u64) -> Result<()> {
    seal_step(&mut ctx.accounts.ticket, expected_ticket_id)
}

pub fn allocate_rows(
    ctx: Context<AllocateRows>,
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    allocate_batch(
        &mut ctx.accounts.ticket,
        ctx.remaining_accounts,
        expected_ticket_id,
        expected_cursor,
        rows,
    )
}

pub fn prune_rows(
    ctx: Context<PruneRows>,
    expected_ticket_id: u64,
    expected_cursor: u64,
    rows: u8,
) -> Result<()> {
    prune_batch(
        &mut ctx.accounts.ticket,
        &mut ctx.accounts.pool,
        ctx.remaining_accounts,
        expected_ticket_id,
        expected_cursor,
        rows,
    )
}

pub fn commit_ticket(ctx: Context<CommitTicket>, expected_ticket_id: u64) -> Result<()> {
    let event = {
        let accounts = &mut *ctx.accounts;
        let close = match accounts.close_record.as_mut() {
            Some(record) => {
                let key = record.key();
                let record: &mut CloseRecord = record;
                Some((key, record))
            }
            None => None,
        };
        commit_step(&mut accounts.ticket, &mut accounts.pool, close, expected_ticket_id)?
    };
    emit_cpi!(event);
    Ok(())
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

#[cfg(test)]
mod ticket_tests {
    //! Host tests for the allocation ticket.  A tiny in-memory `World` of
    //! program-owned accounts (registry pages, snapshot pages, Backers) backs
    //! the real `*_batch` / `*_step` functions the handlers call, so the
    //! goldens, the cursor and stale rules, the alias rules and the
    //! differential against the host `Book` exercise production code paths.

    use super::*;
    use crate::state::{ConfigVersion, INLINE_THRESHOLD};
    use anchor_lang::solana_program::system_program;
    use musical_chairs_economics::backers::{BackerKey, Book, Series};

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

    /// An account backed by leaked memory, as the control-plane tests do.
    fn info(key: Pubkey, owner: Pubkey, data: Vec<u8>) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(key));
        let owner = Box::leak(Box::new(owner));
        let lamports = Box::leak(Box::new(1_000_000u64));
        let data = Box::leak(data.into_boxed_slice());
        AccountInfo::new(key, false, true, lamports, data, owner, false, 0)
    }

    /// Account bytes padded to the on-chain `space`, as a provisioned account.
    fn account_bytes<T: AccountSerialize>(value: &T, space: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; space];
        let mut cursor: &mut [u8] = &mut bytes;
        value.try_serialize(&mut cursor).expect("account fits its space");
        bytes
    }

    fn placeholder() -> AccountInfo<'static> {
        info(PRUNE_PLACEHOLDER, system_program::ID, Vec::new())
    }

    fn owner_key(index: usize) -> Pubkey {
        let mut bytes = [0xAA; 32];
        bytes[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
        Pubkey::new_from_array(bytes)
    }

    #[derive(Clone, Copy)]
    struct Spec {
        series: u8,
        entitlement: u128,
        lifetime: u128,
    }

    fn a(entitlement: u128) -> Spec {
        Spec { series: SERIES_A, entitlement, lifetime: 0 }
    }

    fn b(entitlement: u128) -> Spec {
        Spec { series: SERIES_B, entitlement, lifetime: 0 }
    }

    struct Checkpoint {
        ticket: Ticket,
        pool: Pool,
        data: Vec<Vec<u8>>,
    }

    /// Registry pages, snapshot pages and Backers as program-owned accounts,
    /// plus the pool and ticket the typed accounts would carry.
    struct World {
        pool: Pool,
        ticket: Ticket,
        registry: Vec<AccountInfo<'static>>,
        snapshots: Vec<AccountInfo<'static>>,
        backers: Vec<AccountInfo<'static>>,
    }

    impl World {
        /// Every spec is an active registry row (slot = index), whatever its
        /// remaining, so closed rows can be staged.  First-funding sequences are
        /// `index + 1`, so the lowest index is the oldest A.
        fn new(specs: &[Spec]) -> Self {
            let pages = specs.len().div_ceil(PAGE_CAPACITY).max(1);
            let mut registry_pages = Vec::new();
            let mut snapshot_pages = Vec::new();
            for index in 0..pages as u64 {
                let (_, bump) = Pubkey::find_program_address(
                    &[REGISTRY_SEED, &index.to_le_bytes()],
                    &crate::ID,
                );
                registry_pages.push(RegistryPage { bump, index, ..RegistryPage::default() });
                let (_, bump) = Pubkey::find_program_address(
                    &[SNAPSHOT_SEED, &index.to_le_bytes()],
                    &crate::ID,
                );
                snapshot_pages.push(SnapshotPage { bump, index, ..SnapshotPage::default() });
            }
            let mut backers = Vec::new();
            for (index, spec) in specs.iter().enumerate() {
                let owner = owner_key(index);
                let (address, bump) = Pubkey::find_program_address(
                    &[BACKER_SEED, &[spec.series], owner.as_ref()],
                    &crate::ID,
                );
                let backer = Backer {
                    bump,
                    owner,
                    series: spec.series,
                    entitlement: spec.entitlement,
                    lifetime: spec.lifetime,
                    first_sequence: index as u64 + 1,
                    active: true,
                    slot: index as u64,
                    ..Backer::default()
                };
                registry_pages[index / PAGE_CAPACITY].keys[index % PAGE_CAPACITY] = address;
                backers.push(info(address, crate::ID, account_bytes(&backer, Backer::SPACE)));
            }
            let page_info = |seed: &[u8], index: u64, bytes: Vec<u8>| {
                let (address, _) =
                    Pubkey::find_program_address(&[seed, &index.to_le_bytes()], &crate::ID);
                info(address, crate::ID, bytes)
            };
            let registry = registry_pages
                .iter()
                .map(|page| {
                    page_info(REGISTRY_SEED, page.index, account_bytes(page, RegistryPage::SPACE))
                })
                .collect();
            let snapshots = snapshot_pages
                .iter()
                .map(|page| {
                    page_info(SNAPSHOT_SEED, page.index, account_bytes(page, SnapshotPage::SPACE))
                })
                .collect();
            Self {
                pool: Pool {
                    bump: 254,
                    round_id: 1,
                    registry_len: specs.len() as u64,
                    registry_version: 3,
                    next_ticket: 1,
                    ..Pool::default()
                },
                ticket: Ticket { bump: 255, phase: IDLE, ..Ticket::default() },
                registry,
                snapshots,
                backers,
            }
        }

        /// What `withdraw_simple` does for a toll: escrow it and start a ticket.
        fn start_ticket(&mut self, toll: u64) {
            let config = ConfigVersion::default_v1();
            let id = self.pool.next_ticket;
            self.pool.next_ticket += 1;
            self.pool.active_ticket = id;
            self.pool.pending_toll += toll;
            self.ticket = Ticket {
                bump: 255,
                id,
                phase: DISCOVER,
                close_id: None,
                captured_len: self.pool.registry_len,
                expected_version: self.pool.registry_version,
                toll,
                senior_bps: config.senior_bps,
                a_bps: config.a_bps,
                b_bps: config.b_bps,
                ..Ticket::default()
            };
        }

        fn accounts(&self) -> impl Iterator<Item = &AccountInfo<'static>> {
            self.registry.iter().chain(self.snapshots.iter()).chain(self.backers.iter())
        }

        fn checkpoint(&self) -> Checkpoint {
            Checkpoint {
                ticket: self.ticket.clone(),
                pool: self.pool.clone(),
                data: self.accounts().map(|account| account.data.borrow().to_vec()).collect(),
            }
        }

        fn restore(&mut self, checkpoint: Checkpoint) {
            for (account, saved) in self.accounts().zip(checkpoint.data.iter()) {
                account.data.borrow_mut().copy_from_slice(saved);
            }
            self.ticket = checkpoint.ticket;
            self.pool = checkpoint.pool;
        }

        fn backer(&self, index: usize) -> Backer {
            read_program_account(&self.backers[index]).unwrap()
        }

        fn registry_page(&self, index: usize) -> RegistryPage {
            read_program_account(&self.registry[index]).unwrap()
        }

        fn snapshot_page(&self, index: usize) -> SnapshotPage {
            read_program_account(&self.snapshots[index]).unwrap()
        }

        fn registry_key(&self, slot: u64) -> Pubkey {
            self.registry_page((slot / 8) as usize).keys[page_offset(slot)]
        }

        fn snapshot_row(&self, slot: u64) -> SnapshotRow {
            self.snapshot_page((slot / 8) as usize).rows[page_offset(slot)]
        }

        fn backer_account(&self, key: &Pubkey) -> &AccountInfo<'static> {
            self.backers.iter().find(|account| account.key == key).expect("known backer")
        }

        fn backer_by_key(&self, key: &Pubkey) -> Backer {
            read_program_account(self.backer_account(key)).unwrap()
        }

        fn discover_accounts(&self, take: u8) -> Vec<AccountInfo<'static>> {
            let mut accounts = Vec::new();
            for i in 0..u64::from(take) {
                let slot = self.ticket.cursor + i;
                let page = (slot / 8) as usize;
                accounts.push(self.registry[page].clone());
                accounts.push(self.snapshots[page].clone());
                accounts.push(self.backer_account(&self.registry_key(slot)).clone());
            }
            accounts
        }

        fn discover(&mut self, rows: u8) -> Result<()> {
            while self.ticket.phase == DISCOVER {
                let take = u64::from(rows).min(self.ticket.captured_len - self.ticket.cursor) as u8;
                let accounts = self.discover_accounts(take);
                let (id, cursor) = (self.ticket.id, self.ticket.cursor);
                discover_batch(&mut self.ticket, &self.pool, &accounts, id, cursor, take)?;
            }
            Ok(())
        }

        fn senior(&mut self) -> Result<()> {
            let oldest = if self.ticket.a_count == 0 {
                placeholder()
            } else {
                self.backer_account(&self.ticket.oldest).clone()
            };
            let id = self.ticket.id;
            senior_step(&mut self.ticket, Some(&oldest), id)
        }

        fn seal(&mut self) -> Result<()> {
            let id = self.ticket.id;
            seal_step(&mut self.ticket, id)
        }

        fn allocate_accounts(&self, take: u8) -> Vec<AccountInfo<'static>> {
            let mut accounts = Vec::new();
            for i in 0..u64::from(take) {
                let slot = self.ticket.cursor + i;
                accounts.push(self.snapshots[(slot / 8) as usize].clone());
                accounts.push(self.backer_account(&self.snapshot_row(slot).key).clone());
            }
            accounts
        }

        fn allocate(&mut self, rows: u8) -> Result<()> {
            while self.ticket.phase == ALLOCATE {
                let take = u64::from(rows).min(self.ticket.captured_len - self.ticket.cursor) as u8;
                let accounts = self.allocate_accounts(take);
                let (id, cursor) = (self.ticket.id, self.ticket.cursor);
                allocate_batch(&mut self.ticket, &accounts, id, cursor, take)?;
            }
            Ok(())
        }

        /// One Prune batch as a keeper builds it: real accounts for every row
        /// that is prunable NOW (captured closed, or newly capped by this
        /// ticket's credits unless `skip_newly_capped`), the placeholder for
        /// the rest.  Earlier swap-deletes of the same batch are simulated.
        fn prune_accounts(&self, take: u8, skip_newly_capped: bool) -> Vec<AccountInfo<'static>> {
            let mut keys: Vec<Pubkey> =
                (0..self.pool.registry_len).map(|slot| self.registry_key(slot)).collect();
            let mut accounts = Vec::new();
            for i in 0..u64::from(take) {
                let slot = self.ticket.cursor - 1 - i;
                let row = self.snapshot_row(slot);
                let live = self.backer_by_key(&row.key).remaining().unwrap();
                let real = prune_eligible(row.remaining) || (prune_eligible(live) && !skip_newly_capped);
                accounts.push(self.snapshots[(slot / 8) as usize].clone());
                if real {
                    let last = keys.len() as u64 - 1;
                    accounts.push(self.backer_account(&row.key).clone());
                    accounts.push(self.registry[(slot / 8) as usize].clone());
                    accounts.push(self.registry[(last / 8) as usize].clone());
                    accounts.push(self.backer_account(&keys[last as usize]).clone());
                    if slot != last {
                        keys[slot as usize] = keys[last as usize];
                    }
                    keys.pop();
                } else {
                    for _ in 0..4 {
                        accounts.push(placeholder());
                    }
                }
            }
            accounts
        }

        /// Prune to COMMIT in batches of `rows`.  A batch the keeper cannot
        /// legally carry (`DuplicateWritableAccount`) falls back to one row per
        /// instruction, exactly as a real keeper would resubmit it.
        fn prune(&mut self, rows: u8, skip_newly_capped: bool) -> Result<()> {
            let mut rows = rows;
            while self.ticket.phase == PRUNE {
                let take = u64::from(rows).min(self.ticket.cursor) as u8;
                let accounts = self.prune_accounts(take, skip_newly_capped);
                let (id, cursor) = (self.ticket.id, self.ticket.cursor);
                let checkpoint = self.checkpoint();
                match prune_batch(&mut self.ticket, &mut self.pool, &accounts, id, cursor, take) {
                    Err(error) if take > 1 && is_code(&error, Code::DuplicateWritableAccount) => {
                        self.restore(checkpoint);
                        rows = 1;
                    }
                    other => other?,
                }
            }
            Ok(())
        }

        fn commit(&mut self) -> Result<TicketCommitted> {
            let id = self.ticket.id;
            commit_step(&mut self.ticket, &mut self.pool, None, id)
        }

        /// The remaining accounts of an inline payout over the current
        /// registry: one Backer per slot in registry order, then registry
        /// page 0 and snapshot page 0.
        fn inline_accounts(&self) -> Vec<AccountInfo<'static>> {
            let mut accounts: Vec<AccountInfo<'static>> = (0..self.pool.registry_len)
                .map(|slot| self.backer_account(&self.registry_key(slot)).clone())
                .collect();
            accounts.push(self.registry[0].clone());
            accounts.push(self.snapshots[0].clone());
            accounts
        }

        /// What a non-closing payout with inline accounts does for a toll: the
        /// production ticket start followed by the whole in-instruction run.
        fn begin_inline(&mut self, toll: u64) -> Result<(u64, Option<TicketCommitted>)> {
            let accounts = self.inline_accounts();
            crate::positions::begin_ticket(
                &mut self.ticket,
                &mut self.pool,
                None,
                toll,
                &ConfigVersion::default_v1(),
                Some(&accounts),
            )
        }

        fn run_inline(&mut self, toll: u64) -> TicketCommitted {
            let (id, event) = self.begin_inline(toll).unwrap();
            let event = event.expect("an inline run commits");
            assert_eq!(event.ticket_id, id);
            event
        }

        /// Advance until the ticket reaches `phase`.
        fn advance_to(&mut self, phase: u8, rows: u8) {
            while self.ticket.phase != phase {
                match self.ticket.phase {
                    DISCOVER => self.discover(rows).unwrap(),
                    SENIOR => self.senior().unwrap(),
                    SEAL => self.seal().unwrap(),
                    ALLOCATE => self.allocate(rows).unwrap(),
                    PRUNE => self.prune(rows, false).unwrap(),
                    other => panic!("cannot advance from phase {other}"),
                }
            }
        }

        /// Start a ticket for `toll` and run it to commit.
        fn run(&mut self, toll: u64, rows: u8) -> TicketCommitted {
            self.start_ticket(toll);
            self.advance_to(COMMIT, rows);
            self.commit().unwrap()
        }

        fn digest(&self) -> String {
            let backers: Vec<_> = (0..self.backers.len())
                .map(|index| {
                    let backer = self.backer(index);
                    (
                        backer.lifetime,
                        backer.pending,
                        backer.funded,
                        backer.pending_ticket,
                        backer.active,
                        backer.slot,
                    )
                })
                .collect();
            let keys: Vec<_> = (0..self.pool.registry_len).map(|slot| self.registry_key(slot)).collect();
            format!(
                "{backers:?} {keys:?} seed={} funded={} len={} version={} pending_toll={}",
                self.pool.seed,
                self.pool.funded_claims,
                self.pool.registry_len,
                self.pool.registry_version,
                self.pool.pending_toll
            )
        }
    }

    fn is_code(error: &anchor_lang::error::Error, expected: Code) -> bool {
        matches!(
            error,
            anchor_lang::error::Error::AnchorError(e) if e.error_code_number == expected as u32 + 6000
        )
    }

    /// Deterministic generator for the property-style tests.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 11
        }
    }

    fn series_of(series: u8) -> Series {
        if series == SERIES_A {
            Series::A
        } else {
            Series::B
        }
    }

    /// Specs that are all open (`remaining >= 11`), mixing near-cap, ordinary
    /// and enormous entitlements and some prior lifetime credit.
    fn random_specs(rng: &mut Lcg, n: usize) -> Vec<Spec> {
        (0..n)
            .map(|_| {
                let series = if rng.next().is_multiple_of(3) { SERIES_B } else { SERIES_A };
                let (entitlement, lifetime) = match rng.next() % 4 {
                    0 => (11 + u128::from(rng.next() % 40), 0),
                    1 => (1_000 + u128::from(rng.next() % 100_000), 0),
                    2 => (u128::from(rng.next()) * 1_000, 0),
                    _ => {
                        let entitlement = 50 + u128::from(rng.next() % 5_000);
                        let lifetime = u128::from(rng.next()) % (entitlement - 10);
                        (entitlement, lifetime)
                    }
                };
                Spec { series, entitlement, lifetime }
            })
            .collect()
    }

    fn assert_ticket_closed(w: &World, toll: u64) {
        assert_eq!(w.ticket.credited + w.ticket.seed_credit, toll, "conservation");
    }

    // --- Pure rules ---

    #[test]
    fn batch_bounds_are_one_to_eight_and_never_overrun() {
        assert_code(batch_len(0), Code::InvalidState);
        assert_code(batch_len(9), Code::InvalidState);
        for rows in 1..=8u8 {
            assert_eq!(batch_len(rows).unwrap(), usize::from(rows));
        }
        assert_eq!(usize::from(MAX_BATCH_ROWS), PAGE_CAPACITY);
        assert_eq!(batch_end(4, 4, 8).unwrap(), 8);
        assert_code(batch_end(5, 4, 8), Code::InvalidState);
        assert_code(batch_end(u64::MAX, 1, u64::MAX), Code::Overflow);
    }

    #[test]
    fn open_means_more_than_ten_lamports_remaining() {
        assert!(!is_open(0));
        assert!(!is_open(10));
        assert!(is_open(11));
        assert!(is_open(u128::MAX));
        assert!(prune_eligible(10) && !prune_eligible(11));
    }

    #[test]
    fn the_pinned_split_drives_the_fold_and_always_conserves_the_toll() {
        assert_eq!(
            fold_slices_with_bps(10_000, DEFAULT_TIER_BPS, true, true),
            fold_slices(10_000, true, true)
        );
        let pinned = fold_slices_with_bps(10_000, (5_000, 3_000, 2_000), true, true).unwrap();
        assert_eq!((pinned.seed, pinned.senior, pinned.a, pinned.b), (5_000, 2_500, 1_500, 1_000));
        // A split above the whole Backer half is rejected rather than rounded.
        assert_eq!(fold_slices_with_bps(10_000, (6_000, 3_000, 2_000), true, true), None);
        for toll in [0u64, 1, 2, 3, 99, 10_000, 123_457, u64::MAX] {
            for (has_a, has_b) in [(true, true), (true, false), (false, true), (false, false)] {
                let s = fold_slices(toll, has_a, has_b).unwrap();
                let sum = u128::from(s.seed) + u128::from(s.senior) + u128::from(s.a) + u128::from(s.b);
                assert_eq!(sum, u128::from(toll), "toll {toll} a={has_a} b={has_b}");
            }
        }
    }

    #[test]
    fn b_residue_is_the_floor_remainder_and_shares_cap_at_the_live_remaining() {
        assert_eq!(b_split(1_000, 3).unwrap(), (999, 1));
        assert_eq!(b_split(1_000, 4).unwrap(), (1_000, 0));
        assert_eq!(b_split(7, 0).unwrap(), (0, 7));
        assert_eq!(b_split(0, 5).unwrap(), (0, 0));
        assert_eq!(cap_share(500, 11), 11);
        assert_eq!(cap_share(500, 500), 500);
        assert_eq!(cap_share(500, u128::MAX), 500);
        assert_eq!(cap_share(500, 0), 0);
    }

    #[test]
    fn prune_decision_needs_the_real_backer_unless_the_snapshot_says_open() {
        // Captured closed: certainly eligible, the real account is mandatory.
        assert!(prune_decision(10, Some(10)).unwrap());
        assert!(prune_decision(5, Some(0)).unwrap());
        assert_code(prune_decision(10, None), Code::WrongAccount);
        // Captured open: pruned only on live evidence; a placeholder skips.
        assert!(prune_decision(500, Some(10)).unwrap());
        assert!(!prune_decision(500, Some(11)).unwrap());
        assert!(!prune_decision(500, None).unwrap());
        assert!(!prune_decision(11, None).unwrap());
    }

    #[test]
    fn tally_counts_open_rows_and_tracks_the_oldest_a() {
        let mut tally = Tally::of(&Ticket::default());
        assert_eq!(tally.oldest, None);
        tally.add_open(key(5), SERIES_A, 9, 0, u128::MAX).unwrap();
        tally.add_open(key(6), SERIES_B, 1, 1, 77).unwrap();
        tally.add_open(key(4), SERIES_A, 3, 2, u128::MAX).unwrap();
        tally.add_open(key(3), SERIES_A, 5, 3, 1).unwrap();
        assert_eq!((tally.a_count, tally.b_count), (3, 1));
        // The sum is U256 and passes u128 without narrowing.
        assert_eq!(tally.a_total, U256::from(u128::MAX) * 2 + 1);
        assert_eq!(tally.oldest, Some((key(4), 3, 2)));
        assert_code(tally.add_open(key(1), 9, 0, 0, 50), Code::WrongAccount);

        let mut ticket = Ticket::default();
        tally.store(&mut ticket);
        assert_eq!((ticket.a_count, ticket.b_count), (3, 1));
        assert_eq!(decode_a_denominator(ticket.a_remainder_total), tally.a_total);
        assert_eq!((ticket.oldest, ticket.oldest_sequence, ticket.oldest_slot), (key(4), 3, 2));
        assert_eq!(Tally::of(&ticket), tally);
    }

    #[test]
    fn crediting_respects_the_cap_the_tag_and_zero() {
        let mut ticket = Ticket { id: 4, ..Ticket::default() };
        let mut backer = Backer { entitlement: 100, ..Backer::default() };
        credit_backer(&mut backer, &mut ticket, 0).unwrap();
        assert_eq!((backer.pending, backer.pending_ticket, ticket.credited), (0, 0, 0));
        credit_backer(&mut backer, &mut ticket, 60).unwrap();
        credit_backer(&mut backer, &mut ticket, 40).unwrap();
        assert_eq!((backer.lifetime, backer.pending, backer.pending_ticket), (100, 100, 4));
        assert_eq!(ticket.credited, 100);
        assert_code(credit_backer(&mut backer, &mut ticket, 1), Code::CapExceeded);
        // An earlier ticket's credit must have been materialized first.
        let mut stale = Backer { entitlement: 100, pending: 5, pending_ticket: 3, ..Backer::default() };
        assert_code(credit_backer(&mut stale, &mut ticket, 1), Code::InvalidState);
    }

    #[test]
    fn senior_credit_is_capped_at_the_oldest_remaining_and_overshoot_goes_to_seed() {
        // A1 holds 1_000 and is oldest; the senior slice at T=10,000 is 1,750.
        let mut w = World::new(&[a(1_000), a(10_000)]);
        w.start_ticket(10_000);
        w.advance_to(SEAL, 8);
        assert_eq!(w.ticket.senior_slice, 1_750);
        assert_eq!(w.ticket.senior_credit, 1_000);
        assert_eq!(w.ticket.seed_credit, 5_000 + 750);
        assert_eq!(w.backer(0).pending, 1_000);
        assert_eq!(w.backer(0).pending_ticket, w.ticket.id);
        assert_eq!(w.ticket.credited, 1_000);
        // The same pure cap helper the handler calls.
        assert_eq!(cap_senior(1_750, 1_000), Some((1_000, 750, 0)));
    }

    #[test]
    fn senior_must_be_the_tracked_oldest_open_a() {
        let mut w = World::new(&[a(100_000), a(100_000), b(100_000)]);
        w.start_ticket(10_000);
        w.advance_to(SENIOR, 8);
        let id = w.ticket.id;
        let not_oldest = w.backers[1].clone();
        assert_code(senior_step(&mut w.ticket, Some(&not_oldest), id), Code::WrongAccount);
        let a_b_series = w.backers[2].clone();
        assert_code(senior_step(&mut w.ticket, Some(&a_b_series), id), Code::WrongAccount);
        assert_code(senior_step(&mut w.ticket, Some(&placeholder()), id), Code::WrongAccount);
        // Nothing advanced.
        assert_eq!(w.ticket.phase, SENIOR);
        w.senior().unwrap();
        assert_eq!(w.ticket.phase, SEAL);
        assert_eq!(w.backer(0).pending, 1_750);
    }

    #[test]
    fn with_no_open_a_senior_reads_no_account_and_the_slice_is_already_seed() {
        let mut w = World::new(&[b(1_000_000), b(1_000_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(SENIOR, 8);
        assert_eq!((w.ticket.senior_slice, w.ticket.a_slice), (0, 0));
        // 5,000 initial seed + 1,750 senior folded to seed; B gets 1,000 + 2,250.
        assert_eq!((w.ticket.seed_credit, w.ticket.b_slice), (6_750, 3_250));
        w.senior().unwrap();
        assert_eq!((w.ticket.senior_credit, w.ticket.phase), (0, SEAL));
        w.seal().unwrap();
        // 3,250 / 3 floors to 1,083 each; the 1 lamport residue joins seed now.
        assert_eq!((w.ticket.b_slice, w.ticket.seed_credit), (3_249, 6_751));
        assert_eq!(w.ticket.cursor, 0);
        w.advance_to(COMMIT, 8);
        assert_ticket_closed(&w, 10_000);
        assert_eq!([0, 1, 2].map(|i| w.backer(i).pending), [1_083, 1_083, 1_083]);
        assert_eq!(w.ticket.seed_credit, 6_751);
    }

    #[test]
    fn seal_freezes_a_u256_denominator_and_routes_a_dead_a_slice_to_seed() {
        // Only A, fully consumed by senior: the denominator is zero.
        let mut ticket = Ticket {
            id: 7,
            phase: SEAL,
            a_count: 1,
            a_remainder_total: encode_a_denominator(U256::from(1_000u64)),
            senior_credit: 1_000,
            seed_credit: 5_750,
            a_slice: 2_250,
            ..Ticket::default()
        };
        assert_code(seal_step(&mut ticket, 6), Code::StaleTicket);
        seal_step(&mut ticket, 7).unwrap();
        assert_eq!(decode_a_denominator(ticket.sealed_a_denominator), U256::ZERO);
        assert_eq!((ticket.seed_credit, ticket.a_slice), (8_000, 0));
        assert_eq!((ticket.phase, ticket.cursor), (ALLOCATE, 0));
        assert_code(seal_step(&mut ticket, 7), Code::InvalidPhase);

        // A credit larger than the captured total is corrupt state.
        let mut corrupt = Ticket {
            id: 1,
            phase: SEAL,
            a_count: 1,
            a_remainder_total: encode_a_denominator(U256::from(10u64)),
            senior_credit: 11,
            ..Ticket::default()
        };
        assert_code(seal_step(&mut corrupt, 1), Code::InvalidState);
        // A ticket with no open row never reaches Seal.
        let mut empty = Ticket { id: 1, phase: SEAL, ..Ticket::default() };
        assert_code(seal_step(&mut empty, 1), Code::InvalidState);
    }

    // --- Goldens (spec 3.5) through the production functions ---

    #[test]
    fn golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500() {
        for rows in [1u8, 4, 8] {
            let mut w = World::new(&[a(4_000), a(6_000), b(1_000_000), b(1_000_000)]);
            w.start_ticket(10_000);
            w.advance_to(PRUNE, rows);
            assert_ticket_closed(&w, 10_000);
            w.advance_to(COMMIT, rows);
            let event = w.commit().unwrap();
            assert_eq!(w.pool.seed, 5_001, "rows {rows}");
            assert_eq!([0, 1, 2, 3].map(|i| w.backer(i).pending), [2_363, 1_636, 500, 500]);
            assert_eq!([0, 1, 2, 3].map(|i| w.backer(i).lifetime), [2_363, 1_636, 500, 500]);
            assert_eq!(w.pool.funded_claims, 4_999);
            assert_eq!(w.pool.pending_toll, 0);
            assert_eq!(w.pool.registry_len, 4, "nobody is capped");
            assert_eq!(w.pool.registry_version, 3, "no prune, no version bump");
            assert_eq!(
                event,
                TicketCommitted {
                    schema_version: SCHEMA_VERSION,
                    ticket_id: 1,
                    round_id: 1,
                    toll: 10_000,
                    credited: 4_999,
                    seed: 5_001,
                }
            );
        }
    }

    #[test]
    fn golden_no_b_a1000_a10000_seed_5750_a1000_a3250() {
        for rows in [1u8, 8] {
            let mut w = World::new(&[a(1_000), a(10_000)]);
            w.start_ticket(10_000);
            w.advance_to(PRUNE, rows);
            assert_ticket_closed(&w, 10_000);
            w.advance_to(COMMIT, rows);
            w.commit().unwrap();
            assert_eq!(w.pool.seed, 5_750);
            assert_eq!([0, 1].map(|i| w.backer(i).pending), [1_000, 3_250]);
            assert_eq!(w.pool.funded_claims, 4_250);
            // A1 is fully repaid (remaining 0): it leaves the registry with its
            // history and A2 is swap-deleted into slot 0.
            let first = w.backer(0);
            assert_eq!((first.active, first.slot), (false, 0));
            let second = w.backer(1);
            assert_eq!((second.active, second.slot), (true, 0));
            assert_eq!(w.pool.registry_len, 1);
            assert_eq!(w.registry_key(0), *w.backers[1].key);
            assert_eq!(w.registry_page(0).keys[1], Pubkey::default());
            assert_eq!(w.pool.registry_version, 4, "a prune bumps the version once");
        }
    }

    #[test]
    fn golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed() {
        let mut w = World::new(&[a(1_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        assert_ticket_closed(&w, 10_000);
        assert_eq!(decode_a_denominator(w.ticket.sealed_a_denominator), U256::ZERO);
        w.advance_to(COMMIT, 8);
        w.commit().unwrap();
        assert_eq!(w.pool.seed, 8_000);
        assert_eq!([0, 1].map(|i| w.backer(i).pending), [1_000, 1_000]);
        assert_eq!(w.pool.funded_claims, 2_000);
        assert!(!w.backer(0).active);
        assert_eq!(w.pool.registry_len, 1);
    }

    #[test]
    fn the_floor_residue_is_found_by_difference_and_is_below_the_open_a_count() {
        // Awkward weights so the A floors leave residue; nobody is capped.
        let mut w = World::new(&[a(70_000), a(90_000), a(120_000), b(555_555), b(555_555), b(555_555)]);
        w.start_ticket(123_457);
        w.advance_to(ALLOCATE, 8);
        let a_pool = w.ticket.a_slice;
        let seed_before = w.ticket.seed_credit;
        w.advance_to(PRUNE, 8);
        assert_ticket_closed(&w, 123_457);
        let a_paid: u64 = (0..3).map(|i| w.backer(i).pending).sum::<u64>() - w.ticket.senior_credit;
        let residue = w.ticket.seed_credit - seed_before;
        // Every lamport of the A pool is either paid or the residue now in seed,
        // and the pool itself is still the full constant the shares used.
        assert_eq!(a_paid + residue, a_pool);
        assert!(residue < 3);
        assert_eq!(w.ticket.a_slice, a_pool);
    }

    #[test]
    fn a_lost_lamport_beyond_the_floor_residue_fails_the_ticket() {
        let ticket =
            Ticket { toll: 100, credited: 40, seed_credit: 59, a_count: 2, ..Ticket::default() };
        assert_eq!(allocation_residue(&ticket).unwrap(), 1);
        let lost = Ticket { seed_credit: 58, ..ticket.clone() };
        assert_code(allocation_residue(&lost), Code::InvalidState);
        let over = Ticket { seed_credit: 61, ..ticket.clone() };
        assert_code(allocation_residue(&over), Code::InvalidState);
        // With no open A there is no floor residue at all.
        let no_a = Ticket { a_count: 0, ..ticket.clone() };
        assert_code(allocation_residue(&no_a), Code::InvalidState);
        let exact = Ticket { a_count: 0, seed_credit: 60, ..ticket };
        assert_eq!(allocation_residue(&exact).unwrap(), 0);
    }

    #[test]
    fn closed_rows_are_snapshotted_but_excluded_from_counts_and_credit() {
        // B2 has 5 lamports left (closed); only A1 and B1 are open.
        let mut w = World::new(&[a(100_000), b(1_000_000), b(5)]);
        w.start_ticket(10_000);
        w.advance_to(SENIOR, 8);
        assert_eq!((w.ticket.a_count, w.ticket.b_count), (1, 1));
        assert_eq!(decode_a_denominator(w.ticket.a_remainder_total), U256::from(100_000u64));
        // The closed row is still in the snapshot, so Prune can find it.
        let row = w.snapshot_row(2);
        assert_eq!((row.series, row.remaining, row.slot), (SERIES_B, 5, 2));
        w.advance_to(PRUNE, 8);
        assert_eq!(w.backer(2).pending, 0, "a closed row is never paid");
        assert_eq!(w.backer(1).pending, 1_000);
        w.prune(8, false).unwrap();
        w.commit().unwrap();
        assert!(!w.backer(2).active);
        assert_eq!(w.pool.registry_len, 2);
    }

    #[test]
    fn a_registry_of_only_closed_rows_sends_the_whole_toll_to_seed() {
        let mut w = World::new(&[a(5), b(10)]);
        w.start_ticket(10_000);
        w.discover(8).unwrap();
        assert_eq!((w.ticket.a_count, w.ticket.b_count), (0, 0));
        assert_eq!(w.ticket.phase, COMMIT);
        assert_eq!(w.ticket.seed_credit, 10_000);
        let event = w.commit().unwrap();
        assert_eq!((event.credited, event.seed), (0, 10_000));
        assert_eq!(w.pool.seed, 10_000);
    }

    #[test]
    fn discovery_accumulates_a_remainders_above_u128_in_u256() {
        // Two A rows with u128::MAX remaining, one small open row, and one
        // closed row that must not be counted.
        let mut w = World::new(&[a(u128::MAX), a(u128::MAX), a(70), a(7)]);
        w.start_ticket(10_000);
        w.advance_to(SENIOR, 8);
        let total = U256::from(u128::MAX) * 2 + 70;
        assert_eq!(w.ticket.a_count, 3);
        assert_eq!(decode_a_denominator(w.ticket.a_remainder_total), total);
        w.senior().unwrap();
        w.seal().unwrap();
        assert_eq!(w.ticket.senior_credit, 1_750);
        assert_eq!(
            decode_a_denominator(w.ticket.sealed_a_denominator),
            total - U256::from(w.ticket.senior_credit)
        );
        w.advance_to(PRUNE, 8);
        assert_ticket_closed(&w, 10_000);
        // The u128::MAX rows share nearly the whole A slice.
        assert!(w.backer(0).pending > 1_000 && w.backer(1).pending > 1_000);
    }

    // --- Cursor, stale and phase rules ---

    #[test]
    fn chunk_sizes_produce_identical_state_for_a_random_book() {
        let mut digests = Vec::new();
        for rows in [1u8, 2, 3, 4, 5, 7, 8] {
            let mut rng = Lcg(7);
            let specs = random_specs(&mut rng, 37);
            let mut w = World::new(&specs);
            w.run(123_457, rows);
            digests.push(w.digest());
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]), "{digests:#?}");
    }

    #[test]
    fn replaying_a_discover_chunk_is_a_stale_cursor_and_changes_nothing() {
        let mut w = World::new(&[a(100_000); 10]);
        w.start_ticket(10_000);
        let accounts = w.discover_accounts(2);
        let id = w.ticket.id;
        discover_batch(&mut w.ticket, &w.pool, &accounts, id, 0, 2).unwrap();
        assert_eq!((w.ticket.cursor, w.ticket.a_count), (2, 2));

        let before = w.checkpoint();
        assert_code(discover_batch(&mut w.ticket, &w.pool, &accounts, id, 0, 2), Code::StaleCursor);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &accounts, id, 5, 2), Code::StaleCursor);
        let after = w.checkpoint();
        assert_eq!(before.data, after.data, "snapshot bytes unchanged");
        assert_eq!(before.ticket.try_to_vec().unwrap(), after.ticket.try_to_vec().unwrap());
    }

    #[test]
    fn a_stale_ticket_id_is_rejected_by_every_phase_instruction() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.run(10_000, 8);
        w.start_ticket(10_000);
        let stale = 1;
        let accounts = w.discover_accounts(2);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &accounts, stale, 0, 2), Code::StaleTicket);
        assert_code(senior_step(&mut w.ticket, Some(&placeholder()), stale), Code::StaleTicket);
        assert_code(seal_step(&mut w.ticket, stale), Code::StaleTicket);
        assert_code(allocate_batch(&mut w.ticket, &[], stale, 0, 1), Code::StaleTicket);
        assert_code(
            prune_batch(&mut w.ticket, &mut w.pool, &[], stale, 0, 1),
            Code::StaleTicket,
        );
        assert_code(commit_step(&mut w.ticket, &mut w.pool, None, stale), Code::StaleTicket);
    }

    #[test]
    fn a_retired_ticket_cannot_be_driven_again() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(COMMIT, 8);
        w.commit().unwrap();
        assert_eq!((w.ticket.phase, w.pool.active_ticket, w.pool.finalized_ticket), (IDLE, 0, 1));
        let id = w.ticket.id;
        // A second commit is no longer the active ticket.
        assert_code(commit_step(&mut w.ticket, &mut w.pool, None, id), Code::StaleTicket);
        let accounts = w.discover_accounts(1);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &accounts, id, 0, 1), Code::StaleTicket);
        // The instructions without the pool see an IDLE phase.
        assert_code(senior_step(&mut w.ticket, Some(&placeholder()), id), Code::InvalidPhase);
        assert_code(seal_step(&mut w.ticket, id), Code::InvalidPhase);
        assert_code(allocate_batch(&mut w.ticket, &[], id, 0, 1), Code::InvalidPhase);
    }

    #[test]
    fn each_phase_instruction_requires_its_own_phase() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        let id = w.ticket.id;
        assert_code(senior_step(&mut w.ticket, Some(&placeholder()), id), Code::InvalidPhase);
        assert_code(seal_step(&mut w.ticket, id), Code::InvalidPhase);
        assert_code(allocate_batch(&mut w.ticket, &[], id, 0, 1), Code::InvalidPhase);
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &[], id, 0, 1), Code::InvalidPhase);
        assert_code(commit_step(&mut w.ticket, &mut w.pool, None, id), Code::InvalidPhase);
        w.advance_to(SENIOR, 8);
        let accounts = w.discover_accounts(0);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &accounts, id, 0, 1), Code::InvalidPhase);
    }

    #[test]
    fn discover_rejects_malformed_batches_and_wrong_accounts() {
        let mut w = World::new(&[a(100_000); 10]);
        w.start_ticket(10_000);
        let id = w.ticket.id;
        let two = w.discover_accounts(2);
        // Batch shape.
        assert_code(discover_batch(&mut w.ticket, &w.pool, &two, id, 0, 0), Code::InvalidState);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &two, id, 0, 9), Code::InvalidState);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &two, id, 0, 1), Code::InvalidState);
        let eight = w.discover_accounts(8);
        w.ticket.cursor = 8;
        assert_code(discover_batch(&mut w.ticket, &w.pool, &eight, id, 8, 8), Code::InvalidState);
        w.ticket.cursor = 0;
        // Rows are walked in registry order: swapping two Backers fails.
        let mut swapped = two.clone();
        swapped.swap(2, 5);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &swapped, id, 0, 2), Code::WrongAccount);
        // The wrong registry or snapshot page for the slot.
        let mut wrong_page = two.clone();
        wrong_page[0] = w.registry[1].clone();
        assert_code(discover_batch(&mut w.ticket, &w.pool, &wrong_page, id, 0, 2), Code::WrongAccount);
        let mut wrong_snapshot = two.clone();
        wrong_snapshot[1] = w.snapshots[1].clone();
        assert_code(
            discover_batch(&mut w.ticket, &w.pool, &wrong_snapshot, id, 0, 2),
            Code::WrongAccount,
        );
        // The right key under a foreign owner is not a program account.
        let mut foreign = two.clone();
        let bytes = w.backers[0].data.borrow().to_vec();
        foreign[2] = info(*w.backers[0].key, key(9), bytes);
        assert_code(discover_batch(&mut w.ticket, &w.pool, &foreign, id, 0, 2), Code::WrongOwner);
        // A registry that changed under the ticket.
        w.pool.registry_version += 1;
        assert_code(discover_batch(&mut w.ticket, &w.pool, &two, id, 0, 2), Code::StaleTicket);
        w.pool.registry_version -= 1;
        // Nothing above advanced the cursor.
        assert_eq!(w.ticket.cursor, 0);
        discover_batch(&mut w.ticket, &w.pool, &two, id, 0, 2).unwrap();
    }

    #[test]
    fn snapshot_pages_are_claimed_on_first_touch_and_older_rows_are_reset() {
        let mut w = World::new(&[a(100_000); 3]);
        // Stale garbage from a long-gone ticket on the page.
        let mut page = w.snapshot_page(0);
        page.rows[7] = SnapshotRow { key: key(9), series: SERIES_B, remaining: 99, slot: 7 };
        page.ticket_id = 0;
        write_program_account(&w.snapshots[0], &page).unwrap();

        w.start_ticket(10_000);
        w.discover(8).unwrap();
        let claimed = w.snapshot_page(0);
        assert_eq!(claimed.ticket_id, w.ticket.id);
        assert!(claimed.rows[7] == SnapshotRow::default(), "stale rows are cleared");
        assert_eq!(claimed.rows[0].slot, 0);
        assert_eq!(claimed.rows[2].key, *w.backers[2].key);

        // A tag from a LATER ticket can never be this ticket's page.
        let mut w = World::new(&[a(100_000); 3]);
        let mut page = w.snapshot_page(0);
        page.ticket_id = 99;
        write_program_account(&w.snapshots[0], &page).unwrap();
        w.start_ticket(10_000);
        assert_code(w.discover(8), Code::WrongAccount);

        let mut page = SnapshotPage { ticket_id: 3, ..SnapshotPage::default() };
        assert!(claim_snapshot_page(&mut page, 3).is_ok());
        assert!(claim_snapshot_page(&mut page, 4).is_ok());
        assert_eq!(page.ticket_id, 4);
        assert_code(claim_snapshot_page(&mut page, 3), Code::WrongAccount);
    }

    #[test]
    fn a_second_ticket_materializes_the_first_tickets_credit_exactly_once() {
        let mut w = World::new(&[a(100_000), a(100_000), b(1_000_000)]);
        w.run(10_000, 8);
        let first: Vec<u64> = (0..3).map(|i| w.backer(i).pending).collect();
        assert!(first.iter().all(|&credit| credit > 0));
        assert!((0..3).all(|i| w.backer(i).pending_ticket == 1 && w.backer(i).funded == 0));

        w.run(20_000, 4);
        for (i, credit) in first.iter().enumerate() {
            let backer = w.backer(i);
            assert_eq!(backer.funded, *credit, "ticket 1 credit folded into funded once");
            assert_eq!(backer.pending_ticket, 2);
            assert!(backer.pending > 0);
            assert_eq!(backer.lifetime, u128::from(backer.funded + backer.pending));
        }
        assert_eq!(w.snapshot_page(0).ticket_id, 2);
        // Global liability counted both tickets exactly once.
        let paid: u64 = (0..3).map(|i| w.backer(i).funded + w.backer(i).pending).sum();
        assert_eq!(w.pool.funded_claims, u128::from(paid));
        assert_eq!(w.pool.seed + paid, 30_000);
    }

    // --- Allocate rules ---

    #[test]
    fn allocate_rejects_a_duplicate_backer_before_checking_row_identity() {
        let mut w = World::new(&[a(100_000), a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(ALLOCATE, 8);
        let id = w.ticket.id;
        let mut accounts = w.allocate_accounts(2);
        // The same Backer account stands in for both independent rows.
        accounts[3] = accounts[1].clone();
        assert_code(
            allocate_batch(&mut w.ticket, &accounts, id, 0, 2),
            Code::DuplicateWritableAccount,
        );
        // A different wrong account is a WrongAccount, not a duplicate.
        let mut wrong = w.allocate_accounts(2);
        wrong[3] = w.backers[2].clone();
        assert_code(allocate_batch(&mut w.ticket, &wrong, id, 0, 2), Code::WrongAccount);
        assert_eq!(w.ticket.cursor, 0);
    }

    #[test]
    fn allocate_rejects_stale_cursors_bad_shapes_and_foreign_snapshots() {
        let mut w = World::new(&[a(100_000), a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(ALLOCATE, 8);
        let id = w.ticket.id;
        let accounts = w.allocate_accounts(2);
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 1, 2), Code::StaleCursor);
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 0, 0), Code::InvalidState);
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 0, 3), Code::InvalidState);
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 0, 4), Code::InvalidState);
        // A snapshot page tagged by another ticket is not this ticket's capture.
        let mut page = w.snapshot_page(0);
        page.ticket_id = id + 1;
        write_program_account(&w.snapshots[0], &page).unwrap();
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 0, 2), Code::WrongAccount);
        page.ticket_id = id;
        write_program_account(&w.snapshots[0], &page).unwrap();
        allocate_batch(&mut w.ticket, &accounts, id, 0, 2).unwrap();
        assert_eq!(w.ticket.cursor, 2);
        // A completed batch cannot be replayed into a second credit.
        let before = w.backer(0).pending;
        assert_code(allocate_batch(&mut w.ticket, &accounts, id, 0, 2), Code::StaleCursor);
        assert_eq!(w.backer(0).pending, before);
    }

    #[test]
    fn a_share_above_the_live_cap_is_capped_and_the_overshoot_joins_seed() {
        // B1 has only 40 left: its 1,000 share is capped at 40.
        let mut w = World::new(&[a(100_000), b(40), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(ALLOCATE, 8);
        // Two open B rows: 1,000 / 2 = 500 each; B1 is capped at 40.
        assert_eq!((w.ticket.b_count, w.ticket.b_slice), (2, 1_000));
        let seed_before = w.ticket.seed_credit;
        w.advance_to(PRUNE, 8);
        assert_ticket_closed(&w, 10_000);
        assert_eq!(w.backer(1).pending, 40);
        assert_eq!(w.backer(2).pending, 500);
        assert!(w.ticket.seed_credit >= seed_before + 460, "B overshoot reached seed");
        // B1 now has 0 remaining: newly capped, so Prune removes it.
        w.prune(8, false).unwrap();
        w.commit().unwrap();
        assert!(!w.backer(1).active);
        assert_eq!(w.pool.registry_len, 2);
    }

    // --- Prune rules ---

    #[test]
    fn prune_same_page_swap_delete_clears_the_last_slot_on_the_first_page() {
        // Regression for the draft: pruned slot 1 and last slot 3 share page 0.
        for rows in [1u8, 8] {
            let mut w = World::new(&[a(100_000), a(5), a(100_000), a(100_000)]);
            w.start_ticket(10_000);
            w.advance_to(PRUNE, 8);
            assert_eq!(w.ticket.cursor, 4);
            w.prune(rows, false).unwrap();
            assert_eq!(w.ticket.phase, COMMIT);
            let page = w.registry_page(0);
            assert_eq!(page.keys[1], *w.backers[3].key, "a4 moved into slot 1");
            assert_eq!(page.keys[3], Pubkey::default());
            assert_eq!(w.backer(3).slot, 1);
            assert!(w.backer(3).active);
            // Regression for the inverted alias write: the pruned row is the
            // one that goes inactive, not the moved one.
            let pruned = w.backer(1);
            assert_eq!((pruned.active, pruned.slot), (false, 0));
            assert_eq!(w.pool.registry_len, 3);
            w.commit().unwrap();
            assert_eq!(w.pool.registry_version, 4);
        }
    }

    impl World {
        /// Run a Prune batch that must fail with `expected`, then undo whatever
        /// it wrote (on chain the whole transaction reverts).
        fn prune_fails(&mut self, accounts: &[AccountInfo<'static>], take: u8, expected: Code) {
            let (id, cursor) = (self.ticket.id, self.ticket.cursor);
            let checkpoint = self.checkpoint();
            let result = prune_batch(&mut self.ticket, &mut self.pool, accounts, id, cursor, take);
            assert_code(result, expected);
            self.restore(checkpoint);
        }
    }

    #[test]
    fn prune_alias_when_the_pruned_row_is_the_last_row_marks_it_inactive() {
        let mut w = World::new(&[a(100_000), a(5)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let accounts = w.prune_accounts(2, false);
        // Row 0 (slot 1, the last live slot) names the pruned account as `moved`.
        assert_eq!(accounts[4].key, accounts[1].key);
        let mut wrong = accounts.clone();
        wrong[4] = w.backers[0].clone();
        w.prune_fails(&wrong, 2, Code::WrongAccount);

        w.prune(8, false).unwrap();
        assert_eq!(w.ticket.phase, COMMIT);
        let pruned = w.backer(1);
        assert_eq!((pruned.active, pruned.slot), (false, 0));
        let survivor = w.backer(0);
        assert_eq!((survivor.active, survivor.slot), (true, 0), "no moved write");
        assert_eq!(w.registry_page(0).keys[0], *w.backers[0].key);
        assert_eq!(w.registry_page(0).keys[1], Pubkey::default());
        assert_eq!(w.pool.registry_len, 1);
    }

    #[test]
    fn prune_across_pages_decodes_each_registry_page_once_and_moves_the_last_row_down() {
        // Ten rows: slot 5 (page 0) is closed and the last live slot 9 is on page 1.
        let mut specs = vec![a(100_000); 10];
        specs[5] = a(5);
        let mut w = World::new(&specs);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let accounts = w.prune_accounts(8, false);
        // The wrong page for the pruned slot is rejected.
        let mut wrong = accounts.clone();
        let row = (0..8).find(|row| accounts[row * 5 + 1].key != &PRUNE_PLACEHOLDER).unwrap();
        wrong[row * 5 + 2] = w.registry[1].clone();
        w.prune_fails(&wrong, 8, Code::WrongAccount);

        w.prune(8, false).unwrap();
        assert_eq!(w.registry_page(0).keys[5], *w.backers[9].key);
        assert_eq!(w.registry_page(1).keys[1], Pubkey::default());
        assert_eq!(w.backer(9).slot, 5);
        assert_eq!(w.pool.registry_len, 9);
        assert!(!w.backer(5).active);
    }

    #[test]
    fn prune_chained_moves_are_a_duplicate_batch_but_legal_one_row_at_a_time() {
        // [closed, closed, open]: pruning slot 1 moves x2 down, then pruning
        // slot 0 would move the SAME Backer again within the batch.
        let mut w = World::new(&[a(5), a(5), a(100_000)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let accounts = w.prune_accounts(3, false);
        w.prune_fails(&accounts, 3, Code::DuplicateWritableAccount);

        w.prune(1, false).unwrap();
        assert_eq!(w.pool.registry_len, 1);
        assert_eq!(w.registry_key(0), *w.backers[2].key);
        let survivor = w.backer(2);
        assert_eq!((survivor.active, survivor.slot), (true, 0));
        assert!(!w.backer(0).active && !w.backer(1).active);
    }

    #[test]
    fn prune_with_the_fallback_split_matches_the_single_batch_result() {
        let mut rng = Lcg(99);
        let mut specs = random_specs(&mut rng, 20);
        // Make several rows prunable by leaving only a few lamports.
        for index in [2, 3, 7, 8, 15, 19] {
            specs[index].entitlement = 5;
            specs[index].lifetime = 0;
        }
        let mut digests = Vec::new();
        for rows in [1u8, 3, 8] {
            let mut w = World::new(&specs);
            w.run(500_000, rows);
            digests.push(w.digest());
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]), "{digests:#?}");
    }

    #[test]
    fn prune_rejects_the_same_pruned_backer_for_two_rows() {
        let mut w = World::new(&[a(5), a(5), a(100_000)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let mut accounts = w.prune_accounts(3, false);
        // Rows are slot 2 (skipped), slot 1, slot 0.  Reuse slot 1's account.
        accounts[11] = accounts[6].clone();
        w.prune_fails(&accounts, 3, Code::DuplicateWritableAccount);
    }

    #[test]
    fn a_closed_row_needs_its_real_accounts_but_an_open_row_may_be_skipped() {
        let mut w = World::new(&[a(100_000), a(5)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let mut accounts = w.prune_accounts(2, false);
        // Row 0 is slot 1 (captured closed): placeholders are refused.
        for account in &mut accounts[1..5] {
            *account = placeholder();
        }
        w.prune_fails(&accounts, 2, Code::WrongAccount);
        // Row 1 (slot 0, open) is already all placeholders and is skipped.
        let ok = w.prune_accounts(2, false);
        assert!(ok[6..10].iter().all(|account| account.key == &PRUNE_PLACEHOLDER));
        w.prune(2, false).unwrap();
        assert!(w.backer(0).active);
    }

    #[test]
    fn a_newly_capped_row_is_pruned_when_supplied_and_survives_when_skipped() {
        for skip in [false, true] {
            // One A with 30 lamports: senior takes all 30, leaving 0 remaining.
            let mut w = World::new(&[a(30)]);
            w.start_ticket(10_000);
            w.advance_to(PRUNE, 8);
            assert_ticket_closed(&w, 10_000);
            assert_eq!((w.ticket.credited, w.ticket.seed_credit), (30, 9_970));
            assert_eq!(w.backer(0).remaining().unwrap(), 0);
            w.prune(8, skip).unwrap();
            w.commit().unwrap();
            if skip {
                assert!(w.backer(0).active);
                assert_eq!((w.pool.registry_len, w.pool.registry_version), (1, 3));
            } else {
                assert!(!w.backer(0).active);
                assert_eq!((w.pool.registry_len, w.pool.registry_version), (0, 4));
            }
        }
    }

    #[test]
    fn prune_rejects_bad_shapes_and_stale_cursors() {
        let mut w = World::new(&[a(100_000), a(5)]);
        w.start_ticket(10_000);
        w.advance_to(PRUNE, 8);
        let (id, cursor) = (w.ticket.id, w.ticket.cursor);
        let one = w.prune_accounts(1, false);
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &one, id, cursor - 1, 1), Code::StaleCursor);
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &one, id, cursor, 0), Code::InvalidState);
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &one, id, cursor, 9), Code::InvalidState);
        // More rows than the cursor has left.
        let many = w.prune_accounts(2, false);
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &many, id, cursor, 3), Code::InvalidState);
        // Account count must match the row count.
        assert_code(prune_batch(&mut w.ticket, &mut w.pool, &one, id, cursor, 2), Code::InvalidState);
        assert_eq!(w.ticket.cursor, cursor);
    }

    // --- Commit ---

    #[test]
    fn commit_enforces_conservation_and_books_the_ticket_exactly_once() {
        let mut w = World::new(&[a(4_000), a(6_000), b(1_000_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(COMMIT, 8);
        let id = w.ticket.id;

        // A ticket that lost a lamport is not committable.
        let mut tampered = w.ticket.clone();
        tampered.credited += 1;
        assert_code(commit_step(&mut tampered, &mut w.pool.clone(), None, id), Code::InvalidState);
        let mut poor = w.pool.clone();
        poor.pending_toll = 9_999;
        assert_code(commit_step(&mut w.ticket.clone(), &mut poor, None, id), Code::InvalidState);

        let before = w.pool.clone();
        let event = w.commit().unwrap();
        assert_eq!(w.pool.pending_toll, before.pending_toll - 10_000);
        assert_eq!(w.pool.seed, before.seed + 5_001);
        assert_eq!(w.pool.funded_claims, before.funded_claims + 4_999);
        assert_eq!((w.pool.finalized_ticket, w.pool.active_ticket, w.ticket.phase), (1, 0, IDLE));
        // The books moved between partitions; their sum did not change.
        assert_eq!(crate::game_accounted(&before).unwrap(), crate::game_accounted(&w.pool).unwrap());
        assert_eq!((event.toll, event.credited, event.seed), (10_000, 4_999, 5_001));
        assert_eq!(event.credited + event.seed, event.toll);
        assert_code(w.commit(), Code::StaleTicket);
    }

    #[test]
    fn a_closing_ticket_requires_its_close_record_and_marks_cash_committed() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.ticket.close_id = Some(5);
        w.advance_to(COMMIT, 8);
        let id = w.ticket.id;
        let (address, bump) =
            Pubkey::find_program_address(&[CLOSE_SEED, &5u64.to_le_bytes()], &crate::ID);
        let record = || CloseRecord {
            bump,
            close_id: 5,
            closing_ticket_id: Some(id),
            ..CloseRecord::default()
        };
        let attempt = |w: &World, close: Option<(Pubkey, &mut CloseRecord)>| {
            commit_step(&mut w.ticket.clone(), &mut w.pool.clone(), close, id)
        };

        assert_code(attempt(&w, None), Code::WrongAccount);
        let mut wrong_key = record();
        assert_code(attempt(&w, Some((key(1), &mut wrong_key))), Code::WrongAccount);
        let mut wrong_id = record();
        wrong_id.close_id = 6;
        assert_code(attempt(&w, Some((address, &mut wrong_id))), Code::WrongAccount);
        let mut other_ticket = record();
        other_ticket.closing_ticket_id = Some(id + 1);
        assert_code(attempt(&w, Some((address, &mut other_ticket))), Code::InvalidState);
        let mut already = record();
        already.cash_committed = true;
        assert_code(attempt(&w, Some((address, &mut already))), Code::InvalidState);
        assert!(!wrong_key.cash_committed && !other_ticket.cash_committed);

        let mut good = record();
        let id = w.ticket.id;
        commit_step(&mut w.ticket, &mut w.pool, Some((address, &mut good)), id).unwrap();
        assert!(good.cash_committed);
        assert_eq!(w.ticket.phase, IDLE);
    }

    #[test]
    fn a_non_closing_ticket_ignores_a_supplied_close_record() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.start_ticket(10_000);
        w.advance_to(COMMIT, 8);
        let id = w.ticket.id;
        let mut record = CloseRecord::default();
        commit_step(&mut w.ticket, &mut w.pool, Some((key(2), &mut record)), id).unwrap();
        assert!(!record.cash_committed);
    }

    #[test]
    fn registry_version_moves_only_when_the_registry_shrank() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.run(10_000, 8);
        assert_eq!(w.pool.registry_version, 3);
        let mut w = World::new(&[a(100_000), a(5)]);
        w.run(10_000, 8);
        assert_eq!((w.pool.registry_version, w.pool.registry_len), (4, 1));
        // The next ticket captures the new version and length.
        w.start_ticket(10_000);
        assert_eq!((w.ticket.expected_version, w.ticket.captured_len), (4, 1));
    }

    #[test]
    fn a_toll_near_u64_max_conserves_without_overflow() {
        let toll = u64::MAX / 2;
        let mut w = World::new(&[a(u128::MAX), a(u128::MAX), b(u128::MAX), b(u128::MAX)]);
        w.start_ticket(toll);
        w.advance_to(PRUNE, 8);
        assert_ticket_closed(&w, toll);
        w.advance_to(COMMIT, 8);
        let event = w.commit().unwrap();
        assert_eq!(u128::from(event.credited) + u128::from(event.seed), u128::from(toll));
        assert_eq!(w.pool.pending_toll, 0);
    }

    // --- Inline path (the whole ticket inside one payout instruction) ---

    /// Account bytes, ticket, pool and digest all equal: the inline run left
    /// exactly the state the stepwise ticket leaves.
    fn assert_same_state(inline: &World, stepwise: &World, at: &str) {
        let (a, b) = (inline.checkpoint(), stepwise.checkpoint());
        assert_eq!(a.data, b.data, "{at}: registry / snapshot / backer account bytes");
        assert_eq!(a.ticket.try_to_vec().unwrap(), b.ticket.try_to_vec().unwrap(), "{at}: ticket");
        assert_eq!(a.pool.try_to_vec().unwrap(), b.pool.try_to_vec().unwrap(), "{at}: pool");
        assert_eq!(inline.digest(), stepwise.digest(), "{at}: digest");
    }

    /// Run the same tolls through an inline world and a chunked world (the
    /// real six-phase functions at `rows` per call) and compare after each.
    fn assert_inline_equals_stepwise(specs: &[Spec], tolls: &[u64], rows: u8, at: &str) {
        let mut inline = World::new(specs);
        let mut stepwise = World::new(specs);
        for (turn, &toll) in tolls.iter().enumerate() {
            let at = format!("{at} turn {turn} toll {toll}");
            if stepwise.pool.registry_len == 0 {
                // No Backer left: the toll goes straight to seed, no ticket.
                inline.pool.seed += toll;
                stepwise.pool.seed += toll;
                continue;
            }
            let expected = stepwise.run(toll, rows);
            let (id, event) = inline.begin_inline(toll).unwrap_or_else(|e| panic!("{at}: {e:?}"));
            assert_eq!(id, expected.ticket_id, "{at}: ticket id");
            assert_eq!(event, Some(expected), "{at}: TicketCommitted");
            assert_same_state(&inline, &stepwise, &at);
            assert_eq!((inline.ticket.phase, inline.pool.active_ticket), (IDLE, 0), "{at}");
            assert_eq!(inline.pool.pending_toll, 0, "{at}: escrow released");
            assert_eq!(inline.pool.finalized_ticket, id, "{at}");
        }
    }

    /// Open, closed-at-capture, just-open (capped by the first credit) and
    /// enormous rows, some with prior lifetime credit.
    fn mixed_specs(rng: &mut Lcg, n: usize) -> Vec<Spec> {
        (0..n)
            .map(|_| {
                let series = if rng.next().is_multiple_of(3) { SERIES_B } else { SERIES_A };
                let (entitlement, lifetime) = match rng.next() % 5 {
                    0 => (1 + u128::from(rng.next() % 10), 0),
                    1 => (11 + u128::from(rng.next() % 50), 0),
                    2 => (1_000 + u128::from(rng.next() % 100_000), 0),
                    3 => (u128::from(rng.next()) * 1_000, 0),
                    _ => {
                        let entitlement = 50 + u128::from(rng.next() % 5_000);
                        (entitlement, u128::from(rng.next()) % (entitlement - 10))
                    }
                };
                Spec { series, entitlement, lifetime }
            })
            .collect()
    }

    #[test]
    fn inline_golden_two_a_two_b_seed_5001_a2363_a1636_b500_b500() {
        let specs = [a(4_000), a(6_000), b(1_000_000), b(1_000_000)];
        let mut w = World::new(&specs);
        let event = w.run_inline(10_000);
        assert_eq!(
            event,
            TicketCommitted {
                schema_version: SCHEMA_VERSION,
                ticket_id: 1,
                round_id: 1,
                toll: 10_000,
                credited: 4_999,
                seed: 5_001,
            }
        );
        assert_eq!(w.pool.seed, 5_001);
        assert_eq!([0, 1, 2, 3].map(|i| w.backer(i).pending), [2_363, 1_636, 500, 500]);
        assert_eq!([0, 1, 2, 3].map(|i| w.backer(i).lifetime), [2_363, 1_636, 500, 500]);
        assert_eq!((w.pool.funded_claims, w.pool.pending_toll), (4_999, 0));
        assert_eq!((w.pool.registry_len, w.pool.registry_version), (4, 3));
        assert_eq!((w.ticket.phase, w.pool.active_ticket, w.pool.finalized_ticket), (IDLE, 0, 1));
        assert_eq!(w.pool.next_ticket, 2);
        for rows in [1u8, 4, 8] {
            assert_inline_equals_stepwise(&specs, &[10_000], rows, "m6 golden");
        }
    }

    #[test]
    fn inline_golden_no_b_repays_and_prunes_the_senior_row() {
        let specs = [a(1_000), a(10_000)];
        let mut w = World::new(&specs);
        w.run_inline(10_000);
        assert_eq!(w.pool.seed, 5_750);
        assert_eq!([0, 1].map(|i| w.backer(i).pending), [1_000, 3_250]);
        assert_eq!(w.pool.funded_claims, 4_250);
        // A1 is fully repaid: it leaves the registry and A2 swaps into slot 0.
        assert_eq!((w.backer(0).active, w.backer(0).slot), (false, 0));
        assert_eq!((w.backer(1).active, w.backer(1).slot), (true, 0));
        assert_eq!((w.pool.registry_len, w.pool.registry_version), (1, 4));
        assert_eq!(w.registry_key(0), *w.backers[1].key);
        assert_eq!(w.registry_page(0).keys[1], Pubkey::default());
        assert_inline_equals_stepwise(&specs, &[10_000], 8, "no-b golden");
    }

    #[test]
    fn inline_golden_dead_a_pool_goes_to_seed() {
        let specs = [a(1_000), b(1_000_000)];
        let mut w = World::new(&specs);
        w.run_inline(10_000);
        assert_eq!(decode_a_denominator(w.ticket.sealed_a_denominator), U256::ZERO);
        assert_eq!(w.pool.seed, 8_000);
        assert_eq!([0, 1].map(|i| w.backer(i).pending), [1_000, 1_000]);
        assert_eq!(w.pool.funded_claims, 2_000);
        assert!(!w.backer(0).active);
        assert_eq!(w.pool.registry_len, 1);
        assert_inline_equals_stepwise(&specs, &[10_000], 8, "dead-a golden");
    }

    #[test]
    fn inline_a_registry_of_only_closed_rows_sends_the_whole_toll_to_seed() {
        let specs = [a(5), b(10)];
        let mut w = World::new(&specs);
        let event = w.run_inline(10_000);
        assert_eq!((event.credited, event.seed), (0, 10_000));
        assert_eq!(w.pool.seed, 10_000);
        // Straight Discover -> Commit: the closed rows wait for a later ticket.
        assert_eq!((w.pool.registry_len, w.pool.registry_version), (2, 3));
        assert!(w.backer(0).active && w.backer(1).active);
        assert_inline_equals_stepwise(&specs, &[10_000, 777], 8, "all closed");
    }

    #[test]
    fn inline_matches_the_stepwise_ticket_for_every_population_from_one_to_eight() {
        let tolls = [2u64, 3, 99, 10_000, 123_457, 3_000_000];
        for n in 1..=8usize {
            for seed in 0..24u64 {
                let mut rng = Lcg(seed * 8 + n as u64);
                let specs = mixed_specs(&mut rng, n);
                for &toll in &tolls {
                    for rows in [1u8, 8] {
                        assert_inline_equals_stepwise(
                            &specs,
                            &[toll],
                            rows,
                            &format!("n {n} seed {seed} rows {rows}"),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn inline_matches_the_stepwise_ticket_for_a_full_registry_of_capped_rows() {
        // Eight rows: closed at capture, capped by their own first credit, and
        // ordinary, so Prune removes several in swap-delete order.
        let specs = [a(5), a(100_000), a(30), b(40), b(5), a(100_000), b(1_000_000), a(11)];
        for rows in [1u8, 3, 8] {
            assert_inline_equals_stepwise(&specs, &[10_000, 50_000, 123_457], rows, "capped eight");
        }
        let mut w = World::new(&specs);
        w.run_inline(10_000);
        assert_ticket_closed(&w, 10_000);
        assert!(w.pool.registry_len < 8, "something was pruned");
        assert!(w.pool.registry_version > 3);
    }

    #[test]
    fn inline_sequences_repeat_and_mix_with_chunked_tickets() {
        let mut rng = Lcg(0x1217);
        for case in 0..40 {
            let n = 1 + (rng.next() % 8) as usize;
            let specs = mixed_specs(&mut rng, n);
            let tolls: Vec<u64> = (0..4).map(|_| 2 + rng.next() % 2_000_000).collect();
            // Reference: every ticket chunked.
            let mut stepwise = World::new(&specs);
            // Mixed: inline when the choice bit is set, chunked otherwise.
            let mut mixed = World::new(&specs);
            for (turn, &toll) in tolls.iter().enumerate() {
                let at = format!("case {case} turn {turn}");
                if stepwise.pool.registry_len == 0 {
                    stepwise.pool.seed += toll;
                    mixed.pool.seed += toll;
                    continue;
                }
                stepwise.run(toll, 5);
                if rng.next().is_multiple_of(2) {
                    mixed.run_inline(toll);
                } else {
                    mixed.run(toll, 3);
                }
                assert_same_state(&mixed, &stepwise, &at);
            }
        }
    }

    #[test]
    fn inline_without_accounts_is_exactly_the_chunked_start() {
        let specs = [a(100_000), a(5), b(1_000_000)];
        let mut by_world = World::new(&specs);
        by_world.start_ticket(10_000);
        let mut by_begin = World::new(&specs);
        let (id, event) = crate::positions::begin_ticket(
            &mut by_begin.ticket,
            &mut by_begin.pool,
            None,
            10_000,
            &ConfigVersion::default_v1(),
            None,
        )
        .unwrap();
        assert_eq!((id, event), (1, None));
        assert_same_state(&by_begin, &by_world, "start only");
        assert_eq!((by_begin.ticket.phase, by_begin.pool.active_ticket), (DISCOVER, 1));
        assert_eq!(by_begin.pool.pending_toll, 10_000);
        // A closing payout's ticket keeps its close id and is never inline.
        let mut closing = World::new(&specs);
        crate::positions::begin_ticket(
            &mut closing.ticket,
            &mut closing.pool,
            Some(9),
            10_000,
            &ConfigVersion::default_v1(),
            None,
        )
        .unwrap();
        assert_eq!((closing.ticket.close_id, closing.ticket.phase), (Some(9), DISCOVER));
    }

    #[test]
    fn inline_layout_is_one_backer_per_slot_then_the_two_pages() {
        let w = World::new(&[a(100_000), b(1_000_000), a(100_000)]);
        let ok = w.inline_accounts();
        assert_eq!(ok.len(), 3 + INLINE_PAGE_ACCOUNTS);
        assert_eq!(inline_layout(3, &ok).unwrap(), 3);
        // A partial layout: a Backer, a page, or everything missing / extra.
        assert_code(inline_layout(3, &ok[1..]), Code::WrongAccount);
        assert_code(inline_layout(3, &ok[..4]), Code::WrongAccount);
        assert_code(inline_layout(3, &[]), Code::WrongAccount);
        let mut extra = ok.clone();
        extra.push(w.backers[0].clone());
        assert_code(inline_layout(3, &extra), Code::WrongAccount);
        // A repeated account is a duplicate writable, named as such.
        let mut twice = ok.clone();
        twice[1] = twice[0].clone();
        assert_code(inline_layout(3, &twice), Code::DuplicateWritableAccount);
        let mut page_twice = ok.clone();
        page_twice[4] = page_twice[3].clone();
        assert_code(inline_layout(3, &page_twice), Code::DuplicateWritableAccount);
        // Every inline account is written, so a read-only one is refused.
        let mut read_only = ok.clone();
        let data = read_only[2].data.borrow().to_vec();
        read_only[2] = AccountInfo::new(
            Box::leak(Box::new(*ok[2].key)),
            false,
            false,
            Box::leak(Box::new(1u64)),
            Box::leak(data.into_boxed_slice()),
            Box::leak(Box::new(crate::ID)),
            false,
            0,
        );
        assert_code(inline_layout(3, &read_only), Code::WrongAccount);
    }

    #[test]
    fn inline_is_offered_for_one_to_eight_rows_and_never_past_one_page() {
        let w = World::new(&[a(100_000); 8]);
        let ok = w.inline_accounts();
        assert_eq!(INLINE_THRESHOLD, PAGE_CAPACITY as u64);
        assert_eq!(inline_layout(INLINE_THRESHOLD, &ok).unwrap(), 8);
        assert_eq!(inline_layout(1, &ok[7..]).unwrap(), 1);
        // Nine rows are a second page: the keeper must use chunks, whatever
        // accounts were supplied.
        let nine = World::new(&[a(100_000); 9]);
        let mut accounts: Vec<_> = nine.backers.clone();
        accounts.push(nine.registry[0].clone());
        accounts.push(nine.snapshots[0].clone());
        assert_code(inline_layout(9, &accounts), Code::InvalidState);
        assert_code(inline_layout(0, &ok[8..]), Code::InvalidState);
        assert_code(inline_layout(u64::MAX, &ok), Code::InvalidState);
    }

    #[test]
    fn inline_rejects_a_wrong_layout_before_it_can_commit() {
        let specs = [a(100_000), a(100_000), b(1_000_000), a(100_000)];
        let mut w = World::new(&specs);
        let attempt = |w: &mut World, accounts: Vec<AccountInfo<'static>>, expected: Code| {
            let checkpoint = w.checkpoint();
            let result = crate::positions::begin_ticket(
                &mut w.ticket,
                &mut w.pool,
                None,
                10_000,
                &ConfigVersion::default_v1(),
                Some(&accounts),
            );
            assert_code(result, expected);
            // On chain the whole payout reverts; undo what the host run wrote.
            w.restore(checkpoint);
        };
        let ok = w.inline_accounts();

        // Backers out of registry order.
        let mut swapped = ok.clone();
        swapped.swap(0, 1);
        attempt(&mut w, swapped, Code::WrongAccount);
        // Registry page where the snapshot page belongs, and the reverse.
        let mut pages_swapped = ok.clone();
        pages_swapped.swap(4, 5);
        attempt(&mut w, pages_swapped, Code::WrongAccount);
        // A Backer account standing in for the snapshot page.
        let stranger = World::new(&[a(100_000), a(100_000), b(1_000_000), a(100_000), a(7)]);
        let mut backer_as_page = ok.clone();
        backer_as_page[5] = stranger.backers[4].clone();
        attempt(&mut w, backer_as_page, Code::WrongAccount);
        // An account of another program under a real Backer's key.
        let mut foreign = ok.clone();
        let bytes = w.backers[2].data.borrow().to_vec();
        foreign[2] = info(*w.backers[2].key, key(9), bytes);
        attempt(&mut w, foreign, Code::WrongOwner);
        // A Backer from outside this registry.
        let mut outsider = ok.clone();
        outsider[3] = stranger.backers[4].clone();
        attempt(&mut w, outsider, Code::WrongAccount);

        // The same call with the right layout succeeds.
        let (id, event) = w.begin_inline(10_000).unwrap();
        assert_eq!(event.map(|event| event.ticket_id), Some(id));
    }

    #[test]
    fn a_retired_inline_ticket_cannot_be_run_again() {
        let mut w = World::new(&[a(100_000), b(1_000_000)]);
        w.run_inline(10_000);
        let accounts = w.inline_accounts();
        assert_code(run_inline_ticket(&mut w.ticket, &mut w.pool, &accounts), Code::StaleTicket);
    }

    #[test]
    fn random_inline_sequences_match_the_host_book() {
        let mut rng = Lcg(0x001A_11CE);
        for case in 0..120 {
            let n = 1 + (rng.next() % 8) as usize;
            let specs = random_specs(&mut rng, n);
            let tolls: Vec<u64> = (0..1 + rng.next() % 3).map(|_| 2 + rng.next() % 3_000_000).collect();

            let mut book = Book::with_pot(u64::MAX / 2);
            for (index, spec) in specs.iter().enumerate() {
                book.add_backer_with_lifetime(
                    index as u64,
                    series_of(spec.series),
                    spec.entitlement,
                    spec.lifetime,
                )
                .unwrap();
            }
            let mut w = World::new(&specs);
            for (turn, &toll) in tolls.iter().enumerate() {
                book.allocate(toll, 8).unwrap();
                if w.pool.registry_len == 0 {
                    w.pool.seed += toll;
                } else {
                    w.run_inline(toll);
                }
                for (index, spec) in specs.iter().enumerate() {
                    let at = format!("case {case} turn {turn} backer {index}");
                    let expected =
                        book.backer(BackerKey::new(index as u64, series_of(spec.series))).unwrap();
                    let mine = w.backer(index);
                    assert_eq!(mine.pending, expected.pending, "{at} pending");
                    assert_eq!(mine.funded, expected.funded, "{at} funded");
                    assert_eq!(mine.lifetime, expected.lifetime_credited, "{at} lifetime");
                    let slot = if mine.active { Some(mine.slot as usize) } else { None };
                    assert_eq!(slot, expected.active_slot, "{at} slot");
                }
                assert_eq!(w.pool.seed, book.seed(), "case {case} turn {turn} seed");
                assert_eq!(w.pool.funded_claims, book.global_funded_claims(), "case {case} turn {turn}");
                assert_eq!(w.pool.pending_toll, 0, "case {case} turn {turn} escrow");
            }
        }
    }

    // --- Differential against the host Book ---

    #[test]
    fn random_ticket_sequences_match_the_host_book() {
        let mut rng = Lcg(0x00C0_FFEE);
        for case in 0..120 {
            let n = 1 + (rng.next() % 24) as usize;
            let specs = random_specs(&mut rng, n);
            let tolls: Vec<u64> = (0..1 + rng.next() % 3).map(|_| 2 + rng.next() % 3_000_000).collect();
            let rows = [1u8, 3, 5, 8][(rng.next() % 4) as usize];

            let mut book = Book::with_pot(u64::MAX / 2);
            for (index, spec) in specs.iter().enumerate() {
                book.add_backer_with_lifetime(
                    index as u64,
                    series_of(spec.series),
                    spec.entitlement,
                    spec.lifetime,
                )
                .unwrap();
            }
            let mut w = World::new(&specs);
            for (turn, &toll) in tolls.iter().enumerate() {
                book.allocate(toll, usize::from(rows)).unwrap();
                if w.pool.registry_len == 0 {
                    // No open Backer left: a toll goes straight to seed, no ticket.
                    w.pool.seed += toll;
                } else {
                    w.run(toll, rows);
                }

                for (index, spec) in specs.iter().enumerate() {
                    let at = format!("case {case} turn {turn} backer {index}");
                    let expected =
                        book.backer(BackerKey::new(index as u64, series_of(spec.series))).unwrap();
                    let mine = w.backer(index);
                    // The Book folds an earlier ticket's credit lazily at the next
                    // credit or claim, exactly as the program does.
                    assert_eq!(mine.pending, expected.pending, "{at} pending");
                    assert_eq!(mine.funded, expected.funded, "{at} funded");
                    assert_eq!(mine.lifetime, expected.lifetime_credited, "{at} lifetime");
                    let slot = if mine.active { Some(mine.slot as usize) } else { None };
                    assert_eq!(slot, expected.active_slot, "{at} slot");
                }
                assert_eq!(w.pool.seed, book.seed(), "case {case} turn {turn} seed");
                assert_eq!(
                    w.pool.funded_claims,
                    book.global_funded_claims(),
                    "case {case} turn {turn} funded claims"
                );
                assert_eq!(w.pool.pending_toll, 0, "case {case} turn {turn} escrow");
            }
        }
    }
}
