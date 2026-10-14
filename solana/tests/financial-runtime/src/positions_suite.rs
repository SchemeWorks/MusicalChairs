//! M5 admission and payout suite.
//!
//! Every row runs the actual compiled Musical Chairs SBF in LiteSVM through a
//! [`World`] (the governed genesis fixture) and exercises `open_position`,
//! `fund_series_a`, `withdraw_simple`, `settle_compounding` and
//! `close_simple_position` with real lamports.  Rows follow the milestone 5
//! test list of `docs/superpowers/plans/2026-10-13-native-solana-implementation.md`
//! and keep its exact numbers.
//!
//! Labels are `financial.positions.<row>` for a row's headline observation and
//! `financial.positions.<row>.<variant>` for the supporting ones.  Every
//! rejection asserts the exact program (or framework) error code parsed from
//! `Observation.error` and fee-only rollback of every account the World can
//! touch; every success asserts both cash-register identities
//! (`World` checks them after every transaction) and the exact lamport
//! movement.  Events are decoded from the event-CPI inner instructions, never
//! from logs.
//!
//! Where the plan's sketch cannot be taken literally against this program the
//! deviation is stated next to the row (the fee payer is a separate account
//! signing alongside the owner, so a payout transaction costs 10,000 lamports,
//! not 5,000; the nonce-replay error is the system program's
//! `AccountAlreadyInUse` from `init`, not a program error; and so on).

use musical_chairs_program::{
    positions::PlanKindArg, state, ErrorCode, PayoutPaid, PositionClosed, PositionOpened,
    RoundClosed, SeriesAFunded,
};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, rent::Rent, signature::Signer};

use crate::controls::{assert_outcomes_consistent, assert_unique_labels, custom_code};
use crate::runtime::Observation;
use crate::world::{CloseRecordArg, PayoutKind, World, DAY, SOL};

pub const ROW_OPEN_ONE_SOL: &str = "open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol";
pub const ROW_WITHDRAW_ONE_DAY: &str =
    "withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000";
pub const ROW_TOLL_NO_BACKERS: &str = "toll_with_no_backers_goes_straight_to_seed_without_ticket";
pub const ROW_MINIMUM_RECEIVE: &str = "minimum_receive_failure_is_atomic_and_fee_only";
pub const ROW_STRICT_INSOLVENCY: &str = "strict_insolvency_closes_and_equality_does_not";
pub const ROW_RESERVE_SCALES: &str = "closing_reserve_scales_with_participants_and_registry_tail";
pub const ROW_SHORT_BY_ONE: &str =
    "closing_payout_short_by_one_lamport_rejects_before_mutation";
pub const ROW_OWNER_FUNDED_CLOSE: &str =
    "owner_funded_close_transfers_contribution_to_opscash_and_rolls_back_on_failed_payout";
pub const ROW_COMPOUND_15: &str = "compound_15_maturity_vector";
pub const ROW_QUOTA: &str = "quota_third_success_blocks_fourth_until_exact_hour";
pub const ROW_FAILED_OPEN: &str = "failed_open_does_not_consume_quota_and_nonce_replay_fails";
pub const ROW_FUND_SERIES_A: &str = "fund_series_a_full_gross_to_pot_entitlement_124";
pub const ROW_SERIES_A_REJECTED: &str =
    "fund_series_a_rejected_while_ticket_active_and_when_flag_off";
pub const ROW_STALE_ROUND: &str = "stale_round_position_is_terminal_without_write";
pub const ROW_SAME_NONCE: &str = "same_nonce_cannot_open_a_second_position_after_settlement";
pub const ROW_UNKNOWN_OUTCOME_OPEN: &str = "unknown_outcome_retry_with_same_nonce_is_idempotent";
pub const ROW_SERIES_A_RETRY: &str =
    "fund_series_a_retry_after_unknown_outcome_keeps_original_signature";
pub const ROW_THIRD_PARTY_PAYER: &str = "third_party_fee_payer_gains_no_authority";
pub const ROW_DESTINATION: &str = "destination_cannot_be_game_vault_ops_cash_or_program_pda";

/// Every row, in suite order.
pub const ROW_NAMES: [&str; 19] = [
    ROW_OPEN_ONE_SOL,
    ROW_WITHDRAW_ONE_DAY,
    ROW_TOLL_NO_BACKERS,
    ROW_MINIMUM_RECEIVE,
    ROW_STRICT_INSOLVENCY,
    ROW_RESERVE_SCALES,
    ROW_SHORT_BY_ONE,
    ROW_OWNER_FUNDED_CLOSE,
    ROW_COMPOUND_15,
    ROW_QUOTA,
    ROW_FAILED_OPEN,
    ROW_FUND_SERIES_A,
    ROW_SERIES_A_REJECTED,
    ROW_STALE_ROUND,
    ROW_SAME_NONCE,
    ROW_UNKNOWN_OUTCOME_OPEN,
    ROW_SERIES_A_RETRY,
    ROW_THIRD_PARTY_PAYER,
    ROW_DESTINATION,
];

/// The whole suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_positions_suite() -> Vec<Observation> {
    let rows: [fn() -> Vec<Observation>; 19] = [
        open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol,
        withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000,
        toll_with_no_backers_goes_straight_to_seed_without_ticket,
        minimum_receive_failure_is_atomic_and_fee_only,
        strict_insolvency_closes_and_equality_does_not,
        closing_reserve_scales_with_participants_and_registry_tail,
        closing_payout_short_by_one_lamport_rejects_before_mutation,
        owner_funded_close_transfers_contribution_to_opscash_and_rolls_back_on_failed_payout,
        compound_15_maturity_vector,
        quota_third_success_blocks_fourth_until_exact_hour,
        failed_open_does_not_consume_quota_and_nonce_replay_fails,
        fund_series_a_full_gross_to_pot_entitlement_124,
        fund_series_a_rejected_while_ticket_active_and_when_flag_off,
        stale_round_position_is_terminal_without_write,
        same_nonce_cannot_open_a_second_position_after_settlement,
        unknown_outcome_retry_with_same_nonce_is_idempotent,
        fund_series_a_retry_after_unknown_outcome_keeps_original_signature,
        third_party_fee_payer_gains_no_authority,
        destination_cannot_be_game_vault_ops_cash_or_program_pda,
    ];
    let mut observations = Vec::new();
    for (row, name) in rows.into_iter().zip(ROW_NAMES) {
        let produced = row();
        assert_headline(&produced, name);
        assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    assert_unique_labels(&observations);
    observations
}

