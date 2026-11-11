//! LiteSVM-backed host adapter for the runtime probe.
//!
//! This module executes the compiled probe program in an in-memory validator.
//! Keys are deterministic synthetic fixtures and never represent production
//! wallets or signing authority.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anchor_lang::{AccountDeserialize, AccountSerialize};
use litesvm::LiteSVM;
use musical_chairs_runtime_probe_program::{
    ProbeBacker, ProbeState, BACKER_SEED, PROBE_BACKER_SPACE, PROBE_STATE_SPACE, STATE_SEED,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_sdk::{
    account::Account,
    clock::Clock,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

const AIRDROP_LAMPORTS: u64 = 10_000_000_000;

/// A compact account delta suitable for JSON evidence.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountChange {
    pub pubkey: String,
    pub owner: String,
    pub lamport_delta: i128,
    pub data_sha256: String,
}

/// Observable result from one local, signed LiteSVM transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Observation {
    pub label: String,
    pub success: bool,
    pub error: Option<String>,
    pub compute_limit: u32,
    pub expected_fee_lamports: u64,
    pub fits_legacy_packet: bool,
    pub compute_units: Option<u64>,
    pub packet_bytes: usize,
    pub message_account_count: usize,
    pub writable_account_count: usize,
    pub declared_account_data_bytes: usize,
    pub changed_data_bytes: usize,
    pub payer_lamports_before: u64,
    pub payer_lamports_after: u64,
    pub account_changes: Vec<AccountChange>,
    pub logs: Vec<String>,
    pub return_data: Vec<u8>,
    pub rollback_except_fee: bool,
    pub runtime_loaded_account_count: Option<usize>,
    pub unsupported: Option<String>,
}

impl Observation {
    /// Assert the Solana failure behavior expected from an atomic instruction:
    /// all non-payer accounts are unchanged and the payer may only lose the fee.
    pub fn assert_rollback_except_fee(&self) {
        assert!(
            self.rollback_except_fee,
            "transaction was not fee-only rollback: {self:?}"
        );
    }
}

#[derive(Clone)]
struct AccountSnapshot {
    account: Option<Account>,
}

/// Reusable deterministic LiteSVM fixture and transaction adapter.
pub struct RuntimeProbe {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub authority: Keypair,
    provider_loaded: bool,
}

impl Default for RuntimeProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeProbe {
    /// Construct the full fixture with the compiled probe and ORAO callback
    /// fixture loaded into the in-memory validator.
    pub fn new() -> Self {
        Self::build(true)
    }

    /// Construct the negative-case fixture without loading the ORAO program.
    pub fn new_without_provider() -> Self {
        Self::build(false)
    }

    fn build(load_provider: bool) -> Self {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new_from_array([0x11; 32]);
        let authority = Keypair::new_from_array([0x22; 32]);
        svm.airdrop(&payer.pubkey(), AIRDROP_LAMPORTS)
            .expect("synthetic payer airdrop must succeed");
        svm.airdrop(&authority.pubkey(), AIRDROP_LAMPORTS)
            .expect("synthetic authority airdrop must succeed");

        let probe_elf = read_probe_elf();
        svm.add_program(musical_chairs_runtime_probe_program::ID, &probe_elf)
            .expect("probe ELF must load into LiteSVM");

        if load_provider {
            let provider_elf = read_fixture(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../fixtures/orao-callback-mainnet-finalized.so"),
            );
            assert_eq!(
                sha256(&provider_elf),
                "6d180b1e91be7d5c541f75e2413146beb047d9beeb76ac8eb1d7c883200c6b8b",
                "ORAO fixture hash changed"
            );
            svm.add_program(orao_solana_vrf_cb::id(), &provider_elf)
                .expect("ORAO fixture ELF must load into LiteSVM");
        }

        Self {
            svm,
            payer,
            authority,
            provider_loaded: load_provider,
        }
    }

    /// Whether the real ORAO callback fixture is registered in this SVM.
    pub fn provider_loaded(&self) -> bool {
        self.provider_loaded
    }

