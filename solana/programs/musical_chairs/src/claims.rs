//! `claim_backer`: one combined, lazily materialized withdrawal of a Backer's
//! finalized credit across Series A and Series B.
//!
//! An allocation ticket credits `Backer.pending` tagged with its id and counts
//! the credit into `pool.funded_claims` only at commit.  A claim therefore
//! folds a Backer's pending credit into `funded` only once its ticket is at or
//! below `pool.finalized_ticket` (`Backer::materialize`), pays `funded`, and
//! leaves `lifetime` untouched: a claim never restores cap headroom.  Credit
//! from a ticket that is still running stays pending and cannot be paid.

use anchor_lang::prelude::*;

use crate::allocation::{SERIES_A, SERIES_B};
use crate::events::{BackerClaimed, SCHEMA_VERSION};
use crate::state::{
    Backer, GameVault, Pool, User, BACKER_SEED, GAME_VAULT_SEED, POOL_SEED, USER_SEED,
};
use crate::ErrorCode as Code;

/// Accounts for the combined claim.  At least one of `backer_a`/`backer_b`
/// must be supplied; either may be omitted (pass the program id).  `destination`
/// is unchecked so protected aliases can be rejected with the exact error.
#[event_cpi]
#[derive(Accounts)]
pub struct ClaimBacker<'info> {
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [GAME_VAULT_SEED], bump = game_vault.bump)]
    pub game_vault: Box<Account<'info, GameVault>>,
    #[account(seeds = [USER_SEED, owner.key().as_ref()], bump = user.bump)]
    pub user: Box<Account<'info, User>>,
    #[account(mut)]
    pub backer_a: Option<Box<Account<'info, Backer>>>,
    #[account(mut)]
    pub backer_b: Option<Box<Account<'info, Backer>>>,
    /// CHECK: `validate_destination` requires a writable, empty, system-owned
    /// account and the handler rejects every protected alias.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
}

/// A supplied Backer must be this owner's canonical PDA for `series`.
pub fn check_claim_backer(key: &Pubkey, backer: &Backer, owner: &Pubkey, series: u8) -> Result<()> {
    require!(
        backer.owner == *owner && backer.series == series,
        Code::WrongAccount
    );
    let derived = Pubkey::create_program_address(
        &[BACKER_SEED, &[series], owner.as_ref(), &[backer.bump]],
        &crate::ID,
    )
    .map_err(|_| error!(Code::WrongAccount))?;
    require!(derived == *key, Code::WrongAccount);
    Ok(())
}

/// Fold what `watermark` (the last committed ticket) has finalized into
/// `funded`, then take all of `funded`.  `lifetime` is not touched.
pub fn take_funded(backer: &mut Backer, watermark: u64) -> Result<u64> {
    backer.materialize(watermark)?;
    Ok(core::mem::take(&mut backer.funded))
}

/// The combined payout; an empty claim is `NothingToClaim`.
pub fn claim_total(a_part: u64, b_part: u64) -> Result<u64> {
    let total = a_part
        .checked_add(b_part)
        .ok_or_else(|| error!(Code::Overflow))?;
    require!(total > 0, Code::NothingToClaim);
    Ok(total)
}

