//! M14-TOOL (packet RG-4): the devnet operator tool for the native-Solana deployment ceremony.
//!
//! The binary `devnet_ops` is a thin wrapper around [`run_cli`]; every behaviour lives here so the
//! LiteSVM tests (`tests/devnet_ops_local.rs`) drive the very code paths the operator runs, through
//! the [`Cluster`] trait (a JSON-RPC client in production, a LiteSVM wrapper in tests).
//!
//! ```text
//! devnet_ops plan             # ordered step list, accounts, signers, fee payer, rent, packet sizes
//! devnet_ops deploy           # loader-v3 buffer + chunked writes + deploy, byte check, authority -> G vault
//! devnet_ops verify-bytes     # packet A8: ProgramData header, ELF equality, zero padding (read-only)
//! devnet_ops squads-create    # G (2-of-3, 172,800 s) and P (2-of-3, 0 s) via real multisig_create_v2
//! devnet_ops fund-vault       # small system transfer so the G vault can pay the ORAO Client rent
//! devnet_ops propose <action> # vault_transaction_create + proposal_create (initialize | register)
//! devnet_ops approve <action> # proposal_approve by one member, after re-checking the stored message
//! devnet_ops execute <action> # vault_transaction_execute after the time lock (sponsor co-signs initialize)
//! devnet_ops readback         # packet A6 receipt (JSON): profiles, vaults, Pool/Admission, ORAO Client
//! devnet_ops validator-fixtures <out_dir>   # offline: account JSONs + the exact local validator command
//! ```
//!
//! # Safety interlocks (all enforced in code, all covered by tests)
//!
//! * **Dry run by default.** A write step is only sent with an explicit `--execute`.  `--execute`
//!   needs an [`ExecToken`], which only [`Gate::execute_token`] can mint: the cluster is `local`
//!   (default; the RPC URL must be `localhost`, `127.0.0.1` or `[::1]`), or `--cluster devnet` with
//!   `--i-have-approval <sha256>` equal to the SHA-256 of the approval packet file on disk (so an
//!   edit after approval invalidates the approval).  Any other cluster name is refused: the tool
//!   never targets mainnet.
//! * **Genesis hash.** Before anything else, `getGenesisHash` must be devnet's
//!   (`EtWTRABZ...`) for `--cluster devnet`, or NOT a known public cluster (mainnet-beta, testnet,
//!   devnet) for `local` (a loopback tunnel to a public cluster is refused).
//! * **Packet size.** Every transaction's serialized size (signature slots included) is computed
//!   BEFORE signing and anything over 1,232 bytes is refused.  `vault_transaction_create` is always
//!   built with proposer = creator = rent payer = fee payer (one signature, 1,140 bytes measured;
//!   a separate fee payer would add a 64-byte signature and 32-byte key and overflow).
//! * **Simulate before send.** Every send is preceded by `simulateTransaction`; an error refuses the
//!   send.
//! * **Time lock.** `execute` reads the proposal's `Approved { timestamp }` and the cluster clock and
//!   refuses before `timestamp + time_lock` (the program would answer 6021 after burning a fee).
//! * **Pinned bytes.** `deploy` and `verify-bytes` hash the ELF file first and refuse anything but
//!   the pinned digest ([`PINNED_PROGRAM_ELF_SHA256`]; an override exists on `local` only).
//! * **Approvers vote on what they were shown.** `approve` and `execute` rebuild the intended inner
//!   instruction from the command line and refuse unless the on-chain vault transaction stores
//!   exactly that message.  `propose` refuses when the program's `ProgramData` authority is not the
//!   G vault (the proposal would fail after a wasted 48-hour wait).
//! * **No key is generated here** and nothing is signed except by the key files named on the command
//!   line; `~/.config/solana/id.json` is refused by path.
//!
//! # Solana notes (closest ICP / EVM comparisons)
//!
//! * A loader-v3 deploy is three on-chain objects: a *buffer* (scratch account the ELF is written
//!   into, ~1,000 bytes per transaction because a transaction is capped at 1,232 bytes), the
//!   *ProgramData* account (45-byte header + the ELF + zero padding up to `max_data_len`, so
//!   `max_data_len` is the permanent size ceiling of every future upgrade and is paid in rent
//!   up-front) and the tiny *Program* account.  ICP's install-code uploads a module the same way
//!   but keeps one module hash; here "deployed equals reviewed" is the three-part statement
//!   [`verify_programdata`] checks.
//! * The upgrade authority is just a pubkey in the ProgramData header.  Handing it to the G vault
//!   (a Squads PDA) uses the unchecked `SetAuthority` because a PDA cannot co-sign; from then on
//!   only an approved, time-locked Squads vault transaction can upgrade or freeze the program (EVM
//!   analogue: a proxy admin that is a Safe with a timelock module; ICP analogue: SNS-controlled
//!   canister controllers).
//! * A Squads vault is a PDA, so it "signs" only while Squads executes an approved vault
//!   transaction.  The time lock is measured on the cluster's Clock sysvar (validators vote on it;
//!   it can lag wall time), which is why [`Cluster::unix_time`] is read from the chain.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anchor_lang::AccountDeserialize;
use musical_chairs_program::{
    accounts,
    controls::{InitializeArgs, SQUADS_MULTISIG_DISCRIMINATOR, SQUADS_PROGRAM_ID},
    instruction,
    provider::{self, orao},
    state, ID,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_loader_v3_interface::{instruction as loader_instruction, state::UpgradeableLoaderState};
use solana_rpc_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    sysvar,
    transaction::Transaction,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::{
    devnet,
    runtime::{anchor_ix, fixtures, pda},
    squads_real as sq,
};

// ---------------------------------------------------------------------------
// Constants and pins
// ---------------------------------------------------------------------------

/// The legacy transaction packet limit (`PACKET_DATA_SIZE`).
pub const PACKET_LIMIT: usize = 1_232;
/// Solana devnet's genesis hash (the cluster pin of the packet; also `frontend/.../cluster.ts`).
pub const DEVNET_GENESIS_HASH: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
/// Public cluster genesis hashes the tool refuses to treat as a "local" validator.
pub const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
pub const TESTNET_GENESIS_HASH: &str = "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY";
/// SHA-256 of the reviewed program ELF (`docs/solana/evidence/latest-build.json`,
/// `artifact.sha256`; a test keeps the two in step).
pub const PINNED_PROGRAM_ELF_SHA256: &str =
    "8f97e83db9dbc3112b1c18b5a35dfd5cfe6ae86bcb1a0cc93da78f7bb18dc826";
/// `Pool.cluster_domain` of the devnet acceptance run (packet section 2; NOT the LiteSVM fixture
/// domain `controls::DEVNET_DOMAIN`, and not the localnet dev domain).
pub const DEVNET_CLUSTER_DOMAIN: [u8; 32] = *b"musical-chairs-devnet-domain-v1!";
/// The approval packet whose SHA-256 `--i-have-approval` must equal (relative to the repo root).
pub const DEFAULT_PACKET_FILE: &str =
    "docs/coordination/native-solana-m14-approval-packet-20261023.md";
/// `UpgradeableLoaderState::ProgramData` metadata length (variant u32 + slot u64 + Option<Pubkey>).
pub const PROGRAMDATA_HEADER_LEN: usize = 45;
/// `UpgradeableLoaderState::Buffer` metadata length (variant u32 + Option<Pubkey>).
pub const BUFFER_HEADER_LEN: usize = 37;
/// Compute-unit limit requested by Squads transactions (no priority fee is ever set).
pub const CU_SQUADS: u32 = 400_000;
/// Compute-unit limit of the final loader `DeployWithMaxDataLen` transaction.
pub const CU_DEPLOY: u32 = 1_400_000;
/// Packet 4c: one program deployment is capped at 15 SOL.
pub const DEPLOY_SPEND_CAP_LAMPORTS: u64 = 15_000_000_000;
/// Packet 4b: the G vault needs about 0.02 SOL; the tool refuses to move more than 0.05 SOL.
pub const VAULT_FUNDING_CAP_LAMPORTS: u64 = 50_000_000;
pub const SIGNATURE_FEE_LAMPORTS: u64 = 5_000;
/// Squads `Multisig` account size with three members (231 bytes, packet 4b).
pub const MULTISIG_ACCOUNT_BYTES: usize = 231;
/// `Proposal::size(3)` (discriminator + multisig + index + status + bump + three vectors).
pub const PROPOSAL_ACCOUNT_BYTES: usize = 166;
/// Lower bound for a plausible ELF (anything smaller is a wrong file).
const MIN_ELF_BYTES: usize = 1_024;
/// Upper bound the loader accepts for program data (10 MiB).
const MAX_PROGRAM_BYTES: usize = 10 * 1024 * 1024;
const SQUADS_ALL_PERMISSIONS: u8 = 7;
const SQUADS_INITIATE: u8 = 1;
const SQUADS_VOTE: u8 = 2;
const SQUADS_EXECUTE: u8 = 4;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// What went wrong, mapped to the process exit code by [`exit_code`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpsError {
    /// Bad command line (exit 2).
    Usage(String),
    /// A safety interlock fired; nothing was sent (exit 3).
    Refused(String),
    /// An RPC, I/O or on-chain failure (exit 1).
    Failed(String),
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpsError::Usage(message) => write!(f, "usage: {message}"),
            OpsError::Refused(message) => write!(f, "REFUSED: {message}"),
            OpsError::Failed(message) => write!(f, "failed: {message}"),
        }
    }
}

impl std::error::Error for OpsError {}

/// Exit codes: 0 ok, 1 failure, 2 usage, 3 refused by an interlock, 4 a verification / readback
/// check did not hold (the receipt is still printed).
pub fn exit_code(error: &OpsError) -> i32 {
    match error {
        OpsError::Failed(_) => 1,
        OpsError::Usage(_) => 2,
        OpsError::Refused(_) => 3,
    }
}

fn refuse<T>(message: impl Into<String>) -> Result<T, OpsError> {
    Err(OpsError::Refused(message.into()))
}

fn fail<T>(message: impl Into<String>) -> Result<T, OpsError> {
    Err(OpsError::Failed(message.into()))
}

fn usage<T>(message: impl Into<String>) -> Result<T, OpsError> {
    Err(OpsError::Usage(message.into()))
}

fn rpc_err(context: &str, error: String) -> OpsError {
    OpsError::Failed(format!("{context}: {error}"))
}

fn progress(message: impl AsRef<str>) {
    eprintln!("[devnet_ops] {}", message.as_ref());
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

/// Flags that take a value.
const VALUE_FLAGS: &[&str] = &[
    "rpc",
    "cluster",
    "i-have-approval",
    "packet",
    "commitment",
    "elf",
    "expected-sha256",
    "max-data-len",
    "program-id",
    "deployer",
    "program-keypair",
    "buffer-keypair",
    "payer",
    "member",
    "sponsor",
    "management-recipient",
    "g-create-key",
    "p-create-key",
    "g-members",
    "p-members",
    "cluster-domain",
    "provider-request-fee-lamports",
    "provider-fee-headroom-bps",
    "public-series-a-enabled",
    "tx-index",
    "expect-tx-index",
    "lamports",
    "max-creation-fee",
    "max-spend-lamports",
    "expect-authority",
    "sections",
    "program-so",
    "out-dir",
];
/// Flags without a value.
const SWITCH_FLAGS: &[&str] = &["execute", "simulate", "allow-program-id-mismatch", "help"];
/// Subcommands that never write (they refuse `--execute`).
const READ_ONLY_COMMANDS: &[&str] = &["plan", "verify-bytes", "readback"];
const COMMANDS: &[&str] = &[
    "plan",
    "deploy",
    "verify-bytes",
    "squads-create",
    "fund-vault",
    "propose",
    "approve",
    "execute",
    "readback",
    "validator-fixtures",
];

/// A parsed command line: `devnet_ops <command> [positional] [--flag value] [--switch]`.
#[derive(Clone, Debug, Default)]
pub struct Opts {
    pub command: String,
    pub positional: Vec<String>,
    values: BTreeMap<String, String>,
    switches: BTreeSet<String>,
}

impl Opts {
    pub fn parse(args: &[String]) -> Result<Opts, OpsError> {
        let Some(command) = args.first() else {
            return usage(help_text());
        };
        let mut opts = Opts {
            command: command.clone(),
            ..Opts::default()
        };
        if command == "--help" || command == "help" || command == "-h" {
            opts.command = "help".into();
            return Ok(opts);
        }
        if !COMMANDS.contains(&command.as_str()) {
            return usage(format!("unknown command `{command}`\n{}", help_text()));
        }
        let mut index = 1;
        while index < args.len() {
            let token = &args[index];
            index += 1;
            let Some(flag) = token.strip_prefix("--") else {
                opts.positional.push(token.clone());
                continue;
            };
            let (name, inline) = match flag.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (flag.to_string(), None),
            };
            if SWITCH_FLAGS.contains(&name.as_str()) {
                if inline.is_some() {
                    return usage(format!("--{name} takes no value"));
                }
                if !opts.switches.insert(name.clone()) {
                    return usage(format!("--{name} given twice"));
                }
            } else if VALUE_FLAGS.contains(&name.as_str()) {
                let value = match inline {
                    Some(value) => value,
                    None => match args.get(index) {
                        Some(next) if !next.starts_with("--") => {
                            index += 1;
                            next.clone()
                        }
                        _ => return usage(format!("--{name} needs a value")),
                    },
                };
                if opts.values.insert(name.clone(), value).is_some() {
                    return usage(format!("--{name} given twice"));
                }
            } else {
                return usage(format!("unknown flag --{name}"));
            }
        }
        Ok(opts)
    }

    pub fn get(&self, flag: &str) -> Option<&str> {
        self.values.get(flag).map(String::as_str)
    }

    pub fn require(&self, flag: &str) -> Result<&str, OpsError> {
        self.get(flag)
            .ok_or_else(|| OpsError::Usage(format!("`{}` needs --{flag}", self.command)))
    }

    pub fn switch(&self, flag: &str) -> bool {
        self.switches.contains(flag)
    }

    fn u64_flag(&self, flag: &str) -> Result<Option<u64>, OpsError> {
        self.get(flag)
            .map(|value| {
                value
                    .replace('_', "")
                    .parse::<u64>()
                    .map_err(|_| OpsError::Usage(format!("--{flag} must be an unsigned integer")))
            })
            .transpose()
    }

    fn bool_flag(&self, flag: &str, default: bool) -> Result<bool, OpsError> {
        match self.get(flag) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(_) => usage(format!("--{flag} must be `true` or `false`")),
        }
    }
}

pub fn help_text() -> String {
    "usage: devnet_ops <command> [action] --rpc <url> [--cluster local|devnet] [--execute] [flags]\n\
     commands: plan | deploy | verify-bytes | squads-create | fund-vault | propose <initialize|register> |\n\
     approve <initialize|register> | execute <initialize|register> | readback | validator-fixtures <out_dir>\n\
     Every write is a DRY RUN unless --execute is given. --execute is refused unless the RPC URL is\n\
     loopback, or --cluster devnet is given together with --i-have-approval <sha256 of the approval\n\
     packet file>. The tool never targets mainnet. See docs/solana/devnet-ops.md."
        .to_string()
}

// ---------------------------------------------------------------------------
// The gate: loopback / devnet + approval hash / genesis hash
// ---------------------------------------------------------------------------

/// Which cluster family the operator named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A validator on this machine (`solana-test-validator`).
    Local,
    /// Solana devnet (genesis hash pinned).
    Devnet,
}

/// Proof that the execute gate passed.  Only [`Gate::execute_token`] constructs one, and the only
/// code that sends a transaction ([`Ctx::send_signed`]) demands it.
#[derive(Debug)]
pub struct ExecToken(());

#[derive(Clone, Debug)]
pub struct Gate {
    pub target: Target,
    pub rpc_url: String,
    pub execute: bool,
    pub commitment: CommitmentConfig,
}

/// `(scheme, lowercase host, port)` of a URL, or why it is not acceptable.  Userinfo (`@`),
/// backslashes and whitespace are rejected outright so `http://127.0.0.1@evil.example/` can never
/// pass for loopback.
pub fn parse_rpc_url(url: &str) -> Result<(String, String, Option<u16>), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("`{url}` is not a URL (no scheme)"))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme `{scheme}` is not http or https"));
    }
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| "empty host".to_string())?;
    if authority.contains('@') || authority.contains('\\') || authority.chars().any(char::is_whitespace)
    {
        return Err("userinfo, backslashes and whitespace are not allowed in the RPC URL".into());
    }
    let (host, port_text) = if let Some(stripped) = authority.strip_prefix('[') {
        let (inner, tail) = stripped
            .split_once(']')
            .ok_or_else(|| "unterminated [ipv6] host".to_string())?;
        let port = match tail {
            "" => None,
            tail => Some(
                tail.strip_prefix(':')
                    .ok_or_else(|| "garbage after the [ipv6] host".to_string())?,
            ),
        };
        (inner.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err("empty host".into());
    }
    let port = match port_text {
        None => None,
        Some(text) => Some(
            text.parse::<u16>()
                .map_err(|_| format!("`{text}` is not a port"))?,
        ),
    };
    Ok((scheme, host.to_ascii_lowercase(), port))
}

