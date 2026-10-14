//! IMPL-39b LiteSVM proof: the OPT-IN inline allocation inside a non-closing
//! payout (registry_len <= INLINE_THRESHOLD = 8, remaining accounts
//! `[Backer slot 0.., RegistryPage 0, SnapshotPage 0]`, all writable) leaves
//! EXACTLY the state the chunked ticket leaves, emits `PayoutPaid` (with the
//! committed ticket id) and `TicketCommitted`, and records the real packet
//! bytes, keys and compute of the inline transaction for N = 1..=8.  A registry
//! of 9 with accounts is refused `InvalidState`; a wrong layout is refused
//! before anything moves; with no remaining accounts the chunked ticket starts
//! as before.  Every rejection is fee-only; conservation holds after every
//! transaction (the World checks it).

use musical_chairs_financial_runtime::world::{BackerSpec, CloseRecordArg, PayoutKind, World, DAY, SOL};
use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    state, ErrorCode, PayoutPaid, TicketCommitted,
};
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey, signature::Signer};

fn owner_key(index: u64) -> Pubkey {
    let mut bytes = [0x5Au8; 32];
    bytes[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Pubkey::new_from_array(bytes)
}

/// N fixture Backers: alternating A/B, remaining entitlements that make the
/// ticket credit some and cap others (row 1 is capped by any toll above 20).
fn specs(n: u64) -> Vec<BackerSpec> {
    (0..n)
        .map(|i| {
            let series = if i % 2 == 0 { SERIES_A } else { SERIES_B };
            let remaining: u128 = match i % 4 {
                1 => 20,
                2 => 5_000_000,
                _ => 1_000_000_000,
            };
            BackerSpec::new(owner_key(i), series, remaining)
        })
        .collect()
}

fn world(n: u64, tag: &str) -> World {
    let mut w = World::genesis_with_simple_position(SOL).scoped("financial.inline", tag);
    w.install_backer_specs(&specs(n));
    w.warp_seconds(DAY);
    w
}

fn inline_metas(w: &World, n: u64) -> Vec<AccountMeta> {
    let mut metas: Vec<AccountMeta> = specs(n)
        .iter()
        .map(|s| AccountMeta::new(w.rt.backer_address(s.series, s.owner), false))
        .collect();
    metas.push(AccountMeta::new(w.rt.registry_address(0), false));
    metas.push(AccountMeta::new(w.rt.snapshot_address(0), false));
    metas
}

fn payout_ix(w: &World, extra: Vec<AccountMeta>) -> solana_sdk::instruction::Instruction {
    let owner = w.owner().pubkey();
    let mut ix = w.payout_instruction(PayoutKind::WithdrawSimple, owner, owner, 1, 0, 0, w.dest, CloseRecordArg::Auto);
    ix.accounts.extend(extra);
    ix
}

struct Snap {
    pool: (u64, u128, u64, u64, u64),
    backers: Vec<Vec<u8>>,
    registry: Vec<u8>,
    snapshot: Vec<u8>,
    ticket: Vec<u8>,
    digest: [u8; 32],
}

fn snap(w: &World, n: u64) -> Snap {
    let p = w.pool();
    Snap {
        pool: (p.seed, p.funded_claims, p.pending_toll, p.next_ticket, p.registry_len),
        backers: specs(n).iter().map(|s| w.data(w.rt.backer_address(s.series, s.owner))).collect(),
        registry: w.data(w.rt.registry_address(0)),
        snapshot: w.data(w.rt.snapshot_address(0)),
        ticket: w.data(w.rt.ticket_address()),
        digest: w.digest_financial_state(),
    }
}

#[test]
fn inline_payout_equals_the_chunked_ticket_for_one_to_eight_backers_and_records_its_cost() {
    for n in 1..=8u64 {
        // Chunked reference: the payout starts the ticket, the keeper drives it to IDLE.
        let mut chunked = world(n, &format!("n{n}.chunked"));
        let owner = chunked.owner();
        let ix = payout_ix(&chunked, vec![]);
        let paid = chunked.send("payout", vec![ix], &[&owner]).expect("chunked payout");
        let ev = paid.last_event::<PayoutPaid>();
        assert!(!ev.closing, "n{n}: non-closing");
        assert_eq!(chunked.ticket().phase, state::DISCOVER, "n{n}: the chunked ticket started");
        chunked.advance_until_idle(8);
        let reference = snap(&chunked, n);
        assert_eq!(reference.pool.2, 0, "n{n}: the chunked ticket consumed the toll");

        // Inline: the same payout with the registry's accounts appended.
        let mut inline = world(n, &format!("n{n}.inline"));
        let owner = inline.owner();
        let metas = inline_metas(&inline, n);
        let ix = payout_ix(&inline, metas);
        let obs = inline.send("", vec![ix], &[&owner]).expect("inline payout");
        let ev = obs.last_event::<PayoutPaid>();
        assert!(!ev.closing && ev.ticket_id != 0, "n{n}: PayoutPaid carries the committed ticket id");
        let committed = obs.last_event::<TicketCommitted>();
        assert_eq!(committed.ticket_id, ev.ticket_id, "n{n}: TicketCommitted for the same ticket");
        assert_eq!(inline.ticket().phase, state::IDLE, "n{n}: the ticket is IDLE after the payout");
        let actual = snap(&inline, n);
        assert_eq!(actual.pool, reference.pool, "n{n}: pool seed / funded_claims / pending_toll / next_ticket / registry_len");
        assert_eq!(actual.backers, reference.backers, "n{n}: every Backer account byte-identical");
        assert_eq!(actual.registry, reference.registry, "n{n}: registry page byte-identical");
        assert_eq!(actual.snapshot, reference.snapshot, "n{n}: snapshot page byte-identical");
        assert_eq!(actual.ticket, reference.ticket, "n{n}: ticket workspace byte-identical");
        assert_eq!(actual.digest, reference.digest, "n{n}: financial state digest");
        assert!(obs.fits_legacy_packet, "n{n}: inline payout fits the legacy packet ({} B)", obs.packet_bytes);
        assert!(obs.message_account_count <= 64);
        println!(
            "INLINE n={n} packet_bytes={} keys={} writable={} cu={} toll={} credited={} seed={}",
            obs.packet_bytes,
            obs.message_account_count,
            obs.writable_account_count,
            obs.compute_units.unwrap_or(0),
            committed.toll,
            committed.credited,
            committed.seed
        );
    }
}

#[test]
fn nine_backers_with_accounts_is_refused_and_wrong_layouts_move_nothing() {
    // N = 9: more than one page, inline is not offered even with accounts.
    let mut w = world(9, "n9.refused");
    let owner = w.owner();
    let mut metas: Vec<AccountMeta> = specs(9)
        .iter()
        .map(|s| AccountMeta::new(w.rt.backer_address(s.series, s.owner), false))
        .collect();
    metas.push(AccountMeta::new(w.rt.registry_address(0), false));
    metas.push(AccountMeta::new(w.rt.snapshot_address(0), false));
    metas.push(AccountMeta::new(w.rt.registry_address(1), false));
    metas.push(AccountMeta::new(w.rt.snapshot_address(1), false));
    let before = w.snapshot_all();
    let rejected = w.send("", vec![payout_ix(&w, metas)], &[&owner]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::InvalidState);
    w.assert_fee_only(&before, &rejected);

    // N = 4 with the pages first: a wrong layout is refused before any cash moves.
    let mut w = world(4, "n4.pages_first");
    let owner = w.owner();
    let mut metas = vec![AccountMeta::new(w.rt.registry_address(0), false), AccountMeta::new(w.rt.snapshot_address(0), false)];
    metas.extend(specs(4).iter().map(|s| AccountMeta::new(w.rt.backer_address(s.series, s.owner), false)));
    let before = w.snapshot_all();
    let rejected = w.send("", vec![payout_ix(&w, metas)], &[&owner]).unwrap_err();
    assert!(!rejected.success);
    let code = rejected.error.clone().unwrap_or_default();
    assert!(code.contains("Custom(6005)") || code.contains("Custom(6006)"), "pages-first layout refused as WrongAccount/WrongOwner, got {code}");
    rejected.assert_rollback_except_fee();
    w.assert_fee_only(&before, &rejected);

    // N = 4 without the pages (partial layout) → WrongAccount.
    let mut w = world(4, "n4.partial");
    let owner = w.owner();
    let metas: Vec<AccountMeta> = specs(4).iter().map(|s| AccountMeta::new(w.rt.backer_address(s.series, s.owner), false)).collect();
    let before = w.snapshot_all();
    let rejected = w.send("", vec![payout_ix(&w, metas)], &[&owner]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::WrongAccount);
    w.assert_fee_only(&before, &rejected);

    // N = 4 with one Backer passed twice → DuplicateWritableAccount.
    let mut w = world(4, "n4.duplicate");
    let owner = w.owner();
    let mut metas = inline_metas(&w, 4);
    metas[1] = metas[0].clone();
    let before = w.snapshot_all();
    let rejected = w.send("", vec![payout_ix(&w, metas)], &[&owner]).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::DuplicateWritableAccount);
    w.assert_fee_only(&before, &rejected);
}

