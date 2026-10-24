//! M14-TOOL (packet RG-4): the devnet operator CLI.  All behaviour, safety interlocks and docs live in
//! `musical_chairs_financial_runtime::devnet_ops`; see `docs/solana/devnet-ops.md` for the runbook.
//!
//! Exit codes: 0 ok, 1 failure, 2 usage, 3 refused by a safety interlock, 4 a verification or
//! read-back check did not hold (the JSON receipt is still printed on stdout).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(musical_chairs_financial_runtime::devnet_ops::run_cli(&args));
}