/// Exactly `localhost`, `127.0.0.1` or `::1` (`[::1]` in a URL).
pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn commitment_from(name: Option<&str>) -> Result<CommitmentConfig, OpsError> {
    match name {
        None | Some("confirmed") => Ok(CommitmentConfig::confirmed()),
        Some("finalized") => Ok(CommitmentConfig::finalized()),
        Some(other) => usage(format!(
            "--commitment must be `confirmed` or `finalized`, not `{other}`"
        )),
    }
}

/// The repo root (`solana/tests/financial-runtime/../../..`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

impl Gate {
    /// Static checks that need no network: cluster name, URL shape, loopback rule.  Applies to every
    /// subcommand, read-only ones included (`local` reads only loopback; `--cluster devnet` reads
    /// any https URL, whose genesis hash is then checked).
    pub fn from_opts(opts: &Opts) -> Result<Gate, OpsError> {
        let target = match opts.get("cluster") {
            None | Some("local") => Target::Local,
            Some("devnet") => Target::Devnet,
            Some(other) => {
                return refuse(format!(
                    "--cluster `{other}` is not supported: this tool only targets a local validator or devnet \
                     (mainnet-beta and testnet are refused on purpose)"
                ))
            }
        };
        let rpc_url = opts.require("rpc")?.to_string();
        let (scheme, host, _port) = parse_rpc_url(&rpc_url).map_err(OpsError::Refused)?;
        let loopback = is_loopback_host(&host);
        match target {
            Target::Local if !loopback => {
                return refuse(format!(
                    "cluster `local` requires a loopback RPC URL (localhost, 127.0.0.1, [::1]); `{host}` is not. \
                     Use --cluster devnet for devnet."
                ))
            }
            Target::Devnet if !loopback && scheme != "https" => {
                return refuse("--cluster devnet requires an https RPC URL")
            }
            _ => {}
        }
        Ok(Gate {
            target,
            rpc_url,
            execute: opts.switch("execute"),
            commitment: commitment_from(opts.get("commitment"))?,
        })
    }

    /// `Ok(None)` for a dry run; `Ok(Some(token))` when `--execute` passed every condition; an error
    /// when `--execute` was requested but a condition fails.
    pub fn execute_token(&self, opts: &Opts) -> Result<Option<ExecToken>, OpsError> {
        if !self.execute {
            return Ok(None);
        }
        if READ_ONLY_COMMANDS.contains(&opts.command.as_str()) {
            return usage(format!("`{}` is read-only and takes no --execute", opts.command));
        }
        if self.target == Target::Devnet {
            let approval = opts.get("i-have-approval").ok_or_else(|| {
                OpsError::Refused(
                    "--execute on devnet needs --i-have-approval <sha256 of the approved packet file>".into(),
                )
            })?;
            let approval = approval.to_ascii_lowercase();
            if approval.len() != 64 || !approval.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return refuse("--i-have-approval must be a 64-hex-digit SHA-256");
            }
            let packet_path = opts
                .get("packet")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo_root().join(DEFAULT_PACKET_FILE));
            let packet = fs::read(&packet_path).map_err(|error| {
                OpsError::Refused(format!(
                    "cannot read the approval packet {} to compare its hash: {error}",
                    packet_path.display()
                ))
            })?;
            let actual = devnet::sha256_hex(&packet);
            if actual != approval {
                return refuse(format!(
                    "--i-have-approval {approval} is not the SHA-256 of {} ({actual}); the packet changed after \
                     approval or the wrong hash was given",
                    packet_path.display()
                ));
            }
        }
        Ok(Some(ExecToken(())))
    }

    /// The genesis-hash interlock (run before any command logic).
    pub fn check_genesis(&self, genesis: &str) -> Result<(), OpsError> {
        match self.target {
            Target::Devnet if genesis == DEVNET_GENESIS_HASH => Ok(()),
            Target::Devnet => refuse(format!(
                "genesis hash {genesis} is not devnet's ({DEVNET_GENESIS_HASH}); this tool never targets any other public cluster"
            )),
            Target::Local if genesis == MAINNET_GENESIS_HASH => refuse(
                "the loopback RPC reports the MAINNET-BETA genesis hash (a tunnel?); refusing",
            ),
            Target::Local if genesis == TESTNET_GENESIS_HASH => {
                refuse("the loopback RPC reports the TESTNET genesis hash (a tunnel?); refusing")
            }
            Target::Local if genesis == DEVNET_GENESIS_HASH => refuse(
                "the loopback RPC reports devnet's genesis hash (a tunnel?); use --cluster devnet and the approval hash",
            ),
            Target::Local => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Key files
// ---------------------------------------------------------------------------

/// A key given as a file (Solana CLI keypair JSON, or a file holding one base58 pubkey) or as a
/// bare base58 pubkey.  Public-key-only values are enough for planning and read-backs; signing
/// commands call [`KeyFile::keypair`].
pub struct KeyFile {
    pub pubkey: Pubkey,
    keypair: Option<Keypair>,
    source: String,
}

fn refuse_default_wallet(path: &Path) -> Result<(), OpsError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for candidate in [path, canonical.as_path()] {
        let parts: Vec<String> = candidate
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        if parts.len() >= 3 && parts[parts.len() - 3..] == [".config", "solana", "id.json"] {
            return refuse(format!(
                "{} is the Solana CLI default wallet; this tool never reads it",
                path.display()
            ));
        }
    }
    Ok(())
}

impl KeyFile {
    pub fn load(spec: &str) -> Result<KeyFile, OpsError> {
        let path = Path::new(spec);
        refuse_default_wallet(path)?;
        if path.is_file() {
            let metadata = fs::metadata(path)
                .map_err(|error| OpsError::Failed(format!("{spec}: {error}")))?;
            if metadata.len() > 4_096 {
                return refuse(format!("{spec} is too large to be a key file"));
            }
            let text = fs::read_to_string(path)
                .map_err(|error| OpsError::Failed(format!("{spec}: {error}")))?;
            let trimmed = text.trim();
            if trimmed.starts_with('[') {
                let bytes: Vec<u8> = serde_json::from_str(trimmed).map_err(|error| {
                    OpsError::Failed(format!("{spec} is not a keypair JSON array: {error}"))
                })?;
                let keypair = Keypair::try_from(&bytes[..]).map_err(|_| {
                    OpsError::Failed(format!(
                        "{spec} is not a valid 64-byte ed25519 keypair (public half does not match the secret)"
                    ))
                })?;
                return Ok(KeyFile {
                    pubkey: keypair.pubkey(),
                    keypair: Some(keypair),
                    source: spec.to_string(),
                });
            }
            let pubkey = Pubkey::from_str(trimmed).map_err(|_| {
                OpsError::Failed(format!("{spec} holds neither a keypair JSON array nor a base58 pubkey"))
            })?;
            return Ok(KeyFile {
                pubkey,
                keypair: None,
                source: spec.to_string(),
            });
        }
        match Pubkey::from_str(spec) {
            Ok(pubkey) => Ok(KeyFile {
                pubkey,
                keypair: None,
                source: spec.to_string(),
            }),
            Err(_) => usage(format!("`{spec}` is neither an existing key file nor a base58 pubkey")),
        }
    }

    /// The signing key; an error when only a public key was given.
    pub fn keypair(&self) -> Result<&Keypair, OpsError> {
        self.keypair.as_ref().ok_or_else(|| {
            OpsError::Usage(format!(
                "{} is a public key; this step must sign, so pass a keypair file",
                self.source
            ))
        })
    }
}

fn key_flag(opts: &Opts, flag: &str) -> Result<KeyFile, OpsError> {
    KeyFile::load(opts.require(flag)?)
}

fn optional_key_flag(opts: &Opts, flag: &str) -> Result<Option<KeyFile>, OpsError> {
    opts.get(flag).map(KeyFile::load).transpose()
}

/// Three distinct, non-zero member pubkeys, one per line (`#` comments and blank lines allowed).
pub fn read_members(path: &str) -> Result<Vec<Pubkey>, OpsError> {
    refuse_default_wallet(Path::new(path))?;
    let text = fs::read_to_string(path).map_err(|error| OpsError::Failed(format!("{path}: {error}")))?;
    let mut members = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let key = Pubkey::from_str(line)
            .map_err(|_| OpsError::Usage(format!("{path}: `{line}` is not a base58 pubkey")))?;
        members.push(key);
    }
    let distinct: BTreeSet<Pubkey> = members.iter().copied().collect();
    if members.len() != 3 || distinct.len() != 3 {
        return usage(format!("{path} must list exactly 3 distinct member pubkeys"));
    }
    if members.contains(&Pubkey::default()) {
        return usage(format!("{path} lists the all-zero pubkey"));
    }
    Ok(members)
}

/// A Squads multisig this tool manages: its create key, address and vault 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub create_key: Pubkey,
    pub multisig: Pubkey,
    pub vault: Pubkey,
}

pub fn identity(create_key: Pubkey) -> Identity {
    let multisig = fixtures::squads::multisig_address(&create_key).0;
    let vault = fixtures::squads::vault_address(&multisig, 0).0;
    Identity {
        create_key,
        multisig,
        vault,
    }
}

pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

// ---------------------------------------------------------------------------
// The cluster abstraction
// ---------------------------------------------------------------------------

/// Result of `simulateTransaction`.
#[derive(Clone, Debug, Default)]
pub struct SimOutcome {
    pub error: Option<String>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
}

/// A confirmed transaction.
#[derive(Clone, Debug)]
pub struct SendOutcome {
    pub signature: Signature,
    pub units_consumed: Option<u64>,
}

/// Everything the tool needs from a cluster.  Production uses [`RpcCluster`]; the tests wrap
/// LiteSVM.  Errors are plain strings (the transport's own message).
pub trait Cluster {
    fn genesis_hash(&self) -> Result<String, String>;
    /// A blockhash valid for a transaction signed right now.
    fn blockhash(&self) -> Result<Hash, String>;
    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String>;
    fn rent_minimum(&self, data_len: usize) -> Result<u64, String>;
    /// The cluster's own clock (Clock sysvar `unix_timestamp`), NOT this machine's clock.
    fn unix_time(&self) -> Result<i64, String>;
    fn slot(&self) -> Result<u64, String>;
    /// `sig_verify` asks for signature verification where the transport supports it.
    fn simulate(&self, tx: &Transaction, sig_verify: bool) -> Result<SimOutcome, String>;
    /// Send and wait for confirmation; an error means the transaction did not succeed.
    fn send(&self, tx: &Transaction) -> Result<SendOutcome, String>;
}

/// Decode the `unix_timestamp` of a serialized Clock sysvar (bincode: slot u64, epoch_start
/// i64, epoch u64, leader_schedule_epoch u64, unix_timestamp i64).
pub fn clock_unix_timestamp(data: &[u8]) -> Result<i64, String> {
    let bytes: [u8; 8] = data
        .get(32..40)
        .ok_or_else(|| "Clock sysvar is shorter than 40 bytes".to_string())?
        .try_into()
        .map_err(|_| "Clock sysvar slice".to_string())?;
    Ok(i64::from_le_bytes(bytes))
}

/// JSON-RPC cluster (blocking `solana-rpc-client`).
pub struct RpcCluster {
    rpc: RpcClient,
    commitment: CommitmentConfig,
}

impl RpcCluster {
    pub fn connect(url: &str, commitment: CommitmentConfig) -> RpcCluster {
        RpcCluster {
            rpc: RpcClient::new_with_timeout_and_commitment(
                url.to_string(),
                Duration::from_secs(120),
                commitment,
            ),
            commitment,
        }
    }
}

impl Cluster for RpcCluster {
    fn genesis_hash(&self) -> Result<String, String> {
        self.rpc
            .get_genesis_hash()
            .map(|hash| hash.to_string())
            .map_err(|error| error.to_string())
    }

    fn blockhash(&self) -> Result<Hash, String> {
        self.rpc
            .get_latest_blockhash_with_commitment(self.commitment)
            .map(|(hash, _last_valid_block_height)| hash)
            .map_err(|error| error.to_string())
    }

    fn account(&self, key: &Pubkey) -> Result<Option<Account>, String> {
        self.rpc
            .get_account_with_commitment(key, self.commitment)
            .map(|response| response.value)
            .map_err(|error| error.to_string())
    }

    fn rent_minimum(&self, data_len: usize) -> Result<u64, String> {
        self.rpc
            .get_minimum_balance_for_rent_exemption(data_len)
            .map_err(|error| error.to_string())
    }

    fn unix_time(&self) -> Result<i64, String> {
        let clock = self
            .account(&sysvar::clock::id())?
            .ok_or_else(|| "Clock sysvar account is absent".to_string())?;
        clock_unix_timestamp(&clock.data)
    }

    fn slot(&self) -> Result<u64, String> {
        self.rpc
            .get_slot_with_commitment(self.commitment)
            .map_err(|error| error.to_string())
    }

    fn simulate(&self, tx: &Transaction, _sig_verify: bool) -> Result<SimOutcome, String> {
        // The client API's default simulation (`sigVerify` off, blockhash as given).  The tool has
        // already verified locally that every required signer signed (`sign_checked`), and the
        // cluster verifies signatures again on the real send.
        let response = self
            .rpc
            .simulate_transaction(tx)
            .map_err(|error| error.to_string())?;
        Ok(SimOutcome {
            error: response.value.err.map(|error| error.to_string()),
            logs: response.value.logs.unwrap_or_default(),
            units_consumed: response.value.units_consumed,
        })
    }

    fn send(&self, tx: &Transaction) -> Result<SendOutcome, String> {
        let signature = self
            .rpc
            .send_and_confirm_transaction(tx)
            .map_err(|error| error.to_string())?;
        Ok(SendOutcome {
            signature,
            units_consumed: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Account decoders (bounds-checked; layouts from the Squads IDL and the loader)
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| format!("account data truncated at byte {}", self.pos))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| "u16")?))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| "u32")?))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "u64")?))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "i64")?))
    }
    fn key(&mut self) -> Result<Pubkey, String> {
        Ok(Pubkey::new_from_array(
            self.take(32)?.try_into().map_err(|_| "pubkey")?,
        ))
    }
    /// A Borsh `Vec<Pubkey>` (u32 length prefix).
    fn keys(&mut self) -> Result<Vec<Pubkey>, String> {
        let count = self.u32()? as usize;
        if count.checked_mul(32).is_none_or(|bytes| bytes > self.remaining()) {
            return Err("vector length exceeds the account data".into());
        }
        (0..count).map(|_| self.key()).collect()
    }
    /// A Borsh `Vec<u8>` / `bytes` (u32 length prefix).
    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let count = self.u32()? as usize;
        Ok(self.take(count)?.to_vec())
    }
}

/// Anchor account discriminator: `sha256("account:<Name>")[..8]`.
pub fn account_discriminator(name: &str) -> [u8; 8] {
    let digest = Sha256::digest(format!("account:{name}").as_bytes());
    digest[..8].try_into().expect("8 bytes")
}

/// The squads `Multisig` account (layout of the IDL's `Multisig`).
pub fn decode_multisig(data: &[u8]) -> Result<sq::MultisigView, String> {
    if data.len() < 8 || data[..8] != SQUADS_MULTISIG_DISCRIMINATOR {
        return Err("not a Squads Multisig account (discriminator)".into());
    }
    let mut reader = Reader::new(data);
    reader.take(8)?;
    let create_key = reader.key()?;
    let config_authority = reader.key()?;
    let threshold = reader.u16()?;
    let time_lock = reader.u32()?;
    let transaction_index = reader.u64()?;
    let stale_transaction_index = reader.u64()?;
    let rent_collector = match reader.u8()? {
        0 => None,
        1 => Some(reader.key()?),
        other => return Err(format!("rent_collector option tag {other}")),
    };
    let bump = reader.u8()?;
    let member_count = reader.u32()? as usize;
    if member_count.checked_mul(33).is_none_or(|bytes| bytes > reader.remaining()) {
        return Err("member list exceeds the account data".into());
    }
    let mut members = Vec::with_capacity(member_count);
    for _ in 0..member_count {
        let member = reader.key()?;
        let mask = reader.u8()?;
        members.push((member, mask));
    }
    Ok(sq::MultisigView {
        create_key,
        config_authority,
        threshold,
        time_lock,
        transaction_index,
        stale_transaction_index,
        rent_collector,
        bump,
        members,
        trailing_bytes: reader.remaining(),
    })
}

/// `ProposalStatus` (each variant but `Executing` carries the timestamp it was set at).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    Draft(i64),
    Active(i64),
    Rejected(i64),
    Approved(i64),
    Executing,
    Executed(i64),
    Cancelled(i64),
}

