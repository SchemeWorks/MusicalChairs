use anchor_lang::__private::BpfWriter;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::set_return_data, sysvar::clock::Clock};
use ethnum::U256;

#[path = "../../../../crates/economics/src/earnings.rs"]
pub mod earnings;
#[path = "../../../../crates/economics/src/fees.rs"]
pub mod fees;

use earnings::{Plan, SimpleAccrual};

use orao::state::{
    client::Client,
    network_state::NetworkState,
    request::{RequestAccount, RequestState},
};
use orao_solana_vrf_cb as orao;

declare_id!("6Ckm2BrnXxsSjyG5b17kQQRjoECVrts92RKXVGT8XeqS");

/// The provider registration state is a PDA of this probe program.
pub const STATE_SEED: &[u8] = b"mc-vrf-state";
/// Probe-only row seed. The index is encoded as little-endian `u16`.
pub const BACKER_SEED: &[u8] = b"mc-backer";
pub const MAX_REPETITIONS: u8 = 32;
pub const MAX_CHUNK: u8 = 32;
pub const WIRE_VERSION: u8 = 1;

/// Anchor's eight-byte account discriminator plus the serialized fields below.
pub const PROBE_STATE_SPACE: usize = 208;
/// Anchor's eight-byte account discriminator plus the serialized row fields.
pub const PROBE_BACKER_SPACE: usize = 35;

#[account]
pub struct ProbeState {
    pub authority: Pubkey,
    pub client: Pubkey,
    pub seed: [u8; 32],
    pub deadline: i64,
    pub requested: bool,
    pub captured: bool,
    pub skipped: bool,
    pub randomness: [u8; 64],
    pub pending_cash: u64,
    pub cursor: u16,
    pub captured_n: u16,
    pub accumulator: u128,
    pub bump: u8,
}

#[account]
pub struct ProbeBacker {
    pub index: u16,
    pub headroom: u128,
    pub pending: u64,
    pub bump: u8,
}

#[error_code]
pub enum ProbeError {
    #[msg("unsupported probe wire version")]
    UnsupportedVersion,
    #[msg("repetitions must be between one and thirty-two")]
    InvalidRepetitions,
    #[msg("chunk must be between one and thirty-two")]
    InvalidChunk,
    #[msg("cursor does not match the captured row position")]
    StaleCursor,
    #[msg("unexpected row account count")]
    UnexpectedRowCount,
    #[msg("duplicate or aliased row account")]
    DuplicateAccount,
    #[msg("row account is not writable")]
    RowNotWritable,
    #[msg("row account has the wrong owner")]
    WrongRowOwner,
    #[msg("row account has the wrong address")]
    WrongRowAddress,
    #[msg("row account has the wrong discriminator or data")]
    MalformedRow,
    #[msg("row index does not match the cursor")]
    WrongRowIndex,
    #[msg("probe arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("probe arithmetic helper failed")]
    MathFailure,
    #[msg("provider account binding does not match the state")]
    ProviderBindingMismatch,
    #[msg("provider request is already past the application deadline")]
    RequestDeadlinePassed,
    #[msg("promotion was already terminally skipped")]
    RequestAlreadySkipped,
    #[msg("request account is not owned by ORAO")]
    WrongRequestOwner,
    #[msg("request account has the wrong address")]
    WrongRequestAddress,
    #[msg("request account has the wrong client, seed, or bump")]
    WrongRequestBinding,
    #[msg("request account is malformed")]
    MalformedRequest,
    #[msg("request is not fulfilled")]
    RequestNotFulfilled,
    #[msg("captured randomness does not match the stored result")]
    CaptureMismatch,
    #[msg("the deadline has not expired")]
    DeadlineNotReached,
    #[msg("a timely capture is already stored")]
    CaptureAlreadyStored,
}

#[program]
pub mod musical_chairs_runtime_probe_program {
    use super::*;

    pub fn probe_authorized_write(
        ctx: Context<ProbeAuthorizedWrite>,
        version: u8,
        value: u128,
    ) -> Result<()> {
        require_version(version)?;
        ctx.accounts.state.accumulator = value;
        Ok(())
    }

