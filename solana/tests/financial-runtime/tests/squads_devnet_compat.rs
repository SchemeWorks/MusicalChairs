//! M14-PRE-SQUADS: the program's governance genesis against the REAL Squads v4
//! executable that is deployed on devnet (`SQDS4ep6...`), in LiteSVM.
//!
//! Every other suite uses the test-only vault stand-in, which signs as the vault
//! PDA and knows nothing about proposals, thresholds or time locks.  Here the
//! real program creates the G and P multisigs, takes a vault transaction through
//! `vault_transaction_create`, `proposal_create`, two `proposal_approve`s, a
//! 48-hour time lock and `vault_transaction_execute`, and the program's own
//! `initialize` / ORAO `register` / `set_pause` run inside that CPI.
//!
//! # What is real and what is synthetic
//!
//! * REAL: the Squads program bytes and its `ProgramConfig` and treasury
//!   snapshots (`solana/fixtures/devnet/squads-devnet*.json|so`), every
//!   instruction's execution, the multisig / proposal / vault-transaction
//!   accounts it creates, its error codes, and the time-lock arithmetic against
//!   the LiteSVM clock.  The ORAO test additionally loads the real DEVNET ORAO
//!   binary.
//! * SYNTHETIC: all keys (create keys, the three G and three P members, the
//!   sponsor) are fresh in-test keypairs; the clock is warped; the ORAO network
//!   state is created locally with one synthetic fulfil authority; rent is
//!   LiteSVM's default schedule; no transaction ever left this process.
//!
//! # The question this file answers
//!
//! `initialize` needs the sponsor as a KEYPAIR signer next to the G vault (a
//! PDA).  Can a Squads vault transaction forward such a signer when that
//! keypair signs the OUTER `vault_transaction_execute` transaction?  The test
//! `initialize_through_real_squads_with_the_sponsor_signing_the_outer_
//! transaction_records_the_outcome` runs exactly that and RECORDS the outcome
//! (`FINDING ...` on stderr, and `$MC_DEVNET_COMPAT_EVIDENCE_DIR/<name>.json`
//! when set) instead of asserting either answer.  The tests after it use the
//! G vault itself as the sponsor, which works whatever the answer is, and
//! exercise the profile checks, the ORAO registration and the guardian path.
//!
//! Run (the fixture tests need no profile; nothing here is `#[ignore]`):
//!
//! ```text
//! cargo test --locked --test squads_devnet_compat -- --nocapture --test-threads=1
//! ```

use musical_chairs_financial_runtime::{
    controls::custom_code,
    devnet::{self, OraoProfile},
    runtime::{fixtures, real_g_create_keypair, real_p_create_keypair},
    squads_real::{self as sq, Plan, RealSquadsWorld},
    world::code,
};
use musical_chairs_program::{
    controls::{InitializeArgs, SQUADS_PROGRAM_ID},
    provider::{self, orao},
    state, ErrorCode, ID,
};
use serde_json::json;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

