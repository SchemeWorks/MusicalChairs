//! M14 flow plan data (packet RG-7): the Seq-B devnet acceptance flows with concrete amounts, as DATA.
//!
//! This is the single source of the plan.  It is used by BOTH the clock-warped LiteSVM rehearsal
//! (`tests/m14_flow_plan.rs`, documented in `docs/solana/m14-flow-plan.md`) and the operator tool
//! (`devnet_ops flow`, `src/devnet_ops/flow.rs`), so the rehearsed plan and the plan that is run
//! against a cluster cannot drift apart.  Offsets are seconds from genesis, where genesis is the moment
//! the G approvals are given: the first transaction is at 172,800 s because `initialize` and the ORAO
//! `register` cannot execute earlier.
use musical_chairs_economics::{
    backers::{BackerKey, Book, Series},
    earnings::Plan,
    earnings::SimpleAccrual,
    fees,
};
use musical_chairs_program::positions::PlanKindArg;

use crate::world::DAY;

pub const SOL: u64 = 1_000_000_000;

// ---------------------------------------------------------------------------
// The plan: wallets, amounts and the schedule (all offsets in seconds from genesis,
// where genesis is the moment the G approvals are given)
// ---------------------------------------------------------------------------

/// G's governance delay: the earliest instant `initialize` (and the ORAO register) can
/// execute, so the earliest instant of any flow (packet section 6.4, "day 2.0").
pub const GOVERNANCE_DELAY: u64 = 172_800;
pub const T0: u64 = GOVERNANCE_DELAY;

// Amounts (lamports).
/// Operations funding (packet bucket S: 0.5 SOL "covers two closes").
pub const OPS_FUNDING: u64 = 500_000_000;
/// A1's Series A funding: the program minimum (`min_a`), entitlement 62,000,000.
pub const A1_SERIES_A: u64 = 50_000_000;
/// A1's dominant Simple position: the packet's per-deposit cap.
pub const A1_GAME: u64 = SOL;
/// Each round 1 candidate (P4, P5): the program minimum (`min_game`).
pub const CANDIDATE_GAME: u64 = 10_000_000;
/// Round 2: the plain wallet's Compound30 (opened by the composed finalize/reopen).
pub const W_COMPOUND30: u64 = 600_000_000;
/// Round 2: P1's Compound15.
pub const P1_COMPOUND15: u64 = 200_000_000;
/// Round 2: P2's and P3's Simple positions (extra pot, extra candidates).
pub const P2_SIMPLE: u64 = 100_000_000;
pub const P3_SIMPLE: u64 = 20_000_000;
/// Round 2: the second Backer's and the round 1 promotion winner's Series A.
pub const A2_SERIES_A: u64 = 50_000_000;
pub const WINNER_SERIES_A: u64 = 50_000_000;

// Round 1 timeline.
/// A1 opens the dominant Simple position (`opened_at`).
pub const O1: u64 = T0 + 240;
/// A1's solvent Simple withdrawal: 6.9 days after open, inside the 12 % tier (< 7 d).
pub const F3_AT: u64 = O1 + 596_160;
/// A1's insolvent (closing) withdrawal: 9.45 days after open, inside the 7.5 % tier.
pub const CLOSE1_AT: u64 = O1 + 816_480;
/// The composed finalize/reopen by the plain wallet: round 2 opens (and W's Compound30
/// is pinned) here.
pub const R2O: u64 = CLOSE1_AT + 180;
// Round 2 timeline.
pub const P1_OPEN: u64 = R2O + 60;
/// P1's Compound15 settles 60 s after maturity (1,296,000 s from its open).
pub const C15_AT: u64 = P1_OPEN + 1_296_060;
/// W's Compound30 settles 60 s after maturity (2,592,000 s from R2O).
pub const C30_AT: u64 = R2O + 2_592_060;
/// The promotion deadline is pinned once at `close_started_at + 600`.
pub const DEADLINE_SECS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Who {
    A1,
    A2,
    P1,
    P2,
    P3,
    P4,
    P5,
    /// The plain wallet that submits the composed finalize/reopen transaction.
    W,
    /// Pays shared storage and funds operations.
    Sponsor,
    /// The permissionless keeper fee payer (candidates, draw, tickets, expiry).
    Keeper,
    /// Whichever of P4 / P5 the round 1 draw promotes (resolved at run time).
    Winner,
}

