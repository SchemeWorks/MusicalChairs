//! M4 provisioning and cleanup suite.
//!
//! Every row runs the actual compiled Musical Chairs SBF in LiteSVM against the
//! canonical local fixture (`FinancialRuntime::new`), optionally stripped to the
//! exact post-`initialize` page shape.  Rows exercise `provision_page`,
//! `provision_close_record`, `close_shared_account` and `reconcile_ops_cash`.
//!
//! Labels are `financial.provisioning.<row>` for a row's headline observation
//! and `financial.provisioning.<row>.<variant>` for the supporting ones.  Every
//! rejection asserts the exact program error code parsed from
//! `Observation.error` and fee-only rollback; every success asserts both
//! cash-register conservation invariants and the exact lamport movement,
//! reading the rent from the runtime (`svm.minimum_balance_for_rent_exemption`
//! of the program's own `T::SPACE`), never from a constant.
//!
//! Money movement is measured from account balances before and after the
//! transaction rather than from `Observation.account_changes`, because the
//! latter reports a zero delta for an account the runtime deleted.
//!
//! The rent figures these rows measure are LiteSVM's default rent schedule (the
//! 6,960 lamports per byte-year coefficient), not the mainnet schedule; the
//! `rent-table` observation records them under that label.

use std::collections::BTreeSet;

