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
/// A1's solvent Simple withdrawal: 4.0 days after the planned open, inside the 12 % tier (< 7 d).
/// Revised 2026-10-31 from `O1 + 596_160` (6.9 d) so that steps 10-12 need no wait.  It is only a
/// free-choice offset: the operator guard sends F3 inside the actual-elapsed window
/// `F3_MIN_ELAPSED..F3_MAX_ELAPSED` (cluster clock minus A1's on-chain `opened_at`), and the tests prove
/// every later check of the plan holds throughout that window.
pub const F3_AT: u64 = O1 + 345_600;
/// The smallest ACTUAL elapsed (cluster clock minus A1's on-chain `opened_at`) at which the operator
/// may send F3: 4.0 d less a 1 h allowance for a late open.  The arithmetic (`tests/m14_flow_plan.rs`)
/// proves every later check holds from here on; the computed minimum
/// (`f3_earliest_viable_elapsed`) is lower, the difference being the safety margin.  Below it the
/// round 1 Backer credit would be too small for A1's Series A row to reach its 124 % cap at the
/// Compound15 ticket (F6).
pub const F3_MIN_ELAPSED: u64 = 342_000;
/// Exclusive upper bound of the F3 window: 10 minutes before the 12 % tier ends at 7 days (the plan
/// models the 12 % tier; at or after the boundary the toll would be 7.5 %).
pub const F3_MAX_ELAPSED: u64 = 7 * DAY - 600;
/// Additive label of this revision of the plan's timing; `PLAN_NAME` (in the operator tool) stays
/// the same so that receipts written under an earlier revision still resume.
pub const PLAN_REVISION: &str = "2026-10-31 F3 at 4.0 d";
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
        // ---- day 6.0: solvent Simple payout inside the 12 % tier, then the A-only claim ----
        step("a1_withdraw_simple", "F3", F3_AT, A1, WithdrawSimple { nonce: 1, closing: false }, 0,
            "PayoutPaid, solvent: owed 0.44 of pot 1.0292, toll 12 % = 0.0528, net 0.3872; the toll rides ticket 1"),
        step("advance_ticket_f3", "F3", F3_AT + 30, Keeper, AdvanceTicket { caps: None }, 0,
            "ticket Discover..Commit; TicketCommitted: A1 credited 0.0264 (the Backer half), seed 0.0264"),
        step("a1_claim_a_only", "F5a", F3_AT + 90, A1, Claim { a: true, b: false }, 0,
            "BackerClaimed a_part 0.0264, b_part 0 (A alone, before any promotion)"),
        // ---- day 11.45: the strictly insolvent, pot-capped payout closes round 1 ----
        step("a1_closing_withdraw", "F4,F8", CLOSE1_AT, A1, WithdrawSimple { nonce: 1, closing: true }, 0,
            "strict insolvency: owed 0.5995 > pot 0.5892; PayoutPaid closing, gross_paid = pot (partial), toll 7.5 %; RoundClosed, deadline = now + 600"),
        step("advance_ticket_close1", "F8", CLOSE1_AT + 30, Keeper, AdvanceTicket { caps: None }, 0,
            "closing ticket 2 commits; CloseRecord.cash_committed; A1 credited 0.022095"),
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
            "ONE tx by the plain wallet: finalize_round + participant page + Compound30 open; round 2, seed 0.048495 carried"),
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
            "solvent: owed 0.89471 of pot 1.031695 (15 % spare); Jackpot Fee 9 % = 0.08052, net 0.81419; ticket 3"),
        step("advance_ticket_c15", "F6", C15_AT + 30, Keeper, AdvanceTicket { caps: Some(Who::A1) }, 0,
            "ticket 3 commits; A1's Series A reaches 124 % (remaining 0.013505 -> 0) and leaves the registry"),
        step("winner_claim_a_and_b", "F5b", C15_AT + 90, Winner, Claim { a: true, b: true }, 0,
            "BackerClaimed in ONE instruction: a_part 0.00906, b_part 0.00805"),
        step("a1_claim_after_cap", "F6", C15_AT + 150, A1, Claim { a: true, b: false }, 0,
            "a capped row can still claim its funded credit (0.0356)"),
        // ---- day 41.45: the Compound30 settlement is insolvent: close 2, expiry path ----
        step("w_settle_compound30", "F7b,F4", C30_AT, W, SettleCompound { nonce: 1, closing: true }, 0,
            "strict insolvency: owed 7.3606 vs pot 0.13698 (54x); gross_paid = pot, toll 13 %; deadline = now + 600"),
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
    /// Seconds after A1's open at which F3 (A1's solvent Simple withdrawal) is modelled.
    pub f3_elapsed: u64,
    /// Seconds after A1's open at which close 1 (A1's strictly insolvent withdrawal) is modelled.
    pub close1_elapsed: u64,
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

