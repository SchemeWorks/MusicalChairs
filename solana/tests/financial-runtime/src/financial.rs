//! Financial instruction cases.  These use the actual Anchor instruction
//! ABI and deliberately assert success before the handlers are implemented.

use musical_chairs_program::{accounts, instruction, positions::PlanKindArg};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_system_interface::program as system_program;

use crate::runtime::{
    anchor_ix, FinancialRuntime, Observation, ATTACKER_SEED, OWNER_SEED, SPONSOR_SEED,
};

const DAY: u64 = 86_400;
const OPEN_GROSS: u64 = 100_000_000;
const CLAIM_POT: u64 = 100_000_000;
const CLAIM_MINIMUM: u64 = 9_680_000;

pub fn run_red_open() -> Observation {
    let mut runtime = FinancialRuntime::new();
    let owner = runtime.owner.pubkey();
    let position = runtime.position_address(owner, 1);
    let instruction = open_instruction(&runtime, position, 1, owner);
    let owner_signer = Keypair::new_from_array(OWNER_SEED);
    let observation = runtime.execute(
        "financial.open_moves_real_sol",
        vec![instruction],
        &[&owner_signer],
        1_400_000,
    );

    if observation.success {
        let pool = runtime.deserialize::<musical_chairs_program::state::Pool>(runtime.pool_address());
        let user_round = runtime.deserialize::<musical_chairs_program::state::UserRound>(
            runtime.user_round_address(1, owner),
        );
        let position_state = runtime.deserialize::<musical_chairs_program::state::Position>(position);
        assert_eq!(position_state.principal, OPEN_GROSS);
        assert_eq!(user_round.deposits, u128::from(OPEN_GROSS));
        assert_eq!(pool.pot, 96_000_000);
        assert_eq!(pool.management, 4_000_000);
        assert_eq!(pool.next_sequence, 2);
        assert_eq!(lamport_delta(&observation, runtime.game_vault_address()), OPEN_GROSS as i128);
    }
    observation
}

pub fn run_red_claim() -> Observation {
    let mut runtime = FinancialRuntime::new();
    runtime.seed_claim_position(CLAIM_POT);
    runtime.set_clock(DAY as i64, 2);
    let owner = runtime.owner.pubkey();
    let position = runtime.position_address(owner, 1);
    let instruction = claim_instruction(&runtime, owner, position, runtime.management.pubkey());
    let owner_signer = Keypair::new_from_array(OWNER_SEED);
    let observation = runtime.execute(
        "financial.claim_moves_real_sol",
        vec![instruction],
        &[&owner_signer],
        1_400_000,
    );

    if observation.success {
        let pool = runtime.deserialize::<musical_chairs_program::state::Pool>(runtime.pool_address());
        let position_state = runtime.deserialize::<musical_chairs_program::state::Position>(position);
        assert_eq!(position_state.accrued_gross, 11_000_000);
        assert_eq!(position_state.paid_gross, 11_000_000);
        assert_eq!(position_state.net_payout, 9_680_000);
        assert_eq!(pool.pot, 89_000_000);
        assert_eq!(pool.seed, 1_320_000);
        assert_eq!(lamport_delta(&observation, runtime.management.pubkey()), 9_680_000);
        assert_eq!(lamport_delta(&observation, runtime.game_vault_address()), -9_680_000);
    }
    observation
}

