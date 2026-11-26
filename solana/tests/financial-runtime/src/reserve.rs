//! M8 reserve, refund and cleanup end-to-end suite (plan Milestone 8, step 1).
//!
//! Every row runs the actual compiled Musical Chairs SBF **and the captured
//! mainnet ORAO executable** in LiteSVM, through the same `World` / `Scene`
//! machinery as the M7 close suite (`close.rs`), with the closing payout of
//! every scene a REAL `withdraw_simple`.  The rows follow the plan's M8 test
//! list name for name; the randomized conservation row lives in `fuzz.rs`.
//!
//! Labels are `financial.reserve.<row>` for a row's headline observation and
//! `financial.reserve.<row>.<variant>` for the supporting ones.  Every
//! rejection asserts the exact program (or framework) error code and fee-only
//! rollback of every account the World can touch; every success asserts both
//! conservation identities (`World` checks the OpsCash and game-vault
//! registers after every recorded transaction), the partition identity
//! `ops.locked_remaining_total == sum of every CloseRecord's remaining
//! partitions` (settled closes that still hold a deferred owner refund
//! included), and the exact lamport movement.
//!
//! `Scene::new` (M7) hard-codes the `financial.close.` label root and has no
//! hook between the close record's provisioning and the closing payout, so
//! `build_scene` mirrors it with the `financial.reserve.` root plus one hook
//! (candidate pages provisioned before the payout, for the uneven
//! pre-provisioning row).  The `Spec`, `Scene` and every instruction builder
//! are M7's, reused unchanged.
//!
//! Limits stated up front: ONE synthetic ORAO fulfill authority, no cluster
//! latency, the Squads program is the test-only stand-in, and LiteSVM applies
//! the rent-state transition rule only to accounts WITH data, so an
//! undeliverable owner refund is produced the way the cluster would see it
//! (a destination holding fewer lamports than the credit needs to reach the
//! rent-exempt minimum) but the cluster-side rejection itself is not
//! exercised (see the IMPL-43 receipt).

use std::collections::BTreeMap;