/// Turns a model error into the `String` error of `arithmetic_at`, naming what was being computed.
fn model<T, E: std::fmt::Debug>(what: &str, result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{what}: {error:?}"))
}

/// The plan's arithmetic at its own F3 and close offsets.
pub fn arithmetic() -> Arithmetic {
    arithmetic_at(F3_AT - O1, CLOSE1_AT - O1).expect("the plan's arithmetic")
}

/// The plan's arithmetic with F3 at `O1 + f3_elapsed` and close 1 at `O1 + close1_elapsed` (the round 2
/// offsets are the plan's).  `Err` names the first plan condition that does not hold: F3 solvent and in
/// the 12 % tier; close 1 strictly insolvent, in the 7.5 % tier and emptying the pot; A1 still open
/// after round 1; A1's Series A row capped by the Compound15 ticket (F6); A1 a loser; Compound15
/// solvent with a 10 % margin; Compound30 strictly insolvent; the winner's combined claim has both parts.
pub fn arithmetic_at(f3_elapsed: u64, close1_elapsed: u64) -> Result<Arithmetic, String> {
    const A1: u64 = 1;
    const A2: u64 = 2;
    const X: u64 = 3;
    if f3_elapsed >= close1_elapsed {
        return Err(format!("F3 (elapsed {f3_elapsed}) must come before close 1 (elapsed {close1_elapsed})"));
    }
    let mut book = Book::with_pot(100 * SOL);
    let entitlement = |gross: u64| model("entitlement", fees::series_a_entitlement(gross));

    // ---- round 1 ----
    let mut pot = A1_SERIES_A;
    model("A1 row", book.add_backer(A1, Series::A, entitlement(A1_SERIES_A)?))?;
    for gross in [A1_GAME, CANDIDATE_GAME, CANDIDATE_GAME] {
        pot += front_end_load_credit(gross);
    }
    let r1_pot_before_f3 = pot;

    let f3_toll_bps = fees::simple_toll_bps(f3_elapsed);
    let mut accrual = model("accrual", SimpleAccrual::new(A1_GAME, O1))?;
    model("accrue to F3", accrual.accrue(O1 + f3_elapsed))?;
    let f3 = model("F3", fees::payout(accrual.accrued_gross, pot, f3_toll_bps))?;
    if f3.insolvent {
        return Err(format!("F3 must be solvent: owed {} against pot {pot} at elapsed {f3_elapsed}", f3.gross_owed));
    }
    if f3_toll_bps != 1_200 {
        return Err(format!("F3 must be in the 12 % tier: elapsed {f3_elapsed} pays {f3_toll_bps} bps"));
    }
    pot -= f3.paid_gross;
    model("F3 ticket", book.allocate(f3.toll, 8))?;

    model("accrue to the close", accrual.accrue(O1 + close1_elapsed))?;
    let owed = accrual.accrued_gross - u128::from(f3.paid_gross);
    let close1_toll_bps = fees::simple_toll_bps(close1_elapsed);
    let close1 = model("close 1", fees::payout(owed, pot, close1_toll_bps))?;
    let close1_owed_minus_pot = owed.saturating_sub(u128::from(pot));
    if !close1.insolvent {
        return Err(format!("close 1 must be strictly insolvent: owed {owed} against pot {pot} at elapsed {close1_elapsed}"));
    }
    if close1_toll_bps != 750 {
        return Err(format!("close 1 must be in the 7.5 % tier: elapsed {close1_elapsed} pays {close1_toll_bps} bps"));
    }
    pot -= close1.paid_gross;
    if pot != 0 {
        return Err(format!("a pot-capped payout empties the pot, but {pot} is left after close 1"));
    }
    model("close 1 ticket", book.allocate(close1.toll, 8))?;
    let a1_row = model("A1 row", book.backer(BackerKey::new(A1, Series::A)))?;
    let a1_remaining_after_r1 = model("A1 remaining", a1_row.remaining_entitlement())?;
    if a1_remaining_after_r1 <= 10 {
        return Err(format!("A1 must still be open after round 1: remaining {a1_remaining_after_r1}"));
    }
    let a1_net_total = f3.net + close1.net;

    // First elapsed second at which A1's owed (after F3's payment) exceeds the pot A1
    // saw after F3: a bisection over the accrual, from F3 to the close.
    let pot_after_f3 = r1_pot_before_f3 - f3.paid_gross;
    let owed_at = |elapsed: u64| -> Result<u128, String> {
        let mut copy = model("accrual", SimpleAccrual::new(A1_GAME, 0))?;
        model("accrue", copy.accrue(elapsed))?;
        Ok(copy.accrued_gross.saturating_sub(u128::from(f3.paid_gross)))
    };
    let insolvent_from_elapsed = {
        let (mut low, mut high) = (f3_elapsed, close1_elapsed);
        if owed_at(high)? <= u128::from(pot_after_f3) {
            return Err(format!("A1's owed at elapsed {high} does not exceed the pot {pot_after_f3} left after F3"));
        }
        while low + 1 < high {
            let mid = (low + high) / 2;
            if owed_at(mid)? > u128::from(pot_after_f3) {
                high = mid;
            } else {
                low = mid;
            }
        }
        high
    };

    // ---- promotion: the winner (P4 or P5; identical) holds a 116 % B row ----
    let b_entitlement = u128::from(CANDIDATE_GAME) * 11_600 / 10_000;
    model("B row", book.add_backer(X, Series::B, b_entitlement))?;
    let seed_carried = book.seed();
    book.seed = 0;

    // ---- round 2 ----
    pot += seed_carried;
    for gross in [W_COMPOUND30, P1_COMPOUND15, P2_SIMPLE, P3_SIMPLE] {
        pot += front_end_load_credit(gross);
    }
    model("A2 row", book.add_backer(A2, Series::A, entitlement(A2_SERIES_A)?))?;
    pot += A2_SERIES_A;
    model("winner A row", book.add_backer(X, Series::A, entitlement(WINNER_SERIES_A)?))?;
    pot += WINNER_SERIES_A;
    let r2_pot_at_c15 = pot;
    let c15_plan = model("C15 plan", Plan::compound_15(P1_COMPOUND15, P1_OPEN))?;
    let c15_owed = model("C15 owed", c15_plan.owed_at(C15_AT))?;
    let c15 = model("C15", fees::payout(c15_owed, pot, 900))?;
    pot -= c15.paid_gross;
    model("C15 ticket", book.allocate(c15.toll, 8))?;
    let a1_after = model("A1 row after C15", book.backer(BackerKey::new(A1, Series::A)))?;
    let a1_remaining_after_c15 = model("A1 remaining after C15", a1_after.remaining_entitlement())?;
    let a1_still_open_after_c15 = book.is_active(BackerKey::new(A1, Series::A));
    if !(a1_remaining_after_c15 <= 10 && !a1_still_open_after_c15) {
        return Err(format!(
            "F6 fails: A1's Series A row must reach 124 % at the Compound15 ticket, but {a1_remaining_after_c15} is still remaining after it (open: {a1_still_open_after_c15}); round 1 credited it too little at F3 elapsed {f3_elapsed}"
        ));
    }
    if a1_net_total >= A1_GAME {
        return Err(format!("A1 must be a loser (a fallback candidate): net payouts {a1_net_total} of {A1_GAME}"));
    }
    if c15.insolvent || u128::from(r2_pot_at_c15) < c15.gross_owed + c15.gross_owed / 10 {
        return Err(format!(
            "C15 must be solvent with a 10 % margin: pot {r2_pot_at_c15} against owed {}",
            c15.gross_owed
        ));
    }
    let winner_a = model("winner A row", book.backer(BackerKey::new(X, Series::A)))?;
    let winner_a_pending = u128::from(winner_a.funded) + u128::from(winner_a.pending);
    let winner_b = model("winner B row", book.backer(BackerKey::new(X, Series::B)))?;
    let winner_b_pending = u128::from(winner_b.funded) + u128::from(winner_b.pending);

    let r2_pot_at_c30 = pot;
    let c30_plan = model("C30 plan", Plan::compound_30(W_COMPOUND30, R2O))?;
    let c30_owed = model("C30 owed", c30_plan.owed_at(C30_AT))?;
    let c30 = model("C30", fees::payout(c30_owed, pot, 1_300))?;
    if !c30.insolvent {
        return Err(format!("C30 must be strictly insolvent: owed {} against pot {pot}", c30.gross_owed));
    }
    if winner_a_pending == 0 || winner_b_pending == 0 {
        return Err(format!(
            "the winner's combined claim needs both parts: A {winner_a_pending}, B {winner_b_pending}"
        ));
    }

    Ok(Arithmetic {
        f3_elapsed,
        close1_elapsed,
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
    })
}

/// The smallest F3 elapsed (seconds after A1's open) in `0..F3_MAX_ELAPSED` for which every later check
/// of the plan holds with close 1 at `close1_elapsed`.  Only F6 fails below it (a smaller toll credits A1's
/// Series A row less, so more entitlement is left at the Compound15 ticket), so viability is monotone and
/// a bisection finds it.  Panics if even the last second of the window is not viable.
pub fn f3_earliest_viable_elapsed(close1_elapsed: u64) -> u64 {
    let viable = |f3_elapsed: u64| arithmetic_at(f3_elapsed, close1_elapsed).is_ok();
    let mut high = F3_MAX_ELAPSED - 1;
    if let Err(error) = arithmetic_at(high, close1_elapsed) {
        panic!("no F3 elapsed in the window is viable (the last second fails): {error}");
    }
    if viable(0) {
        return 0;
    }
    let mut low = 0;
    while low + 1 < high {
        let mid = (low + high) / 2;
        if viable(mid) {
            high = mid;
        } else {
            low = mid;
        }
    }
    high
}
