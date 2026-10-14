use anchor_lang::prelude::*;

/// The native program keeps one dense registry. Pages are transport/storage
/// units only; PAGE_CAPACITY is not an economic population cap.
pub const PAGE_CAPACITY: usize = 8;

pub const POOL_SEED: &[u8] = b"pool";
pub const ADMISSION_SEED: &[u8] = b"admission";
pub const GAME_VAULT_SEED: &[u8] = b"game-vault";
pub const OPS_SEED: &[u8] = b"ops";
pub const OPS_CASH_SEED: &[u8] = b"ops-cash";
pub const TICKET_SEED: &[u8] = b"ticket";
pub const USER_SEED: &[u8] = b"user";
pub const USER_ROUND_SEED: &[u8] = b"user-round";
pub const POSITION_SEED: &[u8] = b"position";
pub const BACKER_SEED: &[u8] = b"backer";
pub const REGISTRY_SEED: &[u8] = b"registry";
pub const SNAPSHOT_SEED: &[u8] = b"snapshot";

pub const QUOTA_CALLS: u8 = 3;
pub const QUOTA_WINDOW_SECS: u64 = 3_600;
pub const DISCOVER: u8 = 0;
pub const SENIOR: u8 = 1;
pub const SEAL: u8 = 2;
pub const ALLOCATE: u8 = 3;
pub const PRUNE: u8 = 4;
pub const COMMIT: u8 = 5;
pub const IDLE: u8 = 6;

// Accepted Backer-half split: 35% senior, 45% proportional A, 20% equal B.
pub const SENIOR_SHARE_BPS: u64 = 3_500;
pub const A_SHARE_BPS: u64 = 4_500;
pub const B_SHARE_BPS: u64 = 2_000;

pub const BACKER_REMAINING_CUTOFF: u128 = 10;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace, PartialEq, Eq)]
pub struct SnapshotRow {
    pub key: Pubkey,
    pub series: u8,
    pub remaining: u128,
    pub slot: u64,
}

#[account]
#[derive(InitSpace, Default)]
pub struct Pool {
    pub bump: u8,
    pub round_id: u64,
    pub pot: u64,
    pub seed: u64,
    pub management: u64,
    pub pending_toll: u64,
    pub funded_claims: u64,
    pub excess: u64,
    pub registry_len: u64,
    pub registry_version: u64,
    pub next_sequence: u64,
    pub next_ticket: u64,
    pub active_ticket: u64,
    pub finalized_ticket: u64,
    pub snapshot_pages: u64,
    pub participant_count: u64,
    pub closing: bool,
    pub closed_at: u64,
    pub deadline: u64,
    pub governance: Pubkey,
    pub guardian: Pubkey,
    pub sponsor: Pubkey,
    pub management_recipient: Pubkey,
    pub close_reserve: u64,
}

