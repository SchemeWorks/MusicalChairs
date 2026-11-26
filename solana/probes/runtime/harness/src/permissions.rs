//! Permission, PDA, and chunk-atomicity probes against the compiled program.
//!
//! Every fixture in this module is synthetic.  The payer and authority are
//! separate keypairs, and no wallet, deployment, or production account is
//! consulted.  The assertions intentionally inspect the runtime's custom
//! error code and fee-only rollback so that a host-side account-model result
//! cannot be mistaken for SVM execution evidence.

use anchor_lang::{AccountSerialize, Discriminator, InstructionData, ToAccountMetas};
use musical_chairs_runtime_probe_program as probe;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

use solana_system_interface::program as system_program;

use crate::runtime::{Observation, RuntimeProbe};

const COMPUTE_LIMIT: u32 = 1_400_000;
const ACCOUNT_LAMPORTS: u64 = 1_000_000;

/// Execute the local permission and bounded-row transaction matrix.
pub fn run_permission_suite() -> Vec<Observation> {
    vec![
        authorized_write(),
        missing_authority_signature(),
        wrong_authority(),
        readonly_state(),
        wrong_state_owner(),
        wrong_state_discriminator(),
        bad_stored_bump(),
        wrong_state_pda(),
        unsupported_version(),
        chunk_positive(),
        chunk_readonly_row(),
        chunk_wrong_owner(),
        chunk_wrong_index(),
        chunk_wrong_discriminator(),
        chunk_aliased_row(),
        chunk_malformed_later_row(),
        chunk_stale_cursor(),
        chunk_arithmetic_overflow(),
        chunk_beyond_captured_n(),
        chunk_trailing_row_bytes(),
        chunk_state_alias(),
    ]
}

