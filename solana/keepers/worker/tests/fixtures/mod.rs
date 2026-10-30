//! Worlds for the keeper tests: serialized program accounts loaded into a
//! [`MockChain`] (for the full `once` path) and directly built [`Snapshot`]s
//! (for the pure planner).  No network, no files, no keypair files.

#![allow(dead_code)]

use std::{fs, path::Path};

use anchor_lang::solana_program::system_program;
use anchor_lang::AccountSerialize;
use musical_chairs_keeper::{
    plan::Limits,
    rpc::MockChain,
    state::{
        candidate_address, close_record_address, config_address, ops_address, ops_cash_address,
        participant_address, pool_address, registry_address, ticket_address, ClientView,
        SharedPage, Snapshot, SystemAccountView, PROGRAM_ID,
    },
};
use musical_chairs_program::{
    close::provider_deficit,
    provider::{
        self,
        orao::{
            self,
            state::{
                network_state::{NetworkConfiguration, NetworkState},
                request::{Fulfilled, Pending, RequestAccount, RequestState},
            },
        },
    },
    state::{
        Backer, CandidatePage, CloseRecord, ConfigVersion, Ops, ParticipantPage, Pool, Promotion,
        RegistryPage, RentComponents, ReserveSchedule, SnapshotPage, SnapshotRow, Ticket, ALLOCATE,
        COMMIT, DISCOVER, DRAW_CAPTURED, DRAW_NOT_REQUESTED, DRAW_REQUESTED, PRUNE, SEAL, SENIOR,
    },
    PageKind,
};
use solana_sdk::{account::Account, pubkey::Pubkey, rent::Rent};

/// Wall-clock and cluster-clock "now" of every fixture (unix seconds).
pub const NOW: u64 = 1_700_000_000;