fn key(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

fn sponsor_keypair(world: &RealSquadsWorld) -> Keypair {
    Keypair::new_from_array(*world.rt.sponsor.secret_bytes())
}

fn assert_created(world: &mut RealSquadsWorld) {
    let (g, p) = world.create_g_and_p();
    assert!(g.success, "G multisig_create_v2 failed: {g:?}");
    assert!(p.success, "P multisig_create_v2 failed: {p:?}");
}

/// Genesis through the real G multisig with the G VAULT as the sponsor: the one
/// signer Squads always provides.  Asserts the stand-in genesis post-conditions.
fn genesis_with_g_vault_sponsor(world: &mut RealSquadsWorld) -> InitializeArgs {
    world.fund_g_vault(10_000_000_000);
    let vault = world.rt.g_vault();
    let args = world.g_vault_sponsor_args();
    let initialize = world.initialize_instruction_with_sponsor(args, vault);
    let run = world.run_g("squads.initialize.g_vault_sponsor", initialize, &[]);
    assert!(
        run.succeeded(),
        "initialize with the G vault as sponsor failed: {}",
        sq::summarize_run(&run)
    );
    sq::assert_initialized(&world.rt, &args);
    args
}

// ---------------------------------------------------------------------------
// Fixtures: pins, IDL, packet
// ---------------------------------------------------------------------------

#[test]
fn squads_devnet_fixture_is_pinned_and_the_hand_built_instructions_match_the_idl() {
    let elf = devnet::read_squads_devnet_elf();
    assert_eq!(
        elf.len(),
        980_160,
        "ELF length from the section header table"
    );
    assert_eq!(devnet::sha256_hex(&elf), devnet::SQUADS_DEVNET_ELF_SHA256);
    assert_eq!(devnet::elf64_extent(&elf), Ok(elf.len()));
    let mut payload = elf.clone();
    payload.resize(1_774_800, 0);
    assert_eq!(
        devnet::sha256_hex(&payload),
        "97fb2a5c08e5df5e862d8354ac5b97d3587117f98c4457754ca12e30bef0664d",
        "the packet's R5 payload hash"
    );

    // ProgramConfig: the PDA the packet names, free multisig creation, the packet's treasury.
    let accounts = devnet::load_accounts(devnet::SQUADS_DEVNET_ACCOUNTS_FILE);
    let config = &accounts["program_config"];
    assert_eq!(sq::program_config_address(), config.address);
    assert_eq!(config.address.to_string(), sq::PACKET_PROGRAM_CONFIG);
    assert_eq!(config.owner, SQUADS_PROGRAM_ID);
    assert_eq!(config.data.len(), 144);
    assert_eq!(
        u64::from_le_bytes(config.data[40..48].try_into().unwrap()),
        0,
        "multisig_creation_fee"
    );
    let treasury = &accounts["treasury"];
    assert_eq!(
        Pubkey::new_from_array(config.data[48..80].try_into().unwrap()),
        treasury.address
    );
    assert!(treasury.address.to_string().starts_with("HM5y4mz3"));

    // IDL: version, and the Multisig field order the program's validate_profile reads.
    let idl = sq::idl();
    assert_eq!(idl["version"].as_str(), Some("2.1.0"));
    assert_eq!(idl["name"].as_str(), Some("squads_multisig_program"));
    let multisig_fields: Vec<&str> = idl["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["name"].as_str() == Some("Multisig"))
        .expect("Multisig account in the IDL")["type"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        multisig_fields,
        [
            "createKey",
            "configAuthority",
            "threshold",
            "timeLock",
            "transactionIndex",
            "staleTransactionIndex",
            "rentCollector",
            "bump",
            "members"
        ],
        "validate_profile reads the Multisig fields in this order"
    );

    // Every builder against the IDL: account order, mutability, signer flags, discriminator.
    let members = [(key(4), 7), (key(5), 7), (key(6), 7)];
    let (create_v2, _) = sq::multisig_create_v2_ix(key(1), key(2), key(3), &members, 2, 172_800);
    sq::assert_matches_idl(&idl, "multisigCreateV2", &create_v2);
    let inner = solana_system_interface::instruction::transfer(&key(7), &key(8), 1);
    let compiled = sq::compile_vault_message(&inner);
    let vault_create =
        sq::vault_transaction_create_ix(key(9), 1, key(10), key(11), 0, 0, &compiled.bytes);
    sq::assert_matches_idl(&idl, "vaultTransactionCreate", &vault_create);
    let proposal_create = sq::proposal_create_ix(key(9), 1, key(10), key(11), false);
    sq::assert_matches_idl(&idl, "proposalCreate", &proposal_create);
    let approve = sq::proposal_approve_ix(key(9), 1, key(10));
    sq::assert_matches_idl(&idl, "proposalApprove", &approve);
    let execute = sq::vault_transaction_execute_ix(
        key(9),
        1,
        key(10),
        compiled.remaining_accounts(&key(7), &[]),
    );
    sq::assert_matches_idl(&idl, "vaultTransactionExecute", &execute);

    // The compiled message of a plain transfer, byte for byte: counters, three
    // keys (vault signer-writable, destination writable, system program
    // read-only), one instruction.
    let transfer_data = &inner.data;
    assert_eq!(transfer_data.len(), 12);
    assert_eq!(compiled.bytes[..4], [1, 1, 1, 3]);
    assert_eq!(
        compiled.bytes.len(),
        3 + 1 + 3 * 32 + 1 + 1 + 1 + 2 + 2 + 12 + 1
    );
    assert_eq!(compiled.keys[0], (key(7), true, true));
    assert_eq!(compiled.keys[1], (key(8), false, true));
    assert_eq!(
        compiled.keys[2],
        (Pubkey::default(), false, false),
        "the system program is read-only and last"
    );
    assert_eq!(
        compiled.bytes[3 + 1 + 96..3 + 1 + 96 + 6],
        [1, 2, 2, 0, 1, 12]
    );
    assert_eq!(
        sq::snake_case("vaultTransactionExecute"),
        "vault_transaction_execute"
    );
}

// ---------------------------------------------------------------------------
// Real multisig creation and the time lock
// ---------------------------------------------------------------------------

#[test]
fn real_squads_creates_g_and_p_exactly_as_the_stand_in_fixtures_describe_them() {
    let mut world = RealSquadsWorld::new();
    assert_eq!(
        world.rt.real_squads_artifact_sha256(),
        Some(devnet::SQUADS_DEVNET_ELF_SHA256)
    );
    assert!(
        world.rt.standin_artifact_sha256().is_none(),
        "the stand-in is not loaded in the real-Squads runtime"
    );
    let (g, p) = world.create_g_and_p();
    eprintln!(
        "multisig_create_v2: G {} CU {:?} packet {} B, P {} CU {:?} packet {} B",
        g.success, g.compute_units, g.packet_bytes, p.success, p.compute_units, p.packet_bytes
    );
    assert!(g.success, "G multisig_create_v2 failed: {g:?}");
    assert!(p.success, "P multisig_create_v2 failed: {p:?}");

    for (name, multisig, create_key, time_lock, members) in [
        (
            "G",
            world.rt.g_multisig(),
            real_g_create_keypair().pubkey(),
            sq::G_TIME_LOCK,
            RealSquadsWorld::member_specs(&world.g_members),
        ),
        (
            "P",
            world.rt.p_multisig(),
            real_p_create_keypair().pubkey(),
            sq::P_TIME_LOCK,
            RealSquadsWorld::member_specs(&world.p_members),
        ),
    ] {
        let account = world.rt.account(multisig);
        assert_eq!(
            account.owner, SQUADS_PROGRAM_ID,
            "{name}: owned by the real Squads program"
        );
        assert_eq!(
            account.data.len(),
            fixtures::squads::multisig_space(3),
            "{name}: allocation"
        );
        assert_eq!(
            account.lamports,
            world.rt.rent(account.data.len()),
            "{name}: rent"
        );

        let mut sorted = members.clone();
        sorted.sort_by_key(|(member, _)| *member);
        let (_, bump) = fixtures::squads::multisig_address(&create_key);
        let view = sq::parse_multisig(&account.data);
        assert_eq!(view.create_key, create_key, "{name}");
        assert_eq!(
            view.config_authority,
            Pubkey::default(),
            "{name}: autonomous"
        );
        assert_eq!(view.threshold, 2, "{name}");
        assert_eq!(view.time_lock, time_lock, "{name}");
        assert_eq!(view.transaction_index, 0, "{name}");
        assert_eq!(view.stale_transaction_index, 0, "{name}");
        assert_eq!(view.rent_collector, None, "{name}");
        assert_eq!(view.bump, bump, "{name}: canonical bump");
        assert_eq!(
            view.members, sorted,
            "{name}: three members, all permissions, key order"
        );
        assert_eq!(
            view.trailing_bytes, 32,
            "{name}: the unused Option<Pubkey> allocation"
        );

        // The harness' byte-exact stand-in fixture generator produces the same
        // bytes the real program wrote (plus the zero allocation padding).
        let expected = fixtures::squads::multisig_bytes(
            create_key,
            Pubkey::default(),
            2,
            time_lock,
            None,
            bump,
            &sorted,
        );
        assert_eq!(
            &account.data[..expected.len()],
            expected.as_slice(),
            "{name}: bytes"
        );
        assert!(
            account.data[expected.len()..].iter().all(|byte| *byte == 0),
            "{name}: padding"
        );
    }
    assert_ne!(world.rt.g_multisig(), world.rt.p_multisig());
    assert_ne!(world.rt.g_vault(), world.rt.p_vault());
}

#[test]
fn real_squads_time_lock_blocks_early_execution_and_releases_after_it() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    world.fund_g_vault(1_000_000_000);
    let vault = world.rt.g_vault();
    let destination = key(0xDE);
    let inner = solana_system_interface::instruction::transfer(&vault, &destination, 1_000_000);
    let started = sq::now(&world.rt);
    let run = world.run_g("squads.time_lock", inner, &[]);
    eprintln!("time-lock run: {}", sq::summarize_run(&run));
    assert!(run.first_failure().is_none(), "{}", sq::summarize_run(&run));

    let early = run.early_execute.as_ref().expect("the early probe ran");
    assert!(
        !early.success,
        "execution before the time lock must be refused: {early:?}"
    );
    assert_eq!(
        custom_code(early),
        Some(sq::idl_error_code(&sq::idl(), "TimeLockNotReleased")),
        "{early:?}"
    );
    early.assert_rollback_except_fee();
    assert_eq!(world.rt.account(vault).lamports, 1_000_000_000 - 1_000_000);
    let execute = run.execute.as_ref().expect("final execute ran");
    assert!(execute.success, "{execute:?}");
    assert_eq!(world.rt.account(destination).lamports, 1_000_000);
    assert!(
        sq::now(&world.rt) - started > i64::from(sq::G_TIME_LOCK),
        "the clock moved past the 48 h lock"
    );

    let multisig = world.rt.g_multisig();
    let view = sq::parse_multisig(&world.rt.account(multisig).data);
    assert_eq!(view.transaction_index, 1);
    assert_eq!(run.transaction_index, 1);
    let proposal = world.rt.account(sq::proposal_address(&multisig, 1));
    assert_eq!(sq::proposal_status_name(&proposal.data), "Executed");
}

#[test]
fn real_squads_threshold_two_is_enforced_one_approval_cannot_execute() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    world.fund_g_vault(1_000_000_000);
    let vault = world.rt.g_vault();
    let multisig = world.rt.g_multisig();
    let inner = solana_system_interface::instruction::transfer(&vault, &key(0xDE), 1_000_000);
    let run = sq::run_vault_transaction(
        &mut world.rt,
        &Plan {
            tag: "squads.one_approval",
            multisig,
            vault_index: 0,
            inner,
            proposer: &world.g_members[0],
            approvers: vec![&world.g_members[0]],
            executor: &world.g_members[0],
            outer_signers: Vec::new(),
            ephemeral_signers: 0,
            time_lock: 0,
            probe_early_execute: false,
        },
    );
    assert!(run.create.as_ref().unwrap().success);
    assert!(run.proposal.as_ref().unwrap().success);
    assert!(run.approvals[0].success, "{:?}", run.approvals[0]);
    let execute = run.execute.as_ref().expect("execute attempted");
    assert!(
        !execute.success,
        "one of two approvals must not execute: {execute:?}"
    );
    assert_eq!(
        custom_code(execute),
        Some(sq::idl_error_code(&sq::idl(), "InvalidProposalStatus")),
        "{execute:?}"
    );
    let proposal = world.rt.account(sq::proposal_address(&multisig, 1));
    assert_eq!(sq::proposal_status_name(&proposal.data), "Active");
}

