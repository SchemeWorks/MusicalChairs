//! IMPL-43: the owner's unused close reserve is owed to the owner's immutable
//! refund destination (spec section 8.5 / DR-14) and is never forfeited.
//!
//! Three closes, each a zero-pot closing payout (so the destination receives
//! nothing from the payout) with the owner funding 500,000 lamports of the
//! reserve, below the local rent-exempt minimum:
//!
//! * `fresh_destination`: an empty destination cannot take a sub-minimum
//!   credit, so settlement DEFERS the remainder (it stays in
//!   `locked_owner_remaining`, held in OpsCash); `deliver_owner_refund` is
//!   fee-only `RefundNotDeliverable` until the destination is funded, then pays
//!   exactly once; a replay is `NothingDeferred`; a wrong destination is
//!   `WrongAccount`.
//! * `legacy_rent_paying_destination`: a 1-lamport destination (rent-paying)
//!   cannot be credited while staying below the minimum (Agave rent-state
//!   rule); expiry still succeeds and the remainder is deferred, not lost.
//! * `deliverable_destination`: a funded destination is paid at settlement and
//!   nothing is deferred.
//!
//! RED on the IMPL-32 program (8fdade5): the first case forfeited the
//! remainder to `unallocated_excess` ("owed 0, paid 0"); see the IMPL-43
//! receipt for the captured run.

use musical_chairs_financial_runtime::runtime::anchor_ix;
use musical_chairs_financial_runtime::world::World;
use musical_chairs_program::{accounts, instruction, state, ErrorCode, OwnerRefundDelivered, ID};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Signer};
use solana_system_interface::program as system_program;

const OWNER_PART: u64 = 500_000;
const ROW: &str = "financial.close.owner_refund_never_forfeited";

fn expire_ix(w: &World) -> Instruction {
    anchor_ix(
        accounts::ExpirePromotion {
            pool: w.rt.pool_address(),
            close_record: w.rt.close_record_address(1),
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            owner_refund: w.dest,
            system_program: system_program::ID,
            event_authority: w.rt.event_authority_address(),
            program: ID,
        },
        instruction::ExpirePromotion {},
    )
}

fn deliver_ix(w: &World, destination: Pubkey) -> Instruction {
    anchor_ix(
        accounts::DeliverOwnerRefund {
            close_record: w.rt.close_record_address(1),
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            owner_refund: destination,
            system_program: system_program::ID,
            event_authority: w.rt.event_authority_address(),
            program: ID,
        },
        instruction::DeliverOwnerRefund {},
    )
}

/// A closing payout against an empty pot with the owner funding `OWNER_PART`
/// of the reserve; returns the World in Closing with the record bound and the
/// reserve it locked.
fn closing_world(variant: &str, destination_lamports: u64) -> (World, u64) {
    let mut w = World::closing_fixture(1, 0).scoped(ROW, variant);
    w.pay_fees_as_owner();
    let owner = w.owner();
    let dest = w.dest;
    if destination_lamports > 0 {
        w.rt.credit_lamports(dest, destination_lamports);
    }
    w.set_pot_fixture(0);
    let reserve = w.expected_reserve(1, 0, 0, 0);
    assert!(reserve > OWNER_PART);
    w.set_ops_free(reserve - OWNER_PART);
    w.withdraw_simple("closing_payout", &owner, 1, 0, dest, OWNER_PART)
        .expect("zero-pot closing payout with an owner contribution");
    let record = w.close_record(1);
    assert_eq!(record.locked_owner_remaining, OWNER_PART);
    assert_eq!(record.locked_sponsor_remaining, reserve - OWNER_PART);
    assert_eq!(record.owner_refund_destination, dest);
    assert_eq!(w.lamports(dest), destination_lamports, "a zero-pot payout pays the destination nothing");
    assert_eq!(record.promotion, state::Promotion::Pending);
    assert_eq!(record.promotion_deadline, record.close_started_at + 600);
    (w, reserve)
}

/// After a terminal promotion: the owner's unused reserve is paid or still owed
/// on the CloseRecord, never anonymous excess; the sponsor remainder is free.
fn settled_for_the_owner(w: &World, dest_before: u64, excess_before: u64, reserve: u64) -> u64 {
    let record = w.close_record(1);
    let ops = w.ops();
    let paid = w.lamports(w.dest) - dest_before;
    assert_eq!(
        record.locked_owner_remaining + paid,
        OWNER_PART,
        "the owner's unused reserve is paid or still owed on the CloseRecord (owed {}, paid {paid})",
        record.locked_owner_remaining
    );
    assert_eq!(ops.unallocated_excess, excess_before, "owner lamports never become anonymous excess");
    assert_eq!(ops.free_balance, reserve - OWNER_PART, "the sponsor remainder returned to free");
    assert_eq!(ops.locked_remaining_total, record.locked_owner_remaining, "only the deferred owner refund stays locked");
    assert_eq!(record.locked_sponsor_remaining, 0);
    assert!(record.reserve_settled, "settlement completes");
    assert!(matches!(record.promotion, state::Promotion::Skipped { .. }), "promotion is terminal");
    w.assert_ops_cash_identity();
    paid
}

