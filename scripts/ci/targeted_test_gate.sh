#!/usr/bin/env bash
# "A PR touching NIR/render/midend Rust code must add a test" (AGENTS.md).
# Called by the Pass-Gate Lint job in .github/workflows/ci.yml and by
# `cargo xtask preflight`, so the rule has one definition.
#
#   targeted_test_gate.sh collect BASE_SHA
#       Compare BASE_SHA's tree with HEAD. Prints key=value lines on stdout
#       (CI appends them to $GITHUB_OUTPUT); diagnostics go to stderr.
#   targeted_test_gate.sh enforce
#       Reads NIR_RENDER_MIDEND_CHANGED and HAS_TARGETED_TEST from the
#       environment and fails when the change needs a test and has none.
#
# Bash 3.2 compatible (macOS).
set -euo pipefail

collect() {
  local BASE_SHA="$1"
  local CHANGED
  CHANGED=$(git diff --name-only "$BASE_SHA" HEAD)
  {
    echo "Changed paths relative to fetched base $BASE_SHA:"
    printf '%s\n' "$CHANGED"
  } >&2

  # Consume the complete input: grep -q can close the pipe early and
  # make git diff fail with SIGPIPE under pipefail despite a match.
  local NIR_RENDER_MIDEND_CHANGED=false
  local HAS_TARGETED_TEST=false
  local SLEIGH_CHANGED=false

  # Rust sources only: a manifest edit (a version bump, a dependency
  # drop) changes no NIR semantics, and counting it blocked release
  # PRs for want of a test there is nothing to write for.
  if printf '%s\n' "$CHANGED" | grep '\.rs$' | grep -e "^crates/fission-pcode/src/midend/" \
      -e "^crates/fission-pcode/src/render/" \
      -e "^crates/fission-midend-structuring/" \
      -e "^crates/fission-midend-normalize/" \
      -e "^crates/fission-midend-core/" >/dev/null; then
    NIR_RENDER_MIDEND_CHANGED=true
    # Lines of code the change adds there, blank lines and comments
    # aside. A change that only removes code (a duplicate definition,
    # dead code) has nothing new for a test to target.
    local ADDED_CODE
    ADDED_CODE=$(git diff --unified=0 "$BASE_SHA" HEAD -- \
        crates/fission-pcode/src/midend/ crates/fission-pcode/src/render/ \
        crates/fission-midend-structuring/ crates/fission-midend-normalize/ \
        crates/fission-midend-core/ \
        | grep '^+[^+]' | grep -cvE '^\+[[:space:]]*(//.*)?$' || true)
    if [ "$ADDED_CODE" -eq 0 ]; then
      echo "INFO: NIR/render/midend change adds no code; no targeted test required." >&2
      HAS_TARGETED_TEST=true
    elif printf '%s\n' "$CHANGED" | grep -e "tests/" -e ".*test" -e ".*_tests\.rs" >/dev/null; then
      HAS_TARGETED_TEST=true
    elif git diff "$BASE_SHA" HEAD -- \
        crates/fission-pcode/src/midend/ crates/fission-pcode/src/render/ \
        crates/fission-midend-structuring/ crates/fission-midend-normalize/ \
        crates/fission-midend-core/ \
        | grep "+.*#\[test\]" >/dev/null; then
      HAS_TARGETED_TEST=true
    fi
  fi

  if printf '%s\n' "$CHANGED" | grep -e "^crates/fission-sleigh/" -e "^utils/sleigh-specs/" >/dev/null; then
    SLEIGH_CHANGED=true
  fi

  echo "nir_render_midend_changed=$NIR_RENDER_MIDEND_CHANGED"
  echo "has_targeted_test=$HAS_TARGETED_TEST"
  echo "sleigh_changed=$SLEIGH_CHANGED"
}

enforce() {
  if [ "${NIR_RENDER_MIDEND_CHANGED:-}" = "true" ]; then
    if [ "${HAS_TARGETED_TEST:-}" = "true" ]; then
      echo "OK: Targeted tests included."
    else
      echo "ERROR: Targeted test missing for NIR/render/midend changes."
      exit 1
    fi
  else
    echo "INFO: No NIR/render/midend changes detected."
  fi
}

case "${1:-}" in
  collect)
    [ "$#" -eq 2 ] || { echo "usage: $0 collect BASE_SHA" >&2; exit 2; }
    collect "$2"
    ;;
  enforce)
    enforce
    ;;
  *)
    echo "usage: $0 {collect BASE_SHA|enforce}" >&2
    exit 2
    ;;
esac
