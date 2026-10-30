use musical_chairs_economics::backers::{BackerKey, Book, Error, Phase, Series};

fn seeded_book() -> Book {
    let mut book = Book::with_pot(100_000);
    book.add_backer(1, Series::A, 4_000).unwrap();
    book.add_backer(2, Series::A, 6_000).unwrap();
    book.add_backer(3, Series::B, 100_000).unwrap();
    book.add_backer(4, Series::B, 100_000).unwrap();
    book
}

#[test]
fn accepted_vector_applies_senior_then_sealed_a_and_equal_b() {
    let mut book = seeded_book();
    book.allocate(10_000, 8).unwrap();

    assert_eq!(
        book.backer(BackerKey::new(1, Series::A)).unwrap().pending,
        2_363
    );
    assert_eq!(
        book.backer(BackerKey::new(2, Series::A)).unwrap().pending,
        1_636
    );
    assert_eq!(
        book.backer(BackerKey::new(3, Series::B)).unwrap().pending,
        500
    );
    assert_eq!(
        book.backer(BackerKey::new(4, Series::B)).unwrap().pending,
        500
    );
    assert_eq!(book.seed(), 5_001);
    assert_eq!(book.ticket(), None);
}

#[test]
fn absent_b_folds_before_senior_and_senior_cap_dust_goes_to_seed() {
    let mut book = Book::with_pot(20_000);
    book.add_backer(10, Series::A, 1_000).unwrap();
    book.add_backer(11, Series::A, 10_000).unwrap();
    book.allocate(10_000, 4).unwrap();

    assert_eq!(
        book.backer(BackerKey::new(10, Series::A)).unwrap().pending,
        1_000
    );
    assert_eq!(
        book.backer(BackerKey::new(11, Series::A)).unwrap().pending,
        3_250
    );
    assert_eq!(book.seed(), 5_750);
}

#[test]
fn absent_a_folds_proportional_slice_into_equal_b_without_senior_credit() {
    let mut book = Book::with_pot(20_000);
    book.add_backer(20, Series::B, 10_000).unwrap();
    book.add_backer(21, Series::B, 10_000).unwrap();
    book.add_backer(22, Series::B, 10_000).unwrap();
    book.allocate(10_000, 4).unwrap();

    assert_eq!(
        book.backer(BackerKey::new(20, Series::B)).unwrap().pending,
        1_083
    );
    assert_eq!(
        book.backer(BackerKey::new(21, Series::B)).unwrap().pending,
        1_083
    );
    assert_eq!(
        book.backer(BackerKey::new(22, Series::B)).unwrap().pending,
        1_083
    );
    assert_eq!(book.seed(), 6_751);
}

#[test]
fn no_open_backers_routes_the_whole_toll_to_seed() {
    let mut book = Book::with_pot(10_001);
    book.allocate(10_001, 8).unwrap();
    assert_eq!(book.seed(), 10_001);
    assert_eq!(book.global_funded_claims(), 0);
}

#[test]
fn zero_and_sub_lamport_slice_tolls_skip_ticket_creation() {
    let mut book = Book::with_pot(4);
    book.add_backer(30, Series::A, 1_000).unwrap();
    book.allocate(3, 8).unwrap();
    assert_eq!(book.seed(), 3);
    assert_eq!(book.ticket(), None);
    book.allocate(0, 8).unwrap();
    assert_eq!(book.seed(), 3);
}

#[test]
fn eleven_lamport_senior_keeps_positive_post_senior_weight() {
    let mut book = Book::with_pot(6);
    book.add_backer(31, Series::A, 11).unwrap();
    book.allocate(6, 1).unwrap();
    assert_eq!(
        book.backer(BackerKey::new(31, Series::A)).unwrap().pending,
        2
    );
}

#[test]
fn cursor_rejects_skip_reorder_and_substitution_atomically() {
    let mut book = seeded_book();
    book.reserve_toll(10_000).unwrap();
    book.start_ticket(10_000).unwrap();
    assert_eq!(book.phase(), Some(Phase::Discover));

    let before = book.clone();
    assert_eq!(book.discover_chunk(&[1]).unwrap_err(), Error::InvalidCursor);
    assert_eq!(book, before);
    assert_eq!(
        book.discover_chunk(&[0, 2]).unwrap_err(),
        Error::InvalidCursor
    );
    assert_eq!(book, before);
    assert_eq!(
        book.discover_chunk(&[0, 99]).unwrap_err(),
        Error::InvalidCursor
    );
    assert_eq!(book, before);
    assert_eq!(
        book.discover_chunk_checked(&[(0, BackerKey::new(999, Series::A))])
            .unwrap_err(),
        Error::InvalidRow
    );
    assert_eq!(book, before);
}

#[test]
fn chunk_sizes_have_the_same_oracle() {
    let mut outputs = Vec::new();
    for chunk in [1, 4, 8, 32] {
        let mut book = seeded_book();
        book.allocate(10_001, chunk).unwrap();
        outputs.push((
            book.seed(),
            book.backer(BackerKey::new(1, Series::A)).unwrap().pending,
            book.backer(BackerKey::new(2, Series::A)).unwrap().pending,
            book.backer(BackerKey::new(3, Series::B)).unwrap().pending,
            book.backer(BackerKey::new(4, Series::B)).unwrap().pending,
        ));
    }
    assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
}
