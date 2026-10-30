//! Plan M13 worker tests.  MockChain only: no network, no keypair files, no SBF.
//!
//! The three plan tests (`prepares_one_row_fallback_when_simulation_exceeds_budget`,
//! `does_not_resubmit_after_unknown_outcome_until_status_read`, `any_wallet_can_run_the_same_plan`) come
//! first; the rest are small extra tests per step of the implementation (planner details, the two
//! lowest-priority steps, journal, metrics, the closing branch of the loader).

mod fixtures;

use anchor_lang::{AnchorDeserialize, Discriminator};
use fixtures::*;
use musical_chairs_keeper::{
    journal::{JStatus, Journal, JournalEntry, JournalError, MemSink},
    metrics::{marker_of, AlertLevel, MetricsReport, Progress},
    plan::{
        assert_fee_only, candidate_batch_rows, degrade_to_one_row, next_step, Limits,
        FEE_PAYER_PLACEHOLDER,
    },
    rpc::{MockChain, SendBehavior, SimFailureKind, SimOutcome, StatusAnswer},
    runner::{Action, SendOutcome, Worker},
    sha256_hex,
    state::{
        candidate_address, close_record_address, load_snapshot, ops_address, ops_cash_address,
        participant_address, pool_address, registry_address, snapshot_address, ticket_address,
        RefundTarget, SharedPage, Snapshot, SystemAccountView, PROGRAM_ID,
    },
};
use musical_chairs_program::{
    close::{plan_request_spend, promotion_holdback},
    instruction::{
        ApplySenior, CaptureRoundDraw, CloseSharedAccount, DeliverOwnerRefund, DiscoverRows,
        IncreaseProviderBudget, PruneRows,
    },
    provider,
    state::{
        CloseRecord, Promotion, RentComponents, DRAW_CAPTURED, DRAW_NOT_REQUESTED, DRAW_REQUESTED,
    },
    PageKind,
};
use solana_sdk::{
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

/// A compiled transaction as `(program, account keys in order, data)` per instruction.
type Resolved = Vec<(Pubkey, Vec<Pubkey>, Vec<u8>)>;

fn resolve(tx: &Transaction) -> Resolved {
    let keys = &tx.message.account_keys;
    tx.message
        .instructions
        .iter()
        .map(|ix| {
            (
                keys[usize::from(ix.program_id_index)],
                ix.accounts
                    .iter()
                    .map(|index| keys[usize::from(*index)])
                    .collect(),
                ix.data.clone(),
            )
        })
        .collect()
}

/// The program's own instruction of a transaction (the other one is the compute-unit limit).
fn program_instruction(tx: &Transaction) -> (Vec<Pubkey>, Vec<u8>) {
    let mut ours: Vec<_> = resolve(tx)
        .into_iter()
        .filter(|(program, _, _)| *program == PROGRAM_ID)
        .collect();
    assert_eq!(
        ours.len(),
        1,
        "exactly one program instruction per transaction"
    );
    let (_, accounts, data) = ours.remove(0);
    (accounts, data)
}

fn decode_sent(chain: &MockChain, index: usize) -> Transaction {
    bincode::deserialize(&chain.sent()[index].bytes).expect("a sent transaction")
}

fn memory_journal() -> (Journal, MemSink) {
    let sink = MemSink::new();
    (
        Journal::open(Box::new(sink.clone())).expect("open an empty journal"),
        sink,
    )
}

// ---------------------------------------------------------------------------
// 1. one-row fallback
// ---------------------------------------------------------------------------

#[test]
fn prepares_one_row_fallback_when_simulation_exceeds_budget() {
    // A ticket in Discover over 20 registry rows (three pages: eight, eight, four), the cursor at the start of page 1.
    let chain = MockChain::new();
    discover_world(&chain, 8, 20);

    // The planned batch is the full eight rows and sits on ONE page: it never straddles the page edge.
    let snapshot = load_snapshot(&chain).expect("snapshot");
    let step = next_step(&snapshot, &Limits::default())
        .expect("plan")
        .expect("a discover step");
    assert_eq!(step.name(), "discover_rows");
    assert_eq!(step.rows, 8);
    assert_eq!(step.row_accounts.len(), 8);
    for row in &step.row_accounts {
        assert_eq!(
            row[0].pubkey,
            registry_address(1),
            "every row of the batch is on registry page 1"
        );
        assert_eq!(
            row[1].pubkey,
            snapshot_address(1),
            "... and snapshot page 1"
        );
    }

    // From a cursor in the middle of a page the batch is cut at the page edge (slots 5, 6, 7), not eight rows.
    let unaligned = MockChain::new();
    discover_world(&unaligned, 5, 20);
    let snapshot = load_snapshot(&unaligned).expect("snapshot");
    let cut = next_step(&snapshot, &Limits::default())
        .expect("plan")
        .expect("a discover step");
    assert_eq!(cut.rows, 3);
    for row in &cut.row_accounts {
        assert_eq!(
            row[0].pubkey,
            registry_address(0),
            "the cut batch stays on registry page 0"
        );
    }

    // The cluster says the 8-row batch costs more than the step's compute limit; one row is well inside it.
    chain.script_simulation("discover_rows", 8, SimOutcome::Ok { units: 250_000 });
    chain.script_simulation("discover_rows", 1, SimOutcome::Ok { units: 60_000 });
    let payer = Keypair::new();
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");

    // Both batches were simulated, the big one first; only the one-row transaction was signed and sent.
    let simulated: Vec<_> = chain
        .simulated()
        .into_iter()
        .map(|label| (label.name, label.rows))
        .collect();
    assert_eq!(simulated, vec![("discover_rows", 8), ("discover_rows", 1)]);
    assert_eq!(chain.sent().len(), 1, "exactly one transaction is sent");
    match &report.action {
        Action::Sent {
            step,
            rows,
            outcome,
            ..
        } => {
            assert_eq!(step, "discover_rows:1:8");
            assert_eq!(*rows, 1);
            assert_eq!(*outcome, SendOutcome::Accepted);
        }
        other => panic!("expected a sent step, got {other:?}"),
    }

    // The step sent covers exactly one row, starting at the ticket cursor (slot 8).
    let tx = decode_sent(&chain, 0);
    let (accounts, data) = program_instruction(&tx);
    assert_eq!(&data[..8], DiscoverRows::DISCRIMINATOR);
    let args = DiscoverRows::try_from_slice(&data[8..]).expect("discover args");
    assert_eq!(
        (args.expected_ticket_id, args.expected_cursor, args.rows),
        (1, 8, 1)
    );
    assert_eq!(
        accounts,
        vec![
            ticket_address(),
            pool_address(),
            registry_address(1),
            snapshot_address(1),
            backer_key(8)
        ],
        "ticket, pool, then one row: its registry page, its snapshot page, the Backer at slot 8"
    );
}

// ---------------------------------------------------------------------------
// 2. unknown outcome
// ---------------------------------------------------------------------------

#[test]
fn does_not_resubmit_after_unknown_outcome_until_status_read() {
    let chain = MockChain::new();
    senior_world(&chain);
    let payer = Keypair::new();
    let (journal, sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());

    // The send dies after the bytes may have left: the outcome is unknown.
    chain.script_send(SendBehavior::NetworkErrorAfterSend(
        "connection reset by peer".to_string(),
    ));
    let report = worker.once(NOW).expect("first pass");
    match &report.action {
        Action::Sent { step, outcome, .. } => {
            assert_eq!(step, "apply_senior:1");
            assert_eq!(*outcome, SendOutcome::Unknown);
        }
        other => panic!("expected a sent step, got {other:?}"),
    }
    assert_eq!(chain.sent().len(), 1);
    let first_tx = chain.sent()[0].clone();
    let first: JournalEntry = worker.journal().entries()[0].clone();
    assert_eq!(first.status, JStatus::Unknown);
    assert_eq!(first.step, "apply_senior:1");
    assert_eq!(first.signature, first_tx.signature.to_string());
    assert_eq!(first.bytes_sha256, sha256_hex(&first_tx.bytes));
    assert_eq!(
        first.last_valid_block_height,
        chain.last_valid_block_height()
    );
    // The entry was journaled BEFORE the send (write-ahead) and then updated by a second line.
    let lines = sink.lines();
    assert_eq!(lines.len(), 2);
    let written_first: JournalEntry = serde_json::from_str(&lines[0]).expect("a journal line");
    assert_eq!(written_first.status, JStatus::Submitted);
    assert_eq!(written_first.signature, first.signature);
    let written_second: JournalEntry = serde_json::from_str(&lines[1]).expect("a journal line");
    assert_eq!(written_second.status, JStatus::Unknown);

    // No status answer (the status path is down): a second pass sends NOTHING, not even a re-broadcast.
    chain.set_status_unavailable(true);
    let report = worker.once(NOW + 5).expect("second pass");
    assert!(
        matches!(report.action, Action::StatusUnavailable { .. }),
        "got {:?}",
        report.action
    );
    assert_eq!(
        chain.sent().len(),
        1,
        "an outcome that cannot be read is not resubmitted"
    );
    assert!(report.rebroadcast.is_empty());

    // The status path answers "not seen" and the blockhash is still valid: the IDENTICAL signed bytes go out again.
    chain.set_status_unavailable(false);
    let report = worker.once(NOW + 10).expect("third pass");
    assert_eq!(report.rebroadcast, vec![first.signature.clone()]);
    assert!(
        matches!(report.action, Action::WaitingOnUnresolved { .. }),
        "got {:?}",
        report.action
    );
    let sent = chain.sent();
    assert_eq!(sent.len(), 2, "one re-broadcast, no new transaction");
    assert_eq!(sent[1].bytes, sent[0].bytes, "byte-identical re-broadcast");
    assert_eq!(sha256_hex(&sent[1].bytes), sha256_hex(&sent[0].bytes));
    assert_eq!(
        sent[1].signature, sent[0].signature,
        "same signature, same blockhash"
    );
    assert_eq!(
        worker.journal().entries().len(),
        1,
        "a re-broadcast adds no entry"
    );
    assert_eq!(worker.journal().entries()[0].status, JStatus::Unknown);
    assert_eq!(sink.lines().len(), 2, "... and no line");

    // A status read answers `confirmed` (and the ticket moved on): the entry settles and the NEXT step proceeds.
    chain.script_status(sent[0].signature, StatusAnswer::Confirmed);
    advance_to_seal(&chain);
    let report = worker.once(NOW + 15).expect("fourth pass");
    assert_eq!(
        worker
            .journal()
            .entry(&first.signature)
            .expect("entry")
            .status,
        JStatus::Confirmed
    );
    match &report.action {
        Action::Sent { step, outcome, .. } => {
            assert_eq!(step, "seal_weights:1");
            assert_eq!(*outcome, SendOutcome::Accepted);
        }
        other => panic!("expected the next step to be sent, got {other:?}"),
    }
    let sent = chain.sent();
    assert_eq!(sent.len(), 3);
    assert_ne!(sent[2].signature, sent[0].signature);
    assert_eq!(worker.journal().entries().len(), 2);

    // A new blockhash is taken only after the old one expired: the seal transaction was never seen and the
    // finalized block height is now past its last valid height.
    let seal_signature = sent[2].signature;
    let old_blockhash = decode_sent(&chain, 2).message.recent_blockhash;
    assert_eq!(old_blockhash, Hash::new_from_array([7; 32]));
    chain.set_block_height(chain.last_valid_block_height() + 1);
    let fresh = Hash::new_from_array([8; 32]);
    chain.set_blockhash(fresh, chain.last_valid_block_height() + 300);
    let report = worker.once(NOW + 20).expect("fifth pass");
    assert_eq!(
        worker
            .journal()
            .entry(&seal_signature.to_string())
            .expect("entry")
            .status,
        JStatus::Expired
    );
    assert!(
        report.rebroadcast.is_empty(),
        "an expired transaction is not re-broadcast"
    );
    let sent = chain.sent();
    assert_eq!(sent.len(), 4);
    assert_ne!(sent[3].signature, seal_signature);
    assert_eq!(
        decode_sent(&chain, 3).message.recent_blockhash,
        fresh,
        "the retry uses a fresh blockhash"
    );
    assert_eq!(worker.journal().entries().len(), 3);

    // Decoding the first transaction: it really was apply_senior.
    let (_, data) = program_instruction(&decode_sent(&chain, 0));
    assert_eq!(&data[..8], ApplySenior::DISCRIMINATOR);
}

// ---------------------------------------------------------------------------
// 3. any wallet
// ---------------------------------------------------------------------------

/// IDL account names that denote an authority, owner or governance role.  In this program's IDL the signer
/// roles are `payer`, `owner`, `governance`, `guardian`, `sponsor`, `recipient` and `sender`; the fee payer
/// may only ever sit in a `payer` slot (ORAO documents `payer` as paying transaction fees only, and the
/// program's `provision_*` payer is a refundable rent depositor).
const AUTHORITY_NAMES: &[&str] = &[
    "authority",
    "owner",
    "governance",
    "guardian",
    "sponsor",
    "recipient",
    "management_recipient",
    "sender",
    "g_vault",
    "p_vault",
    "upgrade_authority",
    "admin",
];

/// `instructions` with the payer's key replaced by the placeholder: what must be equal across payers.
/// (program id, (account, is_signer, is_writable) per position, instruction data) of one resolved instruction.
type ResolvedIx = (Pubkey, Vec<(Pubkey, bool, bool)>, Vec<u8>);

fn normalized(instructions: &[Instruction], payer: &Pubkey) -> Vec<ResolvedIx> {
    instructions
        .iter()
        .map(|ix| {
            (
                ix.program_id,
                ix.accounts
                    .iter()
                    .map(|meta| {
                        (
                            if meta.pubkey == *payer {
                                FEE_PAYER_PLACEHOLDER
                            } else {
                                meta.pubkey
                            },
                            meta.is_signer,
                            meta.is_writable,
                        )
                    })
                    .collect(),
                ix.data.clone(),
            )
        })
        .collect()
}

fn payer_slots(instructions: &[Instruction], payer: &Pubkey) -> usize {
    instructions
        .iter()
        .flat_map(|ix| ix.accounts.iter())
        .filter(|meta| meta.pubkey == *payer)
        .count()
}

#[test]
fn any_wallet_can_run_the_same_plan() {
    let wallet_a = Keypair::new();
    let wallet_b = Keypair::new();
    assert_ne!(wallet_a.pubkey(), wallet_b.pubkey());
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), 18, "one scenario per kind of step");

    let mut seen = std::collections::BTreeSet::new();
    for (expected_name, snapshot, limits) in &scenarios {
        let step = next_step(snapshot, limits)
            .unwrap_or_else(|e| panic!("{expected_name}: plan failed: {e}"))
            .unwrap_or_else(|| panic!("{expected_name}: the planner found nothing to do"));
        assert_eq!(step.name(), *expected_name);
        seen.insert(step.name());

        // The plan never sees a wallet, so two payers get the same instruction list; only the payer's meta differs.
        let ixs_a = step.instructions(&wallet_a.pubkey());
        let ixs_b = step.instructions(&wallet_b.pubkey());
        assert_eq!(
            ixs_a.len(),
            2,
            "{expected_name}: [compute-unit limit, program instruction]"
        );
        assert_eq!(
            normalized(&ixs_a, &wallet_a.pubkey()),
            normalized(&ixs_b, &wallet_b.pubkey()),
            "{expected_name}: instruction lists differ beyond the fee-payer meta"
        );
        assert_eq!(
            payer_slots(&ixs_a, &wallet_a.pubkey()),
            payer_slots(&ixs_b, &wallet_b.pubkey())
        );
        assert!(
            payer_slots(&ixs_a, &wallet_a.pubkey()) <= 1,
            "{expected_name}: the payer appears in at most one account slot"
        );
        assert_eq!(
            payer_slots(&ixs_a, &wallet_b.pubkey()),
            0,
            "{expected_name}: the other wallet appears nowhere"
        );

        // Nothing in the transaction can move funds: only the compute budget and this program, payer in the `payer` slot.
        assert_fee_only(&ixs_a, &wallet_a.pubkey())
            .unwrap_or_else(|e| panic!("{expected_name}: {e}"));

        // Static check against the IDL: the fee payer's account position is never an authority/owner/governance
        // signer, and the instruction asks for no signer other than a fee `payer`.
        let program_ix = &ixs_a[1];
        let idl = idl_accounts(step.name());
        assert!(
            program_ix.accounts.len() >= idl.len(),
            "{expected_name}: fewer accounts than the IDL declares"
        );
        for account in &idl {
            if account.signer {
                assert_eq!(
                    account.name, "payer",
                    "{expected_name}: the IDL asks for a signer other than the fee payer"
                );
                assert!(!AUTHORITY_NAMES.contains(&account.name.as_str()));
            }
        }
        for (position, meta) in program_ix.accounts.iter().enumerate() {
            if position < idl.len() {
                assert_eq!(
                    meta.is_signer, idl[position].signer,
                    "{expected_name}: signer flag of {}",
                    idl[position].name
                );
                // An omitted optional account is Anchor's `None`: the program id, always read-only.
                if meta.pubkey != PROGRAM_ID {
                    assert_eq!(
                        meta.is_writable, idl[position].writable,
                        "{expected_name}: writable flag of {}",
                        idl[position].name
                    );
                }
            } else {
                assert!(
                    !meta.is_signer,
                    "{expected_name}: a remaining account must not sign"
                );
            }
            if meta.pubkey == wallet_a.pubkey() {
                assert!(
                    position < idl.len(),
                    "{expected_name}: the payer is not a remaining account"
                );
                assert_eq!(
                    idl[position].name, "payer",
                    "{expected_name}: the fee payer sits in `{}`",
                    idl[position].name
                );
                assert!(!AUTHORITY_NAMES.contains(&idl[position].name.as_str()));
                assert!(
                    meta.is_signer && meta.is_writable,
                    "{expected_name}: a payer pays: signer and writable"
                );
            }
        }
    }
    assert_eq!(
        seen.len(),
        18,
        "all eighteen keeper instructions were exercised: {seen:?}"
    );

    // End to end: two unrelated wallets run the same pass against identical worlds.
    let mut resolved = Vec::new();
    for wallet in [&wallet_a, &wallet_b] {
        let chain = MockChain::new();
        discover_world(&chain, 0, 20);
        let (journal, _sink) = memory_journal();
        let mut worker = Worker::new(&chain, wallet, journal, Limits::default());
        let report = worker.once(NOW).expect("a pass");
        assert!(
            matches!(report.action, Action::Sent { .. }),
            "got {:?}",
            report.action
        );
        let tx = decode_sent(&chain, 0);
        assert_eq!(
            tx.message.account_keys[0],
            wallet.pubkey(),
            "the wallet is the fee payer"
        );
        assert_eq!(
            tx.message.header.num_required_signatures, 1,
            "the fee payer is the ONLY signer"
        );
        assert_eq!(tx.signatures.len(), 1);
        let ixs = resolve(&tx);
        assert!(ixs
            .iter()
            .all(|(program, _, _)| *program == PROGRAM_ID
                || *program == solana_sdk::compute_budget::ID));
        let normal: Resolved = ixs
            .into_iter()
            .map(|(program, accounts, data)| {
                (
                    program,
                    accounts
                        .into_iter()
                        .map(|k| {
                            if k == wallet.pubkey() {
                                FEE_PAYER_PLACEHOLDER
                            } else {
                                k
                            }
                        })
                        .collect(),
                    data,
                )
            })
            .collect();
        resolved.push(normal);
    }
    assert_eq!(
        resolved[0], resolved[1],
        "the same pass from two wallets differs only in the fee payer"
    );
}

