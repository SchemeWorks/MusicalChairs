pub mod compute;
pub mod permissions;
pub mod provider;
pub mod runtime;

pub fn default_state(
    rt: &runtime::RuntimeProbe,
) -> musical_chairs_runtime_probe_program::ProbeState {
    use musical_chairs_runtime_probe_program::ProbeState;
    let (state, bump) = rt.state_address();
    let (client, _) = orao_solana_vrf_cb::state::client::Client::find_address(
        &musical_chairs_runtime_probe_program::ID,
        &state,
        &orao_solana_vrf_cb::id(),
    );
    use solana_sdk::signature::Signer;
    ProbeState {
        authority: rt.authority.pubkey(),
        client,
        seed: [7; 32],
        deadline: 600,
        requested: false,
        captured: false,
        skipped: false,
        randomness: [0; 64],
        pending_cash: 10_000,
        cursor: 0,
        captured_n: 100,
        accumulator: 0,
        bump,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled_sbf_runtime_suites() {
        super::permissions::run_permission_suite();
        super::compute::run_compute_suite();
        super::provider::run_provider_suite();
    }
}
