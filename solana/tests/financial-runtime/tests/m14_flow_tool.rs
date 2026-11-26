//! M14-RUN (packet RG-4, the flow runner): the whole M14 flow plan, driven step by step through the
//! operator tool's `devnet_ops flow next` code path (`src/devnet_ops/flow.rs`) against a LiteSVM-backed
//! cluster, with the production SBF ELF, the DEVNET ORAO binary and the Squads stand-in.
//!
//! `tests/m14_flow_plan.rs` executes the same plan (`musical_chairs_financial_runtime::m14_plan`) with
//! the harness' direct helpers.  Here nothing calls those helpers: every transaction is built by the
//! runner from the cluster's accounts (a mirror `World` fed by `getProgramAccounts`), measured, simulated
//! and sent through the tool's interlocks, and the outcomes the rehearsal asserts are asserted again
//! from the transactions' events and the runner's receipts.
//!
//! * `the_whole_m14_flow_plan_runs_through_flow_next_on_the_devnet_binaries` is `#[ignore]` and needs
//!   `MC_ORAO_PROFILE=devnet` (like `m14_flow_plan`).
//! * `the_f3_window_guard_and_a_late_open_still_reach_every_later_check` is `#[ignore]` as well: A1 opens
//!   3,700 s late (beyond the 1 h allowance), `flow next` refuses F3 before `F3_MIN_ELAPSED` after A1's
//!   on-chain open, accepts it at exactly that elapsed, and the other 38 steps still pass their checks
//!   (F6 included, with F3's payout equal to the pure arithmetic at that elapsed).  The late open also
//!   covers the late close and the maturities: close 1 is refused at the plan's instant and one second
//!   short of A1's ACTUAL strict-insolvency threshold and sent at the threshold, the compounding
//!   settlements wait for the positions' actual maturities (read from chain), and step 38 waits for
//!   `now > deadline`.
//! * `the_current_devnet_run_closes_at_its_threshold_and_every_later_check_holds` is `#[ignore]` as well: it
//!   replays the current devnet run's ACTUAL timings for steps 1-12 (A1 opened 132 s late, F3 at elapsed
//!   358,784), checks the recorded devnet facts, then runs step 13 exactly at the threshold and steps
//!   14-39 on the new schedule.
//! * `a_missed_refusal_window_is_reported_and_never_counted_done` is `#[ignore]` as well: the expected
//!   refusal (step 37) is left until the deadline has passed; `flow status` lists it as missed, nothing
//!   counts it done (not the passed deadline, not a later step that somebody else ran) and the plan stops.
//! * The gate and refusal tests (out of order, too early, wrong wallet, devnet without the packet
//!   identity pin, `--local-oracle` off loopback, the unfulfilled oracle, the upper end of the F3 window,
//!   a fresh status without missed evidence) run in a plain `cargo test`.
//!
//! ```text
//! MC_ORAO_PROFILE=devnet cargo +1.92.0 test --locked --test m14_flow_tool -- --ignored --nocapture
//! cargo +1.92.0 test --locked --test m14_flow_tool            # the refusal tests
//! ```
#![allow(clippy::result_large_err)]

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use musical_chairs_economics::{earnings::SimpleAccrual, fees};
use musical_chairs_financial_runtime::{
    close as m7,
    devnet::{self, OraoProfile},
    devnet_ops::{self as ops, AddressSignature, Cluster, OpsError, SendError, SendOutcome, SigOutcome, SimOutcome},
    m14_plan::{self, Act, Who, ALL_WALLETS},
    runtime::{self, SignedTransaction},
    world::{World, START_TIME},
    Observation,
};
use musical_chairs_program::{
    close as pclose, state, BackerClaimed, CandidatesSealed, DrawCaptured, DrawRequested, OperationsFunded,
    PayoutPaid, PositionOpened, PromotionApplied, PromotionSkipped, RoundClosed, RoundFinalized,
    SeriesAFunded, TicketCommitted, ID,
};
use serde_json::Value;
use solana_sdk::{
    account::{Account, ReadableAccount},
    clock::Clock,
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

const RPC: &str = "http://127.0.0.1:8899";
const DEVNET_URL: &str = "https://api.devnet.solana.com";
const LITESVM_GENESIS: &str = "LiteSVMLocalGenesis11111111111111111111111111";

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "mc-flow-tool-{tag}-{}-{}",
            std::process::id(),
            DIR_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }
    fn file(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
    fn dir(&self) -> String {
        self.0.display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_keypair(path: &str, keypair: &Keypair) {
    fs::write(path, serde_json::to_string(&keypair.to_bytes().to_vec()).expect("json")).expect("write keypair");
}

/// A sent transaction, for `signatures_for_address` and `signature_slot`.
struct SentTx {
    signature: Signature,
    slot: u64,
    unix: i64,
    accounts: Vec<Pubkey>,
}

/// A [`Cluster`] over the harness' `World` (LiteSVM).  It runs transactions through
/// `World::submit`, so every send is recorded as an `Observation` (events included) and both cash
/// registers are asserted after it, exactly like the rehearsal's own sends.
struct WorldCluster<'a> {
    world: RefCell<&'a mut World>,
    genesis: RefCell<String>,
    sims: Cell<usize>,
    sends: Cell<usize>,
    genesis_reads: Cell<usize>,
    observations: RefCell<Vec<Observation>>,
    sent: RefCell<Vec<SentTx>>,
}

impl<'a> WorldCluster<'a> {
    fn new(world: &'a mut World) -> Self {
        WorldCluster {
            world: RefCell::new(world),
            genesis: RefCell::new(LITESVM_GENESIS.to_string()),
            sims: Cell::new(0),
            sends: Cell::new(0),
            genesis_reads: Cell::new(0),
            observations: RefCell::new(Vec::new()),
            sent: RefCell::new(Vec::new()),
        }
    }
    fn set_genesis(&self, genesis: &str) {
        *self.genesis.borrow_mut() = genesis.to_string();
    }
    fn warp_to(&self, unix: u64) {
        self.world.borrow_mut().warp_to(unix);
    }
    fn now(&self) -> u64 {
        self.world.borrow().now()
    }
    fn with_world<T>(&self, f: impl FnOnce(&World) -> T) -> T {
        f(&self.world.borrow())
    }
    fn observed(&self) -> usize {
        self.observations.borrow().len()
    }
}

impl Cluster for WorldCluster<'_> {
    fn genesis_hash(&self) -> Result<String, String> {
        self.genesis_reads.set(self.genesis_reads.get() + 1);
        Ok(self.genesis.borrow().clone())
    }
    fn blockhash(&self) -> Result<Hash, String> {
        let mut world = self.world.borrow_mut();
        world.rt.svm.expire_blockhash();
        Ok(world.rt.svm.latest_blockhash())
    }
    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String> {
        Ok(self.world.borrow().rt.svm.get_account(key).filter(|account| account.lamports > 0))
    }
    fn rent_minimum(&self, data_len: usize) -> Result<u64, String> {
        Ok(self.world.borrow().rt.svm.minimum_balance_for_rent_exemption(data_len))
    }
    fn unix_time(&self) -> Result<i64, String> {
        Ok(self.world.borrow().rt.svm.get_sysvar::<Clock>().unix_timestamp)
    }
    fn slot(&self) -> Result<u64, String> {
        Ok(self.world.borrow().rt.svm.get_sysvar::<Clock>().slot)
    }
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String> {
        if !sig_verify {
            return Err("the LiteSVM double cannot simulate unsigned transactions".into());
        }
        self.sims.set(self.sims.get() + 1);
        match self.world.borrow().rt.svm.simulate_transaction(tx.clone()) {
            Ok(info) => Ok(SimOutcome {
                error: None,
                logs: info.meta.logs,
                units_consumed: Some(info.meta.compute_units_consumed),
            }),
            Err(failed) => Ok(SimOutcome {
                error: Some(format!("{:?}", failed.err)),
                logs: failed.meta.logs,
                units_consumed: Some(failed.meta.compute_units_consumed),
            }),
        }
    }
    fn send(&self, tx: &Transaction) -> Result<SendOutcome, SendError> {
        self.sends.set(self.sends.get() + 1);
        let mut world = self.world.borrow_mut();
        let clock: Clock = world.rt.svm.get_sysvar();
        let label = format!("flow.{}", self.sends.get());
        match world.submit(&label, &SignedTransaction::from_signed(tx.clone())) {
            Ok(observation) => {
                let units = observation.compute_units;
                self.sent.borrow_mut().push(SentTx {
                    signature: tx.signatures[0],
                    slot: clock.slot,
                    unix: clock.unix_timestamp,
                    accounts: tx.message.account_keys.clone(),
                });
                self.observations.borrow_mut().push(observation);
                Ok(SendOutcome { signature: tx.signatures[0], units_consumed: units })
            }
            Err(observation) => Err(SendError::Rejected(format!("{:?}: {:?}", observation.error, observation.logs))),
        }
    }
    fn signature_outcome(&self, signature: &Signature) -> Result<SigOutcome, String> {
        Ok(self
            .sent
            .borrow()
            .iter()
            .find(|sent| sent.signature == *signature)
            .map_or(SigOutcome::Unknown, |sent| SigOutcome::Confirmed { slot: sent.slot }))
    }
    fn program_accounts(&self, program: &Pubkey) -> Result<Vec<(Pubkey, Account)>, String> {
        let world = self.world.borrow();
        Ok(world
            .rt
            .svm
            .accounts_db()
            .inner
            .iter()
            .filter(|(_, account)| account.owner() == program && account.lamports() > 0)
            .map(|(key, account)| (*key, Account::from(account.clone())))
            .collect())
    }
    fn signature_slot(&self, signature: &Signature) -> Result<Option<u64>, String> {
        Ok(self.sent.borrow().iter().find(|sent| sent.signature == *signature).map(|sent| sent.slot))
    }
    fn signatures_for_address(&self, key: &Pubkey) -> Result<Vec<AddressSignature>, String> {
        Ok(self
            .sent
            .borrow()
            .iter()
            .rev()
            .filter(|sent| sent.accounts.contains(key))
            .map(|sent| AddressSignature {
                signature: sent.signature.to_string(),
                slot: sent.slot,
                block_time: Some(sent.unix),
                failed: false,
            })
            .collect())
    }
}

/// A throwaway git repository holding a "packet" at the tool's fixed packet path.
struct PacketRepo {
    dir: TempDir,
    commit: String,
    sha: String,
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=packet-test", "-c", "user.email=packet@test.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

impl PacketRepo {
    fn new(content: &str) -> PacketRepo {
        let dir = TempDir::new("packet-repo");
        let root = PathBuf::from(dir.dir());
        git(&root, &["init", "-q"]);
        let file = root.join(ops::DEFAULT_PACKET_FILE);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, content).unwrap();
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "packet"]);
        let commit = git(&root, &["rev-parse", "HEAD"]);
        PacketRepo { dir, commit, sha: devnet::sha256_hex(content.as_bytes()) }
    }
    fn root(&self) -> PathBuf {
        PathBuf::from(self.dir.dir())
    }
    fn pin_flags(&self) -> Vec<String> {
        vec!["--packet-sha256".into(), self.sha.clone(), "--packet-commit".into(), self.commit.clone()]
    }
}

/// A wrapper that misbehaves like a real RPC fleet: after a send the next `stale_budget` account /
/// program-account reads answer from the PRE-SEND state (a load-balanced node that has not seen the
/// transaction yet), and the next `ambiguous_sends` sends report an unknown outcome (a timeout) --
/// landing the transaction anyway when `land_ambiguous` is set.
struct Hooked<'a, 'b> {
    inner: &'a WorldCluster<'b>,
    stale_budget: Cell<usize>,
    stale_reads: Cell<usize>,
    snapshot: RefCell<BTreeMap<Pubkey, Account>>,
    ambiguous_sends: Cell<usize>,
    land_ambiguous: Cell<bool>,
    blockhash_still_valid: Cell<bool>,
    /// The status lookup finds NOTHING for a transaction that did land (the node's status cache evicted
    /// it and its history does not answer): a missing status the tool must never read as "never landed".
    hide_status: Cell<bool>,
    /// The status lookup ERRORS (a transport or node failure) although the transaction landed.
    fail_status: Cell<bool>,
    noted_slot: Cell<u64>,
    /// A node that honours `minContextSlot`: once the tool has handed it the slot of a confirmed
    /// transaction (`note_confirmed_slot`) it never answers from the pre-send snapshot again.  Off, it is
    /// the lagging replica that ignores the floor.
    honour_min_slot: Cell<bool>,
}

