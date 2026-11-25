#!/bin/sh
# Transactional, rollback-safe installer for the Fission CLI and runtime resources.
#
#   curl -fsSL https://raw.githubusercontent.com/fission-systems/Fission/main/install.sh | sh
#
# Usage:
#   sh install.sh [OPTIONS]
#   --prefix <DIR>        Install prefix (default: $HOME/.local)
#   --version <TAG>       Release tag to install (default: latest)
#   --archive <FILE>      Install offline from a local .tar.gz archive
#   --checksum <FILE>     SHA-256 checksum file for local archive
#   --require-checksum    Fail closed if a local archive has no checksum file
#   --allow-downgrade     Allow installing a lower version than currently active
#   --keep <N>            Generations to retain when pruning (minimum: 2)
#   --rollback            Atomically restore the previous generation (bin + resources)
#   --prune               Remove abandoned staging dirs and old releases beyond --keep
#   --uninstall           Remove Fission-managed files/symlinks (preserves user files)
#
# Environment:
#   FISSION_ACTION            install | rollback | prune | uninstall (default: install)
#   FISSION_VERSION           release tag to install (default: latest)
#   FISSION_PREFIX            install prefix (default: $HOME/.local)
#   FISSION_ARCHIVE           local archive path for offline install
#   FISSION_CHECKSUM_FILE     checksum file path for FISSION_ARCHIVE
#   FISSION_ALLOW_DOWNGRADE   set to 1 to permit downgrades
#   FISSION_REQUIRE_CHECKSUM  set to 1 to require a checksum file for local archives
#   FISSION_KEEP_GENERATIONS  number of generations to retain (default: 2, min: 2)
#   FISSION_LOCK_TIMEOUT      seconds to wait for a concurrent install lock (default: 5)
#
# Layout under $FISSION_PREFIX:
#   bin/fission_cli           -> ../lib/fission/current/bin/fission_cli
#   share/fission             -> ../lib/fission/current/share/fission (or per-dir links if user files exist)
#   lib/fission/releases/<id>/bin/fission_cli
#   lib/fission/releases/<id>/share/fission/{sleigh-specs,signatures,ghidra-data}
#   lib/fission/current       -> releases/<active-id>
#   lib/fission/previous      -> releases/<previous-id>
#
# Running processes keep resolving their pinned release/<id>/share/fission
# while new invocations follow lib/fission/current atomically.
# macOS and Linux only; on Windows, unzip fission-x86_64-pc-windows-msvc.zip
# and run bin\fission_cli.exe.

set -eu

REPO="fission-systems/Fission"
ACTION="${FISSION_ACTION:-install}"
PREFIX="${FISSION_PREFIX:-$HOME/.local}"
VERSION="${FISSION_VERSION:-latest}"
ARCHIVE="${FISSION_ARCHIVE:-}"
CHECKSUM_FILE="${FISSION_CHECKSUM_FILE:-}"
ALLOW_DOWNGRADE="${FISSION_ALLOW_DOWNGRADE:-0}"
REQUIRE_CHECKSUM="${FISSION_REQUIRE_CHECKSUM:-0}"
KEEP_GENERATIONS="${FISSION_KEEP_GENERATIONS:-2}"
LOCK_TIMEOUT="${FISSION_LOCK_TIMEOUT:-5}"

MANAGED_SHARE_ITEMS="sleigh-specs signatures ghidra-data fission-resources.json"
REQUIRED_RESOURCE_DIRS="sleigh-specs signatures ghidra-data"

WORK_DIR=""
STAGE_DIR=""
LOCK_DIR=""
LOCK_HELD=0
LEGACY_BACKUP_TMP=""

say() { printf 'fission-install: %s\n' "$*"; }
warn() { printf 'fission-install: warning: %s\n' "$*" >&2; }
die() { printf 'fission-install: error: %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
Usage: install.sh [OPTIONS]

Actions:
  (default)             Install or upgrade Fission transactionally
  --rollback            Atomically roll back binary and resources to previous generation
  --prune               Clean abandoned staging directories and prune old generations
  --uninstall           Remove Fission-managed files while keeping unmanaged user data
  -h, --help            Show this help

Options:
  --prefix <DIR>        Install prefix (default: $HOME/.local)
  --version <TAG>       Release tag to install (default: latest)
  --archive <FILE>      Local .tar.gz archive for offline install
  --checksum <FILE>     SHA-256 checksum file for --archive
  --require-checksum    Require a valid checksum file even for local archives
  --allow-downgrade     Allow installing a lower version than currently installed
  --keep <N>            Generations to retain when pruning (default: 2, min: 2)
EOF
}

