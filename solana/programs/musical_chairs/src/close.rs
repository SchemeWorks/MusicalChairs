//! Claim-triggered close: sealed candidate sets, the bound provider draw, the
//! promotion, deadline expiry, reserve settlement, finalization and the
//! provider cash path (plan Milestone 7; spec sections 4, 7.4, 8.2-8.7).
//!
//! The closing payout (positions.rs) binds a `CloseRecord`, locks the reserve
//! and, when its toll needs an allocation ticket, leaves `cash_committed`
//! false until `commit_ticket`.  Everything below is permissionless except
//! `withdraw_provider_return`, which the G vault signs.
//!
//! # Two kinds of function
//!
//! Every state transition is a pure function over decoded structs
//! (`book_*`, `plan_*`, `classify`, `expiry_stage`, ...), unit-tested on the
//! host.  The handlers are thin account shells around them: they validate
//! accounts, run the transition, move lamports and emit events.  All checks
//! precede the first lamport movement or CPI, and Solana's per-transaction
//! atomicity reverts everything on any later error.
//!
//! # Cash rules
//!
//! Every spend that leaves `OpsCash` for a close decrements
//! `locked_sponsor_remaining` and then `locked_owner_remaining` by the same
//! lamports, records the sponsor/owner split in `spent`, and lowers
//! `ops.locked_remaining_total` by the total.  Partitions therefore always
//! equal the unspent cash that remains in `OpsCash`, so settlement can never
//! refund money that was already spent.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::system_program;
use ethnum::U256;

use crate::allocation::{batch_len, read_program_account, write_program_account, SERIES_B};
use crate::controls::require_vault_signer;
use crate::events::{
    CandidatesSealed, DrawCaptured, DrawRequested, OwnerRefundDelivered, PromotionApplied,
    PromotionSkipped, RoundFinalized, SCHEMA_VERSION,
};
use crate::positions::remains_open;
use crate::provider::{self, orao};
use crate::sampling;
use crate::state::{
    Backer, CandidatePage, CloseRecord, ConfigVersion, Ops, ParticipantPage, Pool, Promotion,
    RegistryPage, RentComponents, RngAuthority, User, UserRound, BACKER_SEED, CANDIDATES_SEED,
    CLOSE_SEED, CONFIG_SEED, DRAW_CAPTURED, DRAW_IGNORED_LATE, DRAW_NOT_REQUESTED, DRAW_REQUESTED,
    OPS_SEED, PAGE_CAPACITY, PARTICIPANTS_SEED, POOL_SEED, REGISTRY_SEED, RNG_AUTHORITY_SEED,
    USER_ROUND_SEED, USER_SEED,
};
use crate::{checked_add, checked_sub, ErrorCode as Code};

/// Why a promotion was skipped (`Promotion::Skipped.reason`).
pub const REASON_DEADLINE: u8 = 1;
pub const REASON_ENTROPY_EXHAUSTED: u8 = 2;
pub const REASON_NO_CANDIDATES: u8 = 3;

/// How far a deadline-skipped close had got (`Promotion::Skipped.stage`).  A
/// diagnostic only.  `expire_promotion` cannot see provider state, so it
/// reports `STAGE_UNREQUESTED` for a sealed, unrequested close; the two stages
/// it cannot tell apart (`STAGE_PAGES_NOT_PROVISIONED`,
/// `STAGE_PROVIDER_FEE_EXCEEDED`) are defined for indexers and never emitted.
pub const STAGE_CANDIDATES_INCOMPLETE: u8 = 1;
pub const STAGE_PAGES_NOT_PROVISIONED: u8 = 2;
pub const STAGE_UNREQUESTED: u8 = 3;
pub const STAGE_PROVIDER_FEE_EXCEEDED: u8 = 4;
pub const STAGE_REQUESTED_UNFULFILLED: u8 = 5;
pub const STAGE_SEALED: u8 = 6;

/// Candidate set tags (`CandidatePage.set_tag`, `CloseRecord.selected_set_tag`).
pub const SET_PREFERRED: u8 = 0;
pub const SET_FALLBACK: u8 = 1;

/// Remaining accounts per `build_candidates` row:
/// `[participant_page, user_round, user]`.
pub const BUILD_ACCOUNTS_PER_ROW: usize = 3;

const PAGE: u64 = PAGE_CAPACITY as u64;

// ---------------------------------------------------------------------------
// Pure rules
// ---------------------------------------------------------------------------

/// How one participant is classified when the candidate sets are built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidacy {
    /// `deposits - net_payout`, saturating at zero.
    pub loss: u128,
    /// Every loser is a fallback candidate.
    pub fallback: bool,
    /// A loser with no Series A or B history is also a preferred candidate.
    pub preferred: bool,
}

/// A participant's loss: `deposits - net_payout`, saturating at zero.
pub fn loss_of(deposits: u128, net_payout: u128) -> u128 {
    deposits.saturating_sub(net_payout)
}

/// Classify a participant from its sealed `UserRound` and `User` history.
pub fn classify(deposits: u128, net_payout: u128, ever_a: bool, ever_b: bool) -> Candidacy {
    let loss = loss_of(deposits, net_payout);
    let fallback = loss > 0;
    Candidacy { loss, fallback, preferred: fallback && !ever_a && !ever_b }
}

/// `floor(loss * b_entitlement_bps / 10_000)` (11,600 bps is the accepted 116%).
pub fn promotion_entitlement(loss: u128, b_entitlement_bps: u16) -> Result<u128> {
    // u128::MAX * u16::MAX < 2^144, so the product cannot overflow U256.
    let scaled = U256::from(loss) * U256::from(b_entitlement_bps) / U256::from(10_000u16);
    u128::try_from(scaled).map_err(|_| error!(Code::Overflow))
}

/// `(tag, count, digest)` of the sealed candidate set the draw selects from.
pub fn selected_set(close: &CloseRecord) -> (u8, u64, [u8; 32]) {
    if close.selected_set_tag == SET_PREFERRED {
        (SET_PREFERRED, close.preferred_count, close.preferred_digest)
    } else {
        (SET_FALLBACK, close.fallback_count, close.fallback_digest)
    }
}

/// The diagnostic stage `expire_promotion` records.  A captured draw is never
/// expired (`InvalidState`: apply it instead).
pub fn expiry_stage(sealed: bool, draw_state: u8) -> Result<u8> {
    match (sealed, draw_state) {
        (false, DRAW_NOT_REQUESTED) => Ok(STAGE_CANDIDATES_INCOMPLETE),
        (true, DRAW_NOT_REQUESTED) => Ok(STAGE_UNREQUESTED),
        (true, DRAW_REQUESTED) | (true, DRAW_IGNORED_LATE) => Ok(STAGE_REQUESTED_UNFULFILLED),
        _ => err!(Code::InvalidState),
    }
}

/// Where a request's fee and rent come from: sponsor partition first, then the
/// owner partition, fee before rent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestSpend {
    pub fee_s: u64,
    pub fee_o: u64,
    pub rent_s: u64,
    pub rent_o: u64,
}

impl RequestSpend {
    pub fn sponsor(&self) -> Result<u64> {
        checked_add(self.fee_s, self.rent_s)
    }
    pub fn owner(&self) -> Result<u64> {
        checked_add(self.fee_o, self.rent_o)
    }
    pub fn total(&self) -> Result<u64> {
        checked_add(self.sponsor()?, self.owner()?)
    }
}

/// The reserve a request must leave behind so that a timely capture can always
/// be applied from locked funds (spec section 8.3): the Series-B account rent
/// and, when the registry tail needs a new page, that page's rent, both from
/// the schedule captured at close.
pub fn promotion_holdback(close: &CloseRecord) -> Result<u64> {
    let schedule = &close.reserve_schedule;
    let registry = if close.registry_extension_required { schedule.per_registry_page_rent } else { 0 };
    checked_add(schedule.b_account_rent, registry)
}

/// Split `fee + rent` across the remaining partitions.  Both together, plus the
/// `holdback` the promotion still needs, must fit in what remains, else
/// `ProviderFeeExceeded`: a request that would strand the promotion without its
/// storage is refused, and the close expires instead.
pub fn plan_request_spend(sponsor: u64, owner: u64, fee: u64, rent: u64, holdback: u64) -> Result<RequestSpend> {
    let total = fee.checked_add(rent).ok_or_else(|| error!(Code::ProviderFeeExceeded))?;
    let available = sponsor.checked_add(owner).ok_or_else(|| error!(Code::Overflow))?;
    let needed = total.checked_add(holdback).ok_or_else(|| error!(Code::ProviderFeeExceeded))?;
    require!(needed <= available, Code::ProviderFeeExceeded);
    let fee_s = fee.min(sponsor);
    let fee_o = fee - fee_s;
    let rent_s = rent.min(sponsor - fee_s);
    let rent_o = rent - rent_s;
    Ok(RequestSpend { fee_s, fee_o, rent_s, rent_o })
}

/// Book a successful provider request.  All new values are computed before
/// the first assignment, so an error leaves `ops` and `close` untouched.
pub fn book_request(
    ops: &mut Ops,
    close: &mut CloseRecord,
    spend: &RequestSpend,
    request_account: Pubkey,
    seed: [u8; 32],
) -> Result<()> {
    let (sponsor, owner) = (spend.sponsor()?, spend.owner()?);
    let total = checked_add(sponsor, owner)?;
    let sponsor_left = checked_sub(close.locked_sponsor_remaining, sponsor)?;
    let owner_left = checked_sub(close.locked_owner_remaining, owner)?;
    let locked_total = checked_sub(ops.locked_remaining_total, total)?;
    let mut spent = close.spent;
    spent.provider_fee_s = checked_add(spent.provider_fee_s, spend.fee_s)?;
    spent.provider_fee_o = checked_add(spent.provider_fee_o, spend.fee_o)?;
    spent.request_rent_s = checked_add(spent.request_rent_s, spend.rent_s)?;
    spent.request_rent_o = checked_add(spent.request_rent_o, spend.rent_o)?;
    let funded_s = checked_add(close.request_rent_funded_s, spend.rent_s)?;
    let funded_o = checked_add(close.request_rent_funded_o, spend.rent_o)?;
    let fee = checked_add(spend.fee_s, spend.fee_o)?;
    let rent = checked_add(spend.rent_s, spend.rent_o)?;
    let consumed = ops
        .provider_fee_consumed_total
        .checked_add(u128::from(fee))
        .ok_or_else(|| error!(Code::Overflow))?;
    let outstanding = checked_add(ops.request_rent_outstanding, rent)?;

    close.locked_sponsor_remaining = sponsor_left;
    close.locked_owner_remaining = owner_left;
    close.spent = spent;
    close.request_rent_funded_s = funded_s;
    close.request_rent_funded_o = funded_o;
    close.request_account = request_account;
    close.protocol_seed = seed;
    close.draw_state = DRAW_REQUESTED;
    ops.locked_remaining_total = locked_total;
    ops.provider_fee_consumed_total = consumed;
    ops.request_rent_outstanding = outstanding;
    Ok(())
}

/// What a rent-bearing creation (`B` account, registry page) is booked under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spend {
    BAccount,
    RegistryPage,
}

/// Book a locked-reserve rent debit: partitions (sponsor first), `spent`
/// category, `ops.locked_remaining_total` and `ops.total_rent_funded`.
/// All-or-nothing; `free_balance` is never touched.
pub fn book_locked_rent(ops: &mut Ops, close: &mut CloseRecord, debit: u64, kind: Spend) -> Result<(u64, u64)> {
    let (sponsor, owner) =
        crate::split_locked(close.locked_sponsor_remaining, close.locked_owner_remaining, debit)?;
    let sponsor_left = checked_sub(close.locked_sponsor_remaining, sponsor)?;
    let owner_left = checked_sub(close.locked_owner_remaining, owner)?;
    let locked_total = checked_sub(ops.locked_remaining_total, debit)?;
    let rent_funded = ops
        .total_rent_funded
        .checked_add(u128::from(debit))
        .ok_or_else(|| error!(Code::Overflow))?;
    let mut spent = close.spent;
    match kind {
        Spend::BAccount => {
            spent.b_rent_s = checked_add(spent.b_rent_s, sponsor)?;
            spent.b_rent_o = checked_add(spent.b_rent_o, owner)?;
        }
        Spend::RegistryPage => {
            spent.registry_rent_s = checked_add(spent.registry_rent_s, sponsor)?;
            spent.registry_rent_o = checked_add(spent.registry_rent_o, owner)?;
        }
    }
    close.locked_sponsor_remaining = sponsor_left;
    close.locked_owner_remaining = owner_left;
    close.spent = spent;
    ops.locked_remaining_total = locked_total;
    ops.total_rent_funded = rent_funded;
    Ok((sponsor, owner))
}

/// Whether a credit of `amount` can legally land on a data-less system account
/// holding `current_lamports`.  Runtime rule (Agave `svm/src/rent_calculator.rs`,
/// copied verbatim into LiteSVM 0.7.1 `utils/rent.rs::transition_allowed`): a
/// writable account may not END a transaction rent-paying (0 < lamports <
/// minimum) if it was uninitialized or rent-exempt before, and a rent-paying
/// account may not be credited while staying rent-paying.  So the post-balance
/// must reach the minimum; `current >= minimum` is the trivial case.  LiteSVM
/// applies that check only to accounts WITH data, so this predicate is
/// validated against the rule text, not against local execution (IMPL-43).
pub fn credit_is_deliverable(current_lamports: u64, amount: u64, rent_exempt_minimum: u64) -> bool {
    current_lamports.saturating_add(amount) >= rent_exempt_minimum
}

/// Settle the reserve once, when the promotion becomes terminal: the sponsor
/// remainder returns to `free_balance`; the owner remainder is paid to the
/// owner destination when the runtime can deliver it (see
/// [`credit_is_deliverable`]), otherwise it STAYS in `locked_owner_remaining`
/// (held in `OpsCash`, still counted in `ops.locked_remaining_total`) as a
/// deferred owner refund that `deliver_owner_refund` pays later, exactly
/// once.  Returns the owner lamports to transfer now.  Owner lamports are
/// never converted into `unallocated_excess` (spec section 8.5, DR-14; Codex
/// M7 review).  Settlement gates `finalize_round`, so it never fails on the
/// state of an external wallet.
pub fn settle_books(ops: &mut Ops, close: &mut CloseRecord, owner_deliverable: bool) -> Result<u64> {
    if close.reserve_settled {
        return Ok(0);
    }
    let (sponsor, owner) = (close.locked_sponsor_remaining, close.locked_owner_remaining);
    let pay = if owner_deliverable { owner } else { 0 };
    let released = checked_add(sponsor, pay)?;
    let locked_total = checked_sub(ops.locked_remaining_total, released)?;
    let free = checked_add(ops.free_balance, sponsor)?;
    ops.locked_remaining_total = locked_total;
    ops.free_balance = free;
    close.locked_sponsor_remaining = 0;
    close.locked_owner_remaining = owner - pay;
    close.reserve_settled = true;
    Ok(pay)
}

/// Book the delivery of a deferred owner refund: the whole owner partition of
/// a settled close leaves `OpsCash` for the immutable destination, exactly
/// once.  Returns the lamports to transfer.
pub fn book_delivery(ops: &mut Ops, close: &mut CloseRecord) -> Result<u64> {
    require!(close.reserve_settled, Code::InvalidState);
    let owed = close.locked_owner_remaining;
    require!(owed > 0, Code::NothingDeferred);
    let locked_total = checked_sub(ops.locked_remaining_total, owed)?;
    ops.locked_remaining_total = locked_total;
    close.locked_owner_remaining = 0;
    Ok(owed)
}

