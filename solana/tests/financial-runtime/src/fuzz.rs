//! M8 randomized conservation sequence (plan Milestone 8, step 1, last row):
//! `randomized_300_step_sequence_keeps_both_conservation_identities`.
//!
//! One [`World`] (the actual compiled SBF plus the captured ORAO executable in
//! LiteSVM) is driven for exactly [`STEPS`] steps by a deterministic
//! xorshift64* generator seeded with [`SEED`] (`0x4D43_4655_5A5A_3038`,
//! "MCFUZZ08").  Every step is one of three kinds, drawn from the seed:
//!
//! * **advance** -- the next legal move of a *progress engine* that reads the
//!   chain state and plays real rounds to their end: positions opened by real
//!   keypairs, a Series A Backer, a whale whose payout turns insolvent after
//!   enough (clock-warped) days, the closing payout with an engineered reserve
//!   split, the closing allocation ticket (discover, senior, seal, allocate,
//!   prune, commit), claims, locked candidate pages, `build_candidates`, the
//!   bound ORAO request, the provider's fulfilment, capture, observation, the
//!   G-signed provider return, `apply_promotion` or `expire_promotion`,
//!   `finalize_round`, and the delivery of any deferred owner refund;
//! * **perturb** -- an instruction of ANY family at an arbitrary moment (free
//!   and caller-funded page provisioning of every kind, budget increases,
//!   operations funding and withdrawal, anonymous credits, solvent withdrawals,
//!   extra positions, Series A funding, claims, reconciles, cleanup of
//!   whatever page is closable, clock warps, an emptied or refilled owner
//!   destination, ...), valid when the state allows it and rejected
//!   otherwise -- rejections are recorded by code, never hidden;
//! * **adversary** -- an instruction that MUST be refused (wrong signer, stale
//!   round or ticket id, a replay, an out-of-order closing step, a wrong
//!   destination, a cleanup of a live page), with the exact program code
//!   asserted wherever the handler's check order makes it certain.
//!
//! Every step -- success or rejection -- is followed by both conservation
//! identities, the partition identity `ops.locked_remaining_total == sum of
//! every CloseRecord's remaining partitions` (a settled close that holds a
//! deferred owner refund included), and the books-consistency checks of
//! `assert_books` (aggregate provider budget, outstanding request rent, the
//! terminal-iff-settled rule).  Any failure panics with the seed, the step and
//! the WHOLE operation history, so the sequence can be replayed.
//!
//! Coverage is asserted, not hoped for (`Coverage`): at least two finalized
//! rounds, one committed ticket, one paid claim, one executed provider return
//! and one delivered deferred refund, and every instruction family attempted at
//! least once.  Closes 1 and 2 follow fixed profiles (close 1: the full draw,
//! a provider return and an owner refund that is DEFERRED by emptying the
//! owner's destination in the settling transaction; close 2: expiry), so the
//! counters do not depend on the generator's luck.
//!
//! Limits: ONE synthetic fulfill authority, no cluster latency, the Squads
//! program is the test-only stand-in; LiteSVM applies the rent-state rule
//! only to accounts with data, so "undeliverable" is the destination holding
//! fewer lamports than the credit needs to reach the rent-exempt minimum.

use std::collections::{BTreeMap, BTreeSet};

use musical_chairs_program::{
    allocation::{SERIES_A, SERIES_B},
    instruction,
    positions::PlanKindArg,
    state, ErrorCode, Funding, PageKind, RoundFinalized, ID,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

use crate::close::{
    apply_ix, apply_ix_with, assert_identities, budget_ix, build_ix, build_next, candidate_page,
    candidate_page_key, capture_ix, client_spendable, expire_ix, expected_candidates, finalize_ix,
    host_request, observe_ix, pages_of, provision_ix, reconcile_client_ix, reconcile_ix,
    request_ix, withdraw_accounts, withdraw_ix, ApplyKeys,
};
use crate::controls::{assert_outcomes_consistent, assert_unique_labels, custom_code};
use crate::reserve::{
    close_page_ix, deliver_ix, fund_ops_ix, provision_close_record_ix, provision_kind_ix,
    transfer_ix, withdraw_ops_ix,
};
use crate::runtime::{anchor_ix, Observation};
use crate::world::{code, CloseRecordArg, Outcome, PayoutKind, World, SOL};

/// "MCFUZZ08".
pub const SEED: u64 = 0x4D43_4655_5A5A_3038;
/// The number of steps (loop iterations; clock warps are steps too).
pub const STEPS: usize = 300;
/// The row's name; labels are `financial.fuzz.<ROW>[.stepNNN.<op>]`.
pub const ROW: &str = "randomized_300_step_sequence_keeps_both_conservation_identities";

/// The owner opt-in of a deferral close: below the rent-exempt minimum.
const OWNER_PART: u64 = 400_000;
/// Positions per round that the engine opens (the whale and four others).
const ROUND_PARTICIPANTS: u64 = 5;

fn root() -> String {
    format!("financial.fuzz.{ROW}")
}

// ---------------------------------------------------------------------------
// The generator
// ---------------------------------------------------------------------------

/// xorshift64* (Vigna), the same generator `tests/allocation_diff.rs` uses.
#[derive(Clone, Debug)]
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

    fn below(&mut self, n: usize) -> usize {
        self.range(0, n as u64 - 1) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.range(0, 99) < percent
    }
}

// ---------------------------------------------------------------------------
// Actors
// ---------------------------------------------------------------------------

/// Indices into `Fuzz::keys`.
const PLAYERS: usize = 6;
const BACKER0: usize = 6;
const SPONSOR: usize = 8;
const D0: usize = 9;
const OUTSIDER: usize = 10;

// ---------------------------------------------------------------------------
// Instruction families, plans, expectations
// ---------------------------------------------------------------------------

/// The instruction families of the plan's randomized list (and the provider's
/// own fulfilment, which the harness drives as the synthetic authority).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Fam {
    Open,
    FundSeriesA,
    WithdrawSimple,
    SettleCompounding,
    CloseSimple,
    Discover,
    Senior,
    Seal,
    Allocate,
    Prune,
    Commit,
    ClaimBacker,
    ProvisionFree,
    ProvisionCaller,
    ProvisionLocked,
    ProvisionCloseRecord,
    Build,
    Request,
    Fulfill,
    Capture,
    Apply,
    Expire,
    Finalize,
    Budget,
    Observe,
    ReturnG,
    Deliver,
    Reconcile,
    ReconcileClient,
    CloseShared,
    FundOps,
    WithdrawOps,
    /// Plain system transfers and the like (not an instruction family).
    Aux,
}

const ALL_FAMILIES: [Fam; 32] = [
    Fam::Open,
    Fam::FundSeriesA,
    Fam::WithdrawSimple,
    Fam::SettleCompounding,
    Fam::CloseSimple,
    Fam::Discover,
    Fam::Senior,
    Fam::Seal,
    Fam::Allocate,
    Fam::Prune,
    Fam::Commit,
    Fam::ClaimBacker,
    Fam::ProvisionFree,
    Fam::ProvisionCaller,
    Fam::ProvisionLocked,
    Fam::ProvisionCloseRecord,
    Fam::Build,
    Fam::Request,
    Fam::Fulfill,
    Fam::Capture,
    Fam::Apply,
    Fam::Expire,
    Fam::Finalize,
    Fam::Budget,
    Fam::Observe,
    Fam::ReturnG,
    Fam::Deliver,
    Fam::Reconcile,
    Fam::ReconcileClient,
    Fam::CloseShared,
    Fam::FundOps,
    Fam::WithdrawOps,
];

impl Fam {
    fn name(self) -> &'static str {
        match self {
            Fam::Open => "open_position",
            Fam::FundSeriesA => "fund_series_a",
            Fam::WithdrawSimple => "withdraw_simple",
            Fam::SettleCompounding => "settle_compounding",
            Fam::CloseSimple => "close_simple_position",
            Fam::Discover => "discover_rows",
            Fam::Senior => "apply_senior",
            Fam::Seal => "seal_weights",
            Fam::Allocate => "allocate_rows",
            Fam::Prune => "prune_rows",
            Fam::Commit => "commit_ticket",
            Fam::ClaimBacker => "claim_backer",
            Fam::ProvisionFree => "provision_page_free",
            Fam::ProvisionCaller => "provision_page_caller",
            Fam::ProvisionLocked => "provision_page_locked_close",
            Fam::ProvisionCloseRecord => "provision_close_record",
            Fam::Build => "build_candidates",
            Fam::Request => "request_round_draw",
            Fam::Fulfill => "orao_fulfill",
            Fam::Capture => "capture_round_draw",
            Fam::Apply => "apply_promotion",
            Fam::Expire => "expire_promotion",
            Fam::Finalize => "finalize_round",
            Fam::Budget => "increase_provider_budget",
            Fam::Observe => "observe_request_fulfilled",
            Fam::ReturnG => "withdraw_provider_return",
            Fam::Deliver => "deliver_owner_refund",
            Fam::Reconcile => "reconcile_ops_cash",
            Fam::ReconcileClient => "reconcile_ops_cash_with_client",
            Fam::CloseShared => "close_shared_account",
            Fam::FundOps => "fund_operations",
            Fam::WithdrawOps => "withdraw_operations",
            Fam::Aux => "aux_transfer",
        }
    }
}

