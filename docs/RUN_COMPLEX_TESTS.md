# Complex Test Suite Guide

## CI vs Full Quality Runs

- **CI (every commit / PR)**
  - does **not** download or execute Ghidra
  - runs only a **lightweight Fission-only smoke test**:
    `scripts/test/decomp_smoke_ci.sh` decompiles one small C++ test binary and checks for exit code `0` (no crash)
- **Full quality benchmark (with Ghidra)**
  - scripts such as `run_complex_tests.py` and `compare_decompilers_v2.py` are intended for **local runs**
  - or for **scheduled / manually triggered workflows** such as the `Decompilation Quality` workflow
  - full Ghidra quality comparison is intentionally not run on every commit because of CI cost and runtime

## Overview

`run_complex_tests.py` runs six complex test cases automatically and compares Fission output against Ghidra.

## Test Cases

| # | Test | Category | Difficulty | Description |
|---|------|----------|------------|-------------|
| 1 | **Nested Loops** | Control Flow | ⭐⭐⭐ | nested loops, break/continue, goto |
| 2 | **Switch-Case** | Control Flow | ⭐⭐ | fall-through, nested switch |
| 3 | **Recursion** | Control Flow | ⭐⭐⭐⭐ | simple, multiple, and mutual recursion |
| 4 | **Complex Structs** | Data Structures | ⭐⭐⭐⭐ | nested structs, union usage |
| 5 | **Function Pointers** | Pointers | ⭐⭐⭐⭐⭐ | function pointers, callbacks |
| 6 | **Virtual Functions** | C++ Features | ⭐⭐⭐⭐⭐ | virtual functions, vtables |

## Prerequisites

### 1. Build the Test Binaries
```bash
cd test
./build_all_tests.sh
./extract_functions.sh
```

### 2. Required Tooling
- Python 3.6+
- Ghidra (environment variables configured as needed)
- Fission built locally

## Running the Suite

### Run All Tests
```bash
cd /path/to/Fission
python3 scripts/run_complex_tests.py
```

### Example Output
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            🧪 Fission Complex Test Suite Runner
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Output directory: scripts/result_complex_tests_20260108_170000
Total test cases: 6

─────────────────────────────────────────────────────────────────
                          🚀 Running Tests
─────────────────────────────────────────────────────────────────

[1/6] [Control Flow] Nested Loops
  Binary: examples/bin_x64/nested_loops_x64.exe
  Difficulty: ⭐⭐⭐
  Functions to test: 56
  Running comparison...
  ✓ Complete
  Similarity: 85.32%
  Duration: 45.2s

[2/6] [Control Flow] Switch-Case
  ...
```

## Generated Artifacts

### 1. Directory Structure
```
scripts/result_complex_tests_YYYYMMDD_HHMMSS/
├── result_nested_loops/
│   ├── comparison_summary.json
│   ├── addr_0x450_*.txt
│   └── ...
├── result_switch_case/
├── result_recursion/
├── result_complex_structs/
├── result_function_pointers/
├── result_virtual_functions/
├── complex_tests_summary.json
└── complex_tests_report.html
```

### 2. JSON Summary (`complex_tests_summary.json`)
```json
{
  "timestamp": "2026-01-08T17:00:00",
  "total_tests": 6,
  "success": 6,
  "failed": 0,
  "timeout": 0,
  "average_similarity": 82.45,
  "total_duration": 324.5,
  "tests": []
}
```

### 3. HTML Report (`complex_tests_report.html`)
- visual dashboard
- category-by-category results
- color-coded similarity values
- clickable drill-down details

## Interpreting Results

### Similarity Tiers
- **90%+**: 🟢 Excellent
- **80-89%**: 🔵 Good
- **70-79%**: 🟡 Fair
- **<70%**: 🔴 Poor

### Expected Ranges
| Test | Expected Similarity |
|------|---------------------|
| Switch-Case | 90-95% |
| Nested Loops | 85-90% |
| Recursion | 80-85% |
| Complex Structs | 75-85% |
| Function Pointers | 70-80% |
| Virtual Functions | 65-75% |

## Advanced Usage

### Run Selected Tests Only
Edit `run_complex_tests.py` to keep only the cases you want:

```python
TEST_CASES = [
    # Keep only the desired tests here
]
```

### Adjust Timeout
```python
# Default: 600 seconds
timeout = 600
```

## Troubleshooting

### 1. Ghidra Path Errors
```bash
export GHIDRA_HOME=/path/to/ghidra_11.4.2_PUBLIC
```

### 2. Fission Build Errors
```bash
cd /path/to/Fission
cargo build --release
```

### 3. Missing Address Files
```bash
cd test
./extract_functions.sh
```

### 4. Out of Memory
- run tests individually
- increase the Ghidra heap size

## Performance Notes

### Parallel Execution
The current flow is sequential, but it can be extended if needed:

```python
from concurrent.futures import ThreadPoolExecutor
# Add parallel execution logic if needed
```

### Caching
- reuse Ghidra projects
- persist intermediate results

## How to Use the Results

### 1. Prioritize Improvements
Focus first on categories with the lowest similarity.

### 2. Run Regression Checks
Re-run after code changes to confirm quality is preserved.

### 3. Benchmark Versions
Compare output quality and speed across revisions.

### 4. Document Findings
Record known limitations and strong cases as they are discovered.

## Additional References

- test sources: `examples/control_flow/`, `examples/data_structures/`, etc.
- comparison script: `scripts/compare_decompilers_v2.py`
- test build guide: `examples/README_TESTS.md`

## Support and Contributions

If you find a bug or have an improvement idea, open a GitHub issue.