fn deliver_and_check(w: &mut World, minimum: u64) {
    // Not deliverable yet: fee-only, lamports stay owed.
    let before = w.snapshot_all();
    let rejected = w.send("deliver_before_funding", vec![deliver_ix(w, w.dest)], &[]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::RefundNotDeliverable);
    w.assert_fee_only(&before, &rejected);
    assert_eq!(w.close_record(1).locked_owner_remaining, OWNER_PART);

    // The wrong destination is refused even when it could receive the credit.
    let attacker = w.attacker().pubkey();
    w.rt.credit_lamports(attacker, minimum);
    let before = w.snapshot_all();
    let rejected = w.send("deliver_wrong_destination", vec![deliver_ix(w, attacker)], &[]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::WrongAccount);
    w.assert_fee_only(&before, &rejected);

    // Once the destination can take the credit, anyone delivers it exactly once.
    let dest = w.dest;
    w.rt.credit_lamports(dest, minimum);
    let dest_before = w.lamports(dest);
    let cash_before = w.lamports(w.rt.ops_cash_address());
    let delivered = w.send("deliver", vec![deliver_ix(w, dest)], &[]).expect("delivery once the destination is funded");
    assert!(delivered.success);
    assert_eq!(w.lamports(dest) - dest_before, OWNER_PART, "the whole deferred refund reaches the destination");
    assert_eq!(cash_before - w.lamports(w.rt.ops_cash_address()), OWNER_PART, "paid out of OpsCash");
    let record = w.close_record(1);
    assert_eq!((record.locked_owner_remaining, w.ops().locked_remaining_total), (0, 0));
    assert!(record.reserve_settled);
    let event = w.last_event::<OwnerRefundDelivered>();
    assert_eq!((event.close_id, event.round_id, event.amount, event.destination), (1, 1, OWNER_PART, dest));
    w.assert_ops_cash_identity();

    // Exactly once.
    let before = w.snapshot_all();
    let rejected = w.send("deliver_replay", vec![deliver_ix(w, dest)], &[]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::NothingDeferred);
    w.assert_fee_only(&before, &rejected);
}

#[test]
fn fresh_destination_remainder_is_deferred_then_delivered_once() {
    let (mut w, reserve) = closing_world("fresh_destination", 0);
    let minimum = w.rt.rent(0);
    assert!(OWNER_PART < minimum, "the scenario needs a sub-minimum remainder (minimum {minimum})");
    let (dest_before, excess_before) = (w.lamports(w.dest), w.ops().unallocated_excess);
    w.warp_seconds(601);
    let ix = expire_ix(&w);
    w.send("expire", vec![ix], &[]).expect("expiry must succeed");
    let paid = settled_for_the_owner(&w, dest_before, excess_before, reserve);
    assert_eq!(paid, 0, "an empty destination cannot take a sub-minimum credit: deferred");
    deliver_and_check(&mut w, minimum);
}

#[test]
fn legacy_rent_paying_destination_does_not_block_expiry() {
    let (mut w, reserve) = closing_world("legacy_rent_paying_destination", 1);
    let minimum = w.rt.rent(0);
    let (dest_before, excess_before) = (w.lamports(w.dest), w.ops().unallocated_excess);
    w.warp_seconds(601);
    let ix = expire_ix(&w);
    let observation = match w.send("expire", vec![ix], &[]) {
        Ok(observation) => observation,
        Err(observation) => panic!("expiry blocked by the owner destination: {:?}", observation.error),
    };
    assert!(observation.success);
    let paid = settled_for_the_owner(&w, dest_before, excess_before, reserve);
    assert_eq!(paid, 0, "a rent-paying destination cannot be topped up below the minimum: deferred");
    deliver_and_check(&mut w, minimum);
}

#[test]
fn deliverable_destination_is_paid_at_settlement_and_nothing_is_deferred() {
    let minimum = World::genesis().rt.rent(0);
    let (mut w, reserve) = closing_world("deliverable_destination", minimum);
    let (dest_before, excess_before) = (w.lamports(w.dest), w.ops().unallocated_excess);
    w.warp_seconds(601);
    let ix = expire_ix(&w);
    w.send("expire", vec![ix], &[]).expect("expiry must succeed");
    let paid = settled_for_the_owner(&w, dest_before, excess_before, reserve);
    assert_eq!(paid, OWNER_PART, "a funded destination is paid at settlement");
    let before = w.snapshot_all();
    let rejected = w.send("deliver_nothing_deferred", vec![deliver_ix(&w, w.dest)], &[]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::NothingDeferred);
    w.assert_fee_only(&before, &rejected);
}

#[test]
fn delivery_before_settlement_is_refused() {
    let (mut w, _) = closing_world("pending_close", 0);
    let before = w.snapshot_all();
    let rejected = w.send("deliver_while_pending", vec![deliver_ix(&w, w.dest)], &[]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::InvalidState);
    w.assert_fee_only(&before, &rejected);
    assert_eq!(w.close_record(1).locked_owner_remaining, OWNER_PART);
}
