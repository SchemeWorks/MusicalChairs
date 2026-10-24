//! M14-TOOL: the devnet operator tool (`src/devnet_ops.rs`, binary `devnet_ops`) against LOCAL
//! clusters only.
//!
//! * LiteSVM tests drive the tool's real code paths (`run_with_cluster`) through a `Cluster` double
//!   that wraps the harness' real-Squads world (the actual Squads v4 devnet binary, the devnet ORAO
//!   binary, the program ELF under a loader-shaped ProgramData whose authority is the G vault).  The
//!   double counts simulations and sends and records their order.
//! * The safety interlocks (loopback / devnet + packet identity pin / genesis hash, packet size, default
//!   wallet refusal, pins that must not drift) are tested without any cluster.
//! * ONE `#[ignore]` test runs the real loader-v3 deploy, SetAuthority, verify-bytes and the Squads
//!   proposal lifecycle against a local `solana-test-validator` (`MC_LOCAL_RPC`, loopback only);
//!   the exact validator command is in `docs/solana/devnet-ops.md`.
//!
//! Nothing here touches a public network and no existing key file is read: every keypair is
//! generated in memory or derived from a fixed test seed and written into a temp dir the tests own.
//!
//! ```text
//! cargo +1.92.0 test --locked --test devnet_ops_local -- --test-threads=1 --nocapture
//! ```

use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use litesvm::LiteSVM;
use musical_chairs_financial_runtime::{
    devnet::{self, OraoProfile},
    devnet_ops::{
        self as ops, Cluster, Gate, KeyFile, Opts, OpsError, ProposalStatus, RpcCluster, SendError,
        SendOutcome, SigOutcome, SimOutcome, Step, Target,
    },
    runtime::{real_g_create_keypair, real_p_create_keypair},
    squads_real::{self as sq, RealSquadsWorld},
};
use serde_json::Value;
use solana_sdk::{
    account::Account,
    clock::Clock,
    commitment_config::CommitmentConfig,
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

const RPC: &str = "http://127.0.0.1:8899";
const DEVNET_URL: &str = "https://api.devnet.solana.com";
const LITESVM_GENESIS: &str = "LiteSVMLocalGenesis11111111111111111111111111";

// ---------------------------------------------------------------------------
// Test plumbing
// ---------------------------------------------------------------------------

static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A temp dir the test owns (removed on drop).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "mc-devnet-ops-{tag}-{}-{}",
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
    fs::write(path, serde_json::to_string(&keypair.to_bytes().to_vec()).expect("json"))
        .expect("write keypair");
}

fn write_pubkey(path: &str, key: &Pubkey) {
    fs::write(path, key.to_string()).expect("write pubkey");
}

fn write_members(path: &str, members: &[Pubkey]) {
    let text: String = members.iter().map(|key| format!("{key}\n")).collect();
    fs::write(path, text).expect("write members");
}

/// A throwaway git repository holding a "packet" at the tool's fixed packet path.
struct PacketRepo {
    dir: TempDir,
    commit: String,
    sha: String,
}

fn git(repo: &Path, args: &[&str]) -> String {
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
        git(Path::new(&dir.dir()), &["init", "-q"]);
        let mut repo = PacketRepo { dir, commit: String::new(), sha: String::new() };
        repo.commit_packet(content);
        repo
    }
    fn root(&self) -> PathBuf {
        PathBuf::from(self.dir.dir())
    }
    fn packet_file(&self) -> PathBuf {
        self.root().join(ops::DEFAULT_PACKET_FILE)
    }
    /// Overwrite the working file WITHOUT committing.
    fn edit_working_file(&self, content: &str) {
        fs::create_dir_all(self.packet_file().parent().unwrap()).unwrap();
        fs::write(self.packet_file(), content).unwrap();
    }
    /// Write, commit and pin the new text; returns the previous `(commit, sha)`.
    fn commit_packet(&mut self, content: &str) -> (String, String) {
        self.edit_working_file(content);
        git(&self.root(), &["add", "-A"]);
        git(&self.root(), &["commit", "-q", "-m", "packet"]);
        let previous = (std::mem::take(&mut self.commit), std::mem::take(&mut self.sha));
        self.commit = git(&self.root(), &["rev-parse", "HEAD"]);
        self.sha = devnet::sha256_hex(content.as_bytes());
        previous
    }
    /// The pin flags of a devnet `--execute`.
    fn pin_flags(&self) -> Vec<String> {
        vec!["--packet-sha256".into(), self.sha.clone(), "--packet-commit".into(), self.commit.clone()]
    }
}

/// A [`Cluster`] over a LiteSVM instance.  Counts simulations and sends and records their order, so
/// the tests can assert "no send without a preceding simulation" and "dry runs send nothing".
struct SvmCluster<'a> {
    svm: RefCell<&'a mut LiteSVM>,
    genesis: RefCell<String>,
    sims: Cell<usize>,
    sends: Cell<usize>,
    genesis_reads: Cell<usize>,
    fail_simulation: Cell<bool>,
    order: RefCell<Vec<&'static str>>,
    landed: RefCell<std::collections::BTreeMap<Signature, u64>>,
    /// `blockhash_valid` answer: an unresolved write-ahead record is only resolvable once this is false.
    blockhash_still_valid: Cell<bool>,
    /// The node never reports a landed transaction's slot (`signature_slot` finds nothing).
    hide_slot: Cell<bool>,
    /// `slot()` fails (so neither the status nor the fallback can supply a slot).
    fail_slot: Cell<bool>,
    /// The highest slot the tool handed to `note_confirmed_slot` (what `minContextSlot` would carry).
    noted_slot: Cell<u64>,
}

impl<'a> SvmCluster<'a> {
    fn new(svm: &'a mut LiteSVM) -> Self {
        SvmCluster {
            svm: RefCell::new(svm),
            genesis: RefCell::new(LITESVM_GENESIS.to_string()),
            sims: Cell::new(0),
            sends: Cell::new(0),
            genesis_reads: Cell::new(0),
            fail_simulation: Cell::new(false),
            order: RefCell::new(Vec::new()),
            landed: RefCell::new(std::collections::BTreeMap::new()),
            blockhash_still_valid: Cell::new(false),
            hide_slot: Cell::new(false),
            fail_slot: Cell::new(false),
            noted_slot: Cell::new(0),
        }
    }

    /// Overwrite an account's balance (a node that has not seen a transfer yet reports the old one).
    fn set_lamports(&self, key: &Pubkey, lamports: u64) {
        let mut svm = self.svm.borrow_mut();
        let mut account = svm.get_account(key).unwrap_or_default();
        account.lamports = lamports;
        svm.set_account(*key, account).expect("set account");
    }

    fn set_genesis(&self, genesis: &str) {
        *self.genesis.borrow_mut() = genesis.to_string();
    }

    /// Move the cluster clock forward (and one slot), like `squads_real::advance`.
    fn advance(&self, seconds: i64) {
        let mut svm = self.svm.borrow_mut();
        let clock: Clock = svm.get_sysvar();
        let now = clock.unix_timestamp + seconds;
        svm.set_sysvar(&Clock {
            slot: clock.slot + 1,
            epoch_start_timestamp: now,
            epoch: clock.epoch,
            leader_schedule_epoch: clock.leader_schedule_epoch,
            unix_timestamp: now,
        });
    }

    fn now(&self) -> i64 {
        self.svm.borrow().get_sysvar::<Clock>().unix_timestamp
    }

    /// Like `getAccountInfo`: an account with zero lamports does not exist.
    fn account_of(&self, key: &Pubkey) -> Option<Account> {
        self.svm.borrow().get_account(key).filter(|account| account.lamports > 0)
    }
}

impl Cluster for SvmCluster<'_> {
    fn genesis_hash(&self) -> Result<String, String> {
        self.genesis_reads.set(self.genesis_reads.get() + 1);
        Ok(self.genesis.borrow().clone())
    }
    fn blockhash(&self) -> Result<Hash, String> {
        let mut svm = self.svm.borrow_mut();
        svm.expire_blockhash();
        Ok(svm.latest_blockhash())
    }
    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String> {
        Ok(self.account_of(key))
    }
    fn rent_minimum(&self, data_len: usize) -> Result<u64, String> {
        Ok(self.svm.borrow().minimum_balance_for_rent_exemption(data_len))
    }
    fn unix_time(&self) -> Result<i64, String> {
        Ok(self.now())
    }
    fn slot(&self) -> Result<u64, String> {
        if self.fail_slot.get() {
            return Err("injected getSlot failure".into());
        }
        Ok(self.svm.borrow().get_sysvar::<Clock>().slot)
    }
    fn blockhash_valid(&self, _hash: &Hash) -> Result<bool, String> {
        Ok(self.blockhash_still_valid.get())
    }
    fn note_confirmed_slot(&self, slot: u64) {
        self.noted_slot.set(self.noted_slot.get().max(slot));
    }
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String> {
        if !sig_verify {
            return Err("the LiteSVM double cannot simulate unsigned transactions".into());
        }
        self.sims.set(self.sims.get() + 1);
        self.order.borrow_mut().push("simulate");
        if self.fail_simulation.get() {
            return Ok(SimOutcome {
                error: Some("injected simulation failure".into()),
                logs: vec!["injected".into()],
                units_consumed: None,
            });
        }
        match self.svm.borrow().simulate_transaction(tx.clone()) {
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
        self.order.borrow_mut().push("send");
        let slot = self.svm.borrow().get_sysvar::<Clock>().slot;
        match self.svm.borrow_mut().send_transaction(tx.clone()) {
            Ok(meta) => {
                self.landed.borrow_mut().insert(meta.signature, slot);
                Ok(SendOutcome {
                    signature: meta.signature,
                    units_consumed: Some(meta.compute_units_consumed),
                })
            }
            Err(failed) => Err(SendError::Rejected(format!("{:?}\n{}", failed.err, failed.meta.pretty_logs()))),
        }
    }
    fn signature_outcome(&self, signature: &Signature) -> Result<SigOutcome, String> {
        Ok(self
            .landed
            .borrow()
            .get(signature)
            .map_or(SigOutcome::Unknown, |slot| SigOutcome::Confirmed { slot: *slot }))
    }
    fn signature_slot(&self, signature: &Signature) -> Result<Option<u64>, String> {
        if self.hide_slot.get() {
            return Ok(None);
        }
        Ok(self.landed.borrow().get(signature).copied())
    }
}

/// Key files for one real-Squads world, written into a temp dir.
struct Files {
    dir: TempDir,
    payer: String,
    g_create: String,
    p_create: String,
    g_members: String,
    p_members: String,
    g_member: Vec<String>,
    sponsor: String,
    sponsor_pub: String,
    management_pub: String,
    other_management_pub: String,
}

impl Files {
    fn write(world: &RealSquadsWorld, payer: &Keypair) -> Files {
        let dir = TempDir::new("keys");
        let files = Files {
            payer: dir.file("payer.json"),
            g_create: dir.file("g-create.json"),
            p_create: dir.file("p-create.json"),
            g_members: dir.file("g-members.txt"),
            p_members: dir.file("p-members.txt"),
            g_member: (0..3).map(|i| dir.file(&format!("g-member-{i}.json"))).collect(),
            sponsor: dir.file("sponsor.json"),
            sponsor_pub: dir.file("sponsor.pub"),
            management_pub: dir.file("management.pub"),
            other_management_pub: dir.file("other-management.pub"),
            dir,
        };
        write_keypair(&files.payer, payer);
        write_keypair(&files.g_create, &real_g_create_keypair());
        write_keypair(&files.p_create, &real_p_create_keypair());
        for (path, member) in files.g_member.iter().zip(&world.g_members) {
            write_keypair(path, member);
        }
        let g_keys: Vec<Pubkey> = world.g_members.iter().map(Signer::pubkey).collect();
        let p_keys: Vec<Pubkey> = world.p_members.iter().map(Signer::pubkey).collect();
        write_members(&files.g_members, &g_keys);
        write_members(&files.p_members, &p_keys);
        write_keypair(&files.sponsor, &world.rt.sponsor);
        write_pubkey(&files.sponsor_pub, &world.rt.sponsor.pubkey());
        write_pubkey(&files.management_pub, &world.rt.management.pubkey());
        write_pubkey(&files.other_management_pub, &world.rt.attacker.pubkey());
        files
    }

