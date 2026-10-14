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
