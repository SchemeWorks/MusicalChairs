//! Real ORAO Callback SBF runtime probes.
//!
//! The vendored ORAO crate supplies ABI/CPI types only. `RuntimeProbe::new`
//! loads the captured ORAO executable, so this module never uses SDK handler
//! stubs as a provider implementation.

use anchor_lang::{
    AccountDeserialize, AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
};
use bincode::{deserialize, serialize};
use musical_chairs_runtime_probe_program as probe;
use orao_solana_vrf_cb as orao;
use solana_sdk::{
    account::Account,
    ed25519_program,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    sysvar,
};

use solana_loader_v3_interface::state::UpgradeableLoaderState;
use solana_sdk_ids::bpf_loader_upgradeable;
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::runtime::{Observation, RuntimeProbe};

const REQUEST_FEE: u64 = 300_000;
const CALLBACK_DEADLINE_SLOTS: u64 = 9_000;
const APPLICATION_DEADLINE: i64 = 600;
const CLIENT_FUNDING: u64 = 10_000_000;
const COMPUTE_LIMIT: u32 = 1_400_000;
const SEED: [u8; 32] = [7; 32];

#[derive(Clone, Debug)]
struct Binding {
    state: Pubkey,
    state_bump: u8,
    client: Pubkey,
    network_state: Pubkey,
    request: Pubkey,
    request_bump: u8,
    treasury: Pubkey,
}

fn assert_failure_class(observation: &Observation, expected: &[&str]) {
    assert!(!observation.success, "expected failure: {observation:?}");
    let diagnostics = format!(
        "{} {}",
        observation.error.as_deref().unwrap_or_default(),
        observation.logs.join(" ")
    );
    assert!(
        expected.iter().any(|needle| diagnostics.contains(needle)),
        "failure did not contain an expected class {expected:?}: {observation:?}"
    );
    observation.assert_rollback_except_fee();
}

fn assert_state_unchanged(before: &probe::ProbeState, after: &probe::ProbeState) {
    assert_eq!(before.authority, after.authority);
    assert_eq!(before.client, after.client);
    assert_eq!(before.seed, after.seed);
    assert_eq!(before.deadline, after.deadline);
    assert_eq!(before.requested, after.requested);
    assert_eq!(before.captured, after.captured);
    assert_eq!(before.skipped, after.skipped);
    assert_eq!(before.randomness, after.randomness);
    assert_eq!(before.pending_cash, after.pending_cash);
    assert_eq!(before.cursor, after.cursor);
    assert_eq!(before.captured_n, after.captured_n);
    assert_eq!(before.accumulator, after.accumulator);
    assert_eq!(before.bump, after.bump);
}

