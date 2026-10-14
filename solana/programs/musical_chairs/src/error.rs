//! Program error codes.
//!
//! Anchor numbers `#[error_code]` variants from 6000 in declaration order, so
//! the order below is a frozen client ABI: never reorder or insert, only
//! append.  The first 22 variants (through `NotImplemented`, 6021) predate the
//! M2 error split; the nine after it were appended for the allocation,
//! claim, draw and configuration paths; `WrongGovernanceProfile` (6031) was
//! appended in M3 for Squads profile-shape failures; the five after it
//! (6032..=6036) were appended in M4 for provisioning and cleanup; the two
//! after those (6037, 6038) were appended in M5 for admission; the two after
//! those (6039, 6040) were appended in M6 for the allocation ticket; the nine
//! after those (6041..=6049) were appended in M7 for the close, draw and
//! provider-cash paths.
use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    Overflow, InvalidState, BackwardsTime, QuotaExceeded, NotAuthorized,
    WrongAccount, WrongOwner, InsufficientOperations, ProtectedDestination,
    AdmissionPaused, AllocationPending, Closing, MinimumDeposit, MinimumReceive,
    NotMature, AlreadyClaimed, StaleRound, Replay, InvalidPhase, CursorMismatch,
    CapExceeded, NotImplemented,
    // Appended in M2 (6022..=6030); keep this order.
    NothingToClaim, InsufficientManagement, ReserveUnavailable, PositionRoundClosed,
    DuplicateWritableAccount, LateCapture, Collision, WrongProviderTuple,
    ConfigNotActivatable,
    // Appended in M3 (6031); keep this order.
    WrongGovernanceProfile,
    // Appended in M4 (6032..=6036); keep this order.
    WrongIndex, NotClosable, AlreadyClosed, PageNotProvisioned, LockNotPermitted,
    // Appended in M5 (6037..=6038); keep this order.
    StaleConfig, PublicSeriesADisabled,
    // Appended in M6 (6039..=6040); keep this order.
    StaleTicket, StaleCursor,
    // Appended in M7 (6041..=6049); keep this order.
    ProviderFeeExceeded, RequestNotFulfilled, DeadlinePassed, DeadlineNotReached,
    PromotionTerminal, NotSealed, CandidatesIncomplete, NothingEligible, AlreadyObserved,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stable code freeze: Anchor offsets variants by 6000 in declaration order.
    #[test]
    fn error_codes_are_frozen() {
        assert_eq!(ErrorCode::Overflow as u32 + 6000, 6000);
        assert_eq!(ErrorCode::NotImplemented as u32 + 6000, 6021);
        assert_eq!(ErrorCode::NothingToClaim as u32 + 6000, 6022);
        assert_eq!(ErrorCode::InsufficientManagement as u32 + 6000, 6023);
        assert_eq!(ErrorCode::ReserveUnavailable as u32 + 6000, 6024);
        assert_eq!(ErrorCode::PositionRoundClosed as u32 + 6000, 6025);
        assert_eq!(ErrorCode::DuplicateWritableAccount as u32 + 6000, 6026);
        assert_eq!(ErrorCode::LateCapture as u32 + 6000, 6027);
        assert_eq!(ErrorCode::Collision as u32 + 6000, 6028);
        assert_eq!(ErrorCode::WrongProviderTuple as u32 + 6000, 6029);
        assert_eq!(ErrorCode::ConfigNotActivatable as u32 + 6000, 6030);
        assert_eq!(ErrorCode::WrongGovernanceProfile as u32 + 6000, 6031);
        assert_eq!(ErrorCode::WrongIndex as u32 + 6000, 6032);
        assert_eq!(ErrorCode::NotClosable as u32 + 6000, 6033);
        assert_eq!(ErrorCode::AlreadyClosed as u32 + 6000, 6034);
        assert_eq!(ErrorCode::PageNotProvisioned as u32 + 6000, 6035);
        assert_eq!(ErrorCode::LockNotPermitted as u32 + 6000, 6036);
        assert_eq!(ErrorCode::StaleConfig as u32 + 6000, 6037);
        assert_eq!(ErrorCode::PublicSeriesADisabled as u32 + 6000, 6038);
        assert_eq!(ErrorCode::StaleTicket as u32 + 6000, 6039);
        assert_eq!(ErrorCode::StaleCursor as u32 + 6000, 6040);
        assert_eq!(ErrorCode::ProviderFeeExceeded as u32 + 6000, 6041);
        assert_eq!(ErrorCode::RequestNotFulfilled as u32 + 6000, 6042);
        assert_eq!(ErrorCode::DeadlinePassed as u32 + 6000, 6043);
        assert_eq!(ErrorCode::DeadlineNotReached as u32 + 6000, 6044);
        assert_eq!(ErrorCode::PromotionTerminal as u32 + 6000, 6045);
        assert_eq!(ErrorCode::NotSealed as u32 + 6000, 6046);
        assert_eq!(ErrorCode::CandidatesIncomplete as u32 + 6000, 6047);
        assert_eq!(ErrorCode::NothingEligible as u32 + 6000, 6048);
        assert_eq!(ErrorCode::AlreadyObserved as u32 + 6000, 6049);
    }

    /// The codes the M7 close handlers reuse from earlier tranches must not
    /// drift either.
    #[test]
    fn close_codes_are_frozen() {
        assert_eq!(ErrorCode::InvalidState as u32 + 6000, 6001);
        assert_eq!(ErrorCode::NotAuthorized as u32 + 6000, 6004);
        assert_eq!(ErrorCode::WrongAccount as u32 + 6000, 6005);
        assert_eq!(ErrorCode::WrongOwner as u32 + 6000, 6006);
        assert_eq!(ErrorCode::InsufficientOperations as u32 + 6000, 6007);
        assert_eq!(ErrorCode::ProtectedDestination as u32 + 6000, 6008);
        assert_eq!(ErrorCode::CursorMismatch as u32 + 6000, 6019);
        assert_eq!(ErrorCode::ReserveUnavailable as u32 + 6000, 6024);
        assert_eq!(ErrorCode::LateCapture as u32 + 6000, 6027);
        assert_eq!(ErrorCode::Collision as u32 + 6000, 6028);
        assert_eq!(ErrorCode::WrongProviderTuple as u32 + 6000, 6029);
        assert_eq!(ErrorCode::PageNotProvisioned as u32 + 6000, 6035);
        assert_eq!(ErrorCode::LockNotPermitted as u32 + 6000, 6036);
    }

    /// The codes the M6 allocation handlers reuse from earlier tranches must
    /// not drift either.
    #[test]
    fn allocation_codes_are_frozen() {
        assert_eq!(ErrorCode::Overflow as u32 + 6000, 6000);
        assert_eq!(ErrorCode::InvalidState as u32 + 6000, 6001);
        assert_eq!(ErrorCode::WrongAccount as u32 + 6000, 6005);
        assert_eq!(ErrorCode::WrongOwner as u32 + 6000, 6006);
        assert_eq!(ErrorCode::ProtectedDestination as u32 + 6000, 6008);
        assert_eq!(ErrorCode::InvalidPhase as u32 + 6000, 6018);
        assert_eq!(ErrorCode::CursorMismatch as u32 + 6000, 6019);
        assert_eq!(ErrorCode::CapExceeded as u32 + 6000, 6020);
        assert_eq!(ErrorCode::NothingToClaim as u32 + 6000, 6022);
        assert_eq!(ErrorCode::DuplicateWritableAccount as u32 + 6000, 6026);
    }

    /// The codes the M5 handlers reuse from earlier tranches must not drift.
    #[test]
    fn admission_and_payout_codes_are_frozen() {
        assert_eq!(ErrorCode::BackwardsTime as u32 + 6000, 6002);
        assert_eq!(ErrorCode::QuotaExceeded as u32 + 6000, 6003);
        assert_eq!(ErrorCode::ProtectedDestination as u32 + 6000, 6008);
        assert_eq!(ErrorCode::AdmissionPaused as u32 + 6000, 6009);
        assert_eq!(ErrorCode::AllocationPending as u32 + 6000, 6010);
        assert_eq!(ErrorCode::Closing as u32 + 6000, 6011);
        assert_eq!(ErrorCode::MinimumDeposit as u32 + 6000, 6012);
        assert_eq!(ErrorCode::MinimumReceive as u32 + 6000, 6013);
        assert_eq!(ErrorCode::NotMature as u32 + 6000, 6014);
        assert_eq!(ErrorCode::AlreadyClaimed as u32 + 6000, 6015);
        assert_eq!(ErrorCode::StaleRound as u32 + 6000, 6016);
    }
}
