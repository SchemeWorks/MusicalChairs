//! Financial instruction cases.  These use the actual Anchor instruction
//! ABI and deliberately assert success before the handlers are implemented.

use musical_chairs_program::{
    accounts, instruction, positions::PlanKindArg, state, BackerClaimed, TicketCommitted,
};
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
    // The first open of a round appends the owner to the round's participant
    // page, which must already be provisioned (`PageNotProvisioned` 6035
    // otherwise).  The canonical fixture carries no such page, so this row
    // installs Participant page 0 of round 1 itself.
    runtime.install_participant_page(1, 0);
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
        // Opening a game position never allocates a Backer funding sequence:
        // the genesis `next_sequence` of 1 is unchanged.
        assert_eq!(pool.next_sequence, 1);
        assert_eq!(pool.participant_count, 1);
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

/// The first allocation runtime row: the M0 RED turned GREEN with the SAME
/// label.  One `discover_rows` call walks the whole seeded golden registry
/// (toll 10_000; Series A 4_000 / 6_000; two Series B) in one batch of four
/// rows.  The host `fold_slices` assertions are unchanged and are compared with
/// the slices the program itself computed when Discover closed.
pub fn run_red_allocation() -> Observation {
    let slices = musical_chairs_program::allocation::fold_slices(10_000, true, true)
        .expect("golden allocation slices");
    assert_eq!(slices.seed, 5_000);
    assert_eq!(slices.senior, 1_750);
    assert_eq!(slices.a, 2_250);
    assert_eq!(slices.b, 1_000);

    // Golden inputs (toll 10_000, A 4_000/6_000 remaining, two B) are seeded
    // as Backer state; the fresh Discover ticket carries no preloaded
    // counts or slices because `discover_rows` computes them.
    let mut runtime = FinancialRuntime::new();
    runtime.seed_allocation();
    let owner = runtime.owner.pubkey();
    let ticket = runtime.ticket_state();
    let instruction = runtime.discover_ix(
        ticket.id,
        0,
        4,
        runtime.discover_remaining(0, 4),
    );
    let observation = runtime.execute(
        "financial.allocation.golden-t10000.discover-row",
        vec![instruction],
        &[],
        1_400_000,
    );

    if observation.success {
        let ticket = runtime.ticket_state();
        assert_eq!((ticket.phase, ticket.cursor), (state::SENIOR, 4));
        assert_eq!((ticket.a_count, ticket.b_count), (2, 2));
        let mut total = [0u8; 32];
        total[..16].copy_from_slice(&10_000u128.to_le_bytes());
        assert_eq!(ticket.a_remainder_total, total, "4_000 + 6_000 open Series A remainders");
        // The slices the program computed are the host golden slices.
        assert_eq!(ticket.seed_credit, slices.seed);
        assert_eq!(ticket.senior_slice, slices.senior);
        assert_eq!(ticket.a_slice, slices.a);
        assert_eq!(ticket.b_slice, slices.b);
        // The oldest open A is the first-funded A (sequence 1, slot 0).
        assert_eq!(ticket.oldest, runtime.backer_address(musical_chairs_program::allocation::SERIES_A, owner));
        assert_eq!((ticket.oldest_sequence, ticket.oldest_slot), (1, 0));
        assert_eq!(ticket.credited, 0, "Discover credits nothing");
        runtime.conservation_check();
    }
    observation
}