/// What a step must do.
#[derive(Clone, Copy, Debug)]
enum Expect {
    /// Anything the program allows (rejections are recorded by code).
    Any,
    /// Must be refused (any code).
    Reject,
    /// Must be refused with exactly this program code.
    Code(ErrorCode),
}

/// One planned transaction.
struct Plan {
    fam: Fam,
    name: String,
    ixs: Vec<Instruction>,
    /// Indices into `Fuzz::keys` that sign besides the harness fee payer.
    signers: Vec<usize>,
    /// Execute the (single) instruction as the G vault through the stand-in.
    via_g: bool,
    expect: Expect,
    /// A position this transaction opens (recorded when it succeeds).
    track: Option<PositionRec>,
}

impl Plan {
    fn new(fam: Fam, name: impl Into<String>, ix: Instruction) -> Self {
        Self {
            fam,
            name: name.into(),
            ixs: vec![ix],
            signers: Vec::new(),
            via_g: false,
            expect: Expect::Any,
            track: None,
        }
    }

    fn signed(mut self, signers: &[usize]) -> Self {
        self.signers = signers.to_vec();
        self
    }

    fn via_g(mut self) -> Self {
        self.via_g = true;
        self
    }

    fn expecting(mut self, expect: Expect) -> Self {
        self.expect = expect;
        self
    }
}

/// How a close is played by the progress engine.
#[derive(Clone, Copy, Debug)]
struct Profile {
    /// Request, fulfil, observe, return, capture, apply (else let it expire).
    full_path: bool,
    /// Pay the owner's destination `D0` as the closing destination, lock a
    /// sub-minimum owner part, and empty `D0` in the settling transaction.
    deferral: bool,
}

fn profile(close_id: u64) -> Profile {
    match close_id {
        1 => Profile { full_path: true, deferral: true },
        2 => Profile { full_path: false, deferral: false },
        n => Profile { full_path: n % 2 == 1, deferral: n % 3 == 0 },
    }
}

/// The counters the run asserts.
#[derive(Clone, Copy, Debug, Default)]
struct Coverage {
    finalized_rounds: u64,
    commits: u64,
    claims_paid: u64,
    provider_returns: u64,
    deferred_delivered: u64,
    promotions_applied: u64,
    promotions_skipped: u64,
    requests: u64,
    deferrals_booked: u64,
}

#[derive(Clone, Copy, Debug)]
struct PositionRec {
    player: usize,
    nonce: u64,
    round: u64,
    plan: PlanKindArg,
    gross: u64,
}

// ---------------------------------------------------------------------------
// The run state
// ---------------------------------------------------------------------------

struct Fuzz {
    w: World,
    rng: Rng,
    keys: Vec<Keypair>,
    step: usize,
    history: Vec<String>,
    next_nonce: u64,
    positions: Vec<PositionRec>,
    tries: BTreeMap<(u64, usize), u32>,
    whale: usize,
    deliver_tries: BTreeMap<u64, u32>,
    tally: BTreeMap<Fam, (u64, u64)>,
    codes: BTreeMap<u32, u64>,
    coverage: Coverage,
    engine_failures: u32,
    engine_rejections: u64,
    adversary_draws: u64,
    deferred_seen: BTreeSet<u64>,
    claimed_close: BTreeSet<u64>,
    spent_nonces: BTreeSet<u64>,
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        "a non-string panic".to_string()
    }
}

impl Fuzz {
    fn new() -> Self {
        let mut w = World::genesis_with_provider().scoped_label(root());
        w.provider_setup();
        let mut keys = vec![w.owner(), w.attacker()];
        for index in 0..4u8 {
            keys.push(Keypair::new_from_array([0x91 + index; 32]));
        }
        keys.push(Keypair::new_from_array([0xB1; 32]));
        keys.push(Keypair::new_from_array([0xB2; 32]));
        keys.push(w.sponsor());
        keys.push(Keypair::new_from_array([0xD0; 32]));
        keys.push(Keypair::new_from_array([0x0F; 32]));
        // Every actor but the owner, the attacker and the sponsor (funded by the
        // harness) and the destination D0 (funded only by closing payouts).
        for index in (2..BACKER0 + 2).chain([OUTSIDER]) {
            let key = keys[index].pubkey();
            w.rt.credit_lamports(key, 100 * SOL);
        }
        w.watch(keys[D0].pubkey());
        Self {
            w,
            rng: Rng::new(SEED),
            keys,
            step: 0,
            history: Vec::new(),
            next_nonce: 100,
            positions: Vec::new(),
            tries: BTreeMap::new(),
            whale: 0,
            deliver_tries: BTreeMap::new(),
            tally: BTreeMap::new(),
            codes: BTreeMap::new(),
            coverage: Coverage::default(),
            engine_failures: 0,
            engine_rejections: 0,
            adversary_draws: 0,
            deferred_seen: BTreeSet::new(),
            claimed_close: BTreeSet::new(),
            spent_nonces: BTreeSet::new(),
        }
    }

    fn pk(&self, index: usize) -> Pubkey {
        self.keys[index].pubkey()
    }

    fn payer(&self) -> Pubkey {
        self.w.rt.payer.pubkey()
    }

    // -----------------------------------------------------------------
    // Execution and bookkeeping
    // -----------------------------------------------------------------

    /// Send one planned transaction, record it and judge it against its
    /// expectation.  Returns whether it succeeded.
    fn exec(&mut self, plan: Plan) -> bool {
        let variant = format!("step{:03}.{}", self.step, plan.name);
        let (fam, name, expect, via_g, track) =
            (plan.fam, plan.name.clone(), plan.expect, plan.via_g, plan.track);
        let outcome = {
            let signers: Vec<&Keypair> = plan.signers.iter().map(|&index| &self.keys[index]).collect();
            if via_g {
                let inner = plan.ixs.into_iter().next().expect("one inner instruction");
                self.w.send_via_g(&variant, inner, &signers)
            } else {
                self.w.send(&variant, plan.ixs, &signers)
            }
        };
        let success = self.finish(fam, &name, outcome, expect);
        if let (true, Some(position)) = (success, track) {
            self.positions.push(position);
        }
        success
    }

    /// Record an executed transaction (however it was sent).
    fn finish(&mut self, fam: Fam, name: &str, outcome: Outcome, expect: Expect) -> bool {
        let (success, observation) = match outcome {
            Ok(observation) => (true, observation),
            Err(observation) => (false, observation),
        };
        let error_code = custom_code(&observation);
        {
            let entry = self.tally.entry(fam).or_insert((0, 0));
            entry.0 += 1;
            if success {
                entry.1 += 1;
            }
        }
        if !success {
            *self.codes.entry(error_code.unwrap_or(0)).or_insert(0) += 1;
        }
        self.history.push(format!(
            "step {:03} {:<30} {:<28} -> {} (cu {:?}, packet {} B)",
            self.step,
            fam.name(),
            name,
            if success {
                "ok".to_string()
            } else {
                format!("REJECT {}", error_code.map_or("none".to_string(), |code| code.to_string()))
            },
            observation.compute_units,
            observation.packet_bytes
        ));
        if success {
            self.count_success(fam, &observation);
        }
        match expect {
            Expect::Any => {}
            Expect::Reject => assert!(
                !success,
                "adversarial step {name} ({}) unexpectedly SUCCEEDED: {observation:?}",
                fam.name()
            ),
            Expect::Code(expected) => {
                assert!(
                    !success,
                    "adversarial step {name} ({}) unexpectedly SUCCEEDED: {observation:?}",
                    fam.name()
                );
                assert_eq!(
                    error_code,
                    Some(code(expected)),
                    "adversarial step {name} ({}): expected {expected:?}, got {:?}",
                    fam.name(),
                    observation.error
                );
            }
        }
        success
    }

    fn count_success(&mut self, fam: Fam, observation: &Observation) {
        match fam {
            Fam::Commit => self.coverage.commits += 1,
            Fam::ClaimBacker => self.coverage.claims_paid += 1,
            Fam::ReturnG => self.coverage.provider_returns += 1,
            Fam::Deliver => self.coverage.deferred_delivered += 1,
            Fam::Finalize => {
                if !observation.events::<RoundFinalized>().is_empty() {
                    self.coverage.finalized_rounds += 1;
                }
            }
            Fam::Apply => self.coverage.promotions_applied += 1,
            Fam::Expire => self.coverage.promotions_skipped += 1,
            Fam::Request => self.coverage.requests += 1,
            _ => {}
        }
    }

    // -----------------------------------------------------------------
    // The invariants checked after EVERY step
    // -----------------------------------------------------------------

