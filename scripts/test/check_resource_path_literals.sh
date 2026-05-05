#!/usr/bin/env bash
# Fail CI if repo-relative signatures paths leak outside centralized path_config.rs.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

ALLOW_FILE="crates/fission-core/src/core/path_config.rs"

fail=false

while IFS= read -r f; do
  [[ "$f" == "$ALLOW_FILE" ]] && continue
  echo "check_resource_path_literals: forbidden substring utils/signatures in $f"
  fail=true
done < <(rg -l 'utils/signatures' crates --glob '*.rs' || true)

while IFS= read -r f; do
  [[ "$f" == "$ALLOW_FILE" ]] && continue
  echo 'check_resource_path_literals: forbidden join("utils").join("signatures") in '"$f"
  fail=true
done < <(rg -l 'join\("utils"\)\.join\("signatures"\)' crates --glob '*.rs' || true)

if [[ "$fail" == true ]]; then
  exit 1
fi

echo "resource path literal guard: OK"