use anchor_lang::AnchorSerialize;
use musical_chairs_program::{
    accounts, instruction, required_close_reserve, state, ErrorCode, Funding, PageKind, ID,
};
use solana_sdk::{
    account::Account,
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::controls::{self, custom_code};
use crate::runtime::{
    anchor_ix, pda, FinancialRuntime, Observation, ATTACKER_SEED, OWNER_SEED, SPONSOR_SEED,
};

const CU: u32 = 1_400_000;
/// Operations cash for rows that pay rent from `free_balance`: far more than
/// the few pages (a few million lamports each) any one row creates.
const FUND: u64 = 200_000_000;
/// A pre-credit on a page PDA that is far below any rent floor.
const PRECREDIT: u64 = 1_000;
/// A fixture lock that Free and Caller funding must leave alone.
const LOCK: u64 = 1_234_567;

pub const ROW_SNAPSHOT_SEQUENTIAL: &str = "provision_snapshot_sequential_only";
pub const ROW_FROM_OPS: &str = "provision_from_ops_debits_free_balance_by_exact_rent";
pub const ROW_BY_CALLER: &str =
    "provision_by_caller_records_owner_component_and_refund_destination";
pub const ROW_CLOSE_SNAPSHOT_REGISTRY: &str =
    "close_snapshot_and_registry_pages_always_refused_not_closable";
pub const ROW_COUNTER_MONOTONE: &str = "snapshot_pages_counter_never_decrements";
pub const ROW_CLOSE_CANDIDATE: &str =
    "close_candidate_page_refused_until_its_own_close_record_is_terminal";
pub const ROW_CLOSE_PARTICIPANT: &str = "close_participant_page_refused_until_that_round_finalized";
pub const ROW_CANDIDATE_REFUNDS: &str = "close_candidate_page_refunds_components_once";
pub const ROW_CLOSE_CLOSE_RECORD: &str = "close_close_record_always_refused";
pub const ROW_CLOSE_RECORD_NEXT: &str = "provision_close_record_requires_next_close_id";
pub const ROW_CANDIDATE_RESERVE: &str =
    "provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only";
pub const ROW_LOCKED_DEBITS: &str =
    "provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent";
pub const ROW_LOCKED_REJECTS: &str =
    "provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice";
pub const ROW_LOCKED_BEYOND: &str =
    "provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set";
pub const ROW_NEED_BOUND: &str = "free_funding_respects_need_bound";
pub const ROW_RECONCILE: &str =
    "reconcile_ops_cash_classifies_anonymous_credit_as_unallocated_excess";
pub const ROW_RENT_TABLE: &str = "rent-table";

/// Every row, in suite order.
pub const ROW_NAMES: [&str; 17] = [
    ROW_SNAPSHOT_SEQUENTIAL,
    ROW_FROM_OPS,
    ROW_BY_CALLER,
    ROW_CLOSE_SNAPSHOT_REGISTRY,
    ROW_COUNTER_MONOTONE,
    ROW_CLOSE_CANDIDATE,
    ROW_CLOSE_PARTICIPANT,
    ROW_CANDIDATE_REFUNDS,
    ROW_CLOSE_CLOSE_RECORD,
    ROW_CLOSE_RECORD_NEXT,
    ROW_CANDIDATE_RESERVE,
    ROW_LOCKED_DEBITS,
    ROW_LOCKED_REJECTS,
    ROW_LOCKED_BEYOND,
    ROW_NEED_BOUND,
    ROW_RECONCILE,
    ROW_RENT_TABLE,
];

/// The whole provisioning suite as one observation list.  Every row asserts its
/// own expectations (and panics with the offending observation on a violation).
pub fn run_provisioning_suite() -> Vec<Observation> {
    let rows: [fn() -> Vec<Observation>; 17] = [
        provision_snapshot_sequential_only,
        provision_from_ops_debits_free_balance_by_exact_rent,
        provision_by_caller_records_owner_component_and_refund_destination,
        close_snapshot_and_registry_pages_always_refused_not_closable,
        snapshot_pages_counter_never_decrements,
        close_candidate_page_refused_until_its_own_close_record_is_terminal,
        close_participant_page_refused_until_that_round_finalized,
        close_candidate_page_refunds_components_once,
        close_close_record_always_refused,
        provision_close_record_requires_next_close_id,
        provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only,
        provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent,
        provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice,
        provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set,
        free_funding_respects_need_bound,
        reconcile_ops_cash_classifies_anonymous_credit_as_unallocated_excess,
        rent_table,
    ];
    let mut observations = Vec::new();
    for (row, name) in rows.into_iter().zip(ROW_NAMES) {
        let produced = row();
        assert_headline(&produced, name);
        controls::assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    controls::assert_unique_labels(&observations);
    observations
}

/// A row must contain an observation labelled exactly
/// `financial.provisioning.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = label(row, "");
    assert!(
        observations
            .iter()
            .any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

pub use controls::{assert_outcomes_consistent, assert_unique_labels};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn label(row: &str, variant: &str) -> String {
    if variant.is_empty() {
        format!("financial.provisioning.{row}")
    } else {
        format!("financial.provisioning.{row}.{variant}")
    }
}

/// Anchor numbers `#[error_code]` variants from 6000 in declaration order.
fn code(error: ErrorCode) -> u32 {
    error as u32 + 6000
}

fn ser<T: AnchorSerialize>(value: &T) -> Vec<u8> {
    value.try_to_vec().expect("Borsh serialization")
}

fn keys(list: &[Pubkey]) -> BTreeSet<String> {
    list.iter().map(ToString::to_string).collect()
}

/// The accounts whose lamports moved in the transaction (the fee payer always
/// does).  Data-only changes are asserted through typed state reads instead.
fn moved(observation: &Observation) -> BTreeSet<String> {
    observation
        .account_changes
        .iter()
        .filter(|change| change.lamport_delta != 0)
        .map(|change| change.pubkey.clone())
        .collect()
}

fn attacker() -> Keypair {
    Keypair::new_from_array(ATTACKER_SEED)
}
fn owner() -> Keypair {
    Keypair::new_from_array(OWNER_SEED)
}
fn sponsor() -> Keypair {
    Keypair::new_from_array(SPONSOR_SEED)
}

/// Lamports on `key`, zero when the account does not exist (a closed page).
fn lamports(rt: &FinancialRuntime, key: Pubkey) -> u64 {
    rt.svm
        .get_account(&key)
        .map_or(0, |account| account.lamports)
}

/// A page that was closed: gone, or an emptied system-owned shell.
fn is_closed(rt: &FinancialRuntime, key: Pubkey) -> bool {
    match rt.svm.get_account(&key) {
        None => true,
        Some(account) => {
            account.lamports == 0 && account.data.is_empty() && account.owner == system_program::ID
        }
    }
}

fn terminal() -> state::Promotion {
    state::Promotion::Skipped {
        reason: 0,
        stage: 0,
    }
}

/// A `CloseRecord` fixture value for `close_id` (canonical bump, otherwise the
/// pending default).
fn fresh_record(close_id: u64) -> state::CloseRecord {
    state::CloseRecord {
        bump: pda(&[state::CLOSE_SEED, &close_id.to_le_bytes()]).1,
        close_id,
        ..state::CloseRecord::default()
    }
}

fn candidate(close_id: u64, set_tag: u8) -> PageKind {
    PageKind::Candidate { close_id, set_tag }
}

fn kind_name(kind: PageKind) -> String {
    match kind {
        PageKind::Registry => "registry".into(),
        PageKind::Snapshot => "snapshot".into(),
        PageKind::Participant { round } => format!("participant{round}"),
        PageKind::Candidate { close_id, set_tag } => format!("candidate{close_id}_set{set_tag}"),
    }
}

fn page_space(kind: PageKind) -> usize {
    match kind {
        PageKind::Registry => state::RegistryPage::SPACE,
        PageKind::Snapshot => state::SnapshotPage::SPACE,
        PageKind::Participant { .. } => state::ParticipantPage::SPACE,
        PageKind::Candidate { .. } => state::CandidatePage::SPACE,
    }
}

/// The page PDA and bump for `kind`/`index`, derived independently of the
/// runtime's address helpers from the documented seeds.
fn page_pda(kind: PageKind, index: u64) -> (Pubkey, u8) {
    let index_seed = index.to_le_bytes().to_vec();
    let seeds: Vec<Vec<u8>> = match kind {
        PageKind::Registry => vec![state::REGISTRY_SEED.to_vec(), index_seed],
        PageKind::Snapshot => vec![state::SNAPSHOT_SEED.to_vec(), index_seed],
        PageKind::Participant { round } => vec![
            state::PARTICIPANTS_SEED.to_vec(),
            round.to_le_bytes().to_vec(),
            index_seed,
        ],
        PageKind::Candidate { close_id, set_tag } => vec![
            state::CANDIDATES_SEED.to_vec(),
            close_id.to_le_bytes().to_vec(),
            vec![set_tag],
            index_seed,
        ],
    };
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    Pubkey::find_program_address(&refs, &ID)
}

fn page_key(kind: PageKind, index: u64) -> Pubkey {
    page_pda(kind, index).0
}

/// The rent components stored in a provisioned page.
fn page_rent(rt: &FinancialRuntime, kind: PageKind, index: u64) -> state::RentComponents {
    let key = page_key(kind, index);
    match kind {
        PageKind::Registry => rt.deserialize::<state::RegistryPage>(key).rent,
        PageKind::Snapshot => rt.deserialize::<state::SnapshotPage>(key).rent,
        PageKind::Participant { .. } => rt.deserialize::<state::ParticipantPage>(key).rent,
        PageKind::Candidate { .. } => rt.deserialize::<state::CandidatePage>(key).rent,
    }
}

fn components(ops: u64, owner: u64, owner_refund: Pubkey) -> state::RentComponents {
    state::RentComponents {
        ops,
        owner,
        owner_refund,
    }
}

fn free_components(rent: u64) -> state::RentComponents {
    components(rent, 0, Pubkey::default())
}

/// A freshly created page: program-owned, full size, exactly rent exempt, blank
/// rows, identity fields matching its seeds, and the expected rent components.
fn assert_new_page(rt: &FinancialRuntime, kind: PageKind, index: u64, rent: state::RentComponents) {
    let (key, bump) = page_pda(kind, index);
    let account = rt.account(key);
    let space = page_space(kind);
    assert_eq!(account.owner, ID, "{kind:?}[{index}]: program-owned");
    assert_eq!(
        account.data.len(),
        space,
        "{kind:?}[{index}]: declared size"
    );
    assert_eq!(
        account.lamports,
        rt.rent(space),
        "{kind:?}[{index}]: holds exactly the rent floor for {space} bytes"
    );
    let empty = [Pubkey::default(); state::PAGE_CAPACITY];
    let (actual, expected) = match kind {
        PageKind::Registry => (
            ser(&rt.deserialize::<state::RegistryPage>(key)),
            ser(&state::RegistryPage {
                bump,
                index,
                keys: empty,
                rent,
            }),
        ),
        PageKind::Snapshot => (
            ser(&rt.deserialize::<state::SnapshotPage>(key)),
            ser(&state::SnapshotPage {
                bump,
                index,
                ticket_id: 0,
                rows: [state::SnapshotRow::default(); state::PAGE_CAPACITY],
                rent,
            }),
        ),
        PageKind::Participant { round } => (
            ser(&rt.deserialize::<state::ParticipantPage>(key)),
            ser(&state::ParticipantPage {
                bump,
                round_id: round,
                page: index,
                len: 0,
                rows: empty,
                rent,
            }),
        ),
        PageKind::Candidate { close_id, set_tag } => (
            ser(&rt.deserialize::<state::CandidatePage>(key)),
            ser(&state::CandidatePage {
                bump,
                close_id,
                set_tag,
                page: index,
                len: 0,
                rows: empty,
                rent,
            }),
        ),
    };
    assert_eq!(actual, expected, "{kind:?}[{index}]: page contents");
}

/// The Ops value after a `Free` rent debit.
fn ops_after_free(before: &state::Ops, debit: u64) -> state::Ops {
    let mut expected = before.clone();
    expected.free_balance -= debit;
    expected.total_rent_funded += u128::from(debit);
    expected
}

/// The Ops value after a `LockedClose` rent debit.
fn ops_after_locked(before: &state::Ops, debit: u64) -> state::Ops {
    let mut expected = before.clone();
    expected.locked_remaining_total -= debit;
    expected.total_rent_funded += u128::from(debit);
    expected
}

/// The Pool value after a Registry/Snapshot/Participant page was created.
fn pool_after_page(before: &state::Pool, kind: PageKind) -> state::Pool {
    let mut expected = before.clone();
    match kind {
        PageKind::Registry => expected.registry_pages += 1,
        PageKind::Snapshot => expected.snapshot_pages += 1,
        PageKind::Participant { .. } => expected.participant_pages += 1,
        PageKind::Candidate { .. } => {}
    }
    expected
}

/// Sponsor-first split of a rent debit across a close's locked partitions.
fn split(debit: u64, sponsor_remaining: u64) -> (u64, u64) {
    let sponsor_part = debit.min(sponsor_remaining);
    (sponsor_part, debit - sponsor_part)
}

/// Fixture: `amount` lamports locked for a close, physically held in OpsCash
/// beside the rent floor, so both conservation identities keep holding.
fn lock(rig: &mut Rig, amount: u64) {
    rig.set_ops(|ops| ops.locked_remaining_total += amount);
    let ops_cash = rig.rt.ops_cash_address();
    rig.rt.add_lamports(ops_cash, amount);
    rig.rt.conservation_check();
}

/// Balances a page-close assertion needs, read before and after the call.
#[derive(Clone, Copy)]
struct Money {
    cash: u64,
    free: u64,
    locked: u64,
    excess: u64,
    rent_funded: u128,
    refund: u64,
    page: u64,
}

fn money(rig: &Rig, refund: Pubkey, page: Pubkey) -> Money {
    let ops = rig.ops();
    Money {
        cash: rig.cash(),
        free: ops.free_balance,
        locked: ops.locked_remaining_total,
        excess: ops.unallocated_excess,
        rent_funded: ops.total_rent_funded,
        refund: lamports(&rig.rt, refund),
        page: lamports(&rig.rt, page),
    }
}

/// A successful close: the page is gone, its lamports split exactly into the
/// `ops` component (to `free_balance`), the `owner` component (to the refund
/// destination) and unsolicited excess, and nothing else moved.
fn assert_closed(
    rig: &Rig,
    page: Pubkey,
    refund: Pubkey,
    before: &Money,
    to_free: u64,
    to_owner: u64,
    to_excess: u64,
) {
    let after = money(rig, refund, page);
    assert!(is_closed(&rig.rt, page), "the page is deleted");
    assert_eq!(
        before.page,
        to_free + to_owner + to_excess,
        "the page's lamports are exactly its components plus excess"
    );
    assert_eq!(after.cash, before.cash + to_free + to_excess, "OpsCash");
    assert_eq!(after.free, before.free + to_free, "free_balance");
    assert_eq!(
        after.excess,
        before.excess + to_excess,
        "unallocated_excess"
    );
    assert_eq!(
        after.refund,
        before.refund + to_owner,
        "owner refund destination"
    );
    assert_eq!(after.locked, before.locked, "locked_remaining_total");
    assert_eq!(
        after.rent_funded, before.rent_funded,
        "closing never changes the lifetime rent counter"
    );
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn provision_ix_raw(
    rt: &FinancialRuntime,
    kind: PageKind,
    index: u64,
    funding: Funding,
    payer: Pubkey,
    page: Pubkey,
    close_record: Option<Pubkey>,
) -> Instruction {
    anchor_ix(
        accounts::ProvisionPage {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            page,
            payer,
            close_record,
            system_program: system_program::ID,
        },
        instruction::ProvisionPage {
            kind,
            index,
            funding,
        },
    )
}

/// The `CloseRecord` account a `Candidate` kind needs; none for other kinds
/// (the program id placeholder).
fn close_for(rt: &FinancialRuntime, kind: PageKind) -> Option<Pubkey> {
    match kind {
        PageKind::Candidate { close_id, .. } => Some(rt.close_record_address(close_id)),
        _ => None,
    }
}

fn provision_ix(
    rt: &FinancialRuntime,
    kind: PageKind,
    index: u64,
    funding: Funding,
    payer: Pubkey,
) -> Instruction {
    provision_ix_raw(
        rt,
        kind,
        index,
        funding,
        payer,
        page_key(kind, index),
        close_for(rt, kind),
    )
}

fn provision_close_record_ix(
    rt: &FinancialRuntime,
    close_id: u64,
    funding: Funding,
    payer: Pubkey,
    close_record: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::ProvisionCloseRecord {
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            close_record,
            payer,
            system_program: system_program::ID,
        },
        instruction::ProvisionCloseRecord { close_id, funding },
    )
}

fn close_ix(
    rt: &FinancialRuntime,
    kind: PageKind,
    page: Pubkey,
    owner_refund: Pubkey,
    close_record: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::CloseSharedAccount {
            page,
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            owner_refund,
            close_record,
            system_program: system_program::ID,
        },
        instruction::CloseSharedAccount { kind },
    )
}

fn reconcile_ix(rt: &FinancialRuntime, ops_cash: Pubkey) -> Instruction {
    anchor_ix(
        accounts::ReconcileOpsCash {
            ops: rt.ops_address(),
            ops_cash,
        },
        instruction::ReconcileOpsCash,
    )
}

fn fund_ix(rt: &FinancialRuntime, sender: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::FundOperations {
            sender,
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::FundOperations { amount },
    )
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

struct Rig {
    rt: FinancialRuntime,
}

impl Rig {
    /// The canonical local fixture: registry and snapshot page 0 exist, round 1,
    /// next close id 1.
    fn canonical() -> Self {
        Self {
            rt: FinancialRuntime::new(),
        }
    }

    /// The exact post-`initialize` page shape: no registry or snapshot page.
    fn genesis() -> Self {
        let mut rt = FinancialRuntime::new();
        rt.strip_pages_to_genesis();
        Self { rt }
    }

    fn pool(&self) -> state::Pool {
        self.rt.deserialize(self.rt.pool_address())
    }
    fn ops(&self) -> state::Ops {
        self.rt.deserialize(self.rt.ops_address())
    }
    fn record(&self, close_id: u64) -> state::CloseRecord {
        self.rt.deserialize(self.rt.close_record_address(close_id))
    }
    fn cash(&self) -> u64 {
        lamports(&self.rt, self.rt.ops_cash_address())
    }
    fn payer_key(&self) -> Pubkey {
        self.rt.payer.pubkey()
    }

    fn set_pool(&mut self, change: impl FnOnce(&mut state::Pool)) {
        let mut pool = self.pool();
        change(&mut pool);
        self.rt.replace_pool(pool);
    }
    fn set_ops(&mut self, change: impl FnOnce(&mut state::Ops)) {
        let mut ops = self.ops();
        change(&mut ops);
        self.rt.replace_ops(ops);
    }
    fn set_record(&mut self, close_id: u64, change: impl FnOnce(&mut state::CloseRecord)) {
        let mut record = self.record(close_id);
        change(&mut record);
        self.rt.replace_close_record(record);
    }

    fn exec(
        &mut self,
        label: String,
        instruction: Instruction,
        signers: &[&Keypair],
    ) -> Observation {
        self.rt.execute(label, vec![instruction], signers, CU)
    }

    /// Success: the transaction succeeded and both cash registers conserve.
    fn ok(&self, observation: &Observation) {
        assert!(
            observation.success,
            "{} expected success: {observation:?}",
            observation.label
        );
        self.rt.conservation_check();
    }

    /// Rejection: the exact `Custom(code)` and fee-only rollback.  Conservation
    /// still holds because nothing but the fee changed.
    fn rejected(&self, observation: &Observation, expected: ErrorCode) {
        assert!(
            !observation.success,
            "{} unexpectedly succeeded: {observation:?}",
            observation.label
        );
        assert_eq!(
            custom_code(observation),
            Some(code(expected)),
            "{}: expected {expected:?} (Custom({})), got {:?}",
            observation.label,
            code(expected),
            observation.error
        );
        observation.assert_rollback_except_fee();
        self.rt.conservation_check();
    }

    /// The sponsor funds Operations through the real `fund_operations`.
    fn fund(&mut self, label: String, amount: u64) -> Observation {
        let funder = sponsor();
        let instruction = fund_ix(&self.rt, funder.pubkey(), amount);
        self.exec(label, instruction, &[&funder])
    }

    /// Provision a page.  `payer` `None` is the harness fee payer (no extra
    /// signer); `Some` is an extra signing keypair that is the instruction's
    /// `payer` account.
    fn provision(
        &mut self,
        label: String,
        kind: PageKind,
        index: u64,
        funding: Funding,
        payer: Option<&Keypair>,
    ) -> Observation {
        let payer_key = payer.map_or(self.payer_key(), |keypair| keypair.pubkey());
        let instruction = provision_ix(&self.rt, kind, index, funding, payer_key);
        let signers: Vec<&Keypair> = payer.into_iter().collect();
        self.exec(label, instruction, &signers)
    }

    fn provision_record(
        &mut self,
        label: String,
        close_id: u64,
        funding: Funding,
        payer: Option<&Keypair>,
    ) -> Observation {
        let payer_key = payer.map_or(self.payer_key(), |keypair| keypair.pubkey());
        let record = self.rt.close_record_address(close_id);
        let instruction = provision_close_record_ix(&self.rt, close_id, funding, payer_key, record);
        let signers: Vec<&Keypair> = payer.into_iter().collect();
        self.exec(label, instruction, &signers)
    }

    /// `close_shared_account(kind)` on `page`, judged by `CloseRecord(close_id)`.
    fn close_page(
        &mut self,
        label: String,
        kind: PageKind,
        page: Pubkey,
        owner_refund: Pubkey,
        close_id: u64,
    ) -> Observation {
        let record = self.rt.close_record_address(close_id);
        let instruction = close_ix(&self.rt, kind, page, owner_refund, record);
        self.exec(label, instruction, &[])
    }

    /// Provision the next page of `kind` and require success; returns the
    /// observation.
    fn provision_ok(
        &mut self,
        label: String,
        kind: PageKind,
        index: u64,
        funding: Funding,
        payer: Option<&Keypair>,
    ) -> Observation {
        let observation = self.provision(label, kind, index, funding, payer);
        self.ok(&observation);
        observation
    }

    /// A rejected provision leaves Pool, Ops, OpsCash and any existing page at
    /// `index` byte-identical (beyond the fee, which `rejected` checks).
    fn provision_rejected(
        &mut self,
        label: String,
        kind: PageKind,
        index: u64,
        funding: Funding,
        payer: Option<&Keypair>,
        expected: ErrorCode,
    ) -> Observation {
        let pool = ser(&self.pool());
        let ops = ser(&self.ops());
        let cash = self.cash();
        let observation = self.provision(label, kind, index, funding, payer);
        self.rejected(&observation, expected);
        assert_eq!(
            ser(&self.pool()),
            pool,
            "{}: Pool unchanged",
            observation.label
        );
        assert_eq!(
            ser(&self.ops()),
            ops,
            "{}: Ops unchanged",
            observation.label
        );
        assert_eq!(
            self.cash(),
            cash,
            "{}: OpsCash unchanged",
            observation.label
        );
        observation
    }
}

// ---------------------------------------------------------------------------
// Rows: Registry / Snapshot provisioning
// ---------------------------------------------------------------------------

pub fn provision_snapshot_sequential_only() -> Vec<Observation> {
    let row = ROW_SNAPSHOT_SEQUENTIAL;
    let mut rig = Rig::genesis();
    let caller = attacker();
    let mut observations = Vec::new();

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    assert_eq!(
        rig.pool().snapshot_pages,
        0,
        "genesis: no snapshot page exists"
    );
    assert_eq!(
        rig.pool().registry_pages,
        0,
        "genesis: no registry page exists"
    );

    // Index 1 before index 0: sequential-only.
    let observation = rig.provision_rejected(
        label(row, "index_1_before_0"),
        PageKind::Snapshot,
        1,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    assert!(
        rig.rt
            .svm
            .get_account(&page_key(PageKind::Snapshot, 1))
            .is_none(),
        "the out-of-order page was not created"
    );
    observations.push(observation);

    // The address of index 1 passed for index 0: the PDA check comes first.
    let pool_before = ser(&rig.pool());
    let instruction = provision_ix_raw(
        &rig.rt,
        PageKind::Snapshot,
        0,
        Funding::Free,
        rig.payer_key(),
        page_key(PageKind::Snapshot, 1),
        None,
    );
    let observation = rig.exec(
        label(row, "address_of_index_1_for_index_0"),
        instruction,
        &[],
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    assert_eq!(ser(&rig.pool()), pool_before);
    observations.push(observation);

    // Index 0 succeeds (headline).
    let rent = rig.rt.rent(state::SnapshotPage::SPACE);
    let pool_before = rig.pool();
    let ops_before = rig.ops();
    let observation = rig.provision_ok(label(row, ""), PageKind::Snapshot, 0, Funding::Free, None);
    assert_new_page(&rig.rt, PageKind::Snapshot, 0, free_components(rent));
    assert_eq!(rig.pool().snapshot_pages, 1);
    assert_eq!(
        ser(&rig.pool()),
        ser(&pool_after_page(&pool_before, PageKind::Snapshot))
    );
    assert_eq!(ser(&rig.ops()), ser(&ops_after_free(&ops_before, rent)));
    observations.push(observation);

    // Index 0 again: the counter has moved on.
    let observation = rig.provision_rejected(
        label(row, "index_0_again"),
        PageKind::Snapshot,
        0,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);

    // Even with the counter rewound, an existing page is never re-created or
    // paid for twice: the fresh-account guard is an independent line of defence.
    rig.set_pool(|pool| pool.snapshot_pages = 0);
    let observation = rig.provision_rejected(
        label(row, "index_0_again_counter_rewound"),
        PageKind::Snapshot,
        0,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    rig.set_pool(|pool| pool.snapshot_pages = 1);

    // Index 1 now succeeds.  Caller funding is used because Free funding of a
    // page beyond the registry's extent is bounded by need (see
    // `free_funding_respects_need_bound`).
    let pool_before = rig.pool();
    let observation = rig.provision_ok(
        label(row, "index_1"),
        PageKind::Snapshot,
        1,
        Funding::Caller,
        Some(&caller),
    );
    assert_new_page(
        &rig.rt,
        PageKind::Snapshot,
        1,
        components(0, rent, caller.pubkey()),
    );
    assert_eq!(rig.pool().snapshot_pages, 2);
    assert_eq!(
        ser(&rig.pool()),
        ser(&pool_after_page(&pool_before, PageKind::Snapshot))
    );
    observations.push(observation);

    // The registry kind is sequential in the same way and has its own counter.
    let observation = rig.provision_rejected(
        label(row, "registry_index_1_before_0"),
        PageKind::Registry,
        1,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    let registry_rent = rig.rt.rent(state::RegistryPage::SPACE);
    let pool_before = rig.pool();
    let observation = rig.provision_ok(
        label(row, "registry_index_0"),
        PageKind::Registry,
        0,
        Funding::Free,
        None,
    );
    assert_new_page(
        &rig.rt,
        PageKind::Registry,
        0,
        free_components(registry_rent),
    );
    assert_eq!(rig.pool().registry_pages, 1);
    assert_eq!(
        rig.pool().snapshot_pages,
        2,
        "the snapshot counter is independent"
    );
    assert_eq!(
        ser(&rig.pool()),
        ser(&pool_after_page(&pool_before, PageKind::Registry))
    );
    observations.push(observation);
    observations
}

pub fn provision_from_ops_debits_free_balance_by_exact_rent() -> Vec<Observation> {
    let row = ROW_FROM_OPS;
    let mut rig = Rig::genesis();
    let mut observations = Vec::new();
    let rent = rig.rt.rent(state::SnapshotPage::SPACE);
    let registry_rent = rig.rt.rent(state::RegistryPage::SPACE);
    assert_ne!(
        rent, registry_rent,
        "two page kinds have two different rents"
    );

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    lock(&mut rig, LOCK);

    let pool = rig.pool();
    let ops_before = rig.ops();
    let cash_before = rig.cash();
    let page = page_key(PageKind::Snapshot, 0);
    let observation = rig.provision_ok(label(row, ""), PageKind::Snapshot, 0, Funding::Free, None);
    let ops = rig.ops();
    assert_eq!(
        ops.free_balance,
        FUND - rent,
        "free_balance drops by exactly the rent"
    );
    assert_eq!(
        ops.total_rent_funded,
        ops_before.total_rent_funded + u128::from(rent),
        "Free adds the debit to total_rent_funded"
    );
    assert_eq!(
        ops.locked_remaining_total, LOCK,
        "Free never touches the lock"
    );
    assert_eq!(ops.unallocated_excess, ops_before.unallocated_excess);
    assert_eq!(ser(&ops), ser(&ops_after_free(&ops_before, rent)));
    assert_eq!(
        rig.cash(),
        cash_before - rent,
        "OpsCash pays exactly the rent"
    );
    assert_eq!(
        lamports(&rig.rt, page),
        rent,
        "the page holds exactly the rent"
    );
    assert_new_page(&rig.rt, PageKind::Snapshot, 0, free_components(rent));
    assert_eq!(
        page_rent(&rig.rt, PageKind::Snapshot, 0),
        components(rent, 0, Pubkey::default()),
        "rent == {{ops: rent, owner: 0, owner_refund: default}}"
    );
    assert_eq!(
        ser(&rig.pool()),
        ser(&pool_after_page(&pool, PageKind::Snapshot))
    );
    assert_eq!(
        moved(&observation),
        keys(&[rig.rt.ops_cash_address(), page, rig.payer_key()]),
        "lamports move only from OpsCash to the new page, plus the fee"
    );
    assert_eq!(
        observation.payer_lamports_before - observation.payer_lamports_after,
        observation.expected_fee_lamports,
        "the Free payer pays only the transaction fee"
    );
    observations.push(observation);

    // A second kind of a different size debits its own, different, rent and
    // accumulates in the lifetime counter.
    let ops_before = rig.ops();
    let observation = rig.provision_ok(
        label(row, "registry_page"),
        PageKind::Registry,
        0,
        Funding::Free,
        None,
    );
    assert_eq!(rig.ops().free_balance, FUND - rent - registry_rent);
    assert_eq!(
        rig.ops().total_rent_funded,
        ops_before.total_rent_funded + u128::from(registry_rent)
    );
    assert_eq!(
        rig.ops().total_rent_funded,
        u128::from(rent + registry_rent)
    );
    assert_new_page(
        &rig.rt,
        PageKind::Registry,
        0,
        free_components(registry_rent),
    );
    observations.push(observation);

    // The boundary: `free_balance == rent - 1` is refused, one more lamport is
    // enough, and then `free_balance` is exactly zero.
    let mut tight = Rig::genesis();
    let short = tight.fund(label(row, "boundary.fund_rent_minus_1"), rent - 1);
    tight.ok(&short);
    observations.push(short);
    let observation = tight.provision_rejected(
        label(row, "boundary.rent_minus_1"),
        PageKind::Snapshot,
        0,
        Funding::Free,
        None,
        ErrorCode::InsufficientOperations,
    );
    assert!(
        tight.rt.svm.get_account(&page).is_none(),
        "no page was created"
    );
    observations.push(observation);
    let topped = tight.fund(label(row, "boundary.fund_one_more"), 1);
    tight.ok(&topped);
    observations.push(topped);
    let observation = tight.provision_ok(
        label(row, "boundary.exactly_rent"),
        PageKind::Snapshot,
        0,
        Funding::Free,
        None,
    );
    assert_eq!(tight.ops().free_balance, 0);
    assert_eq!(tight.ops().total_rent_funded, u128::from(rent));
    assert_eq!(
        tight.cash(),
        tight.rt.rent(0),
        "OpsCash is back at its rent floor"
    );
    observations.push(observation);

    // A page PDA that already holds lamports is not blocked, and Ops pays only
    // the shortfall.  The pre-credit later lands in `unallocated_excess` on close.
    let mut pre = Rig::canonical();
    let participant = PageKind::Participant { round: 1 };
    let participant_rent = pre.rt.rent(state::ParticipantPage::SPACE);
    let funded = pre.fund(label(row, "prefunded_page.fund_ops"), FUND);
    pre.ok(&funded);
    observations.push(funded);
    let page = page_key(participant, 0);
    pre.rt.credit_lamports(page, PRECREDIT);
    let ops_before = pre.ops();
    let cash_before = pre.cash();
    let debit = participant_rent - PRECREDIT;
    let observation = pre.provision_ok(
        label(row, "prefunded_page"),
        participant,
        0,
        Funding::Free,
        None,
    );
    assert_eq!(pre.ops().free_balance, ops_before.free_balance - debit);
    assert_eq!(
        pre.ops().total_rent_funded,
        ops_before.total_rent_funded + u128::from(debit),
        "only the shortfall is counted as rent funded"
    );
    assert_eq!(pre.cash(), cash_before - debit, "OpsCash pays rent - 1_000");
    assert_new_page(&pre.rt, participant, 0, free_components(debit));
    assert_eq!(
        page_rent(&pre.rt, participant, 0).ops,
        participant_rent - PRECREDIT,
        "the ops component excludes the pre-credit"
    );
    observations.push(observation);

    // Close it (the round is finalized): the ops component returns to
    // free_balance and the 1,000 pre-credit is classified as excess.
    pre.rt.replace_close_record(state::CloseRecord {
        round_id: 1,
        finalized: true,
        ..fresh_record(1)
    });
    let refund = attacker().pubkey();
    let before = money(&pre, refund, page);
    let observation = pre.close_page(
        label(row, "prefunded_page.close"),
        participant,
        page,
        refund,
        1,
    );
    pre.ok(&observation);
    assert_closed(&pre, page, refund, &before, debit, 0, PRECREDIT);
    assert_eq!(
        pre.ops().unallocated_excess,
        ops_before.unallocated_excess + PRECREDIT,
        "the pre-credit lands in unallocated_excess"
    );
    observations.push(observation);
    observations
}

pub fn provision_by_caller_records_owner_component_and_refund_destination() -> Vec<Observation> {
    let row = ROW_BY_CALLER;
    let mut rig = Rig::genesis();
    let caller = attacker();
    let mut observations = Vec::new();
    assert_ne!(
        caller.pubkey(),
        rig.ops().sponsor,
        "the caller is not the sponsor"
    );
    assert_ne!(caller.pubkey(), rig.payer_key());
    let rent = rig.rt.rent(state::SnapshotPage::SPACE);

    // Operations holds real free cash and a real lock, which Caller funding
    // must not touch.
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    lock(&mut rig, LOCK);

    let page = page_key(PageKind::Snapshot, 0);
    let ops_before = rig.ops();
    let pool_before = rig.pool();
    let cash_before = rig.cash();
    let caller_before = lamports(&rig.rt, caller.pubkey());
    let observation = rig.provision_ok(
        label(row, ""),
        PageKind::Snapshot,
        0,
        Funding::Caller,
        Some(&caller),
    );
    assert_eq!(
        caller_before - lamports(&rig.rt, caller.pubkey()),
        rent,
        "the caller pays exactly the rent"
    );
    assert_eq!(
        observation.payer_lamports_before - observation.payer_lamports_after,
        observation.expected_fee_lamports,
        "the harness fee payer pays only the fee (two signatures)"
    );
    assert_eq!(rig.cash(), cash_before, "OpsCash does not move");
    assert_eq!(
        ser(&rig.ops()),
        ser(&ops_before),
        "Ops is unchanged: free_balance, locked_remaining_total and total_rent_funded"
    );
    assert_eq!(
        rig.ops().free_balance,
        FUND,
        "Caller leaves free_balance alone"
    );
    assert_eq!(
        rig.ops().locked_remaining_total,
        LOCK,
        "Caller leaves the lock alone"
    );
    assert_eq!(
        rig.ops().total_rent_funded,
        0,
        "Caller never touches total_rent_funded"
    );
    assert_eq!(lamports(&rig.rt, page), rent);
    assert_new_page(
        &rig.rt,
        PageKind::Snapshot,
        0,
        components(0, rent, caller.pubkey()),
    );
    assert_eq!(
        page_rent(&rig.rt, PageKind::Snapshot, 0),
        components(0, rent, caller.pubkey()),
        "rent == {{ops: 0, owner: rent, owner_refund: caller}}"
    );
    assert_eq!(
        ser(&rig.pool()),
        ser(&pool_after_page(&pool_before, PageKind::Snapshot))
    );
    assert_eq!(
        moved(&observation),
        keys(&[page, caller.pubkey(), rig.payer_key()]),
        "lamports move only from the caller to the page, plus the fee; OpsCash does not move"
    );
    observations.push(observation);

    // When the harness fee payer is itself the caller it pays rent plus fee and
    // becomes the refund destination.
    let registry_rent = rig.rt.rent(state::RegistryPage::SPACE);
    let payer_before = lamports(&rig.rt, rig.payer_key());
    let ops_before = rig.ops();
    let observation = rig.provision_ok(
        label(row, "fee_payer_as_caller"),
        PageKind::Registry,
        0,
        Funding::Caller,
        None,
    );
    assert_eq!(
        payer_before - lamports(&rig.rt, rig.payer_key()),
        registry_rent + observation.expected_fee_lamports,
        "the fee payer pays the rent and the fee"
    );
    assert_new_page(
        &rig.rt,
        PageKind::Registry,
        0,
        components(0, registry_rent, rig.payer_key()),
    );
    assert_eq!(ser(&rig.ops()), ser(&ops_before));
    observations.push(observation);

    // A different caller on the next page records that caller as the refund.
    let second = owner();
    let observation = rig.provision_ok(
        label(row, "second_caller"),
        PageKind::Snapshot,
        1,
        Funding::Caller,
        Some(&second),
    );
    assert_new_page(
        &rig.rt,
        PageKind::Snapshot,
        1,
        components(0, rent, second.pubkey()),
    );
    observations.push(observation);

    // Caller funding needs no operations cash at all.
    let mut empty = Rig::genesis();
    let observation = empty.provision_ok(
        label(row, "empty_ops"),
        PageKind::Snapshot,
        0,
        Funding::Caller,
        Some(&caller),
    );
    assert_eq!(empty.ops().free_balance, 0);
    assert_new_page(
        &empty.rt,
        PageKind::Snapshot,
        0,
        components(0, rent, caller.pubkey()),
    );
    observations.push(observation);
    observations
}

// ---------------------------------------------------------------------------
// Rows: permanent pages
// ---------------------------------------------------------------------------

/// Pages 0 (Free) and 1, 2 (Caller, paid by `payer`) of Snapshot and Registry,
/// from genesis.  Returns the refund destination recorded for each.
fn permanent_pages(
    rig: &mut Rig,
    row: &str,
    payer: &Keypair,
    observations: &mut Vec<Observation>,
) -> Vec<(PageKind, u64, Pubkey)> {
    let mut pages = Vec::new();
    for kind in [PageKind::Snapshot, PageKind::Registry] {
        for index in 0..3u64 {
            let (funding, signer) = if index == 0 {
                (Funding::Free, None)
            } else {
                (Funding::Caller, Some(payer))
            };
            let observation = rig.provision_ok(
                label(row, &format!("setup.{}_{index}", kind_name(kind))),
                kind,
                index,
                funding,
                signer,
            );
            observations.push(observation);
            let refund = page_rent(&rig.rt, kind, index).owner_refund;
            pages.push((kind, index, refund));
        }
    }
    pages
}

pub fn close_snapshot_and_registry_pages_always_refused_not_closable() -> Vec<Observation> {
    let row = ROW_CLOSE_SNAPSHOT_REGISTRY;
    let mut rig = Rig::genesis();
    let caller = attacker();
    let mut observations = Vec::new();
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let pages = permanent_pages(&mut rig, row, &caller, &mut observations);
    assert_eq!(rig.pool().snapshot_pages, 3);
    assert_eq!(rig.pool().registry_pages, 3);

    // The most permissive record that can exist: round 1 finalized and a
    // terminal promotion.  Only the page kind can make the call fail.
    rig.rt.replace_close_record(state::CloseRecord {
        round_id: 1,
        finalized: true,
        promotion: terminal(),
        ..fresh_record(1)
    });
    let watched: Vec<Pubkey> = pages
        .iter()
        .map(|(kind, index, _)| page_key(*kind, *index))
        .chain([
            rig.rt.close_record_address(1),
            rig.rt.ops_cash_address(),
            rig.rt.ops_address(),
        ])
        .collect();
    let snapshots: Vec<Account> = watched.iter().map(|key| rig.rt.account(*key)).collect();
    let pool_before = ser(&rig.pool());

    let mut first = true;
    for phase in ["no_ticket", "ticket_active"] {
        if phase == "ticket_active" {
            // Fixture: ticket 1 is minted and active.
            rig.set_pool(|pool| {
                pool.active_ticket = 1;
                pool.next_ticket = 2;
            });
            rig.rt.replace_ticket(state::Ticket {
                bump: pda(&[state::TICKET_SEED]).1,
                id: 1,
                phase: state::DISCOVER,
                ..state::Ticket::default()
            });
        }
        // Tail first, then middle, then head.
        for (kind, index, refund) in pages.iter().rev() {
            let position = match *index {
                2 => "tail",
                1 => "middle",
                _ => "head",
            };
            let variant = if first {
                String::new()
            } else {
                format!("{}.{position}.{phase}", kind_name(*kind))
            };
            first = false;
            let refund = if *refund == Pubkey::default() {
                attacker().pubkey()
            } else {
                *refund
            };
            let observation = rig.close_page(
                label(row, &variant),
                *kind,
                page_key(*kind, *index),
                refund,
                1,
            );
            rig.rejected(&observation, ErrorCode::NotClosable);
            observations.push(observation);
        }
    }

    for (key, before) in watched.iter().zip(&snapshots) {
        assert_eq!(
            &rig.rt.account(*key),
            before,
            "{key} is untouched by the refused closes"
        );
    }
    let mut expected_pool: state::Pool = rig.pool();
    expected_pool.active_ticket = 0;
    expected_pool.next_ticket = 1;
    assert_eq!(ser(&expected_pool), pool_before, "no counter changed");
    assert_eq!(rig.pool().snapshot_pages, 3);
    assert_eq!(rig.pool().registry_pages, 3);
    observations
}

pub fn snapshot_pages_counter_never_decrements() -> Vec<Observation> {
    let row = ROW_COUNTER_MONOTONE;
    let mut rig = Rig::genesis();
    let caller = attacker();
    let mut observations = Vec::new();
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);

    let mut counts = vec![rig.pool().snapshot_pages];
    let mut provisioned = 0u64;
    macro_rules! provision_snapshot {
        ($variant:expr, $index:expr, $funding:expr, $signer:expr) => {{
            let observation = rig.provision_ok(
                label(row, $variant),
                PageKind::Snapshot,
                $index,
                $funding,
                $signer,
            );
            provisioned += 1;
            counts.push(rig.pool().snapshot_pages);
            assert_eq!(
                *counts.last().unwrap(),
                provisioned,
                "one page, one increment"
            );
            observations.push(observation);
        }};
    }
    provision_snapshot!("provision_0", 0, Funding::Free, None);
    provision_snapshot!("provision_1", 1, Funding::Caller, Some(&caller));

    rig.rt.replace_close_record(state::CloseRecord {
        round_id: 1,
        finalized: true,
        promotion: terminal(),
        ..fresh_record(1)
    });
    // Refused closes, tail and middle: the counter does not move.
    for (variant, index) in [("", 1u64), ("refused_middle", 0u64)] {
        let observation = rig.close_page(
            label(row, variant),
            PageKind::Snapshot,
            page_key(PageKind::Snapshot, index),
            caller.pubkey(),
            1,
        );
        rig.rejected(&observation, ErrorCode::NotClosable);
        counts.push(rig.pool().snapshot_pages);
        observations.push(observation);
    }

    // Still refused once a ticket is active.
    rig.set_pool(|pool| {
        pool.active_ticket = 1;
        pool.next_ticket = 2;
    });
    let observation = rig.close_page(
        label(row, "refused_with_active_ticket"),
        PageKind::Snapshot,
        page_key(PageKind::Snapshot, 1),
        caller.pubkey(),
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    counts.push(rig.pool().snapshot_pages);
    observations.push(observation);
    rig.set_pool(|pool| {
        pool.active_ticket = 0;
        pool.next_ticket = 1;
    });

    provision_snapshot!("provision_2", 2, Funding::Caller, Some(&caller));
    provision_snapshot!("provision_3", 3, Funding::Caller, Some(&caller));

    // A successful close of another kind does not touch the snapshot counter
    // (it does not even touch Pool).
    let participant = PageKind::Participant { round: 1 };
    let observation = rig.provision_ok(
        label(row, "participant_page"),
        participant,
        0,
        Funding::Free,
        None,
    );
    observations.push(observation);
    let pool_before = ser(&rig.pool());
    let page = page_key(participant, 0);
    let before = money(&rig, caller.pubkey(), page);
    let observation = rig.close_page(
        label(row, "close_participant_page"),
        participant,
        page,
        caller.pubkey(),
        1,
    );
    rig.ok(&observation);
    assert_closed(&rig, page, caller.pubkey(), &before, before.page, 0, 0);
    assert_eq!(
        ser(&rig.pool()),
        pool_before,
        "closing a page leaves Pool byte-identical"
    );
    counts.push(rig.pool().snapshot_pages);
    observations.push(observation);

    provision_snapshot!("provision_4", 4, Funding::Caller, Some(&caller));

    assert!(
        counts.windows(2).all(|pair| pair[0] <= pair[1]),
        "snapshot_pages is monotone: {counts:?}"
    );
    assert_eq!(counts.last(), Some(&5));
    assert_eq!(
        rig.pool().registry_pages,
        0,
        "the registry counter never moved"
    );
    observations
}

// ---------------------------------------------------------------------------
// Rows: closing candidate / participant pages
// ---------------------------------------------------------------------------

pub fn close_candidate_page_refused_until_its_own_close_record_is_terminal() -> Vec<Observation> {
    let row = ROW_CLOSE_CANDIDATE;
    let mut rig = Rig::canonical();
    let preferred = candidate(1, 0);
    let fallback = candidate(1, 1);
    let refund = attacker().pubkey();
    let mut observations = Vec::new();

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let rent = rig.rt.rent(state::CandidatePage::SPACE);
    for (variant, kind) in [
        ("provision_close_record_1", None),
        ("provision_preferred_0", Some(preferred)),
        ("provision_fallback_0", Some(fallback)),
    ] {
        let observation = match kind {
            None => rig.provision_record(label(row, variant), 1, Funding::Free, None),
            Some(kind) => rig.provision(label(row, variant), kind, 0, Funding::Free, None),
        };
        rig.ok(&observation);
        observations.push(observation);
    }
    let page = page_key(preferred, 0);
    let fallback_page = page_key(fallback, 0);
    assert_new_page(&rig.rt, preferred, 0, free_components(rent));
    assert_eq!(rig.record(1).promotion, state::Promotion::Pending);

    // Promotion still pending.
    let account_before = rig.rt.account(page);
    let observation = rig.close_page(label(row, "promotion_pending"), preferred, page, refund, 1);
    rig.rejected(&observation, ErrorCode::NotClosable);
    assert_eq!(
        rig.rt.account(page),
        account_before,
        "the page is untouched"
    );
    observations.push(observation);

    // Close 1 has since been captured and close 2 is next; close 2's record is
    // as terminal as it gets.  It still cannot close close 1's page: the
    // predicate reads the page's own record, never a newer ticket's state.
    rig.set_pool(|pool| pool.next_close_id = 2);
    let observation = rig.provision_record(
        label(row, "provision_close_record_2"),
        2,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    rig.set_record(2, |record| {
        record.round_id = 1;
        record.finalized = true;
        record.promotion = terminal();
    });
    let observation = rig.close_page(
        label(row, "newer_close_record_terminal"),
        preferred,
        page,
        refund,
        2,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    assert_eq!(rig.rt.account(page), account_before);
    observations.push(observation);

    // Naming the newer close in the kind, with the newer record, passes the
    // predicate but the page itself says close 1: its decoded identity differs.
    let observation = rig.close_page(
        label(row, "kind_names_newer_close"),
        candidate(2, 0),
        page,
        refund,
        2,
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    assert_eq!(rig.rt.account(page), account_before);
    observations.push(observation);

    // Its own record turns terminal.
    rig.set_record(1, |record| record.promotion = terminal());
    let observation = rig.close_page(
        label(row, "wrong_set_tag_in_kind"),
        fallback,
        page,
        refund,
        1,
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    assert_eq!(
        rig.rt.account(page),
        account_before,
        "a fallback-kind call cannot close a preferred page"
    );
    observations.push(observation);

    let before = money(&rig, refund, page);
    let observation = rig.close_page(label(row, ""), preferred, page, refund, 1);
    rig.ok(&observation);
    assert_closed(&rig, page, refund, &before, rent, 0, 0);
    assert_eq!(
        rig.record(1).provisioned_preferred_pages,
        1,
        "closing never decrements a counter"
    );
    observations.push(observation);

    // The fallback page closes once the promotion is Applied (the other
    // terminal variant).
    rig.set_record(1, |record| {
        record.promotion = state::Promotion::Applied {
            winner: owner().pubkey(),
            entitlement: 1,
        }
    });
    let before = money(&rig, refund, fallback_page);
    let observation = rig.close_page(
        label(row, "applied_promotion_fallback_set"),
        fallback,
        fallback_page,
        refund,
        1,
    );
    rig.ok(&observation);
    assert_closed(&rig, fallback_page, refund, &before, rent, 0, 0);
    observations.push(observation);
    observations
}

pub fn close_participant_page_refused_until_that_round_finalized() -> Vec<Observation> {
    let row = ROW_CLOSE_PARTICIPANT;
    let mut rig = Rig::canonical();
    let participant = PageKind::Participant { round: 1 };
    let caller = attacker();
    let mut observations = Vec::new();

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let rent = rig.rt.rent(state::ParticipantPage::SPACE);
    let observation = rig.provision_record(
        label(row, "provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    let observation = rig.provision_ok(
        label(row, "provision_page_0_free"),
        participant,
        0,
        Funding::Free,
        None,
    );
    observations.push(observation);
    let observation = rig.provision_ok(
        label(row, "provision_page_1_caller"),
        participant,
        1,
        Funding::Caller,
        Some(&caller),
    );
    observations.push(observation);
    assert_new_page(&rig.rt, participant, 0, free_components(rent));
    assert_new_page(
        &rig.rt,
        participant,
        1,
        components(0, rent, caller.pubkey()),
    );
    assert_eq!(rig.pool().participant_pages, 2);
    let pool_before = ser(&rig.pool());
    let free_page = page_key(participant, 0);
    let caller_page = page_key(participant, 1);
    let placeholder = owner().pubkey();
    let free_before = rig.rt.account(free_page);

    // The record exists but its round is not finalized.
    let observation = rig.close_page(
        label(row, "record_not_finalized"),
        participant,
        free_page,
        placeholder,
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    assert_eq!(rig.rt.account(free_page), free_before);
    observations.push(observation);

    // The right round, still not finalized.
    rig.set_record(1, |record| record.round_id = 1);
    let observation = rig.close_page(
        label(row, "round_matches_not_finalized"),
        participant,
        free_page,
        placeholder,
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    observations.push(observation);

    // Finalized, but it is another round's record.
    rig.set_record(1, |record| {
        record.round_id = 2;
        record.finalized = true;
    });
    let observation = rig.close_page(
        label(row, "finalized_other_round"),
        participant,
        free_page,
        placeholder,
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    observations.push(observation);

    // Finalized round 1, but the call names round 2.
    rig.set_record(1, |record| record.round_id = 1);
    let observation = rig.close_page(
        label(row, "kind_names_other_round"),
        PageKind::Participant { round: 2 },
        free_page,
        placeholder,
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    observations.push(observation);

    // A finalized record that does not live at its own canonical address is
    // folded into NotClosable.
    let spoof = Pubkey::new_from_array([0x77; 32]);
    rig.rt.put_close_record_at(
        spoof,
        state::CloseRecord {
            round_id: 1,
            finalized: true,
            ..fresh_record(1)
        },
    );
    let instruction = close_ix(&rig.rt, participant, free_page, placeholder, spoof);
    let observation = rig.exec(
        label(row, "record_not_at_canonical_address"),
        instruction,
        &[],
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    assert_eq!(rig.rt.account(free_page), free_before);
    observations.push(observation);

    // The caller-funded page: the refund must go to the recorded destination.
    let observation = rig.close_page(
        label(row, "caller_page_wrong_refund_destination"),
        participant,
        caller_page,
        placeholder,
        1,
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // Finalized round 1: the Free-funded page returns its ops component.
    let before = money(&rig, placeholder, free_page);
    let observation = rig.close_page(label(row, ""), participant, free_page, placeholder, 1);
    rig.ok(&observation);
    assert_closed(&rig, free_page, placeholder, &before, rent, 0, 0);
    observations.push(observation);

    // The caller-funded page returns its owner component to the caller.
    let before = money(&rig, caller.pubkey(), caller_page);
    let observation = rig.close_page(
        label(row, "caller_page"),
        participant,
        caller_page,
        caller.pubkey(),
        1,
    );
    rig.ok(&observation);
    assert_closed(&rig, caller_page, caller.pubkey(), &before, 0, rent, 0);
    observations.push(observation);

    assert_eq!(
        ser(&rig.pool()),
        pool_before,
        "closing pages never touches Pool"
    );
    assert_eq!(
        rig.pool().participant_pages,
        2,
        "the participant counter does not decrement"
    );
    observations
}

/// A closing world: close 1 is in progress, close 2 is next, and
/// `locked_pages` candidate pages' rent is locked.
struct Closing {
    rig: Rig,
    /// `rent(CandidatePage::SPACE)`.
    rent: u64,
    /// `locked_sponsor_remaining` at the start: `rent / 2`.
    sponsor_part: u64,
    /// `ops.locked_remaining_total` and `required_reserve` at the start.
    total: u64,
}

fn closing_world(locked_pages: u64) -> Closing {
    let mut rig = Rig::canonical();
    let rent = rig.rt.rent(state::CandidatePage::SPACE);
    let total = locked_pages * rent;
    let sponsor_part = rent / 2;
    rig.set_pool(|pool| {
        pool.current_close_id = Some(1);
        pool.next_close_id = 2;
    });
    rig.rt.replace_close_record(state::CloseRecord {
        round_id: 1,
        required_pages_per_set: 2,
        required_reserve: total,
        locked_sponsor_remaining: sponsor_part,
        locked_owner_remaining: total - sponsor_part,
        owner_refund_destination: owner().pubkey(),
        ..fresh_record(1)
    });
    // The lock is real cash: it sits in OpsCash beside the rent floor.
    lock(&mut rig, total);
    assert_eq!(rig.ops().free_balance, 0, "all free Ops cash is locked");
    Closing {
        rig,
        rent,
        sponsor_part,
        total,
    }
}

pub fn close_candidate_page_refunds_components_once() -> Vec<Observation> {
    let row = ROW_CANDIDATE_REFUNDS;
    let mut rig = Rig::canonical();
    let preferred = candidate(1, 0);
    let page = page_key(preferred, 0);
    let caller = attacker();
    let mut observations = Vec::new();

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let rent = rig.rt.rent(state::CandidatePage::SPACE);
    let observation = rig.provision_record(
        label(row, "provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    // A Caller-funded candidate page of the next close.
    let ops_before = rig.ops();
    let observation = rig.provision_ok(
        label(row, "provision_caller_funded"),
        preferred,
        0,
        Funding::Caller,
        Some(&caller),
    );
    assert_eq!(
        ser(&rig.ops()),
        ser(&ops_before),
        "Caller funding leaves Ops untouched"
    );
    assert_new_page(&rig.rt, preferred, 0, components(0, rent, caller.pubkey()));
    observations.push(observation);

    // Not terminal yet.
    let observation = rig.close_page(
        label(row, "promotion_pending"),
        preferred,
        page,
        caller.pubkey(),
        1,
    );
    rig.rejected(&observation, ErrorCode::NotClosable);
    observations.push(observation);

    rig.set_record(1, |record| record.promotion = terminal());

    // The refund goes only to the recorded destination.
    let observation = rig.close_page(
        label(row, "wrong_refund_destination"),
        preferred,
        page,
        owner().pubkey(),
        1,
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // The refund destination must be a valid payout destination; OpsCash is not.
    let observation = rig.close_page(
        label(row, "ops_cash_as_refund_destination"),
        preferred,
        page,
        rig.rt.ops_cash_address(),
        1,
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // Terminal: the owner component goes to the caller, the ops component (zero)
    // to Operations.
    let before = money(&rig, caller.pubkey(), page);
    let observation = rig.close_page(label(row, ""), preferred, page, caller.pubkey(), 1);
    rig.ok(&observation);
    assert_closed(&rig, page, caller.pubkey(), &before, 0, rent, 0);
    assert_eq!(
        lamports(&rig.rt, caller.pubkey()) - before.refund,
        rent,
        "the caller is refunded exactly the rent it paid"
    );
    assert_eq!(
        rig.cash(),
        before.cash,
        "OpsCash receives its (zero) component"
    );
    observations.push(observation);

    // Once only.
    let balances = money(&rig, caller.pubkey(), page);
    let observation = rig.close_page(
        label(row, "second_call"),
        preferred,
        page,
        caller.pubkey(),
        1,
    );
    rig.rejected(&observation, ErrorCode::AlreadyClosed);
    let after = money(&rig, caller.pubkey(), page);
    assert_eq!(after.cash, balances.cash);
    assert_eq!(after.refund, balances.refund, "no second refund");
    observations.push(observation);

    // The slot cannot be re-provisioned either: the close counter only grows.
    let observation = rig.provision_rejected(
        label(row, "reprovision_closed_slot"),
        preferred,
        0,
        Funding::Caller,
        Some(&caller),
        ErrorCode::WrongIndex,
    );
    assert_eq!(rig.record(1).provisioned_preferred_pages, 1);
    observations.push(observation);

    // A page provisioned from the close's locked reserve carries both
    // components (sponsor first, then owner): each is refunded to its own
    // destination, and the sponsor component returns to free_balance.
    let Closing {
        rig: mut mixed,
        rent,
        sponsor_part,
        ..
    } = closing_world(2);
    let owner_key = owner();
    let mixed_page = page_key(preferred, 0);
    let observation = mixed.provision_ok(
        label(row, "mixed_components.provision_locked"),
        preferred,
        0,
        Funding::LockedClose,
        None,
    );
    assert_eq!(
        page_rent(&mixed.rt, preferred, 0),
        components(sponsor_part, rent - sponsor_part, owner_key.pubkey())
    );
    observations.push(observation);
    mixed.set_record(1, |record| record.promotion = terminal());
    let locked_before = mixed.ops().locked_remaining_total;
    let before = money(&mixed, owner_key.pubkey(), mixed_page);
    let observation = mixed.close_page(
        label(row, "mixed_components"),
        preferred,
        mixed_page,
        owner_key.pubkey(),
        1,
    );
    mixed.ok(&observation);
    assert_closed(
        &mixed,
        mixed_page,
        owner_key.pubkey(),
        &before,
        sponsor_part,
        rent - sponsor_part,
        0,
    );
    assert_eq!(
        mixed.ops().locked_remaining_total,
        locked_before,
        "the sponsor component returns to free_balance, not to the lock"
    );
    assert_eq!(mixed.ops().free_balance, sponsor_part);
    observations.push(observation);
    let observation = mixed.close_page(
        label(row, "mixed_components.second_call"),
        preferred,
        mixed_page,
        owner_key.pubkey(),
        1,
    );
    mixed.rejected(&observation, ErrorCode::AlreadyClosed);
    observations.push(observation);
    observations
}

pub fn close_close_record_always_refused() -> Vec<Observation> {
    let row = ROW_CLOSE_CLOSE_RECORD;
    let mut rig = Rig::canonical();
    let mut observations = Vec::new();
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let observation = rig.provision_record(
        label(row, "provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    // The most permissive record: round 1 finalized with a terminal promotion.
    rig.set_record(1, |record| {
        record.round_id = 1;
        record.finalized = true;
        record.promotion = terminal();
    });
    let record_key = rig.rt.close_record_address(1);
    let refund = attacker().pubkey();
    let account_before = rig.rt.account(record_key);

    // There is no `PageKind` for a CloseRecord.  The kinds that can never be
    // closed are refused by kind; the closable kinds fail to decode the record
    // as their page type.  Either way the record is untouched.
    let attempts: [(&str, PageKind, ErrorCode); 4] = [
        ("", PageKind::Snapshot, ErrorCode::NotClosable),
        ("as_registry", PageKind::Registry, ErrorCode::NotClosable),
        (
            "as_participant",
            PageKind::Participant { round: 1 },
            ErrorCode::WrongAccount,
        ),
        ("as_candidate", candidate(1, 0), ErrorCode::WrongAccount),
    ];
    for (variant, kind, expected) in attempts {
        let observation = rig.close_page(label(row, variant), kind, record_key, refund, 1);
        rig.rejected(&observation, expected);
        assert_eq!(
            rig.rt.account(record_key),
            account_before,
            "the CloseRecord is untouched by {kind:?}"
        );
        observations.push(observation);
    }
    observations
}

// ---------------------------------------------------------------------------
// Rows: close records and candidate reserve
// ---------------------------------------------------------------------------

pub fn provision_close_record_requires_next_close_id() -> Vec<Observation> {
    let row = ROW_CLOSE_RECORD_NEXT;
    let mut rig = Rig::canonical();
    let caller = attacker();
    let mut observations = Vec::new();
    let rent = rig.rt.rent(state::CloseRecord::SPACE);
    assert_eq!(rig.pool().next_close_id, 1);
    let record_1 = rig.rt.close_record_address(1);
    let record_2 = rig.rt.close_record_address(2);

    // Free funding with an unfunded Operations account.
    let ops_before = ser(&rig.ops());
    let observation = rig.provision_record(label(row, "unfunded_ops"), 1, Funding::Free, None);
    rig.rejected(&observation, ErrorCode::InsufficientOperations);
    assert!(rig.rt.svm.get_account(&record_1).is_none());
    assert_eq!(ser(&rig.ops()), ops_before);
    observations.push(observation);

    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);

    // Close 2 while 1 is next (headline), and the other off-by-one neighbours.
    let observation = rig.provision_record(label(row, ""), 2, Funding::Free, None);
    rig.rejected(&observation, ErrorCode::InvalidState);
    assert!(rig.rt.svm.get_account(&record_2).is_none());
    observations.push(observation);
    let instruction = provision_close_record_ix(
        &rig.rt,
        0,
        Funding::Free,
        rig.payer_key(),
        rig.rt.close_record_address(0),
    );
    let observation = rig.exec(label(row, "close_id_zero"), instruction, &[]);
    rig.rejected(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // The record account must be the PDA of the id.
    let instruction =
        provision_close_record_ix(&rig.rt, 1, Funding::Free, rig.payer_key(), record_2);
    let observation = rig.exec(
        label(row, "address_of_close_2_for_close_1"),
        instruction,
        &[],
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // LockedClose is never a valid funding for a close record.
    let observation = rig.provision_record(
        label(row, "locked_close_funding"),
        1,
        Funding::LockedClose,
        None,
    );
    rig.rejected(&observation, ErrorCode::LockNotPermitted);
    assert!(rig.rt.svm.get_account(&record_1).is_none());
    observations.push(observation);

    // Close 1 is next: Free pays the exact rent from Operations.
    let pool_before = ser(&rig.pool());
    let ops_before = rig.ops();
    let cash_before = rig.cash();
    let observation =
        rig.provision_record(label(row, "free_next_close_id"), 1, Funding::Free, None);
    rig.ok(&observation);
    assert_eq!(rig.ops().free_balance, FUND - rent);
    assert_eq!(
        rig.ops().total_rent_funded,
        ops_before.total_rent_funded + u128::from(rent)
    );
    assert_eq!(ser(&rig.ops()), ser(&ops_after_free(&ops_before, rent)));
    assert_eq!(rig.cash(), cash_before - rent);
    assert_eq!(
        lamports(&rig.rt, record_1),
        rent,
        "the record holds exactly its rent"
    );
    let account = rig.rt.account(record_1);
    assert_eq!(account.owner, ID);
    assert_eq!(account.data.len(), state::CloseRecord::SPACE);
    let expected = state::CloseRecord {
        draw_state: state::DRAW_NOT_REQUESTED,
        rent: free_components(rent),
        ..fresh_record(1)
    };
    assert_eq!(
        ser(&rig.record(1)),
        ser(&expected),
        "a pending, otherwise blank record"
    );
    assert_eq!(
        ser(&rig.pool()),
        pool_before,
        "provisioning a record does not touch Pool"
    );
    assert_eq!(
        rig.pool().next_close_id,
        1,
        "next_close_id advances only at close capture"
    );
    observations.push(observation);

    // Again: the record exists.
    let observation = rig.provision_record(label(row, "again"), 1, Funding::Free, None);
    rig.rejected(&observation, ErrorCode::WrongIndex);
    observations.push(observation);

    // LockedClose on an existing record fails on the existing record first.
    let observation = rig.provision_record(
        label(row, "locked_close_on_existing"),
        1,
        Funding::LockedClose,
        None,
    );
    rig.rejected(&observation, ErrorCode::WrongIndex);
    observations.push(observation);

    // Close 1 has been captured; close 2 is next, and close 1 is stale.
    rig.set_pool(|pool| pool.next_close_id = 2);
    let observation = rig.provision_record(
        label(row, "stale_close_id"),
        1,
        Funding::Caller,
        Some(&caller),
    );
    rig.rejected(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // Caller funding records the owner component and the refund destination and
    // leaves Operations alone.
    let ops_before = rig.ops();
    let cash_before = rig.cash();
    let caller_before = lamports(&rig.rt, caller.pubkey());
    let observation = rig.provision_record(
        label(row, "caller_next_close_id"),
        2,
        Funding::Caller,
        Some(&caller),
    );
    rig.ok(&observation);
    assert_eq!(caller_before - lamports(&rig.rt, caller.pubkey()), rent);
    assert_eq!(ser(&rig.ops()), ser(&ops_before));
    assert_eq!(rig.cash(), cash_before);
    let expected = state::CloseRecord {
        draw_state: state::DRAW_NOT_REQUESTED,
        rent: components(0, rent, caller.pubkey()),
        ..fresh_record(2)
    };
    assert_eq!(ser(&rig.record(2)), ser(&expected));
    observations.push(observation);
    observations
}

pub fn provision_candidate_pages_for_next_close_reduces_required_reserve_per_set_only(
) -> Vec<Observation> {
    let row = ROW_CANDIDATE_RESERVE;
    let mut rig = Rig::canonical();
    let preferred = candidate(1, 0);
    let fallback = candidate(1, 1);
    let mut observations = Vec::new();

    // Nine participants need two pages per set, so index 1 is Free-fundable.
    rig.set_pool(|pool| pool.participant_count = 9);
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let observation = rig.provision_record(
        label(row, "provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    let rent = rig.rt.rent(state::CandidatePage::SPACE);
    let close = rig.record(1);
    assert_eq!(close.existing_preferred_pages, 0);
    assert_eq!(close.existing_fallback_pages, 0);

    let observation = rig.provision_ok(label(row, ""), preferred, 0, Funding::Free, None);
    assert_new_page(&rig.rt, preferred, 0, free_components(rent));
    observations.push(observation);
    let observation = rig.provision_ok(
        label(row, "preferred_page_1"),
        preferred,
        1,
        Funding::Free,
        None,
    );
    assert_new_page(&rig.rt, preferred, 1, free_components(rent));
    observations.push(observation);

    let close = rig.record(1);
    assert_eq!(close.provisioned_preferred_pages, 2);
    assert_eq!(
        close.existing_preferred_pages, 2,
        "two preferred pages exist"
    );
    assert_eq!(close.provisioned_fallback_pages, 0);
    assert_eq!(close.existing_fallback_pages, 0, "no fallback page exists");

    // The program's own reserve schedule and pure function.
    let config: state::ConfigVersion = rig.rt.deserialize(rig.rt.config_address(1));
    let rent_sysvar: Rent = rig.rt.svm.get_sysvar::<Rent>();
    let schedule = state::ReserveSchedule::capture(&rent_sysvar, &config);
    assert_eq!(
        schedule.per_candidate_page_rent, rent,
        "the captured schedule prices a candidate page at the measured rent"
    );
    let pool = rig.pool();
    assert_eq!((pool.participant_count, pool.registry_len), (9, 0));
    let reserve = |preferred_pages: u64, fallback_pages: u64| {
        required_close_reserve(
            pool.participant_count,
            pool.registry_len,
            preferred_pages,
            fallback_pages,
            &schedule,
        )
        .expect("required_close_reserve")
    };
    let none_existing = reserve(0, 0);
    let after_two = reserve(
        close.existing_preferred_pages,
        close.existing_fallback_pages,
    );
    assert_eq!(
        none_existing - after_two,
        2 * rent,
        "two preferred pages reduce the required reserve by exactly 2 x rent(CandidatePage)"
    );
    // Per set only: the same two pages in the preferred set leave the fallback
    // set's missing count (two pages) untouched.
    assert_eq!(
        reserve(0, 2),
        reserve(2, 2) + 2 * rent,
        "with the fallback set full, the preferred set's two missing pages still cost 2 x rent"
    );
    assert_eq!(
        reserve(2, 0) - reserve(2, 2),
        2 * rent,
        "fallback's two missing pages are all still required after preferred pages exist"
    );
    assert_eq!(
        reserve(5, 0),
        reserve(2, 0),
        "an over-provisioned preferred set never offsets the fallback set"
    );

    // One fallback page lowers only the fallback side.
    let observation = rig.provision_ok(
        label(row, "fallback_page_0"),
        fallback,
        0,
        Funding::Free,
        None,
    );
    assert_new_page(&rig.rt, fallback, 0, free_components(rent));
    observations.push(observation);
    let close = rig.record(1);
    assert_eq!(
        close.existing_preferred_pages, 2,
        "the preferred count did not move"
    );
    assert_eq!(close.existing_fallback_pages, 1);
    assert_eq!(close.provisioned_preferred_pages, 2);
    assert_eq!(close.provisioned_fallback_pages, 1);
    assert_eq!(
        after_two
            - reserve(
                close.existing_preferred_pages,
                close.existing_fallback_pages
            ),
        rent,
        "one fallback page reduces the required reserve by exactly 1 x rent"
    );
    // The same index in the other set is a different PDA.
    assert_ne!(page_key(preferred, 0), page_key(fallback, 0));
    observations
}

// ---------------------------------------------------------------------------
// Rows: LockedClose funding
// ---------------------------------------------------------------------------

pub fn provision_locked_close_debits_remaining_partitions_sponsor_first_and_records_spent(
) -> Vec<Observation> {
    let row = ROW_LOCKED_DEBITS;
    let Closing {
        mut rig,
        rent,
        sponsor_part,
        total,
    } = closing_world(2);
    let preferred = candidate(1, 0);
    let fallback = candidate(1, 1);
    let owner_key = owner().pubkey();
    let mut observations = Vec::new();
    assert!(
        sponsor_part > 0 && sponsor_part < rent,
        "the sponsor partition is partial"
    );

    // First page: the sponsor partition is exhausted first, then the owner's
    // covers the rest.
    let ops_before = rig.ops();
    let close_before = rig.record(1);
    let pool_before = ser(&rig.pool());
    let cash_before = rig.cash();
    let page = page_key(preferred, 0);
    let observation = rig.provision_ok(label(row, ""), preferred, 0, Funding::LockedClose, None);
    let (sponsor_spent, owner_spent) = split(rent, sponsor_part);
    assert_eq!(
        (sponsor_spent, owner_spent),
        (sponsor_part, rent - sponsor_part)
    );
    let ops = rig.ops();
    assert_eq!(ops.free_balance, 0, "free_balance is still zero");
    assert_eq!(
        ops.locked_remaining_total,
        total - rent,
        "the lock falls by exactly the rent"
    );
    assert_eq!(
        ops.total_rent_funded,
        ops_before.total_rent_funded + u128::from(rent),
        "LockedClose adds the debit to total_rent_funded"
    );
    assert_eq!(ser(&ops), ser(&ops_after_locked(&ops_before, rent)));
    let close = rig.record(1);
    assert_eq!(
        close.locked_sponsor_remaining, 0,
        "sponsor partition exhausted first"
    );
    assert_eq!(
        close.locked_owner_remaining,
        close_before.locked_owner_remaining - owner_spent,
        "the owner partition pays only the remainder"
    );
    assert_eq!(
        close.spent.candidate_rent_s, sponsor_spent,
        "spent.candidate_rent_s"
    );
    assert_eq!(
        close.spent.candidate_rent_o, owner_spent,
        "spent.candidate_rent_o"
    );
    assert_eq!(close.provisioned_preferred_pages, 1);
    assert_eq!(
        close.existing_preferred_pages, close_before.existing_preferred_pages,
        "existing_*_pages is frozen once the close is current"
    );
    let mut expected_close = close_before.clone();
    expected_close.locked_sponsor_remaining = 0;
    expected_close.locked_owner_remaining -= owner_spent;
    expected_close.spent.candidate_rent_s += sponsor_spent;
    expected_close.spent.candidate_rent_o += owner_spent;
    expected_close.provisioned_preferred_pages = 1;
    assert_eq!(
        ser(&close),
        ser(&expected_close),
        "nothing else on the record changed"
    );
    assert_eq!(
        rig.cash(),
        cash_before - rent,
        "OpsCash pays exactly the rent"
    );
    assert_new_page(
        &rig.rt,
        preferred,
        0,
        components(sponsor_spent, owner_spent, owner_key),
    );
    assert_eq!(
        page_rent(&rig.rt, preferred, 0),
        components(sponsor_part, rent - sponsor_part, owner_key),
        "rent components by source, refund destination = the close's owner"
    );
    assert_eq!(ser(&rig.pool()), pool_before, "Pool is untouched");
    assert_eq!(
        moved(&observation),
        keys(&[rig.rt.ops_cash_address(), page, rig.payer_key()]),
        "lamports move only from OpsCash to the new page, plus the fee"
    );
    observations.push(observation);

    // Second page: the sponsor partition is empty, so the owner pays all of it.
    let ops_before = rig.ops();
    let close_before = rig.record(1);
    let observation = rig.provision_ok(
        label(row, "second_page"),
        preferred,
        1,
        Funding::LockedClose,
        None,
    );
    let close = rig.record(1);
    assert_eq!(close.locked_sponsor_remaining, 0);
    assert_eq!(close.locked_owner_remaining, 0, "both partitions are spent");
    assert_eq!(
        close.spent.candidate_rent_s, sponsor_part,
        "no further sponsor spend"
    );
    assert_eq!(
        close.spent.candidate_rent_o,
        close_before.spent.candidate_rent_o + rent
    );
    assert_eq!(close.provisioned_preferred_pages, 2);
    assert_eq!(rig.ops().locked_remaining_total, 0);
    assert_eq!(rig.ops().free_balance, 0);
    assert_eq!(ser(&rig.ops()), ser(&ops_after_locked(&ops_before, rent)));
    assert_new_page(&rig.rt, preferred, 1, components(0, rent, owner_key));
    assert_eq!(
        rig.cash(),
        rig.rt.rent(0),
        "the reserve is fully spent into pages"
    );
    observations.push(observation);

    // The reserve is exhausted although the fallback index is within the
    // required pages: LockNotPermitted, nothing moves.
    let observation = rig.provision_rejected(
        label(row, "reserve_exhausted"),
        fallback,
        0,
        Funding::LockedClose,
        None,
        ErrorCode::LockNotPermitted,
    );
    assert_eq!(ser(&rig.record(1)), ser(&close));
    observations.push(observation);

    // A page PDA that already holds lamports lowers the debit, and every figure
    // derived from it, by exactly that amount.
    let Closing {
        rig: mut pre,
        rent,
        sponsor_part,
        total,
    } = closing_world(2);
    let page = page_key(preferred, 0);
    pre.rt.credit_lamports(page, PRECREDIT);
    let debit = rent - PRECREDIT;
    let (sponsor_spent, owner_spent) = split(debit, sponsor_part);
    let ops_before = pre.ops();
    let observation = pre.provision_ok(
        label(row, "prefunded_page"),
        preferred,
        0,
        Funding::LockedClose,
        None,
    );
    assert_eq!(pre.ops().locked_remaining_total, total - debit);
    assert_eq!(ser(&pre.ops()), ser(&ops_after_locked(&ops_before, debit)));
    let close = pre.record(1);
    assert_eq!(close.spent.candidate_rent_s, sponsor_spent);
    assert_eq!(close.spent.candidate_rent_o, owner_spent);
    assert_eq!(sponsor_spent + owner_spent, rent - PRECREDIT);
    assert_eq!(
        lamports(&pre.rt, page),
        rent,
        "the page still holds exactly its rent"
    );
    assert_new_page(
        &pre.rt,
        preferred,
        0,
        components(sponsor_spent, owner_spent, owner_key),
    );
    observations.push(observation);
    observations
}

pub fn provision_locked_close_rejects_wrong_close_set_or_index_and_cannot_debit_twice(
) -> Vec<Observation> {
    let row = ROW_LOCKED_REJECTS;
    let Closing {
        mut rig,
        rent,
        sponsor_part,
        total,
    } = closing_world(4);
    let preferred = candidate(1, 0);
    let next = candidate(2, 0);
    let caller = attacker();
    let mut observations = Vec::new();

    // Close 2 is next and has a record (paid by a caller; Operations has no free cash).
    let observation = rig.provision_record(
        label(row, "provision_close_record_2"),
        2,
        Funding::Caller,
        Some(&caller),
    );
    rig.ok(&observation);
    observations.push(observation);

    // A LockedClose rejection moves nothing: the close in progress, Ops, Pool
    // and OpsCash are byte-identical.
    macro_rules! locked_rejected {
        ($variant:expr, $kind:expr, $index:expr, $expected:expr) => {{
            let record = ser(&rig.record(1));
            let observation = rig.provision_rejected(
                label(row, $variant),
                $kind,
                $index,
                Funding::LockedClose,
                None,
                $expected,
            );
            assert_eq!(
                ser(&rig.record(1)),
                record,
                "{}: CloseRecord(1) unchanged",
                $variant
            );
            observations.push(observation);
        }};
    }

    // The next close (not the current one) cannot draw from the lock.
    locked_rejected!("", next, 0, ErrorCode::LockNotPermitted);
    // A set tag outside {0, 1} is an invalid kind.
    locked_rejected!("set_tag_2", candidate(1, 2), 0, ErrorCode::InvalidState);
    // Not the next sequential index.
    locked_rejected!("wrong_index", preferred, 1, ErrorCode::WrongIndex);
    // Only candidate pages of the current close may be locked-funded.
    locked_rejected!(
        "snapshot_kind",
        PageKind::Snapshot,
        1,
        ErrorCode::LockNotPermitted
    );
    locked_rejected!(
        "registry_kind",
        PageKind::Registry,
        1,
        ErrorCode::LockNotPermitted
    );
    locked_rejected!(
        "participant_kind",
        PageKind::Participant { round: 1 },
        0,
        ErrorCode::LockNotPermitted
    );

    // The right address for the wrong index, and the wrong close record.
    let instruction = provision_ix_raw(
        &rig.rt,
        preferred,
        0,
        Funding::LockedClose,
        rig.payer_key(),
        page_key(preferred, 1),
        close_for(&rig.rt, preferred),
    );
    let observation = rig.exec(
        label(row, "page_address_of_index_1_for_index_0"),
        instruction,
        &[],
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);
    let instruction = provision_ix_raw(
        &rig.rt,
        preferred,
        0,
        Funding::LockedClose,
        rig.payer_key(),
        page_key(preferred, 0),
        Some(rig.rt.close_record_address(2)),
    );
    let observation = rig.exec(
        label(row, "close_record_of_another_close"),
        instruction,
        &[],
    );
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);
    let instruction = provision_ix_raw(
        &rig.rt,
        preferred,
        0,
        Funding::LockedClose,
        rig.payer_key(),
        page_key(preferred, 0),
        None,
    );
    let observation = rig.exec(label(row, "missing_close_record"), instruction, &[]);
    rig.rejected(&observation, ErrorCode::InvalidState);
    observations.push(observation);

    // The lock is still whole after every rejection.
    assert_eq!(rig.ops().locked_remaining_total, total);
    assert_eq!(rig.record(1).locked_sponsor_remaining, sponsor_part);

    // The one valid call succeeds ...
    let observation = rig.provision_ok(
        label(row, "valid_once"),
        preferred,
        0,
        Funding::LockedClose,
        None,
    );
    assert_eq!(rig.ops().locked_remaining_total, total - rent);
    observations.push(observation);

    // ... and repeating it cannot debit again: the page now exists and the
    // sequential counter has moved.
    let record = ser(&rig.record(1));
    let ops = ser(&rig.ops());
    let observation = rig.provision_rejected(
        label(row, "repeat_same_call"),
        preferred,
        0,
        Funding::LockedClose,
        None,
        ErrorCode::WrongIndex,
    );
    assert_eq!(
        ser(&rig.record(1)),
        record,
        "partitions unchanged by the repeat"
    );
    assert_eq!(ser(&rig.ops()), ops, "the lock unchanged by the repeat");
    observations.push(observation);

    // Even with the counter rewound the existing page cannot be paid for twice.
    rig.set_record(1, |close| close.provisioned_preferred_pages = 0);
    let observation = rig.provision_rejected(
        label(row, "repeat_counter_rewound"),
        preferred,
        0,
        Funding::LockedClose,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    observations
}

pub fn provision_locked_close_rejects_index_at_or_beyond_required_pages_per_set() -> Vec<Observation>
{
    let row = ROW_LOCKED_BEYOND;
    let Closing {
        mut rig,
        rent,
        sponsor_part,
        total,
    } = closing_world(4);
    let preferred = candidate(1, 0);
    let fallback = candidate(1, 1);
    let owner_key = owner().pubkey();
    let required = rig.record(1).required_pages_per_set;
    assert_eq!(required, 2);
    let mut observations = Vec::new();

    for index in 0..required {
        let observation = rig.provision_ok(
            label(row, &format!("preferred_{index}")),
            preferred,
            index,
            Funding::LockedClose,
            None,
        );
        // Sponsor first: the first page takes the whole sponsor partition.
        let expected = if index == 0 {
            components(sponsor_part, rent - sponsor_part, owner_key)
        } else {
            components(0, rent, owner_key)
        };
        assert_new_page(&rig.rt, preferred, index, expected);
        observations.push(observation);
    }
    assert_eq!(rig.ops().locked_remaining_total, total - required * rent);

    // Index == required_pages_per_set is the next sequential index, so only the
    // lock permission refuses it.  (Indexes beyond it are unreachable: the
    // sequential check fails first.)
    let observation = rig.provision_rejected(
        label(row, ""),
        preferred,
        required,
        Funding::LockedClose,
        None,
        ErrorCode::LockNotPermitted,
    );
    observations.push(observation);

    // The same index with Free funding succeeds and stays attributed to its
    // preferred set; the lock is untouched.
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let locked = rig.ops().locked_remaining_total;
    let ops_before = rig.ops();
    let close_before = rig.record(1);
    let observation = rig.provision_ok(
        label(row, "same_index_free"),
        preferred,
        required,
        Funding::Free,
        None,
    );
    assert_new_page(&rig.rt, preferred, required, free_components(rent));
    let close = rig.record(1);
    assert_eq!(
        close.provisioned_preferred_pages,
        required + 1,
        "attributed to the preferred set"
    );
    assert_eq!(
        close.provisioned_fallback_pages, 0,
        "the fallback set is untouched"
    );
    assert_eq!(
        close.existing_preferred_pages,
        close_before.existing_preferred_pages
    );
    assert_eq!(
        close.existing_fallback_pages,
        close_before.existing_fallback_pages
    );
    assert_eq!(
        close.locked_sponsor_remaining,
        close_before.locked_sponsor_remaining
    );
    assert_eq!(
        close.locked_owner_remaining,
        close_before.locked_owner_remaining
    );
    assert_eq!(
        close.spent.candidate_rent_s,
        close_before.spent.candidate_rent_s
    );
    assert_eq!(
        close.spent.candidate_rent_o,
        close_before.spent.candidate_rent_o
    );
    assert_eq!(
        rig.ops().locked_remaining_total,
        locked,
        "Free never touches the lock"
    );
    assert_eq!(ser(&rig.ops()), ser(&ops_after_free(&ops_before, rent)));
    observations.push(observation);

    // The fallback set has the same boundary, with its own counter.
    for index in 0..required {
        let observation = rig.provision_ok(
            label(row, &format!("fallback_{index}")),
            fallback,
            index,
            Funding::LockedClose,
            None,
        );
        // The sponsor partition was emptied by the first preferred page.
        assert_new_page(&rig.rt, fallback, index, components(0, rent, owner_key));
        observations.push(observation);
    }
    assert_eq!(
        rig.ops().locked_remaining_total,
        0,
        "all four locked pages are spent"
    );
    let observation = rig.provision_rejected(
        label(row, "fallback_at_required"),
        fallback,
        required,
        Funding::LockedClose,
        None,
        ErrorCode::LockNotPermitted,
    );
    observations.push(observation);
    let observation = rig.provision_ok(
        label(row, "fallback_same_index_free"),
        fallback,
        required,
        Funding::Free,
        None,
    );
    let close = rig.record(1);
    assert_eq!(close.provisioned_fallback_pages, required + 1);
    assert_eq!(close.provisioned_preferred_pages, required + 1);
    observations.push(observation);
    observations
}

// ---------------------------------------------------------------------------
// Rows: need bound, reconcile, rent table
// ---------------------------------------------------------------------------

pub fn free_funding_respects_need_bound() -> Vec<Observation> {
    let row = ROW_NEED_BOUND;
    let caller = attacker();
    let mut observations = Vec::new();

    // Snapshot and Registry pages: index <= registry_len / 8 with Free funding.
    // The canonical fixture already has page 0 of each, so index 1 is the
    // sequential next and only the need bound can refuse it.
    let mut rig = Rig::canonical();
    let rent = rig.rt.rent(state::SnapshotPage::SPACE);
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    assert_eq!((rig.pool().snapshot_pages, rig.pool().registry_len), (1, 0));

    let observation = rig.provision_rejected(
        label(row, ""),
        PageKind::Snapshot,
        1,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    rig.set_pool(|pool| pool.registry_len = 7);
    let observation = rig.provision_rejected(
        label(row, "snapshot_index_1_registry_len_7"),
        PageKind::Snapshot,
        1,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    // Caller funding at the same state and index succeeds: the index itself was
    // valid, only free sponsor cash is bounded by need.
    let ops_before = rig.ops();
    let observation = rig.provision_ok(
        label(row, "snapshot_index_1_caller_same_state"),
        PageKind::Snapshot,
        1,
        Funding::Caller,
        Some(&caller),
    );
    assert_new_page(
        &rig.rt,
        PageKind::Snapshot,
        1,
        components(0, rent, caller.pubkey()),
    );
    assert_eq!(ser(&rig.ops()), ser(&ops_before));
    observations.push(observation);
    // The exact boundary, on the registry kind: 7 refuses index 1, 8 accepts it.
    let observation = rig.provision_rejected(
        label(row, "registry_index_1_registry_len_7"),
        PageKind::Registry,
        1,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    rig.set_pool(|pool| pool.registry_len = 8);
    let observation = rig.provision_ok(
        label(row, "registry_index_1_registry_len_8"),
        PageKind::Registry,
        1,
        Funding::Free,
        None,
    );
    assert_new_page(
        &rig.rt,
        PageKind::Registry,
        1,
        free_components(rig.rt.rent(state::RegistryPage::SPACE)),
    );
    observations.push(observation);

    // After fixture registry_len = 9, a Free snapshot index 1 succeeds.
    let mut nine = Rig::canonical();
    let funded = nine.fund(label(row, "registry_len_9.fund_ops"), FUND);
    nine.ok(&funded);
    observations.push(funded);
    nine.set_pool(|pool| pool.registry_len = 9);
    let ops_before = nine.ops();
    let observation = nine.provision_ok(
        label(row, "snapshot_index_1_registry_len_9"),
        PageKind::Snapshot,
        1,
        Funding::Free,
        None,
    );
    assert_new_page(&nine.rt, PageKind::Snapshot, 1, free_components(rent));
    assert_eq!(ser(&nine.ops()), ser(&ops_after_free(&ops_before, rent)));
    observations.push(observation);

    // Participant pages: index <= participant_count / 8.
    let mut rig = Rig::canonical();
    let participant = PageKind::Participant { round: 1 };
    let funded = rig.fund(label(row, "participant.fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let observation = rig.provision_ok(
        label(row, "participant_index_0_count_0"),
        participant,
        0,
        Funding::Free,
        None,
    );
    observations.push(observation);
    for count in [0u64, 7] {
        rig.set_pool(|pool| pool.participant_count = count);
        let observation = rig.provision_rejected(
            label(row, &format!("participant_index_1_count_{count}")),
            participant,
            1,
            Funding::Free,
            None,
            ErrorCode::WrongIndex,
        );
        observations.push(observation);
    }
    rig.set_pool(|pool| pool.participant_count = 8);
    let observation = rig.provision_ok(
        label(row, "participant_index_1_count_8"),
        participant,
        1,
        Funding::Free,
        None,
    );
    observations.push(observation);
    let observation = rig.provision_rejected(
        label(row, "participant_index_2_count_8"),
        participant,
        2,
        Funding::Free,
        None,
        ErrorCode::WrongIndex,
    );
    observations.push(observation);
    rig.set_pool(|pool| pool.participant_count = 16);
    let observation = rig.provision_ok(
        label(row, "participant_index_2_count_16"),
        participant,
        2,
        Funding::Free,
        None,
    );
    observations.push(observation);

    // Candidate pages of the NEXT close: index < pages_for(max(participants, 1), 8).
    let mut rig = Rig::canonical();
    let preferred = candidate(1, 0);
    let fallback = candidate(1, 1);
    let funded = rig.fund(label(row, "candidate.fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let observation = rig.provision_record(
        label(row, "candidate.provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    let observation = rig.provision_ok(
        label(row, "candidate_index_0_count_0"),
        preferred,
        0,
        Funding::Free,
        None,
    );
    observations.push(observation);
    for count in [0u64, 8] {
        rig.set_pool(|pool| pool.participant_count = count);
        let observation = rig.provision_rejected(
            label(row, &format!("candidate_index_1_count_{count}")),
            preferred,
            1,
            Funding::Free,
            None,
            ErrorCode::WrongIndex,
        );
        observations.push(observation);
    }
    // Caller funding of an unneeded index is not restricted.
    rig.set_pool(|pool| pool.participant_count = 0);
    for index in 0..2 {
        let observation = rig.provision_ok(
            label(
                row,
                &format!("candidate_fallback_index_{index}_caller_count_0"),
            ),
            fallback,
            index,
            Funding::Caller,
            Some(&caller),
        );
        observations.push(observation);
    }
    rig.set_pool(|pool| pool.participant_count = 9);
    let observation = rig.provision_ok(
        label(row, "candidate_index_1_count_9"),
        preferred,
        1,
        Funding::Free,
        None,
    );
    observations.push(observation);
    observations
}

pub fn reconcile_ops_cash_classifies_anonymous_credit_as_unallocated_excess() -> Vec<Observation> {
    let row = ROW_RECONCILE;
    let mut rig = Rig::canonical();
    let ops_cash = rig.rt.ops_cash_address();
    let mut observations = Vec::new();

    // Real free cash is already recorded.
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);

    // An anonymous credit: lamports land in OpsCash with no instruction.
    rig.rt.add_lamports(ops_cash, 777);
    let before = rig.ops();
    assert_eq!(rig.cash(), rig.rt.rent(0) + FUND + 777);

    let instruction = reconcile_ix(&rig.rt, ops_cash);
    let observation = rig.exec(label(row, ""), instruction, &[]);
    rig.ok(&observation);
    let after = rig.ops();
    assert_eq!(after.unallocated_excess, before.unallocated_excess + 777);
    assert_eq!(
        after.free_balance, before.free_balance,
        "free_balance is unchanged"
    );
    assert_eq!(after.locked_remaining_total, before.locked_remaining_total);
    let mut expected = before.clone();
    expected.unallocated_excess += 777;
    assert_eq!(
        ser(&after),
        ser(&expected),
        "only unallocated_excess changed"
    );
    assert_eq!(
        rig.cash(),
        rig.rt.rent(0) + FUND + 777,
        "reconcile moves no lamports"
    );
    assert_eq!(
        moved(&observation),
        keys(&[rig.payer_key()]),
        "no lamports move except the fee"
    );
    observations.push(observation);

    // Idempotent: a second reconcile is a no-op.
    let settled = ser(&rig.ops());
    let instruction = reconcile_ix(&rig.rt, ops_cash);
    let observation = rig.exec(label(row, "second_call_is_a_noop"), instruction, &[]);
    rig.ok(&observation);
    assert_eq!(ser(&rig.ops()), settled);
    assert_eq!(
        moved(&observation),
        keys(&[rig.payer_key()]),
        "only the fee moves"
    );
    observations.push(observation);

    // A further credit accumulates.
    rig.rt.add_lamports(ops_cash, 1);
    let instruction = reconcile_ix(&rig.rt, ops_cash);
    let observation = rig.exec(label(row, "further_credit"), instruction, &[]);
    rig.ok(&observation);
    assert_eq!(
        rig.ops().unallocated_excess,
        before.unallocated_excess + 778
    );
    observations.push(observation);

    // Only the canonical OpsCash PDA is accepted.
    let instruction = reconcile_ix(&rig.rt, attacker().pubkey());
    let observation = rig.exec(label(row, "wrong_ops_cash"), instruction, &[]);
    rig.rejected(&observation, ErrorCode::WrongAccount);
    observations.push(observation);

    // Any instruction that syncs Ops first classifies a credit too: a Caller
    // provision does, and leaves free_balance alone.
    rig.rt.add_lamports(ops_cash, 5);
    let excess_before = rig.ops().unallocated_excess;
    let free_before = rig.ops().free_balance;
    let observation = rig.provision_ok(
        label(row, "provision_also_classifies"),
        PageKind::Snapshot,
        1,
        Funding::Caller,
        Some(&attacker()),
    );
    assert_eq!(rig.ops().unallocated_excess, excess_before + 5);
    assert_eq!(
        rig.ops().free_balance,
        free_before,
        "Caller funding does not touch free_balance"
    );
    observations.push(observation);
    observations
}

/// The measured rent per account kind, at LiteSVM's default rent schedule.
///
/// Each kind is created through the program with `Funding::Free` and its
/// lamports read back; the rows above assert the same values against
/// `minimum_balance_for_rent_exemption(T::SPACE)`.  The numbers ride in the
/// `unsupported` note of a dedicated carrier observation so a report can label
/// them "local default rent (LiteSVM)", distinct from the mainnet schedule.
pub fn rent_table() -> Vec<Observation> {
    let row = ROW_RENT_TABLE;
    let mut rig = Rig::genesis();
    let mut observations = Vec::new();
    let funded = rig.fund(label(row, "fund_ops"), FUND);
    rig.ok(&funded);
    observations.push(funded);
    let observation = rig.provision_record(
        label(row, "provision_close_record_1"),
        1,
        Funding::Free,
        None,
    );
    rig.ok(&observation);
    observations.push(observation);
    for (name, kind) in [
        ("registry", PageKind::Registry),
        ("snapshot", PageKind::Snapshot),
        ("participant", PageKind::Participant { round: 1 }),
        ("candidate", candidate(1, 0)),
    ] {
        let observation = rig.provision_ok(
            label(row, &format!("provision_{name}_0")),
            kind,
            0,
            Funding::Free,
            None,
        );
        observations.push(observation);
    }

    let measured = |key: Pubkey, space: usize| {
        let account = rig.rt.account(key);
        assert_eq!(account.data.len(), space);
        assert_eq!(
            account.lamports,
            rig.rt.rent(space),
            "measured lamports equal the rent floor for {space} bytes"
        );
        serde_json::json!({ "space_bytes": space, "rent_lamports": account.lamports })
    };
    let rent: Rent = rig.rt.svm.get_sysvar::<Rent>();
    // The plan's "6,960 coefficient": 3,480 lamports per byte-year, two years.
    let per_byte = rent.lamports_per_byte_year as f64 * rent.exemption_threshold;
    assert_eq!(per_byte, 6_960.0, "LiteSVM's default rent coefficient");
    for space in [
        state::RegistryPage::SPACE,
        state::SnapshotPage::SPACE,
        state::ParticipantPage::SPACE,
        state::CandidatePage::SPACE,
        state::CloseRecord::SPACE,
    ] {
        assert_eq!(
            rig.rt.rent(space),
            (128 + space as u64) * 6_960,
            "rent is (128-byte account overhead + data) x 6,960 lamports"
        );
    }
    let table = serde_json::json!({
        "label": "local default rent (LiteSVM); not the mainnet schedule",
        "lamports_per_byte_year": rent.lamports_per_byte_year,
        "exemption_threshold": rent.exemption_threshold,
        "lamports_per_byte_with_threshold": 6_960,
        "account_storage_overhead_bytes": 128,
        "rent_floor_for_zero_data": rig.rt.rent(0),
        "registry_page": measured(page_key(PageKind::Registry, 0), state::RegistryPage::SPACE),
        "snapshot_page": measured(page_key(PageKind::Snapshot, 0), state::SnapshotPage::SPACE),
        "participant_page": measured(page_key(PageKind::Participant { round: 1 }, 0), state::ParticipantPage::SPACE),
        "candidate_page": measured(page_key(candidate(1, 0), 0), state::CandidatePage::SPACE),
        "close_record": measured(rig.rt.close_record_address(1), state::CloseRecord::SPACE),
    });

    // The carrier is a real, harmless `reconcile_ops_cash` call.
    let ops_cash = rig.rt.ops_cash_address();
    let instruction = reconcile_ix(&rig.rt, ops_cash);
    let mut observation = rig.exec(label(row, ""), instruction, &[]);
    rig.ok(&observation);
    observation.unsupported = Some(format!("rent_table: {table}"));
    observations.push(observation);
    observations
}
