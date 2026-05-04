#!/usr/bin/env bash
# Fail if any workspace crate reintroduces a path dependency on the removed
# `fission-analysis` compatibility facade. Implementation owners: `fission-static`,
# `fission-core`, `fission-dynamic` (see README / PROJECT_MAP).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0
while IFS= read -r f; do
  if grep -qE '^[[:space:]]*fission-analysis[[:space:]]*=' "$f"; then
    echo "ERROR: forbidden dependency on fission-analysis in $f"
    failed=1
  fi
done < <(find "$root/crates" -maxdepth 2 -name Cargo.toml -print)
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
echo "OK: no fission-analysis dependency entries in crates/*/Cargo.toml"
