//! One pass of the worker (`once`): reconcile what may already be on the
//! network, plan the next step, simulate it, sign, journal, send.
//!
//! The order inside a pass is the safety argument:
//!
//! 1. READ block height, THEN signature statuses of every unresolved journal
//!    entry (height first, so an unseen signature with the height already past
//!    its validity can no longer land).  If either read fails the pass sends
//!    NOTHING: an outcome that cannot be read is not an outcome.
//! 2. Entries the cluster has decided become `confirmed` / `finalized` /
//!    `failed`; unseen and past validity become `expired`; unseen and still
//!    valid are re-broadcast with the IDENTICAL signed bytes.
//! 3. Plan the next step from a fresh snapshot.  If any entry for that step is
//!    still unresolved the pass stops there ([`Journal::may_submit`]).
//! 4. Simulate (one-row fallback at most once), build `[compute budget,
//!    program instruction]`, check it is fee-only, sign, write the journal
//!    line, THEN send.  A preflight rejection is `failed`; an ambiguous send
//!    error is `unknown`.

use std::{fmt, str::FromStr};

use serde::Serialize;
use solana_sdk::{
    message::Message, pubkey::Pubkey, signature::Signature, signer::Signer,
    transaction::Transaction,
};

use crate::{
    hex_decode, hex_encode,
    journal::{JStatus, Journal, JournalEntry, JournalError},
    metrics::{marker_of, AlertLevel, MetricsReport, Progress},
    plan::{
        assert_fee_only, ensure_budget, next_step, Budgeted, Limits, PlanError, Simulator, Step,
        SIMULATION_CU,
    },
    rpc::{Chain, ChainReader, RpcError, SendError, SimOutcome},
    sha256_hex,
    state::{load_snapshot, Snapshot, StateError},
};

/// How the node answered the send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SendOutcome {
    /// The node accepted the bytes.
    Accepted,
    /// The send died ambiguously; the journal entry is `unknown`.
    Unknown,
    /// Preflight refused the transaction; nothing was broadcast.
    Rejected,
}

/// What a pass did about the program's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Nothing to advance.
    Idle,
    /// Work is pending but no step can be sent now (waiting for the cash commit or the provider,
    /// or the simulation refused the step deterministically).
    Blocked { reason: String },
    /// The status path (block height or signature statuses) could not be read: nothing was sent.
    StatusUnavailable { reason: String },
    /// The planned step already has an unresolved journal entry: nothing was sent.
    WaitingOnUnresolved { step: String },
    /// `--dry-run`: the step was planned and simulated, not signed.
    DryRun { step: String, rows: u8 },
    Sent {
        step: String,
        rows: u8,
        signature: String,
        outcome: SendOutcome,
    },
}

impl Action {
    /// The variant's name, for the metrics document.
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Idle => "idle",
            Action::Blocked { .. } => "blocked",
            Action::StatusUnavailable { .. } => "status_unavailable",
            Action::WaitingOnUnresolved { .. } => "waiting_on_unresolved",
            Action::DryRun { .. } => "dry_run",
            Action::Sent { .. } => "sent",
        }
    }
}

/// A journal entry whose status changed during reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reconciled {
    pub signature: String,
    pub status: JStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PassReport {
    pub now: u64,
    pub action: Action,
    pub reconciled: Vec<Reconciled>,
    /// Signatures whose identical signed bytes were re-broadcast this pass.
    pub rebroadcast: Vec<String>,
    pub marker: String,
    pub pending_work: bool,
    pub alert: AlertLevel,
    pub seconds_since_progress: u64,
}

#[derive(Debug)]
pub enum KeeperError {
    Rpc(RpcError),
    State(StateError),
    Plan(PlanError),
    Journal(JournalError),
    Signing(String),
}

impl fmt::Display for KeeperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeeperError::Rpc(e) => write!(f, "{e}"),
            KeeperError::State(e) => write!(f, "{e}"),
            KeeperError::Plan(e) => write!(f, "{e}"),
            KeeperError::Journal(e) => write!(f, "{e}"),
            KeeperError::Signing(e) => write!(f, "signing failed: {e}"),
        }
    }
}

impl std::error::Error for KeeperError {}

impl From<RpcError> for KeeperError {
    fn from(error: RpcError) -> Self {
        KeeperError::Rpc(error)
    }
}
impl From<StateError> for KeeperError {
    fn from(error: StateError) -> Self {
        KeeperError::State(error)
    }
}
impl From<PlanError> for KeeperError {
    fn from(error: PlanError) -> Self {
        KeeperError::Plan(error)
    }
}
impl From<JournalError> for KeeperError {
    fn from(error: JournalError) -> Self {
        KeeperError::Journal(error)
    }
}

/// Simulates a step under the compute-budget ceiling (so the units are measured, not capped) as the
/// unsigned transaction the node would run: signature verification off, blockhash replaced.
struct ChainSimulator<'a, C: ChainReader> {
    chain: &'a C,
    fee_payer: Pubkey,
}

