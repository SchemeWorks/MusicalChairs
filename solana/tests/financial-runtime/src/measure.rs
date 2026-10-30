//! M9 measurement matrix (plan Milestone 9, step 1): the numbers that decide
//! the chunk size, the inline threshold and the keeper count.
//!
//! Every observation here is a REAL transaction executed by the compiled
//! Musical Chairs SBF in LiteSVM (the close path also executes the captured
//! mainnet ORAO executable and the test-only Squads stand-in, exactly like the
//! M7/M8 suites).  The suite measures; it changes no program behaviour and
//! asserts no economics beyond what the existing suites already prove, but it
//! does assert its own fixtures (every open Backer is credited, every ticket
//! reaches `IDLE`, every transaction keeps both conservation identities).
//!
//! # Labels
//!
//! `financial.measure.<family>.<row>` is a row's headline observation (variant
//! `""`, the same convention as `close.rs` / `reserve.rs`) and
//! `financial.measure.<family>.<row>.<variant>` the supporting ones.  The
//! headline carries the row's summary as JSON in `Observation.unsupported`
//! (`family`, `row`, `fixture`, `result`, `rent_local_default`, ...).
//!
//! | family            | rows                                                     |
//! |-------------------|----------------------------------------------------------|
//! | `chunk`           | `n<N>_rows<R>`, N in 64/100/256/1024, R in 1/4/8 (12)     |
//! | `churn`           | `reopen_prune_20_cycles`                                  |
//! | `dust_claim`      | `rent_exempt_destination`, `fresh_destination_litesvm_accepts` |
//! | `full_close`      | `m100`                                                    |
//! | `inline_envelope` | `n0`, `n1`, `n2`, `n4`, `n8`, `n16`, `n17`                |
//! | `keeper_count`    | `pilot`                                                   |
//!
//! # What the numbers are, and are not
//!
//! * RENT: every figure is at LiteSVM's default rent schedule (see
//!   [`rent_schedule`], recorded in every headline).  It is neither a mainnet
//!   quote nor any SIMD-0437 step; the budget rebuild (plan step 3) re-prices.
//! * LIMITS: LiteSVM enforces neither the 1,232-byte legacy packet nor the
//!   64-account lock limit.  Both are evaluated by the harness
//!   (`fits_legacy_packet`, `message_account_count <= 64`) and recorded, never
//!   asserted: a row that exceeds a limit is evidence, not a failure.
//! * COMPUTE: `compute_units` is LiteSVM's `compute_units_consumed` of the
//!   whole transaction (the harness adds a `SetComputeUnitLimit` instruction and
//!   sets 1.4 M, so no row is ever cut short).
//! * ENVELOPE (`inline_envelope`): the transaction is a real `withdraw_simple`
//!   with extra remaining accounts the handler ignores.  It measures what an
//!   inline allocation would have to CARRY (packet, keys, locks, declared
//!   bytes) plus the payout's own compute; the allocation compute is an
//!   ESTIMATE built from the measured per-row phase costs.  It is not an
//!   inline implementation.
//! * CHURN is a cost measurement: a fixture of 61 never-capped Series B rows
//!   plus three open Series A rows that every ticket caps (see [`churn`]).
//! * DUST: the fresh-destination claim succeeds in LiteSVM, which does not
//!   check rent state of data-less accounts; the cluster would refuse it (see
//!   `tests/transfer_constraints.rs`).  Cluster behaviour is not asserted.
//!
//! The chunk family drops `logs` and `account_changes` from its per-step
//! observations (compute, packet, key counts, declared bytes and every other
//! field stay) to keep the CLI report small; the headline of each row keeps its
//! full observation.  `MC_MEASURE_N1024_ROWS` (comma list, default `1,4,8`)
//! restricts which batch sizes the 1,024-row population runs.

use std::{collections::BTreeMap, time::Instant};

use musical_chairs_economics::backers::{BackerKey, Book, Series};
use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    close as pclose, state, PageKind,
};
use serde_json::{json, Map, Value};
use solana_sdk::{
    instruction::AccountMeta,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
};

use crate::close::{
    annotate, apply_ix, assert_identities, breakeven, build_next, candidate_page_key, capture_ix,
    delta, do_request, expected_candidates, finalize_ix, hex, loser, loser_a, nonloser, nonloser_b,
    observe_ix, ok, participant, provision_candidate_pages, Part, Scene, Spec,
};
use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
use crate::reserve::close_page_ix;
use crate::runtime::{FinancialRuntime, Observation, COMPUTE_LIMIT, PACKET_LIMIT_BYTES};
use crate::world::{
    allocation_owner, BackerSpec, CloseRecordArg, PayoutKind, World, Xorshift64, DAY, SOL,
};

// ---------------------------------------------------------------------------
// Names, constants
// ---------------------------------------------------------------------------

pub const ROOT: &str = "financial.measure";

pub const FAMILY_CHUNK: &str = "chunk";
pub const FAMILY_CHURN: &str = "churn";
pub const FAMILY_DUST: &str = "dust_claim";
pub const FAMILY_FULL_CLOSE: &str = "full_close";
pub const FAMILY_ENVELOPE: &str = "inline_envelope";
pub const FAMILY_KEEPER: &str = "keeper_count";

pub const ROW_CHURN: &str = "reopen_prune_20_cycles";
pub const ROW_DUST_RENT_EXEMPT: &str = "rent_exempt_destination";
pub const ROW_DUST_FRESH: &str = "fresh_destination_litesvm_accepts";
pub const ROW_FULL_CLOSE: &str = "m100";
pub const ROW_KEEPER: &str = "pilot";

/// Seed of every random fixture of the matrix ("MCMEAS09").
pub const SEED: u64 = 0x4D43_4D45_4153_3039;
/// Registry populations of the chunk family.
pub const CHUNK_SIZES: [u64; 4] = [64, 100, 256, 1_024];
/// Batch sizes of the chunk family.
pub const CHUNK_ROWS: [u8; 3] = [1, 4, 8];
/// Inline envelope populations.
pub const ENVELOPE_NS: [u64; 7] = [0, 1, 2, 4, 8, 16, 17];
/// Reopen / prune cycles of the churn row.
pub const CHURN_CYCLES: usize = 20;
/// Participants of the full close path (the closer plus 99).
pub const CLOSE_PARTICIPANTS: u64 = 100;
/// The pilot of `docs/coordination/native-solana-icp-budget.md`.
pub const PILOT_PAYOUTS: u64 = 1_000;
pub const PILOT_CLOSES: u64 = 10;
pub const PILOT_BACKERS: u64 = 100;
pub const PILOT_PARTICIPANTS: u64 = 100;

/// Phase instructions in ticket order (the label token after the scope).
pub const PHASES: [&str; 6] = ["discover", "senior", "seal", "allocate", "prune", "commit"];

const PAGE: u64 = state::PAGE_CAPACITY as u64;
/// Solana's account-lock limit per transaction (all keys count).
const MAX_ACCOUNT_LOCKS: usize = 64;
const SIGNATURE_FEE_LAMPORTS: u64 = 5_000;
/// Official production `key_1` threshold-signature price (ICP budget doc).
const CYCLES_PER_SIGNATURE: u128 = 26_153_846_153;
/// `fund_series_a` allows 3 successes per owner per hour.
const QUOTA_WARP_SECONDS: u64 = 3_601;

pub fn label(family: &str, row: &str) -> String {
    format!("{ROOT}.{family}.{row}")
}

pub fn chunk_row(n: u64, rows: u8) -> String {
    format!("n{n}_rows{rows}")
}

pub fn envelope_row(n: u64) -> String {
    format!("n{n}")
}

/// A row must contain an observation labelled exactly
/// `financial.measure.<family>.<row>`.
pub fn assert_headline(observations: &[Observation], family: &str, row: &str) {
    let headline = label(family, row);
    assert!(
        observations
            .iter()
            .any(|observation| observation.label == headline),
        "row {family}.{row} produced no headline observation {headline}"
    );
}

/// Batch sizes the 1,024-row population runs: `MC_MEASURE_N1024_ROWS`
/// (comma list of 1 / 4 / 8), default all three.
pub fn n1024_rows() -> Vec<u8> {
    match std::env::var("MC_MEASURE_N1024_ROWS") {
        Ok(text) => {
            let mut rows: Vec<u8> = text
                .split(',')
                .map(|token| {
                    let value: u8 = token.trim().parse().unwrap_or_else(|_| {
                        panic!("MC_MEASURE_N1024_ROWS: `{token}` is not a number")
                    });
                    assert!(
                        CHUNK_ROWS.contains(&value),
                        "MC_MEASURE_N1024_ROWS: {value} is not one of 1, 4, 8"
                    );
                    value
                })
                .collect();
            rows.sort_unstable();
            rows.dedup();
            assert!(
                !rows.is_empty(),
                "MC_MEASURE_N1024_ROWS names no batch size"
            );
            rows
        }
        Err(_) => CHUNK_ROWS.to_vec(),
    }
}

/// The `(N, rows)` runs of the chunk family, in execution order.
pub fn chunk_plan() -> Vec<(u64, u8)> {
    let big = n1024_rows();
    let mut plan = Vec::new();
    for &n in &CHUNK_SIZES {
        for &rows in &CHUNK_ROWS {
            if n == 1_024 && !big.contains(&rows) {
                continue;
            }
            plan.push((n, rows));
        }
    }
    plan
}

/// Every planned headline label of the matrix, in suite order.
pub fn planned_labels() -> Vec<String> {
    let mut labels: Vec<String> = chunk_plan()
        .into_iter()
        .map(|(n, rows)| label(FAMILY_CHUNK, &chunk_row(n, rows)))
        .collect();
    labels.push(label(FAMILY_CHURN, ROW_CHURN));
    labels.push(label(FAMILY_DUST, ROW_DUST_RENT_EXEMPT));
    labels.push(label(FAMILY_DUST, ROW_DUST_FRESH));
    labels.push(label(FAMILY_FULL_CLOSE, ROW_FULL_CLOSE));
    labels.extend(
        ENVELOPE_NS
            .iter()
            .map(|&n| label(FAMILY_ENVELOPE, &envelope_row(n))),
    );
    labels.push(label(FAMILY_KEEPER, ROW_KEEPER));
    labels
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The rent schedule every figure of the matrix is priced at.
pub fn rent_schedule(rt: &FinancialRuntime) -> Value {
    let rent: Rent = rt.svm.get_sysvar::<Rent>();
    let account = |space: usize| json!({ "space_bytes": space, "rent_lamports": rt.rent(space) });
    let config: state::ConfigVersion = rt.deserialize(rt.config_address(1));
    json!({
        "label": "LOCAL-DEFAULT RENT: LiteSVM's Rent sysvar (Rent::default()), not a mainnet quote and not any SIMD-0437 step",
        "lamports_per_byte_year": rent.lamports_per_byte_year,
        "exemption_threshold_years": rent.exemption_threshold,
        "rent_exempt_zero_data_account": rt.rent(0),
        "per_data_byte_lamports": rt.rent(1) - rt.rent(0),
        "accounts": {
            "Pool": account(state::Pool::SPACE),
            "Admission": account(state::Admission::SPACE),
            "GameVault": account(state::GameVault::SPACE),
            "Ops": account(state::Ops::SPACE),
            "RngAuthority": account(state::RngAuthority::SPACE),
            "ConfigVersion": account(state::ConfigVersion::SPACE),
            "User": account(state::User::SPACE),
            "UserRound": account(state::UserRound::SPACE),
            "Position": account(state::Position::SPACE),
            "Backer": account(state::Backer::SPACE),
            "RegistryPage": account(state::RegistryPage::SPACE),
            "SnapshotPage": account(state::SnapshotPage::SPACE),
            "Ticket": account(state::Ticket::SPACE),
            "ParticipantPage": account(state::ParticipantPage::SPACE),
            "CandidatePage": account(state::CandidatePage::SPACE),
            "CloseRecord": account(state::CloseRecord::SPACE),
            "ORAO pending request (config)": account(usize::from(config.pending_request_bytes)),
            "ORAO fulfilled request (config)": account(usize::from(config.fulfilled_request_bytes)),
        }
    })
}

fn limits_json() -> Value {
    json!({
        "legacy_packet_bytes": PACKET_LIMIT_BYTES,
        "account_locks": MAX_ACCOUNT_LOCKS,
        "compute_unit_limit_set_by_the_harness": COMPUTE_LIMIT,
        "enforced_by_litesvm": false,
        "evaluated_by_the_harness": true
    })
}

/// Seconds with one decimal (wall clock is a convenience, not evidence).
fn seconds(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 10.0).round() / 10.0
}

