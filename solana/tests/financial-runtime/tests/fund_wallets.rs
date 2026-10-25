//! M14-FUND-TOOL: `devnet_ops fund-wallets` (the guarded top-up of the plan wallets from the deployer's
//! remainder) against LOCAL clusters only.
//!
//! * A LiteSVM-backed [`Cluster`] double (system transfers need no program) counts simulations and sends and can
//!   misbehave like a real RPC fleet: a send whose outcome is unknown (a timeout, landing or not), a send that is
//!   definitely rejected, a status lookup that finds nothing, a lagging node that reports a funded wallet as
//!   empty.
//! * Plans and inventories are built in a throwaway git repository (so the `--plan-commit` pin runs the real
//!   `git cat-file` path) and never touch a real key, a real cluster or `~/.mc-devnet-ops`.
//! * One test dry-runs the REAL committed plan against the REAL committed inventory (read only).
//!
//! ```text
//! cargo +1.92.0 test --locked --test fund_wallets -- --test-threads=1
//! ```

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use litesvm::LiteSVM;
use musical_chairs_financial_runtime::{
    devnet,
    devnet_ops::{self as ops, Cluster, OpsError, SendError, SendOutcome, SigOutcome, SimOutcome},
};
use serde_json::{json, Value};
use solana_sdk::{
    account::Account,
    clock::Clock,
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

const RPC: &str = "http://127.0.0.1:8899";
const DEVNET_URL: &str = "https://api.devnet.solana.com";
const LITESVM_GENESIS: &str = "LiteSVMLocalGenesis11111111111111111111111111";
const PLAN_PATH: &str = "docs/solana/devnet-funding-plan.json";
const INVENTORY_PATH: &str = "docs/solana/evidence/test-inventory.json";
const SOL: u64 = 1_000_000_000;
const FEE: u64 = 5_000;
const RENT_FLOOR: u64 = 890_880;

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A temp dir the test owns (removed on drop).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "mc-fund-wallets-{tag}-{}-{}",
            std::process::id(),
            DIR_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }
    fn root(&self) -> &Path {
        &self.0
    }
    fn file(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=fund-test", "-c", "user.email=fund@test.invalid", "-c", "commit.gpgsign=false"])
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

/// `git` stdout, byte for byte (no trimming).
fn git_bytes(repo: &Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

fn write_keypair(path: &str, keypair: &Keypair) {
    fs::write(path, serde_json::to_string(&keypair.to_bytes().to_vec()).expect("json")).expect("write keypair");
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

fn failed(result: Result<Value, OpsError>) -> String {
    match result {
        Err(OpsError::Failed(message)) => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn pending_lines(path: &str) -> Vec<Value> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .lines()
            .map(|line| serde_json::from_str(line).expect("every pending line is JSON"))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn write_aheads(path: &str) -> Vec<Value> {
    pending_lines(path).into_iter().filter(|line| line["event"] == "write_ahead").collect()
}

/// A [`Cluster`] over a LiteSVM instance.  Counts simulations and sends; the hooks make it misbehave.
struct Fund<'a> {
    svm: RefCell<&'a mut LiteSVM>,
    genesis: RefCell<String>,
    sims: Cell<usize>,
    sends: Cell<usize>,
    landed: RefCell<BTreeMap<Signature, u64>>,
    /// Keys a lagging node reports as absent although the chain holds them.
    lagging: RefCell<BTreeSet<Pubkey>>,
    /// The n-th send (1-based, counted over the cluster's life) has an UNKNOWN outcome (0 = never) ...
    ambiguous_at: Cell<usize>,
    /// ... and the transaction really lands when this is set.
    land_ambiguous: Cell<bool>,
    /// The n-th send is definitely REJECTED and never lands (0 = never).
    reject_at: Cell<usize>,
    /// The status lookup finds nothing for a transaction that did land.
    hide_status: Cell<bool>,
    noted_slot: Cell<u64>,
}

impl<'a> Fund<'a> {
    fn new(svm: &'a mut LiteSVM, genesis: &str) -> Self {
        Fund {
            svm: RefCell::new(svm),
            genesis: RefCell::new(genesis.to_string()),
            sims: Cell::new(0),
            sends: Cell::new(0),
            landed: RefCell::new(BTreeMap::new()),
            lagging: RefCell::new(BTreeSet::new()),
            ambiguous_at: Cell::new(0),
            land_ambiguous: Cell::new(false),
            reject_at: Cell::new(0),
            hide_status: Cell::new(false),
            noted_slot: Cell::new(0),
        }
    }
    fn set_lamports(&self, key: &Pubkey, lamports: u64) {
        let account = Account { lamports, data: Vec::new(), owner: Pubkey::default(), executable: false, rent_epoch: 0 };
        self.svm.borrow_mut().set_account(*key, account).expect("set account");
    }
    fn set_account(&self, key: &Pubkey, account: Account) {
        self.svm.borrow_mut().set_account(*key, account).expect("set account");
    }
    fn balance(&self, key: &Pubkey) -> u64 {
        self.svm.borrow().get_account(key).map_or(0, |account| account.lamports)
    }
}

impl Cluster for Fund<'_> {
    fn genesis_hash(&self) -> Result<String, String> {
        Ok(self.genesis.borrow().clone())
    }
    fn blockhash(&self) -> Result<Hash, String> {
        let mut svm = self.svm.borrow_mut();
        svm.expire_blockhash();
        Ok(svm.latest_blockhash())
    }
    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String> {
        if self.lagging.borrow().contains(key) {
            return Ok(None);
        }
        Ok(self.svm.borrow().get_account(key).filter(|account| account.lamports > 0))
    }
    fn rent_minimum(&self, data_len: usize) -> Result<u64, String> {
        Ok(self.svm.borrow().minimum_balance_for_rent_exemption(data_len))
    }
    fn unix_time(&self) -> Result<i64, String> {
        Ok(self.svm.borrow().get_sysvar::<Clock>().unix_timestamp)
    }
    fn slot(&self) -> Result<u64, String> {
        Ok(self.svm.borrow().get_sysvar::<Clock>().slot)
    }
    fn note_confirmed_slot(&self, slot: u64) {
        self.noted_slot.set(self.noted_slot.get().max(slot));
    }
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String> {
        if !sig_verify {
            return Err("the LiteSVM double cannot simulate unsigned transactions".into());
        }
        self.sims.set(self.sims.get() + 1);
        match self.svm.borrow().simulate_transaction(tx.clone()) {
            Ok(info) => Ok(SimOutcome { error: None, logs: info.meta.logs, units_consumed: Some(info.meta.compute_units_consumed) }),
            Err(failure) => Ok(SimOutcome {
                error: Some(format!("{:?}", failure.err)),
                logs: failure.meta.logs,
                units_consumed: Some(failure.meta.compute_units_consumed),
            }),
        }
    }
    fn send(&self, tx: &Transaction) -> Result<SendOutcome, SendError> {
        let number = self.sends.get() + 1;
        self.sends.set(number);
        if self.reject_at.get() == number {
            return Err(SendError::Rejected("injected rejection".into()));
        }
        let ambiguous = self.ambiguous_at.get() == number;
        if ambiguous && !self.land_ambiguous.get() {
            return Err(SendError::Unknown("injected timeout (the transaction never left)".into()));
        }
        let slot = self.svm.borrow().get_sysvar::<Clock>().slot;
        let outcome = self.svm.borrow_mut().send_transaction(tx.clone());
        match outcome {
            Ok(meta) => {
                self.landed.borrow_mut().insert(meta.signature, slot);
                if ambiguous {
                    return Err(SendError::Unknown("injected timeout (the transaction landed)".into()));
                }
                Ok(SendOutcome { signature: meta.signature, units_consumed: Some(meta.compute_units_consumed) })
            }
            Err(failure) => Err(SendError::Rejected(format!("{:?}", failure.err))),
        }
    }
    fn signature_outcome(&self, signature: &Signature) -> Result<SigOutcome, String> {
        if self.hide_status.get() {
            return Ok(SigOutcome::Unknown);
        }
        Ok(self.landed.borrow().get(signature).map_or(SigOutcome::Unknown, |slot| SigOutcome::Confirmed { slot: *slot }))
    }
    fn signature_slot(&self, signature: &Signature) -> Result<Option<u64>, String> {
        Ok(self.landed.borrow().get(signature).copied())
    }
}

// ---------------------------------------------------------------------------
// The plan fixture
// ---------------------------------------------------------------------------

/// A deterministic recipient key for a role (no secret exists: a recipient only receives).
fn key_of(tag: &str) -> Pubkey {
    let digest = ops::sha256_bytes(format!("fund-wallets-test|{tag}").as_bytes());
    Pubkey::new_from_array(digest)
}

#[derive(Clone)]
struct Row {
    role: String,
    pubkey: Pubkey,
    bucket: String,
    target: u64,
}

fn row(role: &str, bucket: &str, target: u64) -> Row {
    Row { role: role.into(), pubkey: key_of(role), bucket: bucket.into(), target }
}

/// The plan the tool reads, as data.  `json()` derives every total from the rows, so a test changes ONE thing.
#[derive(Clone)]
struct PlanSpec {
    network: String,
    genesis: String,
    payer: Pubkey,
    rows: Vec<Row>,
    caps: BTreeMap<String, u64>,
    spent: BTreeMap<String, u64>,
    per_transfer_cap: u64,
    fee: u64,
    /// The inventory's pubkey per role when it is NOT the row's (an alias).
    inventory_override: BTreeMap<String, Pubkey>,
}

impl PlanSpec {
    fn new(network: &str, genesis: &str, payer: Pubkey, rows: Vec<Row>) -> PlanSpec {
        PlanSpec {
            network: network.into(),
            genesis: genesis.into(),
            payer,
            rows,
            caps: ops::FUND_BUCKET_CAPS.iter().map(|(name, cap)| (name.to_string(), *cap)).collect(),
            spent: BTreeMap::new(),
            per_transfer_cap: ops::FUND_PER_TRANSFER_CAP_LAMPORTS,
            fee: FEE,
            inventory_override: BTreeMap::new(),
        }
    }

    fn json(&self) -> Value {
        let mut buckets = serde_json::Map::new();
        for (name, cap) in &self.caps {
            let spent = self.spent.get(name).copied().unwrap_or(0);
            let in_bucket: Vec<&Row> = self.rows.iter().filter(|row| &row.bucket == name).collect();
            let total = spent + in_bucket.iter().map(|row| row.target).sum::<u64>() + in_bucket.len() as u64 * self.fee;
            buckets.insert(
                name.clone(),
                json!({ "cap_lamports": cap, "already_spent_lamports": spent, "history": [], "planned_total_with_fees_lamports": total }),
            );
        }
        json!({
            "schema": 1,
            "purpose": "test plan",
            "network": self.network,
            "genesis": self.genesis,
            "payer": { "role": "deployer D", "pubkey": self.payer.to_string() },
            "inventory": INVENTORY_PATH,
            "per_transfer_cap_lamports": self.per_transfer_cap,
            "fee_per_transfer_lamports": self.fee,
            "buckets": buckets,
            "targets": self.rows.iter().map(|row| json!({
                "role": row.role, "pubkey": row.pubkey.to_string(), "bucket": row.bucket, "target_lamports": row.target,
            })).collect::<Vec<_>>(),
            "not_in_this_plan": [],
            "totals": {
                "targets_lamports": self.rows.iter().map(|row| row.target).sum::<u64>(),
                "fees_lamports": self.rows.len() as u64 * self.fee,
            },
        })
    }

    fn inventory_json(&self) -> Value {
        let mut keys = serde_json::Map::new();
        for row in &self.rows {
            let key = self.inventory_override.get(&row.role).copied().unwrap_or(row.pubkey);
            keys.insert(row.role.clone(), json!(key.to_string()));
        }
        keys.insert("deployer D".into(), json!(self.payer.to_string()));
        json!({ "step": "test inventory", "keys": { "public_keys": keys } })
    }
}

fn default_rows() -> Vec<Row> {
    vec![
        row("G voter 1", "squads", 20_000_000),
        row("plan wallet S", "sponsor", 50_000_000),
        row("plan wallet K", "keeper", 5_000_000),
        row("plan wallet A1", "test_wallets", 30_000_000),
        row("plan wallet W", "test_wallets", 10_000_000),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Net {
    Local,
    Devnet,
}

/// A throwaway git repository holding a "packet" at the tool's fixed path plus the plan and the inventory, the
/// payer's key file and the write-ahead file's path.
struct Fx {
    dir: TempDir,
    net: Net,
    payer: Keypair,
    payer_file: String,
    pending: String,
    packet_sha: String,
    spec: PlanSpec,
}

impl Fx {
    fn new(net: Net) -> Fx {
        Fx::with_rows(net, default_rows())
    }

    fn with_rows(net: Net, rows: Vec<Row>) -> Fx {
        let dir = TempDir::new("repo");
        git(dir.root(), &["init", "-q"]);
        let payer = Keypair::new_from_array([0x91; 32]);
        let payer_file = dir.file("keys-payer.json");
        // The key file lives next to the repo, outside any tracked path (it is not added to a commit).
        write_keypair(&payer_file, &payer);
        let packet = "the reviewed M14 packet text";
        let packet_path = dir.root().join(ops::DEFAULT_PACKET_FILE);
        fs::create_dir_all(packet_path.parent().unwrap()).unwrap();
        fs::write(&packet_path, packet).unwrap();
        let (network, genesis) = match net {
            Net::Local => ("local", LITESVM_GENESIS),
            Net::Devnet => ("devnet", ops::DEVNET_GENESIS_HASH),
        };
        let spec = PlanSpec::new(network, genesis, payer.pubkey(), rows);
        let pending = dir.file("pending.jsonl");
        Fx { dir, net, payer, payer_file, pending, packet_sha: devnet::sha256_hex(packet.as_bytes()), spec }
    }

    fn root(&self) -> &Path {
        self.dir.root()
    }

    fn genesis(&self) -> &'static str {
        match self.net {
            Net::Local => LITESVM_GENESIS,
            Net::Devnet => ops::DEVNET_GENESIS_HASH,
        }
    }

    /// Write the plan and the inventory (after `edit` changed their JSON) and commit EVERYTHING that is tracked
    /// (the key file is excluded by a `.gitignore`-free trick: only `docs/` is added).  Returns the commit.
    fn commit_with(&self, edit: impl FnOnce(&mut Value, &mut Value)) -> String {
        let mut plan = self.spec.json();
        let mut inventory = self.spec.inventory_json();
        edit(&mut plan, &mut inventory);
        for (path, value) in [(PLAN_PATH, &plan), (INVENTORY_PATH, &inventory)] {
            let file = self.root().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, format!("{}\n", serde_json::to_string_pretty(value).unwrap())).unwrap();
        }
        git(self.root(), &["add", "docs"]);
        git(self.root(), &["commit", "-q", "--allow-empty", "-m", "plan"]);
        git(self.root(), &["rev-parse", "HEAD"])
    }

    fn commit(&self) -> String {
        self.commit_with(|_, _| {})
    }

    fn plan_file(&self) -> PathBuf {
        self.root().join(PLAN_PATH)
    }

    fn inventory_file(&self) -> PathBuf {
        self.root().join(INVENTORY_PATH)
    }

    fn cluster_for<'a>(&self, svm: &'a mut LiteSVM) -> Fund<'a> {
        let cluster = Fund::new(svm, self.genesis());
        cluster.set_lamports(&self.payer.pubkey(), 20 * SOL);
        cluster
    }

    /// `fund-wallets` command line.  `pin` = the `--plan-commit` to pass (and, on devnet, the packet commit).
    fn args(&self, execute: bool, pin: Option<&str>, extra: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = ["fund-wallets", "--payer", self.payer_file.as_str(), "--plan", PLAN_PATH, "--pending-file", self.pending.as_str(), "--slot-polls", "1"]
            .iter()
            .map(|part| part.to_string())
            .collect();
        match self.net {
            Net::Local => args.extend(["--rpc".to_string(), RPC.to_string()]),
            Net::Devnet => args.extend(["--rpc".to_string(), DEVNET_URL.to_string(), "--cluster".to_string(), "devnet".to_string()]),
        }
        if execute {
            args.push("--execute".into());
            if self.net == Net::Devnet {
                // The packet pin: any commit that holds the (unchanged) packet blob.
                let head = git(self.root(), &["rev-parse", "HEAD"]);
                args.extend(["--packet-sha256".to_string(), self.packet_sha.clone(), "--packet-commit".to_string(), head]);
            }
        }
        if let Some(commit) = pin {
            args.extend(["--plan-commit".to_string(), commit.to_string()]);
        }
        args.extend(extra.iter().map(|part| part.to_string()));
        args
    }

    fn run(&self, cluster: &dyn Cluster, execute: bool, pin: Option<&str>, extra: &[&str]) -> Result<Value, OpsError> {
        ops::run_with_cluster_in(&self.args(execute, pin, extra), cluster, self.root())
    }

    /// The recipient key the plan gives `role`.
    fn recipient(&self, role: &str) -> Pubkey {
        self.spec.rows.iter().find(|row| row.role == role).expect("role in the plan").pubkey
    }
}

fn step_of(role: &str) -> String {
    format!("A1.fund.{}", role.to_ascii_lowercase().replace(' ', "-"))
}

fn intent_of(role: &str, recipient: &Pubkey, target: u64, payer: &Pubkey, genesis: &str) -> String {
    devnet::sha256_hex(format!("{role}|{recipient}|{target}|{payer}|{genesis}").as_bytes())
}

/// Nothing was sent and no write-ahead record was made.
fn assert_nothing_sent(cluster: &Fund<'_>, sends_before: usize, fx: &Fx) {
    assert_eq!(cluster.sends.get(), sends_before, "ZERO sends");
    assert!(write_aheads(&fx.pending).is_empty(), "no write-ahead record: {:?}", pending_lines(&fx.pending));
}

fn outcomes(receipt: &Value) -> Vec<(String, String)> {
    receipt["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .map(|target| (target["role"].as_str().unwrap().to_string(), target["outcome"].as_str().unwrap().to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Normal success
// ---------------------------------------------------------------------------

#[test]
fn a_normal_run_funds_every_target_once_and_records_what_it_did() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let total: u64 = fx.spec.rows.iter().map(|row| row.target).sum();

    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["mode"], "execute");
    assert_eq!(cluster.sends.get(), 5);
    assert_eq!(cluster.sims.get(), 5, "every send is preceded by a simulation");
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), row.target, "{} holds exactly its target", row.role);
    }
    assert_eq!(receipt["totals"]["funded"], 5);
    assert_eq!(receipt["totals"]["lamports_sent"], total);
    assert_eq!(receipt["totals"]["fees_lamports"], 5 * FEE);
    let before = receipt["payer_balance_before"].as_u64().unwrap();
    let after = receipt["payer_balance_after"].as_u64().unwrap();
    assert_eq!(before, 20 * SOL);
    assert_eq!(after, cluster.balance(&fx.payer.pubkey()));
    assert_eq!(before - after, total + 5 * FEE, "the payer paid the targets plus one signature fee each");
    // Per-target outcomes, in plan order.
    assert_eq!(
        outcomes(&receipt),
        fx.spec.rows.iter().map(|row| (row.role.clone(), "funded".to_string())).collect::<Vec<_>>()
    );
    assert!(receipt["targets"].as_array().unwrap().iter().all(|target| target["balance_before"] == 0));
    assert_eq!(receipt["checks"].as_array().unwrap().len(), 5);

    // The envelope the receipt reports.
    assert_eq!(receipt["envelope"]["per_transfer_cap_lamports"], 2 * SOL);
    let squads = receipt["envelope"]["buckets"].as_array().unwrap().iter().find(|b| b["bucket"] == "squads").unwrap();
    assert_eq!(squads["total_lamports"], 20_000_000 + FEE);

    // The plan pins are in the receipt (no commit on a loopback run without --plan-commit) ...
    let plan_sha = devnet::sha256_hex(&fs::read(fx.plan_file()).unwrap());
    let inventory_sha = devnet::sha256_hex(&fs::read(fx.inventory_file()).unwrap());
    assert_eq!(receipt["plan"], json!({ "path": PLAN_PATH, "commit": null, "sha256": plan_sha }));
    assert_eq!(receipt["inventory"], json!({ "path": INVENTORY_PATH, "commit": null, "sha256": inventory_sha }));

    // ... and the write-ahead file holds one write-ahead + one confirmed resolution per target, in plan order,
    // each annotated with the recipient, the semantic intent and the plan pins.
    let lines = pending_lines(&fx.pending);
    assert_eq!(lines.len(), 10, "{lines:?}");
    let payer = fx.payer.pubkey();
    for (index, row) in fx.spec.rows.iter().enumerate() {
        let write_ahead = &lines[index * 2];
        let resolved = &lines[index * 2 + 1];
        assert_eq!(write_ahead["event"], "write_ahead");
        assert_eq!(write_ahead["command"], "fund-wallets");
        assert_eq!(write_ahead["step"], step_of(&row.role).as_str());
        assert_eq!(write_ahead["fee_payer"], payer.to_string().as_str());
        assert!(write_ahead["packet"].is_null(), "a loopback run has no packet pin");
        let fund = &write_ahead["fund"];
        assert_eq!(fund["role"], row.role.as_str());
        assert_eq!(fund["recipient"], row.pubkey.to_string().as_str());
        assert_eq!(fund["target_lamports"], row.target);
        assert_eq!(fund["top_up_lamports"], row.target);
        assert_eq!(fund["intent_sha256"], intent_of(&row.role, &row.pubkey, row.target, &payer, LITESVM_GENESIS).as_str());
        assert_eq!(fund["plan"]["sha256"], plan_sha.as_str());
        assert_eq!(fund["inventory"]["sha256"], inventory_sha.as_str());
        assert_eq!((resolved["event"].as_str(), resolved["outcome"].as_str()), (Some("resolved"), Some("confirmed")));
        assert_eq!(resolved["signature"], write_ahead["signature"]);
    }
}

