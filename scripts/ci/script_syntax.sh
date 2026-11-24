#!/usr/bin/env bash
# Syntax checks for everything under scripts/, as the Script Checks job in
# .github/workflows/ci.yml runs them. Also called by `cargo xtask preflight`.
#
#   script_syntax.sh py   python3 -m py_compile on scripts/**/*.py
#   script_syntax.sh sh   bash -n on scripts/**/*.sh
#
# Bash 3.2 compatible (macOS has no mapfile).
set -euo pipefail
cd "$(dirname "$0")/../.."

files=()
case "${1:-}" in
  py)
    while IFS= read -r f; do files+=("$f"); done < <(find scripts -type f -name '*.py' 2>/dev/null || true)
    if [ "${#files[@]}" -eq 0 ]; then
      echo "No Python scripts under scripts/"
      exit 0
    fi
    python3 -m py_compile "${files[@]}"
    echo "OK: py_compile ok (${#files[@]} files)"
    ;;
  sh)
    while IFS= read -r f; do files+=("$f"); done < <(find scripts -type f -name '*.sh' 2>/dev/null || true)
    if [ "${#files[@]}" -eq 0 ]; then
      echo "No shell scripts under scripts/"
      exit 0
    fi
    for f in "${files[@]}"; do
      bash -n "$f"
    done
    echo "OK: bash -n ok (${#files[@]} files)"
    ;;
  *)
    echo "usage: $0 {py|sh}" >&2
    exit 2
    ;;
esac
