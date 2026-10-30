//! M6 step 4: a LiteSVM differential of random allocation tickets against the
//! host `Book`.
//!
//! One [`World`] (the actual compiled SBF in LiteSVM) and ONE host
//! `musical_chairs_economics::backers::Book`, both built from the same
//! [`BackerSpec`] registry, replay the same 50 tickets in sequence.  After every
//! ticket (and every claim, re-funding and credit between tickets) the two are
//! compared field by field; the Book is the oracle.
//!
//! Deterministic seed (a tiny xorshift64* below, no PRNG dependency):
//!
//! * [`SEED`] `0x4D43_4449_4646_3330` ("MCDIFF30") drives the registry size
//!   (20..=40 Backers), every override, the toll plan and every random choice
//!   of the operations between tickets.
//!
//! # Registry
//!
//! `random_backer_specs(SEED, n)` supplies the mix: Series A and B, rows with
//! 11..50 lamports left (just above the 10-lamport cutoff), closed rows (at
//! most 10 left), ordinary, enormous and partly repaid rows.  Rows 0..=3 are
//! fixed by that generator (oldest open A with 100_000, a B holding 30, a
//! closed A holding 5, a closed B holding exactly 10).  On top of it:
//!
//! * the three REAL keypair actors (`owner`, `attacker`, `sponsor`) each hold a
//!   Series A and a Series B row, because only a keypair can sign a
//!   `claim_backer` or a `fund_series_a`: `owner` takes rows 0 (A) and 1 (B),
//!   `attacker` rows 5 / 6, `sponsor` rows 7 / 8 (the latter A is partly
//!   repaid).  `attacker` and `sponsor` get the `User` account `claim_backer`
//!   needs (genesis only creates the owner's);
//! * rows 11 (A) and 12 (B) hold 10^11..2*10^11 lamports: no toll of this
//!   sequence caps them, so the registry never empties and both series stay
//!   open for the whole run.
//!
//! # The 50 tickets
//!
//! * Toll plan: ticket 1 is fixed at 7_654_321 (its senior slice of 1_339_506
//!   exceeds the owner's 100_000 entitlement, so the owner's A is capped and
//!   pruned at once, which makes the later re-funding and claims meaningful);
//!   every ninth ticket is tiny (1..=20); the rest are log-uniform over
//!   `[1, 10^9]` (a draw of the exponent, then a draw inside the decade), so
//!   many tickets are partial rather than capping every ordinary row.
//! * Batch size: `rows` cycles 1 / 4 / 8 ([`Rows::Cycling`]).
//! * After ticket n: every 7th a `claim_backer` for a drawn actor (the first
//!   one is the owner, who has finalized credit by then); every 11th a REAL
//!   `fund_series_a` re-funding of an actor's capped A; every 13th a
//!   `fund_backer_fixture` credit of finalized claims to a drawn open Backer.
//!
//! # Book <-> program mapping
//!
//! | program (`state::Backer` / `Pool`)       | host `Book`                          |
//! |------------------------------------------|--------------------------------------|
//! | `pending + funded` (claimable)           | `pending + funded`                   |
//! | `pending`, `funded` (each, exactly)      | `pending`, `funded`                  |
//! | `lifetime`, `entitlement`                | `lifetime_credited`, `entitlement`   |
//! | `active`                                 | `active_slot.is_some()`              |
//! | `first_sequence - base`                  | `first_funding_sequence`             |
//! | `Pool.seed - seed at genesis`            | `seed()`                             |
//! | `Pool.funded_claims - funded at genesis` | `global_funded_claims()`             |
//! | `Pool.registry_len`                      | number of Backers with a live slot   |
//! | `Pool.pending_toll`                      | `pending_toll`                       |
//! | vault identity (`assert_vault_identity`) | `conservation()`                     |
//!
//! `base` is `Pool.next_sequence` before the first fixture (1 at genesis); the
//! Book numbers its sequences from 0.  The pending TICKET tag is not compared:
//! the Book's ids and the program's both start at 1 and advance once per
//! ticket, but they are bookkeeping, not money.
//!
//! Policy choices (so a reader does not have to guess):
//!
//! * The Book is driven STEP BY STEP (`reserve_toll`, `start_ticket`,
//!   `discover_all`, `apply_senior`, `seal_weights`, `allocate_all_current`,
//!   `prune_all`, `commit`), not through `Book::allocate`.  `allocate` sends a
//!   toll whose three slices all floor to zero (toll <= 4) straight to the seed
//!   with no ticket, while the program's `toll_route` always runs a ticket when
//!   the registry is non-empty, and a ticket also materializes every open
//!   row's finalized `pending` into `funded`.  The stepwise route is the same
//!   route the program takes, so `pending` and `funded` can be compared
//!   exactly, tiny tolls included.
//! * The fixtures leave closed rows (<= 10 remaining) in the program's dense
//!   registry until the first ticket prunes them, whereas the Book never
//!   registers them.  So the initial comparison skips liveness and registry
//!   length; from the first ticket on both are compared.
//! * Registry SLOT numbers are not compared (the two start from different
//!   registries and swap-delete differently); liveness is, and the live count
//!   must equal `Pool.registry_len`.
//! * `Book`'s re-funding primitive (`fund_series_a_unchecked`) is crate
//!   private, so the mirror of `fund_series_a` is its two public halves:
//!   `accept_game_deposit(gross)` (pot and vault) and `add_backer(owner, A,
//!   floor(gross * 124 / 100))` (the entitlement top-up that reopens a capped
//!   A and keeps its funding sequence).
//! * `fund_series_a` is rate limited to 3 successes per owner per hour, so the
//!   clock is warped 3_601 s before each re-funding.
//! * A claim whose Book total is 0 must be rejected `NothingToClaim` with
//!   fee-only rollback; otherwise it must pay exactly the Book's total.
//! * A mismatch panics with the seed, the toll plan, the whole operation
//!   sequence so far and an expected (Book) versus actual (program) line for
//!   every Backer.