impl ProposalStatus {
    pub fn name(&self) -> &'static str {
        match self {
            ProposalStatus::Draft(_) => "Draft",
            ProposalStatus::Active(_) => "Active",
            ProposalStatus::Rejected(_) => "Rejected",
            ProposalStatus::Approved(_) => "Approved",
            ProposalStatus::Executing => "Executing",
            ProposalStatus::Executed(_) => "Executed",
            ProposalStatus::Cancelled(_) => "Cancelled",
        }
    }
    pub fn timestamp(&self) -> Option<i64> {
        match self {
            ProposalStatus::Draft(t)
            | ProposalStatus::Active(t)
            | ProposalStatus::Rejected(t)
            | ProposalStatus::Approved(t)
            | ProposalStatus::Executed(t)
            | ProposalStatus::Cancelled(t) => Some(*t),
            ProposalStatus::Executing => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalView {
    pub multisig: Pubkey,
    pub transaction_index: u64,
    pub status: ProposalStatus,
    pub approved: Vec<Pubkey>,
    pub rejected: Vec<Pubkey>,
    pub cancelled: Vec<Pubkey>,
}

pub fn decode_proposal(data: &[u8]) -> Result<ProposalView, String> {
    if data.len() < 8 || data[..8] != account_discriminator("Proposal") {
        return Err("not a Squads Proposal account (discriminator)".into());
    }
    let mut reader = Reader::new(data);
    reader.take(8)?;
    let multisig = reader.key()?;
    let transaction_index = reader.u64()?;
    let status = match reader.u8()? {
        0 => ProposalStatus::Draft(reader.i64()?),
        1 => ProposalStatus::Active(reader.i64()?),
        2 => ProposalStatus::Rejected(reader.i64()?),
        3 => ProposalStatus::Approved(reader.i64()?),
        4 => ProposalStatus::Executing,
        5 => ProposalStatus::Executed(reader.i64()?),
        6 => ProposalStatus::Cancelled(reader.i64()?),
        other => return Err(format!("unknown ProposalStatus variant {other}")),
    };
    let _bump = reader.u8()?;
    Ok(ProposalView {
        multisig,
        transaction_index,
        status,
        approved: reader.keys()?,
        rejected: reader.keys()?,
        cancelled: reader.keys()?,
    })
}

/// One instruction inside a stored vault transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredInstruction {
    pub program_id_index: u8,
    pub account_indexes: Vec<u8>,
    pub data: Vec<u8>,
}

/// The `VaultTransactionMessage` Squads stores (Borsh `Vec`s, unlike the small-vec input format).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMessage {
    pub num_signers: u8,
    pub num_writable_signers: u8,
    pub num_writable_non_signers: u8,
    pub account_keys: Vec<Pubkey>,
    pub instructions: Vec<StoredInstruction>,
    pub address_table_lookups: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultTransactionView {
    pub multisig: Pubkey,
    pub creator: Pubkey,
    pub index: u64,
    pub vault_index: u8,
    pub ephemeral_signer_bumps: Vec<u8>,
    pub message: StoredMessage,
}

pub fn decode_vault_transaction(data: &[u8]) -> Result<VaultTransactionView, String> {
    if data.len() < 8 || data[..8] != account_discriminator("VaultTransaction") {
        return Err("not a Squads VaultTransaction account (discriminator)".into());
    }
    let mut reader = Reader::new(data);
    reader.take(8)?;
    let multisig = reader.key()?;
    let creator = reader.key()?;
    let index = reader.u64()?;
    let _bump = reader.u8()?;
    let vault_index = reader.u8()?;
    let _vault_bump = reader.u8()?;
    let ephemeral_signer_bumps = reader.bytes()?;
    let num_signers = reader.u8()?;
    let num_writable_signers = reader.u8()?;
    let num_writable_non_signers = reader.u8()?;
    let account_keys = reader.keys()?;
    let instruction_count = reader.u32()? as usize;
    if instruction_count > reader.remaining() {
        return Err("instruction list exceeds the account data".into());
    }
    let mut instructions = Vec::with_capacity(instruction_count);
    for _ in 0..instruction_count {
        instructions.push(StoredInstruction {
            program_id_index: reader.u8()?,
            account_indexes: reader.bytes()?,
            data: reader.bytes()?,
        });
    }
    let address_table_lookups = reader.u32()? as usize;
    Ok(VaultTransactionView {
        multisig,
        creator,
        index,
        vault_index,
        ephemeral_signer_bumps,
        message: StoredMessage {
            num_signers,
            num_writable_signers,
            num_writable_non_signers,
            account_keys,
            instructions,
            address_table_lookups,
        },
    })
}

/// The message Squads must have stored for `inner` executed by the vault (what
/// `compile_vault_message` sent, in the stored shape).
pub fn expected_stored_message(inner: &Instruction) -> StoredMessage {
    let compiled = sq::compile_vault_message(inner);
    let keys: Vec<Pubkey> = compiled.keys.iter().map(|entry| entry.0).collect();
    let index_of = |key: &Pubkey| -> u8 {
        keys.iter().position(|candidate| candidate == key).expect("compiled key") as u8
    };
    StoredMessage {
        num_signers: compiled.keys.iter().filter(|entry| entry.1).count() as u8,
        num_writable_signers: compiled.keys.iter().filter(|entry| entry.1 && entry.2).count() as u8,
        num_writable_non_signers: compiled
            .keys
            .iter()
            .filter(|entry| !entry.1 && entry.2)
            .count() as u8,
        instructions: vec![StoredInstruction {
            program_id_index: index_of(&inner.program_id),
            account_indexes: inner.accounts.iter().map(|meta| index_of(&meta.pubkey)).collect(),
            data: inner.data.clone(),
        }],
        account_keys: keys,
        address_table_lookups: 0,
    }
}

/// Loader-v3 `Program` account: the ProgramData address it points at.
pub fn decode_program_account(data: &[u8]) -> Result<Pubkey, String> {
    let mut reader = Reader::new(data);
    match reader.u32()? {
        2 => reader.key(),
        other => Err(format!("loader state variant {other} is not Program (2)")),
    }
}

/// Loader-v3 `Buffer` account: its authority.
pub fn decode_buffer_authority(data: &[u8]) -> Result<Option<Pubkey>, String> {
    let mut reader = Reader::new(data);
    match reader.u32()? {
        1 => match reader.u8()? {
            0 => Ok(None),
            1 => Ok(Some(reader.key()?)),
            other => Err(format!("buffer authority option tag {other}")),
        },
        other => Err(format!("loader state variant {other} is not Buffer (1)")),
    }
}

/// The 45-byte `ProgramData` header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramDataHeader {
    pub slot: u64,
    pub authority: Option<Pubkey>,
}

pub fn decode_programdata_header(data: &[u8]) -> Result<ProgramDataHeader, String> {
    if data.len() < PROGRAMDATA_HEADER_LEN {
        return Err(format!(
            "ProgramData is {} bytes, shorter than its {PROGRAMDATA_HEADER_LEN}-byte header",
            data.len()
        ));
    }
    let mut reader = Reader::new(data);
    match reader.u32()? {
        3 => {}
        other => return Err(format!("loader state variant {other} is not ProgramData (3)")),
    }
    let slot = reader.u64()?;
    let authority = match reader.u8()? {
        0 => {
            if data[13..PROGRAMDATA_HEADER_LEN].iter().any(|byte| *byte != 0) {
                return Err("ProgramData authority is None but the 32 authority bytes are not zero".into());
            }
            None
        }
        1 => Some(reader.key()?),
        other => return Err(format!("ProgramData authority option tag {other}")),
    };
    Ok(ProgramDataHeader { slot, authority })
}

/// The packet's A8 check on a fetched ProgramData account against the reviewed ELF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramDataReport {
    pub header: ProgramDataHeader,
    pub data_len: usize,
    pub elf_len: usize,
    pub elf_matches: bool,
    pub first_mismatch: Option<usize>,
    pub padding_len: usize,
    pub padding_all_zero: bool,
    pub elf_sha256: String,
    pub onchain_elf_sha256: String,
    pub payload_sha256: String,
}

impl ProgramDataReport {
    /// Header decodes, `data[45..45+elf_len] == ELF`, `data[45+elf_len..]` all zero.
    pub fn ok(&self) -> bool {
        self.elf_matches && self.padding_all_zero
    }

    pub fn to_json(&self) -> Value {
        json!({
            "programdata_slot": self.header.slot,
            "upgrade_authority": self.header.authority.map(|key| key.to_string()),
            "account_data_len": self.data_len,
            "header_len": PROGRAMDATA_HEADER_LEN,
            "elf_len": self.elf_len,
            "elf_sha256": self.elf_sha256,
            "onchain_elf_slice_sha256": self.onchain_elf_sha256,
            "payload_sha256_after_header": self.payload_sha256,
            "elf_matches": self.elf_matches,
            "first_mismatch_offset": self.first_mismatch,
            "zero_padding_len": self.padding_len,
            "zero_padding_all_zero": self.padding_all_zero,
            "ok": self.ok(),
        })
    }
}

/// Compare a ProgramData account's bytes with the ELF (`Err` only when the header is undecodable).
pub fn verify_programdata(data: &[u8], elf: &[u8]) -> Result<ProgramDataReport, String> {
    let header = decode_programdata_header(data)?;
    let payload = &data[PROGRAMDATA_HEADER_LEN..];
    let compared = payload.len().min(elf.len());
    let first_mismatch = payload[..compared]
        .iter()
        .zip(&elf[..compared])
        .position(|(left, right)| left != right)
        .or(if payload.len() < elf.len() { Some(payload.len()) } else { None });
    let (onchain_slice, padding) = if payload.len() >= elf.len() {
        payload.split_at(elf.len())
    } else {
        (payload, &payload[payload.len()..])
    };
    Ok(ProgramDataReport {
        header,
        data_len: data.len(),
        elf_len: elf.len(),
        elf_matches: first_mismatch.is_none() && payload.len() >= elf.len(),
        first_mismatch,
        padding_len: padding.len(),
        padding_all_zero: padding.iter().all(|byte| *byte == 0),
        elf_sha256: devnet::sha256_hex(elf),
        onchain_elf_sha256: devnet::sha256_hex(onchain_slice),
        payload_sha256: devnet::sha256_hex(payload),
    })
}

// ---------------------------------------------------------------------------
// Transactions: build, size-check BEFORE signing, sign
// ---------------------------------------------------------------------------

/// One transaction the tool may send.
#[derive(Clone, Debug)]
pub struct Step {
    pub id: String,
    pub label: String,
    pub instructions: Vec<Instruction>,
    pub payer: Pubkey,
    pub cu_limit: Option<u32>,
    /// Needs the on-chain effect of an earlier step (so a dry run cannot simulate it).
    pub depends_on_previous: bool,
}

/// A step compiled to a message, with its measured size.
#[derive(Clone, Debug)]
pub struct Built {
    pub message: Message,
    pub packet_bytes: usize,
    pub required_signers: Vec<Pubkey>,
}

impl Built {
    pub fn fits(&self) -> bool {
        self.packet_bytes <= PACKET_LIMIT
    }
}

/// Compile `step` (compute-budget instruction first, when requested) and measure the serialized
/// transaction size, signature slots included, without signing anything.
pub fn build_step(step: &Step) -> Built {
    let mut instructions = Vec::with_capacity(step.instructions.len() + 1);
    if let Some(limit) = step.cu_limit {
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(limit));
    }
    instructions.extend(step.instructions.iter().cloned());
    let message = Message::new(&instructions, Some(&step.payer));
    let packet_bytes = bincode::serialize(&Transaction::new_unsigned(message.clone()))
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    let required = usize::from(message.header.num_required_signatures);
    let required_signers = message.account_keys[..required].to_vec();
    Built {
        message,
        packet_bytes,
        required_signers,
    }
}

/// The hard rule: refuse to sign anything over 1,232 bytes.  The size is checked on the unsigned
/// message, so no signature exists when the refusal fires.
pub fn sign_checked(
    label: &str,
    built: &Built,
    signers: &[&Keypair],
    blockhash: Hash,
) -> Result<Transaction, OpsError> {
    if !built.fits() {
        return refuse(format!(
            "{label}: serialized transaction is {} bytes, over the {PACKET_LIMIT}-byte packet limit; nothing was signed",
            built.packet_bytes
        ));
    }
    let mut seen = BTreeSet::new();
    let unique: Vec<&dyn Signer> = signers
        .iter()
        .filter(|keypair| seen.insert(keypair.pubkey()))
        .map(|keypair| *keypair as &dyn Signer)
        .collect();
    let mut transaction = Transaction::new_unsigned(built.message.clone());
    transaction
        .try_partial_sign(&unique, blockhash)
        .map_err(|error| {
            OpsError::Refused(format!(
                "{label}: a provided signer does not belong to this transaction ({error}); nothing was sent"
            ))
        })?;
    if !transaction.is_signed() {
        let missing: Vec<String> = built
            .required_signers
            .iter()
            .filter(|key| !seen.contains(key))
            .map(|key| key.to_string())
            .collect();
        return refuse(format!(
            "{label}: required signer(s) {} were not provided; nothing was sent",
            missing.join(", ")
        ));
    }
    Ok(transaction)
}

/// What one step did.
#[derive(Clone, Debug)]
pub struct StepResult {
    pub step_id: String,
    pub label: String,
    pub fee_payer: Pubkey,
    pub required_signers: Vec<Pubkey>,
    pub packet_bytes: usize,
    pub instruction_count: usize,
    pub depends_on_previous: bool,
    pub executed: bool,
    pub signature: Option<Signature>,
    pub simulation: Option<SimOutcome>,
}

impl StepResult {
    pub fn to_json(&self) -> Value {
        json!({
            "step": self.step_id,
            "label": self.label,
            "fee_payer": self.fee_payer.to_string(),
            "required_signers": self.required_signers.iter().map(Pubkey::to_string).collect::<Vec<_>>(),
            "packet_bytes": self.packet_bytes,
            "fits_1232": self.packet_bytes <= PACKET_LIMIT,
            "instructions": self.instruction_count,
            "depends_on_previous": self.depends_on_previous,
            "executed": self.executed,
            "signature": self.signature.map(|signature| signature.to_string()),
            "simulation": self.simulation.as_ref().map(|sim| json!({
                "error": sim.error,
                "units_consumed": sim.units_consumed,
            })),
        })
    }
}

// ---------------------------------------------------------------------------
// Context: the one place that can send
// ---------------------------------------------------------------------------

pub struct Ctx<'a> {
    pub opts: &'a Opts,
    pub gate: &'a Gate,
    pub cluster: &'a dyn Cluster,
    token: Option<ExecToken>,
}

impl<'a> Ctx<'a> {
    pub fn executing(&self) -> bool {
        self.token.is_some()
    }

    fn mode(&self) -> &'static str {
        if self.executing() {
            "execute"
        } else {
            "dry-run"
        }
    }

    pub fn account(&self, key: &Pubkey) -> Result<Option<Account>, OpsError> {
        self.cluster
            .account(key)
            .map_err(|error| rpc_err(&format!("getAccountInfo {key}"), error))
    }

    pub fn rent(&self, data_len: usize) -> Result<u64, OpsError> {
        self.cluster
            .rent_minimum(data_len)
            .map_err(|error| rpc_err("getMinimumBalanceForRentExemption", error))
    }

    fn balance(&self, key: &Pubkey) -> Result<u64, OpsError> {
        Ok(self.account(key)?.map_or(0, |account| account.lamports))
    }

    /// The ONLY place a transaction is sent, and it demands the execute token.
    fn send_signed(&self, label: &str, tx: &Transaction) -> Result<SendOutcome, OpsError> {
        if self.token.is_none() {
            return refuse("internal error: send attempted without the execute token");
        }
        self.cluster
            .send(tx)
            .map_err(|error| OpsError::Failed(format!("{label}: transaction failed: {error}")))
    }

    /// Dry run: measure (and optionally simulate unsigned).  Execute: refuse oversize, sign,
    /// simulate (refuse on error), send, confirm.
    pub fn run_step(&self, step: &Step, signers: &[&Keypair]) -> Result<StepResult, OpsError> {
        let built = build_step(step);
        let mut result = StepResult {
            step_id: step.id.clone(),
            label: step.label.clone(),
            fee_payer: step.payer,
            required_signers: built.required_signers.clone(),
            packet_bytes: built.packet_bytes,
            instruction_count: step.instructions.len() + usize::from(step.cu_limit.is_some()),
            depends_on_previous: step.depends_on_previous,
            executed: false,
            signature: None,
            simulation: None,
        };
        if !built.fits() {
            return refuse(format!(
                "{}: serialized transaction is {} bytes, over the {PACKET_LIMIT}-byte packet limit; nothing was signed",
                step.label, built.packet_bytes
            ));
        }
        if !self.executing() {
            if self.opts.switch("simulate") && !step.depends_on_previous {
                let blockhash = self
                    .cluster
                    .blockhash()
                    .map_err(|error| rpc_err("getLatestBlockhash", error))?;
                let mut message = built.message.clone();
                message.recent_blockhash = blockhash;
                let unsigned = Transaction::new_unsigned(message);
                result.simulation = Some(
                    self.cluster
                        .simulate(&unsigned, false)
                        .map_err(|error| rpc_err("simulateTransaction", error))?,
                );
            }
            return Ok(result);
        }
        let blockhash = self
            .cluster
            .blockhash()
            .map_err(|error| rpc_err("getLatestBlockhash", error))?;
        let tx = sign_checked(&step.label, &built, signers, blockhash)?;
        let simulation = self
            .cluster
            .simulate(&tx, true)
            .map_err(|error| rpc_err("simulateTransaction", error))?;
        if let Some(error) = &simulation.error {
            let tail: Vec<&String> = simulation.logs.iter().rev().take(6).collect();
            return refuse(format!(
                "{}: simulateTransaction failed ({error}); nothing was sent. Last logs: {:?}",
                step.label,
                tail.into_iter().rev().collect::<Vec<_>>()
            ));
        }
        result.simulation = Some(simulation);
        let sent = self.send_signed(&step.label, &tx)?;
        result.executed = true;
        result.signature = Some(sent.signature);
        progress(format!("{} -> {}", step.label, sent.signature));
        Ok(result)
    }
}

