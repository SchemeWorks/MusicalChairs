//! The append-only JSON-lines journal and the "never resubmit an unknown
//! outcome" gate.
//!
//! Every transaction the worker signs is written BEFORE it is sent (status
//! `submitted`), with its blockhash, validity bound, the SHA-256 of the signed
//! bytes and the bytes themselves.  A later line for the same signature is a
//! status update (`unknown`, `confirmed`, `finalized`, `failed`, `expired`);
//! the effective state of a signature is its LAST line.  The file is never
//! rewritten, so a crash at any point leaves a journal that says exactly what
//! may be on the network.
//!
//! Rules enforced here and by the runner:
//! - [`Journal::may_submit`] is false while ANY entry for the same step is
//!   `submitted` or `unknown`;
//! - a re-broadcast of such an entry sends the IDENTICAL signed bytes (same
//!   blockhash, same signature) from `tx_hex`;
//! - a new blockhash is taken only after the entry is `expired` (block height
//!   past `last_valid_block_height`, read BEFORE the signature status).

use std::{
    fmt, fs,
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JStatus {
    /// Signed and journaled, handed to the node (or about to be).
    Submitted,
    /// The send died ambiguously: may or may not be on the network.
    Unknown,
    Confirmed,
    Finalized,
    /// Rejected at preflight (never broadcast) or landed and failed (state rolled back).
    Failed,
    /// The blockhash is past its last valid block height and the signature was never seen.
    Expired,
}

impl JStatus {
    /// `submitted` and `unknown`: the outcome is not known yet.
    pub fn is_unresolved(self) -> bool {
        matches!(self, JStatus::Submitted | JStatus::Unknown)
    }
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub signature: String,
    pub blockhash: String,
    pub last_valid_block_height: u64,
    /// The step key ([`crate::plan::Step::key`]).
    pub step: String,
    /// Lower-case hex SHA-256 of the signed transaction bytes.
    pub bytes_sha256: String,
    /// The signed transaction bytes (hex): re-broadcast sends exactly these.
    pub tx_hex: String,
    pub status: JStatus,
    /// Unix seconds when this line was written.
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    Io(String),
    /// A line that is not a journal entry.
    Corrupt(String),
    /// A status update for a signature the journal never recorded.
    UnknownSignature(String),
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(e) => write!(f, "journal i/o: {e}"),
            JournalError::Corrupt(e) => write!(f, "journal line is not an entry: {e}"),
            JournalError::UnknownSignature(s) => {
                write!(f, "journal has no entry for signature {s}")
            }
        }
    }
}

impl std::error::Error for JournalError {}

/// Where journal lines live.
pub trait JournalSink {
    fn read_lines(&self) -> io::Result<Vec<String>>;
    fn append_line(&mut self, line: &str) -> io::Result<()>;
}

/// `--journal <path>`: one JSON object per line, appended and fsynced.
pub struct FileSink {
    path: PathBuf,
}

impl FileSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl JournalSink for FileSink {
    fn read_lines(&self) -> io::Result<Vec<String>> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn append_line(&mut self, line: &str) -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_data()
    }
}

/// In-memory sink for tests; clones share the same lines so a test can keep a handle.
#[derive(Clone, Default)]
pub struct MemSink {
    lines: Arc<Mutex<Vec<String>>>,
}

impl MemSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().expect("mem sink lock").clone()
    }
}

impl JournalSink for MemSink {
    fn read_lines(&self) -> io::Result<Vec<String>> {
        Ok(self.lines())
    }

    fn append_line(&mut self, line: &str) -> io::Result<()> {
        self.lines
            .lock()
            .expect("mem sink lock")
            .push(line.to_string());
        Ok(())
    }
}

/// The journal: the latest state of every signature ever recorded, in first-seen order.
pub struct Journal {
    sink: Box<dyn JournalSink>,
    latest: Vec<JournalEntry>,
}

/// The last line of a signature wins; a signature seen for the first time keeps its place in the order.
fn upsert(latest: &mut Vec<JournalEntry>, entry: JournalEntry) {
    match latest
        .iter_mut()
        .find(|existing| existing.signature == entry.signature)
    {
        Some(existing) => *existing = entry,
        None => latest.push(entry),
    }
}

impl Journal {
    /// Replay the sink: the last line of each signature wins.
    pub fn open(sink: Box<dyn JournalSink>) -> Result<Self, JournalError> {
        let lines = sink
            .read_lines()
            .map_err(|e| JournalError::Io(e.to_string()))?;
        let mut latest = Vec::new();
        for (number, line) in lines.iter().enumerate() {
            let entry: JournalEntry = serde_json::from_str(line)
                .map_err(|e| JournalError::Corrupt(format!("line {}: {e}", number + 1)))?;
            upsert(&mut latest, entry);
        }
        Ok(Self { sink, latest })
    }

    fn append(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        let line =
            serde_json::to_string(&entry).map_err(|e| JournalError::Corrupt(e.to_string()))?;
        self.sink
            .append_line(&line)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        upsert(&mut self.latest, entry);
        Ok(())
    }

    /// Append a freshly signed entry (status `submitted`) BEFORE it is sent.
    pub fn record_submitted(&mut self, mut entry: JournalEntry) -> Result<(), JournalError> {
        entry.status = JStatus::Submitted;
        self.append(entry)
    }

    /// Append a status line for a recorded signature.
    pub fn update_status(
        &mut self,
        signature: &str,
        status: JStatus,
        at: u64,
        note: Option<String>,
    ) -> Result<(), JournalError> {
        let mut entry = self
            .entry(signature)
            .cloned()
            .ok_or_else(|| JournalError::UnknownSignature(signature.to_string()))?;
        entry.status = status;
        entry.at = at;
        // Keep what the entry already says (the amount of an increase) and add the new reason after it.
        entry.note = match (entry.note.take(), note) {
            (Some(old), Some(new)) => Some(format!("{old}; {new}")),
            (old, None) => old,
            (None, new) => new,
        };
        self.append(entry)
    }

    /// False while any entry for `step_key` is `submitted` or `unknown`.
    pub fn may_submit(&self, step_key: &str) -> bool {
        !self
            .latest
            .iter()
            .any(|entry| entry.step == step_key && entry.status.is_unresolved())
    }

    /// Entries whose outcome is not known yet, oldest first.
    pub fn unresolved(&self) -> Vec<JournalEntry> {
        self.latest
            .iter()
            .filter(|entry| entry.status.is_unresolved())
            .cloned()
            .collect()
    }

    /// The latest state of every signature, oldest first.
    pub fn entries(&self) -> &[JournalEntry] {
        &self.latest
    }

    pub fn entry(&self, signature: &str) -> Option<&JournalEntry> {
        self.latest
            .iter()
            .find(|entry| entry.signature == signature)
    }
}
