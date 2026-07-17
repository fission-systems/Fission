#!/usr/bin/env bash
# ADR 0008 / 0011 owner-boundary smoke scan for fission-pcode.
# Fail on new cross-owner edges that should not reappear.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PCODE="$ROOT/crates/fission-pcode/src"
fail=0

check_absent() {
  local desc="$1"
  local path="$2"
  local pattern="$3"
  if rg -n --glob '*.rs' "$pattern" "$path" >/dev/null 2>&1; then
    echo "BOUNDARY FAIL: $desc"
    rg -n --glob '*.rs' "$pattern" "$path" || true
    fail=1
  else
    echo "BOUNDARY OK:   $desc"
  fi
}

echo "== fission-pcode owner boundary scan =="

# Render must not reach into normalize/structuring owners.
check_absent \
  "render must not import nir::normalize" \
  "$PCODE/render" \
  'crate::nir::normalize|nir::normalize::'

check_absent \
  "render must not import nir::structuring" \
  "$PCODE/render" \
  'crate::nir::structuring|nir::structuring::'

# Semantic owners must not call HIR presentation polish.
check_absent \
  "builder must not call apply_hir_presentation" \
  "$PCODE/nir/builder" \
  'apply_hir_presentation'

check_absent \
  "normalize must not call apply_hir_presentation" \
  "$PCODE/nir/normalize" \
  'apply_hir_presentation'

check_absent \
  "structuring must not call apply_hir_presentation" \
  "$PCODE/nir/structuring" \
  'apply_hir_presentation'

# Prefer crate::render over historical crate::nir::render for new code.
# (Compat alias may remain; this flag is informational only if matches are tests.)
if rg -n --glob '*.rs' 'crate::nir::render::' "$PCODE" >/dev/null 2>&1; then
  echo "BOUNDARY WARN: remaining crate::nir::render:: paths (prefer crate::render::):"
  rg -n --glob '*.rs' 'crate::nir::render::' "$PCODE" || true
else
  echo "BOUNDARY OK:   no crate::nir::render:: qualified paths"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "Owner boundary scan failed."
  exit 1
fi
echo "Owner boundary scan passed."