fn key_list(keys: &[Pubkey]) -> Vec<String> {
    keys.iter().map(Pubkey::to_string).collect()
}

fn check(name: impl Into<String>, ok: bool, expected: impl ToString, actual: impl ToString) -> Value {
    json!({
        "name": name.into(),
        "ok": ok,
        "expected": expected.to_string(),
        "actual": actual.to_string(),
    })
}

fn checks_ok(checks: &[Value]) -> bool {
    checks.iter().all(|entry| entry["ok"].as_bool() == Some(true))
}

// ---------------------------------------------------------------------------
// Inner instructions and step builders
// ---------------------------------------------------------------------------

fn loader_id() -> Pubkey {
    solana_sdk_ids::bpf_loader_upgradeable::id()
}

/// `["<program id>"]` under the upgradeable loader.
pub fn programdata_address(program_id: &Pubkey) -> Pubkey {
    solana_loader_v3_interface::get_program_data_address(program_id)
}

/// The `initialize` arguments of the devnet acceptance run (packet 3c / A4): the provider tuple is
/// the live devnet ORAO tuple, the request fee equals the live devnet fee.
pub fn launch_args(
    opts: &Opts,
    sponsor: Pubkey,
    management_recipient: Pubkey,
) -> Result<InitializeArgs, OpsError> {
    let cluster_domain = match opts.get("cluster-domain") {
        None => DEVNET_CLUSTER_DOMAIN,
        Some(text) => {
            let bytes = text.as_bytes();
            if bytes.len() != 32 {
                return usage("--cluster-domain must be exactly 32 bytes of text");
            }
            bytes.try_into().expect("32 bytes")
        }
    };
    let headroom = match opts.u64_flag("provider-fee-headroom-bps")? {
        None => 2_500,
        Some(value) => u16::try_from(value)
            .map_err(|_| OpsError::Usage("--provider-fee-headroom-bps must fit in u16".into()))?,
    };
    Ok(InitializeArgs {
        sponsor,
        management_recipient,
        cluster_domain,
        provider_request_fee_lamports: opts
            .u64_flag("provider-request-fee-lamports")?
            .unwrap_or(300_000),
        provider_fee_headroom_bps: headroom,
        provider_program: orao::id(),
        provider_network_state: orao::state::network_state::NetworkState::find_address(&orao::id()).0,
        provider_treasury: Pubkey::from_str(sq::DEVNET_ORAO_TREASURY).expect("treasury"),
        provider_client: provider::client_address().0,
        public_series_a_enabled: opts.bool_flag("public-series-a-enabled", true)?,
    })
}

/// The program's `initialize` instruction (mirror of `controls::initialize_accounts`), with the
/// G vault as the governance signer and the sponsor as a keypair signer.
pub fn initialize_inner(g: &Identity, p: &Identity, args: InitializeArgs) -> Instruction {
    anchor_ix(
        accounts::Initialize {
            sponsor: args.sponsor,
            pool: pda(&[state::POOL_SEED]).0,
            admission: pda(&[state::ADMISSION_SEED]).0,
            game_vault: pda(&[state::GAME_VAULT_SEED]).0,
            ops: pda(&[state::OPS_SEED]).0,
            ticket: pda(&[state::TICKET_SEED]).0,
            rng_authority: pda(&[state::RNG_AUTHORITY_SEED]).0,
            config: pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).0,
            ops_cash: pda(&[state::OPS_CASH_SEED]).0,
            squads_program: SQUADS_PROGRAM_ID,
            governance_multisig: g.multisig,
            governance: g.vault,
            guardian_multisig: p.multisig,
            guardian: p.vault,
            self_program: ID,
            program_data: programdata_address(&ID),
            system_program: system_program::ID,
        },
        instruction::Initialize { args },
    )
}

/// ORAO `register` with the G vault as payer (it must be the program's upgrade authority),
/// registering this program's `RngAuthority` PDA (mirror of `squads_real::orao_register_instruction`).
pub fn register_inner(g: &Identity) -> Instruction {
    let (rng_authority, rng_bump) = provider::rng_authority_address();
    crate::close::foreign_ix(
        orao::id(),
        orao::accounts::Register {
            payer: g.vault,
            program: ID,
            program_data: programdata_address(&ID),
            state: rng_authority,
            client: provider::client_address().0,
            network_state: orao::state::network_state::NetworkState::find_address(&orao::id()).0,
            system_program: system_program::ID,
        },
        orao::instruction::Register {
            params: orao::RegisterParams::new(vec![
                state::RNG_AUTHORITY_SEED.to_vec(),
                vec![rng_bump],
            ]),
        },
    )
}

fn step_squads_create(
    id: &str,
    label: &str,
    treasury: Pubkey,
    creator: Pubkey,
    identity: &Identity,
    members: &[Pubkey],
    time_lock: u32,
) -> Step {
    let specs: Vec<(Pubkey, u8)> = members.iter().map(|key| (*key, sq::PERMISSION_ALL)).collect();
    let (instruction, derived) = sq::multisig_create_v2_ix(
        treasury,
        creator,
        identity.create_key,
        &specs,
        sq::THRESHOLD,
        time_lock,
    );
    debug_assert_eq!(derived, identity.multisig);
    Step {
        id: id.into(),
        label: label.into(),
        instructions: vec![instruction],
        payer: creator,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    }
}

fn step_create_buffer(
    deployer: Pubkey,
    buffer: Pubkey,
    lamports: u64,
    elf_len: usize,
) -> Result<Step, OpsError> {
    let instructions = loader_instruction::create_buffer(&deployer, &buffer, &deployer, lamports, elf_len)
        .map_err(|error| OpsError::Failed(format!("create_buffer: {error}")))?;
    Ok(Step {
        id: "A3.create_buffer".into(),
        label: "loader-v3 create buffer (system create_account + InitializeBuffer)".into(),
        instructions,
        payer: deployer,
        cu_limit: None,
        depends_on_previous: false,
    })
}

fn step_write(deployer: Pubkey, buffer: Pubkey, offset: u32, bytes: Vec<u8>) -> Step {
    Step {
        id: "A3.write".into(),
        label: format!("loader-v3 write {} bytes at offset {offset}", bytes.len()),
        instructions: vec![loader_instruction::write(&buffer, &deployer, offset, bytes)],
        payer: deployer,
        cu_limit: None,
        depends_on_previous: true,
    }
}

#[allow(deprecated)] // loader-v3 is the packet's deployment path; loader-v4 is out of scope.
fn step_deploy(
    deployer: Pubkey,
    program: Pubkey,
    buffer: Pubkey,
    program_lamports: u64,
    max_data_len: usize,
) -> Result<Step, OpsError> {
    let instructions = loader_instruction::deploy_with_max_program_len(
        &deployer,
        &program,
        &buffer,
        &deployer,
        program_lamports,
        max_data_len,
    )
    .map_err(|error| OpsError::Failed(format!("deploy_with_max_program_len: {error}")))?;
    Ok(Step {
        id: "A3.deploy".into(),
        label: format!("loader-v3 DeployWithMaxDataLen (max_data_len {max_data_len})"),
        instructions,
        payer: deployer,
        cu_limit: Some(CU_DEPLOY),
        depends_on_previous: true,
    })
}

fn step_set_authority(program: Pubkey, deployer: Pubkey, new_authority: Pubkey) -> Step {
    Step {
        id: "A3.set_upgrade_authority".into(),
        label: "loader-v3 SetAuthority: upgrade authority -> G vault (unchecked: a PDA cannot co-sign)"
            .into(),
        instructions: vec![loader_instruction::set_upgrade_authority(
            &program,
            &deployer,
            Some(&new_authority),
        )],
        payer: deployer,
        cu_limit: None,
        depends_on_previous: true,
    }
}

fn step_transfer(payer: Pubkey, to: Pubkey, lamports: u64) -> Step {
    Step {
        id: "A5.fund_vault".into(),
        label: format!("system transfer {lamports} lamports to the G vault"),
        instructions: vec![system_instruction::transfer(&payer, &to, lamports)],
        payer,
        cu_limit: None,
        depends_on_previous: false,
    }
}

/// `vault_transaction_create` + `proposal_create`.  The proposer is ALWAYS creator, rent payer and
/// fee payer: that is the only arrangement whose `vault_transaction_create` fits the 1,232-byte
/// packet (1,140 bytes measured; a separate payer adds a signature and a key, about 1,235).
fn steps_propose(g: &Identity, member: Pubkey, index: u64, message: &[u8]) -> (Step, Step) {
    let create = Step {
        id: "A4.vault_transaction_create".into(),
        label: format!("squads vault_transaction_create #{index} (proposer = fee payer)"),
        instructions: vec![sq::vault_transaction_create_ix(
            g.multisig, index, member, member, 0, 0, message,
        )],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    };
    let proposal = Step {
        id: "A4.proposal_create".into(),
        label: format!("squads proposal_create #{index}"),
        instructions: vec![sq::proposal_create_ix(g.multisig, index, member, member, false)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: true,
    };
    (create, proposal)
}

fn step_approve(g: &Identity, member: Pubkey, index: u64) -> Step {
    Step {
        id: "A4.proposal_approve".into(),
        label: format!("squads proposal_approve #{index} by {member}"),
        instructions: vec![sq::proposal_approve_ix(g.multisig, index, member)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: true,
    }
}

fn step_execute(g: &Identity, member: Pubkey, index: u64, inner: &Instruction) -> Step {
    let compiled = sq::compile_vault_message(inner);
    Step {
        id: "A4.vault_transaction_execute".into(),
        label: format!("squads vault_transaction_execute #{index}"),
        instructions: vec![sq::vault_transaction_execute_ix(
            g.multisig,
            index,
            member,
            compiled.remaining_accounts(&g.vault, &[]),
        )],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: true,
    }
}

/// The largest chunk whose loader `Write` transaction (deployer pays and signs) fits the packet.
pub fn max_write_chunk(deployer: &Pubkey, buffer: &Pubkey) -> usize {
    let size_with =
        |len: usize| build_step(&step_write(*deployer, *buffer, 0, vec![0u8; len])).packet_bytes;
    let mut chunk = PACKET_LIMIT.saturating_sub(size_with(0)).saturating_sub(4);
    while chunk > 0 && size_with(chunk) > PACKET_LIMIT {
        chunk -= 1;
    }
    chunk
}

// ---------------------------------------------------------------------------
// Shared reads and checks
// ---------------------------------------------------------------------------

impl<'a> Ctx<'a> {
    fn cluster_json(&self, genesis: &str) -> Value {
        json!({
            "rpc": self.gate.rpc_url,
            "target": match self.gate.target { Target::Local => "local", Target::Devnet => "devnet" },
            "genesis_hash": genesis,
            "slot": self.cluster.slot().ok(),
            "commitment": format!("{:?}", self.gate.commitment.commitment),
        })
    }

    fn g_identity(&self) -> Result<Identity, OpsError> {
        Ok(identity(key_flag(self.opts, "g-create-key")?.pubkey))
    }

    fn p_identity(&self) -> Result<Identity, OpsError> {
        Ok(identity(key_flag(self.opts, "p-create-key")?.pubkey))
    }
}

/// Packet 3b: the expected on-chain profile of a multisig.
pub struct ProfileSpec<'a> {
    pub name: &'a str,
    pub identity: Identity,
    pub members: &'a [Pubkey],
    pub time_lock: u32,
}

/// Check an on-chain multisig account against its profile (G: 2-of-3, 172,800 s; P: 2-of-3, 0 s;
/// autonomous, no rent collector, exactly the three listed members each with Initiate|Vote|Execute).
pub fn profile_checks(
    spec: &ProfileSpec<'_>,
    account: Option<&Account>,
) -> (Vec<Value>, Option<sq::MultisigView>) {
    let name = spec.name;
    let Some(account) = account else {
        return (
            vec![check(format!("{name}.multisig_exists"), false, spec.identity.multisig, "absent")],
            None,
        );
    };
    let mut checks = vec![check(
        format!("{name}.multisig_owner_is_squads"),
        account.owner == SQUADS_PROGRAM_ID,
        SQUADS_PROGRAM_ID,
        account.owner,
    )];
    let view = match decode_multisig(&account.data) {
        Ok(view) => view,
        Err(error) => {
            checks.push(check(format!("{name}.multisig_decodes"), false, "a Multisig", error));
            return (checks, None);
        }
    };
    checks.push(check(
        format!("{name}.create_key"),
        view.create_key == spec.identity.create_key,
        spec.identity.create_key,
        view.create_key,
    ));
    checks.push(check(
        format!("{name}.config_authority_is_zero"),
        view.config_authority == Pubkey::default(),
        Pubkey::default(),
        view.config_authority,
    ));
    checks.push(check(format!("{name}.threshold"), view.threshold == sq::THRESHOLD, sq::THRESHOLD, view.threshold));
    checks.push(check(format!("{name}.time_lock_seconds"), view.time_lock == spec.time_lock, spec.time_lock, view.time_lock));
    checks.push(check(
        format!("{name}.rent_collector_none"),
        view.rent_collector.is_none(),
        "none",
        view.rent_collector.map_or("none".to_string(), |key| key.to_string()),
    ));
    let mut expected: Vec<(Pubkey, u8)> = spec
        .members
        .iter()
        .map(|key| (*key, SQUADS_ALL_PERMISSIONS))
        .collect();
    expected.sort_by_key(|(key, _)| *key);
    let mut actual = view.members.clone();
    actual.sort_by_key(|(key, _)| *key);
    checks.push(check(
        format!("{name}.members_and_permissions"),
        actual == expected,
        format!("{expected:?}"),
        format!("{actual:?}"),
    ));
    let vault = fixtures::squads::vault_address(&spec.identity.multisig, 0).0;
    checks.push(check(format!("{name}.vault_index_0"), vault == spec.identity.vault, spec.identity.vault, vault));
    (checks, Some(view))
}

/// What the cluster holds for a program id.
pub struct ProgramLookup {
    pub program_id: Pubkey,
    pub programdata: Pubkey,
    pub program: Option<Account>,
    pub programdata_account: Option<Account>,
}

fn lookup_program(ctx: &Ctx<'_>, program_id: &Pubkey) -> Result<ProgramLookup, OpsError> {
    let programdata = programdata_address(program_id);
    Ok(ProgramLookup {
        program_id: *program_id,
        programdata,
        program: ctx.account(program_id)?,
        programdata_account: ctx.account(&programdata)?,
    })
}

/// Checks of the loader-v3 Program account and the ProgramData account it points at.
fn program_checks(lookup: &ProgramLookup) -> Vec<Value> {
    let Some(program) = &lookup.program else {
        return vec![check("program.account_exists", false, lookup.program_id, "absent")];
    };
    let mut checks = vec![
        check("program.owner_is_loader_v3", program.owner == loader_id(), loader_id(), program.owner),
        check("program.executable", program.executable, true, program.executable),
    ];
    match decode_program_account(&program.data) {
        Ok(pointer) => checks.push(check(
            "program.points_at_derived_programdata",
            pointer == lookup.programdata,
            lookup.programdata,
            pointer,
        )),
        Err(error) => checks.push(check("program.decodes", false, "Program state", error)),
    }
    match &lookup.programdata_account {
        None => checks.push(check("programdata.exists", false, lookup.programdata, "absent")),
        Some(account) => checks.push(check(
            "programdata.owner_is_loader_v3",
            account.owner == loader_id(),
            loader_id(),
            account.owner,
        )),
    }
    checks
}

fn read_pinned_elf(opts: &Opts, gate: &Gate) -> Result<(Vec<u8>, String), OpsError> {
    let path = opts.require("elf")?;
    let elf = fs::read(path).map_err(|error| OpsError::Failed(format!("{path}: {error}")))?;
    if elf.len() < MIN_ELF_BYTES || elf.len() > MAX_PROGRAM_BYTES {
        return refuse(format!("{path} is {} bytes, not a plausible program ELF", elf.len()));
    }
    if &elf[..4] != b"\x7fELF" {
        return refuse(format!("{path} does not start with the ELF magic"));
    }
    let digest = devnet::sha256_hex(&elf);
    let pinned = match opts.get("expected-sha256") {
        None => PINNED_PROGRAM_ELF_SHA256.to_string(),
        Some(value) => {
            if gate.target != Target::Local {
                return refuse("--expected-sha256 overrides the pin only on a local cluster");
            }
            let value = value.to_ascii_lowercase();
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return usage("--expected-sha256 must be 64 hex digits");
            }
            value
        }
    };
    if digest != pinned {
        return refuse(format!(
            "{path} hashes to {digest}, not the pinned {pinned}; nothing was sent"
        ));
    }
    Ok((elf, digest))
}

