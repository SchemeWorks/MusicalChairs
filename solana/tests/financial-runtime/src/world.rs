//! `World`: the M5 test world for the admission and payout suite.
//!
//! A `World` wraps [`FinancialRuntime::new_governed`] (the post-`initialize`
//! genesis shape, bound to the Squads fixtures) plus the one fixture the first
//! `open_position` of a round needs, Participant page 0 of round 1, and turns
//! the instruction ABI into plan-shaped helpers: `open_position`,
//! `fund_series_a`, `withdraw_simple`, `settle_compounding`,
//! `close_simple_position`, typed state readers, fixture writers that keep both
//! cash registers consistent, event decoding from the transaction's inner
//! instructions, and whole-world snapshots for the fee-only rollback proofs.
//!
//! Everything a transaction method executes runs the actual compiled SBF.
//! Fixture helpers (`set_pot_fixture`, `open_fixture_simple`,
//! `provision_close_record`, `install_open_backer`, ...) write accounts
//! directly, always together with the lamports that keep
//! [`FinancialRuntime::conservation_check`] true; they are test scaffolding,
//! not claims about how a deployed program reaches that state.
//!
//! Setup transactions run by `genesis_with_position` are asserted successful
//! but are not recorded as observations: only the transactions a row issues
//! through the labelled methods are logged (see [`World::take_log`]).  Every
//! recorded transaction is followed by both conservation checks, so a row can
//! never forget one.

use std::collections::{BTreeMap, BTreeSet};

use musical_chairs_program::{
    accounts,
    allocation::{SERIES_A, SERIES_B},
    instruction,
    positions::PlanKindArg,
    required_close_reserve, state, BackerClaimed, ErrorCode, ID,
};
use sha2::{Digest, Sha256};
use solana_sdk::{
    account::Account,
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::controls::custom_code;
use crate::runtime::{
    anchor_ix, pda, FinancialRuntime, Observation, SignedTransaction, ATTACKER_SEED, OWNER_SEED,
    SPONSOR_SEED,
};

pub const CU: u32 = 1_400_000;
pub const SOL: u64 = 1_000_000_000;
pub const DAY: u64 = 86_400;
/// The World's clock at genesis (arbitrary, deliberately not zero).
pub const START_TIME: i64 = 1_800_000_000;
const PAGE: u64 = state::PAGE_CAPACITY as u64;
/// Entitlement a fixture Backer holds: `floor(50_000_000 * 124 / 100)`.
const FIXTURE_BACKER_ENTITLEMENT: u128 = 62_000_000;

/// `Ok` carries the successful observation, `Err` the failed one.
pub type Outcome = Result<Observation, Observation>;

/// Anchor numbers `#[error_code]` variants from 6000 in declaration order.
pub fn code(error: ErrorCode) -> u32 {
    error as u32 + 6000
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayoutKind {
    WithdrawSimple,
    SettleCompounding,
}

/// Which `close_record` the payout transaction names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseRecordArg {
    /// `CloseRecord(pool.next_close_id)` when it is provisioned, otherwise the
    /// program-id placeholder that encodes `None`.
    Auto,
    /// Always the placeholder (no record named).
    Absent,
    /// This account, whatever it is.
    At(Pubkey),
}

/// One account's digest inside a [`Snapshot`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct AccountDigest {
    lamports: u64,
    owner: Pubkey,
    executable: bool,
    data_sha256: [u8; 32],
}

/// Lamports and data hash of every account the World can touch, keyed by
/// address; an absent account is `None`.  See [`World::snapshot_all`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    payer: Pubkey,
    accounts: BTreeMap<Pubkey, Option<AccountDigest>>,
}

impl Snapshot {
    /// This snapshot with `fee` lamports added back to the fee payer: the
    /// "after" snapshot of a rejected transaction, fee excluded, so it can be
    /// compared with the "before" one.  Payout transactions carry two
    /// signatures (harness payer + owner), so their fee is 10,000, not 5,000;
    /// [`World::assert_fee_only`] passes the observed fee.
    pub fn except_payer_fee(mut self, fee: u64) -> Self {
        let payer = self.payer;
        if let Some(Some(digest)) = self.accounts.get_mut(&payer) {
            digest.lamports += fee;
        }
        self
    }

    pub fn lamports(&self, key: Pubkey) -> u64 {
        self.accounts
            .get(&key)
            .and_then(|digest| digest.as_ref())
            .map_or(0, |digest| digest.lamports)
    }

    /// Addresses (base58) whose digest differs from `other`'s.
    pub fn differences(&self, other: &Snapshot) -> Vec<String> {
        self.accounts
            .keys()
            .chain(other.accounts.keys())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|key| self.accounts.get(key) != other.accounts.get(key))
            .map(|key| key.to_string())
            .collect()
    }
}

pub struct World {
    pub rt: FinancialRuntime,
    /// Default payout destination: a fresh, unfunded, system-owned address.
    pub dest: Pubkey,
    scope: String,
    log: Vec<Observation>,
    last: Option<Observation>,
    now: i64,
    slot: u64,
    watch: BTreeSet<Pubkey>,
    /// Per-phase transaction counters for the allocation driver's labels.
    drive_counts: BTreeMap<&'static str, u32>,
    /// Every fixture Backer PDA installed by [`World::install_backer_specs`].
    fixture_backers: BTreeSet<Pubkey>,
}

impl World {
    // -----------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------

    /// Genesis: the governed fixture (`Pool.governance` = the G vault, round
    /// 1, config v1 active, next close id 1), Participant page 0 of round 1
    /// provisioned, the owner (and every other actor) funded by the harness,
    /// and `public_series_a_enabled` switched on in the ConfigVersion fixture
    /// (`ConfigVersion::default_v1()` has it off; the rows that test the flag
    /// turn it off again with [`World::set_public_series_a`]).
    pub fn genesis() -> Self {
        Self::genesis_on(FinancialRuntime::new_governed())
    }

    /// [`World::genesis`] on a runtime that also has the captured ORAO
    /// executable loaded (the close suite).  Call [`World::provider_setup`]
    /// (close.rs) to initialise, configure and register the provider.
    pub fn genesis_with_provider() -> Self {
        Self::genesis_on(FinancialRuntime::new_governed_with_provider())
    }

    fn genesis_on(mut rt: FinancialRuntime) -> Self {
        rt.install_participant_page(1, 0);
        let mut config: state::ConfigVersion = rt.deserialize(rt.config_address(1));
        config.public_series_a_enabled = true;
        rt.replace_config(config);
        rt.set_clock(START_TIME, 2);
        Self {
            rt,
            dest: Pubkey::new_from_array([0xDE; 32]),
            scope: "financial.positions".into(),
            log: Vec::new(),
            last: None,
            now: START_TIME,
            slot: 2,
            watch: BTreeSet::new(),
            drive_counts: BTreeMap::new(),
            fixture_backers: BTreeSet::new(),
        }
    }

    /// Genesis plus one REAL `open_position` (owner, nonce 1) of `plan`.
    pub fn genesis_with_position(plan: PlanKindArg, gross: u64) -> Self {
        let mut world = Self::genesis();
        let owner = world.owner();
        let pool = world.pool();
        let open = world.open_instruction(
            owner.pubkey(),
            plan,
            gross,
            1,
            pool.round_id,
            pool.active_config_version,
        );
        world.setup("open", vec![open], &[&owner]);
        world
    }

    /// Genesis plus one real Simple `open_position` (owner, nonce 1).
    pub fn genesis_with_simple_position(gross: u64) -> Self {
        Self::genesis_with_position(PlanKindArg::Simple, gross)
    }

    /// [`World::genesis_with_simple_position`] plus one open Series A Backer
    /// (the `attacker` actor) installed as a fixture: an active Backer row at
    /// registry slot 0 holding `62_000_000` entitlement.  No pot is credited
    /// for it, so the pot stays exactly the position's `96%` of `gross`.
    pub fn genesis_with_simple_position_and_one_open_backer(gross: u64) -> Self {
        let mut world = Self::genesis_with_simple_position(gross);
        let backer = world.attacker().pubkey();
        world.install_open_backer(backer);
        world
    }