fn authorized_write() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let observation = runtime.execute(
        "permission.authorized-write",
        vec![authorized_ix(
            state,
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert!(
        observation.success,
        "authorized write failed: {observation:?}"
    );
    assert_eq!(runtime.state().accumulator, 77);
    assert_eq!(runtime.state().authority, runtime.authority.pubkey());
    observation
}

fn missing_authority_signature() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority_pubkey = runtime.authority.pubkey();
    let mut instruction = authorized_ix(state, authority_pubkey, probe::WIRE_VERSION, 77);
    // Keep the authority account in the instruction but remove its signer
    // bit. Anchor then rejects the account with AccountNotSigner (3010),
    // giving this case a real program-level permission observation without
    // asking Transaction::sign to fabricate a missing signature.
    instruction.accounts[1].is_signer = false;
    let observation = runtime.execute(
        "permission.missing-authority-signature",
        vec![instruction],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 3010);
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn wrong_authority() -> Observation {
    let mut runtime = permission_runtime();
    let wrong = Keypair::new_from_array([0x33; 32]);
    let state = state_address().0;
    let observation = runtime.execute(
        "permission.wrong-authority",
        vec![authorized_ix(
            state,
            wrong.pubkey(),
            probe::WIRE_VERSION,
            77,
        )],
        &[&wrong],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 2001);
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn readonly_state() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let observation = runtime.execute(
        "permission.readonly-state",
        vec![authorized_ix_with_state_meta(
            AccountMeta::new_readonly(state, false),
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 2000);
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn wrong_state_owner() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let data = account_data(&runtime.state());
    install_account(&mut runtime, state, data, system_program::id());
    let observation = runtime.execute(
        "permission.wrong-state-owner",
        vec![authorized_ix(
            state,
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 3007);
    observation
}

fn wrong_state_discriminator() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let mut data = account_data(&runtime.state());
    data[..8].copy_from_slice(&[0xA5; 8]);
    install_account(&mut runtime, state, data, probe::ID);
    let observation = runtime.execute(
        "permission.wrong-state-discriminator",
        vec![authorized_ix(
            state,
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    // Anchor's Account loader reports a discriminator mismatch as its custom
    // account-error range (3000+), rather than ProbeError::MalformedRow.
    assert_failure_code(&observation, 3002);
    observation
}

fn bad_stored_bump() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let mut fixture = runtime.state();
    fixture.bump = fixture.bump.wrapping_add(1);
    runtime.put_state(fixture);
    let observation = runtime.execute(
        "permission.bad-stored-bump",
        vec![authorized_ix(
            state,
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 2006);
    observation
}

fn wrong_state_pda() -> Observation {
    let mut runtime = permission_runtime();
    let wrong_state = Pubkey::new_from_array([0x44; 32]);
    let data = account_data(&runtime.state());
    install_account_with_lamports(&mut runtime, wrong_state, data, probe::ID, 10_000_000);
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let observation = runtime.execute(
        "permission.wrong-state-pda",
        vec![authorized_ix(
            wrong_state,
            authority_pubkey,
            probe::WIRE_VERSION,
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 2006);
    observation
}

fn unsupported_version() -> Observation {
    let mut runtime = permission_runtime();
    let state = state_address().0;
    let authority = runtime.authority.insecure_clone();
    let authority_pubkey = authority.pubkey();
    let observation = runtime.execute(
        "permission.unsupported-version",
        vec![authorized_ix(
            state,
            authority_pubkey,
            probe::WIRE_VERSION.wrapping_add(1),
            77,
        )],
        &[&authority],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6000);
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn chunk_positive() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row0 = runtime.put_row(0, u128::MAX, 0);
    let row1 = runtime.put_row(1, u128::MAX, 0);
    let state = state_address().0;
    let observation = runtime.execute(
        "chunk.positive-two-rows",
        vec![chunk_ix(
            state,
            &[AccountMeta::new(row0, false), AccountMeta::new(row1, false)],
            0,
            2,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert!(
        observation.success,
        "positive chunk failed: {observation:?}"
    );
    assert!(observation.changed_data_bytes > 0);
    assert_eq!(runtime.state().cursor, 2);
    assert_eq!(runtime.state().accumulator, 1);
    assert_eq!(runtime.row(0).pending, 0);
    assert_eq!(runtime.row(1).pending, 1);
    assert_wire_bytes(&runtime, state, row0, row1);
    observation
}

fn chunk_readonly_row() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let observation = runtime.execute(
        "chunk.readonly-row",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new_readonly(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6006);
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.row(0).pending, 0);
    observation
}

fn chunk_wrong_owner() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let account = runtime
        .svm
        .get_account(&row)
        .expect("row fixture must exist");
    install_account(&mut runtime, row, account.data, system_program::id());
    let observation = runtime.execute(
        "chunk.wrong-row-owner",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6007);
    assert_eq!(runtime.state().cursor, 0);
    observation
}

fn chunk_wrong_index() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let (_, expected_bump) = Pubkey::find_program_address(
        &[
            probe::BACKER_SEED,
            state_address().0.as_ref(),
            &0u16.to_le_bytes(),
        ],
        &probe::ID,
    );
    let wrong = probe::ProbeBacker {
        index: 1,
        headroom: u128::MAX,
        pending: 0,
        bump: expected_bump,
    };
    install_account(&mut runtime, row, account_data(&wrong), probe::ID);
    let observation = runtime.execute(
        "chunk.wrong-row-index",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6010);
    assert_eq!(runtime.state().cursor, 0);
    observation
}

fn chunk_wrong_discriminator() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let mut data = account_data(&runtime.row(0));
    data[..8].copy_from_slice(&[0x5A; 8]);
    install_account(&mut runtime, row, data, probe::ID);
    let observation = runtime.execute(
        "chunk.wrong-row-discriminator",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6009);
    assert_eq!(runtime.state().cursor, 0);
    observation
}

fn chunk_aliased_row() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let observation = runtime.execute(
        "chunk.aliased-row",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false), AccountMeta::new(row, false)],
            0,
            2,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6005);
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.row(0).pending, 0);
    observation
}

fn chunk_malformed_later_row() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row0 = runtime.put_row(0, u128::MAX, 0);
    let row1 = runtime.put_row(1, u128::MAX, 0);
    let mut malformed = account_data(&runtime.row(1));
    malformed[..8].copy_from_slice(&[0xC3; 8]);
    install_account(&mut runtime, row1, malformed, probe::ID);
    let observation = runtime.execute(
        "chunk.malformed-later-row",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row0, false), AccountMeta::new(row1, false)],
            0,
            2,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6009);
    // The program validates all rows before borrowing any row mutably.  This
    // is then independently enforced by SVM transaction rollback.
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.row(0).pending, 0);
    observation
}

fn chunk_stale_cursor() -> Observation {
    let mut runtime = chunk_runtime(1, 100, 10_000, 0);
    let row = runtime.put_row(1, u128::MAX, 0);
    let observation = runtime.execute(
        "chunk.stale-cursor",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6003);
    assert_eq!(runtime.state().cursor, 1);
    assert_eq!(runtime.row(1).pending, 0);
    observation
}

fn chunk_arithmetic_overflow() -> Observation {
    let mut runtime = chunk_runtime(1, 100, 10_000, u128::MAX);
    let row = runtime.put_row(1, u128::MAX, 0);
    let observation = runtime.execute(
        "chunk.arithmetic-overflow-after-row-write",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            1,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6011);
    assert_eq!(runtime.state().cursor, 1);
    assert_eq!(runtime.state().accumulator, u128::MAX);
    assert_eq!(runtime.row(1).pending, 0);
    observation
}

fn chunk_beyond_captured_n() -> Observation {
    let mut runtime = chunk_runtime(0, 1, 10_000, 0);
    let row0 = runtime.put_row(0, u128::MAX, 0);
    let row1 = runtime.put_row(1, u128::MAX, 0);
    let observation = runtime.execute(
        "chunk.beyond-captured-n",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row0, false), AccountMeta::new(row1, false)],
            0,
            2,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6002);
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.state().accumulator, 0);
    assert_eq!(runtime.row(0).pending, 0);
    assert_eq!(runtime.row(1).pending, 0);
    observation
}