/// `(creation fee, treasury)` of the Squads `ProgramConfig` account.
pub fn decode_program_config(data: &[u8]) -> Result<(u64, Pubkey), String> {
    let mut reader = Reader::new(data);
    reader.take(8)?;
    let _authority = reader.key()?;
    let fee = reader.u64()?;
    let treasury = reader.key()?;
    Ok((fee, treasury))
}

fn member_has(view: &sq::MultisigView, member: &Pubkey, permission: u8) -> bool {
    view.members
        .iter()
        .any(|(key, mask)| key == member && mask & permission == permission)
}

/// The G multisig as the commands need it: profile verified (refusing on any failed check).
fn require_g_profile(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
) -> Result<sq::MultisigView, OpsError> {
    let account = ctx.account(&g.multisig)?;
    let (checks, view) = profile_checks(
        &ProfileSpec {
            name: "G",
            identity: *g,
            members,
            time_lock: sq::G_TIME_LOCK,
        },
        account.as_ref(),
    );
    match view {
        Some(view) if checks_ok(&checks) => Ok(view),
        _ => refuse(format!(
            "the G multisig {} does not match the expected profile: {}",
            g.multisig,
            serde_json::to_string(&checks.iter().filter(|c| c["ok"] != json!(true)).collect::<Vec<_>>())
                .unwrap_or_default()
        )),
    }
}

// ---------------------------------------------------------------------------
// verify-bytes (packet A8, read-only)
// ---------------------------------------------------------------------------

fn expected_authority(ctx: &Ctx<'_>, spec: &str) -> Result<Option<Pubkey>, OpsError> {
    match spec {
        "none" => Ok(None),
        "g-vault" => Ok(Some(ctx.g_identity()?.vault)),
        other => Ok(Some(KeyFile::load(other)?.pubkey)),
    }
}

fn cmd_verify_bytes(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let (elf, elf_sha) = read_pinned_elf(ctx.opts, ctx.gate)?;
    let program_id = match ctx.opts.get("program-id") {
        None => ID,
        Some(text) => Pubkey::from_str(text)
            .map_err(|_| OpsError::Usage("--program-id must be a base58 pubkey".into()))?,
    };
    let lookup = lookup_program(ctx, &program_id)?;
    let mut checks = program_checks(&lookup);
    let mut report_json = Value::Null;
    if let Some(account) = &lookup.programdata_account {
        match verify_programdata(&account.data, &elf) {
            Ok(report) => {
                checks.push(check(
                    "programdata.elf_bytes_equal_pinned_elf",
                    report.elf_matches,
                    format!("data[45..45+{}] == ELF {elf_sha}", report.elf_len),
                    format!("first mismatch {:?}", report.first_mismatch),
                ));
                checks.push(check(
                    "programdata.padding_all_zero",
                    report.padding_all_zero,
                    "all zero",
                    format!("{} padding bytes", report.padding_len),
                ));
                if let Some(spec) = ctx.opts.get("expect-authority") {
                    let want = expected_authority(ctx, spec)?;
                    checks.push(check(
                        "programdata.upgrade_authority",
                        report.header.authority == want,
                        want.map_or("none".to_string(), |key| key.to_string()),
                        report
                            .header
                            .authority
                            .map_or("none".to_string(), |key| key.to_string()),
                    ));
                }
                report_json = report.to_json();
            }
            Err(error) => checks.push(check("programdata.header_decodes", false, "ProgramData header", error)),
        }
    }
    let all_ok = checks_ok(&checks);
    Ok(json!({
        "command": "verify-bytes",
        "mode": "read-only",
        "program_id": program_id.to_string(),
        "programdata_address": lookup.programdata.to_string(),
        "elf_sha256": elf_sha,
        "elf_len": elf.len(),
        "programdata": report_json,
        "checks": checks,
        "all_ok": all_ok,
    }))
}

// ---------------------------------------------------------------------------
// squads-create (packet A2)
// ---------------------------------------------------------------------------

fn cmd_squads_create(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let payer = key_flag(opts, "payer")?;
    let g_key = key_flag(opts, "g-create-key")?;
    let p_key = key_flag(opts, "p-create-key")?;
    let g_members = read_members(opts.require("g-members")?)?;
    let p_members = read_members(opts.require("p-members")?)?;
    let g = identity(g_key.pubkey);
    let p = identity(p_key.pubkey);
    if g.create_key == p.create_key {
        return refuse("the G and P create keys must differ");
    }
    let config = ctx.account(&sq::program_config_address())?.ok_or_else(|| {
        OpsError::Refused("the Squads ProgramConfig account is absent: Squads is not deployed on this cluster".into())
    })?;
    if config.owner != SQUADS_PROGRAM_ID {
        return refuse(format!("ProgramConfig is owned by {}, not Squads", config.owner));
    }
    let (creation_fee, treasury) = decode_program_config(&config.data)
        .map_err(|error| OpsError::Failed(format!("ProgramConfig: {error}")))?;
    let max_fee = opts.u64_flag("max-creation-fee")?.unwrap_or(0);
    if creation_fee > max_fee {
        return refuse(format!(
            "Squads charges {creation_fee} lamports per multisig, above --max-creation-fee {max_fee}"
        ));
    }
    let overlap: Vec<String> = g_members
        .iter()
        .filter(|key| p_members.contains(key))
        .map(Pubkey::to_string)
        .collect();

    let mut multisigs = Vec::new();
    let mut steps = Vec::new();
    let mut all_ok = true;
    for (name, ident, members, time_lock) in [
        ("G", g, &g_members, sq::G_TIME_LOCK),
        ("P", p, &p_members, sq::P_TIME_LOCK),
    ] {
        let spec = ProfileSpec {
            name,
            identity: ident,
            members,
            time_lock,
        };
        let existing = ctx.account(&ident.multisig)?;
        if existing.is_some() {
            let (checks, _) = profile_checks(&spec, existing.as_ref());
            if !checks_ok(&checks) {
                return refuse(format!(
                    "{name} multisig {} already exists with a DIFFERENT profile; refusing: {}",
                    ident.multisig,
                    serde_json::to_string(&checks).unwrap_or_default()
                ));
            }
            progress(format!("{name} multisig {} already exists with the expected profile", ident.multisig));
            multisigs.push(json!({
                "name": name, "multisig": ident.multisig.to_string(), "vault_0": ident.vault.to_string(),
                "created_by_this_run": false, "checks": checks,
            }));
            continue;
        }
        let step = step_squads_create(
            &format!("A2.multisig_create_v2.{name}"),
            &format!("squads multisig_create_v2 {name} (2-of-3, time lock {time_lock} s)"),
            treasury,
            payer.pubkey,
            &ident,
            members,
            time_lock,
        );
        let signers: Vec<&Keypair> = if ctx.executing() {
            vec![payer.keypair()?, g_or_p_key(&g_key, &p_key, name)?.keypair()?]
        } else {
            Vec::new()
        };
        let result = ctx.run_step(&step, &signers)?;
        let mut entry = json!({
            "name": name, "multisig": ident.multisig.to_string(), "vault_0": ident.vault.to_string(),
            "created_by_this_run": result.executed,
            "rent_lamports_estimate": ctx.rent(MULTISIG_ACCOUNT_BYTES)?,
        });
        if result.executed {
            let (checks, _) = profile_checks(&spec, ctx.account(&ident.multisig)?.as_ref());
            all_ok &= checks_ok(&checks);
            entry["checks"] = json!(checks);
        }
        multisigs.push(entry);
        steps.push(result.to_json());
    }
    Ok(json!({
        "command": "squads-create",
        "mode": ctx.mode(),
        "squads_creation_fee_lamports": creation_fee,
        "squads_treasury": treasury.to_string(),
        "creator_and_fee_payer": payer.pubkey.to_string(),
        "g_and_p_member_overlap": overlap,
        "multisigs": multisigs,
        "steps": steps,
        "all_ok": all_ok,
    }))
}

fn g_or_p_key<'k>(g: &'k KeyFile, p: &'k KeyFile, name: &str) -> Result<&'k KeyFile, OpsError> {
    match name {
        "G" => Ok(g),
        "P" => Ok(p),
        other => fail(format!("internal: unknown multisig {other}")),
    }
}

// ---------------------------------------------------------------------------
// fund-vault
// ---------------------------------------------------------------------------

fn cmd_fund_vault(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let payer = key_flag(opts, "payer")?;
    let g = ctx.g_identity()?;
    let lamports = opts
        .u64_flag("lamports")?
        .ok_or_else(|| OpsError::Usage("`fund-vault` needs --lamports".into()))?;
    if lamports == 0 || lamports > VAULT_FUNDING_CAP_LAMPORTS {
        return refuse(format!(
            "--lamports {lamports} is outside 1..={VAULT_FUNDING_CAP_LAMPORTS} (packet 4b: the vault needs about 0.02 SOL)"
        ));
    }
    let multisig = ctx.account(&g.multisig)?;
    if multisig.as_ref().is_none_or(|account| account.owner != SQUADS_PROGRAM_ID) {
        return refuse(format!(
            "the G multisig {} does not exist (or is not Squads-owned): refusing to fund an unverified vault",
            g.multisig
        ));
    }
    let step = step_transfer(payer.pubkey, g.vault, lamports);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![payer.keypair()?] } else { Vec::new() };
    let result = ctx.run_step(&step, &signers)?;
    Ok(json!({
        "command": "fund-vault",
        "mode": ctx.mode(),
        "g_vault": g.vault.to_string(),
        "vault_balance_after": ctx.balance(&g.vault)?,
        "steps": [result.to_json()],
        "all_ok": true,
    }))
}

// ---------------------------------------------------------------------------
// deploy (packet A3)
// ---------------------------------------------------------------------------

fn cmd_deploy(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let (elf, elf_sha) = read_pinned_elf(opts, ctx.gate)?;
    let deployer = key_flag(opts, "deployer")?;
    let program = key_flag(opts, "program-keypair")?;
    let buffer = key_flag(opts, "buffer-keypair")?;
    let g_key = key_flag(opts, "g-create-key")?;
    let g_members = read_members(opts.require("g-members")?)?;
    let g = identity(g_key.pubkey);

    if program.pubkey != ID
        && !(ctx.gate.target == Target::Local && opts.switch("allow-program-id-mismatch"))
    {
        return refuse(format!(
            "the program keypair is {} but the compiled-in program id is {ID}; a different id needs a rebuilt \
             ELF and re-pinned hashes (packet 5a). --allow-program-id-mismatch exists on a local cluster only",
            program.pubkey
        ));
    }
    let distinct: BTreeSet<Pubkey> = [deployer.pubkey, program.pubkey, buffer.pubkey].into_iter().collect();
    if distinct.len() != 3 {
        return refuse("the deployer, program and buffer keypairs must be three different keys");
    }
    let elf_len = elf.len();
    let max_data_len = match opts.u64_flag("max-data-len")? {
        None => elf_len,
        Some(value) => usize::try_from(value).map_err(|_| OpsError::Usage("--max-data-len".into()))?,
    };
    if max_data_len < elf_len || max_data_len > MAX_PROGRAM_BYTES {
        return refuse(format!(
            "--max-data-len {max_data_len} must be at least the ELF length {elf_len} (and at most {MAX_PROGRAM_BYTES})"
        ));
    }
    let buffer_rent = ctx.rent(BUFFER_HEADER_LEN + elf_len)?;
    let program_rent = ctx.rent(UpgradeableLoaderState::size_of_program())?;
    let programdata_rent = ctx.rent(PROGRAMDATA_HEADER_LEN + max_data_len)?;
    let chunk = max_write_chunk(&deployer.pubkey, &buffer.pubkey);
    if chunk == 0 {
        return fail("cannot fit any ELF bytes in a write transaction");
    }
    let write_count = elf_len.div_ceil(chunk);
    let fee_estimate = (write_count as u64 + 6) * SIGNATURE_FEE_LAMPORTS * 2;
    let conservative_peak = buffer_rent + program_rent + programdata_rent + fee_estimate;
    let cap = opts.u64_flag("max-spend-lamports")?.unwrap_or(DEPLOY_SPEND_CAP_LAMPORTS);
    if conservative_peak > cap {
        return refuse(format!(
            "the deployment's peak lamports {conservative_peak} exceed the cap {cap} (--max-spend-lamports; packet 4c: 15 SOL)"
        ));
    }

    let lookup = lookup_program(ctx, &program.pubkey)?;
    let buffer_account = ctx.account(&buffer.pubkey)?;
    let deployer_balance = ctx.balance(&deployer.pubkey)?;
    let mut notes: Vec<String> = Vec::new();
    if max_data_len > elf_len {
        notes.push(format!(
            "max_data_len exceeds the ELF by {} bytes: that padding is paid in rent and is permanent",
            max_data_len - elf_len
        ));
    }
    if devnet::elf64_extent(&elf) != Ok(elf_len) {
        notes.push("the ELF's own section/segment table does not end exactly at the file length".into());
    }

    let program_exists = lookup.program.is_some();
    let mut buffer_existing: Option<Vec<u8>> = None;
    if !program_exists {
        if let Some(account) = &buffer_account {
            if account.owner != loader_id() {
                return refuse(format!("the buffer address {} exists and is not loader-owned", buffer.pubkey));
            }
            match decode_buffer_authority(&account.data) {
                Ok(Some(authority)) if authority == deployer.pubkey => {}
                Ok(other) => {
                    return refuse(format!(
                        "the existing buffer's authority is {other:?}, not the deployer {}",
                        deployer.pubkey
                    ))
                }
                Err(error) => return refuse(format!("the existing buffer account is not a Buffer: {error}")),
            }
            if account.data.len() != BUFFER_HEADER_LEN + elf_len {
                return refuse(format!(
                    "the existing buffer is {} bytes, expected {}",
                    account.data.len(),
                    BUFFER_HEADER_LEN + elf_len
                ));
            }
            buffer_existing = Some(account.data[BUFFER_HEADER_LEN..].to_vec());
        }
    } else {
        let program_state = program_checks(&lookup);
        if !checks_ok(&program_state) {
            return refuse(format!(
                "the program account exists but is not a healthy loader-v3 program: {}",
                serde_json::to_string(&program_state).unwrap_or_default()
            ));
        }
    }
    let funds_needed = if program_exists {
        0
    } else {
        (if buffer_existing.is_some() { 0 } else { buffer_rent }) + program_rent + programdata_rent + fee_estimate
    };
    if !program_exists && deployer_balance < funds_needed {
        let message = format!(
            "the deployer holds {deployer_balance} lamports but this deployment needs {funds_needed} (buffer {buffer_rent} + \
             program {program_rent} + programdata {programdata_rent} + fees {fee_estimate}); fund it first so no buffer is stranded"
        );
        if ctx.executing() {
            return refuse(message);
        }
        notes.push(format!("a real run would refuse: {message}"));
    }

    let mut steps = Vec::new();
    let mut writes_summary = json!(null);
    let mut stage_log: Vec<String> = Vec::new();

    if !program_exists {
        // 1. buffer
        if buffer_existing.is_none() {
            let step = step_create_buffer(deployer.pubkey, buffer.pubkey, buffer_rent, elf_len)?;
            let signers: Vec<&Keypair> = if ctx.executing() {
                vec![deployer.keypair()?, buffer.keypair()?]
            } else {
                Vec::new()
            };
            steps.push(ctx.run_step(&step, &signers)?.to_json());
            stage_log.push("created the buffer".into());
        } else {
            stage_log.push("resuming into the existing buffer".into());
        }
        // 2. chunked writes (idempotent: identical chunks are skipped, so a re-run resumes)
        let existing = buffer_existing.unwrap_or_else(|| vec![0u8; elf_len]);
        let mut sent = 0usize;
        let mut skipped = 0usize;
        let mut max_packet = 0usize;
        let mut first_signature = None;
        let mut last_signature = None;
        for (index, bytes) in elf.chunks(chunk).enumerate() {
            let offset = index * chunk;
            if existing[offset..offset + bytes.len()] == *bytes {
                skipped += 1;
                continue;
            }
            let step = step_write(deployer.pubkey, buffer.pubkey, offset as u32, bytes.to_vec());
            max_packet = max_packet.max(build_step(&step).packet_bytes);
            if !ctx.executing() {
                sent += 1;
                continue;
            }
            let result = ctx.run_step(&step, &[deployer.keypair()?])?;
            sent += 1;
            if first_signature.is_none() {
                first_signature = result.signature.map(|s| s.to_string());
            }
            last_signature = result.signature.map(|s| s.to_string());
            if sent.is_multiple_of(100) {
                progress(format!("wrote {sent} chunks ({skipped} identical skipped) of {write_count}"));
            }
        }
        writes_summary = json!({
            "chunk_bytes": chunk, "total_chunks": write_count, "to_write": sent, "skipped_identical": skipped,
            "max_packet_bytes": max_packet, "first_signature": first_signature, "last_signature": last_signature,
            "executed": ctx.executing(),
        });
        if ctx.executing() {
            // 3. the buffer must now hold exactly the pinned ELF
            let buffer_now = ctx
                .account(&buffer.pubkey)?
                .ok_or_else(|| OpsError::Failed("the buffer disappeared".into()))?;
            if buffer_now.data.get(BUFFER_HEADER_LEN..) != Some(&elf[..]) {
                return fail("the buffer does not hold the pinned ELF after the writes; re-run `deploy` to rewrite the differing chunks");
            }
            stage_log.push("buffer verified byte-for-byte against the pinned ELF".into());
            // 4. deploy
            let step = step_deploy(deployer.pubkey, program.pubkey, buffer.pubkey, program_rent, max_data_len)?;
            steps.push(ctx.run_step(&step, &[deployer.keypair()?, program.keypair()?])?.to_json());
            stage_log.push("deployed".into());
        } else {
            let step = step_deploy(deployer.pubkey, program.pubkey, buffer.pubkey, program_rent, max_data_len)?;
            steps.push(ctx.run_step(&step, &[])?.to_json());
        }
    } else {
        stage_log.push("the program account already exists: skipping buffer, writes and deploy".into());
    }

    // 5. A8 first pass, BEFORE the authority moves
    let mut checks = Vec::new();
    let mut programdata_report = Value::Null;
    let mut authority_after = Value::Null;
    let g_checks = {
        let account = ctx.account(&g.multisig)?;
        profile_checks(
            &ProfileSpec { name: "G", identity: g, members: &g_members, time_lock: sq::G_TIME_LOCK },
            account.as_ref(),
        )
        .0
    };
    if ctx.executing() || program_exists {
        let fresh = lookup_program(ctx, &program.pubkey)?;
        checks.extend(program_checks(&fresh));
        let Some(programdata_account) = &fresh.programdata_account else {
            return fail("the ProgramData account is absent after the deploy");
        };
        let report = verify_programdata(&programdata_account.data, &elf)
            .map_err(|error| OpsError::Failed(format!("ProgramData header: {error}")))?;
        checks.push(check("programdata.elf_bytes_equal_pinned_elf", report.elf_matches, "equal", format!("{:?}", report.first_mismatch)));
        checks.push(check("programdata.padding_all_zero", report.padding_all_zero, "all zero", report.padding_len));
        programdata_report = report.to_json();
        if !report.ok() {
            return Ok(json!({
                "command": "deploy", "mode": ctx.mode(), "stopped": "deployed bytes differ from the pinned ELF; the upgrade authority was NOT transferred",
                "checks": checks, "programdata": programdata_report, "all_ok": false,
            }));
        }
        // 6. authority -> G vault
        match report.header.authority {
            Some(authority) if authority == g.vault => {
                stage_log.push("the upgrade authority is already the G vault".into());
            }
            Some(authority) if authority == deployer.pubkey => {
                if !checks_ok(&g_checks) {
                    return refuse(format!(
                        "the G multisig is not in place with the expected profile ({}); run `squads-create` first. The upgrade authority stays with the deployer.",
                        serde_json::to_string(&g_checks.iter().filter(|c| c["ok"] != json!(true)).collect::<Vec<_>>()).unwrap_or_default()
                    ));
                }
                let step = step_set_authority(program.pubkey, deployer.pubkey, g.vault);
                let signers: Vec<&Keypair> = if ctx.executing() { vec![deployer.keypair()?] } else { Vec::new() };
                steps.push(ctx.run_step(&step, &signers)?.to_json());
                stage_log.push("upgrade authority transferred to the G vault".into());
            }
            other => {
                return refuse(format!(
                    "the ProgramData upgrade authority is {other:?}: neither the deployer nor the G vault"
                ))
            }
        }
        if ctx.executing() {
            let after = lookup_program(ctx, &program.pubkey)?;
            let header = after
                .programdata_account
                .as_ref()
                .map(|account| decode_programdata_header(&account.data))
                .transpose()
                .map_err(|error| OpsError::Failed(format!("ProgramData header after SetAuthority: {error}")))?;
            let authority = header.and_then(|h| h.authority);
            checks.push(check(
                "programdata.upgrade_authority_is_g_vault",
                authority == Some(g.vault),
                g.vault,
                authority.map_or("none".to_string(), |key| key.to_string()),
            ));
            authority_after = json!(authority.map(|key| key.to_string()));
        }
    }
    let all_ok = checks_ok(&checks);
    Ok(json!({
        "command": "deploy",
        "mode": ctx.mode(),
        "program_id": program.pubkey.to_string(),
        "programdata_address": programdata_address(&program.pubkey).to_string(),
        "g_vault": g.vault.to_string(),
        "elf": { "sha256": elf_sha, "elf_len": elf_len, "max_data_len": max_data_len, "padding": max_data_len - elf_len },
        "rent_lamports": {
            "buffer": buffer_rent, "program": program_rent, "programdata": programdata_rent,
            "net_retained_after_buffer_refund": program_rent + programdata_rent,
            "conservative_peak_with_fees": conservative_peak, "cap": cap,
        },
        "deployer_balance_lamports": deployer_balance,
        "stages": stage_log,
        "writes": writes_summary,
        "steps": steps,
        "g_multisig_ready_for_authority_transfer": checks_ok(&g_checks),
        "upgrade_authority_after": authority_after,
        "programdata": programdata_report,
        "checks": checks,
        "notes": notes,
        "all_ok": all_ok,
    }))
}

