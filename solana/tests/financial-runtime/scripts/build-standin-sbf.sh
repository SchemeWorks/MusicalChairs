#!/usr/bin/env bash
# Builds the TEST-ONLY Squads vault stand-in (squads-vault-standin) as an SBF
# artifact for LiteSVM tests. It is a test fixture that never ships.
#
# Unlike build-sbf.sh, this script has no source-set digest machinery: the
# stand-in is not a shipped program and carries no provenance receipt. It only
# prints the SHA-256 of the artifact, its source and its lockfile.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$RUNTIME_DIR/standin/squads-vault-standin"
MANIFEST="$CRATE_DIR/Cargo.toml"
SOURCE="$CRATE_DIR/src/lib.rs"
LOCKFILE="$CRATE_DIR/Cargo.lock"
ARTIFACT_DIR="${MC_STANDIN_OUTPUT_DIR:-$CRATE_DIR/artifacts}"
ARTIFACT="$ARTIFACT_DIR/squads_vault_standin.so"
KEYPAIR="$ARTIFACT_DIR/squads_vault_standin-keypair.json"

: "${MC_SBF_BUILDER:?set MC_SBF_BUILDER to the pinned cargo-build-sbf executable}"
: "${MC_SBF_SDK:?set MC_SBF_SDK to the pinned SBF SDK directory}"
: "${MC_PLATFORM_TOOLS:?set MC_PLATFORM_TOOLS to the pinned platform-tools directory}"

[[ -x "$MC_SBF_BUILDER" ]] || { echo "MC_SBF_BUILDER is not executable: $MC_SBF_BUILDER" >&2; exit 2; }
[[ -d "$MC_SBF_SDK" ]] || { echo "MC_SBF_SDK is not a directory: $MC_SBF_SDK" >&2; exit 2; }
[[ -d "$MC_PLATFORM_TOOLS" ]] || { echo "MC_PLATFORM_TOOLS is not a directory: $MC_PLATFORM_TOOLS" >&2; exit 2; }
[[ -x "$MC_PLATFORM_TOOLS/rust/bin/rustc" ]] || {
  echo "platform rustc is missing: $MC_PLATFORM_TOOLS/rust/bin/rustc" >&2
  exit 2
}
[[ -x "$MC_SBF_SDK/scripts/strip.sh" ]] || {
  echo "SBF strip helper is missing: $MC_SBF_SDK/scripts/strip.sh" >&2
  exit 2
}
[[ -f "$MANIFEST" ]] || { echo "stand-in manifest is missing: $MANIFEST" >&2; exit 2; }
[[ -f "$SOURCE" ]] || { echo "stand-in source is missing: $SOURCE" >&2; exit 2; }
[[ -f "$LOCKFILE" ]] || { echo "stand-in lockfile is missing: $LOCKFILE" >&2; exit 2; }

case "$("$MC_SBF_BUILDER" --version 2>&1)" in
  *"3.1.0-beta.0"*) ;;
  *) echo "MC_SBF_BUILDER must be cargo-build-sbf 3.1.0-beta.0" >&2; exit 2 ;;
esac
case "$("$MC_PLATFORM_TOOLS/rust/bin/rustc" --version 2>&1)" in
  "rustc 1.84."*) ;;
  *) echo "MC_PLATFORM_TOOLS must provide rustc 1.84.x" >&2; exit 2 ;;
esac

mkdir -p "$ARTIFACT_DIR"
if [[ -e "$KEYPAIR" || -L "$KEYPAIR" ]]; then
  echo "refusing to overwrite pre-existing generated keypair path: $KEYPAIR" >&2
  exit 2
fi

# The wrapper creates a random keypair beside the .so. Delete that file by
# name on every exit without opening or using it.
cleanup_keypairs() {
  if [[ -e "$KEYPAIR" || -L "$KEYPAIR" ]]; then
    rm -f -- "$KEYPAIR"
  fi
}
trap cleanup_keypairs EXIT

export PATH="$MC_PLATFORM_TOOLS/rust/bin:${PATH:-}"
export RUSTC="$MC_PLATFORM_TOOLS/rust/bin/rustc"
export CARGO_TARGET_DIR="${MC_STANDIN_TARGET_DIR:-/private/tmp/mc-solana-runtime-tools-20261013/standin-target}"

"$MC_SBF_BUILDER" \
  --manifest-path "$MANIFEST" \
  --sbf-sdk "$MC_SBF_SDK" \
  --skip-tools-install \
  --tools-version v1.48 \
  --no-rustup-override \
  --sbf-out-dir "$ARTIFACT_DIR" \
  -j 2 \
  -- --locked

[[ -f "$ARTIFACT" ]] || { echo "SBF artifact is missing: $ARTIFACT" >&2; exit 1; }
MAGIC="$(od -An -t x1 -N4 "$ARTIFACT" | tr -d '[:space:]')"
[[ "$MAGIC" == "7f454c46" ]] || { echo "unexpected artifact magic: $MAGIC" >&2; exit 1; }

ARTIFACT_SHA256="$(shasum -a 256 "$ARTIFACT" | awk '{print $1}')"
SOURCE_SHA256="$(shasum -a 256 "$SOURCE" | awk '{print $1}')"
LOCK_SHA256="$(shasum -a 256 "$LOCKFILE" | awk '{print $1}')"
printf 'standin_artifact=%s\nstandin_sha256=%s\nstandin_source_sha256=%s\nstandin_lock_sha256=%s\n' \
  "$ARTIFACT" "$ARTIFACT_SHA256" "$SOURCE_SHA256" "$LOCK_SHA256"
