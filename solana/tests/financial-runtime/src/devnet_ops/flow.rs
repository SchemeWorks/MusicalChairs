//! `devnet_ops flow`: run the M14 flow plan (`crate::m14_plan`, the data the clock-warped LiteSVM
//! rehearsal `tests/m14_flow_plan.rs` executes) against a cluster, one step at a time.
//!
//! ```text
//! devnet_ops flow init   --wallet-dir <dir>                         # record the wallets' PUBLIC keys in wallets.json
//! devnet_ops flow status --wallet-dir <dir> --rpc <url>             # read-only: done / next / earliest time
//! devnet_ops flow next   --step <id> --wallet-dir <dir> --rpc <url> [--execute] [--local-oracle]
//! ```
//!
//! # How a step is built
//!
//! The harness instruction builders (`World`, `close.rs`) read program state from a LiteSVM.  The
//! runner therefore copies every program-owned account of the cluster (`getProgramAccounts`) into a
//! MIRROR `World` (no program loaded, nothing ever executed in it), points the mirror's fee payer at the
//! actor, and lets the builders produce the next transaction.  The transaction then goes through the
//! same interlocks as every other `devnet_ops` write: measured and refused over 1,232 bytes before
//! signing, simulated before sending, sent only with `--execute` on a loopback validator or on devnet
//! with the packet pin (an artifact-identity check, not an approval), a write-ahead file and the devnet
//! genesis hash.  Multi-transaction steps (allocation
//! ticket phases, candidate pages, the candidate build) refresh the mirror after each transaction and
//! build the next from the new chain state, so a step is resumable and idempotent.
//!
//! # What "done" means
//!
//! Every step has a state predicate over the cluster (a counter reached, an account exists, a flag is
//! set).  A step is also done when any LATER step's predicate holds (state only moves forward).  A
//! receipt line counts as evidence for exactly ONE step, the deliberately refused early expiry, which
//! leaves no on-chain trace, and only when the receipt says `refused_as_expected` and is stamped for THIS
//! cluster (genesis hash, program id, `ConfigVersion#1` activation).  The next step is the first one that is not done;
//! `flow next` refuses any other step, any step before its earliest time (genesis + offset), a receipt
//! log holding receipts of another cluster, and any step after a `done_with_failed_checks` receipt that
//! has not been accepted with `--accept-failed-checks`.
//!
//! # Genesis
//!
//! Plan offsets count from the moment the G approvals were given.  `initialize` can execute no earlier
//! than 172,800 s later and stamps `ConfigVersion#1.activated_at` with its clock, so genesis is read as
//! that stamp minus 172,800 s (step 1, offset 172,800, is the first instant after `initialize`).  A local
//! validator cannot wait two days: `--genesis-unix` overrides genesis on a loopback cluster only.
//!
//! # The oracle
//!
//! `fulfil_*` never fulfils on devnet.  The runner waits for the real ORAO fulfilment (`--wait-seconds`)
//! and records request -> fulfilment latency in slots and seconds from `getSignaturesForAddress` of the
//! request account (packet A7).  On a loopback cluster only, `--local-oracle` signs the fulfilment with
//! the synthetic authority in `<wallet-dir>/ORACLE.json`.
//!
//! # The refused-by-design step
//!
//! `expire_at_deadline` is meant to be REFUSED by the program (`DeadlineNotReached`, expiry on the
//! deadline).  The runner builds, signs and simulates it, requires exactly that error, records it and
//! sends nothing (a known-failing transaction would only burn a fee; the simulation is the evidence).

use std::time::Instant;

use musical_chairs_economics::{
    earnings::{self, SimpleAccrual},
    fees,
};
use musical_chairs_program::{close as pclose, positions::PlanKindArg, Funding, PageKind};

use super::*;
use crate::{
    close as m7,
    m14_plan::{self, Act, Step as PlanStep, Who, ALL_WALLETS, DEADLINE_SECS, GOVERNANCE_DELAY},
    runtime::FinancialRuntime,
    world::{CloseRecordArg, PayoutKind, World},
};

const PLAN_NAME: &str = "M14-SIM Seq-B";
const WALLETS_FILE: &str = "wallets.json";
const RECEIPTS_FILE: &str = "flow-receipts.jsonl";
const ORACLE_NAME: &str = "ORACLE";
/// Compute-unit limit of every flow transaction (the harness' `CU`).
const CU_FLOW: u32 = 1_400_000;
const TICKET_ROWS: u8 = 8;
const BUILD_BATCH: u64 = 4;
const MAX_TXS_PER_STEP: usize = 256;

// ---------------------------------------------------------------------------
// Wallets
// ---------------------------------------------------------------------------

/// The plan actors' public keys as recorded in `wallets.json`.
struct Wallets {
    dir: PathBuf,
    keys: BTreeMap<Who, Pubkey>,
    oracle: Option<Pubkey>,
}

fn wallet_dir(opts: &Opts) -> Result<PathBuf, OpsError> {
    Ok(PathBuf::from(opts.require("wallet-dir")?))
}

/// `flow init`: record the PUBLIC keys of `<dir>/{S,K,A1,A2,P1..P5,W}.json` (and `ORACLE.json` when
/// present) in `<dir>/wallets.json`.  Offline; it generates no key, and refuses to overwrite a
/// `wallets.json` that records different keys.
pub(super) fn cmd_flow_init(opts: &Opts) -> Result<Value, OpsError> {
    let dir = wallet_dir(opts)?;
    let mut wallets = serde_json::Map::new();
    let mut seen = BTreeSet::new();
    for who in ALL_WALLETS {
        let path = dir.join(format!("{}.json", who.name()));
        let key = KeyFile::load(&path.display().to_string())?;
        if !seen.insert(key.pubkey) {
            return refuse(format!("{} repeats another wallet's key", path.display()));
        }
        wallets.insert(who.name().to_string(), json!(key.pubkey.to_string()));
    }
    let oracle_path = dir.join(format!("{ORACLE_NAME}.json"));
    if oracle_path.is_file() {
        let key = KeyFile::load(&oracle_path.display().to_string())?;
        if !seen.insert(key.pubkey) {
            return refuse("ORACLE.json repeats a plan wallet's key");
        }
        wallets.insert(ORACLE_NAME.to_string(), json!(key.pubkey.to_string()));
    }
    let document = json!({ "schema": 1, "plan": PLAN_NAME, "program_id": ID.to_string(), "wallets": wallets });
    let target = dir.join(WALLETS_FILE);
    if target.exists() {
        let existing: Value = serde_json::from_str(
            &fs::read_to_string(&target).map_err(|error| OpsError::Failed(format!("{}: {error}", target.display())))?,
        )
        .map_err(|error| OpsError::Failed(format!("{} is not JSON: {error}", target.display())))?;
        if existing != document {
            return refuse(format!(
                "{} already records different wallets; refusing to overwrite it",
                target.display()
            ));
        }
    } else {
        fs::write(&target, serde_json::to_string_pretty(&document).expect("json"))
            .map_err(|error| OpsError::Failed(format!("{}: {error}", target.display())))?;
    }
    Ok(json!({
        "command": "flow init",
        "mode": "offline",
        "wallets_file": target.display().to_string(),
        "wallets": document["wallets"],
        "note": "public keys only; no key was generated or copied",
        "all_ok": true,
    }))
}

fn load_wallets(dir: &Path) -> Result<Wallets, OpsError> {
    let path = dir.join(WALLETS_FILE);
    let text = fs::read_to_string(&path).map_err(|error| {
        OpsError::Refused(format!("cannot read {} ({error}); run `flow init` first", path.display()))
    })?;
    let document: Value = serde_json::from_str(&text)
        .map_err(|error| OpsError::Refused(format!("{} is not JSON: {error}", path.display())))?;
    if document["schema"] != json!(1) {
        return refuse(format!("{}: unsupported schema", path.display()));
    }
    if document["program_id"].as_str() != Some(ID.to_string().as_str()) {
        return refuse(format!(
            "{} was recorded for another program id ({}), this build is {ID}",
            path.display(),
            document["program_id"]
        ));
    }
    let table = document["wallets"]
        .as_object()
        .ok_or_else(|| OpsError::Refused(format!("{} has no wallets object", path.display())))?;
    let parse = |name: &str| -> Result<Pubkey, OpsError> {
        table
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| OpsError::Refused(format!("{} does not record wallet {name}", path.display())))
            .and_then(|text| {
                Pubkey::from_str(text)
                    .map_err(|_| OpsError::Refused(format!("{}: wallet {name} is not a pubkey", path.display())))
            })
    };
    let mut keys = BTreeMap::new();
    for who in ALL_WALLETS {
        keys.insert(who, parse(who.name())?);
    }
    let distinct: BTreeSet<Pubkey> = keys.values().copied().collect();
    if distinct.len() != keys.len() {
        return refuse(format!("{} records the same key for two wallets", path.display()));
    }
    let oracle = table.get(ORACLE_NAME).map(|_| parse(ORACLE_NAME)).transpose()?;
    Ok(Wallets {
        dir: dir.to_path_buf(),
        keys,
        oracle,
    })
}

