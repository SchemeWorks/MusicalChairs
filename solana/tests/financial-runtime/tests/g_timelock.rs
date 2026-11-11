//! M14-F11-TOOL: `devnet_ops g-timelock` (G reduces its own Squads time lock, then restores it) against the
//! REAL Squads v4 devnet binary and the production program ELF, in LiteSVM.
//!
//! Every test drives `devnet_ops::run_with_cluster` through a `Cluster` double over the harness's real-Squads
//! world: genesis (`initialize` and the ORAO `register`) really runs through G's propose / approve / time lock /
//! execute, exactly as in `devnet_ops_local.rs`; then `g-timelock` acts on that G.  The double counts the
//! simulations (signed and unsigned) and the sends, so "zero sends" is a counted fact, and it can land a send
//! whose response is then lost (F3).
//!
//! Nothing here touches a network.  Every key is derived from a fixed test seed or generated in memory and
//! written into a temp directory the tests own; no existing key file is read.
//!
//! ```text
//! cargo +1.92.0 test --locked --test g_timelock -- --test-threads=1 --nocapture
//! ```

use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use litesvm::LiteSVM;
use musical_chairs_financial_runtime::{
    controls::custom_code,
    devnet::{self, OraoProfile},
    devnet_ops::{
        self as ops,
        timelock::{self, ConfigAction, Direction},
        Cluster, OpsError, SendError, SendOutcome, SigOutcome, SimOutcome,
    },
    runtime::{real_g_create_keypair, real_p_create_keypair},
    squads_real::{self as sq, RealSquadsWorld},
};
use musical_chairs_program::controls::SQUADS_PROGRAM_ID;
use serde_json::{json, Value};
use solana_sdk::{
    account::Account,
    clock::Clock,
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

const RPC: &str = "http://127.0.0.1:8899";
const DEVNET_URL: &str = "https://api.devnet.solana.com";
const LITESVM_GENESIS: &str = "LiteSVMLocalGenesis11111111111111111111111111";
const TWO_DAYS: i64 = 172_800;
/// Squads `StaleProposal` / `TimeLockNotReleased` (IDL error codes).
const SQUADS_STALE_PROPOSAL: u32 = 6007;

// ---------------------------------------------------------------------------
// Test plumbing (copied from devnet_ops_local.rs: test files cannot import each other)
// ---------------------------------------------------------------------------

static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A temp dir the test owns (removed on drop).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "mc-g-timelock-{tag}-{}-{}",
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
        let root = PathBuf::from(dir.dir());
        let packet = root.join(ops::DEFAULT_PACKET_FILE);
        fs::create_dir_all(packet.parent().unwrap()).unwrap();
        fs::write(&packet, content).unwrap();
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

/// A [`Cluster`] over a LiteSVM instance.  Counts signed simulations, UNSIGNED simulations and sends, and can
/// land a send whose response is then lost, so the tests can assert "the probe never sends" and "an ambiguous
/// execute is held, never rebuilt".
struct SvmCluster<'a> {
    svm: RefCell<&'a mut LiteSVM>,
    genesis: RefCell<String>,
    /// Signed simulations (the one before every send).
    sims: Cell<usize>,
    /// Unsigned, signature-verification-off simulations (the probe).
    unsigned_sims: Cell<usize>,
    sends: Cell<usize>,
    landed: RefCell<std::collections::BTreeMap<Signature, u64>>,
    /// The next send LANDS but the caller is told the outcome is unknown (a lost response).
    lose_response: Cell<bool>,
    /// The status lookup finds NOTHING for a transaction that did land.
    hide_status: Cell<bool>,
    /// The status lookup ERRORS although the transaction landed.
    fail_status: Cell<bool>,
}