    /// A world about to close its round on the owner's next payout.
    ///
    /// * the owner holds a fixture Simple position (nonce 1, 1 SOL) and is
    ///   participant 1; `participants - 1` synthetic participants follow, all
    ///   on provisioned Participant pages;
    /// * `registry_len` fixture open Backers (synthetic owners) fill the
    ///   registry, with the snapshot pages a ticket would need;
    /// * `CloseRecord(1)` is provisioned, Operations holds no free cash;
    /// * the pot is `100_000_000`, one day later the position owes
    ///   `110_000_000`, so the next payout is insolvent;
    /// * the clock is one day after the position's open.
    pub fn closing_fixture(participants: u64, registry_len: u64) -> Self {
        assert!(participants >= 1, "the owner is always a participant");
        let mut world = Self::genesis();
        world.install_open_backers(registry_len);
        let owner = world.owner().pubkey();
        world.open_fixture_simple(owner, SOL);
        world.append_synthetic_participants(participants - 1);
        world.set_pot_fixture(100_000_000);
        world.provision_close_record(1);
        world.warp_seconds(DAY);
        world
    }

    /// Name the rows' labels: `financial.positions.<row>` plus `.<tag>` when
    /// `tag` is non-empty.  Every recorded transaction is labelled
    /// `<scope>` (variant `""`) or `<scope>.<variant>`.
    pub fn scoped(mut self, row: &str, tag: &str) -> Self {
        self.scope = if tag.is_empty() {
            format!("financial.positions.{row}")
        } else {
            format!("financial.positions.{row}.{tag}")
        };
        self
    }

