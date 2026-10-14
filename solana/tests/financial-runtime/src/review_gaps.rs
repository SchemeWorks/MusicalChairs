//! IMPL-40b review-gap rows (plan Milestone 10, step 2).
//!
//! The two M10 review personas (`security-review.md` T1-T12, `economy-audit.md`
//! section 4) found places where the program has a control but the harness has
//! no row that would notice its removal.  Each row below closes one such gap
//! against the actual compiled SBF in LiteSVM, through the same [`World`] /
//! [`Scene`] the earlier suites use.  Labels are `financial.review.<row>` for a
//! row's headline observation and `financial.review.<row>.<variant>` for the
//! supporting ones.
//!
//! Every rejection asserts the EXACT program (or framework) error code parsed
//! from `Observation.error` and fee-only rollback of every account the World can
//! touch (`close::reject` and friends); every success asserts both cash-register
//! identities plus the partition identity (`close::ok`) and the specific state
//! deltas.
//!
//! Limits, stated where a row has one: the round boundary of the pinned-config
//! row is the harness's `advance_round_fixture` (the pool counters a
//! `finalize_round` would leave), not a played close; the random walk is an
//! independent short walk (seed `MCFUZZ40`), not an extension of the 300-step
//! `fuzz` suite, whose private step engine and assertions are untouched.