    pub fn probe_numeric(ctx: Context<ProbeNumeric>, version: u8, repetitions: u8) -> Result<()> {
        require_version(version)?;
        require!(
            (1..=MAX_REPETITIONS).contains(&repetitions),
            ProbeError::InvalidRepetitions
        );

        let mut total = 0u128;
        for repetition in 0..repetitions {
            total = total
                .checked_add(numeric_iteration(repetition)?)
                .ok_or(ProbeError::ArithmeticOverflow)?;
        }
        ctx.accounts.state.accumulator = ctx
            .accounts
            .state
            .accumulator
            .checked_add(total)
            .ok_or(ProbeError::ArithmeticOverflow)?;
        set_return_data(&ctx.accounts.state.accumulator.to_le_bytes());
        Ok(())
    }

    pub fn probe_chunk(
        ctx: Context<ProbeChunk>,
        version: u8,
        expected_cursor: u16,
        count: u8,
    ) -> Result<()> {
        require_version(version)?;
        require!((1..=MAX_CHUNK).contains(&count), ProbeError::InvalidChunk);
        require_keys_eq!(
            ctx.accounts.state.key(),
            state_pda().0,
            ProbeError::ProviderBindingMismatch
        );
        require_eq!(
            ctx.accounts.state.cursor,
            expected_cursor,
            ProbeError::StaleCursor
        );
        require_eq!(
            ctx.remaining_accounts.len(),
            usize::from(count),
            ProbeError::UnexpectedRowCount
        );

        require!(
            expected_cursor
                .checked_add(u16::from(count))
                .is_some_and(|end| end <= ctx.accounts.state.captured_n),
            ProbeError::InvalidChunk
        );
        let state_key = ctx.accounts.state.key();
        let start = expected_cursor;
        let mut rows = [ValidatedRow::EMPTY; MAX_CHUNK as usize];

        // Validate every account and deserialize all values before taking a
        // mutable borrow. A malformed later row therefore cannot leave an
        // earlier row or the scalar state changed.
        for (offset, row_info) in ctx.remaining_accounts.iter().enumerate() {
            if row_info.key == &state_key
                || ctx
                    .remaining_accounts
                    .iter()
                    .take(offset)
                    .any(|prior| prior.key == row_info.key)
            {
                return err!(ProbeError::DuplicateAccount);
            }
            if !row_info.is_writable {
                return err!(ProbeError::RowNotWritable);
            }
            if row_info.owner != &crate::ID {
                return err!(ProbeError::WrongRowOwner);
            }

            let index = start
                .checked_add(u16::try_from(offset).map_err(|_| ProbeError::ArithmeticOverflow)?)
                .ok_or(ProbeError::ArithmeticOverflow)?;
            let index_bytes = index.to_le_bytes();
            let (expected_key, expected_bump) = Pubkey::find_program_address(
                &[BACKER_SEED, state_key.as_ref(), &index_bytes],
                &crate::ID,
            );
            if *row_info.key != expected_key {
                return err!(ProbeError::WrongRowAddress);
            }

            let data = row_info.try_borrow_data()?;
            if data.len() != PROBE_BACKER_SPACE || &data[..8] != ProbeBacker::DISCRIMINATOR {
                return err!(ProbeError::MalformedRow);
            }
            let mut serialized = &data[..];
            let row = ProbeBacker::try_deserialize(&mut serialized)
                .map_err(|_| error!(ProbeError::MalformedRow))?;
            if row.index != index || row.bump != expected_bump {
                return err!(ProbeError::WrongRowIndex);
            }
            rows[offset] = ValidatedRow {
                headroom: row.headroom,
                pending: row.pending,
                index,
                bump: row.bump,
            };
        }

        // This is deliberately representative arithmetic: it exercises the
        // checked U256 pro-rata/cap path without claiming to implement the
        // complete Backer allocation oracle.
        let denominator = U256::from(10_000u64)
            .checked_add(U256::from(ctx.accounts.state.captured_n))
            .ok_or(ProbeError::ArithmeticOverflow)?;
        let mut delta = 0u128;
        for (offset, row_info) in ctx.remaining_accounts.iter().enumerate() {
            let row = rows[offset];
            let weighted = U256::from(ctx.accounts.state.pending_cash)
                .checked_mul(U256::from(u64::from(row.index) + 1))
                .ok_or(ProbeError::ArithmeticOverflow)?
                / denominator;
            let capped = weighted.min(U256::from(row.headroom));
            let credit = u64::try_from(capped).map_err(|_| ProbeError::ArithmeticOverflow)?;
            let next_pending = row
                .pending
                .checked_add(credit)
                .ok_or(ProbeError::ArithmeticOverflow)?;
            let updated = ProbeBacker {
                index: row.index,
                headroom: row.headroom,
                pending: next_pending,
                bump: row.bump,
            };
            let mut data = row_info.try_borrow_mut_data()?;
            let mut writer = BpfWriter::new(&mut data[..]);
            updated
                .try_serialize(&mut writer)
                .map_err(|_| error!(ProbeError::MalformedRow))?;
            delta = delta
                .checked_add(u128::from(credit))
                .ok_or(ProbeError::ArithmeticOverflow)?;
        }
        ctx.accounts.state.cursor = ctx
            .accounts
            .state
            .cursor
            .checked_add(u16::from(count))
            .ok_or(ProbeError::ArithmeticOverflow)?;
        ctx.accounts.state.accumulator = ctx
            .accounts
            .state
            .accumulator
            .checked_add(delta)
            .ok_or(ProbeError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn probe_request(ctx: Context<ProbeRequest>, version: u8, sign_state: bool) -> Result<()> {
        require_version(version)?;
        validate_provider_binding(&ctx.accounts)?;

        if ctx.accounts.state.requested {
            // The binding has already been checked above. Retrying the same
            // request is idempotent and never selects a new seed/PDA.
            return Ok(());
        }
        require!(
            !ctx.accounts.state.skipped,
            ProbeError::RequestAlreadySkipped
        );
        let now = Clock::get()?.unix_timestamp;
        require!(
            now <= ctx.accounts.state.deadline,
            ProbeError::RequestDeadlinePassed
        );

        let cpi_accounts = orao::cpi::accounts::Request {
            payer: ctx.accounts.payer.to_account_info(),
            state: ctx.accounts.state.to_account_info(),
            client: ctx.accounts.client.to_account_info(),
            network_state: ctx.accounts.network_state.to_account_info(),
            treasury: ctx.accounts.treasury.to_account_info(),
            request: ctx.accounts.request.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let cpi_program = ctx.accounts.provider_program.to_account_info();
        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
        let params = orao::RequestParams::new(ctx.accounts.state.seed);
        if sign_state {
            let bump = [ctx.accounts.state.bump];
            let signer_seeds: &[&[u8]] = &[STATE_SEED, &bump];
            orao::cpi::request(cpi_context.with_signer(&[signer_seeds]), params)?;
        } else {
            // Negative permission fixture: the state account is passed as a
            // signer meta but no PDA signature is supplied to the CPI.
            orao::cpi::request(cpi_context, params)?;
        }
        ctx.accounts.state.requested = true;
        Ok(())
    }

    pub fn probe_capture(ctx: Context<ProbeCapture>, version: u8) -> Result<()> {
        require_version(version)?;
        let request = read_fulfilled_request(&ctx.accounts)?;
        if ctx.accounts.state.skipped {
            return Ok(());
        }
        let now = Clock::get()?.unix_timestamp;
        if ctx.accounts.state.captured {
            require!(
                ctx.accounts.state.randomness == request,
                ProbeError::CaptureMismatch
            );
            return Ok(());
        }
        if now > ctx.accounts.state.deadline {
            // Binding and fulfillment were authenticated first. A late
            // authentic result is a no-op and cannot overwrite or mint state.
            return Ok(());
        }
        ctx.accounts.state.randomness = request;
        ctx.accounts.state.captured = true;
        Ok(())
    }

    pub fn probe_expire(ctx: Context<ProbeExpire>, version: u8) -> Result<()> {
        require_version(version)?;
        let now = Clock::get()?.unix_timestamp;
        require!(
            now > ctx.accounts.state.deadline,
            ProbeError::DeadlineNotReached
        );
        if ctx.accounts.state.captured {
            return err!(ProbeError::CaptureAlreadyStored);
        }
        if !ctx.accounts.state.skipped {
            ctx.accounts.state.skipped = true;
        }
        Ok(())
    }
}

#[derive(Accounts)]
pub struct ProbeAuthorizedWrite<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump, has_one = authority)]
    pub state: Account<'info, ProbeState>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct ProbeNumeric<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump)]
    pub state: Account<'info, ProbeState>,
}

