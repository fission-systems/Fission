# Fission Emulator Roadmap (JIT-Only, QEMU Cleanroom Reference)

**Status:** active  
**Policy:** Cranelift JIT is the **only** execution engine. No interpreter fallback.  
**Reference:** `vendor/qemu-11.0.2` is structural/algorithmic reference only — **no link, no bind, no copy, no runtime dependency**.

## Hard constraints

1. **JIT only** — guest instructions are Sleigh-lifted to P-Code, then Cranelift-compiled to host code.
2. **No QEMU/Unicorn/Capstone/Z3** dependencies in production paths.
3. **Not a decompiler repair layer** — semantic bugs belong in `fission-pcode` / `fission-sleigh`.
4. **Symbolic / TTD / HLE** stay pure Rust (`fission-solver`, `fission-ttd`, OS HLE modules).

## QEMU → Fission mapping (cleanroom)

| QEMU concept (reference) | Fission owner | Notes |
|---|---|---|
| TranslationBlock + TB cache | `jit/cache.rs` `JitCache` | Guest PC → host func; page→blocks for invalidation |
| TCG opcode lowering | `jit/compiler.rs` | P-Code → Cranelift IR (not TCG ops) |
| SoftMMU / page flags | `pcode/page_map.rs` | PAGE_READ/WRITE/EXEC-shaped flags, no QEMU code |
| linux-user `mmap` / `brk` | `os/linux/syscall.rs` + `PageMap` | Real region allocation, not fixed stubs |
| TB invalidate on code write | `jit_write_space` SMC path | Exec-page write → `invalidate_page` |
| cpu_loop / helpers | `core.rs` run loop + JIT callbacks | HLE traps, CallOther |
| linux-user syscalls | `os/linux/*` SimProcedure registry | Expand coverage incrementally |
| signal queue / delivery | `os/linux/signal.rs` + run-loop | Pending/blocked/actions; between-TB delivery |
| PE process image | `os/windows/image_info.rs` | Symmetric to ELF ImageInfo |

## Current architecture

```
guest PC
  → JitCache lookup
  → (miss) Sleigh decode/lift → Cranelift compile → publish hard-chain → run
  → host fn(*mut Emulator) -> next_pc  (hard-chain absolute/fallthrough via chain_table)
  → process_pending_signals()
  → HLE magic-address trap check
  → optional TTD snapshot
```

Callouts: `jit_read_space` / `jit_write_space` / `jit_call_other` / `jit_exit_tb` / `jit_hle_trap`.

## Phased plan

### Phase A — Correctness substrate

- [x] JIT-only policy enforced in `run_instruction` (compile fail is hard error)
- [x] Correct LOAD/STORE lowering (space + pointer)
- [x] Intra-instruction relative BRANCH/CBRANCH via per-op Cranelift blocks
- [x] CallOther → `jit_call_other` (syscall/sysenter → HLE)
- [x] Guest `PageMap` + section/stack mapping + real `mmap`/`brk`
- [x] SMC invalidation on writes to EXEC pages
- [x] Float ops via host callouts (`jit_float_binop` / `jit_float_unop`)
- [x] >8B varnodes via bulk `jit_read_bytes` / `jit_write_bytes`
- [x] SLA-native space-id resolution (`SpaceLayout` from compiled frontend)
- [x] Soft direct TB chaining (`jit_chain`, depth-bounded)

### Phase B — User-mode depth (QEMU linux-user inspired)

- [x] Expand x86-64 Linux syscalls (openat, writev, uname, arch_prctl, mmap/mprotect/munmap, clock_gettime, getrandom, futex stub, …)
- [x] ELF load `ImageInfo` (argc/argv/envp/auxv, stack, brk base)
- [x] PageFault enforcement on RAM (`enforce_page_faults` + `PageMap` R/W checks)
- [x] Windows HLE growth (VirtualAlloc maps pages, file/console, TLS, GetLastError, codepage, …)
- [x] Linux signal delivery (`SignalState`, kill/tkill/rt_sigaction/procmask/sigreturn; between-TB)
- [x] PE `PeImageInfo` (sections/prot, stack, PEB/TEB, heap, entry/SP) — ELF-symmetric

### Phase C — Performance (QEMU TCG-inspired)

- [x] Multi-instruction TBs (up to 8 insns; stop on absolute branch / page / cached edge)
- [x] Soft direct block chaining (`jit_chain`)
- [x] Hard chaining via **global** guest-PC → host-fn table (`jit_exit_tb`) — fallthrough **and absolute** branch/call
- [ ] Hot-path register caching with fewer callouts
- [ ] Optional pure-Rust softfloat for IEEE edge cases

### Phase D — Analysis features

- [ ] TTD snapshot & recompute over JIT segments
- [ ] Symbolic gate: drop to solver when shadow memory is live (still no interpreter for concrete ops)
- [ ] Exploration strategies (already seeded under `sym/`)

### Phase E — Maturity / smoke (in progress)

- [x] `EmulatorMetrics` (unimplemented ops, syscalls, TB/chain counters)
- [x] JIT: Piece / Extract / Insert / LzCount / SegmentOp
- [x] Optional E2E smoke (`tests/smoke_linux_hello.rs`, `FISSION_SMOKE_ELF`)
- [x] Direct CALL address fix + x86 userop fallback (syscall)
- [ ] Checked-in tiny ELF fixture for CI (no zig required)
- [ ] Unimplemented-opcode budget gate in automation
- [ ] Dynamic-linked ELF / PE CRT smoke

## Validation

```bash
cargo check -p fission-emulator
cargo nextest run -p fission-emulator
# optional real binary:
#   zig cc -target x86_64-linux-musl -O0 -o /tmp/fission-emu-test/hello_linux_x64 hello.c
#   FISSION_SMOKE_ELF=/tmp/fission-emu-test/hello_linux_x64 cargo nextest run -p fission-emulator smoke_linux
cargo check -p fission-cli
./target/release/fission_cli sandbox /path/to/elf --max-inst 50000
```

Future: differential execution against a **separate** offline oracle harness is allowed for CI measurement only — never linked into `fission-emulator`.

## Anti-patterns

- Restoring a P-Code interpreter as execution engine
- Linking or shelling out to `vendor/qemu-*`
- Binary/address-specific JIT patches
- Fixing decompiler output inside the emulator