    /// `parts` = command, positional action, flags; the cluster flags are appended.
    fn args(&self, parts: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
        for (flag, value) in [
            ("--rpc", RPC),
            ("--g-create-key", self.g_create.as_str()),
            ("--p-create-key", self.p_create.as_str()),
            ("--g-members", self.g_members.as_str()),
            ("--p-members", self.p_members.as_str()),
        ] {
            args.push(flag.into());
            args.push(value.into());
        }
        args
    }

    fn run(&self, cluster: &dyn Cluster, parts: &[&str]) -> Result<Value, OpsError> {
        ops::run_with_cluster(&self.args(parts), cluster)
    }
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

fn failing_checks(receipt: &Value) -> Vec<String> {
    receipt["checks"]
        .as_array()
        .map(|checks| {
            checks
                .iter()
                .filter(|entry| entry["ok"] != Value::Bool(true))
                .map(|entry| entry["name"].as_str().unwrap_or("?").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The harness world every flow test starts from: real Squads, devnet ORAO (network set up with the
/// real devnet treasury address), the program ELF under a ProgramData whose authority is the G vault.
fn world_with_orao() -> RealSquadsWorld {
    let mut world = RealSquadsWorld::with_provider(OraoProfile::Devnet);
    let treasury = sq::install_devnet_orao_treasury(&mut world.rt);
    for setup in sq::orao_initialize(&mut world.rt, treasury) {
        assert!(setup.success, "ORAO network set-up: {setup:?}");
    }
    world
}

fn funded_payer(world: &mut RealSquadsWorld) -> Keypair {
    let payer = Keypair::new_from_array([0x91; 32]);
    world.rt.credit_lamports(payer.pubkey(), 10_000_000_000);
    payer
}

const TWO_DAYS: i64 = 172_800;

// ---------------------------------------------------------------------------
// Pins that must not drift
// ---------------------------------------------------------------------------

#[test]
fn pins_match_the_recorded_build_the_frontend_and_the_packet() {
    let latest: Value = serde_json::from_slice(
        &fs::read(ops::repo_root().join("docs/solana/evidence/latest-build.json"))
            .expect("latest-build.json"),
    )
    .expect("json");
    assert_eq!(
        latest["artifact"]["sha256"].as_str(),
        Some(ops::PINNED_PROGRAM_ELF_SHA256),
        "the tool's ELF pin must equal docs/solana/evidence/latest-build.json"
    );
    let cluster_ts = fs::read_to_string(ops::repo_root().join("frontend/src/solana/native/cluster.ts"))
        .expect("cluster.ts");
    for (name, hash) in [
        ("devnet", ops::DEVNET_GENESIS_HASH),
        ("mainnet", ops::MAINNET_GENESIS_HASH),
        ("testnet", ops::TESTNET_GENESIS_HASH),
    ] {
        assert!(cluster_ts.contains(hash), "{name} genesis hash {hash} must appear in cluster.ts");
    }
    let packet = fs::read_to_string(ops::repo_root().join(ops::DEFAULT_PACKET_FILE)).expect("packet");
    let domain_hex: String = ops::DEVNET_CLUSTER_DOMAIN.iter().map(|b| format!("{b:02x}")).collect();
    assert!(
        packet.contains(&domain_hex),
        "the packet's devnet cluster domain hex {domain_hex} not found in the packet"
    );
    assert_eq!(&ops::DEVNET_CLUSTER_DOMAIN, b"musical-chairs-devnet-domain-v1!");
    assert_eq!(ops::PACKET_LIMIT, 1_232);
}

// ---------------------------------------------------------------------------
// The gate: URL, cluster name, packet identity pin, genesis hash
// ---------------------------------------------------------------------------

fn opts(parts: &[&str]) -> Opts {
    let args: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
    Opts::parse(&args).expect("opts parse")
}

#[test]
fn rpc_url_parsing_accepts_exactly_the_loopback_hosts() {
    for url in [
        "http://127.0.0.1:8899",
        "http://localhost:8899",
        "http://[::1]:8899",
        "https://localhost",
        "HTTP://LOCALHOST:8899/",
    ] {
        let (_, host, _) = ops::parse_rpc_url(url).unwrap_or_else(|e| panic!("{url}: {e}"));
        assert!(ops::is_loopback_host(&host), "{url} is loopback");
    }
    for url in [
        "http://127.0.0.1.evil.example:8899",
        "http://localhost.evil.example",
        "http://127.0.0.2:8899",
        "http://0.0.0.0:8899",
        "https://api.devnet.solana.com",
    ] {
        let (_, host, _) = ops::parse_rpc_url(url).unwrap_or_else(|e| panic!("{url}: {e}"));
        assert!(!ops::is_loopback_host(&host), "{url} is not loopback");
    }
    for url in [
        "http://127.0.0.1@evil.example:8899/",
        "http://user:pw@localhost:8899",
        "http://localhost\\@evil.example",
        "http://",
        "127.0.0.1:8899",
        "ftp://localhost",
        "http://localhost:99999",
        "http://[::1",
    ] {
        assert!(ops::parse_rpc_url(url).is_err(), "{url} must be rejected");
    }
}

#[test]
fn local_cluster_refuses_a_non_loopback_url_and_unknown_clusters_are_refused() {
    let local_public = opts(&["fund-vault", "--rpc", DEVNET_URL]);
    assert!(matches!(Gate::from_opts(&local_public), Err(OpsError::Refused(_))));

    let mainnet = opts(&["fund-vault", "--rpc", RPC, "--cluster", "mainnet-beta"]);
    let message = match Gate::from_opts(&mainnet) {
        Err(OpsError::Refused(message)) => message,
        other => panic!("{other:?}"),
    };
    assert!(message.contains("mainnet"), "{message}");
    for name in ["mainnet", "testnet", "devnet2", ""] {
        let o = opts(&["fund-vault", "--rpc", RPC, &format!("--cluster={name}")]);
        assert!(matches!(Gate::from_opts(&o), Err(OpsError::Refused(_))), "cluster `{name}`");
    }

    let devnet_http = opts(&["fund-vault", "--rpc", "http://api.devnet.solana.com", "--cluster", "devnet"]);
    assert!(matches!(Gate::from_opts(&devnet_http), Err(OpsError::Refused(_))), "devnet needs https");
    let devnet = Gate::from_opts(&opts(&["fund-vault", "--rpc", DEVNET_URL, "--cluster", "devnet"])).expect("devnet");
    assert_eq!(devnet.target, Target::Devnet);
    let local = Gate::from_opts(&opts(&["fund-vault", "--rpc", RPC])).expect("local");
    assert_eq!(local.target, Target::Local);
    assert!(!local.execute, "dry run is the default");
}

#[test]
fn genesis_hash_gate_pins_devnet_and_refuses_public_clusters_on_loopback() {
    let devnet = Gate::from_opts(&opts(&["fund-vault", "--rpc", DEVNET_URL, "--cluster", "devnet"])).unwrap();
    assert!(devnet.check_genesis(ops::DEVNET_GENESIS_HASH).is_ok());
    assert!(matches!(devnet.check_genesis(ops::MAINNET_GENESIS_HASH), Err(OpsError::Refused(_))));
    assert!(matches!(devnet.check_genesis(ops::TESTNET_GENESIS_HASH), Err(OpsError::Refused(_))));
    assert!(matches!(devnet.check_genesis(LITESVM_GENESIS), Err(OpsError::Refused(_))));

    let local = Gate::from_opts(&opts(&["fund-vault", "--rpc", RPC])).unwrap();
    assert!(local.check_genesis(LITESVM_GENESIS).is_ok(), "any private validator genesis is fine locally");
    for public in [ops::MAINNET_GENESIS_HASH, ops::TESTNET_GENESIS_HASH, ops::DEVNET_GENESIS_HASH] {
        assert!(
            matches!(local.check_genesis(public), Err(OpsError::Refused(_))),
            "a loopback RPC reporting {public} is a tunnel to a public cluster"
        );
    }
}

#[test]
fn the_packet_pin_hashes_the_committed_blob_not_the_working_file() {
    let mut repo = PacketRepo::new("reviewed packet text v1");
    let pin = ops::verify_packet_pin(&repo.root(), &repo.commit, &repo.sha).expect("the pin verifies");
    assert_eq!(
        (pin.packet_path.as_str(), pin.packet_commit.as_str(), pin.packet_sha256.as_str()),
        (ops::DEFAULT_PACKET_FILE, repo.commit.as_str(), repo.sha.as_str())
    );
    assert!(ops::verify_packet_pin(&repo.root(), &repo.commit, &repo.sha.to_uppercase()).is_ok(), "hex case is irrelevant");

    // The wrong hash, malformed or unknown commits.
    let wrong = "0".repeat(64);
    assert!(matches!(ops::verify_packet_pin(&repo.root(), &repo.commit, &wrong), Err(OpsError::Refused(m)) if m.contains("is not the SHA-256")));
    for bad in ["abc", &repo.commit.to_uppercase(), &"0".repeat(40), &format!("{}x", &repo.commit[..39])] {
        assert!(matches!(ops::verify_packet_pin(&repo.root(), bad, &repo.sha), Err(OpsError::Refused(_))), "commit `{bad}`");
    }
    // A tree id is not a commit.
    let tree = git(&repo.root(), &["rev-parse", "HEAD^{tree}"]);
    assert!(matches!(ops::verify_packet_pin(&repo.root(), &tree, &repo.sha), Err(OpsError::Refused(m)) if m.contains("not a commit")));

    // An uncommitted edit of the working file: the blob still hashes, the working file no longer equals it.
    repo.edit_working_file("reviewed packet text v1, EDITED after the commit");
    assert!(matches!(ops::verify_packet_pin(&repo.root(), &repo.commit, &repo.sha), Err(OpsError::Refused(m)) if m.contains("differs from the blob")));

    // A newer commit freezes the new text; the old pin no longer matches the working file.
    let (old_commit, old_sha) = repo.commit_packet("reviewed packet text v2");
    assert!(ops::verify_packet_pin(&repo.root(), &repo.commit, &repo.sha).is_ok());
    assert!(matches!(ops::verify_packet_pin(&repo.root(), &old_commit, &old_sha), Err(OpsError::Refused(m)) if m.contains("differs from the blob")));
    assert!(matches!(ops::verify_packet_pin(&repo.root(), &repo.commit, &old_sha), Err(OpsError::Refused(_))));
}

#[test]
fn execute_token_demands_the_packet_pin_on_devnet_and_nothing_on_a_loopback_validator() {
    let repo = PacketRepo::new("the reviewed M14 packet");
    let base = ["fund-vault", "--rpc", DEVNET_URL, "--cluster", "devnet", "--execute"];
    let token = |extra: &[&str]| {
        let mut parts: Vec<&str> = base.to_vec();
        parts.extend_from_slice(extra);
        let o = opts(&parts);
        Gate::from_opts(&o).and_then(|gate| gate.execute_token_in(&o, &repo.root()))
    };
    let (sha, commit) = (repo.sha.as_str(), repo.commit.as_str());

    // Both halves of the pin are required, and the message says what the pin is NOT.
    assert!(matches!(token(&[]), Err(OpsError::Refused(m)) if m.contains("--packet-sha256") && m.contains("NOT an approval")));
    assert!(matches!(token(&["--packet-sha256", sha]), Err(OpsError::Refused(m)) if m.contains("--packet-commit")));
    assert!(matches!(token(&["--packet-commit", commit]), Err(OpsError::Refused(m)) if m.contains("--packet-sha256")));
    assert!(matches!(token(&["--packet-sha256", sha, "--packet-commit", commit]), Ok(Some(_))));
    assert!(matches!(token(&["--packet-sha256", "deadbeef", "--packet-commit", commit]), Err(OpsError::Refused(_))));

    // The old flag name is rejected with an explanation; a --packet path override is refused.
    assert!(matches!(
        token(&["--i-have-approval", sha, "--packet-sha256", sha, "--packet-commit", commit]),
        Err(OpsError::Usage(m)) if m.contains("not an approval") && m.contains("--packet-sha256")
    ));
    let other = repo.dir.file("other-packet.md");
    fs::write(&other, "the reviewed M14 packet").unwrap();
    assert!(matches!(
        token(&["--packet", &other, "--packet-sha256", sha, "--packet-commit", commit]),
        Err(OpsError::Refused(m)) if m.contains("--packet is refused")
    ));

    // The RPC host allowlist: the public devnet endpoint, a loopback tunnel, or one host named explicitly.
    let hosted = |url: &str, extra: &[&str]| {
        let mut parts = vec!["fund-vault", "--rpc", url, "--cluster", "devnet", "--execute", "--packet-sha256", sha, "--packet-commit", commit];
        parts.extend_from_slice(extra);
        let o = opts(&parts);
        Gate::from_opts(&o).and_then(|gate| gate.execute_token_in(&o, &repo.root()))
    };
    assert!(matches!(hosted("https://rpc.example.com", &[]), Err(OpsError::Refused(m)) if m.contains("allowlist")));
    assert!(matches!(hosted("https://rpc.example.com", &["--allow-rpc-host", "rpc.example.com"]), Ok(Some(_))));
    assert!(matches!(hosted("https://rpc.example.com", &["--allow-rpc-host", "other.example.com"]), Err(OpsError::Refused(_))));
    assert!(matches!(hosted("http://127.0.0.1:8899", &[]), Ok(Some(_))), "a loopback tunnel is the genesis check's business");

    // Loopback: --execute needs no pin (and refuses one); without --execute there is no token at all.
    let local = opts(&["fund-vault", "--rpc", RPC, "--execute"]);
    assert!(matches!(Gate::from_opts(&local).and_then(|g| g.execute_token_in(&local, &repo.root())), Ok(Some(_))));
    let pinned_local = opts(&["fund-vault", "--rpc", RPC, "--execute", "--packet-sha256", sha]);
    assert!(matches!(Gate::from_opts(&pinned_local).and_then(|g| g.execute_token_in(&pinned_local, &repo.root())), Err(OpsError::Usage(_))));
    let dry = opts(&["fund-vault", "--rpc", RPC]);
    assert!(matches!(Gate::from_opts(&dry).and_then(|g| g.execute_token_in(&dry, &repo.root())), Ok(None)));
    // Read-only commands take no --execute.
    for command in ["plan", "verify-bytes", "readback"] {
        let o = opts(&[command, "--rpc", RPC, "--execute"]);
        assert!(matches!(Gate::from_opts(&o).and_then(|g| g.execute_token_in(&o, &repo.root())), Err(OpsError::Usage(_))), "{command}");
    }
}

#[test]
fn command_line_parsing_rejects_typos_duplicates_and_missing_values() {
    let parse = |parts: &[&str]| {
        let args: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
        Opts::parse(&args)
    };
    assert!(matches!(parse(&[]), Err(OpsError::Usage(_))));
    assert!(matches!(parse(&["frobnicate"]), Err(OpsError::Usage(_))));
    assert!(matches!(parse(&["plan", "--i-have-aproval", "x"]), Err(OpsError::Usage(_))));
    assert!(matches!(parse(&["plan", "--rpc"]), Err(OpsError::Usage(_))));
    assert!(matches!(parse(&["plan", "--rpc", "a", "--rpc", "b"]), Err(OpsError::Usage(_))));
    assert!(matches!(parse(&["plan", "--execute=true"]), Err(OpsError::Usage(_))));
    let o = parse(&["propose", "initialize", "--rpc=http://127.0.0.1:8899", "--execute"]).unwrap();
    assert_eq!(o.positional, ["initialize"]);
    assert!(o.switch("execute"));
    assert_eq!(o.get("rpc"), Some("http://127.0.0.1:8899"));
}

#[test]
fn the_solana_cli_default_wallet_is_refused_by_path_without_being_read() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/nobody".into());
    for path in [
        format!("{home}/.config/solana/id.json"),
        "/definitely/not/there/.config/solana/id.json".to_string(),
    ] {
        match KeyFile::load(&path) {
            Err(OpsError::Refused(message)) => assert!(message.contains("default wallet"), "{message}"),
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("{path} must be refused"),
        }
    }
    // Not a file and not a pubkey: a usage error.
    assert!(matches!(KeyFile::load("not-a-file-or-key").err(), Some(OpsError::Usage(_))));
    // A bare base58 pubkey is a public-only key; asking it to sign is a usage error.
    let key = Keypair::new().pubkey();
    let loaded = KeyFile::load(&key.to_string()).expect("pubkey");
    assert_eq!(loaded.pubkey, key);
    assert!(matches!(loaded.keypair().err(), Some(OpsError::Usage(_))));
    // Members: exactly three distinct keys.
    let dir = TempDir::new("members");
    let file = dir.file("m.txt");
    write_members(&file, &[Pubkey::new_unique(), Pubkey::new_unique()]);
    assert!(ops::read_members(&file).is_err());
    let dup = Pubkey::new_unique();
    write_members(&file, &[dup, dup, Pubkey::new_unique()]);
    assert!(ops::read_members(&file).is_err());
    fs::write(&file, format!("# G\n{}\n\n{}  # second\n{}\n", Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique())).unwrap();
    assert_eq!(ops::read_members(&file).expect("three members").len(), 3);
}

// ---------------------------------------------------------------------------
// Packet size: computed before signing, refused over 1,232 bytes
// ---------------------------------------------------------------------------

#[test]
fn an_oversize_transaction_is_refused_before_any_signature_exists() {
    let payer = Keypair::new_from_array([0x11; 32]);
    let oversize = Step {
        id: "test.oversize".into(),
        label: "oversize".into(),
        instructions: vec![Instruction {
            program_id: Pubkey::new_from_array([0x22; 32]),
            accounts: Vec::new(),
            data: vec![7u8; 1_300],
        }],
        payer: payer.pubkey(),
        cu_limit: None,
        depends_on_previous: false,
    };
    let built = ops::build_step(&oversize);
    assert!(built.packet_bytes > ops::PACKET_LIMIT, "{}", built.packet_bytes);
    assert!(!built.fits());
    match ops::sign_checked("oversize", &built, &[&payer], Hash::default()) {
        Err(OpsError::Refused(message)) => assert!(message.contains("1232"), "{message}"),
        other => panic!("{other:?}"),
    }
    // A step that fits signs; a missing signer or an unrelated signer is refused.
    let small = Step {
        instructions: data_len_probe(),
        ..oversize.clone()
    };
    let built = ops::build_step(&small);
    assert!(built.fits());
    let tx = ops::sign_checked("small", &built, &[&payer], Hash::new_unique()).expect("signs");
    assert!(tx.is_signed());
    let stranger = Keypair::new();
    assert!(matches!(ops::sign_checked("small", &built, &[&stranger], Hash::default()), Err(OpsError::Refused(_))));
    assert!(matches!(ops::sign_checked("small", &built, &[], Hash::default()), Err(OpsError::Refused(_))));
}

fn data_len_probe() -> Vec<Instruction> {
    vec![Instruction {
        program_id: Pubkey::new_from_array([0x22; 32]),
        accounts: Vec::new(),
        data: vec![7u8; 100],
    }]
}

#[test]
fn the_loader_write_chunk_fits_the_packet_and_is_close_to_the_maximum() {
    let deployer = Pubkey::new_from_array([1; 32]);
    let buffer = Pubkey::new_from_array([2; 32]);
    let chunk = ops::max_write_chunk(&deployer, &buffer);
    eprintln!("FINDING loader write chunk: {chunk} bytes per transaction");
    assert!((900..=1_100).contains(&chunk), "{chunk}");
}

// ---------------------------------------------------------------------------
// verify-bytes (packet A8) on constructed and on world ProgramData
// ---------------------------------------------------------------------------

fn programdata(slot: u64, authority: Option<Pubkey>, elf: &[u8], padding: usize) -> Vec<u8> {
    use solana_loader_v3_interface::state::UpgradeableLoaderState;
    let mut data = bincode::serialize(&UpgradeableLoaderState::ProgramData {
        slot,
        upgrade_authority_address: authority,
    })
    .unwrap();
    data.resize(ops::PROGRAMDATA_HEADER_LEN, 0);
    data.extend_from_slice(elf);
    data.resize(data.len() + padding, 0);
    data
}

#[test]
fn verify_programdata_checks_header_elf_equality_and_zero_padding() {
    let elf: Vec<u8> = (0..5_000u32).map(|i| (i % 251) as u8).collect();
    let authority = Pubkey::new_from_array([9; 32]);

    let exact = programdata(77, Some(authority), &elf, 0);
    let report = ops::verify_programdata(&exact, &elf).expect("decodes");
    assert!(report.ok());
    assert_eq!((report.header.slot, report.header.authority), (77, Some(authority)));
    assert_eq!((report.data_len, report.elf_len, report.padding_len), (45 + 5_000, 5_000, 0));
    assert_eq!(report.elf_sha256, report.onchain_elf_sha256);

    let padded = programdata(77, Some(authority), &elf, 321);
    let report = ops::verify_programdata(&padded, &elf).unwrap();
    assert!(report.ok());
    assert_eq!(report.padding_len, 321);
    assert_eq!(report.payload_sha256, devnet::sha256_hex(&padded[45..]));

    let mut dirty = padded.clone();
    let last = dirty.len() - 1;
    dirty[last] = 1;
    let report = ops::verify_programdata(&dirty, &elf).unwrap();
    assert!(report.elf_matches && !report.padding_all_zero && !report.ok());

    let mut tampered = exact.clone();
    tampered[45 + 1_234] ^= 0xff;
    let report = ops::verify_programdata(&tampered, &elf).unwrap();
    assert!(!report.elf_matches);
    assert_eq!(report.first_mismatch, Some(1_234));

    let truncated = &exact[..exact.len() - 10];
    let report = ops::verify_programdata(truncated, &elf).unwrap();
    assert!(!report.ok());
    assert_eq!(report.first_mismatch, Some(4_990));

    // Immutable program: authority None decodes (and its 32 bytes must be zero).
    let immutable = programdata(5, None, &elf, 0);
    assert_eq!(ops::verify_programdata(&immutable, &elf).unwrap().header.authority, None);
    let mut bad_none = immutable.clone();
    bad_none[20] = 1;
    assert!(ops::verify_programdata(&bad_none, &elf).is_err());
    // Wrong variant / too short.
    let mut wrong_variant = exact.clone();
    wrong_variant[0] = 2;
    assert!(ops::verify_programdata(&wrong_variant, &elf).is_err());
    assert!(ops::verify_programdata(&exact[..44], &elf).is_err());
}

#[test]
fn verify_bytes_reads_the_world_programdata_and_catches_a_flipped_byte() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g_vault = world.rt.g_vault();
    let pd_address = ops::programdata_address(&musical_chairs_program::ID);
    let pd = world.rt.account(pd_address);
    let elf = pd.data[45..].to_vec();
    let elf_path = files.dir.file("program.so");
    fs::write(&elf_path, &elf).unwrap();
    let elf_sha = devnet::sha256_hex(&elf);

    let cluster = SvmCluster::new(&mut world.rt.svm);
    let receipt = ok(files.run(
        &cluster,
        &["verify-bytes", "--elf", &elf_path, "--expected-sha256", &elf_sha, "--expect-authority", "g-vault"],
    ));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["programdata"]["upgrade_authority"].as_str(), Some(g_vault.to_string().as_str()));
    assert_eq!(receipt["programdata"]["elf_len"].as_u64(), Some(elf.len() as u64));
    assert_eq!(receipt["programdata"]["zero_padding_len"].as_u64(), Some(0));
    assert_eq!(cluster.sends.get() + cluster.sims.get(), 0, "read-only");

    // A wrong authority expectation is a failed check, not a crash.
    let receipt = ok(files.run(
        &cluster,
        &["verify-bytes", "--elf", &elf_path, "--expected-sha256", &elf_sha, "--expect-authority", "none"],
    ));
    assert_eq!(receipt["all_ok"], false);
    assert_eq!(failing_checks(&receipt), ["programdata.upgrade_authority"]);

    // The file is held to the pin: a copy with one byte changed hashes differently and is refused
    // unless the (local-only) override names its own hash.
    let mut altered = elf.clone();
    let last = altered.len() - 1;
    altered[last] ^= 1;
    let altered_path = files.dir.file("altered.so");
    fs::write(&altered_path, &altered).unwrap();
    let message = refused(files.run(&cluster, &["verify-bytes", "--elf", &altered_path]));
    assert!(message.contains("pinned"), "{message}");
    let message = refused(files.run(
        &cluster,
        &["verify-bytes", "--elf", &altered_path, "--expected-sha256", &elf_sha],
    ));
    assert!(message.contains("pinned") || message.contains("hashes to"), "{message}");
    // The override exists on a local cluster only.
    let devnet_args = vec![
        "verify-bytes".to_string(), "--rpc".into(), DEVNET_URL.into(), "--cluster".into(), "devnet".into(),
        "--elf".into(), elf_path.clone(), "--expected-sha256".into(), elf_sha.clone(),
    ];
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    let message = refused(ops::run_with_cluster(&devnet_args, &cluster));
    assert!(message.contains("local cluster"), "{message}");
    cluster.set_genesis(LITESVM_GENESIS);

    // Flip one byte of the deployed ELF.
    {
        let mut svm = cluster.svm.borrow_mut();
        let mut account = svm.get_account(&pd_address).unwrap();
        account.data[45 + 1_000] ^= 0xff;
        svm.set_account(pd_address, account).unwrap();
    }
    let receipt = ok(files.run(&cluster, &["verify-bytes", "--elf", &elf_path, "--expected-sha256", &elf_sha]));
    assert_eq!(receipt["all_ok"], false);
    assert_eq!(receipt["programdata"]["first_mismatch_offset"].as_u64(), Some(1_000));
    assert!(failing_checks(&receipt).contains(&"programdata.elf_bytes_equal_pinned_elf".to_string()));
}

// ---------------------------------------------------------------------------
// squads-create: dry run, execute, idempotence, read-back, simulate-before-send
// ---------------------------------------------------------------------------

#[test]
fn squads_create_is_a_dry_run_until_execute_then_idempotent_and_reads_back() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let p = ops::identity(real_p_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);

    let dry = ok(files.run(&cluster, &["squads-create", "--payer", &files.payer]));
    assert_eq!(dry["mode"], "dry-run");
    assert_eq!(dry["multisigs"].as_array().unwrap().len(), 2);
    assert!(dry["multisigs"].as_array().unwrap().iter().all(|m| m["created_by_this_run"] == false));
    for step in dry["steps"].as_array().unwrap() {
        assert!(step["packet_bytes"].as_u64().unwrap() <= 1_232);
        assert_eq!(step["executed"], false);
        assert_eq!(step["fee_payer"].as_str(), Some(payer.pubkey().to_string().as_str()));
    }
    assert!(cluster.account_of(&g.multisig).is_none() && cluster.account_of(&p.multisig).is_none());
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (0, 0), "a dry run neither sends nor simulates");

    let done = ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    assert_eq!(done["mode"], "execute");
    assert_eq!(done["all_ok"], true, "{done}");
    assert!(done["multisigs"].as_array().unwrap().iter().all(|m| m["created_by_this_run"] == true));
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (2, 2));
    assert_eq!(*cluster.order.borrow(), ["simulate", "send", "simulate", "send"], "simulate precedes every send");

