use musical_chairs_economics::backers::{BackerKey, Book, Series as BackerSeries};
use musical_chairs_economics::earnings::{PlanKind, SECONDS_PER_DAY};
use musical_chairs_economics::rounds::{
    CaptureResult, ClaimError, CloseError, FinalizeResult, Phase, PromotionStatus, ProviderTuple,
    Round, RoundError, Series, SkipReason, PROMOTION_DEADLINE_SECS,
};

const GAME: u64 = 10_000_000;
const MATURITY_15: u64 = 15 * SECONDS_PER_DAY;
const MATURITY_SIMPLE: u64 = 21 * SECONDS_PER_DAY;

fn round() -> Round {
    Round::new(
        7,
        [1; 32],
        [2; 32],
        ProviderTuple::new([3; 32], [4; 32], [5; 32]),
    )
}

fn open(round: &mut Round, owner: u64, position: u64, kind: PlanKind, now: u64) {
    round
        .open_position(owner, position, GAME, kind, now)
        .unwrap();
}

fn synced_book(round: &Round, vault_lamports: u64) -> Book {
    Book::with_balances(
        round.pot_lamports(),
        round.seed_lamports(),
        round.management_lamports(),
        u128::from(round.funded_lamports()),
        vault_lamports,
    )
}

fn close_with_compound(round: &mut Round, owner: u64, position: u64) {
    let payout = round
        .claim_position(owner, position, 0, MATURITY_15)
        .unwrap();
    assert!(payout.close_started);
    assert_eq!(round.phase(), Phase::AllocatingClosingToll);
}

fn prepare_draw(round: &mut Round) -> musical_chairs_economics::rounds::RequestBinding {
    let pending = round.pending_toll_lamports();
    round.commit_allocation(0, pending).unwrap();
    round.seal_candidates().unwrap();
    let now = round.close_started_at().unwrap();
    round
        .request_draw(
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
            false,
            true,
            now,
        )
        .unwrap()
}

#[test]
fn healthy_rounds_cannot_be_timer_closed() {
    let mut round = round();
    assert_eq!(
        round.timer_close(1_000),
        Err(CloseError::TimerCloseForbidden)
    );
    assert_eq!(round.phase(), Phase::Active);
}

#[test]
fn admission_requires_minimum_and_owner_quota() {
    let mut round = round();
    assert_eq!(
        round.open_position(1, 1, GAME - 1, PlanKind::Simple, 0),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::BelowGameMinimum
        ))
    );
    open(&mut round, 1, 1, PlanKind::Simple, 0);
    open(&mut round, 1, 2, PlanKind::Simple, 1);
    open(&mut round, 1, 3, PlanKind::Simple, 2);
    assert!(matches!(
        round.open_position(1, 4, GAME, PlanKind::Simple, 3),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::QuotaExceeded
        ))
    ));
    open(&mut round, 1, 4, PlanKind::Simple, 3_600);
}

#[test]
fn series_a_funding_shares_game_quota_and_preserves_game_pl() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Simple, 100);
    open(&mut round, 1, 2, PlanKind::Simple, 101);
    let mut book = synced_book(&round, GAME * 2);
    let pot_before = round.pot_lamports();

    round.fund_series_a(&mut book, 1, 50_000_000, 102).unwrap();

    assert_eq!(round.pot_lamports(), pot_before + 50_000_000);
    assert_eq!(book.pot, round.pot_lamports());
    assert_eq!(
        book.backer(BackerKey::new(1, BackerSeries::A))
            .unwrap()
            .entitlement,
        62_000_000
    );
    let participant = round.participant_chunk(0, 1).unwrap().remove(0);
    assert_eq!(participant.gross_deposit, u128::from(GAME * 2));
    assert_eq!(participant.net_payout, 0);
    assert_eq!(
        round.open_position(1, 3, GAME, PlanKind::Simple, 103),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::QuotaExceeded
        ))
    );
    assert_eq!(
        round.fund_series_a(&mut book, 1, 50_000_000, 103),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::QuotaExceeded
        ))
    );
}

