use anchor_lang::prelude::*;

/// The native program keeps one dense registry. Pages are transport/storage
/// units only; PAGE_CAPACITY is not an economic population cap.
pub const PAGE_CAPACITY: usize = 8;

pub const POOL_SEED: &[u8] = b"pool";
pub const ADMISSION_SEED: &[u8] = b"admission";
pub const GAME_VAULT_SEED: &[u8] = b"game-vault";
pub const OPS_SEED: &[u8] = b"ops";
pub const OPS_CASH_SEED: &[u8] = b"ops-cash";
pub const TICKET_SEED: &[u8] = b"ticket";
pub const USER_SEED: &[u8] = b"user";
pub const USER_ROUND_SEED: &[u8] = b"user-round";
pub const POSITION_SEED: &[u8] = b"position";
pub const BACKER_SEED: &[u8] = b"backer";
pub const REGISTRY_SEED: &[u8] = b"registry";
pub const SNAPSHOT_SEED: &[u8] = b"snapshot";
/// `["rng-authority"]`: the ORAO `Client.state` request authority.
pub const RNG_AUTHORITY_SEED: &[u8] = b"rng-authority";
/// `["config", version u32 LE]`.
pub const CONFIG_SEED: &[u8] = b"config";
/// `["close", close_id u64 LE]`.
pub const CLOSE_SEED: &[u8] = b"close";
/// `["participants", round u64 LE, page u64 LE]`.
pub const PARTICIPANTS_SEED: &[u8] = b"participants";
/// `["candidates", close_id u64 LE, set_tag u8, page u64 LE]`.
pub const CANDIDATES_SEED: &[u8] = b"candidates";

pub const QUOTA_CALLS: u8 = 3;
pub const QUOTA_WINDOW_SECS: u64 = 3_600;
/// Frozen promotion window (spec section 4): `promotion_deadline = close_started_at + 600`,
/// pinned once at the closing payout and never reset.
pub const PROMOTION_DEADLINE_SECS: u64 = 600;
pub const DISCOVER: u8 = 0;
pub const SENIOR: u8 = 1;
pub const SEAL: u8 = 2;
pub const ALLOCATE: u8 = 3;
pub const PRUNE: u8 = 4;
pub const COMMIT: u8 = 5;
pub const IDLE: u8 = 6;

/// `CloseRecord.draw_state` values.
pub const DRAW_NOT_REQUESTED: u8 = 0;
pub const DRAW_REQUESTED: u8 = 1;
pub const DRAW_CAPTURED: u8 = 2;
pub const DRAW_IGNORED_LATE: u8 = 3;

pub const BACKER_REMAINING_CUTOFF: u128 = 10;

/// The largest registry (`pool.registry_len`) for which `withdraw_simple` /
/// `settle_compounding` may run the whole allocation ticket in the payout
/// instruction (Discover -> Senior -> Seal -> Allocate -> Prune -> Commit) when
/// the caller opts in by appending the inline remaining accounts.
///
/// Measured envelope (`docs/solana/measurements-2026-10.md` section 5 and 7,
/// receipt `docs/solana/evidence/2026-10-14-m9-measurements.json`, rows
/// `inline_envelope` / `decision_output_dr31`): a real non-closing payout with N
/// Backer PDAs plus one registry page and one snapshot page appended fits the
/// legacy 1,232-byte packet up to N = 8 (1,058 B, 26 keys, 20 writable) and does
/// not at N = 16 (1,388 B).  Eight is also one registry page, so the inline path
/// is a single-page walk.  A larger registry never disables a smaller one: above
/// the threshold the chunked ticket is the only path.  No economics depend on it.
pub const INLINE_THRESHOLD: u64 = 8;

// The inline path walks exactly one registry page and one snapshot page.
const _: () = assert!(INLINE_THRESHOLD <= PAGE_CAPACITY as u64);

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace, PartialEq, Eq)]
pub struct SnapshotRow {
    pub key: Pubkey,
    pub series: u8,
    pub remaining: u128,
    pub slot: u64,
}

/// Who funded a shared account's rent, in at most two components (DR-13).
/// The actual payer of the `create_account` CPI is the `OpsCash` PDA; this
/// records the beneficial funders and where the owner share is refunded.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, InitSpace, PartialEq, Eq,
)]
pub struct RentComponents {
    pub ops: u64,
    pub owner: u64,
    pub owner_refund: Pubkey,
}

/// Rent and provider prices snapshotted into a `CloseRecord` at close time.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, InitSpace, PartialEq, Eq,
)]
pub struct ReserveSchedule {
    pub per_candidate_page_rent: u64,
    pub per_registry_page_rent: u64,
    pub b_account_rent: u64,
    pub pending_request_rent: u64,
    pub provider_fee: u64,
    pub headroom_bps: u16,
}

impl ReserveSchedule {
    /// Snapshot current runtime rent and the active config's provider prices.
    /// Account sizes come from the program's own `SPACE` constants; only the
    /// provider-owned pending request account is sized from config.
    pub fn capture(rent: &Rent, config: &ConfigVersion) -> Self {
        Self {
            per_candidate_page_rent: rent.minimum_balance(CandidatePage::SPACE),
            per_registry_page_rent: rent.minimum_balance(RegistryPage::SPACE),
            b_account_rent: rent.minimum_balance(Backer::SPACE),
            pending_request_rent: rent.minimum_balance(config.pending_request_bytes as usize),
            provider_fee: config.provider_request_fee_lamports,
            headroom_bps: config.provider_fee_headroom_bps,
        }
    }
}

/// Spent ledger by category (candidate page, registry page, B account,
/// provider fee, pending-request rent) and source (`_s` sponsor, `_o` owner).
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, InitSpace, PartialEq, Eq,
)]
pub struct SpentLedger {
    pub candidate_rent_s: u64,
    pub candidate_rent_o: u64,
    pub registry_rent_s: u64,
    pub registry_rent_o: u64,
    pub b_rent_s: u64,
    pub b_rent_o: u64,
    pub provider_fee_s: u64,
    pub provider_fee_o: u64,
    pub request_rent_s: u64,
    pub request_rent_o: u64,
}

/// Outcome of the close's Series-B promotion draw.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, InitSpace, PartialEq, Eq)]
pub enum Promotion {
    #[default]
    Pending,
    Applied { winner: Pubkey, entitlement: u128 },
    Skipped { reason: u8, stage: u8 },
}