impl<'a, 'b> Hooked<'a, 'b> {
    fn new(inner: &'a WorldCluster<'b>) -> Self {
        Hooked {
            inner,
            stale_budget: Cell::new(0),
            stale_reads: Cell::new(0),
            snapshot: RefCell::new(BTreeMap::new()),
            ambiguous_sends: Cell::new(0),
            land_ambiguous: Cell::new(false),
            blockhash_still_valid: Cell::new(false),
            hide_status: Cell::new(false),
            fail_status: Cell::new(false),
            noted_slot: Cell::new(0),
            honour_min_slot: Cell::new(false),
        }
    }
    fn take_snapshot(&self) {
        let world = self.inner.world.borrow();
        *self.snapshot.borrow_mut() = world
            .rt
            .svm
            .accounts_db()
            .inner
            .iter()
            .filter(|(_, account)| account.lamports() > 0)
            .map(|(key, account)| (*key, Account::from(account.clone())))
            .collect();
    }
    fn stale(&self) -> bool {
        if self.honour_min_slot.get() && self.noted_slot.get() > 0 {
            return false;
        }
        let left = self.stale_reads.get();
        if left == 0 {
            return false;
        }
        self.stale_reads.set(left - 1);
        true
    }
}

impl Cluster for Hooked<'_, '_> {
    fn genesis_hash(&self) -> Result<String, String> {
        self.inner.genesis_hash()
    }
    fn blockhash(&self) -> Result<Hash, String> {
        self.inner.blockhash()
    }
    fn blockhash_valid(&self, _hash: &Hash) -> Result<bool, String> {
        Ok(self.blockhash_still_valid.get())
    }
    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String> {
        if self.stale() {
            return Ok(self.snapshot.borrow().get(key).cloned());
        }
        self.inner.account(key)
    }
    fn rent_minimum(&self, data_len: usize) -> Result<u64, String> {
        self.inner.rent_minimum(data_len)
    }
    fn unix_time(&self) -> Result<i64, String> {
        self.inner.unix_time()
    }
    fn slot(&self) -> Result<u64, String> {
        self.inner.slot()
    }
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String> {
        self.inner.simulate(tx, sig_verify)
    }
    fn send(&self, tx: &Transaction) -> Result<SendOutcome, SendError> {
        self.take_snapshot();
        if self.ambiguous_sends.get() > 0 {
            self.ambiguous_sends.set(self.ambiguous_sends.get() - 1);
            if self.land_ambiguous.get() {
                let _ = self.inner.send(tx);
            }
            self.stale_reads.set(self.stale_budget.get());
            return Err(SendError::Unknown("simulated timeout".into()));
        }
        let outcome = self.inner.send(tx);
        self.stale_reads.set(self.stale_budget.get());
        outcome
    }
    fn signature_outcome(&self, signature: &Signature) -> Result<SigOutcome, String> {
        if self.fail_status.get() {
            return Err("injected getSignatureStatuses failure".into());
        }
        if self.hide_status.get() {
            return Ok(SigOutcome::Unknown);
        }
        self.inner.signature_outcome(signature)
    }
    fn note_confirmed_slot(&self, slot: u64) {
        self.noted_slot.set(self.noted_slot.get().max(slot));
    }
    fn program_accounts(&self, program: &Pubkey) -> Result<Vec<(Pubkey, Account)>, String> {
        if self.stale() {
            return Ok(self
                .snapshot
                .borrow()
                .iter()
                .filter(|(_, account)| account.owner == *program)
                .map(|(key, account)| (*key, account.clone()))
                .collect());
        }
        self.inner.program_accounts(program)
    }
    fn signature_slot(&self, signature: &Signature) -> Result<Option<u64>, String> {
        self.inner.signature_slot(signature)
    }
    fn signatures_for_address(&self, key: &Pubkey) -> Result<Vec<AddressSignature>, String> {
        self.inner.signatures_for_address(key)
    }
}

fn require_devnet_profile() {
    assert_eq!(
        devnet::orao_profile(),
        OraoProfile::Devnet,
        "set MC_ORAO_PROFILE=devnet: the flow plan must run against the devnet ORAO binary"
    );
    assert_eq!(runtime::provider_artifact_sha256(), devnet::ORAO_DEVNET_ELF_SHA256);
}

fn keypair_of(who: Who) -> Keypair {
    Keypair::new_from_array([who.seed(); 32])
}

/// The rehearsal's world (see `Sim::new` in `m14_flow_plan.rs`) with `initialize` stamped on
/// `ConfigVersion#1` at the plan's first offset, so the runner reads genesis = START_TIME from it.
fn plan_world() -> (World, BTreeMap<Who, Keypair>) {
    let mut w = World::genesis_with_provider().scoped_label("financial.m14.tool");
    w.provider_setup();
    w.set_pool(|pool| pool.cluster_domain = m7::DEVNET_DOMAIN);
    w.rt.strip_pages_to_genesis();
    let page = w.rt.participant_address(1, 0);
    w.rt.remove_account(page);
    w.set_pool(|pool| pool.participant_pages = 0);
    let mut keys = BTreeMap::new();
    for who in ALL_WALLETS {
        let key = match who {
            Who::Sponsor => w.sponsor(),
            other => keypair_of(other),
        };
        let funding = m14_plan::wallet_funding(who);
        match who {
            Who::Sponsor => w.rt.set_lamports(key.pubkey(), funding),
            _ => w.rt.credit_lamports(key.pubkey(), funding),
        }
        keys.insert(who, key);
    }
    w.rt.payer = keys[&Who::Keeper].insecure_clone();
    let mut config = w.config(1);
    let stamp = START_TIME as u64 + m14_plan::T0;
    config.staged_at = stamp;
    config.activated_at = Some(stamp);
    w.rt.replace_config(config);
    (w, keys)
}

fn write_wallets(dir: &TempDir, keys: &BTreeMap<Who, Keypair>) {
    for (who, key) in keys {
        write_keypair(&dir.file(&format!("{}.json", who.name())), key);
    }
    write_keypair(&dir.file("ORACLE.json"), &Keypair::new_from_array([0x55; 32]));
}

fn flow(parts: &[&str], dir: &TempDir) -> Vec<String> {
    let mut args: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
    for (flag, value) in [("--rpc", RPC.to_string()), ("--wallet-dir", dir.dir())] {
        args.push(flag.into());
        args.push(value);
    }
    args
}

fn ok(result: Result<Value, OpsError>) -> Value {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success, got {error}"),
    }
}

fn refused(result: Result<Value, OpsError>) -> String {
    match result {
        Err(OpsError::Refused(message)) => message,
        other => panic!("expected a safety refusal, got {other:?}"),
    }
}

fn last_event<E: anchor_lang::Event>(observations: &[Observation], what: &str) -> E {
    observations
        .iter()
        .rev()
        .find_map(|observation| observation.events::<E>().pop())
        .unwrap_or_else(|| panic!("{what}: no {} event", std::any::type_name::<E>()))
}

fn failing(receipt: &Value) -> Vec<String> {
    receipt["checks"]
        .as_array()
        .map(|checks| {
            checks
                .iter()
                .filter(|entry| entry["ok"] != Value::Bool(true))
                .map(|entry| format!("{}: expected {} actual {}", entry["name"], entry["expected"], entry["actual"]))
                .collect()
        })
        .unwrap_or_default()
}

/// A fresh world with the wallet directory initialized (`flow init`).
fn fixture() -> (World, BTreeMap<Who, Keypair>, TempDir) {
    let (world, keys) = plan_world();
    let dir = TempDir::new("wallets");
    write_wallets(&dir, &keys);
    (world, keys, dir)
}

fn init(cluster: &WorldCluster<'_>, dir: &TempDir) -> Value {
    ok(ops::run_with_cluster(&flow(&["flow", "init"], dir), cluster))
}

// ---------------------------------------------------------------------------
// Gates and refusals (plain `cargo test`)
// ---------------------------------------------------------------------------

#[test]
fn flow_refuses_out_of_order_too_early_wrong_wallet_and_unknown_steps() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    let receipt = init(&cluster, &dir);
    assert_eq!(receipt["all_ok"], true);
    assert!(fs::read_to_string(dir.file("wallets.json")).unwrap().contains(&ID.to_string()));
    // `flow init` is idempotent and never overwrites different keys.
    init(&cluster, &dir);
    write_keypair(&dir.file("A2.json"), &Keypair::new());
    let message = refused(ops::run_with_cluster(&flow(&["flow", "init"], &dir), &cluster));
    assert!(message.contains("different wallets"), "{message}");
    write_keypair(&dir.file("A2.json"), &Keypair::new_from_array([Who::A2.seed(); 32]));

    let args = |parts: &[&str]| flow(parts, &dir);
    let status = |cluster: &WorldCluster<'_>| ok(ops::run_with_cluster(&args(&["flow", "status"]), cluster));

    // Before the plan's first instant: `status` names step 1 and says it is not ready.
    let genesis = START_TIME as u64;
    cluster.warp_to(genesis + m14_plan::T0 - 1);
    let first = status(&cluster);
    assert_eq!(first["next"]["id"], "fund_operations");
    assert_eq!(first["next"]["ready"], false);
    assert_eq!(first["next"]["seconds_until_earliest"], 1);
    assert_eq!(first["genesis"]["genesis_unix"].as_u64(), Some(genesis));
    assert_eq!(first["completed"], 0);

    // Too early by one second.
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &cluster));
    assert!(message.contains("too early") && message.contains("1 s to go"), "{message}");
    assert_eq!(cluster.sends.get(), 0);

    // Out of order: the third step is not the next one (checked before the clock).
    cluster.warp_to(genesis + m14_plan::T0);
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "provision_snapshot_0", "--execute"]), &cluster));
    assert!(message.contains("not the next step") && message.contains("fund_operations"), "{message}");
    // An unknown step is a usage error.
    assert!(matches!(
        ops::run_with_cluster(&args(&["flow", "next", "--step", "no_such_step"]), &cluster),
        Err(OpsError::Usage(_))
    ));
    // Read-only actions take no --execute.
    assert!(matches!(
        ops::run_with_cluster(&args(&["flow", "status", "--execute"]), &cluster),
        Err(OpsError::Usage(_))
    ));

    // Wrong wallet: S.json holds K's key but wallets.json records S's.
    let sponsor_file = dir.file("S.json");
    let original = fs::read_to_string(&sponsor_file).unwrap();
    write_keypair(&sponsor_file, &Keypair::new_from_array([Who::Keeper.seed(); 32]));
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &cluster));
    assert!(message.contains("wallets.json records"), "{message}");
    assert_eq!(cluster.sends.get(), 0, "nothing is signed with a non-matching wallet");
    fs::write(&sponsor_file, original).unwrap();

    // No wallets.json: refused with the way out.
    let empty = TempDir::new("empty");
    let message = refused(ops::run_with_cluster(&flow(&["flow", "status"], &empty), &cluster));
    assert!(message.contains("flow init"), "{message}");

    // The dry run (default) sends and simulates nothing, and describes the transaction.
    let dry = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations"]), &cluster));
    assert_eq!(dry["mode"], "dry-run");
    assert_eq!(dry["preview"]["next_transaction"]["fee_payer"], keypair_of_sponsor(&cluster).to_string());
    assert!(dry["preview"]["next_transaction"]["packet_bytes"].as_u64().unwrap() <= 1_232);
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (0, 0));
    assert!(!std::path::Path::new(&dir.file("flow-receipts.jsonl")).exists(), "a dry run writes no receipt");

    // A real run: step 1 done, receipt appended, step 2 is next.
    let done = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &cluster));
    assert_eq!(done["all_ok"], true, "{done}");
    assert_eq!(done["result"], "done");
    assert_eq!(done["transactions"].as_array().unwrap().len(), 1);
    assert!(done["transactions"][0]["slot"].as_u64().is_some());
    assert_eq!(done["conservation"]["ok"], true);
    assert_eq!(fs::read_to_string(dir.file("flow-receipts.jsonl")).unwrap().lines().count(), 1);
    // ... and the same step again is refused (already complete), as is its successor's successor.
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &cluster));
    assert!(message.contains("already complete"), "{message}");
    let after = status(&cluster);
    assert_eq!(after["next"]["id"], "provision_registry_0");
    assert_eq!(after["completed"], 1);
}

fn keypair_of_sponsor(cluster: &WorldCluster<'_>) -> Pubkey {
    cluster.with_world(|w| w.sponsor().pubkey())
}