// ---------------------------------------------------------------------------
// The finding: does Squads forward the sponsor keypair?
// ---------------------------------------------------------------------------

#[test]
fn initialize_through_real_squads_with_the_sponsor_signing_the_outer_transaction_records_the_outcome(
) {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    let args = world.devnet_initialize_args();
    let initialize = world.initialize_instruction(args);
    assert!(
        initialize
            .accounts
            .iter()
            .any(|meta| meta.pubkey == args.sponsor && meta.is_signer && meta.is_writable),
        "the inner instruction needs the sponsor as a writable signer"
    );
    let sponsor = sponsor_keypair(&world);
    let run = world.run_g(
        "squads.initialize.sponsor_outer_signer",
        initialize,
        &[&sponsor],
    );

    let compiled_signers = {
        let compiled = sq::compile_vault_message(&world.initialize_instruction(args));
        compiled
            .outer_signer_keys(&world.rt.g_vault(), &[])
            .into_iter()
            .map(|key| key.to_string())
            .collect::<Vec<_>>()
    };
    let outcome = match run.first_failure() {
        None => "SUCCESS: Squads forwarded the sponsor signer; initialize ran",
        Some((stage, _)) => match stage {
            "vault_transaction_create" => {
                "REFUSED at vault_transaction_create (the message was rejected when stored)"
            }
            "vault_transaction_execute" => "REFUSED at vault_transaction_execute",
            _ => "FAILED before execution (harness problem, not a Squads verdict)",
        },
    };
    sq::record_finding(
        "initialize_sponsor_outer_signer",
        &json!({
            "question": "does a Squads v4 vault transaction forward a non-vault keypair signer that signs the outer vault_transaction_execute transaction?",
            "outcome": outcome,
            "message_signers_besides_the_vault": compiled_signers,
            "run": sq::summarize_run(&run),
        }),
    );

    match run.first_failure() {
        None => {
            let early = run.early_execute.as_ref().expect("early probe");
            assert!(
                !early.success,
                "the 48 h lock must hold before the second approval settles"
            );
            assert_eq!(
                custom_code(early),
                Some(sq::idl_error_code(&sq::idl(), "TimeLockNotReleased"))
            );
            sq::assert_initialized(&world.rt, &args);
            let pool: state::Pool = world.rt.deserialize(world.rt.pool_address());
            assert_eq!(
                pool.sponsor,
                world.rt.sponsor.pubkey(),
                "the keypair sponsor, not the vault"
            );
            assert_eq!(pool.governance, world.rt.g_vault());
            assert_eq!(pool.guardian_vault, world.rt.p_vault());
        }
        Some((stage, failed)) => {
            // A refusal is a finding, not a harness failure: it must at least be
            // clean (nothing but the fee changed) and must not be one of the
            // earlier, unrelated stages.
            failed.assert_rollback_except_fee();
            assert!(
                world.rt.svm.get_account(&world.rt.pool_address()).is_none(),
                "a refused genesis must leave no Pool behind"
            );
            assert!(
                stage == "vault_transaction_execute" || stage == "vault_transaction_create",
                "{stage} failed for a reason unrelated to the signer question: {failed:?}"
            );
        }
    }
}