#[test]
fn a_dry_run_sends_nothing_and_reports_what_it_would_fund() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let receipt = ok(fx.run(&cluster, false, None, &[]));
    assert_eq!(receipt["mode"], "dry-run");
    assert_eq!(receipt["all_ok"], true);
    assert_eq!(receipt["totals"]["would_fund"], 5);
    assert_eq!(receipt["totals"]["funded"], 0);
    assert!(outcomes(&receipt).iter().all(|(_, outcome)| outcome == "would_fund"), "{receipt}");
    assert_nothing_sent(&cluster, 0, &fx);
    assert_eq!(cluster.sims.get(), 0, "the dry run simulates nothing unless --simulate is given");
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), 0);
    }
}

#[test]
fn the_payer_may_spend_down_to_exactly_the_rent_floor_and_not_one_lamport_less() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let total: u64 = fx.spec.rows.iter().map(|row| row.target).sum::<u64>() + 5 * FEE;

    // One lamport short of (targets + fees + the floor the payer must keep): refused before the first send.
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    cluster.set_lamports(&fx.payer.pubkey(), total + RENT_FLOOR - 1);
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("needs") && message.contains("rent floor") && message.contains("nothing was sent"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);
    assert_eq!(cluster.balance(&fx.payer.pubkey()), total + RENT_FLOOR - 1, "the payer is untouched");

    // Exactly enough: every transfer lands and the payer keeps its floor (the runtime accepts it).
    cluster.set_lamports(&fx.payer.pubkey(), total + RENT_FLOOR);
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(cluster.sends.get(), 5);
    assert_eq!(cluster.balance(&fx.payer.pubkey()), RENT_FLOOR);
}