/// Move `extra` from free sponsor funds into the close's sponsor partition and
/// raise the provider-fee cap (spec section 8.5).
pub fn book_budget_increase(ops: &mut Ops, close: &mut CloseRecord, extra: u64) -> Result<()> {
    require!(extra > 0, Code::InvalidState);
    require!(ops.free_balance >= extra, Code::InsufficientOperations);
    let sponsor = checked_add(close.locked_sponsor_remaining, extra)?;
    let cap = checked_add(close.approved_provider_fee_cap, extra)?;
    let required = checked_add(close.required_reserve, extra)?;
    let locked_total = checked_add(ops.locked_remaining_total, extra)?;
    ops.free_balance -= extra;
    ops.locked_remaining_total = locked_total;
    close.locked_sponsor_remaining = sponsor;
    close.approved_provider_fee_cap = cap;
    close.required_reserve = required;
    Ok(())
}

/// `(funded, eligible, owner_refundable)` for an observed fulfilled request:
/// `eligible = funded - actual request lamports` (bounded, conservative:
/// unsolicited credits to the request account can only lower it) and the
/// owner is refunded first because its lamports were funded last.
pub fn eligibility(funded_s: u64, funded_o: u64, request_lamports: u64) -> Result<(u64, u64, u64)> {
    let funded = checked_add(funded_s, funded_o)?;
    let eligible = funded.saturating_sub(request_lamports);
    Ok((funded, eligible, funded_o.min(eligible)))
}

/// Observe a fulfilled request once (spec section 8.4).
pub fn book_observation(ops: &mut Ops, close: &mut CloseRecord, request_lamports: u64) -> Result<()> {
    require!(!close.reimbursement_observed, Code::AlreadyObserved);
    let (funded, eligible, owner_refundable) =
        eligibility(close.request_rent_funded_s, close.request_rent_funded_o, request_lamports)?;
    let outstanding = checked_sub(ops.request_rent_outstanding, funded)?;
    let budget = checked_add(ops.eligible_return_budget, eligible)?;
    ops.request_rent_outstanding = outstanding;
    ops.eligible_return_budget = budget;
    close.eligible_return_remaining = eligible;
    close.owner_request_rent_refundable_remaining = owner_refundable;
    close.reimbursement_observed = true;
    Ok(())
}

/// Informational shortfall: known eligible unreturned reimbursements versus
/// the spendable observed Client balance.  Never blocks anything.
pub fn provider_deficit(eligible_budget: u64, client_balance: u64, client_rent: u64) -> u64 {
    eligible_budget.saturating_sub(client_balance.saturating_sub(client_rent))
}

/// Book an executed provider return of `amount` (spec section 8.4): the owner
/// share `min(amount, owner_request_rent_refundable_remaining)` goes to the
/// owner destination, the rest to `free_balance`.  Returns the owner lamports
/// to transfer out of `OpsCash`.  When the destination cannot receive the
/// share it is re-locked into the close's owner partition
/// (`locked_owner_remaining`, counted in `ops.locked_remaining_total`): before
/// settlement it funds owner-attributed work like the rest of the partition,
/// after settlement it is the deferred refund `deliver_owner_refund` pays.
/// Owner lamports are never converted into `unallocated_excess` (DR-14; Codex
/// M7 review).
pub fn book_return(
    ops: &mut Ops,
    close: &mut CloseRecord,
    amount: u64,
    owner_deliverable: bool,
) -> Result<u64> {
    require!(amount > 0 && amount <= close.eligible_return_remaining, Code::NothingEligible);
    let owner_share = amount.min(close.owner_request_rent_refundable_remaining);
    let to_free = amount - owner_share;
    let eligible_left = checked_sub(close.eligible_return_remaining, amount)?;
    let owner_left = checked_sub(close.owner_request_rent_refundable_remaining, owner_share)?;
    let budget = checked_sub(ops.eligible_return_budget, amount)?;
    let free = checked_add(ops.free_balance, to_free)?;
    let (pay, deferred) = if owner_deliverable { (owner_share, 0) } else { (0, owner_share) };
    let owner_locked = checked_add(close.locked_owner_remaining, deferred)?;
    let locked_total = checked_add(ops.locked_remaining_total, deferred)?;
    let returned = checked_add(close.request_rent_returned, amount)?;
    let returned_total = ops
        .provider_returned_total
        .checked_add(u128::from(amount))
        .ok_or_else(|| error!(Code::Overflow))?;
    close.eligible_return_remaining = eligible_left;
    close.owner_request_rent_refundable_remaining = owner_left;
    close.locked_owner_remaining = owner_locked;
    close.request_rent_returned = returned;
    ops.eligible_return_budget = budget;
    ops.free_balance = free;
    ops.locked_remaining_total = locked_total;
    ops.provider_returned_total = returned_total;
    Ok(pay)
}

/// Record the observed Client balance and recompute the deficit.
pub fn observe_client(ops: &mut Ops, client_lamports: u64, client_rent: u64) {
    ops.client_balance_observed = client_lamports;
    ops.client_protected_rent = client_rent;
    ops.provider_deficit = provider_deficit(ops.eligible_return_budget, client_lamports, client_rent);
}

/// Finalize the close (spec section 4): the full seed carries into the pot
/// once, round-local counters reset and admission reopens.  Returns
/// `(new_round, seed_carried)`.
pub fn book_finalize(pool: &mut Pool, close: &mut CloseRecord) -> Result<(u64, u64)> {
    require!(pool.current_close_id == Some(close.close_id), Code::InvalidState);
    require!(close.cash_committed, Code::InvalidState);
    require!(close.promotion != Promotion::Pending && close.reserve_settled, Code::InvalidState);
    let carried = pool.seed;
    let pot = checked_add(pool.pot, carried)?;
    let new_round = checked_add(pool.round_id, 1)?;
    pool.pot = pot;
    pool.seed = 0;
    pool.round_id = new_round;
    pool.participant_count = 0;
    pool.participant_pages = 0;
    pool.current_close_id = None;
    close.finalized = true;
    Ok((new_round, carried))
}

/// [`book_finalize`] unless the close is already finalized, which is a no-op
/// (`None`) rather than an error: duplicate finalization of one close returns
/// the original result.
pub fn finalize_step(pool: &mut Pool, close: &mut CloseRecord) -> Result<Option<(u64, u64)>> {
    if close.finalized {
        return Ok(None);
    }
    book_finalize(pool, close).map(Some)
}

fn require_current_close(pool: &Pool, close: &CloseRecord) -> Result<()> {
    require!(pool.current_close_id == Some(close.close_id), Code::InvalidState);
    Ok(())
}

fn require_pending(close: &CloseRecord) -> Result<()> {
    require!(close.promotion == Promotion::Pending, Code::PromotionTerminal);
    Ok(())
}

// ---------------------------------------------------------------------------
// Candidate building
// ---------------------------------------------------------------------------

/// Append position of one candidate set for one `build_candidates` call: the
/// page is decoded on its first append and written back once if dirty.
struct AppendTarget<'a, 'info> {
    info: Option<&'a AccountInfo<'info>>,
    tag: u8,
    provisioned: u64,
    page: Option<CandidatePage>,
    dirty: bool,
}

impl<'a, 'info> AppendTarget<'a, 'info> {
    fn new(info: Option<&'a AccountInfo<'info>>, tag: u8, provisioned: u64) -> Self {
        Self { info, tag, provisioned, page: None, dirty: false }
    }

    /// Append `owner` as candidate number `*count`.  The next-append page must
    /// be provisioned (`PageNotProvisioned`), and a batch may not run past the
    /// supplied page into the next one: use fewer rows (`PageNotProvisioned`).
    #[inline(never)]
    fn append(&mut self, close_id: u64, count: &mut u64, digest: &mut [u8; 32], owner: Pubkey, loss: u128) -> Result<()> {
        let index = *count / PAGE;
        let offset = usize::try_from(*count % PAGE).map_err(|_| error!(Code::Overflow))?;
        require!(self.provisioned > index, Code::PageNotProvisioned);
        // A provisioned page exists; the keeper must have supplied it.
        let info = self.info.ok_or_else(|| error!(Code::WrongAccount))?;
        match &self.page {
            None => {
                let page: CandidatePage = read_program_account(info)?;
                require!(
                    page.close_id == close_id && page.set_tag == self.tag && page.page == index,
                    Code::WrongAccount
                );
                let derived = Pubkey::create_program_address(
                    &[CANDIDATES_SEED, &close_id.to_le_bytes(), &[self.tag], &index.to_le_bytes(), &[page.bump]],
                    &crate::ID,
                )
                .map_err(|_| error!(Code::WrongAccount))?;
                require_keys_eq!(derived, info.key(), Code::WrongAccount);
                self.page = Some(page);
            }
            Some(page) => require!(page.page == index, Code::PageNotProvisioned),
        }
        let page = self.page.as_mut().ok_or_else(|| error!(Code::InvalidState))?;
        require!(usize::from(page.len) == offset, Code::InvalidState);
        page.rows[offset] = owner;
        page.len = page.len.checked_add(1).ok_or_else(|| error!(Code::Overflow))?;
        let base = if *count == 0 { sampling::candidate_digest_start(self.tag) } else { *digest };
        *digest = sampling::candidate_digest_next(&base, &owner, loss);
        *count = checked_add(*count, 1)?;
        self.dirty = true;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        match (self.info, &self.page, self.dirty) {
            (Some(info), Some(page), true) => write_program_account(info, page),
            _ => Ok(()),
        }
    }
}

fn derived_matches(seeds: &[&[u8]], key: &Pubkey) -> bool {
    Pubkey::create_program_address(seeds, &crate::ID).is_ok_and(|derived| derived == *key)
}

/// Visit participant slots `cursor..cursor+rows` once, in order, and append
/// each to the candidate sets.  `accounts` is `[participant_page, user_round,
/// user] x rows`; the page repeats across consecutive rows and is decoded once.
/// Returns `true` when this call sealed the sets.
///
/// Rows are re-derived from the participant list, so a keeper cannot skip or
/// reorder a participant: every row must be the owner recorded at its slot,
/// with that owner's own `UserRound` (carrying the same slot) and `User`.
///
/// Each call appends to at most the one page per set that is passed in; a
/// batch whose appends would run into the next page of a set fails with
/// `PageNotProvisioned`, and the keeper retries with fewer rows (one row
/// always fits).
#[inline(never)]
pub fn build_batch<'info>(
    close: &mut CloseRecord,
    preferred_page: Option<&AccountInfo<'info>>,
    fallback_page: Option<&AccountInfo<'info>>,
    accounts: &[AccountInfo<'info>],
    rows: u8,
) -> Result<bool> {
    let count = batch_len(rows)?;
    require!(accounts.len() == count * BUILD_ACCOUNTS_PER_ROW, Code::InvalidState);
    let end = close
        .candidate_cursor
        .checked_add(u64::from(rows))
        .ok_or_else(|| error!(Code::Overflow))?;
    require!(end <= close.participant_count, Code::CursorMismatch);

    let (close_id, round_id) = (close.close_id, close.round_id);
    let mut preferred = AppendTarget::new(preferred_page, SET_PREFERRED, close.provisioned_preferred_pages);
    let mut fallback = AppendTarget::new(fallback_page, SET_FALLBACK, close.provisioned_fallback_pages);
    let (mut preferred_count, mut preferred_digest) = (close.preferred_count, close.preferred_digest);
    let (mut fallback_count, mut fallback_digest) = (close.fallback_count, close.fallback_digest);
    let mut cached: Option<(Pubkey, ParticipantPage)> = None;

    for (i, row) in accounts.chunks_exact(BUILD_ACCOUNTS_PER_ROW).enumerate() {
        let slot = close.candidate_cursor + i as u64;
        let (page_info, round_info, user_info) = (&row[0], &row[1], &row[2]);
        let page_index = slot / PAGE;
        let offset = usize::try_from(slot % PAGE).map_err(|_| error!(Code::Overflow))?;

        if cached.as_ref().map(|(key, _)| *key) != Some(page_info.key()) {
            let page: ParticipantPage = read_program_account(page_info)?;
            require!(page.round_id == round_id && page.page == page_index, Code::WrongAccount);
            require!(
                derived_matches(
                    &[PARTICIPANTS_SEED, &round_id.to_le_bytes(), &page_index.to_le_bytes(), &[page.bump]],
                    page_info.key,
                ),
                Code::WrongAccount
            );
            cached = Some((page_info.key(), page));
        }
        let page = &cached.as_ref().ok_or_else(|| error!(Code::InvalidState))?.1;
        require!(page.page == page_index && offset < usize::from(page.len), Code::WrongAccount);
        let owner = page.rows[offset];

        let user_round: UserRound = read_program_account(round_info)?;
        require!(
            user_round.owner == owner
                && user_round.round_id == round_id
                && user_round.participant_slot == slot
                && derived_matches(
                    &[USER_ROUND_SEED, &round_id.to_le_bytes(), owner.as_ref(), &[user_round.bump]],
                    round_info.key,
                ),
            Code::WrongAccount
        );
        let user: User = read_program_account(user_info)?;
        require!(
            user.owner == owner && derived_matches(&[USER_SEED, owner.as_ref(), &[user.bump]], user_info.key),
            Code::WrongAccount
        );

        let candidacy = classify(user_round.deposits, user_round.net_payout, user.ever_a, user.ever_b);
        if candidacy.fallback {
            fallback.append(close_id, &mut fallback_count, &mut fallback_digest, owner, candidacy.loss)?;
        }
        if candidacy.preferred {
            preferred.append(close_id, &mut preferred_count, &mut preferred_digest, owner, candidacy.loss)?;
        }
    }

    preferred.flush()?;
    fallback.flush()?;
    close.preferred_count = preferred_count;
    close.preferred_digest = preferred_digest;
    close.fallback_count = fallback_count;
    close.fallback_digest = fallback_digest;
    close.candidate_cursor = end;
    if end < close.participant_count {
        return Ok(false);
    }
    // Sealed: an empty set gets its well-defined empty digest, and the draw
    // selects the preferred set when it has anyone, else the fallback set.
    if close.preferred_count == 0 {
        close.preferred_digest = sampling::candidate_digest_start(SET_PREFERRED);
    }
    if close.fallback_count == 0 {
        close.fallback_digest = sampling::candidate_digest_start(SET_FALLBACK);
    }
    close.sealed = true;
    close.selected_set_tag = if close.preferred_count > 0 { SET_PREFERRED } else { SET_FALLBACK };
    Ok(true)
}

// ---------------------------------------------------------------------------
// Account plumbing
// ---------------------------------------------------------------------------

/// Create a program-owned PDA from the close's locked reserve (the `OpsCash`
/// PDA pays; sponsor partition first), booked under `kind`.  Returns the
/// `(sponsor part, owner part)` of the rent for the page's `RentComponents`.
/// A prefunded target lowers the debit by what it already holds.
#[allow(clippy::too_many_arguments)]
fn create_locked_account<'info>(
    target: &AccountInfo<'info>,
    cash: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    seeds: &[&[u8]],
    space: usize,
    ops: &mut Ops,
    close: &mut CloseRecord,
    kind: Spend,
) -> Result<(u64, u64)> {
    crate::sync_ops(ops, cash)?;
    crate::check_create_target(target, system, seeds)?;
    let rent = Rent::get()?.minimum_balance(space);
    let debit = rent.saturating_sub(target.lamports());
    let parts = book_locked_rent(ops, close, debit, kind)?;
    if debit > 0 {
        crate::transfer_from_cash(cash, target, system, debit)?;
    }
    crate::allocate_and_assign(target, system, seeds, space)?;
    Ok(parts)
}

