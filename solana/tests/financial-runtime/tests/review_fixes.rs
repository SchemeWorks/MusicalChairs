//! IMPL-40a (plan M10 step 3): LiteSVM rows for two program corrections from
//! the review personas, written RED against the pre-fix ELF `d77b23bf…`.
//!
//! * ECO-1 / SEC-3 (dust subset): the host reference model (`Book::allocate`)
//!   sends a toll whose three Backer slices all floor to zero (toll <= 4 at
//!   35/45/20) straight to the seed, with no ticket; the program started a
//!   full ticket for any positive toll with a non-empty registry.  The ticket
//!   credits nothing and seeds everything, so routing the dust to the seed is
//!   the same end state minus 6..387 keeper transactions and the
//!   `AllocationPending` barrier.  A toll of 5 or more still starts a ticket.
//! * ECO-3: `sync_game` / `sync_ops` classified a rent-floor DECREASE as
//!   excess and then refused every instruction (`InvalidState`) once the floor
//!   rose again, because `recorded` included that excess.  Excess now floats:
//!   it is the residual over the owned partitions at the live floor, so a rise
//!   within the classified excess is absorbed.  Excess is never spendable, so
//!   the economics are unchanged.
use musical_chairs_financial_runtime::world::{BackerSpec, World, SOL};
use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    positions::PlanKindArg,
    state, PayoutPaid,
};
use solana_sdk::{pubkey::Pubkey, rent::Rent, signature::Signer};

const CU: u32 = 1_400_000;
const POSITION: u64 = 10_000_000; // the Simple minimum: 10M × 11 %/day = 12.73 lamports per second

fn backer_key(index: u8) -> Pubkey {
    Pubkey::new_from_array([0x60 + index; 32])
}

/// A 10M-lamport Simple position plus one open A and one open B Backer.
fn dust_world(tag: &str) -> World {
    let mut w = World::genesis_with_simple_position(POSITION).scoped("financial.review.dust_toll", tag);
    w.install_backer_specs(&[
        BackerSpec::new(backer_key(0), SERIES_A, 1_000_000_000),
        BackerSpec::new(backer_key(1), SERIES_B, 1_000_000_000),
    ]);
    w
}

#[test]
fn a_toll_whose_slices_all_floor_to_zero_goes_to_the_seed_without_a_ticket() {
    // Two seconds of accrual: owed 25, Carried Interest 12 % → toll 3.
    let mut w = dust_world("");
    let owner = w.owner();
    w.warp_seconds(2);
    let before = w.pool();
    let backers_before: Vec<Vec<u8>> = [(SERIES_A, backer_key(0)), (SERIES_B, backer_key(1))]
        .iter()
        .map(|(s, k)| w.data(w.rt.backer_address(*s, *k)))
        .collect();
    w.withdraw_simple("", &owner, 1, 0, w.dest, 0).expect("dust payout");
    let ev = w.last_event::<PayoutPaid>();
    assert!((1..=4).contains(&ev.toll), "the fixture must produce a dust toll, got {}", ev.toll);
    if ev.ticket_id != 0 {
        // Pre-fix evidence: the ticket that was started credits nothing and seeds the whole toll.
        w.advance_until_idle(8);
        let after = w.pool();
        let backers_after: Vec<Vec<u8>> = [(SERIES_A, backer_key(0)), (SERIES_B, backer_key(1))]
            .iter()
            .map(|(s, k)| w.data(w.rt.backer_address(*s, *k)))
            .collect();
        println!(
            "PRE_FIX_TICKET toll={} seed_delta={} funded_claims_delta={} backers_unchanged={}",
            ev.toll,
            after.seed - before.seed,
            after.funded_claims - before.funded_claims,
            backers_after == backers_before
        );
    }
    assert_eq!(ev.ticket_id, 0, "a dust toll must not start a ticket (host Book::allocate parity)");
    let after = w.pool();
    assert_eq!(w.ticket().phase, state::IDLE, "no ticket is open");
    assert_eq!(after.pending_toll, 0, "nothing is pending");
    assert_eq!(after.next_ticket, before.next_ticket, "no ticket id was consumed");
    assert_eq!(after.seed, before.seed + ev.toll, "the whole dust toll seeds");
    assert_eq!(after.funded_claims, before.funded_claims, "no Backer was credited");
}

#[test]
fn a_toll_of_five_or_more_still_starts_the_ticket() {
    // Four seconds: owed 50, toll 6 → half 3 → the senior slice is 1: a ticket.
    let mut w = dust_world("ticket_at_six");
    let owner = w.owner();
    w.warp_seconds(4);
    let before = w.pool();
    w.withdraw_simple("", &owner, 1, 0, w.dest, 0).expect("payout");
    let ev = w.last_event::<PayoutPaid>();
    assert!(ev.toll >= 5, "the fixture must produce a ticket-bearing toll, got {}", ev.toll);
    assert_ne!(ev.ticket_id, 0, "a toll of {} starts a ticket", ev.toll);
    assert_eq!(w.ticket().phase, state::DISCOVER);
    assert_eq!(w.pool().pending_toll, ev.toll);
    w.advance_until_idle(8);
    let after = w.pool();
    assert_eq!(after.pending_toll, 0);
    assert_eq!(after.seed + u64::try_from(after.funded_claims - before.funded_claims).unwrap(), before.seed + ev.toll, "toll = seed delta + credited");
}

#[test]
fn a_rent_floor_rise_within_the_classified_excess_does_not_freeze_the_vault() {
    let mut w = World::genesis().scoped("financial.review.rent_ratchet", "");
    let owner = w.owner();
    let pool = w.pool();
    let open_low = w.open_instruction(owner.pubkey(), PlanKindArg::Simple, SOL, 1, pool.round_id, pool.active_config_version);
    let original: Rent = w.rt.svm.get_sysvar::<Rent>();
    let mut lowered = original.clone();
    lowered.lamports_per_byte_year = original.lamports_per_byte_year / 10;

    // A deposit while the floor is one tenth: the freed 90 % of the vault's
    // and OpsCash's floors are classified as excess at that floor.
    w.rt.svm.set_sysvar(&lowered);
    let low = w.rt.execute("financial.review.rent_ratchet.open_under_lowered_floor".to_string(), vec![open_low], &[&owner], CU);
    assert!(low.success, "open under the lowered floor: {:?}", low.error);
    w.rt.svm.set_sysvar(&original);
    let classified = w.pool().excess;
    assert!(classified > 0, "the floor decrease was classified as excess ({classified})");

    // The floor is back: a second deposit must sync, with the excess floating
    // back to the residual over the owned partitions, and conservation holds.
    let pool = w.pool();
    let open_restored = w.open_instruction(owner.pubkey(), PlanKindArg::Simple, SOL, 2, pool.round_id, pool.active_config_version);
    let restored = w.rt.execute("financial.review.rent_ratchet.open_after_the_floor_rose".to_string(), vec![open_restored], &[&owner], CU);
    assert!(restored.success, "the vault must not freeze when the floor rises within the classified excess: {:?}", restored.error);
    assert_eq!(w.pool().excess, 0, "excess floated back to the residual");
    w.rt.conservation_check();
    assert_eq!(w.position(owner.pubkey(), 2).principal, SOL);
}
