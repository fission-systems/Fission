# Phase A/B/C Performance Benchmark Results

This document summarizes benchmark and regression results after applying:

- **Phase A**: symbol caching
- **Phase B**: parallel decompilation
- **Phase C**: zero-copy P-code FFI

This file is retained as a checked-in historical benchmark summary for that optimization round.

---

## 1. Regression Validation (Phase C Flat vs JSON)

### 1.1 Unit Tests

The following tests were added in `crates/fission-pcode/src/pcode/types.rs`:

| Test | Description |
|------|-------------|
| `test_flat_roundtrip_equivalence` | `PcodeFunction` → `to_flat_bytes()` → `from_flat_bytes()` preserves the original structure exactly |
| `test_flat_vs_json_optimization_equivalence` | JSON parsing vs flat parsing yields the same block/op/varnode structure |

```bash
cargo test -p fission-pcode --lib
# 32 passed
```

### 1.2 Edge-Case Validation

- Flat-format parser checks `FPCD` magic, version, block count, and op count
- Error paths such as `Truncated`, `BadMagic`, and `TooShort` are testable and should keep expanding over time

---

## 2. Benchmark Snapshot

### 2.1 `putty.exe` (`--decomp-limit 100`)

| Metric | Value |
|--------|-------|
| Functions | 100 |
| `total_decomp_sec` | ~88.7s |
| `total_postprocess_sec` | ~0.09s |
| Wall clock (1 thread) | ~87.5s |
| Top 5 slowest | `0x14000a120(15.1s)`, `0x140007da0(13.9s)`, `0x140001160(12.8s)`, `0x14000ded0(9.4s)`, `0x140001000(8.1s)` |

### 2.2 `test_control_flow_x64_O0.exe` (`--decomp-limit 30`)

| Metric | Value |
|--------|-------|
| Functions | 30 |
| Wall clock | ~4.7s |

---

## 3. `RAYON_NUM_THREADS` Scaling

### 3.1 `test_control_flow` (30 functions)

| Threads | Wall (real) | User | Sys |
|---------|-------------|------|-----|
| 1 | 5.24s | 4.59s | 0.12s |
| 2 | 4.65s | 4.53s | 0.10s |
| 4 | 4.62s | 4.52s | 0.09s |
| 8 | 4.61s | 4.50s | 0.09s |

Interpretation: with only 30 functions, scaling is limited to roughly `1.1x`. The function set is small and the individual functions are not very heavy.

### 3.2 `putty.exe` (100 functions) — Before Round-Robin Distribution

| Threads | Wall (real) | User | Sys |
|---------|-------------|------|-----|
| 1 | 87.51s | 85.29s | 1.89s |
| 2 | 89.25s | 86.29s | 2.40s |
| 4 | 88.86s | 86.28s | 2.21s |
| 8 | 89.12s | 86.26s | 2.31s |

Cause: heavy functions clustered at low addresses, which concentrated the worst work into the first chunk handled by the main thread.

### 3.3 `putty.exe` (100 functions) — After Round-Robin Distribution (2026-03)

| Threads | Wall (real) | User | Sys | Scale |
|---------|-------------|------|-----|-------|
| 1 | 90.36s | 86.84s | 2.00s | 1.0x |
| 2 | 97.93s | 94.57s | 2.73s | 0.9x |
| 4 | 72.24s | 85.64s | 2.65s | **1.25x** |
| 8 | 41.97s | 50.74s | 0.82s | **2.15x** |

Interpretation: round-robin distribution spread the heavy functions across workers, cutting wall time to ~42 seconds on 8 cores and yielding about **2.1x improvement** over the previous ~89-second behavior.

---

## 4. Recommended Follow-Up Work

| Priority | Item | Status |
|----------|------|--------|
| 1 | **Improve chunk distribution (round-robin)** | ✅ completed in 2026-03 |
| 2 | **Step 4: FFI stability work** | ✅ completed |
| 3 | **Profiling** | Recommended — inspect internal C++ global locks and bottlenecks with flamegraphs |
| 4 | **Phase D Arena** | Deferred until the next bottleneck is confirmed |

---

## 5. How To Reproduce

```bash
# Fission-only quick validation
export DYLD_LIBRARY_PATH="$(pwd)/target/release:$DYLD_LIBRARY_PATH"
./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --ghidra-compat --profile balanced \
  --decomp-limit 100 -o artifacts/local/fission.json

# Fission NIR lane regression pass
cargo run -p fission-automation -- nir-check --lane regression --functions-limit 20
```

---

## 6. SIGSEGV (139) Status At That Time

| Item | Status |
|------|--------|
| `TypePropagator::value_is_pointer` dangling `Datatype*` path | ✅ fixed |
| `decomp_destroy` serialization (`TEARDOWN_LOCK`) | ✅ applied |
| residual intermittent crash at 8 threads | ⚠️ still possible |
| practical recommendation | prefer `RAYON_NUM_THREADS=4` or less until stability work is complete |

---

Last updated: 2026-03-08