    /// Set the real Clock sysvar used by subsequent instructions.
    pub fn set_clock(&mut self, unix: i64, slot: u64) {
        self.svm.set_sysvar(&Clock {
            slot,
            epoch_start_timestamp: unix,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: unix,
        });
    }

    /// Insert a canonical, Anchor-serialized ProbeState account.
    pub fn put_state(&mut self, state: ProbeState) {
        let (address, _) = self.state_address();
        let data = serialize_account(&state);
        assert_eq!(
            data.len(),
            PROBE_STATE_SPACE,
            "ProbeState wire size changed"
        );
        self.svm
            .set_account(
                address,
                Account {
                    lamports: self.svm.minimum_balance_for_rent_exemption(data.len()),
                    data,
                    owner: musical_chairs_runtime_probe_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("state fixture account must be accepted");
    }

    /// Read and deserialize the canonical ProbeState account.
    pub fn state(&self) -> ProbeState {
        let (address, _) = self.state_address();
        let account = self
            .svm
            .get_account(&address)
            .expect("ProbeState fixture must exist");
        deserialize_account(&account.data)
    }

    /// Insert a canonical row at the PDA derived from state and little-endian
    /// row index, returning its address for instruction construction.
    pub fn put_row(&mut self, index: u16, headroom: u128, pending: u64) -> Pubkey {
        let (state, _) = self.state_address();
        let (address, bump) = Pubkey::find_program_address(
            &[BACKER_SEED, state.as_ref(), &index.to_le_bytes()],
            &musical_chairs_runtime_probe_program::ID,
        );
        let row = ProbeBacker {
            index,
            headroom,
            pending,
            bump,
        };
        let data = serialize_account(&row);
        assert_eq!(
            data.len(),
            PROBE_BACKER_SPACE,
            "ProbeBacker wire size changed"
        );
        self.svm
            .set_account(
                address,
                Account {
                    lamports: self.svm.minimum_balance_for_rent_exemption(data.len()),
                    data,
                    owner: musical_chairs_runtime_probe_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("row fixture account must be accepted");
        address
    }

    /// Read a canonical row by index.
    pub fn row(&self, index: u16) -> ProbeBacker {
        let (state, _) = self.state_address();
        let (address, _) = Pubkey::find_program_address(
            &[BACKER_SEED, state.as_ref(), &index.to_le_bytes()],
            &musical_chairs_runtime_probe_program::ID,
        );
        let account = self
            .svm
            .get_account(&address)
            .expect("row fixture must exist");
        deserialize_account(&account.data)
    }

    /// Execute one locally signed transaction and return complete observable
    /// metadata. The payer always signs; callers add only explicitly required
    /// extra signers.
    pub fn execute(
        &mut self,
        label: &str,
        instructions: Vec<Instruction>,
        extra_signers: &[&Keypair],
        compute_limit: u32,
    ) -> Observation {
        let mut all_instructions = Vec::with_capacity(instructions.len() + 1);
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_limit,
        ));
        all_instructions.extend(instructions);

        let message = Message::new(&all_instructions, Some(&self.payer.pubkey()));
        let expected_fee_lamports = 5_000
            * (u64::from(message.header.num_required_signatures)
                + all_instructions
                    .iter()
                    .filter(|ix| ix.program_id == solana_sdk::ed25519_program::id())
                    .map(|ix| u64::from(*ix.data.first().expect("Ed25519 fixture header")))
                    .sum::<u64>());
        let message_account_count = message.account_keys.len();
        let writable_account_count = message
            .account_keys
            .iter()
            .enumerate()
            .filter(|(index, _)| message.is_maybe_writable(*index, None))
            .count();
        let keys: Vec<Pubkey> = message.account_keys.clone();
        let snapshots = self.snapshot(&keys);
        let declared_account_data_bytes = snapshots
            .values()
            .filter_map(|snapshot| snapshot.account.as_ref())
            .map(|account| account.data.len())
            .sum();
        let payer_lamports_before = self
            .svm
            .get_account(&self.payer.pubkey())
            .map(|account| account.lamports)
            .unwrap_or_default();
        // A fresh blockhash prevents duplicate signatures when a fixture runs
        // the same instruction sequence more than once.
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let mut transaction = Transaction::new_unsigned(message);
        let mut signers: Vec<&dyn Signer> = Vec::with_capacity(extra_signers.len() + 1);
        signers.push(&self.payer);
        signers.extend(extra_signers.iter().map(|signer| *signer as &dyn Signer));
        transaction
            .try_partial_sign(&signers, blockhash)
            .expect("synthetic fixture signing must succeed");
        let packet_bytes = bincode::serialize(&transaction)
            .map(|bytes| bytes.len())
            .unwrap_or_default();

        let result = self.svm.send_transaction(transaction);
        let (success, error, compute_units, logs, return_data, unsupported) = match result {
            Ok(metadata) => (
                true,
                None,
                Some(metadata.compute_units_consumed),
                metadata.logs,
                metadata.return_data.data,
                Some("LiteSVM does not expose runtime loaded-account count".to_string()),
            ),
            Err(failed) => (
                false,
                Some(format!("{:?}", failed.err)),
                Some(failed.meta.compute_units_consumed),
                failed.meta.logs,
                failed.meta.return_data.data,
                Some("LiteSVM exposes no runtime loaded-account count".to_string()),
            ),
        };

        let after = self.snapshot(&keys);
        let account_changes = account_changes(&snapshots, &after);
        let changed_data_bytes = changed_data_bytes(&snapshots, &after);
        let payer_lamports_after = self
            .svm
            .get_account(&self.payer.pubkey())
            .map(|account| account.lamports)
            .unwrap_or_default();
        let fee_debit_valid = payer_lamports_before.checked_sub(payer_lamports_after)
            == Some(expected_fee_lamports)
            || (compute_units == Some(0) && payer_lamports_before == payer_lamports_after);
        let rollback_except_fee = fee_debit_valid
            && rollback_except_fee(
                &snapshots,
                &after,
                self.payer.pubkey(),
                payer_lamports_before,
                payer_lamports_after,
            );

        let observation = Observation {
            label: label.to_string(),
            success,
            error,
            compute_limit,
            expected_fee_lamports,
            fits_legacy_packet: packet_bytes <= 1232,
            compute_units,
            packet_bytes,
            message_account_count,
            writable_account_count,
            declared_account_data_bytes,
            changed_data_bytes,
            payer_lamports_before,
            payer_lamports_after,
            account_changes,
            logs,
            return_data,
            rollback_except_fee,
            runtime_loaded_account_count: None,
            unsupported,
        };
        if let Some(path) = env::var_os("MC_OBSERVATION_LOG") {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("local observation log");
            serde_json::to_writer(&mut file, &observation).expect("serialize observation");
            writeln!(&mut file).expect("append observation");
        }
        observation
    }

    pub fn state_address(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[STATE_SEED], &musical_chairs_runtime_probe_program::ID)
    }

    fn snapshot(&self, keys: &[Pubkey]) -> BTreeMap<Pubkey, AccountSnapshot> {
        keys.iter()
            .copied()
            .map(|key| {
                (
                    key,
                    AccountSnapshot {
                        account: self.svm.get_account(&key),
                    },
                )
            })
            .collect()
    }
}

fn read_probe_elf() -> Vec<u8> {
    let path = env::var_os("MC_PROBE_SBF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../artifacts/musical_chairs_runtime_probe_program.so")
        });
    let elf = read_fixture(path);
    let receipt: serde_json::Value = serde_json::from_slice(&read_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/build-provenance.json"),
    ))
    .expect("build receipt");
    assert_eq!(
        sha256(&elf),
        receipt["artifact"]["sha256"].as_str().unwrap(),
        "compiled probe hash differs from build receipt"
    );
    let source = read_fixture(Path::new(env!("CARGO_MANIFEST_DIR")).join("../program/src/lib.rs"));
    assert_eq!(
        sha256(&source),
        receipt["source"]["sha256"].as_str().unwrap(),
        "SBF source changed; rebuild and refresh the receipt"
    );
    for dependency in receipt["dependency_sources"].as_array().unwrap() {
        let dependency_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../program/src")
            .join(dependency["path"].as_str().unwrap());
        assert_eq!(
            sha256(&read_fixture(dependency_path)),
            dependency["sha256"].as_str().unwrap(),
            "SBF arithmetic source changed; rebuild and refresh receipt"
        );
    }
    elf
}

