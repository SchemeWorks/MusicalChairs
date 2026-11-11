//! Host-side reference model for serialized native-SOL Backer allocation.
//!
//! The model deliberately keeps the complete state in ordinary Rust values. It
//! is a proof and vector oracle for the eventual account program, not an
//! instruction handler. Each mutating operation stages a clone and commits it
//! only after all validation and checked arithmetic succeeds. That is O(N) in
//! this host model, while the program design uses account-local preflight and
//! bounded cursor transactions.

use ethnum::U256;
use std::fmt;

/// A native wallet identifier in the host model.
pub type Owner = u64;

/// The two source Backer series.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Series {
    A,
    B,
}

/// Persistent identity for one owner's Backer row.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BackerKey {
    pub owner: Owner,
    pub series: Series,
}

impl BackerKey {
    pub const fn new(owner: Owner, series: Series) -> Self {
        Self { owner, series }
    }
}

/// Allocation phases. Discover, Allocate and Prune are cursor phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Discover,
    Senior,
    SealWeights,
    Allocate,
    Prune,
    Commit,
}

/// Errors returned by the model. Every error is intended to be recoverable by
/// leaving the input state unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    ArithmeticOverflow,
    InsufficientPot,
    InsufficientEscrow,
    TicketActive,
    NoTicket,
    ZeroToll,
    DuplicateBacker,
    MissingBacker,
    InvalidRow,
    InvalidCursor,
    InvalidPhase,
    ReplayedRow,
    BackerCapped,
    PendingTicket,
    ClosingAdmission,
    AllocationBarrier,
    ClaimTooLarge,
    ConservationMismatch,
    ZeroDenominator,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

/// Add one captured Series-A remainder to the discovery accumulator.
///
/// Cash is `u64` and entitlements are `u128`, but a sum of `u128` remainders
/// can exceed `u128`, so the accumulator is a fixed `U256` (spec section 3.6).
pub fn accumulate_a_remainder(total: U256, remainder: u128) -> Result<U256, Error> {
    total
        .checked_add(U256::from(remainder))
        .ok_or(Error::ArithmeticOverflow)
}

/// Seal the discovery accumulator into the allocation denominator by removing
/// the senior credit, which is charged against one captured A remainder.
pub fn seal_denominator(accumulator: U256, senior_credit: u64) -> Result<U256, Error> {
    accumulator
        .checked_sub(U256::from(senior_credit))
        .ok_or(Error::ConservationMismatch)
}

/// Sealed Series-A denominator: the `U256` sum of the captured remainders minus
/// the senior credit. Composes the same two steps the ticket runs per row and at
/// seal time, so the host oracle and the program cannot diverge on width.
pub fn sealed_denominator(rows: &[u128], senior_credit: u64) -> Result<U256, Error> {
    let accumulator = rows
        .iter()
        .try_fold(U256::ZERO, |sum, &row| accumulate_a_remainder(sum, row))?;
    seal_denominator(accumulator, senior_credit)
}

/// `floor(slice * weight / denominator)` in checked `U256`, narrowed to the
/// `u64` cash width. A zero denominator is `ZeroDenominator`; the allocator
/// routes that slice to seed instead of calling this. A result above `u64`
/// (only reachable when `weight` exceeds `denominator`) is `ArithmeticOverflow`.
pub fn pro_rata(slice: u64, weight: u128, denominator: U256) -> Result<u64, Error> {
    if denominator == U256::ZERO {
        return Err(Error::ZeroDenominator);
    }
    let product = U256::from(slice)
        .checked_mul(U256::from(weight))
        .ok_or(Error::ArithmeticOverflow)?;
    let quotient = product
        .checked_div(denominator)
        .ok_or(Error::ZeroDenominator)?;
    u64::try_from(quotient).map_err(|_| Error::ArithmeticOverflow)
}

/// A persistent Backer row. `pending` is either a current-ticket credit or a
/// finalized but lazily materialized credit. It is never claimable while its
/// `pending_ticket` is the active ticket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backer {
    pub key: BackerKey,
    pub entitlement: u128,
    pub lifetime_credited: u128,
    pub funded: u64,
    pub pending: u64,
    pub pending_ticket: Option<u64>,
    pub first_funding_sequence: u64,
    pub active_slot: Option<usize>,
}

impl Backer {
    fn remaining(&self) -> Result<u128, Error> {
        self.entitlement
            .checked_sub(self.lifetime_credited)
            .ok_or(Error::ConservationMismatch)
    }

    /// Remaining entitlement, checked against corrupted host state.
    pub fn remaining_entitlement(&self) -> Result<u128, Error> {
        self.remaining()
    }

