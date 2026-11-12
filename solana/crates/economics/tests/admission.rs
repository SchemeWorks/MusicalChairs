use musical_chairs_economics::admission::{Admission, Error};

#[test]
fn game_and_series_a_minima_are_checked_before_consuming_quota() {
    let mut admission = Admission::new();
    assert_eq!(
        admission.open_position(1, 9_999_999),
        Err(Error::BelowGameMinimum)
    );
    assert_eq!(admission.successful_calls(), 0);
    assert_eq!(
        admission.fund_series_a(1, 49_999_999),
        Err(Error::BelowSeriesAMinimum)
    );
    assert_eq!(admission.successful_calls(), 0);
}

#[test]
fn exactly_three_successes_are_allowed_and_fourth_is_rejected_atomically() {
    let mut admission = Admission::new();
    for timestamp in 0..3 {
        admission.open_position(timestamp, 10_000_000).unwrap();
    }
    assert_eq!(
        admission.open_position(3, 10_000_000),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(admission.successful_calls(), 3);
}

#[test]
fn timestamp_exactly_one_hour_old_expires() {
    let mut admission = Admission::new();
    admission.open_position(100, 10_000_000).unwrap();
    assert!(admission.open_position(100 + 3_600, 10_000_000).is_ok());
    assert_eq!(admission.successful_calls(), 1);
}

#[test]
fn backward_time_and_failed_calls_leave_quota_unchanged() {
    let mut admission = Admission::new();
    admission.open_position(100, 10_000_000).unwrap();
    let snapshot = admission;
    assert_eq!(
        admission.open_position(99, 10_000_000),
        Err(Error::BackwardsTime)
    );
    assert_eq!(admission, snapshot);
    assert_eq!(
        admission.fund_series_a(100, 49_999_999),
        Err(Error::BelowSeriesAMinimum)
    );
    assert_eq!(admission, snapshot);
}

#[test]
fn claims_and_round_progress_do_not_consume_quota() {
    let mut admission = Admission::new();
    admission.open_position(1, 10_000_000).unwrap();
    for _ in 0..100 {
        admission.record_claim_or_round_progress();
    }
    assert_eq!(admission.successful_calls(), 1);
}