#[derive(Accounts)]
pub struct ProbeChunk<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump)]
    pub state: Account<'info, ProbeState>,
}

#[derive(Accounts)]
pub struct ProbeRequest<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump)]
    pub state: Account<'info, ProbeState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub client: Account<'info, Client>,
    #[account(mut)]
    pub network_state: Account<'info, NetworkState>,
    /// CHECK: the ORAO account constraint checks the treasury against network state.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: ORAO validates the canonical request PDA and initializes it.
    #[account(mut)]
    pub request: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub provider_program: Program<'info, orao::program::OraoVrfCb>,
}

#[derive(Accounts)]
pub struct ProbeCapture<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump)]
    pub state: Account<'info, ProbeState>,
    /// CHECK: owner, discriminator, PDA, client, seed and bump are checked below.
    pub request: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ProbeExpire<'info> {
    #[account(mut, seeds = [STATE_SEED], bump = state.bump)]
    pub state: Account<'info, ProbeState>,
}

#[derive(Clone, Copy)]
struct ValidatedRow {
    headroom: u128,
    pending: u64,
    index: u16,
    bump: u8,
}

impl ValidatedRow {
    const EMPTY: Self = Self {
        headroom: 0,
        pending: 0,
        index: 0,
        bump: 0,
    };
}