/// A row must contain an observation labelled exactly
/// `financial.positions.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = format!("financial.positions.{row}");
    assert!(
        observations
            .iter()
            .any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The fee the payer paid for a successful transaction: exactly the signature
/// fee, nothing else.
fn assert_fee_paid(observation: &Observation) {
    assert_eq!(
        observation
            .payer_lamports_before
            .checked_sub(observation.payer_lamports_after),
        Some(observation.expected_fee_lamports),
        "{}: the fee payer must pay exactly the signature fee",
        observation.label
    );
}

/// `init` of an already existing Position PDA: the system program refuses the
/// allocation (`SystemError::AccountAlreadyInUse`, `Custom(0)`) before the
/// handler runs.  Anchor's own "already initialised" framework codes (3000,
/// 3003) are accepted too; the exact error is recorded in the observation.
fn assert_account_already_in_use(observation: &Observation) {
    assert!(
        !observation.success,
        "{} unexpectedly succeeded: {observation:?}",
        observation.label
    );
    assert!(
        matches!(custom_code(observation), Some(0 | 3000 | 3003)),
        "{}: expected AccountAlreadyInUse (Custom(0)) or an Anchor already-initialised code, got {:?}",
        observation.label,
        observation.error
    );
    observation.assert_rollback_except_fee();
}

fn schedule_of(world: &World) -> state::ReserveSchedule {
    let rent: Rent = world.rt.svm.get_sysvar::<Rent>();
    state::ReserveSchedule::capture(&rent, &world.config(world.pool().active_config_version))
}

fn received(world: &World, dest: Pubkey, before: u64) -> u64 {
    world.lamports(dest) - before
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

pub fn open_one_sol_simple_splits_40m_management_960m_pot_and_moves_real_sol() -> Vec<Observation> {
    let row = ROW_OPEN_ONE_SOL;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let vault = w.rt.game_vault_address();
    let vault_before = w.lamports(vault);
    let owner_before = w.lamports(owner_key);
    let next_sequence_before = w.pool().next_sequence;

    let observation = w
        .open_position("", &owner, PlanKindArg::Simple, SOL, 1)
        .expect("open one SOL Simple");
    assert_fee_paid(&observation);

    // Real lamports: the whole gross enters the game vault, the owner pays
    // the gross plus the Position's own rent (User and UserRound pre-exist).
    assert_eq!(w.lamports(vault) - vault_before, SOL);
    assert_eq!(
        owner_before - w.lamports(owner_key),
        SOL + w.rt.rent(state::Position::SPACE),
        "the owner is not the fee payer: gross + position rent only"
    );
    let pool = w.pool();
    assert_eq!(pool.management, 40_000_000);
    assert_eq!(pool.pot, 960_000_000);
    assert_eq!(pool.seed + pool.pending_toll + pool.excess, 0);
    assert_eq!(pool.total_gross_deposits, u128::from(SOL));
    assert_eq!(pool.participant_count, 1);
    assert_eq!(
        pool.next_sequence, next_sequence_before,
        "opening a game position never allocates a funding sequence"
    );

    // Accrual base is GROSS (position.principal), pinned to config v1.
    let position = w.position(owner_key, 1);
    assert_eq!(position.principal, SOL);
    assert_eq!(position.owner, owner_key);
    assert_eq!((position.nonce, position.round_id, position.config_version), (1, 1, 1));
    assert_eq!(position.kind, 0);
    assert_eq!(position.opened_at, w.now());
    assert_eq!(position.maturity_at, w.now() + 21 * DAY);
    assert_eq!(position.last_accrued_at, w.now());
    assert_eq!(
        (position.accrued_gross, position.paid_gross, position.net_payout, position.claimed),
        (0, 0, 0, false)
    );
    let user_round = w.user_round(owner_key, 1);
    assert_eq!(user_round.deposits, u128::from(SOL));
    assert_eq!(user_round.participant_slot, 0);
    let page = w.participants(1, 0);
    assert_eq!((page.len, page.rows[0]), (1, owner_key));
    let user = w.user(owner_key);
    assert_eq!((user.call_count, user.successful_at[0]), (1, w.now()));

    assert_eq!(
        w.last_event::<PositionOpened>(),
        PositionOpened {
            schema_version: 1,
            owner: owner_key,
            nonce: 1,
            round_id: 1,
            close_id: None,
            plan: 0,
            gross: SOL,
            load: 40_000_000,
            config_version: 1,
        }
    );
    w.take_log()
}

pub fn withdraw_simple_after_one_day_pays_96_800_000_and_escrows_13_200_000() -> Vec<Observation> {
    let row = ROW_WITHDRAW_ONE_DAY;
    let mut w = World::genesis_with_simple_position_and_one_open_backer(SOL).scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let vault = w.rt.game_vault_address();
    assert_eq!(w.pool().pot, 960_000_000);
    assert_eq!(w.pool().registry_len, 1, "one open Backer");
    w.warp_seconds(DAY);
    let (dest_before, owner_before, vault_before) =
        (w.lamports(dest), w.lamports(owner_key), w.lamports(vault));

    let observation = w
        .withdraw_simple("", &owner, 1, 96_800_000, dest, 0)
        .expect("withdraw after one day");
    assert_fee_paid(&observation);
    assert_eq!(observation.expected_fee_lamports, 10_000, "payer + owner signatures");

    // No deduction from the payout: the destination receives exactly the net,
    // and the owner (not the fee payer) pays nothing.
    assert_eq!(received(&w, dest, dest_before), 96_800_000);
    assert_eq!(w.lamports(owner_key), owner_before);
    assert_eq!(vault_before - w.lamports(vault), 96_800_000);

    let pool = w.pool();
    assert_eq!(pool.pot, 960_000_000 - 110_000_000);
    assert_eq!(pool.pending_toll, 13_200_000);
    assert_eq!(pool.seed, 0, "with a Backer the toll is escrowed, not seeded");
    assert_ne!(pool.active_ticket, 0, "one open Backer: toll escrowed and ticket created");
    assert_eq!((pool.active_ticket, pool.next_ticket), (1, 2));
    assert_eq!(pool.total_net_payouts, 96_800_000);
    assert!(!pool.closing());

    let ticket = w.ticket();
    assert_eq!((ticket.id, ticket.phase, ticket.close_id), (1, state::DISCOVER, None));
    assert_eq!((ticket.toll, ticket.captured_len), (13_200_000, 1));
    assert_eq!(
        (ticket.senior_bps, ticket.a_bps, ticket.b_bps),
        (3_500, 4_500, 2_000),
        "split pinned from the position's config"
    );

    let position = w.position(owner_key, 1);
    assert_eq!(
        (position.accrued_gross, position.paid_gross, position.net_payout, position.claimed),
        (110_000_000, 110_000_000, 96_800_000, false)
    );
    assert_eq!(w.user_round(owner_key, 1).net_payout, 96_800_000);
    assert_eq!(
        w.last_event::<PayoutPaid>(),
        PayoutPaid {
            schema_version: 1,
            owner: owner_key,
            nonce: 1,
            round_id: 1,
            gross_paid: 110_000_000,
            toll: 13_200_000,
            net: 96_800_000,
            destination: dest,
            closing: false,
            close_id: None,
            ticket_id: 1,
        }
    );
    assert!(w.last().events::<RoundClosed>().is_empty());
    w.take_log()
}

pub fn toll_with_no_backers_goes_straight_to_seed_without_ticket() -> Vec<Observation> {
    let row = ROW_TOLL_NO_BACKERS;
    // registry_len == 0: the whole toll goes to seed, no ticket (reference
    // model shortcut, spec 3.5).
    let mut w = World::genesis_with_simple_position(SOL).scoped(row, "");
    let owner = w.owner();
    let dest = w.dest;
    assert_eq!(w.pool().registry_len, 0);
    w.warp_seconds(DAY);
    let ticket_before = w.data(w.rt.ticket_address());
    let next_ticket = w.pool().next_ticket;

    w.withdraw_simple("", &owner, 1, 0, dest, 0).expect("withdraw with no Backers");
    let pool = w.pool();
    assert_eq!(pool.seed, 13_200_000);
    assert_eq!(pool.pending_toll, 0);
    assert_eq!(pool.active_ticket, 0, "no ticket");
    assert_eq!(pool.next_ticket, next_ticket, "no ticket id consumed");
    assert_eq!(pool.pot, 850_000_000);
    assert_eq!(w.data(w.rt.ticket_address()), ticket_before, "Ticket account untouched");
    let event = w.last_event::<PayoutPaid>();
    assert_eq!(
        (event.gross_paid, event.toll, event.net, event.ticket_id, event.closing),
        (110_000_000, 13_200_000, 96_800_000, 0, false)
    );
    w.take_log()
}

pub fn minimum_receive_failure_is_atomic_and_fee_only() -> Vec<Observation> {
    let row = ROW_MINIMUM_RECEIVE;
    let mut w = World::genesis_with_simple_position(SOL).scoped(row, "");
    let owner = w.owner();
    let dest = w.dest;
    w.warp_seconds(DAY);

    let before = w.snapshot_all();
    let rejected = w
        .withdraw_simple("", &owner, 1, 96_800_001, dest, 0)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::MinimumReceive);
    w.assert_fee_only(&before, &rejected);

    // The same transaction with the exact floor is valid.
    let exact = w
        .withdraw_simple("exact_minimum", &owner, 1, 96_800_000, dest, 0)
        .expect("exact minimum receive");
    assert_eq!(exact.last_event::<PayoutPaid>().net, 96_800_000);
    w.take_log()
}

pub fn strict_insolvency_closes_and_equality_does_not() -> Vec<Observation> {
    let row = ROW_STRICT_INSOLVENCY;
    let mut log = Vec::new();

    // Equality: pot exactly equals owed -> solvent, pot ends 0, the round
    // stays Active.
    let mut a = World::genesis().scoped(row, "equality");
    let owner = a.owner();
    let owner_key = owner.pubkey();
    let dest = a.dest;
    a.set_pot_fixture(110_000_000);
    a.open_fixture_simple(owner_key, SOL);
    a.warp_seconds(DAY);
    a.withdraw_simple("", &owner, 1, 0, dest, 0)
        .expect("equality is solvent");
    let pool = a.pool();
    assert_eq!(pool.pot, 0);
    assert!(!pool.closing(), "round stays Active");
    assert_eq!((pool.current_close_id, pool.next_close_id), (None, 1));
    assert_eq!(pool.seed, 13_200_000);
    let event = a.last_event::<PayoutPaid>();
    assert_eq!(
        (event.gross_paid, event.toll, event.net, event.closing, event.close_id),
        (110_000_000, 13_200_000, 96_800_000, false, None)
    );
    assert!(a.last().events::<RoundClosed>().is_empty());
    log.extend(a.take_log());

    // One lamport less -> the payout closes the round.
    let mut b = World::genesis().scoped(row, "");
    b.set_pot_fixture(109_999_999);
    b.open_fixture_simple(owner_key, SOL);
    b.warp_seconds(DAY);
    // The plan's sketch provisions the record and nothing else; the closing
    // path also locks the whole reserve from Operations (or the owner), so
    // Operations is funded with exactly the reserve (no owner contribution).
    let reserve = b.expected_reserve(1, 0, 0, 0);

    // 1. No CloseRecord(next_close_id) provisioned.
    let before = b.snapshot_all();
    let rejected = b.withdraw_simple("no_close_record", &owner, 1, 0, dest, 0).unwrap_err();
    b.assert_rejected(&rejected, ErrorCode::PageNotProvisioned);
    b.assert_fee_only(&before, &rejected);

    // 2. Record provisioned, Operations unfunded.
    b.provision_close_record(1);
    let before = b.snapshot_all();
    let rejected = b
        .withdraw_simple("unfunded_reserve", &owner, 1, 0, dest, 0)
        .unwrap_err();
    b.assert_rejected(&rejected, ErrorCode::ReserveUnavailable);
    b.assert_fee_only(&before, &rejected);

    // 3. Funded: the payout closes the round.
    b.set_ops_free(reserve);
    let closed = b
        .withdraw_simple("", &owner, 1, 0, dest, 0)
        .expect("one lamport short of owed closes the round");
    assert_fee_paid(&closed);
    let pool = b.pool();
    assert_eq!(pool.pot, 0);
    assert!(pool.closing(), "phase is Closing");
    assert_eq!(
        (pool.current_close_id, pool.next_close_id),
        (Some(1), 2),
        "F2: atomic counter capture"
    );
    let record = b.close_record(1);
    assert_eq!(record.close_started_at, b.now());
    assert_eq!(record.promotion_deadline, record.close_started_at + 600);
    assert_eq!(record.participant_count, 1);
    assert_eq!(
        record.required_reserve, reserve,
        "R2/C1: captured per set, not configured"
    );
    assert_eq!(
        record.locked_sponsor_remaining + record.locked_owner_remaining,
        record.required_reserve
    );
    assert_eq!((record.locked_sponsor_remaining, record.locked_owner_remaining), (reserve, 0));
    assert_eq!(
        b.ops().locked_remaining_total,
        record.required_reserve,
        "F1: O(1) aggregate"
    );
    assert_eq!(b.ops().free_balance, 0);
    assert_eq!(
        record.owner_refund_destination, dest,
        "F2: payout destination is the refund destination"
    );
    assert_eq!((record.round_id, record.config_version, record.close_id), (1, 1, 1));
    assert_eq!(record.reserve_schedule, schedule_of(&b));
    assert_eq!(record.required_pages_per_set, 1);
    assert!(record.registry_extension_required);
    assert_eq!(record.approved_provider_fee_cap, 0, "launch provider fee is zero");
    assert_eq!(record.closing_ticket_id, None, "no Backers: no allocation ticket");
    assert!(record.cash_committed);
    assert!(!record.finalized);
    assert_eq!(record.promotion, state::Promotion::Pending);
    assert_eq!(record.draw_state, state::DRAW_NOT_REQUESTED);

    let payout = b.last_event::<PayoutPaid>();
    assert_eq!(
        payout,
        PayoutPaid {
            schema_version: 1,
            owner: owner_key,
            nonce: 1,
            round_id: 1,
            gross_paid: 109_999_999,
            toll: 13_199_999,
            net: 96_800_000,
            destination: dest,
            closing: true,
            close_id: Some(1),
            ticket_id: 0,
        }
    );
    assert_eq!(
        b.last_event::<RoundClosed>(),
        RoundClosed {
            schema_version: 1,
            round_id: 1,
            close_id: 1,
            deadline: record.promotion_deadline,
        }
    );
    assert_eq!(b.pool().seed, 13_199_999);
    log.extend(b.take_log());
    log
}

pub fn closing_reserve_scales_with_participants_and_registry_tail() -> Vec<Observation> {
    let row = ROW_RESERVE_SCALES;
    let mut log = Vec::new();
    // R2 boundaries: participants 8 -> 1 page per set, 9 -> 2; registry_len 8
    // -> extension page, 9 -> none; (1, 0) -> 1 page and an extension (an
    // empty registry's next row opens page 0).
    let cases: [(u64, u64, u64, bool, &str); 3] = [
        (8, 8, 1, true, ""),
        (9, 9, 2, false, "participants9_registry9"),
        (1, 0, 1, true, "participants1_registry0"),
    ];
    for (participants, registry_len, per_set, extension, tag) in cases {
        let mut w = World::closing_fixture(participants, registry_len).scoped(row, tag);
        let owner = w.owner();
        let dest = w.dest;
        let reserve = w.expected_reserve(participants, registry_len, 0, 0);
        w.set_ops_free(reserve);
        w.withdraw_simple("", &owner, 1, 0, dest, 0)
            .expect("closing payout with the reserve funded");

        let record = w.close_record(1);
        assert_eq!(record.required_pages_per_set, per_set, "pages per set");
        assert_eq!(record.registry_extension_required, extension, "registry extension");
        assert_eq!(record.participant_count, participants);
        assert_eq!(record.required_reserve, reserve);
        assert_eq!(
            record.locked_sponsor_remaining + record.locked_owner_remaining,
            record.required_reserve,
            "remaining == unspent at lock time"
        );
        assert_eq!(w.ops().locked_remaining_total, reserve);
        assert_eq!(w.ops().free_balance, 0);
        let pool = w.pool();
        assert_eq!((pool.current_close_id, pool.next_close_id), (Some(1), 2));
        assert_eq!(pool.pot, 0);
        // A non-empty registry escrows the toll behind a ticket that the
        // closing payout pins to the record; an empty one seeds it.
        if registry_len > 0 {
            assert_eq!(pool.active_ticket, 1);
            assert_eq!(pool.pending_toll, 12_000_000);
            assert_eq!(record.closing_ticket_id, Some(1));
            assert!(!record.cash_committed);
            assert_eq!(w.ticket().close_id, Some(1));
        } else {
            assert_eq!(pool.seed, 12_000_000);
            assert_eq!(record.closing_ticket_id, None);
            assert!(record.cash_committed);
        }
        log.extend(w.take_log());
    }
    log
}

pub fn closing_payout_short_by_one_lamport_rejects_before_mutation() -> Vec<Observation> {
    let row = ROW_SHORT_BY_ONE;
    let mut w = World::closing_fixture(9, 8).scoped(row, "");
    let owner = w.owner();
    let dest = w.dest;
    let reserve = w.expected_reserve(9, 8, 0, 0);

    w.set_ops_free(reserve - 1);
    let before = w.snapshot_all();
    let rejected = w.withdraw_simple("", &owner, 1, 0, dest, 0).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::ReserveUnavailable);
    w.assert_fee_only(&before, &rejected);
    assert_eq!(w.ops().free_balance, reserve - 1, "Operations untouched");
    assert!(!w.pool().closing());

    // Spare cash is not mistaken for a smaller obligation.
    w.set_ops_free(reserve + 1_000_000);
    w.withdraw_simple("funded_with_spare", &owner, 1, 0, dest, 0)
        .expect("reserve plus spare cash");
    let record = w.close_record(1);
    assert_eq!(record.locked_sponsor_remaining, reserve);
    assert_eq!(record.locked_owner_remaining, 0);
    assert_eq!(record.required_reserve, reserve);
    let ops = w.ops();
    assert_eq!(ops.free_balance, 1_000_000);
    assert_eq!(ops.locked_remaining_total, reserve);
    w.take_log()
}