cleanup() {
  if [ -n "$STAGE_DIR" ] && [ -d "$STAGE_DIR" ]; then
    rm -rf "$STAGE_DIR" 2>/dev/null || true
  fi
  if [ -n "$LEGACY_BACKUP_TMP" ] && [ -d "$LEGACY_BACKUP_TMP" ]; then
    rm -rf "$LEGACY_BACKUP_TMP" 2>/dev/null || true
  fi
  if [ -n "$WORK_DIR" ] && [ -d "$WORK_DIR" ]; then
    rm -rf "$WORK_DIR" 2>/dev/null || true
  fi
  if [ "$LOCK_HELD" -eq 1 ] && [ -n "$LOCK_DIR" ] && [ -d "$LOCK_DIR" ]; then
    lock_owner=$(cat "$LOCK_DIR/pid" 2>/dev/null || true)
    if [ -z "$lock_owner" ] || [ "$lock_owner" = "$$" ]; then
      rm -rf "$LOCK_DIR" 2>/dev/null || true
    fi
    LOCK_HELD=0
  fi
}

is_uint() {
  case "${1:-}" in
    ''|*[!0-9]*) return 1 ;;
    *) return 0 ;;
  esac
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --rollback) ACTION="rollback"; shift ;;
      --prune) ACTION="prune"; shift ;;
      --uninstall) ACTION="uninstall"; shift ;;
      --allow-downgrade) ALLOW_DOWNGRADE=1; shift ;;
      --require-checksum) REQUIRE_CHECKSUM=1; shift ;;
      --prefix)
        [ "$#" -ge 2 ] || die "--prefix requires a directory argument"
        PREFIX="$2"; shift 2 ;;
      --prefix=*)
        PREFIX="${1#*=}"; shift ;;
      --version)
        [ "$#" -ge 2 ] || die "--version requires a value"
        VERSION="$2"; shift 2 ;;
      --version=*)
        VERSION="${1#*=}"; shift ;;
      --archive)
        [ "$#" -ge 2 ] || die "--archive requires a file path"
        ARCHIVE="$2"; shift 2 ;;
      --archive=*)
        ARCHIVE="${1#*=}"; shift ;;
      --checksum)
        [ "$#" -ge 2 ] || die "--checksum requires a file path"
        CHECKSUM_FILE="$2"; shift 2 ;;
      --checksum=*)
        CHECKSUM_FILE="${1#*=}"; shift ;;
      --keep)
        [ "$#" -ge 2 ] || die "--keep requires an integer"
        KEEP_GENERATIONS="$2"; shift 2 ;;
      --keep=*)
        KEEP_GENERATIONS="${1#*=}"; shift ;;
      -h|--help)
        usage; exit 0 ;;
      *)
        die "unknown option: $1 (try --help)" ;;
    esac
  done

  case "$ACTION" in
    install|rollback|prune|uninstall) ;;
    *) die "invalid FISSION_ACTION: $ACTION" ;;
  esac
  [ -n "$PREFIX" ] || die "FISSION_PREFIX must not be empty"
  case "$PREFIX" in
    /) die "refusing to operate on root directory '/'" ;;
  esac
  # Trim trailing slashes for clean relative path checks.
  while :; do
    case "$PREFIX" in
      */) PREFIX="${PREFIX%/}" ;;
      *) break ;;
    esac
  done
  is_uint "$KEEP_GENERATIONS" || die "--keep must be a positive integer: $KEEP_GENERATIONS"
  if [ "$KEEP_GENERATIONS" -lt 2 ]; then
    KEEP_GENERATIONS=2
  fi
  is_uint "$LOCK_TIMEOUT" || die "FISSION_LOCK_TIMEOUT must be a non-negative integer: $LOCK_TIMEOUT"
}

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
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"
  else
    die "need sha256sum, shasum, or python3 to verify checksums"
  fi
}

verify_checksum() {
  archive=$1
  sumfile=$2
  expected=$(awk 'NF { print tolower($1); exit }' "$sumfile" 2>/dev/null || true)
  case "$expected" in
    ''|*[!0-9a-f]*)
      die "malformed checksum file: $sumfile" ;;
  esac
  if [ "${#expected}" -ne 64 ]; then
    die "invalid SHA-256 digest length in checksum file: $sumfile"
  fi
  actual=$(sha256_of "$archive" | tr '[:upper:]' '[:lower:]')
  [ "$expected" = "$actual" ] || die "checksum mismatch for $(basename "$archive")"
  say "checksum ok"
}