pub fn program_account<T: AccountSerialize>(value: &T) -> Account {
    let mut data = Vec::new();
    value
        .try_serialize(&mut data)
        .expect("anchor account serializes");
    Account {
        lamports: 1_000_000_000,
        data,
        owner: PROGRAM_ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// A deterministic key that is none of the program's PDAs, the system program or the payer placeholder.
pub fn key(n: u64) -> Pubkey {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&n.to_le_bytes());
    bytes[31] = 0x42;
    Pubkey::new_from_array(bytes)
}

/// The Backer registered at registry slot `slot`.
pub fn backer_key(slot: u64) -> Pubkey {
    key(1_000 + slot)
}

pub fn registry_page(index: u64, registry_len: u64) -> RegistryPage {
    let mut page = RegistryPage {
        index,
        ..RegistryPage::default()
    };
    for offset in 0..8u64 {
        let slot = index * 8 + offset;
        if slot < registry_len {
            page.keys[offset as usize] = backer_key(slot);
        }
    }
    page
}

/// Snapshot page `index` holding the captured rows of slots below `len`; `closed(slot)` rows are captured at 5 lamports.
pub fn snapshot_page(index: u64, len: u64, closed: impl Fn(u64) -> bool) -> SnapshotPage {
    let mut page = SnapshotPage {
        index,
        ticket_id: 1,
        ..SnapshotPage::default()
    };
    for offset in 0..8u64 {
        let slot = index * 8 + offset;
        if slot < len {
            page.rows[offset as usize] = SnapshotRow {
                key: backer_key(slot),
                series: 0,
                remaining: if closed(slot) { 5 } else { 500 },
                slot,
            };
        }
    }
    page
}

fn ticket_pool(captured_len: u64) -> Pool {
    Pool {
        round_id: 1,
        active_config_version: 1,
        registry_len: captured_len,
        registry_pages: captured_len.div_ceil(8),
        snapshot_pages: captured_len.div_ceil(8),
        next_ticket: 2,
        active_ticket: 1,
        next_close_id: 1,
        cluster_domain: [9; 32],
        ..Pool::default()
    }
}

// ---------------------------------------------------------------------------
// MockChain worlds (load_snapshot + once)
// ---------------------------------------------------------------------------

/// A ticket in Discover with `captured_len` registry rows (and their registry pages) at `cursor`.
pub fn discover_world(chain: &MockChain, cursor: u64, captured_len: u64) {
    let pool = ticket_pool(captured_len);
    let ticket = Ticket {
        id: 1,
        phase: DISCOVER,
        captured_len,
        cursor,
        expected_version: pool.registry_version,
        ..Ticket::default()
    };
    chain.set_clock(NOW as i64, 100);
    chain.set_account(pool_address(), program_account(&pool));
    chain.set_account(ticket_address(), program_account(&ticket));
    for index in 0..captured_len.div_ceil(8) {
        chain.set_account(
            registry_address(index),
            program_account(&registry_page(index, captured_len)),
        );
    }
}

/// A ticket in Senior (no open Series A captured) or, after `advance_to_seal`, in Seal: both need no pages.
pub fn senior_world(chain: &MockChain) {
    let pool = ticket_pool(8);
    let ticket = Ticket {
        id: 1,
        phase: SENIOR,
        captured_len: 8,
        cursor: 8,
        a_count: 0,
        b_count: 1,
        ..Ticket::default()
    };
    chain.set_clock(NOW as i64, 100);
    chain.set_account(pool_address(), program_account(&pool));
    chain.set_account(ticket_address(), program_account(&ticket));
}

/// The ticket moves on from Senior to Seal (what the cluster does once `apply_senior` lands).
pub fn advance_to_seal(chain: &MockChain) {
    let ticket = Ticket {
        id: 1,
        phase: SEAL,
        captured_len: 8,
        cursor: 8,
        a_count: 0,
        b_count: 1,
        ..Ticket::default()
    };
    chain.set_account(ticket_address(), program_account(&ticket));
}

// ---------------------------------------------------------------------------
// Directly built snapshots (pure planner)
// ---------------------------------------------------------------------------

fn ticket_snapshot(ticket: Ticket, registry_len: u64) -> Snapshot {
    let mut snapshot = Snapshot::new(ticket_pool(registry_len), NOW as i64);
    snapshot.ticket = Some(ticket);
    snapshot
}

fn with_registry_pages(mut snapshot: Snapshot) -> Snapshot {
    let len = snapshot.pool.registry_len;
    for index in 0..len.div_ceil(8) {
        snapshot
            .registry_pages
            .insert(index, registry_page(index, len));
    }
    snapshot
}

/// The pool of a close in progress.
fn closing_pool() -> Pool {
    Pool {
        round_id: 1,
        active_config_version: 1,
        next_close_id: 2,
        current_close_id: Some(1),
        registry_len: 12,
        registry_pages: 2,
        snapshot_pages: 2,
        participant_count: 10,
        participant_pages: 2,
        cluster_domain: [9; 32],
        ..Pool::default()
    }
}

/// A close: 10 participants, 2 candidate pages required per set, cash committed, deadline 500 s ahead.
fn base_close() -> CloseRecord {
    CloseRecord {
        close_id: 1,
        round_id: 1,
        config_version: 1,
        close_started_at: NOW - 100,
        promotion_deadline: NOW + 500,
        cash_committed: true,
        participant_count: 10,
        required_pages_per_set: 2,
        owner_refund_destination: key(0x77),
        ..CloseRecord::default()
    }
}

fn provider_config() -> ConfigVersion {
    ConfigVersion {
        provider_program: key(0xA1),
        provider_network_state: key(0xA2),
        provider_treasury: key(0xA3),
        provider_client: key(0xA4),
        ..ConfigVersion::default_v1()
    }
}

fn closing_snapshot() -> Snapshot {
    let mut snapshot = Snapshot::new(closing_pool(), NOW as i64);
    snapshot.close = Some(base_close());
    snapshot.config = Some(provider_config());
    snapshot
}

fn close_mut(snapshot: &mut Snapshot) -> &mut CloseRecord {
    snapshot.close.as_mut().expect("closing snapshot")
}

/// One snapshot per kind of step the planner can emit, with the limits to plan it under.
/// Order: ticket phases (6), closing pipeline (7), opt-in close-record provisioning (1), then the IMPL-49d
/// operations (4): increase_provider_budget, reconcile_ops_cash, reconcile_ops_cash_with_client,
/// close_shared_account.  New scenarios are only ever appended, so earlier indexes stay valid.
pub fn scenarios() -> Vec<(&'static str, Snapshot, Limits)> {
    let limits = Limits::default();
    let mut all: Vec<(&'static str, Snapshot, Limits)> = Vec::new();

    // 1. Discover: 12 rows captured, cursor 0 -> one full page.
    let discover = Ticket {
        id: 1,
        phase: DISCOVER,
        captured_len: 12,
        cursor: 0,
        ..Ticket::default()
    };
    all.push((
        "discover_rows",
        with_registry_pages(ticket_snapshot(discover, 12)),
        limits,
    ));

    // 2. Senior with one open Series A (the oldest is named).
    let senior = Ticket {
        id: 1,
        phase: SENIOR,
        captured_len: 12,
        cursor: 12,
        a_count: 1,
        b_count: 2,
        oldest: backer_key(0),
        ..Ticket::default()
    };
    all.push(("apply_senior", ticket_snapshot(senior, 12), limits));

    // 3. Seal.
    let seal = Ticket {
        id: 1,
        phase: SEAL,
        captured_len: 12,
        cursor: 12,
        a_count: 1,
        b_count: 2,
        ..Ticket::default()
    };
    all.push(("seal_weights", ticket_snapshot(seal, 12), limits));

    // 4. Allocate: snapshot page 0 holds slots 0..8.
    let allocate = Ticket {
        id: 1,
        phase: ALLOCATE,
        captured_len: 12,
        cursor: 0,
        ..Ticket::default()
    };
    let mut snapshot = ticket_snapshot(allocate, 12);
    snapshot
        .snapshot_pages
        .insert(0, snapshot_page(0, 12, |_| false));
    all.push(("allocate_rows", snapshot, limits));

    // 5. Prune from cursor 12 downwards: slot 11 was captured closed (and is the last live slot), 8..10 stay open.
    let prune = Ticket {
        id: 1,
        phase: PRUNE,
        captured_len: 12,
        cursor: 12,
        ..Ticket::default()
    };
    let mut snapshot = with_registry_pages(ticket_snapshot(prune, 12));
    snapshot
        .snapshot_pages
        .insert(0, snapshot_page(0, 12, |slot| slot == 11));
    snapshot
        .snapshot_pages
        .insert(1, snapshot_page(1, 12, |slot| slot == 11));
    for slot in 4..12u64 {
        let lifetime = if slot == 11 { 995 } else { 500 };
        snapshot.backers.insert(
            backer_key(slot),
            Backer {
                entitlement: 1_000,
                lifetime,
                active: true,
                slot,
                ..Backer::default()
            },
        );
    }
    all.push(("prune_rows", snapshot, limits));

    // 6. Commit of a closing payout's ticket (names the close record).
    let commit = Ticket {
        id: 1,
        phase: COMMIT,
        captured_len: 12,
        cursor: 0,
        close_id: Some(1),
        ..Ticket::default()
    };
    all.push(("commit_ticket", ticket_snapshot(commit, 12), limits));

    // 7. Closing, candidates unsealed, no candidate page provisioned yet.
    all.push(("provision_page", closing_snapshot(), limits));

    // 8. Closing, both sets fully provisioned, 3 of 10 participants visited.
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.provisioned_preferred_pages = 2;
        close.provisioned_fallback_pages = 2;
        close.candidate_cursor = 3;
        close.preferred_count = 2;
        close.fallback_count = 3;
    }
    let mut page = ParticipantPage {
        round_id: 1,
        page: 0,
        len: 8,
        ..ParticipantPage::default()
    };
    for offset in 0..8u64 {
        page.rows[offset as usize] = key(2_000 + offset);
    }
    snapshot.participant_pages.insert(0, page);
    all.push(("build_candidates", snapshot, limits));

    // 9. Sealed, no draw requested.
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.provisioned_preferred_pages = 2;
        close.provisioned_fallback_pages = 2;
        close.candidate_cursor = 10;
        close.sealed = true;
        close.selected_set_tag = 0;
        close.preferred_count = 3;
        close.preferred_digest = [4; 32];
        close.draw_state = DRAW_NOT_REQUESTED;
    }
    all.push(("request_round_draw", snapshot, limits));

    // 10. Draw requested, not captured, deadline ahead.
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.candidate_cursor = 10;
        close.sealed = true;
        close.preferred_count = 3;
        close.draw_state = DRAW_REQUESTED;
        close.request_account = key(0xB1);
    }
    all.push(("capture_round_draw", snapshot, limits));

    // 11. Captured: one preferred candidate, so the first sample always maps to ordinal 0.
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.candidate_cursor = 10;
        close.sealed = true;
        close.selected_set_tag = 0;
        close.preferred_count = 1;
        close.draw_state = DRAW_CAPTURED;
        close.randomness = [3; 64];
    }
    let mut page = CandidatePage {
        close_id: 1,
        set_tag: 0,
        page: 0,
        len: 1,
        ..CandidatePage::default()
    };
    page.rows[0] = key(0x55);
    snapshot.candidate_pages.insert((0, 0), page);
    all.push(("apply_promotion", snapshot, limits));

    // 12. Requested but never captured, and the cluster clock is past the 600 s deadline.
    let mut snapshot = closing_snapshot();
    snapshot.clock_unix = (NOW + 1_000) as i64;
    {
        let close = close_mut(&mut snapshot);
        close.candidate_cursor = 10;
        close.sealed = true;
        close.preferred_count = 3;
        close.draw_state = DRAW_REQUESTED;
        close.request_account = key(0xB1);
    }
    all.push(("expire_promotion", snapshot, limits));

    // 13. Promotion terminal and its reserve settled: finalize.
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.candidate_cursor = 10;
        close.sealed = true;
        close.promotion = Promotion::Skipped {
            reason: 1,
            stage: 1,
        };
        close.reserve_settled = true;
    }
    all.push(("finalize_round", snapshot, limits));

    // 14. Idle pool, opt-in: provision the next close record.
    let pool = Pool {
        round_id: 1,
        active_config_version: 1,
        next_close_id: 3,
        cluster_domain: [9; 32],
        ..Pool::default()
    };
    let snapshot = Snapshot::new(pool, NOW as i64);
    all.push((
        "provision_close_record",
        snapshot,
        Limits {
            preprovision_close_record: true,
            ..limits
        },
    ));

    // 15. A sealed close whose draw would be refused for want of 400,000 lamports of reserve, opted in.
    let needed = request_fee() + request_rent() + b_account_rent();
    all.push((
        "increase_provider_budget",
        topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 5_000_000),
        Limits {
            max_provider_topup_lamports: 1_000_000,
            ..limits
        },
    ));

    // 16. Idle books with an unsolicited credit in OpsCash and no ORAO Client at the derived address.
    all.push(("reconcile_ops_cash", books_snapshot(5), limits));

    // 17. The same, and the ORAO Client balance moved since it was last observed.
    let mut snapshot = with_client(books_snapshot(0), 100_000);
    snapshot.client = Some(ClientView {
        lamports: 100_007,
        data_len: 100,
    });
    all.push(("reconcile_ops_cash_with_client", snapshot, limits));

    // 18. A finished close with a released candidate page owing an owner component to its recorded destination.
    all.push(("close_shared_account", cleanup_snapshot(), limits));

    all
}