    let g_view = ops::decode_multisig(&cluster.account_of(&g.multisig).unwrap().data).unwrap();
    assert_eq!((g_view.threshold, g_view.time_lock, g_view.members.len()), (2, TWO_DAYS as u32, 3));
    assert_eq!(g_view.config_authority, Pubkey::default());
    let p_view = ops::decode_multisig(&cluster.account_of(&p.multisig).unwrap().data).unwrap();
    assert_eq!((p_view.threshold, p_view.time_lock), (2, 0));

    // Re-running creates nothing (profile verified, no new transaction).
    let again = ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    assert!(again["multisigs"].as_array().unwrap().iter().all(|m| m["created_by_this_run"] == false));
    assert_eq!(cluster.sends.get(), 2);

    // A5/A6 read-back of the Squads part.
    let receipt = ok(files.run(&cluster, &["readback", "--sections", "squads"]));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["g"]["vault_0"].as_str(), Some(g.vault.to_string().as_str()));
    assert_eq!(receipt["p"]["vault_0"].as_str(), Some(p.vault.to_string().as_str()));

    // The same read-back against a different member list fails its checks (exit 4 in the CLI).
    let other = files.dir.file("other-members.txt");
    write_members(&other, &[Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique()]);
    let mut args = files.args(&["readback", "--sections", "squads"]);
    let position = args.iter().position(|a| a == "--g-members").unwrap();
    args[position + 1] = other;
    let receipt = ok(ops::run_with_cluster(&args, &cluster));
    assert_eq!(receipt["all_ok"], false);
    assert_eq!(failing_checks(&receipt), ["G.members_and_permissions"]);

    // Same create key, different members: squads-create refuses instead of pretending.
    let mut args = files.args(&["squads-create", "--payer", &files.payer, "--execute"]);
    let position = args.iter().position(|a| a == "--g-members").unwrap();
    args[position + 1] = files.dir.file("other-members.txt");
    assert!(refused(ops::run_with_cluster(&args, &cluster)).contains("DIFFERENT profile"));
}