// ---------------------------------------------------------------------------
// propose / approve / execute (packet A4)
// ---------------------------------------------------------------------------

/// ORAO `Client` account size (packet 4b: R(123)).
const ORAO_CLIENT_BYTES: usize = 123;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    Initialize,
    Register,
}

struct Action {
    kind: ActionKind,
    inner: Instruction,
    /// The keypair that must sign the OUTER `vault_transaction_execute` transaction.
    outer_signer: Option<Pubkey>,
    summary: Value,
}

fn args_json(args: &InitializeArgs) -> Value {
    json!({
        "sponsor": args.sponsor.to_string(),
        "management_recipient": args.management_recipient.to_string(),
        "cluster_domain_utf8": String::from_utf8_lossy(&args.cluster_domain),
        "cluster_domain_hex": args.cluster_domain.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "provider_request_fee_lamports": args.provider_request_fee_lamports,
        "provider_fee_headroom_bps": args.provider_fee_headroom_bps,
        "provider_program": args.provider_program.to_string(),
        "provider_network_state": args.provider_network_state.to_string(),
        "provider_treasury": args.provider_treasury.to_string(),
        "provider_client": args.provider_client.to_string(),
        "public_series_a_enabled": args.public_series_a_enabled,
    })
}

fn action_from_opts(ctx: &Ctx<'_>, g: &Identity) -> Result<Action, OpsError> {
    let name = ctx
        .opts
        .positional
        .first()
        .map(String::as_str)
        .ok_or_else(|| OpsError::Usage(format!("`{}` needs an action: initialize | register", ctx.opts.command)))?;
    if ctx.opts.positional.len() > 1 {
        return usage("only one action may be given");
    }
    match name {
        "initialize" => {
            let p = ctx.p_identity()?;
            let sponsor = key_flag(ctx.opts, "sponsor")?.pubkey;
            let management = key_flag(ctx.opts, "management-recipient")?.pubkey;
            let args = launch_args(ctx.opts, sponsor, management)?;
            let inner = initialize_inner(g, &p, args);
            Ok(Action {
                kind: ActionKind::Initialize,
                inner,
                outer_signer: Some(sponsor),
                summary: args_json(&args),
            })
        }
        "register" => Ok(Action {
            kind: ActionKind::Register,
            inner: register_inner(g),
            outer_signer: None,
            summary: json!({
                "orao_program": orao::id().to_string(),
                "payer_is_g_vault": g.vault.to_string(),
                "rng_authority": provider::rng_authority_address().0.to_string(),
                "client": provider::client_address().0.to_string(),
            }),
        }),
        other => usage(format!("unknown action `{other}` (initialize | register)")),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Propose,
    Approve,
    Execute,
}

/// State the action depends on.  `propose` is where a mistake costs a 48-hour cycle, so the
/// program-authority and not-already-done checks refuse there already.
fn action_preconditions(
    ctx: &Ctx<'_>,
    g: &Identity,
    action: &Action,
    stage: Stage,
) -> Result<Vec<Value>, OpsError> {
    let mut checks = Vec::new();
    let lookup = lookup_program(ctx, &ID)?;
    let authority = lookup
        .programdata_account
        .as_ref()
        .map(|account| decode_programdata_header(&account.data))
        .transpose()
        .map_err(|error| OpsError::Failed(format!("ProgramData header: {error}")))?
        .and_then(|header| header.authority);
    checks.push(check(
        "programdata.upgrade_authority_is_g_vault",
        authority == Some(g.vault),
        g.vault,
        authority.map_or("absent or immutable".to_string(), |key| key.to_string()),
    ));
    match action.kind {
        ActionKind::Initialize => {
            let pool = pda(&[state::POOL_SEED]).0;
            let existing = ctx.account(&pool)?;
            checks.push(check(
                "pool.not_yet_initialized",
                existing.as_ref().is_none_or(|account| account.data.is_empty()),
                "absent",
                existing.map_or("absent".to_string(), |_| format!("{pool} exists")),
            ));
        }
        ActionKind::Register => {
            let client = provider::client_address().0;
            let existing = ctx.account(&client)?;
            checks.push(check(
                "orao_client.not_yet_registered",
                existing.as_ref().is_none_or(|account| account.data.is_empty()),
                "absent",
                existing.map_or("absent".to_string(), |_| format!("{client} exists")),
            ));
            let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
            let network = ctx.account(&network_state)?;
            checks.push(check(
                "orao_network_state.exists_and_orao_owned",
                network.as_ref().is_some_and(|account| account.owner == orao::id()),
                orao::id(),
                network.map_or("absent".to_string(), |account| account.owner.to_string()),
            ));
            if stage == Stage::Execute {
                let rng = ctx.account(&provider::rng_authority_address().0)?;
                checks.push(check(
                    "rng_authority.created_by_initialize",
                    rng.as_ref().is_some_and(|account| account.owner == ID),
                    "program-owned (run `initialize` first)",
                    rng.map_or("absent".to_string(), |account| account.owner.to_string()),
                ));
                let needed = ctx.rent(ORAO_CLIENT_BYTES)? + ctx.rent(0)?;
                let have = ctx.balance(&g.vault)?;
                checks.push(check(
                    "g_vault.can_pay_the_orao_client_rent",
                    have >= needed,
                    format!(">= {needed} lamports (run `fund-vault`)"),
                    have,
                ));
            }
        }
    }
    if !checks_ok(&checks) {
        return refuse(format!(
            "precondition(s) for `{}` do not hold; nothing was sent: {}",
            ctx.opts.command,
            serde_json::to_string(&checks.iter().filter(|c| c["ok"] != json!(true)).collect::<Vec<_>>())
                .unwrap_or_default()
        ));
    }
    Ok(checks)
}

fn stored_message_len(message: &StoredMessage) -> usize {
    3 + (4 + 32 * message.account_keys.len())
        + 4
        + message
            .instructions
            .iter()
            .map(|ix| 1 + (4 + ix.account_indexes.len()) + (4 + ix.data.len()))
            .sum::<usize>()
        + 4
}

/// Estimated size of the `VaultTransaction` account for `inner` (8 + 32 + 32 + 8 + 3 bytes,
/// the empty ephemeral-signer vector, the stored message).
fn vault_transaction_bytes(inner: &Instruction) -> usize {
    8 + 32 + 32 + 8 + 3 + 4 + stored_message_len(&expected_stored_message(inner))
}

fn load_vault_transaction(
    ctx: &Ctx<'_>,
    g: &Identity,
    index: u64,
) -> Result<Option<(VaultTransactionView, Option<ProposalView>)>, OpsError> {
    let Some(account) = ctx.account(&sq::transaction_address(&g.multisig, index))? else {
        return Ok(None);
    };
    if account.owner != SQUADS_PROGRAM_ID {
        return refuse(format!("the vault transaction #{index} account is not Squads-owned"));
    }
    let view = decode_vault_transaction(&account.data)
        .map_err(|error| OpsError::Failed(format!("vault transaction #{index}: {error}")))?;
    let proposal = match ctx.account(&sq::proposal_address(&g.multisig, index))? {
        None => None,
        Some(account) => {
            if account.owner != SQUADS_PROGRAM_ID {
                return refuse(format!("the proposal #{index} account is not Squads-owned"));
            }
            Some(
                decode_proposal(&account.data)
                    .map_err(|error| OpsError::Failed(format!("proposal #{index}: {error}")))?,
            )
        }
    };
    Ok(Some((view, proposal)))
}

/// Approvers (and the executor) act only on a vault transaction that stores exactly the intended
/// message: the same accounts in the same order, the same instruction data.
fn load_checked_proposal(
    ctx: &Ctx<'_>,
    g: &Identity,
    index: u64,
    action: &Action,
) -> Result<(VaultTransactionView, ProposalView), OpsError> {
    let Some((vtx, proposal)) = load_vault_transaction(ctx, g, index)? else {
        return refuse(format!("there is no vault transaction #{index} on G {}", g.multisig));
    };
    let expected = expected_stored_message(&action.inner);
    if vtx.multisig != g.multisig || vtx.index != index || vtx.vault_index != 0 {
        return refuse(format!("vault transaction #{index} does not belong to G / vault 0"));
    }
    if !vtx.ephemeral_signer_bumps.is_empty() {
        return refuse(format!("vault transaction #{index} declares ephemeral signers; not the intended action"));
    }
    if vtx.message != expected {
        return refuse(format!(
            "vault transaction #{index} does NOT store the intended `{}` message (on-chain: {} accounts, {} instruction(s), data sha256 {}; \
             intended: {} accounts, data sha256 {}); not voting on / executing it",
            ctx.opts.positional.first().map(String::as_str).unwrap_or("?"),
            vtx.message.account_keys.len(),
            vtx.message.instructions.len(),
            vtx.message.instructions.first().map_or("-".to_string(), |ix| devnet::sha256_hex(&ix.data)),
            expected.account_keys.len(),
            devnet::sha256_hex(&action.inner.data),
        ));
    }
    let Some(proposal) = proposal else {
        return refuse(format!("vault transaction #{index} has no proposal yet (run `propose` to resume)"));
    };
    if proposal.multisig != g.multisig || proposal.transaction_index != index {
        return refuse(format!("the proposal at #{index} names another multisig or index"));
    }
    Ok((vtx, proposal))
}

fn is_live(status: &ProposalStatus) -> bool {
    matches!(
        status,
        ProposalStatus::Draft(_) | ProposalStatus::Active(_) | ProposalStatus::Approved(_) | ProposalStatus::Executing
    )
}

fn cmd_propose(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let g = ctx.g_identity()?;
    let members = read_members(opts.require("g-members")?)?;
    let view = require_g_profile(ctx, &g, &members)?;
    if !member_has(&view, &member.pubkey, SQUADS_INITIATE) {
        return refuse(format!("{} is not a G member with the Initiate permission", member.pubkey));
    }
    let action = action_from_opts(ctx, &g)?;
    let preconditions = action_preconditions(ctx, &g, &action, Stage::Propose)?;
    let compiled = sq::compile_vault_message(&action.inner);
    let message_sha256 = devnet::sha256_hex(&compiled.bytes);

    // A re-run must neither duplicate a live proposal nor leave a half-made one behind.
    let mut index = view.transaction_index + 1;
    let mut resume_proposal_only = false;
    if view.transaction_index >= 1 {
        if let Some((vtx, proposal)) = load_vault_transaction(ctx, &g, view.transaction_index)? {
            if vtx.message == expected_stored_message(&action.inner) {
                match proposal {
                    None => {
                        index = view.transaction_index;
                        resume_proposal_only = true;
                    }
                    Some(proposal) if is_live(&proposal.status) => {
                        return refuse(format!(
                            "an identical proposal already exists at #{} (status {}); use approve / execute",
                            view.transaction_index,
                            proposal.status.name()
                        ))
                    }
                    Some(_) => {}
                }
            }
        }
    }
    if ctx.executing() {
        let expected = opts.u64_flag("expect-tx-index")?.ok_or_else(|| {
            OpsError::Refused(format!(
                "--execute needs --expect-tx-index {index} (the index this run would take; a dry run prints it)"
            ))
        })?;
        if expected != index {
            return refuse(format!("--expect-tx-index {expected} but this run would take #{index}"));
        }
    }
    let (create, proposal_step) = steps_propose(&g, member.pubkey, index, &compiled.bytes);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![member.keypair()?] } else { Vec::new() };
    let mut steps = Vec::new();
    if !resume_proposal_only {
        steps.push(ctx.run_step(&create, &signers)?.to_json());
    }
    steps.push(ctx.run_step(&proposal_step, &signers)?.to_json());

    let mut checks = Vec::new();
    let mut status = json!("not created (dry run)");
    if ctx.executing() {
        let (vtx, proposal) = load_checked_proposal(ctx, &g, index, &action)?;
        checks.push(check("vault_transaction.stores_the_intended_message", true, "equal", "equal"));
        checks.push(check(
            "proposal.is_active",
            matches!(proposal.status, ProposalStatus::Active(_)),
            "Active",
            proposal.status.name(),
        ));
        checks.push(check("vault_transaction.creator_is_proposer", vtx.creator == member.pubkey, member.pubkey, vtx.creator));
        status = json!(proposal.status.name());
    }
    Ok(json!({
        "command": "propose",
        "mode": ctx.mode(),
        "action": opts.positional.first(),
        "multisig": g.multisig.to_string(),
        "g_vault": g.vault.to_string(),
        "transaction_index": index,
        "vault_transaction": sq::transaction_address(&g.multisig, index).to_string(),
        "proposal": sq::proposal_address(&g.multisig, index).to_string(),
        "proposer_is_creator_rent_payer_and_fee_payer": member.pubkey.to_string(),
        "message_bytes": compiled.bytes.len(),
        "message_sha256": message_sha256,
        "resumed_proposal_only": resume_proposal_only,
        "parameters": action.summary,
        "preconditions": preconditions,
        "rent_lamports_estimate": {
            "vault_transaction": ctx.rent(vault_transaction_bytes(&action.inner))?,
            "proposal": ctx.rent(PROPOSAL_ACCOUNT_BYTES)?,
        },
        "proposal_status": status,
        "steps": steps,
        "checks": checks,
        "all_ok": checks_ok(&checks),
    }))
}