#[test]
fn series_a_funding_rejects_minimum_pause_and_book_closing_without_mutation() {
    let mut round = round();
    let mut book = synced_book(&round, 0);
    let before_round = round.clone();
    let before_book = book.clone();
    assert_eq!(
        round.fund_series_a(&mut book, 1, 49_999_999, 100),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::BelowSeriesAMinimum
        ))
    );
    assert_eq!(round, before_round);
    assert_eq!(book, before_book);

    round.set_admission_paused(true);
    let paused_round = round.clone();
    assert_eq!(
        round.fund_series_a(&mut book, 1, 50_000_000, 100),
        Err(RoundError::AdmissionPaused)
    );
    assert_eq!(round, paused_round);
    assert_eq!(book, before_book);

    round.set_admission_paused(false);
    book.set_closing(true).unwrap();
    let closing_round = round.clone();
    let closing_book = book.clone();
    assert_eq!(
        round.fund_series_a(&mut book, 1, 50_000_000, 100),
        Err(RoundError::Backer(
            musical_chairs_economics::backers::Error::ClosingAdmission
        ))
    );
    assert_eq!(round, closing_round);
    assert_eq!(book, closing_book);
}

#[test]
fn series_a_funding_rejects_a_closing_round_without_touching_the_book() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    let mut book = synced_book(&round, GAME);
    close_with_compound(&mut round, 1, 1);
    let before_round = round.clone();
    let before_book = book.clone();

    assert_eq!(
        round.fund_series_a(&mut book, 2, 50_000_000, MATURITY_15),
        Err(RoundError::NotActive)
    );
    assert_eq!(round, before_round);
    assert_eq!(book, before_book);
}

#[test]
fn repeated_simple_claims_derive_accrual_and_keep_position_open() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Simple, 0);
    let first = round.claim_position(1, 1, 0, 7 * SECONDS_PER_DAY).unwrap();
    assert!(first.net_payout > 0);
    let pending = round.pending_toll_lamports();
    assert!(pending > 0);
    assert_eq!(
        round.claim_position(1, 1, 0, 8 * SECONDS_PER_DAY),
        Err(ClaimError::PendingToll)
    );
    round.commit_allocation(0, pending).unwrap();
    let second = round.claim_position(1, 1, 0, 8 * SECONDS_PER_DAY).unwrap();
    assert!(second.net_payout > 0);
    assert_eq!(round.phase(), Phase::Active);
}

#[test]
fn compound_claim_requires_computed_maturity() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    assert_eq!(
        round.claim_position(1, 1, 0, MATURITY_15 - 1),
        Err(ClaimError::NotMature)
    );
    let payout = round.claim_position(1, 1, 0, MATURITY_15).unwrap();
    assert!(payout.owed > 0);
    assert!(payout.gross_paid > 0);
}

#[test]
fn explicit_simple_close_requires_maturity() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Simple, 0);
    assert_eq!(
        round.close_position(1, 1, 0, MATURITY_SIMPLE - 1),
        Err(ClaimError::NotMature)
    );
    for owner in 2..=3 {
        open(&mut round, owner, owner, PlanKind::Simple, 0);
    }
    let payout = round.close_position(1, 1, 0, MATURITY_SIMPLE).unwrap();
    assert!(!payout.close_started);
    assert_eq!(round.phase(), Phase::Active);
    round
        .commit_allocation(0, round.pending_toll_lamports())
        .unwrap();
    assert_eq!(
        round.close_position(1, 1, 0, MATURITY_SIMPLE),
        Err(ClaimError::Round(RoundError::PositionAlreadyClaimed))
    );
}

#[test]
fn solvent_compound_claim_stays_active_and_cannot_use_simple_close() {
    let mut round = round();
    for owner in 1..=5 {
        open(&mut round, owner, owner, PlanKind::Compound15, 0);
    }
    assert_eq!(
        round.close_position(1, 1, 0, MATURITY_15),
        Err(ClaimError::IneligibleClose)
    );
    let payout = round.claim_position(1, 1, 0, MATURITY_15).unwrap();
    assert!(!payout.close_started);
    assert_eq!(round.phase(), Phase::Active);
}

#[test]
fn toll_is_pending_escrow_and_blocks_later_payouts_until_commit() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    open(&mut round, 1, 2, PlanKind::Compound15, 1);
    round.claim_position(1, 1, 0, MATURITY_15).unwrap();
    let pending = round.pending_toll_lamports();
    assert_eq!(round.management_lamports(), GAME * 8 / 100);
    assert!(pending > 0);
    assert_eq!(
        round.claim_position(1, 2, 0, MATURITY_15),
        Err(ClaimError::PendingToll)
    );
    assert_eq!(
        round.commit_allocation(pending - 1, 0),
        Err(RoundError::InvalidAllocation)
    );
    round.commit_allocation(0, pending).unwrap();
    assert_eq!(round.pending_toll_lamports(), 0);
}