    /// Whether the row can be in the dense active registry (>10 lamports).
    pub fn is_open(&self) -> Result<bool, Error> {
        Ok(self.active_slot.is_some() && self.remaining()? > 10)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    key: BackerKey,
    original_slot: usize,
    remainder: u128,
    is_a: bool,
    is_b: bool,
    weight: u128,
}

/// The durable serialized allocation ticket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ticket {
    pub id: u64,
    pub toll: u64,
    pub seed0: u64,
    pub seed_destination: u64,
    pub senior_slice: u64,
    pub a_slice: u64,
    pub b_slice: u64,
    pub registry_version: u64,
    pub registry_len: usize,
    pub phase: Phase,
    pub discover_cursor: usize,
    pub allocate_cursor: usize,
    pub prune_cursor: usize,
    pub credited_total: u64,
    senior: Option<usize>,
    senior_credit: u64,
    /// Discovery accumulator: sum of captured A remainders (`U256`, may pass `u128`).
    a_remainder_total: U256,
    /// Sealed denominator: `a_remainder_total - senior_credit`, fixed at seal.
    a_denominator: U256,
    snapshots: Vec<Snapshot>,
    prune_order: Vec<BackerKey>,
    allocated: Vec<bool>,
}

/// Persistent Backer book plus custody partitions used by the host oracle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Book {
    rows: Vec<Backer>,
    registry: Vec<usize>,
    next_sequence: u64,
    registry_version: u64,
    next_ticket: u64,
    last_finalized_ticket: Option<u64>,
    ticket: Option<Ticket>,
    closing: bool,
    pub pot: u64,
    pub seed: u64,
    pub management: u64,
    pub pending_toll: u64,
    pub global_funded_claims: u128,
    pub vault_lamports: u64,
    pub protected_rent: u64,
    histories: Vec<(Owner, bool, bool)>,
}

impl Default for Book {
    fn default() -> Self {
        Self::with_pot(0)
    }
}

impl Book {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pot(pot: u64) -> Self {
        Self::with_balances(pot, 0, 0, 0, pot)
    }

    pub fn with_balances(
        pot: u64,
        seed: u64,
        management: u64,
        global_funded_claims: u128,
        vault_lamports: u64,
    ) -> Self {
        Self {
            rows: Vec::new(),
            registry: Vec::new(),
            next_sequence: 0,
            registry_version: 0,
            next_ticket: 1,
            last_finalized_ticket: None,
            ticket: None,
            closing: false,
            pot,
            seed,
            management,
            pending_toll: 0,
            global_funded_claims,
            vault_lamports,
            protected_rent: 0,
            histories: Vec::new(),
        }
    }

    pub fn with_protected_rent(mut self, protected_rent: u64) -> Self {
        self.protected_rent = protected_rent;
        self
    }