// ---------------------------------------------------------------------------
// The program IDL
// ---------------------------------------------------------------------------

/// One flattened IDL account of an instruction.
#[derive(Debug, Clone)]
pub struct IdlAccount {
    pub name: String,
    pub signer: bool,
    pub writable: bool,
}

fn flatten(accounts: &serde_json::Value, out: &mut Vec<IdlAccount>) {
    for account in accounts.as_array().expect("idl accounts array") {
        if let Some(nested) = account.get("accounts") {
            flatten(nested, out);
        } else {
            out.push(IdlAccount {
                name: account["name"]
                    .as_str()
                    .expect("idl account name")
                    .to_string(),
                signer: account
                    .get("signer")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                writable: account
                    .get("writable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            });
        }
    }
}

/// The frontend's checked-in copy of the program IDL: the accounts of `instruction` in order.
pub fn idl_accounts(instruction: &str) -> Vec<IdlAccount> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../frontend/src/solana/native/idl/musical_chairs.json");
    let idl: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the IDL"))
            .expect("parse the IDL");
    let found = idl["instructions"]
        .as_array()
        .expect("idl instructions")
        .iter()
        .find(|ix| ix["name"] == instruction)
        .unwrap_or_else(|| panic!("instruction {instruction} is not in the IDL"));
    let mut out = Vec::new();
    flatten(&found["accounts"], &mut out);
    out
}

// ---------------------------------------------------------------------------
// More worlds
// ---------------------------------------------------------------------------

/// A Prune ticket over `len` registry rows with every row captured open, except where `closed(slot)`
/// (captured and live remaining 5).  The cursor is at `len`, so the first batch walks down from the top.
pub fn prune_snapshot(len: u64, closed: impl Fn(u64) -> bool + Copy) -> Snapshot {
    let ticket = Ticket {
        id: 1,
        phase: PRUNE,
        captured_len: len,
        cursor: len,
        ..Ticket::default()
    };
    let mut snapshot = with_registry_pages(ticket_snapshot(ticket, len));
    for index in 0..len.div_ceil(8) {
        snapshot
            .snapshot_pages
            .insert(index, snapshot_page(index, len, closed));
    }
    for slot in 0..len {
        let lifetime = if closed(slot) { 995 } else { 500 };
        snapshot.backers.insert(
            backer_key(slot),
            Backer {
                entitlement: 1_000,
                lifetime,
                active: true,
                slot,
                ..Backer::default()
            },
        );
    }
    snapshot
}

fn orao_account(request: &RequestAccount) -> Account {
    let mut data = Vec::new();
    request
        .try_serialize(&mut data)
        .expect("orao request serializes");
    Account {
        lamports: 1_000_000_000,
        data,
        owner: orao::id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// A close in progress whose candidates are sealed and whose draw was requested, in the mock.
/// `request`: `Some(false)` an ORAO request still pending, `Some(true)` fulfilled, `None` no request account.
pub fn closing_world(chain: &MockChain, request: Option<bool>) {
    let close = CloseRecord {
        candidate_cursor: 10,
        sealed: true,
        selected_set_tag: 0,
        preferred_count: 3,
        preferred_digest: [4; 32],
        provisioned_preferred_pages: 2,
        provisioned_fallback_pages: 2,
        draw_state: DRAW_REQUESTED,
        request_account: key(0xB1),
        ..base_close()
    };
    chain.set_clock(NOW as i64, 100);
    chain.set_account(pool_address(), program_account(&closing_pool()));
    chain.set_account(close_record_address(1), program_account(&close));
    chain.set_account(config_address(1), program_account(&provider_config()));
    match request {
        None => chain.remove_account(&close.request_account),
        Some(fulfilled) => {
            let state = if fulfilled {
                RequestState::Fulfilled(Fulfilled::new([7; 64], None))
            } else {
                RequestState::Pending(Pending::new(Vec::new(), None, false))
            };
            let request = RequestAccount::new(255, 1, key(0xA4), close.protocol_seed, state);
            chain.set_account(close.request_account, orao_account(&request));
        }
    }
}

/// A finished close whose owner share stayed locked (a deferred refund of `owed` lamports).
pub fn settled_close(owed: u64) -> CloseRecord {
    CloseRecord {
        finalized: true,
        reserve_settled: true,
        promotion: Promotion::Skipped {
            reason: 1,
            stage: 1,
        },
        locked_owner_remaining: owed,
        draw_state: DRAW_NOT_REQUESTED,
        ..base_close()
    }
}

/// An idle pool in the mock; `last_close` (if any) is `CloseRecord(1)` and the pool's `next_close_id` is 2.
pub fn idle_world(chain: &MockChain, last_close: Option<&CloseRecord>) {
    let pool = Pool {
        round_id: 2,
        active_config_version: 1,
        next_close_id: if last_close.is_some() { 2 } else { 1 },
        cluster_domain: [9; 32],
        ..Pool::default()
    };
    chain.set_clock(NOW as i64, 100);
    chain.set_account(pool_address(), program_account(&pool));
    if let Some(close) = last_close {
        chain.set_account(close_record_address(close.close_id), program_account(close));
    }
}

/// An idle snapshot whose most recent close is `close`.
pub fn idle_snapshot(close: CloseRecord) -> Snapshot {
    let pool = Pool {
        round_id: 2,
        next_close_id: 2,
        cluster_domain: [9; 32],
        ..Pool::default()
    };
    let mut snapshot = Snapshot::new(pool, NOW as i64);
    snapshot.last_close = Some(close);
    snapshot
}

// ---------------------------------------------------------------------------
// IMPL-49d worlds: provider budget, OpsCash books, shared-page cleanup
// ---------------------------------------------------------------------------

/// The ORAO per-request fee of the fixtures.
pub fn request_fee() -> u64 {
    360_000
}

/// Rent of the provider's pending request account (`ConfigVersion::default_v1().pending_request_bytes` = 759 bytes).
pub fn request_rent() -> u64 {
    Rent::default().minimum_balance(usize::from(
        ConfigVersion::default_v1().pending_request_bytes,
    ))
}

/// Rent of the Series-B Backer account the promotion still needs (`promotion_holdback`, no registry extension).
pub fn b_account_rent() -> u64 {
    Rent::default().minimum_balance(Backer::SPACE)
}

/// A close that is sealed, with the draw NOT requested yet, holding `sponsor` / `owner` lamports of locked
/// reserve and a provider-fee `cap`; the live ORAO fee is `fee` and the free operations balance `free`.
pub fn topup_snapshot(sponsor: u64, owner: u64, cap: u64, fee: u64, free: u64) -> Snapshot {
    let mut snapshot = closing_snapshot();
    {
        let close = close_mut(&mut snapshot);
        close.candidate_cursor = 10;
        close.sealed = true;
        close.selected_set_tag = 0;
        close.preferred_count = 3;
        close.preferred_digest = [4; 32];
        close.provisioned_preferred_pages = 2;
        close.provisioned_fallback_pages = 2;
        close.draw_state = DRAW_NOT_REQUESTED;
        close.locked_sponsor_remaining = sponsor;
        close.locked_owner_remaining = owner;
        close.approved_provider_fee_cap = cap;
        close.registry_extension_required = false;
        close.reserve_schedule = ReserveSchedule {
            b_account_rent: b_account_rent(),
            ..ReserveSchedule::default()
        };
    }
    snapshot.provider_fee = Some(fee);
    snapshot.ops = Some(Ops {
        free_balance: free,
        ..Ops::default()
    });
    snapshot
}

fn idle_pool() -> Pool {
    Pool {
        round_id: 2,
        active_config_version: 1,
        next_close_id: 2,
        cluster_domain: [9; 32],
        ..Pool::default()
    }
}

/// An idle pool whose Ops books agree with the physical OpsCash account, plus `donation` unsolicited lamports
/// in OpsCash.  No ORAO Client is loaded.
pub fn books_snapshot(donation: u64) -> Snapshot {
    let floor = Rent::default().minimum_balance(0);
    let (free, locked) = (10_000_000, 2_000_000);
    let mut snapshot = Snapshot::new(idle_pool(), NOW as i64);
    snapshot.ops = Some(Ops {
        free_balance: free,
        locked_remaining_total: locked,
        unallocated_excess: 0,
        protected_rent: floor,
        ..Ops::default()
    });
    snapshot.ops_cash = Some(SystemAccountView {
        lamports: floor + free + locked + donation,
        plain_system_account: true,
    });
    snapshot
}

/// `books_snapshot` plus an ORAO Client at the derived address holding `lamports`, observed in full by the books.
pub fn with_client(mut snapshot: Snapshot, lamports: u64) -> Snapshot {
    let rent = Rent::default().minimum_balance(100);
    snapshot.client = Some(ClientView {
        lamports,
        data_len: 100,
    });
    let ops = snapshot.ops.as_mut().expect("books snapshot");
    ops.client_balance_observed = lamports;
    ops.client_protected_rent = rent;
    ops.provider_deficit = provider_deficit(ops.eligible_return_budget, lamports, rent);
    snapshot
}

/// An existing candidate page of `close_id`'s set `set_tag`, as the loader would report it.
pub fn candidate_shared_page(
    close_id: u64,
    set_tag: u8,
    index: u64,
    rent: RentComponents,
    lamports: u64,
) -> SharedPage {
    SharedPage {
        kind: PageKind::Candidate { close_id, set_tag },
        index,
        address: candidate_address(close_id, set_tag, index),
        lamports,
        rent,
    }
}

/// An existing participant page of `round`.
pub fn participant_shared_page(
    round: u64,
    index: u64,
    rent: RentComponents,
    lamports: u64,
) -> SharedPage {
    SharedPage {
        kind: PageKind::Participant { round },
        index,
        address: participant_address(round, index),
        lamports,
        rent,
    }
}

/// The rent components the cleanup fixtures record on their candidate page: an ops share and an owner share
/// refunded to `key(0x88)`.
pub fn recorded_rent() -> RentComponents {
    RentComponents {
        ops: 1_000_000,
        owner: 2_000_000,
        owner_refund: key(0x88),
    }
}

/// A finished (finalized, terminal-promotion) close 1, healthy books and two released pages: candidate page 0
/// of set 0 (owes `key(0x88)` an owner share) and participant page 0 (ops share only).
pub fn cleanup_snapshot() -> Snapshot {
    let mut snapshot = books_snapshot(0);
    snapshot.last_close = Some(settled_close(0));
    snapshot.shared_pages = vec![
        candidate_shared_page(1, 0, 0, recorded_rent(), 3_000_000),
        participant_shared_page(
            1,
            0,
            RentComponents {
                ops: 500_000,
                owner: 0,
                owner_refund: Pubkey::default(),
            },
            500_000,
        ),
    ];
    snapshot.page_refund_targets.insert(
        key(0x88),
        SystemAccountView {
            lamports: 0,
            plain_system_account: true,
        },
    );
    snapshot
}

fn ops_account(ops: &Ops) -> Account {
    program_account(ops)
}

fn system_account(lamports: u64) -> Account {
    Account {
        lamports,
        data: Vec::new(),
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn network_account(fee: u64) -> Account {
    let network = NetworkState::new(255, NetworkConfiguration::new(key(0xC0), key(0xA3), fee));
    let mut data = Vec::new();
    anchor_lang::AccountSerialize::try_serialize(&network, &mut data)
        .expect("network state serializes");
    Account {
        lamports: 1_000_000_000,
        data,
        owner: orao::id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// The mock twin of `topup_snapshot`: a sealed close, draw not requested, the Ops account and the ORAO
/// NetworkState (the live fee) on chain.
pub fn topup_world(chain: &MockChain, sponsor: u64, cap: u64, fee: u64, free: u64) {
    let close = CloseRecord {
        candidate_cursor: 10,
        sealed: true,
        selected_set_tag: 0,
        preferred_count: 3,
        preferred_digest: [4; 32],
        provisioned_preferred_pages: 2,
        provisioned_fallback_pages: 2,
        draw_state: DRAW_NOT_REQUESTED,
        locked_sponsor_remaining: sponsor,
        approved_provider_fee_cap: cap,
        reserve_schedule: ReserveSchedule {
            b_account_rent: b_account_rent(),
            ..ReserveSchedule::default()
        },
        ..base_close()
    };
    chain.set_clock(NOW as i64, 100);
    chain.set_account(pool_address(), program_account(&closing_pool()));
    chain.set_account(close_record_address(1), program_account(&close));
    chain.set_account(config_address(1), program_account(&provider_config()));
    chain.set_account(
        ops_address(),
        ops_account(&Ops {
            free_balance: free,
            ..Ops::default()
        }),
    );
    chain.set_account(key(0xA2), network_account(fee));
}

/// The mock twin of `books_snapshot` (+ `with_client` when `client_lamports` is `Some`): the Ops account whose
/// books agree with OpsCash, `donation` unsolicited lamports in OpsCash, the ORAO Client holding
/// `client_lamports` while the books observed `observed_client_lamports`.
pub fn books_world(chain: &MockChain, donation: u64, client: Option<(u64, u64)>) {
    idle_world(chain, None);
    let floor = Rent::default().minimum_balance(0);
    let (free, locked) = (10_000_000, 2_000_000);
    let mut ops = Ops {
        free_balance: free,
        locked_remaining_total: locked,
        unallocated_excess: 0,
        protected_rent: floor,
        ..Ops::default()
    };
    if let Some((observed, actual)) = client {
        let rent = Rent::default().minimum_balance(100);
        ops.client_balance_observed = observed;
        ops.client_protected_rent = rent;
        ops.provider_deficit = provider_deficit(ops.eligible_return_budget, observed, rent);
        chain.set_account(
            provider::client_address().0,
            Account {
                lamports: actual,
                data: vec![0; 100],
                owner: orao::id(),
                executable: false,
                rent_epoch: 0,
            },
        );
    }
    chain.set_account(ops_address(), ops_account(&ops));
    chain.set_account(
        ops_cash_address(),
        system_account(floor + free + locked + donation),
    );
}

/// A finished close 1 on chain with one released candidate page (set 0, index 0) that owes `key(0x88)` an owner
/// share; healthy books.  Returns the page's address.
pub fn cleanup_world(chain: &MockChain) -> Pubkey {
    let last = CloseRecord {
        provisioned_preferred_pages: 1,
        ..settled_close(0)
    };
    books_world(chain, 0, None);
    idle_world(chain, Some(&last));
    let page = CandidatePage {
        close_id: 1,
        set_tag: 0,
        page: 0,
        len: 0,
        rent: recorded_rent(),
        ..CandidatePage::default()
    };
    let address = candidate_address(1, 0, 0);
    chain.set_account(
        address,
        Account {
            lamports: 3_000_000,
            ..program_account(&page)
        },
    );
    address
}