atomic_symlink_replace() {
  target=$1
  link_path=$2
  parent_dir=$(dirname "$link_path")
  mkdir -p "$parent_dir"
  tmp_link="${link_path}.tmp.$$"
  rm -f "$tmp_link"
  ln -s "$target" "$tmp_link"

  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import os, sys; os.replace(sys.argv[1], sys.argv[2])' "$tmp_link" "$link_path"
  elif command -v perl >/dev/null 2>&1; then
    perl -e 'rename($ARGV[0], $ARGV[1]) or die "$!\n"' "$tmp_link" "$link_path"
  elif mv -T "$tmp_link" "$link_path" 2>/dev/null; then
    :
  else
    rm -f "$tmp_link"
    # Fallback when rename(2) helpers and GNU mv -T are unavailable.
    if [ -L "$link_path" ]; then
      rm -f "$link_path"
    fi
    ln -s "$target" "$link_path"
  fi
}

store_dir() {
  printf '%s/lib/fission' "$PREFIX"
}

acquire_lock() {
  store=$(store_dir)
  mkdir -p "$store"
  LOCK_DIR="$store/.install.lock"
  elapsed=0
  while :; do
    if mkdir "$LOCK_DIR" 2>/dev/null; then
      if ! printf '%s\n' "$$" > "$LOCK_DIR/pid"; then
        rm -rf "$LOCK_DIR" 2>/dev/null || true
        die "failed to write lock owner pid under $LOCK_DIR"
      fi
      LOCK_HELD=1
      return 0
    fi

    holder_pid=$(cat "$LOCK_DIR/pid" 2>/dev/null || true)
    if is_uint "$holder_pid" && [ "$holder_pid" -ne "$$" ]; then
      if ! kill -0 "$holder_pid" 2>/dev/null; then
        warn "reaping stale install lock from terminated pid $holder_pid"
        rm -rf "$LOCK_DIR" 2>/dev/null || true
        continue
      fi
    fi

    if [ "$elapsed" -ge "$LOCK_TIMEOUT" ]; then
      die "another installation is already in progress (lock held at $LOCK_DIR)"
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
}

clean_abandoned_staging() {
  store=$(store_dir)
  [ -d "$store" ] || return 0
  for d in "$store"/.stage.* "$store"/.legacy-share.*; do
    [ -e "$d" ] || [ -L "$d" ] || continue
    rm -rf "$d" 2>/dev/null || true
  done
}

extract_semver() {
  # Extract the first X.Y.Z semver tuple from text; prints empty string if absent.
  printf '%s\n' "${1:-}" | sed -n 's/.*\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -n 1
}

compare_semver() {
  # Echoes: -1 if $1 < $2, 0 if equal, 1 if $1 > $2.
  v1=$1
  v2=$2
  oifs=$IFS
  IFS=.
  # shellcheck disable=SC2086
  set -- $v1
  a1=${1:-0} a2=${2:-0} a3=${3:-0}
  # shellcheck disable=SC2086
  set -- $v2
  b1=${1:-0} b2=${2:-0} b3=${3:-0}
  IFS=$oifs

  if [ "$a1" -lt "$b1" ]; then echo -1; return 0; fi
  if [ "$a1" -gt "$b1" ]; then echo 1; return 0; fi
  if [ "$a2" -lt "$b2" ]; then echo -1; return 0; fi
  if [ "$a2" -gt "$b2" ]; then echo 1; return 0; fi
  if [ "$a3" -lt "$b3" ]; then echo -1; return 0; fi
  if [ "$a3" -gt "$b3" ]; then echo 1; return 0; fi
  echo 0
}

is_managed_share_item() {
  case "$1" in
    sleigh-specs|signatures|ghidra-data|fission-resources.json) return 0 ;;
    *) return 1 ;;
  esac
}