fn cmd_approve(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let g = ctx.g_identity()?;
    let members = read_members(opts.require("g-members")?)?;
    let view = require_g_profile(ctx, &g, &members)?;
    if !member_has(&view, &member.pubkey, SQUADS_VOTE) {
        return refuse(format!("{} is not a G member with the Vote permission", member.pubkey));
    }
    let index = opts
        .u64_flag("tx-index")?
        .ok_or_else(|| OpsError::Usage("`approve` needs --tx-index".into()))?;
    let action = action_from_opts(ctx, &g)?;
    action_preconditions(ctx, &g, &action, Stage::Approve)?;
    let (_vtx, proposal) = load_checked_proposal(ctx, &g, index, &action)?;
    if !matches!(proposal.status, ProposalStatus::Active(_)) {
        return refuse(format!(
            "proposal #{index} is {}, not Active; nothing to approve",
            proposal.status.name()
        ));
    }
    if proposal.approved.contains(&member.pubkey) || proposal.rejected.contains(&member.pubkey) {
        return refuse(format!("{} has already voted on proposal #{index}", member.pubkey));
    }
    let step = step_approve(&g, member.pubkey, index);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![member.keypair()?] } else { Vec::new() };
    let result = ctx.run_step(&step, &signers)?;

    let mut after = json!(null);
    if ctx.executing() {
        let (_, proposal) = load_checked_proposal(ctx, &g, index, &action)?;
        let executable_after = match proposal.status {
            ProposalStatus::Approved(timestamp) => Some(timestamp + i64::from(view.time_lock)),
            _ => None,
        };
        after = json!({
            "status": proposal.status.name(),
            "approvals": proposal.approved.len(),
            "threshold": view.threshold,
            "approved_at": proposal.status.timestamp().filter(|_| matches!(proposal.status, ProposalStatus::Approved(_))),
            "executable_after_unix": executable_after,
            "time_lock_seconds": view.time_lock,
        });
    }
    Ok(json!({
        "command": "approve",
        "mode": ctx.mode(),
        "action": opts.positional.first(),
        "transaction_index": index,
        "member": member.pubkey.to_string(),
        "approvals_before": proposal.approved.len(),
        "after": after,
        "parameters": action.summary,
        "steps": [result.to_json()],
        "all_ok": true,
    }))
}

fn cmd_execute(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let g = ctx.g_identity()?;
    let members = read_members(opts.require("g-members")?)?;
    let view = require_g_profile(ctx, &g, &members)?;
    if !member_has(&view, &member.pubkey, SQUADS_EXECUTE) {
        return refuse(format!("{} is not a G member with the Execute permission", member.pubkey));
    }
    let index = opts
        .u64_flag("tx-index")?
        .ok_or_else(|| OpsError::Usage("`execute` needs --tx-index".into()))?;
    let action = action_from_opts(ctx, &g)?;
    let (_vtx, proposal) = load_checked_proposal(ctx, &g, index, &action)?;
    let approved_at = match proposal.status {
        ProposalStatus::Approved(timestamp) => timestamp,
        other => {
            return refuse(format!(
                "proposal #{index} is {}, not Approved ({} of {} approvals); it cannot execute",
                other.name(),
                proposal.approved.len(),
                view.threshold
            ))
        }
    };
    // Squads: `now - approved_at >= time_lock`, on the cluster's own clock.
    let now = ctx
        .cluster
        .unix_time()
        .map_err(|error| rpc_err("reading the cluster clock", error))?;
    let releases_at = approved_at + i64::from(view.time_lock);
    if now < releases_at {
        return refuse(format!(
            "time lock not released: approved at {approved_at}, lock {} s, releases at {releases_at}, cluster clock {now} \
             ({} s to go); executing now would fail with Squads error 6021 after paying a fee",
            view.time_lock,
            releases_at - now
        ));
    }
    let preconditions = action_preconditions(ctx, &g, &action, Stage::Execute)?;
    let compiled = sq::compile_vault_message(&action.inner);
    let outer_keys = compiled.outer_signer_keys(&g.vault, &[]);
    let sponsor_file;
    let mut signers: Vec<&Keypair> = Vec::new();
    if ctx.executing() {
        signers.push(member.keypair()?);
    }
    if !outer_keys.is_empty() {
        if outer_keys.as_slice() != action.outer_signer.as_slice() {
            return refuse(format!(
                "the message needs outer signer(s) {:?}, but this action expects {:?}",
                key_list(&outer_keys),
                action.outer_signer.map(|key| key.to_string())
            ));
        }
        if ctx.executing() {
            sponsor_file = key_flag(opts, "sponsor")?;
            signers.push(sponsor_file.keypair()?);
        }
    }
    let step = step_execute(&g, member.pubkey, index, &action.inner);
    let result = ctx.run_step(&step, &signers)?;

    let mut checks = Vec::new();
    if ctx.executing() {
        let (_, after) = load_checked_proposal(ctx, &g, index, &action)?;
        checks.push(check(
            "proposal.is_executed",
            matches!(after.status, ProposalStatus::Executed(_)),
            "Executed",
            after.status.name(),
        ));
    }
    Ok(json!({
        "command": "execute",
        "mode": ctx.mode(),
        "action": opts.positional.first(),
        "transaction_index": index,
        "approved_at": approved_at,
        "time_lock_releases_at": releases_at,
        "cluster_clock": now,
        "outer_signers": key_list(&outer_keys),
        "preconditions": preconditions,
        "steps": [result.to_json()],
        "checks": checks,
        "all_ok": checks_ok(&checks),
    }))
}

// ---------------------------------------------------------------------------
// readback (packet A6)
// ---------------------------------------------------------------------------

fn anchor<T: AccountDeserialize>(account: &Account) -> Result<T, String> {
    T::try_deserialize(&mut account.data.as_slice()).map_err(|error| error.to_string())
}

fn cmd_readback(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let g = ctx.g_identity()?;
    let p = ctx.p_identity()?;
    let g_members = read_members(opts.require("g-members")?)?;
    let p_members = read_members(opts.require("p-members")?)?;
    let sections: BTreeSet<String> = opts
        .get("sections")
        .unwrap_or("squads,program,initialize,orao")
        .split(',')
        .map(|part| part.trim().to_string())
        .collect();
    for section in &sections {
        if !["squads", "program", "initialize", "orao"].contains(&section.as_str()) {
            return usage(format!("--sections: unknown section `{section}`"));
        }
    }
    let mut checks: Vec<Value> = Vec::new();

    if sections.contains("squads") {
        for (name, ident, members, lock) in [
            ("G", g, &g_members, sq::G_TIME_LOCK),
            ("P", p, &p_members, sq::P_TIME_LOCK),
        ] {
            let account = ctx.account(&ident.multisig)?;
            let (list, _) = profile_checks(
                &ProfileSpec { name, identity: ident, members, time_lock: lock },
                account.as_ref(),
            );
            checks.extend(list);
        }
    }
    let mut program_json = Value::Null;
    if sections.contains("program") {
        let lookup = lookup_program(ctx, &ID)?;
        checks.extend(program_checks(&lookup));
        if let Some(account) = &lookup.programdata_account {
            match decode_programdata_header(&account.data) {
                Ok(header) => {
                    checks.push(check(
                        "programdata.upgrade_authority_is_g_vault",
                        header.authority == Some(g.vault),
                        g.vault,
                        header.authority.map_or("none".to_string(), |key| key.to_string()),
                    ));
                    program_json = json!({
                        "program_id": ID.to_string(),
                        "programdata": lookup.programdata.to_string(),
                        "programdata_slot": header.slot,
                        "account_data_len": account.data.len(),
                        "payload_sha256_after_header": devnet::sha256_hex(&account.data[PROGRAMDATA_HEADER_LEN..]),
                    });
                    if let Some(path) = opts.get("elf") {
                        let (elf, _) = read_pinned_elf(opts, ctx.gate).map_err(|error| {
                            OpsError::Refused(format!("--elf {path}: {error}"))
                        })?;
                        if let Ok(report) = verify_programdata(&account.data, &elf) {
                            checks.push(check("programdata.elf_bytes_equal_pinned_elf", report.elf_matches, "equal", format!("{:?}", report.first_mismatch)));
                            checks.push(check("programdata.padding_all_zero", report.padding_all_zero, "all zero", report.padding_len));
                        }
                    }
                }
                Err(error) => checks.push(check("programdata.header_decodes", false, "ProgramData header", error)),
            }
        }
    }
    let mut pool_json = Value::Null;
    if sections.contains("initialize") {
        let domain: [u8; 32] = match opts.get("cluster-domain") {
            None => DEVNET_CLUSTER_DOMAIN,
            Some(text) if text.len() == 32 => text.as_bytes().try_into().expect("32 bytes"),
            Some(_) => return usage("--cluster-domain must be exactly 32 bytes of text"),
        };
        let pool_key = pda(&[state::POOL_SEED]).0;
        let admission_key = pda(&[state::ADMISSION_SEED]).0;
        let config_key = pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).0;
        let pool_account = ctx.account(&pool_key)?;
        let admission_account = ctx.account(&admission_key)?;
        let config_account = ctx.account(&config_key)?;
        let pool = pool_account.as_ref().map(anchor::<state::Pool>);
        let admission = admission_account.as_ref().map(anchor::<state::Admission>);
        let config = config_account.as_ref().map(anchor::<state::ConfigVersion>);
        match (&pool_account, &pool) {
            (Some(account), Some(Ok(pool))) => {
                checks.push(check("pool.owner_is_program", account.owner == ID, ID, account.owner));
                checks.push(check("pool.governance_is_g_vault", pool.governance == g.vault, g.vault, pool.governance));
                checks.push(check("pool.guardian_vault_is_p_vault", pool.guardian_vault == p.vault, p.vault, pool.guardian_vault));
                checks.push(check(
                    "pool.cluster_domain",
                    pool.cluster_domain == domain,
                    String::from_utf8_lossy(&domain),
                    String::from_utf8_lossy(&pool.cluster_domain),
                ));
                if let Some(sponsor) = optional_key_flag(opts, "sponsor")? {
                    checks.push(check("pool.sponsor", pool.sponsor == sponsor.pubkey, sponsor.pubkey, pool.sponsor));
                }
                if let Some(management) = optional_key_flag(opts, "management-recipient")? {
                    checks.push(check(
                        "pool.management_recipient",
                        pool.management_recipient == management.pubkey,
                        management.pubkey,
                        pool.management_recipient,
                    ));
                }
                pool_json = json!({
                    "pool": pool_key.to_string(),
                    "round_id": pool.round_id,
                    "active_config_version": pool.active_config_version,
                    "pot": pool.pot, "seed": pool.seed, "management": pool.management,
                    "pending_toll": pool.pending_toll, "funded_claims": pool.funded_claims.to_string(),
                    "excess": pool.excess, "participant_count": pool.participant_count,
                    "next_close_id": pool.next_close_id,
                    "sponsor": pool.sponsor.to_string(),
                    "management_recipient": pool.management_recipient.to_string(),
                });
            }
            (Some(_), Some(Err(error))) => checks.push(check("pool.decodes", false, "a Pool", error)),
            _ => checks.push(check("pool.exists", false, pool_key, "absent")),
        }
        match (&admission, &pool) {
            (Some(Ok(admission)), pool_result) => {
                checks.push(check("admission.guardian_vault_is_p_vault", admission.guardian_vault == p.vault, p.vault, admission.guardian_vault));
                if let Some(Ok(pool)) = pool_result {
                    checks.push(check(
                        "admission.guardian_vault_equals_pool.guardian_vault",
                        admission.guardian_vault == pool.guardian_vault,
                        pool.guardian_vault,
                        admission.guardian_vault,
                    ));
                }
            }
            (Some(Err(error)), _) => checks.push(check("admission.decodes", false, "an Admission", error)),
            (None, _) => checks.push(check("admission.exists", false, admission_key, "absent")),
        }
        match config {
            Some(Ok(config)) => {
                let expected = launch_args(opts, Pubkey::default(), Pubkey::default())?;
                checks.push(check("config1.version", config.version == 1, 1, config.version));
                checks.push(check("config1.provider_program", config.provider_program == expected.provider_program, expected.provider_program, config.provider_program));
                checks.push(check("config1.provider_network_state", config.provider_network_state == expected.provider_network_state, expected.provider_network_state, config.provider_network_state));
                checks.push(check("config1.provider_treasury", config.provider_treasury == expected.provider_treasury, expected.provider_treasury, config.provider_treasury));
                checks.push(check("config1.provider_client", config.provider_client == expected.provider_client, expected.provider_client, config.provider_client));
                checks.push(check("config1.provider_request_fee_lamports", config.provider_request_fee_lamports == expected.provider_request_fee_lamports, expected.provider_request_fee_lamports, config.provider_request_fee_lamports));
                checks.push(check("config1.provider_fee_headroom_bps", config.provider_fee_headroom_bps == expected.provider_fee_headroom_bps, expected.provider_fee_headroom_bps, config.provider_fee_headroom_bps));
                checks.push(check("config1.public_series_a_enabled", config.public_series_a_enabled == expected.public_series_a_enabled, expected.public_series_a_enabled, config.public_series_a_enabled));
            }
            Some(Err(error)) => checks.push(check("config1.decodes", false, "a ConfigVersion", error)),
            None => checks.push(check("config1.exists", false, config_key, "absent")),
        }
    }
    let mut orao_json = Value::Null;
    if sections.contains("orao") {
        let client_key = provider::client_address().0;
        let rng_authority = provider::rng_authority_address().0;
        match ctx.account(&client_key)? {
            None => checks.push(check("orao_client.exists", false, client_key, "absent")),
            Some(account) => {
                checks.push(check("orao_client.account_owner_is_orao_program", account.owner == orao::id(), orao::id(), account.owner));
                match anchor::<orao::state::client::Client>(&account) {
                    Ok(client) => {
                        checks.push(check("orao_client.owner_is_g_vault", client.owner == g.vault, g.vault, client.owner));
                        checks.push(check("orao_client.state_is_rng_authority", client.state == rng_authority, rng_authority, client.state));
                        checks.push(check("orao_client.program_is_ours", client.program == ID, ID, client.program));
                        orao_json = json!({
                            "client": client_key.to_string(),
                            "owner": client.owner.to_string(),
                            "state": client.state.to_string(),
                            "program": client.program.to_string(),
                            "num_requests": client.num_requests,
                        });
                    }
                    Err(error) => checks.push(check("orao_client.decodes", false, "an ORAO Client", error)),
                }
            }
        }
        let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
        let network = ctx.account(&network_state)?;
        checks.push(check(
            "orao_network_state.owner_is_orao_program",
            network.as_ref().is_some_and(|account| account.owner == orao::id()),
            orao::id(),
            network.map_or("absent".to_string(), |account| account.owner.to_string()),
        ));
    }
    let all_ok = checks_ok(&checks);
    Ok(json!({
        "command": "readback",
        "receipt": "packet A6 / A8 read-back",
        "mode": "read-only",
        "g": { "create_key": g.create_key.to_string(), "multisig": g.multisig.to_string(), "vault_0": g.vault.to_string() },
        "p": { "create_key": p.create_key.to_string(), "multisig": p.multisig.to_string(), "vault_0": p.vault.to_string() },
        "program": program_json,
        "pool": pool_json,
        "orao": orao_json,
        "checks": checks,
        "all_ok": all_ok,
    }))
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

fn plan_step(ctx: &Ctx<'_>, step: &Step, all_fit: &mut bool) -> Result<Value, OpsError> {
    let built = build_step(step);
    let accounts: Vec<Value> = built
        .message
        .account_keys
        .iter()
        .enumerate()
        .map(|(position, key)| {
            json!({
                "key": key.to_string(),
                "signer": built.message.is_signer(position),
                "writable": built.message.is_maybe_writable(position, None),
            })
        })
        .collect();
    let mut value = match ctx.run_step(step, &[]) {
        Ok(result) => result.to_json(),
        Err(OpsError::Refused(message)) => {
            *all_fit = false;
            json!({ "step": step.id, "label": step.label, "refused": message, "packet_bytes": built.packet_bytes, "fits_1232": false })
        }
        Err(other) => return Err(other),
    };
    value["accounts"] = json!(accounts);
    Ok(value)
}

