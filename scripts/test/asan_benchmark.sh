#!/usr/bin/env bash
# ASAN build + benchmark to reproduce SIGSEGV (exit 139)
# Usage: ./scripts/test/asan_benchmark.sh [binary] [limit]
# Example: ./scripts/test/asan_benchmark.sh samples/windows/x64/putty.exe 100

set -e
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BINARY="${1:-samples/windows/x64/putty.exe}"
LIMIT="${2:-100}"

echo "[*] Building with ASAN (FISSION_ASAN=1)..."
export FISSION_ASAN=1
cargo build -p fission-cli --features native_decomp --release

echo "[*] Running benchmark (RAYON_NUM_THREADS=8)..."
export DYLD_LIBRARY_PATH="$ROOT/target/release${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="$ROOT/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
RAYON_NUM_THREADS=8 "$ROOT/target/release/fission_cli" "$BINARY" \
  --decomp-all --benchmark --ghidra-compat --profile balanced \
  --decomp-limit "$LIMIT" -o /tmp/asan_benchmark.json

echo "[*] Completed successfully (no SIGSEGV)"
