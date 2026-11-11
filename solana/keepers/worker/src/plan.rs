//! The pure planner: given a [`Snapshot`] and [`Limits`], name the ONE next
//! step that moves the program forward, or nothing.
//!
//! Priority (ported from the browser client's `nextProcessingStep` and the
//! `advancePhase` builders, both Codex-accepted):
//!
//! 1. an active allocation ticket: Discover, Senior, Seal, Allocate, Prune,
//!    Commit, in that order (a payout is refused with `AllocationPending`
//!    until the ticket is idle);
//! 2. a close in progress: wait for the closing ticket's cash to commit,
//!    apply a captured draw, expire a promotion past its deadline, provision
//!    the candidate pages of both sets, build candidates until sealed, request
//!    the draw, capture it when fulfilled, finalize once the promotion is
//!    terminal and its reserve settled;
//! 3. otherwise, lowest priority and only while idle: (opt-in) provision the
//!    next close record, deliver a deferred owner refund of the last close,
//!    observe its fulfilled provider request, close an eligible shared page of
//!    the last close, reconcile the OpsCash books.
//!
//! One operation sits inside the closing pipeline instead, because its
//! predicate only holds there: `increase_provider_budget` is planned right
//! before `request_round_draw` when (and only when) the request would be refused
//! for want of reserve AND the operator opted in (`max_provider_topup_lamports`,
//! default 0 = never).  It moves FREE SPONSOR operations funds (`ops.free_balance`)
//! into the close's locked sponsor partition; the fee payer pays nothing for it,
//! so the opt-in flag is a sponsor budget decision, not a fee-key payment.  The
//! extra is exactly the shortfall (never partial) and must fit the opted-in
//! maximum, so repeats cannot exceed it in total.
//!
//! Cleanup (`close_shared_account`) looks at the LAST finished close only, probes at most
//! [`crate::state::CLEANUP_PROBE_CAP`] page accounts and closes one page per transaction; leftovers of older
//! closes (or pages beyond the cap) need a wallet.  The refund goes to the destination the page itself
//! recorded; when no owner share is owed OpsCash fills the (never paid) slot.
//!
//! The planner invents nothing: every account comes from the snapshot or from
//! the program's own derivations, every amount from an on-chain counter.  It
//! never sees a fee payer; [`Step::instructions`] fills the payer's slot.
//!
//! Account orders (fixed accounts in IDL order, then the per-row remaining
//! accounts):
//!
//! | step | fixed accounts | per row |
//! |---|---|---|
//! | `discover_rows` | ticket, pool | registry page (ro), snapshot page (w), Backer (ro) |
//! | `apply_senior` | ticket, backer (oldest A, or `None`) | - |
//! | `seal_weights` | ticket | - |
//! | `allocate_rows` | ticket | snapshot page (ro), Backer (w) |
//! | `prune_rows` | ticket, pool | snapshot page (ro), then pruned Backer, its registry page, the last row's registry page, the moved Backer (all w) or four `None` |
//! | `commit_ticket` | ticket, pool, close record (or `None`), event authority, program | - |
//! | `provision_page` | pool, ops, ops cash, page, PAYER, close record, system | - |
//! | `provision_close_record` | pool, ops, ops cash, close record, PAYER, system | - |
//! | `build_candidates` | pool, close record, preferred page, fallback page, ops, ops cash, owner refund, system, event authority, program | participant page, user round, user (all ro) |
//! | `request_round_draw` | PAYER, pool, close record, config, ops, ops cash, rng authority, provider program, network state, treasury, client, request, system, event authority, program | - |
//! | `capture_round_draw` | pool, close record, request, client, event authority, program | - |
//! | `apply_promotion` | PAYER, pool, close record, config, ops, ops cash, candidate page, winner, winner user, winner user round, winner B backer, registry tail, owner refund, system, event authority, program | - |
//! | `expire_promotion` | pool, close record, ops, ops cash, owner refund, system, event authority, program | - |
//! | `finalize_round` | pool, close record, event authority, program | - |
//! | `deliver_owner_refund` | close record, ops, ops cash, owner refund, system, event authority, program | - |
//! | `observe_request_fulfilled` | pool, close record, ops, request, client | - |
//! | `increase_provider_budget` | pool, close record, ops, ops cash | - |
//! | `reconcile_ops_cash` | ops, ops cash | - |
//! | `reconcile_ops_cash_with_client` | ops, ops cash, client | - |
//! | `close_shared_account` | page, ops, ops cash, owner refund (the destination the page recorded; OpsCash, never paid, when no owner component), close record, system | - |

use std::collections::{HashMap, HashSet};

use anchor_lang::solana_program::system_program;
use anchor_lang::{Discriminator, InstructionData, ToAccountMetas};
use musical_chairs_program::{
    accounts,
    allocation::SERIES_B,
    close::{credit_is_deliverable, promotion_holdback, provider_deficit, selected_set},
    instruction, provider,
    sampling::{map_output, protocol_seed},
    state::{
        CloseRecord, Promotion, SnapshotRow, Ticket, ALLOCATE, BACKER_REMAINING_CUTOFF, COMMIT,
        DISCOVER, DRAW_CAPTURED, DRAW_NOT_REQUESTED, DRAW_REQUESTED, PRUNE, SEAL, SENIOR,
    },
    Funding, PageKind,
};
// solana-sdk 2.3.1 marks its compute_budget re-export deprecated in favour of a crate the harness does not
// depend on either; the pins are fixed by the brief, so the re-export is used as the harness uses it.
#[allow(deprecated)]
use solana_sdk::compute_budget::{self, ComputeBudgetInstruction};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::{
    rpc::{RpcError, SimLabel, SimOutcome},
    state::{
        backer_address, candidate_address, close_record_address, config_address,
        event_authority_address, ops_address, ops_cash_address, participant_address, pool_address,
        registry_address, rng_authority_address, snapshot_address, ticket_address, user_address,
        user_round_address, SharedPage, Snapshot, PAGE, PROGRAM_ID,
    },
};

/// Most rows any batched instruction accepts (`allocation::MAX_BATCH_ROWS`, one page).
pub const MAX_BATCH_ROWS: u8 = 8;

/// Compute-unit limit when an instruction has no entry in the table.
pub const CU_DEFAULT: u32 = 200_000;

/// The ceiling of one transaction's compute budget; steps are simulated under it to measure real units.
pub const SIMULATION_CU: u32 = 1_400_000;

/// The fee payer's slot in [`Step::accounts`]: replaced by the real payer in
/// [`Step::instructions`].  An arbitrary constant that is not a program account.
pub const FEE_PAYER_PLACEHOLDER: Pubkey = Pubkey::new_from_array([0xFE; 32]);

/// Compute-unit limit of one instruction (the browser client's table: measured
/// maxima with generous headroom; a limit is a cap, not a charge).
pub fn cu_limit_for(name: &str) -> u32 {
    match name {
        "prune_rows" | "apply_promotion" => 300_000,
        "build_candidates" | "request_round_draw" => 400_000,
        _ => CU_DEFAULT,
    }
}

