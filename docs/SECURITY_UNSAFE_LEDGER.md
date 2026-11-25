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
3. **Stack-Slot, Frame & Bulk-Slice Bounds** (`crates/fission-emulator/src/jit/compiler.rs`):
   - *Per slot*: wide `Copy`/`Cast`/`Load`/`Store` temporaries are capped by `MAX_JIT_STACK_BUFFER_BYTES` (`4096`; the widest varnode in any shipped SLEIGH spec is 64 bytes), matching `MAX_JIT_BULK_BYTES` in `jit_read_bytes` / `jit_write_bytes`. `CallOther` argument arrays are capped by `MAX_JIT_CALLOTHER_ARGS` (`64`, i.e. 512 bytes); callback reads use `std::ptr::read_unaligned`.
   - *Alignment*: every explicit slot is created with an explicit 8-byte alignment (`JIT_STACK_SLOT_ALIGN_SHIFT`) and accounted as its size rounded up to 8, which is exactly Cranelift's layout rule (`max(word, 1 << align_shift)`), not an implicit host-word default.
   - *Per block, before codegen*: `validate_tb_buffer_bounds` sums the aligned slot bytes of every op that can create a slot (an over-approximation of emission) with checked arithmetic and refuses a block over `MAX_JIT_TB_STACK_SLOT_BYTES` (16 KiB) before `FunctionBuilder` is created. Every emission site allocates through `create_budgeted_stack_slot`, which charges the same budget, so a drift between plan and emission is a hard error, never a larger frame.
   - *Per block, after layout and before install*: the block is compiled with `Context::compile`, its real frame (`frame_to_fp_offset` + the 16-byte FP/LR pair: explicit slots, spills, clobber saves, outgoing args) is checked against `MAX_JIT_TB_FRAME_BYTES` (32 KiB), and only then are the bytes declared/defined (`define_function_bytes`). A refused block is never defined, finalized, published to `JitCache`, or executed.
   - *Host stack depth*: soft/hard chaining nests at most `MAX_CHAIN_DEPTH + 1` (33) block frames, so compiled code uses at most ~1.06 MiB of host stack plus callback frames.
   - *Stack-clash guard*: Cranelift is configured with `enable_probestack=true`, `probestack_strategy=inline`; any frame of 4 KiB or more touches each page on entry, so overflow hits the guard page instead of skipping it. Frames under 4 KiB (ordinary blocks) get no probe code.
   - *Refusal path*: any `compile_translation_block` error resets `ctx` and `FunctionBuilderContext` and truncates `wide_ops` / `insn_reg_writes` to their pre-call lengths, so a refused block cannot poison the next compilation or grow the side tables. The run loop then interprets the block (`Emulator::run_block_interpreted`, counted in `interpreted_blocks`). This follows the crate's implemented policy (`src/interp.rs`, `src/core.rs`), which supersedes the "JIT only" wording still present in `crates/fission-emulator/AGENTS.md`.