#[test]
fn flow_on_devnet_needs_the_packet_pin_records_it_and_never_the_local_oracle() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    let repo = PacketRepo::new("the reviewed M14 packet");
    let run = |extra: &[String]| {
        let mut args = flow(&["flow", "next", "--step", "fund_operations", "--execute", "--cluster", "devnet"], &dir);
        let position = args.iter().position(|a| a == "--rpc").unwrap();
        args[position + 1] = DEVNET_URL.into();
        args.extend(extra.iter().cloned());
        ops::run_with_cluster_in(&args, &cluster, &repo.root())
    };
    let strings = |parts: &[&str]| -> Vec<String> { parts.iter().map(|part| part.to_string()).collect() };
    let pin = repo.pin_flags();

    // No pin, half a pin, the removed alias, a --packet override: refused before any RPC call.
    assert!(refused(run(&[])).contains("--packet-sha256"));
    assert!(refused(run(&pin[..2])).contains("--packet-commit"));
    assert!(matches!(run(&strings(&["--i-have-approval", &repo.sha])), Err(OpsError::Usage(_))));
    let mut overridden = pin.clone();
    overridden.extend(strings(&["--packet", "elsewhere.md"]));
    assert!(refused(run(&overridden)).contains("--packet is refused"));
    assert_eq!(cluster.genesis_reads.get(), 0);

    // The pin but a cluster that is not devnet (mainnet, or the LiteSVM genesis): refused.
    cluster.set_genesis(ops::MAINNET_GENESIS_HASH);
    assert!(refused(run(&pin)).contains("genesis"));
    cluster.set_genesis(LITESVM_GENESIS);
    assert!(refused(run(&pin)).contains("genesis"));
    assert_eq!(cluster.sends.get(), 0);

    // Devnet genesis and the pin: the gate opens, but the local oracle never does.
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    let mut with_oracle = pin.clone();
    with_oracle.push("--local-oracle".into());
    let message = refused(run(&with_oracle));
    assert!(message.contains("--local-oracle is for a loopback validator only"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
    // --genesis-unix is a loopback override only.
    let mut with_genesis = pin.clone();
    with_genesis.extend(strings(&["--genesis-unix", "1"]));
    assert!(refused(run(&with_genesis)).contains("loopback"));

    let done = ok(run(&pin));
    assert_eq!(done["mode"], "execute");
    assert_eq!(done["cluster"]["target"], "devnet");
    assert_eq!(done["all_ok"], true, "{done}");
    assert_eq!(cluster.sends.get(), 1);
    // Every executed receipt records the pin: the returned one and the appended line.
    for receipt in [
        done.clone(),
        serde_json::from_str::<Value>(fs::read_to_string(dir.file("flow-receipts.jsonl")).unwrap().lines().next().unwrap()).unwrap(),
    ] {
        assert_eq!(receipt["packet"]["packet_path"], ops::DEFAULT_PACKET_FILE);
        assert_eq!(receipt["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
        assert_eq!(receipt["packet"]["packet_sha256"].as_str(), Some(repo.sha.as_str()));
    }
    // The write-ahead record of the flow sits next to the receipts.
    assert_eq!(fs::read_to_string(dir.file("pending.jsonl")).unwrap().lines().count(), 2);
    // N4: it names the verified packet pin of the run, the cluster and the step.
    let record: Value = serde_json::from_str(fs::read_to_string(dir.file("pending.jsonl")).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(record["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
    assert_eq!(record["packet"]["packet_sha256"].as_str(), Some(repo.sha.as_str()));
    assert_eq!(record["genesis_hash"], ops::DEVNET_GENESIS_HASH);
    assert_eq!((record["command"].as_str(), record["step"].as_str()), (Some("flow"), Some("fund_operations")));

    // An edit of the working packet after the pin was taken closes the gate again.
    fs::write(repo.root().join(ops::DEFAULT_PACKET_FILE), "the reviewed M14 packet, edited").unwrap();
    assert!(refused(run(&pin)).contains("differs from the blob"));
}

#[test]
fn flow_genesis_override_exists_on_loopback_only_and_moves_the_clock_gate() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    // Pretend the plan began a week before the cluster clock: step 1 is long due.
    let genesis = START_TIME as u64 - 7 * 86_400;
    let args = flow(&["flow", "status", "--genesis-unix", &genesis.to_string()], &dir);
    let status = ok(ops::run_with_cluster(&args, &cluster));
    assert_eq!(status["genesis"]["genesis_unix"].as_u64(), Some(genesis));
    assert_eq!(status["next"]["ready"], true);
}

// ---------------------------------------------------------------------------
// Double-send safety (F2, F3) and receipt semantics (F4)
// ---------------------------------------------------------------------------

#[test]
fn a_stale_read_after_a_confirmed_send_never_causes_a_second_send_and_a_failed_check_blocks_the_next_step() {
    let (mut world, _keys, dir) = fixture();
    let inner = WorldCluster::new(&mut world);
    init(&inner, &dir);
    inner.warp_to(START_TIME as u64 + m14_plan::T0);
    let hooked = Hooked::new(&inner);
    hooked.stale_budget.set(1_000); // every read after the send answers from the pre-send state
    let args = |parts: &[&str]| flow(parts, &dir);

    // FundOperations is not idempotent. A tool that looped on a stale `done_by_state` would fund twice.
    let receipt = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &hooked));
    assert_eq!(inner.sends.get(), 1, "ONE confirmed send completes a one-transaction step");
    assert_eq!(receipt["transactions"].as_array().unwrap().len(), 1);
    // The post-step read-back was stale, so its checks fail: the receipt says so instead of "done".
    assert_eq!(receipt["result"], "done_with_failed_checks", "{receipt}");
    assert_eq!(receipt["all_ok"], false);
    assert!(hooked.noted_slot.get() > 0, "the confirmed slot is handed to the cluster for minContextSlot reads");
    let funded = inner.with_world(|w| w.ops().total_contributed);
    assert_eq!(funded, u128::from(m14_plan::OPS_FUNDING), "the operations account was funded exactly once");

    // With fresh reads again: the failed-check receipt BLOCKS the plan until it has been examined.
    hooked.stale_reads.set(0);
    hooked.stale_budget.set(0);
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &hooked));
    assert_eq!(status["failed_check_receipts"][0], "fund_operations");
    assert_eq!(status["all_ok"], false);
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "provision_registry_0", "--execute"]), &hooked));
    assert!(message.contains("FAILED CHECKS") && message.contains("--accept-failed-checks fund_operations"), "{message}");
    assert!(matches!(
        ops::run_with_cluster(&args(&["flow", "status", "--accept-failed-checks", "no_such_step"]), &hooked),
        Err(OpsError::Usage(_))
    ));
    assert_eq!(inner.sends.get(), 1);
    inner.warp_to(START_TIME as u64 + m14_plan::T0 + 30); // step 2 is due
    let accepted = ok(ops::run_with_cluster(
        &args(&["flow", "next", "--step", "provision_registry_0", "--execute", "--accept-failed-checks", "fund_operations"]),
        &hooked,
    ));
    assert_eq!(accepted["result"], "done", "{accepted}");
    assert_eq!(inner.sends.get(), 2);
}

#[test]
fn an_ambiguous_send_that_lands_anyway_is_resolved_not_rebuilt() {
    let (mut world, _keys, dir) = fixture();
    let inner = WorldCluster::new(&mut world);
    init(&inner, &dir);
    inner.warp_to(START_TIME as u64 + m14_plan::T0);
    let hooked = Hooked::new(&inner);
    hooked.ambiguous_sends.set(1);
    hooked.land_ambiguous.set(true);
    let receipt = ok(ops::run_with_cluster(
        &flow(&["flow", "next", "--step", "fund_operations", "--execute", "--pending-wait-seconds", "0"], &dir),
        &hooked,
    ));
    // The send reported a timeout, the transaction had landed: the signature was polled to confirmed.
    assert_eq!(receipt["result"], "done", "{receipt}");
    assert_eq!(inner.sends.get(), 1, "the ambiguous send is never repeated");
    assert_eq!(inner.with_world(|w| w.ops().total_contributed), u128::from(m14_plan::OPS_FUNDING));
    let lines: Vec<Value> = fs::read_to_string(dir.file("pending.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "write_ahead");
    assert!(lines[0]["blockhash"].is_string() && lines[0]["signature"].is_string());
    assert_eq!(lines[1]["outcome"], "confirmed_after_ambiguous_send");
}

#[test]
fn an_unresolved_write_ahead_record_blocks_every_rebuild_while_valid_and_is_held_not_closed_once_expired() {
    let (mut world, _keys, dir) = fixture();
    let inner = WorldCluster::new(&mut world);
    init(&inner, &dir);
    inner.warp_to(START_TIME as u64 + m14_plan::T0);
    let hooked = Hooked::new(&inner);
    // The send times out, never lands, and its blockhash is still valid: the outcome cannot be known.
    hooked.ambiguous_sends.set(1);
    hooked.land_ambiguous.set(false);
    hooked.blockhash_still_valid.set(true);
    let args = flow(&["flow", "next", "--step", "fund_operations", "--execute", "--pending-wait-seconds", "0"], &dir);
    let message = match ops::run_with_cluster(&args, &hooked) {
        Err(OpsError::Failed(message)) => message,
        other => panic!("expected a failure, got {other:?}"),
    };
    assert!(message.contains("UNKNOWN") && message.contains("do NOT rebuild"), "{message}");
    assert_eq!(inner.sends.get(), 0);
    let written = fs::read_to_string(dir.file("pending.jsonl")).unwrap();
    assert_eq!(written.lines().count(), 1, "the write-ahead record is there and unresolved");

    // A rerun resolves the record FIRST; while it may still land nothing is rebuilt.
    let message = refused(ops::run_with_cluster(&args, &hooked));
    assert!(message.contains("MAY STILL LAND"), "{message}");
    assert_eq!(inner.sends.get(), 0);
    // Any other command that would send is blocked the same way.
    let other = flow(&["flow", "next", "--step", "provision_registry_0", "--execute", "--pending-wait-seconds", "0"], &dir);
    assert!(refused(ops::run_with_cluster(&other, &hooked)).contains("MAY STILL LAND"));

    // Once the blockhash has expired and the history lookup finds no status, the record is HELD: not
    // closed as "never landed" (that used to rebuild and send the step), not rebuilt, whatever the run.
    hooked.blockhash_still_valid.set(false);
    for command in [&args, &other] {
        let message = refused(ops::run_with_cluster(command, &hooked));
        assert!(message.contains("HELD UNKNOWN") && message.contains("fund_operations"), "{message}");
        assert!(!message.contains("never landed"), "{message}");
    }
    assert_eq!(inner.sends.get(), 0);
    assert_eq!(inner.with_world(|w| w.ops().total_contributed), 0, "nothing was funded or rebuilt");
    let text = fs::read_to_string(dir.file("pending.jsonl")).unwrap();
    assert!(!text.contains("never_landed") && !text.contains("never landed"), "{text}");
    assert!(pending_records(&dir).iter().all(|record| record["event"] != "resolved"), "the record is still open");
}

/// The pending file's lines as JSON.
fn pending_records(dir: &TempDir) -> Vec<Value> {
    fs::read_to_string(dir.file("pending.jsonl")).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// Simulate a process that died after the send landed and before its resolution record was written:
/// keep every line except the closing `resolved` ones.
fn drop_resolutions(dir: &TempDir) {
    let kept: String = pending_records(dir)
        .into_iter()
        .filter(|record| record["event"] != "resolved")
        .map(|record| format!("{record}\n"))
        .collect();
    fs::write(dir.file("pending.jsonl"), kept).unwrap();
}

#[test]
fn a_landed_funding_with_no_resolution_a_dead_blockhash_and_a_missing_or_failing_status_is_held_and_never_sent_twice() {
    let (mut world, _keys, dir) = fixture();
    let inner = WorldCluster::new(&mut world);
    init(&inner, &dir);
    inner.warp_to(START_TIME as u64 + m14_plan::T0);
    let first = Hooked::new(&inner);
    let fund = |cluster: &Hooked<'_, '_>| {
        ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--execute"], &dir), cluster)
    };
    // FundOperations lands ...
    let done = ok(fund(&first));
    assert_eq!(done["result"], "done", "{done}");
    assert_eq!(inner.sends.get(), 1);
    let records = pending_records(&dir);
    let (signature, slot) = (records[0]["signature"].as_str().unwrap().to_string(), records[1]["slot"].as_u64().unwrap());
    // ... and the process dies before the resolution is written.  The resumed run talks to a lagging
    // replica (it still shows the pre-send state), the blockhash is long dead (the double's default) and
    // the status cache has evicted the signature.
    drop_resolutions(&dir);
    let resumed = Hooked::new(&inner);
    *resumed.snapshot.borrow_mut() = first.snapshot.borrow().clone();
    resumed.stale_reads.set(100_000);
    let assert_held = |message: &str| {
        assert!(message.contains("HELD UNKNOWN") && message.contains(&signature) && message.contains("fund_operations"), "{message}");
        assert!(!message.contains("never landed") && !message.contains("never_landed"), "{message}");
        assert_eq!(inner.sends.get(), 1, "ZERO additional sends");
        let text = fs::read_to_string(dir.file("pending.jsonl")).unwrap();
        assert!(!text.contains("never_landed") && !text.contains("never landed"), "{text}");
        assert!(pending_records(&dir).iter().all(|record| record["event"] != "resolved"), "the record is still open");
        assert_eq!(inner.with_world(|w| w.ops().total_contributed), u128::from(m14_plan::OPS_FUNDING), "funded exactly once");
    };

    // (1) No status.  The step itself and any other --execute command are held.
    resumed.hide_status.set(true);
    assert_held(&refused(fund(&resumed)));
    let other = flow(&["flow", "next", "--step", "provision_registry_0", "--execute", "--pending-wait-seconds", "0"], &dir);
    assert_held(&refused(ops::run_with_cluster(&other, &resumed)));
    // A read-only command still runs, and says the record is open and held.
    let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &resumed));
    assert_eq!(status["open_pending_records"][0]["signature"], signature.as_str(), "{status}");
    assert_eq!(status["open_pending_records"][0]["held"], true);

    // (2) An ERROR status response: held as well.
    resumed.hide_status.set(false);
    resumed.fail_status.set(true);
    let message = refused(fund(&resumed));
    assert!(message.contains("FAILED"), "{message}");
    assert_held(&message);

    // (3) History later finds it CONFIRMED: the record resolves `confirmed` with its slot, which restores
    // the once-only guard, so the (still lagging) run is refused as a repeat and sends nothing.
    resumed.fail_status.set(false);
    let message = refused(fund(&resumed));
    assert!(message.contains("already CONFIRMED") && message.contains(&signature), "{message}");
    assert_eq!(inner.sends.get(), 1, "ZERO additional sends");
    let records = pending_records(&dir);
    let resolution = records.last().unwrap();
    assert_eq!((resolution["event"].as_str(), resolution["outcome"].as_str()), (Some("resolved"), Some("confirmed")), "{records:?}");
    assert_eq!((resolution["signature"].as_str(), resolution["slot"].as_u64()), (Some(signature.as_str()), Some(slot)));
    let message = refused(fund(&resumed));
    assert!(message.contains("already CONFIRMED"), "{message}");
    assert_eq!(inner.sends.get(), 1);
    assert_eq!(inner.with_world(|w| w.ops().total_contributed), u128::from(m14_plan::OPS_FUNDING));
}

#[test]
fn an_ambiguous_send_whose_status_cannot_be_found_after_the_blockhash_dies_ends_held_and_is_never_rebuilt() {
    // Both worlds: the transaction landed (the response was lost), and it truly did not.  The tool cannot
    // tell them apart from a missing status, so both end the same way: HELD, no rebuild.
    for landed in [true, false] {
        let (mut world, _keys, dir) = fixture();
        let inner = WorldCluster::new(&mut world);
        init(&inner, &dir);
        inner.warp_to(START_TIME as u64 + m14_plan::T0);
        let hooked = Hooked::new(&inner);
        hooked.ambiguous_sends.set(1);
        hooked.land_ambiguous.set(landed);
        hooked.hide_status.set(true);
        let args = flow(&["flow", "next", "--step", "fund_operations", "--execute", "--pending-wait-seconds", "0"], &dir);
        let message = match ops::run_with_cluster(&args, &hooked) {
            Err(OpsError::Failed(message)) => message,
            other => panic!("landed={landed}: expected a failure, got {other:?}"),
        };
        assert!(message.contains("UNKNOWN") && message.contains("HELD") && message.contains("do NOT rebuild"), "{message}");
        assert!(message.contains("NO command") || message.contains("no command"), "{message}");
        for forbidden in ["never landed", "never_landed", "safely"] {
            assert!(!message.contains(forbidden), "landed={landed}: {message}");
        }
        assert_eq!(inner.sends.get(), usize::from(landed), "landed={landed}: the one send is never repeated");
        let records = pending_records(&dir);
        assert_eq!(records[0]["event"], "write_ahead");
        assert!(records[1..].iter().all(|record| record["event"] == "held_unknown"), "{records:?}");
        assert!(!fs::read_to_string(dir.file("pending.jsonl")).unwrap().contains("never_landed"));

        // The rerun is refused as HELD too: zero rebuilds.
        let message = refused(ops::run_with_cluster(&args, &hooked));
        assert!(message.contains("HELD UNKNOWN"), "{message}");
        assert_eq!(inner.sends.get(), usize::from(landed), "landed={landed}: still no rebuild");
        // A found status resolves it: for a landed one, `confirmed`; the world was funded exactly once.
        if landed {
            hooked.hide_status.set(false);
            let resumed = Hooked::new(&inner);
            *resumed.snapshot.borrow_mut() = hooked.snapshot.borrow().clone();
            resumed.stale_reads.set(100_000);
            let message = refused(ops::run_with_cluster(&args, &resumed));
            assert!(message.contains("already CONFIRMED"), "{message}");
            assert_eq!(inner.sends.get(), 1);
            assert_eq!(inner.with_world(|w| w.ops().total_contributed), u128::from(m14_plan::OPS_FUNDING));
        }
    }
}

#[test]
fn a_second_funding_is_refused_from_the_write_ahead_file_even_when_a_lagging_node_reports_it_undone() {
    let (mut world, _keys, dir) = fixture();
    let inner = WorldCluster::new(&mut world);
    init(&inner, &dir);
    inner.warp_to(START_TIME as u64 + m14_plan::T0);
    let first = Hooked::new(&inner);
    let fund = |cluster: &Hooked<'_, '_>| {
        ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--execute"], &dir), cluster)
    };
    let done = ok(fund(&first));
    assert_eq!(done["result"], "done", "{done}");
    assert_eq!(inner.sends.get(), 1);
    let records = pending_records(&dir);
    let (signature, slot) = (records[0]["signature"].as_str().unwrap().to_string(), records[1]["slot"].as_u64().expect("slot"));
    assert_eq!((records[1]["outcome"].as_str(), records[1]["slot_source"].as_str()), (Some("confirmed"), Some("signature_status")));
    assert!(done["transactions"][0]["slot"].as_u64() == Some(slot));

    // A NEW invocation (the CLI starts with no slot of its own) against a replica that has not seen the
    // transaction and ignores minContextSlot: it reports the plan untouched ...
    let lagging = Hooked::new(&inner);
    *lagging.snapshot.borrow_mut() = first.snapshot.borrow().clone();
    lagging.stale_reads.set(100_000);
    let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &lagging));
    assert_eq!(status["next"]["id"], "fund_operations", "the lagging node claims the funding was never made: {status}");
    assert_eq!(lagging.noted_slot.get(), slot, "min slot is seeded from the confirmed record before the first read");
    // ... and the funding transfer, which is NOT idempotent, is refused on the strength of the pending file.
    let message = refused(fund(&lagging));
    assert!(message.contains("already CONFIRMED") && message.contains(&signature), "{message}");
    assert_eq!(inner.sends.get(), 1, "no second transfer");
    assert_eq!(inner.with_world(|w| w.ops().total_contributed), u128::from(m14_plan::OPS_FUNDING), "funded exactly once");

    // The same invocation against a node that honours the floor sees the funding and moves on.
    let honest = Hooked::new(&inner);
    *honest.snapshot.borrow_mut() = first.snapshot.borrow().clone();
    honest.stale_reads.set(100_000);
    honest.honour_min_slot.set(true);
    let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &honest));
    assert_eq!(honest.noted_slot.get(), slot);
    assert_eq!(status["next"]["id"], "provision_registry_0", "reads were served at or above the confirmed slot: {status}");
    assert_eq!(status["completed"], 1);
}

