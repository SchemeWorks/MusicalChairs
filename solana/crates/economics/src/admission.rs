//! Atomic game and Series A admission quota model.

use crate::fees::{GAME_MINIMUM_LAMPORTS, SERIES_A_MINIMUM_LAMPORTS};

pub const MAX_SUCCESSFUL_CALLS: usize = 3;
pub const ROLLING_WINDOW_SECONDS: u64 = 3_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    BelowGameMinimum,
    BelowSeriesAMinimum,
    QuotaExceeded,
    BackwardsTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Admission {
    successful_at: [u64; MAX_SUCCESSFUL_CALLS],
    count: usize,
}

impl Admission {
    pub const fn new() -> Self {
        Self {
            successful_at: [0; MAX_SUCCESSFUL_CALLS],
            count: 0,
        }
    }

    pub fn open_position(&mut self, now: u64, gross: u64) -> Result<(), Error> {
        if gross < GAME_MINIMUM_LAMPORTS {
            return Err(Error::BelowGameMinimum);
        }
        self.record_success(now)
    }

    pub fn fund_series_a(&mut self, now: u64, gross: u64) -> Result<(), Error> {
        if gross < SERIES_A_MINIMUM_LAMPORTS {
            return Err(Error::BelowSeriesAMinimum);
        }
        self.record_success(now)
    }

    pub fn record_claim_or_round_progress(&mut self) {}

    pub fn successful_calls(&self) -> usize {
        self.count
    }

    fn record_success(&mut self, now: u64) -> Result<(), Error> {
        let mut retained = [0; MAX_SUCCESSFUL_CALLS];
        let mut retained_count = 0;
        for timestamp in self.successful_at[..self.count].iter().copied() {
            if now < timestamp {
                return Err(Error::BackwardsTime);
            }
            if now - timestamp < ROLLING_WINDOW_SECONDS {
                retained[retained_count] = timestamp;
                retained_count += 1;
            }
        }
        if retained_count >= MAX_SUCCESSFUL_CALLS {
            return Err(Error::QuotaExceeded);
        }
        retained[retained_count] = now;
        retained_count += 1;
        self.successful_at = retained;
        self.count = retained_count;
        Ok(())
    }
}

impl Default for Admission {
    fn default() -> Self {
        Self::new()
    }
}