/// An optional account the current path needs: omitted is `WrongAccount`.
fn supplied<'info>(account: &Option<UncheckedAccount<'info>>) -> Result<AccountInfo<'info>> {
    account
        .as_ref()
        .map(|account| account.to_account_info())
        .ok_or_else(|| error!(Code::WrongAccount))
}

fn is_fresh(info: &AccountInfo) -> bool {
    *info.owner == system_program::ID && info.data_is_empty()
}

/// Whether the owner destination can receive `amount` right now.
fn owner_credit_deliverable(destination: &AccountInfo, amount: u64) -> Result<bool> {
    Ok(crate::validate_destination(destination).is_ok()
        && credit_is_deliverable(destination.lamports(), amount, Rent::get()?.minimum_balance(0)))
}

/// Settle the close's reserve (spec section 8.5) when its promotion has just
/// become terminal.  Idempotent.
#[inline(never)]
fn settle<'info>(
    ops: &mut Ops,
    close: &mut CloseRecord,
    cash: &AccountInfo<'info>,
    owner_refund: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
) -> Result<()> {
    if close.reserve_settled {
        return Ok(());
    }
    let owner = close.locked_owner_remaining;
    // The destination matters only when there is an owner remainder to pay.
    if owner > 0 {
        require_keys_eq!(owner_refund.key(), close.owner_refund_destination, Code::WrongAccount);
    }
    crate::sync_ops(ops, cash)?;
    let deliverable = owner > 0 && owner_credit_deliverable(owner_refund, owner)?;
    let pay = settle_books(ops, close, deliverable)?;
    if pay > 0 {
        crate::transfer_from_cash(cash, owner_refund, system, pay)?;
    }
    Ok(())
}

/// Wire form of a skipped promotion event.
fn skipped_event(close: &CloseRecord, reason: u8, stage: u8) -> PromotionSkipped {
    PromotionSkipped {
        schema_version: SCHEMA_VERSION,
        round_id: close.round_id,
        close_id: close.close_id,
        reason,
        stage,
    }
}

// ---------------------------------------------------------------------------
// Instruction accounts
// ---------------------------------------------------------------------------