pub fn owner_funded_close_transfers_contribution_to_opscash_and_rolls_back_on_failed_payout(
) -> Vec<Observation> {
    let row = ROW_OWNER_FUNDED_CLOSE;
    let mut log = Vec::new();

    // Numeric check (Codex): R, sponsor free = R/2, owner opt-in = R - R/2.
    // The owner is the fee payer here, so the owner's balance carries the
    // fee as well; the destination is a separate account.
    let mut w = World::closing_fixture(1, 0).scoped(row, "");
    w.pay_fees_as_owner();
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let ops_cash = w.rt.ops_cash_address();
    let reserve = w.expected_reserve(1, 0, 0, 0);
    let (sponsor_part, owner_part) = (reserve / 2, reserve - reserve / 2);
    w.set_ops_free(sponsor_part);
    let (owner_before, cash_before, dest_before) =
        (w.lamports(owner_key), w.lamports(ops_cash), w.lamports(dest));

    let closed = w
        .withdraw_simple("", &owner, 1, 0, dest, owner_part)
        .expect("owner-funded closing payout");
    assert_eq!(closed.expected_fee_lamports, 5_000, "owner is the only signer");
    let record = w.close_record(1);
    assert_eq!(record.locked_sponsor_remaining, sponsor_part);
    assert_eq!(record.locked_owner_remaining, owner_part);
    assert_eq!(
        record.locked_sponsor_remaining + record.locked_owner_remaining,
        record.required_reserve
    );
    assert_eq!(
        w.lamports(ops_cash) - cash_before,
        owner_part,
        "physical owner -> OpsCash transfer"
    );
    let net = w.last_event::<PayoutPaid>().net;
    assert_eq!(received(&w, dest, dest_before), net);
    assert_eq!(
        owner_before - w.lamports(owner_key),
        owner_part + closed.expected_fee_lamports,
        "owner -(contribution + fee); the net goes to the separate destination"
    );
    let ops = w.ops();
    assert_eq!(ops.free_balance, 0);
    assert_eq!(ops.locked_remaining_total, reserve);
    w.assert_ops_cash_identity();
    log.extend(w.take_log());

    // The common case: the destination is the owner's own wallet, so the
    // owner -(contribution + fee - net) exactly as the plan states.
    let mut same = World::closing_fixture(1, 0).scoped(row, "destination_is_owner");
    same.pay_fees_as_owner();
    let owner_key = same.owner().pubkey();
    let owner = same.owner();
    same.set_ops_free(sponsor_part);
    let owner_before = same.lamports(owner_key);
    let closed = same
        .withdraw_simple("", &owner, 1, 0, owner_key, owner_part)
        .expect("owner-funded closing payout to the owner's own wallet");
    let net = same.last_event::<PayoutPaid>().net;
    // The owner is also the destination, so the wallet may end up HIGHER than
    // before when net exceeds the contribution plus the fee; compare signed.
    assert_eq!(
        i128::from(owner_before) - i128::from(same.lamports(owner_key)),
        i128::from(owner_part) + i128::from(closed.expected_fee_lamports) - i128::from(net),
        "owner -(o + fee - net)"
    );
    assert_eq!(same.last_event::<PayoutPaid>().destination, owner_key);
    assert_eq!(same.ops().locked_remaining_total, reserve);
    assert_eq!(same.close_record(1).owner_refund_destination, owner_key);
    log.extend(same.take_log());

    // A failed payout (minimum not met) reverts the contribution with
    // everything else.
    let mut failed = World::closing_fixture(1, 0).scoped(row, "failed_payout");
    failed.pay_fees_as_owner();
    let owner = failed.owner();
    let dest = failed.dest;
    failed.set_ops_free(sponsor_part);
    let before = failed.snapshot_all();
    let rejected = failed
        .withdraw_simple("", &owner, 1, u64::MAX, dest, reserve)
        .unwrap_err();
    failed.assert_rejected(&rejected, ErrorCode::MinimumReceive);
    failed.assert_fee_only(&before, &rejected);
    log.extend(failed.take_log());
    log
}

