#!/usr/bin/env bash
# Fission-only decompilation smoke test for CI.
# Runs without Ghidra: builds a small C++ test binary, decompiles one function,
# and requires exit 0. Used on every commit/PR; full quality benchmark (Ghidra)
# runs separately (scheduled or local).
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

ARTIFACTS_DIR=".artifacts/decomp-smoke"
BIN="$ARTIFACTS_DIR/comparison_test_cpp_elf"
SRC="examples/sources/comparison_test_cpp.cpp"

mkdir -p "$ARTIFACTS_DIR"

echo "[*] Building C++ test binary for decomp smoke..."
if [ ! -f "$BIN" ] || [ "$SRC" -nt "$BIN" ]; then
  g++ -std=c++17 -O0 -g -fno-omit-frame-pointer -fno-inline -no-pie \
    "$SRC" -o "$BIN"
fi

# Get address of 'main' (portable: Linux uses 'main', macOS uses '_main')
MAIN_ADDR="$(nm -n "$BIN" | awk '$3=="main" || $3=="_main" {print $1; exit}')"
if [ -z "$MAIN_ADDR" ]; then
  echo "Error: symbol 'main' not found in $BIN" >&2
  exit 1
fi

echo "[*] Running Fission decompilation at main (0x$MAIN_ADDR)..."
if ! cargo run --quiet --bin fission_cli -- "$BIN" --decomp "0x$MAIN_ADDR"; then
  echo "Error: fission_cli --decomp exited non-zero" >&2
  exit 1
fi

echo "[*] Decompilation smoke test passed."