fn chunk_trailing_row_bytes() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let row = runtime.put_row(0, u128::MAX, 0);
    let mut data = account_data(&runtime.row(0));
    data.extend_from_slice(&[0xEE, 0xEE]);
    install_account(&mut runtime, row, data, probe::ID);
    let observation = runtime.execute(
        "chunk.trailing-row-bytes",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(row, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6009);
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.row(0).pending, 0);
    observation
}

fn chunk_state_alias() -> Observation {
    let mut runtime = chunk_runtime(0, 100, 10_000, 0);
    let observation = runtime.execute(
        "chunk.state-alias",
        vec![chunk_ix(
            state_address().0,
            &[AccountMeta::new(state_address().0, false)],
            0,
            1,
        )],
        &[],
        COMPUTE_LIMIT,
    );
    assert_failure_code(&observation, 6005);
    assert_eq!(runtime.state().cursor, 0);
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn permission_runtime() -> RuntimeProbe {
    let mut runtime = RuntimeProbe::new_without_provider();
    let state = default_state(&runtime);
    runtime.put_state(state);
    runtime
}

fn chunk_runtime(
    cursor: u16,
    captured_n: u16,
    pending_cash: u64,
    accumulator: u128,
) -> RuntimeProbe {
    let mut runtime = permission_runtime();
    let mut state = runtime.state();
    state.cursor = cursor;
    state.captured_n = captured_n;
    state.pending_cash = pending_cash;
    state.accumulator = accumulator;
    runtime.put_state(state);
    runtime
}

fn default_state(runtime: &RuntimeProbe) -> probe::ProbeState {
    let (state, bump) = state_address();
    let (client, _) = orao_solana_vrf_cb::state::client::Client::find_address(
        &probe::ID,
        &state,
        &orao_solana_vrf_cb::id(),
    );
    probe::ProbeState {
        authority: runtime.authority.pubkey(),
        client,
        seed: [7; 32],
        deadline: 600,
        requested: false,
        captured: false,
        skipped: false,
        randomness: [0; 64],
        pending_cash: 10_000,
        cursor: 0,
        captured_n: 100,
        accumulator: 0,
        bump,
    }
}

fn state_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[probe::STATE_SEED], &probe::ID)
}