#[test]
fn an_unparsable_write_ahead_line_refuses_flow_status_and_flow_next_with_its_line_number() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    // A torn record: the process died halfway through writing the last line.
    fs::write(dir.file("pending.jsonl"), "{\"schema\":1,\"event\":\"write_ahead\",\"step\":\"fund_operations\",\"sign").unwrap();
    let message = refused(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &cluster));
    assert!(message.contains("line 1") && message.contains("not valid JSON"), "{message}");
    let message = refused(ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--execute"], &dir), &cluster));
    assert!(message.contains("line 1") && message.contains("write-ahead file"), "{message}");
    assert_eq!(cluster.sends.get(), 0, "nothing is sent while the write-ahead file is unreadable");
    // Restored, the very same command runs.
    fs::remove_file(dir.file("pending.jsonl")).unwrap();
    let done = ok(ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--execute"], &dir), &cluster));
    assert_eq!(done["result"], "done", "{done}");
}

#[test]
fn a_torn_receipt_line_refuses_instead_of_silently_dropping_a_failed_check() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    // A torn `done_with_failed_checks` receipt: dropping it would unblock the next step.
    fs::write(dir.file("flow-receipts.jsonl"), "{\"schema\":1,\"step\":\"fund_operations\",\"result\":\"done_with_failed_ch").unwrap();
    let message = refused(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &cluster));
    assert!(message.contains("line 1") && message.contains("not a JSON receipt"), "{message}");
    let message = refused(ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--execute"], &dir), &cluster));
    assert!(message.contains("line 1") && message.contains("receipts file"), "{message}");
    assert_eq!(cluster.sends.get(), 0, "nothing is sent while the receipts file is torn");
}

#[test]
fn only_a_refused_as_expected_receipt_makes_the_traceless_step_done() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &cluster));
    let stamp = status["stamp"].clone();
    let receipts = dir.file("flow-receipts.jsonl");
    let step_done = |status: &Value| -> bool {
        status["steps"].as_array().unwrap().iter().find(|step| step["id"] == "expire_at_deadline").unwrap()["done"].as_bool().unwrap()
    };
    assert!(!step_done(&status), "no receipt, not done");
    for (result, done) in [("done", false), ("done_with_failed_checks", false), ("pending", false), ("refused_as_expected", true)] {
        let line = serde_json::json!({ "schema": 1, "step": "expire_at_deadline", "result": result, "stamp": stamp });
        fs::write(&receipts, format!("{line}\n")).unwrap();
        let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &cluster));
        assert_eq!(step_done(&status), done, "a `{result}` receipt: {status}");
        if done {
            let evidence = status["steps"].as_array().unwrap().iter().find(|step| step["id"] == "expire_at_deadline").unwrap()["evidence"].as_str().unwrap().to_string();
            assert!(evidence.contains("receipt of this cluster"), "{evidence}");
        }
    }
}

#[test]
fn receipts_of_another_cluster_are_refused_and_receipts_only_count_for_traceless_steps() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    let args = |parts: &[&str]| flow(parts, &dir);
    ok(ops::run_with_cluster(&args(&["flow", "next", "--step", "fund_operations", "--execute"]), &cluster));
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["foreign_receipts"], 0);
    let stamp = status["stamp"].clone();
    assert_eq!(stamp["genesis_hash"], LITESVM_GENESIS);
    assert_eq!(stamp["program_id"].as_str(), Some(ID.to_string().as_str()));
    let receipts_file = dir.file("flow-receipts.jsonl");
    let appended: Value = serde_json::from_str(fs::read_to_string(&receipts_file).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(appended["stamp"], stamp, "every receipt is stamped with the cluster it ran against");

    // A receipt of THIS cluster claims a step the chain has not done: it counts for nothing.
    let claim = serde_json::json!({ "schema": 1, "step": "provision_registry_0", "result": "done", "stamp": stamp });
    fs::OpenOptions::new().append(true).open(&receipts_file).and_then(|mut f| { use std::io::Write; writeln!(f, "{claim}") }).unwrap();
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["next"]["id"], "provision_registry_0", "a receipt line does not make a stateful step done");
    assert_eq!(status["completed"], 1);

    // A receipt stamped for another cluster (a different genesis, or another activation) is counted as
    // foreign: status reports it, `flow next` refuses to proceed with such a log.
    let mut other_genesis = stamp.clone();
    other_genesis["genesis_hash"] = Value::String(ops::DEVNET_GENESIS_HASH.into());
    let mut other_activation = stamp.clone();
    other_activation["configversion1_activated_at"] = serde_json::json!(1);
    for foreign in [other_genesis, other_activation] {
        let line = serde_json::json!({ "schema": 1, "step": "provision_registry_0", "result": "done", "stamp": foreign });
        fs::OpenOptions::new().append(true).open(&receipts_file).and_then(|mut f| { use std::io::Write; writeln!(f, "{line}") }).unwrap();
    }
    fs::OpenOptions::new().append(true).open(&receipts_file).and_then(|mut f| { use std::io::Write; writeln!(f, "{{\"step\":\"x\",\"result\":\"done\"}}") }).unwrap();
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["foreign_receipts"], 3, "two foreign stamps and one unstamped receipt");
    assert_eq!(status["next"]["id"], "provision_registry_0");
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "provision_registry_0", "--execute"]), &cluster));
    assert!(message.contains("ANOTHER cluster") && message.contains("fresh --receipts file"), "{message}");
    assert_eq!(cluster.sends.get(), 1);
    // A fresh receipts file runs (step 2 is due 30 s after step 1).
    cluster.warp_to(START_TIME as u64 + m14_plan::T0 + 30);
    let fresh = dir.file("fresh-receipts.jsonl");
    let done = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", "provision_registry_0", "--execute", "--receipts", &fresh]), &cluster));
    assert_eq!(done["result"], "done", "{done}");
}

#[test]
fn the_receipts_file_is_never_a_keypair_a_json_file_or_the_default_wallet() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    let status = |receipts: &str| ops::run_with_cluster(&flow(&["flow", "status", "--receipts", receipts], &dir), &cluster);
    assert!(refused(status(&dir.file("A1.json"))).contains(".jsonl"), "A1.json is a keypair file");
    assert!(refused(status(&dir.file("wallets.json"))).contains(".jsonl"));
    let shaped = dir.file("shaped.jsonl");
    fs::copy(dir.file("A1.json"), &shaped).unwrap();
    assert!(refused(status(&shaped)).contains("keypair-shaped"));
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/nobody".into());
    assert!(refused(status(&format!("{home}/.config/solana/id.json"))).contains("default wallet"));
    assert!(status(&dir.file("receipts-ok.jsonl")).is_ok());
    // The same guard protects the write-ahead file.
    let attempt = ops::run_with_cluster(&flow(&["flow", "next", "--step", "fund_operations", "--pending-file", &dir.file("A1.json")], &dir), &cluster);
    assert!(refused(attempt).contains(".jsonl"));
}

