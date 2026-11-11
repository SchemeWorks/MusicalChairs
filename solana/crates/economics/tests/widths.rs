use ethnum::U256;
use musical_chairs_economics::backers::{
    pro_rata, sealed_denominator, BackerKey, Book, Error, Series,
};

#[test]
fn sealed_denominator_sums_past_u128_without_overflow() {
    let rows = vec![u128::MAX, u128::MAX, 7];
    let d = sealed_denominator(&rows, 0).expect("U256 sum");
    assert_eq!(d, U256::from(u128::MAX) * 2 + 7);
}

#[test]
fn pro_rata_floors_and_fits_u64() {
    // Sealed 4,000 + 6,000 rows after 1,750 senior: denominator 8,250.
    // 2,250 * 2,250 / 8,250 = 613.6 -> 613 and 2,250 * 6,000 / 8,250 = 1,636.3 -> 1,636.
    let d = sealed_denominator(&[4_000, 6_000], 1_750).unwrap();
    assert_eq!(d, U256::from(8_250u64));
    assert_eq!(pro_rata(2_250, 4_000 - 1_750, d).unwrap(), 613);
    assert_eq!(pro_rata(2_250, 6_000, d).unwrap(), 1_636);
}

#[test]
fn pro_rata_and_sealing_reject_zero_denominator_overflow_and_underflow() {
    assert_eq!(
        pro_rata(10, 1, U256::ZERO).unwrap_err(),
        Error::ZeroDenominator
    );
    // A weight above its denominator can exceed the cash width: reject, never truncate.
    assert_eq!(
        pro_rata(u64::MAX, 2, U256::ONE).unwrap_err(),
        Error::ArithmeticOverflow
    );
    // The widest legitimate result: the whole slice at weight == denominator.
    let d = sealed_denominator(&[u128::MAX], 0).unwrap();
    assert_eq!(pro_rata(u64::MAX, u128::MAX, d).unwrap(), u64::MAX);
    // A senior credit above the captured total is corrupt state, not a wrap.
    assert_eq!(
        sealed_denominator(&[5], 6).unwrap_err(),
        Error::ConservationMismatch
    );
    assert_eq!(sealed_denominator(&[], 0).unwrap(), U256::ZERO);
}

/// Two near-`u128::MAX` Series-A rows push the discovery accumulator past
/// `u128`. The sealed denominator is therefore `U256`, and each share is the
/// exact `floor(slice * weight / denominator)`.
#[test]
fn discovery_accumulator_above_u128_yields_correct_allocation() {
    let senior_slice = 1_750u64;
    let a_slice = 2_250u64; // 45% of the 5,000 half; B is present, so no fold
    let b_slice = 1_000u64;
    let a1_entitlement = u128::MAX - 1_000;
    let a2_entitlement = u128::MAX - 2_000;
    let b_entitlement = 100_000u128;

    // Independent oracle: the post-senior weights and their U256 sum.
    let w1 = a1_entitlement - u128::from(senior_slice);
    let w2 = a2_entitlement;
    let denominator = U256::from(w1) + U256::from(w2);
    assert!(
        denominator > U256::from(u128::MAX),
        "fixture must exceed u128"
    );
    let share = |weight: u128| (U256::from(a_slice) * U256::from(weight) / denominator).as_u64();
    let expected_a1 = senior_slice + share(w1);
    let expected_a2 = share(w2);
    let expected_b = b_slice;
    let credited = expected_a1 + expected_a2 + expected_b;
    let expected_seed = 10_000 - credited;

    for chunk in [1usize, 2, 3, 8] {
        let mut book = Book::with_pot(10_000);
        book.add_backer(1, Series::A, a1_entitlement).unwrap();
        book.add_backer(2, Series::A, a2_entitlement).unwrap();
        book.add_backer(3, Series::B, b_entitlement).unwrap();

        book.allocate(10_000, chunk).unwrap();

        let pending = |owner, series| book.backer(BackerKey::new(owner, series)).unwrap().pending;
        assert_eq!(pending(1, Series::A), expected_a1, "chunk {chunk}");
        assert_eq!(pending(2, Series::A), expected_a2, "chunk {chunk}");
        assert_eq!(pending(3, Series::B), expected_b, "chunk {chunk}");
        assert_eq!(book.seed(), expected_seed, "chunk {chunk}");
        assert_eq!(book.ticket(), None);
        assert_eq!(book.global_funded_claims(), u128::from(credited));
        book.conservation().unwrap();
    }
}

#[test]
fn backer_goldens_fixture_matches_the_book() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/economics.json")).unwrap();
    let goldens = fixture["backer_goldens"]
        .as_object()
        .expect("backer_goldens");
    assert_eq!(goldens.len(), 3, "the three accepted spec 3.5 goldens");
    let ints = |value: &serde_json::Value| -> Vec<u64> {
        value
            .as_array()
            .expect("integer array")
            .iter()
            .map(|v| v.as_u64().expect("integer"))
            .collect()
    };
    for (name, golden) in goldens {
        let toll = golden["toll"].as_u64().unwrap();
        let a_entitlements = ints(&golden["a_entitlements"]);
        let b_entitlements = ints(&golden["b_entitlements"]);
        let a_credits = ints(&golden["a_credits"]);
        let b_credits = ints(&golden["b_credits"]);
        let seed = golden["seed"].as_u64().unwrap();
        assert_eq!(a_entitlements.len(), a_credits.len(), "{name}");
        assert_eq!(b_entitlements.len(), b_credits.len(), "{name}");
        assert_eq!(
            toll,
            seed + a_credits.iter().sum::<u64>() + b_credits.iter().sum::<u64>(),
            "{name}: fixture conserves the toll"
        );
        for chunk in [1usize, 2, 8] {
            let mut book = Book::with_pot(toll);
            for (index, &entitlement) in a_entitlements.iter().enumerate() {
                book.add_backer(1 + index as u64, Series::A, u128::from(entitlement))
                    .unwrap();
            }
            for (index, &entitlement) in b_entitlements.iter().enumerate() {
                book.add_backer(101 + index as u64, Series::B, u128::from(entitlement))
                    .unwrap();
            }
            book.allocate(toll, chunk).unwrap();
            for (index, &credit) in a_credits.iter().enumerate() {
                let key = BackerKey::new(1 + index as u64, Series::A);
                assert_eq!(
                    book.backer(key).unwrap().pending,
                    credit,
                    "{name} A{index} chunk {chunk}"
                );
            }
            for (index, &credit) in b_credits.iter().enumerate() {
                let key = BackerKey::new(101 + index as u64, Series::B);
                assert_eq!(
                    book.backer(key).unwrap().pending,
                    credit,
                    "{name} B{index} chunk {chunk}"
                );
            }
            assert_eq!(book.seed(), seed, "{name} chunk {chunk}");
            assert_eq!(book.ticket(), None, "{name}");
            book.conservation().unwrap();
        }
    }
}