    /// Both conservation identities, the partition identity, and the books
    /// consistency rules that hold for every transition.
    fn assert_books(&self) {
        assert_identities(&self.w);
        let pool = self.w.pool();
        let ops = self.w.ops();
        let mut eligible_sum = 0u64;
        let mut outstanding = 0u64;
        for close_id in 1..=pool.next_close_id {
            let Some(record) = self
                .w
                .rt
                .try_deserialize::<state::CloseRecord>(self.w.rt.close_record_address(close_id))
            else {
                continue;
            };
            let terminal = record.promotion != state::Promotion::Pending;
            assert_eq!(
                terminal, record.reserve_settled,
                "close {close_id}: a terminal promotion is settled exactly when it is terminal"
            );
            assert!(
                record.locked_sponsor_remaining + record.locked_owner_remaining <= record.required_reserve,
                "close {close_id}: the remaining partitions never exceed the required reserve"
            );
            if record.reserve_settled {
                assert_eq!(record.locked_sponsor_remaining, 0, "close {close_id}: the sponsor partition is released at settlement");
            }
            if record.finalized {
                assert!(terminal && record.cash_committed, "close {close_id}: finalized implies terminal and committed");
            }
            eligible_sum += record.eligible_return_remaining;
            if record.draw_state != state::DRAW_NOT_REQUESTED && !record.reimbursement_observed {
                outstanding += record.request_rent_funded_s + record.request_rent_funded_o;
            }
        }
        assert_eq!(
            ops.eligible_return_budget, eligible_sum,
            "the aggregate provider budget is the sum of the closes' eligible_return_remaining"
        );
        assert_eq!(
            ops.request_rent_outstanding, outstanding,
            "outstanding request rent is the funded rent of the requests not yet observed"
        );
    }

    // -----------------------------------------------------------------
    // The loop
    // -----------------------------------------------------------------

    fn run(&mut self) {
        for step in 0..STEPS {
            self.step = step;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.one_step();
                self.assert_books();
                self.note_deferrals();
            }));
            if let Err(payload) = result {
                let message = panic_text(payload.as_ref());
                panic!(
                    "randomized conservation sequence FAILED\n  seed {SEED:#018x}\n  at step {step}\n  {message}\n  --- operation history ({} lines) ---\n{}",
                    self.history.len(),
                    self.history.join("\n")
                );
            }
        }
    }

    /// Remember every close that holds a deferred owner refund.
    fn note_deferrals(&mut self) {
        let pool = self.w.pool();
        for close_id in 1..pool.next_close_id {
            if let Some(record) = self
                .w
                .rt
                .try_deserialize::<state::CloseRecord>(self.w.rt.close_record_address(close_id))
            {
                if record.reserve_settled && record.locked_owner_remaining > 0 {
                    self.deferred_seen.insert(close_id);
                }
            }
        }
        self.coverage.deferrals_booked = self.deferred_seen.len() as u64;
    }

    fn behind_schedule(&self) -> bool {
        (self.step >= 110 && self.coverage.finalized_rounds < 1)
            || (self.step >= 190 && self.coverage.finalized_rounds < 2)
            || (self.step >= 240 && !self.coverage_met())
    }

    fn coverage_met(&self) -> bool {
        let c = &self.coverage;
        c.finalized_rounds >= 2
            && c.commits >= 1
            && c.claims_paid >= 1
            && c.provider_returns >= 1
            && c.deferred_delivered >= 1
    }

    fn one_step(&mut self) {
        let roll = self.rng.range(0, 99);
        let sweep = self.step % 5 == 4 && self.step < 215;
        if sweep {
            self.sweep();
        } else if self.behind_schedule() || roll < 52 {
            self.advance();
        } else if roll < 82 {
            self.perturb();
        } else {
            self.adversary();
        }
    }
}

// ---------------------------------------------------------------------------
// Actions and the progress engine
// ---------------------------------------------------------------------------

/// What a step does.
enum Action {
    Tx(Plan),
    /// The provider's `fulfill` for a close (the synthetic authority).
    Fulfill(u64),
    /// Advance the clock.
    Warp(u64, &'static str),
    /// Nothing applies right now.
    Nothing,
}

impl Fuzz {
    fn run_action(&mut self, action: Action) -> Option<bool> {
        match action {
            Action::Tx(plan) => Some(self.exec(plan)),
            Action::Fulfill(close_id) => {
                let name = format!("fulfill_c{close_id}");
                let variant = format!("step{:03}.{name}", self.step);
                let outcome = self.w.fulfill(&variant, close_id);
                Some(self.finish(Fam::Fulfill, &name, outcome, Expect::Any))
            }
            Action::Warp(seconds, why) => {
                self.warp(seconds, why);
                None
            }
            Action::Nothing => None,
        }
    }

    fn warp(&mut self, seconds: u64, why: &str) {
        self.w.warp_seconds(seconds);
        self.history.push(format!(
            "step {:03} {:<30} +{seconds} s ({why}) -> clock {}",
            self.step,
            "clock_warp",
            self.w.now()
        ));
    }

    fn deposits(&self, player: usize, round: u64) -> u128 {
        let owner = self.pk(player);
        self.w
            .rt
            .try_deserialize::<state::UserRound>(self.w.rt.user_round_address(round, owner))
            .map_or(0, |user_round| user_round.deposits)
    }

    fn tried(&self, round: u64, player: usize) -> u32 {
        self.tries.get(&(round, player)).copied().unwrap_or(0)
    }

    fn bump_tries(&mut self, round: u64, player: usize) {
        *self.tries.entry((round, player)).or_insert(0) += 1;
    }

    /// This round's whale: a Simple position of at least 4 SOL that has not
    /// matured away.
    fn big_position(&self, round: u64) -> Option<PositionRec> {
        self.positions
            .iter()
            .rev()
            .find(|position| {
                position.round == round
                    && position.plan == PlanKindArg::Simple
                    && position.gross >= 4 * SOL
                    && !self.spent_nonces.contains(&position.nonce)
            })
            .copied()
    }

    fn plan_open(&mut self, player: usize, kind: PlanKindArg, gross: u64, stale: bool) -> Plan {
        let nonce = self.next_nonce;
        self.next_nonce += 1;
        let pool = self.w.pool();
        let owner = self.pk(player);
        let expected_round = if stale { pool.round_id + 1 } else { pool.round_id };
        let ix = self.w.open_instruction(owner, kind, gross, nonce, expected_round, pool.active_config_version);
        let mut plan = Plan::new(Fam::Open, format!("open_p{player}_n{nonce}"), ix).signed(&[player]);
        if !stale {
            plan.track = Some(PositionRec { player, nonce, round: pool.round_id, plan: kind, gross });
        }
        plan
    }

    fn plan_fund_series_a(&mut self, backer: usize, gross: u64, stale: bool) -> Plan {
        let pool = self.w.pool();
        let expected_round = if stale { pool.round_id + 1 } else { pool.round_id };
        let ix = self.w.fund_series_a_instruction(self.pk(backer), gross, expected_round);
        Plan::new(Fam::FundSeriesA, format!("fund_series_a_b{}", backer - BACKER0), ix).signed(&[backer])
    }

    fn plan_participant_page(&self, round: u64, index: u64) -> Plan {
        let rent = self.w.rt.rent(state::ParticipantPage::SPACE);
        let (funding, fam) = if self.w.ops().free_balance >= rent {
            (Funding::Free, Fam::ProvisionFree)
        } else {
            (Funding::Caller, Fam::ProvisionCaller)
        };
        let ix = provision_kind_ix(&self.w, PageKind::Participant { round }, index, funding, self.payer());
        Plan::new(fam, format!("participant_page_r{round}_{index}"), ix)
    }

    /// The next allocation-ticket instruction, if a ticket is active.
    fn plan_ticket_step(&mut self) -> Option<Plan> {
        let pool = self.w.pool();
        let ticket = self.w.ticket();
        if pool.active_ticket == 0 || ticket.phase == state::IDLE {
            return None;
        }
        let rows = if self.step.is_multiple_of(7) { 2 } else { 8 };
        let step = self.w.rt.next_phase_step(rows, false);
        let fam = match step.name {
            "discover" => Fam::Discover,
            "senior" => Fam::Senior,
            "seal" => Fam::Seal,
            "allocate" => Fam::Allocate,
            "prune" => Fam::Prune,
            _ => Fam::Commit,
        };
        Some(Plan::new(fam, format!("{}_t{}_{}", step.name, ticket.id, step.cursor), step.instruction))
    }

    /// A Backer claim for a Backer that has finalized credit, if any.
    fn plan_claim_for_credit(&self) -> Option<Plan> {
        for backer in [BACKER0, BACKER0 + 1] {
            let owner = self.pk(backer);
            let key = self.w.rt.backer_address(SERIES_A, owner);
            if self.w.program_account(key).is_none() {
                continue;
            }
            let record = self.w.backer_at(key);
            if record.pending + record.funded == 0 {
                continue;
            }
            let b_key = self.w.program_account(self.w.rt.backer_address(SERIES_B, owner));
            let ix = self.w.rt.claim_backer_ix(owner, Some(key), b_key, Pubkey::new_from_array([0xE1; 32]));
            return Some(Plan::new(Fam::ClaimBacker, format!("claim_b{}", backer - BACKER0), ix).signed(&[backer]));
        }
        None
    }