/// The actor's key file, loaded ONLY after its pubkey is found to match the recorded one.
fn load_actor(wallets: &Wallets, who: Who) -> Result<KeyFile, OpsError> {
    let path = wallets.dir.join(format!("{}.json", who.name()));
    let key = KeyFile::load(&path.display().to_string())?;
    let recorded = wallets.keys[&who];
    if key.pubkey != recorded {
        return refuse(format!(
            "{} holds {} but wallets.json records {recorded} for plan actor {}; refusing to sign with it",
            path.display(),
            key.pubkey,
            who.name()
        ));
    }
    key.keypair()?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// The mirror view of the cluster
// ---------------------------------------------------------------------------

/// A snapshot of the cluster: every program account copied into a mirror `World`, the cluster clock,
/// the rent schedule and the few non-program accounts the identities need.
struct View {
    w: World,
    now: i64,
    slot: u64,
    ops_cash_lamports: u64,
    rent_vault: u64,
    rent_zero: u64,
    /// `close_id -> the ORAO request is fulfilled`.
    fulfilled: BTreeMap<u64, bool>,
    actor_balance: Option<u64>,
}

fn load_view(ctx: &Ctx<'_>, actor: Option<&Keypair>) -> Result<View, OpsError> {
    let accounts = ctx
        .cluster
        .program_accounts(&ID)
        .map_err(|error| rpc_err("getProgramAccounts", error))?;
    let rent_account = ctx
        .account(&sysvar::rent::id())?
        .ok_or_else(|| OpsError::Failed("the Rent sysvar account is absent".into()))?;
    let rent: solana_sdk::rent::Rent = bincode::deserialize(&rent_account.data)
        .map_err(|error| OpsError::Failed(format!("Rent sysvar: {error}")))?;
    let now = ctx
        .cluster
        .unix_time()
        .map_err(|error| rpc_err("reading the cluster clock", error))?;
    let slot = ctx.cluster.slot().map_err(|error| rpc_err("getSlot", error))?;
    let mut rt = FinancialRuntime::mirror(Pubkey::default(), Pubkey::default());
    rt.svm.set_sysvar(&rent);
    for (key, account) in accounts {
        rt.svm
            .set_account(key, account)
            .map_err(|error| OpsError::Failed(format!("mirror {key}: {error}")))?;
    }
    if let Some(keypair) = actor {
        rt.payer = keypair.insecure_clone();
    }
    let ops_cash = rt.ops_cash_address();
    let w = World::mirror(rt, now, slot);
    let ops_cash_lamports = ctx.account(&ops_cash)?.map_or(0, |account| account.lamports);
    let mut view = View {
        w,
        now,
        slot,
        ops_cash_lamports,
        rent_vault: ctx.rent(state::GameVault::SPACE)?,
        rent_zero: ctx.rent(0)?,
        fulfilled: BTreeMap::new(),
        actor_balance: None,
    };
    if let Some(pool) = view.pool() {
        for close_id in 1..=pool.next_close_id {
            let Some(record) = view.close(close_id) else { continue };
            if record.request_account == Pubkey::default() {
                continue;
            }
            let fulfilled = ctx
                .account(&record.request_account)?
                .and_then(|account| anchor::<orao::state::request::RequestAccount>(&account).ok())
                .is_some_and(|request| matches!(request.state, orao::state::request::RequestState::Fulfilled(_)));
            view.fulfilled.insert(close_id, fulfilled);
        }
    }
    if let Some(keypair) = actor {
        view.actor_balance = Some(ctx.balance(&keypair.pubkey())?);
    }
    Ok(view)
}

impl View {
    fn rt(&self) -> &FinancialRuntime {
        &self.w.rt
    }
    fn pool(&self) -> Option<state::Pool> {
        self.rt().try_deserialize(self.rt().pool_address())
    }
    fn ops(&self) -> Option<state::Ops> {
        self.rt().try_deserialize(self.rt().ops_address())
    }
    fn ticket(&self) -> Option<state::Ticket> {
        self.rt().try_deserialize(self.rt().ticket_address())
    }
    fn config(&self, version: u32) -> Option<state::ConfigVersion> {
        self.rt().try_deserialize(self.rt().config_address(version))
    }
    fn close(&self, close_id: u64) -> Option<state::CloseRecord> {
        self.rt().try_deserialize(self.rt().close_record_address(close_id))
    }
    fn position(&self, owner: Pubkey, nonce: u64) -> Option<state::Position> {
        self.rt().try_deserialize(self.rt().position_address(owner, nonce))
    }
    fn backer(&self, owner: Pubkey, series: u8) -> Option<state::Backer> {
        self.rt().try_deserialize(self.rt().backer_address(series, owner))
    }
    fn participant_page(&self, round: u64, page: u64) -> Option<state::ParticipantPage> {
        self.rt().try_deserialize(self.rt().participant_address(round, page))
    }
    /// The Backer's credit that a claim would pay now (pending tickets at or below the watermark
    /// materialized).
    fn claimable(&self, owner: Pubkey, series: u8, watermark: u64) -> Option<u64> {
        let mut backer = self.backer(owner, series)?;
        backer.materialize(watermark).ok()?;
        Some(backer.funded)
    }
}

/// Both cash registers and the partition identity, computed from the cluster's accounts.
fn conservation(view: &View) -> Value {
    let (Some(pool), Some(ops)) = (view.pool(), view.ops()) else {
        return json!({ "ok": false, "reason": "Pool or Ops is absent (initialize has not run)" });
    };
    let vault = view.w.lamports(view.rt().game_vault_address());
    let vault_spendable = vault.checked_sub(view.rent_vault);
    let accounted = musical_chairs_program::game_accounted(&pool).ok();
    let vault_ok = matches!((vault_spendable, accounted), (Some(spendable), Some(sum)) if u128::from(spendable) == sum);
    let cash_spendable = view.ops_cash_lamports.checked_sub(view.rent_zero);
    let recorded = u128::from(ops.free_balance)
        + u128::from(ops.locked_remaining_total)
        + u128::from(ops.unallocated_excess);
    let cash_ok = cash_spendable.is_some_and(|spendable| u128::from(spendable) == recorded);
    let partitions = m7::partitions_total(&view.w);
    let partitions_ok = partitions == ops.locked_remaining_total;
    json!({
        "ok": vault_ok && cash_ok && partitions_ok,
        "game_vault": {
            "lamports": vault, "rent": view.rent_vault, "spendable": vault_spendable,
            "pot_seed_management_pending_toll_funded_claims_excess": accounted.map(|sum| sum.to_string()),
            "ok": vault_ok,
        },
        "ops_cash": {
            "lamports": view.ops_cash_lamports, "rent": view.rent_zero, "spendable": cash_spendable,
            "free_locked_unallocated": recorded.to_string(), "ok": cash_ok,
        },
        "partitions": { "ops_locked_remaining_total": ops.locked_remaining_total, "sum_of_close_records": partitions, "ok": partitions_ok },
    })
}

fn conservation_ok(value: &Value) -> bool {
    value["ok"] == json!(true)
}

// ---------------------------------------------------------------------------
// Genesis, time, evaluation
// ---------------------------------------------------------------------------

struct Genesis {
    initialize_unix: i64,
    genesis_unix: i64,
    source: &'static str,
}

fn genesis_of(ctx: &Ctx<'_>, view: &View) -> Result<Option<Genesis>, OpsError> {
    if let Some(unix) = ctx.opts.u64_flag("genesis-unix")? {
        if ctx.gate.target != Target::Local {
            return refuse("--genesis-unix overrides the plan clock on a loopback cluster only");
        }
        return Ok(Some(Genesis {
            initialize_unix: unix as i64 + GOVERNANCE_DELAY as i64,
            genesis_unix: unix as i64,
            source: "--genesis-unix (loopback override)",
        }));
    }
    let Some(config) = view.config(1) else { return Ok(None) };
    let stamp = config.activated_at.unwrap_or(config.staged_at);
    if stamp == 0 {
        return Ok(None);
    }
    Ok(Some(Genesis {
        initialize_unix: stamp as i64,
        genesis_unix: stamp as i64 - GOVERNANCE_DELAY as i64,
        source: "ConfigVersion#1.activated_at minus the 172,800 s governance delay",
    }))
}

/// `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, days-from-civil inverse).
fn utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let seconds = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60
    )
}

fn is_closing(act: Act) -> bool {
    matches!(
        act,
        Act::WithdrawSimple { closing: true, .. } | Act::SettleCompound { closing: true, .. }
    )
}

/// 1-based ordinal of the closing payout at `index` (close id it starts).
fn closing_ordinal(steps: &[PlanStep], index: usize) -> u64 {
    steps[..=index].iter().filter(|step| is_closing(step.act)).count() as u64
}

/// 1-based ordinal of the `AdvanceTicket` step at `index` (the allocation ticket id it commits: tickets
/// are created by the toll-bearing payouts in plan order).
fn ticket_ordinal(steps: &[PlanStep], index: usize) -> u64 {
    steps[..=index]
        .iter()
        .filter(|step| matches!(step.act, Act::AdvanceTicket { .. }))
        .count() as u64
}

/// Allocation tickets committed by the steps BEFORE `index`.
fn tickets_before(steps: &[PlanStep], index: usize) -> u64 {
    steps[..index]
        .iter()
        .filter(|step| matches!(step.act, Act::AdvanceTicket { .. }))
        .count() as u64
}

fn resolve_winner(view: &View, wallets: &Wallets) -> Option<Who> {
    match view.close(1)?.promotion {
        state::Promotion::Applied { winner, .. } => [Who::P4, Who::P5]
            .into_iter()
            .find(|candidate| wallets.keys[candidate] == winner),
        _ => None,
    }
}

fn resolve(who: Who, winner: Option<Who>) -> Option<Who> {
    match who {
        Who::Winner => winner,
        other => Some(other),
    }
}

/// Host-derived candidate pages a close needs (preferred, fallback).
fn pages_needed(view: &View, close_id: u64) -> (u64, u64) {
    let expected = m7::expected_candidates(&view.w, close_id);
    (m7::pages_of(expected.preferred.len()), m7::pages_of(expected.fallback.len()))
}

/// The state predicate of a step: `(done, evidence)`.
fn done_by_state(
    view: &View,
    wallets: &Wallets,
    steps: &[PlanStep],
    index: usize,
    winner: Option<Who>,
) -> (bool, String) {
    let step = steps[index];
    let (Some(pool), Some(ops)) = (view.pool(), view.ops()) else {
        return (false, "the program is not initialized".into());
    };
    let owner = resolve(step.who, winner).and_then(|who| wallets.keys.get(&who).copied());
    match step.act {
        Act::FundOperations => (
            ops.total_contributed >= u128::from(step.amount),
            format!("Ops.total_contributed {} >= {}", ops.total_contributed, step.amount),
        ),
        Act::ProvisionRegistry => (pool.registry_pages >= 1, format!("Pool.registry_pages {}", pool.registry_pages)),
        Act::ProvisionSnapshot => (pool.snapshot_pages >= 1, format!("Pool.snapshot_pages {}", pool.snapshot_pages)),
        Act::ProvisionParticipants { round } => {
            let present = view.participant_page(round, 0).is_some();
            (present, format!("ParticipantPage(round {round}, 0) exists: {present}"))
        }
        Act::ProvisionCloseRecord { close_id } => {
            let present = view.close(close_id).is_some();
            (present, format!("CloseRecord({close_id}) exists: {present}"))
        }
        Act::FundSeriesA => match owner.and_then(|owner| view.backer(owner, 0)) {
            Some(backer) => (
                backer.amount >= u128::from(step.amount),
                format!("Backer A amount {} >= {}", backer.amount, step.amount),
            ),
            None => (false, "no Series A Backer account".into()),
        },
        Act::Open { nonce, .. } => {
            let present = owner.and_then(|owner| view.position(owner, nonce)).is_some();
            (present, format!("Position(nonce {nonce}) exists: {present}"))
        }
        Act::WithdrawSimple { nonce, closing } | Act::SettleCompound { nonce, closing } => {
            if closing {
                let ordinal = closing_ordinal(steps, index);
                (
                    pool.next_close_id > ordinal,
                    format!("Pool.next_close_id {} > {ordinal} (close {ordinal} started)", pool.next_close_id),
                )
            } else {
                match owner.and_then(|owner| view.position(owner, nonce)) {
                    Some(position) => (position.paid_gross > 0, format!("Position.paid_gross {}", position.paid_gross)),
                    None => (false, "the position does not exist".into()),
                }
            }
        }
        Act::AdvanceTicket { .. } => {
            let ordinal = ticket_ordinal(steps, index);
            (
                pool.finalized_ticket >= ordinal,
                format!("Pool.finalized_ticket {} >= {ordinal}", pool.finalized_ticket),
            )
        }
        Act::Claim { a, b } => {
            let gate = tickets_before(steps, index);
            let Some(owner) = owner else { return (false, "claimant unresolved".into()) };
            let watermark = pool.finalized_ticket;
            let series_ok = |series: u8, wanted: bool| {
                !wanted || view.claimable(owner, series, watermark) == Some(0)
            };
            let done = pool.finalized_ticket >= gate && series_ok(0, a) && series_ok(1, b);
            (done, format!("ticket {gate} finalized and the wanted Backer credit is claimed (funded 0)"))
        }
        Act::CandidatePages { close_id } => match view.close(close_id) {
            Some(record) if record.close_started_at > 0 => {
                let (preferred, fallback) = pages_needed(view, close_id);
                (
                    record.provisioned_preferred_pages >= preferred && record.provisioned_fallback_pages >= fallback,
                    format!(
                        "provisioned {}/{} preferred and {}/{} fallback pages",
                        record.provisioned_preferred_pages, preferred, record.provisioned_fallback_pages, fallback
                    ),
                )
            }
            _ => (false, format!("close {close_id} has not started")),
        },
        Act::BuildCandidates { close_id } => {
            let sealed = view.close(close_id).is_some_and(|record| record.sealed);
            (sealed, format!("CloseRecord({close_id}).sealed {sealed}"))
        }
        Act::RequestDraw { close_id } => {
            let state_now = view.close(close_id).map_or(0, |record| record.draw_state);
            (
                state_now >= state::DRAW_REQUESTED,
                format!("CloseRecord({close_id}).draw_state {state_now}"),
            )
        }
        Act::Fulfil { close_id } => {
            let fulfilled = view.fulfilled.get(&close_id).copied().unwrap_or(false);
            (fulfilled, format!("the ORAO request of close {close_id} is fulfilled: {fulfilled}"))
        }
        Act::CaptureDraw { close_id } => match view.close(close_id) {
            Some(record) => (
                record.draw_state >= state::DRAW_CAPTURED || record.promotion != state::Promotion::Pending,
                format!("CloseRecord({close_id}).draw_state {}", record.draw_state),
            ),
            None => (false, "no CloseRecord".into()),
        },
        Act::ApplyPromotion { close_id } | Act::ExpirePromotion { close_id } => {
            let terminal = view.close(close_id).is_some_and(|record| record.promotion != state::Promotion::Pending);
            (terminal, format!("CloseRecord({close_id}).promotion terminal: {terminal}"))
        }
        Act::ComposedReopen { close_id, .. } | Act::FinalizeRound { close_id } => {
            let finalized = view.close(close_id).is_some_and(|record| record.finalized);
            (finalized, format!("CloseRecord({close_id}).finalized {finalized}"))
        }
        Act::ExpireTooEarly { close_id } => match view.close(close_id) {
            Some(record) if record.close_started_at > 0 => {
                let passed = record.promotion != state::Promotion::Pending || view.now > record.promotion_deadline as i64;
                (passed, format!("promotion terminal or the deadline {} has passed", record.promotion_deadline))
            }
            _ => (false, format!("close {close_id} has not started")),
        },
    }
}