// ---------------------------------------------------------------------------
// Extra tests
// ---------------------------------------------------------------------------

#[test]
fn prune_removes_a_captured_closed_row_by_swap_delete_and_keeps_open_rows() {
    // Scenario: registry of 12, ticket in Prune at cursor 12, slot 11 captured closed (and the last live slot).
    let (_, snapshot, limits) = scenarios().remove(4);
    let step = next_step(&snapshot, &limits)
        .expect("plan")
        .expect("a prune step");
    assert_eq!(step.name(), "prune_rows");
    assert_eq!(
        step.rows, 4,
        "cut at the page edge: slots 11, 10, 9, 8 are all on page 1"
    );
    let removed: Vec<Pubkey> = step.row_accounts[0]
        .iter()
        .map(|meta| meta.pubkey)
        .collect();
    assert_eq!(
        removed,
        vec![
            snapshot_address(1),
            backer_key(11),
            registry_address(1),
            registry_address(1),
            backer_key(11)
        ],
        "the last live slot is pruned: its moved account aliases itself"
    );
    assert!(!step.row_accounts[0][0].is_writable);
    assert!(step.row_accounts[0][1..]
        .iter()
        .all(|meta| meta.is_writable));
    for row in &step.row_accounts[1..] {
        assert_eq!(row[0].pubkey, snapshot_address(1));
        assert!(
            row[1..]
                .iter()
                .all(|meta| meta.pubkey == PROGRAM_ID && !meta.is_writable),
            "an open row passes four program ids"
        );
    }

    // The one-row fallback keeps row 0 and re-encodes `rows`.
    let one = degrade_to_one_row(&step);
    assert_eq!(one.rows, 1);
    assert_eq!(one.row_accounts, step.row_accounts[..1].to_vec());
    let data = one.data();
    assert_eq!(
        &data[..8],
        musical_chairs_program::instruction::PruneRows::DISCRIMINATOR
    );
    let args = PruneRows::try_from_slice(&data[8..]).expect("prune args");
    assert_eq!(
        (args.expected_ticket_id, args.expected_cursor, args.rows),
        (1, 12, 1)
    );
    assert_eq!(degrade_to_one_row(&one), one, "a one-row step is unchanged");

    // A pruned slot below the last live slot moves the LAST row's Backer into it.
    let snapshot = prune_snapshot(4, |slot| slot == 0);
    let step = next_step(&snapshot, &Limits::default())
        .expect("plan")
        .expect("a prune step");
    assert_eq!(step.rows, 4);
    let last_row: Vec<Pubkey> = step.row_accounts[3]
        .iter()
        .map(|meta| meta.pubkey)
        .collect();
    assert_eq!(
        last_row,
        vec![
            snapshot_address(0),
            backer_key(0),
            registry_address(0),
            registry_address(0),
            backer_key(3)
        ],
        "slot 0 is removed and the Backer of slot 3 (the registry tail) moves into it"
    );
    for row in &step.row_accounts[..3] {
        assert!(row[1..].iter().all(|meta| meta.pubkey == PROGRAM_ID));
    }
}

