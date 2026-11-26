use musical_chairs_runtime_probes::{compute, permissions, provider};
fn main() {
    let suite = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let observations = match suite.as_str() {
        "permissions" => permissions::run_permission_suite(),
        "compute" => compute::run_compute_suite(),
        "provider" => provider::run_provider_suite(),
        "all" => {
            let mut rows = permissions::run_permission_suite();
            rows.extend(compute::run_compute_suite());
            rows.extend(provider::run_provider_suite());
            rows
        }
        _ => panic!("suite must be all, permissions, compute or provider"),
    };
    let report = serde_json::json!({
        "scope": "Compiled probe SBF and captured ORAO SBF executed locally in LiteSVM; synthetic test actors; no deployment or live transactions",
        "versions": {"litesvm":"0.7.1","anchor":"0.32.1","solana_sdk":"2.3.1"},
        "runtime_profile": {"feature_set":"LiteSVM 0.7.1 all_enabled (Agave 2.3.13)","default_heap_bytes":32768,"priority_fee":0,"rent":"local default 6960*(data_bytes+128); not a current mainnet quote"},
        "build_provenance": serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/build-provenance.json")).unwrap(),
        "provider_provenance": serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/orao-provenance.json")).unwrap(),
        "observations": observations
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