4. **Alias, Null, and Unwind Discipline** (`crates/fission-emulator/src/jit/callbacks.rs`):
   - Every callback that takes a raw pointer is `pub unsafe extern "C" fn` with a documented contract (`emu_ptr` null or a live, unaliased `Emulator`; buffers null or valid for the stated size). A null/size check cannot validate provenance, so safe code can no longer pass an arbitrary destination (`compile_fail` doctest). The callbacks still refuse null pointers and oversized sizes defensively.
   - `jit_chain` and `jit_exit_tb` drop any `&mut Emulator` borrow before invoking chained host blocks via `call_chained_host_block`.
   - OS dispatch (`jit_call_other`, `jit_hle_trap`, the run loop's HLE trap) calls through an `Arc` clone of `Emulator::os` (now `Arc<dyn OsEnvironment>`) instead of a raw pointer into the emulator. A handler that replaces `emu.os` during its own dispatch no longer frees the layer whose method is running (regression test `os_layer_replaced_during_its_own_callother_dispatch_outlives_the_call`).
   - Heavyweight callbacks wrap execution in `guard_callback_unwind` (`std::panic::catch_unwind`), halting execution cleanly via `emu.halt_requested` rather than unwinding across `extern "C"` JIT frames.
   - `host_reg_file` base pointer: each block reads `jit_host_reg_base` once at entry and uses it until exit, including across callbacks. Sound only because nothing reachable from a callback reallocates `MachineState::host_reg_file` or replaces `emu.state` (`EmulatorSnapshot::restore_into`, `SymbolicExecutor` and `ttd_seek` run between blocks; thread switches write registers in place). Any new code path that does so mid-block must re-fetch the base.
5. **Code-Cache & W^X Lifecycle** (`crates/fission-emulator/src/jit/cache.rs`):
   - `JitCache::insert` is `unsafe` (the run loop and chaining call whatever pointer it publishes) and its tables are private, so safe code cannot publish an arbitrary host address; it still rejects null `host_func_ptr` entries and uses poison-resilient `RwLock` recovery.
   - Code memory is owned by the emulator's `JITModule`, which never frees it while the emulator lives, so invalidating a block that is still on the host stack (SMC from inside the block) cannot leave a dangling return address.
   - SMC page invalidation (`invalidate_page` / `invalidate_range`) and observer cache flushes (`flush_all`) zero hard-chain slots atomically.
   - Executable memory protection (W^X: `cranelift-jit` writes before `finalize_definitions` marks pages RX on macOS ARM64 / Linux / Windows) is exercised at runtime by compiled JIT execution tests (`crates/fission-emulator/tests/jit_host_safety.rs`).

## 3. Cranelift Dependency Review (`0.133.1`)

- Workspace uses `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`, and `cranelift-native` version `0.133.1`.
- Reachability: only `crates/fission-emulator` compiles and runs guest P-code via `JITBuilder`/`JITModule`; no untrusted WebAssembly/CLIF text parser is exposed, only internally lowered `PcodeOp` translation blocks.
- Keep `cargo audit` checks aligned with Cranelift release advisories on `cranelift-codegen` / `cranelift-jit`.

## 4. Verification Tooling Scope

- **Miri**: cannot execute Cranelift JIT-emitted native code or host OS mmap/W^X transitions. Use Miri selectively on pure-Rust subsystems (`fission-pcode-ir`, pure `pcode::eval`) rather than JIT execution.
- **Native host test coverage**: `crates/fission-emulator/tests/jit_host_safety.rs` and `fission-emulator` unit/integration suites validate compiled JIT execution, W^X code execution, mirror tail accesses, invalid FFI pointer/size rejection, cache invalidation, per-slot/aggregate/alignment/frame budget refusal, compiler reuse after a refused block, JIT-vs-interpreter agreement on a near-budget block of wide ops, and end-to-end interpreter fallback for refused blocks. `compile_fail` doctests in `jit/callbacks.rs` and `jit/cache.rs` pin the `unsafe` API boundary (`cargo test --doc -p fission-emulator`). Passing native tests do **not** constitute an exhaustive formal proof over all unsafe Rust code.

## 5. Mandatory Safety-Review Checklist for New `unsafe` Code

1. Is `unsafe` strictly necessary, or can an existing safe abstraction/checked helper replace it?
2. Does every `unsafe` block / `unsafe impl` carry an adjacent `// SAFETY:` comment stating exact preconditions?
3. Are all untrusted inputs (sizes, offsets, counts, guest pointers, alignment) validated with checked arithmetic **before** forming references, slices, or raw accesses?
4. Do external/FFI/JIT entry points guard against null pointers, prevent unwinding across `extern "C"`, and drop conflicting `&mut` references before re-entrant calls?
5. Have boundary tests been added for minimum, maximum, unaligned, overflow, and null/invalid inputs?