use std::{
    any::Any,
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::OnceLock,
};

use musical_chairs_economics::{
    backers::{BackerKey, Book, Error as BookError, Series},
    fees,
};
use musical_chairs_financial_runtime::{
    runtime::pda,
    world::{random_backer_specs, BackerSpec, World},
};
use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    state, ErrorCode,
};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

/// Seed of the whole differential ("MCDIFF30").
const SEED: u64 = 0x4D43_4449_4646_3330;
const TICKETS: usize = 50;
const REGISTRY_MIN: usize = 20;
const REGISTRY_MAX: usize = 40;

/// Ticket 1: big enough that its senior slice caps the owner's 100_000 A.
const FIRST_TOLL: u64 = 7_654_321;
const MAX_TOLL: u64 = 1_000_000_000;
/// Every ninth ticket (tickets 5, 14, 23, ...) is tiny: 1..=20 lamports.
const TINY_EVERY: usize = 9;
const TINY_PHASE: usize = 4;
const TINY_MAX: u64 = 20;

const CLAIM_EVERY: usize = 7;
const REFUND_EVERY: usize = 11;
const CREDIT_EVERY: usize = 13;
/// The minimum Series A funding (`ConfigVersion.min_a`), which mints 62_000_000.
const REFUND_GROSS: u64 = 50_000_000;
/// `fund_series_a` allows 3 successes per owner per hour.
const QUOTA_WARP_SECONDS: u64 = 3_601;
const CREDIT_MAX: u128 = 1_000_000;

const ROWS_CYCLE: [u8; 3] = [1, 4, 8];
const FIXED_ROWS: u8 = 8;
const PAGE: u64 = state::PAGE_CAPACITY as u64;

const A: u8 = SERIES_A;
const B: u8 = SERIES_B;
const ACTOR_NAMES: [&str; 3] = ["owner", "attacker", "sponsor"];

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

// ---------------------------------------------------------------------------
// The sequence: registry, toll plan, batch policy
// ---------------------------------------------------------------------------

