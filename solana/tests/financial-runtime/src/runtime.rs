//! LiteSVM adapter for the production financial instruction surface.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use anchor_lang::{event::EVENT_IX_TAG_LE, AccountDeserialize, AccountSerialize, Event};
use litesvm::LiteSVM;
use musical_chairs_program::{controls::SQUADS_PROGRAM_ID, state, ID};
use serde::Serialize;
use sha2::{Digest, Sha256};
use solana_sdk::{
    account::Account,
    clock::Clock,
    compute_budget::ComputeBudgetInstruction,
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_sdk_ids::bpf_loader_upgradeable;
use solana_system_interface::program as system_program;

pub const PACKET_LIMIT_BYTES: usize = 1_232;
pub const COMPUTE_LIMIT: u32 = 1_400_000;
pub const PAYER_SEED: [u8; 32] = [0x11; 32];
pub const OWNER_SEED: [u8; 32] = [0x22; 32];
pub const ATTACKER_SEED: [u8; 32] = [0x33; 32];
pub const SPONSOR_SEED: [u8; 32] = [0x44; 32];
pub const MANAGEMENT_SEED: [u8; 32] = [0x55; 32];
/// Squads `create_key` of the governance (G) multisig fixture.  The Multisig
/// PDA and its index-0 Vault PDA are derived from it (see `fixtures::squads`).
pub const G_CREATE_KEY: [u8; 32] = [0x47; 32];
/// Squads `create_key` of the pause-guardian (P) multisig fixture.
pub const P_CREATE_KEY: [u8; 32] = [0x50; 32];

const AIRDROP_LAMPORTS: u64 = 100_000_000_000;
const DAY: u64 = 86_400;
const OPEN_GROSS: u64 = 100_000_000;
const ENTRY_POT: u64 = 96_000_000;
const ENTRY_MANAGEMENT: u64 = 4_000_000;
const CLAIM_GROSS: u64 = 11_000_000;
const CLAIM_TOLL: u64 = 1_320_000;
const CLAIM_NET: u64 = 9_680_000;
/// Synthetic 32-byte `Pool.cluster_domain` test vector.  It is a fixture
/// label, not a real devnet/mainnet cluster domain.
pub const FIXTURE_CLUSTER_DOMAIN: [u8; 32] = *b"musical-chairs-litesvm-fixture!!";

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
    /// Data of every event-CPI inner instruction this program emitted in the
    /// transaction (`EVENT_IX_TAG_LE || discriminator || Borsh body`), in
    /// emission order.  Decoded by [`Observation::events`]; not part of the
    /// evidence JSON, so existing labels stay byte-identical.
    #[serde(skip)]
    pub event_data: Vec<Vec<u8>>,
}

impl Observation {
    /// Every `E` event this transaction emitted through `emit_cpi!`, decoded
    /// from the inner-instruction data (logs can be truncated, instruction
    /// data cannot).  Events from other types are skipped by discriminator.
    pub fn events<E: Event>(&self) -> Vec<E> {
        self.event_data
            .iter()
            .filter_map(|data| {
                let body = data.strip_prefix(EVENT_IX_TAG_LE)?;
                let payload = body.strip_prefix(E::DISCRIMINATOR)?;
                Some(E::try_from_slice(payload).unwrap_or_else(|error| {
                    panic!("{}: malformed event payload: {error}", self.label)
                }))
            })
            .collect()
    }

    /// The last `E` event of the transaction; panics when none was emitted.
    pub fn last_event<E: Event>(&self) -> E {
        self.events::<E>().pop().unwrap_or_else(|| {
            panic!(
                "{}: no {} event in the transaction's inner instructions ({} event payloads)",
                self.label,
                std::any::type_name::<E>(),
                self.event_data.len()
            )
        })
    }

    pub fn assert_rollback_except_fee(&self) {
        assert!(
            self.rollback_except_fee,
            "failed transaction changed an account beyond the runtime fee: {self:?}"
        );
    }
}

#[derive(Clone)]
struct AccountSnapshot(Option<Account>);

/// A locally signed (possibly partially signed) transaction that has not been
/// submitted yet.  See [`FinancialRuntime::sign`].
#[derive(Clone)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    signing_error: Option<String>,
}

impl SignedTransaction {
    /// The fee payer's signature, i.e. the transaction id.
    pub fn signature(&self) -> String {
        self.transaction.signatures[0].to_string()
    }
}

/// The data of every `emit_cpi!` self-invocation in `meta`, in emission order:
/// an inner instruction whose data starts with the 8-byte `EVENT_IX_TAG_LE`
/// magic.  Only this program emits it in these transactions (the Squads
/// stand-in and the system program never do), and the match deliberately does
/// not depend on mapping the inner instruction's program index back to a
/// message key.
fn event_payloads(meta: &litesvm::types::TransactionMetadata) -> Vec<Vec<u8>> {
    meta.inner_instructions
        .iter()
        .flatten()
        .filter(|inner| inner.instruction.data.starts_with(EVENT_IX_TAG_LE))
        .map(|inner| inner.instruction.data.clone())
        .collect()
}

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
    /// The main program ELF, kept so the loader fixture can append it to the
    /// `ProgramData` header.
    elf: Vec<u8>,
    /// SHA-256 of the Squads vault stand-in ELF when it is loaded.
    standin_sha256: Option<String>,
}

impl FinancialRuntime {
    /// The M0/M2 fixture: canonical state injected, no Squads accounts and no
    /// upgradeable-loader shape.  The pause guardian is the synthetic
    /// `attacker` keypair so the M0 control rows keep their recorded
    /// outcomes; `Pool.governance` stays unset.  M3 authenticated-controls
    /// tests use [`FinancialRuntime::new_governed`] instead.
    pub fn new() -> Self {
        let mut runtime = Self::bootstrap(false);
        runtime.inject_canonical_fixture();
        runtime
    }