pub fn compound_15_maturity_vector() -> Vec<Observation> {
    let row = ROW_COMPOUND_15;
    let mut w = World::genesis_with_position(PlanKindArg::Compound15, SOL).scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.set_pot_fixture(10 * SOL);
    assert_eq!(
        w.position(owner_key, 1).maturity_at - w.position(owner_key, 1).opened_at,
        15 * DAY
    );

    // Simple withdrawal does not apply to a compounding position.
    let before = w.snapshot_all();
    let wrong_plan = w
        .withdraw_simple("withdraw_simple_on_compound", &owner, 1, 0, dest, 0)
        .unwrap_err();
    w.assert_rejected(&wrong_plan, ErrorCode::InvalidState);
    w.assert_fee_only(&before, &wrong_plan);

    // One second before maturity.
    w.warp_seconds(15 * DAY - 1);
    let before = w.snapshot_all();
    let early = w
        .settle_compounding("not_mature_by_one_second", &owner, 1, 0, dest, 0)
        .unwrap_err();
    w.assert_rejected(&early, ErrorCode::NotMature);
    w.assert_fee_only(&before, &early);

    // At maturity.
    w.warp_seconds(1);
    let dest_before = w.lamports(dest);
    w.settle_compounding("", &owner, 1, 0, dest, 0)
        .expect("settle at maturity");
    let event = w.last_event::<PayoutPaid>();
    assert_eq!(event.gross_paid, 4_473_565_759);
    assert_eq!(event.toll, 402_620_918);
    assert_eq!(event.net, 4_070_944_841);
    assert!(!event.closing);
    assert_eq!(received(&w, dest, dest_before), 4_070_944_841);
    let position = w.position(owner_key, 1);
    assert!(position.claimed);
    assert_eq!(
        (position.accrued_gross, position.paid_gross, position.net_payout),
        (4_473_565_759, 4_473_565_759, 4_070_944_841)
    );
    let pool = w.pool();
    assert_eq!(pool.pot, 10 * SOL - 4_473_565_759);
    assert_eq!(pool.seed, 402_620_918, "no Backers: the jackpot fee seeds");

    // A compounding settlement is one-shot.
    let before = w.snapshot_all();
    let again = w
        .settle_compounding("second_settlement", &owner, 1, 0, dest, 0)
        .unwrap_err();
    w.assert_rejected(&again, ErrorCode::AlreadyClaimed);
    w.assert_fee_only(&before, &again);
    w.take_log()
}

