#!/usr/bin/env bash
# Pass-Gate Lint: no function-name or absolute-address branches in the NIR
# pipeline (AGENTS.md core rule 2). Called by the Pass-Gate Lint job in
# .github/workflows/ci.yml and by `cargo xtask preflight`.
set -euo pipefail
cd "$(dirname "$0")/../.."

PATTERN_HITS=$(grep -rn \
  -e 'func_name\s*==' \
  -e 'func_name\.contains' \
  -e 'function_name.*==\|==.*function_name' \
  -e 'addr\s*==\s*0x[0-9a-fA-F]' \
  crates/fission-pcode/src/midend/structuring/ \
  crates/fission-pcode/src/midend/normalize/ \
  crates/fission-midend-structuring/src/ \
  crates/fission-midend-normalize/src/ 2>/dev/null || true)
if [ -n "$PATTERN_HITS" ]; then
  echo "ERROR: Function/address-specific pattern detected in NIR pipeline:"
  echo "$PATTERN_HITS"
  exit 1
fi
echo "OK: No function/address-specific patterns found in NIR pipeline."
