# Dynamic analysis: emulator, TTD, symbolic execution, concolic exploration

Moved out of `README.md` when that file was reduced to an orientation
document. This is the reference for Fission's dynamic side: the emulator,
time-travel debugging, the symbolic/taint engine, and concolic path
exploration.

## Dynamic Analysis and Emulator

Fission includes a pure-Rust dynamic analysis engine (`fission-emulator`) capable of executing arbitrary x86/x86-64 P-Code sequences produced by Sleigh, without requiring any external runtime like QEMU or Unicorn.

The emulator is a core pillar for future dynamic reasoning capabilities:
- **Tracing and Differential Analysis** — Execute binaries with a full instruction trace and diff the output against expected behavior.
- **Time-Travel Debugging (TTD)** — Record/replay execution with memory and register delta snapshots.
- **Concolic Execution** — Use TTD snapshots to fork execution at unexplored conditional branches.
- **Symbolic Taint Tracking** — Propagate symbolic expressions through P-Code operations and track tainted data flows from inputs (e.g. `stdin`) through memory and registers.

### Architecture

```
fission-emulator
  ├── core.rs          — Emulator state, main run loop, register I/O, TTD hooks
  ├── pcode/
  │   ├── eval.rs      — P-Code opcode evaluator (taint-aware)
  │   └── state.rs     — MachineState: address spaces + shadow memory
  ├── arch/            — Architecture descriptors, calling conventions
  ├── os/
  │   ├── linux/       — Linux syscall HLE (read, write, mmap, brk, exit, ...)
  │   ├── windows/     — Windows HLE stubs
  │   └── bare_metal/  — No-OS / embedded HLE
  ├── sym/
  │   └── mod.rs       — SymbolicExecutor: TTD-backed concolic path exploration
  ├── trace.rs         — TraceLog: per-instruction audit trail
  ├── snapshot.rs      — Lightweight snapshot helpers
  └── loader.rs        — Binary loading bridge for the emulator
```

### CLI Integration

```bash
# Emulate a binary (default: auto-detect OS, no limits)
fission_cli run <binary>

# Limit to N instructions
fission_cli run <binary> --max-inst 10000

# Provide stdin mock data
fission_cli run <binary> --stdin "hello\n"

# Emit full instruction trace (JSON)
fission_cli run <binary> --trace --json

# Enable TTD recording (snapshot every 1000 instructions)
fission_cli run <binary> --ttd-record 1000

# Seek TTD to step N (rewind and replay from that point)
fission_cli run <binary> --ttd-record 1000 --ttd-seek 5000

# Enable concolic path exploration
fission_cli run <binary> --ttd-record 500 --sym-explore
```

### Operating System HLE

The emulator uses High-Level Emulation (HLE) rather than emulating an actual OS kernel. Each architecture+OS pair has its own HLE handler:

| OS | HLE Handler | Key syscalls |
|---|---|---|
| Linux x86-64 | `os/linux/mod.rs` | `read`, `write`, `mmap`, `brk`, `exit_group`, `open`, `close`, `fstat`, `stat` |
| Windows x86-64 | `os/windows/hle.rs` | `VirtualAlloc`, `HeapAlloc`, `WriteFile`, `ExitProcess`, and common NT stubs |
| Bare Metal | `os/bare_metal/mod.rs` | Minimal: `semihosting` stubs only |

HLE handlers can intercept function calls by name (via PLT/IAT hooks) or by recognized calling patterns.

### Calling Convention Support

The `arch/` subsystem provides architecture-agnostic calling convention helpers:

- `Emulator::read_arg(n)` — read the nth integer argument per the current ABI
- `Emulator::write_return_val(v)` — write to the return register
- `Emulator::simulate_return()` — pop the stack or read the link register and set PC

Currently supported: **System V AMD64 ABI** (Linux x86-64), **Microsoft x64** (Windows), and a stub for **ARM AArch64**.

---

## Time-Travel Debugging (TTD)

The `fission-ttd` crate provides a deterministic execution recorder and replayer. It is the backbone for all forms of non-linear execution analysis.

### How TTD Works

1. **Recording Phase** — As the `Emulator` executes, every N instructions (configurable via `--ttd-record <N>`) it captures:
   - Complete CPU register state (`RegisterState`)
   - Memory write deltas: `(address, old_bytes, new_bytes)`
   - Shadow state (taint) deltas: `(space_id, address, old_node_id, new_node_id)`

2. **Snapshot Storage** — Snapshots are stored in a bounded ring buffer (`TTDRecorder`) with configurable capacity. Older snapshots are evicted when capacity is reached.

3. **Seeking / Rewinding** — `Emulator::ttd_seek(step)` locates the most recent snapshot at or before `step`, then restores:
   - All GP registers from `RegisterState`
   - All memory bytes from `MemoryDelta` records
   - All symbolic taint mappings from `ShadowDelta` records

4. **Replay** — After rewinding, `emulator.run()` continues forward from the restored state.

### TTD Data Structures

| Type | Crate | Purpose |
|---|---|---|
| `TTDRecorder` | `fission-ttd` | Manages the ring buffer of `ExecutionSnapshot`s |
| `ExecutionSnapshot` | `fission-ttd` | Single-point snapshot: registers + memory + shadow deltas |
| `MemoryDelta` | `fission-ttd` | Before/after memory diff for one address range |
| `ShadowDelta` | `fission-ttd` | Before/after taint AST node diff for one byte position |
| `RegisterState` | `fission-ttd` | Full x86-64 GP register snapshot |

