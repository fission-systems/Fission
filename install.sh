#!/bin/sh
# Install the Fission CLI from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/fission-systems/Fission/main/install.sh | sh
#
# Environment:
#   FISSION_VERSION   release tag to install (default: the latest release)
#   FISSION_PREFIX    install prefix (default: $HOME/.local)
#                     -> $FISSION_PREFIX/bin/fission_cli
#                     -> $FISSION_PREFIX/share/fission/  (Sleigh specs, signatures)
#   FISSION_ARCHIVE   install from a local archive instead of downloading
#                     (its .sha256 next to it is checked when present)
#
# The binary finds share/fission on its own (exe_parent/../share/fission), so
# nothing needs to be configured after install. macOS and Linux only; on
# Windows, unzip fission-x86_64-pc-windows-msvc.zip and run bin\fission_cli.exe.

set -eu

REPO="fission-systems/Fission"
PREFIX="${FISSION_PREFIX:-$HOME/.local}"
VERSION="${FISSION_VERSION:-latest}"

say() { printf 'fission-install: %s\n' "$*"; }
die() { printf 'fission-install: error: %s\n' "$*" >&2; exit 1; }

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=aarch64 ;;
    *) die "unsupported CPU: $arch" ;;
  esac
  case "$os" in
    Darwin) echo "$arch-apple-darwin" ;;
    Linux) echo "$arch-unknown-linux-gnu" ;;
    *) die "unsupported OS: $os (on Windows use the .zip from the releases page)" ;;
  esac
}

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "need curl or wget"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    die "need sha256sum or shasum to verify the download"
  fi
}

verify() {
  archive=$1
  sumfile=$2
  expected=$(cut -d' ' -f1 < "$sumfile")
  actual=$(sha256_of "$archive")
  [ "$expected" = "$actual" ] || die "checksum mismatch for $(basename "$archive")"
  say "checksum ok"
}

main() {
  target=$(detect_target)
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT INT TERM

  if [ -n "${FISSION_ARCHIVE:-}" ]; then
    [ -f "$FISSION_ARCHIVE" ] || die "FISSION_ARCHIVE not found: $FISSION_ARCHIVE"
    archive="$FISSION_ARCHIVE"
    if [ -f "$archive.sha256" ]; then verify "$archive" "$archive.sha256"; fi
  else
    asset="fission-$target.tar.gz"
    if [ "$VERSION" = latest ]; then
      base="https://github.com/$REPO/releases/latest/download"
    else
      base="https://github.com/$REPO/releases/download/$VERSION"
    fi
    say "downloading $asset ($VERSION)"
    archive="$work/$asset"
    fetch "$base/$asset" "$archive" || die "download failed: $base/$asset"
    fetch "$base/$asset.sha256" "$work/$asset.sha256" || die "checksum download failed"
    verify "$archive" "$work/$asset.sha256"
  fi

  mkdir -p "$work/unpack"
  tar -C "$work/unpack" -xzf "$archive"
  root=$(find "$work/unpack" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  [ -f "$root/bin/fission_cli" ] || die "archive has no bin/fission_cli"

  mkdir -p "$PREFIX/bin" "$PREFIX/share/fission"
  for d in sleigh-specs signatures ghidra-data; do
    rm -rf "${PREFIX:?}/share/fission/$d"
    cp -R "$root/share/fission/$d" "$PREFIX/share/fission/$d"
  done
  # Replace rather than overwrite in place: a running copy keeps its inode.
  cp "$root/bin/fission_cli" "$PREFIX/bin/fission_cli.new"
  chmod +x "$PREFIX/bin/fission_cli.new"
  mv -f "$PREFIX/bin/fission_cli.new" "$PREFIX/bin/fission_cli"

  say "installed $("$PREFIX/bin/fission_cli" --version) to $PREFIX/bin/fission_cli"
  case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) say "add $PREFIX/bin to PATH, e.g.  export PATH=\"$PREFIX/bin:\$PATH\"" ;;
  esac
}

main "$@"
