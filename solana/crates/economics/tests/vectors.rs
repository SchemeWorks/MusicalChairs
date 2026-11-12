use musical_chairs_economics::{earnings, fees};
#[derive(Debug)]
struct Vectors {
    simple_one_day: Case,
    simple_before_day_7: Case,
    simple_day_7: Case,
    simple_before_day_14: Case,
    simple_day_14: Case,
    simple_maturity: Case,
    compound_15: Case,
    compound_30: Case,
    insolvent: InsolventCase,
}

#[derive(Debug)]
struct Case {
    principal: u64,
    elapsed_seconds: u64,
    owed: u64,
    toll_bps: u16,
    toll: u64,
    net: u64,
}

#[derive(Debug)]
struct InsolventCase {
    owed: u128,
    pot: u64,
    toll_bps: u16,
    paid_gross: u64,
    toll: u64,
    net: u64,
}

fn fixture() -> Vectors {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/economics.json")).unwrap();
    let case = |name: &str| {
        let value = &value[name];
        Case {
            principal: value["principal"].as_u64().unwrap(),
            elapsed_seconds: value["elapsed_seconds"].as_u64().unwrap(),
            owed: value["owed"].as_u64().unwrap(),
            toll_bps: value["toll_bps"].as_u64().unwrap() as u16,
            toll: value["toll"].as_u64().unwrap(),
            net: value["net"].as_u64().unwrap(),
        }
    };
    let insolvent = &value["insolvent"];
    Vectors {
        simple_one_day: case("simple_one_day"),
        simple_before_day_7: case("simple_before_day_7"),
        simple_day_7: case("simple_day_7"),
        simple_before_day_14: case("simple_before_day_14"),
        simple_day_14: case("simple_day_14"),
        simple_maturity: case("simple_maturity"),
        compound_15: case("compound_15"),
        compound_30: case("compound_30"),
        insolvent: InsolventCase {
            owed: insolvent["owed"].as_u64().unwrap() as u128,
            pot: insolvent["pot"].as_u64().unwrap(),
            toll_bps: insolvent["toll_bps"].as_u64().unwrap() as u16,
            paid_gross: insolvent["paid_gross"].as_u64().unwrap(),
            toll: insolvent["toll"].as_u64().unwrap(),
            net: insolvent["net"].as_u64().unwrap(),
        },
    }
}

fn simple_case(case: &Case) -> earnings::Payout {
    let mut accrual = earnings::SimpleAccrual::new(case.principal, 0).unwrap();
    let owed = accrual.accrue(case.elapsed_seconds).unwrap();
    assert_eq!(owed, u128::from(case.owed));
    let fee = fees::payout(case.owed.into(), case.owed, case.toll_bps).unwrap();
    assert_eq!(fee.toll, case.toll);
    assert_eq!(fee.net, case.net);
    earnings::Payout::from_gross(case.owed.into(), case.owed, case.toll_bps, 0).unwrap()
}

#[test]
fn accepted_simple_vectors_match_independent_fixture() {
    let vectors = fixture();
    for case in [
        &vectors.simple_one_day,
        &vectors.simple_before_day_7,
        &vectors.simple_day_7,
        &vectors.simple_before_day_14,
        &vectors.simple_day_14,
        &vectors.simple_maturity,
    ] {
        let payout = simple_case(case);
        assert_eq!(payout.net, case.net);
    }
}

#[test]
fn compound_maturity_vectors_use_floor_q18_factors_and_jackpot_fees() {
    let vectors = fixture();
    let fifteen = earnings::Plan::compound_15(vectors.compound_15.principal, 0).unwrap();
    let thirty = earnings::Plan::compound_30(vectors.compound_30.principal, 0).unwrap();
    assert_eq!(
        fifteen.owed_at(vectors.compound_15.elapsed_seconds - 1),
        Err(earnings::MathError::NotMature)
    );
    assert_eq!(
        thirty.owed_at(vectors.compound_30.elapsed_seconds - 1),
        Err(earnings::MathError::NotMature)
    );
    assert_eq!(
        fifteen
            .owed_at(vectors.compound_15.elapsed_seconds)
            .unwrap(),
        u128::from(vectors.compound_15.owed)
    );
    assert_eq!(
        thirty.owed_at(vectors.compound_30.elapsed_seconds).unwrap(),
        u128::from(vectors.compound_30.owed)
    );
    assert_eq!(
        thirty
            .owed_at(vectors.compound_30.elapsed_seconds + 1)
            .unwrap(),
        u128::from(vectors.compound_30.owed)
    );
    assert_eq!(
        fees::payout(
            vectors.compound_15.owed.into(),
            vectors.compound_15.owed,
            vectors.compound_15.toll_bps
        )
        .unwrap()
        .toll,
        vectors.compound_15.toll
    );
    assert_eq!(
        fees::payout(
            vectors.compound_30.owed.into(),
            vectors.compound_30.owed,
            vectors.compound_30.toll_bps
        )
        .unwrap()
        .net,
        vectors.compound_30.net
    );
}

#[test]
fn insolvent_payout_charges_toll_on_actual_gross_and_conserves_pot() {
    let case = fixture().insolvent;
    let payout = earnings::Payout::from_gross(case.owed, case.pot, case.toll_bps, 0).unwrap();
    assert!(payout.insolvent);
    assert_eq!(payout.paid_gross, case.paid_gross);
    assert_eq!(payout.toll, case.toll);
    assert_eq!(payout.net, case.net);
    assert_eq!(payout.toll + payout.net, case.paid_gross);
}

