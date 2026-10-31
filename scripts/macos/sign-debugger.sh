#!/bin/sh
# Ad-hoc sign a binary with the debugger entitlement so the macOS native
# debugger can obtain a target's Mach task port.
#
#   scripts/macos/sign-debugger.sh target/release/fission_cli
#
# Also needed, once per machine:  sudo DevToolsSecurity -enable
# (your user must be in the `_developer` group; check with `id -Gn`).
# Re-run after every rebuild: a rebuild replaces the signature.
set -eu
if [ "$#" -ne 1 ]; then
    echo "usage: $0 <binary>" >&2
    exit 2
fi
here="$(cd "$(dirname "$0")" && pwd)"
codesign -s - -f --entitlements "$here/debugger.entitlements.plist" "$1"
echo "signed $1 with com.apple.security.cs.debugger"
if ! DevToolsSecurity -status 2>/dev/null | grep -q enabled; then
    echo "note: Developer Tools access is off; run: sudo DevToolsSecurity -enable" >&2
fi
