//! Chain access: the traits the worker is written against, the real
//! `RpcClient` backend and the scripted [`MockChain`] the tests use.
//!
//! Two traits, split the way the failure modes split:
//!
//! - [`ChainReader`]: reads (accounts, blockhash, block height, simulation,
//!   signature statuses).  A failure is an [`RpcError`]: "no information", never
//!   a verdict on the transaction.
//! - [`ChainWriter`]: one operation, `send_raw` of already-signed bytes.  A
//!   failure is a [`SendError`], classified exactly like the browser client's
//!   `classifySendError`: a preflight/simulation rejection means the node
//!   refused the transaction ([`SendError::Rejected`], nothing was broadcast);
//!   anything else after the attempt is [`SendError::Unknown`] (the node may
//!   or may not have it).

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::Mutex,
};

use solana_rpc_client::rpc_client::RpcClient;
use solana_rpc_client_api::{
    client_error::{Error as ClientError, ErrorKind as ClientErrorKind},
    config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig},
    request::{RpcError as RpcApiError, RpcResponseErrorData},
};
use solana_sdk::{
    account::Account,
    clock::Clock,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    hash::Hash,
    instruction::InstructionError,
    packet::PACKET_DATA_SIZE,
    pubkey::Pubkey,
    signature::Signature,
    sysvar,
    transaction::{Transaction, TransactionError},
};

use musical_chairs_program::ErrorCode as ProgramError;

// ---------------------------------------------------------------------------
// Errors and outcomes
// ---------------------------------------------------------------------------

/// A read failed: the RPC could not answer.  This says nothing about any
/// transaction, so the worker never treats it as "the transaction failed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    Unavailable(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::Unavailable(reason) => write!(f, "rpc unavailable: {reason}"),
        }
    }
}

impl std::error::Error for RpcError {}

/// Outcome of a failed send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// The node refused the transaction at preflight: it was NOT broadcast.
    Rejected(String),
    /// Anything else after the attempt (timeout, dropped connection, 5xx):
    /// the transaction may or may not be on its way to a leader.
    Unknown(String),
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Rejected(reason) => write!(f, "rejected before broadcast: {reason}"),
            SendError::Unknown(reason) => write!(f, "outcome unknown: {reason}"),
        }
    }
}

impl std::error::Error for SendError {}

/// What kind of failure a simulation reported (port of `classifySimulationFailure`).
/// The shrinkable kinds are the ones a smaller batch can cure; every other kind
/// is deterministic for the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimFailureKind {
    /// `PageNotProvisioned`: the batch ran into a page the keeper did not supply.
    Page,
    /// Compute budget or instruction-trace exhaustion.
    Budget,
    /// The assembled transaction does not fit one packet.
    Size,
    /// `DuplicateWritableAccount`: two rows of the batch share a writable account.
    Batch,
    /// Any other program error (for example the randomness is not fulfilled yet).
    Program,
    Unknown,
}

impl SimFailureKind {
    pub fn is_shrinkable(self) -> bool {
        matches!(self, Self::Page | Self::Budget | Self::Size | Self::Batch)
    }
}

/// Result of simulating one unsigned transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimOutcome {
    /// The transaction would succeed and consume `units` compute units.
    Ok { units: u64 },
    Failed {
        kind: SimFailureKind,
        reason: String,
    },
}

/// Which step a simulation is for.  The real backend ignores it (it simulates
/// the transaction it is given); [`MockChain`] scripts outcomes by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimLabel {
    pub name: &'static str,
    pub rows: u8,
}

/// Status of one signature as the cluster reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigStatus {
    pub slot: u64,
    /// `Some` when the transaction landed and failed (state rolled back).
    pub err: Option<String>,
    pub confirmed: bool,
    pub finalized: bool,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

pub trait ChainReader {
    /// Accounts in `keys` order; `None` for an account that does not exist.
    fn get_multiple_accounts(&self, keys: &[Pubkey]) -> Result<Vec<Option<Account>>, RpcError>;
    /// A recent blockhash and the last block height at which it is valid.
    fn latest_blockhash(&self) -> Result<(Hash, u64), RpcError>;
    /// The FINALIZED block height (conservative for expiry decisions).
    fn block_height(&self) -> Result<u64, RpcError>;
    /// Simulate the unsigned transaction: signature verification off, recent
    /// blockhash replaced.
    fn simulate(&self, tx: &Transaction, label: &SimLabel) -> Result<SimOutcome, RpcError>;
    /// Statuses in `signatures` order; `None` when the cluster has not seen it.
    fn signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> Result<Vec<Option<SigStatus>>, RpcError>;
}