/// The documented Squads pattern for extra signers: declare an EPHEMERAL SIGNER,
/// a PDA of the transaction that Squads signs for, and make it the sponsor.  An
/// external keypair cannot be one (it is a PDA with no private key), so this
/// can only ever prove that a PDA sponsor works mechanically; it is NOT a
/// candidate genesis shape (the sponsor role would be unusable afterwards).
/// Outcome recorded, not asserted.
#[test]
fn initialize_with_a_squads_ephemeral_signer_as_sponsor_records_the_outcome() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    let multisig = world.rt.g_multisig();
    let index = sq::parse_multisig(&world.rt.account(multisig).data).transaction_index + 1;
    let transaction = sq::transaction_address(&multisig, index);
    let ephemeral = sq::ephemeral_signer_address(&transaction, 0);
    world.rt.credit_lamports(ephemeral, 10_000_000_000);

    let mut args = world.devnet_initialize_args();
    args.sponsor = ephemeral;
    let initialize = world.initialize_instruction_with_sponsor(args, ephemeral);
    let run = sq::run_vault_transaction(
        &mut world.rt,
        &Plan {
            tag: "squads.initialize.ephemeral_sponsor",
            multisig,
            vault_index: 0,
            inner: initialize,
            proposer: &world.g_members[0],
            approvers: vec![&world.g_members[0], &world.g_members[1]],
            executor: &world.g_members[0],
            outer_signers: Vec::new(),
            ephemeral_signers: 1,
            time_lock: sq::G_TIME_LOCK,
            probe_early_execute: false,
        },
    );
    sq::record_finding(
        "initialize_ephemeral_signer_sponsor",
        &json!({
            "ephemeral_signer": ephemeral.to_string(),
            "seed_derivation": "[\"multisig\", transaction, \"ephemeral_signer\", [index]] under the Squads program (from the Squads source; a mismatch would show as InvalidAccount)",
            "note": "an external keypair cannot be an ephemeral signer: it is a PDA nobody holds a key for",
            "run": sq::summarize_run(&run),
        }),
    );
    if run.succeeded() {
        sq::assert_initialized(&world.rt, &args);
    } else if let Some((_, failed)) = run.first_failure() {
        failed.assert_rollback_except_fee();
    }
}