/// The first allocation runtime RED captures the one-row ABI at the exact
/// golden toll.  The host helper is asserted independently so a runtime
/// failure cannot hide a drift in the accepted 35/45/20 split.
pub fn run_red_allocation() -> Observation {
    let slices = musical_chairs_program::allocation::fold_slices(10_000, true, true)
        .expect("golden allocation slices");
    assert_eq!(slices.seed, 5_000);
    assert_eq!(slices.senior, 1_750);
    assert_eq!(slices.a, 2_250);
    assert_eq!(slices.b, 1_000);

    // Golden inputs (toll 10_000, A 4_000/6_000 remaining, two B) are seeded
    // as Backer state; the fresh Discover ticket carries no preloaded
    // counts or slices because `discover_row` computes them.
    let mut runtime = FinancialRuntime::new();
    runtime.seed_allocation();
    let owner = runtime.owner.pubkey();
    let instruction = anchor_ix(
        accounts::DiscoverRow {
            pool: runtime.pool_address(),
            ticket: runtime.ticket_address(),
            registry_page: runtime.registry_address(0),
            backer: runtime.backer_address(musical_chairs_program::allocation::SERIES_A, owner),
            snapshot_page: runtime.snapshot_address(0),
        },
        instruction::DiscoverRow { expected_cursor: 0 },
    );
    runtime.execute(
        "financial.allocation.golden-t10000.discover-row",
        vec![instruction],
        &[],
        1_400_000,
    )
}

