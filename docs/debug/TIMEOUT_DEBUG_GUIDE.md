# Timeout / Infinite-Loop Debugging Guide

If `--decomp-limit 20` hits a 900-second timeout, the issue is likely not ordinary slowness. It is more often a true **infinite loop** or a bug such as **catastrophic regex backtracking**. This guide explains how to identify the culprit function and profile the bottleneck.

---

## Step 1: Identify the Function Causing the Timeout

### Automated Script (Recommended)

```bash
# Test the first 20 functions with a 120-second per-function timeout
python scripts/test/batch_benchmark/find_timeout_culprit.py samples/windows/x64/putty.exe --limit 20 --timeout 120

# Print detailed timing for each function
python scripts/test/batch_benchmark/find_timeout_culprit.py putty.exe --limit 20 --timeout 120 --verbose
```

**How to read the results:**
- An address marked `[TIMEOUT]`: did not finish within 120 seconds → **primary suspect**
- `[OK]` but slower than 60 seconds: likely an extreme bottleneck
- If all functions finish: the problem may come from **parallel execution interactions** (race/lock behavior) or **initialization overhead**. Try the “disable parallelism” step below.

### Manual Run (Single Function)

```bash
# Inspect the function list to find candidate addresses
./target/release/fission_cli samples/windows/x64/putty.exe -l --json | head -80

# Decompile just one function with a 120-second timeout
timeout 120 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 --benchmark --ghidra-compat -o artifacts/local/out.json
```

**Note:** `timeout` comes from GNU coreutils. On macOS, use `gtimeout` (`brew install coreutils`) or run with `time` and interrupt manually with `Ctrl+C`.

### Disable Parallelism to Separate Causes

If a single function finishes but `--decomp-all --decomp-limit 20` times out, the issue may be parallel execution rather than one specific function.

```bash
# Run single-threaded (sequential)
RAYON_NUM_THREADS=1 python scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe --limit 20 --timeout 600
```

- If it finishes with `RAYON_NUM_THREADS=1`: suspect a parallelism or locking issue
- If it still times out: suspect a per-function infinite loop or very heavy initialization

---

## Step 2: Profile the Rust Side

Once you have a suspect function, use profiling tools to see where CPU time is going.

### cargo-flamegraph (Recommended)

```bash
cargo install flamegraph

# Profile decompilation of the suspect function
cargo flamegraph --bin fission_cli -- \
  samples/windows/x64/putty.exe --decomp 0x140001160 --benchmark -o artifacts/local/out.json
```

Wide bars in `flamegraph.svg` indicate hotspots.

**Common bottleneck candidates:**
- `cfg_structurizer` / `postprocess` → CFG structuring or string/regex work
- `main_perform` / FFI → native Ghidra call path
- `analysis_passes` → analysis loop overhead

### macOS Instruments (Time Profiler)

```bash
xcrun xctrace record --template 'Time Profiler' -- \
  ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 --benchmark -o artifacts/local/out.json
```

### samply (Cross-Platform)

```bash
cargo install samply
samply record ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 -o artifacts/local/out.json
```

---

## Step 3: Check FFI Cost and Caching

For each decompiled function, verify whether the following work is being repeated unnecessarily:

- [ ] **Signature JSON**: is the full signature set parsed every time?
- [ ] **GDT/type DB**: is it loaded once per binary, or repeatedly?
- [ ] **Serialization**: how often is JSON serialized/deserialized across the Ghidra ↔ Rust boundary?

Relevant code:
- `fission-ffi/src/decomp/` — FFI boundary
- `fission-analysis/src/analysis/decomp/prepare.rs` — initialization and prepare options

---

## Step 4: Inspect the Most Suspicious Areas

### postprocess (for example string inlining)

- Check whether regex patterns are vulnerable to ReDoS (Regular Expression Denial of Service)
- On long inputs, patterns like `.*` or `(.*)*` can cause catastrophic backtracking
- If using the `regex` crate, consider bounded matching patterns rather than unconstrained scans

### cfg_structurizer

- Check for infinite loops caused by cyclic graph traversal
- Verify that visited-node tracking is not missing

### Ghidra native (`main_perform`)

- Check whether the C++ `DecompilationCore` loops forever on a specific IR shape
- Increase logging and inspect progress directly in the C++ build

---

## Step 5: Check Progress Through Logs

```bash
# Rust-side verbose logging
./target/release/fission_cli putty.exe --decomp 0x140001160 --verbose 2>&1 | tee debug.log

# Trace selected modules
RUST_LOG=fission_analysis=trace,fission_ffi=debug cargo run -p fission-cli -- putty.exe --decomp 0x140001160
```

Pay attention to where execution stops after initialization (FID, GDT load, etc.) and what the last stderr message was.

---

## Reference: Known Slow Functions in `putty.exe`

Top bottlenecks from benchmark runs:

| Address | Function | Notes (`putty-limit100-final`) |
|---------|----------|--------------------------------|
| `0x140001160` | `FUN_0x140001160` | 12.9s, `main_perform` + `follow_flow` |
| `0x140007da0` | `FUN_0x140007da0` | 13.9s, `postprocess` + `cfg_structurizer` |
| `0x14000a120` | `FUN_0x14000a120` | 16.1s, `postprocess` + `cfg_structurizer` |

These are strong timeout suspects. Test them individually first with commands such as `--decomp 0x140001160`.