#[test]
fn candidate_batches_never_cross_a_participant_or_candidate_page_edge() {
    let close = |cursor, preferred, fallback| CloseRecord {
        participant_count: 100,
        candidate_cursor: cursor,
        preferred_count: preferred,
        fallback_count: fallback,
        ..CloseRecord::default()
    };
    assert_eq!(candidate_batch_rows(&close(0, 0, 0), 8), 8);
    assert_eq!(
        candidate_batch_rows(&close(6, 0, 0), 8),
        2,
        "two rows left on participant page 0"
    );
    assert_eq!(
        candidate_batch_rows(&close(0, 5, 0), 8),
        3,
        "three slots left on the preferred page"
    );
    assert_eq!(
        candidate_batch_rows(&close(0, 0, 7), 8),
        1,
        "one slot left on the fallback page"
    );
    assert_eq!(
        candidate_batch_rows(&close(8, 8, 16), 8),
        8,
        "a count that just filled a page starts a fresh one"
    );
    assert_eq!(
        candidate_batch_rows(&close(96, 0, 0), 8),
        4,
        "only four participants are left"
    );
    assert_eq!(
        candidate_batch_rows(&close(100, 0, 0), 8),
        0,
        "everyone was visited"
    );
    assert_eq!(
        candidate_batch_rows(&close(0, 0, 0), 3),
        3,
        "the configured maximum wins"
    );
}

