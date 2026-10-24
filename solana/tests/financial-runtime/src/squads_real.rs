//! M14-PRE-SQUADS: the REAL Squads v4 program (the executable deployed on
//! devnet, `solana/fixtures/devnet/squads-devnet.so`) in LiteSVM, driven by
//! hand-built instructions.
//!
//! Everything else in the harness uses the test-only vault stand-in, which signs
//! as the vault PDA and ignores proposals, thresholds and time locks.  This
//! module replaces it with the real thing so the M14 genesis can be rehearsed
//! locally: `multisig_create_v2` for G (3 members, threshold 2, time lock
//! 172,800 s) and P (3 members, threshold 2, no time lock),
//! `vault_transaction_create` carrying one of our instructions, `proposal_create`,
//! `proposal_approve` x2, a clock warp past the time lock, and
//! `vault_transaction_execute`.
//!
//! # Where the encodings come from
//!
//! Account orders, mutability and signer flags are cross-checked against the
//! Squads IDL read from the devnet on-chain IDL account
//! (`solana/fixtures/devnet/squads-devnet-idl.json`, version 2.1.0; see
//! [`assert_matches_idl`]).  Argument layouts are Borsh, written by hand from the
//! IDL types.  Anchor instruction discriminators are
//! `sha256("global:<snake_case name>")[..8]`; this old-format IDL carries none,
//! so they are derived ([`instruction_discriminator`]) and a wrong one would
//! surface as `InstructionFallbackNotFound` on the first transaction.  The
//! `TransactionMessage` bytes inside `vault_transaction_create` are Squads' own
//! "small vec" format, not a Solana wire message: three `u8` counters, then
//! `u8`-prefixed key and instruction vectors (an instruction's data length is a
//! `u16`), then a `u8`-prefixed (empty) address-table-lookup vector.
//!
//! # Solana note: why an extra signer is the interesting case
//!
//! A Squads vault is a PDA, so it can only "sign" while the Squads program runs
//! an approved vault transaction (closest ICP analogue: a delayed SNS proposal
//! execution; closest EVM analogue: a Safe with a timelock module).  Our
//! `initialize` additionally needs the sponsor as a KEYPAIR signer, which no PDA
//! can be.  Whether a vault transaction may carry a non-vault signer (signing the
//! outer `vault_transaction_execute` transaction and forwarded by the CPI) is a
//! property of the deployed Squads binary, so [`run_vault_transaction`] records
//! exactly what happens instead of assuming it.

use std::{collections::BTreeMap, fs, str::FromStr};

use anchor_lang::AccountDeserialize;
use musical_chairs_program::{
    accounts,
    controls::{InitializeArgs, SQUADS_PROGRAM_ID},
    instruction,
    provider::{self, orao},
    state, ID,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_sdk::{
    clock::Clock,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::{
    devnet,
    runtime::{anchor_ix, fixtures, FinancialRuntime, Observation},
};

pub const CU: u32 = 1_400_000;
/// Squads `Permission::Initiate | Vote | Execute`.
pub const PERMISSION_ALL: u8 = 7;
/// Seconds the G profile's time lock is (`GOVERNANCE_DELAY` in the program).
pub const G_TIME_LOCK: u32 = 172_800;
pub const P_TIME_LOCK: u32 = 0;
pub const THRESHOLD: u16 = 2;
/// The packet's devnet ORAO treasury (also the ORAO network authority).
pub const DEVNET_ORAO_TREASURY: &str = "9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR";
/// The packet's registered-client address for this program (entry 3c).
pub const PACKET_ORAO_CLIENT: &str = "7b1ebeG8qgEtCB3ufPHcZLiUeFJ6cYMS9BeiTnSBJ6HT";
/// The packet's devnet ORAO NetworkState PDA.
pub const PACKET_ORAO_NETWORK_STATE: &str = "3e483QYyYQqu8eesT6ZjfuBiWF9PN5JXKeB8TRmn3hUr";
/// The packet's Squads ProgramConfig PDA.
pub const PACKET_PROGRAM_CONFIG: &str = "BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr";
/// The clock a real-Squads world starts at (arbitrary, deliberately not zero).
pub const START_TIME: i64 = 1_800_000_000;

const SEED_PREFIX: &[u8] = b"multisig";

// ---------------------------------------------------------------------------
// Addresses and discriminators
// ---------------------------------------------------------------------------

/// `["multisig", "program_config"]` under the Squads program.
pub fn program_config_address() -> Pubkey {
    Pubkey::find_program_address(&[SEED_PREFIX, b"program_config"], &SQUADS_PROGRAM_ID).0
}

/// `["multisig", multisig, "transaction", index]`: the `VaultTransaction`.
pub fn transaction_address(multisig: &Pubkey, index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            multisig.as_ref(),
            b"transaction",
            &index.to_le_bytes(),
        ],
        &SQUADS_PROGRAM_ID,
    )
    .0
}

/// `["multisig", multisig, "transaction", index, "proposal"]`.
pub fn proposal_address(multisig: &Pubkey, index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            multisig.as_ref(),
            b"transaction",
            &index.to_le_bytes(),
            b"proposal",
        ],
        &SQUADS_PROGRAM_ID,
    )
    .0
}

/// `["multisig", transaction, "ephemeral_signer", [index]]`: the PDAs Squads
/// signs for besides the vault.  An external keypair can never be one.
pub fn ephemeral_signer_address(transaction: &Pubkey, index: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            transaction.as_ref(),
            b"ephemeral_signer",
            &[index],
        ],
        &SQUADS_PROGRAM_ID,
    )
    .0
}

