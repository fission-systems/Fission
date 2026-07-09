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
- [x] CallOther flush **+ reload** (HLE cannot be clobbered by stale SSA at TB exit)
- [x] Persistent register cache (`MachineState::reg_cache` for 8B-aligned register space)
- [x] Zero-callout host reg file loads (`host_reg_file` + `jit_host_reg_base`)
- [x] Zero-callout host reg file stores (IR `store` to `host_reg_file` in `store_vn`)
- [x] Mid-TB register access zero-callout (IR load/store to `host_reg_file`);
      CallOther/TB-exit still one writeback per dirty unique/ram (and reg for AddressSpace)
- [x] Optional pure-Rust softfloat path (`feature = "softfloat"`, NaN quieting policy)

### Phase D — Analysis features

- [x] TTD: enable `tracing_memory` on `with_ttd`, clear deltas after record, disable chain while recording
- [x] TTD: recompute remaining steps after nearest-snapshot restore (`ttd_seek`)
- [x] Symbolic CBranch gate (`jit_sym_cbranch_gate` → `sym_events` + `sym_stop_requested`)
- [x] JIT shadow prop: COPY/LOAD/STORE + int ALU/compare
- [x] Full symbolic AST on JIT ALU (`jit_shadow_binop` builds `SymExpr` Add/Sub/…/Eq)
- [x] Unary/float AST: `jit_shadow_unop` + solver IEEE float theory (`FAdd`/`FNeg`/… with concrete fold)
- [x] Float theory SAT bit-blast: AIG lowering for `FNeg`/`FAbs`/`FIsNan`/cmp + free-bit `FAdd`/`FMul`/…
- [x] CallOther register dirty → one `jit_reg_bulk_flush` callout (not N× write_space)
- [x] TB-exit register writeback also uses `jit_reg_bulk_flush` (unique/ram still per-slot)
- [x] Exploration manager clears stop flag between forks (`sym/manager.rs`)

### Phase E — Maturity / smoke (in progress)

- [x] `EmulatorMetrics` (unimplemented ops, syscalls, TB/chain counters)
- [x] JIT: Piece / Extract / Insert / LzCount / SegmentOp
- [x] Optional E2E smoke (`tests/smoke_linux_hello.rs`, opt-in `FISSION_SMOKE_ELF` only)
- [x] Direct CALL address fix + x86 userop fallback (syscall)
- [x] CallOther mid-TB dirty register flush (HLE sees current SSA)
- [x] Checked-in tiny ELF fixture: `testdata/linux_x64_hello_sys.elf` + `smoke_ci_fixture_hello_sys`
- [x] PE ExitProcess fixture: `testdata/win_x64_exit.exe` + `smoke_pe_exit_process`
- [x] PE WriteFile fixture: `testdata/win_x64_write.exe` + `smoke_pe_write_file`
- [x] Unimplemented-opcode budget gate (`EmulatorMetrics::check_unimplemented_budget`)
- [x] IAT table + GetProcAddress dynamic trampolines + CRT bootstrap stubs
- [x] CLI sandbox: `--json` / `--metrics-out` / `--max-unimpl-*` / `--fail-on-budget`
- [x] Dynamic-linked ELF GOT/`iat_symbols` from JUMP_SLOT/GLOB_DAT (`fission-loader`)
- [x] Dyn ELF run without ld.so: `__libc_start_main` JumpTo(main) + puts HLE + GOT patch
- [x] Dynlink scaffold: PT_INTERP parse, `DynlinkMode::{Static,HleGot,Interpreter}`,
      opt-in host ld.so map (`FISSION_ENABLE_DYNLINK` + `FISSION_LD_SO`)
- [x] Dynlink bootstrap: VFS seed/alias for main binary, openat/read, file-backed mmap,
      `apply_rela_x86_64` (RELATIVE/JUMP_SLOT/GLOB_DAT)
- [x] DT_NEEDED shared-lib load loop + BIND_NOW (`DynlinkMode::SharedLibs`, `FISSION_LIB_PATH`)
- [x] Lazy PLT bind (`FISSION_LAZY_BIND=1`): GOT markers → first-call bind → JumpTo / nested HLE
- [x] ELF load order: map sections → `prepare_dynlink`/RELA → stack (RELA no longer wiped)
- [x] GOT patch preserves mini-dynlink resolved targets (`is_resolved_got_target`)
- [x] HLE quality metrics: `hle_misses` / `unknown_syscalls` + `check_hle_budget` / CLI `--max-hle-misses`
- [x] Bump heap HLE: `malloc`/`calloc` via `brk`; `printf` `%s/%d/%x/%p/%%` (+ `l`)
- [x] libc HLE: `strlen` / `memcpy` / `memmove` / `memset` / `mmap`
- [x] CI fixtures: `x64_dyn_printf_malloc.elf` (HLE path); static printf **opt-in only**
      (remeasured: not CI-stable / no clean halt ≤500k — CRT/syscall gaps remain)
- [x] Concolic mini-fixture: `x64_concolic_branch_sys.elf` (stdin taint → branch);
      gate records always, stops only when `concolic_stop_on_branch`
- [x] Concolic explore E2E: `SimulationManager` fork resume PCs + `max_steps` + both-exit smoke
- [x] libc HLE: `strcmp`/`strncmp`/`snprintf`/`stat` (+ `__xstat`); `newfstatat` VFS sizes
- [x] TLS path: `fs_base`/`gs_base`, `arch_prctl` SET/GET FS/GS, `set_tid_address`,
      `segment`/`segment_fs` CallOther result → JIT output
- [x] `max_inst` honored inside chain (no chain when budgeted); seal op-blocks after CFG edges
- [x] path-SAT prune re-enabled with OOB-safe `value_lit` + panic isolation
- [x] path-SAT quality tests: Const false/true short-circuit; free vars both SAT
- [x] static CRT profile ladder (`512`/`1500`): early surface = arch_prctl, set_tid,
      brk, mmap — **zero unknown syscalls**; full halt still open
- [x] `max_inst` + `pcode_budget` fuses (TB exit on fuse; no false process halt)
- [x] Automation `sandbox-check` lane (subprocess over CLI JSON + budget gate)

## Validation

```bash
cargo check -p fission-emulator
cargo nextest run -p fission-emulator
# optional large musl binary (explicit path only — no /tmp auto-discovery):
#   zig cc -target x86_64-linux-musl -O0 -o /tmp/fission-emu-test/hello_linux_x64 hello.c
#   FISSION_SMOKE_ELF=/tmp/fission-emu-test/hello_linux_x64 cargo nextest run -p fission-emulator smoke_optional
cargo check -p fission-cli
./target/release/fission_cli sandbox crates/fission-emulator/testdata/linux_x64_hello_sys.elf \
  --max-inst 64 --json --fail-on-budget --max-unimpl-events 0 --max-unimpl-kinds 0
```

Future: differential execution against a **separate** offline oracle harness is allowed for CI measurement only — never linked into `fission-emulator`.

## Anti-patterns

- Restoring a P-Code interpreter as execution engine
- Linking or shelling out to `vendor/qemu-*`
- Binary/address-specific JIT patches
- Fixing decompiler output inside the emulator
