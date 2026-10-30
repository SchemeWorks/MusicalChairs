#!/usr/bin/env bash
# Canonical source-set digest (spec DR-35, plan M0 step 5, C5).
#
#   source_set_sha256 = SHA-256 over the concatenation, in byte-sorted order of
#   the path relative to the solana/ directory, of
#       path || 0x00 || sha256hex(file content) || 0x0A
#   for every file in the canonical set:
#       Cargo.toml                                  (workspace root)
#       crates/economics/Cargo.toml
#       crates/economics/src/**/*.rs
#       programs/musical_chairs/Cargo.toml
#       programs/musical_chairs/Cargo.lock
#       programs/musical_chairs/.cargo/config.toml
#       programs/musical_chairs/src/**/*.rs
#       tests/financial-runtime/scripts/build-sbf.sh
#       tests/financial-runtime/scripts/source-set-digest.sh
#       rust-toolchain.toml
#
# Why each file beyond the program's own sources is in the set:
#   - crates/economics/src/**/*.rs: the program depends on it by path, so its
#     code is compiled into the program ELF.
#   - crates/economics/Cargo.toml: it is the manifest of that path dependency and
#     fixes the dependency set it is compiled with.
#   - Cargo.toml (workspace root): crates/economics inherits edition,
#     rust-version and dependency versions from it, so a change there alters the
#     compiled crate even when its own files do not.
#   - tests/financial-runtime/scripts/source-set-digest.sh: this helper defines
#     the digest encoding, so changing it changes the value the receipt reports.
#
# Generated artifacts, target directories, crates/economics/tests/ (test-only,
# not a build input) and the harness' own manifests and lockfiles are
# deliberately not part of the set.
#
# Usage: source-set-digest.sh [--list] [SOLANA_DIR]
#   SOLANA_DIR  the repository solana/ directory (default: resolved from this
#               script's own location)
#   --list      also print one "<path> <sha256>" line per file before the digest
# Output: [<path> <sha256>]... then "source_set_sha256=<64 hex chars>".
set -euo pipefail

usage() {
  echo "usage: ${0##*/} [--list] [SOLANA_DIR]" >&2
  exit 2
}

LIST=0
SOLANA_DIR=""
for arg in "$@"; do
  case "$arg" in
    --list) LIST=1 ;;
    -*) usage ;;
    *)
      [[ -z "$SOLANA_DIR" ]] || usage
      SOLANA_DIR="$arg"
      ;;
  esac
done

if [[ -z "$SOLANA_DIR" ]]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
  SOLANA_DIR="$SCRIPT_DIR/../../.."
fi
[[ -d "$SOLANA_DIR" ]] || { echo "solana directory is missing: $SOLANA_DIR" >&2; exit 2; }
SOLANA_DIR="$(cd -- "$SOLANA_DIR" && pwd)"

if command -v shasum >/dev/null 2>&1; then
  sha256_stdin() { shasum -a 256 | awk '{print $1}'; }
  sha256_file() { shasum -a 256 -- "$1" | awk '{print $1}'; }
elif command -v sha256sum >/dev/null 2>&1; then
  sha256_stdin() { sha256sum | awk '{print $1}'; }
  sha256_file() { sha256sum -- "$1" | awk '{print $1}'; }
else
  echo "neither shasum nor sha256sum is available" >&2
  exit 2
fi

cd -- "$SOLANA_DIR"

FIXED_FILES=(
  "Cargo.toml"
  "crates/economics/Cargo.toml"
  "programs/musical_chairs/Cargo.toml"
  "programs/musical_chairs/Cargo.lock"
  "programs/musical_chairs/.cargo/config.toml"
  "tests/financial-runtime/scripts/build-sbf.sh"
  "tests/financial-runtime/scripts/source-set-digest.sh"
  "rust-toolchain.toml"
)
SRC_DIRS=(
  "programs/musical_chairs/src"
  "crates/economics/src"
)

for f in "${FIXED_FILES[@]}"; do
  [[ -f "$f" ]] || { echo "canonical source file is missing: $f" >&2; exit 2; }
done
for d in "${SRC_DIRS[@]}"; do
  [[ -d "$d" ]] || { echo "canonical source directory is missing: $d" >&2; exit 2; }
done

# Fixed files plus every .rs file under each source directory (symlinks included
# so they can be rejected below instead of being silently skipped), one path per
# line.
PATH_LIST="$(
  {
    printf '%s\n' "${FIXED_FILES[@]}"
    for d in "${SRC_DIRS[@]}"; do
      find "$d" \( -type f -o -type l \) -name '*.rs'
    done
  } | LC_ALL=C sort
)"

RECORDS=""
LISTING=""
while IFS= read -r rel; do
  [[ -n "$rel" ]] || continue
  [[ ! -L "$rel" ]] || { echo "canonical source file must not be a symlink: $rel" >&2; exit 2; }
  [[ -f "$rel" ]] || { echo "canonical source file is missing: $rel" >&2; exit 2; }
  hash="$(sha256_file "$rel")"
  [[ "$hash" =~ ^[0-9a-f]{64}$ ]] || { echo "unexpected hash for $rel: $hash" >&2; exit 2; }
  LISTING+="$rel $hash"$'\n'
  RECORDS+="$rel"$'\n'"$hash"$'\n'
done <<<"$PATH_LIST"

# Stream the exact records (path, NUL, hash, newline) into SHA-256. Bash
# variables cannot hold NUL bytes, so the records are re-emitted here.
DIGEST="$(
  while IFS= read -r rel && IFS= read -r hash; do
    printf '%s\0%s\n' "$rel" "$hash"
  done <<<"$RECORDS" | sha256_stdin
)"
[[ "$DIGEST" =~ ^[0-9a-f]{64}$ ]] || { echo "unexpected digest: $DIGEST" >&2; exit 2; }

if [[ "$LIST" -eq 1 ]]; then
  printf '%s' "$LISTING"
fi
printf 'source_set_sha256=%s\n' "$DIGEST"