impl<'a> SvmCluster<'a> {
    fn new(svm: &'a mut LiteSVM) -> Self {
        SvmCluster {
            svm: RefCell::new(svm),
            genesis: RefCell::new(LITESVM_GENESIS.to_string()),
            sims: Cell::new(0),
            unsigned_sims: Cell::new(0),
            sends: Cell::new(0),
            landed: RefCell::new(std::collections::BTreeMap::new()),
            lose_response: Cell::new(false),
            hide_status: Cell::new(false),
            fail_status: Cell::new(false),
        }
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

    fn sims_and_sends(&self) -> (usize, usize, usize) {
        (self.sims.get(), self.unsigned_sims.get(), self.sends.get())
    }

    /// Send `instructions` signed by `payer` straight to the SVM, bypassing the tool and its counters (the test
    /// building a foreign transaction: a vault proposal, a hand-made config transaction).
    fn send_direct(&self, payer: &Keypair, instructions: &[Instruction]) -> Result<(), String> {
        let mut svm = self.svm.borrow_mut();
        svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(instructions, Some(&payer.pubkey()), &[payer], svm.latest_blockhash());
        match svm.send_transaction(tx) {
            Ok(_) => Ok(()),
            Err(failed) => Err(format!("{:?}\n{}", failed.err, failed.meta.pretty_logs())),
        }
    }
}

impl Cluster for SvmCluster<'_> {
    fn genesis_hash(&self) -> Result<String, String> {
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
        Ok(self.svm.borrow().get_sysvar::<Clock>().slot)
    }
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String> {
        // `sig_verify` false is the unsigned simulation of the RPC client API (sigVerify off): run it on a COPY
        // of the SVM with signature checking disabled, so it can never change this cluster either.
        let result = if sig_verify {
            self.sims.set(self.sims.get() + 1);
            self.svm.borrow().simulate_transaction(tx.clone())
        } else {
            self.unsigned_sims.set(self.unsigned_sims.get() + 1);
            let copy: LiteSVM = (**self.svm.borrow()).clone();
            copy.with_sigverify(false).simulate_transaction(tx.clone())
        };
        match result {
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
        let slot = self.svm.borrow().get_sysvar::<Clock>().slot;
        match self.svm.borrow_mut().send_transaction(tx.clone()) {
            Ok(meta) => {
                self.landed.borrow_mut().insert(meta.signature, slot);
                if self.lose_response.replace(false) {
                    return Err(SendError::Unknown("injected: the response was lost after the transaction landed".into()));
                }
                Ok(SendOutcome { signature: meta.signature, units_consumed: Some(meta.compute_units_consumed) })
            }
            Err(failed) => Err(SendError::Rejected(format!("{:?}\n{}", failed.err, failed.meta.pretty_logs()))),
        }
    }
    fn signature_outcome(&self, signature: &Signature) -> Result<SigOutcome, String> {
        if self.fail_status.get() {
            return Err("injected getSignatureStatuses failure".into());
        }
        if self.hide_status.get() {
            return Ok(SigOutcome::Unknown);
        }
        Ok(self
            .landed
            .borrow()
            .get(signature)
            .map_or(SigOutcome::Unknown, |slot| SigOutcome::Confirmed { slot: *slot }))
    }
    fn signature_slot(&self, signature: &Signature) -> Result<Option<u64>, String> {
        Ok(self.landed.borrow().get(signature).copied())
    }
}

/// Key files for one real-Squads world, written into a temp dir.
struct Files {
    /// Keeps the temp dir alive (removed on drop).
    _dir: TempDir,
    payer: String,
    g_create: String,
    p_create: String,
    g_members: String,
    p_members: String,
    g_member: Vec<String>,
    outsider: String,
    sponsor: String,
    management_pub: String,
    pending: String,
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
            outsider: dir.file("outsider.json"),
            sponsor: dir.file("sponsor.json"),
            management_pub: dir.file("management.pub"),
            pending: dir.file("pending.jsonl"),
            _dir: dir,
        };
        write_keypair(&files.payer, payer);
        write_keypair(&files.g_create, &real_g_create_keypair());
        write_keypair(&files.p_create, &real_p_create_keypair());
        for (path, member) in files.g_member.iter().zip(&world.g_members) {
            write_keypair(path, member);
        }
        write_keypair(&files.outsider, &Keypair::new_from_array([0x13; 32]));
        let g_keys: Vec<Pubkey> = world.g_members.iter().map(Signer::pubkey).collect();
        let p_keys: Vec<Pubkey> = world.p_members.iter().map(Signer::pubkey).collect();
        write_members(&files.g_members, &g_keys);
        write_members(&files.p_members, &p_keys);
        write_keypair(&files.sponsor, &world.rt.sponsor);
        write_pubkey(&files.management_pub, &world.rt.management.pubkey());
        files
    }

    /// Arguments of the pre-existing commands (`readback`, `squads-create`, `propose initialize`, ...): the
    /// cluster flags and BOTH multisigs are appended.
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

    /// `g-timelock <parts>` on a loopback cluster, with the shared write-ahead file.  G only: no P flags.
    fn tl(&self, cluster: &dyn Cluster, parts: &[&str]) -> Result<Value, OpsError> {
        let mut all = vec!["g-timelock"];
        all.extend_from_slice(parts);
        all.extend(["--rpc", RPC, "--pending-file", self.pending.as_str()]);
        self.tl_raw(cluster, &all)
    }

    /// `parts` verbatim (including `g-timelock`) plus ONLY the two G key flags.
    fn tl_raw(&self, cluster: &dyn Cluster, parts: &[&str]) -> Result<Value, OpsError> {
        let mut args: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
        for (flag, value) in [("--g-create-key", self.g_create.as_str()), ("--g-members", self.g_members.as_str())] {
            args.push(flag.into());
            args.push(value.into());
        }
        ops::run_with_cluster(&args, cluster)
    }

    fn m(&self, ordinal: usize) -> &str {
        self.g_member[ordinal].as_str()
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

fn usage_error(result: Result<Value, OpsError>) -> String {
    match result {
        Err(OpsError::Usage(message)) => message,
        other => panic!("expected a usage error, got {other:?}"),
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

fn check_names(receipt: &Value, field: &str) -> Vec<String> {
    receipt[field]
        .as_array()
        .map(|checks| checks.iter().map(|entry| entry["name"].as_str().unwrap_or("?").to_string()).collect())
        .unwrap_or_default()
}

fn pending_lines(path: &str) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every pending line is JSON"))
        .collect()
}

/// The ids of the steps that went through `Ctx::run_step` (write-ahead records), in order.
fn write_ahead_steps(path: &str) -> Vec<String> {
    pending_lines(path)
        .iter()
        .filter(|line| line["event"] == "write_ahead")
        .map(|line| line["step"].as_str().unwrap().to_string())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The harness world every test starts from: real Squads, devnet ORAO (network set up with the real devnet
/// treasury address), the program ELF under a ProgramData whose authority is the G vault.
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

/// Genesis through the real G multisig, driven by the tool exactly as the operator does: `squads-create`,
/// `initialize` (proposal #1) and the ORAO `register` (proposal #2), each proposed, approved twice, waited out
/// and executed.  Afterwards G has transaction_index 2, stale_transaction_index 0 and a 172,800 s time lock.
fn genesis(files: &Files, cluster: &SvmCluster<'_>) {
    ok(files.run(cluster, &["squads-create", "--payer", &files.payer, "--execute"]));
    let initialize = |parts: &[&str]| {
        let mut args = files.args(parts);
        args.extend(
            ["--sponsor", files.sponsor.as_str(), "--management-recipient", files.management_pub.as_str()]
                .iter()
                .map(|part| part.to_string()),
        );
        args
    };
    ok(ops::run_with_cluster(
        &initialize(&["propose", "initialize", "--member", files.m(0), "--execute", "--expect-tx-index", "1"]),
        cluster,
    ));
    for member in [0, 1] {
        ok(ops::run_with_cluster(
            &initialize(&["approve", "initialize", "--member", files.m(member), "--tx-index", "1", "--execute"]),
            cluster,
        ));
    }
    cluster.advance(TWO_DAYS);
    ok(ops::run_with_cluster(
        &initialize(&["execute", "initialize", "--member", files.m(0), "--tx-index", "1", "--execute"]),
        cluster,
    ));
    ok(files.run(cluster, &["fund-vault", "--payer", &files.payer, "--lamports", "20000000", "--execute"]));
    ok(files.run(cluster, &["propose", "register", "--member", files.m(0), "--execute", "--expect-tx-index", "2"]));
    for member in [0, 1] {
        ok(files.run(cluster, &["approve", "register", "--member", files.m(member), "--tx-index", "2", "--execute"]));
    }
    cluster.advance(TWO_DAYS);
    let registered = ok(files.run(cluster, &["execute", "register", "--member", files.m(0), "--tx-index", "2", "--execute"]));
    assert_eq!(registered["all_ok"], true, "{registered}");
}

fn g_identity() -> ops::Identity {
    ops::identity(real_g_create_keypair().pubkey())
}

fn member_keypair(ordinal: u8) -> Keypair {
    sq::member_keypair(0x61, ordinal)
}

fn read_multisig(cluster: &SvmCluster<'_>, g: &ops::Identity) -> sq::MultisigView {
    sq::parse_multisig(&cluster.account_of(&g.multisig).expect("G multisig").data)
}

fn proposal_status(cluster: &SvmCluster<'_>, g: &ops::Identity, index: u64) -> String {
    let account = cluster.account_of(&sq::proposal_address(&g.multisig, index)).expect("proposal account");
    ops::decode_proposal(&account.data).expect("proposal decodes").status.name().to_string()
}

/// A hand-made vault transaction `index` (a lamport transfer out of the G vault).  `proposal` is `None` for an
/// ORPHAN (the transaction with no proposal) or the approvals (member ordinals) of its proposal.  Returns the
/// instruction that would execute it.
fn foreign_vault(
    cluster: &SvmCluster<'_>,
    g: &ops::Identity,
    index: u64,
    destination: Pubkey,
    proposal: Option<&[u8]>,
) -> Instruction {
    let inner = solana_system_interface::instruction::transfer(&g.vault, &destination, 1_000);
    let compiled = sq::compile_vault_message(&inner);
    let proposer = member_keypair(0);
    cluster
        .send_direct(
            &proposer,
            &[sq::vault_transaction_create_ix(g.multisig, index, proposer.pubkey(), proposer.pubkey(), 0, 0, &compiled.bytes)],
        )
        .expect("vault_transaction_create");
    if let Some(approvers) = proposal {
        cluster
            .send_direct(&proposer, &[sq::proposal_create_ix(g.multisig, index, proposer.pubkey(), proposer.pubkey(), false)])
            .expect("proposal_create");
        for ordinal in approvers {
            let approver = member_keypair(*ordinal);
            cluster
                .send_direct(&approver, &[sq::proposal_approve_ix(g.multisig, index, approver.pubkey())])
                .expect("proposal_approve");
        }
    }
    sq::vault_transaction_execute_ix(g.multisig, index, proposer.pubkey(), compiled.remaining_accounts(&g.vault, &[]))
}

/// A hand-made vault transaction `index` with its proposal and the approvals of `approvers` (member ordinals).
fn foreign_vault_proposal(cluster: &SvmCluster<'_>, g: &ops::Identity, index: u64, destination: Pubkey, approvers: &[u8]) -> Instruction {
    foreign_vault(cluster, g, index, destination, Some(approvers))
}

/// A hand-made CONFIG transaction `index` storing `actions`, with its proposal (no votes).
fn foreign_config_transaction(cluster: &SvmCluster<'_>, g: &ops::Identity, index: u64, actions: &[ConfigAction], with_proposal: bool) {
    let proposer = member_keypair(0);
    cluster
        .send_direct(
            &proposer,
            &[timelock::config_transaction_create_ix(g.multisig, index, proposer.pubkey(), proposer.pubkey(), actions)],
        )
        .expect("config_transaction_create");
    if with_proposal {
        cluster
            .send_direct(&proposer, &[sq::proposal_create_ix(g.multisig, index, proposer.pubkey(), proposer.pubkey(), false)])
            .expect("proposal_create");
    }
}

// ---------------------------------------------------------------------------
// 1. Happy path: reduce, wait, execute, probe, restore at once, restore verified
// ---------------------------------------------------------------------------

#[test]
fn reduce_then_restore_through_the_real_squads_binary() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    let before_view = read_multisig(&cluster, &g);
    assert_eq!((before_view.time_lock, before_view.transaction_index, before_view.stale_transaction_index), (172_800, 2, 0));
    let readback_before = ok(files.run(
        &cluster,
        &["readback", "--sponsor", &files.sponsor, "--management-recipient", &files.management_pub],
    ));
    assert_eq!(readback_before["all_ok"], true, "{readback_before}");
    let readback_checks = readback_before["checks"].as_array().unwrap().len();

    // ---- S: the normal state, read-only --------------------------------------------------------
    let counters = cluster.sims_and_sends();
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["mode"], "read-only");
    assert_eq!(status["phase"], "normal", "{status}");
    assert_eq!(status["profile_matches_normal_172800"], true);
    assert_eq!(status["transaction_index"], 2);
    assert_eq!(status["stale_transaction_index"], 0);
    assert_eq!(status["scan"]["complete"], true);
    assert_eq!(status["all_ok"], true);
    assert_eq!(status["transactions"].as_array().unwrap().len(), 2);
    assert!(status["program_checks"].as_array().unwrap().iter().all(|c| c["ok"] == true), "{status}");

    // ---- the baseline probe: a simulation, never a send ---------------------------------------------
    let baseline = ok(files.tl(&cluster, &["probe", "--member", files.m(0)]));
    assert_eq!(baseline["mode"], "read-only");
    assert_eq!(baseline["simulated_only"], true);
    assert_eq!(baseline["sent"], false);
    assert_eq!(baseline["wrong_governance_profile"], false, "{baseline}");
    assert!(baseline["error_code"].is_null() && baseline["simulation_error"].is_null(), "{baseline}");
    assert_eq!(baseline["expected_wrong_governance_profile"], false);
    assert_eq!(baseline["g_vault_is_a_signer_flag_of_an_unsigned_message"], true);
    assert_eq!(baseline["all_ok"], true, "{baseline}");
    // status and probe together: zero signed simulations, zero sends, exactly ONE unsigned simulation.
    let after = cluster.sims_and_sends();
    assert_eq!((after.0, after.2), (counters.0, counters.2), "read-only commands neither simulate-for-send nor send");
    assert_eq!(after.1, counters.1 + 1, "the probe is one unsigned simulation");

    // ---- R1: propose reduce.  Dry run first: the index, the rent, nothing sent --------------------------
    let dry = ok(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0)]));
    assert_eq!(dry["mode"], "dry-run");
    assert_eq!(dry["transaction_index"], 3);
    assert_eq!(dry["action"], "reduce");
    assert_eq!(dry["resumed_proposal_only"], false);
    assert_eq!(dry["intended_actions"], json!([{ "variant": "SetTimeLock", "new_time_lock": 0 }]));
    assert_eq!(dry["steps"].as_array().unwrap().len(), 2);
    let create_step = &dry["steps"][0];
    assert_eq!(create_step["fee_payer"].as_str(), Some(files_member_key(&files, 0).as_str()));
    assert_eq!(create_step["required_signers"].as_array().unwrap().len(), 1, "proposer = creator = rent payer = fee payer");
    let create_bytes = create_step["packet_bytes"].as_u64().unwrap();
    eprintln!("FINDING config_transaction_create packet: {create_bytes} bytes");
    assert!(create_bytes <= 1_232);
    assert_eq!(dry["rent_lamports_estimate"]["config_transaction_bytes"], 90);
    assert!(dry["preconditions"].as_array().unwrap().iter().all(|c| c["ok"] == true), "{dry}");
    let names = check_names(&dry, "preconditions");
    for expected in [
        "pending.no_open_or_held_records",
        "g.no_live_transaction_above_stale_index",
        "programdata.upgrade_authority_is_g_vault",
        "pool.initialized_and_program_owned",
        "pool.config_version_1_active",
        "orao_client.owner_is_g_vault",
        "orao_client.state_is_rng_authority",
        "probe.baseline_succeeds_and_is_not_wrong_governance_profile",
    ] {
        assert!(names.contains(&expected.to_string()), "R0 is missing `{expected}`: {names:?}");
    }
    assert_eq!(cluster.sends.get(), counters.2, "a dry run sends nothing");

    // --execute needs the index the dry run printed.
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute"]));
    assert!(message.contains("--expect-tx-index 3"), "{message}");
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "9"]));
    assert!(message.contains("would take #3"), "{message}");
    assert_eq!(cluster.sends.get(), counters.2);

    let proposed = ok(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "3"]));
    assert_eq!(proposed["all_ok"], true, "{proposed}");
    assert_eq!(proposed["proposal_status"], "Active");
    assert_eq!(proposed["stored_actions"], json!([{ "variant": "SetTimeLock", "new_time_lock": 0 }]));
    assert_eq!(proposed["steps"].as_array().unwrap().len(), 2);
    assert_eq!(cluster.sends.get(), counters.2 + 2);
    assert_eq!(proposed["g_profile_before"]["transaction_index"], 2);
    assert_eq!(proposed["g_profile_after"]["transaction_index"], 3);
    assert_eq!(
        write_ahead_steps(&files.pending),
        ["F11.reduce.config_transaction_create", "F11.reduce.proposal_create"],
        "every write went through run_step's write-ahead, under the F11 step ids"
    );
    let first_record = &pending_lines(&files.pending)[0];
    assert_eq!(first_record["command"], "g-timelock");

    // The ConfigTransaction the REAL binary stored decodes with the strict decoder and re-encodes to its bytes.
    let config_address = sq::transaction_address(&g.multisig, 3);
    let config_account = cluster.account_of(&config_address).expect("config transaction account");
    assert_eq!(config_account.owner, SQUADS_PROGRAM_ID);
    let stored = timelock::decode_config_transaction(&config_account.data).expect("the real account decodes");
    assert_eq!(stored.multisig, g.multisig);
    assert_eq!(stored.creator, member_keypair(0).pubkey());
    assert_eq!(stored.index, 3);
    assert_eq!(stored.actions, Direction::Reduce.actions());
    let mut rebuilt = ops::account_discriminator("ConfigTransaction").to_vec();
    rebuilt.extend_from_slice(stored.multisig.as_ref());
    rebuilt.extend_from_slice(stored.creator.as_ref());
    rebuilt.extend_from_slice(&stored.index.to_le_bytes());
    rebuilt.push(stored.bump);
    rebuilt.extend_from_slice(&timelock::encode_actions(&stored.actions));
    assert_eq!(&config_account.data[..rebuilt.len()], rebuilt.as_slice(), "decode / re-encode round trip");
    assert_eq!(rebuilt.len(), timelock::config_transaction_bytes(&stored.actions));
    eprintln!(
        "FINDING config transaction account: {} bytes allocated, {} bytes of actions layout, {} bytes of zero padding",
        config_account.data.len(),
        rebuilt.len(),
        stored.padding_bytes
    );

    // status, and the duplicate guard.
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["phase"], "reduce_proposed", "{status}");
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "4"]));
    assert!(message.contains("identical") && message.contains("#3"), "{message}");

    // ---- R2: approve, twice -----------------------------------------------------------------------
    let first = ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert_eq!(first["all_ok"], true, "{first}");
    assert_eq!(first["after"]["status"], "Active");
    assert_eq!(first["after"]["approvals"], 1);
    let message = refused(files.tl(&cluster, &["approve", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert!(message.contains("already voted"), "{message}");
    let message = refused(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert!(message.contains("not Approved"), "{message}");
    let second = ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(1), "--tx-index", "3", "--execute"]));
    assert_eq!(second["all_ok"], true, "{second}");
    assert_eq!(second["after"]["status"], "Approved");
    let approved_at = second["approved_at"].as_i64().expect("approved_at after the second approval");
    assert_eq!(second["time_lock_releases_at"].as_i64(), Some(approved_at + TWO_DAYS));
    let message = refused(files.tl(&cluster, &["approve", "reduce", "--member", files.m(2), "--tx-index", "3", "--execute"]));
    assert!(message.contains("not Active"), "{message}");

    // ---- R3: the wait.  Execute is refused in plain words, with ZERO sends -------------------------------
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["phase"], "reduce_approved", "{status}");
    assert_eq!(status["earliest_execution"]["releases_at"].as_i64(), Some(approved_at + TWO_DAYS));
    assert_eq!(status["earliest_execution"]["executable_now"], false);
    let sends = cluster.sends.get();
    let sims = cluster.sims.get();
    let message = refused(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert!(message.contains("time lock not released") && message.contains("6021"), "{message}");
    cluster.advance(TWO_DAYS - 1);
    refused(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert_eq!((cluster.sends.get(), cluster.sims.get()), (sends, sims), "an early execute is refused before any simulation or send");
    assert_eq!(read_multisig(&cluster, &g).time_lock, 172_800, "nothing moved");
    // the restore is for a G at 0 s: refused while G is at 172,800 s.
    let message = refused(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert!(message.contains("nothing to do"), "{message}");
    cluster.advance(1);
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["earliest_execution"]["executable_now"], true, "{status}");

    // ---- R4: execute reduce --------------------------------------------------------------------------
    let dry_execute = ok(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3"]));
    assert_eq!(dry_execute["mode"], "dry-run");
    let execute_bytes = dry_execute["steps"][0]["packet_bytes"].as_u64().unwrap();
    eprintln!("FINDING config_transaction_execute packet: {execute_bytes} bytes");
    assert!(execute_bytes <= 1_232);
    assert_eq!(cluster.sends.get(), sends);
    let executed = ok(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert_eq!(executed["all_ok"], true, "{executed}");
    assert_eq!(cluster.sends.get(), sends + 1);
    // F11-R1: the execute-time guard passed and recorded its scan: the two genesis vault transactions, both Executed.
    let scanned = executed["other_g_transactions_checked"].as_array().expect("the scan is in the receipt");
    assert_eq!(scanned.len(), 2, "{executed}");
    for (entry, index) in scanned.iter().zip([1, 2]) {
        assert_eq!((&entry["index"], &entry["kind"], &entry["proposal_status"]), (&json!(index), &json!("vault"), &json!("Executed")));
    }
    assert!(
        check_names(&executed, "preconditions")
            .contains(&"g.executing_config_is_the_latest_and_every_other_transaction_above_stale_is_terminal".to_string()),
        "{executed}"
    );
    let names = check_names(&executed, "checks");
    for expected in [
        "proposal.is_executed",
        "multisig.time_lock_is_the_target",
        "multisig.threshold_unchanged",
        "multisig.members_and_permissions_unchanged",
        "multisig.config_authority_unchanged",
        "multisig.rent_collector_unchanged",
        "multisig.create_key_unchanged",
        "multisig.vault_0_unchanged",
        "multisig.stale_transaction_index_covers_the_executed_index",
        "multisig.stale_transaction_index_equals_transaction_index",
        "G.threshold",
    ] {
        assert!(names.contains(&expected.to_string()), "execute is missing the check `{expected}`: {names:?}");
    }
    let after_view = read_multisig(&cluster, &g);
    assert_eq!(after_view.time_lock, 0);
    assert_eq!((after_view.transaction_index, after_view.stale_transaction_index), (3, 3));
    assert_eq!(
        (after_view.threshold, after_view.config_authority, after_view.rent_collector, after_view.create_key, after_view.bump),
        (before_view.threshold, before_view.config_authority, before_view.rent_collector, before_view.create_key, before_view.bump)
    );
    let mut members_before = before_view.members.clone();
    let mut members_after = after_view.members.clone();
    members_before.sort();
    members_after.sort();
    assert_eq!(members_before, members_after);
    assert_eq!(proposal_status(&cluster, &g, 3), "Executed");
    for field in ["threshold", "create_key", "config_authority", "rent_collector", "members", "vault_0", "bump"] {
        assert_eq!(executed["g_profile_before"][field], executed["g_profile_after"][field], "{field} is unchanged");
    }
    assert_eq!(executed["g_profile_before"]["time_lock_seconds"], 172_800);
    assert_eq!(executed["g_profile_after"]["time_lock_seconds"], 0);
    // the proposal cannot run twice.
    let message = refused(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert!(message.contains("already 0 s"), "{message}");

    // ---- R5: the reduced state, documented -----------------------------------------------------------
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["phase"], "reduced", "{status}");
    assert_eq!(status["profile_matches_reduced_0"], true);
    assert_eq!(status["profile_matches_normal_172800"], false);
    assert_eq!(status["all_ok"], true);
    let squads_readback = ok(files.run(&cluster, &["readback", "--sections", "squads"]));
    assert_eq!(squads_readback["all_ok"], false);
    assert_eq!(failing_checks(&squads_readback), ["G.time_lock_seconds"], "exactly one failed check; P is unchanged: {squads_readback}");

    let probe_before = cluster.sims_and_sends();
    let reduced_probe = ok(files.tl(&cluster, &["probe", "--member", files.m(0)]));
    assert_eq!(reduced_probe["simulated_only"], true);
    assert_eq!(reduced_probe["wrong_governance_profile"], true, "{reduced_probe}");
    assert_eq!(reduced_probe["error_code"], 6031);
    assert_eq!(reduced_probe["error_name"], "WrongGovernanceProfile");
    assert_eq!(reduced_probe["expected"], "WrongGovernanceProfile");
    assert_eq!(reduced_probe["expected_wrong_governance_profile"], true);
    assert_eq!(reduced_probe["g_time_lock_seconds"], 0);
    assert_eq!(reduced_probe["all_ok"], true, "{reduced_probe}");
    assert_eq!(reduced_probe["instruction_accounts"][1]["signer"], true, "the G vault is a signer flag");
    assert_eq!(reduced_probe["instruction_accounts"][1]["key"].as_str(), Some(g.vault.to_string().as_str()));
    assert_eq!(
        (cluster.sims.get(), cluster.sends.get()),
        (probe_before.0, probe_before.2),
        "the probe sent and signed-simulated nothing"
    );
    // reduce while at 0 s: refused.
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0)]));
    assert!(message.contains("already 0 s") && message.contains("nothing to do"), "{message}");

    // ---- S1..S3: restore, with NO wait ------------------------------------------------------------------
    let dry = ok(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert_eq!(dry["transaction_index"], 4);
    assert_eq!(dry["intended_actions"], json!([{ "variant": "SetTimeLock", "new_time_lock": 172_800 }]));
    let names = check_names(&dry, "preconditions");
    assert!(
        !names.iter().any(|name| name.starts_with("probe.") || name.starts_with("orao_client.") || name.starts_with("pool.")),
        "restore stands on none of the program-side state: {names:?}"
    );
    assert!(names.contains(&"g.no_live_transaction_above_stale_index".to_string()));
    let proposed = ok(files.tl(&cluster, &["propose", "restore", "--member", files.m(0), "--execute", "--expect-tx-index", "4"]));
    assert_eq!(proposed["all_ok"], true, "{proposed}");
    assert_eq!(proposed["stored_actions"], json!([{ "variant": "SetTimeLock", "new_time_lock": 172_800 }]));
    assert_eq!(ok(files.tl(&cluster, &["status"]))["phase"], "restore_proposed");
    for member in [0, 1] {
        let approved = ok(files.tl(&cluster, &["approve", "restore", "--member", files.m(member), "--tx-index", "4", "--execute"]));
        assert_eq!(approved["all_ok"], true, "{approved}");
    }
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["phase"], "restore_approved", "{status}");
    assert_eq!(status["earliest_execution"]["executable_now"], true, "restore is executable at once");
    let restore_sends = cluster.sends.get();
    let restored = ok(files.tl(&cluster, &["execute", "restore", "--member", files.m(0), "--tx-index", "4", "--execute"]));
    assert_eq!(restored["all_ok"], true, "{restored}");
    assert_eq!(cluster.sends.get(), restore_sends + 1, "no warp, one send");
    assert_eq!(restored["approved_at"], restored["time_lock_releases_at"], "released at once: the lock was 0 s");
    // F11-R1: the restore is the only transaction above the stale index (#3), so there is nothing else to scan.
    assert_eq!(restored["other_g_transactions_checked"], json!([]), "{restored}");

    // ---- S4: the final read-back --------------------------------------------------------------------------
    let final_view = read_multisig(&cluster, &g);
    assert_eq!(final_view.time_lock, 172_800);
    assert_eq!((final_view.transaction_index, final_view.stale_transaction_index), (4, 4));
    assert_eq!(final_view.threshold, before_view.threshold);
    assert_eq!(final_view.config_authority, before_view.config_authority);
    assert_eq!(final_view.rent_collector, before_view.rent_collector);
    let mut members_final = final_view.members.clone();
    members_final.sort();
    assert_eq!(members_final, members_before);
    assert_eq!(proposal_status(&cluster, &g, 4), "Executed");
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["phase"], "restored", "{status}");
    assert_eq!(status["profile_matches_normal_172800"], true);
    let clean_probe = ok(files.tl(&cluster, &["probe", "--member", files.m(0)]));
    assert_eq!(clean_probe["wrong_governance_profile"], false, "{clean_probe}");
    assert_eq!(clean_probe["all_ok"], true);
    let squads_readback = ok(files.run(&cluster, &["readback", "--sections", "squads"]));
    assert_eq!(squads_readback["all_ok"], true, "{squads_readback}");
    let readback_after = ok(files.run(
        &cluster,
        &["readback", "--sponsor", &files.sponsor, "--management-recipient", &files.management_pub],
    ));
    assert_eq!(readback_after["all_ok"], true, "{readback_after}");
    assert_eq!(readback_after["checks"].as_array().unwrap().len(), readback_checks, "the same read-back as before F11");
    // restore while at 172,800 s: refused.
    let message = refused(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert!(message.contains("already 172800 s") && message.contains("nothing to do"), "{message}");

    // The whole sitting, as the write-ahead file recorded it: eight sends under the F11 step ids.
    assert_eq!(
        write_ahead_steps(&files.pending),
        [
            "F11.reduce.config_transaction_create",
            "F11.reduce.proposal_create",
            "F11.reduce.proposal_approve",
            "F11.reduce.proposal_approve",
            "F11.reduce.config_transaction_execute",
            "F11.restore.config_transaction_create",
            "F11.restore.proposal_create",
            "F11.restore.proposal_approve",
            "F11.restore.proposal_approve",
            "F11.restore.config_transaction_execute",
        ]
    );
    let lines = pending_lines(&files.pending);
    assert!(lines.iter().filter(|line| line["event"] == "resolved").all(|line| line["outcome"] == "confirmed"));
    assert_eq!(lines.iter().filter(|line| line["event"] == "resolved").count(), 10);
}

