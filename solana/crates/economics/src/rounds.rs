//! Host-side reference model for the native SOL round and promotion contract.
//!
//! This module intentionally models account/state invariants without an Anchor
//! instruction entrypoint, an SVM runtime, a provider CPI, signing, or funds.
//! In a deployed program the participant and candidate pages would be
//! program-owned accounts. Here a `BTreeMap` gives the same canonical ordering
//! and makes duplicate or skipped traversal impossible to hide.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub use crate::earnings::PlanKind;
use crate::{admission, backers, earnings, fees};

pub const PROMOTION_DEADLINE_SECS: u64 = 600;
const EEC_BPS: u128 = 11_600;
const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Series {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Active,
    AllocatingClosingToll,
    CountingCandidates,
    AwaitingRandomness,
    PromotionApplied,
    PromotionSkipped(SkipReason),
    ResetReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Deadline,
    NoCandidates,
    EntropyExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundError {
    AdmissionPaused,
    NotActive,
    PositionAlreadyExists,
    PositionNotFound,
    StalePosition,
    WrongOwner,
    PositionAlreadyClaimed,
    BackwardsClock,
    InvalidCloseState,
    ProviderBinding,
    ProviderCreateFailed,
    RequestCollision,
    RequestNotFound,
    TimelyCaptureAlreadyStored,
    CaptureAfterDeadline,
    WrongCaptureAuthority,
    NoCandidates,
    EntropyExhausted,
    NoCapturedOutput,
    PromotionAlreadyTerminal,
    ArithmeticOverflow,
    InvalidExpectedRound,
    Admission(admission::Error),
    Backer(backers::Error),
    BookStateMismatch,
    PendingToll,
    InvalidAllocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimError {
    Round(RoundError),
    NotMature,
    MinimumReceive,
    InvalidPayout,
    IneligibleClose,
    BackwardsClock,
    PendingToll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    TimerCloseForbidden,
    AlreadyClosing,
    NotClosing,
    CashNotCommitted,
    TimelyCaptureExists,
    BeforeDeadline,
    NoCapturedOutput,
    StaleCloseId,
    WrongPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureResult {
    Captured,
    IgnoredLate,
    AlreadyCaptured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionStatus {
    Applied { owner: u64, entitlement: u128 },
    Skipped(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeResult {
    Advanced { round_id: u64, seed_lamports: u64 },
    AlreadyFinalized { round_id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderTuple {
    pub provider_owner: [u8; 32],
    pub client: [u8; 32],
    pub config: [u8; 32],
}

impl ProviderTuple {
    pub const fn new(provider_owner: [u8; 32], client: [u8; 32], config: [u8; 32]) -> Self {
        Self {
            provider_owner,
            client,
            config,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestBinding {
    pub provider_owner: [u8; 32],
    pub client: [u8; 32],
    pub config: [u8; 32],
    pub request_address: [u8; 32],
    pub seed: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapturedDraw {
    binding: RequestBinding,
    output: [u8; 64],
    captured_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    owner: u64,
    round_id: u64,
    plan: earnings::Plan,
    simple_accrual: Option<earnings::SimpleAccrual>,
    gross_deposit: u128,
    net_payout: u128,
    paid_earnings: u128,
    claimed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct UserRound {
    gross_deposit: u128,
    net_payout: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Participant {
    pub owner: u64,
    pub gross_deposit: u128,
    pub net_payout: u128,
    pub loss: u128,
    pub has_ever_a: bool,
    pub has_ever_b: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidates {
    pub preferred: Vec<(u64, u128)>,
    pub fallback: Vec<(u64, u128)>,
    preferred_digest: [u8; 32],
    fallback_digest: [u8; 32],
}

impl Candidates {
    pub fn selected_tag(&self) -> u8 {
        if self.preferred.is_empty() {
            1
        } else {
            0
        }
    }
    pub fn selected(&self) -> &[(u64, u128)] {
        if self.preferred.is_empty() {
            &self.fallback
        } else {
            &self.preferred
        }
    }
    fn selected_digest(&self) -> [u8; 32] {
        if self.preferred.is_empty() {
            self.fallback_digest
        } else {
            self.preferred_digest
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CloseState {
    close_id: u64,
    close_started_at: u64,
    promotion_deadline: u64,
    cash_committed: bool,
    request: Option<RequestBinding>,
    captured: Option<CapturedDraw>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    round_id: u64,
    phase: Phase,
    cluster_domain: [u8; 32],
    native_program_id: [u8; 32],
    provider: ProviderTuple,
    admission_paused: bool,
    pot_lamports: u64,
    management_lamports: u64,
    pending_toll_lamports: u64,
    funded_lamports: u64,
    seed_lamports: u64,
    next_close_id: u64,
    close: Option<CloseState>,
    last_finalized_close: Option<(u64, u64, u64)>,
    positions: BTreeMap<u64, Position>,
    admissions: BTreeMap<u64, admission::Admission>,
    user_rounds: BTreeMap<u64, UserRound>,
    ever_backer: BTreeMap<u64, (bool, bool)>,
    candidates: Option<Candidates>,
    promotion_result: Option<PromotionStatus>,
    b_entitlements: BTreeMap<u64, u128>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payout {
    pub owed: u128,
    pub gross_paid: u64,
    pub net_payout: u64,
    pub toll: u64,
    pub close_started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPositionArgs {
    pub owner: u64,
    pub position_id: u64,
    pub gross_deposit: u64,
    pub plan_kind: PlanKind,
}

impl Round {
    pub fn new(
        round_id: u64,
        cluster_domain: [u8; 32],
        native_program_id: [u8; 32],
        provider: ProviderTuple,
    ) -> Self {
        Self {
            round_id,
            phase: Phase::Active,
            cluster_domain,
            native_program_id,
            provider,
            admission_paused: false,
            pot_lamports: 0,
            management_lamports: 0,
            pending_toll_lamports: 0,
            funded_lamports: 0,
            seed_lamports: 0,
            next_close_id: 1,
            close: None,
            last_finalized_close: None,
            positions: BTreeMap::new(),
            admissions: BTreeMap::new(),
            user_rounds: BTreeMap::new(),
            ever_backer: BTreeMap::new(),
            candidates: None,
            promotion_result: None,
            b_entitlements: BTreeMap::new(),
        }
    }

    pub fn round_id(&self) -> u64 {
        self.round_id
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn pot_lamports(&self) -> u64 {
        self.pot_lamports
    }
    pub fn seed_lamports(&self) -> u64 {
        self.seed_lamports
    }
    pub fn management_lamports(&self) -> u64 {
        self.management_lamports
    }
    pub fn pending_toll_lamports(&self) -> u64 {
        self.pending_toll_lamports
    }
    pub fn funded_lamports(&self) -> u64 {
        self.funded_lamports
    }
    pub fn set_admission_paused(&mut self, paused: bool) {
        self.admission_paused = paused;
    }
    pub fn b_entitlement(&self, owner: u64) -> Option<u128> {
        self.b_entitlements.get(&owner).copied()
    }
    pub fn promotion_deadline(&self) -> Option<u64> {
        self.close.map(|c| c.promotion_deadline)
    }
    pub fn close_started_at(&self) -> Option<u64> {
        self.close.map(|c| c.close_started_at)
    }
    pub fn captured_output(&self) -> Option<[u8; 64]> {
        self.close.and_then(|c| c.captured.map(|d| d.output))
    }
    pub fn captured_at(&self) -> Option<u64> {
        self.close.and_then(|c| c.captured.map(|d| d.captured_at))
    }
    pub fn request_binding(&self) -> Option<&RequestBinding> {
        self.close.as_ref().and_then(|c| c.request.as_ref())
    }

    pub fn open_position(
        &mut self,
        owner: u64,
        position_id: u64,
        gross_deposit: u64,
        plan_kind: PlanKind,
        opened_at: u64,
    ) -> Result<(), RoundError> {
        if self.phase != Phase::Active {
            return Err(RoundError::NotActive);
        }
        if self.admission_paused {
            return Err(RoundError::AdmissionPaused);
        }
        if self.positions.contains_key(&position_id) {
            return Err(RoundError::PositionAlreadyExists);
        }
        let mut admission = self.admissions.get(&owner).copied().unwrap_or_default();
        admission
            .open_position(opened_at, gross_deposit)
            .map_err(RoundError::Admission)?;
        let plan = match plan_kind {
            PlanKind::Simple => earnings::Plan::simple(gross_deposit, opened_at),
            PlanKind::Compound15 => earnings::Plan::compound_15(gross_deposit, opened_at),
            PlanKind::Compound30 => earnings::Plan::compound_30(gross_deposit, opened_at),
        }
        .map_err(|_| RoundError::ArithmeticOverflow)?;
        let simple_accrual = if plan_kind == PlanKind::Simple {
            Some(
                earnings::SimpleAccrual::new(gross_deposit, opened_at)
                    .map_err(|_| RoundError::ArithmeticOverflow)?,
            )
        } else {
            None
        };
        let entry =
            fees::front_end_load(gross_deposit).map_err(|_| RoundError::ArithmeticOverflow)?;
        let next_pot = self
            .pot_lamports
            .checked_add(entry.pot_credit)
            .ok_or(RoundError::ArithmeticOverflow)?;
        let next_management = self
            .management_lamports
            .checked_add(entry.entry_fee)
            .ok_or(RoundError::ArithmeticOverflow)?;
        let user = self.user_rounds.get(&owner).copied().unwrap_or_default();
        let next_gross = user
            .gross_deposit
            .checked_add(u128::from(gross_deposit))
            .ok_or(RoundError::ArithmeticOverflow)?;
        fees::prospective_series_b_entitlement(
            next_gross,
            user.net_payout,
            self.b_entitlements.get(&owner).copied().unwrap_or(0),
        )
        .map_err(|_| RoundError::ArithmeticOverflow)?;
        self.admissions.insert(owner, admission);
        self.pot_lamports = next_pot;
        self.management_lamports = next_management;
        self.user_rounds.insert(
            owner,
            UserRound {
                gross_deposit: next_gross,
                ..user
            },
        );
        self.positions.insert(
            position_id,
            Position {
                owner,
                round_id: self.round_id,
                plan,
                simple_accrual,
                gross_deposit: u128::from(gross_deposit),
                net_payout: 0,
                paid_earnings: 0,
                claimed: false,
            },
        );
        Ok(())
    }

    pub fn claim_position(
        &mut self,
        owner: u64,
        position_id: u64,
        minimum_receive: u64,
        now: u64,
    ) -> Result<Payout, ClaimError> {
        let mut next = self.clone();
        let payout = next.claim_position_inner(owner, position_id, minimum_receive, now, false)?;
        *self = next;
        Ok(payout)
    }

    pub fn close_position(
        &mut self,
        owner: u64,
        position_id: u64,
        minimum_receive: u64,
        now: u64,
    ) -> Result<Payout, ClaimError> {
        let mut next = self.clone();
        let payout = next.claim_position_inner(owner, position_id, minimum_receive, now, true)?;
        *self = next;
        Ok(payout)
    }

    fn claim_position_inner(
        &mut self,
        owner: u64,
        position_id: u64,
        minimum_receive: u64,
        now: u64,
        explicit_close: bool,
    ) -> Result<Payout, ClaimError> {
        if self.pending_toll_lamports != 0 {
            return Err(ClaimError::PendingToll);
        }
        if self.phase != Phase::Active {
            return Err(ClaimError::Round(RoundError::NotActive));
        }
        let mut position = *self
            .positions
            .get(&position_id)
            .ok_or(ClaimError::Round(RoundError::PositionNotFound))?;
        if position.owner != owner {
            return Err(ClaimError::Round(RoundError::WrongOwner));
        }
        if position.round_id != self.round_id {
            return Err(ClaimError::Round(RoundError::StalePosition));
        }
        if position.claimed {
            return Err(ClaimError::Round(RoundError::PositionAlreadyClaimed));
        }
        if explicit_close && position.plan.kind != PlanKind::Simple {
            return Err(ClaimError::IneligibleClose);
        }
        let (owed, toll_bps, matured) = match position.plan.kind {
            PlanKind::Simple => {
                let accrual = position
                    .simple_accrual
                    .as_mut()
                    .ok_or(ClaimError::InvalidPayout)?;
                accrual.accrue(now).map_err(map_earnings_error)?;
                let owed = accrual
                    .accrued_gross
                    .checked_sub(position.paid_earnings)
                    .ok_or(ClaimError::InvalidPayout)?;
                let elapsed = now
                    .checked_sub(position.plan.opened_at)
                    .ok_or(ClaimError::BackwardsClock)?;
                (
                    owed,
                    fees::simple_toll_bps(elapsed),
                    now >= position.plan.maturity_at,
                )
            }
            PlanKind::Compound15 | PlanKind::Compound30 => {
                if now < position.plan.maturity_at {
                    return Err(ClaimError::NotMature);
                }
                let owed = position.plan.owed_at(now).map_err(map_earnings_error)?;
                let toll_bps = if position.plan.kind == PlanKind::Compound15 {
                    fees::compound_jackpot_fee_bps(15)
                } else {
                    fees::compound_jackpot_fee_bps(30)
                }
                .map_err(|_| ClaimError::InvalidPayout)?;
                (owed, toll_bps, true)
            }
        };
        let payout =
            earnings::Payout::from_gross(owed, self.pot_lamports, toll_bps, minimum_receive)
                .map_err(map_earnings_error)?;
        let closes = payout.insolvent;
        if explicit_close && !matured {
            return Err(ClaimError::NotMature);
        }
        if closes {
            self.start_close(now).map_err(ClaimError::Round)?;
        }
        self.pot_lamports -= payout.paid_gross;
        self.pending_toll_lamports = self
            .pending_toll_lamports
            .checked_add(payout.toll)
            .ok_or(ClaimError::Round(RoundError::ArithmeticOverflow))?;
        position.paid_earnings = position
            .paid_earnings
            .checked_add(u128::from(payout.paid_gross))
            .ok_or(ClaimError::Round(RoundError::ArithmeticOverflow))?;
        position.net_payout = position
            .net_payout
            .checked_add(u128::from(payout.net))
            .ok_or(ClaimError::Round(RoundError::ArithmeticOverflow))?;
        position.claimed = !matches!(position.plan.kind, PlanKind::Simple) || matured;
        self.positions.insert(position_id, position);
        let user = self.user_rounds.get(&owner).copied().unwrap_or_default();
        let net_payout = user
            .net_payout
            .checked_add(u128::from(payout.net))
            .ok_or(ClaimError::Round(RoundError::ArithmeticOverflow))?;
        self.user_rounds
            .insert(owner, UserRound { net_payout, ..user });
        Ok(Payout {
            owed,
            gross_paid: payout.paid_gross,
            net_payout: payout.net,
            close_started: closes,
            toll: payout.toll,
        })
    }

    pub fn commit_allocation(
        &mut self,
        seed_credit: u64,
        funded_credit: u64,
    ) -> Result<(), RoundError> {
        let total = seed_credit
            .checked_add(funded_credit)
            .ok_or(RoundError::ArithmeticOverflow)?;
        if total != self.pending_toll_lamports {
            return Err(RoundError::InvalidAllocation);
        }
        let next_seed = self
            .seed_lamports
            .checked_add(seed_credit)
            .ok_or(RoundError::ArithmeticOverflow)?;
        let next_funded = self
            .funded_lamports
            .checked_add(funded_credit)
            .ok_or(RoundError::ArithmeticOverflow)?;
        let marks_cash_committed = self.phase == Phase::AllocatingClosingToll
            || self.phase == Phase::PromotionSkipped(SkipReason::Deadline);
        if marks_cash_committed && self.close.is_none() {
            return Err(RoundError::InvalidCloseState);
        }
        self.seed_lamports = next_seed;
        self.funded_lamports = next_funded;
        self.pending_toll_lamports = 0;
        if marks_cash_committed {
            let close = self.close.as_mut().ok_or(RoundError::InvalidCloseState)?;
            close.cash_committed = true;
        }
        if self.phase == Phase::AllocatingClosingToll {
            self.phase = Phase::CountingCandidates;
        }
        Ok(())
    }

    /// Fund Series A through the same owner admission ledger as game opens.
    ///
    /// The Round and Book are one host-model cash view for this operation: the
    /// caller must supply matching pot, pending-toll, and management partitions.
    /// Series A cash has no front-end load and does not become game P/L.
    pub fn fund_series_a(
        &mut self,
        book: &mut backers::Book,
        owner: u64,
        cash: u64,
        execution_now: u64,
    ) -> Result<(), RoundError> {
        if self.phase != Phase::Active {
            return Err(RoundError::NotActive);
        }
        if self.admission_paused {
            return Err(RoundError::AdmissionPaused);
        }
        if self.pot_lamports != book.pot
            || self.pending_toll_lamports != book.pending_toll
            || self.management_lamports != book.management
        {
            return Err(RoundError::BookStateMismatch);
        }
        if book.ticket().is_some() {
            return Err(RoundError::Backer(backers::Error::AllocationBarrier));
        }
        if self.pending_toll_lamports != 0 {
            return Err(RoundError::PendingToll);
        }

        let mut next_round = self.clone();
        let mut next_book = book.clone();
        let mut admission = next_round
            .admissions
            .get(&owner)
            .copied()
            .unwrap_or_default();
        admission
            .fund_series_a(execution_now, cash)
            .map_err(RoundError::Admission)?;
        let next_pot = next_round
            .pot_lamports
            .checked_add(cash)
            .ok_or(RoundError::ArithmeticOverflow)?;
        next_round.admissions.insert(owner, admission);
        next_round
            .ever_backer
            .entry(owner)
            .or_insert((false, false))
            .0 = true;
        next_book
            .fund_series_a_unchecked(owner, cash)
            .map_err(RoundError::Backer)?;
        if next_book.pot != next_pot {
            return Err(RoundError::BookStateMismatch);
        }
        next_round.pot_lamports = next_pot;
        *self = next_round;
        *book = next_book;
        Ok(())
    }

    /// Fixture-only host setup for pre-existing Backer history.
    ///
    /// Production-shaped Series A funding must use `fund_series_a`, which
    /// consumes the shared admission ledger and updates the Book atomically.
    pub fn record_backer_exists_fixture(
        &mut self,
        owner: u64,
        series: Series,
    ) -> Result<(), RoundError> {
        if self.phase != Phase::Active {
            return Err(RoundError::NotActive);
        }
        if self.admission_paused {
            return Err(RoundError::AdmissionPaused);
        }
        let entry = self.ever_backer.entry(owner).or_insert((false, false));
        match series {
            Series::A => entry.0 = true,
            Series::B => entry.1 = true,
        }
        Ok(())
    }

    pub fn participant_count(&self) -> usize {
        self.user_rounds.len()
    }
    pub fn participant_chunk(
        &self,
        start: usize,
        limit: usize,
    ) -> Result<Vec<Participant>, RoundError> {
        if limit == 0 || start >= self.user_rounds.len() {
            return Err(RoundError::InvalidCloseState);
        }
        let rows = self.participant_rows();
        let end = start
            .checked_add(limit)
            .unwrap_or(rows.len())
            .min(rows.len());
        Ok(rows[start..end].to_vec())
    }
    pub fn participant_digest(&self) -> [u8; 32] {
        digest_participants(&self.participant_rows())
    }

    fn participant_rows(&self) -> Vec<Participant> {
        self.user_rounds
            .iter()
            .map(|(&owner, user)| {
                let loss = user.gross_deposit.saturating_sub(user.net_payout);
                let (has_ever_a, has_ever_b) = self
                    .ever_backer
                    .get(&owner)
                    .copied()
                    .unwrap_or((false, false));
                Participant {
                    owner,
                    gross_deposit: user.gross_deposit,
                    net_payout: user.net_payout,
                    loss,
                    has_ever_a,
                    has_ever_b,
                }
            })
            .collect()
    }

    fn start_close(&mut self, close_started_at: u64) -> Result<u64, RoundError> {
        if self.phase != Phase::Active {
            return Err(RoundError::InvalidCloseState);
        }
        let deadline = close_started_at
            .checked_add(PROMOTION_DEADLINE_SECS)
            .ok_or(RoundError::ArithmeticOverflow)?;
        let close_id = self.next_close_id;
        self.next_close_id = self
            .next_close_id
            .checked_add(1)
            .ok_or(RoundError::ArithmeticOverflow)?;
        self.close = Some(CloseState {
            close_id,
            close_started_at,
            promotion_deadline: deadline,
            cash_committed: false,
            request: None,
            captured: None,
        });
        self.phase = Phase::AllocatingClosingToll;
        self.candidates = None;
        self.promotion_result = None;
        Ok(close_id)
    }

    pub fn seal_candidates(&mut self) -> Result<Candidates, RoundError> {
        if self.phase != Phase::CountingCandidates {
            return Err(RoundError::InvalidCloseState);
        }
        let rows = self.participant_rows();
        let mut preferred = Vec::new();
        let mut fallback = Vec::new();
        for row in rows {
            if row.loss == 0 {
                continue;
            }
            fallback.push((row.owner, row.loss));
            if !row.has_ever_a && !row.has_ever_b {
                preferred.push((row.owner, row.loss));
            }
        }
        let candidates = Candidates {
            preferred_digest: digest_candidates(&preferred),
            fallback_digest: digest_candidates(&fallback),
            preferred,
            fallback,
        };
        self.candidates = Some(candidates.clone());
        if candidates.selected().is_empty() {
            self.phase = Phase::PromotionSkipped(SkipReason::NoCandidates);
            self.promotion_result = Some(PromotionStatus::Skipped(SkipReason::NoCandidates));
        } else {
            self.phase = Phase::AwaitingRandomness;
        }
        Ok(candidates)
    }

    pub fn protocol_seed(&self) -> Result<[u8; 32], RoundError> {
        let close = self.close.as_ref().ok_or(RoundError::InvalidCloseState)?;
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(RoundError::InvalidCloseState)?;
        let selected = candidates.selected();
        if selected.is_empty() {
            return Err(RoundError::NoCandidates);
        }
        let mut hasher = Sha256::new();
        hasher.update(b"SchemeWorks/MC/EEC/v1");
        hasher.update(self.cluster_domain);
        hasher.update(self.native_program_id);
        hasher.update(self.round_id.to_le_bytes());
        hasher.update(close.close_id.to_le_bytes());
        hasher.update([candidates.selected_tag()]);
        hasher.update((selected.len() as u64).to_le_bytes());
        hasher.update(candidates.selected_digest());
        Ok(hasher.finalize().into())
    }

    pub fn request_draw(
        &mut self,
        tuple: ProviderTuple,
        existing_request: bool,
        provider_created: bool,
        now: u64,
    ) -> Result<RequestBinding, RoundError> {
        let mut next = self.clone();
        let binding = next.request_draw_inner(tuple, existing_request, provider_created, now)?;
        *self = next;
        Ok(binding)
    }
    fn request_draw_inner(
        &mut self,
        tuple: ProviderTuple,
        existing_request: bool,
        provider_created: bool,
        now: u64,
    ) -> Result<RequestBinding, RoundError> {
        if self.phase != Phase::AwaitingRandomness {
            return Err(RoundError::InvalidCloseState);
        }
        let seed = self.protocol_seed()?;
        let close = self.close.as_mut().ok_or(RoundError::InvalidCloseState)?;
        if let Some(existing) = close.request {
            if existing.provider_owner == tuple.provider_owner
                && existing.client == tuple.client
                && existing.config == tuple.config
            {
                return Ok(existing);
            }
            return Err(RoundError::ProviderBinding);
        }
        if now < close.close_started_at {
            return Err(RoundError::BackwardsClock);
        }
        if now > close.promotion_deadline {
            return Err(RoundError::CaptureAfterDeadline);
        }
        if existing_request {
            return Err(RoundError::RequestCollision);
        }
        if tuple != self.provider {
            return Err(RoundError::ProviderBinding);
        }
        if !provider_created {
            return Err(RoundError::ProviderCreateFailed);
        }
        let binding = RequestBinding {
            provider_owner: tuple.provider_owner,
            client: tuple.client,
            config: tuple.config,
            request_address: request_address(tuple.client, seed),
            seed,
        };
        close.request = Some(binding);
        Ok(binding)
    }

    pub fn capture_draw(
        &mut self,
        binding: RequestBinding,
        now: u64,
        output: [u8; 64],
    ) -> Result<CaptureResult, RoundError> {
        let mut next = self.clone();
        let result = next.capture_draw_inner(binding, now, output)?;
        *self = next;
        Ok(result)
    }
    fn capture_draw_inner(
        &mut self,
        binding: RequestBinding,
        now: u64,
        output: [u8; 64],
    ) -> Result<CaptureResult, RoundError> {
        let phase = self.phase;
        let close = self.close.as_mut().ok_or(RoundError::RequestNotFound)?;
        if close.request != Some(binding) {
            return Err(RoundError::ProviderBinding);
        }
        if let Some(captured) = close.captured {
            if captured.binding == binding && captured.output == output {
                return Ok(CaptureResult::AlreadyCaptured);
            }
            return Err(RoundError::TimelyCaptureAlreadyStored);
        }
        if phase == Phase::PromotionSkipped(SkipReason::Deadline) {
            return Ok(CaptureResult::IgnoredLate);
        }
        if phase != Phase::AwaitingRandomness {
            return Err(RoundError::InvalidCloseState);
        }
        if now > close.promotion_deadline {
            return Ok(CaptureResult::IgnoredLate);
        }
        if now < close.close_started_at {
            return Err(RoundError::BackwardsClock);
        }
        close.captured = Some(CapturedDraw {
            binding,
            output,
            captured_at: now,
        });
        Ok(CaptureResult::Captured)
    }
    pub fn expire_promotion(&mut self, now: u64) -> Result<PromotionStatus, CloseError> {
        if let Some(close) = self.close {
            if close.captured.is_some() {
                return Err(CloseError::TimelyCaptureExists);
            }
            if now <= close.promotion_deadline {
                return Err(CloseError::BeforeDeadline);
            }
            if !matches!(
                self.phase,
                Phase::AllocatingClosingToll
                    | Phase::CountingCandidates
                    | Phase::AwaitingRandomness
            ) {
                if self.phase == Phase::PromotionSkipped(SkipReason::Deadline) {
                    return Ok(PromotionStatus::Skipped(SkipReason::Deadline));
                }
                return Err(CloseError::WrongPhase);
            }
            self.phase = Phase::PromotionSkipped(SkipReason::Deadline);
            self.candidates = None;
            let result = PromotionStatus::Skipped(SkipReason::Deadline);
            self.promotion_result = Some(result);
            return Ok(result);
        }
        Err(CloseError::NotClosing)
    }

    pub fn apply_promotion(&mut self) -> Result<PromotionStatus, RoundError> {
        if let Some(result) = self.promotion_result {
            return Ok(result);
        }
        let captured = self
            .close
            .and_then(|c| c.captured)
            .ok_or(RoundError::NoCapturedOutput)?;
        if self.phase != Phase::AwaitingRandomness {
            return Err(RoundError::PromotionAlreadyTerminal);
        }
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(RoundError::InvalidCloseState)?;
        let selected = candidates.selected();
        let Some((ordinal, _counter)) = map_output(captured.output, self.round_id, selected.len())
        else {
            self.phase = Phase::PromotionSkipped(SkipReason::EntropyExhausted);
            let result = PromotionStatus::Skipped(SkipReason::EntropyExhausted);
            self.promotion_result = Some(result);
            return Ok(result);
        };
        let (owner, loss) = selected[ordinal];
        let entitlement = loss
            .checked_mul(EEC_BPS)
            .ok_or(RoundError::ArithmeticOverflow)?
            / BPS_DENOMINATOR;
        let previous = self.b_entitlements.get(&owner).copied().unwrap_or(0);
        let next = previous
            .checked_add(entitlement)
            .ok_or(RoundError::ArithmeticOverflow)?;
        self.b_entitlements.insert(owner, next);
        self.ever_backer.entry(owner).or_insert((false, false)).1 = true;
        self.phase = Phase::PromotionApplied;
        let result = PromotionStatus::Applied { owner, entitlement };
        self.promotion_result = Some(result);
        Ok(result)
    }

    pub fn finalize_round(&mut self, close_id: u64) -> Result<FinalizeResult, CloseError> {
        if let Some((finalized_close, next_round, _seed)) = self.last_finalized_close {
            if finalized_close == close_id {
                return Ok(FinalizeResult::AlreadyFinalized {
                    round_id: next_round,
                });
            }
        }
        let close = self.close.ok_or(CloseError::StaleCloseId)?;
        if close.close_id != close_id {
            return Err(CloseError::StaleCloseId);
        }
        if !close.cash_committed {
            return Err(CloseError::CashNotCommitted);
        }
        if !matches!(
            self.phase,
            Phase::PromotionApplied | Phase::PromotionSkipped(_)
        ) {
            return Err(CloseError::WrongPhase);
        }
        let next_round = self.round_id.checked_add(1).ok_or(CloseError::WrongPhase)?;
        let seed = self.seed_lamports;
        self.last_finalized_close = Some((close_id, next_round, seed));
        self.round_id = next_round;
        self.phase = Phase::Active;
        self.pot_lamports = seed;
        self.seed_lamports = 0;
        self.close = None;
        self.candidates = None;
        self.promotion_result = None;
        self.user_rounds.clear();
        Ok(FinalizeResult::Advanced {
            round_id: next_round,
            seed_lamports: seed,
        })
    }

    pub fn timer_close(&mut self, _now: u64) -> Result<u64, CloseError> {
        Err(CloseError::TimerCloseForbidden)
    }
    pub fn expire_finalize_open(
        &mut self,
        close_id: u64,
        expected_round_id: u64,
        execution_now: u64,
        args: OpenPositionArgs,
    ) -> Result<(), CloseError> {
        let mut next = self.clone();
        if next.round_id.checked_add(1) != Some(expected_round_id) {
            return Err(CloseError::StaleCloseId);
        }
        if next.close.and_then(|close| close.captured).is_some()
            && next.phase == Phase::AwaitingRandomness
        {
            next.apply_promotion().map_err(|_| CloseError::WrongPhase)?;
        } else if matches!(
            next.phase,
            Phase::AllocatingClosingToll | Phase::CountingCandidates | Phase::AwaitingRandomness
        ) {
            next.expire_promotion(execution_now)?;
        }
        next.finalize_round(close_id)?;
        next.open_position(
            args.owner,
            args.position_id,
            args.gross_deposit,
            args.plan_kind,
            execution_now,
        )
        .map_err(|_| CloseError::WrongPhase)?;
        *self = next;
        Ok(())
    }

    #[cfg(test)]
    pub fn map_output_for_test(
        output: [u8; 64],
        round_id: u64,
        candidate_count: usize,
    ) -> Option<(usize, u64)> {
        map_output(output, round_id, candidate_count)
    }
}

fn map_earnings_error(error: earnings::MathError) -> ClaimError {
    match error {
        earnings::MathError::NotMature => ClaimError::NotMature,
        earnings::MathError::BackwardsTime => ClaimError::BackwardsClock,
        earnings::MathError::MinimumReceiveNotMet => ClaimError::MinimumReceive,
        earnings::MathError::Overflow
        | earnings::MathError::InvalidTollBps
        | earnings::MathError::InvalidState
        | earnings::MathError::InvalidRemainder => ClaimError::InvalidPayout,
    }
}

fn request_address(client: [u8; 32], seed: [u8; 32]) -> [u8; 32] {
    // This is a deterministic host-model stand-in for the provider SDK's
    // request PDA derivation. It is not an on-chain PDA, account, or CPI proof.
    let mut hasher = Sha256::new();
    hasher.update(b"MC/EEC/REQUEST/v1");
    hasher.update(client);
    hasher.update(seed);
    hasher.finalize().into()
}

fn digest_participants(rows: &[Participant]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"MC/EEC/PARTICIPANTS/v1");
    hasher.update((rows.len() as u64).to_le_bytes());
    for row in rows {
        hasher.update(row.owner.to_le_bytes());
        hasher.update(row.gross_deposit.to_le_bytes());
        hasher.update(row.net_payout.to_le_bytes());
        hasher.update(row.loss.to_le_bytes());
        hasher.update([row.has_ever_a as u8, row.has_ever_b as u8]);
    }
    hasher.finalize().into()
}

fn digest_candidates(rows: &[(u64, u128)]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"MC/EEC/CANDIDATES/v1");
    hasher.update((rows.len() as u64).to_le_bytes());
    for (owner, loss) in rows {
        hasher.update(owner.to_le_bytes());
        hasher.update(loss.to_le_bytes());
    }
    hasher.finalize().into()
}

fn map_output(output: [u8; 64], round_id: u64, candidate_count: usize) -> Option<(usize, u64)> {
    map_samples(candidate_count, |counter| {
        let mut hasher = Sha256::new();
        hasher.update(b"MC/EEC/INDEX/v1");
        hasher.update(output);
        hasher.update(round_id.to_le_bytes());
        hasher.update(counter.to_le_bytes());
        let digest = hasher.finalize();
        let bytes: [u8; 8] = digest[..8].try_into().ok()?;
        Some(u64::from_le_bytes(bytes))
    })
}

fn map_samples(
    candidate_count: usize,
    mut sample: impl FnMut(u64) -> Option<u64>,
) -> Option<(usize, u64)> {
    let n = u128::try_from(candidate_count).ok()?;
    if n == 0 {
        return None;
    }
    let limit = (1u128 << 64) / n * n;
    for counter in 0..64u64 {
        let r = sample(counter)?;
        if u128::from(r) < limit {
            return Some((usize::try_from(u128::from(r) % n).ok()?, counter));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_rejection_sampler_has_no_winner() {
        assert_eq!(map_samples(3, |_| Some(u64::MAX)), None);
    }

    #[test]
    fn rejection_sampler_accepts_the_first_in_range_ordinal() {
        assert_eq!(
            map_samples(3, |counter| match counter {
                0 => Some(u64::MAX),
                _ => Some(7),
            }),
            Some((1, 1))
        );
    }

    #[test]
    fn allocation_overflow_does_not_leave_seed_mutated() {
        let mut round = Round::new(
            7,
            [1; 32],
            [2; 32],
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
        );
        round.phase = Phase::AllocatingClosingToll;
        round.close = Some(CloseState {
            close_id: 1,
            close_started_at: 0,
            promotion_deadline: PROMOTION_DEADLINE_SECS,
            cash_committed: false,
            request: None,
            captured: None,
        });
        round.pending_toll_lamports = 2;
        round.seed_lamports = u64::MAX - 1;
        round.funded_lamports = u64::MAX;
        let before = round.clone();

        assert_eq!(
            round.commit_allocation(1, 1),
            Err(RoundError::ArithmeticOverflow)
        );
        assert_eq!(round, before);
    }

    #[test]
    fn zero_owed_matured_simple_close_terminates_only_the_position() {
        let mut round = Round::new(
            7,
            [1; 32],
            [2; 32],
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
        );
        round
            .open_position(1, 1, 10_000_000, PlanKind::Simple, 0)
            .unwrap();
        round
            .open_position(2, 2, 10_000_000, PlanKind::Simple, 0)
            .unwrap();
        round
            .open_position(3, 3, 10_000_000, PlanKind::Simple, 0)
            .unwrap();
        let position = round.positions.get_mut(&1).unwrap();
        let maturity = position.plan.maturity_at;
        let accrual = position.simple_accrual.as_mut().unwrap();
        accrual.accrue(maturity).unwrap();
        position.paid_earnings = accrual.accrued_gross;

        let payout = round.close_position(1, 1, 0, maturity).unwrap();
        assert_eq!(payout.owed, 0);
        assert!(!payout.close_started);
        assert_eq!(round.phase, Phase::Active);
        assert!(round.positions.get(&1).unwrap().claimed);
    }

    #[test]
    fn series_a_book_overflow_and_ticket_barrier_roll_back_both_models() {
        let mut round = Round::new(
            7,
            [1; 32],
            [2; 32],
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
        );
        let mut overflow_book = backers::Book::with_balances(0, 0, 0, 0, u64::MAX);
        let before_round = round.clone();
        let before_book = overflow_book.clone();
        assert_eq!(
            round.fund_series_a(&mut overflow_book, 1, 50_000_000, 0),
            Err(RoundError::Backer(backers::Error::ArithmeticOverflow))
        );
        assert_eq!(round, before_round);
        assert_eq!(overflow_book, before_book);

        let mut book = backers::Book::with_balances(100, 0, 0, 0, 100);
        book.reserve_toll(1).unwrap();
        round.pot_lamports = book.pot;
        round.pending_toll_lamports = book.pending_toll;
        let before_round = round.clone();
        let before_book = book.clone();
        assert_eq!(
            round.fund_series_a(&mut book, 1, 50_000_000, 1),
            Err(RoundError::PendingToll)
        );
        assert_eq!(round, before_round);
        assert_eq!(book, before_book);

        book.start_ticket(1).unwrap();
        let before_round = round.clone();
        let before_book = book.clone();
        assert_eq!(
            round.fund_series_a(&mut book, 1, 50_000_000, 1),
            Err(RoundError::Backer(backers::Error::AllocationBarrier))
        );
        assert_eq!(round, before_round);
        assert_eq!(book, before_book);
    }
}