impl Who {
    pub fn name(self) -> &'static str {
        match self {
            Who::A1 => "A1",
            Who::A2 => "A2",
            Who::P1 => "P1",
            Who::P2 => "P2",
            Who::P3 => "P3",
            Who::P4 => "P4",
            Who::P5 => "P5",
            Who::W => "W",
            Who::Sponsor => "S",
            Who::Keeper => "K",
            Who::Winner => "Winner",
        }
    }

    /// In-test keypair seed (never a real key).
    pub fn seed(self) -> u8 {
        match self {
            Who::A1 => 0xE1,
            Who::A2 => 0xE2,
            Who::P1 => 0xF1,
            Who::P2 => 0xF2,
            Who::P3 => 0xF3,
            Who::P4 => 0xF4,
            Who::P5 => 0xF5,
            Who::W => 0xE9,
            Who::Keeper => 0xEA,
            Who::Sponsor | Who::Winner => unreachable!("{self:?} has no seed of its own"),
        }
    }
}

/// What a step does.  Everything is a real instruction of the program (or of ORAO for
/// the fulfilment) executed on the SBF.
#[derive(Clone, Copy, Debug)]
pub enum Act {
    FundOperations,
    ProvisionRegistry,
    ProvisionSnapshot,
    ProvisionParticipants { round: u64 },
    ProvisionCloseRecord { close_id: u64 },
    FundSeriesA,
    Open { plan: PlanKindArg, nonce: u64 },
    /// `closing`: the payout must be the strictly insolvent, pot-capped one.
    WithdrawSimple { nonce: u64, closing: bool },
    SettleCompound { nonce: u64, closing: bool },
    /// Run the active allocation ticket to its commit; `caps` names a Backer whose
    /// Series A row must reach 124 % (and leave the registry) in this ticket.
    AdvanceTicket { caps: Option<Who> },
    /// `a` / `b`: which parts of the combined claim must be positive.
    Claim { a: bool, b: bool },
    CandidatePages { close_id: u64 },
    BuildCandidates { close_id: u64 },
    RequestDraw { close_id: u64 },
    Fulfil { close_id: u64 },
    CaptureDraw { close_id: u64 },
    ApplyPromotion { close_id: u64 },
    /// `finalize_round` + `provision_page(Participant)` + `open_position` in ONE
    /// transaction signed by the plain wallet.
    ComposedReopen { close_id: u64, plan: PlanKindArg, nonce: u64 },
    /// `expire_promotion` AT the deadline: refused (`now > deadline` is strict).
    ExpireTooEarly { close_id: u64 },
    ExpirePromotion { close_id: u64 },
    FinalizeRound { close_id: u64 },
}

#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub id: &'static str,
    /// Flow codes this step realizes (comma separated; see `REQUIRED_FLOWS`).
    pub flows: &'static str,
    /// Seconds from genesis.
    pub at: u64,
    pub who: Who,
    pub act: Act,
    /// Lamports for deposits / Series A / operations funding; 0 otherwise.
    pub amount: u64,
    /// The expected outcome, as it appears in the plan table of the doc.
    pub expect: &'static str,
}

/// The flows of the plan's M14 list (docs/superpowers/plans/2026-10-13-native-solana-
/// implementation.md, M14), in the order the doc names them.
pub const REQUIRED_FLOWS: [&str; 15] = [
    "F1", "F2", "F3", "F4", "F5a", "F5b", "F6", "F7a", "F7b", "F8", "F8r", "F8c", "F9a", "F9b",
    "F10",
];

pub fn step(
    id: &'static str,
    flows: &'static str,
    at: u64,
    who: Who,
    act: Act,
    amount: u64,
    expect: &'static str,
) -> Step {
    Step { id, flows, at, who, act, amount, expect }
}