fn files_member_key(files: &Files, ordinal: usize) -> String {
    let bytes: Vec<u8> = serde_json::from_str(&fs::read_to_string(&files.g_member[ordinal]).unwrap()).unwrap();
    Keypair::try_from(&bytes[..]).unwrap().pubkey().to_string()
}

// ---------------------------------------------------------------------------
// 2. The real consequence: a G vault transaction fails WrongGovernanceProfile while G is reduced
// ---------------------------------------------------------------------------

#[test]
fn a_real_g_vault_transaction_fails_with_wrong_governance_profile_while_reduced_and_succeeds_before() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let management = world.rt.management.pubkey();
    {
        let cluster = SvmCluster::new(&mut world.rt.svm);
        genesis(&files, &cluster);
    }

    // BEFORE the reduce: the very instruction the probe simulates succeeds as a real G vault transaction.
    let noop = timelock::noop_rotate_recipient_ix(&g, management);
    assert!(noop.accounts.iter().any(|meta| meta.pubkey == g.vault && meta.is_signer), "the vault is a signer");
    let before = world.run_g("g_timelock.rotate_recipient_noop.before", noop.clone(), &[]);
    assert!(before.succeeded(), "{}", sq::summarize_run(&before));
    assert_eq!(before.transaction_index, 3);

    // The reduce, through the tool: #4.
    let cluster = SvmCluster::new(&mut world.rt.svm);
    ok(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "4"]));
    for member in [0, 1] {
        ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(member), "--tx-index", "4", "--execute"]));
    }
    cluster.advance(TWO_DAYS);
    let executed = ok(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "4", "--execute"]));
    assert_eq!(executed["all_ok"], true, "{executed}");
    let probe = ok(files.tl(&cluster, &["probe", "--member", files.m(0)]));
    assert_eq!(probe["error_code"], 6031, "the probe predicts the real failure: {probe}");
    drop(cluster);

    // AFTER the reduce: the same instruction, as a real G vault transaction, is refused by the program.
    let after = world.run_g("g_timelock.rotate_recipient_noop.after", noop, &[]);
    assert_eq!(after.transaction_index, 5);
    let (stage, observation) = after.first_failure().expect("the vault transaction must fail while G is reduced");
    assert_eq!(stage, "vault_transaction_execute");
    assert_eq!(
        custom_code(observation),
        Some(timelock::WRONG_GOVERNANCE_PROFILE_CODE),
        "WrongGovernanceProfile (6031) from inside the Squads CPI: {observation:?}"
    );
    // With a lock of 0 s the early probe is no longer early: it fails the same way.
    let early = after.early_execute.as_ref().expect("the early execute ran");
    assert_eq!(custom_code(early), Some(timelock::WRONG_GOVERNANCE_PROFILE_CODE));
    sq::record_finding(
        "g_timelock_reduced_g_refuses_its_own_governance",
        &json!({
            "before_reduce": "rotate_recipient(no-op) succeeded as a G vault transaction",
            "after_reduce": "rotate_recipient(no-op) failed with WrongGovernanceProfile (6031) inside vault_transaction_execute",
        }),
    );

    // The failed vault transaction #5 stays Approved (live): the tool will not stack a restore on it.
    let cluster = SvmCluster::new(&mut world.rt.svm);
    let message = refused(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert!(message.contains("g.no_live_transaction_above_stale_index"), "{message}");
    assert_eq!(read_multisig(&cluster, &g).time_lock, 0, "G stays reduced; no restore was attempted");
}