// ---------------------------------------------------------------------------
// Codex M9 correction 1 (IMPL-39c): the SECOND inline entry point.  Step 2(b)
// exposes the inline ticket through `withdraw_simple` AND `settle_compounding`;
// the rows above only drive the first.  A matured Compound15 / Compound30
// settlement with the registry's accounts appended must leave exactly the
// chunked ticket's state, carry the same Jackpot toll in `PayoutPaid` and
// `TicketCommitted`, pay the owner the same net, record the real packet/CU,
// and refuse a malformed remaining-account layout fee-only (the position stays
// open and the well-formed retry succeeds).  Same production ELF; no rebuild.
// ---------------------------------------------------------------------------

use musical_chairs_program::positions::PlanKindArg;

fn plan_of(name: &str) -> PlanKindArg {
    match name {
        "compound15" => PlanKindArg::Compound15,
        "compound30" => PlanKindArg::Compound30,
        other => panic!("unknown plan {other}"),
    }
}

/// A matured compounding position (owner, nonce 1) over a pot large enough
/// to pay it SOLVENTLY (a Compound30 gross exceeds a 10-SOL pot, which would
/// route the payout to the closing path and need provisioned close pages:
/// `PageNotProvisioned`), plus N fixture Backers so the Jackpot toll routes
/// to a non-closing ticket.
fn compounding_world(plan: &str, days: u64, n: u64, tag: &str) -> World {
    let mut w = World::genesis_with_position(plan_of(plan), SOL).scoped("financial.inline.compounding", tag);
    w.set_pot_fixture(100 * SOL);
    w.install_backer_specs(&specs(n));
    w.warp_seconds(days * DAY);
    w
}