fn read_fixture(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    fs::read(path)
        .unwrap_or_else(|error| panic!("runtime probe fixture {}: {error}", path.display()))
}

fn serialize_account<T: AccountSerialize>(value: &T) -> Vec<u8> {
    let mut data = Vec::new();
    value
        .try_serialize(&mut data)
        .expect("fixture account serialization must succeed");
    data
}

fn deserialize_account<T: AccountDeserialize>(data: &[u8]) -> T {
    let mut serialized = data;
    T::try_deserialize(&mut serialized).expect("fixture account deserialization must succeed")
}

fn account_changes(
    before: &BTreeMap<Pubkey, AccountSnapshot>,
    after: &BTreeMap<Pubkey, AccountSnapshot>,
) -> Vec<AccountChange> {
    before
        .keys()
        .filter_map(|key| {
            let before = before
                .get(key)
                .and_then(|snapshot| snapshot.account.as_ref());
            let after = after
                .get(key)
                .and_then(|snapshot| snapshot.account.as_ref());
            if before.map(account_fingerprint) == after.map(account_fingerprint) {
                return None;
            }
            let owner = after
                .or(before)
                .map(|account| account.owner.to_string())
                .unwrap_or_else(|| Pubkey::default().to_string());
            let before_lamports = before.map(|account| account.lamports).unwrap_or_default();
            let after_lamports = after.map(|account| account.lamports).unwrap_or_default();
            let data = after
                .or(before)
                .map(|account| sha256(&account.data))
                .unwrap_or_default();
            Some(AccountChange {
                pubkey: key.to_string(),
                owner,
                lamport_delta: i128::from(after_lamports) - i128::from(before_lamports),
                data_sha256: data,
            })
        })
        .collect()
}