#[test]
fn entry_fee_and_jackpot_fee_vectors_match_the_accepted_rates() {
    let entry = fees::entry_fee(1_000_000_000).unwrap();
    assert_eq!(entry.entry_fee, 40_000_000);
    assert_eq!(entry.pot_credit, 960_000_000);
    assert_eq!(fees::jackpot_fee_bps(15).unwrap(), 900);
    assert_eq!(fees::jackpot_fee_bps(30).unwrap(), 1_300);
    assert_eq!(
        fees::jackpot_fee_bps(14),
        Err(fees::MathError::InvalidCompoundDuration)
    );
    assert!(!fees::can_open_backer_with_remaining(10));
    assert!(fees::can_open_backer_with_remaining(11));
    assert!(!fees::funded_dust_is_claimable(0));
    assert!(fees::funded_dust_is_claimable(1));
}

#[test]
fn equality_with_pot_is_solvent_while_zero_pot_positive_claim_is_allowed_at_zero_minimum() {
    let solvent = earnings::Payout::from_gross(10, 10, 1_200, 9).unwrap();
    assert!(!solvent.insolvent);
    assert_eq!(solvent.paid_gross, 10);
    let empty = earnings::Payout::from_gross(10, 0, 1_200, 0).unwrap();
    assert!(empty.insolvent);
    assert_eq!(empty.net, 0);
    assert_eq!(
        earnings::Payout::from_gross(10, 0, 1_200, 1),
        Err(earnings::MathError::MinimumReceiveNotMet)
    );
}

#[test]
fn simple_remainder_carries_across_claims_and_backwards_time_leaves_state_unchanged() {
    let mut accrual = earnings::SimpleAccrual::new(11, 0).unwrap();
    assert_eq!(accrual.accrue(86_400).unwrap(), 1);
    let snapshot = accrual;
    assert_eq!(
        accrual.accrue(86_399),
        Err(earnings::MathError::BackwardsTime)
    );
    assert_eq!(accrual, snapshot);
    assert_eq!(accrual.accrue(21 * 86_400).unwrap(), 24);
    assert_eq!(accrual.accrue(22 * 86_400).unwrap(), 0);
}

#[test]
fn boundaries_and_checked_widths_are_explicit() {
    assert_eq!(fees::simple_toll_bps(7 * 86_400 - 1), 1_200);
    assert_eq!(fees::simple_toll_bps(7 * 86_400), 750);
    assert_eq!(fees::simple_toll_bps(7 * 86_400 + 1), 750);
    assert_eq!(fees::simple_toll_bps(14 * 86_400 - 1), 750);
    assert_eq!(fees::simple_toll_bps(14 * 86_400), 300);
    assert_eq!(fees::simple_toll_bps(14 * 86_400 + 1), 300);
    assert_eq!(
        fees::series_a_entitlement(u64::MAX).unwrap(),
        22_873_962_651_399_844_002
    );
    assert_eq!(
        fees::series_b_entitlement(u128::MAX),
        Err(fees::MathError::Overflow)
    );
    assert_eq!(
        fees::prospective_series_b_entitlement(u128::MAX, 0, 0),
        Err(fees::MathError::Overflow)
    );
    assert_eq!(
        fees::front_end_load(u64::MAX).unwrap().pot_credit,
        17_708_874_310_761_169_551
    );
    let extreme = earnings::Payout::from_gross(u128::MAX, 1, 0, 1).unwrap();
    assert_eq!(extreme.paid_gross, 1);
    assert_eq!(extreme.net, 1);
}

#[test]
fn malformed_public_simple_state_is_rejected_without_mutation() {
    let malformed = earnings::SimpleAccrual {
        principal: 1_000_000_000,
        last_accrued_at: 100,
        expires_at: 99,
        remainder: 0,
        accrued_gross: 0,
    };
    let mut state = malformed;
    assert_eq!(state.accrue(100), Err(earnings::MathError::InvalidState));
    assert_eq!(state, malformed);

    let malformed_remainder = earnings::SimpleAccrual {
        expires_at: 200,
        remainder: earnings::SIMPLE_DENOMINATOR,
        ..malformed
    };
    let mut state = malformed_remainder;
    assert_eq!(
        state.accrue(100),
        Err(earnings::MathError::InvalidRemainder)
    );
    assert_eq!(state, malformed_remainder);
}

#[test]
fn malformed_public_compound_schedule_is_rejected_without_mutation() {
    let malformed = earnings::Plan {
        principal: 1_000_000_000,
        opened_at: 100,
        maturity_at: 99,
        kind: earnings::PlanKind::Compound30,
    };
    assert_eq!(
        malformed.owed_at(u64::MAX),
        Err(earnings::MathError::InvalidState)
    );
}

#[test]
fn simple_maturity_accumulates_exactly_once_across_repeated_claims() {
    let mut accrual = earnings::SimpleAccrual::new(1_000_000_000, 0).unwrap();
    let first = accrual.accrue(86_400).unwrap();
    let second = accrual.accrue(21 * 86_400).unwrap();
    assert_eq!(first + second, 2_310_000_000);
    assert_eq!(accrual.accrue(21 * 86_400).unwrap(), 0);
    assert_eq!(accrual.accrued_gross, 2_310_000_000);
}

#[test]
fn extreme_principal_uses_checked_wide_intermediates() {
    let mut accrual = earnings::SimpleAccrual::new(u64::MAX, 0).unwrap();
    assert!(accrual.accrue(21 * 86_400).is_ok());
    assert!(earnings::Plan::compound_30(u64::MAX, 0)
        .unwrap()
        .owed_at(30 * 86_400)
        .is_ok());
}
