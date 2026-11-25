# Production Unsafe & JIT Host-Memory Safety Ledger

Generated for Issue #1171 audit on `cranelift-*` `0.133.1`.

## 1. Production `unsafe` Inventory (Excluding `tests/`, `benches/`, `examples/`)

Baseline workspace scan recorded 237 `unsafe` blocks/items across non-test sources, concentrated in three trust tiers:

| Tier | Primary Modules | Reachability | Ownership & Invariants |
|---|---|---|---|
| **Tier 1 (Untrusted Guest Bytes)** | `crates/fission-emulator/src/jit/{callbacks.rs, compiler.rs, cache.rs}`, `crates/fission-emulator/src/core.rs` | Triggered during lifting, compilation, and JIT execution of arbitrary analyzed binaries | `JITModule` code cache, synchronous `*mut Emulator` callbacks, `host_reg_file` direct loads/stores, bulk byte FFI slices, TB chaining `transmute`s |
| **Tier 2 (Live Process / Plugin FFI)** | `crates/fission-dynamic/src/debug/platform/{macos/mach.rs, windows/**, linux/debugger.rs}`, `crates/fission-plugin/src/plugin/manager/` | Active debugging sessions and explicitly loaded plugins | Native OS debugging APIs (`mach`, Win32 debug/memory APIs, `ptrace`), dynamic plugin symbol resolution and C ABI adapters |
| **Tier 3 (Host CLI / Internal Helpers)** | `crates/fission-cli/src/cli/output.rs`, `crates/fission-signatures/`, `crates/fission-pcode/`, `crates/fission-midend-*/` | Local terminal queries, process identity, internal fast paths | Console/ioctl/Win32 queries, aligned buffer conversions, internal slice/pointer optimizations |

Hardening in #1171 removed 3 raw-pointer self-borrows (`jit_wide_op`, `jit_taint_output`, `jit_insn_checkpoint`) in `callbacks.rs`, eliminated `&mut Emulator` aliasing across recursive TB calls in `jit_chain`/`jit_exit_tb` and `jit_read_*`/`jit_write_*`, added `catch_unwind` FFI containment, and enforced width/overflow/null guards across JIT IR emission and callbacks.

## 2. Verified JIT Host-Memory-Safety Invariants

1. **Guest Address Translation**: Guest addresses in P-code `LOAD`/`STORE` never convert into host pointers. All guest RAM accesses route through `MachineState::{read_into, write_space, read_space}` (`AddressSpace` + `PageMap`), enforcing bounds and guest page permissions.
2. **Host Register Mirror (`host_reg_file`)**:
   - Enforced by `can_direct_host_reg_access(offset, size)` in `crates/fission-emulator/src/jit/compiler.rs`.
   - Only exact power-of-two widths (`1 | 2 | 4 | 8`) with overflow-checked `offset + size <= HOST_REG_FILE_SIZE` emit direct Cranelift `uload8`/`uload16`/`uload32`/`load` or `istore8`/`istore16`/`istore32`/`store`.
   - Narrow reads emit exact-width loads rather than 8-byte over-reads; odd widths (`3, 5, 6, 7`) and out-of-bounds offsets route through `jit_read_space` / `jit_write_space`.
3. **Stack-Slot & Bulk-Slice Bounds**:
   - Wide `Copy`/`Cast`/`Load`/`Store` temporaries are capped by `MAX_JIT_STACK_BUFFER_BYTES` (`4096`) via checked rounding before `create_sized_stack_slot`, matching `MAX_JIT_BULK_BYTES` in `jit_read_bytes` / `jit_write_bytes`.
   - `CallOther` argument slots are bounded by `MAX_JIT_CALLOTHER_ARGS` (`64`), and callback reads use `std::ptr::read_unaligned` after bounds validation.
4. **Alias, Null, and Unwind Discipline**:
   - Every `jit_*` callback validates `emu_ptr` and guest/buffer pointers before dereferencing.
   - `jit_chain` and `jit_exit_tb` drop any `&mut Emulator` borrow before invoking chained host blocks via `call_chained_host_block`.
   - Heavyweight callbacks wrap execution in `guard_callback_unwind` (`std::panic::catch_unwind`), halting execution cleanly via `emu.halt_requested` rather than unwinding across `extern "C"` JIT frames.
5. **Code-Cache & W^X Lifecycle**:
   - `JitCache::insert` rejects null `host_func_ptr` entries and uses poison-resilient `RwLock` recovery.
   - SMC page invalidation (`invalidate_page` / `invalidate_range`) and observer cache flushes (`flush_all`) zero hard-chain slots atomically.
   - Executable memory protection (W^X: `cranelift-jit` writes before `finalize_definitions` marks pages RX on macOS ARM64 / Linux / Windows) is exercised at runtime by compiled JIT execution tests (`crates/fission-emulator/tests/jit_host_safety.rs`).

## 3. Cranelift Dependency Review (`0.133.1`)

- Workspace uses `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`, and `cranelift-native` version `0.133.1`.
- Reachability: only `crates/fission-emulator` compiles and runs guest P-code via `JITBuilder`/`JITModule`; no untrusted WebAssembly/CLIF text parser is exposed, only internally lowered `PcodeOp` translation blocks.
- Keep `cargo audit` checks aligned with Cranelift release advisories on `cranelift-codegen` / `cranelift-jit`.

## 4. Verification Tooling Scope

- **Miri**: cannot execute Cranelift JIT-emitted native code or host OS mmap/W^X transitions. Use Miri selectively on pure-Rust subsystems (`fission-pcode-ir`, pure `pcode::eval`) rather than JIT execution.
- **Native host test coverage**: `crates/fission-emulator/tests/jit_host_safety.rs` and `fission-emulator` unit/integration suites validate compiled JIT execution, W^X code execution, mirror tail accesses, invalid FFI pointer/size rejection, and cache invalidation. Passing native tests do **not** constitute an exhaustive formal proof over all unsafe Rust code.

## 5. Mandatory Safety-Review Checklist for New `unsafe` Code

1. Is `unsafe` strictly necessary, or can an existing safe abstraction/checked helper replace it?
2. Does every `unsafe` block / `unsafe impl` carry an adjacent `// SAFETY:` comment stating exact preconditions?
3. Are all untrusted inputs (sizes, offsets, counts, guest pointers, alignment) validated with checked arithmetic **before** forming references, slices, or raw accesses?
4. Do external/FFI/JIT entry points guard against null pointers, prevent unwinding across `extern "C"`, and drop conflicting `&mut` references before re-entrant calls?
5. Have boundary tests been added for minimum, maximum, unaligned, overflow, and null/invalid inputs?