/// Knobs of the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Rows per batched step, 1..=8.
    pub max_rows: u8,
    /// Prune fallback: leave rows that only became capped by this ticket's credits for a later ticket.
    pub skip_newly_capped: bool,
    /// Opt-in: while idle, provision `CloseRecord(next_close_id)` (the payer fronts its refundable rent)
    /// so a user's closing payout does not fail `PageNotProvisioned`.
    pub preprovision_close_record: bool,
    /// Opt-in ceiling, in lamports, on the free sponsor operations funds one `increase_provider_budget` may
    /// move into a close's locked reserve (spec: an owner/operator contribution needs an explicit maximum).
    /// The default 0 means the step is never planned.  The extra is exactly the shortfall (never partial, so a
    /// repeat cannot exceed the ceiling), planned only when that shortfall is at most this many lamports.
    pub max_provider_topup_lamports: u64,
    /// The OpsCash / ORAO Client books are reconciled only for a change of at least this many lamports (a
    /// dust donation costs the donor 1 lamport and would cost the keeper a fee).  The default 0 reconciles any
    /// change; a changed protected-rent floor always counts.
    pub min_reconcile_lamports: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: MAX_BATCH_ROWS,
            skip_newly_capped: false,
            preprovision_close_record: false,
            max_provider_topup_lamports: 0,
            min_reconcile_lamports: 0,
        }
    }
}

/// Which permissionless instruction a step is, with the on-chain identity it is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    Discover {
        ticket_id: u64,
        cursor: u64,
    },
    Senior {
        ticket_id: u64,
    },
    Seal {
        ticket_id: u64,
    },
    Allocate {
        ticket_id: u64,
        cursor: u64,
    },
    Prune {
        ticket_id: u64,
        cursor: u64,
    },
    Commit {
        ticket_id: u64,
    },
    ProvisionCloseRecord {
        close_id: u64,
    },
    /// A candidate page of one set, paid from the close's locked reserve.
    ProvisionPage {
        close_id: u64,
        set_tag: u8,
        index: u64,
    },
    BuildCandidates {
        close_id: u64,
        cursor: u64,
    },
    RequestDraw {
        close_id: u64,
    },
    CaptureDraw {
        close_id: u64,
    },
    ApplyPromotion {
        close_id: u64,
    },
    ExpirePromotion {
        close_id: u64,
    },
    Finalize {
        close_id: u64,
    },
    /// Lowest priority: pay a settled close's deferred owner refund to its recorded destination.
    DeliverOwnerRefund {
        close_id: u64,
    },
    /// Lowest priority: freeze a fulfilled request's provider reimbursement figures, once.
    ObserveRequestFulfilled {
        close_id: u64,
    },
    /// Opt-in, before the draw request only: move `extra` lamports of free sponsor operations funds into the
    /// close's locked sponsor partition (and raise its provider-fee cap by the same amount).
    IncreaseProviderBudget {
        close_id: u64,
        extra: u64,
    },
    /// Idle bookkeeping: fold the OpsCash surplus into `unallocated_excess` (the books held these figures).
    ReconcileOpsCash {
        observed_excess: u64,
        observed_protected_rent: u64,
    },
    /// As `ReconcileOpsCash`, plus the ORAO Client observation.
    ReconcileOpsCashWithClient {
        observed_excess: u64,
        observed_client_balance: u64,
    },
    /// Idle cleanup of one released shared page, judged by `CloseRecord(close_id)`; the rent returns to
    /// OpsCash and to the refund destination the page itself recorded.
    CloseSharedAccount {
        close_id: u64,
        page: PageKind,
        index: u64,
    },
}

/// One transaction's worth of work, independent of who pays for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub kind: StepKind,
    /// Rows a batched step covers (0 for the unbatched instructions).
    pub rows: u8,
    pub cu_limit: u32,
    /// The Anchor-declared accounts in IDL order; the fee payer's slot (if the
    /// instruction has one) holds [`FEE_PAYER_PLACEHOLDER`].
    pub accounts: Vec<AccountMeta>,
    /// The variable-length remaining accounts, grouped per logical row.
    pub row_accounts: Vec<Vec<AccountMeta>>,
}

