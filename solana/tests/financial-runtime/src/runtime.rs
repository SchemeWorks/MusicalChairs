//! LiteSVM adapter for the production financial instruction surface.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use anchor_lang::{AccountDeserialize, AccountSerialize};
use litesvm::LiteSVM;
use musical_chairs_program::{state, ID};
use serde::Serialize;
use sha2::{Digest, Sha256};
use solana_sdk::{
    account::Account,
    clock::Clock,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::Transaction,
};

pub const PACKET_LIMIT_BYTES: usize = 1_232;
pub const COMPUTE_LIMIT: u32 = 1_400_000;
pub const PAYER_SEED: [u8; 32] = [0x11; 32];
pub const OWNER_SEED: [u8; 32] = [0x22; 32];
pub const ATTACKER_SEED: [u8; 32] = [0x33; 32];
pub const SPONSOR_SEED: [u8; 32] = [0x44; 32];
pub const MANAGEMENT_SEED: [u8; 32] = [0x55; 32];

const AIRDROP_LAMPORTS: u64 = 100_000_000_000;
const DAY: u64 = 86_400;
const OPEN_GROSS: u64 = 100_000_000;
const ENTRY_POT: u64 = 96_000_000;
const ENTRY_MANAGEMENT: u64 = 4_000_000;
const CLAIM_GROSS: u64 = 11_000_000;
const CLAIM_TOLL: u64 = 1_320_000;
const CLAIM_NET: u64 = 9_680_000;

#[derive(Clone, Debug, Serialize)]
pub struct AccountChange {
    pub pubkey: String,
    pub owner: String,
    pub lamport_delta: i128,
    pub data_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub label: String,
    pub artifact_sha256: String,
    pub success: bool,
    pub error: Option<String>,
    pub signature: Option<String>,
    pub expected_fee_lamports: u64,
    pub payer_lamports_before: u64,
    pub payer_lamports_after: u64,
    pub compute_limit: u32,
    pub compute_units: Option<u64>,
    pub packet_bytes: usize,
    pub fits_legacy_packet: bool,
    pub message_account_count: usize,
    pub writable_account_count: usize,
    pub declared_account_data_bytes: usize,
    pub changed_data_bytes: usize,
    pub account_changes: Vec<AccountChange>,
    pub logs: Vec<String>,
    pub return_data: Vec<u8>,
    pub rollback_except_fee: bool,
    pub runtime_loaded_account_count: Option<usize>,
    pub unsupported: Option<String>,
}

impl Observation {
    pub fn assert_rollback_except_fee(&self) {
        assert!(
            self.rollback_except_fee,
            "failed transaction changed an account beyond the runtime fee: {self:?}"
        );
    }
}

#[derive(Clone)]
struct AccountSnapshot(Option<Account>);

/// Deterministic local fixture.  Every actor is a synthetic keypair, and the
/// compiled ELF is read from disk rather than replaced by a host model.
pub struct FinancialRuntime {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub owner: Keypair,
    pub attacker: Keypair,
    pub sponsor: Keypair,
    pub management: Keypair,
    artifact_sha256: String,
}

impl FinancialRuntime {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new_from_array(PAYER_SEED);
        let owner = Keypair::new_from_array(OWNER_SEED);
        let attacker = Keypair::new_from_array(ATTACKER_SEED);
        let sponsor = Keypair::new_from_array(SPONSOR_SEED);
        let management = Keypair::new_from_array(MANAGEMENT_SEED);
        for actor in [&payer, &owner, &attacker, &sponsor, &management] {
            svm.airdrop(&actor.pubkey(), AIRDROP_LAMPORTS)
                .expect("synthetic fixture actor airdrop");
        }

        let elf = read_financial_elf();
        let artifact_sha256 = hex_sha256(&elf);
        svm.add_program(ID, &elf)
            .expect("actual Musical Chairs financial ELF must load into LiteSVM");
        svm.set_sysvar(&Clock {
            slot: 1,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 0,
        });