#[test]
fn a_simulation_error_refuses_the_send() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);

    cluster.fail_simulation.set(true);
    let message = refused(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    assert!(message.contains("simulateTransaction failed"), "{message}");
    assert!(message.contains("nothing was sent"), "{message}");
    assert_eq!(cluster.sends.get(), 0, "no send after a failed simulation");
    assert_eq!(cluster.sims.get(), 1);
    assert!(cluster.account_of(&g.multisig).is_none());

    cluster.fail_simulation.set(false);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    assert_eq!(cluster.sends.get(), 2);
}

#[test]
fn execute_on_devnet_goes_through_the_packet_pin_and_the_devnet_genesis_and_records_the_pin() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let (g, p) = world.create_g_and_p();
    assert!(g.success && p.success);
    let files = Files::write(&world, &payer);
    let repo = PacketRepo::new("the reviewed M14 packet");
    let pending = files.dir.file("pending.jsonl");
    let cluster = SvmCluster::new(&mut world.rt.svm);
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);

    let fund = |extra: &[String]| {
        let mut args: Vec<String> = [
            "fund-vault", "--rpc", DEVNET_URL, "--cluster", "devnet", "--payer", files.payer.as_str(),
            "--g-create-key", files.g_create.as_str(), "--g-members", files.g_members.as_str(),
            "--lamports", "5000000", "--execute",
        ]
        .iter()
        .map(|part| part.to_string())
        .collect();
        args.extend(extra.iter().cloned());
        ops::run_with_cluster_in(&args, &cluster, &repo.root())
    };
    let pin = repo.pin_flags();
    let with_pending = |mut flags: Vec<String>| {
        flags.extend(["--pending-file".to_string(), pending.clone()]);
        flags
    };

    // No pin, a half pin, the removed alias, a path override: refused before any RPC call.
    assert!(refused(fund(&[])).contains("--packet-sha256"));
    assert!(refused(fund(&pin[..2])).contains("--packet-commit"));
    assert!(matches!(fund(&["--i-have-approval".into(), repo.sha.clone()]), Err(OpsError::Usage(_))));
    let mut overridden = with_pending(pin.clone());
    overridden.extend(["--packet".to_string(), repo.dir.file("elsewhere.md")]);
    assert!(refused(fund(&overridden)).contains("--packet is refused"));
    // The pin is fine but no write-ahead file was named: refused (still before any RPC call).
    assert!(refused(fund(&pin)).contains("--pending-file"));
    assert_eq!(cluster.genesis_reads.get(), 0, "the pin gate fires before any RPC call");
    assert_eq!(cluster.sends.get(), 0);

    // mainnet or any other genesis behind the devnet flag: refused, nothing sent.
    cluster.set_genesis(ops::MAINNET_GENESIS_HASH);
    assert!(refused(fund(&with_pending(pin.clone()))).contains("genesis"));
    cluster.set_genesis(LITESVM_GENESIS);
    assert!(refused(fund(&with_pending(pin.clone()))).contains("genesis"));
    assert_eq!(cluster.sends.get(), 0);

    // The pin, the devnet genesis and a write-ahead file: executes, and the receipt records the pin.
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    let receipt = ok(fund(&with_pending(pin.clone())));
    assert_eq!(receipt["mode"], "execute");
    assert_eq!(receipt["cluster"]["target"], "devnet");
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    assert_eq!(receipt["packet"]["packet_path"], ops::DEFAULT_PACKET_FILE);
    assert_eq!(receipt["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
    assert_eq!(receipt["packet"]["packet_sha256"].as_str(), Some(repo.sha.as_str()));
    assert_eq!(cluster.sends.get(), 1);
    // The write-ahead record names the signature before the send and is resolved after it.
    let lines: Vec<Value> = fs::read_to_string(&pending).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0]["event"].as_str(), lines[1]["event"].as_str()), (Some("write_ahead"), Some("resolved")));
    assert_eq!(lines[0]["signature"], lines[1]["signature"]);
    assert_eq!(lines[1]["outcome"], "confirmed");
    assert_eq!(lines[0]["genesis_hash"], ops::DEVNET_GENESIS_HASH);
    // N4: the write-ahead record names the verified packet pin of the run it belongs to.
    assert_eq!(lines[0]["packet"]["packet_path"], ops::DEFAULT_PACKET_FILE);
    assert_eq!(lines[0]["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
    assert_eq!(lines[0]["packet"]["packet_sha256"].as_str(), Some(repo.sha.as_str()));
    assert_eq!(lines[0]["command"], "fund-vault");
    assert_eq!(lines[1]["slot_source"], "signature_status");

    // An edit of the working packet after the pin was taken closes the gate again.
    repo.edit_working_file("the reviewed M14 packet, edited after review");
    assert!(refused(fund(&with_pending(pin.clone()))).contains("differs from the blob"));
    assert_eq!(cluster.sends.get(), 1);
}

#[test]
fn devnet_deploy_ceilings_cannot_be_raised_and_padding_needs_a_recorded_justification() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let repo = PacketRepo::new("the reviewed M14 packet");
    let pending = files.dir.file("pending.jsonl");
    let cluster = SvmCluster::new(&mut world.rt.svm);
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    let deploy = |extra: &[&str]| {
        let mut args: Vec<String> = ["deploy", "--rpc", DEVNET_URL, "--cluster", "devnet", "--execute", "--pending-file", pending.as_str()]
            .iter()
            .map(|part| part.to_string())
            .collect();
        args.extend(repo.pin_flags());
        args.extend(extra.iter().map(|part| part.to_string()));
        ops::run_with_cluster_in(&args, &cluster, &repo.root())
    };
    let message = refused(deploy(&["--max-spend-lamports", "16000000000"]));
    assert!(message.contains("15 SOL") && message.contains("cannot be raised"), "{message}");
    let message = refused(deploy(&["--max-data-len", "1500000"]));
    assert!(message.contains("--max-data-len-justification"), "{message}");
    let message = refused(deploy(&["--max-data-len", "1500000", "--max-data-len-justification", "   "]));
    assert!(message.contains("--max-data-len-justification"), "{message}");
    // At or below the ceiling, and with a justification, the new checks pass (the run then stops at the
    // next missing argument, which is a usage error, not one of these refusals).
    assert!(matches!(deploy(&["--max-spend-lamports", "15000000000"]), Err(OpsError::Usage(_))));
    assert!(matches!(
        deploy(&["--max-data-len", "1500000", "--max-data-len-justification", "room for a v2 of the economics"]),
        Err(OpsError::Usage(_))
    ));
    assert_eq!(cluster.sends.get(), 0);
}

// ---------------------------------------------------------------------------
// propose -> approve -> time lock -> execute (initialize, then ORAO register)
// ---------------------------------------------------------------------------