impl Pool {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Admission {
    pub bump: u8,
    pub guardian: Pubkey,
    pub paused: bool,
}

impl Admission {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct GameVault {
    pub bump: u8,
}

impl GameVault {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Ops {
    pub bump: u8,
    pub sponsor: Pubkey,
    pub free: u64,
    pub locked: u64,
    pub excess: u64,
}

impl Ops {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct User {
    pub bump: u8,
    pub owner: Pubkey,
    pub ever_a: bool,
    pub ever_b: bool,
    pub successful_at: [u64; QUOTA_CALLS as usize],
    pub call_count: u8,
    pub b_entitlement: u128,
}

impl User {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    /// Record one successful admission call in the three-entry rolling window.
    /// An exactly one-hour-old entry has expired and is replaced. Calls do not
    /// record until the enclosing financial operation has passed all checks.
    pub fn record_success(&mut self, now: u64) -> Result<()> {
        let count = self.call_count as usize;
        if count > QUOTA_CALLS as usize {
            return Err(crate::ErrorCode::InvalidState.into());
        }

        if count > 0 && now < self.successful_at[count - 1] {
            return Err(crate::ErrorCode::BackwardsTime.into());
        }

        if count < QUOTA_CALLS as usize {
            self.successful_at[count] = now;
            self.call_count = self
                .call_count
                .checked_add(1)
                .ok_or(crate::ErrorCode::Overflow)?;
            return Ok(());
        }

        let oldest = self.successful_at[0];
        let elapsed = now
            .checked_sub(oldest)
            .ok_or(crate::ErrorCode::BackwardsTime)?;
        if elapsed < QUOTA_WINDOW_SECS {
            return Err(crate::ErrorCode::QuotaExceeded.into());
        }

        self.successful_at[0] = self.successful_at[1];
        self.successful_at[1] = self.successful_at[2];
        self.successful_at[2] = now;
        Ok(())
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct UserRound {
    pub bump: u8,
    pub round_id: u64,
    pub owner: Pubkey,
    pub deposits: u128,
    pub net_payout: u128,
    pub participant_slot: u64,
}

impl UserRound {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Position {
    pub bump: u8,
    pub owner: Pubkey,
    pub nonce: u64,
    pub round_id: u64,
    pub kind: u8,
    pub principal: u64,
    pub opened_at: u64,
    pub maturity_at: u64,
    pub last_accrued_at: u64,
    pub remainder: u128,
    pub accrued_gross: u128,
    pub paid_gross: u128,
    pub net_payout: u128,
    pub claimed: bool,
}

impl Position {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Backer {
    pub bump: u8,
    pub owner: Pubkey,
    pub series: u8,
    pub entitlement: u128,
    pub lifetime: u128,
    pub funded: u64,
    pub pending: u64,
    pub pending_ticket: u64,
    pub first_sequence: u64,
    pub active: bool,
    pub slot: u64,
}

impl Backer {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn remaining(&self) -> Result<u128> {
        self.entitlement
            .checked_sub(self.lifetime)
            .ok_or(crate::ErrorCode::InvalidState.into())
    }

    /// Materialize a finalized ticket's pending credit exactly once. A pending
    /// ticket after the scalar watermark remains pending; no global liability
    /// is changed here because commit already counted it.
    pub fn materialize(&mut self, watermark: u64) -> Result<()> {
        if self.pending_ticket == 0 || self.pending_ticket > watermark {
            return Ok(());
        }

        self.funded = self
            .funded
            .checked_add(self.pending)
            .ok_or(crate::ErrorCode::Overflow)?;
        self.pending = 0;
        self.pending_ticket = 0;
        Ok(())
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct RegistryPage {
    pub bump: u8,
    pub index: u64,
    pub keys: [Pubkey; PAGE_CAPACITY],
}

impl RegistryPage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct SnapshotPage {
    pub bump: u8,
    pub index: u64,
    pub ticket_id: u64,
    pub rows: [SnapshotRow; PAGE_CAPACITY],
}

impl SnapshotPage {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Default)]
pub struct Ticket {
    pub bump: u8,
    pub id: u64,
    pub phase: u8,
    pub captured_len: u64,
    pub expected_version: u64,
    pub cursor: u64,
    pub a_count: u64,
    pub b_count: u64,
    /// Fixed U256 denominator, serialized little-endian as 32 bytes.
    pub a_remaining: [u8; 32],
    pub oldest: Pubkey,
    pub oldest_sequence: u64,
    pub oldest_slot: u64,
    pub senior_credit: u64,
    pub senior_slice: u64,
    pub a_slice: u64,
    pub b_slice: u64,
    pub seed_credit: u64,
    pub credited: u64,
    pub toll: u64,
}

impl Ticket {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialized_len<T: AnchorSerialize>(value: &T) -> usize {
        value.try_to_vec().expect("fixed state serializes").len()
    }

    #[test]
    fn fixed_account_layout_sizes_match_serialized_payloads() {
        assert_eq!(Pool::SPACE, 8 + serialized_len(&Pool::default()));
        assert_eq!(Admission::SPACE, 8 + serialized_len(&Admission::default()));
        assert_eq!(GameVault::SPACE, 8 + serialized_len(&GameVault::default()));
        assert_eq!(Ops::SPACE, 8 + serialized_len(&Ops::default()));
        assert_eq!(User::SPACE, 8 + serialized_len(&User::default()));
        assert_eq!(UserRound::SPACE, 8 + serialized_len(&UserRound::default()));
        assert_eq!(Position::SPACE, 8 + serialized_len(&Position::default()));
        assert_eq!(Backer::SPACE, 8 + serialized_len(&Backer::default()));
        assert_eq!(RegistryPage::SPACE, 8 + serialized_len(&RegistryPage::default()));
        assert_eq!(SnapshotPage::SPACE, 8 + serialized_len(&SnapshotPage::default()));
        assert_eq!(Ticket::SPACE, 8 + serialized_len(&Ticket::default()));
    }

    #[test]
    fn snapshot_page_has_the_frozen_capacity() {
        assert_eq!(SnapshotPage::default().rows.len(), PAGE_CAPACITY);
        assert_eq!(RegistryPage::default().keys.len(), PAGE_CAPACITY);
    }

    #[test]
    fn quota_accepts_exact_hour_boundary() {
        let mut user = User::default();
        user.record_success(10).unwrap();
        user.record_success(20).unwrap();
        user.record_success(30).unwrap();
        user.record_success(10 + QUOTA_WINDOW_SECS).unwrap();
        assert_eq!(user.call_count, QUOTA_CALLS);
        assert_eq!(user.successful_at, [20, 30, 10 + QUOTA_WINDOW_SECS]);
    }

    #[test]
    fn quota_rejects_before_exact_hour_boundary() {
        let mut user = User::default();
        user.record_success(10).unwrap();
        user.record_success(20).unwrap();
        user.record_success(30).unwrap();
        assert!(matches!(
            user.record_success(10 + QUOTA_WINDOW_SECS - 1),
            Err(error) if error == crate::ErrorCode::QuotaExceeded.into()
        ));
    }

    #[test]
    fn backer_remaining_and_finalized_materialization_are_separate() {
        let mut backer = Backer {
            entitlement: 100,
            lifetime: 40,
            funded: 7,
            pending: 11,
            pending_ticket: 4,
            ..Backer::default()
        };
        assert_eq!(backer.remaining().unwrap(), 60);
        backer.materialize(3).unwrap();
        assert_eq!(backer.funded, 7);
        assert_eq!(backer.pending, 11);
        backer.materialize(4).unwrap();
        assert_eq!(backer.funded, 18);
        assert_eq!(backer.pending, 0);
        assert_eq!(backer.pending_ticket, 0);
        assert_eq!(backer.remaining().unwrap(), 60);
    }
}
