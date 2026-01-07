#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v cppcheck >/dev/null 2>&1; then
  echo "cppcheck not found in PATH" >&2
  exit 1
fi

DEFAULT_TARGETS=(
  "$REPO_ROOT/ghidra_decompiler/src/core"
  "$REPO_ROOT/ghidra_decompiler/src/ffi"
  "$REPO_ROOT/ghidra_decompiler/src/decompiler"
)

INCLUDES=(
  -I "$REPO_ROOT/ghidra_decompiler/include"
  -I "$REPO_ROOT/ghidra_decompiler/decompile"
)

if [ "$#" -gt 0 ]; then
  TARGETS=("$@")
else
  TARGETS=("${DEFAULT_TARGETS[@]}")
fi

cppcheck \
  "${INCLUDES[@]}" \
  --enable=warning,style,performance \
  --std=c++17 \
  --suppress=missingInclude \
  --suppress=missingIncludeSystem \
  '--suppress=*:ghidra_decompiler/decompile/*' \
  "${TARGETS[@]}"