    /// Name the labels' root outright (the allocation suite uses
    /// `financial.allocation.m6.<row>`); every recorded transaction is then
    /// `<scope>` (variant `""`) or `<scope>.<variant>`.
    pub fn scoped_label(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    pub fn label(&self, variant: &str) -> String {
        if variant.is_empty() {
            self.scope.clone()
        } else {
            format!("{}.{variant}", self.scope)
        }
    }

    /// The observations recorded so far, in execution order.
    pub fn take_log(&mut self) -> Vec<Observation> {
        std::mem::take(&mut self.log)
    }

    /// The observations recorded so far, without draining them (the M8 suites
    /// read the closing payout's lamport deltas from the log).
    pub fn logged(&self) -> &[Observation] {
        &self.log
    }

    /// Make the owner the fee payer too (one signature, 5,000 lamports), so
    /// the owner's balance carries the fee as well as every other movement.
    pub fn pay_fees_as_owner(&mut self) {
        self.rt.payer = Keypair::new_from_array(OWNER_SEED);
    }

    // -----------------------------------------------------------------
    // Actors, time, plain reads
    // -----------------------------------------------------------------

    pub fn owner(&self) -> Keypair {
        Keypair::new_from_array(OWNER_SEED)
    }
    pub fn attacker(&self) -> Keypair {
        Keypair::new_from_array(ATTACKER_SEED)
    }
    pub fn sponsor(&self) -> Keypair {
        Keypair::new_from_array(SPONSOR_SEED)
    }
    /// Owner of the `index`-th synthetic fixture Backer.
    pub fn synthetic_backer(index: u64) -> Pubkey {
        Pubkey::new_from_array([0xB0u8.wrapping_add(index as u8); 32])
    }
    /// The `index`-th synthetic fixture participant (never the owner).
    pub fn synthetic_participant(index: u64) -> Pubkey {
        Pubkey::new_from_array([0xC0u8.wrapping_add(index as u8); 32])
    }

    pub fn now(&self) -> u64 {
        self.now as u64
    }

    /// Advance the clock by `seconds` (one slot per call).
    pub fn warp_seconds(&mut self, seconds: u64) {
        self.now += seconds as i64;
        self.slot += 1;
        self.rt.set_clock(self.now, self.slot);
    }

    /// Move the clock to exactly `unix` (never backwards; one slot per call).
    pub fn warp_to(&mut self, unix: u64) {
        let target = i64::try_from(unix).expect("unix time fits i64");
        assert!(target >= self.now, "the World's clock never runs backwards");
        self.warp_seconds((target - self.now) as u64);
    }

    /// Set the Clock sysvar to a time BEFORE the World's own clock, without
    /// moving the World's clock (a backdated-clock probe).  Call
    /// [`World::restore_clock`] before the next ordinary transaction.
    pub fn backdate_clock(&mut self, unix: u64) {
        self.slot += 1;
        self.rt
            .set_clock(i64::try_from(unix).expect("unix time fits i64"), self.slot);
    }

    /// Put the Clock sysvar back to the World's own time.
    pub fn restore_clock(&mut self) {
        self.slot += 1;
        self.rt.set_clock(self.now, self.slot);
    }

    pub fn lamports(&self, key: Pubkey) -> u64 {
        self.rt
            .svm
            .get_account(&key)
            .map_or(0, |account| account.lamports)
    }

    pub fn account(&self, key: Pubkey) -> Option<Account> {
        self.rt.svm.get_account(&key)
    }

    pub fn pool(&self) -> state::Pool {
        self.rt.deserialize(self.rt.pool_address())
    }
    pub fn ops(&self) -> state::Ops {
        self.rt.deserialize(self.rt.ops_address())
    }
    pub fn ticket(&self) -> state::Ticket {
        self.rt.deserialize(self.rt.ticket_address())
    }
    pub fn admission(&self) -> state::Admission {
        self.rt.deserialize(self.rt.admission_address())
    }
    pub fn config(&self, version: u32) -> state::ConfigVersion {
        self.rt.deserialize(self.rt.config_address(version))
    }
    pub fn position(&self, owner: Pubkey, nonce: u64) -> state::Position {
        self.rt.deserialize(self.rt.position_address(owner, nonce))
    }
    pub fn user(&self, owner: Pubkey) -> state::User {
        self.rt.deserialize(self.rt.user_address(owner))
    }
    pub fn user_round(&self, owner: Pubkey, round: u64) -> state::UserRound {
        self.rt.deserialize(self.rt.user_round_address(round, owner))
    }
    pub fn backer(&self, owner: Pubkey, series: u8) -> state::Backer {
        self.rt.deserialize(self.rt.backer_address(series, owner))
    }
    pub fn registry(&self, page: u64) -> state::RegistryPage {
        self.rt.deserialize(self.rt.registry_address(page))
    }
    pub fn participants(&self, round: u64, page: u64) -> state::ParticipantPage {
        self.rt.deserialize(self.rt.participant_address(round, page))
    }
    pub fn close_record(&self, close_id: u64) -> state::CloseRecord {
        self.rt.deserialize(self.rt.close_record_address(close_id))
    }
    /// Raw bytes of an account (empty when absent), for byte-identity checks.
    pub fn data(&self, key: Pubkey) -> Vec<u8> {
        self.account(key).map(|account| account.data).unwrap_or_default()
    }

    /// Both cash registers: `game_vault - rent == pot + seed + management +
    /// pending_toll + funded_claims + excess` and `ops_cash - rent == free +
    /// locked + unallocated_excess`.
    pub fn assert_ops_cash_identity(&self) {
        self.rt.conservation_check();
    }

    // -----------------------------------------------------------------
    // Instruction builders
    // -----------------------------------------------------------------

    pub fn open_instruction(
        &self,
        owner: Pubkey,
        plan: PlanKindArg,
        gross: u64,
        nonce: u64,
        expected_round_id: u64,
        expected_config: u32,
    ) -> Instruction {
        let pool = self.pool();
        anchor_ix(
            accounts::OpenPosition {
                owner,
                pool: self.rt.pool_address(),
                admission: self.rt.admission_address(),
                game_vault: self.rt.game_vault_address(),
                user: self.rt.user_address(owner),
                user_round: self.rt.user_round_address(pool.round_id, owner),
                position: self.rt.position_address(owner, nonce),
                config: self.rt.config_address(pool.active_config_version),
                // The page the NEXT participant of the round lands on; only
                // read when this is the owner's first position of the round.
                participant_page: self
                    .rt
                    .participant_address(pool.round_id, pool.participant_count / PAGE),
                system_program: system_program::ID,
                event_authority: self.rt.event_authority_address(),
                program: ID,
            },
            instruction::OpenPosition {
                plan,
                gross,
                client_nonce: nonce,
                expected_round_id,
                expected_config,
            },
        )
    }

    pub fn fund_series_a_instruction(
        &self,
        owner: Pubkey,
        gross: u64,
        expected_round_id: u64,
    ) -> Instruction {
        let pool = self.pool();
        anchor_ix(
            accounts::FundSeriesA {
                owner,
                pool: self.rt.pool_address(),
                admission: self.rt.admission_address(),
                game_vault: self.rt.game_vault_address(),
                user: self.rt.user_address(owner),
                backer: self.rt.backer_address(0, owner),
                // The tail page the next appended row lands on; only read
                // when this funding appends a registry row.
                registry: self.rt.registry_address(pool.registry_len / PAGE),
                config: self.rt.config_address(pool.active_config_version),
                system_program: system_program::ID,
                event_authority: self.rt.event_authority_address(),
                program: ID,
            },
            instruction::FundSeriesA {
                gross,
                expected_round_id,
            },
        )
    }

    /// A payout instruction.  `signer` is the `owner` account (and its
    /// `UserRound`); `position_owner` and `nonce` pick the Position PDA, so a
    /// third party can be modelled by passing different keys.
    #[allow(clippy::too_many_arguments)]
    pub fn payout_instruction(
        &self,
        kind: PayoutKind,
        signer: Pubkey,
        position_owner: Pubkey,
        nonce: u64,
        minimum_receive: u64,
        max_work_contribution: u64,
        destination: Pubkey,
        close_record: CloseRecordArg,
    ) -> Instruction {
        let pool = self.pool();
        let position = self.rt.position_address(position_owner, nonce);
        let config_version = self
            .try_position(position_owner, nonce)
            .map_or(1, |position| position.config_version);
        let close_record = match close_record {
            CloseRecordArg::Auto => self.provisioned_next_close_record(),
            CloseRecordArg::Absent => None,
            CloseRecordArg::At(key) => Some(key),
        };
        let metas = accounts::Payout {
            owner: signer,
            pool: self.rt.pool_address(),
            admission: self.rt.admission_address(),
            game_vault: self.rt.game_vault_address(),
            ops: self.rt.ops_address(),
            ops_cash: self.rt.ops_cash_address(),
            user_round: self.rt.user_round_address(pool.round_id, signer),
            position,
            ticket: self.rt.ticket_address(),
            config: self.rt.config_address(config_version),
            destination,
            close_record,
            system_program: system_program::ID,
            event_authority: self.rt.event_authority_address(),
            program: ID,
        };
        match kind {
            PayoutKind::WithdrawSimple => anchor_ix(
                metas,
                instruction::WithdrawSimple {
                    minimum_receive,
                    max_work_contribution,
                },
            ),
            PayoutKind::SettleCompounding => anchor_ix(
                metas,
                instruction::SettleCompounding {
                    minimum_receive,
                    max_work_contribution,
                },
            ),
        }
    }

    pub fn close_simple_instruction(&self, owner: Pubkey, nonce: u64) -> Instruction {
        let config_version = self
            .try_position(owner, nonce)
            .map_or(1, |position| position.config_version);
        anchor_ix(
            accounts::CloseSimplePosition {
                owner,
                pool: self.rt.pool_address(),
                position: self.rt.position_address(owner, nonce),
                config: self.rt.config_address(config_version),
                event_authority: self.rt.event_authority_address(),
                program: ID,
            },
            instruction::CloseSimplePosition,
        )
    }

    fn try_position(&self, owner: Pubkey, nonce: u64) -> Option<state::Position> {
        self.rt
            .try_deserialize::<state::Position>(self.rt.position_address(owner, nonce))
    }

    fn provisioned_next_close_record(&self) -> Option<Pubkey> {
        let key = self.rt.close_record_address(self.pool().next_close_id);
        self.account(key)
            .filter(|account| account.owner == ID && !account.data.is_empty())
            .map(|_| key)
    }

    // -----------------------------------------------------------------
    // Transactions (recorded)
    // -----------------------------------------------------------------

    /// Execute `instructions`, record the observation under `variant`, and
    /// check both cash registers.
    pub fn send(
        &mut self,
        variant: &str,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Outcome {
        let label = self.label(variant);
        let observation = self.rt.execute(label, instructions, signers, CU);
        self.record(observation, true)
    }

    /// Execute `inner` as the G (governance) vault through the test-only Squads
    /// stand-in (`FinancialRuntime::execute_via_vault`), record the
    /// observation under `variant` and check both cash registers.
    pub fn send_via_g(
        &mut self,
        variant: &str,
        inner: Instruction,
        signers: &[&Keypair],
    ) -> Outcome {
        let multisig = self.rt.g_multisig();
        self.send_via_vault(variant, multisig, inner, signers)
    }

    /// [`World::send_via_g`] for any Squads multisig's index-0 vault (the P
    /// vault, a rogue multisig).
    pub fn send_via_vault(
        &mut self,
        variant: &str,
        multisig: Pubkey,
        inner: Instruction,
        signers: &[&Keypair],
    ) -> Outcome {
        let label = self.label(variant);
        let observation = self
            .rt
            .execute_via_vault(label, multisig, 0, inner, signers, CU);
        self.record(observation, true)
    }

    /// [`World::send`] with the Rent sysvar's `lamports_per_byte_year` set to
    /// `rate` for this one transaction only (a rent-quote change between a
    /// fulfillment and its observation).  The original schedule is restored
    /// before the cash registers are checked, because the registers' rent
    /// floors are the schedule in force when the program last synced them.
    pub fn send_with_rent_rate(
        &mut self,
        variant: &str,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
        rate: u64,
    ) -> Outcome {
        let original: Rent = self.rt.svm.get_sysvar::<Rent>();
        let mut changed = original.clone();
        changed.lamports_per_byte_year = rate;
        self.rt.svm.set_sysvar(&changed);
        let label = self.label(variant);
        let observation = self.rt.execute(label, instructions, signers, CU);
        self.rt.svm.set_sysvar(&original);
        self.record(observation, true)
    }

    /// Submit an already signed transaction (see [`FinancialRuntime::sign`]).
    pub fn submit(&mut self, variant: &str, signed: &SignedTransaction) -> Outcome {
        let label = self.label(variant);
        let observation = self.rt.submit(label, signed, CU);
        self.record(observation, true)
    }

    fn setup(
        &mut self,
        what: &str,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Observation {
        let label = format!("{}.setup.{what}", self.scope);
        let observation = self.rt.execute(label, instructions, signers, CU);
        match self.record(observation, false) {
            Ok(observation) => observation,
            Err(observation) => panic!("fixture setup `{what}` failed: {observation:?}"),
        }
    }

    fn record(&mut self, observation: Observation, logged: bool) -> Outcome {
        self.rt.conservation_check();
        self.last = Some(observation.clone());
        if logged {
            self.log.push(observation.clone());
        }
        if observation.success {
            Ok(observation)
        } else {
            Err(observation)
        }
    }

    pub fn open_position(
        &mut self,
        variant: &str,
        owner: &Keypair,
        plan: PlanKindArg,
        gross: u64,
        nonce: u64,
    ) -> Outcome {
        let pool = self.pool();
        self.open_position_expecting(
            variant,
            owner,
            plan,
            gross,
            nonce,
            pool.round_id,
            pool.active_config_version,
        )
    }

    /// `open_position` with explicit `expected_round_id` / `expected_config`
    /// arguments (the stale-argument rejections).
    #[allow(clippy::too_many_arguments)]
    pub fn open_position_expecting(
        &mut self,
        variant: &str,
        owner: &Keypair,
        plan: PlanKindArg,
        gross: u64,
        nonce: u64,
        expected_round_id: u64,
        expected_config: u32,
    ) -> Outcome {
        let instruction = self.open_instruction(
            owner.pubkey(),
            plan,
            gross,
            nonce,
            expected_round_id,
            expected_config,
        );
        self.send(variant, vec![instruction], &[owner])
    }

    pub fn fund_series_a(
        &mut self,
        variant: &str,
        owner: &Keypair,
        gross: u64,
        expected_round_id: u64,
    ) -> Outcome {
        let instruction = self.fund_series_a_instruction(owner.pubkey(), gross, expected_round_id);
        self.send(variant, vec![instruction], &[owner])
    }

    pub fn withdraw_simple(
        &mut self,
        variant: &str,
        owner: &Keypair,
        nonce: u64,
        minimum_receive: u64,
        destination: Pubkey,
        max_work_contribution: u64,
    ) -> Outcome {
        self.payout(
            variant,
            PayoutKind::WithdrawSimple,
            owner,
            nonce,
            minimum_receive,
            destination,
            max_work_contribution,
            CloseRecordArg::Auto,
        )
    }

    pub fn settle_compounding(
        &mut self,
        variant: &str,
        owner: &Keypair,
        nonce: u64,
        minimum_receive: u64,
        destination: Pubkey,
        max_work_contribution: u64,
    ) -> Outcome {
        self.payout(
            variant,
            PayoutKind::SettleCompounding,
            owner,
            nonce,
            minimum_receive,
            destination,
            max_work_contribution,
            CloseRecordArg::Auto,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn payout(
        &mut self,
        variant: &str,
        kind: PayoutKind,
        owner: &Keypair,
        nonce: u64,
        minimum_receive: u64,
        destination: Pubkey,
        max_work_contribution: u64,
        close_record: CloseRecordArg,
    ) -> Outcome {
        let instruction = self.payout_instruction(
            kind,
            owner.pubkey(),
            owner.pubkey(),
            nonce,
            minimum_receive,
            max_work_contribution,
            destination,
            close_record,
        );
        self.send(variant, vec![instruction], &[owner])
    }

    pub fn close_simple_position(&mut self, variant: &str, owner: &Keypair, nonce: u64) -> Outcome {
        let instruction = self.close_simple_instruction(owner.pubkey(), nonce);
        self.send(variant, vec![instruction], &[owner])
    }

    // -----------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------

    /// The most recent transaction's observation (recorded or setup).
    pub fn last(&self) -> &Observation {
        self.last
            .as_ref()
            .expect("no transaction has been executed in this world yet")
    }

    /// The last `E` event of the most recent transaction, decoded from the
    /// event-CPI inner instruction (`EVENT_IX_TAG_LE || disc || Borsh`).
    pub fn last_event<E: anchor_lang::Event>(&self) -> E {
        self.last().last_event::<E>()
    }

    // -----------------------------------------------------------------
    // Rejection and rollback assertions
    // -----------------------------------------------------------------

    /// The transaction failed with exactly `Custom(6000 + expected)` and
    /// changed nothing but the fee.
    pub fn assert_rejected(&self, observation: &Observation, expected: ErrorCode) {
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
    }

    /// The transaction failed with exactly `Custom(framework_code)` (an
    /// Anchor framework or system-program code) and changed nothing but the
    /// fee.
    pub fn assert_rejected_with_custom(&self, observation: &Observation, framework_code: u32) {
        assert!(
            !observation.success,
            "{} unexpectedly succeeded: {observation:?}",
            observation.label
        );
        assert_eq!(
            custom_code(observation),
            Some(framework_code),
            "{}: expected Custom({framework_code}), got {:?}",
            observation.label,
            observation.error
        );
        observation.assert_rollback_except_fee();
    }

    /// Every account the World can touch is byte-identical to `before`
    /// except for the fee the payer actually paid (0 for a runtime refusal
    /// before execution, the signature fee otherwise).
    pub fn assert_fee_only(&self, before: &Snapshot, observation: &Observation) {
        let paid = observation
            .payer_lamports_before
            .checked_sub(observation.payer_lamports_after)
            .unwrap_or_else(|| panic!("{}: the payer gained lamports", observation.label));
        assert!(
            paid == 0 || paid == observation.expected_fee_lamports,
            "{}: payer paid {paid}, expected 0 or {}",
            observation.label,
            observation.expected_fee_lamports
        );
        let after = self.snapshot_all().except_payer_fee(paid);
        assert!(
            &after == before,
            "{}: accounts changed beyond the fee: {:?}",
            observation.label,
            after.differences(before)
        );
    }

    // -----------------------------------------------------------------
    // Snapshots
    // -----------------------------------------------------------------

    /// Mark one more address for [`World::snapshot_all`].
    pub fn watch(&mut self, key: Pubkey) {
        self.watch.insert(key);
    }

    fn watch_keys(&self) -> BTreeSet<Pubkey> {
        let rt = &self.rt;
        let owner = rt.owner.pubkey();
        let attacker = rt.attacker.pubkey();
        let mut keys: BTreeSet<Pubkey> = [
            rt.pool_address(),
            rt.admission_address(),
            rt.game_vault_address(),
            rt.ops_address(),
            rt.ops_cash_address(),
            rt.ticket_address(),
            rt.config_address(1),
            rt.config_address(2),
            rt.payer.pubkey(),
            owner,
            attacker,
            rt.sponsor.pubkey(),
            rt.management.pubkey(),
            self.dest,
            rt.user_address(owner),
            rt.user_address(attacker),
        ]
        .into_iter()
        .collect();
        for round in 1..=3 {
            keys.insert(rt.user_round_address(round, owner));
            keys.insert(rt.user_round_address(round, attacker));
            for index in 0..=2 {
                keys.insert(rt.participant_address(round, index));
            }
        }
        for nonce in 1..=8 {
            keys.insert(rt.position_address(owner, nonce));
            keys.insert(rt.position_address(attacker, nonce));
        }
        for series in 0..=1 {
            keys.insert(rt.backer_address(series, owner));
            keys.insert(rt.backer_address(series, attacker));
        }
        for index in 0..16 {
            keys.insert(rt.backer_address(0, Self::synthetic_backer(index)));
        }
        for index in 0..=2 {
            keys.insert(rt.registry_address(index));
            keys.insert(rt.snapshot_address(index));
        }
        for close_id in 1..=3 {
            keys.insert(rt.close_record_address(close_id));
        }
        keys.extend(self.watch.iter().copied());
        keys
    }

    /// `(lamports, owner, data sha256)` of every account the World can touch:
    /// the program PDAs, every actor, the destination, and the position,
    /// round, participant, registry, snapshot, backer and close-record
    /// addresses a row can reach.
    pub fn snapshot_all(&self) -> Snapshot {
        let accounts = self
            .watch_keys()
            .into_iter()
            .map(|key| {
                let digest = self.rt.svm.get_account(&key).map(|account| AccountDigest {
                    lamports: account.lamports,
                    owner: account.owner,
                    executable: account.executable,
                    data_sha256: Sha256::digest(&account.data).into(),
                });
                (key, digest)
            })
            .collect();
        Snapshot {
            payer: self.rt.payer.pubkey(),
            accounts,
        }
    }

    // -----------------------------------------------------------------
    // Fixture writers (kept consistent with both cash registers)
    // -----------------------------------------------------------------

    pub fn set_pool(&mut self, change: impl FnOnce(&mut state::Pool)) {
        let mut pool = self.pool();
        change(&mut pool);
        self.rt.replace_pool(pool);
    }

    pub fn set_ops(&mut self, change: impl FnOnce(&mut state::Ops)) {
        let mut ops = self.ops();
        change(&mut ops);
        self.rt.replace_ops(ops);
    }

    pub fn set_public_series_a(&mut self, enabled: bool) {
        let mut config = self.config(1);
        config.public_series_a_enabled = enabled;
        self.rt.replace_config(config);
    }

    pub fn set_paused(&mut self, paused: bool) {
        let mut admission = self.admission();
        admission.paused = paused;
        self.rt.replace_admission(admission);
    }

    /// Write `Pool.pot` AND the game vault's lamports together: the vault
    /// gains (or loses) exactly the pot's difference, so the game-vault
    /// identity holds.
    pub fn set_pot_fixture(&mut self, pot: u64) {
        let before = self.pool().pot;
        self.set_pool(|pool| pool.pot = pot);
        let vault = self.rt.game_vault_address();
        let lamports = self.lamports(vault);
        let adjusted = lamports
            .checked_sub(before)
            .and_then(|value| value.checked_add(pot))
            .expect("vault lamports stay in range");
        self.rt.set_lamports(vault, adjusted);
    }

    /// Write `Ops.free_balance` AND the OpsCash lamports together: the cash
    /// gains (or loses) exactly the difference, so the ops-cash identity
    /// holds.
    pub fn set_ops_free(&mut self, free: u64) {
        let before = self.ops().free_balance;
        self.set_ops(|ops| ops.free_balance = free);
        let cash = self.rt.ops_cash_address();
        let lamports = self.lamports(cash);
        let adjusted = lamports
            .checked_sub(before)
            .and_then(|value| value.checked_add(free))
            .expect("ops cash lamports stay in range");
        self.rt.set_lamports(cash, adjusted);
    }

    /// A Simple position (nonce 1) that never went through `open_position`:
    /// the pot, management and vault are NOT touched.  The owner is
    /// registered as a participant of the current round (page row, count,
    /// `UserRound.deposits` and slot), because the closing rows read the
    /// participant count; no quota call is recorded.
    pub fn open_fixture_simple(&mut self, owner: Pubkey, gross: u64) {
        self.open_fixture_position(owner, 1, PlanKindArg::Simple, gross);
    }

    pub fn open_fixture_position(
        &mut self,
        owner: Pubkey,
        nonce: u64,
        plan: PlanKindArg,
        gross: u64,
    ) {
        let pool = self.pool();
        let config = self.config(pool.active_config_version);
        let now = self.now();
        let duration = u64::from(config.plan_durations_s[usize::from(plan.discriminant())]);
        self.rt.replace_position(state::Position {
            bump: pda(&[
                state::POSITION_SEED,
                owner.as_ref(),
                &nonce.to_le_bytes(),
            ])
            .1,
            owner,
            nonce,
            round_id: pool.round_id,
            config_version: pool.active_config_version,
            kind: plan.discriminant(),
            principal: gross,
            opened_at: now,
            maturity_at: now + duration,
            last_accrued_at: now,
            ..state::Position::default()
        });
        let mut user_round = self.user_round_or_new(owner, pool.round_id);
        let first_in_round = user_round.deposits == 0;
        user_round.deposits += u128::from(gross);
        if first_in_round {
            user_round.participant_slot = pool.participant_count;
            self.append_participant(owner);
        }
        self.rt.replace_user_round(user_round);
        self.set_pool(|pool| pool.total_gross_deposits += u128::from(gross));
    }

    fn user_round_or_new(&self, owner: Pubkey, round: u64) -> state::UserRound {
        match self.try_user_round(owner, round) {
            Some(user_round) => user_round,
            None => state::UserRound {
                bump: pda(&[
                    state::USER_ROUND_SEED,
                    &round.to_le_bytes(),
                    owner.as_ref(),
                ])
                .1,
                round_id: round,
                owner,
                ..state::UserRound::default()
            },
        }
    }

    fn try_user_round(&self, owner: Pubkey, round: u64) -> Option<state::UserRound> {
        self.rt
            .try_deserialize::<state::UserRound>(self.rt.user_round_address(round, owner))
    }

    /// Append `key` to the current round's participant list on the page
    /// `participant_count / 8` (created when missing) and bump the counters.
    fn append_participant(&mut self, key: Pubkey) {
        let pool = self.pool();
        let index = pool.participant_count / PAGE;
        let mut page = self
            .rt
            .try_deserialize::<state::ParticipantPage>(
                self.rt.participant_address(pool.round_id, index),
            )
            .unwrap_or_else(|| state::ParticipantPage {
                bump: pda(&[
                    state::PARTICIPANTS_SEED,
                    &pool.round_id.to_le_bytes(),
                    &index.to_le_bytes(),
                ])
                .1,
                round_id: pool.round_id,
                page: index,
                ..state::ParticipantPage::default()
            });
        let slot = usize::from(page.len);
        assert!(slot < state::PAGE_CAPACITY, "participant page {index} is full");
        page.rows[slot] = key;
        page.len += 1;
        self.rt.replace_participant_page(page);
        self.set_pool(|pool| {
            pool.participant_count += 1;
            pool.participant_pages = pool.participant_pages.max(index + 1);
        });
    }

    /// A fixture participant of the current round with its sealed `UserRound`
    /// (`deposits`, `net_payout`) and `User` history bits, appended to the
    /// participant list: the inputs `build_candidates` reads.  The list row,
    /// `participant_count`, `UserRound.participant_slot` and the pool totals
    /// stay consistent; no cash moves (the lamports behind `deposits` are not
    /// modelled, exactly like [`World::open_fixture_simple`]).
    pub fn install_participant(
        &mut self,
        owner: Pubkey,
        deposits: u128,
        net_payout: u128,
        ever_a: bool,
        ever_b: bool,
    ) {
        let pool = self.pool();
        self.install_user(owner, ever_a, ever_b);
        self.rt.replace_user_round(state::UserRound {
            bump: pda(&[
                state::USER_ROUND_SEED,
                &pool.round_id.to_le_bytes(),
                owner.as_ref(),
            ])
            .1,
            round_id: pool.round_id,
            owner,
            deposits,
            net_payout,
            participant_slot: pool.participant_count,
        });
        self.append_participant(owner);
        self.set_pool(|pool| {
            pool.total_gross_deposits += deposits;
            pool.total_net_payouts += net_payout;
        });
        self.watch.insert(self.rt.user_round_address(pool.round_id, owner));
    }

    /// Fixture write: the `User` account of `owner` with the given Series A /
    /// B history bits (everything else default).
    pub fn install_user(&mut self, owner: Pubkey, ever_a: bool, ever_b: bool) {
        self.rt.replace_user(state::User {
            bump: pda(&[state::USER_SEED, owner.as_ref()]).1,
            owner,
            ever_a,
            ever_b,
            ..state::User::default()
        });
        self.watch.insert(self.rt.user_address(owner));
    }

    /// `count` synthetic participants (never the owner) after the existing
    /// ones, each with a `UserRound`-less list row only.
    pub fn append_synthetic_participants(&mut self, count: u64) {
        let already = self.pool().participant_count;
        for index in 0..count {
            self.append_participant(Self::synthetic_participant(already + index));
        }
    }

    /// A fixture open Series A Backer at the next registry slot: active, with
    /// `62_000_000` entitlement, funding sequence `next_sequence`, a registry
    /// row, `registry_len + 1`, and the registry / snapshot pages that slot
    /// implies.  No pot is credited.
    pub fn install_open_backer(&mut self, owner: Pubkey) {
        let pool = self.pool();
        let slot = pool.registry_len;
        let index = slot / PAGE;
        let backer_key = self.rt.backer_address(0, owner);
        let mut page = self
            .rt
            .try_deserialize::<state::RegistryPage>(self.rt.registry_address(index))
            .unwrap_or_else(|| state::RegistryPage {
                bump: pda(&[state::REGISTRY_SEED, &index.to_le_bytes()]).1,
                index,
                ..state::RegistryPage::default()
            });
        page.keys[(slot % PAGE) as usize] = backer_key;
        self.rt.replace_registry(page);
        self.rt.replace_backer(state::Backer {
            bump: pda(&[state::BACKER_SEED, &[0u8], owner.as_ref()]).1,
            owner,
            series: 0,
            amount: 50_000_000,
            entitlement: FIXTURE_BACKER_ENTITLEMENT,
            first_sequence: pool.next_sequence,
            active: true,
            slot,
            ..state::Backer::default()
        });
        let len_after = slot + 1;
        let snapshot_pages = len_after.div_ceil(PAGE).max(1);
        for page_index in pool.snapshot_pages..snapshot_pages {
            self.rt.replace_snapshot(state::SnapshotPage {
                bump: pda(&[state::SNAPSHOT_SEED, &page_index.to_le_bytes()]).1,
                index: page_index,
                ..state::SnapshotPage::default()
            });
        }
        self.set_pool(|pool| {
            pool.registry_len = len_after;
            pool.next_sequence += 1;
            pool.registry_pages = pool.registry_pages.max(index + 1);
            pool.snapshot_pages = pool.snapshot_pages.max(snapshot_pages);
        });
    }

    /// `count` fixture open Backers with synthetic owners.
    pub fn install_open_backers(&mut self, count: u64) {
        for index in 0..count {
            self.install_open_backer(Self::synthetic_backer(index));
        }
    }

    /// `CloseRecord(close_id)` exactly as `provision_close_record(Free)`
    /// leaves it (identity, pending promotion, the ops rent component), but
    /// installed directly: the rent comes from the fixture, not from Ops.
    pub fn provision_close_record(&mut self, close_id: u64) {
        let rent = self.rt.rent(state::CloseRecord::SPACE);
        self.rt.replace_close_record(state::CloseRecord {
            bump: pda(&[state::CLOSE_SEED, &close_id.to_le_bytes()]).1,
            close_id,
            rent: state::RentComponents {
                ops: rent,
                owner: 0,
                owner_refund: Pubkey::default(),
            },
            ..state::CloseRecord::default()
        });
    }

    /// The round invalidation path: the pool moves to the next round (no
    /// close in progress, an empty participant list with its page 0
    /// provisioned).  Positions of the old round are left untouched.
    pub fn advance_round_fixture(&mut self) {
        let round = self.pool().round_id + 1;
        self.set_pool(|pool| {
            pool.round_id = round;
            pool.participant_count = 0;
            pool.participant_pages = 0;
            pool.current_close_id = None;
        });
        self.rt.install_participant_page(round, 0);
    }

    // -----------------------------------------------------------------
    // Reserve arithmetic
    // -----------------------------------------------------------------

    /// What a closing payout must lock for `participants` participants and a
    /// registry of `registry_len` rows with `existing_p` / `existing_f`
    /// candidate pages already provisioned: the program's own pure
    /// `required_close_reserve` over a schedule captured from the active
    /// config and this LiteSVM's rent, cross-checked against an independent
    /// restatement of spec 8.1 from the program's `SPACE` constants.
    pub fn expected_reserve(
        &self,
        participants: u64,
        registry_len: u64,
        existing_p: u64,
        existing_f: u64,
    ) -> u64 {
        let rent: Rent = self.rt.svm.get_sysvar::<Rent>();
        let config = self.config(self.pool().active_config_version);
        let schedule = state::ReserveSchedule::capture(&rent, &config);
        let from_program =
            required_close_reserve(participants, registry_len, existing_p, existing_f, &schedule)
                .expect("required_close_reserve");

        let per_set = participants.div_ceil(PAGE);
        let missing_pages = per_set.saturating_sub(existing_p) + per_set.saturating_sub(existing_f);
        let registry_extension = u64::from(registry_len.is_multiple_of(PAGE));
        let fee = config.provider_request_fee_lamports;
        let provider_cap = fee + fee * u64::from(config.provider_fee_headroom_bps) / 10_000;
        let by_hand = self.rt.rent(state::CandidatePage::SPACE) * missing_pages
            + self.rt.rent(state::RegistryPage::SPACE) * registry_extension
            + self.rt.rent(state::Backer::SPACE)
            + self.rt.rent(usize::from(config.pending_request_bytes))
            + provider_cap;
        assert_eq!(
            from_program, by_hand,
            "required_close_reserve({participants}, {registry_len}, {existing_p}, {existing_f}) \
             disagrees with the independent restatement"
        );
        from_program
    }
}

// ---------------------------------------------------------------------------
// Allocation fixtures, typed phase instructions and the ticket driver (M6)
// ---------------------------------------------------------------------------

/// One fixture Backer: `entitlement - lifetime` is its remaining entitlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackerSpec {
    pub owner: Pubkey,
    pub series: u8,
    pub entitlement: u128,
    pub lifetime: u128,
}

impl BackerSpec {
    /// A fresh Backer (`lifetime` 0) holding `remaining` entitlement.
    pub fn new(owner: Pubkey, series: u8, remaining: u128) -> Self {
        Self {
            owner,
            series,
            entitlement: remaining,
            lifetime: 0,
        }
    }

    pub fn remaining(&self) -> u128 {
        self.entitlement - self.lifetime
    }
}

/// Deterministic xorshift64* generator for the randomized fixtures.
#[derive(Clone, Debug)]
pub struct Xorshift64(u64);

impl Xorshift64 {
    pub fn new(seed: u64) -> Self {
        // A zero state would be a fixed point; mix the seed first.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    // Not an `Iterator`: a seeded generator that never ends; the name is deliberate.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// The `index`-th synthetic Backer owner of the allocation fixtures.  Distinct
/// for every index (the index is stored in the first eight bytes).
pub fn allocation_owner(index: u64) -> Pubkey {
    let mut bytes = [0xA7u8; 32];
    bytes[..8].copy_from_slice(&(index + 1).to_le_bytes());
    Pubkey::new_from_array(bytes)
}

/// `n` deterministic fixture Backers: a mix of Series A (about two thirds) and
/// B, with open rows of every size class (near the 10-lamport cutoff, ordinary,
/// enormous, partly repaid) and rows at or below the cutoff.  The first four
/// rows are fixed so every seed has an oldest open A, a B that any toll caps
/// (newly capped, pruned with real accounts), a closed A (5) and a closed B
/// at exactly the cutoff (10).
pub fn random_backer_specs(seed: u64, n: usize) -> Vec<BackerSpec> {
    let mut rng = Xorshift64::new(seed);
    (0..n)
        .map(|index| {
            let owner = allocation_owner(index as u64);
            let (series, entitlement, lifetime) = match index {
                0 => (SERIES_A, 100_000u128, 0u128),
                1 => (SERIES_B, 30, 0),
                2 => (SERIES_A, 5, 0),
                3 => (SERIES_B, 10, 0),
                _ => {
                    let series = if rng.next().is_multiple_of(3) { SERIES_B } else { SERIES_A };
                    let (entitlement, lifetime) = match rng.next() % 6 {
                        // Closed: at most 10 lamports remain.
                        0 => (1 + u128::from(rng.next() % 10), 0),
                        // Open, just above the cutoff.
                        1 => (11 + u128::from(rng.next() % 40), 0),
                        // Ordinary.
                        2 => (1_000 + u128::from(rng.next() % 100_000), 0),
                        // Enormous.
                        3 => (u128::from(rng.next()) * 1_000, 0),
                        // Partly repaid, still open (remaining >= 11).
                        4 => {
                            let entitlement = 50 + u128::from(rng.next() % 5_000);
                            let lifetime = u128::from(rng.next()) % (entitlement - 10);
                            (entitlement, lifetime)
                        }
                        // Repaid down to 0..=10 remaining: closed.
                        _ => {
                            let entitlement = 100 + u128::from(rng.next() % 5_000);
                            (entitlement, entitlement - u128::from(rng.next() % 11))
                        }
                    };
                    (series, entitlement, lifetime)
                }
            };
            BackerSpec {
                owner,
                series,
                entitlement,
                lifetime,
            }
        })
        .collect()
}

impl World {
    // -----------------------------------------------------------------
    // Allocation fixtures
    // -----------------------------------------------------------------

    /// Genesis plus fixture Backers `(owner, series, remaining)`: entitlement
    /// = `remaining`, `lifetime` 0, `active` rows at sequential registry slots
    /// with sequential `first_sequence` (the first row is the oldest), the
    /// registry rows are the Backer PDAs, and the registry / snapshot pages
    /// the slots imply are provisioned (`snapshot_pages * 8 >= registry_len`).
    /// No cash backs the entitlements: only a ticket's toll does.
    pub fn with_backers(specs: &[(Pubkey, u8, u128)]) -> Self {
        let specs: Vec<BackerSpec> = specs
            .iter()
            .map(|&(owner, series, remaining)| BackerSpec::new(owner, series, remaining))
            .collect();
        Self::with_backer_specs(&specs)
    }

    pub fn with_backer_specs(specs: &[BackerSpec]) -> Self {
        let mut world = Self::genesis();
        world.install_backer_specs(specs);
        world
    }

    /// `n` deterministic Backers from [`random_backer_specs`].
    pub fn with_random_backers(seed: u64, n: usize) -> Self {
        Self::with_backer_specs(&random_backer_specs(seed, n))
    }

    /// Append fixture Backers to the dense registry, creating the registry and
    /// snapshot pages their slots need.
    pub fn install_backer_specs(&mut self, specs: &[BackerSpec]) {
        for spec in specs {
            let pool = self.pool();
            let slot = pool.registry_len;
            let page_index = slot / PAGE;
            let backer_key = self.rt.backer_address(spec.series, spec.owner);
            assert!(
                self.fixture_backers.insert(backer_key),
                "fixture Backer {backer_key} is installed twice"
            );

            let mut page = self
                .rt
                .try_deserialize::<state::RegistryPage>(self.rt.registry_address(page_index))
                .unwrap_or_else(|| state::RegistryPage {
                    bump: pda(&[state::REGISTRY_SEED, &page_index.to_le_bytes()]).1,
                    index: page_index,
                    ..state::RegistryPage::default()
                });
            page.keys[(slot % PAGE) as usize] = backer_key;
            self.rt.replace_registry(page);
            self.rt.replace_backer(state::Backer {
                bump: pda(&[state::BACKER_SEED, &[spec.series], spec.owner.as_ref()]).1,
                owner: spec.owner,
                series: spec.series,
                // Mirrors the 124 % mint; the allocation never reads it.
                amount: spec.entitlement / 124 * 100,
                entitlement: spec.entitlement,
                lifetime: spec.lifetime,
                first_sequence: pool.next_sequence,
                active: true,
                slot,
                ..state::Backer::default()
            });

            let snapshot_pages = (slot + 1).div_ceil(PAGE).max(1);
            for index in pool.snapshot_pages..snapshot_pages {
                self.rt.replace_snapshot(state::SnapshotPage {
                    bump: pda(&[state::SNAPSHOT_SEED, &index.to_le_bytes()]).1,
                    index,
                    ..state::SnapshotPage::default()
                });
            }
            self.set_pool(|pool| {
                pool.registry_len = slot + 1;
                pool.next_sequence += 1;
                pool.registry_pages = pool.registry_pages.max(page_index + 1);
                pool.snapshot_pages = pool.snapshot_pages.max(snapshot_pages);
            });
            self.watch.insert(backer_key);
            self.watch.insert(self.rt.registry_address(page_index));
            self.watch.insert(self.rt.snapshot_address(page_index));
        }
    }

    /// What `withdraw_simple` does for a toll, as a fixture: escrow it
    /// (`pending_toll`, plus the same lamports in the game vault so the vault
    /// identity holds) and start the next ticket in `DISCOVER` over the whole
    /// registry.  Mirrors `positions.rs::ticket_start` field for field: the
    /// workspace must be `IDLE`, the snapshot pages must cover the registry,
    /// the captured length and version are the pool's, the split is pinned from
    /// the active `ConfigVersion` (3_500 / 4_500 / 2_000), `close_id` is `None`
    /// and every cursor, count, slice, accumulator and credit is zero.
    pub fn start_ticket_fixture(&mut self, toll: u64) {
        let pool = self.pool();
        let current = self.ticket();
        assert_eq!(pool.active_ticket, 0, "a ticket is already active");
        assert_eq!(current.phase, state::IDLE, "the ticket workspace is not IDLE");
        assert!(
            toll > 0 && pool.registry_len > 0,
            "a toll starts a ticket only when there is a toll and at least one Backer"
        );
        assert!(
            pool.snapshot_pages * PAGE >= pool.registry_len,
            "snapshot pages must cover the registry before a toll is taken"
        );
        let config = self.config(pool.active_config_version);
        let id = pool.next_ticket;
        self.rt.replace_ticket(state::Ticket {
            bump: current.bump,
            id,
            phase: state::DISCOVER,
            close_id: None,
            captured_len: pool.registry_len,
            expected_version: pool.registry_version,
            toll,
            senior_bps: config.senior_bps,
            a_bps: config.a_bps,
            b_bps: config.b_bps,
            ..state::Ticket::default()
        });
        self.set_pool(|pool| {
            pool.pending_toll += toll;
            pool.next_ticket = id + 1;
            pool.active_ticket = id;
        });
        let vault = self.rt.game_vault_address();
        self.rt.add_lamports(vault, toll);
    }

    /// Give a Backer `funded` lamports of already-finalized claims: `funded`
    /// and `lifetime` rise by it, `Pool.funded_claims` counts it and the game
    /// vault holds the cash.
    pub fn fund_backer_fixture(&mut self, owner: Pubkey, series: u8, funded: u64) {
        let mut backer = self.backer(owner, series);
        backer.funded = backer.funded.checked_add(funded).expect("funded fits u64");
        backer.lifetime += u128::from(funded);
        assert!(
            backer.lifetime <= backer.entitlement,
            "fixture credit exceeds the entitlement"
        );
        self.rt.replace_backer(backer);
        self.set_pool(|pool| pool.funded_claims += u128::from(funded));
        let vault = self.rt.game_vault_address();
        self.rt.add_lamports(vault, funded);
    }

    // -----------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------

    pub fn snapshot_page(&self, index: u64) -> state::SnapshotPage {
        self.rt.deserialize(self.rt.snapshot_address(index))
    }

    /// A Backer by its PDA.
    pub fn backer_at(&self, key: Pubkey) -> state::Backer {
        self.rt.deserialize(key)
    }

    /// `Some(key)` when `key` is an existing, non-empty program-owned account
    /// (an optional claim account is otherwise the placeholder).
    pub fn program_account(&self, key: Pubkey) -> Option<Pubkey> {
        self.account(key)
            .filter(|account| account.owner == ID && !account.data.is_empty())
            .map(|_| key)
    }

    /// The game-vault identity, spelled out: `vault - rent == pot + seed +
    /// management + pending_toll + funded_claims + excess`.  (`World` also
    /// checks it after every recorded transaction; a row calls this to assert
    /// it explicitly at the point it names.)
    pub fn assert_vault_identity(&self) {
        let pool = self.pool();
        let vault = self.lamports(self.rt.game_vault_address());
        let rent = self.rt.rent(state::GameVault::SPACE);
        let spendable = u128::from(vault.checked_sub(rent).expect("vault above its rent floor"));
        let accounted = u128::from(pool.pot)
            + u128::from(pool.seed)
            + u128::from(pool.management)
            + u128::from(pool.pending_toll)
            + pool.funded_claims
            + u128::from(pool.excess);
        assert_eq!(
            spendable, accounted,
            "game vault identity: vault {vault} - rent {rent} != pot {} + seed {} + management {} + pending_toll {} + funded_claims {} + excess {}",
            pool.pot, pool.seed, pool.management, pool.pending_toll, pool.funded_claims, pool.excess
        );
        assert_eq!(
            accounted,
            musical_chairs_program::game_accounted(&pool).expect("game_accounted")
        );
    }

    /// SHA-256 over the Pool, the Ticket, every fixture Backer (in address
    /// order) and every registry and snapshot page (in index order), each
    /// length-prefixed, so two worlds that took different batch routes to the
    /// same books digest identically.
    pub fn digest_financial_state(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        let mut feed = |tag: &str, key: Pubkey| {
            let data = self.data(key);
            hasher.update(tag.as_bytes());
            hasher.update((data.len() as u64).to_le_bytes());
            hasher.update(&data);
        };
        feed("pool", self.rt.pool_address());
        feed("ticket", self.rt.ticket_address());
        for key in &self.fixture_backers {
            feed("backer", *key);
        }
        let pool = self.pool();
        for index in 0..pool.registry_pages {
            feed("registry", self.rt.registry_address(index));
        }
        for index in 0..pool.snapshot_pages {
            feed("snapshot", self.rt.snapshot_address(index));
        }
        hasher.finalize().into()
    }

    // -----------------------------------------------------------------
    // Typed phase instructions (default accounts built from the chain state)
    // -----------------------------------------------------------------

    pub fn discover_rows(
        &mut self,
        variant: &str,
        expected_ticket_id: u64,
        expected_cursor: u64,
        rows: u8,
    ) -> Outcome {
        let remaining = self.rt.discover_remaining(expected_cursor, rows);
        let instruction =
            self.rt
                .discover_ix(expected_ticket_id, expected_cursor, rows, remaining);
        self.send(variant, vec![instruction], &[])
    }

    pub fn apply_senior(&mut self, variant: &str, expected_ticket_id: u64) -> Outcome {
        let instruction = self
            .rt
            .senior_ix(expected_ticket_id, self.rt.senior_backer());
        self.send(variant, vec![instruction], &[])
    }

    pub fn seal_weights(&mut self, variant: &str, expected_ticket_id: u64) -> Outcome {
        let instruction = self.rt.seal_ix(expected_ticket_id);
        self.send(variant, vec![instruction], &[])
    }

    pub fn allocate_rows(
        &mut self,
        variant: &str,
        expected_ticket_id: u64,
        expected_cursor: u64,
        rows: u8,
    ) -> Outcome {
        let remaining = self.rt.allocate_remaining(expected_cursor, rows);
        let instruction =
            self.rt
                .allocate_ix(expected_ticket_id, expected_cursor, rows, remaining);
        self.send(variant, vec![instruction], &[])
    }

    /// `prune_rows` over the keeper's plan for the ticket's current cursor
    /// (see [`FinancialRuntime::prune_plan`], chains NOT cut, so an illegal
    /// batch is sent as built).
    pub fn prune_rows(
        &mut self,
        variant: &str,
        expected_ticket_id: u64,
        expected_cursor: u64,
        rows: u8,
    ) -> Outcome {
        let plan = self.rt.prune_plan(rows, false, false);
        let instruction =
            self.rt
                .prune_ix(expected_ticket_id, expected_cursor, rows, plan.remaining);
        self.send(variant, vec![instruction], &[])
    }

    pub fn commit_ticket(&mut self, variant: &str, expected_ticket_id: u64) -> Outcome {
        let close_record = self
            .ticket()
            .close_id
            .map(|close_id| self.rt.close_record_address(close_id));
        let instruction = self.rt.commit_ix(expected_ticket_id, close_record);
        self.send(variant, vec![instruction], &[])
    }

    /// The owner's combined claim over whichever of its Series A and B Backer
    /// accounts exist.  `Ok` is the lamports paid (the `BackerClaimed` amount,
    /// cross-checked against the destination's balance change).
    pub fn claim_backer(
        &mut self,
        variant: &str,
        owner: &Keypair,
        destination: Pubkey,
    ) -> Result<u64, Observation> {
        let owner_key = owner.pubkey();
        let backer_a = self.program_account(self.rt.backer_address(SERIES_A, owner_key));
        let backer_b = self.program_account(self.rt.backer_address(SERIES_B, owner_key));
        self.claim_backer_with(variant, owner, backer_a, backer_b, destination)
    }

    /// [`World::claim_backer`] with explicit optional Backer accounts.
    pub fn claim_backer_with(
        &mut self,
        variant: &str,
        owner: &Keypair,
        backer_a: Option<Pubkey>,
        backer_b: Option<Pubkey>,
        destination: Pubkey,
    ) -> Result<u64, Observation> {
        let before = self.lamports(destination);
        let instruction =
            self.rt
                .claim_backer_ix(owner.pubkey(), backer_a, backer_b, destination);
        let observation = self.send(variant, vec![instruction], &[owner])?;
        let claimed = observation.last_event::<BackerClaimed>();
        assert_eq!(claimed.owner, owner.pubkey());
        assert_eq!(
            self.lamports(destination) - before,
            claimed.amount,
            "{}: the destination receives exactly the claimed amount",
            observation.label
        );
        assert_eq!(claimed.a_part + claimed.b_part, claimed.amount);
        Ok(claimed.amount)
    }

    // -----------------------------------------------------------------
    // The ticket driver
    // -----------------------------------------------------------------

    /// Send the next phase instruction of the active ticket, in batches of at
    /// most `rows` rows, and return its observation.  Labelled
    /// `<phase>.<ordinal>` (the ordinal counts per phase over the whole
    /// world).  A Prune batch stops before a chained move (the keeper's
    /// one-row fallback), so the step never trips `DuplicateWritableAccount`.
    /// Panics if the transaction fails: the happy path must succeed.
    pub fn advance_step(&mut self, rows: u8) -> Observation {
        let step = self.rt.next_phase_step(rows, false);
        let ordinal = {
            let count = self.drive_counts.entry(step.name).or_insert(0);
            let ordinal = *count;
            *count += 1;
            ordinal
        };
        let variant = format!("{}.{ordinal}", step.name);
        match self.send(&variant, vec![step.instruction], &[]) {
            Ok(observation) => observation,
            Err(observation) => panic!(
                "allocation step `{}` failed: {observation:?}",
                observation.label
            ),
        }
    }

    /// Advance until the ticket's phase is `phase` (the instruction OF that
    /// phase has not run).  `state::IDLE` runs the ticket through Commit.
    pub fn advance_until_phase(&mut self, phase: u8, rows: u8) {
        let mut guard = 0u32;
        while self.ticket().phase != phase {
            assert!(
                self.ticket().phase != state::IDLE,
                "the ticket is IDLE, phase {phase} is unreachable"
            );
            self.advance_step(rows);
            guard += 1;
            assert!(guard < 100_000, "allocation driver did not converge");
        }
    }

    /// Discover, Senior, Seal, Allocate, Prune and Commit in batches of `rows`.
    pub fn advance_until_idle(&mut self, rows: u8) {
        self.advance_until_phase(state::IDLE, rows);
    }
}

// ---------------------------------------------------------------------------
// IMPL-40b helpers (additive)
// ---------------------------------------------------------------------------

impl World {
    /// Re-root this world's labels: every recorded observation (and the scope
    /// that names later ones) whose label starts with `from` now starts with
    /// `to`.  Suites whose scene builders hard-code their own root (the close
    /// `Scene` uses `financial.close.`) use this to publish rows under a
    /// different one (`financial.review.`) without touching the builder.
    pub fn relabel_root(&mut self, from: &str, to: &str) {
        let swap = |label: &mut String| {
            if let Some(rest) = label.strip_prefix(from) {
                *label = format!("{to}{rest}");
            }
        };
        swap(&mut self.scope);
        for observation in &mut self.log {
            swap(&mut observation.label);
        }
        if let Some(last) = self.last.as_mut() {
            swap(&mut last.label);
        }
    }
}
