#!/usr/bin/env bash
# Static guard for the PP-ledger 32-byte memo fix.
#
# Fails if any ledger memo in shenanigans/main.mo can bypass LedgerMemo.ledgerMemo
# (which keeps <= 32-byte memos byte-identical and SHA-256-hashes longer ones).
# The PP ledger traps (IC0503) on a memo above icrc1:max_memo_length = 32, so a
# single raw `Text.encodeUtf8(<long id>)` is enough to reintroduce the bug.
#
# Usage:
#   shenanigans/test/memo-static.test.sh               # checks shenanigans/main.mo
#   shenanigans/test/memo-static.test.sh path/main.mo  # checks another file
#   shenanigans/test/memo-static.test.sh --self-test   # proves the guard itself
#                                                      # fails on injected bypasses
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_TARGET="$HERE/../main.mo"

# Prints one line per violation to stdout; exit status = number of violations
# (capped at 125).
check_file() {
  local f="$1" n=0
  if [ ! -f "$f" ]; then echo "missing file: $f"; return 125; fi

  # Strip full-line comments so doc text mentioning memos/encodeUtf8 cannot
  # trip or mask the checks. Keep line numbers by blanking instead of deleting.
  local code
  code="$(awk '{ if ($0 ~ /^[[:space:]]*\/\//) print ""; else print $0 }' "$f")"

  # 1. The module must be imported.
  if ! grep -Eq '^import LedgerMemo "LedgerMemo";' <<< "$code"; then
    echo "main.mo does not import LedgerMemo"; n=$((n + 1))
  fi

  # 2. No raw Text.encodeUtf8 at all in main.mo (every encode must be inside LedgerMemo).
  while IFS= read -r hit; do
    echo "raw Text.encodeUtf8 outside LedgerMemo: $hit"; n=$((n + 1))
  done < <(printf '%s\n' "$code" | grep -n 'encodeUtf8' || true)

  # 3. Every `memo =` assignment either goes through LedgerMemo.ledgerMemo(...) or is a
  #    plain Text local (`let memo = "<text>" # ...`, the event text that is later
  #    handed to a helper which calls ledgerMemo). Anything else bypasses it.
  while IFS= read -r hit; do
    line="${hit#*:}"
    case "$line" in
      *"LedgerMemo.ledgerMemo("*) ;;
      *"let memo = \""*) ;;
      *) echo "memo assignment bypasses ledgerMemo: $hit"; n=$((n + 1)) ;;
    esac
  done < <(printf '%s\n' "$code" | grep -nE '(^|[^A-Za-z0-9_])memo[[:space:]]*=' || true)

  # 4. No record-punned `memo;` field (it would hide a Blob built elsewhere).
  while IFS= read -r hit; do
    echo "punned memo field bypasses ledgerMemo: $hit"; n=$((n + 1))
  done < <(printf '%s\n' "$code" | grep -nE '^[[:space:]]*memo;[[:space:]]*$' || true)

  # 5. Every ledger call carries exactly one ledgerMemo memo field.
  local calls memos
  calls="$(printf '%s\n' "$code" | grep -cE 'ppLedger\.(icrc1_transfer|icrc2_transfer_from|icrc2_approve)\(' || true)"
  memos="$(printf '%s\n' "$code" | grep -cE 'memo = \?LedgerMemo\.ledgerMemo\(' || true)"
  if [ "$calls" != "$memos" ]; then
    echo "ledger call count ($calls) != ledgerMemo memo-field count ($memos)"; n=$((n + 1))
  fi
  if [ "$calls" -lt 1 ]; then echo "no ledger calls found (pattern drift?)"; n=$((n + 1)); fi

  [ "$n" -gt 125 ] && n=125
  return "$n"
}

if [ "${1:-}" = "--self-test" ]; then
  fail=0
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  src="$DEFAULT_TARGET"

  # Baseline must be clean.
  if ! out="$(check_file "$src")"; then echo "SELF-TEST FAIL: baseline main.mo is not clean:"; echo "$out"; exit 1; fi
  echo "  ok  baseline main.mo is clean"

  expect_violation() {  # name, sed-expression
    local name="$1" expr="$2" mutated="$tmp/mut.mo"
    sed -E "$expr" "$src" > "$mutated"
    if cmp -s "$src" "$mutated"; then echo "SELF-TEST FAIL: mutation '$name' did not change the file"; fail=1; return; fi
    if check_file "$mutated" > "$tmp/out.txt"; then
      echo "SELF-TEST FAIL: mutation '$name' was NOT detected"; fail=1
    else
      echo "  ok  mutation detected: $name ($(wc -l < "$tmp/out.txt" | tr -d ' ') violation line(s))"
    fi
  }
  expect_violation "chip-deposit memo bypass" 's/memo = \?LedgerMemo\.ledgerMemo\("chip-deposit"\);/memo = ?Text.encodeUtf8("chip-deposit");/'
  expect_violation "mint/burn/transfer helper memo bypass" 's/memo = \?LedgerMemo\.ledgerMemo\(memoText\);/memo = ?Text.encodeUtf8(memoText);/'
  expect_violation "kickup-residual memo bypass" 's/LedgerMemo\.ledgerMemo\("kickup-residual-" # eventId\)/Text.encodeUtf8("kickup-residual-" # eventId)/'
  expect_violation "punned memo field" 's/^([[:space:]]*)memo = \?LedgerMemo\.ledgerMemo\(memoText\);$/\1memo;/'
  expect_violation "import removed" '/^import LedgerMemo "LedgerMemo";$/d'
  expect_violation "memo = null on a ledger call" 's/memo = \?LedgerMemo\.ledgerMemo\("cash-out-" # Nat\.toText\(id\)\);/memo = null;/'
  [ "$fail" -eq 0 ] && echo "SELF-TEST PASSED" || { echo "SELF-TEST FAILED"; exit 1; }
  exit 0
fi

target="${1:-$DEFAULT_TARGET}"
if out="$(check_file "$target")"; then
  calls="$(awk '{ if ($0 !~ /^[[:space:]]*\/\//) print $0 }' "$target" | grep -cE 'ppLedger\.(icrc1_transfer|icrc2_transfer_from|icrc2_approve)\(')"
  echo "PASS memo-static: $calls ledger calls, every memo goes through LedgerMemo.ledgerMemo, no raw encodeUtf8 ($target)"
  exit 0
else
  echo "FAIL memo-static ($target):"
  echo "$out"
  exit 1
fi