// ---------------------------------------------------------------------------
// Genesis with the G vault as sponsor, and the profile checks on real accounts
// ---------------------------------------------------------------------------

#[test]
fn initialize_through_real_squads_with_the_g_vault_as_sponsor_runs_the_full_governance_path() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    let args = genesis_with_g_vault_sponsor(&mut world);

    let pool: state::Pool = world.rt.deserialize(world.rt.pool_address());
    let g_vault = world.rt.g_vault();
    assert_eq!(pool.governance, g_vault);
    assert_eq!(pool.guardian_vault, world.rt.p_vault());
    assert_eq!(
        pool.sponsor, g_vault,
        "the sponsor role is the G vault in this variant"
    );
    assert_eq!(pool.cluster_domain, args.cluster_domain);
    // The packet's provider tuple was accepted as written.
    let config: state::ConfigVersion = world.rt.deserialize(world.rt.config_address(1));
    assert_eq!(config.provider_program, orao::id());
    assert_eq!(
        config.provider_network_state.to_string(),
        sq::PACKET_ORAO_NETWORK_STATE
    );
    assert_eq!(
        config.provider_treasury.to_string(),
        sq::DEVNET_ORAO_TREASURY
    );
    assert_eq!(config.provider_client.to_string(), sq::PACKET_ORAO_CLIENT);
    assert_eq!(config.provider_client, provider::client_address().0);
    assert_eq!(config.provider_request_fee_lamports, 300_000);
    assert_eq!(config.provider_fee_headroom_bps, 2_500);

    // Squads state after the run: one executed transaction on G, none on P.
    let g = sq::parse_multisig(&world.rt.account(world.rt.g_multisig()).data);
    let p = sq::parse_multisig(&world.rt.account(world.rt.p_multisig()).data);
    assert_eq!((g.transaction_index, p.transaction_index), (1, 0));
    let proposal = world
        .rt
        .account(sq::proposal_address(&world.rt.g_multisig(), 1));
    assert_eq!(sq::proposal_status_name(&proposal.data), "Executed");
}

