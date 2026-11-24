#!/usr/bin/env bash
# Tests for a selected package set, exactly as the "Specific Crates" steps of
# .github/workflows/reusable-run-tests.yml run them. Also called by
# `cargo xtask preflight`, so the local and CI commands cannot drift.
#
#   test_crates.sh nextest   cargo nextest run for the packages
#   test_crates.sh doc       cargo test --doc (nextest does not run doctests)
#
#   CRATES    packages, comma-separated (required)
#   FEATURES  cargo features, may be empty
#   RETRIES   nextest --retries (nextest only)
#   PROFILE   debug or release
#
# macOS runners use Bash 3.2, where `set -u` rejects direct expansion of an
# empty array. Keep array expansions in the `${array[@]+...}` form.
set -euo pipefail

CRATES="${CRATES:-}"
FEATURES="${FEATURES:-}"
PROFILE="${PROFILE:-debug}"

package_flags=()
IFS=',' read -ra CRATE_ARRAY <<< "$CRATES"
for crate in "${CRATE_ARRAY[@]+"${CRATE_ARRAY[@]}"}"; do
  crate="$(echo "$crate" | tr -d '[:space:]')"
  [ -z "$crate" ] && continue
  package_flags+=(-p "$crate")
done
feature_flags=()
if [ -n "$FEATURES" ]; then
  feature_flags+=(--features "$FEATURES")
fi
profile_flags=()
if [ "$PROFILE" = "release" ]; then
  profile_flags+=(--release)
fi

case "${1:-}" in
  nextest)
    if [ "${#package_flags[@]}" -eq 0 ]; then
      echo "No crates specified"
      exit 1
    fi
    echo "Testing packages: ${package_flags[*]}"
    cmd=(cargo nextest run \
      "${package_flags[@]}" \
      "${feature_flags[@]+"${feature_flags[@]}"}" \
      --locked \
      --no-tests=fail \
      --no-fail-fast \
      --retries "${RETRIES:?RETRIES is required}" \
      "${profile_flags[@]+"${profile_flags[@]}"}")
    ;;
  doc)
    # A doctest that fails must fail the job; `|| true` here meant no
    # doctest could ever fail CI.
    cmd=(cargo test --doc "${package_flags[@]+"${package_flags[@]}"}" \
      "${feature_flags[@]+"${feature_flags[@]}"}" --locked \
      "${profile_flags[@]+"${profile_flags[@]}"}")
    ;;
  *)
    echo "usage: $0 {nextest|doc}" >&2
    exit 2
    ;;
esac
echo "+ ${cmd[*]}" >&2
exec "${cmd[@]}"