    /// Deliver any owed (deferred) owner refund; refill an empty destination in
    /// the same transaction when it could not take the credit.
    fn plan_deliver_owed(&mut self) -> Option<Plan> {
        let pool = self.w.pool();
        for close_id in 1..pool.next_close_id {
            let Some(record) = self
                .w
                .rt
                .try_deserialize::<state::CloseRecord>(self.w.rt.close_record_address(close_id))
            else {
                continue;
            };
            let tries = self.deliver_tries.get(&close_id).copied().unwrap_or(0);
            if !(record.reserve_settled && record.locked_owner_remaining > 0) || tries >= 3 {
                continue;
            }
            self.deliver_tries.insert(close_id, tries + 1);
            let destination = record.owner_refund_destination;
            let minimum = self.w.rt.rent(0);
            let deliver = deliver_ix(&self.w, close_id, destination);
            let plan = if self.w.lamports(destination) + record.locked_owner_remaining >= minimum {
                Plan::new(Fam::Deliver, format!("deliver_c{close_id}"), deliver)
            } else {
                let refill = transfer_ix(self.pk(OUTSIDER), destination, minimum);
                let mut plan = Plan::new(Fam::Deliver, format!("refill_and_deliver_c{close_id}"), refill);
                plan.ixs.push(deliver);
                plan.signed(&[OUTSIDER])
            };
            return Some(plan);
        }
        None
    }

    /// The transaction that settles close `id`: a deferral close also empties
    /// its destination `D0` in the SAME transaction (so no step can refill it
    /// in between), which makes the sub-minimum owner remainder undeliverable.
    fn settle_plan(&mut self, id: u64, fam: Fam, name: String, settle: Instruction) -> Plan {
        let record = self.w.close_record(id);
        let d0 = self.pk(D0);
        if profile(id).deferral && record.owner_refund_destination == d0 && self.w.lamports(d0) > 0 {
            let drain = transfer_ix(d0, self.pk(OUTSIDER), self.w.lamports(d0));
            let mut plan = Plan::new(fam, format!("{name}_draining_d0"), drain);
            plan.ixs.push(settle);
            return plan.signed(&[D0]);
        }
        Plan::new(fam, name, settle)
    }

    fn advance(&mut self) {
        let pool = self.w.pool();
        let action = match pool.current_close_id {
            Some(id) => self.engine_closing(id),
            None => self.engine_active(),
        };
        match self.run_action(action) {
            Some(false) => {
                self.engine_failures += 1;
                self.engine_rejections += 1;
            }
            _ => self.engine_failures = 0,
        }
    }

    /// The next move of an open round: the allocation ticket if one is
    /// running, the round's first page, a Backer, the whale and four more
    /// positions, then the preparation and the insolvent closing payout.
    fn engine_active(&mut self) -> Action {
        let pool = self.w.pool();
        let round = pool.round_id;
        if let Some(plan) = self.plan_deliver_owed() {
            return Action::Tx(plan);
        }
        if pool.active_ticket != 0 {
            return self.plan_ticket_step().map_or(Action::Nothing, Action::Tx);
        }
        if pool.participant_pages == 0 {
            return Action::Tx(self.plan_participant_page(round, 0));
        }
        if pool.registry_len == 0 {
            let backer = BACKER0 + self.step % 2;
            let gross = self.rng.range(150_000_000, 400_000_000);
            return Action::Tx(self.plan_fund_series_a(backer, gross, false));
        }
        let whale = self.whale;
        if self.big_position(round).is_none() {
            if self.tried(round, whale) >= 3 {
                self.whale = (whale + 1) % PLAYERS;
                return Action::Nothing;
            }
            self.bump_tries(round, whale);
            // A whale that matured away is replaced by a bigger one: the pot (which still holds
            // the old whale's deposit) must stay under 0.11 * gross * 19 days.
            let spent = self.positions.iter().filter(|p| p.round == round && self.spent_nonces.contains(&p.nonce)).count() as u64;
            let gross = 8 * SOL * (1 + spent.min(3));
            return Action::Tx(self.plan_open(whale, PlanKindArg::Simple, gross, false));
        }
        if pool.participant_count < ROUND_PARTICIPANTS {
            let next = (0..PLAYERS)
                .find(|&player| player != whale && self.deposits(player, round) == 0 && self.tried(round, player) < 2);
            if let Some(player) = next {
                self.bump_tries(round, player);
                let gross = self.rng.range(150_000_000, 300_000_000);
                return Action::Tx(self.plan_open(player, PlanKindArg::Simple, gross, false));
            }
        }
        self.engine_prepare_close(round)
    }

    /// Wait for the whale to out-accrue the pot, provision what the closing
    /// payout needs, engineer the reserve split, then take the insolvent payout.
    fn engine_prepare_close(&mut self, round: u64) -> Action {
        let pool = self.w.pool();
        let ops = self.w.ops();
        let Some(big) = self.big_position(round) else {
            return Action::Nothing;
        };
        let owner = self.pk(big.player);
        let position = self.w.position(owner, big.nonce);
        if position.claimed || self.w.now() >= position.maturity_at {
            self.spent_nonces.insert(big.nonce);
            return Action::Nothing;
        }
        let principal = u128::from(position.principal);
        let elapsed = self.w.now().saturating_sub(position.last_accrued_at);
        // owed(t) = principal * 1100 bps * t / day  >  pot  <=>  t > pot * day * 10_000 / (1_100 * principal)
        let needed = (u128::from(pool.pot) * 86_400 * 10_000 / (1_100 * principal)) as u64 + 3_600;
        if elapsed < needed {
            let wait = needed - elapsed + self.rng.range(0, 7_200);
            return Action::Warp(wait, "the whale out-accrues the pot");
        }
        let id = pool.next_close_id;
        let record_key = self.w.rt.close_record_address(id);
        if self.w.rt.try_deserialize::<state::CloseRecord>(record_key).is_none() {
            let funding = if ops.free_balance >= self.w.rt.rent(state::CloseRecord::SPACE) {
                Funding::Free
            } else {
                Funding::Caller
            };
            let ix = provision_close_record_ix(&self.w, id, funding, self.payer());
            return Action::Tx(Plan::new(Fam::ProvisionCloseRecord, format!("close_record{id}"), ix));
        }
        let page = state::PAGE_CAPACITY as u64;
        if pool.snapshot_pages * page < pool.registry_len {
            let ix = provision_kind_ix(&self.w, PageKind::Snapshot, pool.snapshot_pages, Funding::Caller, self.payer());
            return Action::Tx(Plan::new(Fam::ProvisionCaller, format!("snapshot_page{}", pool.snapshot_pages), ix));
        }
        let record = self.w.close_record(id);
        let reserve = self.w.expected_reserve(
            pool.participant_count,
            pool.registry_len,
            record.existing_preferred_pages,
            record.existing_fallback_pages,
        );
        let prof = profile(id);
        if prof.deferral {
            let target = reserve - OWNER_PART;
            let free = ops.free_balance;
            if free < target {
                let ix = fund_ops_ix(&self.w, self.pk(OUTSIDER), target - free);
                return Action::Tx(Plan::new(Fam::FundOps, "fund_to_target", ix).signed(&[OUTSIDER]));
            }
            if free > target {
                let ix = withdraw_ops_ix(&self.w, self.pk(SPONSOR), free - target);
                return Action::Tx(Plan::new(Fam::WithdrawOps, "withdraw_to_target", ix).signed(&[SPONSOR]));
            }
        }
        let dest = if prof.deferral { self.pk(D0) } else { self.w.dest };
        let cap = if prof.deferral { OWNER_PART } else { reserve };
        let ix = self.w.payout_instruction(
            PayoutKind::WithdrawSimple,
            owner,
            owner,
            big.nonce,
            0,
            cap,
            dest,
            CloseRecordArg::Auto,
        );
        Action::Tx(
            Plan::new(Fam::WithdrawSimple, format!("payout_p{}_n{}_c{id}", big.player, big.nonce), ix)
                .signed(&[big.player]),
        )
    }