fn require_version(version: u8) -> Result<()> {
    require_eq!(version, WIRE_VERSION, ProbeError::UnsupportedVersion);
    Ok(())
}

fn state_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STATE_SEED], &crate::ID)
}

fn numeric_iteration(repetition: u8) -> Result<u128> {
    let gross = 10_000_000u64
        .checked_add(u64::from(repetition) * 1_000_000)
        .ok_or(ProbeError::ArithmeticOverflow)?;
    let entry = fees::front_end_load(gross).map_err(|_| error!(ProbeError::MathFailure))?;
    let mut simple = SimpleAccrual::new(gross, 0).map_err(|_| error!(ProbeError::MathFailure))?;
    let simple_gross = simple
        .accrue((u64::from(repetition) + 1) * earnings::SECONDS_PER_DAY)
        .map_err(|_| error!(ProbeError::MathFailure))?;
    let compound = Plan::compound_15(gross, 0)
        .and_then(|plan| plan.owed_at(15 * earnings::SECONDS_PER_DAY))
        .map_err(|_| error!(ProbeError::MathFailure))?;
    let compound_30 = Plan::compound_30(gross, 0)
        .and_then(|plan| plan.owed_at(30 * earnings::SECONDS_PER_DAY))
        .map_err(|_| error!(ProbeError::MathFailure))?;
    let payout = earnings::Payout::from_gross(compound, entry.pot_credit, 1_200, 0)
        .map_err(|_| error!(ProbeError::MathFailure))?;
    let series_a =
        fees::series_a_entitlement(gross).map_err(|_| error!(ProbeError::MathFailure))?;
    let series_b =
        fees::series_b_entitlement(simple_gross).map_err(|_| error!(ProbeError::MathFailure))?;
    let prospective =
        fees::prospective_series_b_entitlement(u128::from(gross), u128::from(payout.net), series_b)
            .map_err(|_| error!(ProbeError::MathFailure))?;

    // Keep a checked wide intermediate in the same style as the host oracle.
    let wide = U256::from(payout.paid_gross)
        .checked_mul(U256::from(series_a))
        .ok_or(ProbeError::ArithmeticOverflow)?
        / U256::from(100u8);
    let wide = u128::try_from(wide).map_err(|_| error!(ProbeError::ArithmeticOverflow))?;
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
    .try_fold(0u128, |sum, value| {
        sum.checked_add(value)
            .ok_or_else(|| error!(ProbeError::ArithmeticOverflow))
    })
}