    fn atomic<T, F>(&mut self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut Self) -> Result<T, Error>,
    {
        let before = self.clone();
        match f(self) {
            Ok(value) => Ok(value),
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }

    fn floor_percent(value: u64, percent: u64) -> Result<u64, Error> {
        (U256::from(value) * U256::from(percent) / U256::from(100u64))
            .try_into()
            .map_err(|_| Error::ArithmeticOverflow)
    }

    fn row_index(&self, key: BackerKey) -> Option<usize> {
        self.rows.iter().position(|row| row.key == key)
    }

    fn active_row(&self, slot: usize) -> Result<usize, Error> {
        self.registry.get(slot).copied().ok_or(Error::InvalidRow)
    }

    fn history_mut(&mut self, owner: Owner) -> &mut (Owner, bool, bool) {
        if let Some(index) = self.histories.iter().position(|entry| entry.0 == owner) {
            return &mut self.histories[index];
        }
        self.histories.push((owner, false, false));
        let last = self.histories.len() - 1;
        &mut self.histories[last]
    }

    fn set_history(&mut self, key: BackerKey) {
        let history = self.history_mut(key.owner);
        match key.series {
            Series::A => history.1 = true,
            Series::B => history.2 = true,
        }
    }

    /// Add a new row or merge a same-owner top-up. Cash mutation is explicit:
    /// this method only changes the entitlement registry, while `fund` adds A
    /// cash to the pot.
    pub fn add_backer(
        &mut self,
        owner: Owner,
        series: Series,
        entitlement: u128,
    ) -> Result<(), Error> {
        self.add_backer_with_lifetime(owner, series, entitlement, 0)
    }

    /// Create an initial row with a supplied lifetime credit. Existing rows
    /// accept only the ordinary same-owner entitlement top-up path; restoring
    /// lifetime state on one would require a registry reconciliation proof.
    pub fn add_backer_with_lifetime(
        &mut self,
        owner: Owner,
        series: Series,
        entitlement: u128,
        lifetime_credited: u128,
    ) -> Result<(), Error> {
        self.atomic(|book| {
            if book.ticket.is_some() {
                return Err(Error::AllocationBarrier);
            }
            if lifetime_credited > entitlement {
                return Err(Error::BackerCapped);
            }
            let key = BackerKey::new(owner, series);
            if let Some(index) = book.row_index(key) {
                // This helper is an initial-state constructor. A nonzero
                // lifetime restore on an existing row could rewrite capped
                // history without reconciling its dense-registry slot.
                if lifetime_credited != 0 {
                    return Err(Error::InvalidRow);
                }
                let row = &mut book.rows[index];
                row.entitlement = row
                    .entitlement
                    .checked_add(entitlement)
                    .ok_or(Error::ArithmeticOverflow)?;
                if lifetime_credited > row.lifetime_credited {
                    row.lifetime_credited = lifetime_credited;
                }
                if row.active_slot.is_none() && row.remaining()? > 10 {
                    let slot = book.registry.len();
                    row.active_slot = Some(slot);
                    book.registry.push(index);
                }
                book.set_history(key);
                return Ok(());
            }
            let sequence = book.next_sequence;
            book.next_sequence = book
                .next_sequence
                .checked_add(1)
                .ok_or(Error::ArithmeticOverflow)?;
            let index = book.rows.len();
            let remaining = entitlement
                .checked_sub(lifetime_credited)
                .ok_or(Error::BackerCapped)?;
            let active_slot = if remaining > 10 {
                let slot = book.registry.len();
                book.registry.push(index);
                Some(slot)
            } else {
                None
            };
            book.rows.push(Backer {
                key,
                entitlement,
                lifetime_credited,
                funded: 0,
                pending: 0,
                pending_ticket: None,
                first_funding_sequence: sequence,
                active_slot,
            });
            book.set_history(key);
            Ok(())
        })
    }

    /// Apply the pure Backer-side 124% A-funding primitive for the canonical
    /// Round admission path. This is crate-local and is not a standalone
    /// native-money entrypoint; Round owns quota, pause, phase, and admission
    /// checks before calling it.
    pub(crate) fn fund_series_a_unchecked(&mut self, owner: Owner, cash: u64) -> Result<(), Error> {
        self.atomic(|book| {
            if book.closing {
                return Err(Error::ClosingAdmission);
            }
            if book.ticket.is_some() {
                return Err(Error::AllocationBarrier);
            }
            let entitlement =
                (U256::from(cash) * U256::from(124u64) / U256::from(100u64)).as_u128();
            book.pot = book
                .pot
                .checked_add(cash)
                .ok_or(Error::ArithmeticOverflow)?;
            book.vault_lamports = book
                .vault_lamports
                .checked_add(cash)
                .ok_or(Error::ArithmeticOverflow)?;
            book.add_backer(owner, Series::A, entitlement)?;
            Ok(())
        })
    }

    /// Add a physically matched funded-claim fixture for host tests.
    ///
    /// Native financial instructions never mint a claim this way. The fixture
    /// explicitly adds the same lamports to the modeled vault, so conservation
    /// remains true before and after the fixture and after its claim.
    pub fn seed_funded_fixture(
        &mut self,
        owner: Owner,
        series: Series,
        amount: u64,
    ) -> Result<(), Error> {
        self.atomic(|book| {
            if book.ticket.is_some() {
                return Err(Error::AllocationBarrier);
            }
            let index = book
                .row_index(BackerKey::new(owner, series))
                .ok_or(Error::MissingBacker)?;
            let row = &book.rows[index];
            let new_funded = row
                .funded
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
            let new_lifetime = row
                .lifetime_credited
                .checked_add(amount as u128)
                .ok_or(Error::ArithmeticOverflow)?;
            if new_lifetime > row.entitlement || new_funded as u128 > new_lifetime {
                return Err(Error::BackerCapped);
            }
            let remaining = row
                .entitlement
                .checked_sub(new_lifetime)
                .ok_or(Error::BackerCapped)?;
            let should_deactivate = remaining <= 10 && row.active_slot.is_some();
            if should_deactivate {
                let slot = row.active_slot.ok_or(Error::InvalidRow)?;
                let last_slot = book
                    .registry
                    .len()
                    .checked_sub(1)
                    .ok_or(Error::InvalidRow)?;
                let last_row = book.registry[last_slot];
                if slot > last_slot || (slot != last_slot && last_row == index) {
                    return Err(Error::InvalidRow);
                }
                book.registry_version
                    .checked_add(1)
                    .ok_or(Error::ArithmeticOverflow)?;
            }
            let new_global = book
                .global_funded_claims
                .checked_add(amount as u128)
                .ok_or(Error::ArithmeticOverflow)?;
            let new_vault = book
                .vault_lamports
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
            book.rows[index].funded = new_funded;
            book.rows[index].lifetime_credited = new_lifetime;
            book.global_funded_claims = new_global;
            book.vault_lamports = new_vault;
            if should_deactivate {
                book.deactivate_row(index)?;
            }
            Ok(())
        })
    }

    pub fn reserve_toll(&mut self, toll: u64) -> Result<(), Error> {
        self.atomic(|book| {
            if toll == 0 {
                return Err(Error::ZeroToll);
            }
            if book.ticket.is_some() {
                return Err(Error::TicketActive);
            }
            book.pot = book.pot.checked_sub(toll).ok_or(Error::InsufficientPot)?;
            book.pending_toll = book
                .pending_toll
                .checked_add(toll)
                .ok_or(Error::ArithmeticOverflow)?;
            Ok(())
        })
    }

    pub fn start_ticket(&mut self, toll: u64) -> Result<u64, Error> {
        self.atomic(|book| book.start_ticket_inner(toll))
    }

    fn start_ticket_inner(&mut self, toll: u64) -> Result<u64, Error> {
        if toll == 0 {
            return Err(Error::ZeroToll);
        }
        if self.ticket.is_some() {
            return Err(Error::TicketActive);
        }
        if self.pending_toll < toll {
            return Err(Error::InsufficientEscrow);
        }
        let ticket_id = self.next_ticket;
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or(Error::ArithmeticOverflow)?;
        let seed0 = toll / 2;
        let half = toll - seed0;
        let has_a = self
            .registry
            .iter()
            .any(|&index| self.rows[index].key.series == Series::A);
        let has_b = self
            .registry
            .iter()
            .any(|&index| self.rows[index].key.series == Series::B);
        let mut senior = Self::floor_percent(half, 35)?;
        let mut a_slice = Self::floor_percent(half, 45)?;
        let mut b_slice = Self::floor_percent(half, 20)?;
        let rounded_senior = senior;
        let rounded_a = a_slice;
        let rounded_b = b_slice;
        let rounded = seed0
            .checked_add(rounded_senior)
            .and_then(|v| v.checked_add(rounded_a))
            .and_then(|v| v.checked_add(rounded_b))
            .ok_or(Error::ArithmeticOverflow)?;
        let slice_sum = rounded
            .checked_sub(seed0)
            .ok_or(Error::ArithmeticOverflow)?;
        let mut seed_destination = seed0
            .checked_add(
                half.checked_sub(slice_sum)
                    .ok_or(Error::ArithmeticOverflow)?,
            )
            .ok_or(Error::ArithmeticOverflow)?;
        if !has_a {
            seed_destination = seed_destination
                .checked_add(senior)
                .ok_or(Error::ArithmeticOverflow)?;
            senior = 0;
            b_slice = b_slice
                .checked_add(a_slice)
                .ok_or(Error::ArithmeticOverflow)?;
            a_slice = 0;
        }
        if !has_b {
            a_slice = a_slice
                .checked_add(b_slice)
                .ok_or(Error::ArithmeticOverflow)?;
            b_slice = 0;
        }
        if !has_a && !has_b {
            seed_destination = toll;
            senior = 0;
            a_slice = 0;
            b_slice = 0;
        }
        let ticket = Ticket {
            id: ticket_id,
            toll,
            seed0,
            seed_destination,
            senior_slice: senior,
            a_slice,
            b_slice,
            registry_version: self.registry_version,
            registry_len: self.registry.len(),
            phase: Phase::Discover,
            discover_cursor: 0,
            allocate_cursor: 0,
            prune_cursor: 0,
            credited_total: 0,
            senior: None,
            senior_credit: 0,
            a_remainder_total: U256::ZERO,
            a_denominator: U256::ZERO,
            snapshots: Vec::new(),
            prune_order: Vec::new(),
            allocated: Vec::new(),
        };
        self.ticket = Some(ticket);
        Ok(ticket_id)
    }

    pub fn allocate(&mut self, toll: u64, chunk: usize) -> Result<(), Error> {
        if toll == 0 {
            return Ok(());
        }
        self.atomic(|book| {
            book.reserve_toll(toll)?;
            let half = toll - toll / 2;
            if book.registry.is_empty()
                || (Self::floor_percent(half, 35)? == 0
                    && Self::floor_percent(half, 45)? == 0
                    && Self::floor_percent(half, 20)? == 0)
            {
                book.pending_toll = book
                    .pending_toll
                    .checked_sub(toll)
                    .ok_or(Error::InsufficientEscrow)?;
                book.seed = book
                    .seed
                    .checked_add(toll)
                    .ok_or(Error::ArithmeticOverflow)?;
                return Ok(());
            }
            book.start_ticket_inner(toll)?;
            book.discover_all_inner()?;
            book.apply_senior_inner()?;
            book.seal_weights_inner()?;
            book.allocate_all_current_inner(chunk)?;
            book.prune_all_inner(chunk)?;
            book.commit_inner()
        })
    }

    pub fn discover_chunk(&mut self, slots: &[usize]) -> Result<(), Error> {
        self.atomic(|book| book.discover_chunk_inner(slots))
    }

    /// Discover transport accounts with their claimed row identities. The
    /// program equivalent receives accounts from the caller, so the PDA/key
    /// cross-check is explicit here rather than trusting list position alone.
    pub fn discover_chunk_checked(&mut self, rows: &[(usize, BackerKey)]) -> Result<(), Error> {
        self.atomic(|book| {
            let cursor = book.ticket.as_ref().ok_or(Error::NoTicket)?.discover_cursor;
            let mut slots = Vec::with_capacity(rows.len());
            for (offset, &(slot, key)) in rows.iter().enumerate() {
                if slot != cursor + offset {
                    return Err(Error::InvalidCursor);
                }
                let row_index = book.active_row(slot)?;
                if book.rows[row_index].key != key {
                    return Err(Error::InvalidRow);
                }
                slots.push(slot);
            }
            book.discover_chunk_inner(&slots)
        })
    }

    fn discover_chunk_inner(&mut self, slots: &[usize]) -> Result<(), Error> {
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.phase != Phase::Discover {
            return Err(Error::InvalidPhase);
        }
        if self
            .ticket
            .as_ref()
            .ok_or(Error::NoTicket)?
            .registry_version
            != self.registry_version
        {
            return Err(Error::InvalidRow);
        }
        let cursor = self.ticket.as_ref().ok_or(Error::NoTicket)?.discover_cursor;
        let registry_len = self.ticket.as_ref().ok_or(Error::NoTicket)?.registry_len;
        let mut snapshots = Vec::with_capacity(slots.len());
        for (offset, &slot) in slots.iter().enumerate() {
            if slot != cursor + offset || slot >= registry_len {
                return Err(Error::InvalidCursor);
            }
            let row_index = self.registry[slot];
            let row = &self.rows[row_index];
            if row.active_slot != Some(slot) {
                return Err(Error::InvalidRow);
            }
            let remainder = row.remaining()?;
            snapshots.push(Snapshot {
                key: row.key,
                original_slot: slot,
                remainder,
                is_a: row.key.series == Series::A,
                is_b: row.key.series == Series::B,
                weight: 0,
            });
        }
        let reaches_end = self
            .ticket
            .as_ref()
            .ok_or(Error::NoTicket)?
            .discover_cursor
            .checked_add(slots.len())
            .ok_or(Error::ArithmeticOverflow)?
            == registry_len;
        let senior = if reaches_end {
            let current = self.ticket.as_ref().ok_or(Error::NoTicket)?;
            current
                .snapshots
                .iter()
                .chain(snapshots.iter())
                .filter(|snapshot| snapshot.is_a)
                .map(|snapshot| {
                    let index = self.row_index(snapshot.key).ok_or(Error::MissingBacker)?;
                    Ok((
                        self.rows[index].first_funding_sequence,
                        snapshot.key,
                        snapshot.original_slot,
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?
                .into_iter()
                .min_by_key(|entry| (entry.0, entry.1))
                .map(|entry| entry.2)
        } else {
            None
        };
        let a_remainder_total = snapshots.iter().filter(|snapshot| snapshot.is_a).try_fold(
            self.ticket
                .as_ref()
                .ok_or(Error::NoTicket)?
                .a_remainder_total,
            |total, snapshot| accumulate_a_remainder(total, snapshot.remainder),
        )?;
        let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
        ticket.a_remainder_total = a_remainder_total;
        ticket.snapshots.extend(snapshots);
        ticket
            .allocated
            .extend(std::iter::repeat_n(false, slots.len()));
        ticket.discover_cursor = ticket
            .discover_cursor
            .checked_add(slots.len())
            .ok_or(Error::ArithmeticOverflow)?;
        if reaches_end {
            ticket.phase = Phase::Senior;
            ticket.senior = senior;
        }
        Ok(())
    }

    pub fn discover_all(&mut self) -> Result<(), Error> {
        self.atomic(|book| book.discover_all_inner())
    }

    fn discover_all_inner(&mut self) -> Result<(), Error> {
        let len = self.ticket.as_ref().ok_or(Error::NoTicket)?.registry_len;
        let start = self.ticket.as_ref().ok_or(Error::NoTicket)?.discover_cursor;
        let slots: Vec<usize> = (start..len).collect();
        self.discover_chunk_inner(&slots)
    }

    pub fn apply_senior(&mut self) -> Result<(), Error> {
        self.atomic(|book| book.apply_senior_inner())
    }

    fn apply_senior_inner(&mut self) -> Result<(), Error> {
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.phase != Phase::Senior {
            return Err(Error::InvalidPhase);
        }
        let (ticket_id, senior_slot, senior_slice) = {
            let ticket = self.ticket.as_ref().ok_or(Error::NoTicket)?;
            (ticket.id, ticket.senior, ticket.senior_slice)
        };
        let (key, remainder) = if let Some(slot) = senior_slot {
            let snapshot = self
                .ticket
                .as_ref()
                .ok_or(Error::NoTicket)?
                .snapshots
                .iter()
                .find(|s| s.original_slot == slot)
                .ok_or(Error::InvalidRow)?;
            (snapshot.key, snapshot.remainder)
        } else {
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            ticket.seed_destination = ticket
                .seed_destination
                .checked_add(senior_slice)
                .ok_or(Error::ArithmeticOverflow)?;
            ticket.phase = Phase::SealWeights;
            return Ok(());
        };
        let credit = senior_slice.min(remainder.min(u64::MAX as u128) as u64);
        self.credit(key, ticket_id, credit)?;
        let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
        ticket.senior_credit = credit;
        if credit < senior_slice {
            ticket.seed_destination = ticket
                .seed_destination
                .checked_add(senior_slice - credit)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        ticket.phase = Phase::SealWeights;
        Ok(())
    }

    pub fn seal_weights(&mut self) -> Result<(), Error> {
        self.atomic(|book| book.seal_weights_inner())
    }

    fn seal_weights_inner(&mut self) -> Result<(), Error> {
        self.atomic_seal_weights()
    }

    fn atomic_seal_weights(&mut self) -> Result<(), Error> {
        let before = self.clone();
        let result = (|| {
            if self.ticket.as_ref().ok_or(Error::NoTicket)?.phase != Phase::SealWeights {
                return Err(Error::InvalidPhase);
            }
            let (senior, senior_credit, a_remainder_total) = {
                let ticket = self.ticket.as_ref().ok_or(Error::NoTicket)?;
                (
                    ticket.senior,
                    ticket.senior_credit,
                    ticket.a_remainder_total,
                )
            };
            let denominator = seal_denominator(a_remainder_total, senior_credit)?;
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            ticket.a_denominator = denominator;
            for snapshot in &mut ticket.snapshots {
                let subtraction = if Some(snapshot.original_slot) == senior {
                    u128::from(senior_credit)
                } else {
                    0
                };
                snapshot.weight = snapshot
                    .remainder
                    .checked_sub(subtraction)
                    .ok_or(Error::ConservationMismatch)?;
            }
            ticket.phase = Phase::Allocate;
            Ok(())
        })();
        if result.is_err() {
            *self = before;
        }
        result
    }

    pub fn allocate_chunk(&mut self, slots: &[usize]) -> Result<(), Error> {
        self.atomic(|book| book.allocate_chunk_inner(slots))
    }

    fn allocate_chunk_inner(&mut self, slots: &[usize]) -> Result<(), Error> {
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.phase != Phase::Allocate {
            return Err(Error::InvalidPhase);
        }
        let start_cursor = self.ticket.as_ref().ok_or(Error::NoTicket)?.allocate_cursor;
        for (offset, &slot) in slots.iter().enumerate() {
            if slot != start_cursor + offset {
                return Err(Error::InvalidCursor);
            }
            let (snapshot, a_pool, b_pool, denominator, ticket_id) = {
                let ticket = self.ticket.as_ref().ok_or(Error::NoTicket)?;
                let snapshot = ticket
                    .snapshots
                    .get(slot)
                    .cloned()
                    .ok_or(Error::InvalidRow)?;
                (
                    snapshot,
                    ticket.a_slice,
                    ticket.b_slice,
                    ticket.a_denominator,
                    ticket.id,
                )
            };
            if self.ticket.as_ref().ok_or(Error::NoTicket)?.allocated[slot] {
                return Err(Error::ReplayedRow);
            }
            let credit: u128 = if snapshot.is_a && denominator != U256::ZERO {
                u128::from(pro_rata(a_pool, snapshot.weight, denominator)?)
            } else if snapshot.is_b {
                let count = self
                    .ticket
                    .as_ref()
                    .ok_or(Error::NoTicket)?
                    .snapshots
                    .iter()
                    .filter(|row| row.is_b)
                    .count() as u64;
                if count == 0 {
                    0
                } else {
                    (b_pool / count) as u128
                }
            } else {
                0
            };
            let cap = if snapshot.is_a {
                snapshot.weight
            } else {
                snapshot.remainder
            };
            let credit = credit.min(cap).min(u64::MAX as u128) as u64;
            self.credit(snapshot.key, ticket_id, credit)?;
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            ticket.allocated[slot] = true;
            ticket.allocate_cursor = ticket
                .allocate_cursor
                .checked_add(1)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.allocate_cursor
            == self.ticket.as_ref().ok_or(Error::NoTicket)?.snapshots.len()
        {
            let (a_credited, b_credited, a_pool, b_pool, denominator, has_a) = {
                let ticket = self.ticket.as_ref().ok_or(Error::NoTicket)?;
                let mut a = 0u64;
                let mut b = 0u64;
                for snapshot in &ticket.snapshots {
                    let row =
                        &self.rows[self.row_index(snapshot.key).ok_or(Error::MissingBacker)?];
                    let credited = row.pending_ticket == Some(ticket.id);
                    if credited {
                        let amount = if Some(snapshot.original_slot) == ticket.senior {
                            row.pending.saturating_sub(ticket.senior_credit)
                        } else {
                            row.pending
                        };
                        if snapshot.is_a {
                            a = a.saturating_add(amount);
                        }
                        if snapshot.is_b {
                            b = b.saturating_add(amount);
                        }
                    }
                }
                (
                    a,
                    b,
                    ticket.a_slice,
                    ticket.b_slice,
                    ticket.a_denominator,
                    ticket.snapshots.iter().any(|row| row.is_a),
                )
            };
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            if denominator == U256::ZERO && has_a {
                ticket.seed_destination = ticket
                    .seed_destination
                    .checked_add(a_pool)
                    .ok_or(Error::ArithmeticOverflow)?;
            } else if denominator != U256::ZERO {
                ticket.seed_destination = ticket
                    .seed_destination
                    .checked_add(a_pool.saturating_sub(a_credited))
                    .ok_or(Error::ArithmeticOverflow)?;
            }
            ticket.seed_destination = ticket
                .seed_destination
                .checked_add(b_pool.saturating_sub(b_credited))
                .ok_or(Error::ArithmeticOverflow)?;
            let mut prune_order = Vec::new();
            let ticket = self.ticket.as_ref().ok_or(Error::NoTicket)?;
            for snapshot in &ticket.snapshots {
                let row = &self.rows[self.row_index(snapshot.key).ok_or(Error::MissingBacker)?];
                if row.remaining()? <= 10 {
                    prune_order.push((snapshot.original_slot, snapshot.key));
                }
            }
            prune_order.sort_by(|left, right| right.0.cmp(&left.0));
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            ticket.prune_order = prune_order.into_iter().map(|(_, key)| key).collect();
            ticket.phase = Phase::Prune;
        }
        Ok(())
    }

    pub fn allocate_all_current(&mut self, chunk: usize) -> Result<(), Error> {
        self.atomic(|book| book.allocate_all_current_inner(chunk))
    }

    fn allocate_all_current_inner(&mut self, chunk: usize) -> Result<(), Error> {
        if chunk == 0 {
            return Err(Error::InvalidCursor);
        }
        let len = self.ticket.as_ref().ok_or(Error::NoTicket)?.snapshots.len();
        while self.ticket.as_ref().ok_or(Error::NoTicket)?.phase == Phase::Allocate {
            let cursor = self.ticket.as_ref().ok_or(Error::NoTicket)?.allocate_cursor;
            if cursor > len {
                return Err(Error::InvalidCursor);
            }
            let end = cursor.saturating_add(chunk).min(len);
            let slots: Vec<usize> = (cursor..end).collect();
            self.allocate_chunk_inner(&slots)?;
        }
        Ok(())
    }

    pub fn prune_chunk(&mut self, keys: &[BackerKey]) -> Result<(), Error> {
        self.atomic(|book| book.prune_chunk_inner(keys))
    }

    fn prune_chunk_inner(&mut self, keys: &[BackerKey]) -> Result<(), Error> {
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.phase != Phase::Prune {
            return Err(Error::InvalidPhase);
        }
        let start_cursor = self.ticket.as_ref().ok_or(Error::NoTicket)?.prune_cursor;
        for (offset, &key) in keys.iter().enumerate() {
            if self
                .ticket
                .as_ref()
                .ok_or(Error::NoTicket)?
                .prune_order
                .get(start_cursor + offset)
                != Some(&key)
            {
                return Err(Error::InvalidCursor);
            }
            self.prune_key(key)?;
            self.ticket.as_mut().ok_or(Error::NoTicket)?.prune_cursor += 1;
        }
        if self.ticket.as_ref().ok_or(Error::NoTicket)?.prune_cursor
            == self
                .ticket
                .as_ref()
                .ok_or(Error::NoTicket)?
                .prune_order
                .len()
        {
            self.ticket.as_mut().ok_or(Error::NoTicket)?.phase = Phase::Commit;
        }
        Ok(())
    }

    fn deactivate_row(&mut self, row_index: usize) -> Result<(), Error> {
        let slot = self.rows[row_index].active_slot.ok_or(Error::InvalidRow)?;
        let last_slot = self
            .registry
            .len()
            .checked_sub(1)
            .ok_or(Error::InvalidRow)?;
        let last_row_index = self.registry[last_slot];
        if slot > last_slot || (slot != last_slot && last_row_index == row_index) {
            return Err(Error::InvalidRow);
        }
        let new_registry_version = self
            .registry_version
            .checked_add(1)
            .ok_or(Error::ArithmeticOverflow)?;
        self.registry.pop();
        if slot != last_slot {
            self.registry[slot] = last_row_index;
            self.rows[last_row_index].active_slot = Some(slot);
        }
        self.rows[row_index].active_slot = None;
        self.registry_version = new_registry_version;
        Ok(())
    }

    fn prune_key(&mut self, key: BackerKey) -> Result<(), Error> {
        let row_index = self.row_index(key).ok_or(Error::MissingBacker)?;
        self.deactivate_row(row_index)
    }

    pub fn prune_all(&mut self) -> Result<(), Error> {
        self.atomic(|book| book.prune_all_inner(usize::MAX))
    }

    fn prune_all_inner(&mut self, chunk: usize) -> Result<(), Error> {
        if chunk == 0 {
            return Err(Error::InvalidCursor);
        }
        while self.ticket.as_ref().ok_or(Error::NoTicket)?.phase == Phase::Prune {
            let cursor = self.ticket.as_ref().ok_or(Error::NoTicket)?.prune_cursor;
            let len = self
                .ticket
                .as_ref()
                .ok_or(Error::NoTicket)?
                .prune_order
                .len();
            if cursor > len {
                return Err(Error::InvalidCursor);
            }
            let end = cursor.saturating_add(chunk).min(len);
            let keys =
                self.ticket.as_ref().ok_or(Error::NoTicket)?.prune_order[cursor..end].to_vec();
            self.prune_chunk_inner(&keys)?;
        }
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), Error> {
        self.atomic(|book| book.commit_inner())
    }

    fn commit_inner(&mut self) -> Result<(), Error> {
        let ticket = self.ticket.clone().ok_or(Error::NoTicket)?;
        if ticket.phase != Phase::Commit
            || ticket
                .credited_total
                .checked_add(ticket.seed_destination)
                .ok_or(Error::ArithmeticOverflow)?
                != ticket.toll
        {
            return Err(Error::InvalidPhase);
        }
        self.pending_toll = self
            .pending_toll
            .checked_sub(ticket.toll)
            .ok_or(Error::InsufficientEscrow)?;
        self.seed = self
            .seed
            .checked_add(ticket.seed_destination)
            .ok_or(Error::ArithmeticOverflow)?;
        self.global_funded_claims = self
            .global_funded_claims
            .checked_add(ticket.credited_total as u128)
            .ok_or(Error::ArithmeticOverflow)?;
        for row in &mut self.rows {
            if row.pending_ticket == Some(ticket.id) {
                row.pending_ticket = Some(ticket.id);
            }
        }
        self.last_finalized_ticket = Some(ticket.id);
        self.ticket = None;
        Ok(())
    }

    fn credit(&mut self, key: BackerKey, ticket_id: u64, amount: u64) -> Result<(), Error> {
        let index = self.row_index(key).ok_or(Error::MissingBacker)?;
        self.materialize_if_finalized(index, Some(ticket_id))?;
        let row = &mut self.rows[index];
        if amount != 0 {
            row.lifetime_credited = row
                .lifetime_credited
                .checked_add(amount as u128)
                .ok_or(Error::ArithmeticOverflow)?;
            if row.lifetime_credited > row.entitlement {
                return Err(Error::BackerCapped);
            }
            row.pending = row
                .pending
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
            row.pending_ticket = Some(ticket_id);
            let ticket = self.ticket.as_mut().ok_or(Error::NoTicket)?;
            ticket.credited_total = ticket
                .credited_total
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        Ok(())
    }

    fn materialize_if_finalized(
        &mut self,
        index: usize,
        active_ticket: Option<u64>,
    ) -> Result<(), Error> {
        let pending_ticket = self.rows[index].pending_ticket;
        if pending_ticket.is_none() || pending_ticket == active_ticket {
            return Ok(());
        }
        let pending = self.rows[index].pending;
        let funded = self.rows[index]
            .funded
            .checked_add(pending)
            .ok_or(Error::ArithmeticOverflow)?;
        self.rows[index].funded = funded;
        self.rows[index].pending = 0;
        self.rows[index].pending_ticket = None;
        Ok(())
    }

    pub fn claim(&mut self, owner: Owner) -> Result<u64, Error> {
        self.atomic(|book| {
            let mut total = 0u64;
            for index in 0..book.rows.len() {
                if book.rows[index].key.owner == owner {
                    book.materialize_if_finalized(
                        index,
                        book.ticket.as_ref().map(|ticket| ticket.id),
                    )?;
                    total = total
                        .checked_add(book.rows[index].funded)
                        .ok_or(Error::ArithmeticOverflow)?;
                }
            }
            book.global_funded_claims = book
                .global_funded_claims
                .checked_sub(total as u128)
                .ok_or(Error::ConservationMismatch)?;
            book.vault_lamports = book
                .vault_lamports
                .checked_sub(total)
                .ok_or(Error::ConservationMismatch)?;
            for row in &mut book.rows {
                if row.key.owner == owner {
                    row.funded = 0;
                }
            }
            Ok(total)
        })
    }

    pub fn accept_game_deposit(&mut self, amount: u64) -> Result<(), Error> {
        self.atomic(|book| {
            if book.closing {
                return Err(Error::ClosingAdmission);
            }
            book.pot = book
                .pot
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
            book.vault_lamports = book
                .vault_lamports
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
            Ok(())
        })
    }

    pub fn record_game_payout(&mut self, amount: u64) -> Result<(), Error> {
        self.atomic(|book| {
            if book.closing {
                return Err(Error::ClosingAdmission);
            }
            if book.ticket.is_some() {
                return Err(Error::AllocationBarrier);
            }
            book.pot = book.pot.checked_sub(amount).ok_or(Error::InsufficientPot)?;
            book.vault_lamports = book
                .vault_lamports
                .checked_sub(amount)
                .ok_or(Error::InsufficientPot)?;
            Ok(())
        })
    }

    pub fn set_closing(&mut self, closing: bool) -> Result<(), Error> {
        self.atomic(|book| {
            book.closing = closing;
            Ok(())
        })
    }

    pub fn conservation(&self) -> Result<u128, Error> {
        let tracked = (self.pot as u128)
            .checked_add(self.seed as u128)
            .and_then(|v| v.checked_add(self.management as u128))
            .and_then(|v| v.checked_add(self.global_funded_claims))
            .and_then(|v| v.checked_add(self.pending_toll as u128))
            .ok_or(Error::ArithmeticOverflow)?;
        let physical = (self.vault_lamports as u128)
            .checked_sub(self.protected_rent as u128)
            .ok_or(Error::ConservationMismatch)?;
        if tracked != physical {
            return Err(Error::ConservationMismatch);
        }
        Ok(physical)
    }

    pub fn backer(&self, key: BackerKey) -> Result<&Backer, Error> {
        let index = self.row_index(key).ok_or(Error::MissingBacker)?;
        Ok(&self.rows[index])
    }

    pub fn active_slot(&self, key: BackerKey) -> Option<usize> {
        self.row_index(key)
            .and_then(|index| self.rows[index].active_slot)
    }

    pub fn is_active(&self, key: BackerKey) -> bool {
        self.active_slot(key).is_some()
    }

    pub fn has_ever(&self, owner: Owner, series: Series) -> bool {
        self.histories
            .iter()
            .find(|entry| entry.0 == owner)
            .map(|entry| match series {
                Series::A => entry.1,
                Series::B => entry.2,
            })
            .unwrap_or(false)
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn global_funded_claims(&self) -> u128 {
        self.global_funded_claims
    }
    pub fn ticket(&self) -> Option<&Ticket> {
        self.ticket.as_ref()
    }
    pub fn phase(&self) -> Option<Phase> {
        self.ticket.as_ref().map(|ticket| ticket.phase)
    }
}
