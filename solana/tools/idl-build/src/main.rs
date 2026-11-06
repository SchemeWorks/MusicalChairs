//! Generate an Anchor-format IDL for a Solana program crate without the
//! `anchor` CLI.
//!
//! The IDL is Solana's analogue of a Candid `.did` file: a JSON description of
//! a program's address, instructions, account layouts, types, events and error
//! codes, which clients use to encode calls and decode accounts and logs.
//!
//! Usage: musical-chairs-idl-build <program-crate-dir> <out-json-path>

use std::{env, fs, path::PathBuf, process::ExitCode};

use anchor_lang_idl::build::IdlBuilder;
use anyhow::{bail, Context, Result};

fn build_program_idl(program_dir: PathBuf) -> Result<anchor_lang_idl::types::Idl> {
    IdlBuilder::new()
        .program_path(program_dir)
        .resolution(true)
        .skip_lint(false)
        .no_docs(false)
        // The nested `cargo test` must fail on a stale program Cargo.lock, not rewrite it.
        .cargo_args(vec!["--locked".to_owned()])
        .build()
}

fn run() -> Result<PathBuf> {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let [program_dir, out_path] = args.as_slice() else {
        bail!("usage: musical-chairs-idl-build <program-crate-dir> <out-json-path>");
    };

    // Toolchain for the nested `cargo test __anchor_private_print_idl` run.
    // Why: anchor-syn 0.32's IDL build compiles with RUSTFLAGS
    // `--cfg procmacro2_semver_exempt` (set by IdlBuilder), which makes proc-macro2 drop
    // `wrap_proc_macro`, hiding the `Span::unwrap` that anchor-syn calls in idl/defined.rs.
    // On stable, proc-macro2 only brings that path back when RUSTC_BOOTSTRAP is set (its
    // proc_macro_span probe). Upstream defaults to nightly for the same reason. No new
    // toolchain is installed here: the rustup `nightly` is 1.72.0-nightly (2023), too old
    // for this dependency graph. So the nested build stays on the pinned 1.92.0 with
    // RUSTC_BOOTSTRAP=1. Set MC_IDL_NESTED_TOOLCHAIN to a current nightly to drop the hack.
    let nested = env::var("MC_IDL_NESTED_TOOLCHAIN").unwrap_or_else(|_| "1.92.0".to_owned());
    env::set_var("RUSTUP_TOOLCHAIN", nested);
    env::set_var("RUSTC_BOOTSTRAP", "1");

    // The nested build runs with the program dir as cwd, so the path must be absolute.
    let program_dir = fs::canonicalize(program_dir)
        .with_context(|| format!("resolving program dir {}", program_dir.display()))?;
    if !program_dir.join("Cargo.toml").is_file() {
        bail!("{} has no Cargo.toml", program_dir.display());
    }

    let idl = build_program_idl(program_dir.clone())
        .with_context(|| format!("building IDL for {}", program_dir.display()))?;

    let mut json = serde_json::to_string_pretty(&idl).context("serializing IDL to JSON")?;
    json.push('\n');

    if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(out_path, json).with_context(|| format!("writing {}", out_path.display()))?;

    Ok(out_path.clone())
}

fn main() -> ExitCode {
    match run() {
        Ok(path) => {
            println!("idl_path={}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