#[test]
fn initialize_and_orao_register_through_the_tool_end_to_end() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));

    let init_flags = |files: &Files| {
        vec![
            "--sponsor".to_string(), files.sponsor.clone(),
            "--management-recipient".to_string(), files.management_pub.clone(),
        ]
    };
    let with = |parts: &[&str], extra: Vec<String>| {
        let mut args = files.args(parts);
        args.extend(extra);
        args
    };

    // ---- propose initialize: dry run first -------------------------------------------------
    let propose_dry = ok(ops::run_with_cluster(
        &with(&["propose", "initialize", "--member", &files.g_member[0]], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(propose_dry["mode"], "dry-run");
    assert_eq!(propose_dry["transaction_index"], 1);
    let create_step = &propose_dry["steps"][0];
    // The hard rule: proposer = creator = rent payer = FEE PAYER, one signature, under the packet.
    let proposer = world_member(&files, 0);
    assert_eq!(create_step["fee_payer"].as_str(), Some(proposer.as_str()));
    assert_eq!(create_step["required_signers"].as_array().unwrap().len(), 1);
    let create_bytes = create_step["packet_bytes"].as_u64().unwrap();
    eprintln!("FINDING vault_transaction_create packet: {create_bytes} bytes (measured 1,140 in the preflight)");
    assert!(create_bytes <= 1_232, "{create_bytes}");
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (2, 2), "the dry run sent nothing");

    // --execute needs the index the dry run printed.
    let message = refused(ops::run_with_cluster(
        &with(&["propose", "initialize", "--member", &files.g_member[0], "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("--expect-tx-index 1"), "{message}");
    let message = refused(ops::run_with_cluster(
        &with(&["propose", "initialize", "--member", &files.g_member[0], "--execute", "--expect-tx-index", "7"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("would take #1"), "{message}");

    let proposed = ok(ops::run_with_cluster(
        &with(&["propose", "initialize", "--member", &files.g_member[0], "--execute", "--expect-tx-index", "1"], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(proposed["all_ok"], true, "{proposed}");
    assert_eq!(proposed["proposal_status"], "Active");
    assert_eq!(proposed["steps"].as_array().unwrap().len(), 2);
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (4, 4));
    let proposal = ops::decode_proposal(&cluster.account_of(&sq::proposal_address(&g.multisig, 1)).unwrap().data).unwrap();
    assert!(matches!(proposal.status, ProposalStatus::Active(_)));

    // A second identical proposal is refused (no duplicates while one is live).
    let message = refused(ops::run_with_cluster(
        &with(&["propose", "initialize", "--member", &files.g_member[0], "--execute", "--expect-tx-index", "2"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("already exists"), "{message}");

    // ---- approve: only what was proposed ------------------------------------------------------
    let mut wrong = init_flags(&files);
    wrong[3] = files.other_management_pub.clone();
    let message = refused(ops::run_with_cluster(
        &with(&["approve", "initialize", "--member", &files.g_member[1], "--tx-index", "1", "--execute"], wrong),
        &cluster,
    ));
    assert!(message.contains("does NOT store the intended"), "{message}");
    assert_eq!(cluster.sends.get(), 4, "no vote on a mismatching proposal");

    let first = ok(ops::run_with_cluster(
        &with(&["approve", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(first["after"]["status"], "Active");
    assert_eq!(first["after"]["approvals"], 1);
    assert_eq!(first["all_ok"], true, "{first}");
    assert_eq!(first["checks"].as_array().unwrap().len(), 2, "approve's all_ok is computed from read-backs");
    let message = refused(ops::run_with_cluster(
        &with(&["approve", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("already voted"), "{message}");
    // Not enough approvals yet: execute refuses.
    let message = refused(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("not Approved"), "{message}");
    let second = ok(ops::run_with_cluster(
        &with(&["approve", "initialize", "--member", &files.g_member[1], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(second["after"]["status"], "Approved");
    let approved_at = second["after"]["approved_at"].as_i64().expect("approved_at");
    assert_eq!(second["after"]["executable_after_unix"].as_i64(), Some(approved_at + TWO_DAYS));

    // ---- execute: refused before the time lock, never sent ---------------------------------------
    let sends_before = cluster.sends.get();
    let message = refused(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("time lock not released") && message.contains("6021"), "{message}");
    cluster.advance(TWO_DAYS - 1);
    refused(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(cluster.sends.get(), sends_before, "an early execute is refused before any simulation or send");
    cluster.advance(1);

    // The sponsor must co-sign the OUTER transaction: a public key alone cannot.
    let mut pubkey_only = init_flags(&files);
    pubkey_only[1] = files.sponsor_pub.clone();
    let by_pubkey = ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], pubkey_only),
        &cluster,
    );
    assert!(matches!(by_pubkey, Err(OpsError::Usage(_))), "{by_pubkey:?}");
    assert_eq!(cluster.sends.get(), sends_before);

    let dry_execute = ok(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1"], init_flags(&files)),
        &cluster,
    ));
    let execute_bytes = dry_execute["steps"][0]["packet_bytes"].as_u64().unwrap();
    eprintln!("FINDING vault_transaction_execute(initialize) packet: {execute_bytes} bytes (measured 878)");
    assert!(execute_bytes <= 1_232);
    assert_eq!(dry_execute["steps"][0]["required_signers"].as_array().unwrap().len(), 2, "member + sponsor");
    assert_eq!(cluster.sends.get(), sends_before);

    let executed = ok(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert_eq!(executed["all_ok"], true, "{executed}");
    assert_eq!(cluster.sends.get(), sends_before + 1);
    assert_eq!(*cluster.order.borrow().last().unwrap(), "send");
    let pool_key = musical_chairs_financial_runtime::runtime::pda(&[musical_chairs_program::state::POOL_SEED]).0;
    assert!(cluster.account_of(&pool_key).is_some(), "initialize ran inside the vault transaction");

    // A proposal that has run cannot run again.
    let message = refused(ops::run_with_cluster(
        &with(&["execute", "initialize", "--member", &files.g_member[0], "--tx-index", "1", "--execute"], init_flags(&files)),
        &cluster,
    ));
    assert!(message.contains("precondition") || message.contains("not Approved"), "{message}");

    // ---- A6 read-back of the initialized program -------------------------------------------------
    let receipt = ok(files.run(
        &cluster,
        &["readback", "--sections", "squads,program,initialize", "--sponsor", &files.sponsor_pub, "--management-recipient", &files.management_pub],
    ));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    let names: Vec<&str> = receipt["checks"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    for expected in [
        "pool.governance_is_g_vault",
        "pool.guardian_vault_is_p_vault",
        "admission.guardian_vault_is_p_vault",
        "admission.guardian_vault_equals_pool.guardian_vault",
        "pool.cluster_domain",
        "programdata.upgrade_authority_is_g_vault",
        "config1.provider_client",
    ] {
        assert!(names.contains(&expected), "read-back is missing `{expected}`: {names:?}");
    }
    assert_eq!(receipt["pool"]["sponsor"].as_str(), Some(world_pubkey(&files.sponsor_pub).as_str()));

    // ---- fund the G vault, then register -------------------------------------------------------------
    let funded_dry = ok(files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "20000000"]));
    assert_eq!(funded_dry["mode"], "dry-run");
    assert!(refused(files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "500000000", "--execute"])).contains("outside"));
    let funded = ok(files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "20000000", "--execute"]));
    assert_eq!(funded["vault_balance_after"].as_u64(), Some(20_000_000));

    let propose_register = ok(files.run(
        &cluster,
        &["propose", "register", "--member", &files.g_member[0], "--execute", "--expect-tx-index", "2"],
    ));
    assert_eq!(propose_register["transaction_index"], 2);
    for member in [0usize, 1] {
        ok(files.run(&cluster, &["approve", "register", "--member", &files.g_member[member], "--tx-index", "2", "--execute"]));
    }
    assert!(refused(files.run(&cluster, &["execute", "register", "--member", &files.g_member[0], "--tx-index", "2", "--execute"])).contains("time lock not released"));
    cluster.advance(TWO_DAYS);
    let registered = ok(files.run(&cluster, &["execute", "register", "--member", &files.g_member[0], "--tx-index", "2", "--execute"]));
    assert_eq!(registered["all_ok"], true, "{registered}");
    assert_eq!(registered["outer_signers"].as_array().unwrap().len(), 0, "register needs no outer signer");

    let receipt = ok(files.run(
        &cluster,
        &["readback", "--sponsor", &files.sponsor_pub, "--management-recipient", &files.management_pub],
    ));
    assert_eq!(receipt["all_ok"], true, "{receipt}");
    for expected in ["orao_client.owner_is_g_vault", "orao_client.state_is_rng_authority", "orao_client.account_owner_is_orao_program"] {
        assert!(receipt["checks"].as_array().unwrap().iter().any(|c| c["name"] == expected), "{expected}");
    }
    assert_eq!(receipt["orao"]["owner"].as_str(), Some(g.vault.to_string().as_str()));
}

fn world_member(files: &Files, ordinal: usize) -> String {
    let bytes: Vec<u8> = serde_json::from_str(&fs::read_to_string(&files.g_member[ordinal]).unwrap()).unwrap();
    Keypair::try_from(&bytes[..]).unwrap().pubkey().to_string()
}

fn world_pubkey(path: &str) -> String {
    fs::read_to_string(path).unwrap().trim().to_string()
}

#[test]
fn propose_refuses_when_the_program_authority_is_not_the_g_vault_and_resumes_a_half_made_proposal() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let pd_address = ops::programdata_address(&musical_chairs_program::ID);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let flags = [
        "--member", files.g_member[0].as_str(), "--sponsor", files.sponsor.as_str(),
        "--management-recipient", files.management_pub.as_str(),
    ];
    let mut propose_initialize: Vec<&str> = vec!["propose", "initialize"];
    propose_initialize.extend(flags);

    // Hand the authority to someone else: the proposal would fail after a wasted 48 hours.
    let genuine = cluster.account_of(&pd_address).unwrap();
    {
        let mut svm = cluster.svm.borrow_mut();
        let mut account = genuine.clone();
        let header = {
            use solana_loader_v3_interface::state::UpgradeableLoaderState;
            let mut bytes = bincode::serialize(&UpgradeableLoaderState::ProgramData {
                slot: 1,
                upgrade_authority_address: Some(Pubkey::new_unique()),
            })
            .unwrap();
            bytes.resize(45, 0);
            bytes
        };
        account.data[..45].copy_from_slice(&header);
        svm.set_account(pd_address, account).unwrap();
    }
    let message = refused(files.run(&cluster, &propose_initialize));
    assert!(message.contains("upgrade_authority_is_g_vault"), "{message}");
    cluster.svm.borrow_mut().set_account(pd_address, genuine).unwrap();

    // Half-made proposal: vault_transaction_create landed, proposal_create did not.
    let mut with_index = propose_initialize.clone();
    with_index.extend(["--execute", "--expect-tx-index", "1"]);
    let full = ok(files.run(&cluster, &with_index));
    assert_eq!(full["resumed_proposal_only"], false);
    // Remove the proposal account to model the half-made state.
    let g = ops::identity(real_g_create_keypair().pubkey());
    let proposal_address = sq::proposal_address(&g.multisig, 1);
    {
        let mut svm = cluster.svm.borrow_mut();
        svm.set_account(proposal_address, Account::default()).unwrap();
    }
    assert!(cluster.account_of(&proposal_address).is_none_or(|a| a.data.is_empty() && a.lamports == 0));
    let mut resume = propose_initialize.clone();
    resume.extend(["--execute", "--expect-tx-index", "1"]);
    let resumed = ok(files.run(&cluster, &resume));
    assert_eq!(resumed["resumed_proposal_only"], true, "{resumed}");
    assert_eq!(resumed["steps"].as_array().unwrap().len(), 1, "only proposal_create is re-sent");
    assert_eq!(resumed["proposal_status"], "Active");
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

#[test]
fn fund_vault_checks_the_g_profile_the_vault_balance_and_reads_back() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    let fund = |extra: &[&str]| {
        let mut parts = vec!["fund-vault", "--payer", files.payer.as_str(), "--lamports", "20000000", "--execute"];
        parts.extend_from_slice(extra);
        files.run(&cluster, &parts)
    };

    // No G multisig yet: the full profile check refuses (it used to accept any Squads-owned account).
    assert!(refused(fund(&[])).contains("does not match the expected profile"));
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let sends_after_create = cluster.sends.get();

    // The wrong member list is a profile mismatch, not a funding.
    let other = files.dir.file("other-members.txt");
    write_members(&other, &[Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique()]);
    let mut args = files.args(&["fund-vault", "--payer", &files.payer, "--lamports", "20000000", "--execute"]);
    let position = args.iter().position(|a| a == "--g-members").unwrap();
    args[position + 1] = other;
    assert!(refused(ops::run_with_cluster(&args, &cluster)).contains("does not match the expected profile"));
    assert_eq!(cluster.sends.get(), sends_after_create);

    // The funding reads itself back.
    let funded = ok(fund(&[]));
    assert_eq!(funded["all_ok"], true, "{funded}");
    assert_eq!(funded["vault_balance_before"], 0);
    assert_eq!(funded["vault_balance_after"], 20_000_000);
    assert_eq!(funded["checks"].as_array().unwrap().len(), 1);

    // A transfer is not idempotent: a repeated run is refused by the balance guard.
    let message = refused(fund(&[]));
    assert!(message.contains("already holds") && message.contains("not funding it again"), "{message}");
    assert_eq!(cluster.sends.get(), sends_after_create + 1);
}

#[test]
fn fund_vault_caps_the_vaults_total_not_just_one_transfer() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let (g, p) = world.create_g_and_p();
    assert!(g.success && p.success);
    let vault = world.rt.g_vault();
    world.rt.credit_lamports(vault, 1_000_000); // below the ~2.6 M the vault needs
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    let message = refused(files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "49500000", "--execute"]));
    assert!(message.contains("TOTAL"), "{message}");
    assert_eq!(cluster.sends.get(), 0);
    let funded = ok(files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "40000000", "--execute"]));
    assert_eq!(funded["vault_balance_after"], 41_000_000);
    assert_eq!(funded["all_ok"], true);
}

// ---------------------------------------------------------------------------
// M14-TOOL-FIX2: read configs (N1), minContextSlot (N2), write-ahead durability (N3, N4)
// ---------------------------------------------------------------------------

/// The write-ahead file's lines as JSON.
fn pending_lines(path: &str) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every pending line is JSON"))
        .collect()
}

/// A world with the G and P multisigs created (through the tool, with no write-ahead file) and a
/// `fund-vault` command line that records to `pending`.
fn fund_vault_args(files: &Files, pending: &str, extra: &[&str]) -> Vec<String> {
    let mut parts =
        vec!["fund-vault", "--payer", files.payer.as_str(), "--lamports", "20000000", "--execute", "--pending-file", pending];
    parts.extend_from_slice(extra);
    files.args(&parts)
}

#[test]
fn rpc_read_configs_carry_the_commitment_at_the_top_level_and_min_context_slot_only_after_a_send() {
    // The regression: the commitment was nested as {"commitment":{"commitment":"confirmed"}}, which the
    // client's flattened config type silently turned into "no commitment" (the node then answers at
    // FINALIZED).  These are the configs exactly as they go on the wire.
    for (commitment, name) in [(CommitmentConfig::confirmed(), "confirmed"), (CommitmentConfig::finalized(), "finalized")] {
        let cluster = RpcCluster::connect("http://127.0.0.1:9", commitment);
        let (account, program_accounts) = cluster.request_configs().expect("configs");
        for (what, config) in [("account info", &account), ("program accounts", &program_accounts)] {
            assert_eq!(config["commitment"], name, "{what}: the commitment is a STRING at the top level: {config}");
            assert_eq!(config["encoding"], "base64", "{what}: {config}");
            assert!(config["minContextSlot"].is_null(), "{what}: no minContextSlot before any send: {config}");
        }
        // After a confirmed send every read carries the floor, and the commitment survives it.
        cluster.note_confirmed_slot(1_234);
        cluster.note_confirmed_slot(7); // a lower slot never lowers the floor
        let (account, program_accounts) = cluster.request_configs().expect("configs");
        for (what, config) in [("account info", &account), ("program accounts", &program_accounts)] {
            assert_eq!(config["minContextSlot"], 1_234, "{what}: {config}");
            assert_eq!(config["commitment"], name, "{what}: {config}");
            assert_eq!(config["encoding"], "base64", "{what}: {config}");
        }
    }
}

#[test]
fn a_confirmed_transfer_is_never_repeated_even_when_a_lagging_node_reports_it_unpaid() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);
    let pending = files.dir.file("pending.jsonl");
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let sends_before = cluster.sends.get();

    let first = ok(ops::run_with_cluster(&fund_vault_args(&files, &pending, &[]), &cluster));
    assert_eq!(first["all_ok"], true, "{first}");
    assert_eq!(cluster.sends.get(), sends_before + 1);
    let lines = pending_lines(&pending);
    assert_eq!((lines[0]["event"].as_str(), lines[1]["event"].as_str()), (Some("write_ahead"), Some("resolved")));
    let signature = lines[0]["signature"].as_str().unwrap().to_string();
    let slot = lines[1]["slot"].as_u64().expect("the confirmed record carries its slot");
    assert_eq!(lines[1]["slot_source"], "signature_status");
    assert_eq!(first["steps"][0]["slot"].as_u64(), Some(slot));
    assert_eq!(lines[0]["step"], "A5.fund_vault");
    assert_eq!(lines[0]["command"], "fund-vault");
    // N4: a loopback run has no packet pin, and the record says so explicitly.
    assert!(lines[0]["packet"].is_null(), "{}", lines[0]);

    // A lagging node: the vault looks unfunded again, so the balance guard alone would pay a second time.
    cluster.set_lamports(&g.vault, 0);
    // A new invocation starts with no slot of its own ...
    cluster.noted_slot.set(0);
    let message = refused(ops::run_with_cluster(&fund_vault_args(&files, &pending, &[]), &cluster));
    assert!(
        message.contains("already CONFIRMED") && message.contains(&signature) && message.contains("never sent twice"),
        "{message}"
    );
    assert_eq!(cluster.sends.get(), sends_before + 1, "the second transfer was never sent");
    // ... and seeds it from the confirmed record BEFORE any read it decides from.
    assert_eq!(cluster.noted_slot.get(), slot, "min slot is seeded from the write-ahead file at startup");

    // A confirmed record of ANOTHER cluster counts for nothing: neither guard nor slot seed.
    let foreign_signature = Signature::new_unique().to_string();
    let foreign = [
        serde_json::json!({ "schema": 1, "event": "write_ahead", "step": "A5.fund_vault", "signature": foreign_signature,
            "blockhash": Hash::new_unique().to_string(), "genesis_hash": ops::MAINNET_GENESIS_HASH }),
        serde_json::json!({ "schema": 1, "event": "resolved", "signature": foreign_signature, "outcome": "confirmed",
            "slot": 999_999_999u64, "genesis_hash": ops::MAINNET_GENESIS_HASH }),
    ];
    let other_file = files.dir.file("other-cluster.jsonl");
    fs::write(&other_file, foreign.iter().map(|record| format!("{record}\n")).collect::<String>()).unwrap();
    cluster.noted_slot.set(0);
    let funded = ok(ops::run_with_cluster(&fund_vault_args(&files, &other_file, &[]), &cluster));
    assert_eq!(funded["all_ok"], true, "{funded}");
    assert!(cluster.noted_slot.get() < 999_999_999, "a foreign record never seeds the floor: {}", cluster.noted_slot.get());
    assert_eq!(cluster.sends.get(), sends_before + 2);
}

#[test]
fn a_confirmed_send_whose_slot_is_not_reported_falls_back_to_the_cluster_slot_and_fails_loudly_without_one() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));

    // (a) The node confirms the transfer but its status never reports a slot: after the polls the tool
    // reads the cluster slot (observed AFTER the confirmation), uses it as the floor, and records its source.
    cluster.hide_slot.set(true);
    let pending = files.dir.file("pending-fallback.jsonl");
    let done = ok(ops::run_with_cluster(&fund_vault_args(&files, &pending, &["--slot-polls", "2"]), &cluster));
    assert_eq!(done["all_ok"], true, "{done}");
    let lines = pending_lines(&pending);
    assert_eq!(lines[1]["outcome"], "confirmed");
    assert_eq!(lines[1]["slot_source"], "get_slot_after_confirmation");
    let slot = lines[1]["slot"].as_u64().expect("a fallback slot is recorded");
    assert!(slot > 0 && cluster.noted_slot.get() == slot, "the floor is advanced to the fallback slot");

    // (b) Neither the status nor getSlot works: the step ENDS with an error that says the transaction
    // CONFIRMED (and names the signature); it never carries on with a floor of 0.
    cluster.set_lamports(&g.vault, 0);
    cluster.fail_slot.set(true);
    cluster.noted_slot.set(0);
    let sends = cluster.sends.get();
    let pending_b = files.dir.file("pending-no-slot.jsonl");
    let message = match ops::run_with_cluster(&fund_vault_args(&files, &pending_b, &["--slot-polls", "1"]), &cluster) {
        Err(OpsError::Failed(message)) => message,
        other => panic!("expected a failure that says the transaction confirmed, got {other:?}"),
    };
    assert_eq!(cluster.sends.get(), sends + 1, "the transaction did land");
    let lines = pending_lines(&pending_b);
    let signature = lines[0]["signature"].as_str().unwrap();
    assert!(message.contains("CONFIRMED") && message.contains(signature) && message.contains("Do NOT re-send"), "{message}");
    assert_eq!((lines[1]["event"].as_str(), lines[1]["outcome"].as_str()), (Some("resolved"), Some("confirmed")));
    assert!(lines[1]["slot"].is_null());
    // A confirmed record without a slot still blocks the repeat.
    let again = refused(ops::run_with_cluster(&fund_vault_args(&files, &pending_b, &["--slot-polls", "1"]), &cluster));
    assert!(again.contains("already CONFIRMED") && again.contains(signature), "{again}");
    assert_eq!(cluster.sends.get(), sends + 1);
    assert!(matches!(
        ops::run_with_cluster(&fund_vault_args(&files, &pending_b, &["--slot-polls", "0"]), &cluster),
        Err(OpsError::Usage(_))
    ));
}

#[test]
fn an_unreadable_or_unparsable_write_ahead_file_refuses_every_command_instead_of_being_skipped() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    let run = |pending: &str| ops::run_with_cluster(&fund_vault_args(&files, pending, &[]), &cluster);
    let good = serde_json::json!({
        "schema": 1, "event": "write_ahead", "step": "x", "signature": Signature::new_unique().to_string(),
        "blockhash": Hash::new_unique().to_string(), "genesis_hash": LITESVM_GENESIS,
    });
    let resolved = serde_json::json!({
        "schema": 1, "event": "resolved", "signature": good["signature"], "outcome": "expired_never_landed", "genesis_hash": LITESVM_GENESIS,
    });

    // A read error that is not "no such file" (here: the path is a directory) refuses.
    let directory = files.dir.file("a-directory.jsonl");
    fs::create_dir_all(&directory).unwrap();
    let message = refused(run(&directory));
    assert!(message.contains("cannot read the write-ahead file"), "{message}");

    // A torn last line (half a record, no newline) refuses with its line number; so does every other
    // line the tool did not write.  Blank lines are only blank.
    let bad_lines: [(&str, String, &str); 6] = [
        ("truncated", format!("{good}\n{{\"schema\":1,\"event\":\"resolved\",\"signa"), "line 2"),
        ("not json", format!("{good}\n{resolved}\nthis is not json\n"), "line 3"),
        ("unknown event", format!("{good}\n{{\"event\":\"something_else\"}}\n"), "line 2"),
        ("no genesis", format!("{{\"event\":\"write_ahead\",\"signature\":{},\"blockhash\":{}}}\n", good["signature"], good["blockhash"]), "line 1"),
        ("bad blockhash", format!("{{\"event\":\"write_ahead\",\"signature\":{},\"blockhash\":\"nope\",\"genesis_hash\":\"g\"}}\n", good["signature"]), "line 1"),
        ("resolution without outcome", "{\"event\":\"resolved\",\"signature\":\"abc\"}\n".to_string(), "line 1"),
    ];
    for (name, content, line) in bad_lines {
        let path = files.dir.file(&format!("bad-{}.jsonl", name.replace(' ', "-")));
        fs::write(&path, content).unwrap();
        let message = refused(run(&path));
        assert!(message.contains(line) && message.contains("write-ahead"), "{name}: {message}");
        assert_eq!(cluster.sends.get(), 0, "{name}: nothing is sent past an unreadable record");
    }
    // The same file also stops a READ-ONLY command that was given it (its answers seed the read floor).
    let broken = files.dir.file("bad-not-json.jsonl");
    let message = refused(files.run(&cluster, &["plan", "--pending-file", &broken]));
    assert!(message.contains("line 3"), "{message}");

    // A file that is only good records, with blank lines between, is read.
    let fine = files.dir.file("fine-with-blanks.jsonl");
    fs::write(&fine, format!("{good}\n\n{resolved}\n   \n")).unwrap();
    let message = match run(&fine) {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    };
    assert!(!message.contains("write-ahead file"), "a well-formed file is not a pending-file refusal: {message}");
}

#[test]
fn an_unresolved_write_ahead_record_blocks_fund_vault_too_until_its_blockhash_expires() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let sends = cluster.sends.get();
    let pending = files.dir.file("pending.jsonl");
    // A send that timed out in an earlier run: recorded before the send, never resolved.
    let stray = Signature::new_unique();
    let record = serde_json::json!({
        "schema": 1, "event": "write_ahead", "step": "A5.fund_vault", "signature": stray.to_string(),
        "blockhash": Hash::new_unique().to_string(), "genesis_hash": LITESVM_GENESIS,
    });
    fs::write(&pending, format!("{record}\n")).unwrap();

    cluster.blockhash_still_valid.set(true);
    let args = fund_vault_args(&files, &pending, &["--pending-wait-seconds", "0"]);
    let message = refused(ops::run_with_cluster(&args, &cluster));
    assert!(message.contains("MAY STILL LAND") && message.contains(&stray.to_string()), "{message}");
    assert_eq!(cluster.sends.get(), sends, "nothing is rebuilt while the pending transaction may still land");

    // Once its blockhash has expired without the signature appearing, the record is closed and the
    // transfer is built and sent exactly once.
    cluster.blockhash_still_valid.set(false);
    let done = ok(ops::run_with_cluster(&args, &cluster));
    assert_eq!(done["all_ok"], true, "{done}");
    assert_eq!(done["pending_resolved"][0]["outcome"], "expired_never_landed");
    assert_eq!(cluster.sends.get(), sends + 1);
}

#[test]
fn the_removed_flags_are_refused_by_the_offline_commands_too() {
    let dir = TempDir::new("removed-flags");
    let strings = |parts: &[&str]| -> Vec<String> { parts.iter().map(|part| part.to_string()).collect() };
    let out = dir.file("fixtures-out");
    for (args, usage_error) in [
        (strings(&["validator-fixtures", &out, "--i-have-approval", "abc"]), true),
        (strings(&["validator-fixtures", &out, "--packet", "elsewhere.md"]), false),
        (strings(&["flow", "init", "--wallet-dir", &dir.dir(), "--i-have-approval", "abc"]), true),
        (strings(&["flow", "init", "--wallet-dir", &dir.dir(), "--packet", "elsewhere.md"]), false),
    ] {
        let result = ops::run_with_cluster(&args, &UnreachableCluster);
        match (result, usage_error) {
            (Err(OpsError::Usage(message)), true) => assert!(message.contains("REMOVED"), "{message}"),
            (Err(OpsError::Refused(message)), false) => assert!(message.contains("--packet is refused"), "{message}"),
            (other, _) => panic!("{args:?}: expected the removed-flag refusal, got {other:?}"),
        }
    }
    assert!(!std::path::Path::new(&out).exists(), "nothing was written before the refusal");
}

#[test]
fn file_arguments_never_reach_the_default_wallet_or_a_keypair_shaped_log() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/nobody".into());
    let wallet = format!("{home}/.config/solana/id.json");
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);

    // --elf and --program-so are refused by path, before any read.
    assert!(refused(files.run(&cluster, &["verify-bytes", "--elf", &wallet])).contains("default wallet"));
    let out = files.dir.file("fixtures-out");
    let args: Vec<String> = ["validator-fixtures", out.as_str(), "--program-so", wallet.as_str(), "--g-create-key", files.g_create.as_str()]
        .iter()
        .map(|part| part.to_string())
        .collect();
    assert!(refused(ops::run_with_cluster(&args, &UnreachableCluster)).contains("default wallet"));

    // The write-ahead file: never the default wallet, never `.json`, never keypair-shaped.
    let attempt = |path: &str| files.run(&cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "20000000", "--pending-file", path]);
    assert!(refused(attempt(&wallet)).contains("default wallet"));
    assert!(refused(attempt(&files.payer)).contains(".jsonl"), "payer.json is a keypair file");
    let shaped = files.dir.file("shaped.jsonl");
    fs::copy(&files.payer, &shaped).unwrap();
    assert!(refused(attempt(&shaped)).contains("keypair-shaped"));
    assert_eq!(cluster.sends.get(), 0);
    // An ordinary new path is fine.
    assert!(attempt(&files.dir.file("fine.jsonl")).is_err_and(|e| !matches!(e, OpsError::Refused(m) if m.contains("keypair") || m.contains("default wallet"))));
}