impl Step {
    /// The program instruction's IDL name.
    pub fn name(&self) -> &'static str {
        match self.kind {
            StepKind::Discover { .. } => "discover_rows",
            StepKind::Senior { .. } => "apply_senior",
            StepKind::Seal { .. } => "seal_weights",
            StepKind::Allocate { .. } => "allocate_rows",
            StepKind::Prune { .. } => "prune_rows",
            StepKind::Commit { .. } => "commit_ticket",
            StepKind::ProvisionCloseRecord { .. } => "provision_close_record",
            StepKind::ProvisionPage { .. } => "provision_page",
            StepKind::BuildCandidates { .. } => "build_candidates",
            StepKind::RequestDraw { .. } => "request_round_draw",
            StepKind::CaptureDraw { .. } => "capture_round_draw",
            StepKind::ApplyPromotion { .. } => "apply_promotion",
            StepKind::ExpirePromotion { .. } => "expire_promotion",
            StepKind::Finalize { .. } => "finalize_round",
            StepKind::DeliverOwnerRefund { .. } => "deliver_owner_refund",
            StepKind::ObserveRequestFulfilled { .. } => "observe_request_fulfilled",
            StepKind::IncreaseProviderBudget { .. } => "increase_provider_budget",
            StepKind::ReconcileOpsCash { .. } => "reconcile_ops_cash",
            StepKind::ReconcileOpsCashWithClient { .. } => "reconcile_ops_cash_with_client",
            StepKind::CloseSharedAccount { .. } => "close_shared_account",
        }
    }

    /// The identity the journal gates on: the instruction plus the on-chain
    /// anchor it is bound to (ticket id / cursor, close id / cursor, page).  It
    /// deliberately excludes `rows`, so a one-row retry of an 8-row attempt
    /// whose outcome is unknown is the SAME step and stays blocked.
    pub fn key(&self) -> String {
        let name = self.name();
        match &self.kind {
            StepKind::Discover { ticket_id, cursor }
            | StepKind::Allocate { ticket_id, cursor }
            | StepKind::Prune { ticket_id, cursor } => format!("{name}:{ticket_id}:{cursor}"),
            StepKind::Senior { ticket_id }
            | StepKind::Seal { ticket_id }
            | StepKind::Commit { ticket_id } => {
                format!("{name}:{ticket_id}")
            }
            StepKind::ProvisionPage {
                close_id,
                set_tag,
                index,
            } => format!("{name}:{close_id}:{set_tag}:{index}"),
            StepKind::BuildCandidates { close_id, cursor } => format!("{name}:{close_id}:{cursor}"),
            StepKind::ProvisionCloseRecord { close_id }
            | StepKind::RequestDraw { close_id }
            | StepKind::CaptureDraw { close_id }
            | StepKind::ApplyPromotion { close_id }
            | StepKind::ExpirePromotion { close_id }
            | StepKind::Finalize { close_id }
            | StepKind::DeliverOwnerRefund { close_id }
            | StepKind::ObserveRequestFulfilled { close_id }
            | StepKind::IncreaseProviderBudget { close_id, .. } => format!("{name}:{close_id}"),
            StepKind::ReconcileOpsCash {
                observed_excess,
                observed_protected_rent,
            } => format!("{name}:{observed_excess}:{observed_protected_rent}"),
            StepKind::ReconcileOpsCashWithClient {
                observed_excess,
                observed_client_balance,
            } => format!("{name}:{observed_excess}:{observed_client_balance}"),
            StepKind::CloseSharedAccount {
                close_id,
                page,
                index,
            } => {
                let which = match page {
                    PageKind::Participant { .. } => "p".to_string(),
                    PageKind::Candidate { set_tag, .. } => format!("c{set_tag}"),
                    PageKind::Registry | PageKind::Snapshot => "x".to_string(),
                };
                format!("{name}:{close_id}:{which}:{index}")
            }
        }
    }

    /// What the journal records beside the signed bytes (the amount an `increase_provider_budget` moves).
    pub fn journal_note(&self) -> Option<String> {
        match &self.kind {
            StepKind::IncreaseProviderBudget { extra, .. } => Some(format!(
                "extra={extra} lamports of free operations funds into the close's provider reserve"
            )),
            _ => None,
        }
    }

    /// What the simulator (and the mock) are told this step is.
    pub fn label(&self) -> SimLabel {
        SimLabel {
            name: self.name(),
            rows: self.rows,
        }
    }

    /// Anchor instruction data (8-byte discriminator + Borsh args) for the step's current `rows`.
    pub fn data(&self) -> Vec<u8> {
        let rows = self.rows;
        match &self.kind {
            StepKind::Discover { ticket_id, cursor } => instruction::DiscoverRows {
                expected_ticket_id: *ticket_id,
                expected_cursor: *cursor,
                rows,
            }
            .data(),
            StepKind::Senior { ticket_id } => instruction::ApplySenior {
                expected_ticket_id: *ticket_id,
            }
            .data(),
            StepKind::Seal { ticket_id } => instruction::SealWeights {
                expected_ticket_id: *ticket_id,
            }
            .data(),
            StepKind::Allocate { ticket_id, cursor } => instruction::AllocateRows {
                expected_ticket_id: *ticket_id,
                expected_cursor: *cursor,
                rows,
            }
            .data(),
            StepKind::Prune { ticket_id, cursor } => instruction::PruneRows {
                expected_ticket_id: *ticket_id,
                expected_cursor: *cursor,
                rows,
            }
            .data(),
            StepKind::Commit { ticket_id } => instruction::CommitTicket {
                expected_ticket_id: *ticket_id,
            }
            .data(),
            StepKind::ProvisionCloseRecord { close_id } => instruction::ProvisionCloseRecord {
                close_id: *close_id,
                funding: Funding::Caller,
            }
            .data(),
            StepKind::ProvisionPage {
                close_id,
                set_tag,
                index,
            } => instruction::ProvisionPage {
                kind: PageKind::Candidate {
                    close_id: *close_id,
                    set_tag: *set_tag,
                },
                index: *index,
                funding: Funding::LockedClose,
            }
            .data(),
            StepKind::BuildCandidates { .. } => instruction::BuildCandidates { rows }.data(),
            StepKind::RequestDraw { .. } => instruction::RequestRoundDraw.data(),
            StepKind::CaptureDraw { .. } => instruction::CaptureRoundDraw.data(),
            StepKind::ApplyPromotion { .. } => instruction::ApplyPromotion.data(),
            StepKind::ExpirePromotion { .. } => instruction::ExpirePromotion.data(),
            StepKind::Finalize { .. } => instruction::FinalizeRound.data(),
            StepKind::DeliverOwnerRefund { .. } => instruction::DeliverOwnerRefund.data(),
            StepKind::ObserveRequestFulfilled { .. } => instruction::ObserveRequestFulfilled.data(),
            StepKind::IncreaseProviderBudget { extra, .. } => {
                instruction::IncreaseProviderBudget { extra: *extra }.data()
            }
            StepKind::ReconcileOpsCash { .. } => instruction::ReconcileOpsCash.data(),
            StepKind::ReconcileOpsCashWithClient { .. } => {
                instruction::ReconcileOpsCashWithClient.data()
            }
            StepKind::CloseSharedAccount { page, .. } => {
                instruction::CloseSharedAccount { kind: *page }.data()
            }
        }
    }

    /// `[compute-unit limit, the program instruction]` with `fee_payer` in the payer slot.
    pub fn instructions(&self, fee_payer: &Pubkey) -> Vec<Instruction> {
        self.instructions_with_cu(fee_payer, self.cu_limit)
    }

    /// As [`Step::instructions`] under an explicit compute-unit limit (simulation measures under the ceiling).
    pub fn instructions_with_cu(&self, fee_payer: &Pubkey, units: u32) -> Vec<Instruction> {
        let mut accounts: Vec<AccountMeta> = self
            .accounts
            .iter()
            .map(|meta| {
                let mut meta = meta.clone();
                if meta.pubkey == FEE_PAYER_PLACEHOLDER {
                    meta.pubkey = *fee_payer;
                }
                meta
            })
            .collect();
        accounts.extend(self.row_accounts.iter().flatten().cloned());
        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(units),
            Instruction {
                program_id: PROGRAM_ID,
                accounts,
                data: self.data(),
            },
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// An account the snapshot should hold for this decision is absent.
    Missing(String),
    /// The on-chain counters contradict each other (a phase with nothing left, an unknown phase ...).
    Inconsistent(String),
    /// A built transaction would do more than pay fees ([`assert_fee_only`]).
    NotFeeOnly(String),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Missing(what) => {
                write!(f, "plan needs {what}, which the snapshot does not hold")
            }
            PlanError::Inconsistent(what) => write!(f, "inconsistent on-chain state: {what}"),
            PlanError::NotFeeOnly(what) => {
                write!(f, "refusing a transaction that is not fee-only: {what}")
            }
        }
    }
}

impl std::error::Error for PlanError {}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn ro(key: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(key, false)
}

fn rw(key: Pubkey) -> AccountMeta {
    AccountMeta::new(key, false)
}

/// Anchor's `None` for an optional account: the program id, read-only.
fn none_placeholder() -> AccountMeta {
    ro(PROGRAM_ID)
}

fn step(
    kind: StepKind,
    rows: u8,
    accounts: Vec<AccountMeta>,
    row_accounts: Vec<Vec<AccountMeta>>,
) -> Step {
    let mut step = Step {
        kind,
        rows,
        cu_limit: CU_DEFAULT,
        accounts,
        row_accounts,
    };
    step.cu_limit = cu_limit_for(step.name());
    step
}

fn registry_key_at(snapshot: &Snapshot, slot: u64) -> Result<Pubkey, PlanError> {
    let page = snapshot
        .registry_pages
        .get(&(slot / PAGE))
        .ok_or_else(|| PlanError::Missing(format!("registry page {}", slot / PAGE)))?;
    let key = page.keys[(slot % PAGE) as usize];
    if key == Pubkey::default() {
        return Err(PlanError::Inconsistent(format!(
            "registry slot {slot} holds no key"
        )));
    }
    Ok(key)
}

fn snapshot_row_at(snapshot: &Snapshot, slot: u64) -> Result<SnapshotRow, PlanError> {
    let page = snapshot
        .snapshot_pages
        .get(&(slot / PAGE))
        .ok_or_else(|| PlanError::Missing(format!("snapshot page {}", slot / PAGE)))?;
    let row = page.rows[(slot % PAGE) as usize];
    if row.slot != slot || row.key == Pubkey::default() {
        return Err(PlanError::Inconsistent(format!(
            "snapshot row {slot} is not captured"
        )));
    }
    Ok(row)
}

/// Rows a forward ticket batch may carry: at most `max_rows`, at most what is left of `end`, and never
/// past the end of the page `cursor` is on (one batch never straddles a page edge).
pub fn batch_rows_forward(cursor: u64, end: u64, max_rows: u8) -> u8 {
    let left = end.saturating_sub(cursor);
    let to_edge = PAGE - cursor % PAGE;
    u8::try_from(u64::from(max_rows).min(left).min(to_edge)).unwrap_or(0)
}