fn series_of(series: u8) -> Series {
    if series == A {
        Series::A
    } else {
        Series::B
    }
}

/// How many rows each phase instruction of ticket `step` carries.
#[derive(Clone, Copy, Debug)]
enum Rows {
    /// 1, 4, 8, 1, 4, 8, ...
    Cycling,
    Fixed(u8),
}

impl Rows {
    fn at(self, step: usize) -> u8 {
        match self {
            Rows::Cycling => ROWS_CYCLE[step % ROWS_CYCLE.len()],
            Rows::Fixed(rows) => rows,
        }
    }
}

/// The keypair actors' public keys: `owner`, `attacker`, `sponsor`.
fn actor_keys() -> [Pubkey; 3] {
    use musical_chairs_financial_runtime::runtime::{ATTACKER_SEED, OWNER_SEED, SPONSOR_SEED};
    [OWNER_SEED, ATTACKER_SEED, SPONSOR_SEED].map(|seed| Keypair::new_from_array(seed).pubkey())
}

/// The registry both sides start from (see the module docs).
fn fixture_specs(actors: &[Pubkey; 3]) -> Vec<BackerSpec> {
    let mut rng = Rng::new(SEED ^ 0x5350_4543); // "SPEC"
    let n = REGISTRY_MIN + rng.range(0, (REGISTRY_MAX - REGISTRY_MIN) as u64) as usize;
    let mut specs = random_backer_specs(SEED, n);
    assert_eq!(
        (specs[0].series, specs[0].entitlement, specs[1].series, specs[1].entitlement),
        (A, 100_000, B, 30),
        "random_backer_specs pins its first rows: the oldest open A and a B any toll caps"
    );
    let [owner, attacker, sponsor] = *actors;

    // owner: the oldest open A (100_000) and the cap-prone B (30), as generated.
    specs[0].owner = owner;
    specs[1].owner = owner;

    // attacker: ordinary rows that a few tickets chip at.
    specs[5] = BackerSpec {
        owner: attacker,
        series: A,
        entitlement: 2_000_000 + u128::from(rng.range(0, 3_000_000)),
        lifetime: 0,
    };
    specs[6] = BackerSpec {
        owner: attacker,
        series: B,
        entitlement: 50_000 + u128::from(rng.range(0, 100_000)),
        lifetime: 0,
    };

    // sponsor: a larger, partly repaid A and a B.
    let entitlement = 30_000_000 + u128::from(rng.range(0, 30_000_000));
    specs[7] = BackerSpec {
        owner: sponsor,
        series: A,
        entitlement,
        lifetime: entitlement / 3,
    };
    specs[8] = BackerSpec {
        owner: sponsor,
        series: B,
        entitlement: 3_000_000 + u128::from(rng.range(0, 3_000_000)),
        lifetime: 0,
    };

    // Anchors no toll of this sequence can cap: both series stay open, so the
    // registry never empties and every toll takes the ticket route.
    specs[11].series = A;
    specs[11].entitlement = 100_000_000_000 + u128::from(rng.range(0, 100_000_000_000));
    specs[11].lifetime = 0;
    specs[12].series = B;
    specs[12].entitlement = 100_000_000_000 + u128::from(rng.range(0, 100_000_000_000));
    specs[12].lifetime = 0;
    specs
}