// ---------------------------------------------------------------------------
// Whole-plan validation: every failure refuses with ZERO sends
// ---------------------------------------------------------------------------

/// Run the plan the fixture committed (after `edit`) on a loopback cluster and return the refusal.
fn refusal_of(fx: &Fx, edit: impl FnOnce(&mut Value, &mut Value)) -> String {
    fx.commit_with(edit);
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert_nothing_sent(&cluster, 0, fx);
    assert_eq!(cluster.sims.get(), 0, "nothing was even simulated");
    message
}

#[test]
fn a_target_above_the_per_transfer_cap_is_refused_not_clamped() {
    let mut fx = Fx::new(Net::Local);
    fx.spec.rows[3].target = 2 * SOL + 1; // plan wallet A1, test_wallets bucket (cap 7 SOL): only the per-transfer cap trips
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("above the per-transfer cap") && message.contains("never clamped"), "{message}");
    // Exactly the cap is fine.
    let mut at_cap = Fx::new(Net::Local);
    at_cap.spec.rows[3].target = 2 * SOL;
    at_cap.commit();
    let mut svm = LiteSVM::new();
    let cluster = at_cap.cluster_for(&mut svm);
    let receipt = ok(at_cap.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true);
    assert_eq!(cluster.balance(&at_cap.recipient("plan wallet A1")), 2 * SOL);
}

#[test]
fn a_bucket_over_its_cap_counting_history_targets_and_fees_is_refused() {
    // Targets alone over the 0.5 SOL Squads cap.
    let mut fx = Fx::with_rows(
        Net::Local,
        vec![row("G voter 1", "squads", 300_000_000), row("G voter 2", "squads", 150_000_000), row("P voter 1", "squads", 60_000_000)],
    );
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("bucket `squads`") && message.contains("is above its cap") && message.contains("new approval"), "{message}");

    // Under the cap by targets alone, over it once the fees count: 499,990,000 + 2 x 5,000 = 500,000,000 is fine,
    // one lamport more is not.
    fx.spec.rows = vec![row("G voter 1", "squads", 300_000_000), row("G voter 2", "squads", 199_990_000)];
    fx.commit();
    {
        let mut svm = LiteSVM::new();
        let cluster = fx.cluster_for(&mut svm);
        assert_eq!(ok(fx.run(&cluster, false, None, &[]))["all_ok"], true, "exactly at the cap is allowed");
    }
    fx.spec.rows[1].target += 1;
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("bucket `squads`") && message.contains("2 x 5000 fees"), "{message}");

    // The bucket's already-spent history counts (the real plan's Squads bucket starts at 3,667,440).
    let mut spent = Fx::with_rows(Net::Local, vec![row("G voter 1", "squads", 100_000_000)]);
    spent.spec.spent.insert("squads".into(), 400_000_000 - FEE);
    spent.commit();
    {
        let mut svm = LiteSVM::new();
        let cluster = spent.cluster_for(&mut svm);
        assert_eq!(ok(spent.run(&cluster, false, None, &[]))["all_ok"], true, "history + target + fee = the cap exactly");
    }
    spent.spec.spent.insert("squads".into(), 400_000_000);
    let message = refusal_of(&spent, |_, _| {});
    assert!(message.contains("already spent 400000000"), "{message}");
}