pub fn quota_third_success_blocks_fourth_until_exact_hour() -> Vec<Observation> {
    let row = ROW_QUOTA;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let first_at = w.now();
    for nonce in 1..=3u64 {
        w.open_position(
            &format!("success_{nonce}"),
            &owner,
            PlanKindArg::Simple,
            10_000_000,
            nonce,
        )
        .unwrap_or_else(|failed| panic!("success {nonce} failed: {failed:?}"));
    }
    assert_eq!(w.user(owner_key).call_count, 3);

    let before = w.snapshot_all();
    let blocked = w
        .open_position("", &owner, PlanKindArg::Simple, 10_000_000, 4)
        .unwrap_err();
    w.assert_rejected(&blocked, ErrorCode::QuotaExceeded);
    w.assert_fee_only(&before, &blocked);

    w.warp_seconds(3_599);
    let before = w.snapshot_all();
    let still_blocked = w
        .open_position("at_3599_seconds", &owner, PlanKindArg::Simple, 10_000_000, 4)
        .unwrap_err();
    w.assert_rejected(&still_blocked, ErrorCode::QuotaExceeded);
    w.assert_fee_only(&before, &still_blocked);

    w.warp_seconds(1);
    w.open_position("at_3600_seconds", &owner, PlanKindArg::Simple, 10_000_000, 4)
        .expect("exactly one hour old has expired");
    let user = w.user(owner_key);
    assert_eq!(user.call_count, 3, "a rolling window of three entries");
    assert_eq!(user.successful_at, [first_at, first_at, w.now()]);
    let pool = w.pool();
    assert_eq!(pool.pot, 4 * 9_600_000);
    assert_eq!(pool.management, 4 * 400_000);
    assert_eq!(pool.participant_count, 1, "one owner, one participant row");
    w.take_log()
}