/// The toll of every ticket, a pure function of [`SEED`].
fn toll_plan() -> Vec<u64> {
    let mut rng = Rng::new(SEED ^ 0x544F_4C4C); // "TOLL"
    (0..TICKETS)
        .map(|step| {
            // Always draw, so a pinned step does not shift the later draws.
            let exponent = rng.range(0, 9) as u32;
            let low = 10u64.pow(exponent);
            let high = (low * 10 - 1).min(MAX_TOLL);
            let drawn = rng.range(low, high);
            let tiny = rng.range(1, TINY_MAX);
            if step == 0 {
                FIRST_TOLL
            } else if step % TINY_EVERY == TINY_PHASE {
                tiny
            } else {
                drawn
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// What is compared
// ---------------------------------------------------------------------------

/// One Backer, in the vocabulary both sides map to (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    entitlement: u128,
    lifetime: u128,
    pending: u64,
    funded: u64,
    active: bool,
    sequence: u64,
}

impl Row {
    fn claimable(&self) -> u128 {
        u128::from(self.pending) + u128::from(self.funded)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Totals {
    seed: u64,
    funded_claims: u128,
    registry_len: u64,
    pending_toll: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct View {
    rows: Vec<Row>,
    totals: Totals,
}

/// The fields in which two rows differ, strongest first.
fn row_differences(expected: &Row, actual: &Row, liveness: bool) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if expected.claimable() != actual.claimable() {
        fields.push("claimable (pending + funded)");
    }
    if expected.lifetime != actual.lifetime {
        fields.push("lifetime");
    }
    if expected.entitlement != actual.entitlement {
        fields.push("entitlement");
    }
    if expected.sequence != actual.sequence {
        fields.push("first funding sequence");
    }
    if liveness && expected.active != actual.active {
        fields.push("liveness (active)");
    }
    if expected.pending != actual.pending {
        fields.push("pending");
    }
    if expected.funded != actual.funded {
        fields.push("funded");
    }
    fields
}

fn host_ticket(book: &mut Book, toll: u64, chunk: usize) -> Result<(), BookError> {
    book.reserve_toll(toll)?;
    book.start_ticket(toll)?;
    book.discover_all()?;
    book.apply_senior()?;
    book.seal_weights()?;
    book.allocate_all_current(chunk)?;
    book.prune_all()?;
    book.commit()
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        "<non-string panic payload>".to_string()
    }
}

// ---------------------------------------------------------------------------
// The simulation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Stats {
    tiny_tickets: usize,
    claims_paid: usize,
    lamports_claimed: u128,
    empty_claims: usize,
    refunds: usize,
    refunds_skipped: usize,
    credits: usize,
}

#[derive(Debug)]
struct RunResult {
    view: View,
    digest: [u8; 32],
    stats: Stats,
}

struct Sim {
    world: World,
    book: Book,
    specs: Vec<BackerSpec>,
    /// The Book's `u64` owner id of each spec (one per distinct public key).
    ids: Vec<u64>,
    owner_ids: BTreeMap<Pubkey, u64>,
    actors: [Pubkey; 3],
    tolls: Vec<u64>,
    base_sequence: u64,
    seed_base: u64,
    funded_base: u128,
    history: Vec<String>,
    stats: Stats,
}

impl Sim {
    fn new() -> Self {
        let actors = actor_keys();
        let specs = fixture_specs(&actors);
        let mut world = World::with_backer_specs(&specs);
        // Genesis creates only the owner's `User`; `claim_backer` and
        // `fund_series_a` need one for each actor that signs.
        for key in [actors[1], actors[2]] {
            world.rt.replace_user(state::User {
                bump: pda(&[state::USER_SEED, key.as_ref()]).1,
                owner: key,
                ..state::User::default()
            });
        }
        let pool = world.pool();
        let base_sequence = pool.next_sequence - specs.len() as u64;

        let mut owner_ids: BTreeMap<Pubkey, u64> = BTreeMap::new();
        let ids: Vec<u64> = specs
            .iter()
            .map(|spec| {
                let next = owner_ids.len() as u64;
                *owner_ids.entry(spec.owner).or_insert(next)
            })
            .collect();
        let mut book = Book::with_pot(u64::MAX / 2);
        for (spec, id) in specs.iter().zip(&ids) {
            book.add_backer_with_lifetime(
                *id,
                series_of(spec.series),
                spec.entitlement,
                spec.lifetime,
            )
            .expect("host Book Backer");
        }
        Self {
            world,
            book,
            specs,
            ids,
            owner_ids,
            actors,
            tolls: toll_plan(),
            base_sequence,
            seed_base: pool.seed,
            funded_base: pool.funded_claims,
            history: Vec::new(),
            stats: Stats::default(),
        }
    }

    fn actor(&self, index: usize) -> Keypair {
        match index {
            0 => self.world.owner(),
            1 => self.world.attacker(),
            _ => self.world.sponsor(),
        }
    }

    fn describe(&self, index: usize) -> String {
        let spec = &self.specs[index];
        let who = match self.actors.iter().position(|key| *key == spec.owner) {
            Some(actor) => ACTOR_NAMES[actor].to_string(),
            None => spec.owner.to_string().chars().take(6).collect(),
        };
        format!("{} {}", if spec.series == A { "A" } else { "B" }, who)
    }

    fn key(&self, index: usize) -> BackerKey {
        BackerKey::new(self.ids[index], series_of(self.specs[index].series))
    }

    /// The Book's view.
    fn expected(&self) -> View {
        let rows: Vec<Row> = (0..self.specs.len())
            .map(|index| {
                let row = self.book.backer(self.key(index)).expect("host Book row");
                Row {
                    entitlement: row.entitlement,
                    lifetime: row.lifetime_credited,
                    pending: row.pending,
                    funded: row.funded,
                    active: row.active_slot.is_some(),
                    sequence: row.first_funding_sequence,
                }
            })
            .collect();
        let live = rows.iter().filter(|row| row.active).count() as u64;
        View {
            totals: Totals {
                seed: self.book.seed(),
                funded_claims: self.book.global_funded_claims(),
                registry_len: live,
                pending_toll: self.book.pending_toll,
            },
            rows,
        }
    }

    /// The program's view.
    fn actual(&self) -> View {
        let rows: Vec<Row> = self
            .specs
            .iter()
            .map(|spec| {
                let backer = self.world.backer(spec.owner, spec.series);
                Row {
                    entitlement: backer.entitlement,
                    lifetime: backer.lifetime,
                    pending: backer.pending,
                    funded: backer.funded,
                    active: backer.active,
                    sequence: backer.first_sequence.wrapping_sub(self.base_sequence),
                }
            })
            .collect();
        let pool = self.world.pool();
        View {
            totals: Totals {
                seed: pool.seed - self.seed_base,
                funded_claims: pool.funded_claims - self.funded_base,
                registry_len: pool.registry_len,
                pending_toll: pool.pending_toll,
            },
            rows,
        }
    }

    /// Compare the two sides.  `liveness` is false only before the first
    /// ticket (see the module docs).
    fn check(&self, context: &str, liveness: bool) {
        let expected = self.expected();
        let actual = self.actual();
        let mut problems = Vec::new();
        for (index, (e, a)) in expected.rows.iter().zip(&actual.rows).enumerate() {
            for field in row_differences(e, a, liveness) {
                problems.push(format!(
                    "Backer #{index} ({}) differs in {field}: expected {e:?}, actual {a:?}",
                    self.describe(index)
                ));
            }
        }
        let (e, a) = (&expected.totals, &actual.totals);
        if e.seed != a.seed {
            problems.push(format!("Pool.seed delta: expected {}, actual {}", e.seed, a.seed));
        }
        if e.funded_claims != a.funded_claims {
            problems.push(format!(
                "Pool.funded_claims delta: expected {}, actual {}",
                e.funded_claims, a.funded_claims
            ));
        }
        if e.pending_toll != a.pending_toll {
            problems.push(format!(
                "Pool.pending_toll: expected {}, actual {}",
                e.pending_toll, a.pending_toll
            ));
        }
        if liveness && e.registry_len != a.registry_len {
            problems.push(format!(
                "registry length: expected {} live Backers, actual {}",
                e.registry_len, a.registry_len
            ));
        }
        // The program's own claim bookkeeping: every claimable lamport is a
        // funded claim, and nothing else is.
        let held: u128 = actual.rows.iter().map(Row::claimable).sum();
        if held != a.funded_claims {
            problems.push(format!(
                "program: Backers hold {held} claimable lamports but Pool.funded_claims delta is {}",
                a.funded_claims
            ));
        }
        if let Err(error) = self.book.conservation() {
            problems.push(format!("host Book conservation failed: {error}"));
        }
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| self.world.assert_vault_identity())) {
            problems.push(format!("program vault identity: {}", panic_text(payload)));
        }
        if !problems.is_empty() {
            self.fail(context, &problems);
        }
    }

    /// Panic with the whole story: seed, toll plan, every operation so far and
    /// expected (Book) versus actual (program) for every Backer.
    fn fail(&self, context: &str, problems: &[String]) -> ! {
        let mut report = format!(
            "allocation differential FAILED\n  seed {SEED:#018x}\n  at: {context}\n"
        );
        for problem in problems {
            report.push_str(&format!("  mismatch: {problem}\n"));
        }
        report.push_str(&format!("  toll plan: {:?}\n  sequence so far:\n", self.tolls));
        for line in &self.history {
            report.push_str(&format!("    {line}\n"));
        }
        report.push_str("  per Backer, expected (host Book) versus actual (program):\n");
        let expected = self.expected();
        let actual = self.actual();
        for (index, (e, a)) in expected.rows.iter().zip(&actual.rows).enumerate() {
            let mark = if e == a { "  " } else { "!!" };
            report.push_str(&format!(
                "   {mark} #{index} ({})\n         expected {e:?}\n         actual   {a:?}\n",
                self.describe(index)
            ));
        }
        report.push_str(&format!(
            "  totals: expected {:?}\n          actual   {:?}\n",
            expected.totals, actual.totals
        ));
        panic!("{report}");
    }

    // -----------------------------------------------------------------
    // Operations (each mirrored on the Book, then compared)
    // -----------------------------------------------------------------

    fn ticket(&mut self, n: usize, rows: u8) {
        let toll = self.tolls[n - 1];
        let context = format!("ticket {n} (toll {toll}, rows {rows})");
        self.history.push(format!("ticket {n}: toll {toll}, rows {rows}"));
        if toll <= TINY_MAX {
            self.stats.tiny_tickets += 1;
        }
        let driven = catch_unwind(AssertUnwindSafe(|| {
            self.world.start_ticket_fixture(toll);
            self.world.advance_until_idle(rows);
        }));
        if let Err(payload) = driven {
            self.fail(
                &context,
                &[format!("the program's ticket panicked: {}", panic_text(payload))],
            );
        }
        if let Err(error) = host_ticket(&mut self.book, toll, usize::from(rows)) {
            self.fail(&context, &[format!("the host Book refused the ticket: {error}")]);
        }
        self.check(&format!("after {context}"), true);
        drop(self.world.take_log());
    }

    fn claim(&mut self, n: usize, actor: usize) {
        let owner = self.actor(actor);
        let owner_key = owner.pubkey();
        let id = self.owner_ids[&owner_key];
        let expected: u128 = self
            .expected()
            .rows
            .iter()
            .zip(&self.ids)
            .filter(|(_, row_id)| **row_id == id)
            .map(|(row, _)| row.claimable())
            .sum();
        let context = format!("claim after ticket {n} ({})", ACTOR_NAMES[actor]);
        self.history.push(format!(
            "claim after ticket {n}: {} (Book owner {id}), Book total {expected}",
            ACTOR_NAMES[actor]
        ));
        let destination = self.world.rt.management.pubkey();
        let variant = format!("claim.{n}");
        if expected == 0 {
            let before = self.world.snapshot_all();
            let rejected = match self.world.claim_backer(&variant, &owner, destination) {
                Ok(paid) => self.fail(
                    &context,
                    &[format!("the program paid {paid} where the host Book has nothing")],
                ),
                Err(observation) => observation,
            };
            self.world.assert_rejected(&rejected, ErrorCode::NothingToClaim);
            self.world.assert_fee_only(&before, &rejected);
            self.stats.empty_claims += 1;
        } else {
            let paid = match self.world.claim_backer(&variant, &owner, destination) {
                Ok(paid) => paid,
                Err(observation) => self.fail(
                    &context,
                    &[format!("the program rejected a claim worth {expected}: {observation:?}")],
                ),
            };
            if u128::from(paid) != expected {
                self.fail(
                    &context,
                    &[format!("the program paid {paid}, the host Book total is {expected}")],
                );
            }
            self.stats.claims_paid += 1;
            self.stats.lamports_claimed += expected;
        }
        let total = self
            .book
            .claim(id)
            .unwrap_or_else(|error| self.fail(&context, &[format!("host Book claim: {error}")]));
        if u128::from(total) != expected {
            self.fail(
                &context,
                &[format!("host Book claim returned {total}, its rows summed to {expected}")],
            );
        }
        self.check(&format!("after {context}"), true);
        drop(self.world.take_log());
    }

    /// A REAL `fund_series_a` for the first actor (rotating with `n`) whose
    /// Series A is capped (out of the registry): it reopens with its original
    /// funding sequence, behind everyone else in the registry.
    fn refund(&mut self, n: usize) {
        let capped = (0..3)
            .map(|offset| (n + offset) % 3)
            .find(|&actor| {
                let index = self
                    .specs
                    .iter()
                    .position(|spec| spec.owner == self.actors[actor] && spec.series == A)
                    .expect("every actor holds a Series A row");
                !self.expected().rows[index].active
            });
        let Some(actor) = capped else {
            self.history.push(format!("re-fund after ticket {n}: skipped, no capped actor A"));
            self.stats.refunds_skipped += 1;
            return;
        };
        let pool = self.world.pool();
        let tail = pool.registry_len / PAGE;
        if self.world.account(self.world.rt.registry_address(tail)).is_none() {
            self.history.push(format!(
                "re-fund after ticket {n}: skipped, registry tail page {tail} is not provisioned"
            ));
            self.stats.refunds_skipped += 1;
            return;
        }
        let context = format!("re-funding after ticket {n} ({})", ACTOR_NAMES[actor]);
        self.history.push(format!(
            "re-fund after ticket {n}: {} funds {REFUND_GROSS} lamports into its capped A",
            ACTOR_NAMES[actor]
        ));
        self.world.warp_seconds(QUOTA_WARP_SECONDS);
        let owner = self.actor(actor);
        if let Err(observation) = self.world.fund_series_a(
            &format!("refund.{n}"),
            &owner,
            REFUND_GROSS,
            pool.round_id,
        ) {
            self.fail(
                &context,
                &[format!("the program rejected the re-funding: {observation:?}")],
            );
        }
        // Mirror: the pot and vault take the cash, the entitlement grows by
        // the 124 % mint and the capped row reopens.
        let minted = fees::series_a_entitlement(REFUND_GROSS).expect("mint fits");
        let id = self.owner_ids[&self.actors[actor]];
        if let Err(error) = self.book.accept_game_deposit(REFUND_GROSS) {
            self.fail(&context, &[format!("host Book deposit: {error}")]);
        }
        if let Err(error) = self.book.add_backer(id, Series::A, minted) {
            self.fail(&context, &[format!("host Book top-up: {error}")]);
        }
        self.stats.refunds += 1;
        self.check(&format!("after {context}"), true);
        drop(self.world.take_log());
    }

    /// `fund_backer_fixture`: finalized claims credited to a drawn open
    /// Backer, keeping it open (the fixture does not prune, the Book does).
    fn credit(&mut self, n: usize, rng: &mut Rng) {
        let expected = self.expected();
        let candidates: Vec<usize> = (0..self.specs.len())
            .filter(|&index| {
                let row = &expected.rows[index];
                row.active && row.entitlement - row.lifetime >= 100
            })
            .collect();
        assert!(!candidates.is_empty(), "the anchors keep the registry open");
        let index = candidates[rng.range(0, candidates.len() as u64 - 1) as usize];
        let row = &expected.rows[index];
        // Leave at least 11 lamports, so the Backer is still open afterwards.
        let cap = (row.entitlement - row.lifetime - 11).min(CREDIT_MAX);
        let amount = rng.range(1, cap as u64);
        let spec = self.specs[index];
        let context = format!("credit after ticket {n} (Backer #{index})");
        let line = format!(
            "credit after ticket {n}: {amount} lamports of finalized claims to Backer #{index} ({})",
            self.describe(index)
        );
        self.history.push(line);
        self.world.fund_backer_fixture(spec.owner, spec.series, amount);
        if let Err(error) =
            self.book
                .seed_funded_fixture(self.ids[index], series_of(spec.series), amount)
        {
            self.fail(&context, &[format!("host Book funded fixture: {error}")]);
        }
        self.stats.credits += 1;
        self.check(&format!("after {context}"), true);
    }

    /// Run the whole sequence under `rows`.
    fn run(rows: Rows) -> RunResult {
        let mut sim = Sim::new();
        sim.history.push(format!(
            "fixture: {} Backers, owners {:?}, rows policy {rows:?}",
            sim.specs.len(),
            sim.owner_ids.len()
        ));
        sim.check("the fixture, before any ticket", false);
        let mut rng = Rng::new(SEED ^ 0x4F50_5321); // "OPS!"
        for step in 0..TICKETS {
            let n = step + 1;
            sim.ticket(n, rows.at(step));
            if n % CLAIM_EVERY == 0 {
                // The first claim is the owner's, who holds finalized credit
                // by then; the later ones draw an actor.
                let actor = if n == CLAIM_EVERY { 0 } else { rng.range(0, 2) as usize };
                sim.claim(n, actor);
            }
            if n % REFUND_EVERY == 0 {
                sim.refund(n);
            }
            if n % CREDIT_EVERY == 0 {
                sim.credit(n, &mut rng);
            }
        }
        let view = sim.actual();
        assert_eq!(view, sim.expected(), "the final states agree");
        RunResult {
            view,
            digest: sim.world.digest_financial_state(),
            stats: sim.stats,
        }
    }
}

/// The sequence under [`Rows::Cycling`] runs once and is shared by both tests.
fn cycling_run() -> &'static RunResult {
    static RUN: OnceLock<RunResult> = OnceLock::new();
    RUN.get_or_init(|| Sim::run(Rows::Cycling))
}

