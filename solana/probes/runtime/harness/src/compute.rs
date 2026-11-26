//! Compute, packet, and bounded-row probes for the compiled runtime program.
//!
//! The observations in this module are local LiteSVM measurements.  They are
//! useful for sizing a future instruction, but LiteSVM acceptance is not
//! network admission evidence and these rows do not implement the complete
//! Backer allocation oracle.

use anchor_lang::{InstructionData, ToAccountMetas};
use musical_chairs_runtime_probe_program as probe;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::runtime::{Observation, RuntimeProbe};

const PACKET_LIMIT_BYTES: usize = 1_232;
const COMPUTE_LIMIT: u32 = 1_400_000;
const INSUFFICIENT_COMPUTE_LIMIT: u32 = 1_000;
const CAPTURED_N_VALUES: [u16; 2] = [100, 256];
const REPETITIONS: [u8; 5] = [1, 4, 8, 16, 32];
const CHUNKS: [u8; 5] = [1, 4, 8, 16, 32];
// Independently derived integer vectors for the current numeric probe, including the pinned
// Compound-30 term.  The source oracle below is checked against these values
// so a source change cannot silently move the runtime acceptance target.
const NUMERIC_GOLDEN: [(u8, u128); 5] = [
    (1, 1_190_603_712_761),
    (4, 6_357_700_103_503),
    (8, 17_858_353_481_825),
    (16, 62_383_555_045_327),
    (32, 280_193_874_389_371),
];

/// Execute the compiled SBF numeric and representative chunk paths.
///
/// Every successful row carries the runtime's actual compute metadata.  The
/// source-derived host oracle checks the current Simple/Compound-15/
/// Compound-30/fee mix in `probe_numeric`.
pub fn run_compute_suite() -> Vec<Observation> {
    let mut observations = Vec::new();
    observations.extend(run_numeric_matrix());
    observations.extend(run_chunk_matrix());
    observations.push(run_insufficient_compute());
    observations
}

fn run_numeric_matrix() -> Vec<Observation> {
    let mut observations = Vec::new();

    for repetitions in REPETITIONS {
        let mut runtime = RuntimeProbe::new();
        let state = canonical_state(&runtime, 100);
        runtime.put_state(state);
        let state_address = runtime.state_address().0;
        let observation = runtime.execute(
            &format!("compute.numeric.repetitions-{repetitions}"),
            vec![numeric_instruction(state_address, repetitions)],
            &[],
            COMPUTE_LIMIT,
        );
        assert_success_metadata(&observation);
        let expected = expected_numeric_golden(repetitions);
        let state = runtime.state();
        assert_eq!(state.accumulator, expected, "numeric state oracle mismatch");
        assert_eq!(decode_u128(&observation.return_data), expected);
        assert!(observation.changed_data_bytes > 0);
        observations.push(observation);
    }

    for (label, repetitions) in [("zero", 0u8), ("max-plus-one", probe::MAX_REPETITIONS + 1)] {
        let mut runtime = RuntimeProbe::new();
        let state = canonical_state(&runtime, 100);
        runtime.put_state(state);
        let state_address = runtime.state_address().0;
        let observation = runtime.execute(
            &format!("compute.numeric.invalid-{label}"),
            vec![numeric_instruction(state_address, repetitions)],
            &[],
            COMPUTE_LIMIT,
        );
        assert!(
            !observation.success,
            "invalid repetition unexpectedly succeeded"
        );
        assert_failure_metadata(&observation);
        observation.assert_rollback_except_fee();
        assert_eq!(runtime.state().accumulator, 0);
        observations.push(observation);
    }

    observations
}