    /// Programs loaded (main ELF under the upgradeable loader, Squads vault
    /// stand-in at the Squads id), actors funded, `ProgramData` authority =
    /// the G vault, and the committed G and P `Multisig` accounts installed
    /// owned by the Squads id.  NO program state is injected, so `initialize`
    /// runs against fresh PDAs.
    pub fn new_uninitialized() -> Self {
        Self::bootstrap(true)
    }

    /// [`FinancialRuntime::new_uninitialized`] plus the canonical state
    /// fixture, bound to the Squads fixtures: `Pool.governance` is the G
    /// vault and `Pool.guardian_vault` = `Admission.guardian_vault` is the P
    /// vault.  This is the post-`initialize` genesis shape.
    pub fn new_governed() -> Self {
        let mut runtime = Self::new_uninitialized();
        runtime.inject_canonical_fixture();
        let governance = runtime.g_vault();
        let guardian = runtime.p_vault();
        let mut pool: state::Pool = runtime.deserialize(runtime.pool_address());
        pool.governance = governance;
        pool.guardian_vault = guardian;
        runtime.replace_pool(pool);
        let mut admission: state::Admission = runtime.deserialize(runtime.admission_address());
        admission.guardian_vault = guardian;
        runtime.replace_admission(admission);
        runtime
    }

    fn bootstrap(m3: bool) -> Self {
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
        let standin_sha256 = if m3 {
            let standin = read_standin_elf();
            svm.add_program(SQUADS_PROGRAM_ID, &standin)
                .expect("actual Squads vault stand-in ELF must load into LiteSVM");
            Some(hex_sha256(&standin))
        } else {
            None
        };

        let mut runtime = Self {
            svm,
            payer,
            owner,
            attacker,
            sponsor,
            management,
            artifact_sha256,
            elf,
            standin_sha256,
        };
        if m3 {
            // Installed while the clock is still at its default, exactly like
            // the M0 `add_program` above, so no program-cache entry is created
            // after the slot moves.
            let governance = runtime.g_vault();
            runtime.install_loader_fixture(Some(governance));
            let g_multisig = runtime.g_multisig();
            runtime.install_squads_account(
                g_multisig,
                SQUADS_PROGRAM_ID,
                fixtures::squads::committed_governance(),
            );
            let p_multisig = runtime.p_multisig();
            runtime.install_squads_account(
                p_multisig,
                SQUADS_PROGRAM_ID,
                fixtures::squads::committed_guardian(),
            );
        }
        runtime.set_clock(0, 1);
        runtime
    }