---

## Symbolic Execution and Taint Engine

Fission's taint and symbolic engine is a pure-Rust implementation — no Z3 bindings, no C++ FFI, no external SMT dependencies. It is designed for long-term maintainability and architecture-agnostic reasoning.

### Two-Layer Architecture

```
fission-solver            — Pure-Rust SMT/Constraint engine
  ├── ast.rs              — SymExpr: the symbolic expression AST
  └── solver.rs           — Solver: node registry, path conditions, SAT stub

fission-emulator          — Concrete + Symbolic execution
  └── pcode/
      ├── state.rs        — MachineState: shadow_memory (taint map)
      └── eval.rs         — Evaluator: taint propagation per P-Code opcode
```

### `SymExpr` AST Nodes

Every symbolic value is represented as a node in the `SymExpr` tree:

| Variant | Description |
|---|---|
| `Const { val, size }` | Concrete bitvector constant |
| `Var { id, name, size }` | Named symbolic variable (e.g. `stdin_0x4000`) |
| `Add(a, b)` / `Sub(a, b)` / `Mul(a, b)` | Integer arithmetic |
| `And(a, b)` / `Or(a, b)` / `Xor(a, b)` | Bitwise operations |
| `Shl(a, b)` / `Lshr(a, b)` | Shift operations |
| `Eq(a, b)` / `Neq(a, b)` / `Ult(a, b)` / `Ule(a, b)` | Comparisons (return 1-bit) |
| `Ite { cond, t, f }` | If-then-else |
| `Extract { expr, lsb, size }` | Bit extraction |
| `Concat(a, b)` | Bitvector concatenation |

### Shadow Memory (Taint State)

The `MachineState` holds a parallel "shadow" layer alongside the concrete memory:

```rust
// shadow_memory: (space_id, byte_address) -> SymNodeId
pub shadow_memory: HashMap<(u64, u64), u32>
```

- `space_id = 2` → CPU registers (Sleigh register address space)
- `space_id = 3` → RAM (Sleigh ram address space)

When a concrete byte is written, its shadow entry is cleared. When a symbolic value is written, its shadow entry is updated with the corresponding AST node ID.

### Taint Propagation Table

| P-Code Op | Taint Behavior |
|---|---|
| `COPY` | Propagate source shadow to destination |
| `LOAD` | Propagate shadow from RAM byte to output varnode |
| `STORE` | Propagate shadow from source varnode to RAM byte |
| `INT_ADD` | If either input is tainted, build `SymExpr::Add(a, b)` and store new node |
| `INT_SUB` | Similar: build `SymExpr::Sub` |
| Other ops | Currently concrete-only (no taint propagation yet) |

### Taint Sources

The primary taint source is `stdin`. In `os/linux/mod.rs`, the `sys_read(fd=0, ...)` handler:
1. Reads bytes from `stdin_buffer` (the `--stdin` mock).
2. Writes them into RAM as concrete bytes.
3. For each byte, calls `solver.register_var("stdin_<addr>", 1)` to create a `SymExpr::Var`.
4. Tags the corresponding `shadow_memory` entries with the new node ID.

From that point forward, any P-Code operation that reads those bytes will propagate the taint forward into new `SymExpr` constraint trees.

---

## Concolic Path Exploration

Concolic (concrete + symbolic) execution combines real execution with symbolic state to explore multiple code paths automatically.

### How It Works

1. The emulator runs normally in **concrete mode**, recording TTD snapshots along the way.
2. Every `CBranch` (conditional branch) P-Code instruction emits a `SymBranch` event containing:
   - The TTD step index at the branch point
   - The current PC value
   - Whether the branch was taken or not
   - The alternate target (address or relative P-Code index)
3. After the current path terminates, the `SymbolicExecutor` pops unexplored branches from a queue.
4. It rewinds the emulator to the snapshot closest to the branch step via `ttd_seek()`.
5. It forces the PC (or P-Code index) to the **alternate** target and resumes execution.
6. This continues until the exploration queue is empty.

```
Path 1:  [A] → [B] → [D] → halt        (branch at B taken = true)
Rewind to B
Path 2:  [A] → [B] → [C] → [E] → halt  (branch at B taken = false)
```

### `SymbolicExecutor`

The `SymbolicExecutor` (`sym/mod.rs`) is the exploration driver:

```rust
pub struct SymbolicExecutor {
    pub emu: Emulator,
    pub queue: Vec<SymBranch>,  // unexplored branch events
}
```

It calls `emu.run()` in a loop, drains `emu.sym_events` into the queue, and rewinds to the next unexplored branch. Each new execution path may reveal additional branches, which are added to the queue.

### Future: Full Symbolic Mode

The current implementation is **concolic scaffolding**: it explores paths by forcing PC values, but does not yet invert branch conditions symbolically via the `Solver`. The planned evolution:

1. When a `CBranch` is encountered and a taint variable is used in the branch condition, add the negated constraint `!condition` to `solver.assertions`.
2. Call `solver.check_sat()` to verify the alternate path is feasible.
3. Use `solver.get_value(var_id)` to obtain a concrete input that triggers the alternate path.
4. Replay with that concrete input instead of forcing PC.

This requires implementing the DPLL/CDCL bit-blasting core inside `fission-solver`, which is the next development milestone.