use musical_chairs_program::{
    accounts, allocation::SERIES_A, close as pclose, instruction, positions::PlanKindArg, state,
    ErrorCode, Funding, OwnerRefundDelivered, PageKind, PayoutPaid, PromotionApplied, ID,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::close::{
    apply_ix, apply_ix_with, apply_keys, assert_identities, budget_ix, candidate_page,
    candidate_page_key, capture_ix, chain, client_spendable, delta, do_request, expire_ix,
    expire_ok, expected_candidates, finalize_ix, finish_by_expiry, host_request, loser,
    nineteen, observe_ix, ok, ok_g, pages_of, provider_keys, provision_candidate_pages, provision_ix,
    raw_withdraw_ix, reconcile_client_ix, reconcile_ix, reject, reject_g, reject_provider,
    rent_of, request_ix, request_rents, restore, seal_labelled, sponsor_for_owner_rent,
    standard_reserve, start_second_close, tamper, withdraw_ix, ApplyKeys, Part, Scene, Spec,
    REQUEST_FEE,
};
use crate::controls::{assert_outcomes_consistent, assert_unique_labels};
use crate::runtime::{anchor_ix, Observation};
use crate::world::{BackerSpec, World, DAY, SOL};

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

pub const ROW_SHORT_BY_ONE: &str = "reserve_short_by_one_lamport_rejects_closing_payout_before_mutation";
pub const ROW_BOTH_SETS: &str =
    "required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser";
pub const ROW_MISSING_B: &str = "missing_b_account_is_created_from_locked_reserve_with_ops_free_zero";
pub const ROW_REGISTRY_TAIL: &str = "registry_tail_full_extension_page_created_from_locked_reserve";
pub const ROW_OWNER_QUOTED: &str =
    "owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution";
pub const ROW_SPONSOR_FIRST: &str = "sponsor_partition_consumed_first_then_owner";
pub const ROW_SPARE: &str = "spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation";
pub const ROW_UNEVEN: &str =
    "uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages";
pub const ROW_ALL_FREE_LOCKED: &str =
    "all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds";
pub const ROW_UNUSED_REFUNDED: &str =
    "unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination";
pub const ROW_CLIENT_REIMBURSEMENT: &str =
    "client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return";
pub const ROW_TABLE_8_6: &str = "conservation_transitions_table_8_6_holds_for_every_row";
pub const ROW_SPONSOR_WITHDRAWAL: &str = "sponsor_withdrawal_cannot_touch_locked";
pub const ROW_PERMANENT_PAGES: &str = "snapshot_and_registry_pages_are_not_closable";
pub const ROW_CANDIDATE_CLOSABLE: &str =
    "candidate_pages_closable_only_after_their_own_close_is_terminal";
pub const ROW_PARTICIPANT_CLOSABLE: &str =
    "participant_pages_closable_only_after_their_round_finalized";
pub const ROW_OLD_TAG: &str = "old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts";
pub const ROW_DEPLETED_FREE: &str = "full_cash_progress_with_depleted_free_ops_funds";
pub const ROW_NO_CLEANUP: &str = "cleanup_is_never_required_for_commit_finalize_or_open";
/// Codex M8 review: runtime proof of the IMPL-36 G1 guard (not a plan row).
pub const ROW_SETTLED_LOCKS_NOTHING: &str =
    "settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable";

/// Every row, in suite order (the randomized conservation row is `fuzz.rs`).
pub const ROW_NAMES: [&str; 20] = [
    ROW_SHORT_BY_ONE,
    ROW_BOTH_SETS,
    ROW_MISSING_B,
    ROW_REGISTRY_TAIL,
    ROW_OWNER_QUOTED,
    ROW_SPONSOR_FIRST,
    ROW_SPARE,
    ROW_UNEVEN,
    ROW_ALL_FREE_LOCKED,
    ROW_UNUSED_REFUNDED,
    ROW_CLIENT_REIMBURSEMENT,
    ROW_TABLE_8_6,
    ROW_SPONSOR_WITHDRAWAL,
    ROW_PERMANENT_PAGES,
    ROW_CANDIDATE_CLOSABLE,
    ROW_PARTICIPANT_CLOSABLE,
    ROW_OLD_TAG,
    ROW_DEPLETED_FREE,
    ROW_NO_CLEANUP,
    ROW_SETTLED_LOCKS_NOTHING,
];

/// One suite row: a function that runs the scenario and returns its observations.
pub type RowFn = fn() -> Vec<Observation>;

/// Every transaction row as `(name, runner)`, in suite order.
pub fn rows() -> Vec<(&'static str, RowFn)> {
    vec![
        (ROW_SHORT_BY_ONE, reserve_short_by_one_lamport_rejects_closing_payout_before_mutation),
        (ROW_BOTH_SETS, required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser),
        (ROW_MISSING_B, missing_b_account_is_created_from_locked_reserve_with_ops_free_zero),
        (ROW_REGISTRY_TAIL, registry_tail_full_extension_page_created_from_locked_reserve),
        (ROW_OWNER_QUOTED, owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution),
        (ROW_SPONSOR_FIRST, sponsor_partition_consumed_first_then_owner),
        (ROW_SPARE, spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation),
        (ROW_UNEVEN, uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages),
        (ROW_ALL_FREE_LOCKED, all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds),
        (ROW_UNUSED_REFUNDED, unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination),
        (ROW_CLIENT_REIMBURSEMENT, client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return),
        (ROW_TABLE_8_6, conservation_transitions_table_8_6_holds_for_every_row),
        (ROW_SPONSOR_WITHDRAWAL, sponsor_withdrawal_cannot_touch_locked),
        (ROW_PERMANENT_PAGES, snapshot_and_registry_pages_are_not_closable),
        (ROW_CANDIDATE_CLOSABLE, candidate_pages_closable_only_after_their_own_close_is_terminal),
        (ROW_PARTICIPANT_CLOSABLE, participant_pages_closable_only_after_their_round_finalized),
        (ROW_OLD_TAG, old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts),
        (ROW_DEPLETED_FREE, full_cash_progress_with_depleted_free_ops_funds),
        (ROW_NO_CLEANUP, cleanup_is_never_required_for_commit_finalize_or_open),
        (ROW_SETTLED_LOCKS_NOTHING, settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable),
    ]
}

/// A row must contain an observation labelled exactly
/// `financial.reserve.<row>`.
pub fn assert_headline(observations: &[Observation], row: &str) {
    let headline = root(row, "");
    assert!(
        observations.iter().any(|observation| observation.label == headline),
        "row {row} produced no headline observation {headline}"
    );
}

/// Run one row and check it is internally consistent and its labels unique.
pub fn run_row(name: &str, row: fn() -> Vec<Observation>) {
    let observations = row();
    assert_headline(&observations, name);
    assert_outcomes_consistent(&observations);
    assert_unique_labels(&observations);
}

/// The whole reserve suite as one observation list.  Every row asserts its own
/// expectations (and panics with the offending observation on a violation).
pub fn run_reserve_suite() -> Vec<Observation> {
    let mut observations = Vec::new();
    for (name, row) in rows() {
        let produced = row();
        assert_headline(&produced, name);
        assert_outcomes_consistent(&produced);
        observations.extend(produced);
    }
    assert_unique_labels(&observations);
    observations
}

// ---------------------------------------------------------------------------
// Labels, small readers
// ---------------------------------------------------------------------------

/// `financial.reserve.<row>[.<tag>]`.
pub(crate) fn root(row: &str, tag: &str) -> String {
    if tag.is_empty() {
        format!("financial.reserve.{row}")
    } else {
        format!("financial.reserve.{row}.{tag}")
    }
}

/// Rent of one candidate page at LiteSVM's default schedule.
pub(crate) fn page_rent() -> u64 {
    rent_of(state::CandidatePage::SPACE)
}

pub(crate) fn cash_of(w: &World) -> u64 {
    w.lamports(w.rt.ops_cash_address())
}

fn i(value: u64) -> i128 {
    i128::from(value)
}

/// Sum of every spent-ledger cell (candidate pages, registry page, B account,
/// provider fee, pending-request rent; sponsor and owner).
pub(crate) fn spent_total(spent: &state::SpentLedger) -> u64 {
    spent.candidate_rent_s
        + spent.candidate_rent_o
        + spent.registry_rent_s
        + spent.registry_rent_o
        + spent.b_rent_s
        + spent.b_rent_o
        + spent.provider_fee_s
        + spent.provider_fee_o
        + spent.request_rent_s
        + spent.request_rent_o
}

/// `(sponsor part, owner part)` of a `debit` against partitions `(s, o)`,
/// sponsor first (spec section 8.3), as the host model of the program rule.
pub(crate) fn split_sponsor_first(s: u64, o: u64, debit: u64) -> (u64, u64) {
    let sponsor_part = debit.min(s);
    let owner_part = debit - sponsor_part;
    assert!(owner_part <= o, "the partitions cannot cover the debit");
    (sponsor_part, owner_part)
}

// ---------------------------------------------------------------------------
// Tracked figures (spec 8.6)
// ---------------------------------------------------------------------------

/// Every figure the section 8.6 table names, plus the wallets a row moves.
pub(crate) type Tracked = BTreeMap<&'static str, i128>;

/// Read every tracked figure of `close_id` (its `CloseRecord` must exist) and
/// the lamports of the named `wallets`.
pub(crate) fn tracked(w: &World, close_id: u64, wallets: &[(&'static str, Pubkey)]) -> Tracked {
    let ops = w.ops();
    let close = w.close_record(close_id);
    let spent = close.spent;
    let mut t: Tracked = BTreeMap::new();
    let mut put = |key: &'static str, value: i128| {
        t.insert(key, value);
    };
    put("cash", i(cash_of(w)));
    put("free", i(ops.free_balance));
    put("s", i(close.locked_sponsor_remaining));
    put("o", i(close.locked_owner_remaining));
    put("L", i(ops.locked_remaining_total));
    put("unallocated_excess", i(ops.unallocated_excess));
    put("total_rent_funded", ops.total_rent_funded as i128);
    put("candidate_rent_s", i(spent.candidate_rent_s));
    put("candidate_rent_o", i(spent.candidate_rent_o));
    put("registry_rent_s", i(spent.registry_rent_s));
    put("registry_rent_o", i(spent.registry_rent_o));
    put("b_rent_s", i(spent.b_rent_s));
    put("b_rent_o", i(spent.b_rent_o));
    put("provider_fee_s", i(spent.provider_fee_s));
    put("provider_fee_o", i(spent.provider_fee_o));
    put("request_rent_s", i(spent.request_rent_s));
    put("request_rent_o", i(spent.request_rent_o));
    put("request_rent_funded_s", i(close.request_rent_funded_s));
    put("request_rent_funded_o", i(close.request_rent_funded_o));
    put("provisioned_preferred", i(close.provisioned_preferred_pages));
    put("provisioned_fallback", i(close.provisioned_fallback_pages));
    put("approved_cap", i(close.approved_provider_fee_cap));
    put("required_reserve", i(close.required_reserve));
    put("fee_consumed_total", ops.provider_fee_consumed_total as i128);
    put("rent_outstanding", i(ops.request_rent_outstanding));
    put("eligible_return_budget", i(ops.eligible_return_budget));
    put("returned_total", ops.provider_returned_total as i128);
    put("eligible_return_remaining", i(close.eligible_return_remaining));
    put("owner_refundable_remaining", i(close.owner_request_rent_refundable_remaining));
    put("rent_returned", i(close.request_rent_returned));
    put("reserve_settled", i128::from(u8::from(close.reserve_settled)));
    put("reimbursement_observed", i128::from(u8::from(close.reimbursement_observed)));
    put("client_balance_observed", i(ops.client_balance_observed));
    put("client_protected_rent", i(ops.client_protected_rent));
    put("provider_deficit", i(ops.provider_deficit));
    for &(name, key) in wallets {
        put(name, i(w.lamports(key)));
    }
    t
}

/// The figures that moved between `before` and `after` equal `expected`
/// exactly (everything not listed did not move).
pub(crate) fn assert_row(name: &str, before: &Tracked, after: &Tracked, expected: &[(&'static str, i128)]) {
    let mut got: BTreeMap<&'static str, i128> = BTreeMap::new();
    for (key, value) in after {
        let earlier = before
            .get(key)
            .copied()
            .unwrap_or_else(|| panic!("row `{name}`: `{key}` is tracked after but not before"));
        if *value != earlier {
            got.insert(*key, *value - earlier);
        }
    }
    let mut want: BTreeMap<&'static str, i128> = BTreeMap::new();
    for &(key, value) in expected {
        *want.entry(key).or_insert(0) += value;
    }
    want.retain(|_, value| *value != 0);
    assert_eq!(
        got, want,
        "conservation row `{name}`: the figures that moved (left) differ from the spec 8.6 figures (right)"
    );
}

/// The figures of a row as JSON, for the evidence note.
fn row_json(name: &str, before: &Tracked, after: &Tracked) -> serde_json::Value {
    let mut moved = serde_json::Map::new();
    for (key, value) in after {
        let earlier = before.get(key).copied().unwrap_or(0);
        if *value != earlier {
            moved.insert((*key).to_string(), serde_json::json!(*value - earlier));
        }
    }
    serde_json::json!({ "row": name, "moved": moved })
}

// ---------------------------------------------------------------------------
// The scene builder (M7 `Scene::new` with the reserve label root and a hook)
// ---------------------------------------------------------------------------

/// What the closing payout of a built scene started from.
pub(crate) struct PayoutFacts {
    /// Every tracked figure, read immediately before the closing payout, with
    /// the `owner` and `dest` wallets.
    pub(crate) before: Tracked,
    /// The closing payout's observation.
    pub(crate) observation: Observation,
}

/// Build the world of `spec` up to and including the closing payout, exactly
/// as the M7 `Scene::new` does, labelled `financial.reserve.<row>[.<tag>]`.
/// `pre` lists candidate pages (`set_tag`, funding) provisioned for the NEXT
/// close BEFORE the payout (in order, sequential per set); the payout then
/// reserves only the pages still missing.
pub(crate) fn build_scene(
    row: &str,
    tag: &str,
    payout_variant: &str,
    spec: Spec,
    pre: &[(u8, Funding)],
) -> (Scene, PayoutFacts) {
    let mut w = World::genesis_with_provider().scoped_label(root(row, tag));
    w.provider_setup();
    assert_eq!(rent_of(0), w.rt.rent(0), "LiteSVM's rent schedule is Rent::default()");
    if let Some(bytes) = spec.request_bytes {
        let mut config = w.config(1);
        config.pending_request_bytes = bytes;
        w.rt.replace_config(config);
    }
    if let Some(domain) = spec.domain {
        w.set_pool(|pool| pool.cluster_domain = domain);
    }
    if spec.genesis_pages {
        assert!(spec.backers.is_empty());
        w.rt.strip_pages_to_genesis();
    }
    if !spec.backers.is_empty() {
        let backers: Vec<BackerSpec> = spec
            .backers
            .iter()
            .map(|owner| BackerSpec::new(*owner, SERIES_A, 62_000_000))
            .collect();
        w.install_backer_specs(&backers);
        // A real Series A Backer always has a `User` (`fund_series_a` is
        // `init_if_needed` on it) and `claim_backer` requires it.
        for owner in &spec.backers {
            w.install_user(*owner, true, false);
        }
    }

    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = spec.dest.unwrap_or(w.dest);
    w.watch(dest);
    for &(nonce, gross) in &spec.positions {
        w.open_fixture_position(owner_key, nonce, PlanKindArg::Simple, gross);
    }
    for (index, part) in spec.roster.iter().enumerate() {
        let key = spec.key(index);
        w.install_participant(key, part.deposits, part.net, part.ever_a, part.ever_b);
        w.watch(w.rt.backer_address(1, key));
    }
    w.watch(w.rt.backer_address(1, owner_key));
    for close_id in 1..=2 {
        for set_tag in 0..=1u8 {
            for index in 0..5 {
                w.watch(candidate_page_key(close_id, set_tag, index));
            }
        }
    }

    let prior = &spec.positions[..spec.positions.len() - 1];
    w.set_pot_fixture(if prior.is_empty() { spec.pot } else { 10 * SOL });
    w.provision_close_record(1);
    let participants = 1 + spec.roster.len() as u64;
    if !pre.is_empty() {
        // Free pages are paid from exactly the cash set aside for them; Caller
        // pages by the harness payer.
        let free_pages = pre.iter().filter(|(_, funding)| *funding == Funding::Free).count() as u64;
        w.set_ops_free(page_rent() * free_pages);
        let mut next = [0u64; 2];
        for &(set, funding) in pre {
            let index = next[usize::from(set)];
            next[usize::from(set)] += 1;
            let name = format!("preprovision_{}{index}", if set == 0 { "preferred" } else { "fallback" });
            let instruction = provision_ix(&w, 1, set, index, funding);
            ok(&mut w, &name, vec![instruction], &[]);
        }
        assert_eq!(w.ops().free_balance, 0, "the pre-provisioned pages consumed exactly their rent");
    }
    w.warp_seconds(spec.days * DAY);
    for &(nonce, _) in prior {
        w.withdraw_simple(&format!("settle_position{nonce}"), &owner, nonce, 0, dest, 0)
            .unwrap_or_else(|failed| panic!("prior solvent payout failed: {failed:?}"));
    }
    if !prior.is_empty() {
        w.set_pot_fixture(spec.pot);
    }

    let (existing_p, existing_f) = {
        let record = w.close_record(1);
        (record.existing_preferred_pages, record.existing_fallback_pages)
    };
    let reserve = w.expected_reserve(participants, spec.backers.len() as u64, existing_p, existing_f);
    let free = spec.sponsor.unwrap_or(reserve) + spec.spare;
    w.set_ops_free(free);
    let contribution = reserve.saturating_sub(free);
    let closing_nonce = spec.positions.last().expect("a closing position").0;
    let before = tracked(&w, 1, &[("owner", owner_key), ("dest", dest)]);
    let paid = w
        .withdraw_simple(payout_variant, &owner, closing_nonce, 0, dest, contribution)
        .unwrap_or_else(|failed| panic!("the closing payout failed: {failed:?}"));
    let payout = paid.last_event::<PayoutPaid>();
    assert!(payout.closing && payout.close_id == Some(1), "the payout closed the round");
    let bound = w.close_record(1);
    assert_eq!(bound.close_started_at, w.now(), "the close starts at the payout's Clock second");
    assert_eq!(
        bound.promotion_deadline,
        bound.close_started_at + 600,
        "the pinned deadline is exactly close_started_at + 600 s"
    );
    assert_eq!(bound.owner_refund_destination, dest, "the payout destination is the refund destination");
    assert_eq!(bound.required_reserve, reserve);
    assert_eq!(
        bound.locked_sponsor_remaining + bound.locked_owner_remaining,
        reserve,
        "remaining == unspent at lock time"
    );
    assert_identities(&w);
    assert!(
        w.lamports(dest) >= w.rt.rent(0),
        "closing destination holds at least the rent-exempt minimum"
    );
    w.watch(w.rt.close_record_address(1));
    (
        Scene { w, owner, dest, reserve, payout },
        PayoutFacts { before, observation: paid },
    )
}

/// [`build_scene`] with the default payout label and no pre-provisioning.
pub(crate) fn make_scene(row: &str, tag: &str, spec: Spec) -> Scene {
    build_scene(row, tag, "closing_payout", spec, &[]).0
}

/// A round of twenty participants who are ALL preferred losers (the closer
/// included): both candidate sets hold all twenty.
pub(crate) fn every_participant_a_preferred_loser() -> Spec {
    Spec {
        roster: (0..19u128).map(|index| loser(100_000_000 + 10_000_000 * index)).collect::<Vec<Part>>(),
        ..Spec::standard()
    }
}

/// A round whose closer WINS its payout (21 days at 11 % a day against a
/// 1.5 SOL pot pays it more than it deposited), so the closer is never a
/// candidate: the draw's winner is always a roster loser, and no roster key
/// holds a Series B account -- the B account is always created from the locked
/// reserve (the canonical fixture gives the CLOSER an inactive Series B account,
/// which a closer-winner would merge into at no rent).
pub(crate) fn closer_wins(roster: Vec<Part>) -> Spec {
    Spec { roster, days: 21, pot: 1_500_000_000, ..Spec::standard() }
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

pub(crate) fn deliver_ix(w: &World, close_id: u64, destination: Pubkey) -> Instruction {
    anchor_ix(
        accounts::DeliverOwnerRefund {
            close_record: w.rt.close_record_address(close_id),
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            owner_refund: destination,
            system_program: system_program::ID,
            event_authority: w.rt.event_authority_address(),
            program: ID,
        },
        instruction::DeliverOwnerRefund {},
    )
}

/// The page account a `PageKind`/`index` addresses.
pub(crate) fn page_key(w: &World, kind: PageKind, index: u64) -> Pubkey {
    match kind {
        PageKind::Registry => w.rt.registry_address(index),
        PageKind::Snapshot => w.rt.snapshot_address(index),
        PageKind::Participant { round } => w.rt.participant_address(round, index),
        PageKind::Candidate { close_id, set_tag } => candidate_page_key(close_id, set_tag, index),
    }
}

/// `provision_page` with an explicit `payer`.
pub(crate) fn provision_kind_ix(
    w: &World,
    kind: PageKind,
    index: u64,
    funding: Funding,
    payer: Pubkey,
) -> Instruction {
    let close_record = match kind {
        PageKind::Candidate { close_id, .. } => Some(w.rt.close_record_address(close_id)),
        _ => None,
    };
    anchor_ix(
        accounts::ProvisionPage {
            pool: w.rt.pool_address(),
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            page: page_key(w, kind, index),
            payer,
            close_record,
            system_program: system_program::ID,
        },
        instruction::ProvisionPage { kind, index, funding },
    )
}

/// `provision_close_record` for `close_id`.
pub(crate) fn provision_close_record_ix(
    w: &World,
    close_id: u64,
    funding: Funding,
    payer: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::ProvisionCloseRecord {
            pool: w.rt.pool_address(),
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            close_record: w.rt.close_record_address(close_id),
            payer,
            system_program: system_program::ID,
        },
        instruction::ProvisionCloseRecord { close_id, funding },
    )
}

/// `close_shared_account(kind)` on `page`, judged by `CloseRecord(close_id)`.
pub(crate) fn close_page_ix(
    w: &World,
    kind: PageKind,
    page: Pubkey,
    owner_refund: Pubkey,
    close_id: u64,
) -> Instruction {
    anchor_ix(
        accounts::CloseSharedAccount {
            page,
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            owner_refund,
            close_record: w.rt.close_record_address(close_id),
            system_program: system_program::ID,
        },
        instruction::CloseSharedAccount { kind },
    )
}

pub(crate) fn fund_ops_ix(w: &World, sender: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::FundOperations {
            sender,
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: w.rt.event_authority_address(),
            program: ID,
        },
        instruction::FundOperations { amount },
    )
}

pub(crate) fn withdraw_ops_ix(w: &World, sponsor: Pubkey, amount: u64) -> Instruction {
    anchor_ix(
        accounts::WithdrawOperations {
            sponsor,
            ops: w.rt.ops_address(),
            ops_cash: w.rt.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: w.rt.event_authority_address(),
            program: ID,
        },
        instruction::WithdrawOperations { amount },
    )
}

/// A plain system transfer.
pub(crate) fn transfer_ix(from: Pubkey, to: Pubkey, lamports: u64) -> Instruction {
    system_instruction::transfer(&from, &to, lamports)
}

/// A keypair-controlled owner destination (so a row can really move its
/// lamports away, or fund it, with a system transfer it signs).
pub(crate) fn destination_keypair(seed: u8) -> Keypair {
    Keypair::new_from_array([seed; 32])
}

// ---------------------------------------------------------------------------
// Row: reserve_short_by_one_lamport_rejects_closing_payout_before_mutation
// ---------------------------------------------------------------------------

/// A closing payout whose sponsor cash plus the owner's capped opt-in is short
/// of the reserve by ONE lamport is refused (`ReserveUnavailable`, 6024) before
/// anything moves, whichever source is the short one; one lamport richer, the
/// same payout locks exactly the reserve.  (The plan's thin re-run of the
/// positions suite's `closing_payout_short_by_one_lamport_rejects_before_mutation`
/// scenario, here with all three short sources and the exact boundary.)
pub fn reserve_short_by_one_lamport_rejects_closing_payout_before_mutation() -> Vec<Observation> {
    let row = ROW_SHORT_BY_ONE;
    let mut w = World::closing_fixture(9, 8).scoped_label(root(row, ""));
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let cash = w.rt.ops_cash_address();
    let reserve = w.expected_reserve(9, 8, 0, 0);
    let record0 = w.data(w.rt.close_record_address(1));

    // (variant, free sponsor cash, owner opt-in cap): each one lamport short.
    let short: [(&str, u64, u64); 3] = [
        ("sponsor_short_by_one", reserve - 1, 0),
        ("owner_cap_short_by_one", reserve - 1_001, 1_000),
        ("nothing_locked_and_cap_short_by_one", 0, reserve - 1),
    ];
    for (variant, free, cap) in short {
        w.set_ops_free(free);
        let before = w.snapshot_all();
        let (owner0, cash0) = (w.lamports(owner_key), w.lamports(cash));
        let rejected = w.withdraw_simple(variant, &owner, 1, 0, dest, cap).unwrap_err();
        w.assert_rejected(&rejected, ErrorCode::ReserveUnavailable);
        w.assert_fee_only(&before, &rejected);
        assert_identities(&w);
        assert_eq!((w.lamports(owner_key), w.lamports(cash)), (owner0, cash0), "no contribution moved");
        assert_eq!(w.ops().free_balance, free, "Operations untouched");
        assert_eq!(w.ops().locked_remaining_total, 0, "nothing locked");
        assert!(!w.pool().closing(), "the round is still open");
        assert_eq!(w.data(w.rt.close_record_address(1)), record0, "the CloseRecord was not bound");
    }

    // One lamport richer in either source: the lock is exactly the reserve.
    w.set_ops_free(reserve - 1_000);
    let (owner0, cash0) = (w.lamports(owner_key), w.lamports(cash));
    w.withdraw_simple("", &owner, 1, 0, dest, 1_000)
        .expect("the payout one lamport richer locks the reserve");
    assert_identities(&w);
    let record = w.close_record(1);
    assert_eq!(record.required_reserve, reserve);
    assert_eq!(
        (record.locked_sponsor_remaining, record.locked_owner_remaining),
        (reserve - 1_000, 1_000),
        "sponsor cash first, the owner opt-in for the remainder"
    );
    assert_eq!(w.ops().free_balance, 0, "all sponsor cash was locked");
    assert_eq!(w.ops().locked_remaining_total, reserve);
    assert_eq!(owner0 - w.lamports(owner_key), 1_000, "the owner paid exactly its contribution");
    assert_eq!(w.lamports(cash) - cash0, 1_000, "OpsCash gained exactly the owner partition");
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser
// ---------------------------------------------------------------------------

/// Twenty participants, every one a preferred loser (so every one is also a
/// fallback candidate): both sets fill to three pages.  The reserve's
/// candidate component is exactly the six pages, all six are created from the
/// locked reserve with `ops.free_balance == 0` throughout, the sets seal, the
/// draw runs and the promotion applies without any mid-close provisioning
/// dependency.
pub fn required_reserve_covers_both_candidate_sets_when_every_participant_is_a_preferred_loser(
) -> Vec<Observation> {
    let row = ROW_BOTH_SETS;
    let page = page_rent();
    let mut scene = make_scene(row, "", every_participant_a_preferred_loser());
    let reserve = scene.reserve;
    assert_eq!(reserve, standard_reserve(), "twenty participants and an empty registry");
    let w = &mut scene.w;
    assert_eq!(w.ops().free_balance, 0, "every free lamport is locked");
    let close = w.close_record(1);
    assert_eq!(close.required_pages_per_set, 3);
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (reserve, 0));

    let expected = expected_candidates(w, 1);
    assert_eq!(
        (expected.preferred.len(), expected.fallback.len()),
        (20, 20),
        "every participant is in BOTH sets"
    );
    assert_eq!((pages_of(20), pages_of(20)), (3, 3));

    // All six pages out of the locked reserve, sponsor partition first, with
    // no free cash anywhere.
    let mut sponsor_left = reserve;
    for set in 0..=1u8 {
        for index in 0..3u64 {
            let instruction = provision_ix(w, 1, set, index, Funding::LockedClose);
            let name = format!("provision_{}{index}", if set == 0 { "preferred" } else { "fallback" });
            ok(w, &name, vec![instruction], &[]);
            sponsor_left -= page;
            let close = w.close_record(1);
            assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (sponsor_left, 0));
            assert_eq!(w.ops().free_balance, 0, "no free cash was needed");
            assert_eq!(w.ops().locked_remaining_total, sponsor_left);
        }
    }
    seal_labelled(w, 1, 4, "build_last");
    let close = w.close_record(1);
    assert!(close.sealed);
    assert_eq!((close.preferred_count, close.fallback_count, close.selected_set_tag), (20, 20, 0));
    assert_eq!(close.preferred_digest, chain(0, &expected.preferred));
    assert_eq!(close.fallback_digest, chain(1, &expected.fallback));
    for set in 0..=1u8 {
        for (index, len) in [(0u64, 8u8), (1, 8), (2, 4)] {
            assert_eq!(candidate_page(w, 1, set, index).len, len, "set {set} page {index} is full");
        }
    }
    assert_eq!(close.spent.candidate_rent_s, 6 * page, "the six pages were the reserve's candidate component");
    assert_eq!(close.spent.candidate_rent_o, 0);
    assert_eq!(close.locked_sponsor_remaining, reserve - 6 * page);

    // The draw and the promotion run from what is left, still with no free cash.
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    assert_eq!(w.ops().free_balance, 0, "request, fulfilment and capture needed no free cash");
    // The closer is a candidate here, so the winner may be the closer, whose
    // inactive Series B account (canonical fixture) is merged into at no rent.
    let b_key = apply_keys(w, 1).winner_backer_b.expect("B address");
    let b_rent = if w.account(b_key).is_some() { 0 } else { rent_of(state::Backer::SPACE) };
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    let close = w.close_record(1);
    assert!(close.reserve_settled);
    assert_eq!(
        spent_total(&close.spent) + w.ops().free_balance,
        reserve,
        "spent + the settled remainder is exactly the reserve"
    );
    assert_eq!(
        w.ops().free_balance,
        REQUEST_FEE * 2_000 / 10_000 + if b_rent == 0 { rent_of(state::Backer::SPACE) } else { 0 },
        "only the unspent fee headroom (and a B account that already existed) came back"
    );
    assert_eq!(w.ops().locked_remaining_total, 0);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: missing_b_account_is_created_from_locked_reserve_with_ops_free_zero
// ---------------------------------------------------------------------------

/// The winner has no Series B account and Operations holds NO free cash: the
/// B account is created from the locked reserve, rent-exempt for exactly
/// `Backer::SPACE`, its rent components recorded by source.  Run twice: with
/// the sponsor partition covering it, and with the sponsor partition spent
/// exactly through the request so the OWNER partition funds the B account (and
/// the registry page) -- free stays zero throughout.
pub fn missing_b_account_is_created_from_locked_reserve_with_ops_free_zero() -> Vec<Observation> {
    let row = ROW_MISSING_B;
    let page = page_rent();
    let (rent, _) = request_rents();
    let b_rent = rent_of(state::Backer::SPACE);
    let registry_rent = rent_of(state::RegistryPage::SPACE);
    let mut log = Vec::new();

    for (tag, sponsor) in [("sponsor_funded", None::<u64>), ("", Some(2 * page + REQUEST_FEE + rent))] {
        let spec = match sponsor {
            Some(sponsor) => closer_wins(nineteen()).with_sponsor(sponsor),
            None => closer_wins(nineteen()),
        };
        let mut scene = make_scene(row, tag, spec);
        let dest = scene.dest;
        scene.provision_and_seal("build_last");
        let w = &mut scene.w;
        do_request(w, 1, "request");
        w.fulfill("fulfill", 1).expect("fulfill");
        let capture = capture_ix(w, 1);
        ok(w, "capture", vec![capture], &[]);

        let keys = apply_keys(w, 1);
        let (winner, b_key) = (keys.winner.expect("winner"), keys.winner_backer_b.expect("B address"));
        assert!(w.account(b_key).is_none(), "no Series B account exists yet");
        assert_eq!(w.ops().free_balance, 0, "Operations free cash is zero");
        let before = w.close_record(1);
        let cash0 = cash_of(w);
        let apply = apply_ix(w, 1);
        let applied = ok(w, "", vec![apply], &[]);
        let after = w.close_record(1);

        let event = applied.last_event::<PromotionApplied>();
        assert_eq!(event.winner, winner);
        let account = w.account(b_key).expect("the B account now exists");
        assert_eq!(account.owner, ID, "program-owned");
        assert_eq!(account.lamports, b_rent, "rent-exempt for exactly Backer::SPACE");
        let backer = w.backer_at(b_key);
        assert_eq!((backer.owner, backer.series), (winner, 1));
        assert_eq!(backer.entitlement, event.entitlement);
        let (spent_s, spent_o) = (
            after.spent.b_rent_s - before.spent.b_rent_s,
            after.spent.b_rent_o - before.spent.b_rent_o,
        );
        assert_eq!(spent_s + spent_o, b_rent, "the ledger records exactly the B rent");
        assert_eq!(
            (backer.rent_funded_by_ops, backer.rent_funded_by_owner),
            (spent_s, spent_o),
            "the account records its rent by source (DR-13)"
        );
        let registry_spent = (
            after.spent.registry_rent_s - before.spent.registry_rent_s,
            after.spent.registry_rent_o - before.spent.registry_rent_o,
        );
        assert_eq!(registry_spent.0 + registry_spent.1, registry_rent, "the empty registry's first page too");
        if sponsor.is_none() {
            assert_eq!((spent_s, spent_o), (b_rent, 0), "the sponsor partition paid");
            assert_eq!(
                w.ops().free_balance,
                before.locked_sponsor_remaining - b_rent - registry_rent,
                "the sponsor remainder came back at settlement"
            );
            assert_eq!(delta(&applied, dest), 0, "no owner partition to refund");
        } else {
            assert_eq!(before.locked_sponsor_remaining, 0, "the sponsor partition was spent exactly");
            assert_eq!((spent_s, spent_o), (0, b_rent), "the OWNER partition paid");
            assert_eq!(registry_spent, (0, registry_rent));
            assert_eq!(w.ops().free_balance, 0, "free cash stayed zero");
            assert_eq!(
                delta(&applied, dest),
                i(before.locked_owner_remaining - b_rent - registry_rent),
                "the owner's unspent remainder was refunded"
            );
            assert_eq!(
                cash0 - cash_of(w),
                b_rent + registry_rent + (before.locked_owner_remaining - b_rent - registry_rent),
                "OpsCash paid the two accounts and the refund"
            );
        }
        log.extend(scene.w.take_log());
    }
    log
}

// ---------------------------------------------------------------------------
// Row: registry_tail_full_extension_page_created_from_locked_reserve
// ---------------------------------------------------------------------------

/// Eight open Backers fill registry page 0, so the close reserves a registry
/// extension page; the promoted B Backer lands on slot 8 and the new page 1 is
/// created from the locked reserve at `apply_promotion` with
/// `ops.free_balance == 0`.
pub fn registry_tail_full_extension_page_created_from_locked_reserve() -> Vec<Observation> {
    let row = ROW_REGISTRY_TAIL;
    let registry_rent = rent_of(state::RegistryPage::SPACE);
    let backers: Vec<Pubkey> = (0..8u64).map(World::synthetic_backer).collect();
    let mut scene = make_scene(row, "", Spec::standard().with_backers(backers));
    scene.commit_cash();
    let reserve = scene.reserve;
    let dest = scene.dest;
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;

    let close = w.close_record(1);
    assert!(close.registry_extension_required, "registry_len 8 is a full page: the next row opens page 1");
    assert_eq!(w.pool().registry_len, 8, "no Backer was capped by the closing toll");
    assert_eq!(
        reserve,
        w.expected_reserve(20, 9, 0, 0) + registry_rent,
        "the reserve includes one registry extension page"
    );
    assert_eq!(w.ops().free_balance, 0);

    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let registry1 = w.rt.registry_address(1);
    assert!(w.account(registry1).is_none(), "registry page 1 does not exist before the promotion");
    assert_eq!(w.pool().registry_pages, 1);
    assert_eq!(w.ops().free_balance, 0, "Operations free cash is zero");

    let keys = apply_keys(w, 1);
    let b_key = keys.winner_backer_b.expect("B address");
    let before = w.close_record(1);
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    let after = w.close_record(1);

    let pool = w.pool();
    assert_eq!((pool.registry_len, pool.registry_pages), (9, 2), "one row appended, one page created");
    let account = w.account(registry1).expect("registry page 1 exists");
    assert_eq!(account.owner, ID);
    assert_eq!(account.lamports, registry_rent, "rent-exempt for exactly RegistryPage::SPACE");
    let page = w.registry(1);
    assert_eq!(page.index, 1);
    assert_eq!(page.keys[0], b_key, "the promoted B Backer is the first row of the new page");
    assert_eq!(page.rent.ops + page.rent.owner, registry_rent, "components sum to the rent");
    assert_eq!(page.rent.owner_refund, dest, "the owner component returns to the close's destination");
    let backer = w.backer_at(b_key);
    assert!(backer.active);
    assert_eq!(backer.slot, 8, "the dense registry's next slot");
    assert_eq!(
        (after.spent.registry_rent_s - before.spent.registry_rent_s)
            + (after.spent.registry_rent_o - before.spent.registry_rent_o),
        registry_rent,
        "the ledger records the extension page"
    );
    assert_eq!(page.rent.ops, after.spent.registry_rent_s - before.spent.registry_rent_s);
    assert_eq!(page.rent.owner, after.spent.registry_rent_o - before.spent.registry_rent_o);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution
// ---------------------------------------------------------------------------

/// The sponsor partition runs dry 40,000 lamports into the second candidate
/// page.  From there the OWNER's quoted opt-in funds, in order: the rest of
/// that page, the provider fee, the pending-request rent, the B account and
/// the registry page -- every cell of the spent ledger attributed to its true
/// source.  The consumed fee is never refunded; the owner-funded request rent
/// comes back ONLY through `withdraw_provider_return` after
/// `observe_request_fulfilled`; the unspent owner partition is refunded once.
pub fn owner_quoted_fallback_funds_storage_and_provider_costs_after_sponsor_with_exact_source_attribution(
) -> Vec<Observation> {
    let row = ROW_OWNER_QUOTED;
    let page = page_rent();
    let (rent, retained) = request_rents();
    let b_rent = rent_of(state::Backer::SPACE);
    let registry_rent = rent_of(state::RegistryPage::SPACE);
    let sponsor_part = 40_000u64;
    let sponsor = page + sponsor_part;
    let mut scene = make_scene(row, "", closer_wins(nineteen()).with_sponsor(sponsor));
    let (reserve, dest) = (scene.reserve, scene.dest);
    let owner_initial = reserve - sponsor;
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;

    // Storage: the first page is the sponsor's, the second is split.
    let close = w.close_record(1);
    assert_eq!(
        (close.spent.candidate_rent_s, close.spent.candidate_rent_o),
        (page + sponsor_part, page - sponsor_part),
        "candidate rent: sponsor first, the owner pays the rest"
    );
    assert_eq!(close.locked_sponsor_remaining, 0, "the sponsor partition is exhausted");
    assert_eq!(close.locked_owner_remaining, owner_initial - (page - sponsor_part));
    let schedule = (
        state::RentComponents { ops: page, owner: 0, owner_refund: dest },
        state::RentComponents { ops: sponsor_part, owner: page - sponsor_part, owner_refund: dest },
    );
    assert_eq!(candidate_page(w, 1, 0, 0).rent, schedule.0, "page 0 components");
    assert_eq!(candidate_page(w, 1, 1, 0).rent, schedule.1, "page 1 components record the split");

    // Provider costs: all owner.
    let before = w.close_record(1);
    let spend = pclose::plan_request_spend(
        before.locked_sponsor_remaining,
        before.locked_owner_remaining,
        REQUEST_FEE,
        rent,
        pclose::promotion_holdback(&before).expect("holdback"),
    )
    .expect("affordable from the owner partition");
    assert_eq!(
        spend,
        pclose::RequestSpend { fee_s: 0, fee_o: REQUEST_FEE, rent_s: 0, rent_o: rent }
    );
    do_request(w, 1, "request");
    let requested = w.close_record(1);
    assert_eq!((requested.spent.provider_fee_s, requested.spent.provider_fee_o), (0, REQUEST_FEE));
    assert_eq!((requested.spent.request_rent_s, requested.spent.request_rent_o), (0, rent));
    assert_eq!((requested.request_rent_funded_s, requested.request_rent_funded_o), (0, rent));
    assert_eq!(w.ops().provider_fee_consumed_total, u128::from(REQUEST_FEE), "the fee is consumed, whoever funded it");

    // Owner-funded rent is refundable ONLY through withdraw_provider_return
    // after observe_request_fulfilled; the owner-funded fee never is.
    w.fulfill("fulfill", 1).expect("fulfill");
    let dest0 = w.lamports(dest);
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let eligible = rent - retained;
    let observed = w.close_record(1);
    assert_eq!(observed.eligible_return_remaining, eligible);
    assert_eq!(
        observed.owner_request_rent_refundable_remaining,
        eligible,
        "attributed owner-first: the owner funded all of the rent"
    );
    assert_eq!(w.lamports(dest), dest0, "observing pays nothing");
    let (free0, cash0) = (w.ops().free_balance, cash_of(w));
    let withdraw = withdraw_ix(w, 1, eligible);
    ok_g(w, "return", withdraw);
    assert_eq!(w.lamports(dest) - dest0, eligible, "the owner receives the reimbursement ...");
    assert_eq!(w.ops().free_balance, free0, "... none of it reaches free cash ...");
    assert_eq!(cash_of(w), cash0, "... and OpsCash is net unchanged (in by G, out to the owner)");
    assert_eq!(w.ops().provider_fee_consumed_total, u128::from(REQUEST_FEE), "the fee was not refunded");

    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let before_apply = w.close_record(1);
    let dest1 = w.lamports(dest);
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    let done = w.close_record(1);

    // Storage after the promotion: the owner funded the B account and the
    // registry page too.
    assert_eq!(
        (done.spent.b_rent_s, done.spent.b_rent_o, done.spent.registry_rent_s, done.spent.registry_rent_o),
        (0, b_rent, 0, registry_rent),
        "B account and registry page: owner-funded, nothing from the exhausted sponsor partition"
    );
    // The whole ledger by source.
    assert_eq!(done.spent.candidate_rent_s + done.spent.provider_fee_s + done.spent.request_rent_s
        + done.spent.b_rent_s + done.spent.registry_rent_s, sponsor, "the sponsor funded exactly its partition");
    let spent_o = done.spent.candidate_rent_o + done.spent.provider_fee_o + done.spent.request_rent_o
        + done.spent.b_rent_o + done.spent.registry_rent_o;
    assert_eq!(spent_o, (page - sponsor_part) + REQUEST_FEE + rent + b_rent + registry_rent);
    let remainder = before_apply.locked_owner_remaining - b_rent - registry_rent;
    assert_eq!(w.lamports(dest) - dest1, remainder, "settlement refunded the owner's UNSPENT partition once");
    assert_eq!(done.locked_owner_remaining, 0);
    assert_eq!(spent_o + remainder, owner_initial, "owner spent + refunded == the owner partition");
    // What the owner finally bore: its storage, the consumed fee and the
    // retained fulfilled-request rent -- not the reimbursed rent.
    assert_eq!(
        spent_o - eligible,
        (page - sponsor_part) + REQUEST_FEE + retained + b_rent + registry_rent,
        "net owner cost = owner-funded storage + the consumed fee + the retained request rent"
    );
    assert_eq!(w.ops().free_balance, 0, "free cash was never involved");
    w.take_log()
}

// ---------------------------------------------------------------------------
// Partition audits
// ---------------------------------------------------------------------------

/// The books of close 1 and OpsCash at one instant.
pub(crate) struct Mark {
    pub(crate) close: state::CloseRecord,
    pub(crate) ops: state::Ops,
    pub(crate) cash: u64,
}

pub(crate) fn mark(w: &World) -> Mark {
    Mark { close: w.close_record(1), ops: w.ops(), cash: cash_of(w) }
}

/// One `LockedClose` spend of `debit`: both partitions, `L` and OpsCash fall by
/// exactly the debit, sponsor first (spec 8.3), and free cash is never the
/// source.  Returns `(sponsor part, owner part)`.
pub(crate) fn audit_spend(w: &World, before: &Mark, debit: u64) -> (u64, u64) {
    let (sponsor_part, owner_part) = split_sponsor_first(
        before.close.locked_sponsor_remaining,
        before.close.locked_owner_remaining,
        debit,
    );
    let after = mark(w);
    assert_eq!(
        after.close.locked_sponsor_remaining,
        before.close.locked_sponsor_remaining - sponsor_part,
        "sponsor partition falls first"
    );
    assert_eq!(
        after.close.locked_owner_remaining,
        before.close.locked_owner_remaining - owner_part,
        "owner partition pays only what the sponsor partition could not"
    );
    assert_eq!(after.ops.locked_remaining_total, before.ops.locked_remaining_total - debit, "L falls by the debit");
    assert_eq!(after.cash, before.cash - debit, "OpsCash falls by the debit");
    assert_eq!(after.ops.free_balance, before.ops.free_balance, "free cash is never the source");
    (sponsor_part, owner_part)
}

// ---------------------------------------------------------------------------
// Row: sponsor_partition_consumed_first_then_owner
// ---------------------------------------------------------------------------

/// Sponsor cash covers one and a half candidate pages.  Walking the whole
/// close, every spend (two pages, the request, the B account, the registry
/// page) is audited against the host model "sponsor first, then owner": the
/// owner partition does not move while the sponsor partition can pay, the
/// spent ledger gets exactly the split, and settlement refunds only the
/// owner's unspent remainder.
pub fn sponsor_partition_consumed_first_then_owner() -> Vec<Observation> {
    let row = ROW_SPONSOR_FIRST;
    let page = page_rent();
    let (rent, _) = request_rents();
    let b_rent = rent_of(state::Backer::SPACE);
    let registry_rent = rent_of(state::RegistryPage::SPACE);
    let sponsor = page + page / 2;
    let mut scene = make_scene(row, "", closer_wins(nineteen()).with_sponsor(sponsor));
    let dest = scene.dest;
    let w = &mut scene.w;
    assert_eq!(w.close_record(1).locked_sponsor_remaining, sponsor);

    // Page 1: entirely the sponsor's; the owner partition has not moved.
    let before = mark(w);
    let instruction = provision_ix(w, 1, 0, 0, Funding::LockedClose);
    ok(w, "provision_preferred0", vec![instruction], &[]);
    assert_eq!(audit_spend(w, &before, page), (page, 0));
    assert_eq!(
        w.close_record(1).locked_owner_remaining,
        before.close.locked_owner_remaining,
        "the owner partition did not move while the sponsor partition could pay"
    );

    // Page 2: the sponsor's last half page, then the owner.
    let before = mark(w);
    let instruction = provision_ix(w, 1, 1, 0, Funding::LockedClose);
    ok(w, "provision_fallback0", vec![instruction], &[]);
    assert_eq!(audit_spend(w, &before, page), (page / 2, page - page / 2));
    let close = w.close_record(1);
    assert_eq!(close.locked_sponsor_remaining, 0, "the sponsor partition is gone");
    assert_eq!(
        (close.spent.candidate_rent_s, close.spent.candidate_rent_o),
        (page + page / 2, page - page / 2)
    );

    seal_labelled(w, 1, 4, "build_last");
    assert_eq!(
        w.close_record(1).locked_owner_remaining,
        close.locked_owner_remaining,
        "building moves no cash"
    );

    // The request: fee and rent, all owner now.
    let before = mark(w);
    do_request(w, 1, "request");
    assert_eq!(audit_spend(w, &before, REQUEST_FEE + rent), (0, REQUEST_FEE + rent));
    let close = w.close_record(1);
    assert_eq!((close.spent.provider_fee_s, close.spent.provider_fee_o), (0, REQUEST_FEE));
    assert_eq!((close.spent.request_rent_s, close.spent.request_rent_o), (0, rent));

    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);

    // The promotion: B and registry from the owner partition, then the
    // owner's unspent remainder is refunded.
    let before = mark(w);
    let dest0 = w.lamports(dest);
    let apply = apply_ix(w, 1);
    let applied = ok(w, "", vec![apply], &[]);
    let after = w.close_record(1);
    assert_eq!(
        (after.spent.b_rent_s, after.spent.b_rent_o, after.spent.registry_rent_s, after.spent.registry_rent_o),
        (0, b_rent, 0, registry_rent)
    );
    let remainder = before.close.locked_owner_remaining - b_rent - registry_rent;
    assert_eq!(w.lamports(dest) - dest0, remainder, "settlement refunds only the unspent owner partition");
    assert_eq!(delta(&applied, dest), i(remainder));
    assert_eq!(w.ops().free_balance, 0, "the sponsor partition was fully spent: nothing returns to free");
    assert_eq!(w.ops().locked_remaining_total, 0);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation
// ---------------------------------------------------------------------------

/// Free cash above the reserve is NOT locked: the obligation is the reserve
/// whatever Operations holds, the spare stays free (and spendable by the
/// sponsor) through the whole close, an owner opt-in cap that is never needed
/// is never drawn, and settlement returns exactly `reserve - spent` on top of
/// the untouched spare.
pub fn spare_sponsor_cash_is_not_mistaken_for_a_smaller_locked_obligation() -> Vec<Observation> {
    let row = ROW_SPARE;
    let mut log = Vec::new();
    let reserve_of_standard = standard_reserve();

    for (tag, spare) in [("", 7_777_777u64), ("triple_the_reserve", 2 * reserve_of_standard)] {
        let mut scene = make_scene(row, tag, Spec::standard().with_spare(spare));
        let reserve = scene.reserve;
        assert_eq!(reserve, reserve_of_standard);
        let owner_key = scene.owner.pubkey();
        let w = &mut scene.w;
        let close = w.close_record(1);
        assert_eq!(close.required_reserve, reserve, "the obligation is the reserve, not the free cash");
        assert_eq!(
            (close.locked_sponsor_remaining, close.locked_owner_remaining),
            (reserve, 0),
            "exactly the reserve is locked"
        );
        assert_eq!(w.ops().free_balance, spare, "the spare is still free");
        assert_eq!(w.ops().locked_remaining_total, reserve);
        let owner_after_lock = w.lamports(owner_key);

        scene.provision_and_seal("build_last");
        let w = &mut scene.w;
        do_request(w, 1, "request");
        w.fulfill("fulfill", 1).expect("fulfill");
        let capture = capture_ix(w, 1);
        ok(w, "capture", vec![capture], &[]);
        assert_eq!(w.ops().free_balance, spare, "pages and the request never touched the spare");
        let apply = apply_ix(w, 1);
        ok(w, "", vec![apply], &[]);
        let close = w.close_record(1);
        assert_eq!(
            w.ops().free_balance,
            spare + (reserve - spent_total(&close.spent)),
            "free ends at the spare plus exactly the unspent reserve"
        );
        assert_eq!(w.ops().locked_remaining_total, 0);
        assert_eq!(w.lamports(owner_key), owner_after_lock, "the owner contributed nothing");
        log.extend(scene.w.take_log());
    }

    // An owner cap that is never needed is never drawn (a cap is a ceiling, not a target).
    let mut w = World::closing_fixture(9, 8).scoped_label(root(row, "owner_cap_never_drawn"));
    let owner = w.owner();
    let owner_key = owner.pubkey();
    let dest = w.dest;
    let reserve = w.expected_reserve(9, 8, 0, 0);
    w.set_ops_free(reserve + 1_000_000);
    let owner0 = w.lamports(owner_key);
    w.withdraw_simple("", &owner, 1, 0, dest, reserve).expect("closing payout with a cap that is not needed");
    assert_identities(&w);
    let record = w.close_record(1);
    assert_eq!((record.locked_sponsor_remaining, record.locked_owner_remaining), (reserve, 0));
    assert_eq!(w.ops().free_balance, 1_000_000);
    assert_eq!(w.lamports(owner_key), owner0, "the owner's cap was not drawn");
    log.extend(w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Row: uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages
// ---------------------------------------------------------------------------

/// The preferred set's pages (three, plus one more than the round can ever
/// need) are provisioned BEFORE the close; the fallback set has none.  The
/// reserve counts missing pages per set, so it locks exactly the three
/// fallback pages: the over-provisioned preferred set never offsets the
/// fallback set's gap, and the abandoned fourth page stays empty.
pub fn uneven_preprovisioning_preferred_over_fallback_absent_locks_only_fallback_pages() -> Vec<Observation> {
    let row = ROW_UNEVEN;
    let page = page_rent();
    let pre = [(0u8, Funding::Free), (0, Funding::Free), (0, Funding::Free), (0, Funding::Caller)];
    let (mut scene, _) = build_scene(row, "", "closing_payout", every_participant_a_preferred_loser(), &pre);
    let reserve = scene.reserve;
    let payer = scene.w.rt.payer.pubkey();
    let w = &mut scene.w;

    let close = w.close_record(1);
    assert_eq!((close.existing_preferred_pages, close.existing_fallback_pages), (4, 0));
    assert_eq!((close.provisioned_preferred_pages, close.provisioned_fallback_pages), (4, 0));
    assert_eq!(close.required_pages_per_set, 3);
    assert_eq!(
        reserve,
        standard_reserve() - 3 * page,
        "three fallback pages are locked; the preferred set's four pages offset nothing"
    );
    assert_eq!((close.locked_sponsor_remaining, close.locked_owner_remaining), (reserve, 0));
    assert_eq!(w.ops().free_balance, 0);
    // The pre-provisioned pages carry the components of whoever paid them.
    let free_page = state::RentComponents { ops: page, owner: 0, owner_refund: Pubkey::default() };
    for index in 0..3 {
        assert_eq!(candidate_page(w, 1, 0, index).rent, free_page, "preferred page {index}: Operations paid");
    }
    assert_eq!(
        candidate_page(w, 1, 0, 3).rent,
        state::RentComponents { ops: 0, owner: page, owner_refund: payer },
        "preferred page 3: the caller paid and is the refund destination"
    );

    // The preferred set may not draw on the reserve any more: its next index
    // is past the required pages.
    let extra = provision_ix(w, 1, 0, 4, Funding::LockedClose);
    reject(w, "preferred_locked_beyond_required", vec![extra], &[], ErrorCode::LockNotPermitted);
    // The fallback set builds from the reserve.
    for index in 0..3u64 {
        let before = mark(w);
        let instruction = provision_ix(w, 1, 1, index, Funding::LockedClose);
        ok(w, &format!("provision_fallback{index}"), vec![instruction], &[]);
        assert_eq!(audit_spend(w, &before, page), (page, 0));
    }
    assert_eq!(
        w.close_record(1).locked_sponsor_remaining,
        reserve - 3 * page,
        "exactly the three fallback pages were drawn"
    );
    assert_eq!(w.ops().free_balance, 0);

    seal_labelled(w, 1, 4, "build_last");
    let close = w.close_record(1);
    assert!(close.sealed);
    assert_eq!((close.preferred_count, close.fallback_count), (20, 20));
    let expected = expected_candidates(w, 1);
    assert_eq!(close.preferred_digest, chain(0, &expected.preferred));
    assert_eq!(close.fallback_digest, chain(1, &expected.fallback));
    for set in 0..=1u8 {
        for (index, len) in [(0u64, 8u8), (1, 8), (2, 4)] {
            assert_eq!(candidate_page(w, 1, set, index).len, len);
        }
    }
    assert_eq!(candidate_page(w, 1, 0, 3).len, 0, "the over-provisioned page stays empty");
    assert_eq!(close.spent.candidate_rent_s, 3 * page, "only the fallback pages were charged to the reserve");

    // The close completes from the reserve alone.
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    let close = w.close_record(1);
    assert_eq!(
        w.ops().free_balance,
        reserve - spent_total(&close.spent),
        "the sponsor partition's unspent remainder came back; nothing the close spent did"
    );
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds
// ---------------------------------------------------------------------------

/// Operations holds less than the reserve: ALL of it is locked (free drops to
/// zero), the owner opts in for the rest, and the close then builds both
/// candidate sets (six pages) from the locked partitions -- sponsor first,
/// then owner -- seals, draws and promotes with `ops.free_balance == 0` until
/// settlement.
pub fn all_free_ops_locked_then_locked_close_provisioning_builds_both_sets_without_free_funds(
) -> Vec<Observation> {
    let row = ROW_ALL_FREE_LOCKED;
    let page = page_rent();
    let free = 2 * page + 123;
    let (mut scene, facts) = build_scene(
        row,
        "",
        "closing_payout",
        every_participant_a_preferred_loser().with_sponsor(free),
        &[],
    );
    let reserve = scene.reserve;
    let w = &mut scene.w;
    assert_eq!(facts.before["free"], i(free));
    let close = w.close_record(1);
    assert_eq!(
        (close.locked_sponsor_remaining, close.locked_owner_remaining),
        (free, reserve - free),
        "all free cash locked, the owner opted in for the rest"
    );
    assert_eq!(w.ops().free_balance, 0, "no free cash is left");

    for set in 0..=1u8 {
        for index in 0..3u64 {
            let before = mark(w);
            let instruction = provision_ix(w, 1, set, index, Funding::LockedClose);
            let name = format!("provision_{}{index}", if set == 0 { "preferred" } else { "fallback" });
            ok(w, &name, vec![instruction], &[]);
            audit_spend(w, &before, page);
            assert_eq!(w.ops().free_balance, 0, "locked provisioning never needs free cash");
        }
    }
    let close = w.close_record(1);
    assert_eq!(close.spent.candidate_rent_s, free, "the whole sponsor partition went into pages");
    assert_eq!(close.spent.candidate_rent_o, 6 * page - free, "the owner paid the rest of the six pages");
    assert_eq!(close.locked_sponsor_remaining, 0);
    seal_labelled(w, 1, 4, "build_last");
    assert!(w.close_record(1).sealed);
    assert_eq!((w.close_record(1).preferred_count, w.close_record(1).fallback_count), (20, 20));
    assert_eq!(w.ops().free_balance, 0);

    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    assert_eq!(w.ops().free_balance, 0, "request and capture needed no free cash");
    let apply = apply_ix(w, 1);
    ok(w, "", vec![apply], &[]);
    assert!(w.close_record(1).reserve_settled);
    assert_eq!(w.ops().locked_remaining_total, 0);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination
// ---------------------------------------------------------------------------

/// Settlement at the terminal promotion refunds exactly what is unspent, once:
/// the sponsor remainder to `free_balance`, the owner remainder to the
/// immutable destination.  A replay of every closing instruction finds the
/// promotion terminal; finalization refunds nothing more.
///
/// `.deferred`: the owner's destination holds nothing when the promotion ends
/// (the owner moved the payout away with a real transfer) and the owner
/// remainder is below the rent-exempt minimum, so the runtime cannot deliver
/// it.  Expiry still succeeds, the remainder stays OWED on the `CloseRecord`
/// (counted in `locked_remaining_total`, never anonymous excess), finalization
/// does not need it, and `deliver_owner_refund` pays it exactly once later
/// (mirrors `tests/owner_refund.rs`).
pub fn unused_remaining_refunded_once_at_terminal_promotion_to_ops_and_owner_destination(
) -> Vec<Observation> {
    let row = ROW_UNUSED_REFUNDED;
    let mut log = Vec::new();
    let page = page_rent();
    let reserve = standard_reserve();

    // ---- Deliverable destination: paid at settlement ------------------------
    let owner_part = 5_000_000u64;
    let mut scene = make_scene(row, "", Spec::standard().with_sponsor(reserve - owner_part));
    let dest = scene.dest;
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let before = mark(w);
    let dest0 = w.lamports(dest);
    assert_eq!(before.close.locked_owner_remaining, owner_part);
    w.warp_past_deadline(1);
    // `expire_ok` checks the whole settlement: sponsor remainder -> free, owner
    // remainder -> destination, partitions and L to zero, the ledger retained.
    let expired = expire_ok(w, "", dest, pclose::STAGE_UNREQUESTED);
    assert_eq!(w.lamports(dest) - dest0, owner_part, "the owner's unspent partition reached the destination");
    assert_eq!(delta(&expired, dest), i(owner_part));
    assert_eq!(
        w.ops().free_balance,
        (reserve - owner_part) - 2 * page,
        "the sponsor's unspent partition (the reserve less the owner part and the two pages) returned to free"
    );
    assert_eq!(w.ops().locked_remaining_total, 0);
    assert_eq!(w.ops().unallocated_excess, before.ops.unallocated_excess);

    // Once: every closing instruction now finds the promotion terminal.
    let (free_after, dest_after, cash_after) = (w.ops().free_balance, w.lamports(dest), cash_of(w));
    let expire = expire_ix(w, 1);
    reject(w, "expire_again", vec![expire], &[], ErrorCode::PromotionTerminal);
    let apply = apply_ix_with(w, 1, ApplyKeys::default());
    reject(w, "apply_after_settlement", vec![apply], &[], ErrorCode::PromotionTerminal);
    let budget = budget_ix(w, 1, 1);
    reject(w, "budget_after_settlement", vec![budget], &[], ErrorCode::PromotionTerminal);
    let deliver = deliver_ix(w, 1, dest);
    reject(w, "deliver_nothing_deferred", vec![deliver], &[], ErrorCode::NothingDeferred);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    let expire = expire_ix(w, 1);
    reject(w, "expire_after_finalize", vec![expire], &[], ErrorCode::InvalidState);
    assert_eq!(
        (w.ops().free_balance, w.lamports(dest), cash_of(w)),
        (free_after, dest_after, cash_after),
        "no replay and no finalization refunded anything more"
    );
    log.extend(scene.w.take_log());

    // ---- `.deferred`: an empty destination, a sub-minimum owner remainder ------
    let owner_part = 500_000u64;
    let dest_key = destination_keypair(0xD7);
    let dest = dest_key.pubkey();
    let spec = Spec { dest: Some(dest), ..Spec::standard().with_sponsor(reserve - owner_part) };
    let mut deferred = make_scene(row, "deferred", spec);
    let attacker = deferred.w.attacker();
    let minimum = deferred.w.rt.rent(0);
    assert!(owner_part < minimum, "the scenario needs a sub-minimum remainder (minimum {minimum})");
    provision_candidate_pages(&mut deferred.w, 1);
    let w = &mut deferred.w;
    // The owner moves its payout elsewhere: a real transfer the destination signs.
    let paid_out = w.lamports(dest);
    assert!(paid_out > 0, "the closing payout funded the destination");
    let drain = transfer_ix(dest, attacker.pubkey(), paid_out);
    ok(w, "drain_destination", vec![drain], &[&dest_key]);
    assert_eq!(w.lamports(dest), 0);

    w.warp_past_deadline(1);
    let (before, ops_before) = (w.close_record(1), w.ops());
    assert_eq!(before.locked_owner_remaining, owner_part);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    let (close, ops) = (w.close_record(1), w.ops());
    assert!(close.reserve_settled, "settlement completes");
    assert_eq!(close.locked_sponsor_remaining, 0);
    assert_eq!(close.locked_owner_remaining, owner_part, "deferred, not forfeited: still owed on the CloseRecord");
    assert_eq!(ops.locked_remaining_total, owner_part, "only the deferred refund stays locked");
    assert_eq!(ops.free_balance, ops_before.free_balance + before.locked_sponsor_remaining);
    assert_eq!(ops.unallocated_excess, ops_before.unallocated_excess, "owner lamports never become anonymous excess");
    assert_eq!(w.lamports(dest), 0, "nothing was pushed at a destination that cannot receive it");

    // Finalization does not need the refund.
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert!(w.close_record(1).finalized);
    assert_eq!(w.close_record(1).locked_owner_remaining, owner_part, "still owed after finalization");
    assert_eq!(w.pool().round_id, 2, "the round reopened with the refund still owed");

    // Refused while the destination cannot take the credit; wrong destination refused.
    let deliver = deliver_ix(w, 1, dest);
    reject(w, "deliver_before_funding", vec![deliver], &[], ErrorCode::RefundNotDeliverable);
    let wrong = deliver_ix(w, 1, attacker.pubkey());
    reject(w, "deliver_wrong_destination", vec![wrong], &[], ErrorCode::WrongAccount);
    // The destination is funded (a real transfer of exactly the minimum), then anyone delivers, once.
    let fund = transfer_ix(attacker.pubkey(), dest, minimum);
    ok(w, "fund_destination", vec![fund], &[&attacker]);
    let (dest0, cash0) = (w.lamports(dest), cash_of(w));
    let deliver = deliver_ix(w, 1, dest);
    let delivered = ok(w, "deliver", vec![deliver], &[]);
    assert_eq!(w.lamports(dest) - dest0, owner_part, "the whole deferred refund reached the destination");
    assert_eq!(cash0 - cash_of(w), owner_part, "paid out of OpsCash");
    assert_eq!((w.close_record(1).locked_owner_remaining, w.ops().locked_remaining_total), (0, 0));
    let event = delivered.last_event::<OwnerRefundDelivered>();
    assert_eq!((event.round_id, event.close_id, event.amount, event.destination), (1, 1, owner_part, dest));
    let replay = deliver_ix(w, 1, dest);
    reject(w, "deliver_again", vec![replay], &[], ErrorCode::NothingDeferred);
    assert_eq!(w.lamports(dest) - dest0, owner_part, "delivered exactly once");
    log.extend(deferred.w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Row: client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return
// ---------------------------------------------------------------------------

/// The ORAO reimbursement lands in the CLIENT.  From fulfilment through
/// observation, both reconciles, capture, settlement and finalization it never
/// becomes free Operations cash and never reaches the owner destination; only
/// the executed `withdraw_provider_return` moves it, splitting it into the
/// owner's share (the rent the owner funded) and the rest to free cash.
pub fn client_reimbursement_never_becomes_free_ops_or_owner_refund_without_withdraw_provider_return(
) -> Vec<Observation> {
    let row = ROW_CLIENT_REIMBURSEMENT;
    let (rent, retained) = request_rents();
    let eligible = rent - retained;
    let owner_rent = 173_520u64;
    let mut scene = make_scene(row, "", Spec::standard().with_sponsor(sponsor_for_owner_rent(owner_rent)));
    let dest = scene.dest;
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");

    // Fulfilment: the reimbursement reaches the Client, and the books do not move.
    let (ops0, close0, cash0, dest0, client0) = (
        w.data(w.rt.ops_address()),
        w.data(w.rt.close_record_address(1)),
        cash_of(w),
        w.lamports(dest),
        w.client_lamports(),
    );
    w.fulfill("fulfill", 1).expect("fulfill");
    assert_identities(w);
    assert_eq!(w.client_lamports() - client0, eligible, "the Client holds the reimbursement");
    assert_eq!(w.data(w.rt.ops_address()), ops0, "Ops byte-identical: not free, not locked, not excess");
    assert_eq!(w.data(w.rt.close_record_address(1)), close0, "CloseRecord byte-identical");
    assert_eq!((cash_of(w), w.lamports(dest)), (cash0, dest0), "neither OpsCash nor the owner moved");

    // Observation freezes the eligibility; still no cash moves.
    let observe = observe_ix(w, 1);
    ok(w, "observe", vec![observe], &[]);
    let observed = w.close_record(1);
    assert_eq!(observed.eligible_return_remaining, eligible);
    assert_eq!(observed.owner_request_rent_refundable_remaining, owner_rent);
    let ops = w.ops();
    assert_eq!(ops.eligible_return_budget, eligible);
    assert_eq!(ops.free_balance, 0, "eligible is a budget, not cash");
    assert_eq!((cash_of(w), w.lamports(dest), w.client_lamports() - client0), (cash0, dest0, eligible));

    // Neither reconcile reclassifies the Client's lamports.
    let reconcile = reconcile_ix(&w.rt);
    ok(w, "reconcile", vec![reconcile], &[]);
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "reconcile_with_client", vec![reconcile], &[]);
    let ops = w.ops();
    assert_eq!((ops.free_balance, ops.unallocated_excess), (0, 0), "the Client's balance is observed, never minted");
    assert_eq!(ops.client_balance_observed, w.client_lamports());
    assert_eq!(ops.eligible_return_budget, eligible);

    // Capture, promotion and settlement: the owner gets its UNSPENT partition,
    // not the reimbursement; free gets the (here empty) sponsor remainder.
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let before = mark(w);
    let apply = apply_ix(w, 1);
    let applied = ok(w, "apply", vec![apply], &[]);
    let spent_now = w.close_record(1).spent;
    let storage = (spent_now.b_rent_s + spent_now.b_rent_o + spent_now.registry_rent_s + spent_now.registry_rent_o)
        - (before.close.spent.b_rent_s + before.close.spent.b_rent_o
            + before.close.spent.registry_rent_s + before.close.spent.registry_rent_o);
    assert_eq!(before.close.locked_sponsor_remaining, 0, "the sponsor partition was spent through the request");
    assert_eq!(
        delta(&applied, dest),
        i(before.close.locked_owner_remaining - storage),
        "the owner received only its unspent partition at settlement"
    );
    assert_eq!(w.ops().free_balance, 0, "free cash did not receive the reimbursement");
    assert_eq!(w.client_lamports() - client0, eligible, "the Client still holds it");
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    let after_close = w.close_record(1);
    assert_eq!(after_close.eligible_return_remaining, eligible, "still owed to the books, still in the Client");
    assert_eq!(after_close.owner_request_rent_refundable_remaining, owner_rent);
    assert_eq!(w.ops().unallocated_excess, 0);
    assert_eq!(client_spendable(w), eligible);

    // Only the executed withdrawal moves it: owner share to the destination,
    // the rest to free cash, the Client down by the whole amount.
    let (free1, dest1, client1) = (w.ops().free_balance, w.lamports(dest), w.client_lamports());
    let withdraw = withdraw_ix(w, 1, eligible);
    ok_g(w, "", withdraw);
    assert_eq!(w.lamports(dest) - dest1, owner_rent, "the owner's share: the rent the owner funded");
    assert_eq!(w.ops().free_balance - free1, eligible - owner_rent, "the rest reaches free cash");
    assert_eq!(client1 - w.client_lamports(), eligible, "out of the Client");
    assert_eq!(w.close_record(1).owner_request_rent_refundable_remaining, 0);
    assert_eq!(w.close_record(1).eligible_return_remaining, 0);
    assert_eq!(w.ops().eligible_return_budget, 0);
    w.take_log()
}

// ---------------------------------------------------------------------------
// Row: sponsor_withdrawal_cannot_touch_locked
// ---------------------------------------------------------------------------

/// `withdraw_operations` can only take `free_balance`: every amount above it
/// -- one lamport over, free plus the locked reserve, everything OpsCash
/// holds, `u64::MAX` -- is `InsufficientOperations`, a non-sponsor signer is
/// `NotAuthorized`, and after the sponsor takes ALL the free cash OpsCash still
/// holds exactly its rent floor plus the locked reserve.  The close then runs
/// to settlement with free cash zero; the settled sponsor remainder is
/// withdrawable but the deferred owner refund (`.deferred_owner_refund`) never
/// is.
pub fn sponsor_withdrawal_cannot_touch_locked() -> Vec<Observation> {
    let row = ROW_SPONSOR_WITHDRAWAL;
    let mut log = Vec::new();
    let spare = 2_000_000u64;
    let mut scene = make_scene(row, "", Spec::standard().with_spare(spare));
    let sponsor = scene.w.sponsor();
    let sponsor_key = sponsor.pubkey();
    let attacker = scene.w.attacker();
    let reserve = scene.reserve;
    let w = &mut scene.w;
    assert_eq!(w.ops().free_balance, spare);
    assert_eq!(w.ops().locked_remaining_total, reserve);

    let withdraw = withdraw_ops_ix(w, attacker.pubkey(), 1);
    reject(w, "not_the_sponsor", vec![withdraw], &[&attacker], ErrorCode::NotAuthorized);
    let everything = cash_of(w);
    for (variant, amount) in [
        ("one_over_free", spare + 1),
        ("free_plus_locked", spare + reserve),
        ("everything_in_opscash", everything),
        ("u64_max", u64::MAX),
    ] {
        let withdraw = withdraw_ops_ix(w, sponsor_key, amount);
        reject(w, variant, vec![withdraw], &[&sponsor], ErrorCode::InsufficientOperations);
    }

    let (sponsor0, cash0, locked0) = (w.lamports(sponsor_key), cash_of(w), w.ops().locked_remaining_total);
    let withdraw = withdraw_ops_ix(w, sponsor_key, spare);
    ok(w, "withdraw_all_free", vec![withdraw], &[&sponsor]);
    assert_eq!(w.lamports(sponsor_key) - sponsor0, spare, "the sponsor received the free cash");
    assert_eq!(cash0 - cash_of(w), spare);
    assert_eq!(w.ops().free_balance, 0);
    assert_eq!(w.ops().locked_remaining_total, locked0, "the lock is untouched");
    assert_eq!(
        cash_of(w),
        w.rt.rent(0) + reserve,
        "OpsCash keeps exactly its rent floor plus the locked reserve"
    );
    let withdraw = withdraw_ops_ix(w, sponsor_key, 1);
    reject(w, "after_drain", vec![withdraw], &[&sponsor], ErrorCode::InsufficientOperations);

    // The close runs on the locked reserve alone; the sponsor cannot strand it.
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    assert_eq!(w.ops().free_balance, 0);
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    // Settlement returned the sponsor remainder to free: now it is withdrawable (and only it).
    let free = w.ops().free_balance;
    assert!(free > 0);
    let withdraw = withdraw_ops_ix(w, sponsor_key, free + 1);
    reject(w, "settled_one_over", vec![withdraw], &[&sponsor], ErrorCode::InsufficientOperations);
    let withdraw = withdraw_ops_ix(w, sponsor_key, free);
    ok(w, "", vec![withdraw], &[&sponsor]);
    assert_eq!(
        cash_of(w),
        w.rt.rent(0) + w.ops().unallocated_excess + w.ops().locked_remaining_total,
        "nothing but the rent floor, excess and locks is left"
    );
    log.extend(scene.w.take_log());

    // ---- A deferred owner refund is locked, never sponsor-withdrawable ---------------
    let owner_part = 500_000u64;
    let dest_key = destination_keypair(0xD9);
    let dest = dest_key.pubkey();
    let spec = Spec { dest: Some(dest), ..Spec::standard().with_sponsor(standard_reserve() - owner_part) };
    let mut deferred = make_scene(row, "deferred_owner_refund", spec);
    let attacker = deferred.w.attacker();
    provision_candidate_pages(&mut deferred.w, 1);
    let w = &mut deferred.w;
    let paid_out = w.lamports(dest);
    let drain = transfer_ix(dest, attacker.pubkey(), paid_out);
    ok(w, "drain_destination", vec![drain], &[&dest_key]);
    w.warp_past_deadline(1);
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    assert_eq!(w.close_record(1).locked_owner_remaining, owner_part, "the refund is deferred");
    let free = w.ops().free_balance;
    let withdraw = withdraw_ops_ix(w, sponsor_key, free + owner_part);
    reject(w, "deferred_refund_is_not_free", vec![withdraw], &[&sponsor], ErrorCode::InsufficientOperations);
    let withdraw = withdraw_ops_ix(w, sponsor_key, free);
    ok(w, "withdraw_free", vec![withdraw], &[&sponsor]);
    assert_eq!(
        cash_of(w),
        w.rt.rent(0) + owner_part,
        "OpsCash holds exactly its rent floor plus the deferred owner refund"
    );
    assert_eq!(w.ops().locked_remaining_total, owner_part);
    log.extend(deferred.w.take_log());
    log
}

// ---------------------------------------------------------------------------
// Rows: cleanup predicates (R3)
// ---------------------------------------------------------------------------

/// A page's raw account, for byte-identity checks.
fn raw(w: &World, key: Pubkey) -> Vec<u8> {
    w.data(key)
}

/// `snapshot_pages` and `registry_pages` only grow and
/// `close_shared_account` refuses them whatever the `CloseRecord`: the permanent
/// pages are never closable, with a ticket active, with the ticket committed,
/// before the close is terminal and after the round is finalized, for the
/// pages the fixture installed AND for pages a caller paid for, and with the
/// kind named wrongly.  Every refusal is `NotClosable` (6033), fee-only, with
/// both pages and both counters byte-identical.
pub fn snapshot_and_registry_pages_are_not_closable() -> Vec<Observation> {
    let row = ROW_PERMANENT_PAGES;
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let mut scene = make_scene(row, "", Spec::standard().with_backers(vec![attacker.pubkey()]));
    let w = &mut scene.w;
    let payer = w.rt.payer.pubkey();
    for index in 0..=2 {
        w.watch(w.rt.registry_address(index));
        w.watch(w.rt.snapshot_address(index));
    }
    // Two more pages, paid by the caller: their owner component names the payer.
    let snapshot1 = w.rt.snapshot_address(1);
    let registry1 = w.rt.registry_address(1);
    let provision = provision_kind_ix(w, PageKind::Snapshot, 1, Funding::Caller, payer);
    ok(w, "provision_snapshot1_by_caller", vec![provision], &[]);
    let provision = provision_kind_ix(w, PageKind::Registry, 1, Funding::Caller, payer);
    ok(w, "provision_registry1_by_caller", vec![provision], &[]);
    assert_eq!((w.pool().snapshot_pages, w.pool().registry_pages), (2, 2));
    assert_eq!(w.snapshot_page(1).rent.owner_refund, payer, "the caller-paid page names its payer");

    let pages: [(&str, PageKind, Pubkey, Pubkey); 6] = [
        ("snapshot0", PageKind::Snapshot, w.rt.snapshot_address(0), attacker.pubkey()),
        ("snapshot1", PageKind::Snapshot, snapshot1, payer),
        ("registry0", PageKind::Registry, w.rt.registry_address(0), attacker.pubkey()),
        ("registry1", PageKind::Registry, registry1, payer),
        // The kind named wrongly is refused by the same predicate.
        ("registry0_as_snapshot", PageKind::Snapshot, w.rt.registry_address(0), attacker.pubkey()),
        ("snapshot0_as_registry", PageKind::Registry, w.rt.snapshot_address(0), attacker.pubkey()),
    ];

    let mut first = true;
    let mut attempt_all = |w: &mut World, phase: &str| {
        let before: Vec<Vec<u8>> = pages.iter().map(|(_, _, key, _)| raw(w, *key)).collect();
        let pool_before = w.data(w.rt.pool_address());
        for (name, kind, key, refund) in &pages {
            let variant = if first { String::new() } else { format!("{phase}.{name}") };
            first = false;
            let close = close_page_ix(w, *kind, *key, *refund, 1);
            reject(w, &variant, vec![close], &[], ErrorCode::NotClosable);
        }
        for ((name, _, key, _), bytes) in pages.iter().zip(&before) {
            assert_eq!(&raw(w, *key), bytes, "{name} is byte-identical after the refusals ({phase})");
        }
        assert_eq!(w.data(w.rt.pool_address()), pool_before, "no counter moved ({phase})");
        assert_eq!((w.pool().snapshot_pages, w.pool().registry_pages), (2, 2));
    };

    // The closing ticket is running; the close is pending.
    assert_eq!(w.pool().active_ticket, 1);
    attempt_all(w, "ticket_active_close_pending");
    // The ticket committed; the close is still pending.
    scene.commit_cash();
    let w = &mut scene.w;
    attempt_all(w, "ticket_committed_close_pending");
    // The close is terminal and the round finalized: the most permissive record there can be.
    w.warp_past_deadline(1);
    finish_by_expiry(w, 1);
    assert!(w.close_record(1).finalized);
    assert!(matches!(w.close_record(1).promotion, state::Promotion::Skipped { .. }));
    attempt_all(w, "close_terminal_round_finalized");
    scene.w.take_log()
}

/// A candidate page closes only against its OWN close record and only once
/// that record's promotion is terminal -- finalization is not required, and a
/// terminal record of ANOTHER close does not count.  On the closing path the
/// page's rent components return exactly per DR-13: the `ops` component to
/// `free_balance`, the `owner` component to the recorded owner destination
/// (a wrong destination is `WrongAccount`), whatever else the page holds to
/// `unallocated_excess`, `locked_remaining_total` and the lifetime rent
/// counter untouched; a second close finds nothing (`AlreadyClosed`).
pub fn candidate_pages_closable_only_after_their_own_close_is_terminal() -> Vec<Observation> {
    let row = ROW_CANDIDATE_CLOSABLE;
    let mut log = Vec::new();
    let page = page_rent();
    let sponsor_part = 40_000u64;
    let mut scene = make_scene(row, "", Spec::standard().with_sponsor(page + sponsor_part));
    let owner = scene.owner.insecure_clone();
    let dest = scene.dest;
    scene.provision_and_seal("build_last");
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    let (pref0, fall0) = (candidate_page_key(1, 0, 0), candidate_page_key(1, 1, 0));
    let (kind_p, kind_f) = (
        PageKind::Candidate { close_id: 1, set_tag: 0 },
        PageKind::Candidate { close_id: 1, set_tag: 1 },
    );
    assert_eq!(candidate_page(w, 1, 0, 0).rent, state::RentComponents { ops: page, owner: 0, owner_refund: dest });
    assert_eq!(
        candidate_page(w, 1, 1, 0).rent,
        state::RentComponents { ops: sponsor_part, owner: page - sponsor_part, owner_refund: dest }
    );

    // The promotion is Pending: neither page is closable.
    let close = close_page_ix(w, kind_p, pref0, dest, 1);
    reject(w, "pending_preferred", vec![close], &[], ErrorCode::NotClosable);
    let close = close_page_ix(w, kind_f, fall0, dest, 1);
    reject(w, "pending_fallback", vec![close], &[], ErrorCode::NotClosable);

    // Terminal (expired), NOT finalized: closable.
    w.warp_past_deadline(1);
    expire_ok(w, "expire", dest, pclose::STAGE_UNREQUESTED);
    assert!(!w.close_record(1).finalized, "terminal is enough");
    // The owner component must go to the recorded destination.
    let close = close_page_ix(w, kind_f, fall0, attacker.pubkey(), 1);
    reject(w, "wrong_refund_destination", vec![close], &[], ErrorCode::WrongAccount);
    // Unsolicited lamports on the page are excess, not a component.
    let donation = 1_234u64;
    let donate = transfer_ix(attacker.pubkey(), fall0, donation);
    ok(w, "donate_to_page", vec![donate], &[&attacker]);
    let before = mark(w);
    let (dest0, rent_funded0) = (w.lamports(dest), w.ops().total_rent_funded);
    assert_eq!(w.lamports(fall0), page + donation);
    let close = close_page_ix(w, kind_f, fall0, dest, 1);
    ok(w, "close_fallback_page", vec![close], &[]);
    let after = mark(w);
    assert!(w.account(fall0).is_none(), "the page is gone");
    assert_eq!(after.ops.free_balance - before.ops.free_balance, sponsor_part, "ops component -> free_balance");
    assert_eq!(w.lamports(dest) - dest0, page - sponsor_part, "owner component -> the recorded destination");
    assert_eq!(
        after.ops.unallocated_excess - before.ops.unallocated_excess,
        donation,
        "whatever else the page held -> unallocated_excess"
    );
    assert_eq!(after.cash - before.cash, sponsor_part + donation, "OpsCash gained the ops component and the excess");
    assert_eq!(after.ops.locked_remaining_total, before.ops.locked_remaining_total, "L untouched");
    assert_eq!(w.ops().total_rent_funded, rent_funded0, "closing never changes the lifetime rent counter");
    // A sponsor-only page ignores the refund account: it has no owner component.
    let before = mark(w);
    let close = close_page_ix(w, kind_p, pref0, attacker.pubkey(), 1);
    ok(w, "close_preferred_page", vec![close], &[]);
    let after = mark(w);
    assert_eq!(after.ops.free_balance - before.ops.free_balance, page);
    assert_eq!(after.cash - before.cash, page);
    assert!(w.account(pref0).is_none());
    // Once.
    let close = close_page_ix(w, kind_p, pref0, dest, 1);
    reject(w, "already_closed", vec![close], &[], ErrorCode::AlreadyClosed);
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);

    // ---- A second close: its pages answer only to ITS record ------------------------
    let dest2 = Pubkey::new_from_array([0xDF; 32]);
    start_second_close(w, &owner, 2, dest2, &[], 3);
    provision_candidate_pages(w, 2);
    let (pref2, fall2) = (candidate_page_key(2, 0, 0), candidate_page_key(2, 1, 0));
    let (kind_p2, kind_f2) = (
        PageKind::Candidate { close_id: 2, set_tag: 0 },
        PageKind::Candidate { close_id: 2, set_tag: 1 },
    );
    assert!(w.close_record(1).finalized, "close 1 is terminal AND finalized ...");
    // ... but that does not make close 2's pages closable, and neither does its own pending record.
    let close = close_page_ix(w, kind_p2, pref2, dest2, 2);
    reject(w, "close2_pending_own_record", vec![close], &[], ErrorCode::NotClosable);
    let close = close_page_ix(w, kind_p2, pref2, dest2, 1);
    reject(w, "close2_page_judged_by_close1_terminal_record", vec![close], &[], ErrorCode::NotClosable);
    let close = close_page_ix(w, kind_f2, fall2, dest2, 1);
    reject(w, "close2_fallback_judged_by_close1_terminal_record", vec![close], &[], ErrorCode::NotClosable);
    finish_by_expiry(w, 2);
    let close = close_page_ix(w, kind_p2, pref2, dest2, 1);
    reject(w, "close2_terminal_still_not_by_close1_record", vec![close], &[], ErrorCode::NotClosable);
    let before = mark(w);
    let close = close_page_ix(w, kind_p2, pref2, dest2, 2);
    ok(w, "", vec![close], &[]);
    assert!(w.account(pref2).is_none());
    assert_eq!(w.ops().free_balance - before.ops.free_balance, page, "close 2's page returns to free cash");
    log.extend(scene.w.take_log());
    log
}

/// A participant page closes only against the close record of ITS round and
/// only once that record is `finalized`: not while the promotion is pending,
/// not when it is merely terminal, and not for another round's pages.  The
/// three pages the fixture installed carry no rent components, so everything
/// they hold is excess; a page a caller paid for refunds its owner component to
/// that caller (a wrong destination is `WrongAccount`).
pub fn participant_pages_closable_only_after_their_round_finalized() -> Vec<Observation> {
    let row = ROW_PARTICIPANT_CLOSABLE;
    let mut scene = make_scene(row, "", Spec::standard());
    let dest = scene.dest;
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    let participant_rent = rent_of(state::ParticipantPage::SPACE);
    let kind = PageKind::Participant { round: 1 };
    // A REAL page 3 (the fixture holds pages 0..=2 for the twenty participants), paid by the caller.
    let page3 = w.rt.participant_address(1, 3);
    w.watch(page3);
    let provision = provision_kind_ix(w, kind, 3, Funding::Caller, attacker.pubkey());
    ok(w, "provision_page3_by_caller", vec![provision], &[&attacker]);
    assert_eq!(
        w.participants(1, 3).rent,
        state::RentComponents { ops: 0, owner: participant_rent, owner_refund: attacker.pubkey() }
    );
    let keys: Vec<Pubkey> = (0..4u64).map(|index| w.rt.participant_address(1, index)).collect();
    let refunds = [dest, dest, dest, attacker.pubkey()];

    let mut first = true;
    for phase in ["pending", "terminal_not_finalized"] {
        if phase == "terminal_not_finalized" {
            w.warp_past_deadline(1);
            expire_ok(w, "expire", dest, pclose::STAGE_CANDIDATES_INCOMPLETE);
            assert!(!w.close_record(1).finalized);
        }
        for (index, key) in keys.iter().enumerate() {
            let variant = if first { String::new() } else { format!("{phase}.page{index}") };
            first = false;
            let close = close_page_ix(w, kind, *key, refunds[index], 1);
            reject(w, &variant, vec![close], &[], ErrorCode::NotClosable);
        }
    }
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert!(w.close_record(1).finalized);

    // The caller-paid page: wrong destination refused, then refunded to the payer.
    let close = close_page_ix(w, kind, page3, dest, 1);
    reject(w, "page3_wrong_refund_destination", vec![close], &[], ErrorCode::WrongAccount);
    let before = mark(w);
    let attacker0 = w.lamports(attacker.pubkey());
    let close = close_page_ix(w, kind, page3, attacker.pubkey(), 1);
    ok(w, "close_page3", vec![close], &[]);
    let after = mark(w);
    assert_eq!(w.lamports(attacker.pubkey()) - attacker0, participant_rent, "owner component -> the payer");
    assert_eq!(after.cash, before.cash, "OpsCash unchanged: nothing was an ops component or excess");
    assert_eq!(after.ops.free_balance, before.ops.free_balance);
    assert!(w.account(page3).is_none());
    // The fixture pages hold no components: everything is excess.
    for (index, key) in keys.iter().enumerate().take(3) {
        let before = mark(w);
        assert_eq!(w.lamports(*key), participant_rent);
        let close = close_page_ix(w, kind, *key, dest, 1);
        ok(w, &format!("close_page{index}"), vec![close], &[]);
        let after = mark(w);
        assert_eq!(after.ops.unallocated_excess - before.ops.unallocated_excess, participant_rent, "fixture page: all excess");
        assert_eq!(after.ops.free_balance, before.ops.free_balance);
        assert_eq!(after.cash - before.cash, participant_rent);
        assert!(w.account(*key).is_none());
    }
    let close = close_page_ix(w, kind, keys[0], dest, 1);
    reject(w, "page0_already_closed", vec![close], &[], ErrorCode::AlreadyClosed);

    // Round binding: round 2's page is not judged by round 1's finalized record.
    assert_eq!(w.pool().round_id, 2);
    let round2 = w.rt.participant_address(2, 0);
    w.watch(round2);
    let payer = w.rt.payer.pubkey();
    let provision = provision_kind_ix(w, PageKind::Participant { round: 2 }, 0, Funding::Free, payer);
    ok(w, "provision_round2_page0", vec![provision], &[]);
    let close = close_page_ix(w, PageKind::Participant { round: 2 }, round2, dest, 1);
    reject(w, "round2_page_not_judged_by_the_round1_record", vec![close], &[], ErrorCode::NotClosable);
    scene.w.take_log()
}

// ---------------------------------------------------------------------------
// Row: old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts
// ---------------------------------------------------------------------------

/// Round 1's closing ticket (id 1) tags snapshot page 0.  After the round is
/// finalized (the most permissive record there is) a SECOND ticket starts from
/// a solvent payout in round 2; with page 0 still carrying the old tag, with
/// the new ticket discovering, and with it committed, NOTHING can close the
/// page: `Snapshot` and `Registry` kinds are `NotClosable`, and naming it a
/// `Participant` or `Candidate` page is `WrongAccount` (the decode fails)
/// even though the supplied record is terminal and finalized.
pub fn old_tag_snapshot_page_cannot_be_closed_after_new_ticket_starts() -> Vec<Observation> {
    let row = ROW_OLD_TAG;
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let mut scene = make_scene(row, "", Spec::standard().with_backers(vec![attacker.pubkey()]));
    let owner = scene.owner.insecure_clone();
    let dest = scene.dest;
    scene.commit_cash();
    let w = &mut scene.w;
    assert_eq!(w.snapshot_page(0).ticket_id, 1, "page 0 carries ticket 1's tag");
    w.warp_past_deadline(1);
    finish_by_expiry(w, 1);
    assert_eq!(w.pool().round_id, 2);

    // Round 2: a position, a day, a solvent payout: a toll with a Backer starts ticket 2.
    let payer = w.rt.payer.pubkey();
    let provision = provision_kind_ix(w, PageKind::Participant { round: 2 }, 0, Funding::Free, payer);
    ok(w, "provision_round2_page0", vec![provision], &[]);
    w.open_position("open_round2", &owner, PlanKindArg::Simple, SOL, 5).expect("open in round 2");
    assert_identities(w);
    w.warp_seconds(DAY);
    w.withdraw_simple("solvent_payout", &owner, 5, 0, dest, 0).expect("a solvent payout");
    assert_identities(w);
    assert_eq!(w.pool().active_ticket, 2, "ticket 2 started");
    assert_eq!(w.snapshot_page(0).ticket_id, 1, "page 0 still carries the OLD tag");

    let snapshot0 = w.rt.snapshot_address(0);
    let attempts: [(&str, PageKind, ErrorCode); 4] = [
        ("snapshot_kind", PageKind::Snapshot, ErrorCode::NotClosable),
        ("registry_kind", PageKind::Registry, ErrorCode::NotClosable),
        ("participant_kind", PageKind::Participant { round: 1 }, ErrorCode::WrongAccount),
        ("candidate_kind", PageKind::Candidate { close_id: 1, set_tag: 0 }, ErrorCode::WrongAccount),
    ];
    let mut first = true;
    for moment in ["old_tag_ticket2_started", "new_tag_ticket2_discovering", "ticket2_committed"] {
        match moment {
            "new_tag_ticket2_discovering" => {
                w.advance_step(8);
                assert_eq!(w.snapshot_page(0).ticket_id, 2, "the first Discover batch re-tagged the page");
            }
            "ticket2_committed" => {
                w.advance_until_idle(8);
                assert_eq!(w.pool().active_ticket, 0);
            }
            _ => {}
        }
        let bytes = raw(w, snapshot0);
        for (name, kind, expected) in attempts {
            let variant = if first { String::new() } else { format!("{moment}.{name}") };
            first = false;
            let close = close_page_ix(w, kind, snapshot0, attacker.pubkey(), 1);
            reject(w, &variant, vec![close], &[], expected);
        }
        assert_eq!(raw(w, snapshot0), bytes, "the page is byte-identical ({moment})");
        assert_eq!(w.pool().snapshot_pages, 1, "the counter never decrements");
    }
    scene.w.take_log()
}

// ---------------------------------------------------------------------------
// Row: full_cash_progress_with_depleted_free_ops_funds
// ---------------------------------------------------------------------------

/// Operations holds ONE million lamports at the closing payout (all of it
/// locked, so `free_balance` is zero from then on), and the whole close still
/// progresses: the closing ticket commits, a Backer claims, six candidate pages
/// are built from the locked reserve, the draw is requested, captured and
/// applied, the round is finalized, and -- the owner's destination having been
/// emptied -- the sub-minimum owner remainder is DEFERRED and then paid by
/// `deliver_owner_refund`.  `ops.free_balance == 0` before, at and after every
/// one of those instructions.
pub fn full_cash_progress_with_depleted_free_ops_funds() -> Vec<Observation> {
    let row = ROW_DEPLETED_FREE;
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let dest_key = destination_keypair(0xD8);
    let dest = dest_key.pubkey();
    let spec = Spec {
        dest: Some(dest),
        ..closer_wins((0..19u128).map(|index| loser(100_000_000 + 10_000_000 * index)).collect())
            .with_backers(vec![attacker.pubkey()])
            .with_sponsor(1_000_000)
    };
    let mut scene = make_scene(row, "", spec);
    let reserve = scene.reserve;
    scene.commit_cash();
    let w = &mut scene.w;
    assert_eq!(w.ops().free_balance, 0, "every free lamport is locked");
    assert!(w.close_record(1).cash_committed, "the closing ticket committed with free cash zero");

    // A Backer claims with free cash zero.
    let claimed = w
        .claim_backer("claim", &attacker, Pubkey::new_from_array([0xE1; 32]))
        .unwrap_or_else(|failed| panic!("claim failed: {failed:?}"));
    assert_identities(w);
    assert!(claimed > 0, "the Backer was paid");
    assert_eq!(w.ops().free_balance, 0);

    // Both candidate sets from the locked reserve; the draw and its capture.
    // 19 history-free losers fill three candidate pages per set, and one
    // build_candidates call carries ONE page per set, so a batch must not
    // straddle a page edge (the program reports a straddle as
    // PageNotProvisioned, M8-F2); one participant per build keeps every batch
    // inside a page.
    provision_candidate_pages(w, 1);
    seal_labelled(w, 1, 1, "build_last");
    assert_eq!(w.ops().free_balance, 0);
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    assert_eq!(w.ops().free_balance, 0);

    // The owner has moved its payout away; the promotion applies and settles.
    let attacker_key = attacker.pubkey();
    let paid_out = w.lamports(dest);
    let drain = transfer_ix(dest, attacker_key, paid_out);
    ok(w, "drain_destination", vec![drain], &[&dest_key]);
    assert_eq!(w.lamports(dest), 0);
    let before = mark(w);
    let apply = apply_ix(w, 1);
    ok(w, "apply", vec![apply], &[]);
    let close = w.close_record(1);
    assert!(close.reserve_settled);
    assert_eq!(close.locked_sponsor_remaining, 0, "the sponsor partition was spent");
    assert_eq!(
        close.locked_owner_remaining,
        reserve - spent_total(&close.spent),
        "the deferred remainder is the unspent reserve (here the fee headroom)"
    );
    assert!(close.locked_owner_remaining > 0 && close.locked_owner_remaining < w.rt.rent(0));
    assert_eq!(w.lamports(dest), 0, "undeliverable to an empty destination: deferred");
    assert_eq!(w.ops().free_balance, 0, "settlement returned nothing to free: the sponsor partition was spent");
    assert_eq!(w.ops().locked_remaining_total, close.locked_owner_remaining);
    assert_eq!(before.ops.unallocated_excess, w.ops().unallocated_excess);

    // Finalization with free cash zero, then the deferred refund with free cash zero.
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert_eq!(w.pool().round_id, 2);
    assert_eq!(w.ops().free_balance, 0);
    let owed = w.close_record(1).locked_owner_remaining;
    let deliver = deliver_ix(w, 1, dest);
    reject(w, "deliver_before_funding", vec![deliver], &[], ErrorCode::RefundNotDeliverable);
    let minimum = w.rt.rent(0);
    let fund = transfer_ix(attacker_key, dest, minimum);
    ok(w, "fund_destination", vec![fund], &[&attacker]);
    let dest0 = w.lamports(dest);
    let deliver = deliver_ix(w, 1, dest);
    let delivered = ok(w, "", vec![deliver], &[]);
    assert_eq!(w.lamports(dest) - dest0, owed);
    assert_eq!(delivered.last_event::<OwnerRefundDelivered>().amount, owed);
    assert_eq!((w.ops().free_balance, w.ops().locked_remaining_total), (0, 0));
    assert_eq!(w.close_record(1).locked_owner_remaining, 0);
    scene.w.take_log()
}

// ---------------------------------------------------------------------------
// Row: cleanup_is_never_required_for_commit_finalize_or_open
// ---------------------------------------------------------------------------

/// A full close -- closing ticket commit, a claim, candidate pages abandoned
/// unclosed, expiry, finalization -- and the next round's first `open_position`
/// all run with NO `close_shared_account` call ever made: every page of the
/// finished round is still there, byte for byte.
pub fn cleanup_is_never_required_for_commit_finalize_or_open() -> Vec<Observation> {
    let row = ROW_NO_CLEANUP;
    let attacker = Keypair::new_from_array(crate::runtime::ATTACKER_SEED);
    let mut scene = make_scene(row, "", Spec::standard().with_backers(vec![attacker.pubkey()]));
    let owner = scene.owner.insecure_clone();
    scene.commit_cash();
    let w = &mut scene.w;
    provision_candidate_pages(w, 1);
    let claimed = w
        .claim_backer("claim", &attacker, Pubkey::new_from_array([0xE1; 32]))
        .unwrap_or_else(|failed| panic!("claim failed: {failed:?}"));
    assert!(claimed > 0);
    w.warp_past_deadline(1);
    finish_by_expiry(w, 1);
    assert_eq!(w.pool().round_id, 2, "finalized and reopened");

    // The finished round's pages are all still here.
    let leftovers: Vec<Pubkey> = (0..3u64)
        .map(|index| w.rt.participant_address(1, index))
        .chain([candidate_page_key(1, 0, 0), candidate_page_key(1, 1, 0)])
        .collect();
    let snapshot: Vec<Vec<u8>> = leftovers.iter().map(|key| raw(w, *key)).collect();
    for (key, bytes) in leftovers.iter().zip(&snapshot) {
        assert!(!bytes.is_empty(), "{key} exists: nothing was cleaned up");
    }

    // The next round opens: its first page, its first position -- no cleanup.
    let payer = w.rt.payer.pubkey();
    let provision = provision_kind_ix(w, PageKind::Participant { round: 2 }, 0, Funding::Free, payer);
    ok(w, "provision_round2_page0", vec![provision], &[]);
    w.open_position("", &owner, PlanKindArg::Simple, SOL, 5).expect("open in the reopened round");
    assert_identities(w);
    assert_eq!((w.pool().round_id, w.pool().participant_count), (2, 1));
    for (key, bytes) in leftovers.iter().zip(&snapshot) {
        assert_eq!(&raw(w, *key), bytes, "{key} is still untouched after the next round's first open");
    }
    scene.w.take_log()
}

// ---------------------------------------------------------------------------
// Row: conservation_transitions_table_8_6_holds_for_every_row
// ---------------------------------------------------------------------------

/// Every row of the spec 8.6 conservation-transitions table (including the
/// settlement *undeliverable* variant, `deliver_owner_refund`, and the refused
/// `withdraw_provider_return`), each DRIVEN for real in a closing scene and
/// checked as an exact delta: OpsCash lamports, `free`, `s`, `o`, `L` and every
/// named counter (any figure not listed in a row did not move).  Rows are
/// labelled `...table_8_6.<row-name>`.  The figures of every row are attached
/// to the headline observation as JSON.
///
/// `apply_promotion` spends the B account and registry rent AND settles in one
/// instruction; its row is the table's spend row followed by the table's
/// settlement row computed from the partitions the spends left.
pub fn conservation_transitions_table_8_6_holds_for_every_row() -> Vec<Observation> {
    let row = ROW_TABLE_8_6;
    let mut table: Vec<serde_json::Value> = Vec::new();
    let mut log = table_world_a(row, &mut table);
    log.extend(table_world_split(row, &mut table));
    log.extend(table_world_settle(row, &mut table));
    log.extend(table_world_deferred(row, &mut table));
    log.extend(table_world_cpi_failure(row, &mut table));
    crate::close::annotate(
        &mut log,
        &root(row, ""),
        format!("conservation_table_8_6: {}", serde_json::Value::Array(table)),
    );
    log
}

/// World A: owner-funded partly, deliverable keypair destination.  The full
/// lifecycle: payout, pages, budget, request, fulfilment, observation, a G
/// withdrawal elsewhere (deficit), an anonymous credit, a refused return, the
/// real return, capture, promotion + settlement, finalization.
fn table_world_a(row: &str, table: &mut Vec<serde_json::Value>) -> Vec<Observation> {
    let page = page_rent();
    let (rent, retained) = request_rents();
    let eligible = rent - retained;
    let (b_rent, registry_rent) = (rent_of(state::Backer::SPACE), rent_of(state::RegistryPage::SPACE));
    let cap = REQUEST_FEE + REQUEST_FEE * 2_000 / 10_000;
    let dest_key = destination_keypair(0xD5);
    let dest = dest_key.pubkey();
    let sponsor0 = sponsor_for_owner_rent(173_520);
    let spec = Spec { dest: Some(dest), ..closer_wins(nineteen()).with_sponsor(sponsor0) };
    let (mut scene, facts) =
        build_scene(row, "", "table_8_6.closing_payout_locks_reserve", spec, &[]);
    let (reserve, owner_key, net) = (scene.reserve, scene.owner.pubkey(), scene.payout.net);
    let attacker = scene.w.attacker();
    let w = &mut scene.w;
    let keys = provider_keys();
    let g_vault = w.rt.g_vault();
    let cash_key = w.rt.ops_cash_address();
    let contribution = reserve - sponsor0;
    assert!(contribution > 0, "the owner opted in");

    // ---- closing payout locks R -----------------------------------------------------
    let after = tracked(w, 1, &[("owner", owner_key), ("dest", dest)]);
    assert_row(
        "closing_payout_locks_reserve",
        &facts.before,
        &after,
        &[
            ("cash", i(contribution)),
            ("free", -i(sponsor0)),
            ("s", i(sponsor0)),
            ("o", i(contribution)),
            ("L", i(reserve)),
            ("owner", -i(contribution)),
            ("dest", i(net)),
            ("required_reserve", i(reserve)),
            ("approved_cap", i(cap)),
        ],
    );
    assert_eq!(delta(&facts.observation, cash_key), i(contribution), "OpsCash +c, from the observation itself");
    assert_eq!(delta(&facts.observation, owner_key), -i(contribution), "owner wallet -c");
    table.push(row_json("closing_payout_locks_reserve", &facts.before, &after));

    let base: Vec<(&'static str, Pubkey)> = vec![
        ("owner", owner_key),
        ("dest", dest),
        ("client", keys.client),
        ("treasury", keys.treasury),
        ("g_vault", g_vault),
    ];

    // ---- provision_page(LockedClose), sponsor part covers it ------------------------------
    for (set, name) in [(0u8, "preferred0"), (1u8, "fallback0")] {
        let before = tracked(w, 1, &base);
        let instruction = provision_ix(w, 1, set, 0, Funding::LockedClose);
        ok(w, &format!("table_8_6.provision_page_locked_close.sponsor_only.{name}"), vec![instruction], &[]);
        let after = tracked(w, 1, &base);
        let provisioned = if set == 0 { "provisioned_preferred" } else { "provisioned_fallback" };
        assert_row(
            "provision_page_locked_close",
            &before,
            &after,
            &[
                ("cash", -i(page)),
                ("s", -i(page)),
                ("L", -i(page)),
                ("candidate_rent_s", i(page)),
                (provisioned, 1),
                ("total_rent_funded", i(page)),
            ],
        );
        table.push(row_json(&format!("provision_page_locked_close.sponsor_only.{name}"), &before, &after));
    }
    // Building moves no cash.
    let before = tracked(w, 1, &base);
    seal_labelled(w, 1, 4, "build_last");
    assert_row("build_candidates", &before, &tracked(w, 1, &base), &[]);

    // ---- fund_operations (not a table row: free +x, OpsCash +x) ------------------------------
    let before = tracked(w, 1, &base);
    let fund = fund_ops_ix(w, attacker.pubkey(), 1_000_000);
    ok(w, "table_8_6.fund_operations", vec![fund], &[&attacker]);
    assert_row("fund_operations", &before, &tracked(w, 1, &base), &[("cash", 1_000_000), ("free", 1_000_000)]);

    // ---- increase_provider_budget(extra) ----------------------------------------------------
    let extra = 100_000u64;
    let before = tracked(w, 1, &base);
    let budget = budget_ix(w, 1, extra);
    ok(w, "table_8_6.increase_provider_budget", vec![budget], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "increase_provider_budget",
        &before,
        &after,
        &[
            ("free", -i(extra)),
            ("s", i(extra)),
            ("L", i(extra)),
            ("approved_cap", i(extra)),
            ("required_reserve", i(extra)),
        ],
    );
    table.push(row_json("increase_provider_budget", &before, &after));

    // ---- request_round_draw (the CPI succeeds) -------------------------------------------------
    let request_key = host_request(w, 1).0;
    let mut with_request = base.clone();
    with_request.push(("request", request_key));
    let before = tracked(w, 1, &with_request);
    let close_b = w.close_record(1);
    let spend = pclose::plan_request_spend(
        close_b.locked_sponsor_remaining,
        close_b.locked_owner_remaining,
        REQUEST_FEE,
        rent,
        pclose::promotion_holdback(&close_b).expect("holdback"),
    )
    .expect("the request is affordable");
    do_request(w, 1, "table_8_6.request_round_draw");
    let after = tracked(w, 1, &with_request);
    let (sponsor_spend, owner_spend) = (spend.fee_s + spend.rent_s, spend.fee_o + spend.rent_o);
    assert_row(
        "request_round_draw",
        &before,
        &after,
        &[
            ("cash", -i(REQUEST_FEE + rent)),
            ("s", -i(sponsor_spend)),
            ("o", -i(owner_spend)),
            ("L", -i(REQUEST_FEE + rent)),
            ("provider_fee_s", i(spend.fee_s)),
            ("provider_fee_o", i(spend.fee_o)),
            ("request_rent_s", i(spend.rent_s)),
            ("request_rent_o", i(spend.rent_o)),
            ("request_rent_funded_s", i(spend.rent_s)),
            ("request_rent_funded_o", i(spend.rent_o)),
            ("fee_consumed_total", i(REQUEST_FEE)),
            ("rent_outstanding", i(rent)),
            ("treasury", i(REQUEST_FEE)),
            ("request", i(rent)),
        ],
    );
    table.push(row_json("request_round_draw", &before, &after));
    assert!(spend.rent_o > 0, "the owner partition funds part of the rent");

    // ---- ORAO fulfilment reimburses the Client ------------------------------------------------------
    let before = tracked(w, 1, &with_request);
    w.fulfill("table_8_6.orao_fulfillment_reimburses_client", 1).expect("fulfill");
    assert_identities(w);
    let after = tracked(w, 1, &with_request);
    assert_row(
        "orao_fulfillment_reimburses_client",
        &before,
        &after,
        &[("client", i(eligible)), ("request", -i(eligible))],
    );
    table.push(row_json("orao_fulfillment_reimburses_client", &before, &after));

    // ---- observe_request_fulfilled -----------------------------------------------------------------------
    let funded_o = w.close_record(1).request_rent_funded_o;
    let refundable = funded_o.min(eligible);
    let before = tracked(w, 1, &with_request);
    let observe = observe_ix(w, 1);
    ok(w, "table_8_6.observe_request_fulfilled", vec![observe], &[]);
    let after = tracked(w, 1, &with_request);
    assert_row(
        "observe_request_fulfilled",
        &before,
        &after,
        &[
            ("rent_outstanding", -i(rent)),
            ("eligible_return_remaining", i(eligible)),
            ("eligible_return_budget", i(eligible)),
            ("owner_refundable_remaining", i(refundable)),
            ("reimbursement_observed", 1),
        ],
    );
    table.push(row_json("observe_request_fulfilled", &before, &after));

    // ---- G withdraws the Client elsewhere --------------------------------------------------------------------
    let drain = client_spendable(w);
    assert_eq!(drain, eligible);
    let before = tracked(w, 1, &with_request);
    let raw_withdraw = raw_withdraw_ix(w, drain);
    ok_g(w, "table_8_6.g_withdraws_client_elsewhere", raw_withdraw);
    let after = tracked(w, 1, &with_request);
    assert_row(
        "g_withdraws_client_elsewhere",
        &before,
        &after,
        &[("client", -i(drain)), ("g_vault", i(drain))],
    );
    table.push(row_json("g_withdraws_client_elsewhere", &before, &after));
    // The deficit is recomputed on the next observation of the Client; nothing else moves.
    let before = tracked(w, 1, &with_request);
    let reconcile = reconcile_client_ix(&w.rt);
    ok(w, "table_8_6.g_withdraws_client_elsewhere.reconcile", vec![reconcile], &[]);
    let after = tracked(w, 1, &with_request);
    let (client_now, client_rent) = (w.client_lamports(), w.rt.rent(w.account(keys.client).expect("Client").data.len()));
    let deficit = pclose::provider_deficit(w.ops().eligible_return_budget, client_now, client_rent);
    assert_eq!(deficit, eligible, "the whole reimbursement is missing from the Client");
    assert_row(
        "g_withdraws_client_elsewhere.reconcile",
        &before,
        &after,
        &[
            ("client_balance_observed", i(client_now) - before["client_balance_observed"]),
            ("client_protected_rent", i(client_rent) - before["client_protected_rent"]),
            ("provider_deficit", i(deficit) - before["provider_deficit"]),
        ],
    );
    table.push(row_json("g_withdraws_client_elsewhere.reconcile", &before, &after));
    // Someone puts the money back (an anonymous Client credit never raises any budget).
    let before = tracked(w, 1, &with_request);
    let refill = transfer_ix(attacker.pubkey(), keys.client, drain);
    ok(w, "refill_client", vec![refill], &[&attacker]);
    assert_row("anonymous_client_credit", &before, &tracked(w, 1, &with_request), &[("client", i(drain))]);

    // ---- anonymous OpsCash credit x (+ reconcile) ------------------------------------------------------------------
    let x = 777_000u64;
    let before = tracked(w, 1, &with_request);
    let credit = transfer_ix(attacker.pubkey(), cash_key, x);
    let reconcile = reconcile_ix(&w.rt);
    ok(w, "table_8_6.anonymous_ops_cash_credit", vec![credit, reconcile], &[&attacker]);
    let after = tracked(w, 1, &with_request);
    assert_row("anonymous_ops_cash_credit", &before, &after, &[("cash", i(x)), ("unallocated_excess", i(x))]);
    table.push(row_json("anonymous_ops_cash_credit", &before, &after));

    // ---- withdraw_provider_return refused: the owner share is undeliverable ---------------------------------------------
    let drained = w.lamports(dest);
    let drain_dest = transfer_ix(dest, attacker.pubkey(), drained);
    ok(w, "drain_destination", vec![drain_dest], &[&dest_key]);
    assert_eq!(w.lamports(dest), 0);
    let before = tracked(w, 1, &with_request);
    let withdraw = withdraw_ix(w, 1, eligible);
    reject_g(w, "table_8_6.withdraw_provider_return_undeliverable_refused", withdraw, ErrorCode::RefundNotDeliverable);
    let after = tracked(w, 1, &with_request);
    assert_row("withdraw_provider_return_undeliverable_refused", &before, &after, &[]);
    table.push(row_json("withdraw_provider_return_undeliverable_refused", &before, &after));
    let minimum = w.rt.rent(0);
    let refund_dest = transfer_ix(attacker.pubkey(), dest, minimum);
    ok(w, "refund_destination", vec![refund_dest], &[&attacker]);

    // ---- withdraw_provider_return(amount), owner share w ---------------------------------------------------------------------
    let share = eligible.min(w.close_record(1).owner_request_rent_refundable_remaining);
    assert_eq!(share, refundable);
    let before = tracked(w, 1, &with_request);
    let withdraw = withdraw_ix(w, 1, eligible);
    ok_g(w, "table_8_6.withdraw_provider_return", withdraw);
    let after = tracked(w, 1, &with_request);
    let client_after = w.client_lamports();
    assert_row(
        "withdraw_provider_return",
        &before,
        &after,
        &[
            ("cash", i(eligible - share)),
            ("free", i(eligible - share)),
            ("eligible_return_remaining", -i(eligible)),
            ("eligible_return_budget", -i(eligible)),
            ("owner_refundable_remaining", -i(share)),
            ("rent_returned", i(eligible)),
            ("returned_total", i(eligible)),
            ("client", -i(eligible)),
            ("dest", i(share)),
            ("client_balance_observed", i(client_after) - before["client_balance_observed"]),
            ("provider_deficit", -before["provider_deficit"]),
        ],
    );
    table.push(row_json("withdraw_provider_return", &before, &after));

    // ---- capture (no cash), then apply_promotion + settlement -----------------------------------------------------------------
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let mark_before = mark(w);
    let before = tracked(w, 1, &with_request);
    let (s, o) = (mark_before.close.locked_sponsor_remaining, mark_before.close.locked_owner_remaining);
    let (sb, ob) = split_sponsor_first(s, o, b_rent);
    let (sg, og) = split_sponsor_first(s - sb, o - ob, registry_rent);
    let (s2, o2) = (s - sb - sg, o - ob - og);
    let apply = apply_ix(w, 1);
    ok(w, "table_8_6.apply_promotion_then_settlement", vec![apply], &[]);
    let after = tracked(w, 1, &with_request);
    assert_row(
        "apply_promotion_then_settlement",
        &before,
        &after,
        &[
            ("cash", -i(b_rent + registry_rent + o2)),
            ("free", i(s2)),
            ("s", -i(s)),
            ("o", -i(o)),
            ("L", -i(s + o)),
            ("b_rent_s", i(sb)),
            ("b_rent_o", i(ob)),
            ("registry_rent_s", i(sg)),
            ("registry_rent_o", i(og)),
            ("total_rent_funded", i(b_rent + registry_rent)),
            ("reserve_settled", 1),
            ("dest", i(o2)),
        ],
    );
    table.push(row_json("apply_promotion_then_settlement", &before, &after));

    // ---- finalize: no cash, the headline observation ---------------------------------------------------------------------------------
    let before = tracked(w, 1, &with_request);
    let finalize = finalize_ix(w, 1);
    ok(w, "", vec![finalize], &[]);
    assert_row("finalize_round", &before, &tracked(w, 1, &with_request), &[]);
    assert_eq!((w.ops().locked_remaining_total, mark(w).close.locked_owner_remaining), (0, 0));
    scene.w.take_log()
}

/// World B (split): the sponsor partition runs dry INSIDE the second candidate
/// page, so that provisioning row draws on both partitions; the close then
/// expires with only the owner partition left (settlement with `s == 0`).
fn table_world_split(row: &str, table: &mut Vec<serde_json::Value>) -> Vec<Observation> {
    let page = page_rent();
    let sponsor_part = 40_000u64;
    let mut scene = make_scene(row, "split", Spec::standard().with_sponsor(page + sponsor_part));
    let (owner_key, dest) = (scene.owner.pubkey(), scene.dest);
    let w = &mut scene.w;
    let base: Vec<(&'static str, Pubkey)> = vec![("owner", owner_key), ("dest", dest)];

    let instruction = provision_ix(w, 1, 0, 0, Funding::LockedClose);
    ok(w, "provision_preferred0", vec![instruction], &[]);
    let before = tracked(w, 1, &base);
    let instruction = provision_ix(w, 1, 1, 0, Funding::LockedClose);
    ok(w, "table_8_6.provision_page_locked_close.split", vec![instruction], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "provision_page_locked_close.split",
        &before,
        &after,
        &[
            ("cash", -i(page)),
            ("s", -i(sponsor_part)),
            ("o", -i(page - sponsor_part)),
            ("L", -i(page)),
            ("candidate_rent_s", i(sponsor_part)),
            ("candidate_rent_o", i(page - sponsor_part)),
            ("provisioned_fallback", 1),
            ("total_rent_funded", i(page)),
        ],
    );
    table.push(row_json("provision_page_locked_close.split", &before, &after));

    seal_labelled(w, 1, 4, "build_last");
    w.warp_past_deadline(1);
    let owed = w.close_record(1).locked_owner_remaining;
    assert_eq!(w.close_record(1).locked_sponsor_remaining, 0);
    let before = tracked(w, 1, &base);
    let expire = expire_ix(w, 1);
    ok(w, "table_8_6.settlement_at_terminal_promotion.owner_only", vec![expire], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "settlement_at_terminal_promotion.owner_only",
        &before,
        &after,
        &[("cash", -i(owed)), ("o", -i(owed)), ("L", -i(owed)), ("reserve_settled", 1), ("dest", i(owed))],
    );
    table.push(row_json("settlement_at_terminal_promotion.owner_only", &before, &after));
    scene.w.take_log()
}

/// World C (settle): both partitions are non-zero at the terminal promotion
/// (expiry), the destination deliverable: the table's settlement row.
fn table_world_settle(row: &str, table: &mut Vec<serde_json::Value>) -> Vec<Observation> {
    let owner_part = 5_000_000u64;
    let reserve = standard_reserve();
    let mut scene = make_scene(row, "settle", Spec::standard().with_sponsor(reserve - owner_part));
    let (owner_key, dest) = (scene.owner.pubkey(), scene.dest);
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let base: Vec<(&'static str, Pubkey)> = vec![("owner", owner_key), ("dest", dest)];
    w.warp_past_deadline(1);
    let (s, o) = (w.close_record(1).locked_sponsor_remaining, w.close_record(1).locked_owner_remaining);
    assert!(s > 0 && o == owner_part, "both partitions are non-zero at settlement");
    let before = tracked(w, 1, &base);
    let expire = expire_ix(w, 1);
    ok(w, "table_8_6.settlement_at_terminal_promotion", vec![expire], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "settlement_at_terminal_promotion",
        &before,
        &after,
        &[
            ("cash", -i(o)),
            ("free", i(s)),
            ("s", -i(s)),
            ("o", -i(o)),
            ("L", -i(s + o)),
            ("reserve_settled", 1),
            ("dest", i(o)),
        ],
    );
    table.push(row_json("settlement_at_terminal_promotion", &before, &after));
    scene.w.take_log()
}

/// World D (deferred): the destination is empty at the terminal promotion and
/// the owner partition is below the rent-exempt minimum: settlement DEFERS the
/// owner remainder (the undeliverable variant), and `deliver_owner_refund`
/// pays it once the destination can take it.
fn table_world_deferred(row: &str, table: &mut Vec<serde_json::Value>) -> Vec<Observation> {
    let (b_rent, registry_rent) = (rent_of(state::Backer::SPACE), rent_of(state::RegistryPage::SPACE));
    let owner_part = 400_000u64;
    let reserve = standard_reserve();
    let dest_key = destination_keypair(0xD6);
    let dest = dest_key.pubkey();
    let spec = Spec { dest: Some(dest), ..closer_wins(nineteen()).with_sponsor(reserve - owner_part) };
    let mut scene = make_scene(row, "deferred", spec);
    let owner_key = scene.owner.pubkey();
    let attacker = scene.w.attacker();
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let base: Vec<(&'static str, Pubkey)> = vec![("owner", owner_key), ("dest", dest)];
    do_request(w, 1, "request");
    w.fulfill("fulfill", 1).expect("fulfill");
    let capture = capture_ix(w, 1);
    ok(w, "capture", vec![capture], &[]);
    let paid_out = w.lamports(dest);
    let drain = transfer_ix(dest, attacker.pubkey(), paid_out);
    ok(w, "drain_destination", vec![drain], &[&dest_key]);
    assert_eq!(w.lamports(dest), 0);

    let (s, o) = (w.close_record(1).locked_sponsor_remaining, w.close_record(1).locked_owner_remaining);
    assert_eq!(o, owner_part, "the sponsor partition paid for everything so far");
    assert!(s >= b_rent + registry_rent, "the sponsor partition covers the promotion's storage");
    let s2 = s - b_rent - registry_rent;
    let before = tracked(w, 1, &base);
    let apply = apply_ix(w, 1);
    ok(w, "table_8_6.settlement_at_terminal_promotion_undeliverable", vec![apply], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "settlement_at_terminal_promotion_undeliverable",
        &before,
        &after,
        &[
            ("cash", -i(b_rent + registry_rent)),
            ("free", i(s2)),
            ("s", -i(s)),
            ("L", -i(s)),
            ("b_rent_s", i(b_rent)),
            ("registry_rent_s", i(registry_rent)),
            ("total_rent_funded", i(b_rent + registry_rent)),
            ("reserve_settled", 1),
        ],
    );
    assert_eq!(w.close_record(1).locked_owner_remaining, owner_part, "o is unchanged: deferred");
    table.push(row_json("settlement_at_terminal_promotion_undeliverable", &before, &after));

    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    let minimum = w.rt.rent(0);
    let fund = transfer_ix(attacker.pubkey(), dest, minimum);
    ok(w, "fund_destination", vec![fund], &[&attacker]);
    let before = tracked(w, 1, &base);
    let deliver = deliver_ix(w, 1, dest);
    ok(w, "table_8_6.deliver_owner_refund", vec![deliver], &[]);
    let after = tracked(w, 1, &base);
    assert_row(
        "deliver_owner_refund",
        &before,
        &after,
        &[("cash", -i(owner_part)), ("o", -i(owner_part)), ("L", -i(owner_part)), ("dest", i(owner_part))],
    );
    table.push(row_json("deliver_owner_refund", &before, &after));
    scene.w.take_log()
}

/// World E (CPI failure): the pinned pending-request size understates ORAO's,
/// the provider's own charge fails inside the CPI, and nothing moves.
fn table_world_cpi_failure(row: &str, table: &mut Vec<serde_json::Value>) -> Vec<Observation> {
    let spec = Spec { request_bytes: Some(100), ..Spec::standard() };
    let mut scene = make_scene(row, "cpi_failure", spec);
    let owner_key = scene.owner.pubkey();
    let dest = scene.dest;
    scene.provision_and_seal("build_last");
    let w = &mut scene.w;
    let keys = provider_keys();
    let request_key = host_request(w, 1).0;
    w.watch(request_key);
    let wallets: Vec<(&'static str, Pubkey)> = vec![
        ("owner", owner_key),
        ("dest", dest),
        ("client", keys.client),
        ("treasury", keys.treasury),
        ("request", request_key),
    ];
    let before = tracked(w, 1, &wallets);
    let request = request_ix(w, 1);
    reject_provider(w, "table_8_6.request_round_draw_cpi_fails", vec![request], &[]);
    let after = tracked(w, 1, &wallets);
    assert_row("request_round_draw_cpi_fails", &before, &after, &[]);
    assert_eq!(w.close_record(1).draw_state, state::DRAW_NOT_REQUESTED);
    table.push(row_json("request_round_draw_cpi_fails", &before, &after));
    scene.w.take_log()
}

// ---------------------------------------------------------------------------
// Row: settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable
// ---------------------------------------------------------------------------

/// Codex M8 review, runtime proof of the IMPL-36 G1 guard.  A close expires with
/// a deferred owner remainder LARGE enough to pay for a candidate page (the
/// immutable destination was reassigned to a foreign program, so no amount is
/// deliverable) while both candidate sets still need their first page.  Before
/// finalization, `provision_page(Candidate{..}, LockedClose)` for each set must
/// be refused with exact `LockNotPermitted`, fee-only, with CloseRecord, Ops,
/// OpsCash and the page address byte-identical; finalization and, once the
/// destination is a system wallet again, `deliver_owner_refund` still succeed.
/// RED on the IMPL-45 ELF `28f54d5e…` (the page was provisioned out of the
/// owner's deferred refund), GREEN from IMPL-36 (`locked_permission` requires
/// `!reserve_settled`).
pub fn settled_close_locks_no_further_pages_so_a_deferred_owner_refund_is_not_spendable() -> Vec<Observation> {
    let row = ROW_SETTLED_LOCKS_NOTHING;
    let reserve = standard_reserve();
    let page_rent = rent_of(state::CandidatePage::SPACE);
    let owner_part = 2 * page_rent + 12_345;
    assert!(owner_part < reserve, "the owner funds {owner_part} of the {reserve}-lamport reserve");
    let dest_key = destination_keypair(0xD9);
    let dest = dest_key.pubkey();
    let spec = Spec { dest: Some(dest), ..Spec::standard().with_sponsor(reserve - owner_part) };
    let mut scene = make_scene(row, "", spec);
    let w = &mut scene.w;
    let cash = w.rt.ops_cash_address();
    let close0 = w.close_record(1);
    assert_eq!(close0.locked_owner_remaining, owner_part);
    assert!(close0.required_pages_per_set >= 1);
    for set in 0u8..2 {
        assert!(w.account(candidate_page_key(1, set, 0)).is_none(), "set {set}: the first candidate page is still missing");
    }

    // The immutable destination can no longer receive lamports: reassigned to a
    // foreign program with data, `validate_destination` fails whatever the amount.
    let foreign = Pubkey::new_from_array([0xF0; 32]);
    let original = tamper(w, dest, |account| {
        account.owner = foreign;
        account.data = vec![1u8; 8];
    });

    w.warp_past_deadline(1);
    let ops_before = w.ops();
    let expire = expire_ix(w, 1);
    ok(w, "expire", vec![expire], &[]);
    let close = w.close_record(1);
    assert!(close.reserve_settled, "settlement completes");
    assert!(matches!(close.promotion, state::Promotion::Skipped { .. }));
    assert_eq!(close.locked_sponsor_remaining, 0);
    assert_eq!(close.locked_owner_remaining, owner_part, "deferred: undeliverable destination state, nothing spent");
    assert!(close.locked_owner_remaining >= page_rent, "absent the guard this credit would pay for a page");
    assert_eq!(w.ops().locked_remaining_total, owner_part);
    assert_eq!(w.ops().free_balance, ops_before.free_balance + close0.locked_sponsor_remaining);
    assert_eq!(w.ops().unallocated_excess, ops_before.unallocated_excess);
    let frozen = (w.data(w.rt.close_record_address(1)), w.data(w.rt.ops_address()), w.lamports(cash));

    // G1: LockedClose provisioning of the still-missing first page of EACH set is
    // refused; nothing moves.
    for (set, variant) in [(0u8, "locked_preferred_refused"), (1u8, "locked_fallback_refused")] {
        let page = candidate_page_key(1, set, 0);
        let provision = provision_ix(w, 1, set, 0, Funding::LockedClose);
        reject(w, variant, vec![provision], &[], ErrorCode::LockNotPermitted);
        assert!(w.account(page).is_none(), "set {set}: no page account was created");
        assert_eq!(
            (w.data(w.rt.close_record_address(1)), w.data(w.rt.ops_address()), w.lamports(cash)),
            frozen,
            "set {set}: CloseRecord, Ops and OpsCash are byte-identical"
        );
    }
    assert_eq!(w.close_record(1).locked_owner_remaining, owner_part, "the deferred refund is intact");
    assert_identities(w);

    // Finalization never depends on the refund; delivery waits for a valid destination.
    let finalize = finalize_ix(w, 1);
    ok(w, "finalize", vec![finalize], &[]);
    assert!(w.close_record(1).finalized);
    let deliver = deliver_ix(w, 1, dest);
    reject(w, "deliver_invalid_destination", vec![deliver], &[], ErrorCode::RefundNotDeliverable);
    restore(w, dest, original);
    let (dest0, cash0) = (w.lamports(dest), cash_of(w));
    let deliver = deliver_ix(w, 1, dest);
    let delivered = ok(w, "", vec![deliver], &[]);
    assert_eq!(w.lamports(dest) - dest0, owner_part, "the whole deferred refund reached the destination");
    assert_eq!(cash0 - cash_of(w), owner_part, "paid out of OpsCash");
    assert_eq!((w.close_record(1).locked_owner_remaining, w.ops().locked_remaining_total), (0, 0));
    assert_eq!(delivered.last_event::<OwnerRefundDelivered>().amount, owner_part);
    assert_identities(w);
    scene.w.take_log()
}
