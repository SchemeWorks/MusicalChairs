//! M5 step 4: property and differential payout tests.
//!
//! Every case runs the compiled Musical Chairs SBF in LiteSVM through the M5
//! [`World`] and is compared with the host economics crate
//! (`musical_chairs_economics`), which is the oracle:
//!
//! * `earnings::SimpleAccrual` / `earnings::Plan` for what a position owes,
//! * `fees::payout` / `earnings::Payout::from_gross` for the pot cap, the
//!   carried-interest or jackpot toll and the net,
//! * `fees::simple_toll_bps` for the day-7 / day-14 tier flips.
//!
//! Deterministic seeds (a tiny xorshift64* below, no PRNG dependency):
//!
//! * [`SAMPLED_SEED`]  `0x4D43_5041_594F_5554` ("MCPAYOUT"), 200 cases;
//! * [`ROUTE_SEED`]    `0x4D43_544F_4C4C_5345` ("MCTOLLSE"), 25 cases.
//!
//! Policy choices (so a reader does not have to guess):
//!
//! * Every case is a fresh `World` with a FIXTURE Simple position (no
//!   `open_position`, so `P` up to 10^15 needs no owner cash) opened at
//!   `START_TIME`, config version 1, `Pool.pot` written together with the game
//!   vault lamports by `set_pot_fixture`.
//! * Operations is funded with EXACTLY the closing reserve
//!   (`World::expected_reserve`) and `CloseRecord(1)` is provisioned in EVERY
//!   case, solvent or not: an insolvent payout must then close the round, and a
//!   solvent one must leave the record and the Operations books untouched.
//!   Under-funding (`ReserveUnavailable`) is covered by the positions suite,
//!   not here.
//! * The destination is pre-funded with 1 SOL: the Solana runtime refuses to
//!   leave a brand-new account below the rent-exempt minimum, so a dust net
//!   (a handful of lamports) paid to the harness's unfunded default
//!   destination would fail for a reason that has nothing to do with the
//!   payout math.
//! * `minimum_receive = 0` and `max_work_contribution = 0` everywhere unless a
//!   row says otherwise.
//! * Pot values are drawn from `[0, 2^63]` by the `Uniform` rule and, because a
//!   uniform draw is essentially always solvent (owed never exceeds 2.31e15),
//!   from rules RELATIVE to the host-computed owed (`Equal`, `OneShort`,
//!   `Within`, `OneOver`, `Empty`) so that about half of the cases sit on or
//!   around the strict `owed > pot` insolvency edge.

use musical_chairs_economics::{
    earnings::{Payout, Plan, SimpleAccrual, ROI_30_Q18},
    fees,
};
use musical_chairs_financial_runtime::world::{code, World, DAY, SOL, START_TIME};
use musical_chairs_program::{positions::PlanKindArg, state, ErrorCode, PayoutPaid, RoundClosed};
use solana_sdk::{clock::Clock, signature::Signer};

/// Seed of the 200-case differential sample ("MCPAYOUT").
const SAMPLED_SEED: u64 = 0x4D43_5041_594F_5554;
/// Seed of the 25-case empty-registry routing sample ("MCTOLLSE").
const ROUTE_SEED: u64 = 0x4D43_544F_4C4C_5345;
const SAMPLED_CASES: usize = 200;
const ROUTE_CASES: usize = 25;

const PRINCIPAL_MIN: u64 = 10_000_000;
const PRINCIPAL_MAX: u64 = 1_000_000_000_000_000;
const ELAPSED_MAX: u64 = 21 * DAY;
const POT_MAX: u64 = 1 << 63;

/// Elapsed values the first twelve sampled cases are pinned to: zero, the
/// one-hour and one-day marks, both tier edges +/- 1 s and the 21-day clamp.
const PINNED_ELAPSED: [u64; 12] = [
    0,
    1,
    3_599,
    3_600,
    DAY - 1,
    DAY,
    7 * DAY - 1,
    7 * DAY,
    14 * DAY - 1,
    14 * DAY,
    21 * DAY - 1,
    21 * DAY,
];

// ---------------------------------------------------------------------------
// Deterministic generator
// ---------------------------------------------------------------------------