use musical_chairs_program::{
    accounts,
    allocation::{SERIES_A, SERIES_B},
    close as pclose,
    controls::SQUADS_PROGRAM_ID,
    instruction,
    positions::{self, PlanKindArg},
    state, BackerClaimed, ConfigActivated, ConfigTerms, ErrorCode, Funding, PageKind, PayoutPaid,
    PositionOpened, PromotionApplied, PromotionSkipped, TicketCommitted, ID,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::close::{
    annotate, apply_ix, apply_ix_with, apply_keys, assert_identities, build_next,
    expected_candidates, expire_accounts, expire_ok, loser, nonloser, ok, ok_g, reject,
    reject_framework, reject_g, roster_key, standard_reserve, Scene, Spec,
};
use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
use crate::reserve::{close_page_ix, fund_ops_ix, provision_kind_ix, withdraw_ops_ix};
use crate::runtime::{anchor_ix, pda, FinancialRuntime, Observation, ATTACKER_SEED, OWNER_SEED};
use crate::world::{
    allocation_owner, BackerSpec, CloseRecordArg, PayoutKind, World, Xorshift64, DAY, SOL,
};

pub const ROW_WRONG_OWNER_REFUND: &str =
    "wrong_owner_refund_is_refused_while_an_owner_remainder_is_locked";
pub const ROW_CLAIM_SUBSTITUTIONS: &str = "claim_backer_substitutions_are_refused";
pub const ROW_PAGES_REQUIRED: &str = "open_and_fund_series_a_need_their_provisioned_pages";
pub const ROW_REGISTRY_BEYOND_SNAPSHOTS: &str =
    "payout_with_registry_beyond_the_snapshot_pages_is_refused_at_runtime";
pub const ROW_PINNED_CONFIG: &str = "position_keeps_its_pinned_config_across_a_later_activation";
pub const ROW_EXITS_WHILE_PAUSED: &str = "exit_paths_work_while_paused";
pub const ROW_WRONG_OPS_CASH_AND_CONFIG: &str = "wrong_ops_cash_and_wrong_config_pda_are_refused";
pub const ROW_FUZZ_BACKER_LEDGER: &str = "fuzz_keeps_per_backer_funded_equal_to_funded_claims";
pub const ROW_INLINE_CAPPED: &str = "inline_eight_capped_rows_matches_the_chunked_ticket";
pub const ROW_DUST_LOSS: &str = "dust_loss_candidate_entitlement_cutoff";
pub const ROW_CALLER_SNAPSHOT: &str = "caller_funded_snapshot_page_is_not_closable";

/// Every row, in suite order.
pub const ROW_NAMES: [&str; 11] = [
    ROW_WRONG_OWNER_REFUND,
    ROW_CLAIM_SUBSTITUTIONS,
    ROW_PAGES_REQUIRED,
    ROW_REGISTRY_BEYOND_SNAPSHOTS,
    ROW_PINNED_CONFIG,
    ROW_EXITS_WHILE_PAUSED,
    ROW_WRONG_OPS_CASH_AND_CONFIG,
    ROW_FUZZ_BACKER_LEDGER,
    ROW_INLINE_CAPPED,
    ROW_DUST_LOSS,
    ROW_CALLER_SNAPSHOT,
];

/// The rows as functions, in the same order as [`ROW_NAMES`].
pub fn rows() -> [fn() -> Vec<Observation>; 11] {
    [
        wrong_owner_refund_is_refused_while_an_owner_remainder_is_locked,
        claim_backer_substitutions_are_refused,
        open_and_fund_series_a_need_their_provisioned_pages,
        payout_with_registry_beyond_the_snapshot_pages_is_refused_at_runtime,
        position_keeps_its_pinned_config_across_a_later_activation,
        exit_paths_work_while_paused,
        wrong_ops_cash_and_wrong_config_pda_are_refused,
        fuzz_keeps_per_backer_funded_equal_to_funded_claims,
        inline_eight_capped_rows_matches_the_chunked_ticket,
        dust_loss_candidate_entitlement_cutoff,
        caller_funded_snapshot_page_is_not_closable,
    ]
}

/// The whole suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_review_suite() -> Vec<Observation> {
    let mut observations = Vec::new();
    for (row, name) in rows().into_iter().zip(ROW_NAMES) {
        let produced = row();
        assert_headline(&produced, name);
        assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    assert_unique_labels(&observations);
    observations
}

/// A row must contain an observation labelled exactly
/// `financial.review.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = root(row);
    assert!(
        observations
            .iter()
            .any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn root(row: &str) -> String {
    format!("financial.review.{row}")
}

/// A close `Scene` builder publishes under `financial.close.`; the review rows
/// publish under `financial.review.`.
fn rebased(mut scene: Scene) -> Scene {
    scene
        .w
        .relabel_root("financial.close.", "financial.review.");
    scene
}

/// Replace the one account meta keyed `from` with `to`, keeping its flags.
fn swap_meta(instruction: &mut Instruction, from: Pubkey, to: Pubkey) {
    let hits = instruction
        .accounts
        .iter()
        .filter(|meta| meta.pubkey == from)
        .count();
    assert_eq!(hits, 1, "expected exactly one account meta keyed {from}");
    for meta in &mut instruction.accounts {
        if meta.pubkey == from {
            meta.pubkey = to;
        }
    }
}

fn owner_keypair() -> Keypair {
    Keypair::new_from_array(OWNER_SEED)
}

fn attacker_keypair() -> Keypair {
    Keypair::new_from_array(ATTACKER_SEED)
}

/// `stage_config_version` signed by the G vault (through the stand-in) with an
/// explicit `config` account.
fn stage_ix(
    rt: &FinancialRuntime,
    version: u32,
    terms: ConfigTerms,
    config: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::StageConfigVersion {
            governance_multisig: rt.g_multisig(),
            governance: rt.g_vault(),
            pool: rt.pool_address(),
            ops: rt.ops_address(),
            ops_cash: rt.ops_cash_address(),
            config,
            system_program: system_program::ID,
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::StageConfigVersion { version, terms },
    )
}

/// The currently active version's terms with a chosen `effective_round_id`.
fn terms_of_active(w: &World, effective_round_id: u64) -> ConfigTerms {
    let mut terms = ConfigTerms::from(&w.config(w.pool().active_config_version));
    terms.effective_round_id = effective_round_id;
    terms
}

fn activate_ix(rt: &FinancialRuntime, version: u32, expected_current: u32) -> Instruction {
    anchor_ix(
        accounts::ActivateConfigVersion {
            pool: rt.pool_address(),
            config: rt.config_address(version),
            ticket: rt.ticket_address(),
            admission: rt.admission_address(),
            event_authority: rt.event_authority_address(),
            program: ID,
        },
        instruction::ActivateConfigVersion {
            version,
            expected_current,
        },
    )
}

/// `floor(amount * bps / 10_000)`, the program's toll rule.
fn bps_of(amount: u64, bps: u64) -> u64 {
    u64::try_from(u128::from(amount) * u128::from(bps) / 10_000).expect("fits u64")
}

// ---------------------------------------------------------------------------
// Row 1 (security T2): a stranger's account as `owner_refund`
// ---------------------------------------------------------------------------

/// The owner opted in to the close's reserve (`max_work_contribution`), so
/// `locked_owner_remaining > 0` when the promotion becomes terminal.  Every
/// instruction that settles the reserve (`expire_promotion`, `apply_promotion`
/// and the zero-loser `build_candidates` skip) takes an `owner_refund` account
/// and, with an owner remainder, must refuse anything but the close's recorded
/// destination (`settle`'s `require_keys_eq!`, `WrongAccount` 6005).  Nothing
/// else stops a stranger who calls the instruction from redirecting the
/// owner's remainder, and no earlier suite substituted this account.
///
/// Three scenes (one per settling instruction), each: the wrong account is
/// refused 6005 fee-only with the close still `Pending` and the owner partition
/// untouched; the correct destination then settles and the remainder reaches it.
/// The owner remainder is 5,000,000 lamports above a deliverable (funded)
/// destination, so it is paid at settlement rather than deferred.
pub fn wrong_owner_refund_is_refused_while_an_owner_remainder_is_locked() -> Vec<Observation> {
    let row = ROW_WRONG_OWNER_REFUND;
    let owner_part = 5_000_000u64;
    let sponsor = standard_reserve() - owner_part;
    let mut log = Vec::new();

    // ---- expire_promotion ----------------------------------------------------
    let mut scene = rebased(Scene::new(row, "", Spec::standard().with_sponsor(sponsor)));
    scene.provision_and_seal("build_last");
    let dest = scene.dest;
    let w = &mut scene.w;
    let stranger = w.attacker().pubkey();
    assert_ne!(stranger, dest);
    let before = w.close_record(1);
    let locked_before = w.ops().locked_remaining_total;
    assert_eq!(
        before.locked_owner_remaining, owner_part,
        "the owner partition is untouched"
    );
    assert!(!before.reserve_settled);
    w.warp_past_deadline(1);
    let mut accounts = expire_accounts(w, 1);
    assert_eq!(
        accounts.owner_refund, dest,
        "the recorded destination is the closing payout's"
    );
    accounts.owner_refund = stranger;
    let wrong = anchor_ix(accounts, instruction::ExpirePromotion);
    reject(w, "", vec![wrong], &[], ErrorCode::WrongAccount);
    let after = w.close_record(1);
    assert_eq!(
        after.locked_owner_remaining, owner_part,
        "expire: the owner remainder is not released"
    );
    assert!(!after.reserve_settled && after.promotion == state::Promotion::Pending);
    assert_eq!(w.ops().locked_remaining_total, locked_before);
    let dest_before = w.lamports(dest);
    expire_ok(
        w,
        "expire_with_the_recorded_destination",
        dest,
        pclose::STAGE_UNREQUESTED,
    );
    assert_eq!(
        w.lamports(dest) - dest_before,
        owner_part,
        "expire: the remainder reached the owner"
    );
    log.extend(scene.w.take_log());

    // ---- apply_promotion -----------------------------------------------------
    let mut apply = rebased(Scene::new(
        row,
        "apply",
        Spec::standard().with_sponsor(sponsor),
    ));
    apply.through_capture();
    let dest = apply.dest;
    let w = &mut apply.w;
    let stranger = w.attacker().pubkey();
    let pre = w.close_record(1);
    assert!(
        pre.locked_owner_remaining > 0,
        "the owner partition survives the request"
    );
    assert_eq!(pre.locked_owner_remaining, owner_part);
    let keys = apply_keys(w, 1);
    assert!(keys.winner.is_some(), "a real draw selected a winner");
    let mut wrong = apply_ix_with(w, 1, keys);
    swap_meta(&mut wrong, dest, stranger);
    let locked_before = w.ops().locked_remaining_total;
    reject(
        w,
        "wrong_owner_refund",
        vec![wrong],
        &[],
        ErrorCode::WrongAccount,
    );
    let after = w.close_record(1);
    assert_eq!(
        after.locked_owner_remaining, pre.locked_owner_remaining,
        "apply: nothing was released"
    );
    assert!(!after.reserve_settled && after.promotion == state::Promotion::Pending);
    assert_eq!(
        after.spent, pre.spent,
        "apply: the refused attempt created no account"
    );
    assert_eq!(w.ops().locked_remaining_total, locked_before);
    let dest_before = w.lamports(dest);
    let honest = apply_ix(w, 1);
    ok(w, "apply", vec![honest], &[]);
    let applied = w.last_event::<PromotionApplied>();
    assert_eq!(applied.winner, keys.winner.expect("winner"));
    let post = w.close_record(1);
    assert!(post.reserve_settled);
    assert_eq!(
        (post.locked_sponsor_remaining, post.locked_owner_remaining),
        (0, 0)
    );
    // The promotion itself may spend the owner partition once the sponsor
    // partition is exhausted; what is left is what settlement pays.
    let spent_by_owner = (post.spent.b_rent_o - pre.spent.b_rent_o)
        + (post.spent.registry_rent_o - pre.spent.registry_rent_o);
    let remainder = pre.locked_owner_remaining - spent_by_owner;
    assert!(remainder > 0, "a remainder was owed at settlement");
    assert_eq!(
        w.lamports(dest) - dest_before,
        remainder,
        "apply: the remainder reached the owner"
    );
    log.extend(apply.w.take_log());

    // ---- build_candidates, zero-loser skip -----------------------------------
    let mut skip = rebased(Scene::new(
        row,
        "build",
        Spec::winners_only().with_sponsor(sponsor),
    ));
    let dest = skip.dest;
    let w = &mut skip.w;
    let stranger = w.attacker().pubkey();
    let candidates = expected_candidates(w, 1);
    assert!(
        candidates.preferred.is_empty() && candidates.fallback.is_empty(),
        "no loser: no candidates"
    );
    assert_eq!(w.close_record(1).locked_owner_remaining, owner_part);
    let mut call = 0;
    loop {
        let close = w.close_record(1);
        let remaining = close.participant_count - close.candidate_cursor;
        let rows = remaining.min(4) as u8;
        let honest = build_next(w, 1, rows);
        if u64::from(rows) < remaining {
            ok(w, &format!("build{call}"), vec![honest], &[]);
            call += 1;
            assert!(call < 100, "sealing did not converge");
            continue;
        }
        // The sealing batch settles the reserve (no candidate set to draw from).
        let mut wrong = honest.clone();
        swap_meta(&mut wrong, dest, stranger);
        let locked_before = w.ops().locked_remaining_total;
        reject(
            w,
            "wrong_owner_refund",
            vec![wrong],
            &[],
            ErrorCode::WrongAccount,
        );
        let after = w.close_record(1);
        assert!(!after.sealed, "build: the sealing batch was rolled back");
        assert_eq!(after.candidate_cursor, close.candidate_cursor);
        assert_eq!(after.locked_owner_remaining, owner_part);
        assert!(!after.reserve_settled && after.promotion == state::Promotion::Pending);
        assert_eq!(w.ops().locked_remaining_total, locked_before);
        let dest_before = w.lamports(dest);
        let sealed = ok(w, "seal", vec![honest], &[]);
        assert_eq!(
            sealed.last_event::<PromotionSkipped>().reason,
            pclose::REASON_NO_CANDIDATES
        );
        assert_eq!(
            w.lamports(dest) - dest_before,
            owner_part,
            "build: the remainder reached the owner"
        );
        let settled = w.close_record(1);
        assert!(settled.reserve_settled && settled.locked_owner_remaining == 0);
        break;
    }
    log.extend(skip.w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Row 2 (security T1): `claim_backer` account substitutions
// ---------------------------------------------------------------------------

/// `claim_backer` takes up to two optional Backer accounts and an unchecked
/// destination.  The handler pins each supplied Backer to the signer's own
/// canonical PDA for its slot's series (`check_claim_backer`, `WrongAccount`),
/// needs at least one (`InvalidState`) and refuses every protected destination
/// (`ProtectedDestination`).  The allocation suite only ever passed the honest
/// accounts.  Rejections, each fee-only with every Backer byte-identical:
///
/// * 6005: another owner's Backer, a Series B PDA in the A slot, a Series A PDA
///   in the B slot, the two swapped, and one Backer passed in both slots;
/// * 6001: both slots omitted;
/// * 6008: OpsCash, the game vault, the pool, the owner's `User`, the claimed
///   Backer itself, and an executable account (the test-only Squads stand-in,
///   which is neither the called program nor a reserved key, so the runtime
///   leaves it writable and the handler's `executable` check is what refuses).
///
/// Then the legitimate combined claim pays exactly the owner's A and B credit
/// and leaves the foreign Backer untouched.
pub fn claim_backer_substitutions_are_refused() -> Vec<Observation> {
    let row = ROW_CLAIM_SUBSTITUTIONS;
    let owner = owner_keypair();
    let owner_key = owner.pubkey();
    let attacker_key = attacker_keypair().pubkey();
    let mut w = World::with_backers(&[
        (owner_key, SERIES_A, 10_000_000_000),
        (owner_key, SERIES_B, 10_000_000_000),
        (attacker_key, SERIES_A, 10_000_000_000),
    ])
    .scoped_label(root(row));
    w.fund_backer_fixture(owner_key, SERIES_A, 2_000_000);
    w.fund_backer_fixture(owner_key, SERIES_B, 1_000_000);
    w.fund_backer_fixture(attacker_key, SERIES_A, 3_000_000);
    let a_own = w.rt.backer_address(SERIES_A, owner_key);
    let b_own = w.rt.backer_address(SERIES_B, owner_key);
    let a_foreign = w.rt.backer_address(SERIES_A, attacker_key);
    let dest = w.dest;
    assert_eq!(w.pool().funded_claims, 6_000_000);

    // ---- substituted Backer accounts --------------------------------------------
    let substitutions: [(&str, Option<Pubkey>, Option<Pubkey>, ErrorCode); 8] = [
        ("", Some(a_foreign), None, ErrorCode::WrongAccount),
        (
            "foreign_a_beside_own_b",
            Some(a_foreign),
            Some(b_own),
            ErrorCode::WrongAccount,
        ),
        (
            "foreign_in_the_b_slot",
            None,
            Some(a_foreign),
            ErrorCode::WrongAccount,
        ),
        (
            "series_b_in_the_a_slot",
            Some(b_own),
            None,
            ErrorCode::WrongAccount,
        ),
        (
            "series_a_in_the_b_slot",
            None,
            Some(a_own),
            ErrorCode::WrongAccount,
        ),
        (
            "slots_swapped",
            Some(b_own),
            Some(a_own),
            ErrorCode::WrongAccount,
        ),
        (
            "same_backer_in_both_slots",
            Some(a_own),
            Some(a_own),
            ErrorCode::WrongAccount,
        ),
        ("both_omitted", None, None, ErrorCode::InvalidState),
    ];
    for (variant, backer_a, backer_b, expected) in substitutions {
        let before = w.snapshot_all();
        let rejected = w
            .claim_backer_with(variant, &owner, backer_a, backer_b, dest)
            .unwrap_err();
        w.assert_rejected(&rejected, expected);
        w.assert_fee_only(&before, &rejected);
    }
    assert_identities(&w);

    // ---- protected destinations --------------------------------------------------
    let protected: [(&str, Pubkey); 6] = [
        ("destination_ops_cash", w.rt.ops_cash_address()),
        ("destination_game_vault", w.rt.game_vault_address()),
        ("destination_pool", w.rt.pool_address()),
        ("destination_user", w.rt.user_address(owner_key)),
        ("destination_own_backer", a_own),
        ("destination_executable", SQUADS_PROGRAM_ID),
    ];
    for (variant, destination) in protected {
        let before = w.snapshot_all();
        let rejected = w
            .claim_backer_with(variant, &owner, Some(a_own), Some(b_own), destination)
            .unwrap_err();
        w.assert_rejected(&rejected, ErrorCode::ProtectedDestination);
        w.assert_fee_only(&before, &rejected);
    }
    assert_identities(&w);

    // ---- the legitimate claim ------------------------------------------------------
    let foreign_before = w.data(a_foreign);
    let dest_before = w.lamports(dest);
    let paid = w
        .claim_backer_with("legitimate", &owner, Some(a_own), Some(b_own), dest)
        .expect("the honest claim pays");
    assert_eq!(paid, 3_000_000);
    let claimed = w.last_event::<BackerClaimed>();
    assert_eq!(
        (claimed.owner, claimed.a_part, claimed.b_part),
        (owner_key, 2_000_000, 1_000_000)
    );
    assert_eq!(w.lamports(dest) - dest_before, 3_000_000);
    assert_eq!(
        w.data(a_foreign),
        foreign_before,
        "the foreign Backer was never touched"
    );
    assert_eq!(w.backer(attacker_key, SERIES_A).funded, 3_000_000);
    assert_eq!(w.backer(owner_key, SERIES_A).funded, 0);
    assert_eq!(w.backer(owner_key, SERIES_B).funded, 0);
    assert_eq!(
        w.pool().funded_claims,
        3_000_000,
        "only the foreign credit remains"
    );
    w.assert_vault_identity();
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 3 (security T3): the participant / registry pages
// ---------------------------------------------------------------------------

/// `open_position` reads (and appends to) the round's participant page at
/// `participant_count / 8` for the owner's first position of the round, and
/// `fund_series_a` the registry tail page at `registry_len / 8` for a row it
/// appends.  Both pages are caller-supplied and unchecked by Anchor; the
/// handlers re-derive them: the wrong key is `WrongAccount` (6005), the right
/// key with nothing provisioned there is `PageNotProvisioned` (6035).  Only the
/// first page of each list is ever provisioned in the earlier rows, so the page
/// boundary (a full page 0) was never crossed.  The fixture sets the count to
/// 8, so the next row belongs on page 1, which does not exist; each refusal is
/// fee-only, and once page 1 is provisioned the same call succeeds.
pub fn open_and_fund_series_a_need_their_provisioned_pages() -> Vec<Observation> {
    let row = ROW_PAGES_REQUIRED;
    let mut w = World::genesis().scoped_label(root(row));
    let attacker = attacker_keypair();
    let attacker_key = attacker.pubkey();

    // ---- open_position: participant page 1 -----------------------------------------
    w.set_pool(|pool| pool.participant_count = 8);
    let round = w.pool().round_id;
    let page1 = w.rt.participant_address(round, 1);
    let page0 = w.rt.participant_address(round, 0);
    assert!(
        w.account(page1).is_none(),
        "participant page 1 does not exist"
    );
    let open = w.open_instruction(attacker_key, PlanKindArg::Simple, 10_000_000, 1, round, 1);
    reject(
        &mut w,
        "",
        vec![open],
        &[&attacker],
        ErrorCode::PageNotProvisioned,
    );
    let mut wrong = w.open_instruction(attacker_key, PlanKindArg::Simple, 10_000_000, 1, round, 1);
    swap_meta(&mut wrong, page1, page0);
    reject(
        &mut w,
        "open_wrong_page",
        vec![wrong],
        &[&attacker],
        ErrorCode::WrongAccount,
    );
    assert_eq!(w.pool().participant_count, 8, "nothing was appended");
    w.rt.install_participant_page(round, 1);
    w.open_position(
        "open_with_the_page",
        &attacker,
        PlanKindArg::Simple,
        10_000_000,
        1,
    )
    .expect("the same open succeeds once page 1 exists");
    assert_eq!(w.pool().participant_count, 9);
    let page = w.participants(round, 1);
    assert_eq!(
        (page.len, page.rows[0]),
        (1, attacker_key),
        "the owner landed on page 1"
    );

    // ---- fund_series_a: registry page 1 ----------------------------------------------
    w.set_pool(|pool| pool.registry_len = 8);
    let registry1 = w.rt.registry_address(1);
    let registry0 = w.rt.registry_address(0);
    assert!(
        w.account(registry1).is_none(),
        "registry page 1 does not exist"
    );
    let fund = w.fund_series_a_instruction(attacker_key, 50_000_000, round);
    reject(
        &mut w,
        "fund_series_a",
        vec![fund],
        &[&attacker],
        ErrorCode::PageNotProvisioned,
    );
    let mut wrong = w.fund_series_a_instruction(attacker_key, 50_000_000, round);
    swap_meta(&mut wrong, registry1, registry0);
    reject(
        &mut w,
        "fund_series_a_wrong_page",
        vec![wrong],
        &[&attacker],
        ErrorCode::WrongAccount,
    );
    assert_eq!(w.pool().registry_len, 8, "nothing was appended");
    w.rt.replace_registry(state::RegistryPage {
        bump: pda(&[state::REGISTRY_SEED, &1u64.to_le_bytes()]).1,
        index: 1,
        ..state::RegistryPage::default()
    });
    w.fund_series_a("fund_series_a_with_the_page", &attacker, 50_000_000, round)
        .expect("the same funding succeeds once registry page 1 exists");
    assert_eq!(w.pool().registry_len, 9);
    assert_eq!(
        w.registry(1).keys[0],
        w.rt.backer_address(SERIES_A, attacker_key)
    );
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 4 (security T4): registry beyond the snapshot pages
// ---------------------------------------------------------------------------

/// A non-closing payout whose toll routes to an allocation ticket must find a
/// provisioned snapshot slot for every captured registry row
/// (`snapshots_cover`, `PageNotProvisioned` 6035), otherwise it would start a
/// ticket that can never finish.  The harness's own fixture writers assert that
/// coverage, so the program's refusal was only unit-tested.  Here the fixture
/// writes `registry_len` past `snapshot_pages * 8` (only a fixture can: the
/// program never lets the registry outgrow its snapshot pages), and the payout
/// is refused fee-only with no ticket, no escrow and the position unclaimed.
/// An exactly covered registry (8 rows, one page) is accepted.
pub fn payout_with_registry_beyond_the_snapshot_pages_is_refused_at_runtime() -> Vec<Observation> {
    let row = ROW_REGISTRY_BEYOND_SNAPSHOTS;
    let mut w =
        World::genesis_with_simple_position_and_one_open_backer(SOL).scoped_label(root(row));
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.warp_seconds(DAY);
    assert_eq!((w.pool().registry_len, w.pool().snapshot_pages), (1, 1));
    let payout = |w: &World| {
        w.payout_instruction(
            PayoutKind::WithdrawSimple,
            owner_key,
            owner_key,
            1,
            0,
            0,
            dest,
            CloseRecordArg::Auto,
        )
    };

    // Nine rows, one snapshot page of eight slots.
    w.set_pool(|pool| pool.registry_len = 9);
    let ix = payout(&w);
    reject(
        &mut w,
        "",
        vec![ix],
        &[&owner],
        ErrorCode::PageNotProvisioned,
    );
    // No snapshot page at all.
    w.set_pool(|pool| {
        pool.registry_len = 1;
        pool.snapshot_pages = 0;
    });
    let ix = payout(&w);
    reject(
        &mut w,
        "no_snapshot_pages",
        vec![ix],
        &[&owner],
        ErrorCode::PageNotProvisioned,
    );
    let pool = w.pool();
    assert_eq!(
        (pool.active_ticket, pool.pending_toll, pool.next_ticket),
        (0, 0, 1)
    );
    assert_eq!(w.ticket().phase, state::IDLE, "no ticket was started");
    assert!(!w.position(owner_key, 1).claimed);

    // Eight rows exactly fill one snapshot page: accepted, the ticket starts.
    w.set_pool(|pool| {
        pool.registry_len = 8;
        pool.snapshot_pages = 1;
    });
    let ix = payout(&w);
    ok(&mut w, "covered_exactly", vec![ix], &[&owner]);
    let paid = w.last_event::<PayoutPaid>();
    assert_eq!((paid.toll, paid.ticket_id), (13_200_000, 1));
    assert_eq!(w.ticket().captured_len, 8);
    assert_eq!(w.pool().pending_toll, 13_200_000);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 5 (security T5): a position keeps the config it opened under
// ---------------------------------------------------------------------------

/// A position pins `config_version` at open and every payout reads the config
/// seeded from that pin (`Payout.config` / `CloseSimplePosition.config`), never
/// the pool's active one.  Nothing outside the M3 controls rows even staged or
/// activated a version, so this was untested end to end.
///
/// Version 2 is staged for round 2 with the canonical economics except a 5%
/// Front-End Load and a 10% / 14% Jackpot Fee (v1: 4% and 9% / 13%), so a
/// position priced under either is distinguishable by `PositionOpened.load`
/// and `PayoutPaid.toll`.  The pool then reaches round 2 (the harness's
/// `advance_round_fixture`, the counters a `finalize_round` leaves; the round
/// boundary is a fixture, not a played close), two Compound15 positions open
/// under v1, v2 is activated (permissionless, nothing in flight), and:
///
/// * `open_position` naming `expected_config = 1` is `StaleConfig` (6037)
///   fee-only; naming 2 opens a position pinned to v2 and priced at 5%;
/// * the v1 position refuses the v2 config account (`ConstraintSeeds`, 2006);
/// * at maturity the v1 position pays its Jackpot Fee at 900 bps
///   (402,620,918 of 4,473,565,759), the v2 position at 1,000 bps;
/// * a v1 position's CLOSING payout binds `CloseRecord.config_version == 1`
///   although v2 is the active version.
pub fn position_keeps_its_pinned_config_across_a_later_activation() -> Vec<Observation> {
    let row = ROW_PINNED_CONFIG;
    let mut w = World::genesis().scoped_label(root(row));
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;

    // Stage v2 (round 1 is current, so effective round 2 is in the future).
    let mut v2 = terms_of_active(&w, 2);
    v2.front_end_load_bps = 500;
    v2.jackpot_bps = [1_000, 1_400];
    w.set_ops_free(20_000_000);
    let stage = stage_ix(&w.rt, 2, v2, w.rt.config_address(2));
    ok_g(&mut w, "stage_v2", stage);
    let staged = w.config(2);
    assert_eq!(
        ConfigTerms::from(&staged),
        v2,
        "the stored terms are exactly the staged terms"
    );
    assert_eq!(staged.activated_at, None);
    assert_eq!(w.pool().active_config_version, 1);

    // The round boundary (fixture), then two positions under v1.
    w.advance_round_fixture();
    assert_eq!(w.pool().round_id, 2);
    for nonce in [1u64, 2] {
        w.open_position(
            &format!("open_v1_nonce{nonce}"),
            &owner,
            PlanKindArg::Compound15,
            SOL,
            nonce,
        )
        .expect("open under v1");
        let opened = w.last_event::<PositionOpened>();
        assert_eq!(
            (opened.config_version, opened.load),
            (1, 40_000_000),
            "v1 prices the load at 4%"
        );
        let position = w.position(owner_key, nonce);
        assert_eq!((position.config_version, position.round_id), (1, 2));
    }

    // Activation: permissionless, idle, round 2 has reached the effective round.
    let activate = activate_ix(&w.rt, 2, 1);
    let activated = ok(&mut w, "activate_v2", vec![activate], &[]);
    assert_eq!(activated.last_event::<ConfigActivated>().version, 2);
    assert_eq!(w.pool().active_config_version, 2);
    assert!(w.config(2).activated_at.is_some());
    assert_eq!(
        w.position(owner_key, 1).config_version,
        1,
        "activation never rewrites a position"
    );
    assert_eq!(w.position(owner_key, 2).config_version, 1);

    // A new open must name the new active version.
    let stale = w.open_instruction(owner_key, PlanKindArg::Compound15, SOL, 3, 2, 1);
    reject(
        &mut w,
        "stale_open_expected_config_1",
        vec![stale],
        &[&owner],
        ErrorCode::StaleConfig,
    );
    w.open_position("open_v2", &owner, PlanKindArg::Compound15, SOL, 3)
        .expect("open under v2");
    let opened = w.last_event::<PositionOpened>();
    assert_eq!(
        (opened.config_version, opened.load),
        (2, 50_000_000),
        "v2 prices the load at 5%"
    );
    assert_eq!(w.position(owner_key, 3).config_version, 2);

    // Maturity: all three positions are 15 days old; the pot pays them solvently.
    w.set_pot_fixture(30 * SOL);
    w.warp_seconds(15 * DAY);

    // The v1 position cannot be settled against the v2 config account.
    let mut wrong = w.payout_instruction(
        PayoutKind::SettleCompounding,
        owner_key,
        owner_key,
        1,
        0,
        0,
        dest,
        CloseRecordArg::Auto,
    );
    swap_meta(&mut wrong, w.rt.config_address(1), w.rt.config_address(2));
    reject_framework(
        &mut w,
        "v1_position_with_the_v2_config",
        vec![wrong],
        &[&owner],
        2006,
    );

    // The v1 position settles at v1's Jackpot Fee (the positions suite's 900 bps vector).
    w.settle_compounding("", &owner, 1, 0, dest, 0)
        .expect("settle the v1 position");
    let paid = w.last_event::<PayoutPaid>();
    assert_eq!(paid.gross_paid, 4_473_565_759);
    assert_eq!(paid.toll, 402_620_918, "v1: 900 bps");
    assert_eq!(paid.toll, bps_of(paid.gross_paid, 900));
    assert!(!paid.closing);

    // The v2 position settles at v2's.
    w.settle_compounding("settle_v2_position", &owner, 3, 0, dest, 0)
        .expect("settle the v2 position");
    let paid = w.last_event::<PayoutPaid>();
    assert_eq!(paid.gross_paid, 4_473_565_759);
    assert_eq!(paid.toll, bps_of(paid.gross_paid, 1_000), "v2: 1,000 bps");
    assert_ne!(paid.toll, bps_of(paid.gross_paid, 900));

    // The other v1 position settles INSOLVENTLY (one lamport short): the closing
    // payout binds its own pinned version into the CloseRecord.
    w.provision_close_record(1);
    let reserve = w.expected_reserve(1, 0, 0, 0);
    w.set_ops_free(reserve);
    w.set_pot_fixture(4_473_565_758);
    w.settle_compounding("closing_payout_v1_position", &owner, 2, 0, dest, 0)
        .expect("a one-lamport-short v1 settlement closes the round");
    let closing = w.last_event::<PayoutPaid>();
    assert!(closing.closing && closing.close_id == Some(1));
    assert_eq!(closing.toll, bps_of(closing.gross_paid, 900), "v1: 900 bps");
    let record = w.close_record(1);
    assert_eq!((record.config_version, record.round_id), (1, 2));
    assert_eq!(
        w.pool().active_config_version,
        2,
        "v2 is still the active version"
    );
    assert_identities(&w);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 6 (security T6): exits are not admissions
// ---------------------------------------------------------------------------

/// The admission pause stops new money in (`open_position`, `fund_series_a`:
/// `AdmissionPaused` 6009) and must never trap money already in: the payouts,
/// the Backer claim and `close_simple_position` do not consult `Admission`.
/// Nothing asserted that.  With `paused = true` for the whole row: both
/// admissions are refused fee-only; then a Simple withdrawal, a matured
/// Compound15 settlement, a `close_simple_position` and a `claim_backer` all
/// succeed; unpaused, the very same admission works.
pub fn exit_paths_work_while_paused() -> Vec<Observation> {
    let row = ROW_EXITS_WHILE_PAUSED;
    let mut w = World::genesis_with_simple_position(SOL).scoped_label(root(row));
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    w.open_position(
        "setup_open_compound15",
        &owner,
        PlanKindArg::Compound15,
        100_000_000,
        2,
    )
    .expect("a Compound15 position");
    // A third Simple position that will be fully paid up to its maturity, so
    // `close_simple_position` has a legitimate success.
    w.open_fixture_position(owner_key, 3, PlanKindArg::Simple, SOL);
    w.set_pot_fixture(10 * SOL);
    // A capped Series A Backer with finalized, unclaimed credit.
    w.rt.replace_backer(state::Backer {
        bump: pda(&[state::BACKER_SEED, &[SERIES_A], owner_key.as_ref()]).1,
        owner: owner_key,
        series: SERIES_A,
        amount: 1_000_000,
        entitlement: 2_000_000,
        first_sequence: 1,
        active: false,
        ..state::Backer::default()
    });
    w.watch(w.rt.backer_address(SERIES_A, owner_key));
    w.fund_backer_fixture(owner_key, SERIES_A, 2_000_000);

    // ---- pause: admissions refused -------------------------------------------------
    w.set_paused(true);
    assert!(w.admission().paused);
    let open = w.open_instruction(owner_key, PlanKindArg::Simple, 10_000_000, 4, 1, 1);
    reject(
        &mut w,
        "open_position_paused",
        vec![open],
        &[&owner],
        ErrorCode::AdmissionPaused,
    );
    let fund = w.fund_series_a_instruction(owner_key, 50_000_000, 1);
    reject(
        &mut w,
        "fund_series_a_paused",
        vec![fund],
        &[&owner],
        ErrorCode::AdmissionPaused,
    );

    // ---- pause: exits keep working ------------------------------------------------------
    w.warp_seconds(15 * DAY);
    w.withdraw_simple("", &owner, 1, 0, dest, 0)
        .expect("withdraw_simple while paused");
    let paid = w.last_event::<PayoutPaid>();
    assert!(paid.net > 0 && !paid.closing);
    assert!(w.admission().paused);

    w.settle_compounding("settle_compounding", &owner, 2, 0, dest, 0)
        .expect("a matured settlement while paused");
    assert!(w.position(owner_key, 2).claimed);

    w.warp_seconds(6 * DAY);
    let mut paid_up = w.position(owner_key, 3);
    paid_up.accrued_gross = 2_310_000_000;
    paid_up.paid_gross = 2_310_000_000;
    paid_up.net_payout = 2_240_700_000;
    paid_up.last_accrued_at = paid_up.maturity_at;
    w.rt.replace_position(paid_up);
    w.close_simple_position("close_simple_position", &owner, 3)
        .expect("close_simple_position while paused");
    assert!(w.position(owner_key, 3).claimed);

    let claimed = w
        .claim_backer("claim_backer", &owner, dest)
        .expect("claim_backer while paused");
    assert_eq!(claimed, 2_000_000);
    assert!(w.admission().paused, "the pause held throughout");

    // ---- unpaused: the very same admission works ----------------------------------------
    w.set_paused(false);
    w.open_position(
        "open_position_unpaused",
        &owner,
        PlanKindArg::Simple,
        10_000_000,
        4,
    )
    .expect("the same open once the pause is lifted");
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 7 (security T7): wrong OpsCash, wrong ConfigVersion PDA
// ---------------------------------------------------------------------------

/// `fund_operations` / `withdraw_operations` validate the `ops_cash` account
/// (`validate_ops_cash`: the canonical PDA first, `WrongAccount` 6005) and
/// `stage_config_version` its `config` account (`pda_bump`, `WrongAccount`);
/// the M3 rows only passed the honest ones.  Each refusal is fee-only; the same
/// instructions with the right accounts then succeed.
pub fn wrong_ops_cash_and_wrong_config_pda_are_refused() -> Vec<Observation> {
    let row = ROW_WRONG_OPS_CASH_AND_CONFIG;
    let mut w = World::genesis().scoped_label(root(row));
    let owner = w.owner();
    let sponsor = w.sponsor();
    let attacker = w.attacker();
    let real_cash = w.rt.ops_cash_address();

    // ---- fund_operations -------------------------------------------------------------
    let impostors: [(&str, Pubkey); 3] = [
        ("random_key", Pubkey::new_from_array([0x7C; 32])),
        ("game_vault", w.rt.game_vault_address()),
        ("funded_wallet", attacker.pubkey()),
    ];
    for (name, impostor) in impostors {
        let variant = if name == "random_key" {
            String::new()
        } else {
            format!("fund_operations_{name}")
        };
        let mut fund = fund_ops_ix(&w, owner.pubkey(), 1_000_000);
        swap_meta(&mut fund, real_cash, impostor);
        reject(
            &mut w,
            &variant,
            vec![fund],
            &[&owner],
            ErrorCode::WrongAccount,
        );
    }
    // ---- withdraw_operations ---------------------------------------------------------
    for (name, impostor) in impostors {
        let mut withdraw = withdraw_ops_ix(&w, sponsor.pubkey(), 1);
        swap_meta(&mut withdraw, real_cash, impostor);
        reject(
            &mut w,
            &format!("withdraw_operations_{name}"),
            vec![withdraw],
            &[&sponsor],
            ErrorCode::WrongAccount,
        );
    }
    // ---- stage_config_version --------------------------------------------------------
    let terms = terms_of_active(&w, 2);
    let wrong_configs: [(&str, Pubkey); 2] = [
        ("stage_config_v3_pda", w.rt.config_address(3)),
        ("stage_config_existing_v1", w.rt.config_address(1)),
    ];
    for (name, wrong) in wrong_configs {
        let stage = stage_ix(&w.rt, 2, terms, wrong);
        reject_g(&mut w, name, stage, ErrorCode::WrongAccount);
    }
    assert!(
        w.account(w.rt.config_address(2)).is_none(),
        "no version 2 was created"
    );
    assert!(w.account(w.rt.config_address(3)).is_none());

    // ---- the same instructions with the right accounts --------------------------------
    let fund = fund_ops_ix(&w, owner.pubkey(), 10_000_000);
    ok(&mut w, "fund_operations_right_cash", vec![fund], &[&owner]);
    assert_eq!(w.ops().free_balance, 10_000_000);
    let withdraw = withdraw_ops_ix(&w, sponsor.pubkey(), 1_000_000);
    ok(
        &mut w,
        "withdraw_operations_right_cash",
        vec![withdraw],
        &[&sponsor],
    );
    assert_eq!(w.ops().free_balance, 9_000_000);
    let stage = stage_ix(&w.rt, 2, terms, w.rt.config_address(2));
    ok_g(&mut w, "stage_config_right_pda", stage);
    assert_eq!(w.config(2).version, 2);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row 8 (economy T5): the per-Backer ledger sums to the global claims
// ---------------------------------------------------------------------------

/// "MCFUZZ40".
pub const FUZZ_SEED: u64 = 0x4D43_4655_5A5A_3430;
/// Random steps after the scripted prefix.
pub const FUZZ_STEPS: usize = 120;
const FUZZ_PLAYERS: usize = 6;
/// Pot floor the walk restores before a payout, so no payout is insolvent (a
/// closing payout needs the close machinery this walk does not drive).
const FUZZ_POT: u64 = 40 * SOL;

struct OpenPos {
    player: usize,
    nonce: u64,
    plan: PlanKindArg,
}

#[derive(Default)]
struct WalkStats {
    ok: u64,
    rejected: u64,
    fundings: u64,
    payouts_with_toll: u64,
    claims_paid: u64,
}

struct Walk {
    w: World,
    rng: Xorshift64,
    players: Vec<Keypair>,
    positions: Vec<OpenPos>,
    next_nonce: u64,
    step: usize,
    history: Vec<String>,
    stats: WalkStats,
}

impl Walk {
    fn new() -> Self {
        let mut w = World::genesis().scoped_label(root(ROW_FUZZ_BACKER_LEDGER));
        let mut players = vec![w.owner(), w.attacker()];
        for index in 0..4u8 {
            players.push(Keypair::new_from_array([0x91 + index; 32]));
        }
        assert_eq!(players.len(), FUZZ_PLAYERS);
        for player in &players[2..] {
            w.rt.credit_lamports(player.pubkey(), 100 * SOL);
        }
        // Two Series B fixture Backers (players 4 and 5) from the start: one
        // large and one a few lamports from its cap, so the walk exercises the
        // B pool, caps and prunes as well as the Series A rows it funds.
        let b_large = BackerSpec::new(players[4].pubkey(), SERIES_B, 400_000_000);
        let b_small = BackerSpec::new(players[5].pubkey(), SERIES_B, 3_000);
        w.install_backer_specs(&[b_large, b_small]);
        w.install_user(players[4].pubkey(), false, true);
        w.install_user(players[5].pubkey(), false, true);
        Self {
            w,
            rng: Xorshift64::new(FUZZ_SEED),
            players,
            positions: Vec::new(),
            next_nonce: 10,
            step: 0,
            history: Vec::new(),
            stats: WalkStats::default(),
        }
    }

    fn variant(&self, what: &str) -> String {
        format!("step{:03}.{what}", self.step)
    }

    fn pick(&mut self, n: usize) -> usize {
        (self.rng.next() % n as u64) as usize
    }

    /// Record `outcome` in the history and the counters.
    fn note(&mut self, what: &str, outcome: &Result<Observation, Observation>) {
        let line = match outcome {
            Ok(_) => {
                self.stats.ok += 1;
                format!("step {:03} {what} -> ok", self.step)
            }
            Err(observation) => {
                self.stats.rejected += 1;
                format!(
                    "step {:03} {what} -> REJECT {:?}",
                    self.step,
                    crate::controls::custom_code(observation)
                )
            }
        };
        self.history.push(line);
    }

    /// The invariant under test, plus the vault identity: the sum over EVERY
    /// Backer account of `funded` and of the pending credit its committed
    /// ticket already counted equals `Pool.funded_claims`; the pending credit
    /// of the ACTIVE ticket equals what the ticket has credited so far.
    fn check(&self) {
        let pool = self.w.pool();
        let ticket = self.w.ticket();
        let mut counted: u128 = 0;
        let mut active_pending: u128 = 0;
        for player in &self.players {
            for series in [SERIES_A, SERIES_B] {
                let key = self.w.rt.backer_address(series, player.pubkey());
                let Some(backer) = self.w.rt.try_deserialize::<state::Backer>(key) else {
                    continue;
                };
                assert!(
                    backer.lifetime <= backer.entitlement,
                    "a Backer never exceeds its cap"
                );
                counted += u128::from(backer.funded);
                if backer.pending_ticket != 0 {
                    if backer.pending_ticket <= pool.finalized_ticket {
                        counted += u128::from(backer.pending);
                    } else {
                        assert_eq!(
                            backer.pending_ticket, pool.active_ticket,
                            "pending credit belongs to a finalized or the active ticket"
                        );
                        active_pending += u128::from(backer.pending);
                    }
                } else {
                    assert_eq!(backer.pending, 0, "untagged pending credit");
                }
            }
        }
        assert_eq!(
            counted, pool.funded_claims,
            "sum of Backer funded + finalized pending must equal Pool.funded_claims"
        );
        if pool.active_ticket != 0 {
            assert_eq!(
                active_pending,
                u128::from(ticket.credited),
                "the active ticket's pending credit equals what it has credited"
            );
        } else {
            assert_eq!(
                active_pending, 0,
                "no pending credit without an active ticket"
            );
        }
        self.w.assert_vault_identity();
        self.w.assert_ops_cash_identity();
    }

    fn fund(
        &mut self,
        player: usize,
        gross: u64,
        variant: &str,
    ) -> Result<Observation, Observation> {
        let round = self.w.pool().round_id;
        let outcome = self
            .w
            .fund_series_a(variant, &self.players[player], gross, round);
        if outcome.is_ok() {
            self.stats.fundings += 1;
        }
        outcome
    }

    fn open(
        &mut self,
        player: usize,
        plan: PlanKindArg,
        gross: u64,
        variant: &str,
    ) -> Result<Observation, Observation> {
        let nonce = self.next_nonce;
        self.next_nonce += 1;
        let outcome = self
            .w
            .open_position(variant, &self.players[player], plan, gross, nonce);
        if outcome.is_ok() {
            self.positions.push(OpenPos {
                player,
                nonce,
                plan,
            });
        }
        outcome
    }

    fn payout(&mut self, index: usize, variant: &str) -> Result<Observation, Observation> {
        if self.w.pool().pot < FUZZ_POT {
            self.w.set_pot_fixture(FUZZ_POT);
        }
        let (player, nonce, plan) = {
            let position = &self.positions[index];
            (position.player, position.nonce, position.plan)
        };
        let kind = if plan == PlanKindArg::Simple {
            PayoutKind::WithdrawSimple
        } else {
            PayoutKind::SettleCompounding
        };
        let dest = self.w.dest;
        let outcome = self.w.payout(
            variant,
            kind,
            &self.players[player],
            nonce,
            0,
            dest,
            0,
            CloseRecordArg::Auto,
        );
        if let Ok(observation) = &outcome {
            if observation.last_event::<PayoutPaid>().toll > 0 {
                self.stats.payouts_with_toll += 1;
            }
            let owner = self.players[player].pubkey();
            if self.w.position(owner, nonce).claimed {
                self.positions.remove(index);
            }
        }
        outcome
    }

    fn claim(&mut self, player: usize, variant: &str) -> Result<u64, Observation> {
        let outcome = self
            .w
            .claim_backer(variant, &self.players[player], self.w.dest);
        if let Ok(paid) = &outcome {
            if *paid > 0 {
                self.stats.claims_paid += 1;
            }
        }
        outcome
    }

    fn has_backer(&self, player: usize) -> bool {
        let owner = self.players[player].pubkey();
        [SERIES_A, SERIES_B].iter().any(|&series| {
            self.w
                .program_account(self.w.rt.backer_address(series, owner))
                .is_some()
        })
    }

    /// The scripted prefix: guarantees the coverage the random part cannot be
    /// trusted to reach (a funding, a position, a toll that starts a ticket, a
    /// committed ticket and a paid claim), each step checked.
    fn prefix(&mut self) {
        self.check();
        let first = self.fund(2, 100_000_000, "");
        first.expect("prefix: fund Series A (player 2)");
        self.check();
        self.fund(3, 50_000_000, "prefix.fund_series_a_player3")
            .expect("prefix: fund Series A (player 3)");
        self.check();
        self.open(1, PlanKindArg::Simple, SOL, "prefix.open_simple")
            .expect("prefix: open a Simple position");
        self.check();
        self.w.warp_seconds(4 * DAY);
        let toll = self.payout(0, "prefix.withdraw_simple");
        let observation = toll.expect("prefix: withdraw (the toll starts a ticket)");
        assert!(
            observation.last_event::<PayoutPaid>().ticket_id != 0,
            "a ticket was started"
        );
        self.check();
        while self.w.pool().active_ticket != 0 {
            self.w.advance_step(3);
            self.check();
        }
        assert!(self.w.pool().finalized_ticket >= 1, "a ticket committed");
        let paid = self
            .claim(2, "prefix.claim_backer")
            .expect("prefix: player 2 claims");
        assert!(paid > 0, "the claim paid");
        self.check();
    }

    fn random_step(&mut self) {
        // A running ticket is driven forward most of the time so tickets end.
        if self.w.pool().active_ticket != 0 && self.rng.next() % 100 < 60 {
            let rows = [1u8, 2, 3, 5, 8][self.pick(5)];
            self.w.advance_step(rows);
            self.history
                .push(format!("step {:03} advance_step({rows})", self.step));
            self.stats.ok += 1;
            return;
        }
        match self.rng.next() % 100 {
            0..=22 => {
                let player = self.pick(4);
                let gross = 50_000_000 * (1 + self.rng.next() % 6);
                let variant = self.variant("fund_series_a");
                let outcome = self.fund(player, gross, &variant);
                self.note("fund_series_a", &outcome);
            }
            23..=42 => {
                let player = self.pick(FUZZ_PLAYERS);
                let plan = if self.rng.next().is_multiple_of(2) {
                    PlanKindArg::Simple
                } else {
                    PlanKindArg::Compound15
                };
                let gross = 10_000_000 * (1 + self.rng.next() % 40);
                let variant = self.variant("open_position");
                let outcome = self.open(player, plan, gross, &variant);
                self.note("open_position", &outcome);
            }
            43..=64 => {
                if self.positions.is_empty() {
                    self.history
                        .push(format!("step {:03} payout (no open position)", self.step));
                    return;
                }
                let index = self.pick(self.positions.len());
                let variant = self.variant("payout");
                let outcome = self.payout(index, &variant);
                self.note("payout", &outcome);
            }
            65..=82 => {
                let player = self.pick(FUZZ_PLAYERS);
                if !self.has_backer(player) {
                    self.history.push(format!(
                        "step {:03} claim (player {player} has no Backer)",
                        self.step
                    ));
                    return;
                }
                let variant = self.variant("claim_backer");
                let outcome = self.claim(player, &variant);
                let recorded: Result<Observation, Observation> = match outcome {
                    Ok(_) => Ok(self.w.last().clone()),
                    Err(observation) => Err(observation),
                };
                self.note("claim_backer", &recorded);
            }
            _ => {
                let seconds = if self.rng.next().is_multiple_of(3) {
                    3_600
                } else {
                    DAY * (1 + self.rng.next() % 6)
                };
                self.w.warp_seconds(seconds);
                self.history
                    .push(format!("step {:03} warp {seconds} s", self.step));
            }
        }
    }

    fn run(&mut self) {
        let prefix = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.prefix()));
        if let Err(payload) = prefix {
            panic!(
                "review fuzz prefix FAILED (seed {FUZZ_SEED:#018x})\n  {}\n  --- history ---\n{}",
                panic_text(payload.as_ref()),
                self.history.join("\n")
            );
        }
        for step in 0..FUZZ_STEPS {
            self.step = step;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.random_step();
                self.check();
            }));
            if let Err(payload) = result {
                panic!(
                    "review fuzz FAILED\n  seed {FUZZ_SEED:#018x}\n  at step {step}\n  {}\n  --- history ({} lines) ---\n{}",
                    panic_text(payload.as_ref()),
                    self.history.len(),
                    self.history.join("\n")
                );
            }
        }
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        "a non-string panic".to_string()
    }
}

/// An independent short randomized walk (seed `MCFUZZ40`, 120 random steps
/// after a scripted prefix) over the allocation money path -- Series A funding,
/// Simple and Compound positions, payouts whose tolls start tickets, ticket
/// phases in random batch sizes, claims, clock warps -- asserting after EVERY
/// step that the per-Backer ledger sums to the global one:
///
/// ```text
/// sum over all Backer accounts of
///     funded + (pending if its ticket is already finalized)  ==  Pool.funded_claims
/// sum of pending credit tagged with the active ticket         ==  Ticket.credited
/// ```
///
/// (`funded` alone equals the pool figure only once every committed credit has
/// been materialized by a claim or a later credit; the finalized-pending term
/// is the exact statement.)  It is a separate walk because the 300-step `fuzz`
/// suite's step engine and `assert_books` are private to it and this row must
/// not touch that suite's assertions; closes and promotions are not driven
/// here (the pot is kept solvent), the 300-step suite covers those.
pub fn fuzz_keeps_per_backer_funded_equal_to_funded_claims() -> Vec<Observation> {
    let mut walk = Walk::new();
    walk.run();
    let stats = &walk.stats;
    let pool = walk.w.pool();
    assert!(pool.finalized_ticket >= 1, "at least one ticket committed");
    assert!(stats.fundings >= 2 && stats.claims_paid >= 1 && stats.payouts_with_toll >= 1);
    assert!(
        stats.ok >= 10,
        "the random part executed real transactions ({} ok)",
        stats.ok
    );
    let note = format!(
        "seed {FUZZ_SEED:#018x} (MCFUZZ40); a scripted prefix + {FUZZ_STEPS} random steps; ok {} / rejected {}; fundings {}; payouts with a toll {}; claims paid {}; tickets committed {}",
        stats.ok,
        stats.rejected,
        stats.fundings,
        stats.payouts_with_toll,
        stats.claims_paid,
        pool.finalized_ticket
    );
    let mut log = walk.w.take_log();
    annotate(&mut log, &root(ROW_FUZZ_BACKER_LEDGER), note);
    log
}

// ---------------------------------------------------------------------------
// Row 9 (economy T6): inline allocation when every row is capped and pruned
// ---------------------------------------------------------------------------

/// Eight fixture Backers (four Series A, four Series B), each with 20 lamports
/// of remaining entitlement: open (above the 10-lamport cutoff) but capped by
/// the first credit, then pruned.
fn capped_specs() -> Vec<BackerSpec> {
    (0..8u64)
        .map(|index| {
            let series = if index % 2 == 0 { SERIES_A } else { SERIES_B };
            BackerSpec::new(allocation_owner(index), series, 20)
        })
        .collect()
}

fn capped_world(scope: String) -> World {
    let mut w = World::genesis_with_simple_position(SOL).scoped_label(scope);
    w.install_backer_specs(&capped_specs());
    w.warp_seconds(DAY);
    w
}

/// Everything the two routes must leave identical.
struct Books {
    pool: (u64, u128, u64, u64, u64),
    backers: Vec<Vec<u8>>,
    registry: Vec<u8>,
    snapshot: Vec<u8>,
    ticket: Vec<u8>,
    digest: [u8; 32],
}

fn books(w: &World) -> Books {
    let pool = w.pool();
    Books {
        pool: (
            pool.seed,
            pool.funded_claims,
            pool.pending_toll,
            pool.next_ticket,
            pool.registry_len,
        ),
        backers: capped_specs()
            .iter()
            .map(|spec| w.data(w.rt.backer_address(spec.series, spec.owner)))
            .collect(),
        registry: w.data(w.rt.registry_address(0)),
        snapshot: w.data(w.rt.snapshot_address(0)),
        ticket: w.data(w.rt.ticket_address()),
        digest: w.digest_financial_state(),
    }
}

/// The inline allocation (remaining accounts `[Backer slot 0..7, RegistryPage
/// 0, SnapshotPage 0]`, all writable) runs the whole ticket inside a
/// non-closing payout and must leave exactly the chunked ticket's state.  The
/// 1..=8 byte-identity proof used rows that are partly credited; this is the
/// extreme: with a 13,200,000-lamport toll every one of the eight rows is
/// capped at 20 lamports and swap-deleted in the same page, which exercises
/// the inline prune chain (every moved row is itself pruned) end to end.
/// Recorded: the inline transaction's compute, packet bytes, keys and writable
/// accounts (the headline observation carries them).
pub fn inline_eight_capped_rows_matches_the_chunked_ticket() -> Vec<Observation> {
    let row = ROW_INLINE_CAPPED;
    let specs = capped_specs();

    // Chunked reference: the payout starts the ticket, the keeper drives it.
    let mut chunked = capped_world(format!("{}.chunked", root(row)));
    let owner = chunked.owner();
    let owner_key = owner.pubkey();
    let ix = chunked.payout_instruction(
        PayoutKind::WithdrawSimple,
        owner_key,
        owner_key,
        1,
        0,
        0,
        chunked.dest,
        CloseRecordArg::Auto,
    );
    chunked
        .send("payout", vec![ix], &[&owner])
        .expect("the chunked payout");
    let started = chunked.last_event::<PayoutPaid>();
    assert!(!started.closing);
    assert_eq!(started.toll, 13_200_000);
    assert_eq!(
        chunked.ticket().phase,
        state::DISCOVER,
        "the chunked ticket started"
    );
    chunked.advance_until_idle(8);
    let reference = books(&chunked);
    let committed = chunked.last_event::<TicketCommitted>();
    assert_eq!(reference.pool.2, 0, "the chunked ticket consumed the toll");
    assert_eq!(reference.pool.4, 0, "all eight rows were pruned");
    assert_eq!(
        committed.credited,
        8 * 20,
        "every row was credited exactly its 20 lamports"
    );
    assert_eq!(committed.credited + committed.seed, 13_200_000);
    for spec in &specs {
        let backer = chunked.backer(spec.owner, spec.series);
        assert!(!backer.active, "pruned");
        assert_eq!(backer.lifetime, backer.entitlement, "capped");
    }

    // Inline: the same payout with the registry's accounts appended.
    let mut inline = capped_world(root(row));
    let owner = inline.owner();
    let mut ix = inline.payout_instruction(
        PayoutKind::WithdrawSimple,
        owner_key,
        owner_key,
        1,
        0,
        0,
        inline.dest,
        CloseRecordArg::Auto,
    );
    for spec in &specs {
        ix.accounts.push(AccountMeta::new(
            inline.rt.backer_address(spec.series, spec.owner),
            false,
        ));
    }
    ix.accounts
        .push(AccountMeta::new(inline.rt.registry_address(0), false));
    ix.accounts
        .push(AccountMeta::new(inline.rt.snapshot_address(0), false));
    let observation = inline
        .send("", vec![ix], &[&owner])
        .expect("the inline payout");
    let paid = observation.last_event::<PayoutPaid>();
    let inline_committed = observation.last_event::<TicketCommitted>();
    assert!(!paid.closing && paid.ticket_id != 0);
    assert_eq!(inline_committed.ticket_id, paid.ticket_id);
    assert_eq!(inline_committed, committed, "the same TicketCommitted");
    assert_eq!(inline.ticket().phase, state::IDLE);
    let actual = books(&inline);
    assert_eq!(
        actual.pool, reference.pool,
        "pool seed / funded_claims / pending_toll / next_ticket / registry_len"
    );
    assert_eq!(
        actual.backers, reference.backers,
        "every Backer account byte-identical"
    );
    assert_eq!(
        actual.registry, reference.registry,
        "registry page byte-identical"
    );
    assert_eq!(
        actual.snapshot, reference.snapshot,
        "snapshot page byte-identical"
    );
    assert_eq!(
        actual.ticket, reference.ticket,
        "ticket workspace byte-identical"
    );
    assert_eq!(actual.digest, reference.digest, "financial state digest");
    assert!(
        observation.fits_legacy_packet,
        "fits the legacy packet ({} B)",
        observation.packet_bytes
    );
    assert!(observation.message_account_count <= 64);
    let note = format!(
        "inline N=8, every row capped and pruned: compute_units {:?}, packet_bytes {}, keys {}, writable {}; byte-identical to the chunked ticket (toll {}, credited {}, seed {})",
        observation.compute_units,
        observation.packet_bytes,
        observation.message_account_count,
        observation.writable_account_count,
        inline_committed.toll,
        inline_committed.credited,
        inline_committed.seed
    );
    let mut log = chunked.take_log();
    log.extend(inline.take_log());
    annotate(&mut log, &root(row), note);
    log
}

// ---------------------------------------------------------------------------
// Row 10 (economy T8): the dust-loss candidate cutoff
// ---------------------------------------------------------------------------

/// A round whose only candidate is a participant with loss `loss`:
/// the closer was paid more than it deposited (21 days against a 1.5 SOL pot)
/// and eighteen participants are winners, so the candidate set is that one
/// participant and the draw selects it whatever the provider returns (N = 1
/// maps every output to ordinal 0).  Returns the captured scene.
fn dust_scene(row: &str, tag: &str, loss: u128) -> Scene {
    let mut roster = vec![nonloser(); 19];
    roster[0] = loser(loss);
    let mut scene = rebased(Scene::new(
        row,
        tag,
        Spec {
            roster,
            ..Spec::winners_only()
        },
    ));
    scene.through_capture();
    scene
}

/// Candidacy needs only `loss > 0`, so a Sybil with a single-digit loss can be
/// drawn.  The promoted entitlement is `floor(loss * 11,600 / 10,000)` and the
/// Series B row is appended to the registry only when more than the 10-lamport
/// cutoff remains.  Characterization with the exact numbers, end to end through
/// the real close, draw and `apply_promotion`:
///
/// * loss 9  -> entitlement 10 -> NOT appended (`registry_len` stays 0), yet the
///   Series B account was created from the locked reserve and `ever_b` is set
///   (the wasted 1,914,000 lamports of rent and the lost preferred status);
/// * loss 10 -> entitlement 11 -> appended (`registry_len` 1, registry page 0
///   created from the locked reserve, `Backer.active`).
///
/// The pure rule (`promotion_entitlement`, `remains_open`) is asserted first.
pub fn dust_loss_candidate_entitlement_cutoff() -> Vec<Observation> {
    let row = ROW_DUST_LOSS;
    // The pure rule, on the exact boundary.
    assert_eq!(
        pclose::promotion_entitlement(9, 11_600).expect("entitlement"),
        10
    );
    assert_eq!(
        pclose::promotion_entitlement(10, 11_600).expect("entitlement"),
        11
    );
    assert!(!positions::remains_open(10) && positions::remains_open(11));

    let mut log = Vec::new();

    // ---- loss 9: below the cutoff ------------------------------------------------------
    let mut scene = dust_scene(row, "", 9);
    let w = &mut scene.w;
    let winner = roster_key(1);
    let round = w.user_round(winner, 1);
    assert_eq!(round.deposits - round.net_payout, 9);
    let candidates = expected_candidates(w, 1);
    assert_eq!(
        candidates.preferred,
        vec![(winner, 9)],
        "the dust participant is the only candidate"
    );
    assert_eq!(candidates.fallback, vec![(winner, 9)]);
    let keys = apply_keys(w, 1);
    assert_eq!(keys.winner, Some(winner));
    let pool_before = w.pool();
    let user_before = w.user(winner);
    assert!(!user_before.ever_b);
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    assert_eq!(
        w.last_event::<PromotionApplied>(),
        PromotionApplied {
            schema_version: 1,
            round_id: 1,
            close_id: Some(1),
            winner,
            loss: 9,
            entitlement: 10,
        }
    );
    let close = w.close_record(1);
    assert_eq!(
        close.promotion,
        state::Promotion::Applied {
            winner,
            entitlement: 10
        }
    );
    let backer = w.backer(winner, SERIES_B);
    assert_eq!(
        (backer.entitlement, backer.lifetime, backer.active),
        (10, 0, false),
        "10 remaining is not open"
    );
    assert_eq!(
        w.pool().registry_len,
        pool_before.registry_len,
        "not appended to the registry"
    );
    assert_eq!(
        w.pool().registry_pages,
        pool_before.registry_pages,
        "no registry page was created"
    );
    assert_eq!(
        w.pool().next_sequence,
        pool_before.next_sequence + 1,
        "the funding sequence was still taken"
    );
    assert_eq!(close.spent.registry_rent_s + close.spent.registry_rent_o, 0);
    assert_eq!(
        close.spent.b_rent_s + close.spent.b_rent_o,
        w.rt.rent(state::Backer::SPACE),
        "the Series B account was created from the locked reserve"
    );
    let user = w.user(winner);
    assert!(
        user.ever_b,
        "ever_b is set: the wallet is no longer preferred"
    );
    assert_eq!(user.b_entitlement, 10);
    log.extend(scene.w.take_log());

    // ---- loss 10: one lamport above ----------------------------------------------------
    let mut scene = dust_scene(row, "loss10", 10);
    let w = &mut scene.w;
    let round = w.user_round(winner, 1);
    assert_eq!(round.deposits - round.net_payout, 10);
    let candidates = expected_candidates(w, 1);
    assert_eq!(candidates.preferred, vec![(winner, 10)]);
    let pool_before = w.pool();
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    assert_eq!(w.last_event::<PromotionApplied>().entitlement, 11);
    let close = w.close_record(1);
    assert_eq!(
        close.promotion,
        state::Promotion::Applied {
            winner,
            entitlement: 11
        }
    );
    let backer = w.backer(winner, SERIES_B);
    assert_eq!(
        (backer.entitlement, backer.lifetime, backer.active),
        (11, 0, true),
        "11 remaining is open"
    );
    assert_eq!(backer.slot, pool_before.registry_len);
    let pool = w.pool();
    assert_eq!(
        pool.registry_len,
        pool_before.registry_len + 1,
        "appended to the registry"
    );
    assert_eq!(
        pool.registry_pages,
        pool_before.registry_pages + 1,
        "the tail page was created from the locked reserve"
    );
    assert_eq!(w.registry(0).keys[0], w.rt.backer_address(SERIES_B, winner));
    assert!(close.spent.registry_rent_s + close.spent.registry_rent_o > 0);
    assert!(w.user(winner).ever_b);
    log.extend(scene.w.take_log());

    let note = "end to end through the real close and draw (the candidate set has one member, so the draw selects it for any provider output); loss 9 -> entitlement 10 (not appended), loss 10 -> entitlement 11 (appended)".to_string();
    annotate(&mut log, &root(row), note);
    log
}

// ---------------------------------------------------------------------------
// Row 11 (economy T10): a caller-funded snapshot page is permanent
// ---------------------------------------------------------------------------

/// `provision_page(Snapshot, Caller)` charges the caller the page's rent and
/// records the caller as the refund destination, but snapshot (and registry)
/// pages are permanent: `close_shared_account` refuses them whatever the close
/// record (`NotClosable`, 6033 -- the review's "6036" is `LockNotPermitted`;
/// `NotClosable` is 6033 in `error.rs`).  The refund component is therefore
/// never paid: the caller's rent is a donation to the page.  The row shows the
/// payer's cost, the refusal against a pending record and against a terminal,
/// finalized one, and that neither the page, the payer's balance nor the
/// operations books moved.
pub fn caller_funded_snapshot_page_is_not_closable() -> Vec<Observation> {
    let row = ROW_CALLER_SNAPSHOT;
    let mut w = World::genesis().scoped_label(root(row));
    let payer = w.rt.payer.pubkey();
    w.provision_close_record(1);
    let page_rent = w.rt.rent(state::SnapshotPage::SPACE);
    let snapshot1 = w.rt.snapshot_address(1);
    w.watch(snapshot1);
    assert_eq!(w.pool().snapshot_pages, 1);

    let payer_before = w.lamports(payer);
    let provision = provision_kind_ix(&w, PageKind::Snapshot, 1, Funding::Caller, payer);
    let provisioned = ok(
        &mut w,
        "provision_snapshot1_by_caller",
        vec![provision],
        &[],
    );
    assert_eq!(w.pool().snapshot_pages, 2);
    let page = w.snapshot_page(1);
    assert_eq!(
        (page.rent.ops, page.rent.owner, page.rent.owner_refund),
        (0, page_rent, payer)
    );
    assert_eq!(w.lamports(snapshot1), page_rent);
    assert_eq!(
        payer_before - w.lamports(payer),
        page_rent + provisioned.expected_fee_lamports,
        "the caller paid the page's whole rent and the fee"
    );
    let ops_before = w.ops();

    // Against the pending record, and against the most permissive one a close can leave.
    let close = close_page_ix(&w, PageKind::Snapshot, snapshot1, payer, 1);
    reject(&mut w, "", vec![close], &[], ErrorCode::NotClosable);
    let mut record = w.close_record(1);
    record.promotion = state::Promotion::Skipped {
        reason: pclose::REASON_DEADLINE,
        stage: pclose::STAGE_UNREQUESTED,
    };
    record.reserve_settled = true;
    record.cash_committed = true;
    record.finalized = true;
    w.rt.replace_close_record(record);
    let close = close_page_ix(&w, PageKind::Snapshot, snapshot1, payer, 1);
    reject(
        &mut w,
        "terminal_finalized_record",
        vec![close],
        &[],
        ErrorCode::NotClosable,
    );

    assert_eq!(
        w.lamports(snapshot1),
        page_rent,
        "the rent stays in the page"
    );
    assert_eq!(
        w.snapshot_page(1).rent.owner_refund,
        payer,
        "the refund component is recorded but unreachable"
    );
    assert_eq!(w.pool().snapshot_pages, 2, "the counter never decrements");
    let ops = w.ops();
    assert_eq!(
        (
            ops.free_balance,
            ops.unallocated_excess,
            ops.locked_remaining_total
        ),
        (
            ops_before.free_balance,
            ops_before.unallocated_excess,
            ops_before.locked_remaining_total
        )
    );
    w.take_log()
}
