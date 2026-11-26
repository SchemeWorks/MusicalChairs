//! Integer fee and entitlement helpers for the native SOL model.

use ethnum::U256;

pub const BPS_DENOMINATOR: u128 = 10_000;
pub const FRONT_END_LOAD_BPS: u16 = 400;
pub const GAME_MINIMUM_LAMPORTS: u64 = 10_000_000;
pub const SERIES_A_MINIMUM_LAMPORTS: u64 = 50_000_000;
pub const DAY: u64 = 86_400;
pub const BACKER_OPEN_REMAINING_CUTOFF_LAMPORTS: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    Overflow,
    InvalidBps,
    InvalidCompoundDuration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryFee {
    pub gross: u64,
    pub entry_fee: u64,
    pub pot_credit: u64,
}

pub fn front_end_load(gross: u64) -> Result<EntryFee, MathError> {
    let fee = checked_bps(gross, FRONT_END_LOAD_BPS)?;
    Ok(EntryFee {
        gross,
        entry_fee: fee,
        pot_credit: gross.checked_sub(fee).ok_or(MathError::Overflow)?,
    })
}

pub fn entry_fee(gross: u64) -> Result<EntryFee, MathError> {
    front_end_load(gross)
}

pub fn simple_toll_bps(elapsed_seconds: u64) -> u16 {
    if elapsed_seconds < 7 * DAY {
        1_200
    } else if elapsed_seconds < 14 * DAY {
        750
    } else {
        300
    }
}

pub fn carried_interest_bps(elapsed_seconds: u64) -> u16 {
    simple_toll_bps(elapsed_seconds)
}

pub fn compound_jackpot_fee_bps(duration_days: u64) -> Result<u16, MathError> {
    match duration_days {
        15 => Ok(900),
        30 => Ok(1_300),
        _ => Err(MathError::InvalidCompoundDuration),
    }
}

pub fn jackpot_fee_bps(duration_days: u64) -> Result<u16, MathError> {
    compound_jackpot_fee_bps(duration_days)
}

pub fn can_open_backer_with_remaining(remaining: u64) -> bool {
    remaining > BACKER_OPEN_REMAINING_CUTOFF_LAMPORTS
}

pub fn funded_dust_is_claimable(funded: u64) -> bool {
    funded > 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeePayout {
    pub gross_owed: u128,
    pub paid_gross: u64,
    pub toll: u64,
    pub net: u64,
    pub insolvent: bool,
}

pub fn payout(gross_owed: u128, pot: u64, toll_bps: u16) -> Result<FeePayout, MathError> {
    if toll_bps > 10_000 {
        return Err(MathError::InvalidBps);
    }
    let paid_gross = gross_owed.min(u128::from(pot));
    let paid_gross = u64::try_from(paid_gross).map_err(|_| MathError::Overflow)?;
    let toll = checked_bps(paid_gross, toll_bps)?;
    Ok(FeePayout {
        gross_owed,
        paid_gross,
        toll,
        net: paid_gross.checked_sub(toll).ok_or(MathError::Overflow)?,
        insolvent: gross_owed > u128::from(pot),
    })
}

pub fn payout_fee(gross_owed: u128, pot: u64, toll_bps: u16) -> Result<FeePayout, MathError> {
    payout(gross_owed, pot, toll_bps)
}

fn checked_bps(amount: u64, bps: u16) -> Result<u64, MathError> {
    if bps > 10_000 {
        return Err(MathError::InvalidBps);
    }
    let value = U256::from(amount)
        .checked_mul(U256::from(bps))
        .ok_or(MathError::Overflow)?
        / U256::from(BPS_DENOMINATOR);
    u64::try_from(value).map_err(|_| MathError::Overflow)
}

pub fn series_a_entitlement(principal: u64) -> Result<u128, MathError> {
    checked_entitlement(U256::from(principal), 124)
}

pub fn series_b_entitlement(loss: u128) -> Result<u128, MathError> {
    checked_entitlement(U256::from(loss), 116)
}

/// Computes the future B entitlement before accepting a game deposit.
pub fn prospective_series_b_entitlement(
    deposits: u128,
    payouts: u128,
    existing_b_entitlement: u128,
) -> Result<u128, MathError> {
    let loss = deposits.saturating_sub(payouts);
    let future = U256::from(existing_b_entitlement)
        .checked_add(
            U256::from(loss)
                .checked_mul(U256::from(116u16))
                .ok_or(MathError::Overflow)?
                / U256::from(100u8),
        )
        .ok_or(MathError::Overflow)?;
    if future > U256::from(u128::MAX) {
        return Err(MathError::Overflow);
    }
    Ok(future.as_u128())
}

fn checked_entitlement(value: U256, multiplier: u16) -> Result<u128, MathError> {
    let entitlement = value
        .checked_mul(U256::from(multiplier))
        .ok_or(MathError::Overflow)?
        / U256::from(100u8);
    if entitlement > U256::from(u128::MAX) {
        return Err(MathError::Overflow);
    }
    Ok(entitlement.as_u128())
}
