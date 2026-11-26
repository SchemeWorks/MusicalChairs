//! `mc-keeper`: the fee-only keeper worker CLI.
//!
//! ```text
//! mc-keeper once|run --rpc <url> --fee-payer <keypair.json>
//!                    [--journal <path>] [--metrics-json <path>] [--interval <seconds>]
//!                    [--dry-run] [--max-rows <1..8>] [--skip-newly-capped]
//!                    [--preprovision-close-record]
//!                    [--max-provider-topup-lamports <n>] [--min-reconcile-lamports <n>]
//! ```
//!
//! `--max-provider-topup-lamports` (default 0 = never) lets the worker call `increase_provider_budget`, but only
//! when a draw request would be refused for want of reserve and the whole shortfall fits the maximum.  The extra
//! is FREE SPONSOR operations funds (`ops.free_balance`) moved into the close's provider budget: this flag is a
//! sponsor budget decision, not a payment from the fee key (the fee key pays only transaction fees).
//! `--min-reconcile-lamports` (default 0 = any difference) is a dust threshold: the OpsCash / ORAO Client books are
//! reconciled only for a balance change of at least that many lamports, so a 1-lamport donation does not cost a
//! fee.  Cleanup (`close_shared_account`) covers the last finished close only; older closes' leftovers need a wallet.
//!
//! The fee payer is ANY ed25519 keypair file: it pays network fees (and the
//! refundable rent of an opt-in `provision_close_record`) and is never an
//! authority of the program.  There is no leader election and no lease: run as
//! many workers as you like; a worker that loses a race fails fee-only.
//! `--journal` is required unless `--dry-run` (the journal is what keeps an
//! unknown send outcome from being resubmitted).  One JSON line per pass goes
//! to stdout; `--metrics-json` also rewrites a metrics document each pass.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use musical_chairs_keeper::{
    journal::{FileSink, Journal, MemSink},
    metrics::Progress,
    plan::{Limits, MAX_BATCH_ROWS},
    rpc::RpcClientBackend,
    runner::Worker,
};
use solana_sdk::signature::{read_keypair_file, Keypair};

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Once,
    Run,
}

#[derive(Debug)]
struct Cli {
    mode: Mode,
    rpc: String,
    fee_payer: PathBuf,
    journal: Option<PathBuf>,
    metrics_json: Option<PathBuf>,
    interval_secs: u64,
    dry_run: bool,
    limits: Limits,
}

