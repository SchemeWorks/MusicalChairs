//! Program error codes.
//!
//! Anchor numbers `#[error_code]` variants from 6000 in declaration order, so
//! the order below is a frozen client ABI: never reorder or insert, only
//! append.  The first 22 variants (through `NotImplemented`, 6021) predate the
//! M2 error split; the nine after it were appended for the allocation,
//! claim, draw and configuration paths.
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
    }
}
