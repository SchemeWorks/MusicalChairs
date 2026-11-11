//! Cross-module economic checks.
//!
//! These tests join the fee, earnings, Round, and Backer books. The unit
//! vector tests prove each table in isolation; this harness checks that the
//! same physical vault partitions remain conserved when matured claims feed
//! Backer allocation.

use musical_chairs_economics::{
    backers::{
        BackerKey, Book, Error as BackerError, Phase as BackerPhase, Series as BackerSeries,
    },
    earnings::{self, PlanKind},
    fees,
    rounds::{
        ClaimError, OpenPositionArgs, Payout as RoundPayout, Phase, PromotionStatus, ProviderTuple,
        Round, RoundError,
    },
};

const OPENED_AT: u64 = 1_000;
const MATURITY: u64 = OPENED_AT + earnings::SIMPLE_EXPIRY_SECONDS;
const GROSS: u64 = 50_000_000;

fn round() -> Round {
    Round::new(
        7,
        [1; 32],
        [2; 32],
        ProviderTuple::new([3; 32], [4; 32], [5; 32]),
    )
}

fn simple_owed(now: u64) -> u128 {
    let mut accrual = earnings::SimpleAccrual::new(GROSS, OPENED_AT).unwrap();
    accrual.accrue(now).unwrap();
    accrual.accrued_gross
}

fn expected_simple_payout(pot: u64) -> earnings::Payout {
    earnings::Payout::from_gross(
        simple_owed(MATURITY),
        pot,
        fees::simple_toll_bps(MATURITY - OPENED_AT),
        0,
    )
    .unwrap()
}

fn assert_book_partition(book: &Book) {
    let tracked = u128::from(book.pot)
        + u128::from(book.management)
        + u128::from(book.pending_toll)
        + u128::from(book.seed)
        + book.global_funded_claims;
    assert_eq!(tracked, u128::from(book.vault_lamports));
    assert_eq!(
        book.conservation().unwrap(),
        u128::from(book.vault_lamports)
    );
}

fn assert_joined_cash(round: &Round, book: &Book) {
    assert_eq!(book.pot, round.pot_lamports());
    assert_eq!(book.management, round.management_lamports());
    assert_eq!(book.pending_toll, round.pending_toll_lamports());
    assert_eq!(book.seed, round.seed_lamports());
    assert_eq!(
        book.global_funded_claims,
        u128::from(round.funded_lamports())
    );
    assert_book_partition(book);
}

/// Feed one Round payout through the same Book vault, then commit the exact
/// seed/funded split back into Round. The intermediate assertions cover the
/// escrow and cursor phases; the joined assertion covers the post-commit state.
fn settle_claim(round: &mut Round, book: &mut Book, payout: RoundPayout) {
    book.record_game_payout(payout.net_payout).unwrap();
    book.reserve_toll(payout.toll).unwrap();
    assert_joined_cash(round, book);

    let seed_before = book.seed();
    let funded_before = book.global_funded_claims();
    book.start_ticket(payout.toll).unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::Discover));
    book.discover_all().unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::Senior));
    book.apply_senior().unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::SealWeights));
    book.seal_weights().unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::Allocate));
    book.allocate_all_current(8).unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::Prune));
    book.prune_all().unwrap();
    assert_eq!(book.phase(), Some(BackerPhase::Commit));
    book.commit().unwrap();
    assert_book_partition(book);

    let seed_credit = book.seed() - seed_before;
    let funded_credit = u64::try_from(book.global_funded_claims - funded_before).unwrap();
    round.commit_allocation(seed_credit, funded_credit).unwrap();
    assert_joined_cash(round, book);
}