fn run_chunk_matrix() -> Vec<Observation> {
    let mut observations = Vec::new();

    for captured_n in CAPTURED_N_VALUES {
        for count in CHUNKS {
            let mut runtime = RuntimeProbe::new();
            let mut state = canonical_state(&runtime, captured_n);
            // Use a non-zero representative cash amount so even the first
            // row persists a credit.  The denominator remains the exact
            // captured_n-dependent checked-U256 path in the SBF program.
            state.pending_cash = 1_000_000;
            runtime.put_state(state);
            let rows = inject_rows(&mut runtime, count);
            let state_address = runtime.state_address().0;
            let observation = runtime.execute(
                &format!("compute.chunk.captured-n-{captured_n}.rows-{count}"),
                vec![chunk_instruction(state_address, count, &rows)],
                &[],
                COMPUTE_LIMIT,
            );
            assert_success_metadata(&observation);

            let state_after = runtime.state();
            assert_eq!(state_after.cursor, u16::from(count));
            let expected_delta = expected_chunk_delta(state_after.pending_cash, captured_n, count);
            assert_eq!(state_after.accumulator, expected_delta);
            for (offset, row_key) in rows.iter().enumerate() {
                let row = runtime.row(offset as u16);
                assert_eq!(row.index, offset as u16);
                let expected_credit = expected_credit(
                    state_after.pending_cash,
                    captured_n,
                    offset as u16,
                    row.headroom,
                );
                assert_eq!(row.pending, 7 + expected_credit);
                assert_eq!(*row_key, row_address(state_address, offset as u16));
            }
            assert!(observation.changed_data_bytes > 0);
            annotate_packet_limit(&mut observations, observation);
        }
    }

    observations
}

fn run_insufficient_compute() -> Observation {
    let mut runtime = RuntimeProbe::new();
    let state = canonical_state(&runtime, 100);
    runtime.put_state(state);
    let state_address = runtime.state_address().0;
    let observation = runtime.execute(
        "compute.numeric.insufficient-compute-1000",
        vec![numeric_instruction(state_address, 1)],
        &[],
        INSUFFICIENT_COMPUTE_LIMIT,
    );
    assert!(
        !observation.success,
        "the 1,000-CU negative case unexpectedly succeeded"
    );
    assert_failure_metadata(&observation);
    assert_eq!(
        observation.error.as_deref(),
        Some("InstructionError(1, ProgramFailedToComplete)")
    );
    assert!(
        observation
            .logs
            .iter()
            .any(|line| line.contains("exceeded CUs meter")),
        "failure must be compute exhaustion"
    );
    assert_eq!(observation.compute_units, Some(1_000));
    observation.assert_rollback_except_fee();
    assert_eq!(runtime.state().accumulator, 0);
    observation
}

fn annotate_packet_limit(observations: &mut Vec<Observation>, mut observation: Observation) {
    if observation.packet_bytes > PACKET_LIMIT_BYTES {
        let note = if observation.success {
            format!(
                "legacy packet is {} bytes (> {}); LiteSVM accepted it, which is not network-admission proof",
                observation.packet_bytes, PACKET_LIMIT_BYTES
            )
        } else {
            format!(
                "legacy packet is {} bytes (> {}); LiteSVM failure observed, with no network-admission conclusion",
                observation.packet_bytes, PACKET_LIMIT_BYTES
            )
        };
        observation.unsupported = Some(match observation.unsupported.take() {
            Some(existing) => format!("{existing}; {note}"),
            None => note,
        });
    }
    observations.push(observation);
}

fn assert_success_metadata(observation: &Observation) {
    assert!(observation.success, "probe failed: {observation:?}");
    let units = observation
        .compute_units
        .expect("LiteSVM success must expose compute metadata");
    assert!(units > 0, "successful probe reported zero compute units");
    assert!(
        units <= u64::from(observation.compute_limit),
        "successful probe exceeded its declared compute limit: {observation:?}"
    );
}

fn assert_failure_metadata(observation: &Observation) {
    let units = observation
        .compute_units
        .expect("LiteSVM failure must expose compute metadata");
    assert!(units > 0, "failed probe reported zero compute units");
    assert!(
        units <= u64::from(observation.compute_limit),
        "failed probe exceeded its declared compute limit: {observation:?}"
    );
}

fn canonical_state(runtime: &RuntimeProbe, captured_n: u16) -> probe::ProbeState {
    let mut state = crate::default_state(runtime);
    state.captured_n = captured_n;
    state
}

fn inject_rows(runtime: &mut RuntimeProbe, count: u8) -> Vec<Pubkey> {
    (0..u16::from(count))
        .map(|index| runtime.put_row(index, u128::MAX, 7))
        .collect()
}

fn row_address(state: Pubkey, index: u16) -> Pubkey {
    Pubkey::find_program_address(
        &[probe::BACKER_SEED, state.as_ref(), &index.to_le_bytes()],
        &probe::ID,
    )
    .0
}