fn authorized_ix(state: Pubkey, authority: Pubkey, version: u8, value: u128) -> Instruction {
    authorized_ix_with_state_meta(AccountMeta::new(state, false), authority, version, value)
}

fn authorized_ix_with_state_meta(
    state_meta: AccountMeta,
    authority: Pubkey,
    version: u8,
    value: u128,
) -> Instruction {
    let accounts = probe::accounts::ProbeAuthorizedWrite {
        state: state_meta.pubkey,
        authority,
    }
    .to_account_metas(None)
    .into_iter()
    .enumerate()
    .map(|(index, mut meta)| {
        if index == 0 {
            meta = state_meta.clone();
        }
        meta
    })
    .collect();
    Instruction {
        program_id: probe::ID,
        accounts,
        data: probe::instruction::ProbeAuthorizedWrite { version, value }.data(),
    }
}

fn chunk_ix(state: Pubkey, rows: &[AccountMeta], expected_cursor: u16, count: u8) -> Instruction {
    let mut accounts = probe::accounts::ProbeChunk { state }.to_account_metas(None);
    accounts.extend_from_slice(rows);
    Instruction {
        program_id: probe::ID,
        accounts,
        data: probe::instruction::ProbeChunk {
            version: probe::WIRE_VERSION,
            expected_cursor,
            count,
        }
        .data(),
    }
}

fn install_account(runtime: &mut RuntimeProbe, address: Pubkey, data: Vec<u8>, owner: Pubkey) {
    install_account_with_lamports(runtime, address, data, owner, ACCOUNT_LAMPORTS);
}

fn install_account_with_lamports(
    runtime: &mut RuntimeProbe,
    address: Pubkey,
    data: Vec<u8>,
    owner: Pubkey,
    lamports: u64,
) {
    runtime
        .svm
        .set_account(
            address,
            Account {
                lamports,
                data,
                owner,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("synthetic account injection");
}

fn account_data<T: AccountSerialize + Discriminator>(value: &T) -> Vec<u8> {
    let mut data = Vec::new();
    value
        .try_serialize(&mut data)
        .expect("synthetic account serialization must succeed");
    data
}

fn assert_wire_bytes(runtime: &RuntimeProbe, state: Pubkey, row0: Pubkey, row1: Pubkey) {
    let mut expected_state = default_state(runtime);
    expected_state.cursor = 2;
    expected_state.accumulator = 1;
    assert_eq!(
        runtime.svm.get_account(&state).expect("state").data,
        account_data(&expected_state)
    );
    let (_, bump0) = row_address(0);
    let (_, bump1) = row_address(1);
    let expected_row0 = probe::ProbeBacker {
        index: 0,
        headroom: u128::MAX,
        pending: 0,
        bump: bump0,
    };
    let expected_row1 = probe::ProbeBacker {
        index: 1,
        headroom: u128::MAX,
        pending: 1,
        bump: bump1,
    };
    assert_eq!(
        runtime.svm.get_account(&row0).expect("row 0").data,
        account_data(&expected_row0)
    );
    assert_eq!(
        runtime.svm.get_account(&row1).expect("row 1").data,
        account_data(&expected_row1)
    );
}

fn row_address(index: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            probe::BACKER_SEED,
            state_address().0.as_ref(),
            &index.to_le_bytes(),
        ],
        &probe::ID,
    )
}

fn assert_failure(observation: &Observation, marker: &str) {
    assert!(!observation.success, "expected failure: {observation:?}");
    observation.assert_rollback_except_fee();
    let error = observation.error.as_deref().unwrap_or_default();
    assert!(
        error.contains(marker),
        "expected {marker} in runtime error, got {error:?}"
    );
}

fn assert_failure_code(observation: &Observation, code: u32) {
    assert_failure(observation, &format!("Custom({code})"));
}