pub trait ChainWriter {
    /// Send already-signed transaction bytes.  `skip_preflight` is set for a
    /// re-broadcast of bytes that already passed preflight once.
    fn send_raw(&self, bytes: &[u8], skip_preflight: bool) -> Result<Signature, SendError>;
}

/// Everything the worker needs from the chain.
pub trait Chain: ChainReader + ChainWriter {}
impl<T: ChainReader + ChainWriter + ?Sized> Chain for T {}

// ---------------------------------------------------------------------------
// Real backend
// ---------------------------------------------------------------------------

/// `ChainReader` + `ChainWriter` over the blocking `solana_rpc_client::RpcClient`.
pub struct RpcClientBackend {
    client: RpcClient,
    commitment: CommitmentConfig,
}

impl RpcClientBackend {
    pub fn new(url: &str) -> Self {
        let commitment = CommitmentConfig::confirmed();
        Self {
            client: RpcClient::new_with_commitment(url.to_string(), commitment),
            commitment,
        }
    }
}

/// `getMultipleAccounts` accepts at most 100 keys per call.
const MAX_ACCOUNTS_PER_CALL: usize = 100;

fn unavailable(error: ClientError) -> RpcError {
    RpcError::Unavailable(error.to_string())
}

impl ChainReader for RpcClientBackend {
    fn get_multiple_accounts(&self, keys: &[Pubkey]) -> Result<Vec<Option<Account>>, RpcError> {
        let mut out = Vec::with_capacity(keys.len());
        for chunk in keys.chunks(MAX_ACCOUNTS_PER_CALL) {
            let response = self
                .client
                .get_multiple_accounts_with_commitment(chunk, self.commitment)
                .map_err(unavailable)?;
            out.extend(response.value);
        }
        Ok(out)
    }

    fn latest_blockhash(&self) -> Result<(Hash, u64), RpcError> {
        self.client
            .get_latest_blockhash_with_commitment(self.commitment)
            .map_err(unavailable)
    }

    fn block_height(&self) -> Result<u64, RpcError> {
        self.client
            .get_block_height_with_commitment(CommitmentConfig::finalized())
            .map_err(unavailable)
    }

    fn simulate(&self, tx: &Transaction, _label: &SimLabel) -> Result<SimOutcome, RpcError> {
        // A transaction that cannot fit one packet is decided locally.
        let size = bincode::serialized_size(tx).map_err(|e| RpcError::Unavailable(e.to_string()))?
            as usize;
        if size > PACKET_DATA_SIZE {
            return Ok(SimOutcome::Failed {
                kind: SimFailureKind::Size,
                reason: format!("the transaction does not fit one packet ({size} bytes, limit {PACKET_DATA_SIZE})"),
            });
        }
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: true,
            commitment: Some(self.commitment),
            ..RpcSimulateTransactionConfig::default()
        };
        let response = self
            .client
            .simulate_transaction_with_config(tx, config)
            .map_err(unavailable)?;
        let result = response.value;
        match result.err {
            None => Ok(SimOutcome::Ok {
                units: result.units_consumed.unwrap_or(0),
            }),
            Some(err) => Ok(classify_simulation_failure(
                &err,
                result.logs.as_deref().unwrap_or(&[]),
            )),
        }
    }

    fn signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> Result<Vec<Option<SigStatus>>, RpcError> {
        let response = self
            .client
            .get_signature_statuses(signatures)
            .map_err(unavailable)?;
        Ok(response
            .value
            .into_iter()
            .map(|status| {
                status.map(|s| SigStatus {
                    slot: s.slot,
                    err: s.err.as_ref().map(|e| e.to_string()),
                    confirmed: s.satisfies_commitment(CommitmentConfig::confirmed()),
                    finalized: s.satisfies_commitment(CommitmentConfig::finalized()),
                })
            })
            .collect())
    }
}

impl ChainWriter for RpcClientBackend {
    fn send_raw(&self, bytes: &[u8], skip_preflight: bool) -> Result<Signature, SendError> {
        let tx: Transaction = bincode::deserialize(bytes).map_err(|e| {
            SendError::Rejected(format!("the signed bytes are not a transaction: {e}"))
        })?;
        let config = RpcSendTransactionConfig {
            skip_preflight,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            ..RpcSendTransactionConfig::default()
        };
        self.client
            .send_transaction_with_config(&tx, config)
            .map_err(|e| classify_send_error(&e))
    }
}

/// Port of the browser client's `classifySendError`: a preflight/simulation
/// failure is `Rejected`; everything else after the attempt is `Unknown`.
pub fn classify_send_error(error: &ClientError) -> SendError {
    match error.kind() {
        ClientErrorKind::RpcError(RpcApiError::RpcResponseError {
            data: RpcResponseErrorData::SendTransactionPreflightFailure(_),
            ..
        })
        | ClientErrorKind::TransactionError(_) => SendError::Rejected(error.to_string()),
        _ => SendError::Unknown(error.to_string()),
    }
}