/// `ceil(count / page_capacity)` in checked u64.
pub fn pages_for(count: u64, page_capacity: u64) -> Result<u64> {
    let whole = count
        .checked_div(page_capacity)
        .ok_or(crate::ErrorCode::InvalidState)?;
    let partial = count
        .checked_rem(page_capacity)
        .ok_or(crate::ErrorCode::InvalidState)?;
    if partial == 0 {
        Ok(whole)
    } else {
        whole
            .checked_add(1)
            .ok_or_else(|| error!(crate::ErrorCode::Overflow))
    }
}

/// Spec 8.1 (C1): the reserve a closing payout must lock, computed per close.
///
/// Candidate pages are counted per set (preferred and fallback may each hold
/// every loser) and an over-provisioned set never offsets the other. The
/// provider fee headroom is floored. All inputs and the result are u64; the
/// headroom product is formed in u128 only so a large fee cannot overflow
/// before the floored term is narrowed back.
// `u64::is_multiple_of` needs Rust 1.87; platform-tools v1.48 compiles with rustc 1.84.
#[allow(clippy::manual_is_multiple_of)]
pub fn required_close_reserve(
    participant_count: u64,
    registry_len: u64,
    existing_preferred_pages: u64,
    existing_fallback_pages: u64,
    schedule: &ReserveSchedule,
) -> Result<u64> {
    let overflow = || error!(crate::ErrorCode::Overflow);
    let required_pages_per_set = pages_for(participant_count, PAGE_CAPACITY as u64)?;
    let missing_preferred = required_pages_per_set.saturating_sub(existing_preferred_pages);
    let missing_fallback = required_pages_per_set.saturating_sub(existing_fallback_pages);
    let missing_pages = missing_preferred
        .checked_add(missing_fallback)
        .ok_or_else(overflow)?;
    let registry_extension: u64 = if registry_len % PAGE_CAPACITY as u64 == 0 {
        1
    } else {
        0
    };

    let candidate_rent = schedule
        .per_candidate_page_rent
        .checked_mul(missing_pages)
        .ok_or_else(overflow)?;
    let registry_rent = schedule
        .per_registry_page_rent
        .checked_mul(registry_extension)
        .ok_or_else(overflow)?;
    let headroom = u64::try_from(
        u128::from(schedule.provider_fee) * u128::from(schedule.headroom_bps) / 10_000u128,
    )
    .map_err(|_| overflow())?;
    let provider_cap = schedule
        .provider_fee
        .checked_add(headroom)
        .ok_or_else(overflow)?;

    candidate_rent
        .checked_add(registry_rent)
        .and_then(|total| total.checked_add(schedule.b_account_rent))
        .and_then(|total| total.checked_add(schedule.pending_request_rent))
        .and_then(|total| total.checked_add(provider_cap))
        .ok_or_else(overflow)
}

#[account]
#[derive(InitSpace, Default)]
pub struct Pool {
    pub bump: u8,
    /// Immutable genesis domain: written once by `initialize`, never again.
    pub cluster_domain: [u8; 32],
    pub round_id: u64,
    pub active_config_version: u32,
    pub pot: u64,
    pub seed: u64,
    pub management: u64,
    pub pending_toll: u64,
    pub funded_claims: u128,
    pub excess: u64,
    pub protected_rent: u64,
    pub registry_len: u64,
    pub registry_version: u64,
    pub next_sequence: u64,
    pub next_ticket: u64,
    pub active_ticket: u64,
    pub finalized_ticket: u64,
    pub snapshot_pages: u64,
    pub registry_pages: u64,
    pub participant_count: u64,
    /// Current-round participant page capacity; reset at finalize.
    pub participant_pages: u64,
    pub next_close_id: u64,
    /// `Some` exactly while a close is in progress; see [`Pool::closing`].
    pub current_close_id: Option<u64>,
    pub total_gross_deposits: u128,
    pub total_net_payouts: u128,
    pub governance: Pubkey,
    /// Genesis guardian vault. `Admission.guardian_vault` is authoritative.
    /// Genesis copy of `Admission.guardian_vault`; `rotate_guardian` (M3) writes both in one instruction.
    pub guardian_vault: Pubkey,
    pub sponsor: Pubkey,
    pub management_recipient: Pubkey,
}