#[test]
fn closing_pipeline_waits_expires_and_applies_in_the_reference_order() {
    let all = scenarios();
    let plan = |snapshot: &musical_chairs_keeper::state::Snapshot| {
        next_step(snapshot, &Limits::default())
            .expect("plan")
            .map(|step| step.name())
    };

    // The closing ticket's cash has not committed: nothing on the close can move yet.
    let mut snapshot = all[6].1.clone();
    snapshot.close.as_mut().expect("close").cash_committed = false;
    assert_eq!(plan(&snapshot), None);

    // Past the deadline an unsealed close expires instead of building more candidates.
    let mut snapshot = all[6].1.clone();
    snapshot.clock_unix = (NOW + 1_000) as i64;
    assert_eq!(plan(&snapshot), Some("expire_promotion"));

    // A captured draw still applies after the deadline (a timely capture is never deadline-skipped).
    let mut snapshot = all[10].1.clone();
    snapshot.clock_unix = (NOW + 1_000) as i64;
    assert_eq!(plan(&snapshot), Some("apply_promotion"));

    // A known-pending provider request waits; fulfilled or unknown is captured; past the deadline it expires.
    let mut snapshot = all[9].1.clone();
    snapshot.request_fulfilled = Some(false);
    assert_eq!(plan(&snapshot), None);
    snapshot.request_fulfilled = Some(true);
    assert_eq!(plan(&snapshot), Some("capture_round_draw"));
    snapshot.request_fulfilled = Some(false);
    snapshot.clock_unix = (NOW + 1_000) as i64;
    assert_eq!(plan(&snapshot), Some("expire_promotion"));

    // A terminal promotion whose reserve is not settled yet cannot finalize.
    let mut snapshot = all[12].1.clone();
    snapshot.close.as_mut().expect("close").reserve_settled = false;
    assert_eq!(plan(&snapshot), None);
}

#[test]
fn delivers_a_deferred_owner_refund_only_when_the_destination_can_take_it() {
    let limits = Limits::default();
    let plan = |snapshot: &musical_chairs_keeper::state::Snapshot| {
        next_step(snapshot, &limits)
            .expect("plan")
            .map(|step| step.name())
    };
    let deliverable = |owed: u64, lamports: u64| {
        let mut snapshot = idle_snapshot(settled_close(owed));
        snapshot.refund_target = Some(RefundTarget {
            lamports,
            plain_system_account: true,
        });
        snapshot
    };

    // A fresh destination can take a credit that reaches the rent floor of a data-less account (890,880).
    let snapshot = deliverable(5_000_000, 0);
    let step = next_step(&snapshot, &limits)
        .expect("plan")
        .expect("a delivery");
    assert_eq!(step.name(), "deliver_owner_refund");
    assert_eq!(step.key(), "deliver_owner_refund:1");
    assert_eq!(step.accounts[0].pubkey, close_record_address(1));
    assert_eq!(
        step.accounts[3].pubkey,
        key(0x77),
        "the recorded destination is the only recipient"
    );
    assert!(step.accounts[3].is_writable && !step.accounts[3].is_signer);
    assert!(
        idl_accounts("deliver_owner_refund")
            .iter()
            .all(|account| !account.signer),
        "permissionless: no signer"
    );
    assert_eq!(&step.data()[..8], DeliverOwnerRefund::DISCRIMINATOR);

    // A credit below the rent floor into an empty account would be refused by the runtime: not attempted.
    assert_eq!(plan(&deliverable(100, 0)), None);
    // ... but the same credit into an account that already holds the floor is fine.
    assert_eq!(
        plan(&deliverable(100, 890_880)),
        Some("deliver_owner_refund")
    );

    // Not a plain system account, destination not loaded, nothing owed, reserve not settled, OpsCash as destination.
    let mut snapshot = deliverable(5_000_000, 0);
    snapshot.refund_target = Some(RefundTarget {
        lamports: 0,
        plain_system_account: false,
    });
    assert_eq!(plan(&snapshot), None);
    let mut snapshot = deliverable(5_000_000, 0);
    snapshot.refund_target = None;
    assert_eq!(plan(&snapshot), None);
    assert_eq!(plan(&deliverable(0, 0)), None);
    let mut snapshot = deliverable(5_000_000, 0);
    snapshot
        .last_close
        .as_mut()
        .expect("last close")
        .reserve_settled = false;
    assert_eq!(plan(&snapshot), None);
    let mut snapshot = deliverable(5_000_000, 0);
    snapshot
        .last_close
        .as_mut()
        .expect("last close")
        .owner_refund_destination = ops_cash_address();
    assert_eq!(plan(&snapshot), None);

    // End to end through the loader: an idle pool, a settled close, a destination that does not exist yet.
    let chain = MockChain::new();
    idle_world(&chain, Some(&settled_close(5_000_000)));
    let payer = Keypair::new();
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "deliver_owner_refund:1"),
        "got {:?}",
        report.action
    );
    let tx = decode_sent(&chain, 0);
    assert_eq!(
        tx.message.header.num_required_signatures, 1,
        "only the fee payer signs"
    );
    let (accounts, _) = program_instruction(&tx);
    assert!(accounts.contains(&key(0x77)));
}

#[test]
fn observes_a_fulfilled_request_once() {
    let limits = Limits::default();
    let observed = |draw_state: u8, fulfilled: Option<bool>, reimbursement_observed: bool| {
        let close = CloseRecord {
            draw_state,
            request_account: key(0xB1),
            reimbursement_observed,
            ..settled_close(0)
        };
        let mut snapshot = idle_snapshot(close);
        snapshot.request_fulfilled = fulfilled;
        next_step(&snapshot, &limits)
            .expect("plan")
            .map(|step| (step.name(), step.key()))
    };
    assert_eq!(
        observed(DRAW_CAPTURED, Some(true), false),
        Some((
            "observe_request_fulfilled",
            "observe_request_fulfilled:1".to_string()
        ))
    );
    assert_eq!(
        observed(DRAW_CAPTURED, Some(false), false),
        None,
        "a pending request cannot be observed"
    );
    assert_eq!(
        observed(DRAW_CAPTURED, None, false),
        None,
        "an unknown request state is not guessed"
    );
    assert_eq!(
        observed(DRAW_CAPTURED, Some(true), true),
        None,
        "observed exactly once"
    );
    assert_eq!(
        observed(DRAW_NOT_REQUESTED, Some(true), false),
        None,
        "no request was ever made"
    );

    let close = CloseRecord {
        draw_state: DRAW_CAPTURED,
        request_account: key(0xB1),
        ..settled_close(0)
    };
    let mut snapshot = idle_snapshot(close);
    snapshot.request_fulfilled = Some(true);
    let step = next_step(&snapshot, &limits)
        .expect("plan")
        .expect("an observation");
    assert_eq!(step.accounts[1].pubkey, close_record_address(1));
    assert_eq!(
        step.accounts[3].pubkey,
        key(0xB1),
        "the request account the close recorded"
    );
    assert_eq!(step.accounts[4].pubkey, provider::client_address().0);
    assert!(
        idl_accounts("observe_request_fulfilled")
            .iter()
            .all(|account| !account.signer),
        "permissionless: no signer"
    );

    // A deliverable deferred refund comes first; once it is paid the observation follows.
    let close = CloseRecord {
        draw_state: DRAW_CAPTURED,
        request_account: key(0xB1),
        ..settled_close(5_000_000)
    };
    let mut snapshot = idle_snapshot(close);
    snapshot.request_fulfilled = Some(true);
    snapshot.refund_target = Some(RefundTarget {
        lamports: 0,
        plain_system_account: true,
    });
    assert_eq!(
        next_step(&snapshot, &limits)
            .expect("plan")
            .expect("a step")
            .name(),
        "deliver_owner_refund"
    );
    snapshot
        .last_close
        .as_mut()
        .expect("last close")
        .locked_owner_remaining = 0;
    assert_eq!(
        next_step(&snapshot, &limits)
            .expect("plan")
            .expect("a step")
            .name(),
        "observe_request_fulfilled"
    );
}