#[test]
fn candidates_freeze_backer_history_at_close() {
    let mut round = round();
    open(&mut round, 3, 1, PlanKind::Compound15, 0);
    open(&mut round, 4, 2, PlanKind::Compound15, 0);
    round.record_backer_exists_fixture(4, Series::A).unwrap();
    close_with_compound(&mut round, 3, 1);
    assert_eq!(
        round.record_backer_exists_fixture(5, Series::B),
        Err(RoundError::NotActive)
    );
    round
        .commit_allocation(0, round.pending_toll_lamports())
        .unwrap();
    let candidates = round.seal_candidates().unwrap();
    assert_eq!(candidates.preferred, Vec::<(u64, u128)>::new());
    assert_eq!(candidates.fallback, vec![(4, GAME as u128)]);
}

#[test]
fn promoted_b_history_excludes_prior_winner_from_next_round_preferred_candidates() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let binding = prepare_draw(&mut round);
    round
        .capture_draw(binding, round.close_started_at().unwrap(), [8; 64])
        .unwrap();
    assert!(matches!(
        round.apply_promotion().unwrap(),
        PromotionStatus::Applied { owner: 1, .. }
    ));
    let prior_entitlement = round.b_entitlement(1).unwrap();
    assert_eq!(
        round.finalize_round(1),
        Ok(FinalizeResult::Advanced {
            round_id: 8,
            seed_lamports: 0,
        })
    );

    let opened_at = MATURITY_15 + 100;
    open(&mut round, 1, 3, PlanKind::Compound15, opened_at);
    open(&mut round, 3, 4, PlanKind::Compound15, opened_at);
    open(&mut round, 4, 5, PlanKind::Compound15, opened_at);
    let payout = round
        .claim_position(4, 5, 0, opened_at + MATURITY_15)
        .unwrap();
    assert!(payout.close_started);
    round
        .commit_allocation(0, round.pending_toll_lamports())
        .unwrap();
    let candidates = round.seal_candidates().unwrap();

    assert_eq!(candidates.preferred, vec![(3, GAME as u128)]);
    assert_eq!(
        candidates.fallback,
        vec![(1, GAME as u128), (3, GAME as u128)]
    );
    assert_eq!(round.b_entitlement(1), Some(prior_entitlement));
}

#[test]
fn canonical_participants_are_stable_and_digest_complete() {
    let mut round = round();
    for (owner, position) in [(9, 1), (3, 2), (7, 3)] {
        open(&mut round, owner, position, PlanKind::Simple, 0);
    }
    assert_eq!(round.participant_count(), 3);
    assert_eq!(round.participant_chunk(0, 2).unwrap().len(), 2);
    assert!(round.participant_chunk(3, 1).is_err());
    assert_ne!(round.participant_digest(), [0; 32]);
}

#[test]
fn request_collision_and_retry_are_bound_to_one_marker() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let binding = prepare_draw(&mut round);
    let retry = round
        .request_draw(
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
            false,
            false,
            round.close_started_at().unwrap(),
        )
        .unwrap();
    assert_eq!(retry, binding);
}

#[test]
fn request_creation_after_deadline_is_rejected() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let pending = round.pending_toll_lamports();
    round.commit_allocation(0, pending).unwrap();
    round.seal_candidates().unwrap();
    assert_eq!(
        round.request_draw(
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
            false,
            true,
            round.close_started_at().unwrap() + PROMOTION_DEADLINE_SECS + 1,
        ),
        Err(RoundError::CaptureAfterDeadline)
    );
}

#[test]
fn capture_uses_full_64_byte_output_and_rejects_wrong_binding() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let binding = prepare_draw(&mut round);
    let mut wrong = binding;
    wrong.provider_owner = [9; 32];
    assert_eq!(
        round.capture_draw(wrong, 100, [8; 64]),
        Err(RoundError::ProviderBinding)
    );
    let start = round.close_started_at().unwrap();
    assert_eq!(
        round.capture_draw(binding, start + PROMOTION_DEADLINE_SECS, [8; 64]),
        Ok(CaptureResult::Captured)
    );
    assert_eq!(round.captured_at(), Some(start + PROMOTION_DEADLINE_SECS));
    assert_eq!(round.captured_output(), Some([8; 64]));
}

#[test]
fn expiry_is_available_before_candidate_work_and_backdated_terminal_capture_is_noop() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let deadline = round.promotion_deadline().unwrap();
    let skipped = round.expire_promotion(deadline + 1).unwrap();
    assert_eq!(skipped, PromotionStatus::Skipped(SkipReason::Deadline));
    assert_eq!(round.phase(), Phase::PromotionSkipped(SkipReason::Deadline));
    assert_eq!(
        round.expire_promotion(deadline + 1),
        Ok(PromotionStatus::Skipped(SkipReason::Deadline))
    );
    assert!(round
        .commit_allocation(0, round.pending_toll_lamports())
        .is_ok());
    assert_eq!(
        round.finalize_round(1),
        Ok(FinalizeResult::Advanced {
            round_id: 8,
            seed_lamports: 0,
        })
    );
}

