//! Decoded view of the program accounts the planner needs, and the PDA
//! derivations (the program's own seed constants, so they cannot drift).
//!
//! The planner is pure, so everything it may look at is loaded here, once per
//! pass, into a [`Snapshot`].  Loading follows the same priority the planner
//! uses: when an allocation ticket is active only the ticket's pages are
//! loaded; otherwise, when a close is in progress, only the close's accounts;
//! otherwise the pool is idle.  Page windows are loaded generously (the eight
//! slots a maximal batch could touch, which is at most two pages) so the
//! loader does not depend on the planner's batch alignment.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use anchor_lang::solana_program::system_program;
use anchor_lang::AccountDeserialize;
use musical_chairs_program::{
    close::selected_set,
    provider::orao::{
        self,
        state::{
            network_state::NetworkState,
            request::{RequestAccount, RequestState},
        },
    },
    sampling::map_output,
    state::{
        Backer, CandidatePage, CloseRecord, ConfigVersion, Ops, ParticipantPage, Pool, Promotion,
        RegistryPage, RentComponents, SnapshotPage, Ticket, ALLOCATE, DISCOVER, DRAW_CAPTURED,
        DRAW_NOT_REQUESTED, DRAW_REQUESTED, PAGE_CAPACITY, PRUNE,
    },
    PageKind,
};
use solana_sdk::{account::Account, clock::Clock, pubkey::Pubkey, rent::Rent, sysvar};

use crate::rpc::{ChainReader, RpcError};

pub use musical_chairs_program::ID as PROGRAM_ID;

/// Rows per page, as a slot divisor.
pub const PAGE: u64 = PAGE_CAPACITY as u64;

/// Most shared-page accounts one idle pass probes for cleanup (the last finished close only).  A close with
/// more candidate and participant pages than this is cleaned only up to the cap by this keeper; any wallet can
/// close the rest.
pub const CLEANUP_PROBE_CAP: usize = 512;

// ---------------------------------------------------------------------------
// PDA derivations
// ---------------------------------------------------------------------------

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &PROGRAM_ID).0
}