/// Execute the complete six-phase allocation ABI against one actual compiled
/// SBF program, driven by the golden vector with ONE row per instruction: four
/// Discover calls, Senior, Seal, four Allocate calls, four Prune calls (every
/// row is open after the credits, so each passes the placeholder), Commit, and
/// the first Series A Backer's combined claim.  Every instruction must succeed;
/// the ticket must end in `IDLE` with the golden credits (seed 5_001; A 2_363
/// / 1_636; B 500 / 500; `funded_claims` 4_999; `pending_toll` 0) and the
/// game-vault identity must hold after every step.  Labels carry an ordinal
/// for every repeated row; no observations are collapsed by phase name.
pub fn run_allocation_golden_flow() -> Vec<Observation> {
    let mut runtime = FinancialRuntime::new();
    runtime.seed_allocation();
    let owners = [
        runtime.owner.pubkey(),
        runtime.attacker.pubkey(),
        runtime.sponsor.pubkey(),
        runtime.management.pubkey(),
    ];
    let series = [0u8, 0, 1, 1];
    let mut observations = Vec::new();
    let mut prune_steps = 0;

    while runtime.ticket_state().phase != state::IDLE {
        let step = runtime.next_phase_step(1, false);
        let ticket_id = runtime.ticket_state().id;
        let label = match step.name {
            "discover" => format!("financial.allocation.discover.row-{}", step.cursor),
            "senior" => format!("financial.allocation.senior.ticket-{ticket_id}"),
            "seal" => format!("financial.allocation.seal-weights.ticket-{ticket_id}"),
            "allocate" => format!("financial.allocation.allocate.row-{}", step.cursor),
            "prune" => {
                let label = format!(
                    "financial.allocation.prune.row-{}.step-{prune_steps}",
                    step.cursor - 1
                );
                prune_steps += 1;
                label
            }
            _ => format!("financial.allocation.commit.ticket-{ticket_id}"),
        };
        let observation = runtime.execute(label, vec![step.instruction], &[], 1_400_000);
        assert!(
            observation.success,
            "{} must succeed: {observation:?}",
            observation.label
        );
        runtime.conservation_check();
        observations.push(observation);
    }

    // The committed ticket, read from the commit transaction's own event.
    let committed = observations
        .last()
        .expect("the flow ends with Commit")
        .last_event::<TicketCommitted>();
    assert_eq!(
        committed,
        TicketCommitted {
            schema_version: 1,
            ticket_id: 1,
            round_id: 1,
            toll: 10_000,
            credited: 4_999,
            seed: 5_001,
        }
    );

    let pool = runtime.pool_state();
    let ticket = runtime.ticket_state();
    assert_eq!(pool.seed, 5_001);
    assert_eq!(pool.funded_claims, 4_999);
    assert_eq!(pool.pending_toll, 0);
    assert_eq!((pool.active_ticket, pool.finalized_ticket), (0, 1));
    assert_eq!((ticket.phase, ticket.credited, ticket.seed_credit), (state::IDLE, 4_999, 5_001));
    let credits = [2_363u64, 1_636, 500, 500];
    for index in 0..4 {
        let backer = runtime.backer_state(runtime.backer_address(series[index], owners[index]));
        assert_eq!(backer.pending, credits[index], "owner {index} credit");
        assert_eq!(backer.lifetime, u128::from(credits[index]), "owner {index} lifetime");
        assert!(backer.active, "nobody reached the cap");
    }

    // The first Series A Backer claims its finalized credit.
    let owner_signer = Keypair::new_from_array(OWNER_SEED);
    let destination = runtime.management.pubkey();
    let destination_before = runtime.account(destination).lamports;
    let claim_backer_instruction = runtime.claim_backer_ix(
        owners[0],
        Some(runtime.backer_address(0, owners[0])),
        None,
        destination,
    );
    let claim = runtime.execute(
        "financial.allocation.claim-backer.owner-0",
        vec![claim_backer_instruction],
        &[&owner_signer],
        1_400_000,
    );
    assert!(claim.success, "claim_backer must succeed: {claim:?}");
    runtime.conservation_check();
    assert_eq!(runtime.account(destination).lamports - destination_before, 2_363);
    assert_eq!(runtime.pool_state().funded_claims, 4_999 - 2_363);
    let claimed = claim.last_event::<BackerClaimed>();
    assert_eq!((claimed.amount, claimed.a_part, claimed.b_part), (2_363, 2_363, 0));
    observations.push(claim);

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
            // M5 account list: the pool's active ConfigVersion (v1 in the
            // fixture) and the round-1 participant page at index
            // `participant_count / 8` (0 in the fixture).
            config: runtime.config_address(1),
            participant_page: runtime.participant_address(1, 0),
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::OpenPosition {
            plan: PlanKindArg::Simple,
            gross: OPEN_GROSS,
            client_nonce: nonce,
            expected_round_id: 1,
            expected_config: 1,
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
        accounts::Payout {
            owner,
            pool: runtime.pool_address(),
            admission: runtime.admission_address(),
            game_vault: runtime.game_vault_address(),
            ops: runtime.ops_address(),
            ops_cash: runtime.ops_cash_address(),
            user_round: runtime.user_round_address(1, owner),
            position,
            ticket: runtime.ticket_address(),
            // The position's pinned ConfigVersion (v1 in the fixture).
            config: runtime.config_address(1),
            destination,
            // Optional: only a payout that closes the round passes a
            // CloseRecord; `None` encodes as the program id placeholder.
            close_record: None,
            system_program: system_program::ID,
            event_authority: runtime.event_authority_address(),
            program: musical_chairs_program::ID,
        },
        instruction::WithdrawSimple {
            minimum_receive: CLAIM_MINIMUM,
            max_work_contribution: 0,
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