/// xorshift64* (Vigna).  Deterministic across platforms; seed must be nonzero.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0, "xorshift seeds must be nonzero");
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[low, high]` (inclusive), multiply-shift reduction.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        assert!(low <= high);
        let span = u128::from(high - low) + 1;
        low + ((u128::from(self.next_u64()) * span) >> 64) as u64
    }
}

#[derive(Clone, Copy, Debug)]
enum PotRule {
    /// Uniform in `[0, 2^63]`.
    Uniform,
    /// `pot == owed`: strict insolvency says solvent, the pot ends at 0.
    Equal,
    /// `pot == owed - 1`: one lamport short, the payout closes the round.
    OneShort,
    /// Uniform in `[0, owed]`.
    Within,
    /// `pot == owed + 1`.
    OneOver,
    /// `pot == 0`.
    Empty,
}

const POT_RULES: [PotRule; 6] = [
    PotRule::Uniform,
    PotRule::Equal,
    PotRule::OneShort,
    PotRule::Within,
    PotRule::OneOver,
    PotRule::Empty,
];

#[derive(Clone, Copy, Debug)]
struct Case {
    index: usize,
    principal: u64,
    elapsed: u64,
    pot: u64,
    registry_len: u64,
    /// How `pot` was derived; read only through `Debug` in failure messages.
    #[allow(dead_code)]
    rule: PotRule,
}

/// What a fresh Simple position of `principal` opened at `START_TIME` owes
/// after `elapsed` seconds (host oracle).
fn host_owed(principal: u64, elapsed: u64) -> (SimpleAccrual, u128) {
    let t0 = START_TIME as u64;
    let mut accrual = SimpleAccrual::new(principal, t0).expect("host accrual opens");
    let owed = accrual.accrue(t0 + elapsed).expect("host accrual accrues");
    assert_eq!(owed, accrual.accrued_gross, "a fresh accrual owes what it accrued");
    (accrual, owed)
}

fn build_case(
    rng: &mut Rng,
    index: usize,
    principal: u64,
    elapsed: u64,
    rule: PotRule,
    registry_len: u64,
) -> Case {
    let (_, owed) = host_owed(principal, elapsed);
    let owed = u64::try_from(owed).expect("owed fits u64 for principal <= 10^15");
    let pot = match rule {
        PotRule::Uniform => rng.range(0, POT_MAX),
        PotRule::Equal => owed,
        PotRule::OneShort => owed.saturating_sub(1),
        PotRule::Within => rng.range(0, owed),
        PotRule::OneOver => owed + 1,
        PotRule::Empty => 0,
    };
    Case { index, principal, elapsed, pot, registry_len, rule }
}

/// The 200 differential cases: principal in `[10^7, 10^15]` (the first two
/// pinned to the bounds), elapsed in `[0, 21 d]` (the first twelve pinned to
/// [`PINNED_ELAPSED`]), pot by cycling [`POT_RULES`], and one open Backer in
/// every fourth case (so the toll sometimes goes to escrow instead of seed).
fn sampled_cases() -> Vec<Case> {
    let mut rng = Rng::new(SAMPLED_SEED);
    let mut cases = Vec::with_capacity(SAMPLED_CASES);
    for index in 0..SAMPLED_CASES {
        let principal = match index {
            0 => PRINCIPAL_MIN,
            1 => PRINCIPAL_MAX,
            _ => rng.range(PRINCIPAL_MIN, PRINCIPAL_MAX),
        };
        let elapsed = match PINNED_ELAPSED.get(index) {
            Some(pinned) => *pinned,
            None => rng.range(0, ELAPSED_MAX),
        };
        let rule = POT_RULES[index % POT_RULES.len()];
        let registry_len = u64::from(index % 4 == 3);
        cases.push(build_case(&mut rng, index, principal, elapsed, rule, registry_len));
    }
    cases
}

// ---------------------------------------------------------------------------
// One sampled Simple payout through LiteSVM
// ---------------------------------------------------------------------------

struct Executed {
    owed: u128,
    fee: fees::FeePayout,
    event: PayoutPaid,
    pool_before: state::Pool,
    pool_after: state::Pool,
    compute_units: u64,
}

/// Run `case` on the compiled ELF and assert everything the host oracle
/// predicts.  Panics with the full case on any disagreement.
fn run_case(case: Case) -> Executed {
    let t0 = START_TIME as u64;
    let (accrual, owed) = host_owed(case.principal, case.elapsed);
    let toll_bps = fees::simple_toll_bps(case.elapsed);
    let fee = fees::payout(owed, case.pot, toll_bps).expect("host fee payout");

    // Two host oracles must agree before the program is even consulted, and
    // their arithmetic is restated independently here.
    let from_gross =
        Payout::from_gross(owed, case.pot, toll_bps, 0).expect("host Payout::from_gross");
    assert_eq!(
        (from_gross.paid_gross, from_gross.toll, from_gross.net, from_gross.insolvent),
        (fee.paid_gross, fee.toll, fee.net, fee.insolvent),
        "{case:?}: the two host oracles disagree"
    );
    assert_eq!(u128::from(fee.paid_gross), owed.min(u128::from(case.pot)), "{case:?}");
    assert_eq!(fee.toll + fee.net, fee.paid_gross, "{case:?}");
    assert_eq!(fee.insolvent, owed > u128::from(case.pot), "{case:?}");

    let mut w = World::genesis().scoped("payout_props", "sampled");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let vault = w.rt.game_vault_address();
    w.install_open_backers(case.registry_len);
    w.set_pot_fixture(case.pot);
    w.open_fixture_simple(owner_key, case.principal);
    w.rt.credit_lamports(dest, SOL);
    w.provision_close_record(1);
    let reserve = w.expected_reserve(1, case.registry_len, 0, 0);
    w.set_ops_free(reserve);
    assert_eq!(w.now(), t0, "{case:?}: the position opens at the genesis clock");
    w.warp_seconds(case.elapsed);

    let pool_before = w.pool();
    let ops_before = w.ops();
    let (dest_before, vault_before) = (w.lamports(dest), w.lamports(vault));
    let record_before = w.data(w.rt.close_record_address(1));
    let ticket_before = w.data(w.rt.ticket_address());

    let observation = w
        .withdraw_simple("", &owner, 1, 0, dest, 0)
        .unwrap_or_else(|failed| panic!("{case:?}: withdraw_simple failed: {failed:?}"));
    assert_eq!(
        observation.payer_lamports_before - observation.payer_lamports_after,
        observation.expected_fee_lamports,
        "{case:?}: the fee payer pays exactly the signature fee"
    );

    // --- the event against the oracle -------------------------------------
    let event = w.last_event::<PayoutPaid>();
    assert_eq!(
        (event.gross_paid, event.toll, event.net),
        (fee.paid_gross, fee.toll, fee.net),
        "{case:?}: PayoutPaid disagrees with the host oracle"
    );
    assert_eq!((event.owner, event.nonce, event.round_id), (owner_key, 1, 1), "{case:?}");
    assert_eq!(event.destination, dest, "{case:?}");
    assert_eq!(event.closing, fee.insolvent, "{case:?}: closing == (owed > pot)");
    assert_eq!(event.close_id, fee.insolvent.then_some(1), "{case:?}");
    assert_eq!(event.toll + event.net, event.gross_paid, "{case:?}: toll + net == gross_paid");

    // --- the pot and the cash ----------------------------------------------
    let pool = w.pool();
    assert!(event.gross_paid <= case.pot, "{case:?}: never pays more than the pot");
    assert_eq!(pool.pot, case.pot - event.gross_paid, "{case:?}");
    assert_eq!(
        event.gross_paid + pool.pot,
        case.pot,
        "{case:?}: gross_paid + (pot - gross_paid) == pot"
    );
    assert_eq!(w.lamports(dest) - dest_before, fee.net, "{case:?}: destination receives the net");
    assert_eq!(vault_before - w.lamports(vault), fee.net, "{case:?}: the vault pays the net");
    assert_eq!(
        pool.total_net_payouts - pool_before.total_net_payouts,
        u128::from(fee.net),
        "{case:?}"
    );
    assert_eq!(w.user_round(owner_key, 1).net_payout, u128::from(fee.net), "{case:?}");

    // --- toll routing --------------------------------------------------------
    let seed_delta = pool.seed - pool_before.seed;
    let pending_delta = pool.pending_toll - pool_before.pending_toll;
    assert_eq!(
        seed_delta + pending_delta,
        fee.toll,
        "{case:?}: pool.pending_toll + pool.seed delta == toll"
    );
    let escrowed = case.registry_len > 0 && fee.toll > 0;
    if escrowed {
        // A Backer exists: the toll is escrowed behind a fresh ticket.
        assert_eq!((seed_delta, pending_delta), (0, fee.toll), "{case:?}");
        assert_eq!(event.ticket_id, pool_before.next_ticket, "{case:?}");
        assert_eq!(pool.active_ticket, pool_before.next_ticket, "{case:?}");
        assert_eq!(pool.next_ticket, pool_before.next_ticket + 1, "{case:?}");
        let ticket = w.ticket();
        assert_eq!((ticket.id, ticket.toll), (event.ticket_id, fee.toll), "{case:?}");
    } else {
        // Empty registry (or a zero toll): straight to seed, no ticket.
        assert_eq!((seed_delta, pending_delta), (fee.toll, 0), "{case:?}");
        assert_eq!(event.ticket_id, 0, "{case:?}");
        assert_eq!(pool.active_ticket, pool_before.active_ticket, "{case:?}");
        assert_eq!(pool.next_ticket, pool_before.next_ticket, "{case:?}");
        assert_eq!(w.data(w.rt.ticket_address()), ticket_before, "{case:?}: Ticket untouched");
    }

    // --- the position against the host accrual -------------------------------
    let position = w.position(owner_key, 1);
    let maturity = t0 + ELAPSED_MAX;
    assert_eq!(position.principal, case.principal, "{case:?}");
    assert_eq!(position.accrued_gross, accrual.accrued_gross, "{case:?}");
    assert_eq!(position.remainder, accrual.remainder, "{case:?}");
    assert_eq!(position.last_accrued_at, accrual.last_accrued_at, "{case:?}");
    assert_eq!(position.last_accrued_at, (t0 + case.elapsed).min(maturity), "{case:?}");
    assert!(position.last_accrued_at <= position.maturity_at, "{case:?}: the 21-day clamp");
    assert_eq!(position.paid_gross, u128::from(fee.paid_gross), "{case:?}");
    assert_eq!(position.net_payout, u128::from(fee.net), "{case:?}");
    assert_eq!(position.claimed, t0 + case.elapsed >= maturity, "{case:?}");

    // --- the close (or its absence) -------------------------------------------
    let ops = w.ops();
    if fee.insolvent {
        assert!(pool.closing(), "{case:?}");
        assert_eq!((pool.current_close_id, pool.next_close_id), (Some(1), 2), "{case:?}");
        let record = w.close_record(1);
        assert_eq!(record.required_reserve, reserve, "{case:?}");
        assert_eq!(
            (record.locked_sponsor_remaining, record.locked_owner_remaining),
            (reserve, 0),
            "{case:?}"
        );
        assert_eq!(record.participant_count, 1, "{case:?}");
        assert_eq!(record.close_started_at, w.now(), "{case:?}");
        assert_eq!(record.owner_refund_destination, dest, "{case:?}");
        assert_eq!(
            record.closing_ticket_id,
            escrowed.then_some(event.ticket_id),
            "{case:?}: the close pins the ticket only when a toll was escrowed"
        );
        assert_eq!(record.cash_committed, !escrowed, "{case:?}");
        assert_eq!((ops.free_balance, ops.locked_remaining_total), (0, reserve), "{case:?}");
        let closed = w.last().events::<RoundClosed>();
        assert_eq!(closed.len(), 1, "{case:?}: exactly one RoundClosed");
        assert_eq!(
            (closed[0].round_id, closed[0].close_id, closed[0].deadline),
            (1, 1, record.promotion_deadline),
            "{case:?}"
        );
    } else {
        assert!(!pool.closing(), "{case:?}");
        assert_eq!((pool.current_close_id, pool.next_close_id), (None, 1), "{case:?}");
        assert_eq!(
            w.data(w.rt.close_record_address(1)),
            record_before,
            "{case:?}: a solvent payout leaves the provisioned CloseRecord untouched"
        );
        assert_eq!(
            (ops.free_balance, ops.locked_remaining_total),
            (ops_before.free_balance, ops_before.locked_remaining_total),
            "{case:?}: a solvent payout locks no reserve"
        );
        assert!(w.last().events::<RoundClosed>().is_empty(), "{case:?}");
    }

    Executed {
        owed,
        fee,
        event,
        pool_before,
        pool_after: pool,
        compute_units: observation.compute_units.unwrap_or(0),
    }
}

// ---------------------------------------------------------------------------
// Small world helpers
// ---------------------------------------------------------------------------

/// A world with one fixture Simple position (nonce 1) of `principal` opened at
/// `START_TIME`, the pot written to `pot`, and a pre-funded destination.
fn simple_world(tag: &str, principal: u64, pot: u64) -> World {
    let mut w = World::genesis().scoped("payout_props", tag);
    let owner_key = w.owner().pubkey();
    w.set_pot_fixture(pot);
    w.open_fixture_simple(owner_key, principal);
    let dest = w.dest;
    w.rt.credit_lamports(dest, SOL);
    w
}

/// Move ONLY the Clock sysvar's `unix_timestamp` (the slot and the World's own
/// notion of "now" stay put), so a row can run the clock backwards and then put
/// it back with `set_unix_time(w, w.now() as i64)`.
fn set_unix_time(w: &mut World, unix_timestamp: i64) {
    let mut clock = w.rt.svm.get_sysvar::<Clock>();
    clock.unix_timestamp = unix_timestamp;
    w.rt.svm.set_sysvar(&clock);
}

// ---------------------------------------------------------------------------
// 1. The 200-case differential sample
// ---------------------------------------------------------------------------

#[test]
fn two_hundred_sampled_simple_payouts_agree_with_the_host_oracle() {
    let cases = sampled_cases();
    assert_eq!(cases.len(), 200);
    eprintln!("payout_props: SAMPLED_SEED = {SAMPLED_SEED:#018x}, {} cases", cases.len());

    let (mut solvent, mut insolvent, mut tiers) = (0usize, 0usize, [0usize; 3]);
    let (mut zero_elapsed, mut zero_net, mut with_backer, mut dust_toll) = (0usize, 0, 0, 0);
    let mut max_compute_units = 0u64;
    for case in cases {
        let executed = run_case(case);
        if executed.fee.insolvent {
            insolvent += 1;
        } else {
            solvent += 1;
        }
        match fees::simple_toll_bps(case.elapsed) {
            1_200 => tiers[0] += 1,
            750 => tiers[1] += 1,
            300 => tiers[2] += 1,
            other => panic!("{case:?}: unexpected tier {other}"),
        }
        zero_elapsed += usize::from(case.elapsed == 0);
        zero_net += usize::from(executed.event.net == 0);
        with_backer += usize::from(case.registry_len > 0);
        dust_toll += usize::from(executed.fee.paid_gross > 0 && executed.fee.toll == 0);
        max_compute_units = max_compute_units.max(executed.compute_units);
        if executed.owed == 0 {
            // Elapsed 0 owes nothing: the payout SUCCEEDS paying 0 (it is not
            // MinimumReceive with minimum 0 and it is not an error).
            assert_eq!(case.elapsed, 0, "{case:?}: only elapsed 0 owes nothing");
            assert_eq!(executed.event.gross_paid, 0, "{case:?}");
            assert_eq!(executed.event.net, 0, "{case:?}");
        }
        assert!(executed.pool_after.pot <= executed.pool_before.pot, "{case:?}");
    }
    eprintln!(
        "payout_props: sampled solvent={solvent} insolvent={insolvent} tiers(1200/750/300)={tiers:?} \
         zero_elapsed={zero_elapsed} zero_net={zero_net} with_backer={with_backer} \
         dust_toll_zero={dust_toll} max_compute_units={max_compute_units}"
    );
    assert_eq!(solvent + insolvent, 200);
    assert!(solvent >= 60 && insolvent >= 60, "both branches must be sampled: {solvent}/{insolvent}");
    assert!(tiers.iter().all(|count| *count >= 30), "every toll tier must be sampled: {tiers:?}");
    assert!(zero_elapsed >= 1, "elapsed 0 must be sampled");
    assert!(with_backer >= 40, "the escrow route must be sampled: {with_backer}");
}

// ---------------------------------------------------------------------------
// 2. Repeated Simple claims carry the remainder exactly
// ---------------------------------------------------------------------------

#[test]
fn repeated_simple_claims_carry_the_remainder_exactly() {
    // 123_456_789_013 lamports accrue ~0.157 lamport per second, so most of the
    // early claims owe 0 whole lamports and live entirely on the carried
    // remainder.
    const PRINCIPAL: u64 = 123_456_789_013;
    const OFFSETS: [u64; 7] =
        [1, 3_601, DAY - 1, 7 * DAY - 1, 7 * DAY, 14 * DAY + 5, 21 * DAY + 3_600];
    let pot0 = 10 * PRINCIPAL;
    let mut w = simple_world("repeated_claims", PRINCIPAL, pot0);
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let t0 = w.now();
    let maturity = t0 + 21 * DAY;
    assert_eq!(w.position(owner_key, 1).maturity_at, maturity);

    let mut stepped = SimpleAccrual::new(PRINCIPAL, t0).expect("host accrual");
    let mut pot = pot0;
    let (mut paid_total, mut net_total) = (0u128, 0u128);
    let mut previous = 0;
    for offset in OFFSETS {
        w.warp_seconds(offset - previous);
        previous = offset;
        let dest_before = w.lamports(dest);

        // Host side, stepped exactly like the program.
        let earned = stepped.accrue(t0 + offset).expect("host step");
        let owed = stepped.accrued_gross - paid_total;
        assert_eq!(owed, earned, "offset {offset}: everything earlier was paid in full");
        let fee = fees::payout(owed, pot, fees::simple_toll_bps(offset)).expect("host payout");
        assert!(!fee.insolvent, "offset {offset}: the pot is ten times the principal");

        // One-shot accrual to the same instant must agree with the stepped one
        // (the remainder carry makes accrual path independent).
        let (one_shot, one_shot_owed) = host_owed(PRINCIPAL, offset);
        assert_eq!(one_shot.accrued_gross, stepped.accrued_gross, "offset {offset}");
        assert_eq!(one_shot_owed, stepped.accrued_gross, "offset {offset}");
        assert_eq!(one_shot.remainder, stepped.remainder, "offset {offset}");

        w.withdraw_simple("", &owner, 1, 0, dest, 0)
            .unwrap_or_else(|failed| panic!("claim at +{offset}s failed: {failed:?}"));
        let event = w.last_event::<PayoutPaid>();
        assert_eq!(
            (event.gross_paid, event.toll, event.net),
            (fee.paid_gross, fee.toll, fee.net),
            "claim at +{offset}s"
        );
        assert!(!event.closing, "claim at +{offset}s");
        assert_eq!(w.lamports(dest) - dest_before, fee.net, "claim at +{offset}s");

        pot -= fee.paid_gross;
        paid_total += u128::from(fee.paid_gross);
        net_total += u128::from(fee.net);
        let position = w.position(owner_key, 1);
        assert_eq!(position.accrued_gross, stepped.accrued_gross, "claim at +{offset}s");
        assert_eq!(position.remainder, stepped.remainder, "claim at +{offset}s");
        assert!(position.remainder < 8_640_000, "claim at +{offset}s: remainder < denominator");
        assert_eq!(position.last_accrued_at, stepped.last_accrued_at, "claim at +{offset}s");
        assert_eq!(position.last_accrued_at, (t0 + offset).min(maturity), "claim at +{offset}s");
        assert!(
            position.last_accrued_at <= position.maturity_at,
            "claim at +{offset}s: last_accrued_at never passes maturity_at"
        );
        assert_eq!(position.paid_gross, paid_total, "claim at +{offset}s");
        assert_eq!(position.net_payout, net_total, "claim at +{offset}s");
        assert_eq!(position.claimed, t0 + offset >= maturity, "claim at +{offset}s");
        assert_eq!(w.pool().pot, pot, "claim at +{offset}s");
    }

    // The sum of the claims is the single-claim owed at the final time, which
    // (the 21-day clamp) is the closed form floor(P * 231 / 100).
    let final_offset = *OFFSETS.last().expect("offsets");
    let (_, single_claim) = host_owed(PRINCIPAL, final_offset);
    assert_eq!(paid_total, single_claim, "total paid == single-claim owed");
    assert_eq!(
        single_claim,
        u128::from(PRINCIPAL) * 231 / 100,
        "11 bps-of-1% per second over 1_814_400 s is exactly 2.31 x principal"
    );
    assert_eq!(w.position(owner_key, 1).paid_gross, single_claim);
    assert_eq!(w.pool().pot, pot0 - u64::try_from(single_claim).expect("fits u64"));

    // The clamped claim terminated the position.
    let before = w.snapshot_all();
    let again = w
        .withdraw_simple("after_maturity", &owner, 1, 0, dest, 0)
        .expect_err("a matured and fully claimed Simple position cannot withdraw again");
    w.assert_rejected(&again, ErrorCode::AlreadyClaimed);
    w.assert_fee_only(&before, &again);
}

// ---------------------------------------------------------------------------
// 3. Backwards clock
// ---------------------------------------------------------------------------

#[test]
fn backwards_clock_is_rejected_without_mutation() {
    assert_eq!(code(ErrorCode::BackwardsTime), 6002, "the frozen code");
    let mut w = simple_world("backwards", SOL, 100 * SOL);
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let opened_at = w.now();

    // (a) Before any claim: one second before the position opened.
    set_unix_time(&mut w, opened_at as i64 - 1);
    let before = w.snapshot_all();
    let rejected = w
        .withdraw_simple("before_open", &owner, 1, 0, dest, 0)
        .expect_err("a clock before opened_at is backwards");
    w.assert_rejected(&rejected, ErrorCode::BackwardsTime);
    w.assert_fee_only(&before, &rejected);
    set_unix_time(&mut w, opened_at as i64);

    // Forward, then a real claim.
    w.warp_seconds(10_000);
    w.withdraw_simple("forward_claim", &owner, 1, 0, dest, 0)
        .expect("forward claim");
    let claimed_at = w.position(owner_key, 1).last_accrued_at;
    assert_eq!(claimed_at, opened_at + 10_000);
    let accrued = w.position(owner_key, 1).accrued_gross;
    assert!(accrued > 0);

    // (b) One second before last_accrued_at, and (c) far before it.
    for (variant, unix) in [
        ("one_second_back", claimed_at as i64 - 1),
        ("back_to_open", opened_at as i64),
        ("back_to_epoch_second_one", 1),
    ] {
        set_unix_time(&mut w, unix);
        let before = w.snapshot_all();
        let rejected = w
            .withdraw_simple(variant, &owner, 1, 0, dest, 0)
            .expect_err("a clock before last_accrued_at is backwards");
        w.assert_rejected(&rejected, ErrorCode::BackwardsTime);
        w.assert_fee_only(&before, &rejected);
        assert_eq!(w.position(owner_key, 1).last_accrued_at, claimed_at, "{variant}");
        assert_eq!(w.position(owner_key, 1).accrued_gross, accrued, "{variant}");
    }

    // (d) Exactly last_accrued_at is not backwards: the claim succeeds and
    // pays nothing (the boundary is `now >= last_accrued_at`).
    set_unix_time(&mut w, claimed_at as i64);
    let pot = w.pool().pot;
    let dest_before = w.lamports(dest);
    w.withdraw_simple("same_second", &owner, 1, 0, dest, 0)
        .expect("a claim in the same second succeeds");
    let event = w.last_event::<PayoutPaid>();
    assert_eq!((event.gross_paid, event.toll, event.net), (0, 0, 0));
    assert_eq!(w.pool().pot, pot);
    assert_eq!(w.lamports(dest), dest_before);
    assert_eq!(w.position(owner_key, 1).last_accrued_at, claimed_at);
    assert_eq!(w.position(owner_key, 1).accrued_gross, accrued);
}

// ---------------------------------------------------------------------------
// 4. Toll tier boundaries
// ---------------------------------------------------------------------------

#[test]
fn tier_boundaries_at_day_7_and_14_plus_minus_one_second() {
    let mut w = World::genesis().scoped("payout_props", "tiers");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.set_pot_fixture(100 * SOL);
    // Four positions opened at the same instant; each is claimed once, at a
    // different distance from a boundary, with the clock moving forward only.
    for nonce in 1..=4u64 {
        w.open_fixture_position(owner_key, nonce, PlanKindArg::Simple, SOL);
    }
    let t0 = w.now();
    let steps: [(u64, u16); 4] = [
        (7 * DAY - 1, 1_200),
        (7 * DAY, 750),
        (14 * DAY - 1, 750),
        (14 * DAY, 300),
    ];
    let mut seed_expected = 0u64;
    for (index, (elapsed, bps)) in steps.into_iter().enumerate() {
        let nonce = index as u64 + 1;
        assert_eq!(fees::simple_toll_bps(elapsed), bps, "host tier at +{elapsed}s");
        w.warp_seconds(elapsed - (w.now() - t0));
        let (_, owed) = host_owed(SOL, elapsed);
        w.withdraw_simple("", &owner, nonce, 0, dest, 0)
            .unwrap_or_else(|failed| panic!("tier claim +{elapsed}s failed: {failed:?}"));
        let event = w.last_event::<PayoutPaid>();
        assert_eq!(u128::from(event.gross_paid), owed, "+{elapsed}s");
        // The literal tier, not only the host function.
        assert_eq!(
            u128::from(event.toll),
            owed * u128::from(bps) / 10_000,
            "+{elapsed}s must be priced at {bps} bps"
        );
        assert_eq!(event.net, event.gross_paid - event.toll, "+{elapsed}s");
        seed_expected += event.toll;
        assert_eq!(w.pool().seed, seed_expected, "no Backers: every toll seeds");
    }
}

// ---------------------------------------------------------------------------
// 5. Compound30 maturity vector
// ---------------------------------------------------------------------------

#[test]
fn compound30_maturity_vector() {
    let mut w = World::genesis_with_position(PlanKindArg::Compound30, SOL)
        .scoped("payout_props", "compound30");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.set_pot_fixture(100 * SOL);
    let position = w.position(owner_key, 1);
    assert_eq!(position.maturity_at - position.opened_at, 30 * DAY);

    // The plan's vector and the host oracle agree.
    let plan = Plan::compound_30(SOL, position.opened_at).expect("host plan");
    let owed = plan.owed_at(position.maturity_at).expect("host owed at maturity");
    assert_eq!(owed, 12_267_678_469);
    assert_eq!(owed, u128::from(SOL) * ROI_30_Q18 / 1_000_000_000_000_000_000);
    assert_eq!(fees::compound_jackpot_fee_bps(30), Ok(1_300));
    let fee = fees::payout(owed, 100 * SOL, 1_300).expect("host fee");
    assert_eq!((fee.paid_gross, fee.toll, fee.net), (12_267_678_469, 1_594_798_200, 10_672_880_269));

    // One second early.
    w.warp_seconds(30 * DAY - 1);
    let before = w.snapshot_all();
    let early = w
        .settle_compounding("not_mature_by_one_second", &owner, 1, 0, dest, 0)
        .expect_err("one second before maturity");
    w.assert_rejected(&early, ErrorCode::NotMature);
    w.assert_fee_only(&before, &early);

    // At maturity.
    w.warp_seconds(1);
    let dest_before = w.lamports(dest);
    w.settle_compounding("", &owner, 1, 0, dest, 0)
        .expect("settle at maturity");
    let event = w.last_event::<PayoutPaid>();
    assert_eq!(event.gross_paid, 12_267_678_469);
    assert_eq!(event.toll, 1_594_798_200, "1,300 bps jackpot fee");
    assert_eq!(event.net, 10_672_880_269);
    assert!(!event.closing);
    assert_eq!(w.lamports(dest) - dest_before, 10_672_880_269);
    let position = w.position(owner_key, 1);
    assert!(position.claimed);
    assert_eq!(
        (position.accrued_gross, position.paid_gross, position.net_payout),
        (12_267_678_469, 12_267_678_469, 10_672_880_269)
    );
    let pool = w.pool();
    assert_eq!(pool.pot, 100 * SOL - 12_267_678_469);
    assert_eq!(pool.seed, 1_594_798_200, "no Backers: the jackpot fee seeds");

    // One-shot.
    let before = w.snapshot_all();
    let again = w
        .settle_compounding("second_settlement", &owner, 1, 0, dest, 0)
        .expect_err("a compounding settlement is one-shot");
    w.assert_rejected(&again, ErrorCode::AlreadyClaimed);
    w.assert_fee_only(&before, &again);
}

// ---------------------------------------------------------------------------
// 6. Owed above u64 is narrowed against the pot
// ---------------------------------------------------------------------------

/// A Compound30 fixture position of `u64::MAX / 2` owes about 1.13e20 lamports
/// at maturity, far above `u64::MAX`.  `pot` is chosen by `pot_for`; the payout
/// must narrow to `pot`, close the round and report no overflow.
fn narrowed_against_the_pot(tag: &str, pot_for: impl FnOnce(&World) -> u64) {
    let principal = u64::MAX / 2;
    let mut w = World::genesis().scoped("payout_props", tag);
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let vault = w.rt.game_vault_address();
    w.open_fixture_position(owner_key, 1, PlanKindArg::Compound30, principal);
    let pot = pot_for(&w);
    w.set_pot_fixture(pot);
    w.rt.credit_lamports(dest, SOL);
    w.provision_close_record(1);
    let reserve = w.expected_reserve(1, 0, 0, 0);
    w.set_ops_free(reserve);

    let position = w.position(owner_key, 1);
    let plan = Plan::compound_30(principal, position.opened_at).expect("host plan");
    let owed = plan.owed_at(position.maturity_at).expect("host owed");
    assert!(owed > u128::from(u64::MAX), "{tag}: the case must owe more than u64::MAX");
    assert!(u128::from(pot) < owed, "{tag}: and the pot (a u64) is below it");
    let fee = fees::payout(owed, pot, 1_300).expect("host narrows against the pot");
    assert_eq!(fee.paid_gross, pot, "{tag}");
    assert!(fee.insolvent, "{tag}");

    w.warp_seconds(30 * DAY);
    let (dest_before, vault_before) = (w.lamports(dest), w.lamports(vault));
    w.settle_compounding("", &owner, 1, 0, dest, 0)
        .unwrap_or_else(|failed| panic!("{tag}: settle failed: {failed:?}"));

    let event = w.last_event::<PayoutPaid>();
    assert_eq!(event.gross_paid, pot, "{tag}: gross_paid == pot");
    assert_eq!(
        u128::from(event.toll),
        u128::from(pot) * 1_300 / 10_000,
        "{tag}: 1,300 bps of the narrowed amount"
    );
    assert_eq!(event.net, pot - event.toll, "{tag}");
    assert_eq!((event.toll, event.net), (fee.toll, fee.net), "{tag}");
    assert!(event.closing, "{tag}: owed > pot closes the round");
    assert_eq!(event.close_id, Some(1), "{tag}");
    assert_eq!(w.lamports(dest) - dest_before, event.net, "{tag}");
    assert_eq!(vault_before - w.lamports(vault), event.net, "{tag}");

    let pool = w.pool();
    assert_eq!(pool.pot, 0, "{tag}");
    assert!(pool.closing(), "{tag}");
    assert_eq!(pool.seed, event.toll, "{tag}: no Backers, the toll seeds");
    let position = w.position(owner_key, 1);
    assert!(position.claimed, "{tag}");
    assert_eq!(position.accrued_gross, owed, "{tag}: the full u128 owed is recorded");
    assert_eq!(position.paid_gross, u128::from(pot), "{tag}");
    assert_eq!(position.net_payout, u128::from(event.net), "{tag}");
    assert_eq!(w.last().events::<RoundClosed>().len(), 1, "{tag}");
}

#[test]
fn owed_above_u64_is_narrowed_against_the_pot() {
    // A pot of 2^63 lamports (the sampled upper bound) ...
    narrowed_against_the_pot("pot_two_pow_63", |_| POT_MAX);
    // ... and a pot that fills the game vault to exactly u64::MAX lamports.
    narrowed_against_the_pot("pot_fills_vault_to_u64_max", |w| {
        let headroom = u64::MAX - w.lamports(w.rt.game_vault_address());
        w.pool().pot + headroom
    });
}

// ---------------------------------------------------------------------------
// 7. Toll with an empty registry goes entirely to seed
// ---------------------------------------------------------------------------

/// 25 sampled tolls plus five deterministic dust cases (pot so small that the
/// toll floors to 0 or 1 lamport).
fn route_cases() -> Vec<Case> {
    let mut rng = Rng::new(ROUTE_SEED);
    let rules = [
        PotRule::Uniform,
        PotRule::OneOver,
        PotRule::Within,
        PotRule::OneShort,
        PotRule::Uniform,
    ];
    let mut cases = Vec::new();
    for index in 0..ROUTE_CASES {
        let principal = rng.range(PRINCIPAL_MIN, PRINCIPAL_MAX);
        let elapsed = rng.range(1, ELAPSED_MAX);
        cases.push(build_case(&mut rng, index, principal, elapsed, rules[index % rules.len()], 0));
    }
    // Dust: owed is millions of lamports, the pot is a handful, so the payout
    // is insolvent and the toll floors to 0 (pot 1 and 8 at 1,200 bps; pot 33
    // at 300 bps) or to 1 (pot 9 at 1,200 bps; pot 34 at 300 bps).
    for (offset, (elapsed, pot)) in [(DAY, 1), (DAY, 8), (DAY, 9), (15 * DAY, 33), (15 * DAY, 34)]
        .into_iter()
        .enumerate()
    {
        cases.push(Case {
            index: ROUTE_CASES + offset,
            principal: PRINCIPAL_MIN,
            elapsed,
            pot,
            registry_len: 0,
            rule: PotRule::Empty,
        });
    }
    cases
}

#[test]
fn toll_with_empty_registry_routes_entirely_to_seed_for_sampled_tolls() {
    let cases = route_cases();
    assert_eq!(cases.len(), ROUTE_CASES + 5);
    eprintln!("payout_props: ROUTE_SEED = {ROUTE_SEED:#018x}, {} cases", cases.len());
    let mut nonzero_tolls = 0usize;
    let mut dust_tolls = Vec::new();
    for case in cases {
        assert_eq!(case.registry_len, 0);
        let executed = run_case(case);
        let (before, after) = (&executed.pool_before, &executed.pool_after);
        // The whole toll is seed; nothing is escrowed and no ticket exists.
        assert_eq!(after.seed - before.seed, executed.fee.toll, "{case:?}: seed delta == toll");
        assert_eq!(after.pending_toll, before.pending_toll, "{case:?}: nothing escrowed");
        assert_eq!(after.active_ticket, 0, "{case:?}");
        assert_eq!(after.next_ticket, before.next_ticket, "{case:?}: no ticket consumed");
        assert_eq!(executed.event.ticket_id, 0, "{case:?}");
        nonzero_tolls += usize::from(executed.fee.toll > 0);
        if case.index >= ROUTE_CASES {
            dust_tolls.push((case.pot, executed.fee.toll));
        }
    }
    assert!(nonzero_tolls >= 20, "most sampled tolls are nonzero: {nonzero_tolls}");
    assert_eq!(
        dust_tolls,
        vec![(1, 0), (8, 0), (9, 1), (33, 0), (34, 1)],
        "dust tolls floor as the host oracle says"
    );
}

#[test]
#[ignore = "M6: only-B folds, caps and dust need the allocation ticket handlers, still NotImplemented"]
fn only_b_fold_and_dust_cases_route_to_seed() {
    // Plan M5 step 4 lists "only-B folds/caps/dust" in the production
    // differential matrix, but those cases flow through discover/seal/allocate/
    // prune/commit_ticket (milestone 6).  Un-ignore and fill this in with the
    // host `rounds`/`backers` allocation model as the oracle when they land.
    todo!("M6: differential allocation cases for only-B folds, caps and dust");
}