#[test]
fn a_plan_claiming_a_larger_cap_or_a_different_fee_is_refused() {
    let fx = Fx::new(Net::Local);
    let message = refusal_of(&fx, |plan, _| plan["per_transfer_cap_lamports"] = json!(3 * SOL));
    assert!(message.contains("claims a per-transfer cap of 3000000000") && message.contains("never clamped"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["per_transfer_cap_lamports"] = json!(SOL));
    assert!(message.contains("must be exactly"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["buckets"]["test_wallets"]["cap_lamports"] = json!(8 * SOL));
    assert!(message.contains("bucket `test_wallets` claims a cap of 8000000000") && message.contains("never clamped"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["buckets"]["squads"]["cap_lamports"] = json!(600_000_000u64));
    assert!(message.contains("bucket `squads` claims a cap of 600000000"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["fee_per_transfer_lamports"] = json!(1));
    assert!(message.contains("fee_per_transfer_lamports must be exactly"), "{message}");

    // A plan that claims a SMALLER bucket cap is held to it (the stricter number wins).
    let mut tight = Fx::new(Net::Local);
    tight.spec.caps.insert("keeper".into(), 4_000_000); // the keeper's 5,000,000 target no longer fits
    let message = refusal_of(&tight, |_, _| {});
    assert!(message.contains("bucket `keeper`") && message.contains("is above its cap 4000000"), "{message}");
}

#[test]
fn a_command_total_above_the_sum_of_the_bucket_caps_is_refused() {
    // Every bucket filled to its cap by targets alone (8.6 SOL): 17 targets, each at most 2 SOL.
    let rows = vec![
        row("G voter 1", "squads", 100_000_000),
        row("G voter 2", "squads", 80_000_000),
        row("G voter 3", "squads", 80_000_000),
        row("P voter 1", "squads", 80_000_000),
        row("P voter 2", "squads", 80_000_000),
        row("P voter 3", "squads", 80_000_000),
        row("plan wallet S", "sponsor", 990_000_000),
        row("management recipient", "sponsor", 10_000_000),
        row("plan wallet K", "keeper", 100_000_000),
        row("plan wallet A1", "test_wallets", 1_500_000_000),
        row("plan wallet A2", "test_wallets", 500_000_000),
        row("plan wallet P1", "test_wallets", 1_000_000_000),
        row("plan wallet P2", "test_wallets", 1_000_000_000),
        row("plan wallet P3", "test_wallets", 1_000_000_000),
        row("plan wallet P4", "test_wallets", 1_000_000_000),
        row("plan wallet P5", "test_wallets", 500_000_000),
        row("plan wallet W", "test_wallets", 500_000_000),
    ];
    assert_eq!(rows.len(), ops::FUND_ROLES.len());
    assert_eq!(rows.iter().map(|row| row.target).sum::<u64>(), 8_600_000_000);
    let fx = Fx::with_rows(Net::Local, rows);
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("the command's total") && message.contains("sum of the packet's bucket caps"), "{message}");
}

#[test]
fn a_duplicate_role_a_duplicate_pubkey_and_an_unknown_role_are_refused() {
    // Duplicate role (same pubkey, so the inventory agrees).
    let mut fx = Fx::new(Net::Local);
    fx.spec.rows.push(fx.spec.rows[0].clone());
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("role `G voter 1` appears twice"), "{message}");

    // Duplicate pubkey under two different roles (the inventory maps both to the same key: no mismatch to hide behind).
    let mut fx = Fx::new(Net::Local);
    fx.spec.rows[1].pubkey = fx.spec.rows[0].pubkey;
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("appears twice") && message.contains("pubkey"), "{message}");

    // A role outside the 17-name allowlist: an invented one, a near miss, the payer's own role, a case variant.
    for role in ["plan wallet Z", "G voter 4", "deployer D", "g voter 1", "plan wallet S "] {
        let mut fx = Fx::new(Net::Local);
        fx.spec.rows[4] = Row { role: role.into(), pubkey: key_of(role), bucket: "test_wallets".into(), target: 10_000_000 };
        let message = refusal_of(&fx, |_, _| {});
        assert!(message.contains(&format!("role `{role}` is not one of the 17 plan roles")), "{role}: {message}");
    }
}

#[test]
fn an_inventory_mismatch_a_missing_role_and_a_self_target_are_refused() {
    // The plan pubkey is not the inventory's: no aliases.
    let mut fx = Fx::new(Net::Local);
    fx.spec.inventory_override.insert("plan wallet S".into(), key_of("somebody else"));
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("`plan wallet S`") && message.contains("in the inventory") && message.contains("no aliases"), "{message}");

    // A role the inventory lacks.
    let fx = Fx::new(Net::Local);
    let message = refusal_of(&fx, |_, inventory| {
        inventory["keys"]["public_keys"].as_object_mut().unwrap().remove("plan wallet K");
    });
    assert!(message.contains("`plan wallet K` is not in the inventory"), "{message}");

    // The payer itself as a target (the inventory agrees, so only the self-target rule can fire).
    let mut fx = Fx::new(Net::Local);
    let payer = fx.payer.pubkey();
    fx.spec.rows[2].pubkey = payer;
    let message = refusal_of(&fx, |_, _| {});
    assert!(message.contains("is the payer itself"), "{message}");

    // The inventory's deployer is not the payer; the plan's payer is not the --payer key.
    let fx = Fx::new(Net::Local);
    let message = refusal_of(&fx, |_, inventory| inventory["keys"]["public_keys"]["deployer D"] = json!(key_of("another deployer").to_string()));
    assert!(message.contains("inventory's `deployer D` is not the --payer key"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["payer"]["pubkey"] = json!(key_of("another payer").to_string()));
    assert!(message.contains("plan.payer.pubkey") && message.contains("is not the --payer key"), "{message}");
}

#[test]
fn zero_negative_fractional_and_below_the_rent_floor_targets_are_refused() {
    let fx = Fx::new(Net::Local);
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["target_lamports"] = json!(0));
    assert!(message.contains("target of zero lamports"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["target_lamports"] = json!(-5));
    assert!(message.contains("non-negative integer") && message.contains("-5"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["target_lamports"] = json!(1.5e7));
    assert!(message.contains("non-negative integer"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["target_lamports"] = json!("20000000"));
    assert!(message.contains("non-negative integer"), "{message}");
    // Overflow cannot sneak through the sums: a number beyond u64 is refused as a number.
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["target_lamports"] = json!(u64::MAX as f64 * 4.0));
    assert!(message.contains("non-negative integer"), "{message}");

    // A wallet funded below the floor of an empty account could not be created.
    let mut small = Fx::new(Net::Local);
    small.spec.rows[2].target = RENT_FLOOR - 1;
    let message = refusal_of(&small, |_, _| {});
    assert!(message.contains("rent-exempt minimum") && message.contains("plan wallet K"), "{message}");
}

#[test]
fn a_wrong_bucket_an_unknown_bucket_an_unknown_key_and_a_plan_for_another_cluster_are_refused() {
    let fx = Fx::new(Net::Local);
    let message = refusal_of(&fx, |plan, _| plan["targets"][1]["bucket"] = json!("keeper"));
    assert!(message.contains("`plan wallet S` is charged to bucket `sponsor`, not `keeper`"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["targets"][1]["bucket"] = json!("treasury"));
    assert!(message.contains("unknown bucket `treasury`"), "{message}");
    let message = refusal_of(&fx, |plan, _| {
        plan["buckets"]["treasury"] = json!({ "cap_lamports": 1, "already_spent_lamports": 0 });
    });
    assert!(message.contains("unknown bucket `treasury`"), "{message}");
    let message = refusal_of(&fx, |plan, _| {
        plan["buckets"].as_object_mut().unwrap().remove("sponsor");
    });
    assert!(message.contains("bucket `sponsor` is not declared"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["priority_fee_lamports"] = json!(1));
    assert!(message.contains("unknown key `priority_fee_lamports`"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["targets"][0]["memo"] = json!("x"));
    assert!(message.contains("unknown key `memo`"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["schema"] = json!(2));
    assert!(message.contains("`schema` must be 1"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["network"] = json!("devnet"));
    assert!(message.contains("`network` is `devnet` but this run targets `local`"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["genesis"] = json!(ops::MAINNET_GENESIS_HASH));
    assert!(message.contains("not the cluster's genesis hash"), "{message}");
    // The plan's own totals must agree with its targets.
    let message = refusal_of(&fx, |plan, _| plan["totals"]["targets_lamports"] = json!(1));
    assert!(message.contains("plan.totals disagrees"), "{message}");
    let message = refusal_of(&fx, |plan, _| plan["buckets"]["keeper"]["planned_total_with_fees_lamports"] = json!(1));
    assert!(message.contains("planned_total_with_fees_lamports 1 is not the computed"), "{message}");
}

#[test]
fn a_plan_with_duplicate_json_keys_a_bad_inventory_or_a_keypair_file_is_refused() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    // A duplicate object key would let two readers disagree about what the plan says.
    let text = fs::read_to_string(fx.plan_file()).unwrap();
    fs::write(fx.plan_file(), text.replacen("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,", 1)).unwrap();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("not strict JSON") && message.contains("duplicate key `schema`"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);

    // The same for the inventory: two entries for one role.
    fx.commit();
    let text = fs::read_to_string(fx.inventory_file()).unwrap();
    let doubled = text.replacen("\"deployer D\":", "\"deployer D\": \"x\",\n      \"deployer D\":", 1);
    assert_ne!(text, doubled);
    fs::write(fx.inventory_file(), doubled).unwrap();
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("not strict JSON") && message.contains("duplicate key `deployer D`"), "{message}");

    // An inventory without keys.public_keys.
    let message = refusal_of(&fx, |_, inventory| *inventory = json!({ "keys": {} }));
    assert!(message.contains("no `keys.public_keys` object"), "{message}");

    // A keypair-shaped file is never read as a plan, whatever its name.
    let keypair_plan = fx.root().join("docs/solana/keypair-plan.json");
    fs::write(&keypair_plan, serde_json::to_string(&fx.payer.to_bytes().to_vec()).unwrap()).unwrap();
    let mut args = fx.args(true, None, &[]);
    let position = args.iter().position(|arg| arg == "--plan").unwrap();
    args[position + 1] = "docs/solana/keypair-plan.json".into();
    let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
    assert!(message.contains("looks like a keypair file"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
}

#[test]
fn a_recipient_that_is_not_a_plain_system_wallet_is_refused_before_anything_is_sent() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    // The LAST target is a program-owned account: no earlier target may be sent first.
    cluster.set_account(
        &fx.recipient("plan wallet W"),
        Account { lamports: 5_000_000, data: vec![1, 2, 3], owner: Pubkey::new_unique(), executable: false, rent_epoch: 0 },
    );
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("not a plain system wallet") && message.contains("plan wallet W"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);
}

#[test]
fn an_insufficient_payer_is_refused_before_the_first_send_even_when_the_early_targets_are_affordable() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    // Enough for the first four targets, not for the fifth: nothing at all may be sent.
    let first_four: u64 = fx.spec.rows[..4].iter().map(|row| row.target).sum::<u64>() + 4 * FEE + RENT_FLOOR;
    cluster.set_lamports(&fx.payer.pubkey(), first_four);
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("needs") && message.contains("to fund 5 target(s)") && message.contains("nothing was sent"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), 0, "{}", row.role);
    }
    // A dry run refuses too: the plan cannot be carried out.
    let message = refused(fx.run(&cluster, false, None, &[]));
    assert!(message.contains("needs"), "{message}");
}

// ---------------------------------------------------------------------------
// No-op, partial balances, and the once-per-recipient guard
// ---------------------------------------------------------------------------

#[test]
fn a_wallet_already_at_its_target_is_a_noop_and_a_partial_balance_is_topped_up_by_the_difference() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    for row in &fx.spec.rows {
        cluster.set_lamports(&row.pubkey, row.target); // exactly at the target
    }
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert!(outcomes(&receipt).iter().all(|(_, outcome)| outcome == "noop_already_at_target"), "{receipt}");
    assert_eq!(receipt["totals"]["noop_already_at_target"], 5);
    assert_eq!(receipt["totals"]["funded"], 0);
    assert_nothing_sent(&cluster, 0, &fx);
    assert_eq!(receipt["payer_balance_before"], receipt["payer_balance_after"], "no fee was paid");

    // Above the target is a no-op as well (never an over-fund); below it is topped up by the DIFFERENCE only.
    let a1 = fx.recipient("plan wallet A1");
    let w = fx.recipient("plan wallet W");
    cluster.set_lamports(&a1, 30_000_000 + 7);
    cluster.set_lamports(&w, 4_000_000); // target 10,000,000
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 1, "only the short wallet is paid");
    assert_eq!(cluster.balance(&a1), 30_000_000 + 7);
    assert_eq!(cluster.balance(&w), 10_000_000);
    let w_row = receipt["targets"].as_array().unwrap().iter().find(|t| t["role"] == "plan wallet W").unwrap();
    assert_eq!((w_row["outcome"].as_str(), w_row["balance_before"].as_u64(), w_row["top_up_lamports"].as_u64()), (Some("funded"), Some(4_000_000), Some(6_000_000)));
    let lines = write_aheads(&fx.pending);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["fund"]["top_up_lamports"], 6_000_000);
    assert_eq!(lines[0]["fund"]["target_lamports"], 10_000_000);
    // The intent hashes the TARGET, not the top-up.
    assert_eq!(
        lines[0]["fund"]["intent_sha256"],
        intent_of("plan wallet W", &w, 10_000_000, &fx.payer.pubkey(), LITESVM_GENESIS).as_str()
    );
}

#[test]
fn a_lagging_node_that_reports_funded_wallets_unfunded_cannot_make_the_tool_pay_twice() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 5);
    let payer_after_first = cluster.balance(&fx.payer.pubkey());

    // Every recipient looks unfunded again: to a lagging node, and (for two of them) on chain as well.
    for row in &fx.spec.rows {
        cluster.lagging.borrow_mut().insert(row.pubkey);
    }
    cluster.set_lamports(&fx.recipient("plan wallet K"), 0);
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 5, "no second transfer to anyone");
    assert!(outcomes(&receipt).iter().all(|(_, outcome)| outcome == "already_funded_by_record"), "{receipt}");
    assert_eq!(receipt["totals"]["already_funded_by_record"], 5);
    assert_eq!(cluster.balance(&fx.payer.pubkey()), payer_after_first);
    let first = &receipt["targets"][0];
    assert_eq!(first["record"]["matched_by_role"], true);
    assert_eq!(first["record"]["matched_by_recipient"], true);
    assert_eq!(first["record"]["same_intent"], true);
    assert!(first["record"]["signature"].is_string() && first["record"]["slot"].is_u64(), "{first}");
    assert_eq!(write_aheads(&fx.pending).len(), 5, "no new write-ahead record");
}