#[test]
fn propose_scans_every_live_proposal_for_a_duplicate_not_only_the_newest() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let member = files.g_member[0].as_str();
    let flags = ["--member", member, "--sponsor", files.sponsor.as_str(), "--management-recipient", files.management_pub.as_str()];
    let mut initialize = vec!["propose", "initialize"];
    initialize.extend(flags);
    let mut first = initialize.clone();
    first.extend(["--execute", "--expect-tx-index", "1"]);
    ok(files.run(&cluster, &first));
    // A DIFFERENT live proposal now sits at the newest index.
    ok(files.run(&cluster, &["propose", "register", "--member", member, "--execute", "--expect-tx-index", "2"]));
    let sends = cluster.sends.get();
    // The identical initialize proposal at #1 is still live: refused although #2 is the newest.
    let mut again = initialize.clone();
    again.extend(["--execute", "--expect-tx-index", "3"]);
    let message = refused(files.run(&cluster, &again));
    assert!(message.contains("already exists at #1"), "{message}");
    assert_eq!(cluster.sends.get(), sends);
}

#[test]
fn plan_lists_every_step_with_accounts_signers_fee_payer_rent_and_packet_sizes() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let elf_path = files.dir.file("program.so");
    let elf = world.rt.account(ops::programdata_address(&musical_chairs_program::ID)).data[45..].to_vec();
    fs::write(&elf_path, &elf).unwrap();
    let elf_sha = devnet::sha256_hex(&elf);
    let deployer = Keypair::new_from_array([0x21; 32]);
    let program = Keypair::new_from_array([0x22; 32]);
    let buffer = Keypair::new_from_array([0x23; 32]);
    let (deployer_file, program_file, buffer_file) = (files.dir.file("deployer.json"), files.dir.file("program.json"), files.dir.file("buffer.json"));
    write_keypair(&deployer_file, &deployer);
    write_keypair(&program_file, &program);
    write_keypair(&buffer_file, &buffer);
    let cluster = SvmCluster::new(&mut world.rt.svm);

    let plan = ok(files.run(
        &cluster,
        &[
            "plan", "--elf", &elf_path, "--expected-sha256", &elf_sha, "--deployer", &deployer_file,
            "--program-keypair", &program_file, "--buffer-keypair", &buffer_file, "--payer", &files.payer,
            "--sponsor", &files.sponsor_pub, "--management-recipient", &files.management_pub,
        ],
    ));
    assert_eq!(plan["all_ok"], true, "{plan}");
    assert_eq!(plan["mode"].as_str().map(|m| m.starts_with("read-only")), Some(true));
    let phases = plan["phases"].as_array().unwrap();
    let ids: Vec<&str> = phases.iter().map(|p| p["phase"].as_str().unwrap()).collect();
    assert_eq!(ids, ["A2", "A3", "A4-prep", "A4.propose", "A4.approve", "A4.execute"]);
    let mut max_packet = 0u64;
    for phase in phases {
        for step in phase["steps"].as_array().unwrap() {
            if let Some(bytes) = step["packet_bytes"].as_u64() {
                assert!(bytes <= 1_232, "{}: {bytes}", step["step"]);
                max_packet = max_packet.max(bytes);
                assert!(step["fee_payer"].is_string(), "{}", step["step"]);
                assert!(step["required_signers"].as_array().is_some_and(|s| !s.is_empty()));
                assert!(step["accounts"].as_array().is_some_and(|a| !a.is_empty()));
            }
        }
    }
    assert!(max_packet > 0);
    // Propose steps carry the proposer as fee payer with a single signature.
    let propose = phases[3]["steps"].as_array().unwrap();
    assert_eq!(propose[0]["step"], "A4.vault_transaction_create");
    assert_eq!(propose[0]["required_signers"].as_array().unwrap().len(), 1);
    assert_eq!(propose[0]["fee_payer"], propose[0]["required_signers"][0]);
    let writes = phases[1]["steps"].as_array().unwrap().iter().find(|s| s["step"] == "A3.write").unwrap();
    assert_eq!(writes["repeat"].as_u64(), Some(elf.len().div_ceil(writes["chunk_bytes"].as_u64().unwrap() as usize) as u64));
    let rent = &plan["rent_lamports_estimates"];
    for key in ["buffer", "program", "programdata", "multisig_each", "proposal_each", "vault_transaction_initialize"] {
        assert!(rent[key].as_u64().unwrap() > 0, "rent `{key}`");
    }
    assert_eq!(plan["initialize_parameters"]["public_series_a_enabled"], true);
    assert_eq!(plan["initialize_parameters"]["cluster_domain_utf8"], "musical-chairs-devnet-domain-v1!");
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (0, 0), "plan signs and sends nothing");
}

