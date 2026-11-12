//! Checked host-side earnings model for native SOL positions.

use ethnum::U256;

pub const SECONDS_PER_DAY: u64 = 86_400;
pub const SIMPLE_DAYS: u64 = 21;
pub const SIMPLE_EXPIRY_SECONDS: u64 = SIMPLE_DAYS * SECONDS_PER_DAY;
pub const SIMPLE_DENOMINATOR: u128 = 8_640_000;
pub const SIMPLE_RATE_NUMERATOR: u128 = 11;
pub const Q18: u128 = 1_000_000_000_000_000_000;
pub const ROI_15_Q18: u128 = 4_473_565_759_257_037_859;
pub const ROI_30_Q18: u128 = 12_267_678_469_131_246_220;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    Overflow,
    BackwardsTime,
    NotMature,
    InvalidTollBps,
    MinimumReceiveNotMet,
    InvalidState,
    InvalidRemainder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanKind {
    Simple,
    Compound15,
    Compound30,
}

/// Terms pinned when a position opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub principal: u64,
    pub opened_at: u64,
    pub maturity_at: u64,
    pub kind: PlanKind,
}

impl Plan {
    pub fn simple(principal: u64, opened_at: u64) -> Result<Self, MathError> {
        Ok(Self {
            principal,
            opened_at,
            maturity_at: opened_at
                .checked_add(SIMPLE_EXPIRY_SECONDS)
                .ok_or(MathError::Overflow)?,
            kind: PlanKind::Simple,
        })
    }

    pub fn compound_15(principal: u64, opened_at: u64) -> Result<Self, MathError> {
        Self::compound(principal, opened_at, 15, PlanKind::Compound15)
    }

    pub fn compound_30(principal: u64, opened_at: u64) -> Result<Self, MathError> {
        Self::compound(principal, opened_at, 30, PlanKind::Compound30)
    }

    fn compound(
        principal: u64,
        opened_at: u64,
        days: u64,
        kind: PlanKind,
    ) -> Result<Self, MathError> {
        let duration = days
            .checked_mul(SECONDS_PER_DAY)
            .ok_or(MathError::Overflow)?;
        Ok(Self {
            principal,
            opened_at,
            maturity_at: opened_at.checked_add(duration).ok_or(MathError::Overflow)?,
            kind,
        })
    }

    pub fn owed_at(&self, now: u64) -> Result<u128, MathError> {
        let duration = match self.kind {
            PlanKind::Simple => SIMPLE_EXPIRY_SECONDS,
            PlanKind::Compound15 => 15 * SECONDS_PER_DAY,
            PlanKind::Compound30 => 30 * SECONDS_PER_DAY,
        };
        let expected_maturity = self
            .opened_at
            .checked_add(duration)
            .ok_or(MathError::InvalidState)?;
        if self.maturity_at != expected_maturity {
            return Err(MathError::InvalidState);
        }
        if self.kind == PlanKind::Simple {
            return Err(MathError::NotMature);
        }
        if now < self.maturity_at {
            return Err(MathError::NotMature);
        }
        let factor = match self.kind {
            PlanKind::Simple => return Err(MathError::NotMature),
            PlanKind::Compound15 => ROI_15_Q18,
            PlanKind::Compound30 => ROI_30_Q18,
        };
        U256::from(self.principal)
            .checked_mul(U256::from(factor))
            .ok_or(MathError::Overflow)
            .and_then(|value| {
                let owed = value / U256::from(Q18);
                if owed > U256::from(u128::MAX) {
                    Err(MathError::Overflow)
                } else {
                    Ok(owed.as_u128())
                }
            })
    }
}

/// Mutable Simple accrual state. `remainder` carries sub-lamport numerator dust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleAccrual {
    pub principal: u64,
    pub last_accrued_at: u64,
    pub expires_at: u64,
    pub remainder: u128,
    pub accrued_gross: u128,
}

impl SimpleAccrual {
    pub fn new(principal: u64, opened_at: u64) -> Result<Self, MathError> {
        Ok(Self {
            principal,
            last_accrued_at: opened_at,
            expires_at: opened_at
                .checked_add(SIMPLE_EXPIRY_SECONDS)
                .ok_or(MathError::Overflow)?,
            remainder: 0,
            accrued_gross: 0,
        })
    }

    /// Accrues only the newly earned gross amount and leaves state untouched on error.
    pub fn accrue(&mut self, now: u64) -> Result<u128, MathError> {
        if self.expires_at < self.last_accrued_at {
            return Err(MathError::InvalidState);
        }
        if self.remainder >= SIMPLE_DENOMINATOR {
            return Err(MathError::InvalidRemainder);
        }
        if now < self.last_accrued_at {
            return Err(MathError::BackwardsTime);
        }
        let end = now.min(self.expires_at);
        if end == self.last_accrued_at {
            return Ok(0);
        }
        let elapsed = u128::from(end - self.last_accrued_at);
        let numerator = U256::from(self.principal)
            .checked_mul(U256::from(SIMPLE_RATE_NUMERATOR))
            .and_then(|value| value.checked_mul(U256::from(elapsed)))
            .and_then(|value| value.checked_add(U256::from(self.remainder)))
            .ok_or(MathError::Overflow)?;
        let denominator = U256::from(SIMPLE_DENOMINATOR);
        let whole = numerator / denominator;
        let remainder = numerator % denominator;
        if whole > U256::from(u128::MAX) {
            return Err(MathError::Overflow);
        }
        let whole = whole.as_u128();
        let accrued_gross = self
            .accrued_gross
            .checked_add(whole)
            .ok_or(MathError::Overflow)?;
        self.last_accrued_at = end;
        self.remainder = remainder.as_u128();
        self.accrued_gross = accrued_gross;
        Ok(whole)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payout {
    pub gross_owed: u128,
    pub paid_gross: u64,
    pub toll: u64,
    pub net: u64,
    pub insolvent: bool,
}

impl Payout {
    /// Pays earnings only: principal is never refunded by this model.
    pub fn from_gross(
        gross_owed: u128,
        pot: u64,
        toll_bps: u16,
        minimum_receive: u64,
    ) -> Result<Self, MathError> {
        let fee = crate::fees::payout(gross_owed, pot, toll_bps).map_err(|error| match error {
            crate::fees::MathError::Overflow => MathError::Overflow,
            crate::fees::MathError::InvalidBps
            | crate::fees::MathError::InvalidCompoundDuration => MathError::InvalidTollBps,
        })?;
        if fee.net < minimum_receive {
            return Err(MathError::MinimumReceiveNotMet);
        }
        Ok(Self {
            gross_owed,
            paid_gross: fee.paid_gross,
            toll: fee.toll,
            net: fee.net,
            insolvent: fee.insolvent,
        })
    }
}