pub fn pool_address() -> Pubkey {
    pda(&[musical_chairs_program::state::POOL_SEED])
}
pub fn ops_address() -> Pubkey {
    pda(&[musical_chairs_program::state::OPS_SEED])
}
pub fn ops_cash_address() -> Pubkey {
    pda(&[musical_chairs_program::state::OPS_CASH_SEED])
}
pub fn ticket_address() -> Pubkey {
    pda(&[musical_chairs_program::state::TICKET_SEED])
}
pub fn rng_authority_address() -> Pubkey {
    pda(&[musical_chairs_program::state::RNG_AUTHORITY_SEED])
}
/// Anchor `#[event_cpi]` self-CPI authority (`__event_authority`).
pub fn event_authority_address() -> Pubkey {
    pda(&[b"__event_authority"])
}
pub fn config_address(version: u32) -> Pubkey {
    pda(&[
        musical_chairs_program::state::CONFIG_SEED,
        &version.to_le_bytes(),
    ])
}
pub fn close_record_address(close_id: u64) -> Pubkey {
    pda(&[
        musical_chairs_program::state::CLOSE_SEED,
        &close_id.to_le_bytes(),
    ])
}
pub fn registry_address(index: u64) -> Pubkey {
    pda(&[
        musical_chairs_program::state::REGISTRY_SEED,
        &index.to_le_bytes(),
    ])
}
pub fn snapshot_address(index: u64) -> Pubkey {
    pda(&[
        musical_chairs_program::state::SNAPSHOT_SEED,
        &index.to_le_bytes(),
    ])
}
pub fn participant_address(round: u64, index: u64) -> Pubkey {
    pda(&[
        musical_chairs_program::state::PARTICIPANTS_SEED,
        &round.to_le_bytes(),
        &index.to_le_bytes(),
    ])
}
pub fn candidate_address(close_id: u64, set_tag: u8, index: u64) -> Pubkey {
    pda(&[
        musical_chairs_program::state::CANDIDATES_SEED,
        &close_id.to_le_bytes(),
        &[set_tag],
        &index.to_le_bytes(),
    ])
}
pub fn user_address(owner: &Pubkey) -> Pubkey {
    pda(&[musical_chairs_program::state::USER_SEED, owner.as_ref()])
}
pub fn user_round_address(round: u64, owner: &Pubkey) -> Pubkey {
    pda(&[
        musical_chairs_program::state::USER_ROUND_SEED,
        &round.to_le_bytes(),
        owner.as_ref(),
    ])
}
pub fn backer_address(series: u8, owner: &Pubkey) -> Pubkey {
    pda(&[
        musical_chairs_program::state::BACKER_SEED,
        &[series],
        owner.as_ref(),
    ])
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Everything the pure planner may read.  Fields are public so tests can build
/// a world directly; [`load_snapshot`] fills them from a [`ChainReader`].
#[derive(Clone)]
pub struct Snapshot {
    /// Cluster Clock `unix_timestamp` at the read (the deadline decisions use this, not the wall clock).
    pub clock_unix: i64,
    pub pool: Pool,
    pub ticket: Option<Ticket>,
    /// The close in progress (`pool.current_close_id`), when the ticket branch did not take precedence.
    pub close: Option<CloseRecord>,
    /// The close's pinned ConfigVersion (provider tuple for the draw).
    pub config: Option<ConfigVersion>,
    pub registry_pages: BTreeMap<u64, RegistryPage>,
    pub snapshot_pages: BTreeMap<u64, SnapshotPage>,
    /// Pages of `close.round_id`'s participant list.
    pub participant_pages: BTreeMap<u64, ParticipantPage>,
    /// Candidate pages keyed by `(set_tag, page index)`; only the winner's page is loaded.
    pub candidate_pages: BTreeMap<(u8, u64), CandidatePage>,
    /// Backers named by the snapshot rows of a prune window (live remaining entitlement).
    pub backers: BTreeMap<Pubkey, Backer>,
    /// Whether `CloseRecord(pool.next_close_id)` exists (read only while the pool is idle).
    pub next_close_record_exists: bool,
    /// The most recent finished close (`next_close_id - 1`), read only while the pool is idle: the
    /// leftovers (a deferred owner refund, an unobserved provider reimbursement) the lowest-priority steps tidy.
    pub last_close: Option<CloseRecord>,
    /// Whether the ORAO request of `close` (pending promotion, draw requested) or of `last_close` (reimbursement
    /// not yet observed) is `Fulfilled`; `None` when unknown (not loaded, absent, undecodable).
    pub request_fulfilled: Option<bool>,
    /// The destination of `last_close`'s deferred owner refund, when there is one.
    pub refund_target: Option<RefundTarget>,
    /// `Rent::minimum_balance(0)`: the floor a credit to a data-less account must reach.
    pub rent_exempt_zero: u64,
    /// The Rent sysvar (idle passes, and the pre-request branch of a close).
    pub rent: Rent,
    /// The Ops account (idle passes, and the pre-request branch of a close).
    pub ops: Option<Ops>,
    /// The OpsCash PDA as a system account (idle passes); `None` when it does not exist.
    pub ops_cash: Option<SystemAccountView>,
    /// The ORAO Client at the address this program derives, when it exists and the provider owns it (idle passes).
    pub client: Option<ClientView>,
    /// The live ORAO per-request fee (a close that is sealed, not yet requested and still pending).
    pub provider_fee: Option<u64>,
    /// Existing, decodable participant / candidate pages of `last_close`, lowest index first (idle passes).
    pub shared_pages: Vec<SharedPage>,
    /// The refund destinations the program recorded on `shared_pages` that owe an owner component.
    pub page_refund_targets: BTreeMap<Pubkey, SystemAccountView>,
}

/// A system account as the program's destination checks see it: its lamports and whether it is a plain
/// (system-owned, empty, non-executable) account.  An account that does not exist counts as plain with 0 lamports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemAccountView {
    pub lamports: u64,
    pub plain_system_account: bool,
}

/// What the planner needs to know about a refund destination to run the program's
/// `credit_is_deliverable` / `validate_destination` preflight.
pub type RefundTarget = SystemAccountView;

/// The ORAO Client account as `reconcile_ops_cash_with_client` observes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientView {
    pub lamports: u64,
    pub data_len: usize,
}

/// One existing shared page that may be closable, with the rent components the program recorded on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedPage {
    /// `PageKind::Participant { round }` or `PageKind::Candidate { close_id, set_tag }`.
    pub kind: PageKind,
    pub index: u64,
    pub address: Pubkey,
    pub lamports: u64,
    pub rent: RentComponents,
}