        let mut runtime = Self {
            svm,
            payer,
            owner,
            attacker,
            sponsor,
            management,
            artifact_sha256,
        };
        runtime.inject_canonical_fixture();
        runtime
    }

    pub fn set_clock(&mut self, now: i64, slot: u64) {
        self.svm.set_sysvar(&Clock {
            slot,
            epoch_start_timestamp: now,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now,
        });
    }

    pub fn pool_address(&self) -> Pubkey {
        pda(&[state::POOL_SEED]).0
    }
    pub fn admission_address(&self) -> Pubkey {
        pda(&[state::ADMISSION_SEED]).0
    }
    pub fn game_vault_address(&self) -> Pubkey {
        pda(&[state::GAME_VAULT_SEED]).0
    }
    pub fn ops_address(&self) -> Pubkey {
        pda(&[state::OPS_SEED]).0
    }
    pub fn ops_cash_address(&self) -> Pubkey {
        pda(&[state::OPS_CASH_SEED]).0
    }
    pub fn ticket_address(&self) -> Pubkey {
        pda(&[state::TICKET_SEED]).0
    }
    pub fn user_address(&self, owner: Pubkey) -> Pubkey {
        pda(&[state::USER_SEED, owner.as_ref()]).0
    }
    pub fn user_round_address(&self, round: u64, owner: Pubkey) -> Pubkey {
        pda(&[state::USER_ROUND_SEED, &round.to_le_bytes(), owner.as_ref()]).0
    }
    pub fn position_address(&self, owner: Pubkey, nonce: u64) -> Pubkey {
        pda(&[state::POSITION_SEED, owner.as_ref(), &nonce.to_le_bytes()]).0
    }
    pub fn backer_address(&self, series: u8, owner: Pubkey) -> Pubkey {
        pda(&[state::BACKER_SEED, &[series], owner.as_ref()]).0
    }
    pub fn registry_address(&self, index: u64) -> Pubkey {
        pda(&[state::REGISTRY_SEED, &index.to_le_bytes()]).0
    }
    pub fn snapshot_address(&self, index: u64) -> Pubkey {
        pda(&[state::SNAPSHOT_SEED, &index.to_le_bytes()]).0
    }

    pub fn account(&self, key: Pubkey) -> Account {
        self.svm
            .get_account(&key)
            .unwrap_or_else(|| panic!("fixture account {key} missing"))
    }

    pub fn deserialize<T: AccountDeserialize>(&self, key: Pubkey) -> T {
        let account = self.account(key);
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).expect("fixture Anchor account deserializes")
    }

    /// Submit one locally signed transaction and capture all observable
    /// runtime metadata.  `extra_signers` is explicit so payer and owner are
    /// kept separate in fee and custody assertions.
    pub fn execute(
        &mut self,
        label: impl Into<String>,
        instructions: Vec<Instruction>,
        extra_signers: &[&Keypair],
        compute_limit: u32,
    ) -> Observation {
        let label = label.into();
        let mut all_instructions = Vec::with_capacity(instructions.len() + 1);
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_limit,
        ));
        all_instructions.extend(instructions);
        let message = Message::new(&all_instructions, Some(&self.payer.pubkey()));
        let message_keys: BTreeSet<Pubkey> = message.account_keys.iter().copied().collect();
        let snapshots = self.snapshot(&message_keys);
        let payer_before = self.account(self.payer.pubkey()).lamports;
        let expected_fee_lamports = 5_000 * u64::from(message.header.num_required_signatures);
        let declared_account_data_bytes = snapshots
            .values()
            .filter_map(|snapshot| snapshot.0.as_ref())
            .map(|account| account.data.len())
            .sum();
        let mut transaction = Transaction::new_unsigned(message.clone());
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let mut signers: Vec<&dyn Signer> = Vec::with_capacity(extra_signers.len() + 1);
        signers.push(&self.payer);
        signers.extend(extra_signers.iter().map(|signer| *signer as &dyn Signer));
        let signing_error = transaction
            .try_partial_sign(&signers, blockhash)
            .err()
            .map(|error| format!("transaction signing failed: {error}"));
        let packet_bytes = bincode::serialize(&transaction).map(|bytes| bytes.len()).unwrap_or(0);
        if let Some(error) = signing_error {
            let after = self.snapshot(&message_keys);
            let payer_after = self.account(self.payer.pubkey()).lamports;
            let account_changes = account_changes(&snapshots, &after);
            let changed_data_bytes = changed_data_bytes(&snapshots, &after);
            return Observation {
                label,
                artifact_sha256: self.artifact_sha256.clone(),
                success: false,
                error: Some(error),
                signature: None,
                expected_fee_lamports,
                payer_lamports_before: payer_before,
                payer_lamports_after: payer_after,
                compute_limit,
                compute_units: None,
                packet_bytes,
                fits_legacy_packet: packet_bytes <= PACKET_LIMIT_BYTES,
                message_account_count: message.account_keys.len(),
                writable_account_count: message
                    .account_keys
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| message.is_maybe_writable(*index, None))
                    .count(),
                declared_account_data_bytes,
                changed_data_bytes,
                account_changes,
                logs: Vec::new(),
                return_data: Vec::new(),
                rollback_except_fee: rollback_except_fee(&snapshots, &after, self.payer.pubkey()),
                runtime_loaded_account_count: None,
                unsupported: Some("transaction was not submitted after signer validation".into()),
            };
        }
        let signature = transaction.signatures[0];
        let result = self.svm.send_transaction(transaction);
        let (success, error, compute_units, logs, return_data) = match result {
            Ok(meta) => (true, None, Some(meta.compute_units_consumed), meta.logs, meta.return_data.data),
            Err(failed) => (
                false,
                Some(format!("{:?}", failed.err)),
                Some(failed.meta.compute_units_consumed),
                failed.meta.logs,
                failed.meta.return_data.data,
            ),
        };
        let after = self.snapshot(&message_keys);
        let payer_after = self.account(self.payer.pubkey()).lamports;
        let account_changes = account_changes(&snapshots, &after);
        let changed_data_bytes = changed_data_bytes(&snapshots, &after);
        let fee_only = payer_before.checked_sub(payer_after) == Some(expected_fee_lamports)
            || (compute_units == Some(0) && payer_before == payer_after);
        Observation {
            label,
            artifact_sha256: self.artifact_sha256.clone(),
            success,
            error,
            signature: signature.map(|value| value.to_string()),
            expected_fee_lamports,
            payer_lamports_before: payer_before,
            payer_lamports_after: payer_after,
            compute_limit,
            compute_units,
            packet_bytes,
            fits_legacy_packet: packet_bytes <= PACKET_LIMIT_BYTES,
            message_account_count: message.account_keys.len(),
            writable_account_count: message
                .account_keys
                .iter()
                .enumerate()
                .filter(|(index, _)| message.is_maybe_writable(*index, None))
                .count(),
            declared_account_data_bytes,
            changed_data_bytes,
            account_changes,
            logs,
            return_data,
            rollback_except_fee: fee_only && rollback_except_fee(&snapshots, &after, self.payer.pubkey()),
            runtime_loaded_account_count: None,
            unsupported: Some("LiteSVM exposes no production loaded-account count".into()),
        }
    }

    fn inject_canonical_fixture(&mut self) {
        let owner = self.owner.pubkey();
        let round = 1;
        let pool = state::Pool {
            bump: pda(&[state::POOL_SEED]).1,
            round_id: round,
            next_sequence: 1,
            next_ticket: 1,
            sponsor: self.sponsor.pubkey(),
            management_recipient: self.management.pubkey(),
            ..state::Pool::default()
        };
        let pool_address = self.pool_address();
        self.put_program_account(pool_address, &pool, state::Pool::SPACE);
        let admission = state::Admission {
            bump: pda(&[state::ADMISSION_SEED]).1,
            guardian: self.attacker.pubkey(),
            ..state::Admission::default()
        };
        let admission_address = self.admission_address();
        self.put_program_account(admission_address, &admission, state::Admission::SPACE);
        let game_vault = state::GameVault {
            bump: pda(&[state::GAME_VAULT_SEED]).1,
        };
        let game_vault_address = self.game_vault_address();
        self.put_program_account(game_vault_address, &game_vault, state::GameVault::SPACE);
        let ops = state::Ops {
            bump: pda(&[state::OPS_SEED]).1,
            sponsor: self.sponsor.pubkey(),
            ..state::Ops::default()
        };
        let ops_address = self.ops_address();
        self.put_program_account(ops_address, &ops, state::Ops::SPACE);
        let ops_cash_address = self.ops_cash_address();
        let ops_cash_rent = self.rent(0);
        self.put_system_account(ops_cash_address, ops_cash_rent);
        let ticket = state::Ticket {
            bump: pda(&[state::TICKET_SEED]).1,
            ..state::Ticket::default()
        };
        let ticket_address = self.ticket_address();
        self.put_program_account(ticket_address, &ticket, state::Ticket::SPACE);
        let user = state::User {
            bump: pda(&[state::USER_SEED, owner.as_ref()]).1,
            owner,
            ..state::User::default()
        };
        let user_address = self.user_address(owner);
        self.put_program_account(user_address, &user, state::User::SPACE);
        let user_round = state::UserRound {
            bump: pda(&[state::USER_ROUND_SEED, &round.to_le_bytes(), owner.as_ref()]).1,
            round_id: round,
            owner,
            ..state::UserRound::default()
        };
        let user_round_address = self.user_round_address(round, owner);
        self.put_program_account(user_round_address, &user_round, state::UserRound::SPACE);
        for series in [0u8, 1u8] {
            let backer = state::Backer {
                bump: pda(&[state::BACKER_SEED, &[series], owner.as_ref()]).1,
                owner,
                series,
                ..state::Backer::default()
            };
            let backer_address = self.backer_address(series, owner);
            self.put_program_account(backer_address, &backer, state::Backer::SPACE);
        }
        let registry = state::RegistryPage {
            bump: pda(&[state::REGISTRY_SEED, &0u64.to_le_bytes()]).1,
            index: 0,
            ..state::RegistryPage::default()
        };
        let registry_address = self.registry_address(0);
        self.put_program_account(registry_address, &registry, state::RegistryPage::SPACE);
        let snapshot = state::SnapshotPage {
            bump: pda(&[state::SNAPSHOT_SEED, &0u64.to_le_bytes()]).1,
            index: 0,
            ticket_id: 0,
            ..state::SnapshotPage::default()
        };
        let snapshot_address = self.snapshot_address(0);
        self.put_program_account(snapshot_address, &snapshot, state::SnapshotPage::SPACE);
    }

    pub fn seed_claim_position(&mut self, pot: u64) {
        let owner = self.owner.pubkey();
        let position = state::Position {
            bump: pda(&[state::POSITION_SEED, owner.as_ref(), &1u64.to_le_bytes()]).1,
            owner,
            nonce: 1,
            round_id: 1,
            kind: 0,
            principal: OPEN_GROSS,
            opened_at: 0,
            maturity_at: 21 * DAY,
            last_accrued_at: 0,
            ..state::Position::default()
        };
        let position_address = self.position_address(owner, 1);
        self.put_program_account(position_address, &position, state::Position::SPACE);
        let pool_address = self.pool_address();
        let mut pool: state::Pool = self.deserialize(pool_address);
        pool.pot = pot;
        self.replace_program_account(pool_address, &pool, state::Pool::SPACE);
        let user_round_address = self.user_round_address(1, owner);
        let mut user_round: state::UserRound = self.deserialize(user_round_address);
        user_round.deposits = OPEN_GROSS as u128;
        self.replace_program_account(user_round_address, &user_round, state::UserRound::SPACE);
        let game_vault_address = self.game_vault_address();
        let vault = self.account(game_vault_address);
        self.replace_lamports(game_vault_address, vault.lamports + pot);
    }

    pub fn replace_pool(&mut self, pool: state::Pool) {
        let address = self.pool_address();
        self.replace_program_account(address, &pool, state::Pool::SPACE);
    }

    pub fn replace_ticket(&mut self, ticket: state::Ticket) {
        let address = self.ticket_address();
        self.replace_program_account(address, &ticket, state::Ticket::SPACE);
    }

    pub fn replace_snapshot(&mut self, snapshot: state::SnapshotPage) {
        let address = self.snapshot_address(snapshot.index);
        self.replace_program_account(address, &snapshot, state::SnapshotPage::SPACE);
    }

    pub fn replace_backer(&mut self, backer: state::Backer) {
        let address = self.backer_address(backer.series, backer.owner);
        self.replace_program_account(address, &backer, state::Backer::SPACE);
    }

    pub fn replace_registry(&mut self, registry: state::RegistryPage) {
        let address = self.registry_address(registry.index);
        self.replace_program_account(address, &registry, state::RegistryPage::SPACE);
    }

    /// Install the four-row allocation fixture used by the actual-SBF golden
    /// flow.  Page size remains eight; the four rows are deliberately below
    /// the transport boundary while the separate N17/N100 cases can reuse the
    /// same account codec when the worker expands them.
    pub fn seed_allocation(&mut self) {
        let owners = [
            self.owner.pubkey(),
            self.attacker.pubkey(),
            self.sponsor.pubkey(),
            self.management.pubkey(),
        ];
        let series = [0u8, 0, 1, 1];
        let remaining = [100_000u128, 200_000, 100_000, 100_000];
        let mut pool: state::Pool = self.deserialize(self.pool_address());
        pool.pending_toll = 10_000;
        pool.registry_len = 4;
        pool.registry_version = 1;
        pool.active_ticket = 1;
        self.replace_pool(pool);

        let mut ticket: state::Ticket = self.deserialize(self.ticket_address());
        ticket.id = 1;
        ticket.phase = state::DISCOVER;
        ticket.captured_len = 4;
        ticket.a_count = 2;
        ticket.b_count = 2;
        ticket.expected_version = 1;
        ticket.seed_credit = 5_000;
        ticket.senior_slice = 1_750;
        ticket.a_slice = 2_250;
        ticket.b_slice = 1_000;
        ticket.toll = 10_000;
        ticket.oldest = owners[0];
        ticket.oldest_sequence = 1;
        ticket.oldest_slot = 0;
        self.replace_ticket(ticket);

        let mut registry: state::RegistryPage = self.deserialize(self.registry_address(0));
        let mut snapshot: state::SnapshotPage = self.deserialize(self.snapshot_address(0));
        registry.index = 0;
        snapshot.index = 0;
        snapshot.ticket_id = 1;
        for index in 0..4 {
            registry.keys[index] = owners[index];
            snapshot.rows[index] = state::SnapshotRow {
                key: owners[index],
                series: series[index],
                remaining: remaining[index],
                slot: index as u64,
            };
            self.replace_backer(state::Backer {
                bump: pda(&[state::BACKER_SEED, &[series[index]], owners[index].as_ref()]).1,
                owner: owners[index],
                series: series[index],
                entitlement: remaining[index],
                first_sequence: index as u64 + 1,
                active: true,
                slot: index as u64,
                ..state::Backer::default()
            });
        }
        self.replace_registry(registry);
        self.replace_snapshot(snapshot);
    }

    fn rent(&self, data_len: usize) -> u64 {
        self.svm.minimum_balance_for_rent_exemption(data_len)
    }

    fn put_program_account<T: AccountSerialize>(&mut self, key: Pubkey, value: &T, space: usize) {
        let data = serialize_account(value);
        assert_eq!(data.len(), space, "production account wire size changed");
        let lamports = self.rent(data.len());
        self.svm
            .set_account(
                key,
                Account {
                    lamports,
                    data,
                    owner: ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("program-owned fixture account");
    }

    fn replace_program_account<T: AccountSerialize>(&mut self, key: Pubkey, value: &T, space: usize) {
        self.put_program_account(key, value, space);
    }

    fn put_system_account(&mut self, key: Pubkey, lamports: u64) {
        self.svm
            .set_account(
                key,
                Account {
                    lamports,
                    data: Vec::new(),
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("system-owned fixture account");
    }

    fn replace_lamports(&mut self, key: Pubkey, lamports: u64) {
        let mut account = self.account(key);
        account.lamports = lamports;
        self.svm.set_account(key, account).expect("fixture lamports");
    }

    fn snapshot(&self, keys: &BTreeSet<Pubkey>) -> BTreeMap<Pubkey, AccountSnapshot> {
        keys.iter()
            .copied()
            .map(|key| (key, AccountSnapshot(self.svm.get_account(&key))))
            .collect()
    }
}

impl Default for FinancialRuntime {
    fn default() -> Self {
        Self::new()
    }
}

pub fn anchor_ix<A: anchor_lang::ToAccountMetas, D: anchor_lang::InstructionData>(
    accounts: A,
    data: D,
) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

pub fn pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &ID)
}

fn serialize_account<T: AccountSerialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    value.try_serialize(&mut bytes).expect("Anchor fixture serialization");
    bytes
}

