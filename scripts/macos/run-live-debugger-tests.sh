#!/bin/sh
# Run the macOS live debugger tests: build the test binary, sign a copy with the
# debugger entitlement, and run it against real processes it launches.
# Needs `sudo DevToolsSecurity -enable` once (see sign-debugger.sh).
set -eu
cd "$(dirname "$0")/../.."
out="$(cargo test -p fission-dynamic --features interactive_runtime --test macos_live --no-run 2>&1)"
bin="$(printf '%s\n' "$out" | sed -n 's/.*(\(target[^)]*macos_live-[^)]*\)).*/\1/p' | tail -1)"
[ -n "$bin" ] || { echo "could not find the macos_live test binary" >&2; exit 1; }
signed="$(mktemp -d)/macos_live"
cp "$bin" "$signed"
scripts/macos/sign-debugger.sh "$signed"
FISSION_MACOS_LIVE_DEBUG_TESTS=1 "$signed" --test-threads=1 "$@"