fn cmd_plan(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let (elf, elf_sha) = read_pinned_elf(opts, ctx.gate)?;
    let deployer = key_flag(opts, "deployer")?;
    let program = key_flag(opts, "program-keypair")?;
    let buffer = key_flag(opts, "buffer-keypair")?;
    let payer = key_flag(opts, "payer")?;
    let g = identity(key_flag(opts, "g-create-key")?.pubkey);
    let p = identity(key_flag(opts, "p-create-key")?.pubkey);
    let g_members = read_members(opts.require("g-members")?)?;
    let p_members = read_members(opts.require("p-members")?)?;
    let sponsor = key_flag(opts, "sponsor")?.pubkey;
    let management = key_flag(opts, "management-recipient")?.pubkey;
    let elf_len = elf.len();
    let max_data_len = opts
        .u64_flag("max-data-len")?
        .map_or(elf_len, |value| value as usize);
    if max_data_len < elf_len {
        return refuse("--max-data-len is below the ELF length");
    }
    let mut warnings: Vec<String> = Vec::new();
    if program.pubkey != ID {
        warnings.push(format!(
            "the program keypair {} is not the compiled-in program id {ID}: `deploy` will refuse outside a local cluster",
            program.pubkey
        ));
    }
    let treasury = match ctx.account(&sq::program_config_address())? {
        Some(config) => decode_program_config(&config.data)
            .map_err(|error| OpsError::Failed(format!("ProgramConfig: {error}")))?
            .1,
        None => {
            warnings.push("the Squads ProgramConfig is absent on this cluster; the treasury below is a placeholder".into());
            Pubkey::default()
        }
    };

    let buffer_rent = ctx.rent(BUFFER_HEADER_LEN + elf_len)?;
    let program_rent = ctx.rent(UpgradeableLoaderState::size_of_program())?;
    let programdata_rent = ctx.rent(PROGRAMDATA_HEADER_LEN + max_data_len)?;
    let multisig_rent = ctx.rent(MULTISIG_ACCOUNT_BYTES)?;
    let proposal_rent = ctx.rent(PROPOSAL_ACCOUNT_BYTES)?;
    let args = launch_args(opts, sponsor, management)?;
    let initialize = initialize_inner(&g, &p, args);
    let register = register_inner(&g);
    let initialize_vtx_rent = ctx.rent(vault_transaction_bytes(&initialize))?;
    let register_vtx_rent = ctx.rent(vault_transaction_bytes(&register))?;

    let mut all_fit = true;
    let mut phases = Vec::new();
    let mut transactions = 0usize;

    // A2
    let mut steps = Vec::new();
    for (name, ident, members, lock) in [("G", g, &g_members, sq::G_TIME_LOCK), ("P", p, &p_members, sq::P_TIME_LOCK)] {
        let step = step_squads_create(
            &format!("A2.multisig_create_v2.{name}"),
            &format!("squads multisig_create_v2 {name} (2-of-3, time lock {lock} s)"),
            treasury, payer.pubkey, &ident, members, lock,
        );
        steps.push(plan_step(ctx, &step, &mut all_fit)?);
        transactions += 1;
    }
    phases.push(json!({ "phase": "A2", "title": "create G and P with real multisig_create_v2", "signers": "payer + each create key", "steps": steps }));

    // A3
    let chunk = max_write_chunk(&deployer.pubkey, &buffer.pubkey);
    let write_count = elf_len.div_ceil(chunk.max(1));
    let mut steps = Vec::new();
    steps.push(plan_step(ctx, &step_create_buffer(deployer.pubkey, buffer.pubkey, buffer_rent, elf_len)?, &mut all_fit)?);
    let mut sample_write = plan_step(ctx, &step_write(deployer.pubkey, buffer.pubkey, 0, vec![0u8; chunk]), &mut all_fit)?;
    sample_write["repeat"] = json!(write_count);
    sample_write["chunk_bytes"] = json!(chunk);
    steps.push(sample_write);
    steps.push(plan_step(ctx, &step_deploy(deployer.pubkey, program.pubkey, buffer.pubkey, program_rent, max_data_len)?, &mut all_fit)?);
    steps.push(json!({ "step": "A3.verify_bytes", "label": "read-only A8 first pass: ProgramData header, ELF equality, zero padding (BEFORE the authority moves)" }));
    steps.push(plan_step(ctx, &step_set_authority(program.pubkey, deployer.pubkey, g.vault), &mut all_fit)?);
    transactions += 3 + write_count;
    phases.push(json!({ "phase": "A3", "title": "deploy the pinned ELF, verify bytes, move the upgrade authority to the G vault", "signers": "deployer (+ buffer key, program key)", "steps": steps }));

    // A5 (vault funding) happens before the register execution
    let fund = opts.u64_flag("lamports")?.unwrap_or(20_000_000);
    let steps = vec![plan_step(ctx, &step_transfer(payer.pubkey, g.vault, fund), &mut all_fit)?];
    transactions += 1;
    phases.push(json!({ "phase": "A4-prep", "title": "fund the G vault so it can pay the ORAO Client rent", "steps": steps }));

    // A4
    let current = ctx
        .account(&g.multisig)?
        .and_then(|account| decode_multisig(&account.data).ok())
        .map_or(0, |view| view.transaction_index);
    let proposer = g_members[0];
    let approver_two = g_members[1];
    let mut steps = Vec::new();
    for (offset, (name, inner)) in [("initialize", &initialize), ("register", &register)].into_iter().enumerate() {
        let index = current + 1 + offset as u64;
        let compiled = sq::compile_vault_message(inner);
        let (create, proposal) = steps_propose(&g, proposer, index, &compiled.bytes);
        let mut s = plan_step(ctx, &create, &mut all_fit)?;
        s["action"] = json!(name);
        s["message_sha256"] = json!(devnet::sha256_hex(&compiled.bytes));
        steps.push(s);
        steps.push(plan_step(ctx, &proposal, &mut all_fit)?);
        transactions += 2;
    }
    phases.push(json!({ "phase": "A4.propose", "title": "propose initialize and register (proposer = creator = rent payer = fee payer = G member 1)", "steps": steps }));
    let mut steps = Vec::new();
    for (offset, name) in ["initialize", "register"].into_iter().enumerate() {
        let index = current + 1 + offset as u64;
        for approver in [proposer, approver_two] {
            let mut s = plan_step(ctx, &step_approve(&g, approver, index), &mut all_fit)?;
            s["action"] = json!(name);
            steps.push(s);
            transactions += 1;
        }
    }
    phases.push(json!({ "phase": "A4.approve", "title": "two approvals each (threshold 2); the clock for the time lock starts at the second approval", "steps": steps }));
    let mut steps = vec![json!({
        "step": "A4.wait",
        "label": format!("wait {} s on the CLUSTER clock after the second approval of each proposal (one shared wait); `execute` refuses earlier", sq::G_TIME_LOCK),
    })];
    for (offset, (name, inner)) in [("initialize", &initialize), ("register", &register)].into_iter().enumerate() {
        let index = current + 1 + offset as u64;
        let mut s = plan_step(ctx, &step_execute(&g, proposer, index, inner), &mut all_fit)?;
        s["action"] = json!(name);
        if name == "initialize" {
            s["outer_signers_in_addition_to_fee_payer"] = json!([sponsor.to_string()]);
        }
        steps.push(s);
        transactions += 1;
    }
    phases.push(json!({ "phase": "A4.execute", "title": "execute initialize (sponsor co-signs the outer transaction), then register", "steps": steps }));

    let fee_estimate = transactions as u64 * SIGNATURE_FEE_LAMPORTS * 2;
    Ok(json!({
        "command": "plan",
        "mode": "read-only (nothing is signed or sent)",
        "cluster": ctx.cluster_json(&ctx.cluster.genesis_hash().unwrap_or_default()),
        "program_id": program.pubkey.to_string(),
        "programdata": programdata_address(&program.pubkey).to_string(),
        "elf": { "sha256": elf_sha, "elf_len": elf_len, "max_data_len": max_data_len, "write_chunk_bytes": chunk, "write_transactions": write_count },
        "g": { "create_key": g.create_key.to_string(), "multisig": g.multisig.to_string(), "vault_0": g.vault.to_string(), "members": key_list(&g_members), "threshold": sq::THRESHOLD, "time_lock_seconds": sq::G_TIME_LOCK },
        "p": { "create_key": p.create_key.to_string(), "multisig": p.multisig.to_string(), "vault_0": p.vault.to_string(), "members": key_list(&p_members), "threshold": sq::THRESHOLD, "time_lock_seconds": sq::P_TIME_LOCK },
        "fee_payers": {
            "squads_create_and_fund_vault": payer.pubkey.to_string(),
            "deploy": deployer.pubkey.to_string(),
            "propose_and_execute": proposer.to_string(),
            "second_approval": approver_two.to_string(),
        },
        "initialize_parameters": args_json(&args),
        "rent_lamports_estimates": {
            "basis": "getMinimumBalanceForRentExemption on this cluster; Squads vault-transaction and proposal sizes are formula estimates",
            "buffer": buffer_rent, "program": program_rent, "programdata": programdata_rent,
            "deploy_net_retained": program_rent + programdata_rent,
            "deploy_peak_while_buffer_and_programdata_coexist": buffer_rent + program_rent + programdata_rent,
            "multisig_each": multisig_rent, "proposal_each": proposal_rent,
            "vault_transaction_initialize": initialize_vtx_rent, "vault_transaction_register": register_vtx_rent,
            "vault_funding": fund,
            "fees_upper_estimate": fee_estimate,
        },
        "transactions_total": transactions,
        "phases": phases,
        "warnings": warnings,
        "all_ok": all_fit,
    }))
}

// ---------------------------------------------------------------------------
// validator-fixtures (offline helper for the local-validator test)
// ---------------------------------------------------------------------------

fn account_file_json(address: &Pubkey, owner: &Pubkey, lamports: u64, data: &[u8]) -> Value {
    use base64::Engine;
    json!({
        "pubkey": address.to_string(),
        "account": {
            "lamports": lamports,
            "data": [base64::engine::general_purpose::STANDARD.encode(data), "base64"],
            "owner": owner.to_string(),
            "executable": false,
            "rentEpoch": 0,
            "space": data.len(),
        }
    })
}

/// Write the validator `--account` files for the Squads `ProgramConfig` and the ORAO NetworkState
/// and treasury (devnet snapshots from `solana/fixtures/devnet/`) and return the exact
/// `solana-test-validator` arguments.  Offline: no RPC, no key.
fn cmd_validator_fixtures(opts: &Opts) -> Result<Value, OpsError> {
    let out = opts
        .positional
        .first()
        .map(String::as_str)
        .or(opts.get("out-dir"))
        .ok_or_else(|| OpsError::Usage("`validator-fixtures` needs an output directory".into()))?;
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).map_err(|error| OpsError::Failed(format!("{}: {error}", out.display())))?;
    let loaded = std::panic::catch_unwind(|| {
        (
            devnet::load_accounts(devnet::SQUADS_DEVNET_ACCOUNTS_FILE),
            devnet::load_accounts(devnet::ORAO_DEVNET_ACCOUNTS_FILE),
        )
    })
    .map_err(|_| OpsError::Failed("the devnet account fixtures failed their integrity checks".into()))?;
    let (squads, orao_accounts) = loaded;
    let mut written = Vec::new();
    for (file, account) in [
        ("squads_program_config.json", &squads["program_config"]),
        ("orao_network_state.json", &orao_accounts["network_state"]),
        ("orao_treasury.json", &orao_accounts["treasury"]),
    ] {
        let path = out.join(file);
        let body = account_file_json(&account.address, &account.owner, account.lamports, &account.data);
        fs::write(&path, serde_json::to_string_pretty(&body).expect("json"))
            .map_err(|error| OpsError::Failed(format!("{}: {error}", path.display())))?;
        written.push((account.address, path));
    }
    let fixtures_dir = devnet::fixture_dir()
        .canonicalize()
        .unwrap_or_else(|_| devnet::fixture_dir());
    let mut command = format!(
        "solana-test-validator --reset --quiet --rpc-port 8899 --faucet-port 9900 --ledger {ledger} --limit-ledger-size 10000 \
         --bpf-program {squads} {fix}/{squads_so} --bpf-program {orao} {fix}/{orao_so}",
        ledger = out.join("ledger").display(),
        squads = SQUADS_PROGRAM_ID,
        orao = orao::id(),
        fix = fixtures_dir.display(),
        squads_so = devnet::SQUADS_DEVNET_ELF_FILE,
        orao_so = devnet::ORAO_DEVNET_ELF_FILE,
    );
    for (address, path) in &written {
        command.push_str(&format!(" --account {address} {}", path.display()));
    }
    let mut preloaded = Value::Null;
    if let (Some(so), Some(g_key)) = (opts.get("program-so"), opts.get("g-create-key")) {
        let g = identity(KeyFile::load(g_key)?.pubkey);
        let digest = devnet::sha256_hex(&fs::read(so).map_err(|error| OpsError::Failed(format!("{so}: {error}")))?);
        if digest != PINNED_PROGRAM_ELF_SHA256 {
            return refuse(format!("{so} hashes to {digest}, not the pinned {PINNED_PROGRAM_ELF_SHA256}"));
        }
        command.push_str(&format!(" --upgradeable-program {ID} {so} {}", g.vault));
        preloaded = json!({ "program": ID.to_string(), "upgrade_authority_g_vault": g.vault.to_string() });
    }
    Ok(json!({
        "command": "validator-fixtures",
        "mode": "offline",
        "files": written.iter().map(|(address, path)| json!({ "account": address.to_string(), "file": path.display().to_string() })).collect::<Vec<_>>(),
        "preloaded_program": preloaded,
        "validator_command": command,
        "all_ok": true,
    }))
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Run a parsed command against `cluster`: gate checks first (URL / cluster name / execute
/// token / genesis hash), then the command.
pub fn run_opts(opts: &Opts, cluster: &dyn Cluster) -> Result<Value, OpsError> {
    if opts.command == "validator-fixtures" {
        return cmd_validator_fixtures(opts);
    }
    if opts.command == "help" {
        return Ok(json!({ "help": help_text() }));
    }
    let gate = Gate::from_opts(opts)?;
    let token = gate.execute_token(opts)?;
    let genesis = cluster
        .genesis_hash()
        .map_err(|error| rpc_err("getGenesisHash", error))?;
    gate.check_genesis(&genesis)?;
    let ctx = Ctx {
        opts,
        gate: &gate,
        cluster,
        token,
    };
    let mut value = match opts.command.as_str() {
        "plan" => cmd_plan(&ctx),
        "deploy" => cmd_deploy(&ctx),
        "verify-bytes" => cmd_verify_bytes(&ctx),
        "squads-create" => cmd_squads_create(&ctx),
        "fund-vault" => cmd_fund_vault(&ctx),
        "propose" => cmd_propose(&ctx),
        "approve" => cmd_approve(&ctx),
        "execute" => cmd_execute(&ctx),
        "readback" => cmd_readback(&ctx),
        other => usage(format!("unknown command `{other}`")),
    }?;
    if value.get("cluster").is_none() {
        value["cluster"] = ctx.cluster_json(&genesis);
    }
    Ok(value)
}

/// Parse `args` (without the program name) and run against `cluster`.
pub fn run_with_cluster(args: &[String], cluster: &dyn Cluster) -> Result<Value, OpsError> {
    run_opts(&Opts::parse(args)?, cluster)
}

/// The binary's body: connects an [`RpcCluster`] to `--rpc`, prints the JSON receipt on stdout and
/// returns the process exit code (see [`exit_code`]; 4 when a verification check did not hold).
pub fn run_cli(args: &[String]) -> i32 {
    let outcome = Opts::parse(args).and_then(|opts| {
        if opts.command == "validator-fixtures" || opts.command == "help" {
            return run_opts(&opts, &OfflineCluster);
        }
        let gate = Gate::from_opts(&opts)?;
        let cluster = RpcCluster::connect(&gate.rpc_url, gate.commitment);
        run_opts(&opts, &cluster)
    });
    match outcome {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).expect("receipt JSON"));
            if value["all_ok"] == json!(false) {
                4
            } else {
                0
            }
        }
        Err(error) => {
            eprintln!("{error}");
            exit_code(&error)
        }
    }
}

/// A cluster for the offline subcommands: every call is an error.
struct OfflineCluster;

impl Cluster for OfflineCluster {
    fn genesis_hash(&self) -> Result<String, String> {
        Err("offline".into())
    }
    fn blockhash(&self) -> Result<Hash, String> {
        Err("offline".into())
    }
    fn account(&self, _key: &Pubkey) -> Result<Option<Account>, String> {
        Err("offline".into())
    }
    fn rent_minimum(&self, _data_len: usize) -> Result<u64, String> {
        Err("offline".into())
    }
    fn unix_time(&self) -> Result<i64, String> {
        Err("offline".into())
    }
    fn slot(&self) -> Result<u64, String> {
        Err("offline".into())
    }
    fn simulate(&self, _tx: &Transaction, _sig_verify: bool) -> Result<SimOutcome, String> {
        Err("offline".into())
    }
    fn send(&self, _tx: &Transaction) -> Result<SendOutcome, String> {
        Err("offline".into())
    }
}