    /// SHA-256 of the loaded Squads vault stand-in ELF, when this runtime
    /// loaded it (`new_uninitialized` / `new_governed`).
    pub fn standin_artifact_sha256(&self) -> Option<&str> {
        self.standin_sha256.as_deref()
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

    /// The G (governance) Squads `Multisig` PDA.
    pub fn g_multisig(&self) -> Pubkey {
        fixtures::squads::multisig_address(&Pubkey::new_from_array(G_CREATE_KEY)).0
    }
    /// The G Vault PDA (vault index 0) that must sign governance instructions.
    pub fn g_vault(&self) -> Pubkey {
        fixtures::squads::vault_address(&self.g_multisig(), 0).0
    }
    /// The P (pause guardian) Squads `Multisig` PDA.
    pub fn p_multisig(&self) -> Pubkey {
        fixtures::squads::multisig_address(&Pubkey::new_from_array(P_CREATE_KEY)).0
    }
    /// The P Vault PDA (vault index 0) that must sign `set_pause`.
    pub fn p_vault(&self) -> Pubkey {
        fixtures::squads::vault_address(&self.p_multisig(), 0).0
    }
    /// `["<main program id>"]` under the upgradeable loader.
    pub fn program_data_address(&self) -> Pubkey {
        fixtures::loader::program_data_address()
    }

    /// Install the loader-shaped fixture: a `ProgramData` account (header
    /// with `slot` 1 and `authority`, then the main ELF) and the `Program`
    /// account at the main program id, owned by the upgradeable loader and
    /// executable.  The ProgramData is written first because LiteSVM loads
    /// the program from it.  Execution keeps using the actual ELF.
    pub fn install_loader_fixture(&mut self, authority: Option<Pubkey>) {
        let mut program_data =
            fixtures::loader::programdata_header(fixtures::loader::SLOT, authority);
        program_data.extend_from_slice(&self.elf);
        let lamports = self.rent(program_data.len());
        self.svm
            .set_account(
                fixtures::loader::program_data_address(),
                Account {
                    lamports,
                    data: program_data,
                    owner: bpf_loader_upgradeable::id(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("ProgramData loader fixture");
        let program = fixtures::loader::program_header();
        let lamports = self.rent(program.len());
        self.svm
            .set_account(
                ID,
                Account {
                    lamports,
                    data: program,
                    owner: bpf_loader_upgradeable::id(),
                    executable: true,
                    rent_epoch: 0,
                },
            )
            .expect("Program loader fixture");
    }

    /// Rewrite only the `ProgramData` header (the ELF suffix and the program
    /// cache entry are untouched), e.g. to model an authority mismatch or an
    /// immutable (`None`) program.
    pub fn set_programdata_authority(&mut self, authority: Option<Pubkey>) {
        let address = fixtures::loader::program_data_address();
        let mut account = self.account(address);
        let header = fixtures::loader::programdata_header(fixtures::loader::SLOT, authority);
        account.data[..header.len()].copy_from_slice(&header);
        self.svm
            .set_account(address, account)
            .expect("ProgramData authority rewrite");
    }

    /// Install (or overwrite) an arbitrary account with rent-exempt lamports.
    /// Used for the Squads `Multisig` fixtures, including malformed variants.
    pub fn install_squads_account(&mut self, address: Pubkey, owner: Pubkey, data: Vec<u8>) {
        let lamports = self.rent(data.len());
        self.svm
            .set_account(
                address,
                Account {
                    lamports,
                    data,
                    owner,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("Squads multisig fixture account");
    }

    /// Install a canonical-shape Multisig (threshold 2, three members with
    /// mask 7, autonomous, no rent collector) for `create_key` with the given
    /// `time_lock`, owned by the Squads id.  Returns the Multisig PDA.
    pub fn install_multisig(&mut self, create_key: [u8; 32], time_lock: u32) -> Pubkey {
        let data = fixtures::squads::profile_bytes(create_key, time_lock, 2);
        let (address, _) = fixtures::squads::multisig_address(&Pubkey::new_from_array(create_key));
        self.install_squads_account(address, SQUADS_PROGRAM_ID, data);
        address
    }

    /// Run `inner` as if the Squads vault at `vault_index` of `multisig` had
    /// executed it: wrap it in one stand-in instruction.
    ///
    /// Stand-in data: `[vault_index u8][bump u8][inner.data]`.  Stand-in
    /// accounts, in order: `[multisig (read-only), vault (read-only, NOT a
    /// signer in this outer list), inner.program_id (read-only), inner.accounts
    /// ...]` where the vault's own meta inside the inner list is de-signed
    /// here and re-signed by the stand-in through `invoke_signed` with the
    /// vault seeds.  Every other signer flag is kept, so the matching keypairs
    /// must be in `extra_signers`.
    pub fn execute_via_vault(
        &mut self,
        label: impl Into<String>,
        multisig: Pubkey,
        vault_index: u8,
        inner: Instruction,
        extra_signers: &[&Keypair],
        compute_limit: u32,
    ) -> Observation {
        let (vault, bump) = fixtures::squads::vault_address(&multisig, vault_index);
        let mut data = Vec::with_capacity(2 + inner.data.len());
        data.push(vault_index);
        data.push(bump);
        data.extend_from_slice(&inner.data);
        let mut accounts = vec![
            AccountMeta::new_readonly(multisig, false),
            AccountMeta::new_readonly(vault, false),
            AccountMeta::new_readonly(inner.program_id, false),
        ];
        accounts.extend(inner.accounts.into_iter().map(|mut meta| {
            if meta.pubkey == vault {
                meta.is_signer = false;
            }
            meta
        }));
        let wrapped = Instruction {
            program_id: SQUADS_PROGRAM_ID,
            accounts,
            data,
        };
        self.execute(label, vec![wrapped], extra_signers, compute_limit)
    }

    /// Value-conservation invariants of the two physical cash registers,
    /// asserted on current state:
    /// `ops_cash.lamports - rent(0) == free + locked + unallocated_excess` and
    /// `game_vault.lamports - rent(GameVault::SPACE) ==` every game partition
    /// (`pot + seed + management + pending_toll + funded_claims + excess`, the
    /// program's own `game_accounted`).
    pub fn conservation_check(&self) {
        let pool: state::Pool = self.deserialize(self.pool_address());
        let ops: state::Ops = self.deserialize(self.ops_address());
        let ops_cash = self.account(self.ops_cash_address()).lamports;
        let ops_spendable = ops_cash
            .checked_sub(self.rent(0))
            .unwrap_or_else(|| panic!("ops_cash {ops_cash} lamports is below its rent floor"));
        let ops_recorded = u128::from(ops.free_balance)
            + u128::from(ops.locked_remaining_total)
            + u128::from(ops.unallocated_excess);
        assert_eq!(
            u128::from(ops_spendable),
            ops_recorded,
            "ops_cash conservation: spendable {ops_spendable} != free {} + locked {} + unallocated {}",
            ops.free_balance,
            ops.locked_remaining_total,
            ops.unallocated_excess
        );
        let vault = self.account(self.game_vault_address()).lamports;
        let vault_spendable = vault
            .checked_sub(self.rent(state::GameVault::SPACE))
            .unwrap_or_else(|| panic!("game_vault {vault} lamports is below its rent floor"));
        let accounted = musical_chairs_program::game_accounted(&pool)
            .expect("game partitions sum without overflow");
        assert_eq!(
            u128::from(vault_spendable),
            accounted,
            "game_vault conservation: spendable {vault_spendable} != pot {} + seed {} + management {} + pending_toll {} + funded_claims {} + excess {}",
            pool.pot,
            pool.seed,
            pool.management,
            pool.pending_toll,
            pool.funded_claims,
            pool.excess
        );
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
    pub fn config_address(&self, version: u32) -> Pubkey {
        pda(&[state::CONFIG_SEED, &version.to_le_bytes()]).0
    }
    pub fn rng_authority_address(&self) -> Pubkey {
        pda(&[state::RNG_AUTHORITY_SEED]).0
    }
    /// Anchor `#[event_cpi]` self-CPI authority PDA (`__event_authority`).
    pub fn event_authority_address(&self) -> Pubkey {
        pda(&[b"__event_authority"]).0
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
    /// `["participants", round, index]`: a round's participant page PDA.
    pub fn participant_address(&self, round: u64, index: u64) -> Pubkey {
        pda(&[
            state::PARTICIPANTS_SEED,
            &round.to_le_bytes(),
            &index.to_le_bytes(),
        ])
        .0
    }
    /// `["close", close_id]`: the canonical `CloseRecord` PDA.
    pub fn close_record_address(&self, close_id: u64) -> Pubkey {
        pda(&[state::CLOSE_SEED, &close_id.to_le_bytes()]).0
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

    /// [`FinancialRuntime::deserialize`], except that an account which is
    /// absent, not program-owned or not of type `T` is `None`.
    pub fn try_deserialize<T: AccountDeserialize>(&self, key: Pubkey) -> Option<T> {
        let account = self.svm.get_account(&key)?;
        if account.owner != ID || account.data.is_empty() {
            return None;
        }
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).ok()
    }

    /// Build and sign one transaction (fresh blockhash, harness payer first)
    /// without submitting it.  [`FinancialRuntime::submit`] can then send the
    /// very same signed bytes more than once, which is how the duplicate
    /// signature rows observe the runtime's `AlreadyProcessed` refusal.
    pub fn sign(
        &mut self,
        instructions: Vec<Instruction>,
        extra_signers: &[&Keypair],
        compute_limit: u32,
    ) -> SignedTransaction {
        let mut all_instructions = Vec::with_capacity(instructions.len() + 1);
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_limit,
        ));
        all_instructions.extend(instructions);
        let message = Message::new(&all_instructions, Some(&self.payer.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let mut signers: Vec<&dyn Signer> = Vec::with_capacity(extra_signers.len() + 1);
        signers.push(&self.payer);
        signers.extend(extra_signers.iter().map(|signer| *signer as &dyn Signer));
        let signing_error = transaction
            .try_partial_sign(&signers, blockhash)
            .err()
            .map(|error| format!("transaction signing failed: {error}"));
        SignedTransaction {
            transaction,
            signing_error,
        }
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
        let signed = self.sign(instructions, extra_signers, compute_limit);
        self.submit(label, &signed, compute_limit)
    }

    /// Submit an already signed transaction and capture all observable
    /// runtime metadata.  Submitting the same [`SignedTransaction`] twice
    /// presents the identical signature twice.
    pub fn submit(
        &mut self,
        label: impl Into<String>,
        signed: &SignedTransaction,
        compute_limit: u32,
    ) -> Observation {
        let label = label.into();
        let transaction = signed.transaction.clone();
        let message = transaction.message.clone();
        let message_keys: BTreeSet<Pubkey> = message.account_keys.iter().copied().collect();
        let snapshots = self.snapshot(&message_keys);
        let payer_before = self.account(self.payer.pubkey()).lamports;
        let expected_fee_lamports = 5_000 * u64::from(message.header.num_required_signatures);
        let declared_account_data_bytes = snapshots
            .values()
            .filter_map(|snapshot| snapshot.0.as_ref())
            .map(|account| account.data.len())
            .sum();
        let packet_bytes = bincode::serialize(&transaction).map(|bytes| bytes.len()).unwrap_or(0);
        if let Some(error) = signed.signing_error.clone() {
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
                event_data: Vec::new(),
            };
        }
        let signature = transaction.signatures[0];
        let result = self.svm.send_transaction(transaction);
        let (success, error, compute_units, logs, return_data, event_data) = match result {
            Ok(meta) => {
                let events = event_payloads(&meta);
                (
                    true,
                    None,
                    Some(meta.compute_units_consumed),
                    meta.logs,
                    meta.return_data.data,
                    events,
                )
            }
            Err(failed) => {
                let events = event_payloads(&failed.meta);
                (
                    false,
                    Some(format!("{:?}", failed.err)),
                    Some(failed.meta.compute_units_consumed),
                    failed.meta.logs,
                    failed.meta.return_data.data,
                    events,
                )
            }
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
            signature: Some(signature.to_string()),
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
            event_data,
        }
    }

    fn inject_canonical_fixture(&mut self) {
        let owner = self.owner.pubkey();
        let round = 1;
        let pool = state::Pool {
            bump: pda(&[state::POOL_SEED]).1,
            round_id: round,
            // No active Backer is seeded here (the two Backer accounts below
            // are inactive, `first_sequence == 0`), so the next sequence is
            // the genesis value one past the highest seeded sequence (0).
            next_sequence: 1,
            next_ticket: 1,
            // The empty RegistryPage 0 below holds no rows.
            registry_len: 0,
            // SnapshotPage 0 is injected below, so exactly one page exists.
            snapshot_pages: 1,
            // Likewise RegistryPage 0 is injected below.
            registry_pages: 1,
            // Genesis shape, mirroring `controls::initialize`: config v1 is
            // active, the first close id is 1 and no close is in progress.
            cluster_domain: FIXTURE_CLUSTER_DOMAIN,
            active_config_version: 1,
            next_close_id: 1,
            current_close_id: None,
            // Genesis copy of `Admission.guardian_vault` (set below).
            guardian_vault: self.attacker.pubkey(),
            sponsor: self.sponsor.pubkey(),
            management_recipient: self.management.pubkey(),
            // Genesis shape (M4): `protected_rent` is the live GameVault rent
            // floor, which is also what `sync_game` writes on later calls.
            protected_rent: self.rent(state::GameVault::SPACE),
            ..state::Pool::default()
        };
        let pool_address = self.pool_address();
        self.put_program_account(pool_address, &pool, state::Pool::SPACE);
        let admission = state::Admission {
            bump: pda(&[state::ADMISSION_SEED]).1,
            guardian_vault: self.attacker.pubkey(),
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
            // Genesis shape (M4): `protected_rent` is the live OpsCash rent
            // floor, which is also what `sync_ops` writes on later calls.
            protected_rent: self.rent(0),
            ..state::Ops::default()
        };
        let ops_address = self.ops_address();
        self.put_program_account(ops_address, &ops, state::Ops::SPACE);
        let ops_cash_address = self.ops_cash_address();
        let ops_cash_rent = self.rent(0);
        self.put_system_account(ops_cash_address, ops_cash_rent);
        // Genesis-shaped, mirroring `controls::initialize`: id 0, phase IDLE,
        // every other field zero.
        let ticket = state::Ticket {
            bump: pda(&[state::TICKET_SEED]).1,
            id: 0,
            phase: state::IDLE,
            ..state::Ticket::default()
        };
        let ticket_address = self.ticket_address();
        self.put_program_account(ticket_address, &ticket, state::Ticket::SPACE);
        // `controls::initialize` does not create these two accounts; the
        // fixture installs them at their canonical PDAs.  Config v1 is the
        // accepted launch rule set; the RNG authority is unbound until a
        // provider client is registered (`client == Pubkey::default()`).
        let config = state::ConfigVersion {
            bump: pda(&[state::CONFIG_SEED, &1u32.to_le_bytes()]).1,
            ..state::ConfigVersion::default_v1()
        };
        let config_address = self.config_address(1);
        self.put_program_account(config_address, &config, state::ConfigVersion::SPACE);
        let rng_authority = state::RngAuthority {
            bump: pda(&[state::RNG_AUTHORITY_SEED]).1,
            client: Pubkey::default(),
        };
        let rng_authority_address = self.rng_authority_address();
        self.put_program_account(
            rng_authority_address,
            &rng_authority,
            state::RngAuthority::SPACE,
        );
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
            rent: state::RentComponents::default(),
            ..state::RegistryPage::default()
        };
        let registry_address = self.registry_address(0);
        self.put_program_account(registry_address, &registry, state::RegistryPage::SPACE);
        let snapshot = state::SnapshotPage {
            bump: pda(&[state::SNAPSHOT_SEED, &0u64.to_le_bytes()]).1,
            index: 0,
            ticket_id: 0,
            rent: state::RentComponents::default(),
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
            // The payout's `config` account is the version the position
            // pinned at open; the canonical fixture holds config v1.
            config_version: 1,
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

    /// Fixture write: install round `round`'s `ParticipantPage` number `index`
    /// (empty, identity fields matching its seeds) and raise
    /// `Pool.participant_pages` to cover it, exactly the shape a `Free`
    /// `provision_page(Participant { round })` leaves.  The canonical fixture
    /// deliberately does NOT carry one (the provisioning rows create their
    /// own), so the first `open_position` of a round needs this.
    pub fn install_participant_page(&mut self, round: u64, index: u64) {
        let page = state::ParticipantPage {
            bump: pda(&[
                state::PARTICIPANTS_SEED,
                &round.to_le_bytes(),
                &index.to_le_bytes(),
            ])
            .1,
            round_id: round,
            page: index,
            ..state::ParticipantPage::default()
        };
        self.replace_participant_page(page);
        let mut pool: state::Pool = self.deserialize(self.pool_address());
        pool.participant_pages = pool.participant_pages.max(index + 1);
        self.replace_pool(pool);
    }

    pub fn replace_participant_page(&mut self, page: state::ParticipantPage) {
        let address = self.participant_address(page.round_id, page.page);
        self.replace_program_account(address, &page, state::ParticipantPage::SPACE);
    }

    pub fn replace_user(&mut self, user: state::User) {
        let address = self.user_address(user.owner);
        self.replace_program_account(address, &user, state::User::SPACE);
    }

    pub fn replace_user_round(&mut self, user_round: state::UserRound) {
        let address = self.user_round_address(user_round.round_id, user_round.owner);
        self.replace_program_account(address, &user_round, state::UserRound::SPACE);
    }

    pub fn replace_position(&mut self, position: state::Position) {
        let address = self.position_address(position.owner, position.nonce);
        self.replace_program_account(address, &position, state::Position::SPACE);
    }

    pub fn replace_config(&mut self, config: state::ConfigVersion) {
        let address = self.config_address(config.version);
        self.replace_program_account(address, &config, state::ConfigVersion::SPACE);
    }

    /// Fixture write: overwrite the lamports of an existing account.
    pub fn set_lamports(&mut self, key: Pubkey, lamports: u64) {
        self.replace_lamports(key, lamports);
    }

    pub fn replace_pool(&mut self, pool: state::Pool) {
        let address = self.pool_address();
        self.replace_program_account(address, &pool, state::Pool::SPACE);
    }

    pub fn replace_admission(&mut self, admission: state::Admission) {
        let address = self.admission_address();
        self.replace_program_account(address, &admission, state::Admission::SPACE);
    }

    pub fn replace_ops(&mut self, ops: state::Ops) {
        let address = self.ops_address();
        self.replace_program_account(address, &ops, state::Ops::SPACE);
    }

    /// Fixture write: credit `lamports` to an existing account (the funding
    /// half of a state seed, so conservation keeps holding).
    pub fn add_lamports(&mut self, key: Pubkey, lamports: u64) {
        let current = self.account(key).lamports;
        self.replace_lamports(key, current + lamports);
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

    /// Fixture write: install or overwrite `CloseRecord(record.close_id)` at
    /// its canonical PDA.  An existing account keeps its lamports (so a
    /// provisioned record's rent, or a pre-credit, is never disturbed); a new
    /// one is funded with exactly its rent floor.
    pub fn replace_close_record(&mut self, record: state::CloseRecord) {
        let address = self.close_record_address(record.close_id);
        self.write_program_account_keeping_lamports(address, &record, state::CloseRecord::SPACE);
    }

    /// Fixture write: a `CloseRecord`-typed, program-owned account at an
    /// ARBITRARY address (for spoofed-address rejection rows).
    pub fn put_close_record_at(&mut self, address: Pubkey, record: state::CloseRecord) {
        self.write_program_account_keeping_lamports(address, &record, state::CloseRecord::SPACE);
    }

    /// Fixture write: delete an account (LiteSVM drops zero-lamport accounts).
    pub fn remove_account(&mut self, key: Pubkey) {
        self.svm
            .set_account(key, Account::default())
            .expect("fixture account removal");
    }

    /// Fixture write: credit `lamports` to `key`, creating an empty
    /// system-owned account when none exists (a prefunded, not yet created PDA).
    pub fn credit_lamports(&mut self, key: Pubkey, lamports: u64) {
        match self.svm.get_account(&key) {
            Some(account) => self.replace_lamports(key, account.lamports + lamports),
            None => self.put_system_account(key, lamports),
        }
    }

    /// Turn the canonical fixture into the exact post-`initialize` page shape:
    /// `initialize` creates no registry or snapshot page, so page 0 of each is
    /// removed and both page counters are zero.
    pub fn strip_pages_to_genesis(&mut self) {
        let registry = self.registry_address(0);
        let snapshot = self.snapshot_address(0);
        self.remove_account(registry);
        self.remove_account(snapshot);
        let mut pool: state::Pool = self.deserialize(self.pool_address());
        pool.registry_pages = 0;
        pool.snapshot_pages = 0;
        self.replace_pool(pool);
    }

    /// Install the four-row allocation fixture used by the actual-SBF golden
    /// flow.  Page size remains eight; the four rows are deliberately below
    /// the transport boundary while the separate N17/N100 cases can reuse the
    /// same account codec when the worker expands them.
    ///
    /// Inputs are the accepted `allocation_golden_t10000` vector: toll 10_000,
    /// Series-A remaining 4_000 / 6_000, two Series-B rows.  They are seeded
    /// as Backer state; the Discover-phase ticket carries only
    /// `id`/`phase`/`captured_len`/`expected_version`/`toll`, the pinned
    /// 3_500/4_500/2_000 backer-half split and `close_id: None`.  Counts, slices,
    /// `a_remainder_total`, `oldest*` and `credited` start at zero because the
    /// handlers compute them (so no `ticket.oldest` is seeded; the handler
    /// derives it as a Backer PDA).  Registry and snapshot row keys are Backer
    /// PDA addresses, not owner wallet keys.
    pub fn seed_allocation(&mut self) {
        let owners = [
            self.owner.pubkey(),
            self.attacker.pubkey(),
            self.sponsor.pubkey(),
            self.management.pubkey(),
        ];
        let series = [0u8, 0, 1, 1];
        let remaining = [4_000u128, 6_000, 100_000, 100_000];
        let backer_keys: [Pubkey; 4] =
            std::array::from_fn(|index| self.backer_address(series[index], owners[index]));
        // Seeded Backer funding sequences; `next_sequence` is one past the max.
        let first_sequences = [1u64, 2, 3, 4];
        let highest_first_sequence = first_sequences.into_iter().max().unwrap_or(0);

        let mut pool: state::Pool = self.deserialize(self.pool_address());
        pool.pending_toll = 10_000;
        pool.registry_len = owners.len() as u64;
        pool.registry_version = 1;
        pool.next_sequence = highest_first_sequence + 1;
        // Ticket 1 is already minted and active, so the next id is 2.
        pool.next_ticket = 2;
        pool.active_ticket = 1;
        pool.snapshot_pages = 1;
        self.replace_pool(pool);

        let ticket = state::Ticket {
            bump: pda(&[state::TICKET_SEED]).1,
            id: 1,
            phase: state::DISCOVER,
            captured_len: owners.len() as u64,
            expected_version: 1,
            toll: 10_000,
            // Pinned from config v1 at ticket creation; plain allocation
            // (not a closing payout), so no close id.
            senior_bps: 3_500,
            a_bps: 4_500,
            b_bps: 2_000,
            close_id: None,
            ..state::Ticket::default()
        };
        self.replace_ticket(ticket);

        let mut registry: state::RegistryPage = self.deserialize(self.registry_address(0));
        let mut snapshot: state::SnapshotPage = self.deserialize(self.snapshot_address(0));
        registry.index = 0;
        snapshot.index = 0;
        snapshot.ticket_id = 1;
        for index in 0..owners.len() {
            registry.keys[index] = backer_keys[index];
            snapshot.rows[index] = state::SnapshotRow {
                key: backer_keys[index],
                series: series[index],
                remaining: remaining[index],
                slot: index as u64,
            };
            self.replace_backer(state::Backer {
                bump: pda(&[state::BACKER_SEED, &[series[index]], owners[index].as_ref()]).1,
                owner: owners[index],
                series: series[index],
                entitlement: remaining[index],
                first_sequence: first_sequences[index],
                active: true,
                slot: index as u64,
                ..state::Backer::default()
            });
        }
        self.replace_registry(registry);
        self.replace_snapshot(snapshot);
    }

    pub fn rent(&self, data_len: usize) -> u64 {
        self.svm.minimum_balance_for_rent_exemption(data_len)
    }

    /// Anchor allocates `T::SPACE` bytes and Borsh writes the serialized prefix;
    /// `Option`/enum fields encode shorter than the space `InitSpace` reserves,
    /// so the fixture pads to `space` with zeros exactly as an on-chain account
    /// created with `space = T::SPACE` would look. A longer encoding is a bug.
    fn put_program_account<T: AccountSerialize>(&mut self, key: Pubkey, value: &T, space: usize) {
        let mut data = serialize_account(value);
        assert!(
            data.len() <= space,
            "production account wire size {} exceeds declared space {}",
            data.len(),
            space
        );
        data.resize(space, 0);
        let lamports = self.rent(space);
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

    /// [`Self::put_program_account`], except that an account which already
    /// exists keeps the lamports it holds.
    fn write_program_account_keeping_lamports<T: AccountSerialize>(
        &mut self,
        key: Pubkey,
        value: &T,
        space: usize,
    ) {
        let existing = self.svm.get_account(&key).map(|account| account.lamports);
        self.put_program_account(key, value, space);
        if let Some(lamports) = existing {
            self.replace_lamports(key, lamports);
        }
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

/// SHA-256 of the Squads vault stand-in ELF selected by `MC_STANDIN_SBF`.
pub fn standin_artifact_sha256() -> String {
    hex_sha256(&read_standin_elf())
}

fn read_standin_elf() -> Vec<u8> {
    let path = env::var_os("MC_STANDIN_SBF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("standin/squads-vault-standin/artifacts/squads_vault_standin.so")
        });
    fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "actual compiled Squads vault stand-in SBF {} is required (set MC_STANDIN_SBF): {error}",
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

/// Byte-exact local fixtures for the two external account shapes the control
/// plane reads: Squads v4 `Multisig` accounts and the upgradeable-loader
/// `Program`/`ProgramData` pair.  Both are LOCAL fixtures: no on-chain Squads
/// account or deployed program is involved.  The committed `.bin` files under
/// `fixtures/` are reproduced by these generators (see the tests in `lib.rs`).
pub mod fixtures {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn fixture_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(relative)
    }

    fn read_fixture(relative: &str) -> Vec<u8> {
        let path = fixture_path(relative);
        fs::read(&path).unwrap_or_else(|error| {
            panic!("committed fixture {} is required: {error}", path.display())
        })
    }

    pub mod squads {
        use musical_chairs_program::controls::{SQUADS_MULTISIG_DISCRIMINATOR, SQUADS_PROGRAM_ID};
        use solana_sdk::pubkey::Pubkey;

        use super::{fixture_path, read_fixture};
        use crate::runtime::{G_CREATE_KEY, P_CREATE_KEY};

        /// squads-protocol/v4 commit the `Multisig` layout is taken from
        /// (`programs/squads_multisig_program/src/state/multisig.rs`).
        pub const PINNED_COMMIT: &str = "887627b3ac3f59548fdb6dfd1dfe23a21bb72b60";
        /// `Permissions { mask }`: Initiate (1) | Vote (2) | Execute (4).
        pub const ALL_PERMISSIONS: u8 = 7;
        /// A freshly created multisig has executed no transactions.
        pub const TRANSACTION_INDEX: u64 = 0;
        pub const STALE_TRANSACTION_INDEX: u64 = 0;
        pub const GOVERNANCE_TIME_LOCK: u32 = 172_800;
        pub const GUARDIAN_TIME_LOCK: u32 = 0;
        pub const THRESHOLD: u16 = 2;
        pub const MEMBER_COUNT: usize = 3;
        pub const GOVERNANCE_FILE: &str = "squads/multisig_g.bin";
        pub const GUARDIAN_FILE: &str = "squads/multisig_p.bin";

        /// `["multisig", "multisig", create_key]` under the Squads program.
        pub fn multisig_address(create_key: &Pubkey) -> (Pubkey, u8) {
            Pubkey::find_program_address(
                &[b"multisig", b"multisig", create_key.as_ref()],
                &SQUADS_PROGRAM_ID,
            )
        }

        /// `["multisig", multisig, "vault", [vault_index]]` under the Squads
        /// program.  The Squads vault stand-in signs with the same seeds.
        pub fn vault_address(multisig: &Pubkey, vault_index: u8) -> (Pubkey, u8) {
            Pubkey::find_program_address(
                &[b"multisig", multisig.as_ref(), b"vault", &[vault_index]],
                &SQUADS_PROGRAM_ID,
            )
        }

        /// Three distinct synthetic members for a multisig: the first byte is
        /// the `create_key` tag, the last byte is the member ordinal.
        pub fn canonical_members(create_key: &Pubkey) -> Vec<(Pubkey, u8)> {
            (1..=MEMBER_COUNT as u8)
                .map(|ordinal| {
                    let mut key = [create_key.to_bytes()[0]; 32];
                    key[31] = ordinal;
                    (Pubkey::new_from_array(key), ALL_PERMISSIONS)
                })
                .collect()
        }

        /// Serialize a `Multisig` account in the pinned Borsh layout:
        /// `discriminator[8] | create_key | config_authority (plain Pubkey) |
        /// threshold u16 | time_lock u32 | transaction_index u64 |
        /// stale_transaction_index u64 | rent_collector Option<Pubkey> (tag +
        /// 32 if Some) | bump u8 | members: u32 len + N x (key | permissions
        /// mask u8)`.  Integers are little-endian.  No allocation padding.
        pub fn multisig_bytes(
            create_key: Pubkey,
            config_authority: Pubkey,
            threshold: u16,
            time_lock: u32,
            rent_collector: Option<Pubkey>,
            bump: u8,
            members: &[(Pubkey, u8)],
        ) -> Vec<u8> {
            let mut bytes = Vec::with_capacity(8 + 32 + 32 + 2 + 4 + 8 + 8 + 33 + 1 + 4 + members.len() * 33);
            bytes.extend_from_slice(&SQUADS_MULTISIG_DISCRIMINATOR);
            bytes.extend_from_slice(create_key.as_ref());
            bytes.extend_from_slice(config_authority.as_ref());
            bytes.extend_from_slice(&threshold.to_le_bytes());
            bytes.extend_from_slice(&time_lock.to_le_bytes());
            bytes.extend_from_slice(&TRANSACTION_INDEX.to_le_bytes());
            bytes.extend_from_slice(&STALE_TRANSACTION_INDEX.to_le_bytes());
            match rent_collector {
                None => bytes.push(0),
                Some(collector) => {
                    bytes.push(1);
                    bytes.extend_from_slice(collector.as_ref());
                }
            }
            bytes.push(bump);
            bytes.extend_from_slice(&(members.len() as u32).to_le_bytes());
            for (key, mask) in members {
                bytes.extend_from_slice(key.as_ref());
                bytes.push(*mask);
            }
            bytes
        }

        /// The size Squads allocates for a multisig with `member_count`
        /// members: `Option<Pubkey>` always reserves 33 bytes, so an account
        /// whose `rent_collector` is `None` carries 32 trailing zero bytes.
        pub fn multisig_space(member_count: usize) -> usize {
            8 + 32 + 32 + 2 + 4 + 8 + 8 + 33 + 1 + 4 + member_count * 33
        }

        /// Zero-pad serialized bytes up to the allocation size (never shrinks).
        pub fn pad_to_allocation(mut bytes: Vec<u8>, member_count: usize) -> Vec<u8> {
            let space = multisig_space(member_count);
            if bytes.len() < space {
                bytes.resize(space, 0);
            }
            bytes
        }

        /// Autonomous (`config_authority` = default), no rent collector,
        /// canonical members and canonical bump, with the given threshold and
        /// time lock.
        pub fn profile_bytes(create_key: [u8; 32], time_lock: u32, threshold: u16) -> Vec<u8> {
            let create_key = Pubkey::new_from_array(create_key);
            let (_, bump) = multisig_address(&create_key);
            multisig_bytes(
                create_key,
                Pubkey::default(),
                threshold,
                time_lock,
                None,
                bump,
                &canonical_members(&create_key),
            )
        }

        /// The G profile: 2-of-3, 172,800 s time lock, `create_key` `[0x47; 32]`.
        pub fn governance_bytes() -> Vec<u8> {
            profile_bytes(G_CREATE_KEY, GOVERNANCE_TIME_LOCK, THRESHOLD)
        }

        /// The P profile: 2-of-3, no time lock, `create_key` `[0x50; 32]`.
        pub fn guardian_bytes() -> Vec<u8> {
            profile_bytes(P_CREATE_KEY, GUARDIAN_TIME_LOCK, THRESHOLD)
        }

        /// The committed `multisig_g.bin`.
        pub fn committed_governance() -> Vec<u8> {
            read_fixture(GOVERNANCE_FILE)
        }

        /// The committed `multisig_p.bin`.
        pub fn committed_guardian() -> Vec<u8> {
            read_fixture(GUARDIAN_FILE)
        }

        /// Write both committed fixtures from the generators (used by the
        /// fixture regeneration helper; the tests only compare).
        pub fn write_committed() {
            for (file, bytes) in [
                (GOVERNANCE_FILE, governance_bytes()),
                (GUARDIAN_FILE, guardian_bytes()),
            ] {
                let path = fixture_path(file);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("fixture directory");
                }
                std::fs::write(&path, bytes).expect("fixture write");
            }
        }
    }

    pub mod loader {
        use musical_chairs_program::ID;
        use solana_loader_v3_interface::state::UpgradeableLoaderState;
        use solana_sdk::pubkey::Pubkey;
        use solana_sdk_ids::bpf_loader_upgradeable;

        use super::{fixture_path, read_fixture};

        /// `ProgramData.slot` of the fixture.
        pub const SLOT: u64 = 1;
        pub const PROGRAM_FILE: &str = "loader/program.bin";
        pub const PROGRAMDATA_FILE: &str = "loader/programdata.bin";

        /// `["<main program id>"]` under the upgradeable loader.
        pub fn program_data_address() -> Pubkey {
            Pubkey::find_program_address(&[ID.as_ref()], &bpf_loader_upgradeable::id()).0
        }

        /// Serialized `UpgradeableLoaderState::Program { programdata_address }`:
        /// variant 2 (u32) then the 32-byte address, 36 bytes.
        pub fn program_header() -> Vec<u8> {
            bincode::serialize(&UpgradeableLoaderState::Program {
                programdata_address: program_data_address(),
            })
            .expect("Program header serialization")
        }

        /// Serialized `UpgradeableLoaderState::ProgramData` header: variant 3
        /// (u32), slot (u64), `Option<Pubkey>` authority (tag + 32 if Some),
        /// zero-padded to the 45-byte metadata size (so an immutable `None`
        /// program has a zero tag byte and 32 zero bytes).  The ELF follows
        /// this header in the real account; it is appended at runtime.
        pub fn programdata_header(slot: u64, authority: Option<Pubkey>) -> Vec<u8> {
            let mut bytes = bincode::serialize(&UpgradeableLoaderState::ProgramData {
                slot,
                upgrade_authority_address: authority,
            })
            .expect("ProgramData header serialization");
            bytes.resize(UpgradeableLoaderState::size_of_programdata_metadata(), 0);
            bytes
        }

        /// The committed `program.bin`.
        pub fn committed_program() -> Vec<u8> {
            read_fixture(PROGRAM_FILE)
        }

        /// The committed `programdata.bin` (authority = the G vault).
        pub fn committed_programdata() -> Vec<u8> {
            read_fixture(PROGRAMDATA_FILE)
        }

        /// The generator output for the committed `programdata.bin`.
        pub fn governance_programdata_header() -> Vec<u8> {
            let governance = super::squads::vault_address(
                &super::squads::multisig_address(&Pubkey::new_from_array(
                    crate::runtime::G_CREATE_KEY,
                ))
                .0,
                0,
            )
            .0;
            programdata_header(SLOT, Some(governance))
        }

        /// Write both committed fixtures from the generators.
        pub fn write_committed() {
            for (file, bytes) in [
                (PROGRAM_FILE, program_header()),
                (PROGRAMDATA_FILE, governance_programdata_header()),
            ] {
                let path = fixture_path(file);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("fixture directory");
                }
                std::fs::write(&path, bytes).expect("fixture write");
            }
        }
    }
}