    /// The next move of the close in progress.
    fn engine_closing(&mut self, id: u64) -> Action {
        let close = self.w.close_record(id);
        let prof = profile(id);
        let now = self.w.now();
        let deadline = close.promotion_deadline;
        let pending = close.promotion == state::Promotion::Pending;

        if pending && self.engine_failures >= 5 && now <= deadline && close.draw_state != state::DRAW_CAPTURED {
            self.engine_failures = 0;
            return Action::Warp(deadline + 1 - now, "unstick: let the deadline pass");
        }
        if !pending {
            if !close.cash_committed {
                return self.plan_ticket_step().map_or(Action::Nothing, Action::Tx);
            }
            if !close.finalized {
                let ix = finalize_ix(&self.w, id);
                return Action::Tx(Plan::new(Fam::Finalize, format!("finalize_c{id}"), ix));
            }
            return Action::Nothing;
        }
        if !close.cash_committed && close.closing_ticket_id.is_some() {
            return self.plan_ticket_step().map_or(Action::Nothing, Action::Tx);
        }
        if close.closing_ticket_id.is_some() && !self.claimed_close.contains(&id) {
            self.claimed_close.insert(id);
            if let Some(plan) = self.plan_claim_for_credit() {
                return Action::Tx(plan);
            }
        }
        if !close.sealed {
            if now > deadline {
                let ix = expire_ix(&self.w, id);
                return Action::Tx(self.settle_plan(id, Fam::Expire, format!("expire_c{id}"), ix));
            }
            let expected = expected_candidates(&self.w, id);
            let (need_preferred, need_fallback) =
                (pages_of(expected.preferred.len()), pages_of(expected.fallback.len()));
            if close.provisioned_preferred_pages < need_preferred {
                let index = close.provisioned_preferred_pages;
                let ix = provision_ix(&self.w, id, 0, index, Funding::LockedClose);
                return Action::Tx(Plan::new(Fam::ProvisionLocked, format!("candidate_preferred{index}_c{id}"), ix));
            }
            if close.provisioned_fallback_pages < need_fallback {
                let index = close.provisioned_fallback_pages;
                let ix = provision_ix(&self.w, id, 1, index, Funding::LockedClose);
                return Action::Tx(Plan::new(Fam::ProvisionLocked, format!("candidate_fallback{index}_c{id}"), ix));
            }
            let remaining = close.participant_count - close.candidate_cursor;
            let rows = remaining.min(4) as u8;
            let ix = build_next(&self.w, id, rows);
            let name = format!("build{}_c{id}", close.candidate_cursor);
            let sealing_empty = u64::from(rows) == remaining && expected.preferred.is_empty() && expected.fallback.is_empty();
            return Action::Tx(if sealing_empty {
                self.settle_plan(id, Fam::Build, name, ix)
            } else {
                Plan::new(Fam::Build, name, ix)
            });
        }
        match close.draw_state {
            state::DRAW_NOT_REQUESTED => {
                if !prof.full_path || now > deadline {
                    if now <= deadline {
                        return Action::Warp(deadline + 1 - now, "the deadline passes unrequested");
                    }
                    let ix = expire_ix(&self.w, id);
                    return Action::Tx(self.settle_plan(id, Fam::Expire, format!("expire_c{id}"), ix));
                }
                let (request, _) = host_request(&self.w, id);
                self.w.watch(request);
                let ix = request_ix(&self.w, id);
                Action::Tx(Plan::new(Fam::Request, format!("request_c{id}"), ix))
            }
            state::DRAW_REQUESTED | state::DRAW_IGNORED_LATE => {
                let fulfilled = self.w.request_fulfilled(id);
                if close.draw_state == state::DRAW_IGNORED_LATE {
                    let ix = expire_ix(&self.w, id);
                    return Action::Tx(self.settle_plan(id, Fam::Expire, format!("expire_c{id}"), ix));
                }
                if !fulfilled {
                    if now > deadline {
                        let ix = expire_ix(&self.w, id);
                        return Action::Tx(self.settle_plan(id, Fam::Expire, format!("expire_c{id}"), ix));
                    }
                    return Action::Fulfill(id);
                }
                if !close.reimbursement_observed {
                    let ix = observe_ix(&self.w, id);
                    return Action::Tx(Plan::new(Fam::Observe, format!("observe_c{id}"), ix));
                }
                if close.eligible_return_remaining > 0 && client_spendable(&self.w) >= close.eligible_return_remaining {
                    let ix = withdraw_ix(&self.w, id, close.eligible_return_remaining);
                    return Action::Tx(Plan::new(Fam::ReturnG, format!("return_c{id}"), ix).via_g());
                }
                if now <= deadline {
                    let ix = capture_ix(&self.w, id);
                    return Action::Tx(Plan::new(Fam::Capture, format!("capture_c{id}"), ix));
                }
                let ix = expire_ix(&self.w, id);
                Action::Tx(self.settle_plan(id, Fam::Expire, format!("expire_c{id}"), ix))
            }
            state::DRAW_CAPTURED => {
                // A capture can land before the observation (a perturbation or a sweep may
                // capture first): observe and return the reimbursement before the promotion
                // settles, both are legal on a captured, pending close.
                if !close.reimbursement_observed {
                    let ix = observe_ix(&self.w, id);
                    return Action::Tx(Plan::new(Fam::Observe, format!("observe_c{id}"), ix));
                }
                if close.eligible_return_remaining > 0 && client_spendable(&self.w) >= close.eligible_return_remaining {
                    let ix = withdraw_ix(&self.w, id, close.eligible_return_remaining);
                    return Action::Tx(Plan::new(Fam::ReturnG, format!("return_c{id}"), ix).via_g());
                }
                let ix = apply_ix(&self.w, id);
                Action::Tx(self.settle_plan(id, Fam::Apply, format!("apply_c{id}"), ix))
            }
            _ => Action::Nothing,
        }
    }
}

// ---------------------------------------------------------------------------
// Perturbations: any family, at any moment
// ---------------------------------------------------------------------------

impl Fuzz {
    /// The closes a perturbation may name: the one in progress, else the
    /// latest bound one.
    fn target_close(&self) -> Option<u64> {
        let pool = self.w.pool();
        pool.current_close_id.or_else(|| pool.next_close_id.checked_sub(1).filter(|id| *id >= 1))
    }

    fn warp_action(&mut self, big: bool) -> Action {
        let pool = self.w.pool();
        let now = self.w.now();
        if big {
            if pool.current_close_id.is_some() || pool.active_ticket != 0 {
                return Action::Nothing;
            }
            return Action::Warp(self.rng.range(3_600, 172_800), "idle days");
        }
        match pool.current_close_id {
            Some(id) => {
                let close = self.w.close_record(id);
                if close.promotion == state::Promotion::Pending {
                    let room = close.promotion_deadline.saturating_sub(now).saturating_sub(200);
                    if room >= 5 {
                        Action::Warp(self.rng.range(1, room.min(20)), "seconds inside the window")
                    } else {
                        Action::Nothing
                    }
                } else {
                    Action::Warp(self.rng.range(1, 300), "seconds after the promotion")
                }
            }
            None => Action::Warp(self.rng.range(30, 7_200), "minutes"),
        }
    }

    fn anonymous_credit(&mut self) -> Action {
        let amount = self.rng.range(1_000, 2_000_000);
        let credit = transfer_ix(self.pk(OUTSIDER), self.w.rt.ops_cash_address(), amount);
        let mut plan = Plan::new(Fam::Reconcile, "anonymous_credit_and_reconcile", credit);
        plan.ixs.push(reconcile_ix(&self.w.rt));
        Action::Tx(plan.signed(&[OUTSIDER]))
    }

    /// The owner's destination `D0` emptied or refilled by a real transfer.
    fn destination_action(&mut self) -> Action {
        let d0 = self.pk(D0);
        let lamports = self.w.lamports(d0);
        if lamports > 0 && self.rng.chance(50) {
            let ix = transfer_ix(d0, self.pk(OUTSIDER), lamports);
            Action::Tx(Plan::new(Fam::Aux, "empty_destination_d0", ix).signed(&[D0]))
        } else {
            // At least the rent-exempt minimum, so the refill is a transfer the cluster accepts too.
            let amount = self.rng.range(self.w.rt.rent(0), 3_000_000);
            let ix = transfer_ix(self.pk(OUTSIDER), d0, amount);
            Action::Tx(Plan::new(Fam::Aux, "refill_destination_d0", ix).signed(&[OUTSIDER]))
        }
    }

    fn perturb(&mut self) {
        let roll = self.rng.range(0, 99);
        let action = match roll {
            0..=9 => self.warp_action(false),
            10..=13 => self.warp_action(true),
            14..=21 => self.build_action(Fam::FundOps),
            22..=27 => self.build_action(Fam::WithdrawOps),
            28..=33 => self.build_action(Fam::Reconcile),
            34..=37 => self.build_action(Fam::ReconcileClient),
            38..=41 => self.anonymous_credit(),
            42..=49 => self.build_action(Fam::ClaimBacker),
            50..=57 => self.build_action(Fam::WithdrawSimple),
            58..=63 => self.build_action(Fam::Open),
            64..=67 => self.build_action(Fam::FundSeriesA),
            68..=75 => {
                let fam = [Fam::ProvisionFree, Fam::ProvisionCaller, Fam::ProvisionLocked][self.rng.below(3)];
                self.build_action(fam)
            }
            76..=79 => self.build_action(Fam::ProvisionCloseRecord),
            80..=83 => self.build_action(Fam::Budget),
            84..=87 => {
                let fam = [Fam::Observe, Fam::ReturnG, Fam::Deliver][self.rng.below(3)];
                self.build_action(fam)
            }
            88..=93 => self.build_action(Fam::CloseShared),
            94..=96 => {
                let fam = [Fam::SettleCompounding, Fam::CloseSimple][self.rng.below(2)];
                self.build_action(fam)
            }
            _ => self.destination_action(),
        };
        self.run_action(action);
    }