/// A transaction is "within limits" when it fits the legacy packet, the
/// 64-lock bound and the harness compute limit.
pub fn within_limits(observation: &Observation) -> bool {
    observation.fits_legacy_packet
        && observation.message_account_count <= MAX_ACCOUNT_LOCKS
        && observation.compute_units.unwrap_or(0) <= u64::from(COMPUTE_LIMIT)
}

/// The token after `scope.` in a label (`discover.3` -> `discover`).
fn phase_of<'a>(label: &'a str, scope: &str) -> Option<&'a str> {
    label
        .strip_prefix(scope)?
        .strip_prefix('.')?
        .split('.')
        .next()
}

/// The part of a label after `scope.` (empty for the bare scope).
fn variant_of<'a>(label: &'a str, scope: &str) -> &'a str {
    label
        .strip_prefix(scope)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or("")
}

/// Relabel the one observation labelled `from` as `to` (a row's headline).
fn promote(log: &mut [Observation], from: &str, to: &str) {
    let observation = log
        .iter_mut()
        .find(|observation| observation.label == from)
        .unwrap_or_else(|| panic!("no observation labelled {from} to promote"));
    observation.label = to.to_string();
}

fn note(value: &Value) -> String {
    serde_json::to_string(value).expect("measurement note JSON")
}

/// Per-phase aggregate of a set of transactions.
#[derive(Clone, Debug)]
pub struct PhaseStats {
    pub tx: usize,
    pub cu_min: u64,
    pub cu_max: u64,
    pub cu_sum: u64,
    pub packet_max: usize,
    pub keys_max: usize,
    pub writable_max: usize,
    pub declared_bytes_max: usize,
    pub all_fit_packet: bool,
    pub all_within_limits: bool,
}

impl PhaseStats {
    fn new() -> Self {
        Self {
            tx: 0,
            cu_min: u64::MAX,
            cu_max: 0,
            cu_sum: 0,
            packet_max: 0,
            keys_max: 0,
            writable_max: 0,
            declared_bytes_max: 0,
            all_fit_packet: true,
            all_within_limits: true,
        }
    }

    fn add(&mut self, observation: &Observation) {
        let cu = observation.compute_units.unwrap_or(0);
        self.tx += 1;
        self.cu_min = self.cu_min.min(cu);
        self.cu_max = self.cu_max.max(cu);
        self.cu_sum += cu;
        self.packet_max = self.packet_max.max(observation.packet_bytes);
        self.keys_max = self.keys_max.max(observation.message_account_count);
        self.writable_max = self.writable_max.max(observation.writable_account_count);
        self.declared_bytes_max = self
            .declared_bytes_max
            .max(observation.declared_account_data_bytes);
        self.all_fit_packet &= observation.fits_legacy_packet;
        self.all_within_limits &= within_limits(observation);
    }

    pub fn cu_mean(&self) -> u64 {
        if self.tx == 0 {
            0
        } else {
            self.cu_sum / self.tx as u64
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "tx": self.tx,
            "cu_min": if self.tx == 0 { 0 } else { self.cu_min },
            "cu_max": self.cu_max,
            "cu_mean": self.cu_mean(),
            "cu_sum": self.cu_sum,
            "packet_bytes_max": self.packet_max,
            "message_keys_max": self.keys_max,
            "writable_accounts_max": self.writable_max,
            "declared_account_data_bytes_max": self.declared_bytes_max,
            "fits_legacy_packet": self.all_fit_packet,
            "within_all_limits": self.all_within_limits,
        })
    }
}

/// Aggregate `observations` by the phase token after `scope.`.
pub fn phase_table(observations: &[Observation], scope: &str) -> BTreeMap<String, PhaseStats> {
    let mut table: BTreeMap<String, PhaseStats> = BTreeMap::new();
    for observation in observations {
        if let Some(phase) = phase_of(&observation.label, scope) {
            table
                .entry(phase.to_string())
                .or_insert_with(PhaseStats::new)
                .add(observation);
        }
    }
    table
}

fn phases_json(table: &BTreeMap<String, PhaseStats>) -> Value {
    let mut map = Map::new();
    for phase in PHASES {
        if let Some(stats) = table.get(phase) {
            map.insert(phase.to_string(), stats.to_json());
        }
    }
    Value::Object(map)
}

/// One observation as a table row.
fn observation_json(observation: &Observation) -> Value {
    json!({
        "label": observation.label,
        "success": observation.success,
        "compute_units": observation.compute_units,
        "packet_bytes": observation.packet_bytes,
        "fits_legacy_packet": observation.fits_legacy_packet,
        "message_keys": observation.message_account_count,
        "writable_accounts": observation.writable_account_count,
        "declared_account_data_bytes": observation.declared_account_data_bytes,
        "within_all_limits": within_limits(observation),
    })
}

/// Keep every measurement field, drop the bulky `logs` / `account_changes`.
fn compact(observation: &mut Observation) {
    observation.logs.clear();
    observation.account_changes.clear();
}

// ---------------------------------------------------------------------------
// Family 1: chunk populations
// ---------------------------------------------------------------------------

/// A registry of `n` deterministic Backers: Series A about two thirds, B the
/// rest.  Rows 0..=3 are fixed (the oldest open A with 100,000 entitlement, an
/// open B holding 30, a closed A holding 5, a closed B holding exactly the
/// 10-lamport cutoff); every other row is closed (1..=10 remaining) one time
/// in ten, "just open" (11..=60) one time in ten and ordinary
/// (50,000..=150,000) otherwise.  Closed rows give Prune work, and the toll
/// ([`solve_toll`]) is sized so every open row is credited.
pub fn measure_specs(n: usize) -> Vec<BackerSpec> {
    let mut rng = Xorshift64::new(SEED ^ n as u64);
    (0..n)
        .map(|index| {
            let owner = allocation_owner(index as u64);
            let (series, remaining) = match index {
                0 => (SERIES_A, 100_000u128),
                1 => (SERIES_B, 30),
                2 => (SERIES_A, 5),
                3 => (SERIES_B, 10),
                _ => {
                    let series = if rng.next().is_multiple_of(3) {
                        SERIES_B
                    } else {
                        SERIES_A
                    };
                    let remaining = match rng.next() % 10 {
                        0 => 1 + u128::from(rng.next() % 10),
                        1 => 11 + u128::from(rng.next() % 50),
                        _ => 50_000 + u128::from(rng.next() % 100_001),
                    };
                    (series, remaining)
                }
            };
            BackerSpec::new(owner, series, remaining)
        })
        .collect()
}

fn host_series(series: u8) -> Series {
    if series == SERIES_A {
        Series::A
    } else {
        Series::B
    }
}

/// The host `Book` (the differential-tested oracle of the program) holding
/// exactly `specs`, built once so every candidate toll starts from a clone.
fn host_book(specs: &[BackerSpec]) -> Book {
    let mut book = Book::with_pot(u64::MAX / 2);
    for (index, spec) in specs.iter().enumerate() {
        book.add_backer_with_lifetime(
            index as u64,
            host_series(spec.series),
            spec.entitlement,
            spec.lifetime,
        )
        .unwrap_or_else(|error| panic!("host Book refused fixture Backer {index}: {error:?}"));
    }
    book
}

/// `true` when `toll` credits every open row of `specs` at least one lamport
/// on the host `Book` built from them.
fn every_open_row_is_credited(base: &Book, specs: &[BackerSpec], toll: u64) -> bool {
    let mut book = base.clone();
    if book.allocate(toll, 8).is_err() {
        return false;
    }
    specs.iter().enumerate().all(|(index, spec)| {
        spec.remaining() <= state::BACKER_REMAINING_CUTOFF
            || book
                .backer(BackerKey::new(index as u64, host_series(spec.series)))
                .is_ok_and(|row| row.lifetime_credited > spec.lifetime)
    })
}

/// The toll of a population: the smallest doubling (from `1,000 * N`) the host
/// `Book` credits every open row for, times four, re-checked.
pub fn solve_toll(specs: &[BackerSpec]) -> u64 {
    let base = host_book(specs);
    let mut toll = (specs.len() as u64).max(1) * 1_000;
    while !every_open_row_is_credited(&base, specs, toll) {
        toll = toll
            .checked_mul(2)
            .filter(|next| *next < (1u64 << 50))
            .unwrap_or_else(|| {
                panic!(
                    "no toll below 2^50 credits every open row of {} Backers",
                    specs.len()
                )
            });
    }
    let wide = toll * 4;
    assert!(
        every_open_row_is_credited(&base, specs, wide),
        "toll {wide} (four times the smallest crediting toll) no longer credits every open row"
    );
    wide
}

/// One complete allocation ticket over a population at one batch size.
pub struct ChunkRun {
    pub n: u64,
    pub rows: u8,
    pub toll: u64,
    pub scope: String,
    /// Real transactions, `logs` / `account_changes` dropped except on the
    /// headline (the Commit, relabelled to the bare row label).
    pub observations: Vec<Observation>,
    pub phases: BTreeMap<String, PhaseStats>,
    pub tx_total: usize,
    pub registry_len_after: u64,
    pub rows_pruned: u64,
    pub open_rows: u64,
    pub fits_all: bool,
}

impl ChunkRun {
    pub fn phase(&self, name: &str) -> &PhaseStats {
        self.phases
            .get(name)
            .unwrap_or_else(|| panic!("{}: no `{name}` transactions", self.scope))
    }
}