fn settle_ix(w: &World, extra: Vec<AccountMeta>) -> solana_sdk::instruction::Instruction {
    let owner = w.owner().pubkey();
    let mut ix = w.payout_instruction(PayoutKind::SettleCompounding, owner, owner, 1, 0, 0, w.dest, CloseRecordArg::Auto);
    ix.accounts.extend(extra);
    ix
}

#[test]
fn matured_compounding_inline_settlement_equals_the_chunked_ticket_for_both_plans() {
    // (plan, maturity days, Jackpot fee bps): the fee rates are the frozen
    // `ConfigTerms` pins `jackpot_bps: [900, 1_300]` (state.rs); the
    // Compound15 toll additionally equals the positions suite's
    // `compound_15_maturity_vector` value 402,620,918.
    for (plan, days, jackpot_bps) in [("compound15", 15u64, 900u64), ("compound30", 30u64, 1_300u64)] {
        for n in [1u64, 4, 8] {
            // Chunked reference: the settlement starts the ticket, the keeper drives it to IDLE.
            let mut chunked = compounding_world(plan, days, n, &format!("{plan}.n{n}.chunked"));
            let owner = chunked.owner();
            let dest_before = chunked.lamports(chunked.dest);
            let paid = chunked.send("settle", vec![settle_ix(&chunked, vec![])], &[&owner]).expect("chunked settlement");
            let ev = paid.last_event::<PayoutPaid>();
            assert!(!ev.closing, "{plan} n{n}: non-closing");
            assert!(ev.toll > 0, "{plan} n{n}: the Jackpot toll is charged at maturity");
            assert_eq!(ev.toll, ev.gross_paid * jackpot_bps / 10_000, "{plan} n{n}: Jackpot toll = gross × the pinned jackpot_bps");
            if plan == "compound15" {
                assert_eq!(ev.toll, 402_620_918, "{plan} n{n}: Jackpot toll pinned by the positions suite");
            }
            let reference_event = (ev.gross_paid, ev.toll, ev.net);
            assert_eq!(chunked.lamports(chunked.dest) - dest_before, ev.net, "{plan} n{n}: owner received the net");
            assert_eq!(chunked.ticket().phase, state::DISCOVER, "{plan} n{n}: the chunked ticket started");
            chunked.advance_until_idle(8);
            let reference = snap(&chunked, n);
            assert_eq!(reference.pool.2, 0, "{plan} n{n}: the chunked ticket consumed the toll");

            // Inline: the same settlement with the registry's accounts appended.
            let mut inline = compounding_world(plan, days, n, &format!("{plan}.n{n}.inline"));
            let owner = inline.owner();
            let dest_before = inline.lamports(inline.dest);
            let obs = inline.send("", vec![settle_ix(&inline, inline_metas(&inline, n))], &[&owner]).expect("inline settlement");
            let ev = obs.last_event::<PayoutPaid>();
            assert_eq!((ev.gross_paid, ev.toll, ev.net), reference_event, "{plan} n{n}: PayoutPaid gross / Jackpot toll / net equal the chunked run");
            assert!(!ev.closing && ev.ticket_id != 0, "{plan} n{n}: PayoutPaid carries the committed ticket id");
            let committed = obs.last_event::<TicketCommitted>();
            assert_eq!(committed.ticket_id, ev.ticket_id, "{plan} n{n}: TicketCommitted for the same ticket");
            assert_eq!(committed.toll, ev.toll, "{plan} n{n}: the committed ticket carried the whole Jackpot toll");
            assert_eq!(inline.lamports(inline.dest) - dest_before, ev.net, "{plan} n{n}: owner received the same net");
            assert!(inline.position(owner.pubkey(), 1).claimed, "{plan} n{n}: the position is settled");
            assert_eq!(inline.ticket().phase, state::IDLE, "{plan} n{n}: the ticket is IDLE after the settlement");
            let actual = snap(&inline, n);
            assert_eq!(actual.pool, reference.pool, "{plan} n{n}: pool seed / funded_claims / pending_toll / next_ticket / registry_len");
            assert_eq!(actual.backers, reference.backers, "{plan} n{n}: every Backer account byte-identical");
            assert_eq!(actual.registry, reference.registry, "{plan} n{n}: registry page byte-identical");
            assert_eq!(actual.snapshot, reference.snapshot, "{plan} n{n}: snapshot page byte-identical");
            assert_eq!(actual.ticket, reference.ticket, "{plan} n{n}: ticket workspace byte-identical");
            assert_eq!(actual.digest, reference.digest, "{plan} n{n}: financial state digest");
            assert!(obs.fits_legacy_packet, "{plan} n{n}: inline settlement fits the legacy packet ({} B)", obs.packet_bytes);
            assert!(obs.message_account_count <= 64);
            println!(
                "INLINE_COMPOUNDING plan={plan} n={n} packet_bytes={} keys={} writable={} cu={} gross={} toll={} net={} credited={} seed={}",
                obs.packet_bytes,
                obs.message_account_count,
                obs.writable_account_count,
                obs.compute_units.unwrap_or(0),
                ev.gross_paid,
                ev.toll,
                ev.net,
                committed.credited,
                committed.seed
            );
        }

        // Malformed layouts on the matured position move nothing and leave it open.
        // Partial layout (Backers without the pages) → WrongAccount.
        let mut w = compounding_world(plan, days, 4, &format!("{plan}.n4.partial"));
        let owner = w.owner();
        let metas: Vec<AccountMeta> = specs(4).iter().map(|s| AccountMeta::new(w.rt.backer_address(s.series, s.owner), false)).collect();
        let before = w.snapshot_all();
        let rejected = w.send("", vec![settle_ix(&w, metas)], &[&owner]).unwrap_err();
        w.assert_rejected(&rejected, ErrorCode::WrongAccount);
        w.assert_fee_only(&before, &rejected);
        assert!(!w.position(owner.pubkey(), 1).claimed, "{plan}: the partial layout settled nothing");

        // One Backer passed twice → DuplicateWritableAccount; then the well-formed inline retry succeeds.
        let mut w = compounding_world(plan, days, 4, &format!("{plan}.n4.duplicate"));
        let owner = w.owner();
        let mut metas = inline_metas(&w, 4);
        metas[1] = metas[0].clone();
        let before = w.snapshot_all();
        let rejected = w.send("", vec![settle_ix(&w, metas)], &[&owner]).unwrap_err();
        w.assert_rejected(&rejected, ErrorCode::DuplicateWritableAccount);
        w.assert_fee_only(&before, &rejected);
        assert!(!w.position(owner.pubkey(), 1).claimed, "{plan}: the duplicate layout settled nothing");
        w.send("", vec![settle_ix(&w, inline_metas(&w, 4))], &[&owner]).expect("well-formed retry after a malformed layout");
        assert!(w.position(owner.pubkey(), 1).claimed, "{plan}: the retry settled the position inline");
        assert_eq!(w.ticket().phase, state::IDLE, "{plan}: the retry committed the ticket");
    }
}
