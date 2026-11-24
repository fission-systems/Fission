#!/usr/bin/env bash
# ADR 0008 / 0011 / 0012 owner-boundary smoke scan for fission-pcode and the
# extracted midend crates. Fail on new cross-owner edges that should not reappear.
#
# Fail-closed: a scanned directory that does not exist, or a search that cannot
# run (no rg/grep, bad pattern), is an error, never "OK". Before 2026-11-24 the
# scan treated both as "no match": `crates/fission-pcode/src/midend/normalize`
# no longer exists (ADR 0012 moved it), and CI runners without `rg` passed every
# check without searching anything.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PCODE="$ROOT/crates/fission-pcode/src"
NORMALIZE="$ROOT/crates/fission-midend-normalize/src"
STRUCTURING="$ROOT/crates/fission-midend-structuring/src"
fail=0

# search PATTERN PATH...: print matches; exit status 0 = match, 1 = none,
# anything else = the search itself failed.
search() {
  local pattern="$1"
  shift
  local path
  for path in "$@"; do
    if [[ ! -d "$path" ]]; then
      echo "BOUNDARY ERROR: scanned directory does not exist: ${path#"$ROOT"/}" >&2
      return 2
    fi
  done
  if command -v rg >/dev/null 2>&1; then
    rg -n --glob '*.rs' -e "$pattern" "$@"
  else
    grep -rnE --include='*.rs' -e "$pattern" "$@"
  fi
}

check_absent() {
  local desc="$1"
  local pattern="$2"
  shift 2
  local out rc
  set +e
  out="$(search "$pattern" "$@")"
  rc=$?
  set -e
  case "$rc" in
    0)
      echo "BOUNDARY FAIL: $desc"
      echo "$out"
      fail=1
      ;;
    1) echo "BOUNDARY OK:   $desc" ;;
    *)
      echo "BOUNDARY ERROR: could not scan for: $desc (status $rc)"
      exit 2
      ;;
  esac
}

echo "== owner boundary scan =="

# Render must not reach into normalize/structuring owners: the extracted crates,
# or the legacy in-crate module paths.
check_absent \
  "render must not import normalize" \
  'crate::midend::normalize|crate::nir::normalize|midend::normalize::|nir::normalize::|fission_midend_normalize' \
  "$PCODE/render"

check_absent \
  "render must not import structuring" \
  'crate::midend::structuring|crate::nir::structuring|midend::structuring::|nir::structuring::|fission_midend_structuring' \
  "$PCODE/render"

# Semantic owners must not call HIR presentation polish.
check_absent \
  "builder must not call apply_hir_presentation" \
  'apply_hir_presentation' \
  "$PCODE/midend/builder"

check_absent \
  "normalize must not call apply_hir_presentation" \
  'apply_hir_presentation' \
  "$NORMALIZE"

check_absent \
  "structuring must not call apply_hir_presentation" \
  'apply_hir_presentation' \
  "$PCODE/midend/structuring" "$STRUCTURING"

# Prefer crate::render over historical module paths for print helpers (warning only).
set +e
out="$(search 'crate::nir::render::|crate::midend::render::' "$PCODE")"
rc=$?
set -e
case "$rc" in
  0)
    echo "BOUNDARY WARN: remaining *.render:: qualified paths (prefer crate::render::):"
    echo "$out"
    ;;
  1) echo "BOUNDARY OK:   no historical render module paths" ;;
  *)
    echo "BOUNDARY ERROR: could not scan for historical render paths (status $rc)"
    exit 2
    ;;
esac

if [[ "$fail" -ne 0 ]]; then
  echo "Owner boundary scan failed."
  exit 1
fi
echo "Owner boundary scan passed."
