#!/usr/bin/env bash
# Package a built fission_cli into the release archive layout, or package the
# platform-independent resource bundle.
#
#   package.sh binary <target> <path/to/fission_cli[.exe]> <out_dir>
#   package.sh resources <out_dir>
#
# Every archive uses the install-prefix layout the binary already resolves
# without configuration (`exe_parent/../share/fission`, see
# crates/fission-core/src/core/resource_roots.rs):
#
#   fission-<target>/
#     bin/fission_cli[.exe]
#     share/fission/{sleigh-specs,signatures,ghidra-data,fission-resources.json}
#     LICENSE NOTICE README.md
#
# `fission-resources.json` names the release the resources were cut from (the
# Fission version, FISSION_BUILD_TAG, FISSION_BUILD_SHA: the same variables the
# release build embeds in the binary) and digests each directory. `fission_cli
# resources status` compares it with the binary (#1208).
#
# The earlier flat layout (fission_cli next to utils/) matched no candidate,
# so an unpacked release could not find its own Sleigh specs unless it was run
# from inside a source checkout. Homebrew, install.sh and `cargo install` all
# install into a prefix, so one layout serves every channel.
#
# The resource bundle (`fission-resources.tar.gz`) holds the share/fission
# contents at its top level; it is what a `cargo install` user unpacks into
# `~/.cargo/share/fission`.
#
# Run from the repository root. Writes <name>.<ext> and <name>.<ext>.sha256.

set -euo pipefail

# macOS tar would otherwise add AppleDouble `._*` entries for extended attributes.
export COPYFILE_DISABLE=1

RESOURCE_DIRS=(sleigh-specs signatures ghidra-data)

die() {
  echo "package.sh: $*" >&2
  exit 1
}

sha256_line() {
  # "<hash>  <file>", the format `shasum -c` / `sha256sum -c` read.
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

check_resources() {
  [ -d utils ] || die "run from the repository root (no utils/ here)"
  for d in "${RESOURCE_DIRS[@]}"; do
    [ -d "utils/$d" ] || die "utils/$d is missing; the archive would not decompile anything"
  done
  local slaspecs
  slaspecs=$(find utils/sleigh-specs -name '*.slaspec' | wc -l | tr -d ' ')
  [ "$slaspecs" -gt 10 ] || die "utils/sleigh-specs has only $slaspecs .slaspec files"
}

python_cmd() {
  # The first Python 3 that runs: Windows has a non-working `python3` stub.
  local py
  for py in python3 python; do
    if "$py" -c 'import sys; sys.exit(sys.version_info < (3, 9))' >/dev/null 2>&1; then
      echo "$py"
      return
    fi
  done
  die "Python 3.9+ is needed to write the resource manifest"
}

copy_resources() {
  local dest=$1
  mkdir -p "$dest"
  for d in "${RESOURCE_DIRS[@]}"; do
    cp -R "utils/$d" "$dest/$d"
  done
  "$(python_cmd)" scripts/release/resource_manifest.py "$dest"
}

write_checksum() {
  local dir=$1 file=$2
  (cd "$dir" && sha256_line "$file" > "$file.sha256")
  cat "$dir/$file.sha256"
}

package_binary() {
  local target=$1 binary=$2 out=$3
  [ -f "$binary" ] || die "binary not found: $binary"
  check_resources
  mkdir -p "$out"
  out=$(cd "$out" && pwd)

  local name="fission-$target"
  local stage
  stage=$(mktemp -d)
  local root="$stage/$name"
  mkdir -p "$root/bin"
  cp "$binary" "$root/bin/"
  chmod +x "$root/bin/$(basename "$binary")"
  copy_resources "$root/share/fission"
  cp LICENSE NOTICE README.md "$root/"

  local archive
  case "$target" in
    *windows*)
      archive="$name.zip"
      rm -f "$out/$archive"
      if command -v 7z >/dev/null 2>&1; then
        (cd "$stage" && 7z a -tzip -bd "$out/$archive" "$name" >/dev/null)
      else
        (cd "$stage" && zip -qr "$out/$archive" "$name")
      fi
      ;;
    *)
      archive="$name.tar.gz"
      tar -C "$stage" -czf "$out/$archive" "$name"
      ;;
  esac
  rm -rf "$stage"
  write_checksum "$out" "$archive"
}

package_resources() {
  local out=$1
  check_resources
  mkdir -p "$out"
  out=$(cd "$out" && pwd)
  local stage
  stage=$(mktemp -d)
  copy_resources "$stage"
  tar -C "$stage" -czf "$out/fission-resources.tar.gz" "${RESOURCE_DIRS[@]}" fission-resources.json
  rm -rf "$stage"
  write_checksum "$out" fission-resources.tar.gz
}

case "${1:-}" in
  binary)
    [ $# -eq 4 ] || die "usage: package.sh binary <target> <binary> <out_dir>"
    package_binary "$2" "$3" "$4"
    ;;
  resources)
    [ $# -eq 2 ] || die "usage: package.sh resources <out_dir>"
    package_resources "$2"
    ;;
  *)
    die "usage: package.sh binary <target> <binary> <out_dir> | package.sh resources <out_dir>"
    ;;
esac