#[test]
fn joined_round_backer_and_fee_cash_conservation() {
    let entry = fees::front_end_load(GROSS).unwrap();
    let mut round = round();
    for owner in 1..=6 {
        round
            .open_position(owner, owner, GROSS, PlanKind::Simple, OPENED_AT)
            .unwrap();
        assert_eq!(round.pot_lamports(), entry.pot_credit * owner);
        assert_eq!(round.management_lamports(), entry.entry_fee * owner);
    }

    // The Book starts from exactly the six real deposits represented by Round:
    // pot + management is the front-end partition and vault is six gross.
    let mut book = Book::with_balances(
        round.pot_lamports(),
        0,
        round.management_lamports(),
        0,
        GROSS * 6,
    );
    book.add_backer(10, BackerSeries::A, 500_000_000).unwrap();
    book.add_backer(11, BackerSeries::B, 500_000_000).unwrap();
    assert_joined_cash(&round, &book);

    let before_maturity = round.clone();
    assert_eq!(
        round.close_position(6, 6, 0, MATURITY - 1),
        Err(ClaimError::NotMature)
    );
    assert_eq!(round, before_maturity);

    let expected = expected_simple_payout(round.pot_lamports());
    let payout = round.claim_position(1, 1, expected.net, MATURITY).unwrap();
    assert_eq!(payout.owed, expected.gross_owed);
    assert_eq!(payout.gross_paid, expected.paid_gross);
    assert_eq!(payout.net_payout, expected.net);
    assert_eq!(payout.toll, expected.toll);
    assert_eq!(payout.gross_paid, payout.net_payout + payout.toll);
    settle_claim(&mut round, &mut book, payout);

    for owner in 1..=5 {
        round
            .record_backer_exists_fixture(owner, musical_chairs_economics::rounds::Series::A)
            .unwrap();
    }

    let expected = expected_simple_payout(round.pot_lamports());
    let payout = round.claim_position(2, 2, expected.net, MATURITY).unwrap();
    assert_eq!(payout.owed, expected.gross_owed);
    assert_eq!(payout.gross_paid, payout.net_payout + payout.toll);
    settle_claim(&mut round, &mut book, payout);

    let before_final = round.clone();
    let expected = expected_simple_payout(round.pot_lamports());
    let insolvent = round.claim_position(3, 3, expected.net, MATURITY).unwrap();
    assert!(insolvent.close_started);
    assert_eq!(insolvent.owed, expected.gross_owed);
    assert_eq!(insolvent.gross_paid, insolvent.net_payout + insolvent.toll);
    assert!(matches!(round.phase(), Phase::AllocatingClosingToll));
    assert_ne!(round, before_final);
    settle_claim(&mut round, &mut book, insolvent);
    assert_eq!(round.pending_toll_lamports(), 0);

    let a = book.backer(BackerKey::new(10, BackerSeries::A)).unwrap();
    let b = book.backer(BackerKey::new(11, BackerSeries::B)).unwrap();
    let pending = u128::from(a.pending) + u128::from(b.pending);
    let funded = u128::from(a.funded) + u128::from(b.funded);
    assert!(pending > 0);
    assert_eq!(funded + pending, book.global_funded_claims());
    assert_eq!(
        u128::from(book.seed()) + book.global_funded_claims(),
        u128::from(round.seed_lamports()) + u128::from(round.funded_lamports())
    );
    assert_joined_cash(&round, &book);

    let participants = round
        .participant_chunk(0, round.participant_count())
        .unwrap();
    assert_eq!(participants.len(), 6);
    for participant in &participants {
        assert_eq!(
            participant.loss,
            participant
                .gross_deposit
                .saturating_sub(participant.net_payout)
        );
    }
    assert_eq!(participants[5].owner, 6);
    assert_eq!(participants[5].gross_deposit, u128::from(GROSS));
    assert_eq!(participants[5].net_payout, 0);
    assert_eq!(participants[5].loss, u128::from(GROSS));

    let candidates = round.seal_candidates().unwrap();
    assert_eq!(candidates.preferred.len(), 1);
    assert_eq!(candidates.preferred[0], (6, participants[5].loss));

    let provider = ProviderTuple::new([3; 32], [4; 32], [5; 32]);
    let close_started = round.close_started_at().unwrap();
    let binding = round
        .request_draw(provider, false, true, close_started)
        .unwrap();
    assert!(round
        .capture_draw(binding, round.promotion_deadline().unwrap(), [8; 64])
        .is_ok());
    assert_eq!(
        round.apply_promotion().unwrap(),
        PromotionStatus::Applied {
            owner: 6,
            entitlement: participants[5].loss * 116 / 100,
        }
    );

    let carry = round.seed_lamports();
    let deadline = round.promotion_deadline().unwrap();
    round
        .expire_finalize_open(
            1,
            8,
            deadline,
            OpenPositionArgs {
                owner: 9,
                position_id: 9,
                gross_deposit: GROSS,
                plan_kind: PlanKind::Simple,
            },
        )
        .unwrap();
    assert_eq!(round.round_id(), 8);
    assert_eq!(round.pot_lamports(), carry + entry.pot_credit);
    assert_eq!(round.seed_lamports(), 0);

    // Round rollover carries only its governed seed. The Book model has no
    // rollover instruction, so reclassify its remaining old-round pot into
    // seed without adding or removing lamports, then check its full vault.
    let old_round_pot = book.pot;
    book.pot = 0;
    book.seed = book.seed.checked_add(old_round_pot).unwrap();
    let vault_before_reclass = book.vault_lamports;
    assert_book_partition(&book);
    assert_eq!(book.vault_lamports, vault_before_reclass);

    assert_eq!(
        round.claim_position(1, 1, 0, MATURITY + 1),
        Err(ClaimError::Round(RoundError::StalePosition))
    );
    let carried_pot = round.pot_lamports();
    assert_eq!(
        round.finalize_round(1),
        Ok(musical_chairs_economics::rounds::FinalizeResult::AlreadyFinalized { round_id: 8 })
    );
    assert_eq!(round.pot_lamports(), carried_pot);
}