/// Run one ticket over `specs` (a population of `n` Backers) at `rows` rows per
/// instruction.  Panics when the ticket does not reach `IDLE`, when a row that
/// was open at the start is not credited, or when a phase has the wrong number
/// of transactions.
pub fn chunk_run(n: u64, rows: u8, toll: u64, specs: &[BackerSpec]) -> ChunkRun {
    assert_eq!(
        specs.len() as u64,
        n,
        "the fixture is a population of exactly N Backers"
    );
    let row = chunk_row(n, rows);
    let scope = label(FAMILY_CHUNK, &row);
    let started = Instant::now();
    let mut w = World::with_backer_specs(specs).scoped_label(scope.clone());
    let open_rows = specs
        .iter()
        .filter(|spec| spec.remaining() > state::BACKER_REMAINING_CUTOFF)
        .count() as u64;
    let series_a = specs.iter().filter(|spec| spec.series == SERIES_A).count() as u64;
    w.start_ticket_fixture(toll);
    w.advance_until_idle(rows);
    w.assert_vault_identity();

    let pool = w.pool();
    let ticket = w.ticket();
    assert_eq!(ticket.phase, state::IDLE, "{scope}: the ticket is retired");
    assert_eq!(
        ticket.credited + ticket.seed_credit,
        toll,
        "{scope}: credited + seed_credit == toll"
    );
    let credited = specs
        .iter()
        .filter(|spec| {
            spec.remaining() > state::BACKER_REMAINING_CUTOFF
                && w.backer(spec.owner, spec.series).lifetime > spec.lifetime
        })
        .count() as u64;
    assert_eq!(
        credited, open_rows,
        "{scope}: every row that was open is credited at least one lamport (toll {toll})"
    );
    let rent = rent_schedule(&w.rt);
    let mut observations = w.take_log();
    let registry_len_after = pool.registry_len;
    let rows_pruned = n - registry_len_after;

    let table = phase_table(&observations, &scope);
    let batches = n.div_ceil(u64::from(rows)) as usize;
    for phase in ["discover", "allocate"] {
        assert_eq!(
            table.get(phase).map_or(0, |s| s.tx),
            batches,
            "{scope}: {phase} batches"
        );
    }
    for phase in ["senior", "seal", "commit"] {
        assert_eq!(
            table.get(phase).map_or(0, |s| s.tx),
            1,
            "{scope}: one {phase} transaction"
        );
    }
    assert!(
        table.get("prune").map_or(0, |s| s.tx) >= batches,
        "{scope}: Prune visits every captured row (a chained move only adds batches)"
    );
    assert!(
        observations.iter().all(|observation| observation.success),
        "{scope}: the happy path never fails"
    );

    let tx_total = observations.len();
    let fits_all = observations.iter().all(within_limits);
    let tx_by_phase: Map<String, Value> = PHASES
        .iter()
        .map(|phase| {
            (
                (*phase).to_string(),
                json!(table.get(*phase).map_or(0, |s| s.tx)),
            )
        })
        .collect();
    let summary = json!({
        "family": FAMILY_CHUNK,
        "row": row,
        "what": "one complete allocation ticket (Discover, Senior, Seal, Allocate, Prune, Commit) over a registry of N fixture Backers at `rows` rows per instruction; every observation of the row is a real transaction (labels <row>.<phase>.<ordinal>), the headline is the Commit",
        "fixture": {
            "n": n,
            "rows_per_instruction": rows,
            "seed": format!("0x{SEED:016X}"),
            "population_rule": "measure_specs(N): rows 0..=3 fixed (oldest open A 100,000; open B 30; closed A 5; closed B 10), the rest A two thirds / B one third, closed (1..=10) 10%, just open (11..=60) 10%, ordinary (50,000..=150,000) 80%",
            "toll_lamports": toll,
            "toll_rule": "four times the smallest toll (doubling from 1,000 x N) for which the host Book credits every open row at least one lamport",
            "series_a_rows": series_a,
            "series_b_rows": n - series_a,
            "open_rows_at_start": open_rows,
            "closed_rows_at_start": n - open_rows,
        },
        "result": {
            "tx_total": tx_total,
            "tx_by_phase": tx_by_phase,
            "registry_len_after": registry_len_after,
            "rows_pruned": rows_pruned,
            "open_rows_credited": credited,
            "credited_plus_seed_equals_toll": true,
            "every_transaction_within_all_limits": fits_all,
        },
        "phases": phases_json(&table),
        "limits": limits_json(),
        "rent_local_default": rent,
        "wall_clock_s_not_evidence": seconds(started),
    });

    for observation in &mut observations {
        compact(observation);
    }
    let commit = format!("{scope}.commit.0");
    promote(&mut observations, &commit, &scope);
    annotate(&mut observations, &scope, note(&summary));
    ChunkRun {
        n,
        rows,
        toll,
        scope,
        observations,
        phases: table,
        tx_total,
        registry_len_after,
        rows_pruned,
        open_rows,
        fits_all,
    }
}

/// [`chunk_run`] for a population and batch size, building the fixture and the
/// toll (what the standalone tests use).
pub fn chunk_run_for(n: u64, rows: u8) -> ChunkRun {
    let specs = measure_specs(n as usize);
    let toll = solve_toll(&specs);
    chunk_run(n, rows, toll, &specs)
}

/// The whole chunk family (12 runs unless `MC_MEASURE_N1024_ROWS` narrows the
/// 1,024-row population).
pub fn chunk_matrix() -> Vec<ChunkRun> {
    let plan = chunk_plan();
    let mut runs = Vec::new();
    for &n in &CHUNK_SIZES {
        let wanted: Vec<u8> = plan
            .iter()
            .filter(|(population, _)| *population == n)
            .map(|(_, rows)| *rows)
            .collect();
        if wanted.is_empty() {
            continue;
        }
        let specs = measure_specs(n as usize);
        let toll = solve_toll(&specs);
        for rows in wanted {
            runs.push(chunk_run(n, rows, toll, &specs));
        }
    }
    runs
}

/// The chunk family as one observation list.
pub fn run_chunk_family() -> Vec<Observation> {
    chunk_matrix()
        .into_iter()
        .flat_map(|run| run.observations)
        .collect()
}

// ---------------------------------------------------------------------------
// Family 2: churn (reopen / prune cycles)
// ---------------------------------------------------------------------------

/// Never-capped Series B rows that fill the registry to 64 with the three open
/// Series A rows ([`CHURN_BALLAST`] + 3).
const CHURN_BALLAST: u64 = 61;
/// 10^15 lamports of remaining entitlement: no toll of this row caps it.
const CHURN_BALLAST_REMAINING: u128 = 1_000_000_000_000_000;
/// Sized so that, with exactly three open Series A rows of 62,000,000 remaining
/// each, the senior slice (175,000,000) caps the oldest and the A slice
/// (225,000,000, shared in proportion to remaining over the other two)
/// credits each of them its whole 62,000,000.
const CHURN_TOLL: u64 = 1_000_000_000;
/// The minimum Series A funding; mints 62,000,000 of entitlement.
const CHURN_GROSS: u64 = 50_000_000;

