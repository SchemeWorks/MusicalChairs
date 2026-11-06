//! M6 allocation suite.
//!
//! Every row runs the actual compiled Musical Chairs SBF in LiteSVM through a
//! [`World`] and drives the six allocation-ticket instructions
//! (`discover_rows`, `apply_senior`, `seal_weights`, `allocate_rows`,
//! `prune_rows`, `commit_ticket`) and `claim_backer` with real lamports.  Rows
//! follow the milestone 6 test list of
//! `docs/superpowers/plans/2026-10-13-native-solana-implementation.md` and keep
//! its numbers.
//!
//! # Labels
//!
//! `financial.allocation.m6.<row>` is a row's headline observation;
//! `financial.allocation.m6.<row>.<variant>` are the supporting ones.  A driven
//! ticket labels each phase transaction `<phase>.<ordinal>` (`discover.0`,
//! `allocate.3`, `commit.0`, ...) under its world's scope.  The chunk-size row
//! scopes each pass `...rows-<R>` and the registry-size row `...n-<N>`, so the
//! coordinator can tabulate compute units and packet bytes per phase
//! instruction and batch size straight from the labels.
//!
//! A row whose headline is a driven phase transaction (a golden's `commit`)
//! relabels that one observation to the bare row label (see [`promote`]); no
//! observation is duplicated.
//!
//! # Fixtures
//!
//! Backers are fixtures ([`World::with_backers`], [`World::with_random_backers`]):
//! entitlement = remaining entitlement, `lifetime` 0, sequential
//! `first_sequence`, registry rows that are the Backer PDAs, and the registry
//! and snapshot pages their slots imply.  A ticket is started by
//! [`World::start_ticket_fixture`], which mirrors `positions.rs::ticket_start`
//! and escrows the toll as real vault lamports; rows that need the
//! program-started ticket run a real `withdraw_simple` instead (the
//! normal-deposit row does).
//!
//! # Assertions
//!
//! Every rejection asserts the exact program error code parsed from
//! `Observation.error` and fee-only rollback of every account the World can
//! touch.  Every success is followed by both cash-register identities (checked
//! by the World after every recorded transaction) and rows assert the exact
//! credits, books and events.  Events are decoded from the event-CPI inner
//! instructions, never from logs.
//!
//! Where the plan's sketch cannot be taken literally against this program the
//! deviation is stated next to the row: a Backer holding 7 lamports is
//! CLOSED (`<= 10`) and is never counted, so the u256 row uses a 70-lamport
//! open row (and keeps a closed 7 to prove it is not counted); a tx that has
//! to carry the sketch's `Phase::SealWeights` stops at `state::SEAL`.

use musical_chairs_economics::backers::{BackerKey, Book, Series};
use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    positions::PlanKindArg,
    state::{self, ALLOCATE, COMMIT, DISCOVER, IDLE, PRUNE, SEAL, SENIOR},
    ErrorCode, TicketCommitted, ID,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
use crate::runtime::{Observation, OWNER_SEED};
use crate::world::{allocation_owner, random_backer_specs, BackerSpec, World, DAY, SOL};

/// Root of every label of this suite.
pub const ROOT: &str = "financial.allocation.m6";

const A: u8 = SERIES_A;
const B: u8 = SERIES_B;

pub const ROW_GOLDEN_T10000: &str = "golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500";
pub const ROW_GOLDEN_NO_B: &str = "golden_no_b_a1000_a10000_seed_5750_a1000_a3250";
pub const ROW_GOLDEN_ONLY_A_AND_B1: &str =
    "golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed";
pub const ROW_CHUNK_SIZES: &str = "chunk_sizes_1_4_8_produce_identical_state";
pub const ROW_REPLAY: &str = "replaying_a_completed_discover_chunk_fails_without_credit";
pub const ROW_STALE_TICKET: &str = "stale_ticket_id_is_rejected_on_every_phase_instruction";
pub const ROW_U256: &str = "discovery_accumulates_a_remainders_above_u128_in_u256";
pub const ROW_SAME_PAGE_SWAP: &str = "prune_same_page_swap_delete_clears_last_slot_on_first_page";
pub const ROW_PRUNE_ALIAS: &str = "prune_alias_when_pruned_row_is_last_row_marks_it_inactive";
pub const ROW_DUPLICATES: &str =
    "duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once";
pub const ROW_CLAIM_DURING_TICKET: &str =
    "claim_during_ticket_pays_only_finalized_and_commit_adds_once";
pub const ROW_CLAIM_CAP: &str =
    "claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history";
pub const ROW_REOPEN: &str = "reopen_capped_a_keeps_first_funding_seq";
pub const ROW_NORMAL_DEPOSIT: &str =
    "normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait";
pub const ROW_CONSERVATION: &str = "conservation_identity_holds_after_every_phase";
// Rows beyond the plan list: CU / packet capture and two ABI hazards.
pub const ROW_SIZES: &str = "registry_sizes_16_64_100_256_fit_the_legacy_packet_at_8_rows";
pub const ROW_MALFORMED: &str = "malformed_batches_are_invalid_state_and_fee_only";
pub const ROW_SENIOR_NO_A: &str = "senior_without_an_open_a_needs_a_writable_stand_in";

/// Every row, in suite order.
pub const ROW_NAMES: [&str; 18] = [
    ROW_GOLDEN_T10000,
    ROW_GOLDEN_NO_B,
    ROW_GOLDEN_ONLY_A_AND_B1,
    ROW_CHUNK_SIZES,
    ROW_REPLAY,
    ROW_STALE_TICKET,
    ROW_U256,
    ROW_SAME_PAGE_SWAP,
    ROW_PRUNE_ALIAS,
    ROW_DUPLICATES,
    ROW_CLAIM_DURING_TICKET,
    ROW_CLAIM_CAP,
    ROW_REOPEN,
    ROW_NORMAL_DEPOSIT,
    ROW_CONSERVATION,
    ROW_SIZES,
    ROW_MALFORMED,
    ROW_SENIOR_NO_A,
];

/// The whole suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_allocation_suite() -> Vec<Observation> {
    let rows: [fn() -> Vec<Observation>; 18] = [
        golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500,
        golden_no_b_a1000_a10000_seed_5750_a1000_a3250,
        golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed,
        chunk_sizes_1_4_8_produce_identical_state,
        replaying_a_completed_discover_chunk_fails_without_credit,
        stale_ticket_id_is_rejected_on_every_phase_instruction,
        discovery_accumulates_a_remainders_above_u128_in_u256,
        prune_same_page_swap_delete_clears_last_slot_on_first_page,
        prune_alias_when_pruned_row_is_last_row_marks_it_inactive,
        duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once,
        claim_during_ticket_pays_only_finalized_and_commit_adds_once,
        claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history,
        reopen_capped_a_keeps_first_funding_seq,
        normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait,
        conservation_identity_holds_after_every_phase,
        registry_sizes_16_64_100_256_fit_the_legacy_packet_at_8_rows,
        malformed_batches_are_invalid_state_and_fee_only,
        senior_without_an_open_a_needs_a_writable_stand_in,
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
/// `financial.allocation.m6.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = scope(row);
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

fn scope(row: &str) -> String {
    format!("{ROOT}.{row}")
}

/// The `index`-th named fixture owner of a row.
fn who(index: u64) -> Pubkey {
    allocation_owner(index)
}

fn owner_signer() -> Keypair {
    Keypair::new_from_array(OWNER_SEED)
}

/// Relabel the one observation labelled `from` as `to` (a row's headline).
fn promote(log: &mut [Observation], from: &str, to: &str) {
    let observation = log
        .iter_mut()
        .find(|observation| observation.label == from)
        .unwrap_or_else(|| panic!("no observation labelled {from} to promote"));
    observation.label = to.to_string();
}

/// Send `instruction` (no signer beyond the fee payer), expect the exact
/// program error and fee-only rollback of everything the World can touch.
fn reject(
    w: &mut World,
    variant: &str,
    instruction: Instruction,
    expected: ErrorCode,
) -> Observation {
    let before = w.snapshot_all();
    let observation = w
        .send(variant, vec![instruction], &[])
        .expect_err("the transaction must be rejected");
    w.assert_rejected(&observation, expected);
    w.assert_fee_only(&before, &observation);
    observation
}

fn assert_all_fit(log: &[Observation]) {
    for observation in log {
        assert!(
            observation.fits_legacy_packet,
            "{} exceeds the legacy packet: {} bytes",
            observation.label, observation.packet_bytes
        );
    }
}

fn open_book(n: u64, series_of: impl Fn(u64) -> u8) -> Vec<BackerSpec> {
    (0..n)
        .map(|index| BackerSpec::new(who(index), series_of(index), 1_000_000_000))
        .collect()
}

/// A U256 as `(hi, lo)` u128 limbs.  The harness has no `ethnum` dependency,
/// and the program persists the accumulator as 32 little-endian bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wide {
    hi: u128,
    lo: u128,
}