const USAGE: &str = "usage: mc-keeper once|run --rpc <url> --fee-payer <keypair.json> [--journal <path>] \
[--metrics-json <path>] [--interval <seconds>] [--dry-run] [--max-rows <1..8>] [--skip-newly-capped] \
[--preprovision-close-record] [--max-provider-topup-lamports <n>] [--min-reconcile-lamports <n>]
  --max-provider-topup-lamports <n>  default 0 = never. Let the worker call increase_provider_budget when a draw
                                     request would be refused for want of reserve and the whole shortfall is at most
                                     <n>. The extra is FREE SPONSOR operations funds (ops.free_balance) moved into the
                                     close's provider budget: a sponsor budget decision, not a fee-key payment.
  --min-reconcile-lamports <n>       default 0 = any difference. Dust threshold: reconcile the OpsCash / ORAO Client
                                     books only for a balance change of at least <n> lamports.";

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut iter = args.iter();
    let mode = match iter.next().map(String::as_str) {
        Some("once") => Mode::Once,
        Some("run") => Mode::Run,
        _ => return Err(USAGE.to_string()),
    };
    let (mut rpc, mut fee_payer, mut journal, mut metrics_json) = (None, None, None, None);
    let mut interval_secs = 10;
    let mut dry_run = false;
    let mut limits = Limits::default();
    while let Some(flag) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--rpc" => rpc = Some(value("--rpc")?),
            "--fee-payer" => fee_payer = Some(PathBuf::from(value("--fee-payer")?)),
            "--journal" => journal = Some(PathBuf::from(value("--journal")?)),
            "--metrics-json" => metrics_json = Some(PathBuf::from(value("--metrics-json")?)),
            "--interval" => {
                interval_secs = value("--interval")?
                    .parse()
                    .map_err(|_| "--interval must be whole seconds".to_string())?;
                if interval_secs == 0 {
                    return Err("--interval must be at least 1 second".to_string());
                }
            }
            "--max-rows" => {
                let rows: u8 = value("--max-rows")?
                    .parse()
                    .map_err(|_| "--max-rows must be a number".to_string())?;
                if !(1..=MAX_BATCH_ROWS).contains(&rows) {
                    return Err(format!("--max-rows must be in 1..={MAX_BATCH_ROWS}"));
                }
                limits.max_rows = rows;
            }
            "--dry-run" => dry_run = true,
            "--skip-newly-capped" => limits.skip_newly_capped = true,
            "--preprovision-close-record" => limits.preprovision_close_record = true,
            "--max-provider-topup-lamports" => {
                limits.max_provider_topup_lamports = value("--max-provider-topup-lamports")?
                    .parse()
                    .map_err(|_| {
                        "--max-provider-topup-lamports must be whole lamports".to_string()
                    })?;
            }
            "--min-reconcile-lamports" => {
                limits.min_reconcile_lamports = value("--min-reconcile-lamports")?
                    .parse()
                    .map_err(|_| "--min-reconcile-lamports must be whole lamports".to_string())?;
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let rpc = rpc.ok_or_else(|| format!("--rpc is required\n{USAGE}"))?;
    let fee_payer = fee_payer.ok_or_else(|| format!("--fee-payer is required\n{USAGE}"))?;
    if journal.is_none() && !dry_run {
        return Err(format!("--journal is required unless --dry-run (it is what stops an unknown outcome being resubmitted)\n{USAGE}"));
    }
    Ok(Cli {
        mode,
        rpc,
        fee_payer,
        journal,
        metrics_json,
        interval_secs,
        dry_run,
        limits,
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Replace the metrics file atomically (write a sibling, then rename).
fn write_metrics(path: &Path, json: &str) -> Result<(), String> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    fs::write(&temp, json).map_err(|e| format!("write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| format!("rename to {}: {e}", path.display()))
}

fn run_cli(args: &[String]) -> Result<(), String> {
    let cli = parse_args(args)?;
    let fee_payer: Keypair = read_keypair_file(&cli.fee_payer)
        .map_err(|e| format!("cannot read keypair {}: {e}", cli.fee_payer.display()))?;
    let sink: Box<dyn musical_chairs_keeper::journal::JournalSink> = match &cli.journal {
        Some(path) => Box::new(FileSink::new(path.clone())),
        None => Box::new(MemSink::new()),
    };
    let journal = Journal::open(sink).map_err(|e| e.to_string())?;
    let chain = RpcClientBackend::new(&cli.rpc);
    let mut worker = Worker::new(&chain, &fee_payer, journal, cli.limits).with_dry_run(cli.dry_run);
    if let Some(path) = &cli.metrics_json {
        if let Some(progress) = fs::read_to_string(path)
            .ok()
            .and_then(|text| Progress::from_json(&text))
        {
            worker = worker.with_progress(progress);
        }
    }

    loop {
        let now = unix_now();
        let result = worker.once(now);
        match &result {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string(report).map_err(|e| e.to_string())?
                );
                if let Some(path) = &cli.metrics_json {
                    write_metrics(path, &worker.metrics(now, report).to_json())?;
                }
            }
            Err(error) if cli.mode == Mode::Run => eprintln!("mc-keeper: pass failed: {error}"),
            Err(error) => return Err(error.to_string()),
        }
        if cli.mode == Mode::Once {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(cli.interval_secs));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(message) = run_cli(&args) {
        eprintln!("mc-keeper: {message}");
        std::process::exit(2);
    }
}
