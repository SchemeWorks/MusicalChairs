#!/usr/bin/env bash
# Generates the Anchor-format IDL for the native Solana program without the
# Anchor CLI. The IDL is Solana's analogue of a Candid .did file: it describes
# the program address, instructions, account layouts, types, events and error
# codes that clients use to encode calls and decode accounts and logs.
#
# Runs solana/tools/idl-build (pinned anchor-lang-idl 0.1.2, Rust 1.92.0), which
# calls anchor_lang_idl::build::IdlBuilder. That runs `cargo test
# __anchor_private_print_idl --features idl-build --locked` inside the program
# crate, so the program must declare `idl-build = ["anchor-lang/idl-build"]`.
# A stale program Cargo.lock fails instead of being rewritten. The nested build
# runs on 1.92.0 with RUSTC_BOOTSTRAP=1 (see tools/idl-build/src/main.rs).
# Prints idl_sha256 of the written JSON.
#
# Usage: gen-idl.sh [PROGRAM_DIR] [OUT_JSON]
# Requires CARGO_HOME. MC_IDL_TOOL_TARGET_DIR overrides the shared target dir.
set -euo pipefail

if [[ -z "${CARGO_HOME:-}" ]]; then
  echo "error: CARGO_HOME must be set (pinned registry cache)" >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
solana_dir="$(cd "$script_dir/.." && pwd)"
tool_dir="$solana_dir/tools/idl-build"

program_dir="${1:-$solana_dir/programs/musical_chairs}"
out_json="${2:-$solana_dir/programs/musical_chairs/idl/musical_chairs.json}"

if [[ ! -d "$program_dir" ]]; then
  echo "error: no such program dir: $program_dir" >&2
  exit 1
fi
program_dir="$(cd "$program_dir" && pwd)"
mkdir -p "$(dirname "$out_json")"
out_json="$(cd "$(dirname "$out_json")" && pwd)/$(basename "$out_json")"

# Shared by the tool build and the nested program test build that IdlBuilder spawns.
export CARGO_TARGET_DIR="${MC_IDL_TOOL_TARGET_DIR:-/private/tmp/mc-solana-runtime-tools-20261013/idl-tool-target}"

cargo +1.92.0 run --locked --manifest-path "$tool_dir/Cargo.toml" -- "$program_dir" "$out_json"

echo "idl_sha256=$(shasum -a 256 "$out_json" | awk '{print $1}')"