/// Execute the complete six-phase allocation ABI against one actual compiled
/// SBF program.  The current production handlers are RED stubs, so this
/// returns observations for the root worker to use while each phase is
/// enabled.  Labels carry an ordinal for every repeated row; no observations
/// are collapsed by phase name.
pub fn run_allocation_golden_flow() -> Vec<Observation> {
    let mut runtime = FinancialRuntime::new();
    runtime.seed_allocation();
    let owners = [
        runtime.owner.pubkey(),
        runtime.attacker.pubkey(),
        runtime.sponsor.pubkey(),
        runtime.management.pubkey(),
    ];
    let mut observations = Vec::new();

    for (row, owner) in owners.into_iter().enumerate() {
        let instruction = anchor_ix(
            accounts::DiscoverRow {
                pool: runtime.pool_address(),
                ticket: runtime.ticket_address(),
                registry_page: runtime.registry_address(0),
                backer: runtime.backer_address(if row < 2 { 0 } else { 1 }, owner),
                snapshot_page: runtime.snapshot_address(0),
            },
            instruction::DiscoverRow {
                expected_cursor: row as u64,
            },
        );
        observations.push(runtime.execute(
            format!("financial.allocation.discover.row-{row}"),
            vec![instruction],
            &[],
            1_400_000,
        ));
    }

    let senior_instruction = anchor_ix(
        accounts::Senior {
            pool: runtime.pool_address(),
            ticket: runtime.ticket_address(),
            oldest_backer: runtime.backer_address(0, owners[0]),
        },
        instruction::Senior {
            expected_ticket_id: 1,
        },
    );
    observations.push(runtime.execute(
        "financial.allocation.senior.row-0",
        vec![senior_instruction],
        &[],
        1_400_000,
    ));
    let seal_instruction = anchor_ix(
        accounts::Seal {
            pool: runtime.pool_address(),
            ticket: runtime.ticket_address(),
        },
        instruction::SealWeights {
            expected_ticket_id: 1,
        },
    );
    observations.push(runtime.execute(
        "financial.allocation.seal-weights.ticket-1",
        vec![seal_instruction],
        &[],
        1_400_000,
    ));

    for (row, owner) in owners.into_iter().enumerate() {
        let instruction = anchor_ix(
            accounts::AllocateRow {
                pool: runtime.pool_address(),
                ticket: runtime.ticket_address(),
                snapshot_page: runtime.snapshot_address(0),
                backer: runtime.backer_address(if row < 2 { 0 } else { 1 }, owner),
            },
            instruction::AllocateRow {
                expected_cursor: row as u64,
            },
        );
        observations.push(runtime.execute(
            format!("financial.allocation.allocate.row-{row}"),
            vec![instruction],
            &[],
            1_400_000,
        ));
    }

    for (step, row) in (0..4).rev().enumerate() {
        let owner = owners[row];
        let moved_owner = if row == 0 { owner } else { owners[0] };
        let instruction = anchor_ix(
            accounts::PruneRow {
                pool: runtime.pool_address(),
                ticket: runtime.ticket_address(),
                snapshot_page: runtime.snapshot_address(0),
                backer: runtime.backer_address(if row < 2 { 0 } else { 1 }, owner),
                registry_page: runtime.registry_address(0),
                last_registry_page: runtime.registry_address(0),
                moved_backer: runtime.backer_address(0, moved_owner),
            },
            instruction::PruneRow {
                expected_cursor: row as u64,
            },
        );
        observations.push(runtime.execute(
            format!("financial.allocation.prune.row-{row}.step-{step}"),
            vec![instruction],
            &[],
            1_400_000,
        ));
    }

    let commit_instruction = anchor_ix(
        accounts::Commit {
            pool: runtime.pool_address(),
            ticket: runtime.ticket_address(),
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::Commit {
            expected_ticket_id: 1,
        },
    );
    observations.push(runtime.execute(
        "financial.allocation.commit.ticket-1",
        vec![commit_instruction],
        &[],
        1_400_000,
    ));

    let owner_signer = Keypair::new_from_array(OWNER_SEED);
    let claim_backer_instruction = anchor_ix(
        accounts::ClaimBacker {
            pool: runtime.pool_address(),
            game_vault: runtime.game_vault_address(),
            owner_signer: owners[0],
            backer: runtime.backer_address(0, owners[0]),
            destination: runtime.management.pubkey(),
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::ClaimBacker {
            minimum_receive: 0,
        },
    );
    observations.push(runtime.execute(
        "financial.allocation.claim-backer.owner-0",
        vec![claim_backer_instruction],
        &[&owner_signer],
        1_400_000,
    ));

    for observation in &observations {
        assert!(
            observation.fits_legacy_packet,
            "production allocation transaction exceeded legacy packet budget: {observation:?}"
        );
    }
    observations
}

/// Control-plane runtime cases are included alongside the financial flow so
/// signer/fee/rollback metadata is measured through the same actual ELF.
pub fn run_controls_suite() -> Vec<Observation> {
    let mut runtime = FinancialRuntime::new();
    let guardian = Keypair::new_from_array(ATTACKER_SEED);
    let owner = Keypair::new_from_array(OWNER_SEED);
    let sponsor = Keypair::new_from_array(SPONSOR_SEED);
    let pause_instruction = anchor_ix(
        accounts::Pause {
            admission: runtime.admission_address(),
            guardian: guardian.pubkey(),
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::SetPause { paused: true },
    );
    let paused = runtime.execute(
        "financial.controls.pause.guardian",
        vec![pause_instruction],
        &[&guardian],
        1_400_000,
    );

    let wrong_pause_instruction = anchor_ix(
        accounts::Pause {
            admission: runtime.admission_address(),
            guardian: owner.pubkey(),
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::SetPause { paused: false },
    );
    let wrong_guardian = runtime.execute(
        "financial.controls.pause.wrong-guardian",
        vec![wrong_pause_instruction],
        &[&owner],
        1_400_000,
    );
    wrong_guardian.assert_rollback_except_fee();

    let fund_instruction = anchor_ix(
        accounts::FundOperations {
            sender: sponsor.pubkey(),
            ops: runtime.ops_address(),
            ops_cash: runtime.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::FundOperations { amount: 20_000 },
    );
    let funded = runtime.execute(
        "financial.controls.operations.fund-20000",
        vec![fund_instruction],
        &[&sponsor],
        1_400_000,
    );

    let withdraw_instruction = anchor_ix(
        accounts::WithdrawOperations {
            sponsor: sponsor.pubkey(),
            ops: runtime.ops_address(),
            ops_cash: runtime.ops_cash_address(),
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::WithdrawOperations { amount: 5_000 },
    );
    let withdrawn = runtime.execute(
        "financial.controls.operations.withdraw-5000",
        vec![withdraw_instruction],
        &[&sponsor],
        1_400_000,
    );

    let management_instruction = anchor_ix(
        accounts::WithdrawManagement {
            pool: runtime.pool_address(),
            game_vault: runtime.game_vault_address(),
            ops: runtime.ops_address(),
            ops_cash: runtime.ops_cash_address(),
            recipient: runtime.management.pubkey(),
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::WithdrawManagement { amount: 1 },
    );
    let management_signer = Keypair::new_from_array(crate::runtime::MANAGEMENT_SEED);
    let management_empty = runtime.execute(
        "financial.controls.management.empty-balance",
        vec![management_instruction],
        &[&management_signer],
        1_400_000,
    );
    management_empty.assert_rollback_except_fee();
    let observations = vec![paused, wrong_guardian, funded, withdrawn, management_empty];
    for observation in &observations {
        assert!(
            observation.fits_legacy_packet,
            "control transaction exceeded legacy packet budget: {observation:?}"
        );
    }
    observations
}

pub fn run_admission_negative_suite() -> Vec<Observation> {
    let mut runtime = FinancialRuntime::new();
    let owner = runtime.owner.pubkey();
    let position = runtime.position_address(owner, 7);
    let no_signer = runtime.execute(
        "financial.open.no-owner-signer",
        vec![open_instruction(&runtime, position, 7, owner)],
        &[],
        1_400_000,
    );
    no_signer.assert_rollback_except_fee();

    let attacker = runtime.attacker.pubkey();
    let attacker_position = runtime.position_address(attacker, 8);
    let attacker_signer = Keypair::new_from_array(ATTACKER_SEED);
    let wrong_owner = runtime.execute(
        "financial.open.wrong-owner-pda",
        vec![open_instruction(&runtime, attacker_position, 8, attacker)],
        &[&attacker_signer],
        1_400_000,
    );
    wrong_owner.assert_rollback_except_fee();

    let mut replay_runtime = FinancialRuntime::new();
    replay_runtime.seed_claim_position(100_000_000);
    let replay_owner = replay_runtime.owner.pubkey();
    let replay_position = replay_runtime.position_address(replay_owner, 1);
    let replay_signer = Keypair::new_from_array(OWNER_SEED);
    let nonce_replay = replay_runtime.execute(
        "financial.open.nonce-replay",
        vec![open_instruction(
            &replay_runtime,
            replay_position,
            1,
            replay_owner,
        )],
        &[&replay_signer],
        1_400_000,
    );
    nonce_replay.assert_rollback_except_fee();

    vec![no_signer, wrong_owner, nonce_replay]
}

fn open_instruction(
    runtime: &FinancialRuntime,
    position: Pubkey,
    nonce: u64,
    owner: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::OpenPosition {
            owner,
            pool: runtime.pool_address(),
            admission: runtime.admission_address(),
            game_vault: runtime.game_vault_address(),
            user: runtime.user_address(owner),
            user_round: runtime.user_round_address(1, owner),
            position,
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::OpenPosition {
            plan: PlanKindArg::Simple,
            gross_lamports: OPEN_GROSS,
            client_nonce: nonce,
        },
    )
}

fn claim_instruction(
    runtime: &FinancialRuntime,
    owner: Pubkey,
    position: Pubkey,
    destination: Pubkey,
) -> Instruction {
    anchor_ix(
        accounts::ClaimPosition {
            owner,
            pool: runtime.pool_address(),
            admission: runtime.admission_address(),
            game_vault: runtime.game_vault_address(),
            ops: runtime.ops_address(),
            ops_cash: runtime.ops_cash_address(),
            user: runtime.user_address(owner),
            user_round: runtime.user_round_address(1, owner),
            position,
            ticket: runtime.ticket_address(),
            destination,
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::ClaimPosition {
            minimum_receive: CLAIM_MINIMUM,
            close_simple: false,
        },
    )
}

fn lamport_delta(observation: &Observation, key: Pubkey) -> i128 {
    observation
        .account_changes
        .iter()
        .find(|change| change.pubkey == key.to_string())
        .map(|change| change.lamport_delta)
        .unwrap_or_else(|| panic!("observation did not include account {key}: {observation:?}"))
}

#[allow(dead_code)]
fn _keypair_pubkey(keypair: &Keypair) -> Pubkey {
    keypair.pubkey()
}