impl Pool {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    /// A close is in progress while `current_close_id` is set.
    pub fn closing(&self) -> bool {
        self.current_close_id.is_some()
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct Admission {
    pub bump: u8,
    /// Authoritative guardian key; written by `initialize` and `rotate_guardian`.
    pub guardian_vault: Pubkey,
    pub paused: bool,
    pub changed_at: u64,
}

impl Admission {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct GameVault {
    pub bump: u8,
}

impl GameVault {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Ops {
    pub bump: u8,
    pub sponsor: Pubkey,
    pub free_balance: u64,
    /// O(1) aggregate of close-reserve lamports still locked; never rescanned.
    pub locked_remaining_total: u64,
    /// Anonymous OpsCash credits not yet assigned to a partition.
    pub unallocated_excess: u64,
    pub protected_rent: u64,
    // Provider-side bookkeeping: observations and counters, never spendable.
    pub provider_fee_consumed_total: u128,
    pub request_rent_outstanding: u64,
    pub eligible_return_budget: u64,
    pub provider_returned_total: u128,
    pub provider_deficit: u64,
    pub client_balance_observed: u64,
    pub client_protected_rent: u64,
    pub total_contributed: u128,
    pub total_withdrawn: u128,
    pub total_rent_funded: u128,
}

impl Ops {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct RngAuthority {
    pub bump: u8,
    /// The registered ORAO client this authority signs requests for.
    pub client: Pubkey,
}

impl RngAuthority {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

/// One immutable-once-activated rule set. Positions and tickets pin the
/// version they were created under, so a later version never rewrites them.
#[account]
#[derive(InitSpace, Default, Debug, PartialEq)]
pub struct ConfigVersion {
    pub bump: u8,
    pub version: u32,
    pub effective_round_id: u64,
    pub staged_at: u64,
    pub activated_at: Option<u64>,
    pub plan_rates_bps_per_day: [u16; 3],
    pub plan_durations_s: [u32; 3],
    /// Compound15 / Compound30 total-return factors (Q18), not per-day rates.
    pub compound_factors_q18: [u64; 2],
    pub simple_toll_bps: [u16; 3],
    pub simple_toll_day_bounds: [u16; 2],
    pub jackpot_bps: [u16; 2],
    pub front_end_load_bps: u16,
    pub min_game: u64,
    pub min_a: u64,
    pub quota_count: u8,
    pub quota_window_s: u32,
    pub seed_bps: u16,
    pub senior_bps: u16,
    pub a_bps: u16,
    pub b_bps: u16,
    pub a_entitlement_bps: u16,
    pub b_entitlement_bps: u16,
    pub open_cutoff: u64,
    pub deadline_seconds: u32,
    pub provider_request_fee_lamports: u64,
    pub provider_fee_headroom_bps: u16,
    pub pending_request_bytes: u16,
    pub fulfilled_request_bytes: u16,
    pub provider_program: Pubkey,
    pub provider_network_state: Pubkey,
    pub provider_treasury: Pubkey,
    pub provider_client: Pubkey,
    pub public_series_a_enabled: bool,
    pub page_capacity: u8,
}

impl ConfigVersion {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    /// The accepted launch rule set. Provider fee and provider pubkeys are
    /// LAUNCH values supplied by genesis arguments, so they default to zero.
    pub fn default_v1() -> Self {
        Self {
            bump: 0,
            version: 1,
            effective_round_id: 1,
            staged_at: 0,
            activated_at: None,
            plan_rates_bps_per_day: [1_100, 0, 0],
            plan_durations_s: [1_814_400, 1_296_000, 2_592_000],
            compound_factors_q18: [4_473_565_759_257_037_859, 12_267_678_469_131_246_220],
            simple_toll_bps: [1_200, 750, 300],
            simple_toll_day_bounds: [7, 14],
            jackpot_bps: [900, 1_300],
            front_end_load_bps: 400,
            min_game: 10_000_000,
            min_a: 50_000_000,
            quota_count: 3,
            quota_window_s: 3_600,
            seed_bps: 5_000,
            senior_bps: 3_500,
            a_bps: 4_500,
            b_bps: 2_000,
            a_entitlement_bps: 12_400,
            b_entitlement_bps: 11_600,
            open_cutoff: 10,
            deadline_seconds: PROMOTION_DEADLINE_SECS as u32,
            provider_request_fee_lamports: 0,
            provider_fee_headroom_bps: 2_000,
            pending_request_bytes: 759,
            fulfilled_request_bytes: 147,
            provider_program: Pubkey::default(),
            provider_network_state: Pubkey::default(),
            provider_treasury: Pubkey::default(),
            provider_client: Pubkey::default(),
            public_series_a_enabled: false,
            page_capacity: 8,
        }
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct User {
    pub bump: u8,
    pub owner: Pubkey,
    pub ever_a: bool,
    pub ever_b: bool,
    pub successful_at: [u64; QUOTA_CALLS as usize],
    pub call_count: u8,
    pub b_entitlement: u128,
}

impl User {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
    /// Record one successful admission call in the three-entry rolling window.
    /// An exactly one-hour-old entry has expired and is replaced. Calls do not
    /// record until the enclosing financial operation has passed all checks.
    pub fn record_success(&mut self, now: u64) -> Result<()> {
        let count = self.call_count as usize;
        if count > QUOTA_CALLS as usize {
            return Err(crate::ErrorCode::InvalidState.into());
        }

        if count > 0 && now < self.successful_at[count - 1] {
            return Err(crate::ErrorCode::BackwardsTime.into());
        }

        if count < QUOTA_CALLS as usize {
            self.successful_at[count] = now;
            self.call_count = self
                .call_count
                .checked_add(1)
                .ok_or(crate::ErrorCode::Overflow)?;
            return Ok(());
        }

        let oldest = self.successful_at[0];
        let elapsed = now
            .checked_sub(oldest)
            .ok_or(crate::ErrorCode::BackwardsTime)?;
        if elapsed < QUOTA_WINDOW_SECS {
            return Err(crate::ErrorCode::QuotaExceeded.into());
        }

        self.successful_at[0] = self.successful_at[1];
        self.successful_at[1] = self.successful_at[2];
        self.successful_at[2] = now;
        Ok(())
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct UserRound {
    pub bump: u8,
    pub round_id: u64,
    pub owner: Pubkey,
    pub deposits: u128,
    pub net_payout: u128,
    pub participant_slot: u64,
}

impl UserRound {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Position {
    pub bump: u8,
    pub owner: Pubkey,
    pub nonce: u64,
    pub round_id: u64,
    pub config_version: u32,
    pub kind: u8,
    pub principal: u64,
    pub opened_at: u64,
    pub maturity_at: u64,
    pub last_accrued_at: u64,
    pub remainder: u128,
    pub accrued_gross: u128,
    pub paid_gross: u128,
    pub net_payout: u128,
    pub claimed: bool,
}

impl Position {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Backer {
    pub bump: u8,
    pub owner: Pubkey,
    pub series: u8,
    pub amount: u128,
    pub entitlement: u128,
    pub lifetime: u128,
    pub funded: u64,
    pub pending: u64,
    pub pending_ticket: u64,
    pub first_sequence: u64,
    pub active: bool,
    pub slot: u64,
    pub rent_funded_by_ops: u64,
    pub rent_funded_by_owner: u64,
}

impl Backer {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn remaining(&self) -> Result<u128> {
        self.entitlement
            .checked_sub(self.lifetime)
            .ok_or(crate::ErrorCode::InvalidState.into())
    }

    /// Materialize a finalized ticket's pending credit exactly once. A pending
    /// ticket after the scalar watermark remains pending; no global liability
    /// is changed here because commit already counted it.
    pub fn materialize(&mut self, watermark: u64) -> Result<()> {
        if self.pending_ticket == 0 || self.pending_ticket > watermark {
            return Ok(());
        }

        self.funded = self
            .funded
            .checked_add(self.pending)
            .ok_or(crate::ErrorCode::Overflow)?;
        self.pending = 0;
        self.pending_ticket = 0;
        Ok(())
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct RegistryPage {
    pub bump: u8,
    pub index: u64,
    pub keys: [Pubkey; PAGE_CAPACITY],
    pub rent: RentComponents,
}

impl RegistryPage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct SnapshotPage {
    pub bump: u8,
    pub index: u64,
    pub ticket_id: u64,
    pub rows: [SnapshotRow; PAGE_CAPACITY],
    pub rent: RentComponents,
}

impl SnapshotPage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Ticket {
    pub bump: u8,
    pub id: u64,
    pub phase: u8,
    /// `Some` when this ticket allocates a closing payout's toll.
    pub close_id: Option<u64>,
    pub captured_len: u64,
    pub expected_version: u64,
    pub cursor: u64,
    pub a_count: u64,
    pub b_count: u64,
    /// Discovery accumulator: U256 sum of Series-A remainders, little-endian.
    /// A sum of u128 remainders can exceed u128, so it never narrows.
    pub a_remainder_total: [u8; 32],
    /// Fixed U256 denominator frozen at seal, serialized little-endian.
    pub sealed_a_denominator: [u8; 32],
    pub oldest: Pubkey,
    pub oldest_sequence: u64,
    pub oldest_slot: u64,
    pub senior_credit: u64,
    pub senior_slice: u64,
    pub a_slice: u64,
    pub b_slice: u64,
    pub seed_credit: u64,
    pub credited: u64,
    pub toll: u64,
    /// Backer-half split pinned from `ConfigVersion` when the ticket is created.
    pub senior_bps: u16,
    pub a_bps: u16,
    pub b_bps: u16,
}

impl Ticket {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

/// Round participant list, one page of owners. Closable once the round's
/// `CloseRecord.finalized`.
#[account]
#[derive(InitSpace, Default)]
pub struct ParticipantPage {
    pub bump: u8,
    pub round_id: u64,
    pub page: u64,
    pub len: u8,
    pub rows: [Pubkey; PAGE_CAPACITY],
    pub rent: RentComponents,
}

impl ParticipantPage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

/// One page of a close's preferred or fallback candidate set. Closable once
/// the close's promotion is terminal.
#[account]
#[derive(InitSpace, Default)]
pub struct CandidatePage {
    pub bump: u8,
    pub close_id: u64,
    pub set_tag: u8,
    pub page: u64,
    pub len: u8,
    pub rows: [Pubkey; PAGE_CAPACITY],
    pub rent: RentComponents,
}

impl CandidatePage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

/// Everything one closing payout needs: candidate-set progress, the draw,
/// the captured reserve and its by-source spend ledger.
#[account]
#[derive(InitSpace)]
pub struct CloseRecord {
    pub bump: u8,
    pub close_id: u64,
    pub round_id: u64,
    pub config_version: u32,
    pub close_started_at: u64,
    pub promotion_deadline: u64,
    pub closing_ticket_id: Option<u64>,
    pub cash_committed: bool,
    pub participant_count: u64,
    pub candidate_cursor: u64,
    pub preferred_count: u64,
    pub fallback_count: u64,
    pub preferred_digest: [u8; 32],
    pub fallback_digest: [u8; 32],
    pub selected_set_tag: u8,
    pub sealed: bool,
    pub draw_state: u8,
    pub protocol_seed: [u8; 32],
    pub request_account: Pubkey,
    pub randomness: [u8; 64],
    pub captured_at: u64,
    pub promotion: Promotion,
    pub finalized: bool,
    pub required_reserve: u64,
    pub reserve_schedule: ReserveSchedule,
    pub required_pages_per_set: u64,
    pub existing_preferred_pages: u64,
    pub existing_fallback_pages: u64,
    pub provisioned_preferred_pages: u64,
    pub provisioned_fallback_pages: u64,
    pub registry_extension_required: bool,
    pub locked_sponsor_remaining: u64,
    pub locked_owner_remaining: u64,
    pub approved_provider_fee_cap: u64,
    pub owner_refund_destination: Pubkey,
    pub spent: SpentLedger,
    /// Sponsor/owner source split of the pending-request rent; equals
    /// `spent.request_rent_s/o` at request time. There is no singular total.
    pub request_rent_funded_s: u64,
    pub request_rent_funded_o: u64,
    pub reimbursement_observed: bool,
    /// Zero until `observe_request_fulfilled`; then decremented per return.
    pub eligible_return_remaining: u64,
    /// Zero until observed; then decremented by each owner share paid.
    pub owner_request_rent_refundable_remaining: u64,
    pub request_rent_returned: u64,
    pub reserve_settled: bool,
    pub rent: RentComponents,
}

impl CloseRecord {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

// `randomness` is `[u8; 64]`, which has no `Default` impl, so this is manual.
impl Default for CloseRecord {
    fn default() -> Self {
        Self {
            bump: 0,
            close_id: 0,
            round_id: 0,
            config_version: 0,
            close_started_at: 0,
            promotion_deadline: 0,
            closing_ticket_id: None,
            cash_committed: false,
            participant_count: 0,
            candidate_cursor: 0,
            preferred_count: 0,
            fallback_count: 0,
            preferred_digest: [0; 32],
            fallback_digest: [0; 32],
            selected_set_tag: 0,
            sealed: false,
            draw_state: DRAW_NOT_REQUESTED,
            protocol_seed: [0; 32],
            request_account: Pubkey::default(),
            randomness: [0; 64],
            captured_at: 0,
            promotion: Promotion::Pending,
            finalized: false,
            required_reserve: 0,
            reserve_schedule: ReserveSchedule::default(),
            required_pages_per_set: 0,
            existing_preferred_pages: 0,
            existing_fallback_pages: 0,
            provisioned_preferred_pages: 0,
            provisioned_fallback_pages: 0,
            registry_extension_required: false,
            locked_sponsor_remaining: 0,
            locked_owner_remaining: 0,
            approved_provider_fee_cap: 0,
            owner_refund_destination: Pubkey::default(),
            spent: SpentLedger::default(),
            request_rent_funded_s: 0,
            request_rent_funded_o: 0,
            reimbursement_observed: false,
            eligible_return_remaining: 0,
            owner_request_rent_refundable_remaining: 0,
            request_rent_returned: 0,
            reserve_settled: false,
            rent: RentComponents::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use musical_chairs_economics::{earnings, fees};

    fn serialized_len<T: AnchorSerialize>(value: &T) -> usize {
        value.try_to_vec().expect("fixed state serializes").len()
    }

    /// Length of the Anchor account encoding: 8-byte discriminator + Borsh body.
    fn account_len<T: AccountSerialize>(value: &T) -> usize {
        let mut buffer = Vec::new();
        value
            .try_serialize(&mut buffer)
            .expect("account serializes");
        buffer.len()
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    // `Option` encodes as one byte when `None` but `InitSpace` reserves the
    // `Some` payload, so size checks encode the maximal populated form.
    fn full_pool() -> Pool {
        Pool {
            current_close_id: Some(u64::MAX),
            ..Pool::default()
        }
    }

    fn full_ticket() -> Ticket {
        Ticket {
            close_id: Some(u64::MAX),
            ..Ticket::default()
        }
    }

    fn full_config() -> ConfigVersion {
        ConfigVersion {
            activated_at: Some(u64::MAX),
            ..ConfigVersion::default_v1()
        }
    }

    fn full_close() -> CloseRecord {
        CloseRecord {
            closing_ticket_id: Some(u64::MAX),
            promotion: Promotion::Applied {
                winner: key(9),
                entitlement: u128::MAX,
            },
            ..CloseRecord::default()
        }
    }

    macro_rules! assert_space {
        ($ty:ty, $full:expr) => {{
            let full: $ty = $full;
            let name = stringify!($ty);
            assert_eq!(
                <$ty>::SPACE,
                8 + serialized_len(&full),
                "{name}: SPACE vs Borsh payload"
            );
            assert_eq!(
                <$ty>::SPACE,
                account_len(&full),
                "{name}: SPACE vs account encoding"
            );
            assert!(
                account_len(&<$ty>::default()) <= <$ty>::SPACE,
                "{name}: default encoding must fit SPACE"
            );
        }};
    }

    #[test]
    fn fixed_account_layout_sizes_match_serialized_payloads() {
        assert_space!(Pool, full_pool());
        assert_space!(Admission, Admission::default());
        assert_space!(GameVault, GameVault::default());
        assert_space!(Ops, Ops::default());
        assert_space!(RngAuthority, RngAuthority::default());
        assert_space!(ConfigVersion, full_config());
        assert_space!(User, User::default());
        assert_space!(UserRound, UserRound::default());
        assert_space!(Position, Position::default());
        assert_space!(Backer, Backer::default());
        assert_space!(RegistryPage, RegistryPage::default());
        assert_space!(SnapshotPage, SnapshotPage::default());
        assert_space!(Ticket, full_ticket());
        assert_space!(ParticipantPage, ParticipantPage::default());
        assert_space!(CandidatePage, CandidatePage::default());
        assert_space!(CloseRecord, full_close());
    }

    #[test]
    fn helper_struct_widths_cover_their_largest_encodings() {
        assert_eq!(
            RentComponents::INIT_SPACE,
            serialized_len(&RentComponents::default())
        );
        assert_eq!(RentComponents::INIT_SPACE, 8 + 8 + 32);
        assert_eq!(
            SpentLedger::INIT_SPACE,
            serialized_len(&SpentLedger::default())
        );
        assert_eq!(SpentLedger::INIT_SPACE, 10 * 8);
        assert_eq!(
            ReserveSchedule::INIT_SPACE,
            serialized_len(&ReserveSchedule::default())
        );
        assert_eq!(ReserveSchedule::INIT_SPACE, 5 * 8 + 2);
    }

    /// Hand-computed byte totals (discriminator included). These are the
    /// first real inputs to the DR-27 rent model; any change is a schema
    /// change and must update this table deliberately.
    #[test]
    fn frozen_account_space_table_is_pinned() {
        assert_eq!(Pool::SPACE, 374);
        assert_eq!(Admission::SPACE, 50);
        assert_eq!(GameVault::SPACE, 9);
        assert_eq!(Ops::SPACE, 193);
        assert_eq!(RngAuthority::SPACE, 41);
        assert_eq!(ConfigVersion::SPACE, 277);
        assert_eq!(User::SPACE, 84);
        assert_eq!(UserRound::SPACE, 89);
        assert_eq!(Position::SPACE, 159);
        assert_eq!(Backer::SPACE, 147);
        assert_eq!(RegistryPage::SPACE, 321);
        assert_eq!(SnapshotPage::SPACE, 529);
        assert_eq!(Ticket::SPACE, 241);
        assert_eq!(ParticipantPage::SPACE, 330);
        assert_eq!(CandidatePage::SPACE, 331);
        assert_eq!(CloseRecord::SPACE, 657);
    }

    #[test]
    fn pages_have_the_frozen_capacity() {
        assert_eq!(SnapshotPage::default().rows.len(), PAGE_CAPACITY);
        assert_eq!(RegistryPage::default().keys.len(), PAGE_CAPACITY);
        assert_eq!(ParticipantPage::default().rows.len(), PAGE_CAPACITY);
        assert_eq!(CandidatePage::default().rows.len(), PAGE_CAPACITY);
    }

    #[test]
    fn new_seeds_are_the_frozen_byte_strings() {
        assert_eq!(RNG_AUTHORITY_SEED, b"rng-authority");
        assert_eq!(CONFIG_SEED, b"config");
        assert_eq!(CLOSE_SEED, b"close");
        assert_eq!(PARTICIPANTS_SEED, b"participants");
        assert_eq!(CANDIDATES_SEED, b"candidates");
        let all: [&[u8]; 17] = [
            POOL_SEED,
            ADMISSION_SEED,
            GAME_VAULT_SEED,
            OPS_SEED,
            OPS_CASH_SEED,
            TICKET_SEED,
            USER_SEED,
            USER_ROUND_SEED,
            POSITION_SEED,
            BACKER_SEED,
            REGISTRY_SEED,
            SNAPSHOT_SEED,
            RNG_AUTHORITY_SEED,
            CONFIG_SEED,
            CLOSE_SEED,
            PARTICIPANTS_SEED,
            CANDIDATES_SEED,
        ];
        for (index, seed) in all.iter().enumerate() {
            assert!(
                all[index + 1..].iter().all(|other| other != seed),
                "duplicate seed {seed:?}"
            );
        }
    }

    #[test]
    fn draw_state_constants_are_distinct_and_ordered() {
        assert_eq!(DRAW_NOT_REQUESTED, 0);
        assert_eq!(DRAW_REQUESTED, 1);
        assert_eq!(DRAW_CAPTURED, 2);
        assert_eq!(DRAW_IGNORED_LATE, 3);
    }

    #[test]
    fn config_default_v1_encodes_global_constraints() {
        let config = ConfigVersion::default_v1();
        assert_eq!(config.version, 1);
        assert_eq!(config.effective_round_id, 1);
        assert_eq!(config.staged_at, 0);
        assert_eq!(config.activated_at, None);
        assert_eq!(config.plan_rates_bps_per_day, [1_100, 0, 0]);
        assert_eq!(config.plan_durations_s, [1_814_400, 1_296_000, 2_592_000]);
        assert_eq!(
            config.compound_factors_q18,
            [4_473_565_759_257_037_859, 12_267_678_469_131_246_220]
        );
        assert_eq!(config.simple_toll_bps, [1_200, 750, 300]);
        assert_eq!(config.simple_toll_day_bounds, [7, 14]);
        assert_eq!(config.jackpot_bps, [900, 1_300]);
        assert_eq!(config.front_end_load_bps, 400);
        assert_eq!(config.min_game, 10_000_000);
        assert_eq!(config.min_a, 50_000_000);
        assert_eq!(config.quota_count, 3);
        assert_eq!(config.quota_window_s, 3_600);
        assert_eq!(config.seed_bps, 5_000);
        assert_eq!(config.senior_bps, 3_500);
        assert_eq!(config.a_bps, 4_500);
        assert_eq!(config.b_bps, 2_000);
        assert_eq!(config.a_entitlement_bps, 12_400);
        assert_eq!(config.b_entitlement_bps, 11_600);
        assert_eq!(config.open_cutoff, 10);
        assert_eq!(config.deadline_seconds, 600);
        assert_eq!(config.provider_request_fee_lamports, 0);
        assert_eq!(config.provider_fee_headroom_bps, 2_000);
        assert_eq!(config.pending_request_bytes, 759);
        assert_eq!(config.fulfilled_request_bytes, 147);
        assert_eq!(config.provider_program, Pubkey::default());
        assert_eq!(config.provider_network_state, Pubkey::default());
        assert_eq!(config.provider_treasury, Pubkey::default());
        assert_eq!(config.provider_client, Pubkey::default());
        assert!(!config.public_series_a_enabled);
        assert_eq!(config.page_capacity, 8);

        // The Backer half splits 35/45/20 and the three shares are the whole half.
        assert_eq!(config.senior_bps + config.a_bps + config.b_bps, 10_000);
        // `default_v1` is a real value, not the all-zero `Default`.
        assert_ne!(config, ConfigVersion::default());
    }

    #[test]
    fn config_default_v1_matches_host_economics_constants() {
        let config = ConfigVersion::default_v1();
        assert_eq!(config.front_end_load_bps, fees::FRONT_END_LOAD_BPS);
        assert_eq!(config.min_game, fees::GAME_MINIMUM_LAMPORTS);
        assert_eq!(config.min_a, fees::SERIES_A_MINIMUM_LAMPORTS);
        assert_eq!(
            u64::from(config.plan_durations_s[0]),
            earnings::SIMPLE_EXPIRY_SECONDS
        );
        assert_eq!(
            u64::from(config.plan_durations_s[1]),
            15 * earnings::SECONDS_PER_DAY
        );
        assert_eq!(
            u64::from(config.plan_durations_s[2]),
            30 * earnings::SECONDS_PER_DAY
        );
        assert_eq!(
            u128::from(config.compound_factors_q18[0]),
            earnings::ROI_15_Q18
        );
        assert_eq!(
            u128::from(config.compound_factors_q18[1]),
            earnings::ROI_30_Q18
        );
        // 11 / 8_640_000 per second is 11% per day, i.e. 1_100 bps per day.
        assert_eq!(
            u128::from(config.plan_rates_bps_per_day[0]) * earnings::SIMPLE_DENOMINATOR,
            earnings::SIMPLE_RATE_NUMERATOR * 10_000 * u128::from(earnings::SECONDS_PER_DAY)
        );
        let [early, mid, late] = config.simple_toll_bps;
        let [first_bound, second_bound] = config.simple_toll_day_bounds;
        let day = earnings::SECONDS_PER_DAY;
        assert_eq!(
            fees::simple_toll_bps(u64::from(first_bound) * day - 1),
            early
        );
        assert_eq!(fees::simple_toll_bps(u64::from(first_bound) * day), mid);
        assert_eq!(
            fees::simple_toll_bps(u64::from(second_bound) * day - 1),
            mid
        );
        assert_eq!(fees::simple_toll_bps(u64::from(second_bound) * day), late);
        assert_eq!(
            fees::compound_jackpot_fee_bps(15).unwrap(),
            config.jackpot_bps[0]
        );
        assert_eq!(
            fees::compound_jackpot_fee_bps(30).unwrap(),
            config.jackpot_bps[1]
        );
        assert_eq!(config.quota_count, QUOTA_CALLS);
        assert_eq!(u64::from(config.quota_window_s), QUOTA_WINDOW_SECS);
        assert_eq!(config.page_capacity as usize, PAGE_CAPACITY);
        assert_eq!(u128::from(config.open_cutoff), BACKER_REMAINING_CUTOFF);
        assert_eq!(
            config.open_cutoff,
            fees::BACKER_OPEN_REMAINING_CUTOFF_LAMPORTS
        );
    }

    #[test]
    fn config_account_round_trips_through_the_account_codec() {
        let config = ConfigVersion {
            bump: 254,
            version: 7,
            activated_at: Some(1_700_000_000),
            provider_program: key(1),
            provider_network_state: key(2),
            provider_treasury: key(3),
            provider_client: key(4),
            provider_request_fee_lamports: 1_234_567,
            public_series_a_enabled: true,
            ..ConfigVersion::default_v1()
        };
        let mut buffer = Vec::new();
        config.try_serialize(&mut buffer).unwrap();
        let decoded = ConfigVersion::try_deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn cluster_domain_round_trips_through_the_account_codec() {
        let domain: [u8; 32] = std::array::from_fn(|index| index as u8 ^ 0xA5);
        let pool = Pool {
            cluster_domain: domain,
            bump: 253,
            ..Pool::default()
        };
        let mut buffer = Vec::new();
        pool.try_serialize(&mut buffer).unwrap();
        // bump is the first body byte; cluster_domain follows it immediately.
        assert_eq!(&buffer[8 + 1..8 + 1 + 32], &domain);
        let decoded = Pool::try_deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(decoded.cluster_domain, domain);
        assert_eq!(decoded.bump, 253);
        assert_eq!(Pool::default().cluster_domain, [0u8; 32]);
    }

    #[test]
    fn pool_closing_is_derived_from_current_close_id() {
        let mut pool = Pool::default();
        assert!(!pool.closing());
        pool.current_close_id = Some(0);
        assert!(pool.closing());
        pool.current_close_id = None;
        assert!(!pool.closing());
    }

    #[test]
    fn promotion_defaults_to_pending_and_space_covers_the_largest_variant() {
        assert_eq!(Promotion::default(), Promotion::Pending);
        assert_eq!(CloseRecord::default().promotion, Promotion::Pending);
        let applied = Promotion::Applied {
            winner: key(7),
            entitlement: u128::MAX,
        };
        let skipped = Promotion::Skipped {
            reason: u8::MAX,
            stage: u8::MAX,
        };
        let largest = [Promotion::Pending, applied, skipped]
            .iter()
            .map(serialized_len)
            .max()
            .unwrap();
        assert_eq!(largest, 1 + 32 + 16);
        assert_eq!(Promotion::INIT_SPACE, largest);
        // Pending stays strictly smaller, so InitSpace is not just the default size.
        assert!(serialized_len(&Promotion::Pending) < Promotion::INIT_SPACE);
    }

    #[test]
    fn reserve_schedule_capture_reads_rent_and_config() {
        let rent = Rent::default();
        let config = ConfigVersion {
            provider_request_fee_lamports: 5_000_000,
            provider_fee_headroom_bps: 1_500,
            pending_request_bytes: 759,
            ..ConfigVersion::default_v1()
        };
        let schedule = ReserveSchedule::capture(&rent, &config);
        assert_eq!(
            schedule.per_candidate_page_rent,
            rent.minimum_balance(CandidatePage::SPACE)
        );
        assert_eq!(
            schedule.per_registry_page_rent,
            rent.minimum_balance(RegistryPage::SPACE)
        );
        assert_eq!(schedule.b_account_rent, rent.minimum_balance(Backer::SPACE));
        assert_eq!(schedule.pending_request_rent, rent.minimum_balance(759));
        assert_eq!(schedule.provider_fee, 5_000_000);
        assert_eq!(schedule.headroom_bps, 1_500);
        assert!(schedule.per_candidate_page_rent > 0 && schedule.pending_request_rent > 0);
    }

    fn schedule() -> ReserveSchedule {
        // Distinct magnitudes so every term is individually identifiable.
        ReserveSchedule {
            per_candidate_page_rent: 3,
            per_registry_page_rent: 5_000,
            b_account_rent: 70_000,
            pending_request_rent: 1_100_000,
            provider_fee: 9_000_000,
            headroom_bps: 2_000,
        }
    }

    /// b + pending + fee + floor(fee * 2_000 / 10_000) for `schedule()`.
    const FIXED_TERMS: u64 = 70_000 + 1_100_000 + 9_000_000 + 1_800_000;

    #[test]
    fn pages_for_is_a_checked_ceiling_division() {
        assert_eq!(pages_for(0, 8).unwrap(), 0);
        assert_eq!(pages_for(1, 8).unwrap(), 1);
        assert_eq!(pages_for(8, 8).unwrap(), 1);
        assert_eq!(pages_for(9, 8).unwrap(), 2);
        assert_eq!(pages_for(16, 8).unwrap(), 2);
        assert_eq!(pages_for(17, 8).unwrap(), 3);
        assert_eq!(pages_for(262_144, 8).unwrap(), 32_768);
        assert_eq!(pages_for(262_145, 8).unwrap(), 32_769);
        assert_eq!(pages_for(u64::MAX, 8).unwrap(), (u64::MAX / 8) + 1);
        assert_eq!(pages_for(u64::MAX, 1).unwrap(), u64::MAX);
        assert_eq!(
            pages_for(5, 0).unwrap_err(),
            crate::ErrorCode::InvalidState.into()
        );
    }

    #[test]
    fn close_reserve_counts_one_page_per_set_for_eight_participants() {
        // 8 participants: 1 page per set, 2 pages total; registry_len 1 -> no extension.
        assert_eq!(
            required_close_reserve(8, 1, 0, 0, &schedule()).unwrap(),
            3 * 2 + FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_counts_two_pages_per_set_for_nine_participants() {
        // 9 participants: 2 pages per set, 4 pages total.
        assert_eq!(
            required_close_reserve(9, 1, 0, 0, &schedule()).unwrap(),
            3 * 4 + FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_with_no_participants_reserves_only_the_fixed_terms() {
        assert_eq!(
            required_close_reserve(0, 1, 0, 0, &schedule()).unwrap(),
            FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_registry_extension_follows_page_boundary() {
        let base = 3 * 2 + FIXED_TERMS;
        for (registry_len, extension) in [(0u64, 1u64), (7, 0), (8, 1), (9, 0), (15, 0), (16, 1)] {
            assert_eq!(
                required_close_reserve(8, registry_len, 0, 0, &schedule()).unwrap(),
                base + 5_000 * extension,
                "registry_len {registry_len}"
            );
        }
    }

    #[test]
    // The arithmetic spells out (missing preferred + missing fallback) on purpose.
    #[allow(clippy::identity_op)]
    fn close_reserve_never_lets_one_over_provisioned_set_offset_the_other() {
        // required = 2 pages per set. Preferred holds 5 (over by 3); fallback holds none.
        let reserve = required_close_reserve(9, 1, 5, 0, &schedule()).unwrap();
        assert_eq!(reserve, 3 * (0 + 2) + FIXED_TERMS);
        // A pooled (wrong) calculation would have netted the surplus: 4 - 3 = 1 page.
        assert_ne!(reserve, 3 * 1 + FIXED_TERMS);
        // Symmetric case.
        assert_eq!(
            required_close_reserve(9, 1, 0, 5, &schedule()).unwrap(),
            reserve
        );
        // Partially provisioned on both sides: missing 0 and 1.
        assert_eq!(
            required_close_reserve(9, 1, 2, 1, &schedule()).unwrap(),
            3 * (0 + 1) + FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_over_provisioning_never_underflows() {
        assert_eq!(
            required_close_reserve(9, 1, 100, 100, &schedule()).unwrap(),
            FIXED_TERMS
        );
        assert_eq!(
            required_close_reserve(9, 1, u64::MAX, u64::MAX, &schedule()).unwrap(),
            FIXED_TERMS
        );
        assert_eq!(
            required_close_reserve(0, 1, 1, 1, &schedule()).unwrap(),
            FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_scales_to_262_145_participants_in_u64() {
        // Arithmetic-only fixture: 32_769 pages per set, both sets absent.
        assert_eq!(pages_for(262_145, 8).unwrap(), 32_769);
        assert_eq!(
            required_close_reserve(262_145, 1, 0, 0, &schedule()).unwrap(),
            3 * 2 * 32_769 + FIXED_TERMS
        );
        // Preferred already holds every page; only fallback is missing.
        assert_eq!(
            required_close_reserve(262_145, 1, 32_769, 0, &schedule()).unwrap(),
            3 * 32_769 + FIXED_TERMS
        );
    }

    #[test]
    fn close_reserve_one_lamport_exactness() {
        let ones = ReserveSchedule {
            per_candidate_page_rent: 1,
            per_registry_page_rent: 1,
            b_account_rent: 1,
            pending_request_rent: 1,
            provider_fee: 1,
            headroom_bps: 10_000,
        };
        // 9 participants -> 4 candidate pages; registry_len 8 -> 1 extension page;
        // plus b, pending, fee and a full-headroom fee of 1: 4 + 1 + 1 + 1 + 1 + 1.
        assert_eq!(required_close_reserve(9, 8, 0, 0, &ones).unwrap(), 9);

        // Headroom is floored, never rounded up: floor(3 * 3_333 / 10_000) = 0.
        let fee_only = |headroom_bps| ReserveSchedule {
            provider_fee: 3,
            headroom_bps,
            ..ReserveSchedule::default()
        };
        assert_eq!(
            required_close_reserve(0, 1, 0, 0, &fee_only(3_333)).unwrap(),
            3
        );
        assert_eq!(
            required_close_reserve(0, 1, 0, 0, &fee_only(3_334)).unwrap(),
            4
        );
        assert_eq!(required_close_reserve(0, 1, 0, 0, &fee_only(0)).unwrap(), 3);
    }

    #[test]
    fn close_reserve_headroom_product_does_not_overflow_prematurely() {
        // fee * headroom_bps exceeds u64 but the floored headroom term fits.
        let fee = 1u64 << 60;
        let schedule = ReserveSchedule {
            provider_fee: fee,
            headroom_bps: 100,
            ..ReserveSchedule::default()
        };
        let expected_headroom = (u128::from(fee) * 100 / 10_000) as u64;
        assert_eq!(
            required_close_reserve(0, 1, 0, 0, &schedule).unwrap(),
            fee + expected_headroom
        );
    }

    #[test]
    fn close_reserve_reports_overflow_instead_of_wrapping() {
        let overflow: anchor_lang::error::Error = crate::ErrorCode::Overflow.into();
        // Page count itself is fine (2^61 per set, 2^62 missing in total) but
        // the rent product is not: 5 * 2^62 > 2^64.
        let expensive = ReserveSchedule {
            per_candidate_page_rent: 5,
            ..schedule()
        };
        assert_eq!(pages_for(u64::MAX, 8).unwrap(), 1 << 61);
        assert_eq!(
            required_close_reserve(u64::MAX, 1, 0, 0, &expensive).unwrap_err(),
            overflow
        );
        // The same population is representable when the product still fits.
        assert_eq!(
            required_close_reserve(u64::MAX, 1, 0, 0, &schedule()).unwrap(),
            3 * (1u64 << 62) + FIXED_TERMS
        );
        // Fee plus its full headroom exceeds u64.
        let doubled = ReserveSchedule {
            provider_fee: u64::MAX,
            headroom_bps: 10_000,
            ..ReserveSchedule::default()
        };
        assert_eq!(
            required_close_reserve(0, 1, 0, 0, &doubled).unwrap_err(),
            overflow
        );
        // The final addition chain is checked too: u64::MAX of registry rent plus a B account.
        let full = ReserveSchedule {
            per_registry_page_rent: u64::MAX,
            b_account_rent: 1,
            ..ReserveSchedule::default()
        };
        assert_eq!(
            required_close_reserve(0, 8, 0, 0, &full).unwrap_err(),
            overflow
        );
    }

    #[test]
    fn quota_accepts_exact_hour_boundary() {
        let mut user = User::default();
        user.record_success(10).unwrap();
        user.record_success(20).unwrap();
        user.record_success(30).unwrap();
        user.record_success(10 + QUOTA_WINDOW_SECS).unwrap();
        assert_eq!(user.call_count, QUOTA_CALLS);
        assert_eq!(user.successful_at, [20, 30, 10 + QUOTA_WINDOW_SECS]);
    }

    #[test]
    fn quota_rejects_before_exact_hour_boundary() {
        let mut user = User::default();
        user.record_success(10).unwrap();
        user.record_success(20).unwrap();
        user.record_success(30).unwrap();
        assert!(matches!(
            user.record_success(10 + QUOTA_WINDOW_SECS - 1),
            Err(error) if error == crate::ErrorCode::QuotaExceeded.into()
        ));
    }

    #[test]
    fn backer_remaining_and_finalized_materialization_are_separate() {
        let mut backer = Backer {
            entitlement: 100,
            lifetime: 40,
            funded: 7,
            pending: 11,
            pending_ticket: 4,
            ..Backer::default()
        };
        assert_eq!(backer.remaining().unwrap(), 60);
        backer.materialize(3).unwrap();
        assert_eq!(backer.funded, 7);
        assert_eq!(backer.pending, 11);
        backer.materialize(4).unwrap();
        assert_eq!(backer.funded, 18);
        assert_eq!(backer.pending, 0);
        assert_eq!(backer.pending_ticket, 0);
        assert_eq!(backer.remaining().unwrap(), 60);
    }
}