#[test]
fn a_changed_target_after_a_confirmed_record_sends_nothing() {
    let mut fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 5);

    // The plan is edited (a higher target for one wallet, a lower one for another) and re-committed.
    fx.spec.rows[3].target = 45_000_000; // plan wallet A1: was 30,000,000
    fx.spec.rows[1].target = 40_000_000; // plan wallet S: was 50,000,000
    fx.commit();
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 5, "a changed target is a new approval, not a second transfer");
    assert!(outcomes(&receipt).iter().all(|(_, outcome)| outcome == "already_funded_by_record"), "{receipt}");
    let a1 = receipt["targets"].as_array().unwrap().iter().find(|t| t["role"] == "plan wallet A1").unwrap();
    assert_eq!(a1["record"]["matched_by_role"], true);
    assert_eq!(a1["record"]["same_intent"], false, "the record says the intent changed");
    assert_eq!(cluster.balance(&fx.recipient("plan wallet A1")), 30_000_000);
    assert_eq!(write_aheads(&fx.pending).len(), 5);
}

#[test]
fn a_changed_recipient_for_a_funded_role_is_refused_and_the_old_record_still_blocks() {
    // One funded role: A1.
    let mut fx = Fx::with_rows(Net::Local, vec![row("plan wallet A1", "test_wallets", 30_000_000)]);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    ok(fx.run(&cluster, true, None, &[]));
    let old = fx.recipient("plan wallet A1");
    assert_eq!(cluster.balance(&old), 30_000_000);
    assert_eq!(cluster.sends.get(), 1);

    // (1) The plan names another pubkey for the role but the inventory still says the old one: no aliases.
    let new_key = key_of("a replacement for A1");
    fx.spec.rows[0].pubkey = new_key;
    fx.spec.inventory_override.insert("plan wallet A1".into(), old);
    fx.commit();
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("no aliases") && message.contains("plan wallet A1"), "{message}");
    assert_eq!(cluster.sends.get(), 1);

    // (2) Plan and inventory both name the new pubkey (a new inventory commit): the ROLE's confirmed record still
    // blocks, and the new address is not funded.
    fx.spec.inventory_override.clear();
    fx.commit();
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(outcomes(&receipt), vec![("plan wallet A1".to_string(), "already_funded_by_record".to_string())]);
    let record = &receipt["targets"][0]["record"];
    assert_eq!((record["matched_by_role"].as_bool(), record["matched_by_recipient"].as_bool()), (Some(true), Some(false)));
    assert_eq!(record["recipient"], old.to_string().as_str());
    assert_eq!(cluster.balance(&new_key), 0);
    assert_eq!(cluster.sends.get(), 1);

    // (3) The funded ADDRESS under another role: the RECIPIENT's confirmed record blocks it (any role, any intent).
    let mut other = Fx::with_rows(Net::Local, vec![row("plan wallet A2", "test_wallets", 30_000_000)]);
    other.spec.rows[0].pubkey = old;
    other.pending = fx.pending.clone();
    other.commit();
    let receipt = ok(other.run(&cluster, true, None, &[]));
    assert_eq!(outcomes(&receipt), vec![("plan wallet A2".to_string(), "already_funded_by_record".to_string())]);
    let record = &receipt["targets"][0]["record"];
    assert_eq!((record["matched_by_role"].as_bool(), record["matched_by_recipient"].as_bool()), (Some(false), Some(true)));
    assert_eq!(record["step"], "A1.fund.plan-wallet-a1");
    assert_eq!(cluster.sends.get(), 1, "still the single original transfer");
}