    /// A transaction of family `fam`: valid when the chain state allows it,
    /// otherwise a well-formed one the program must refuse.  `Nothing` only
    /// when no instruction of the family can even be addressed yet.
    fn build_action(&mut self, fam: Fam) -> Action {
        let pool = self.w.pool();
        let ops = self.w.ops();
        let round = pool.round_id;
        let current = pool.current_close_id;
        let target = self.target_close();
        let attacker = 1usize;
        match fam {
            Fam::Open => {
                let player = self.rng.below(PLAYERS);
                let kind = [PlanKindArg::Simple, PlanKindArg::Compound15, PlanKindArg::Compound30][self.rng.below(3)];
                let gross = self.rng.range(20_000_000, 250_000_000);
                Action::Tx(self.plan_open(player, kind, gross, false))
            }
            Fam::FundSeriesA => {
                let backer = BACKER0 + self.rng.below(2);
                let gross = self.rng.range(50_000_000, 200_000_000);
                Action::Tx(self.plan_fund_series_a(backer, gross, false))
            }
            Fam::WithdrawSimple => {
                let whale = self.whale;
                let choice = self
                    .positions
                    .iter()
                    .rev()
                    .find(|p| p.round == round && p.plan == PlanKindArg::Simple && p.player != whale && p.gross < 4 * SOL)
                    .copied();
                let (player, nonce) = choice.map_or((attacker, 9_999), |p| (p.player, p.nonce));
                let owner = self.pk(player);
                let ix = self.w.payout_instruction(
                    PayoutKind::WithdrawSimple,
                    owner,
                    owner,
                    nonce,
                    0,
                    0,
                    self.w.dest,
                    CloseRecordArg::Auto,
                );
                Action::Tx(Plan::new(Fam::WithdrawSimple, format!("withdraw_p{player}_n{nonce}"), ix).signed(&[player]))
            }
            Fam::SettleCompounding => {
                let choice = self.positions.iter().rev().find(|p| p.round == round && p.plan != PlanKindArg::Simple).copied();
                let (player, nonce) = choice.map_or((attacker, 9_998), |p| (p.player, p.nonce));
                let owner = self.pk(player);
                let ix = self.w.payout_instruction(
                    PayoutKind::SettleCompounding,
                    owner,
                    owner,
                    nonce,
                    0,
                    0,
                    self.w.dest,
                    CloseRecordArg::Auto,
                );
                Action::Tx(Plan::new(Fam::SettleCompounding, format!("settle_p{player}_n{nonce}"), ix).signed(&[player]))
            }
            Fam::CloseSimple => {
                let choice = self.positions.iter().rev().find(|p| p.round == round && p.plan == PlanKindArg::Simple).copied();
                let (player, nonce) = choice.map_or((attacker, 9_997), |p| (p.player, p.nonce));
                let ix = self.w.close_simple_instruction(self.pk(player), nonce);
                Action::Tx(Plan::new(Fam::CloseSimple, format!("close_simple_p{player}_n{nonce}"), ix).signed(&[player]))
            }
            Fam::Discover | Fam::Senior | Fam::Seal | Fam::Allocate | Fam::Prune | Fam::Commit => {
                Action::Tx(self.ticket_family(fam))
            }
            Fam::ClaimBacker => {
                let actor = if self.rng.chance(80) { BACKER0 + self.rng.below(2) } else { self.rng.below(PLAYERS) };
                let owner = self.pk(actor);
                let a = self.w.program_account(self.w.rt.backer_address(SERIES_A, owner));
                let b = self.w.program_account(self.w.rt.backer_address(SERIES_B, owner));
                let ix = self.w.rt.claim_backer_ix(owner, a, b, Pubkey::new_from_array([0xE1; 32]));
                Action::Tx(Plan::new(Fam::ClaimBacker, format!("claim_p{actor}"), ix).signed(&[actor]))
            }
            Fam::ProvisionFree | Fam::ProvisionCaller => {
                let funding = if fam == Fam::ProvisionFree { Funding::Free } else { Funding::Caller };
                let next = pool.next_close_id;
                let (kind, index) = match self.rng.below(4) {
                    0 => (PageKind::Participant { round }, pool.participant_pages),
                    1 => (PageKind::Registry, pool.registry_pages),
                    2 => (PageKind::Snapshot, pool.snapshot_pages),
                    _ => {
                        let set = self.rng.below(2) as u8;
                        let record = self
                            .w
                            .rt
                            .try_deserialize::<state::CloseRecord>(self.w.rt.close_record_address(next));
                        let index = record.map_or(0, |record| {
                            if set == 0 { record.provisioned_preferred_pages } else { record.provisioned_fallback_pages }
                        });
                        (PageKind::Candidate { close_id: next, set_tag: set }, index)
                    }
                };
                let ix = provision_kind_ix(&self.w, kind, index, funding, self.payer());
                Action::Tx(Plan::new(fam, format!("provision_{kind:?}_{index}").replace(' ', ""), ix))
            }
            Fam::ProvisionLocked => {
                let (close_id, set, index) = match current {
                    Some(id) => {
                        let close = self.w.close_record(id);
                        let set = self.rng.below(2) as u8;
                        let index = if set == 0 { close.provisioned_preferred_pages } else { close.provisioned_fallback_pages };
                        (id, set, index)
                    }
                    None => (pool.next_close_id, 0, 0),
                };
                let ix = provision_ix(&self.w, close_id, set, index, Funding::LockedClose);
                Action::Tx(Plan::new(Fam::ProvisionLocked, format!("locked_c{close_id}_s{set}_{index}"), ix))
            }
            Fam::ProvisionCloseRecord => {
                let id = pool.next_close_id;
                let funding = if ops.free_balance >= self.w.rt.rent(state::CloseRecord::SPACE) {
                    Funding::Free
                } else {
                    Funding::Caller
                };
                let ix = provision_close_record_ix(&self.w, id, funding, self.payer());
                Action::Tx(Plan::new(Fam::ProvisionCloseRecord, format!("close_record{id}"), ix))
            }
            Fam::Build => {
                let Some(id) = target else { return Action::Nothing };
                let close = self.w.close_record(id);
                let remaining = close.participant_count.saturating_sub(close.candidate_cursor);
                let mut sealing_empty = false;
                let ix = if current == Some(id) && !close.sealed && remaining > 0 {
                    let rows = self.rng.range(1, remaining.min(4)) as u8;
                    let expected = expected_candidates(&self.w, id);
                    sealing_empty =
                        u64::from(rows) == remaining && expected.preferred.is_empty() && expected.fallback.is_empty();
                    build_next(&self.w, id, rows)
                } else {
                    build_ix(&self.w, id, 1, Vec::new())
                };
                let name = format!("build_c{id}");
                // Sealing an empty candidate set settles the reserve: a deferral close empties D0 in that transaction.
                if sealing_empty {
                    return Action::Tx(self.settle_plan(id, Fam::Build, name, ix));
                }
                Action::Tx(Plan::new(Fam::Build, name, ix))
            }
            Fam::Request | Fam::Capture | Fam::Expire | Fam::Finalize | Fam::Observe => {
                let Some(id) = target else { return Action::Nothing };
                if fam == Fam::Request && current == Some(id) && !profile(id).full_path {
                    let close = self.w.close_record(id);
                    if close.sealed && close.draw_state == state::DRAW_NOT_REQUESTED {
                        // This close's profile is expiry: a valid request would turn it into a draw.
                        return Action::Nothing;
                    }
                }
                let ix = match fam {
                    Fam::Request => request_ix(&self.w, id),
                    Fam::Capture => capture_ix(&self.w, id),
                    Fam::Expire => expire_ix(&self.w, id),
                    Fam::Finalize => finalize_ix(&self.w, id),
                    _ => observe_ix(&self.w, id),
                };
                let name = format!("{}_c{id}", fam.name());
                // A settling transaction of a deferral close always empties D0 in the same transaction.
                if fam == Fam::Expire {
                    return Action::Tx(self.settle_plan(id, fam, name, ix));
                }
                Action::Tx(Plan::new(fam, name, ix))
            }
            Fam::Apply => {
                let Some(id) = target else { return Action::Nothing };
                let close = self.w.close_record(id);
                // The winner's accounts are read from candidate pages that a terminal close's
                // cleanup may already have closed: build them only for a pending close.
                let live = close.promotion == state::Promotion::Pending;
                let ix = if live && close.draw_state == state::DRAW_CAPTURED {
                    apply_ix(&self.w, id)
                } else {
                    apply_ix_with(&self.w, id, ApplyKeys::default())
                };
                Action::Tx(self.settle_plan(id, Fam::Apply, format!("apply_c{id}"), ix))
            }
            Fam::Budget => {
                let Some(id) = target else { return Action::Nothing };
                let extra = if ops.free_balance > 0 { self.rng.range(1, ops.free_balance.min(500_000)) } else { 1 };
                let ix = budget_ix(&self.w, id, extra);
                Action::Tx(Plan::new(Fam::Budget, format!("budget_c{id}_{extra}"), ix))
            }
            Fam::ReturnG => {
                let Some(id) = target else { return Action::Nothing };
                let close = self.w.close_record(id);
                let amount = close.eligible_return_remaining.max(1);
                let ix = withdraw_ix(&self.w, id, amount);
                Action::Tx(Plan::new(Fam::ReturnG, format!("return_c{id}_{amount}"), ix).via_g())
            }
            Fam::Deliver => {
                let Some(id) = target else { return Action::Nothing };
                let destination = self.w.close_record(id).owner_refund_destination;
                let ix = deliver_ix(&self.w, id, destination);
                Action::Tx(Plan::new(Fam::Deliver, format!("deliver_c{id}"), ix))
            }
            Fam::Fulfill => match target {
                Some(id) => Action::Fulfill(id),
                None => Action::Nothing,
            },
            Fam::Reconcile => {
                let ix = reconcile_ix(&self.w.rt);
                Action::Tx(Plan::new(Fam::Reconcile, "reconcile", ix))
            }
            Fam::ReconcileClient => {
                let ix = reconcile_client_ix(&self.w.rt);
                Action::Tx(Plan::new(Fam::ReconcileClient, "reconcile_with_client", ix))
            }
            Fam::CloseShared => self.close_shared_action(),
            Fam::FundOps => {
                let sender = [OUTSIDER, attacker][self.rng.below(2)];
                let amount = self.rng.range(10_000, 3_000_000);
                let ix = fund_ops_ix(&self.w, self.pk(sender), amount);
                Action::Tx(Plan::new(Fam::FundOps, format!("fund_{amount}"), ix).signed(&[sender]))
            }
            Fam::WithdrawOps => {
                let amount = if ops.free_balance > 0 && self.rng.chance(85) {
                    self.rng.range(1, ops.free_balance)
                } else {
                    ops.free_balance + 1
                };
                let ix = withdraw_ops_ix(&self.w, self.pk(SPONSOR), amount);
                Action::Tx(Plan::new(Fam::WithdrawOps, format!("withdraw_{amount}"), ix).signed(&[SPONSOR]))
            }
            Fam::Aux => Action::Nothing,
        }
    }

