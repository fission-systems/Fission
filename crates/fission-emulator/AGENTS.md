# fission-emulator Agent Guide

Generated: 2026-07-09
Scope: `crates/fission-emulator`

## Overview

`fission-emulator` is Fission's pure-Rust **JIT-only** guest execution engine. Guest instructions are lifted with Sleigh to P-Code, then compiled to host code with Cranelift. It supports taint tracking, time-travel debugging, OS HLE, and concolic exploration around that JIT path.

This crate is a **dynamic analysis pillar**, not a decompiler semantic repair layer. Do not use it to patch decompiler output.

**Execution policy: JIT only.** Do not restore a P-Code interpreter as a runtime fallback. Expand Cranelift lowering and host callouts instead.

**QEMU reference:** `vendor/qemu-11.0.2` may be consulted for structure (TB cache, page protection, linux-user mmap/brk, SMC invalidation). Never link, bind, shell out to, or copy QEMU into production paths. See `docs/plans/emulator-jit-only-roadmap.md`.

## Module Map

| Module | File | Purpose |
|---|---|---|
| `Emulator` | `src/core.rs` | Main state machine: JIT run loop, TTD hooks, register I/O, arch-agnostic CC helpers |
| `MachineState` | `src/pcode/state.rs` | Address spaces, shadow memory (taint map), page map, memory trace buffers |
| `PageMap` | `src/pcode/page_map.rs` | Guest virtual map + R/W/X protections + mmap/brk |
| `SpaceLayout` | `src/pcode/spaces.rs` | SLA-native ram/register/unique indices |
| `Evaluator` | `src/pcode/eval.rs` | Offline / symbolic helper dispatch (not the primary run loop) |
| `JitCompiler` | `src/jit/compiler.rs` | Multi-insn TB: P-Code → Cranelift (sole execution engine) |
| `JitCache` | `src/jit/cache.rs` | Guest PC → host TB; page-level SMC invalidation |
| JIT callbacks | `src/jit/callbacks.rs` | space I/O, float, bulk bytes, CallOther, soft chain |
| `SymbolicExecutor` | `src/sym/mod.rs` | TTD-backed concolic exploration driver |
| `TraceLog` | `src/trace.rs` | Per-instruction audit trail |
| Linux HLE | `src/os/linux/mod.rs` | Linux syscall HLE + `image_info` + `signal` delivery |
| Windows HLE | `src/os/windows/hle.rs` | Win32 HLE; PE `image_info` (stack/PEB/TEB/heap) |
| Bare Metal HLE | `src/os/bare_metal/mod.rs` | Minimal semihosting-style HLE |
| Arch descriptors | `src/arch/` | Architecture metadata, calling convention traits, register maps |

## Key Invariants

1. **JIT only** — Runtime execution is Cranelift-compiled blocks only. Compile failure is a hard error, not a silent skip and not an interpreter fallback.
2. **No external runtime** — Never depend on QEMU, Unicorn, Capstone, or any native C/C++ binary emulation library.
3. **Taint is per-byte** — Shadow memory tracks taint at individual byte granularity via page-level symbolic maps.
4. **Concrete write clears taint** — When `write_space()` stores concrete bytes, the corresponding shadow entries are removed.
5. **TTD snapshots are complete** — Each `ExecutionSnapshot` must include `MemoryDelta` and `ShadowDelta` so that `ttd_seek()` can fully reconstruct both concrete and symbolic state.
6. **Taint sources are explicit** — Only designated OS HLE handlers (e.g., `sys_read`) may tag new `SymExpr::Var` nodes as taint sources.
7. **SMC invalidation** — Writes that touch EXEC-mapped guest pages must invalidate `JitCache` entries for those pages.
8. **Emulator is not a semantic repair layer** — If decompiler output is wrong, fix it in `fission-pcode`, not here.

## Where To Look

| Task | Location |
|---|---|
| Lower a new P-Code opcode in the JIT | `src/jit/compiler.rs` |
| Host callouts (mem/reg/userop) | `src/jit/callbacks.rs` |
| JIT cache / SMC invalidation | `src/jit/cache.rs`, `jit_write_space` |
| Guest page map / mmap / brk | `src/pcode/page_map.rs`, `src/os/linux/syscall.rs` |
| Add a new syscall handler (Linux) | `src/os/linux/mod.rs` + `syscall.rs` |
| Add a new syscall handler (Windows) | `src/os/windows/hle.rs` |
| Symbolic / taint helpers | `src/pcode/eval.rs`, `src/pcode/state.rs` |
| Change TTD snapshot interval or format | `src/core.rs` — `run()` loop, `ttd_seek()` |
| Add a new architecture | `src/arch/` — implement `CallingConvention` trait and register map |
| Change exploration strategy | `src/sym/mod.rs` — `SymbolicExecutor::explore()` |
| Roadmap | `docs/plans/emulator-jit-only-roadmap.md` |