/// Rows a Prune batch (walking DOWN from `cursor`, exclusive) may carry without leaving the page of slot `cursor - 1`.
pub fn batch_rows_down(cursor: u64, max_rows: u8) -> u8 {
    if cursor == 0 {
        return 0;
    }
    let to_edge = (cursor - 1) % PAGE + 1;
    u8::try_from(u64::from(max_rows).min(cursor).min(to_edge)).unwrap_or(0)
}

/// Candidate rows one `build_candidates` batch may carry.  At most `max_rows`, and aligned so the batch
/// cannot run past a page the call supplies: a call passes ONE participant page and ONE page per
/// candidate set, and fails `PageNotProvisioned` if its appends cross into the next.  A row can land in
/// the preferred set, the fallback set or both, which is only known on chain, so the bound is the worst
/// case for BOTH sets:
/// `min(max_rows, participants left, rows left on this participant page,
/// 8 - preferred_count % 8, 8 - fallback_count % 8)`.  Zero once every participant was visited.
pub fn candidate_batch_rows(close: &CloseRecord, max_rows: u8) -> u8 {
    let left = close
        .participant_count
        .saturating_sub(close.candidate_cursor);
    if left == 0 {
        return 0;
    }
    let bound = [
        u64::from(max_rows),
        left,
        PAGE - close.candidate_cursor % PAGE,
        PAGE - close.preferred_count % PAGE,
        PAGE - close.fallback_count % PAGE,
    ]
    .into_iter()
    .min()
    .unwrap_or(0);
    u8::try_from(bound).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// next_step
// ---------------------------------------------------------------------------

/// The one step that moves the program forward from `snapshot`, or `None` when nothing can
/// be sent right now (idle, or waiting on the cash commit / the provider).
pub fn next_step(snapshot: &Snapshot, limits: &Limits) -> Result<Option<Step>, PlanError> {
    if !(1..=MAX_BATCH_ROWS).contains(&limits.max_rows) {
        return Err(PlanError::Inconsistent(format!(
            "max_rows must be 1..={MAX_BATCH_ROWS}, got {}",
            limits.max_rows
        )));
    }
    if snapshot.pool.active_ticket != 0 {
        return ticket_step(snapshot, limits).map(Some);
    }
    if let Some(close_id) = snapshot.pool.current_close_id {
        return closing_step(snapshot, close_id, limits);
    }
    Ok(idle_step(snapshot, limits))
}

fn ticket_step(snapshot: &Snapshot, limits: &Limits) -> Result<Step, PlanError> {
    let ticket = snapshot
        .ticket
        .as_ref()
        .ok_or_else(|| PlanError::Missing("the active ticket".to_string()))?;
    if ticket.id != snapshot.pool.active_ticket {
        return Err(PlanError::Inconsistent(format!(
            "pool.active_ticket is {} but the ticket account is {}",
            snapshot.pool.active_ticket, ticket.id
        )));
    }
    match ticket.phase {
        DISCOVER => discover_step(snapshot, ticket, limits),
        SENIOR => {
            let backer = if ticket.a_count == 0 {
                None
            } else {
                Some(ticket.oldest)
            };
            let accounts = accounts::ApplySenior {
                ticket: ticket_address(),
                backer,
            }
            .to_account_metas(None);
            Ok(step(
                StepKind::Senior {
                    ticket_id: ticket.id,
                },
                0,
                accounts,
                Vec::new(),
            ))
        }
        SEAL => {
            let accounts = accounts::SealWeights {
                ticket: ticket_address(),
            }
            .to_account_metas(None);
            Ok(step(
                StepKind::Seal {
                    ticket_id: ticket.id,
                },
                0,
                accounts,
                Vec::new(),
            ))
        }
        ALLOCATE => allocate_step(snapshot, ticket, limits),
        PRUNE => prune_step(snapshot, ticket, limits),
        COMMIT => {
            let accounts = accounts::CommitTicket {
                ticket: ticket_address(),
                pool: pool_address(),
                close_record: ticket.close_id.map(close_record_address),
                event_authority: event_authority_address(),
                program: PROGRAM_ID,
            }
            .to_account_metas(None);
            Ok(step(
                StepKind::Commit {
                    ticket_id: ticket.id,
                },
                0,
                accounts,
                Vec::new(),
            ))
        }
        other => Err(PlanError::Inconsistent(format!(
            "ticket {} is active in phase {other}",
            ticket.id
        ))),
    }
}

fn discover_step(snapshot: &Snapshot, ticket: &Ticket, limits: &Limits) -> Result<Step, PlanError> {
    let rows = batch_rows_forward(ticket.cursor, ticket.captured_len, limits.max_rows);
    if rows == 0 {
        return Err(PlanError::Inconsistent(
            "Discover has no rows left but the ticket is still in that phase".to_string(),
        ));
    }
    let mut row_accounts = Vec::with_capacity(usize::from(rows));
    for i in 0..u64::from(rows) {
        let slot = ticket.cursor + i;
        let backer = registry_key_at(snapshot, slot)?;
        row_accounts.push(vec![
            ro(registry_address(slot / PAGE)),
            rw(snapshot_address(slot / PAGE)),
            ro(backer),
        ]);
    }
    let accounts = accounts::DiscoverRows {
        ticket: ticket_address(),
        pool: pool_address(),
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::Discover {
            ticket_id: ticket.id,
            cursor: ticket.cursor,
        },
        rows,
        accounts,
        row_accounts,
    ))
}

fn allocate_step(snapshot: &Snapshot, ticket: &Ticket, limits: &Limits) -> Result<Step, PlanError> {
    let rows = batch_rows_forward(ticket.cursor, ticket.captured_len, limits.max_rows);
    if rows == 0 {
        return Err(PlanError::Inconsistent(
            "Allocate has no rows left but the ticket is still in that phase".to_string(),
        ));
    }
    let mut row_accounts = Vec::with_capacity(usize::from(rows));
    for i in 0..u64::from(rows) {
        let slot = ticket.cursor + i;
        let row = snapshot_row_at(snapshot, slot)?;
        row_accounts.push(vec![ro(snapshot_address(slot / PAGE)), rw(row.key)]);
    }
    let accounts = accounts::AllocateRows {
        ticket: ticket_address(),
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::Allocate {
            ticket_id: ticket.id,
            cursor: ticket.cursor,
        },
        rows,
        accounts,
        row_accounts,
    ))
}