fn validate_provider_binding(accounts: &ProbeRequest<'_>) -> Result<()> {
    let state_key = accounts.state.key();
    let (expected_client, expected_client_bump) =
        Client::find_address(&crate::ID, &state_key, &orao::id());
    require_keys_eq!(
        accounts.client.key(),
        expected_client,
        ProbeError::ProviderBindingMismatch
    );
    require_keys_eq!(
        accounts.state.client,
        expected_client,
        ProbeError::ProviderBindingMismatch
    );
    require_eq!(
        accounts.client.bump,
        expected_client_bump,
        ProbeError::ProviderBindingMismatch
    );
    require_keys_eq!(
        accounts.client.program,
        crate::ID,
        ProbeError::ProviderBindingMismatch
    );
    require_keys_eq!(
        accounts.client.state,
        state_key,
        ProbeError::ProviderBindingMismatch
    );

    let (expected_network, expected_network_bump) = NetworkState::find_address(&orao::id());
    require_keys_eq!(
        accounts.network_state.key(),
        expected_network,
        ProbeError::ProviderBindingMismatch
    );
    require_eq!(
        accounts.network_state.bump,
        expected_network_bump,
        ProbeError::ProviderBindingMismatch
    );
    require_keys_eq!(
        accounts.network_state.config.treasury,
        accounts.treasury.key(),
        ProbeError::ProviderBindingMismatch
    );

    let (expected_request, expected_request_bump) =
        RequestAccount::find_address(&expected_client, &accounts.state.seed, &orao::id());
    require_keys_eq!(
        accounts.request.key(),
        expected_request,
        ProbeError::ProviderBindingMismatch
    );
    if accounts.request.owner != &orao::id() && accounts.state.requested {
        return err!(ProbeError::WrongRequestOwner);
    }
    if accounts.state.requested {
        let data = accounts.request.try_borrow_data()?;
        let mut serialized = &data[..];
        let request = RequestAccount::try_deserialize(&mut serialized)
            .map_err(|_| error!(ProbeError::MalformedRequest))?;
        require_eq!(
            request.bump,
            expected_request_bump,
            ProbeError::WrongRequestBinding
        );
        require_keys_eq!(
            request.client,
            expected_client,
            ProbeError::WrongRequestBinding
        );
        require!(
            request.seed == accounts.state.seed,
            ProbeError::WrongRequestBinding
        );
    }
    Ok(())
}

fn read_fulfilled_request(accounts: &ProbeCapture<'_>) -> Result<[u8; 64]> {
    let state_key = accounts.state.key();
    let (expected_client, _) = Client::find_address(&crate::ID, &state_key, &orao::id());
    require_keys_eq!(
        accounts.state.client,
        expected_client,
        ProbeError::WrongRequestBinding
    );
    let (expected_request, expected_request_bump) =
        RequestAccount::find_address(&expected_client, &accounts.state.seed, &orao::id());
    require_keys_eq!(
        accounts.request.key(),
        expected_request,
        ProbeError::WrongRequestAddress
    );
    require!(
        accounts.request.owner == &orao::id(),
        ProbeError::WrongRequestOwner
    );
    let data = accounts.request.try_borrow_data()?;
    if data.len() < 8 || &data[..8] != RequestAccount::DISCRIMINATOR {
        return err!(ProbeError::MalformedRequest);
    }
    let mut serialized = &data[..];
    let request = RequestAccount::try_deserialize(&mut serialized)
        .map_err(|_| error!(ProbeError::MalformedRequest))?;
    require_eq!(
        request.bump,
        expected_request_bump,
        ProbeError::WrongRequestBinding
    );
    require_keys_eq!(
        request.client,
        expected_client,
        ProbeError::WrongRequestBinding
    );
    require!(
        request.seed == accounts.state.seed,
        ProbeError::WrongRequestBinding
    );
    require!(
        request.state.fulfilled().is_some(),
        ProbeError::RequestNotFulfilled
    );
    match request.state {
        RequestState::Fulfilled(result) => Ok(result.randomness),
        _ => err!(ProbeError::RequestNotFulfilled),
    }
}