/// Remaining accounts: `[participant_page, user_round, user] x rows`.
///
/// `preferred_page` / `fallback_page` are the pages at `preferred_count / 8` and
/// `fallback_count / 8`; omit (pass the program id) for a set this call will not
/// append to.  `ops`, `ops_cash` and `owner_refund` are needed only on the call
/// that seals an empty candidate set (it settles the reserve); omit them
/// otherwise.  An omitted account that turns out to be needed is `WrongAccount`.
#[event_cpi]
#[derive(Accounts)]
pub struct BuildCandidates<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    /// CHECK: the preferred-set page receiving the next append; decoded and
    /// re-derived by the handler only when a row appends to the preferred set.
    #[account(mut)]
    pub preferred_page: Option<UncheckedAccount<'info>>,
    /// CHECK: as `preferred_page`, for the fallback set.
    #[account(mut)]
    pub fallback_page: Option<UncheckedAccount<'info>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Option<Box<Account<'info, Ops>>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops` when settling.
    #[account(mut)]
    pub ops_cash: Option<UncheckedAccount<'info>>,
    /// CHECK: must equal `owner_refund_destination` when settling.
    #[account(mut)]
    pub owner_refund: Option<UncheckedAccount<'info>>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct RequestRoundDraw<'info> {
    /// Any signer: ORAO documents `payer` as paying transaction fees only (the
    /// Client pays the request fee and rent).
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(seeds = [CONFIG_SEED, &close_record.config_version.to_le_bytes()], bump = config.bump)]
    pub config: Box<Account<'info, ConfigVersion>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    #[account(seeds = [RNG_AUTHORITY_SEED], bump = rng_authority.bump)]
    pub rng_authority: Box<Account<'info, RngAuthority>>,
    /// CHECK: pinned to `config.provider_program`.
    pub provider_program: UncheckedAccount<'info>,
    /// CHECK: pinned and decoded by `validate_provider_tuple`.
    #[account(mut)]
    pub network_state: UncheckedAccount<'info>,
    /// CHECK: pinned to `config.provider_treasury`.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: pinned and decoded by `validate_provider_tuple`.
    #[account(mut)]
    pub client: UncheckedAccount<'info>,
    /// CHECK: must be the request PDA of `(client, protocol seed)`; created by
    /// the provider CPI.
    #[account(mut)]
    pub request: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct CaptureRoundDraw<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    /// CHECK: bound to the close's stored request by `read_fulfilled`.
    pub request: UncheckedAccount<'info>,
    /// CHECK: must be the Client PDA of this program's RngAuthority.
    pub client: UncheckedAccount<'info>,
}

/// The winner-side accounts (`candidate_page` through `registry_tail`) are
/// optional because an entropy-exhausted promotion reads none of them (pass the
/// program id); on a winning draw each is required and the handler re-derives
/// and verifies every one (`WrongAccount` when omitted).
#[event_cpi]
#[derive(Accounts)]
pub struct ApplyPromotion<'info> {
    /// Pays only the transaction fee; any signer may progress the close.
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(seeds = [CONFIG_SEED, &close_record.config_version.to_le_bytes()], bump = config.bump)]
    pub config: Box<Account<'info, ConfigVersion>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: the candidate page holding the drawn ordinal; re-derived.
    pub candidate_page: Option<UncheckedAccount<'info>>,
    /// CHECK: the drawn owner; must equal the candidate row.
    pub winner: Option<UncheckedAccount<'info>>,
    /// CHECK: `["user", winner]`; re-derived.
    #[account(mut)]
    pub winner_user: Option<UncheckedAccount<'info>>,
    /// CHECK: `["user-round", round, winner]`; re-derived.
    pub winner_user_round: Option<UncheckedAccount<'info>>,
    /// CHECK: `["backer", [1], winner]`; created from the locked reserve when
    /// absent, merged when present.
    #[account(mut)]
    pub winner_backer_b: Option<UncheckedAccount<'info>>,
    /// CHECK: the registry page at `registry_len / 8`; created from the locked
    /// reserve when that page does not exist yet.
    #[account(mut)]
    pub registry_tail: Option<UncheckedAccount<'info>>,
    /// CHECK: must equal `owner_refund_destination`.
    #[account(mut)]
    pub owner_refund: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct ExpirePromotion<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: must equal `owner_refund_destination`.
    #[account(mut)]
    pub owner_refund: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[event_cpi]
#[derive(Accounts)]
pub struct FinalizeRound<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
}

#[derive(Accounts)]
pub struct IncreaseProviderBudget<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    pub ops_cash: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ObserveRequestFulfilled<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: bound to the close's stored request by `read_fulfilled`.
    pub request: UncheckedAccount<'info>,
    /// CHECK: must be the Client PDA of this program's RngAuthority.
    pub client: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct WithdrawProviderReturn<'info> {
    /// CHECK: the Squads Multisig whose index-0 vault is `governance`.
    pub governance_multisig: UncheckedAccount<'info>,
    /// The G vault: signs through the Squads CPI, owns the ORAO Client and
    /// receives the ORAO withdrawal.
    #[account(mut)]
    pub governance: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(seeds = [CONFIG_SEED, &close_record.config_version.to_le_bytes()], bump = config.bump)]
    pub config: Box<Account<'info, ConfigVersion>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: pinned to `config.provider_program`.
    pub provider_program: UncheckedAccount<'info>,
    /// CHECK: pinned and decoded by `validate_client`.
    #[account(mut)]
    pub client: UncheckedAccount<'info>,
    /// CHECK: must equal `owner_refund_destination`.
    #[account(mut)]
    pub owner_refund: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReconcileOpsCashWithClient<'info> {
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: must be the ORAO Client PDA of this program's RngAuthority.
    pub client: UncheckedAccount<'info>,
}

/// `deliver_owner_refund()`: permissionless delivery of a settled close's
/// deferred owner refund to the immutable destination.  No Pool account: the
/// close may already be finalized.
#[event_cpi]
#[derive(Accounts)]
pub struct DeliverOwnerRefund<'info> {
    #[account(mut, seeds = [CLOSE_SEED, &close_record.close_id.to_le_bytes()], bump = close_record.bump)]
    pub close_record: Box<Account<'info, CloseRecord>>,
    #[account(mut, seeds = [OPS_SEED], bump = ops.bump)]
    pub ops: Box<Account<'info, Ops>>,
    /// CHECK: canonical OpsCash PDA; validated by `sync_ops`.
    #[account(mut)]
    pub ops_cash: UncheckedAccount<'info>,
    /// CHECK: must equal `owner_refund_destination`.
    #[account(mut)]
    pub owner_refund: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `build_candidates(rows)`: see [`build_batch`].
pub fn build_candidates<'info>(
    ctx: Context<'_, '_, '_, 'info, BuildCandidates<'info>>,
    rows: u8,
) -> Result<()> {
    let sealed_event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require_current_close(&a.pool, close)?;
        require_pending(close)?;
        require!(close.cash_committed || close.closing_ticket_id.is_none(), Code::InvalidState);
        require!(!close.sealed, Code::InvalidState);
        let preferred = a.preferred_page.as_ref().map(|account| account.to_account_info());
        let fallback = a.fallback_page.as_ref().map(|account| account.to_account_info());
        let sealed = build_batch(close, preferred.as_ref(), fallback.as_ref(), ctx.remaining_accounts, rows)?;
        if !sealed {
            None
        } else {
            let (tag, count, digest) = selected_set(close);
            let skipped = if count == 0 {
                close.promotion = Promotion::Skipped { reason: REASON_NO_CANDIDATES, stage: STAGE_SEALED };
                let ops = a.ops.as_deref_mut().ok_or_else(|| error!(Code::WrongAccount))?;
                let cash = supplied(&a.ops_cash)?;
                let refund = supplied(&a.owner_refund)?;
                settle(ops, close, &cash, &refund, &a.system_program.to_account_info())?;
                Some(skipped_event(close, REASON_NO_CANDIDATES, STAGE_SEALED))
            } else {
                None
            };
            Some((
                CandidatesSealed {
                    schema_version: SCHEMA_VERSION,
                    round_id: close.round_id,
                    close_id: close.close_id,
                    tag,
                    count,
                    digest,
                },
                skipped,
            ))
        }
    };
    if let Some((sealed, skipped)) = sealed_event {
        emit_cpi!(sealed);
        if let Some(skipped) = skipped {
            emit_cpi!(skipped);
        }
    }
    Ok(())
}

/// `request_round_draw()`: transfer `live fee + pending request rent` from
/// `OpsCash` to the Client, then run the ORAO `request` CPI signed by the
/// `RngAuthority`.  The `Requested` marker and every ledger write follow the
/// CPI, so a failed CPI changes nothing.
pub fn request_round_draw(ctx: Context<RequestRoundDraw>) -> Result<()> {
    let now = crate::now()?;
    let event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require_current_close(&a.pool, close)?;
        require_pending(close)?;
        require!(close.sealed, Code::NotSealed);
        let (tag, count, digest) = selected_set(close);
        require!(count > 0, Code::InvalidState);
        require!(close.draw_state == DRAW_NOT_REQUESTED, Code::InvalidState);
        require!(now <= close.promotion_deadline, Code::DeadlinePassed);

        let program = a.provider_program.to_account_info();
        let network = a.network_state.to_account_info();
        let treasury = a.treasury.to_account_info();
        let client = a.client.to_account_info();
        let view = provider::validate_provider_tuple(&a.config, &a.pool.governance, &program, &network, &treasury, &client)?;
        require_keys_eq!(a.rng_authority.client, client.key(), Code::WrongProviderTuple);
        let live_fee = view.request_fee;
        require!(live_fee <= close.approved_provider_fee_cap, Code::ProviderFeeExceeded);
        let rent = Rent::get()?.minimum_balance(usize::from(a.config.pending_request_bytes));
        let holdback = promotion_holdback(close)?;
        let spend = plan_request_spend(close.locked_sponsor_remaining, close.locked_owner_remaining, live_fee, rent, holdback)?;

        let seed = sampling::protocol_seed(&a.pool.cluster_domain, &crate::ID, close.round_id, close.close_id, tag, count, &digest);
        let (request_key, _) = provider::request_binding(&client.key(), &seed);
        let request = a.request.to_account_info();
        require_keys_eq!(request.key(), request_key, Code::WrongAccount);
        require!(!provider::request_exists(&request), Code::Collision);

        // Effects: top the Client up, then let the provider charge it.
        let cash = a.ops_cash.to_account_info();
        let system = a.system_program.to_account_info();
        crate::sync_ops(&mut a.ops, &cash)?;
        crate::transfer_from_cash(&cash, &client, &system, spend.total()?)?;
        provider::cpi_request(
            &program,
            &a.payer.to_account_info(),
            &a.rng_authority.to_account_info(),
            a.rng_authority.bump,
            &client,
            &network,
            &treasury,
            &request,
            &system,
            seed,
        )?;
        require_keys_eq!(*request.owner, orao::id(), Code::InvalidState);
        book_request(&mut a.ops, close, &spend, request_key, seed)?;
        DrawRequested {
            schema_version: SCHEMA_VERSION,
            round_id: close.round_id,
            close_id: close.close_id,
            request_account: request_key,
            fee_paid: live_fee,
            request_rent: rent,
        }
    };
    emit_cpi!(event);
    Ok(())
}

/// `capture_round_draw()`: copy the fulfilled randomness once.  A result seen
/// after the deadline is recorded as `IgnoredLate` and copies nothing.
pub fn capture_round_draw(ctx: Context<CaptureRoundDraw>) -> Result<()> {
    let now = crate::now()?;
    let event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require!(close.close_id < a.pool.next_close_id, Code::InvalidState);
        match close.draw_state {
            // Replays after the decision are no-ops once the binding re-checks.
            DRAW_IGNORED_LATE => return Ok(()),
            DRAW_CAPTURED | DRAW_REQUESTED => {}
            _ => return err!(Code::InvalidState),
        }
        let (client_key, _) = provider::client_address();
        require_keys_eq!(a.client.key(), client_key, Code::WrongProviderTuple);
        let request = a.request.to_account_info();
        require_keys_eq!(request.key(), close.request_account, Code::WrongAccount);
        let (randomness, _) = provider::read_fulfilled(&request, &client_key, &close.protocol_seed)?;
        if close.draw_state == DRAW_CAPTURED {
            require!(close.randomness == randomness, Code::InvalidState);
            return Ok(());
        }
        if now > close.promotion_deadline {
            close.draw_state = DRAW_IGNORED_LATE;
            return Ok(());
        }
        require_pending(close)?;
        close.randomness = randomness;
        close.draw_state = DRAW_CAPTURED;
        close.captured_at = now;
        DrawCaptured {
            schema_version: SCHEMA_VERSION,
            round_id: close.round_id,
            close_id: close.close_id,
            captured_at: now,
        }
    };
    emit_cpi!(event);
    Ok(())
}

struct Winner {
    key: Pubkey,
    loss: u128,
    entitlement: u128,
}

/// The promotion's account work for a drawn ordinal: verify the winner through
/// the sealed candidate page and the sealed `UserRound`, merge the 116%
/// entitlement into the winner's Series-B Backer (created from the locked
/// reserve if absent), mark the history bit and append to the dense registry
/// when the row is open.  No game cash moves.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn apply_winner<'info>(
    pool: &mut Pool,
    close: &mut CloseRecord,
    ops: &mut Ops,
    config: &ConfigVersion,
    ordinal: u64,
    cash: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    candidate_page: &AccountInfo<'info>,
    winner: &AccountInfo<'info>,
    user_info: &AccountInfo<'info>,
    user_round_info: &AccountInfo<'info>,
    backer_info: &AccountInfo<'info>,
    registry_info: &AccountInfo<'info>,
) -> Result<Winner> {
    let (tag, count, _) = selected_set(close);
    require!(ordinal < count, Code::InvalidState);
    let page_index = ordinal / PAGE;
    let offset = usize::try_from(ordinal % PAGE).map_err(|_| error!(Code::Overflow))?;

    let page: CandidatePage = read_program_account(candidate_page)?;
    require!(
        page.close_id == close.close_id && page.set_tag == tag && page.page == page_index,
        Code::WrongAccount
    );
    require!(
        derived_matches(
            &[CANDIDATES_SEED, &close.close_id.to_le_bytes(), &[tag], &page_index.to_le_bytes(), &[page.bump]],
            candidate_page.key,
        ),
        Code::WrongAccount
    );
    require!(offset < usize::from(page.len), Code::WrongAccount);
    let owner = page.rows[offset];
    require_keys_eq!(owner, winner.key(), Code::WrongAccount);

    let user_round: UserRound = read_program_account(user_round_info)?;
    require!(
        user_round.owner == owner
            && user_round.round_id == close.round_id
            && derived_matches(
                &[USER_ROUND_SEED, &close.round_id.to_le_bytes(), owner.as_ref(), &[user_round.bump]],
                user_round_info.key,
            ),
        Code::WrongAccount
    );
    let loss = loss_of(user_round.deposits, user_round.net_payout);
    require!(loss > 0, Code::InvalidState);
    let entitlement = promotion_entitlement(loss, config.b_entitlement_bps)?;

    let mut user: User = read_program_account(user_info)?;
    require!(
        user.owner == owner && derived_matches(&[USER_SEED, owner.as_ref(), &[user.bump]], user_info.key),
        Code::WrongAccount
    );

    let mut backer = if is_fresh(backer_info) {
        let (key, bump) = Pubkey::find_program_address(&[BACKER_SEED, &[SERIES_B], owner.as_ref()], &crate::ID);
        require_keys_eq!(backer_info.key(), key, Code::WrongAccount);
        let (sponsor, owner_part) = create_locked_account(
            backer_info,
            cash,
            system,
            &[BACKER_SEED, &[SERIES_B], owner.as_ref(), &[bump]],
            Backer::SPACE,
            ops,
            close,
            Spend::BAccount,
        )?;
        Backer {
            bump,
            owner,
            series: SERIES_B,
            rent_funded_by_ops: sponsor,
            rent_funded_by_owner: owner_part,
            ..Backer::default()
        }
    } else {
        let existing: Backer = read_program_account(backer_info)?;
        require!(
            existing.owner == owner
                && existing.series == SERIES_B
                && derived_matches(&[BACKER_SEED, &[SERIES_B], owner.as_ref(), &[existing.bump]], backer_info.key),
            Code::WrongAccount
        );
        existing
    };

    backer.entitlement = backer
        .entitlement
        .checked_add(entitlement)
        .ok_or_else(|| error!(Code::Overflow))?;
    let remaining = backer.remaining()?;
    if backer.first_sequence == 0 {
        backer.first_sequence = pool.next_sequence;
        pool.next_sequence = checked_add(pool.next_sequence, 1)?;
    }
    user.ever_b = true;
    user.b_entitlement = user
        .b_entitlement
        .checked_add(entitlement)
        .ok_or_else(|| error!(Code::Overflow))?;

    if !backer.active && remains_open(remaining) {
        append_to_registry(pool, close, ops, &mut backer, backer_info.key(), cash, system, registry_info)?;
    }
    write_program_account(backer_info, &backer)?;
    write_program_account(user_info, &user)?;
    Ok(Winner { key: owner, loss, entitlement })
}

/// Append `backer_key` to the registry tail, creating the tail page from the
/// locked reserve when `registry_len / 8` is past the existing pages.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn append_to_registry<'info>(
    pool: &mut Pool,
    close: &mut CloseRecord,
    ops: &mut Ops,
    backer: &mut Backer,
    backer_key: Pubkey,
    cash: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    registry_info: &AccountInfo<'info>,
) -> Result<()> {
    let index = pool.registry_len / PAGE;
    let offset = usize::try_from(pool.registry_len % PAGE).map_err(|_| error!(Code::Overflow))?;
    let mut page = if index < pool.registry_pages {
        let existing: RegistryPage = read_program_account(registry_info)?;
        require!(existing.index == index, Code::PageNotProvisioned);
        require!(
            derived_matches(&[REGISTRY_SEED, &index.to_le_bytes(), &[existing.bump]], registry_info.key),
            Code::WrongAccount
        );
        existing
    } else {
        require!(index == pool.registry_pages, Code::InvalidState);
        let (key, bump) = Pubkey::find_program_address(&[REGISTRY_SEED, &index.to_le_bytes()], &crate::ID);
        require_keys_eq!(registry_info.key(), key, Code::WrongAccount);
        let (sponsor, owner_part) = create_locked_account(
            registry_info,
            cash,
            system,
            &[REGISTRY_SEED, &index.to_le_bytes(), &[bump]],
            RegistryPage::SPACE,
            ops,
            close,
            Spend::RegistryPage,
        )?;
        pool.registry_pages = checked_add(pool.registry_pages, 1)?;
        RegistryPage {
            bump,
            index,
            keys: [Pubkey::default(); PAGE_CAPACITY],
            rent: RentComponents { ops: sponsor, owner: owner_part, owner_refund: close.owner_refund_destination },
        }
    };
    page.keys[offset] = backer_key;
    backer.active = true;
    backer.slot = pool.registry_len;
    pool.registry_len = checked_add(pool.registry_len, 1)?;
    write_program_account(registry_info, &page)
}

enum PromotionOutcome {
    Applied(Winner),
    Skipped(PromotionSkipped),
}

/// `apply_promotion()`: map the captured randomness to a candidate and grant
/// the Series-B entitlement, or terminalize as entropy-exhausted.  Either way
/// the reserve settles.
pub fn apply_promotion(ctx: Context<ApplyPromotion>) -> Result<()> {
    let outcome = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require_current_close(&a.pool, close)?;
        require_pending(close)?;
        require!(close.draw_state == DRAW_CAPTURED, Code::InvalidState);
        let (_, count, _) = selected_set(close);
        let cash = a.ops_cash.to_account_info();
        let refund = a.owner_refund.to_account_info();
        let system = a.system_program.to_account_info();
        match sampling::map_output(close.randomness, close.round_id, count) {
            None => {
                close.promotion = Promotion::Skipped { reason: REASON_ENTROPY_EXHAUSTED, stage: STAGE_SEALED };
                settle(&mut a.ops, close, &cash, &refund, &system)?;
                PromotionOutcome::Skipped(skipped_event(close, REASON_ENTROPY_EXHAUSTED, STAGE_SEALED))
            }
            Some((ordinal, _counter)) => {
                let winner = apply_winner(
                    &mut a.pool,
                    close,
                    &mut a.ops,
                    &a.config,
                    ordinal,
                    &cash,
                    &system,
                    &supplied(&a.candidate_page)?,
                    &supplied(&a.winner)?,
                    &supplied(&a.winner_user)?,
                    &supplied(&a.winner_user_round)?,
                    &supplied(&a.winner_backer_b)?,
                    &supplied(&a.registry_tail)?,
                )?;
                close.promotion = Promotion::Applied { winner: winner.key, entitlement: winner.entitlement };
                settle(&mut a.ops, close, &cash, &refund, &system)?;
                PromotionOutcome::Applied(winner)
            }
        }
    };
    let (round_id, close_id) = (ctx.accounts.close_record.round_id, ctx.accounts.close_record.close_id);
    match outcome {
        PromotionOutcome::Applied(winner) => {
            emit_cpi!(PromotionApplied {
                schema_version: SCHEMA_VERSION,
                round_id,
                close_id: Some(close_id),
                winner: winner.key,
                // Impossible in practice (2^64 lamports exceeds the SOL supply); saturate
                // rather than let an event field strand an applied promotion.
                loss: u64::try_from(winner.loss).unwrap_or(u64::MAX),
                entitlement: winner.entitlement,
            });
        }
        PromotionOutcome::Skipped(event) => emit_cpi!(event),
    }
    Ok(())
}

/// `expire_promotion()`: skip a promotion whose deadline has passed with no
/// timely capture, and settle the reserve.
pub fn expire_promotion(ctx: Context<ExpirePromotion>) -> Result<()> {
    let now = crate::now()?;
    let event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require_current_close(&a.pool, close)?;
        require_pending(close)?;
        require!(now > close.promotion_deadline, Code::DeadlineNotReached);
        require!(close.draw_state != DRAW_CAPTURED, Code::InvalidState);
        let stage = expiry_stage(close.sealed, close.draw_state)?;
        close.promotion = Promotion::Skipped { reason: REASON_DEADLINE, stage };
        settle(
            &mut a.ops,
            close,
            &a.ops_cash.to_account_info(),
            &a.owner_refund.to_account_info(),
            &a.system_program.to_account_info(),
        )?;
        skipped_event(close, REASON_DEADLINE, stage)
    };
    emit_cpi!(event);
    Ok(())
}

/// `finalize_round()`: carry the full seed into the pot and reopen admission.
/// Finalizing an already-finalized close is a no-op.
pub fn finalize_round(ctx: Context<FinalizeRound>) -> Result<()> {
    let event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        let Some((new_round, seed_carried)) = finalize_step(&mut a.pool, close)? else {
            return Ok(());
        };
        RoundFinalized { schema_version: SCHEMA_VERSION, close_id: close.close_id, new_round, seed_carried }
    };
    emit_cpi!(event);
    Ok(())
}

/// `increase_provider_budget(extra)`: before the request, move `extra` from
/// free sponsor funds into the close's sponsor partition.
pub fn increase_provider_budget(ctx: Context<IncreaseProviderBudget>, extra: u64) -> Result<()> {
    let now = crate::now()?;
    let a = &mut *ctx.accounts;
    let close = &mut *a.close_record;
    require_current_close(&a.pool, close)?;
    require_pending(close)?;
    require!(close.draw_state == DRAW_NOT_REQUESTED, Code::InvalidState);
    require!(now <= close.promotion_deadline, Code::DeadlinePassed);
    crate::sync_ops(&mut a.ops, &a.ops_cash.to_account_info())?;
    book_budget_increase(&mut a.ops, close, extra)
}

/// `observe_request_fulfilled()`: freeze the close's provider reimbursement
/// from the fulfilled request's actual retained lamports, once.
pub fn observe_request_fulfilled(ctx: Context<ObserveRequestFulfilled>) -> Result<()> {
    let a = &mut *ctx.accounts;
    let close = &mut *a.close_record;
    require!(close.close_id < a.pool.next_close_id, Code::InvalidState);
    // A request exists exactly when the draw left `NotRequested`.
    require!(close.draw_state != DRAW_NOT_REQUESTED, Code::InvalidState);
    require!(!close.reimbursement_observed, Code::AlreadyObserved);
    let (client_key, _) = provider::client_address();
    require_keys_eq!(a.client.key(), client_key, Code::WrongProviderTuple);
    let request = a.request.to_account_info();
    require_keys_eq!(request.key(), close.request_account, Code::WrongAccount);
    let (_, lamports) = provider::read_fulfilled(&request, &client_key, &close.protocol_seed)?;
    book_observation(&mut a.ops, close, lamports)
}

/// `withdraw_provider_return(amount)`: G executes the ORAO withdrawal, the
/// program asserts the Client and `OpsCash` deltas in this same instruction and
/// books the return.  Without the executed CPI nothing is credited.
pub fn withdraw_provider_return(ctx: Context<WithdrawProviderReturn>, amount: u64) -> Result<()> {
    let a = &mut *ctx.accounts;
    let close = &mut *a.close_record;
    let governance = a.governance.to_account_info();
    require_vault_signer(&a.governance_multisig.to_account_info(), 0, &governance)?;
    require_keys_eq!(governance.key(), a.pool.governance, Code::NotAuthorized);
    let program = a.provider_program.to_account_info();
    let client = a.client.to_account_info();
    let provider_client = provider::validate_client(&a.config, &a.pool.governance, &program, &client)?;
    require_keys_eq!(provider_client.owner, governance.key(), Code::WrongProviderTuple);
    require!(amount > 0 && amount <= close.eligible_return_remaining, Code::NothingEligible);
    let refund = a.owner_refund.to_account_info();
    require_keys_eq!(refund.key(), close.owner_refund_destination, Code::WrongAccount);
    let cash = a.ops_cash.to_account_info();
    let system = a.system_program.to_account_info();
    crate::sync_ops(&mut a.ops, &cash)?;
    let owner_share = amount.min(close.owner_request_rent_refundable_remaining);
    let deliverable = owner_share > 0 && owner_credit_deliverable(&refund, owner_share)?;

    // 1. ORAO pays the Client owner (the G vault); the Client must shrink by `amount`.
    let client_before = client.lamports();
    let cash_before = cash.lamports();
    provider::cpi_withdraw(&program, &governance, &client, amount)?;
    require!(client.lamports() == checked_sub(client_before, amount)?, Code::InvalidState);
    // 2. G forwards it to OpsCash; OpsCash must grow by `amount`.
    provider::transfer_signed(&governance, &cash, &system, amount)?;
    require!(cash.lamports() == checked_add(cash_before, amount)?, Code::InvalidState);

    // 3. Book it and pay the owner share to the immutable destination.
    let pay = book_return(&mut a.ops, close, amount, deliverable)?;
    if pay > 0 {
        crate::transfer_from_cash(&cash, &refund, &system, pay)?;
    }
    observe_client(&mut a.ops, client.lamports(), Rent::get()?.minimum_balance(client.data_len()));
    Ok(())
}

/// `reconcile_ops_cash_with_client()`: `reconcile_ops_cash` plus the ORAO
/// Client observation (informational; mints no accounted cash).
pub fn reconcile_ops_cash_with_client(ctx: Context<ReconcileOpsCashWithClient>) -> Result<()> {
    let a = &mut *ctx.accounts;
    crate::sync_ops(&mut a.ops, &a.ops_cash.to_account_info())?;
    let (client_key, _) = provider::client_address();
    let client = a.client.to_account_info();
    require_keys_eq!(client.key(), client_key, Code::WrongProviderTuple);
    require_keys_eq!(*client.owner, orao::id(), Code::WrongOwner);
    observe_client(&mut a.ops, client.lamports(), Rent::get()?.minimum_balance(client.data_len()));
    Ok(())
}

/// `deliver_owner_refund()`: pay a settled close's deferred owner refund (its
/// whole `locked_owner_remaining`) to the immutable `owner_refund_destination`
/// once the runtime can deliver it.  Anyone may call it; nothing but the
/// destination can receive the lamports; a second call finds nothing deferred.
/// `RefundNotDeliverable` (fee-only) while the destination still cannot take
/// the credit; the lamports stay owed.
pub fn deliver_owner_refund(ctx: Context<DeliverOwnerRefund>) -> Result<()> {
    let event = {
        let a = &mut *ctx.accounts;
        let close = &mut *a.close_record;
        require!(close.reserve_settled, Code::InvalidState);
        let owed = close.locked_owner_remaining;
        require!(owed > 0, Code::NothingDeferred);
        let refund = a.owner_refund.to_account_info();
        require_keys_eq!(refund.key(), close.owner_refund_destination, Code::WrongAccount);
        let cash = a.ops_cash.to_account_info();
        crate::sync_ops(&mut a.ops, &cash)?;
        require!(owner_credit_deliverable(&refund, owed)?, Code::RefundNotDeliverable);
        let pay = book_delivery(&mut a.ops, close)?;
        crate::transfer_from_cash(&cash, &refund, &a.system_program.to_account_info(), pay)?;
        OwnerRefundDelivered {
            schema_version: SCHEMA_VERSION,
            round_id: close.round_id,
            close_id: close.close_id,
            amount: pay,
            destination: refund.key(),
        }
    };
    emit_cpi!(event);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    const RENT_FLOOR: u64 = 890_880;

    /// A close with `s` sponsor and `o` owner lamports locked, and the matching
    /// `Ops` aggregate (`L = s + o`).
    fn locked(s: u64, o: u64) -> (Ops, CloseRecord) {
        let ops = Ops { locked_remaining_total: s + o, ..Ops::default() };
        let close = CloseRecord {
            close_id: 1,
            round_id: 1,
            locked_sponsor_remaining: s,
            locked_owner_remaining: o,
            required_reserve: s + o,
            approved_provider_fee_cap: 360_000,
            owner_refund_destination: key(0xD0),
            ..CloseRecord::default()
        };
        (ops, close)
    }

    // ---- classification, sampling inputs, entitlement ------------------------

    #[test]
    fn losers_are_fallback_candidates_and_only_history_free_losers_are_preferred() {
        let nobody = classify(500, 200, false, false);
        assert_eq!(nobody, Candidacy { loss: 300, fallback: true, preferred: true });
        for (a, b) in [(true, false), (false, true), (true, true)] {
            let with_history = classify(500, 200, a, b);
            assert_eq!(with_history, Candidacy { loss: 300, fallback: true, preferred: false });
        }
        // Break-even and winners are no candidates at all, whatever their history.
        assert_eq!(classify(500, 500, false, false), Candidacy { loss: 0, fallback: false, preferred: false });
        assert_eq!(classify(500, 900, false, false), Candidacy { loss: 0, fallback: false, preferred: false });
        assert_eq!(loss_of(7, 9), 0);
    }

    #[test]
    fn the_116_percent_entitlement_floors_and_never_overflows_silently() {
        assert_eq!(promotion_entitlement(300_000_000, 11_600).unwrap(), 348_000_000);
        assert_eq!(promotion_entitlement(1, 11_600).unwrap(), 1, "floor(1.16)");
        assert_eq!(promotion_entitlement(9, 11_600).unwrap(), 10, "floor(10.44)");
        assert_eq!(promotion_entitlement(0, 11_600).unwrap(), 0);
        assert_eq!(promotion_entitlement(10_000, 10_000).unwrap(), 10_000);
        // The product is formed in 256 bits: a loss near u128::MAX at 100% still fits...
        assert_eq!(promotion_entitlement(u128::MAX, 10_000).unwrap(), u128::MAX);
        // ...and at 116% it is a reported overflow, not a wrap.
        assert_code(promotion_entitlement(u128::MAX, 11_600), Code::Overflow);
    }

    #[test]
    fn the_selected_set_follows_the_sealed_tag() {
        let close = CloseRecord {
            preferred_count: 4,
            fallback_count: 7,
            preferred_digest: [1; 32],
            fallback_digest: [2; 32],
            selected_set_tag: SET_PREFERRED,
            ..CloseRecord::default()
        };
        assert_eq!(selected_set(&close), (SET_PREFERRED, 4, [1; 32]));
        let close = CloseRecord { selected_set_tag: SET_FALLBACK, ..close };
        assert_eq!(selected_set(&close), (SET_FALLBACK, 7, [2; 32]));
    }

    #[test]
    fn expiry_stage_classifies_where_the_close_got_stuck() {
        assert_eq!(expiry_stage(false, DRAW_NOT_REQUESTED).unwrap(), STAGE_CANDIDATES_INCOMPLETE);
        assert_eq!(expiry_stage(true, DRAW_NOT_REQUESTED).unwrap(), STAGE_UNREQUESTED);
        assert_eq!(expiry_stage(true, DRAW_REQUESTED).unwrap(), STAGE_REQUESTED_UNFULFILLED);
        assert_eq!(expiry_stage(true, DRAW_IGNORED_LATE).unwrap(), STAGE_REQUESTED_UNFULFILLED);
        // A timely capture is never expired: apply it instead.
        assert_code(expiry_stage(true, DRAW_CAPTURED), Code::InvalidState);
        // An unsealed close cannot have a request.
        assert_code(expiry_stage(false, DRAW_REQUESTED), Code::InvalidState);
        assert_ne!(STAGE_SEALED, STAGE_CANDIDATES_INCOMPLETE);
    }

    #[test]
    fn reason_and_stage_codes_are_distinct_and_frozen() {
        assert_eq!((REASON_DEADLINE, REASON_ENTROPY_EXHAUSTED, REASON_NO_CANDIDATES), (1, 2, 3));
        assert_eq!(
            [
                STAGE_CANDIDATES_INCOMPLETE,
                STAGE_PAGES_NOT_PROVISIONED,
                STAGE_UNREQUESTED,
                STAGE_PROVIDER_FEE_EXCEEDED,
                STAGE_REQUESTED_UNFULFILLED,
                STAGE_SEALED,
            ],
            [1, 2, 3, 4, 5, 6]
        );
    }

    // ---- the draw request ----------------------------------------------------

    #[test]
    fn a_request_spends_fee_then_rent_sponsor_first() {
        // Sponsor covers everything.
        let spend = plan_request_spend(10_000_000, 5_000_000, 300_000, 6_173_520, 0).unwrap();
        assert_eq!(spend, RequestSpend { fee_s: 300_000, fee_o: 0, rent_s: 6_173_520, rent_o: 0 });
        assert_eq!(spend.total().unwrap(), 6_473_520);
        // Sponsor runs out inside the rent: the owner funds the rest of the rent.
        let spend = plan_request_spend(6_000_000, 5_000_000, 300_000, 6_173_520, 0).unwrap();
        assert_eq!(spend, RequestSpend { fee_s: 300_000, fee_o: 0, rent_s: 5_700_000, rent_o: 473_520 });
        // Sponsor runs out inside the fee: the owner funds the rest of the fee and all the rent.
        let spend = plan_request_spend(100_000, 9_000_000, 300_000, 6_173_520, 0).unwrap();
        assert_eq!(spend, RequestSpend { fee_s: 100_000, fee_o: 200_000, rent_s: 0, rent_o: 6_173_520 });
        // No sponsor partition at all.
        let spend = plan_request_spend(0, 7_000_000, 300_000, 6_173_520, 0).unwrap();
        assert_eq!(spend, RequestSpend { fee_s: 0, fee_o: 300_000, rent_s: 0, rent_o: 6_173_520 });
        // Exactly the whole reserve is allowed; one lamport more is not.
        assert!(plan_request_spend(3, 4, 3, 4, 0).is_ok());
        assert_code(plan_request_spend(3, 3, 3, 4, 0), Code::ProviderFeeExceeded);
        assert_code(plan_request_spend(0, 0, 1, 0, 0), Code::ProviderFeeExceeded);
        assert_code(plan_request_spend(u64::MAX, u64::MAX, u64::MAX, 1, 0), Code::ProviderFeeExceeded);
    }

    #[test]
    fn a_request_must_leave_the_promotions_own_storage_rent_in_the_reserve() {
        let mut close = CloseRecord::default();
        close.reserve_schedule.b_account_rent = 1_650_000;
        close.reserve_schedule.per_registry_page_rent = 2_800_000;
        assert_eq!(promotion_holdback(&close).unwrap(), 1_650_000);
        close.registry_extension_required = true;
        assert_eq!(promotion_holdback(&close).unwrap(), 4_450_000);
        // 7,000,000 remain; fee + rent is 6,473,520: it fits alone but not with the holdback.
        assert!(plan_request_spend(7_000_000, 0, 300_000, 6_173_520, 0).is_ok());
        assert_code(plan_request_spend(7_000_000, 0, 300_000, 6_173_520, 1_650_000), Code::ProviderFeeExceeded);
        // The holdback is availability only: with enough left it changes no attribution.
        assert_eq!(
            plan_request_spend(7_000_000, 5_000_000, 300_000, 6_173_520, 4_450_000).unwrap(),
            plan_request_spend(7_000_000, 5_000_000, 300_000, 6_173_520, 0).unwrap()
        );
        // Exactly enough is allowed; one lamport short is not.
        assert!(plan_request_spend(10, 0, 3, 4, 3).is_ok());
        assert_code(plan_request_spend(10, 0, 3, 4, 4), Code::ProviderFeeExceeded);
        assert_code(plan_request_spend(10, 0, 3, 4, u64::MAX), Code::ProviderFeeExceeded);
    }

    #[test]
    fn booking_a_request_moves_partitions_ledger_and_provider_counters_together() {
        let (mut ops, mut close) = locked(6_000_000, 5_000_000);
        let spend = plan_request_spend(6_000_000, 5_000_000, 300_000, 6_173_520, 0).unwrap();
        book_request(&mut ops, &mut close, &spend, key(0xA1), [0xB2; 32]).unwrap();
        assert_eq!(close.locked_sponsor_remaining, 0);
        assert_eq!(close.locked_owner_remaining, 11_000_000 - 6_473_520);
        assert_eq!(ops.locked_remaining_total, close.locked_sponsor_remaining + close.locked_owner_remaining);
        assert_eq!(close.spent.provider_fee_s, 300_000);
        assert_eq!(close.spent.provider_fee_o, 0);
        assert_eq!(close.spent.request_rent_s, 5_700_000);
        assert_eq!(close.spent.request_rent_o, 473_520);
        assert_eq!((close.request_rent_funded_s, close.request_rent_funded_o), (5_700_000, 473_520));
        assert_eq!(ops.provider_fee_consumed_total, 300_000);
        assert_eq!(ops.request_rent_outstanding, 6_173_520);
        assert_eq!((close.request_account, close.protocol_seed), (key(0xA1), [0xB2; 32]));
        assert_eq!(close.draw_state, DRAW_REQUESTED);
        // Nothing is refundable until the fulfillment is observed.
        assert_eq!((close.eligible_return_remaining, close.owner_request_rent_refundable_remaining), (0, 0));
        assert_eq!(ops.eligible_return_budget, 0);
        assert!(!close.reimbursement_observed);
    }

    #[test]
    fn booking_a_request_that_cannot_be_covered_changes_nothing() {
        // The aggregate is below the spend: the partitions say yes, the books say no.
        let (mut ops, mut close) = locked(1_000, 1_000);
        ops.locked_remaining_total = 10;
        let spend = RequestSpend { fee_s: 100, fee_o: 0, rent_s: 100, rent_o: 0 };
        assert_code(book_request(&mut ops, &mut close, &spend, key(1), [1; 32]), Code::InvalidState);
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (1_000, 1_000));
        assert_eq!(close.draw_state, DRAW_NOT_REQUESTED);
        assert_eq!(close.spent, SpentLedgerExt::zero());
        assert_eq!((ops.provider_fee_consumed_total, ops.request_rent_outstanding), (0, 0));
    }

    /// `SpentLedger::default()` spelled out so the assertion reads as intent.
    struct SpentLedgerExt;
    impl SpentLedgerExt {
        fn zero() -> crate::state::SpentLedger {
            crate::state::SpentLedger::default()
        }
    }

    #[test]
    fn locked_rent_is_booked_under_its_own_category_sponsor_first() {
        let (mut ops, mut close) = locked(100, 500);
        // B account rent 150: 100 from the sponsor, 50 from the owner.
        let parts = book_locked_rent(&mut ops, &mut close, 150, Spend::BAccount).unwrap();
        assert_eq!(parts, (100, 50));
        assert_eq!((close.spent.b_rent_s, close.spent.b_rent_o), (100, 50));
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 450));
        // Registry page 200, all from the owner now.
        let parts = book_locked_rent(&mut ops, &mut close, 200, Spend::RegistryPage).unwrap();
        assert_eq!(parts, (0, 200));
        assert_eq!((close.spent.registry_rent_s, close.spent.registry_rent_o), (0, 200));
        assert_eq!(close.spent.candidate_rent_s + close.spent.candidate_rent_o, 0, "candidate category untouched");
        assert_eq!(ops.locked_remaining_total, 250);
        assert_eq!(ops.total_rent_funded, 350);
        assert_eq!(ops.free_balance, 0, "free_balance is never touched");
    }

    #[test]
    fn locked_rent_beyond_the_reserve_is_refused_and_changes_nothing() {
        let (mut ops, mut close) = locked(100, 50);
        assert_code(book_locked_rent(&mut ops, &mut close, 151, Spend::BAccount), Code::LockNotPermitted);
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (100, 50));
        assert_eq!((ops.locked_remaining_total, ops.total_rent_funded), (150, 0));
        assert_eq!(close.spent.b_rent_s + close.spent.b_rent_o, 0);
    }

    // ---- budget, settlement ----------------------------------------------------

    #[test]
    fn a_budget_increase_moves_free_funds_into_the_sponsor_partition_and_the_cap() {
        let (mut ops, mut close) = locked(1_000, 400);
        ops.free_balance = 5_000;
        book_budget_increase(&mut ops, &mut close, 700).unwrap();
        assert_eq!(ops.free_balance, 4_300);
        assert_eq!(close.locked_sponsor_remaining, 1_700);
        assert_eq!(close.locked_owner_remaining, 400, "the owner partition never moves");
        assert_eq!(close.approved_provider_fee_cap, 360_700);
        assert_eq!(close.required_reserve, 1_400 + 700);
        assert_eq!(ops.locked_remaining_total, 2_100);
        assert_code(book_budget_increase(&mut ops, &mut close, 0), Code::InvalidState);
        assert_code(book_budget_increase(&mut ops, &mut close, 4_301), Code::InsufficientOperations);
        assert_eq!(ops.free_balance, 4_300, "a refused increase moves nothing");
    }

    #[test]
    fn settlement_returns_only_the_remaining_partitions_exactly_once() {
        let (mut ops, mut close) = locked(700, 300);
        ops.free_balance = 50;
        ops.locked_remaining_total += 9_000; // another close's reserve
        let pay = settle_books(&mut ops, &mut close, true).unwrap();
        assert_eq!(pay, 300, "the owner remainder is paid out of OpsCash");
        assert_eq!(ops.free_balance, 750, "the sponsor remainder returns to free");
        assert_eq!(ops.locked_remaining_total, 9_000, "only this close's s + o leave the aggregate");
        assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (0, 0));
        assert!(close.reserve_settled);
        // A second settlement is a no-op.
        assert_eq!(settle_books(&mut ops, &mut close, true).unwrap(), 0);
        assert_eq!(ops.free_balance, 750);
        assert_eq!(ops.locked_remaining_total, 9_000);
    }

    #[test]
    fn settlement_never_refunds_what_was_already_spent() {
        let (mut ops, mut close) = locked(1_000, 500);
        let spend = plan_request_spend(1_000, 500, 100, 600, 0).unwrap();
        book_request(&mut ops, &mut close, &spend, key(1), [1; 32]).unwrap();
        book_locked_rent(&mut ops, &mut close, 400, Spend::BAccount).unwrap();
        let remaining = close.locked_sponsor_remaining + close.locked_owner_remaining;
        assert_eq!(remaining, 1_500 - 700 - 400);
        let pay = settle_books(&mut ops, &mut close, true).unwrap();
        assert_eq!(ops.free_balance + pay, remaining);
        assert_eq!(ops.locked_remaining_total, 0);
    }

    #[test]
    fn an_undeliverable_owner_destination_defers_the_remainder_to_the_owner_without_blocking_finalization() {
        let (mut ops, mut close) = locked(10, 300);
        let pay = settle_books(&mut ops, &mut close, false).unwrap();
        assert_eq!(pay, 0);
        assert_eq!(ops.unallocated_excess, 0, "owner lamports never become anonymous excess");
        assert_eq!(ops.free_balance, 10, "the sponsor remainder is released");
        assert_eq!(ops.locked_remaining_total, 300, "the owner remainder stays locked to the owner");
        assert_eq!(close.locked_owner_remaining, 300);
        assert_eq!(close.locked_sponsor_remaining, 0);
        assert!(close.reserve_settled, "settlement still completes");
        // Settlement is idempotent and does not pay the deferred amount.
        assert_eq!(settle_books(&mut ops, &mut close, true).unwrap(), 0);
        assert_eq!(close.locked_owner_remaining, 300);
        // Delivery pays the whole deferred amount exactly once.
        assert_eq!(book_delivery(&mut ops, &mut close).unwrap(), 300);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total), (0, 0));
        assert_code(book_delivery(&mut ops, &mut close), Code::NothingDeferred);
        // With nothing owed to the owner nothing is deferred.
        let (mut ops, mut close) = locked(10, 0);
        assert_eq!(settle_books(&mut ops, &mut close, false).unwrap(), 0);
        assert_eq!((ops.unallocated_excess, ops.locked_remaining_total), (0, 0));
        assert_code(book_delivery(&mut ops, &mut close), Code::NothingDeferred);
    }

    #[test]
    fn a_deliverable_owner_destination_is_paid_at_settlement_and_nothing_is_deferred() {
        let (mut ops, mut close) = locked(10, 300);
        assert_eq!(settle_books(&mut ops, &mut close, true).unwrap(), 300);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total, ops.free_balance), (0, 0, 10));
        assert_code(book_delivery(&mut ops, &mut close), Code::NothingDeferred);
    }

    #[test]
    fn delivery_requires_a_settled_close() {
        let (mut ops, mut close) = locked(10, 300);
        assert_code(book_delivery(&mut ops, &mut close), Code::InvalidState);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total), (300, 310));
    }

    /// The runtime rule (Agave rent-state transitions): the post-balance of a
    /// data-less destination must reach the rent-exempt minimum; a rent-paying
    /// account may not be credited while staying rent-paying.
    #[test]
    fn a_credit_is_deliverable_only_when_the_destination_ends_rent_exempt() {
        assert!(!credit_is_deliverable(0, 1, RENT_FLOOR));
        assert!(!credit_is_deliverable(0, RENT_FLOOR - 1, RENT_FLOOR));
        assert!(credit_is_deliverable(0, RENT_FLOOR, RENT_FLOOR));
        assert!(!credit_is_deliverable(1, 1, RENT_FLOOR), "a legacy rent-paying wallet cannot be topped up below the minimum");
        assert!(!credit_is_deliverable(1, RENT_FLOOR - 2, RENT_FLOOR));
        assert!(credit_is_deliverable(1, RENT_FLOOR - 1, RENT_FLOOR), "reaching the minimum exactly is allowed");
        assert!(credit_is_deliverable(RENT_FLOOR, 1, RENT_FLOOR), "a rent-exempt wallet can receive any credit");
        assert!(credit_is_deliverable(u64::MAX, 1, RENT_FLOOR), "no overflow at the top of the range");
    }

    // ---- provider cash path ---------------------------------------------------

    #[test]
    fn eligibility_is_funded_minus_the_actual_retained_lamports() {
        // The runtime receipt: 6,173,520 funded, 4,259,520 reimbursed, 1,914,000 retained.
        let (funded, eligible, owner) = eligibility(6_000_000, 173_520, 1_914_000).unwrap();
        assert_eq!((funded, eligible, owner), (6_173_520, 4_259_520, 173_520));
        // Extra lamports on the request account can only lower it, never raise it.
        let mut previous = eligible;
        for extra in [0u64, 1, 1_000, 4_000_000, 10_000_000] {
            let (_, now_eligible, _) = eligibility(6_000_000, 173_520, 1_914_000 + extra).unwrap();
            assert!(now_eligible <= previous);
            previous = now_eligible;
        }
        assert_eq!(previous, 0, "saturates at zero rather than wrapping");
        // The owner is reimbursed first but only up to what it funded.
        assert_eq!(eligibility(100, 400, 450).unwrap(), (500, 50, 50));
        assert_eq!(eligibility(100, 400, 0).unwrap(), (500, 500, 400));
        assert_code(eligibility(u64::MAX, 1, 0), Code::Overflow);
    }

    #[test]
    fn observation_freezes_eligibility_once_and_clears_the_outstanding_rent() {
        let (mut ops, mut close) = locked(0, 0);
        close.request_rent_funded_s = 6_000_000;
        close.request_rent_funded_o = 173_520;
        close.draw_state = DRAW_REQUESTED;
        ops.request_rent_outstanding = 6_173_520;
        ops.eligible_return_budget = 7;
        book_observation(&mut ops, &mut close, 1_914_000).unwrap();
        assert_eq!(close.eligible_return_remaining, 4_259_520);
        assert_eq!(close.owner_request_rent_refundable_remaining, 173_520);
        assert_eq!(ops.eligible_return_budget, 7 + 4_259_520);
        assert_eq!(ops.request_rent_outstanding, 0);
        assert!(close.reimbursement_observed);
        // Once only, and later balance changes cannot raise it.
        assert_code(book_observation(&mut ops, &mut close, 0), Code::AlreadyObserved);
        assert_eq!(close.eligible_return_remaining, 4_259_520);
    }

    #[test]
    fn a_second_close_cannot_claim_the_first_closes_reimbursement() {
        // Each close freezes its own eligibility from its own funded rent.
        let (mut ops, mut first) = locked(0, 0);
        first.request_rent_funded_s = 1_000;
        ops.request_rent_outstanding = 3_000;
        let mut second = CloseRecord { close_id: 2, request_rent_funded_s: 2_000, ..CloseRecord::default() };
        book_observation(&mut ops, &mut first, 400).unwrap();
        assert_eq!(first.eligible_return_remaining, 600);
        book_observation(&mut ops, &mut second, 500).unwrap();
        assert_eq!(second.eligible_return_remaining, 1_500);
        assert_eq!(ops.eligible_return_budget, 2_100);
        // Returning against the first cannot exceed the first's own remainder.
        assert_code(book_return(&mut ops, &mut first, 601, true), Code::NothingEligible);
    }

    #[test]
    fn two_partial_returns_pay_the_owner_residual_exactly_once() {
        let (mut ops, mut close) = locked(0, 0);
        close.request_rent_funded_s = 6_000_000;
        close.request_rent_funded_o = 173_520;
        ops.request_rent_outstanding = 6_173_520;
        book_observation(&mut ops, &mut close, 1_914_000).unwrap();
        ops.free_balance = 0;

        // First return is smaller than the owner's refundable share: all of it is owner money.
        let pay = book_return(&mut ops, &mut close, 100_000, true).unwrap();
        assert_eq!(pay, 100_000);
        assert_eq!(ops.free_balance, 0);
        assert_eq!(close.owner_request_rent_refundable_remaining, 73_520);
        assert_eq!(close.eligible_return_remaining, 4_159_520);
        // Second return covers the owner residual once and the rest goes to free.
        let pay = book_return(&mut ops, &mut close, 4_159_520, true).unwrap();
        assert_eq!(pay, 73_520);
        assert_eq!(ops.free_balance, 4_086_000);
        assert_eq!(close.owner_request_rent_refundable_remaining, 0);
        assert_eq!(close.eligible_return_remaining, 0);
        assert_eq!(ops.eligible_return_budget, 0);
        assert_eq!(close.request_rent_returned, 4_259_520);
        assert_eq!(ops.provider_returned_total, 4_259_520);
        // The total paid to the owner is exactly what it funded, and a third call finds nothing.
        assert_eq!(100_000 + 73_520, 173_520);
        assert_code(book_return(&mut ops, &mut close, 1, true), Code::NothingEligible);
        assert_eq!(ops.free_balance, 4_086_000, "a replay pays nothing");
    }

    #[test]
    fn a_return_must_be_positive_and_within_what_is_eligible() {
        let (mut ops, mut close) = locked(0, 0);
        close.eligible_return_remaining = 500;
        ops.eligible_return_budget = 500;
        assert_code(book_return(&mut ops, &mut close, 0, true), Code::NothingEligible);
        assert_code(book_return(&mut ops, &mut close, 501, true), Code::NothingEligible);
        assert_eq!(close.eligible_return_remaining, 500);
        assert_eq!(book_return(&mut ops, &mut close, 500, true).unwrap(), 0, "no owner share was funded");
        assert_eq!(ops.free_balance, 500, "an owner-free return is all free balance");
    }

    #[test]
    fn an_undeliverable_owner_share_is_relocked_for_the_owner_not_lost() {
        let (mut ops, mut close) = locked(0, 0);
        close.eligible_return_remaining = 1_000;
        close.owner_request_rent_refundable_remaining = 400;
        ops.eligible_return_budget = 1_000;
        let pay = book_return(&mut ops, &mut close, 1_000, false).unwrap();
        assert_eq!(pay, 0);
        assert_eq!(ops.unallocated_excess, 0, "owner lamports never become anonymous excess");
        assert_eq!(ops.free_balance, 600);
        assert_eq!(close.owner_request_rent_refundable_remaining, 0, "the share is accounted exactly once");
        assert_eq!(close.locked_owner_remaining, 400, "re-locked into the owner partition");
        assert_eq!(ops.locked_remaining_total, 400);
        // Not yet settled: the partition settles with the rest at terminal
        // promotion (paid when deliverable, deferred otherwise).
        assert_code(book_delivery(&mut ops, &mut close), Code::InvalidState);
        assert_eq!(settle_books(&mut ops, &mut close, false).unwrap(), 0);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total), (400, 400));
        assert_eq!(book_delivery(&mut ops, &mut close).unwrap(), 400);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total), (0, 0));
    }

    #[test]
    fn an_undeliverable_owner_share_after_settlement_is_deferred_until_delivered() {
        let (mut ops, mut close) = locked(0, 0);
        assert_eq!(settle_books(&mut ops, &mut close, true).unwrap(), 0);
        close.eligible_return_remaining = 1_000;
        close.owner_request_rent_refundable_remaining = 400;
        ops.eligible_return_budget = 1_000;
        assert_eq!(book_return(&mut ops, &mut close, 1_000, false).unwrap(), 0);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total, ops.free_balance), (400, 400, 600));
        assert_eq!(book_delivery(&mut ops, &mut close).unwrap(), 400);
        assert_eq!((close.locked_owner_remaining, ops.locked_remaining_total), (0, 0));
    }

    #[test]
    fn the_provider_deficit_is_informational_and_a_real_return_lowers_it() {
        assert_eq!(provider_deficit(5_000, 9_000, 1_000), 0, "spendable balance covers the budget");
        assert_eq!(provider_deficit(5_000, 3_000, 1_000), 3_000);
        assert_eq!(provider_deficit(5_000, 500, 1_000), 5_000, "balance below rent counts as zero");
        let mut ops = Ops { eligible_return_budget: 5_000, ..Ops::default() };
        observe_client(&mut ops, 3_000, 1_000);
        assert_eq!((ops.client_balance_observed, ops.client_protected_rent, ops.provider_deficit), (3_000, 1_000, 3_000));
        // G withdrew elsewhere: the deficit grows, nothing else moves.
        observe_client(&mut ops, 1_000, 1_000);
        assert_eq!(ops.provider_deficit, 5_000);
        assert_eq!(ops.free_balance, 0);
        // A real return shrinks the budget and the deficit with it.
        ops.eligible_return_budget = 2_000;
        observe_client(&mut ops, 2_500, 1_000);
        assert_eq!(ops.provider_deficit, 500);
    }

    // ---- finalization ----------------------------------------------------------

    fn finalizable() -> (Pool, CloseRecord) {
        let pool = Pool {
            round_id: 1,
            pot: 5,
            seed: 13_200_000,
            participant_count: 20,
            participant_pages: 3,
            current_close_id: Some(1),
            next_close_id: 2,
            ..Pool::default()
        };
        let close = CloseRecord {
            close_id: 1,
            round_id: 1,
            cash_committed: true,
            promotion: Promotion::Skipped { reason: REASON_DEADLINE, stage: STAGE_UNREQUESTED },
            reserve_settled: true,
            ..CloseRecord::default()
        };
        (pool, close)
    }

    #[test]
    fn finalize_carries_the_full_seed_once_and_reopens_the_next_round() {
        let (mut pool, mut close) = finalizable();
        assert_eq!(finalize_step(&mut pool, &mut close).unwrap(), Some((2, 13_200_000)));
        assert_eq!((pool.pot, pool.seed), (13_200_005, 0));
        assert_eq!(pool.round_id, 2);
        assert_eq!((pool.participant_count, pool.participant_pages), (0, 0));
        assert_eq!(pool.current_close_id, None);
        assert_eq!(pool.next_close_id, 2, "a permanent CloseRecord id is never reused");
        assert!(close.finalized);
        // The same close again is a no-op that changes nothing.
        assert_eq!(finalize_step(&mut pool, &mut close).unwrap(), None);
        assert_eq!((pool.pot, pool.seed, pool.round_id), (13_200_005, 0, 2));
    }

    #[test]
    fn finalize_requires_committed_cash_a_terminal_promotion_and_the_current_close() {
        let (mut pool, mut close) = finalizable();
        close.cash_committed = false;
        assert_code(finalize_step(&mut pool, &mut close), Code::InvalidState);
        close.cash_committed = true;
        close.promotion = Promotion::Pending;
        assert_code(finalize_step(&mut pool, &mut close), Code::InvalidState);
        close.promotion = Promotion::Skipped { reason: REASON_DEADLINE, stage: STAGE_UNREQUESTED };
        close.reserve_settled = false;
        assert_code(finalize_step(&mut pool, &mut close), Code::InvalidState);
        close.reserve_settled = true;
        pool.current_close_id = Some(2);
        assert_code(finalize_step(&mut pool, &mut close), Code::InvalidState);
        pool.current_close_id = Some(1);
        pool.pot = u64::MAX;
        assert_code(finalize_step(&mut pool, &mut close), Code::Overflow);
        assert_eq!((pool.seed, pool.round_id, pool.current_close_id), (13_200_000, 1, Some(1)));
        assert!(!close.finalized, "a refused finalization changes nothing");
    }

    // ---- conservation ------------------------------------------------------------

    /// A model of the OpsCash balance next to the books, checking the spec
    /// section 8.6 identity `lamports - protected_rent == free + L + excess`
    /// after every transition.
    struct Model {
        cash: u64,
        ops: Ops,
        close: CloseRecord,
    }

    impl Model {
        fn identity(&self, step: &str) {
            let recorded = self.ops.free_balance + self.ops.locked_remaining_total + self.ops.unallocated_excess;
            assert_eq!(self.cash - self.ops.protected_rent, recorded, "{step}");
            assert_eq!(
                self.ops.locked_remaining_total,
                self.close.locked_sponsor_remaining + self.close.locked_owner_remaining,
                "{step}: L equals the sum of the remaining partitions"
            );
        }
    }

    #[test]
    fn locked_total_equals_the_remaining_partitions_after_every_transition() {
        let mut m = Model {
            cash: RENT_FLOOR + 2_000,
            ops: Ops { protected_rent: RENT_FLOOR, free_balance: 2_000, ..Ops::default() },
            close: CloseRecord { close_id: 1, approved_provider_fee_cap: 100, ..CloseRecord::default() },
        };
        m.identity("genesis");
        // Closing payout locks R = 3,000: sponsor 2,000 from free plus 1,000 from the owner wallet.
        m.ops.free_balance -= 2_000;
        m.ops.locked_remaining_total += 3_000;
        m.close.locked_sponsor_remaining = 2_000;
        m.close.locked_owner_remaining = 1_000;
        m.close.required_reserve = 3_000;
        m.cash += 1_000;
        m.identity("lock");
        // A candidate page (rent 500) from the locked reserve.
        book_locked_rent(&mut m.ops, &mut m.close, 500, Spend::BAccount).unwrap();
        m.cash -= 500;
        m.identity("locked rent");
        // The draw request: fee 100, rent 700.
        let spend = plan_request_spend(m.close.locked_sponsor_remaining, m.close.locked_owner_remaining, 100, 700, 0).unwrap();
        book_request(&mut m.ops, &mut m.close, &spend, key(7), [7; 32]).unwrap();
        m.cash -= 800;
        m.identity("request");
        // A budget increase before the request would move free into the sponsor partition.
        m.ops.free_balance += 300;
        m.cash += 300; // a fresh sponsor contribution
        book_budget_increase(&mut m.ops, &mut m.close, 300).unwrap();
        m.identity("budget increase");
        // Settlement pays the owner remainder out and frees the sponsor remainder.
        let pay = settle_books(&mut m.ops, &mut m.close, true).unwrap();
        m.cash -= pay;
        m.identity("settlement");
        assert_eq!(m.ops.locked_remaining_total, 0);
        // The reimbursement is observed (no cash moves), then returned with an owner share.
        m.close.request_rent_funded_s = 400;
        m.close.request_rent_funded_o = 300;
        m.ops.request_rent_outstanding = 700;
        book_observation(&mut m.ops, &mut m.close, 200).unwrap();
        m.identity("observation");
        let pay = book_return(&mut m.ops, &mut m.close, 500, true).unwrap();
        m.cash += 500; // G forwarded the withdrawal to OpsCash
        m.cash -= pay;
        m.identity("provider return");
        assert_eq!(m.close.eligible_return_remaining, 0);
    }

    // ---- candidate building ---------------------------------------------------------

    use anchor_lang::solana_program::system_program;

    fn info(key: Pubkey, owner: Pubkey, data: Vec<u8>) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(key));
        let owner = Box::leak(Box::new(owner));
        let lamports = Box::leak(Box::new(1_000_000u64));
        let data = Box::leak(data.into_boxed_slice());
        AccountInfo::new(key, false, true, lamports, data, owner, false, 0)
    }

    fn account_bytes<T: AccountSerialize>(value: &T, space: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; space];
        let mut cursor: &mut [u8] = &mut bytes;
        value.try_serialize(&mut cursor).expect("account fits its space");
        bytes
    }

    fn owner_key(index: usize) -> Pubkey {
        let mut bytes = [0xAA; 32];
        bytes[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
        Pubkey::new_from_array(bytes)
    }

    #[derive(Clone, Copy)]
    struct Part {
        deposits: u128,
        net: u128,
        ever_a: bool,
        ever_b: bool,
    }

    const WINNER: Part = Part { deposits: 500, net: 500, ever_a: false, ever_b: false };
    fn loser(loss: u128) -> Part {
        Part { deposits: 1_000, net: 1_000 - loss, ever_a: false, ever_b: false }
    }
    fn loser_with_a(loss: u128) -> Part {
        Part { ever_a: true, ..loser(loss) }
    }

    const ROUND: u64 = 1;
    const CLOSE: u64 = 1;

    /// Participant pages, `UserRound`s, `User`s and candidate pages as
    /// program-owned accounts, plus the bound `CloseRecord`.
    struct World {
        close: CloseRecord,
        owners: Vec<Pubkey>,
        participants: Vec<AccountInfo<'static>>,
        rounds: Vec<AccountInfo<'static>>,
        users: Vec<AccountInfo<'static>>,
        preferred: Vec<AccountInfo<'static>>,
        fallback: Vec<AccountInfo<'static>>,
    }

    impl World {
        fn new(parts: &[Part], preferred_pages: u64, fallback_pages: u64) -> Self {
            let n = parts.len();
            let owners: Vec<Pubkey> = (0..n).map(owner_key).collect();
            let mut participants = Vec::new();
            for index in 0..n.div_ceil(PAGE_CAPACITY) {
                let (address, bump) = Pubkey::find_program_address(
                    &[PARTICIPANTS_SEED, &ROUND.to_le_bytes(), &(index as u64).to_le_bytes()],
                    &crate::ID,
                );
                let mut page = ParticipantPage { bump, round_id: ROUND, page: index as u64, ..ParticipantPage::default() };
                for (offset, owner) in owners.iter().skip(index * PAGE_CAPACITY).take(PAGE_CAPACITY).enumerate() {
                    page.rows[offset] = *owner;
                    page.len += 1;
                }
                participants.push(info(address, crate::ID, account_bytes(&page, ParticipantPage::SPACE)));
            }
            let mut rounds = Vec::new();
            let mut users = Vec::new();
            for (slot, (owner, part)) in owners.iter().zip(parts).enumerate() {
                let (address, bump) = Pubkey::find_program_address(
                    &[USER_ROUND_SEED, &ROUND.to_le_bytes(), owner.as_ref()],
                    &crate::ID,
                );
                let round = UserRound {
                    bump,
                    round_id: ROUND,
                    owner: *owner,
                    deposits: part.deposits,
                    net_payout: part.net,
                    participant_slot: slot as u64,
                };
                rounds.push(info(address, crate::ID, account_bytes(&round, UserRound::SPACE)));
                let (address, bump) = Pubkey::find_program_address(&[USER_SEED, owner.as_ref()], &crate::ID);
                let user = User { bump, owner: *owner, ever_a: part.ever_a, ever_b: part.ever_b, ..User::default() };
                users.push(info(address, crate::ID, account_bytes(&user, User::SPACE)));
            }
            let candidate_pages = |tag: u8, pages: u64| -> Vec<AccountInfo<'static>> {
                (0..pages)
                    .map(|index| {
                        let (address, bump) = Pubkey::find_program_address(
                            &[CANDIDATES_SEED, &CLOSE.to_le_bytes(), &[tag], &index.to_le_bytes()],
                            &crate::ID,
                        );
                        let page = CandidatePage { bump, close_id: CLOSE, set_tag: tag, page: index, ..CandidatePage::default() };
                        info(address, crate::ID, account_bytes(&page, CandidatePage::SPACE))
                    })
                    .collect()
            };
            let close = CloseRecord {
                close_id: CLOSE,
                round_id: ROUND,
                participant_count: n as u64,
                provisioned_preferred_pages: preferred_pages,
                provisioned_fallback_pages: fallback_pages,
                cash_committed: true,
                ..CloseRecord::default()
            };
            Self {
                close,
                owners,
                participants,
                rounds,
                users,
                preferred: candidate_pages(SET_PREFERRED, preferred_pages),
                fallback: candidate_pages(SET_FALLBACK, fallback_pages),
            }
        }

        fn rows(&self, from: usize, rows: usize) -> Vec<AccountInfo<'static>> {
            (from..from + rows)
                .flat_map(|slot| {
                    [
                        self.participants[slot / PAGE_CAPACITY].clone(),
                        self.rounds[slot].clone(),
                        self.users[slot].clone(),
                    ]
                })
                .collect()
        }

        fn placeholder() -> AccountInfo<'static> {
            info(crate::ID, system_program::ID, Vec::new())
        }

        /// The page a keeper supplies for `set`: the one at `count / 8`, or
        /// nothing (the program id) when that page is not provisioned.
        fn append_page(&self, set: u8, count: u64) -> Option<AccountInfo<'static>> {
            let pages = if set == SET_PREFERRED { &self.preferred } else { &self.fallback };
            pages.get((count / PAGE) as usize).cloned()
        }

        fn build(&mut self, rows: u8) -> Result<bool> {
            let accounts = self.rows(self.close.candidate_cursor as usize, usize::from(rows));
            let preferred = self.append_page(SET_PREFERRED, self.close.preferred_count);
            let fallback = self.append_page(SET_FALLBACK, self.close.fallback_count);
            build_batch(&mut self.close, preferred.as_ref(), fallback.as_ref(), &accounts, rows)
        }

        fn page(&self, set: u8, index: usize) -> CandidatePage {
            let pages = if set == SET_PREFERRED { &self.preferred } else { &self.fallback };
            let data = pages[index].try_borrow_data().unwrap();
            let mut bytes: &[u8] = &data;
            CandidatePage::try_deserialize(&mut bytes).unwrap()
        }
    }

    fn chain(tag: u8, rows: &[(Pubkey, u128)]) -> [u8; 32] {
        rows.iter().fold(sampling::candidate_digest_start(tag), |digest, (owner, loss)| {
            sampling::candidate_digest_next(&digest, owner, *loss)
        })
    }

    /// Twenty participants; losers at 1, 3, 4, 8, 12, 15 and 19, of which 3, 12
    /// and 19 have a Series A history.
    fn twenty() -> Vec<Part> {
        let mut parts = vec![WINNER; 20];
        for (slot, loss) in [(1usize, 100u128), (4, 400), (8, 800), (15, 150)] {
            parts[slot] = loser(loss);
        }
        for (slot, loss) in [(3usize, 300u128), (12, 120), (19, 190)] {
            parts[slot] = loser_with_a(loss);
        }
        parts
    }

    #[test]
    fn build_candidates_visits_every_participant_once_and_seals_chained_digests() {
        let mut world = World::new(&twenty(), 1, 1);
        assert!(!world.build(8).unwrap());
        assert!(!world.build(8).unwrap());
        assert!(world.build(4).unwrap(), "the third call reaches the end and seals");
        let close = &world.close;
        assert_eq!(close.candidate_cursor, 20);
        assert!(close.sealed);
        assert_eq!((close.preferred_count, close.fallback_count), (4, 7));
        assert_eq!(close.selected_set_tag, SET_PREFERRED);

        let o = &world.owners;
        let preferred = [(o[1], 100u128), (o[4], 400), (o[8], 800), (o[15], 150)];
        let fallback = [(o[1], 100u128), (o[3], 300), (o[4], 400), (o[8], 800), (o[12], 120), (o[15], 150), (o[19], 190)];
        assert_eq!(close.preferred_digest, chain(SET_PREFERRED, &preferred));
        assert_eq!(close.fallback_digest, chain(SET_FALLBACK, &fallback));

        let page = world.page(SET_PREFERRED, 0);
        assert_eq!(page.len, 4);
        assert_eq!(&page.rows[..4], &preferred.map(|(owner, _)| owner));
        let page = world.page(SET_FALLBACK, 0);
        assert_eq!(page.len, 7);
        assert_eq!(&page.rows[..7], &fallback.map(|(owner, _)| owner));
        assert_eq!(selected_set(close).1, 4);
    }

    #[test]
    fn chunking_does_not_change_the_sealed_sets() {
        let mut whole = World::new(&twenty(), 1, 1);
        for rows in [8u8, 8, 4] {
            whole.build(rows).unwrap();
        }
        let mut singles = World::new(&twenty(), 1, 1);
        for _ in 0..20 {
            singles.build(1).unwrap();
        }
        assert!(singles.close.sealed);
        assert_eq!(whole.close.preferred_digest, singles.close.preferred_digest);
        assert_eq!(whole.close.fallback_digest, singles.close.fallback_digest);
        assert_eq!(whole.close.preferred_count, singles.close.preferred_count);
    }

    #[test]
    fn with_no_preferred_candidate_the_fallback_set_is_selected() {
        let mut parts = vec![WINNER; 6];
        parts[2] = loser_with_a(50);
        parts[4] = Part { ever_b: true, ..loser(70) };
        let mut world = World::new(&parts, 1, 1);
        assert!(world.build(6).unwrap());
        assert_eq!((world.close.preferred_count, world.close.fallback_count), (0, 2));
        assert_eq!(world.close.selected_set_tag, SET_FALLBACK);
        assert_eq!(world.close.preferred_digest, sampling::candidate_digest_start(SET_PREFERRED), "empty set has the empty digest");
    }

    #[test]
    fn zero_losers_seal_an_empty_selection_without_needing_candidate_pages() {
        let mut world = World::new(&[WINNER; 10], 0, 0);
        assert!(!world.build(8).unwrap());
        assert!(world.build(2).unwrap());
        assert!(world.close.sealed);
        let (tag, count, digest) = selected_set(&world.close);
        assert_eq!((tag, count), (SET_FALLBACK, 0));
        assert_eq!(digest, sampling::candidate_digest_start(SET_FALLBACK));
    }

    #[test]
    fn a_rows_page_must_be_provisioned_before_it_is_appended_to() {
        let mut world = World::new(&[loser(10); 3], 0, 0);
        assert_code(world.build(1), Code::PageNotProvisioned);
        assert_eq!(world.close.candidate_cursor, 0, "a refused batch advances nothing");
        assert_eq!((world.close.preferred_count, world.close.fallback_count), (0, 0));
        // Only the preferred page exists: the loser also appends to the fallback set.
        let mut world = World::new(&[loser(10); 3], 1, 0);
        assert_code(world.build(1), Code::PageNotProvisioned);
        assert_eq!(world.page(SET_PREFERRED, 0).len, 0, "nothing was written");
    }

    #[test]
    fn a_batch_that_would_cross_into_an_unsupplied_page_fails_and_fewer_rows_succeed() {
        let mut world = World::new(&[loser(10); 12], 2, 2);
        world.build(3).unwrap();
        // Counts are 3: eight rows would append number 8 onto page 1, which was not supplied.
        assert_code(world.build(8), Code::PageNotProvisioned);
        assert_eq!(world.close.candidate_cursor, 3);
        // Five rows fill page 0 exactly, then the next call supplies page 1.
        world.build(5).unwrap();
        assert_eq!(world.page(SET_FALLBACK, 0).len, 8);
        world.build(4).unwrap();
        assert!(world.close.sealed);
        assert_eq!(world.page(SET_FALLBACK, 1).len, 4);
        assert_eq!((world.close.preferred_count, world.close.fallback_count), (12, 12));
    }

    #[test]
    fn skipped_or_reordered_rows_are_rejected() {
        let world = World::new(&twenty(), 1, 1);
        let preferred = world.append_page(SET_PREFERRED, 0);
        let fallback = world.append_page(SET_FALLBACK, 0);
        let (preferred, fallback) = (preferred.as_ref(), fallback.as_ref());
        let close = |w: &World| w.close.clone();

        // Starting at slot 1 when the cursor is 0 (a skipped row).
        let mut c = close(&world);
        assert_code(build_batch(&mut c, preferred, fallback, &world.rows(1, 2), 2), Code::WrongAccount);
        // Swapping two rows' UserRounds (a reordered row).
        let mut rows = world.rows(0, 2);
        rows.swap(1, 4);
        let mut c = close(&world);
        assert_code(build_batch(&mut c, preferred, fallback, &rows, 2), Code::WrongAccount);
        // Another owner's User for a row.
        let mut rows = world.rows(0, 2);
        rows[2] = world.users[5].clone();
        let mut c = close(&world);
        assert_code(build_batch(&mut c, preferred, fallback, &rows, 2), Code::WrongAccount);
        // The wrong participant page for the slot.
        let mut rows = world.rows(0, 2);
        rows[0] = world.participants[1].clone();
        let mut c = close(&world);
        assert_code(build_batch(&mut c, preferred, fallback, &rows, 2), Code::WrongAccount);
        // A UserRound that lies about its participant slot.
        let lying = {
            let mut round = UserRound::try_deserialize(&mut &world.rounds[1].data.borrow()[..]).unwrap();
            round.participant_slot = 9;
            let mut rows = world.rows(0, 2);
            rows[4] = info(*world.rounds[1].key, crate::ID, account_bytes(&round, UserRound::SPACE));
            rows
        };
        let mut c = close(&world);
        assert_code(build_batch(&mut c, preferred, fallback, &lying, 2), Code::WrongAccount);
        // The unmodified rows still work from a fresh record.
        let mut c = close(&world);
        assert!(build_batch(&mut c, preferred, fallback, &world.rows(0, 2), 2).is_ok());
    }

    #[test]
    fn an_unused_set_needs_no_page_but_a_needed_one_must_be_supplied_and_belong_to_the_set() {
        // Nobody lost: no page is read, so none is required even though both are provisioned.
        let mut world = World::new(&[WINNER; 4], 1, 1);
        let accounts = world.rows(0, 4);
        assert!(build_batch(&mut world.close, None, None, &accounts, 4).unwrap());
        // A loser needs both pages: omitting a provisioned one is WrongAccount, not a silent skip.
        let world = World::new(&[loser(10); 2], 1, 1);
        let accounts = world.rows(0, 1);
        let (preferred, fallback) = (world.preferred[0].clone(), world.fallback[0].clone());
        let mut close = world.close.clone();
        assert_code(build_batch(&mut close, None, Some(&fallback), &accounts, 1), Code::WrongAccount);
        assert_code(build_batch(&mut close, Some(&preferred), None, &accounts, 1), Code::WrongAccount);
        // The two sets' pages are not interchangeable.
        assert_code(build_batch(&mut close, Some(&fallback), Some(&preferred), &accounts, 1), Code::WrongAccount);
        // A candidate page of another close is refused.
        let (other, bump) = Pubkey::find_program_address(
            &[CANDIDATES_SEED, &7u64.to_le_bytes(), &[SET_FALLBACK], &0u64.to_le_bytes()],
            &crate::ID,
        );
        let foreign = CandidatePage { bump, close_id: 7, set_tag: SET_FALLBACK, page: 0, ..CandidatePage::default() };
        let foreign = info(other, crate::ID, account_bytes(&foreign, CandidatePage::SPACE));
        assert_code(build_batch(&mut close, Some(&preferred), Some(&foreign), &accounts, 1), Code::WrongAccount);
        assert_eq!((close.candidate_cursor, close.fallback_count), (0, 0), "refused batches advance nothing");
        assert!(build_batch(&mut close, Some(&preferred), Some(&fallback), &accounts, 1).is_ok());
    }

    #[test]
    fn row_counts_and_cursor_bounds_are_enforced() {
        let mut world = World::new(&[WINNER; 5], 0, 0);
        let five = world.rows(0, 5);
        // Zero rows and more than a page's worth of rows are refused up front.
        assert_code(world.build(0), Code::InvalidState);
        assert_code(build_batch(&mut world.close, None, None, &five, 9), Code::InvalidState);
        // A wrong account count for the rows claimed.
        assert_code(build_batch(&mut world.close, None, None, &five[..6], 3), Code::InvalidState);
        // More rows than participants left, even with enough (padded) accounts.
        let mut six = five.clone();
        six.extend_from_slice(&five[12..15]);
        assert_code(build_batch(&mut world.close, None, None, &six, 6), Code::CursorMismatch);
        assert_eq!(world.close.candidate_cursor, 0);
        // Exactly the remaining participants is fine.
        assert!(world.build(5).unwrap());
        // A run past the end after a partial first call is a cursor mismatch too.
        let mut world = World::new(&[WINNER; 12], 0, 0);
        world.build(8).unwrap();
        let tail = world.rows(4, 8);
        assert_code(build_batch(&mut world.close, None, None, &tail, 8), Code::CursorMismatch);
    }

    #[test]
    fn same_owner_positions_aggregate_into_one_loss_and_one_candidacy() {
        // One owner with three positions already aggregated into one UserRound:
        // deposits 3 x 1,000, payouts 1,200 + 0 + 400 => one loss of 1,400.
        let part = Part { deposits: 3_000, net: 1_600, ever_a: false, ever_b: false };
        let mut world = World::new(&[part, WINNER], 1, 1);
        assert!(world.build(2).unwrap());
        assert_eq!(world.close.fallback_count, 1);
        assert_eq!(world.close.preferred_count, 1);
        assert_eq!(world.close.fallback_digest, chain(SET_FALLBACK, &[(world.owners[0], 1_400)]));
        assert_eq!(world.page(SET_PREFERRED, 0).len, 1);
    }

    #[test]
    fn building_a_sealed_set_again_is_a_cursor_error() {
        let mut world = World::new(&[WINNER; 3], 0, 0);
        assert!(world.build(3).unwrap());
        // `candidate_cursor == participant_count`: no row remains to visit.
        let accounts = world.rows(0, 1);
        assert_code(build_batch(&mut world.close, None, None, &accounts, 1), Code::CursorMismatch);
    }

    // ---- promotion application (existing accounts only: no Rent sysvar on the host) ----

    struct Promo {
        world: World,
        pool: Pool,
        ops: Ops,
        config: ConfigVersion,
        slot: usize,
        ordinal: u64,
    }

    fn backer_pda(owner: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[BACKER_SEED, &[SERIES_B], owner.as_ref()], &crate::ID)
    }

    fn backer_account(owner: &Pubkey, backer: Backer) -> AccountInfo<'static> {
        let (address, bump) = backer_pda(owner);
        let backer = Backer { bump, owner: *owner, series: SERIES_B, ..backer };
        info(address, crate::ID, account_bytes(&backer, Backer::SPACE))
    }

    fn registry_account(index: u64, page: RegistryPage) -> AccountInfo<'static> {
        let (address, bump) = Pubkey::find_program_address(&[REGISTRY_SEED, &index.to_le_bytes()], &crate::ID);
        let page = RegistryPage { bump, index, ..page };
        info(address, crate::ID, account_bytes(&page, RegistryPage::SPACE))
    }

    fn decode<T: AccountDeserialize>(info: &AccountInfo) -> T {
        let data = info.try_borrow_data().unwrap();
        let mut bytes: &[u8] = &data;
        T::try_deserialize(&mut bytes).unwrap()
    }

    /// Six participants; the one at slot 3 lost 300,000,000 and is the sole
    /// preferred candidate, so any draw selects it.
    fn promo(registry_len: u64) -> Promo {
        let mut parts = vec![WINNER; 6];
        parts[3] = Part { deposits: 400_000_000, net: 100_000_000, ever_a: false, ever_b: false };
        let mut world = World::new(&parts, 1, 1);
        assert!(world.build(6).unwrap());
        world.close.config_version = 1;
        world.close.owner_refund_destination = key(0xD0);
        let pool = Pool {
            round_id: ROUND,
            registry_len,
            // The tail page at `registry_len / 8` exists in every promotion test.
            registry_pages: registry_len / PAGE + 1,
            next_sequence: 9,
            current_close_id: Some(CLOSE),
            ..Pool::default()
        };
        Promo {
            world,
            pool,
            ops: Ops::default(),
            config: ConfigVersion::default_v1(),
            slot: 3,
            ordinal: 0,
        }
    }

    fn apply(p: &mut Promo, backer: &AccountInfo<'static>, registry: &AccountInfo<'static>) -> Result<Winner> {
        let winner = winner_info(p);
        apply_as(p, &winner, backer, registry)
    }

    fn apply_as(
        p: &mut Promo,
        winner: &AccountInfo<'static>,
        backer: &AccountInfo<'static>,
        registry: &AccountInfo<'static>,
    ) -> Result<Winner> {
        let placeholder = World::placeholder();
        let candidate = p.world.preferred[(p.ordinal / PAGE) as usize].clone();
        apply_winner(
            &mut p.pool,
            &mut p.world.close,
            &mut p.ops,
            &p.config,
            p.ordinal,
            &placeholder,
            &placeholder,
            &candidate,
            winner,
            &p.world.users[p.slot],
            &p.world.rounds[p.slot],
            backer,
            registry,
        )
    }

    fn winner_info(p: &Promo) -> AccountInfo<'static> {
        info(p.world.owners[p.slot], system_program::ID, Vec::new())
    }

    #[test]
    fn apply_promotion_grants_116_percent_to_a_reopened_backer_and_appends_it_to_the_registry() {
        let mut p = promo(3);
        assert_eq!(selected_set(&p.world.close), (SET_PREFERRED, 1, p.world.close.preferred_digest));
        let owner = p.world.owners[p.slot];
        let backer = backer_account(&owner, Backer::default());
        let tail = registry_account(0, RegistryPage::default());
        let winner = apply(&mut p, &backer, &tail).unwrap();
        assert_eq!((winner.key, winner.loss, winner.entitlement), (owner, 300_000_000, 348_000_000));

        let b: Backer = decode(&backer);
        assert_eq!(b.entitlement, 348_000_000);
        assert_eq!(b.amount, 0, "B carries no deposit amount");
        assert!(b.active);
        assert_eq!(b.slot, 3, "appended at the old registry_len");
        assert_eq!(b.first_sequence, 9);
        assert_eq!(p.pool.next_sequence, 10);
        assert_eq!(p.pool.registry_len, 4);
        let page: RegistryPage = decode(&tail);
        assert_eq!(page.keys[3], backer.key());
        let user: User = decode(&p.world.users[p.slot]);
        assert!(user.ever_b);
        assert_eq!(user.b_entitlement, 348_000_000);
        // No game cash moves and nothing is spent: the Backer already existed.
        assert_eq!((p.pool.pot, p.pool.seed, p.pool.management, p.pool.funded_claims), (0, 0, 0, 0));
        assert_eq!(p.world.close.spent.b_rent_s + p.world.close.spent.registry_rent_s, 0);
    }

    #[test]
    fn a_winner_who_is_already_an_open_backer_only_merges_entitlement() {
        let mut p = promo(5);
        let owner = p.world.owners[p.slot];
        let existing = Backer { entitlement: 1_000, lifetime: 100, first_sequence: 2, active: true, slot: 4, ..Backer::default() };
        let backer = backer_account(&owner, existing);
        let tail = registry_account(0, RegistryPage::default());
        apply(&mut p, &backer, &tail).unwrap();
        let b: Backer = decode(&backer);
        assert_eq!(b.entitlement, 1_000 + 348_000_000);
        assert_eq!((b.first_sequence, b.slot), (2, 4), "sequence and slot are kept");
        assert_eq!(p.pool.next_sequence, 9, "no new sequence is issued");
        assert_eq!(p.pool.registry_len, 5, "an active row is not appended twice");
        assert_eq!(decode::<RegistryPage>(&tail).keys, [Pubkey::default(); PAGE_CAPACITY]);
    }

    #[test]
    fn a_capped_backer_is_reopened_and_a_tiny_grant_stays_closed() {
        // Capped: entitlement == lifetime, inactive; the grant reopens it.
        let mut p = promo(0);
        let owner = p.world.owners[p.slot];
        let capped = Backer { entitlement: 500, lifetime: 500, first_sequence: 1, ..Backer::default() };
        let backer = backer_account(&owner, capped);
        let tail = registry_account(0, RegistryPage::default());
        apply(&mut p, &backer, &tail).unwrap();
        let b: Backer = decode(&backer);
        assert!(b.active && b.slot == 0);
        assert_eq!(b.first_sequence, 1);
        assert_eq!(p.pool.registry_len, 1);

        // A one-lamport-scale grant leaves remaining <= 10: merged, not appended.
        let mut p = promo(0);
        p.world.rounds[p.slot] = {
            let (address, bump) = Pubkey::find_program_address(
                &[USER_ROUND_SEED, &ROUND.to_le_bytes(), p.world.owners[p.slot].as_ref()],
                &crate::ID,
            );
            let round = UserRound { bump, round_id: ROUND, owner, deposits: 20, net_payout: 12, participant_slot: 3 };
            info(address, crate::ID, account_bytes(&round, UserRound::SPACE))
        };
        let backer = backer_account(&owner, Backer::default());
        let tail = registry_account(0, RegistryPage::default());
        let winner = apply(&mut p, &backer, &tail).unwrap();
        assert_eq!((winner.loss, winner.entitlement), (8, 9), "floor(8 * 1.16)");
        let b: Backer = decode(&backer);
        assert_eq!(b.entitlement, 9);
        assert!(!b.active);
        assert_eq!(p.pool.registry_len, 0);
    }

    #[test]
    fn the_winner_must_be_the_candidate_at_the_drawn_ordinal_with_their_own_accounts() {
        let owner;
        {
            let p = promo(0);
            owner = p.world.owners[p.slot];
        }
        // Naming someone else as the winner.
        let mut p = promo(0);
        let backer = backer_account(&owner, Backer::default());
        let tail = registry_account(0, RegistryPage::default());
        let impostor = info(key(0x99), system_program::ID, Vec::new());
        assert_code(apply_as(&mut p, &impostor, &backer, &tail), Code::WrongAccount);
        // Someone else's UserRound.
        let mut p = promo(0);
        p.world.rounds[3] = p.world.rounds[0].clone();
        assert_code(apply(&mut p, &backer, &tail), Code::WrongAccount);
        // Someone else's User.
        let mut p = promo(0);
        p.world.users[3] = p.world.users[0].clone();
        assert_code(apply(&mut p, &backer, &tail), Code::WrongAccount);
        // A Backer account that belongs to another owner.
        let mut p = promo(0);
        let other = backer_account(&p.world.owners[0], Backer::default());
        assert_code(apply(&mut p, &other, &tail), Code::WrongAccount);
        // A drawn ordinal outside the sealed set.
        let mut p = promo(0);
        p.ordinal = 1;
        assert_code(apply(&mut p, &backer, &tail), Code::InvalidState);
    }

    #[test]
    fn a_registry_tail_that_is_not_the_next_page_is_refused() {
        let mut p = promo(8);
        let owner = p.world.owners[p.slot];
        let backer = backer_account(&owner, Backer::default());
        // registry_len 8 needs page 1; page 0 is full and must not be reused.
        let wrong = registry_account(0, RegistryPage::default());
        p.pool.registry_pages = 2;
        assert_code(apply(&mut p, &backer, &wrong), Code::PageNotProvisioned);
        assert_eq!(p.pool.registry_len, 8);
    }
}