fn churn_actor(index: u64) -> Keypair {
    let mut seed = [0x9C; 32];
    seed[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Keypair::new_from_array(seed)
}

/// `reopen_prune_20_cycles`: [`CHURN_CYCLES`] reopen / prune cycles on a
/// 64-row registry.
///
/// Fixture: 61 Series B rows nobody can cap (10^15 remaining) plus three open
/// Series A rows funded by REAL `fund_series_a` transactions.  Each cycle:
///
/// 1. a toll of 1,000,000,000 (a fixture-started ticket, exactly like the
///    chunk family, driven at 8 rows) caps ALL THREE open A rows (the oldest by
///    the senior slice, the other two by the A slice) and Prune removes them;
/// 2. the two oldest of them are re-funded with `fund_series_a` (50,000,000
///    gross, reopening the capped row with its original funding sequence at
///    the registry tail) and a brand-new Series A funder opens a third row, so
///    the next ticket again has exactly three open A rows.
///
/// Why a new funder: every open A row shares one credit-to-remaining ratio, so
/// a ticket caps either all open A rows or none (the senior slice aside);
/// "cap three per ticket" therefore needs exactly three open A rows each
/// time, and re-funding two of three needs one new row per cycle to keep it
/// so.  The clock is warped 3,601 s before every funding (3 successes per
/// owner per hour).
///
/// Asserted per cycle: `registry_len` 64 -> 61 -> 64, the three capped rows
/// leave the registry, both conservation identities after every transaction
/// (the World does that), and the financial digest differs from the previous
/// snapshot.  The Book is not used: this is a cost measurement.
pub fn churn() -> Vec<Observation> {
    let root = label(FAMILY_CHURN, ROW_CHURN);
    let started = Instant::now();
    let mut w = World::genesis().scoped_label(format!("{root}.c00"));
    let ballast: Vec<BackerSpec> = (0..CHURN_BALLAST)
        .map(|index| {
            BackerSpec::new(
                allocation_owner(10_000 + index),
                SERIES_B,
                CHURN_BALLAST_REMAINING,
            )
        })
        .collect();
    w.install_backer_specs(&ballast);
    let round_id = w.pool().round_id;
    let actors: Vec<Keypair> = (0..3 + CHURN_CYCLES as u64).map(churn_actor).collect();
    for actor in &actors {
        w.rt.credit_lamports(actor.pubkey(), 20 * SOL);
    }

    // Cycle 0: the three opening funders (first-time funding, real transactions).
    let mut open: Vec<usize> = vec![0, 1, 2];
    for &actor in &open {
        w.warp_seconds(QUOTA_WARP_SECONDS);
        w.fund_series_a(
            &format!("first_fund.a{actor:02}"),
            &actors[actor],
            CHURN_GROSS,
            round_id,
        )
        .unwrap_or_else(|failed| panic!("opening Series A funding failed: {failed:?}"));
    }
    assert_eq!(
        w.pool().registry_len,
        CHURN_BALLAST + 3,
        "61 Series B + 3 open Series A"
    );

    let mut next_fresh = 3usize;
    let mut cycles: Vec<Value> = Vec::new();
    let mut digests: Vec<[u8; 32]> = vec![w.digest_financial_state()];
    let mut max_prune: (u64, usize) = (0, 0);
    let mut max_refund: (u64, usize) = (0, 0);
    let mut max_first_fund: (u64, usize) = (0, 0);
    let mut max_cycle_tx = 0usize;
    for cycle in 1..=CHURN_CYCLES {
        let scope = format!("{root}.c{cycle:02}");
        w = w.scoped_label(scope.clone());
        assert_eq!(
            w.pool().registry_len,
            CHURN_BALLAST + 3,
            "{scope}: 64 rows before the ticket"
        );

        // 1. One ticket caps the three open A rows.
        w.start_ticket_fixture(CHURN_TOLL);
        let mut ticket_steps: Vec<Observation> = Vec::new();
        while w.ticket().phase != state::IDLE {
            ticket_steps.push(w.advance_step(8));
            assert!(
                ticket_steps.len() < 1_000,
                "{scope}: the ticket did not converge"
            );
        }
        assert_eq!(
            w.pool().registry_len,
            CHURN_BALLAST,
            "{scope}: the three capped A rows were pruned"
        );
        for &actor in &open {
            let backer = w.backer(actors[actor].pubkey(), SERIES_A);
            assert!(!backer.active, "{scope}: actor {actor} left the registry");
            assert_eq!(
                backer.remaining().expect("remaining"),
                0,
                "{scope}: actor {actor} was credited its whole remaining entitlement"
            );
        }
        let after_ticket = w.digest_financial_state();
        assert_ne!(
            after_ticket,
            *digests.last().expect("a digest"),
            "{scope}: the ticket changed the books"
        );
        digests.push(after_ticket);

        // 2. Two re-fundings and one brand-new funder.
        let refunded = [open[0], open[1]];
        let fresh = next_fresh;
        next_fresh += 1;
        let mut refund_steps: Vec<Observation> = Vec::new();
        for &actor in &refunded {
            w.warp_seconds(QUOTA_WARP_SECONDS);
            refund_steps.push(
                w.fund_series_a(
                    &format!("refund.a{actor:02}"),
                    &actors[actor],
                    CHURN_GROSS,
                    round_id,
                )
                .unwrap_or_else(|failed| panic!("{scope}: re-funding failed: {failed:?}")),
            );
        }
        w.warp_seconds(QUOTA_WARP_SECONDS);
        let first_fund = w
            .fund_series_a(
                &format!("first_fund.a{fresh:02}"),
                &actors[fresh],
                CHURN_GROSS,
                round_id,
            )
            .unwrap_or_else(|failed| panic!("{scope}: first funding failed: {failed:?}"));
        open = vec![refunded[0], refunded[1], fresh];
        assert_eq!(
            w.pool().registry_len,
            CHURN_BALLAST + 3,
            "{scope}: back to 64 rows"
        );
        for &actor in &open {
            assert!(
                w.backer(actors[actor].pubkey(), SERIES_A).active,
                "{scope}: actor {actor} is open again"
            );
        }
        let after_refunds = w.digest_financial_state();
        assert_ne!(
            after_refunds,
            *digests.last().expect("a digest"),
            "{scope}: the funding changed the books"
        );
        digests.push(after_refunds);

        // Per-cycle figures.
        let ticket_table = phase_table(&ticket_steps, &scope);
        let ticket_tx = ticket_steps.len();
        let tx_this_cycle = ticket_tx + refund_steps.len() + 1;
        max_cycle_tx = max_cycle_tx.max(tx_this_cycle);
        let prune = ticket_table.get("prune").expect("prune transactions");
        let refund_cu = refund_steps
            .iter()
            .map(|o| o.compute_units.unwrap_or(0))
            .max()
            .unwrap_or(0);
        let refund_packet = refund_steps
            .iter()
            .map(|o| o.packet_bytes)
            .max()
            .unwrap_or(0);
        max_prune = (
            max_prune.0.max(prune.cu_max),
            max_prune.1.max(prune.packet_max),
        );
        max_refund = (max_refund.0.max(refund_cu), max_refund.1.max(refund_packet));
        max_first_fund = (
            max_first_fund.0.max(first_fund.compute_units.unwrap_or(0)),
            max_first_fund.1.max(first_fund.packet_bytes),
        );
        let tx_by_phase: Map<String, Value> = PHASES
            .iter()
            .map(|phase| {
                (
                    (*phase).to_string(),
                    json!(ticket_table.get(*phase).map_or(0, |s| s.tx)),
                )
            })
            .collect();
        cycles.push(json!({
            "cycle": cycle,
            "registry_len": { "before_ticket": CHURN_BALLAST + 3, "after_ticket": CHURN_BALLAST, "after_refunds": CHURN_BALLAST + 3 },
            "rows_pruned": 3,
            "ticket_tx": ticket_tx,
            "ticket_tx_by_phase": tx_by_phase,
            "refund_tx": refund_steps.len(),
            "first_funding_tx": 1,
            "total_tx": tx_this_cycle,
            "prune_cu_max": prune.cu_max,
            "prune_packet_bytes_max": prune.packet_max,
            "prune_message_keys_max": prune.keys_max,
            "refund_cu_max": refund_cu,
            "refund_packet_bytes_max": refund_packet,
            "first_funding_cu": first_fund.compute_units,
            "first_funding_packet_bytes": first_fund.packet_bytes,
            "digest_after_ticket": hex(&after_ticket),
            "digest_after_refunds": hex(&after_refunds),
        }));
    }
    let pool = w.pool();
    assert_eq!(
        pool.next_ticket,
        CHURN_CYCLES as u64 + 1,
        "{CHURN_CYCLES} tickets ran"
    );
    assert_eq!(pool.registry_len, CHURN_BALLAST + 3);
    w.assert_vault_identity();

    let rent = rent_schedule(&w.rt);
    let mut log = w.take_log();
    let tx_total = log.len();
    assert!(
        log.iter().all(|observation| observation.success),
        "every churn transaction succeeds"
    );
    // The headline: the last cycle's most expensive Prune (the batch that
    // really removes the three capped rows).
    let last_scope = format!("{root}.c{CHURN_CYCLES:02}");
    let headline_from = log
        .iter()
        .filter(|observation| phase_of(&observation.label, &last_scope) == Some("prune"))
        .max_by_key(|observation| observation.compute_units.unwrap_or(0))
        .map(|observation| observation.label.clone())
        .expect("the last cycle pruned");
    let distinct_digests = digests
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let summary = json!({
        "family": FAMILY_CHURN,
        "row": ROW_CHURN,
        "what": "20 reopen/prune cycles on a 64-row registry; the headline is the last cycle's most expensive Prune transaction",
        "fixture": {
            "registry": "61 Series B rows (10^15 remaining, never capped) + 3 open Series A rows of 62,000,000 remaining",
            "toll_lamports": CHURN_TOLL,
            "rows_per_instruction": 8,
            "series_a_funding_gross": CHURN_GROSS,
            "quota_warp_seconds_before_every_funding": QUOTA_WARP_SECONDS,
            "per_cycle": "one ticket caps and prunes all three open A rows; the two oldest are re-funded; one new funder opens the third row",
            "why_a_new_funder": "every open A row shares one credit-to-remaining ratio, so a ticket caps all open A rows or none; keeping exactly three open per ticket while re-funding two needs one new row per cycle",
            "actors": actors.len(),
        },
        "result": {
            "cycles": CHURN_CYCLES,
            "tx_total": tx_total,
            "max_tx_in_one_cycle": max_cycle_tx,
            "prune": { "cu_max": max_prune.0, "packet_bytes_max": max_prune.1 },
            "refund_series_a": { "cu_max": max_refund.0, "packet_bytes_max": max_refund.1 },
            "first_funding_series_a": { "cu_max": max_first_fund.0, "packet_bytes_max": max_first_fund.1 },
            "registry_len_final": pool.registry_len,
            "next_ticket_final": pool.next_ticket,
            "digest_snapshots": digests.len(),
            "digest_snapshots_distinct": distinct_digests,
            "every_transaction_succeeded": true,
        },
        "per_cycle": cycles,
        "limits": limits_json(),
        "rent_local_default": rent,
        "wall_clock_s_not_evidence": seconds(started),
    });
    assert_eq!(
        distinct_digests,
        digests.len(),
        "every snapshot of the financial state is new"
    );
    promote(&mut log, &headline_from, &root);
    annotate(&mut log, &root, note(&summary));
    log
}

// ---------------------------------------------------------------------------
// Family 3: dust claims
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    /// Pre-funded with exactly the rent-exempt minimum of a data-less account.
    RentExempt,
    /// Never funded: no account at all.
    Fresh,
}

