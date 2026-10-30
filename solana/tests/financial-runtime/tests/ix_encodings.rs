//! IMPL-46a (plan M11 step 1): the byte-exact instruction encodings the
//! frontend's hand-written Borsh codecs must reproduce.  For every one of the
//! 36 instructions this renders deterministic sample arguments, Anchor's
//! 8-byte discriminator (`sha256("global:<name>")[..8]`) and the full
//! instruction data, cross-checks the discriminator against the committed IDL
//! and compares the whole rendering with `solana/fixtures/ix-encodings.json`.
//! With `MC_WRITE_IX_FIXTURE=1` it (re)writes the fixture instead.
//!
//! Account orders are NOT pinned here: the IDL carries them and the TypeScript
//! adapter reads the IDL; this fixture pins the argument bytes, which the IDL
//! cannot express as vectors.
use anchor_lang::{Discriminator, InstructionData};
use musical_chairs_program::{
    controls::{ConfigTerms, Funding, InitializeArgs, PageKind},
    instruction as ix,
    positions::PlanKindArg,
    state::ConfigVersion,
};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/ix-encodings.json");
const IDL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/musical_chairs/idl/musical_chairs.json");

fn key(n: u8) -> Pubkey {
    Pubkey::new_from_array([n; 32])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn entry<T: InstructionData + Discriminator>(name: &str, sample: Value, data: T) -> Value {
    let bytes = data.data();
    assert_eq!(&bytes[..8], T::DISCRIMINATOR, "{name}: data starts with the discriminator");
    json!({ "name": name, "discriminator_hex": hex(T::DISCRIMINATOR), "sample": sample, "data_hex": hex(&bytes), "data_len": bytes.len() })
}

fn render_all() -> Vec<Value> {
    let terms = ConfigTerms::from(&ConfigVersion::default_v1());
    let args = InitializeArgs {
        sponsor: key(0xA1),
        management_recipient: key(0xA2),
        cluster_domain: *b"musical-chairs-fixture-domain!!!",
        provider_request_fee_lamports: 300_000,
        provider_fee_headroom_bps: 2_500,
        provider_program: key(0xB1),
        provider_network_state: key(0xB2),
        provider_treasury: key(0xB3),
        provider_client: key(0xB4),
        public_series_a_enabled: true,
    };
    vec![
        entry("initialize", json!({ "args": format!("{args:?}") }), ix::Initialize { args }),
        entry("set_pause", json!({ "paused": true }), ix::SetPause { paused: true }),
        entry("fund_operations", json!({ "amount": 1_000_000 }), ix::FundOperations { amount: 1_000_000 }),
        entry("withdraw_operations", json!({ "amount": 2_000_000 }), ix::WithdrawOperations { amount: 2_000_000 }),
        entry("withdraw_management", json!({ "amount": 3_000_000 }), ix::WithdrawManagement { amount: 3_000_000 }),
        entry("stage_config_version", json!({ "version": 2, "terms": format!("{terms:?}") }), ix::StageConfigVersion { version: 2, terms }),
        entry("activate_config_version", json!({ "version": 2, "expected_current": 1 }), ix::ActivateConfigVersion { version: 2, expected_current: 1 }),
        entry("rotate_recipient", json!({ "kind": 1, "new_recipient": key(0xC1).to_string() }), ix::RotateRecipient { kind: 1, new_recipient: key(0xC1) }),
        entry("rotate_guardian", json!({}), ix::RotateGuardian {}),
        entry("provision_page", json!({ "kind": "Participant { round: 3 }", "index": 2, "funding": "Free" }), ix::ProvisionPage { kind: PageKind::Participant { round: 3 }, index: 2, funding: Funding::Free }),
        entry("provision_page_candidate_caller", json!({ "kind": "Candidate { close_id: 7, set_tag: 1 }", "index": 0, "funding": "Caller" }), ix::ProvisionPage { kind: PageKind::Candidate { close_id: 7, set_tag: 1 }, index: 0, funding: Funding::Caller }),
        entry("provision_close_record", json!({ "close_id": 7, "funding": "LockedClose" }), ix::ProvisionCloseRecord { close_id: 7, funding: Funding::LockedClose }),
        entry("close_shared_account", json!({ "kind": "Registry" }), ix::CloseSharedAccount { kind: PageKind::Registry }),
        entry("reconcile_ops_cash", json!({}), ix::ReconcileOpsCash {}),
        entry("open_position", json!({ "plan": "Simple", "gross": 1_000_000_000u64, "client_nonce": 1, "expected_round_id": 1, "expected_config": 1 }), ix::OpenPosition { plan: PlanKindArg::Simple, gross: 1_000_000_000, client_nonce: 1, expected_round_id: 1, expected_config: 1 }),
        entry("open_position_compound15", json!({ "plan": "Compound15", "gross": 10_000_000u64, "client_nonce": 2, "expected_round_id": 1, "expected_config": 1 }), ix::OpenPosition { plan: PlanKindArg::Compound15, gross: 10_000_000, client_nonce: 2, expected_round_id: 1, expected_config: 1 }),
        entry("open_position_compound30", json!({ "plan": "Compound30", "gross": 10_000_000u64, "client_nonce": 3, "expected_round_id": 2, "expected_config": 2 }), ix::OpenPosition { plan: PlanKindArg::Compound30, gross: 10_000_000, client_nonce: 3, expected_round_id: 2, expected_config: 2 }),
        entry("fund_series_a", json!({ "gross": 50_000_000u64, "expected_round_id": 1 }), ix::FundSeriesA { gross: 50_000_000, expected_round_id: 1 }),
        entry("withdraw_simple", json!({ "minimum_receive": 96_800_000u64, "max_work_contribution": 0 }), ix::WithdrawSimple { minimum_receive: 96_800_000, max_work_contribution: 0 }),
        entry("settle_compounding", json!({ "minimum_receive": 0, "max_work_contribution": 5_000_000u64 }), ix::SettleCompounding { minimum_receive: 0, max_work_contribution: 5_000_000 }),
        entry("close_simple_position", json!({}), ix::CloseSimplePosition {}),
        entry("discover_rows", json!({ "expected_ticket_id": 1, "expected_cursor": 0, "rows": 8 }), ix::DiscoverRows { expected_ticket_id: 1, expected_cursor: 0, rows: 8 }),
        entry("apply_senior", json!({ "expected_ticket_id": 1 }), ix::ApplySenior { expected_ticket_id: 1 }),
        entry("seal_weights", json!({ "expected_ticket_id": 1 }), ix::SealWeights { expected_ticket_id: 1 }),
        entry("allocate_rows", json!({ "expected_ticket_id": 1, "expected_cursor": 8, "rows": 8 }), ix::AllocateRows { expected_ticket_id: 1, expected_cursor: 8, rows: 8 }),
        entry("prune_rows", json!({ "expected_ticket_id": 1, "expected_cursor": 0, "rows": 1 }), ix::PruneRows { expected_ticket_id: 1, expected_cursor: 0, rows: 1 }),
        entry("commit_ticket", json!({ "expected_ticket_id": 1 }), ix::CommitTicket { expected_ticket_id: 1 }),
        entry("claim_backer", json!({}), ix::ClaimBacker {}),
        entry("build_candidates", json!({ "rows": 8 }), ix::BuildCandidates { rows: 8 }),
        entry("request_round_draw", json!({}), ix::RequestRoundDraw {}),
        entry("capture_round_draw", json!({}), ix::CaptureRoundDraw {}),
        entry("apply_promotion", json!({}), ix::ApplyPromotion {}),
        entry("expire_promotion", json!({}), ix::ExpirePromotion {}),
        entry("finalize_round", json!({}), ix::FinalizeRound {}),
        entry("increase_provider_budget", json!({ "extra": 100_000 }), ix::IncreaseProviderBudget { extra: 100_000 }),
        entry("observe_request_fulfilled", json!({}), ix::ObserveRequestFulfilled {}),
        entry("withdraw_provider_return", json!({ "amount": 4_259_520u64 }), ix::WithdrawProviderReturn { amount: 4_259_520 }),
        entry("reconcile_ops_cash_with_client", json!({}), ix::ReconcileOpsCashWithClient {}),
        entry("deliver_owner_refund", json!({}), ix::DeliverOwnerRefund {}),
    ]
}

#[test]
fn instruction_encodings_match_the_checked_in_fixture_and_the_idl() {
    let entries = render_all();
    // Cross-check every discriminator against the committed IDL (36 distinct instructions).
    let idl: Value = serde_json::from_str(&std::fs::read_to_string(IDL).expect("idl")).expect("idl json");
    let idl_disc: std::collections::BTreeMap<String, String> = idl["instructions"].as_array().unwrap().iter().map(|i| {
        let d: Vec<u8> = i["discriminator"].as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect();
        (i["name"].as_str().unwrap().to_string(), hex(&d))
    }).collect();
    assert_eq!(idl_disc.len(), 36, "the IDL lists 36 instructions");
    let mut covered = std::collections::BTreeSet::new();
    for e in &entries {
        let name = e["name"].as_str().unwrap();
        let base = ["_compound15", "_compound30", "_candidate_caller"].iter().find_map(|suffix| name.strip_suffix(suffix)).unwrap_or(name);
        assert_eq!(e["discriminator_hex"], json!(idl_disc[base]), "{name}: discriminator equals the IDL's");
        covered.insert(base.to_string());
    }
    assert_eq!(covered.len(), 36, "every IDL instruction has at least one encoding sample");
    let rendered = serde_json::to_string_pretty(&json!({ "program": musical_chairs_program::ID.to_string(), "note": "Anchor instruction data = sha256(\"global:<name>\")[..8] ++ Borsh(args); account orders come from the IDL", "instructions": entries })).unwrap() + "\n";
    if std::env::var("MC_WRITE_IX_FIXTURE").as_deref() == Ok("1") {
        std::fs::write(FIXTURE, &rendered).expect("write ix-encodings.json");
        return;
    }
    let on_disk = std::fs::read_to_string(FIXTURE).expect("solana/fixtures/ix-encodings.json is missing; regenerate with MC_WRITE_IX_FIXTURE=1");
    assert_eq!(on_disk, rendered, "instruction encodings drifted from solana/fixtures/ix-encodings.json");
}