pub fn failed_open_does_not_consume_quota_and_nonce_replay_fails() -> Vec<Observation> {
    let row = ROW_FAILED_OPEN;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();

    let before = w.snapshot_all();
    let below_minimum = w
        .open_position("below_minimum", &owner, PlanKindArg::Simple, 9_999_999, 1)
        .unwrap_err();
    w.assert_rejected(&below_minimum, ErrorCode::MinimumDeposit);
    w.assert_fee_only(&before, &below_minimum);
    assert_eq!(w.user(owner_key).call_count, 0, "a failed open consumes no quota");

    w.open_position("first_open", &owner, PlanKindArg::Simple, 10_000_000, 1)
        .expect("minimum open");
    assert_eq!(w.user(owner_key).call_count, 1);
    assert_eq!(w.pool().pot, 9_600_000);

    // Replaying the nonce: `init` of the existing Position PDA.
    let before = w.snapshot_all();
    let replay = w
        .open_position("", &owner, PlanKindArg::Simple, 10_000_000, 1)
        .unwrap_err();
    assert_account_already_in_use(&replay);
    w.assert_fee_only(&before, &replay);
    assert_eq!(w.user(owner_key).call_count, 1, "successful_calls == 1");
    assert_eq!(w.pool().pot, 9_600_000, "credited once");

    // The remaining admission gates, each fee-only and quota-free, in the
    // order the spec fixes (paused, closing, round, config, minimum).
    let gates: [(&str, ErrorCode); 5] = [
        ("paused", ErrorCode::AdmissionPaused),
        ("closing", ErrorCode::Closing),
        ("stale_round", ErrorCode::StaleRound),
        ("stale_config", ErrorCode::StaleConfig),
        ("paused_beats_minimum", ErrorCode::AdmissionPaused),
    ];
    for (name, expected) in gates {
        match name {
            "paused" | "paused_beats_minimum" => w.set_paused(true),
            "closing" => w.set_pool(|pool| pool.current_close_id = Some(1)),
            _ => {}
        }
        let (gross, round, config) = match name {
            "stale_round" => (10_000_000, 2, 1),
            "stale_config" => (10_000_000, 1, 2),
            "paused_beats_minimum" => (9_999_999, 1, 1),
            _ => (10_000_000, 1, 1),
        };
        let before = w.snapshot_all();
        let rejected = w
            .open_position_expecting(name, &owner, PlanKindArg::Simple, gross, 2, round, config)
            .unwrap_err();
        w.assert_rejected(&rejected, expected);
        w.assert_fee_only(&before, &rejected);
        w.set_paused(false);
        w.set_pool(|pool| pool.current_close_id = None);
        assert_eq!(w.user(owner_key).call_count, 1, "{name} consumed no quota");
    }
    w.take_log()
}

pub fn fund_series_a_full_gross_to_pot_entitlement_124() -> Vec<Observation> {
    let row = ROW_FUND_SERIES_A;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let vault = w.rt.game_vault_address();
    let (vault_before, owner_before) = (w.lamports(vault), w.lamports(owner_key));

    let observation = w
        .fund_series_a("", &owner, 50_000_000, 1)
        .expect("fund Series A");
    assert_fee_paid(&observation);
    assert_eq!(w.lamports(vault) - vault_before, 50_000_000);
    assert_eq!(owner_before - w.lamports(owner_key), 50_000_000);
    let pool = w.pool();
    assert_eq!(pool.pot, 50_000_000, "the full gross goes to the pot");
    assert_eq!(pool.management, 0, "no Front-End Load on Series A");
    assert_eq!(pool.registry_len, 1);
    assert_eq!(pool.next_sequence, 2);
    let backer = w.backer(owner_key, 0);
    assert_eq!(backer.entitlement, 62_000_000);
    assert_eq!(backer.amount, 50_000_000);
    assert_eq!(backer.first_sequence, 1);
    assert!(backer.active);
    assert_eq!((backer.slot, backer.series, backer.owner), (0, 0, owner_key));
    assert_eq!(
        w.registry(0).keys[0],
        w.rt.backer_address(0, owner_key),
        "registry row 0 is the Backer PDA"
    );
    let user = w.user(owner_key);
    assert!(user.ever_a);
    assert_eq!(user.call_count, 1);
    assert_eq!(
        w.last_event::<SeriesAFunded>(),
        SeriesAFunded {
            schema_version: 1,
            owner: owner_key,
            round_id: 1,
            close_id: None,
            gross: 50_000_000,
            entitlement_after: 62_000_000,
        }
    );
    w.take_log()
}

pub fn fund_series_a_rejected_while_ticket_active_and_when_flag_off() -> Vec<Observation> {
    let row = ROW_SERIES_A_REJECTED;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();

    // An allocation ticket is active.
    w.set_pool(|pool| {
        pool.active_ticket = 1;
        pool.next_ticket = 2;
    });
    w.rt.replace_ticket(state::Ticket {
        bump: w.ticket().bump,
        id: 1,
        phase: state::DISCOVER,
        ..state::Ticket::default()
    });
    let before = w.snapshot_all();
    let rejected = w.fund_series_a("", &owner, 50_000_000, 1).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::AllocationPending);
    w.assert_fee_only(&before, &rejected);
    w.set_pool(|pool| pool.active_ticket = 0);

    // The public Series A flag is off (and wins over every later gate).
    w.set_public_series_a(false);
    let before = w.snapshot_all();
    let rejected = w.fund_series_a("flag_off", &owner, 50_000_000, 1).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::PublicSeriesADisabled);
    w.assert_fee_only(&before, &rejected);
    w.set_paused(true);
    let before = w.snapshot_all();
    let rejected = w
        .fund_series_a("flag_off_beats_paused", &owner, 50_000_000, 1)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::PublicSeriesADisabled);
    w.assert_fee_only(&before, &rejected);
    w.set_paused(false);
    w.set_public_series_a(true);

    // The other gates, in order: paused, closing, ticket, round, minimum.
    w.set_paused(true);
    let before = w.snapshot_all();
    let rejected = w.fund_series_a("paused", &owner, 50_000_000, 1).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::AdmissionPaused);
    w.assert_fee_only(&before, &rejected);
    w.set_paused(false);

    w.set_pool(|pool| pool.current_close_id = Some(1));
    let before = w.snapshot_all();
    let rejected = w.fund_series_a("closing", &owner, 50_000_000, 1).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::Closing);
    w.assert_fee_only(&before, &rejected);
    w.set_pool(|pool| pool.current_close_id = None);

    let before = w.snapshot_all();
    let rejected = w.fund_series_a("stale_round", &owner, 50_000_000, 2).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::StaleRound);
    w.assert_fee_only(&before, &rejected);

    let before = w.snapshot_all();
    let rejected = w
        .fund_series_a("below_minimum", &owner, 49_999_999, 1)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::MinimumDeposit);
    w.assert_fee_only(&before, &rejected);

    // With every gate open the same transaction succeeds.
    w.fund_series_a("enabled_and_idle", &owner, 50_000_000, 1)
        .expect("Series A funding once the gates are open");
    assert_eq!(w.pool().pot, 50_000_000);
    w.take_log()
}