/// The Seq-B plan with concrete amounts.  Ordered by `at`.
pub fn plan() -> Vec<Step> {
    use Act::*;
    use Who::*;
    let simple = PlanKindArg::Simple;
    vec![
        // ---- day 2.0: first instant after G's delay; shared storage and operations ----
        step("fund_operations", "setup", T0, Sponsor, FundOperations, OPS_FUNDING,
            "Ops.free_balance += 0.5 SOL; OperationsFunded"),
        step("provision_registry_0", "setup", T0 + 30, Keeper, ProvisionRegistry, 0,
            "registry_pages 1; rent from Ops.free_balance"),
        step("provision_snapshot_0", "setup", T0 + 60, Keeper, ProvisionSnapshot, 0,
            "snapshot_pages 1"),
        step("provision_participants_r1_0", "setup", T0 + 90, Keeper,
            ProvisionParticipants { round: 1 }, 0, "participant_pages 1 (round 1)"),
        step("provision_close_record_1", "setup", T0 + 120, Keeper,
            ProvisionCloseRecord { close_id: 1 }, 0, "CloseRecord(1) exists"),
        // ---- round 1: a dominant Simple position, two minimum candidates, one Backer ----
        step("a1_fund_series_a", "F1", T0 + 180, A1, FundSeriesA, A1_SERIES_A,
            "SeriesAFunded: pot += 0.05 in full (no load), entitlement 0.062, registry_len 1"),
        step("a1_open_simple", "F2", O1, A1, Open { plan: simple, nonce: 1 }, A1_GAME,
            "PositionOpened: Front-End Load 4 % = 0.04, pot += 0.96; round 1 pot is 1.0292 with P4 and P5"),
        step("p4_open_simple", "F2", T0 + 300, P4, Open { plan: simple, nonce: 1 }, CANDIDATE_GAME,
            "PositionOpened (candidate-to-be)"),
        step("p5_open_simple", "F2", T0 + 360, P5, Open { plan: simple, nonce: 1 }, CANDIDATE_GAME,
            "PositionOpened (candidate-to-be)"),
        // ---- day 8.9: solvent Simple payout inside the 12 % tier, then the A-only claim ----
        step("a1_withdraw_simple", "F3", F3_AT, A1, WithdrawSimple { nonce: 1, closing: false }, 0,
            "PayoutPaid, solvent: owed 0.759 of pot 1.0292, toll 12 % = 0.09108, net 0.66792; the toll rides ticket 1"),
        step("advance_ticket_f3", "F3", F3_AT + 30, Keeper, AdvanceTicket { caps: None }, 0,
            "ticket Discover..Commit; TicketCommitted: A1 credited 0.04554 (the Backer half), seed 0.04554"),
        step("a1_claim_a_only", "F5a", F3_AT + 90, A1, Claim { a: true, b: false }, 0,
            "BackerClaimed a_part 0.04554, b_part 0 (A alone, before any promotion)"),
        // ---- day 11.45: the strictly insolvent, pot-capped payout closes round 1 ----
        step("a1_closing_withdraw", "F4,F8", CLOSE1_AT, A1, WithdrawSimple { nonce: 1, closing: true }, 0,
            "strict insolvency: owed 0.2805 > pot 0.2702; PayoutPaid closing, gross_paid = pot (partial), toll 7.5 %; RoundClosed, deadline = now + 600"),
        step("advance_ticket_close1", "F8", CLOSE1_AT + 30, Keeper, AdvanceTicket { caps: None }, 0,
            "closing ticket 2 commits; CloseRecord.cash_committed; A1 credited 0.0101325"),
        step("candidate_pages_1", "F8", CLOSE1_AT + 60, Keeper, CandidatePages { close_id: 1 }, 0,
            "1 preferred + 1 fallback page from the locked reserve"),
        step("build_candidates_1", "F8", CLOSE1_AT + 90, Keeper, BuildCandidates { close_id: 1 }, 0,
            "CandidatesSealed: preferred set {P4, P5}, count 2 (A1 is fallback-only: it holds Series A history)"),
        step("request_draw_1", "F8r", CLOSE1_AT + 120, Keeper, RequestDraw { close_id: 1 }, 0,
            "DrawRequested: fee 300,000 lamports + request rent, paid from the locked reserve"),
        step("fulfil_1", "F8c", CLOSE1_AT + 135, Keeper, Fulfil { close_id: 1 }, 0,
            "ORAO request fulfilled by the synthetic authority (no cluster latency in LiteSVM)"),
        step("capture_draw_1", "F8c", CLOSE1_AT + 150, Keeper, CaptureDraw { close_id: 1 }, 0,
            "DrawCaptured inside the 600 s deadline"),
        step("apply_promotion_1", "F9a", CLOSE1_AT + 165, Keeper, ApplyPromotion { close_id: 1 }, 0,
            "PromotionApplied: winner in {P4, P5}, loss 0.01, Series B entitlement 0.0116; B row joins the registry"),
        step("w_finalize_reopen", "F9b", R2O, W, ComposedReopen { close_id: 1, plan: PlanKindArg::Compound30, nonce: 1 }, W_COMPOUND30,
            "ONE tx by the plain wallet: finalize_round + participant page + Compound30 open; round 2, seed 0.0556725 carried"),
        // ---- round 2: positions, two more Series A rows, then the compounding settlements ----
        step("p1_open_compound15", "F2", P1_OPEN, P1, Open { plan: PlanKindArg::Compound15, nonce: 1 }, P1_COMPOUND15,
            "PositionOpened; maturity +15 d"),
        step("p2_open_simple", "F2", R2O + 120, P2, Open { plan: simple, nonce: 1 }, P2_SIMPLE,
            "PositionOpened"),
        step("p3_open_simple", "F2", R2O + 180, P3, Open { plan: simple, nonce: 1 }, P3_SIMPLE,
            "PositionOpened"),
        step("a2_fund_series_a", "F1", R2O + 240, A2, FundSeriesA, A2_SERIES_A,
            "SeriesAFunded; registry_len +1"),
        step("winner_fund_series_a", "F1", R2O + 300, Winner, FundSeriesA, WINNER_SERIES_A,
            "SeriesAFunded: the promoted wallet now holds A and B"),
        step("provision_close_record_2", "setup", R2O + 360, Keeper,
            ProvisionCloseRecord { close_id: 2 }, 0, "CloseRecord(2) exists"),
        step("p1_settle_compound15", "F7a", C15_AT, P1, SettleCompound { nonce: 1, closing: false }, 0,
            "solvent: owed 0.89471 of pot 1.03887 (16 % spare); Jackpot Fee 9 % = 0.08052, net 0.81419; ticket 3"),
        step("advance_ticket_c15", "F6", C15_AT + 30, Keeper, AdvanceTicket { caps: Some(Who::A1) }, 0,
            "ticket 3 commits; A1's Series A reaches 124 % (remaining 0.0063275 -> 0) and leaves the registry"),
        step("winner_claim_a_and_b", "F5b", C15_AT + 90, Winner, Claim { a: true, b: true }, 0,
            "BackerClaimed in ONE instruction: a_part 0.00906, b_part 0.00805"),
        step("a1_claim_after_cap", "F6", C15_AT + 150, A1, Claim { a: true, b: false }, 0,
            "a capped row can still claim its funded credit (0.01646)"),
        // ---- day 41.45: the Compound30 settlement is insolvent: close 2, expiry path ----
        step("w_settle_compound30", "F7b,F4", C30_AT, W, SettleCompound { nonce: 1, closing: true }, 0,
            "strict insolvency: owed 7.3606 vs pot 0.14416 (51x); gross_paid = pot, toll 13 %; deadline = now + 600"),
        step("advance_ticket_close2", "F10", C30_AT + 30, Keeper, AdvanceTicket { caps: None }, 0,
            "closing ticket 4 commits"),
        step("candidate_pages_2", "F10", C30_AT + 60, Keeper, CandidatePages { close_id: 2 }, 0,
            "1 + 1 pages"),
        step("build_candidates_2", "F10", C30_AT + 90, Keeper, BuildCandidates { close_id: 2 }, 0,
            "CandidatesSealed: preferred {W, P2, P3}, count 3"),
        step("request_draw_2", "F10", C30_AT + 120, Keeper, RequestDraw { close_id: 2 }, 0,
            "DrawRequested; the request is never fulfilled"),
        step("expire_at_deadline", "F10", C30_AT + DEADLINE_SECS, Keeper, ExpireTooEarly { close_id: 2 }, 0,
            "refused DeadlineNotReached at now == deadline (fee only)"),
        step("expire_promotion_2", "F10", C30_AT + DEADLINE_SECS + 1, Keeper, ExpirePromotion { close_id: 2 }, 0,
            "PromotionSkipped(Deadline, requested-unfulfilled)"),
        step("finalize_2", "F10", C30_AT + DEADLINE_SECS + 11, Keeper, FinalizeRound { close_id: 2 }, 0,
            "RoundFinalized: round 3"),
    ]
}