fn account_fingerprint(account: &Account) -> (u64, &[u8], Pubkey, bool, u64) {
    (
        account.lamports,
        &account.data,
        account.owner,
        account.executable,
        account.rent_epoch,
    )
}

fn changed_data_bytes(
    before: &BTreeMap<Pubkey, AccountSnapshot>,
    after: &BTreeMap<Pubkey, AccountSnapshot>,
) -> usize {
    before
        .keys()
        .map(|key| {
            let before = before
                .get(key)
                .and_then(|snapshot| snapshot.account.as_ref())
                .map(|account| account.data.as_slice())
                .unwrap_or_default();
            let after = after
                .get(key)
                .and_then(|snapshot| snapshot.account.as_ref())
                .map(|account| account.data.as_slice())
                .unwrap_or_default();
            let common = before.len().min(after.len());
            before[..common]
                .iter()
                .zip(&after[..common])
                .filter(|(left, right)| left != right)
                .count()
                + before.len().saturating_sub(common)
                + after.len().saturating_sub(common)
        })
        .sum()
}

fn rollback_except_fee(
    before: &BTreeMap<Pubkey, AccountSnapshot>,
    after: &BTreeMap<Pubkey, AccountSnapshot>,
    payer: Pubkey,
    payer_before: u64,
    payer_after: u64,
) -> bool {
    if payer_after > payer_before {
        return false;
    }
    before.iter().all(|(key, snapshot)| {
        if *key == payer {
            return snapshot
                .account
                .as_ref()
                .zip(after.get(key).and_then(|next| next.account.as_ref()))
                .map(|(before, after)| {
                    before.owner == after.owner
                        && before.executable == after.executable
                        && before.rent_epoch == after.rent_epoch
                        && before.data == after.data
                })
                .unwrap_or(false);
        }
        snapshot.account.as_ref().map(account_fingerprint)
            == after
                .get(key)
                .and_then(|next| next.account.as_ref())
                .map(account_fingerprint)
    })
}

fn sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