/// One deliberately wrong G profile to build with real `multisig_create_v2`.
struct WrongProfile {
    label: &'static str,
    member_tag: u8,
    create_seed: [u8; 32],
    members: u8,
    threshold: u16,
    time_lock: u32,
}

#[test]
fn initialize_rejects_wrong_governance_profiles_built_by_real_squads() {
    let wrong = [
        WrongProfile {
            label: "time_lock_3600",
            member_tag: 0x62,
            create_seed: [0x48; 32],
            members: 3,
            threshold: 2,
            time_lock: 3_600,
        },
        WrongProfile {
            label: "threshold_3_of_3",
            member_tag: 0x63,
            create_seed: [0x49; 32],
            members: 3,
            threshold: 3,
            time_lock: sq::G_TIME_LOCK,
        },
        WrongProfile {
            label: "two_members",
            member_tag: 0x64,
            create_seed: [0x4a; 32],
            members: 2,
            threshold: 2,
            time_lock: sq::G_TIME_LOCK,
        },
        WrongProfile {
            label: "no_time_lock",
            member_tag: 0x65,
            create_seed: [0x4b; 32],
            members: 3,
            threshold: 2,
            time_lock: 0,
        },
    ];
    for profile in wrong {
        let label = profile.label;
        let mut world = RealSquadsWorld::new();
        assert_created(&mut world);
        let rogue = world.create_extra_multisig(
            label,
            profile.member_tag,
            profile.create_seed,
            profile.members,
            profile.threshold,
            profile.time_lock,
        );
        world.rt.credit_lamports(rogue.vault, 10_000_000_000);

        let mut args = world.devnet_initialize_args();
        args.sponsor = rogue.vault;
        let mut initialize = world.initialize_instruction_with_sponsor(args, rogue.vault);
        sq::replace_key(&mut initialize, world.rt.g_multisig(), rogue.multisig);
        sq::replace_key(&mut initialize, world.rt.g_vault(), rogue.vault);

        let run = rogue.run(&mut world.rt, label, initialize);
        assert!(
            run.create.as_ref().unwrap().success,
            "{label}: {}",
            sq::summarize_run(&run)
        );
        assert!(
            run.proposal.as_ref().unwrap().success,
            "{label}: {}",
            sq::summarize_run(&run)
        );
        assert!(
            run.approvals.iter().all(|o| o.success),
            "{label}: {}",
            sq::summarize_run(&run)
        );
        let execute = run.execute.as_ref().expect("execute attempted");
        assert!(
            !execute.success,
            "{label}: a wrong profile must not initialize: {execute:?}"
        );
        assert_eq!(
            custom_code(execute),
            Some(code(ErrorCode::WrongGovernanceProfile)),
            "{label}: {execute:?}"
        );
        execute.assert_rollback_except_fee();
        assert!(
            world.rt.svm.get_account(&world.rt.pool_address()).is_none(),
            "{label}: no Pool"
        );
    }
}