    /// A ticket-family transaction: the real next step when the ticket's phase
    /// is this family, otherwise a well-formed out-of-phase one.
    fn ticket_family(&mut self, fam: Fam) -> Plan {
        let pool = self.w.pool();
        let ticket = self.w.ticket();
        let active = pool.active_ticket != 0 && ticket.phase != state::IDLE;
        let short = match fam {
            Fam::Discover => "discover",
            Fam::Senior => "senior",
            Fam::Seal => "seal",
            Fam::Allocate => "allocate",
            Fam::Prune => "prune",
            _ => "commit",
        };
        if active {
            let step = self.w.rt.next_phase_step(8, false);
            if step.name == short {
                return Plan::new(fam, format!("{short}_t{}_{}", ticket.id, step.cursor), step.instruction);
            }
        }
        let id = if active { ticket.id } else { 0 };
        let rt = &self.w.rt;
        let ix = match fam {
            Fam::Discover => rt.discover_ix(id, 0, 1, Vec::new()),
            Fam::Senior => rt.senior_ix(id, ID),
            Fam::Seal => rt.seal_ix(id),
            Fam::Allocate => rt.allocate_ix(id, 0, 1, Vec::new()),
            Fam::Prune => rt.prune_ix(id, 0, 1, Vec::new()),
            _ => rt.commit_ix(id, None),
        };
        Plan::new(fam, format!("{short}_out_of_phase_t{id}"), ix)
    }

    /// `close_shared_account` on a page that IS closable when one exists
    /// (participant pages of finalized rounds, candidate pages of terminal
    /// closes), else on a live page (refused).
    fn close_shared_action(&mut self) -> Action {
        let pool = self.w.pool();
        let attacker = self.pk(1);
        let mut options: Vec<(PageKind, Pubkey, Pubkey, u64)> = Vec::new();
        for close_id in 1..pool.next_close_id {
            let Some(record) = self
                .w
                .rt
                .try_deserialize::<state::CloseRecord>(self.w.rt.close_record_address(close_id))
            else {
                continue;
            };
            if record.finalized {
                for index in 0..3u64 {
                    let key = self.w.rt.participant_address(record.round_id, index);
                    if self.w.program_account(key).is_some() {
                        let rent = self.w.participants(record.round_id, index).rent;
                        let refund = if rent.owner > 0 { rent.owner_refund } else { attacker };
                        options.push((PageKind::Participant { round: record.round_id }, key, refund, close_id));
                    }
                }
            }
            if record.promotion != state::Promotion::Pending {
                for set in 0..=1u8 {
                    for index in 0..3u64 {
                        let key = candidate_page_key(close_id, set, index);
                        if self.w.program_account(key).is_some() {
                            let rent = candidate_page(&self.w, close_id, set, index).rent;
                            let refund = if rent.owner > 0 { rent.owner_refund } else { attacker };
                            options.push((PageKind::Candidate { close_id, set_tag: set }, key, refund, close_id));
                        }
                    }
                }
            }
        }
        if options.is_empty() {
            // A live page, judged by the earliest close record there is.
            let Some(close_id) = self.target_close() else { return Action::Nothing };
            let kind = PageKind::Participant { round: pool.round_id };
            let key = self.w.rt.participant_address(pool.round_id, 0);
            options.push((kind, key, attacker, close_id));
        }
        let (kind, key, refund, close_id) = options[self.rng.below(options.len())];
        let ix = close_page_ix(&self.w, kind, key, refund, close_id);
        Action::Tx(Plan::new(Fam::CloseShared, format!("close_page_c{close_id}"), ix))
    }