// ---------------------------------------------------------------------------
// The whole plan, through `flow next`
// ---------------------------------------------------------------------------

/// Balances of every plan wallet.
fn balances(cluster: &WorldCluster<'_>, keys: &BTreeMap<Who, Keypair>) -> BTreeMap<Who, u64> {
    cluster.with_world(|w| ALL_WALLETS.iter().map(|who| (*who, w.lamports(keys[who].pubkey()))).collect())
}

/// `flow next` for `step_id`, as a dry run and with `--execute`, is refused (a safety refusal) with every
/// `needle` in its message, and nothing is simulated or sent.
fn assert_refused_untouched(cluster: &WorldCluster<'_>, dir: &TempDir, step_id: &str, needles: &[&str]) {
    let (sends, sims) = (cluster.sends.get(), cluster.sims.get());
    for execute in [false, true] {
        let mut parts = vec!["flow", "next", "--step", step_id, "--local-oracle"];
        if execute {
            parts.push("--execute");
        }
        let message = refused(ops::run_with_cluster(&flow(&parts, dir), cluster));
        for needle in needles {
            assert!(message.contains(*needle), "{step_id} (execute={execute}): `{needle}` is not in: {message}");
        }
    }
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "{step_id}: nothing was simulated or sent");
}

/// The step's entry in a `flow status` document.
fn status_step<'a>(status: &'a Value, id: &str) -> &'a Value {
    status["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|step| step["id"] == id)
        .unwrap_or_else(|| panic!("{id} is in the plan"))
}

/// A Simple position's owed minus the pot at `now`, from the chain's own position and pot with the host
/// economics (as `simple_expectation` in `m14_flow_plan.rs`).
fn simple_owed_minus_pot(cluster: &WorldCluster<'_>, owner: Pubkey, nonce: u64, now: u64) -> i128 {
    cluster.with_world(|w| {
        let position = w.position(owner, nonce);
        let mut accrual = SimpleAccrual {
            principal: position.principal,
            last_accrued_at: position.last_accrued_at,
            expires_at: position.maturity_at,
            remainder: position.remainder,
            accrued_gross: position.accrued_gross,
        };
        accrual.accrue(now).expect("the position accrues");
        let owed = accrual.accrued_gross - position.paid_gross;
        let pot = w.pool().pot;
        let payout = fees::payout(owed, pot, fees::simple_toll_bps(now - position.opened_at)).expect("payout arithmetic");
        let difference = i128::try_from(owed).expect("owed fits") - i128::from(pot);
        assert_eq!(payout.insolvent, difference > 0, "strict insolvency is owed > pot");
        difference
    })
}

/// One executed plan step.
struct Driven {
    receipt: Value,
    /// The transactions the step sent.
    observations: Vec<Observation>,
    pool_before: state::Pool,
    /// The cluster clock the step ran at.
    at: u64,
}

/// One plan step through `flow next`: one second before the plan's earliest instant it is refused as too
/// early (when the clock is still before it), then the clock moves to `at` (never before the plan's
/// instant, never backwards), a dry run sends and simulates nothing, and the real run must pass every check.
fn drive(cluster: &WorldCluster<'_>, dir: &TempDir, step: &m14_plan::Step, at: u64) -> Driven {
    let args = |parts: &[&str]| flow(parts, dir);
    let earliest = START_TIME as u64 + step.at;
    if cluster.now() + 1 < earliest {
        cluster.warp_to(earliest - 1);
    }
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), cluster));
    assert_eq!(status["next"]["id"], step.id, "status names {} as next", step.id);
    if cluster.now() < earliest {
        assert_eq!(status["next"]["ready"], false, "{} is not ready yet", step.id);
        let (sends, sims) = (cluster.sends.get(), cluster.sims.get());
        let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute", "--local-oracle"]), cluster));
        assert!(message.contains("too early"), "{}: {message}", step.id);
        assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "{}: too early sends nothing", step.id);
    }
    let at = at.max(earliest);
    if cluster.now() < at {
        cluster.warp_to(at);
    }
    let (sends, sims) = (cluster.sends.get(), cluster.sims.get());
    let dry = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--local-oracle"]), cluster));
    assert_eq!(dry["mode"], "dry-run", "{}", step.id);
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "{}: a dry run sends nothing", step.id);

    let observed_before = cluster.observed();
    let pool_before = cluster.with_world(|w| w.pool());
    let now = cluster.now();
    let receipt = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute", "--local-oracle"]), cluster));
    let observations: Vec<Observation> = cluster.observations.borrow()[observed_before..].to_vec();
    assert_eq!(receipt["step"], step.id);
    assert_eq!(receipt["all_ok"], true, "{}: {:?}", step.id, failing(&receipt));
    assert_eq!(receipt["conservation"]["ok"], true, "{}: {}", step.id, receipt["conservation"]);
    assert_eq!(receipt["cluster_unix_before"].as_u64(), Some(now));
    cluster.with_world(m7::assert_identities);
    Driven { receipt, observations, pool_before, at: now }
}

#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn the_whole_m14_flow_plan_runs_through_flow_next_on_the_devnet_binaries() {
    require_devnet_profile();
    let predicted = m14_plan::arithmetic();
    let (mut world, keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let steps = m14_plan::plan();
    let genesis = START_TIME as u64;
    let args = |parts: &[&str]| flow(parts, &dir);
    let mut winner: Option<Who> = None;
    let mut first: BTreeMap<String, u64> = BTreeMap::new();
    let mut receipts: Vec<Value> = Vec::new();

    for (index, step) in steps.iter().enumerate() {
        let at = genesis + step.at;
        // ---- one second early: the runner knows this is the next step and refuses it ----------
        if cluster.now() + 1 < at {
            cluster.warp_to(at - 1);
        }
        let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
        assert_eq!(status["next"]["id"], step.id, "status names {} as next", step.id);
        assert_eq!(status["completed"].as_u64(), Some(index as u64));
        if cluster.now() < at {
            assert_eq!(status["next"]["ready"], false);
            let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute", "--local-oracle"]), &cluster));
            assert!(message.contains("too early"), "{}: {message}", step.id);
        }
        cluster.warp_to(at);
        let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
        assert_eq!(status["next"]["ready"], true, "{} is ready at its offset", step.id);
        assert_eq!(status["next"]["earliest_unix"].as_u64(), Some(at));

        // ---- out of order: the step after this one is refused at this instant -------------------
        if let Some(later) = steps.get(index + 1) {
            let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", later.id, "--execute", "--local-oracle"]), &cluster));
            assert!(message.contains("not the next step"), "{}: {message}", later.id);
        }

        // ---- a dry run first: nothing is sent or simulated ------------------------------------------
        let sends = cluster.sends.get();
        let sims = cluster.sims.get();
        let dry = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--local-oracle"]), &cluster));
        assert_eq!(dry["mode"], "dry-run", "{}", step.id);
        assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "{}: a dry run sends nothing", step.id);

        // ---- the oracle: no fulfilment without --local-oracle -------------------------------------
        if matches!(step.act, Act::Fulfil { .. }) {
            let pending = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute"]), &cluster));
            assert_eq!(pending["result"], "pending", "{pending}");
            assert_eq!(pending["all_ok"], false);
            assert_eq!(cluster.sends.get(), sends, "the runner never fulfils on its own");
        }

        // ---- the real run ----------------------------------------------------------------------------
        let before = balances(&cluster, &keys);
        let observed_before = cluster.observed();
        let pool_before = cluster.with_world(|w| w.pool());
        let receipt = ok(ops::run_with_cluster(
            &args(&["flow", "next", "--step", step.id, "--execute", "--local-oracle"]),
            &cluster,
        ));
        let new_observations: Vec<Observation> = cluster.observations.borrow()[observed_before..].to_vec();
        assert_eq!(receipt["step"], step.id);
        assert_eq!(receipt["all_ok"], true, "{}: {:?}", step.id, failing(&receipt));
        assert_eq!(receipt["conservation"]["ok"], true, "{}: {}", step.id, receipt["conservation"]);
        assert_eq!(receipt["cluster_unix_before"].as_u64(), Some(at));
        cluster.with_world(m7::assert_identities);
        for transaction in receipt["transactions"].as_array().unwrap() {
            assert!(transaction["packet_bytes"].as_u64().unwrap() <= 1_232, "{}", step.id);
            assert!(transaction["slot"].as_u64().is_some(), "{}: slot recorded", step.id);
        }
        // Only the actor's wallet moved.
        let actor = if step.who == Who::Winner { winner.expect("the winner is known") } else { step.who };
        let after = balances(&cluster, &keys);
        for who in ALL_WALLETS {
            if who != actor {
                assert_eq!(after[&who], before[&who], "{}: {} must not move", step.id, who.name());
            }
        }
        for flow_code in step.flows.split(',') {
            first.entry(flow_code.to_string()).or_insert(step.at);
        }

        // ---- the outcomes m14_flow_plan.rs asserts, from events and the books ------------------------
        match step.id {
            "fund_operations" => {
                let event: OperationsFunded = last_event(&new_observations, step.id);
                assert_eq!((event.funder, event.amount), (keys[&Who::Sponsor].pubkey(), m14_plan::OPS_FUNDING));
            }
            "a1_fund_series_a" => {
                let event: SeriesAFunded = last_event(&new_observations, step.id);
                assert_eq!((event.gross, event.entitlement_after), (m14_plan::A1_SERIES_A, 62_000_000));
            }
            "a1_open_simple" => {
                let event: PositionOpened = last_event(&new_observations, step.id);
                assert_eq!((event.gross, event.load), (m14_plan::A1_GAME, 40_000_000));
            }
            "a1_withdraw_simple" => {
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(!event.closing);
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (predicted.f3.paid_gross, predicted.f3.toll, predicted.f3.net),
                    "F3 equals the pure arithmetic"
                );
                assert_eq!(cluster.with_world(|w| w.pool()).pot, pool_before.pot - event.gross_paid);
            }
            "advance_ticket_f3" | "advance_ticket_close1" | "advance_ticket_c15" | "advance_ticket_close2" => {
                let committed: TicketCommitted = last_event(&new_observations, step.id);
                assert_eq!(committed.credited + committed.seed, committed.toll);
                assert!(new_observations.len() >= 2, "{}: a ticket takes several transactions", step.id);
            }
            "a1_claim_a_only" => {
                let event: BackerClaimed = last_event(&new_observations, step.id);
                assert!(event.a_part > 0 && event.b_part == 0);
            }
            "a1_closing_withdraw" => {
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(event.closing);
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (predicted.close1.paid_gross, predicted.close1.toll, predicted.close1.net),
                    "close 1 equals the pure arithmetic"
                );
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped: the whole pot is paid");
                let closed: RoundClosed = last_event(&new_observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (1, at + m14_plan::DEADLINE_SECS));
            }
            "candidate_pages_1" | "candidate_pages_2" => {
                let close_id = if step.id.ends_with('1') { 1 } else { 2 };
                let record = cluster.with_world(|w| w.close_record(close_id));
                assert!(record.provisioned_preferred_pages >= 1 && record.provisioned_fallback_pages >= 1);
            }
            "build_candidates_1" | "build_candidates_2" => {
                let sealed: CandidatesSealed = last_event(&new_observations, step.id);
                let (close_id, count) = if step.id.ends_with('1') { (1, 2) } else { (2, 3) };
                assert_eq!((sealed.close_id, sealed.tag, sealed.count), (close_id, 0, count));
            }
            "request_draw_1" | "request_draw_2" => {
                let event: DrawRequested = last_event(&new_observations, step.id);
                assert_eq!(event.fee_paid, 300_000);
            }
            "fulfil_1" => {
                let latency = &receipt["oracle"];
                assert!(latency["latency_slots"].as_u64().unwrap() >= 1, "{latency}");
                assert_eq!(latency["latency_seconds"].as_i64(), Some(15), "the plan spaces request and fulfilment by 15 s: {latency}");
                assert!(latency["request_signature"].is_string() && latency["fulfil_signature"].is_string());
                assert_eq!(new_observations.len(), 1);
                // The synthetic precompile signature is charged: the fee is two signatures.
                assert_eq!(new_observations[0].expected_fee_lamports, 10_000);
            }
            "capture_draw_1" => {
                let event: DrawCaptured = last_event(&new_observations, step.id);
                assert_eq!(event.captured_at, at);
            }
            "apply_promotion_1" => {
                let event: PromotionApplied = last_event(&new_observations, step.id);
                let promoted = [Who::P4, Who::P5]
                    .into_iter()
                    .find(|candidate| keys[candidate].pubkey() == event.winner)
                    .expect("the winner is P4 or P5");
                winner = Some(promoted);
                assert_eq!((event.loss, event.entitlement), (m14_plan::CANDIDATE_GAME, 11_600_000));
                assert_eq!(receipt["winner"], Value::Null, "the runner resolved the winner only after this step");
            }
            "w_finalize_reopen" => {
                assert_eq!(new_observations.len(), 1, "finalize + page + open is ONE transaction");
                assert_eq!(new_observations[0].expected_fee_lamports, 5_000, "one signature: the plain wallet's");
                let finalized: RoundFinalized = last_event(&new_observations, step.id);
                assert_eq!((finalized.close_id, finalized.new_round, finalized.seed_carried), (1, 2, pool_before.seed));
                println!(
                    "model seed carried {} vs chain {} ({})",
                    predicted.seed_carried,
                    finalized.seed_carried,
                    if predicted.seed_carried == finalized.seed_carried { "match" } else { "DIFFERS" }
                );
                let opened: PositionOpened = last_event(&new_observations, step.id);
                assert_eq!((opened.round_id, opened.gross), (2, m14_plan::W_COMPOUND30));
            }
            "p1_settle_compound15" => {
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(!event.closing);
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (predicted.c15.paid_gross, predicted.c15.toll, predicted.c15.net),
                    "C15 equals the pure arithmetic"
                );
            }
            "winner_claim_a_and_b" => {
                let event: BackerClaimed = last_event(&new_observations, step.id);
                assert!(event.a_part > 0 && event.b_part > 0, "A and B in ONE instruction");
            }
            "a1_claim_after_cap" => {
                let event: BackerClaimed = last_event(&new_observations, step.id);
                assert!(event.a_part > 0 && event.b_part == 0);
            }
            "w_settle_compound30" => {
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(event.closing);
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped");
                let closed: RoundClosed = last_event(&new_observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (2, at + m14_plan::DEADLINE_SECS));
                assert!(predicted.c30.insolvent);
            }
            "expire_at_deadline" => {
                assert_eq!(receipt["result"], "refused_as_expected");
                assert_eq!(receipt["refused_as_expected"]["error"], "DeadlineNotReached");
                assert_eq!(receipt["refused_as_expected"]["sent"], false);
                assert!(new_observations.is_empty(), "the refused-by-design transaction is not sent");
                let close2 = cluster.with_world(|w| w.close_record(2));
                assert_eq!(close2.promotion, state::Promotion::Pending);
                let before = receipt["cluster_unix_before"].as_u64().expect("clock");
                assert!(before < close2.promotion_deadline, "the refusal is simulated inside the deadline: {before} vs {}", close2.promotion_deadline);
            }
            "expire_promotion_2" => {
                let event: PromotionSkipped = last_event(&new_observations, step.id);
                assert_eq!(
                    (event.close_id, event.reason, event.stage),
                    (2, pclose::REASON_DEADLINE, pclose::STAGE_REQUESTED_UNFULFILLED)
                );
            }
            "finalize_2" => {
                let event: RoundFinalized = last_event(&new_observations, step.id);
                assert_eq!((event.close_id, event.new_round), (2, 3));
            }
            _ => {}
        }
        // The runner's own wallet-equation and identity checks passed for every step.
        assert!(
            receipt["checks"].as_array().unwrap().iter().any(|c| c["name"].as_str().is_some_and(|n| n.contains("actor lamport equation"))),
            "{}: the per-wallet equation was checked",
            step.id
        );
        receipts.push(receipt);
    }

    // ---- the end state m14_flow_plan.rs asserts -------------------------------------------------------
    let pool = cluster.with_world(|w| w.pool());
    assert_eq!(
        (pool.round_id, pool.next_close_id, pool.current_close_id, pool.active_ticket),
        (3, 3, None, 0),
        "two closes, two finalized rounds, nothing in flight"
    );
    let winner_key = keys[&winner.expect("a winner")].pubkey();
    let (first_close, second_close) = cluster.with_world(|w| (w.close_record(1), w.close_record(2)));
    assert!(first_close.finalized && first_close.cash_committed && first_close.reserve_settled);
    assert!(matches!(first_close.promotion, state::Promotion::Applied { winner: promoted, .. } if promoted == winner_key));
    assert!(second_close.finalized && second_close.cash_committed && second_close.reserve_settled);
    assert_eq!(
        second_close.promotion,
        state::Promotion::Skipped { reason: pclose::REASON_DEADLINE, stage: pclose::STAGE_REQUESTED_UNFULFILLED }
    );
    for flow_code in m14_plan::REQUIRED_FLOWS {
        assert!(first.contains_key(flow_code), "flow {flow_code} was never reached");
    }
    cluster.with_world(|w| {
        w.assert_vault_identity();
        w.assert_ops_cash_identity();
        m7::assert_identities(w);
    });

    // A1 reached 124 percent in ticket 3 and left the registry; the winner holds both rows.
    let a1 = cluster.with_world(|w| w.backer(keys[&Who::A1].pubkey(), 0));
    assert!(!a1.active && a1.remaining().unwrap() <= 10);

    // The runner agrees: the plan is complete, one receipt line per step, all of them ok.
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["completed"].as_u64(), Some(steps.len() as u64));
    assert_eq!(status["next"], "the plan is complete");
    assert_eq!(status["winner"], winner.unwrap().name());
    assert_eq!(status["conservation"]["ok"], true);
    assert_eq!(status["missed_refusal_evidence"], serde_json::json!([]));
    let log = fs::read_to_string(dir.file("flow-receipts.jsonl")).unwrap();
    assert_eq!(log.lines().count(), steps.len(), "one receipt line per step");
    for (line, step) in log.lines().zip(&steps) {
        let entry: Value = serde_json::from_str(line).unwrap();
        assert_eq!(entry["step"], step.id);
        assert_eq!(entry["all_ok"], true);
    }
    // A completed plan has no next step: anything is refused as already complete.
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "finalize_2", "--execute"]), &cluster));
    assert!(message.contains("already complete"), "{message}");
    println!(
        "M14 flow runner: PASS - {} steps through `flow next`, {} transactions sent, {} simulations, both identities after every step",
        steps.len(),
        cluster.sends.get(),
        cluster.sims.get()
    );
    for receipt in &receipts {
        if receipt["step"] == "fulfil_1" {
            println!("oracle latency (LiteSVM, plan spacing): {}", receipt["oracle"]);
        }
    }
}