fn dust_owner(index: u64) -> Keypair {
    let mut seed = [0xD5; 32];
    seed[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Keypair::new_from_array(seed)
}

fn dust_destination(index: u64, kind: Destination) -> Pubkey {
    let mut bytes = match kind {
        Destination::RentExempt => [0xE1; 32],
        Destination::Fresh => [0xE2; 32],
    };
    bytes[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Pubkey::new_from_array(bytes)
}

/// Ten Series A Backers owed 1..=10 lamports (fixture `funded`, finalized
/// credit) each claim to `kind` destination.  The rent-exempt row adds one
/// owner holding BOTH series (owed 3 + 4).  Returns the row's observations; the
/// headline is the claim owed 10 lamports.
fn dust_row(row: &str, kind: Destination) -> Vec<Observation> {
    let scope = label(FAMILY_DUST, row);
    let started = Instant::now();
    let mut w = World::genesis().scoped_label(scope.clone());
    let owners: Vec<Keypair> = (0..10).map(dust_owner).collect();
    let specs: Vec<BackerSpec> = owners
        .iter()
        .map(|owner| BackerSpec::new(owner.pubkey(), SERIES_A, 1_000))
        .collect();
    w.install_backer_specs(&specs);
    for owner in &owners {
        // A real Series A Backer always has a `User`; `claim_backer` requires it.
        w.install_user(owner.pubkey(), true, false);
    }
    for (index, owner) in owners.iter().enumerate() {
        w.fund_backer_fixture(owner.pubkey(), SERIES_A, index as u64 + 1);
    }
    let minimum = w.rt.rent(0);

    let mut table: Vec<Value> = Vec::new();
    let mut accepted_all = true;
    for (index, owner) in owners.iter().enumerate() {
        let owed = index as u64 + 1;
        let destination = dust_destination(index as u64, kind);
        if kind == Destination::RentExempt {
            w.rt.credit_lamports(destination, minimum);
        }
        let before = w.lamports(destination);
        let variant = format!("owed_{owed:02}");
        match w.claim_backer(&variant, owner, destination) {
            Ok(paid) => {
                assert_eq!(paid, owed, "{scope}: the claim pays exactly what is owed");
                assert_eq!(w.lamports(destination), before + owed);
            }
            Err(failed) => {
                assert_eq!(
                    kind,
                    Destination::Fresh,
                    "{scope}: only a fresh destination may be refused: {failed:?}"
                );
                accepted_all = false;
            }
        }
    }
    if kind == Destination::RentExempt {
        // One owner holding both series, owed 3 + 4: the combined claim.
        let both = dust_owner(10);
        w.install_backer_specs(&[
            BackerSpec::new(both.pubkey(), SERIES_A, 1_000),
            BackerSpec::new(both.pubkey(), SERIES_B, 1_000),
        ]);
        w.install_user(both.pubkey(), true, true);
        w.fund_backer_fixture(both.pubkey(), SERIES_A, 3);
        w.fund_backer_fixture(both.pubkey(), SERIES_B, 4);
        let destination = dust_destination(10, kind);
        w.rt.credit_lamports(destination, minimum);
        let paid = w
            .claim_backer("owed_a3_b4_combined", &both, destination)
            .unwrap_or_else(|failed| panic!("{scope}: the combined dust claim failed: {failed:?}"));
        assert_eq!(paid, 7);
    }
    w.assert_vault_identity();

    let rent = rent_schedule(&w.rt);
    let mut log = w.take_log();
    for observation in &log {
        let owed = variant_of(&observation.label, &scope);
        table.push(json!({
            "variant": owed,
            "success": observation.success,
            "compute_units": observation.compute_units,
            "packet_bytes": observation.packet_bytes,
            "fits_legacy_packet": observation.fits_legacy_packet,
            "message_keys": observation.message_account_count,
            "writable_accounts": observation.writable_account_count,
            "declared_account_data_bytes": observation.declared_account_data_bytes,
        }));
    }
    let (destination_text, cluster_note) = match kind {
        Destination::RentExempt => (
            "a system wallet holding exactly the rent-exempt minimum of a zero-data account before the claim",
            "the credit leaves the destination rent-exempt: accepted on a cluster too",
        ),
        Destination::Fresh => (
            "a never-funded address (no account, 0 lamports)",
            "LiteSVM 0.7.1 applies the rent-state transition rule only to accounts WITH data, so this credit SUCCEEDS locally; the cluster would refuse it (a data-less account may end a transaction rent-paying only if it was already rent-paying and was not credited), so on a cluster a dust claim to a sub-minimum fresh wallet fails atomically with the runtime's InsufficientFundsForRent and the credit stays claimable. Cluster behaviour is NOT asserted here; see tests/transfer_constraints.rs",
        ),
    };
    let summary = json!({
        "family": FAMILY_DUST,
        "row": row,
        "what": "claim_backer for Backers owed 1..=10 lamports (finalized credit installed with fund_backer_fixture); the headline is the claim owed 10",
        "destination": destination_text,
        "cluster_note": cluster_note,
        "litesvm_accepted_every_claim": accepted_all,
        "rent_exempt_minimum_zero_data": minimum,
        "claims": table,
        "limits": limits_json(),
        "rent_local_default": rent,
        "wall_clock_s_not_evidence": seconds(started),
    });
    let from = format!("{scope}.owed_10");
    promote(&mut log, &from, &scope);
    annotate(&mut log, &scope, note(&summary));
    log
}

/// `rent_exempt_destination` and `fresh_destination_litesvm_accepts`.
pub fn dust_claims() -> Vec<Observation> {
    let mut log = dust_row(ROW_DUST_RENT_EXEMPT, Destination::RentExempt);
    log.extend(dust_row(ROW_DUST_FRESH, Destination::Fresh));
    log
}

// ---------------------------------------------------------------------------
// Family 4: the full close path at M = 100
// ---------------------------------------------------------------------------

/// The `index`-th roster participant key of the full close (never the default
/// key, which the synthetic roster keys would reach at slot 64).
fn m100_key(index: u64) -> Pubkey {
    let mut bytes = [0x6D; 32];
    bytes[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Pubkey::new_from_array(bytes)
}

/// 99 roster participants in a repeating pattern of five: a history-free loser
/// (preferred AND fallback candidate), a loser with Series A history (fallback
/// only), a winner with Series B history, a winner and a break-even; with the
/// closer (a history-free loser) that is 21 preferred and 41 fallback
/// candidates.
fn m100_roster() -> (Vec<Part>, Vec<Pubkey>) {
    let mut parts = Vec::new();
    let mut keys = Vec::new();
    for index in 0..CLOSE_PARTICIPANTS - 1 {
        keys.push(m100_key(index));
        let loss = 10_000_000 * (u128::from(index) + 1);
        parts.push(match index % 5 {
            0 => loser(loss),
            1 => loser_a(loss),
            2 => nonloser_b(),
            3 => nonloser(),
            _ => breakeven(),
        });
    }
    (parts, keys)
}

/// `(preferred, fallback)` candidacy of every participant slot, derived on the
/// host from the sealed `UserRound` / `User` accounts.
fn candidate_flags(w: &World, close_id: u64) -> Vec<(bool, bool)> {
    let close = w.close_record(close_id);
    (0..close.participant_count)
        .map(|slot| {
            let owner = participant(w, close.round_id, slot);
            let round = w.user_round(owner, close.round_id);
            let user = w.user(owner);
            let candidacy =
                pclose::classify(round.deposits, round.net_payout, user.ever_a, user.ever_b);
            (candidacy.preferred, candidacy.fallback)
        })
        .collect()
}

/// Greedy `build_candidates` batches of at most `max_rows` rows that never run
/// a set's append across a candidate-page edge: the instruction carries ONE
/// page per set (the page at `count / 8` when the batch starts) and reports an
/// append that lands on another page as `PageNotProvisioned`.  A row that
/// appends to no set never forces a cut.  Returns the batch sizes.
pub fn plan_build_batches(flags: &[(bool, bool)], max_rows: usize) -> Vec<usize> {
    assert!((1..=8).contains(&max_rows));
    let (mut preferred, mut fallback) = (0u64, 0u64);
    let mut sizes = Vec::new();
    let mut at = 0usize;
    while at < flags.len() {
        let (start_preferred, start_fallback) = (preferred / PAGE, fallback / PAGE);
        let mut take = 0usize;
        while take < max_rows && at + take < flags.len() {
            let (is_preferred, is_fallback) = flags[at + take];
            if (is_fallback && fallback / PAGE != start_fallback)
                || (is_preferred && preferred / PAGE != start_preferred)
            {
                break;
            }
            preferred += u64::from(is_preferred);
            fallback += u64::from(is_fallback);
            take += 1;
        }
        assert!(take >= 1, "one row always fits");
        sizes.push(take);
        at += take;
    }
    sizes
}

/// What the keeper count needs from the full close path.
#[derive(Clone, Debug)]
pub struct FullCloseFacts {
    pub participants: u64,
    pub preferred: usize,
    pub fallback: usize,
    pub closing_payout_tx: usize,
    pub provision_tx: usize,
    pub build_tx: usize,
    pub request_tx: usize,
    pub fulfill_tx: usize,
    pub observe_tx: usize,
    pub capture_tx: usize,
    pub apply_tx: usize,
    pub finalize_tx: usize,
    pub cleanup_tx: usize,
    pub tx_total: usize,
    pub build_batch_sizes: Vec<usize>,
    /// `build_candidates` batches a page-edge-aware plan needs at 8 / 4 / 1 rows.
    pub planned_build_batches: BTreeMap<usize, usize>,
    pub build_rows_within_limits: bool,
    /// The rent schedule the close was priced at (local default).
    pub rent_local_default: Value,
    /// The finalize transaction (a copy of it carries the keeper-count note).
    pub carrier: Observation,
}

/// The `m100` row: the whole life of a close at M = 100 participants, from the
/// closing payout to the cleanup of every page.
///
/// Fixture: [`Spec::standard`] with 99 roster participants ([`m100_roster`])
/// and no Backers, so the closing toll goes straight to the seed (no closing
/// ticket); the closer holds the one Simple position that closes the round.
/// Sequence: closing payout (real `withdraw_simple`), `provision_page(
/// LockedClose)` for exactly the pages the host-derived candidate lists need,
/// `build_candidates` in page-edge-aware batches of up to 8 rows
/// ([`plan_build_batches`]), `request_round_draw`, the provider's `fulfill`
/// (an oracle transaction, not a keeper one), `observe_request_fulfilled`,
/// `capture_round_draw`, `apply_promotion`, `finalize_round`, then
/// `close_shared_account` for every candidate and participant page.
pub fn full_close_m100() -> (Vec<Observation>, FullCloseFacts) {
    let row = ROW_FULL_CLOSE;
    let scope = label(FAMILY_FULL_CLOSE, row);
    let started = Instant::now();
    let (roster, keys) = m100_roster();
    let spec = Spec {
        roster,
        keys,
        ..Spec::standard()
    };
    let scene = Scene::new("measure_m100", "", spec);
    let old_root = "financial.close.measure_m100";
    let Scene {
        w, dest, reserve, ..
    } = scene;
    let mut w = w.scoped_label(scope.clone());

    // Reserve and its components.
    let pool = w.pool();
    let config = w.config(pool.active_config_version);
    let rent_sysvar: Rent = w.rt.svm.get_sysvar::<Rent>();
    let schedule = state::ReserveSchedule::capture(&rent_sysvar, &config);
    let participants = w.close_record(1).participant_count;
    assert_eq!(participants, CLOSE_PARTICIPANTS);
    let pages_per_set = participants.div_ceil(PAGE);
    let registry_extension = u64::from(pool.registry_len % PAGE == 0);
    let provider_cap =
        schedule.provider_fee + schedule.provider_fee * u64::from(schedule.headroom_bps) / 10_000;
    let reserve_parts = [
        schedule.per_candidate_page_rent * 2 * pages_per_set,
        schedule.per_registry_page_rent * registry_extension,
        schedule.b_account_rent,
        schedule.pending_request_rent,
        provider_cap,
    ];
    assert_eq!(
        reserve_parts.iter().sum::<u64>(),
        reserve,
        "required_close_reserve == its components"
    );
    assert_eq!(w.close_record(1).required_reserve, reserve);
    let reserve_json = json!({
        "required_close_reserve": reserve,
        "participants": participants,
        "registry_len": pool.registry_len,
        "existing_candidate_pages": [0, 0],
        "candidate_pages_reserved": 2 * pages_per_set,
        "components": {
            "candidate_pages": reserve_parts[0],
            "candidate_page_rent_each": schedule.per_candidate_page_rent,
            "registry_extension_page": reserve_parts[1],
            "winner_b_account": reserve_parts[2],
            "pending_request_rent": reserve_parts[3],
            "provider_fee_cap_with_headroom": reserve_parts[4],
            "provider_fee": schedule.provider_fee,
            "headroom_bps": schedule.headroom_bps,
        },
    });

    // Host-derived candidate lists, pages and batches.
    let flags = candidate_flags(&w, 1);
    let expected = expected_candidates(&w, 1);
    let (preferred, fallback) = (expected.preferred.len(), expected.fallback.len());
    assert_eq!(
        flags
            .iter()
            .filter(|(is_preferred, _)| *is_preferred)
            .count(),
        preferred
    );
    assert!(
        preferred >= 20 && fallback >= 20,
        "both candidate sets hold at least 20 rows"
    );
    let plan = plan_build_batches(&flags, 8);
    assert_eq!(
        plan.iter().sum::<usize>(),
        flags.len(),
        "the plan visits every participant once"
    );
    let planned: BTreeMap<usize, usize> = [8usize, 4, 1]
        .into_iter()
        .map(|rows| (rows, plan_build_batches(&flags, rows).len()))
        .collect();
    let planned_json: Map<String, Value> = planned
        .iter()
        .map(|(rows, batches)| (rows.to_string(), json!(batches)))
        .collect();

    // 1. Candidate pages from the locked reserve.
    let (preferred_pages, fallback_pages) = provision_candidate_pages(&mut w, 1);
    assert_eq!(
        (preferred_pages, fallback_pages),
        (preferred.div_ceil(8) as u64, fallback.div_ceil(8) as u64)
    );

    // 2. build_candidates, page-edge aware.
    for (call, rows) in plan.iter().enumerate() {
        let instruction = build_next(&w, 1, *rows as u8);
        ok(&mut w, &format!("build{call:02}"), vec![instruction], &[]);
    }
    let sealed = w.close_record(1);
    assert!(sealed.sealed, "the last batch sealed the sets");
    assert_eq!(sealed.candidate_cursor, sealed.participant_count);
    assert_eq!(
        (sealed.preferred_count, sealed.fallback_count),
        (preferred as u64, fallback as u64)
    );

    // 3. The bound ORAO draw and the promotion.
    do_request(&mut w, 1, "request");
    w.fulfill("fulfill", 1)
        .unwrap_or_else(|failed| panic!("{scope}: fulfill failed: {failed:?}"));
    assert_identities(&w);
    let observe = observe_ix(&w, 1);
    ok(&mut w, "observe", vec![observe], &[]);
    let capture = capture_ix(&w, 1);
    ok(&mut w, "capture", vec![capture], &[]);
    let apply = apply_ix(&w, 1);
    ok(&mut w, "apply", vec![apply], &[]);

    // Rent held by the close's accounts before they are returned.
    let candidate_keys: Vec<(u8, u64, Pubkey)> = [(0u8, preferred_pages), (1u8, fallback_pages)]
        .into_iter()
        .flat_map(|(tag, pages)| {
            (0..pages).map(move |index| (tag, index, candidate_page_key(1, tag, index)))
        })
        .collect();
    let participant_pages = participants.div_ceil(PAGE);
    let participant_keys: Vec<(u64, Pubkey)> = (0..participant_pages)
        .map(|index| (index, w.rt.participant_address(1, index)))
        .collect();
    let candidate_lamports: u64 = candidate_keys
        .iter()
        .map(|(_, _, key)| w.lamports(*key))
        .sum();
    let participant_lamports: u64 = participant_keys
        .iter()
        .map(|(_, key)| w.lamports(*key))
        .sum();
    let request_account = w.close_record(1).request_account;
    let request_lamports = w.lamports(request_account);
    let spent = w.close_record(1).spent;

    let finalize = finalize_ix(&w, 1);
    let finalize_tx = ok(&mut w, "finalize", vec![finalize], &[]);
    assert!(w.close_record(1).finalized);
    assert!(
        w.close_record(1).reserve_settled,
        "the unspent reserve was refunded at the terminal promotion"
    );
    assert_eq!(
        w.close_record(1).locked_sponsor_remaining + w.close_record(1).locked_owner_remaining,
        0
    );

    // 4. Cleanup: close every candidate and participant page.
    let ops_before = w.ops();
    let cash_before = w.lamports(w.rt.ops_cash_address());
    let dest_before = w.lamports(dest);
    let mut returned_to_cash = 0i128;
    let mut returned_to_dest = 0i128;
    for (tag, index, key) in &candidate_keys {
        let kind = PageKind::Candidate {
            close_id: 1,
            set_tag: *tag,
        };
        let instruction = close_page_ix(&w, kind, *key, dest, 1);
        let name = format!(
            "close_candidate_{}{index}",
            if *tag == 0 { "preferred" } else { "fallback" }
        );
        let observation = ok(&mut w, &name, vec![instruction], &[]);
        returned_to_cash += delta(&observation, w.rt.ops_cash_address());
        returned_to_dest += delta(&observation, dest);
    }
    for (index, key) in &participant_keys {
        let kind = PageKind::Participant { round: 1 };
        let instruction = close_page_ix(&w, kind, *key, dest, 1);
        let observation = ok(
            &mut w,
            &format!("close_participant{index:02}"),
            vec![instruction],
            &[],
        );
        returned_to_cash += delta(&observation, w.rt.ops_cash_address());
        returned_to_dest += delta(&observation, dest);
    }
    let ops_after = w.ops();
    assert_eq!(
        i128::from(w.lamports(w.rt.ops_cash_address())) - i128::from(cash_before),
        returned_to_cash
    );
    assert_eq!(
        i128::from(w.lamports(dest)) - i128::from(dest_before),
        returned_to_dest
    );
    for (_, key) in &participant_keys {
        assert!(w.account(*key).is_none(), "participant page closed");
    }
    for (_, _, key) in &candidate_keys {
        assert!(w.account(*key).is_none(), "candidate page closed");
    }

    // Tally.
    let rent = rent_schedule(&w.rt);
    let mut log = w.take_log();
    for observation in &mut log {
        if let Some(rest) = observation.label.strip_prefix(old_root) {
            observation.label = format!("{scope}{rest}");
        }
    }
    let count = |prefix: &str| -> usize {
        log.iter()
            .filter(|observation| variant_of(&observation.label, &scope).starts_with(prefix))
            .count()
    };
    let facts_counts = (
        count("closing_payout"),
        count("provision_"),
        count("build"),
        count("request"),
        count("fulfill"),
        count("observe"),
        count("capture"),
        count("apply"),
        count("finalize"),
        count("close_"),
    );
    let tx_total = log.len();
    assert_eq!(
        facts_counts.0
            + facts_counts.1
            + facts_counts.2
            + facts_counts.3
            + facts_counts.4
            + facts_counts.5
            + facts_counts.6
            + facts_counts.7
            + facts_counts.8
            + facts_counts.9,
        tx_total,
        "every transaction of the path is classified"
    );
    assert_eq!(facts_counts.2, plan.len());
    let build_within_limits = log
        .iter()
        .filter(|observation| variant_of(&observation.label, &scope).starts_with("build"))
        .all(within_limits);

    // Per-instruction table (CU / packet / keys / writable / declared bytes).
    let per_instruction: Vec<Value> = log.iter().map(observation_json).collect();
    let mut kinds: BTreeMap<String, PhaseStats> = BTreeMap::new();
    for observation in &log {
        let variant = variant_of(&observation.label, &scope);
        let stem = variant.trim_end_matches(|c: char| c.is_ascii_digit());
        let kind = match stem
            .strip_suffix("_preferred")
            .or_else(|| stem.strip_suffix("_fallback"))
        {
            Some(head) => format!("{head}_set"),
            None => stem.to_string(),
        };
        kinds
            .entry(kind)
            .or_insert_with(PhaseStats::new)
            .add(observation);
    }
    let by_kind: Map<String, Value> = kinds
        .iter()
        .map(|(k, v)| (k.clone(), v.to_json()))
        .collect();
    let held = candidate_lamports + participant_lamports + request_lamports;
    let summary = json!({
        "family": FAMILY_FULL_CLOSE,
        "row": row,
        "what": "the whole life of one close at M = 100 participants: closing payout, LockedClose provisioning, build_candidates at 8 rows aligned to candidate-page edges, request, provider fulfill, observe, capture, apply, finalize, then close_shared_account for every page; the headline is the finalize transaction",
        "fixture": {
            "participants": participants,
            "roster_pattern": "99 roster participants repeating [history-free loser, loser with Series A history, winner with Series B history, winner, break-even] + the closer (a history-free loser)",
            "preferred_candidates": preferred,
            "fallback_candidates": fallback,
            "candidate_pages": [preferred_pages, fallback_pages],
            "participant_pages": participant_pages,
            "backers_in_registry": 0,
            "closing_ticket": false,
            "build_batch_sizes": plan,
            "build_batch_rule": "greedy, at most 8 rows, cut BEFORE a row that would append to a set whose count has reached another page than the one the batch started on",
        },
        "reserve": reserve_json,
        "result": {
            "tx_total": tx_total,
            "tx_by_kind": {
                "closing_payout": facts_counts.0,
                "provision_candidate_pages": facts_counts.1,
                "build_candidates": facts_counts.2,
                "request_round_draw": facts_counts.3,
                "fulfill_oracle": facts_counts.4,
                "observe_request_fulfilled": facts_counts.5,
                "capture_round_draw": facts_counts.6,
                "apply_promotion": facts_counts.7,
                "finalize_round": facts_counts.8,
                "close_shared_account": facts_counts.9,
            },
            "keeper_signed_core": facts_counts.2 + facts_counts.3 + facts_counts.6 + facts_counts.7 + facts_counts.8,
            "planned_build_batches_by_max_rows": planned_json,
            "build_batches_within_limits": build_within_limits,
            "every_transaction_within_all_limits": log.iter().all(within_limits),
        },
        "rent_flow_local_default": {
            "locked_at_the_closing_payout": reserve,
            "spent_ledger": {
                "candidate_rent_sponsor": spent.candidate_rent_s,
                "candidate_rent_owner": spent.candidate_rent_o,
                "registry_rent_sponsor": spent.registry_rent_s,
                "b_rent_sponsor": spent.b_rent_s,
                "provider_fee_sponsor": spent.provider_fee_s,
                "request_rent_sponsor": spent.request_rent_s,
            },
            "held_by_close_accounts_before_cleanup": {
                "candidate_pages": candidate_lamports,
                "participant_pages": participant_lamports,
                "pending_request_account": request_lamports,
                "total": held,
            },
            "returned_by_close_shared_account": {
                "to_ops_cash": returned_to_cash as i64,
                "to_owner_refund_destination": returned_to_dest as i64,
                "ops_free_balance_delta": ops_after.free_balance as i64 - ops_before.free_balance as i64,
                "ops_unallocated_excess_delta": ops_after.unallocated_excess as i64 - ops_before.unallocated_excess as i64,
            },
            "note": "participant pages are fixtures here (installed with default rent components), so their rent returns as unallocated excess, not as free operations cash; the provider fee is consumed to the ORAO treasury and the pending-request rent stays in the (fulfilled) request account",
        },
        "by_kind": by_kind,
        "per_instruction": per_instruction,
        "limits": limits_json(),
        "rent_local_default": rent,
        "wall_clock_s_not_evidence": seconds(started),
    });
    let facts = FullCloseFacts {
        participants,
        preferred,
        fallback,
        closing_payout_tx: facts_counts.0,
        provision_tx: facts_counts.1,
        build_tx: facts_counts.2,
        request_tx: facts_counts.3,
        fulfill_tx: facts_counts.4,
        observe_tx: facts_counts.5,
        capture_tx: facts_counts.6,
        apply_tx: facts_counts.7,
        finalize_tx: facts_counts.8,
        cleanup_tx: facts_counts.9,
        tx_total,
        build_batch_sizes: plan,
        planned_build_batches: planned,
        build_rows_within_limits: build_within_limits,
        rent_local_default: rent,
        carrier: finalize_tx,
    };
    let finalize_label = format!("{scope}.finalize");
    promote(&mut log, &finalize_label, &scope);
    annotate(&mut log, &scope, note(&summary));
    (log, facts)
}

// ---------------------------------------------------------------------------
// Family 5: the inline envelope
// ---------------------------------------------------------------------------

/// Measured per-row phase costs the inline estimate is built from.
#[derive(Clone, Debug)]
pub struct PerRowCosts {
    /// Rows-1 transactions of the 64-row population: `cu_max` / `cu_mean` of
    /// one Discover / Allocate / Prune instruction carrying ONE row.
    pub discover_max: u64,
    pub discover_mean: u64,
    pub allocate_max: u64,
    pub allocate_mean: u64,
    pub prune_max: u64,
    pub prune_mean: u64,
    /// The three row-less phases.
    pub senior: u64,
    pub seal: u64,
    pub commit: u64,
    /// `(cu_max at rows 8 - cu_max at rows 1) / 7` for Discover / Allocate /
    /// Prune, when a rows-8 run was supplied.
    pub marginal: Option<[u64; 3]>,
}

impl PerRowCosts {
    pub fn from_runs(rows1: &ChunkRun, rows8: Option<&ChunkRun>) -> Self {
        assert_eq!(rows1.rows, 1, "per-row costs come from a rows-1 run");
        let (discover, allocate, prune) = (
            rows1.phase("discover"),
            rows1.phase("allocate"),
            rows1.phase("prune"),
        );
        let marginal = rows8.map(|run| {
            assert_eq!(run.rows, 8);
            let step = |phase: &str, one: &PhaseStats| {
                run.phase(phase).cu_max.saturating_sub(one.cu_max) / 7
            };
            [
                step("discover", discover),
                step("allocate", allocate),
                step("prune", prune),
            ]
        });
        Self {
            discover_max: discover.cu_max,
            discover_mean: discover.cu_mean(),
            allocate_max: allocate.cu_max,
            allocate_mean: allocate.cu_mean(),
            prune_max: prune.cu_max,
            prune_mean: prune.cu_mean(),
            senior: rows1.phase("senior").cu_max,
            seal: rows1.phase("seal").cu_max,
            commit: rows1.phase("commit").cu_max,
            marginal,
        }
    }

    fn fixed(&self) -> u64 {
        self.senior + self.seal + self.commit
    }

    /// Payout CU plus `n` rows at the rows-1 MAXIMUM of each row phase plus the
    /// row-less phases (the brief's formula; an upper bound, because every
    /// rows-1 transaction also pays the per-transaction fixed cost).
    pub fn estimate_conservative(&self, payout_cu: u64, n: u64) -> u64 {
        if n == 0 {
            return payout_cu;
        }
        payout_cu + n * (self.discover_max + self.allocate_max + self.prune_max) + self.fixed()
    }

    /// The same with the rows-1 MEAN of each row phase (a typical Prune row is
    /// a skipped one).
    pub fn estimate_mean(&self, payout_cu: u64, n: u64) -> u64 {
        if n == 0 {
            return payout_cu;
        }
        payout_cu + n * (self.discover_mean + self.allocate_mean + self.prune_mean) + self.fixed()
    }

    /// Fixed part once plus the marginal row cost `n` times (needs a rows-8 run).
    pub fn estimate_marginal(&self, payout_cu: u64, n: u64) -> Option<u64> {
        let marginal = self.marginal?;
        if n == 0 {
            return Some(payout_cu);
        }
        let per_row = marginal[0] + marginal[1] + marginal[2];
        let one_row = self.discover_max + self.allocate_max + self.prune_max;
        let batch_fixed = one_row.saturating_sub(per_row);
        Some(payout_cu + batch_fixed + n * per_row + self.fixed())
    }
}

/// `inline_envelope.n<N>`: a real non-closing solvent `withdraw_simple` (1 SOL
/// Simple position, one day, a 13,200,000 toll) over a registry of N fixture
/// Backers, with the N Backer PDAs and their registry and snapshot page PDAs
/// appended as extra remaining accounts (all writable, as an inline allocation
/// would write them).  The handler ignores them, so the transaction succeeds;
/// its packet, keys, locks, declared bytes and compute are what an inline
/// allocation would at least have to carry.  The headline is that payout.
///
/// THIS IS AN ENVELOPE, NOT AN INLINE IMPLEMENTATION: for N > 0 the payout
/// still starts the chunked ticket, so `payout_cu` includes a `ticket_start`
/// an inline path would not run; the allocation compute is estimated from
/// `costs` (see [`PerRowCosts`]).
pub fn inline_envelope(costs: &PerRowCosts) -> Vec<Observation> {
    let mut log: Vec<Observation> = Vec::new();
    let mut rows: Vec<(u64, Value)> = Vec::new();
    let mut base_payout_cu: u64 = 0;
    for &n in &ENVELOPE_NS {
        let scope = label(FAMILY_ENVELOPE, &envelope_row(n));
        let started = Instant::now();
        let mut w = World::genesis_with_simple_position(SOL).scoped_label(scope.clone());
        let specs: Vec<BackerSpec> = (0..n)
            .map(|index| {
                let series = if index % 2 == 0 { SERIES_A } else { SERIES_B };
                BackerSpec::new(allocation_owner(index), series, 1_000_000_000)
            })
            .collect();
        w.install_backer_specs(&specs);
        w.warp_seconds(DAY);
        let owner = w.owner();
        let destination = w.dest;
        let mut instruction = w.payout_instruction(
            PayoutKind::WithdrawSimple,
            owner.pubkey(),
            owner.pubkey(),
            1,
            0,
            0,
            destination,
            CloseRecordArg::Auto,
        );
        // IMPL-39b: the program's inline layout is Backers in registry order,
        // then RegistryPage 0 and SnapshotPage 0 (one page each: inline is only
        // offered while the registry fits one page).  With N <= INLINE_THRESHOLD
        // the payout now RUNS the whole ticket inline; above it the program
        // refuses (InvalidState) and the row records the rejected transaction's
        // packet envelope.
        let pages = n.div_ceil(PAGE);
        for spec in &specs {
            instruction.accounts.push(AccountMeta::new(
                w.rt.backer_address(spec.series, spec.owner),
                false,
            ));
        }
        for page in 0..pages {
            instruction
                .accounts
                .push(AccountMeta::new(w.rt.registry_address(page), false));
            instruction
                .accounts
                .push(AccountMeta::new(w.rt.snapshot_address(page), false));
        }
        let base_accounts = instruction.accounts.len() - (2 * pages + n) as usize;
        let inline_offered = n > 0 && n <= state::INLINE_THRESHOLD;
        let outcome = w.send("", vec![instruction], &[&owner]);
        let (observation, executed_inline, rejection) = match outcome {
            Ok(observation) => {
                let payout = observation.last_event::<musical_chairs_program::PayoutPaid>();
                assert!(!payout.closing, "{scope}: a solvent, non-closing payout");
                assert_eq!(payout.toll, 13_200_000, "{scope}: the one-day Simple toll");
                if inline_offered {
                    assert_ne!(payout.ticket_id, 0, "{scope}: PayoutPaid carries the committed ticket id");
                    let committed = observation.last_event::<musical_chairs_program::TicketCommitted>();
                    assert_eq!(committed.ticket_id, payout.ticket_id, "{scope}: TicketCommitted for the same ticket");
                    assert_eq!(w.ticket().phase, state::IDLE, "{scope}: the ticket is IDLE after the inline payout");
                } else {
                    assert_eq!(n, 0, "{scope}: only the empty registry succeeds without an inline run");
                }
                (observation, inline_offered, Value::Null)
            }
            Err(observation) => {
                assert!(n > state::INLINE_THRESHOLD, "{scope}: only a registry past one page is refused: {observation:?}");
                let code = observation.error.clone().unwrap_or_default();
                assert!(code.contains("Custom(6001)"), "{scope}: InvalidState for registry_len > INLINE_THRESHOLD, got {code}");
                observation.assert_rollback_except_fee();
                (observation, false, json!("InvalidState 6001: registry_len > INLINE_THRESHOLD (inline is not offered past one page); fee-only"))
            }
        };
        let cu = observation.compute_units.unwrap_or(0);
        if n == 0 {
            base_payout_cu = cu;
        }
        let payout_cu = base_payout_cu;
        let conservative = costs.estimate_conservative(payout_cu, n);
        let mean = costs.estimate_mean(payout_cu, n);
        let marginal = costs.estimate_marginal(payout_cu, n);
        let fits_packet = observation.fits_legacy_packet;
        let under_64_locks = observation.message_account_count <= MAX_ACCOUNT_LOCKS;
        let effective_cu = if executed_inline || n == 0 { cu } else { conservative };
        let under_200k = effective_cu <= 200_000;
        let under_1_4m = effective_cu <= 1_400_000;
        let margin_pct = 100i64 - (effective_cu * 100 / 1_400_000) as i64;
        let row = json!({
            "n": n,
            "executed_inline": executed_inline,
            "rejection": rejection,
            "payout_cu_base_n0": payout_cu,
            "measured_cu": cu,
            "measured_inline_cu": if executed_inline { json!(cu) } else { Value::Null },
            "packet_bytes": observation.packet_bytes,
            "message_keys": observation.message_account_count,
            "writable_accounts": observation.writable_account_count,
            "declared_account_data_bytes": observation.declared_account_data_bytes,
            "fits_packet": fits_packet,
            "under_64_locks": under_64_locks,
            "estimated_inline_cu": conservative,
            "effective_cu_for_fits_all": effective_cu,
            "under_200k_cu": under_200k,
            "under_1_4m_cu": under_1_4m,
            "cu_headroom_pct_of_1_4m": margin_pct,
            "fits_all": fits_packet && under_64_locks && under_1_4m && margin_pct >= 25,
        });
        let summary = json!({
            "family": FAMILY_ENVELOPE,
            "row": envelope_row(n),
            "kind": if executed_inline { "REAL INLINE ALLOCATION (IMPL-39b): a withdraw_simple that supplies the registry's Backers and pages runs Discover..Commit in-instruction; measured_inline_cu is the whole transaction" } else if n == 0 { "plain payout on an empty registry (no ticket)" } else { "ENVELOPE ONLY: registry_len > INLINE_THRESHOLD, the program refuses inline (InvalidState); packet/keys are those of the rejected transaction, the compute figure is an estimate from per-row phase costs" },
            "fixture": {
                "registry_backers": n,
                "backer_series": "alternating A, B (A first), 1,000,000,000 remaining each",
                "payout": "1 SOL Simple position, one day, withdraw_simple, non-closing and solvent, toll 13,200,000",
                "extra_remaining_accounts": { "layout": "Backers in registry order, then RegistryPage k, SnapshotPage k", "backer_pdas_writable": n, "snapshot_pages_writable": pages, "registry_pages_writable": pages, "total": 2 * pages + n },
                "base_accounts_in_the_instruction": base_accounts,
                "close_record_account": "placeholder (a non-closing payout does not need it)",
            },
            "measured": row,
            "estimate": {
                "formula": "estimated_inline_cu = payout_cu + N x (discover + allocate + prune CU of ONE row, each the maximum over the rows-1 transactions of the 64-row population) + senior + seal + commit",
                "estimated_inline_cu": conservative,
                "per_row_cu_rows1_max": { "discover": costs.discover_max, "allocate": costs.allocate_max, "prune": costs.prune_max },
                "fixed_cu": { "senior": costs.senior, "seal": costs.seal, "commit": costs.commit },
                "payout_cu_base": "the N = 0 payout (no ticket) measured in this family; the estimate adds per-row phase costs to it",
                "mean_basis": {
                    "estimated_inline_cu": mean,
                    "per_row_cu_rows1_mean": { "discover": costs.discover_mean, "allocate": costs.allocate_mean, "prune": costs.prune_mean },
                    "under_200k_cu": mean <= 200_000,
                    "under_1_4m_cu": mean <= 1_400_000,
                },
                "marginal_basis": marginal.map(|estimate| json!({
                    "estimated_inline_cu": estimate,
                    "formula": "payout_cu + batch_fixed + N x marginal_row + senior + seal + commit, marginal_row = (rows-8 cu_max - rows-1 cu_max) / 7 per phase, batch_fixed = one-row cost - marginal_row",
                    "marginal_row_cu": costs.marginal.map(|m| json!({ "discover": m[0], "allocate": m[1], "prune": m[2] })),
                    "under_200k_cu": estimate <= 200_000,
                    "under_1_4m_cu": estimate <= 1_400_000,
                })),
                "fits_all_rule": "fits_packet && under_64_locks && estimated_inline_cu <= 1,400,000 with at least 25% headroom",
            },
            "limits": limits_json(),
            "limits_note": "LiteSVM executed (or rejected) this transaction although a packet over 1,232 bytes or more than 64 locks could not be sent to a cluster; fits_* are the harness's verdicts; fits_all uses the MEASURED compute where the inline ran and the conservative estimate otherwise",
            "rent_local_default": rent_schedule(&w.rt),
            "wall_clock_s_not_evidence": seconds(started),
        });
        let mut one = w.take_log();
        assert_eq!(
            one.len(),
            1,
            "{scope}: exactly the envelope payout is recorded"
        );
        one[0].unsupported = Some(note(&summary));
        log.extend(one);
        rows.push((n, row));
    }

    // The summary over all N, attached to every headline.
    let feasible: Vec<u64> = rows
        .iter()
        .filter(|(_, row)| row["fits_all"] == json!(true))
        .map(|(n, _)| *n)
        .collect();
    let largest = feasible.iter().copied().max();
    let table: Vec<Value> = rows.iter().map(|(_, row)| row.clone()).collect();
    for observation in &mut log {
        let mut parsed: Value =
            serde_json::from_str(observation.unsupported.as_deref().unwrap_or("{}"))
                .expect("the envelope note is JSON");
        parsed["summary_over_all_n"] = json!({
            "feasible_n_under_the_fits_all_rule": feasible,
            "largest_feasible_n": largest,
            "note": "a failure at one N never disables a smaller feasible N (DR-31)",
            "rows": table,
        });
        observation.unsupported = Some(note(&parsed));
    }
    log
}

/// The envelope with its per-row costs measured here (one 64-row ticket at 1
/// row per instruction and one at 8).
pub fn run_inline_envelope() -> Vec<Observation> {
    let rows1 = chunk_run_for(64, 1);
    let rows8 = chunk_run_for(64, 8);
    inline_envelope(&PerRowCosts::from_runs(&rows1, Some(&rows8)))
}

// ---------------------------------------------------------------------------
// Family 6: the keeper count
// ---------------------------------------------------------------------------

/// `keeper_count.pilot`: the monthly keeper transaction count for P = 1,000
/// payouts, C = 10 closes, N = M = 100 from MEASURED per-phase transaction
/// counts at the largest batch size every phase of the N = 100 ticket fits
/// (packet, 64 locks, compute), against the planning formula of
/// `docs/coordination/native-solana-icp-budget.md`:
///
/// ```text
/// K_scenario = P x (3 x ceil(N/b) + 3) + C x (ceil(M/b) + 4)
/// K_measured = P x tickets_tx(N, b) + C x close_tx(M, b)
/// ```
///
/// `tickets_tx` is the number of transactions of one N = 100 ticket (Discover +
/// Senior + Seal + Allocate + Prune + Commit, measured, so a chained-move Prune
/// batch counts); `close_tx` is the keeper-signed core of the M = 100 close
/// (`build_candidates` batches + request + capture + apply + finalize).  The
/// closing payout is a user transaction (inside P), and the provider's
/// `fulfill` is the oracle's.  Page provisioning, `observe_request_fulfilled`
/// and cleanup are optional or separate allowances and are reported as extra
/// terms, never folded into the core.
///
/// The headline carries a COPY of the full-close `finalize` observation (this
/// row is arithmetic over measured counts, not a transaction of its own).
pub fn keeper_count(chunk: &[ChunkRun], close: &FullCloseFacts) -> Observation {
    let n100: Vec<&ChunkRun> = chunk.iter().filter(|run| run.n == PILOT_BACKERS).collect();
    assert!(
        !n100.is_empty(),
        "the keeper count needs the N = 100 ticket runs"
    );
    let chosen = [8u8, 4, 1].into_iter().find_map(|rows| {
        n100.iter()
            .find(|run| run.rows == rows && run.fits_all)
            .copied()
    });
    let (run, fits) = match chosen {
        Some(run) => (run, true),
        None => (
            *n100.iter().min_by_key(|run| run.rows).expect("a run"),
            false,
        ),
    };
    let b = u64::from(run.rows);
    let (p, c, n, m) = (
        PILOT_PAYOUTS,
        PILOT_CLOSES,
        PILOT_BACKERS,
        PILOT_PARTICIPANTS,
    );
    let ticket_tx = run.tx_total as u64;
    let ticket_by_phase: Map<String, Value> = PHASES
        .iter()
        .map(|phase| {
            (
                (*phase).to_string(),
                json!(run.phases.get(*phase).map_or(0, |s| s.tx)),
            )
        })
        .collect();
    let build_batches = *close
        .planned_build_batches
        .get(&(b as usize))
        .expect("build batches are planned for 8, 4 and 1 rows");
    let build_executed = if b == 8 { Some(close.build_tx) } else { None };
    if let Some(executed) = build_executed {
        assert_eq!(
            executed, build_batches,
            "the executed 8-row plan is the planned one"
        );
    }
    let close_core = build_batches as u64 + 4;
    let scenario_ticket = 3 * n.div_ceil(b) + 3;
    let scenario_close = m.div_ceil(b) + 4;
    let k_scenario = p * scenario_ticket + c * scenario_close;
    let k_core = p * ticket_tx + c * close_core;
    let provisioning = close.provision_tx as u64;
    let k_with_provisioning = k_core + c * provisioning;
    let optional = close.observe_tx as u64 + close.cleanup_tx as u64;
    let k_with_everything = k_with_provisioning + c * optional;
    // The same arithmetic at every batch size the N = 100 population was run at.
    let mut by_rows: Vec<&ChunkRun> = n100.clone();
    by_rows.sort_by_key(|candidate| candidate.rows);
    let k_by_rows: Vec<Value> = by_rows
        .iter()
        .map(|candidate| {
            let rows = u64::from(candidate.rows);
            let batches = close.planned_build_batches.get(&usize::from(candidate.rows)).copied();
            json!({
                "rows": rows,
                "ticket_tx": candidate.tx_total,
                "every_transaction_fits": candidate.fits_all,
                "close_build_batches_planned": batches,
                "k_measured_core": batches.map(|batches| p * candidate.tx_total as u64 + c * (batches as u64 + 4)),
                "k_scenario": p * (3 * n.div_ceil(rows) + 3) + c * (m.div_ceil(rows) + 4),
            })
        })
        .collect();
    let fee = |k: u64| k * SIGNATURE_FEE_LAMPORTS;
    let sol = |k: u64| (fee(k) as f64) / 1e9;
    let cycles = |k: u64| (u128::from(k) * CYCLES_PER_SIGNATURE).to_string();
    let summary = json!({
        "family": FAMILY_KEEPER,
        "row": ROW_KEEPER,
        "carrier_observation": format!("a COPY of {}; this row is arithmetic over measured transaction counts, not an additional transaction", close.carrier.label),
        "pilot": { "P_payouts": p, "C_closes": c, "N_backers": n, "M_participants": m, "keeper_signatures_per_transaction": 1 },
        "formulas": {
            "scenario": "K_scenario = P x (3 x ceil(N/b) + 3) + C x (ceil(M/b) + 4)",
            "measured": "K_measured = P x ticket_tx(N = 100, b) + C x close_core_tx(M = 100, b); close_core_tx = build_candidates batches + request + capture + apply + finalize",
            "extras": "K_with_provisioning = K_measured + C x provision_tx; K_with_everything = K_with_provisioning + C x (observe + close_shared_account transactions)",
        },
        "chosen_rows_b": b,
        "chosen_rows_rule": "the largest of 8 / 4 / 1 for which every transaction of the N = 100 ticket fits the legacy packet, 64 locks and the 1.4 M compute limit",
        "every_transaction_fits_at_b": fits,
        "ticket_n100": {
            "rows": b,
            "tx_total": ticket_tx,
            "tx_by_phase": ticket_by_phase,
            "scenario_tx": scenario_ticket,
            "source": run.scope,
        },
        "close_m100": {
            "build_batches": build_batches,
            "build_batches_source": if build_executed.is_some() { "executed (page-edge-aware 8-row plan)" } else { "planned on the host (not executed) with the same page-edge rule at this b" },
            "keeper_core_tx": close_core,
            "scenario_tx": scenario_close,
            "provision_tx": provisioning,
            "observe_tx": close.observe_tx,
            "cleanup_close_shared_account_tx": close.cleanup_tx,
            "closing_payout_tx_user_paid": close.closing_payout_tx,
            "fulfill_tx_oracle_paid": close.fulfill_tx,
            "executed_tx_total": close.tx_total,
            "participants": close.participants,
            "preferred_candidates": close.preferred,
            "fallback_candidates": close.fallback,
            "planned_build_batches_by_max_rows": close.planned_build_batches.iter().map(|(rows, batches)| (rows.to_string(), json!(batches))).collect::<Map<String, Value>>(),
            "build_batches_within_limits_at_8": close.build_rows_within_limits,
        },
        "k_monthly": {
            "scenario": k_scenario,
            "measured_core": k_core,
            "measured_with_provisioning": k_with_provisioning,
            "measured_with_provisioning_observe_and_cleanup": k_with_everything,
            "measured_core_minus_scenario": i64::try_from(k_core).unwrap_or(i64::MAX) - i64::try_from(k_scenario).unwrap_or(i64::MAX),
        },
        "k_by_rows": k_by_rows,
        "base_network_fee": {
            "assumption": "5,000 lamports per signature, one signature per keeper transaction (the documented planning assumption, not a quote)",
            "scenario_sol": sol(k_scenario),
            "measured_core_sol": sol(k_core),
            "measured_with_provisioning_sol": sol(k_with_provisioning),
            "measured_with_everything_sol": sol(k_with_everything),
        },
        "icp_signing_cycles": {
            "cycles_per_signature": CYCLES_PER_SIGNATURE.to_string(),
            "measured_core": cycles(k_core),
            "measured_with_provisioning": cycles(k_with_provisioning),
            "scenario": cycles(k_scenario),
        },
        "not_counted": "fee retries, standby races, ordinal exhaustion of the draw, zero-toll / inline shortcuts that reduce K, and any batch smaller than b forced by a chained Prune move beyond what the measured ticket shows",
        "rent_local_default": close.rent_local_default.clone(),
        "rent_note": "every count here is rent-independent; the schedule is the one the M = 100 close was priced at",
    });
    let mut carrier = close.carrier.clone();
    carrier.label = label(FAMILY_KEEPER, ROW_KEEPER);
    carrier.unsupported = Some(note(&summary));
    carrier
}

/// The keeper count with its inputs measured here (the N = 100 ticket at 8
/// rows, falling back to 4 and 1 only while the larger batch does not fit, and
/// the M = 100 close).
pub fn run_keeper_count() -> Vec<Observation> {
    let mut runs = vec![chunk_run_for(PILOT_BACKERS, 8)];
    for rows in [4u8, 1] {
        if runs.iter().any(|run| run.fits_all) {
            break;
        }
        runs.push(chunk_run_for(PILOT_BACKERS, rows));
    }
    let (_, facts) = full_close_m100();
    vec![keeper_count(&runs, &facts)]
}

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

/// The whole matrix as one observation list: the chunk family, churn, dust
/// claims, the M = 100 close, the inline envelope and the keeper count, in that
/// order.  Every row asserts its own fixtures (see the module docs).
pub fn run_measure_suite() -> Vec<Observation> {
    let chunk = chunk_matrix();
    let rows1 = chunk
        .iter()
        .find(|run| run.n == 64 && run.rows == 1)
        .expect("the 64-row rows-1 run");
    let rows8 = chunk.iter().find(|run| run.n == 64 && run.rows == 8);
    let costs = PerRowCosts::from_runs(rows1, rows8);

    let churn = churn();
    let dust = dust_claims();
    let (close, facts) = full_close_m100();
    let envelope = inline_envelope(&costs);
    let keeper = keeper_count(&chunk, &facts);

    let mut observations: Vec<Observation> =
        chunk.into_iter().flat_map(|run| run.observations).collect();
    observations.extend(churn);
    observations.extend(dust);
    observations.extend(close);
    observations.extend(envelope);
    observations.push(keeper);
    for headline in planned_labels() {
        assert!(
            observations
                .iter()
                .any(|observation| observation.label == headline),
            "the matrix produced no headline observation {headline}"
        );
    }
    assert_outcomes_consistent(&observations);
    assert_unique_labels(&observations);
    observations
}
