#!/usr/bin/env bash
# Pass-Gate Lint: the structuring pass cap must stay declared. Called by the
# Pass-Gate Lint job in .github/workflows/ci.yml and by `cargo xtask preflight`.
set -euo pipefail
cd "$(dirname "$0")/../.."

MANAGER=crates/fission-pcode/src/midend/pass/manager.rs
if ! grep -q 'MAX_STRUCTURING_PASSES' "$MANAGER"; then
  echo "ERROR: MAX_STRUCTURING_PASSES constant missing from midend/pass/manager.rs"
  exit 1
fi
CAP=$(grep 'MAX_STRUCTURING_PASSES.*usize.*=' "$MANAGER" \
      | grep -oE '[0-9]+' | head -1)
echo "OK: MAX_STRUCTURING_PASSES = ${CAP} (current cap)."