fn numeric_instruction(state: Pubkey, repetitions: u8) -> Instruction {
    anchor_ix(
        probe::accounts::ProbeNumeric { state },
        probe::instruction::ProbeNumeric {
            version: probe::WIRE_VERSION,
            repetitions,
        },
    )
}

fn chunk_instruction(state: Pubkey, count: u8, rows: &[Pubkey]) -> Instruction {
    let mut accounts = probe::accounts::ProbeChunk { state }.to_account_metas(None);
    accounts.extend(rows.iter().copied().map(|row| AccountMeta::new(row, false)));
    Instruction {
        program_id: probe::ID,
        accounts,
        data: probe::instruction::ProbeChunk {
            version: probe::WIRE_VERSION,
            expected_cursor: 0,
            count,
        }
        .data(),
    }
}

fn anchor_ix<A: ToAccountMetas, D: InstructionData>(accounts: A, data: D) -> Instruction {
    Instruction {
        program_id: probe::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// This is intentionally the same source path as the on-program numeric
/// iteration.  It is an oracle for the current probe, not an independent game
/// allocation implementation.  The current input range also keeps the wide
/// product below u128; the program uses checked U256 for that intermediate.
fn expected_numeric_total(repetitions: u8) -> u128 {
    (0..repetitions)
        .map(|repetition| {
            let gross = 10_000_000u64 + u64::from(repetition) * 1_000_000;
            let entry = probe::fees::front_end_load(gross).expect("source fee oracle");
            let mut simple =
                probe::earnings::SimpleAccrual::new(gross, 0).expect("source simple oracle");
            let simple_gross = simple
                .accrue((u64::from(repetition) + 1) * probe::earnings::SECONDS_PER_DAY)
                .expect("source accrual oracle");
            let compound = probe::earnings::Plan::compound_15(gross, 0)
                .and_then(|plan| plan.owed_at(15 * probe::earnings::SECONDS_PER_DAY))
                .expect("source compound oracle");
            let compound_30 = probe::earnings::Plan::compound_30(gross, 0)
                .and_then(|plan| plan.owed_at(30 * probe::earnings::SECONDS_PER_DAY))
                .expect("source compound-30 oracle");
            let payout = probe::earnings::Payout::from_gross(compound, entry.pot_credit, 1_200, 0)
                .expect("source payout oracle");
            let series_a = probe::fees::series_a_entitlement(gross).expect("source A oracle");
            let series_b =
                probe::fees::series_b_entitlement(simple_gross).expect("source B oracle");
            let prospective = probe::fees::prospective_series_b_entitlement(
                u128::from(gross),
                u128::from(payout.net),
                series_b,
            )
            .expect("source prospective B oracle");
            let wide = u128::from(payout.paid_gross)
                .checked_mul(series_a)
                .expect("representative wide product")
                / 100;
            [
                u128::from(entry.entry_fee),
                u128::from(entry.pot_credit),
                simple_gross,
                compound,
                compound_30,
                u128::from(payout.net),
                series_a,
                series_b,
                prospective,
                wide,
            ]
            .into_iter()
            .sum::<u128>()
        })
        .sum()
}

fn expected_numeric_golden(repetitions: u8) -> u128 {
    let expected = NUMERIC_GOLDEN
        .iter()
        .find_map(|(count, total)| (*count == repetitions).then_some(*total))
        .expect("numeric repetition must have an independent integer golden vector");
    assert_eq!(
        expected_numeric_total(repetitions),
        expected,
        "source numeric oracle diverged from the independent integer golden vector"
    );
    expected
}

fn expected_credit(pending_cash: u64, captured_n: u16, index: u16, headroom: u128) -> u64 {
    let denominator = 10_000u128 + u128::from(captured_n);
    let weighted = u128::from(pending_cash) * u128::from(u64::from(index) + 1) / denominator;
    weighted.min(headroom) as u64
}

fn expected_chunk_delta(pending_cash: u64, captured_n: u16, count: u8) -> u128 {
    (0..u16::from(count))
        .map(|index| u128::from(expected_credit(pending_cash, captured_n, index, u128::MAX)))
        .sum()
}

fn decode_u128(bytes: &[u8]) -> u128 {
    let value: [u8; 16] = bytes
        .try_into()
        .expect("probe numeric return data must be one u128");
    u128::from_le_bytes(value)
}