#[test]
fn records_of_another_cluster_and_closed_failures_do_not_block_a_funding() {
    let fx = Fx::with_rows(Net::Local, vec![row("plan wallet A1", "test_wallets", 30_000_000)]);
    fx.commit();
    let a1 = fx.recipient("plan wallet A1");
    let foreign_signature = Signature::new_unique().to_string();
    let failed_signature = Signature::new_unique().to_string();
    let annotation = json!({ "role": "plan wallet A1", "recipient": a1.to_string(), "intent_sha256": "x" });
    let records = [
        // Confirmed, but on ANOTHER cluster: counts for nothing.
        json!({ "schema": 1, "event": "write_ahead", "step": "A1.fund.plan-wallet-a1", "signature": foreign_signature,
                "blockhash": Hash::new_unique().to_string(), "genesis_hash": ops::MAINNET_GENESIS_HASH, "fund": annotation }),
        json!({ "schema": 1, "event": "resolved", "signature": foreign_signature, "outcome": "confirmed", "slot": 9, "genesis_hash": ops::MAINNET_GENESIS_HASH }),
        // Closed as failed on chain on this cluster: it moved nothing.
        json!({ "schema": 1, "event": "write_ahead", "step": "A1.fund.plan-wallet-a1", "signature": failed_signature,
                "blockhash": Hash::new_unique().to_string(), "genesis_hash": LITESVM_GENESIS, "fund": annotation }),
        json!({ "schema": 1, "event": "resolved", "signature": failed_signature, "outcome": "failed_on_chain", "genesis_hash": LITESVM_GENESIS }),
    ];
    fs::write(&fx.pending, records.iter().map(|record| format!("{record}\n")).collect::<String>()).unwrap();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(outcomes(&receipt), vec![("plan wallet A1".to_string(), "funded".to_string())]);
    assert_eq!(cluster.balance(&a1), 30_000_000);
}

#[test]
fn a_malformed_fund_annotation_in_the_write_ahead_file_fails_closed() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let record = json!({
        "schema": 1, "event": "write_ahead", "step": "A1.fund.g-voter-1", "signature": Signature::new_unique().to_string(),
        "blockhash": Hash::new_unique().to_string(), "genesis_hash": LITESVM_GENESIS, "fund": { "recipient": "not a pubkey" },
    });
    fs::write(&fx.pending, format!("{record}\n")).unwrap();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("line 1") && message.contains("`fund` annotation lacks a valid `recipient`"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
}

// ---------------------------------------------------------------------------
// Held, ambiguous and rejected sends
// ---------------------------------------------------------------------------

#[test]
fn an_ambiguous_send_ends_held_and_nothing_further_is_sent_by_any_command() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    // The 3rd send (plan wallet K) times out, lands, and the status lookup then finds nothing (the blockhash is long dead).
    cluster.ambiguous_at.set(3);
    cluster.land_ambiguous.set(true);
    cluster.hide_status.set(true);
    let message = failed(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("INCIDENT") && message.contains("HELD") && message.contains("plan wallet K"), "{message}");
    assert_eq!(cluster.sends.get(), 3, "the 4th and 5th targets were never attempted");
    assert_eq!(cluster.balance(&fx.recipient("plan wallet K")), 5_000_000, "the ambiguous transfer did land");
    assert_eq!(cluster.balance(&fx.recipient("plan wallet A1")), 0);
    assert_eq!(cluster.balance(&fx.recipient("plan wallet W")), 0);
    let lines = pending_lines(&fx.pending);
    assert_eq!(write_aheads(&fx.pending).len(), 3);
    assert_eq!(lines.iter().filter(|l| l["event"] == "resolved").count(), 2, "only the first two are closed");
    assert_eq!(lines.last().unwrap()["event"], "held_unknown");
    let held_signature = lines.last().unwrap()["signature"].as_str().unwrap().to_string();

    // Every later --execute command (this one, fund-vault's family included) refuses before building anything.
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("HELD UNKNOWN") && message.contains(&held_signature) && message.contains("NO command to clear"), "{message}");
    assert_eq!(cluster.sends.get(), 3);
    // A dry run refuses to plan past the open record of a target.
    let message = refused(fx.run(&cluster, false, None, &[]));
    assert!(message.contains("OPEN") && message.contains("plan wallet K") && message.contains(&held_signature), "{message}");
    assert_eq!(cluster.sends.get(), 3);
    assert!(!pending_lines(&fx.pending).iter().any(|l| l["event"] == "resolved" && l["signature"] == held_signature.as_str()), "never closed by the tool");
}

#[test]
fn a_partial_batch_resumes_after_history_finds_the_held_send_and_never_resends_what_is_confirmed() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    cluster.ambiguous_at.set(3);
    cluster.land_ambiguous.set(true);
    cluster.hide_status.set(true);
    failed(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 3);
    let payer_after_partial = cluster.balance(&fx.payer.pubkey());

    // Still held: the command refuses and sends nothing.
    refused(fx.run(&cluster, true, None, &[]));
    assert_eq!(cluster.sends.get(), 3);

    // History now finds the third transaction confirmed: it is resolved, and ONLY the remaining targets are sent.
    cluster.hide_status.set(false);
    cluster.ambiguous_at.set(0);
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["pending_resolved"][0]["outcome"], "confirmed");
    assert_eq!(cluster.sends.get(), 5, "two more sends: the 4th and 5th targets");
    let by_record = "already_funded_by_record".to_string();
    let funded = "funded".to_string();
    assert_eq!(
        outcomes(&receipt),
        vec![
            ("G voter 1".to_string(), by_record.clone()),
            ("plan wallet S".to_string(), by_record.clone()),
            ("plan wallet K".to_string(), by_record),
            ("plan wallet A1".to_string(), funded.clone()),
            ("plan wallet W".to_string(), funded),
        ],
        "the held third send is covered by the record that history found; only the remaining targets are sent"
    );
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), row.target, "{} received exactly its target, once", row.role);
    }
    let k_sends = write_aheads(&fx.pending).iter().filter(|l| l["step"] == "A1.fund.plan-wallet-k").count();
    assert_eq!(k_sends, 1, "the held transfer is never rebuilt");
    assert!(cluster.balance(&fx.payer.pubkey()) < payer_after_partial);
}

#[test]
fn a_rejected_send_stops_the_batch_and_a_closed_rejection_does_not_block_the_retry() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    cluster.reject_at.set(2);
    let message = failed(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("transaction failed") && message.contains("plan wallet S"), "{message}");
    assert_eq!(cluster.sends.get(), 2, "the batch stopped at the rejection");
    assert_eq!(cluster.balance(&fx.recipient("G voter 1")), 20_000_000);
    assert_eq!(cluster.balance(&fx.recipient("plan wallet S")), 0);
    assert_eq!(cluster.balance(&fx.recipient("plan wallet K")), 0, "later targets were never attempted");
    let lines = pending_lines(&fx.pending);
    assert!(lines.iter().any(|l| l["event"] == "resolved" && l["outcome"] == "rejected"), "{lines:?}");

    // The rejected send moved nothing, so the retry sends the rest (and does not repeat the first target).
    cluster.reject_at.set(0);
    let receipt = ok(fx.run(&cluster, true, None, &[]));
    assert_eq!(receipt["all_ok"], true);
    assert_eq!(cluster.sends.get(), 6, "4 more sends");
    assert_eq!(receipt["targets"][0]["outcome"], "already_funded_by_record");
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), row.target);
    }
}