struct StepEval {
    index: usize,
    step: PlanStep,
    done: bool,
    evidence: String,
}

/// Judge every step from the cluster (and the receipt log for the traceless ones); a step is done when
/// its predicate or any later step's holds.
fn evaluate(view: &View, wallets: &Wallets, receipts: &ReceiptLog) -> (Vec<StepEval>, Option<Who>) {
    let steps = m14_plan::plan();
    let winner = resolve_winner(view, wallets);
    let local: Vec<(bool, String)> = (0..steps.len())
        .map(|index| {
            let (state_done, evidence) = done_by_state(view, wallets, &steps, index, winner);
            if state_done {
                (true, evidence)
            } else if traceless(steps[index].act) && receipts.traceless_done(steps[index].id) {
                (true, "a receipt of this cluster records it (the step leaves no on-chain trace)".to_string())
            } else {
                (false, evidence)
            }
        })
        .collect();
    let mut done = vec![false; steps.len()];
    let mut evidence = vec![String::new(); steps.len()];
    let mut later = false;
    for index in (0..steps.len()).rev() {
        if local[index].0 {
            done[index] = true;
            evidence[index] = local[index].1.clone();
            later = true;
        } else if later {
            done[index] = true;
            evidence[index] = "a later step is complete".into();
        } else {
            evidence[index] = local[index].1.clone();
        }
    }
    let evals = steps
        .iter()
        .enumerate()
        .map(|(index, step)| StepEval {
            index,
            step: *step,
            done: done[index],
            evidence: std::mem::take(&mut evidence[index]),
        })
        .collect();
    (evals, winner)
}

/// `--receipts`, else `<wallet-dir>/flow-receipts.jsonl`; never the default wallet, a `.json` file or a
/// keypair-shaped file.
fn receipts_path(opts: &Opts, wallets: &Wallets) -> Result<PathBuf, OpsError> {
    match opts.get("receipts") {
        Some(spec) => guarded_output_path("receipts", spec),
        None => guarded_output_path("receipts", &wallets.dir.join(RECEIPTS_FILE).display().to_string()),
    }
}

/// The steps that leave NO on-chain trace, so a receipt is their only evidence: the refused-by-design
/// expiry.  (The oracle step is judged from the request account; its receipt only records the latency.)
fn traceless(act: Act) -> bool {
    matches!(act, Act::ExpireTooEarly { .. })
}

/// Steps of a single transaction: after ONE confirmed send they never loop on a possibly stale read.
fn is_single_tx(act: Act) -> bool {
    !matches!(
        act,
        Act::AdvanceTicket { .. } | Act::CandidatePages { .. } | Act::BuildCandidates { .. }
    )
}

/// Which cluster a receipt belongs to: the genesis hash, the program id and the `ConfigVersion#1`
/// activation stamp (a fresh `--reset` validator or another deployment has a different one).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    genesis_hash: String,
    program_id: String,
    activation_unix: u64,
}

impl Stamp {
    fn to_json(&self) -> Value {
        json!({
            "genesis_hash": self.genesis_hash,
            "program_id": self.program_id,
            "configversion1_activated_at": self.activation_unix,
        })
    }
}

fn stamp_of(ctx: &Ctx<'_>, view: &View) -> Option<Stamp> {
    let config = view.config(1)?;
    let activation_unix = config.activated_at.unwrap_or(config.staged_at);
    Some(Stamp {
        genesis_hash: ctx.genesis.clone(),
        program_id: ID.to_string(),
        activation_unix,
    })
}

/// The receipts of THIS cluster; receipts stamped for another cluster (or unstamped) are only counted.
#[derive(Default)]
struct ReceiptLog {
    results: BTreeMap<String, String>,
    foreign: usize,
    failed_checks: Vec<String>,
}

/// The `result` a receipt records for a step that was REFUSED BY THE PROGRAM as designed (the traceless
/// `expire_at_deadline`): the only receipt that can stand in for on-chain evidence.
const RESULT_REFUSED_AS_EXPECTED: &str = "refused_as_expected";

impl ReceiptLog {
    /// Any receipt of this cluster exists for the step (whatever its result).
    fn has(&self, step_id: &str) -> bool {
        self.results.contains_key(step_id)
    }

    /// A traceless step is done from its receipt only when the receipt says the program refused it as
    /// designed.  A `done_with_failed_checks` (or any other) result is NOT evidence that it was refused
    /// for the right reason, so the step is simulated again instead of being skipped.
    fn traceless_done(&self, step_id: &str) -> bool {
        self.results.get(step_id).is_some_and(|result| result == RESULT_REFUSED_AS_EXPECTED)
    }
}

/// FAIL CLOSED: only a missing file is an empty log; any other read error, or a non-blank line that is
/// not JSON, refuses (a torn `done_with_failed_checks` receipt must not silently drop out and unblock
/// the next step).
fn read_receipts(path: &Path, stamp: Option<&Stamp>) -> Result<ReceiptLog, OpsError> {
    let mut log = ReceiptLog::default();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(log),
        Err(error) => return refuse(format!("cannot read the receipts file {} ({error})", path.display())),
    };
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            return refuse(format!(
                "{} line {}: not a JSON receipt; the receipts file must be intact (inspect it, do not delete lines)",
                path.display(),
                index + 1
            ));
        };
        let ours = stamp.is_some_and(|stamp| entry["stamp"] == stamp.to_json());
        if !ours {
            log.foreign += 1;
            continue;
        }
        let (Some(step), Some(result)) = (entry["step"].as_str(), entry["result"].as_str()) else { continue };
        if result == "done_with_failed_checks" {
            log.failed_checks.push(step.to_string());
        }
        log.results.insert(step.to_string(), result.to_string());
    }
    Ok(log)
}

/// `--accept-failed-checks a,b`: the steps whose failed-check receipts the operator has examined.
fn accepted_failed_checks(opts: &Opts) -> Result<BTreeSet<String>, OpsError> {
    let known: BTreeSet<&str> = m14_plan::plan().iter().map(|step| step.id).collect();
    let mut accepted = BTreeSet::new();
    for id in opts.get("accept-failed-checks").unwrap_or("").split(',').map(str::trim).filter(|id| !id.is_empty()) {
        if !known.contains(id) {
            return usage(format!("--accept-failed-checks: `{id}` is not a plan step"));
        }
        accepted.insert(id.to_string());
    }
    Ok(accepted)
}

/// The failed-check receipts nobody has accepted.
fn unaccepted_failures<'a>(log: &'a ReceiptLog, accepted: &BTreeSet<String>) -> Vec<&'a String> {
    log.failed_checks.iter().filter(|id| !accepted.contains(*id)).collect()
}

// ---------------------------------------------------------------------------
// flow status
// ---------------------------------------------------------------------------

pub(super) fn cmd_flow(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    match ctx.opts.positional.first().map(String::as_str) {
        Some("status") => cmd_status(ctx),
        Some("next") => cmd_next(ctx),
        Some(other) => usage(format!("unknown flow action `{other}` (init | status | next)")),
        None => usage("`flow` needs an action: init | status | next"),
    }
}

fn cmd_status(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let wallets = load_wallets(&wallet_dir(ctx.opts)?)?;
    let receipts_file = receipts_path(ctx.opts, &wallets)?;
    let view = load_view(ctx, None)?;
    let stamp = stamp_of(ctx, &view);
    let receipts = read_receipts(&receipts_file, stamp.as_ref())?;
    let accepted = accepted_failed_checks(ctx.opts)?;
    let unaccepted: Vec<String> = unaccepted_failures(&receipts, &accepted).into_iter().cloned().collect();
    let genesis = genesis_of(ctx, &view)?;
    let (evals, winner) = evaluate(&view, &wallets, &receipts);
    let completed = evals.iter().filter(|eval| eval.done).count();
    let next = evals.iter().find(|eval| !eval.done);
    let steps: Vec<Value> = evals
        .iter()
        .map(|eval| {
            let actor = resolve(eval.step.who, winner);
            json!({
                "index": eval.index + 1,
                "id": eval.step.id,
                "flows": eval.step.flows,
                "actor": eval.step.who.name(),
                "actor_pubkey": actor.map(|who| wallets.keys[&who].to_string()),
                "offset_s": eval.step.at,
                "earliest_unix": genesis.as_ref().map(|g| g.genesis_unix + eval.step.at as i64),
                "done": eval.done,
                "evidence": eval.evidence,
                "receipt_recorded": receipts.has(eval.step.id),
            })
        })
        .collect();
    let next_json = match (next, &genesis) {
        (Some(eval), Some(genesis)) => {
            let earliest = genesis.genesis_unix + eval.step.at as i64;
            json!({
                "index": eval.index + 1,
                "id": eval.step.id,
                "actor": eval.step.who.name(),
                "expect": eval.step.expect,
                "earliest_unix": earliest,
                "earliest_utc": utc(earliest),
                "seconds_until_earliest": (earliest - view.now).max(0),
                "ready": view.now >= earliest,
            })
        }
        (Some(eval), None) => json!({
            "index": eval.index + 1, "id": eval.step.id, "actor": eval.step.who.name(),
            "note": "genesis is unknown until initialize has run",
        }),
        (None, _) => json!("the plan is complete"),
    };
    let books = conservation(&view);
    Ok(json!({
        "command": "flow status",
        "mode": "read-only",
        "plan": PLAN_NAME,
        "program_id": ID.to_string(),
        "cluster_unix": view.now,
        "cluster_slot": view.slot,
        "genesis": genesis.as_ref().map(|g| json!({
            "genesis_unix": g.genesis_unix, "genesis_utc": utc(g.genesis_unix),
            "initialize_unix": g.initialize_unix, "source": g.source,
        })),
        "winner": winner.map(|who| who.name()),
        "completed": completed,
        "total": evals.len(),
        "next": next_json,
        "steps": steps,
        "conservation": books,
        "stamp": stamp.as_ref().map(Stamp::to_json),
        "foreign_receipts": receipts.foreign,
        "failed_check_receipts": receipts.failed_checks,
        "unaccepted_failed_check_receipts": unaccepted,
        "all_ok": (conservation_ok(&books) || view.pool().is_none()) && unaccepted.is_empty(),
    }))
}

