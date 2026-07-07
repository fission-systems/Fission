# fission-emulator Agent Guide

Generated: 2026-07-07
Scope: `crates/fission-emulator`

## Overview

`fission-emulator` is Fission's pure-Rust P-Code execution engine. It evaluates Ghidra/Sleigh P-Code operations directly, providing a dynamic analysis platform for taint tracking, time-travel debugging, and concolic path exploration.

This crate is a **dynamic analysis pillar**, not a decompiler semantic repair layer. Do not use it to patch decompiler output.

## Module Map

| Module | File | Purpose |
|---|---|---|
| `Emulator` | `src/core.rs` | Main state machine: run loop, TTD hooks, register I/O, arch-agnostic CC helpers |
| `MachineState` | `src/pcode/state.rs` | Address spaces, shadow memory (taint map), memory trace buffers |
| `Evaluator` | `src/pcode/eval.rs` | P-Code opcode dispatch, taint propagation, CBranch emission |
| `SymbolicExecutor` | `src/sym/mod.rs` | TTD-backed concolic exploration driver |
| `TraceLog` | `src/trace.rs` | Per-instruction audit trail |
| Linux HLE | `src/os/linux/mod.rs` | Linux syscall HLE: read, write, mmap, brk, exit, stat, open, close |
| Windows HLE | `src/os/windows/hle.rs` | Windows HLE stubs: VirtualAlloc, HeapAlloc, WriteFile, ExitProcess |
| Bare Metal HLE | `src/os/bare_metal/mod.rs` | Minimal semihosting-style HLE |
| Arch descriptors | `src/arch/` | Architecture metadata, calling convention traits, register maps |

## Key Invariants

1. **No external runtime** — The emulator must never depend on QEMU, Unicorn, Capstone, or any native C/C++ binary emulation library. It evaluates P-Code semantics directly.
2. **Taint is per-byte** — Shadow memory tracks taint at individual byte granularity via `shadow_memory: HashMap<(space_id, addr), SymNodeId>`.
3. **Concrete write clears taint** — When `write_space()` stores concrete bytes, the corresponding shadow entries are removed.
4. **TTD snapshots are complete** — Each `ExecutionSnapshot` must include `MemoryDelta` and `ShadowDelta` so that `ttd_seek()` can fully reconstruct both concrete and symbolic state.
5. **Taint sources are explicit** — Only designated OS HLE handlers (e.g., `sys_read`) may tag new `SymExpr::Var` nodes as taint sources. Do not introduce implicit taint sources in the evaluator.
6. **CBranch emits SymBranch events** — Every conditional P-Code branch records a `SymBranch` so the `SymbolicExecutor` can queue alternate paths.
7. **Emulator is not a semantic repair layer** — If decompiler output is wrong, fix it in `fission-pcode`, not here.

## Where To Look

| Task | Location |
|---|---|
| Add a new syscall handler (Linux) | `src/os/linux/mod.rs` |
| Add a new syscall handler (Windows) | `src/os/windows/hle.rs` |
| Add taint propagation to a new P-Code opcode | `src/pcode/eval.rs` — find the opcode match arm and add shadow read/write calls |
| Change how shadow memory works | `src/pcode/state.rs` — `shadow_memory`, `set_shadow_memory`, `trace_shadow_writes` |
| Change TTD snapshot interval or format | `src/core.rs` — `run()` loop, `ttd_seek()` |
| Add a new architecture | `src/arch/` — implement `CallingConvention` trait and register map |
| Change exploration strategy | `src/sym/mod.rs` — `SymbolicExecutor::explore()` |
| Change how taint variables are named | `src/os/linux/mod.rs` — `sys_read` handler |

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

- Do not add QEMU, Unicorn, or Capstone as dependencies.
- Do not hardcode register offsets or syscall numbers for specific binaries.
- Do not invent new calling convention or type rules in HLE handlers — consume ABI facts from `arch/`.
- Do not taint data that did not come from a recognized taint source.
- Do not use the emulator as a decompiler semantic patch.
- Do not add Z3, STP, or any C++ SMT solver as a dependency. The solver must remain pure Rust via `fission-solver`.
- Do not record shadow deltas for memory spaces that are not `ram` (space_id=3) or `register` (space_id=2) unless you add a new canonical taint space.

## Build / Test Commands

```bash
# Type-check emulator crate
cargo check -p fission-emulator

# Run all emulator tests (if any)
cargo nextest run -p fission-emulator

# Type-check solver
cargo check -p fission-solver

# Type-check downstream CLI
cargo check -p fission-cli

# Build release CLI (includes emulator)
cargo build -p fission-cli --release
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
- `vendor/angr-master/` — Reference implementation (read-only reference, no dependencies)
- Root `AGENTS.md` — Repository-level rules that override anything here if they conflict
