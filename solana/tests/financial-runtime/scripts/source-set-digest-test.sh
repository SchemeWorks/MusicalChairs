#!/usr/bin/env bash
# Self-test for source-set-digest.sh (spec DR-35, C5). It copies the files the
# helper lists under --list into a mktemp -d tree and mutates only that copy.
# It checks that the copy reproduces the real digest, that each class of hashed
# input changes it, that crates/economics/tests/ does not, and that a symlink in
# the set is rejected. It builds nothing.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
HELPER="$SCRIPT_DIR/source-set-digest.sh"
SOLANA_DIR="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"

TMP="$(mktemp -d)"
trap 'rm -rf -- "$TMP"' EXIT
COPY="$TMP/copy"

fail() {
  echo "FAIL $*" >&2
  exit 1
}

pass() {
  echo "PASS $1"
}

# digest_of <command...>: runs the command (its last argument is the solana
# directory) and prints the 64-hex source_set_sha256 that it reports.
digest_of() {
  local out value
  out="$("$@")" || return 1
  value="$(printf '%s\n' "$out" | sed -n 's/^source_set_sha256=//p')"
  [[ "$value" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s\n' "$value"
}

# Step 1: copy every file the helper lists into the scratch tree (cp only).
LIST="$TMP/list.txt"
"$HELPER" --list "$SOLANA_DIR" > "$LIST" || fail "copy_listed_set: helper --list failed on $SOLANA_DIR"
REAL="$(sed -n 's/^source_set_sha256=//p' "$LIST")"
[[ "$REAL" =~ ^[0-9a-f]{64}$ ]] || fail "copy_listed_set: --list output has no source_set_sha256 line"
grep -v '^source_set_sha256=' "$LIST" > "$TMP/files.txt"

mkdir -p -- "$COPY"
FILE_COUNT=0
while IFS= read -r line; do
  rel="${line% *}"
  sha="${line##* }"
  [[ -n "$rel" && "$sha" =~ ^[0-9a-f]{64}$ ]] || fail "copy_listed_set: malformed listing line: $line"
  mkdir -p -- "$COPY/$(dirname -- "$rel")"
  cp -- "$SOLANA_DIR/$rel" "$COPY/$rel"
  FILE_COUNT=$((FILE_COUNT + 1))
done < "$TMP/files.txt"
[[ "$FILE_COUNT" -gt 0 ]] || fail "copy_listed_set: the listed set is empty"
pass copy_listed_set

# Step 2: the copy must reproduce the real digest.
D0="$(digest_of "$HELPER" "$COPY")" || fail "baseline: helper failed on the copy"
[[ "$D0" == "$REAL" ]] || fail "baseline: copy digest $D0 != real tree digest $REAL"
pass baseline

# Step 3: a hashed economics source file changes the digest, and restoring it does too.
ECON_SRC="crates/economics/src/backers.rs"
echo "# mutation" >> "$COPY/$ECON_SRC"
D1="$(digest_of "$HELPER" "$COPY")" || fail "economics_src_mutation: helper failed after mutation"
[[ "$D1" != "$D0" ]] || fail "economics_src_mutation: editing $ECON_SRC did not change the digest"
cp -- "$SOLANA_DIR/$ECON_SRC" "$COPY/$ECON_SRC"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "economics_src_mutation: restoring $ECON_SRC did not restore the digest"
pass economics_src_mutation

# Step 4: the helper hashes itself. Mutating the copied helper changes the digest,
# and running that mutated copy gives the same value as the original helper on
# the same tree (the algorithm is unchanged; only the hashed bytes differ).
HELPER_REL="tests/financial-runtime/scripts/source-set-digest.sh"
echo "# mutation" >> "$COPY/$HELPER_REL"
D2="$(digest_of "$HELPER" "$COPY")" || fail "helper_self_mutation: original helper failed on the mutated copy"
[[ "$D2" != "$D0" ]] || fail "helper_self_mutation: editing the copied helper did not change the digest"
D2B="$(digest_of bash "$COPY/$HELPER_REL" "$COPY")" || fail "helper_self_mutation: mutated copied helper failed to run"
[[ "$D2B" == "$D2" ]] || fail "helper_self_mutation: mutated copied helper reports $D2B, original reports $D2"
cp -- "$SOLANA_DIR/$HELPER_REL" "$COPY/$HELPER_REL"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "helper_self_mutation: restoring the helper did not restore the digest"
pass helper_self_mutation

# Step 5: the workspace root manifest is hashed because crates/economics inherits from it.
echo "# mutation" >> "$COPY/Cargo.toml"
D3="$(digest_of "$HELPER" "$COPY")" || fail "workspace_root_mutation: helper failed after mutation"
[[ "$D3" != "$D0" ]] || fail "workspace_root_mutation: editing the workspace Cargo.toml did not change the digest"
cp -- "$SOLANA_DIR/Cargo.toml" "$COPY/Cargo.toml"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "workspace_root_mutation: restoring Cargo.toml did not restore the digest"
pass workspace_root_mutation

# Step 6: a new .rs file under crates/economics/src joins the set.
NEW_SRC="$COPY/crates/economics/src/zz_mutation.rs"
echo "// zz_mutation" > "$NEW_SRC"
D4="$(digest_of "$HELPER" "$COPY")" || fail "new_economics_src_file: helper failed after adding the file"
[[ "$D4" != "$D0" ]] || fail "new_economics_src_file: adding a .rs file under crates/economics/src did not change the digest"
rm -f -- "$NEW_SRC"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "new_economics_src_file: removing the added file did not restore the digest"
pass new_economics_src_file

# Step 7: crates/economics/tests/ is not a build input and must not change the digest.
TESTS_DIR="$COPY/crates/economics/tests"
mkdir -p -- "$TESTS_DIR"
echo "// zz_mutation" > "$TESTS_DIR/zz_mutation.rs"
D5="$(digest_of "$HELPER" "$COPY")" || fail "economics_tests_not_in_set: helper failed after adding the test file"
[[ "$D5" == "$D0" ]] || fail "economics_tests_not_in_set: a file under crates/economics/tests changed the digest"
rm -f -- "$TESTS_DIR/zz_mutation.rs"
rmdir -- "$TESTS_DIR"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "economics_tests_not_in_set: removing the test file did not restore the digest"
pass economics_tests_not_in_set

# Step 8: a symlink inside the set makes the helper exit non-zero for that reason.
LINK="$COPY/crates/economics/src/zz_link.rs"
ln -s backers.rs "$LINK"
ERR="$TMP/symlink.err"
if "$HELPER" "$COPY" > /dev/null 2> "$ERR"; then
  fail "symlink_rejected: helper accepted a symlink inside the canonical set"
fi
grep -q 'symlink' "$ERR" || fail "symlink_rejected: helper failed, but not for a symlink: $(cat "$ERR")"
rm -f -- "$LINK"
[[ "$(digest_of "$HELPER" "$COPY")" == "$D0" ]] || fail "symlink_rejected: removing the link did not restore the digest"
pass symlink_rejected

# Final: the real tree's digest, and the size of the set.
FINAL="$(digest_of "$HELPER" "$SOLANA_DIR")" || fail "final: helper failed on the real tree"
[[ "$FINAL" == "$REAL" ]] || fail "final: real tree digest moved during the test ($REAL -> $FINAL)"
echo "set_file_count=$FILE_COUNT"
echo "source_set_sha256=$FINAL"
echo "ALL PASS"