pub fn stale_round_position_is_terminal_without_write() -> Vec<Observation> {
    let row = ROW_STALE_ROUND;
    let mut w = World::genesis_with_simple_position(SOL).scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.open_position("open_compound_15", &owner, PlanKindArg::Compound15, 100_000_000, 2)
        .expect("second position");
    // A third Simple position that is fully paid up to its maturity, so
    // `close_simple_position` has a legitimate success to contrast with.
    w.open_fixture_position(owner_key, 3, PlanKindArg::Simple, SOL);
    w.set_pot_fixture(10 * SOL);

    // `close_simple_position` before maturity.
    let before = w.snapshot_all();
    let early = w
        .close_simple_position("close_simple_not_mature", &owner, 3)
        .unwrap_err();
    w.assert_rejected(&early, ErrorCode::NotMature);
    w.assert_fee_only(&before, &early);

    // Past both maturities, so only the closed round can stop the calls.
    w.warp_seconds(21 * DAY);
    let mut paid_up = w.position(owner_key, 3);
    paid_up.accrued_gross = 2_310_000_000;
    paid_up.paid_gross = 2_310_000_000;
    paid_up.net_payout = 2_240_700_000;
    paid_up.last_accrued_at = paid_up.maturity_at;
    w.rt.replace_position(paid_up);

    // Closing with earnings outstanding would forfeit them.
    let before = w.snapshot_all();
    let outstanding = w
        .close_simple_position("close_simple_with_earnings_outstanding", &owner, 1)
        .unwrap_err();
    w.assert_rejected(&outstanding, ErrorCode::InvalidState);
    w.assert_fee_only(&before, &outstanding);

    // Fully paid and matured: a terminal bookkeeping close that moves no cash.
    let vault = w.rt.game_vault_address();
    let vault_before = w.lamports(vault);
    w.close_simple_position("close_simple_after_full_payment", &owner, 3)
        .expect("close a fully paid matured Simple position");
    assert!(w.position(owner_key, 3).claimed);
    assert_eq!(w.lamports(vault), vault_before, "no cash moves");
    assert_eq!(
        w.last_event::<PositionClosed>(),
        PositionClosed {
            schema_version: 1,
            owner: owner_key,
            nonce: 3,
            round_id: 1,
        }
    );

    // The round advances: every old-round Position is terminal.
    w.advance_round_fixture();
    assert_eq!(w.pool().round_id, 2);
    let simple_bytes = w.data(w.rt.position_address(owner_key, 1));
    let compound_bytes = w.data(w.rt.position_address(owner_key, 2));

    let before = w.snapshot_all();
    let withdraw = w.withdraw_simple("", &owner, 1, 0, dest, 0).unwrap_err();
    w.assert_rejected(&withdraw, ErrorCode::PositionRoundClosed);
    w.assert_fee_only(&before, &withdraw);

    let before = w.snapshot_all();
    let settle = w
        .settle_compounding("settle_compounding", &owner, 2, 0, dest, 0)
        .unwrap_err();
    w.assert_rejected(&settle, ErrorCode::PositionRoundClosed);
    w.assert_fee_only(&before, &settle);

    let before = w.snapshot_all();
    let close = w
        .close_simple_position("close_simple_position", &owner, 1)
        .unwrap_err();
    w.assert_rejected(&close, ErrorCode::PositionRoundClosed);
    w.assert_fee_only(&before, &close);

    // No reclamation instruction exists: the old-round Positions stay as
    // they were, byte for byte.
    assert_eq!(w.data(w.rt.position_address(owner_key, 1)), simple_bytes);
    assert_eq!(w.data(w.rt.position_address(owner_key, 2)), compound_bytes);
    assert!(!w.position(owner_key, 1).claimed);
    assert!(!w.position(owner_key, 2).claimed);
    w.take_log()
}

pub fn same_nonce_cannot_open_a_second_position_after_settlement() -> Vec<Observation> {
    let row = ROW_SAME_NONCE;
    // R1: a Position is permanent, so its PDA is the nonce tombstone.
    let mut w = World::genesis_with_simple_position(SOL).scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    assert_eq!(w.user(owner_key).call_count, 1, "the setup open");
    // Owed at maturity is 2.31 SOL, more than the 0.96 SOL pot.
    w.set_pot_fixture(10 * SOL);
    w.warp_seconds(21 * DAY);
    w.withdraw_simple("settle_at_maturity", &owner, 1, 0, dest, 0)
        .expect("matured payout");
    let event = w.last_event::<PayoutPaid>();
    assert_eq!((event.gross_paid, event.toll), (2_310_000_000, 69_300_000));
    assert!(w.position(owner_key, 1).claimed, "matured -> Closed");

    w.advance_round_fixture();
    let settled_bytes = w.data(w.rt.position_address(owner_key, 1));
    let before = w.snapshot_all();
    let replay = w
        .open_position("", &owner, PlanKindArg::Simple, 10_000_000, 1)
        .unwrap_err();
    assert_account_already_in_use(&replay);
    w.assert_fee_only(&before, &replay);
    assert_eq!(w.user(owner_key).call_count, 1, "the failed replay consumed no quota");
    assert_eq!(w.data(w.rt.position_address(owner_key, 1)), settled_bytes);

    w.open_position("different_nonce", &owner, PlanKindArg::Simple, 10_000_000, 2)
        .expect("a different nonce remains valid");
    // The plan's sketch counts one success here; the setup open was one,
    // so the quota is now two (the replay still counted for nothing).
    assert_eq!(w.user(owner_key).call_count, 2);
    assert_eq!(w.position(owner_key, 2).round_id, 2);
    assert_eq!(w.participants(2, 0).rows[0], owner_key);
    assert_eq!(w.pool().participant_count, 1);
    w.take_log()
}

pub fn unknown_outcome_retry_with_same_nonce_is_idempotent() -> Vec<Observation> {
    let row = ROW_UNKNOWN_OUTCOME_OPEN;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let vault = w.rt.game_vault_address();
    let vault_before = w.lamports(vault);

    // The client submitted and never saw the outcome.
    w.open_position("first_submission", &owner, PlanKindArg::Simple, SOL, 1)
        .expect("original submission");
    let after_first = w.snapshot_all();

    // It retries with the same nonce: a fresh transaction (new blockhash, new
    // signature) for the same Position PDA.
    let retry = w
        .open_position("", &owner, PlanKindArg::Simple, SOL, 1)
        .unwrap_err();
    assert_account_already_in_use(&retry);
    w.assert_fee_only(&after_first, &retry);

    // Exactly one Position is funded and the pot is credited once.
    assert_eq!(w.lamports(vault) - vault_before, SOL);
    let pool = w.pool();
    assert_eq!((pool.pot, pool.management), (960_000_000, 40_000_000));
    assert_eq!(pool.total_gross_deposits, u128::from(SOL));
    assert_eq!(w.user_round(owner_key, 1).deposits, u128::from(SOL));
    assert_eq!(w.user(owner_key).call_count, 1);
    assert_eq!(w.pool().participant_count, 1);
    w.take_log()
}