// ---------------------------------------------------------------------------
// The F3 window
// ---------------------------------------------------------------------------

/// A1 opens `LATE` seconds after its plan offset, beyond the 1 h allowance that `F3_MIN_ELAPSED` makes for
/// a late open; the plan's own instant for F3 is then before the window opens.  `flow next` refuses F3
/// there (dry run and `--execute`, nothing simulated or sent), accepts it at exactly `F3_MIN_ELAPSED`
/// after A1's on-chain `opened_at`, and every later check of the plan still holds, F6 at the Compound15
/// ticket included.
///
/// The late open also covers late execution and the actual-open maturity checks.  Close 1's plan instant is
/// 3,568 s BEFORE A1's actual strict-insolvency threshold (`opened_at + CLOSE1_INSOLVENT_FROM_ELAPSED`): the
/// runner's insolvency pre-check refuses it there and one second short of the threshold, and sends it at the
/// threshold.  Steps 14-27 are then overdue and run at once.  The Compound15 and Compound30 settlements are
/// refused before the positions' ACTUAL maturities (read from chain) and sent at them; steps 33-37 run at once
/// at the Compound30 close; step 38 is refused at the deadline itself and sent one second later.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn the_f3_window_guard_and_a_late_open_still_reach_every_later_check() {
    const LATE: u64 = 3_700;
    require_devnet_profile();
    let (mut world, keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let steps = m14_plan::plan();
    let genesis = START_TIME as u64;
    let args = |parts: &[&str]| flow(parts, &dir);
    // Where A1 will open, and so where F3 falls in A1's own elapsed time and where close 1 first becomes possible.
    let opened_at = genesis + m14_plan::O1 + LATE;
    let threshold = opened_at + m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED;
    let (window_opens, window_closes) = (opened_at + m14_plan::F3_MIN_ELAPSED, opened_at + m14_plan::F3_MAX_ELAPSED);
    assert_eq!(
        threshold - (genesis + m14_plan::CLOSE1_AT),
        LATE - m14_plan::A1_DEVNET_OPEN_DELAY,
        "close 1's plan instant is 3,568 s before A1's actual threshold"
    );
    // Close 1 is sent exactly at the threshold, with F3 at the first second of its window.
    let expected_at_threshold =
        m14_plan::arithmetic_at(m14_plan::F3_MIN_ELAPSED, m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED).unwrap_or_else(|error| {
            panic!("a late open ({LATE} s) with F3 at {} s and the close at the threshold: {error}", m14_plan::F3_MIN_ELAPSED)
        });
    assert_eq!(expected_at_threshold.insolvent_from_elapsed, m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED);

    let mut receipts = 0usize;
    let mut finalized_round_three = false;
    // The cluster clock at which the Compound30 settlement closed round 2 (close 2 starts there).
    let mut t32 = 0u64;
    for step in &steps {
        let mut at = genesis + step.at;
        if step.id == "a1_open_simple" {
            at += LATE;
        }
        // The clock only moves forward: a step that is overdue (F3 and its follow-ups after the guard
        // test below, steps 14-27 after the close) runs at once.
        if cluster.now() < at {
            cluster.warp_to(at);
        }

        if step.id == "a1_withdraw_simple" {
            // The plan's own instant: 100 s before the window opens, and nothing may be signed or sent.
            assert_eq!(cluster.now(), at);
            let elapsed = cluster.now() - opened_at;
            assert_eq!(elapsed, m14_plan::F3_AT - m14_plan::O1 - LATE);
            assert!(elapsed < m14_plan::F3_MIN_ELAPSED);
            let (sends, sims) = (cluster.sends.get(), cluster.sims.get());
            let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
            assert_eq!(status["next"]["id"], step.id);
            assert_eq!(status["next"]["ready"], true, "`ready` stays earliest-only: the plan's instant has come");
            let window = &status["next"]["f3_window"];
            assert_eq!(window["inside"], false, "{window}");
            assert_eq!(window["opened_at"].as_u64(), Some(opened_at));
            assert_eq!((window["opens_unix"].as_u64(), window["closes_unix"].as_u64()), (Some(window_opens), Some(window_closes)));
            for execute in [false, true] {
                let mut parts = vec!["flow", "next", "--step", step.id, "--local-oracle"];
                if execute {
                    parts.push("--execute");
                }
                let message = refused(ops::run_with_cluster(&args(&parts[..]), &cluster));
                assert!(message.contains("F3 window"), "execute={execute}: {message}");
                assert!(message.contains(&elapsed.to_string()), "the elapsed time is named: {message}");
                assert!(message.contains(&window_opens.to_string()) && message.contains(&window_closes.to_string()), "both bounds are named: {message}");
                assert!(message.contains("F6"), "the reason for the lower bound is given: {message}");
            }
            // One second short of the window is refused too; the window's first second is accepted.
            cluster.warp_to(window_opens - 1);
            let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--local-oracle"]), &cluster));
            assert!(message.contains("F3 window"), "{message}");
            cluster.warp_to(window_opens);
            let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
            assert_eq!(status["next"]["f3_window"]["inside"], true, "{}", status["next"]["f3_window"]);
            let dry = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--local-oracle"]), &cluster));
            assert_eq!(dry["mode"], "dry-run");
            assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "the refusals and the dry run send and simulate nothing");
        }

        if step.id == "a1_closing_withdraw" {
            // The plan's instant is before A1's ACTUAL threshold (read from chain): the insolvency pre-check
            // refuses close 1 there and one second short of the threshold, and nothing is simulated or sent.
            let a1_opened = cluster.with_world(|w| w.position(keys[&Who::A1].pubkey(), 1).opened_at);
            assert_eq!(a1_opened + m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED, threshold);
            assert_eq!(cluster.now(), at);
            assert!(at < threshold);
            assert_refused_untouched(&cluster, &dir, step.id, &["strictly insolvent", "solvent"]);
            cluster.warp_to(threshold - 1);
            assert_refused_untouched(&cluster, &dir, step.id, &["strictly insolvent", "solvent"]);
            cluster.warp_to(threshold);
        }

        if step.id == "p1_settle_compound15" || step.id == "w_settle_compound30" {
            // The plan's instant is before the position's ACTUAL maturity (it opened at the late threshold).
            let owner = if step.id == "p1_settle_compound15" { Who::P1 } else { Who::W };
            let maturity = cluster.with_world(|w| w.position(keys[&owner].pubkey(), 1).maturity_at);
            assert_eq!(cluster.now(), at);
            assert!(at < maturity, "{}: the plan's instant {at} is before the maturity {maturity}", step.id);
            assert_refused_untouched(&cluster, &dir, step.id, &["matures at"]);
            cluster.warp_to(maturity - 1);
            assert_refused_untouched(&cluster, &dir, step.id, &["matures at"]);
            cluster.warp_to(maturity);
        }

        if step.id == "expire_promotion_2" {
            // The plan's instant is long past, but the program needs now > deadline: refused AT the deadline.
            let deadline = cluster.with_world(|w| w.close_record(2)).promotion_deadline;
            assert_eq!(deadline, t32 + m14_plan::DEADLINE_SECS);
            assert_eq!(cluster.now(), t32, "steps 33-37 ran at once at the close");
            cluster.warp_to(deadline);
            assert_refused_untouched(&cluster, &dir, step.id, &["not past"]);
            cluster.warp_to(deadline + 1);
        }

        let observed_before = cluster.observed();
        let pool_before = cluster.with_world(|w| w.pool());
        let receipt = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute", "--local-oracle"]), &cluster));
        let new_observations: Vec<Observation> = cluster.observations.borrow()[observed_before..].to_vec();
        assert_eq!(receipt["step"], step.id);
        assert_eq!(receipt["all_ok"], true, "{}: {:?}", step.id, failing(&receipt));
        assert_eq!(receipt["conservation"]["ok"], true, "{}: {}", step.id, receipt["conservation"]);
        cluster.with_world(m7::assert_identities);
        receipts += 1;

        match step.id {
            "a1_open_simple" => {
                let position = cluster.with_world(|w| w.position(keys[&Who::A1].pubkey(), 1));
                assert_eq!(position.opened_at, opened_at, "A1 opened {LATE} s after its plan offset");
            }
            "a1_withdraw_simple" => {
                assert_eq!(cluster.now(), window_opens, "F3 ran at the first second of its window");
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(!event.closing);
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (expected_at_threshold.f3.paid_gross, expected_at_threshold.f3.toll, expected_at_threshold.f3.net),
                    "F3 at the window's first second equals the pure arithmetic"
                );
            }
            "a1_closing_withdraw" => {
                assert_eq!(cluster.now(), threshold, "close 1 ran at A1's actual threshold");
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(event.closing);
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (
                        expected_at_threshold.close1.paid_gross,
                        expected_at_threshold.close1.toll,
                        expected_at_threshold.close1.net
                    ),
                    "close 1 at the threshold equals the pure arithmetic"
                );
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped: the whole pot is paid");
                let closed: RoundClosed = last_event(&new_observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (1, threshold + m14_plan::DEADLINE_SECS));
            }
            "request_draw_1" | "capture_draw_1" => {
                let deadline = cluster.with_world(|w| w.close_record(1)).promotion_deadline;
                assert!(cluster.now() <= deadline, "{}: inside the deadline of close 1 ({} vs {deadline})", step.id, cluster.now());
            }
            "w_finalize_reopen" => {
                // W opens at the threshold instant, so its maturity is 30 days after it.
                let position = cluster.with_world(|w| w.position(keys[&Who::W].pubkey(), 1));
                assert_eq!((position.opened_at, position.maturity_at), (threshold, threshold + 2_592_000));
            }
            "p1_open_compound15" => {
                let position = cluster.with_world(|w| w.position(keys[&Who::P1].pubkey(), 1));
                assert_eq!((position.opened_at, position.maturity_at), (threshold, threshold + 1_296_000));
            }
            "advance_ticket_c15" => {
                // F6: the Compound15 toll caps A1's Series A row and it leaves the registry.
                let a1 = cluster.with_world(|w| w.backer(keys[&Who::A1].pubkey(), 0));
                assert!(!a1.active && a1.remaining().unwrap() <= 10, "A1 reached 124 percent in ticket 3");
            }
            "p1_settle_compound15" => {
                let maturity = cluster.with_world(|w| w.position(keys[&Who::P1].pubkey(), 1).maturity_at);
                assert_eq!(cluster.now(), maturity, "C15 ran at the position's actual maturity");
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(!event.closing, "C15 is solvent");
                assert!(event.gross_paid < pool_before.pot && u128::from(event.gross_paid) == expected_at_threshold.c15.gross_owed, "C15 paid in full out of a larger pot");
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (expected_at_threshold.c15.paid_gross, expected_at_threshold.c15.toll, expected_at_threshold.c15.net),
                    "C15 equals the pure arithmetic"
                );
            }
            "winner_claim_a_and_b" => {
                let event: BackerClaimed = last_event(&new_observations, step.id);
                assert!(event.a_part > 0 && event.b_part > 0, "the winner's combined claim has both parts");
            }
            "w_settle_compound30" => {
                let maturity = cluster.with_world(|w| w.position(keys[&Who::W].pubkey(), 1).maturity_at);
                assert_eq!(cluster.now(), maturity, "C30 ran at the position's actual maturity");
                t32 = cluster.now();
                let event: PayoutPaid = last_event(&new_observations, step.id);
                assert!(event.closing, "C30 is strictly insolvent");
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped");
                assert!(expected_at_threshold.c30.gross_owed > u128::from(pool_before.pot));
                let closed: RoundClosed = last_event(&new_observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (2, t32 + m14_plan::DEADLINE_SECS));
            }
            "expire_at_deadline" => {
                assert_eq!(receipt["result"], "refused_as_expected");
                assert!(new_observations.is_empty(), "the refused-by-design transaction is not sent");
                let deadline = cluster.with_world(|w| w.close_record(2)).promotion_deadline;
                assert!(receipt["cluster_unix_before"].as_u64().expect("clock") <= deadline, "the refusal is simulated inside the deadline");
            }
            "expire_promotion_2" => {
                assert_eq!(cluster.now(), t32 + m14_plan::DEADLINE_SECS + 1, "expired one second past the deadline");
                let event: PromotionSkipped = last_event(&new_observations, step.id);
                assert_eq!(
                    (event.close_id, event.reason, event.stage),
                    (2, pclose::REASON_DEADLINE, pclose::STAGE_REQUESTED_UNFULFILLED)
                );
            }
            "finalize_2" => {
                let event: RoundFinalized = last_event(&new_observations, step.id);
                assert_eq!((event.close_id, event.new_round), (2, 3));
                finalized_round_three = true;
            }
            _ => {}
        }
    }

    assert_eq!(receipts, steps.len(), "all {} steps ran", steps.len());
    assert!(finalized_round_three, "the last receipt finalizes round 2");
    let pool = cluster.with_world(|w| w.pool());
    assert_eq!((pool.round_id, pool.current_close_id, pool.active_ticket), (3, None, 0), "round 3 is open, nothing in flight");
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["completed"].as_u64(), Some(steps.len() as u64));
    assert_eq!(status["next"], "the plan is complete");
    assert_eq!(status["conservation"]["ok"], true);
    println!(
        "M14 late open: PASS - A1 opened {LATE} s late; F3 refused at the plan's instant (elapsed {}), accepted at {}; close 1 refused at the plan's instant and at threshold - 1, sent at the threshold ({} s after the plan's instant); the settlements waited for the actual maturities; all {} steps passed their checks",
        m14_plan::F3_AT - m14_plan::O1 - LATE,
        m14_plan::F3_MIN_ELAPSED,
        threshold - (genesis + m14_plan::CLOSE1_AT),
        steps.len()
    );
}