#[test]
fn waits_for_pending_randomness_alerts_on_stale_progress_then_captures() {
    let chain = MockChain::new();
    closing_world(&chain, Some(false));
    let payer = Keypair::new();
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());

    // The ORAO request is pending: nothing to send, work is pending, and the marker does not move.
    let report = worker.once(NOW).expect("first pass");
    assert!(
        matches!(report.action, Action::Blocked { .. }),
        "got {:?}",
        report.action
    );
    assert!(report.pending_work);
    assert_eq!(report.alert, AlertLevel::None);
    let report = worker.once(NOW + 59).expect("second pass");
    assert_eq!(report.alert, AlertLevel::None);
    let report = worker.once(NOW + 90).expect("third pass");
    assert_eq!(report.alert, AlertLevel::Alert);
    assert_eq!(report.seconds_since_progress, 90);
    let report = worker.once(NOW + 400).expect("fourth pass");
    assert_eq!(report.alert, AlertLevel::Escalate);
    assert!(chain.sent().is_empty(), "waiting sends nothing");
    let metrics = worker.metrics(NOW + 400, &report);
    assert_eq!(metrics.alert_level, AlertLevel::Escalate);
    assert_eq!(metrics.last_action, "blocked");
    assert!(metrics.pending_work);

    // The provider answers: the capture goes out (the program, not the keeper, copies the randomness).
    closing_world(&chain, Some(true));
    let report = worker.once(NOW + 410).expect("fifth pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "capture_round_draw:1"),
        "got {:?}",
        report.action
    );
    let (accounts, data) = program_instruction(&decode_sent(&chain, 0));
    assert_eq!(&data[..8], CaptureRoundDraw::DISCRIMINATOR);
    assert_eq!(accounts[0], pool_address());
    assert_eq!(accounts[1], close_record_address(1));
    assert_eq!(accounts[2], key(0xB1), "the request the close recorded");
    assert_eq!(
        accounts[3],
        key(0xA4),
        "the provider client of the pinned config"
    );
}

#[test]
fn deterministic_program_failures_block_and_page_failures_shrink() {
    let payer = Keypair::new();

    // A page failure of the 8-row batch is cured by one row: the one-row transaction is sent.
    let chain = MockChain::new();
    discover_world(&chain, 0, 20);
    chain.script_simulation(
        "discover_rows",
        8,
        SimOutcome::Failed {
            kind: SimFailureKind::Page,
            reason: "PageNotProvisioned".to_string(),
        },
    );
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(report.action, Action::Sent { rows: 1, .. }),
        "got {:?}",
        report.action
    );

    // Any other program failure is final for this state: nothing is signed, nothing is journaled.
    let chain = MockChain::new();
    discover_world(&chain, 0, 20);
    chain.script_simulation(
        "discover_rows",
        8,
        SimOutcome::Failed {
            kind: SimFailureKind::Program,
            reason: "StaleCursor".to_string(),
        },
    );
    let (journal, sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(report.action, Action::Blocked { .. }),
        "got {:?}",
        report.action
    );
    assert!(chain.sent().is_empty());
    assert_eq!(
        chain.simulated().len(),
        1,
        "a deterministic failure is not retried with fewer rows"
    );
    assert!(sink.lines().is_empty());

    // Dry run: planned and simulated, never signed.
    let chain = MockChain::new();
    discover_world(&chain, 0, 20);
    let (journal, sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default()).with_dry_run(true);
    let report = worker.once(NOW).expect("a pass");
    assert_eq!(
        report.action,
        Action::DryRun {
            step: "discover_rows:1:0".to_string(),
            rows: 8
        }
    );
    assert!(chain.sent().is_empty());
    assert!(sink.lines().is_empty());
}

#[test]
fn an_idle_pool_sends_nothing_and_never_alerts() {
    let chain = MockChain::new();
    idle_world(&chain, None);
    let payer = Keypair::new();
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    for now in [NOW, NOW + 600, NOW + 6_000] {
        let report = worker.once(now).expect("a pass");
        assert_eq!(report.action, Action::Idle);
        assert!(!report.pending_work);
        assert_eq!(report.alert, AlertLevel::None);
    }
    assert!(chain.sent().is_empty());
    let snapshot = load_snapshot(&chain).expect("snapshot");
    assert!(marker_of(&snapshot).text.starts_with("idle:"));
}

#[test]
fn alert_levels_follow_the_60_and_300_second_thresholds() {
    let working = "ticket:1:phase=0:cursor=0:close=none";
    let mut progress = Progress::new(1_000);
    progress.observe(1_000, working, true);
    assert_eq!(progress.alert_level(1_059), AlertLevel::None);
    assert_eq!(progress.alert_level(1_060), AlertLevel::Alert);
    assert_eq!(progress.alert_level(1_299), AlertLevel::Alert);
    assert_eq!(progress.alert_level(1_300), AlertLevel::Escalate);

    // A changed on-chain marker is progress; the same marker is not.
    progress.observe(1_300, "ticket:1:phase=0:cursor=8:close=none", true);
    assert_eq!(progress.alert_level(1_301), AlertLevel::None);
    progress.observe(1_340, "ticket:1:phase=0:cursor=8:close=none", true);
    assert_eq!(
        progress.alert_level(1_360),
        AlertLevel::Alert,
        "60 s since the marker last moved, at 1,300"
    );

    // Nothing pending is never an alert, however long it lasts.
    progress.observe(2_000, "idle:round=1:next_close=1", false);
    assert_eq!(progress.alert_level(99_999), AlertLevel::None);

    // The metrics document round-trips the state `--metrics-json` resumes from.
    let report = MetricsReport {
        now: 1_360,
        last_progress_at: 1_300,
        last_progress_marker: "ticket:1:phase=0:cursor=8:close=none".to_string(),
        seconds_since_progress: 60,
        alert_level: AlertLevel::Alert,
        pending_work: true,
        unresolved_journal_entries: 1,
        last_action: "sent".to_string(),
        dry_run: false,
    };
    let json = report.to_json();
    assert!(json.contains("\"alert_level\": \"alert\""), "{json}");
    assert_eq!(
        Progress::from_json(&json),
        Some(Progress {
            last_progress_at: 1_300,
            last_progress_marker: "ticket:1:phase=0:cursor=8:close=none".to_string(),
            pending_work: true,
        })
    );
    assert_eq!(Progress::from_json("not json"), None);
}

