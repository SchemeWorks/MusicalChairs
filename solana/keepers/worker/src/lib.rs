//! Fee-only keeper worker for the native Musical Chairs program (plan M13).
//!
//! The worker advances the program's permissionless state machines and does
//! nothing else: allocation-ticket phases (Discover, Senior, Seal, Allocate,
//! Prune, Commit) and the round-closing pipeline (candidate pages, candidate
//! build, draw request, capture, apply or expire, finalize).  It carries no
//! authority: the fee payer is ANY ed25519 key, it only pays network fees (and
//! the refundable rent of an opt-in `provision_close_record`), it never builds
//! a system transfer, and no instruction it builds names an owner, governance,
//! guardian, sponsor or recipient signer.  There is no leader election and no
//! exclusive lease: two workers (or a user's own wallet) racing on the same
//! step is harmless because every step carries an expected ticket id / cursor
//! and the loser fails fee-only.
//!
//! Layout:
//! - [`rpc`]: the chain access traits, the `RpcClient` backend and the
//!   scripted [`rpc::MockChain`] used by the tests (no network in tests).
//! - [`state`]: decoded [`state::Snapshot`] of the accounts the planner needs
//!   and the PDA derivations (the program's own seeds).
//! - [`plan`]: the pure planner (`next_step`, one-row fallback, batch
//!   alignment, compute-unit table) and the [`plan::Step`] it produces.
//! - [`journal`]: the append-only JSON-lines journal and the "never resubmit
//!   an unknown outcome" gate.
//! - [`metrics`]: progress markers and the 60 s / 300 s alert levels.
//! - [`runner`]: one pass (`once`): reconcile, plan, simulate, sign, journal,
//!   send.

pub mod journal;
pub mod metrics;
pub mod plan;
pub mod rpc;
pub mod runner;
pub mod state;

/// Lower-case hex of `bytes` (journal fields, digests).
pub fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// Inverse of [`hex_encode`]; `None` on odd length or a non-hex digit.
pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let raw = text.as_bytes();
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    raw.chunks_exact(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(bytes))
}