/// The upper end of the window: `F3_MAX_ELAPSED - 1` after A1's on-chain open is accepted, `F3_MAX_ELAPSED`
/// is refused (10 minutes before the 12 % tier ends), in a fresh world because the clock only moves
/// forward.  Steps 1-9 involve no ORAO instruction, so this runs in a plain `cargo test`.
#[test]
fn the_f3_window_closes_ten_minutes_before_the_12_percent_tier_ends() {
    let (mut world, keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let steps = m14_plan::plan();
    let genesis = START_TIME as u64;
    let args = |parts: &[&str]| flow(parts, &dir);
    assert_eq!(steps[9].id, "a1_withdraw_simple", "steps 1-9 precede F3");
    for step in &steps[..9] {
        if cluster.now() < genesis + step.at {
            cluster.warp_to(genesis + step.at);
        }
        let receipt = ok(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute"]), &cluster));
        assert_eq!(receipt["all_ok"], true, "{}: {:?}", step.id, failing(&receipt));
    }
    let opened_at = cluster.with_world(|w| w.position(keys[&Who::A1].pubkey(), 1).opened_at);
    assert_eq!(opened_at, genesis + m14_plan::O1);
    let (sends, sims) = (cluster.sends.get(), cluster.sims.get());
    let dry_run = || ops::run_with_cluster(&args(&["flow", "next", "--step", "a1_withdraw_simple"]), &cluster);
    let inside = || ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster))["next"]["f3_window"]["inside"].clone();

    // The last second of the window: accepted (a dry run: nothing is sent).
    cluster.warp_to(opened_at + m14_plan::F3_MAX_ELAPSED - 1);
    assert_eq!(inside(), true);
    let accepted = ok(dry_run());
    assert_eq!(accepted["mode"], "dry-run");
    assert_eq!(accepted["step"], "a1_withdraw_simple");
    assert_eq!(accepted["plan_revision"], m14_plan::PLAN_REVISION);

    // The bound itself: refused, in a dry run and with --execute, nothing simulated or sent.
    cluster.warp_to(opened_at + m14_plan::F3_MAX_ELAPSED);
    assert_eq!(inside(), false);
    let message = refused(dry_run());
    assert!(message.contains("F3 window") && message.contains("12 %"), "{message}");
    assert!(message.contains(&(opened_at + m14_plan::F3_MAX_ELAPSED).to_string()), "the bound is named: {message}");
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "a1_withdraw_simple", "--execute"]), &cluster));
    assert!(message.contains("F3 window"), "{message}");
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "nothing was sent or simulated");
}

// ---------------------------------------------------------------------------
// The current devnet run, and a missed refusal window
// ---------------------------------------------------------------------------

/// A1's F3 elapsed in the current devnet run (recorded: seconds after its open; it paid 456,785,185).
const DEVNET_F3_ELAPSED: u64 = 358_784;