fn program_code(error: ProgramError) -> u32 {
    u32::from(error)
}

/// Port of `classifySimulationFailure`: map the cluster's error value (and the
/// logs, for the compute meter) to a [`SimFailureKind`].  Never panics.
pub fn classify_simulation_failure(err: &TransactionError, logs: &[String]) -> SimOutcome {
    let log_text = logs.join("\n");
    let kind = match err {
        TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
            if *code == program_code(ProgramError::PageNotProvisioned) {
                SimFailureKind::Page
            } else if *code == program_code(ProgramError::DuplicateWritableAccount) {
                SimFailureKind::Batch
            } else {
                SimFailureKind::Program
            }
        }
        TransactionError::InstructionError(
            _,
            InstructionError::ComputationalBudgetExceeded
            | InstructionError::MaxInstructionTraceLengthExceeded
            | InstructionError::ProgramFailedToComplete
            | InstructionError::CallDepth,
        ) => SimFailureKind::Budget,
        _ if log_text.contains("exceeded CUs meter")
            || log_text.contains("computational budget exceeded") =>
        {
            SimFailureKind::Budget
        }
        _ => SimFailureKind::Unknown,
    };
    SimOutcome::Failed {
        kind,
        reason: err.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Scripted mock
// ---------------------------------------------------------------------------

/// What the mock does with the next `send_raw`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendBehavior {
    /// The node accepts the bytes.
    Accept,
    /// Preflight refuses them: not broadcast.
    Reject(String),
    /// The bytes may have reached the network but the answer was lost.
    NetworkErrorAfterSend(String),
}

/// What the mock answers for one signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusAnswer {
    NotFound,
    Confirmed,
    Finalized,
    Failed(String),
}

/// One `send_raw` attempt as the mock saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentTx {
    pub bytes: Vec<u8>,
    pub signature: Signature,
    pub skip_preflight: bool,
}

struct MockInner {
    accounts: HashMap<Pubkey, Account>,
    block_height: u64,
    blockhash: Hash,
    last_valid_block_height: u64,
    scripted_simulations: HashMap<(String, u8), SimOutcome>,
    default_simulation: SimOutcome,
    sends: VecDeque<SendBehavior>,
    statuses: HashMap<Signature, StatusAnswer>,
    status_unavailable: bool,
    sent: Vec<SentTx>,
    simulated: Vec<SimLabel>,
    status_reads: usize,
}

/// A scripted [`Chain`]: a map of accounts, scripted simulation results per
/// `(instruction name, rows)`, a queue of send behaviors (including the
/// "network error after send" case) and per-signature status answers.  Every
/// simulation and every send attempt is recorded for the test to inspect.
pub struct MockChain {
    inner: Mutex<MockInner>,
}

impl Default for MockChain {
    fn default() -> Self {
        Self::new()
    }
}