/// Prune walks DOWN from the ticket cursor, swap-deleting capped/closed rows from the dense registry.
/// Per row: `[SnapshotPage (ro), row Backer (w), RegistryPage of the slot (w), RegistryPage of the last
/// row (w), moved Backer (w)]` for a row to remove, or `[SnapshotPage (ro), 4 x program id (ro)]` for a
/// row to keep.  Earlier swap-deletes of the same batch are simulated, and the batch is cut BEFORE a row
/// whose moved Backer was already written earlier in the batch (that would be a duplicate writable
/// account).  Port of `planPrune`.
fn prune_step(snapshot: &Snapshot, ticket: &Ticket, limits: &Limits) -> Result<Step, PlanError> {
    let limit = batch_rows_down(ticket.cursor, limits.max_rows);
    if limit == 0 {
        return Err(PlanError::Inconsistent(
            "Prune has no rows left but the ticket is still in that phase".to_string(),
        ));
    }
    let mut overrides: HashMap<u64, Pubkey> = HashMap::new();
    let mut written: HashSet<Pubkey> = HashSet::new();
    let mut len = snapshot.pool.registry_len;
    let mut row_accounts: Vec<Vec<AccountMeta>> = Vec::with_capacity(usize::from(limit));

    for i in 0..u64::from(limit) {
        let slot = ticket.cursor - 1 - i;
        let row = snapshot_row_at(snapshot, slot)?;
        let captured_closed = row.remaining <= BACKER_REMAINING_CUTOFF;
        let newly_capped = if limits.skip_newly_capped {
            false
        } else {
            let backer = snapshot.backers.get(&row.key).ok_or_else(|| {
                PlanError::Missing(format!("Backer {} named by snapshot row {slot}", row.key))
            })?;
            backer.entitlement.saturating_sub(backer.lifetime) <= BACKER_REMAINING_CUTOFF
        };
        if captured_closed || newly_capped {
            let last = len.checked_sub(1).ok_or_else(|| {
                PlanError::Inconsistent("prune from an empty registry".to_string())
            })?;
            let moved = match overrides.get(&last) {
                Some(key) => *key,
                None => registry_key_at(snapshot, last)?,
            };
            if slot != last && written.contains(&moved) {
                break;
            }
            row_accounts.push(vec![
                ro(snapshot_address(slot / PAGE)),
                rw(row.key),
                rw(registry_address(slot / PAGE)),
                rw(registry_address(last / PAGE)),
                rw(moved),
            ]);
            written.insert(row.key);
            if slot != last {
                written.insert(moved);
                overrides.insert(slot, moved);
            }
            len -= 1;
        } else {
            row_accounts.push(vec![
                ro(snapshot_address(slot / PAGE)),
                none_placeholder(),
                none_placeholder(),
                none_placeholder(),
                none_placeholder(),
            ]);
        }
    }
    let rows = u8::try_from(row_accounts.len()).unwrap_or(0);
    let accounts = accounts::PruneRows {
        ticket: ticket_address(),
        pool: pool_address(),
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::Prune {
            ticket_id: ticket.id,
            cursor: ticket.cursor,
        },
        rows,
        accounts,
        row_accounts,
    ))
}

// ---------------------------------------------------------------------------
// The closing pipeline
// ---------------------------------------------------------------------------

fn closing_step(
    snapshot: &Snapshot,
    close_id: u64,
    limits: &Limits,
) -> Result<Option<Step>, PlanError> {
    let close = snapshot
        .close
        .as_ref()
        .ok_or_else(|| PlanError::Missing(format!("close record {close_id}")))?;
    if close.close_id != close_id {
        return Err(PlanError::Inconsistent(format!(
            "pool.current_close_id is {close_id} but the close record is {}",
            close.close_id
        )));
    }
    if close.finalized {
        return Ok(None);
    }
    // A terminal promotion only waits for its reserve settlement before finalizing (apply / expire settle it).
    if close.promotion != Promotion::Pending {
        return Ok(close.reserve_settled.then(|| finalize_step(close)));
    }
    // The closing payout's allocation ticket must commit its cash first.
    if !close.cash_committed {
        return Ok(None);
    }
    // A timely captured output still applies after the deadline; otherwise a pending promotion past it expires.
    if close.draw_state == DRAW_CAPTURED {
        return apply_step(snapshot, close).map(Some);
    }
    if snapshot.now() > close.promotion_deadline {
        return Ok(Some(expire_step(close)));
    }
    if !close.sealed {
        for set_tag in [0u8, 1] {
            let provisioned = if set_tag == 0 {
                close.provisioned_preferred_pages
            } else {
                close.provisioned_fallback_pages
            };
            if provisioned < close.required_pages_per_set {
                return Ok(Some(provision_page_step(close, set_tag, provisioned)));
            }
        }
        return build_candidates_step(snapshot, close, limits).map(Some);
    }
    match close.draw_state {
        DRAW_NOT_REQUESTED => {
            // Only when the request would be refused for want of reserve and the operator opted in.
            if let Some(topup) = increase_budget_step(snapshot, close, limits) {
                return Ok(Some(topup));
            }
            request_step(snapshot, close).map(Some)
        }
        // Known pending: wait. Fulfilled or not knowable: try the capture and let the program decide.
        DRAW_REQUESTED if snapshot.request_fulfilled == Some(false) => Ok(None),
        DRAW_REQUESTED => capture_step(snapshot, close).map(Some),
        // IgnoredLate arrives only after the deadline, which was handled above.
        _ => Ok(None),
    }
}

fn config_of(
    snapshot: &Snapshot,
) -> Result<&musical_chairs_program::state::ConfigVersion, PlanError> {
    snapshot
        .config
        .as_ref()
        .ok_or_else(|| PlanError::Missing("the close's pinned ConfigVersion".to_string()))
}

fn provision_page_step(close: &CloseRecord, set_tag: u8, index: u64) -> Step {
    let accounts = accounts::ProvisionPage {
        pool: pool_address(),
        ops: ops_address(),
        ops_cash: ops_cash_address(),
        page: candidate_address(close.close_id, set_tag, index),
        payer: FEE_PAYER_PLACEHOLDER,
        close_record: Some(close_record_address(close.close_id)),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    step(
        StepKind::ProvisionPage {
            close_id: close.close_id,
            set_tag,
            index,
        },
        0,
        accounts,
        Vec::new(),
    )
}

fn build_candidates_step(
    snapshot: &Snapshot,
    close: &CloseRecord,
    limits: &Limits,
) -> Result<Step, PlanError> {
    let rows = candidate_batch_rows(close, limits.max_rows);
    if rows == 0 {
        return Err(PlanError::Inconsistent(
            "every participant was visited but the candidates are not sealed".to_string(),
        ));
    }
    let mut row_accounts = Vec::with_capacity(usize::from(rows));
    for i in 0..u64::from(rows) {
        let slot = close.candidate_cursor + i;
        let page = snapshot
            .participant_pages
            .get(&(slot / PAGE))
            .ok_or_else(|| {
                PlanError::Missing(format!(
                    "participant page {} of round {}",
                    slot / PAGE,
                    close.round_id
                ))
            })?;
        let offset = (slot % PAGE) as usize;
        if offset >= usize::from(page.len) {
            return Err(PlanError::Inconsistent(format!(
                "participant slot {slot} is past the page's {} rows",
                page.len
            )));
        }
        let owner = page.rows[offset];
        row_accounts.push(vec![
            ro(participant_address(close.round_id, slot / PAGE)),
            ro(user_round_address(close.round_id, &owner)),
            ro(user_address(&owner)),
        ]);
    }
    let preferred_index = close.preferred_count / PAGE;
    let fallback_index = close.fallback_count / PAGE;
    let accounts = accounts::BuildCandidates {
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        preferred_page: (close.provisioned_preferred_pages > preferred_index)
            .then(|| candidate_address(close.close_id, 0, preferred_index)),
        fallback_page: (close.provisioned_fallback_pages > fallback_index)
            .then(|| candidate_address(close.close_id, 1, fallback_index)),
        ops: Some(ops_address()),
        ops_cash: Some(ops_cash_address()),
        owner_refund: Some(close.owner_refund_destination),
        system_program: system_program::ID,
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::BuildCandidates {
            close_id: close.close_id,
            cursor: close.candidate_cursor,
        },
        rows,
        accounts,
        row_accounts,
    ))
}

