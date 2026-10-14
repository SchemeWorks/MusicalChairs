use musical_chairs_financial_runtime::{
    run_allocation_golden_flow, run_controls_suite, run_red_allocation, run_red_claim,
    run_red_open,
};

fn main() {
    let suite = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let observations = match suite.as_str() {
        "open" => vec![run_red_open()],
        "claim" => vec![run_red_claim()],
        "allocation" => vec![run_red_allocation()],
        "flow" => run_allocation_golden_flow(),
        "controls" => run_controls_suite(),
        "all" => {
            let mut observations = vec![run_red_open(), run_red_claim(), run_red_allocation()];
            observations.extend(run_allocation_golden_flow());
            observations.extend(run_controls_suite());
            observations
        }
        _ => panic!("suite must be open, claim, allocation, flow, controls or all"),
    };
    let report = serde_json::json!({
        "scope": "Compiled Musical Chairs financial SBF executed locally in LiteSVM with synthetic actors and injected production accounts; no deployment, provider, wallet, Squads, signing, broadcast, or live settlement proof",
        "program": musical_chairs_program::ID.to_string(),
        "runtime": {
            "litesvm": "0.7.1",
            "anchor": "0.32.1",
            "solana_sdk": "2.3.1",
            "legacy_packet_limit": 1232,
            "fee_model": "observed LiteSVM base fee; not a mainnet quote"
        },
        "observations": observations
    });
    let text = serde_json::to_string_pretty(&report).expect("financial report JSON");
    println!("{text}");
    if let Some(path) = std::env::var_os("MC_FINANCIAL_EVIDENCE_PATH") {
        std::fs::write(path, text).expect("financial evidence report");
    }
}