    /// The coverage sweep: attempt the least-attempted family (valid when the
    /// state allows it, refused otherwise).
    fn sweep(&mut self) {
        let mut order: Vec<Fam> = ALL_FAMILIES.to_vec();
        order.sort_by_key(|fam| self.tally.get(fam).map_or(0, |entry| entry.0));
        for fam in order {
            let action = self.build_action(fam);
            if !matches!(action, Action::Nothing) {
                self.run_action(action);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Adversaries: transactions that MUST be refused
// ---------------------------------------------------------------------------

impl Fuzz {
    fn adversary(&mut self) {
        self.adversary_draws += 1;
        let kinds = 15usize;
        let start = self.rng.below(kinds);
        for offset in 0..kinds {
            if let Some(plan) = self.adversary_plan((start + offset) % kinds) {
                self.exec(plan);
                return;
            }
        }
    }

    fn adversary_plan(&mut self, kind: usize) -> Option<Plan> {
        let pool = self.w.pool();
        let ops = self.w.ops();
        let round = pool.round_id;
        let attacker = 1usize;
        let attacker_key = self.pk(attacker);
        let current = pool.current_close_id;
        let latest = pool.next_close_id.checked_sub(1).filter(|id| *id >= 1);
        let now = self.w.now();
        match kind {
            // The attacker is not the sponsor.
            0 => {
                let ix = withdraw_ops_ix(&self.w, attacker_key, 1);
                Some(Plan::new(Fam::WithdrawOps, "adv_wrong_signer_withdraw_ops", ix)
                    .signed(&[attacker])
                    .expecting(Expect::Code(ErrorCode::NotAuthorized)))
            }
            // A stale expected round: Closing while a close runs, StaleRound otherwise.
            1 => {
                let player = self.rng.below(PLAYERS);
                let plan = self.plan_open(player, PlanKindArg::Simple, 50_000_000, true);
                let expected = if current.is_some() { ErrorCode::Closing } else { ErrorCode::StaleRound };
                Some(plan.expecting(Expect::Code(expected)))
            }
            // A provider return signed by someone who is not the G vault.
            2 => {
                let id = latest?;
                let mut accounts = withdraw_accounts(&self.w, id);
                accounts.governance = attacker_key;
                let ix = anchor_ix(accounts, instruction::WithdrawProviderReturn { amount: 1 });
                Some(Plan::new(Fam::ReturnG, "adv_return_not_signed_by_g", ix)
                    .signed(&[attacker])
                    .expecting(Expect::Code(ErrorCode::NotAuthorized)))
            }
            // Expiry before the deadline.
            3 => {
                let id = current?;
                let close = self.w.close_record(id);
                if close.promotion != state::Promotion::Pending || now > close.promotion_deadline {
                    return None;
                }
                let ix = expire_ix(&self.w, id);
                Some(Plan::new(Fam::Expire, "adv_expire_before_deadline", ix)
                    .expecting(Expect::Code(ErrorCode::DeadlineNotReached)))
            }
            // Applying a draw nobody captured.
            4 => {
                let id = current?;
                let close = self.w.close_record(id);
                if close.promotion != state::Promotion::Pending || close.draw_state == state::DRAW_CAPTURED {
                    return None;
                }
                let ix = apply_ix_with(&self.w, id, ApplyKeys::default());
                Some(Plan::new(Fam::Apply, "adv_apply_without_capture", ix)
                    .expecting(Expect::Code(ErrorCode::InvalidState)))
            }
            // Requesting a draw before the sets are sealed.
            5 => {
                let id = current?;
                let close = self.w.close_record(id);
                if close.promotion != state::Promotion::Pending || close.sealed {
                    return None;
                }
                let ix = request_ix(&self.w, id);
                Some(Plan::new(Fam::Request, "adv_request_before_sealed", ix)
                    .expecting(Expect::Code(ErrorCode::NotSealed)))
            }
            // A deposit into a round that is closing.
            6 => {
                current?;
                let player = self.rng.below(PLAYERS);
                let plan = self.plan_open(player, PlanKindArg::Simple, 50_000_000, false);
                let mut plan = plan;
                plan.track = None;
                Some(plan.expecting(Expect::Code(ErrorCode::Closing)))
            }
            // Series A funding while a close runs (Closing) or a ticket is active (AllocationPending).
            7 => {
                let expected = if current.is_some() {
                    ErrorCode::Closing
                } else if pool.active_ticket != 0 {
                    ErrorCode::AllocationPending
                } else {
                    return None;
                };
                let plan = self.plan_fund_series_a(BACKER0, 60_000_000, false);
                Some(plan.expecting(Expect::Code(expected)))
            }
            // A stale ticket id on a phase instruction.
            8 => {
                let ticket = self.w.ticket();
                if pool.active_ticket == 0 || ticket.phase == state::IDLE {
                    return None;
                }
                let ix = self.w.rt.seal_ix(ticket.id + 1);
                Some(Plan::new(Fam::Seal, "adv_stale_ticket_id", ix).expecting(Expect::Code(ErrorCode::StaleTicket)))
            }
            // Observing a reimbursement twice.
            9 => {
                let id = latest?;
                if !self.w.close_record(id).reimbursement_observed {
                    return None;
                }
                let ix = observe_ix(&self.w, id);
                Some(Plan::new(Fam::Observe, "adv_observe_again", ix)
                    .expecting(Expect::Code(ErrorCode::AlreadyObserved)))
            }
            // Delivering a refund nobody is owed.
            10 => {
                let id = latest?;
                let close = self.w.close_record(id);
                if !close.reserve_settled || close.locked_owner_remaining > 0 {
                    return None;
                }
                let ix = deliver_ix(&self.w, id, close.owner_refund_destination);
                Some(Plan::new(Fam::Deliver, "adv_deliver_nothing_owed", ix)
                    .expecting(Expect::Code(ErrorCode::NothingDeferred)))
            }
            // Provisioning a participant page that already exists.
            11 => {
                if pool.participant_pages == 0 {
                    return None;
                }
                let ix = provision_kind_ix(&self.w, PageKind::Participant { round }, 0, Funding::Free, self.payer());
                Some(Plan::new(Fam::ProvisionFree, "adv_provision_existing_page", ix)
                    .expecting(Expect::Code(ErrorCode::WrongIndex)))
            }
            // Cleanup of a live page: the round's own participant page judged by an unfinished
            // close, or by a finished close of ANOTHER round.
            12 => {
                let id = self.target_close()?;
                let key = self.w.rt.participant_address(round, 0);
                let ix = close_page_ix(&self.w, PageKind::Participant { round }, key, attacker_key, id);
                let finished_other_round = self.w.close_record(id).round_id != round;
                let unfinished = !self.w.close_record(id).finalized;
                if !(finished_other_round || unfinished) {
                    return None;
                }
                Some(Plan::new(Fam::CloseShared, "adv_cleanup_of_a_live_page", ix)
                    .expecting(Expect::Code(ErrorCode::NotClosable)))
            }
            // A payout signed by someone who does not own the position.
            13 => {
                let big = self.big_position(round).or_else(|| self.positions.last().copied())?;
                let ix = self.w.payout_instruction(
                    PayoutKind::WithdrawSimple,
                    attacker_key,
                    self.pk(big.player),
                    big.nonce,
                    0,
                    0,
                    attacker_key,
                    CloseRecordArg::Auto,
                );
                if big.player == attacker {
                    return None;
                }
                Some(Plan::new(Fam::WithdrawSimple, "adv_payout_by_a_stranger", ix)
                    .signed(&[attacker])
                    .expecting(Expect::Reject))
            }
            // Withdrawing more operations cash than is free.
            _ => {
                let ix = withdraw_ops_ix(&self.w, self.pk(SPONSOR), ops.free_balance + 1);
                Some(Plan::new(Fam::WithdrawOps, "adv_withdraw_more_than_free", ix)
                    .signed(&[SPONSOR])
                    .expecting(Expect::Code(ErrorCode::InsufficientOperations)))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The row
// ---------------------------------------------------------------------------

/// The coverage the run must have reached, as an error message (or `Ok`).
fn coverage_problems(fuzz: &Fuzz) -> Vec<String> {
    let c = &fuzz.coverage;
    let mut problems = Vec::new();
    let mut need = |what: &str, have: u64, want: u64| {
        if have < want {
            problems.push(format!("{what}: {have} < {want}"));
        }
    };
    need("finalized rounds", c.finalized_rounds, 2);
    need("committed tickets", c.commits, 1);
    need("claims paid", c.claims_paid, 1);
    need("provider returns", c.provider_returns, 1);
    need("deferred refunds delivered", c.deferred_delivered, 1);
    need("deferred refunds booked", fuzz.deferred_seen.len() as u64, 1);
    for fam in ALL_FAMILIES {
        let attempts = fuzz.tally.get(&fam).map_or(0, |entry| entry.0);
        if attempts == 0 {
            problems.push(format!("family {} was never attempted", fam.name()));
        }
    }
    problems
}

/// Run the sequence and return every recorded transaction.
pub fn run_fuzz_suite() -> Vec<Observation> {
    let mut fuzz = Fuzz::new();
    fuzz.run();
    let problems = coverage_problems(&fuzz);
    assert!(
        problems.is_empty(),
        "randomized sequence coverage not reached (seed {SEED:#018x}): {problems:?}\ncoverage: {:?}\ntally: {:?}\nlast history:\n{}",
        fuzz.coverage,
        fuzz.tally.iter().map(|(fam, entry)| (fam.name(), *entry)).collect::<Vec<_>>(),
        fuzz.history.iter().rev().take(60).rev().cloned().collect::<Vec<_>>().join("\n")
    );
    // The headline observation: a closing reconcile that also carries the run's summary.
    let reconcile = reconcile_ix(&fuzz.w.rt);
    let headline = fuzz.w.send("", vec![reconcile], &[]).expect("the closing reconcile");
    let _ = headline;
    assert_identities(&fuzz.w);
    let summary = serde_json::json!({
        "seed": format!("{SEED:#018x}"),
        "seed_ascii": "MCFUZZ08",
        "rng": "xorshift64* (x ^= x >> 12; x ^= x << 25; x ^= x >> 27; x * 0x2545F4914F6CDD1D)",
        "steps": STEPS,
        "step_mix": "sweep every 5th step before step 215 (least-attempted family); otherwise 52% advance (progress engine), 30% perturb, 18% adversary; 100% advance when behind schedule",
        "profiles": "close 1: full draw + provider return + owner refund deferred by emptying D0 in the settling transaction; close 2: expiry; later closes alternate",
        "coverage": {
            "finalized_rounds": fuzz.coverage.finalized_rounds,
            "committed_tickets": fuzz.coverage.commits,
            "claims_paid": fuzz.coverage.claims_paid,
            "provider_returns": fuzz.coverage.provider_returns,
            "deferred_refunds_delivered": fuzz.coverage.deferred_delivered,
            "deferred_refunds_booked": fuzz.deferred_seen.len(),
            "promotions_applied": fuzz.coverage.promotions_applied,
            "promotions_skipped": fuzz.coverage.promotions_skipped,
            "draw_requests": fuzz.coverage.requests,
            "engine_rejections": fuzz.engine_rejections,
            "adversary_draws": fuzz.adversary_draws,
        },
        "families": fuzz.tally.iter().map(|(fam, (attempts, successes))| {
            (fam.name().to_string(), serde_json::json!({ "attempts": attempts, "successes": successes }))
        }).collect::<serde_json::Map<String, serde_json::Value>>(),
        "rejection_codes": fuzz.codes.iter().map(|(code, count)| (code.to_string(), serde_json::json!(count)))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "history_lines": fuzz.history.len(),
    });
    let mut log = fuzz.w.take_log();
    if let Some(observation) = log.iter_mut().find(|observation| observation.label == root()) {
        observation.unsupported = Some(format!("fuzz_summary: {summary}"));
    }
    assert_outcomes_consistent(&log);
    assert_unique_labels(&log);
    log
}
