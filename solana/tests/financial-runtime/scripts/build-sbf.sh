#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$RUNTIME_DIR/../../programs/musical_chairs/Cargo.toml"
SOURCE="$RUNTIME_DIR/../../programs/musical_chairs/src/lib.rs"
LOCKFILE="$RUNTIME_DIR/../../programs/musical_chairs/Cargo.lock"
SOURCE_SET_HELPER="$SCRIPT_DIR/source-set-digest.sh"
ARTIFACT_DIR="${MC_SBF_OUTPUT_DIR:-$RUNTIME_DIR/../../programs/musical_chairs/artifacts}"
ARTIFACT="$ARTIFACT_DIR/musical_chairs_program.so"
KEYPAIR="$ARTIFACT_DIR/musical_chairs_program-keypair.json"

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
[[ -f "$MANIFEST" ]] || { echo "program manifest is missing: $MANIFEST" >&2; exit 2; }
[[ -f "$SOURCE" ]] || { echo "program source is missing: $SOURCE" >&2; exit 2; }
[[ -f "$LOCKFILE" ]] || { echo "program lockfile is missing: $LOCKFILE" >&2; exit 2; }
[[ -f "$SOURCE_SET_HELPER" && -x "$SOURCE_SET_HELPER" ]] || {
  echo "source-set digest helper is missing or not executable: $SOURCE_SET_HELPER" >&2
  exit 2
}

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
if [[ -n "${MC_SBF_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="$MC_SBF_TARGET_DIR"
fi

# Canonical source-set digest (spec DR-35). Sets SOURCE_SET_DIGEST to the 64-hex
# value the helper reports for the current tree. The set covers the workspace
# root and economics manifests, the program manifest, lockfile and cargo config,
# every src/**/*.rs in the program and in crates/economics, this wrapper, the
# helper itself and rust-toolchain.toml.
source_set_digest() {
  local output
  output="$("$SOURCE_SET_HELPER" "$RUNTIME_DIR/../..")"
  [[ "$output" =~ ^source_set_sha256=[0-9a-f]{64}$ ]] || {
    echo "unexpected source-set digest output: $output" >&2
    exit 1
  }
  SOURCE_SET_DIGEST="${output#source_set_sha256=}"
}

# Digest before the build. The artifact is attributed to this source set only if
# the same digest is observed again once cargo has finished.
source_set_digest
SOURCE_SET_BEFORE="$SOURCE_SET_DIGEST"

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
LOCK_SHA256="$(shasum -a 256 "$LOCKFILE" | awk '{print $1}')"

# Digest after the build. If any hashed input changed while cargo ran, the
# artifact may not come from the recorded source set: move it aside and print no
# receipt.
source_set_digest
SOURCE_SET_AFTER="$SOURCE_SET_DIGEST"
if [[ "$SOURCE_SET_AFTER" != "$SOURCE_SET_BEFORE" ]]; then
  echo "source set changed during build (before=$SOURCE_SET_BEFORE after=$SOURCE_SET_AFTER); provenance rejected" >&2
  mv -f -- "$ARTIFACT" "$ARTIFACT.rejected"
  exit 3
fi
printf 'sbf_artifact=%s\nartifact_sha256=%s\nsource_set_sha256=%s\nsource_set_sha256_before_build=%s\nlock_sha256=%s\n' \
  "$ARTIFACT" "$ARTIFACT_SHA256" "$SOURCE_SET_AFTER" "$SOURCE_SET_BEFORE" "$LOCK_SHA256"