// ---------------------------------------------------------------------------
// 3. Refusals
// ---------------------------------------------------------------------------

#[test]
fn usage_errors_non_members_and_wrong_state_refuse_with_zero_sends() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    let counters = cluster.sims_and_sends();
    let member = files.m(0);

    // Unknown actions and subcommands.
    for parts in [
        vec!["propose", "raise", "--member", member],
        vec!["propose", "--member", member],
        vec!["propose", "reduce", "restore", "--member", member],
        vec!["approve", "set", "--member", member, "--tx-index", "3"],
        vec!["execute", "0", "--member", member, "--tx-index", "3"],
        vec!["frobnicate"],
        vec!["status", "reduce"],
        vec!["probe", "restore", "--member", member],
    ] {
        usage_error(files.tl(&cluster, &parts));
    }
    usage_error(files.tl_raw(&cluster, &["g-timelock", "--rpc", RPC]));

    // No flag can name a value, a threshold, a member, a memo, a spending limit, a rent collector or the P multisig.
    for flag in [
        vec!["--time-lock", "5"],
        vec!["--new-time-lock", "172800"],
        vec!["--value", "1"],
        vec!["--threshold", "3"],
        vec!["--add-member", "x"],
        vec!["--memo", "hello"],
        vec!["--spending-limit", "x"],
        vec!["--rent-collector", "x"],
        vec!["--p-create-key", files.p_create.as_str()],
        vec!["--p-members", files.p_members.as_str()],
        vec!["--lamports", "5"],
        vec!["--sponsor", files.sponsor.as_str()],
        vec!["--payer", files.payer.as_str()],
    ] {
        let mut parts = vec!["propose", "reduce", "--member", member];
        parts.extend(flag.iter().copied());
        let message = usage_error(files.tl(&cluster, &parts));
        assert!(message.contains(flag[0].trim_start_matches("--")), "{message}");
    }
    // The index flags belong to their own subcommands.
    assert!(usage_error(files.tl(&cluster, &["propose", "reduce", "--member", member, "--tx-index", "3"])).contains("--tx-index"));
    assert!(usage_error(files.tl(&cluster, &["approve", "reduce", "--member", member, "--tx-index", "3", "--expect-tx-index", "3"])).contains("--expect-tx-index"));
    assert!(usage_error(files.tl(&cluster, &["status", "--tx-index", "3"])).contains("--tx-index"));
    assert!(matches!(files.tl(&cluster, &["approve", "reduce", "--member", member, "--execute"]), Err(OpsError::Usage(_))), "approve needs --tx-index");

    // status and probe are read-only: --execute is a usage error, before any pin or send.
    for parts in [vec!["status", "--execute"], vec!["probe", "--member", member, "--execute"]] {
        let message = usage_error(files.tl(&cluster, &parts));
        assert!(message.contains("read-only") && message.contains("--execute"), "{message}");
    }
    assert!(matches!(files.tl(&cluster, &["probe"]), Err(OpsError::Usage(_))), "the probe needs --member (the simulated fee payer)");
    assert_eq!(cluster.sims_and_sends(), counters, "usage errors never reach the cluster's send path");

    // A key that is not a G member, for each stage.
    for parts in [
        vec!["propose", "reduce", "--member", files.outsider.as_str()],
        vec!["approve", "reduce", "--member", files.outsider.as_str(), "--tx-index", "3"],
        vec!["execute", "reduce", "--member", files.outsider.as_str(), "--tx-index", "3"],
    ] {
        let message = refused(files.tl(&cluster, &parts));
        assert!(message.contains("is not a G member with the"), "{message}");
    }

    // The state says no: restore while G is at 172,800 s.
    for parts in [
        vec!["propose", "restore", "--member", member],
        vec!["approve", "restore", "--member", member, "--tx-index", "3"],
        vec!["execute", "restore", "--member", member, "--tx-index", "3"],
    ] {
        let message = refused(files.tl(&cluster, &parts));
        assert!(message.contains("already 172800 s") && message.contains("nothing to do"), "{message}");
    }
    // A transaction that does not exist.
    let message = refused(files.tl(&cluster, &["approve", "reduce", "--member", member, "--tx-index", "99", "--execute"]));
    assert!(message.contains("no transaction #99"), "{message}");
    // The dry run prints the index and sends nothing; --execute needs --expect-tx-index and the right one.
    let dry = ok(files.tl(&cluster, &["propose", "reduce", "--member", member]));
    assert_eq!(dry["transaction_index"], 3);
    assert!(refused(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute"])).contains("--expect-tx-index 3"));
    assert!(refused(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute", "--expect-tx-index", "2"])).contains("would take #3"));
    assert_eq!(cluster.sends.get(), counters.2, "every refusal and dry run above sent nothing");
    assert_eq!(read_multisig(&cluster, &g_identity()).transaction_index, 2, "no transaction was created");
}

#[test]
fn a_stored_action_that_differs_is_refused_by_approve_and_execute_and_live_transactions_block_propose() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    let member = files.m(0);
    let outsider_destination = Pubkey::new_from_array([0xDE; 32]);

    // A live VAULT proposal above the stale index: propose reduce refuses (dry run too).
    foreign_vault_proposal(&cluster, &g, 3, outsider_destination, &[]);
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", member]));
    assert!(message.contains("g.no_live_transaction_above_stale_index"), "{message}");
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute", "--expect-tx-index", "4"]));
    assert!(message.contains("g.no_live_transaction_above_stale_index"), "{message}");
    // ... and `status` lists it.
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["transactions"][2]["kind"], "vault");
    assert_eq!(status["transactions"][2]["proposal"]["status"], "Active");
    assert_eq!(status["phase"], "normal");

    // Config transactions with the WRONG stored actions, each with an Active proposal.
    let wrong: [(u64, Vec<ConfigAction>); 4] = [
        (4, vec![ConfigAction::ChangeThreshold { new_threshold: 2 }]),
        (5, vec![ConfigAction::SetTimeLock { new_time_lock: 1 }]),
        (6, vec![ConfigAction::SetTimeLock { new_time_lock: 0 }, ConfigAction::ChangeThreshold { new_threshold: 2 }]),
        (7, Direction::Restore.actions()),
    ];
    for (index, actions) in &wrong {
        foreign_config_transaction(&cluster, &g, *index, actions, true);
    }
    let sends = cluster.sims_and_sends();
    for (index, _) in &wrong {
        let index = index.to_string();
        for verb in ["approve", "execute"] {
            let message = refused(files.tl(&cluster, &[verb, "reduce", "--member", member, "--tx-index", &index, "--execute"]));
            assert!(message.contains("NOT exactly the intended"), "{verb} #{index}: {message}");
        }
    }
    // The vault transaction is not a config transaction at all.
    let message = refused(files.tl(&cluster, &["approve", "reduce", "--member", member, "--tx-index", "3", "--execute"]));
    assert!(message.contains("vault transaction, not a config transaction"), "{message}");
    assert_eq!(cluster.sims_and_sends(), sends, "no vote and no execution on anything but the exact intended action");
    let view = read_multisig(&cluster, &g);
    assert_eq!((view.time_lock, view.threshold, view.stale_transaction_index), (172_800, 2, 0));
    // The wrong transactions are still Active: nothing was voted on.
    for index in 4..=7 {
        assert_eq!(proposal_status(&cluster, &g, index), "Active");
    }
}

#[test]
fn propose_resumes_a_config_transaction_without_a_proposal_and_refuses_an_identical_live_one() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    let member = files.m(0);

    // create landed, proposal_create did not (a half-made proposal).
    foreign_config_transaction(&cluster, &g, 3, &Direction::Reduce.actions(), false);
    let dry = ok(files.tl(&cluster, &["propose", "reduce", "--member", member]));
    assert_eq!(dry["transaction_index"], 3);
    assert_eq!(dry["resumed_proposal_only"], true, "{dry}");
    assert_eq!(dry["steps"].as_array().unwrap().len(), 1);
    assert_eq!(dry["steps"][0]["step"], "F11.reduce.proposal_create");
    assert!(refused(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute", "--expect-tx-index", "4"])).contains("would take #3"));
    let resumed = ok(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute", "--expect-tx-index", "3"]));
    assert_eq!(resumed["all_ok"], true, "{resumed}");
    assert_eq!(resumed["resumed_proposal_only"], true);
    assert_eq!(resumed["proposal_status"], "Active");
    assert_eq!(write_ahead_steps(&files.pending), ["F11.reduce.proposal_create"], "only proposal_create is re-sent");
    let message = refused(files.tl(&cluster, &["propose", "reduce", "--member", member, "--execute", "--expect-tx-index", "4"]));
    assert!(message.contains("identical") && message.contains("#3"), "{message}");
}

// ---------------------------------------------------------------------------
// 4. F3: an ambiguous execute that landed
// ---------------------------------------------------------------------------

#[test]
fn an_execute_whose_response_is_lost_is_held_never_rebuilt_and_resolves_from_history() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    ok(files.tl(&cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "3"]));
    for member in [0, 1] {
        ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(member), "--tx-index", "3", "--execute"]));
    }
    cluster.advance(TWO_DAYS);

    // The execute LANDS, its response is lost, the blockhash is dead and no status can be found: HELD.
    let sends = cluster.sends.get();
    cluster.lose_response.set(true);
    cluster.hide_status.set(true);
    let error = files
        .tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"])
        .expect_err("an ambiguous send is an incident");
    let message = match error {
        OpsError::Failed(message) => message,
        other => panic!("expected the INCIDENT failure, got {other:?}"),
    };
    assert!(message.contains("INCIDENT") && message.contains("HELD") && message.contains("do NOT rebuild"), "{message}");
    assert_eq!(cluster.sends.get(), sends + 1, "exactly the one send");
    assert_eq!(read_multisig(&cluster, &g).time_lock, 0, "the transaction did land");
    let lines = pending_lines(&files.pending);
    let record = lines.iter().rev().find(|line| line["event"] == "write_ahead").unwrap();
    assert_eq!(record["step"], "F11.reduce.config_transaction_execute");
    let signature = record["signature"].as_str().unwrap().to_string();
    assert!(lines.iter().any(|line| line["event"] == "held_unknown" && line["signature"] == signature.as_str()));
    assert!(
        !lines.iter().any(|line| line["event"] == "resolved" && line["signature"] == signature.as_str()),
        "the record stays open"
    );

    // Any later --execute refuses (a missing status, then an erroring one) and builds / sends nothing.
    for erroring in [false, true] {
        cluster.hide_status.set(!erroring);
        cluster.fail_status.set(erroring);
        for parts in [
            vec!["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"],
            vec!["propose", "restore", "--member", files.m(0), "--execute", "--expect-tx-index", "4"],
        ] {
            let message = refused(files.tl(&cluster, &parts));
            assert!(message.contains("HELD UNKNOWN") && message.contains(&signature) && message.contains("NO command to clear"), "{message}");
            assert!(!message.contains("never landed"), "{message}");
        }
        assert_eq!(cluster.sends.get(), sends + 1, "zero further sends");
    }
    // Read-only commands still run and list the open record ...
    let status = ok(files.tl(&cluster, &["status"]));
    assert_eq!(status["open_pending_records"][0]["held"], true, "{status}");
    assert_eq!(status["phase"], "reduced");
    assert_eq!(status["pending"]["ok"], false, "the pending check shows the open record: {status}");
    // ... and even a DRY-RUN propose refuses (R0 / S0 need 0 open or held records).
    let message = refused(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert!(message.contains("pending.no_open_or_held_records"), "{message}");
    assert_eq!(cluster.sends.get(), sends + 1);

    // History later finds the transaction CONFIRMED: the next --execute resolves it (and rebuilds nothing).
    cluster.hide_status.set(false);
    cluster.fail_status.set(false);
    let message = refused(files.tl(&cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert!(message.contains("already 0 s"), "the landed execute is not repeated: {message}");
    assert_eq!(cluster.sends.get(), sends + 1, "nothing is rebuilt");
    let lines = pending_lines(&files.pending);
    let resolution = lines.last().unwrap();
    assert_eq!((resolution["event"].as_str(), resolution["outcome"].as_str()), (Some("resolved"), Some("confirmed")), "{lines:?}");
    assert_eq!(resolution["signature"], signature.as_str());
    assert!(resolution["slot"].as_u64().is_some());
    let status = ok(files.tl(&cluster, &["status"]));
    assert!(status.get("open_pending_records").is_none(), "no open record remains: {status}");
    // The restore is now free to be proposed (dry run).
    let dry = ok(files.tl(&cluster, &["propose", "restore", "--member", files.m(0)]));
    assert_eq!(dry["transaction_index"], 4);
}

// ---------------------------------------------------------------------------
// 5. Encoding
// ---------------------------------------------------------------------------

#[test]
fn the_config_transaction_instructions_match_the_committed_idl_and_the_borsh_layout() {
    let idl = sq::idl();
    let multisig = Pubkey::new_from_array([0x11; 32]);
    let creator = Pubkey::new_from_array([0x22; 32]);
    let rent_payer = Pubkey::new_from_array([0x33; 32]);

    // config_transaction_create: every account required, so the generic IDL cross-check applies unchanged.
    let create = timelock::config_transaction_create_ix(multisig, 3, creator, rent_payer, &Direction::Reduce.actions());
    sq::assert_matches_idl(&idl, "configTransactionCreate", &create);
    assert_eq!(create.accounts.len(), 5);
    assert_eq!(
        create.accounts.iter().map(|meta| meta.pubkey).collect::<Vec<_>>(),
        [multisig, sq::transaction_address(&multisig, 3), creator, rent_payer, solana_system_interface::program::ID],
        "multisig, transaction, creator, rentPayer, systemProgram (the IDL's order)"
    );
    assert_eq!(
        sq::idl_accounts(&idl, "configTransactionCreate").iter().map(|(name, _, _)| name.as_str()).collect::<Vec<_>>(),
        ["multisig", "transaction", "creator", "rentPayer", "systemProgram"]
    );

    // config_transaction_execute: the last two accounts are OPTIONAL in the IDL.  Anchor 0.29 spells an unused
    // optional account as the PROGRAM ID in its slot (read-only, not a signer), so the IDL's flags for those two
    // slots (writable / signer) differ on purpose: assert exactly that rule.
    let execute = timelock::config_transaction_execute_ix(multisig, 3, creator);
    assert_matches_idl_with_optionals(&idl, "configTransactionExecute", &execute);
    assert_eq!(execute.accounts.len(), 6);
    assert_eq!(execute.accounts[0].pubkey, multisig);
    assert_eq!(execute.accounts[1].pubkey, creator);
    assert_eq!(execute.accounts[2].pubkey, sq::proposal_address(&multisig, 3));
    assert_eq!(execute.accounts[3].pubkey, sq::transaction_address(&multisig, 3));
    assert_eq!(
        (execute.accounts[4].pubkey, execute.accounts[5].pubkey),
        (SQUADS_PROGRAM_ID, SQUADS_PROGRAM_ID),
        "rentPayer and systemProgram are None"
    );

    // Discriminators (sha256("global:<name>")[..8], pinned as literals) and the exact Borsh bytes.
    assert_eq!(hex(&create.data[..8]), "9bec57e4894b5127");
    assert_eq!(hex(&execute.data), "7292f4bdfc8c2428", "config_transaction_execute carries no arguments");
    assert_eq!(hex(&ops::account_discriminator("ConfigTransaction")), "5e080423718b8b70");
    // ConfigTransactionCreateArgs { actions: [SetTimeLock { new_time_lock: 0 }], memo: None }
    assert_eq!(hex(&create.data), "9bec57e4894b5127" /* disc */.to_string() + "01000000" /* 1 action */ + "03" /* SetTimeLock */ + "00000000" /* 0 s */ + "00" /* memo None */);
    let restore = timelock::config_transaction_create_ix(multisig, 4, creator, rent_payer, &Direction::Restore.actions());
    assert_eq!(hex(&restore.data), "9bec57e4894b5127".to_string() + "01000000" + "03" + "00a30200" /* 172800 LE */ + "00");
    assert_eq!(hex(&timelock::encode_actions(&Direction::Reduce.actions())), "010000000300000000");
    assert_eq!(hex(&timelock::encode_actions(&Direction::Restore.actions())), "0100000003".to_string() + "00a30200");
    assert_eq!(timelock::config_transaction_bytes(&Direction::Reduce.actions()), 90);
    assert_eq!((timelock::T_NORMAL, timelock::T_REDUCED), (172_800, 0), "the two values are code constants");
    assert_eq!((Direction::Reduce.from_lock(), Direction::Reduce.target_lock()), (172_800, 0));
    assert_eq!((Direction::Restore.from_lock(), Direction::Restore.target_lock()), (0, 172_800));
    assert_eq!(Direction::Reduce.step_id("proposal_approve"), "F11.reduce.proposal_approve");

    // The other variants (never built by the tool; decoded so they can be refused).
    let key = Pubkey::new_from_array([0x44; 32]);
    let every_variant = vec![
        ConfigAction::AddMember { key, permissions: 7 },
        ConfigAction::RemoveMember { key },
        ConfigAction::ChangeThreshold { new_threshold: 2 },
        ConfigAction::SetTimeLock { new_time_lock: 1 },
        ConfigAction::AddSpendingLimit {
            create_key: key,
            vault_index: 0,
            mint: Pubkey::default(),
            amount: 5,
            period: 2,
            members: vec![key, creator],
            destinations: vec![],
        },
        ConfigAction::RemoveSpendingLimit { spending_limit: key },
        ConfigAction::SetRentCollector { new_rent_collector: None },
        ConfigAction::SetRentCollector { new_rent_collector: Some(key) },
    ];
    assert_eq!(hex(&every_variant[2].encoded()), "020200");
    assert_eq!(hex(&every_variant[3].encoded()), "0301000000");
    assert_eq!(every_variant[0].encoded()[0], 0);
    assert_eq!(every_variant[5].encoded()[0], 5);
    assert_eq!(every_variant[6].encoded(), [6, 0]);
    let account = config_transaction_account(multisig, creator, 9, 254, &every_variant, 11);
    let view = timelock::decode_config_transaction(&account).expect("every variant round-trips");
    assert_eq!(view.actions, every_variant);
    assert_eq!((view.multisig, view.creator, view.index, view.bump, view.padding_bytes), (multisig, creator, 9, 254, 11));
}

/// Like `sq::assert_matches_idl`, except that an account the IDL marks `isOptional` must be the program-id
/// placeholder (read-only, not a signer): the Anchor 0.29 spelling of `None`.
fn assert_matches_idl_with_optionals(idl: &Value, camel: &str, ix: &Instruction) {
    let instruction = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["name"].as_str() == Some(camel))
        .unwrap_or_else(|| panic!("IDL has no instruction {camel}"));
    let accounts = instruction["accounts"].as_array().unwrap();
    assert_eq!(ix.accounts.len(), accounts.len(), "{camel}: account count");
    let mut optional_positions = Vec::new();
    for (position, account) in accounts.iter().enumerate() {
        let meta = &ix.accounts[position];
        let name = account["name"].as_str().unwrap();
        let (want_writable, want_signer) = if account["isOptional"].as_bool() == Some(true) {
            optional_positions.push(position);
            assert_eq!(meta.pubkey, SQUADS_PROGRAM_ID, "{camel}: optional account #{position} `{name}` is None (the program id)");
            (false, false)
        } else {
            (account["isMut"].as_bool().unwrap_or(false), account["isSigner"].as_bool().unwrap_or(false))
        };
        assert_eq!((meta.is_writable, meta.is_signer), (want_writable, want_signer), "{camel}: account #{position} `{name}` flags");
    }
    assert_eq!(optional_positions, [4, 5], "{camel}: exactly rentPayer and systemProgram are optional");
    assert_eq!(ix.data[..8], sq::instruction_discriminator(&sq::snake_case(camel)), "{camel}: discriminator");
    assert_eq!(ix.program_id, SQUADS_PROGRAM_ID);
}

/// Hand-built `ConfigTransaction` account bytes (`padding` zero bytes after the actions).
fn config_transaction_account(multisig: Pubkey, creator: Pubkey, index: u64, bump: u8, actions: &[ConfigAction], padding: usize) -> Vec<u8> {
    let mut data = ops::account_discriminator("ConfigTransaction").to_vec();
    data.extend_from_slice(multisig.as_ref());
    data.extend_from_slice(creator.as_ref());
    data.extend_from_slice(&index.to_le_bytes());
    data.push(bump);
    data.extend_from_slice(&timelock::encode_actions(actions));
    data.extend(std::iter::repeat_n(0u8, padding));
    data
}

#[test]
fn the_config_transaction_decoder_is_strict() {
    let multisig = Pubkey::new_from_array([0x11; 32]);
    let creator = Pubkey::new_from_array([0x22; 32]);
    let actions = Direction::Reduce.actions();
    let good = config_transaction_account(multisig, creator, 3, 255, &actions, 0);
    assert!(timelock::decode_config_transaction(&good).is_ok());
    assert!(timelock::decode_config_transaction(&config_transaction_account(multisig, creator, 3, 255, &actions, 7)).is_ok(), "zero padding is allocation slack");

    // The wrong account type, truncation, trailing garbage, an unknown variant, an impossible action count.
    let mut wrong_discriminator = good.clone();
    wrong_discriminator[0] ^= 0xFF;
    assert!(timelock::decode_config_transaction(&wrong_discriminator).unwrap_err().contains("discriminator"));
    assert!(timelock::decode_config_transaction(&[]).is_err());
    for cut in [8, 40, 72, 80, 81, 85, 88, good.len() - 1] {
        assert!(timelock::decode_config_transaction(&good[..cut]).is_err(), "truncated at {cut}");
    }
    let mut garbage = good.clone();
    garbage.extend_from_slice(&[0, 0, 1]);
    assert!(timelock::decode_config_transaction(&garbage).unwrap_err().contains("non-zero"));
    let mut unknown_variant = good.clone();
    unknown_variant[8 + 32 + 32 + 8 + 1 + 4] = 9;
    assert!(timelock::decode_config_transaction(&unknown_variant).unwrap_err().contains("unknown ConfigAction"));
    let mut huge_count = good.clone();
    huge_count[8 + 32 + 32 + 8 + 1..8 + 32 + 32 + 8 + 1 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(timelock::decode_config_transaction(&huge_count).is_err());
    let mut bad_period = config_transaction_account(
        multisig,
        creator,
        3,
        255,
        &[ConfigAction::AddSpendingLimit {
            create_key: multisig,
            vault_index: 0,
            mint: creator,
            amount: 1,
            period: 0,
            members: vec![],
            destinations: vec![],
        }],
        0,
    );
    let period_at = 8 + 32 + 32 + 8 + 1 + 4 + 1 + 32 + 1 + 32 + 8;
    bad_period[period_at] = 4;
    assert!(timelock::decode_config_transaction(&bad_period).unwrap_err().contains("Period"));

    // Exactly one SetTimeLock{target} is the only thing the tool accepts.
    assert!(timelock::is_exactly_set_time_lock(&actions, 0));
    assert!(!timelock::is_exactly_set_time_lock(&actions, 172_800));
    assert!(!timelock::is_exactly_set_time_lock(&[], 0));
    assert!(!timelock::is_exactly_set_time_lock(&[actions[0].clone(), actions[0].clone()], 0));
    assert!(!timelock::is_exactly_set_time_lock(&[ConfigAction::ChangeThreshold { new_threshold: 0 }], 0));
}

#[test]
fn the_probe_reads_a_program_error_in_both_spellings_and_names_it_from_the_anchor_log() {
    // LiteSVM / Debug spelling and the RPC client's Display spelling of the same transaction error.
    assert_eq!(timelock::parse_custom_code("InstructionError(1, Custom(6031))"), Some(6031));
    assert_eq!(timelock::parse_custom_code("Error processing Instruction 1: custom program error: 0x178f"), Some(6031));
    assert_eq!(0x178f, timelock::WRONG_GOVERNANCE_PROFILE_CODE);
    assert_eq!(timelock::parse_custom_code("InstructionError(0, ComputationalBudgetExceeded)"), None);
    assert_eq!(timelock::parse_custom_code("AccountNotFound"), None);
    let logs = vec![
        "Program 1111 invoke [1]".to_string(),
        "Program log: AnchorError thrown in programs/musical_chairs/src/controls.rs:1403. Error Code: WrongGovernanceProfile. Error Number: 6031. Error Message: WrongGovernanceProfile.".to_string(),
        "Program log: Left: 0".to_string(),
    ];
    assert_eq!(timelock::anchor_error_name(&logs, 6031).as_deref(), Some("WrongGovernanceProfile"));
    assert_eq!(timelock::anchor_error_name(&logs, 6004), None);
    assert_eq!(timelock::anchor_error_name(&[], 6031), None);
}

// ---------------------------------------------------------------------------
// 6. F11-R1: the execute-time guard (the config is the latest transaction and no other G transaction is live),
//    and what the raw Squads binary does with the transactions it stales
// ---------------------------------------------------------------------------

/// Everything a refused execute must leave exactly as it found it.
struct Frozen {
    /// `(signed simulations, unsigned simulations, sends)` of the cluster double.
    counters: (usize, usize, usize),
    pending: Option<String>,
    time_lock: u32,
    stale: u64,
    transaction_index: u64,
}

fn freeze(cluster: &SvmCluster<'_>, files: &Files, g: &ops::Identity) -> Frozen {
    let view = read_multisig(cluster, g);
    Frozen {
        counters: cluster.sims_and_sends(),
        pending: fs::read_to_string(&files.pending).ok(),
        time_lock: view.time_lock,
        stale: view.stale_transaction_index,
        transaction_index: view.transaction_index,
    }
}

/// `g-timelock execute <direction> --tx-index <index>` as a DRY RUN and with `--execute`: both are REFUSED with
/// every one of `fragments` in the message, and each leaves the world untouched: ZERO signed simulations, zero
/// unsigned simulations, ZERO sends, nothing written to the pending file, G's time lock, stale index and
/// transaction index unchanged, and the intended proposal still Approved.
fn assert_execute_refused(
    files: &Files,
    cluster: &SvmCluster<'_>,
    g: &ops::Identity,
    direction: Direction,
    index: u64,
    fragments: &[String],
) {
    let before = freeze(cluster, files, g);
    let index_text = index.to_string();
    for execute in [false, true] {
        let mut parts = vec!["execute", direction.name(), "--member", files.m(0), "--tx-index", index_text.as_str()];
        if execute {
            parts.push("--execute");
        }
        let message = refused(files.tl(cluster, &parts));
        for fragment in fragments {
            assert!(
                message.contains(fragment.as_str()),
                "{} {} (execute: {execute}) must be refused with `{fragment}`: {message}",
                direction.name(),
                index
            );
        }
        let after = freeze(cluster, files, g);
        assert_eq!(after.counters, before.counters, "ZERO signed simulations, unsigned simulations and sends (execute: {execute})");
        assert_eq!(after.pending, before.pending, "nothing was written to the pending file (execute: {execute})");
        assert_eq!(
            (after.time_lock, after.stale, after.transaction_index),
            (before.time_lock, before.stale, before.transaction_index),
            "G is unchanged (execute: {execute})"
        );
        assert_eq!(proposal_status(cluster, g, index), "Approved", "the intended proposal is still Approved (execute: {execute})");
    }
}

/// The foreign transactions a hostile or careless actor can leave next to the intended config.
#[derive(Clone, Copy, Debug)]
enum Foreign {
    /// A vault transaction with an Active proposal (one approval).
    VaultActive,
    /// A vault transaction with an Approved proposal (two approvals): it can still run after a config executes.
    VaultApproved,
    /// A vault transaction with NO proposal.
    VaultOrphan,
    /// A config transaction identical to the intended one, with NO proposal (no exception for an identical foreign config).
    ConfigOrphan,
}

impl Foreign {
    const ALL: [Foreign; 4] = [Foreign::VaultActive, Foreign::VaultApproved, Foreign::VaultOrphan, Foreign::ConfigOrphan];

    /// Create it as G transaction `index`; the vault variants return the instruction that would execute them.
    fn create(self, cluster: &SvmCluster<'_>, g: &ops::Identity, index: u64, direction: Direction) -> Option<Instruction> {
        let destination = Pubkey::new_from_array([0xF0 + (index % 8) as u8; 32]);
        match self {
            Foreign::VaultActive => Some(foreign_vault(cluster, g, index, destination, Some(&[0]))),
            Foreign::VaultApproved => Some(foreign_vault(cluster, g, index, destination, Some(&[0, 1]))),
            Foreign::VaultOrphan => Some(foreign_vault(cluster, g, index, destination, None)),
            Foreign::ConfigOrphan => {
                foreign_config_transaction(cluster, g, index, &direction.actions(), false);
                None
            }
        }
    }

    /// What the scan names when this transaction sits at `index` below the executing config.
    fn scan_message(self, index: u64) -> String {
        match self {
            Foreign::VaultActive => format!("G transaction #{index} (vault) has a LIVE proposal (Active)"),
            Foreign::VaultApproved => format!("G transaction #{index} (vault) has a LIVE proposal (Approved)"),
            Foreign::VaultOrphan => format!("G transaction #{index} (vault) has NO proposal"),
            Foreign::ConfigOrphan => format!("G transaction #{index} (config) has NO proposal"),
        }
    }
}

/// The tool's own reduce, start to finish (#3): G ends at 0 s with stale index 3.
fn tool_reduce(files: &Files, cluster: &SvmCluster<'_>) {
    ok(files.tl(cluster, &["propose", "reduce", "--member", files.m(0), "--execute", "--expect-tx-index", "3"]));
    for ordinal in [0, 1] {
        ok(files.tl(cluster, &["approve", "reduce", "--member", files.m(ordinal), "--tx-index", "3", "--execute"]));
    }
    cluster.advance(TWO_DAYS);
    let executed = ok(files.tl(cluster, &["execute", "reduce", "--member", files.m(0), "--tx-index", "3", "--execute"]));
    assert_eq!(executed["all_ok"], true, "{executed}");
}

#[test]
fn execute_refuses_when_a_foreign_transaction_appears_after_the_approval_for_reduce_and_restore() {
    for direction in [Direction::Reduce, Direction::Restore] {
        let mut world = world_with_orao();
        let payer = funded_payer(&mut world);
        let files = Files::write(&world, &payer);
        let g = g_identity();
        // The intended config is proposed and approved twice through the tool; restore starts from a G the tool
        // really reduced (0 s, stale index 3).
        let intended = {
            let cluster = SvmCluster::new(&mut world.rt.svm);
            genesis(&files, &cluster);
            if direction == Direction::Restore {
                tool_reduce(&files, &cluster);
            }
            let intended = if direction == Direction::Restore { 4 } else { 3 };
            let index_text = intended.to_string();
            ok(files.tl(&cluster, &["propose", direction.name(), "--member", files.m(0), "--execute", "--expect-tx-index", &index_text]));
            for ordinal in [0, 1] {
                ok(files.tl(&cluster, &["approve", direction.name(), "--member", files.m(ordinal), "--tx-index", &index_text, "--execute"]));
            }
            intended
        };
        let base = world.rt.svm.clone();
        for foreign in Foreign::ALL {
            let mut svm = base.clone();
            let cluster = SvmCluster::new(&mut svm);
            // AFTER the approval a foreign transaction appears (during the wait, in real life) ...
            let foreign_index = intended + 1;
            foreign.create(&cluster, &g, foreign_index, direction);
            if direction == Direction::Reduce {
                cluster.advance(TWO_DAYS + 1); // ... and the 172,800 s lock passes (restore has a 0 s lock).
            }
            assert_eq!(read_multisig(&cluster, &g).transaction_index, foreign_index);
            // The intended config is no longer the latest G transaction: the exact-index rule refuses it.
            assert_execute_refused(
                &files,
                &cluster,
                &g,
                direction,
                intended,
                &["is not the LATEST G transaction".to_string(), format!("transaction_index is {foreign_index}")],
            );
        }
    }
}

#[test]
fn execute_refuses_a_live_or_orphan_foreign_transaction_below_the_executing_config_for_reduce_and_restore() {
    for direction in [Direction::Reduce, Direction::Restore] {
        let mut world = world_with_orao();
        let payer = funded_payer(&mut world);
        let files = Files::write(&world, &payer);
        let g = g_identity();
        let first_free = {
            let cluster = SvmCluster::new(&mut world.rt.svm);
            genesis(&files, &cluster);
            if direction == Direction::Restore {
                tool_reduce(&files, &cluster);
                4
            } else {
                3
            }
        };
        let base = world.rt.svm.clone();
        for foreign in Foreign::ALL {
            let mut svm = base.clone();
            let cluster = SvmCluster::new(&mut svm);
            // A foreign transaction BELOW the intended config, which is hand-made at the latest index (the tool's
            // `propose` would have refused to stack on it) and approved twice by the tool (`approve` does not scan).
            let execute_foreign = foreign.create(&cluster, &g, first_free, direction);
            let intended = first_free + 1;
            foreign_config_transaction(&cluster, &g, intended, &direction.actions(), true);
            let index_text = intended.to_string();
            for ordinal in [0, 1] {
                ok(files.tl(&cluster, &["approve", direction.name(), "--member", files.m(ordinal), "--tx-index", &index_text, "--execute"]));
            }
            if direction == Direction::Reduce {
                cluster.advance(TWO_DAYS + 1);
            }
            assert_eq!(read_multisig(&cluster, &g).transaction_index, intended, "the config IS the latest transaction");
            // The exact-index rule passes; the full scan of (stale, transaction_index] names the foreign one.
            assert_execute_refused(&files, &cluster, &g, direction, intended, &[foreign.scan_message(first_free)]);

            // Positive control (the guard does not over-block): once the Approved foreign vault transaction has run
            // for real it is terminal (Executed), and the very same execute goes through, recording that scan.
            if let (Foreign::VaultApproved, Some(execute_foreign)) = (foreign, execute_foreign) {
                cluster
                    .send_direct(&member_keypair(0), &[execute_foreign])
                    .expect("the approved foreign vault transaction executes once its lock has passed");
                assert_eq!(proposal_status(&cluster, &g, first_free), "Executed");
                let executed = ok(files.tl(&cluster, &["execute", direction.name(), "--member", files.m(0), "--tx-index", &index_text, "--execute"]));
                assert_eq!(executed["all_ok"], true, "{executed}");
                let scanned = executed["other_g_transactions_checked"].as_array().unwrap();
                assert_eq!(scanned.last().map(|entry| (&entry["index"], &entry["proposal_status"])), Some((&json!(first_free), &json!("Executed"))), "{executed}");
                assert_eq!(read_multisig(&cluster, &g).time_lock, direction.target_lock());
            }
        }
    }
}

#[test]
fn execute_refuses_a_range_above_the_scan_cap_instead_of_scanning_part_of_it() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    // The intended config is the latest transaction, but more than LIVE_SCAN_CAP transactions sit above stale index 0.
    let intended = timelock::LIVE_SCAN_CAP + 1;
    for index in 3..intended {
        foreign_config_transaction(&cluster, &g, index, &Direction::Reduce.actions(), false);
    }
    foreign_config_transaction(&cluster, &g, intended, &Direction::Reduce.actions(), true);
    let index_text = intended.to_string();
    for ordinal in [0, 1] {
        ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(ordinal), "--tx-index", &index_text, "--execute"]));
    }
    cluster.advance(TWO_DAYS + 1);
    assert_execute_refused(
        &files,
        &cluster,
        &g,
        Direction::Reduce,
        intended,
        &[format!("scans at most {}", timelock::LIVE_SCAN_CAP), "partial scan".to_string()],
    );
}