#[test]
fn initialize_rejects_a_wrong_guardian_profile_built_by_real_squads() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    world.fund_g_vault(10_000_000_000);
    let rogue_p = world.create_extra_multisig("P_time_lock_60", 0x72, [0x51; 32], 3, 2, 60);

    let vault = world.rt.g_vault();
    let args = world.g_vault_sponsor_args();
    let mut initialize = world.initialize_instruction_with_sponsor(args, vault);
    sq::replace_key(&mut initialize, world.rt.p_multisig(), rogue_p.multisig);
    sq::replace_key(&mut initialize, world.rt.p_vault(), rogue_p.vault);
    // The REAL G (and its real 48 h time lock) is the executor; the guardian is the wrong one.
    let run = world.run_g("squads.initialize.wrong_guardian", initialize, &[]);
    assert!(
        run.create.as_ref().unwrap().success,
        "{}",
        sq::summarize_run(&run)
    );
    let early = run.early_execute.as_ref().expect("early probe");
    assert_eq!(
        custom_code(early),
        Some(sq::idl_error_code(&sq::idl(), "TimeLockNotReleased")),
        "{early:?}"
    );
    let execute = run.execute.as_ref().expect("execute attempted");
    assert!(!execute.success, "{execute:?}");
    assert_eq!(
        custom_code(execute),
        Some(code(ErrorCode::WrongGovernanceProfile)),
        "{execute:?}"
    );
    execute.assert_rollback_except_fee();
    assert!(world.rt.svm.get_account(&world.rt.pool_address()).is_none());
}

#[test]
fn initialize_rejects_a_governance_signer_that_is_not_vault_index_zero() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    let g_multisig = world.rt.g_multisig();
    let vault_one = fixtures::squads::vault_address(&g_multisig, 1).0;
    world.rt.credit_lamports(vault_one, 10_000_000_000);

    let mut args = world.devnet_initialize_args();
    args.sponsor = vault_one;
    let mut initialize = world.initialize_instruction_with_sponsor(args, vault_one);
    sq::replace_key(&mut initialize, world.rt.g_vault(), vault_one);
    let run = sq::run_vault_transaction(
        &mut world.rt,
        &Plan {
            tag: "squads.initialize.vault_index_one",
            multisig: g_multisig,
            vault_index: 1,
            inner: initialize,
            proposer: &world.g_members[0],
            approvers: vec![&world.g_members[0], &world.g_members[1]],
            executor: &world.g_members[0],
            outer_signers: Vec::new(),
            ephemeral_signers: 0,
            time_lock: sq::G_TIME_LOCK,
            probe_early_execute: false,
        },
    );
    assert!(
        run.approvals.iter().all(|o| o.success),
        "{}",
        sq::summarize_run(&run)
    );
    let execute = run.execute.as_ref().expect("execute attempted");
    assert!(!execute.success, "{execute:?}");
    assert_eq!(
        custom_code(execute),
        Some(code(ErrorCode::NotAuthorized)),
        "{execute:?}"
    );
    execute.assert_rollback_except_fee();
}

// ---------------------------------------------------------------------------
// ORAO registration through a real G vault transaction (devnet binary)
// ---------------------------------------------------------------------------

