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
//! * The gate and refusal tests (out of order, too early, wrong wallet, devnet without the approval
//!   hash, `--local-oracle` off loopback, the unfulfilled oracle) run in a plain `cargo test`.
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

use musical_chairs_financial_runtime::{
    close as m7,
    devnet::{self, OraoProfile},
    devnet_ops::{self as ops, AddressSignature, Cluster, OpsError, SendOutcome, SimOutcome},
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
    fn send(&self, tx: &Transaction) -> Result<SendOutcome, String> {
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
            Err(observation) => Err(format!("{:?}: {:?}", observation.error, observation.logs)),
        }
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
fn flow_on_devnet_needs_the_approval_hash_and_never_the_local_oracle() {
    let (mut world, _keys, dir) = fixture();
    let cluster = WorldCluster::new(&mut world);
    init(&cluster, &dir);
    cluster.warp_to(START_TIME as u64 + m14_plan::T0);
    let packet = dir.file("packet.md");
    fs::write(&packet, "the approved M14 packet").unwrap();
    let approval = devnet::sha256_hex(b"the approved M14 packet");
    let run = |extra: &[&str]| {
        let mut args = flow(
            &["flow", "next", "--step", "fund_operations", "--execute", "--cluster", "devnet", "--packet", &packet],
            &dir,
        );
        let position = args.iter().position(|a| a == "--rpc").unwrap();
        args[position + 1] = DEVNET_URL.into();
        args.extend(extra.iter().map(|part| part.to_string()));
        ops::run_with_cluster(&args, &cluster)
    };

    // No hash, wrong hash: refused before any RPC call.
    assert!(refused(run(&[])).contains("--i-have-approval"));
    assert!(refused(run(&["--i-have-approval", &"cd".repeat(32)])).contains("not the SHA-256"));
    assert_eq!(cluster.genesis_reads.get(), 0);

    // The right hash but a cluster that is not devnet (mainnet, or the LiteSVM genesis): refused.
    cluster.set_genesis(ops::MAINNET_GENESIS_HASH);
    assert!(refused(run(&["--i-have-approval", &approval])).contains("genesis"));
    cluster.set_genesis(LITESVM_GENESIS);
    assert!(refused(run(&["--i-have-approval", &approval])).contains("genesis"));
    assert_eq!(cluster.sends.get(), 0);

    // Devnet genesis and the right hash: the gate opens, but the local oracle never does.
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    let message = refused(run(&["--i-have-approval", &approval, "--local-oracle"]));
    assert!(message.contains("--local-oracle is for a loopback validator only"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
    // --genesis-unix is a loopback override only.
    let message = refused(run(&["--i-have-approval", &approval, "--genesis-unix", "1"]));
    assert!(message.contains("loopback"), "{message}");

    let done = ok(run(&["--i-have-approval", &approval]));
    assert_eq!(done["mode"], "execute");
    assert_eq!(done["cluster"]["target"], "devnet");
    assert_eq!(done["all_ok"], true, "{done}");
    assert_eq!(cluster.sends.get(), 1);

    // An edit of the packet after approval closes the gate again.
    fs::write(&packet, "the approved M14 packet, edited").unwrap();
    assert!(refused(run(&["--i-have-approval", &approval])).contains("changed after approval"));
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
// The whole plan, through `flow next`
// ---------------------------------------------------------------------------

/// Balances of every plan wallet.
fn balances(cluster: &WorldCluster<'_>, keys: &BTreeMap<Who, Keypair>) -> BTreeMap<Who, u64> {
    cluster.with_world(|w| ALL_WALLETS.iter().map(|who| (*who, w.lamports(keys[who].pubkey()))).collect())
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
                assert_eq!(cluster.with_world(|w| w.close_record(2)).promotion, state::Promotion::Pending);
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