impl Wide {
    const ZERO: Wide = Wide { hi: 0, lo: 0 };

    fn from_le(bytes: [u8; 32]) -> Self {
        Wide {
            lo: u128::from_le_bytes(bytes[..16].try_into().expect("16 bytes")),
            hi: u128::from_le_bytes(bytes[16..].try_into().expect("16 bytes")),
        }
    }

    fn add(self, value: u128) -> Self {
        let (lo, carry) = self.lo.overflowing_add(value);
        Wide {
            hi: self.hi + u128::from(carry),
            lo,
        }
    }

    fn sub(self, value: u128) -> Self {
        let (lo, borrow) = self.lo.overflowing_sub(value);
        Wide {
            hi: self.hi - u128::from(borrow),
            lo,
        }
    }
}

fn series_of(series: u8) -> Series {
    if series == SERIES_A {
        Series::A
    } else {
        Series::B
    }
}

/// The host `Book` replays the same ticket and must agree with the program's
/// per-Backer `pending` / `funded` / `lifetime` / liveness, the seed, the
/// global funded claims and the surviving registry length.  The `Book` treats
/// a Backer with `<= 10` remaining as already out of its registry, whereas the
/// fixtures here (like the chain between a cap and the next Prune) leave such
/// rows in the dense registry until the ticket prunes them, so registry SLOTS
/// differ by construction and only liveness is compared.
fn assert_matches_host_book(w: &World, specs: &[BackerSpec], toll: u64, rows: u8) {
    let mut book = Book::with_pot(u64::MAX / 2);
    for (index, spec) in specs.iter().enumerate() {
        book.add_backer_with_lifetime(
            index as u64,
            series_of(spec.series),
            spec.entitlement,
            spec.lifetime,
        )
        .expect("host book Backer");
    }
    book.allocate(toll, usize::from(rows))
        .expect("host book allocation");
    let mut live = 0u64;
    for (index, spec) in specs.iter().enumerate() {
        let expected = book
            .backer(BackerKey::new(index as u64, series_of(spec.series)))
            .expect("host book row");
        let mine = w.backer(spec.owner, spec.series);
        let at = format!("rows {rows} backer {index}");
        assert_eq!(mine.pending, expected.pending, "{at} pending");
        assert_eq!(mine.funded, expected.funded, "{at} funded");
        assert_eq!(mine.lifetime, expected.lifetime_credited, "{at} lifetime");
        assert_eq!(mine.active, expected.active_slot.is_some(), "{at} liveness");
        live += u64::from(mine.active);
    }
    assert_eq!(w.pool().registry_len, live, "rows {rows} registry length");
    assert_eq!(w.pool().seed, book.seed(), "rows {rows} seed");
    assert_eq!(
        w.pool().funded_claims,
        book.global_funded_claims(),
        "rows {rows} funded claims"
    );
    assert_eq!(w.pool().pending_toll, 0, "rows {rows} escrow");
}

// ---------------------------------------------------------------------------
// Goldens
// ---------------------------------------------------------------------------

pub fn golden_t10000_a4000_a6000_two_b_seed_5001_a2363_a1636_b500() -> Vec<Observation> {
    let row = ROW_GOLDEN_T10000;
    let (a1, a2, b1, b2) = (who(0), who(1), who(2), who(3));
    let mut w = World::with_backers(&[
        (a1, A, 4_000),
        (a2, A, 6_000),
        (b1, B, 1_000_000),
        (b2, B, 1_000_000),
    ])
    .scoped_label(scope(row));
    let version_before = w.pool().registry_version;

    w.start_ticket_fixture(10_000);
    let ticket = w.ticket();
    assert_eq!(
        (ticket.id, ticket.phase, ticket.captured_len, ticket.toll),
        (1, DISCOVER, 4, 10_000)
    );
    assert_eq!(
        (ticket.senior_bps, ticket.a_bps, ticket.b_bps),
        (3_500, 4_500, 2_000)
    );
    assert_eq!(w.pool().pending_toll, 10_000);
    w.assert_vault_identity();

    w.advance_until_idle(8);

    assert_eq!(
        w.last_event::<TicketCommitted>(),
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 10_000,
            credited: 4_999,
            seed: 5_001,
        }
    );
    let pool = w.pool();
    assert_eq!(pool.seed, 5_001);
    assert_eq!(pool.funded_claims, 2_363 + 1_636 + 500 + 500);
    assert_eq!(pool.pending_toll, 0);
    assert_eq!((pool.active_ticket, pool.finalized_ticket), (0, 1));
    assert_eq!(pool.registry_len, 4, "nobody reached the cap");
    assert_eq!(
        pool.registry_version, version_before,
        "no prune, no version bump"
    );
    for (owner, series, credit) in [
        (a1, A, 2_363u64),
        (a2, A, 1_636),
        (b1, B, 500),
        (b2, B, 500),
    ] {
        let backer = w.backer(owner, series);
        assert_eq!(backer.pending, credit, "pending credit");
        assert_eq!(backer.pending_ticket, 1);
        assert_eq!(backer.lifetime, u128::from(credit), "lifetime credited");
        assert_eq!(
            backer.funded, 0,
            "a credit is not funded until materialized"
        );
        assert!(backer.active);
    }
    let ticket = w.ticket();
    assert_eq!(ticket.phase, IDLE);
    assert_eq!((ticket.credited, ticket.seed_credit), (4_999, 5_001));
    w.assert_vault_identity();

    let mut log = w.take_log();
    assert_all_fit(&log);
    promote(&mut log, &format!("{}.commit.0", scope(row)), &scope(row));
    log
}

pub fn golden_no_b_a1000_a10000_seed_5750_a1000_a3250() -> Vec<Observation> {
    let row = ROW_GOLDEN_NO_B;
    let (a1, a2) = (who(0), who(1));
    let mut w = World::with_backers(&[(a1, A, 1_000), (a2, A, 10_000)]).scoped_label(scope(row));
    let version_before = w.pool().registry_version;
    w.start_ticket_fixture(10_000);
    w.advance_until_idle(8);

    assert_eq!(
        w.last_event::<TicketCommitted>(),
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 10_000,
            credited: 4_250,
            seed: 5_750,
        }
    );
    let pool = w.pool();
    assert_eq!(pool.seed, 5_750);
    assert_eq!(pool.funded_claims, 1_000 + 3_250);
    assert_eq!(pool.pending_toll, 0);
    assert_eq!(w.backer(a1, A).pending, 1_000);
    assert_eq!(w.backer(a2, A).pending, 3_250);
    // No B: the B equal slice folds into A (3_250); senior overshoot (750)
    // joined the seed because A1 only had 1_000 left.
    assert_eq!(w.ticket().a_slice, 3_250);
    // A1 is fully repaid (remaining 0): it leaves the registry with its
    // history and A2 is swap-deleted into slot 0.
    let first = w.backer(a1, A);
    assert_eq!((first.active, first.slot), (false, 0));
    assert_eq!(first.lifetime, first.entitlement);
    let second = w.backer(a2, A);
    assert_eq!((second.active, second.slot), (true, 0));
    assert_eq!(pool.registry_len, 1);
    let page = w.registry(0);
    assert_eq!(page.keys[0], w.rt.backer_address(A, a2));
    assert_eq!(page.keys[1], Pubkey::default());
    assert_eq!(
        pool.registry_version,
        version_before + 1,
        "a prune bumps the version once"
    );
    w.assert_vault_identity();

    let mut log = w.take_log();
    assert_all_fit(&log);
    promote(&mut log, &format!("{}.commit.0", scope(row)), &scope(row));
    log
}