#[test]
fn an_open_record_for_a_target_role_or_recipient_stops_the_command_before_any_send() {
    for by_recipient_only in [false, true] {
        let fx = Fx::new(Net::Local);
        fx.commit();
        let target = fx.recipient("plan wallet K");
        // An earlier run's write-ahead record that was never resolved (its blockhash is long dead).
        let step = if by_recipient_only { "A1.fund.some-renamed-role" } else { "A1.fund.plan-wallet-k" };
        let recipient = if by_recipient_only { target } else { key_of("unrelated") };
        let signature = Signature::new_unique();
        let record = json!({
            "schema": 1, "event": "write_ahead", "step": step, "signature": signature.to_string(),
            "blockhash": Hash::new_unique().to_string(), "genesis_hash": LITESVM_GENESIS,
            "fund": { "recipient": recipient.to_string(), "intent_sha256": "y" },
        });
        fs::write(&fx.pending, format!("{record}\n")).unwrap();
        let mut svm = LiteSVM::new();
        let cluster = fx.cluster_for(&mut svm);

        // A dry run reports the stop (it resolves nothing) ...
        let message = refused(fx.run(&cluster, false, None, &[]));
        assert!(message.contains("OPEN") && message.contains("plan wallet K") && message.contains(&signature.to_string()), "{message}");
        // ... and an --execute run resolves it FIRST: no status and a dead blockhash is HELD, never a send.
        let message = refused(fx.run(&cluster, true, None, &[]));
        assert!(message.contains("HELD UNKNOWN") && message.contains(&signature.to_string()), "{message}");
        assert_eq!(cluster.sends.get(), 0, "by_recipient_only={by_recipient_only}");
        for row in &fx.spec.rows {
            assert_eq!(cluster.balance(&row.pubkey), 0);
        }
    }
}

// ---------------------------------------------------------------------------
// Gates and the plan pin
// ---------------------------------------------------------------------------

#[test]
fn devnet_execute_needs_the_plan_commit_the_pending_file_and_the_packet_pin() {
    let fx = Fx::new(Net::Devnet);
    let commit = fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);

    // No --plan-commit.
    let message = refused(fx.run(&cluster, true, None, &[]));
    assert!(message.contains("--plan-commit"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);

    // No packet pin (the existing gate, unchanged).
    let mut args = fx.args(true, Some(&commit), &[]);
    let position = args.iter().position(|arg| arg == "--packet-sha256").unwrap();
    args.drain(position..position + 4);
    let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
    assert!(message.contains("--packet-sha256"), "{message}");

    // No --pending-file (the existing devnet gate), and none on loopback either.
    let mut args = fx.args(true, Some(&commit), &[]);
    let position = args.iter().position(|arg| arg == "--pending-file").unwrap();
    args.drain(position..position + 2);
    let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
    assert!(message.contains("--pending-file"), "{message}");
    let local = Fx::new(Net::Local);
    local.commit();
    let mut svm = LiteSVM::new();
    let local_cluster = local.cluster_for(&mut svm);
    let mut args = local.args(true, None, &[]);
    let position = args.iter().position(|arg| arg == "--pending-file").unwrap();
    args.drain(position..position + 2);
    let message = refused(ops::run_with_cluster_in(&args, &local_cluster, local.root()));
    assert!(message.contains("needs --pending-file") && message.contains("never be the only protection"), "{message}");
    assert_eq!(local_cluster.sends.get(), 0);
    assert_eq!(cluster.sends.get(), 0);
}