share_dir_has_unmanaged_entries() {
  share_path="$PREFIX/share/fission"
  [ -d "$share_path" ] && [ ! -L "$share_path" ] || return 1
  for entry in "$share_path"/* "$share_path"/.[!.]* "$share_path"/..?*; do
    [ -e "$entry" ] || [ -L "$entry" ] || continue
    base=$(basename "$entry")
    if ! is_managed_share_item "$base"; then
      return 0
    fi
  done
  return 1
}

validate_archive_safety() {
  archive=$1
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$archive" <<'PY' || die "unsafe or unreadable archive: $archive"
import posixpath
import sys
import tarfile

archive = sys.argv[1]
try:
    with tarfile.open(archive, "r:gz") as tf:
        members = tf.getmembers()
except Exception as exc:
    sys.stderr.write(f"fission-install: archive read failed: {exc}\n")
    sys.exit(1)

if not members:
    sys.stderr.write("fission-install: archive is empty\n")
    sys.exit(1)

top_dirs = set()
for m in members:
    name = m.name
    if not name or name.startswith("/") or "\\" in name:
        sys.stderr.write(f"fission-install: unsafe archive path: {name!r}\n")
        sys.exit(1)
    parts = [p for p in name.split("/") if p not in ("", ".")]
    if ".." in parts:
        sys.stderr.write(f"fission-install: path traversal in archive: {name!r}\n")
        sys.exit(1)
    norm = posixpath.normpath(name)
    if norm == ".." or norm.startswith("../") or norm.startswith("/"):
        sys.stderr.write(f"fission-install: escaping normalized path: {name!r}\n")
        sys.exit(1)
    if m.issym() or m.islnk() or not (m.isdir() or m.isreg()):
        sys.stderr.write(f"fission-install: unsupported link or special entry in archive: {name!r}\n")
        sys.exit(1)
    if parts:
        top_dirs.add(parts[0])

if len(top_dirs) != 1:
    sys.stderr.write(f"fission-install: expected one top-level directory, found {sorted(top_dirs)}\n")
    sys.exit(1)
PY
    return 0
  fi

  list_file="$WORK_DIR/tar_list.txt"
  tar -tzf "$archive" > "$list_file" 2>/dev/null || die "corrupt or unreadable archive: $archive"
  [ -s "$list_file" ] || die "archive is empty: $archive"
  while IFS= read -r member; do
    case "$member" in
      /*|..|../*|*/..|*/../*)
        die "unsafe path in archive: $member" ;;
    esac
  done < "$list_file"

  if tar -tvzf "$archive" > "$WORK_DIR/tar_verbose.txt" 2>/dev/null; then
    while IFS= read -r line; do
      ftype=$(printf '%s' "$line" | cut -c1)
      case "$ftype" in
        -|d) ;;
        *) die "unsupported entry type ('$ftype') in archive" ;;
      esac
    done < "$WORK_DIR/tar_verbose.txt"
  fi
}

validate_manifest_and_pair() {
  gen_dir=$1
  bin_ver=$2
  expected_target=$3
  manifest="$gen_dir/share/fission/fission-resources.json"
  [ -f "$manifest" ] || return 0

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$manifest" "$bin_ver" "$expected_target" "$gen_dir/share/fission" <<'PY' || die "staged resource manifest validation failed"
import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
bin_ver = sys.argv[2]
bundle_root = Path(sys.argv[4])

try:
    data = json.loads(manifest_path.read_text(encoding="utf-8"))
except Exception as exc:
    sys.stderr.write(f"fission-install: invalid fission-resources.json: {exc}\n")
    sys.exit(1)

if not isinstance(data, dict) or data.get("schema") != 1:
    sys.stderr.write("fission-install: unsupported fission-resources.json schema\n")
    sys.exit(1)

man_ver = data.get("fission_version")
if not isinstance(man_ver, str) or not man_ver.strip():
    sys.stderr.write("fission-install: missing fission_version in manifest\n")
    sys.exit(1)

if bin_ver and man_ver.strip() != bin_ver.strip():
    sys.stderr.write(
        f"fission-install: binary/resource version mismatch: binary={bin_ver} manifest={man_ver}\n"
    )
    sys.exit(1)

resources = data.get("resources")
if isinstance(resources, dict):
    for d, info in resources.items():
        d_path = bundle_root / d
        if not d_path.is_dir():
            sys.stderr.write(f"fission-install: manifest resource dir missing: {d}\n")
            sys.exit(1)
        if isinstance(info, dict) and "files" in info and "sha256" in info:
            lines = []
            for p in sorted(x for x in d_path.rglob("*") if x.is_file()):
                rel = p.relative_to(d_path).as_posix()
                digest = hashlib.sha256(p.read_bytes()).hexdigest()
                lines.append(f"{rel}\0{digest}\n")
            actual_digest = hashlib.sha256("".join(lines).encode("utf-8")).hexdigest()
            if len(lines) != info["files"] or actual_digest != info["sha256"]:
                sys.stderr.write(
                    f"fission-install: resource integrity mismatch in {d}: "
                    f"expected files={info['files']} sha256={info['sha256']}, "
                    f"got files={len(lines)} sha256={actual_digest}\n"
                )
                sys.exit(1)
PY
  fi
}

health_check_binary() {
  bin_path=$1
  expected_ver=${2:-}
  ver_out=$(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$bin_path" --version 2>/dev/null) || return 1
  [ -n "$ver_out" ] || return 1

  if [ -n "$expected_ver" ]; then
    actual_ver=$(extract_semver "$ver_out")
    if [ -n "$actual_ver" ] && [ "$actual_ver" != "$expected_ver" ]; then
      warn "health check version mismatch: expected $expected_ver, got $actual_ver"
      return 1
    fi
  fi

  status_out=$(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$bin_path" resources status --json 2>/dev/null) || return 1
  [ -n "$status_out" ] || return 1

  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$status_out" | python3 -c '
import json
import sys

expected_ver = sys.argv[1]
try:
    payload = json.loads(sys.stdin.read())
except Exception:
    sys.exit(1)
if not isinstance(payload, dict):
    sys.exit(1)

build = payload.get("build") or {}
if expected_ver and isinstance(build, dict):
    bver = str(build.get("version") or "")
    if bver and bver != expected_ver:
        sys.exit(1)

rman = payload.get("resource_manifest") or {}
if isinstance(rman, dict):
    state = rman.get("state")
    if state in ("mismatch", "invalid"):
        sys.exit(1)

res = payload.get("resources") or {}
if isinstance(res, dict) and "signatures_base" in res and not res.get("signatures_base"):
    sys.exit(1)
' "$expected_ver" || return 1
  fi
  return 0
}

current_installed_version() {
  store=$(store_dir)
  if [ -L "$store/current" ] && [ -f "$store/current/.fission-generation" ]; then
    v=$(sed -n 's/^version=//p' "$store/current/.fission-generation" | head -n 1)
    v=$(extract_semver "$v")
    if [ -n "$v" ]; then
      printf '%s\n' "$v"
      return 0
    fi
  fi
  if [ -x "$PREFIX/bin/fission_cli" ]; then
    v=$(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$PREFIX/bin/fission_cli" --version 2>/dev/null || true)
    v=$(extract_semver "$v")
    if [ -n "$v" ]; then
      printf '%s\n' "$v"
      return 0
    fi
  fi
  return 0
}

snapshot_legacy_install_if_needed() {
  store=$(store_dir)
  # Only snapshot if transactional `current` does not yet exist, while a legacy
  # flat binary is already present in $PREFIX/bin/fission_cli.
  [ ! -e "$store/current" ] && [ ! -L "$store/current" ] || return 0
  [ -f "$PREFIX/bin/fission_cli" ] || return 0
  if [ -L "$PREFIX/bin/fission_cli" ]; then
    link_dest=$(readlink "$PREFIX/bin/fission_cli" 2>/dev/null || true)
    case "$link_dest" in
      *lib/fission/current/*) return 0 ;;
    esac
  fi

  legacy_id="legacy-$(date -u +%Y%m%dT%H%M%SZ 2>/dev/null || echo 0)-$$"
  legacy_dir="$store/releases/$legacy_id"
  mkdir -p "$legacy_dir/bin" "$legacy_dir/share/fission"
  cp -p "$PREFIX/bin/fission_cli" "$legacy_dir/bin/fission_cli"
  chmod +x "$legacy_dir/bin/fission_cli"

  if [ -d "$PREFIX/share/fission" ]; then
    for item in $MANAGED_SHARE_ITEMS; do
      src="$PREFIX/share/fission/$item"
      if [ -e "$src" ]; then
        cp -R "$src" "$legacy_dir/share/fission/$item"
      fi
    done
  fi

  legacy_ver=$(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$legacy_dir/bin/fission_cli" --version 2>/dev/null || true)
  legacy_semver=$(extract_semver "$legacy_ver")
  {
    printf 'version=%s\n' "${legacy_semver:-unknown}"
    printf 'legacy=1\n'
  } > "$legacy_dir/.fission-generation"

  atomic_symlink_replace "releases/$legacy_id" "$store/previous"
  say "captured legacy installation as rollback generation $legacy_id"
}

sync_prefix_entrypoints() {
  mkdir -p "$PREFIX/bin" "$PREFIX/share"
  atomic_symlink_replace "../lib/fission/current/bin/fission_cli" "$PREFIX/bin/fission_cli"

  store=$(store_dir)
  active_gen_rel=$(readlink "$store/current" 2>/dev/null || true)
  active_share="$store/$active_gen_rel/share/fission"

  if [ -L "$PREFIX/share/fission" ]; then
    atomic_symlink_replace "../lib/fission/current/share/fission" "$PREFIX/share/fission"
    return 0
  fi

  if [ -d "$PREFIX/share/fission" ]; then
    if share_dir_has_unmanaged_entries; then
      # Keep $PREFIX/share/fission as a real directory to preserve custom user files.
      LEGACY_BACKUP_TMP="$store/.legacy-share.$$"
      mkdir -p "$LEGACY_BACKUP_TMP"
      for item in $REQUIRED_RESOURCE_DIRS; do
        dst="$PREFIX/share/fission/$item"
        if [ -e "$dst" ] && [ ! -L "$dst" ]; then
          mv "$dst" "$LEGACY_BACKUP_TMP/$item"
        fi
        atomic_symlink_replace "../../lib/fission/current/share/fission/$item" "$dst"
      done
      man_dst="$PREFIX/share/fission/fission-resources.json"
      if [ -e "$man_dst" ] && [ ! -L "$man_dst" ]; then
        mv "$man_dst" "$LEGACY_BACKUP_TMP/fission-resources.json"
      fi
      if [ -f "$active_share/fission-resources.json" ]; then
        atomic_symlink_replace "../../lib/fission/current/share/fission/fission-resources.json" "$man_dst"
      elif [ -L "$man_dst" ]; then
        rm -f "$man_dst"
      fi
      rm -rf "$LEGACY_BACKUP_TMP"
      LEGACY_BACKUP_TMP=""
      return 0
    fi

    # Pure legacy resource dir with no user files: replace atomically with symlink.
    LEGACY_BACKUP_TMP="$store/.legacy-share.$$"
    mv "$PREFIX/share/fission" "$LEGACY_BACKUP_TMP"
    atomic_symlink_replace "../lib/fission/current/share/fission" "$PREFIX/share/fission"
    rm -rf "$LEGACY_BACKUP_TMP"
    LEGACY_BACKUP_TMP=""
    return 0
  fi

  atomic_symlink_replace "../lib/fission/current/share/fission" "$PREFIX/share/fission"
}

prune_releases_locked() {
  store=$(store_dir)
  rel_dir="$store/releases"
  [ -d "$rel_dir" ] || return 0

  cur_target=""
  prev_target=""
  if [ -L "$store/current" ]; then
    cur_target=$(basename "$(readlink "$store/current")")
  fi
  if [ -L "$store/previous" ]; then
    prev_target=$(basename "$(readlink "$store/previous")")
  fi

  # Collect release directory names ordered newest-first using mtime/order fallback.
  # Always preserve current and previous.
  ordered=$(ls -1t "$rel_dir" 2>/dev/null || ls -1 "$rel_dir" 2>/dev/null || true)
  kept=0
  for name in $ordered; do
    [ -d "$rel_dir/$name" ] || continue
    if [ "$name" = "$cur_target" ] || [ "$name" = "$prev_target" ]; then
      kept=$((kept + 1))
    fi
  done

  for name in $ordered; do
    [ -d "$rel_dir/$name" ] || continue
    if [ "$name" = "$cur_target" ] || [ "$name" = "$prev_target" ]; then
      continue
    fi
    if [ "$kept" -lt "$KEEP_GENERATIONS" ]; then
      kept=$((kept + 1))
      continue
    fi
    rm -rf "$rel_dir/$name"
  done
}

do_install() {
  target=$(detect_target)
  WORK_DIR=$(mktemp -d)

  if [ -n "$ARCHIVE" ]; then
    [ -f "$ARCHIVE" ] || die "FISSION_ARCHIVE not found: $ARCHIVE"
    archive_path="$ARCHIVE"
    if [ -n "$CHECKSUM_FILE" ]; then
      [ -f "$CHECKSUM_FILE" ] || die "checksum file not found: $CHECKSUM_FILE"
      verify_checksum "$archive_path" "$CHECKSUM_FILE"
    elif [ -f "$archive_path.sha256" ]; then
      verify_checksum "$archive_path" "$archive_path.sha256"
    elif [ "$REQUIRE_CHECKSUM" -eq 1 ]; then
      die "checksum file required for local archive (--require-checksum)"
    fi
  else
    asset="fission-$target.tar.gz"
    if [ "$VERSION" = latest ]; then
      base="https://github.com/$REPO/releases/latest/download"
    else
      base="https://github.com/$REPO/releases/download/$VERSION"
    fi
    say "downloading $asset ($VERSION)"
    archive_path="$WORK_DIR/$asset"
    fetch "$base/$asset" "$archive_path" || die "download failed: $base/$asset"
    fetch "$base/$asset.sha256" "$WORK_DIR/$asset.sha256" || die "checksum download failed"
    verify_checksum "$archive_path" "$WORK_DIR/$asset.sha256"
  fi

  validate_archive_safety "$archive_path"
  archive_sha=$(sha256_of "$archive_path")

  acquire_lock
  clean_abandoned_staging

  store=$(store_dir)
  mkdir -p "$store/releases"

  STAGE_DIR="$store/.stage.$$.$(date -u +%Y%m%d%H%M%S 2>/dev/null || echo 0)"
  mkdir -p "$STAGE_DIR/unpack" "$STAGE_DIR/gen/bin" "$STAGE_DIR/gen/share/fission"

  tar -C "$STAGE_DIR/unpack" -xzf "$archive_path" || die "failed to extract archive"

  # Reject any symlinks created during extraction as defense in depth.
  if [ -n "$(find "$STAGE_DIR/unpack" -type l 2>/dev/null || true)" ]; then
    die "archive contains unexpected symbolic links"
  fi

  root=$(find "$STAGE_DIR/unpack" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  [ -n "$root" ] || die "archive has no top-level directory"
  [ -f "$root/bin/fission_cli" ] && [ -s "$root/bin/fission_cli" ] || die "archive has no executable bin/fission_cli"

  for d in $REQUIRED_RESOURCE_DIRS; do
    [ -d "$root/share/fission/$d" ] || die "archive lacks required resource directory: share/fission/$d"
  done

  cp -p "$root/bin/fission_cli" "$STAGE_DIR/gen/bin/fission_cli" || die "failed to stage bin/fission_cli"
  chmod 755 "$STAGE_DIR/gen/bin/fission_cli" || die "failed to mark staged binary executable"

  for d in $REQUIRED_RESOURCE_DIRS; do
    cp -R "$root/share/fission/$d" "$STAGE_DIR/gen/share/fission/$d" || die "failed to stage resource dir: $d"
  done
  if [ -f "$root/share/fission/fission-resources.json" ]; then
    cp -p "$root/share/fission/fission-resources.json" "$STAGE_DIR/gen/share/fission/fission-resources.json" \
      || die "failed to stage fission-resources.json"
  fi

  staged_bin="$STAGE_DIR/gen/bin/fission_cli"
  staged_ver_raw=$(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$staged_bin" --version 2>/dev/null || true)
  [ -n "$staged_ver_raw" ] || die "staged binary failed --version check"
  staged_semver=$(extract_semver "$staged_ver_raw")

  validate_manifest_and_pair "$STAGE_DIR/gen" "$staged_semver" "$target"

  installed_semver=$(current_installed_version)
  if [ -n "$installed_semver" ] && [ -n "$staged_semver" ]; then
    cmp_res=$(compare_semver "$staged_semver" "$installed_semver")
    if [ "$cmp_res" -lt 0 ] && [ "$ALLOW_DOWNGRADE" -ne 1 ]; then
      die "refusing silent downgrade from $installed_semver to $staged_semver (pass --allow-downgrade to override)"
    fi
  fi

  health_check_binary "$staged_bin" "$staged_semver" || die "staged generation failed pre-activation health check"

  base_label="${staged_semver:-custom}-$(date -u +%Y%m%dT%H%M%SZ 2>/dev/null || echo 0)-$$"
  gen_label="$base_label"
  seq=1
  while [ -e "$store/releases/$gen_label" ]; do
    gen_label="${base_label}-$seq"
    seq=$((seq + 1))
  done
  gen_dir="$store/releases/$gen_label"
  {
    printf 'version=%s\n' "${staged_semver:-unknown}"
    printf 'target=%s\n' "$target"
    printf 'archive_sha256=%s\n' "$archive_sha"
    printf 'installed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo unknown)"
  } > "$STAGE_DIR/gen/.fission-generation"

  mv "$STAGE_DIR/gen" "$gen_dir" || die "failed to commit staged generation to $gen_dir"
  rm -rf "$STAGE_DIR"
  STAGE_DIR=""

  snapshot_legacy_install_if_needed

  prev_rel=""
  had_prev=0
  if [ -L "$store/current" ]; then
    prev_rel=$(readlink "$store/current" 2>/dev/null || true)
    if [ -n "$prev_rel" ] && [ "$prev_rel" != "releases/$gen_label" ]; then
      atomic_symlink_replace "$prev_rel" "$store/previous"
      had_prev=1
    fi
  elif [ -L "$store/previous" ]; then
    prev_rel=$(readlink "$store/previous" 2>/dev/null || true)
    [ -n "$prev_rel" ] && had_prev=1
  fi

  atomic_symlink_replace "releases/$gen_label" "$store/current"
  sync_prefix_entrypoints

  if ! health_check_binary "$PREFIX/bin/fission_cli" "$staged_semver"; then
    warn "post-activation health check failed; attempting rollback"
    if [ "$had_prev" -eq 1 ] && [ -n "$prev_rel" ] && [ -d "$store/$prev_rel" ]; then
      atomic_symlink_replace "$prev_rel" "$store/current"
      sync_prefix_entrypoints
      rm -rf "$gen_dir"
      die "post-activation health check failed; rolled back to $prev_rel"
    fi
    die "post-activation health check failed"
  fi

  prune_releases_locked

  say "installed $(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$PREFIX/bin/fission_cli" --version) to $PREFIX/bin/fission_cli (generation $gen_label)"
  case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) say "add $PREFIX/bin to PATH, e.g.  export PATH=\"$PREFIX/bin:\$PATH\"" ;;
  esac
}

do_rollback() {
  store=$(store_dir)
  [ -d "$store" ] || die "no Fission installation store at $store"

  acquire_lock
  clean_abandoned_staging

  [ -L "$store/previous" ] || die "no previous generation recorded for rollback"
  prev_rel=$(readlink "$store/previous" 2>/dev/null || true)
  [ -n "$prev_rel" ] && [ -d "$store/$prev_rel" ] || die "previous generation target is missing: $prev_rel"
  [ -x "$store/$prev_rel/bin/fission_cli" ] || die "previous generation lacks bin/fission_cli: $prev_rel"

  prev_ver=""
  if [ -f "$store/$prev_rel/.fission-generation" ]; then
    prev_ver=$(extract_semver "$(sed -n 's/^version=//p' "$store/$prev_rel/.fission-generation" | head -n 1)")
  fi

  health_check_binary "$store/$prev_rel/bin/fission_cli" "$prev_ver" \
    || die "previous generation $prev_rel failed health check; refusing rollback"

  cur_rel=""
  if [ -L "$store/current" ]; then
    cur_rel=$(readlink "$store/current" 2>/dev/null || true)
  fi

  atomic_symlink_replace "$prev_rel" "$store/current"
  if [ -n "$cur_rel" ] && [ "$cur_rel" != "$prev_rel" ] && [ -d "$store/$cur_rel" ]; then
    atomic_symlink_replace "$cur_rel" "$store/previous"
  fi
  sync_prefix_entrypoints

  health_check_binary "$PREFIX/bin/fission_cli" "$prev_ver" \
    || die "post-rollback health check failed"

  say "rolled back to $(basename "$prev_rel") ($(env -u FISSION_ROOT -u FISSION_RESOURCE_ROOT "$PREFIX/bin/fission_cli" --version))"
}

do_prune() {
  store=$(store_dir)
  if [ ! -d "$store" ]; then
    say "nothing to prune under $PREFIX"
    return 0
  fi
  acquire_lock
  clean_abandoned_staging
  prune_releases_locked
  say "pruned abandoned staging and excess generations (kept $KEEP_GENERATIONS)"
}

do_uninstall() {
  store=$(store_dir)
  if [ -d "$store" ]; then
    acquire_lock
  fi

  if [ -L "$PREFIX/bin/fission_cli" ]; then
    bin_link=$(readlink "$PREFIX/bin/fission_cli" 2>/dev/null || true)
    case "$bin_link" in
      *lib/fission/*) rm -f "$PREFIX/bin/fission_cli" ;;
    esac
  fi

  if [ -L "$PREFIX/share/fission" ]; then
    share_link=$(readlink "$PREFIX/share/fission" 2>/dev/null || true)
    case "$share_link" in
      *lib/fission/*) rm -f "$PREFIX/share/fission" ;;
    esac
  elif [ -d "$PREFIX/share/fission" ]; then
    for item in $MANAGED_SHARE_ITEMS; do
      entry="$PREFIX/share/fission/$item"
      if [ -L "$entry" ]; then
        item_link=$(readlink "$entry" 2>/dev/null || true)
        case "$item_link" in
          *lib/fission/*) rm -f "$entry" ;;
        esac
      fi
    done
    rmdir "$PREFIX/share/fission" 2>/dev/null || true
  fi

  if [ -d "$store" ]; then
    LOCK_HELD=0
    rm -rf "$store"
  fi

  rmdir "$PREFIX/bin" 2>/dev/null || true
  rmdir "$PREFIX/lib" 2>/dev/null || true
  rmdir "$PREFIX/share" 2>/dev/null || true

  say "uninstalled Fission-managed files from $PREFIX (unmanaged user files preserved)"
}

main() {
  parse_args "$@"
  trap cleanup EXIT INT TERM
  case "$ACTION" in
    install) do_install ;;
    rollback) do_rollback ;;
    prune) do_prune ;;
    uninstall) do_uninstall ;;
  esac
}

main "$@"