pub fn golden_only_a1000_and_b1_seed_8000_a1000_b1000_zero_denominator_to_seed() -> Vec<Observation>
{
    let row = ROW_GOLDEN_ONLY_A_AND_B1;
    let (a1, b1) = (who(0), who(1));
    let mut w = World::with_backers(&[(a1, A, 1_000), (b1, B, 1_000_000)]).scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(ALLOCATE, 8);
    // Senior consumed the only A entirely: the sealed denominator is zero and
    // the whole A slice (2_250) went to seed, NOT to B.
    let ticket = w.ticket();
    assert_eq!(ticket.senior_credit, 1_000);
    assert_eq!(Wide::from_le(ticket.sealed_a_denominator), Wide::ZERO);
    assert_eq!((ticket.a_slice, ticket.b_slice), (0, 1_000));
    assert_eq!(ticket.seed_credit, 5_000 + 750 + 2_250);
    w.advance_until_idle(8);

    assert_eq!(
        w.last_event::<TicketCommitted>(),
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 10_000,
            credited: 2_000,
            seed: 8_000,
        }
    );
    let pool = w.pool();
    assert_eq!(pool.seed, 8_000);
    assert_eq!(pool.funded_claims, 2_000);
    assert_eq!(pool.pending_toll, 0);
    assert_eq!(w.backer(a1, A).pending, 1_000);
    assert_eq!(w.backer(b1, B).pending, 1_000);
    assert!(!w.backer(a1, A).active, "A1 is capped and pruned");
    let survivor = w.backer(b1, B);
    assert_eq!((survivor.active, survivor.slot), (true, 0));
    assert_eq!(pool.registry_len, 1);
    w.assert_vault_identity();

    let mut log = w.take_log();
    assert_all_fit(&log);
    promote(&mut log, &format!("{}.commit.0", scope(row)), &scope(row));
    log
}

// ---------------------------------------------------------------------------
// Chunk sizes and the capture of compute units / packet bytes
// ---------------------------------------------------------------------------

/// 37 deterministic Backers (about a third closed) and toll 123_457, run to
/// `IDLE` in batches of 1, 4 and 8 rows.  The state digest after the
/// `rows = 1` pass is the oracle for the other two (computed in the run, never
/// hard-coded), and each pass also agrees with the host `Book`.
///
/// The labels `rows-<R>.<phase>.<k>` carry the compute units and packet bytes
/// of every phase instruction at each batch size.  The headline is the
/// `rows-8` Commit (Commit's cost does not depend on the batch size, and the
/// `rows-1` and `rows-4` Commits keep their own labels).
pub fn chunk_sizes_1_4_8_produce_identical_state() -> Vec<Observation> {
    let row = ROW_CHUNK_SIZES;
    let toll = 123_457;
    let specs = random_backer_specs(7, 37);
    let mut log = Vec::new();
    let mut oracle: Option<[u8; 32]> = None;
    for rows in [1u8, 4, 8] {
        let mut w =
            World::with_backer_specs(&specs).scoped_label(format!("{}.rows-{rows}", scope(row)));
        let version_before = w.pool().registry_version;
        w.start_ticket_fixture(toll);
        w.advance_until_idle(rows);
        w.assert_vault_identity();

        let pool = w.pool();
        let ticket = w.ticket();
        assert_eq!(ticket.phase, IDLE);
        assert_eq!(ticket.credited + ticket.seed_credit, toll, "conservation");
        assert!(
            ticket.a_count > 0 && ticket.b_count > 0,
            "both series are open"
        );
        assert!(
            pool.registry_len <= 34,
            "at least the closed A (5), the closed B (10) and the capped B left the registry"
        );
        assert_eq!(pool.registry_version, version_before + 1);
        assert_eq!(
            pool.seed + u64::try_from(pool.funded_claims).expect("small"),
            toll,
            "the toll is now seed plus funded claims"
        );
        assert_matches_host_book(&w, &specs, toll, rows);

        let digest = w.digest_financial_state();
        match oracle {
            None => oracle = Some(digest),
            Some(expected) => assert_eq!(
                digest, expected,
                "rows = {rows} must reach exactly the state of rows = 1"
            ),
        }
        let mut pass = w.take_log();
        assert_all_fit(&pass);
        if rows == 8 {
            promote(
                &mut pass,
                &format!("{}.rows-8.commit.0", scope(row)),
                &scope(row),
            );
        }
        log.extend(pass);
    }
    log
}

pub fn registry_sizes_16_64_100_256_fit_the_legacy_packet_at_8_rows() -> Vec<Observation> {
    let row = ROW_SIZES;
    let mut log = Vec::new();

    // A whole ticket at 8 rows per instruction for four registry sizes; every
    // row stays open, so Prune passes the placeholder for each.
    for n in [16u64, 64, 100, 256] {
        let specs = open_book(n, |index| if index % 2 == 0 { A } else { B });
        let mut w = World::with_backer_specs(&specs).scoped_label(format!("{}.n-{n}", scope(row)));
        w.start_ticket_fixture(1_000_001);
        w.advance_until_idle(8);
        assert_eq!(w.pool().registry_len, n, "nobody reached the cap");
        assert_eq!(w.ticket().phase, IDLE);
        w.assert_vault_identity();
        let pass = w.take_log();
        assert_all_fit(&pass);
        // ceil(n / 8) Discover and Allocate batches.
        let batches = n.div_ceil(8) as usize;
        for phase in ["discover", "allocate", "prune"] {
            let prefix = format!("{}.n-{n}.{phase}.", scope(row));
            assert_eq!(
                pass.iter().filter(|o| o.label.starts_with(&prefix)).count(),
                batches,
                "{phase} batches at n = {n}"
            );
        }
        log.extend(pass);
    }

    // The widest legal Prune batch: eight closed rows in the middle of a
    // 100-row registry, each swap-deleted by a DISTINCT open tail row, so the
    // batch carries 8 pruned + 8 moved Backers and up to four registry pages.
    // The packet is recorded, not asserted: whether it fits is the evidence.
    for rows in [8u8, 4] {
        let mut specs = open_book(100, |index| if index % 2 == 0 { A } else { B });
        for spec in &mut specs[52..60] {
            spec.entitlement = 5;
        }
        let mut w = World::with_backer_specs(&specs)
            .scoped_label(format!("{}.prune-worst.rows-{rows}", scope(row)));
        w.start_ticket_fixture(1_000_001);
        w.advance_until_phase(PRUNE, 8);
        w.advance_until_idle(rows);
        assert_eq!(
            w.pool().registry_len,
            92,
            "the eight closed rows are pruned"
        );
        for spec in &specs[52..60] {
            assert!(!w.backer(spec.owner, spec.series).active);
        }
        w.assert_vault_identity();
        log.extend(w.take_log());
    }

    // The headline: the 100-row ticket's Commit (the widest-prune labels
    // `prune-worst.rows-<R>.prune.<k>` stay intact for the packet table).
    promote(
        &mut log,
        &format!("{}.n-100.commit.0", scope(row)),
        &scope(row),
    );
    log
}

// ---------------------------------------------------------------------------
// Cursor, stale and phase rules
// ---------------------------------------------------------------------------