fn view_of(account: Option<&Account>) -> SystemAccountView {
    match account {
        None => SystemAccountView {
            lamports: 0,
            plain_system_account: true,
        },
        Some(account) => SystemAccountView {
            lamports: account.lamports,
            plain_system_account: account.owner == system_program::ID
                && account.data.is_empty()
                && !account.executable,
        },
    }
}

impl Snapshot {
    /// An otherwise empty snapshot of `pool` at `clock_unix`.
    pub fn new(pool: Pool, clock_unix: i64) -> Self {
        Self {
            clock_unix,
            pool,
            ticket: None,
            close: None,
            config: None,
            registry_pages: BTreeMap::new(),
            snapshot_pages: BTreeMap::new(),
            participant_pages: BTreeMap::new(),
            candidate_pages: BTreeMap::new(),
            backers: BTreeMap::new(),
            next_close_record_exists: false,
            last_close: None,
            request_fulfilled: None,
            refund_target: None,
            rent_exempt_zero: Rent::default().minimum_balance(0),
            rent: Rent::default(),
            ops: None,
            ops_cash: None,
            client: None,
            provider_fee: None,
            shared_pages: Vec::new(),
            page_refund_targets: BTreeMap::new(),
        }
    }

    /// Cluster time as unsigned seconds (a negative Clock reads as 0).
    pub fn now(&self) -> u64 {
        u64::try_from(self.clock_unix).unwrap_or(0)
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("clock_unix", &self.clock_unix)
            .field("round_id", &self.pool.round_id)
            .field("active_ticket", &self.pool.active_ticket)
            .field("current_close_id", &self.pool.current_close_id)
            .field("ticket_phase", &self.ticket.as_ref().map(|t| t.phase))
            .field("close_id", &self.close.as_ref().map(|c| c.close_id))
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    Rpc(RpcError),
    /// The Pool account does not exist: the program is not initialized on this cluster.
    PoolMissing,
    /// An account the pool's own counters promise is absent.
    Missing(String),
    WrongOwner(String),
    Decode(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Rpc(error) => write!(f, "{error}"),
            StateError::PoolMissing => write!(
                f,
                "the Pool account does not exist (program not initialized on this cluster)"
            ),
            StateError::Missing(what) => write!(f, "missing account: {what}"),
            StateError::WrongOwner(what) => write!(f, "account {what} is not owned by the program"),
            StateError::Decode(what) => write!(f, "cannot decode {what}"),
        }
    }
}

impl std::error::Error for StateError {}