/// Per-wallet starting balance (what the test funds, as faucet-style transfers of at
/// most 2 SOL each), sized for the LiteSVM rent schedule.
pub fn wallet_funding(who: Who) -> u64 {
    match who {
        Who::A1 => 1_100_000_000,
        Who::A2 => 60_000_000,
        Who::P1 => 210_000_000,
        Who::P2 => 110_000_000,
        Who::P3 => 30_000_000,
        Who::P4 | Who::P5 => 80_000_000,
        Who::W => 630_000_000,
        Who::Sponsor => 550_000_000,
        Who::Keeper => 50_000_000,
        Who::Winner => unreachable!("Winner is P4 or P5"),
    }
}

pub const ALL_WALLETS: [Who; 10] = [
    Who::A1,
    Who::A2,
    Who::P1,
    Who::P2,
    Who::P3,
    Who::P4,
    Who::P5,
    Who::W,
    Who::Sponsor,
    Who::Keeper,
];

pub fn day(at: u64) -> f64 {
    at as f64 / DAY as f64
}

// ---------------------------------------------------------------------------
// Pure arithmetic: the plan's solvency, from the host oracles
// ---------------------------------------------------------------------------

/// The numbers the plan depends on, from `musical-chairs-economics` (the host oracles
/// the program is differential-tested against) with no SBF involved.
#[derive(Debug)]
pub struct Arithmetic {
    pub r1_pot_before_f3: u64,
    pub f3: fees::FeePayout,
    pub close1: fees::FeePayout,
    pub close1_owed_minus_pot: u128,
    /// Seconds after A1's open at which its owed first exceeds the round 1 pot.
    pub insolvent_from_elapsed: u64,
    pub a1_net_total: u64,
    pub seed_carried: u64,
    pub r2_pot_at_c15: u64,
    pub c15: fees::FeePayout,
    pub a1_remaining_after_r1: u128,
    pub a1_remaining_after_c15: u128,
    pub a1_still_open_after_c15: bool,
    pub winner_a_pending: u128,
    pub winner_b_pending: u128,
    pub r2_pot_at_c30: u64,
    pub c30: fees::FeePayout,
}

