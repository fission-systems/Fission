# Fission Testing Architecture

## Current System Structure

### PyGhidra Is Already in Use

Fission already uses **pyghidra 2.2.1** to automate Ghidra in headless workflows.

```
Test flow:
┌─────────────────────────────────────────────────────────┐
│  run_complex_tests.py (automation runner)              │
│  ├─ Iterates over 6 test cases                         │
│  └─ Calls compare_decompilers_v2.py for each test      │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  compare_decompilers_v2.py (comparison engine)         │
│  ├─ Ghidra: calls pyghidra_decompile.py                │
│  ├─ Fission: calls fission_cli --decomp                │
│  └─ Compares outputs and computes similarity           │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  pyghidra_decompile.py (Ghidra wrapper)                │
│  └─ Uses pyghidra.open_program()                       │
│     ├─ Loads the binary and runs auto-analysis         │
│     ├─ Extracts function disassembly                   │
│     └─ Produces decompilation output                   │
└─────────────────────────────────────────────────────────┘
```

## Why PyGhidra Is Useful

### 1. Automation-Friendly
```python
with pyghidra.open_program(binary_path, analyze=True) as flat_api:
    program = flat_api.getCurrentProgram()
    # Direct access to the Ghidra API
```

### 2. No GUI Required
- Runs in headless mode
- Works well in CI/CD pipelines
- Usable on server environments

### 3. Fast to Launch
- No Ghidra GUI startup cost
- Direct Python control
- Easy project creation and management automation

### 4. Operationally Stable
- Java process isolation
- Timeout control
- Straightforward error handling

## Current Implementation

### 1. `pyghidra_decompile.py`
```python
# Location: scripts/ghidra/pyghidra_decompile.py

Key functionality:
+ Load binaries (PE/ELF supported)
+ Run auto-analysis (`analyze=True`)
+ Extract function disassembly
+ Run decompilation
+ Format output

Output format:
- Assembly listing (address, mnemonic, operands)
- Decompiled code (C-like output)
- Function metadata
```

### 2. `compare_decompilers_v2.py`
```python
# Location: scripts/compare/compare_decompilers_v2.py

Key functionality:
+ Run Ghidra and Fission side by side
+ Normalize output (strip ANSI, filter noise)
+ Compute similarity (`difflib`)
+ Measure timing
+ Produce JSON/HTML reports

Comparison metrics:
- Line-by-line similarity
- Code structure analysis
- Performance comparison (execution time)
```

### 3. `run_complex_tests.py`
```python
# Location: scripts/run_complex_tests.py

Key functionality:
+ Run 6 complex tests automatically
+ Organize results by category
+ Compute summary statistics (difficulty, category)
+ Generate HTML reports
+ Show live progress

Test groups:
1. Control Flow (3)
2. Data Structures (1)
3. Pointers (1)
4. C++ Features (1)
```

## PyGhidra Configuration

### Current Environment
```bash
PyGhidra Version: 2.2.1
Ghidra Path: /path/to/ghidra_11.4.2_PUBLIC
Python: 3.x
```

### Verification
```bash
# Check PyGhidra installation
python3 -c "import pyghidra; print(pyghidra.__version__)"

# Check Ghidra path
echo $GHIDRA_INSTALL_DIR

# Or verify the auto-configuration in the script
# scripts/ghidra/pyghidra_decompile.py:15-16
```

## Performance Considerations

### 1. Analysis Caching
PyGhidra automatically creates and caches Ghidra projects:
```
~/.local/share/pyghidra/projects/
├── project_ABC123/
│   ├── project.gpr
│   ├── project.rep/
│   └── ...
```

### 2. Parallel Execution Potential
The current flow is sequential, but it can be improved:
```python
from concurrent.futures import ProcessPoolExecutor

# Process multiple binaries in parallel
with ProcessPoolExecutor(max_workers=4) as executor:
    futures = [executor.submit(run_test, test) for test in tests]
```

Warning: Ghidra is memory-heavy, so concurrency limits are important.

### 3. Timeout Configuration
```python
# compare_decompilers_v2.py
timeout=600  # 10 minutes

# Large binaries may need a longer timeout
timeout=1200  # 20 minutes
```

## Example Test Runs

### Simple Test
```bash
# Single-function decompilation via PyGhidra
python3 scripts/ghidra/pyghidra_decompile.py \
    examples/bin_x64/nested_loops_x64.exe \
    0x450
```

### Comparison Test
```bash
# Ghidra vs Fission comparison
python3 scripts/compare_decompilers_v2.py \
    examples/bin_x64/nested_loops_x64.exe \
    examples/addresses/nested_loops_addrs.txt \
    scripts/result_nested_loops \
    --batch
```

### Full Test Suite
```bash
# Run all complex tests
python3 scripts/run_complex_tests.py
```

## Troubleshooting

### 1. PyGhidra Installation Problems
```bash
# Reinstall
pip3 uninstall pyghidra
pip3 install pyghidra

# Or use the development version
pip3 install git+https://github.com/Defense-Cyber-Crime-Center/pyghidra.git
```

### 2. Ghidra Path Problems
```bash
# Set the environment variable
export GHIDRA_INSTALL_DIR=/path/to/ghidra_11.4.2_PUBLIC

# Or change the script directly
# scripts/ghidra/pyghidra_decompile.py:15
ghidra_path = "/custom/path/to/ghidra"
```

### 3. Out of Memory
```bash
# Increase the Ghidra JVM heap
export _JAVA_OPTIONS="-Xmx8G"

# Or reduce concurrency
max_workers=2
```

### 4. Java Version Problems
```bash
# Ghidra 11.4.2 requires Java 17+
java -version

# Set Java explicitly
export JAVA_HOME=/path/to/java17
```

## Future Improvements

### Already Implemented
- [x] PyGhidra integration
- [x] Automated test execution
- [x] Similarity calculation
- [x] HTML report generation

### Possible Improvements
- [ ] Parallel execution with memory-aware scheduling
- [ ] Better caching of analysis results
- [ ] Dynamic timeout adjustment
- [ ] Retry logic for failed tests

### Potential New Features
- [ ] Live dashboard
- [ ] Historical tracking across versions
- [ ] CI/CD integration
- [ ] Slack/email notifications

## References

- **PyGhidra docs**: https://github.com/Defense-Cyber-Crime-Center/pyghidra
- **Ghidra API**: https://ghidra.re/ghidra_docs/api/
- **Project structure**: `scripts/README.md`
- **Test guide**: `examples/README_TESTS.md`

## Summary

PyGhidra is already part of the Fission testing stack.

The current system provides:
- Ghidra automation through PyGhidra
- Fully headless execution
- A fast and stable testing pipeline
- Broad result analysis and reporting

You can start running tests immediately with:

```bash
python3 scripts/run_complex_tests.py
```