#[test]
fn joined_model_rejects_invalid_provider_without_mutation() {
    let mut round = round();
    round
        .open_position(1, 1, GROSS, PlanKind::Simple, OPENED_AT)
        .unwrap();
    let expected = expected_simple_payout(round.pot_lamports());
    round.claim_position(1, 1, expected.net, MATURITY).unwrap();
    round.commit_allocation(0, expected.toll).unwrap();
    round.seal_candidates().unwrap();
    let before = round.clone();
    assert_eq!(
        round.request_draw(
            ProviderTuple::new([9; 32], [4; 32], [5; 32]),
            false,
            true,
            MATURITY,
        ),
        Err(RoundError::ProviderBinding)
    );
    assert_eq!(round, before);

    let mut book = Book::with_pot(100_000);
    let before_book = book.clone();
    assert_eq!(
        book.reserve_toll(100_001),
        Err(BackerError::InsufficientPot)
    );
    assert_eq!(book, before_book);
}

#[test]
fn derived_plan_admission_and_repeated_claims_keep_fee_terms_atomic() {
    let mut round = round();
    let before = round.clone();
    assert_eq!(
        round.open_position(
            1,
            1,
            fees::GAME_MINIMUM_LAMPORTS - 1,
            PlanKind::Simple,
            OPENED_AT
        ),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::BelowGameMinimum
        ))
    );
    assert_eq!(round, before);

    for position_id in 1..=3 {
        round
            .open_position(
                1,
                position_id,
                GROSS,
                PlanKind::Simple,
                OPENED_AT + position_id,
            )
            .unwrap();
    }
    let before_quota = round.clone();
    assert_eq!(
        round.open_position(1, 4, GROSS, PlanKind::Simple, OPENED_AT + 4),
        Err(RoundError::Admission(
            musical_chairs_economics::admission::Error::QuotaExceeded
        ))
    );
    assert_eq!(round, before_quota);

    let mut repeated = Round::new(
        7,
        [1; 32],
        [2; 32],
        ProviderTuple::new([3; 32], [4; 32], [5; 32]),
    );
    repeated
        .open_position(1, 1, GROSS, PlanKind::Simple, OPENED_AT)
        .unwrap();
    let first_now = OPENED_AT + 7 * earnings::SECONDS_PER_DAY;
    let first_expected = earnings::Payout::from_gross(
        simple_owed(first_now),
        repeated.pot_lamports(),
        fees::simple_toll_bps(first_now - OPENED_AT),
        0,
    )
    .unwrap();
    let first = repeated
        .claim_position(1, 1, first_expected.net + 1, first_now)
        .unwrap_err();
    assert_eq!(first, ClaimError::MinimumReceive);
    assert_eq!(repeated.pending_toll_lamports(), 0);
    let first = repeated
        .claim_position(1, 1, first_expected.net, first_now)
        .unwrap();
    assert_eq!(first.net_payout, first_expected.net);
    assert_eq!(first.toll, first_expected.toll);
    let pending = repeated.pending_toll_lamports();
    assert!(pending > 0);
    assert_eq!(
        repeated.claim_position(1, 1, 0, first_now + earnings::SECONDS_PER_DAY),
        Err(ClaimError::PendingToll)
    );
    repeated.commit_allocation(0, pending).unwrap();

    let second_now = OPENED_AT + 8 * earnings::SECONDS_PER_DAY;
    let second_owed = simple_owed(second_now) - u128::from(first.gross_paid);
    let second_expected = earnings::Payout::from_gross(
        second_owed,
        repeated.pot_lamports(),
        fees::simple_toll_bps(second_now - OPENED_AT),
        0,
    )
    .unwrap();
    let second = repeated
        .claim_position(1, 1, second_expected.net, second_now)
        .unwrap();
    assert_eq!(second.owed, second_owed);
    assert_eq!(second.net_payout, second_expected.net);
    assert_eq!(second.toll, second_expected.toll);
    assert_eq!(repeated.phase(), Phase::Active);
}