impl<C: ChainReader> Simulator for ChainSimulator<'_, C> {
    fn simulate_step(&self, step: &Step) -> Result<SimOutcome, RpcError> {
        let instructions = step.instructions_with_cu(&self.fee_payer, SIMULATION_CU);
        let tx = Transaction::new_unsigned(Message::new(&instructions, Some(&self.fee_payer)));
        self.chain.simulate(&tx, &step.label())
    }
}

/// The worker.  The fee payer is ANY signer; the worker never holds or needs another key.
pub struct Worker<'a, C: Chain> {
    chain: &'a C,
    fee_payer: &'a dyn Signer,
    journal: Journal,
    limits: Limits,
    dry_run: bool,
    progress: Progress,
}

impl<'a, C: Chain> Worker<'a, C> {
    pub fn new(chain: &'a C, fee_payer: &'a dyn Signer, journal: Journal, limits: Limits) -> Self {
        Self {
            chain,
            fee_payer,
            journal,
            limits,
            dry_run: false,
            progress: Progress::new(0),
        }
    }

    /// `--dry-run`: plan and simulate, never sign or send.
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Resume progress tracking from a previous run (the `--metrics-json` file).
    pub fn with_progress(mut self, progress: Progress) -> Self {
        self.progress = progress;
        self
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// One reconcile / plan / send pass at wall-clock `now` (unix seconds).
    pub fn once(&mut self, now: u64) -> Result<PassReport, KeeperError> {
        let mut reconciled = Vec::new();
        let mut rebroadcast = Vec::new();

        // 1 + 2. What may already be on the network.
        let unresolved = self.journal.unresolved();
        let status_unavailable = if unresolved.is_empty() {
            None
        } else {
            self.reconcile(now, &unresolved, &mut reconciled, &mut rebroadcast)?
        };

        // The snapshot feeds the planner and the progress marker.  When the status path is down nothing is
        // planned, so a failed read is then only a missing marker, not a failed pass.
        let snapshot = match load_snapshot(self.chain) {
            Ok(snapshot) => Some(snapshot),
            Err(_) if status_unavailable.is_some() => None,
            Err(error) => return Err(error.into()),
        };
        let (marker_text, pending_work) = match &snapshot {
            Some(snapshot) => {
                let marker = marker_of(snapshot);
                self.progress
                    .observe(now, &marker.text, marker.pending_work);
                (marker.text, marker.pending_work)
            }
            None => ("unavailable".to_string(), self.progress.pending_work),
        };

        let action = match (&status_unavailable, &snapshot) {
            (Some(reason), _) => Action::StatusUnavailable {
                reason: reason.clone(),
            },
            (None, Some(snapshot)) => {
                self.plan_and_send(now, snapshot, pending_work, &marker_text)?
            }
            // Cannot happen: a failed snapshot read returns early unless the status path is down.
            (None, None) => return Err(StateError::Missing("snapshot".to_string()).into()),
        };

        Ok(PassReport {
            now,
            action,
            reconciled,
            rebroadcast,
            marker: marker_text,
            pending_work,
            alert: self.progress.alert_level(now),
            seconds_since_progress: self.progress.seconds_since_progress(now),
        })
    }

    /// Read block height THEN signature statuses of the unresolved entries, settle what the cluster has
    /// decided, expire what can no longer land and re-broadcast the rest with the identical signed bytes.
    /// Returns the reason when the status path could not be read (nothing is then re-broadcast or planned).
    fn reconcile(
        &mut self,
        now: u64,
        unresolved: &[JournalEntry],
        reconciled: &mut Vec<Reconciled>,
        rebroadcast: &mut Vec<String>,
    ) -> Result<Option<String>, KeeperError> {
        let signatures: Vec<Signature> = unresolved
            .iter()
            .map(|entry| {
                Signature::from_str(&entry.signature).map_err(|e| {
                    JournalError::Corrupt(format!("signature {}: {e}", entry.signature))
                })
            })
            .collect::<Result<_, _>>()?;

        // Height before status: an unseen signature with the height already past its validity can no longer land.
        let height = self.chain.block_height();
        let statuses = self.chain.signature_statuses(&signatures);
        let (height, statuses) = match (height, statuses) {
            (Ok(height), Ok(statuses)) if statuses.len() == unresolved.len() => (height, statuses),
            (Ok(_), Ok(_)) => {
                return Ok(Some(
                    "the cluster answered the wrong number of statuses".to_string(),
                ))
            }
            (Err(error), _) | (_, Err(error)) => return Ok(Some(error.to_string())),
        };

        let mut to_rebroadcast = Vec::new();
        for (entry, status) in unresolved.iter().zip(statuses) {
            let decided = match status {
                Some(status) if status.err.is_some() => Some((JStatus::Failed, status.err)),
                Some(status) if status.finalized => Some((JStatus::Finalized, None)),
                Some(status) if status.confirmed => Some((JStatus::Confirmed, None)),
                // Seen but only processed: the cluster has it; keep waiting.
                Some(_) => None,
                None if height > entry.last_valid_block_height => Some((
                    JStatus::Expired,
                    Some(format!("not seen by block height {height}")),
                )),
                None => {
                    to_rebroadcast.push(entry.clone());
                    None
                }
            };
            if let Some((status, note)) = decided {
                self.journal
                    .update_status(&entry.signature, status, now, note)?;
                reconciled.push(Reconciled {
                    signature: entry.signature.clone(),
                    status,
                });
            }
        }

        for entry in to_rebroadcast {
            let bytes = hex_decode(&entry.tx_hex)
                .filter(|bytes| sha256_hex(bytes) == entry.bytes_sha256)
                .ok_or_else(|| {
                    JournalError::Corrupt(format!(
                        "signed bytes of {} do not match their digest",
                        entry.signature
                    ))
                })?;
            // The identical bytes: same blockhash, same signature, so the cluster deduplicates. Whatever the
            // node answers, the NEXT status read decides; this is never a new transaction.
            let _ = self.chain.send_raw(&bytes, true);
            rebroadcast.push(entry.signature);
        }
        Ok(None)
    }

    fn plan_and_send(
        &mut self,
        now: u64,
        snapshot: &Snapshot,
        pending_work: bool,
        marker: &str,
    ) -> Result<Action, KeeperError> {
        let Some(step) = next_step(snapshot, &self.limits)? else {
            return Ok(if pending_work {
                Action::Blocked {
                    reason: format!("waiting on the chain or the provider ({marker})"),
                }
            } else {
                Action::Idle
            });
        };
        let key = step.key();
        if !self.journal.may_submit(&key) {
            return Ok(Action::WaitingOnUnresolved { step: key });
        }

        let fee_payer = self.fee_payer.pubkey();
        let simulator = ChainSimulator {
            chain: self.chain,
            fee_payer,
        };
        let step = match ensure_budget(step, &simulator)? {
            Budgeted::Ready { step, .. } => step,
            Budgeted::Blocked { reason, .. } => {
                return Ok(Action::Blocked {
                    reason: format!("{key}: {reason}"),
                })
            }
        };
        if self.dry_run {
            return Ok(Action::DryRun {
                step: key,
                rows: step.rows,
            });
        }
        self.sign_journal_send(now, &step, &key)
    }

    /// Build `[compute budget, program instruction]`, check it is fee-only, sign, journal, then send.
    fn sign_journal_send(
        &mut self,
        now: u64,
        step: &Step,
        key: &str,
    ) -> Result<Action, KeeperError> {
        let fee_payer = self.fee_payer.pubkey();
        let (blockhash, last_valid_block_height) = self.chain.latest_blockhash()?;
        let instructions = step.instructions(&fee_payer);
        assert_fee_only(&instructions, &fee_payer)?;

        let mut tx = Transaction::new_unsigned(Message::new_with_blockhash(
            &instructions,
            Some(&fee_payer),
            &blockhash,
        ));
        if tx.message.header.num_required_signatures != 1 {
            return Err(KeeperError::Signing(format!(
                "the transaction needs {} signatures; the fee payer is the only key the worker holds",
                tx.message.header.num_required_signatures
            )));
        }
        let signature = self
            .fee_payer
            .try_sign_message(&tx.message_data())
            .map_err(|e| KeeperError::Signing(e.to_string()))?;
        tx.signatures[0] = signature;
        let bytes = bincode::serialize(&tx).map_err(|e| KeeperError::Signing(e.to_string()))?;

        // Write-ahead: the journal knows about the bytes before the network can.
        self.journal.record_submitted(JournalEntry {
            signature: signature.to_string(),
            blockhash: blockhash.to_string(),
            last_valid_block_height,
            step: key.to_string(),
            bytes_sha256: sha256_hex(&bytes),
            tx_hex: hex_encode(&bytes),
            status: JStatus::Submitted,
            at: now,
            note: None,
        })?;

        let outcome = match self.chain.send_raw(&bytes, false) {
            Ok(_) => SendOutcome::Accepted,
            Err(SendError::Rejected(reason)) => {
                self.journal.update_status(
                    &signature.to_string(),
                    JStatus::Failed,
                    now,
                    Some(reason),
                )?;
                SendOutcome::Rejected
            }
            Err(SendError::Unknown(reason)) => {
                self.journal.update_status(
                    &signature.to_string(),
                    JStatus::Unknown,
                    now,
                    Some(reason),
                )?;
                SendOutcome::Unknown
            }
        };
        Ok(Action::Sent {
            step: key.to_string(),
            rows: step.rows,
            signature: signature.to_string(),
            outcome,
        })
    }

    /// The metrics document for the pass that just ran.
    pub fn metrics(&self, now: u64, report: &PassReport) -> MetricsReport {
        MetricsReport {
            now,
            last_progress_at: self.progress.last_progress_at,
            last_progress_marker: self.progress.last_progress_marker.clone(),
            seconds_since_progress: self.progress.seconds_since_progress(now),
            alert_level: report.alert,
            pending_work: self.progress.pending_work,
            unresolved_journal_entries: self.journal.unresolved().len(),
            last_action: report.action.kind().to_string(),
            dry_run: self.dry_run,
        }
    }
}
