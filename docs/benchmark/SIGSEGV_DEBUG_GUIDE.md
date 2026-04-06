# SIGSEGV (exit 139) Debugging Guide

This document records the main crash patterns observed in the multi-threaded C++ FFI decompilation path and the mitigation steps that were taken during the 2026-03 stabilization work.

It is best treated as a **debugging reference note**, not as current architecture source of truth.

---

## Known Fixes (2026-03)

### Fix 1: `TypePropagator` (`value_is_pointer`)

**Cause:** several paths such as `getHighTypeReadFacing`, `getHighTypeDefFacing`, `temp_type`, `base_type`, and `fc->getOutputType()` could yield dangling `Datatype*` pointers, leading to a crash when `getMetatype()` was called.

**Action:**
- removed unsafe `Datatype*` access from the `value_is_pointer` lambda
- disabled unsafe `existing->getMetatype()` / `field_type` access in stack-struct field update loops

### Phase D Rollback Check

Result: crashes still occurred on both 1-thread and 8-thread runs after rolling back the Object Pool work.

Interpretation: the root cause was not Phase D Object Pool work. There was still a deeper UAF issue somewhere in the Ghidra engine path.

### UAF Fix in `apply_inferred_types`

**LLDB crash site:** `ghidra::Datatype::getMetatype()`

**Cause:** `temp_type`, `existing`, and `ep->getPtrTo()` could be dangling.

**Action:**
- removed the `temp_type` block
- removed `existing`/`inferred` `typeOrder` comparisons that dereferenced stale pointer chains

**Effect:** 1-thread runs became stable again. 8-thread runs could still crash, implying a separate race/UAF class.

### 8-Thread Crash: `Varnode::isWritten()`

**Crash site:** `ghidra::Varnode::isWritten() const` with `this=0x0`

Interpretation:
- either `op->getIn(slot)` / `high->getInstance(i)` returned null and the caller skipped null checks
- or a cross-thread access hit an already-freed `Funcdata` / `Varnode`

Recommended next actions:
1. collect the full crashing stack
2. add null checks at Fission call sites
3. reproduce under ASAN with 8 threads

### Fix 4: `UserPcodeOp::getOp()` Null Checks

**Cause:** `UserOpManage::getOp(index)` could return null for an unregistered CALLOTHER index, followed by unchecked `->getType()`.

**Action:** added null checks in:
- `jumptable.cc`
- `flow.cc`
- `typeop.cc`
- `funcdata_block.cc`
- `printc.cc`

**Effect:** removed one class of `UserPcodeOp` null-dereference crashes. More 8-thread crash sites still remained.

### 8-Thread Crash 2: `Sleigh::resolve`

**Crash site:** `ghidra::Sleigh::resolve(ParserContext&) const`

Interpretation: `SleighArchitecture::translators` was shared across multiple architectures and workers. Concurrent translation likely corrupted shared `Sleigh` / `ParserContext` state.

### Fix 5: Serialize `Sleigh::resolve` / `resolveHandles`

**Action:** introduced a global `sleigh_resolve_mutex` and wrapped both functions.

**Result:** 8-thread crashes still appeared. Sleigh locking alone was not sufficient.

### Fix 6: Guard `isWritten()` / `getDef()` Null Paths

Added null checks in:
- `TypePropagator.cc`
- `StackFrameAnalyzer.cc`
- `StructureAnalyzer.cc`
- `EmulationAnalyzer.cc`

### Fix 7: Worker-Local `DecompilerNative`

Review result: the oneshot decompile path already used per-worker independent `DecompilerNative` instances. The remaining crashes were therefore more likely caused by internal Ghidra global-state contention rather than shared Fission wrapper state.

### Fix 8: `Heritage::splitByRefinement` Container Overflow

**ASAN crash site:** `ghidra::Heritage::splitByRefinement`

**Action:** added `diff >= refine.size()` bounds checks and early-return / safe-break behavior.

### Fix 9: `Sleigh` `ContextCache` UAF and `PcodeCacher` Overflow

**ASAN findings:**
- `ContextCache::getContext` UAF
- `PcodeCacher::emit` container overflow

**Action:**
- serialized `Sleigh::reset()`
- switched the resolve mutex to `recursive_mutex`
- serialized the full `Sleigh::oneInstruction()` body

**Effect:** 8-thread limit-150 benchmark runs became much more stable, though intermittent failures were still possible.

---

## Typical Root Causes

| Cause | Description |
|------|-------------|
| **Teardown race** | concurrent `DecompilerNative::drop` → `decomp_destroy` corrupts Ghidra global state |
| **Double free / UAF** | thread-unsafe sharing in `DecompContext`, Sleigh, or type/cache internals |
| **Heap contention / corruption** | many concurrent `new` / `delete` calls expose memory bugs |

---

## Reproducing With ASAN

### 1. Build `libdecomp` With ASAN

```bash
cd legacy-native-decompiler-tree/build
cmake -S .. -B . --fresh \
  -DCMAKE_CXX_FLAGS="-fsanitize=address -fno-omit-frame-pointer -g" \
  -DCMAKE_SHARED_LINKER_FLAGS="-fsanitize=address"
cmake --build . --target decomp -j8

cd ../..
RUSTFLAGS="-L $(pwd)/legacy-native-decompiler-tree/build" \
  cargo build -p fission-cli --features legacy_native_feature --release
```

### 2. Run an 8-Thread Benchmark

```bash
export DYLD_LIBRARY_PATH="$(pwd)/target/release:$DYLD_LIBRARY_PATH"
export ASAN_OPTIONS="abort_on_error=1:halt_on_error=1:print_stacktrace=1"

RAYON_NUM_THREADS=8 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --ghidra-compat --profile balanced \
  --decomp-limit 100 -o artifacts/local/asan_output.json
```

### 3. If Needed, Rebuild Manually

```bash
cd legacy-native-decompiler-tree
rm -rf build && mkdir build && cd build

cmake .. \
  -DCMAKE_CXX_FLAGS="-fsanitize=address -fno-omit-frame-pointer -g" \
  -DCMAKE_EXE_LINKER_FLAGS="-fsanitize=address"

cmake --build . --target decomp --parallel 4
```

---

## Core Dump Workflow

### macOS (LLDB)

```bash
ulimit -c unlimited

RAYON_NUM_THREADS=8 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp-all --benchmark --decomp-limit 100 -o artifacts/local/out.json

lldb -c core ./target/release/fission_cli
(lldb) bt
(lldb) thread list
```

### Linux (GDB)

```bash
ulimit -c unlimited
gdb ./target/release/fission_cli core
(gdb) bt
(gdb) info threads
```

---

## Practical Response Strategy

### If the Crash Is in `decomp_destroy`

- move teardown onto the main thread
- serialize `decomp_destroy` calls rather than letting workers destroy contexts concurrently

### If the Crash Is in Ghidra Global State

- add locking around global initialization/teardown
- or evaluate per-process worker isolation

### If the Crash Looks Like Heap Contention

- reduce allocator churn
- revisit arena / pool strategies only after confirming the crash site

---

## Notes

- 8-thread crashes were intermittent by nature
- `RAYON_NUM_THREADS=4` was often the practical stability recommendation during this phase
- this guide is historical debugging context, not an up-to-date guarantee of the current crash surface

Last updated: 2026-03