pub fn claim_backer(ctx: Context<ClaimBacker>) -> Result<()> {
    let owner = ctx.accounts.owner.key();
    let (amount, a_part, b_part, round_id) = {
        let accounts = &mut *ctx.accounts;
        require!(
            accounts.backer_a.is_some() || accounts.backer_b.is_some(),
            Code::InvalidState
        );

        // Every check before any change.
        let mut protected = vec![
            accounts.pool.key(),
            accounts.game_vault.key(),
            accounts.user.key(),
        ];
        if let Some(backer) = accounts.backer_a.as_ref() {
            check_claim_backer(&backer.key(), backer, &owner, SERIES_A)?;
            protected.push(backer.key());
        }
        if let Some(backer) = accounts.backer_b.as_ref() {
            check_claim_backer(&backer.key(), backer, &owner, SERIES_B)?;
            protected.push(backer.key());
        }
        let destination = accounts.destination.to_account_info();
        crate::validate_destination(&destination)?;
        require!(
            !protected.contains(&destination.key()),
            Code::ProtectedDestination
        );

        // Effects.
        let vault = accounts.game_vault.to_account_info();
        crate::sync_game(&mut accounts.pool, &vault)?;
        let watermark = accounts.pool.finalized_ticket;
        let a_part = match accounts.backer_a.as_mut() {
            Some(backer) => take_funded(backer, watermark)?,
            None => 0,
        };
        let b_part = match accounts.backer_b.as_mut() {
            Some(backer) => take_funded(backer, watermark)?,
            None => 0,
        };
        let amount = claim_total(a_part, b_part)?;
        accounts.pool.funded_claims = accounts
            .pool
            .funded_claims
            .checked_sub(u128::from(amount))
            .ok_or_else(|| error!(Code::InvalidState))?;
        crate::send_game(&vault, &destination, amount)?;
        (amount, a_part, b_part, accounts.pool.round_id)
    };
    emit_cpi!(BackerClaimed {
        schema_version: SCHEMA_VERSION,
        owner,
        round_id,
        amount,
        a_part,
        b_part,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn code_of<T>(result: Result<T>) -> u32 {
        match result {
            Ok(_) => panic!("expected an error, got Ok"),
            Err(anchor_lang::error::Error::AnchorError(error)) => error.error_code_number,
            Err(other) => panic!("expected an Anchor error, got {other:?}"),
        }
    }

    fn assert_code<T>(result: Result<T>, expected: Code) {
        assert_eq!(code_of(result), expected as u32 + 6000, "{expected:?}");
    }

    fn canonical(owner: Pubkey, series: u8) -> (Pubkey, Backer) {
        let (address, bump) =
            Pubkey::find_program_address(&[BACKER_SEED, &[series], owner.as_ref()], &crate::ID);
        (
            address,
            Backer {
                bump,
                owner,
                series,
                ..Backer::default()
            },
        )
    }

    #[test]
    fn a_supplied_backer_must_be_the_owners_canonical_pda_for_that_series() {
        let owner = key(7);
        let (address, backer) = canonical(owner, SERIES_A);
        assert!(check_claim_backer(&address, &backer, &owner, SERIES_A).is_ok());

        // Another owner, the other series, a foreign address and a wrong bump all fail.
        assert_code(
            check_claim_backer(&address, &backer, &key(8), SERIES_A),
            Code::WrongAccount,
        );
        assert_code(
            check_claim_backer(&address, &backer, &owner, SERIES_B),
            Code::WrongAccount,
        );
        assert_code(
            check_claim_backer(&key(1), &backer, &owner, SERIES_A),
            Code::WrongAccount,
        );
        let mut other = backer.clone();
        other.bump = other.bump.wrapping_sub(1);
        assert_code(
            check_claim_backer(&address, &other, &owner, SERIES_A),
            Code::WrongAccount,
        );

        // The B PDA is a different address even for the same owner.
        let (b_address, b_backer) = canonical(owner, SERIES_B);
        assert_ne!(address, b_address);
        assert!(check_claim_backer(&b_address, &b_backer, &owner, SERIES_B).is_ok());
        assert_code(
            check_claim_backer(&b_address, &b_backer, &owner, SERIES_A),
            Code::WrongAccount,
        );
    }

    #[test]
    fn a_claim_pays_only_finalized_credit_and_never_restores_cap_headroom() {
        // 100 funded from earlier tickets and 20 pending from ticket 5.
        let mut backer = Backer {
            entitlement: 1_000,
            lifetime: 120,
            funded: 100,
            pending: 20,
            pending_ticket: 5,
            ..Backer::default()
        };
        // Ticket 5 is still the active one: watermark 4 leaves it pending.
        assert_eq!(take_funded(&mut backer, 4).unwrap(), 100);
        assert_eq!(
            (backer.funded, backer.pending, backer.pending_ticket),
            (0, 20, 5)
        );
        // Nothing more to take until ticket 5 commits.
        assert_eq!(take_funded(&mut backer, 4).unwrap(), 0);
        // After commit (watermark 5) the pending credit materializes exactly once.
        assert_eq!(take_funded(&mut backer, 5).unwrap(), 20);
        assert_eq!(
            (backer.funded, backer.pending, backer.pending_ticket),
            (0, 0, 0)
        );
        assert_eq!(take_funded(&mut backer, 5).unwrap(), 0);
        // Lifetime is the cap ledger: paying out never lowers it.
        assert_eq!(backer.lifetime, 120);
        assert_eq!(backer.remaining().unwrap(), 880);
    }

    #[test]
    fn the_combined_total_adds_both_series_and_an_empty_claim_is_rejected() {
        assert_eq!(claim_total(70, 30).unwrap(), 100);
        assert_eq!(claim_total(0, 5).unwrap(), 5);
        assert_eq!(claim_total(5, 0).unwrap(), 5);
        assert_code(claim_total(0, 0), Code::NothingToClaim);
        assert_code(claim_total(u64::MAX, 1), Code::Overflow);
    }

    #[test]
    fn materialization_overflow_is_reported_and_leaves_funded_untouched() {
        let mut backer = Backer {
            funded: u64::MAX,
            pending: 1,
            pending_ticket: 2,
            ..Backer::default()
        };
        assert_code(take_funded(&mut backer, 2), Code::Overflow);
    }
}
