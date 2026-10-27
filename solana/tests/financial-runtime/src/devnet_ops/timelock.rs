//! `devnet_ops g-timelock` (M14-F11-TOOL, packet F11): G reduces its own Squads time lock, the
//! consequence is documented, and G restores it.  Contract: `docs/coordination/m14-f11-timelock-protocol-20261027.md`.
//!
//! ```text
//! devnet_ops g-timelock status                          # read-only: G profile, phase, earliest execution
//! devnet_ops g-timelock probe                           # read-only, SIMULATE ONLY: the RG-8 consequence
//! devnet_ops g-timelock propose|approve|execute reduce  # 172,800 s -> 0 s
//! devnet_ops g-timelock propose|approve|execute restore # 0 s -> 172,800 s
//! ```
//!
//! # What the tool will and will not do
//!
//! * The two time-lock values are CODE CONSTANTS ([`T_NORMAL`] = 172,800, [`T_REDUCED`] = 0).  `reduce` is
//!   172,800 -> 0 and `restore` is 0 -> 172,800.  No flag, file or environment variable can name another
//!   value, another action, a threshold, a member, a spending limit, a rent collector or a memo; the P
//!   multisig is refused.  Anything else on the command line is a USAGE error ([`check_flags`]).
//! * Every write is a dry run unless `--execute` is given, under the SAME interlocks as every other
//!   `devnet_ops` command: the gate (loopback, or devnet with the packet pin, the host allowlist and a
//!   required `--pending-file`), the genesis check, `resolve_pending_all` (held-unknown records refuse every
//!   `--execute`; there is no command that clears one), and `Ctx::run_step` for every transaction (packet size,
//!   simulate before send, write-ahead BEFORE send, F3 handling of an ambiguous send, `minContextSlot`).
//!   This module has no retry logic, no release command and no way to clear a record.
//! * `status` and `probe` are read-only and refuse `--execute`.  `probe` never reaches the send path: it
//!   builds an UNSIGNED transaction in which the G vault is only a signer FLAG and simulates it with signature
//!   verification off, so nothing could be sent for the vault even by accident (no key can sign for a PDA).
//!
//! # Preconditions (the protocol's R0 / S0)
//!
//! `propose` refuses unless the G profile is exact at the action's FROM lock (`reduce`: 172,800; `restore`: 0),
//! the write-ahead file holds no open or held record, and NO G transaction in `(stale_transaction_index,
//! transaction_index]` is live (a proposal that is not Executed, Rejected or Cancelled, or a transaction with no
//! proposal unless it is exactly our resumable config transaction).  `reduce` additionally needs the program's
//! upgrade authority to be the G vault, an initialized Pool with ConfigVersion#1 active, the ORAO Client registered
//! (owner G vault, state RngAuthority) and a baseline probe that succeeds.  `restore` depends on none of those.
//! `approve` and `execute` vote on, and run, only a config transaction whose stored actions are EXACTLY
//! `[SetTimeLock { target }]`; `execute` also refuses, in plain words and before any send, until the cluster
//! clock reaches `approved_at + the CURRENT time lock` (Squads error 6021 otherwise, after a paid fee).
//! `execute` ALSO re-proves "no other live G transaction" at the moment the config would run
//! ([`require_no_other_live`], F11-R1): the executed config must be the LATEST G transaction and every other
//! transaction above the stale index must have a terminal proposal (an orphan, a missing or undecodable account
//! and an over-large range all refuse), before any signing, simulation or send.  A stale, already-Approved VAULT
//! transaction can still run after a config executes, so `propose`'s earlier refusal is not enough.
//!
//! # Solana notes (closest ICP / EVM comparisons)
//!
//! * A Squads CONFIG transaction changes the multisig's own settings (members, threshold, time lock).  It is
//!   executed by Squads against its own `Multisig` account, not through a CPI signed by the vault PDA, so it
//!   needs none of the remaining accounts a vault transaction carries.  EVM analogue: a Safe transaction that
//!   calls the Safe on itself (`changeThreshold`, `addOwnerWithThreshold`); ICP analogue: an SNS
//!   parameter-change proposal.  Its concrete effect here is `Multisig.time_lock`, which the program reads on
//!   EVERY G governance instruction (`require_governance` demands exactly 172,800), so while G is reduced the
//!   program refuses all of them (`WrongGovernanceProfile`).
//! * Executing a config transaction sets `stale_transaction_index` to its own index, which makes every earlier
//!   unexecuted G transaction stale: Squads refuses new votes on a stale proposal and refuses to execute a stale
//!   CONFIG transaction (a stale, already-approved VAULT transaction can still run; the tests record the real
//!   binary's behaviour).  That is why `propose` refuses while any G transaction is live.
//! * Anchor 0.29 optional accounts (`rent_payer`, `system_program` of `config_transaction_execute`) that are not
//!   used are passed as the PROGRAM ID in their slot (read-only, non-signer), the Anchor spelling of `None`.  A
//!   `SetTimeLock` never reallocates the multisig account, so both are `None`.
//! * The time lock runs on the cluster's Clock sysvar (validators vote on it; it can lag wall time), which is why
//!   the release time is judged against [`Cluster::unix_time`], never this machine's clock.

use solana_sdk::instruction::AccountMeta;

use super::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// G's normal time lock (`GOVERNANCE_DELAY` in the program; packet 3b): 48 hours.
pub const T_NORMAL: u32 = sq::G_TIME_LOCK;
/// G's reduced time lock (packet 6.4: "restoring is immediate at the reduced delay"; also P's profile value).
pub const T_REDUCED: u32 = 0;
/// The most transactions above the stale index the liveness scan reads.  A larger range is REFUSED (a partial
/// scan could not prove that no G transaction is live).
pub const LIVE_SCAN_CAP: u64 = 64;
/// How many of the newest transactions `status` lists.
const STATUS_SCAN_WINDOW: u64 = 16;
/// `ConfigTransaction` account bytes besides the actions: discriminator, multisig, creator, index, bump, vec length.
const CONFIG_TX_FIXED_BYTES: usize = 8 + 32 + 32 + 8 + 1 + 4;
/// The code of the program error the consequence probe must show while G is reduced.
pub const WRONG_GOVERNANCE_PROFILE_CODE: u32 =
    musical_chairs_program::ErrorCode::WrongGovernanceProfile as u32 + anchor_lang::error::ERROR_CODE_OFFSET;
/// `NotAuthorized`, named in a probe receipt when it is what the simulation returned.
const NOT_AUTHORIZED_CODE: u32 =
    musical_chairs_program::ErrorCode::NotAuthorized as u32 + anchor_lang::error::ERROR_CODE_OFFSET;

/// The only two actions this tool knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// 172,800 s -> 0 s.
    Reduce,
    /// 0 s -> 172,800 s.
    Restore,
}

impl Direction {
    fn parse(text: &str) -> Result<Direction, OpsError> {
        match text {
            "reduce" => Ok(Direction::Reduce),
            "restore" => Ok(Direction::Restore),
            other => usage(format!(
                "unknown action `{other}`: `g-timelock` knows only `reduce` (172,800 s -> 0 s) and `restore` (0 s -> 172,800 s); \
                 the time-lock values are fixed in code"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Direction::Reduce => "reduce",
            Direction::Restore => "restore",
        }
    }

    /// The time lock G must have BEFORE this action.
    pub fn from_lock(self) -> u32 {
        match self {
            Direction::Reduce => T_NORMAL,
            Direction::Restore => T_REDUCED,
        }
    }

    /// The time lock this action sets.
    pub fn target_lock(self) -> u32 {
        match self {
            Direction::Reduce => T_REDUCED,
            Direction::Restore => T_NORMAL,
        }
    }

    /// The config-transaction actions this direction stores: exactly one `SetTimeLock`.
    pub fn actions(self) -> Vec<ConfigAction> {
        vec![ConfigAction::SetTimeLock { new_time_lock: self.target_lock() }]
    }

    /// The write-ahead step id of `what` (`F11.reduce.proposal_approve`, ...).
    pub fn step_id(self, what: &str) -> String {
        format!("F11.{}.{what}", self.name())
    }
}

// ---------------------------------------------------------------------------
// Squads config actions and the config-transaction instructions (Borsh by hand from the committed IDL)
// ---------------------------------------------------------------------------

/// Squads `ConfigAction` in IDL variant order (AddMember 0 ... SetRentCollector 6).  Only `SetTimeLock` is ever
/// built by this tool; the others exist so a stored transaction can be DECODED FULLY and compared, never trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigAction {
    AddMember { key: Pubkey, permissions: u8 },
    RemoveMember { key: Pubkey },
    ChangeThreshold { new_threshold: u16 },
    SetTimeLock { new_time_lock: u32 },
    AddSpendingLimit {
        create_key: Pubkey,
        vault_index: u8,
        mint: Pubkey,
        amount: u64,
        period: u8,
        members: Vec<Pubkey>,
        destinations: Vec<Pubkey>,
    },
    RemoveSpendingLimit { spending_limit: Pubkey },
    SetRentCollector { new_rent_collector: Option<Pubkey> },
}

impl ConfigAction {
    /// Borsh bytes: a `u8` variant tag, then the fields in IDL order.
    pub fn encode(&self, out: &mut Vec<u8>) {
        fn keys(out: &mut Vec<u8>, list: &[Pubkey]) {
            out.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for key in list {
                out.extend_from_slice(key.as_ref());
            }
        }
        match self {
            ConfigAction::AddMember { key, permissions } => {
                out.push(0);
                out.extend_from_slice(key.as_ref());
                out.push(*permissions);
            }
            ConfigAction::RemoveMember { key } => {
                out.push(1);
                out.extend_from_slice(key.as_ref());
            }
            ConfigAction::ChangeThreshold { new_threshold } => {
                out.push(2);
                out.extend_from_slice(&new_threshold.to_le_bytes());
            }
            ConfigAction::SetTimeLock { new_time_lock } => {
                out.push(3);
                out.extend_from_slice(&new_time_lock.to_le_bytes());
            }
            ConfigAction::AddSpendingLimit { create_key, vault_index, mint, amount, period, members, destinations } => {
                out.push(4);
                out.extend_from_slice(create_key.as_ref());
                out.push(*vault_index);
                out.extend_from_slice(mint.as_ref());
                out.extend_from_slice(&amount.to_le_bytes());
                out.push(*period);
                keys(out, members);
                keys(out, destinations);
            }
            ConfigAction::RemoveSpendingLimit { spending_limit } => {
                out.push(5);
                out.extend_from_slice(spending_limit.as_ref());
            }
            ConfigAction::SetRentCollector { new_rent_collector } => {
                out.push(6);
                match new_rent_collector {
                    None => out.push(0),
                    Some(key) => {
                        out.push(1);
                        out.extend_from_slice(key.as_ref());
                    }
                }
            }
        }
    }

