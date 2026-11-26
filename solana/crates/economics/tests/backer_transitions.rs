use musical_chairs_economics::backers::{BackerKey, Book, Error, Phase, Series};

#[test]
fn claim_during_ticket_spends_old_funded_balance_only() {
    let mut book = Book::with_pot(40_000);
    book.add_backer(1, Series::A, 20_000).unwrap();
    book.seed_funded_fixture(1, Series::A, 100).unwrap();
    book.reserve_toll(10_000).unwrap();
    book.start_ticket(10_000).unwrap();
    book.discover_all().unwrap();
    book.apply_senior().unwrap();
    book.seal_weights().unwrap();

    let before = book.backer(BackerKey::new(1, Series::A)).unwrap().clone();
    assert_eq!(book.claim(1).unwrap(), 100);
    let after = book.backer(BackerKey::new(1, Series::A)).unwrap();
    assert_eq!(after.funded, 0);
    assert_eq!(after.pending, before.pending);
    assert_eq!(after.lifetime_credited, before.lifetime_credited);
    assert_eq!(book.phase(), Some(Phase::Allocate));
    book.allocate_all_current(8).unwrap();
    book.prune_all().unwrap();
    book.commit().unwrap();
}

#[test]
fn capped_rows_leave_registry_but_history_and_reopen_order_remain() {
    let mut book = Book::with_pot(30_000);
    book.add_backer(1, Series::A, 20).unwrap();
    book.add_backer(2, Series::A, 10_000).unwrap();
    book.allocate(10_000, 4).unwrap();
    assert!(!book.is_active(BackerKey::new(1, Series::A)));
    assert!(book.has_ever(1, Series::A));
    let original = book
        .backer(BackerKey::new(1, Series::A))
        .unwrap()
        .first_funding_sequence;
    book.add_backer(1, Series::A, 100).unwrap();
    assert_eq!(
        book.backer(BackerKey::new(1, Series::A))
            .unwrap()
            .first_funding_sequence,
        original
    );
    assert_eq!(book.active_slot(BackerKey::new(1, Series::A)), Some(1));
}

#[test]
fn barrier_allows_deposits_and_claims_but_rejects_backer_mutation_and_payout() {
    let mut book = Book::with_pot(30_000);
    book.add_backer(1, Series::A, 20_000).unwrap();
    book.seed_funded_fixture(1, Series::A, 100).unwrap();
    book.reserve_toll(10_000).unwrap();
    book.start_ticket(10_000).unwrap();
    assert!(book.add_backer(2, Series::B, 1_000).is_err());
    assert!(book.record_game_payout(1).is_err());
    assert!(book.accept_game_deposit(1_000).is_ok());
    assert_eq!(book.claim(1).unwrap(), 100);
}

#[test]
fn closing_rejects_deposit_without_affecting_ticket() {
    let mut book = Book::with_pot(30_000);
    book.add_backer(1, Series::A, 20_000).unwrap();
    book.reserve_toll(10_000).unwrap();
    book.start_ticket(10_000).unwrap();
    book.set_closing(true).unwrap();
    let before = book.clone();
    assert_eq!(
        book.accept_game_deposit(1).unwrap_err(),
        Error::ClosingAdmission
    );
    assert_eq!(book, before);
}

#[test]
fn lifetime_restore_on_existing_row_is_rejected_without_membership_change() {
    let mut book = Book::with_pot(1_000);
    book.add_backer(40, Series::A, 100).unwrap();
    let before = book.clone();
    assert!(book.add_backer_with_lifetime(40, Series::A, 0, 95).is_err());
    assert_eq!(book, before);
    assert!(book.is_active(BackerKey::new(40, Series::A)));
}

#[test]
fn descending_prune_rejects_replay_and_old_key_without_partial_mutation() {
    let mut book = Book::with_pot(1_000);
    for owner in 50..53 {
        book.add_backer(owner, Series::A, 11).unwrap();
    }
    book.reserve_toll(100).unwrap();
    book.start_ticket(100).unwrap();
    book.discover_all().unwrap();
    book.apply_senior().unwrap();
    book.seal_weights().unwrap();
    book.allocate_all_current(8).unwrap();
    assert_eq!(book.phase(), Some(Phase::Prune));

    let third = BackerKey::new(52, Series::A);
    let second = BackerKey::new(51, Series::A);
    let first = BackerKey::new(50, Series::A);
    book.prune_chunk(&[third]).unwrap();
    let before_replay = book.clone();
    assert!(book.prune_chunk(&[third]).is_err());
    assert_eq!(book, before_replay);
    assert!(book.prune_chunk(&[first]).is_err());
    assert_eq!(book, before_replay);
    book.prune_chunk(&[second, first]).unwrap();
    book.commit().unwrap();
}

#[test]
fn consecutive_tickets_materialize_old_credit_before_new_credit() {
    let mut book = Book::with_pot(30_000);
    book.add_backer(60, Series::A, 20_000).unwrap();
    book.allocate(10_000, 4).unwrap();
    let lifetime_after_first = book
        .backer(BackerKey::new(60, Series::A))
        .unwrap()
        .lifetime_credited;
    assert!(book.claim(60).unwrap() > 0);
    book.allocate(10_000, 4).unwrap();
    let row = book.backer(BackerKey::new(60, Series::A)).unwrap();
    assert!(row.lifetime_credited > lifetime_after_first);
    assert!(row.pending > 0);
    assert!(book.claim(60).unwrap() > 0);
    assert_eq!(book.global_funded_claims(), 0);
}

#[test]
fn oversized_allocation_continuation_after_partial_progress_is_bounded() {
    let mut book = Book::with_pot(2_000);
    book.add_backer(70, Series::A, 10_000).unwrap();
    book.add_backer(71, Series::A, 10_000).unwrap();
    book.reserve_toll(1_000).unwrap();
    book.start_ticket(1_000).unwrap();
    book.discover_all().unwrap();
    book.apply_senior().unwrap();
    book.seal_weights().unwrap();
    assert_eq!(book.phase(), Some(Phase::Allocate));
    book.allocate_chunk(&[0]).unwrap();
    book.allocate_all_current(usize::MAX).unwrap();
    assert_eq!(book.phase(), Some(Phase::Prune));
}

#[test]
fn partial_prune_then_default_prune_all_handles_max_chunk_atomically() {
    let mut book = Book::with_pot(1_000);
    for owner in 80..83 {
        book.add_backer(owner, Series::A, 11).unwrap();
    }
    book.reserve_toll(100).unwrap();
    book.start_ticket(100).unwrap();
    book.discover_all().unwrap();
    book.apply_senior().unwrap();
    book.seal_weights().unwrap();
    book.allocate_all_current(1).unwrap();
    assert_eq!(book.phase(), Some(Phase::Prune));
    book.prune_chunk(&[BackerKey::new(82, Series::A)]).unwrap();
    let after_partial = book.clone();
    book.prune_all().unwrap();
    assert_eq!(book.phase(), Some(Phase::Commit));
    assert_ne!(book, after_partial);
    book.commit().unwrap();
}

#[test]
fn closing_rejects_later_game_payout_atomically() {
    let mut book = Book::with_pot(1_000);
    book.set_closing(true).unwrap();
    let before = book.clone();
    assert_eq!(
        book.record_game_payout(1).unwrap_err(),
        Error::ClosingAdmission
    );
    assert_eq!(book, before);
}