#[test]
fn journal_replays_the_last_line_of_each_signature() {
    let entry = |signature: &str, step: &str| JournalEntry {
        signature: signature.to_string(),
        blockhash: "hash".to_string(),
        last_valid_block_height: 150,
        step: step.to_string(),
        bytes_sha256: "00".to_string(),
        tx_hex: "ab".to_string(),
        status: JStatus::Finalized,
        at: 10,
        note: None,
    };
    let sink = MemSink::new();
    {
        let mut journal = Journal::open(Box::new(sink.clone())).expect("open");
        journal
            .record_submitted(entry("sigA", "step:1"))
            .expect("record");
        assert_eq!(
            journal.entry("sigA").expect("entry").status,
            JStatus::Submitted,
            "recorded as submitted whatever it came in as"
        );
        journal
            .update_status("sigA", JStatus::Unknown, 11, Some("reset".to_string()))
            .expect("update");
        journal
            .record_submitted(entry("sigB", "step:2"))
            .expect("record");
        journal
            .update_status("sigB", JStatus::Confirmed, 12, None)
            .expect("update");
        assert!(
            !journal.may_submit("step:1"),
            "an unknown outcome blocks its step"
        );
        assert!(journal.may_submit("step:2"), "a confirmed one does not");
        assert!(journal.may_submit("step:3"));
        assert_eq!(
            journal.update_status("nope", JStatus::Failed, 13, None),
            Err(JournalError::UnknownSignature("nope".to_string()))
        );
    }
    assert_eq!(
        sink.lines().len(),
        4,
        "two entries, two updates, nothing rewritten"
    );

    let reopened = Journal::open(Box::new(sink.clone())).expect("reopen");
    let entries = reopened.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        (
            entries[0].signature.as_str(),
            entries[0].status,
            entries[0].note.as_deref()
        ),
        ("sigA", JStatus::Unknown, Some("reset"))
    );
    assert_eq!(
        (entries[1].signature.as_str(), entries[1].status),
        ("sigB", JStatus::Confirmed)
    );
    assert_eq!(reopened.unresolved().len(), 1);
    assert!(!reopened.may_submit("step:1"));

    let corrupt = MemSink::new();
    let mut handle = corrupt.clone();
    musical_chairs_keeper::journal::JournalSink::append_line(&mut handle, "{not json")
        .expect("append");
    assert!(matches!(
        Journal::open(Box::new(corrupt)),
        Err(JournalError::Corrupt(_))
    ));
}

// ---------------------------------------------------------------------------
// IMPL-49d (Codex review R4): provider budget, OpsCash books, shared-page cleanup
// ---------------------------------------------------------------------------

fn plan_name(snapshot: &Snapshot, limits: &Limits) -> Option<&'static str> {
    next_step(snapshot, limits)
        .expect("plan")
        .map(|step| step.name())
}

#[test]
fn plans_increase_provider_budget_only_when_short_and_opted_in() {
    let needed = request_fee() + request_rent() + b_account_rent();
    let opted_in = Limits {
        max_provider_topup_lamports: 1_000_000,
        ..Limits::default()
    };
    // A close short by 400,000 lamports of reserve (fee cap ample, free operations balance ample).
    let short = topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 5_000_000);

    // Default 0: never planned. The draw request goes out and the program will refuse it: the operator did not opt in.
    assert_eq!(
        plan_name(&short, &Limits::default()),
        Some("request_round_draw")
    );

    // Opted in: exactly the shortfall, within the maximum, journaled with its amount.
    let step = next_step(&short, &opted_in).expect("plan").expect("a step");
    assert_eq!(step.name(), "increase_provider_budget");
    assert_eq!(step.key(), "increase_provider_budget:1");
    assert_eq!(&step.data()[..8], IncreaseProviderBudget::DISCRIMINATOR);
    let args = IncreaseProviderBudget::try_from_slice(&step.data()[8..]).expect("args");
    assert_eq!(args.extra, 400_000);
    assert!(args.extra <= opted_in.max_provider_topup_lamports);
    let accounts: Vec<Pubkey> = step.accounts.iter().map(|meta| meta.pubkey).collect();
    assert_eq!(
        accounts,
        vec![
            pool_address(),
            close_record_address(1),
            ops_address(),
            ops_cash_address()
        ]
    );
    assert!(step
        .journal_note()
        .expect("the amount is journaled")
        .contains("extra=400000"));
    assert!(
        idl_accounts("increase_provider_budget")
            .iter()
            .all(|account| !account.signer),
        "permissionless: no signer, and the fee payer pays nothing toward the extra"
    );

    // The program's own helper agrees: the request is refused before the top-up and accepted after it.
    let close = short.close.as_ref().expect("close");
    let holdback = promotion_holdback(close).expect("holdback");
    let (sponsor, owner) = (close.locked_sponsor_remaining, close.locked_owner_remaining);
    assert!(plan_request_spend(sponsor, owner, request_fee(), request_rent(), holdback).is_err());
    assert!(plan_request_spend(
        sponsor + args.extra,
        owner,
        request_fee(),
        request_rent(),
        holdback
    )
    .is_ok());

    // Never partial: a shortfall above the opted-in maximum is not planned (a repeat could exceed the cap).
    let one_short = Limits {
        max_provider_topup_lamports: 399_999,
        ..Limits::default()
    };
    assert_eq!(plan_name(&short, &one_short), Some("request_round_draw"));
    let exact = Limits {
        max_provider_topup_lamports: 400_000,
        ..Limits::default()
    };
    assert_eq!(plan_name(&short, &exact), Some("increase_provider_budget"));

    // The owner partition counts toward what the request can spend.
    let split = topup_snapshot(needed - 500_000, 100_000, 500_000, request_fee(), 5_000_000);
    let step = next_step(&split, &opted_in).expect("plan").expect("a step");
    assert_eq!(
        IncreaseProviderBudget::try_from_slice(&step.data()[8..])
            .expect("args")
            .extra,
        400_000
    );

    // NEGATIVE CONTROL: a close that is not short is never topped up, opted in or not.
    for sponsor in [needed, needed + 1, needed + 10_000_000] {
        let enough = topup_snapshot(sponsor, 0, 500_000, request_fee(), 5_000_000);
        assert_eq!(
            plan_name(&enough, &opted_in),
            Some("request_round_draw"),
            "sponsor {sponsor}"
        );
    }

    // A live fee above the close's fee cap is also a shortfall: the cap rises by the same extra.
    let cap_short = topup_snapshot(needed + 10_000_000, 0, 432_000, 500_000, 5_000_000);
    let step = next_step(&cap_short, &opted_in)
        .expect("plan")
        .expect("a step");
    assert_eq!(
        IncreaseProviderBudget::try_from_slice(&step.data()[8..])
            .expect("args")
            .extra,
        500_000 - 432_000
    );
    // Both gaps: the larger one decides (one extra raises both the partition and the cap).
    let both = topup_snapshot(needed - 400_000, 0, 432_000, 500_000, 5_000_000);
    let step = next_step(&both, &opted_in).expect("plan").expect("a step");
    let extra = IncreaseProviderBudget::try_from_slice(&step.data()[8..])
        .expect("args")
        .extra;
    assert_eq!(
        extra, 540_000,
        "reserve gap 400,000 + 140,000 more fee; the 68,000 cap gap is covered by it"
    );

    // NEGATIVE CONTROLS: the program would refuse these, so they are not planned.
    let poor = topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 399_999);
    assert_eq!(
        plan_name(&poor, &opted_in),
        Some("request_round_draw"),
        "free operations funds cannot cover it"
    );
    let mut unknown_fee = topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 5_000_000);
    unknown_fee.provider_fee = None;
    assert_eq!(
        plan_name(&unknown_fee, &opted_in),
        Some("request_round_draw"),
        "the live fee was not read"
    );
    let mut unknown_books = topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 5_000_000);
    unknown_books.ops = None;
    assert_eq!(
        plan_name(&unknown_books, &opted_in),
        Some("request_round_draw"),
        "the Ops books were not read"
    );
    let mut requested = topup_snapshot(needed - 400_000, 0, 500_000, request_fee(), 5_000_000);
    requested.close.as_mut().expect("close").draw_state = DRAW_REQUESTED;
    assert_ne!(
        plan_name(&requested, &opted_in),
        Some("increase_provider_budget"),
        "only before the request"
    );

    // End to end through the loader (Ops account, ORAO NetworkState fee): opted in sends the top-up and journals
    // the amount; not opted in sends the draw request.
    let payer = Keypair::new();
    let chain = MockChain::new();
    topup_world(&chain, needed - 400_000, 500_000, request_fee(), 5_000_000);
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, opted_in);
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "increase_provider_budget:1"),
        "got {:?}",
        report.action
    );
    let (accounts, data) = program_instruction(&decode_sent(&chain, 0));
    assert_eq!(&data[..8], IncreaseProviderBudget::DISCRIMINATOR);
    assert_eq!(accounts[2], ops_address());
    let entry = worker.journal().entries()[0].clone();
    assert!(
        entry.note.as_deref().unwrap_or("").contains("extra=400000"),
        "{:?}",
        entry.note
    );
    assert_eq!(entry.step, "increase_provider_budget:1");

    let chain = MockChain::new();
    topup_world(&chain, needed - 400_000, 500_000, request_fee(), 5_000_000);
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "request_round_draw:1"),
        "got {:?}",
        report.action
    );
}