impl MockChain {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(MockInner {
                accounts: HashMap::new(),
                block_height: 1_000,
                blockhash: Hash::new_from_array([7; 32]),
                last_valid_block_height: 1_150,
                scripted_simulations: HashMap::new(),
                default_simulation: SimOutcome::Ok { units: 10_000 },
                sends: VecDeque::new(),
                statuses: HashMap::new(),
                status_unavailable: false,
                sent: Vec::new(),
                simulated: Vec::new(),
                status_reads: 0,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MockInner> {
        self.inner.lock().expect("mock chain lock")
    }

    pub fn set_account(&self, key: Pubkey, account: Account) {
        self.lock().accounts.insert(key, account);
    }

    pub fn remove_account(&self, key: &Pubkey) {
        self.lock().accounts.remove(key);
    }

    /// Write the Clock sysvar account (unix timestamp and slot).
    pub fn set_clock(&self, unix_timestamp: i64, slot: u64) {
        let clock = Clock {
            slot,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp,
        };
        let account = Account {
            lamports: 1,
            data: bincode::serialize(&clock).expect("clock serializes"),
            owner: sysvar::ID,
            executable: false,
            rent_epoch: 0,
        };
        self.set_account(sysvar::clock::ID, account);
    }

    pub fn set_block_height(&self, height: u64) {
        self.lock().block_height = height;
    }

    pub fn set_blockhash(&self, blockhash: Hash, last_valid_block_height: u64) {
        let mut inner = self.lock();
        inner.blockhash = blockhash;
        inner.last_valid_block_height = last_valid_block_height;
    }

    /// Script the simulation of instruction `name` carrying `rows` rows.
    pub fn script_simulation(&self, name: &str, rows: u8, outcome: SimOutcome) {
        self.lock()
            .scripted_simulations
            .insert((name.to_string(), rows), outcome);
    }

    /// What an unscripted simulation answers (default: `Ok { units: 10_000 }`).
    pub fn set_default_simulation(&self, outcome: SimOutcome) {
        self.lock().default_simulation = outcome;
    }

    /// Queue the behavior of the next `send_raw` (an empty queue accepts).
    pub fn script_send(&self, behavior: SendBehavior) {
        self.lock().sends.push_back(behavior);
    }

    /// Answer for one signature (an unscripted signature is `NotFound`).
    pub fn script_status(&self, signature: Signature, answer: StatusAnswer) {
        self.lock().statuses.insert(signature, answer);
    }

    /// When set, `signature_statuses` and `block_height` fail: the status path is down.
    pub fn set_status_unavailable(&self, unavailable: bool) {
        self.lock().status_unavailable = unavailable;
    }

    /// Every `send_raw` attempt, in order (re-broadcasts included).
    pub fn sent(&self) -> Vec<SentTx> {
        self.lock().sent.clone()
    }

    /// Every simulation, in order.
    pub fn simulated(&self) -> Vec<SimLabel> {
        self.lock().simulated.clone()
    }

    /// How many `signature_statuses` calls were answered or refused.
    pub fn status_reads(&self) -> usize {
        self.lock().status_reads
    }

    pub fn last_valid_block_height(&self) -> u64 {
        self.lock().last_valid_block_height
    }
}

impl ChainReader for MockChain {
    fn get_multiple_accounts(&self, keys: &[Pubkey]) -> Result<Vec<Option<Account>>, RpcError> {
        let inner = self.lock();
        Ok(keys
            .iter()
            .map(|key| inner.accounts.get(key).cloned())
            .collect())
    }

    fn latest_blockhash(&self) -> Result<(Hash, u64), RpcError> {
        let inner = self.lock();
        Ok((inner.blockhash, inner.last_valid_block_height))
    }

    fn block_height(&self) -> Result<u64, RpcError> {
        let inner = self.lock();
        if inner.status_unavailable {
            return Err(RpcError::Unavailable(
                "block height unavailable (scripted)".to_string(),
            ));
        }
        Ok(inner.block_height)
    }

    fn simulate(&self, _tx: &Transaction, label: &SimLabel) -> Result<SimOutcome, RpcError> {
        let mut inner = self.lock();
        inner.simulated.push(label.clone());
        let scripted = inner
            .scripted_simulations
            .get(&(label.name.to_string(), label.rows))
            .cloned();
        Ok(scripted.unwrap_or_else(|| inner.default_simulation.clone()))
    }

    fn signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> Result<Vec<Option<SigStatus>>, RpcError> {
        let mut inner = self.lock();
        inner.status_reads += 1;
        if inner.status_unavailable {
            return Err(RpcError::Unavailable(
                "signature statuses unavailable (scripted)".to_string(),
            ));
        }
        Ok(signatures
            .iter()
            .map(|signature| match inner.statuses.get(signature) {
                None | Some(StatusAnswer::NotFound) => None,
                Some(StatusAnswer::Confirmed) => Some(SigStatus {
                    slot: 1,
                    err: None,
                    confirmed: true,
                    finalized: false,
                }),
                Some(StatusAnswer::Finalized) => Some(SigStatus {
                    slot: 1,
                    err: None,
                    confirmed: true,
                    finalized: true,
                }),
                Some(StatusAnswer::Failed(reason)) => Some(SigStatus {
                    slot: 1,
                    err: Some(reason.clone()),
                    confirmed: true,
                    finalized: false,
                }),
            })
            .collect())
    }
}

impl ChainWriter for MockChain {
    fn send_raw(&self, bytes: &[u8], skip_preflight: bool) -> Result<Signature, SendError> {
        let tx: Transaction = bincode::deserialize(bytes).map_err(|e| {
            SendError::Rejected(format!("the signed bytes are not a transaction: {e}"))
        })?;
        let signature = *tx
            .signatures
            .first()
            .ok_or_else(|| SendError::Rejected("unsigned transaction".to_string()))?;
        let mut inner = self.lock();
        inner.sent.push(SentTx {
            bytes: bytes.to_vec(),
            signature,
            skip_preflight,
        });
        match inner.sends.pop_front().unwrap_or(SendBehavior::Accept) {
            SendBehavior::Accept => Ok(signature),
            SendBehavior::Reject(reason) => Err(SendError::Rejected(reason)),
            SendBehavior::NetworkErrorAfterSend(reason) => Err(SendError::Unknown(reason)),
        }
    }
}