fn read_financial_elf() -> Vec<u8> {
    let path = env::var_os("MC_FINANCIAL_SBF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../programs/musical_chairs/artifacts/musical_chairs_program.so")
        });
    fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "actual compiled financial SBF {} is required (set MC_FINANCIAL_SBF): {error}",
            path.display()
        )
    })
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

fn account_changes(
    before: &BTreeMap<Pubkey, AccountSnapshot>,
    after: &BTreeMap<Pubkey, AccountSnapshot>,
) -> Vec<AccountChange> {
    before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|key| {
            let before = before.get(&key).and_then(|snapshot| snapshot.0.as_ref());
            let after = after.get(&key).and_then(|snapshot| snapshot.0.as_ref());
            if before.map(account_fingerprint) == after.map(account_fingerprint) {
                return None;
            }
            let account = after.or(before)?;
            let before_lamports = before.map(|value| value.lamports).unwrap_or_default();
            Some(AccountChange {
                pubkey: key.to_string(),
                owner: account.owner.to_string(),
                lamport_delta: i128::from(account.lamports) - i128::from(before_lamports),
                data_sha256: hex_sha256(&account.data),
            })
        })
        .collect()
}

fn changed_data_bytes(
    before: &BTreeMap<Pubkey, AccountSnapshot>,
    after: &BTreeMap<Pubkey, AccountSnapshot>,
) -> usize {
    before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|key| {
            let before = before
                .get(&key)
                .and_then(|snapshot| snapshot.0.as_ref())
                .map(|account| account.data.as_slice())
                .unwrap_or_default();
            let after = after
                .get(&key)
                .and_then(|snapshot| snapshot.0.as_ref())
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
) -> bool {
    before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .all(|key| {
            if key == payer {
                let before = before.get(&key).and_then(|snapshot| snapshot.0.as_ref());
                let after = after.get(&key).and_then(|snapshot| snapshot.0.as_ref());
                return before
                    .zip(after)
                    .map(|(before, after)| {
                        before.owner == after.owner
                            && before.executable == after.executable
                            && before.rent_epoch == after.rent_epoch
                            && before.data == after.data
                    })
                    .unwrap_or(false);
            }
            before.get(&key).and_then(|snapshot| snapshot.0.as_ref()).map(account_fingerprint)
                == after.get(&key).and_then(|snapshot| snapshot.0.as_ref()).map(account_fingerprint)
        })
}

fn hex_sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn open_constants() -> (u64, u64, u64) {
    (OPEN_GROSS, ENTRY_POT, ENTRY_MANAGEMENT)
}

pub fn claim_constants() -> (u64, u64, u64) {
    (CLAIM_GROSS, CLAIM_TOLL, CLAIM_NET)
}