// ---------------------------------------------------------------------------
// validator-fixtures (offline)
// ---------------------------------------------------------------------------

#[test]
fn validator_fixtures_writes_the_account_files_and_the_exact_validator_arguments() {
    let dir = TempDir::new("validator");
    let out = dir.file("out");
    let args: Vec<String> = ["validator-fixtures", out.as_str()].iter().map(|s| s.to_string()).collect();
    let receipt = ok(ops::run_with_cluster(&args, &UnreachableCluster));
    let command = receipt["validator_command"].as_str().unwrap();
    assert!(command.starts_with("solana-test-validator --reset"), "{command}");
    for needle in [
        "--bpf-program SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf",
        "squads-devnet.so",
        "--bpf-program VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU",
        "orao-devnet.so",
        "--account BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr",
        "--account 3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr",
    ] {
        assert!(command.contains(needle), "missing `{needle}` in {command}");
    }
    for file in ["squads_program_config.json", "orao_network_state.json", "orao_treasury.json"] {
        let body: Value = serde_json::from_slice(&fs::read(Path::new(&out).join(file)).unwrap()).unwrap();
        assert!(body["pubkey"].is_string());
        assert_eq!(body["account"]["data"][1], "base64");
        assert_eq!(body["account"]["executable"], false);
    }
    // A program ELF that is not the pinned one is refused.
    let wrong_so = dir.file("wrong.so");
    fs::write(&wrong_so, b"not the pinned elf").unwrap();
    let g_file = dir.file("g.json");
    write_keypair(&g_file, &real_g_create_keypair());
    let args: Vec<String> = ["validator-fixtures", out.as_str(), "--program-so", wrong_so.as_str(), "--g-create-key", g_file.as_str()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(refused(ops::run_with_cluster(&args, &UnreachableCluster)).contains("pinned"));
}

/// A cluster that must never be called (offline subcommands).
struct UnreachableCluster;

impl Cluster for UnreachableCluster {
    fn genesis_hash(&self) -> Result<String, String> { panic!("offline command touched the cluster") }
    fn blockhash(&self) -> Result<Hash, String> { panic!("offline command touched the cluster") }
    fn account(&self, _: &Pubkey) -> Result<Option<Account>, String> { panic!("offline command touched the cluster") }
    fn rent_minimum(&self, _: usize) -> Result<u64, String> { panic!("offline command touched the cluster") }
    fn unix_time(&self) -> Result<i64, String> { panic!("offline command touched the cluster") }
    fn slot(&self) -> Result<u64, String> { panic!("offline command touched the cluster") }
    fn simulate(&self, _: &Transaction, _: bool) -> Result<SimOutcome, String> { panic!("offline command touched the cluster") }
    fn send(&self, _: &Transaction) -> Result<SendOutcome, SendError> { panic!("offline command touched the cluster") }
}

// ---------------------------------------------------------------------------
// deploy in LiteSVM (probe: loader-v3 DeployWithMaxDataLen support is not assumed)
// ---------------------------------------------------------------------------

fn standin_elf() -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("standin/squads-vault-standin/artifacts/squads_vault_standin.so"))
        .expect("the committed stand-in ELF (any valid SBF object works as the deploy payload)")
}

