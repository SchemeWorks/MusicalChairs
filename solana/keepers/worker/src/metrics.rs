//! Progress markers and alert levels.
//!
//! Progress is judged from ON-CHAIN markers only (ticket id / phase / cursor,
//! close stage, round id): if the marker has not changed for 60 s while work
//! is pending, alert; at 300 s, escalate.  These are monitoring thresholds,
//! not completion guarantees (spec section 7).  An idle pool with nothing
//! pending never alerts.

use serde::{Deserialize, Serialize};

use musical_chairs_program::state::Promotion;

use crate::state::Snapshot;

/// Seconds without a changed on-chain marker (with work pending) before `Alert`.
pub const ALERT_AFTER_SECS: u64 = 60;
/// Seconds before `Escalate`.
pub const ESCALATE_AFTER_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    None,
    Alert,
    Escalate,
}

/// When the on-chain marker last changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub last_progress_at: u64,
    pub last_progress_marker: String,
    /// Whether the latest observation had a ticket or a close waiting to be advanced.
    pub pending_work: bool,
}

impl Progress {
    pub fn new(now: u64) -> Self {
        Self {
            last_progress_at: now,
            last_progress_marker: String::new(),
            pending_work: false,
        }
    }

    /// Record an observation: `last_progress_at` moves to `now` when the marker changed (or nothing is pending).
    pub fn observe(&mut self, now: u64, marker: &str, pending_work: bool) {
        if self.last_progress_marker != marker || !pending_work {
            self.last_progress_at = now;
            self.last_progress_marker = marker.to_string();
        }
        self.pending_work = pending_work;
    }

    pub fn seconds_since_progress(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_progress_at)
    }

    /// `None` below 60 s or when nothing is pending, `Alert` from 60 s, `Escalate` from 300 s.
    pub fn alert_level(&self, now: u64) -> AlertLevel {
        if !self.pending_work {
            return AlertLevel::None;
        }
        match self.seconds_since_progress(now) {
            secs if secs >= ESCALATE_AFTER_SECS => AlertLevel::Escalate,
            secs if secs >= ALERT_AFTER_SECS => AlertLevel::Alert,
            _ => AlertLevel::None,
        }
    }

    /// Parse a previously written metrics JSON (the `--metrics-json` file) back into progress state.
    pub fn from_json(text: &str) -> Option<Progress> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(Progress {
            last_progress_at: value.get("last_progress_at")?.as_u64()?,
            last_progress_marker: value.get("last_progress_marker")?.as_str()?.to_string(),
            pending_work: value.get("pending_work")?.as_bool()?,
        })
    }
}

/// The on-chain progress marker of a snapshot and whether work is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub text: String,
    pub pending_work: bool,
}

/// Ticket id / phase / cursor, or close id / stage / cursors, or the round id when idle.
pub fn marker_of(snapshot: &Snapshot) -> Marker {
    let pool = &snapshot.pool;
    if pool.active_ticket != 0 {
        let text = match &snapshot.ticket {
            Some(ticket) => format!(
                "ticket:{}:phase={}:cursor={}:close={}",
                ticket.id,
                ticket.phase,
                ticket.cursor,
                ticket
                    .close_id
                    .map_or_else(|| "none".to_string(), |id| id.to_string()),
            ),
            None => format!("ticket:{}", pool.active_ticket),
        };
        return Marker {
            text,
            pending_work: true,
        };
    }
    if let Some(close_id) = pool.current_close_id {
        let text = match &snapshot.close {
            Some(close) => format!(
                "close:{close_id}:round={}:cash={}:pages={}+{}:cursor={}:sealed={}:draw={}:promotion={}:settled={}",
                close.round_id,
                u8::from(close.cash_committed),
                close.provisioned_preferred_pages,
                close.provisioned_fallback_pages,
                close.candidate_cursor,
                u8::from(close.sealed),
                close.draw_state,
                match close.promotion {
                    Promotion::Pending => "pending",
                    Promotion::Applied { .. } => "applied",
                    Promotion::Skipped { .. } => "skipped",
                },
                u8::from(close.reserve_settled),
            ),
            None => format!("close:{close_id}"),
        };
        let finished = snapshot.close.as_ref().is_some_and(|close| close.finalized);
        return Marker {
            text,
            pending_work: !finished,
        };
    }
    Marker {
        text: format!(
            "idle:round={}:next_close={}",
            pool.round_id, pool.next_close_id
        ),
        pending_work: false,
    }
}

/// One metrics document (`--metrics-json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetricsReport {
    pub now: u64,
    pub last_progress_at: u64,
    pub last_progress_marker: String,
    pub seconds_since_progress: u64,
    pub alert_level: AlertLevel,
    pub pending_work: bool,
    pub unresolved_journal_entries: usize,
    pub last_action: String,
    pub dry_run: bool,
}

impl MetricsReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}
