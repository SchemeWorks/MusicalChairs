//! M14-SIM (packet RG-7): the devnet acceptance flow list, rehearsed end to end in
//! LiteSVM against the production SBF ELF and the DEVNET ORAO executable, on the
//! packet's recommended schedule (Seq-B) with concrete amounts, a clock warped to every
//! step's offset, both conservation identities after EVERY step, and a per-wallet
//! lamport ledger (fees, rent, deposits, receipts).
//!
//! # What is real and what is synthetic
//!
//! * REAL: the program bytes (`MC_FINANCIAL_SBF`), the devnet ORAO bytes
//!   (`MC_ORAO_PROFILE=devnet`), every instruction of the plan, the Clock-dependent
//!   economics (Simple accrual, tolls by tier, Compound15/30 maturity, the strict
//!   insolvency test, the 600 s promotion deadline).
//! * SYNTHETIC: the cluster.  Time only moves where the plan warps it (one slot per
//!   warp, so slot figures are NOT cluster-realistic); ORAO is fulfilled by ONE
//!   synthetic authority at the instant the plan says; Squads is the test-only
//!   stand-in (the G delay is represented only by the plan's first offset,
//!   172,800 s); rent is LiteSVM's default schedule (the ledger also prints the
//!   devnet-priced rent, 5,080 lamports x (bytes + 128)); governance steps
//!   (`initialize`, ORAO `register`) are the harness' `provider_setup` and genesis
//!   fixture, not transactions of this plan.
//! * Production economics are unchanged: default `ConfigVersion` terms, 11 %/day
//!   Simple, Compound15/30, 4 % Front-End Load, Carried Interest tiers 12 / 7.5 / 3 %,
//!   Jackpot Fee 9 / 13 %, 124 % Series A, 116 % Series B.
//!
//! # Running
//!
//! The LiteSVM run is `#[ignore]` and refuses to run on the default (mainnet ORAO)
//! profile, like the other devnet tests:
//!
//! ```text
//! MC_ORAO_PROFILE=devnet cargo test --locked --test m14_flow_plan -- --ignored --nocapture
//! ```
//!
//! plus the usual `MC_FINANCIAL_SBF` / `MC_STANDIN_SBF` if the artifacts are not at
//! their default paths.  `MC_M14_FLOW_REPORT=<path>` additionally writes the report as
//! JSON.  The two pure tests (plan shape and solvency arithmetic against the host
//! oracles) need no artifacts and run in a plain `cargo test`.
//!
//! The plan itself is documented in `docs/solana/m14-flow-plan.md`.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use musical_chairs_economics::{
    backers::{BackerKey, Book, Series},
    earnings::{self, Plan, SimpleAccrual},
    fees,
};
use musical_chairs_financial_runtime::{
    close as m7,
    devnet::{self, OraoProfile},
    runtime::{self, anchor_ix},
    world::{CloseRecordArg, PayoutKind, World, DAY, START_TIME},
    Observation,
};
use musical_chairs_program::{
    accounts, close as pclose, instruction, positions::PlanKindArg, state, BackerClaimed,
    CandidatesSealed, DrawCaptured, DrawRequested, ErrorCode, Funding, OperationsFunded,
    PageKind, PayoutPaid, PositionOpened, PromotionApplied, PromotionSkipped, RoundClosed,
    RoundFinalized, SeriesAFunded, TicketCommitted, ID,
};
use solana_sdk::{
    clock::Clock,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

const SOL: u64 = 1_000_000_000;

// ---------------------------------------------------------------------------
// The plan: wallets, amounts and the schedule (all offsets in seconds from genesis,
// where genesis is the moment the G approvals are given)
// ---------------------------------------------------------------------------

/// G's governance delay: the earliest instant `initialize` (and the ORAO register) can
/// execute, so the earliest instant of any flow (packet section 6.4, "day 2.0").
const GOVERNANCE_DELAY: u64 = 172_800;
const T0: u64 = GOVERNANCE_DELAY;

// Amounts (lamports).
/// Operations funding (packet bucket S: 0.5 SOL "covers two closes").
const OPS_FUNDING: u64 = 500_000_000;
/// A1's Series A funding: the program minimum (`min_a`), entitlement 62,000,000.
const A1_SERIES_A: u64 = 50_000_000;
/// A1's dominant Simple position: the packet's per-deposit cap.
const A1_GAME: u64 = SOL;
/// Each round 1 candidate (P4, P5): the program minimum (`min_game`).
const CANDIDATE_GAME: u64 = 10_000_000;
/// Round 2: the plain wallet's Compound30 (opened by the composed finalize/reopen).
const W_COMPOUND30: u64 = 600_000_000;
/// Round 2: P1's Compound15.
const P1_COMPOUND15: u64 = 200_000_000;
/// Round 2: P2's and P3's Simple positions (extra pot, extra candidates).
const P2_SIMPLE: u64 = 100_000_000;
const P3_SIMPLE: u64 = 20_000_000;
/// Round 2: the second Backer's and the round 1 promotion winner's Series A.
const A2_SERIES_A: u64 = 50_000_000;
const WINNER_SERIES_A: u64 = 50_000_000;

// Round 1 timeline.
/// A1 opens the dominant Simple position (`opened_at`).
const O1: u64 = T0 + 240;
/// A1's solvent Simple withdrawal: 6.9 days after open, inside the 12 % tier (< 7 d).
const F3_AT: u64 = O1 + 596_160;
/// A1's insolvent (closing) withdrawal: 9.45 days after open, inside the 7.5 % tier.
const CLOSE1_AT: u64 = O1 + 816_480;
/// The composed finalize/reopen by the plain wallet: round 2 opens (and W's Compound30
/// is pinned) here.
const R2O: u64 = CLOSE1_AT + 180;
// Round 2 timeline.
const P1_OPEN: u64 = R2O + 60;
/// P1's Compound15 settles 60 s after maturity (1,296,000 s from its open).
const C15_AT: u64 = P1_OPEN + 1_296_060;
/// W's Compound30 settles 60 s after maturity (2,592,000 s from R2O).
const C30_AT: u64 = R2O + 2_592_060;
/// The promotion deadline is pinned once at `close_started_at + 600`.
const DEADLINE_SECS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Who {
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
    fn name(self) -> &'static str {
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
    fn seed(self) -> u8 {
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
enum Act {
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
struct Step {
    id: &'static str,
    /// Flow codes this step realizes (comma separated; see `REQUIRED_FLOWS`).
    flows: &'static str,
    /// Seconds from genesis.
    at: u64,
    who: Who,
    act: Act,
    /// Lamports for deposits / Series A / operations funding; 0 otherwise.
    amount: u64,
    /// The expected outcome, as it appears in the plan table of the doc.
    expect: &'static str,
}

/// The flows of the plan's M14 list (docs/superpowers/plans/2026-10-13-native-solana-
/// implementation.md, M14), in the order the doc names them.
const REQUIRED_FLOWS: [&str; 15] = [
    "F1", "F2", "F3", "F4", "F5a", "F5b", "F6", "F7a", "F7b", "F8", "F8r", "F8c", "F9a", "F9b",
    "F10",
];

fn step(
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
fn plan() -> Vec<Step> {
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
fn wallet_funding(who: Who) -> u64 {
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

const ALL_WALLETS: [Who; 10] = [
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

fn day(at: u64) -> f64 {
    at as f64 / DAY as f64
}

/// Devnet rent: 5,080 lamports x (bytes + 128) (packet section 4a).
fn devnet_rent(space: usize) -> u64 {
    5_080 * (space as u64 + 128)
}

// ---------------------------------------------------------------------------
// Pure arithmetic: the plan's solvency, checked against the host oracles
// ---------------------------------------------------------------------------

/// The numbers the plan depends on, from `musical-chairs-economics` (the host oracles
/// the program is differential-tested against) with no SBF involved.
#[derive(Debug)]
struct Arithmetic {
    r1_pot_before_f3: u64,
    f3: fees::FeePayout,
    close1: fees::FeePayout,
    close1_owed_minus_pot: u128,
    /// Seconds after A1's open at which its owed first exceeds the round 1 pot.
    insolvent_from_elapsed: u64,
    a1_net_total: u64,
    seed_carried: u64,
    r2_pot_at_c15: u64,
    c15: fees::FeePayout,
    a1_remaining_after_r1: u128,
    a1_remaining_after_c15: u128,
    a1_still_open_after_c15: bool,
    winner_a_pending: u128,
    winner_b_pending: u128,
    r2_pot_at_c30: u64,
    c30: fees::FeePayout,
}

fn front_end_load_credit(gross: u64) -> u64 {
    fees::front_end_load(gross).expect("load").pot_credit
}

fn arithmetic() -> Arithmetic {
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

#[test]
fn plan_solvency_arithmetic_holds_against_the_host_oracles() {
    let a = arithmetic();
    println!("{a:#?}");
    // Round 1: the close is strict and has a comfortable margin (10 s of accrual is
    // about 12,700 lamports, so 5,000,000 is more than 6 minutes).
    assert!(a.close1_owed_minus_pot >= 5_000_000, "close 1 margin {}", a.close1_owed_minus_pot);
    assert!(a.close1.paid_gross < a.close1.gross_owed.try_into().expect("fits"));
    let insolvent_day = day(O1 + a.insolvent_from_elapsed);
    assert!(
        (11.3..11.4).contains(&insolvent_day),
        "close 1 becomes possible at day {insolvent_day}"
    );
    // The F3 payout is inside the 12 % tier and the close inside the 7.5 % tier.
    assert_eq!(fees::simple_toll_bps(F3_AT - O1), 1_200);
    assert_eq!(fees::simple_toll_bps(CLOSE1_AT - O1), 750);
    // Round 2: C15 solvent with a 10 percent margin, C30 insolvent by a wide margin.
    assert!(
        u128::from(a.r2_pot_at_c15) >= a.c15.gross_owed + a.c15.gross_owed / 10,
        "C15 margin: pot {} owed {}",
        a.r2_pot_at_c15,
        a.c15.gross_owed
    );
    assert!(a.c30.gross_owed >= 10 * u128::from(a.r2_pot_at_c30), "C30 is far beyond the pot");
    // Cap completion: A1 had room left after round 1 and none after the C15 ticket.
    assert!(a.a1_remaining_after_r1 > 10, "A1 must still be open after round 1");
    assert!(a.a1_remaining_after_c15 <= 10 && !a.a1_still_open_after_c15, "A1 reaches 124 percent at C15");
    // The winner's combined claim has both parts.
    assert!(a.winner_a_pending > 0 && a.winner_b_pending > 0);
    // P1's Compound15 pays well above its deposit; W's Compound30 loses most of it.
    assert!(a.c15.net > P1_COMPOUND15);
    assert!(a.c30.net < W_COMPOUND30 / 2);
    // A1, the closer of round 1, is a loser (so it is a fallback candidate, not preferred).
    assert!(a.a1_net_total < A1_GAME);
    assert!(a.seed_carried > 0);
    assert!(a.r1_pot_before_f3 > 0);
}

#[test]
fn plan_is_ordered_within_the_packet_buckets_and_covers_every_flow() {
    let steps = plan();
    // Ordered, and nothing happens before G's delay has run.
    assert!(steps.windows(2).all(|pair| pair[0].at <= pair[1].at), "steps are ordered by offset");
    assert_eq!(steps[0].at, GOVERNANCE_DELAY, "the first flow is at day 2.0");
    let mut ids: Vec<&str> = steps.iter().map(|step| step.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), steps.len(), "step ids are unique");

    // Every flow of the M14 list is realized by some step.
    for flow in REQUIRED_FLOWS {
        assert!(
            steps.iter().any(|step| step.flows.split(',').any(|code| code == flow)),
            "no step realizes {flow}"
        );
    }

    // Packet section 4b/4c caps: <= 1.0 SOL per deposit, <= 0.25 SOL per Series A,
    // <= 6.0 SOL of game deposits, <= 0.5 SOL of Series A, <= 2 SOL per transfer.
    let (mut deposits, mut series_a) = (0u64, 0u64);
    for step in &steps {
        match step.act {
            Act::Open { .. } | Act::ComposedReopen { .. } => {
                assert!(step.amount >= 10_000_000 && step.amount <= SOL, "{} deposit", step.id);
                deposits += step.amount;
            }
            Act::FundSeriesA => {
                assert!(step.amount >= 50_000_000 && step.amount <= SOL / 4, "{} Series A", step.id);
                series_a += step.amount;
            }
            _ => {}
        }
    }
    assert!(deposits <= 6 * SOL, "game deposits {deposits}");
    assert!(series_a <= SOL / 2, "Series A {series_a}");
    let player_funding: u64 = [Who::A1, Who::A2, Who::P1, Who::P2, Who::P3, Who::P4, Who::P5, Who::W]
        .into_iter()
        .map(wallet_funding)
        .sum();
    assert!(player_funding <= 7 * SOL, "test-wallet bucket {player_funding}");
    assert!(wallet_funding(Who::Sponsor) <= SOL, "sponsor bucket");
    assert!(wallet_funding(Who::Keeper) <= SOL / 10, "keeper bucket");
    for who in ALL_WALLETS {
        assert!(wallet_funding(who) <= 2 * SOL, "{} funds in one transfer", who.name());
    }

    // The clock constraints that are the plan's reason to exist.
    let at = |id: &str| steps.iter().find(|step| step.id == id).expect(id).at;
    assert!(at("p1_settle_compound15") >= at("p1_open_compound15") + 15 * DAY, "C15 matured");
    assert!(at("w_settle_compound30") >= at("w_finalize_reopen") + 30 * DAY, "C30 matured");
    let close1 = at("a1_closing_withdraw");
    for id in ["request_draw_1", "fulfil_1", "capture_draw_1", "apply_promotion_1"] {
        assert!(at(id) <= close1 + DEADLINE_SECS, "{id} is inside the deadline of close 1");
    }
    let close2 = at("w_settle_compound30");
    assert!(at("request_draw_2") < close2 + DEADLINE_SECS, "close 2 requests inside the deadline");
    assert_eq!(at("expire_at_deadline"), close2 + DEADLINE_SECS, "the early expiry sits ON the deadline");
    assert_eq!(at("expire_promotion_2"), close2 + DEADLINE_SECS + 1);

    // The duration, as the doc states it.
    let last = steps.last().expect("steps").at;
    let days = day(last);
    assert!((41.0..42.0).contains(&days), "total duration {days} days");
}

// ---------------------------------------------------------------------------
// The LiteSVM run
// ---------------------------------------------------------------------------

fn require_devnet_profile() {
    assert_eq!(
        devnet::orao_profile(),
        OraoProfile::Devnet,
        "set MC_ORAO_PROFILE=devnet: the flow plan must run against the devnet ORAO binary"
    );
    assert_eq!(
        runtime::provider_artifact_sha256(),
        devnet::ORAO_DEVNET_ELF_SHA256,
        "the loaded provider is not the pinned devnet ELF"
    );
}

/// Lamports a step moved for its actor, besides the fees it paid.
#[derive(Default)]
struct Facts {
    /// Deposits, Series A funding and operations funding (leave the actor).
    gross: u64,
    /// Payouts, claims and refunds (reach the actor).
    received: u64,
    /// Rent-bearing accounts the actor paid for in this step: `(kind, space)`.
    created: Vec<(&'static str, usize)>,
}

/// One executed step, for the report.
struct Record {
    step: Step,
    actor: Who,
    clock: u64,
    slot: u64,
    txs: usize,
    fees: u64,
    facts_gross: u64,
    facts_received: u64,
}

#[derive(Default)]
struct Ledger {
    funded: u64,
    deposits: u64,
    series_a: u64,
    operations: u64,
    fees: u64,
    rent_litesvm: u64,
    rent_devnet: u64,
    received: u64,
    min_balance: u64,
    created: Vec<(&'static str, usize)>,
}

struct Sim {
    w: World,
    keys: BTreeMap<Who, Keypair>,
    winner: Option<Who>,
    ledger: BTreeMap<Who, Ledger>,
    records: Vec<Record>,
    first: BTreeMap<String, u64>,
    /// The pure arithmetic the chain is held to at the payout steps.
    predicted: Arithmetic,
    /// `(close_id, what) -> (clock offset, slot)`.
    oracle: BTreeMap<(u64, &'static str), (u64, u64)>,
    /// Pot / owed / margin lines for the solvency report.
    arithmetic: Vec<String>,
}

fn keypair(who: Who) -> Keypair {
    Keypair::new_from_array([who.seed(); 32])
}

impl Sim {
    fn new(predicted: Arithmetic) -> Self {
        let mut w = World::genesis_with_provider().scoped_label("financial.m14");
        w.provider_setup();
        // The devnet cluster domain (hex 6d75736963616c2d... 763121), as in the packet.
        w.set_pool(|pool| pool.cluster_domain = m7::DEVNET_DOMAIN);
        // The post-`initialize` page shape: no registry, snapshot or participant page;
        // the plan provisions them with real `provision_page` transactions.
        w.rt.strip_pages_to_genesis();
        let page = w.rt.participant_address(1, 0);
        w.rt.remove_account(page);
        w.set_pool(|pool| pool.participant_pages = 0);

        let mut keys = BTreeMap::new();
        let mut ledger = BTreeMap::new();
        for who in ALL_WALLETS {
            let key = match who {
                Who::Sponsor => w.sponsor(),
                other => keypair(other),
            };
            let funding = wallet_funding(who);
            match who {
                Who::Sponsor => w.rt.set_lamports(key.pubkey(), funding),
                _ => w.rt.credit_lamports(key.pubkey(), funding),
            }
            ledger.insert(
                who,
                Ledger { funded: funding, min_balance: funding, ..Ledger::default() },
            );
            keys.insert(who, key);
        }
        // The keeper is the default fee payer of every permissionless step.
        w.rt.payer = keys[&Who::Keeper].insecure_clone();
        Self {
            w,
            keys,
            winner: None,
            ledger,
            records: Vec::new(),
            first: BTreeMap::new(),
            predicted,
            oracle: BTreeMap::new(),
            arithmetic: Vec::new(),
        }
    }

    fn resolve(&self, who: Who) -> Who {
        match who {
            Who::Winner => self.winner.expect("the promotion winner is known by now"),
            other => other,
        }
    }

    fn key(&self, who: Who) -> Keypair {
        self.keys[&self.resolve(who)].insecure_clone()
    }

    fn pubkey(&self, who: Who) -> Pubkey {
        self.keys[&self.resolve(who)].pubkey()
    }

    fn balance(&self, who: Who) -> u64 {
        self.w.lamports(self.pubkey(who))
    }

    fn clock_slot(&self) -> u64 {
        self.w.rt.svm.get_sysvar::<Clock>().slot
    }

    fn offset(&self) -> u64 {
        self.w.now() - START_TIME as u64
    }

    fn identities(&self) {
        m7::assert_identities(&self.w);
    }

    /// Send as `who` (the fee payer and the only signer), panicking with context on a
    /// failure.  The result is checked against both identities by the caller's step.
    fn send_as(&mut self, who: Who, id: &str, instructions: Vec<Instruction>) -> Observation {
        match self.try_send_as(who, id, instructions) {
            Ok(observation) => observation,
            Err(failed) => panic!("step {id} failed: {failed:?}"),
        }
    }

    fn try_send_as(
        &mut self,
        who: Who,
        id: &str,
        instructions: Vec<Instruction>,
    ) -> Result<Observation, Observation> {
        let actor = self.key(who);
        let original = std::mem::replace(&mut self.w.rt.payer, actor.insecure_clone());
        let outcome = self.w.send(id, instructions, &[&actor]);
        self.w.rt.payer = original;
        self.identities();
        outcome
    }

    /// Run `step`, then check the actor's lamport equation and that nobody else moved.
    fn run(&mut self, step: Step) {
        self.w.warp_to(START_TIME as u64 + step.at);
        assert_eq!(self.offset(), step.at, "{}: the clock is at the step's offset", step.id);
        let actor = match step.who {
            Who::Winner => self.resolve(Who::Winner),
            other => other,
        };
        let before: BTreeMap<Who, u64> =
            ALL_WALLETS.iter().map(|who| (*who, self.balance(*who))).collect();
        let log_before = self.w.logged().len();
        let facts = self.exec(&step);
        self.identities();

        let fees: u64 =
            self.w.logged()[log_before..].iter().map(|o| o.expected_fee_lamports).sum();
        let txs = self.w.logged().len() - log_before;
        let rent_litesvm: u64 = facts.created.iter().map(|(_, space)| self.w.rt.rent(*space)).sum();
        let rent_devnet: u64 = facts.created.iter().map(|(_, space)| devnet_rent(*space)).sum();
        let delta = i128::from(self.balance(actor)) - i128::from(before[&actor]);
        let expected = i128::from(facts.received)
            - i128::from(facts.gross)
            - i128::from(fees)
            - i128::from(rent_litesvm);
        assert_eq!(
            delta, expected,
            "{}: {} moved {delta} lamports, expected received {} - gross {} - fees {fees} - rent {rent_litesvm}",
            step.id,
            actor.name(),
            facts.received,
            facts.gross
        );
        for who in ALL_WALLETS {
            if who != actor {
                assert_eq!(
                    self.balance(who),
                    before[&who],
                    "{}: {} must not move",
                    step.id,
                    who.name()
                );
            }
        }

        let ledger = self.ledger.get_mut(&actor).expect("ledger");
        match step.act {
            Act::FundSeriesA => ledger.series_a += facts.gross,
            Act::FundOperations => ledger.operations += facts.gross,
            _ => ledger.deposits += facts.gross,
        }
        ledger.fees += fees;
        ledger.rent_litesvm += rent_litesvm;
        ledger.rent_devnet += rent_devnet;
        ledger.received += facts.received;
        ledger.created.extend(facts.created.iter().copied());
        let balance = self.w.lamports(self.keys[&actor].pubkey());
        let ledger = self.ledger.get_mut(&actor).expect("ledger");
        ledger.min_balance = ledger.min_balance.min(balance);

        for flow in step.flows.split(',') {
            self.first.entry(flow.to_string()).or_insert(step.at);
        }
        self.records.push(Record {
            step,
            actor,
            clock: self.w.now(),
            slot: self.clock_slot(),
            txs,
            fees,
            facts_gross: facts.gross,
            facts_received: facts.received,
        });
    }

    fn exec(&mut self, step: &Step) -> Facts {
        let who = step.who;
        let id = step.id;
        let mut facts = Facts::default();
        match step.act {
            Act::FundOperations => self.fund_operations(who, id, step.amount, &mut facts),
            Act::ProvisionRegistry => self.provision_page(id, PageKind::Registry),
            Act::ProvisionSnapshot => self.provision_page(id, PageKind::Snapshot),
            Act::ProvisionParticipants { round } => {
                self.provision_page(id, PageKind::Participant { round })
            }
            Act::ProvisionCloseRecord { close_id } => self.provision_close_record(id, close_id),
            Act::FundSeriesA => self.fund_series_a(who, id, step.amount, &mut facts),
            Act::Open { plan, nonce } => self.open(who, id, plan, nonce, step.amount, &mut facts),
            Act::WithdrawSimple { nonce, closing } => {
                self.payout(who, id, PayoutKind::WithdrawSimple, nonce, closing, &mut facts)
            }
            Act::SettleCompound { nonce, closing } => {
                self.payout(who, id, PayoutKind::SettleCompounding, nonce, closing, &mut facts)
            }
            Act::AdvanceTicket { caps } => self.advance_ticket(id, caps),
            Act::Claim { a, b } => self.claim(who, id, a, b, &mut facts),
            Act::CandidatePages { close_id } => self.candidate_pages(close_id),
            Act::BuildCandidates { close_id } => self.build_candidates(close_id),
            Act::RequestDraw { close_id } => self.request_draw(id, close_id),
            Act::Fulfil { close_id } => self.fulfil(id, close_id),
            Act::CaptureDraw { close_id } => self.capture_draw(id, close_id),
            Act::ApplyPromotion { close_id } => self.apply_promotion(id, close_id),
            Act::ComposedReopen { close_id, plan, nonce } => {
                self.composed_reopen(who, id, close_id, plan, nonce, step.amount, &mut facts)
            }
            Act::ExpireTooEarly { close_id } => self.expire_too_early(id, close_id),
            Act::ExpirePromotion { close_id } => self.expire_promotion(id, close_id),
            Act::FinalizeRound { close_id } => self.finalize_round(id, close_id),
        }
        facts
    }
}

// ---------------------------------------------------------------------------
// Expectations computed from the chain's own pre-state with the host economics
// ---------------------------------------------------------------------------

/// What `withdraw_simple` must pay at `now`, from the position and the pot on chain.
fn simple_expectation(w: &World, owner: Pubkey, nonce: u64, now: u64) -> fees::FeePayout {
    let position = w.position(owner, nonce);
    let mut accrual = SimpleAccrual {
        principal: position.principal,
        last_accrued_at: position.last_accrued_at,
        expires_at: position.maturity_at,
        remainder: position.remainder,
        accrued_gross: position.accrued_gross,
    };
    accrual.accrue(now).expect("the position accrues");
    let owed = accrual.accrued_gross - position.paid_gross;
    fees::payout(owed, w.pool().pot, fees::simple_toll_bps(now - position.opened_at))
        .expect("payout arithmetic")
}

/// What `settle_compounding` must pay at `now`.
fn compound_expectation(w: &World, owner: Pubkey, nonce: u64, now: u64) -> fees::FeePayout {
    let position = w.position(owner, nonce);
    assert!(now >= position.maturity_at, "the position has matured");
    let config = w.config(position.config_version);
    let index = match position.kind {
        1 => 0,
        2 => 1,
        other => panic!("kind {other} is not a compounding plan"),
    };
    let owed = u128::from(position.principal)
        * u128::from(config.compound_factors_q18[index])
        / earnings::Q18;
    fees::payout(owed, w.pool().pot, config.jackpot_bps[index]).expect("payout arithmetic")
}

/// `open_position` against `round`'s accounts (a composed transaction changes the
/// pool's round mid-flight, so the World's own builder cannot be used).
fn open_ix_for_round(
    w: &World,
    owner: Pubkey,
    round: u64,
    plan: PlanKindArg,
    nonce: u64,
    gross: u64,
) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::OpenPosition {
            owner,
            pool: rt.pool_address(),
            admission: rt.admission_address(),
            game_vault: rt.game_vault_address(),
            user: rt.user_address(owner),
            user_round: rt.user_round_address(round, owner),
            position: rt.position_address(owner, nonce),
            config: rt.config_address(1),
            participant_page: rt.participant_address(round, 0),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::OpenPosition {
            plan,
            gross,
            client_nonce: nonce,
            expected_round_id: round,
            expected_config: 1,
        },
    )
}

fn provision_ix(w: &World, kind: PageKind, index: u64, page: Pubkey, payer: Pubkey) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::ProvisionPage {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            page,
            payer,
            close_record: None,
            system_program: system_program::ID,
        },
        instruction::ProvisionPage { kind, index, funding: Funding::Free },
    )
}

// ---------------------------------------------------------------------------
// The steps
// ---------------------------------------------------------------------------

impl Sim {
    fn fund_operations(&mut self, who: Who, id: &str, amount: u64, facts: &mut Facts) {
        let sender = self.pubkey(who);
        let before = self.w.ops();
        let ix = anchor_ix(
            accounts::FundOperations {
                sender,
                ops: self.w.rt.ops_address(),
                ops_cash: self.w.rt.ops_cash_address(),
                system_program: system_program::ID,
                event_authority: self.w.rt.event_authority_address(),
                program: ID,
            },
            instruction::FundOperations { amount },
        );
        let observation = self.send_as(who, id, vec![ix]);
        let event = observation.last_event::<OperationsFunded>();
        assert_eq!(
            (event.funder, event.amount, event.free_balance_after),
            (sender, amount, before.free_balance + amount)
        );
        let after = self.w.ops();
        assert_eq!(after.free_balance, before.free_balance + amount);
        assert_eq!(after.total_contributed, before.total_contributed + u128::from(amount));
        facts.gross = amount;
    }

    fn provision_page(&mut self, id: &str, kind: PageKind) {
        let pool = self.w.pool();
        let free_before = self.w.ops().free_balance;
        let (index, page, space) = match kind {
            PageKind::Registry => (
                pool.registry_pages,
                self.w.rt.registry_address(pool.registry_pages),
                state::RegistryPage::SPACE,
            ),
            PageKind::Snapshot => (
                pool.snapshot_pages,
                self.w.rt.snapshot_address(pool.snapshot_pages),
                state::SnapshotPage::SPACE,
            ),
            PageKind::Participant { round } => (
                pool.participant_pages,
                self.w.rt.participant_address(round, pool.participant_pages),
                state::ParticipantPage::SPACE,
            ),
            PageKind::Candidate { .. } => {
                unreachable!("candidate pages are provisioned by the close helpers")
            }
        };
        let payer = self.w.rt.payer.pubkey();
        let ix = provision_ix(&self.w, kind, index, page, payer);
        m7::ok(&mut self.w, id, vec![ix], &[]);
        let after = self.w.pool();
        match kind {
            PageKind::Registry => assert_eq!(after.registry_pages, index + 1),
            PageKind::Snapshot => assert_eq!(after.snapshot_pages, index + 1),
            PageKind::Participant { .. } => assert_eq!(after.participant_pages, index + 1),
            PageKind::Candidate { .. } => unreachable!(),
        }
        assert_eq!(
            self.w.ops().free_balance,
            free_before - self.w.rt.rent(space),
            "{id}: the page rent comes from Ops.free_balance"
        );
        assert!(
            self.w.account(page).is_some_and(|account| account.owner == ID && account.data.len() == space),
            "{id}: the page exists, program-owned, with its full size"
        );
    }

    fn provision_close_record(&mut self, id: &str, close_id: u64) {
        let free_before = self.w.ops().free_balance;
        let record = self.w.rt.close_record_address(close_id);
        let payer = self.w.rt.payer.pubkey();
        let ix = anchor_ix(
            accounts::ProvisionCloseRecord {
                pool: self.w.rt.pool_address(),
                ops: self.w.rt.ops_address(),
                ops_cash: self.w.rt.ops_cash_address(),
                close_record: record,
                payer,
                system_program: system_program::ID,
            },
            instruction::ProvisionCloseRecord { close_id, funding: Funding::Free },
        );
        m7::ok(&mut self.w, id, vec![ix], &[]);
        assert_eq!(self.w.close_record(close_id).close_id, close_id);
        assert_eq!(
            self.w.ops().free_balance,
            free_before - self.w.rt.rent(state::CloseRecord::SPACE),
            "{id}: the CloseRecord rent comes from Ops.free_balance"
        );
    }

    fn fund_series_a(&mut self, who: Who, id: &str, gross: u64, facts: &mut Facts) {
        let owner = self.pubkey(who);
        let pool0 = self.w.pool();
        let user_new = self.w.account(self.w.rt.user_address(owner)).is_none();
        let backer_key = self.w.rt.backer_address(0, owner);
        let backer_new = self.w.account(backer_key).is_none();
        let (prior_amount, prior_entitlement) = if backer_new {
            (0, 0)
        } else {
            let prior = self.w.backer(owner, 0);
            (prior.amount, prior.entitlement)
        };
        let ix = self.w.fund_series_a_instruction(owner, gross, pool0.round_id);
        let observation = self.send_as(who, id, vec![ix]);
        let event = observation.last_event::<SeriesAFunded>();
        let minted = fees::series_a_entitlement(gross).expect("entitlement");
        assert_eq!(
            (event.owner, event.round_id, event.close_id, event.gross, event.entitlement_after),
            (owner, pool0.round_id, None, gross, prior_entitlement + minted)
        );
        let pool1 = self.w.pool();
        assert_eq!(pool1.pot, pool0.pot + gross, "{id}: Series A enters the pot in full (no load)");
        assert_eq!(pool1.management, pool0.management, "{id}: no Front-End Load on Series A");
        assert_eq!(pool1.registry_len, pool0.registry_len + 1, "{id}: the Backer joins the registry");
        let backer = self.w.backer(owner, 0);
        assert!(backer.active);
        assert_eq!(backer.amount, prior_amount + u128::from(gross));
        assert_eq!(backer.entitlement, prior_entitlement + minted);
        assert!(self.w.user(owner).ever_a);
        facts.gross = gross;
        if user_new {
            facts.created.push(("User", state::User::SPACE));
        }
        if backer_new {
            facts.created.push(("Backer", state::Backer::SPACE));
        }
    }

    fn open(
        &mut self,
        who: Who,
        id: &str,
        plan: PlanKindArg,
        nonce: u64,
        gross: u64,
        facts: &mut Facts,
    ) {
        let owner = self.pubkey(who);
        let pool0 = self.w.pool();
        let user_new = self.w.account(self.w.rt.user_address(owner)).is_none();
        let round_new =
            self.w.account(self.w.rt.user_round_address(pool0.round_id, owner)).is_none();
        let ix = self.w.open_instruction(
            owner,
            plan,
            gross,
            nonce,
            pool0.round_id,
            pool0.active_config_version,
        );
        let observation = self.send_as(who, id, vec![ix]);
        let event = observation.last_event::<PositionOpened>();
        let load = gross * 400 / 10_000;
        assert_eq!(
            (event.owner, event.nonce, event.round_id, event.close_id, event.plan, event.gross, event.load),
            (owner, nonce, pool0.round_id, None, plan.discriminant(), gross, load)
        );
        let now = self.w.now();
        let position = self.w.position(owner, nonce);
        assert_eq!(
            (position.round_id, position.kind, position.principal, position.opened_at, position.maturity_at),
            (pool0.round_id, plan.discriminant(), gross, now, now + plan.duration_seconds())
        );
        let pool1 = self.w.pool();
        assert_eq!(pool1.pot, pool0.pot + gross - load, "{id}: 96 % of the deposit enters the pot");
        assert_eq!(pool1.management, pool0.management + load, "{id}: 4 % Front-End Load");
        assert_eq!(pool1.participant_count, pool0.participant_count + u64::from(round_new));
        facts.gross = gross;
        if user_new {
            facts.created.push(("User", state::User::SPACE));
        }
        if round_new {
            facts.created.push(("UserRound", state::UserRound::SPACE));
        }
        facts.created.push(("Position", state::Position::SPACE));
    }

    /// The payout the chain's pre-state predicts for a step, when the arithmetic module
    /// predicted it independently of the chain.
    fn predicted_payout(&self, id: &str) -> Option<fees::FeePayout> {
        match id {
            "a1_withdraw_simple" => Some(self.predicted.f3),
            "a1_closing_withdraw" => Some(self.predicted.close1),
            "p1_settle_compound15" => Some(self.predicted.c15),
            _ => None,
        }
    }

    fn payout(
        &mut self,
        who: Who,
        id: &str,
        kind: PayoutKind,
        nonce: u64,
        closing: bool,
        facts: &mut Facts,
    ) {
        let owner = self.pubkey(who);
        let now = self.w.now();
        let pool0 = self.w.pool();
        let ops0 = self.w.ops();
        let expected = match kind {
            PayoutKind::WithdrawSimple => simple_expectation(&self.w, owner, nonce, now),
            PayoutKind::SettleCompounding => compound_expectation(&self.w, owner, nonce, now),
        };
        // The solvency arithmetic of this instant, from the chain's own state.
        self.arithmetic.push(format!(
            "{id}: pot {} owed {} -> {} (margin {} lamports); paid {} toll {} net {}",
            pool0.pot,
            expected.gross_owed,
            if expected.insolvent { "INSOLVENT (pot-capped)" } else { "solvent" },
            if expected.insolvent {
                format!("-{}", expected.gross_owed - u128::from(pool0.pot))
            } else {
                format!("{}", u128::from(pool0.pot) - expected.gross_owed)
            },
            expected.paid_gross,
            expected.toll,
            expected.net
        ));
        let model_pot = match id {
            "p1_settle_compound15" => Some(self.predicted.r2_pot_at_c15),
            "w_settle_compound30" => Some(self.predicted.r2_pot_at_c30),
            _ => None,
        };
        if let Some(model) = model_pot {
            self.arithmetic.push(format!(
                "{id}: model pot {model} vs chain pot {} ({})",
                pool0.pot,
                if model == pool0.pot { "match" } else { "DIFFERS" }
            ));
        }
        assert_eq!(
            expected.insolvent, closing,
            "{id}: the plan expects the payout to be {} at this instant (pot {}, owed {})",
            if closing { "strictly insolvent" } else { "solvent" },
            pool0.pot,
            expected.gross_owed
        );
        if let Some(predicted) = self.predicted_payout(id) {
            assert_eq!(expected, predicted, "{id}: the chain disagrees with the pure arithmetic");
        }
        let reserve = closing.then(|| {
            self.w.expected_reserve(pool0.participant_count, pool0.registry_len, 0, 0)
        });
        if let Some(reserve) = reserve {
            assert!(ops0.free_balance >= reserve, "{id}: Ops can lock the close reserve");
        }

        let ix = self.w.payout_instruction(kind, owner, owner, nonce, 0, 0, owner, CloseRecordArg::Auto);
        let observation = self.send_as(who, id, vec![ix]);
        let event = observation.last_event::<PayoutPaid>();
        assert_eq!(
            (event.owner, event.nonce, event.round_id, event.destination, event.closing),
            (owner, nonce, pool0.round_id, owner, closing)
        );
        assert_eq!(
            (event.gross_paid, event.toll, event.net),
            (expected.paid_gross, expected.toll, expected.net)
        );
        assert_eq!(event.toll + event.net, event.gross_paid);
        let pool1 = self.w.pool();
        assert_eq!(pool1.pot, pool0.pot - event.gross_paid, "{id}: the pot pays the gross");
        let user_round = self.w.user_round(owner, pool0.round_id);
        assert!(user_round.net_payout >= u128::from(event.net));
        if closing {
            // Pot-capped: the position is paid the whole pot, strictly less than owed.
            assert_eq!(event.gross_paid, pool0.pot);
            assert!(u128::from(event.gross_paid) < expected.gross_owed);
            assert_eq!(pool1.pot, 0);
            let close_id = pool0.next_close_id;
            assert_eq!(event.close_id, Some(close_id));
            let closed = observation.last_event::<RoundClosed>();
            assert_eq!(
                (closed.round_id, closed.close_id, closed.deadline),
                (pool0.round_id, close_id, now + DEADLINE_SECS)
            );
            assert_eq!(pool1.current_close_id, Some(close_id));
            assert_eq!(pool1.next_close_id, close_id + 1);
            let record = self.w.close_record(close_id);
            let reserve = reserve.expect("closing payout has a reserve");
            assert_eq!(record.required_reserve, reserve, "{id}: spec 8.1 reserve for this population");
            assert_eq!(record.locked_sponsor_remaining, reserve);
            assert_eq!(record.locked_owner_remaining, 0, "{id}: the sponsor funds the whole reserve");
            assert_eq!(record.close_started_at, now);
            assert_eq!(record.promotion_deadline, now + DEADLINE_SECS);
            assert_eq!(record.owner_refund_destination, owner);
            assert_eq!(record.participant_count, pool0.participant_count);
            assert_eq!(record.closing_ticket_id, Some(event.ticket_id));
            assert_eq!(self.w.ops().free_balance, ops0.free_balance - reserve);
            assert_eq!(self.w.ops().locked_remaining_total, ops0.locked_remaining_total + reserve);
        } else {
            assert_eq!(event.close_id, None);
            assert_eq!(pool1.current_close_id, None);
        }
        if event.toll > 0 {
            assert_ne!(event.ticket_id, 0, "{id}: the toll rides an allocation ticket");
            assert_eq!(pool1.active_ticket, event.ticket_id);
            assert_eq!(pool1.pending_toll, pool0.pending_toll + event.toll);
        }
        facts.received = event.net;
    }

    fn advance_ticket(&mut self, id: &str, caps: Option<Who>) {
        let ticket = self.w.ticket();
        assert_ne!(ticket.phase, state::IDLE, "{id}: an allocation ticket is waiting");
        let pool0 = self.w.pool();
        let capped_before = caps.map(|who| {
            let backer = self.w.backer(self.pubkey(who), 0);
            assert!(backer.active && backer.remaining().expect("remaining") > 10, "{id}: still open");
            backer.remaining().expect("remaining")
        });
        self.w.advance_until_idle(8);
        let committed = self.w.logged().last().expect("a commit").last_event::<TicketCommitted>();
        assert_eq!((committed.ticket_id, committed.toll), (ticket.id, ticket.toll));
        assert_eq!(committed.credited + committed.seed, committed.toll, "{id}: the toll is fully allocated");
        let pool1 = self.w.pool();
        assert_eq!(pool1.active_ticket, 0);
        assert_eq!(pool1.finalized_ticket, ticket.id);
        assert_eq!(pool1.pending_toll, pool0.pending_toll - ticket.toll);
        assert_eq!(pool1.seed, pool0.seed + committed.seed);
        assert_eq!(pool1.funded_claims, pool0.funded_claims + u128::from(committed.credited));
        if let Some(close_id) = ticket.close_id {
            assert!(self.w.close_record(close_id).cash_committed, "{id}: the close's cash is committed");
        }
        if let Some(who) = caps {
            let backer = self.w.backer(self.pubkey(who), 0);
            assert!(
                backer.remaining().expect("remaining") <= 10 && !backer.active,
                "{id}: {} did not reach 124 percent (remaining {} of {})",
                who.name(),
                backer.remaining().expect("remaining"),
                backer.entitlement
            );
            assert_eq!(backer.lifetime, backer.entitlement - backer.remaining().expect("remaining"));
            assert_eq!(pool1.registry_len, pool0.registry_len - 1, "{id}: the capped row leaves the registry");
            self.arithmetic.push(format!(
                "{id}: {} had {} lamports of entitlement left, the ticket credited {} -> capped",
                who.name(),
                capped_before.expect("recorded"),
                committed.credited
            ));
        }
    }

    fn claim(&mut self, who: Who, id: &str, want_a: bool, want_b: bool, facts: &mut Facts) {
        let owner = self.pubkey(who);
        let watermark = self.w.pool().finalized_ticket;
        let funded = |series: u8| -> Option<(Pubkey, u64)> {
            let key = self.w.program_account(self.w.rt.backer_address(series, owner))?;
            let mut backer = self.w.backer(owner, series);
            backer.materialize(watermark).expect("materialize");
            Some((key, backer.funded))
        };
        let (a, b) = (funded(0), funded(1));
        let (part_a, part_b) = (a.map_or(0, |(_, funded)| funded), b.map_or(0, |(_, funded)| funded));
        assert_eq!(part_a > 0, want_a, "{id}: Series A funded credit {part_a}");
        assert_eq!(part_b > 0, want_b, "{id}: Series B funded credit {part_b}");
        let claims0 = self.w.pool().funded_claims;
        let ix = self.w.rt.claim_backer_ix(owner, a.map(|(key, _)| key), b.map(|(key, _)| key), owner);
        let observation = self.send_as(who, id, vec![ix]);
        let event = observation.last_event::<BackerClaimed>();
        assert_eq!(
            (event.owner, event.a_part, event.b_part, event.amount),
            (owner, part_a, part_b, part_a + part_b)
        );
        assert_eq!(self.w.pool().funded_claims, claims0 - u128::from(event.amount));
        facts.received = event.amount;
    }

    fn candidate_pages(&mut self, close_id: u64) {
        let (preferred, fallback) = m7::provision_candidate_pages(&mut self.w, close_id);
        let record = self.w.close_record(close_id);
        assert_eq!(
            (preferred, fallback),
            (record.provisioned_preferred_pages, record.provisioned_fallback_pages)
        );
        assert!(preferred >= 1 && fallback >= 1, "close {close_id}: both candidate sets have a page");
    }

    fn build_candidates(&mut self, close_id: u64) {
        let record = self.w.close_record(close_id);
        // The two lists, derived independently from the participants' UserRound / User.
        let (mut preferred, mut fallback) = (Vec::new(), Vec::new());
        for slot in 0..record.participant_count {
            let owner = self.w.participants(record.round_id, slot / 8).rows[(slot % 8) as usize];
            let round = self.w.user_round(owner, record.round_id);
            let user = self.w.user(owner);
            let candidacy = pclose::classify(round.deposits, round.net_payout, user.ever_a, user.ever_b);
            if candidacy.fallback {
                fallback.push(owner);
            }
            if candidacy.preferred {
                preferred.push(owner);
            }
        }
        let (tag, list) = if preferred.is_empty() { (1u8, fallback) } else { (0u8, preferred) };
        let observations = m7::seal_labelled(&mut self.w, close_id, 4, "build_last");
        let sealed = observations.last().expect("a sealing batch").last_event::<CandidatesSealed>();
        assert_eq!(
            (sealed.round_id, sealed.close_id, sealed.tag, sealed.count),
            (record.round_id, close_id, tag, list.len() as u64)
        );
        assert!(self.w.close_record(close_id).sealed);
        let page: state::CandidatePage = self.w.rt.deserialize(
            runtime::pda(&[
                state::CANDIDATES_SEED,
                &close_id.to_le_bytes(),
                &[tag],
                &0u64.to_le_bytes(),
            ])
            .0,
        );
        assert_eq!(
            &page.rows[..usize::from(page.len)],
            list.as_slice(),
            "close {close_id}: the sealed candidate page lists the independently derived set"
        );
    }

    fn request_draw(&mut self, id: &str, close_id: u64) {
        let observation = m7::do_request(&mut self.w, close_id, id);
        let event = observation.last_event::<DrawRequested>();
        let record = self.w.close_record(close_id);
        let config = self.w.config(record.config_version);
        assert_eq!(
            (event.round_id, event.close_id, event.request_account, event.fee_paid),
            (record.round_id, close_id, record.request_account, config.provider_request_fee_lamports)
        );
        assert_eq!(event.request_rent, self.w.rt.rent(usize::from(config.pending_request_bytes)));
        assert_eq!(record.draw_state, state::DRAW_REQUESTED);
        assert!(self.w.now() <= record.promotion_deadline, "{id}: inside the deadline");
        self.oracle.insert((close_id, "requested"), (self.offset(), self.clock_slot()));
    }

    fn fulfil(&mut self, id: &str, close_id: u64) {
        if let Err(failed) = self.w.fulfill(id, close_id) {
            panic!("step {id} failed: {failed:?}");
        }
        assert!(self.w.request_fulfilled(close_id), "{id}: the ORAO request is fulfilled");
        self.oracle.insert((close_id, "fulfilled"), (self.offset(), self.clock_slot()));
    }

    fn capture_draw(&mut self, id: &str, close_id: u64) {
        let ix = m7::capture_ix(&self.w, close_id);
        let observation = m7::ok(&mut self.w, id, vec![ix], &[]);
        let event = observation.last_event::<DrawCaptured>();
        let record = self.w.close_record(close_id);
        assert_eq!(
            (event.round_id, event.close_id, event.captured_at),
            (record.round_id, close_id, self.w.now())
        );
        assert_eq!(record.draw_state, state::DRAW_CAPTURED);
        assert!(self.w.now() <= record.promotion_deadline, "{id}: captured inside the deadline");
        self.oracle.insert((close_id, "captured"), (self.offset(), self.clock_slot()));
    }

    fn apply_promotion(&mut self, id: &str, close_id: u64) {
        let registry0 = self.w.pool().registry_len;
        let ix = m7::apply_ix(&self.w, close_id);
        let observation = m7::ok(&mut self.w, id, vec![ix], &[]);
        let event = observation.last_event::<PromotionApplied>();
        let winner = [Who::P4, Who::P5]
            .into_iter()
            .find(|candidate| self.pubkey(*candidate) == event.winner)
            .unwrap_or_else(|| panic!("{id}: the winner {} is not P4 or P5", event.winner));
        self.winner = Some(winner);
        assert_eq!((event.close_id, event.loss), (Some(close_id), CANDIDATE_GAME));
        let entitlement = u128::from(CANDIDATE_GAME) * 11_600 / 10_000;
        assert_eq!(event.entitlement, entitlement, "{id}: Series B is 116 % of the loss");
        let b_row = self.w.backer(event.winner, 1);
        assert!(b_row.active && b_row.entitlement == entitlement);
        assert_eq!(self.w.pool().registry_len, registry0 + 1, "{id}: the B row joins the registry");
        assert!(self.w.user(event.winner).ever_b);
        assert_eq!(
            self.w.close_record(close_id).promotion,
            state::Promotion::Applied { winner: event.winner, entitlement }
        );
        self.arithmetic.push(format!("{id}: the draw promoted {}", winner.name()));
    }

    #[allow(clippy::too_many_arguments)]
    fn composed_reopen(
        &mut self,
        who: Who,
        id: &str,
        close_id: u64,
        plan: PlanKindArg,
        nonce: u64,
        gross: u64,
        facts: &mut Facts,
    ) {
        let owner = self.pubkey(who);
        let pool0 = self.w.pool();
        let free_before = self.w.ops().free_balance;
        let new_round = pool0.round_id + 1;
        let user_new = self.w.account(self.w.rt.user_address(owner)).is_none();
        let finalize = m7::finalize_ix(&self.w, close_id);
        let page = provision_ix(
            &self.w,
            PageKind::Participant { round: new_round },
            0,
            self.w.rt.participant_address(new_round, 0),
            owner,
        );
        let open = open_ix_for_round(&self.w, owner, new_round, plan, nonce, gross);
        let observation = self.send_as(who, id, vec![finalize, page, open]);
        assert_eq!(observation.expected_fee_lamports, 5_000, "{id}: ONE signature, the plain wallet's");
        let finalized = observation.last_event::<RoundFinalized>();
        assert_eq!(
            (finalized.close_id, finalized.new_round, finalized.seed_carried),
            (close_id, new_round, pool0.seed)
        );
        let opened = observation.last_event::<PositionOpened>();
        let load = gross * 400 / 10_000;
        assert_eq!(
            (opened.owner, opened.round_id, opened.nonce, opened.gross, opened.load),
            (owner, new_round, nonce, gross, load)
        );
        self.arithmetic.push(format!(
            "{id}: model seed carried {} vs chain {} ({})",
            self.predicted.seed_carried,
            finalized.seed_carried,
            if self.predicted.seed_carried == finalized.seed_carried { "match" } else { "DIFFERS" }
        ));
        let pool1 = self.w.pool();
        assert_eq!(pool1.round_id, new_round);
        assert_eq!(pool1.pot, pool0.pot + pool0.seed + gross - load, "{id}: the seed carries into the pot once");
        assert_eq!(pool1.seed, 0);
        assert_eq!(pool1.current_close_id, None);
        assert_eq!((pool1.participant_count, pool1.participant_pages), (1, 1));
        assert!(self.w.close_record(close_id).finalized);
        assert_eq!(self.w.user_round(owner, new_round).participant_slot, 0);
        assert_eq!(
            self.w.ops().free_balance,
            free_before - self.w.rt.rent(state::ParticipantPage::SPACE),
            "{id}: Ops, not the wallet, pays the new round's page"
        );
        facts.gross = gross;
        if user_new {
            facts.created.push(("User", state::User::SPACE));
        }
        facts.created.push(("UserRound", state::UserRound::SPACE));
        facts.created.push(("Position", state::Position::SPACE));
    }

    fn expire_too_early(&mut self, id: &str, close_id: u64) {
        let record = self.w.close_record(close_id);
        assert_eq!(self.w.now(), record.promotion_deadline, "{id}: exactly on the deadline");
        let before = self.w.snapshot_all();
        let ix = m7::expire_ix(&self.w, close_id);
        match self.w.send(id, vec![ix], &[]) {
            Ok(unexpected) => panic!("{id}: expiry on the deadline must be refused: {unexpected:?}"),
            Err(failed) => {
                self.w.assert_rejected(&failed, ErrorCode::DeadlineNotReached);
                self.w.assert_fee_only(&before, &failed);
            }
        }
        assert_eq!(self.w.close_record(close_id).promotion, state::Promotion::Pending);
    }

    fn expire_promotion(&mut self, id: &str, close_id: u64) {
        let record = self.w.close_record(close_id);
        assert_eq!(self.w.now(), record.promotion_deadline + 1, "{id}: one second past the deadline");
        assert_eq!(record.draw_state, state::DRAW_REQUESTED, "{id}: the draw was requested and never captured");
        let ix = m7::expire_ix(&self.w, close_id);
        let observation = m7::ok(&mut self.w, id, vec![ix], &[]);
        let event = observation.last_event::<PromotionSkipped>();
        assert_eq!(
            (event.round_id, event.close_id, event.reason, event.stage),
            (record.round_id, close_id, pclose::REASON_DEADLINE, pclose::STAGE_REQUESTED_UNFULFILLED)
        );
        let after = self.w.close_record(close_id);
        assert_eq!(
            after.promotion,
            state::Promotion::Skipped {
                reason: pclose::REASON_DEADLINE,
                stage: pclose::STAGE_REQUESTED_UNFULFILLED
            }
        );
        assert!(after.reserve_settled, "{id}: the reserve is settled");
    }

    fn finalize_round(&mut self, id: &str, close_id: u64) {
        let pool0 = self.w.pool();
        let ix = m7::finalize_ix(&self.w, close_id);
        let observation = m7::ok(&mut self.w, id, vec![ix], &[]);
        let event = observation.last_event::<RoundFinalized>();
        assert_eq!(
            (event.close_id, event.new_round, event.seed_carried),
            (close_id, pool0.round_id + 1, pool0.seed)
        );
        let pool1 = self.w.pool();
        assert_eq!((pool1.round_id, pool1.pot, pool1.seed), (pool0.round_id + 1, pool0.pot + pool0.seed, 0));
        assert_eq!((pool1.current_close_id, pool1.participant_count), (None, 0));
        assert!(self.w.close_record(close_id).finalized);
    }
}

// ---------------------------------------------------------------------------
// End state and the report
// ---------------------------------------------------------------------------

fn sol(lamports: u64) -> String {
    format!("{:.6}", lamports as f64 / SOL as f64)
}

fn signed_sol(lamports: i128) -> String {
    let magnitude = lamports.unsigned_abs() as u64;
    format!("{}{}", if lamports < 0 { "-" } else { "" }, sol(magnitude))
}

impl Sim {
    fn finish(&mut self) {
        // ---- the end state ----
        let pool = self.w.pool();
        assert_eq!(
            (pool.round_id, pool.next_close_id, pool.current_close_id, pool.active_ticket),
            (3, 3, None, 0),
            "two closes, two finalized rounds, nothing in flight"
        );
        let winner = self.pubkey(Who::Winner);
        let first = self.w.close_record(1);
        assert!(first.finalized && first.cash_committed && first.reserve_settled);
        assert_eq!(first.round_id, 1);
        assert!(
            matches!(first.promotion, state::Promotion::Applied { winner: promoted, .. } if promoted == winner),
            "close 1 promoted the draw's winner"
        );
        let second = self.w.close_record(2);
        assert!(second.finalized && second.cash_committed && second.reserve_settled);
        assert_eq!(second.round_id, 2);
        assert_eq!(
            second.promotion,
            state::Promotion::Skipped {
                reason: pclose::REASON_DEADLINE,
                stage: pclose::STAGE_REQUESTED_UNFULFILLED
            }
        );
        for flow in REQUIRED_FLOWS {
            assert!(self.first.contains_key(flow), "flow {flow} was never reached");
        }
        self.identities();
        self.w.assert_vault_identity();
        self.w.assert_ops_cash_identity();
        // Every wallet kept its rent-exempt floor throughout.
        for (who, ledger) in &self.ledger {
            assert!(ledger.min_balance >= self.w.rt.rent(0), "{} dipped below the rent floor", who.name());
        }
        self.report();
    }

    fn report(&self) {
        let winner = self.winner.expect("winner");
        let last = self.records.last().expect("records").step.at;
        println!("== M14 flow plan: Seq-B with concrete amounts, devnet ORAO binary ==");
        println!(
            "profile {:?}, ORAO ELF {}, cluster domain {}",
            devnet::orao_profile(),
            devnet::ORAO_DEVNET_ELF_SHA256,
            String::from_utf8_lossy(&m7::DEVNET_DOMAIN)
        );
        println!("\n-- steps (offset s | day | actor | txs | fees | slot | gross | received | flows | id) --");
        for record in &self.records {
            let step = &record.step;
            println!(
                "{:>9} | {:>7.3} | {:<6} | {} | {:>6} | {:>3} | {:>10} | {:>10} | {:<7} | {}",
                step.at,
                day(step.at),
                record.actor.name(),
                record.txs,
                record.fees,
                record.slot,
                sol(record.facts_gross),
                sol(record.facts_received),
                step.flows,
                step.id
            );
            assert_eq!(record.clock, START_TIME as u64 + step.at);
            let _ = step.expect;
        }

        println!("\n-- first success of each flow (day) --");
        for flow in REQUIRED_FLOWS {
            println!("{flow:<4} day {:>7.3}", day(self.first[flow]));
        }

        println!("\n-- ids --");
        println!("rounds 1 -> 2 (close 1, promotion applied) -> 3 (close 2, deadline expiry)");
        println!("promotion winner: {} ({})", winner.name(), self.pubkey(Who::Winner));
        let first = self.w.close_record(1);
        let second = self.w.close_record(2);
        println!(
            "close 1: round {} started_at +{} deadline +{} request {}",
            first.round_id,
            first.close_started_at - START_TIME as u64,
            first.promotion_deadline - START_TIME as u64,
            first.request_account
        );
        println!(
            "close 2: round {} started_at +{} deadline +{} request {}",
            second.round_id,
            second.close_started_at - START_TIME as u64,
            second.promotion_deadline - START_TIME as u64,
            second.request_account
        );

        println!("\n-- oracle (LiteSVM: the offsets are the plan's, one slot per warp) --");
        for ((close_id, what), (offset, slot)) in &self.oracle {
            println!("close {close_id} {what:<9} +{offset} s (day {:.3}) slot {slot}", day(*offset));
        }
        let requested = self.oracle[&(1, "requested")];
        let captured = self.oracle[&(1, "captured")];
        println!(
            "close 1 request -> capture: {} s in the plan (deadline 600 s); first live draw evidence at day {:.3}",
            captured.0 - requested.0,
            day(requested.0)
        );

        println!("\n-- solvency arithmetic (chain state at each payout, and model vs chain) --");
        for line in &self.arithmetic {
            println!("{line}");
        }

        println!("\n-- wallets (SOL; rent shown at LiteSVM and at devnet prices) --");
        println!("wallet | funded | deposits | seriesA | ops | rent lite | rent devnet | fees | received | end | net | min | devnet need");
        let mut total_deposits = 0u64;
        let mut total_series_a = 0u64;
        for who in ALL_WALLETS {
            let ledger = &self.ledger[&who];
            let end = self.w.lamports(self.keys[&who].pubkey());
            total_deposits += ledger.deposits;
            total_series_a += ledger.series_a;
            let need = ledger.deposits + ledger.series_a + ledger.operations + ledger.rent_devnet + ledger.fees;
            let role = if who == winner { " (winner)" } else { "" };
            println!(
                "{}{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}",
                who.name(),
                role,
                sol(ledger.funded),
                sol(ledger.deposits),
                sol(ledger.series_a),
                sol(ledger.operations),
                sol(ledger.rent_litesvm),
                sol(ledger.rent_devnet),
                sol(ledger.fees),
                sol(ledger.received),
                sol(end),
                signed_sol(i128::from(end) - i128::from(ledger.funded)),
                sol(ledger.min_balance),
                sol(need)
            );
        }
        println!(
            "totals: game deposits {} SOL (packet cap 6.0), Series A {} SOL (packet cap 0.5)",
            sol(total_deposits),
            sol(total_series_a)
        );

        let ops = self.w.ops();
        let pool = self.w.pool();
        println!("\n-- program books at the end --");
        println!(
            "ops: free {} locked {} unallocated {} total_contributed {} total_rent_funded {} request_rent_outstanding {}",
            ops.free_balance,
            ops.locked_remaining_total,
            ops.unallocated_excess,
            ops.total_contributed,
            ops.total_rent_funded,
            ops.request_rent_outstanding
        );
        println!(
            "pool: pot {} seed {} management {} pending_toll {} funded_claims {} registry_len {} next_ticket {} finalized_ticket {}",
            pool.pot,
            pool.seed,
            pool.management,
            pool.pending_toll,
            pool.funded_claims,
            pool.registry_len,
            pool.next_ticket,
            pool.finalized_ticket
        );
        println!(
            "total duration {last} s = {:.3} days (packet Seq-B: 40.74)",
            day(last)
        );
        println!("M14 flow plan: PASS (both identities held after every step)");

        if let Some(path) = std::env::var_os("MC_M14_FLOW_REPORT") {
            let steps: Vec<serde_json::Value> = self
                .records
                .iter()
                .map(|record| {
                    serde_json::json!({
                        "id": record.step.id,
                        "flows": record.step.flows,
                        "offset_s": record.step.at,
                        "day": day(record.step.at),
                        "actor": record.actor.name(),
                        "txs": record.txs,
                        "fees_lamports": record.fees,
                        "slot": record.slot,
                        "gross_lamports": record.facts_gross,
                        "received_lamports": record.facts_received,
                        "expect": record.step.expect,
                    })
                })
                .collect();
            let wallets: Vec<serde_json::Value> = ALL_WALLETS
                .iter()
                .map(|who| {
                    let ledger = &self.ledger[who];
                    serde_json::json!({
                        "wallet": who.name(),
                        "funded": ledger.funded,
                        "deposits": ledger.deposits,
                        "series_a": ledger.series_a,
                        "operations": ledger.operations,
                        "rent_litesvm": ledger.rent_litesvm,
                        "rent_devnet": ledger.rent_devnet,
                        "fees": ledger.fees,
                        "received": ledger.received,
                        "end": self.w.lamports(self.keys[who].pubkey()),
                        "min_balance": ledger.min_balance,
                    })
                })
                .collect();
            let flows: BTreeMap<&str, u64> =
                REQUIRED_FLOWS.iter().map(|flow| (*flow, self.first[*flow])).collect();
            let oracle: Vec<serde_json::Value> = self
                .oracle
                .iter()
                .map(|((close_id, what), (offset, slot))| {
                    serde_json::json!({"close_id": close_id, "what": what, "offset_s": offset, "slot": slot})
                })
                .collect();
            let report = serde_json::json!({
                "plan": "M14-SIM Seq-B",
                "total_duration_s": last,
                "winner": winner.name(),
                "steps": steps,
                "wallets": wallets,
                "first_success_offset_s": flows,
                "oracle": oracle,
                "arithmetic": self.arithmetic,
            });
            std::fs::write(&path, serde_json::to_vec_pretty(&report).expect("report json"))
                .expect("write MC_M14_FLOW_REPORT");
        }
    }
}

/// The whole plan, executed: every flow of the M14 list is reached, in order, on the
/// production SBF and the devnet ORAO binary, with both identities after every step.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn m14_flow_plan_executes_on_the_devnet_binaries() {
    require_devnet_profile();
    let mut sim = Sim::new(arithmetic());
    for step in plan() {
        sim.run(step);
    }
    sim.finish();
}