// ---------------------------------------------------------------------------
// Instruction helpers (copies of the rehearsal's private builders)
// ---------------------------------------------------------------------------

fn page_ix(w: &World, kind: PageKind, index: u64, page: Pubkey, payer: Pubkey) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::ProvisionPage {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            page,
            payer,
            close_record: None,
            system_program: system_program::ID,
        },
        instruction::ProvisionPage { kind, index, funding: Funding::Free },
    )
}

/// `open_position` against `round`'s accounts (a composed transaction changes the pool's round
/// mid-flight, so the World's own builder cannot be used).
fn open_ix_for_round(
    w: &World,
    owner: Pubkey,
    round: u64,
    plan: PlanKindArg,
    nonce: u64,
    gross: u64,
) -> Instruction {
    let rt = &w.rt;
    anchor_ix(
        accounts::OpenPosition {
            owner,
            pool: rt.pool_address(),
            admission: rt.admission_address(),
            game_vault: rt.game_vault_address(),
            user: rt.user_address(owner),
            user_round: rt.user_round_address(round, owner),
            position: rt.position_address(owner, nonce),
            config: rt.config_address(1),
            participant_page: rt.participant_address(round, 0),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::OpenPosition {
            plan,
            gross,
            client_nonce: nonce,
            expected_round_id: round,
            expected_config: 1,
        },
    )
}

/// What `withdraw_simple` must pay at `now`, from the position and the pot on chain.
fn simple_expectation(w: &World, owner: Pubkey, nonce: u64, now: u64) -> Result<fees::FeePayout, OpsError> {
    let position = w.position(owner, nonce);
    let mut accrual = SimpleAccrual {
        principal: position.principal,
        last_accrued_at: position.last_accrued_at,
        expires_at: position.maturity_at,
        remainder: position.remainder,
        accrued_gross: position.accrued_gross,
    };
    accrual
        .accrue(now)
        .map_err(|error| OpsError::Failed(format!("accrual: {error:?}")))?;
    let owed = accrual.accrued_gross - position.paid_gross;
    fees::payout(owed, w.pool().pot, fees::simple_toll_bps(now - position.opened_at))
        .map_err(|error| OpsError::Failed(format!("payout arithmetic: {error:?}")))
}

/// What `settle_compounding` must pay at `now` (the position has matured).
fn compound_expectation(w: &World, owner: Pubkey, nonce: u64) -> Result<fees::FeePayout, OpsError> {
    let position = w.position(owner, nonce);
    let config = w.config(position.config_version);
    let index = match position.kind {
        1 => 0,
        2 => 1,
        other => return refuse(format!("position kind {other} is not a compounding plan")),
    };
    let owed = u128::from(position.principal) * u128::from(config.compound_factors_q18[index]) / earnings::Q18;
    fees::payout(owed, w.pool().pot, config.jackpot_bps[index])
        .map_err(|error| OpsError::Failed(format!("payout arithmetic: {error:?}")))
}

/// One Ed25519-precompile signature over `message`, all offsets in the same instruction.
fn ed25519_instruction(pubkey: &Pubkey, signature: &[u8; 64], message: &[u8]) -> Instruction {
    let start = 16usize;
    let mut data = Vec::with_capacity(start + 32 + 64 + message.len());
    data.extend_from_slice(&[1, 0]);
    data.extend_from_slice(&((start + 32) as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&(start as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&((start + 32 + 64) as u16).to_le_bytes());
    data.extend_from_slice(&(message.len() as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(pubkey.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction {
        program_id: solana_sdk::ed25519_program::id(),
        accounts: Vec::new(),
        data,
    }
}

// ---------------------------------------------------------------------------
// Building the next transaction of a step
// ---------------------------------------------------------------------------

struct TxSpec {
    label: String,
    instructions: Vec<Instruction>,
    /// Anchor error name the simulation MUST return (the refused-by-design step).
    expect_failure: Option<&'static str>,
    /// Pre-state facts and the economics oracle's expectation, for the receipt.
    info: Value,
}

enum Next {
    Done,
    Tx(TxSpec),
    /// Nothing to send: waiting for the cluster (the ORAO oracle).
    Wait(String),
}

fn tx(label: impl Into<String>, instructions: Vec<Instruction>) -> Result<Next, OpsError> {
    Ok(Next::Tx(TxSpec {
        label: label.into(),
        instructions,
        expect_failure: None,
        info: Value::Null,
    }))
}

/// The close a step concerns, when it concerns one.
fn act_close_id(steps: &[PlanStep], index: usize) -> Option<u64> {
    match steps[index].act {
        Act::CandidatePages { close_id }
        | Act::BuildCandidates { close_id }
        | Act::RequestDraw { close_id }
        | Act::Fulfil { close_id }
        | Act::CaptureDraw { close_id }
        | Act::ApplyPromotion { close_id }
        | Act::ComposedReopen { close_id, .. }
        | Act::ExpireTooEarly { close_id }
        | Act::ExpirePromotion { close_id }
        | Act::FinalizeRound { close_id }
        | Act::ProvisionCloseRecord { close_id } => Some(close_id),
        Act::WithdrawSimple { closing: true, .. } | Act::SettleCompound { closing: true, .. } => {
            Some(closing_ordinal(steps, index))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn next_tx(
    view: &View,
    wallets: &Wallets,
    steps: &[PlanStep],
    index: usize,
    winner: Option<Who>,
    actor_key: Pubkey,
    local_oracle: Option<&Keypair>,
) -> Result<Next, OpsError> {
    let step = steps[index];
    if done_by_state(view, wallets, steps, index, winner).0 {
        return Ok(Next::Done);
    }
    let rt = view.rt();
    let w = &view.w;
    let pool = view
        .pool()
        .ok_or_else(|| OpsError::Refused("the program is not initialized on this cluster".into()))?;
    let owner = actor_key;
    let now = view.now as u64;
    let need_close = |close_id: u64| -> Result<state::CloseRecord, OpsError> {
        view.close(close_id)
            .ok_or_else(|| OpsError::Refused(format!("CloseRecord({close_id}) does not exist")))
    };
    match step.act {
        Act::FundOperations => tx(
            "fund_operations",
            vec![anchor_ix(
                accounts::FundOperations {
                    sender: owner,
                    ops: rt.ops_address(),
                    ops_cash: rt.ops_cash_address(),
                    system_program: system_program::ID,
                    event_authority: rt.event_authority_address(),
                    program: ID,
                },
                instruction::FundOperations { amount: step.amount },
            )],
        ),
        Act::ProvisionRegistry => {
            let index = pool.registry_pages;
            tx("provision_registry", vec![page_ix(w, PageKind::Registry, index, rt.registry_address(index), owner)])
        }
        Act::ProvisionSnapshot => {
            let index = pool.snapshot_pages;
            tx("provision_snapshot", vec![page_ix(w, PageKind::Snapshot, index, rt.snapshot_address(index), owner)])
        }
        Act::ProvisionParticipants { round } => {
            let index = pool.participant_pages;
            tx(
                "provision_participants",
                vec![page_ix(w, PageKind::Participant { round }, index, rt.participant_address(round, index), owner)],
            )
        }
        Act::ProvisionCloseRecord { close_id } => tx(
            "provision_close_record",
            vec![anchor_ix(
                accounts::ProvisionCloseRecord {
                    pool: rt.pool_address(),
                    ops: rt.ops_address(),
                    ops_cash: rt.ops_cash_address(),
                    close_record: rt.close_record_address(close_id),
                    payer: owner,
                    system_program: system_program::ID,
                },
                instruction::ProvisionCloseRecord { close_id, funding: Funding::Free },
            )],
        ),
        Act::FundSeriesA => tx("fund_series_a", vec![w.fund_series_a_instruction(owner, step.amount, pool.round_id)]),
        Act::Open { plan, nonce } => tx(
            "open_position",
            vec![w.open_instruction(owner, plan, step.amount, nonce, pool.round_id, pool.active_config_version)],
        ),
        Act::WithdrawSimple { nonce, closing } | Act::SettleCompound { nonce, closing } => {
            let kind = if matches!(step.act, Act::WithdrawSimple { .. }) {
                PayoutKind::WithdrawSimple
            } else {
                PayoutKind::SettleCompounding
            };
            let position = view
                .position(owner, nonce)
                .ok_or_else(|| OpsError::Refused(format!("{} has no position with nonce {nonce}", step.who.name())))?;
            let expected = match kind {
                PayoutKind::WithdrawSimple => simple_expectation(w, owner, nonce, now)?,
                PayoutKind::SettleCompounding => {
                    if now < position.maturity_at {
                        return refuse(format!(
                            "the position matures at {} ({} s from the cluster clock {now})",
                            position.maturity_at,
                            position.maturity_at - now
                        ));
                    }
                    compound_expectation(w, owner, nonce)?
                }
            };
            if expected.insolvent != closing {
                return refuse(format!(
                    "the plan expects this payout to be {} but at the cluster clock {now} (pot {}, owed {}) it would be {}; not sending",
                    if closing { "strictly insolvent (it closes the round)" } else { "solvent" },
                    pool.pot,
                    expected.gross_owed,
                    if expected.insolvent { "insolvent" } else { "solvent" },
                ));
            }
            let mut reserve = None;
            if closing {
                let ops = view.ops().ok_or_else(|| OpsError::Refused("Ops is absent".into()))?;
                let needed = w.expected_reserve(pool.participant_count, pool.registry_len, 0, 0);
                if ops.free_balance < needed {
                    return refuse(format!(
                        "Ops.free_balance {} cannot lock the close reserve {needed}; fund operations first",
                        ops.free_balance
                    ));
                }
                reserve = Some(needed);
            }
            Ok(Next::Tx(TxSpec {
                label: "payout".into(),
                instructions: vec![w.payout_instruction(kind, owner, owner, nonce, 0, 0, owner, CloseRecordArg::Auto)],
                expect_failure: None,
                info: json!({
                    "pot_before": pool.pot,
                    "expectation": {
                        "gross_owed": expected.gross_owed.to_string(), "paid_gross": expected.paid_gross,
                        "toll": expected.toll, "net": expected.net, "insolvent": expected.insolvent,
                        "margin": if expected.insolvent { format!("-{}", expected.gross_owed - u128::from(pool.pot)) } else { (u128::from(pool.pot) - expected.gross_owed).to_string() },
                    },
                    "close_reserve": reserve,
                }),
            }))
        }
        Act::AdvanceTicket { .. } => {
            let target = ticket_ordinal(steps, index);
            let ticket = view
                .ticket()
                .ok_or_else(|| OpsError::Refused("the Ticket account is absent".into()))?;
            if ticket.phase == state::IDLE {
                return refuse(format!(
                    "no allocation ticket is active, but ticket {target} is not finalized (finalized_ticket {}); the payout that creates it has not run",
                    pool.finalized_ticket
                ));
            }
            if ticket.id != target {
                return refuse(format!("the active ticket is {}, this step commits ticket {target}", ticket.id));
            }
            let phase = rt.next_phase_step(TICKET_ROWS, false);
            tx(format!("ticket {target}: {} (rows {})", phase.name, phase.rows), vec![phase.instruction])
        }
        Act::Claim { a, b } => {
            let watermark = pool.finalized_ticket;
            let part_a = view.claimable(owner, 0, watermark).unwrap_or(0);
            let part_b = view.claimable(owner, 1, watermark).unwrap_or(0);
            if (part_a > 0) != a || (part_b > 0) != b {
                return refuse(format!(
                    "the plan expects the claim to carry Series A {a} / Series B {b}, the chain shows A credit {part_a} and B credit {part_b}"
                ));
            }
            let key_a = w.program_account(rt.backer_address(0, owner));
            let key_b = w.program_account(rt.backer_address(1, owner));
            Ok(Next::Tx(TxSpec {
                label: "claim_backer".into(),
                instructions: vec![rt.claim_backer_ix(owner, key_a, key_b, owner)],
                expect_failure: None,
                info: json!({ "a_part": part_a, "b_part": part_b }),
            }))
        }
        Act::CandidatePages { close_id } => {
            let record = need_close(close_id)?;
            if record.close_started_at == 0 {
                return refuse(format!("close {close_id} has not started"));
            }
            let (preferred, fallback) = pages_needed(view, close_id);
            if record.provisioned_preferred_pages < preferred {
                let page = record.provisioned_preferred_pages;
                tx(
                    format!("candidate page preferred #{page}"),
                    vec![m7::provision_ix(w, close_id, 0, page, Funding::LockedClose)],
                )
            } else if record.provisioned_fallback_pages < fallback {
                let page = record.provisioned_fallback_pages;
                tx(
                    format!("candidate page fallback #{page}"),
                    vec![m7::provision_ix(w, close_id, 1, page, Funding::LockedClose)],
                )
            } else {
                Ok(Next::Done)
            }
        }
        Act::BuildCandidates { close_id } => {
            let record = need_close(close_id)?;
            let remaining = record.participant_count - record.candidate_cursor;
            let rows = remaining.min(BUILD_BATCH) as u8;
            tx(format!("build_candidates rows {rows} from {}", record.candidate_cursor), vec![m7::build_next(w, close_id, rows)])
        }
        Act::RequestDraw { close_id } => {
            let record = need_close(close_id)?;
            if now > record.promotion_deadline {
                return refuse(format!(
                    "the promotion deadline {} has passed (cluster clock {now}); the request is no longer accepted, the close can only expire",
                    record.promotion_deadline
                ));
            }
            let config = view
                .config(record.config_version)
                .ok_or_else(|| OpsError::Refused("the close's ConfigVersion is absent".into()))?;
            let mut accounts_of = m7::request_accounts(w, close_id);
            // The provider tuple is the one PINNED in the ConfigVersion (the devnet treasury), not the
            // rehearsal's synthetic one.
            accounts_of.treasury = config.provider_treasury;
            accounts_of.network_state = config.provider_network_state;
            accounts_of.client = config.provider_client;
            tx("request_round_draw", vec![anchor_ix(accounts_of, instruction::RequestRoundDraw)])
        }
        Act::Fulfil { close_id } => {
            let record = need_close(close_id)?;
            if record.request_account == Pubkey::default() {
                return refuse(format!("close {close_id} has no request yet"));
            }
            let Some(authority) = local_oracle else {
                return Ok(Next::Wait(format!(
                    "waiting for the ORAO oracle to fulfil request {} (deadline {}, cluster clock {now}); this tool never fulfils on devnet",
                    record.request_account, record.promotion_deadline
                )));
            };
            let keys = m7::provider_keys();
            let mut message = Vec::with_capacity(64);
            message.extend_from_slice(keys.client.as_ref());
            message.extend_from_slice(&record.protocol_seed);
            let signature = authority.sign_message(&message);
            let bytes: &[u8; 64] = signature
                .as_ref()
                .try_into()
                .map_err(|_| OpsError::Failed("an Ed25519 signature is 64 bytes".into()))?;
            let ed25519 = ed25519_instruction(&authority.pubkey(), bytes, &message);
            let fulfill = m7::foreign_ix(
                orao::id(),
                orao::accounts::Fulfill {
                    payer: owner,
                    program: ID,
                    state: keys.rng_authority,
                    client: keys.client,
                    request: record.request_account,
                    network_state: keys.network_state,
                    instruction_acc: sysvar::instructions::id(),
                },
                orao::instruction::Fulfill { params: orao::FulfillParams::new() },
            );
            tx("orao fulfill (synthetic loopback authority)", vec![ed25519, fulfill])
        }
        Act::CaptureDraw { close_id } => {
            let record = need_close(close_id)?;
            if now > record.promotion_deadline {
                return refuse(format!(
                    "the promotion deadline {} has passed (cluster clock {now}); a late capture is ignored, the close can only expire",
                    record.promotion_deadline
                ));
            }
            tx("capture_round_draw", vec![m7::capture_ix(w, close_id)])
        }
        Act::ApplyPromotion { close_id } => tx("apply_promotion", vec![m7::apply_ix(w, close_id)]),
        Act::ComposedReopen { close_id, plan, nonce } => {
            let new_round = pool.round_id + 1;
            let finalize = m7::finalize_ix(w, close_id);
            let page = page_ix(
                w,
                PageKind::Participant { round: new_round },
                0,
                rt.participant_address(new_round, 0),
                owner,
            );
            let open = open_ix_for_round(w, owner, new_round, plan, nonce, step.amount);
            tx("finalize_round + provision_page + open_position (one transaction)", vec![finalize, page, open])
        }
        Act::ExpireTooEarly { close_id } => {
            let record = need_close(close_id)?;
            if record.close_started_at == 0 || record.promotion != state::Promotion::Pending {
                return refuse(format!("close {close_id} is not waiting on its promotion"));
            }
            if now > record.promotion_deadline {
                return refuse(format!(
                    "the deadline {} has passed (cluster clock {now}): expiry would now SUCCEED, so the refusal evidence cannot be produced",
                    record.promotion_deadline
                ));
            }
            Ok(Next::Tx(TxSpec {
                label: "expire_promotion on/before the deadline (must be refused)".into(),
                instructions: vec![m7::expire_ix(w, close_id)],
                expect_failure: Some("DeadlineNotReached"),
                info: json!({ "promotion_deadline": record.promotion_deadline, "cluster_unix": now }),
            }))
        }
        Act::ExpirePromotion { close_id } => {
            let record = need_close(close_id)?;
            if now <= record.promotion_deadline {
                return refuse(format!(
                    "the deadline {} is not past (cluster clock {now}): wait {} more second(s)",
                    record.promotion_deadline,
                    record.promotion_deadline - now + 1
                ));
            }
            tx("expire_promotion", vec![m7::expire_ix(w, close_id)])
        }
        Act::FinalizeRound { close_id } => tx("finalize_round", vec![m7::finalize_ix(w, close_id)]),
    }
}

// ---------------------------------------------------------------------------
// Read-backs and post-conditions
// ---------------------------------------------------------------------------

fn pool_json(pool: &state::Pool) -> Value {
    json!({
        "round_id": pool.round_id, "pot": pool.pot, "seed": pool.seed, "management": pool.management,
        "pending_toll": pool.pending_toll, "funded_claims": pool.funded_claims.to_string(), "excess": pool.excess,
        "registry_len": pool.registry_len, "registry_pages": pool.registry_pages, "snapshot_pages": pool.snapshot_pages,
        "participant_count": pool.participant_count, "participant_pages": pool.participant_pages,
        "next_close_id": pool.next_close_id, "current_close_id": pool.current_close_id,
        "next_ticket": pool.next_ticket, "active_ticket": pool.active_ticket, "finalized_ticket": pool.finalized_ticket,
    })
}

fn backer_json(backer: &state::Backer) -> Value {
    json!({
        "amount": backer.amount.to_string(), "entitlement": backer.entitlement.to_string(),
        "lifetime": backer.lifetime.to_string(), "funded": backer.funded, "pending": backer.pending,
        "pending_ticket": backer.pending_ticket, "active": backer.active,
    })
}

fn close_json(record: &state::CloseRecord) -> Value {
    json!({
        "close_id": record.close_id, "round_id": record.round_id, "close_started_at": record.close_started_at,
        "promotion_deadline": record.promotion_deadline, "cash_committed": record.cash_committed,
        "participant_count": record.participant_count, "candidate_cursor": record.candidate_cursor,
        "preferred_count": record.preferred_count, "fallback_count": record.fallback_count, "sealed": record.sealed,
        "draw_state": record.draw_state, "request_account": record.request_account.to_string(),
        "captured_at": record.captured_at, "promotion": format!("{:?}", record.promotion),
        "finalized": record.finalized, "required_reserve": record.required_reserve,
        "locked_sponsor_remaining": record.locked_sponsor_remaining, "locked_owner_remaining": record.locked_owner_remaining,
        "provisioned_preferred_pages": record.provisioned_preferred_pages, "provisioned_fallback_pages": record.provisioned_fallback_pages,
        "reserve_settled": record.reserve_settled,
    })
}

/// The account read-backs the step's `expect` names, from the post-step cluster state.
fn readbacks(post: &View, steps: &[PlanStep], index: usize, actor_key: Pubkey) -> Value {
    let step = steps[index];
    let mut out = serde_json::Map::new();
    if let Some(pool) = post.pool() {
        out.insert("pool".into(), pool_json(&pool));
    }
    if let Some(ops) = post.ops() {
        out.insert(
            "ops".into(),
            json!({
                "free_balance": ops.free_balance, "locked_remaining_total": ops.locked_remaining_total,
                "unallocated_excess": ops.unallocated_excess, "total_contributed": ops.total_contributed.to_string(),
                "total_rent_funded": ops.total_rent_funded.to_string(), "request_rent_outstanding": ops.request_rent_outstanding,
            }),
        );
    }
    if let Some(ticket) = post.ticket() {
        out.insert("ticket".into(), json!({ "id": ticket.id, "phase": ticket.phase, "toll": ticket.toll, "close_id": ticket.close_id }));
    }
    if let Some(balance) = post.actor_balance {
        out.insert("actor_lamports".into(), json!(balance));
    }
    match step.act {
        Act::Open { nonce, .. } | Act::WithdrawSimple { nonce, .. } | Act::SettleCompound { nonce, .. } => {
            if let Some(position) = post.position(actor_key, nonce) {
                out.insert(
                    "position".into(),
                    json!({
                        "principal": position.principal, "opened_at": position.opened_at, "maturity_at": position.maturity_at,
                        "paid_gross": position.paid_gross.to_string(), "net_payout": position.net_payout.to_string(),
                        "claimed": position.claimed, "kind": position.kind,
                    }),
                );
            }
        }
        Act::FundSeriesA | Act::Claim { .. } | Act::AdvanceTicket { .. } => {
            for (name, series) in [("backer_a", 0u8), ("backer_b", 1u8)] {
                if let Some(backer) = post.backer(actor_key, series) {
                    out.insert(name.into(), backer_json(&backer));
                }
            }
        }
        _ => {}
    }
    if let Some(close_id) = act_close_id(steps, index) {
        if let Some(record) = post.close(close_id) {
            out.insert("close".into(), close_json(&record));
        }
    }
    Value::Object(out)
}

/// Post-conditions against the pre-step state: the facts the rehearsal asserts from events, restated as
/// state differences (a live cluster has no event log to read).  A failing check is reported in the
/// receipt and the exit code; the transactions have already been sent.
#[allow(clippy::too_many_arguments)]
fn post_checks(
    pre: &View,
    post: &View,
    wallets: &Wallets,
    steps: &[PlanStep],
    index: usize,
    actor_key: Pubkey,
    tx_count: usize,
    pre_info: &Value,
) -> Vec<Value> {
    let step = steps[index];
    let (Some(a), Some(b)) = (pre.pool(), post.pool()) else {
        return vec![check("pool.readable", false, "Pool before and after", "absent")];
    };
    let (Some(oa), Some(ob)) = (pre.ops(), post.ops()) else {
        return vec![check("ops.readable", false, "Ops before and after", "absent")];
    };
    let fee = SIGNATURE_FEE_LAMPORTS * tx_count as u64;
    let rent = |space: usize| post.w.rt.rent(space);
    let delta = match (pre.actor_balance, post.actor_balance) {
        (Some(before), Some(after)) => Some(i128::from(after) - i128::from(before)),
        _ => None,
    };
    let mut checks = Vec::new();
    let wallet_delta = |expected: i128, label: &str| -> Option<Value> {
        delta.map(|actual| {
            check(format!("{label}: actor lamport equation"), actual == expected, expected, actual)
        })
    };
    let existed = |w: &World, key: Pubkey| w.account(key).is_some_and(|account| account.lamports > 0);
    let load_of = |gross: u64| gross * 400 / 10_000;
    match step.act {
        Act::FundOperations => {
            checks.push(check("ops.free_balance += amount", ob.free_balance == oa.free_balance + step.amount, oa.free_balance + step.amount, ob.free_balance));
            checks.push(check("ops.total_contributed += amount", ob.total_contributed == oa.total_contributed + u128::from(step.amount), oa.total_contributed + u128::from(step.amount), ob.total_contributed));
            checks.extend(wallet_delta(-i128::from(step.amount) - i128::from(fee), "fund_operations"));
        }
        Act::ProvisionRegistry | Act::ProvisionSnapshot | Act::ProvisionParticipants { .. } => {
            let (name, before, after, space) = match step.act {
                Act::ProvisionRegistry => ("registry_pages", a.registry_pages, b.registry_pages, state::RegistryPage::SPACE),
                Act::ProvisionSnapshot => ("snapshot_pages", a.snapshot_pages, b.snapshot_pages, state::SnapshotPage::SPACE),
                _ => ("participant_pages", a.participant_pages, b.participant_pages, state::ParticipantPage::SPACE),
            };
            checks.push(check(format!("pool.{name} += 1"), after == before + 1, before + 1, after));
            checks.push(check("page rent comes from Ops.free_balance", ob.free_balance == oa.free_balance - rent(space), oa.free_balance - rent(space), ob.free_balance));
        }
        Act::ProvisionCloseRecord { close_id } => {
            checks.push(check("CloseRecord exists", post.close(close_id).is_some_and(|r| r.close_id == close_id), true, post.close(close_id).is_some()));
            checks.push(check("close record rent comes from Ops.free_balance", ob.free_balance == oa.free_balance - rent(state::CloseRecord::SPACE), oa.free_balance - rent(state::CloseRecord::SPACE), ob.free_balance));
        }
        Act::FundSeriesA => {
            let user_new = !existed(&pre.w, pre.rt().user_address(actor_key));
            let backer_new = pre.backer(actor_key, 0).is_none();
            let prior = pre.backer(actor_key, 0);
            let minted = fees::series_a_entitlement(step.amount).unwrap_or(0);
            checks.push(check("pot += gross (no load on Series A)", b.pot == a.pot + step.amount, a.pot + step.amount, b.pot));
            checks.push(check("management unchanged", b.management == a.management, a.management, b.management));
            checks.push(check("registry_len += 1 for a new row", b.registry_len == a.registry_len + u64::from(backer_new), a.registry_len + u64::from(backer_new), b.registry_len));
            match post.backer(actor_key, 0) {
                Some(backer) => {
                    checks.push(check("backer active", backer.active, true, backer.active));
                    let prior_amount = prior.as_ref().map_or(0, |p| p.amount);
                    let prior_entitlement = prior.as_ref().map_or(0, |p| p.entitlement);
                    checks.push(check("backer.amount += gross", backer.amount == prior_amount + u128::from(step.amount), prior_amount + u128::from(step.amount), backer.amount));
                    checks.push(check("backer.entitlement += 124 %", backer.entitlement == prior_entitlement + minted, prior_entitlement + minted, backer.entitlement));
                }
                None => checks.push(check("backer exists", false, true, false)),
            }
            let created = (if user_new { rent(state::User::SPACE) } else { 0 }) + (if backer_new { rent(state::Backer::SPACE) } else { 0 });
            checks.extend(wallet_delta(-i128::from(step.amount) - i128::from(fee) - i128::from(created), "fund_series_a"));
        }
        Act::Open { plan, nonce } => {
            let load = load_of(step.amount);
            let user_new = !existed(&pre.w, pre.rt().user_address(actor_key));
            let round_new = !existed(&pre.w, pre.rt().user_round_address(a.round_id, actor_key));
            checks.push(check("pot += 96 % of the deposit", b.pot == a.pot + step.amount - load, a.pot + step.amount - load, b.pot));
            checks.push(check("management += 4 % Front-End Load", b.management == a.management + load, a.management + load, b.management));
            checks.push(check("participant_count += 1 for a new participant", b.participant_count == a.participant_count + u64::from(round_new), a.participant_count + u64::from(round_new), b.participant_count));
            match post.position(actor_key, nonce) {
                Some(position) => {
                    checks.push(check("position.principal == gross", position.principal == step.amount, step.amount, position.principal));
                    checks.push(check("position.kind", position.kind == plan.discriminant(), plan.discriminant(), position.kind));
                    checks.push(check("position.maturity_at - opened_at == plan duration", position.maturity_at - position.opened_at == plan.duration_seconds(), plan.duration_seconds(), position.maturity_at - position.opened_at));
                    checks.push(check("position opened inside the step", position.opened_at as i64 >= pre.now && position.opened_at as i64 <= post.now, format!("{}..={}", pre.now, post.now), position.opened_at));
                }
                None => checks.push(check("position exists", false, true, false)),
            }
            let created = (if user_new { rent(state::User::SPACE) } else { 0 })
                + (if round_new { rent(state::UserRound::SPACE) } else { 0 })
                + rent(state::Position::SPACE);
            checks.extend(wallet_delta(-i128::from(step.amount) - i128::from(fee) - i128::from(created), "open_position"));
        }
        Act::WithdrawSimple { nonce, closing } | Act::SettleCompound { nonce, closing } => {
            let before_paid = pre.position(actor_key, nonce).map_or(0, |p| p.paid_gross);
            let after_paid = post.position(actor_key, nonce).map_or(0, |p| p.paid_gross);
            let paid = after_paid - before_paid;
            let expectation = &pre_info["expectation"];
            let expected_paid = expectation["paid_gross"].as_u64().unwrap_or(0);
            // Simple accrual runs per second (about 1,273 lamports/s per SOL of principal): a live cluster
            // executes the transaction a few seconds after the expectation's clock reading.
            let principal = post.position(actor_key, nonce).map_or(0, |p| p.principal);
            let seconds = (post.now - pre.now).max(0) as u128 + 15;
            let tolerance = if matches!(step.act, Act::WithdrawSimple { .. }) && !closing {
                u128::from(principal) * 11 * seconds / (100 * 86_400) + 2
            } else {
                0
            };
            let gap = paid.abs_diff(u128::from(expected_paid));
            checks.push(check("paid_gross matches the economics oracle", paid > 0 && gap <= tolerance, format!("{expected_paid} (+/- {tolerance})"), paid));
            checks.push(check("pot -= paid_gross", u128::from(b.pot) + paid == u128::from(a.pot), a.pot, b.pot));
            if closing {
                let close_id = closing_ordinal(steps, index);
                checks.push(check("pot-capped: paid the whole pot", paid == u128::from(a.pot), a.pot, paid));
                checks.push(check("pot is empty", b.pot == 0, 0, b.pot));
                checks.push(check("round is closing", b.current_close_id == Some(close_id), format!("{:?}", Some(close_id)), format!("{:?}", b.current_close_id)));
                checks.push(check("next_close_id += 1", b.next_close_id == close_id + 1, close_id + 1, b.next_close_id));
                match post.close(close_id) {
                    Some(record) => {
                        let reserve = pre_info["close_reserve"].as_u64().unwrap_or(0);
                        checks.push(check("deadline == close_started_at + 600", record.promotion_deadline == record.close_started_at + DEADLINE_SECS, record.close_started_at + DEADLINE_SECS, record.promotion_deadline));
                        checks.push(check("required_reserve == spec 8.1 reserve", record.required_reserve == reserve, reserve, record.required_reserve));
                        checks.push(check("sponsor funds the whole reserve", record.locked_sponsor_remaining == reserve && record.locked_owner_remaining == 0, format!("{reserve} / 0"), format!("{} / {}", record.locked_sponsor_remaining, record.locked_owner_remaining)));
                        checks.push(check("Ops.free_balance -= reserve", ob.free_balance == oa.free_balance - reserve, oa.free_balance - reserve, ob.free_balance));
                    }
                    None => checks.push(check("CloseRecord exists", false, true, false)),
                }
            } else {
                checks.push(check("round stays open", b.current_close_id.is_none(), "none", format!("{:?}", b.current_close_id)));
            }
            if expectation["toll"].as_u64().unwrap_or(0) > 0 {
                checks.push(check("the toll rides an allocation ticket", b.active_ticket != 0, "non-zero", b.active_ticket));
            }
            let net_before = pre.position(actor_key, nonce).map_or(0, |p| p.net_payout);
            let net_after = post.position(actor_key, nonce).map_or(0, |p| p.net_payout);
            let net_paid = net_after - net_before;
            checks.push(check("net + toll == paid_gross", net_paid <= paid, format!("net <= {paid}"), net_paid));
            checks.extend(wallet_delta(net_paid as i128 - i128::from(fee), "payout (destination = owner)"));
        }
        Act::AdvanceTicket { caps } => {
            let target = ticket_ordinal(steps, index);
            let ticket_toll = pre.ticket().map_or(0, |t| t.toll);
            checks.push(check("finalized_ticket == ticket", b.finalized_ticket == target, target, b.finalized_ticket));
            checks.push(check("no ticket in flight", b.active_ticket == 0, 0, b.active_ticket));
            checks.push(check("pending_toll -= ticket toll", b.pending_toll == a.pending_toll - ticket_toll, a.pending_toll - ticket_toll, b.pending_toll));
            checks.push(check("toll fully allocated (credited + seed)", u128::from(b.seed - a.seed) + (b.funded_claims - a.funded_claims) == u128::from(ticket_toll), ticket_toll, u128::from(b.seed - a.seed) + (b.funded_claims - a.funded_claims)));
            if let Some(who) = caps {
                if let Some(owner) = wallets.keys.get(&who) {
                    let capped = post.backer(*owner, 0).map(|backer| (backer.remaining().unwrap_or(u128::MAX) <= 10) && !backer.active);
                    checks.push(check(format!("{} reached 124 percent and left the registry", who.name()), capped == Some(true), true, format!("{capped:?}")));
                    checks.push(check("registry_len -= 1", b.registry_len + 1 == a.registry_len, a.registry_len - 1, b.registry_len));
                }
            }
        }
        Act::Claim { .. } => {
            let amount = pre_info["a_part"].as_u64().unwrap_or(0) + pre_info["b_part"].as_u64().unwrap_or(0);
            checks.push(check("funded_claims -= claimed amount", a.funded_claims - b.funded_claims == u128::from(amount), amount, a.funded_claims - b.funded_claims));
            checks.extend(wallet_delta(i128::from(amount) - i128::from(fee), "claim_backer"));
        }
        Act::CandidatePages { close_id } => {
            let (preferred, fallback) = pages_needed(post, close_id);
            let record = post.close(close_id);
            checks.push(check("both candidate sets are provisioned", record.as_ref().is_some_and(|r| r.provisioned_preferred_pages >= preferred.max(1) && r.provisioned_fallback_pages >= fallback.max(1)), format!("{preferred}/{fallback}"), format!("{:?}", record.as_ref().map(|r| (r.provisioned_preferred_pages, r.provisioned_fallback_pages)))));
        }
        Act::BuildCandidates { close_id } => {
            let record = post.close(close_id);
            let expected = m7::expected_candidates(&post.w, close_id);
            let (tag, list) = if expected.preferred.is_empty() { (1u8, &expected.fallback) } else { (0u8, &expected.preferred) };
            checks.push(check("candidate set sealed", record.as_ref().is_some_and(|r| r.sealed), true, record.as_ref().is_some_and(|r| r.sealed)));
            checks.push(check("selected set tag", record.as_ref().is_some_and(|r| r.selected_set_tag == tag), tag, record.as_ref().map_or(255, |r| r.selected_set_tag)));
            let page: Option<state::CandidatePage> = post.rt().try_deserialize(m7::candidate_page_key(close_id, tag, 0));
            let rows: Vec<Pubkey> = page.map(|p| p.rows[..usize::from(p.len)].to_vec()).unwrap_or_default();
            let expected_rows: Vec<Pubkey> = list.iter().map(|(owner, _)| *owner).take(state::PAGE_CAPACITY).collect();
            checks.push(check("page 0 lists the independently derived candidates", rows == expected_rows, format!("{expected_rows:?}"), format!("{rows:?}")));
        }
        Act::RequestDraw { close_id } => {
            let record = post.close(close_id);
            checks.push(check("draw requested", record.as_ref().is_some_and(|r| r.draw_state == state::DRAW_REQUESTED), state::DRAW_REQUESTED, record.as_ref().map_or(255, |r| r.draw_state)));
            checks.push(check("inside the deadline", record.as_ref().is_some_and(|r| post.now <= r.promotion_deadline as i64 + 5), "now <= deadline", post.now));
        }
        Act::Fulfil { close_id } => {
            checks.push(check("the ORAO request is fulfilled", post.fulfilled.get(&close_id).copied().unwrap_or(false), true, post.fulfilled.get(&close_id).copied().unwrap_or(false)));
            // The Ed25519 precompile signature is charged like a transaction signature.
            let paid = if tx_count > 0 { 2 * i128::from(fee) } else { 0 };
            checks.extend(wallet_delta(-paid, "orao fulfill (transaction + precompile signature)"));
        }
        Act::CaptureDraw { close_id } => {
            let record = post.close(close_id);
            checks.push(check("draw captured", record.as_ref().is_some_and(|r| r.draw_state == state::DRAW_CAPTURED), state::DRAW_CAPTURED, record.as_ref().map_or(255, |r| r.draw_state)));
        }
        Act::ApplyPromotion { close_id } => {
            let record = post.close(close_id);
            let promoted = match record.as_ref().map(|r| r.promotion) {
                Some(state::Promotion::Applied { winner, entitlement }) => Some((winner, entitlement)),
                _ => None,
            };
            let entitlement = u128::from(m14_plan::CANDIDATE_GAME) * 11_600 / 10_000;
            checks.push(check("promotion applied", promoted.is_some(), true, promoted.is_some()));
            if let Some((winner, got)) = promoted {
                checks.push(check("the winner is one of P4 / P5", [Who::P4, Who::P5].iter().any(|who| wallets.keys[who] == winner), "P4 or P5", winner));
                checks.push(check("Series B entitlement is 116 % of the loss", got == entitlement, entitlement, got));
                let b_row = post.backer(winner, 1);
                checks.push(check("the winner's B row is active", b_row.as_ref().is_some_and(|row| row.active && row.entitlement == entitlement), true, b_row.is_some()));
                checks.push(check("the B row joined the registry", b.registry_len == a.registry_len + 1, a.registry_len + 1, b.registry_len));
            }
        }
        Act::ComposedReopen { close_id, plan, nonce } => {
            let load = load_of(step.amount);
            checks.push(check("round advanced", b.round_id == a.round_id + 1, a.round_id + 1, b.round_id));
            checks.push(check("the seed carries into the pot once", b.pot == a.pot + a.seed + step.amount - load, a.pot + a.seed + step.amount - load, b.pot));
            checks.push(check("seed reset", b.seed == 0, 0, b.seed));
            checks.push(check("no close in flight", b.current_close_id.is_none(), "none", format!("{:?}", b.current_close_id)));
            checks.push(check("one participant on one page", b.participant_count == 1 && b.participant_pages == 1, "1/1", format!("{}/{}", b.participant_count, b.participant_pages)));
            checks.push(check("close finalized", post.close(close_id).is_some_and(|r| r.finalized), true, post.close(close_id).is_some_and(|r| r.finalized)));
            checks.push(check("Ops pays the new round's page", ob.free_balance == oa.free_balance - rent(state::ParticipantPage::SPACE), oa.free_balance - rent(state::ParticipantPage::SPACE), ob.free_balance));
            let user_new = !existed(&pre.w, pre.rt().user_address(actor_key));
            let created = (if user_new { rent(state::User::SPACE) } else { 0 }) + rent(state::UserRound::SPACE) + rent(state::Position::SPACE);
            checks.push(check("the position opened", post.position(actor_key, nonce).is_some_and(|p| p.principal == step.amount && p.kind == plan.discriminant()), true, post.position(actor_key, nonce).is_some()));
            checks.extend(wallet_delta(-i128::from(step.amount) - i128::from(fee) - i128::from(created), "composed finalize/reopen"));
        }
        Act::ExpireTooEarly { .. } => {}
        Act::ExpirePromotion { close_id } => {
            let record = post.close(close_id);
            let skipped = matches!(record.as_ref().map(|r| r.promotion), Some(state::Promotion::Skipped { reason, .. }) if reason == pclose::REASON_DEADLINE);
            checks.push(check("promotion skipped for the deadline", skipped, "Skipped(Deadline)", record.as_ref().map_or("absent".into(), |r| format!("{:?}", r.promotion))));
            checks.push(check("reserve settled", record.as_ref().is_some_and(|r| r.reserve_settled), true, record.as_ref().is_some_and(|r| r.reserve_settled)));
        }
        Act::FinalizeRound { close_id } => {
            checks.push(check("round advanced", b.round_id == a.round_id + 1, a.round_id + 1, b.round_id));
            checks.push(check("seed folded into the pot", b.pot == a.pot + a.seed && b.seed == 0, a.pot + a.seed, b.pot));
            checks.push(check("no close, no participants", b.current_close_id.is_none() && b.participant_count == 0, "none/0", format!("{:?}/{}", b.current_close_id, b.participant_count)));
            checks.push(check("close finalized", post.close(close_id).is_some_and(|r| r.finalized), true, post.close(close_id).is_some_and(|r| r.finalized)));
        }
    }
    // Every other step moves nothing but fees for its actor (rent comes from Ops or the locked reserve).
    if !checks.iter().any(|entry| entry["name"].as_str().is_some_and(|name| name.contains("actor lamport equation"))) {
        checks.extend(wallet_delta(-i128::from(fee), step.id));
    }
    checks
}

// ---------------------------------------------------------------------------
// The oracle latency (packet A7)
// ---------------------------------------------------------------------------

/// Request -> fulfilment latency from the request account's own transaction history: the oldest
/// successful signature created it, the next one fulfilled it.
fn oracle_latency(ctx: &Ctx<'_>, view: &View, close_id: u64, waited: Option<f64>) -> Value {
    let Some(record) = view.close(close_id) else { return Value::Null };
    let history = match ctx.cluster.signatures_for_address(&record.request_account) {
        Ok(history) => history,
        Err(error) => return json!({ "request_account": record.request_account.to_string(), "latency": null, "error": error }),
    };
    let mut ascending: Vec<&AddressSignature> = history.iter().filter(|entry| !entry.failed).collect();
    ascending.sort_by_key(|entry| entry.slot);
    let (Some(request), Some(fulfil)) = (ascending.first(), ascending.get(1)) else {
        return json!({ "request_account": record.request_account.to_string(), "latency": null, "error": "fewer than two successful signatures on the request account" });
    };
    let seconds = match (request.block_time, fulfil.block_time) {
        (Some(from), Some(to)) => Some(to - from),
        _ => None,
    };
    json!({
        "request_account": record.request_account.to_string(),
        "request_signature": request.signature, "request_slot": request.slot, "request_block_time": request.block_time,
        "fulfil_signature": fulfil.signature, "fulfil_slot": fulfil.slot, "fulfil_block_time": fulfil.block_time,
        "latency_slots": fulfil.slot - request.slot,
        "latency_seconds": seconds,
        "tool_waited_seconds": waited,
        "deadline_margin_seconds": fulfil.block_time.map(|time| record.promotion_deadline as i64 - time),
        "source": "getSignaturesForAddress(request account)",
    })
}

// ---------------------------------------------------------------------------
// flow next
// ---------------------------------------------------------------------------

fn cmd_next(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let wallets = load_wallets(&wallet_dir(opts)?)?;
    let step_id = opts.require("step")?;
    let steps = m14_plan::plan();
    let index = steps
        .iter()
        .position(|candidate| candidate.id == step_id)
        .ok_or_else(|| OpsError::Usage(format!("unknown step `{step_id}`; `flow status` lists the plan")))?;
    let step = steps[index];
    let receipts_file = receipts_path(opts, &wallets)?;
    let accepted = accepted_failed_checks(opts)?;

    let view = load_view(ctx, None)?;
    let genesis = genesis_of(ctx, &view)?.ok_or_else(|| {
        OpsError::Refused("genesis is unknown: `initialize` has not run on this cluster (ConfigVersion#1 is not activated)".into())
    })?;
    let stamp = stamp_of(ctx, &view).ok_or_else(|| {
        OpsError::Refused("ConfigVersion#1 carries no activation stamp: `initialize` has not run on this cluster".into())
    })?;
    // Receipts are scoped to this cluster: a log carried over from another cluster (or an older
    // `--reset` ledger of the same one) is refused, never silently reused.
    let receipts = read_receipts(&receipts_file, Some(&stamp))?;
    if receipts.foreign > 0 {
        return refuse(format!(
            "{} holds {} receipt(s) stamped for ANOTHER cluster or activation (this one is genesis {}, program {}, \
             ConfigVersion#1 activated at {}); use a fresh --receipts file for this cluster",
            receipts_file.display(),
            receipts.foreign,
            stamp.genesis_hash,
            stamp.program_id,
            stamp.activation_unix
        ));
    }
    let blocking = unaccepted_failures(&receipts, &accepted);
    if !blocking.is_empty() {
        return refuse(format!(
            "step(s) {blocking:?} were recorded with FAILED CHECKS (result `done_with_failed_checks`); the plan does not \
             advance past them until the receipt has been examined: re-run with --accept-failed-checks {}",
            blocking.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(",")
        ));
    }
    let (evals, winner) = evaluate(&view, &wallets, &receipts);
    // The oracle step completes BETWEEN invocations (the real oracle fulfils on its own time), so its
    // latency record is written by a later `flow next` that merely observes the fulfilment.
    let observation_only = matches!(step.act, Act::Fulfil { .. })
        && evals[index].done
        && !receipts.has(step.id)
        && done_by_state(&view, &wallets, &steps, index, winner).0;
    if evals[index].done && !observation_only {
        return refuse(format!("step `{}` is already complete ({})", step.id, evals[index].evidence));
    }
    let next = evals.iter().find(|eval| !eval.done).map_or(steps.len(), |eval| eval.index);
    if !observation_only && next != index {
        let next = &evals[next];
        return refuse(format!(
            "step `{}` (#{}) is not the next step: `{}` (#{}) is ({}); steps run strictly in plan order",
            step.id,
            index + 1,
            next.step.id,
            next.index + 1,
            next.evidence
        ));
    }
    // The plain funding transfers are NOT idempotent (a second one adds again).  When the cluster's own
    // state already says "done" the refusal above fires; this is the guard for a node that does NOT say it
    // (a lagging replica): the write-ahead file's CONFIRMED record for the step stops the second transfer
    // whatever any RPC node answers.
    if matches!(step.act, Act::FundOperations | Act::FundSeriesA) {
        ctx.refuse_repeat_of_confirmed(step.id)?;
    }
    let earliest = genesis.genesis_unix + step.at as i64;
    if !observation_only && view.now < earliest {
        return refuse(format!(
            "too early: step `{}` is at offset {} s, i.e. no earlier than {} ({earliest}); the cluster clock is {} ({} s to go)",
            step.id,
            step.at,
            utc(earliest),
            view.now,
            earliest - view.now
        ));
    }
    let books = conservation(&view);
    if !conservation_ok(&books) {
        return refuse(format!("the books do not balance before the step; stopping: {books}"));
    }
    let actor = resolve(step.who, winner)
        .ok_or_else(|| OpsError::Refused("the promotion winner is not known yet".into()))?;
    let actor_file = load_actor(&wallets, actor)?;
    let actor_kp = actor_file.keypair()?;
    let actor_key = actor_kp.pubkey();

    let oracle_file;
    let local_oracle: Option<&Keypair> = if opts.switch("local-oracle") {
        if ctx.gate.target != Target::Local {
            return refuse("--local-oracle is for a loopback validator only; on devnet the real ORAO oracle fulfils");
        }
        let path = wallets.dir.join(format!("{ORACLE_NAME}.json"));
        oracle_file = KeyFile::load(&path.display().to_string())?;
        if let Some(recorded) = wallets.oracle {
            if recorded != oracle_file.pubkey {
                return refuse("ORACLE.json does not match the key recorded in wallets.json");
            }
        }
        Some(oracle_file.keypair()?)
    } else {
        None
    };
    let wait_seconds = opts.u64_flag("wait-seconds")?.unwrap_or(0);
    let wait_started = Instant::now();
    // The state the step starts from, WITH the actor's balance (the checks compare against it).
    let pre = load_view(ctx, Some(actor_kp))?;

    let mut txs: Vec<Value> = Vec::new();
    let mut info = Value::Null;
    let mut refused_as_expected: Option<Value> = None;
    let mut dry_run_preview: Option<Value> = None;
    let mut pending: Option<String> = None;
    let mut fulfilled_after_wait = false;
    let mut sent = 0usize;
    loop {
        if sent > MAX_TXS_PER_STEP {
            return fail(format!("step `{}` needed more than {MAX_TXS_PER_STEP} transactions; stopping", step.id));
        }
        let current = load_view(ctx, Some(actor_kp))?;
        match next_tx(&current, &wallets, &steps, index, winner, actor_key, local_oracle)? {
            Next::Done => break,
            Next::Wait(reason) => {
                if !ctx.executing() {
                    dry_run_preview = Some(json!({ "would_wait": reason }));
                    break;
                }
                if wait_started.elapsed().as_secs() >= wait_seconds {
                    pending = Some(reason);
                    break;
                }
                progress(format!("{reason}; polling every 2 s (up to {wait_seconds} s)"));
                std::thread::sleep(std::time::Duration::from_secs(2));
                fulfilled_after_wait = true;
            }
            Next::Tx(spec) => {
                if info.is_null() {
                    info = spec.info.clone();
                }
                let tx_step = Step {
                    id: step.id.to_string(),
                    label: format!("{}: {}", step.id, spec.label),
                    instructions: spec.instructions,
                    payer: actor_key,
                    cu_limit: Some(CU_FLOW),
                    depends_on_previous: false,
                };
                if let Some(error_name) = spec.expect_failure {
                    let built = build_step(&tx_step);
                    if !ctx.executing() {
                        let result = ctx.run_step(&tx_step, &[])?;
                        dry_run_preview = Some(json!({ "transaction": result.to_json(), "must_be_refused_with": error_name }));
                        break;
                    }
                    let blockhash = ctx.cluster.blockhash().map_err(|error| rpc_err("getLatestBlockhash", error))?;
                    let signed = sign_checked(&tx_step.label, &built, &[actor_kp], blockhash)?;
                    let simulation = ctx
                        .cluster
                        .simulate(&signed, true)
                        .map_err(|error| rpc_err("simulateTransaction", error))?;
                    let marker = format!("Error Code: {error_name}");
                    if simulation.error.is_some() && simulation.logs.iter().any(|line| line.contains(&marker)) {
                        refused_as_expected = Some(json!({
                            "error": error_name, "simulation_error": simulation.error,
                            "packet_bytes": built.packet_bytes, "logs_tail": simulation.logs.iter().rev().take(4).rev().collect::<Vec<_>>(),
                            "sent": false,
                            "note": "the program refused the transaction as designed; nothing was sent (a failing transaction would only burn a fee)",
                        }));
                        break;
                    }
                    return refuse(format!(
                        "{}: the simulation was expected to fail with {error_name} but returned {:?}; nothing was sent",
                        tx_step.label, simulation.error
                    ));
                }
                let result = ctx.run_step(&tx_step, &[actor_kp])?;
                sent += 1;
                // `to_json` carries the signature and the slot `run_step` learned (never silently absent).
                txs.push(result.to_json());
                if ctx.executing() && is_single_tx(step.act) {
                    // One confirmed send completes this step; never loop on a read that may still be
                    // pre-send (a second send of a deposit or funding would be a real double spend).
                    break;
                }
                if !ctx.executing() {
                    dry_run_preview = Some(json!({
                        "next_transaction": txs.pop(),
                        "note": "a dry run builds only the first transaction; later ones depend on the chain after it",
                        "info": info,
                    }));
                    break;
                }
            }
        }
    }

    if let Some(reason) = pending {
        return Ok(json!({
            "command": "flow next", "mode": ctx.mode(), "step": step.id, "actor": actor.name(),
            "result": "pending", "reason": reason, "waited_seconds": wait_started.elapsed().as_secs(),
            "note": "no transaction was sent; re-run when the oracle has fulfilled (--wait-seconds N polls)",
            "all_ok": false,
        }));
    }
    if !ctx.executing() && dry_run_preview.is_none() {
        dry_run_preview = Some(json!({
            "note": "nothing to send: the step's effect is already on chain; a real run would record the observation in the receipt log",
        }));
    }
    if let Some(preview) = dry_run_preview {
        return Ok(json!({
            "command": "flow next", "mode": "dry-run", "plan": PLAN_NAME, "step": step.id, "index": index + 1,
            "flows": step.flows, "actor": actor.name(), "actor_pubkey": actor_key.to_string(),
            "expect": step.expect, "earliest_unix": earliest, "earliest_utc": utc(earliest), "cluster_unix": view.now,
            "preview": preview, "conservation": books, "all_ok": true,
        }));
    }

    // ---- the step is complete: read back, check, record ------------------------------------------
    let post = load_view(ctx, Some(actor_kp))?;
    let post_books = conservation(&post);
    // Fees are charged only for transactions actually sent: a step refused as designed is simulated, not
    // sent, so its expected actor delta is zero (receipt truthfulness).
    let mut checks = post_checks(&pre, &post, &wallets, &steps, index, actor_key, txs.len(), &info);
    if let Some(sim) = &refused_as_expected {
        checks.push(check("refused by the program as designed", true, "DeadlineNotReached", sim["error"].clone()));
    }
    checks.push(check("both conservation identities and the partition identity", conservation_ok(&post_books), true, conservation_ok(&post_books)));
    let oracle = match step.act {
        Act::Fulfil { close_id } => Some(oracle_latency(ctx, &post, close_id, fulfilled_after_wait.then(|| wait_started.elapsed().as_secs_f64()))),
        _ => None,
    };
    let all_ok = checks_ok(&checks);
    let result = if !all_ok {
        "done_with_failed_checks"
    } else if refused_as_expected.is_some() {
        RESULT_REFUSED_AS_EXPECTED
    } else {
        "done"
    };
    let receipt = json!({
        "schema": 1, "plan": PLAN_NAME, "step": step.id, "index": index + 1, "flows": step.flows,
        "result": result, "mode": ctx.mode(), "cluster": ctx.cluster_json(&ctx.genesis),
        "stamp": stamp.to_json(), "packet": ctx.packet_json(),
        "actor": actor.name(), "actor_pubkey": actor_key.to_string(), "winner": winner.map(|who| who.name()),
        "expect": step.expect, "offset_s": step.at, "earliest_unix": earliest,
        "cluster_unix_before": view.now, "cluster_unix_after": post.now, "slot_after": post.slot,
        "transactions": txs, "refused_as_expected": refused_as_expected,
        "pre_state": info, "readbacks": readbacks(&post, &steps, index, actor_key),
        "checks": checks, "conservation": post_books, "oracle": oracle, "all_ok": all_ok,
    });
    append_receipt(&receipts_file, &receipt)?;
    let mut shown = receipt;
    shown["command"] = json!("flow next");
    shown["receipt_file"] = json!(receipts_file.display().to_string());
    Ok(shown)
}

fn append_receipt(path: &Path, receipt: &Value) -> Result<(), OpsError> {
    append_json_line(path, receipt)
}