fn assert_coverage(run: &RunResult) {
    let stats = &run.stats;
    assert!(stats.tiny_tickets >= 1, "{stats:?}: no tiny toll was exercised");
    assert!(
        stats.claims_paid >= 1 && stats.lamports_claimed > 0,
        "{stats:?}: no claim paid anything"
    );
    assert!(stats.refunds >= 1, "{stats:?}: no capped A was re-funded");
    assert!(stats.credits >= 1, "{stats:?}: no funded-claims credit was applied");
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn fifty_random_tickets_match_the_host_book() {
    eprintln!(
        "allocation_diff: SEED = {SEED:#018x}, {TICKETS} tickets, rows cycling {ROWS_CYCLE:?}"
    );
    let run = cycling_run();
    eprintln!("allocation_diff: {:?}", run.stats);
    assert_coverage(run);
    assert!(run.view.rows.len() >= REGISTRY_MIN, "the registry is 20..=40 Backers");
}

#[test]
fn chunk_size_does_not_change_the_differential() {
    let cycling = cycling_run();
    let fixed = Sim::run(Rows::Fixed(FIXED_ROWS));
    assert_eq!(
        fixed.view.rows, cycling.view.rows,
        "rows = {FIXED_ROWS} must reach the per-Backer state of the cycling run"
    );
    assert_eq!(
        fixed.view.totals, cycling.view.totals,
        "rows = {FIXED_ROWS} must reach the seed, funded claims and registry of the cycling run"
    );
    assert_eq!(
        fixed.digest, cycling.digest,
        "rows = {FIXED_ROWS} must reach exactly the financial state of the cycling run"
    );
    assert_eq!(fixed.stats.refunds, cycling.stats.refunds);
}
