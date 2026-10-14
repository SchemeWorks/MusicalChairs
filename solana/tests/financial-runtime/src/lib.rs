//! Actual compiled-SBF financial runtime harness.
//!
//! The fixture is intentionally local and synthetic.  It loads the production
//! Musical Chairs ELF selected by `MC_FINANCIAL_SBF`, injects Anchor-serialized
//! production accounts, and records what LiteSVM actually executed.  It does
//! not make provider, wallet, Squads, deployment, or live-settlement claims.

pub mod financial;
pub mod runtime;

pub use financial::{
    run_allocation_golden_flow, run_controls_suite, run_red_allocation, run_red_claim, run_red_open,
};
pub use runtime::{FinancialRuntime, Observation};

/// Run the small evidence CLI without making a live transaction claim.
pub fn run_suite() -> Vec<Observation> {
    let mut observations = vec![run_red_open(), run_red_claim(), run_red_allocation()];
    observations.extend(run_allocation_golden_flow());
    observations.extend(run_controls_suite());
    observations
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This is intentionally RED while the production handler is still a
    /// `NotImplemented` gate.  It must be the first meaningful failure when
    /// the compiled SBF artifact is loaded by the root build executor.
    #[test]
    fn open_moves_real_sol() {
        let observation = run_red_open();
        assert!(
            observation.success,
            "open_moves_real_sol expected a successful compiled-SBF admission: {observation:?}"
        );
    }

    /// Claim is also RED until the production payout handler is enabled.  It
    /// stays separate from the open test so cargo test reports both financial
    /// instruction boundaries once the first gate is repaired.
    #[test]
    fn claim_moves_real_sol() {
        let observation = run_red_claim();
        assert!(
            observation.success,
            "claim_moves_real_sol expected a successful compiled-SBF payout: {observation:?}"
        );
    }

    #[test]
    fn allocation_golden_t10000() {
        let observation = run_red_allocation();
        assert!(
            observation.success,
            "allocation_golden_t10000 expected a successful compiled-SBF discovery: {observation:?}"
        );
    }

    #[test]
    fn allocation_six_phase_flow_has_unique_row_observations() {
        let observations = run_allocation_golden_flow();
        assert_eq!(observations.len(), 4 + 1 + 1 + 4 + 4 + 1 + 1);
        let labels: std::collections::BTreeSet<_> =
            observations.iter().map(|observation| observation.label.as_str()).collect();
        assert_eq!(labels.len(), observations.len());
        assert!(observations.iter().all(|observation| observation.fits_legacy_packet));
    }

    #[test]
    fn fee_only_rollback_is_observed_for_the_current_open_gate() {
        let observation = run_red_open();
        if !observation.success {
            observation.assert_rollback_except_fee();
        }
    }
}