#[test]
fn expiry_is_available_during_candidate_counting_after_cash_commit() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let pending = round.pending_toll_lamports();
    round.commit_allocation(0, pending).unwrap();
    assert_eq!(round.phase(), Phase::CountingCandidates);
    let skipped = round
        .expire_promotion(round.promotion_deadline().unwrap() + 1)
        .unwrap();
    assert_eq!(skipped, PromotionStatus::Skipped(SkipReason::Deadline));
    assert_eq!(
        round.finalize_round(1),
        Ok(FinalizeResult::Advanced {
            round_id: 8,
            seed_lamports: 0,
        })
    );
}

#[test]
fn deadline_capture_is_authenticated_noop_even_for_backdated_execution_input() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let binding = prepare_draw(&mut round);
    let deadline = round.promotion_deadline().unwrap();
    round.expire_promotion(deadline + 1).unwrap();
    assert_eq!(
        round.capture_draw(binding, 100, [7; 64]),
        Ok(CaptureResult::IgnoredLate)
    );
    assert!(round.captured_output().is_none());
}

#[test]
fn identical_authenticated_capture_remains_idempotent_after_promotion() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let binding = prepare_draw(&mut round);
    let output = [7; 64];
    round
        .capture_draw(binding, round.close_started_at().unwrap(), output)
        .unwrap();
    round.apply_promotion().unwrap();
    assert_eq!(
        round.capture_draw(binding, round.promotion_deadline().unwrap() + 1, output),
        Ok(CaptureResult::AlreadyCaptured)
    );
    assert_eq!(
        round.capture_draw(binding, round.promotion_deadline().unwrap() + 1, [8; 64]),
        Err(RoundError::TimelyCaptureAlreadyStored)
    );
}

#[test]
fn composed_expiry_rolls_back_when_cash_is_uncommitted() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let deadline = round.promotion_deadline().unwrap();
    let before = round.clone();
    assert_eq!(
        round.expire_finalize_open(
            1,
            8,
            deadline + 1,
            musical_chairs_economics::rounds::OpenPositionArgs {
                owner: 2,
                position_id: 2,
                gross_deposit: GAME,
                plan_kind: PlanKind::Simple,
            },
        ),
        Err(CloseError::CashNotCommitted)
    );
    assert_eq!(round, before);
    assert_eq!(
        round.expire_promotion(deadline + 1),
        Ok(PromotionStatus::Skipped(SkipReason::Deadline))
    );
}

#[test]
fn composed_expiry_finalizes_from_candidate_counting_after_cash_commit() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    round
        .commit_allocation(0, round.pending_toll_lamports())
        .unwrap();
    assert_eq!(round.phase(), Phase::CountingCandidates);
    let deadline = round.promotion_deadline().unwrap();

    round
        .expire_finalize_open(
            1,
            8,
            deadline + 1,
            musical_chairs_economics::rounds::OpenPositionArgs {
                owner: 2,
                position_id: 2,
                gross_deposit: GAME,
                plan_kind: PlanKind::Simple,
            },
        )
        .unwrap();

    assert_eq!(round.round_id(), 8);
    assert_eq!(round.phase(), Phase::Active);
}

#[test]
fn finalization_carries_seed_from_committed_allocation_once() {
    let mut round = round();
    open(&mut round, 1, 1, PlanKind::Compound15, 0);
    close_with_compound(&mut round, 1, 1);
    let seed = round.pending_toll_lamports();
    round.commit_allocation(seed, 0).unwrap();
    round.seal_candidates().unwrap();
    let binding = round
        .request_draw(
            ProviderTuple::new([3; 32], [4; 32], [5; 32]),
            false,
            true,
            round.close_started_at().unwrap(),
        )
        .unwrap();
    round
        .capture_draw(binding, round.close_started_at().unwrap(), [8; 64])
        .unwrap();
    round.apply_promotion().unwrap();
    assert_eq!(
        round.finalize_round(1),
        Ok(FinalizeResult::Advanced {
            round_id: 8,
            seed_lamports: seed,
        })
    );
    assert_eq!(round.pot_lamports(), seed);
    assert_eq!(round.seed_lamports(), 0);
    assert_eq!(
        round.finalize_round(1),
        Ok(FinalizeResult::AlreadyFinalized { round_id: 8 })
    );
}