#[test]
fn the_executed_reduce_makes_an_earlier_unexecuted_vault_transaction_stale() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let g = g_identity();
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    let member = files.m(0);

    // Two earlier VAULT proposals: #3 Active (one approval), #4 Approved (two approvals), neither executed.
    let destination_active = Pubkey::new_from_array([0xA3; 32]);
    let destination_approved = Pubkey::new_from_array([0xA4; 32]);
    foreign_vault_proposal(&cluster, &g, 3, destination_active, &[0]);
    let execute_approved = foreign_vault_proposal(&cluster, &g, 4, destination_approved, &[0, 1]);
    assert_eq!((proposal_status(&cluster, &g, 3), proposal_status(&cluster, &g, 4)), ("Active".to_string(), "Approved".to_string()));
    // The tool will not propose a reduce on top of them ...
    assert!(refused(files.tl(&cluster, &["propose", "reduce", "--member", member])).contains("g.no_live_transaction_above_stale_index"));
    // ... so the reduce is made by hand (a config transaction #5) and voted on by the tool (`approve` does not scan).
    foreign_config_transaction(&cluster, &g, 5, &Direction::Reduce.actions(), true);
    for ordinal in [0, 1] {
        ok(files.tl(&cluster, &["approve", "reduce", "--member", files.m(ordinal), "--tx-index", "5", "--execute"]));
    }
    cluster.advance(TWO_DAYS + 1);

    // F11-R1: the GUARDED CLI refuses to execute that very state, in a dry run and with --execute, with zero sends:
    // it would stale #3 and #4, and the Approved #4 would still run afterwards.
    assert_execute_refused(
        &files,
        &cluster,
        &g,
        Direction::Reduce,
        5,
        &["G transaction #3 (vault) has a LIVE proposal (Active)".to_string(), "nothing was sent".to_string()],
    );

    // The raw-Squads experiment, preserved: the same unsafe config executed DIRECTLY through the fixture (a raw
    // config_transaction_execute signed by a member, not through the tool).
    let executor = member_keypair(0);
    cluster
        .send_direct(&executor, &[timelock::config_transaction_execute_ix(g.multisig, 5, executor.pubkey())])
        .expect("the raw config_transaction_execute");
    let view = read_multisig(&cluster, &g);
    assert_eq!((view.time_lock, view.transaction_index, view.stale_transaction_index), (0, 5, 5));
    assert_eq!(proposal_status(&cluster, &g, 5), "Executed");

    // What the REAL binary does with the stale proposals (recorded).
    assert_eq!(proposal_status(&cluster, &g, 3), "Active", "the stale proposal's own status does not change");
    let late_voter = member_keypair(1);
    let vote = cluster.send_direct(&late_voter, &[sq::proposal_approve_ix(g.multisig, 3, late_voter.pubkey())]);
    let vote_outcome = match &vote {
        Ok(()) => json!({ "succeeded": true }),
        Err(text) => json!({ "succeeded": false, "error": text.lines().next().unwrap_or("") }),
    };
    let run_stale_approved = cluster.send_direct(&executor, &[execute_approved]);
    let execute_outcome = match &run_stale_approved {
        Ok(()) => json!({ "succeeded": true }),
        Err(text) => json!({ "succeeded": false, "error": text.lines().next().unwrap_or("") }),
    };
    sq::record_finding(
        "g_timelock_stale_vault_proposals",
        &json!({
            "stale_transaction_index_after_the_executed_reduce": view.stale_transaction_index,
            "vote_on_a_stale_active_vault_proposal": vote_outcome,
            "execute_of_a_vault_proposal_approved_before_it_went_stale": execute_outcome,
        }),
    );
    // Squads refuses a NEW vote on a stale proposal (StaleProposal).  Executing a vault proposal that was already
    // approved is deliberately not asserted: the finding above is the record of what the binary does, and it is why
    // the guarded execute refuses to stale an Approved transaction.
    let refusal = vote.expect_err("a vote on a stale proposal must be refused");
    assert!(refusal.contains(&format!("Custom({SQUADS_STALE_PROPOSAL})")), "{refusal}");
    assert_eq!(sq::idl_error_code(&sq::idl(), "StaleProposal"), SQUADS_STALE_PROPOSAL);
}