pub fn replaying_a_completed_discover_chunk_fails_without_credit() -> Vec<Observation> {
    let row = ROW_REPLAY;
    let specs: Vec<(Pubkey, u8, u128)> = (0..10).map(|i| (who(i), A, 100_000)).collect();
    let mut w = World::with_backers(&specs).scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    let id = w.ticket().id;

    w.discover_rows("first_chunk", id, 0, 2)
        .expect("the first Discover chunk");
    let ticket = w.ticket();
    assert_eq!((ticket.cursor, ticket.a_count), (2, 2));
    let ticket_bytes = w.data(w.rt.ticket_address());
    let snapshot_bytes = w.data(w.rt.snapshot_address(0));
    let pool_bytes = w.data(w.rt.pool_address());

    // The same chunk again: StaleCursor, nothing moves, nothing is credited.
    let before = w.snapshot_all();
    let replay = w.discover_rows("", id, 0, 2).unwrap_err();
    w.assert_rejected(&replay, ErrorCode::StaleCursor);
    w.assert_fee_only(&before, &replay);
    assert_eq!(
        w.data(w.rt.ticket_address()),
        ticket_bytes,
        "ticket unchanged"
    );
    assert_eq!(
        w.data(w.rt.snapshot_address(0)),
        snapshot_bytes,
        "snapshot unchanged"
    );
    assert_eq!(w.data(w.rt.pool_address()), pool_bytes, "pool unchanged");

    // A cursor from the future is just as stale.
    let before = w.snapshot_all();
    let future = w.discover_rows("future_cursor", id, 5, 2).unwrap_err();
    w.assert_rejected(&future, ErrorCode::StaleCursor);
    w.assert_fee_only(&before, &future);

    // The prior ticket is uncommitted, so no NEW ticket can Discover: the
    // next ticket id is stale.
    let before = w.snapshot_all();
    let next = w.discover_rows("next_ticket_id", id + 1, 2, 2).unwrap_err();
    w.assert_rejected(&next, ErrorCode::StaleTicket);
    w.assert_fee_only(&before, &next);

    // Nothing was credited by any of it.
    let ticket = w.ticket();
    assert_eq!((ticket.credited, ticket.cursor, ticket.a_count), (0, 2, 2));
    let pool = w.pool();
    assert_eq!((pool.funded_claims, pool.seed), (0, 0));
    assert_eq!(pool.pending_toll, 10_000);
    for spec in &specs {
        assert_eq!(w.backer(spec.0, A).pending, 0);
    }

    // Finish ticket 1; only now can the next ticket Discover.
    w.advance_until_idle(8);
    assert_eq!(w.pool().active_ticket, 0);
    assert_eq!(w.snapshot_page(0).ticket_id, 1);
    assert_eq!(w.snapshot_page(1).ticket_id, 1);
    w.start_ticket_fixture(10_000);
    assert_eq!(w.ticket().id, 2);

    // Ticket 1's id is now stale; ticket 2's first touch of a page re-tags it
    // and clears the rows ticket 1 left behind.
    let before = w.snapshot_all();
    let old = w.discover_rows("old_ticket_replay", 1, 0, 2).unwrap_err();
    w.assert_rejected(&old, ErrorCode::StaleTicket);
    w.assert_fee_only(&before, &old);
    w.discover_rows("second_ticket_first_chunk", 2, 0, 2)
        .expect("the next ticket's Discover after the commit");
    let page = w.snapshot_page(0);
    assert_eq!(page.ticket_id, 2, "page claimed by ticket 2");
    assert!(page.rows[0].key == w.rt.backer_address(A, who(0)));
    assert!(
        page.rows[2] == state::SnapshotRow::default()
            && page.rows[7] == state::SnapshotRow::default(),
        "rows left by ticket 1 are cleared on the first touch"
    );
    assert_eq!(
        w.snapshot_page(1).ticket_id,
        1,
        "an untouched page keeps its old tag"
    );
    w.advance_until_idle(8);

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

pub fn stale_ticket_id_is_rejected_on_every_phase_instruction() -> Vec<Observation> {
    let row = ROW_STALE_TICKET;
    let mut w = World::with_backers(&[
        (who(0), A, 100_000),
        (who(1), A, 100_000),
        (who(2), B, 1_000_000),
        (who(3), B, 1_000_000),
        (who(4), A, 5),
    ])
    .scoped_label(scope(row));
    // Ticket 1 commits (and prunes the closed A); ticket 2 is the live one.
    w.start_ticket_fixture(10_000);
    w.advance_until_idle(8);
    w.start_ticket_fixture(10_000);
    assert_eq!(w.ticket().id, 2);
    let stale = 1;

    // Each phase instruction, fired at ITS OWN phase with the previous
    // ticket's id: the id is checked first, so every one is StaleTicket.
    let instruction = w.rt.discover_ix(stale, 0, 2, w.rt.discover_remaining(0, 2));
    reject(&mut w, "", instruction, ErrorCode::StaleTicket);

    w.advance_until_phase(SENIOR, 8);
    let instruction = w.rt.senior_ix(stale, w.rt.senior_backer());
    reject(&mut w, "senior", instruction, ErrorCode::StaleTicket);

    w.advance_until_phase(SEAL, 8);
    let instruction = w.rt.seal_ix(stale);
    reject(&mut w, "seal", instruction, ErrorCode::StaleTicket);

    w.advance_until_phase(ALLOCATE, 8);
    let instruction = w.rt.allocate_ix(stale, 0, 2, w.rt.allocate_remaining(0, 2));
    reject(&mut w, "allocate", instruction, ErrorCode::StaleTicket);

    w.advance_until_phase(PRUNE, 8);
    let cursor = w.ticket().cursor;
    let plan = w.rt.prune_plan(1, false, true);
    let instruction = w.rt.prune_ix(stale, cursor, plan.rows, plan.remaining);
    reject(&mut w, "prune", instruction, ErrorCode::StaleTicket);

    w.advance_until_phase(COMMIT, 8);
    let instruction = w.rt.commit_ix(stale, None);
    reject(&mut w, "commit", instruction, ErrorCode::StaleTicket);
    w.advance_until_idle(8);

    // A retired ticket (id 2, IDLE): the pool-bearing instructions say the
    // ticket is no longer the active one; the others see the IDLE phase.
    let retired = 2;
    let instruction =
        w.rt.discover_ix(retired, 0, 1, w.rt.discover_remaining(0, 1));
    reject(
        &mut w,
        "retired_discover",
        instruction,
        ErrorCode::StaleTicket,
    );
    let instruction = w.rt.senior_ix(retired, w.rt.senior_stand_in());
    reject(
        &mut w,
        "retired_senior",
        instruction,
        ErrorCode::InvalidPhase,
    );
    let instruction = w.rt.seal_ix(retired);
    reject(&mut w, "retired_seal", instruction, ErrorCode::InvalidPhase);
    let instruction =
        w.rt.allocate_ix(retired, 0, 1, w.rt.allocate_remaining(0, 1));
    reject(
        &mut w,
        "retired_allocate",
        instruction,
        ErrorCode::InvalidPhase,
    );
    let placeholders = vec![
        AccountMeta::new_readonly(w.rt.snapshot_address(0), false),
        AccountMeta::new_readonly(ID, false),
        AccountMeta::new_readonly(ID, false),
        AccountMeta::new_readonly(ID, false),
        AccountMeta::new_readonly(ID, false),
    ];
    let instruction = w.rt.prune_ix(retired, 0, 1, placeholders);
    reject(&mut w, "retired_prune", instruction, ErrorCode::StaleTicket);
    let instruction = w.rt.commit_ix(retired, None);
    reject(
        &mut w,
        "retired_commit",
        instruction,
        ErrorCode::StaleTicket,
    );

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

/// Plan sketch: two A rows with `remaining = u128::MAX` plus "one small row",
/// `a_remainder_total == 2 * u128::MAX + 7`.  A Backer with 7 lamports left is
/// CLOSED (`<= 10`): it is snapshotted but never counted.  So this row uses a
/// 70-lamport OPEN row (total `2 * u128::MAX + 70`) and keeps a closed 7 to
/// prove it adds nothing.
pub fn discovery_accumulates_a_remainders_above_u128_in_u256() -> Vec<Observation> {
    let row = ROW_U256;
    let (a1, a2, a3, a4) = (who(0), who(1), who(2), who(3));
    let mut w = World::with_backers(&[
        (a1, A, u128::MAX),
        (a2, A, u128::MAX),
        (a3, A, 70),
        (a4, A, 7),
    ])
    .scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    // Discover and Senior done; the ticket waits for `seal_weights`.
    w.advance_until_phase(SEAL, 8);
    let ticket = w.ticket();
    assert_eq!(
        (ticket.a_count, ticket.b_count),
        (3, 0),
        "the closed 7 is not counted"
    );
    let total = Wide::ZERO.add(u128::MAX).add(u128::MAX).add(70);
    assert_eq!(Wide::from_le(ticket.a_remainder_total), total);
    assert_eq!(
        (total.hi, total.lo),
        (2, 68),
        "2 * u128::MAX + 70 = 2^129 + 68"
    );
    assert_eq!(
        ticket.senior_credit, 1_750,
        "senior slice, fully covered by A1"
    );

    let id = ticket.id;
    w.seal_weights("", id).expect("seal_weights");
    let sealed = Wide::from_le(w.ticket().sealed_a_denominator);
    assert_eq!(sealed, total.sub(1_750), "sealed = total - senior_credit");
    assert_eq!(sealed.hi, 1, "the sealed denominator is above u128");
    assert_eq!(w.ticket().phase, ALLOCATE);

    w.advance_until_idle(8);
    let ticket = w.ticket();
    assert_eq!(ticket.credited + ticket.seed_credit, 10_000);
    // The two u128::MAX rows share nearly the whole A slice; the 70-lamport
    // row's share floors to zero; the closed row is never paid.
    assert!(w.backer(a1, A).pending > 1_000 && w.backer(a2, A).pending > 1_000);
    assert_eq!(w.backer(a3, A).pending, 0);
    assert_eq!(w.backer(a4, A).pending, 0);
    assert!(!w.backer(a4, A).active, "the closed 7 is pruned");
    assert!(w.backer(a3, A).active);
    w.assert_vault_identity();

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

// ---------------------------------------------------------------------------
// Prune: swap-delete, aliases and duplicates
// ---------------------------------------------------------------------------

pub fn prune_same_page_swap_delete_clears_last_slot_on_first_page() -> Vec<Observation> {
    let row = ROW_SAME_PAGE_SWAP;
    // Regression for the draft: pruned slot 1 and last slot 3 share page 0.
    let (a1, a2, a3, a4) = (who(0), who(1), who(2), who(3));
    let mut log = Vec::new();
    for rows in [1u8, 8] {
        let mut w = World::with_backers(&[
            (a1, A, 100_000),
            (a2, A, 5),
            (a3, A, 100_000),
            (a4, A, 100_000),
        ])
        .scoped_label(format!("{}.rows-{rows}", scope(row)));
        let version_before = w.pool().registry_version;
        w.start_ticket_fixture(10_000);
        w.advance_until_phase(PRUNE, 8);
        assert_eq!(w.ticket().cursor, 4);
        assert_eq!(w.registry(0).keys[1], w.rt.backer_address(A, a2));
        assert_eq!(w.registry(0).keys[3], w.rt.backer_address(A, a4));

        w.advance_until_phase(COMMIT, rows);
        let page = w.registry(0);
        assert_eq!(
            page.keys[1],
            w.rt.backer_address(A, a4),
            "a4 moved into slot 1"
        );
        assert_eq!(page.keys[3], Pubkey::default(), "the last slot is cleared");
        assert_eq!(page.keys[0], w.rt.backer_address(A, a1));
        assert_eq!(page.keys[2], w.rt.backer_address(A, a3));
        let moved = w.backer(a4, A);
        assert_eq!((moved.active, moved.slot), (true, 1));
        // Regression for the inverted alias write: the pruned row is the one
        // that goes inactive, never the moved one.
        let pruned = w.backer(a2, A);
        assert_eq!((pruned.active, pruned.slot), (false, 0));
        assert_eq!(w.pool().registry_len, 3);
        w.commit_ticket("commit_after_prune", 1).expect("commit");
        assert_eq!(w.pool().registry_version, version_before + 1);
        w.assert_vault_identity();

        let mut pass = w.take_log();
        assert_all_fit(&pass);
        if rows == 8 {
            // One batch carries the whole Prune phase.
            promote(
                &mut pass,
                &format!("{}.rows-8.prune.0", scope(row)),
                &scope(row),
            );
        }
        log.extend(pass);
    }
    log
}

pub fn prune_alias_when_pruned_row_is_last_row_marks_it_inactive() -> Vec<Observation> {
    let row = ROW_PRUNE_ALIAS;
    let (a1, a2) = (who(0), who(1));
    let mut w = World::with_backers(&[(a1, A, 100_000), (a2, A, 5)]).scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(PRUNE, 8);
    let (id, cursor) = (w.ticket().id, w.ticket().cursor);
    assert_eq!(cursor, 2);

    // Row 0 is slot 1, the last live slot: its `moved` account IS the pruned
    // account.  Naming the other Backer there is a WrongAccount.
    let plan = w.rt.prune_plan(2, false, true);
    assert_eq!((plan.rows, plan.pruned), (2, 1));
    assert_eq!(
        plan.remaining[4].pubkey, plan.remaining[1].pubkey,
        "moved aliases pruned"
    );
    let mut wrong = plan.remaining.clone();
    wrong[4] = AccountMeta::new(w.rt.backer_address(A, a1), false);
    let instruction = w.rt.prune_ix(id, cursor, 2, wrong);
    reject(&mut w, "wrong_moved", instruction, ErrorCode::WrongAccount);

    // The honest batch: the pruned row goes inactive, the survivor is not
    // written, the last registry position is cleared.
    let survivor_before = w.data(w.rt.backer_address(A, a1));
    let instruction = w.rt.prune_ix(id, cursor, 2, plan.remaining);
    w.send("", vec![instruction], &[]).expect("alias prune");
    assert_eq!(w.ticket().phase, COMMIT);
    let pruned = w.backer(a2, A);
    assert_eq!((pruned.active, pruned.slot), (false, 0));
    let survivor = w.backer(a1, A);
    assert_eq!((survivor.active, survivor.slot), (true, 0));
    assert_eq!(
        w.data(w.rt.backer_address(A, a1)),
        survivor_before,
        "no moved write"
    );
    assert_eq!(w.registry(0).keys[0], w.rt.backer_address(A, a1));
    assert_eq!(w.registry(0).keys[1], Pubkey::default());
    assert_eq!(w.pool().registry_len, 1);
    w.advance_until_idle(8);

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

pub fn duplicate_independent_backer_rows_are_rejected_but_legitimate_prune_aliases_are_decoded_once(
) -> Vec<Observation> {
    let row = ROW_DUPLICATES;
    let mut log = Vec::new();

    // (a) The same Backer as two INDEPENDENT Allocate rows.
    {
        let (a1, a2, b1) = (who(0), who(1), who(2));
        let mut w = World::with_backers(&[(a1, A, 100_000), (a2, A, 100_000), (b1, B, 1_000_000)])
            .scoped_label(scope(row));
        w.start_ticket_fixture(10_000);
        w.advance_until_phase(ALLOCATE, 8);
        let id = w.ticket().id;
        let mut duplicated = w.rt.allocate_remaining(0, 2);
        duplicated[3] = AccountMeta::new(duplicated[1].pubkey, false);
        let instruction = w.rt.allocate_ix(id, 0, 2, duplicated);
        reject(&mut w, "", instruction, ErrorCode::DuplicateWritableAccount);
        // A different wrong account is a WrongAccount, not a duplicate.
        let mut wrong = w.rt.allocate_remaining(0, 2);
        wrong[3] = AccountMeta::new(w.rt.backer_address(B, b1), false);
        let instruction = w.rt.allocate_ix(id, 0, 2, wrong);
        reject(
            &mut w,
            "wrong_account",
            instruction,
            ErrorCode::WrongAccount,
        );
        assert_eq!(w.ticket().cursor, 0, "neither attempt advanced the cursor");
        w.allocate_rows("honest_batch", id, 0, 2)
            .expect("the honest batch");
        w.advance_until_idle(8);
        log.extend(w.take_log());
    }

    // (b) RegistryPage(pruned) == RegistryPage(last): the page is decoded and
    // written once, so clearing the last slot survives (a double decode would
    // write the stale copy back over it).
    {
        let (a1, a2, a3, a4) = (who(0), who(1), who(2), who(3));
        let mut w = World::with_backers(&[
            (a1, A, 100_000),
            (a2, A, 5),
            (a3, A, 100_000),
            (a4, A, 100_000),
        ])
        .scoped_label(format!("{}.same_registry_page", scope(row)));
        w.start_ticket_fixture(10_000);
        w.advance_until_phase(PRUNE, 8);
        let (id, cursor) = (w.ticket().id, w.ticket().cursor);
        let plan = w.rt.prune_plan(4, false, true);
        assert_eq!((plan.rows, plan.pruned), (4, 1));
        // Row 2 (slot 1) is the pruned row: pruned page == last page.
        assert_eq!(
            plan.remaining[2 * 5 + 2].pubkey,
            plan.remaining[2 * 5 + 3].pubkey
        );
        let instruction = w.rt.prune_ix(id, cursor, 4, plan.remaining);
        w.send("prune", vec![instruction], &[])
            .expect("same-page prune");
        let page = w.registry(0);
        assert_eq!(page.keys[1], w.rt.backer_address(A, a4));
        assert_eq!(page.keys[3], Pubkey::default());
        assert_eq!(w.backer(a4, A).slot, 1);
        assert!(!w.backer(a2, A).active);
        w.advance_until_idle(8);
        log.extend(w.take_log());
    }

    // (c) Backer(moved) == Backer(pruned) at the last live slot: marks it
    // inactive and performs no moved write.
    {
        let (a1, a2) = (who(0), who(1));
        let mut w = World::with_backers(&[(a1, A, 100_000), (a2, A, 5)])
            .scoped_label(format!("{}.moved_is_pruned", scope(row)));
        w.start_ticket_fixture(10_000);
        w.advance_until_phase(PRUNE, 8);
        let (id, cursor) = (w.ticket().id, w.ticket().cursor);
        let plan = w.rt.prune_plan(2, false, true);
        assert_eq!(plan.remaining[4].pubkey, plan.remaining[1].pubkey);
        let instruction = w.rt.prune_ix(id, cursor, 2, plan.remaining);
        w.send("prune", vec![instruction], &[])
            .expect("alias prune");
        assert!(!w.backer(a2, A).active);
        assert_eq!(w.pool().registry_len, 1);
        w.advance_until_idle(8);
        log.extend(w.take_log());
    }

    // (d) A chained move is an independent duplicate of the moved Backer:
    // [closed, closed, open] prunes slot 1 (moving the open tail row down)
    // and then slot 0 (moving the SAME row again).  One batch is rejected; one
    // row per instruction (what the driver falls back to) is legal.
    {
        let (a1, a2, a3) = (who(0), who(1), who(2));
        let mut w = World::with_backers(&[(a1, A, 5), (a2, A, 5), (a3, A, 100_000)])
            .scoped_label(format!("{}.chained_move", scope(row)));
        w.start_ticket_fixture(10_000);
        w.advance_until_phase(PRUNE, 8);
        let (id, cursor) = (w.ticket().id, w.ticket().cursor);
        let illegal = w.rt.prune_plan(3, false, false);
        assert_eq!((illegal.rows, illegal.pruned), (3, 2));
        let instruction = w.rt.prune_ix(id, cursor, 3, illegal.remaining);
        reject(
            &mut w,
            "one_batch",
            instruction,
            ErrorCode::DuplicateWritableAccount,
        );
        let cut = w.rt.prune_plan(3, false, true);
        assert_eq!(cut.rows, 2, "the keeper stops before the chained row");
        w.advance_until_idle(3);
        assert_eq!(w.pool().registry_len, 1);
        assert_eq!(w.registry(0).keys[0], w.rt.backer_address(A, a3));
        let survivor = w.backer(a3, A);
        assert_eq!((survivor.active, survivor.slot), (true, 0));
        assert!(!w.backer(a1, A).active && !w.backer(a2, A).active);
        log.extend(w.take_log());
    }

    assert_all_fit(&log);
    log
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

pub fn claim_during_ticket_pays_only_finalized_and_commit_adds_once() -> Vec<Observation> {
    let row = ROW_CLAIM_DURING_TICKET;
    let owner = owner_signer();
    let owner_key = owner.pubkey();
    // One open A holding plenty of headroom.  It already has 100 lamports of
    // finalized claims (`funded`), counted in the pool and held by the vault.
    let mut w = World::with_backers(&[(owner_key, A, 1_000_000)]).scoped_label(scope(row));
    w.fund_backer_fixture(owner_key, A, 100);
    // The claims here are a few lamports, far below the rent-exempt minimum of
    // a fresh account (the runtime refuses to leave a NEW account rent-paying),
    // so they go to an existing, funded system account.
    let dest = w.rt.management.pubkey();
    // Toll 40 with a single A: half 20 to seed, senior 7, A 9 + B-fold 4, so
    // the Backer is credited exactly 7 + 13 = 20.
    w.start_ticket_fixture(40);
    w.advance_until_phase(COMMIT, 8);
    let backer = w.backer(owner_key, A);
    assert_eq!(
        (
            backer.funded,
            backer.pending,
            backer.pending_ticket,
            backer.lifetime
        ),
        (100, 20, 1, 120)
    );
    assert_eq!(
        w.pool().funded_claims,
        100,
        "the pending credit is not counted until Commit"
    );
    w.assert_vault_identity();

    // A claim while the ticket is still active pays only what is finalized.
    let paid = w
        .claim_backer("", &owner, dest)
        .expect("claim during the active ticket");
    assert_eq!(paid, 100);
    let backer = w.backer(owner_key, A);
    assert_eq!(
        (backer.funded, backer.pending, backer.pending_ticket),
        (0, 20, 1),
        "the active ticket's credit stays pending"
    );
    assert_eq!(backer.lifetime, 120, "a claim never touches lifetime");
    assert_eq!(w.pool().funded_claims, 0);
    w.assert_vault_identity();

    // Nothing more can be taken until the ticket commits.
    let before = w.snapshot_all();
    let rejected = w
        .claim_backer("nothing_while_active", &owner, dest)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::NothingToClaim);
    w.assert_fee_only(&before, &rejected);

    // Commit counts the 20 into the pool exactly once.
    w.commit_ticket("commit", 1).expect("commit");
    assert_eq!(
        w.last_event::<TicketCommitted>(),
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 40,
            credited: 20,
            seed: 20,
        }
    );
    assert_eq!(w.pool().funded_claims, 20);
    assert_eq!(w.pool().seed, 20);
    w.assert_vault_identity();

    // Lazy materialization: the next claim folds the 20 into `funded` and pays
    // it, with no second lifetime credit.
    let paid = w
        .claim_backer("after_commit", &owner, dest)
        .expect("claim after the commit");
    assert_eq!(paid, 20);
    let backer = w.backer(owner_key, A);
    assert_eq!(
        (backer.funded, backer.pending, backer.pending_ticket),
        (0, 0, 0)
    );
    assert_eq!(
        backer.lifetime, 120,
        "lifetime credited is unchanged by claims"
    );
    assert_eq!(w.pool().funded_claims, 0);
    w.assert_vault_identity();

    let before = w.snapshot_all();
    let rejected = w.claim_backer("nothing_left", &owner, dest).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::NothingToClaim);
    w.assert_fee_only(&before, &rejected);

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

/// A world where the owner's real Series A funding (50M gross, 62M
/// entitlement) is capped by a 200M toll in ONE ticket: the Backer is
/// credited its whole entitlement (35M senior + 27M A share), the 38M
/// overshoot joins the seed, and Prune removes the newly capped row.  The
/// ticket is committed; the 62M is still `pending`.
fn capped_world(label: String) -> (World, Keypair) {
    let owner = owner_signer();
    let owner_key = owner.pubkey();
    let mut w = World::genesis().scoped_label(label);
    w.fund_series_a("fund_series_a", &owner, 50_000_000, 1)
        .expect("fund Series A");
    let funded = w.backer(owner_key, A);
    assert_eq!(funded.entitlement, 62_000_000);
    assert_eq!(
        (funded.first_sequence, funded.active, funded.slot),
        (1, true, 0)
    );
    assert!(w.user(owner_key).ever_a);
    let version_before = w.pool().registry_version;

    w.start_ticket_fixture(200_000_000);
    w.advance_until_idle(8);
    assert_eq!(
        w.last_event::<TicketCommitted>(),
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 200_000_000,
            credited: 62_000_000,
            seed: 138_000_000,
        }
    );
    let backer = w.backer(owner_key, A);
    assert_eq!(
        backer.lifetime, backer.entitlement,
        "capped: nothing remains"
    );
    assert_eq!(backer.remaining().expect("remaining"), 0);
    assert_eq!((backer.pending, backer.pending_ticket), (62_000_000, 1));
    // Capped, so it left the registry; its history stays.
    assert!(!backer.active);
    assert_eq!(backer.first_sequence, 1);
    let pool = w.pool();
    assert_eq!(pool.registry_len, 0);
    assert_eq!(pool.registry_version, version_before + 1);
    assert_eq!(w.registry(0).keys[0], Pubkey::default());
    assert_eq!(pool.funded_claims, 62_000_000);
    assert_eq!(pool.seed, 138_000_000);
    w.assert_vault_identity();
    (w, owner)
}

pub fn claim_never_restores_cap_headroom_and_capped_backer_leaves_registry_with_history(
) -> Vec<Observation> {
    let row = ROW_CLAIM_CAP;
    let (mut w, owner) = capped_world(scope(row));
    let owner_key = owner.pubkey();
    let dest = w.dest;

    let paid = w
        .claim_backer("", &owner, dest)
        .expect("claim the capped credit");
    assert_eq!(paid, 62_000_000);
    let backer = w.backer(owner_key, A);
    assert_eq!(
        (backer.funded, backer.pending, backer.pending_ticket),
        (0, 0, 0)
    );
    assert_eq!(
        backer.lifetime, 62_000_000,
        "lifetime unchanged by the claim"
    );
    assert_eq!(
        backer.remaining().expect("remaining"),
        0,
        "the claim restores no cap headroom"
    );
    // The Backer keeps its history: still a program account with its original
    // funding sequence, the owner's `ever_a` flag is set, and it is out of the
    // registry.
    assert!(!backer.active);
    assert_eq!((backer.first_sequence, backer.owner), (1, owner_key));
    assert!(w.user(owner_key).ever_a);
    assert_eq!(w.pool().registry_len, 0);
    assert_eq!(w.pool().funded_claims, 0);
    w.assert_vault_identity();

    let before = w.snapshot_all();
    let rejected = w.claim_backer("nothing_left", &owner, dest).unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::NothingToClaim);
    w.assert_fee_only(&before, &rejected);

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

pub fn reopen_capped_a_keeps_first_funding_seq() -> Vec<Observation> {
    let row = ROW_REOPEN;
    let (mut w, owner) = capped_world(format!("{}.capped_setup", scope(row)));
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.claim_backer("claim", &owner, dest)
        .expect("claim the capped credit");

    // Another A is funded AFTER the owner's original funding (sequence 2).
    let attacker_key = w.attacker().pubkey();
    w.install_open_backer(attacker_key);
    let other = w.backer(attacker_key, A);
    assert_eq!((other.first_sequence, other.slot), (2, 0));
    let next_sequence = w.pool().next_sequence;
    assert_eq!(next_sequence, 3);

    // The owner funds again: the capped Backer is appended behind the other A,
    // with its ORIGINAL funding sequence.
    let mut w = w.scoped_label(scope(row));
    w.fund_series_a("", &owner, 50_000_000, 1)
        .expect("fund the capped Backer again");
    let backer = w.backer(owner_key, A);
    assert_eq!(
        backer.first_sequence, 1,
        "the original funding sequence is kept"
    );
    assert_eq!(
        w.pool().next_sequence,
        next_sequence,
        "reopening allocates no new sequence"
    );
    assert!(backer.active);
    assert_eq!(backer.slot, 1, "appended behind the other A");
    assert_eq!(backer.entitlement, 124_000_000);
    assert_eq!(backer.lifetime, 62_000_000);
    assert_eq!(
        backer.remaining().expect("remaining"),
        62_000_000,
        "only the new mint is headroom: the earlier claim restored nothing"
    );
    assert_eq!(backer.amount, 100_000_000);
    assert_eq!(w.pool().registry_len, 2);
    assert_eq!(w.registry(0).keys[0], w.rt.backer_address(A, attacker_key));
    assert_eq!(w.registry(0).keys[1], w.rt.backer_address(A, owner_key));

    // Seniority is preserved: the reopened Backer, sitting in slot 1, is
    // still the oldest open A of the next ticket.
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(SENIOR, 8);
    let ticket = w.ticket();
    assert_eq!(ticket.oldest, w.rt.backer_address(A, owner_key));
    assert_eq!((ticket.oldest_sequence, ticket.oldest_slot), (1, 1));
    assert_eq!(ticket.a_count, 2);
    w.advance_until_idle(8);
    w.assert_vault_identity();

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

// ---------------------------------------------------------------------------
// Admission around a ticket and conservation
// ---------------------------------------------------------------------------

pub fn normal_deposit_allowed_during_nonclosing_ticket_but_a_funding_and_payout_wait(
) -> Vec<Observation> {
    let row = ROW_NORMAL_DEPOSIT;
    let owner = owner_signer();
    let mut w = World::genesis_with_simple_position(SOL).scoped_label(scope(row));
    w.install_backer_specs(&[
        BackerSpec::new(who(0), A, 100_000),
        BackerSpec::new(who(1), B, 1_000_000),
    ]);
    w.warp_seconds(DAY);
    let dest = w.dest;
    w.start_ticket_fixture(10_000);
    assert!(!w.pool().closing());
    assert_eq!(w.pool().active_ticket, 1);

    // A normal deposit is allowed during a nonclosing ticket.
    w.open_position("", &owner, PlanKindArg::Simple, SOL, 2)
        .expect("open during a nonclosing ticket");

    // Series A funding waits for the ticket ...
    let before = w.snapshot_all();
    let rejected = w
        .fund_series_a("fund_series_a_waits", &owner, 50_000_000, 1)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::AllocationPending);
    w.assert_fee_only(&before, &rejected);

    // ... and so does a payout (which would need to start the next ticket).
    let before = w.snapshot_all();
    let rejected = w
        .withdraw_simple("withdraw_simple_waits", &owner, 1, 0, dest, 0)
        .unwrap_err();
    w.assert_rejected(&rejected, ErrorCode::AllocationPending);
    w.assert_fee_only(&before, &rejected);

    // Once the ticket commits, both go through; the payout starts the NEXT
    // ticket for real (not a fixture), which the same driver then finishes.
    w.advance_until_idle(8);
    assert_eq!(w.pool().active_ticket, 0);
    w.fund_series_a("fund_series_a_after_commit", &owner, 50_000_000, 1)
        .expect("Series A funding after the commit");
    w.withdraw_simple("withdraw_simple_after_commit", &owner, 1, 0, dest, 0)
        .expect("payout after the commit");
    let pool = w.pool();
    assert_eq!((pool.active_ticket, pool.next_ticket), (2, 3));
    let ticket = w.ticket();
    assert_eq!(
        (ticket.id, ticket.phase, ticket.close_id),
        (2, DISCOVER, None)
    );
    assert_eq!(ticket.toll, 13_200_000);
    assert_eq!(pool.pending_toll, 13_200_000);
    assert_eq!(
        ticket.captured_len, 3,
        "two fixture Backers plus the owner's new A"
    );
    w.advance_until_idle(8);
    assert_eq!(w.pool().pending_toll, 0);
    assert_eq!(w.pool().active_ticket, 0);
    w.assert_vault_identity();

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

pub fn conservation_identity_holds_after_every_phase() -> Vec<Observation> {
    let row = ROW_CONSERVATION;
    let toll = 98_765;
    let mut w = World::with_random_backers(11, 20).scoped_label(scope(row));
    w.assert_vault_identity();
    w.start_ticket_fixture(toll);
    w.assert_vault_identity();
    let books = |w: &World| {
        let pool = w.pool();
        (
            pool.pot,
            pool.seed,
            pool.management,
            pool.pending_toll,
            pool.funded_claims,
            pool.excess,
        )
    };
    let before = books(&w);
    let accounted_before = musical_chairs_program::game_accounted(&w.pool()).expect("accounted");

    let mut commits = 0;
    while w.ticket().phase != IDLE {
        let phase = w.ticket().phase;
        w.advance_step(3);
        w.assert_vault_identity();
        let ticket = w.ticket();
        if phase != COMMIT {
            // No partition of the pool moves before Commit: the toll stays
            // escrowed while the ticket credits Backers.
            assert_eq!(books(&w), before, "after a phase-{phase} step");
            assert!(ticket.credited + ticket.seed_credit <= ticket.toll);
        } else {
            commits += 1;
        }
        if ticket.phase == PRUNE || ticket.phase == COMMIT {
            assert_eq!(
                ticket.credited + ticket.seed_credit,
                ticket.toll,
                "Allocate has attributed every lamport"
            );
        }
    }
    assert_eq!(commits, 1);
    let pool = w.pool();
    assert_eq!(pool.pending_toll, 0);
    assert_eq!(
        u128::from(pool.seed) + pool.funded_claims,
        u128::from(toll),
        "the escrowed toll became seed plus funded claims"
    );
    assert_eq!(
        musical_chairs_program::game_accounted(&pool).expect("accounted"),
        accounted_before,
        "the partitions' sum is unchanged by the ticket"
    );

    let mut log = w.take_log();
    assert_all_fit(&log);
    promote(&mut log, &format!("{}.commit.0", scope(row)), &scope(row));
    log
}

// ---------------------------------------------------------------------------
// Rows beyond the plan list
// ---------------------------------------------------------------------------

pub fn malformed_batches_are_invalid_state_and_fee_only() -> Vec<Observation> {
    let row = ROW_MALFORMED;
    let specs: Vec<(Pubkey, u8, u128)> = (0..10).map(|i| (who(i), A, 100_000)).collect();
    let mut w = World::with_backers(&specs).scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    let id = w.ticket().id;

    // Zero rows, more than eight rows, and an account list that does not match
    // the row count.
    let instruction = w.rt.discover_ix(id, 0, 0, Vec::new());
    reject(&mut w, "", instruction, ErrorCode::InvalidState);
    let instruction = w.rt.discover_ix(id, 0, 9, w.rt.discover_remaining(0, 9));
    reject(&mut w, "nine_rows", instruction, ErrorCode::InvalidState);
    let instruction = w.rt.discover_ix(id, 0, 2, w.rt.discover_remaining(0, 1));
    reject(
        &mut w,
        "wrong_account_count",
        instruction,
        ErrorCode::InvalidState,
    );

    // A batch that runs past the captured length is never silently shortened.
    w.discover_rows("eight_rows", id, 0, 8).expect("eight rows");
    let instruction = w.rt.discover_ix(id, 8, 4, w.rt.discover_remaining(8, 4));
    reject(&mut w, "overrun", instruction, ErrorCode::InvalidState);
    w.discover_rows("last_two_rows", id, 8, 2)
        .expect("last two rows");

    // The same shape rules hold for Prune.
    w.advance_until_phase(PRUNE, 8);
    let cursor = w.ticket().cursor;
    let instruction = w.rt.prune_ix(id, cursor, 0, Vec::new());
    reject(
        &mut w,
        "prune_zero_rows",
        instruction,
        ErrorCode::InvalidState,
    );
    let plan = w.rt.prune_plan(8, false, true);
    let instruction = w.rt.prune_ix(id, cursor, 7, plan.remaining);
    reject(
        &mut w,
        "prune_wrong_account_count",
        instruction,
        ErrorCode::InvalidState,
    );
    w.advance_until_idle(8);

    let log = w.take_log();
    assert_all_fit(&log);
    log
}

/// The ABI note says `apply_senior` takes the program id when the ticket has
/// no open Series A.  A program id is always a READ-ONLY key of a compiled
/// message and the handler's `backer` is `#[account(mut)]`, so Anchor's mut
/// check is expected to refuse it (`ConstraintMut`, 2000).  That attempt is
/// recorded WITHOUT asserting its code; the driver passes a writable stand-in
/// (the Pool PDA, which `senior_step` never reads) and finishes the ticket.
pub fn senior_without_an_open_a_needs_a_writable_stand_in() -> Vec<Observation> {
    let row = ROW_SENIOR_NO_A;
    let owners = [who(0), who(1), who(2)];
    let mut w = World::with_backers(&[
        (owners[0], B, 1_000_000),
        (owners[1], B, 1_000_000),
        (owners[2], B, 1_000_000),
    ])
    .scoped_label(scope(row));
    w.start_ticket_fixture(10_000);
    w.advance_until_phase(SENIOR, 8);
    let ticket = w.ticket();
    assert_eq!((ticket.a_count, ticket.b_count), (0, 3));
    assert_eq!(
        (ticket.senior_slice, ticket.a_slice),
        (0, 0),
        "the missing A tier is folded"
    );
    // 5_000 initial seed + 1_750 senior folded to seed; B gets 1_000 + 2_250.
    assert_eq!((ticket.seed_credit, ticket.b_slice), (6_750, 3_250));
    let id = ticket.id;

    // `ApplySenior.backer` is optional: with no open A the program id is the
    // Anchor `None` placeholder and must be accepted (it used to fail Anchor's
    // `mut` check with ConstraintMut 2000 when the account was mandatory).
    let program_id = w.rt.senior_ix(id, ID);
    let attempt = w.send("program_id_placeholder", vec![program_id], &[]);
    let advanced = attempt.is_ok();
    assert!(
        advanced,
        "program-id placeholder must be accepted for a ticket with no open A: {attempt:?}"
    );
    assert_eq!(w.ticket().phase, SEAL);
    assert_eq!(w.ticket().senior_credit, 0);
    w.advance_until_idle(8);

    // 3_250 / 3 floors to 1_083 each; the one-lamport residue joined the seed
    // at Seal.
    for owner in owners {
        assert_eq!(w.backer(owner, B).pending, 1_083);
    }
    let pool = w.pool();
    assert_eq!((pool.seed, pool.funded_claims), (6_751, 3_249));
    w.assert_vault_identity();

    let mut log = w.take_log();
    assert_all_fit(&log);
    if advanced {
        promote(
            &mut log,
            &format!("{}.program_id_placeholder", scope(row)),
            &scope(row),
        );
    }
    log
}