#[test]
fn reconciles_ops_cash_only_when_the_predicate_holds() {
    let limits = Limits::default();
    let floor = Rent::default().minimum_balance(0);

    // In sync: nothing to reconcile.
    assert_eq!(plan_name(&books_snapshot(0), &limits), None);

    // An unsolicited credit to OpsCash: the books differ and the handler accepts the call. No ORAO Client is
    // loaded, so the plain variant.
    let donated = books_snapshot(5);
    let step = next_step(&donated, &limits).expect("plan").expect("a step");
    assert_eq!(step.name(), "reconcile_ops_cash");
    assert_eq!(step.key(), format!("reconcile_ops_cash:0:{floor}"));
    assert_eq!(
        &step.data()[..8],
        musical_chairs_program::instruction::ReconcileOpsCash::DISCRIMINATOR
    );
    let accounts: Vec<Pubkey> = step.accounts.iter().map(|meta| meta.pubkey).collect();
    assert_eq!(accounts, vec![ops_address(), ops_cash_address()]);
    assert!(idl_accounts("reconcile_ops_cash")
        .iter()
        .all(|account| !account.signer));

    // Dust below the operator's minimum is not worth a fee; at the minimum it is.
    let dust = Limits {
        min_reconcile_lamports: 10,
        ..limits
    };
    assert_eq!(plan_name(&donated, &dust), None);
    assert_eq!(
        plan_name(&books_snapshot(10), &dust),
        Some("reconcile_ops_cash")
    );
    // A changed protected-rent floor always counts, whatever the minimum.
    let mut stale_floor = books_snapshot(0);
    stale_floor.ops.as_mut().expect("ops").protected_rent = 1;
    let huge = Limits {
        min_reconcile_lamports: u64::MAX,
        ..limits
    };
    assert_eq!(plan_name(&stale_floor, &huge), Some("reconcile_ops_cash"));

    // NEGATIVE CONTROLS: a call the handler would reject is never planned.
    let mut shortfall = books_snapshot(0);
    shortfall.ops_cash.as_mut().expect("cash").lamports -= 1; // below floor + free + locked: sync_ops would fail
    shortfall.ops.as_mut().expect("ops").unallocated_excess = 99; // ... even though the books differ
    assert_eq!(plan_name(&shortfall, &limits), None);
    let mut below_floor = books_snapshot(0);
    below_floor.ops_cash = Some(SystemAccountView {
        lamports: floor - 1,
        plain_system_account: true,
    });
    below_floor.ops.as_mut().expect("ops").protected_rent = 1;
    assert_eq!(plan_name(&below_floor, &limits), None);
    let mut not_plain = books_snapshot(5);
    not_plain
        .ops_cash
        .as_mut()
        .expect("cash")
        .plain_system_account = false;
    assert_eq!(plan_name(&not_plain, &limits), None);
    let mut no_cash = books_snapshot(5);
    no_cash.ops_cash = None;
    assert_eq!(plan_name(&no_cash, &limits), None);
    let mut no_books = books_snapshot(5);
    no_books.ops = None;
    assert_eq!(plan_name(&no_books, &limits), None);

    // The ORAO-Client variant: in sync, nothing; a moved Client balance, a changed Client rent floor or a stale
    // deficit each plan it, and it also covers the OpsCash books.
    let in_sync = with_client(books_snapshot(0), 100_000);
    assert_eq!(plan_name(&in_sync, &limits), None);

    let mut moved = in_sync.clone();
    moved.client.as_mut().expect("client").lamports += 7;
    let step = next_step(&moved, &limits).expect("plan").expect("a step");
    assert_eq!(step.name(), "reconcile_ops_cash_with_client");
    assert_eq!(step.key(), "reconcile_ops_cash_with_client:0:100000");
    assert_eq!(
        &step.data()[..8],
        musical_chairs_program::instruction::ReconcileOpsCashWithClient::DISCRIMINATOR
    );
    let accounts: Vec<Pubkey> = step.accounts.iter().map(|meta| meta.pubkey).collect();
    assert_eq!(
        accounts,
        vec![
            ops_address(),
            ops_cash_address(),
            provider::client_address().0
        ]
    );
    assert!(idl_accounts("reconcile_ops_cash_with_client")
        .iter()
        .all(|account| !account.signer));
    assert_eq!(
        plan_name(&moved, &dust),
        None,
        "a 7-lamport move is below the minimum"
    );
    moved.client.as_mut().expect("client").lamports += 3;
    assert_eq!(
        plan_name(&moved, &dust),
        Some("reconcile_ops_cash_with_client")
    );

    let mut stale_rent = in_sync.clone();
    stale_rent.ops.as_mut().expect("ops").client_protected_rent = 1;
    assert_eq!(
        plan_name(&stale_rent, &huge),
        Some("reconcile_ops_cash_with_client")
    );

    let mut stale_deficit = in_sync.clone();
    stale_deficit
        .ops
        .as_mut()
        .expect("ops")
        .eligible_return_budget = 1_000_000; // budget grew since the observation
    assert_eq!(
        plan_name(&stale_deficit, &limits),
        Some("reconcile_ops_cash_with_client")
    );

    let mut both = moved.clone();
    both.ops_cash.as_mut().expect("cash").lamports += 5;
    assert_eq!(
        plan_name(&both, &limits),
        Some("reconcile_ops_cash_with_client"),
        "the superset variant"
    );

    // NEGATIVE CONTROL: a moved Client balance cannot be recorded while OpsCash itself would fail `sync_ops`.
    let mut client_but_shortfall = in_sync.clone();
    client_but_shortfall
        .client
        .as_mut()
        .expect("client")
        .lamports += 7;
    client_but_shortfall
        .ops_cash
        .as_mut()
        .expect("cash")
        .lamports -= 1;
    assert_eq!(plan_name(&client_but_shortfall, &limits), None);
    // Without a provider-owned Client at the derived address the plain variant is the only one.
    let mut no_client = in_sync.clone();
    no_client.client = None;
    no_client.ops_cash.as_mut().expect("cash").lamports += 5;
    assert_eq!(plan_name(&no_client, &limits), Some("reconcile_ops_cash"));

    // End to end through the loader.
    let payer = Keypair::new();
    let chain = MockChain::new();
    books_world(&chain, 5, None);
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == &format!("reconcile_ops_cash:0:{floor}")),
        "got {:?}",
        report.action
    );

    let chain = MockChain::new();
    books_world(&chain, 0, Some((100_000, 100_007)));
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "reconcile_ops_cash_with_client:0:100000"),
        "got {:?}",
        report.action
    );

    let chain = MockChain::new();
    books_world(&chain, 0, Some((100_000, 100_000)));
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert_eq!(report.action, Action::Idle, "books and Client in sync");
    assert!(chain.sent().is_empty());
}