/// Execute the provider lifecycle and focused rollback/expiry cases.
pub fn run_provider_suite() -> Vec<Observation> {
    let (mut runtime, binding) = setup_runtime();
    let mut observations = Vec::new();
    let initialized = initialize_provider(&mut runtime);
    assert!(
        initialized.success,
        "provider.initialize failed: {initialized:?}"
    );
    observations.push(initialized);
    let configured = configure_provider(&mut runtime, &binding);
    assert!(
        configured.success,
        "provider.configure failed: {configured:?}"
    );
    observations.push(configured);
    install_client_loader_fixture(&mut runtime);
    runtime.put_state(initial_state(&binding, runtime.authority.pubkey()));
    let registered = register_client(&mut runtime, &binding);
    assert!(
        registered.success,
        "provider.register failed: {registered:?}"
    );
    observations.push(registered);
    assert!(runtime.svm.get_account(&binding.client).is_some());
    let funded = fund_client(&mut runtime, &binding);
    assert!(funded.success, "client funding failed: {funded:?}");
    observations.push(funded);
    assert!(runtime.svm.get_balance(&binding.client).unwrap_or_default() >= CLIENT_FUNDING);
    runtime.set_clock(100, 100);

    // A request fee and request rent are charged to the registered client PDA.
    // Force the client below that balance first and retain the failed packet as
    // evidence that the client and request ticket roll back together.
    let mut underfunded_client = runtime
        .svm
        .get_account(&binding.client)
        .expect("client account");
    let funded_client = underfunded_client.clone();
    underfunded_client.lamports = 1;
    runtime
        .svm
        .set_account(binding.client, underfunded_client)
        .expect("underfunded client fixture");
    let insufficient_state = runtime.state();
    let mut insufficient_funds = request_randomness(&mut runtime, &binding, true);
    insufficient_funds.label = "provider.request.insufficient-client-funds".into();
    assert_failure_class(
        &insufficient_funds,
        &["insufficient", "InsufficientFunds", "lamport", "rent"],
    );
    assert_state_unchanged(&insufficient_state, &runtime.state());
    assert!(runtime.svm.get_account(&binding.request).is_none());
    observations.push(insufficient_funds);
    runtime
        .svm
        .set_account(binding.client, funded_client)
        .expect("restore funded client fixture");

    // Keep the request inside the probe's 600-second application window.  The
    // expiry cases below move the same synthetic clock past that deadline.
    let request_observation = request_randomness(&mut runtime, &binding, true);
    assert!(
        request_observation.success,
        "provider.request failed: {request_observation:?}"
    );
    let client_delta = request_observation
        .account_changes
        .iter()
        .find(|change| change.pubkey == binding.client.to_string())
        .map(|change| change.lamport_delta)
        .expect("request must report client lamport delta");
    let request_delta = request_observation
        .account_changes
        .iter()
        .find(|change| change.pubkey == binding.request.to_string())
        .map(|change| change.lamport_delta)
        .expect("request must report request PDA rent delta");
    assert_eq!(client_delta, -(REQUEST_FEE as i128 + request_delta));
    assert!(
        request_delta > 0,
        "request PDA must receive rent: {request_delta}"
    );
    observations.push(request_observation);
    assert!(runtime.state().requested);
    assert!(runtime.svm.get_account(&binding.request).is_some());
    let invalid_authority = Keypair::new_from_array([0x33; 32]);
    let bad_fulfillment_state = runtime.state();
    let bad_fulfillment = fulfill_randomness_with_key(
        &mut runtime,
        &binding,
        &invalid_authority,
        "provider.fulfill.invalid-ed25519-authority",
    );
    assert_failure_class(&bad_fulfillment, &["NotAuthorized", "0x1770", "6000"]);
    observations.push(bad_fulfillment);
    assert_state_unchanged(&bad_fulfillment_state, &runtime.state());
    let valid_authority = Keypair::new_from_array([0x55; 32]);
    let valid_fulfillment = fulfill_randomness_with_key(
        &mut runtime,
        &binding,
        &valid_authority,
        "provider.fulfill.ed25519-precompile",
    );
    assert!(
        valid_fulfillment.success,
        "provider.fulfill failed: {valid_fulfillment:?}"
    );
    observations.push(valid_fulfillment);
    let account = runtime
        .svm
        .get_account(&binding.request)
        .expect("fulfilled request account");
    let randomness = decode_fulfilled(&account.data).expect("request must be fulfilled");
    assert_eq!(randomness.len(), 64);
    runtime.set_clock(APPLICATION_DEADLINE, 600);
    let deadline_expiry = expire(&mut runtime, &binding, "probe.expire.at-deadline");
    assert_failure_class(&deadline_expiry, &["DeadlineNotReached"]);
    observations.push(deadline_expiry);
    let timely_capture = capture_randomness(&mut runtime, &binding);
    assert!(
        timely_capture.success,
        "probe.capture.timely failed: {timely_capture:?}"
    );
    observations.push(timely_capture);
    let captured = runtime.state();
    assert!(captured.captured);
    assert_eq!(captured.randomness, randomness);
    let duplicate_capture_state = captured.clone();
    let duplicate_capture =
        capture_randomness_labeled(&mut runtime, &binding, "probe.capture.duplicate");
    assert!(duplicate_capture.success);
    observations.push(duplicate_capture);
    assert_state_unchanged(&duplicate_capture_state, &runtime.state());

    // A captured result is terminal for expiry even after the deadline.
    runtime.set_clock(APPLICATION_DEADLINE + 1, 700);
    let duplicate_after_deadline = capture_randomness_labeled(
        &mut runtime,
        &binding,
        "probe.capture.duplicate-after-deadline",
    );
    assert!(duplicate_after_deadline.success);
    assert_state_unchanged(&captured, &runtime.state());
    observations.push(duplicate_after_deadline);
    let expire_after_capture = expire(&mut runtime, &binding, "probe.expire.after-timely-capture");
    assert_failure_class(
        &expire_after_capture,
        &["CaptureAlreadyStored", "0x1787", "6023"],
    );
    observations.push(expire_after_capture);
    runtime.set_clock(100, 100);

    let fulfilled_account = runtime
        .svm
        .get_account(&binding.request)
        .expect("fulfilled request account");
    observations.extend(run_capture_validation_suite(&mut runtime, &binding));
    runtime
        .svm
        .set_account(binding.request, fulfilled_account)
        .expect("restore fulfilled request");
    runtime.put_state(captured.clone());

    // Reset only the probe marker to expose ORAO's seed/PDA collision path.
    let mut collision = captured.clone();
    collision.requested = false;
    runtime.put_state(collision);
    let collision_state = runtime.state();
    let mut collision_observation = request_randomness(&mut runtime, &binding, true);
    collision_observation.label = "provider.request.seed-collision".into();
    assert_failure_class(
        &collision_observation,
        &["already in use", "custom program error: 0x0"],
    );
    observations.push(collision_observation);
    assert_state_unchanged(&collision_state, &runtime.state());
    runtime.put_state(captured.clone());

    // The state PDA is passed as a signer meta but no PDA signer is supplied.
    let mut unsigned = captured.clone();
    unsigned.requested = false;
    runtime.put_state(unsigned);
    let unsigned_state = runtime.state();
    let unsigned_observation = request_randomness(&mut runtime, &binding, false);
    assert_failure_class(
        &unsigned_observation,
        &["signer", "Privilege", "Cross-program invocation"],
    );
    observations.push(unsigned_observation);
    assert_state_unchanged(&unsigned_state, &runtime.state());
    runtime.put_state(captured.clone());

    let wrong_request_state = runtime.state();
    let wrong_request_observation = capture_wrong_request(&mut runtime, &binding);
    assert_failure_class(
        &wrong_request_observation,
        &["0x1781", "6017", "WrongRequestAddress"],
    );
    observations.push(wrong_request_observation);
    assert_state_unchanged(&wrong_request_state, &runtime.state());
    observations.extend(run_expiry_suite());
    observations.extend(run_provider_binding_negative_suite());
    observations.extend(run_real_late_suite());
    observations
}