#[test]
fn a_pinned_devnet_run_funds_the_wallets_and_records_path_commit_and_sha256_of_both_files() {
    let fx = Fx::new(Net::Devnet);
    let commit = fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let receipt = ok(fx.run(&cluster, true, Some(&commit), &[]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(cluster.sends.get(), 5);
    for row in &fx.spec.rows {
        assert_eq!(cluster.balance(&row.pubkey), row.target);
    }
    let plan_sha = devnet::sha256_hex(&fs::read(fx.plan_file()).unwrap());
    let inventory_sha = devnet::sha256_hex(&fs::read(fx.inventory_file()).unwrap());
    let blob_sha = devnet::sha256_hex(&git_bytes(fx.root(), &["cat-file", "blob", &format!("{commit}:{PLAN_PATH}")]));
    assert_eq!(plan_sha, blob_sha, "the recorded hash is the committed blob's");
    let plan = json!({ "path": PLAN_PATH, "commit": commit, "sha256": plan_sha });
    let inventory = json!({ "path": INVENTORY_PATH, "commit": commit, "sha256": inventory_sha });
    assert_eq!(receipt["plan"], plan);
    assert_eq!(receipt["inventory"], inventory);
    assert_eq!(receipt["packet"]["packet_sha256"], fx.packet_sha.as_str(), "the packet pin is in the receipt too");
    for write_ahead in write_aheads(&fx.pending) {
        assert_eq!(write_ahead["fund"]["plan"], plan);
        assert_eq!(write_ahead["fund"]["inventory"], inventory);
        assert_eq!(write_ahead["packet"]["packet_sha256"], fx.packet_sha.as_str());
        assert_eq!(write_ahead["genesis_hash"], ops::DEVNET_GENESIS_HASH);
        assert_eq!(
            write_ahead["fund"]["intent_sha256"],
            intent_of(
                write_ahead["fund"]["role"].as_str().unwrap(),
                &write_ahead["fund"]["recipient"].as_str().unwrap().parse().unwrap(),
                write_ahead["fund"]["target_lamports"].as_u64().unwrap(),
                &fx.payer.pubkey(),
                ops::DEVNET_GENESIS_HASH
            )
            .as_str()
        );
    }
    // The same run again, pinned: nothing more is sent.
    let again = ok(fx.run(&cluster, true, Some(&commit), &[]));
    assert!(outcomes(&again).iter().all(|(_, outcome)| outcome == "already_funded_by_record"));
    assert_eq!(cluster.sends.get(), 5);
}

#[test]
fn a_plan_or_inventory_that_is_not_its_committed_blob_is_refused_with_zero_sends() {
    let fx = Fx::new(Net::Devnet);
    let commit = fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let good_plan = fs::read_to_string(fx.plan_file()).unwrap();
    let good_inventory = fs::read_to_string(fx.inventory_file()).unwrap();

    // (a) The working plan was edited after the commit (a bigger target, still a valid plan).
    fs::write(fx.plan_file(), good_plan.replace("20000000", "20000001")).unwrap();
    let message = refused(fx.run(&cluster, true, Some(&commit), &[]));
    assert!(message.contains("working plan") && message.contains("differs from the blob committed at"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);
    fs::write(fx.plan_file(), &good_plan).unwrap();

    // (b) The working inventory was edited after the commit (an alias slipped in).
    let alias = key_of("alias").to_string();
    let edited = good_inventory.replace(&fx.recipient("plan wallet K").to_string(), &alias);
    assert_ne!(edited, good_inventory);
    fs::write(fx.inventory_file(), edited).unwrap();
    let message = refused(fx.run(&cluster, true, Some(&commit), &[]));
    assert!(message.contains("working inventory") && message.contains("differs from the blob committed at"), "{message}");
    assert_nothing_sent(&cluster, 0, &fx);
    fs::write(fx.inventory_file(), &good_inventory).unwrap();

    // (c) A commit that is not the full lowercase hex id, not a commit, or not holding the files.
    for bad in ["abc", &commit.to_uppercase(), &commit[..39], &format!("{}x", &commit[..39])] {
        let message = refused(fx.run(&cluster, true, Some(bad), &[]));
        assert!(message.contains("40-digit lowercase hex"), "`{bad}`: {message}");
    }
    let tree = git(fx.root(), &["rev-parse", "HEAD^{tree}"]);
    let message = refused(fx.run(&cluster, true, Some(&tree), &[]));
    assert!(message.contains("not a commit"), "{message}");
    let empty = {
        let other = TempDir::new("elsewhere");
        git(other.root(), &["init", "-q"]);
        fs::write(other.root().join("x"), "x").unwrap();
        git(other.root(), &["add", "x"]);
        git(other.root(), &["commit", "-q", "-m", "x"]);
        git(other.root(), &["rev-parse", "HEAD"])
    };
    let message = refused(fx.run(&cluster, true, Some(&empty), &[]));
    assert!(message.contains("--plan-commit") || message.contains("git cat-file"), "{message}");

    // (d) The files are fine at HEAD but the pin names an OLDER commit that held a different plan.
    let mut older = fx.spec.clone();
    older.rows[0].target = 21_000_000;
    let mut changed = Fx::new(Net::Devnet);
    changed.spec = older;
    let old_commit = changed.commit();
    changed.spec.rows[0].target = 22_000_000;
    let new_commit = changed.commit();
    let mut svm2 = LiteSVM::new();
    let cluster2 = changed.cluster_for(&mut svm2);
    let message = refused(changed.run(&cluster2, true, Some(&old_commit), &[]));
    assert!(message.contains("differs from the blob committed at"), "{message}");
    assert_eq!(cluster2.sends.get(), 0);
    ok(changed.run(&cluster2, true, Some(&new_commit), &[]));
    assert_eq!(cluster2.sends.get(), 5);

    // (e) Paths: outside the repository, hidden or parent components.
    for bad in ["../outside.json", ".hidden/plan.json", "docs//plan.json", "docs/solana/", "/etc/hosts"] {
        let mut args = fx.args(true, Some(&commit), &[]);
        let position = args.iter().position(|arg| arg == "--plan").unwrap();
        args[position + 1] = bad.into();
        let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
        assert!(
            message.contains("plain repository-relative path") || message.contains("outside the repository"),
            "`{bad}`: {message}"
        );
    }
    assert_eq!(cluster.sends.get(), 0);

    // (f) An uncommitted plan file: no blob at the commit.
    fs::write(fx.root().join("docs/solana/uncommitted-plan.json"), &good_plan).unwrap();
    let mut args = fx.args(true, Some(&commit), &[]);
    let position = args.iter().position(|arg| arg == "--plan").unwrap();
    args[position + 1] = "docs/solana/uncommitted-plan.json".into();
    let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
    assert!(message.contains("git cat-file"), "{message}");
    assert_eq!(cluster.sends.get(), 0);

    // The good files with the good commit still run.
    ok(fx.run(&cluster, true, Some(&commit), &[]));
    assert_eq!(cluster.sends.get(), 5);
}

#[test]
fn the_inventory_path_comes_from_the_pinned_plan_and_must_be_a_plain_repository_path() {
    let fx = Fx::new(Net::Devnet);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    for bad in ["../../.mc-devnet-keys/inventory.json", "/abs/inventory.json", "docs/../../x.json", ".config/solana/id.json"] {
        let commit = fx.commit_with(|plan, _| plan["inventory"] = json!(bad));
        let message = refused(fx.run(&cluster, true, Some(&commit), &[]));
        assert!(message.contains("plan.inventory") && message.contains("plain repository-relative path"), "`{bad}`: {message}");
    }
    assert_eq!(cluster.sends.get(), 0);
}

#[test]
fn a_pinned_file_reached_through_a_symlink_is_refused() {
    let fx = Fx::new(Net::Devnet);
    let commit = fx.commit();
    // The committed path is replaced by a symlink to a byte-identical copy elsewhere in the repository.
    let copy = fx.root().join("docs/solana/copy.json");
    fs::copy(fx.plan_file(), &copy).unwrap();
    fs::remove_file(fx.plan_file()).unwrap();
    std::os::unix::fs::symlink(&copy, fx.plan_file()).unwrap();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let message = refused(fx.run(&cluster, true, Some(&commit), &[]));
    assert!(message.contains("symlink") && message.contains("committed path itself"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
}

#[test]
fn the_default_wallet_mainnet_and_stray_flags_are_refused() {
    let fx = Fx::new(Net::Local);
    fx.commit();
    let mut svm = LiteSVM::new();
    let cluster = fx.cluster_for(&mut svm);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/nobody".into());
    let default_wallet = format!("{home}/.config/solana/id.json");

    // No --payer (no default wallet), the default wallet as --payer, as --plan, and as the write-ahead file.
    let mut args = fx.args(false, None, &[]);
    let position = args.iter().position(|arg| arg == "--payer").unwrap();
    args.drain(position..position + 2);
    assert!(matches!(ops::run_with_cluster_in(&args, &cluster, fx.root()), Err(OpsError::Usage(_))));
    for flag in ["--payer", "--plan", "--pending-file"] {
        let mut args = fx.args(false, None, &[]);
        let position = args.iter().position(|arg| arg == flag).unwrap();
        args[position + 1] = default_wallet.clone();
        let message = refused(ops::run_with_cluster_in(&args, &cluster, fx.root()));
        assert!(message.contains("default wallet"), "{flag}: {message}");
    }
    // A bare pubkey cannot sign an --execute run (usage), and mainnet is never a target.
    let mut args = fx.args(true, None, &[]);
    let position = args.iter().position(|arg| arg == "--payer").unwrap();
    args[position + 1] = fx.payer.pubkey().to_string();
    assert!(matches!(ops::run_with_cluster_in(&args, &cluster, fx.root()), Err(OpsError::Usage(_))));
    let mut args = fx.args(false, None, &[]);
    args.extend(["--cluster".to_string(), "mainnet-beta".to_string()]);
    assert!(matches!(ops::run_with_cluster_in(&args, &cluster, fx.root()), Err(OpsError::Usage(_)) | Err(OpsError::Refused(_))));
    // A loopback RPC that reports mainnet's genesis is a tunnel to a public cluster.
    let mut mainnet_svm = LiteSVM::new();
    let mainnet = Fund::new(&mut mainnet_svm, ops::MAINNET_GENESIS_HASH);
    let message = refused(fx.run(&mainnet, false, None, &[]));
    assert!(message.contains("MAINNET"), "{message}");
    // --plan / --plan-commit belong to this command only.
    let args: Vec<String> = ["fund-vault", "--rpc", RPC, "--plan", PLAN_PATH].iter().map(|s| s.to_string()).collect();
    assert!(matches!(ops::run_with_cluster_in(&args, &cluster, fx.root()), Err(OpsError::Usage(m)) if m.contains("fund-wallets")));
    assert_eq!(cluster.sends.get(), 0);
    assert_eq!(mainnet.sends.get(), 0);
}

#[test]
fn help_lists_the_command_and_the_role_allowlist_has_the_packets_seventeen_wallets() {
    assert!(ops::help_text().contains("fund-wallets"));
    assert_eq!(ops::FUND_ROLES.len(), 17);
    let names: BTreeSet<&str> = ops::FUND_ROLES.iter().map(|(role, _)| *role).collect();
    assert_eq!(names.len(), 17, "no duplicate role");
    let slugs: BTreeSet<String> = ops::FUND_ROLES.iter().map(|(role, _)| step_of(role)).collect();
    assert_eq!(slugs.len(), 17, "no two roles share a step id");
    assert_eq!(ops::FUND_PER_TRANSFER_CAP_LAMPORTS, 2 * SOL);
    assert_eq!(
        ops::FUND_BUCKET_CAPS.iter().map(|(_, cap)| *cap).sum::<u64>(),
        8_600_000_000,
        "0.5 + 1.0 + 0.1 + 7.0 SOL"
    );
}

// ---------------------------------------------------------------------------
// The REAL committed plan against the REAL committed inventory (read only, dry run)
// ---------------------------------------------------------------------------

#[test]
fn the_committed_funding_plan_validates_against_the_committed_inventory_in_a_dry_run() {
    let repo = ops::repo_root();
    let plan: Value = serde_json::from_slice(&fs::read(repo.join("docs/solana/devnet-funding-plan.json")).expect("the committed plan")).unwrap();
    let payer: Pubkey = plan["payer"]["pubkey"].as_str().unwrap().parse().unwrap();
    let mut svm = LiteSVM::new();
    let cluster = Fund::new(&mut svm, ops::DEVNET_GENESIS_HASH);
    cluster.set_lamports(&payer, 7_879_683_960); // the deployer's post-deploy remainder
    let args: Vec<String> = [
        "fund-wallets", "--payer", &payer.to_string(), "--plan", "docs/solana/devnet-funding-plan.json", "--rpc", DEVNET_URL, "--cluster", "devnet",
    ]
    .iter()
    .map(|part| part.to_string())
    .collect();
    let receipt = ok(ops::run_with_cluster_in(&args, &cluster, &repo));
    assert_eq!(receipt["mode"], "dry-run");
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["totals"]["targets"], plan["targets"].as_array().unwrap().len());
    assert_eq!(receipt["totals"]["would_fund"], plan["targets"].as_array().unwrap().len());
    assert!(outcomes(&receipt).iter().all(|(_, outcome)| outcome == "would_fund"));
    // The tool's arithmetic reproduces the plan's own bucket totals.
    for bucket in receipt["envelope"]["buckets"].as_array().unwrap() {
        let name = bucket["bucket"].as_str().unwrap();
        assert_eq!(
            bucket["total_lamports"], plan["buckets"][name]["planned_total_with_fees_lamports"],
            "bucket {name}"
        );
    }
    assert_eq!(receipt["envelope"]["command_total_lamports"].as_u64().unwrap(), 3_830_000_000 + 17 * FEE);
    assert_eq!(cluster.sends.get(), 0);
    assert!(receipt["plan"]["sha256"].is_string() && receipt["inventory"]["sha256"].is_string());
}
