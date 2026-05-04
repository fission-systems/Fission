#!/usr/bin/env bash
# Fail if canonical signature/type data is reintroduced under the crate-local tree.
# Single source of truth: utils/signatures/ (see utils/MANIFEST.md).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
base="$root/crates/fission-signatures/data"
failed=0
for sub in win_api win_types signatures; do
  if [[ -d "$base/$sub" ]]; then
    echo "ERROR: forbidden canonical data directory exists: crates/fission-signatures/data/$sub"
    echo "       Move resources to utils/signatures/ or use tests/fixtures only."
    failed=1
  fi
done
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
echo "OK: no forbidden crates/fission-signatures/data/{win_api,win_types,signatures} directories"