/// Anchor's instruction discriminator: `sha256("global:<snake_name>")[..8]`.
pub fn instruction_discriminator(snake_name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{snake_name}").as_bytes());
    hash[..8].try_into().expect("8 bytes")
}

/// `multisigCreateV2` -> `multisig_create_v2`.
pub fn snake_case(camel: &str) -> String {
    let mut out = String::new();
    for (index, character) in camel.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The IDL (cross-check only)
// ---------------------------------------------------------------------------

/// The committed devnet Squads IDL, after re-checking its SHA-256.
pub fn idl() -> Value {
    let path = devnet::fixture_dir().join(devnet::SQUADS_DEVNET_IDL_FILE);
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("Squads IDL {} is required: {error}", path.display()));
    assert_eq!(
        devnet::sha256_hex(&bytes),
        devnet::SQUADS_DEVNET_IDL_SHA256,
        "the committed Squads IDL changed"
    );
    serde_json::from_slice(&bytes).expect("the Squads IDL is JSON")
}

/// `(name, isMut, isSigner)` of every account of the IDL instruction `camel`.
pub fn idl_accounts(idl: &Value, camel: &str) -> Vec<(String, bool, bool)> {
    let instruction = idl["instructions"]
        .as_array()
        .expect("IDL instructions")
        .iter()
        .find(|candidate| candidate["name"].as_str() == Some(camel))
        .unwrap_or_else(|| panic!("IDL has no instruction {camel}"));
    instruction["accounts"]
        .as_array()
        .expect("IDL accounts")
        .iter()
        .map(|account| {
            (
                account["name"].as_str().expect("account name").to_string(),
                account["isMut"].as_bool().unwrap_or(false),
                account["isSigner"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// The numeric code of the IDL error called `name`.
pub fn idl_error_code(idl: &Value, name: &str) -> u32 {
    idl["errors"]
        .as_array()
        .expect("IDL errors")
        .iter()
        .find(|error| error["name"].as_str() == Some(name))
        .and_then(|error| error["code"].as_u64())
        .unwrap_or_else(|| panic!("IDL has no error {name}")) as u32
}

/// Assert that `ix` is a well-formed `camel` instruction according to the IDL:
/// the first `idl accounts` metas have the IDL's mutability and signer flags
/// (extra remaining accounts are allowed), and the data starts with the
/// discriminator derived from the instruction's name.
pub fn assert_matches_idl(idl: &Value, camel: &str, ix: &Instruction) {
    let expected = idl_accounts(idl, camel);
    assert!(
        ix.accounts.len() >= expected.len(),
        "{camel}: {} accounts, the IDL lists {}",
        ix.accounts.len(),
        expected.len()
    );
    for (position, (name, is_mut, is_signer)) in expected.iter().enumerate() {
        let meta = &ix.accounts[position];
        assert_eq!(
            (meta.is_writable, meta.is_signer),
            (*is_mut, *is_signer),
            "{camel}: account #{position} `{name}` flags (writable, signer) differ from the IDL"
        );
    }
    assert_eq!(
        ix.data[..8],
        instruction_discriminator(&snake_case(camel)),
        "{camel}: discriminator"
    );
    assert_eq!(ix.program_id, SQUADS_PROGRAM_ID);
}

// ---------------------------------------------------------------------------
// Instruction builders (Borsh by hand)
// ---------------------------------------------------------------------------

/// `multisig_create_v2` for an autonomous multisig (`config_authority` None,
/// `rent_collector` None, no memo).  `members` are sorted by key first, as
/// Squads stores them.  Returns the instruction and the multisig PDA.
pub fn multisig_create_v2_ix(
    treasury: Pubkey,
    creator: Pubkey,
    create_key: Pubkey,
    members: &[(Pubkey, u8)],
    threshold: u16,
    time_lock: u32,
) -> (Instruction, Pubkey) {
    let multisig = fixtures::squads::multisig_address(&create_key).0;
    let mut sorted = members.to_vec();
    sorted.sort_by_key(|(key, _)| *key);
    let mut data = instruction_discriminator("multisig_create_v2").to_vec();
    data.push(0); // config_authority: None
    data.extend_from_slice(&threshold.to_le_bytes());
    data.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    for (key, mask) in &sorted {
        data.extend_from_slice(key.as_ref());
        data.push(*mask);
    }
    data.extend_from_slice(&time_lock.to_le_bytes());
    data.push(0); // rent_collector: None
    data.push(0); // memo: None
    let instruction = Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(program_config_address(), false),
            AccountMeta::new(treasury, false),
            AccountMeta::new(multisig, false),
            AccountMeta::new_readonly(create_key, true),
            AccountMeta::new(creator, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    };
    (instruction, multisig)
}

/// `vault_transaction_create`; `message` is the Squads `TransactionMessage`.
pub fn vault_transaction_create_ix(
    multisig: Pubkey,
    transaction_index: u64,
    creator: Pubkey,
    rent_payer: Pubkey,
    vault_index: u8,
    ephemeral_signers: u8,
    message: &[u8],
) -> Instruction {
    let mut data = instruction_discriminator("vault_transaction_create").to_vec();
    data.push(vault_index);
    data.push(ephemeral_signers);
    data.extend_from_slice(&(message.len() as u32).to_le_bytes());
    data.extend_from_slice(message);
    data.push(0); // memo: None
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(multisig, false),
            AccountMeta::new(transaction_address(&multisig, transaction_index), false),
            AccountMeta::new_readonly(creator, true),
            AccountMeta::new(rent_payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

/// `proposal_create` (`draft` false: the proposal starts Active).
pub fn proposal_create_ix(
    multisig: Pubkey,
    transaction_index: u64,
    creator: Pubkey,
    rent_payer: Pubkey,
    draft: bool,
) -> Instruction {
    let mut data = instruction_discriminator("proposal_create").to_vec();
    data.extend_from_slice(&transaction_index.to_le_bytes());
    data.push(u8::from(draft));
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(multisig, false),
            AccountMeta::new(proposal_address(&multisig, transaction_index), false),
            AccountMeta::new_readonly(creator, true),
            AccountMeta::new(rent_payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

/// `proposal_approve` (no memo).
pub fn proposal_approve_ix(
    multisig: Pubkey,
    transaction_index: u64,
    member: Pubkey,
) -> Instruction {
    let mut data = instruction_discriminator("proposal_approve").to_vec();
    data.push(0); // memo: None
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(multisig, false),
            AccountMeta::new(member, true),
            AccountMeta::new(proposal_address(&multisig, transaction_index), false),
        ],
        data,
    }
}

/// `vault_transaction_execute`, followed by the transaction message's accounts
/// in message order as remaining accounts.
pub fn vault_transaction_execute_ix(
    multisig: Pubkey,
    transaction_index: u64,
    member: Pubkey,
    remaining: Vec<AccountMeta>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(multisig, false),
        AccountMeta::new(proposal_address(&multisig, transaction_index), false),
        AccountMeta::new_readonly(transaction_address(&multisig, transaction_index), false),
        AccountMeta::new_readonly(member, true),
    ];
    accounts.extend(remaining);
    Instruction {
        program_id: SQUADS_PROGRAM_ID,
        accounts,
        data: instruction_discriminator("vault_transaction_execute").to_vec(),
    }
}

// ---------------------------------------------------------------------------
// The Squads TransactionMessage
// ---------------------------------------------------------------------------

/// One inner instruction compiled into the Squads `TransactionMessage` format.
#[derive(Clone, Debug)]
pub struct CompiledVaultMessage {
    /// The Borsh bytes passed as `transaction_message`.
    pub bytes: Vec<u8>,
    /// `(key, is_signer, is_writable)` in message order: signers (writable
    /// first), then non-signers (writable first).
    pub keys: Vec<(Pubkey, bool, bool)>,
}

fn touch(
    order: &mut Vec<Pubkey>,
    flags: &mut BTreeMap<Pubkey, (bool, bool)>,
    key: Pubkey,
    signer: bool,
    writable: bool,
) {
    match flags.get_mut(&key) {
        Some(existing) => {
            existing.0 |= signer;
            existing.1 |= writable;
        }
        None => {
            order.push(key);
            flags.insert(key, (signer, writable));
        }
    }
}

/// Compile `inner` for execution by the vault.  Every account the inner
/// instruction marks as a signer stays a signer in the message: the vault (which
/// Squads signs for) and whoever else the instruction needs (the sponsor
/// keypair, or a Squads ephemeral signer PDA in the ephemeral-signer
/// experiment).
pub fn compile_vault_message(inner: &Instruction) -> CompiledVaultMessage {
    let mut order = Vec::new();
    let mut flags = BTreeMap::new();
    for meta in &inner.accounts {
        touch(
            &mut order,
            &mut flags,
            meta.pubkey,
            meta.is_signer,
            meta.is_writable,
        );
    }
    touch(&mut order, &mut flags, inner.program_id, false, false);

    let mut keys = Vec::with_capacity(order.len());
    for (want_signer, want_writable) in [(true, true), (true, false), (false, true), (false, false)]
    {
        for key in &order {
            let (signer, writable) = flags[key];
            if signer == want_signer && writable == want_writable {
                keys.push((*key, signer, writable));
            }
        }
    }
    let num_signers = keys.iter().filter(|entry| entry.1).count() as u8;
    let num_writable_signers = keys.iter().filter(|entry| entry.1 && entry.2).count() as u8;
    let num_writable_non_signers = keys.iter().filter(|entry| !entry.1 && entry.2).count() as u8;
    let index_of = |key: &Pubkey| -> u8 {
        keys.iter()
            .position(|(candidate, _, _)| candidate == key)
            .expect("compiled key") as u8
    };

    let mut bytes = vec![num_signers, num_writable_signers, num_writable_non_signers];
    bytes.push(keys.len() as u8);
    for (key, _, _) in &keys {
        bytes.extend_from_slice(key.as_ref());
    }
    bytes.push(1); // one instruction
    bytes.push(index_of(&inner.program_id));
    bytes.push(inner.accounts.len() as u8);
    for meta in &inner.accounts {
        bytes.push(index_of(&meta.pubkey));
    }
    bytes.extend_from_slice(&(inner.data.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&inner.data);
    bytes.push(0); // no address table lookups
    CompiledVaultMessage { bytes, keys }
}

impl CompiledVaultMessage {
    /// The accounts of `vault_transaction_execute` after its four fixed ones:
    /// the message's keys in order.  The vault is never a signer of the outer
    /// transaction (Squads signs for it); every other message signer is, so its
    /// keypair has to sign the outer transaction.  `ephemeral` are the PDAs
    /// Squads signs for as well.
    pub fn remaining_accounts(&self, vault: &Pubkey, ephemeral: &[Pubkey]) -> Vec<AccountMeta> {
        self.keys
            .iter()
            .map(|(key, signer, writable)| AccountMeta {
                pubkey: *key,
                is_signer: *signer && key != vault && !ephemeral.contains(key),
                is_writable: *writable,
            })
            .collect()
    }

    /// Signer keys of the message that are neither the vault nor an ephemeral
    /// signer: the keypairs that must sign the outer execute transaction.
    pub fn outer_signer_keys(&self, vault: &Pubkey, ephemeral: &[Pubkey]) -> Vec<Pubkey> {
        self.keys
            .iter()
            .filter(|(key, signer, _)| *signer && key != vault && !ephemeral.contains(key))
            .map(|(key, _, _)| *key)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Reading Squads accounts
// ---------------------------------------------------------------------------

/// A decoded real `Multisig` account (layout of squads-protocol/v4
/// `state/multisig.rs`, the same one `controls::validate_profile` reads).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultisigView {
    pub create_key: Pubkey,
    pub config_authority: Pubkey,
    pub threshold: u16,
    pub time_lock: u32,
    pub transaction_index: u64,
    pub stale_transaction_index: u64,
    pub rent_collector: Option<Pubkey>,
    pub bump: u8,
    pub members: Vec<(Pubkey, u8)>,
    /// Bytes after the member list (allocation padding).
    pub trailing_bytes: usize,
}

fn take<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> &'a [u8] {
    let slice = &data[*cursor..*cursor + len];
    *cursor += len;
    slice
}

fn take_key(data: &[u8], cursor: &mut usize) -> Pubkey {
    Pubkey::new_from_array(take(data, cursor, 32).try_into().expect("32 bytes"))
}

pub fn parse_multisig(data: &[u8]) -> MultisigView {
    assert_eq!(
        data[..8],
        musical_chairs_program::controls::SQUADS_MULTISIG_DISCRIMINATOR,
        "Multisig discriminator"
    );
    let mut cursor = 8usize;
    let create_key = take_key(data, &mut cursor);
    let config_authority = take_key(data, &mut cursor);
    let threshold = u16::from_le_bytes(take(data, &mut cursor, 2).try_into().unwrap());
    let time_lock = u32::from_le_bytes(take(data, &mut cursor, 4).try_into().unwrap());
    let transaction_index = u64::from_le_bytes(take(data, &mut cursor, 8).try_into().unwrap());
    let stale_transaction_index =
        u64::from_le_bytes(take(data, &mut cursor, 8).try_into().unwrap());
    let rent_collector = match take(data, &mut cursor, 1)[0] {
        0 => None,
        1 => Some(take_key(data, &mut cursor)),
        other => panic!("rent_collector option tag {other}"),
    };
    let bump = take(data, &mut cursor, 1)[0];
    let member_count = u32::from_le_bytes(take(data, &mut cursor, 4).try_into().unwrap()) as usize;
    let mut members = Vec::with_capacity(member_count);
    for _ in 0..member_count {
        let member = take_key(data, &mut cursor);
        let mask = take(data, &mut cursor, 1)[0];
        members.push((member, mask));
    }
    MultisigView {
        create_key,
        config_authority,
        threshold,
        time_lock,
        transaction_index,
        stale_transaction_index,
        rent_collector,
        bump,
        members,
        trailing_bytes: data.len() - cursor,
    }
}

/// The Squads `ProposalStatus` variant name of a `Proposal` account
/// (`discriminator(8) | multisig(32) | transaction_index(8) | status`).
pub fn proposal_status_name(data: &[u8]) -> &'static str {
    match data[48] {
        0 => "Draft",
        1 => "Active",
        2 => "Rejected",
        3 => "Approved",
        4 => "Executing",
        5 => "Executed",
        6 => "Cancelled",
        other => panic!("unknown ProposalStatus variant {other}"),
    }
}

/// `(multisig_creation_fee, treasury)` of the installed `ProgramConfig`
/// (`discriminator(8) | authority(32) | multisig_creation_fee u64 | treasury(32)`).
pub fn program_config_fee_and_treasury(rt: &FinancialRuntime) -> (u64, Pubkey) {
    let data = rt.account(program_config_address()).data;
    let fee = u64::from_le_bytes(data[40..48].try_into().unwrap());
    let treasury = Pubkey::new_from_array(data[48..80].try_into().unwrap());
    (fee, treasury)
}

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------

fn clone_keypair(keypair: &Keypair) -> Keypair {
    Keypair::new_from_array(*keypair.secret_bytes())
}

/// Execute `instructions` with `payer` as the fee payer (instead of the
/// runtime's harness payer), restoring the payer afterwards.  `extra` signs in
/// addition; every key in it must be required by the message.
pub fn send_as(
    rt: &mut FinancialRuntime,
    payer: &Keypair,
    label: impl Into<String>,
    instructions: Vec<Instruction>,
    extra: &[&Keypair],
) -> Observation {
    let original = std::mem::replace(&mut rt.payer, clone_keypair(payer));
    let observation = rt.execute(label, instructions, extra, CU);
    rt.payer = original;
    observation
}

/// Move the clock forward by `seconds` (and one slot).
pub fn advance(rt: &mut FinancialRuntime, seconds: i64) {
    let clock: Clock = rt.svm.get_sysvar();
    rt.set_clock(clock.unix_timestamp + seconds, clock.slot + 1);
}

/// The current unix time of the runtime's clock.
pub fn now(rt: &FinancialRuntime) -> i64 {
    rt.svm.get_sysvar::<Clock>().unix_timestamp
}

/// Create the multisig for `create_key` with real `multisig_create_v2`; the
/// harness payer pays rent and signs as `creator`.
pub fn create_multisig(
    rt: &mut FinancialRuntime,
    label: impl Into<String>,
    create_key: &Keypair,
    members: &[(Pubkey, u8)],
    threshold: u16,
    time_lock: u32,
) -> (Observation, Pubkey) {
    let (_, treasury) = program_config_fee_and_treasury(rt);
    let (ix, multisig) = multisig_create_v2_ix(
        treasury,
        rt.payer.pubkey(),
        create_key.pubkey(),
        members,
        threshold,
        time_lock,
    );
    let observation = rt.execute(label, vec![ix], &[create_key], CU);
    (observation, multisig)
}

// ---------------------------------------------------------------------------
// One vault transaction, end to end
// ---------------------------------------------------------------------------

/// Everything [`run_vault_transaction`] needs.
pub struct Plan<'a> {
    /// Label prefix of every transaction.
    pub tag: &'a str,
    pub multisig: Pubkey,
    pub vault_index: u8,
    /// The instruction the vault executes.
    pub inner: Instruction,
    /// A member with Initiate; also pays the fees of create and proposal.
    pub proposer: &'a Keypair,
    /// Members that vote, in order.
    pub approvers: Vec<&'a Keypair>,
    /// A member with Execute; pays the execute transaction's fee.
    pub executor: &'a Keypair,
    /// Keypairs that sign the OUTER execute transaction in addition (the
    /// sponsor, for `initialize`).
    pub outer_signers: Vec<&'a Keypair>,
    /// Ephemeral signer PDAs the transaction declares (count) and uses.
    pub ephemeral_signers: u8,
    /// Seconds to wait after approval (the multisig's time lock).
    pub time_lock: u32,
    /// Also try to execute BEFORE the time lock and record the refusal.
    pub probe_early_execute: bool,
}

/// What happened, stage by stage.  A stage that was not reached is `None`.
#[derive(Clone, Debug, Default)]
pub struct VaultTxRun {
    pub transaction_index: u64,
    pub vault: Pubkey,
    pub create: Option<Observation>,
    pub proposal: Option<Observation>,
    pub approvals: Vec<Observation>,
    pub early_execute: Option<Observation>,
    pub execute: Option<Observation>,
}

impl VaultTxRun {
    /// The first stage that failed (early execution excluded: it is expected to).
    pub fn first_failure(&self) -> Option<(&'static str, &Observation)> {
        if let Some(create) = self.create.as_ref().filter(|o| !o.success) {
            return Some(("vault_transaction_create", create));
        }
        if let Some(proposal) = self.proposal.as_ref().filter(|o| !o.success) {
            return Some(("proposal_create", proposal));
        }
        if let Some(approval) = self.approvals.iter().find(|o| !o.success) {
            return Some(("proposal_approve", approval));
        }
        if let Some(execute) = self.execute.as_ref().filter(|o| !o.success) {
            return Some(("vault_transaction_execute", execute));
        }
        None
    }

    /// Every transaction in execution order.
    pub fn observations(&self) -> Vec<&Observation> {
        self.create
            .iter()
            .chain(self.proposal.iter())
            .chain(self.approvals.iter())
            .chain(self.early_execute.iter())
            .chain(self.execute.iter())
            .collect()
    }

    /// `true` when every stage ran and succeeded.
    pub fn succeeded(&self) -> bool {
        self.first_failure().is_none() && self.execute.as_ref().is_some_and(|o| o.success)
    }
}

/// Run one vault transaction through the real Squads program: create, propose,
/// approve, (probe an early execution), wait out the time lock, execute.  Stops
/// at the first failing stage and returns what was observed; a non-zero
/// `plan.time_lock` advances the runtime clock.
pub fn run_vault_transaction(rt: &mut FinancialRuntime, plan: &Plan<'_>) -> VaultTxRun {
    let view = parse_multisig(&rt.account(plan.multisig).data);
    let index = view.transaction_index + 1;
    let (vault, _) = fixtures::squads::vault_address(&plan.multisig, plan.vault_index);
    let transaction = transaction_address(&plan.multisig, index);
    let ephemeral: Vec<Pubkey> = (0..plan.ephemeral_signers)
        .map(|signer| ephemeral_signer_address(&transaction, signer))
        .collect();
    let compiled = compile_vault_message(&plan.inner);
    let mut run = VaultTxRun {
        transaction_index: index,
        vault,
        ..VaultTxRun::default()
    };

    let proposer = plan.proposer.pubkey();
    let create = send_as(
        rt,
        plan.proposer,
        format!("{}.vault_transaction_create", plan.tag),
        vec![vault_transaction_create_ix(
            plan.multisig,
            index,
            proposer,
            proposer,
            plan.vault_index,
            plan.ephemeral_signers,
            &compiled.bytes,
        )],
        &[],
    );
    let created = create.success;
    run.create = Some(create);
    if !created {
        return run;
    }

    let proposal = send_as(
        rt,
        plan.proposer,
        format!("{}.proposal_create", plan.tag),
        vec![proposal_create_ix(
            plan.multisig,
            index,
            proposer,
            proposer,
            false,
        )],
        &[],
    );
    let proposed = proposal.success;
    run.proposal = Some(proposal);
    if !proposed {
        return run;
    }

    for (position, approver) in plan.approvers.iter().enumerate() {
        let approval = send_as(
            rt,
            approver,
            format!("{}.proposal_approve.{}", plan.tag, position + 1),
            vec![proposal_approve_ix(plan.multisig, index, approver.pubkey())],
            &[],
        );
        let approved = approval.success;
        run.approvals.push(approval);
        if !approved {
            return run;
        }
    }

    let execute_ix = |executor: &Keypair| {
        vault_transaction_execute_ix(
            plan.multisig,
            index,
            executor.pubkey(),
            compiled.remaining_accounts(&vault, &ephemeral),
        )
    };
    let outer: Vec<&Keypair> = plan.outer_signers.clone();

    if plan.probe_early_execute {
        let early = send_as(
            rt,
            plan.executor,
            format!("{}.vault_transaction_execute.before_time_lock", plan.tag),
            vec![execute_ix(plan.executor)],
            &outer,
        );
        run.early_execute = Some(early);
    }

    if plan.time_lock > 0 {
        // One second past the lock: Squads requires `now - approved_at >= time_lock`.
        advance(rt, i64::from(plan.time_lock) + 1);
    }

    let execute = send_as(
        rt,
        plan.executor,
        format!("{}.vault_transaction_execute", plan.tag),
        vec![execute_ix(plan.executor)],
        &outer,
    );
    run.execute = Some(execute);
    run
}

// ---------------------------------------------------------------------------
// A world with real Squads multisigs
// ---------------------------------------------------------------------------

/// Deterministic member keypair `ordinal` (0..3) of a multisig: `tag` keeps G,
/// P and any rogue multisig apart.
pub fn member_keypair(tag: u8, ordinal: u8) -> Keypair {
    let mut seed = [tag; 32];
    seed[31] = ordinal;
    Keypair::new_from_array(seed)
}

/// A runtime with the real Squads program, the G and P member keypairs and,
/// after [`RealSquadsWorld::create_g_and_p`], the real G and P multisigs.
pub struct RealSquadsWorld {
    pub rt: FinancialRuntime,
    pub g_members: Vec<Keypair>,
    pub p_members: Vec<Keypair>,
}

impl RealSquadsWorld {
    /// Real Squads only (no ORAO); the clock starts at [`START_TIME`].
    pub fn new() -> Self {
        Self::from_runtime(FinancialRuntime::new_uninitialized_real_squads())
    }

    /// Real Squads plus the ORAO executable of `profile` loaded at its id.
    pub fn with_provider(profile: devnet::OraoProfile) -> Self {
        Self::from_runtime(FinancialRuntime::new_uninitialized_real_squads_with_provider(profile))
    }

    fn from_runtime(mut rt: FinancialRuntime) -> Self {
        rt.set_clock(START_TIME, 100);
        let g_members: Vec<Keypair> = (0..3).map(|i| member_keypair(0x61, i)).collect();
        let p_members: Vec<Keypair> = (0..3).map(|i| member_keypair(0x71, i)).collect();
        for member in g_members.iter().chain(p_members.iter()) {
            // Members pay the fees of their own transactions.
            rt.credit_lamports(member.pubkey(), 10_000_000_000);
        }
        Self {
            rt,
            g_members,
            p_members,
        }
    }

    /// `(key, ALL)` of every member.
    pub fn member_specs(members: &[Keypair]) -> Vec<(Pubkey, u8)> {
        members
            .iter()
            .map(|member| (member.pubkey(), PERMISSION_ALL))
            .collect()
    }

    /// Create G (3 members, threshold 2, time lock 172,800) and P (3 members,
    /// threshold 2, time lock 0) with real `multisig_create_v2`.
    pub fn create_g_and_p(&mut self) -> (Observation, Observation) {
        let g_specs = Self::member_specs(&self.g_members);
        let (g, g_multisig) = create_multisig(
            &mut self.rt,
            "squads.multisig_create_v2.G",
            &crate::runtime::real_g_create_keypair(),
            &g_specs,
            THRESHOLD,
            G_TIME_LOCK,
        );
        assert_eq!(g_multisig, self.rt.g_multisig(), "G multisig PDA");
        let p_specs = Self::member_specs(&self.p_members);
        let (p, p_multisig) = create_multisig(
            &mut self.rt,
            "squads.multisig_create_v2.P",
            &crate::runtime::real_p_create_keypair(),
            &p_specs,
            THRESHOLD,
            P_TIME_LOCK,
        );
        assert_eq!(p_multisig, self.rt.p_multisig(), "P multisig PDA");
        (g, p)
    }

    /// The program's `initialize` arguments for the devnet tuple of the M14
    /// packet (section 3c): ORAO program, NetworkState PDA, the real devnet
    /// treasury, the derived client, fee 300,000 lamports, headroom 2,500 bps.
    pub fn devnet_initialize_args(&self) -> InitializeArgs {
        InitializeArgs {
            sponsor: self.rt.sponsor.pubkey(),
            management_recipient: self.rt.management.pubkey(),
            cluster_domain: crate::controls::DEVNET_DOMAIN,
            provider_request_fee_lamports: 300_000,
            provider_fee_headroom_bps: 2_500,
            provider_program: orao::id(),
            provider_network_state: orao::state::network_state::NetworkState::find_address(
                &orao::id(),
            )
            .0,
            provider_treasury: Pubkey::from_str(DEVNET_ORAO_TREASURY).expect("treasury"),
            provider_client: provider::client_address().0,
            public_series_a_enabled: false,
        }
    }

    /// The inner `initialize` instruction against this world's G and P.
    pub fn initialize_instruction(&self, args: InitializeArgs) -> Instruction {
        crate::controls::initialize_ix(&self.rt, args)
    }

    /// Run `inner` as G's vault transaction (member 0 proposes and executes,
    /// members 0 and 1 approve), with `outer_signers` also signing the execute
    /// transaction.
    pub fn run_g(
        &mut self,
        tag: &str,
        inner: Instruction,
        outer_signers: &[&Keypair],
    ) -> VaultTxRun {
        let multisig = self.rt.g_multisig();
        run_vault_transaction(
            &mut self.rt,
            &Plan {
                tag,
                multisig,
                vault_index: 0,
                inner,
                proposer: &self.g_members[0],
                approvers: vec![&self.g_members[0], &self.g_members[1]],
                executor: &self.g_members[0],
                outer_signers: outer_signers.to_vec(),
                ephemeral_signers: 0,
                time_lock: G_TIME_LOCK,
                probe_early_execute: true,
            },
        )
    }
}

/// A second multisig created next to G and P (a "rogue" or deliberately
/// wrong-profile one).
pub struct ExtraMultisig {
    pub create_key: Keypair,
    pub members: Vec<Keypair>,
    pub multisig: Pubkey,
    pub vault: Pubkey,
    pub threshold: u16,
    pub time_lock: u32,
}

impl RealSquadsWorld {
    /// Fund the G vault (a system account; the ORAO `register` payer and, in the
    /// G-vault-as-sponsor variant, the `initialize` payer).
    pub fn fund_g_vault(&mut self, lamports: u64) {
        let vault = self.rt.g_vault();
        self.rt.credit_lamports(vault, lamports);
    }

    /// [`RealSquadsWorld::devnet_initialize_args`] with the G vault itself as
    /// the sponsor: the one signer Squads always provides.
    pub fn g_vault_sponsor_args(&self) -> InitializeArgs {
        let mut args = self.devnet_initialize_args();
        args.sponsor = self.rt.g_vault();
        args
    }

    /// The `initialize` instruction for `args` whose `sponsor` account is `sponsor`
    /// (the shared builder names `rt.sponsor`).
    pub fn initialize_instruction_with_sponsor(
        &self,
        args: InitializeArgs,
        sponsor: Pubkey,
    ) -> Instruction {
        let mut ix = self.initialize_instruction(args);
        replace_key(&mut ix, self.rt.sponsor.pubkey(), sponsor);
        ix
    }

    /// Run `inner` as P's vault transaction (members 0 and 1 approve; P has no
    /// time lock, so execution follows immediately).
    pub fn run_p(&mut self, tag: &str, inner: Instruction) -> VaultTxRun {
        let multisig = self.rt.p_multisig();
        run_vault_transaction(
            &mut self.rt,
            &Plan {
                tag,
                multisig,
                vault_index: 0,
                inner,
                proposer: &self.p_members[0],
                approvers: vec![&self.p_members[0], &self.p_members[1]],
                executor: &self.p_members[0],
                outer_signers: Vec::new(),
                ephemeral_signers: 0,
                time_lock: P_TIME_LOCK,
                probe_early_execute: false,
            },
        )
    }

    /// Create one more real multisig with `member_count` funded members.
    pub fn create_extra_multisig(
        &mut self,
        label: &str,
        member_tag: u8,
        create_seed: [u8; 32],
        member_count: u8,
        threshold: u16,
        time_lock: u32,
    ) -> ExtraMultisig {
        let create_key = Keypair::new_from_array(create_seed);
        let members: Vec<Keypair> = (0..member_count)
            .map(|ordinal| member_keypair(member_tag, ordinal))
            .collect();
        for member in &members {
            self.rt.credit_lamports(member.pubkey(), 10_000_000_000);
        }
        let (observation, multisig) = create_multisig(
            &mut self.rt,
            label,
            &create_key,
            &Self::member_specs(&members),
            threshold,
            time_lock,
        );
        assert!(
            observation.success,
            "{label}: multisig_create_v2 failed: {observation:?}"
        );
        let vault = fixtures::squads::vault_address(&multisig, 0).0;
        ExtraMultisig {
            create_key,
            members,
            multisig,
            vault,
            threshold,
            time_lock,
        }
    }
}

impl ExtraMultisig {
    /// Run `inner` as this multisig's vault-0 transaction: member 0 proposes and
    /// executes, the first `threshold` members approve, and the clock waits out
    /// this multisig's own time lock.
    pub fn run(&self, rt: &mut FinancialRuntime, tag: &str, inner: Instruction) -> VaultTxRun {
        run_vault_transaction(
            rt,
            &Plan {
                tag,
                multisig: self.multisig,
                vault_index: 0,
                inner,
                proposer: &self.members[0],
                approvers: self
                    .members
                    .iter()
                    .take(usize::from(self.threshold))
                    .collect(),
                executor: &self.members[0],
                outer_signers: Vec::new(),
                ephemeral_signers: 0,
                time_lock: self.time_lock,
                probe_early_execute: false,
            },
        )
    }
}

/// Replace every occurrence of `from` among the instruction's accounts by `to`.
pub fn replace_key(ix: &mut Instruction, from: Pubkey, to: Pubkey) {
    for meta in &mut ix.accounts {
        if meta.pubkey == from {
            meta.pubkey = to;
        }
    }
}

/// The post-`initialize` assertions of the stand-in genesis row (every created
/// account at its declared size and rent, bound to the G and P vaults and the
/// arguments; genesis counters; ConfigVersion#1; conservation), applied to a
/// genesis that really ran through the real Squads program.  Call it right
/// after the run: `ConfigVersion#1` is stamped with the clock of the execute
/// transaction, which is the runtime's current clock until something moves it.
pub fn assert_initialized(rt: &FinancialRuntime, args: &InitializeArgs) {
    let executed_at = u64::try_from(now(rt)).expect("positive clock");
    assert!(
        executed_at > 0,
        "a real-Squads genesis cannot run at clock 0"
    );
    crate::controls::assert_genesis_accounts_at(rt, args, executed_at);
    let pool: state::Pool = rt.deserialize(rt.pool_address());
    crate::controls::assert_genesis_counters(&pool, rt.rent(state::GameVault::SPACE));
    rt.conservation_check();
}

/// `set_pause` for the guardian (P) vault.
pub fn set_pause_instruction(rt: &FinancialRuntime, paused: bool) -> Instruction {
    anchor_ix(
        accounts::Pause {
            admission: rt.admission_address(),
            guardian: rt.p_vault(),
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::SetPause { paused },
    )
}

impl Default for RealSquadsWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ORAO set-up helpers (the devnet binary, synthetic authority)
// ---------------------------------------------------------------------------

/// Install the real devnet treasury and NetworkState SNAPSHOT accounts'
/// treasury as a plain system account (the NetworkState itself is NOT
/// installed: the loaded ORAO binary creates its own, see [`orao_initialize`]).
pub fn install_devnet_orao_treasury(rt: &mut FinancialRuntime) -> Pubkey {
    let accounts = devnet::load_accounts(devnet::ORAO_DEVNET_ACCOUNTS_FILE);
    let treasury = accounts.get("treasury").expect("treasury snapshot");
    rt.svm
        .set_account(treasury.address, treasury.to_account())
        .expect("ORAO treasury snapshot");
    treasury.address
}

/// ORAO `initialize` then `configure` by the harness payer: request fee
/// 300,000, callback deadline 9,000 slots, the real devnet treasury address and
/// ONE synthetic fulfil authority (`crate::close::fulfill_authority`).
pub fn orao_initialize(rt: &mut FinancialRuntime, treasury: Pubkey) -> Vec<Observation> {
    use crate::close::{foreign_ix, fulfill_authority};
    let payer = rt.payer.pubkey();
    let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    let initialize = rt.execute(
        "squads.orao.initialize",
        vec![foreign_ix(
            orao::id(),
            orao::accounts::Initialize {
                payer,
                network_state,
                system_program: system_program::ID,
            },
            orao::instruction::Initialize {
                params: orao::InitializeParams::new(300_000),
            },
        )],
        &[],
        CU,
    );
    let mut config =
        orao::state::network_state::NetworkConfiguration::new(payer, treasury, 300_000);
    config.callback_deadline = 9_000;
    config.fulfill_authorities = vec![fulfill_authority().pubkey()];
    let configure = rt.execute(
        "squads.orao.configure",
        vec![foreign_ix(
            orao::id(),
            orao::accounts::Configure {
                payer,
                network_state,
                system_program: system_program::ID,
            },
            orao::instruction::Configure {
                params: orao::ConfigureParams::new(config),
            },
        )],
        &[],
        CU,
    );
    vec![initialize, configure]
}

/// The ORAO `register` instruction with the G vault as payer (the
/// `ProgramData` upgrade authority), registering this program's `RngAuthority`.
pub fn orao_register_instruction(rt: &FinancialRuntime) -> Instruction {
    use crate::close::foreign_ix;
    let (rng_authority, rng_bump) = provider::rng_authority_address();
    foreign_ix(
        orao::id(),
        orao::accounts::Register {
            payer: rt.g_vault(),
            program: ID,
            program_data: rt.program_data_address(),
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

/// Decode the registered ORAO `Client`.
pub fn orao_client(rt: &FinancialRuntime) -> orao::state::client::Client {
    rt.deserialize(provider::client_address().0)
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// A JSON summary of one observation, for the finding records.
pub fn summarize(observation: &Observation) -> Value {
    json!({
        "label": observation.label,
        "success": observation.success,
        "error": observation.error,
        "compute_units": observation.compute_units,
        "packet_bytes": observation.packet_bytes,
        "fits_legacy_packet": observation.fits_legacy_packet,
        "message_account_count": observation.message_account_count,
        "writable_account_count": observation.writable_account_count,
        "rollback_except_fee": observation.rollback_except_fee,
        "logs": observation.logs,
    })
}

/// JSON summary of a whole [`VaultTxRun`].
pub fn summarize_run(run: &VaultTxRun) -> Value {
    json!({
        "transaction_index": run.transaction_index,
        "vault": run.vault.to_string(),
        "succeeded": run.succeeded(),
        "first_failure": run.first_failure().map(|(stage, o)| json!({"stage": stage, "observation": summarize(o)})),
        "transactions": run.observations().into_iter().map(summarize).collect::<Vec<_>>(),
    })
}

/// Print `FINDING <name>: <json>` to stderr (visible with `--nocapture`) and,
/// when `MC_DEVNET_COMPAT_EVIDENCE_DIR` names a directory, write
/// `<dir>/<name>.json`.
pub fn record_finding(name: &str, value: &Value) {
    let text = serde_json::to_string_pretty(value).expect("finding JSON");
    eprintln!("FINDING {name}: {text}");
    if let Some(dir) = std::env::var_os("MC_DEVNET_COMPAT_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        fs::create_dir_all(&dir).expect("evidence directory");
        fs::write(dir.join(format!("{name}.json")), text).expect("evidence file");
    }
}

/// Parse an Anchor account of type `T` from raw account data.
pub fn anchor_account<T: AccountDeserialize>(data: &[u8]) -> T {
    let mut slice = data;
    T::try_deserialize(&mut slice).expect("Anchor account deserializes")
}