fn setup_runtime() -> (RuntimeProbe, Binding) {
    setup_runtime_with_provider(true)
}

fn setup_runtime_without_provider() -> (RuntimeProbe, Binding) {
    setup_runtime_with_provider(false)
}

fn setup_runtime_with_provider(load_provider: bool) -> (RuntimeProbe, Binding) {
    let mut runtime = if load_provider {
        RuntimeProbe::new()
    } else {
        RuntimeProbe::new_without_provider()
    };
    let (state, state_bump) = Pubkey::find_program_address(&[probe::STATE_SEED], &probe::ID);
    let (client, _) = orao::state::client::Client::find_address(&probe::ID, &state, &orao::id());
    let (network_state, _) = orao::state::network_state::NetworkState::find_address(&orao::id());
    let (request, request_bump) =
        orao::state::request::RequestAccount::find_address(&client, &SEED, &orao::id());
    let treasury = Keypair::new_from_array([0x44; 32]).pubkey();
    runtime
        .svm
        .set_account(
            treasury,
            Account {
                lamports: 10_000_000,
                data: Vec::new(),
                owner: system_program::id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("synthetic treasury");
    runtime.set_clock(1_000, 1_000);
    (
        runtime,
        Binding {
            state,
            state_bump,
            client,
            network_state,
            request,
            request_bump,
            treasury,
        },
    )
}

fn initial_state(binding: &Binding, authority: Pubkey) -> probe::ProbeState {
    probe::ProbeState {
        authority,
        client: binding.client,
        seed: SEED,
        deadline: APPLICATION_DEADLINE,
        requested: false,
        captured: false,
        skipped: false,
        randomness: [0; 64],
        pending_cash: 10_000,
        cursor: 0,
        captured_n: 100,
        accumulator: 0,
        bump: binding.state_bump,
    }
}

fn anchor_ix<A: ToAccountMetas, D: InstructionData>(
    program_id: Pubkey,
    accounts: A,
    data: D,
) -> Instruction {
    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

fn initialize_provider(runtime: &mut RuntimeProbe) -> Observation {
    let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    runtime.execute(
        "provider.initialize",
        vec![anchor_ix(
            orao::id(),
            orao::accounts::Initialize {
                payer: runtime.payer.pubkey(),
                network_state,
                system_program: system_program::id(),
            },
            orao::instruction::Initialize {
                params: orao::InitializeParams::new(REQUEST_FEE),
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn configure_provider(runtime: &mut RuntimeProbe, binding: &Binding) -> Observation {
    let network_state = orao::state::network_state::NetworkState::find_address(&orao::id()).0;
    let mut config = orao::state::network_state::NetworkConfiguration::new(
        runtime.payer.pubkey(),
        binding.treasury,
        REQUEST_FEE,
    );
    config.callback_deadline = CALLBACK_DEADLINE_SLOTS;
    config.fulfill_authorities = vec![Keypair::new_from_array([0x55; 32]).pubkey()];
    runtime.execute(
        "provider.configure.synthetic-authority",
        vec![anchor_ix(
            orao::id(),
            orao::accounts::Configure {
                payer: runtime.payer.pubkey(),
                network_state,
                system_program: system_program::id(),
            },
            orao::instruction::Configure {
                params: orao::ConfigureParams::new(config),
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn install_client_loader_fixture(runtime: &mut RuntimeProbe) {
    let program_data_address =
        Pubkey::find_program_address(&[probe::ID.as_ref()], &bpf_loader_upgradeable::id()).0;
    let mut program_data = serialize(&UpgradeableLoaderState::ProgramData {
        slot: 1,
        upgrade_authority_address: Some(runtime.payer.pubkey()),
    })
    .expect("ProgramData serialization");
    let path = std::env::var_os("MC_PROBE_SBF")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../artifacts/musical_chairs_runtime_probe_program.so")
        });
    program_data.extend_from_slice(&std::fs::read(path).expect("actual probe ELF"));
    let data_account = Account {
        lamports: runtime
            .svm
            .minimum_balance_for_rent_exemption(program_data.len()),
        data: program_data,
        owner: bpf_loader_upgradeable::id(),
        executable: false,
        rent_epoch: 0,
    };
    runtime
        .svm
        .set_account(program_data_address, data_account)
        .expect("client ProgramData fixture");
    let data = serialize(&UpgradeableLoaderState::Program {
        programdata_address: program_data_address,
    })
    .expect("Program serialization");
    let program = Account {
        lamports: runtime.svm.minimum_balance_for_rent_exemption(data.len()),
        data,
        owner: bpf_loader_upgradeable::id(),
        executable: true,
        rent_epoch: 0,
    };
    runtime
        .svm
        .set_account(probe::ID, program)
        .expect("client Program fixture");
}

fn register_client(runtime: &mut RuntimeProbe, binding: &Binding) -> Observation {
    let params =
        orao::RegisterParams::new(vec![probe::STATE_SEED.to_vec(), vec![binding.state_bump]]);
    let program_data = synthetic_program_data(runtime);
    runtime.execute(
        "provider.register.no-callback",
        vec![anchor_ix(
            orao::id(),
            orao::accounts::Register {
                payer: runtime.payer.pubkey(),
                program: probe::ID,
                program_data,
                state: binding.state,
                client: binding.client,
                network_state: binding.network_state,
                system_program: system_program::id(),
            },
            orao::instruction::Register { params },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn synthetic_program_data(runtime: &RuntimeProbe) -> Pubkey {
    let account = runtime
        .svm
        .get_account(&probe::ID)
        .expect("client Program fixture");
    match deserialize(&account.data) {
        Ok(UpgradeableLoaderState::Program {
            programdata_address,
        }) => programdata_address,
        _ => panic!("client Program fixture is not UpgradeableLoader::Program"),
    }
}

fn fund_client(runtime: &mut RuntimeProbe, binding: &Binding) -> Observation {
    runtime.execute(
        "provider.client-funding",
        vec![system_instruction::transfer(
            &runtime.payer.pubkey(),
            &binding.client,
            CLIENT_FUNDING,
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn request_randomness(
    runtime: &mut RuntimeProbe,
    binding: &Binding,
    sign_state: bool,
) -> Observation {
    let label = if sign_state {
        "provider.request.cpi"
    } else {
        "provider.request.missing-pda-signer"
    };
    runtime.execute(
        label,
        vec![anchor_ix(
            probe::ID,
            probe::accounts::ProbeRequest {
                state: binding.state,
                payer: runtime.payer.pubkey(),
                client: binding.client,
                network_state: binding.network_state,
                treasury: binding.treasury,
                request: binding.request,
                system_program: system_program::id(),
                provider_program: orao::id(),
            },
            probe::instruction::ProbeRequest {
                version: probe::WIRE_VERSION,
                sign_state,
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn fulfill_randomness_with_key(
    runtime: &mut RuntimeProbe,
    binding: &Binding,
    authority: &Keypair,
    label: &'static str,
) -> Observation {
    let mut message = Vec::with_capacity(64);
    message.extend_from_slice(binding.client.as_ref());
    message.extend_from_slice(&SEED);
    let signature = authority.sign_message(&message);
    let signature_bytes: &[u8; 64] = signature
        .as_ref()
        .try_into()
        .expect("Solana signature must be 64 bytes");
    let ed25519 = ed25519_instruction(&authority.pubkey(), signature_bytes, &message);
    let fulfill = anchor_ix(
        orao::id(),
        orao::accounts::Fulfill {
            payer: runtime.payer.pubkey(),
            program: probe::ID,
            state: binding.state,
            client: binding.client,
            request: binding.request,
            network_state: binding.network_state,
            instruction_acc: sysvar::instructions::id(),
        },
        orao::instruction::Fulfill {
            params: orao::FulfillParams::new(),
        },
    );
    runtime.execute(label, vec![ed25519, fulfill], &[], COMPUTE_LIMIT)
}

fn ed25519_instruction(pubkey: &Pubkey, signature: &[u8; 64], message: &[u8]) -> Instruction {
    // One signature whose pubkey, signature, and message all live in this
    // instruction.  The offsets are the stable Solana Ed25519 precompile ABI.
    let start = 16usize;
    let pubkey_offset = start as u16;
    let signature_offset = (start + 32) as u16;
    let message_offset = (start + 32 + 64) as u16;
    let mut data = Vec::with_capacity(start + 32 + 64 + message.len());
    data.extend_from_slice(&[1, 0]);
    data.extend_from_slice(&signature_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&message_offset.to_le_bytes());
    data.extend_from_slice(&(message.len() as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(pubkey.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction {
        program_id: ed25519_program::id(),
        accounts: Vec::new(),
        data,
    }
}

fn capture_randomness(runtime: &mut RuntimeProbe, binding: &Binding) -> Observation {
    capture_randomness_labeled(runtime, binding, "probe.capture.timely")
}

fn capture_randomness_labeled(
    runtime: &mut RuntimeProbe,
    binding: &Binding,
    label: &'static str,
) -> Observation {
    runtime.execute(
        label,
        vec![anchor_ix(
            probe::ID,
            probe::accounts::ProbeCapture {
                state: binding.state,
                request: binding.request,
            },
            probe::instruction::ProbeCapture {
                version: probe::WIRE_VERSION,
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn capture_wrong_request(runtime: &mut RuntimeProbe, binding: &Binding) -> Observation {
    let wrong_request = Pubkey::new_unique();
    runtime
        .svm
        .set_account(
            wrong_request,
            Account {
                lamports: 1,
                data: Vec::new(),
                owner: system_program::id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("wrong request address fixture");
    runtime.execute(
        "probe.capture.wrong-request-address",
        vec![anchor_ix(
            probe::ID,
            probe::accounts::ProbeCapture {
                state: binding.state,
                request: wrong_request,
            },
            probe::instruction::ProbeCapture {
                version: probe::WIRE_VERSION,
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn run_capture_validation_suite(runtime: &mut RuntimeProbe, binding: &Binding) -> Vec<Observation> {
    let request_bump = binding.request_bump;
    let pending = orao::state::request::RequestAccount::new(
        request_bump,
        100,
        binding.client,
        SEED,
        orao::state::request::RequestState::new(None, false),
    );
    let wrong_client = orao::state::request::RequestAccount::new(
        request_bump,
        100,
        Pubkey::new_unique(),
        SEED,
        orao::state::request::RequestState::Fulfilled(orao::state::request::Fulfilled::new(
            [4; 64], None,
        )),
    );
    let wrong_seed = orao::state::request::RequestAccount::new(
        request_bump,
        100,
        binding.client,
        [8; 32],
        orao::state::request::RequestState::Fulfilled(orao::state::request::Fulfilled::new(
            [5; 64], None,
        )),
    );
    let wrong_bump = orao::state::request::RequestAccount::new(
        request_bump.wrapping_add(1),
        100,
        binding.client,
        SEED,
        orao::state::request::RequestState::Fulfilled(orao::state::request::Fulfilled::new(
            [6; 64], None,
        )),
    );

    vec![
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.pending-request",
            request_account_fixture(&pending, orao::id()),
            &["0x1784", "6020", "RequestNotFulfilled"],
        ),
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.wrong-owner",
            request_account_fixture(&wrong_client, system_program::id()),
            &["0x1780", "6016", "WrongRequestOwner"],
        ),
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.wrong-client",
            request_account_fixture(&wrong_client, orao::id()),
            &["0x1782", "6018", "WrongRequestBinding"],
        ),
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.wrong-seed",
            request_account_fixture(&wrong_seed, orao::id()),
            &["0x1782", "6018", "WrongRequestBinding"],
        ),
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.wrong-bump",
            request_account_fixture(&wrong_bump, orao::id()),
            &["0x1782", "6018", "WrongRequestBinding"],
        ),
        capture_fixture_case(
            runtime,
            binding,
            "probe.capture.malformed-request",
            Account {
                lamports: 1_000_000,
                data: vec![0; 8],
                owner: orao::id(),
                executable: false,
                rent_epoch: 0,
            },
            &["0x1783", "6019", "MalformedRequest"],
        ),
    ]
}

fn capture_fixture_case(
    runtime: &mut RuntimeProbe,
    binding: &Binding,
    label: &'static str,
    account: Account,
    expected: &[&str],
) -> Observation {
    let mut state = runtime.state();
    state.requested = true;
    state.captured = false;
    state.skipped = false;
    state.randomness = [0; 64];
    runtime.put_state(state);
    let before_capture = runtime.state();
    runtime
        .svm
        .set_account(binding.request, account)
        .expect("request validation fixture");
    let observation = capture_randomness_labeled(runtime, binding, label);
    assert_failure_class(&observation, expected);
    assert_state_unchanged(&before_capture, &runtime.state());
    observation
}

fn request_account_fixture(
    request: &orao::state::request::RequestAccount,
    owner: Pubkey,
) -> Account {
    let mut data = orao::state::request::RequestAccount::DISCRIMINATOR.to_vec();
    request
        .serialize(&mut data)
        .expect("request fixture serialization");
    Account {
        lamports: 1_000_000,
        data,
        owner,
        executable: false,
        rent_epoch: 0,
    }
}

fn run_expiry_suite() -> Vec<Observation> {
    let (mut runtime, binding) = setup_runtime_without_provider();
    runtime.put_state(initial_state(&binding, runtime.authority.pubkey()));
    runtime.set_clock(100, 100);
    let early_expiry = expire(&mut runtime, &binding, "probe.expire.before-deadline");
    assert_failure_class(&early_expiry, &["0x1786", "6022", "DeadlineNotReached"]);
    assert!(!runtime.state().requested);
    runtime.set_clock(APPLICATION_DEADLINE + 1_000, 2_000);
    let no_request_before = runtime.state();
    let no_request_expiry = expire(
        &mut runtime,
        &binding,
        "probe.expire.provider-absent-no-request",
    );
    assert!(no_request_expiry.success);
    assert!(runtime.state().skipped);
    assert!(!runtime.state().requested);
    assert_eq!(runtime.state().pending_cash, no_request_before.pending_cash);
    runtime.put_state(initial_state(&binding, runtime.authority.pubkey()));
    let mut state = runtime.state();
    state.requested = true;
    runtime.put_state(state);
    // This is deliberately an account-shaped late-result fixture. It tests
    // the authenticated late-result no-op while the request is still
    // unskipped; it is not provider fulfillment evidence.
    let fulfilled = orao::state::request::RequestAccount::new(
        binding.request_bump,
        1_000,
        binding.client,
        SEED,
        orao::state::request::RequestState::Fulfilled(orao::state::request::Fulfilled::new(
            [3; 64], None,
        )),
    );
    runtime
        .svm
        .set_account(
            binding.request,
            request_account_fixture(&fulfilled, orao::id()),
        )
        .expect("late fulfilled account fixture");
    let pending_cash_before_expiry = runtime.state().pending_cash;
    let randomness_before_late = runtime.state().randomness;
    let state_before_late = runtime.state();
    let late_capture =
        capture_randomness_labeled(&mut runtime, &binding, "probe.capture.late-fixture-noop");
    assert!(
        late_capture.success,
        "late capture should be an authenticated no-op: {late_capture:?}"
    );
    let mut observations = vec![early_expiry, no_request_expiry, late_capture];
    assert!(!runtime.state().captured);
    assert!(!runtime.state().skipped);
    assert_eq!(runtime.state().randomness, randomness_before_late);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);
    assert_state_unchanged(&state_before_late, &runtime.state());

    let state_before_expiry = runtime.state();
    let expiry = expire(&mut runtime, &binding, "probe.expire.deadline");
    assert!(
        expiry.success,
        "expiry should skip the unfulfilled request: {expiry:?}"
    );
    observations.push(expiry);
    assert!(runtime.state().skipped);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);
    assert_eq!(
        runtime.state().pending_cash,
        state_before_expiry.pending_cash
    );
    assert_eq!(runtime.state().randomness, state_before_expiry.randomness);
    assert_eq!(runtime.state().cursor, state_before_expiry.cursor);
    assert_eq!(runtime.state().accumulator, state_before_expiry.accumulator);
    let before_duplicate = runtime.state();
    let duplicate_expiry = expire(&mut runtime, &binding, "probe.expire.duplicate");
    assert!(
        duplicate_expiry.success,
        "duplicate expiry should be idempotent: {duplicate_expiry:?}"
    );
    observations.push(duplicate_expiry);
    assert_state_unchanged(&before_duplicate, &runtime.state());
    assert_eq!(runtime.state().skipped, before_duplicate.skipped);

    // The same authenticated late result remains a no-op after expiry.
    let before_skipped_capture = runtime.state();
    let skipped_late_capture =
        capture_randomness_labeled(&mut runtime, &binding, "probe.capture.skipped-fixture-noop");
    assert!(
        skipped_late_capture.success,
        "capture after skip should be a no-op: {skipped_late_capture:?}"
    );
    observations.push(skipped_late_capture);
    assert!(!runtime.state().captured);
    assert_eq!(runtime.state().randomness, randomness_before_late);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);
    assert_state_unchanged(&before_skipped_capture, &runtime.state());
    observations
}

fn expire(runtime: &mut RuntimeProbe, binding: &Binding, label: &'static str) -> Observation {
    runtime.execute(
        label,
        vec![anchor_ix(
            probe::ID,
            probe::accounts::ProbeExpire {
                state: binding.state,
            },
            probe::instruction::ProbeExpire {
                version: probe::WIRE_VERSION,
            },
        )],
        &[],
        COMPUTE_LIMIT,
    )
}

fn decode_fulfilled(data: &[u8]) -> Option<[u8; 64]> {
    let mut bytes = data;
    let request = orao::state::request::RequestAccount::try_deserialize(&mut bytes).ok()?;
    match request.state {
        orao::state::request::RequestState::Fulfilled(result) => Some(result.randomness),
        _ => None,
    }
}

/// Run a second, independent real-provider lifecycle where the actual ORAO
/// request is fulfilled after the probe deadline. The provider still runs
/// locally from the captured SBF, while the probe must preserve its skipped
/// state and ignore the late result.
fn run_real_late_suite() -> Vec<Observation> {
    let (mut runtime, binding) = setup_runtime();
    let mut observations = Vec::new();
    let initialized = initialize_provider(&mut runtime);
    assert!(
        initialized.success,
        "provider.real-late.initialize failed: {initialized:?}"
    );
    observations.push(initialized);
    let configured = configure_provider(&mut runtime, &binding);
    assert!(
        configured.success,
        "provider.real-late.configure failed: {configured:?}"
    );
    observations.push(configured);
    install_client_loader_fixture(&mut runtime);
    runtime.put_state(initial_state(&binding, runtime.authority.pubkey()));
    let registered = register_client(&mut runtime, &binding);
    assert!(
        registered.success,
        "provider.real-late.register failed: {registered:?}"
    );
    observations.push(registered);
    let funded = fund_client(&mut runtime, &binding);
    assert!(
        funded.success,
        "provider.real-late.client-funding failed: {funded:?}"
    );
    observations.push(funded);

    let request = |runtime: &mut RuntimeProbe, label: &'static str| {
        runtime.execute(
            label,
            vec![anchor_ix(
                probe::ID,
                probe::accounts::ProbeRequest {
                    state: binding.state,
                    payer: runtime.payer.pubkey(),
                    client: binding.client,
                    network_state: binding.network_state,
                    treasury: binding.treasury,
                    request: binding.request,
                    system_program: system_program::id(),
                    provider_program: orao::id(),
                },
                probe::instruction::ProbeRequest {
                    version: probe::WIRE_VERSION,
                    sign_state: true,
                },
            )],
            &[],
            COMPUTE_LIMIT,
        )
    };

    runtime.set_clock(APPLICATION_DEADLINE + 1, 601);
    let before_deadline_request = runtime.state();
    let after_deadline_request = request(&mut runtime, "provider.real-late.request-after-deadline");
    assert_failure_class(
        &after_deadline_request,
        &["RequestDeadlinePassed", "0x177e", "6014"],
    );
    assert_state_unchanged(&before_deadline_request, &runtime.state());
    assert!(runtime.svm.get_account(&binding.request).is_none());
    for account in [binding.client, binding.treasury, binding.request] {
        assert!(
            !after_deadline_request
                .account_changes
                .iter()
                .any(|change| change.pubkey == account.to_string()),
            "deadline rejection changed {account}: {after_deadline_request:?}"
        );
    }
    observations.push(after_deadline_request);

    runtime.set_clock(100, 100);
    let requested = request(&mut runtime, "provider.real-late.request");
    assert!(
        requested.success,
        "provider.real-late.request failed: {requested:?}"
    );
    observations.push(requested);
    assert!(runtime.state().requested);
    let pending_cash_before_expiry = runtime.state().pending_cash;
    let randomness_before_expiry = runtime.state().randomness;

    runtime.set_clock(APPLICATION_DEADLINE + 1, 601);
    let expired = expire(
        &mut runtime,
        &binding,
        "provider.real-late.expire-before-fulfill",
    );
    assert!(
        expired.success,
        "provider.real-late.expire-before-fulfill failed: {expired:?}"
    );
    observations.push(expired);
    assert!(runtime.state().skipped);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);
    assert_eq!(runtime.state().randomness, randomness_before_expiry);

    let authority = Keypair::new_from_array([0x55; 32]);
    let fulfilled = fulfill_randomness_with_key(
        &mut runtime,
        &binding,
        &authority,
        "provider.real-late.fulfill-after-expiry",
    );
    assert!(
        fulfilled.success,
        "provider.real-late.fulfill-after-expiry failed: {fulfilled:?}"
    );
    observations.push(fulfilled);
    let request_account = runtime
        .svm
        .get_account(&binding.request)
        .expect("real late request account");
    let real_randomness = decode_fulfilled(&request_account.data)
        .expect("real late ORAO request must contain 64-byte fulfillment");
    assert_eq!(real_randomness.len(), 64);

    let before_late_capture = runtime.state();
    let late_capture = capture_randomness_labeled(
        &mut runtime,
        &binding,
        "provider.real-late.capture-after-expiry",
    );
    assert!(
        late_capture.success,
        "provider.real-late.capture-after-expiry failed: {late_capture:?}"
    );
    observations.push(late_capture);
    assert_state_unchanged(&before_late_capture, &runtime.state());
    assert!(runtime.state().skipped);
    assert!(!runtime.state().captured);
    assert_eq!(runtime.state().randomness, randomness_before_expiry);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);

    runtime.set_clock(100, 100);
    let before_backdated_capture = runtime.state();
    let backdated_capture = capture_randomness_labeled(
        &mut runtime,
        &binding,
        "provider.real-late.capture-backdated-after-skip",
    );
    assert!(
        backdated_capture.success,
        "provider.real-late.capture-backdated-after-skip failed: {backdated_capture:?}"
    );
    observations.push(backdated_capture);
    assert_state_unchanged(&before_backdated_capture, &runtime.state());
    assert!(!runtime.state().captured);
    assert_eq!(runtime.state().randomness, randomness_before_expiry);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);

    runtime.set_clock(APPLICATION_DEADLINE + 1, 601);
    let before_retry = runtime.state();
    let retry = request(
        &mut runtime,
        "provider.real-late.request-idempotent-after-expiry",
    );
    assert!(
        retry.success,
        "provider.real-late.request-idempotent-after-expiry failed: {retry:?}"
    );
    retry.assert_rollback_except_fee();
    observations.push(retry);
    assert_state_unchanged(&before_retry, &runtime.state());
    assert!(runtime.state().requested);
    assert!(runtime.state().skipped);
    assert!(!runtime.state().captured);
    assert_eq!(runtime.state().randomness, randomness_before_expiry);
    assert_eq!(runtime.state().pending_cash, pending_cash_before_expiry);

    for observation in &mut observations {
        if observation.label.starts_with("provider.")
            && !observation.label.starts_with("provider.real-late.")
        {
            observation.label = observation
                .label
                .replacen("provider.", "provider.real-late.", 1);
        }
    }
    observations
}

/// Exercise provider account binding failures with typed account fixtures so
/// each case reaches the probe's validation rather than failing on a missing
/// or unparseable account first.
fn run_provider_binding_negative_suite() -> Vec<Observation> {
    let (mut runtime, binding) = setup_runtime();
    let mut observations = Vec::new();
    let initialized = initialize_provider(&mut runtime);
    assert!(
        initialized.success,
        "provider.binding.initialize failed: {initialized:?}"
    );
    observations.push(initialized);
    let configured = configure_provider(&mut runtime, &binding);
    assert!(
        configured.success,
        "provider.binding.configure failed: {configured:?}"
    );
    observations.push(configured);
    install_client_loader_fixture(&mut runtime);
    runtime.put_state(initial_state(&binding, runtime.authority.pubkey()));
    let registered = register_client(&mut runtime, &binding);
    assert!(
        registered.success,
        "provider.binding.register failed: {registered:?}"
    );
    observations.push(registered);
    let funded = fund_client(&mut runtime, &binding);
    assert!(
        funded.success,
        "provider.binding.client-funding failed: {funded:?}"
    );
    observations.push(funded);
    runtime.set_clock(100, 100);

    let invoke = |runtime: &mut RuntimeProbe,
                  label: &'static str,
                  client: Pubkey,
                  network_state: Pubkey,
                  treasury: Pubkey,
                  request: Pubkey,
                  provider_program: Pubkey| {
        runtime.execute(
            label,
            vec![anchor_ix(
                probe::ID,
                probe::accounts::ProbeRequest {
                    state: binding.state,
                    payer: runtime.payer.pubkey(),
                    client,
                    network_state,
                    treasury,
                    request,
                    system_program: system_program::id(),
                    provider_program,
                },
                probe::instruction::ProbeRequest {
                    version: probe::WIRE_VERSION,
                    sign_state: true,
                },
            )],
            &[],
            COMPUTE_LIMIT,
        )
    };

    let initial = runtime.state();
    let wrong_client = Pubkey::new_unique();
    let client_account = runtime
        .svm
        .get_account(&binding.client)
        .expect("genuine Client account fixture");
    runtime
        .svm
        .set_account(wrong_client, client_account)
        .expect("typed wrong-client fixture");
    let wrong_client_observation = invoke(
        &mut runtime,
        "provider.binding.wrong-client",
        wrong_client,
        binding.network_state,
        binding.treasury,
        binding.request,
        orao::id(),
    );
    assert_failure_class(
        &wrong_client_observation,
        &["ProviderBindingMismatch", "0x177d", "6013"],
    );
    observations.push(wrong_client_observation);
    assert_state_unchanged(&initial, &runtime.state());

    let wrong_network = Pubkey::new_unique();
    let network_account = runtime
        .svm
        .get_account(&binding.network_state)
        .expect("genuine NetworkState account fixture");
    runtime
        .svm
        .set_account(wrong_network, network_account)
        .expect("typed wrong-network fixture");
    let wrong_network_observation = invoke(
        &mut runtime,
        "provider.binding.wrong-network",
        binding.client,
        wrong_network,
        binding.treasury,
        binding.request,
        orao::id(),
    );
    assert_failure_class(
        &wrong_network_observation,
        &["ProviderBindingMismatch", "0x177d", "6013"],
    );
    observations.push(wrong_network_observation);
    assert_state_unchanged(&initial, &runtime.state());

    let wrong_treasury = Pubkey::new_unique();
    runtime
        .svm
        .set_account(
            wrong_treasury,
            Account {
                lamports: 1,
                data: Vec::new(),
                owner: system_program::id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("system wrong-treasury fixture");
    let wrong_treasury_observation = invoke(
        &mut runtime,
        "provider.binding.wrong-treasury",
        binding.client,
        binding.network_state,
        wrong_treasury,
        binding.request,
        orao::id(),
    );
    assert_failure_class(
        &wrong_treasury_observation,
        &["ProviderBindingMismatch", "0x177d", "6013"],
    );
    observations.push(wrong_treasury_observation);
    assert_state_unchanged(&initial, &runtime.state());

    let wrong_request = Pubkey::new_unique();
    runtime
        .svm
        .set_account(
            wrong_request,
            Account {
                lamports: 1,
                data: Vec::new(),
                owner: system_program::id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("system wrong-request fixture");
    let wrong_request_observation = invoke(
        &mut runtime,
        "provider.binding.wrong-request",
        binding.client,
        binding.network_state,
        binding.treasury,
        wrong_request,
        orao::id(),
    );
    assert_failure_class(
        &wrong_request_observation,
        &["ProviderBindingMismatch", "0x177d", "6013"],
    );
    observations.push(wrong_request_observation);
    assert_state_unchanged(&initial, &runtime.state());

    let wrong_provider_observation = invoke(
        &mut runtime,
        "provider.binding.wrong-provider-program",
        binding.client,
        binding.network_state,
        binding.treasury,
        binding.request,
        system_program::id(),
    );
    assert_failure_class(
        &wrong_provider_observation,
        &["InvalidProgramId", "0xbb8", "3008"],
    );
    observations.push(wrong_provider_observation);
    assert_state_unchanged(&initial, &runtime.state());

    let mut requested_state = initial.clone();
    requested_state.requested = true;
    runtime.put_state(requested_state.clone());
    runtime
        .svm
        .set_account(
            binding.request,
            Account {
                lamports: 1_000_000,
                data: Vec::new(),
                owner: orao::id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("empty ORAO-owned request fixture");
    let malformed = invoke(
        &mut runtime,
        "provider.binding.malformed-empty-idempotent-request",
        binding.client,
        binding.network_state,
        binding.treasury,
        binding.request,
        orao::id(),
    );
    assert_failure_class(&malformed, &["MalformedRequest", "0x1783", "6019"]);
    assert_state_unchanged(&requested_state, &runtime.state());
    assert!(
        !malformed
            .logs
            .iter()
            .any(|log| log.contains(&format!("Program {} invoke", orao::id()))),
        "malformed idempotent request must fail before ORAO CPI: {malformed:?}"
    );
    observations.push(malformed);

    for observation in &mut observations {
        if observation.label.starts_with("provider.")
            && !observation.label.starts_with("provider.binding.")
        {
            observation.label = observation
                .label
                .replacen("provider.", "provider.binding.", 1);
        }
    }
    observations
}