impl From<RpcError> for StateError {
    fn from(error: RpcError) -> Self {
        StateError::Rpc(error)
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Decode a program-owned account (owner and Anchor discriminator checked).
pub fn decode<T: AccountDeserialize>(what: &str, account: &Account) -> Result<T, StateError> {
    if account.owner != PROGRAM_ID {
        return Err(StateError::WrongOwner(what.to_string()));
    }
    let mut data: &[u8] = &account.data;
    T::try_deserialize(&mut data).map_err(|error| StateError::Decode(format!("{what}: {error}")))
}

fn fetch(chain: &dyn ChainReader, keys: &[Pubkey]) -> Result<Vec<Option<Account>>, StateError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let accounts = chain.get_multiple_accounts(keys)?;
    if accounts.len() != keys.len() {
        return Err(StateError::Rpc(RpcError::Unavailable(format!(
            "asked for {} accounts, got {}",
            keys.len(),
            accounts.len()
        ))));
    }
    Ok(accounts)
}

fn page_indexes(slots: std::ops::Range<u64>) -> BTreeSet<u64> {
    slots.map(|slot| slot / PAGE).collect()
}

/// Load each page in `indexes` that `have` does not hold yet; a page that does not exist is simply
/// absent (the planner raises the error only if it needs it).
fn load_pages<T: AccountDeserialize>(
    chain: &dyn ChainReader,
    what: &str,
    indexes: &BTreeSet<u64>,
    address: impl Fn(u64) -> Pubkey,
    into: &mut BTreeMap<u64, T>,
) -> Result<(), StateError> {
    let wanted: Vec<u64> = indexes
        .iter()
        .copied()
        .filter(|index| !into.contains_key(index))
        .collect();
    let keys: Vec<Pubkey> = wanted.iter().map(|index| address(*index)).collect();
    for (index, account) in wanted.iter().zip(fetch(chain, &keys)?) {
        if let Some(account) = account {
            into.insert(*index, decode(&format!("{what} {index}"), &account)?);
        }
    }
    Ok(())
}

/// Read the pool and whatever the next planning decision needs.
pub fn load_snapshot(chain: &dyn ChainReader) -> Result<Snapshot, StateError> {
    let core = fetch(
        chain,
        &[pool_address(), ticket_address(), sysvar::clock::ID],
    )?;
    let pool: Pool = decode("pool", core[0].as_ref().ok_or(StateError::PoolMissing)?)?;
    let clock: Clock = {
        let account = core[2]
            .as_ref()
            .ok_or_else(|| StateError::Missing("clock sysvar".to_string()))?;
        bincode::deserialize(&account.data)
            .map_err(|e| StateError::Decode(format!("clock sysvar: {e}")))?
    };
    let mut snapshot = Snapshot::new(pool, clock.unix_timestamp);

    if snapshot.pool.active_ticket != 0 {
        let account = core[1]
            .as_ref()
            .ok_or_else(|| StateError::Missing("ticket".to_string()))?;
        let ticket: Ticket = decode("ticket", account)?;
        load_ticket_pages(chain, &mut snapshot, &ticket)?;
        snapshot.ticket = Some(ticket);
    } else if let Some(close_id) = snapshot.pool.current_close_id {
        load_close(chain, &mut snapshot, close_id)?;
    } else {
        load_idle(chain, &mut snapshot)?;
    }
    Ok(snapshot)
}

/// Idle pool: the bookkeeping accounts, whether the next close record exists, and the leftovers of the
/// most recent close (deferred refund, unobserved request, closable shared pages).
fn load_idle(chain: &dyn ChainReader, snapshot: &mut Snapshot) -> Result<(), StateError> {
    let next_id = snapshot.pool.next_close_id;
    let mut keys = vec![
        close_record_address(next_id),
        ops_address(),
        ops_cash_address(),
        musical_chairs_program::provider::client_address().0,
        sysvar::rent::ID,
    ];
    if next_id >= 2 {
        keys.push(close_record_address(next_id - 1));
    }
    let accounts = fetch(chain, &keys)?;
    snapshot.next_close_record_exists = accounts[0].is_some();
    if let Some(account) = &accounts[1] {
        snapshot.ops = Some(decode("ops", account)?);
    }
    snapshot.ops_cash = accounts[2].as_ref().map(|account| view_of(Some(account)));
    snapshot.client = accounts[3]
        .as_ref()
        .filter(|account| account.owner == orao::id())
        .map(|account| ClientView {
            lamports: account.lamports,
            data_len: account.data.len(),
        });
    apply_rent_sysvar(snapshot, accounts[4].as_ref());

    let Some(Some(account)) = accounts.get(5) else {
        return Ok(());
    };
    let last: CloseRecord = decode("last close record", account)?;

    if last.reserve_settled && last.locked_owner_remaining > 0 {
        let fetched = fetch(chain, &[last.owner_refund_destination])?;
        snapshot.refund_target = Some(view_of(fetched[0].as_ref()));
    }
    if last.draw_state != DRAW_NOT_REQUESTED && !last.reimbursement_observed {
        snapshot.request_fulfilled = request_fulfilled(chain, &last)?;
    }
    if last.promotion != Promotion::Pending {
        load_shared_pages(chain, snapshot, &last)?;
    }
    snapshot.last_close = Some(last);
    Ok(())
}

fn apply_rent_sysvar(snapshot: &mut Snapshot, account: Option<&Account>) {
    if let Some(account) = account {
        if let Ok(rent) = bincode::deserialize::<Rent>(&account.data) {
            snapshot.rent_exempt_zero = rent.minimum_balance(0);
            snapshot.rent = rent;
        }
    }
}

/// Probe the participant (once finalized) and candidate pages of a finished close and keep the ones that
/// still exist, with the rent components the program recorded on them and the recorded refund destinations.
fn load_shared_pages(
    chain: &dyn ChainReader,
    snapshot: &mut Snapshot,
    last: &CloseRecord,
) -> Result<(), StateError> {
    let mut wanted: Vec<(PageKind, u64, Pubkey)> = Vec::new();
    for (set_tag, provisioned) in [
        (0u8, last.provisioned_preferred_pages),
        (1u8, last.provisioned_fallback_pages),
    ] {
        for index in 0..provisioned {
            let kind = PageKind::Candidate {
                close_id: last.close_id,
                set_tag,
            };
            wanted.push((
                kind,
                index,
                candidate_address(last.close_id, set_tag, index),
            ));
        }
    }
    if last.finalized {
        // Participant pages are not counted on the record: one past the pages the participants fill covers a
        // spare provisioned tail page.
        for index in 0..last.participant_count.div_ceil(PAGE).saturating_add(1) {
            let kind = PageKind::Participant {
                round: last.round_id,
            };
            wanted.push((kind, index, participant_address(last.round_id, index)));
        }
    }
    wanted.truncate(CLEANUP_PROBE_CAP);
    let keys: Vec<Pubkey> = wanted.iter().map(|(_, _, address)| *address).collect();
    for ((kind, index, address), account) in wanted.iter().zip(fetch(chain, &keys)?) {
        let Some(account) = account else { continue };
        if account.owner != PROGRAM_ID {
            continue;
        }
        let mut data: &[u8] = &account.data;
        let rent = match *kind {
            PageKind::Candidate { close_id, set_tag } => CandidatePage::try_deserialize(&mut data)
                .ok()
                .filter(|page| {
                    page.close_id == close_id && page.set_tag == set_tag && page.page == *index
                })
                .map(|page| page.rent),
            PageKind::Participant { round } => ParticipantPage::try_deserialize(&mut data)
                .ok()
                .filter(|page| page.round_id == round && page.page == *index)
                .map(|page| page.rent),
            _ => None,
        };
        if let Some(rent) = rent {
            snapshot.shared_pages.push(SharedPage {
                kind: *kind,
                index: *index,
                address: *address,
                lamports: account.lamports,
                rent,
            });
        }
    }
    let destinations: BTreeSet<Pubkey> = snapshot
        .shared_pages
        .iter()
        .filter(|page| page.rent.owner > 0)
        .map(|page| page.rent.owner_refund)
        .collect();
    let keys: Vec<Pubkey> = destinations.into_iter().collect();
    for (key, account) in keys.iter().zip(fetch(chain, &keys)?) {
        snapshot
            .page_refund_targets
            .insert(*key, view_of(account.as_ref()));
    }
    Ok(())
}

/// Whether the close's ORAO request account is `Fulfilled` (`Some(false)` pending), `None` when it
/// cannot be told (the account is absent, not ORAO-owned, undecodable, or in a state this keeper does not know).
fn request_fulfilled(
    chain: &dyn ChainReader,
    close: &CloseRecord,
) -> Result<Option<bool>, StateError> {
    if close.request_account == Pubkey::default() {
        return Ok(None);
    }
    let accounts = fetch(chain, &[close.request_account])?;
    let Some(account) = &accounts[0] else {
        return Ok(None);
    };
    if account.owner != musical_chairs_program::provider::orao::id() {
        return Ok(None);
    }
    let mut data: &[u8] = &account.data;
    Ok(match RequestAccount::try_deserialize(&mut data) {
        Ok(request) => match request.state {
            RequestState::Fulfilled(_) => Some(true),
            RequestState::Pending(_) => Some(false),
            _ => None,
        },
        Err(_) => None,
    })
}

fn load_ticket_pages(
    chain: &dyn ChainReader,
    snapshot: &mut Snapshot,
    ticket: &Ticket,
) -> Result<(), StateError> {
    let forward = ticket.cursor..ticket.captured_len.min(ticket.cursor.saturating_add(PAGE));
    match ticket.phase {
        DISCOVER => {
            load_pages(
                chain,
                "registry page",
                &page_indexes(forward),
                registry_address,
                &mut snapshot.registry_pages,
            )?;
        }
        ALLOCATE => {
            load_pages(
                chain,
                "snapshot page",
                &page_indexes(forward),
                snapshot_address,
                &mut snapshot.snapshot_pages,
            )?;
        }
        PRUNE => {
            // Prune walks DOWN from the cursor and swap-deletes against the registry tail.
            let window = ticket.cursor.saturating_sub(PAGE)..ticket.cursor;
            let len = snapshot.pool.registry_len;
            let tail = len.saturating_sub(PAGE)..len;
            let mut registry = page_indexes(window.clone());
            registry.extend(page_indexes(tail));
            load_pages(
                chain,
                "snapshot page",
                &page_indexes(window.clone()),
                snapshot_address,
                &mut snapshot.snapshot_pages,
            )?;
            load_pages(
                chain,
                "registry page",
                &registry,
                registry_address,
                &mut snapshot.registry_pages,
            )?;
            // The live remaining entitlement of each Backer the snapshot rows name decides prune vs skip.
            let mut keys = BTreeSet::new();
            for slot in window {
                let row = snapshot
                    .snapshot_pages
                    .get(&(slot / PAGE))
                    .map(|page| page.rows[(slot % PAGE) as usize]);
                if let Some(row) = row {
                    if row.key != Pubkey::default() {
                        keys.insert(row.key);
                    }
                }
            }
            let keys: Vec<Pubkey> = keys.into_iter().collect();
            for (key, account) in keys.iter().zip(fetch(chain, &keys)?) {
                if let Some(account) = account {
                    snapshot
                        .backers
                        .insert(*key, decode(&format!("backer {key}"), &account)?);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn load_close(
    chain: &dyn ChainReader,
    snapshot: &mut Snapshot,
    close_id: u64,
) -> Result<(), StateError> {
    let accounts = fetch(chain, &[close_record_address(close_id)])?;
    let account = accounts[0]
        .as_ref()
        .ok_or_else(|| StateError::Missing(format!("close record {close_id}")))?;
    let close: CloseRecord = decode("close record", account)?;

    let accounts = fetch(chain, &[config_address(close.config_version)])?;
    let account = accounts[0]
        .as_ref()
        .ok_or_else(|| StateError::Missing(format!("config version {}", close.config_version)))?;
    snapshot.config = Some(decode("config version", account)?);

    if !close.sealed {
        let end = close
            .participant_count
            .min(close.candidate_cursor.saturating_add(PAGE));
        let wanted = page_indexes(close.candidate_cursor..end);
        let round = close.round_id;
        load_pages(
            chain,
            "participant page",
            &wanted,
            |index| participant_address(round, index),
            &mut snapshot.participant_pages,
        )?;
    }
    if close.draw_state == DRAW_CAPTURED && close.promotion == Promotion::Pending {
        let (tag, count, _) = selected_set(&close);
        if let Some((ordinal, _counter)) = map_output(close.randomness, close.round_id, count) {
            let index = ordinal / PAGE;
            let accounts = fetch(chain, &[candidate_address(close.close_id, tag, index)])?;
            if let Some(account) = &accounts[0] {
                snapshot
                    .candidate_pages
                    .insert((tag, index), decode("candidate page", account)?);
            }
        }
    }
    if close.draw_state == DRAW_REQUESTED && close.promotion == Promotion::Pending {
        snapshot.request_fulfilled = request_fulfilled(chain, &close)?;
    }
    if close.sealed
        && close.draw_state == DRAW_NOT_REQUESTED
        && close.promotion == Promotion::Pending
        && close.cash_committed
    {
        load_request_inputs(chain, snapshot)?;
    }
    snapshot.close = Some(close);
    Ok(())
}

/// What the provider-budget predicate reads before a draw is requested: the Ops books, the live ORAO
/// per-request fee and the Rent sysvar.
fn load_request_inputs(chain: &dyn ChainReader, snapshot: &mut Snapshot) -> Result<(), StateError> {
    let Some(config) = &snapshot.config else {
        return Ok(());
    };
    let fetched = fetch(
        chain,
        &[
            ops_address(),
            config.provider_network_state,
            sysvar::rent::ID,
        ],
    )?;
    if let Some(account) = &fetched[0] {
        snapshot.ops = Some(decode("ops", account)?);
    }
    if let Some(account) = fetched[1].as_ref().filter(|a| a.owner == orao::id()) {
        let mut data: &[u8] = &account.data;
        if let Ok(network) = NetworkState::try_deserialize(&mut data) {
            snapshot.provider_fee = Some(network.config.request_fee);
        }
    }
    apply_rent_sysvar(snapshot, fetched[2].as_ref());
    Ok(())
}
