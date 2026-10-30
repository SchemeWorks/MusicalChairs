#!/usr/bin/env bash
# Regenerates the program IDL into a temporary file and diffs it against the
# committed programs/musical_chairs/idl/musical_chairs.json. Exit 0 when they
# are byte-identical, 1 when they differ (the committed IDL is stale), 2 on a
# generation failure. Intended for the pinned-tool CI job (plan M9) and for
# local use after any change to the program's instructions, accounts, events
# or errors. Requires CARGO_HOME like gen-idl.sh.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
solana_dir="$(cd "$script_dir/.." && pwd)"
committed="$solana_dir/programs/musical_chairs/idl/musical_chairs.json"
[[ -f "$committed" ]] || { echo "error: committed IDL missing: $committed" >&2; exit 2; }
tmp="$(mktemp -d)"; trap 'rm -rf -- "$tmp"' EXIT
"$script_dir/gen-idl.sh" "$solana_dir/programs/musical_chairs" "$tmp/musical_chairs.json" >"$tmp/gen.log" 2>&1 || { cat "$tmp/gen.log" >&2; echo "error: IDL generation failed" >&2; exit 2; }
if cmp -s "$committed" "$tmp/musical_chairs.json"; then
  echo "idl_check=clean sha256=$(shasum -a 256 "$committed" | awk '{print $1}')"
else
  echo "idl_check=STALE (committed IDL differs from a fresh generation)" >&2
  diff -u "$committed" "$tmp/musical_chairs.json" | head -60 >&2
  exit 1
fi