## Core Data Flow

```
stdin (--stdin mock)
  → sys_read() in os/linux/mod.rs
    → write bytes to MachineState RAM
    → call solver.register_var() for each byte
    → tag shadow_memory entries
      → MachineState.shadow_memory[(3, addr)] = SymNodeId

P-Code LOAD/COPY/STORE/INT_ADD/...
  → Evaluator reads shadow_memory for inputs
  → If any input is tainted:
      → build SymExpr::Add/Sub/... from fission-solver
      → solver.register_node(expr) → new SymNodeId
      → write new SymNodeId to output shadow_memory

CBranch P-Code op
  → if TTD is recording:
      → push SymBranch { step_index, pc, alt_addr, alt_rel_idx } to emu.sym_events
  → evaluate concrete condition → take taken path

TTD snapshot (every N instructions)
  → collect trace_mem_writes as MemoryDelta
  → collect trace_shadow_writes as ShadowDelta
  → call ttd.record_step_with_memory(regs, 0, mem_deltas, shadow_deltas)

SymbolicExecutor.explore()
  → emu.run() → drain sym_events into queue
  → pop SymBranch from queue
  → emu.ttd_seek(step) → restore registers + memory + shadow
  → force PC to alt_addr (or log rel_idx limitation)
  → repeat
```

## Anti-Patterns

- Do not reintroduce a P-Code interpreter as the run-loop execution engine.
- Do not add QEMU, Unicorn, or Capstone as dependencies (vendor QEMU is reference-only).
- Do not hardcode register offsets or syscall numbers for specific binaries.
- Do not invent new calling convention or type rules in HLE handlers — consume ABI facts from `arch/`.
- Do not taint data that did not come from a recognized taint source.
- Do not use the emulator as a decompiler semantic patch.
- Do not add Z3, STP, or any C++ SMT solver as a dependency. The solver must remain pure Rust via `fission-solver`.
- Do not record shadow deltas for memory spaces that are not `ram` (space_id=3) or `register` (space_id=2) unless you add a new canonical taint space.
- Do not silently skip instructions on JIT compile failure.

## Build / Test Commands

```bash
# Type-check emulator crate
cargo check -p fission-emulator

# Unit + optional smoke tests
cargo nextest run -p fission-emulator

# Optional real-binary smoke (static musl x86_64 hello)
#   zig cc -target x86_64-linux-musl -O0 -o /tmp/fission-emu-test/hello_linux_x64 hello.c
#   FISSION_SMOKE_ELF=/tmp/fission-emu-test/hello_linux_x64 cargo nextest run -p fission-emulator smoke_linux

# Sandbox CLI
cargo build -p fission-cli --release
./target/release/fission_cli sandbox /path/to/elf --max-inst 50000
# End-of-run log line: "Emulator metrics: ..."
```

## Workflow Bias

- When adding a new syscall HLE: add the match arm in the appropriate `os/` module, read arguments with `emu.read_arg(n)`, write return value with `emu.write_return_val(v)`, simulate return with `emu.simulate_return()`.
- When adding taint propagation: find the P-Code opcode match arm in `eval.rs`, call `self.read_varnode_shadow(&vn)` on inputs, build the `SymExpr` formula, call `self.solver.register_node(expr)`, then `self.write_varnode_shadow(out, id)`.
- When debugging TTD rewind: check that both `MemoryDelta` and `ShadowDelta` are being collected in `run()`, and that `ttd_seek()` applies both.
- When debugging taint flow: enable `RUST_LOG=debug` and look for shadow reads/writes in the evaluator trace.

## Child AGENTS Files

None currently. Add them if `os/`, `arch/`, or `sym/` grow large enough to warrant their own engineering guides.

## References

- `crates/fission-ttd/` — TTD recording and snapshot primitives
- `crates/fission-solver/` — Pure-Rust SMT constraint engine
- `crates/fission-sleigh/` — Produces P-Code ops consumed by this evaluator
- `crates/fission-cli/src/cli/oneshot/mod.rs` — CLI surface integrating the emulator
- `vendor/qemu-11.0.2/` — Structural reference only (TB, pages, linux-user); no dependencies
- `vendor/angr-master/` — Symbolic/HLE reference only; no dependencies
- `docs/plans/emulator-jit-only-roadmap.md` — JIT-only roadmap
- Root `AGENTS.md` — Repository-level rules that override anything here if they conflict