pub fn fund_series_a_retry_after_unknown_outcome_keeps_original_signature() -> Vec<Observation> {
    let row = ROW_SERIES_A_RETRY;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();

    // The client signs once and keeps the signature.
    let instruction = w.fund_series_a_instruction(owner_key, 50_000_000, 1);
    let signed = w.rt.sign(vec![instruction], &[&owner], crate::world::CU);
    let original = w.submit("original_submission", &signed).expect("original funding");
    let original_signature = original.signature.clone();
    assert_eq!(original_signature.as_deref(), Some(signed.signature().as_str()));
    let after_first = w.snapshot_all();

    // Re-presenting the identical signed bytes is not a second funding: the
    // runtime refuses the duplicate signature before execution and charges
    // nothing.  (The client reads finality, it does not auto-resubmit.)
    let replay = w.submit("", &signed).unwrap_err();
    assert_eq!(replay.signature, original_signature, "the original signature is kept");
    assert!(
        replay
            .error
            .as_deref()
            .is_some_and(|error| error.contains("AlreadyProcessed")),
        "{}: expected AlreadyProcessed, got {:?}",
        replay.label,
        replay.error
    );
    assert_eq!(replay.payer_lamports_before, replay.payer_lamports_after, "no fee");
    replay.assert_rollback_except_fee();
    w.assert_fee_only(&after_first, &replay);
    assert_eq!(w.pool().pot, 50_000_000, "credited once");

    // A deliberate second funding is a new transaction and merges into the
    // same Backer (repeatable ABI): entitlement 124 %, no second registry row.
    let first_sequence = w.backer(owner_key, 0).first_sequence;
    w.fund_series_a("second_funding_merges", &owner, 50_000_000, 1)
        .expect("deliberate second funding");
    let backer = w.backer(owner_key, 0);
    assert_eq!(backer.entitlement, 124_000_000);
    assert_eq!(backer.amount, 100_000_000);
    assert_eq!(backer.first_sequence, first_sequence, "funding sequence kept");
    let pool = w.pool();
    assert_eq!((pool.pot, pool.registry_len, pool.next_sequence), (100_000_000, 1, 2));
    assert_eq!(w.user(owner_key).call_count, 2);
    assert_eq!(
        w.last_event::<SeriesAFunded>().entitlement_after,
        124_000_000
    );
    w.take_log()
}

pub fn third_party_fee_payer_gains_no_authority() -> Vec<Observation> {
    let row = ROW_THIRD_PARTY_PAYER;
    let mut w = World::genesis().scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let attacker = w.attacker();
    let payer = w.rt.payer.pubkey();
    assert_ne!(payer, owner_key, "the fee payer is a third party");

    // The payer cannot open for the owner without the owner's signature.
    let open = w.open_instruction(owner_key, PlanKindArg::Simple, 100_000_000, 1, 1, 1);
    let before = w.snapshot_all();
    let unsigned = w.send("", vec![open.clone()], &[]).unwrap_err();
    assert!(
        unsigned.error.as_deref().is_some_and(|error| {
            error.contains("SignatureFailure") || error.contains("MissingRequiredSignature")
        }),
        "{}: expected a missing-signature refusal, got {:?}",
        unsigned.label,
        unsigned.error
    );
    unsigned.assert_rollback_except_fee();
    w.assert_fee_only(&before, &unsigned);
    assert!(w.account(w.rt.position_address(owner_key, 1)).is_none());

    // With the owner's signature the owner is the lamport source and the
    // third party pays the signature fee only.
    let (owner_before, vault_before) =
        (w.lamports(owner_key), w.lamports(w.rt.game_vault_address()));
    let opened = w
        .send("owner_signs_payer_pays_fee", vec![open], &[&owner])
        .expect("owner-signed open");
    assert_fee_paid(&opened);
    assert_eq!(opened.expected_fee_lamports, 10_000);
    assert_eq!(
        owner_before - w.lamports(owner_key),
        100_000_000 + w.rt.rent(state::Position::SPACE)
    );
    assert_eq!(
        w.lamports(w.rt.game_vault_address()) - vault_before,
        100_000_000
    );
    assert_eq!(w.position(owner_key, 1).owner, owner_key);

    // Signing as itself does not let a third party touch the owner's
    // Position: the Position PDA is derived from the signer.
    w.warp_seconds(DAY);
    let steal: Instruction = w.payout_instruction(
        PayoutKind::WithdrawSimple,
        attacker.pubkey(),
        owner_key,
        1,
        0,
        0,
        attacker.pubkey(),
        CloseRecordArg::Auto,
    );
    let before = w.snapshot_all();
    let rejected = w
        .send("attacker_signs_for_owners_position", vec![steal], &[&attacker])
        .unwrap_err();
    assert!(
        matches!(custom_code(&rejected), Some(2001 | 2006)),
        "{}: expected ConstraintHasOne (2001) or ConstraintSeeds (2006), got {:?}",
        rejected.label,
        rejected.error
    );
    rejected.assert_rollback_except_fee();
    w.assert_fee_only(&before, &rejected);
    w.take_log()
}

pub fn destination_cannot_be_game_vault_ops_cash_or_program_pda() -> Vec<Observation> {
    let row = ROW_DESTINATION;
    let mut w = World::genesis_with_simple_position(SOL).scoped(row, "");
    let owner = w.owner();
    let owner_key = owner.pubkey();
    w.warp_seconds(DAY);

    let protected: [(&str, Pubkey); 10] = [
        ("", w.rt.game_vault_address()),
        ("ops_cash", w.rt.ops_cash_address()),
        ("pool", w.rt.pool_address()),
        ("ops", w.rt.ops_address()),
        ("admission", w.rt.admission_address()),
        ("ticket", w.rt.ticket_address()),
        ("position", w.rt.position_address(owner_key, 1)),
        ("user_round", w.rt.user_round_address(1, owner_key)),
        ("config", w.rt.config_address(1)),
        ("program", musical_chairs_program::ID),
    ];
    for (variant, destination) in protected {
        let before = w.snapshot_all();
        let rejected = w
            .withdraw_simple(variant, &owner, 1, 0, destination, 0)
            .unwrap_err();
        if variant == "program" {
            // An executable account can never be writable, so Anchor's `mut`
            // constraint on `destination` rejects the program account (2000,
            // ConstraintMut) before the handler's ProtectedDestination check.
            w.assert_rejected_with_custom(&rejected, 2000);
        } else {
            w.assert_rejected(&rejected, ErrorCode::ProtectedDestination);
        }
        w.assert_fee_only(&before, &rejected);
    }

    // The same payout to an ordinary wallet is valid.
    let dest = w.dest;
    w.withdraw_simple("ordinary_wallet", &owner, 1, 0, dest, 0)
        .expect("ordinary destination");
    assert_eq!(w.lamports(dest), 96_800_000);
    w.take_log()
}