#[test]
fn cleans_up_only_eligible_shared_accounts_with_the_recorded_refund_destination() {
    let limits = Limits::default();
    let recorded = key(0x88);

    // Two released pages: candidate page 0 of set 0 (owes the recorded destination an owner share) first.
    let snapshot = cleanup_snapshot();
    let step = next_step(&snapshot, &limits)
        .expect("plan")
        .expect("a cleanup");
    assert_eq!(step.name(), "close_shared_account");
    assert_eq!(step.key(), "close_shared_account:1:c0:0");
    let accounts: Vec<Pubkey> = step.accounts.iter().map(|meta| meta.pubkey).collect();
    assert_eq!(
        accounts,
        vec![
            candidate_address(1, 0, 0),
            ops_address(),
            ops_cash_address(),
            recorded,
            close_record_address(1),
            anchor_lang::solana_program::system_program::ID,
        ],
        "the refund slot holds the destination the PAGE recorded, never a caller-chosen one"
    );
    assert!(step.accounts[3].is_writable && !step.accounts[3].is_signer);
    assert_eq!(idl_accounts("close_shared_account")[3].name, "owner_refund");
    assert!(idl_accounts("close_shared_account")
        .iter()
        .all(|account| !account.signer));
    assert_eq!(&step.data()[..8], CloseSharedAccount::DISCRIMINATOR);
    let args = CloseSharedAccount::try_from_slice(&step.data()[8..]).expect("args");
    assert_eq!(
        args.kind,
        PageKind::Candidate {
            close_id: 1,
            set_tag: 0
        }
    );

    // The next page (ops share only) names OpsCash in the refund slot: nothing is paid to it.
    let mut after_first = snapshot.clone();
    after_first.shared_pages.remove(0);
    let step = next_step(&after_first, &limits)
        .expect("plan")
        .expect("a cleanup");
    assert_eq!(step.key(), "close_shared_account:1:p:0");
    let accounts: Vec<Pubkey> = step.accounts.iter().map(|meta| meta.pubkey).collect();
    assert_eq!(accounts[0], participant_address(1, 0));
    assert_eq!(
        accounts[3],
        ops_cash_address(),
        "no owner component: the slot is never paid"
    );
    let args = CloseSharedAccount::try_from_slice(&step.data()[8..]).expect("args");
    assert_eq!(args.kind, PageKind::Participant { round: 1 });

    // Nothing left: nothing planned.
    after_first.shared_pages.clear();
    assert_eq!(plan_name(&after_first, &limits), None);

    // NEGATIVE CONTROL (still funded): the judging close is still Pending, so its candidate reserve is in use.
    let mut in_use = cleanup_snapshot();
    {
        let last = in_use.last_close.as_mut().expect("last close");
        last.promotion = Promotion::Pending;
        last.finalized = false;
    }
    assert_eq!(plan_name(&in_use, &limits), None);

    // NEGATIVE CONTROL: a terminal promotion releases candidate pages but not the round's participant pages.
    let mut not_finalized = cleanup_snapshot();
    not_finalized
        .last_close
        .as_mut()
        .expect("last close")
        .finalized = false;
    let step = next_step(&not_finalized, &limits)
        .expect("plan")
        .expect("the candidate page");
    assert_eq!(step.key(), "close_shared_account:1:c0:0");
    not_finalized.shared_pages.remove(0);
    assert_eq!(
        plan_name(&not_finalized, &limits),
        None,
        "the participant page is not closable yet"
    );

    // NEGATIVE CONTROL: registry and snapshot pages are permanent, whatever they hold.
    for kind in [PageKind::Registry, PageKind::Snapshot] {
        let mut permanent = cleanup_snapshot();
        permanent.shared_pages = vec![SharedPage {
            kind,
            index: 0,
            address: registry_address(0),
            lamports: 3_000_000,
            rent: recorded_rent(),
        }];
        assert_eq!(plan_name(&permanent, &limits), None, "{kind:?}");
    }

    // NEGATIVE CONTROL: a page of some other close is not judged by the last close's record.
    let mut foreign = cleanup_snapshot();
    foreign.shared_pages = vec![candidate_shared_page(7, 0, 0, recorded_rent(), 3_000_000)];
    assert_eq!(plan_name(&foreign, &limits), None);

    // NEGATIVE CONTROLS: an owner share the recorded destination cannot take (the runtime would reject the
    // transfer), a destination that is not a plain account, one that was not loaded, and a page holding less
    // than the components it recorded are skipped, not attempted.
    let small_owner = RentComponents {
        ops: 1_000_000,
        owner: 100,
        owner_refund: recorded,
    };
    let candidate_with = |rent: RentComponents, lamports: u64| {
        let mut snapshot = cleanup_snapshot();
        snapshot.shared_pages = vec![candidate_shared_page(1, 0, 0, rent, lamports)];
        snapshot
    };
    assert_eq!(
        plan_name(&candidate_with(small_owner, 1_000_100), &limits),
        None,
        "0 + 100 lamports is below the rent floor"
    );
    let mut funded_destination = candidate_with(small_owner, 1_000_100);
    funded_destination.page_refund_targets.insert(
        recorded,
        SystemAccountView {
            lamports: Rent::default().minimum_balance(0),
            plain_system_account: true,
        },
    );
    assert_eq!(
        plan_name(&funded_destination, &limits),
        Some("close_shared_account"),
        "a destination already at the floor can take it"
    );
    let mut not_plain = candidate_with(recorded_rent(), 3_000_000);
    not_plain.page_refund_targets.insert(
        recorded,
        SystemAccountView {
            lamports: 0,
            plain_system_account: false,
        },
    );
    assert_eq!(plan_name(&not_plain, &limits), None);
    let mut not_loaded = candidate_with(recorded_rent(), 3_000_000);
    not_loaded.page_refund_targets.clear();
    assert_eq!(plan_name(&not_loaded, &limits), None);
    assert_eq!(
        plan_name(&candidate_with(recorded_rent(), 2_999_999), &limits),
        None,
        "less than ops + owner"
    );
    let mut to_ops_cash = candidate_with(
        RentComponents {
            owner_refund: ops_cash_address(),
            ..recorded_rent()
        },
        3_000_000,
    );
    to_ops_cash.page_refund_targets.insert(
        ops_cash_address(),
        SystemAccountView {
            lamports: 0,
            plain_system_account: true,
        },
    );
    assert_eq!(
        plan_name(&to_ops_cash, &limits),
        None,
        "OpsCash is never a payout destination"
    );

    // NEGATIVE CONTROL: the handler runs `sync_ops` first; books that would fail it block the cleanup.
    let mut shortfall = cleanup_snapshot();
    shortfall.ops_cash.as_mut().expect("cash").lamports -= 1;
    assert_eq!(plan_name(&shortfall, &limits), None);
    let mut no_books = cleanup_snapshot();
    no_books.ops = None;
    assert_eq!(plan_name(&no_books, &limits), None);

    // End to end through the loader: the page exists on chain and owes the recorded destination; after it is
    // closed (gone) the next pass has nothing to clean.
    let payer = Keypair::new();
    let chain = MockChain::new();
    let page = cleanup_world(&chain);
    let (journal, _sink) = memory_journal();
    let mut worker = Worker::new(&chain, &payer, journal, Limits::default());
    let report = worker.once(NOW).expect("a pass");
    assert!(
        matches!(&report.action, Action::Sent { step, .. } if step == "close_shared_account:1:c0:0"),
        "got {:?}",
        report.action
    );
    let tx = decode_sent(&chain, 0);
    assert_eq!(
        tx.message.header.num_required_signatures, 1,
        "only the fee payer signs"
    );
    let (accounts, _) = program_instruction(&tx);
    assert_eq!(accounts[0], page);
    assert_eq!(accounts[3], recorded);
    let signature = chain.sent()[0].signature;
    chain.script_status(signature, StatusAnswer::Confirmed);
    chain.remove_account(&page);
    let report = worker.once(NOW + 5).expect("a pass");
    assert_eq!(
        report.action,
        Action::Idle,
        "closed pages are gone; the books are in sync"
    );
    assert_eq!(chain.sent().len(), 1);
}