    /// The Borsh bytes of this one action.
    pub fn encoded(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }

    fn decode(reader: &mut Reader<'_>) -> Result<ConfigAction, String> {
        Ok(match reader.u8()? {
            0 => ConfigAction::AddMember { key: reader.key()?, permissions: reader.u8()? },
            1 => ConfigAction::RemoveMember { key: reader.key()? },
            2 => ConfigAction::ChangeThreshold { new_threshold: reader.u16()? },
            3 => ConfigAction::SetTimeLock { new_time_lock: reader.u32()? },
            4 => {
                let create_key = reader.key()?;
                let vault_index = reader.u8()?;
                let mint = reader.key()?;
                let amount = reader.u64()?;
                let period = reader.u8()?;
                if period > 3 {
                    return Err(format!("spending-limit Period variant {period} (OneTime 0, Day 1, Week 2, Month 3)"));
                }
                ConfigAction::AddSpendingLimit {
                    create_key,
                    vault_index,
                    mint,
                    amount,
                    period,
                    members: reader.keys()?,
                    destinations: reader.keys()?,
                }
            }
            5 => ConfigAction::RemoveSpendingLimit { spending_limit: reader.key()? },
            6 => ConfigAction::SetRentCollector {
                new_rent_collector: match reader.u8()? {
                    0 => None,
                    1 => Some(reader.key()?),
                    other => return Err(format!("rent collector option tag {other}")),
                },
            },
            other => return Err(format!("unknown ConfigAction variant {other}")),
        })
    }

    pub fn to_json(&self) -> Value {
        match self {
            ConfigAction::AddMember { key, permissions } => {
                json!({ "variant": "AddMember", "key": key.to_string(), "permissions": permissions })
            }
            ConfigAction::RemoveMember { key } => json!({ "variant": "RemoveMember", "key": key.to_string() }),
            ConfigAction::ChangeThreshold { new_threshold } => {
                json!({ "variant": "ChangeThreshold", "new_threshold": new_threshold })
            }
            ConfigAction::SetTimeLock { new_time_lock } => {
                json!({ "variant": "SetTimeLock", "new_time_lock": new_time_lock })
            }
            ConfigAction::AddSpendingLimit { create_key, vault_index, mint, amount, period, members, destinations } => json!({
                "variant": "AddSpendingLimit", "create_key": create_key.to_string(), "vault_index": vault_index,
                "mint": mint.to_string(), "amount": amount, "period": period,
                "members": key_list(members), "destinations": key_list(destinations),
            }),
            ConfigAction::RemoveSpendingLimit { spending_limit } => {
                json!({ "variant": "RemoveSpendingLimit", "spending_limit": spending_limit.to_string() })
            }
            ConfigAction::SetRentCollector { new_rent_collector } => json!({
                "variant": "SetRentCollector",
                "new_rent_collector": new_rent_collector.map(|key| key.to_string()),
            }),
        }
    }
}

/// `Vec<ConfigAction>` as Borsh: a `u32` length, then each action.
pub fn encode_actions(actions: &[ConfigAction]) -> Vec<u8> {
    let mut out = (actions.len() as u32).to_le_bytes().to_vec();
    for action in actions {
        action.encode(&mut out);
    }
    out
}

pub fn actions_json(actions: &[ConfigAction]) -> Value {
    json!(actions.iter().map(ConfigAction::to_json).collect::<Vec<_>>())
}

/// `true` when `actions` is exactly one `SetTimeLock { target }`.
pub fn is_exactly_set_time_lock(actions: &[ConfigAction], target: u32) -> bool {
    matches!(actions, [ConfigAction::SetTimeLock { new_time_lock }] if *new_time_lock == target)
}

/// `config_transaction_create` (`ConfigTransactionCreateArgs { actions, memo: None }`).  Accounts exactly as the
/// IDL lists them: multisig (mut), transaction (mut; the same PDA as a vault transaction at that index), creator
/// (signer), rent payer (mut, signer), system program.
pub fn config_transaction_create_ix(
    multisig: Pubkey,
    transaction_index: u64,
    creator: Pubkey,
    rent_payer: Pubkey,
    actions: &[ConfigAction],
) -> Instruction {
    let mut data = sq::instruction_discriminator("config_transaction_create").to_vec();
    data.extend_from_slice(&encode_actions(actions));
    data.push(0); // memo: None
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(multisig, false),
            AccountMeta::new(sq::transaction_address(&multisig, transaction_index), false),
            AccountMeta::new_readonly(creator, true),
            AccountMeta::new(rent_payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

/// `config_transaction_execute` (no arguments).  Accounts: multisig (mut), member (signer), proposal (mut),
/// transaction, then the two OPTIONAL accounts `rent_payer` and `system_program`, both `None`: Anchor spells an
/// unused optional account as the PROGRAM ID in its slot, read-only and not a signer (the IDL marks the slot
/// writable / signer; that is the flag set for a `Some`).  A `SetTimeLock` needs no reallocation.
pub fn config_transaction_execute_ix(multisig: Pubkey, transaction_index: u64, member: Pubkey) -> Instruction {
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(multisig, false),
            AccountMeta::new_readonly(member, true),
            AccountMeta::new(sq::proposal_address(&multisig, transaction_index), false),
            AccountMeta::new_readonly(sq::transaction_address(&multisig, transaction_index), false),
            AccountMeta::new_readonly(SQUADS_PROGRAM_ID, false), // rent_payer: None
            AccountMeta::new_readonly(SQUADS_PROGRAM_ID, false), // system_program: None
        ],
        data: sq::instruction_discriminator("config_transaction_execute").to_vec(),
    }
}

/// The size of a `ConfigTransaction` account holding `actions` (what Squads allocates, so what rent is paid on).
pub fn config_transaction_bytes(actions: &[ConfigAction]) -> usize {
    CONFIG_TX_FIXED_BYTES + actions.iter().map(|action| action.encoded().len()).sum::<usize>()
}

/// A decoded `ConfigTransaction` account (IDL layout: discriminator, multisig, creator, index u64, bump, actions).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigTransactionView {
    pub multisig: Pubkey,
    pub creator: Pubkey,
    pub index: u64,
    pub bump: u8,
    pub actions: Vec<ConfigAction>,
    /// Bytes after the last action (allocation padding; all zero, else the decode is refused).
    pub padding_bytes: usize,
}

/// Strict decode: the exact discriminator, every action fully decoded, and nothing but zero padding after them.
pub fn decode_config_transaction(data: &[u8]) -> Result<ConfigTransactionView, String> {
    if data.len() < 8 || data[..8] != account_discriminator("ConfigTransaction") {
        return Err("not a Squads ConfigTransaction account (discriminator)".into());
    }
    let mut reader = Reader::new(data);
    reader.take(8)?;
    let multisig = reader.key()?;
    let creator = reader.key()?;
    let index = reader.u64()?;
    let bump = reader.u8()?;
    let count = reader.u32()? as usize;
    if count > reader.remaining() {
        return Err("action list exceeds the account data".into());
    }
    let mut actions = Vec::with_capacity(count);
    for _ in 0..count {
        actions.push(ConfigAction::decode(&mut reader)?);
    }
    let padding_bytes = reader.remaining();
    if reader.take(padding_bytes)?.iter().any(|byte| *byte != 0) {
        return Err("non-zero bytes after the last action".into());
    }
    Ok(ConfigTransactionView { multisig, creator, index, bump, actions, padding_bytes })
}

