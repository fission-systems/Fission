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
"${cmd[@]}"

# Optional features no default build compiles. Each runs when its package is
# linted at all (workspace mode, or selected and not excluded).
lints_package() {
  local pkg="$1" flag
  if [ "${#package_flags[@]}" -eq 0 ]; then
    for flag in "${exclude_flags[@]+"${exclude_flags[@]}"}"; do
      [ "$flag" = "$pkg" ] && return 1
    done
    return 0
  fi
  for flag in "${package_flags[@]}"; do
    [ "$flag" = "$pkg" ] && return 0
  done
  return 1
}
if lints_package fission-cli; then
  cmd=(cargo clippy -p fission-cli --features sentry --all-targets --locked -- -D warnings -A clippy::style -A clippy::complexity)
  echo "+ ${cmd[*]}" >&2
  "${cmd[@]}"
fi