fn request_step(snapshot: &Snapshot, close: &CloseRecord) -> Result<Step, PlanError> {
    let config = config_of(snapshot)?;
    let (tag, count, digest) = selected_set(close);
    let seed = protocol_seed(
        &snapshot.pool.cluster_domain,
        &PROGRAM_ID,
        close.round_id,
        close.close_id,
        tag,
        count,
        &digest,
    );
    let (request, _) = provider::request_binding(&config.provider_client, &seed);
    let accounts = accounts::RequestRoundDraw {
        payer: FEE_PAYER_PLACEHOLDER,
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        config: config_address(close.config_version),
        ops: ops_address(),
        ops_cash: ops_cash_address(),
        rng_authority: rng_authority_address(),
        provider_program: config.provider_program,
        network_state: config.provider_network_state,
        treasury: config.provider_treasury,
        client: config.provider_client,
        request,
        system_program: system_program::ID,
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::RequestDraw {
            close_id: close.close_id,
        },
        0,
        accounts,
        Vec::new(),
    ))
}

fn capture_step(snapshot: &Snapshot, close: &CloseRecord) -> Result<Step, PlanError> {
    let config = config_of(snapshot)?;
    let accounts = accounts::CaptureRoundDraw {
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        request: close.request_account,
        client: config.provider_client,
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::CaptureDraw {
            close_id: close.close_id,
        },
        0,
        accounts,
        Vec::new(),
    ))
}