#[test]
#[ignore = "probe: runs the loader-v3 deploy inside LiteSVM, whose support for DeployWithMaxDataLen is not assumed; promote if it passes"]
fn deploy_in_litesvm_writes_deploys_verifies_and_moves_the_authority_to_the_g_vault() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let deployer = Keypair::new_from_array([0x31; 32]);
    world.rt.credit_lamports(deployer.pubkey(), 20_000_000_000);
    let files = Files::write(&world, &payer);
    let elf = standin_elf();
    let elf_path = files.dir.file("payload.so");
    fs::write(&elf_path, &elf).unwrap();
    let elf_sha = devnet::sha256_hex(&elf);
    let program = Keypair::new_from_array([0x32; 32]);
    let buffer = Keypair::new_from_array([0x33; 32]);
    let (deployer_file, program_file, buffer_file) = (files.dir.file("deployer.json"), files.dir.file("program.json"), files.dir.file("buffer.json"));
    write_keypair(&deployer_file, &deployer);
    write_keypair(&program_file, &program);
    write_keypair(&buffer_file, &buffer);
    let g = ops::identity(real_g_create_keypair().pubkey());
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.run(&cluster, &["squads-create", "--payer", &files.payer, "--execute"]));

    let deploy_args = |extra: &[&str]| {
        let mut parts = vec![
            "deploy", "--elf", elf_path.as_str(), "--expected-sha256", elf_sha.as_str(), "--deployer", deployer_file.as_str(),
            "--program-keypair", program_file.as_str(), "--buffer-keypair", buffer_file.as_str(), "--allow-program-id-mismatch",
        ];
        parts.extend_from_slice(extra);
        files.args(&parts)
    };
    let dry = ok(ops::run_with_cluster(&deploy_args(&[]), &cluster));
    assert_eq!(dry["mode"], "dry-run");
    assert!(dry["writes"]["to_write"].as_u64().unwrap() > 0);
    assert_eq!(cluster.sends.get(), 2, "only squads-create sent anything");

    let done = ok(ops::run_with_cluster(&deploy_args(&["--execute"]), &cluster));
    assert_eq!(done["all_ok"], true, "{done}");
    assert_eq!(done["upgrade_authority_after"].as_str(), Some(g.vault.to_string().as_str()));
    assert_eq!(done["programdata"]["ok"], true);
    assert!(done["deployer_balance_after_lamports"].as_u64().is_some(), "the deployer's post-deploy balance is recorded");
    let again = ok(ops::run_with_cluster(&deploy_args(&["--execute"]), &cluster));
    assert_eq!(again["all_ok"], true, "{again}");
}

// ---------------------------------------------------------------------------
// The one real-cluster test: a LOCAL solana-test-validator (loopback only)
// ---------------------------------------------------------------------------

fn airdrop(rpc: &solana_rpc_client::rpc_client::RpcClient, to: &Pubkey, lamports: u64) {
    let signature = rpc.request_airdrop(to, lamports).expect("local airdrop");
    for _ in 0..120 {
        if rpc.confirm_transaction(&signature).unwrap_or(false) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    panic!("airdrop to {to} not confirmed");
}

/// Real loader-v3 deploy (buffer, chunked writes, DeployWithMaxDataLen), SetAuthority to the G
/// vault, verify-bytes, real Squads multisig creation and the proposal lifecycle up to the time-lock
/// refusal, all against `MC_LOCAL_RPC` (loopback).  Start the validator exactly as
/// `docs/solana/devnet-ops.md` says (a fresh `--reset` ledger per run).  Set
/// `MC_LOCAL_PROGRAM_PRELOADED=1` when the validator was started with the pinned program preloaded at
/// its id with the G vault as upgrade authority to also exercise propose / approve / execute.
#[test]
#[ignore = "needs a local solana-test-validator: MC_LOCAL_RPC=http://127.0.0.1:8899 (see docs/solana/devnet-ops.md)"]
fn local_validator_deploy_authority_squads_and_proposal_lifecycle() {
    use solana_rpc_client::rpc_client::RpcClient;
    use solana_sdk::commitment_config::CommitmentConfig;

    let url = std::env::var("MC_LOCAL_RPC").expect("set MC_LOCAL_RPC=http://127.0.0.1:8899");
    let (_, host, _) = ops::parse_rpc_url(&url).expect("MC_LOCAL_RPC must be a URL");
    assert!(ops::is_loopback_host(&host), "MC_LOCAL_RPC must be loopback, got {host}");
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    let genesis = rpc.get_genesis_hash().expect("validator reachable").to_string();
    for public in [ops::MAINNET_GENESIS_HASH, ops::TESTNET_GENESIS_HASH, ops::DEVNET_GENESIS_HASH] {
        assert_ne!(genesis, public, "MC_LOCAL_RPC points at a public cluster");
    }

    let dir = TempDir::new("validator-run");
    // Fixed seeds for the multisig keys (the preloaded program's authority is derived from them);
    // everything else is random per run.
    let g_create = real_g_create_keypair();
    let p_create = real_p_create_keypair();
    let g_members: Vec<Keypair> = (0..3).map(|i| sq::member_keypair(0x61, i)).collect();
    let p_members: Vec<Keypair> = (0..3).map(|i| sq::member_keypair(0x71, i)).collect();
    let payer = Keypair::new();
    let deployer = Keypair::new();
    let program = Keypair::new();
    let buffer = Keypair::new();
    let sponsor = Keypair::new();
    for (key, sol) in [(&payer, 20), (&deployer, 10), (&sponsor, 5)] {
        airdrop(&rpc, &key.pubkey(), sol * 1_000_000_000);
    }
    for member in g_members.iter().chain(&p_members) {
        airdrop(&rpc, &member.pubkey(), 1_000_000_000);
    }
    let paths = |name: &str| dir.file(name);
    let (payer_f, deployer_f, program_f, buffer_f, sponsor_f) = (paths("payer.json"), paths("deployer.json"), paths("program.json"), paths("buffer.json"), paths("sponsor.json"));
    let (g_create_f, p_create_f, g_members_f, p_members_f, management_f) = (paths("g-create.json"), paths("p-create.json"), paths("g-members.txt"), paths("p-members.txt"), paths("management.pub"));
    write_keypair(&payer_f, &payer);
    write_keypair(&deployer_f, &deployer);
    write_keypair(&program_f, &program);
    write_keypair(&buffer_f, &buffer);
    write_keypair(&sponsor_f, &sponsor);
    write_keypair(&g_create_f, &g_create);
    write_keypair(&p_create_f, &p_create);
    let g_member_files: Vec<String> = g_members.iter().enumerate().map(|(i, k)| { let f = paths(&format!("g-member-{i}.json")); write_keypair(&f, k); f }).collect();
    write_members(&g_members_f, &g_members.iter().map(Signer::pubkey).collect::<Vec<_>>());
    write_members(&p_members_f, &p_members.iter().map(Signer::pubkey).collect::<Vec<_>>());
    write_pubkey(&management_f, &Keypair::new().pubkey());
    let elf = standin_elf();
    let elf_f = paths("payload.so");
    fs::write(&elf_f, &elf).unwrap();
    let elf_sha = devnet::sha256_hex(&elf);
    let g = ops::identity(g_create.pubkey());

    let cluster = ops::RpcCluster::connect(&url, CommitmentConfig::confirmed());
    let run = |parts: &[&str]| {
        let mut args: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
        for (flag, value) in [
            ("--rpc", url.as_str()), ("--g-create-key", g_create_f.as_str()), ("--p-create-key", p_create_f.as_str()),
            ("--g-members", g_members_f.as_str()), ("--p-members", p_members_f.as_str()),
        ] {
            args.push(flag.into());
            args.push(value.into());
        }
        ops::run_with_cluster(&args, &cluster)
    };

    // 1. Squads (the real binary loaded by the validator).
    let dry = ok(run(&["squads-create", "--payer", &payer_f]));
    assert_eq!(dry["mode"], "dry-run");
    let created = ok(run(&["squads-create", "--payer", &payer_f, "--execute"]));
    assert_eq!(created["all_ok"], true, "{created}");

    // 2. Real loader-v3 deploy of the payload, then the authority moves to the G vault.
    let deploy = |extra: &[&str]| {
        let mut parts = vec![
            "deploy", "--elf", elf_f.as_str(), "--expected-sha256", elf_sha.as_str(), "--deployer", deployer_f.as_str(),
            "--program-keypair", program_f.as_str(), "--buffer-keypair", buffer_f.as_str(), "--allow-program-id-mismatch",
        ];
        parts.extend_from_slice(extra);
        run(&parts)
    };
    let planned = ok(deploy(&[]));
    assert_eq!(planned["mode"], "dry-run");
    let deployed = ok(deploy(&["--execute"]));
    assert_eq!(deployed["all_ok"], true, "{deployed}");
    assert_eq!(deployed["upgrade_authority_after"].as_str(), Some(g.vault.to_string().as_str()));

    // 3. verify-bytes straight from the cluster (read-only), including the authority.
    let program_id = program.pubkey().to_string();
    let verified = ok(run(&[
        "verify-bytes", "--elf", &elf_f, "--expected-sha256", &elf_sha, "--program-id", &program_id, "--expect-authority", "g-vault",
    ]));
    assert_eq!(verified["all_ok"], true, "{verified}");
    assert_eq!(verified["programdata"]["elf_len"].as_u64(), Some(elf.len() as u64));
    // Re-running deploy converges without sending anything.
    let again = ok(deploy(&["--execute"]));
    assert_eq!(again["all_ok"], true, "{again}");

    // 4. Optional: the proposal lifecycle against a validator that preloaded the pinned program.
    if std::env::var_os("MC_LOCAL_PROGRAM_PRELOADED").is_none() {
        eprintln!("MC_LOCAL_PROGRAM_PRELOADED not set: skipping propose / approve / execute");
        return;
    }
    ok(run(&["fund-vault", "--payer", &payer_f, "--lamports", "20000000", "--execute"]));
    let flags = ["--sponsor", sponsor_f.as_str(), "--management-recipient", management_f.as_str()];
    let mut dry_args = vec!["propose", "register", "--member", g_member_files[0].as_str()];
    let dry = ok(run(&dry_args));
    let index = dry["transaction_index"].as_u64().expect("index").to_string();
    dry_args.extend(["--execute", "--expect-tx-index", index.as_str()]);
    let proposed = ok(run(&dry_args));
    assert_eq!(proposed["proposal_status"], "Active", "{proposed}");
    for member in [0usize, 1] {
        ok(run(&["approve", "register", "--member", &g_member_files[member], "--tx-index", &index, "--execute"]));
    }
    let early = refused(run(&["execute", "register", "--member", &g_member_files[0], "--tx-index", &index, "--execute"]));
    assert!(early.contains("time lock not released"), "{early}");
    let mut initialize = vec!["propose", "initialize", "--member", g_member_files[0].as_str()];
    initialize.extend(flags);
    let dry = ok(run(&initialize));
    let init_index = dry["transaction_index"].as_u64().expect("index").to_string();
    initialize.extend(["--execute", "--expect-tx-index", init_index.as_str()]);
    let proposed = ok(run(&initialize));
    assert_eq!(proposed["proposal_status"], "Active", "{proposed}");
    let create_bytes = proposed["steps"][0]["packet_bytes"].as_u64().unwrap();
    eprintln!("FINDING (local validator) vault_transaction_create packet {create_bytes} bytes");
    assert!(create_bytes <= 1_232);
}