/// The consequence probe's instruction: `rotate_recipient(kind = 0, new_recipient = the CURRENT management
/// recipient)`, a no-op when it runs.  G's multisig is `governance_multisig` and the G vault (index 0) is the
/// `governance` signer.  It is only ever SIMULATED.
pub fn noop_rotate_recipient_ix(g: &Identity, current_management_recipient: Pubkey) -> Instruction {
    anchor_ix(
        accounts::RotateRecipient {
            governance_multisig: g.multisig,
            governance: g.vault,
            pool: pda(&[state::POOL_SEED]).0,
            ops: pda(&[state::OPS_SEED]).0,
        },
        instruction::RotateRecipient { kind: 0, new_recipient: current_management_recipient },
    )
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

/// Value flags `g-timelock` accepts; everything else is a usage error.
const ALLOWED_VALUE_FLAGS: &[&str] = &[
    "rpc",
    "cluster",
    "packet-sha256",
    "packet-commit",
    "allow-rpc-host",
    "pending-file",
    "pending-wait-seconds",
    "slot-polls",
    "commitment",
    "g-create-key",
    "g-members",
    "member",
    "tx-index",
    "expect-tx-index",
];
/// Switches `g-timelock` accepts.
const ALLOWED_SWITCHES: &[&str] = &["execute", "simulate"];

/// `true` for the two read-only subcommands, which refuse `--execute` (`Gate::execute_token_in` asks).
pub(super) fn is_read_only(opts: &Opts) -> bool {
    opts.command == "g-timelock"
        && matches!(opts.positional.first().map(String::as_str), Some("status" | "probe"))
}

/// Refuse every flag outside the allowlist: there is no way to name a value, a threshold, a member change, a
/// memo, a spending limit, a rent collector or the P multisig.
fn check_flags(opts: &Opts) -> Result<(), OpsError> {
    let names = opts
        .values
        .keys()
        .map(|name| (name, ALLOWED_VALUE_FLAGS))
        .chain(opts.switches.iter().map(|name| (name, ALLOWED_SWITCHES)));
    for (name, allowed) in names {
        if allowed.contains(&name.as_str()) {
            continue;
        }
        return usage(match name.as_str() {
            "p-create-key" | "p-members" => format!(
                "--{name} is refused: `g-timelock` acts on the G multisig only (never the P multisig)"
            ),
            _ => format!(
                "--{name} is not a `g-timelock` flag: the time-lock values 0 and {T_NORMAL} are fixed in code, and there is \
                 no flag for another value, a threshold, a member, a spending limit, a rent collector or a memo"
            ),
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sub {
    Status,
    Probe,
    Propose(Direction),
    Approve(Direction),
    Execute(Direction),
}

fn parse_sub(opts: &Opts) -> Result<Sub, OpsError> {
    let Some(name) = opts.positional.first() else {
        return usage("`g-timelock` needs a subcommand: status | probe | propose|approve|execute reduce|restore");
    };
    let read_only = |sub: Sub| -> Result<Sub, OpsError> {
        if opts.positional.len() > 1 {
            return usage(format!("`g-timelock {name}` takes no action"));
        }
        Ok(sub)
    };
    let with_direction = |build: fn(Direction) -> Sub| -> Result<Sub, OpsError> {
        let action = opts.positional.get(1).ok_or_else(|| {
            OpsError::Usage(format!("`g-timelock {name}` needs an action: reduce | restore"))
        })?;
        if opts.positional.len() > 2 {
            return usage("only one action may be given");
        }
        Ok(build(Direction::parse(action)?))
    };
    match name.as_str() {
        "status" => read_only(Sub::Status),
        "probe" => read_only(Sub::Probe),
        "propose" => with_direction(Sub::Propose),
        "approve" => with_direction(Sub::Approve),
        "execute" => with_direction(Sub::Execute),
        other => usage(format!(
            "unknown `g-timelock` subcommand `{other}` (status | probe | propose | approve | execute)"
        )),
    }
}

/// `--tx-index` belongs to approve / execute, `--expect-tx-index` to propose.
fn check_index_flags(opts: &Opts, sub: Sub) -> Result<(), OpsError> {
    let (tx_index, expect) = match sub {
        Sub::Status | Sub::Probe => (false, false),
        Sub::Propose(_) => (false, true),
        Sub::Approve(_) | Sub::Execute(_) => (true, false),
    };
    if !tx_index && opts.get("tx-index").is_some() {
        return usage("--tx-index belongs to `approve` and `execute` only");
    }
    if !expect && opts.get("expect-tx-index").is_some() {
        return usage("--expect-tx-index belongs to `propose` only");
    }
    Ok(())
}

pub(super) fn cmd_g_timelock(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    check_flags(ctx.opts)?;
    let sub = parse_sub(ctx.opts)?;
    check_index_flags(ctx.opts, sub)?;
    if matches!(sub, Sub::Status | Sub::Probe) && ctx.executing() {
        // Defence in depth: `Gate::execute_token_in` never mints a token for these two.
        return usage(format!("`g-timelock {}` is read-only and takes no --execute", ctx.opts.positional[0]));
    }
    let g = ctx.g_identity()?;
    let members = read_members(ctx.opts.require("g-members")?)?;
    match sub {
        Sub::Status => cmd_status(ctx, &g, &members),
        Sub::Probe => cmd_probe(ctx, &g, &members),
        Sub::Propose(direction) => cmd_propose_tl(ctx, &g, &members, direction),
        Sub::Approve(direction) => cmd_approve_tl(ctx, &g, &members, direction),
        Sub::Execute(direction) => cmd_execute_tl(ctx, &g, &members, direction),
    }
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

fn failing_checks(checks: &[Value]) -> Vec<&Value> {
    checks.iter().filter(|entry| entry["ok"] != json!(true)).collect()
}

fn refuse_if_failing(what: &str, checks: &[Value]) -> Result<(), OpsError> {
    let failing = failing_checks(checks);
    if failing.is_empty() {
        return Ok(());
    }
    refuse(format!(
        "precondition(s) for `g-timelock {what}` do not hold; nothing was sent: {}",
        serde_json::to_string(&failing).unwrap_or_default()
    ))
}

fn read_view(ctx: &Ctx<'_>, g: &Identity) -> Result<sq::MultisigView, OpsError> {
    let account = ctx
        .account(&g.multisig)?
        .ok_or_else(|| OpsError::Refused(format!("the G multisig {} is absent", g.multisig)))?;
    if account.owner != SQUADS_PROGRAM_ID {
        return refuse(format!("the G multisig {} is owned by {}, not Squads", g.multisig, account.owner));
    }
    decode_multisig(&account.data).map_err(|error| OpsError::Failed(format!("G multisig: {error}")))
}

fn cluster_clock(ctx: &Ctx<'_>) -> Result<i64, OpsError> {
    ctx.cluster
        .unix_time()
        .map_err(|error| rpc_err("reading the cluster clock", error))
}

/// The G multisig with its exact profile at `lock` (everything else as `profile_checks` demands).
fn profile_at(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
    lock: u32,
) -> Result<(Vec<Value>, Option<sq::MultisigView>), OpsError> {
    let account = ctx.account(&g.multisig)?;
    Ok(profile_checks(
        &ProfileSpec { name: "G", identity: *g, members, time_lock: lock },
        account.as_ref(),
    ))
}

/// The G profile for `direction`'s FROM lock, or a refusal in plain words: a time lock that is already at the
/// target ("reduce while at 0", "restore while at 172,800") and a time lock that is neither get their own
/// message; any other deviation lists the failing checks.
fn require_profile_for(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
    direction: Direction,
) -> Result<sq::MultisigView, OpsError> {
    let (checks, view) = profile_at(ctx, g, members, direction.from_lock())?;
    let failing = failing_checks(&checks);
    if failing.is_empty() {
        return view.ok_or_else(|| OpsError::Failed("internal: a profile that passed has no view".into()));
    }
    if let (Some(view), [only]) = (&view, failing.as_slice()) {
        if only["name"] == "G.time_lock_seconds" {
            return refuse(if view.time_lock == direction.target_lock() {
                format!(
                    "G's time lock is already {} s: `{}` has nothing to do (`{}` is for a G at {} s)",
                    view.time_lock,
                    direction.name(),
                    direction.name(),
                    direction.from_lock()
                )
            } else {
                format!(
                    "G's time lock is {} s, but `{}` is only for a G at {} s (it sets {} s); nothing was sent",
                    view.time_lock,
                    direction.name(),
                    direction.from_lock(),
                    direction.target_lock()
                )
            });
        }
    }
    refuse(format!(
        "the G multisig {} does not match the expected profile at {} s for `{}`: {}",
        g.multisig,
        direction.from_lock(),
        direction.name(),
        serde_json::to_string(&failing).unwrap_or_default()
    ))
}

fn profile_json(g: &Identity, view: &sq::MultisigView) -> Value {
    let mut members = view.members.clone();
    members.sort_by_key(|(key, _)| *key);
    json!({
        "multisig": g.multisig.to_string(),
        "vault_0": g.vault.to_string(),
        "create_key": view.create_key.to_string(),
        "bump": view.bump,
        "threshold": view.threshold,
        "time_lock_seconds": view.time_lock,
        "members": members
            .iter()
            .map(|(key, mask)| json!({ "key": key.to_string(), "permissions": mask }))
            .collect::<Vec<_>>(),
        "config_authority": view.config_authority.to_string(),
        "rent_collector": view.rent_collector.map(|key| key.to_string()),
        "transaction_index": view.transaction_index,
        "stale_transaction_index": view.stale_transaction_index,
    })
}

fn proposal_json(proposal: &ProposalView) -> Value {
    json!({
        "status": proposal.status.name(),
        "status_timestamp": proposal.status.timestamp(),
        "approved": key_list(&proposal.approved),
        "rejected": key_list(&proposal.rejected),
        "cancelled": key_list(&proposal.cancelled),
    })
}

/// One G transaction slot: the transaction account (classified), its decoded config body when it is a config
/// transaction, and its proposal.
struct GTx {
    index: u64,
    /// `config`, `vault`, `batch`, `other` or `missing`.
    kind: &'static str,
    config: Option<Result<ConfigTransactionView, String>>,
    proposal: Option<ProposalView>,
}

impl GTx {
    /// A config transaction of THIS multisig and index that stores exactly `[SetTimeLock { target }]`.
    fn is_exact(&self, g: &Identity, target: u32) -> bool {
        matches!(
            &self.config,
            Some(Ok(config)) if config.multisig == g.multisig
                && config.index == self.index
                && is_exactly_set_time_lock(&config.actions, target)
        )
    }

    fn to_json(&self) -> Value {
        json!({
            "index": self.index,
            "kind": self.kind,
            "creator": match &self.config { Some(Ok(config)) => json!(config.creator.to_string()), _ => Value::Null },
            "actions": match &self.config {
                Some(Ok(config)) => actions_json(&config.actions),
                Some(Err(error)) => json!({ "undecodable": error }),
                None => Value::Null,
            },
            "proposal": self.proposal.as_ref().map(proposal_json),
        })
    }
}

fn load_g_tx(ctx: &Ctx<'_>, g: &Identity, index: u64) -> Result<GTx, OpsError> {
    let account = ctx.account(&sq::transaction_address(&g.multisig, index))?;
    let (kind, config) = match &account {
        None => ("missing", None),
        Some(account) => {
            if account.owner != SQUADS_PROGRAM_ID {
                return refuse(format!("the G transaction #{index} account is not Squads-owned"));
            }
            let discriminator = account.data.get(..8);
            if discriminator == Some(&account_discriminator("ConfigTransaction")[..]) {
                ("config", Some(decode_config_transaction(&account.data)))
            } else if discriminator == Some(&account_discriminator("VaultTransaction")[..]) {
                ("vault", None)
            } else if discriminator == Some(&account_discriminator("Batch")[..]) {
                ("batch", None)
            } else {
                ("other", None)
            }
        }
    };
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
    Ok(GTx { index, kind, config, proposal })
}

/// The config transaction at `index` and its proposal, for approve / execute: it must BE the intended
/// `[SetTimeLock { target }]` and nothing else, belong to G at this index, and have a proposal.
fn load_checked_config(
    ctx: &Ctx<'_>,
    g: &Identity,
    index: u64,
    direction: Direction,
) -> Result<(ConfigTransactionView, ProposalView), OpsError> {
    let tx = load_g_tx(ctx, g, index)?;
    let config = match tx.config {
        Some(Ok(config)) => config,
        Some(Err(error)) => {
            return refuse(format!("config transaction #{index} cannot be decoded ({error}); not voting on / executing it"))
        }
        None if tx.kind == "missing" => return refuse(format!("there is no transaction #{index} on G {}", g.multisig)),
        None => {
            return refuse(format!(
                "transaction #{index} on G is a {} transaction, not a config transaction; not voting on / executing it",
                tx.kind
            ))
        }
    };
    if config.multisig != g.multisig || config.index != index {
        return refuse(format!("config transaction #{index} does not belong to G {} at this index", g.multisig));
    }
    if !is_exactly_set_time_lock(&config.actions, direction.target_lock()) {
        return refuse(format!(
            "config transaction #{index} stores {} and NOT exactly the intended [SetTimeLock {{ new_time_lock: {} }}] for `{}`; \
             not voting on / executing it",
            serde_json::to_string(&actions_json(&config.actions)).unwrap_or_default(),
            direction.target_lock(),
            direction.name()
        ));
    }
    let Some(proposal) = tx.proposal else {
        return refuse(format!("config transaction #{index} has no proposal yet (run `propose` to resume)"));
    };
    if proposal.multisig != g.multisig || proposal.transaction_index != index {
        return refuse(format!("the proposal at #{index} names another multisig or index"));
    }
    Ok((config, proposal))
}

/// Squads refuses votes on, and execution of, a proposal whose index is at or below the stale index.
fn require_not_stale(view: &sq::MultisigView, index: u64) -> Result<(), OpsError> {
    if index <= view.stale_transaction_index {
        return refuse(format!(
            "proposal #{index} is stale (stale_transaction_index is {}): Squads refuses to vote on or execute it",
            view.stale_transaction_index
        ));
    }
    Ok(())
}

/// What the liveness scan of `(stale_transaction_index, transaction_index]` found.
struct LiveScan {
    /// Transactions that are live, or cannot be proven terminal.
    live: Vec<Value>,
    /// The newest identical (same exact action) transaction whose proposal is live: `(index, status)`.
    identical_live: Option<(u64, &'static str)>,
    /// The newest identical config transaction that has NO proposal yet (resume with `proposal_create` only).
    resumable: Option<u64>,
}

/// Every index in `(stale, transaction_index]`: terminal proposals (Executed, Rejected, Cancelled) are fine; a
/// live proposal, or a transaction with no proposal that is not our own resumable config transaction, is live.
/// A range above [`LIVE_SCAN_CAP`] is REFUSED (a partial scan proves nothing).
fn scan_live(
    ctx: &Ctx<'_>,
    g: &Identity,
    view: &sq::MultisigView,
    target: u32,
) -> Result<LiveScan, OpsError> {
    let first = view.stale_transaction_index + 1;
    let last = view.transaction_index;
    let mut scan = LiveScan { live: Vec::new(), identical_live: None, resumable: None };
    if first > last {
        return Ok(scan);
    }
    if last - first + 1 > LIVE_SCAN_CAP {
        return refuse(format!(
            "G has {} transactions above its stale index {}; the tool scans at most {LIVE_SCAN_CAP} and will not claim that \
             none is live from a partial scan",
            last - first + 1,
            view.stale_transaction_index
        ));
    }
    // Newest first, so the NEWEST resumable config transaction is the one a re-run resumes.
    for index in (first..=last).rev() {
        let tx = load_g_tx(ctx, g, index)?;
        let exact = tx.is_exact(g, target);
        match &tx.proposal {
            Some(proposal) if is_live(&proposal.status) => {
                if exact && scan.identical_live.is_none() {
                    scan.identical_live = Some((index, proposal.status.name()));
                }
                scan.live.push(json!({
                    "index": index, "kind": tx.kind, "proposal_status": proposal.status.name(),
                }));
            }
            Some(_) => {}
            None if exact && scan.resumable.is_none() => scan.resumable = Some(index),
            None => scan.live.push(json!({
                "index": index, "kind": tx.kind, "proposal_status": null,
                "why": "a transaction with no proposal that is not our resumable config transaction",
            })),
        }
    }
    Ok(scan)
}

/// The EXECUTE-time guard (F11-R1): executing a config transaction makes every earlier unexecuted G transaction
/// stale, and an already-Approved stale VAULT transaction can still run afterwards (the real binary does that).
/// So "no other live G transaction" is enforced where it matters, at the moment the config would run, for
/// `reduce` and `restore` alike, in a dry run as well as with `--execute`.  A G transaction can appear during
/// the 48-hour wait, or a config can be hand-made; `propose` having refused earlier proves nothing about now.
///
/// 1. EXACT INDEX: the executed config must be the LATEST G transaction (`index == transaction_index`).
/// 2. FULL BOUNDED SCAN of `(stale_transaction_index, transaction_index]`.  Only `index` itself is excluded, and
///    only because `load_checked_config` has already proven it is exactly our `[SetTimeLock { target }]` config
///    with its proposal.  Every OTHER index must hold a transaction whose proposal is terminal (Executed,
///    Rejected or Cancelled).  A live proposal (Draft, Active, Approved, Executing), a transaction with NO
///    proposal (an orphan: there is NO exception for an identical config), a missing account, an account that
///    cannot be decoded, or a range above [`LIVE_SCAN_CAP`] is a REFUSAL.  Read errors propagate as failures
///    (`load_g_tx` returns them); nothing is ever treated as absent.
///
/// Returns one entry per other index (index, kind, terminal status) for the receipt.  Call it BEFORE
/// `Ctx::run_step`, so a refusal precedes any signing, signed simulation or send.
fn require_no_other_live(
    ctx: &Ctx<'_>,
    g: &Identity,
    view: &sq::MultisigView,
    index: u64,
) -> Result<Vec<Value>, OpsError> {
    if index != view.transaction_index {
        return refuse(format!(
            "config transaction #{index} is not the LATEST G transaction (transaction_index is {}): executing it would stale \
             every other unexecuted G transaction as well, and the exact-index contract is that the executed config is the \
             newest one (re-confirm the index with a dry run); nothing was sent",
            view.transaction_index
        ));
    }
    let first = view.stale_transaction_index.saturating_add(1);
    let last = view.transaction_index;
    if first > last || index < first {
        // Unreachable after `require_not_stale`, but a guard that cannot place the executing index in its own
        // range proves nothing: fail closed.
        return refuse(format!(
            "config transaction #{index} is not inside (stale_transaction_index {}, transaction_index {last}]; nothing was sent",
            view.stale_transaction_index
        ));
    }
    let span = last - first + 1;
    if span > LIVE_SCAN_CAP {
        return refuse(format!(
            "G has {span} transactions above its stale index {}; the tool scans at most {LIVE_SCAN_CAP} and will not claim \
             that none is live from a partial scan; nothing was sent",
            view.stale_transaction_index
        ));
    }
    let mut checked = Vec::new();
    for other in first..=last {
        if other == index {
            continue;
        }
        let tx = load_g_tx(ctx, g, other)?;
        if tx.kind == "missing" {
            return refuse(format!(
                "G transaction #{other} has no transaction account (missing), so it cannot be proven terminal; refusing to \
                 execute config #{index}; nothing was sent"
            ));
        }
        if tx.kind == "other" {
            return refuse(format!(
                "G transaction #{other} is an account of an unrecognized type, so it cannot be proven terminal; refusing to \
                 execute config #{index}; nothing was sent"
            ));
        }
        if let Some(Err(error)) = &tx.config {
            return refuse(format!(
                "G config transaction #{other} cannot be decoded ({error}), so it cannot be proven terminal; refusing to \
                 execute config #{index}; nothing was sent"
            ));
        }
        let Some(proposal) = &tx.proposal else {
            return refuse(format!(
                "G transaction #{other} ({}) has NO proposal: an orphan can still be given one, and an approved one would run \
                 after this config (a stale, already-approved vault transaction still executes); refusing to execute config \
                 #{index}; nothing was sent",
                tx.kind
            ));
        };
        if !matches!(
            proposal.status,
            ProposalStatus::Executed(_) | ProposalStatus::Rejected(_) | ProposalStatus::Cancelled(_)
        ) {
            return refuse(format!(
                "G transaction #{other} ({}) has a LIVE proposal ({}): executing config #{index} would stale it, and a stale \
                 transaction that is already Approved can still execute; refusing; nothing was sent",
                tx.kind,
                proposal.status.name()
            ));
        }
        checked.push(json!({
            "index": other,
            "kind": tx.kind,
            "proposal_status": proposal.status.name(),
            "status_timestamp": proposal.status.timestamp(),
        }));
    }
    Ok(checked)
}

fn pending_check(ctx: &Ctx<'_>) -> Result<Value, OpsError> {
    match &ctx.pending_path {
        None => Ok(check(
            "pending.no_open_or_held_records",
            true,
            "0 open or held records",
            "no --pending-file given (nothing to read)",
        )),
        Some(path) => {
            let log = read_pending(path, &ctx.genesis)?;
            Ok(check(
                "pending.no_open_or_held_records",
                log.open.is_empty(),
                "0 open or held records",
                format!("{} open record(s) in {}", log.open.len(), path.display()),
            ))
        }
    }
}

/// `programdata.upgrade_authority_is_g_vault`.
fn upgrade_authority_check(ctx: &Ctx<'_>, g: &Identity) -> Result<Value, OpsError> {
    let lookup = lookup_program(ctx, &ID)?;
    let authority = lookup
        .programdata_account
        .as_ref()
        .map(|account| decode_programdata_header(&account.data))
        .transpose()
        .map_err(|error| OpsError::Failed(format!("ProgramData header: {error}")))?
        .and_then(|header| header.authority);
    Ok(check(
        "programdata.upgrade_authority_is_g_vault",
        authority == Some(g.vault),
        g.vault,
        authority.map_or("absent or immutable".to_string(), |key| key.to_string()),
    ))
}

/// The program-side state a `reduce` stands on, read-only.
struct ProgramRead {
    checks: Vec<Value>,
    pool: Option<state::Pool>,
}

/// Upgrade authority, Pool, ConfigVersion#1 and the ORAO Client, as checks (the R0 list).
fn read_program_checks(ctx: &Ctx<'_>, g: &Identity) -> Result<ProgramRead, OpsError> {
    let mut checks = vec![upgrade_authority_check(ctx, g)?];
    let pool_key = pda(&[state::POOL_SEED]).0;
    let pool_account = ctx.account(&pool_key)?;
    let pool = pool_account
        .as_ref()
        .filter(|account| account.owner == ID)
        .and_then(|account| anchor::<state::Pool>(account).ok());
    checks.push(check(
        "pool.initialized_and_program_owned",
        pool.is_some(),
        format!("{pool_key} owned by the program and decodable as a Pool"),
        match (&pool_account, &pool) {
            (None, _) => "absent".to_string(),
            (Some(account), None) => format!("owner {}, not a Pool", account.owner),
            (Some(_), Some(_)) => "a Pool".to_string(),
        },
    ));
    let config_key = pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).0;
    let config = ctx
        .account(&config_key)?
        .filter(|account| account.owner == ID)
        .and_then(|account| anchor::<state::ConfigVersion>(&account).ok());
    let active = pool.as_ref().map(|pool| pool.active_config_version);
    checks.push(check(
        "pool.config_version_1_active",
        active.is_some_and(|version| version >= 1) && config.as_ref().is_some_and(|config| config.version == 1),
        "Pool.active_config_version >= 1 and ConfigVersion#1 present",
        format!(
            "active_config_version {:?}, ConfigVersion#1 {}",
            active,
            if config.is_some() { "present" } else { "absent" }
        ),
    ));
    let client_key = provider::client_address().0;
    let rng_authority = provider::rng_authority_address().0;
    let client_account = ctx.account(&client_key)?;
    let client = client_account
        .as_ref()
        .filter(|account| account.owner == orao::id())
        .and_then(|account| anchor::<orao::state::client::Client>(account).ok());
    checks.push(check(
        "orao_client.exists_and_orao_owned",
        client.is_some(),
        format!("{client_key} owned by the ORAO program and decodable as a Client"),
        client_account.as_ref().map_or("absent".to_string(), |account| format!("owner {}", account.owner)),
    ));
    checks.push(check(
        "orao_client.owner_is_g_vault",
        client.as_ref().is_some_and(|client| client.owner == g.vault),
        g.vault,
        client.as_ref().map_or("no Client".to_string(), |client| client.owner.to_string()),
    ));
    checks.push(check(
        "orao_client.state_is_rng_authority",
        client.as_ref().is_some_and(|client| client.state == rng_authority),
        rng_authority,
        client.as_ref().map_or("no Client".to_string(), |client| client.state.to_string()),
    ));
    Ok(ProgramRead { checks, pool })
}

// ---------------------------------------------------------------------------
// The consequence probe (RG-8): SIMULATION ONLY
// ---------------------------------------------------------------------------

/// `InstructionError(0, Custom(6031))` (Debug form) or `... custom program error: 0x178f` (Display form).
pub fn parse_custom_code(error: &str) -> Option<u32> {
    if let Some(rest) = error.split("Custom(").nth(1) {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(code) = digits.parse() {
            return Some(code);
        }
    }
    let rest = error.split("custom program error: 0x").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
    u32::from_str_radix(&digits, 16).ok()
}

/// Anchor logs `Error Code: <Name>. Error Number: <code>.`; the name whose number is `code`.
pub fn anchor_error_name(logs: &[String], code: u32) -> Option<String> {
    let number = format!("Error Number: {code}.");
    logs.iter().find(|line| line.contains(&number)).and_then(|line| {
        let after = line.split("Error Code: ").nth(1)?;
        Some(after.split(". Error Number:").next()?.trim().to_string())
    })
}

struct ProbeRun {
    receipt: Value,
    wrong_governance_profile: bool,
    /// `true` when the result is the one this G time lock must produce.
    matches_expectation: bool,
}

/// Simulate (never send) the no-op `rotate_recipient` with the G vault as a signer FLAG, fee payer `fee_payer`,
/// signature verification off.  There is no keypair parameter and no call to the send path in this function.
fn run_probe(
    ctx: &Ctx<'_>,
    g: &Identity,
    time_lock: u32,
    pool: &state::Pool,
    fee_payer: Pubkey,
) -> Result<ProbeRun, OpsError> {
    let instruction = noop_rotate_recipient_ix(g, pool.management_recipient);
    let step = Step {
        id: "F11.probe.rotate_recipient_noop".into(),
        label: "simulate rotate_recipient(kind 0, new_recipient = the current management recipient), G vault as signer flag".into(),
        instructions: vec![instruction.clone()],
        payer: fee_payer,
        // The same ceiling a Squads execute transaction asks for, so compute exhaustion cannot masquerade as the
        // result being looked for.
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    };
    let built = build_step(&step);
    if !built.fits() {
        return refuse(format!("the probe transaction is {} bytes, over the packet limit", built.packet_bytes));
    }
    if !built.required_signers.contains(&g.vault) {
        return fail("internal: the G vault is not a signer flag of the probe message");
    }
    let blockhash = ctx.cluster.blockhash().map_err(|error| rpc_err("getLatestBlockhash", error))?;
    let mut message = built.message.clone();
    message.recent_blockhash = blockhash;
    let unsigned = Transaction::new_unsigned(message);
    // SIMULATION ONLY, sigVerify off: the transaction is not signed and is never handed to `Cluster::send`.
    let simulation = ctx
        .cluster
        .simulate(&unsigned, false)
        .map_err(|error| rpc_err("simulateTransaction (the probe)", error))?;
    let error_code = simulation.error.as_deref().and_then(parse_custom_code);
    let error_name = error_code.and_then(|code| {
        anchor_error_name(&simulation.logs, code).or_else(|| match code {
            WRONG_GOVERNANCE_PROFILE_CODE => Some("WrongGovernanceProfile".to_string()),
            NOT_AUTHORIZED_CODE => Some("NotAuthorized".to_string()),
            _ => None,
        })
    });
    let wrong_governance_profile = error_code == Some(WRONG_GOVERNANCE_PROFILE_CODE);
    let normal = time_lock == T_NORMAL;
    // At 172,800 s the no-op must simply succeed; at any other time lock it must be refused with exactly
    // WrongGovernanceProfile.  Any OTHER error would make "not WrongGovernanceProfile" a vacuous baseline.
    let matches_expectation = if normal { simulation.error.is_none() } else { wrong_governance_profile };
    let tail: Vec<&String> = simulation.logs.iter().rev().take(8).collect();
    let accounts: Vec<Value> = instruction
        .accounts
        .iter()
        .map(|meta| json!({ "key": meta.pubkey.to_string(), "signer": meta.is_signer, "writable": meta.is_writable }))
        .collect();
    let receipt = json!({
        "simulated_only": true,
        "sent": false,
        "instruction": format!(
            "rotate_recipient(kind = 0, new_recipient = {}) [the current Pool.management_recipient: a no-op when it runs]",
            pool.management_recipient
        ),
        "program": ID.to_string(),
        "instruction_accounts": accounts,
        "governance_multisig": g.multisig.to_string(),
        "governance_signer_g_vault": g.vault.to_string(),
        "g_vault_is_a_signer_flag_of_an_unsigned_message": true,
        "fee_payer": fee_payer.to_string(),
        "packet_bytes": built.packet_bytes,
        "signature_verification": false,
        "g_time_lock_seconds": time_lock,
        "simulation_error": simulation.error,
        "units_consumed": simulation.units_consumed,
        "error_code": error_code,
        "error_name": error_name,
        "wrong_governance_profile": wrong_governance_profile,
        "expected": if normal { "no program error (the no-op succeeds)" } else { "WrongGovernanceProfile" },
        "expected_wrong_governance_profile": !normal,
        "logs_tail": tail.into_iter().rev().collect::<Vec<_>>(),
    });
    Ok(ProbeRun { receipt, wrong_governance_profile, matches_expectation })
}

fn cmd_probe(ctx: &Ctx<'_>, g: &Identity, members: &[Pubkey]) -> Result<Value, OpsError> {
    let fee_payer = key_flag(ctx.opts, "member")?.pubkey;
    let (profile, view) = profile_at(ctx, g, members, T_NORMAL)?;
    let Some(view) = view else {
        return refuse(format!(
            "the G multisig {} is absent or undecodable; there is nothing to probe: {}",
            g.multisig,
            serde_json::to_string(&failing_checks(&profile)).unwrap_or_default()
        ));
    };
    let pool_key = pda(&[state::POOL_SEED]).0;
    let pool = ctx
        .account(&pool_key)?
        .filter(|account| account.owner == ID)
        .and_then(|account| anchor::<state::Pool>(&account).ok());
    let Some(pool) = pool else {
        return refuse("the Pool is not initialized: there is no management recipient to build the no-op probe from");
    };
    let run = run_probe(ctx, g, view.time_lock, &pool, fee_payer)?;
    let normal = view.time_lock == T_NORMAL;
    let checks = vec![
        check(
            "probe.wrong_governance_profile_iff_time_lock_is_not_172800",
            run.wrong_governance_profile != normal,
            format!("WrongGovernanceProfile = {}", !normal),
            format!("WrongGovernanceProfile = {}", run.wrong_governance_profile),
        ),
        check(
            "probe.no_error_other_than_the_expected_one",
            run.matches_expectation,
            if normal { "no program error".to_string() } else { "error 6031 WrongGovernanceProfile".to_string() },
            run.receipt["simulation_error"].as_str().unwrap_or("no error").to_string(),
        ),
    ];
    let mut receipt = json!({
        "command": "g-timelock",
        "subcommand": "probe",
        "mode": "read-only",
        "g_profile": profile_json(g, &view),
        "g_profile_checks_at_172800": profile,
        "checks": checks,
        "all_ok": checks_ok(&checks),
    });
    if let (Some(target), Some(source)) = (receipt.as_object_mut(), run.receipt.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(receipt)
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

/// `(phase, detail, earliest execution)` from the live G profile and the newest config transactions.
fn derive_phase(
    g: &Identity,
    view: &sq::MultisigView,
    entries: &[GTx],
    now: i64,
) -> (&'static str, String, Value) {
    let lock = view.time_lock;
    let stale = view.stale_transaction_index;
    // The newest exact SetTimeLock{target} config transaction above the stale index that can still run.
    let live = |target: u32| {
        entries.iter().rev().find(|entry| {
            entry.index > stale
                && entry.is_exact(g, target)
                && entry.proposal.as_ref().is_none_or(|proposal| is_live(&proposal.status))
        })
    };
    // The last config change that executed is the transaction AT the stale index.
    let last_executed = |target: u32| {
        stale > 0
            && entries.iter().any(|entry| {
                entry.index == stale
                    && entry.is_exact(g, target)
                    && matches!(entry.proposal.as_ref().map(|p| p.status), Some(ProposalStatus::Executed(_)))
            })
    };
    let earliest_for = |entry: &GTx| -> Value {
        match entry.proposal.as_ref().map(|proposal| proposal.status) {
            Some(ProposalStatus::Approved(approved_at)) => {
                let releases_at = approved_at + i64::from(lock);
                json!({
                    "transaction_index": entry.index,
                    "approved_at": approved_at,
                    "time_lock_seconds": lock,
                    "releases_at": releases_at,
                    "cluster_clock": now,
                    "seconds_until_release": releases_at - now,
                    "executable_now": now >= releases_at,
                })
            }
            _ => Value::Null,
        }
    };
    let proposed = |entry: &GTx, noun: &str| -> (&'static str, String, Value) {
        let (phase, text) = match (noun, entry.proposal.as_ref().map(|proposal| proposal.status)) {
            ("reduce", Some(ProposalStatus::Approved(_))) => ("reduce_approved", "reduce approved"),
            ("reduce", Some(_)) => ("reduce_proposed", "reduce proposed"),
            ("reduce", None) => ("reduce_proposed", "reduce config transaction created, proposal not yet created"),
            (_, Some(ProposalStatus::Approved(_))) => ("restore_approved", "restore approved"),
            (_, Some(_)) => ("restore_proposed", "restore proposed"),
            (_, None) => ("restore_proposed", "restore config transaction created, proposal not yet created"),
        };
        let earliest = earliest_for(entry);
        let detail = match earliest["releases_at"].as_i64() {
            Some(at) => format!("{text} (transaction #{}; executable at cluster time {at})", entry.index),
            None => format!("{text} (transaction #{})", entry.index),
        };
        (phase, detail, earliest)
    };
    if lock == T_NORMAL {
        if let Some(entry) = live(T_REDUCED) {
            return proposed(entry, "reduce");
        }
        if last_executed(T_NORMAL) {
            return ("restored", format!("restored: the last config change (#{stale}) set {T_NORMAL} s"), Value::Null);
        }
        return ("normal", format!("normal: time lock {T_NORMAL} s, no reduce in flight"), Value::Null);
    }
    if lock == T_REDUCED {
        if let Some(entry) = live(T_NORMAL) {
            return proposed(entry, "restore");
        }
        if last_executed(T_REDUCED) {
            return ("reduced", format!("reduced: the last config change (#{stale}) set 0 s"), Value::Null);
        }
        return (
            "reduced_unrecorded",
            "time lock is 0 s but no executed reduce was found in the scanned window".into(),
            Value::Null,
        );
    }
    ("other", format!("time lock is {lock} s, neither {T_NORMAL} nor {T_REDUCED}"), Value::Null)
}

fn cmd_status(ctx: &Ctx<'_>, g: &Identity, members: &[Pubkey]) -> Result<Value, OpsError> {
    let (normal_checks, view) = profile_at(ctx, g, members, T_NORMAL)?;
    let Some(view) = view else {
        return refuse(format!(
            "the G multisig {} is absent or undecodable: {}",
            g.multisig,
            serde_json::to_string(&failing_checks(&normal_checks)).unwrap_or_default()
        ));
    };
    let (reduced_checks, _) = profile_at(ctx, g, members, T_REDUCED)?;
    let matches_normal = checks_ok(&normal_checks);
    let matches_reduced = checks_ok(&reduced_checks);
    let now = cluster_clock(ctx)?;

    let hi = view.transaction_index;
    let lo = view
        .stale_transaction_index
        .max(hi.saturating_sub(STATUS_SCAN_WINDOW - 1))
        .max(1);
    let mut entries = Vec::new();
    if hi >= lo {
        for index in lo..=hi {
            entries.push(load_g_tx(ctx, g, index)?);
        }
    }
    let complete = hi == 0 || lo <= view.stale_transaction_index.max(1);
    let (phase, detail, earliest) = derive_phase(g, &view, &entries, now);
    let program = read_program_checks(ctx, g)?;
    let pending = pending_check(ctx)?;
    Ok(json!({
        "command": "g-timelock",
        "subcommand": "status",
        "mode": "read-only",
        "g": {
            "create_key": g.create_key.to_string(),
            "multisig": g.multisig.to_string(),
            "vault_0": g.vault.to_string(),
        },
        "g_profile": profile_json(g, &view),
        "profile_matches_normal_172800": matches_normal,
        "profile_matches_reduced_0": matches_reduced,
        "profile_failing_checks_at_172800": failing_checks(&normal_checks),
        "transaction_index": view.transaction_index,
        "stale_transaction_index": view.stale_transaction_index,
        "phase": phase,
        "phase_detail": detail,
        "earliest_execution": earliest,
        "cluster_clock": now,
        "scan": { "from": lo, "to": hi, "complete": complete },
        "transactions": entries.iter().map(GTx::to_json).collect::<Vec<_>>(),
        "program_checks": program.checks,
        "pending": pending,
        // A status is informational; it is "not ok" only when G matches NEITHER known profile.
        "all_ok": matches_normal || matches_reduced,
    }))
}

// ---------------------------------------------------------------------------
// propose / approve / execute
// ---------------------------------------------------------------------------

fn step_create(direction: Direction, g: &Identity, member: Pubkey, index: u64, actions: &[ConfigAction]) -> Step {
    Step {
        id: direction.step_id("config_transaction_create"),
        label: format!(
            "squads config_transaction_create #{index} [SetTimeLock {}] (proposer = fee payer)",
            direction.target_lock()
        ),
        instructions: vec![config_transaction_create_ix(g.multisig, index, member, member, actions)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    }
}

fn step_proposal(direction: Direction, g: &Identity, member: Pubkey, index: u64) -> Step {
    Step {
        id: direction.step_id("proposal_create"),
        label: format!("squads proposal_create #{index} ({})", direction.name()),
        instructions: vec![sq::proposal_create_ix(g.multisig, index, member, member, false)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: true,
    }
}

fn step_vote(direction: Direction, g: &Identity, member: Pubkey, index: u64) -> Step {
    Step {
        id: direction.step_id("proposal_approve"),
        label: format!("squads proposal_approve #{index} by {member} ({})", direction.name()),
        instructions: vec![sq::proposal_approve_ix(g.multisig, index, member)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    }
}

fn step_run(direction: Direction, g: &Identity, member: Pubkey, index: u64) -> Step {
    Step {
        id: direction.step_id("config_transaction_execute"),
        label: format!("squads config_transaction_execute #{index} ({})", direction.name()),
        instructions: vec![config_transaction_execute_ix(g.multisig, index, member)],
        payer: member,
        cu_limit: Some(CU_SQUADS),
        depends_on_previous: false,
    }
}

fn cmd_propose_tl(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
    direction: Direction,
) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let view = require_profile_for(ctx, g, members, direction)?;
    if !member_has(&view, &member.pubkey, SQUADS_INITIATE) {
        return refuse(format!("{} is not a G member with the Initiate permission", member.pubkey));
    }
    let actions = direction.actions();

    // A re-run must neither duplicate a live proposal nor leave a half-made one behind, and a reduce or a
    // restore must not run while ANY other G transaction is live (executing it would stale that transaction).
    let scan = scan_live(ctx, g, &view, direction.target_lock())?;
    if let Some((candidate, status)) = scan.identical_live {
        return refuse(format!(
            "an identical `{}` proposal already exists at #{candidate} (status {status}); use approve / execute",
            direction.name()
        ));
    }
    let (index, resume_proposal_only) = match scan.resumable {
        Some(candidate) => (candidate, true),
        None => (view.transaction_index + 1, false),
    };

    let mut preconditions = vec![
        check(
            "g.profile_exact_at_from_lock",
            true,
            format!("G profile exact with time lock {} s", direction.from_lock()),
            format!("time lock {} s", view.time_lock),
        ),
        pending_check(ctx)?,
        check(
            "g.no_live_transaction_above_stale_index",
            scan.live.is_empty(),
            "no live G transaction in (stale_transaction_index, transaction_index]",
            if scan.live.is_empty() { "none".to_string() } else { serde_json::to_string(&scan.live).unwrap_or_default() },
        ),
    ];
    if direction == Direction::Reduce {
        let program = read_program_checks(ctx, g)?;
        preconditions.extend(program.checks);
        let baseline = match &program.pool {
            Some(pool) => {
                let run = run_probe(ctx, g, view.time_lock, pool, member.pubkey)?;
                check(
                    "probe.baseline_succeeds_and_is_not_wrong_governance_profile",
                    run.matches_expectation && !run.wrong_governance_profile,
                    "the no-op rotate_recipient simulates without a program error",
                    run.receipt["simulation_error"].as_str().unwrap_or("no error").to_string(),
                )
            }
            None => check(
                "probe.baseline_succeeds_and_is_not_wrong_governance_profile",
                false,
                "the baseline probe ran",
                "no Pool to build the probe from",
            ),
        };
        preconditions.push(baseline);
    }
    let config_rent = ctx.rent(config_transaction_bytes(&actions))?;
    let proposal_rent = ctx.rent(PROPOSAL_ACCOUNT_BYTES)?;
    let balance = ctx.balance(&member.pubkey)?;
    let needed = (if resume_proposal_only { 0 } else { config_rent })
        + proposal_rent
        + 2 * SIGNATURE_FEE_LAMPORTS
        + ctx.rent(0)?;
    preconditions.push(check(
        "proposer.balance_covers_rent_fees_and_its_own_floor",
        balance >= needed,
        format!(">= {needed} lamports"),
        balance,
    ));
    refuse_if_failing(&format!("propose {}", direction.name()), &preconditions)?;

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

    let create = step_create(direction, g, member.pubkey, index, &actions);
    let proposal_step = step_proposal(direction, g, member.pubkey, index);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![member.keypair()?] } else { Vec::new() };
    let mut steps = Vec::new();
    if !resume_proposal_only {
        steps.push(ctx.run_step(&create, &signers)?.to_json());
    }
    steps.push(ctx.run_step(&proposal_step, &signers)?.to_json());

    let mut checks = Vec::new();
    let mut stored = Value::Null;
    let mut status = json!("not created (dry run)");
    let mut after = Value::Null;
    if ctx.executing() {
        let tx = load_g_tx(ctx, g, index)?;
        let config = tx.config.as_ref().and_then(|result| result.as_ref().ok());
        stored = config.map_or(Value::Null, |config| actions_json(&config.actions));
        checks.push(check(
            "config_transaction.stores_exactly_the_intended_actions",
            config.is_some_and(|config| config.actions == actions),
            serde_json::to_string(&actions_json(&actions)).unwrap_or_default(),
            serde_json::to_string(&stored).unwrap_or_default(),
        ));
        checks.push(check(
            "config_transaction.multisig_index_and_creator",
            config.is_some_and(|config| {
                config.multisig == g.multisig && config.index == index && config.creator == member.pubkey
            }),
            format!("multisig {}, index {index}, creator {}", g.multisig, member.pubkey),
            config.map_or("no decodable config transaction".to_string(), |config| {
                format!("multisig {}, index {}, creator {}", config.multisig, config.index, config.creator)
            }),
        ));
        let proposal_status = tx.proposal.as_ref().map(|proposal| proposal.status);
        checks.push(check(
            "proposal.is_active",
            matches!(proposal_status, Some(ProposalStatus::Active(_))),
            "Active",
            proposal_status.map_or("no proposal", |status| status.name()),
        ));
        let now_view = read_view(ctx, g)?;
        let expected_index = if resume_proposal_only { view.transaction_index } else { index };
        checks.push(check(
            "multisig.transaction_index_covers_the_proposal",
            now_view.transaction_index == expected_index,
            expected_index,
            now_view.transaction_index,
        ));
        status = json!(proposal_status.map(|status| status.name()));
        after = profile_json(g, &now_view);
    }
    Ok(json!({
        "command": "g-timelock",
        "subcommand": "propose",
        "mode": ctx.mode(),
        "action": direction.name(),
        "multisig": g.multisig.to_string(),
        "g_vault": g.vault.to_string(),
        "transaction_index": index,
        "config_transaction": sq::transaction_address(&g.multisig, index).to_string(),
        "proposal": sq::proposal_address(&g.multisig, index).to_string(),
        "proposer_is_creator_rent_payer_and_fee_payer": member.pubkey.to_string(),
        "intended_actions": actions_json(&actions),
        "stored_actions": stored,
        "time_lock_before": view.time_lock,
        "time_lock_target": direction.target_lock(),
        "resumed_proposal_only": resume_proposal_only,
        "g_profile_before": profile_json(g, &view),
        "g_profile_after": after,
        "approved_at": null,
        "time_lock_releases_at": null,
        "cluster_clock": cluster_clock(ctx)?,
        "preconditions": preconditions,
        "rent_lamports_estimate": {
            "config_transaction": config_rent,
            "config_transaction_bytes": config_transaction_bytes(&actions),
            "proposal": proposal_rent,
            "basis": "getMinimumBalanceForRentExemption on this cluster; the config-transaction and proposal sizes are formula values",
        },
        "proposer_balance_lamports": balance,
        "proposal_status": status,
        "steps": steps,
        "checks": checks,
        "all_ok": checks_ok(&checks),
    }))
}

fn cmd_approve_tl(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
    direction: Direction,
) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let view = require_profile_for(ctx, g, members, direction)?;
    if !member_has(&view, &member.pubkey, SQUADS_VOTE) {
        return refuse(format!("{} is not a G member with the Vote permission", member.pubkey));
    }
    let index = opts
        .u64_flag("tx-index")?
        .ok_or_else(|| OpsError::Usage("`g-timelock approve` needs --tx-index".into()))?;
    let mut preconditions = Vec::new();
    if direction == Direction::Reduce {
        preconditions.push(upgrade_authority_check(ctx, g)?);
        refuse_if_failing("approve reduce", &preconditions)?;
    }
    let (config, proposal) = load_checked_config(ctx, g, index, direction)?;
    require_not_stale(&view, index)?;
    if !matches!(proposal.status, ProposalStatus::Active(_)) {
        return refuse(format!("proposal #{index} is {}, not Active; nothing to approve", proposal.status.name()));
    }
    if proposal.approved.contains(&member.pubkey) || proposal.rejected.contains(&member.pubkey) {
        return refuse(format!("{} has already voted on proposal #{index}", member.pubkey));
    }
    let before_approvals = proposal.approved.len();
    let step = step_vote(direction, g, member.pubkey, index);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![member.keypair()?] } else { Vec::new() };
    let result = ctx.run_step(&step, &signers)?;

    let mut checks = Vec::new();
    let mut after = Value::Null;
    let mut approved_at = Value::Null;
    let mut releases_at = Value::Null;
    let mut profile_after = Value::Null;
    if ctx.executing() {
        let (_, now_proposal) = load_checked_config(ctx, g, index, direction)?;
        let now_view = read_view(ctx, g)?;
        checks.push(check(
            "the member's approval is recorded",
            now_proposal.approved.contains(&member.pubkey),
            true,
            now_proposal.approved.contains(&member.pubkey),
        ));
        checks.push(check(
            "approvals += 1",
            now_proposal.approved.len() == before_approvals + 1,
            before_approvals + 1,
            now_proposal.approved.len(),
        ));
        let reached = now_proposal.approved.len() >= usize::from(view.threshold);
        checks.push(check(
            "proposal.is_approved_exactly_when_the_threshold_is_reached",
            matches!(now_proposal.status, ProposalStatus::Approved(_)) == reached,
            if reached { "Approved" } else { "Active" },
            now_proposal.status.name(),
        ));
        checks.push(check(
            "multisig.time_lock_unchanged_by_a_vote",
            now_view.time_lock == view.time_lock,
            view.time_lock,
            now_view.time_lock,
        ));
        if let ProposalStatus::Approved(timestamp) = now_proposal.status {
            approved_at = json!(timestamp);
            releases_at = json!(timestamp + i64::from(now_view.time_lock));
        }
        after = json!({
            "status": now_proposal.status.name(),
            "approvals": now_proposal.approved.len(),
            "threshold": view.threshold,
            "approved_at": approved_at,
            "executable_after_unix": releases_at,
            "time_lock_seconds": now_view.time_lock,
        });
        profile_after = profile_json(g, &now_view);
    }
    Ok(json!({
        "command": "g-timelock",
        "subcommand": "approve",
        "mode": ctx.mode(),
        "action": direction.name(),
        "transaction_index": index,
        "config_transaction": sq::transaction_address(&g.multisig, index).to_string(),
        "proposal": sq::proposal_address(&g.multisig, index).to_string(),
        "member": member.pubkey.to_string(),
        "stored_actions": actions_json(&config.actions),
        "approvals_before": before_approvals,
        "after": after,
        "g_profile_before": profile_json(g, &view),
        "g_profile_after": profile_after,
        "approved_at": approved_at,
        "time_lock_releases_at": releases_at,
        "cluster_clock": cluster_clock(ctx)?,
        "preconditions": preconditions,
        "steps": [result.to_json()],
        "checks": checks,
        "all_ok": checks_ok(&checks),
    }))
}

/// The post-execution facts (one `check` each): the proposal Executed, the time lock at its target, nothing else
/// of the profile changed, and the stale index at the executed transaction.
fn execute_checks(
    g: &Identity,
    members: &[Pubkey],
    direction: Direction,
    index: u64,
    views: (&sq::MultisigView, &sq::MultisigView),
    after_account: Option<&Account>,
    proposal: Option<&ProposalView>,
) -> Vec<Value> {
    let (before, after) = views;
    let sorted = |view: &sq::MultisigView| {
        let mut list = view.members.clone();
        list.sort_by_key(|(key, _)| *key);
        list
    };
    let mut checks = vec![
        check(
            "proposal.is_executed",
            matches!(proposal.map(|proposal| proposal.status), Some(ProposalStatus::Executed(_))),
            "Executed",
            proposal.map_or("no proposal", |proposal| proposal.status.name()),
        ),
        check(
            "multisig.time_lock_is_the_target",
            after.time_lock == direction.target_lock(),
            direction.target_lock(),
            after.time_lock,
        ),
        check("multisig.threshold_unchanged", after.threshold == before.threshold, before.threshold, after.threshold),
        check(
            "multisig.members_and_permissions_unchanged",
            sorted(after) == sorted(before),
            format!("{:?}", sorted(before)),
            format!("{:?}", sorted(after)),
        ),
        check(
            "multisig.config_authority_unchanged",
            after.config_authority == before.config_authority,
            before.config_authority,
            after.config_authority,
        ),
        check(
            "multisig.rent_collector_unchanged",
            after.rent_collector == before.rent_collector,
            format!("{:?}", before.rent_collector),
            format!("{:?}", after.rent_collector),
        ),
        check("multisig.create_key_unchanged", after.create_key == before.create_key, before.create_key, after.create_key),
        check("multisig.bump_unchanged", after.bump == before.bump, before.bump, after.bump),
        check(
            "multisig.vault_0_unchanged",
            fixtures::squads::vault_address(&g.multisig, 0).0 == g.vault,
            g.vault,
            fixtures::squads::vault_address(&g.multisig, 0).0,
        ),
        check(
            "multisig.transaction_index_unchanged",
            after.transaction_index == before.transaction_index,
            before.transaction_index,
            after.transaction_index,
        ),
        // Squads invalidates every transaction up to the LATEST one, which is the executed one in this flow
        // (`propose` refuses while anything else is live): so stale == index == transaction_index.
        check(
            "multisig.stale_transaction_index_covers_the_executed_index",
            after.stale_transaction_index >= index,
            format!(">= {index}"),
            after.stale_transaction_index,
        ),
        check(
            "multisig.stale_transaction_index_equals_transaction_index",
            after.stale_transaction_index == after.transaction_index,
            after.transaction_index,
            after.stale_transaction_index,
        ),
    ];
    // The whole profile, once more, at the TARGET lock (the same checks `readback` runs).
    let (profile, _) = profile_checks(
        &ProfileSpec { name: "G", identity: *g, members, time_lock: direction.target_lock() },
        after_account,
    );
    checks.extend(profile);
    checks
}

fn cmd_execute_tl(
    ctx: &Ctx<'_>,
    g: &Identity,
    members: &[Pubkey],
    direction: Direction,
) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let member = key_flag(opts, "member")?;
    let view = require_profile_for(ctx, g, members, direction)?;
    if !member_has(&view, &member.pubkey, SQUADS_EXECUTE) {
        return refuse(format!("{} is not a G member with the Execute permission", member.pubkey));
    }
    let index = opts
        .u64_flag("tx-index")?
        .ok_or_else(|| OpsError::Usage("`g-timelock execute` needs --tx-index".into()))?;
    let mut preconditions = Vec::new();
    if direction == Direction::Reduce {
        preconditions.push(upgrade_authority_check(ctx, g)?);
        refuse_if_failing("execute reduce", &preconditions)?;
    }
    let (config, proposal) = load_checked_config(ctx, g, index, direction)?;
    require_not_stale(&view, index)?;
    // F11-R1: the config must be the latest G transaction and every OTHER G transaction above the stale index
    // must be terminal.  Before the time-lock check and long before `run_step`: a refusal here is before any
    // signing, signed simulation or send, in a dry run and with `--execute` alike.
    let other_transactions = require_no_other_live(ctx, g, &view, index)?;
    preconditions.push(check(
        "g.executing_config_is_the_latest_and_every_other_transaction_above_stale_is_terminal",
        true,
        format!("config #{index} == transaction_index {}; every other index in (stale, transaction_index] terminal", view.transaction_index),
        format!("{} other transaction(s) checked, all terminal", other_transactions.len()),
    ));
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
    // Squads: `now - approved_at >= time_lock` with the CURRENT multisig time lock, on the cluster's own clock.
    let now = cluster_clock(ctx)?;
    let releases_at = approved_at + i64::from(view.time_lock);
    if now < releases_at {
        return refuse(format!(
            "time lock not released: approved at {approved_at}, lock {} s, releases at {releases_at}, cluster clock {now} \
             ({} s to go); executing now would fail with Squads error 6021 after paying a fee",
            view.time_lock,
            releases_at - now
        ));
    }
    let step = step_run(direction, g, member.pubkey, index);
    let signers: Vec<&Keypair> = if ctx.executing() { vec![member.keypair()?] } else { Vec::new() };
    let result = ctx.run_step(&step, &signers)?;

    let mut checks = Vec::new();
    let mut profile_after = Value::Null;
    if ctx.executing() {
        let after_account = ctx.account(&g.multisig)?;
        let after = read_view(ctx, g)?;
        let tx = load_g_tx(ctx, g, index)?;
        checks = execute_checks(
            g,
            members,
            direction,
            index,
            (&view, &after),
            after_account.as_ref(),
            tx.proposal.as_ref(),
        );
        profile_after = profile_json(g, &after);
    }
    Ok(json!({
        "command": "g-timelock",
        "subcommand": "execute",
        "mode": ctx.mode(),
        "action": direction.name(),
        "transaction_index": index,
        "config_transaction": sq::transaction_address(&g.multisig, index).to_string(),
        "proposal": sq::proposal_address(&g.multisig, index).to_string(),
        "member": member.pubkey.to_string(),
        "stored_actions": actions_json(&config.actions),
        "time_lock_before": view.time_lock,
        "time_lock_target": direction.target_lock(),
        "g_profile_before": profile_json(g, &view),
        "g_profile_after": profile_after,
        "approved_at": approved_at,
        "time_lock_releases_at": releases_at,
        "cluster_clock": now,
        "preconditions": preconditions,
        "other_g_transactions_checked": other_transactions,
        "steps": [result.to_json()],
        "checks": checks,
        "all_ok": checks_ok(&checks),
    }))
}