// ---------------------------------------------------------------------------
// 7. The standard devnet gate applies to g-timelock writes
// ---------------------------------------------------------------------------

#[test]
fn g_timelock_writes_on_devnet_need_the_pin_a_pending_file_and_the_devnet_genesis_and_record_the_pin() {
    let mut world = world_with_orao();
    let payer = funded_payer(&mut world);
    let files = Files::write(&world, &payer);
    let repo = PacketRepo::new("the reviewed M14 packet");
    let cluster = SvmCluster::new(&mut world.rt.svm);
    genesis(&files, &cluster);
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);

    let devnet = |extra: &[String]| {
        let mut parts: Vec<String> = ["g-timelock", "propose", "reduce", "--member", files.m(0), "--cluster", "devnet", "--rpc", DEVNET_URL]
            .iter()
            .map(|part| part.to_string())
            .collect();
        parts.extend(extra.iter().cloned());
        for (flag, value) in [("--g-create-key", files.g_create.as_str()), ("--g-members", files.g_members.as_str())] {
            parts.push(flag.into());
            parts.push(value.into());
        }
        ops::run_with_cluster_in(&parts, &cluster, &repo.root())
    };
    let execute = |extra: &[String]| {
        let mut flags: Vec<String> = vec!["--execute".into(), "--expect-tx-index".into(), "3".into()];
        flags.extend(extra.iter().cloned());
        devnet(&flags)
    };
    let pin = repo.pin_flags();
    let with_pending = |mut flags: Vec<String>| {
        flags.extend(["--pending-file".to_string(), files.pending.clone()]);
        flags
    };
    let sends = cluster.sends.get();

    // status and probe never take --execute, pin or no pin.
    let mut read_only = vec!["g-timelock".to_string(), "status".to_string(), "--execute".to_string()];
    read_only.extend(["--cluster", "devnet", "--rpc", DEVNET_URL, "--g-create-key", files.g_create.as_str(), "--g-members", files.g_members.as_str()].map(String::from));
    assert!(matches!(ops::run_with_cluster_in(&read_only, &cluster, &repo.root()), Err(OpsError::Usage(m)) if m.contains("read-only")));

    // No pin, a half pin, a pin without a write-ahead file: refused before anything is built.
    assert!(refused(execute(&[])).contains("--packet-sha256"));
    assert!(refused(execute(&pin[..2])).contains("--packet-commit"));
    assert!(refused(execute(&pin)).contains("--pending-file"));
    // mainnet (or any other) genesis behind the devnet flag.
    cluster.set_genesis(ops::MAINNET_GENESIS_HASH);
    assert!(refused(execute(&with_pending(pin.clone()))).contains("genesis"));
    cluster.set_genesis(ops::DEVNET_GENESIS_HASH);
    assert_eq!(cluster.sends.get(), sends);

    // A dry run needs no pin.
    let dry = ok(devnet(&[]));
    assert_eq!(dry["mode"], "dry-run");
    assert_eq!(dry["cluster"]["target"], "devnet");
    assert_eq!(cluster.sends.get(), sends);

    // The pin, the devnet genesis and a write-ahead file: executes, and the receipt and the records carry the pin.
    let proposed = ok(execute(&with_pending(pin.clone())));
    assert_eq!(proposed["all_ok"], true, "{proposed}");
    assert_eq!(proposed["mode"], "execute");
    assert_eq!(proposed["packet"]["packet_path"], ops::DEFAULT_PACKET_FILE);
    assert_eq!(proposed["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
    assert_eq!(proposed["packet"]["packet_sha256"].as_str(), Some(repo.sha.as_str()));
    assert_eq!(proposed["cluster"]["genesis_hash"], ops::DEVNET_GENESIS_HASH);
    assert_eq!(cluster.sends.get(), sends + 2);
    let lines = pending_lines(&files.pending);
    assert_eq!(lines[0]["command"], "g-timelock");
    assert_eq!(lines[0]["step"], "F11.reduce.config_transaction_create");
    assert_eq!(lines[0]["genesis_hash"], ops::DEVNET_GENESIS_HASH);
    assert_eq!(lines[0]["packet"]["packet_commit"].as_str(), Some(repo.commit.as_str()));
}
