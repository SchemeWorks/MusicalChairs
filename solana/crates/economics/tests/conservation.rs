use musical_chairs_economics::backers::{BackerKey, Book, Series};

#[test]
fn commit_preserves_physical_custody_and_claims_are_not_recredited() {
    let mut book = Book::with_balances(100_000, 7_000, 3_000, 0, 110_000);
    book.add_backer(1, Series::A, 10_000).unwrap();
    book.add_backer(2, Series::B, 10_000).unwrap();
    let initial = book.conservation().unwrap();

    book.allocate(10_000, 1).unwrap();
    assert_eq!(book.conservation().unwrap(), initial);
    let funded_before_claim = book.global_funded_claims();
    let lifetime = book
        .backer(BackerKey::new(1, Series::A))
        .unwrap()
        .lifetime_credited;
    let claimed = book.claim(1).unwrap();
    assert!(claimed > 0);
    assert_eq!(
        book.global_funded_claims(),
        funded_before_claim - claimed as u128
    );
    assert_eq!(
        book.backer(BackerKey::new(1, Series::A))
            .unwrap()
            .lifetime_credited,
        lifetime
    );
    assert_eq!(book.conservation().unwrap(), initial - claimed as u128);

    let claimed_again = book.claim(1).unwrap();
    assert_eq!(claimed_again, 0);
    assert_eq!(book.conservation().unwrap(), initial - claimed as u128);
}

#[test]
fn overflow_and_insufficient_custody_fail_without_mutation() {
    let mut book = Book::with_pot(9_999);
    book.add_backer(1, Series::A, 100).unwrap();
    let before = book.clone();
    assert!(book.reserve_toll(10_000).is_err());
    assert_eq!(book, before);

    let mut overflow = Book::with_pot(10_000);
    overflow.add_backer(1, Series::A, u128::MAX).unwrap();
    let before = overflow.clone();
    assert!(overflow.add_backer(1, Series::A, 1).is_err());
    assert_eq!(overflow, before);
}

#[test]
fn fixture_funding_is_physically_backed_and_overflow_rolls_back() {
    let mut book = Book::with_pot(100);
    book.add_backer(1, Series::A, 1_000).unwrap();
    let before = book.conservation().unwrap();
    book.seed_funded_fixture(1, Series::A, 1).unwrap();
    assert_eq!(book.conservation().unwrap(), before + 1);
    assert_eq!(book.claim(1).unwrap(), 1);
    assert_eq!(book.conservation().unwrap(), before);

    let mut overflow = Book::with_pot(u64::MAX);
    overflow.add_backer(2, Series::A, 1_000).unwrap();
    let snapshot = overflow.clone();
    assert!(overflow.seed_funded_fixture(2, Series::A, 1).is_err());
    assert_eq!(overflow, snapshot);
}

#[test]
fn pending_credit_is_materialized_lazily_once() {
    let mut book = Book::with_pot(30_000);
    book.add_backer(1, Series::A, 10_000).unwrap();
    book.allocate(10_000, 1).unwrap();
    let after_commit = book.global_funded_claims();
    assert!(after_commit > 0);
    let first = book.claim(1).unwrap();
    assert!(first > 0);
    assert_eq!(book.global_funded_claims(), after_commit - first as u128);
    let second = book.claim(1).unwrap();
    assert_eq!(second, 0);
    assert_eq!(book.global_funded_claims(), 0);
}

#[test]
fn randomized_broad_inputs_conserve_against_an_independent_row_sum() {
    let mut state = 0x5eed_u64;
    for case in 0..32_u64 {
        let mut next = || {
            state ^= state << 7;
            state ^= state >> 9;
            state
        };
        let toll = 1 + next() % 50_000;
        let count = 1 + next() % 12;
        let mut book = Book::with_pot(toll + 1);
        let mut keys = Vec::new();
        for owner in 0..count {
            let series = if next() & 1 == 0 {
                Series::A
            } else {
                Series::B
            };
            let key = BackerKey::new(case * 100 + owner, series);
            let entitlement = 11 + next() % 20_000;
            book.add_backer(key.owner, key.series, entitlement as u128)
                .unwrap();
            keys.push(key);
        }
        let initial = book.conservation().unwrap();
        if let Err(error) = book.allocate(toll, 1 + (next() % 8) as usize) {
            eprintln!("case={case} toll={toll} count={count} error={error:?}");
            panic!("allocation failed");
        }
        assert_eq!(book.conservation().unwrap(), initial);
        let pending_sum: u128 = keys
            .iter()
            .map(|key| book.backer(*key).unwrap().pending as u128)
            .sum();
        assert_eq!(pending_sum, book.global_funded_claims());
        assert_eq!(pending_sum + book.seed() as u128, toll as u128);
    }
}

#[test]
fn fixture_funding_consumes_lifetime_cap_and_rejects_over_cap_atomically() {
    let mut book = Book::with_pot(2_000);
    book.add_backer(3, Series::A, 100).unwrap();
    book.seed_funded_fixture(3, Series::A, 60).unwrap();
    assert_eq!(
        book.backer(BackerKey::new(3, Series::A))
            .unwrap()
            .lifetime_credited,
        60
    );
    book.allocate(1_000, 1).unwrap();
    assert_eq!(
        book.backer(BackerKey::new(3, Series::A))
            .unwrap()
            .lifetime_credited,
        100
    );

    let mut over = Book::with_pot(100);
    over.add_backer(4, Series::A, 5).unwrap();
    let before = over.clone();
    assert!(over.seed_funded_fixture(4, Series::A, 6).is_err());
    assert_eq!(over, before);
}

#[test]
fn fixture_funding_reconciles_cutoff_and_rejects_active_ticket_mutation() {
    let mut book = Book::with_pot(100);
    book.add_backer(5, Series::A, 11).unwrap();
    book.seed_funded_fixture(5, Series::A, 11).unwrap();
    assert!(!book.is_active(BackerKey::new(5, Series::A)));
    assert_eq!(book.active_slot(BackerKey::new(5, Series::A)), None);

    let mut active = Book::with_pot(2_000);
    active.add_backer(6, Series::A, 100).unwrap();
    active.reserve_toll(1_000).unwrap();
    active.start_ticket(1_000).unwrap();
    let before = active.clone();
    assert!(active.seed_funded_fixture(6, Series::A, 1).is_err());
    assert_eq!(active, before);
}