/// Replays the current devnet run: its ACTUAL timings for steps 1-12 relative to genesis (`START_TIME` plays
/// devnet's genesis, 1,792,942,295): A1 opened 132 s late, P4 and P5 at T0 + 401 / 420, F3 at A1's elapsed
/// 358,784 (steps 11 and 12 right after).  Then close 1 exactly at the threshold of A1's actual open and
/// steps 14-39 on the new schedule, each refused one second early and run at its instant.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn the_current_devnet_run_closes_at_its_threshold_and_every_later_check_holds() {
    require_devnet_profile();
    let predicted = m14_plan::arithmetic_at(DEVNET_F3_ELAPSED, m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED)
        .expect("the devnet run's F3 and its close at the threshold");
    assert_eq!(predicted.seed_carried, 48_872_669);
    let (mut world, keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let steps = m14_plan::plan();
    let genesis = START_TIME as u64;
    let args = |parts: &[&str]| flow(parts, &dir);
    let a1 = keys[&Who::A1].pubkey();

    // ---- steps 1-12 at devnet's actual timings ----------------------------------------------------
    let mut opened_at = 0u64;
    for step in &steps[..12] {
        let at = match step.id {
            "a1_open_simple" => genesis + m14_plan::O1 + m14_plan::A1_DEVNET_OPEN_DELAY,
            "p4_open_simple" => genesis + m14_plan::T0 + 401,
            "p5_open_simple" => genesis + m14_plan::T0 + 420,
            "a1_withdraw_simple" => opened_at + DEVNET_F3_ELAPSED,
            // Steps 11 and 12: their plan instants have passed, so they run right after F3.
            _ => genesis + step.at,
        };
        let ran = drive(&cluster, &dir, step, at);
        match step.id {
            "a1_open_simple" => {
                opened_at = cluster.with_world(|w| w.position(a1, 1).opened_at);
                assert_eq!(opened_at, genesis + m14_plan::O1 + 132);
                assert_eq!(opened_at - genesis, m14_plan::A1_DEVNET_OPENED_AT - m14_plan::DEVNET_GENESIS_UNIX);
            }
            "a1_withdraw_simple" => {
                assert_eq!(ran.at - opened_at, DEVNET_F3_ELAPSED);
                let event: PayoutPaid = last_event(&ran.observations, step.id);
                assert!(!event.closing);
                assert_eq!((event.gross_paid, event.toll, event.net), (456_785_185, 54_814_222, 401_970_963));
            }
            "advance_ticket_f3" | "a1_claim_a_only" => {
                assert_eq!(ran.at, opened_at + DEVNET_F3_ELAPSED, "{}: right after F3", step.id);
                if step.id == "a1_claim_a_only" {
                    let event: BackerClaimed = last_event(&ran.observations, step.id);
                    assert_eq!(event.amount, 27_407_109, "the claim of step 12 paid devnet's amount");
                }
            }
            _ => {}
        }
    }

    // ---- the replay holds devnet's recorded facts ------------------------------------------------
    let (pool, row) = cluster.with_world(|w| (w.pool(), w.backer(a1, 0)));
    assert_eq!((pool.pot, pool.seed), (572_414_815, 27_407_113));
    assert_eq!(row.remaining().expect("remaining"), 34_592_891);

    // ---- resume and once-only ----------------------------------------------------------------------
    let earliest = genesis + m14_plan::CLOSE1_AT;
    assert_eq!(earliest, opened_at + m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED, "the plan gate is the threshold of the actual open");
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["completed"].as_u64(), Some(12));
    assert_eq!(status["next"]["id"], "a1_closing_withdraw");
    assert_eq!(status["plan_revision"], m14_plan::PLAN_REVISION);
    assert_eq!(status["next"]["earliest_unix"].as_u64(), Some(earliest));
    for step in &steps[..12] {
        let sends = cluster.sends.get();
        let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", step.id, "--execute"]), &cluster));
        assert!(message.contains("already complete"), "{}: {message}", step.id);
        assert_eq!(cluster.sends.get(), sends, "{}: a completed step sends nothing", step.id);
    }

    // ---- steps 13-39 ------------------------------------------------------------------------------------
    let mut owed_at_threshold = 0i128;
    let mut receipts = 12usize;
    for step in &steps[12..] {
        let at = genesis + step.at;
        if step.id == "a1_closing_withdraw" {
            // The plan's gate coincides with the game's own threshold for this run: one second short A1 is
            // still owed less than the pot, at the instant strictly more.
            assert_eq!(simple_owed_minus_pot(&cluster, a1, 1, at - 1), -1_042);
            owed_at_threshold = simple_owed_minus_pot(&cluster, a1, 1, at);
            assert_eq!(owed_at_threshold, 231);
        }
        if step.id == "expire_at_deadline" {
            // The refusal window is shown beside `ready`: closed one second early, open at the instant, and it
            // ends with the (inclusive) deadline.
            let deadline = cluster.with_world(|w| w.close_record(2)).promotion_deadline;
            assert_eq!(deadline, genesis + m14_plan::C30_AT + m14_plan::DEADLINE_SECS);
            cluster.warp_to(at - 1);
            let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
            let window = &status["next"]["refusal_window"];
            assert_eq!((window["inside"].clone(), window["missed"].clone()), (Value::Bool(false), Value::Bool(false)), "{window}");
            assert_eq!(window["opens_unix"].as_u64(), Some(at));
            assert_eq!(window["deadline_unix"].as_u64(), Some(deadline));
            assert_eq!(window["deadline_inclusive"], true);
            assert_refused_untouched(&cluster, &dir, step.id, &["too early"]);
            cluster.warp_to(at);
            let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
            let window = &status["next"]["refusal_window"];
            assert_eq!((window["inside"].clone(), window["missed"].clone()), (Value::Bool(true), Value::Bool(false)), "{window}");
            assert_eq!(status["missed_refusal_evidence"], serde_json::json!([]));
        }

        let ran = drive(&cluster, &dir, step, at);
        receipts += 1;
        let (observations, pool_before) = (&ran.observations, &ran.pool_before);
        match step.id {
            "a1_closing_withdraw" => {
                assert_eq!(ran.at, earliest);
                let event: PayoutPaid = last_event(observations, step.id);
                assert!(event.closing);
                assert_eq!((event.gross_paid, event.toll, event.net), (572_414_815, 42_931_111, 529_483_704));
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (predicted.close1.paid_gross, predicted.close1.toll, predicted.close1.net)
                );
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped: the whole pot is paid");
                let closed: RoundClosed = last_event(observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (1, earliest + m14_plan::DEADLINE_SECS));
            }
            "advance_ticket_close1" => {
                let committed: TicketCommitted = last_event(observations, step.id);
                assert_eq!(committed.credited + committed.seed, committed.toll);
                assert_eq!(committed.toll, predicted.close1.toll);
            }
            "w_finalize_reopen" => {
                let finalized: RoundFinalized = last_event(observations, step.id);
                assert_eq!((finalized.close_id, finalized.new_round), (1, 2));
                assert_eq!(finalized.seed_carried, pool_before.seed);
                assert_eq!(finalized.seed_carried, predicted.seed_carried, "the carried seed equals the model");
            }
            "p1_settle_compound15" => {
                let event: PayoutPaid = last_event(observations, step.id);
                assert!(!event.closing, "C15 is solvent");
                assert_eq!(
                    (event.gross_paid, event.toll, event.net),
                    (predicted.c15.paid_gross, predicted.c15.toll, predicted.c15.net),
                    "C15 equals the pure arithmetic"
                );
            }
            "advance_ticket_c15" => {
                // F6: ticket 3 caps A1's Series A row and it leaves the registry.
                let row = cluster.with_world(|w| w.backer(a1, 0));
                assert!(!row.active && row.remaining().expect("remaining") <= 10, "A1 reached 124 percent in ticket 3");
            }
            "winner_claim_a_and_b" => {
                let event: BackerClaimed = last_event(observations, step.id);
                assert!(event.a_part > 0 && event.b_part > 0, "the winner's combined claim has both parts");
            }
            "w_settle_compound30" => {
                let event: PayoutPaid = last_event(observations, step.id);
                assert!(event.closing, "C30 is strictly insolvent");
                assert_eq!(pool_before.pot, predicted.r2_pot_at_c30, "the pot before C30 equals the model");
                assert_eq!(event.gross_paid, pool_before.pot, "pot-capped");
                let closed: RoundClosed = last_event(observations, step.id);
                assert_eq!((closed.close_id, closed.deadline), (2, at + m14_plan::DEADLINE_SECS));
            }
            "expire_at_deadline" => {
                let close2 = cluster.with_world(|w| w.close_record(2));
                assert_eq!(ran.receipt["result"], "refused_as_expected");
                assert_eq!(ran.receipt["refused_as_expected"]["error"], "DeadlineNotReached");
                assert_eq!(ran.receipt["refused_as_expected"]["sent"], false);
                assert!(observations.is_empty(), "the refused-by-design transaction is not sent");
                assert!(ran.at < close2.promotion_deadline, "the refusal is simulated inside the deadline");
                assert_eq!(close2.promotion, state::Promotion::Pending);
            }
            "expire_promotion_2" => {
                assert_eq!(ran.at, cluster.with_world(|w| w.close_record(2)).promotion_deadline + 1, "one second past the deadline");
                let event: PromotionSkipped = last_event(observations, step.id);
                assert_eq!(
                    (event.close_id, event.reason, event.stage),
                    (2, pclose::REASON_DEADLINE, pclose::STAGE_REQUESTED_UNFULFILLED)
                );
            }
            "finalize_2" => {
                let event: RoundFinalized = last_event(observations, step.id);
                assert_eq!((event.close_id, event.new_round), (2, 3));
            }
            _ => {}
        }
    }

    // ---- the end ---------------------------------------------------------------------------------------
    assert_eq!(receipts, steps.len());
    let status = ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(status["completed"].as_u64(), Some(steps.len() as u64));
    assert_eq!(status["next"], "the plan is complete");
    assert_eq!(status["missed_refusal_evidence"], serde_json::json!([]));
    assert_eq!(status["all_ok"], true, "{status}");
    let log = fs::read_to_string(dir.file("flow-receipts.jsonl")).unwrap();
    assert_eq!(log.lines().count(), steps.len(), "one receipt line per step");
    for (line, step) in log.lines().zip(&steps) {
        let entry: Value = serde_json::from_str(line).unwrap();
        assert_eq!(entry["step"], step.id);
        assert_eq!(entry["all_ok"], true, "{}", step.id);
    }
    println!(
        "M14 current devnet run: PASS - close 1 at {earliest} (A1's open {opened_at} + {}), owed - pot {owed_at_threshold} lamports; all {} steps passed their checks",
        m14_plan::CLOSE1_INSOLVENT_FROM_ELAPSED,
        steps.len()
    );
}

/// The expected refusal (step 37) is left until the deadline of close 2 has passed.  `flow status` then lists
/// it under `missed_refusal_evidence` and `all_ok` is false; neither the passed deadline nor a later step
/// (step 38, expired by a third party) counts it done; `flow next` refuses it and every later step.
#[test]
#[ignore = "needs MC_ORAO_PROFILE=devnet (see the file docs)"]
fn a_missed_refusal_window_is_reported_and_never_counted_done() {
    require_devnet_profile();
    let (mut world, keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let steps = m14_plan::plan();
    let genesis = START_TIME as u64;
    let args = |parts: &[&str]| flow(parts, &dir);
    let status = || ok(ops::run_with_cluster(&args(&["flow", "status"]), &cluster));
    assert_eq!(steps[36].id, "expire_at_deadline");
    for step in &steps[..36] {
        drive(&cluster, &dir, step, genesis + step.at);
    }
    let deadline = cluster.with_world(|w| w.close_record(2)).promotion_deadline;

    // Inside the window nothing is missed ...
    let inside = status();
    assert_eq!(inside["next"]["id"], "expire_at_deadline");
    assert_eq!(inside["next"]["refusal_window"]["missed"], false);
    assert_eq!(inside["missed_refusal_evidence"], serde_json::json!([]));
    assert_eq!(inside["all_ok"], true, "{inside}");
    // ... the deadline second itself is still inside (the program refuses until now > deadline) ...
    cluster.warp_to(deadline);
    let boundary = status();
    assert_eq!(boundary["next"]["refusal_window"]["inside"], true);
    assert_eq!(boundary["next"]["refusal_window"]["missed"], false);
    assert_eq!(boundary["missed_refusal_evidence"], serde_json::json!([]));

    // ... and one second later the program would ACCEPT the expiry: the window is missed.
    cluster.warp_to(deadline + 1);
    let missed = status();
    let entry = status_step(&missed, "expire_at_deadline");
    assert_eq!(entry["done"], false, "{entry}");
    assert!(entry["evidence"].as_str().expect("evidence").starts_with("MISSED"), "{entry}");
    assert_eq!(missed["missed_refusal_evidence"], serde_json::json!(["expire_at_deadline"]));
    assert_eq!(missed["all_ok"], false);
    assert_eq!(missed["completed"].as_u64(), Some(36));
    assert_eq!(missed["next"]["id"], "expire_at_deadline");
    assert_eq!(missed["next"]["refusal_window"]["missed"], true);
    assert_eq!(missed["next"]["refusal_window"]["inside"], false);
    assert_eq!(missed["next"]["refusal_window"]["deadline_unix"].as_u64(), Some(deadline));

    // The runner refuses the step (dry run and --execute) and the step after it.
    assert_refused_untouched(&cluster, &dir, "expire_at_deadline", &["MISSED"]);
    let sends = cluster.sends.get();
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "expire_promotion_2", "--execute"]), &cluster));
    assert!(message.contains("not the next step") && message.contains("MISSED"), "{message}");
    assert_eq!(cluster.sends.get(), sends);

    // Somebody else expires close 2 (permissionless), outside `flow next`: the chain now shows step 38's effect.
    let expire = cluster.with_world(|w| m7::expire_ix(w, 2));
    {
        let mut world = cluster.world.borrow_mut();
        let keeper = std::mem::replace(&mut world.rt.payer, keys[&Who::A1].insecure_clone());
        let outcome = world.send("third_party_expire", vec![expire], &[]);
        world.rt.payer = keeper;
        let observation = outcome.expect("anybody may expire a close once its deadline has passed");
        let skipped: PromotionSkipped = observation.last_event();
        assert_eq!((skipped.close_id, skipped.reason), (2, pclose::REASON_DEADLINE));
    }
    let after = status();
    assert_eq!(status_step(&after, "expire_promotion_2")["done"], true, "step 38 is done: its promotion is terminal");
    let entry = status_step(&after, "expire_at_deadline");
    assert_eq!(entry["done"], false, "a later step's completion does not stand in for the refusal: {entry}");
    assert!(entry["evidence"].as_str().expect("evidence").starts_with("MISSED"), "{entry}");
    assert_eq!(after["completed"].as_u64(), Some(37), "steps 1-36 and 38");
    assert_eq!(after["next"]["id"], "expire_at_deadline");
    assert_eq!(after["missed_refusal_evidence"], serde_json::json!(["expire_at_deadline"]));
    assert_eq!(after["all_ok"], false);
    // The plan stops there: step 37 stays refused and step 39 is not the next step.
    assert_refused_untouched(&cluster, &dir, "expire_at_deadline", &["MISSED"]);
    let sends = cluster.sends.get();
    let message = refused(ops::run_with_cluster(&args(&["flow", "next", "--step", "finalize_2", "--execute"]), &cluster));
    assert!(message.contains("not the next step"), "{message}");
    assert_eq!(cluster.sends.get(), sends);
}

/// A fresh cluster has no missed refusal evidence and no refusal window yet.
#[test]
fn a_fresh_status_reports_no_missed_refusal_evidence() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    let status = ok(ops::run_with_cluster(&flow(&["flow", "status"], &dir), &cluster));
    assert_eq!(status["missed_refusal_evidence"], serde_json::json!([]));
    assert_eq!(status["all_ok"], true, "{status}");
    assert!(status["next"].get("refusal_window").is_none(), "{status}");
}