#[test]
fn orao_registration_through_a_real_g_vault_transaction_against_the_devnet_binary() {
    let mut world = RealSquadsWorld::with_provider(OraoProfile::Devnet);
    assert_eq!(
        world.rt.provider_artifact_sha256(),
        Some(devnet::ORAO_DEVNET_ELF_SHA256),
        "the devnet ORAO binary is loaded regardless of MC_ORAO_PROFILE"
    );
    assert_created(&mut world);
    genesis_with_g_vault_sponsor(&mut world);

    // ORAO's own network set-up (payer = network authority), with the real
    // devnet treasury address and ONE synthetic fulfil authority.
    let treasury = sq::install_devnet_orao_treasury(&mut world.rt);
    assert_eq!(treasury.to_string(), sq::DEVNET_ORAO_TREASURY);
    for setup in sq::orao_initialize(&mut world.rt, treasury) {
        assert!(setup.success, "{setup:?}");
    }

    // register: payer = the G vault (the ProgramData upgrade authority), through
    // a SECOND real G vault transaction with its own 48 h lock.
    let register = sq::orao_register_instruction(&world.rt);
    let run = world.run_g("squads.orao.register", register, &[]);
    assert!(run.succeeded(), "{}", sq::summarize_run(&run));
    assert_eq!(run.transaction_index, 2);
    let early = run.early_execute.as_ref().expect("early probe");
    assert_eq!(
        custom_code(early),
        Some(sq::idl_error_code(&sq::idl(), "TimeLockNotReleased")),
        "{early:?}"
    );

    // The packet's readback line (section 3c): Client owner = G vault, state =
    // RngAuthority, account owner = the ORAO program.
    let g_vault = world.rt.g_vault();
    let client_address = provider::client_address().0;
    assert_eq!(client_address.to_string(), sq::PACKET_ORAO_CLIENT);
    let client_account = world.rt.account(client_address);
    assert_eq!(client_account.owner, orao::id());
    let client = sq::orao_client(&world.rt);
    assert_eq!(client.owner, g_vault, "Client.owner is the G vault");
    assert_eq!(client.program, ID);
    assert_eq!(
        client.state,
        provider::rng_authority_address().0,
        "Client.state is the RngAuthority PDA"
    );
    assert!(client.callback.is_none());
    assert_eq!(client.num_requests, 0);

    // The locally created NetworkState carries the packet's fee/deadline and the real treasury address.
    let network_address = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    assert_eq!(network_address.to_string(), sq::PACKET_ORAO_NETWORK_STATE);
    let network: orao::state::network_state::NetworkState = world.rt.deserialize(network_address);
    assert_eq!(network.config.request_fee, 300_000);
    assert_eq!(network.config.callback_deadline, 9_000);
    assert_eq!(network.config.treasury, treasury);
    assert_eq!(network.num_registered, 1);
}

// ---------------------------------------------------------------------------
// The guardian path: P has no time lock
// ---------------------------------------------------------------------------

#[test]
fn guardian_pause_through_the_real_p_multisig_needs_no_time_lock() {
    let mut world = RealSquadsWorld::new();
    assert_created(&mut world);
    genesis_with_g_vault_sponsor(&mut world);
    let admission: state::Admission = world.rt.deserialize(world.rt.admission_address());
    assert!(!admission.paused);

    let pause = sq::set_pause_instruction(&world.rt, true);
    let before = sq::now(&world.rt);
    let run = world.run_p("squads.pause", pause);
    assert!(run.succeeded(), "{}", sq::summarize_run(&run));
    assert_eq!(run.transaction_index, 1);
    assert!(run.early_execute.is_none());
    assert_eq!(
        sq::now(&world.rt),
        before,
        "P's execution did not wait for any clock"
    );
    let admission: state::Admission = world.rt.deserialize(world.rt.admission_address());
    assert!(admission.paused, "the guardian vault paused admission");
    assert_eq!(admission.changed_at, before as u64);

    let unpause = sq::set_pause_instruction(&world.rt, false);
    let run = world.run_p("squads.unpause", unpause);
    assert!(run.succeeded(), "{}", sq::summarize_run(&run));
    assert_eq!(run.transaction_index, 2);
    let admission: state::Admission = world.rt.deserialize(world.rt.admission_address());
    assert!(!admission.paused);
}
