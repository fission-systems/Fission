#!/usr/bin/env bash
# Optional-feature suites, exactly as .github/workflows/reusable-run-tests.yml
# runs them when the feature owner is affected. Also called by
# `cargo xtask preflight`.
#
#   feature_suite.sh interactive_runtime|script_emulator|emulator_softfloat|plugin_interactive|cli_alt
set -euo pipefail

run() {
  echo "+ $*" >&2
  "$@"
}

case "${1:-}" in
  interactive_runtime)
    run cargo nextest run --locked --no-tests=fail -p fission-dynamic --features interactive_runtime
    ;;
  script_emulator)
    run cargo nextest run --locked --no-tests=fail -p fission-script --features emulator
    ;;
  emulator_softfloat)
    run cargo check -p fission-emulator --features softfloat --all-targets --locked
    ;;
  plugin_interactive)
    run cargo check -p fission-plugin --features interactive_runtime --all-targets --locked
    ;;
  cli_alt)
    run cargo check -p fission-cli --no-default-features --locked
    run cargo check -p fission-cli --no-default-features --features allocator-jemallocator --locked
    ;;
  *)
    echo "usage: $0 {interactive_runtime|script_emulator|emulator_softfloat|plugin_interactive|cli_alt}" >&2
    exit 2
    ;;
esac