pub fn front_end_load_credit(gross: u64) -> u64 {
    fees::front_end_load(gross).expect("load").pot_credit
}

pub fn arithmetic() -> Arithmetic {
    const A1: u64 = 1;
    const A2: u64 = 2;
    const X: u64 = 3;
    let mut book = Book::with_pot(100 * SOL);
    let entitlement = |gross: u64| fees::series_a_entitlement(gross).expect("entitlement");

    // ---- round 1 ----
    let mut pot = A1_SERIES_A;
    book.add_backer(A1, Series::A, entitlement(A1_SERIES_A)).expect("A1 row");
    for gross in [A1_GAME, CANDIDATE_GAME, CANDIDATE_GAME] {
        pot += front_end_load_credit(gross);
    }
    let r1_pot_before_f3 = pot;

    let mut accrual = SimpleAccrual::new(A1_GAME, O1).expect("accrual");
    accrual.accrue(F3_AT).expect("accrue to F3");
    let f3 = fees::payout(accrual.accrued_gross, pot, fees::simple_toll_bps(F3_AT - O1)).expect("F3");
    assert!(!f3.insolvent, "F3 must be solvent");
    pot -= f3.paid_gross;
    book.allocate(f3.toll, 8).expect("F3 ticket");

    accrual.accrue(CLOSE1_AT).expect("accrue to the close");
    let owed = accrual.accrued_gross - u128::from(f3.paid_gross);
    let close1 = fees::payout(owed, pot, fees::simple_toll_bps(CLOSE1_AT - O1)).expect("close 1");
    let close1_owed_minus_pot = owed.saturating_sub(u128::from(pot));
    assert!(close1.insolvent, "the close must be strictly insolvent");
    pot -= close1.paid_gross;
    assert_eq!(pot, 0, "a pot-capped payout empties the pot");
    book.allocate(close1.toll, 8).expect("close 1 ticket");
    let a1_row = book.backer(BackerKey::new(A1, Series::A)).expect("A1 row");
    let a1_remaining_after_r1 = a1_row.remaining_entitlement().expect("remaining");
    let a1_net_total = f3.net + close1.net;

    // First elapsed second at which A1's owed (after F3's payment) exceeds the pot A1
    // saw after F3: a bisection over the accrual.
    let pot_after_f3 = r1_pot_before_f3 - f3.paid_gross;
    let insolvent_from_elapsed = {
        let owed_at = |elapsed: u64| {
            let mut copy = SimpleAccrual::new(A1_GAME, 0).expect("accrual");
            copy.accrue(elapsed).expect("accrue");
            copy.accrued_gross - u128::from(f3.paid_gross)
        };
        let (mut low, mut high) = (F3_AT - O1, CLOSE1_AT - O1);
        assert!(owed_at(high) > u128::from(pot_after_f3));
        while low + 1 < high {
            let mid = (low + high) / 2;
            if owed_at(mid) > u128::from(pot_after_f3) {
                high = mid;
            } else {
                low = mid;
            }
        }
        high
    };

    // ---- promotion: the winner (P4 or P5; identical) holds a 116 % B row ----
    let b_entitlement = u128::from(CANDIDATE_GAME) * 11_600 / 10_000;
    book.add_backer(X, Series::B, b_entitlement).expect("B row");
    let seed_carried = book.seed();
    book.seed = 0;

    // ---- round 2 ----
    pot += seed_carried;
    for gross in [W_COMPOUND30, P1_COMPOUND15, P2_SIMPLE, P3_SIMPLE] {
        pot += front_end_load_credit(gross);
    }
    book.add_backer(A2, Series::A, entitlement(A2_SERIES_A)).expect("A2 row");
    pot += A2_SERIES_A;
    book.add_backer(X, Series::A, entitlement(WINNER_SERIES_A)).expect("winner A row");
    pot += WINNER_SERIES_A;
    let r2_pot_at_c15 = pot;
    let c15_owed = Plan::compound_15(P1_COMPOUND15, P1_OPEN)
        .expect("plan")
        .owed_at(C15_AT)
        .expect("matured");
    let c15 = fees::payout(c15_owed, pot, 900).expect("C15");
    assert!(!c15.insolvent, "C15 must be solvent");
    pot -= c15.paid_gross;
    book.allocate(c15.toll, 8).expect("C15 ticket");
    let a1_after = book.backer(BackerKey::new(A1, Series::A)).expect("A1 row");
    let a1_remaining_after_c15 = a1_after.remaining_entitlement().expect("remaining");
    let a1_still_open_after_c15 = book.is_active(BackerKey::new(A1, Series::A));
    let winner_a = book.backer(BackerKey::new(X, Series::A)).expect("winner A row");
    let winner_a_pending = u128::from(winner_a.funded) + u128::from(winner_a.pending);
    let winner_b = book.backer(BackerKey::new(X, Series::B)).expect("winner B row");
    let winner_b_pending = u128::from(winner_b.funded) + u128::from(winner_b.pending);

    let r2_pot_at_c30 = pot;
    let c30_owed = Plan::compound_30(W_COMPOUND30, R2O)
        .expect("plan")
        .owed_at(C30_AT)
        .expect("matured");
    let c30 = fees::payout(c30_owed, pot, 1_300).expect("C30");
    assert!(c30.insolvent, "C30 must be strictly insolvent");

    Arithmetic {
        r1_pot_before_f3,
        f3,
        close1,
        close1_owed_minus_pot,
        insolvent_from_elapsed,
        a1_net_total,
        seed_carried,
        r2_pot_at_c15,
        c15,
        a1_remaining_after_r1,
        a1_remaining_after_c15,
        a1_still_open_after_c15,
        winner_a_pending,
        winner_b_pending,
        r2_pot_at_c30,
        c30,
    }
}