fn apply_step(snapshot: &Snapshot, close: &CloseRecord) -> Result<Step, PlanError> {
    // The captured output maps (or, entropy-exhausted, does not map) to a candidate ordinal; the winner's
    // accounts are derived from the sealed candidate page.  An output that maps to nothing needs none.
    let (tag, count, _) = selected_set(close);
    let (
        mut candidate_page,
        mut winner,
        mut winner_user,
        mut winner_user_round,
        mut winner_backer_b,
        mut registry_tail,
    ) = (None, None, None, None, None, None);
    if let Some((ordinal, _counter)) = map_output(close.randomness, close.round_id, count) {
        let page = snapshot
            .candidate_pages
            .get(&(tag, ordinal / PAGE))
            .ok_or_else(|| {
                PlanError::Missing(format!("candidate page {} of set {tag}", ordinal / PAGE))
            })?;
        let key = page.rows[(ordinal % PAGE) as usize];
        candidate_page = Some(candidate_address(close.close_id, tag, ordinal / PAGE));
        winner = Some(key);
        winner_user = Some(user_address(&key));
        winner_user_round = Some(user_round_address(close.round_id, &key));
        winner_backer_b = Some(backer_address(SERIES_B, &key));
        registry_tail = Some(registry_address(snapshot.pool.registry_len / PAGE));
    }
    let accounts = accounts::ApplyPromotion {
        payer: FEE_PAYER_PLACEHOLDER,
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        config: config_address(close.config_version),
        ops: ops_address(),
        ops_cash: ops_cash_address(),
        candidate_page,
        winner,
        winner_user,
        winner_user_round,
        winner_backer_b,
        registry_tail,
        owner_refund: close.owner_refund_destination,
        system_program: system_program::ID,
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    Ok(step(
        StepKind::ApplyPromotion {
            close_id: close.close_id,
        },
        0,
        accounts,
        Vec::new(),
    ))
}

fn expire_step(close: &CloseRecord) -> Step {
    let accounts = accounts::ExpirePromotion {
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        ops: ops_address(),
        ops_cash: ops_cash_address(),
        owner_refund: close.owner_refund_destination,
        system_program: system_program::ID,
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    step(
        StepKind::ExpirePromotion {
            close_id: close.close_id,
        },
        0,
        accounts,
        Vec::new(),
    )
}

fn finalize_step(close: &CloseRecord) -> Step {
    let accounts = accounts::FinalizeRound {
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        event_authority: event_authority_address(),
        program: PROGRAM_ID,
    }
    .to_account_metas(None);
    step(
        StepKind::Finalize {
            close_id: close.close_id,
        },
        0,
        accounts,
        Vec::new(),
    )
}

// ---------------------------------------------------------------------------
// Idle: lowest-priority housekeeping
// ---------------------------------------------------------------------------

fn idle_step(snapshot: &Snapshot, limits: &Limits) -> Option<Step> {
    if limits.preprovision_close_record && !snapshot.next_close_record_exists {
        let close_id = snapshot.pool.next_close_id;
        let accounts = accounts::ProvisionCloseRecord {
            pool: pool_address(),
            ops: ops_address(),
            ops_cash: ops_cash_address(),
            close_record: close_record_address(close_id),
            payer: FEE_PAYER_PLACEHOLDER,
            system_program: system_program::ID,
        }
        .to_account_metas(None);
        return Some(step(
            StepKind::ProvisionCloseRecord { close_id },
            0,
            accounts,
            Vec::new(),
        ));
    }
    if let Some(step) = last_close_step(snapshot) {
        return Some(step);
    }
    cleanup_step(snapshot).or_else(|| reconcile_step(snapshot, limits))
}

/// Lowest-priority steps that look at the most recent finished close: the deferred owner refund, then the
/// provider-reimbursement observation.
fn last_close_step(snapshot: &Snapshot) -> Option<Step> {
    let last = snapshot.last_close.as_ref()?;
    if refund_is_deliverable(snapshot, last) {
        let accounts = accounts::DeliverOwnerRefund {
            close_record: close_record_address(last.close_id),
            ops: ops_address(),
            ops_cash: ops_cash_address(),
            owner_refund: last.owner_refund_destination,
            system_program: system_program::ID,
            event_authority: event_authority_address(),
            program: PROGRAM_ID,
        }
        .to_account_metas(None);
        return Some(step(
            StepKind::DeliverOwnerRefund {
                close_id: last.close_id,
            },
            0,
            accounts,
            Vec::new(),
        ));
    }
    if last.draw_state != DRAW_NOT_REQUESTED
        && !last.reimbursement_observed
        && last.request_account != Pubkey::default()
        && snapshot.request_fulfilled == Some(true)
    {
        let accounts = accounts::ObserveRequestFulfilled {
            pool: pool_address(),
            close_record: close_record_address(last.close_id),
            ops: ops_address(),
            request: last.request_account,
            client: provider::client_address().0,
        }
        .to_account_metas(None);
        return Some(step(
            StepKind::ObserveRequestFulfilled {
                close_id: last.close_id,
            },
            0,
            accounts,
            Vec::new(),
        ));
    }
    None
}

/// The program's own preflight for `deliver_owner_refund`: the close's reserve is settled, a deferred owner
/// share is owed, the recorded destination is a plain system account that is not OpsCash, and
/// `credit_is_deliverable(destination lamports, owed, rent floor of a data-less account)`.  Without a loaded
/// destination the answer is no: the keeper never guesses.
fn refund_is_deliverable(snapshot: &Snapshot, last: &CloseRecord) -> bool {
    if !last.reserve_settled || last.locked_owner_remaining == 0 {
        return false;
    }
    let Some(target) = snapshot.refund_target else {
        return false;
    };
    target.plain_system_account
        && last.owner_refund_destination != ops_cash_address()
        && credit_is_deliverable(
            target.lamports,
            last.locked_owner_remaining,
            snapshot.rent_exempt_zero,
        )
}

// ---------------------------------------------------------------------------
// Provider budget, cleanup, bookkeeping (IMPL-49d)
// ---------------------------------------------------------------------------

/// The extra lamports `increase_provider_budget` must add before the program accepts `request_round_draw`,
/// or `None` when the request is affordable (or cannot be judged: the live fee, the Ops books or the config
/// were not read).  The request is refused (`ProviderFeeExceeded`) when the live ORAO fee is over the close's
/// `approved_provider_fee_cap`, or when `fee + request rent + promotion_holdback` is over what the sponsor
/// and owner partitions still hold; one extra raises BOTH the partition and the cap, so the extra needed is the
/// larger of the two gaps.
pub fn provider_budget_shortfall(snapshot: &Snapshot, close: &CloseRecord) -> Option<u64> {
    let config = snapshot.config.as_ref()?;
    let fee = snapshot.provider_fee?;
    let rent = snapshot
        .rent
        .minimum_balance(usize::from(config.pending_request_bytes));
    let holdback = promotion_holdback(close).ok()?;
    let needed = fee.checked_add(rent)?.checked_add(holdback)?;
    let available = close
        .locked_sponsor_remaining
        .checked_add(close.locked_owner_remaining)?;
    let extra = needed
        .saturating_sub(available)
        .max(fee.saturating_sub(close.approved_provider_fee_cap));
    (extra > 0).then_some(extra)
}

/// `increase_provider_budget`, planned right before `request_round_draw`: only when the operator opted in
/// (`max_provider_topup_lamports`; the extra is FREE SPONSOR operations funds, `ops.free_balance`, moved into
/// the close's locked sponsor partition, so the flag is a sponsor budget decision and not a fee-key payment),
/// the request would be refused for want of reserve ([`provider_budget_shortfall`]), the free operations
/// balance can cover the shortfall, and the whole shortfall is within the opted-in maximum.  Never partial: a
/// partial top-up would repeat on the next pass and exceed the maximum in total.
fn increase_budget_step(snapshot: &Snapshot, close: &CloseRecord, limits: &Limits) -> Option<Step> {
    if limits.max_provider_topup_lamports == 0 {
        return None;
    }
    let extra = provider_budget_shortfall(snapshot, close)?;
    let ops = snapshot.ops.as_ref()?;
    if extra > limits.max_provider_topup_lamports || extra > ops.free_balance {
        return None;
    }
    let accounts = accounts::IncreaseProviderBudget {
        pool: pool_address(),
        close_record: close_record_address(close.close_id),
        ops: ops_address(),
        ops_cash: ops_cash_address(),
    }
    .to_account_metas(None);
    Some(step(
        StepKind::IncreaseProviderBudget {
            close_id: close.close_id,
            extra,
        },
        0,
        accounts,
        Vec::new(),
    ))
}

/// What `sync_ops` would compute from the physical OpsCash account: `(unallocated_excess, rent floor)`, or
/// `None` when the handler would reject the call (OpsCash missing, not a plain system account, below its own
/// rent floor, or holding less than free + locked) or the books were not read.
fn synced_books(snapshot: &Snapshot) -> Option<(u64, u64)> {
    let ops = snapshot.ops.as_ref()?;
    let cash = snapshot.ops_cash.as_ref()?;
    if !cash.plain_system_account {
        return None;
    }
    let floor = snapshot.rent.minimum_balance(0);
    let available = cash.lamports.checked_sub(floor)?;
    let owned = ops.free_balance.checked_add(ops.locked_remaining_total)?;
    Some((available.checked_sub(owned)?, floor))
}

/// `reconcile_ops_cash` / `reconcile_ops_cash_with_client` when the books differ from the physical accounts
/// and the handler would accept the call (it runs `sync_ops` first, so books that would fail it block both).
/// The Client variant is planned when the ORAO Client observation differs (a moved balance, a changed rent
/// floor, or a deficit computed from a newer reimbursement budget); it also covers the OpsCash books.  Otherwise
/// the plain variant when `unallocated_excess` or the protected-rent floor differ.  A balance difference below
/// `min_reconcile_lamports` (a dust donation) is not worth a fee; a changed rent floor or deficit always is.
fn reconcile_step(snapshot: &Snapshot, limits: &Limits) -> Option<Step> {
    let (excess, floor) = synced_books(snapshot)?;
    let ops = snapshot.ops.as_ref()?;
    let minimum = limits.min_reconcile_lamports.max(1);

    if let Some(client) = snapshot.client {
        let client_rent = snapshot.rent.minimum_balance(client.data_len);
        let deficit = provider_deficit(ops.eligible_return_budget, client.lamports, client_rent);
        if client.lamports.abs_diff(ops.client_balance_observed) >= minimum
            || client_rent != ops.client_protected_rent
            || deficit != ops.provider_deficit
        {
            let accounts = accounts::ReconcileOpsCashWithClient {
                ops: ops_address(),
                ops_cash: ops_cash_address(),
                client: provider::client_address().0,
            }
            .to_account_metas(None);
            return Some(step(
                StepKind::ReconcileOpsCashWithClient {
                    observed_excess: ops.unallocated_excess,
                    observed_client_balance: ops.client_balance_observed,
                },
                0,
                accounts,
                Vec::new(),
            ));
        }
    }
    if excess.abs_diff(ops.unallocated_excess) >= minimum || floor != ops.protected_rent {
        let accounts = accounts::ReconcileOpsCash {
            ops: ops_address(),
            ops_cash: ops_cash_address(),
        }
        .to_account_metas(None);
        return Some(step(
            StepKind::ReconcileOpsCash {
                observed_excess: ops.unallocated_excess,
                observed_protected_rent: ops.protected_rent,
            },
            0,
            accounts,
            Vec::new(),
        ));
    }
    None
}

/// The address a shared page of this kind and index must have (never a caller-chosen one).
fn shared_page_address(kind: &PageKind, index: u64) -> Option<Pubkey> {
    match *kind {
        PageKind::Participant { round } => Some(participant_address(round, index)),
        PageKind::Candidate { close_id, set_tag } => {
            Some(candidate_address(close_id, set_tag, index))
        }
        PageKind::Registry | PageKind::Snapshot => None,
    }
}

/// The program's `closable` predicate: registry and snapshot pages are permanent; a participant page closes
/// once its round's close record is finalized; a candidate page once its own close's promotion is terminal.
fn page_is_closable(last: &CloseRecord, kind: &PageKind) -> bool {
    match *kind {
        PageKind::Registry | PageKind::Snapshot => false,
        PageKind::Participant { round } => last.round_id == round && last.finalized,
        PageKind::Candidate { close_id, .. } => {
            last.close_id == close_id && last.promotion != Promotion::Pending
        }
    }
}

/// Whether `close_shared_account` on `page` would succeed: closable, the page address is the derived one, it
/// holds at least the components it recorded, and (when an owner share is owed) the destination THE PAGE recorded
/// is a plain non-OpsCash account that can take the credit (the handler has no deliverability preflight of its
/// own, and the runtime would reject the transfer).
fn page_can_be_closed(
    snapshot: &Snapshot,
    last: &CloseRecord,
    page: &SharedPage,
    floor: u64,
) -> bool {
    if !page_is_closable(last, &page.kind)
        || shared_page_address(&page.kind, page.index) != Some(page.address)
    {
        return false;
    }
    let Some(components) = page.rent.ops.checked_add(page.rent.owner) else {
        return false;
    };
    if page.lamports < components {
        return false;
    }
    if page.rent.owner == 0 {
        return true;
    }
    let destination = page.rent.owner_refund;
    snapshot
        .page_refund_targets
        .get(&destination)
        .is_some_and(|target| {
            target.plain_system_account
                && destination != ops_cash_address()
                && credit_is_deliverable(target.lamports, page.rent.owner, floor)
        })
}

/// `close_shared_account` for the first released shared page of the LAST finished close (older closes'
/// leftovers need a wallet), one page per transaction.  The handler runs `sync_ops` first, so books that would
/// fail it block the cleanup.  The refund slot holds the destination the page recorded whenever an owner share
/// is owed; with no owner share nothing is paid to it and OpsCash (already writable in the instruction) fills
/// it, because the recorded field may be the default key and a read-only demoted account fails Anchor's `mut`.
fn cleanup_step(snapshot: &Snapshot) -> Option<Step> {
    let last = snapshot.last_close.as_ref()?;
    let (_, floor) = synced_books(snapshot)?;
    let page = snapshot
        .shared_pages
        .iter()
        .find(|page| page_can_be_closed(snapshot, last, page, floor))?;
    let owner_refund = if page.rent.owner > 0 {
        page.rent.owner_refund
    } else {
        ops_cash_address()
    };
    let accounts = accounts::CloseSharedAccount {
        page: page.address,
        ops: ops_address(),
        ops_cash: ops_cash_address(),
        owner_refund,
        close_record: close_record_address(last.close_id),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    Some(step(
        StepKind::CloseSharedAccount {
            close_id: last.close_id,
            page: page.kind,
            index: page.index,
        },
        0,
        accounts,
        Vec::new(),
    ))
}

// ---------------------------------------------------------------------------
// One-row fallback and the compute budget
// ---------------------------------------------------------------------------

/// The fallback for a batch the cluster refuses on page / compute / size /
/// duplicate-account grounds: the same step restricted to its FIRST row.  A
/// step with at most one row is returned unchanged.
pub fn degrade_to_one_row(step: &Step) -> Step {
    if step.rows <= 1 {
        return step.clone();
    }
    let mut one = step.clone();
    one.rows = 1;
    one.row_accounts.truncate(1);
    one
}

/// Simulates a step as it would be sent (compute-budget instruction first).
pub trait Simulator {
    fn simulate_step(&self, step: &Step) -> Result<SimOutcome, RpcError>;
}

/// The verdict of [`ensure_budget`].
#[derive(Debug, Clone, PartialEq)]
pub enum Budgeted {
    /// Send this step (possibly degraded to one row); `units` is what its simulation consumed.
    Ready { step: Step, units: u64 },
    /// Do not send: the simulation fails deterministically (or even one row exceeds the limit).
    Blocked { step: Step, reason: String },
}

/// Simulate `step`; when the simulation consumes more units than the step's limit, or fails on a
/// page / compute / size / duplicate-account ground that fewer rows cure, replace the step with its
/// one-row fallback and simulate again.  Every other failure is final: nothing is signed.
pub fn ensure_budget(step: Step, sim: &dyn Simulator) -> Result<Budgeted, RpcError> {
    let limit = u64::from(step.cu_limit);
    let reason = match sim.simulate_step(&step)? {
        SimOutcome::Ok { units } if units <= limit => return Ok(Budgeted::Ready { step, units }),
        SimOutcome::Ok { units } => {
            format!("the simulation used {units} compute units, over the {limit} limit")
        }
        SimOutcome::Failed { kind, reason } if kind.is_shrinkable() => {
            format!("the simulation failed ({kind:?}): {reason}")
        }
        SimOutcome::Failed { kind, reason } => {
            return Ok(Budgeted::Blocked {
                step,
                reason: format!("the simulation failed ({kind:?}): {reason}"),
            });
        }
    };
    if step.rows <= 1 {
        return Ok(Budgeted::Blocked { step, reason });
    }
    let small = degrade_to_one_row(&step);
    Ok(match sim.simulate_step(&small)? {
        SimOutcome::Ok { units } if units <= limit => Budgeted::Ready { step: small, units },
        SimOutcome::Ok { units } => Budgeted::Blocked {
            step: small,
            reason: format!(
                "even one row used {units} compute units, over the {limit} limit ({reason})"
            ),
        },
        SimOutcome::Failed {
            kind,
            reason: failure,
        } => Budgeted::Blocked {
            step: small,
            reason: format!("even one row failed in simulation ({kind:?}): {failure} ({reason})"),
        },
    })
}

// ---------------------------------------------------------------------------
// Fee-only guard
// ---------------------------------------------------------------------------

/// Discriminators of every instruction the worker may send.
fn keeper_discriminators() -> [&'static [u8]; 20] {
    [
        instruction::DiscoverRows::DISCRIMINATOR,
        instruction::ApplySenior::DISCRIMINATOR,
        instruction::SealWeights::DISCRIMINATOR,
        instruction::AllocateRows::DISCRIMINATOR,
        instruction::PruneRows::DISCRIMINATOR,
        instruction::CommitTicket::DISCRIMINATOR,
        instruction::ProvisionPage::DISCRIMINATOR,
        instruction::ProvisionCloseRecord::DISCRIMINATOR,
        instruction::BuildCandidates::DISCRIMINATOR,
        instruction::RequestRoundDraw::DISCRIMINATOR,
        instruction::CaptureRoundDraw::DISCRIMINATOR,
        instruction::ApplyPromotion::DISCRIMINATOR,
        instruction::ExpirePromotion::DISCRIMINATOR,
        instruction::FinalizeRound::DISCRIMINATOR,
        instruction::DeliverOwnerRefund::DISCRIMINATOR,
        instruction::ObserveRequestFulfilled::DISCRIMINATOR,
        instruction::IncreaseProviderBudget::DISCRIMINATOR,
        instruction::ReconcileOpsCash::DISCRIMINATOR,
        instruction::ReconcileOpsCashWithClient::DISCRIMINATOR,
        instruction::CloseSharedAccount::DISCRIMINATOR,
    ]
}

/// Assert a built transaction is fee-only: its program instructions are the compute-budget program
/// and this program's permissionless keeper instructions, nothing else (no system transfer, no owner /
/// governance / guardian / sponsor instruction), and the only signer is the fee payer, in at most one
/// writable `payer` slot.  (The fee payer's key may legitimately also appear as a NON-signer account,
/// for example as a close's recorded refund destination.)
pub fn assert_fee_only(instructions: &[Instruction], fee_payer: &Pubkey) -> Result<(), PlanError> {
    let allowed = keeper_discriminators();
    let mut payer_slots = 0usize;
    for ix in instructions {
        if ix.program_id == compute_budget::ID {
            if !ix.accounts.is_empty() {
                return Err(PlanError::NotFeeOnly(
                    "a compute-budget instruction with accounts".to_string(),
                ));
            }
            continue;
        }
        if ix.program_id != PROGRAM_ID {
            return Err(PlanError::NotFeeOnly(format!(
                "an instruction for program {}",
                ix.program_id
            )));
        }
        let discriminator = ix.data.get(..8).ok_or_else(|| {
            PlanError::NotFeeOnly("an instruction without a discriminator".to_string())
        })?;
        if !allowed.contains(&discriminator) {
            return Err(PlanError::NotFeeOnly(
                "an instruction that is not a keeper instruction".to_string(),
            ));
        }
        for meta in &ix.accounts {
            if meta.is_signer {
                if meta.pubkey != *fee_payer {
                    return Err(PlanError::NotFeeOnly(format!(
                        "signer {} is not the fee payer",
                        meta.pubkey
                    )));
                }
                if !meta.is_writable {
                    return Err(PlanError::NotFeeOnly(
                        "the fee payer slot is not writable".to_string(),
                    ));
                }
                payer_slots += 1;
            }
        }
    }
    if payer_slots > 1 {
        return Err(PlanError::NotFeeOnly(format!(
            "the fee payer signs {payer_slots} account slots"
        )));
    }
    Ok(())
}
