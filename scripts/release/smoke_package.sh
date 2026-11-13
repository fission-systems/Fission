#!/usr/bin/env bash
# Prove a release archive works the way a user runs it: unpacked somewhere
# that is not a source checkout, invoked from an unrelated directory, with no
# FISSION_* environment. The binary must find its own resources and decompile.
#
#   smoke_package.sh <archive> <fixture-binary>
#
# This is the check the flat v0.2.x layout would have failed: it printed
# "no Ghidra language definitions were loaded" everywhere except inside a
# checkout, and no release step ever ran it from outside one.

set -euo pipefail

archive=$1
fixture=$2
[ -f "$archive" ] || { echo "smoke: archive not found: $archive" >&2; exit 1; }
[ -f "$fixture" ] || { echo "smoke: fixture not found: $fixture" >&2; exit 1; }
archive=$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")
fixture=$(cd "$(dirname "$fixture")" && pwd)/$(basename "$fixture")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/unpack" "$work/cwd" "$work/home"

case "$archive" in
  *.zip)
    if command -v 7z >/dev/null 2>&1; then
      7z x -bd -o"$work/unpack" "$archive" >/dev/null
    else
      unzip -q "$archive" -d "$work/unpack"
    fi
    ;;
  *) tar -C "$work/unpack" -xzf "$archive" ;;
esac

root=$(find "$work/unpack" -mindepth 1 -maxdepth 1 -type d | head -1)
cli="$root/bin/fission_cli"
[ -f "$cli.exe" ] && cli="$cli.exe"
[ -x "$cli" ] || { echo "smoke: no bin/fission_cli in $archive" >&2; exit 1; }
for d in sleigh-specs signatures ghidra-data; do
  [ -d "$root/share/fission/$d" ] || { echo "smoke: share/fission/$d missing" >&2; exit 1; }
done
cp "$fixture" "$work/cwd/"

run() {
  (cd "$work/cwd" && env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT \
    -u FISSION_SLEIGH_SPEC_DIR -u FISSION_GHIDRA_DATA_DIR -u FISSION_SLA_DIR \
    -u XDG_DATA_HOME HOME="$work/home" "$cli" "$@")
}

run --version
status=$(run resources status --json)
# JSON escapes Windows separators as `\\`, hence `+`.
echo "$status" | grep -Eq '"signatures_base": *"[^"]*share[/\\]+fission[/\\]+signatures"' || {
  echo "smoke: signatures did not resolve to the archive's share/fission" >&2
  echo "$status" >&2
  exit 1
}

out=$(run decomp "$(basename "$fixture")" --all)
echo "$out" | head -12
echo "$out" | grep -q "Function:" || { echo "smoke: decomp produced no function" >&2; exit 1; }
echo "smoke: ok ($(echo "$out" | grep -c 'Function:') functions from $(basename "$fixture"))"
