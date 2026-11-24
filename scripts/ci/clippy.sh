#!/usr/bin/env bash
# Clippy exactly as the Lint & Format job runs it. Called by
# .github/workflows/reusable-lint-format.yml and by `cargo xtask preflight`,
# so the two cannot drift.
#
#   CRATES          affected packages, comma-separated; empty selects the workspace
#   EXCLUDE_CRATES  packages never linted, comma-separated
#
# Bash 3.2 compatible (macOS): array expansions use the ${a[@]+...} form.
set -euo pipefail

CRATES="${CRATES:-}"
EXCLUDE_CRATES="${EXCLUDE_CRATES:-}"

package_flags=()
exclude_flags=()
if [ -n "$CRATES" ]; then
  IFS=',' read -ra PACKAGES <<< "$CRATES"
  IFS=',' read -ra EX <<< "$EXCLUDE_CRATES"
  for p in "${PACKAGES[@]+"${PACKAGES[@]}"}"; do
    p="$(echo "$p" | tr -d '[:space:]')"
    [ -z "$p" ] && continue
    skip=false
    for c in "${EX[@]+"${EX[@]}"}"; do
      c="$(echo "$c" | tr -d '[:space:]')"
      [ "$p" = "$c" ] && skip=true
    done
    [ "$skip" = true ] || package_flags+=(-p "$p")
  done
  if [ "${#package_flags[@]}" -eq 0 ]; then
    echo "No selected packages remain after exclusions; skipping Clippy."
    exit 0
  fi
fi
if [ -n "$EXCLUDE_CRATES" ]; then
  IFS=',' read -ra EX <<< "$EXCLUDE_CRATES"
  for c in "${EX[@]+"${EX[@]}"}"; do
    c="$(echo "$c" | tr -d '[:space:]')"
    [ -z "$c" ] && continue
    exclude_flags+=(--exclude "$c")
  done
fi
if [ "${#package_flags[@]}" -gt 0 ]; then
  cmd=(cargo clippy "${package_flags[@]}" --all-targets --locked -- -D warnings -A clippy::style -A clippy::complexity)
else
  cmd=(cargo clippy --workspace "${exclude_flags[@]+"${exclude_flags[@]}"}" --all-targets --locked -- -D warnings -A clippy::style -A clippy::complexity)
fi
echo "+ ${cmd[*]}" >&2
exec "${cmd[@]}"
